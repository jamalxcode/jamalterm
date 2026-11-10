//! Shell integration marks in the output: OSC 133 (prompt A, command start C,
//! done D;exit), OSC 7 (folder) and jamalterm's OSC 6973;cmd;<base64> (the
//! command as typed). The scanner finds them in the byte stream, across
//! reads, so the session can note the cursor position at each one; the bytes
//! themselves still go to the terminal (which ignores what it doesn't know).

use base64::Engine;

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Prompt,
    Start,
    Done(i32),
    Command(String),
    Cwd(String),
}

/// An OSC found in a chunk: `end` is the index just past it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub end: usize,
    pub mark: Mark,
}

#[derive(Default)]
enum State {
    #[default]
    Text,
    Esc,
    Osc,
    OscEsc,
}

/// Finds marks in output that arrives in pieces.
#[derive(Default)]
pub struct Scanner {
    state: State,
    buf: Vec<u8>,
}

const MAX_OSC: usize = 64 * 1024; // longer ones (images, clipboard) aren't ours

impl Scanner {
    pub fn scan(&mut self, chunk: &[u8]) -> Vec<Found> {
        let mut out = vec![];
        for (i, &b) in chunk.iter().enumerate() {
            match self.state {
                State::Text => {
                    if b == 0x1b {
                        self.state = State::Esc
                    }
                }
                State::Esc => {
                    self.state = if b == b']' {
                        self.buf.clear();
                        State::Osc
                    } else if b == 0x1b {
                        State::Esc
                    } else {
                        State::Text
                    }
                }
                State::Osc => match b {
                    0x07 => {
                        if let Some(m) = parse(&self.buf) {
                            out.push(Found {
                                end: i + 1,
                                mark: m,
                            });
                        }
                        self.state = State::Text;
                    }
                    0x1b => self.state = State::OscEsc,
                    _ => {
                        if self.buf.len() < MAX_OSC {
                            self.buf.push(b)
                        }
                    }
                },
                State::OscEsc => {
                    if b == b'\\' {
                        if let Some(m) = parse(&self.buf) {
                            out.push(Found {
                                end: i + 1,
                                mark: m,
                            });
                        }
                        self.state = State::Text;
                    } else if b == b']' {
                        self.buf.clear();
                        self.state = State::Osc; // a new OSC right away
                    } else {
                        self.state = State::Text;
                    }
                }
            }
        }
        out
    }
}

fn parse(body: &[u8]) -> Option<Mark> {
    let s = std::str::from_utf8(body).ok()?;
    let (code, rest) = s.split_once(';').unwrap_or((s, ""));
    match code {
        "133" => {
            // fish 4 writes its own A and C, with parameters ("A;click_events=1",
            // "C;cmdline_url=…") and before our hooks run: ignored, or each
            // command would also count once with no text. Ours are bare.
            let mut parts = rest.split(';');
            let kind = parts.next()?;
            let params = rest.len() > kind.len();
            match kind {
                "A" | "C" if params => None,
                "A" => Some(Mark::Prompt),
                "C" => Some(Mark::Start),
                "D" => Some(Mark::Done(
                    parts.next().and_then(|c| c.parse().ok()).unwrap_or(0),
                )),
                _ => None,
            }
        }
        "6973" => {
            let b64 = rest.strip_prefix("cmd;")?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            Some(Mark::Command(
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches('\n')
                    .to_string(),
            ))
        }
        "7" => {
            let after = rest.strip_prefix("file://")?;
            let path = &after[after.find('/')?..];
            Some(Mark::Cwd(percent_decode(path)))
        }
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(chunks: &[&[u8]]) -> Vec<Mark> {
        let mut s = Scanner::default();
        chunks
            .iter()
            .flat_map(|c| s.scan(c))
            .map(|f| f.mark)
            .collect()
    }

    #[test]
    fn finds_marks_and_where_they_end() {
        let data = b"hi\x1b]133;A\x07$ \x1b]6973;cmd;bHMgLWw=\x07\x1b]133;C\x07out\x1b]133;D;2\x07";
        let mut s = Scanner::default();
        let found = s.scan(data);
        assert_eq!(
            found.iter().map(|f| f.mark.clone()).collect::<Vec<_>>(),
            vec![
                Mark::Prompt,
                Mark::Command("ls -l".into()),
                Mark::Start,
                Mark::Done(2)
            ]
        );
        assert_eq!(&data[..found[0].end], b"hi\x1b]133;A\x07");
    }

    #[test]
    fn split_across_reads_and_st_endings() {
        // fish 4.9 ends marks with ST (ESC \) and adds parameters.
        assert_eq!(
            marks(&[b"\x1b]13", b"3;D;0\x1b", b"\\", b"\x1b]133;A\x1b\\"]),
            vec![Mark::Done(0), Mark::Prompt]
        );
        // fish 4's own marks (with parameters) are not ours: ignored.
        assert_eq!(
            marks(&[
                b"\x1b]133;A;click_events=1\x1b\\\x1b]133;C;cmdline_url=ls\x07\x1b]133;D;1\x07"
            ]),
            vec![Mark::Done(1)]
        );
    }

    #[test]
    fn folder_and_other_oscs() {
        assert_eq!(
            marks(&[b"\x1b]7;file://mac.local/Users/me/My%20Code\x07"]),
            vec![Mark::Cwd("/Users/me/My Code".into())]
        );
        assert_eq!(
            marks(&[b"\x1b]0;title\x07\x1b]52;c;aGk=\x07plain \x1b[31mred"]),
            vec![]
        );
    }
}
