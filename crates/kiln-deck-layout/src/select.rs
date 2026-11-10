// SPDX-License-Identifier: AGPL-3.0-or-later
//! Selection and transform geometry: the eight sizing handles and the rotation handle of a
//! shape or a multiple selection's bounds, which grip a point is on, and the new transform of a
//! move, resize or rotation with PowerPoint's modifiers (Shift keeps proportions, constrains a
//! move to an axis and turns in 15° steps; Ctrl resizes from the centre; Alt turns snapping off).
//! Smart guides and snapping are in [`crate::guides`].

use kiln_deck::Transform;
use kiln_scene::Bounds;
use kurbo::Affine;

use crate::geom::{degrees, emu, rect_px, shape_matrix};
use crate::render::invert;

/// A sizing handle, clockwise from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    /// Top-left.
    NW,
    /// Top.
    N,
    /// Top-right.
    NE,
    /// Right.
    E,
    /// Bottom-right.
    SE,
    /// Bottom.
    S,
    /// Bottom-left.
    SW,
    /// Left.
    W,
}

impl Handle {
    /// Every handle, clockwise from the top-left.
    pub const ALL: [Handle; 8] = [
        Handle::NW,
        Handle::N,
        Handle::NE,
        Handle::E,
        Handle::SE,
        Handle::S,
        Handle::SW,
        Handle::W,
    ];

    /// The handle's position as fractions of the box.
    #[must_use]
    pub const fn fraction(self) -> (f32, f32) {
        match self {
            Handle::NW => (0.0, 0.0),
            Handle::N => (0.5, 0.0),
            Handle::NE => (1.0, 0.0),
            Handle::E => (1.0, 0.5),
            Handle::SE => (1.0, 1.0),
            Handle::S => (0.5, 1.0),
            Handle::SW => (0.0, 1.0),
            Handle::W => (0.0, 0.5),
        }
    }

    /// Which edges it moves: `(x side: -1 left, 0 none, 1 right; y side)`.
    #[must_use]
    pub const fn sides(self) -> (i8, i8) {
        match self {
            Handle::NW => (-1, -1),
            Handle::N => (0, -1),
            Handle::NE => (1, -1),
            Handle::E => (1, 0),
            Handle::SE => (1, 1),
            Handle::S => (0, 1),
            Handle::SW => (-1, 1),
            Handle::W => (-1, 0),
        }
    }
}

/// What a pointer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grip {
    /// The shape itself (a move).
    Body,
    /// A sizing handle.
    Size(Handle),
    /// The rotation handle.
    Rotate,
}

/// Distance of the rotation handle above the top edge, in screen px.
pub const ROTATE_OFFSET: f32 = 22.0;
/// Radius of a handle's hit area, in screen px.
pub const HANDLE_HIT: f32 = 7.0;

/// The handles of a transform in slide px: the eight sizing handles and the rotation handle.
/// `zoom` keeps the rotation handle a fixed screen distance away.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn handles(x: &Transform, zoom: f32) -> ([(f32, f32); 8], (f32, f32)) {
    let r = rect_px(x);
    let m = unflipped_matrix(x);
    let pt = |fx: f32, fy: f32| {
        let p = m * kurbo::Point::new(f64::from(fx * r.w), f64::from(fy * r.h));
        (p.x as f32, p.y as f32)
    };
    let mut out = [(0.0, 0.0); 8];
    for (i, h) in Handle::ALL.iter().enumerate() {
        let (fx, fy) = h.fraction();
        out[i] = pt(fx, fy);
    }
    let off = ROTATE_OFFSET / zoom.max(0.05);
    let p = m * kurbo::Point::new(f64::from(r.w / 2.0), f64::from(-off));
    (out, (p.x as f32, p.y as f32))
}

/// The matrix of a transform's box ignoring flips (handles stay where the user sees them).
#[must_use]
pub fn unflipped_matrix(x: &Transform) -> Affine {
    let mut t = *x;
    t.flip_h = false;
    t.flip_v = false;
    shape_matrix(&t)
}

/// The grip under a point (slide px), for a selected shape whose box is `x`. Small shapes keep
/// only the corner handles reachable, as PowerPoint does.
#[must_use]
pub fn grip_at(x: &Transform, px: f32, py: f32, zoom: f32, rotatable: bool) -> Option<Grip> {
    let (hs, rot) = handles(x, zoom);
    let tol = HANDLE_HIT / zoom.max(0.05);
    let d = |a: (f32, f32)| ((a.0 - px).powi(2) + (a.1 - py).powi(2)).sqrt();
    if rotatable && d(rot) <= tol {
        return Some(Grip::Rotate);
    }
    let r = rect_px(x);
    let small = r.w * zoom < 24.0 || r.h * zoom < 24.0;
    let mut best: Option<(f32, Handle)> = None;
    for (i, h) in Handle::ALL.iter().enumerate() {
        if small && matches!(h, Handle::N | Handle::S | Handle::E | Handle::W) {
            continue;
        }
        let dist = d(hs[i]);
        if dist <= tol && best.is_none_or(|b| dist < b.0) {
            best = Some((dist, *h));
        }
    }
    if let Some((_, h)) = best {
        return Some(Grip::Size(h));
    }
    let inv = invert(unflipped_matrix(x))?;
    let q = inv * kurbo::Point::new(f64::from(px), f64::from(py));
    let slop = 3.0 / zoom.max(0.05);
    #[allow(clippy::cast_possible_truncation)]
    let inside = Bounds::new(0.0, 0.0, r.w, r.h)
        .inflate(slop, slop)
        .contains(q.x as f32, q.y as f32);
    inside.then_some(Grip::Body)
}

/// Modifier keys of a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift: proportional resize, axis-constrained move, 15° rotation steps.
    pub shift: bool,
    /// Ctrl (or Cmd): resize from the centre.
    pub ctrl: bool,
    /// Alt: no snapping.
    pub alt: bool,
}

/// Smallest size a resize leaves, in px.
pub const MIN_SIDE: f32 = 1.0;

/// The transform after dragging handle `h` of `orig` by `(dx, dy)` slide px.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn resize(orig: &Transform, h: Handle, dx: f32, dy: f32, m: Modifiers) -> Transform {
    let r = rect_px(orig);
    // The drag in the shape's own (unrotated) frame.
    let theta = degrees(orig).to_radians();
    let (s, c) = theta.sin_cos();
    #[allow(clippy::cast_possible_truncation)]
    let (ldx, ldy) = (
        (f64::from(dx) * c + f64::from(dy) * s) as f32,
        (-f64::from(dx) * s + f64::from(dy) * c) as f32,
    );
    let (sx, sy) = h.sides();
    let mut w = r.w;
    let mut hgt = r.h;
    let k = if m.ctrl { 2.0 } else { 1.0 };
    if sx != 0 {
        w = r.w + f32::from(sx) * ldx * k;
    }
    if sy != 0 {
        hgt = r.h + f32::from(sy) * ldy * k;
    }
    let flip_x = w < 0.0;
    let flip_y = hgt < 0.0;
    let mut aw = w.abs().max(MIN_SIDE);
    let mut ah = hgt.abs().max(MIN_SIDE);
    if m.shift && r.w > 0.0 && r.h > 0.0 {
        let ratio = r.w / r.h;
        // Edge handles and the dominant side of a corner drag set the scale.
        if sx == 0 || (sy != 0 && aw / r.w <= ah / r.h) {
            aw = ah * ratio;
        } else {
            ah = aw / ratio;
        }
    }
    // The fixed point: the opposite handle (or the centre with Ctrl), in the local frame.
    let anchor_local = if m.ctrl {
        (r.w / 2.0, r.h / 2.0)
    } else {
        (
            match sx {
                -1 => r.w,
                1 => 0.0,
                _ => r.w / 2.0,
            },
            match sy {
                -1 => r.h,
                1 => 0.0,
                _ => r.h / 2.0,
            },
        )
    };
    let om = unflipped_matrix(orig);
    let a = om * kurbo::Point::new(f64::from(anchor_local.0), f64::from(anchor_local.1));
    // Where the anchor sits in the new box (local fractions), then place the new box so it
    // stays put.
    let fx = if m.ctrl {
        0.5
    } else {
        match (sx, flip_x) {
            (-1, false) | (1, true) => 1.0,
            (1, false) | (-1, true) => 0.0,
            _ => 0.5,
        }
    };
    let fy = if m.ctrl {
        0.5
    } else {
        match (sy, flip_y) {
            (-1, false) | (1, true) => 1.0,
            (1, false) | (-1, true) => 0.0,
            _ => 0.5,
        }
    };
    // Centre of the new box: anchor + rotation(offset from anchor to centre).
    let (ox, oy) = (f64::from(aw * (0.5 - fx)), f64::from(ah * (0.5 - fy)));
    let cx = a.x + ox * c - oy * s;
    let cy = a.y + ox * s + oy * c;
    let mut out = *orig;
    out.cx = emu(f64::from(aw));
    out.cy = emu(f64::from(ah));
    out = out.with_center(cx * crate::geom::EMU_PER_PX, cy * crate::geom::EMU_PER_PX);
    if flip_x {
        out.flip_h = !orig.flip_h;
    }
    if flip_y {
        out.flip_v = !orig.flip_v;
    }
    out.clamped()
}

/// The transform after rotating `orig` so its rotation handle points at `(px, py)`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn rotate_to(orig: &Transform, px: f32, py: f32, m: Modifiers) -> Transform {
    let r = rect_px(orig);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let ang = f64::from(py - cy).atan2(f64::from(px - cx)).to_degrees() + 90.0;
    let mut deg = ang.rem_euclid(360.0);
    if m.shift {
        deg = (deg / 15.0).round() * 15.0;
    } else {
        // A gentle pull to the quarter turns.
        for q in [0.0, 90.0, 180.0, 270.0, 360.0] {
            if (deg - q).abs() < 3.0 {
                deg = q;
            }
        }
    }
    let mut out = *orig;
    out.rot = ((deg.rem_euclid(360.0) * 60_000.0).round() as i32).rem_euclid(21_600_000);
    out
}

/// The transform after moving by `(dx, dy)` slide px (Shift keeps the larger axis only).
#[must_use]
pub fn moved(orig: &Transform, dx: f32, dy: f32, m: Modifiers) -> Transform {
    let (dx, dy) = if m.shift {
        if dx.abs() >= dy.abs() {
            (dx, 0.0)
        } else {
            (0.0, dy)
        }
    } else {
        (dx, dy)
    };
    orig.translated(emu(f64::from(dx)), emu(f64::from(dy)))
}

/// The bounds of several transforms (a multiple selection's box), in slide px.
#[must_use]
pub fn union_bounds(xfrms: &[Transform]) -> Option<Bounds> {
    let mut it = xfrms.iter().map(|x| {
        let b = x.bounds();
        Bounds::new(
            crate::geom::px(b.x),
            crate::geom::px(b.y),
            crate::geom::px(b.cx),
            crate::geom::px(b.cy),
        )
    });
    let first = it.next()?;
    Some(it.fold(first, |a, b| a.union(&b)))
}

/// Maps a transform resized as part of a group box from `from` to `to` (slide px): position and
/// size scale with the box, rotation is kept.
#[must_use]
pub fn scale_into(x: &Transform, from: Bounds, to: Bounds) -> Transform {
    let sx = if from.w > 0.0 { to.w / from.w } else { 1.0 };
    let sy = if from.h > 0.0 { to.h / from.h } else { 1.0 };
    let r = rect_px(x);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let ncx = to.x + (cx - from.x) * sx;
    let ncy = to.y + (cy - from.y) * sy;
    let quarter = (x.rot / 5_400_000) % 2 == 1;
    let (kx, ky) = if quarter { (sy, sx) } else { (sx, sy) };
    let mut out = *x;
    out.cx = emu(f64::from((r.w * kx).abs().max(MIN_SIDE)));
    out.cy = emu(f64::from((r.h * ky).abs().max(MIN_SIDE)));
    out.with_center(
        f64::from(ncx) * crate::geom::EMU_PER_PX,
        f64::from(ncy) * crate::geom::EMU_PER_PX,
    )
    .clamped()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(x: f64, y: f64, w: f64, h: f64) -> Transform {
        Transform::new(emu(x), emu(y), emu(w), emu(h))
    }

    #[test]
    fn handles_sit_on_the_box() {
        let (hs, rot) = handles(&t(100.0, 50.0, 200.0, 100.0), 1.0);
        assert_eq!(hs[0], (100.0, 50.0));
        assert_eq!(hs[4], (300.0, 150.0));
        assert!((rot.0 - 200.0).abs() < 1e-3 && (rot.1 - (50.0 - ROTATE_OFFSET)).abs() < 1e-3);
    }

    #[test]
    fn grips_are_found() {
        let x = t(100.0, 50.0, 200.0, 100.0);
        assert_eq!(
            grip_at(&x, 300.0, 150.0, 1.0, true),
            Some(Grip::Size(Handle::SE))
        );
        assert_eq!(grip_at(&x, 200.0, 100.0, 1.0, true), Some(Grip::Body));
        assert_eq!(
            grip_at(&x, 200.0, 50.0 - ROTATE_OFFSET, 1.0, true),
            Some(Grip::Rotate)
        );
        assert_eq!(grip_at(&x, 500.0, 500.0, 1.0, true), None);
    }

    #[test]
    fn corner_resize_keeps_the_opposite_corner() {
        let x = t(100.0, 50.0, 200.0, 100.0);
        let r = rect_px(&resize(&x, Handle::SE, 50.0, 20.0, Modifiers::default()));
        assert!((r.x - 100.0).abs() < 0.01 && (r.y - 50.0).abs() < 0.01);
        assert!((r.w - 250.0).abs() < 0.01 && (r.h - 120.0).abs() < 0.01);
        // Shift keeps the proportions; Ctrl grows from the centre.
        let p = rect_px(&resize(
            &x,
            Handle::SE,
            100.0,
            0.0,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ));
        assert!((p.w / p.h - 2.0).abs() < 0.01, "{p:?}");
        let c = rect_px(&resize(
            &x,
            Handle::E,
            10.0,
            0.0,
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ));
        assert!(
            (c.x - 90.0).abs() < 0.01 && (c.w - 220.0).abs() < 0.01,
            "{c:?}"
        );
    }

    #[test]
    fn dragging_past_the_edge_flips() {
        let x = t(100.0, 50.0, 200.0, 100.0);
        let f = resize(&x, Handle::E, -260.0, 0.0, Modifiers::default());
        assert!(f.flip_h);
        let r = rect_px(&f);
        assert!(
            (r.x - 40.0).abs() < 0.01 && (r.w - 60.0).abs() < 0.01,
            "{r:?}"
        );
    }

    #[test]
    fn rotation_snaps_with_shift() {
        let x = t(0.0, 0.0, 100.0, 100.0);
        // Pointing right of the centre is a quarter turn.
        let r = rotate_to(&x, 200.0, 50.0, Modifiers::default());
        assert_eq!(r.rot, 90 * 60_000);
        let s = rotate_to(
            &x,
            120.0,
            0.0,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(s.rot % (15 * 60_000), 0);
    }

    #[test]
    fn shift_constrains_moves() {
        let x = t(0.0, 0.0, 10.0, 10.0);
        let m = rect_px(&moved(
            &x,
            30.0,
            5.0,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ));
        assert!((m.x - 30.0).abs() < 0.01 && m.y.abs() < 0.01);
    }
}
