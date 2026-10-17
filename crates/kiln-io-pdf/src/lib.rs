// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-pdf
//!
//! PDF export, print and print preview (PLAN.md §3.10): documents become PDF through `krilla`
//! from the same layout and Scene ops that paint them on screen, so screen, preview, print and
//! PDF agree.
//!
//! - [`printable`]: the [`PrintablePages`] contract every app implements (pages, labels,
//!   structure, outline, images, painting into a [`PageCanvas`]). Kiln Write's implementation
//!   is [`write::WritePrint`]; Grid and Deck register theirs in their work packages.
//! - [`options`]: [`PdfOptions`] (page ranges, odd/even, copies, fit to paper, scale, pages per
//!   sheet, grayscale, tagging, bookmarks).
//! - [`sheet`]: imposition of pages on sheets, shared by the writer and the preview.
//! - [`export`]: the writer. Text is real text with subset embedded fonts (selectable,
//!   searchable, extractable); JPEG pictures are passed through, PNG re-compressed losslessly;
//!   paths, clips, transforms and gradients map one to one; drop shadows are approximated.
//!   Tagged PDF from the model's structure (title, H1–H6, P, Table/TR/TH/TD, Figure with alt
//!   text, Link annotations for hyperlinks and TOC entries, Note), outline bookmarks from
//!   headings, page labels from section page-number formats, XMP metadata with producer `Kiln`.
//! - [`preview()`]: one sheet as a Scene for the Surface painter (Backstage ▸ Print).
//!
//! ```no_run
//! use kiln_io_pdf::{PdfOptions, export, write::{WritePrint, print_layout}};
//! # let doc = kiln_doc::Document::new();
//! let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
//! let pdf = export(&WritePrint::new(&doc, &layout, Some("Report".into())), &PdfOptions::default())?;
//! assert!(pdf.starts_with(b"%PDF"));
//! # Ok::<(), kiln_common::KilnError>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod options;
mod pdf;
mod preview;
pub mod printable;
pub mod sheet;
pub mod write;

pub use options::{PageParity, PageRange, PageSelection, Paper, PdfOptions, Scaling};
pub use pdf::{Exported, PRODUCER, export, export_report};
pub use preview::{MARGIN as PREVIEW_MARGIN, Preview, preview};
pub use printable::{PageCanvas, PrintablePages};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
