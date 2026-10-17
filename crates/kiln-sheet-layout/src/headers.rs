// SPDX-License-Identifier: AGPL-3.0-or-later
//! Row and column headers (the brand kit's Grid mockup): letters and numbers on the panel
//! colour with hairline borders; the headers of selected rows and columns take the app's tint,
//! its text colour and a 2 px bar on the cell side; wholly selected ones are solid.

use kiln_scene::{Bounds, ColorRole, DrawOp, Paint, PathData};
use kiln_sheet::Range;
use kiln_text::{ParagraphSpec, SpanStyle, TextEngine};

use crate::geom::{SheetGeometry, Snap};
use crate::glyphs::GlyphCache;

/// Font size of header labels.
pub const LABEL_PX: f32 = 11.5;

/// How much of a header the selection covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Marked {
    /// Not selected.
    No,
    /// Some cells of the row or column.
    Partly,
    /// The whole row or column.
    Wholly,
}

/// The selection as the headers show it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct HeaderMarks {
    /// The selected ranges.
    pub ranges: Vec<Range>,
}

impl HeaderMarks {
    /// How a column header is marked.
    #[must_use]
    pub fn col(&self, col: u32) -> Marked {
        let mut m = Marked::No;
        for r in &self.ranges {
            if r.c0 <= col && col <= r.c1 {
                if r.is_whole_cols() {
                    return Marked::Wholly;
                }
                m = Marked::Partly;
            }
        }
        m
    }

    /// How a row header is marked.
    #[must_use]
    pub fn row(&self, row: u32) -> Marked {
        let mut m = Marked::No;
        for r in &self.ranges {
            if r.r0 <= row && row <= r.r1 {
                if r.is_whole_rows() {
                    return Marked::Wholly;
                }
                m = Marked::Partly;
            }
        }
        m
    }

    /// A signature of the marks of columns `a..=b` (for tile versions).
    #[must_use]
    pub fn col_signature(&self, a: u32, b: u32) -> u64 {
        signature(
            self.ranges.iter().map(|r| (r.c0, r.c1, r.is_whole_cols())),
            a,
            b,
        )
    }

    /// A signature of the marks of rows `a..=b`.
    #[must_use]
    pub fn row_signature(&self, a: u32, b: u32) -> u64 {
        signature(
            self.ranges.iter().map(|r| (r.r0, r.r1, r.is_whole_rows())),
            a,
            b,
        )
    }
}

fn signature(spans: impl Iterator<Item = (u32, u32, bool)>, a: u32, b: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (lo, hi, whole) in spans {
        if hi < a || lo > b {
            continue;
        }
        for v in [u64::from(lo.max(a)), u64::from(hi.min(b)), u64::from(whole)] {
            h = (h ^ v).wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

/// Column letters: 0 → `A`, 25 → `Z`, 26 → `AA`, 16383 → `XFD`.
#[must_use]
pub fn col_letters(mut col: u32) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn label(
    engine: &mut TextEngine,
    glyphs: &mut GlyphCache,
    text: &str,
    cell: Bounds,
    tile: (f32, f32),
    paint: Paint,
    out: &mut Vec<DrawOp>,
) {
    let mut style = SpanStyle::new("Calibri", LABEL_PX);
    style.kerning = false;
    let layout = engine.layout(&ParagraphSpec::plain(text, style, 1.0e6));
    let w = layout.lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let h = layout.height;
    if w > cell.w + 2.0 {
        return;
    }
    let x = cell.x + (cell.w - w) / 2.0;
    let y = cell.y + (cell.h - h) / 2.0;
    let s = glyphs.scale();
    let ox = x * s - (tile.0 * s).round();
    let oy = y * s - (tile.1 * s).round();
    for line in &layout.lines {
        for run in &line.runs {
            glyphs.push_run(engine.fonts(), run, (ox, oy), paint, out);
        }
    }
}

fn marked_paints(m: Marked) -> (Paint, Paint) {
    match m {
        Marked::No => (Paint::Role(ColorRole::Panel), Paint::Role(ColorRole::Muted)),
        Marked::Partly => (
            Paint::Role(ColorRole::AppTint),
            Paint::Role(ColorRole::AppText),
        ),
        Marked::Wholly => (Paint::Role(ColorRole::AppFill), Paint::Rgba(0xFFFF_FFFF)),
    }
}

/// The column header tile covering sheet x `region.x .. region.right()` (its y is the header
/// band, `-header_h .. 0`).
#[must_use]
pub fn column_header_ops(
    geo: &SheetGeometry,
    region: Bounds,
    marks: &HeaderMarks,
    engine: &mut TextEngine,
    glyphs: &mut GlyphCache,
) -> Vec<DrawOp> {
    let snap = Snap(glyphs.scale());
    let px = snap.px(1.0);
    let tile = (region.x, region.y);
    let h = region.h;
    let mut out = vec![DrawOp::FillRect {
        rect: Bounds::new(0.0, 0.0, region.w, h),
        paint: Paint::Role(ColorRole::Panel),
        radius: 0.0,
    }];
    let (c0, c1) = geo
        .cols
        .indices_in(f64::from(region.x), f64::from(region.right()));
    for c in c0..=c1 {
        if geo.cols.is_hidden(c) {
            continue;
        }
        let (x, w) = geo.cols.span(c);
        let x0 = snap.at(x);
        let x1 = snap.at(x + w);
        let cell = Bounds::new(x0 - tile.0, 0.0, x1 - x0, h);
        let m = marks.col(c);
        let (bg, ink) = marked_paints(m);
        if m != Marked::No {
            out.push(DrawOp::FillRect {
                rect: cell,
                paint: bg,
                radius: 0.0,
            });
        }
        out.push(DrawOp::FillRect {
            rect: Bounds::new(cell.right() - px, 0.0, px, h),
            paint: Paint::Role(ColorRole::Line),
            radius: 0.0,
        });
        label(
            engine,
            glyphs,
            &col_letters(c),
            Bounds::new(x0, region.y, x1 - x0, h),
            tile,
            ink,
            &mut out,
        );
        if m == Marked::Partly {
            out.push(DrawOp::FillRect {
                rect: Bounds::new(cell.x, h - 2.0, cell.w, 2.0),
                paint: Paint::Role(ColorRole::AppFill),
                radius: 0.0,
            });
        }
    }
    out.push(DrawOp::FillRect {
        rect: Bounds::new(0.0, h - px, region.w, px),
        paint: Paint::Role(ColorRole::Line),
        radius: 0.0,
    });
    out
}

/// The row header tile covering sheet y `region.y .. region.bottom()` (its x is the header
/// band, `-header_w .. 0`).
#[must_use]
pub fn row_header_ops(
    geo: &SheetGeometry,
    region: Bounds,
    marks: &HeaderMarks,
    engine: &mut TextEngine,
    glyphs: &mut GlyphCache,
) -> Vec<DrawOp> {
    let snap = Snap(glyphs.scale());
    let px = snap.px(1.0);
    let tile = (region.x, region.y);
    let w = region.w;
    let mut out = vec![DrawOp::FillRect {
        rect: Bounds::new(0.0, 0.0, w, region.h),
        paint: Paint::Role(ColorRole::Panel),
        radius: 0.0,
    }];
    let (r0, r1) = geo
        .rows
        .indices_in(f64::from(region.y), f64::from(region.bottom()));
    for r in r0..=r1 {
        if geo.rows.is_hidden(r) {
            continue;
        }
        let (y, h) = geo.rows.span(r);
        let y0 = snap.at(y);
        let y1 = snap.at(y + h);
        let cell = Bounds::new(0.0, y0 - tile.1, w, y1 - y0);
        let m = marks.row(r);
        let (bg, ink) = marked_paints(m);
        if m != Marked::No {
            out.push(DrawOp::FillRect {
                rect: cell,
                paint: bg,
                radius: 0.0,
            });
        }
        out.push(DrawOp::FillRect {
            rect: Bounds::new(0.0, cell.bottom() - px, w, px),
            paint: Paint::Role(ColorRole::Line),
            radius: 0.0,
        });
        label(
            engine,
            glyphs,
            &(u64::from(r) + 1).to_string(),
            Bounds::new(region.x, y0, w, y1 - y0),
            tile,
            ink,
            &mut out,
        );
        if m == Marked::Partly {
            out.push(DrawOp::FillRect {
                rect: Bounds::new(w - 2.0, cell.y, 2.0, cell.h),
                paint: Paint::Role(ColorRole::AppFill),
                radius: 0.0,
            });
        }
    }
    out.push(DrawOp::FillRect {
        rect: Bounds::new(w - px, 0.0, px, region.h),
        paint: Paint::Role(ColorRole::Line),
        radius: 0.0,
    });
    out
}

/// The corner box: panel, borders and the Select All triangle.
#[must_use]
pub fn corner_ops(w: f32, h: f32, all: bool, snap: Snap) -> Vec<DrawOp> {
    let px = snap.px(1.0);
    let mut tri = PathData::new();
    tri.move_to(w - 4.0, h - 12.0)
        .line_to(w - 4.0, h - 4.0)
        .line_to(w - 12.0, h - 4.0)
        .close();
    vec![
        DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, w, h),
            paint: Paint::Role(if all {
                ColorRole::AppTint
            } else {
                ColorRole::Panel
            }),
            radius: 0.0,
        },
        DrawOp::Path {
            path: tri,
            fill: Some(Paint::Role(if all {
                ColorRole::AppFill
            } else {
                ColorRole::Line
            })),
            stroke: None,
            rule: kiln_scene::FillRule::NonZero,
        },
        DrawOp::FillRect {
            rect: Bounds::new(w - px, 0.0, px, h),
            paint: Paint::Role(ColorRole::Line),
            radius: 0.0,
        },
        DrawOp::FillRect {
            rect: Bounds::new(0.0, h - px, w, px),
            paint: Paint::Role(ColorRole::Line),
            radius: 0.0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_letters_follow_excel() {
        assert_eq!(col_letters(0), "A");
        assert_eq!(col_letters(25), "Z");
        assert_eq!(col_letters(26), "AA");
        assert_eq!(col_letters(701), "ZZ");
        assert_eq!(col_letters(702), "AAA");
        assert_eq!(col_letters(16_383), "XFD");
    }

    #[test]
    fn marks_partial_and_whole_selections() {
        let marks = HeaderMarks {
            ranges: vec![Range::new(2, 1, 5, 3), Range::new(0, 7, 1_048_575, 7)],
        };
        assert_eq!(marks.col(0), Marked::No);
        assert_eq!(marks.col(2), Marked::Partly);
        assert_eq!(marks.col(7), Marked::Wholly);
        assert_eq!(marks.row(3), Marked::Partly);
        assert_ne!(marks.col_signature(0, 4), marks.col_signature(5, 9));
        assert_eq!(
            marks.col_signature(10, 20),
            HeaderMarks::default().col_signature(10, 20)
        );
    }
}
