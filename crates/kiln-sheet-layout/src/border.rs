// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell borders: which side wins on a shared edge, and how each of Excel's 13 line styles is
//! drawn.
//!
//! Two cells can both describe the edge between them (A1's right side and B1's left side).
//! Excel shows one line: the heavier style wins (thick over double over the medium styles over
//! thin over the dashed and dotted ones over hair); on a tie the cell to the right or below
//! wins, being drawn later. Merged areas have no inner edges.

use kiln_scene::{DrawOp, Paint, PathData, Stroke};
use kiln_sheet::styles::{BorderSide, BorderStyle};

use crate::geom::Snap;

/// The precedence of a line style (higher wins a shared edge).
#[must_use]
pub fn weight(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::None => 0,
        BorderStyle::Hair => 1,
        BorderStyle::Dotted => 2,
        BorderStyle::DashDotDot => 3,
        BorderStyle::DashDot => 4,
        BorderStyle::Dashed => 5,
        BorderStyle::Thin => 6,
        BorderStyle::MediumDashDotDot => 7,
        BorderStyle::SlantDashDot => 8,
        BorderStyle::MediumDashDot => 9,
        BorderStyle::MediumDashed => 10,
        BorderStyle::Medium => 11,
        BorderStyle::Double => 12,
        BorderStyle::Thick => 13,
    }
}

/// The side shown on an edge described by `before` (the left or upper cell's side) and `after`
/// (the right or lower cell's side).
#[must_use]
pub fn winner<T: Copy>(
    before: Option<(BorderSide, T)>,
    after: Option<(BorderSide, T)>,
) -> Option<(BorderSide, T)> {
    let w = |s: &Option<(BorderSide, T)>| s.as_ref().map_or(0, |(b, _)| weight(b.style));
    let (wb, wa) = (w(&before), w(&after));
    if wb == 0 && wa == 0 {
        return None;
    }
    if wb > wa { before } else { after }
}

/// How a style is drawn: width in CSS pixels, dash pattern, and whether it is a double line.
#[must_use]
pub fn line_of(style: BorderStyle) -> (f32, &'static [f32], bool) {
    match style {
        BorderStyle::None => (0.0, &[], false),
        BorderStyle::Thin => (1.0, &[], false),
        BorderStyle::Hair => (1.0, &[1.0, 1.0], false),
        BorderStyle::Dotted => (1.0, &[1.0, 2.0], false),
        BorderStyle::Dashed => (1.0, &[3.0, 1.0], false),
        BorderStyle::DashDot => (1.0, &[9.0, 3.0, 3.0, 3.0], false),
        BorderStyle::DashDotDot => (1.0, &[9.0, 3.0, 3.0, 3.0, 3.0, 3.0], false),
        BorderStyle::Medium => (2.0, &[], false),
        BorderStyle::MediumDashed => (2.0, &[9.0, 3.0], false),
        BorderStyle::MediumDashDot => (2.0, &[9.0, 3.0, 3.0, 3.0], false),
        BorderStyle::MediumDashDotDot => (2.0, &[9.0, 3.0, 3.0, 3.0, 3.0, 3.0], false),
        BorderStyle::SlantDashDot => (2.0, &[11.0, 1.0, 5.0, 1.0], false),
        BorderStyle::Thick => (3.0, &[], false),
        BorderStyle::Double => (3.0, &[], true),
    }
}

/// The extent of a line of `width` drawn on a boundary at `at`: Excel's thin line is the last
/// pixel of the cell before the boundary; wider lines grow to both sides.
fn band(at: f32, width: f32, snap: Snap) -> (f32, f32) {
    let px = snap.px(1.0);
    match width as u32 {
        0 | 1 => (at - px, at),
        2 => (at - px, at + px),
        _ => (at - 2.0 * px, at + px),
    }
}

/// Appends the ops of one edge. `horizontal`: from `(a, at)` to `(b, at)`; otherwise from
/// `(at, a)` to `(at, b)`. Solid lines of 2 px and more reach one pixel past both ends so
/// corners meet.
#[allow(clippy::too_many_arguments)]
pub fn edge_ops(
    out: &mut Vec<DrawOp>,
    side: BorderSide,
    paint: Paint,
    horizontal: bool,
    at: f32,
    a: f32,
    b: f32,
    snap: Snap,
) {
    let (width, dash, double) = line_of(side.style);
    if width <= 0.0 || b <= a {
        return;
    }
    let px = snap.px(1.0);
    let (lo, hi) = band(at, width, snap);
    let ext = if width >= 2.0 { px } else { 0.0 };
    let rect = |l0: f32, l1: f32, a: f32, b: f32| {
        if horizontal {
            kiln_scene::Bounds::from_corners(a, l0, b, l1)
        } else {
            kiln_scene::Bounds::from_corners(l0, a, l1, b)
        }
    };
    if double {
        for (l0, l1) in [(lo, lo + px), (hi - px, hi)] {
            out.push(DrawOp::FillRect {
                rect: rect(l0, l1, a - px, b + px),
                paint,
                radius: 0.0,
            });
        }
        return;
    }
    if dash.is_empty() {
        out.push(DrawOp::FillRect {
            rect: rect(lo, hi, a - px - ext, b + ext),
            paint,
            radius: 0.0,
        });
        return;
    }
    let mid = (lo + hi) / 2.0;
    let mut path = PathData::new();
    if horizontal {
        path.move_to(a, mid).line_to(b, mid);
    } else {
        path.move_to(mid, a).line_to(mid, b);
    }
    out.push(DrawOp::Path {
        path,
        fill: None,
        stroke: Some(Stroke {
            dash: dash.to_vec(),
            ..Stroke::solid(paint, hi - lo)
        }),
        rule: kiln_scene::FillRule::NonZero,
    });
}

/// Appends a diagonal line across `rect` (`up`: bottom-left to top-right).
pub fn diagonal_ops(
    out: &mut Vec<DrawOp>,
    side: BorderSide,
    paint: Paint,
    rect: kiln_scene::Bounds,
    up: bool,
) {
    let (width, dash, double) = line_of(side.style);
    if width <= 0.0 {
        return;
    }
    let (x0, y0, x1, y1) = if up {
        (rect.x, rect.bottom(), rect.right(), rect.y)
    } else {
        (rect.x, rect.y, rect.right(), rect.bottom())
    };
    let lines: &[f32] = if double { &[-1.0, 1.0] } else { &[0.0] };
    for &off in lines {
        let mut path = PathData::new();
        path.move_to(x0, y0 + off).line_to(x1, y1 + off);
        out.push(DrawOp::Path {
            path,
            fill: None,
            stroke: Some(Stroke {
                dash: dash.to_vec(),
                ..Stroke::solid(paint, if double { 1.0 } else { width })
            }),
            rule: kiln_scene::FillRule::NonZero,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(style: BorderStyle) -> Option<(BorderSide, u8)> {
        Some((BorderSide { style, color: None }, 0))
    }

    #[test]
    fn heavier_lines_win_shared_edges() {
        assert_eq!(
            winner(side(BorderStyle::Thin), side(BorderStyle::Thick)).map(|s| s.0.style),
            Some(BorderStyle::Thick)
        );
        assert_eq!(
            winner(side(BorderStyle::Double), side(BorderStyle::Dashed)).map(|s| s.0.style),
            Some(BorderStyle::Double)
        );
        assert_eq!(
            winner(side(BorderStyle::Medium), None).map(|s| s.0.style),
            Some(BorderStyle::Medium)
        );
        assert!(winner::<u8>(None, None).is_none());
        // Every style ranks between none and thick.
        for s in [
            BorderStyle::Hair,
            BorderStyle::Dotted,
            BorderStyle::Thin,
            BorderStyle::MediumDashed,
            BorderStyle::SlantDashDot,
        ] {
            assert!(weight(s) > 0 && weight(s) < weight(BorderStyle::Thick));
        }
    }

    #[test]
    fn lines_land_on_device_pixels() {
        let mut out = Vec::new();
        let thin = BorderSide {
            style: BorderStyle::Thin,
            color: None,
        };
        edge_ops(
            &mut out,
            thin,
            Paint::BLACK,
            true,
            20.0,
            0.0,
            64.0,
            Snap(1.0),
        );
        assert_eq!(
            out[0],
            DrawOp::FillRect {
                rect: kiln_scene::Bounds::new(-1.0, 19.0, 65.0, 1.0),
                paint: Paint::BLACK,
                radius: 0.0
            }
        );
        out.clear();
        let dbl = BorderSide {
            style: BorderStyle::Double,
            color: None,
        };
        edge_ops(
            &mut out,
            dbl,
            Paint::BLACK,
            false,
            64.0,
            0.0,
            20.0,
            Snap(1.0),
        );
        assert_eq!(out.len(), 2);
        out.clear();
        let dashed = BorderSide {
            style: BorderStyle::MediumDashed,
            color: None,
        };
        edge_ops(
            &mut out,
            dashed,
            Paint::BLACK,
            true,
            20.0,
            0.0,
            64.0,
            Snap(2.0),
        );
        assert!(
            matches!(&out[0], DrawOp::Path { stroke: Some(s), .. } if s.width == 2.0 && !s.dash.is_empty())
        );
    }
}
