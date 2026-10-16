// SPDX-License-Identifier: AGPL-3.0-or-later
//! The result of paragraph layout.
//!
//! Coordinates are px in the paragraph's box: x = 0 is the left edge of the text area (the width
//! passed in the spec, before indents), y = 0 is the top of the paragraph (before `space_before`).
//! y grows downwards.

use std::ops::Range;

use crate::fonts::{FontId, ResolvedFamily};
use crate::spec::{Rgba, StrikeKind, UnderlineKind};

/// A glyph with its final position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// Glyph id in the run's font.
    pub id: u32,
    /// Pen x (left of the glyph's origin), px.
    pub x: f32,
    /// Baseline y, px.
    pub y: f32,
    /// Advance, px.
    pub advance: f32,
    /// Byte offset (in the paragraph text) of the cluster this glyph belongs to.
    pub cluster: u32,
}

/// Glyphs that share font, size, colour and direction, in visual order.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    /// The font instance.
    pub font: FontId,
    /// Font size, px.
    pub size: f32,
    /// Index of the [`crate::TextRun`] the glyphs come from (`usize::MAX` for synthesised glyphs such
    /// as tab leaders and automatic hyphens, which take the style of the neighbouring run).
    pub span: usize,
    /// Text colour.
    pub color: Rgba,
    /// Bidi embedding level (odd = right to left).
    pub level: u8,
    /// Byte range of the text covered.
    pub text_range: Range<usize>,
    /// The glyphs, left to right.
    pub glyphs: Vec<PositionedGlyph>,
}

impl GlyphRun {
    /// True for right-to-left text.
    #[must_use]
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }
}

/// What a decoration line represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    /// An underline.
    Underline(UnderlineKind),
    /// A strikethrough.
    Strike(StrikeKind),
    /// A heavy tab leader rule.
    LeaderRule,
}

/// A horizontal line drawn with the text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoration {
    /// Kind.
    pub kind: DecorationKind,
    /// Left x.
    pub x0: f32,
    /// Right x.
    pub x1: f32,
    /// y of the line's top edge.
    pub y: f32,
    /// Thickness.
    pub thickness: f32,
    /// Colour.
    pub color: Rgba,
}

/// Why a line ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnd {
    /// Wrapped at a break opportunity.
    Wrap,
    /// Wrapped inside a word with a hyphen.
    Hyphen,
    /// Wrapped where no break opportunity fitted (a word wider than the line).
    Emergency,
    /// A manual line break.
    LineBreak,
    /// The end of the paragraph.
    Paragraph,
}

/// One grapheme cluster's box on a line, in visual order (used for carets and hit testing).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterBox {
    /// Byte range in the paragraph text.
    pub text_range: Range<usize>,
    /// Left x.
    pub x: f32,
    /// Advance (width).
    pub advance: f32,
    /// Bidi level.
    pub level: u8,
}

impl ClusterBox {
    /// True for right-to-left clusters.
    #[must_use]
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }
}

/// A laid-out line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Byte range of the text on this line (including trailing spaces and the line break).
    pub text_range: Range<usize>,
    /// Top of the line box.
    pub top: f32,
    /// Height of the line box (after line spacing).
    pub height: f32,
    /// Baseline y.
    pub baseline: f32,
    /// Largest ascent on the line.
    pub ascent: f32,
    /// Largest descent on the line.
    pub descent: f32,
    /// Left x of the visible content (trailing spaces excluded).
    pub x: f32,
    /// Width of the visible content.
    pub width: f32,
    /// Why the line ended.
    pub end: LineEnd,
    /// Glyph runs in visual order.
    pub runs: Vec<GlyphRun>,
    /// Underlines, strikethroughs and leader rules.
    pub decorations: Vec<Decoration>,
    /// Cluster boxes in visual order.
    pub clusters: Vec<ClusterBox>,
}

impl Line {
    /// Number of glyphs on the line.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.runs.iter().map(|r| r.glyphs.len()).sum()
    }
}

/// A caret: a vertical bar at `x` from `top` to `top + height`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretRect {
    /// x of the caret.
    pub x: f32,
    /// Top.
    pub top: f32,
    /// Height.
    pub height: f32,
    /// Line index.
    pub line: usize,
    /// Direction of the character the caret is attached to.
    pub rtl: bool,
}

impl CaretRect {
    /// The caret's vertical centre.
    #[must_use]
    pub fn center_y(&self) -> f32 {
        self.top + self.height * 0.5
    }
}

/// Which side of a line wrap a caret position belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Affinity {
    /// The position attaches to the following text (start of the next line at a wrap).
    #[default]
    Downstream,
    /// The position attaches to the preceding text (end of the previous line at a wrap).
    Upstream,
}

/// A rectangle (selection highlight).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// A laid-out paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphLayout {
    pub(crate) text: String,
    /// Width of the text area.
    pub width: f32,
    /// Total height including space before and after.
    pub height: f32,
    /// Whether the resolved paragraph direction is right to left.
    pub rtl: bool,
    /// Lines, top to bottom.
    pub lines: Vec<Line>,
    /// Families that were substituted (Exact resolutions are omitted).
    pub substitutions: Vec<ResolvedFamily>,
    /// Caret stops: byte offsets of grapheme cluster boundaries, ascending, including 0 and the
    /// text length.
    pub(crate) boundaries: Vec<usize>,
}

impl ParagraphLayout {
    /// The paragraph text (all runs concatenated).
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Valid caret positions (grapheme cluster boundaries), ascending.
    #[must_use]
    pub fn caret_positions(&self) -> &[usize] {
        &self.boundaries
    }

    /// Total number of glyphs.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.lines.iter().map(Line::glyph_count).sum()
    }

    /// A plain-text rendering of the line breaks: one line per output line with the line's text
    /// (tabs as `→`, line breaks as `↵`, soft hyphens dropped, automatic hyphens as `-`) followed by
    /// the glyph count and why it ended. Used by snapshot tests.
    #[must_use]
    pub fn debug_lines(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            let mut text: String = self.text[line.text_range.clone()]
                .chars()
                .filter(|c| *c != '\u{ad}')
                .map(|c| match c {
                    '\t' => '→',
                    '\n' | '\u{2028}' => '↵',
                    c => c,
                })
                .collect();
            if matches!(line.end, LineEnd::Hyphen)
                || (self.text[..line.text_range.end].ends_with('\u{ad}')
                    && !matches!(line.end, LineEnd::Paragraph))
            {
                text.push('-');
            }
            let end = match line.end {
                LineEnd::Wrap => "wrap",
                LineEnd::Hyphen => "hyphen",
                LineEnd::Emergency => "emergency",
                LineEnd::LineBreak => "break",
                LineEnd::Paragraph => "end",
            };
            let _ = writeln!(
                out,
                "{i:>2} [{} glyphs, {end}] {}",
                line.glyph_count(),
                text.trim_end_matches(' ')
            );
        }
        out
    }
}
