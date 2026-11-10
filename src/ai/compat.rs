//! ChatGPT, DeepSeek, Grok, OpenRouter and local models, through their
//! OpenAI-compatible chat completions APIs (streaming, with the same
//! type_in_terminal tool as a function). Models that can't use tools still
//! answer, without it. "Automatic" picks the newest general chat model the
//! key can use (as 3.x).

use super::{Endpoint, Event, Link, Question, SYSTEM, TOOL_DESCRIPTION, TOOL_NAME};
use regex::Regex;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::LazyLock;

static NOT_CHAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(embed|image|dall|tts|whisper|audio|realtime|transcri|speech|moderation|search|live|sora|vision-preview|instruct|davinci|babbage)").unwrap()
});
static O_SERIES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^o\d").unwrap());
static VERSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)").unwrap());
static DATED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}|\d{4}$").unwrap());

/// Models that can chat (not images, audio, embeddings…).
pub fn is_chat_model(provider: &str, id: &str) -> bool {
    if NOT_CHAT.is_match(id) {
        return false;
    }
    match provider {
        "openai" => id.starts_with("gpt-") || id.starts_with("chatgpt-") || O_SERIES.is_match(id),
        "xai" => id.starts_with("grok-"),
        _ => true,
    }
}

/// Newer, general-purpose models rank higher.
pub fn rank(id: &str) -> f64 {
    let v: f64 = VERSION
        .find(id)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0.0);
    let l = id.to_lowercase();
    let mut score = v * 100.0;
    if ["mini", "nano", "luna", "lite", "flash"]
        .iter()
        .any(|s| l.contains(s))
    {
        score -= 30.0;
    }
    if l.contains("pro") {
        score -= 10.0; // slow and pricey for terminal help
    }
    if DATED.is_match(&l) {
        score -= 5.0; // dated snapshots after the moving alias
    }
    score
}

/// The chat models a provider offers this key, best first (OpenRouter and
/// local servers keep their own order).
pub fn list_models(ep: &Endpoint) -> Result<Vec<String>, String> {
    let resp = super::agent()
        .get(format!("{}/models", ep.base))
        .header("authorization", &format!("Bearer {}", ep.key))
        .call()
        .map_err(|e| {
            format!(
                "Couldn't reach {} ({e}).",
                super::provider(ep.provider).name
            )
        })?;
    let status = resp.status().as_u16();
    let text = resp.into_body().read_to_string().unwrap_or_default();
    if status != 200 {
        return Err(super::http_error(ep.provider, status, &text));
    }
    let v: Value = serde_json::from_str(&text).unwrap_or_default();
    let mut out: Vec<String> = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| is_chat_model(ep.provider, id))
        .map(str::to_string)
        .collect();
    if ep.provider != "openrouter" && ep.provider != "local" {
        out.sort_by(|a, b| rank(b).total_cmp(&rank(a)));
    }
    Ok(out)
}

/// The chosen model, or for "automatic" the best one this key can use.
pub fn resolve_model(ep: &Endpoint) -> Result<String, String> {
    if !ep.model.is_empty() {
        return Ok(ep.model.clone());
    }
    match ep.provider {
        "deepseek" => return Ok("deepseek-v4-flash".into()),
        "openrouter" => return Ok("openrouter/auto".into()),
        _ => {}
    }
    list_models(ep)?.into_iter().next().ok_or_else(|| {
        format!(
            "{} offered no chat models; pick one in Settings → AI.",
            super::provider(ep.provider).name
        )
    })
}

pub fn body(model: &str, messages: &[Value], tools: bool) -> Value {
    let mut all = vec![json!({"role": "system", "content": SYSTEM})];
    all.extend(messages.iter().cloned());
    let mut b = json!({"model": model, "stream": true, "messages": all});
    if tools {
        b["tools"] = json!([{
            "type": "function",
            "function": {"name": TOOL_NAME, "description": TOOL_DESCRIPTION, "parameters": super::tool_schema()}
        }]);
    }
    b
}

#[derive(Default, Debug)]
pub struct Reply {
    pub text: String,
    /// (id, name, arguments) by index.
    pub calls: BTreeMap<u64, (String, String, String)>,
    pub finish: String,
    pub model: String,
    pub error: Option<String>,
}

pub fn read_stream(r: impl std::io::BufRead, link: Option<&Link>) -> Reply {
    let mut reply = Reply::default();
    let res = super::sse::read(r, |_, data| {
        if data == "[DONE]" || link.is_some_and(|l| l.cancelled()) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return true;
        };
        if let Some(e) = v["error"]["message"].as_str() {
            reply.error = Some(e.to_string());
            return false;
        }
        if reply.model.is_empty()
            && let Some(m) = v["model"].as_str()
        {
            reply.model = m.to_string();
            if let Some(l) = link {
                l.send(Event::Model(reply.model.clone()));
            }
        }
        let c = &v["choices"][0];
        if let Some(t) = c["delta"]["content"].as_str() {
            reply.text.push_str(t);
            if let Some(l) = link {
                l.send(Event::Text(t.to_string()));
            }
        }
        for call in c["delta"]["tool_calls"].as_array().into_iter().flatten() {
            let e = reply
                .calls
                .entry(call["index"].as_u64().unwrap_or(0))
                .or_default();
            if let Some(id) = call["id"].as_str() {
                e.0 = id.to_string();
            }
            if let Some(n) = call["function"]["name"].as_str() {
                e.1.push_str(n);
            }
            if let Some(a) = call["function"]["arguments"].as_str() {
                e.2.push_str(a);
            }
        }
        if let Some(f) = c["finish_reason"].as_str() {
            reply.finish = f.to_string();
        }
        true
    });
    if let Err(e) = res
        && reply.error.is_none()
    {
        reply.error = Some(format!("the connection broke off ({e})"));
    }
    reply
}

pub fn ask(
    ep: &Endpoint,
    history: &mut Vec<Value>,
    q: &Question,
    link: &Link,
) -> Result<(), String> {
    let name = super::provider(ep.provider).name;
    let model = resolve_model(ep)?;
    let mut messages = history.clone();
    messages.push(json!({"role": "user", "content": super::user_text(q)}));
    let agent = super::agent();
    let mut tools = true;
    for _turn in 0..12 {
        let resp = agent
            .post(format!("{}/chat/completions", ep.base))
            .header("authorization", &format!("Bearer {}", ep.key))
            .header("content-type", "application/json")
            .header("x-title", "jamalterm")
            .send_json(body(&model, &messages, tools))
            .map_err(|e| format!("Couldn't reach {name} ({e})."))?;
        let status = resp.status().as_u16();
        let mut b = resp.into_body();
        if status != 200 {
            let text = b.read_to_string().unwrap_or_default();
            // A model that can't use tools: ask again without the tool.
            if tools && status == 400 && text.to_lowercase().contains("tool") {
                tools = false;
                continue;
            }
            return Err(super::http_error(ep.provider, status, &text));
        }
        let reply = read_stream(std::io::BufReader::new(b.as_reader()), Some(link));
        if link.cancelled() {
            return Ok(());
        }
        if let Some(e) = reply.error {
            return Err(format!("{name} stopped: {e}"));
        }
        let mut msg = json!({"role": "assistant", "content": reply.text});
        if !reply.calls.is_empty() {
            msg["tool_calls"] = reply
                .calls
                .values()
                .map(|(id, n, a)| json!({"id": id, "type": "function", "function": {"name": n, "arguments": a}}))
                .collect();
        }
        messages.push(msg);
        if reply.calls.is_empty() {
            *history = messages;
            return Ok(());
        }
        for (id, n, args) in reply.calls.values() {
            let (text, _is_error) = if n != TOOL_NAME {
                ("unknown tool".to_string(), true)
            } else {
                match super::parse_tool_input(args) {
                    Some((cmd, run)) if reply.finish != "length" => {
                        let r = link.tool(cmd, run);
                        (r.text, r.is_error)
                    }
                    _ => (super::invalid_input(args), true),
                }
            };
            messages.push(json!({"role": "tool", "tool_call_id": id, "content": text}));
        }
    }
    *history = messages;
    Err(format!(
        "{name} kept calling the tool; stopped after 12 rounds."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::{mock, run};
    use crate::ai::{History, Reply as ToolReply};
    use std::sync::{Arc, Mutex};

    fn chunks(parts: &[Value]) -> String {
        let mut s: String = parts.iter().map(|p| format!("data: {p}\n\n")).collect();
        s.push_str("data: [DONE]\n\n");
        s
    }

    fn ep(base: &str, provider: &'static str, model: &str) -> Endpoint {
        Endpoint {
            provider,
            base: base.into(),
            key: "k".into(),
            model: model.into(),
        }
    }

    #[test]
    fn automatic_model_choice() {
        assert!(is_chat_model("openai", "gpt-5.2") && is_chat_model("openai", "o4"));
        assert!(
            !is_chat_model("openai", "gpt-image-1") && !is_chat_model("openai", "text-embedding-3")
        );
        assert!(!is_chat_model("xai", "aurora"));
        let m = mock(vec![(
            200,
            r#"{"data":[{"id":"gpt-4.1"},{"id":"gpt-5.2-mini"},{"id":"gpt-5.2"},{"id":"gpt-5.2-2026-01-01"},{"id":"tts-1"}]}"#.into(),
        )]);
        assert_eq!(
            resolve_model(&ep(&m.base, "openai", "")).unwrap(),
            "gpt-5.2"
        );
        assert_eq!(
            resolve_model(&ep(&m.base, "deepseek", "")).unwrap(),
            "deepseek-v4-flash"
        );
        assert_eq!(
            resolve_model(&ep(&m.base, "xai", "grok-9")).unwrap(),
            "grok-9",
            "chosen in Settings"
        );
    }

    #[test]
    fn streaming_text_and_a_tool_call() {
        let m = mock(vec![
            (
                200,
                chunks(&[
                    json!({"model":"gpt-5.2","choices":[{"delta":{"content":"Typing it."}}]}),
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"type_in_terminal","arguments":"{\"command\":\"ls\","}}]}}]}),
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"run\":false}"}}]},"finish_reason":"tool_calls"}]}),
                ]),
            ),
            (
                200,
                chunks(&[
                    json!({"choices":[{"delta":{"content":"Done."},"finish_reason":"stop"}]}),
                ]),
            ),
        ]);
        let ev = run(
            ep(&m.base, "openai", "gpt-5.2"),
            Arc::new(Mutex::new(History::default())),
            Question {
                prompt: "type ls".into(),
                ..Default::default()
            },
            |c, r| {
                assert_eq!((c, r), ("ls", false));
                ToolReply {
                    text: "typed".into(),
                    is_error: false,
                }
            },
        );
        assert!(ev.contains(&Event::Text("Typing it.".into())));
        assert_eq!(ev.last(), Some(&Event::Done));
        let b = m.bodies.lock().unwrap()[1].clone();
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(
            msgs[2]["tool_calls"][0]["function"]["name"],
            "type_in_terminal"
        );
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(msgs[3]["tool_call_id"], "call_1");
        assert_eq!(m.paths.lock().unwrap()[0], "/chat/completions");
    }

    #[test]
    fn a_model_without_tools_still_answers() {
        let m = mock(vec![
            (
                400,
                r#"{"error":{"message":"this model does not support tools"}}"#.into(),
            ),
            (
                200,
                chunks(&[json!({"choices":[{"delta":{"content":"Hi."},"finish_reason":"stop"}]})]),
            ),
        ]);
        let ev = run(
            ep(&m.base, "local", "llama"),
            Arc::new(Mutex::new(History::default())),
            Question {
                prompt: "hi".into(),
                ..Default::default()
            },
            |_, _| unreachable!(),
        );
        assert_eq!(ev.last(), Some(&Event::Done));
        let bodies = m.bodies.lock().unwrap();
        assert!(bodies[0].get("tools").is_some() && bodies[1].get("tools").is_none());
    }
}
