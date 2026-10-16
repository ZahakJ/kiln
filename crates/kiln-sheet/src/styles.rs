// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell formatting deduplicated the way SpreadsheetML's `styles.xml` does it: interned fonts,
//! fills, borders and number format codes, and `cellXfs`-like entries ([`Xf`]) combining them with
//! alignment and protection. Cells refer to an [`Xf`] index; index 0 is the workbook default
//! ("Normal"). Differential formats ([`Dxf`], for conditional formatting and table styles) and
//! named cell styles live in the same table.
//!
//! Every component is canonical by construction (absent and default members compare equal), so
//! equal formats always share one index and applying the same formatting twice never grows the
//! table.

use std::hash::Hash;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::color::Color;
use crate::error::ModelError;
use crate::num::F64;

// ── fonts ─────────────────────────────────────────────────────────────────

/// Underline styles of a cell font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Underline {
    /// No underline.
    #[default]
    None,
    /// Single.
    Single,
    /// Double.
    Double,
    /// Single accounting (under the whole cell width).
    SingleAccounting,
    /// Double accounting.
    DoubleAccounting,
}

/// Superscript/subscript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VertAlign {
    /// Normal.
    #[default]
    Baseline,
    /// Superscript.
    Superscript,
    /// Subscript.
    Subscript,
}

/// Theme font scheme of a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontScheme {
    /// Not a theme font.
    #[default]
    None,
    /// The theme's body font.
    Minor,
    /// The theme's heading font.
    Major,
}

/// A cell font.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Font {
    /// Family name.
    pub name: Arc<str>,
    /// Size in points.
    pub size: F64,
    /// Bold.
    #[serde(default)]
    pub bold: bool,
    /// Italic.
    #[serde(default)]
    pub italic: bool,
    /// Underline.
    #[serde(default)]
    pub underline: Underline,
    /// Strikethrough.
    #[serde(default)]
    pub strike: bool,
    /// Colour (`None` = automatic).
    #[serde(default)]
    pub color: Option<Color>,
    /// Superscript/subscript.
    #[serde(default)]
    pub vert_align: VertAlign,
    /// Theme font scheme.
    #[serde(default)]
    pub scheme: FontScheme,
    /// Font family class (`family`, 2 = swiss).
    #[serde(default)]
    pub family: Option<u8>,
    /// Character set.
    #[serde(default)]
    pub charset: Option<u8>,
    /// Outline (Mac).
    #[serde(default)]
    pub outline: bool,
    /// Shadow (Mac).
    #[serde(default)]
    pub shadow: bool,
    /// Condense.
    #[serde(default)]
    pub condense: bool,
    /// Extend.
    #[serde(default)]
    pub extend: bool,
}

impl Default for Font {
    /// Excel 2016–2021's default body font: Calibri 11, theme minor font, text colour Text 1.
    fn default() -> Font {
        Font {
            name: Arc::from("Calibri"),
            size: F64(11.0),
            bold: false,
            italic: false,
            underline: Underline::None,
            strike: false,
            color: Some(Color::theme(1, 0.0)),
            vert_align: VertAlign::Baseline,
            scheme: FontScheme::Minor,
            family: Some(2),
            charset: None,
            outline: false,
            shadow: false,
            condense: false,
            extend: false,
        }
    }
}

/// A partial font: the members a format change or a rich-text run sets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FontPatch {
    /// Family name.
    #[serde(default)]
    pub name: Option<Arc<str>>,
    /// Size in points.
    #[serde(default)]
    pub size: Option<F64>,
    /// Bold.
    #[serde(default)]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(default)]
    pub italic: Option<bool>,
    /// Underline.
    #[serde(default)]
    pub underline: Option<Underline>,
    /// Strikethrough.
    #[serde(default)]
    pub strike: Option<bool>,
    /// Colour ([`Color::Auto`] for automatic).
    #[serde(default)]
    pub color: Option<Color>,
    /// Superscript/subscript.
    #[serde(default)]
    pub vert_align: Option<VertAlign>,
    /// Theme font scheme.
    #[serde(default)]
    pub scheme: Option<FontScheme>,
}

impl FontPatch {
    /// `base` with this patch applied.
    pub fn apply(&self, base: &Font) -> Font {
        let mut f = base.clone();
        if let Some(n) = &self.name {
            f.name = n.clone();
            // An explicit family name is no longer the theme font.
            if self.scheme.is_none() {
                f.scheme = FontScheme::None;
            }
        }
        if let Some(s) = self.size {
            f.size = s;
        }
        if let Some(b) = self.bold {
            f.bold = b;
        }
        if let Some(i) = self.italic {
            f.italic = i;
        }
        if let Some(u) = self.underline {
            f.underline = u;
        }
        if let Some(s) = self.strike {
            f.strike = s;
        }
        if let Some(c) = self.color {
            f.color = (c != Color::Auto).then_some(c);
        }
        if let Some(v) = self.vert_align {
            f.vert_align = v;
        }
        if let Some(s) = self.scheme {
            f.scheme = s;
        }
        f
    }
}

// ── fills ─────────────────────────────────────────────────────────────────

/// Pattern fill types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PatternType {
    #[default]
    None,
    Solid,
    MediumGray,
    DarkGray,
    LightGray,
    DarkHorizontal,
    DarkVertical,
    DarkDown,
    DarkUp,
    DarkGrid,
    DarkTrellis,
    LightHorizontal,
    LightVertical,
    LightDown,
    LightUp,
    LightGrid,
    LightTrellis,
    Gray125,
    Gray0625,
}

/// Gradient kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientKind {
    /// Along an angle.
    #[default]
    Linear,
    /// From a rectangle outwards.
    Path,
}

/// A gradient stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GradientStop {
    /// 0…1.
    pub position: F64,
    /// Colour.
    pub color: Color,
}

/// A gradient fill.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct GradientFill {
    /// Linear or path.
    pub kind: GradientKind,
    /// Angle in degrees (linear).
    pub degree: F64,
    /// Path rectangle, 0…1.
    pub left: F64,
    /// Path rectangle, 0…1.
    pub right: F64,
    /// Path rectangle, 0…1.
    pub top: F64,
    /// Path rectangle, 0…1.
    pub bottom: F64,
    /// Stops.
    pub stops: Vec<GradientStop>,
}

/// A cell fill.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fill {
    /// A pattern (solid fills are `Solid` with `fg`).
    Pattern {
        /// Pattern.
        pattern: PatternType,
        /// Foreground (the solid colour).
        fg: Option<Color>,
        /// Background.
        bg: Option<Color>,
    },
    /// A gradient.
    Gradient(GradientFill),
}

impl Default for Fill {
    fn default() -> Fill {
        Fill::NONE
    }
}

impl Fill {
    /// No fill.
    pub const NONE: Fill = Fill::Pattern {
        pattern: PatternType::None,
        fg: None,
        bg: None,
    };

    /// The mandatory second fill of every `styles.xml`.
    pub const GRAY125: Fill = Fill::Pattern {
        pattern: PatternType::Gray125,
        fg: None,
        bg: None,
    };

    /// A solid fill.
    pub fn solid(color: Color) -> Fill {
        Fill::Pattern {
            pattern: PatternType::Solid,
            fg: Some(color),
            bg: None,
        }
    }

    /// The canonical form (no colours on an empty pattern).
    pub fn normalized(self) -> Fill {
        match self {
            Fill::Pattern {
                pattern: PatternType::None,
                ..
            } => Fill::NONE,
            other => other,
        }
    }

    /// The colour a cell shows for this fill (the foreground of a solid fill, the first stop of
    /// a gradient), if any.
    pub fn main_color(&self) -> Option<Color> {
        match self {
            Fill::Pattern {
                pattern: PatternType::None,
                ..
            } => None,
            Fill::Pattern {
                pattern: PatternType::Solid,
                fg,
                ..
            } => *fg,
            Fill::Pattern { fg, bg, .. } => bg.or(*fg),
            Fill::Gradient(g) => g.stops.first().map(|s| s.color),
        }
    }
}

// ── borders ───────────────────────────────────────────────────────────────

/// The 13 line styles of a border side (plus none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum BorderStyle {
    #[default]
    None,
    Thin,
    Medium,
    Dashed,
    Dotted,
    Thick,
    Double,
    Hair,
    MediumDashed,
    DashDot,
    MediumDashDot,
    DashDotDot,
    MediumDashDotDot,
    SlantDashDot,
}

/// One side of a border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BorderSide {
    /// Line style.
    pub style: BorderStyle,
    /// Colour (`None` = automatic).
    #[serde(default)]
    pub color: Option<Color>,
}

impl BorderSide {
    /// A thin automatic line.
    pub const THIN: BorderSide = BorderSide {
        style: BorderStyle::Thin,
        color: None,
    };
}

/// A cell border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Border {
    /// Left side.
    #[serde(default)]
    pub left: Option<BorderSide>,
    /// Right side.
    #[serde(default)]
    pub right: Option<BorderSide>,
    /// Top side.
    #[serde(default)]
    pub top: Option<BorderSide>,
    /// Bottom side.
    #[serde(default)]
    pub bottom: Option<BorderSide>,
    /// Diagonal line(s).
    #[serde(default)]
    pub diagonal: Option<BorderSide>,
    /// Diagonal from bottom left to top right.
    #[serde(default)]
    pub diagonal_up: bool,
    /// Diagonal from top left to bottom right.
    #[serde(default)]
    pub diagonal_down: bool,
}

impl Border {
    /// The canonical form (sides with style `None` removed).
    pub fn normalized(mut self) -> Border {
        for s in [
            &mut self.left,
            &mut self.right,
            &mut self.top,
            &mut self.bottom,
            &mut self.diagonal,
        ] {
            if s.is_some_and(|x| x.style == BorderStyle::None) {
                *s = None;
            }
        }
        self
    }
}

// ── alignment, protection ─────────────────────────────────────────────────

/// Horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum HAlign {
    /// Text left, numbers right.
    #[default]
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}

/// Vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum VAlign {
    Top,
    Center,
    #[default]
    Bottom,
    Justify,
    Distributed,
}

/// Cell alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Alignment {
    /// Horizontal.
    #[serde(default)]
    pub horizontal: HAlign,
    /// Vertical.
    #[serde(default)]
    pub vertical: VAlign,
    /// Wrap text.
    #[serde(default)]
    pub wrap_text: bool,
    /// Shrink to fit.
    #[serde(default)]
    pub shrink_to_fit: bool,
    /// Indent level (about three characters each).
    #[serde(default)]
    pub indent: u8,
    /// 0–90 counter-clockwise, 91–180 clockwise (90 + degrees), 255 stacked.
    #[serde(default)]
    pub text_rotation: u8,
    /// 0 context, 1 left-to-right, 2 right-to-left.
    #[serde(default)]
    pub reading_order: u8,
    /// Justify the last line of distributed text.
    #[serde(default)]
    pub justify_last_line: bool,
}

/// Cell protection (effective only on a protected sheet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Protection {
    /// Locked (Excel's default).
    pub locked: bool,
    /// Formula hidden.
    pub hidden: bool,
}

impl Default for Protection {
    fn default() -> Protection {
        Protection {
            locked: true,
            hidden: false,
        }
    }
}

// ── formats ───────────────────────────────────────────────────────────────

/// One `cellXfs` entry: component indices plus alignment and protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Xf {
    /// Index into the fonts.
    pub font: u32,
    /// Index into the fills.
    pub fill: u32,
    /// Index into the borders.
    pub border: u32,
    /// Index into the number format codes.
    pub num_fmt: u32,
    /// Alignment.
    pub alignment: Alignment,
    /// Protection.
    pub protection: Protection,
    /// Index into the named cell styles (0 = Normal).
    pub style_id: u32,
    /// A `'` was typed in front of the value to keep it as text.
    pub quote_prefix: bool,
}

/// A fully resolved cell format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CellStyle {
    /// Number format code.
    pub num_fmt: Arc<str>,
    /// Font.
    pub font: Font,
    /// Fill.
    pub fill: Fill,
    /// Border.
    pub border: Border,
    /// Alignment.
    pub alignment: Alignment,
    /// Protection.
    pub protection: Protection,
    /// Named cell style.
    pub style_id: u32,
    /// Quote prefix.
    pub quote_prefix: bool,
}

/// Keep, clear or set one member of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Patch<T> {
    /// Leave as it is.
    #[default]
    Keep,
    /// Remove (back to the default).
    Clear,
    /// Set.
    Set(T),
}

impl<T: Clone> Patch<T> {
    /// The member after the patch, given its current value.
    pub fn apply_opt(&self, base: Option<T>) -> Option<T> {
        match self {
            Patch::Keep => base,
            Patch::Clear => None,
            Patch::Set(v) => Some(v.clone()),
        }
    }

    /// True for [`Patch::Keep`].
    pub fn is_keep(&self) -> bool {
        matches!(self, Patch::Keep)
    }
}

/// Changes to a border.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BorderPatch {
    #[serde(default)]
    pub left: Patch<BorderSide>,
    #[serde(default)]
    pub right: Patch<BorderSide>,
    #[serde(default)]
    pub top: Patch<BorderSide>,
    #[serde(default)]
    pub bottom: Patch<BorderSide>,
    #[serde(default)]
    pub diagonal: Patch<BorderSide>,
    #[serde(default)]
    pub diagonal_up: Option<bool>,
    #[serde(default)]
    pub diagonal_down: Option<bool>,
}

/// Changes to an alignment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct AlignmentPatch {
    #[serde(default)]
    pub horizontal: Option<HAlign>,
    #[serde(default)]
    pub vertical: Option<VAlign>,
    #[serde(default)]
    pub wrap_text: Option<bool>,
    #[serde(default)]
    pub shrink_to_fit: Option<bool>,
    #[serde(default)]
    pub indent: Option<u8>,
    #[serde(default)]
    pub text_rotation: Option<u8>,
    #[serde(default)]
    pub reading_order: Option<u8>,
    #[serde(default)]
    pub justify_last_line: Option<bool>,
}

/// Changes to protection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ProtectionPatch {
    #[serde(default)]
    pub locked: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// A partial change to a cell format (Home ribbon commands, Format Cells).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct StylePatch {
    /// Number format code.
    #[serde(default)]
    pub num_fmt: Option<String>,
    /// Font members.
    #[serde(default)]
    pub font: Option<FontPatch>,
    /// Fill.
    #[serde(default)]
    pub fill: Patch<Fill>,
    /// Border sides.
    #[serde(default)]
    pub border: Option<BorderPatch>,
    /// Alignment members.
    #[serde(default)]
    pub alignment: Option<AlignmentPatch>,
    /// Protection members.
    #[serde(default)]
    pub protection: Option<ProtectionPatch>,
    /// Named cell style.
    #[serde(default)]
    pub style_id: Option<u32>,
    /// Quote prefix.
    #[serde(default)]
    pub quote_prefix: Option<bool>,
}

impl StylePatch {
    /// `base` with the patch applied.
    pub fn apply(&self, base: &CellStyle) -> CellStyle {
        let mut s = base.clone();
        if let Some(n) = &self.num_fmt {
            s.num_fmt = Arc::from(n.as_str());
        }
        if let Some(f) = &self.font {
            s.font = f.apply(&base.font);
        }
        match &self.fill {
            Patch::Keep => {}
            Patch::Clear => s.fill = Fill::NONE,
            Patch::Set(f) => s.fill = f.clone().normalized(),
        }
        if let Some(b) = &self.border {
            let o = &base.border;
            s.border = Border {
                left: b.left.apply_opt(o.left),
                right: b.right.apply_opt(o.right),
                top: b.top.apply_opt(o.top),
                bottom: b.bottom.apply_opt(o.bottom),
                diagonal: b.diagonal.apply_opt(o.diagonal),
                diagonal_up: b.diagonal_up.unwrap_or(o.diagonal_up),
                diagonal_down: b.diagonal_down.unwrap_or(o.diagonal_down),
            }
            .normalized();
        }
        if let Some(a) = &self.alignment {
            let o = &base.alignment;
            s.alignment = Alignment {
                horizontal: a.horizontal.unwrap_or(o.horizontal),
                vertical: a.vertical.unwrap_or(o.vertical),
                wrap_text: a.wrap_text.unwrap_or(o.wrap_text),
                shrink_to_fit: a.shrink_to_fit.unwrap_or(o.shrink_to_fit),
                indent: a.indent.unwrap_or(o.indent),
                text_rotation: a.text_rotation.unwrap_or(o.text_rotation),
                reading_order: a.reading_order.unwrap_or(o.reading_order),
                justify_last_line: a.justify_last_line.unwrap_or(o.justify_last_line),
            };
        }
        if let Some(p) = &self.protection {
            s.protection = Protection {
                locked: p.locked.unwrap_or(base.protection.locked),
                hidden: p.hidden.unwrap_or(base.protection.hidden),
            };
        }
        if let Some(id) = self.style_id {
            s.style_id = id;
        }
        if let Some(q) = self.quote_prefix {
            s.quote_prefix = q;
        }
        s
    }
}

/// A differential format (conditional formatting, table styles): only what it sets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Dxf {
    /// Number format code.
    #[serde(default)]
    pub num_fmt: Option<Arc<str>>,
    /// Font members.
    #[serde(default)]
    pub font: Option<FontPatch>,
    /// Fill.
    #[serde(default)]
    pub fill: Option<Fill>,
    /// Border.
    #[serde(default)]
    pub border: Option<Border>,
    /// Alignment.
    #[serde(default)]
    pub alignment: Option<Alignment>,
    /// Protection.
    #[serde(default)]
    pub protection: Option<Protection>,
}

impl Dxf {
    /// Layers `over` on top of `self` (members `over` sets win), as stacked conditional formats
    /// combine.
    pub fn layered(&self, over: &Dxf) -> Dxf {
        let font = match (&self.font, &over.font) {
            (Some(a), Some(b)) => Some(FontPatch {
                name: b.name.clone().or_else(|| a.name.clone()),
                size: b.size.or(a.size),
                bold: b.bold.or(a.bold),
                italic: b.italic.or(a.italic),
                underline: b.underline.or(a.underline),
                strike: b.strike.or(a.strike),
                color: b.color.or(a.color),
                vert_align: b.vert_align.or(a.vert_align),
                scheme: b.scheme.or(a.scheme),
            }),
            (a, b) => b.clone().or_else(|| a.clone()),
        };
        Dxf {
            num_fmt: over.num_fmt.clone().or_else(|| self.num_fmt.clone()),
            font,
            fill: over.fill.clone().or_else(|| self.fill.clone()),
            border: over.border.or(self.border),
            alignment: over.alignment.or(self.alignment),
            protection: over.protection.or(self.protection),
        }
    }
}

/// A named cell style (Home › Cell Styles): "Normal", "Good", "Heading 1"…
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedCellStyle {
    /// Name.
    pub name: String,
    /// Excel's built-in style id (0 = Normal) for built-in styles.
    #[serde(default)]
    pub builtin_id: Option<u32>,
    /// The format index holding the style's format.
    pub xf: u32,
    /// Hidden from the gallery.
    #[serde(default)]
    pub hidden: bool,
    /// A built-in style the user modified.
    #[serde(default)]
    pub custom_builtin: bool,
}

// ── built-in number formats ───────────────────────────────────────────────

/// SpreadsheetML's implicit number formats (ECMA-376 Part 1 §18.8.30), en-US.
pub const BUILTIN_NUMFMTS: &[(u32, &str)] = &[
    (0, "General"),
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (5, "\"$\"#,##0_);\\(\"$\"#,##0\\)"),
    (6, "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)"),
    (7, "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)"),
    (8, "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "m/d/yyyy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yyyy h:mm"),
    (37, "#,##0_);(#,##0)"),
    (38, "#,##0_);[Red](#,##0)"),
    (39, "#,##0.00_);(#,##0.00)"),
    (40, "#,##0.00_);[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)"),
    (
        42,
        "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)",
    ),
    (43, "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)"),
    (
        44,
        "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
    ),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mm:ss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// The format code of a built-in `numFmtId`.
pub fn builtin_num_fmt(id: u32) -> Option<&'static str> {
    BUILTIN_NUMFMTS
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, c)| *c)
}

/// The built-in `numFmtId` of a format code, if it is one.
pub fn builtin_num_fmt_id(code: &str) -> Option<u32> {
    BUILTIN_NUMFMTS
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(i, _)| *i)
}

// ── the table ─────────────────────────────────────────────────────────────

/// An interning table: equal items share one index.
#[derive(Debug, Clone)]
pub struct Interner<T: Eq + Hash + Clone> {
    items: Vec<T>,
    index: FxHashMap<T, u32>,
}

impl<T: Eq + Hash + Clone> Default for Interner<T> {
    fn default() -> Self {
        Interner {
            items: Vec::new(),
            index: FxHashMap::default(),
        }
    }
}

impl<T: Eq + Hash + Clone> Interner<T> {
    /// The index of an equal item, adding it when new.
    pub fn intern(&mut self, item: T) -> u32 {
        if let Some(&i) = self.index.get(&item) {
            return i;
        }
        let i = self.items.len() as u32;
        self.index.insert(item.clone(), i);
        self.items.push(item);
        i
    }

    /// Appends without deduplication (files may contain duplicates whose indices must stay).
    pub fn push(&mut self, item: T) -> u32 {
        let i = self.items.len() as u32;
        self.index.entry(item.clone()).or_insert(i);
        self.items.push(item);
        i
    }

    /// The item at an index.
    pub fn get(&self, i: u32) -> Option<&T> {
        self.items.get(i as usize)
    }

    /// All items in index order.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Eq + Hash + Clone + Serialize> Serialize for Interner<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(s)
    }
}

impl<'de, T: Eq + Hash + Clone + Deserialize<'de>> Deserialize<'de> for Interner<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let items = Vec::<T>::deserialize(d)?;
        let mut out = Interner::default();
        for it in items {
            out.push(it);
        }
        Ok(out)
    }
}

/// The raw parts of a style table, as a file reader builds them (indices are kept).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StyleParts {
    /// Fonts.
    pub fonts: Vec<Font>,
    /// Fills.
    pub fills: Vec<Fill>,
    /// Borders.
    pub borders: Vec<Border>,
    /// Number format codes (an [`Xf::num_fmt`] indexes this list).
    pub num_fmts: Vec<String>,
    /// Cell formats.
    pub xfs: Vec<Xf>,
    /// Differential formats.
    pub dxfs: Vec<Dxf>,
    /// Named cell styles.
    pub named_styles: Vec<NamedCellStyle>,
    /// Custom table styles.
    #[serde(default)]
    pub table_styles: Vec<crate::table_style::TableStyleDef>,
    /// The default table style name.
    #[serde(default)]
    pub default_table_style: Option<String>,
}

/// The workbook's style table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleTable {
    fonts: Interner<Font>,
    fills: Interner<Fill>,
    borders: Interner<Border>,
    num_fmts: Interner<Arc<str>>,
    xfs: Interner<Xf>,
    dxfs: Interner<Dxf>,
    named_styles: Vec<NamedCellStyle>,
    table_styles: Vec<crate::table_style::TableStyleDef>,
    default_table_style: Option<String>,
}

impl Default for StyleTable {
    fn default() -> StyleTable {
        StyleTable::new(Font::default())
    }
}

impl StyleTable {
    /// A table whose index 0 ("Normal") uses `default_font`, with the two mandatory fills.
    pub fn new(default_font: Font) -> StyleTable {
        let mut t = StyleTable {
            fonts: Interner::default(),
            fills: Interner::default(),
            borders: Interner::default(),
            num_fmts: Interner::default(),
            xfs: Interner::default(),
            dxfs: Interner::default(),
            named_styles: Vec::new(),
            table_styles: Vec::new(),
            default_table_style: None,
        };
        t.fonts.intern(default_font);
        t.fills.intern(Fill::NONE);
        t.fills.intern(Fill::GRAY125);
        t.borders.intern(Border::default());
        t.num_fmts.intern(Arc::from("General"));
        t.xfs.intern(Xf::default());
        t.named_styles.push(NamedCellStyle {
            name: "Normal".into(),
            builtin_id: Some(0),
            xf: 0,
            hidden: false,
            custom_builtin: false,
        });
        t
    }

    /// Builds a table from a file's parts, keeping every index.
    pub fn from_parts(parts: StyleParts) -> Result<StyleTable, ModelError> {
        if parts.fonts.is_empty()
            || parts.fills.is_empty()
            || parts.borders.is_empty()
            || parts.xfs.is_empty()
        {
            return Err(ModelError::invalid(
                "a style table needs at least one font, fill, border and cell format",
            ));
        }
        let mut t = StyleTable {
            fonts: Interner::default(),
            fills: Interner::default(),
            borders: Interner::default(),
            num_fmts: Interner::default(),
            xfs: Interner::default(),
            dxfs: Interner::default(),
            named_styles: parts.named_styles,
            table_styles: parts.table_styles,
            default_table_style: parts.default_table_style,
        };
        for f in parts.fonts {
            t.fonts.push(f);
        }
        for f in parts.fills {
            t.fills.push(f.normalized());
        }
        for b in parts.borders {
            t.borders.push(b.normalized());
        }
        if parts.num_fmts.is_empty() {
            t.num_fmts.push(Arc::from("General"));
        }
        for n in parts.num_fmts {
            t.num_fmts.push(Arc::from(n));
        }
        for x in parts.xfs {
            if x.font as usize >= t.fonts.len()
                || x.fill as usize >= t.fills.len()
                || x.border as usize >= t.borders.len()
                || x.num_fmt as usize >= t.num_fmts.len()
            {
                return Err(ModelError::invalid(
                    "a cell format refers to a missing font, fill, border or number format",
                ));
            }
            t.xfs.push(x);
        }
        for d in parts.dxfs {
            t.dxfs.push(d);
        }
        if t.named_styles.is_empty() {
            t.named_styles.push(NamedCellStyle {
                name: "Normal".into(),
                builtin_id: Some(0),
                xf: 0,
                hidden: false,
                custom_builtin: false,
            });
        }
        Ok(t)
    }

    /// The table's parts (for file writers).
    pub fn to_parts(&self) -> StyleParts {
        StyleParts {
            fonts: self.fonts.items().to_vec(),
            fills: self.fills.items().to_vec(),
            borders: self.borders.items().to_vec(),
            num_fmts: self
                .num_fmts
                .items()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            xfs: self.xfs.items().to_vec(),
            dxfs: self.dxfs.items().to_vec(),
            named_styles: self.named_styles.clone(),
            table_styles: self.table_styles.clone(),
            default_table_style: self.default_table_style.clone(),
        }
    }

    /// Number of cell formats.
    pub fn len(&self) -> usize {
        self.xfs.len()
    }

    /// Always false (index 0 exists).
    pub fn is_empty(&self) -> bool {
        self.xfs.is_empty()
    }

    /// True when `index` is a cell format.
    pub fn contains(&self, index: u32) -> bool {
        (index as usize) < self.xfs.len()
    }

    /// Checks a cell format index exists.
    pub fn check(&self, index: u32) -> Result<(), ModelError> {
        if self.contains(index) {
            Ok(())
        } else {
            Err(ModelError::invalid(format!(
                "cell format {index} does not exist"
            )))
        }
    }

    /// The `cellXfs` entry (index 0 for unknown indices).
    pub fn xf(&self, index: u32) -> &Xf {
        self.xfs
            .get(index)
            .or_else(|| self.xfs.get(0))
            .unwrap_or(&DEFAULT_XF)
    }

    /// A font by index (the default font for unknown indices).
    pub fn font(&self, index: u32) -> &Font {
        self.fonts
            .get(index)
            .or_else(|| self.fonts.get(0))
            .unwrap_or(&DEFAULT_FONT)
    }

    /// A fill by index.
    pub fn fill(&self, index: u32) -> &Fill {
        self.fills.get(index).unwrap_or(&Fill::NONE)
    }

    /// A border by index.
    pub fn border(&self, index: u32) -> &Border {
        self.borders.get(index).unwrap_or(&DEFAULT_BORDER)
    }

    /// The number format code of a cell format.
    pub fn num_fmt(&self, index: u32) -> &str {
        let xf = self.xf(index);
        self.num_fmts.get(xf.num_fmt).map_or("General", |s| &**s)
    }

    /// The font of a cell format.
    pub fn font_of(&self, index: u32) -> &Font {
        self.font(self.xf(index).font)
    }

    /// The fill of a cell format.
    pub fn fill_of(&self, index: u32) -> &Fill {
        self.fill(self.xf(index).fill)
    }

    /// The default (index 0) font.
    pub fn default_font(&self) -> &Font {
        self.font_of(0)
    }

    /// The resolved format of a cell format index.
    pub fn get(&self, index: u32) -> CellStyle {
        let xf = *self.xf(index);
        CellStyle {
            num_fmt: self
                .num_fmts
                .get(xf.num_fmt)
                .cloned()
                .unwrap_or_else(|| Arc::from("General")),
            font: self.font(xf.font).clone(),
            fill: self.fill(xf.fill).clone(),
            border: *self.border(xf.border),
            alignment: xf.alignment,
            protection: xf.protection,
            style_id: xf.style_id,
            quote_prefix: xf.quote_prefix,
        }
    }

    /// Interns a full format and returns its index.
    pub fn intern(&mut self, style: &CellStyle) -> u32 {
        let code: Arc<str> = if style.num_fmt.is_empty() {
            Arc::from("General")
        } else {
            style.num_fmt.clone()
        };
        let xf = Xf {
            font: self.fonts.intern(style.font.clone()),
            fill: self.fills.intern(style.fill.clone().normalized()),
            border: self.borders.intern(style.border.normalized()),
            num_fmt: self.num_fmts.intern(code),
            alignment: style.alignment,
            protection: style.protection,
            style_id: style.style_id,
            quote_prefix: style.quote_prefix,
        };
        self.xfs.intern(xf)
    }

    /// Applies a patch to the format at `index` and returns the index of the result.
    pub fn patch(&mut self, index: u32, patch: &StylePatch) -> u32 {
        let base = self.get(index);
        let next = patch.apply(&base);
        if next == base && self.contains(index) {
            return index;
        }
        self.intern(&next)
    }

    /// Number of differential formats.
    pub fn dxf_count(&self) -> usize {
        self.dxfs.len()
    }

    /// Interns a differential format.
    pub fn intern_dxf(&mut self, dxf: Dxf) -> u32 {
        self.dxfs.intern(dxf)
    }

    /// A differential format.
    pub fn dxf(&self, index: u32) -> Option<&Dxf> {
        self.dxfs.get(index)
    }

    /// Named cell styles.
    pub fn named_styles(&self) -> &[NamedCellStyle] {
        &self.named_styles
    }

    /// Custom table styles.
    pub fn table_styles(&self) -> &[crate::table_style::TableStyleDef] {
        &self.table_styles
    }

    /// Adds or replaces a custom table style (by name, ignoring case).
    pub fn set_table_style(&mut self, style: crate::table_style::TableStyleDef) {
        match self
            .table_styles
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&style.name))
        {
            Some(s) => *s = style,
            None => self.table_styles.push(style),
        }
    }

    /// The default table style name (Excel's is `TableStyleMedium2`).
    pub fn default_table_style(&self) -> &str {
        self.default_table_style
            .as_deref()
            .unwrap_or("TableStyleMedium2")
    }

    /// Adds or replaces a named cell style (by name, ignoring case).
    pub fn set_named_style(&mut self, style: NamedCellStyle) {
        match self
            .named_styles
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&style.name))
        {
            Some(s) => *s = style,
            None => self.named_styles.push(style),
        }
    }
}

static DEFAULT_XF: Xf = Xf {
    font: 0,
    fill: 0,
    border: 0,
    num_fmt: 0,
    alignment: Alignment {
        horizontal: HAlign::General,
        vertical: VAlign::Bottom,
        wrap_text: false,
        shrink_to_fit: false,
        indent: 0,
        text_rotation: 0,
        reading_order: 0,
        justify_last_line: false,
    },
    protection: Protection {
        locked: true,
        hidden: false,
    },
    style_id: 0,
    quote_prefix: false,
};

static DEFAULT_BORDER: Border = Border {
    left: None,
    right: None,
    top: None,
    bottom: None,
    diagonal: None,
    diagonal_up: false,
    diagonal_down: false,
};

static DEFAULT_FONT: std::sync::LazyLock<Font> = std::sync::LazyLock::new(Font::default);

#[cfg(test)]
mod tests {
    use super::*;

    fn bold() -> StylePatch {
        StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        }
    }

    #[test]
    fn starts_with_normal_at_index_0_and_the_two_mandatory_fills() {
        let t = StyleTable::default();
        assert_eq!(t.len(), 1);
        assert_eq!(t.num_fmt(0), "General");
        assert_eq!(&*t.default_font().name, "Calibri");
        let p = t.to_parts();
        assert_eq!(p.fills, vec![Fill::NONE, Fill::GRAY125]);
        assert_eq!(t.named_styles()[0].name, "Normal");
    }

    #[test]
    fn deduplicates_equal_formats() {
        let mut t = StyleTable::default();
        let a = t.patch(0, &bold());
        let b = t.patch(0, &bold());
        assert_eq!(a, b);
        assert_eq!(t.patch(a, &bold()), a, "re-applying changes nothing");
        assert_eq!(t.len(), 2);
        let mut s = t.get(0);
        s.font.bold = true;
        assert_eq!(t.intern(&s), a);
    }

    #[test]
    fn treats_default_members_as_absent() {
        let mut t = StyleTable::default();
        let mut s = t.get(0);
        s.fill = Fill::Pattern {
            pattern: PatternType::None,
            fg: Some(Color::rgb(0xFF0000)),
            bg: None,
        };
        s.border.left = Some(BorderSide {
            style: BorderStyle::None,
            color: None,
        });
        assert_eq!(t.intern(&s), 0);
    }

    #[test]
    fn merges_patches_field_by_field_and_clears() {
        let mut t = StyleTable::default();
        let a = t.patch(
            0,
            &StylePatch {
                num_fmt: Some("0.00".into()),
                fill: Patch::Set(Fill::solid(Color::rgb(0xFFFF00))),
                border: Some(BorderPatch {
                    top: Patch::Set(BorderSide::THIN),
                    bottom: Patch::Set(BorderSide::THIN),
                    ..BorderPatch::default()
                }),
                alignment: Some(AlignmentPatch {
                    horizontal: Some(HAlign::Center),
                    wrap_text: Some(true),
                    ..AlignmentPatch::default()
                }),
                ..StylePatch::default()
            },
        );
        let b = t.patch(a, &bold());
        let s = t.get(b);
        assert!(s.font.bold);
        assert_eq!(&*s.num_fmt, "0.00");
        assert_eq!(s.alignment.horizontal, HAlign::Center);
        assert!(s.border.top.is_some() && s.border.bottom.is_some());
        let c = t.patch(
            b,
            &StylePatch {
                fill: Patch::Clear,
                border: Some(BorderPatch {
                    top: Patch::Clear,
                    ..BorderPatch::default()
                }),
                ..StylePatch::default()
            },
        );
        let s = t.get(c);
        assert_eq!(s.fill, Fill::NONE);
        assert!(s.border.top.is_none() && s.border.bottom.is_some());
    }

    #[test]
    fn shares_components_between_formats() {
        let mut t = StyleTable::default();
        let a = t.patch(
            0,
            &StylePatch {
                num_fmt: Some("0%".into()),
                ..StylePatch::default()
            },
        );
        let b = t.patch(a, &bold());
        let c = t.patch(
            0,
            &StylePatch {
                num_fmt: Some("0.0".into()),
                font: bold().font,
                ..StylePatch::default()
            },
        );
        assert_eq!(t.xf(b).font, t.xf(c).font);
        assert_eq!(t.xf(a).fill, t.xf(b).fill);
        assert_eq!(t.to_parts().fonts.len(), 2);
    }

    #[test]
    fn interns_differential_formats() {
        let mut t = StyleTable::default();
        let d = Dxf {
            fill: Some(Fill::solid(Color::rgb(0xFFC7CE))),
            font: Some(FontPatch {
                color: Some(Color::rgb(0x9C0006)),
                ..FontPatch::default()
            }),
            ..Dxf::default()
        };
        let a = t.intern_dxf(d.clone());
        assert_eq!(t.intern_dxf(d.clone()), a);
        assert_eq!(t.dxf(a), Some(&d));
        let over = Dxf {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..Dxf::default()
        };
        let l = d.layered(&over);
        assert_eq!(l.fill, d.fill);
        let f = l.font.unwrap_or_default();
        assert_eq!(f.bold, Some(true));
        assert_eq!(f.color, Some(Color::rgb(0x9C0006)));
    }

    #[test]
    fn round_trips_through_parts_keeping_indices_and_duplicates() {
        let mut t = StyleTable::default();
        let a = t.patch(0, &bold());
        let mut parts = t.to_parts();
        parts.xfs.push(parts.xfs[a as usize]);
        let back = StyleTable::from_parts(parts.clone()).expect("valid");
        assert_eq!(back.len(), 3);
        assert_eq!(back.get(2), t.get(a));
        assert_eq!(back.to_parts(), parts);
        let bytes = postcard::to_allocvec(&back).expect("encode");
        let again: StyleTable = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(again.to_parts(), parts);
    }

    #[test]
    fn rejects_broken_references() {
        let mut parts = StyleTable::default().to_parts();
        parts.xfs[0].font = 9;
        assert!(StyleTable::from_parts(parts).is_err());
        assert!(StyleTable::from_parts(StyleParts::default()).is_err());
        assert!(StyleTable::default().check(5).is_err());
    }

    #[test]
    fn knows_the_built_in_number_formats() {
        assert_eq!(builtin_num_fmt(14), Some("m/d/yyyy"));
        assert_eq!(builtin_num_fmt_id("0.00%"), Some(10));
        assert_eq!(builtin_num_fmt(23), None);
    }
}
