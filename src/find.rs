//! Find (⌘F): every place the text appears in the scrollback, "3 of 17",
//! ⌘G / ⇧⌘G to step through them. Plain text, not a pattern; case matters
//! only when the text has a capital letter (like Terminal and Safari).

use alacritty_terminal::Term;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// At most this many matches are counted (a huge scrollback stays fast).
pub const LIMIT: usize = 1000;

#[derive(Default)]
pub struct Find {
    pub query: String,
    pub matches: Vec<Match>,
    pub current: usize,
    /// The query the matches are for (search again when it changes).
    searched: String,
}

/// Regex special characters escaped: the search is for the text as typed.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Every match from the top of the history to the bottom of the screen.
pub fn search_all<T>(term: &Term<T>, query: &str) -> Vec<Match> {
    if query.is_empty() {
        return vec![];
    }
    let Ok(mut regex) = RegexSearch::new(&escape(query)) else {
        return vec![];
    };
    let top = Point::new(Line(-(term.grid().history_size() as i32)), Column(0));
    let bottom = Point::new(
        Line(term.screen_lines() as i32 - 1),
        Column(term.columns() - 1),
    );
    RegexIter::new(top, bottom, Direction::Right, term, &mut regex)
        .take(LIMIT)
        .collect()
}

impl Find {
    /// Searches again if the text changed; the match nearest the bottom (the
    /// most recent output) becomes current.
    pub fn update<T>(&mut self, term: &Term<T>) -> bool {
        if self.query == self.searched {
            return false;
        }
        self.searched = self.query.clone();
        self.matches = search_all(term, &self.query);
        self.current = self.matches.len().saturating_sub(1);
        true
    }

    /// The next (1) or previous (-1) match, wrapping around.
    pub fn step(&mut self, dir: i32) {
        let n = self.matches.len();
        if n > 0 {
            self.current = (self.current as i64 + dir as i64).rem_euclid(n as i64) as usize;
        }
    }

    /// "3 of 17", "No matches", or "1000+ matches".
    pub fn status(&self) -> String {
        match self.matches.len() {
            0 if self.query.is_empty() => String::new(),
            0 => "No matches".into(),
            n if n >= LIMIT => format!("{} of {LIMIT}+", self.current + 1),
            n => format!("{} of {n}", self.current + 1),
        }
    }

    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::{feed_all, screen};

    #[test]
    fn finds_text_and_counts() {
        let mut s = screen(40, 5, 100);
        feed_all(
            &mut s,
            b"error: one\r\nok\r\nError: two\r\nall fine (1+1)\r\nerror again\r\n",
        );
        let mut f = Find {
            query: "error".into(),
            ..Default::default()
        };
        assert!(f.update(&s.term));
        assert_eq!(f.matches.len(), 3, "lower case: any case");
        assert_eq!(f.status(), "3 of 3", "the newest is current");
        f.step(1);
        assert_eq!(f.status(), "1 of 3", "wraps around");
        f.step(-1);
        assert_eq!(f.current, 2);
        f.query = "Error".into();
        f.update(&s.term);
        assert_eq!(f.matches.len(), 1, "a capital letter: exact case");
        f.query = "(1+1)".into();
        f.update(&s.term);
        assert_eq!(f.matches.len(), 1, "the text as typed, not a pattern");
        f.query = "nothing".into();
        f.update(&s.term);
        assert_eq!(f.status(), "No matches");
        assert!(!f.update(&s.term), "same text: no new search");
    }

    #[test]
    fn finds_text_in_the_history() {
        let mut s = screen(40, 3, 100);
        feed_all(&mut s, b"needle\r\n1\r\n2\r\n3\r\n4\r\n5\r\n");
        assert!(s.term.grid().history_size() > 0);
        assert_eq!(search_all(&s.term, "needle").len(), 1);
    }
}
