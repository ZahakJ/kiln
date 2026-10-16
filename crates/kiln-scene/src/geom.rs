// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scene geometry: axis-aligned bounds in document CSS pixels (`f32`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An axis-aligned rectangle in document CSS pixels at zoom 1 (origin top-left, y down).
///
/// Named `Bounds` (not `Rect`) so it does not collide with `kiln_protocol::geom::Rect` in the
/// generated TypeScript.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Bounds {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (never negative for bounds produced by [`Bounds::new`]).
    pub w: f32,
    /// Height (never negative for bounds produced by [`Bounds::new`]).
    pub h: f32,
}

impl Bounds {
    /// The empty rectangle at the origin.
    pub const ZERO: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    /// Bounds from origin and size; a negative size is clamped to zero.
    #[must_use]
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
        }
    }

    /// Bounds from two corners in any order.
    #[must_use]
    pub fn from_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
    }

    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether the area is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.w > 0.0 && self.h > 0.0)
    }

    /// Whether every number is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.w.is_finite() && self.h.is_finite()
    }

    /// Whether the two rectangles overlap with a positive area.
    #[must_use]
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Whether the point lies inside (left/top inclusive, right/bottom exclusive).
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The overlap of two rectangles, if any.
    #[must_use]
    pub fn intersection(&self, other: &Bounds) -> Option<Bounds> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        (x1 > x0 && y1 > y0).then(|| Bounds::new(x0, y0, x1 - x0, y1 - y0))
    }

    /// The smallest rectangle containing both.
    #[must_use]
    pub fn union(&self, other: &Bounds) -> Bounds {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Bounds::from_corners(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    /// Grown by `dx` on the left and right and `dy` on the top and bottom (shrunk when negative).
    #[must_use]
    pub fn inflate(&self, dx: f32, dy: f32) -> Bounds {
        Bounds::new(
            self.x - dx,
            self.y - dy,
            self.w + 2.0 * dx,
            self.h + 2.0 * dy,
        )
    }

    /// Moved by `(dx, dy)`.
    #[must_use]
    pub fn translate(&self, dx: f32, dy: f32) -> Bounds {
        Bounds {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }

    /// Distance from a point to the rectangle (0 inside).
    #[must_use]
    pub fn distance_to(&self, x: f32, y: f32) -> f32 {
        let dx = (self.x - x).max(0.0).max(x - self.right());
        let dy = (self.y - y).max(0.0).max(y - self.bottom());
        dx.hypot(dy)
    }
}

// Narrowing casts are the point of this conversion: scene geometry is f32.
#[allow(clippy::cast_possible_truncation)]
impl From<kurbo::Rect> for Bounds {
    fn from(r: kurbo::Rect) -> Self {
        let r = r.abs();
        Bounds::new(
            r.x0 as f32,
            r.y0 as f32,
            r.width() as f32,
            r.height() as f32,
        )
    }
}

impl From<Bounds> for kurbo::Rect {
    fn from(b: Bounds) -> Self {
        kurbo::Rect::new(
            f64::from(b.x),
            f64::from(b.y),
            f64::from(b.right()),
            f64::from(b.bottom()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_union_and_containment() {
        let a = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let b = Bounds::new(5.0, 5.0, 10.0, 10.0);
        let c = Bounds::new(10.0, 0.0, 5.0, 5.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c), "touching edges do not intersect");
        assert_eq!(a.intersection(&b), Some(Bounds::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.intersection(&c), None);
        assert_eq!(a.union(&b), Bounds::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(Bounds::ZERO.union(&b), b);
        assert!(a.contains(0.0, 0.0) && !a.contains(10.0, 5.0));
        assert_eq!(a.inflate(2.0, 1.0), Bounds::new(-2.0, -1.0, 14.0, 12.0));
        assert_eq!(a.translate(3.0, -1.0), Bounds::new(3.0, -1.0, 10.0, 10.0));
        assert!((a.distance_to(13.0, 14.0) - 5.0).abs() < 1e-6);
        assert_eq!(a.distance_to(4.0, 4.0), 0.0);
    }

    #[test]
    fn constructors_normalise() {
        assert_eq!(Bounds::new(1.0, 1.0, -3.0, 2.0).w, 0.0);
        assert!(Bounds::new(1.0, 1.0, -3.0, 2.0).is_empty());
        assert_eq!(
            Bounds::from_corners(4.0, 6.0, 1.0, 2.0),
            Bounds::new(1.0, 2.0, 3.0, 4.0)
        );
        let k = kurbo::Rect::new(10.0, 20.0, 4.0, 5.0);
        assert_eq!(Bounds::from(k), Bounds::new(4.0, 5.0, 6.0, 15.0));
        assert_eq!(
            kurbo::Rect::from(Bounds::new(4.0, 5.0, 6.0, 15.0)),
            kurbo::Rect::new(4.0, 5.0, 10.0, 20.0)
        );
        assert!(!Bounds::new(f32::NAN, 0.0, 1.0, 1.0).is_finite());
    }
}
