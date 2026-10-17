// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review annotations around the laid-out pages (WP-66): comment balloons in the markup area
//! right of the page, their connectors, change bars in the left margin beside changed lines,
//! spelling squiggles, and the reviewer colours the markup uses.
//!
//! Everything here is in document coordinates (the pages' origins applied) and is drawn in the
//! surface's overlay, so it never invalidates the page tiles: balloons move when comments are
//! added, squiggles come and go as the speller works, without a repaint of the text.
//!
//! - [`place_balloons`] stacks one balloon per top-level comment at the height of its anchor,
//!   pushing later ones down so none overlap (Word's markup area), with heights the caller
//!   gives (the comment's text decides them).
//! - [`change_bars`] draws Word's vertical bars beside every line holding a tracked change.
//! - [`connector`] joins a comment's anchor to its balloon with a dotted line.
//! - [`squiggles`] draws wavy underlines under misspelled words.

use kiln_common::NodeId;
use kiln_doc::{Document, Pos};
use kiln_scene::{Bounds, ColorRole, DrawOp, Paint};

use crate::engine::DocLayout;
use crate::geometry;

/// Simple Markup's change bar colour (a quiet red, as in Word).
pub const SIMPLE_BAR: u32 = 0xC6_3B_2B_FF;
/// All Markup's change bar colour (a neutral grey).
pub const ALL_BAR: u32 = 0x8A_80_76_FF;
/// Gap between balloons, px.
pub const BALLOON_GAP: f32 = 8.0;
/// Distance of a change bar from the text area's left edge, px.
const BAR_GAP: f32 = 14.0;
/// Width of a change bar, px.
const BAR_WIDTH: f32 = 2.0;

/// The colour of a reviewer's changes and comments (`0xRRGGBBAA`), the same one the markup in
/// the text uses.
#[must_use]
pub fn author_color(author: &str) -> u32 {
    crate::source::author_color(author)
}

/// The colour of a reviewer as `RRGGBB`.
#[must_use]
pub fn author_hex(author: &str) -> String {
    format!("{:06X}", author_color(author) >> 8)
}

/// A comment balloon placed in the markup area.
#[derive(Debug, Clone, PartialEq)]
pub struct BalloonPlacement {
    /// The comment.
    pub comment: NodeId,
    /// The page it is beside.
    pub page: usize,
    /// Where the commented text starts (document coordinates).
    pub anchor: (f32, f32),
    /// The balloon (document coordinates).
    pub rect: Bounds,
}

/// Places the balloons of the layout's comments (All Markup): each at the height of its anchor
/// unless the balloon above it reaches further down, with `height(comment)` px of height.
/// Balloons the engine did not place (comments without an anchor in the laid-out pages, or a
/// layout without markup) have none.
#[must_use]
pub fn place_balloons(
    layout: &DocLayout,
    height: &dyn Fn(NodeId) -> f32,
) -> Vec<BalloonPlacement> {
    let mut out = Vec::new();
    for (i, page) in layout.pages().iter().enumerate() {
        let (ox, oy) = page.origin;
        let mut placed: Vec<(NodeId, (f32, f32), f32)> = page
            .balloons
            .iter()
            .map(|b| (b.comment, b.anchor, b.rect.x))
            .collect();
        placed.sort_by(|a, b| a.1.1.total_cmp(&b.1.1).then(a.1.0.total_cmp(&b.1.0)));
        let mut next_free = f32::MIN;
        for (comment, (ax, ay), x) in placed {
            let h = height(comment).clamp(32.0, 600.0);
            let top = (ay - 6.0).max(next_free);
            next_free = top + h + BALLOON_GAP;
            let width = page
                .balloons
                .iter()
                .find(|b| b.comment == comment)
                .map_or(240.0, |b| b.rect.w);
            out.push(BalloonPlacement {
                comment,
                page: i,
                anchor: (ox + ax, oy + ay),
                rect: Bounds::new(ox + x, oy + top, width, h),
            });
        }
    }
    out
}

/// Change bars: one bar in the left margin beside every line that holds part of the ranges.
#[must_use]
pub fn change_bars(
    layout: &DocLayout,
    doc: &Document,
    ranges: &[(Pos, Pos)],
    color: u32,
) -> Vec<DrawOp> {
    let mut bars: Vec<Bounds> = Vec::new();
    for &(a, b) in ranges {
        let rects = if a == b {
            geometry::caret_in(layout, a).map(|c| vec![c.rect]).unwrap_or_default()
        } else if a.para == b.para {
            paragraph_range_rects(layout, a.para, &[(a.offset.min(b.offset), a.offset.max(b.offset))])
        } else {
            geometry::selection_rects(layout, doc, a, b)
        };
        for r in rects {
            let Some(page) = layout.page_at(r.x + r.w / 2.0, r.y + r.h / 2.0) else {
                continue;
            };
            let Some(p) = layout.pages().get(page) else {
                continue;
            };
            let x = p.origin.0 + p.body.x - BAR_GAP;
            let bar = Bounds::new(x, r.y, BAR_WIDTH, r.h.max(1.0));
            // One bar per run of consecutive lines: merge with a bar it touches.
            match bars
                .iter_mut()
                .find(|o| {
                    (o.x - bar.x).abs() < 0.5
                        && o.y <= bar.bottom() + 1.0
                        && bar.y <= o.bottom() + 1.0
                })
            {
                Some(o) => {
                    let top = o.y.min(bar.y);
                    let bottom = o.bottom().max(bar.bottom());
                    *o = Bounds::new(o.x, top, o.w, bottom - top);
                }
                None => bars.push(bar),
            }
        }
    }
    bars.into_iter()
        .map(|rect| DrawOp::FillRect {
            rect,
            paint: Paint::Rgba(color),
            radius: 1.0,
        })
        .collect()
}

/// A dotted connector from a comment's anchor along the text line to the page edge, then to the
/// balloon's left edge.
#[must_use]
pub fn connector(anchor: (f32, f32), page_right: f32, balloon: Bounds, paint: Paint) -> Vec<DrawOp> {
    let (ax, ay) = anchor;
    let by = (balloon.y + 14.0).min(balloon.bottom());
    vec![
        DrawOp::Guide {
            x0: ax,
            y0: ay,
            x1: page_right,
            y1: ay,
            paint,
            dashed: true,
        },
        DrawOp::Guide {
            x0: page_right,
            y0: ay,
            x1: balloon.x,
            y1: by,
            paint,
            dashed: true,
        },
    ]
}

/// The rectangles of char ranges `[a, b)` of one paragraph on its laid-out lines (document
/// coordinates), for many ranges at once: unlike [`geometry::selection_rects`] it never walks the
/// document, so a page full of squiggles costs one pass over the paragraph's lines.
#[must_use]
pub fn paragraph_range_rects(
    layout: &DocLayout,
    para: NodeId,
    ranges: &[(usize, usize)],
) -> Vec<Bounds> {
    let mut out = Vec::new();
    if ranges.is_empty() {
        return out;
    }
    for &r in layout.lines_of(para) {
        let Some(l) = layout.line(r) else { continue };
        let Some(info) = l.para.lines.get(l.line as usize) else {
            continue;
        };
        let Some(piece) = l.para.pieces.get(info.piece as usize) else {
            continue;
        };
        let Some(line) = piece.layout.lines.get(info.line as usize) else {
            continue;
        };
        let map = &l.para.src.map;
        let page = r.page as usize;
        for &(from, to) in ranges {
            let a = map.byte_of(from).max(info.bytes.start);
            let b = map.byte_of(to).min(info.bytes.end);
            if a >= b {
                continue;
            }
            for rect in piece
                .layout
                .selection_rects(a - piece.byte_start..b - piece.byte_start)
            {
                if (rect.y - line.top).abs() > 0.01 {
                    continue;
                }
                let (x, y) = layout.to_doc(page, l.x + piece.x + rect.x, l.y + piece.y + rect.y);
                out.push(Bounds::new(x, y, rect.width, rect.height));
            }
        }
    }
    out
}

/// Wavy underlines under misspelled words: `(paragraph, [(start, end)])` char ranges.
#[must_use]
pub fn squiggles(layout: &DocLayout, words: &[(NodeId, Vec<(usize, usize)>)]) -> Vec<DrawOp> {
    let mut out = Vec::new();
    for (para, ranges) in words {
        for r in paragraph_range_rects(layout, *para, ranges) {
            if r.w < 1.0 {
                continue;
            }
            out.push(DrawOp::Squiggle {
                x: r.x,
                y: r.bottom() - 2.0,
                width: r.w,
                paint: Paint::Role(ColorRole::Squiggle),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
