// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paragraph, run and table properties mirroring `w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr` and
//! `w:tcPr` (ECMA-376 Part 1 §17.3, §17.4).
//!
//! Values keep the file's units so a DOCX round trip never re-quantises them:
//!
//! - lengths (indents, spacing, tab positions, table widths, margins) are **twips**;
//! - font sizes are **half-points** (`w:sz`);
//! - border widths are **eighths of a point** (`w:sz` of a border), border spacing is **points**;
//! - line spacing with [`LineRule::Auto`] is **240ths of a line** (240 = single, 259 = 1.08).
//!
//! Every property is optional: `None` means "not set at this level, inherit". Effective values come
//! from [`crate::resolve`].

use kiln_common::Color;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::st::{
    Alignment, BorderStyle, CellVAlign, DropCapKind, HeightRule, HighlightColor, LineRule,
    ShadingPattern, TabAlignment, TabLeader, TableAlignment, TableLayout, TextAlignment,
    TextDirection, ThemeColorName, ThemeFontRef, UnderlineStyle, VertAlign, WidthType,
};

// ------------------------------------------------------------------ colours

/// The `w:val` of a WordprocessingML colour: an RGB value or `auto` (the application decides,
/// black text on a light background).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ColorValue {
    /// `auto`.
    #[default]
    Auto,
    /// `RRGGBB`.
    Rgb(Color),
}

impl ColorValue {
    /// Parses `auto` or a six-digit hex value (with or without `#`). Anything else is `None`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("auto") {
            return Some(Self::Auto);
        }
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 {
            return None;
        }
        Color::parse_hex(hex).ok().map(Self::Rgb)
    }

    /// An RGB colour from a `0xRRGGBB` literal.
    #[must_use]
    pub const fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self::Rgb(Color::rgb(r, g, b))
    }

    /// The colour, or `None` for `auto`.
    #[must_use]
    pub const fn rgb(self) -> Option<Color> {
        match self {
            Self::Auto => None,
            Self::Rgb(c) => Some(c),
        }
    }
}

impl fmt::Display for ColorValue {
    /// `auto` or upper-case `RRGGBB`, as WordprocessingML writes it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Rgb(c) => write!(f, "{:02X}{:02X}{:02X}", c.r, c.g, c.b),
        }
    }
}

/// A WordprocessingML colour: `w:val` plus an optional theme reference. When a theme colour is
/// present it wins over `val`, which is then only the cached rendering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DocColor {
    /// `w:val`.
    pub val: ColorValue,
    /// `w:themeColor`.
    pub theme_color: Option<ThemeColorName>,
    /// `w:themeTint` (0–255).
    pub theme_tint: Option<u8>,
    /// `w:themeShade` (0–255).
    pub theme_shade: Option<u8>,
}

impl DocColor {
    /// A plain RGB colour from a `0xRRGGBB` literal.
    #[must_use]
    pub const fn rgb(rgb: u32) -> Self {
        Self {
            val: ColorValue::hex(rgb),
            theme_color: None,
            theme_tint: None,
            theme_shade: None,
        }
    }

    /// `auto`.
    #[must_use]
    pub const fn auto() -> Self {
        Self {
            val: ColorValue::Auto,
            theme_color: None,
            theme_tint: None,
            theme_shade: None,
        }
    }

    /// A theme colour with its cached value.
    #[must_use]
    pub const fn theme(cached: u32, theme: ThemeColorName) -> Self {
        Self {
            val: ColorValue::hex(cached),
            theme_color: Some(theme),
            theme_tint: None,
            theme_shade: None,
        }
    }

    /// The same colour with `w:themeTint`.
    #[must_use]
    pub const fn with_tint(mut self, tint: u8) -> Self {
        self.theme_tint = Some(tint);
        self
    }

    /// The same colour with `w:themeShade`.
    #[must_use]
    pub const fn with_shade(mut self, shade: u8) -> Self {
        self.theme_shade = Some(shade);
        self
    }
}

// ------------------------------------------------------------------ borders and shading

/// One border edge (`w:top`, `w:left`, …).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Border {
    /// `w:val`.
    pub style: BorderStyle,
    /// Width in eighths of a point (`w:sz`).
    pub size: Option<u32>,
    /// Distance from the text in points (`w:space`).
    pub space: Option<u32>,
    /// `w:color` with its theme reference.
    pub color: Option<DocColor>,
    /// `w:shadow`.
    pub shadow: Option<bool>,
    /// `w:frame`.
    pub frame: Option<bool>,
}

impl Border {
    /// A border of `style` with a width in eighths of a point.
    #[must_use]
    pub fn new(style: BorderStyle, size: u32) -> Self {
        Self {
            style,
            size: Some(size),
            ..Self::default()
        }
    }
}

/// Paragraph borders (`w:pBdr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ParagraphBorders {
    /// Top.
    pub top: Option<Border>,
    /// Left (leading).
    pub left: Option<Border>,
    /// Bottom.
    pub bottom: Option<Border>,
    /// Right (trailing).
    pub right: Option<Border>,
    /// Between consecutive paragraphs with identical border settings.
    pub between: Option<Border>,
    /// Vertical bar beside the paragraph.
    pub bar: Option<Border>,
}

/// Table borders (`w:tblBorders`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableBorders {
    /// Top.
    pub top: Option<Border>,
    /// Left (leading).
    pub left: Option<Border>,
    /// Bottom.
    pub bottom: Option<Border>,
    /// Right (trailing).
    pub right: Option<Border>,
    /// Between rows.
    pub inside_h: Option<Border>,
    /// Between columns.
    pub inside_v: Option<Border>,
}

/// Cell borders (`w:tcBorders`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CellBorders {
    /// Top.
    pub top: Option<Border>,
    /// Left (leading).
    pub left: Option<Border>,
    /// Bottom.
    pub bottom: Option<Border>,
    /// Right (trailing).
    pub right: Option<Border>,
    /// Inside horizontal (used by table style conditions).
    pub inside_h: Option<Border>,
    /// Inside vertical (used by table style conditions).
    pub inside_v: Option<Border>,
    /// Diagonal from top-left to bottom-right.
    pub tl2br: Option<Border>,
    /// Diagonal from top-right to bottom-left.
    pub tr2bl: Option<Border>,
}

/// Shading (`w:shd`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Shading {
    /// The pattern.
    pub pattern: ShadingPattern,
    /// The pattern colour.
    pub color: Option<DocColor>,
    /// The background fill.
    pub fill: Option<DocColor>,
}

impl Shading {
    /// A clear pattern over a solid fill.
    #[must_use]
    pub fn fill(fill: DocColor) -> Self {
        Self {
            pattern: ShadingPattern::Clear,
            color: Some(DocColor::auto()),
            fill: Some(fill),
        }
    }
}

// ------------------------------------------------------------------ paragraphs

/// A tab stop (`w:tab`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TabStop {
    /// Position in twips from the leading indent edge of the text area.
    pub pos: i32,
    /// Alignment; [`TabAlignment::Clear`] removes an inherited stop at this position.
    pub align: TabAlignment,
    /// Leader.
    pub leader: Option<TabLeader>,
}

impl TabStop {
    /// A stop without leader.
    #[must_use]
    pub const fn new(pos: i32, align: TabAlignment) -> Self {
        Self {
            pos,
            align,
            leader: None,
        }
    }
}

/// Indentation (`w:ind`) in twips. `first_line` and `hanging` are mutually exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Indent {
    /// Leading indent (`w:start` is normalised to `left`).
    pub left: Option<i32>,
    /// Trailing indent (`w:end` is normalised to `right`).
    pub right: Option<i32>,
    /// First-line indent.
    pub first_line: Option<i32>,
    /// Hanging indent.
    pub hanging: Option<i32>,
}

/// Paragraph spacing (`w:spacing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Spacing {
    /// Space before, twips.
    pub before: Option<i32>,
    /// Space after, twips.
    pub after: Option<i32>,
    /// Line spacing: 240ths of a line for [`LineRule::Auto`], twips otherwise.
    pub line: Option<i32>,
    /// How `line` is interpreted.
    pub line_rule: Option<LineRule>,
    /// `w:beforeAutospacing` (HTML-style automatic space).
    pub before_autospacing: Option<bool>,
    /// `w:afterAutospacing`.
    pub after_autospacing: Option<bool>,
}

/// `w:numPr`: list membership. `num_id` 0 explicitly removes numbering inherited from a style; a
/// level without a list id overrides only the level of a style's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct NumberingRef {
    /// `w:numId`.
    pub num_id: Option<u32>,
    /// `w:ilvl`, 0–8.
    pub level: Option<u8>,
}

impl NumberingRef {
    /// A list reference.
    #[must_use]
    pub const fn new(num_id: u32, level: u8) -> Self {
        Self {
            num_id: Some(num_id),
            level: Some(level),
        }
    }
}

/// A drop cap (`w:framePr/@w:dropCap` with `@w:lines`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DropCap {
    /// In the text or in the margin.
    pub kind: DropCapKind,
    /// Number of lines the letter spans.
    pub lines: u8,
}

/// Paragraph properties (`w:pPr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ParagraphProps {
    /// `w:pStyle`.
    pub style_id: Option<String>,
    /// `w:jc`.
    pub alignment: Option<Alignment>,
    /// `w:ind`.
    pub indent: Option<Indent>,
    /// `w:spacing`.
    pub spacing: Option<Spacing>,
    /// `w:numPr`.
    pub numbering: Option<NumberingRef>,
    /// `w:keepNext`.
    pub keep_next: Option<bool>,
    /// `w:keepLines`.
    pub keep_lines: Option<bool>,
    /// `w:pageBreakBefore`.
    pub page_break_before: Option<bool>,
    /// `w:widowControl`.
    pub widow_control: Option<bool>,
    /// `w:contextualSpacing`: ignore spacing between paragraphs of the same style.
    pub contextual_spacing: Option<bool>,
    /// `w:suppressAutoHyphens`.
    pub suppress_auto_hyphens: Option<bool>,
    /// `w:suppressLineNumbers`.
    pub suppress_line_numbers: Option<bool>,
    /// `w:mirrorIndents`.
    pub mirror_indents: Option<bool>,
    /// `w:snapToGrid`.
    pub snap_to_grid: Option<bool>,
    /// `w:outlineLvl`: 0–8 for headings 1–9, 9 = body text.
    pub outline_level: Option<u8>,
    /// `w:tabs`.
    pub tabs: Option<Vec<TabStop>>,
    /// `w:pBdr`.
    pub borders: Option<ParagraphBorders>,
    /// `w:shd`.
    pub shading: Option<Shading>,
    /// `w:bidi`: right-to-left paragraph.
    pub bidi: Option<bool>,
    /// `w:textAlignment`.
    pub text_alignment: Option<TextAlignment>,
    /// Drop cap.
    pub drop_cap: Option<DropCap>,
}

// ------------------------------------------------------------------ runs

/// Font names per script slot, explicit or by theme reference (`w:rFonts`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RunFonts {
    /// ASCII (0–127).
    pub ascii: Option<String>,
    /// High ANSI.
    pub h_ansi: Option<String>,
    /// East Asian.
    pub east_asia: Option<String>,
    /// Complex script.
    pub cs: Option<String>,
    /// Theme reference for `ascii`.
    pub ascii_theme: Option<ThemeFontRef>,
    /// Theme reference for `h_ansi`.
    pub h_ansi_theme: Option<ThemeFontRef>,
    /// Theme reference for `east_asia`.
    pub east_asia_theme: Option<ThemeFontRef>,
    /// Theme reference for `cs`.
    pub cs_theme: Option<ThemeFontRef>,
}

impl RunFonts {
    /// The same explicit font in the ASCII and High ANSI slots (what the font box sets).
    #[must_use]
    pub fn latin(name: &str) -> Self {
        Self {
            ascii: Some(name.to_owned()),
            h_ansi: Some(name.to_owned()),
            ..Self::default()
        }
    }
}

/// Underline (`w:u`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Underline {
    /// The style.
    pub style: UnderlineStyle,
    /// Its colour, when not the text colour.
    pub color: Option<DocColor>,
}

impl Underline {
    /// An underline of a style in the text colour.
    #[must_use]
    pub const fn new(style: UnderlineStyle) -> Self {
        Self { style, color: None }
    }
}

/// Languages (`w:lang`), BCP 47.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Lang {
    /// Latin text.
    pub val: Option<String>,
    /// East Asian text.
    pub east_asia: Option<String>,
    /// Complex script text.
    pub bidi: Option<String>,
}

/// Run properties (`w:rPr`). Toggle properties are tri-state: `None` inherits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RunProps {
    /// Character style (`w:rStyle`).
    pub style_id: Option<String>,
    /// `w:rFonts`.
    pub fonts: Option<RunFonts>,
    /// `w:b`.
    pub bold: Option<bool>,
    /// `w:bCs`.
    pub bold_cs: Option<bool>,
    /// `w:i`.
    pub italic: Option<bool>,
    /// `w:iCs`.
    pub italic_cs: Option<bool>,
    /// `w:u`.
    pub underline: Option<Underline>,
    /// `w:strike`.
    pub strike: Option<bool>,
    /// `w:dstrike`.
    pub dstrike: Option<bool>,
    /// `w:caps`.
    pub caps: Option<bool>,
    /// `w:smallCaps`.
    pub small_caps: Option<bool>,
    /// `w:vanish` (hidden text).
    pub hidden: Option<bool>,
    /// `w:emboss`.
    pub emboss: Option<bool>,
    /// `w:imprint`.
    pub imprint: Option<bool>,
    /// `w:outline`.
    pub outline: Option<bool>,
    /// `w:shadow`.
    pub shadow: Option<bool>,
    /// `w:color`.
    pub color: Option<DocColor>,
    /// `w:highlight`.
    pub highlight: Option<HighlightColor>,
    /// `w:shd`.
    pub shading: Option<Shading>,
    /// `w:bdr`: a border around the run.
    pub border: Option<Border>,
    /// Font size in half-points (`w:sz`).
    pub size: Option<u32>,
    /// Complex-script font size in half-points (`w:szCs`).
    pub size_cs: Option<u32>,
    /// `w:vertAlign`.
    pub vert_align: Option<VertAlign>,
    /// Character spacing adjustment in twips (`w:spacing`).
    pub spacing: Option<i32>,
    /// Horizontal scale in percent (`w:w`).
    pub scale: Option<u32>,
    /// Kerning threshold in half-points (`w:kern`).
    pub kern: Option<u32>,
    /// Baseline offset in half-points (`w:position`).
    pub position: Option<i32>,
    /// `w:rtl`.
    pub rtl: Option<bool>,
    /// `w:lang`.
    pub lang: Option<Lang>,
}

/// The toggle properties of ECMA-376 §17.7.3, which a character style XORs with the paragraph's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToggleProp {
    /// `w:b`.
    Bold,
    /// `w:bCs`.
    BoldCs,
    /// `w:i`.
    Italic,
    /// `w:iCs`.
    ItalicCs,
    /// `w:caps`.
    Caps,
    /// `w:smallCaps`.
    SmallCaps,
    /// `w:strike`.
    Strike,
    /// `w:dstrike`.
    Dstrike,
    /// `w:vanish`.
    Hidden,
    /// `w:emboss`.
    Emboss,
    /// `w:imprint`.
    Imprint,
    /// `w:outline`.
    Outline,
    /// `w:shadow`.
    Shadow,
}

impl ToggleProp {
    /// Every toggle property.
    pub const ALL: [ToggleProp; 13] = [
        Self::Bold,
        Self::BoldCs,
        Self::Italic,
        Self::ItalicCs,
        Self::Caps,
        Self::SmallCaps,
        Self::Strike,
        Self::Dstrike,
        Self::Hidden,
        Self::Emboss,
        Self::Imprint,
        Self::Outline,
        Self::Shadow,
    ];

    /// The index into [`ToggleProp::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl RunProps {
    /// The value of a toggle property at this level.
    #[must_use]
    pub const fn toggle(&self, prop: ToggleProp) -> Option<bool> {
        match prop {
            ToggleProp::Bold => self.bold,
            ToggleProp::BoldCs => self.bold_cs,
            ToggleProp::Italic => self.italic,
            ToggleProp::ItalicCs => self.italic_cs,
            ToggleProp::Caps => self.caps,
            ToggleProp::SmallCaps => self.small_caps,
            ToggleProp::Strike => self.strike,
            ToggleProp::Dstrike => self.dstrike,
            ToggleProp::Hidden => self.hidden,
            ToggleProp::Emboss => self.emboss,
            ToggleProp::Imprint => self.imprint,
            ToggleProp::Outline => self.outline,
            ToggleProp::Shadow => self.shadow,
        }
    }

    /// Sets a toggle property at this level (`None` inherits).
    pub fn set_toggle(&mut self, prop: ToggleProp, value: Option<bool>) {
        let slot = match prop {
            ToggleProp::Bold => &mut self.bold,
            ToggleProp::BoldCs => &mut self.bold_cs,
            ToggleProp::Italic => &mut self.italic,
            ToggleProp::ItalicCs => &mut self.italic_cs,
            ToggleProp::Caps => &mut self.caps,
            ToggleProp::SmallCaps => &mut self.small_caps,
            ToggleProp::Strike => &mut self.strike,
            ToggleProp::Dstrike => &mut self.dstrike,
            ToggleProp::Hidden => &mut self.hidden,
            ToggleProp::Emboss => &mut self.emboss,
            ToggleProp::Imprint => &mut self.imprint,
            ToggleProp::Outline => &mut self.outline,
            ToggleProp::Shadow => &mut self.shadow,
        };
        *slot = value;
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

// ------------------------------------------------------------------ tables

/// A table measure (`w:tblW`, `w:tcW`, `w:tblInd`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableWidth {
    /// The unit.
    pub kind: WidthType,
    /// Twips for `dxa`, fiftieths of a percent for `pct` (5000 = 100 %).
    pub value: i32,
}

impl TableWidth {
    /// A width in twips.
    #[must_use]
    pub const fn dxa(twips: i32) -> Self {
        Self {
            kind: WidthType::Dxa,
            value: twips,
        }
    }

    /// A width in fiftieths of a percent.
    #[must_use]
    pub const fn pct(fiftieths: i32) -> Self {
        Self {
            kind: WidthType::Pct,
            value: fiftieths,
        }
    }
}

/// Cell margins in twips (`w:tblCellMar`, `w:tcMar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CellMargins {
    /// Top.
    pub top: Option<i32>,
    /// Left (leading).
    pub left: Option<i32>,
    /// Bottom.
    pub bottom: Option<i32>,
    /// Right (trailing).
    pub right: Option<i32>,
}

/// `w:tblLook`: which conditional formats of the table style apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableLook {
    /// Header row.
    pub first_row: bool,
    /// Total row.
    pub last_row: bool,
    /// First column.
    pub first_column: bool,
    /// Last column.
    pub last_column: bool,
    /// No banded rows.
    pub no_h_band: bool,
    /// No banded columns.
    pub no_v_band: bool,
}

impl Default for TableLook {
    /// Word's default for new tables: header row, first column, banded rows.
    fn default() -> Self {
        Self {
            first_row: true,
            last_row: false,
            first_column: true,
            last_column: false,
            no_h_band: false,
            no_v_band: true,
        }
    }
}

/// A floating table's position (`w:tblpPr`), lengths in twips.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TablePosition {
    /// `w:leftFromText`.
    pub left_from_text: Option<i32>,
    /// `w:rightFromText`.
    pub right_from_text: Option<i32>,
    /// `w:topFromText`.
    pub top_from_text: Option<i32>,
    /// `w:bottomFromText`.
    pub bottom_from_text: Option<i32>,
    /// `w:horzAnchor` (`text`, `margin`, `page`).
    pub horz_anchor: Option<String>,
    /// `w:vertAnchor` (`text`, `margin`, `page`).
    pub vert_anchor: Option<String>,
    /// `w:tblpX`.
    pub x: Option<i32>,
    /// `w:tblpXSpec` (`left`, `center`, …).
    pub x_spec: Option<String>,
    /// `w:tblpY`.
    pub y: Option<i32>,
    /// `w:tblpYSpec`.
    pub y_spec: Option<String>,
}

/// Table properties (`w:tblPr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableProps {
    /// `w:tblStyle`.
    pub style_id: Option<String>,
    /// `w:tblW`.
    pub width: Option<TableWidth>,
    /// `w:jc`.
    pub alignment: Option<TableAlignment>,
    /// `w:tblInd` in twips.
    pub indent: Option<i32>,
    /// `w:tblBorders`.
    pub borders: Option<TableBorders>,
    /// `w:shd`.
    pub shading: Option<Shading>,
    /// `w:tblCellMar`.
    pub cell_margins: Option<CellMargins>,
    /// `w:tblCellSpacing` in twips.
    pub cell_spacing: Option<i32>,
    /// `w:tblLayout`.
    pub layout: Option<TableLayout>,
    /// `w:tblLook`.
    pub look: Option<TableLook>,
    /// `w:tblStyleRowBandSize` (default 1).
    pub row_band_size: Option<u32>,
    /// `w:tblStyleColBandSize` (default 1).
    pub col_band_size: Option<u32>,
    /// `w:bidiVisual`.
    pub bidi_visual: Option<bool>,
    /// `w:tblpPr`: floating table position.
    pub position: Option<TablePosition>,
    /// `w:tblOverlap` = `never` when false.
    pub allow_overlap: Option<bool>,
    /// `w:tblCaption` (alt text title).
    pub caption: Option<String>,
    /// `w:tblDescription` (alt text).
    pub description: Option<String>,
}

/// A row height (`w:trHeight`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RowHeight {
    /// Twips.
    pub value: i32,
    /// How the height applies.
    pub rule: HeightRule,
}

/// Table row properties (`w:trPr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableRowProps {
    /// `w:trHeight`.
    pub height: Option<RowHeight>,
    /// `w:tblHeader`: repeat as a header row on each page.
    pub header: Option<bool>,
    /// `w:cantSplit`.
    pub cant_split: Option<bool>,
    /// `w:jc`: row alignment override.
    pub alignment: Option<TableAlignment>,
    /// `w:gridBefore`: grid columns skipped before the first cell.
    pub grid_before: Option<u32>,
    /// `w:gridAfter`: grid columns skipped after the last cell.
    pub grid_after: Option<u32>,
    /// `w:hidden`.
    pub hidden: Option<bool>,
}

/// Table cell properties (`w:tcPr`) besides the span attributes on [`crate::Cell`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableCellProps {
    /// `w:tcW`.
    pub width: Option<TableWidth>,
    /// `w:tcBorders`.
    pub borders: Option<CellBorders>,
    /// `w:shd`.
    pub shading: Option<Shading>,
    /// `w:tcMar`.
    pub margins: Option<CellMargins>,
    /// `w:vAlign`.
    pub v_align: Option<CellVAlign>,
    /// `w:noWrap`.
    pub no_wrap: Option<bool>,
    /// `w:textDirection`.
    pub text_direction: Option<TextDirection>,
    /// `w:tcFitText`.
    pub fit_text: Option<bool>,
    /// `w:hideMark`.
    pub hide_mark: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_values_print_like_wordprocessingml() {
        assert_eq!(ColorValue::hex(0x2F5496).to_string(), "2F5496");
        assert_eq!(ColorValue::Auto.to_string(), "auto");
        assert_eq!(ColorValue::parse("auto"), Some(ColorValue::Auto));
        assert_eq!(
            ColorValue::parse("#ff0000"),
            Some(ColorValue::hex(0xFF0000))
        );
        assert_eq!(ColorValue::parse("F00"), None);
        assert_eq!(ColorValue::parse("zzzzzz"), None);
        assert_eq!(
            ColorValue::hex(0x123456).rgb(),
            Some(Color::rgb(0x12, 0x34, 0x56))
        );
        let c = DocColor::theme(0x2F5496, ThemeColorName::Accent1).with_shade(0xBF);
        assert_eq!(c.theme_shade, Some(0xBF));
        assert_eq!(DocColor::rgb(0xFF).with_tint(3).theme_tint, Some(3));
    }

    #[test]
    fn toggles_read_and_write_every_slot() {
        let mut rp = RunProps::default();
        assert!(rp.is_empty());
        for (i, t) in ToggleProp::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
            rp.set_toggle(*t, Some(i % 2 == 0));
            assert_eq!(rp.toggle(*t), Some(i % 2 == 0));
        }
        assert!(!rp.is_empty());
        assert_eq!(RunFonts::latin("Arial").h_ansi.as_deref(), Some("Arial"));
    }

    #[test]
    fn table_defaults_match_word() {
        let look = TableLook::default();
        assert!(look.first_row && look.first_column && !look.no_h_band && look.no_v_band);
        assert_eq!(TableWidth::pct(5000).kind, WidthType::Pct);
        assert_eq!(TableWidth::dxa(9360).value, 9360);
        assert_eq!(Border::new(BorderStyle::Single, 4).size, Some(4));
        assert_eq!(
            Shading::fill(DocColor::rgb(0xFFFF00)).pattern,
            ShadingPattern::Clear
        );
        assert_eq!(NumberingRef::new(3, 1).level, Some(1));
        assert_eq!(TabStop::new(720, TabAlignment::Left).leader, None);
        assert_eq!(
            Underline::new(UnderlineStyle::Double).style,
            UnderlineStyle::Double
        );
    }
}
