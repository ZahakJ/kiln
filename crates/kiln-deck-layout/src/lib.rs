// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-deck-layout
//!
//! Slide rendering for Kiln Deck: a slide becomes Scene ops —
//! the background, the master's and layout's decoration, then the slide's shapes in z-order
//! with preset and custom geometry, fills (solid, gradient, pattern, picture), outlines with
//! dashes and arrowheads, shadows and glows, pictures with cropping, groups, tables, charts
//! through `kiln-charts`, connectors and media posters, and text bodies through `kiln-text`
//! with the formatting inherited from layouts and masters, bullets and numbering, autofit,
//! anchoring, columns and vertical text.
//!
//! - [`render`]: [`render::Renderer`] draws a slide, layout or master into [`render::DrawnSlide`]
//!   (ops per shape with their bounds), and answers shape geometry and text layouts for
//!   editing; empty placeholders show their prompt in the editing view.
//! - [`scene`]: the stage as tiles of whole device pixels on a pasteboard, thumbnails and
//!   single-slide scenes for export and golden tests.
//! - [`text`]: text bodies laid out, drawn, and asked for carets, hit tests and selections.
//! - [`select`] and [`guides`]: the eight handles and the rotation handle, grips under a point,
//!   move/resize/rotate with Shift, Ctrl and Alt, smart guides and snapping.
//! - [`motion`]: transitions and animation effects evaluated at a progress, for the slide show.
//! - [`print`]: slides, handouts, notes pages and the outline as printable pages (PDF, print).
//! - [`paint`], [`table`], [`picture`], [`glyphs`], [`geom`], [`fonts`], [`a11y`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod a11y;
pub mod fonts;
pub mod geom;
pub mod glyphs;
pub mod guides;
pub mod motion;
pub mod names;
pub mod paint;
pub mod patterns;
pub mod picture;
pub mod print;
pub mod render;
pub mod scene;
pub mod select;
pub mod table;
pub mod text;

pub use glyphs::Glyphs;
pub use render::{DrawnShape, DrawnSlide, Options, PromptLanguage, Renderer, ShapeGeom};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
