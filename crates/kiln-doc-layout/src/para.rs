// SPDX-License-Identifier: AGPL-3.0-or-later
//! Laid-out paragraphs: lines from `kiln-text`, optionally flowed around floating objects.
//!
//! A paragraph is normally one `kiln-text` layout. Next to a floating object its lines have
//! other widths and positions, which one layout cannot express, so the paragraph becomes several
//! *pieces*: each piece lays out the rest of the text in the free interval beside the object and
//! keeps its first line (or every line, once the object is passed).

use std::ops::Range;
use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::st::WrapSide;
use kiln_scene::Bounds;
use kiln_text::{ParagraphLayout, ParagraphSpec, TextEngine, TextRun};

use crate::source::ParaSource;

/// Free intervals narrower than this are skipped (Word leaves very thin gaps empty).
pub const MIN_GAP: f32 = 18.0;

/// One `kiln-text` layout of part of a paragraph.
#[derive(Debug, Clone)]
pub struct Piece {
    /// The layout (its byte offsets are relative to `byte_start`).
    pub layout: ParagraphLayout,
    /// First byte of the paragraph's laid-out text in this piece.
    pub byte_start: usize,
    /// Span index of the piece's first run in the paragraph's spec.
    pub span_base: usize,
    /// Horizontal offset of the piece's text area from the paragraph's text area.
    pub x: f32,
    /// Vertical offset of the piece's origin from the paragraph's top.
    pub y: f32,
}

/// A line of a laid-out paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    /// Its piece.
    pub piece: u32,
    /// Index of the line in the piece's layout.
    pub line: u32,
    /// Top relative to the paragraph's top.
    pub top: f32,
    /// Height.
    pub height: f32,
    /// Byte range in the paragraph's laid-out text.
    pub bytes: Range<usize>,
    /// A forced break after the line: `Some(false)` page, `Some(true)` column.
    pub forced: Option<bool>,
    /// Notes referenced on the line.
    pub notes: Vec<NodeId>,
}

impl LineInfo {
    /// Bottom relative to the paragraph's top.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// A laid-out paragraph.
#[derive(Debug, Clone)]
pub struct ParaBox {
    /// The source.
    pub src: Arc<ParaSource>,
    /// Pieces (one unless the paragraph wraps around a float).
    pub pieces: Vec<Piece>,
    /// Lines in order.
    pub lines: Vec<LineInfo>,
    /// Width of the text area.
    pub width: f32,
}

impl ParaBox {
    /// The paragraph.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.src.id
    }

    /// Height of all lines.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.lines.last().map_or(0.0, LineInfo::bottom)
    }

    /// Height of lines `range`.
    #[must_use]
    pub fn lines_height(&self, range: Range<usize>) -> f32 {
        match (
            self.lines.get(range.start),
            range.end.checked_sub(1).and_then(|e| self.lines.get(e)),
        ) {
            (Some(a), Some(b)) if range.end > range.start => b.bottom() - a.top,
            _ => 0.0,
        }
    }

    /// The layout line of a line.
    #[must_use]
    pub fn text_line(&self, i: usize) -> Option<(&Piece, &kiln_text::Line)> {
        let l = self.lines.get(i)?;
        let p = self.pieces.get(l.piece as usize)?;
        Some((p, p.layout.lines.get(l.line as usize)?))
    }

    /// Lays out a paragraph without obstacles.
    pub fn simple(src: Arc<ParaSource>, engine: &mut TextEngine) -> Self {
        let layout = engine.layout(&src.spec);
        let width = src.spec.width;
        let mut b = Self {
            src,
            pieces: Vec::new(),
            lines: Vec::new(),
            width,
        };
        b.push_piece(layout, 0, 0, 0.0, 0.0, usize::MAX);
        b
    }

    /// Appends a piece and `take` of its lines (all with `usize::MAX`). Returns the byte after
    /// the last line taken.
    fn push_piece(
        &mut self,
        layout: ParagraphLayout,
        byte_start: usize,
        span_base: usize,
        x: f32,
        y: f32,
        take: usize,
    ) -> usize {
        let piece = u32::try_from(self.pieces.len()).unwrap_or(u32::MAX);
        let mut end = byte_start;
        for (i, line) in layout.lines.iter().enumerate().take(take) {
            let bytes = line.text_range.start + byte_start..line.text_range.end + byte_start;
            end = bytes.end;
            let forced = self
                .src
                .breaks
                .iter()
                .find(|b| b.byte == bytes.end && bytes.end > bytes.start)
                .map(|b| b.column);
            let notes = self
                .src
                .notes
                .iter()
                .filter(|n| bytes.contains(&n.byte))
                .map(|n| n.note)
                .collect();
            self.lines.push(LineInfo {
                piece,
                line: u32::try_from(i).unwrap_or(u32::MAX),
                top: y + line.top,
                height: line.height,
                bytes,
                forced,
                notes,
            });
        }
        self.pieces.push(Piece {
            layout,
            byte_start,
            span_base,
            x,
            y,
        });
        end
    }

    /// Lays out the paragraph from byte `from` (0 = the whole paragraph) with its top at page
    /// y `top`, text area starting at page x `left`, flowing around `obstacles`. Without an
    /// obstacle in the way this is one layout.
    pub fn wrapped(
        src: Arc<ParaSource>,
        engine: &mut TextEngine,
        from: usize,
        left: f32,
        top: f32,
        obstacles: &[Obstacle],
    ) -> Self {
        let width = src.spec.width;
        let len = src.map.text().len();
        let mut b = Self {
            src,
            pieces: Vec::new(),
            lines: Vec::new(),
            width,
        };
        let mut byte = from.min(len);
        let mut y = 0.0_f32;
        let mut guard = 0usize;
        while byte < len || b.lines.is_empty() {
            guard += 1;
            if guard > 10_000 {
                break;
            }
            let (spec, span_base) = continuation(&b.src.spec, byte, 0.0, b.width, byte > 0);
            let full = engine.layout(&spec);
            let Some(first) = full.lines.first() else {
                b.push_piece(full, byte, span_base, 0.0, y, usize::MAX);
                break;
            };
            let h = first.height.max(1.0);
            let band = (top + y, top + y + h);
            let blocked = blocked_in(obstacles, band, left, left + width);
            if blocked.is_empty() {
                // Take lines until one meets an obstacle.
                let mut take = 0usize;
                let mut yy = y;
                for line in &full.lines {
                    let band = (top + yy, top + yy + line.height);
                    if take > 0 && !blocked_in(obstacles, band, left, left + width).is_empty() {
                        break;
                    }
                    take += 1;
                    yy += line.height;
                }
                let origin = y - first.top;
                byte = b.push_piece(full, byte, span_base, 0.0, origin, take);
                y = yy;
                continue;
            }
            if blocked.iter().any(|x| x.full) {
                y = next_y(obstacles, band, left, left + width) - top;
                continue;
            }
            let intervals = free_intervals(&blocked, left, left + width);
            if intervals.is_empty() {
                y = next_y(obstacles, band, left, left + width) - top;
                continue;
            }
            let rtl = b.src.format.bidi;
            let mut ordered = intervals;
            if rtl {
                ordered.reverse();
            }
            let mut band_h = 0.0_f32;
            for (ix, iw) in ordered {
                if byte >= len && !b.lines.is_empty() {
                    break;
                }
                let x = ix - left;
                let (spec, span_base) = continuation(&b.src.spec, byte, x, iw, byte > 0);
                let piece = engine.layout(&spec);
                let Some(l0) = piece.lines.first() else {
                    break;
                };
                band_h = band_h.max(l0.height);
                let origin = y - l0.top;
                byte = b.push_piece(piece, byte, span_base, x, origin, 1);
            }
            y += band_h.max(1.0);
        }
        b
    }
}

/// A spec for the text from `byte` on, in a text area `width` wide whose start edge is `dx` px
/// from the paragraph's. Returns the spec and the index of its first run in the original.
#[must_use]
pub fn continuation(
    spec: &ParagraphSpec,
    byte: usize,
    dx: f32,
    width: f32,
    continued: bool,
) -> (ParagraphSpec, usize) {
    let full_width = spec.width;
    let mut props = spec.props.clone();
    // Indents stay where they are on the page: an area starting further right absorbs the left
    // indent, an area ending earlier absorbs the right one.
    props.indent_start = (props.indent_start - dx).max(0.0);
    let right_gap = (full_width - (dx + width)).max(0.0);
    props.indent_end = (props.indent_end - right_gap).max(0.0);
    if continued {
        props.first_line = 0.0;
    } else if props.indent_start + props.first_line < 0.0 {
        props.first_line = -props.indent_start;
    }
    // Tab stops are measured from the paragraph's start edge.
    for t in &mut props.tab_stops {
        t.position -= dx;
    }
    if byte == 0 {
        return (
            ParagraphSpec {
                runs: spec.runs.clone(),
                props,
                width,
            },
            0,
        );
    }
    let mut runs = Vec::new();
    let mut pos = 0usize;
    let mut base = None;
    for (i, r) in spec.runs.iter().enumerate() {
        let end = pos + r.text.len();
        if end > byte {
            let cut = byte.saturating_sub(pos);
            if base.is_none() {
                base = Some(i);
            }
            let text = r.text.get(cut..).unwrap_or_default();
            runs.push(TextRun::new(text, r.style.clone()));
        }
        pos = end;
    }
    let base = base.unwrap_or(spec.runs.len());
    (ParagraphSpec { runs, props, width }, base)
}

/// Something text flows around, in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Obstacle {
    /// The outer rectangle (the object plus its distances from text).
    pub rect: Bounds,
    /// For tight and through wrapping: the object's horizontal extent per band (`y0, y1, x0,
    /// x1`), already widened by the distances. `None` uses the rectangle.
    pub profile: Option<Vec<[f32; 4]>>,
    /// Which sides text may use.
    pub side: WrapSide,
    /// Top and bottom: no text beside the object at all.
    pub top_bottom: bool,
}

/// A blocked horizontal range in one band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blocked {
    /// Start x.
    pub x0: f32,
    /// End x.
    pub x1: f32,
    /// The whole width is blocked.
    pub full: bool,
}

impl Obstacle {
    /// The blocked range of this obstacle in the band `y0..y1`, within the area `left..right`.
    #[must_use]
    pub fn blocked(&self, y0: f32, y1: f32, left: f32, right: f32) -> Option<Blocked> {
        if y1 <= self.rect.y || y0 >= self.rect.bottom() || self.rect.w <= 0.0 {
            return None;
        }
        if self.rect.right() <= left || self.rect.x >= right {
            return None;
        }
        if self.top_bottom {
            return Some(Blocked {
                x0: left,
                x1: right,
                full: true,
            });
        }
        let (mut x0, mut x1) = match &self.profile {
            Some(bands) => {
                let mut r: Option<(f32, f32)> = None;
                for b in bands.iter().filter(|b| b[1] > y0 && b[0] < y1) {
                    r = Some(match r {
                        Some((a, c)) => (a.min(b[2]), c.max(b[3])),
                        None => (b[2], b[3]),
                    });
                }
                r?
            }
            None => (self.rect.x, self.rect.right()),
        };
        match self.side {
            WrapSide::Left => x1 = right,
            WrapSide::Right => x0 = left,
            WrapSide::Largest => {
                if x0 - left >= right - x1 {
                    x1 = right;
                } else {
                    x0 = left;
                }
            }
            _ => {}
        }
        Some(Blocked {
            x0: x0.max(left),
            x1: x1.min(right),
            full: false,
        })
    }
}

/// Blocked ranges of a band.
#[must_use]
pub fn blocked_in(obstacles: &[Obstacle], band: (f32, f32), left: f32, right: f32) -> Vec<Blocked> {
    obstacles
        .iter()
        .filter_map(|o| o.blocked(band.0, band.1, left, right))
        .collect()
}

/// The y below which the band's obstacles end (where text continues after a blocked band).
#[must_use]
pub fn next_y(obstacles: &[Obstacle], band: (f32, f32), left: f32, right: f32) -> f32 {
    let mut y = f32::INFINITY;
    for o in obstacles {
        if o.blocked(band.0, band.1, left, right).is_some() {
            // Profiles end band by band; rectangles at their bottom.
            let end = match &o.profile {
                Some(bands) if !o.top_bottom => bands
                    .iter()
                    .filter(|b| b[1] > band.0 && b[0] < band.1)
                    .map(|b| b[1])
                    .fold(f32::INFINITY, f32::min),
                _ => o.rect.bottom(),
            };
            y = y.min(end.max(band.0 + 1.0));
        }
    }
    if y.is_finite() { y } else { band.1 }
}

/// Free intervals (x, width) of `left..right` outside `blocked`, at least [`MIN_GAP`] wide.
#[must_use]
pub fn free_intervals(blocked: &[Blocked], left: f32, right: f32) -> Vec<(f32, f32)> {
    let mut b: Vec<(f32, f32)> = blocked.iter().map(|b| (b.x0, b.x1)).collect();
    b.sort_by(|a, c| a.0.total_cmp(&c.0));
    let mut out = Vec::new();
    let mut x = left;
    for (a, c) in b {
        if a > x && a - x >= MIN_GAP {
            out.push((x, a - x));
        }
        x = x.max(c);
    }
    if right - x >= MIN_GAP {
        out.push((x, right - x));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_obstacle(x: f32, y: f32, w: f32, h: f32, side: WrapSide) -> Obstacle {
        Obstacle {
            rect: Bounds::new(x, y, w, h),
            profile: None,
            side,
            top_bottom: false,
        }
    }

    #[test]
    fn intervals_around_obstacles() {
        let o = rect_obstacle(100.0, 0.0, 100.0, 50.0, WrapSide::BothSides);
        let b = blocked_in(std::slice::from_ref(&o), (10.0, 20.0), 0.0, 400.0);
        assert_eq!(
            free_intervals(&b, 0.0, 400.0),
            vec![(0.0, 100.0), (200.0, 200.0)]
        );
        assert!(blocked_in(std::slice::from_ref(&o), (50.0, 60.0), 0.0, 400.0).is_empty());
        let left = rect_obstacle(100.0, 0.0, 100.0, 50.0, WrapSide::Left);
        let b = blocked_in(&[left], (10.0, 20.0), 0.0, 400.0);
        assert_eq!(free_intervals(&b, 0.0, 400.0), vec![(0.0, 100.0)]);
        let largest = rect_obstacle(100.0, 0.0, 100.0, 50.0, WrapSide::Largest);
        let b = blocked_in(&[largest], (10.0, 20.0), 0.0, 400.0);
        assert_eq!(free_intervals(&b, 0.0, 400.0), vec![(200.0, 200.0)]);
        // Gaps narrower than the minimum stay empty.
        let edge = rect_obstacle(10.0, 0.0, 100.0, 50.0, WrapSide::BothSides);
        let b = blocked_in(&[edge], (10.0, 20.0), 0.0, 400.0);
        assert_eq!(free_intervals(&b, 0.0, 400.0), vec![(110.0, 290.0)]);
    }

    #[test]
    fn profiles_follow_the_shape_band_by_band() {
        let o = Obstacle {
            rect: Bounds::new(100.0, 0.0, 200.0, 40.0),
            profile: Some(vec![[0.0, 20.0, 180.0, 220.0], [20.0, 40.0, 100.0, 300.0]]),
            side: WrapSide::BothSides,
            top_bottom: false,
        };
        let b = o.blocked(0.0, 10.0, 0.0, 400.0).unwrap();
        assert_eq!((b.x0, b.x1), (180.0, 220.0));
        let b = o.blocked(15.0, 25.0, 0.0, 400.0).unwrap();
        assert_eq!((b.x0, b.x1), (100.0, 300.0));
        assert_eq!(next_y(&[o], (0.0, 10.0), 0.0, 400.0), 20.0);
    }

    #[test]
    fn top_and_bottom_blocks_everything() {
        let o = Obstacle {
            top_bottom: true,
            ..rect_obstacle(100.0, 0.0, 50.0, 30.0, WrapSide::BothSides)
        };
        let b = o.blocked(5.0, 10.0, 0.0, 400.0).unwrap();
        assert!(b.full);
        assert_eq!(next_y(&[o], (5.0, 10.0), 0.0, 400.0), 30.0);
    }
}
