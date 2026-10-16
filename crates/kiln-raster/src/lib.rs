// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-raster
//!
//! A CPU painter on `tiny-skia` that turns a Scene into an RGBA bitmap, used by `kiln render`,
//! golden-image tests, thumbnails and as a native fallback (PLAN.md §3.6).
//!
//! [`Compositor`] is the raster twin of `packages/surface`'s tile cache: it applies the same
//! [`SceneDelta`](kiln_scene::SceneDelta)s the canvas painter receives (tiles, atlas pages,
//! images, overlay), renders each tile once into its own bitmap at the scene's device scale, and
//! composites tiles and overlay for a [`View`] with the same pixel rounding as the canvas painter,
//! so the two agree pixel for pixel (checked by `e2e/specs/painter-parity.spec.ts`).
//!
//! - Glyph runs are copied 1:1 from atlas pages (A8 coverage tinted with the run's colour, or
//!   RGBA colour glyphs), clipped by the active clip mask.
//! - Paths are anti-aliased (non-zero and even-odd), stroked with joins, caps and dashes.
//! - Clips are anti-aliased masks; transforms nest; gradients are linear or radial.
//! - Shadows are exact Gaussian box shadows (erf), the same formula as the canvas painter.
//! - [`golden`] compares renderings with committed PNGs (0.3 % differing pixels allowed) and
//!   writes diff images on failure.
//!
//! `#![forbid(unsafe_code)]` is relaxed to `deny` here (PLAN.md §3.2) for possible zero-copy
//! buffer hand-off; the crate contains no `unsafe`.
#![deny(unsafe_code)]
#![warn(missing_docs)]

mod compositor;
pub mod golden;
mod painter;
mod png_io;

pub use compositor::{Compositor, MAX_SIDE, RasterError, View, device, tile_device_size};
pub use painter::{AtlasPixels, OpPainter, PaintOptions, Resources, sk_path, underline_segments};
pub use png_io::Rgba;
pub use tiny_skia::Pixmap;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Renders a single delta (from an empty cache) and returns the straight-alpha pixels.
///
/// # Errors
///
/// When the delta is malformed or the view is empty or too large.
pub fn render_delta(
    delta: &kiln_scene::SceneDelta,
    view: &View,
    palette: &kiln_scene::Palette,
) -> Result<Rgba, RasterError> {
    let mut c = Compositor::new();
    c.apply(delta)?;
    Ok(Rgba::from_pixmap(&c.render(view, palette)?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
