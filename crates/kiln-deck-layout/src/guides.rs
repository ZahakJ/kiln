// SPDX-License-Identifier: AGPL-3.0-or-later
//! Smart guides and snapping (PowerPoint's alignment guides): while a shape moves or resizes,
//! its edges and centre snap to the slide's edges and centre lines and to other shapes' edges
//! and centres, and to equal spacing between neighbours; the lines it snapped to are shown.

use kiln_scene::Bounds;

/// A guide line to show, in slide px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    /// Start x.
    pub x0: f32,
    /// Start y.
    pub y0: f32,
    /// End x.
    pub x1: f32,
    /// End y.
    pub y1: f32,
    /// An equal-spacing marker rather than an alignment line.
    pub spacing: bool,
}

/// Which edges of the moving box may snap (a move snaps all; a resize only the dragged ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    /// Left edge.
    pub left: bool,
    /// Horizontal centre.
    pub center_x: bool,
    /// Right edge.
    pub right: bool,
    /// Top edge.
    pub top: bool,
    /// Vertical centre.
    pub center_y: bool,
    /// Bottom edge.
    pub bottom: bool,
}

impl Edges {
    /// Every edge and centre (a move).
    pub const ALL: Edges = Edges {
        left: true,
        center_x: true,
        right: true,
        top: true,
        center_y: true,
        bottom: true,
    };
}

/// The correction that snaps a box, and the guides to draw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapped {
    /// Horizontal correction.
    pub dx: f32,
    /// Vertical correction.
    pub dy: f32,
    /// Guides.
    pub guides: Vec<Guide>,
}

fn xs(b: &Bounds) -> [f32; 3] {
    [b.x, b.x + b.w / 2.0, b.right()]
}

fn ys(b: &Bounds) -> [f32; 3] {
    [b.y, b.y + b.h / 2.0, b.bottom()]
}

/// Snaps `moving` to the slide and to `others` within `threshold` slide px.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn snap(
    moving: Bounds,
    others: &[Bounds],
    slide: Bounds,
    threshold: f32,
    edges: Edges,
) -> Snapped {
    let mx = xs(&moving);
    let my = ys(&moving);
    let mx_on = [edges.left, edges.center_x, edges.right];
    let my_on = [edges.top, edges.center_y, edges.bottom];
    // (delta, target line, target box) per axis.
    let mut best_x: Option<(f32, f32, Bounds)> = None;
    let mut best_y: Option<(f32, f32, Bounds)> = None;
    let mut consider = |target: &Bounds, slide_lines: bool| {
        let tx = xs(target);
        let ty = ys(target);
        for (i, m) in mx.iter().enumerate() {
            if !mx_on[i] {
                continue;
            }
            for (j, t) in tx.iter().enumerate() {
                // The slide offers its edges and centre; shapes offer edges to edges and centre to
                // centre (and edge to edge across, as PowerPoint).
                if !slide_lines && (i == 1) != (j == 1) {
                    continue;
                }
                let d = t - m;
                if d.abs() <= threshold && best_x.is_none_or(|b| d.abs() < b.0.abs()) {
                    best_x = Some((d, *t, *target));
                }
            }
        }
        for (i, m) in my.iter().enumerate() {
            if !my_on[i] {
                continue;
            }
            for (j, t) in ty.iter().enumerate() {
                if !slide_lines && (i == 1) != (j == 1) {
                    continue;
                }
                let d = t - m;
                if d.abs() <= threshold && best_y.is_none_or(|b| d.abs() < b.0.abs()) {
                    best_y = Some((d, *t, *target));
                }
            }
        }
    };
    consider(&slide, true);
    for o in others {
        consider(o, false);
    }
    let mut out = Snapped::default();
    // Equal spacing between two neighbours on a row (or a column) the box shares.
    let spacing_x = equal_spacing(moving, others, threshold, true, edges.left && edges.right);
    let spacing_y = equal_spacing(moving, others, threshold, false, edges.top && edges.bottom);
    if let Some((d, a, b)) = spacing_x
        && best_x.is_none_or(|bx| d.abs() + 0.01 < bx.0.abs())
    {
        out.dx = d;
        let snapped = moving.translate(d, 0.0);
        let y = snapped.y + snapped.h / 2.0;
        for (l, r) in [(a.right(), snapped.x), (snapped.right(), b.x)] {
            out.guides.push(Guide {
                x0: l,
                y0: y,
                x1: r,
                y1: y,
                spacing: true,
            });
        }
        best_x = None;
    }
    if let Some((d, a, b)) = spacing_y
        && best_y.is_none_or(|by| d.abs() + 0.01 < by.0.abs())
    {
        out.dy = d;
        let snapped = moving.translate(0.0, d);
        let x = snapped.x + snapped.w / 2.0;
        for (t, bt) in [(a.bottom(), snapped.y), (snapped.bottom(), b.y)] {
            out.guides.push(Guide {
                x0: x,
                y0: t,
                x1: x,
                y1: bt,
                spacing: true,
            });
        }
        best_y = None;
    }
    if let Some((d, line, target)) = best_x {
        out.dx = d;
        let b = moving.translate(out.dx, out.dy);
        out.guides.push(Guide {
            x0: line,
            y0: b.y.min(target.y),
            x1: line,
            y1: b.bottom().max(target.bottom()),
            spacing: false,
        });
    }
    if let Some((d, line, target)) = best_y {
        out.dy = d;
        let b = moving.translate(out.dx, out.dy);
        out.guides.push(Guide {
            x0: b.x.min(target.x),
            y0: line,
            x1: b.right().max(target.right()),
            y1: line,
            spacing: false,
        });
    }
    out
}

/// The correction that puts `moving` midway between two neighbours along an axis, with the two
/// neighbours: `(delta, before, after)`.
fn equal_spacing(
    moving: Bounds,
    others: &[Bounds],
    threshold: f32,
    horizontal: bool,
    allowed: bool,
) -> Option<(f32, Bounds, Bounds)> {
    if !allowed {
        return None;
    }
    let overlaps = |o: &Bounds| {
        if horizontal {
            o.y < moving.bottom() && o.bottom() > moving.y
        } else {
            o.x < moving.right() && o.right() > moving.x
        }
    };
    let (start, end, size) = if horizontal {
        (moving.x, moving.right(), moving.w)
    } else {
        (moving.y, moving.bottom(), moving.h)
    };
    let lo = |o: &Bounds| if horizontal { o.x } else { o.y };
    let hi = |o: &Bounds| if horizontal { o.right() } else { o.bottom() };
    let before = others
        .iter()
        .filter(|o| overlaps(o) && hi(o) <= start + threshold)
        .max_by(|a, b| hi(a).total_cmp(&hi(b)))?;
    let after = others
        .iter()
        .filter(|o| overlaps(o) && lo(o) >= end - threshold)
        .min_by(|a, b| lo(a).total_cmp(&lo(b)))?;
    let room = lo(after) - hi(before) - size;
    if room < 0.0 {
        return None;
    }
    let want = hi(before) + room / 2.0;
    let d = want - start;
    (d.abs() <= threshold).then_some((d, *before, *after))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLIDE: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        w: 960.0,
        h: 540.0,
    };

    #[test]
    fn snaps_to_the_slide_centre() {
        let b = Bounds::new(377.0, 100.0, 200.0, 50.0);
        let s = snap(b, &[], SLIDE, 6.0, Edges::ALL);
        assert!((s.dx - 3.0).abs() < 1e-4, "{s:?}");
        assert!(s.guides.iter().any(|g| (g.x0 - 480.0).abs() < 1e-4));
    }

    #[test]
    fn snaps_edges_to_other_shapes() {
        let other = Bounds::new(100.0, 300.0, 120.0, 80.0);
        let b = Bounds::new(103.0, 100.0, 60.0, 60.0);
        let s = snap(b, &[other], SLIDE, 6.0, Edges::ALL);
        assert!((s.dx - -3.0).abs() < 1e-4, "{s:?}");
        assert!(
            s.guides
                .iter()
                .any(|g| !g.spacing && (g.x0 - 100.0).abs() < 1e-4)
        );
    }

    #[test]
    fn equal_spacing_between_neighbours() {
        let a = Bounds::new(100.0, 200.0, 100.0, 50.0);
        let c = Bounds::new(500.0, 200.0, 100.0, 50.0);
        // Midway is x = 300 for a 100-wide box: 298 snaps there.
        let b = Bounds::new(298.0, 210.0, 100.0, 40.0);
        let s = snap(
            b,
            &[a, c],
            Bounds::new(-10_000.0, -10_000.0, 1.0, 1.0),
            6.0,
            Edges::ALL,
        );
        assert!((s.dx - 2.0).abs() < 1e-4, "{s:?}");
        assert_eq!(s.guides.iter().filter(|g| g.spacing).count(), 2);
    }

    #[test]
    fn resizing_snaps_only_dragged_edges() {
        let other = Bounds::new(300.0, 300.0, 100.0, 100.0);
        let b = Bounds::new(100.0, 100.0, 197.0, 50.0);
        let edges = Edges {
            left: false,
            center_x: false,
            right: true,
            top: false,
            center_y: false,
            bottom: false,
        };
        let s = snap(
            b,
            &[other],
            Bounds::new(-10_000.0, -10_000.0, 1.0, 1.0),
            6.0,
            edges,
        );
        assert!((s.dx - 3.0).abs() < 1e-4 && s.dy == 0.0, "{s:?}");
    }
}
