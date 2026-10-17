// SPDX-License-Identifier: AGPL-3.0-or-later
//! Printing and PDF export (WP-67): the settings of File ▸ Print and File ▸ Export ▸ PDF, the
//! print preview (`Query::PrintPreview`) and a rendered PDF (`Query::Pdf`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::scene::SceneDelta;

/// Odd or even pages only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PrintParity {
    /// Every selected page.
    #[default]
    All,
    /// Only odd pages.
    Odd,
    /// Only even pages.
    Even,
}

/// A paper to scale the pages to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PrintPaper {
    /// US Letter.
    Letter,
    /// US Legal.
    Legal,
    /// A3.
    A3,
    /// A4.
    A4,
    /// A5.
    A5,
}

/// Page orientation for printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PrintOrientation {
    /// Portrait.
    Portrait,
    /// Landscape.
    Landscape,
}

/// Word's margin presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PrintMargins {
    /// 1″ all round.
    Normal,
    /// 0.5″ all round.
    Narrow,
    /// 1″ top and bottom, 0.75″ left and right.
    Moderate,
    /// 1″ top and bottom, 2″ left and right.
    Wide,
}

impl PrintMargins {
    /// Top, right, bottom, left in twips.
    #[must_use]
    pub const fn twips(self) -> [i32; 4] {
        match self {
            Self::Normal => [1440, 1440, 1440, 1440],
            Self::Narrow => [720, 720, 720, 720],
            Self::Moderate => [1440, 1080, 1440, 1080],
            Self::Wide => [1440, 2880, 1440, 2880],
        }
    }
}

/// What to print and how (File ▸ Print, File ▸ Export ▸ PDF).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PrintSettings {
    /// Word's page box (`1-3, 5, 8-`); empty for every page.
    pub pages: String,
    /// Odd or even pages only.
    pub parity: PrintParity,
    /// Copies, 1–999 (collated).
    pub copies: u32,
    /// Scale every page to this paper; `None` prints each page at its own size.
    pub paper: Option<PrintPaper>,
    /// Zoom, 0.1–4.0.
    pub scale: f32,
    /// Pages per sheet: 1, 2, 4, 6, 9 or 16.
    pub per_sheet: u32,
    /// Print in this orientation (documents with pages); `None` keeps the document's.
    pub orientation: Option<PrintOrientation>,
    /// Print with these margins (documents with pages); `None` keeps the document's.
    pub margins: Option<PrintMargins>,
    /// Shades of grey.
    pub grayscale: bool,
    /// Tracked changes and comments as markup (Kiln Write).
    pub markup: bool,
    /// Hidden text (Kiln Write).
    pub hidden_text: bool,
    /// Tagged PDF (accessibility structure).
    pub tagged: bool,
    /// Bookmarks from headings.
    pub bookmarks: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            pages: String::new(),
            parity: PrintParity::All,
            copies: 1,
            paper: None,
            scale: 1.0,
            per_sheet: 1,
            orientation: None,
            margins: None,
            grayscale: false,
            markup: true,
            hidden_text: false,
            tagged: true,
            bookmarks: true,
        }
    }
}

/// One sheet of the print preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PrintPreview {
    /// Sheets of one copy.
    pub sheets: u32,
    /// The sheet shown (0-based; clamped to the sheets there are).
    pub sheet: u32,
    /// Sheet width in points.
    pub width_pt: f32,
    /// Sheet height in points.
    pub height_pt: f32,
    /// The document pages on the sheet (0-based).
    pub pages: Vec<u32>,
    /// Pages in the document.
    pub page_count: u32,
    /// The sheet for the Surface painter (a reset delta: one tile, its glyph atlas and pictures).
    pub scene: SceneDelta,
}

/// A rendered PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PdfFile {
    /// A file name for it (`Report.pdf`).
    pub name: String,
    /// The file.
    #[serde(with = "kiln_scene::bytes")]
    #[ts(type = "Array<number>")]
    pub bytes: Vec<u8>,
    /// It is PDF/UA-1.
    pub pdf_ua: bool,
    /// What kept it from PDF/UA-1 (empty when it is, or when it is untagged).
    pub ua_issues: Vec<String>,
}
