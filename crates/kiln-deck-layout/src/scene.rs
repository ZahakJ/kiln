// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawn slides as Scene tiles: the stage (the slide on its pasteboard, cut into tiles whose
//! size is a whole number of device pixels so glyph bitmaps meet exactly at tile seams) and
//! thumbnails (one tile per slide, painted small).

use kiln_common::NodeId;
use kiln_deck::{Owner, Presentation};
use kiln_scene::{
    Bounds, ColorRole, DrawOp, HitEntry, HitIndex, Paint, SceneDelta, Shadow, ShadowStyle, Tile,
    TileId, TileSlot, layer,
};

use crate::glyphs::Glyphs;
use crate::picture::Uploads;
use crate::render::{DrawnSlide, Options, Renderer};

/// Side of a stage tile in device pixels.
pub const TILE_DEVICE: f32 = 1024.0;
/// The pasteboard around the slide, in slide px (room for shapes off the slide and handles).
pub const PASTEBOARD: f32 = 64.0;
/// The gap between thumbnails in a thumbnail scene, in slide px.
pub const THUMB_GAP: f32 = 16.0;

/// Side of a stage tile in slide px at a device scale.
#[must_use]
pub fn tile_side(scale: f32) -> f32 {
    TILE_DEVICE
        / if scale > 0.0 && scale.is_finite() {
            scale
        } else {
            1.0
        }
}

/// The stage's extent: the slide and everything drawn around it, with a margin.
#[must_use]
pub fn stage_extent(drawn: &DrawnSlide) -> Bounds {
    let mut e = Bounds::new(0.0, 0.0, drawn.width, drawn.height);
    for s in &drawn.shapes {
        if s.bounds.is_finite() && s.bounds.w > 0.0 {
            e = e.union(&s.bounds);
        }
    }
    let e = e.inflate(PASTEBOARD, PASTEBOARD);
    // Shapes far off the slide (corrupt files) must not make the surface enormous.
    let max = drawn.width.max(drawn.height) * 3.0;
    Bounds::from_corners(
        e.x.max(-max),
        e.y.max(-max),
        e.right().min(drawn.width + max),
        e.bottom().min(drawn.height + max),
    )
}

/// The tile slots covering `extent` at `scale`, all with `version`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn stage_slots(extent: Bounds, scale: f32, version: u64) -> Vec<TileSlot> {
    let side = tile_side(scale);
    let x0 = (extent.x / side).floor() as i32;
    let y0 = (extent.y / side).floor() as i32;
    let x1 = (extent.right() / side).ceil() as i32;
    let y1 = (extent.bottom() / side).ceil() as i32;
    let cols = (x1 - x0).clamp(1, 64);
    let rows = (y1 - y0).clamp(1, 64);
    let mut out = Vec::with_capacity(usize::try_from(cols * rows).unwrap_or(0));
    for j in 0..rows {
        for i in 0..cols {
            #[allow(clippy::cast_precision_loss)]
            let rect = Bounds::new((x0 + i) as f32 * side, (y0 + j) as f32 * side, side, side);
            out.push(TileSlot {
                id: TileId {
                    layer: layer::CONTENT,
                    index: u32::try_from(j * cols + i).unwrap_or(0),
                },
                rect,
                version,
            });
        }
    }
    out
}

/// Builds one stage tile: the pasteboard, the slide's shadow, the background and every shape
/// that meets the tile, with a hit index of the shapes (key = shape id).
#[must_use]
pub fn stage_tile(drawn: &DrawnSlide, slot: &TileSlot) -> (Tile, HitIndex) {
    let r = slot.rect;
    let slide = Bounds::new(0.0, 0.0, drawn.width, drawn.height);
    let mut ops = vec![DrawOp::FillRect {
        rect: Bounds::new(0.0, 0.0, r.w, r.h),
        paint: Paint::Role(ColorRole::Canvas),
        radius: 0.0,
    }];
    let mut hits = Vec::new();
    if slide.intersects(&r.inflate(48.0, 48.0)) {
        ops.push(DrawOp::Shadow(Shadow {
            rect: slide.translate(-r.x, -r.y),
            radius: 0.0,
            style: ShadowStyle::Page,
        }));
    }
    ops.push(DrawOp::PushTransform {
        matrix: [1.0, 0.0, 0.0, 1.0, -r.x, -r.y],
    });
    if slide.intersects(&r) {
        ops.extend(drawn.background.iter().cloned());
    }
    for s in &drawn.shapes {
        if !s.bounds.intersects(&r) {
            continue;
        }
        ops.extend(s.ops.iter().cloned());
        hits.push(HitEntry {
            rect: s.bounds.translate(-r.x, -r.y),
            key: s.id.0,
        });
    }
    ops.push(DrawOp::PopTransform);
    (Tile::new(slot.id, r, ops), HitIndex::new(hits))
}

/// Ops of a slide clipped to its rectangle, in slide px.
#[must_use]
pub fn slide_ops(drawn: &DrawnSlide) -> Vec<DrawOp> {
    let slide = Bounds::new(0.0, 0.0, drawn.width, drawn.height);
    let mut ops = vec![DrawOp::PushClip {
        rect: slide,
        path: None,
    }];
    ops.extend(drawn.all_ops());
    ops.push(DrawOp::PopClip);
    ops
}

/// Thumbnails of slides (or layouts and masters) as one self-contained scene: tile `i` is the
/// i-th owner, stacked top to bottom `THUMB_GAP` apart, rendered for `width` CSS px at `dpr`.
pub fn thumbnails(
    renderer: &mut Renderer,
    pres: &Presentation,
    owners: &[(u32, Owner)],
    width: f32,
    dpr: f32,
    revision: u64,
) -> SceneDelta {
    let slide_w = crate::geom::px(pres.slide_size.cx).max(1.0);
    let slide_h = crate::geom::px(pres.slide_size.cy).max(1.0);
    let scale = (width.max(16.0) / slide_w) * dpr.clamp(0.5, 4.0);
    let mut glyphs = Glyphs::new(scale);
    let mut uploads = Uploads::default();
    let mut images = Vec::new();
    let mut tiles = Vec::with_capacity(owners.len());
    let opts = Options::default();
    for (k, (index, owner)) in owners.iter().enumerate() {
        let drawn = renderer.draw(pres, *owner, &opts, &mut glyphs);
        #[allow(clippy::cast_precision_loss)]
        let y = k as f32 * (slide_h + THUMB_GAP);
        let rect = Bounds::new(0.0, y, slide_w, slide_h);
        images.extend(uploads.take(&renderer.pictures, &drawn.images, false));
        tiles.push(Tile::new(
            TileId {
                layer: layer::CONTENT,
                index: *index,
            },
            rect,
            slide_ops(&drawn),
        ));
    }
    #[allow(clippy::cast_precision_loss)]
    let total = owners.len() as f32 * (slide_h + THUMB_GAP);
    SceneDelta {
        revision,
        reset: true,
        scale,
        extent: Bounds::new(0.0, 0.0, slide_w, total.max(slide_h)),
        background: Paint::Rgba(0),
        tiles,
        removed: Vec::new(),
        atlas: glyphs.updates(true),
        images,
        overlay: None,
    }
}

/// One slide rendered for export or a golden test at `scale`, as a self-contained scene whose
/// only tile is the slide.
pub fn slide_delta(
    renderer: &mut Renderer,
    pres: &Presentation,
    slide: NodeId,
    scale: f32,
    opts: &Options,
) -> SceneDelta {
    let mut glyphs = Glyphs::new(scale);
    let drawn = renderer.draw(pres, Owner::Slide(slide), opts, &mut glyphs);
    let mut uploads = Uploads::default();
    let images = uploads.take(&renderer.pictures, &drawn.images, true);
    let rect = Bounds::new(0.0, 0.0, drawn.width, drawn.height);
    SceneDelta {
        revision: 1,
        reset: true,
        scale,
        extent: rect,
        background: Paint::Rgba(0xffff_ffff),
        tiles: vec![Tile::new(TileId::content(0), rect, slide_ops(&drawn))],
        removed: Vec::new(),
        atlas: glyphs.updates(true),
        images,
        overlay: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_whole_device_pixels() {
        let scale = 0.72 * 2.0;
        let slots = stage_slots(Bounds::new(-64.0, -64.0, 1088.0, 668.0), scale, 3);
        for s in &slots {
            let dx = s.rect.x * scale;
            assert!((dx - dx.round()).abs() < 1e-2, "{dx}");
            assert_eq!(s.version, 3);
        }
        let covered = slots.iter().fold(Bounds::ZERO, |a, s| a.union(&s.rect));
        assert!(covered.x <= -64.0 && covered.right() >= 1024.0);
    }
}
