// SPDX-License-Identifier: AGPL-3.0-or-later
//! Show/Hide ¶: formatting marks drawn in the overlay over the visible pages — a pilcrow at
//! every paragraph end, a centred dot for every space, an arrow for every tab — in the theme's
//! guide colour, so they never print or export.

use kiln_doc::{Document, Pos};
use kiln_doc_layout::geometry::caret_in;
use kiln_scene::{Bounds, ColorRole, DrawOp, FillRule, Paint, PathData, Stroke};

use crate::apps::write::WriteView;

/// Adds a clockwise rectangle to a path.
fn rect(p: &mut PathData, x: f32, y: f32, w: f32, h: f32) {
    p.move_to(x, y)
        .line_to(x + w, y)
        .line_to(x + w, y + h)
        .line_to(x, y + h)
        .close();
}

/// A pilcrow (¶) in a box whose top-left is `(x, y)` and height `h`: a half-disc bowl, a top
/// bar and two stems, all wound the same way.
fn pilcrow(x: f32, y: f32, h: f32) -> PathData {
    let w = h * 0.56;
    let stem = (h * 0.1).max(0.8);
    let r = w * 0.5;
    let (cx, cy) = (x + r, y + r);
    let k = 0.552_284_8 * r;
    let mut p = PathData::new();
    // Bowl: from the top of the first stem round the left side to its middle, clockwise.
    p.move_to(cx, y)
        .line_to(cx, y + 2.0 * r)
        .cubic_to(cx - k, y + 2.0 * r, x, cy + k, x, cy)
        .cubic_to(x, cy - k, cx - k, y, cx, y)
        .close();
    rect(&mut p, cx, y, w - r, stem);
    rect(&mut p, cx, y, stem, h);
    rect(&mut p, x + w - stem, y, stem, h);
    p
}

/// A tab arrow from `x0` to `x1` at height `y`.
fn arrow(x0: f32, x1: f32, y: f32, h: f32) -> PathData {
    let head = (h * 0.22).max(2.0);
    let mut p = PathData::new();
    p.move_to(x0 + 1.0, y)
        .line_to(x1 - 1.0, y)
        .move_to(x1 - 1.0 - head, y - head)
        .line_to(x1 - 1.0, y)
        .line_to(x1 - 1.0 - head, y + head);
    p
}

/// The formatting marks of the visible pages.
#[must_use]
pub fn marks(v: &WriteView, doc: &Document) -> Vec<DrawOp> {
    let paint = Paint::Role(ColorRole::Guide);
    let mut fill = PathData::new();
    let mut strokes = PathData::new();
    let layout = &v.layout;
    let (first, last) = v.visible_pages();
    let mut dots = Vec::new();
    for page in first..=last {
        for item in layout.page_lines(page) {
            let para = item.para.id();
            let Some(info) = item.para.lines.get(item.line as usize) else {
                continue;
            };
            let map = &item.para.src.map;
            let text = map.text();
            let (a, b) = (
                info.bytes.start.min(text.len()),
                info.bytes.end.min(text.len()),
            );
            if a > b || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
                continue;
            }
            for (i, c) in text[a..b].char_indices() {
                if c != ' ' && c != '\t' {
                    continue;
                }
                let off = map.doc_of(a + i);
                let (Some(here), Some(next)) = (
                    caret_in(layout, Pos::new(para, off)),
                    caret_in(layout, Pos::new(para, off + 1)),
                ) else {
                    continue;
                };
                let (x0, x1) = (here.rect.x, next.rect.x.max(here.rect.x + 1.0));
                let mid = here.rect.y + here.rect.h * 0.58;
                if c == ' ' {
                    let r = (here.rect.h * 0.06).clamp(0.8, 2.0);
                    dots.push(Bounds::new((x0 + x1) / 2.0 - r, mid - r, 2.0 * r, 2.0 * r));
                } else {
                    strokes = append_path(strokes, arrow(x0, x1, mid, here.rect.h));
                }
            }
            // The paragraph mark after the paragraph's last line.
            if item.line as usize + 1 == item.para.lines.len() {
                let len = doc.paragraph(para).map_or(0, |p| p.content.len());
                if let Some(c) = caret_in(layout, Pos::new(para, len)) {
                    let h = c.rect.h * 0.62;
                    let y = c.rect.y + (c.rect.h - h) * 0.55;
                    fill = append_path(fill, pilcrow(c.rect.x + 2.0, y, h));
                }
            }
        }
    }
    let mut ops: Vec<DrawOp> = dots
        .into_iter()
        .map(|rect| DrawOp::FillRect {
            rect,
            paint,
            radius: rect.w / 2.0,
        })
        .collect();
    if !fill.is_empty() {
        ops.push(DrawOp::Path {
            path: fill,
            fill: Some(paint),
            stroke: None,
            rule: FillRule::NonZero,
        });
    }
    if !strokes.is_empty() {
        ops.push(DrawOp::Path {
            path: strokes,
            fill: None,
            stroke: Some(Stroke::solid(paint, 1.0)),
            rule: FillRule::NonZero,
        });
    }
    ops
}

fn append_path(mut a: PathData, b: PathData) -> PathData {
    a.verbs.extend(b.verbs);
    a.coords.extend(b.coords);
    a
}
