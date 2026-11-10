//! Claude, through Anthropic's Messages API (plain HTTP: there's no official
//! Rust SDK). Streaming, adaptive thinking (Claude Opus 5.5 always thinks;
//! effort is set explicitly), server-side fallbacks when a request is
//! declined, and the type_in_terminal tool with its input streamed as it's
//! written (so it's checked here before anything is done with it).

use super::{Endpoint, Event, Link, Question, SYSTEM, TOOL_DESCRIPTION, TOOL_NAME};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// The request body for a conversation.
#[cfg(test)]
pub fn body(model: &str, messages: &[Value], tools: bool) -> Value {
    body_for(model, messages, tools, false)
}

/// The command bar's request: its own instructions, low effort, no tool.
pub fn body_for(model: &str, messages: &[Value], tools: bool, command: bool) -> Value {
    let mut b = json!({
        "model": model,
        "max_tokens": 64000,
        "stream": true,
        "system": if command { super::COMMAND_SYSTEM } else { SYSTEM },
        "messages": messages,
        // Terminal help: medium effort (also Claude Opus 5.5's default); a
        // single command: low.
        "output_config": {"effort": if command { "low" } else { "medium" }},
    });
    if tools {
        b["tools"] = json!([{
            "name": TOOL_NAME,
            "description": TOOL_DESCRIPTION,
            "input_schema": super::tool_schema(),
            "eager_input_streaming": true,
        }]);
    }
    // When a request is declined, the API re-runs it on a fallback model
    // (Haiku has none).
    if !model.contains("haiku") {
        b["fallbacks"] = json!("default");
    }
    b
}

/// One streamed reply, put back together.
#[derive(Default, Debug)]
pub struct Reply {
    pub model: String,
    pub stop_reason: String,
    /// Content blocks in order, as the API sent them (thinking blocks with
    /// their signatures, so they go back unchanged).
    pub blocks: Vec<Value>,
    /// tool_use inputs as they streamed (by block index).
    pub inputs: BTreeMap<usize, String>,
    pub error: Option<String>,
}

/// Reads the event stream into a Reply, passing text on as it arrives.
pub fn read_stream(r: impl std::io::BufRead, link: Option<&Link>) -> Reply {
    let mut reply = Reply::default();
    let res = super::sse::read(r, |event, data| {
        if link.is_some_and(|l| l.cancelled()) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return true;
        };
        let index = v["index"].as_u64().unwrap_or(0) as usize;
        match v["type"].as_str().unwrap_or(event) {
            "message_start" => {
                reply.model = v["message"]["model"].as_str().unwrap_or("").to_string();
                if let Some(l) = link {
                    l.send(Event::Model(reply.model.clone()));
                }
            }
            "content_block_start" => {
                while reply.blocks.len() <= index {
                    reply.blocks.push(Value::Null);
                }
                reply.blocks[index] = v["content_block"].clone();
            }
            "content_block_delta" => {
                let d = &v["delta"];
                let Some(b) = reply.blocks.get_mut(index) else {
                    return true;
                };
                match d["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        let t = d["text"].as_str().unwrap_or("");
                        let now = b["text"].as_str().unwrap_or("").to_string() + t;
                        b["text"] = json!(now);
                        if let Some(l) = link {
                            l.send(Event::Text(t.to_string()));
                        }
                    }
                    "input_json_delta" => {
                        reply
                            .inputs
                            .entry(index)
                            .or_default()
                            .push_str(d["partial_json"].as_str().unwrap_or(""));
                    }
                    "thinking_delta" => {
                        let now = b["thinking"].as_str().unwrap_or("").to_string()
                            + d["thinking"].as_str().unwrap_or("");
                        b["thinking"] = json!(now);
                    }
                    "signature_delta" => {
                        let now = b["signature"].as_str().unwrap_or("").to_string()
                            + d["signature"].as_str().unwrap_or("");
                        b["signature"] = json!(now);
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(s) = v["delta"]["stop_reason"].as_str() {
                    reply.stop_reason = s.to_string();
                }
            }
            "error" => {
                reply.error = Some(
                    v["error"]["message"]
                        .as_str()
                        .unwrap_or("the stream broke off")
                        .to_string(),
                );
                return false;
            }
            _ => {}
        }
        true
    });
    if let Err(e) = res
        && reply.error.is_none()
    {
        reply.error = Some(format!("the connection broke off ({e})"));
    }
    // Tool inputs as parsed objects where they parse (others stay raw and
    // are answered as invalid).
    for (i, raw) in &reply.inputs {
        if let Some(b) = reply.blocks.get_mut(*i) {
            b["input"] = serde_json::from_str(raw).unwrap_or(json!({}));
        }
    }
    reply.blocks.retain(|b| !b.is_null());
    reply
}

/// The reply's blocks as they're echoed back in the history. After a
/// fallback partway through, thinking and tool_use blocks before the last
/// `fallback` block are left out (the fallback block stays where it was).
pub fn echo(blocks: &[Value]) -> Vec<Value> {
    let last_fallback = blocks.iter().rposition(|b| b["type"] == "fallback");
    blocks
        .iter()
        .enumerate()
        .filter(|(i, b)| match last_fallback {
            Some(f) if *i < f => matches!(b["type"].as_str(), Some("text") | Some("fallback")),
            _ => true,
        })
        .map(|(_, b)| b.clone())
        .collect()
}

pub fn ask(
    ep: &Endpoint,
    history: &mut Vec<Value>,
    q: &Question,
    link: &Link,
) -> Result<(), String> {
    let model = if ep.model.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        ep.model.clone()
    };
    let mut messages = history.clone();
    messages
        .push(json!({"role": "user", "content": [{"type": "text", "text": super::user_text(q)}]}));
    let agent = super::agent();
    // Signed in with `ant auth login`: a short-lived token instead of a key.
    let token = if ep.key == super::SIGNED_IN {
        Some(super::anthropic_token()?)
    } else {
        None
    };
    for _turn in 0..12 {
        let req = agent
            .post(format!("{}/v1/messages", ep.base))
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        let req = match &token {
            Some(t) => req.header("authorization", &format!("Bearer {t}")).header(
                "anthropic-beta",
                &format!("{FALLBACK_BETA},oauth-2025-04-20"),
            ),
            None => req
                .header("x-api-key", &ep.key)
                .header("anthropic-beta", FALLBACK_BETA),
        };
        let resp = req
            .send_json(body_for(&model, &messages, !q.command, q.command))
            .map_err(|e| format!("Couldn't reach Anthropic ({e})."))?;
        let status = resp.status().as_u16();
        let mut body = resp.into_body();
        if status != 200 {
            let text = body.read_to_string().unwrap_or_default();
            return Err(super::http_error(ep.provider, status, &text));
        }
        let reply = read_stream(std::io::BufReader::new(body.as_reader()), Some(link));
        if link.cancelled() {
            return Ok(());
        }
        if let Some(e) = reply.error {
            return Err(format!("Claude stopped: {e}"));
        }
        if reply.stop_reason == "refusal" {
            // Declined (even after the fallback): the partial answer is
            // dropped, and the history stays as it was.
            return Err("Claude declined to answer this. Try asking another way.".into());
        }
        let content = echo(&reply.blocks);
        messages.push(json!({"role": "assistant", "content": content}));
        if reply.stop_reason != "tool_use" {
            *history = messages;
            return Ok(());
        }
        // Do each tool call (after any fallback), answering all at once.
        let mut results = vec![];
        for b in content.iter().filter(|b| b["type"] == "tool_use") {
            let id = b["id"].as_str().unwrap_or("");
            let index = reply.blocks.iter().position(|x| x["id"] == b["id"]);
            let raw = index
                .and_then(|i| reply.inputs.get(&i).cloned())
                .unwrap_or_else(|| b["input"].to_string());
            let (text, is_error) = if b["name"] != TOOL_NAME {
                ("unknown tool".to_string(), true)
            } else {
                match super::parse_tool_input(&raw) {
                    Some((cmd, run)) if reply.stop_reason != "max_tokens" => {
                        let r = link.tool(cmd, run);
                        (r.text, r.is_error)
                    }
                    _ => (super::invalid_input(&raw), true),
                }
            };
            results.push(json!({"type": "tool_result", "tool_use_id": id, "content": text, "is_error": is_error}));
        }
        messages.push(json!({"role": "user", "content": results}));
    }
    *history = messages;
    Err("Claude kept calling the tool; stopped after 12 rounds.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::{mock, run};
    use crate::ai::{History, Reply as ToolReply};
    use std::sync::{Arc, Mutex};

    fn sse(events: &[Value]) -> String {
        events
            .iter()
            .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().unwrap(), e))
            .collect()
    }

    fn text_reply(model: &str, text: &str) -> String {
        sse(&[
            json!({"type":"message_start","message":{"model":model}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig1"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":text}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
            json!({"type":"message_stop"}),
        ])
    }

    fn tool_reply(input_parts: &[&str]) -> String {
        let mut ev = vec![
            json!({"type":"message_start","message":{"model":"claude-opus-5-5"}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"type_in_terminal","input":{}}}),
        ];
        for p in input_parts {
            ev.push(json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":p}}));
        }
        ev.push(json!({"type":"content_block_stop","index":0}));
        ev.push(json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}));
        sse(&ev)
    }

    fn endpoint(base: &str) -> Endpoint {
        Endpoint {
            provider: "anthropic",
            base: base.into(),
            key: "sk-ant-test".into(),
            model: String::new(),
        }
    }

    #[test]
    fn the_request() {
        let b = body(DEFAULT_MODEL, &[], true);
        assert_eq!(b["model"], "claude-opus-5-5");
        assert_eq!(b["stream"], true);
        assert_eq!(b["output_config"]["effort"], "medium");
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["tools"][0]["eager_input_streaming"], true);
        assert!(b.get("thinking").is_none(), "Opus 5.5 always thinks");
        assert!(
            body("claude-haiku-5-5", &[], false)
                .get("fallbacks")
                .is_none()
        );
    }

    #[test]
    fn a_streamed_answer_and_its_history() {
        let m = mock(vec![
            (200, text_reply("claude-opus-5-5", "Use ls -la.")),
            (200, text_reply("claude-opus-5-5", "Again.")),
        ]);
        let h = Arc::new(Mutex::new(History::default()));
        let q = Question {
            prompt: "is my key sk-ant-api03-mine0123456789abcdef ok?".into(),
            context: "$ env\nOPENAI_API_KEY=sk-proj-AbCdEfGhIjKlMnOpQrStUv".into(),
            shell: "zsh".into(),
            ..Default::default()
        };
        let ev = run(
            endpoint(&m.base),
            h.clone(),
            q.clone(),
            |_, _| unreachable!(),
        );
        assert!(ev.contains(&Event::Model("claude-opus-5-5".into())));
        assert!(ev.contains(&Event::Hidden(1)));
        assert!(ev.contains(&Event::Text("Use ls -la.".into())));
        assert_eq!(ev.last(), Some(&Event::Done));
        let sent = m.bodies.lock().unwrap()[0].clone();
        let text = sent["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("OPENAI_API_KEY=[hidden secret]") && !text.contains("AbCdEf"));
        assert!(
            text.contains("sk-ant-api03-mine0123456789abcdef"),
            "the question as typed"
        );
        assert_eq!(m.paths.lock().unwrap()[0], "/v1/messages");
        // The next question carries the history, thinking signature included.
        run(endpoint(&m.base), h.clone(), q, |_, _| unreachable!());
        let sent = m.bodies.lock().unwrap()[1].clone();
        let msgs = sent["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["content"][0]["type"], "thinking");
        assert_eq!(msgs[1]["content"][0]["signature"], "sig1");
        assert_eq!(msgs[1]["content"][1]["text"], "Use ls -la.");
    }

    #[test]
    fn the_tool_loop() {
        let m = mock(vec![
            (
                200,
                tool_reply(&[r#"{"command": "ec"#, r#"ho hi", "run": true}"#]),
            ),
            (200, text_reply("claude-opus-5-5", "Ran it.")),
        ]);
        let h = Arc::new(Mutex::new(History::default()));
        let ev = run(
            endpoint(&m.base),
            h,
            Question {
                prompt: "run echo hi".into(),
                ..Default::default()
            },
            |cmd, run| {
                assert_eq!((cmd, run), ("echo hi", true));
                ToolReply {
                    text: "exit code 0\nhi".into(),
                    is_error: false,
                }
            },
        );
        assert!(ev.contains(&Event::Tool {
            command: "echo hi".into(),
            run: true
        }));
        assert_eq!(ev.last(), Some(&Event::Done));
        let second = m.bodies.lock().unwrap()[1].clone();
        let msgs = second["messages"].as_array().unwrap();
        assert_eq!(msgs[1]["content"][0]["input"]["command"], "echo hi");
        let result = &msgs[2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "toolu_1");
        assert_eq!(result["content"], "exit code 0\nhi");
    }

    #[test]
    fn a_cut_short_tool_input_is_not_run() {
        let m = mock(vec![
            (200, tool_reply(&[r#"{"command": "rm -rf"#])),
            (200, text_reply("claude-opus-5-5", "Sorry.")),
        ]);
        let ev = run(
            endpoint(&m.base),
            Arc::new(Mutex::new(History::default())),
            Question {
                prompt: "x".into(),
                ..Default::default()
            },
            |_, _| panic!("must not run"),
        );
        assert!(!ev.iter().any(|e| matches!(e, Event::Tool { .. })));
        let second = m.bodies.lock().unwrap()[1].clone();
        let result = &second["messages"][2]["content"][0];
        assert_eq!(result["is_error"], true);
        assert!(result["content"].as_str().unwrap().contains("INVALID_JSON"));
    }

    #[test]
    fn refusals_and_errors() {
        let refused = sse(&[
            json!({"type":"message_start","message":{"model":"claude-opus-5-5"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"refusal"}}),
        ]);
        let m = mock(vec![
            (200, refused),
            (401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#.into()),
        ]);
        let h = Arc::new(Mutex::new(History::default()));
        let q = Question {
            prompt: "x".into(),
            ..Default::default()
        };
        let ev = run(
            endpoint(&m.base),
            h.clone(),
            q.clone(),
            |_, _| unreachable!(),
        );
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("declined")));
        assert!(
            h.lock().unwrap().messages.is_empty(),
            "nothing kept from a refusal"
        );
        let ev = run(endpoint(&m.base), h, q, |_, _| unreachable!());
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("API key")));
    }

    #[test]
    fn echo_after_a_fallback() {
        let blocks = vec![
            json!({"type":"thinking","thinking":"","signature":"a"}),
            json!({"type":"text","text":"partial"}),
            json!({"type":"tool_use","id":"t0","name":"type_in_terminal","input":{}}),
            json!({"type":"fallback","from":{"model":"x"},"to":{"model":"y"}}),
            json!({"type":"thinking","thinking":"","signature":"b"}),
            json!({"type":"text","text":"rest"}),
        ];
        let echoed = echo(&blocks);
        let types: Vec<&str> = echoed.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["text", "fallback", "thinking", "text"]);
        assert_eq!(echo(&blocks[..2]).len(), 2, "no fallback: everything");
    }
}
