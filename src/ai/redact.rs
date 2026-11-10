//! Hides things that look like secrets in terminal text before it's sent to
//! an AI provider, and says how many it hid. A safety net, not a guarantee:
//! it catches common key and token formats (3.x's rules, unchanged). The
//! user's own question is never changed.

use regex::Regex;
use std::sync::LazyLock;

struct Rule {
    re: Regex,
    /// Group 1 is a name to keep ("password="), group 2 an optional tail.
    keep: bool,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |p: &str, keep: bool| Rule {
        re: Regex::new(p).expect("redaction rule"),
        keep,
    };
    vec![
        // Private keys (the whole block).
        r(
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            false,
        ),
        // Provider API keys and tokens.
        r(
            r"\bsk-(?:ant|or|proj|svcacct|admin)-[A-Za-z0-9_\-]{16,}",
            false,
        ),
        r(r"\bsk-[A-Za-z0-9]{20,}\b", false),
        r(r"\bxai-[A-Za-z0-9]{20,}\b", false),
        r(r"\bAIza[0-9A-Za-z_\-]{35}\b", false),
        r(r"\b(?:sk|rk)_(?:live|test)_[0-9A-Za-z]{16,}\b", false),
        r(
            r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})\b",
            false,
        ),
        r(r"\bglpat-[A-Za-z0-9_\-]{20,}\b", false),
        r(r"\bxox[abprs]-[A-Za-z0-9\-]{10,}\b", false),
        r(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", false),
        r(
            r"\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\b",
            false,
        ),
        // "Authorization: Bearer <token>"
        r(
            r#"(?i)(authorization:\s*(?:bearer|token|basic)\s+)[^\s"']+"#,
            true,
        ),
        // user:password@ in URLs
        r(r"(://[^\s:/@]+:)[^\s@/]+(@)", true),
        // NAME=value / NAME: value for password-like names
        r(
            r#"(?i)(\b[A-Z0-9_]*(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key)[A-Z0-9_]*\b["']?\s*[=:]\s*["']?)[^\s"']{6,}"#,
            true,
        ),
    ]
});

const HIDDEN: &str = "[hidden secret]";

/// The text with secrets hidden, and how many were.
pub fn redact(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut n = 0;
    for rule in RULES.iter() {
        out = rule
            .re
            .replace_all(&out, |c: &regex::Captures| {
                let m = &c[0];
                if m.contains("[hidden") {
                    return m.to_string(); // already hidden by an earlier rule
                }
                n += 1;
                if !rule.keep {
                    return HIDDEN.to_string();
                }
                let tail = c.get(2).map(|t| t.as_str()).unwrap_or("");
                format!("{}{HIDDEN}{tail}", &c[1])
            })
            .into_owned();
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3.x's cases.
    #[test]
    fn hides_secrets() {
        let cases = [
            (
                "export ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz",
                "export ANTHROPIC_API_KEY=[hidden secret]",
            ),
            (
                "key sk-or-v1-0123456789abcdef0123456789abcdef here",
                "key [hidden secret] here",
            ),
            (
                "OPENAI sk-proj-AbCdEfGhIjKlMnOpQrStUv",
                "OPENAI [hidden secret]",
            ),
            ("aws AKIAIOSFODNN7EXAMPLE ok", "aws [hidden secret] ok"),
            (
                "token ghp_0123456789abcdefghijklmnopqrstuvwxyz",
                "token [hidden secret]",
            ),
            (
                "Authorization: Bearer abc.def.ghi",
                "Authorization: Bearer [hidden secret]",
            ),
            (
                "git clone https://jamal:hunter2secret@github.com/x/y",
                "git clone https://jamal:[hidden secret]@github.com/x/y",
            ),
            (
                r#"DB_PASSWORD="correcthorse""#,
                r#"DB_PASSWORD="[hidden secret]""#,
            ),
            (
                "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpM",
                "jwt [hidden secret]",
            ),
        ];
        for (input, want) in cases {
            let (got, n) = redact(input);
            assert_eq!(got, want, "{input}");
            assert!(n > 0);
        }
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(
            redact(&format!("cat id\n{key}\ndone")).0,
            "cat id\n[hidden secret]\ndone"
        );
    }

    #[test]
    fn leaves_ordinary_output_alone() {
        let plain = "ls -la\ntotal 8\n-rw-r--r-- 1 j staff 12 notes.txt\ngit status: on branch main\npassword reset email sent";
        assert_eq!(redact(plain), (plain.to_string(), 0));
    }
}
