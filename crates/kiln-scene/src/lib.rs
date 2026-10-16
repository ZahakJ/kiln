// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-scene
//!
//! The retained display list (PLAN.md §3.6): every surface (Write pages, Grid tiles, Deck slides)
//! becomes a [`Scene`] of tiles holding [`DrawOp`]s, plus an overlay (caret, selection, composition,
//! handles). Painters — `packages/surface` (Canvas 2D), [`kiln-raster`](../kiln_raster) (tiny-skia)
//! and later the PDF writer — only execute ops; they never lay anything out.
//!
//! - [`Bounds`], [`Paint`]/[`ColorRole`]/[`Palette`], [`PathData`]: the vocabulary.
//! - [`DrawOp`]: rectangles, paths (non-zero/even-odd, strokes), glyph runs blitted from atlas pages,
//!   outline glyphs, images, clips, transforms, gradients, shadows, squiggles, underlines, the
//!   caret, selection rectangles, guides and touch handles.
//! - [`Scene`]: tile slots for the whole document (cheap rectangles with a content version), the
//!   tiles built for the region a frame needs (hashed with FxHash over their ops), a hit-test index
//!   per tile and the overlay.
//! - [`SceneDelta`] and [`DeltaTracker`]: the smallest update for a painter that already holds
//!   some tiles, encoded with postcard.
//!
//! Every wire type derives `serde` and `ts_rs::TS`; `kiln-protocol` re-exports them in
//! `kiln_protocol::scene`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bytes;
mod delta;
mod geom;
mod op;
mod paint;
mod path;
mod scene;
pub mod shapes;

pub use delta::{AtlasFormat, AtlasUpdate, DeltaTracker, ImageUpload, Overlay, SceneDelta};
pub use geom::Bounds;
pub use op::{
    AtlasPageId, DrawOp, GlyphRun, Gradient, GradientKind, GradientStop, HandleSide, ImageId,
    LineStyle, Shadow, ShadowStyle, cull,
};
pub use paint::{ColorRole, Paint, Palette, SelectionBlend, ShadowLayer, channels};
pub use path::{FillRule, LineCap, LineJoin, PathData, Segment, Stroke, verb};
pub use scene::{
    BuiltTile, HASH_MASK, Hit, HitEntry, HitIndex, Scene, Tile, TileId, TileSlot, hash_ops, layer,
};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
