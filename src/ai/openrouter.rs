//! Signing in to OpenRouter in the browser: OpenRouter's public OAuth PKCE
//! flow, which needs no app registration. You approve in the browser and
//! OpenRouter gives back an API key for your account (3.x's flow).
//!
//! 1. open https://openrouter.ai/auth?callback_url=http://localhost:PORT/callback
//!    &code_challenge=…&code_challenge_method=S256
//! 2. OpenRouter sends the browser back to the callback with ?code=…
//! 3. POST https://openrouter.ai/api/v1/auth/keys {code, code_verifier,
//!    code_challenge_method} → {"key": "…"}

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const AUTH_URL: &str = "https://openrouter.ai/auth";
const KEYS_URL: &str = "https://openrouter.ai/api/v1/auth/keys";

/// A PKCE verifier and its S256 challenge.
pub fn pkce() -> (String, String) {
    let mut raw = [0u8; 32];
    let _ = getrandom::fill(&mut raw);
    let verifier = URL_SAFE_NO_PAD.encode(raw);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The code in a callback request line ("GET /callback?code=abc HTTP/1.1").
pub fn code_from(request_line: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.strip_prefix("/callback?")?;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("code="))
        .map(str::to_string)
        .filter(|c| !c.is_empty())
}

fn page(title: &str, text: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><body style=\"font:16px -apple-system,sans-serif;margin:4em\"><h2>{title}</h2><p>{text}</p>"
    )
}

/// Runs the browser sign-in (blocking: call it on a background thread) and
/// returns the new key. `open` opens a web page in the browser.
pub fn sign_in(open: impl Fn(&str)) -> Result<String, String> {
    sign_in_with(open, AUTH_URL, KEYS_URL, Duration::from_secs(300))
}

pub fn sign_in_with(
    open: impl Fn(&str),
    auth: &str,
    keys: &str,
    limit: Duration,
) -> Result<String, String> {
    let (verifier, challenge) = pkce();
    let l = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("couldn't start the sign-in listener: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let callback = format!("http://localhost:{port}/callback");
    open(&format!(
        "{auth}?callback_url={}&code_challenge={challenge}&code_challenge_method=S256",
        encode(&callback)
    ));
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    let until = Instant::now() + limit;
    let code = loop {
        if Instant::now() > until {
            return Err("sign-in timed out; try again".into());
        }
        match l.accept() {
            Ok((mut s, _)) => {
                let _ = s.set_nonblocking(false);
                let mut line = String::new();
                let _ = BufReader::new(&s).read_line(&mut line);
                let found = code_from(&line);
                let body = match &found {
                    Some(_) => page(
                        "Signed in to OpenRouter",
                        "You can close this tab and go back to jamalterm.",
                    ),
                    None => page(
                        "Sign-in didn't finish",
                        "No code came back from OpenRouter. Close this tab and try again from jamalterm.",
                    ),
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                if let Some(c) = found {
                    break c;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(format!("the sign-in listener stopped: {e}")),
        }
    };
    exchange(keys, &code, &verifier)
}

/// Trades the code for a key.
pub fn exchange(keys_url: &str, code: &str, verifier: &str) -> Result<String, String> {
    let resp = super::agent()
        .post(keys_url)
        .header("content-type", "application/json")
        .send_json(serde_json::json!({"code": code, "code_verifier": verifier, "code_challenge_method": "S256"}))
        .map_err(|e| format!("couldn't reach OpenRouter: {e}"))?;
    match resp.status().as_u16() {
        200 | 201 => {}
        403 => {
            return Err(
                "OpenRouter rejected the sign-in code (it may have expired); try again".into(),
            );
        }
        // Not the body: it may echo the code.
        s => return Err(format!("OpenRouter sign-in failed (HTTP {s})")),
    }
    let v: serde_json::Value = resp
        .into_body()
        .read_json()
        .map_err(|_| "OpenRouter's answer couldn't be read".to_string())?;
    v["key"]
        .as_str()
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "OpenRouter didn't send a key".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::mock;

    #[test]
    fn pkce_pairs() {
        let (v, c) = pkce();
        assert!(v.len() >= 43);
        assert_eq!(c, URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())));
        assert_ne!(pkce().0, v, "random");
    }

    #[test]
    fn callback_codes() {
        assert_eq!(
            code_from("GET /callback?code=abc123 HTTP/1.1").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            code_from("GET /callback?x=1&code=z HTTP/1.1").as_deref(),
            Some("z")
        );
        assert_eq!(code_from("GET /callback HTTP/1.1"), None);
        assert_eq!(code_from("GET /other?code=a HTTP/1.1"), None);
    }

    #[test]
    fn the_whole_sign_in() {
        let m = mock(vec![(200, r#"{"key":"sk-or-v1-new"}"#.into())]);
        let keys = format!("{}/keys", m.base);
        // The "browser": follows the sign-in page straight to the callback.
        let key = sign_in_with(
            |url: &str| {
                let cb = url
                    .split("callback_url=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap();
                let cb = cb.replace("%3A", ":").replace("%2F", "/");
                let addr = cb
                    .trim_start_matches("http://localhost:")
                    .trim_end_matches("/callback")
                    .to_string();
                std::thread::spawn(move || {
                    let mut s = std::net::TcpStream::connect(format!("127.0.0.1:{addr}")).unwrap();
                    write!(s, "GET /callback?code=the-code HTTP/1.1\r\n\r\n").unwrap();
                    let mut out = String::new();
                    let _ = std::io::Read::read_to_string(&mut s, &mut out);
                    assert!(out.contains("Signed in"));
                });
            },
            "https://openrouter.example/auth",
            &keys,
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(key, "sk-or-v1-new");
        let sent = m.bodies.lock().unwrap()[0].clone();
        assert_eq!(sent["code"], "the-code");
        assert_eq!(sent["code_challenge_method"], "S256");
    }

    #[test]
    fn refused_codes() {
        let m = mock(vec![(403, "{}".into())]);
        let e = exchange(&format!("{}/keys", m.base), "c", "v").unwrap_err();
        assert!(e.contains("rejected"));
    }
}
