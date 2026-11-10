// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ink on the show (the pen and the highlighter), the eraser and the laser pointer, drawn over
//! the slide as Scene ops in slide px.

use kiln_scene::{DrawOp, FillRule, LineCap, LineJoin, Paint, PathData, Stroke};

/// PowerPoint's pen: red, 2.25 pt.
const PEN: u32 = 0xff00_00ff;
const PEN_WIDTH: f32 = 3.0;
/// The highlighter: yellow at half strength, 12 px wide with square ends.
const HIGHLIGHTER: u32 = 0xffff_0080;
const HIGHLIGHTER_WIDTH: f32 = 14.0;
/// How near the eraser has to come to a stroke, slide px.
const ERASER_REACH: f32 = 10.0;

/// A stroke of ink.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InkStroke {
    pub highlighter: bool,
    pub points: Vec<(f32, f32)>,
}

impl InkStroke {
    pub(crate) fn new(highlighter: bool, at: (f32, f32)) -> Self {
        Self {
            highlighter,
            points: vec![at],
        }
    }

    /// Adds a point unless it is next to the last one.
    pub(crate) fn push(&mut self, p: (f32, f32)) {
        if let Some(last) = self.points.last()
            && (last.0 - p.0).hypot(last.1 - p.1) < 0.75
        {
            return;
        }
        self.points.push(p);
    }

    /// Whether a point is within `reach` of the stroke.
    pub(crate) fn near(&self, x: f32, y: f32, reach: f32) -> bool {
        if self.points.len() == 1 {
            let (a, b) = self.points[0];
            return (a - x).hypot(b - y) <= reach;
        }
        self.points.windows(2).any(|w| {
            let (ax, ay) = w[0];
            let (bx, by) = w[1];
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let t = if len2 <= f32::EPSILON {
                0.0
            } else {
                (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0)
            };
            let (px, py) = (ax + t * dx, ay + t * dy);
            (px - x).hypot(py - y) <= reach
        })
    }

    fn op(&self) -> DrawOp {
        let mut path = PathData::new();
        for (i, (x, y)) in self.points.iter().enumerate() {
            if i == 0 {
                path.move_to(*x, *y);
            } else {
                path.line_to(*x, *y);
            }
        }
        if self.points.len() == 1 {
            // A dot.
            let (x, y) = self.points[0];
            path.line_to(x + 0.01, y);
        }
        let (paint, width, cap) = if self.highlighter {
            (HIGHLIGHTER, HIGHLIGHTER_WIDTH, LineCap::Square)
        } else {
            (PEN, PEN_WIDTH, LineCap::Round)
        };
        DrawOp::Path {
            path,
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Rgba(paint),
                width,
                join: LineJoin::Round,
                cap,
                dash: Vec::new(),
            }),
            rule: FillRule::NonZero,
        }
    }
}

/// Removes the strokes the eraser touches at a point; whether any went.
pub(crate) fn erase_at(strokes: &mut Vec<InkStroke>, x: f32, y: f32) -> bool {
    let before = strokes.len();
    strokes.retain(|s| !s.near(x, y, ERASER_REACH));
    strokes.len() != before
}

fn disc(x: f32, y: f32, r: f32) -> PathData {
    kiln_scene::shapes::rounded_rect(kiln_scene::Bounds::new(x - r, y - r, r * 2.0, r * 2.0), r)
}

/// The ops of the ink and the laser pointer (highlighter strokes under the pen's).
pub(crate) fn ops(strokes: &[InkStroke], laser: Option<(f32, f32)>) -> Vec<DrawOp> {
    let mut out: Vec<DrawOp> = strokes
        .iter()
        .filter(|s| s.highlighter)
        .chain(strokes.iter().filter(|s| !s.highlighter))
        .map(InkStroke::op)
        .collect();
    if let Some((x, y)) = laser {
        // A red dot with a soft glow, as PowerPoint's laser.
        for (r, alpha) in [(14.0, 0x30), (10.0, 0x60), (7.0, 0xc0)] {
            out.push(DrawOp::Path {
                path: disc(x, y, r),
                fill: Some(Paint::Rgba(0xff10_1000 | alpha)),
                stroke: None,
                rule: FillRule::NonZero,
            });
        }
        out.push(DrawOp::Path {
            path: disc(x, y, 4.5),
            fill: Some(Paint::Rgba(0xff4a_3aff)),
            stroke: None,
            rule: FillRule::NonZero,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_draw_and_erase() {
        let mut s = InkStroke::new(false, (10.0, 10.0));
        s.push((10.2, 10.1));
        s.push((50.0, 10.0));
        assert_eq!(s.points.len(), 2);
        assert!(s.near(30.0, 14.0, 5.0));
        assert!(!s.near(30.0, 40.0, 5.0));
        let h = InkStroke::new(true, (100.0, 100.0));
        let mut all = vec![s, h];
        let drawn = ops(&all, Some((5.0, 5.0)));
        // Highlighter first, then the pen, then four discs of the laser.
        assert_eq!(drawn.len(), 6);
        assert!(matches!(&drawn[0], DrawOp::Path { stroke: Some(st), .. } if st.width > 10.0));
        assert!(erase_at(&mut all, 30.0, 12.0));
        assert_eq!(all.len(), 1);
        assert!(!erase_at(&mut all, 500.0, 500.0));
    }
}
