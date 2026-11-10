//! Server-sent events: `event:` and `data:` lines, a blank line ends one.

use std::io::BufRead;

/// Calls `f(event, data)` for each event until the stream ends or `f`
/// returns false.
pub fn read(r: impl BufRead, mut f: impl FnMut(&str, &str) -> bool) -> std::io::Result<()> {
    let mut event = String::new();
    let mut data = String::new();
    for line in r.lines() {
        let line = line?;
        if line.is_empty() {
            if !data.is_empty() && !f(&event, &data) {
                return Ok(());
            }
            event.clear();
            data.clear();
        } else if let Some(v) = line.strip_prefix("event:") {
            event = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(v.strip_prefix(' ').unwrap_or(v));
        }
    }
    if !data.is_empty() {
        f(&event, &data);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn events() {
        let text = "event: a\ndata: {\"x\":1}\n\n: comment\ndata: one\ndata: two\n\ndata: [DONE]";
        let mut got = vec![];
        super::read(text.as_bytes(), |e, d| {
            got.push((e.to_string(), d.to_string()));
            true
        })
        .unwrap();
        assert_eq!(
            got,
            [
                ("a".into(), "{\"x\":1}".into()),
                (String::new(), "one\ntwo".into()),
                (String::new(), "[DONE]".into())
            ]
        );
    }
}
