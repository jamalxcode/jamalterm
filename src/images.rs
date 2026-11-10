//! Inline images (iTerm2's protocol, as `imgcat` and many tools send:
//! OSC 1337 File=…;inline=1:<base64>). PNG, JPEG and GIF (its first frame).
//! An image takes as many lines as its height needs, below the cursor, and
//! scrolls with the text. Sixel and the kitty graphics protocol aren't
//! supported (yet).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// An image placed in the scrollback.
#[derive(Clone)]
pub struct Placed {
    pub id: u64,
    /// Its first line (counted like the marks: from the top of the history).
    pub line: i64,
    pub col: usize,
    /// How many lines it takes.
    pub rows: usize,
    /// Its size on screen, in points.
    pub size: (f32, f32),
    pub image: Arc<egui::ColorImage>,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A size argument: "40" (cells), "200px", "50%" or "auto" (None).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Dim {
    Cells(f32),
    Px(f32),
    Percent(f32),
}

pub fn dim(v: &str) -> Option<Dim> {
    let v = v.trim();
    if v.is_empty() || v == "auto" {
        return None;
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.parse().ok().map(Dim::Px);
    }
    if let Some(n) = v.strip_suffix('%') {
        return n.parse().ok().map(Dim::Percent);
    }
    v.parse().ok().map(Dim::Cells)
}

/// The size to show an image at, in points: the arguments, or its own size;
/// the aspect ratio kept unless told otherwise; never wider than the screen.
pub fn fit(args: &str, img: (f32, f32), cell: (f32, f32), cols: usize) -> (f32, f32) {
    let mut w = None;
    let mut h = None;
    let mut keep = true;
    for a in args.split(';') {
        match a.split_once('=') {
            Some(("width", v)) => w = dim(v),
            Some(("height", v)) => h = dim(v),
            Some(("preserveAspectRatio", "0")) => keep = false,
            _ => {}
        }
    }
    let screen_w = cols as f32 * cell.0;
    let to_pt = |d: Dim, along: f32, cell_len: f32| match d {
        Dim::Cells(n) => n * cell_len,
        Dim::Px(n) => n,
        Dim::Percent(p) => along * p / 100.0,
    };
    let (iw, ih) = img;
    let (mut ow, mut oh) = match (w, h) {
        (Some(w), Some(h)) => (to_pt(w, screen_w, cell.0), to_pt(h, screen_w, cell.1)),
        (Some(w), None) => {
            let w = to_pt(w, screen_w, cell.0);
            (w, w * ih / iw.max(1.0))
        }
        (None, Some(h)) => {
            let h = to_pt(h, screen_w, cell.1);
            (h * iw / ih.max(1.0), h)
        }
        (None, None) => (iw, ih),
    };
    if keep && w.is_some() && h.is_some() {
        let s = (ow / iw).min(oh / ih);
        (ow, oh) = (iw * s, ih * s);
    }
    if ow > screen_w && ow > 0.0 {
        oh *= screen_w / ow;
        ow = screen_w;
    }
    (ow.max(1.0), oh.max(1.0))
}

/// Decodes and sizes an inline image (None if it isn't one we can read).
pub fn decode(args: &str, data: &[u8], cell: (f32, f32), cols: usize) -> Option<Placed> {
    let img = image::load_from_memory(data).ok()?.to_rgba8();
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || iw > 8192 || ih > 8192 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([iw as usize, ih as usize], img.as_raw());
    let size = fit(args, (iw as f32, ih as f32), cell, cols);
    Some(Placed {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        line: 0,
        col: 0,
        rows: (size.1 / cell.1).ceil().max(1.0) as usize,
        size,
        image: Arc::new(color),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tiny PNG (w×h, solid red), made with the png crate.
    pub fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![];
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            wr.write_image_data(&[255u8, 0, 0, 255].repeat((w * h) as usize))
                .unwrap();
        }
        out
    }

    #[test]
    fn sizes() {
        let cell = (10.0, 20.0);
        assert_eq!(dim("40"), Some(Dim::Cells(40.0)));
        assert_eq!(dim("200px"), Some(Dim::Px(200.0)));
        assert_eq!(dim("50%"), Some(Dim::Percent(50.0)));
        assert_eq!(dim("auto"), None);
        assert_eq!(
            fit("inline=1", (100.0, 50.0), cell, 80),
            (100.0, 50.0),
            "its own size"
        );
        assert_eq!(
            fit("width=20", (100.0, 50.0), cell, 80),
            (200.0, 100.0),
            "20 cells wide, ratio kept"
        );
        assert_eq!(fit("width=50%", (100.0, 50.0), cell, 80), (400.0, 200.0));
        assert_eq!(
            fit("inline=1", (1600.0, 800.0), cell, 80),
            (800.0, 400.0),
            "never wider than the screen"
        );
        let (w, h) = fit("width=10;height=10", (100.0, 50.0), cell, 80);
        assert_eq!((w, h), (100.0, 50.0), "fits the box, ratio kept");
        assert_eq!(
            fit(
                "width=10;height=10;preserveAspectRatio=0",
                (100.0, 50.0),
                cell,
                80
            ),
            (100.0, 200.0)
        );
    }

    #[test]
    fn decoding() {
        let p = decode("inline=1", &png(30, 45), (10.0, 20.0), 80).unwrap();
        assert_eq!(p.image.size, [30, 45]);
        assert_eq!(p.rows, 3, "45 points over 20-point lines");
        assert!(decode("inline=1", b"not an image", (10.0, 20.0), 80).is_none());
    }
}
