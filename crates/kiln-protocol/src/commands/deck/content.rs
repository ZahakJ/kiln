// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck's chart data, Slide Master view, Notes Page view and guides: the answer of
//! [`super::DeckQuery::ChartData`], which [`super::DeckCommand::SetChartData`] writes back; what
//! the Slide Master tab and its thumbnail rail show ([`super::DeckState::master_view`]); the
//! notes page's geometry ([`super::DeckQuery::NotesPage`]); the drawing guides; crops to a ratio;
//! and the selected cells of a table.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Edit Data: the selected chart's data range on its data sheet, cell by cell as the sheet shows
/// it (the first row holds the series names, the first column the categories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckChartData {
    /// The data sheet's name.
    pub sheet: String,
    /// The range's first row on the sheet (0-based).
    pub row: u32,
    /// The range's first column on the sheet (0-based).
    pub col: u32,
    /// The cells, row by row.
    pub cells: Vec<Vec<String>>,
    /// Why the data cannot be edited, when the chart's workbook is one Kiln cannot read (the
    /// cells are then the values the chart holds).
    pub unreadable: Option<String>,
}

/// One thumbnail of Slide Master view's rail: a master, or a layout under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckMasterItem {
    /// The master or layout.
    pub id: NodeId,
    /// Its name.
    pub name: String,
    /// A master (layouts follow their master).
    pub master: bool,
    /// Preserve: kept in the file when no slide uses it.
    pub preserved: bool,
    /// Slides that use it (a master: its layouts').
    pub used: u32,
    /// Changes whenever what it shows changes (thumbnail cache key).
    #[ts(type = "number")]
    pub version: u64,
}

/// Slide Master view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckMasterView {
    /// Every master with its layouts, in order (the thumbnails of `DeckQuery::MasterThumbnails`).
    pub items: Vec<DeckMasterItem>,
    /// The master or layout on the stage.
    pub current: NodeId,
    /// The layout on the stage has a title (Slide Master ▸ Title).
    pub title: bool,
    /// The layout on the stage has the date, footer and slide number (Slide Master ▸ Footers).
    pub footers: bool,
}

/// What Slide Master ▸ Insert Placeholder puts on a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckPlaceholderKind {
    /// Content (text or any object).
    Content,
    /// Text.
    Text,
    /// Picture.
    Picture,
    /// Chart.
    Chart,
    /// Table.
    Table,
    /// Media.
    Media,
}

/// View ▸ Notes Page: where the slide picture and the notes box sit on the notes page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckNotesPage {
    /// The page width, px.
    pub width: f32,
    /// The page height, px.
    pub height: f32,
    /// The slide picture, page px.
    pub slide: super::DeckRect,
    /// The notes box, page px.
    pub body: super::DeckRect,
}

/// A drawing guide (View ▸ Guides): a line across the slide that objects snap to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckGuide {
    /// A horizontal line (at `pos` from the top), else a vertical one (from the left).
    pub horizontal: bool,
    /// Where it is, slide px.
    pub pos: f32,
}

/// Picture Format ▸ Crop ▸ Aspect Ratio, Fill and Fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckCropFit {
    /// Crop to an aspect ratio (`width`:`height`), as much of the picture as fits.
    Aspect {
        /// Width part.
        width: u32,
        /// Height part.
        height: u32,
    },
    /// Fill the frame with the picture, cropping what overflows.
    Fill,
    /// Fit all of the picture in the frame, keeping its proportions.
    Fit,
}

/// The layout Convert to List Diagram makes of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DeckListDiagram {
    /// A grid of blocks, one per item (Basic Block List).
    BlockList,
    /// A row of boxes joined by arrows (Basic Process).
    Process,
    /// Bars stacked down the area (Vertical Bullet List).
    VerticalList,
}

/// The cells selected in a table (Table Layout acts on them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckCellRange {
    /// First row.
    pub row: u32,
    /// First column.
    pub col: u32,
    /// Rows.
    pub rows: u32,
    /// Columns.
    pub cols: u32,
}

/// The text of one shape of a slide ([`super::DeckQuery::SlideText`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckShapeText {
    /// The shape's name.
    pub name: String,
    /// `shape`, `table`, `group`, … (as [`super::DeckShapeInfo::kind`] without `text box` and
    /// `placeholder`, which are shapes).
    pub kind: String,
    /// Its paragraphs (a table's cell by cell, row by row), empty ones left out.
    pub lines: Vec<String>,
}

/// Everything a slide says: the text of each shape that has text and is not hidden, back to
/// front (a group's shapes in its place), and its speaker notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSlideText {
    /// The slide.
    pub slide: NodeId,
    /// How many shapes the slide has (a group counts once).
    pub shapes: u32,
    /// The shapes with text.
    pub texts: Vec<DeckShapeText>,
    /// The notes' paragraphs, empty ones left out.
    pub notes: Vec<String>,
    /// Hidden in the show.
    pub hidden: bool,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of these types (see `crate::samples`).

    use super::*;
    use crate::samples::Samples;

    macro_rules! samples_of {
        ($($ty:ty => [$($v:expr),* $(,)?];)*) => {
            $(impl Samples for $ty {
                fn samples() -> Vec<Self> {
                    vec![$($v),*]
                }
            })*
        };
    }

    /// A shape's text.
    #[must_use]
    pub fn shape_text() -> DeckShapeText {
        DeckShapeText {
            name: "Title 1".into(),
            kind: "shape".into(),
            lines: vec!["Firing schedule".into(), "Autumn".into()],
        }
    }

    /// A data range.
    #[must_use]
    pub fn chart_data() -> DeckChartData {
        DeckChartData {
            sheet: "Sheet1".into(),
            row: 0,
            col: 0,
            cells: vec![
                vec![String::new(), "Series 1".into()],
                vec!["Category 1".into(), "4.3".into()],
            ],
            unreadable: None,
        }
    }

    /// Slide Master view on a layout.
    #[must_use]
    pub fn master_view() -> DeckMasterView {
        DeckMasterView {
            items: vec![
                DeckMasterItem {
                    id: NodeId(2),
                    name: "Kiln".into(),
                    master: true,
                    preserved: true,
                    used: 4,
                    version: 7,
                },
                DeckMasterItem {
                    id: NodeId(3),
                    name: "Title Slide".into(),
                    master: false,
                    preserved: false,
                    used: 1,
                    version: 9,
                },
            ],
            current: NodeId(3),
            title: true,
            footers: false,
        }
    }

    samples_of! {
        DeckNotesPage => [DeckNotesPage {
            width: 720.0,
            height: 960.0,
            slide: super::super::DeckRect { x: 96.0, y: 72.0, width: 528.0, height: 297.0 },
            body: super::super::DeckRect { x: 72.0, y: 456.0, width: 576.0, height: 403.2 },
        }];
        DeckGuide => [DeckGuide { horizontal: true, pos: 360.0 }, DeckGuide { horizontal: false, pos: 120.5 }];
        DeckCropFit => [DeckCropFit::Aspect { width: 16, height: 9 }, DeckCropFit::Fill, DeckCropFit::Fit];
        DeckCellRange => [DeckCellRange { row: 1, col: 0, rows: 2, cols: 3 }];
        DeckListDiagram => [DeckListDiagram::BlockList, DeckListDiagram::Process, DeckListDiagram::VerticalList];
        DeckChartData => [chart_data()];
        DeckShapeText => [shape_text()];
        DeckSlideText => [DeckSlideText { slide: NodeId(7), shapes: 3, texts: vec![shape_text()], notes: vec!["Open with the numbers.".into()], hidden: false }];
        DeckMasterItem => [master_view().items[0].clone()];
        DeckMasterView => [master_view()];
        DeckPlaceholderKind => [DeckPlaceholderKind::Content, DeckPlaceholderKind::Picture, DeckPlaceholderKind::Media];
    }

    /// Registers the types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(
            v;
            DeckChartData, DeckMasterItem, DeckMasterView, DeckPlaceholderKind, DeckNotesPage,
            DeckGuide, DeckCropFit, DeckCellRange, DeckListDiagram, DeckShapeText, DeckSlideText,
        );
    }
}
