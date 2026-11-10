//! Drawing a terminal: cells grouped into runs of the same style (one text
//! shape per run, not per character), backgrounds, selection, the cursor, and
//! the ✓/✕ mark beside each command in the left margin.

use crate::session::Screen;
use crate::themes::Theme;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use egui::{
    Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2,
};

/// Room on the left for the command marks.
pub const GUTTER: f32 = 18.0;
pub const PAD: f32 = 4.0;

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-bold".into()))
}

/// The size of one character cell.
pub fn cell_size(ctx: &egui::Context, font_size: f32, line_height: f32) -> Vec2 {
    let w = ctx.fonts_mut(|f| f.glyph_width(&mono(font_size), 'M'));
    vec2(
        w.max(1.0),
        (font_size * line_height * 1.18).round().max(1.0),
    )
}

/// Columns and rows that fit in a rect.
pub fn grid_size(rect: Rect, cell: Vec2) -> (u16, u16) {
    let cols = ((rect.width() - GUTTER - PAD) / cell.x).floor().max(2.0) as u16;
    let rows = ((rect.height() - 2.0 * PAD) / cell.y).floor().max(1.0) as u16;
    (cols, rows)
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// The xterm 256-color palette above the 16 named colors.
pub fn indexed(i: u8, theme: &Theme) -> Color32 {
    match i {
        0..=15 => theme.ansi[i as usize],
        16..=231 => {
            let i = i - 16;
            let v = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
            Color32::from_rgb(v(i / 36), v((i / 6) % 6), v(i % 6))
        }
        _ => {
            let g = 8 + (i - 232) * 10;
            Color32::from_rgb(g, g, g)
        }
    }
}

fn dim(c: Color32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * 0.66) as u8,
        (c.g() as f32 * 0.66) as u8,
        (c.b() as f32 * 0.66) as u8,
    )
}

/// A cell color: what the program set (OSC 4 overrides first), else the theme.
pub fn resolve(
    c: Color,
    theme: &Theme,
    overrides: &alacritty_terminal::term::color::Colors,
) -> Color32 {
    match c {
        Color::Spec(rgbv) => rgb(rgbv),
        Color::Indexed(i) => overrides[i as usize]
            .map(rgb)
            .unwrap_or_else(|| indexed(i, theme)),
        Color::Named(n) => {
            if let Some(o) = overrides[n] {
                return rgb(o);
            }
            let idx = n as usize;
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground => theme.foreground,
                NamedColor::Background => theme.background,
                NamedColor::Cursor => theme.cursor,
                NamedColor::DimForeground => dim(theme.foreground),
                _ if idx < 16 => theme.ansi[idx],
                // Dim black … dim white (259..267).
                _ if (259..267).contains(&idx) => dim(theme.ansi[idx - 259]),
                _ => theme.foreground,
            }
        }
    }
}

/// What the view needs besides the screen.
pub struct Look<'a> {
    pub theme: &'a Theme,
    pub font_size: f32,
    pub cell: Vec2,
    pub cursor_style: &'a str,
    pub focused: bool,
    pub blink_on: bool,
}

/// Paints the terminal into rect. Returns the top-left of the cell grid.
pub fn paint(painter: &Painter, rect: Rect, s: &Screen, look: &Look) -> Pos2 {
    let theme = look.theme;
    painter.rect_filled(rect, 0.0, theme.background);
    let origin = pos2(rect.left() + GUTTER, rect.top() + PAD);
    let cell = look.cell;
    let content = s.term.renderable_content();
    let offset = content.display_offset as i32;
    let colors = content.colors;
    let sel = content.selection;
    let mode = content.mode;
    let cursor = content.cursor;

    // Runs of cells with the same look, per line.
    struct Run {
        line: i32,
        col: usize,
        text: String,
        fg: Color32,
        bold: bool,
        underline: bool,
        strike: bool,
    }
    let mut runs: Vec<Run> = vec![];
    let mut bgs: Vec<(i32, usize, usize, Color32)> = vec![]; // line, col, width, color
    for item in content.display_iter {
        let cellv = item.cell;
        if cellv
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let line = item.point.line.0 + offset;
        let col = item.point.column.0;
        let mut fg = resolve(cellv.fg, theme, colors);
        let mut bg = resolve(cellv.bg, theme, colors);
        let bold = cellv.flags.contains(Flags::BOLD);
        if bold {
            // Bold plain colors use their bright version, like other terminals.
            if let Color::Named(n) = cellv.fg {
                let i = n as usize;
                if i < 8 {
                    fg = theme.ansi[i + 8];
                }
            }
        }
        if cellv.flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        let selected = sel.is_some_and(|r| r.contains(item.point));
        if cellv.flags.contains(Flags::INVERSE) ^ selected {
            if selected && !cellv.flags.contains(Flags::INVERSE) {
                bg = theme.selection.to_opaque();
            } else {
                std::mem::swap(&mut fg, &mut bg);
            }
        }
        if cellv.flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        let width = if cellv.flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        if bg != theme.background {
            match bgs.last_mut() {
                Some(b) if b.0 == line && b.1 + b.2 == col && b.3 == bg => b.2 += width,
                _ => bgs.push((line, col, width, bg)),
            }
        }
        let ch = cellv.c;
        if ch == ' ' || ch == '\0' {
            continue;
        }
        let underline = cellv.flags.intersects(Flags::ALL_UNDERLINES);
        let strike = cellv.flags.contains(Flags::STRIKEOUT);
        let mut text = String::from(ch);
        if let Some(extra) = cellv.zerowidth() {
            text.extend(extra.iter());
        }
        // Wide characters (CJK, emoji) get their own run so the grid stays aligned.
        match runs.last_mut() {
            Some(r)
                if width == 1
                    && r.line == line
                    && r.col + r.text.chars().count() == col
                    && r.fg == fg
                    && r.bold == bold
                    && r.underline == underline
                    && r.strike == strike
                    && !r.text.chars().any(|c| c.len_utf8() > 2)
                    && text.len() == 1 =>
            {
                r.text.push(ch)
            }
            _ => runs.push(Run {
                line,
                col,
                text,
                fg,
                bold,
                underline,
                strike,
            }),
        }
    }

    let at = |line: i32, col: usize| {
        pos2(
            origin.x + col as f32 * cell.x,
            origin.y + line as f32 * cell.y,
        )
    };
    for (line, col, w, color) in bgs {
        painter.rect_filled(
            Rect::from_min_size(at(line, col), vec2(w as f32 * cell.x, cell.y)),
            0.0,
            color,
        );
    }
    for r in &runs {
        let font = if r.bold {
            mono_bold(look.font_size)
        } else {
            mono(look.font_size)
        };
        let p = at(r.line, r.col);
        let galley = painter.layout_no_wrap(r.text.clone(), font, r.fg);
        // Centre the glyphs in the line height.
        let dy = ((cell.y - galley.size().y) / 2.0).max(0.0);
        let width = r.text.chars().count() as f32 * cell.x;
        painter.galley(p + vec2(0.0, dy), galley, r.fg);
        if r.underline {
            painter.line_segment(
                [p + vec2(0.0, cell.y - 2.0), p + vec2(width, cell.y - 2.0)],
                Stroke::new(1.0, r.fg),
            );
        }
        if r.strike {
            painter.line_segment(
                [p + vec2(0.0, cell.y / 2.0), p + vec2(width, cell.y / 2.0)],
                Stroke::new(1.0, r.fg),
            );
        }
    }

    // The cursor (hidden when the program hides it, or while scrolled up).
    if mode.contains(TermMode::SHOW_CURSOR) && cursor.shape != CursorShape::Hidden {
        let line = cursor.point.line.0 + offset;
        if line >= 0 && (line as usize) < s.term.screen_lines() {
            let r = Rect::from_min_size(at(line, cursor.point.column.0), cell);
            let color = theme.cursor;
            if !look.focused {
                painter.rect_stroke(
                    r.shrink(0.5),
                    1.0,
                    Stroke::new(1.0, color),
                    StrokeKind::Inside,
                );
            } else if look.blink_on {
                match look.cursor_style {
                    "bar" => {
                        painter.rect_filled(
                            Rect::from_min_size(r.min, vec2(2.0, cell.y)),
                            0.0,
                            color,
                        );
                    }
                    "underline" => {
                        painter.rect_filled(
                            Rect::from_min_size(
                                pos2(r.left(), r.bottom() - 2.0),
                                vec2(cell.x, 2.0),
                            ),
                            0.0,
                            color,
                        );
                    }
                    _ => {
                        painter.rect_filled(r, 1.0, color);
                        // The character under a block cursor, in the background color.
                        let g = &s.term.grid()
                            [Point::new(Line(cursor.point.line.0), Column(cursor.point.column.0))];
                        if g.c != ' ' {
                            painter.text(
                                r.left_top()
                                    + vec2(0.0, ((cell.y - look.font_size * 1.2) / 2.0).max(0.0)),
                                egui::Align2::LEFT_TOP,
                                g.c,
                                mono(look.font_size),
                                theme.background,
                            );
                        }
                    }
                }
            }
        }
    }

    // Command marks: ✓ worked, ✕ failed, … running (shapes, not just color).
    let grid = s.term.grid();
    let top = grid.history_size() as i64 - offset as i64;
    let rows = s.term.screen_lines() as i64;
    for b in &s.commands.blocks {
        let line = b.prompt - top;
        if line < 0 || line >= rows {
            continue;
        }
        let (mark, color) = match b.exit {
            Some(0) => ("✓", theme.ansi[2]),
            Some(_) => ("✕", theme.ansi[1]),
            None => ("…", theme.ansi[4]),
        };
        painter.text(
            pos2(
                rect.left() + GUTTER / 2.0,
                origin.y + line as f32 * cell.y + cell.y / 2.0,
            ),
            egui::Align2::CENTER_CENTER,
            mark,
            mono_bold(look.font_size * 0.85),
            color,
        );
    }
    origin
}

/// The cell under a point in the view (for selection), clamped to the grid.
pub fn point_at(
    pos: Pos2,
    origin: Pos2,
    cell: Vec2,
    s: &Screen,
) -> (Point, alacritty_terminal::index::Side) {
    let cols = s.term.columns();
    let rows = s.term.screen_lines() as i32;
    let x = (pos.x - origin.x) / cell.x;
    let col = (x.floor().max(0.0) as usize).min(cols.saturating_sub(1));
    let side = if x.fract() < 0.5 {
        alacritty_terminal::index::Side::Left
    } else {
        alacritty_terminal::index::Side::Right
    };
    let row = (((pos.y - origin.y) / cell.y).floor() as i32).clamp(0, rows - 1);
    let offset = s.term.grid().display_offset() as i32;
    (Point::new(Line(row - offset), Column(col)), side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette() {
        let t = crate::themes::by_id("jamal-night");
        assert_eq!(indexed(1, &t), t.ansi[1]);
        assert_eq!(indexed(16, &t), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed(231, &t), Color32::from_rgb(255, 255, 255));
        assert_eq!(indexed(232, &t), Color32::from_rgb(8, 8, 8));
    }

    #[test]
    fn grid_fits() {
        let (c, r) = grid_size(
            Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(GUTTER + PAD + 80.0 * 8.0, 2.0 * PAD + 24.0 * 17.0),
            ),
            vec2(8.0, 17.0),
        );
        assert_eq!((c, r), (80, 24));
    }
}
