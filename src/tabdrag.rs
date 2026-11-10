//! Dragging tabs: along the tab bar to reorder them; out of the window to
//! move the tab into another jamalterm window (let go over it) or into a new
//! window right there. Esc while dragging puts the tab back. Whatever runs
//! in the tab keeps running: its shells just change windows.

use crate::restore::Frame;
use egui::{Pos2, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drop {
    /// Back in the bar: the tab goes to this place.
    Reorder(usize),
    /// Over another window (its id): the tab moves there.
    ToWindow(u64),
    /// Outside every window: a new window here (screen position).
    NewWindow(Pos2),
    Nothing,
}

/// Where a dragged tab lands. `pos` is the pointer in this window, `window`
/// this window's area, `here` its place on screen, `bar` the tab bar and
/// `tabs` each tab's place in it; `others` the other windows on screen.
pub fn drop_at(
    pos: Pos2,
    window: Rect,
    here: Option<Frame>,
    bar: Rect,
    tabs: &[Rect],
    others: &[(u64, Frame)],
) -> Drop {
    if window.contains(pos) {
        if bar.expand2(egui::vec2(0.0, 40.0)).contains(pos) {
            return Drop::Reorder(slot(pos.x, tabs));
        }
        return Drop::Nothing; // let go over its own terminal: stays put
    }
    let Some(here) = here else {
        return Drop::Nothing;
    };
    let screen = Pos2::new(here.x as f32 + pos.x, here.y as f32 + pos.y);
    for (id, f) in others {
        let r = Rect::from_min_size(
            Pos2::new(f.x as f32, f.y as f32),
            egui::vec2(f.w as f32, f.h as f32 + 30.0), // with its title bar
        );
        if r.contains(screen) {
            return Drop::ToWindow(*id);
        }
    }
    Drop::NewWindow(screen)
}

/// The place in the bar for a pointer at `x`: before the first tab whose
/// middle is right of it.
pub fn slot(x: f32, tabs: &[Rect]) -> usize {
    tabs.iter()
        .position(|r| x < r.center().x)
        .unwrap_or(tabs.len())
}

/// Moves item `from` to place `to` (a slot counted before the move).
pub fn reorder<T>(v: &mut Vec<T>, from: usize, to: usize) -> usize {
    let t = v.remove(from);
    let to = if to > from { to - 1 } else { to }.min(v.len());
    v.insert(to, t);
    to
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<Rect> {
        (0..3)
            .map(|i| Rect::from_min_size(Pos2::new(i as f32 * 100.0, 0.0), egui::vec2(90.0, 24.0)))
            .collect()
    }

    #[test]
    fn reordering() {
        let mut v = vec!['a', 'b', 'c'];
        assert_eq!(reorder(&mut v, 0, 2), 1);
        assert_eq!(v, ['b', 'a', 'c']);
        assert_eq!(reorder(&mut v, 2, 0), 0);
        assert_eq!(v, ['c', 'b', 'a']);
        assert_eq!(reorder(&mut v, 0, 3), 2, "to the end");
        assert_eq!(v, ['b', 'a', 'c']);
        assert_eq!(slot(10.0, &tabs()), 0);
        assert_eq!(slot(60.0, &tabs()), 1);
        assert_eq!(slot(500.0, &tabs()), 3);
    }

    #[test]
    fn where_a_tab_lands() {
        let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let bar = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 28.0));
        let here = Some(Frame {
            x: 100,
            y: 100,
            w: 800,
            h: 600,
        });
        let other = (
            7,
            Frame {
                x: 1000,
                y: 100,
                w: 500,
                h: 400,
            },
        );
        let at = |x: f32, y: f32| drop_at(Pos2::new(x, y), window, here, bar, &tabs(), &[other]);
        assert_eq!(at(250.0, 10.0), Drop::Reorder(3));
        assert_eq!(at(120.0, 50.0), Drop::Reorder(1), "a little below the bar");
        assert_eq!(at(400.0, 300.0), Drop::Nothing, "over its own terminal");
        assert_eq!(
            at(1000.0, 200.0),
            Drop::ToWindow(7),
            "over the other window"
        );
        assert_eq!(
            at(-50.0, 700.0),
            Drop::NewWindow(Pos2::new(50.0, 800.0)),
            "anywhere else"
        );
        assert_eq!(
            drop_at(Pos2::new(-5.0, 5.0), window, None, bar, &tabs(), &[]),
            Drop::Nothing,
            "unknown place on screen"
        );
    }
}
