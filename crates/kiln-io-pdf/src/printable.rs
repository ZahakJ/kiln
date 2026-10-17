// SPDX-License-Identifier: AGPL-3.0-or-later
//! The contract between an app and the print pipeline.
//!
//! An app makes its document printable by implementing [`PrintablePages`]: how many pages, how
//! big, how each page is labelled and painted, which images it uses, the logical structure
//! (for tagged PDF) and the outline. It paints a page into a [`PageCanvas`] with the same
//! Scene [`DrawOp`]s it paints on screen, plus [`TextRun`]s for text (so the PDF gets real,
//! selectable text instead of the screen's glyph bitmaps) and marks that tie content to the
//! structure. Kiln Write implements it in [`crate::write`]; Grid (page setup → sheet pages) and
//! Deck (slides, handouts, notes pages) implement it in their work packages.
//!
//! Two canvases consume the same calls: the PDF writer ([`crate::export`]) and the print
//! preview ([`crate::preview`]), so the preview always shows what the PDF contains.

use std::ops::Range;
use std::sync::Arc;

use kiln_scene::{Bounds, DrawOp, ImageId, Paint};
use kiln_text::{FontStore, GlyphRun};

/// How a page number is written (page labels in the PDF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelStyle {
    /// 1, 2, 3.
    #[default]
    Decimal,
    /// i, ii, iii.
    LowerRoman,
    /// I, II, III.
    UpperRoman,
    /// a, b, c.
    LowerLetter,
    /// A, B, C.
    UpperLetter,
    /// No number (only the prefix, if any).
    None,
}

/// A page's label as PDF viewers show it ("iv", "A-3").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageLabel {
    /// Number style.
    pub style: LabelStyle,
    /// Text before the number.
    pub prefix: String,
    /// The page's number (≥ 1).
    pub number: u32,
}

/// One page of a printable document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageInfo {
    /// Width in CSS px (96 per inch).
    pub width: f32,
    /// Height in CSS px.
    pub height: f32,
    /// Its label.
    pub label: PageLabel,
}

/// Document information for the PDF's metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocInfo {
    /// Title.
    pub title: Option<String>,
    /// Authors.
    pub authors: Vec<String>,
    /// Subject (description).
    pub subject: Option<String>,
    /// Keywords.
    pub keywords: Vec<String>,
    /// Natural language (BCP 47), e.g. `en-US`.
    pub language: Option<String>,
    /// The application that made the document ("Kiln Write").
    pub creator: Option<String>,
    /// Creation time, ISO 8601 (`2026-10-17T09:30:00Z`).
    pub created: Option<String>,
    /// Right-to-left main text direction.
    pub rtl: bool,
}

/// An entry of the outline (bookmarks), in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    /// Nesting level, 1 = top.
    pub level: u8,
    /// Text.
    pub title: String,
    /// 0-based page.
    pub page: usize,
    /// Top of the target on the page, px.
    pub y: f32,
}

/// Identifies a node of the [`StructTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId(pub u32);

/// What a structure node is (a subset of PDF's standard structure types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructKind {
    /// A generic section of the document.
    Section,
    /// A generic block container (text boxes).
    Div,
    /// A paragraph.
    P,
    /// A heading of level 1–6 with its text (PDF/UA wants a title).
    Heading {
        /// Level 1–6.
        level: u8,
        /// The heading's text.
        title: String,
    },
    /// A table.
    Table,
    /// A table row.
    TableRow,
    /// A header cell.
    TableHeader {
        /// Rows spanned.
        rows: u32,
        /// Columns spanned.
        cols: u32,
    },
    /// A data cell.
    TableCell {
        /// Rows spanned.
        rows: u32,
        /// Columns spanned.
        cols: u32,
    },
    /// A picture, chart or shape with its alternative text.
    Figure {
        /// Alt text (`None` for decorative figures, which become artifacts).
        alt: Option<String>,
    },
    /// A hyperlink (its text and the link annotation).
    Link,
    /// A footnote or endnote.
    Note,
    /// A list.
    List,
    /// A list item.
    ListItem,
    /// A table of contents.
    Toc,
    /// An entry of a table of contents.
    TocItem,
}

/// One node of the logical structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructNode {
    /// What it is.
    pub kind: StructKind,
    /// Its parent (`None` = the document root).
    pub parent: Option<StructId>,
}

/// The logical structure of a document: nodes with parents. The order of children is the order
/// in which content of each node is first painted (reading order follows the layout).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructTree {
    /// Nodes, indexed by [`StructId`].
    pub nodes: Vec<StructNode>,
}

impl StructTree {
    /// Adds a node.
    pub fn add(&mut self, kind: StructKind, parent: Option<StructId>) -> StructId {
        let id = StructId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(StructNode { kind, parent });
        id
    }

    /// A node.
    #[must_use]
    pub fn get(&self, id: StructId) -> Option<&StructNode> {
        self.nodes.get(id.0 as usize)
    }
}

/// Where a link goes.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// An external URI.
    Uri(String),
    /// A place in the document: 0-based page and the y of the target, px.
    Page {
        /// Page.
        page: usize,
        /// Top of the target.
        y: f32,
    },
}

/// What non-structural content is (artifacts in tagged PDF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// Running headers.
    Header,
    /// Running footers.
    Footer,
    /// Backgrounds, shading, rules and other decoration.
    Layout,
}

/// What the content painted until the matching [`PageCanvas::end`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Content of a structure node.
    Content(StructId),
    /// Decoration that is not part of the logical structure.
    Artifact(ArtifactKind),
}

/// Glyphs of one run of text, positioned on the page, with the text they show.
#[derive(Debug, Clone)]
pub struct TextRun<'a> {
    /// The fonts the run's font ids refer to.
    pub fonts: &'a FontStore,
    /// The run (glyph ids, positions relative to `origin`, cluster byte offsets into `text`).
    pub run: &'a GlyphRun,
    /// The glyphs of the run to draw (a link splits a run into parts).
    pub glyphs: Range<usize>,
    /// Page position of the run's coordinate origin, px.
    pub origin: (f32, f32),
    /// The text the clusters index into.
    pub text: &'a str,
    /// Literal colour.
    pub paint: Paint,
    /// What the glyphs stand for when it is not their text (synthesised hyphens and tab
    /// leaders): e.g. `Some("")` for a leader, `Some("\u{AD}")` for an automatic hyphen.
    pub actual: Option<&'a str>,
}

/// An image in the encoding it came in (for passthrough) and decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintImage {
    /// The id ops refer to.
    pub id: ImageId,
    /// Width in pixels (of `rgba`).
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA, `width × height × 4`.
    pub rgba: Arc<Vec<u8>>,
    /// The original file when it can be embedded as is (JPEG) or re-encoded losslessly at full
    /// resolution (PNG).
    pub source: Option<EncodedImage>,
}

/// An encoded image file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    /// The format.
    pub format: ImageFormat,
    /// The bytes.
    pub data: Arc<Vec<u8>>,
}

/// Encoded image formats the PDF embeds from source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG (embedded as is: DCT passthrough).
    Jpeg,
    /// PNG (decoded and re-compressed at full resolution).
    Png,
}

impl ImageFormat {
    /// Recognises JPEG and PNG by their signatures.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else {
            None
        }
    }
}

/// Receives the content of one page. Coordinates are page px (origin top left, y down).
pub trait PageCanvas {
    /// A Scene op with literal paints. Glyph bitmap ops ([`DrawOp::Glyphs`]) are ignored: text
    /// arrives as [`PageCanvas::text`]. [`DrawOp::Image`] refers to [`PrintablePages::image`].
    fn op(&mut self, op: &DrawOp);
    /// Text.
    fn text(&mut self, run: &TextRun<'_>);
    /// Starts content that belongs to `mark` (marks do not nest; a new one ends the last).
    fn begin(&mut self, mark: Mark);
    /// Ends the current mark.
    fn end(&mut self);
    /// A link area over content of the `Link` node `node`.
    fn link(&mut self, rect: Bounds, target: &LinkTarget, node: StructId);
}

/// A document that can be printed.
pub trait PrintablePages {
    /// The pages.
    fn pages(&self) -> Vec<PageInfo>;
    /// Metadata.
    fn info(&self) -> DocInfo;
    /// The logical structure (an empty tree for documents without one).
    fn structure(&self) -> StructTree;
    /// The outline, in document order.
    fn outline(&self) -> Vec<OutlineEntry>;
    /// An image painted by [`DrawOp::Image`].
    fn image(&self, id: ImageId) -> Option<Arc<PrintImage>>;
    /// Paints page `index` (0-based) into `canvas`.
    fn paint(&self, index: usize, canvas: &mut dyn PageCanvas);
}
