// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-text
//!
//! The text pipeline of Kiln (PLAN.md §3.6): fonts, shaping, bidi, line breaking, hyphenation,
//! carets and the glyph atlas. Every surface (Write pages, Grid cells, Deck text bodies, PDF export)
//! lays text out through this crate, so line breaks are identical on every platform and in print.
//!
//! - [`FontStore`] holds the bundled OFL document fonts (Liberation, Carlito, Caladea; feature
//!   `bundled-fonts`), fonts added at run time and, on native targets, the system's fonts (feature
//!   `system-fonts`). Office family names go through the substitution table ([`SubstitutionKind`]).
//! - [`TextEngine::layout`] turns a [`ParagraphSpec`] (styled runs + Word paragraph properties) into a
//!   [`ParagraphLayout`]: lines with glyph runs, decorations, carets and hit testing.
//! - [`GlyphAtlas`] rasterises glyphs into atlas pages for the Scene; large glyphs become outlines.
//!
//! ```
//! use kiln_text::{FontStore, ParagraphSpec, SpanStyle, TextEngine};
//!
//! let mut engine = TextEngine::new(FontStore::bundled());
//! let spec = ParagraphSpec::plain("Kiln lays out text like Word.", SpanStyle::new("Calibri", 16.0), 120.0);
//! let layout = engine.layout(&spec);
//! assert_eq!(layout.lines.len(), 2);
//! let caret = layout.caret_rect(5);
//! assert_eq!(layout.hit_test(caret.x, caret.center_y()), 5);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod atlas;
#[cfg(feature = "bundled-fonts")]
mod bundled;
mod caret;
mod fonts;
mod hyphen;
mod layout;
mod metrics;
mod output;
mod shape_cache;
mod spec;
mod substitution;

pub use atlas::{
    ATLAS_PAGE_SIZE, AtlasError, AtlasFormat, AtlasGlyph, AtlasPage, GlyphAtlas, GlyphKey,
    OUTLINE_THRESHOLD, PageId, PlacedGlyph, RunGlyphs, SUBPIXEL_STEPS, glyph_outline,
};
pub use fonts::{FontId, FontInstance, FontStore, FontStoreOptions, ResolvedFamily};
pub use layout::TextEngine;
pub use metrics::{ScaledMetrics, UnitMetrics};
pub use output::{
    Affinity, CaretRect, ClusterBox, Decoration, DecorationKind, GlyphRun, Line, LineEnd,
    ParagraphLayout, PositionedGlyph, Rect,
};
pub use spec::{
    Alignment, BLACK, Direction, LineSpacing, ParagraphProps, ParagraphSpec, Rgba, SpanStyle,
    StrikeKind, TabAlignment, TabLeader, TabStop, TextRun, UnderlineKind,
};
pub use substitution::SubstitutionKind;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Points to CSS pixels (96 dpi).
#[must_use]
pub fn pt_to_px(pt: f32) -> f32 {
    pt * 4.0 / 3.0
}

/// Twentieths of a point (OOXML twips) to CSS pixels.
#[must_use]
pub fn twips_to_px(twips: f32) -> f32 {
    twips / 15.0
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn unit_conversions() {
        assert!((super::pt_to_px(12.0) - 16.0).abs() < 1e-6);
        assert!((super::twips_to_px(1440.0) - 96.0).abs() < 1e-6);
    }
}
