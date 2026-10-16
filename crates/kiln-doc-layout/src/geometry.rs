// SPDX-License-Identifier: AGPL-3.0-or-later
//! Positions ↔ geometry across pages, columns, tables and stories: hit testing (point → model
//! position), carets (with Word's second caret at bidi boundaries), selection highlights
//! (paragraph marks included), line ends for Home/End and line-to-line motion for Up/Down.

use kiln_doc::{Document, Pos, Story};
use kiln_scene::Bounds;
use kiln_text::Affinity;

use crate::engine::{DocLayout, LineRef};
use crate::page::{LineItem, StoryKind};

/// Width of the caret bar.
pub const CARET_W: f32 = 2.0;

/// A caret in document coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretGeom {
    /// Page index.
    pub page: usize,
    /// The bar (centred on the caret position, [`CARET_W`] wide).
    pub rect: Bounds,
    /// Right-to-left context.
    pub rtl: bool,
    /// Word draws a second caret where left-to-right and right-to-left text meet: the other
    /// edge.
    pub secondary: Option<Bounds>,
    /// The line.
    pub line: LineRef,
}

/// The result of a hit test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The position.
    pub pos: Pos,
    /// Which story was hit.
    pub story: StoryKind,
    /// Page index.
    pub page: usize,
    /// At the end of a wrapped line (the caret stays on that line).
    pub upstream: bool,
    /// The point is inside the line's box (not in a margin or between lines).
    pub inside: bool,
}

fn line_ref_of(layout: &DocLayout, pos: Pos, affinity: Affinity) -> Option<(LineRef, usize)> {
    let refs = layout.lines_of(pos.para);
    let first = layout.line(*refs.first()?)?;
    let byte = first.para.src.map.byte_of(pos.offset);
    let mut best: Option<(LineRef, usize)> = None;
    for &r in refs {
        let l = layout.line(r)?;
        let info = l.para.lines.get(l.line as usize)?;
        let last_of_para = info.bytes.end >= l.para.src.map.text().len();
        if info.bytes.contains(&byte) {
            if byte == info.bytes.start && affinity == Affinity::Upstream && best.is_some() {
                return best;
            }
            return Some((r, byte));
        }
        if byte == info.bytes.end {
            best = Some((r, byte));
            if last_of_para {
                return best;
            }
        }
        if byte < info.bytes.start && best.is_none() {
            return Some((r, info.bytes.start));
        }
    }
    best.or_else(|| refs.last().map(|r| (*r, byte)))
}

fn caret_x(l: &LineItem, byte: usize, affinity: Affinity) -> Option<(f32, f32, f32, bool, f32)> {
    let info = l.para.lines.get(l.line as usize)?;
    let piece = l.para.pieces.get(info.piece as usize)?;
    let line = piece.layout.lines.get(info.line as usize)?;
    let local = byte.saturating_sub(piece.byte_start);
    let c = piece.layout.caret_rect_with(local, affinity);
    // When the engine picks another line of the piece (a boundary), place the caret on ours.
    let (x, rtl) = if c.line == info.line as usize {
        (c.x, c.rtl)
    } else if local <= line.text_range.start {
        let e = piece
            .layout
            .caret_rect_with(line.text_range.start, Affinity::Downstream);
        (e.x, e.rtl)
    } else {
        let e = piece
            .layout
            .caret_rect_with(line.text_range.end, Affinity::Upstream);
        (e.x, e.rtl)
    };
    let other = piece.layout.caret_rect_with(
        local,
        if affinity == Affinity::Upstream {
            Affinity::Downstream
        } else {
            Affinity::Upstream
        },
    );
    let top = line.baseline - line.ascent;
    let h = line.ascent + line.descent;
    let ox = l.x + piece.x;
    let oy = l.y + piece.y;
    let second = if other.line == info.line as usize && (other.x - x).abs() > 0.5 {
        ox + other.x
    } else {
        f32::NAN
    };
    Some((ox + x, oy + top, h, rtl, second))
}

/// The caret at a position (downstream affinity).
#[must_use]
pub fn caret_in(layout: &DocLayout, pos: Pos) -> Option<CaretGeom> {
    caret_with(layout, pos, Affinity::Downstream)
}

/// The caret at a position with an affinity (upstream keeps a caret at a wrap on the line it
/// ends).
#[must_use]
pub fn caret_with(layout: &DocLayout, pos: Pos, affinity: Affinity) -> Option<CaretGeom> {
    let (r, byte) = line_ref_of(layout, pos, affinity)?;
    let l = layout.line(r)?;
    let (x, y, h, rtl, second) = caret_x(l, byte, affinity)?;
    let page = r.page as usize;
    let (dx, dy) = layout.to_doc(page, x, y);
    let secondary = second.is_finite().then(|| {
        let (sx, _) = layout.to_doc(page, second, y);
        Bounds::new(sx - CARET_W / 2.0, dy, CARET_W, h)
    });
    Some(CaretGeom {
        page,
        rect: Bounds::new(dx - CARET_W / 2.0, dy, CARET_W, h),
        rtl,
        secondary,
        line: r,
    })
}

/// The position under a document point. `story` limits the search to one story (clicks while
/// editing the body never land in a header); `None` searches every line.
#[must_use]
pub fn hit_test(layout: &DocLayout, x: f32, y: f32, story: Option<StoryKind>) -> Option<Hit> {
    let page = layout.page_at(x, y)?;
    let (ox, oy) = layout.to_doc(page, 0.0, 0.0);
    let (px, py) = (x - ox, y - oy);
    let lines = layout.page_lines(page);
    let wanted = |l: &LineItem| match story {
        None => true,
        Some(StoryKind::Body) => l.story == StoryKind::Body,
        Some(s) => l.story == s,
    };
    let mut best: Option<(f32, usize, bool)> = None;
    for (i, l) in lines.iter().enumerate() {
        if !wanted(l) {
            continue;
        }
        let r = l.rect();
        let inside_y = py >= r.y && py < r.bottom();
        let dx = if px < r.x {
            r.x - px
        } else if px > r.right() {
            px - r.right()
        } else {
            0.0
        };
        let dy = if inside_y {
            0.0
        } else if py < r.y {
            r.y - py
        } else {
            py - r.bottom()
        };
        // Rows first: a point beside a line belongs to it; between lines the nearer one.
        let d = dy * 4.0 + dx;
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, i, inside_y && dx == 0.0));
        }
    }
    let (_, i, inside) = match best {
        Some(b) => b,
        None => {
            // Nothing of that story on this page: the nearest line of the story anywhere.
            let r = nearest_in_story(layout, story, page)?;
            let l = layout.line(r)?;
            return hit_line(layout, l, r.page as usize, px, py, false);
        }
    };
    let l = &lines[i];
    hit_line(layout, l, page, px, py, inside)
}

fn nearest_in_story(layout: &DocLayout, story: Option<StoryKind>, page: usize) -> Option<LineRef> {
    let order = layout.reading_order();
    match story {
        Some(StoryKind::Body) | None => {
            let after = order.iter().find(|r| r.page as usize >= page);
            after.or_else(|| order.last()).copied()
        }
        Some(_) => None,
    }
}

fn hit_line(
    layout: &DocLayout,
    l: &LineItem,
    page: usize,
    px: f32,
    py: f32,
    inside: bool,
) -> Option<Hit> {
    let _ = layout;
    let info = l.para.lines.get(l.line as usize)?;
    let piece = l.para.pieces.get(info.piece as usize)?;
    let line = piece.layout.lines.get(info.line as usize)?;
    let lx = px - (l.x + piece.x);
    let ly = line.top + line.height / 2.0;
    let _ = py;
    let (local, aff) = piece.layout.hit_test_with_affinity(lx, ly);
    let mut byte = local + piece.byte_start;
    // Stay on this line (a piece may hold lines that were placed elsewhere).
    byte = byte.clamp(info.bytes.start, info.bytes.end);
    let map = &l.para.src.map;
    let offset = map.doc_of(byte);
    let upstream = aff == Affinity::Upstream || (byte == info.bytes.end && byte < map.text().len());
    Some(Hit {
        pos: Pos::new(l.para.id(), offset),
        story: l.story,
        page,
        upstream,
        inside,
    })
}

/// Paragraphs from `start` to `end` in document order (same story kind).
fn paragraphs_between(doc: &Document, start: Pos, end: Pos) -> Vec<kiln_common::NodeId> {
    let story = doc.story_of(start.para);
    let paras: Vec<kiln_common::NodeId> = match story {
        Some(Story::Body(_)) => doc.body_paragraphs().iter().map(|p| p.id).collect(),
        Some(s) => doc.paragraphs_in(s).iter().map(|p| p.id).collect(),
        None => return Vec::new(),
    };
    let a = paras.iter().position(|p| *p == start.para);
    let b = paras.iter().position(|p| *p == end.para);
    match (a, b) {
        (Some(a), Some(b)) if a <= b => paras[a..=b].to_vec(),
        (Some(a), Some(b)) => paras[b..=a].to_vec(),
        _ => vec![start.para],
    }
}

/// Highlight rectangles of the range `start..end` in document coordinates, with the width of a
/// paragraph mark for each paragraph end inside the range (as Word draws it).
#[must_use]
pub fn selection_rects(layout: &DocLayout, doc: &Document, start: Pos, end: Pos) -> Vec<Bounds> {
    let mut out = Vec::new();
    if start == end {
        return out;
    }
    let (start, end) = match doc.compare(start, end) {
        Some(std::cmp::Ordering::Greater) => (end, start),
        _ => (start, end),
    };
    let paras = paragraphs_between(doc, start, end);
    let n = paras.len();
    for (k, id) in paras.iter().enumerate() {
        for &r in layout.lines_of(*id) {
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
            let from = if k == 0 { map.byte_of(start.offset) } else { 0 };
            let to = if k + 1 == n {
                map.byte_of(end.offset)
            } else {
                map.text().len()
            };
            let a = from.max(info.bytes.start);
            let b = to.min(info.bytes.end);
            let page = r.page as usize;
            if a < b {
                for rect in piece
                    .layout
                    .selection_rects(a - piece.byte_start..b - piece.byte_start)
                {
                    if (rect.y - line.top).abs() > 0.01 {
                        continue;
                    }
                    let (x, y) =
                        layout.to_doc(page, l.x + piece.x + rect.x, l.y + piece.y + rect.y);
                    out.push(Bounds::new(x, y, rect.width, rect.height));
                }
            }
            // The paragraph mark at the end of the paragraph's last line.
            let last = info.bytes.end >= map.text().len();
            if last && k + 1 < n {
                let size = l
                    .para
                    .src
                    .spec
                    .props
                    .mark_style
                    .as_ref()
                    .map_or(14.0, |m| m.size);
                let (x, y) = layout.to_doc(
                    page,
                    l.x + piece.x + line.x + line.width,
                    l.y + piece.y + line.top,
                );
                out.push(Bounds::new(x, y, size * 0.4, line.height));
            }
        }
    }
    out
}

/// The positions where the line holding `pos` starts and ends (End stops before the spaces a
/// wrapped line ends with, so the caret stays on the line).
#[must_use]
pub fn line_bounds(layout: &DocLayout, pos: Pos) -> Option<(Pos, Pos)> {
    let (r, _) = line_ref_of(layout, pos, Affinity::Downstream)?;
    let l = layout.line(r)?;
    let info = l.para.lines.get(l.line as usize)?;
    let map = &l.para.src.map;
    let text = map.text();
    let last = info.bytes.end >= text.len();
    let end = if last {
        info.bytes.end
    } else {
        let s = &text[info.bytes.clone()];
        info.bytes.start + s.trim_end_matches([' ', '\n', '\u{2028}', '\u{b}']).len()
    };
    let start = map.doc_of(info.bytes.start);
    Some((
        Pos::new(pos.para, start),
        Pos::new(pos.para, map.doc_of(end)),
    ))
}

/// The position one line above or below `pos` at document x `goal_x`, following the reading
/// order of the body (or the page order of other stories); `None` at the first or last line.
#[must_use]
pub fn vertical(layout: &DocLayout, pos: Pos, goal_x: f32, down: bool) -> Option<Pos> {
    let (r, _) = line_ref_of(layout, pos, Affinity::Downstream)?;
    let l = layout.line(r)?;
    let target = if l.story == StoryKind::Body {
        let order = layout.reading_order();
        let i = order.iter().position(|x| *x == r)?;
        if down {
            *order.get(i + 1)?
        } else {
            *order.get(i.checked_sub(1)?)?
        }
    } else {
        let lines = layout.page_lines(r.page as usize);
        let same: Vec<usize> = (0..lines.len())
            .filter(|&i| lines[i].story == l.story)
            .collect();
        let i = same.iter().position(|&i| i == r.index as usize)?;
        let j = if down { i + 1 } else { i.checked_sub(1)? };
        LineRef {
            page: r.page,
            index: u32::try_from(*same.get(j)?).ok()?,
        }
    };
    let t = layout.line(target)?;
    let page = target.page as usize;
    let (ox, oy) = layout.to_doc(page, 0.0, 0.0);
    let rect = t.rect();
    hit_line(
        layout,
        t,
        page,
        goal_x - ox,
        oy + rect.y + rect.h / 2.0 - oy,
        true,
    )
    .map(|h| h.pos)
}
