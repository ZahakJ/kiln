// SPDX-License-Identifier: AGPL-3.0-or-later
//! Editing commands the document editors share (WP-64): the clipboard, character and paragraph
//! formatting, lists, page setup, tables and a few insertions that Kiln Write, Deck text and
//! Notes all offer under the same ribbon ids (`suite.format.bold`, `suite.edit.paste`, …).
//!
//! They travel as [`SuiteCommand::Edit`](crate::SuiteCommand::Edit) and the session hands them
//! to the document's model, which applies them to its own selection (a text range in Write, the
//! text of a shape in Deck, the selected cells in Grid). Apps that do not implement one yet answer
//! `Unsupported`; each command's `apps` mask in the suite table lists the apps that do.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What the system clipboard (or a drop) holds, as the host read it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClipboardData {
    /// `text/plain`.
    pub text: String,
    /// `text/html`, if the source offered it.
    pub html: Option<String>,
    /// Kiln's own rich format (`application/x-kiln-fragment`), when the content was copied in
    /// Kiln. The core also remembers its last copy and recognises it by its plain text.
    pub kiln: Option<String>,
}

/// What [`EditCommand::Paste`] keeps of the source formatting (Word's paste options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum PasteMode {
    /// Keep Source Formatting: the source's character and paragraph formatting and styles
    /// (Ctrl+V's default).
    #[default]
    KeepSource,
    /// Merge Formatting: the destination's formatting, keeping only bold, italic and underline
    /// and the list and table structure.
    Merge,
    /// Keep Text Only: plain text in the destination's formatting.
    TextOnly,
    /// Paste Special ▸ HTML Format: the HTML flavour even when Kiln's own format is available.
    Html,
}

/// Change Case (Shift+F3 cycles sentence → lower → upper).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TextCase {
    /// Sentence case.
    Sentence,
    /// lowercase.
    Lower,
    /// UPPERCASE.
    Upper,
    /// Capitalize Each Word.
    Title,
    /// tOGGLE cASE.
    Toggle,
}

/// A numbering format of the Numbering gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ListNumberStyle {
    /// 1. 2. 3.
    Decimal,
    /// 1) 2) 3)
    DecimalParen,
    /// I. II. III.
    UpperRoman,
    /// A. B. C.
    UpperLetter,
    /// a) b) c)
    LowerLetterParen,
    /// a. b. c.
    LowerLetter,
    /// i. ii. iii.
    LowerRoman,
}

/// Word's margin presets (Layout ▸ Margins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum MarginsPreset {
    /// 2.54 cm all round.
    Normal,
    /// 1.27 cm all round.
    Narrow,
    /// 2.54 cm top and bottom, 1.91 cm left and right.
    Moderate,
    /// 2.54 cm top and bottom, 5.08 cm left and right.
    Wide,
    /// Inside 3.18 cm, outside 2.54 cm, mirrored on facing pages.
    Mirrored,
}

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PageOrientation {
    /// Taller than wide.
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// Paper sizes of Layout ▸ Size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PaperSize {
    /// US Letter, 8.5 × 11 in.
    Letter,
    /// US Legal, 8.5 × 14 in.
    Legal,
    /// Executive, 7.25 × 10.5 in.
    Executive,
    /// A3, 297 × 420 mm.
    A3,
    /// A4, 210 × 297 mm.
    A4,
    /// A5, 148 × 210 mm.
    A5,
    /// B5 (JIS), 182 × 257 mm.
    B5,
    /// Tabloid, 11 × 17 in.
    Tabloid,
}

/// What Table Layout ▸ Delete removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TableDeleteTarget {
    /// The rows of the selection.
    Rows,
    /// The columns of the selection.
    Columns,
    /// The whole table.
    Table,
    /// The selected cells' contents (Delete Cells ▸ shift is not offered: Word's default keeps
    /// the grid rectangular).
    CellContents,
}

/// Vertical alignment of cell content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CellVerticalAlign {
    /// Top.
    Top,
    /// Centre.
    Center,
    /// Bottom.
    Bottom,
}

/// Horizontal alignment of paragraphs (cell alignment, the alignment buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TextAlign {
    /// Left (start).
    Left,
    /// Centre.
    Center,
    /// Right (end).
    Right,
    /// Justified.
    Justify,
}

/// Borders gallery presets (Home ▸ Borders, Table Design ▸ Borders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum BorderPreset {
    /// No border.
    None,
    /// All borders (box and inside lines).
    All,
    /// Outside borders.
    Outside,
    /// Inside borders (horizontal and vertical).
    Inside,
    /// Top.
    Top,
    /// Bottom.
    Bottom,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Inside horizontal.
    InsideHorizontal,
    /// Inside vertical.
    InsideVertical,
}

/// The separator of Convert Table to Text / Convert Text to Table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum TextSeparator {
    /// Tab characters (Word's default).
    #[default]
    Tabs,
    /// Commas.
    Commas,
    /// Paragraph marks (one cell per paragraph).
    Paragraphs,
}

/// Editing commands shared by the document editors (see the module documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum EditCommand {
    /// Cut (Ctrl+X): copies the selection to the clipboard and deletes it as one undo step.
    Cut,
    /// Copy (Ctrl+C): the selection as Kiln's rich format, HTML and plain text, written to the
    /// clipboard through the host.
    Copy,
    /// Paste (Ctrl+V) keeping the source formatting. Without `data` the core asks the host for
    /// the clipboard (the ribbon button); the editor passes the data of a `paste` event.
    Paste {
        /// The clipboard contents, if the UI already has them.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Keep Source Formatting.
    PasteKeepSource {
        /// The clipboard contents, if the UI already has them.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Merge Formatting.
    PasteMerge {
        /// The clipboard contents, if the UI already has them.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Keep Text Only.
    PasteTextOnly {
        /// The clipboard contents, if the UI already has them.
        data: Option<ClipboardData>,
    },
    /// Paste Special: paste one flavour of the clipboard.
    PasteAs {
        /// The flavour.
        mode: PasteMode,
        /// The clipboard contents, if the UI already has them.
        data: Option<ClipboardData>,
    },
    /// Repeat the last action (F4): typing, a formatting command, an insertion.
    Repeat,
    /// Bold (Ctrl+B), Word's toggle rule: off when the whole selection is bold.
    Bold,
    /// Italic (Ctrl+I).
    Italic,
    /// Single underline (Ctrl+U).
    Underline,
    /// Double underline (Ctrl+Shift+D).
    DoubleUnderline,
    /// Strikethrough.
    Strikethrough,
    /// Subscript (Ctrl+=).
    Subscript,
    /// Superscript (Ctrl+Shift+=).
    Superscript,
    /// All caps (Ctrl+Shift+A).
    AllCaps,
    /// Set the font family.
    FontFamily {
        /// The family name.
        family: String,
    },
    /// Set the font size in points (1–1638, half points as in Word).
    FontSize {
        /// Points.
        size: f32,
    },
    /// Grow Font: to the next size of the preset list, or by one point (Ctrl+]).
    GrowFont {
        /// One point instead of the next preset.
        one_point: bool,
    },
    /// Shrink Font: to the previous preset size, or by one point (Ctrl+[).
    ShrinkFont {
        /// One point instead of the previous preset.
        one_point: bool,
    },
    /// Change Case; `None` cycles like Shift+F3.
    ChangeCase {
        /// The case.
        case: Option<TextCase>,
    },
    /// Clear All Formatting: the paragraph style's Normal formatting and no direct formatting.
    ClearFormatting,
    /// Font colour, `RRGGBB`; `None` is Automatic.
    FontColor {
        /// The colour.
        color: Option<String>,
    },
    /// Text highlight, `RRGGBB` matched to the nearest of Word's 15 highlight colours; `None`
    /// removes the highlight.
    Highlight {
        /// The colour.
        color: Option<String>,
    },
    /// Bullets (Ctrl+Shift+L): toggles a bulleted list, or switches to `glyph`.
    Bullets {
        /// The bullet character (`•`, `○`, `▪`, `❖`, `➢`, `✓`, `–`); `None` is Word's default.
        glyph: Option<String>,
    },
    /// Numbering: toggles a numbered list, or switches to `style`.
    Numbering {
        /// The format; `None` is `1.`.
        style: Option<ListNumberStyle>,
    },
    /// Multilevel List: Word's `1. 1.1. 1.1.1.` list (`legal`: legal numbering).
    MultilevelList {
        /// Legal-style numbering (every level in decimal).
        legal: bool,
    },
    /// Increase Indent (Ctrl+M): one list level deeper, or the left indent by 1.27 cm.
    Indent,
    /// Decrease Indent (Ctrl+Shift+M).
    Outdent,
    /// Align left (Ctrl+L).
    AlignLeft,
    /// Centre (Ctrl+E).
    AlignCenter,
    /// Align right (Ctrl+R).
    AlignRight,
    /// Justify (Ctrl+J).
    Justify,
    /// Line spacing as a multiple of single (1, 1.15, 1.5, 2, 2.5, 3).
    LineSpacing {
        /// The multiple.
        spacing: f32,
    },
    /// Insert today's date (Ctrl+;), in the document language's short format.
    InsertDate,
    /// Insert the current time (Ctrl+Shift+;).
    InsertTime,
    /// Insert ▸ Pictures ▸ This Device: the host picks an image file and the core inserts it
    /// inline at the selection.
    InsertPictures,
    /// Layout ▸ Margins preset for the sections of the selection.
    Margins {
        /// The preset.
        preset: MarginsPreset,
    },
    /// Layout ▸ Orientation.
    Orientation {
        /// The orientation.
        orientation: PageOrientation,
    },
    /// Layout ▸ Size.
    PageSize {
        /// The paper size.
        size: PaperSize,
    },
    /// Table Layout ▸ Insert Above.
    TableInsertRowAbove,
    /// Table Layout ▸ Insert Below.
    TableInsertRowBelow,
    /// Table Layout ▸ Insert Left.
    TableInsertColumnLeft,
    /// Table Layout ▸ Insert Right.
    TableInsertColumnRight,
    /// Table Layout ▸ Delete.
    TableDelete {
        /// What to delete.
        what: TableDeleteTarget,
    },
    /// Table Layout ▸ Merge Cells (a rectangular selection of cells).
    TableMergeCells,
    /// Table Layout ▸ Split Cells.
    TableSplitCells {
        /// Rows per cell after the split (1–20).
        rows: u32,
        /// Columns per cell after the split (1–20).
        cols: u32,
    },
    /// Table Layout ▸ Distribute Rows.
    TableDistributeRows,
    /// Table Layout ▸ Distribute Columns.
    TableDistributeColumns,
    /// Table Layout ▸ Alignment: the cells' vertical alignment and their paragraphs' alignment.
    TableCellAlignment {
        /// Vertical.
        vertical: CellVerticalAlign,
        /// Horizontal.
        horizontal: TextAlign,
    },
    /// Table Layout ▸ Repeat Header Rows (toggle on the selected rows).
    TableRepeatHeaderRows,
    /// Table Layout ▸ Convert to Text.
    TableConvertToText {
        /// The separator.
        separator: TextSeparator,
    },
    /// Table Design ▸ Header Row (toggle).
    TableHeaderRow,
    /// Table Design ▸ Total Row (toggle).
    TableTotalRow,
    /// Table Design ▸ Banded Rows (toggle).
    TableBandedRows,
    /// Table Design ▸ First Column (toggle).
    TableFirstColumn,
    /// Table Design ▸ Banded Columns (toggle).
    TableBandedColumns,
    /// Table Design ▸ Table Styles: apply a table style by id (`TableGrid`, `PlainTable1`,
    /// `GridTable4-Accent1`, `ListTable3-Accent1`, or one the document defines).
    TableStyle {
        /// The style id.
        style: String,
    },
    /// Table Design ▸ Shading of the selected cells, `RRGGBB`; `None` removes it.
    TableShading {
        /// The fill.
        color: Option<String>,
    },
    /// Table Design ▸ Borders of the selected cells.
    TableBorders {
        /// The preset.
        preset: BorderPreset,
    },
}
