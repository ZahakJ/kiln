// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write commands. WP-52 declares the editing commands of the scratch document; the Write work
//! packages (WP-63 onwards) own this file and append the rest of the ribbon.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppMask;
use crate::commands::edit::{BorderPreset, ClipboardData, TextAlign, TextSeparator};
use crate::geom::{Point, Rect};
use crate::selection::TextPos;

/// How [`WriteCommand::MoveCaret`] moves the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CaretMotion {
    /// One grapheme cluster back (Left in left-to-right text).
    Backward,
    /// One grapheme cluster forward.
    Forward,
    /// To the start of the previous word (Ctrl+Left).
    WordBackward,
    /// To the start of the next word (Ctrl+Right).
    WordForward,
    /// To the start of the line (Home). Without layout, the start of the paragraph.
    LineStart,
    /// To the end of the line (End). Without layout, the end of the paragraph.
    LineEnd,
    /// One line up. Without layout, the same offset in the previous paragraph.
    LineUp,
    /// One line down. Without layout, the same offset in the next paragraph.
    LineDown,
    /// To the start of the document (Ctrl+Home).
    DocumentStart,
    /// To the end of the document (Ctrl+End).
    DocumentEnd,
    /// To the start of the paragraph, or of the previous one when already there (Ctrl+Up).
    ParagraphBackward,
    /// To the start of the next paragraph (Ctrl+Down).
    ParagraphForward,
    /// One screen up (Page Up). Without layout, ten paragraphs.
    PageUp,
    /// One screen down (Page Down). Without layout, ten paragraphs.
    PageDown,
}

/// What a click selects: Word's single, double and triple click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum SelectUnit {
    /// A caret position (single click); dragging extends by characters.
    Char,
    /// The word under the point (double click); dragging extends by words.
    Word,
    /// The paragraph under the point (triple click); dragging extends by paragraphs.
    Paragraph,
}

/// Kiln Write commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum WriteCommand {
    /// Type text at the caret, replacing the selection. A line feed starts a new paragraph.
    InsertText {
        /// The text (any Unicode; `\r\n` and `\r` count as one paragraph break).
        text: String,
    },
    /// Backspace: delete the selection, or the grapheme cluster before the caret, or join the
    /// paragraph with the previous one at its start.
    DeleteBackward,
    /// Move the caret; with `extend` the selection grows from its anchor (Shift).
    MoveCaret {
        /// The movement.
        motion: CaretMotion,
        /// Extend the selection instead of collapsing it.
        extend: bool,
    },
    /// Put the caret at a position (a click), or extend the selection to it (Shift+click, drag).
    SelectTo {
        /// The position.
        pos: TextPos,
        /// Extend the selection instead of collapsing it.
        extend: bool,
    },
    /// Delete: the selection, or the grapheme cluster after the caret, or join the next paragraph
    /// at the end of one.
    DeleteForward,
    /// Ctrl+Backspace: delete the selection, or back to the start of the word.
    DeleteWordBackward,
    /// Ctrl+Delete: delete the selection, or up to the start of the next word.
    DeleteWordForward,
    /// Select everything (Ctrl+A).
    SelectAll,
    /// A click or drag at a point of the surface (document CSS pixels at zoom 1): the core hit
    /// tests its layout and moves the caret or extends the selection by `unit`.
    SelectAt {
        /// The point.
        point: Point,
        /// Extend the selection (Shift+click, drag) instead of starting a new one.
        extend: bool,
        /// Characters, words or paragraphs.
        unit: SelectUnit,
    },
    /// The IME's current composition (`compositionstart`/`compositionupdate`): replaces the
    /// selection (first time) or the previous composition with `text`, shown underlined. Not an
    /// undo step until committed.
    SetComposition {
        /// The composition string.
        text: String,
        /// Caret offset inside it, in `char`s.
        caret: u32,
    },
    /// Ends the composition (`compositionend`) by inserting `text` in its place as one typing
    /// step. Without a composition it inserts `text` at the caret.
    CommitComposition {
        /// The committed text (may differ from the last composition, or be empty).
        text: String,
    },
    /// Removes the composition string and restores the text it replaced (Escape in the IME).
    CancelComposition,
    // ── WP-64: the Write editor ─────────────────────────────────────────────────────────────
    /// Tab / Shift+Tab: the next or previous cell in a table (a new row after the last cell),
    /// one list level deeper or shallower at the start of a list item, else a tab character.
    Tab {
        /// Shift+Tab.
        back: bool,
    },
    /// Shift+Enter: a line break inside the paragraph.
    LineBreak,
    /// Show/Hide ¶ (Ctrl+Shift+8): paragraph marks, spaces, tabs and breaks on the page.
    ShowMarks,
    /// Clear paragraph formatting (Ctrl+Q): the paragraph style's properties only.
    ClearParagraphFormatting,
    /// Clear character formatting (Ctrl+Space): the style's run formatting only.
    ClearCharacterFormatting,
    /// Move the selected paragraphs up one paragraph (Alt+Shift+Up).
    MoveParagraphUp,
    /// Move the selected paragraphs down one paragraph (Alt+Shift+Down).
    MoveParagraphDown,
    /// Promote (Alt+Shift+Left): one heading level up (Heading 2 → Heading 1), or one list
    /// level shallower.
    Promote,
    /// Demote (Alt+Shift+Right): one heading level down, a body paragraph stays.
    Demote,
    /// Home ▸ Shading of the selected paragraphs, `RRGGBB`; `None` removes it.
    ParagraphShading {
        /// The fill.
        color: Option<String>,
    },
    /// Home ▸ Borders of the selected paragraphs.
    ParagraphBorders {
        /// The preset.
        preset: BorderPreset,
    },
    /// The Paragraph dialog: every field that is `Some` is set on the selected paragraphs.
    SetParagraph {
        /// The fields to set.
        format: Box<ParagraphFormat>,
    },
    /// The Font dialog: every field that is `Some` is set on the selection.
    SetFont {
        /// The fields to set.
        format: Box<FontFormat>,
    },
    /// The Tabs dialog: replaces the tab stops of the selected paragraphs.
    SetTabs {
        /// The stops (positions in points from the left margin, the text area's left edge).
        tabs: Vec<TabStopSpec>,
    },
    /// Small caps (Ctrl+Shift+K).
    SmallCaps,
    /// Underline ▾ with a line style (single, double, thick, dotted, dashed, wave, words).
    UnderlineStyle {
        /// The style; `None` removes the underline.
        style: Option<UnderlineKind>,
    },
    /// Apply a paragraph or character style by id (the Styles gallery and pane).
    ApplyStyle {
        /// The style id (`Heading1`, `Quote`, `Strong`, …).
        style_id: String,
    },
    /// Normal style (Ctrl+Shift+N).
    StyleNormal,
    /// Heading 1 (Ctrl+Alt+1).
    StyleHeading1,
    /// Heading 2 (Ctrl+Alt+2).
    StyleHeading2,
    /// Heading 3 (Ctrl+Alt+3).
    StyleHeading3,
    /// Title.
    StyleTitle,
    /// Update a style to match the selection's formatting (the paragraph's style when `None`).
    UpdateStyleFromSelection {
        /// The style id.
        style_id: Option<String>,
    },
    /// Create a paragraph style from the selection's formatting and apply it.
    NewStyleFromSelection {
        /// The style's name (also its id, without spaces).
        name: String,
    },
    /// The Modify Style dialog.
    ModifyStyle {
        /// The style id.
        style_id: String,
        /// The changes.
        spec: Box<StyleSpec>,
    },
    /// Set the list level (0–8) of the selected list paragraphs.
    ListLevel {
        /// The level.
        level: u8,
    },
    /// Restart Numbering at 1 from the caret's list item.
    RestartNumbering,
    /// Continue Numbering from the previous list.
    ContinueNumbering,
    /// Set Numbering Value of the caret's list item.
    SetNumberingValue {
        /// The value (1 or more).
        value: u32,
    },
    /// Page break (Ctrl+Enter).
    PageBreak,
    /// Column break (Ctrl+Shift+Enter).
    ColumnBreak,
    /// Blank Page: two page breaks around an empty paragraph.
    BlankPage,
    /// Layout ▸ Breaks ▸ section break.
    SectionBreak {
        /// Where the new section starts.
        kind: SectionStart,
    },
    /// Layout ▸ Columns: equal-width text columns in the selection's sections.
    Columns {
        /// Column count (1–12).
        count: u8,
    },
    /// Layout ▸ Indent Left, in centimetres.
    IndentLeft {
        /// Centimetres.
        cm: f32,
    },
    /// Layout ▸ Indent Right, in centimetres.
    IndentRight {
        /// Centimetres.
        cm: f32,
    },
    /// Layout ▸ Spacing Before, in points.
    SpacingBefore {
        /// Points.
        pt: f32,
    },
    /// Layout ▸ Spacing After, in points.
    SpacingAfter {
        /// Points.
        pt: f32,
    },
    /// The Page Setup dialog, applied to the sections of the selection.
    PageSetup {
        /// The settings.
        setup: Box<PageSetupSpec>,
    },
    /// Edit Header: puts the caret in the current page's header (creating it if needed).
    EditHeader,
    /// Edit Footer.
    EditFooter,
    /// Close Header and Footer (Esc, double-click the body).
    CloseHeaderFooter,
    /// Header & Footer ▸ Different First Page (toggle).
    DifferentFirstPage,
    /// Header & Footer ▸ Different Odd & Even Pages (toggle).
    DifferentOddEven,
    /// Header & Footer ▸ Link to Previous (toggle) for the edited header or footer.
    LinkToPrevious,
    /// Insert ▸ Page Number: a PAGE field in the header, the footer or at the caret.
    PageNumber {
        /// Where.
        position: PageNumberPosition,
    },
    /// Insert or edit a hyperlink (Ctrl+K) on the selection, or insert `text` linked.
    InsertLink {
        /// URL (`https:`, `mailto:`, …) or `#bookmark`.
        url: String,
        /// Text to insert when the selection is empty (the URL when `None`).
        text: Option<String>,
        /// ScreenTip.
        tooltip: Option<String>,
    },
    /// Remove Hyperlink from the selection.
    RemoveLink,
    /// Insert a bookmark around the selection.
    InsertBookmark {
        /// The bookmark name (letters, digits, `_`; starts with a letter).
        name: String,
    },
    /// Insert ▸ Date & Time with a format; as a DATE field when `as_field`.
    InsertDateTime {
        /// A Word date-time picture (`d MMMM yyyy`, `HH:mm`, …).
        format: String,
        /// Update automatically (a field) rather than fixed text.
        as_field: bool,
    },
    /// Quick Parts ▸ Field: insert a field from its code (`PAGE`, `NUMPAGES`, `DATE \@ "d MMM"`,
    /// `AUTHOR`, `FILENAME`, `SEQ Figure`, …).
    InsertField {
        /// The field code.
        code: String,
    },
    /// Update fields (F9) in the selection, or every field of the document when it is empty.
    UpdateFields,
    /// Insert ▸ Drop Cap on the caret's paragraph.
    DropCap {
        /// Dropped or in the margin; `None` removes it.
        placement: Option<DropCapPlacement>,
    },
    /// Insert a picture from its bytes (drag and drop, or the host's file after
    /// `suite.insert.pictures`).
    InsertPicture {
        /// The file name (its extension decides the media type).
        name: String,
        /// The image file.
        bytes: Vec<u8>,
        /// Float it (square wrapping) instead of inline.
        floating: bool,
    },
    /// Insert a table of `rows` × `cols` (the grid picker and Insert Table dialog).
    InsertTable {
        /// Rows (1–32,767).
        rows: u32,
        /// Columns (1–63).
        cols: u32,
        /// AutoFit behaviour.
        autofit: TableAutofit,
    },
    /// Convert Text to Table: the selected paragraphs, split at `separator`.
    ConvertTextToTable {
        /// The separator.
        separator: TextSeparator,
    },
    /// Table Layout ▸ AutoFit.
    TableAutofit {
        /// The behaviour.
        mode: TableAutofit,
    },
    /// Table Properties ▸ Alignment of the table on the page.
    TableAlign {
        /// Left, centre or right.
        align: TextAlign,
    },
    /// Table Layout ▸ Split Table above the caret's row.
    TableSplit,
    /// Table Layout ▸ Select.
    TableSelect {
        /// Cell, row, column or table.
        what: TableSelectTarget,
    },
    /// Table Layout ▸ Cell Size: the column width and row height at the caret, in points.
    TableCellSize {
        /// Column width; `None` keeps it.
        width: Option<f32>,
        /// Minimum row height; `None` keeps it.
        height: Option<f32>,
    },
    /// Find Next (and the Navigation pane's arrows): selects the next or previous match.
    FindNext {
        /// What to find.
        query: FindQuery,
        /// Search backwards.
        backward: bool,
    },
    /// Replace: replaces the selection when it is a match, then selects the next one.
    Replace {
        /// What to find.
        query: FindQuery,
        /// The replacement (`^&` is the found text, `\1`… groups with wildcards or regex).
        replacement: String,
    },
    /// Replace All, as one undo step.
    ReplaceAll {
        /// What to find.
        query: FindQuery,
        /// The replacement.
        replacement: String,
    },
    /// Go To (Ctrl+G, F5).
    GoTo {
        /// Where.
        target: GoToTarget,
    },
}

/// Where a new section starts (Layout ▸ Breaks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum SectionStart {
    /// Next Page.
    NextPage,
    /// Continuous.
    Continuous,
    /// Even Page.
    EvenPage,
    /// Odd Page.
    OddPage,
}

/// Where Insert ▸ Page Number puts the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PageNumberPosition {
    /// Top of Page, left.
    TopLeft,
    /// Top of Page, centred.
    TopCenter,
    /// Top of Page, right.
    TopRight,
    /// Bottom of Page, left.
    BottomLeft,
    /// Bottom of Page, centred.
    BottomCenter,
    /// Bottom of Page, right.
    BottomRight,
    /// Current Position.
    Caret,
}

/// Drop cap placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DropCapPlacement {
    /// Dropped into the text (three lines).
    Dropped,
    /// In the margin.
    InMargin,
}

/// Underline styles (Underline ▾).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum UnderlineKind {
    /// Single.
    Single,
    /// Words only.
    Words,
    /// Double.
    Double,
    /// Thick.
    Thick,
    /// Dotted.
    Dotted,
    /// Dashed.
    Dash,
    /// Dot-dash.
    DotDash,
    /// Wave.
    Wave,
}

/// Table AutoFit behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum TableAutofit {
    /// Fixed column widths, equal shares of the text column (Word's default for new tables).
    #[default]
    Fixed,
    /// AutoFit Contents.
    Contents,
    /// AutoFit Window (100 % of the text column).
    Window,
}

/// Table Layout ▸ Select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TableSelectTarget {
    /// The caret's cell.
    Cell,
    /// The caret's row.
    Row,
    /// The caret's column (its cells' text).
    Column,
    /// The whole table.
    Table,
}

/// Paragraph dialog fields; `None` leaves a property as it is. Lengths in points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct ParagraphFormat {
    /// Alignment.
    pub alignment: Option<TextAlign>,
    /// Left indent.
    pub indent_left: Option<f32>,
    /// Right indent.
    pub indent_right: Option<f32>,
    /// First-line indent (negative: hanging).
    pub first_line: Option<f32>,
    /// Space before.
    pub space_before: Option<f32>,
    /// Space after.
    pub space_after: Option<f32>,
    /// Line spacing as a multiple of single.
    pub line_multiple: Option<f32>,
    /// Exact line height, points (overrides `line_multiple`).
    pub line_exact: Option<f32>,
    /// Minimum line height, points (overrides `line_multiple`).
    pub line_at_least: Option<f32>,
    /// Keep with next.
    pub keep_next: Option<bool>,
    /// Keep lines together.
    pub keep_lines: Option<bool>,
    /// Widow/orphan control.
    pub widow_control: Option<bool>,
    /// Page break before.
    pub page_break_before: Option<bool>,
    /// Don't add space between paragraphs of the same style.
    pub contextual_spacing: Option<bool>,
    /// Outline level 1–9 (`Some(0)`: body text).
    pub outline_level: Option<u8>,
    /// Right-to-left paragraph.
    pub rtl: Option<bool>,
}

/// Font dialog fields; `None` leaves a property as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct FontFormat {
    /// Font family.
    pub family: Option<String>,
    /// Size in points.
    pub size: Option<f32>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// Underline style (`Some(None)` cannot be expressed: use `underline_none`).
    pub underline: Option<UnderlineKind>,
    /// Remove the underline.
    pub underline_none: bool,
    /// Underline colour `RRGGBB`.
    pub underline_color: Option<String>,
    /// Font colour `RRGGBB` (`auto` for Automatic).
    pub color: Option<String>,
    /// Strikethrough.
    pub strike: Option<bool>,
    /// Double strikethrough.
    pub double_strike: Option<bool>,
    /// Superscript.
    pub superscript: Option<bool>,
    /// Subscript.
    pub subscript: Option<bool>,
    /// Small caps.
    pub small_caps: Option<bool>,
    /// All caps.
    pub all_caps: Option<bool>,
    /// Hidden.
    pub hidden: Option<bool>,
    /// Character spacing in points (negative: condensed).
    pub spacing: Option<f32>,
    /// Horizontal scale in percent (1–600).
    pub scale: Option<u32>,
    /// Baseline position in points (positive: raised).
    pub position: Option<f32>,
    /// Kerning for fonts of this size (points) and above; `Some(0.0)` turns kerning off.
    pub kerning: Option<f32>,
}

/// A tab stop of the Tabs dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TabStopSpec {
    /// Position in points from the left margin (the text area's left edge).
    pub pos: f32,
    /// Alignment.
    pub align: TabAlign,
    /// Leader.
    pub leader: TabLeaderKind,
}

/// Tab stop alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TabAlign {
    /// Left.
    Left,
    /// Centre.
    Center,
    /// Right.
    Right,
    /// Decimal.
    Decimal,
    /// Bar.
    Bar,
}

/// Tab leaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TabLeaderKind {
    /// None.
    None,
    /// Dots.
    Dot,
    /// Hyphens.
    Hyphen,
    /// Underscores.
    Underscore,
}

/// Modify Style dialog fields; `None` leaves a property as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct StyleSpec {
    /// The display name.
    pub name: Option<String>,
    /// Style based on (an empty string: none).
    pub based_on: Option<String>,
    /// Style for the following paragraph.
    pub next: Option<String>,
    /// Character formatting.
    pub font: FontFormat,
    /// Paragraph formatting.
    pub paragraph: ParagraphFormat,
    /// Show in the Styles gallery.
    pub quick: Option<bool>,
}

/// Page Setup dialog settings; `None` leaves a value as it is. Lengths in points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct PageSetupSpec {
    /// Top margin.
    pub top: Option<f32>,
    /// Bottom margin.
    pub bottom: Option<f32>,
    /// Left (inside) margin.
    pub left: Option<f32>,
    /// Right (outside) margin.
    pub right: Option<f32>,
    /// Gutter.
    pub gutter: Option<f32>,
    /// Header distance from the edge.
    pub header: Option<f32>,
    /// Footer distance from the edge.
    pub footer: Option<f32>,
    /// Page width.
    pub width: Option<f32>,
    /// Page height.
    pub height: Option<f32>,
    /// Landscape.
    pub landscape: Option<bool>,
    /// Text columns (1–12).
    pub columns: Option<u8>,
    /// Space between columns.
    pub column_space: Option<f32>,
    /// Vertical alignment of the text on the page.
    pub vertical_align: Option<crate::commands::edit::CellVerticalAlign>,
}

/// What Find looks for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FindQuery {
    /// The text, wildcard pattern or regular expression. `^p` (paragraph mark), `^t` (tab)
    /// and `^?` (any character) work as in Word when neither `wildcards` nor `regex` is set.
    pub text: String,
    /// Match case.
    pub match_case: bool,
    /// Find whole words only.
    pub whole_word: bool,
    /// Word's wildcards: `?`, `*`, `[abc]`, `[a-z]`, `[!x]`, `{n,m}`, `@`, `<`, `>`, `(…)`.
    pub wildcards: bool,
    /// A regular expression (Kiln extra).
    pub regex: bool,
}

/// Go To targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum GoToTarget {
    /// A page (1-based); with `relative`, pages forward (positive) or back.
    Page {
        /// The page.
        number: i32,
        /// Relative to the current page.
        relative: bool,
    },
    /// A section (1-based).
    Section {
        /// The section.
        number: u32,
    },
    /// A line of the document (1-based).
    Line {
        /// The line.
        number: u32,
    },
    /// A bookmark by name.
    Bookmark {
        /// The name.
        name: String,
    },
    /// A heading (the Navigation pane): the paragraph.
    Paragraph {
        /// The paragraph id.
        para: kiln_common::NodeId,
    },
    /// The next (or previous) footnote.
    Footnote {
        /// Search backwards.
        backward: bool,
    },
    /// The next (or previous) comment.
    Comment {
        /// Search backwards.
        backward: bool,
    },
}

// ── Queries (Query::Write) ───────────────────────────────────────────────────────────────────

/// Questions about a Write document that the editor's panes, rulers and dialogs ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum WriteQuery {
    /// The selection's formatting (ribbon toggles and boxes, mini toolbar, dialogs).
    Format,
    /// The rulers at the caret.
    Rulers,
    /// The styles for the gallery and the Styles pane.
    Styles,
    /// The headings for the Navigation pane.
    Headings,
    /// Page geometry for the Navigation pane and the zoom presets.
    Pages,
    /// Every match of a Find query (the Navigation pane's results).
    FindAll {
        /// What to find.
        query: FindQuery,
    },
    /// The Word Count dialog.
    WordCount {
        /// Include text boxes, footnotes and endnotes.
        include_notes: bool,
    },
    /// The document properties (Backstage ▸ Info, Advanced Properties).
    Properties,
    /// The selection as clipboard flavours (what Copy would write).
    Clipboard,
    /// The Page Setup of the caret's section.
    PageSetup,
    /// Bookmark names (Go To, the Link dialog).
    Bookmarks,
    /// The commands the Write model has executed, oldest first (diagnostics, e2e).
    CommandLog,
}

/// The answer to a [`WriteQuery`], in the same variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum WriteQueryResult {
    /// Formatting.
    Format(Box<FormatState>),
    /// Rulers (`None` before the first layout).
    Rulers(Option<Box<RulerInfo>>),
    /// Styles.
    Styles(Vec<StyleInfo>),
    /// Headings.
    Headings(Vec<HeadingInfo>),
    /// Pages.
    Pages(Box<PagesInfo>),
    /// Matches.
    FindAll(FindResults),
    /// Counts.
    WordCount(Box<WordCountInfo>),
    /// Properties.
    Properties(Box<DocPropertiesInfo>),
    /// Clipboard flavours (`None` for an empty selection).
    Clipboard(Option<ClipboardData>),
    /// Page Setup (every field set).
    PageSetup(Box<PageSetupSpec>),
    /// Bookmarks.
    Bookmarks(Vec<String>),
    /// Command log.
    CommandLog(Vec<CommandLogEntry>),
}

/// The selection's formatting. Character values are those of the selection's first character
/// (or the caret's pending formatting); `mixed` lists the properties that vary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct FormatState {
    /// Font family.
    pub font: String,
    /// Size in points.
    pub size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline style.
    pub underline: Option<UnderlineKind>,
    /// Strikethrough.
    pub strike: bool,
    /// Double strikethrough.
    pub double_strike: bool,
    /// Subscript.
    pub subscript: bool,
    /// Superscript.
    pub superscript: bool,
    /// All caps.
    pub all_caps: bool,
    /// Small caps.
    pub small_caps: bool,
    /// Hidden.
    pub hidden: bool,
    /// Font colour `RRGGBB` (`None`: automatic).
    pub color: Option<String>,
    /// Highlight `RRGGBB`.
    pub highlight: Option<String>,
    /// Character spacing, points.
    pub spacing: f32,
    /// Properties that differ across the selection (`bold`, `font`, `size`, `alignment`…).
    pub mixed: Vec<String>,
    /// Paragraph alignment.
    pub alignment: Option<TextAlign>,
    /// Line spacing as a multiple of single (`None` for exact or at-least spacing).
    pub line_multiple: Option<f32>,
    /// Exact or minimum line height, points.
    pub line_points: Option<f32>,
    /// Left indent, points.
    pub indent_left: f32,
    /// Right indent, points.
    pub indent_right: f32,
    /// First-line indent, points (negative: hanging).
    pub first_line: f32,
    /// Space before, points.
    pub space_before: f32,
    /// Space after, points.
    pub space_after: f32,
    /// Keep with next.
    pub keep_next: bool,
    /// Keep lines together.
    pub keep_lines: bool,
    /// Widow/orphan control.
    pub widow_control: bool,
    /// Page break before.
    pub page_break_before: bool,
    /// Contextual spacing.
    pub contextual_spacing: bool,
    /// Outline level 1–9, 0 for body text.
    pub outline_level: u8,
    /// Paragraph style id.
    pub style_id: String,
    /// Paragraph style name.
    pub style_name: String,
    /// Character style id, if one is applied.
    pub char_style_id: Option<String>,
    /// The list the paragraph is in.
    pub list: Option<ListState>,
    /// The caret is in a table.
    pub table: Option<TableState>,
    /// The story the caret is in.
    pub story: StoryKind,
    /// Header & Footer options of the caret's section (in a header or footer).
    pub header_footer: Option<HeaderFooterState>,
    /// The page setup of the caret's section.
    pub section: SectionState,
    /// Formatting marks shown.
    pub show_marks: bool,
    /// The selection is empty.
    pub collapsed: bool,
    /// The hyperlink at the caret.
    pub link: Option<String>,
    /// F4 has something to repeat.
    pub can_repeat: bool,
    /// The paragraph's tab stops (points from the left indent).
    pub tabs: Vec<TabStopSpec>,
    /// Paragraph shading `RRGGBB`.
    pub shading: Option<String>,
    /// The paragraph has borders.
    pub borders: bool,
}

/// The list of the caret's paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListState {
    /// `w:numId`.
    pub num_id: u32,
    /// Level 0–8.
    pub level: u8,
    /// Bulleted (otherwise numbered).
    pub bullet: bool,
    /// The label of the caret's paragraph (`•`, `3.`, `1.2.`).
    pub label: String,
}

/// The table at the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TableState {
    /// Rows.
    pub rows: u32,
    /// Grid columns.
    pub cols: u32,
    /// Caret row (0-based).
    pub row: u32,
    /// Caret grid column (0-based).
    pub col: u32,
    /// Table style id.
    pub style: Option<String>,
    /// Header Row.
    pub header_row: bool,
    /// Total Row.
    pub total_row: bool,
    /// Banded Rows.
    pub banded_rows: bool,
    /// First Column.
    pub first_column: bool,
    /// Banded Columns.
    pub banded_columns: bool,
    /// The caret's row repeats as a header row.
    pub repeat_header: bool,
    /// The selection covers several cells.
    pub multi_cell: bool,
}

/// The story the caret is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum StoryKind {
    /// The body.
    #[default]
    Body,
    /// A header.
    Header,
    /// A footer.
    Footer,
    /// A footnote or endnote.
    Note,
    /// A comment.
    Comment,
    /// A text box.
    TextBox,
}

/// Header & Footer options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HeaderFooterState {
    /// Different First Page.
    pub different_first: bool,
    /// Different Odd & Even Pages.
    pub different_odd_even: bool,
    /// The edited header/footer is linked to the previous section's.
    pub linked: bool,
    /// The section has a previous section (Link to Previous applies).
    pub has_previous: bool,
    /// What is being edited (`First Page Header`, `Even Page Footer -Section 2-`, …).
    pub label: String,
}

/// The page setup of a section, in points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct SectionState {
    /// Page width.
    pub width: f32,
    /// Page height.
    pub height: f32,
    /// Landscape.
    pub landscape: bool,
    /// Top margin.
    pub top: f32,
    /// Bottom margin.
    pub bottom: f32,
    /// Left margin.
    pub left: f32,
    /// Right margin.
    pub right: f32,
    /// Text columns.
    pub columns: u8,
    /// The paper size's name (`Letter`, `A4`, or `Custom`).
    pub paper: String,
}

/// What the rulers show (document px at zoom 1; the UI scales and scrolls them).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct RulerInfo {
    /// The caret's page (0-based).
    pub page: u32,
    /// The page rectangle.
    pub page_rect: Rect,
    /// The text area of the caret's column or cell.
    pub text_rect: Rect,
    /// Column bands (x, width) of the page.
    pub columns: Vec<Rect>,
    /// Left indent from the text area's left edge.
    pub indent_left: f32,
    /// Right indent from the text area's right edge.
    pub indent_right: f32,
    /// First-line indent relative to the left indent.
    pub first_line: f32,
    /// Tab stops (position from the text area's left edge, kind).
    pub tabs: Vec<RulerTabInfo>,
    /// Default tab interval.
    pub default_tab: f32,
    /// Top margin edge (from the page's top), for the vertical ruler.
    pub margin_top: f32,
    /// Bottom margin edge (from the page's top).
    pub margin_bottom: f32,
}

/// A tab stop on the ruler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RulerTabInfo {
    /// Position from the text area's left edge, px.
    pub pos: f32,
    /// Alignment.
    pub align: TabAlign,
}

/// A style for the gallery and pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StyleInfo {
    /// Id.
    pub id: String,
    /// The name Word shows (`Heading 1`).
    pub name: String,
    /// Paragraph, character, linked (both), table or list.
    pub kind: StyleKindInfo,
    /// In the Styles gallery.
    pub quick: bool,
    /// Hidden from the pane until used.
    pub hidden: bool,
    /// Sort priority.
    pub priority: u32,
    /// Based on.
    pub based_on: Option<String>,
    /// Next style.
    pub next: Option<String>,
    /// Preview: font family, size, bold, italic, colour.
    pub font: String,
    /// Preview size, points.
    pub size: f32,
    /// Preview bold.
    pub bold: bool,
    /// Preview italic.
    pub italic: bool,
    /// Preview colour `RRGGBB`.
    pub color: Option<String>,
    /// Applied to the selection.
    pub current: bool,
    /// Used in the document.
    pub in_use: bool,
}

/// Style kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum StyleKindInfo {
    /// Paragraph.
    Paragraph,
    /// Character.
    Character,
    /// Linked (paragraph and character).
    Linked,
    /// Table.
    Table,
    /// Numbering.
    Numbering,
}

/// A heading of the Navigation pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HeadingInfo {
    /// The paragraph.
    pub para: kiln_common::NodeId,
    /// Level 1–9.
    pub level: u8,
    /// Its text.
    pub text: String,
    /// Its page (1-based), when laid out.
    pub page: Option<u32>,
    /// The caret is in this heading's part of the document.
    pub current: bool,
}

/// Page geometry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct PagesInfo {
    /// Pages laid out so far.
    pub count: u32,
    /// The layout is complete.
    pub complete: bool,
    /// The caret's page (0-based).
    pub current: u32,
    /// Page rectangles in document px (the first 2,000 pages).
    pub rects: Vec<Rect>,
    /// The whole document's extent.
    pub extent: Rect,
}

/// Matches of a Find query.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct FindResults {
    /// The matches, in document order (at most 1,000).
    pub hits: Vec<FindHit>,
    /// Every match, also beyond the listed ones.
    pub total: u32,
    /// The query is not a valid pattern (`hits` is empty).
    pub error: Option<String>,
}

/// One match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FindHit {
    /// Start.
    pub start: TextPos,
    /// End.
    pub end: TextPos,
    /// Text before the match (up to 40 chars).
    pub before: String,
    /// The match.
    pub text: String,
    /// Text after the match (up to 40 chars).
    pub after: String,
    /// The page (1-based), when laid out.
    pub page: Option<u32>,
}

/// The Word Count dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordCountInfo {
    /// Pages.
    pub pages: u32,
    /// Words.
    pub words: u32,
    /// Characters (no spaces).
    pub characters: u32,
    /// Characters (with spaces).
    pub characters_with_spaces: u32,
    /// Paragraphs.
    pub paragraphs: u32,
    /// Lines.
    pub lines: u32,
    /// The counts are of the selection.
    pub selection: bool,
}

/// Document properties.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DocPropertiesInfo {
    /// Title.
    pub title: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Keywords.
    pub keywords: Option<String>,
    /// Comments.
    pub description: Option<String>,
    /// Last modified by.
    pub last_modified_by: Option<String>,
    /// Created (W3CDTF).
    pub created: Option<String>,
    /// Modified (W3CDTF).
    pub modified: Option<String>,
    /// Revision number.
    pub revision: Option<String>,
    /// The application that wrote the file.
    pub application: Option<String>,
    /// Pages.
    pub pages: u32,
    /// Words.
    pub words: u32,
    /// Characters with spaces.
    pub characters: u32,
    /// Paragraphs.
    pub paragraphs: u32,
}

/// One command the Write model executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommandLogEntry {
    /// The catalog id.
    pub id: String,
    /// `applied`, `selection`, `unchanged` or `error`.
    pub outcome: String,
}

crate::command_table! {
    WriteCommand => WRITE_COMMANDS;
    Self::InsertText { .. } => {
        id: "write.edit.insert_text", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteBackward => {
        id: "write.edit.delete_backward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::MoveCaret { .. } => {
        id: "write.caret.move", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SelectTo { .. } => {
        id: "write.caret.select_to", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    // WP-54: editing surface (layout-aware selection, deletion, IME composition).
    Self::DeleteForward => {
        id: "write.edit.delete_forward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteWordBackward => {
        id: "write.edit.delete_word_backward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteWordForward => {
        id: "write.edit.delete_word_forward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SelectAll => {
        id: "write.caret.select_all", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SelectAt { .. } => {
        id: "write.caret.select_at", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SetComposition { .. } => {
        id: "write.ime.set_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::CommitComposition { .. } => {
        id: "write.ime.commit_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::CancelComposition => {
        id: "write.ime.cancel_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    // WP-64: the Write editor.
    Self::Tab { .. } => {
        id: "write.edit.tab", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::LineBreak => {
        id: "write.edit.line_break", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ShowMarks => {
        id: "write.paragraph.show_marks", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::ClearParagraphFormatting => {
        id: "write.paragraph.clear_paragraph_formatting", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ClearCharacterFormatting => {
        id: "write.paragraph.clear_character_formatting", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::MoveParagraphUp => {
        id: "write.paragraph.move_paragraph_up", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::MoveParagraphDown => {
        id: "write.paragraph.move_paragraph_down", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::Promote => {
        id: "write.paragraph.promote", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::Demote => {
        id: "write.paragraph.demote", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ParagraphShading { .. } => {
        id: "write.paragraph.shading", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ParagraphBorders { .. } => {
        id: "write.paragraph.borders", keytip: None, group: "paragraph",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SetParagraph { .. } => {
        id: "write.format.paragraph", keytip: None, group: "format",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SetFont { .. } => {
        id: "write.format.font", keytip: None, group: "format",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SetTabs { .. } => {
        id: "write.format.tabs", keytip: None, group: "format",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SmallCaps => {
        id: "write.format.small_caps", keytip: None, group: "format",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::UnderlineStyle { .. } => {
        id: "write.format.underline_style", keytip: None, group: "format",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ApplyStyle { .. } => {
        id: "write.styles.apply", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::StyleNormal => {
        id: "write.styles.normal", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::StyleHeading1 => {
        id: "write.styles.heading_1", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::StyleHeading2 => {
        id: "write.styles.heading_2", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::StyleHeading3 => {
        id: "write.styles.heading_3", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::StyleTitle => {
        id: "write.styles.title", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::UpdateStyleFromSelection { .. } => {
        id: "write.styles.update_from_selection", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::NewStyleFromSelection { .. } => {
        id: "write.styles.new_from_selection", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ModifyStyle { .. } => {
        id: "write.styles.modify", keytip: None, group: "styles",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ListLevel { .. } => {
        id: "write.list.set_level", keytip: None, group: "list",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::RestartNumbering => {
        id: "write.list.restart", keytip: None, group: "list",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ContinueNumbering => {
        id: "write.list.continue", keytip: None, group: "list",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SetNumberingValue { .. } => {
        id: "write.list.set_value", keytip: None, group: "list",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::PageBreak => {
        id: "write.insert.page_break", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ColumnBreak => {
        id: "write.insert.column_break", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::BlankPage => {
        id: "write.insert.blank_page", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SectionBreak { .. } => {
        id: "write.layout.section_break", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::Columns { .. } => {
        id: "write.layout.columns", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::IndentLeft { .. } => {
        id: "write.layout.indent_left", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::IndentRight { .. } => {
        id: "write.layout.indent_right", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SpacingBefore { .. } => {
        id: "write.layout.spacing_before", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SpacingAfter { .. } => {
        id: "write.layout.spacing_after", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::PageSetup { .. } => {
        id: "write.layout.page_setup", keytip: None, group: "layout",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::EditHeader => {
        id: "write.insert.header", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::EditFooter => {
        id: "write.insert.footer", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::CloseHeaderFooter => {
        id: "write.header_footer.close", keytip: None, group: "header_footer",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::DifferentFirstPage => {
        id: "write.header_footer.different_first_page", keytip: None, group: "header_footer",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DifferentOddEven => {
        id: "write.header_footer.different_odd_even", keytip: None, group: "header_footer",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::LinkToPrevious => {
        id: "write.header_footer.link_to_previous", keytip: None, group: "header_footer",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::PageNumber { .. } => {
        id: "write.insert.page_number", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertLink { .. } => {
        id: "write.insert.link", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::RemoveLink => {
        id: "write.insert.remove_link", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertBookmark { .. } => {
        id: "write.insert.bookmark", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertDateTime { .. } => {
        id: "write.insert.date_time", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertField { .. } => {
        id: "write.insert.field", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::UpdateFields => {
        id: "write.insert.update_fields", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DropCap { .. } => {
        id: "write.insert.drop_cap", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertPicture { .. } => {
        id: "write.insert.picture", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::InsertTable { .. } => {
        id: "write.table.insert", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ConvertTextToTable { .. } => {
        id: "write.insert.convert_text_to_table", keytip: None, group: "insert",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::TableAutofit { .. } => {
        id: "write.table.autofit", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::TableAlign { .. } => {
        id: "write.table.align", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::TableSplit => {
        id: "write.table.split", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::TableSelect { .. } => {
        id: "write.table.select", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::TableCellSize { .. } => {
        id: "write.table.cell_size", keytip: None, group: "table",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::FindNext { .. } => {
        id: "write.find.next", keytip: None, group: "find",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::Replace { .. } => {
        id: "write.find.replace", keytip: None, group: "find",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::ReplaceAll { .. } => {
        id: "write.find.replace_all", keytip: None, group: "find",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::GoTo { .. } => {
        id: "write.find.go_to", keytip: None, group: "find",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
}

// ── Samples and schema registration (WP-64) ──────────────────────────────────────────────────

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the Write editor's protocol types (see `crate::samples`).

    use kiln_common::NodeId;

    use super::*;
    use crate::commands::edit::{
        CellVerticalAlign, EditCommand, ListNumberStyle, MarginsPreset, PageOrientation, PaperSize,
        PasteMode, TableDeleteTarget, TextCase,
    };
    use crate::commands::{Command, SuiteCommand};
    use crate::samples::Samples;

    fn clip() -> ClipboardData {
        ClipboardData {
            text: "Kiln «Write»\tok".into(),
            html: Some("<p><b>Kiln</b> «Write»</p>".into()),
            kiln: Some("{\"v\":1}".into()),
        }
    }

    fn query() -> FindQuery {
        FindQuery {
            text: "<(K)iln>".into(),
            match_case: true,
            whole_word: false,
            wildcards: true,
            regex: false,
        }
    }

    fn paragraph() -> ParagraphFormat {
        ParagraphFormat {
            alignment: Some(TextAlign::Justify),
            indent_left: Some(36.0),
            first_line: Some(-18.0),
            space_after: Some(8.0),
            line_multiple: Some(1.08),
            keep_next: Some(true),
            outline_level: Some(0),
            ..ParagraphFormat::default()
        }
    }

    fn font() -> FontFormat {
        FontFormat {
            family: Some("Fraunces".into()),
            size: Some(10.5),
            bold: Some(true),
            underline: Some(UnderlineKind::Wave),
            color: Some("3159C6".into()),
            spacing: Some(-0.5),
            scale: Some(120),
            ..FontFormat::default()
        }
    }

    fn tab() -> TabStopSpec {
        TabStopSpec {
            pos: 216.5,
            align: TabAlign::Decimal,
            leader: TabLeaderKind::Dot,
        }
    }

    fn setup() -> PageSetupSpec {
        PageSetupSpec {
            top: Some(72.0),
            left: Some(90.0),
            width: Some(595.3),
            height: Some(841.9),
            landscape: Some(false),
            columns: Some(2),
            vertical_align: Some(CellVerticalAlign::Center),
            ..PageSetupSpec::default()
        }
    }

    fn pos(block: u64, offset: u32) -> TextPos {
        TextPos {
            block: NodeId(block),
            offset,
        }
    }

    fn rect() -> Rect {
        Rect::new(64.0, 32.0, 816.0, 1056.0)
    }

    /// One sample of every WP-64 command.
    #[must_use]
    pub fn commands() -> Vec<Command> {
        let edit = |e: EditCommand| Command::Suite(SuiteCommand::Edit(e));
        let mut out: Vec<Command> = vec![
            edit(EditCommand::Cut),
            edit(EditCommand::Copy),
            edit(EditCommand::Paste { data: None }),
            edit(EditCommand::PasteKeepSource { data: Some(clip()) }),
            edit(EditCommand::PasteMerge { data: None }),
            edit(EditCommand::PasteTextOnly { data: Some(clip()) }),
            edit(EditCommand::PasteAs {
                mode: PasteMode::Html,
                data: None,
            }),
            edit(EditCommand::Repeat),
            edit(EditCommand::Bold),
            edit(EditCommand::Italic),
            edit(EditCommand::Underline),
            edit(EditCommand::DoubleUnderline),
            edit(EditCommand::Strikethrough),
            edit(EditCommand::Subscript),
            edit(EditCommand::Superscript),
            edit(EditCommand::AllCaps),
            edit(EditCommand::FontFamily {
                family: "Figtree".into(),
            }),
            edit(EditCommand::FontSize { size: 10.5 }),
            edit(EditCommand::GrowFont { one_point: false }),
            edit(EditCommand::ShrinkFont { one_point: true }),
            edit(EditCommand::ChangeCase {
                case: Some(TextCase::Title),
            }),
            edit(EditCommand::ClearFormatting),
            edit(EditCommand::FontColor {
                color: Some("C8352B".into()),
            }),
            edit(EditCommand::Highlight { color: None }),
            edit(EditCommand::Bullets {
                glyph: Some("✓".into()),
            }),
            edit(EditCommand::Numbering {
                style: Some(ListNumberStyle::LowerRoman),
            }),
            edit(EditCommand::MultilevelList { legal: true }),
            edit(EditCommand::Indent),
            edit(EditCommand::Outdent),
            edit(EditCommand::AlignLeft),
            edit(EditCommand::AlignCenter),
            edit(EditCommand::AlignRight),
            edit(EditCommand::Justify),
            edit(EditCommand::LineSpacing { spacing: 1.15 }),
            edit(EditCommand::InsertDate),
            edit(EditCommand::InsertTime),
            edit(EditCommand::InsertPictures),
            edit(EditCommand::Margins {
                preset: MarginsPreset::Narrow,
            }),
            edit(EditCommand::Orientation {
                orientation: PageOrientation::Landscape,
            }),
            edit(EditCommand::PageSize {
                size: PaperSize::A4,
            }),
            edit(EditCommand::TableInsertRowAbove),
            edit(EditCommand::TableInsertRowBelow),
            edit(EditCommand::TableInsertColumnLeft),
            edit(EditCommand::TableInsertColumnRight),
            edit(EditCommand::TableDelete {
                what: TableDeleteTarget::Columns,
            }),
            edit(EditCommand::TableMergeCells),
            edit(EditCommand::TableSplitCells { rows: 2, cols: 3 }),
            edit(EditCommand::TableDistributeRows),
            edit(EditCommand::TableDistributeColumns),
            edit(EditCommand::TableCellAlignment {
                vertical: CellVerticalAlign::Bottom,
                horizontal: TextAlign::Right,
            }),
            edit(EditCommand::TableRepeatHeaderRows),
            edit(EditCommand::TableConvertToText {
                separator: TextSeparator::Commas,
            }),
            edit(EditCommand::TableHeaderRow),
            edit(EditCommand::TableTotalRow),
            edit(EditCommand::TableBandedRows),
            edit(EditCommand::TableFirstColumn),
            edit(EditCommand::TableBandedColumns),
            edit(EditCommand::TableStyle {
                style: "GridTable4-Accent1".into(),
            }),
            edit(EditCommand::TableShading {
                color: Some("DCE6FB".into()),
            }),
            edit(EditCommand::TableBorders {
                preset: BorderPreset::Outside,
            }),
        ];
        let write: Vec<WriteCommand> = vec![
            WriteCommand::Tab { back: true },
            WriteCommand::LineBreak,
            WriteCommand::ShowMarks,
            WriteCommand::ClearParagraphFormatting,
            WriteCommand::ClearCharacterFormatting,
            WriteCommand::MoveParagraphUp,
            WriteCommand::MoveParagraphDown,
            WriteCommand::Promote,
            WriteCommand::Demote,
            WriteCommand::ParagraphShading {
                color: Some("FFF2CC".into()),
            },
            WriteCommand::ParagraphBorders {
                preset: BorderPreset::Bottom,
            },
            WriteCommand::SetParagraph {
                format: Box::new(paragraph()),
            },
            WriteCommand::SetFont {
                format: Box::new(font()),
            },
            WriteCommand::SetTabs {
                tabs: vec![tab(), tab()],
            },
            WriteCommand::SmallCaps,
            WriteCommand::UnderlineStyle {
                style: Some(UnderlineKind::Double),
            },
            WriteCommand::ApplyStyle {
                style_id: "IntenseQuote".into(),
            },
            WriteCommand::StyleNormal,
            WriteCommand::StyleHeading1,
            WriteCommand::StyleHeading2,
            WriteCommand::StyleHeading3,
            WriteCommand::StyleTitle,
            WriteCommand::UpdateStyleFromSelection { style_id: None },
            WriteCommand::NewStyleFromSelection {
                name: "Pull Quote".into(),
            },
            WriteCommand::ModifyStyle {
                style_id: "Heading1".into(),
                spec: Box::new(StyleSpec {
                    name: Some("Heading 1".into()),
                    based_on: Some("Normal".into()),
                    next: Some("Normal".into()),
                    font: font(),
                    paragraph: paragraph(),
                    quick: Some(true),
                }),
            },
            WriteCommand::ListLevel { level: 8 },
            WriteCommand::RestartNumbering,
            WriteCommand::ContinueNumbering,
            WriteCommand::SetNumberingValue { value: 300 },
            WriteCommand::PageBreak,
            WriteCommand::ColumnBreak,
            WriteCommand::BlankPage,
            WriteCommand::SectionBreak {
                kind: SectionStart::OddPage,
            },
            WriteCommand::Columns { count: 3 },
            WriteCommand::IndentLeft { cm: 1.27 },
            WriteCommand::IndentRight { cm: 0.0 },
            WriteCommand::SpacingBefore { pt: 12.0 },
            WriteCommand::SpacingAfter { pt: 6.0 },
            WriteCommand::PageSetup {
                setup: Box::new(setup()),
            },
            WriteCommand::EditHeader,
            WriteCommand::EditFooter,
            WriteCommand::CloseHeaderFooter,
            WriteCommand::DifferentFirstPage,
            WriteCommand::DifferentOddEven,
            WriteCommand::LinkToPrevious,
            WriteCommand::PageNumber {
                position: PageNumberPosition::BottomCenter,
            },
            WriteCommand::InsertLink {
                url: "https://zahakj.github.io/kiln/".into(),
                text: Some("Kiln".into()),
                tooltip: None,
            },
            WriteCommand::RemoveLink,
            WriteCommand::InsertBookmark {
                name: "Résumé_2".into(),
            },
            WriteCommand::InsertDateTime {
                format: "d MMMM yyyy".into(),
                as_field: true,
            },
            WriteCommand::InsertField {
                code: "NUMPAGES \\* Arabic".into(),
            },
            WriteCommand::UpdateFields,
            WriteCommand::DropCap {
                placement: Some(DropCapPlacement::InMargin),
            },
            WriteCommand::InsertPicture {
                name: "kiln.png".into(),
                bytes: vec![0x89, b'P', b'N', b'G', 0, 255],
                floating: false,
            },
            WriteCommand::InsertTable {
                rows: 3,
                cols: 4,
                autofit: TableAutofit::Window,
            },
            WriteCommand::ConvertTextToTable {
                separator: TextSeparator::Tabs,
            },
            WriteCommand::TableAutofit {
                mode: TableAutofit::Contents,
            },
            WriteCommand::TableAlign {
                align: TextAlign::Center,
            },
            WriteCommand::TableSplit,
            WriteCommand::TableSelect {
                what: TableSelectTarget::Column,
            },
            WriteCommand::TableCellSize {
                width: Some(120.5),
                height: None,
            },
            WriteCommand::FindNext {
                query: query(),
                backward: true,
            },
            WriteCommand::Replace {
                query: query(),
                replacement: "\\1ln".into(),
            },
            WriteCommand::ReplaceAll {
                query: FindQuery::default(),
                replacement: String::new(),
            },
            WriteCommand::GoTo {
                target: GoToTarget::Page {
                    number: -2,
                    relative: true,
                },
            },
        ];
        out.extend(write.into_iter().map(Command::Write));
        out
    }

    fn format_state() -> FormatState {
        FormatState {
            font: "Carlito".into(),
            size: 11.0,
            bold: true,
            underline: Some(UnderlineKind::Single),
            color: Some("2F5496".into()),
            mixed: vec!["size".into()],
            alignment: Some(TextAlign::Left),
            line_multiple: Some(1.08),
            space_after: 8.0,
            style_id: "Heading1".into(),
            style_name: "Heading 1".into(),
            list: Some(ListState {
                num_id: 3,
                level: 1,
                bullet: false,
                label: "1.2.".into(),
            }),
            table: Some(TableState {
                rows: 4,
                cols: 3,
                row: 1,
                col: 2,
                style: Some("TableGrid".into()),
                header_row: true,
                total_row: false,
                banded_rows: true,
                first_column: true,
                banded_columns: false,
                repeat_header: false,
                multi_cell: true,
            }),
            story: StoryKind::Header,
            header_footer: Some(HeaderFooterState {
                different_first: true,
                different_odd_even: false,
                linked: false,
                has_previous: true,
                label: "First Page Header -Section 2-".into(),
            }),
            section: SectionState {
                width: 612.0,
                height: 792.0,
                landscape: false,
                top: 72.0,
                bottom: 72.0,
                left: 72.0,
                right: 72.0,
                columns: 1,
                paper: "Letter".into(),
            },
            link: Some("https://example.org/".into()),
            tabs: vec![tab()],
            ..FormatState::default()
        }
    }

    impl Samples for WriteQuery {
        fn samples() -> Vec<Self> {
            vec![
                WriteQuery::Format,
                WriteQuery::Rulers,
                WriteQuery::Styles,
                WriteQuery::Headings,
                WriteQuery::Pages,
                WriteQuery::FindAll { query: query() },
                WriteQuery::WordCount {
                    include_notes: true,
                },
                WriteQuery::Properties,
                WriteQuery::Clipboard,
                WriteQuery::PageSetup,
                WriteQuery::Bookmarks,
                WriteQuery::CommandLog,
            ]
        }
    }

    impl Samples for WriteQueryResult {
        fn samples() -> Vec<Self> {
            vec![
                WriteQueryResult::Format(Box::new(format_state())),
                WriteQueryResult::Rulers(None),
                WriteQueryResult::Rulers(Some(Box::new(RulerInfo {
                    page: 2,
                    page_rect: rect(),
                    text_rect: rect(),
                    columns: vec![rect()],
                    indent_left: 48.0,
                    indent_right: 0.0,
                    first_line: -24.0,
                    tabs: vec![RulerTabInfo {
                        pos: 120.0,
                        align: TabAlign::Center,
                    }],
                    default_tab: 48.0,
                    margin_top: 96.0,
                    margin_bottom: 960.0,
                }))),
                WriteQueryResult::Styles(vec![StyleInfo {
                    id: "Heading1".into(),
                    name: "Heading 1".into(),
                    kind: StyleKindInfo::Linked,
                    quick: true,
                    hidden: false,
                    priority: 9,
                    based_on: Some("Normal".into()),
                    next: Some("Normal".into()),
                    font: "Carlito".into(),
                    size: 16.0,
                    bold: false,
                    italic: false,
                    color: Some("2F5496".into()),
                    current: true,
                    in_use: true,
                }]),
                WriteQueryResult::Headings(vec![HeadingInfo {
                    para: NodeId(12),
                    level: 2,
                    text: "Résumé".into(),
                    page: Some(3),
                    current: false,
                }]),
                WriteQueryResult::Pages(Box::new(PagesInfo {
                    count: 300,
                    complete: true,
                    current: 4,
                    rects: vec![rect()],
                    extent: rect(),
                })),
                WriteQueryResult::FindAll(FindResults {
                    hits: vec![FindHit {
                        start: pos(4, 2),
                        end: pos(4, 6),
                        before: "the ".into(),
                        text: "kiln".into(),
                        after: ", warm".into(),
                        page: Some(1),
                    }],
                    total: 1_284,
                    error: None,
                }),
                WriteQueryResult::WordCount(Box::new(WordCountInfo {
                    pages: 12,
                    words: 1_284,
                    characters: 6_000,
                    characters_with_spaces: 7_311,
                    paragraphs: 80,
                    lines: 240,
                    selection: false,
                })),
                WriteQueryResult::Properties(Box::new(DocPropertiesInfo {
                    title: Some("Quarterly report".into()),
                    author: Some("Zoë".into()),
                    created: Some("2026-10-17T09:00:00Z".into()),
                    pages: 12,
                    words: 1_284,
                    ..DocPropertiesInfo::default()
                })),
                WriteQueryResult::Clipboard(Some(clip())),
                WriteQueryResult::Clipboard(None),
                WriteQueryResult::PageSetup(Box::new(setup())),
                WriteQueryResult::Bookmarks(vec!["_Toc1".into(), "intro".into()]),
                WriteQueryResult::CommandLog(vec![CommandLogEntry {
                    id: "suite.format.bold".into(),
                    outcome: "applied".into(),
                }]),
            ]
        }
    }

    crate::samples::samples_for! {
        EditCommand => commands().into_iter().filter_map(|c| match c {
            Command::Suite(SuiteCommand::Edit(e)) => Some(e),
            _ => None,
        }).collect();
        ClipboardData => vec![clip(), ClipboardData::default()];
        PasteMode => vec![PasteMode::KeepSource, PasteMode::Merge, PasteMode::TextOnly, PasteMode::Html];
        TextCase => vec![TextCase::Sentence, TextCase::Toggle];
        ListNumberStyle => vec![ListNumberStyle::Decimal, ListNumberStyle::UpperRoman];
        MarginsPreset => vec![MarginsPreset::Normal, MarginsPreset::Mirrored];
        PageOrientation => vec![PageOrientation::Portrait, PageOrientation::Landscape];
        PaperSize => vec![PaperSize::Letter, PaperSize::A5];
        TableDeleteTarget => vec![TableDeleteTarget::Rows, TableDeleteTarget::CellContents];
        CellVerticalAlign => vec![CellVerticalAlign::Top, CellVerticalAlign::Center];
        TextAlign => vec![TextAlign::Left, TextAlign::Justify];
        BorderPreset => vec![BorderPreset::None, BorderPreset::InsideVertical];
        TextSeparator => vec![TextSeparator::Tabs, TextSeparator::Paragraphs];
        SectionStart => vec![SectionStart::NextPage, SectionStart::Continuous];
        PageNumberPosition => vec![PageNumberPosition::TopLeft, PageNumberPosition::Caret];
        DropCapPlacement => vec![DropCapPlacement::Dropped];
        UnderlineKind => vec![UnderlineKind::Single, UnderlineKind::DotDash];
        TableAutofit => vec![TableAutofit::Fixed, TableAutofit::Window];
        TableSelectTarget => vec![TableSelectTarget::Cell, TableSelectTarget::Table];
        ParagraphFormat => vec![paragraph(), ParagraphFormat::default()];
        FontFormat => vec![font(), FontFormat::default()];
        TabStopSpec => vec![tab()];
        TabAlign => vec![TabAlign::Left, TabAlign::Bar];
        TabLeaderKind => vec![TabLeaderKind::None, TabLeaderKind::Underscore];
        StyleSpec => vec![StyleSpec::default()];
        PageSetupSpec => vec![setup(), PageSetupSpec::default()];
        FindQuery => vec![query(), FindQuery::default()];
        GoToTarget => vec![
            GoToTarget::Page { number: 3, relative: false },
            GoToTarget::Section { number: 2 },
            GoToTarget::Line { number: 40 },
            GoToTarget::Bookmark { name: "intro".into() },
            GoToTarget::Paragraph { para: NodeId(9) },
            GoToTarget::Footnote { backward: false },
            GoToTarget::Comment { backward: true },
        ];
        FormatState => vec![format_state(), FormatState::default()];
        ListState => format_state().list.into_iter().collect();
        TableState => format_state().table.into_iter().collect();
        StoryKind => vec![StoryKind::Body, StoryKind::TextBox];
        HeaderFooterState => format_state().header_footer.into_iter().collect();
        SectionState => vec![format_state().section];
        RulerInfo => vec![RulerInfo::default()];
        RulerTabInfo => vec![RulerTabInfo { pos: 1.5, align: TabAlign::Right }];
        StyleInfo => match WriteQueryResult::samples().remove(3) {
            WriteQueryResult::Styles(s) => s,
            _ => Vec::new(),
        };
        StyleKindInfo => vec![StyleKindInfo::Paragraph, StyleKindInfo::Numbering];
        HeadingInfo => vec![HeadingInfo { para: NodeId(1), level: 1, text: "Intro".into(), page: None, current: true }];
        PagesInfo => vec![PagesInfo::default()];
        FindResults => vec![FindResults { hits: vec![], total: 0, error: Some("unbalanced (".into()) }];
        FindHit => vec![FindHit { start: pos(1, 0), end: pos(1, 3), before: String::new(), text: "abc".into(), after: String::new(), page: None }];
        WordCountInfo => vec![WordCountInfo::default()];
        DocPropertiesInfo => vec![DocPropertiesInfo::default()];
        CommandLogEntry => vec![CommandLogEntry { id: "write.edit.tab".into(), outcome: "error".into() }];
    }

    /// Registers the WP-64 types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            EditCommand, ClipboardData, PasteMode, TextCase, ListNumberStyle, MarginsPreset,
            PageOrientation, PaperSize, TableDeleteTarget, CellVerticalAlign, TextAlign,
            BorderPreset, TextSeparator,
            SectionStart, PageNumberPosition, DropCapPlacement, UnderlineKind, TableAutofit,
            TableSelectTarget, ParagraphFormat, FontFormat, TabStopSpec, TabAlign, TabLeaderKind,
            StyleSpec, PageSetupSpec, FindQuery, GoToTarget,
            WriteQuery, WriteQueryResult, FormatState, ListState, TableState, StoryKind,
            HeaderFooterState, SectionState, RulerInfo, RulerTabInfo, StyleInfo, StyleKindInfo,
            HeadingInfo, PagesInfo, FindResults, FindHit, WordCountInfo, DocPropertiesInfo,
            CommandLogEntry,
        );
    }
}
