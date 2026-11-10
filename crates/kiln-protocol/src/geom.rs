// SPDX-License-Identifier: AGPL-3.0-or-later
//! Geometry on the wire: CSS pixels as `f32`, relative to the document surface.
//!
//! The core computes in `kurbo` (`f64`, re-exported by `kiln-common`); these mirrors are what the
//! painter and overlays receive. `f32` halves the payload and is exact to 1/1024 px up to 16,384 px.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A point in CSS pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
}

/// A size in CSS pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Size {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// An axis-aligned rectangle in CSS pixels (origin top-left).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (never negative when produced by the core).
    pub width: f32,
    /// Height (never negative when produced by the core).
    pub height: f32,
}

impl Point {
    /// A point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Rect {
    /// A rectangle from origin and size.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

// The narrowing `as f32` casts are the purpose of these conversions: wire geometry is f32.
#[allow(clippy::cast_possible_truncation)]
impl From<kiln_common::Point> for Point {
    fn from(p: kiln_common::Point) -> Self {
        Self::new(p.x as f32, p.y as f32)
    }
}

impl From<Point> for kiln_common::Point {
    fn from(p: Point) -> Self {
        kiln_common::Point::new(f64::from(p.x), f64::from(p.y))
    }
}

#[allow(clippy::cast_possible_truncation)]
impl From<kiln_common::Rect> for Rect {
    fn from(r: kiln_common::Rect) -> Self {
        let r = r.abs();
        Self::new(
            r.x0 as f32,
            r.y0 as f32,
            r.width() as f32,
            r.height() as f32,
        )
    }
}

impl From<Rect> for kiln_common::Rect {
    fn from(r: Rect) -> Self {
        kiln_common::Rect::new(
            f64::from(r.x),
            f64::from(r.y),
            f64::from(r.x) + f64::from(r.width),
            f64::from(r.y) + f64::from(r.height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_and_from_kurbo() {
        let k = kiln_common::Rect::new(10.0, 20.0, 4.0, 5.0); // inverted x
        let r = Rect::from(k);
        assert_eq!(r, Rect::new(4.0, 5.0, 6.0, 15.0));
        assert_eq!(
            kiln_common::Rect::from(r),
            kiln_common::Rect::new(4.0, 5.0, 10.0, 20.0)
        );
        let p = Point::from(kiln_common::Point::new(1.5, -2.0));
        assert_eq!(p, Point::new(1.5, -2.0));
        assert_eq!(
            kiln_common::Point::from(p),
            kiln_common::Point::new(1.5, -2.0)
        );
    }
}
