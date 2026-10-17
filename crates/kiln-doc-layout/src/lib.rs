// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-doc-layout
//!
//! The layout engine of Kiln Write (PLAN.md §3.4, §3.6): turns a `kiln-doc` [`Document`] into
//! pages and paints them as Scene tiles.
//!
//! - **Paragraphs** ([`source`], [`para`]): resolved Word formatting → `kiln-text` runs (fonts
//!   per script slot, caps, super/subscript, highlights, list labels, field results, note
//!   numbers, inline pictures as sized placeholders), laid out by `kiln-text` (tabs and leaders,
//!   justification, bidi, hyphenation) and flowed around floating objects.
//! - **Pages** ([`flow`]): sections with size, margins, orientation and columns; keep with next,
//!   keep lines together, widow/orphan control, page break before, forced page/column breaks,
//!   odd/even section breaks, vertical page alignment; tables row by row with split rows, rows
//!   that cannot split and repeated header rows ([`table`]: fixed and autofit widths, merged
//!   cells, nested tables, cell margins, Word's border conflict rules); floating pictures and
//!   shapes anchored to the page, margins, column, paragraph, line or character with square,
//!   tight, through, top-and-bottom, behind and in-front wrapping ([`drawing`]); footnotes at
//!   the page bottom with a separator, endnotes at the end.
//! - **Finishing** ([`engine`]): headers and footers per first/odd/even page with
//!   Link to Previous and live PAGE, NUMPAGES, SECTIONPAGES, DATE, TIME, FILENAME and AUTHOR
//!   fields; page number formats; comment balloons (geometry); views (print, web, read, draft).
//! - **Incremental**: edits re-flow from the first changed block and stop when the flow meets
//!   the previous layout again ([`DocLayout::update`]); the first pages can be produced before
//!   the rest ([`DocLayout::layout_until`]).
//! - **Geometry** ([`geometry`]): hit testing, carets (with the second caret at bidi
//!   boundaries), selections, line ends and line-to-line motion across pages, columns and
//!   tables.
//! - **Scene** ([`paint`]): pages as tiles with the theme's page shadow, glyphs from the atlas
//!   ([`glyphs`]), pictures ([`media`]) and DrawingML shapes; accessibility nodes for visible
//!   pages ([`a11y`]); ruler data ([`rulers`]).
//!
//! ```
//! use kiln_doc::Document;
//! use kiln_doc_layout::{DocLayout, default_fonts};
//!
//! let doc = Document::new();
//! let mut layout = DocLayout::new(default_fonts());
//! layout.layout(&doc);
//! assert_eq!(layout.pages().len(), 1);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod a11y;
pub mod annotations;
pub mod cx;
pub mod drawing;
pub mod engine;
pub mod fields;
pub mod flow;
pub mod geometry;
pub mod glyphs;
pub mod media;
pub mod page;
pub mod paint;
pub mod para;
pub mod rulers;
pub mod snapshot;
pub mod source;
pub mod style;
pub mod table;
pub mod units;

pub use engine::{Change, DocLayout, LineRef, Stats, default_fonts};
pub use flow::View;
pub use geometry::{CaretGeom, Hit};
pub use kiln_doc::Document;
pub use page::{Item, LineItem, Page, StoryKind};
pub use paint::DocScene;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
