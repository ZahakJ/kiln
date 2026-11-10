// SPDX-License-Identifier: AGPL-3.0-or-later
//! `styles.xml` ↔ [`kiln_sheet::StyleTable`]: number formats, fonts, fills, borders, cell formats
//! (`cellXfs`), named cell styles, differential formats and custom table styles.
//!
//! Indices are kept exactly (a cell's `s` is its format index in the model). The reader keeps
//! every list element as read, together with the model value it produced; the writer reuses an
//! element whenever the model still holds that value at that index, so attributes and children
//! the model does not cover (`x14ac:knownFonts`, `applyX` flags, extensions) survive, and
//! serialises the rest from the model. `cellStyleXfs`, `colors` and `extLst` are not modelled and
//! are kept as read.

use std::collections::BTreeMap;
use std::sync::Arc;

use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::num::F64;
use kiln_sheet::styles::{
    Alignment, Border, BorderSide, BorderStyle, Dxf, Fill, Font, FontPatch, FontScheme,
    GradientFill, GradientKind, GradientStop, HAlign, NamedCellStyle, PatternType, Protection,
    StyleParts, StyleTable, Underline, VAlign, VertAlign, Xf, builtin_num_fmt, builtin_num_fmt_id,
};
use kiln_sheet::table_style::{TableStyleDef, TableStyleElement, TableStyleEntry};

use crate::xmlx::{
    abool, abool_or, anum, au32, child_local, color_el, parse_color, replace_children,
};

/// The `styles.xml` child order (CT_Stylesheet).
pub(crate) const STYLESHEET_ORDER: &[&str] = &[
    "numFmts",
    "fonts",
    "fills",
    "borders",
    "cellStyleXfs",
    "cellXfs",
    "cellStyles",
    "dxfs",
    "tableStyles",
    "colors",
    "extLst",
];

/// What the reader kept of `styles.xml`.
#[derive(Debug, Clone, Default)]
pub struct StylesSource {
    /// The part as read (lists are replaced on write; everything else is kept).
    pub(crate) root: Option<XmlElement>,
    /// Custom number formats as read: id → code.
    pub(crate) num_fmt_ids: BTreeMap<u32, String>,
    /// Format code → the id it had (the first one when several ids share a code).
    pub(crate) code_ids: BTreeMap<String, u32>,
    /// The model's style parts right after reading.
    pub(crate) parts: StyleParts,
    /// Per list, the elements as read, by index.
    pub(crate) fonts: Vec<XmlElement>,
    pub(crate) fills: Vec<XmlElement>,
    pub(crate) borders: Vec<XmlElement>,
    pub(crate) xfs: Vec<XmlElement>,
    pub(crate) dxfs: Vec<XmlElement>,
    pub(crate) cell_styles: Vec<XmlElement>,
    pub(crate) table_styles: Vec<XmlElement>,
    /// `numFmtId` of each `cellXfs` entry as read (ids whose code the model cannot name, such as
    /// locale-specific built-ins, are written back as read while the entry is unchanged).
    pub(crate) xf_num_fmt_ids: Vec<u32>,
    /// Number of `cellStyleXfs` entries.
    pub(crate) style_xf_count: usize,
}

// ── enumerations ──────────────────────────────────────────────────────────

fn underline_from(v: &str) -> Underline {
    match v {
        "double" => Underline::Double,
        "singleAccounting" => Underline::SingleAccounting,
        "doubleAccounting" => Underline::DoubleAccounting,
        "none" => Underline::None,
        _ => Underline::Single,
    }
}

fn underline_str(u: Underline) -> Option<&'static str> {
    match u {
        Underline::None => None,
        Underline::Single => Some("single"),
        Underline::Double => Some("double"),
        Underline::SingleAccounting => Some("singleAccounting"),
        Underline::DoubleAccounting => Some("doubleAccounting"),
    }
}

fn vert_align_from(v: &str) -> VertAlign {
    match v {
        "superscript" => VertAlign::Superscript,
        "subscript" => VertAlign::Subscript,
        _ => VertAlign::Baseline,
    }
}

fn vert_align_str(v: VertAlign) -> &'static str {
    match v {
        VertAlign::Baseline => "baseline",
        VertAlign::Superscript => "superscript",
        VertAlign::Subscript => "subscript",
    }
}

fn scheme_from(v: &str) -> FontScheme {
    match v {
        "minor" => FontScheme::Minor,
        "major" => FontScheme::Major,
        _ => FontScheme::None,
    }
}

fn scheme_str(s: FontScheme) -> &'static str {
    match s {
        FontScheme::None => "none",
        FontScheme::Minor => "minor",
        FontScheme::Major => "major",
    }
}

const PATTERNS: &[(PatternType, &str)] = &[
    (PatternType::None, "none"),
    (PatternType::Solid, "solid"),
    (PatternType::MediumGray, "mediumGray"),
    (PatternType::DarkGray, "darkGray"),
    (PatternType::LightGray, "lightGray"),
    (PatternType::DarkHorizontal, "darkHorizontal"),
    (PatternType::DarkVertical, "darkVertical"),
    (PatternType::DarkDown, "darkDown"),
    (PatternType::DarkUp, "darkUp"),
    (PatternType::DarkGrid, "darkGrid"),
    (PatternType::DarkTrellis, "darkTrellis"),
    (PatternType::LightHorizontal, "lightHorizontal"),
    (PatternType::LightVertical, "lightVertical"),
    (PatternType::LightDown, "lightDown"),
    (PatternType::LightUp, "lightUp"),
    (PatternType::LightGrid, "lightGrid"),
    (PatternType::LightTrellis, "lightTrellis"),
    (PatternType::Gray125, "gray125"),
    (PatternType::Gray0625, "gray0625"),
];

fn pattern_from(v: &str) -> Option<PatternType> {
    PATTERNS.iter().find(|(_, s)| *s == v).map(|(p, _)| *p)
}

fn pattern_str(p: PatternType) -> &'static str {
    PATTERNS
        .iter()
        .find(|(x, _)| *x == p)
        .map_or("none", |(_, s)| s)
}

const BORDER_STYLES: &[(BorderStyle, &str)] = &[
    (BorderStyle::None, "none"),
    (BorderStyle::Thin, "thin"),
    (BorderStyle::Medium, "medium"),
    (BorderStyle::Dashed, "dashed"),
    (BorderStyle::Dotted, "dotted"),
    (BorderStyle::Thick, "thick"),
    (BorderStyle::Double, "double"),
    (BorderStyle::Hair, "hair"),
    (BorderStyle::MediumDashed, "mediumDashed"),
    (BorderStyle::DashDot, "dashDot"),
    (BorderStyle::MediumDashDot, "mediumDashDot"),
    (BorderStyle::DashDotDot, "dashDotDot"),
    (BorderStyle::MediumDashDotDot, "mediumDashDotDot"),
    (BorderStyle::SlantDashDot, "slantDashDot"),
];

fn border_style_from(v: &str) -> BorderStyle {
    BORDER_STYLES
        .iter()
        .find(|(_, s)| *s == v)
        .map_or(BorderStyle::None, |(b, _)| *b)
}

fn border_style_str(b: BorderStyle) -> &'static str {
    BORDER_STYLES
        .iter()
        .find(|(x, _)| *x == b)
        .map_or("none", |(_, s)| s)
}

const H_ALIGN: &[(HAlign, &str)] = &[
    (HAlign::General, "general"),
    (HAlign::Left, "left"),
    (HAlign::Center, "center"),
    (HAlign::Right, "right"),
    (HAlign::Fill, "fill"),
    (HAlign::Justify, "justify"),
    (HAlign::CenterContinuous, "centerContinuous"),
    (HAlign::Distributed, "distributed"),
];

const V_ALIGN: &[(VAlign, &str)] = &[
    (VAlign::Top, "top"),
    (VAlign::Center, "center"),
    (VAlign::Bottom, "bottom"),
    (VAlign::Justify, "justify"),
    (VAlign::Distributed, "distributed"),
];

const TABLE_ELEMENTS: &[(TableStyleElement, &str)] = &[
    (TableStyleElement::WholeTable, "wholeTable"),
    (TableStyleElement::FirstColumnStripe, "firstColumnStripe"),
    (TableStyleElement::SecondColumnStripe, "secondColumnStripe"),
    (TableStyleElement::FirstRowStripe, "firstRowStripe"),
    (TableStyleElement::SecondRowStripe, "secondRowStripe"),
    (TableStyleElement::LastColumn, "lastColumn"),
    (TableStyleElement::FirstColumn, "firstColumn"),
    (TableStyleElement::HeaderRow, "headerRow"),
    (TableStyleElement::TotalRow, "totalRow"),
    (TableStyleElement::FirstHeaderCell, "firstHeaderCell"),
    (TableStyleElement::LastHeaderCell, "lastHeaderCell"),
    (TableStyleElement::FirstTotalCell, "firstTotalCell"),
    (TableStyleElement::LastTotalCell, "lastTotalCell"),
];

// ── fonts ─────────────────────────────────────────────────────────────────

fn val(e: &XmlElement) -> Option<&str> {
    e.attr("val")
}

/// `<b/>`, `<b val="0"/>`: a toggle element.
fn toggle(e: &XmlElement) -> bool {
    abool(e, "val").unwrap_or(true)
}

/// A rich-text run's or a dxf's font members (`rPr` or `font`).
pub(crate) fn parse_font_patch(e: &XmlElement) -> FontPatch {
    let mut p = FontPatch::default();
    for c in e.elements() {
        match kiln_ooxml::xml::namespaces::local_name(&c.name) {
            "b" => p.bold = Some(toggle(c)),
            "i" => p.italic = Some(toggle(c)),
            "strike" => p.strike = Some(toggle(c)),
            "u" => p.underline = Some(val(c).map_or(Underline::Single, underline_from)),
            "vertAlign" => p.vert_align = val(c).map(vert_align_from),
            "sz" => p.size = anum(c, "val").map(F64),
            "color" => p.color = parse_color(c),
            "name" | "rFont" => p.name = val(c).map(Arc::from),
            "scheme" => p.scheme = val(c).map(scheme_from),
            _ => {}
        }
    }
    p
}

/// A `<font>` of the fonts list.
pub(crate) fn parse_font(e: &XmlElement) -> Font {
    let mut f = Font {
        name: Arc::from("Calibri"),
        size: F64(11.0),
        bold: false,
        italic: false,
        underline: Underline::None,
        strike: false,
        color: None,
        vert_align: VertAlign::Baseline,
        scheme: FontScheme::None,
        family: None,
        charset: None,
        outline: false,
        shadow: false,
        condense: false,
        extend: false,
    };
    for c in e.elements() {
        match kiln_ooxml::xml::namespaces::local_name(&c.name) {
            "b" => f.bold = toggle(c),
            "i" => f.italic = toggle(c),
            "strike" => f.strike = toggle(c),
            "outline" => f.outline = toggle(c),
            "shadow" => f.shadow = toggle(c),
            "condense" => f.condense = toggle(c),
            "extend" => f.extend = toggle(c),
            "u" => f.underline = val(c).map_or(Underline::Single, underline_from),
            "vertAlign" => f.vert_align = val(c).map_or(VertAlign::Baseline, vert_align_from),
            "sz" => {
                if let Some(s) = anum(c, "val").filter(|s| *s > 0.0) {
                    f.size = F64(s);
                }
            }
            "color" => f.color = parse_color(c),
            "name" | "rFont" => {
                if let Some(n) = val(c) {
                    f.name = Arc::from(n);
                }
            }
            "family" => f.family = au32(c, "val").and_then(|v| u8::try_from(v).ok()),
            "charset" => f.charset = au32(c, "val").and_then(|v| u8::try_from(v).ok()),
            "scheme" => f.scheme = val(c).map_or(FontScheme::None, scheme_from),
            _ => {}
        }
    }
    f
}

fn val_el(name: &str, v: impl Into<String>) -> XmlElement {
    XmlElement::new(name).with_attr("val", v.into())
}

/// `<font>` in Excel's child order.
pub(crate) fn font_el(f: &Font) -> XmlElement {
    let mut e = XmlElement::new("font");
    let mut push = |c: XmlElement| e.children.push(XmlNode::Element(c));
    for (on, name) in [
        (f.bold, "b"),
        (f.italic, "i"),
        (f.strike, "strike"),
        (f.condense, "condense"),
        (f.extend, "extend"),
        (f.outline, "outline"),
        (f.shadow, "shadow"),
    ] {
        if on {
            push(XmlElement::new(name));
        }
    }
    if let Some(u) = underline_str(f.underline) {
        push(if u == "single" {
            XmlElement::new("u")
        } else {
            val_el("u", u)
        });
    }
    if f.vert_align != VertAlign::Baseline {
        push(val_el("vertAlign", vert_align_str(f.vert_align)));
    }
    push(XmlElement::new("sz").with_attr("val", f.size.0));
    if let Some(c) = &f.color {
        push(color_el("color", c));
    }
    push(val_el("name", f.name.to_string()));
    if let Some(fam) = f.family {
        push(val_el("family", fam.to_string()));
    }
    if let Some(cs) = f.charset {
        push(val_el("charset", cs.to_string()));
    }
    if f.scheme != FontScheme::None {
        push(val_el("scheme", scheme_str(f.scheme)));
    }
    e
}

/// A font patch as `rPr` (rich-text run, `rFont`) or as a dxf `font` (`name`).
pub(crate) fn font_patch_el(p: &FontPatch, name: &str, run: bool) -> XmlElement {
    let mut e = XmlElement::new(name);
    let mut push = |c: XmlElement| e.children.push(XmlNode::Element(c));
    let bool_el = |n: &str, v: bool| {
        if v {
            XmlElement::new(n)
        } else {
            val_el(n, "0")
        }
    };
    if run && let Some(n) = &p.name {
        push(val_el("rFont", n.to_string()));
    }
    if let Some(b) = p.bold {
        push(bool_el("b", b));
    }
    if let Some(i) = p.italic {
        push(bool_el("i", i));
    }
    if let Some(s) = p.strike {
        push(bool_el("strike", s));
    }
    if run {
        if let Some(c) = &p.color {
            push(color_el("color", c));
        }
        if let Some(s) = p.size {
            push(XmlElement::new("sz").with_attr("val", s.0));
        }
    }
    if let Some(u) = p.underline {
        push(val_el("u", underline_str(u).unwrap_or("none")));
    }
    if let Some(v) = p.vert_align {
        push(val_el("vertAlign", vert_align_str(v)));
    }
    if !run {
        if let Some(s) = p.size {
            push(XmlElement::new("sz").with_attr("val", s.0));
        }
        if let Some(c) = &p.color {
            push(color_el("color", c));
        }
        if let Some(n) = &p.name {
            push(val_el("name", n.to_string()));
        }
    }
    if let Some(s) = p.scheme {
        push(val_el("scheme", scheme_str(s)));
    }
    e
}

// ── fills ─────────────────────────────────────────────────────────────────

/// A `<fill>` (`dxf`: the differential form, where a solid fill's colour is `bgColor`).
pub(crate) fn parse_fill(e: &XmlElement, dxf: bool) -> Fill {
    if let Some(g) = child_local(e, "gradientFill") {
        let kind = if g.attr("type") == Some("path") {
            GradientKind::Path
        } else {
            GradientKind::Linear
        };
        let stops = g
            .elements()
            .filter(|s| kiln_ooxml::xml::namespaces::local_name(&s.name) == "stop")
            .filter_map(|s| {
                Some(GradientStop {
                    position: F64(anum(s, "position").unwrap_or(0.0)),
                    color: child_local(s, "color").and_then(parse_color)?,
                })
            })
            .collect();
        return Fill::Gradient(GradientFill {
            kind,
            degree: F64(anum(g, "degree").unwrap_or(0.0)),
            left: F64(anum(g, "left").unwrap_or(0.0)),
            right: F64(anum(g, "right").unwrap_or(0.0)),
            top: F64(anum(g, "top").unwrap_or(0.0)),
            bottom: F64(anum(g, "bottom").unwrap_or(0.0)),
            stops,
        });
    }
    let Some(p) = child_local(e, "patternFill") else {
        return Fill::NONE;
    };
    let fg = child_local(p, "fgColor").and_then(parse_color);
    let bg = child_local(p, "bgColor").and_then(parse_color);
    let declared = p.attr("patternType").and_then(pattern_from);
    if dxf {
        let pattern = declared.unwrap_or(if fg.is_some() || bg.is_some() {
            PatternType::Solid
        } else {
            PatternType::None
        });
        if pattern == PatternType::Solid {
            // Excel paints a differential solid fill with its background colour.
            return Fill::Pattern {
                pattern,
                fg: bg.or(fg),
                bg: None,
            };
        }
        return Fill::Pattern { pattern, fg, bg };
    }
    Fill::Pattern {
        pattern: declared.unwrap_or(PatternType::None),
        fg,
        bg,
    }
}

pub(crate) fn fill_el(f: &Fill, dxf: bool) -> XmlElement {
    let inner = match f {
        Fill::Gradient(g) => {
            let mut e = XmlElement::new("gradientFill");
            if g.kind == GradientKind::Path {
                e.attrs.set("type", "path");
            }
            for (name, v) in [
                ("degree", g.degree),
                ("left", g.left),
                ("right", g.right),
                ("top", g.top),
                ("bottom", g.bottom),
            ] {
                if v.0 != 0.0 {
                    e.attrs.set(name, kiln_ooxml::xml::format_number(v.0));
                }
            }
            for s in &g.stops {
                e.children.push(XmlNode::Element(
                    XmlElement::new("stop")
                        .with_attr("position", s.position.0)
                        .with_child(color_el("color", &s.color)),
                ));
            }
            e
        }
        Fill::Pattern { pattern, fg, bg } => {
            let mut e = XmlElement::new("patternFill");
            if dxf && *pattern == PatternType::Solid {
                e.attrs.set("patternType", "solid");
                if let Some(c) = fg {
                    e.children.push(XmlNode::Element(color_el("fgColor", c)));
                    e.children.push(XmlNode::Element(color_el("bgColor", c)));
                }
                return XmlElement::new("fill").with_child(e);
            }
            if *pattern != PatternType::None || !dxf {
                e.attrs.set("patternType", pattern_str(*pattern));
            }
            if let Some(c) = fg {
                e.children.push(XmlNode::Element(color_el("fgColor", c)));
            }
            if let Some(c) = bg {
                e.children.push(XmlNode::Element(color_el("bgColor", c)));
            }
            e
        }
    };
    XmlElement::new("fill").with_child(inner)
}

// ── borders ───────────────────────────────────────────────────────────────

fn parse_side(e: Option<&XmlElement>) -> Option<BorderSide> {
    let e = e?;
    let style = border_style_from(e.attr("style")?);
    (style != BorderStyle::None).then(|| BorderSide {
        style,
        color: child_local(e, "color").and_then(parse_color),
    })
}

pub(crate) fn parse_border(e: &XmlElement) -> Border {
    Border {
        left: parse_side(child_local(e, "left").or_else(|| child_local(e, "start"))),
        right: parse_side(child_local(e, "right").or_else(|| child_local(e, "end"))),
        top: parse_side(child_local(e, "top")),
        bottom: parse_side(child_local(e, "bottom")),
        diagonal: parse_side(child_local(e, "diagonal")),
        diagonal_up: abool_or(e, "diagonalUp", false),
        diagonal_down: abool_or(e, "diagonalDown", false),
    }
}

pub(crate) fn border_el(b: &Border, dxf: bool) -> XmlElement {
    let mut e = XmlElement::new("border");
    if b.diagonal_up {
        e.attrs.set("diagonalUp", "1");
    }
    if b.diagonal_down {
        e.attrs.set("diagonalDown", "1");
    }
    for (name, side) in [
        ("left", b.left),
        ("right", b.right),
        ("top", b.top),
        ("bottom", b.bottom),
        ("diagonal", b.diagonal),
    ] {
        let mut s = XmlElement::new(name);
        match side {
            Some(side) => {
                s.attrs.set("style", border_style_str(side.style));
                if let Some(c) = &side.color {
                    s.children.push(XmlNode::Element(color_el("color", c)));
                }
            }
            None if dxf => continue,
            None => {}
        }
        e.children.push(XmlNode::Element(s));
    }
    e
}

// ── alignment, protection ─────────────────────────────────────────────────

pub(crate) fn parse_alignment(e: &XmlElement) -> Alignment {
    let byte = |n: &str| au32(e, n).map_or(0, |v| u8::try_from(v.min(255)).unwrap_or(0));
    Alignment {
        horizontal: e
            .attr("horizontal")
            .and_then(|v| H_ALIGN.iter().find(|(_, s)| *s == v))
            .map_or(HAlign::General, |(h, _)| *h),
        vertical: e
            .attr("vertical")
            .and_then(|v| V_ALIGN.iter().find(|(_, s)| *s == v))
            .map_or(VAlign::Bottom, |(h, _)| *h),
        wrap_text: abool_or(e, "wrapText", false),
        shrink_to_fit: abool_or(e, "shrinkToFit", false),
        indent: byte("indent"),
        text_rotation: byte("textRotation"),
        reading_order: byte("readingOrder"),
        justify_last_line: abool_or(e, "justifyLastLine", false),
    }
}

pub(crate) fn alignment_el(a: &Alignment) -> XmlElement {
    let mut e = XmlElement::new("alignment");
    if a.horizontal != HAlign::General {
        let h = H_ALIGN
            .iter()
            .find(|(x, _)| *x == a.horizontal)
            .map_or("general", |(_, s)| s);
        e.attrs.set("horizontal", h);
    }
    if a.vertical != VAlign::Bottom {
        let v = V_ALIGN
            .iter()
            .find(|(x, _)| *x == a.vertical)
            .map_or("bottom", |(_, s)| s);
        e.attrs.set("vertical", v);
    }
    if a.text_rotation != 0 {
        e.attrs.set("textRotation", a.text_rotation.to_string());
    }
    if a.wrap_text {
        e.attrs.set("wrapText", "1");
    }
    if a.indent != 0 {
        e.attrs.set("indent", a.indent.to_string());
    }
    if a.justify_last_line {
        e.attrs.set("justifyLastLine", "1");
    }
    if a.shrink_to_fit {
        e.attrs.set("shrinkToFit", "1");
    }
    if a.reading_order != 0 {
        e.attrs.set("readingOrder", a.reading_order.to_string());
    }
    e
}

pub(crate) fn parse_protection(e: &XmlElement) -> Protection {
    Protection {
        locked: abool_or(e, "locked", true),
        hidden: abool_or(e, "hidden", false),
    }
}

pub(crate) fn protection_el(p: &Protection) -> XmlElement {
    let mut e = XmlElement::new("protection");
    if !p.locked {
        e.attrs.set("locked", "0");
    }
    if p.hidden {
        e.attrs.set("hidden", "1");
    }
    e
}

// ── reading ───────────────────────────────────────────────────────────────

/// Number format codes: the model list and the id → index map.
struct NumFmts {
    codes: Vec<String>,
    index: BTreeMap<String, u32>,
}

impl NumFmts {
    fn intern(&mut self, code: &str) -> u32 {
        if let Some(&i) = self.index.get(code) {
            return i;
        }
        let i = self.codes.len() as u32;
        self.codes.push(code.to_owned());
        self.index.insert(code.to_owned(), i);
        i
    }
}

/// The code of a number format id (custom, built in, or General for unknown built-ins).
fn code_for(id: u32, custom: &BTreeMap<u32, String>) -> String {
    custom
        .get(&id)
        .cloned()
        .or_else(|| builtin_num_fmt(id).map(str::to_owned))
        .unwrap_or_else(|| "General".to_owned())
}

pub(crate) fn parse_dxf(e: &XmlElement, custom: &BTreeMap<u32, String>) -> Dxf {
    let num_fmt = child_local(e, "numFmt").map(|n| {
        let code = n.attr("formatCode").map_or_else(
            || code_for(au32(n, "numFmtId").unwrap_or(0), custom),
            str::to_owned,
        );
        Arc::from(code.as_str())
    });
    Dxf {
        num_fmt,
        font: child_local(e, "font").map(parse_font_patch),
        fill: child_local(e, "fill").map(|f| parse_fill(f, true)),
        border: child_local(e, "border").map(parse_border),
        alignment: child_local(e, "alignment").map(parse_alignment),
        protection: child_local(e, "protection").map(parse_protection),
    }
}

/// Parses `styles.xml` into style parts (indices kept) and what the writer needs to reuse.
pub(crate) fn read_styles(
    root: &XmlElement,
    warnings: &mut Vec<String>,
) -> (StyleParts, StylesSource) {
    let mut src = StylesSource::default();
    for n in root
        .child("numFmts")
        .into_iter()
        .flat_map(|n| n.children_named("numFmt"))
    {
        if let (Some(id), Some(code)) = (au32(n, "numFmtId"), n.attr("formatCode")) {
            src.num_fmt_ids.insert(id, code.to_owned());
            src.code_ids.entry(code.to_owned()).or_insert(id);
        }
    }
    let list = |name: &str, item: &str| -> Vec<XmlElement> {
        root.child(name)
            .map(|l| l.children_named(item).cloned().collect())
            .unwrap_or_default()
    };
    src.fonts = list("fonts", "font");
    src.fills = list("fills", "fill");
    src.borders = list("borders", "border");
    src.xfs = list("cellXfs", "xf");
    src.dxfs = list("dxfs", "dxf");
    src.cell_styles = list("cellStyles", "cellStyle");
    src.table_styles = list("tableStyles", "tableStyle");
    src.style_xf_count = root
        .child("cellStyleXfs")
        .map_or(0, |l| l.children_named("xf").count());

    let mut fmts = NumFmts {
        codes: vec!["General".to_owned()],
        index: BTreeMap::from([("General".to_owned(), 0)]),
    };
    let mut fonts: Vec<Font> = src.fonts.iter().map(parse_font).collect();
    if fonts.is_empty() {
        fonts.push(Font::default());
    }
    let mut fills: Vec<Fill> = src.fills.iter().map(|f| parse_fill(f, false)).collect();
    if fills.is_empty() {
        fills = vec![Fill::NONE, Fill::GRAY125];
    }
    let mut borders: Vec<Border> = src.borders.iter().map(parse_border).collect();
    if borders.is_empty() {
        borders.push(Border::default());
    }
    // Named styles: the model index is the position in cellStyles; `xf` is the cellStyleXfs index.
    let named: Vec<NamedCellStyle> = src
        .cell_styles
        .iter()
        .map(|c| NamedCellStyle {
            name: c.attr("name").unwrap_or("").to_owned(),
            builtin_id: au32(c, "builtinId"),
            xf: au32(c, "xfId").unwrap_or(0),
            hidden: abool_or(c, "hidden", false),
            custom_builtin: abool_or(c, "customBuiltin", false),
        })
        .collect();
    let style_of_xf = |xf_id: u32| -> u32 {
        named
            .iter()
            .position(|n| n.xf == xf_id)
            .map_or(0, |i| i as u32)
    };
    let clamp = |v: Option<u32>, len: usize, what: &str, warnings: &mut Vec<String>| -> u32 {
        let v = v.unwrap_or(0);
        if (v as usize) < len {
            v
        } else {
            warnings.push(format!(
                "a cell format refers to {what} {v}, which does not exist"
            ));
            0
        }
    };
    let mut xfs = Vec::with_capacity(src.xfs.len().max(1));
    for x in &src.xfs {
        let id = au32(x, "numFmtId").unwrap_or(0);
        src.xf_num_fmt_ids.push(id);
        let code = code_for(id, &src.num_fmt_ids);
        let num_fmt = fmts.intern(&code);
        xfs.push(Xf {
            font: clamp(au32(x, "fontId"), fonts.len(), "font", warnings),
            fill: clamp(au32(x, "fillId"), fills.len(), "fill", warnings),
            border: clamp(au32(x, "borderId"), borders.len(), "border", warnings),
            num_fmt,
            alignment: x
                .child("alignment")
                .map(parse_alignment)
                .unwrap_or_default(),
            protection: x
                .child("protection")
                .map(parse_protection)
                .unwrap_or_default(),
            style_id: style_of_xf(au32(x, "xfId").unwrap_or(0)),
            quote_prefix: abool_or(x, "quotePrefix", false),
        });
    }
    if xfs.is_empty() {
        xfs.push(Xf::default());
    }
    let dxfs: Vec<Dxf> = src
        .dxfs
        .iter()
        .map(|d| parse_dxf(d, &src.num_fmt_ids))
        .collect();
    let dxf_count = dxfs.len() as u32;
    let table_styles: Vec<TableStyleDef> = src
        .table_styles
        .iter()
        .map(|t| TableStyleDef {
            name: t.attr("name").unwrap_or("").to_owned(),
            table: abool_or(t, "table", true),
            pivot: abool_or(t, "pivot", true),
            elements: t
                .children_named("tableStyleElement")
                .filter_map(|el| {
                    let kind = el.attr("type")?;
                    let element = TABLE_ELEMENTS.iter().find(|(_, s)| *s == kind)?.0;
                    Some(TableStyleEntry {
                        element,
                        dxf: au32(el, "dxfId").filter(|d| *d < dxf_count)?,
                        size: au32(el, "size").unwrap_or(1).max(1),
                    })
                })
                .collect(),
        })
        .collect();
    let parts = StyleParts {
        fonts,
        fills,
        borders,
        num_fmts: fmts.codes,
        xfs,
        dxfs,
        named_styles: named,
        table_styles,
        default_table_style: root
            .child("tableStyles")
            .and_then(|t| t.attr("defaultTableStyle"))
            .map(str::to_owned),
    };
    src.root = Some(root.clone());
    (parts, src)
}

/// The model parts of a table built from the parts (fills and borders normalised), for
/// comparisons with what the writer is given later.
pub(crate) fn settle(src: &mut StylesSource, table: &StyleTable) {
    src.parts = table.to_parts();
}

// ── writing ───────────────────────────────────────────────────────────────

/// Number format ids for writing.
pub(crate) struct NumFmtIds {
    /// Model code index → numFmtId.
    pub(crate) by_index: Vec<u32>,
    /// Code → id for dxfs.
    pub(crate) by_code: BTreeMap<String, u32>,
    /// Custom formats to declare: id → code.
    pub(crate) custom: BTreeMap<u32, String>,
}

impl NumFmtIds {
    fn id_for(&mut self, code: &str) -> u32 {
        if let Some(&id) = self.by_code.get(code) {
            return id;
        }
        let id = builtin_num_fmt_id(code).unwrap_or_else(|| {
            let next = self
                .custom
                .keys()
                .next_back()
                .map_or(164, |k| (k + 1).max(164));
            self.custom.insert(next, code.to_owned());
            next
        });
        self.by_code.insert(code.to_owned(), id);
        id
    }
}

fn reuse_or<T: PartialEq>(
    i: usize,
    value: &T,
    read: &[T],
    xml: &[XmlElement],
    make: impl FnOnce() -> XmlElement,
) -> XmlElement {
    match (read.get(i), xml.get(i)) {
        (Some(r), Some(x)) if r == value => x.clone(),
        _ => make(),
    }
}

fn list_el(original: Option<&XmlElement>, name: &str, items: Vec<XmlElement>) -> XmlElement {
    let mut e = original.cloned().unwrap_or_else(|| XmlElement::new(name));
    e.children.clear();
    e.attrs.set("count", items.len().to_string());
    e.children.extend(items.into_iter().map(XmlNode::Element));
    e
}

/// The default `styles.xml` root of a new workbook.
fn template() -> XmlElement {
    let mut root = XmlElement::new("styleSheet");
    root.attrs.set(
        "xmlns",
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    );
    root.children.push(XmlNode::Element(
        XmlElement::new("cellStyleXfs")
            .with_attr("count", "1")
            .with_child(
                XmlElement::new("xf")
                    .with_attr("numFmtId", "0")
                    .with_attr("fontId", "0")
                    .with_attr("fillId", "0")
                    .with_attr("borderId", "0"),
            ),
    ));
    root
}

fn xf_el(x: &Xf, num_fmt_id: u32, xf_id: u32) -> XmlElement {
    let mut e = XmlElement::new("xf")
        .with_attr("numFmtId", num_fmt_id)
        .with_attr("fontId", x.font)
        .with_attr("fillId", x.fill)
        .with_attr("borderId", x.border)
        .with_attr("xfId", xf_id);
    if x.quote_prefix {
        e.attrs.set("quotePrefix", "1");
    }
    if num_fmt_id != 0 {
        e.attrs.set("applyNumberFormat", "1");
    }
    if x.font != 0 {
        e.attrs.set("applyFont", "1");
    }
    if x.fill != 0 {
        e.attrs.set("applyFill", "1");
    }
    if x.border != 0 {
        e.attrs.set("applyBorder", "1");
    }
    if x.alignment != Alignment::default() {
        e.attrs.set("applyAlignment", "1");
        e.children
            .push(XmlNode::Element(alignment_el(&x.alignment)));
    }
    if x.protection != Protection::default() {
        e.attrs.set("applyProtection", "1");
        e.children
            .push(XmlNode::Element(protection_el(&x.protection)));
    }
    e
}

pub(crate) fn dxf_el(d: &Dxf, ids: &mut NumFmtIds) -> XmlElement {
    let mut e = XmlElement::new("dxf");
    if let Some(f) = &d.font {
        e.children
            .push(XmlNode::Element(font_patch_el(f, "font", false)));
    }
    if let Some(code) = &d.num_fmt {
        let id = ids.id_for(code);
        e.children.push(XmlNode::Element(
            XmlElement::new("numFmt")
                .with_attr("numFmtId", id)
                .with_attr("formatCode", code.to_string()),
        ));
    }
    if let Some(f) = &d.fill {
        e.children.push(XmlNode::Element(fill_el(f, true)));
    }
    if let Some(a) = &d.alignment {
        e.children.push(XmlNode::Element(alignment_el(a)));
    }
    if let Some(b) = &d.border {
        e.children.push(XmlNode::Element(border_el(b, true)));
    }
    if let Some(p) = &d.protection {
        e.children.push(XmlNode::Element(protection_el(p)));
    }
    e
}

/// Builds `styles.xml` from the model, reusing the elements of the file read where the model
/// still holds what they said.
pub(crate) fn write_styles(table: &StyleTable, src: Option<&StylesSource>) -> XmlElement {
    let empty = StylesSource::default();
    let src = src.unwrap_or(&empty);
    let parts = table.to_parts();
    let read = &src.parts;
    let mut root = src.root.clone().unwrap_or_else(template);

    // Number formats: every custom format of the file is kept (ids stay stable), new codes get
    // fresh ids from 164.
    let mut ids = NumFmtIds {
        by_index: Vec::with_capacity(parts.num_fmts.len()),
        by_code: src.code_ids.clone(),
        custom: src.num_fmt_ids.clone(),
    };
    for code in &parts.num_fmts {
        let id = ids.id_for(code);
        ids.by_index.push(id);
    }

    let fonts = parts
        .fonts
        .iter()
        .enumerate()
        .map(|(i, f)| reuse_or(i, f, &read.fonts, &src.fonts, || font_el(f)))
        .collect();
    let fills = parts
        .fills
        .iter()
        .enumerate()
        .map(|(i, f)| reuse_or(i, f, &read.fills, &src.fills, || fill_el(f, false)))
        .collect();
    let borders = parts
        .borders
        .iter()
        .enumerate()
        .map(|(i, b)| reuse_or(i, b, &read.borders, &src.borders, || border_el(b, false)))
        .collect();
    // Every named style needs its cellStyleXfs entry: entries missing from the file read (or
    // all of them for a new workbook) are added with the default format.
    let needed = parts
        .named_styles
        .iter()
        .map(|n| n.xf as usize + 1)
        .max()
        .unwrap_or(1)
        .max(src.style_xf_count)
        .max(1);
    let style_xf_count = needed as u32;
    let xfs = parts
        .xfs
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let same_codes = read
                .num_fmts
                .get(read.xfs.get(i).map_or(u32::MAX, |r| r.num_fmt) as usize)
                == parts.num_fmts.get(x.num_fmt as usize);
            match (read.xfs.get(i), src.xfs.get(i)) {
                (Some(r), Some(orig)) if r == x && same_codes => orig.clone(),
                _ => {
                    let num_fmt_id = ids.by_index.get(x.num_fmt as usize).copied().unwrap_or(0);
                    let xf_id = parts
                        .named_styles
                        .get(x.style_id as usize)
                        .map_or(0, |n| n.xf)
                        .min(style_xf_count - 1);
                    xf_el(x, num_fmt_id, xf_id)
                }
            }
        })
        .collect();
    let dxfs: Vec<XmlElement> = parts
        .dxfs
        .iter()
        .enumerate()
        .map(|(i, d)| match (read.dxfs.get(i), src.dxfs.get(i)) {
            (Some(r), Some(orig)) if r == d => orig.clone(),
            _ => dxf_el(d, &mut ids),
        })
        .collect();
    let cell_styles: Vec<XmlElement> = parts
        .named_styles
        .iter()
        .enumerate()
        .map(
            |(i, n)| match (read.named_styles.get(i), src.cell_styles.get(i)) {
                (Some(r), Some(orig)) if r == n => orig.clone(),
                _ => {
                    let mut e = XmlElement::new("cellStyle")
                        .with_attr("name", n.name.clone())
                        .with_attr("xfId", n.xf.min(style_xf_count - 1));
                    if let Some(b) = n.builtin_id {
                        e.attrs.set("builtinId", b.to_string());
                    }
                    if n.hidden {
                        e.attrs.set("hidden", "1");
                    }
                    if n.custom_builtin {
                        e.attrs.set("customBuiltin", "1");
                    }
                    e
                }
            },
        )
        .collect();
    let table_styles: Vec<XmlElement> = parts
        .table_styles
        .iter()
        .enumerate()
        .map(
            |(i, t)| match (read.table_styles.get(i), src.table_styles.get(i)) {
                (Some(r), Some(orig)) if r == t => orig.clone(),
                _ => {
                    let mut e = XmlElement::new("tableStyle").with_attr("name", t.name.clone());
                    if !t.pivot {
                        e.attrs.set("pivot", "0");
                    }
                    if !t.table {
                        e.attrs.set("table", "0");
                    }
                    e.attrs.set("count", t.elements.len().to_string());
                    for el in &t.elements {
                        let kind = TABLE_ELEMENTS
                            .iter()
                            .find(|(k, _)| *k == el.element)
                            .map_or("wholeTable", |(_, s)| s);
                        let mut x = XmlElement::new("tableStyleElement")
                            .with_attr("type", kind)
                            .with_attr("dxfId", el.dxf);
                        if el.size > 1 {
                            x.attrs.set("size", el.size.to_string());
                        }
                        e.children.push(XmlNode::Element(x));
                    }
                    e
                }
            },
        )
        .collect();

    let numfmts: Vec<XmlElement> = ids
        .custom
        .iter()
        .map(|(id, code)| {
            XmlElement::new("numFmt")
                .with_attr("numFmtId", *id)
                .with_attr("formatCode", code.clone())
        })
        .collect();
    let numfmts_el =
        (!numfmts.is_empty()).then(|| list_el(root.child("numFmts"), "numFmts", numfmts));
    replace_children(
        &mut root,
        "numFmts",
        numfmts_el.into_iter().collect(),
        STYLESHEET_ORDER,
    );
    let fonts_el = list_el(root.child("fonts"), "fonts", fonts);
    replace_children(&mut root, "fonts", vec![fonts_el], STYLESHEET_ORDER);
    let fills_el = list_el(root.child("fills"), "fills", fills);
    replace_children(&mut root, "fills", vec![fills_el], STYLESHEET_ORDER);
    let borders_el = list_el(root.child("borders"), "borders", borders);
    replace_children(&mut root, "borders", vec![borders_el], STYLESHEET_ORDER);
    let have = root
        .child("cellStyleXfs")
        .map_or(0, |l| l.children_named("xf").count());
    if have < needed {
        let mut csx = root
            .child("cellStyleXfs")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("cellStyleXfs"));
        for _ in have..needed {
            csx.children.push(XmlNode::Element(
                XmlElement::new("xf")
                    .with_attr("numFmtId", "0")
                    .with_attr("fontId", "0")
                    .with_attr("fillId", "0")
                    .with_attr("borderId", "0"),
            ));
        }
        csx.attrs.set("count", needed.to_string());
        replace_children(&mut root, "cellStyleXfs", vec![csx], STYLESHEET_ORDER);
    }
    let xfs_el = list_el(root.child("cellXfs"), "cellXfs", xfs);
    replace_children(&mut root, "cellXfs", vec![xfs_el], STYLESHEET_ORDER);
    let cs_el = list_el(root.child("cellStyles"), "cellStyles", cell_styles);
    replace_children(&mut root, "cellStyles", vec![cs_el], STYLESHEET_ORDER);
    let dxfs_el = (!dxfs.is_empty() || root.child("dxfs").is_some())
        .then(|| list_el(root.child("dxfs"), "dxfs", dxfs));
    replace_children(
        &mut root,
        "dxfs",
        dxfs_el.into_iter().collect(),
        STYLESHEET_ORDER,
    );
    let had_ts = root.child("tableStyles").is_some();
    if had_ts || !table_styles.is_empty() || parts.default_table_style.is_some() {
        let mut ts = list_el(root.child("tableStyles"), "tableStyles", table_styles);
        crate::xmlx::set_attr(
            &mut ts,
            "defaultTableStyle",
            parts.default_table_style.clone(),
        );
        replace_children(&mut root, "tableStyles", vec![ts], STYLESHEET_ORDER);
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;
    use kiln_sheet::Color;

    fn root(xml: &str) -> XmlElement {
        parse_xml_str(xml).expect("xml").root
    }

    #[test]
    fn fonts_fills_and_borders_round_trip_through_elements() {
        let f = Font {
            bold: true,
            underline: Underline::Double,
            vert_align: VertAlign::Superscript,
            color: Some(Color::theme(4, 0.4)),
            charset: Some(1),
            ..Font::default()
        };
        assert_eq!(parse_font(&font_el(&f)), f);
        for fill in [
            Fill::solid(Color::rgb(0xFF0000)),
            Fill::GRAY125,
            Fill::Gradient(GradientFill {
                kind: GradientKind::Path,
                degree: F64(0.0),
                left: F64(0.5),
                right: F64(0.5),
                top: F64(0.5),
                bottom: F64(0.5),
                stops: vec![
                    GradientStop {
                        position: F64(0.0),
                        color: Color::rgb(0xFFFFFF),
                    },
                    GradientStop {
                        position: F64(1.0),
                        color: Color::theme(4, 0.0),
                    },
                ],
            }),
        ] {
            assert_eq!(parse_fill(&fill_el(&fill, false), false), fill);
        }
        let solid = Fill::solid(Color::rgb(0x00FF00));
        assert_eq!(parse_fill(&fill_el(&solid, true), true), solid);
        let b = Border {
            left: Some(BorderSide::THIN),
            bottom: Some(BorderSide {
                style: BorderStyle::Double,
                color: Some(Color::rgb(0x112233)),
            }),
            diagonal_up: true,
            ..Border::default()
        };
        assert_eq!(parse_border(&border_el(&b, false)), b);
        assert_eq!(parse_border(&border_el(&b, true)), b);
    }

    #[test]
    fn dxf_solid_fills_use_the_background_colour() {
        let d = root(
            r#"<dxf xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf>"#,
        );
        let dxf = parse_dxf(&d, &BTreeMap::new());
        assert_eq!(
            dxf.fill,
            Some(Fill::solid(Color::hex("FFC7CE").expect("hex")))
        );
    }

    #[test]
    fn styles_read_and_write_keep_indices_and_unknown_markup() {
        let xml = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac"><numFmts count="1"><numFmt numFmtId="164" formatCode="0.000"/></numFmts><fonts count="2" x14ac:knownFonts="1"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="164" fontId="1" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="14" fontId="9" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="0"/><tableStyles count="0" defaultTableStyle="TableStyleMedium2"/><extLst><ext uri="{X}"/></extLst></styleSheet>"#;
        let mut warnings = Vec::new();
        let (parts, mut src) = read_styles(&root(xml), &mut warnings);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(parts.xfs.len(), 3);
        assert_eq!(parts.num_fmts[parts.xfs[1].num_fmt as usize], "0.000");
        assert_eq!(parts.num_fmts[parts.xfs[2].num_fmt as usize], "m/d/yyyy");
        assert!(parts.fonts[1].bold);
        let table = StyleTable::from_parts(parts).expect("table");
        settle(&mut src, &table);
        let out = write_styles(&table, Some(&src));
        // Unchanged entries are the elements read (applyNumberFormat survives).
        let xfs = out.child("cellXfs").expect("cellXfs");
        assert_eq!(
            xfs.children_named("xf")
                .nth(1)
                .and_then(|x| x.attr("applyNumberFormat")),
            Some("1")
        );
        assert_eq!(
            out.child("fonts").and_then(|f| f.attr("x14ac:knownFonts")),
            Some("1")
        );
        assert!(out.child("extLst").is_some());
        // Reading the output gives the same model.
        let (again, _) = read_styles(&out, &mut Vec::new());
        assert_eq!(
            StyleTable::from_parts(again).expect("again").to_parts(),
            table.to_parts()
        );
    }

    #[test]
    fn new_formats_get_fresh_ids() {
        let mut table = StyleTable::default();
        let mut style = table.get(0);
        style.num_fmt = Arc::from("0.0000\" kg\"");
        let i = table.intern(&style);
        style.num_fmt = Arc::from("0%");
        let j = table.intern(&style);
        let out = write_styles(&table, None);
        let ids: Vec<&str> = out
            .child("cellXfs")
            .expect("xfs")
            .children_named("xf")
            .map(|x| x.attr("numFmtId").unwrap_or(""))
            .collect();
        assert_eq!(ids[i as usize], "164");
        assert_eq!(ids[j as usize], "9");
        let (parts, _) = read_styles(&out, &mut Vec::new());
        let again = StyleTable::from_parts(parts).expect("table");
        assert_eq!(&*again.get(i).num_fmt, "0.0000\" kg\"");
        assert_eq!(&*again.get(j).num_fmt, "0%");
    }
}
