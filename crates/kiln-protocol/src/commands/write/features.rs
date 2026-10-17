// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of Kiln Write's advanced features (WP-66): comments, track changes,
//! tables of contents, notes, captions and cross-references, floating objects, Format Painter,
//! AutoFormat (Markdown shortcuts, smart typography), focus mode and word-count goals, code and
//! callout blocks, cover pages, document properties and spelling. The commands themselves are
//! variants of [`super::WriteCommand`].

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::TabLeaderKind;
use crate::geom::{Point, Rect};
use crate::selection::TextPos;

// ── Track changes ───────────────────────────────────────────────────────────────────────────

/// Review ▸ Display for Review: how tracked changes and comments show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum MarkupMode {
    /// Simple Markup: the final text, a bar in the margin beside changed lines, comment icons.
    #[default]
    Simple,
    /// All Markup: insertions underlined and deletions struck through in the author's colour,
    /// comments in balloons.
    All,
    /// No Markup: the final text without marks.
    None,
    /// Original: the text before the changes.
    Original,
}

/// Review ▸ Show Markup: what the markup shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum MarkupKind {
    /// Comments.
    Comments,
    /// Insertions and deletions.
    InsertionsDeletions,
    /// Formatting changes.
    Formatting,
}

/// Which changes Accept or Reject acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum ChangeScope {
    /// The changes in the selection (or the change at the caret), then select the next one
    /// (Accept and Move to Next, the button's default).
    #[default]
    CurrentAndNext,
    /// The changes in the selection or at the caret only (Accept This Change).
    Current,
    /// Every change the markup filter shows (Accept All Changes Shown).
    AllShown,
    /// Every change (Accept All Changes).
    All,
    /// Every change, then turn Track Changes off (Accept All and Stop Tracking).
    AllAndStop,
}

/// The kind of a tracked change, as the Reviewing pane names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum RevisionKindInfo {
    /// Inserted text.
    Inserted,
    /// Deleted text.
    Deleted,
    /// Text moved away.
    MovedFrom,
    /// Text moved here.
    MovedTo,
    /// Character formatting.
    Formatted,
    /// Paragraph formatting.
    ParagraphFormatted,
    /// A paragraph mark inserted (a new paragraph).
    ParagraphInserted,
    /// A paragraph mark deleted (paragraphs joined).
    ParagraphDeleted,
    /// A table row inserted.
    RowInserted,
    /// A table row deleted.
    RowDeleted,
}

/// Addresses one tracked change (from [`RevisionEntryInfo::target`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct RevisionRef {
    /// What changed.
    pub kind: RevisionKindInfo,
    /// The paragraph (or the row for row changes).
    pub node: NodeId,
    /// First char of a text change (0 otherwise).
    pub start: u32,
    /// One past the last char of a text change (0 otherwise).
    pub end: u32,
}

/// One tracked change for the Reviewing pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RevisionEntryInfo {
    /// How to address it.
    pub target: RevisionRef,
    /// Who.
    pub author: String,
    /// When (ISO 8601), if recorded.
    pub date: Option<String>,
    /// The inserted or deleted text, or a description of the formatting change.
    pub text: String,
    /// The author's colour, `RRGGBB` (the same colour the markup uses).
    pub color: String,
    /// The story (Main Document, Header, Footnote, Comment…).
    pub story: String,
    /// The change is at the caret or in the selection.
    pub current: bool,
}

/// A reviewer and the colour of their changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewerInfo {
    /// Name.
    pub name: String,
    /// `RRGGBB`.
    pub color: String,
    /// Changes by this reviewer.
    pub changes: u32,
}

/// The Reviewing pane.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RevisionsInfo {
    /// Every tracked change in document order.
    pub entries: Vec<RevisionEntryInfo>,
    /// Reviewers in order of their first change.
    pub reviewers: Vec<ReviewerInfo>,
    /// Insertions.
    pub insertions: u32,
    /// Deletions.
    pub deletions: u32,
    /// Formatting changes.
    pub formatting: u32,
    /// Comments.
    pub comments: u32,
}

// ── Comments ────────────────────────────────────────────────────────────────────────────────

/// One comment of a thread for the Comments pane and the balloons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CommentInfo {
    /// The comment.
    pub id: NodeId,
    /// The comment it replies to.
    pub parent: Option<NodeId>,
    /// Author.
    pub author: String,
    /// Initials (for the avatar).
    pub initials: String,
    /// When (ISO 8601).
    pub date: Option<String>,
    /// The comment's text (paragraphs joined by line feeds).
    pub text: String,
    /// The thread is resolved.
    pub resolved: bool,
    /// The commented text (top-level comments).
    pub anchor_text: Option<String>,
    /// The author's colour, `RRGGBB`.
    pub color: String,
    /// The page of the anchor (1-based), when laid out.
    pub page: Option<u32>,
    /// The balloon's rectangle in document coordinates (All Markup, top-level comments).
    pub balloon: Option<Rect>,
    /// Where the commented text starts, document coordinates.
    pub anchor: Option<Point>,
    /// The caret is in the commented text.
    pub current: bool,
}

// ── Table of contents, notes, captions, cross-references ────────────────────────────────────

/// The Table of Contents dialog (and the gallery's built-in tables).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TocSpec {
    /// Show levels 1–9.
    pub levels: u8,
    /// Entries link to their headings.
    pub hyperlinks: bool,
    /// Show page numbers.
    pub page_numbers: bool,
    /// Right-align page numbers (at a right tab with the leader).
    pub right_align: bool,
    /// Tab leader.
    pub leader: TabLeaderKind,
    /// A heading above the table ("Contents"), in the TOC Heading style.
    pub title: Option<String>,
}

impl Default for TocSpec {
    fn default() -> Self {
        Self {
            levels: 3,
            hyperlinks: true,
            page_numbers: true,
            right_align: true,
            leader: TabLeaderKind::Dot,
            title: Some("Contents".into()),
        }
    }
}

/// Footnote and Endnote dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoteOptionsSpec {
    /// Endnotes (otherwise footnotes).
    pub endnotes: bool,
    /// Number format: `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`, `upperLetter`,
    /// `chicago` (*, †, ‡, §).
    pub format: String,
    /// Start at.
    pub start: u32,
    /// `continuous`, `eachSect` or `eachPage` (footnotes).
    pub restart: String,
}

/// Where a caption goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum CaptionPosition {
    /// Below the selected item (Word's default for figures).
    #[default]
    Below,
    /// Above the selected item (Word's default for tables).
    Above,
}

/// The Insert Caption dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CaptionSpec {
    /// The label (`Figure`, `Table`, `Equation`, or a new label).
    pub label: String,
    /// The text after the number (`: Glaze test tiles`).
    pub text: String,
    /// Above or below the picture or table at the selection.
    pub position: CaptionPosition,
    /// Leave the label out ("1: Glaze test tiles").
    pub exclude_label: bool,
    /// Include the chapter number from headings of this level (1–9).
    pub chapter_level: Option<u8>,
}

/// What a cross-reference points to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CrossRefTarget {
    /// A heading paragraph.
    Heading {
        /// The heading.
        para: NodeId,
    },
    /// A bookmark.
    Bookmark {
        /// Its name.
        name: String,
    },
    /// A caption (figure, table, equation) paragraph.
    Caption {
        /// The caption paragraph.
        para: NodeId,
    },
    /// A footnote or endnote.
    Note {
        /// The note.
        note: NodeId,
    },
}

/// What a cross-reference shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum CrossRefShow {
    /// The text (heading text, bookmark text, the whole caption).
    #[default]
    Text,
    /// The page number.
    PageNumber,
    /// The number (heading number, "Figure 3" as label and number, the note number).
    Number,
    /// "above" or "below".
    AboveBelow,
}

/// The Cross-reference dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CrossRefSpec {
    /// The target.
    pub target: CrossRefTarget,
    /// What to insert.
    pub show: CrossRefShow,
    /// Insert as a hyperlink.
    pub hyperlink: bool,
}

/// One target of the Cross-reference dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CrossRefItem {
    /// The target.
    pub target: CrossRefTarget,
    /// What the list shows (`1.2 Glazes`, `Figure 3: Tiles`, `[1] Source`).
    pub label: String,
    /// The category (`Heading`, `Bookmark`, `Figure`, `Table`, `Equation`, `Footnote`, `Endnote`).
    pub category: String,
    /// Indent level in the list (headings).
    pub level: u8,
}

// ── Floating objects ────────────────────────────────────────────────────────────────────────

/// How text wraps around an object (Wrap Text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum WrapMode {
    /// In Line with Text.
    #[default]
    InLine,
    /// Square.
    Square,
    /// Tight.
    Tight,
    /// Through.
    Through,
    /// Top and Bottom.
    TopAndBottom,
    /// Behind Text.
    Behind,
    /// In Front of Text.
    InFront,
}

/// Position presets: in line, or one of nine places on the page with square wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PositionPreset {
    /// In Line with Text.
    InLine,
    /// Top left.
    TopLeft,
    /// Top centre.
    TopCenter,
    /// Top right.
    TopRight,
    /// Middle left.
    MiddleLeft,
    /// Middle centre.
    MiddleCenter,
    /// Middle right.
    MiddleRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom centre.
    BottomCenter,
    /// Bottom right.
    BottomRight,
}

/// Bring Forward / Send Backward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ZOrder {
    /// Bring Forward.
    Forward,
    /// Bring to Front.
    ToFront,
    /// Send Backward.
    Backward,
    /// Send to Back.
    ToBack,
}

/// Rotate ▾.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum RotateOp {
    /// Rotate Right 90°.
    Right90,
    /// Rotate Left 90°.
    Left90,
    /// Flip Vertical.
    FlipVertical,
    /// Flip Horizontal.
    FlipHorizontal,
}

/// Align ▾ (relative to the margins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ObjectAlign {
    /// Align Left.
    Left,
    /// Align Center.
    Center,
    /// Align Right.
    Right,
    /// Align Top.
    Top,
    /// Align Middle.
    Middle,
    /// Align Bottom.
    Bottom,
}

/// What a floating position is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum RelativeTo {
    /// The page edge.
    #[default]
    Page,
    /// The margin.
    Margin,
    /// The column (horizontal) or the paragraph (vertical).
    ColumnOrParagraph,
}

/// The Layout dialog (More Layout Options): every field that is `Some` is set. Lengths in points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct ObjectLayoutSpec {
    /// Text wrapping.
    pub wrap: Option<WrapMode>,
    /// Absolute horizontal position.
    pub x: Option<f32>,
    /// What `x` is measured from.
    pub x_from: Option<RelativeTo>,
    /// Absolute vertical position.
    pub y: Option<f32>,
    /// What `y` is measured from.
    pub y_from: Option<RelativeTo>,
    /// Width.
    pub width: Option<f32>,
    /// Height.
    pub height: Option<f32>,
    /// Rotation in degrees (clockwise).
    pub rotation: Option<f32>,
    /// Distance from text on every side.
    pub distance: Option<f32>,
    /// Move with the paragraph (vertical position relative to the paragraph).
    pub move_with_text: Option<bool>,
    /// Allow overlap.
    pub allow_overlap: Option<bool>,
    /// The name (Selection pane).
    pub name: Option<String>,
    /// Alt text.
    pub alt_text: Option<String>,
    /// Decorative (no alt text needed).
    pub decorative: Option<bool>,
}

/// The selected floating or inline object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ObjectInfo {
    /// The drawing.
    pub id: NodeId,
    /// Its name (`Picture 1`, `Text Box 2`).
    pub name: String,
    /// `Picture`, `Shape`, `TextBox`, `Chart` or `Other`.
    pub kind: String,
    /// Wrapping.
    pub wrap: WrapMode,
    /// The paragraph holding it.
    pub para: NodeId,
    /// Its char offset there.
    pub offset: u32,
    /// The rectangle on its page, document coordinates (when laid out).
    pub rect: Option<Rect>,
    /// Size in points.
    pub width: f32,
    /// Size in points.
    pub height: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Z-order (higher is in front).
    pub z: i64,
    /// Alt text.
    pub alt_text: Option<String>,
    /// Decorative.
    pub decorative: bool,
    /// The selection is this object.
    pub selected: bool,
}

// ── Editing options ─────────────────────────────────────────────────────────────────────────

/// Options ▸ Proofing ▸ AutoFormat As You Type (Kiln's Markdown shortcuts and typography).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(clippy::struct_excessive_bools)]
pub struct AutoFormatOptions {
    /// Markdown shortcuts: `# ` headings, `- ` bullets, `1. ` numbering, `> ` quotes, `---`
    /// rules, `**bold**`, `*italic*`, `` `code` ``.
    pub markdown: bool,
    /// "Straight quotes" with “smart quotes”.
    pub smart_quotes: bool,
    /// Hyphens (--) with dashes (—), and ` - ` with an en dash.
    pub dashes: bool,
    /// `...` with an ellipsis (…).
    pub ellipsis: bool,
    /// Fractions (1/2) with fraction characters (½).
    pub fractions: bool,
}

impl Default for AutoFormatOptions {
    fn default() -> Self {
        Self {
            markdown: true,
            smart_quotes: true,
            dashes: true,
            ellipsis: true,
            fractions: true,
        }
    }
}

/// Insert ▸ Callout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CalloutKind {
    /// A note (blue).
    Note,
    /// A tip (green).
    Tip,
    /// A warning (amber).
    Warning,
    /// Important (red).
    Important,
}

/// The cover page gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CoverDesign {
    /// A large serif title on a calm page.
    Kiln,
    /// A band of colour across the top.
    Banded,
    /// A rule under a left-aligned title.
    Ledger,
    /// Centred, classic.
    Classic,
    /// A dark slab behind the title.
    Slab,
    /// A minimal page: title and date at the bottom.
    Quiet,
}

/// File ▸ Info ▸ Properties: every field that is `Some` is set (an empty string clears it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PropertiesEdit {
    /// Title.
    pub title: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Keywords (tags).
    pub keywords: Option<String>,
    /// Comments.
    pub description: Option<String>,
    /// Category.
    pub category: Option<String>,
    /// Status.
    pub status: Option<String>,
    /// Company.
    pub company: Option<String>,
    /// Manager.
    pub manager: Option<String>,
}

// ── Spelling ────────────────────────────────────────────────────────────────────────────────

/// A misspelled word with suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SpellingHit {
    /// The word.
    pub word: String,
    /// Where it starts.
    pub start: TextPos,
    /// Its length in chars.
    pub len: u32,
    /// Corrections, best first.
    pub suggestions: Vec<String>,
    /// The sentence around it (the Spelling dialog's "Not in Dictionary" box).
    pub context: String,
    /// The word's char offset inside `context`.
    pub context_offset: u32,
}

// ── State reported with the formatting ──────────────────────────────────────────────────────

/// The review and feature state of the document (ribbon toggles, status bar, panes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[allow(clippy::struct_excessive_bools)]
pub struct FeatureState {
    /// Track Changes is on.
    pub tracking: bool,
    /// The reviewer's name used for new comments and changes.
    pub author: String,
    /// Display for Review.
    pub markup: MarkupMode,
    /// Show Markup ▸ Comments.
    pub show_comments: bool,
    /// Show Markup ▸ Insertions and Deletions.
    pub show_insertions: bool,
    /// Show Markup ▸ Formatting.
    pub show_formatting: bool,
    /// Tracked changes in the document.
    pub revisions: u32,
    /// Comments in the document (top level).
    pub comments: u32,
    /// Top-level comments not resolved (the Comments button's badge).
    pub open_comments: u32,
    /// The comment whose range holds the caret.
    pub comment: Option<NodeId>,
    /// Format Painter is armed (`Some(true)`: double-clicked, it stays on).
    pub painter: Option<bool>,
    /// Copied formatting is waiting for Ctrl+Shift+V.
    pub copied_format: bool,
    /// Focus mode.
    pub focus: bool,
    /// The word-count goal (0: none).
    pub word_goal: u32,
    /// AutoFormat As You Type options.
    pub autoformat: AutoFormatOptions,
    /// Check spelling as you type.
    pub spelling: bool,
    /// The spelling dictionary is loaded (the web core gets it from the page).
    pub dictionary: bool,
    /// Misspellings found in the laid-out pages so far.
    pub misspellings: u32,
    /// Automatic hyphenation.
    pub hyphenation: bool,
    /// The selected object.
    pub object: Option<ObjectInfo>,
    /// The caret is in a footnote or endnote (Show Notes goes back to the reference).
    pub in_note: bool,
    /// A table of contents exists.
    pub has_toc: bool,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the feature types (see `crate::samples`).

    use super::*;
    use crate::commands::{Command, WriteCommand};
    use crate::samples::Samples;

    fn rect() -> Rect {
        Rect::new(880.5, 120.0, 240.0, 72.25)
    }

    fn pos() -> TextPos {
        TextPos {
            block: NodeId(70_000),
            offset: 300,
        }
    }

    fn rref() -> RevisionRef {
        RevisionRef {
            kind: RevisionKindInfo::Deleted,
            node: NodeId(4_097),
            start: 3,
            end: 200,
        }
    }

    fn object() -> ObjectInfo {
        ObjectInfo {
            id: NodeId(1_000_000),
            name: "Picture «1»".into(),
            kind: "Picture".into(),
            wrap: WrapMode::Square,
            para: NodeId(12),
            offset: 0,
            rect: Some(rect()),
            width: 144.5,
            height: -0.0,
            rotation: 90.0,
            z: 251_659_264,
            alt_text: Some("Glaze tiles in the kiln — 🔥".into()),
            decorative: false,
            selected: true,
        }
    }

    fn comment() -> CommentInfo {
        CommentInfo {
            id: NodeId(300),
            parent: None,
            author: "Zoë Ångström".into(),
            initials: "ZÅ".into(),
            date: Some("2026-10-17T09:30:00Z".into()),
            text: "Check the cone «10» temperature.\nSecond line".into(),
            resolved: false,
            anchor_text: Some("cone 10".into()),
            color: "3159C6".into(),
            page: Some(2),
            balloon: Some(rect()),
            anchor: Some(Point { x: 96.0, y: 1_500.5 }),
            current: true,
        }
    }

    /// One sample of every WP-66 command.
    #[must_use]
    pub fn commands() -> Vec<Command> {
        use WriteCommand as W;
        vec![
            W::NewComment {
                text: "Is «cone 10» right? 🔥".into(),
            },
            W::ReplyComment {
                parent: NodeId(300),
                text: "Yes.".into(),
            },
            W::EditComment {
                id: NodeId(300),
                text: String::new(),
            },
            W::ResolveComment {
                id: NodeId(300),
                resolved: true,
            },
            W::DeleteComment {
                id: Some(NodeId(70_000)),
            },
            W::DeleteAllComments,
            W::SelectComment { id: NodeId(300) },
            W::TrackChanges,
            W::SetReviewer {
                name: "Zoë Ångström".into(),
            },
            W::MarkupMode {
                mode: MarkupMode::Original,
            },
            W::ShowMarkup {
                kind: MarkupKind::Formatting,
            },
            W::AcceptChange {
                scope: ChangeScope::AllAndStop,
            },
            W::RejectChange {
                scope: ChangeScope::CurrentAndNext,
            },
            W::ResolveRevision {
                target: rref(),
                accept: false,
            },
            W::PreviousChange,
            W::NextChange,
            W::InsertToc {
                spec: Box::new(TocSpec {
                    levels: 9,
                    hyperlinks: false,
                    page_numbers: true,
                    right_align: false,
                    leader: TabLeaderKind::Underscore,
                    title: None,
                }),
            },
            W::InsertQuickToc,
            W::RemoveToc,
            W::UpdateToc { entire: true },
            W::AddText { level: 2 },
            W::InsertTableOfFigures {
                label: "Figure".into(),
            },
            W::InsertFootnote,
            W::InsertEndnote,
            W::NextNote { backward: true },
            W::ShowNotes,
            W::NoteOptions {
                spec: Box::new(NoteOptionsSpec {
                    endnotes: true,
                    format: "lowerRoman".into(),
                    start: 3,
                    restart: "eachSect".into(),
                }),
            },
            W::ConvertNotes { to_endnotes: false },
            W::InsertCaption {
                spec: Box::new(CaptionSpec {
                    label: "Tableau".into(),
                    text: ": Glaze «tests»".into(),
                    position: CaptionPosition::Above,
                    exclude_label: false,
                    chapter_level: Some(1),
                }),
            },
            W::InsertCrossReference {
                spec: Box::new(CrossRefSpec {
                    target: CrossRefTarget::Bookmark {
                        name: "_Ref123".into(),
                    },
                    show: CrossRefShow::AboveBelow,
                    hyperlink: true,
                }),
            },
            W::FollowLink,
            W::SelectObject { id: NodeId(1_000_000) },
            W::ObjectPosition {
                preset: PositionPreset::BottomCenter,
            },
            W::ObjectWrap {
                wrap: WrapMode::TopAndBottom,
            },
            W::ObjectTransform {
                id: NodeId(1_000_000),
                rect: rect(),
            },
            W::ObjectLayout {
                id: None,
                spec: Box::new(ObjectLayoutSpec {
                    wrap: Some(WrapMode::Behind),
                    x: Some(-12.5),
                    x_from: Some(RelativeTo::Margin),
                    rotation: Some(359.5),
                    name: Some("Logo «kiln»".into()),
                    ..ObjectLayoutSpec::default()
                }),
            },
            W::ObjectOrder {
                order: ZOrder::ToBack,
            },
            W::ObjectRotate {
                op: RotateOp::FlipHorizontal,
            },
            W::ObjectAlign {
                align: ObjectAlign::Middle,
            },
            W::InsertTextBox {
                text: Some("Pull quote".into()),
            },
            W::InsertShape {
                preset: "roundRect".into(),
                text: None,
            },
            W::FormatPainter { sticky: true },
            W::CopyFormat,
            W::PasteFormat,
            W::CancelPainter,
            W::SetAutoFormat {
                options: AutoFormatOptions {
                    markdown: false,
                    smart_quotes: true,
                    dashes: false,
                    ellipsis: true,
                    fractions: false,
                },
            },
            W::PasteMarkdown {
                text: "# Kiln\n\n- **glaze**\n- `cone`\n\n| a | b |\n|---|---|\n| 1 | 2 |".into(),
            },
            W::FocusMode,
            W::WordGoal { words: 1_500 },
            W::Hyphenation,
            W::InsertCodeBlock {
                language: Some("rust".into()),
            },
            W::InsertCallout {
                kind: CalloutKind::Warning,
            },
            W::InsertCoverPage {
                design: CoverDesign::Slab,
            },
            W::SetProperties {
                props: Box::new(PropertiesEdit {
                    title: Some("Firing «log»".into()),
                    keywords: Some(String::new()),
                    ..PropertiesEdit::default()
                }),
            },
            W::SpellCheck { on: false },
            W::SetUserDictionary {
                words: vec!["Kiln".into(), "Zoë".into()],
            },
            W::SpellAddWord {
                word: "glazeware".into(),
            },
            W::SpellIgnoreAll {
                word: "teh".into(),
            },
            W::SpellIgnore { pos: pos() },
            W::SpellReplace {
                pos: pos(),
                with: "the".into(),
                all: true,
            },
            W::SpellNext,
            W::SpellDictionary {
                aff: "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz".into(),
                dic: "2\nkiln/S\nglaze/DSG".into(),
            },
        ]
        .into_iter()
        .map(Command::Write)
        .collect()
    }

    crate::samples::samples_for! {
        MarkupMode => vec![MarkupMode::Simple, MarkupMode::All, MarkupMode::None, MarkupMode::Original];
        MarkupKind => vec![MarkupKind::Comments, MarkupKind::InsertionsDeletions];
        ChangeScope => vec![ChangeScope::Current, ChangeScope::All];
        RevisionKindInfo => vec![RevisionKindInfo::Inserted, RevisionKindInfo::RowDeleted];
        RevisionRef => vec![rref()];
        RevisionEntryInfo => vec![RevisionEntryInfo {
            target: rref(),
            author: "Ana".into(),
            date: None,
            text: "«glaze» — 🔥".into(),
            color: "C2410C".into(),
            story: "Main Document".into(),
            current: false,
        }];
        ReviewerInfo => vec![ReviewerInfo { name: "Ana".into(), color: "3159C6".into(), changes: 300 }];
        RevisionsInfo => vec![RevisionsInfo::default(), RevisionsInfo {
            entries: Vec::new(),
            reviewers: vec![ReviewerInfo { name: "Bo".into(), color: "0F766E".into(), changes: 1 }],
            insertions: 70_000,
            deletions: 2,
            formatting: 0,
            comments: 128,
        }];
        CommentInfo => vec![comment(), CommentInfo {
            parent: Some(NodeId(300)),
            balloon: None,
            anchor: None,
            page: None,
            date: None,
            anchor_text: None,
            current: false,
            ..comment()
        }];
        TocSpec => vec![TocSpec::default()];
        NoteOptionsSpec => vec![NoteOptionsSpec {
            endnotes: false,
            format: "chicago".into(),
            start: 1,
            restart: "eachPage".into(),
        }];
        CaptionPosition => vec![CaptionPosition::Below, CaptionPosition::Above];
        CaptionSpec => vec![CaptionSpec {
            label: "Figure".into(),
            text: String::new(),
            position: CaptionPosition::Below,
            exclude_label: true,
            chapter_level: None,
        }];
        CrossRefTarget => vec![
            CrossRefTarget::Heading { para: NodeId(9) },
            CrossRefTarget::Caption { para: NodeId(300) },
            CrossRefTarget::Note { note: NodeId(70_000) },
        ];
        CrossRefShow => vec![CrossRefShow::Text, CrossRefShow::PageNumber, CrossRefShow::Number];
        CrossRefSpec => vec![CrossRefSpec {
            target: CrossRefTarget::Heading { para: NodeId(9) },
            show: CrossRefShow::Number,
            hyperlink: false,
        }];
        CrossRefItem => vec![CrossRefItem {
            target: CrossRefTarget::Heading { para: NodeId(9) },
            label: "1.2 Glazes — «Ünïcödé»".into(),
            category: "Heading".into(),
            level: 2,
        }];
        WrapMode => vec![WrapMode::InLine, WrapMode::Tight, WrapMode::InFront];
        PositionPreset => vec![PositionPreset::InLine, PositionPreset::TopLeft, PositionPreset::MiddleRight];
        ZOrder => vec![ZOrder::Forward, ZOrder::Backward];
        RotateOp => vec![RotateOp::Right90, RotateOp::Left90];
        ObjectAlign => vec![ObjectAlign::Left, ObjectAlign::Bottom];
        RelativeTo => vec![RelativeTo::Page, RelativeTo::ColumnOrParagraph];
        ObjectLayoutSpec => vec![ObjectLayoutSpec::default(), ObjectLayoutSpec {
            y: Some(1_440.75),
            y_from: Some(RelativeTo::Page),
            width: Some(200.0),
            height: Some(100.0),
            distance: Some(9.0),
            move_with_text: Some(false),
            allow_overlap: Some(true),
            alt_text: Some(String::new()),
            decorative: Some(true),
            ..ObjectLayoutSpec::default()
        }];
        ObjectInfo => vec![object(), ObjectInfo { rect: None, alt_text: None, ..object() }];
        AutoFormatOptions => vec![AutoFormatOptions::default()];
        CalloutKind => vec![CalloutKind::Note, CalloutKind::Tip, CalloutKind::Important];
        CoverDesign => vec![CoverDesign::Kiln, CoverDesign::Banded, CoverDesign::Quiet];
        PropertiesEdit => vec![PropertiesEdit::default(), PropertiesEdit {
            author: Some("Zoë".into()),
            subject: Some(String::new()),
            ..PropertiesEdit::default()
        }];
        SpellingHit => vec![SpellingHit {
            word: "teh".into(),
            start: pos(),
            len: 3,
            suggestions: vec!["the".into(), "ten".into()],
            context: "Fire teh kiln — «slowly».".into(),
            context_offset: 5,
        }];
        FeatureState => vec![FeatureState::default(), FeatureState {
            tracking: true,
            author: "Zoë".into(),
            markup: MarkupMode::All,
            show_comments: true,
            show_insertions: true,
            show_formatting: false,
            revisions: 300,
            comments: 2,
            open_comments: 1,
            comment: Some(NodeId(300)),
            painter: Some(false),
            copied_format: true,
            focus: true,
            word_goal: 70_000,
            autoformat: AutoFormatOptions::default(),
            spelling: true,
            dictionary: true,
            misspellings: 1,
            hyphenation: true,
            object: Some(object()),
            in_note: false,
            has_toc: true,
        }];
    }

    /// Registers the feature types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            MarkupMode, MarkupKind, ChangeScope, RevisionKindInfo, RevisionRef, RevisionEntryInfo,
            ReviewerInfo, RevisionsInfo, CommentInfo, TocSpec, NoteOptionsSpec, CaptionPosition,
            CaptionSpec, CrossRefTarget, CrossRefShow, CrossRefSpec, CrossRefItem, WrapMode,
            PositionPreset, ZOrder, RotateOp, ObjectAlign, RelativeTo, ObjectLayoutSpec,
            ObjectInfo, AutoFormatOptions, CalloutKind, CoverDesign, PropertiesEdit, SpellingHit,
            FeatureState,
        );
    }
}
