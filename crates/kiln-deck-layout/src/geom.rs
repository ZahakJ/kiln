// SPDX-License-Identifier: AGPL-3.0-or-later
//! Units and geometry: EMU ↔ CSS px (px = EMU / 9525), the affine transform of a
//! shape (rotation about its centre, flips) and helpers for Scene paths.

use kiln_deck::Transform;
use kiln_scene::{Bounds, PathData, verb};
use kurbo::Affine;

/// EMU per CSS px at zoom 1 (96 dpi).
pub const EMU_PER_PX: f64 = 9525.0;

/// EMU to CSS px.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn px(emu: i64) -> f32 {
    (emu as f64 / EMU_PER_PX) as f32
}

/// CSS px to EMU, rounded.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn emu(px: f64) -> i64 {
    if !px.is_finite() {
        return 0;
    }
    (px * EMU_PER_PX).round().clamp(-9.0e15, 9.0e15) as i64
}

/// Points to CSS px.
#[must_use]
pub fn pt(points: f32) -> f32 {
    points * 4.0 / 3.0
}

/// The unrotated rectangle of a transform, in px.
#[must_use]
pub fn rect_px(t: &Transform) -> Bounds {
    Bounds::new(px(t.x), px(t.y), px(t.cx).max(0.0), px(t.cy).max(0.0))
}

/// Rotation of a transform in degrees (clockwise).
#[must_use]
pub fn degrees(t: &Transform) -> f64 {
    f64::from(t.rot) / 60_000.0
}

/// Whether the shape needs a transform beyond a translation (rotated or flipped).
#[must_use]
pub fn is_transformed(t: &Transform) -> bool {
    t.rot != 0 || t.flip_h || t.flip_v
}

/// The matrix mapping shape-local px (0..w, 0..h) to slide px: flips and rotation about the
/// centre, as PowerPoint applies them.
#[must_use]
pub fn shape_matrix(t: &Transform) -> Affine {
    let r = rect_px(t);
    let (w, h) = (f64::from(r.w), f64::from(r.h));
    let (cx, cy) = (f64::from(r.x) + w / 2.0, f64::from(r.y) + h / 2.0);
    let flip = Affine::scale_non_uniform(
        if t.flip_h { -1.0 } else { 1.0 },
        if t.flip_v { -1.0 } else { 1.0 },
    );
    Affine::translate((cx, cy))
        * Affine::rotate(degrees(t).to_radians())
        * flip
        * Affine::translate((-w / 2.0, -h / 2.0))
}

/// The matrix for a shape's text: text turns with the shape but is never mirrored (a vertical
/// flip turns it upside down, as in PowerPoint).
#[must_use]
pub fn text_matrix(t: &Transform) -> Affine {
    let r = rect_px(t);
    let (w, h) = (f64::from(r.w), f64::from(r.h));
    let (cx, cy) = (f64::from(r.x) + w / 2.0, f64::from(r.y) + h / 2.0);
    let extra = if t.flip_v { 180.0 } else { 0.0 };
    Affine::translate((cx, cy))
        * Affine::rotate((degrees(t) + extra).to_radians())
        * Affine::translate((-w / 2.0, -h / 2.0))
}

/// `[a, b, c, d, e, f]` of a matrix (canvas `transform()` order).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn coeffs(m: Affine) -> [f32; 6] {
    let c = m.as_coeffs();
    [
        c[0] as f32,
        c[1] as f32,
        c[2] as f32,
        c[3] as f32,
        c[4] as f32,
        c[5] as f32,
    ]
}

/// Applies a matrix to every point of a path.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn transform_path(p: &PathData, m: Affine) -> PathData {
    let mut out = p.clone();
    for pair in out.coords.as_chunks_mut::<2>().0 {
        let q = m * kurbo::Point::new(f64::from(pair[0]), f64::from(pair[1]));
        pair[0] = q.x as f32;
        pair[1] = q.y as f32;
    }
    out
}

/// Bounds of a rectangle after a matrix.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn transform_bounds(b: Bounds, m: Affine) -> Bounds {
    let pts = [
        (b.x, b.y),
        (b.right(), b.y),
        (b.right(), b.bottom()),
        (b.x, b.bottom()),
    ];
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for (x, y) in pts {
        let q = m * kurbo::Point::new(f64::from(x), f64::from(y));
        x0 = x0.min(q.x);
        y0 = y0.min(q.y);
        x1 = x1.max(q.x);
        y1 = y1.max(q.y);
    }
    Bounds::from_corners(x0 as f32, y0 as f32, x1 as f32, y1 as f32)
}

/// Translates a path by `(dx, dy)`.
#[must_use]
pub fn translated(p: &PathData, dx: f32, dy: f32) -> PathData {
    let mut out = p.clone();
    out.translate(dx, dy);
    out
}

/// `(start, direction out of the start, end, direction out of the end)` of an open path.
pub type PathEnds = ((f32, f32), (f32, f32), (f32, f32), (f32, f32));

/// The start and end points of a path's first subpath with the directions arriving at them,
/// for arrowheads: `(start, direction pointing out of the start, end, direction out of the end)`.
#[must_use]
pub fn path_ends(p: &PathData) -> Option<PathEnds> {
    let mut pts: Vec<(f32, f32)> = Vec::new();
    // Control points count: the tangent at an end of a curve runs to its nearest control point.
    let mut i = 0usize;
    for &v in &p.verbs {
        let n = verb::arity(v);
        if v == verb::MOVE && !pts.is_empty() {
            break;
        }
        for k in 0..n / 2 {
            let x = *p.coords.get(i + 2 * k)?;
            let y = *p.coords.get(i + 2 * k + 1)?;
            pts.push((x, y));
        }
        i += n;
        if v == verb::CLOSE {
            return None;
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let first = pts[0];
    let next = pts
        .iter()
        .skip(1)
        .find(|q| dist(**q, first) > 0.01)
        .copied()?;
    let last = *pts.last()?;
    let prev = pts
        .iter()
        .rev()
        .skip(1)
        .find(|q| dist(**q, last) > 0.01)
        .copied()?;
    Some((
        first,
        unit((first.0 - next.0, first.1 - next.1)),
        last,
        unit((last.0 - prev.0, last.1 - prev.1)),
    ))
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn unit(v: (f32, f32)) -> (f32, f32) {
    let l = (v.0 * v.0 + v.1 * v.1).sqrt();
    if l > 0.0 {
        (v.0 / l, v.1 / l)
    } else {
        (1.0, 0.0)
    }
}

/// Whether a point lies inside a closed path (non-zero rule), flattening curves.
#[must_use]
pub fn path_contains(p: &PathData, x: f32, y: f32) -> bool {
    let poly = flatten(p);
    let mut winding = 0i32;
    for sub in &poly {
        let n = sub.len();
        if n < 3 {
            continue;
        }
        for k in 0..n {
            let (x0, y0) = sub[k];
            let (x1, y1) = sub[(k + 1) % n];
            if y0 <= y {
                if y1 > y && cross(x0, y0, x1, y1, x, y) > 0.0 {
                    winding += 1;
                }
            } else if y1 <= y && cross(x0, y0, x1, y1, x, y) < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
}

fn cross(x0: f32, y0: f32, x1: f32, y1: f32, x: f32, y: f32) -> f32 {
    (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0)
}

/// The distance from a point to the nearest edge of a path (for hit-testing lines).
#[must_use]
pub fn distance_to_path(p: &PathData, x: f32, y: f32) -> f32 {
    let mut best = f32::INFINITY;
    for sub in flatten_open(p) {
        for w in sub.windows(2) {
            best = best.min(segment_distance(w[0], w[1], (x, y)));
        }
    }
    best
}

fn segment_distance(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, (a.0 + t * dx, a.1 + t * dy))
}

/// Subpaths as polylines (closed subpaths repeat no point; the closing edge is implied).
fn flatten(p: &PathData) -> Vec<Vec<(f32, f32)>> {
    flatten_impl(p, false)
}

/// Subpaths as polylines with the closing edge explicit.
fn flatten_open(p: &PathData) -> Vec<Vec<(f32, f32)>> {
    flatten_impl(p, true)
}

fn flatten_impl(p: &PathData, close_explicit: bool) -> Vec<Vec<(f32, f32)>> {
    let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut i = 0usize;
    let c = &p.coords;
    let at = |k: usize| -> (f32, f32) {
        (
            c.get(k).copied().unwrap_or(0.0),
            c.get(k + 1).copied().unwrap_or(0.0),
        )
    };
    for &v in &p.verbs {
        match v {
            verb::MOVE => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                cur.push(at(i));
                i += 2;
            }
            verb::LINE => {
                cur.push(at(i));
                i += 2;
            }
            verb::QUAD => {
                let p0 = cur.last().copied().unwrap_or((0.0, 0.0));
                let (c1, p1) = (at(i), at(i + 2));
                for s in 1..=8 {
                    #[allow(clippy::cast_precision_loss)]
                    let t = s as f32 / 8.0;
                    let mt = 1.0 - t;
                    cur.push((
                        mt * mt * p0.0 + 2.0 * mt * t * c1.0 + t * t * p1.0,
                        mt * mt * p0.1 + 2.0 * mt * t * c1.1 + t * t * p1.1,
                    ));
                }
                i += 4;
            }
            verb::CUBIC => {
                let p0 = cur.last().copied().unwrap_or((0.0, 0.0));
                let (c1, c2, p1) = (at(i), at(i + 2), at(i + 4));
                for s in 1..=12 {
                    #[allow(clippy::cast_precision_loss)]
                    let t = s as f32 / 12.0;
                    let mt = 1.0 - t;
                    let (a, b, cc, d) =
                        (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
                    cur.push((
                        a * p0.0 + b * c1.0 + cc * c2.0 + d * p1.0,
                        a * p0.1 + b * c1.1 + cc * c2.1 + d * p1.1,
                    ));
                }
                i += 6;
            }
            _ => {
                if close_explicit && let Some(f) = cur.first().copied() {
                    cur.push(f);
                }
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_round_trip() {
        assert!((px(9525) - 1.0).abs() < 1e-6);
        assert_eq!(emu(1.0), 9525);
        assert!((px(12_192_000) - 1280.0).abs() < 1e-3);
    }

    #[test]
    fn rotation_turns_about_the_centre() {
        let t = Transform::new(0, 0, emu(100.0), emu(50.0)).with_rotation(90 * 60_000);
        let m = shape_matrix(&t);
        let p = m * kurbo::Point::new(0.0, 0.0);
        // The top-left corner of a 100 × 50 box turned a quarter clockwise about (50, 25).
        assert!(
            (p.x - 75.0).abs() < 1e-6 && (p.y - -25.0).abs() < 1e-6,
            "{p:?}"
        );
    }

    #[test]
    fn containment_and_distance() {
        let r = PathData::rect(Bounds::new(0.0, 0.0, 10.0, 10.0));
        assert!(path_contains(&r, 5.0, 5.0));
        assert!(!path_contains(&r, 15.0, 5.0));
        assert!((distance_to_path(&r, 15.0, 5.0) - 5.0).abs() < 1e-4);
        let mut line = PathData::new();
        line.move_to(0.0, 0.0).line_to(10.0, 0.0);
        let ends = path_ends(&line).unwrap();
        assert_eq!(ends.1, (-1.0, 0.0));
        assert_eq!(ends.3, (1.0, 0.0));
    }
}
