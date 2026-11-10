// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left sheets (`sheetView rightToLeft`): the sheet is drawn mirrored,
//! as Excel draws it, with column A at the right, the row headers at the right, the frozen
//! columns at the right and every cell mirrored inside (borders, indicators, fill handle).
//!
//! Tiles keep their place in sheet space; their ops are mirrored about the tile's vertical
//! centre and the painter places the tile at the mirrored position (`x → −x` across the sheet,
//! see [`mirror_x`]). Text, pictures and icons are not mirrored: a [`Group`] of ops keeps its own
//! layout and moves so that its box lands where the mirrored box is, so Arabic and Latin text read
//! as they should in a right-to-left sheet.

use std::ops::Range;

use kiln_scene::{Bounds, DrawOp, GradientKind, PathData, ShadowStyle};

/// Ops that move as a whole (text, an icon): their box in the op coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The ops (indices into the op list).
    pub ops: Range<usize>,
    /// Left of the box.
    pub x0: f32,
    /// Right of the box.
    pub x1: f32,
}

/// The x of a sheet-space position in the mirrored sheet (the sheet's origin stays where it
/// is, cells extend to the left).
#[must_use]
pub fn mirror_x(x: f32) -> f32 {
    -x
}

/// A sheet-space rectangle in the mirrored sheet.
#[must_use]
pub fn mirror_rect(r: Bounds) -> Bounds {
    Bounds::new(-r.right(), r.y, r.w, r.h)
}

fn flip(r: Bounds, w: f32) -> Bounds {
    Bounds::new(w - r.x - r.w, r.y, r.w, r.h)
}

fn flip_path(p: &mut PathData, w: f32) {
    for c in p.coords.as_chunks_mut::<2>().0 {
        c[0] = w - c[0];
    }
}

/// Device x extent of a glyph run's bitmaps.
fn glyph_extent(quads: &[i32]) -> Option<(i32, i32)> {
    let mut lo = i32::MAX;
    let mut hi = i32::MIN;
    for q in quads.as_chunks::<{ kiln_scene::GlyphRun::STRIDE }>().0 {
        lo = lo.min(q[4]);
        hi = hi.max(q[4] + q[2]);
    }
    (lo <= hi).then_some((lo, hi))
}

fn shift_glyphs(quads: &mut [i32], dev: i32) {
    for q in quads.as_chunks_mut::<{ kiln_scene::GlyphRun::STRIDE }>().0 {
        q[4] += dev;
    }
}

/// Moves an op by `dx` (CSS px) and its glyphs by `dev` device px.
fn translate(op: &mut DrawOp, dx: f32, dev: i32) {
    match op {
        DrawOp::FillRect { rect, .. }
        | DrawOp::StrokeRect { rect, .. }
        | DrawOp::Caret { rect, .. } => *rect = rect.translate(dx, 0.0),
        DrawOp::Gradient { rect, gradient, .. } => {
            *rect = rect.translate(dx, 0.0);
            match &mut gradient.kind {
                GradientKind::Linear { x0, x1, .. } => {
                    *x0 += dx;
                    *x1 += dx;
                }
                GradientKind::Radial { cx, .. } => *cx += dx,
            }
        }
        DrawOp::Path { path, .. } | DrawOp::OutlineGlyphs { path, .. } => path.translate(dx, 0.0),
        DrawOp::Glyphs(run) => shift_glyphs(&mut run.quads, dev),
        DrawOp::Image { dst, .. } => *dst = dst.translate(dx, 0.0),
        DrawOp::PushClip { rect, path } => {
            *rect = rect.translate(dx, 0.0);
            if let Some(p) = path {
                p.translate(dx, 0.0);
            }
        }
        DrawOp::PushTransform { matrix } => matrix[4] += dx,
        DrawOp::Shadow(s) => s.rect = s.rect.translate(dx, 0.0),
        DrawOp::Squiggle { x, .. } | DrawOp::Underline { x, .. } | DrawOp::Handle { x, .. } => {
            *x += dx;
        }
        DrawOp::SelectionRects { rects, .. } => {
            for r in rects {
                *r = r.translate(dx, 0.0);
            }
        }
        DrawOp::Guide { x0, x1, .. } => {
            *x0 += dx;
            *x1 += dx;
        }
        DrawOp::PopClip | DrawOp::PopTransform => {}
    }
}

/// Mirrors an op about `x = w / 2` (glyph runs and outlines move instead: text stays readable).
fn flip_op(op: &mut DrawOp, w: f32, scale: f32) {
    match op {
        DrawOp::FillRect { rect, .. }
        | DrawOp::StrokeRect { rect, .. }
        | DrawOp::Caret { rect, .. } => *rect = flip(*rect, w),
        DrawOp::Gradient { rect, gradient, .. } => {
            *rect = flip(*rect, w);
            match &mut gradient.kind {
                GradientKind::Linear { x0, x1, .. } => {
                    *x0 = w - *x0;
                    *x1 = w - *x1;
                }
                GradientKind::Radial { cx, .. } => *cx = w - *cx,
            }
        }
        DrawOp::Path { path, .. } => flip_path(path, w),
        DrawOp::OutlineGlyphs { path, .. } => {
            let b = path.bounds();
            path.translate(w - b.x - b.right(), 0.0);
        }
        DrawOp::Glyphs(run) => {
            if let Some((lo, hi)) = glyph_extent(&run.quads) {
                #[allow(clippy::cast_possible_truncation)]
                let wd = (w * scale).round() as i32;
                shift_glyphs(&mut run.quads, wd - lo - hi);
            }
        }
        DrawOp::Image { dst, .. } => *dst = flip(*dst, w),
        DrawOp::PushClip { rect, path } => {
            *rect = flip(*rect, w);
            if let Some(p) = path {
                flip_path(p, w);
            }
        }
        DrawOp::PushTransform { matrix } => {
            // F·M·F with the reflection, then the move to the mirrored origin.
            matrix[1] = -matrix[1];
            matrix[2] = -matrix[2];
            matrix[4] = w - matrix[4];
        }
        DrawOp::Shadow(s) => {
            s.rect = flip(s.rect, w);
            if let ShadowStyle::Drop { dx, .. } = &mut s.style {
                *dx = -*dx;
            }
        }
        DrawOp::Squiggle { x, width, .. } | DrawOp::Underline { x, width, .. } => {
            *x = w - *x - *width;
        }
        DrawOp::Handle { x, .. } => *x = w - *x,
        DrawOp::SelectionRects { rects, .. } => {
            for r in rects {
                *r = flip(*r, w);
            }
        }
        DrawOp::Guide { x0, x1, .. } => {
            *x0 = w - *x0;
            *x1 = w - *x1;
        }
        DrawOp::PopClip | DrawOp::PopTransform => {}
    }
}

/// Mirrors ops about `x = width / 2` (`width` 0 mirrors about the origin, as sheet-space
/// overlays are). `groups` (sorted, not overlapping) move as a whole; everything else is
/// reflected. Inside a pushed transform ops are in local coordinates: the transform is
/// reflected and its contents are mirrored about their own origin. `scale` is the device scale
/// (glyph runs are in device pixels).
pub fn mirror_ops(ops: &mut [DrawOp], width: f32, scale: f32, groups: &[Group]) {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let mut gi = 0usize;
    // Transforms pushed outside groups: inside them the mirror axis is the local origin.
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < ops.len() {
        while gi < groups.len() && groups[gi].ops.end <= i {
            gi += 1;
        }
        if depth == 0
            && let Some(g) = groups.get(gi).filter(|g| g.ops.contains(&i))
        {
            // The group's box lands on the mirrored box.
            #[allow(clippy::cast_possible_truncation)]
            let dev = ((width - g.x0 - g.x1) * s).round() as i32;
            #[allow(clippy::cast_precision_loss)]
            let dx = dev as f32 / s;
            let mut inner = 0usize;
            let end = g.ops.end.min(ops.len());
            for op in &mut ops[g.ops.start.max(i)..end] {
                match op {
                    DrawOp::PushClip { .. } if inner == 0 => flip_op(op, width, s),
                    DrawOp::PushTransform { .. } => {
                        if inner == 0 {
                            translate(op, dx, dev);
                        }
                        inner += 1;
                    }
                    DrawOp::PopTransform => inner = inner.saturating_sub(1),
                    _ if inner == 0 => translate(op, dx, dev),
                    _ => {}
                }
            }
            i = g.ops.end.max(i + 1);
            continue;
        }
        let w = if depth == 0 { width } else { 0.0 };
        match &ops[i] {
            DrawOp::PushTransform { .. } => {
                flip_op(&mut ops[i], w, s);
                depth += 1;
            }
            DrawOp::PopTransform => depth = depth.saturating_sub(1),
            _ => flip_op(&mut ops[i], w, s),
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_scene::{ColorRole, GlyphRun, Paint};

    fn fill(x: f32, w: f32) -> DrawOp {
        DrawOp::FillRect {
            rect: Bounds::new(x, 0.0, w, 10.0),
            paint: Paint::Role(ColorRole::Page),
            radius: 0.0,
        }
    }

    fn rect(op: &DrawOp) -> Bounds {
        match op {
            DrawOp::FillRect { rect, .. } => *rect,
            _ => panic!("fill"),
        }
    }

    #[test]
    fn shapes_reflect_and_groups_move() {
        let mut glyphs = GlyphRun {
            page: kiln_scene::AtlasPageId(0),
            paint: Paint::Role(ColorRole::Ink),
            quads: Vec::new(),
        };
        // Two glyphs at device x 10 and 20, 8 wide (a word from 10 to 28).
        glyphs.push([0, 0, 8, 8], [10, 0]);
        glyphs.push([8, 0, 8, 8], [20, 0]);
        let mut ops = vec![
            fill(0.0, 50.0),
            DrawOp::Glyphs(glyphs.clone()),
            fill(10.0, 18.0),
            fill(200.0, 56.0),
        ];
        // Text box 10..28 in a 256 px tile at scale 1.
        let groups = [Group {
            ops: 1..3,
            x0: 10.0,
            x1: 28.0,
        }];
        mirror_ops(&mut ops, 256.0, 1.0, &groups);
        assert_eq!(rect(&ops[0]), Bounds::new(206.0, 0.0, 50.0, 10.0));
        assert_eq!(rect(&ops[3]), Bounds::new(0.0, 0.0, 56.0, 10.0));
        // The word keeps its glyph order and lands on 228..246.
        let DrawOp::Glyphs(run) = &ops[1] else {
            panic!("glyphs")
        };
        assert_eq!((run.quads[4], run.quads[10]), (228, 238));
        assert_eq!(rect(&ops[2]), Bounds::new(228.0, 0.0, 18.0, 10.0));
        // A lone glyph run moves by its own extent.
        let mut lone = vec![DrawOp::Glyphs(glyphs)];
        mirror_ops(&mut lone, 100.0, 2.0, &[]);
        let DrawOp::Glyphs(run) = &lone[0] else {
            panic!("glyphs")
        };
        assert_eq!((run.quads[4], run.quads[10]), (172, 182));
    }

    #[test]
    fn transforms_are_reflected_with_their_contents() {
        let mut path = PathData::new();
        path.move_to(1.0, 0.0).line_to(5.0, 0.0);
        let mut ops = vec![
            DrawOp::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, 30.0, 0.0],
            },
            DrawOp::Path {
                path,
                fill: None,
                stroke: None,
                rule: kiln_scene::FillRule::NonZero,
            },
            DrawOp::PopTransform,
            fill(0.0, 10.0),
        ];
        mirror_ops(&mut ops, 100.0, 1.0, &[]);
        assert!(matches!(ops[0], DrawOp::PushTransform { matrix } if matrix[4] == 70.0));
        let DrawOp::Path { path, .. } = &ops[1] else {
            panic!("path")
        };
        assert_eq!(path.coords, vec![-1.0, 0.0, -5.0, 0.0]);
        assert_eq!(rect(&ops[3]), Bounds::new(90.0, 0.0, 10.0, 10.0));
        assert_eq!(
            mirror_rect(Bounds::new(10.0, 5.0, 20.0, 1.0)),
            Bounds::new(-30.0, 5.0, 20.0, 1.0)
        );
        assert_eq!(mirror_x(4.0), -4.0);
    }
}
