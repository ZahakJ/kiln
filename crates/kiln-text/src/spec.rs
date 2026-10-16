// SPDX-License-Identifier: AGPL-3.0-or-later
//! The input of paragraph layout: styled text runs plus paragraph properties.
//!
//! All lengths are CSS pixels (1 px = 0.75 pt = 1/96 in). Callers convert from twips or EMU.

/// A colour as `0xRRGGBBAA`.
pub type Rgba = u32;

/// Opaque black.
pub const BLACK: Rgba = 0x0000_00ff;

/// Underline styles (the common subset of Word's `w:u` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UnderlineKind {
    /// No underline.
    #[default]
    None,
    /// One line.
    Single,
    /// One line under words only (not under spaces).
    Words,
    /// Two lines.
    Double,
    /// One thick line.
    Thick,
    /// Dotted line.
    Dotted,
    /// Dashed line.
    Dashed,
    /// Wavy line.
    Wavy,
}

/// Strikethrough styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StrikeKind {
    /// No strikethrough.
    #[default]
    None,
    /// One line.
    Single,
    /// Two lines.
    Double,
}

/// Character formatting of one run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanStyle {
    /// The requested family name, as written in the document (e.g. "Calibri"); it goes through the
    /// substitution table.
    pub family: String,
    /// Font size in px.
    pub size: f32,
    /// Weight, 100–900 (400 regular, 700 bold).
    pub weight: u16,
    /// Italic.
    pub italic: bool,
    /// Text colour.
    pub color: Rgba,
    /// Underline.
    pub underline: UnderlineKind,
    /// Strikethrough.
    pub strike: StrikeKind,
    /// Extra space after every character, in px (Word's character spacing "Expanded by").
    pub letter_spacing: f32,
    /// Baseline shift in px, positive raises the text (superscript), negative lowers it.
    pub baseline_shift: f32,
    /// BCP 47 language tag (selects hyphenation patterns, `locl` forms and CJK fallback fonts).
    pub lang: Option<String>,
    /// Word's run-level `w:rtl`: neutral characters (spaces, punctuation) in this run resolve
    /// right-to-left.
    pub rtl: bool,
    /// Apply pair kerning (`kern`). Word only kerns when asked to, so documents set this explicitly.
    pub kerning: bool,
    /// Apply standard ligatures (`liga`).
    pub ligatures: bool,
}

impl Default for SpanStyle {
    fn default() -> Self {
        Self {
            family: "Calibri".to_owned(),
            size: 11.0 * 4.0 / 3.0,
            weight: 400,
            italic: false,
            color: BLACK,
            underline: UnderlineKind::None,
            strike: StrikeKind::None,
            letter_spacing: 0.0,
            baseline_shift: 0.0,
            lang: None,
            rtl: false,
            kerning: true,
            ligatures: true,
        }
    }
}

impl SpanStyle {
    /// A style with the given family and size (px), everything else default.
    #[must_use]
    pub fn new(family: impl Into<String>, size: f32) -> Self {
        Self {
            family: family.into(),
            size,
            ..Self::default()
        }
    }
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The text. `\t` is a tab; `\n` (or U+2028) is a manual line break (Word's `w:br`); U+00AD is a
    /// soft hyphen.
    pub text: String,
    /// Its style.
    pub style: SpanStyle,
}

impl TextRun {
    /// A run of `text` in `style`.
    #[must_use]
    pub fn new(text: impl Into<String>, style: SpanStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// Horizontal alignment of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Alignment {
    /// The start edge (left in LTR paragraphs, right in RTL ones).
    #[default]
    Start,
    /// Centred between the indents.
    Center,
    /// The end edge.
    End,
    /// Always the left edge.
    Left,
    /// Always the right edge.
    Right,
    /// Both edges: space between words is stretched on every line except the last.
    Justify,
    /// Both edges on every line including the last, stretching space between all characters
    /// (Word's "Distributed").
    Distribute,
}

/// Paragraph direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left (Word's `w:bidi`).
    Rtl,
    /// From the first strong character (UAX #9 rules P2–P3).
    Auto,
}

/// How a tab stop aligns the text that follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TabAlignment {
    /// Text starts at the stop.
    #[default]
    Start,
    /// Text is centred on the stop.
    Center,
    /// Text ends at the stop.
    End,
    /// The decimal separator sits on the stop.
    Decimal,
}

/// The characters drawn in the space a tab covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TabLeader {
    /// Blank.
    #[default]
    None,
    /// `.....`
    Dot,
    /// `-----`
    Hyphen,
    /// `_____`
    Underscore,
    /// A heavy rule.
    Heavy,
    /// `·····`
    MiddleDot,
}

/// A custom tab stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStop {
    /// Distance from the start edge of the text area (not from the indent), in px.
    pub position: f32,
    /// Alignment.
    pub alignment: TabAlignment,
    /// Leader.
    pub leader: TabLeader,
}

impl TabStop {
    /// A stop at `position` px.
    #[must_use]
    pub fn new(position: f32, alignment: TabAlignment, leader: TabLeader) -> Self {
        Self {
            position,
            alignment,
            leader,
        }
    }
}

/// Line spacing (Word's `w:spacing/@w:lineRule`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineSpacing {
    /// A multiple of the single line height (1.0 = single, 2.0 = double; `auto` in OOXML).
    Multiple(f32),
    /// Exactly this height in px; taller glyphs are clipped by the line above.
    Exactly(f32),
    /// At least this height in px.
    AtLeast(f32),
}

impl Default for LineSpacing {
    fn default() -> Self {
        Self::Multiple(1.0)
    }
}

/// Paragraph formatting.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphProps {
    /// Line alignment.
    pub alignment: Alignment,
    /// Indent from the start edge, px.
    pub indent_start: f32,
    /// Indent from the end edge, px.
    pub indent_end: f32,
    /// Additional indent of the first line, px: positive for a first-line indent, negative for a
    /// hanging indent (the first line starts at `indent_start + first_line`).
    pub first_line: f32,
    /// Custom tab stops (any order).
    pub tab_stops: Vec<TabStop>,
    /// Interval of the default tab stops, px (Word: 0.5 in = 48 px).
    pub default_tab: f32,
    /// Decimal separator for decimal tabs.
    pub decimal_separator: char,
    /// Line spacing.
    pub line_spacing: LineSpacing,
    /// Space above the paragraph, px.
    pub space_before: f32,
    /// Space below the paragraph, px.
    pub space_after: f32,
    /// Direction.
    pub direction: Direction,
    /// Automatic hyphenation (patterns chosen by each run's language; en-US and German built in).
    pub hyphenation: bool,
    /// Style of the paragraph mark: sizes an empty paragraph. Defaults to the last run's style.
    pub mark_style: Option<SpanStyle>,
}

impl Default for ParagraphProps {
    fn default() -> Self {
        Self {
            alignment: Alignment::Start,
            indent_start: 0.0,
            indent_end: 0.0,
            first_line: 0.0,
            tab_stops: Vec::new(),
            default_tab: 48.0,
            decimal_separator: '.',
            line_spacing: LineSpacing::default(),
            space_before: 0.0,
            space_after: 0.0,
            direction: Direction::Ltr,
            hyphenation: false,
            mark_style: None,
        }
    }
}

/// Everything needed to lay out one paragraph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParagraphSpec {
    /// The styled runs, in logical order.
    pub runs: Vec<TextRun>,
    /// Paragraph formatting.
    pub props: ParagraphProps,
    /// Width of the text area in px (the indents are inside it).
    pub width: f32,
}

impl ParagraphSpec {
    /// A paragraph of one run.
    #[must_use]
    pub fn plain(text: impl Into<String>, style: SpanStyle, width: f32) -> Self {
        Self {
            runs: vec![TextRun::new(text, style)],
            props: ParagraphProps::default(),
            width,
        }
    }

    /// The concatenated text of all runs.
    #[must_use]
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}
