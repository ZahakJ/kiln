// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paths on the wire: a verb list and a flat coordinate list, built from `kurbo` paths.
//!
//! The flat form decodes into two plain arrays in TypeScript and maps one to one onto `Path2D`
//! (`moveTo`, `lineTo`, `quadraticCurveTo`, `bezierCurveTo`, `closePath`) and onto tiny-skia's
//! `PathBuilder`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Bounds;
use crate::paint::Paint;

/// Verb codes of [`PathData::verbs`].
pub mod verb {
    /// Move to (1 point).
    pub const MOVE: u8 = 0;
    /// Line to (1 point).
    pub const LINE: u8 = 1;
    /// Quadratic Bézier (control point, end point).
    pub const QUAD: u8 = 2;
    /// Cubic Bézier (two control points, end point).
    pub const CUBIC: u8 = 3;
    /// Close the subpath (no points).
    pub const CLOSE: u8 = 4;

    /// Number of coordinates (x and y count separately) a verb consumes.
    #[must_use]
    pub const fn arity(v: u8) -> usize {
        match v {
            MOVE | LINE => 2,
            QUAD => 4,
            CUBIC => 6,
            _ => 0,
        }
    }
}

/// A path in document CSS pixels (or tile-local pixels inside a tile).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct PathData {
    /// Verbs (see [`verb`]).
    pub verbs: Vec<u8>,
    /// Coordinates, `x, y` pairs, consumed by the verbs in order.
    pub coords: Vec<f32>,
}

/// One segment of a [`PathData`], as iterated by [`PathData::segments`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// Move to.
    Move([f32; 2]),
    /// Line to.
    Line([f32; 2]),
    /// Quadratic Bézier: control, end.
    Quad([f32; 4]),
    /// Cubic Bézier: control 1, control 2, end.
    Cubic([f32; 6]),
    /// Close.
    Close,
}

impl PathData {
    /// An empty path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a subpath.
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.verbs.push(verb::MOVE);
        self.coords.extend([x, y]);
        self
    }

    /// Adds a line.
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.verbs.push(verb::LINE);
        self.coords.extend([x, y]);
        self
    }

    /// Adds a quadratic curve.
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self {
        self.verbs.push(verb::QUAD);
        self.coords.extend([cx, cy, x, y]);
        self
    }

    /// Adds a cubic curve.
    pub fn cubic_to(
        &mut self,
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    ) -> &mut Self {
        self.verbs.push(verb::CUBIC);
        self.coords.extend([c1x, c1y, c2x, c2y, x, y]);
        self
    }

    /// Closes the subpath.
    pub fn close(&mut self) -> &mut Self {
        self.verbs.push(verb::CLOSE);
        self
    }

    /// A rectangle.
    #[must_use]
    pub fn rect(b: Bounds) -> Self {
        let mut p = Self::new();
        p.move_to(b.x, b.y)
            .line_to(b.right(), b.y)
            .line_to(b.right(), b.bottom())
            .line_to(b.x, b.bottom())
            .close();
        p
    }

    /// Whether the path has no drawing segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verbs
            .iter()
            .all(|v| *v == verb::MOVE || *v == verb::CLOSE)
    }

    /// Whether the verbs and coordinates agree and every coordinate is finite. Painters skip
    /// paths that fail this check instead of trusting their input.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let mut needed = 0usize;
        for v in &self.verbs {
            if *v > verb::CLOSE {
                return false;
            }
            needed += verb::arity(*v);
        }
        needed == self.coords.len() && self.coords.iter().all(|c| c.is_finite())
    }

    /// The segments, or nothing when the path is not [valid](Self::is_valid).
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        let valid = self.is_valid();
        let mut at = 0usize;
        self.verbs.iter().filter(move |_| valid).map(move |v| {
            let c = &self.coords;
            let seg = match *v {
                verb::MOVE => Segment::Move([c[at], c[at + 1]]),
                verb::LINE => Segment::Line([c[at], c[at + 1]]),
                verb::QUAD => Segment::Quad([c[at], c[at + 1], c[at + 2], c[at + 3]]),
                verb::CUBIC => {
                    Segment::Cubic([c[at], c[at + 1], c[at + 2], c[at + 3], c[at + 4], c[at + 5]])
                }
                _ => Segment::Close,
            };
            at += verb::arity(*v);
            seg
        })
    }

    /// The control-point bounding box (contains the path).
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        if !self.is_valid() || self.coords.is_empty() {
            return Bounds::ZERO;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in self.coords.as_chunks::<2>().0 {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            y0 = y0.min(p[1]);
            y1 = y1.max(p[1]);
        }
        Bounds::from_corners(x0, y0, x1, y1)
    }

    /// Appends another path.
    pub fn extend(&mut self, other: &PathData) {
        self.verbs.extend_from_slice(&other.verbs);
        self.coords.extend_from_slice(&other.coords);
    }

    /// Moves every point by `(dx, dy)`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        for p in self.coords.as_chunks_mut::<2>().0 {
            p[0] += dx;
            p[1] += dy;
        }
    }

    /// Scales every point by `s` around the origin.
    pub fn scale(&mut self, s: f32) {
        for c in &mut self.coords {
            *c *= s;
        }
    }
}

// Narrowing to f32 is the purpose of the conversion.
#[allow(clippy::cast_possible_truncation)]
impl From<&kurbo::BezPath> for PathData {
    fn from(path: &kurbo::BezPath) -> Self {
        use kurbo::PathEl;
        let mut p = PathData::new();
        let f = |v: f64| v as f32;
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(a) => {
                    p.move_to(f(a.x), f(a.y));
                }
                PathEl::LineTo(a) => {
                    p.line_to(f(a.x), f(a.y));
                }
                PathEl::QuadTo(c, a) => {
                    p.quad_to(f(c.x), f(c.y), f(a.x), f(a.y));
                }
                PathEl::CurveTo(c1, c2, a) => {
                    p.cubic_to(f(c1.x), f(c1.y), f(c2.x), f(c2.y), f(a.x), f(a.y));
                }
                PathEl::ClosePath => {
                    p.close();
                }
            }
        }
        p
    }
}

impl From<&PathData> for kurbo::BezPath {
    fn from(p: &PathData) -> Self {
        let mut out = kurbo::BezPath::new();
        let pt = |x: f32, y: f32| kurbo::Point::new(f64::from(x), f64::from(y));
        for s in p.segments() {
            match s {
                Segment::Move([x, y]) => out.move_to(pt(x, y)),
                Segment::Line([x, y]) => out.line_to(pt(x, y)),
                Segment::Quad([cx, cy, x, y]) => out.quad_to(pt(cx, cy), pt(x, y)),
                Segment::Cubic([a, b, c, d, x, y]) => out.curve_to(pt(a, b), pt(c, d), pt(x, y)),
                Segment::Close => out.close_path(),
            }
        }
        out
    }
}

/// How a path's interior is decided.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum FillRule {
    /// Non-zero winding (glyphs, most shapes).
    #[default]
    NonZero,
    /// Even-odd (DrawingML `evenOdd` holes).
    EvenOdd,
}

/// Line joins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum LineJoin {
    /// Mitred corners.
    #[default]
    Miter,
    /// Rounded corners.
    Round,
    /// Bevelled corners.
    Bevel,
}

/// Line caps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum LineCap {
    /// Flat at the end point.
    #[default]
    Butt,
    /// Rounded.
    Round,
    /// Square, extending past the end point.
    Square,
}

/// A stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Stroke {
    /// Colour.
    pub paint: Paint,
    /// Width in CSS pixels.
    pub width: f32,
    /// Join.
    pub join: LineJoin,
    /// Cap.
    pub cap: LineCap,
    /// Dash pattern (lengths, alternately on and off); empty for a solid line.
    pub dash: Vec<f32>,
}

impl Stroke {
    /// A solid stroke with mitred joins and butt caps.
    #[must_use]
    pub fn solid(paint: Paint, width: f32) -> Self {
        Self {
            paint,
            width,
            join: LineJoin::Miter,
            cap: LineCap::Butt,
            dash: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_iterates_and_bounds() {
        let mut p = PathData::new();
        p.move_to(1.0, 2.0)
            .line_to(5.0, 2.0)
            .quad_to(6.0, 3.0, 5.0, 6.0)
            .cubic_to(4.0, 7.0, 2.0, 7.0, 1.0, 6.0)
            .close();
        assert!(p.is_valid());
        assert!(!p.is_empty());
        let segs: Vec<Segment> = p.segments().collect();
        assert_eq!(segs.len(), 5);
        assert_eq!(segs[2], Segment::Quad([6.0, 3.0, 5.0, 6.0]));
        assert_eq!(p.bounds(), Bounds::new(1.0, 2.0, 5.0, 5.0));
        p.translate(1.0, 1.0);
        assert_eq!(p.bounds(), Bounds::new(2.0, 3.0, 5.0, 5.0));
    }

    #[test]
    fn rejects_malformed_paths() {
        let bad = PathData {
            verbs: vec![verb::MOVE, verb::CUBIC],
            coords: vec![0.0, 0.0, 1.0],
        };
        assert!(!bad.is_valid());
        assert_eq!(bad.segments().count(), 0);
        assert_eq!(bad.bounds(), Bounds::ZERO);
        let nan = PathData {
            verbs: vec![verb::MOVE],
            coords: vec![f32::NAN, 0.0],
        };
        assert!(!nan.is_valid());
        let unknown = PathData {
            verbs: vec![9],
            coords: vec![],
        };
        assert!(!unknown.is_valid());
    }

    #[test]
    fn round_trips_through_kurbo() {
        let mut k = kurbo::BezPath::new();
        k.move_to((0.0, 0.0));
        k.line_to((10.0, 0.0));
        k.quad_to((12.0, 5.0), (10.0, 10.0));
        k.curve_to((8.0, 12.0), (2.0, 12.0), (0.0, 10.0));
        k.close_path();
        let p = PathData::from(&k);
        assert_eq!(p.verbs, vec![0, 1, 2, 3, 4]);
        assert_eq!(kurbo::BezPath::from(&p), k);
        assert_eq!(
            PathData::rect(Bounds::new(1.0, 1.0, 2.0, 2.0)).verbs.len(),
            5
        );
    }
}
