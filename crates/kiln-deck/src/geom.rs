// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape geometry in EMU (914 400 per inch, 12 700 per point): transforms, rotated bounds and the
//! child coordinate space of groups (`a:xfrm` with `a:chOff`/`a:chExt`).

use serde::{Deserialize, Serialize};

/// EMU per point.
pub const EMU_PER_PT: i64 = 12_700;
/// EMU per inch.
pub const EMU_PER_INCH: i64 = 914_400;
/// Rotation units per degree (`ST_Angle`: 60 000ths of a degree).
pub const ROT_PER_DEGREE: i32 = 60_000;
/// A full turn in rotation units.
pub const FULL_TURN: i32 = 360 * ROT_PER_DEGREE;

/// An axis-aligned rectangle in EMU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: i64,
    /// Top edge.
    pub y: i64,
    /// Width (never negative).
    pub cx: i64,
    /// Height (never negative).
    pub cy: i64,
}

impl Rect {
    /// A rectangle; negative sizes are clamped to 0.
    #[must_use]
    pub const fn new(x: i64, y: i64, cx: i64, cy: i64) -> Self {
        Self {
            x,
            y,
            cx: if cx < 0 { 0 } else { cx },
            cy: if cy < 0 { 0 } else { cy },
        }
    }

    /// The right edge.
    #[must_use]
    pub const fn right(&self) -> i64 {
        self.x.saturating_add(self.cx)
    }

    /// The bottom edge.
    #[must_use]
    pub const fn bottom(&self) -> i64 {
        self.y.saturating_add(self.cy)
    }

    /// The centre (rounded towards the top left).
    #[must_use]
    pub const fn center(&self) -> (i64, i64) {
        (self.x + self.cx / 2, self.y + self.cy / 2)
    }

    /// The smallest rectangle containing both.
    #[must_use]
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// True when the rectangles share interior area.
    #[must_use]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// A shape transform (`a:xfrm`): position and size in EMU, rotation clockwise in 60 000ths of a
/// degree about the centre, and flips applied before the rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Transform {
    /// Left edge of the unrotated box.
    pub x: i64,
    /// Top edge of the unrotated box.
    pub y: i64,
    /// Width.
    pub cx: i64,
    /// Height.
    pub cy: i64,
    /// Clockwise rotation, `0..FULL_TURN`.
    pub rot: i32,
    /// Mirrored horizontally.
    pub flip_h: bool,
    /// Mirrored vertically.
    pub flip_v: bool,
}

impl Transform {
    /// An unrotated transform from a rectangle.
    #[must_use]
    pub const fn from_rect(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            cx: r.cx,
            cy: r.cy,
            rot: 0,
            flip_h: false,
            flip_v: false,
        }
    }

    /// An unrotated transform.
    #[must_use]
    pub const fn new(x: i64, y: i64, cx: i64, cy: i64) -> Self {
        Self::from_rect(Rect::new(x, y, cx, cy))
    }

    /// The same transform with a rotation (normalised into `0..FULL_TURN`).
    #[must_use]
    pub const fn with_rotation(mut self, rot: i32) -> Self {
        self.rot = normalize_rotation(rot);
        self
    }

    /// The unrotated box.
    #[must_use]
    pub const fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.cx, self.cy)
    }

    /// The axis-aligned box around the rotated shape (what alignment, distribution, grouping
    /// and selection outlines use, as PowerPoint does).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        if self.rot == 0 || self.rot == FULL_TURN / 2 {
            return self.rect();
        }
        if self.rot == FULL_TURN / 4 || self.rot == 3 * FULL_TURN / 4 {
            let (cx, cy) = self.center_f();
            let (hw, hh) = (self.cy as f64 / 2.0, self.cx as f64 / 2.0);
            return rect_from_f(cx - hw, cy - hh, cx + hw, cy + hh);
        }
        let theta = rot_radians(self.rot);
        let (s, c) = theta.sin_cos();
        let (hw, hh) = (self.cx as f64 / 2.0, self.cy as f64 / 2.0);
        let ex = (hw * c).abs() + (hh * s).abs();
        let ey = (hw * s).abs() + (hh * c).abs();
        let (cx, cy) = self.center_f();
        rect_from_f(cx - ex, cy - ey, cx + ex, cy + ey)
    }

    /// The centre.
    #[must_use]
    pub fn center_f(&self) -> (f64, f64) {
        (
            self.x as f64 + self.cx as f64 / 2.0,
            self.y as f64 + self.cy as f64 / 2.0,
        )
    }

    /// Moves the shape so that its centre is at a point (size, rotation and flips are kept).
    #[must_use]
    pub fn with_center(mut self, cx: f64, cy: f64) -> Self {
        self.x = round_emu(cx - self.cx as f64 / 2.0);
        self.y = round_emu(cy - self.cy as f64 / 2.0);
        self
    }

    /// Moves the shape by an offset.
    #[must_use]
    pub const fn translated(mut self, dx: i64, dy: i64) -> Self {
        self.x = self.x.saturating_add(dx);
        self.y = self.y.saturating_add(dy);
        self
    }

    /// Maps a point from the shape's own (unrotated, unflipped) box to the parent space.
    #[must_use]
    pub fn map_point(&self, px: f64, py: f64) -> (f64, f64) {
        let (cx, cy) = self.center_f();
        let mut dx = px - cx;
        let mut dy = py - cy;
        if self.flip_h {
            dx = -dx;
        }
        if self.flip_v {
            dy = -dy;
        }
        let (s, c) = rot_radians(self.rot).sin_cos();
        (cx + dx * c - dy * s, cy + dx * s + dy * c)
    }
}

/// The child coordinate space of a group (`a:chOff`, `a:chExt`): children are positioned in
/// this space, which maps onto the group's own box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ChildSpace {
    /// `a:chOff/@x`.
    pub x: i64,
    /// `a:chOff/@y`.
    pub y: i64,
    /// `a:chExt/@cx`.
    pub cx: i64,
    /// `a:chExt/@cy`.
    pub cy: i64,
}

impl ChildSpace {
    /// The identity space for a group whose box is `r` (children keep their coordinates).
    #[must_use]
    pub const fn identity(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            cx: r.cx,
            cy: r.cy,
        }
    }

    /// Scale factors from child space to the group box (1 when an extent is 0).
    #[must_use]
    pub fn scale(&self, group: &Transform) -> (f64, f64) {
        let f = |g: i64, c: i64| if c == 0 { 1.0 } else { g as f64 / c as f64 };
        (f(group.cx, self.cx), f(group.cy, self.cy))
    }

    /// Maps a child transform into the group's parent space: the result is what the child looks
    /// like after ungrouping (PowerPoint's Ungroup).
    #[must_use]
    pub fn child_to_parent(&self, group: &Transform, child: &Transform) -> Transform {
        let (sx, sy) = self.scale(group);
        // Child centre in the group's unrotated box.
        let (ccx, ccy) = child.center_f();
        let bx = group.x as f64 + (ccx - self.x as f64) * sx;
        let by = group.y as f64 + (ccy - self.y as f64) * sy;
        let (px, py) = group.map_point(bx, by);
        // Size: scaled along the child's own axes. With a quarter-turn child the group's axes
        // swap relative to it.
        let quarter = (child.rot / (FULL_TURN / 4)) % 2 == 1 && child.rot % (FULL_TURN / 4) == 0;
        let (kx, ky) = if quarter { (sy, sx) } else { (sx, sy) };
        let cx = round_emu(child.cx as f64 * kx);
        let cy = round_emu(child.cy as f64 * ky);
        let mut rot = child.rot;
        let flip_h = child.flip_h ^ group.flip_h;
        let flip_v = child.flip_v ^ group.flip_v;
        // A single mirror reverses the sense of the child's rotation.
        if group.flip_h ^ group.flip_v {
            rot = -rot;
        }
        rot = rot.saturating_add(group.rot);
        Transform {
            x: 0,
            y: 0,
            cx,
            cy,
            rot: normalize_rotation(rot),
            flip_h,
            flip_v,
        }
        .with_center(px, py)
    }
}

/// Normalises a rotation into `0..FULL_TURN`.
#[must_use]
pub const fn normalize_rotation(rot: i32) -> i32 {
    rot.rem_euclid(FULL_TURN)
}

fn rot_radians(rot: i32) -> f64 {
    f64::from(rot) / f64::from(ROT_PER_DEGREE) * std::f64::consts::PI / 180.0
}

/// Rounds a coordinate to whole EMU, saturating at the `i64` range (NaN becomes 0).
#[must_use]
pub fn round_emu(v: f64) -> i64 {
    if v.is_nan() {
        return 0;
    }
    // `as` saturates for out-of-range floats.
    #[allow(clippy::cast_possible_truncation)]
    let r = v.round() as i64;
    r
}

fn rect_from_f(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    let x = round_emu(x0);
    let y = round_emu(y0);
    Rect::new(x, y, round_emu(x1) - x, round_emu(y1) - y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_helpers() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));
        assert!(a.intersects(&b));
        assert!(!a.intersects(&Rect::new(10, 0, 5, 5)));
        assert_eq!(Rect::new(1, 2, -3, 4).cx, 0);
        assert_eq!(b.center(), (10, 10));
    }

    #[test]
    fn rotated_bounds() {
        let t = Transform::new(0, 0, 200, 100);
        assert_eq!(t.bounds(), t.rect());
        let q = t.with_rotation(90 * ROT_PER_DEGREE);
        assert_eq!(q.bounds(), Rect::new(50, -50, 100, 200));
        let d = t.with_rotation(45 * ROT_PER_DEGREE);
        let b = d.bounds();
        // (200 + 100) / √2 ≈ 212.13 on both axes, centred on (100, 50).
        assert_eq!((b.cx, b.cy), (212, 212));
        assert_eq!(b.center(), (100, 50));
        assert_eq!(
            Transform::new(0, 0, 1, 1)
                .with_rotation(-ROT_PER_DEGREE)
                .rot,
            FULL_TURN - ROT_PER_DEGREE
        );
    }

    #[test]
    fn identity_child_space_keeps_children_in_place() {
        let g = Transform::new(100, 100, 400, 200);
        let space = ChildSpace::identity(g.rect());
        let child = Transform::new(150, 120, 50, 40).with_rotation(30 * ROT_PER_DEGREE);
        assert_eq!(space.child_to_parent(&g, &child), child);
    }

    #[test]
    fn scaled_and_rotated_groups_map_children() {
        // The group shows a 100×100 child space at twice the size.
        let g = Transform::new(0, 0, 200, 200);
        let space = ChildSpace {
            x: 0,
            y: 0,
            cx: 100,
            cy: 100,
        };
        let child = Transform::new(0, 0, 50, 50);
        assert_eq!(
            space.child_to_parent(&g, &child),
            Transform::new(0, 0, 100, 100)
        );
        // Rotating the group 180° moves the top-left child to the bottom right and turns it.
        let g = g.with_rotation(180 * ROT_PER_DEGREE);
        let out = space.child_to_parent(&g, &child);
        assert_eq!(out.rect(), Rect::new(100, 100, 100, 100));
        assert_eq!(out.rot, 180 * ROT_PER_DEGREE);
        // A mirrored group mirrors the child.
        let mut g = Transform::new(0, 0, 100, 100);
        g.flip_h = true;
        let space = ChildSpace::identity(g.rect());
        let out = space.child_to_parent(
            &g,
            &Transform::new(0, 0, 10, 10).with_rotation(ROT_PER_DEGREE),
        );
        assert_eq!(out.rect(), Rect::new(90, 0, 10, 10));
        assert!(out.flip_h);
        assert_eq!(out.rot, FULL_TURN - ROT_PER_DEGREE);
    }

    #[test]
    fn round_emu_saturates() {
        assert_eq!(round_emu(f64::NAN), 0);
        assert_eq!(round_emu(1e30), i64::MAX);
        assert_eq!(round_emu(-2.5), -3);
    }
}
