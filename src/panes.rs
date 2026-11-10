//! Split panes: a tab is a tree of panes. A split puts two parts side by
//! side (⌘D, `Dir::Right`) or one above the other (⇧⌘D, `Dir::Down`), with a
//! divider that can be dragged (double-click: equal halves). Closing a pane
//! gives its space to the part next to it. Generic over the pane, so the
//! layout is tested without starting shells.

use egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    /// Side by side (a vertical divider).
    Right,
    /// One above the other (a horizontal divider).
    Down,
}

pub enum Node<T> {
    Leaf(T),
    Split {
        dir: Dir,
        /// The first part's share of the space (0.1–0.9).
        ratio: f32,
        a: Box<Node<T>>,
        b: Box<Node<T>>,
    },
}

/// A divider on screen: which split it belongs to (the path of first/second
/// turns from the root), where it is, and the space its split divides.
#[derive(Clone, Debug, PartialEq)]
pub struct Divider {
    pub path: Vec<bool>,
    pub dir: Dir,
    pub rect: Rect,
    pub area: Rect,
}

/// The gap between panes, where the divider is drawn and grabbed.
pub const GAP: f32 = 6.0;

impl<T> Node<T> {
    /// The panes, first to last (the order ⌘] goes through them).
    pub fn leaves(&self) -> Vec<&T> {
        match self {
            Node::Leaf(t) => vec![t],
            Node::Split { a, b, .. } => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    pub fn leaves_mut(&mut self) -> Vec<&mut T> {
        match self {
            Node::Leaf(t) => vec![t],
            Node::Split { a, b, .. } => {
                let mut v = a.leaves_mut();
                v.extend(b.leaves_mut());
                v
            }
        }
    }

    pub fn count(&self) -> usize {
        match self {
            Node::Leaf(_) => 1,
            Node::Split { a, b, .. } => a.count() + b.count(),
        }
    }

    pub fn find(&self, is: &impl Fn(&T) -> bool) -> Option<&T> {
        self.leaves().into_iter().find(|t| is(t))
    }

    pub fn find_mut(&mut self, is: &impl Fn(&T) -> bool) -> Option<&mut T> {
        self.leaves_mut().into_iter().find(|t| is(t))
    }

    /// Splits the pane `is` picks: it keeps the first part, `new` gets the
    /// second. Gives `new` back if no pane matched.
    pub fn split(self, is: &impl Fn(&T) -> bool, dir: Dir, new: &mut Option<T>) -> Node<T> {
        match self {
            Node::Leaf(t) if new.is_some() && is(&t) => Node::Split {
                dir,
                ratio: 0.5,
                a: Box::new(Node::Leaf(t)),
                b: Box::new(Node::Leaf(new.take().unwrap())),
            },
            Node::Split {
                dir: d,
                ratio,
                a,
                b,
            } => {
                let a = a.split(is, dir, new);
                let b = b.split(is, dir, new);
                Node::Split {
                    dir: d,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }
            }
            leaf => leaf,
        }
    }

    /// Takes out the pane `is` picks; its neighbour takes the space. Returns
    /// what's left (None when that was the last pane) and the removed pane.
    pub fn remove(self, is: &impl Fn(&T) -> bool) -> (Option<Node<T>>, Option<T>) {
        match self {
            Node::Leaf(t) if is(&t) => (None, Some(t)),
            Node::Split { dir, ratio, a, b } => {
                let (a, gone) = a.remove(is);
                if gone.is_some() {
                    return (Some(a.unwrap_or(*b)), gone);
                }
                let a = a.unwrap();
                let (b, gone) = b.remove(is);
                match b {
                    None => (Some(a), gone),
                    Some(b) => (
                        Some(Node::Split {
                            dir,
                            ratio,
                            a: Box::new(a),
                            b: Box::new(b),
                        }),
                        gone,
                    ),
                }
            }
            leaf => (Some(leaf), None),
        }
    }

    /// The same tree with each pane made into another; panes `f` can't make
    /// are left out (their neighbours take the space). None if none are left.
    pub fn filter_map<U>(self, f: &mut impl FnMut(T) -> Option<U>) -> Option<Node<U>> {
        match self {
            Node::Leaf(t) => f(t).map(Node::Leaf),
            Node::Split { dir, ratio, a, b } => match (a.filter_map(f), b.filter_map(f)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (a, b) => a.or(b),
            },
        }
    }

    /// Where every pane and divider goes in `rect`.
    pub fn layout<'a>(
        &'a self,
        rect: Rect,
        panes: &mut Vec<(&'a T, Rect)>,
        divs: &mut Vec<Divider>,
    ) {
        self.layout_at(rect, &mut vec![], panes, divs);
    }

    fn layout_at<'a>(
        &'a self,
        rect: Rect,
        path: &mut Vec<bool>,
        panes: &mut Vec<(&'a T, Rect)>,
        divs: &mut Vec<Divider>,
    ) {
        match self {
            Node::Leaf(t) => panes.push((t, rect)),
            Node::Split { dir, ratio, a, b } => {
                let (ra, div, rb) = cut(rect, *dir, *ratio);
                divs.push(Divider {
                    path: path.clone(),
                    dir: *dir,
                    rect: div,
                    area: rect,
                });
                path.push(false);
                a.layout_at(ra, path, panes, divs);
                path.pop();
                path.push(true);
                b.layout_at(rb, path, panes, divs);
                path.pop();
            }
        }
    }

    /// The split at `path` (from a Divider).
    fn at_path(&mut self, path: &[bool]) -> Option<&mut Node<T>> {
        match (path.split_first(), self) {
            (None, n) => Some(n),
            (Some((second, rest)), Node::Split { a, b, .. }) => {
                if *second { b } else { a }.at_path(rest)
            }
            _ => None,
        }
    }

    /// Moves a divider to follow the pointer.
    pub fn drag(&mut self, d: &Divider, pointer: Pos2) {
        if let Some(Node::Split { ratio, .. }) = self.at_path(&d.path) {
            let r = match d.dir {
                Dir::Right => (pointer.x - d.area.left()) / d.area.width(),
                Dir::Down => (pointer.y - d.area.top()) / d.area.height(),
            };
            *ratio = r.clamp(0.1, 0.9);
        }
    }

    /// Double-clicking a divider makes the two parts equal.
    pub fn equalize(&mut self, d: &Divider) {
        if let Some(Node::Split { ratio, .. }) = self.at_path(&d.path) {
            *ratio = 0.5;
        }
    }
}

/// Cuts a rect in two with a GAP between.
fn cut(r: Rect, dir: Dir, ratio: f32) -> (Rect, Rect, Rect) {
    match dir {
        Dir::Right => {
            let x = (r.left() + (r.width() - GAP) * ratio).round();
            (
                Rect::from_min_max(r.min, Pos2::new(x, r.bottom())),
                Rect::from_min_max(Pos2::new(x, r.top()), Pos2::new(x + GAP, r.bottom())),
                Rect::from_min_max(Pos2::new(x + GAP, r.top()), r.max),
            )
        }
        Dir::Down => {
            let y = (r.top() + (r.height() - GAP) * ratio).round();
            (
                Rect::from_min_max(r.min, Pos2::new(r.right(), y)),
                Rect::from_min_max(Pos2::new(r.left(), y), Pos2::new(r.right(), y + GAP)),
                Rect::from_min_max(Pos2::new(r.left(), y + GAP), r.max),
            )
        }
    }
}

/// The pane next to `from` in a direction (⌥⌘ arrows): of the panes on that
/// side that overlap it across, the nearest; ties go to the one most in line.
pub fn neighbour(rects: &[Rect], from: usize, toward: Vec2) -> Option<usize> {
    let f = rects.get(from)?;
    let gap = GAP + 1.0;
    rects
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != from)
        .filter_map(|(i, r)| {
            let (dist, overlap) = if toward.x > 0.0 {
                (r.left() - f.right(), overlap(r.y_range(), f.y_range()))
            } else if toward.x < 0.0 {
                (f.left() - r.right(), overlap(r.y_range(), f.y_range()))
            } else if toward.y > 0.0 {
                (r.top() - f.bottom(), overlap(r.x_range(), f.x_range()))
            } else {
                (f.top() - r.bottom(), overlap(r.x_range(), f.x_range()))
            };
            (dist >= -0.5 && dist <= gap && overlap > 0.0).then_some((i, dist, overlap))
        })
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap()
                .then(b.2.partial_cmp(&a.2).unwrap())
        })
        .map(|(i, _, _)| i)
}

fn overlap(a: egui::Rangef, b: egui::Rangef) -> f32 {
    a.max.min(b.max) - a.min.max(b.min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1006.0, 606.0))
    }

    fn split(n: Node<u32>, at: u32, dir: Dir, new: u32) -> Node<u32> {
        let mut new = Some(new);
        let n = n.split(&|p| *p == at, dir, &mut new);
        assert!(new.is_none(), "pane {at} not found");
        n
    }

    fn rects(n: &Node<u32>) -> Vec<(u32, Rect)> {
        let (mut p, mut d) = (vec![], vec![]);
        n.layout(area(), &mut p, &mut d);
        p.into_iter().map(|(t, r)| (*t, r)).collect()
    }

    #[test]
    fn split_close_and_order() {
        // 1 | 2, then 2 split down: 1 | (2 / 3)
        let n = split(Node::Leaf(1), 1, Dir::Right, 2);
        let n = split(n, 2, Dir::Down, 3);
        assert_eq!(n.leaves(), vec![&1, &2, &3]);
        assert_eq!(n.count(), 3);
        let r = rects(&n);
        assert_eq!(
            r[0].1,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 606.0))
        );
        assert_eq!(r[1].1.left(), 506.0, "the gap between");
        assert_eq!(r[1].1.bottom(), 300.0);
        assert_eq!(r[2].1.top(), 306.0);
        // Closing 2: 3 takes the whole right side.
        let (n, gone) = n.remove(&|p| *p == 2);
        assert_eq!(gone, Some(2));
        let n = n.unwrap();
        assert_eq!(n.leaves(), vec![&1, &3]);
        assert_eq!(rects(&n)[1].1.height(), 606.0);
        // Closing the first pane: the other gets everything.
        let (n, _) = n.remove(&|p| *p == 1);
        let n = n.unwrap();
        assert_eq!(rects(&n), vec![(3, area())]);
        let (n, gone) = n.remove(&|p| *p == 3);
        assert!(n.is_none() && gone == Some(3), "the last pane");
    }

    #[test]
    fn mapping_a_tree_leaves_out_what_fails() {
        let n = split(split(Node::Leaf(1), 1, Dir::Right, 2), 2, Dir::Down, 3);
        let m = n.filter_map(&mut |p| (p != 2).then_some(p * 10)).unwrap();
        assert_eq!(m.leaves(), vec![&10, &30]);
        assert!(matches!(
            m,
            Node::Split {
                dir: Dir::Right,
                ..
            }
        ));
        assert!(Node::Leaf(1).filter_map(&mut |_| None::<u32>).is_none());
    }

    #[test]
    fn a_missing_pane_changes_nothing() {
        let n = Node::Leaf(1);
        let mut new = Some(9);
        let n = n.split(&|p| *p == 5, Dir::Right, &mut new);
        assert_eq!(new, Some(9));
        let (n, gone) = n.remove(&|p| *p == 5);
        assert!(gone.is_none());
        assert_eq!(n.unwrap().leaves(), vec![&1]);
    }

    #[test]
    fn dragging_and_equalizing_a_divider() {
        let mut n = split(Node::Leaf(1), 1, Dir::Right, 2);
        let (mut p, mut d) = (vec![], vec![]);
        n.layout(area(), &mut p, &mut d);
        let div = d[0].clone();
        drop(p);
        n.drag(&div, Pos2::new(251.5, 10.0));
        assert!((rects(&n)[0].1.width() - 250.0).abs() < 2.0);
        n.drag(&div, Pos2::new(5.0, 10.0));
        assert!(rects(&n)[0].1.width() > 90.0, "never squeezed shut");
        n.equalize(&div);
        assert_eq!(rects(&n)[0].1.width(), 500.0);
    }

    #[test]
    fn neighbours_by_direction() {
        // 1 | (2 / 3)
        let n = split(split(Node::Leaf(1), 1, Dir::Right, 2), 2, Dir::Down, 3);
        let r: Vec<Rect> = rects(&n).into_iter().map(|(_, r)| r).collect();
        let right = Vec2::new(1.0, 0.0);
        let left = Vec2::new(-1.0, 0.0);
        let down = Vec2::new(0.0, 1.0);
        let up = Vec2::new(0.0, -1.0);
        assert_eq!(neighbour(&r, 0, right), Some(1), "ties: the top one");
        assert_eq!(neighbour(&r, 2, left), Some(0));
        assert_eq!(neighbour(&r, 1, down), Some(2));
        assert_eq!(neighbour(&r, 2, up), Some(1));
        assert_eq!(neighbour(&r, 0, left), None, "nothing there");
        assert_eq!(neighbour(&r, 1, right), None);
    }
}
