// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property parsers: `w:rPr`, `w:pPr`, `w:tblPr`, `w:trPr`, `w:tcPr` and their building blocks
//! (colours, borders, shading, tabs). Every child the model has no field for is kept as raw XML so
//! nothing is lost on save.

use kiln_doc::props::{
    Border, CellBorders, CellMargins, ColorValue, DocColor, DropCap, Indent, Lang, NumberingRef,
    ParagraphBorders, ParagraphProps, RowHeight, RunFonts, RunProps, Shading, Spacing, TabStop,
    TableBorders, TableCellProps, TableLook, TablePosition, TableProps, TableRowProps, Underline,
};
use kiln_doc::revision::{InsDel, InsDelRevision};
use kiln_doc::st::{
    Alignment, BorderStyle, CellVAlign, DropCapKind, HeightRule, HighlightColor, LineRule,
    ShadingPattern, TabAlignment, TabLeader, TableAlignment, TableLayout, TextAlignment,
    TextDirection, ThemeColorName, ThemeFontRef, UnderlineStyle, VMerge, VertAlign, WidthType,
};
use kiln_doc::{RevisionInfo, TableWidth};
use kiln_ooxml::xml::{Attrs, XmlElement};

use crate::common::units::{
    parse_decimal, parse_half_points, parse_id, parse_on_off, parse_table_width, parse_twips,
    parse_u32,
};
use crate::common::xml::{attr, attr_of, fragment_xml, val};

/// Serialises unmodelled elements in the namespace scope of their part.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Raw<'a> {
    pub scope: &'a Attrs,
}

impl Raw<'_> {
    pub(crate) fn xml(&self, node: &XmlElement) -> String {
        fragment_xml(node, self.scope)
    }
}

/// A WordprocessingML toggle element: present = on unless `w:val` says off.
fn on_off(node: &XmlElement) -> bool {
    parse_on_off(val(Some(node)), true)
}

fn hex_byte(v: Option<&str>) -> Option<u8> {
    let s = v?.trim();
    if s.is_empty() || s.len() > 2 {
        return None;
    }
    u8::from_str_radix(s, 16).ok()
}

/// `w:val` colour text: `RRGGBB`, `auto`, or an `AARRGGBB` some producers write.
fn color_value(v: Option<&str>) -> ColorValue {
    let Some(s) = v.map(str::trim) else {
        return ColorValue::Auto;
    };
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return ColorValue::parse(&s[2..]).unwrap_or_default();
    }
    ColorValue::parse(s).unwrap_or_default()
}

/// A colour from `value_attr` plus `{prefix}Color`/`Tint`/`Shade` theme attributes (`w:color`
/// uses `w:val` + `w:theme…`; borders use `w:color` + `w:theme…`).
pub(crate) fn parse_color(node: &XmlElement, value_attr: &str) -> Option<DocColor> {
    let v = attr(node, value_attr);
    let theme = attr(node, "w:themeColor");
    if v.is_none() && theme.is_none() {
        return None;
    }
    Some(DocColor {
        val: color_value(v),
        theme_color: theme.and_then(ThemeColorName::parse),
        theme_tint: hex_byte(attr(node, "w:themeTint")),
        theme_shade: hex_byte(attr(node, "w:themeShade")),
    })
}

/// One border edge (`w:top`, `w:insideH`, …).
pub(crate) fn parse_border(node: &XmlElement) -> Option<Border> {
    let style = val(Some(node))?;
    Some(Border {
        style: BorderStyle::parse(style),
        size: parse_u32(attr(node, "w:sz")),
        space: parse_u32(attr(node, "w:space")),
        color: parse_color(node, "w:color"),
        shadow: attr(node, "w:shadow").map(|v| parse_on_off(Some(v), true)),
        frame: attr(node, "w:frame").map(|v| parse_on_off(Some(v), true)),
    })
}

/// The edge name of a border child, with the bidi-aware `start`/`end` mapped to left/right.
fn edge_name(name: &str) -> &str {
    match kiln_ooxml::xml::namespaces::local_name(name) {
        "start" => "left",
        "end" => "right",
        other => other,
    }
}

pub(crate) fn parse_paragraph_borders(node: &XmlElement) -> Option<ParagraphBorders> {
    let mut b = ParagraphBorders::default();
    for c in node.elements() {
        let slot = match edge_name(&c.name) {
            "top" => &mut b.top,
            "left" => &mut b.left,
            "bottom" => &mut b.bottom,
            "right" => &mut b.right,
            "between" => &mut b.between,
            "bar" => &mut b.bar,
            _ => continue,
        };
        *slot = parse_border(c);
    }
    (b != ParagraphBorders::default()).then_some(b)
}

pub(crate) fn parse_table_borders(node: &XmlElement) -> Option<TableBorders> {
    let mut b = TableBorders::default();
    for c in node.elements() {
        let slot = match edge_name(&c.name) {
            "top" => &mut b.top,
            "left" => &mut b.left,
            "bottom" => &mut b.bottom,
            "right" => &mut b.right,
            "insideH" => &mut b.inside_h,
            "insideV" => &mut b.inside_v,
            _ => continue,
        };
        *slot = parse_border(c);
    }
    (b != TableBorders::default()).then_some(b)
}

pub(crate) fn parse_cell_borders(node: &XmlElement) -> Option<CellBorders> {
    let mut b = CellBorders::default();
    for c in node.elements() {
        let slot = match edge_name(&c.name) {
            "top" => &mut b.top,
            "left" => &mut b.left,
            "bottom" => &mut b.bottom,
            "right" => &mut b.right,
            "insideH" => &mut b.inside_h,
            "insideV" => &mut b.inside_v,
            "tl2br" => &mut b.tl2br,
            "tr2bl" => &mut b.tr2bl,
            _ => continue,
        };
        *slot = parse_border(c);
    }
    (b != CellBorders::default()).then_some(b)
}

/// `w:shd`.
pub(crate) fn parse_shading(node: &XmlElement) -> Shading {
    let fill_val = attr(node, "w:fill");
    let fill_theme = attr(node, "w:themeFill");
    let fill = (fill_val.is_some() || fill_theme.is_some()).then(|| DocColor {
        val: color_value(fill_val),
        theme_color: fill_theme.and_then(ThemeColorName::parse),
        theme_tint: hex_byte(attr(node, "w:themeFillTint")),
        theme_shade: hex_byte(attr(node, "w:themeFillShade")),
    });
    Shading {
        pattern: ShadingPattern::parse(val(Some(node)).unwrap_or("clear")),
        color: parse_color(node, "w:color"),
        fill,
    }
}

/// `w:ins`/`w:del`/`w:rPrChange`/`w:pPrChange` metadata.
pub(crate) fn parse_revision(node: &XmlElement) -> RevisionInfo {
    RevisionInfo {
        id: parse_id(attr(node, "w:id")),
        author: attr(node, "w:author").unwrap_or_default().to_owned(),
        date: attr(node, "w:date").map(str::to_owned),
    }
}

// ------------------------------------------------------------------ run properties

fn parse_fonts(node: &XmlElement) -> Option<RunFonts> {
    let name = |a: &str| attr(node, a).filter(|s| !s.is_empty()).map(str::to_owned);
    let theme = |a: &str| attr(node, a).and_then(ThemeFontRef::parse);
    let f = RunFonts {
        ascii: name("w:ascii"),
        h_ansi: name("w:hAnsi"),
        east_asia: name("w:eastAsia"),
        cs: name("w:cs"),
        ascii_theme: theme("w:asciiTheme"),
        h_ansi_theme: theme("w:hAnsiTheme"),
        east_asia_theme: theme("w:eastAsiaTheme"),
        cs_theme: theme("w:cstheme"),
    };
    (f != RunFonts::default()).then_some(f)
}

fn half_points_u32(node: &XmlElement) -> Option<u32> {
    parse_half_points(val(Some(node))).map(|n| u32::try_from(n.max(0)).unwrap_or(0))
}

/// A paragraph-mark revision (`w:ins`/`w:del`/`w:moveFrom`/`w:moveTo` inside a paragraph's
/// `w:rPr`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkRevision {
    pub revision: InsDelRevision,
    pub moved: bool,
}

/// A parsed `w:rPr`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ParsedRunProps {
    pub props: RunProps,
    /// Unmodelled children as raw XML, in file order.
    pub xml: Vec<String>,
    /// `w:rFonts/@w:hint`.
    pub font_hint: Option<String>,
    /// `w:rPrChange`: the revision and the properties before it.
    pub change: Option<(RevisionInfo, RunProps)>,
    /// Paragraph-mark revision.
    pub mark_revision: Option<MarkRevision>,
}

/// Parses `w:rPr` (or a style's / numbering level's `w:rPr`).
pub(crate) fn parse_run_props(node: Option<&XmlElement>, raw: Raw<'_>) -> ParsedRunProps {
    let mut out = ParsedRunProps::default();
    let Some(node) = node else {
        return out;
    };
    let p = &mut out.props;
    for c in node.elements() {
        match c.name.as_str() {
            "w:b" => p.bold = Some(on_off(c)),
            "w:bCs" => p.bold_cs = Some(on_off(c)),
            "w:i" => p.italic = Some(on_off(c)),
            "w:iCs" => p.italic_cs = Some(on_off(c)),
            "w:caps" => p.caps = Some(on_off(c)),
            "w:smallCaps" => p.small_caps = Some(on_off(c)),
            "w:strike" => p.strike = Some(on_off(c)),
            "w:dstrike" => p.dstrike = Some(on_off(c)),
            "w:vanish" => p.hidden = Some(on_off(c)),
            "w:emboss" => p.emboss = Some(on_off(c)),
            "w:imprint" => p.imprint = Some(on_off(c)),
            "w:outline" => p.outline = Some(on_off(c)),
            "w:shadow" => p.shadow = Some(on_off(c)),
            "w:rtl" => p.rtl = Some(on_off(c)),
            "w:rStyle" => {
                if let Some(v) = val(Some(c)).filter(|v| !v.is_empty()) {
                    p.style_id = Some(v.to_owned());
                }
            }
            "w:rFonts" => {
                p.fonts = parse_fonts(c);
                out.font_hint = attr(c, "w:hint").map(str::to_owned);
            }
            "w:color" => p.color = parse_color(c, "w:val"),
            "w:spacing" => p.spacing = parse_twips(val(Some(c))),
            "w:w" => {
                p.scale = val(Some(c))
                    .and_then(|s| s.trim().trim_end_matches('%').parse::<f64>().ok())
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .map(|n| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let v = n.round().min(f64::from(u32::MAX)) as u32;
                        v
                    });
            }
            "w:kern" => p.kern = half_points_u32(c),
            "w:position" => p.position = parse_half_points(val(Some(c))),
            "w:sz" => p.size = half_points_u32(c),
            "w:szCs" => p.size_cs = half_points_u32(c),
            "w:highlight" => match val(Some(c)).and_then(HighlightColor::parse) {
                Some(h) => p.highlight = Some(h),
                None => out.xml.push(raw.xml(c)),
            },
            "w:u" => {
                let style = val(Some(c)).unwrap_or("single");
                match UnderlineStyle::parse(style) {
                    Some(style) => {
                        p.underline = Some(Underline {
                            style,
                            color: parse_color(c, "w:color"),
                        });
                    }
                    None => out.xml.push(raw.xml(c)),
                }
            }
            "w:shd" => p.shading = Some(parse_shading(c)),
            "w:bdr" => p.border = parse_border(c),
            "w:vertAlign" => p.vert_align = val(Some(c)).and_then(VertAlign::parse),
            "w:lang" => {
                let l = Lang {
                    val: attr(c, "w:val").map(str::to_owned),
                    east_asia: attr(c, "w:eastAsia").map(str::to_owned),
                    bidi: attr(c, "w:bidi").map(str::to_owned),
                };
                if l != Lang::default() {
                    p.lang = Some(l);
                }
            }
            "w:rPrChange" => {
                let before = parse_run_props(c.child("w:rPr"), raw).props;
                out.change = Some((parse_revision(c), before));
            }
            "w:ins" | "w:moveTo" | "w:del" | "w:moveFrom" => {
                let kind = if matches!(c.name.as_str(), "w:ins" | "w:moveTo") {
                    InsDel::Insertion
                } else {
                    InsDel::Deletion
                };
                out.mark_revision = Some(MarkRevision {
                    revision: InsDelRevision {
                        info: parse_revision(c),
                        kind,
                    },
                    moved: matches!(c.name.as_str(), "w:moveTo" | "w:moveFrom"),
                });
            }
            // w:noProof, w:snapToGrid, w:webHidden, w:effect, w:fitText, w:cs, w:em,
            // w:eastAsianLayout, w:specVanish, w:oMath, w14:* text effects, …
            _ => out.xml.push(raw.xml(c)),
        }
    }
    out
}

// ------------------------------------------------------------------ paragraph properties

fn parse_alignment(v: &str) -> Option<Alignment> {
    Some(match v {
        "left" | "start" => Alignment::Left,
        "center" => Alignment::Center,
        "right" | "end" => Alignment::Right,
        "both" | "justify" => Alignment::Both,
        other => return Alignment::parse(other),
    })
}

fn parse_indent(node: &XmlElement) -> Option<Indent> {
    let left = parse_twips(attr(node, "w:left").or_else(|| attr(node, "w:start")));
    let right = parse_twips(attr(node, "w:right").or_else(|| attr(node, "w:end")));
    let first = parse_twips(attr(node, "w:firstLine"));
    let hanging = parse_twips(attr(node, "w:hanging"));
    // Hanging wins over firstLine when both are present (ECMA-376 §17.3.1.12).
    let ind = Indent {
        left,
        right,
        hanging,
        first_line: if hanging.is_some() { None } else { first },
    };
    (ind != Indent::default()).then_some(ind)
}

fn parse_spacing(node: &XmlElement) -> Option<Spacing> {
    let line = parse_twips(attr(node, "w:line"));
    let sp = Spacing {
        before: parse_twips(attr(node, "w:before")),
        after: parse_twips(attr(node, "w:after")),
        line,
        line_rule: match attr(node, "w:lineRule").and_then(LineRule::parse) {
            Some(r) => Some(r),
            None => line.map(|_| LineRule::Auto),
        },
        before_autospacing: attr(node, "w:beforeAutospacing").map(|v| parse_on_off(Some(v), true)),
        after_autospacing: attr(node, "w:afterAutospacing").map(|v| parse_on_off(Some(v), true)),
    };
    (sp != Spacing::default()).then_some(sp)
}

fn parse_tabs(node: &XmlElement) -> Vec<TabStop> {
    node.children_named("w:tab")
        .filter_map(|t| {
            let pos = parse_twips(attr(t, "w:pos"))?;
            let align = match val(Some(t)).unwrap_or("left") {
                "start" => TabAlignment::Left,
                "end" => TabAlignment::Right,
                other => TabAlignment::parse(other).unwrap_or_default(),
            };
            Some(TabStop {
                pos,
                align,
                leader: attr(t, "w:leader").and_then(TabLeader::parse),
            })
        })
        .collect()
}

/// A parsed `w:pPr`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedParagraphProps<'a> {
    pub props: ParagraphProps,
    /// Unmodelled children as raw XML.
    pub xml: Vec<String>,
    /// The paragraph mark's run properties (`w:pPr/w:rPr`).
    pub mark: ParsedRunProps,
    /// A `w:sectPr` ending a section at this paragraph.
    pub sect_pr: Option<&'a XmlElement>,
    /// `w:pPrChange`: the revision and the properties before it.
    pub change: Option<(RevisionInfo, ParagraphProps)>,
}

/// Parses `w:pPr`.
pub(crate) fn parse_paragraph_props<'a>(
    node: Option<&'a XmlElement>,
    raw: Raw<'_>,
) -> ParsedParagraphProps<'a> {
    let mut out = ParsedParagraphProps::default();
    let Some(node) = node else {
        return out;
    };
    let p = &mut out.props;
    for c in node.elements() {
        match c.name.as_str() {
            "w:pStyle" => {
                if let Some(v) = val(Some(c)).filter(|v| !v.is_empty()) {
                    p.style_id = Some(v.to_owned());
                }
            }
            "w:jc" => match val(Some(c)).and_then(parse_alignment) {
                Some(a) => p.alignment = Some(a),
                None => out.xml.push(raw.xml(c)),
            },
            "w:ind" => p.indent = parse_indent(c),
            "w:spacing" => p.spacing = parse_spacing(c),
            "w:numPr" => {
                let num_id = parse_u32(val(c.child("w:numId")));
                let level =
                    parse_decimal(val(c.child("w:ilvl"))).map(kiln_doc::numbering::clamp_level);
                if num_id.is_some() || level.is_some() {
                    p.numbering = Some(NumberingRef { num_id, level });
                }
            }
            "w:keepNext" => p.keep_next = Some(on_off(c)),
            "w:keepLines" => p.keep_lines = Some(on_off(c)),
            "w:pageBreakBefore" => p.page_break_before = Some(on_off(c)),
            "w:widowControl" => p.widow_control = Some(on_off(c)),
            "w:contextualSpacing" => p.contextual_spacing = Some(on_off(c)),
            "w:suppressAutoHyphens" => p.suppress_auto_hyphens = Some(on_off(c)),
            "w:suppressLineNumbers" => p.suppress_line_numbers = Some(on_off(c)),
            "w:mirrorIndents" => p.mirror_indents = Some(on_off(c)),
            "w:snapToGrid" => p.snap_to_grid = Some(on_off(c)),
            "w:bidi" => p.bidi = Some(on_off(c)),
            "w:outlineLvl" => {
                p.outline_level =
                    parse_decimal(val(Some(c))).and_then(|n| u8::try_from(n.clamp(0, 9)).ok());
            }
            "w:tabs" => p.tabs = Some(parse_tabs(c)),
            "w:pBdr" => p.borders = parse_paragraph_borders(c),
            "w:shd" => p.shading = Some(parse_shading(c)),
            "w:textAlignment" => {
                p.text_alignment = val(Some(c)).and_then(TextAlignment::parse);
            }
            "w:framePr" => {
                let kind = attr(c, "w:dropCap").and_then(DropCapKind::parse);
                if let Some(kind) = kind {
                    let lines = parse_decimal(attr(c, "w:lines")).unwrap_or(3);
                    p.drop_cap = Some(DropCap {
                        kind,
                        lines: u8::try_from(lines.clamp(1, 10)).unwrap_or(3),
                    });
                }
                // The frame (position, wrap, size) is kept verbatim.
                out.xml.push(raw.xml(c));
            }
            "w:rPr" => out.mark = parse_run_props(Some(c), raw),
            "w:sectPr" => out.sect_pr = Some(c),
            "w:pPrChange" => {
                let before = parse_paragraph_props(c.child("w:pPr"), raw).props;
                out.change = Some((parse_revision(c), before));
            }
            // w:adjustRightInd, w:autoSpaceDE/DN, w:kinsoku, w:wordWrap, w:overflowPunct,
            // w:topLinePunct, w:cnfStyle, w:divId, w:textDirection, w:textboxTightWrap, …
            _ => out.xml.push(raw.xml(c)),
        }
    }
    out
}

// ------------------------------------------------------------------ tables

fn parse_table_alignment(v: Option<&str>) -> Option<TableAlignment> {
    match v? {
        "center" => Some(TableAlignment::Center),
        "right" | "end" => Some(TableAlignment::Right),
        "left" | "start" | "both" => Some(TableAlignment::Left),
        _ => None,
    }
}

fn parse_cell_margins(node: &XmlElement) -> Option<CellMargins> {
    let mut m = CellMargins::default();
    for c in node.elements() {
        let slot = match edge_name(&c.name) {
            "top" => &mut m.top,
            "left" => &mut m.left,
            "bottom" => &mut m.bottom,
            "right" => &mut m.right,
            _ => continue,
        };
        // Cell margins are twips; a percentage margin has no meaning in Word and is ignored.
        if let Some(w) = parse_table_width(attr(c, "w:w"), attr(c, "w:type"))
            && w.kind != WidthType::Pct
        {
            *slot = Some(w.value);
        }
    }
    (m != CellMargins::default()).then_some(m)
}

/// `w:tblLook`: the Word 2010 attributes or the Word 2007 hex bitmask.
pub(crate) fn parse_table_look(node: &XmlElement) -> TableLook {
    if attr(node, "w:firstRow").is_none()
        && let Some(hex) = val(Some(node))
        && let Ok(n) = u32::from_str_radix(hex.trim(), 16)
    {
        return TableLook {
            first_row: n & 0x20 != 0,
            last_row: n & 0x40 != 0,
            first_column: n & 0x80 != 0,
            last_column: n & 0x100 != 0,
            no_h_band: n & 0x200 != 0,
            no_v_band: n & 0x400 != 0,
        };
    }
    let b = |name: &str| parse_on_off(attr(node, name), false);
    TableLook {
        first_row: b("w:firstRow"),
        last_row: b("w:lastRow"),
        first_column: b("w:firstColumn"),
        last_column: b("w:lastColumn"),
        no_h_band: b("w:noHBand"),
        no_v_band: b("w:noVBand"),
    }
}

fn parse_table_position(node: &XmlElement) -> TablePosition {
    let s = |a: &str| attr(node, a).map(str::to_owned);
    TablePosition {
        left_from_text: parse_twips(attr(node, "w:leftFromText")),
        right_from_text: parse_twips(attr(node, "w:rightFromText")),
        top_from_text: parse_twips(attr(node, "w:topFromText")),
        bottom_from_text: parse_twips(attr(node, "w:bottomFromText")),
        horz_anchor: s("w:horzAnchor"),
        vert_anchor: s("w:vertAnchor"),
        x: parse_twips(attr(node, "w:tblpX")),
        x_spec: s("w:tblpXSpec"),
        y: parse_twips(attr(node, "w:tblpY")),
        y_spec: s("w:tblpYSpec"),
    }
}

/// A parsed `w:tblPr` and its unmodelled children.
pub(crate) fn parse_table_props(
    node: Option<&XmlElement>,
    raw: Raw<'_>,
) -> (TableProps, Vec<String>) {
    let mut p = TableProps::default();
    let mut xml = Vec::new();
    let Some(node) = node else {
        return (p, xml);
    };
    for c in node.elements() {
        match c.name.as_str() {
            "w:tblStyle" => {
                if let Some(v) = val(Some(c)).filter(|v| !v.is_empty()) {
                    p.style_id = Some(v.to_owned());
                }
            }
            "w:tblW" => p.width = parse_table_width(attr(c, "w:w"), attr(c, "w:type")),
            "w:jc" => match parse_table_alignment(val(Some(c))) {
                Some(a) => p.alignment = Some(a),
                None => xml.push(raw.xml(c)),
            },
            "w:tblInd" => match parse_table_width(attr(c, "w:w"), attr(c, "w:type")) {
                Some(TableWidth {
                    kind: WidthType::Dxa,
                    value,
                }) => p.indent = Some(value),
                _ => xml.push(raw.xml(c)),
            },
            "w:tblBorders" => p.borders = parse_table_borders(c),
            "w:shd" => p.shading = Some(parse_shading(c)),
            "w:tblCellMar" => p.cell_margins = parse_cell_margins(c),
            "w:tblCellSpacing" => match parse_table_width(attr(c, "w:w"), attr(c, "w:type")) {
                Some(TableWidth {
                    kind: WidthType::Dxa,
                    value,
                }) => p.cell_spacing = Some(value),
                _ => xml.push(raw.xml(c)),
            },
            "w:tblLayout" => {
                p.layout = Some(if attr(c, "w:type") == Some("fixed") {
                    TableLayout::Fixed
                } else {
                    TableLayout::Autofit
                });
            }
            "w:tblLook" => p.look = Some(parse_table_look(c)),
            "w:tblStyleRowBandSize" => p.row_band_size = parse_u32(val(Some(c))),
            "w:tblStyleColBandSize" => p.col_band_size = parse_u32(val(Some(c))),
            "w:bidiVisual" => p.bidi_visual = Some(on_off(c)),
            "w:tblpPr" => p.position = Some(parse_table_position(c)),
            "w:tblOverlap" => p.allow_overlap = Some(val(Some(c)) != Some("never")),
            "w:tblCaption" => p.caption = val(Some(c)).map(str::to_owned),
            "w:tblDescription" => p.description = val(Some(c)).map(str::to_owned),
            // w:tblPrChange, …
            _ => xml.push(raw.xml(c)),
        }
    }
    (p, xml)
}

/// A parsed `w:trPr`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedRowProps {
    pub props: TableRowProps,
    pub revision: Option<InsDelRevision>,
    pub xml: Vec<String>,
}

pub(crate) fn parse_row_props(node: Option<&XmlElement>, raw: Raw<'_>) -> ParsedRowProps {
    let mut out = ParsedRowProps::default();
    let Some(node) = node else {
        return out;
    };
    let p = &mut out.props;
    for c in node.elements() {
        match c.name.as_str() {
            "w:trHeight" => {
                if let Some(value) = parse_twips(val(Some(c))) {
                    let rule = match attr(c, "w:hRule") {
                        Some("exact") => HeightRule::Exact,
                        Some("auto") => HeightRule::Auto,
                        _ => HeightRule::AtLeast,
                    };
                    p.height = Some(RowHeight { value, rule });
                }
            }
            "w:tblHeader" => p.header = Some(on_off(c)),
            "w:cantSplit" => p.cant_split = Some(on_off(c)),
            "w:hidden" => p.hidden = Some(on_off(c)),
            "w:jc" => match parse_table_alignment(val(Some(c))) {
                Some(a) => p.alignment = Some(a),
                None => out.xml.push(raw.xml(c)),
            },
            "w:gridBefore" => p.grid_before = parse_grid_count(c),
            "w:gridAfter" => p.grid_after = parse_grid_count(c),
            "w:ins" | "w:del" => {
                out.revision = Some(InsDelRevision {
                    info: parse_revision(c),
                    kind: if c.name == "w:ins" {
                        InsDel::Insertion
                    } else {
                        InsDel::Deletion
                    },
                });
            }
            // w:wBefore, w:wAfter, w:tblCellSpacing, w:cnfStyle, w:divId, w:trPrChange, …
            _ => out.xml.push(raw.xml(c)),
        }
    }
    out
}

/// Word's table grid limit: a table has at most 63 columns, so no `w:gridSpan`,
/// `w:gridBefore` or `w:gridAfter` can cover more. Clamping here keeps hostile values (up to
/// `u32::MAX`) out of the column arithmetic and out of the layout engine.
pub(crate) const MAX_GRID_COLUMNS: u32 = 63;

/// `w:gridBefore` / `w:gridAfter`: a count of skipped grid columns, clamped to the grid limit.
fn parse_grid_count(c: &XmlElement) -> Option<u32> {
    parse_u32(val(Some(c))).map(|n| n.min(MAX_GRID_COLUMNS))
}

/// A parsed `w:tcPr`.
#[derive(Debug, Clone)]
pub(crate) struct ParsedCellProps {
    pub props: TableCellProps,
    pub grid_span: u32,
    pub v_merge: Option<VMerge>,
    /// Legacy horizontal merge (`w:hMerge`).
    pub h_merge: Option<VMerge>,
    /// A cell-level revision mark (`w:cellIns`, `w:cellDel`, `w:cellMerge`).
    pub revision: bool,
    pub xml: Vec<String>,
}

pub(crate) fn parse_cell_props(node: Option<&XmlElement>, raw: Raw<'_>) -> ParsedCellProps {
    let mut out = ParsedCellProps {
        props: TableCellProps::default(),
        grid_span: 1,
        v_merge: None,
        h_merge: None,
        revision: false,
        xml: Vec::new(),
    };
    let Some(node) = node else {
        return out;
    };
    let merge = |c: &XmlElement| {
        if val(Some(c)) == Some("restart") {
            VMerge::Restart
        } else {
            VMerge::Continue
        }
    };
    for c in node.elements() {
        let p = &mut out.props;
        match c.name.as_str() {
            "w:tcW" => p.width = parse_table_width(attr(c, "w:w"), attr(c, "w:type")),
            "w:gridSpan" => {
                out.grid_span = parse_u32(val(Some(c)))
                    .unwrap_or(1)
                    .clamp(1, MAX_GRID_COLUMNS);
            }
            "w:vMerge" => out.v_merge = Some(merge(c)),
            "w:hMerge" => out.h_merge = Some(merge(c)),
            "w:tcBorders" => p.borders = parse_cell_borders(c),
            "w:shd" => p.shading = Some(parse_shading(c)),
            "w:tcMar" => p.margins = parse_cell_margins(c),
            "w:vAlign" => {
                p.v_align = match val(Some(c)) {
                    Some("top") => Some(CellVAlign::Top),
                    Some("bottom") => Some(CellVAlign::Bottom),
                    Some("center" | "both") => Some(CellVAlign::Center),
                    _ => None,
                };
            }
            "w:noWrap" => p.no_wrap = Some(on_off(c)),
            "w:textDirection" => {
                let dir = match val(Some(c)) {
                    // Strict / Word 2010 spellings.
                    Some("lr") => Some(TextDirection::LrTb),
                    Some("tb" | "rl") => Some(TextDirection::TbRl),
                    Some(v) => TextDirection::parse(v),
                    None => None,
                };
                match dir {
                    Some(d) => p.text_direction = Some(d),
                    None => out.xml.push(raw.xml(c)),
                }
            }
            "w:tcFitText" => p.fit_text = Some(on_off(c)),
            "w:hideMark" => p.hide_mark = Some(on_off(c)),
            "w:cellIns" | "w:cellDel" | "w:cellMerge" => {
                out.revision = true;
                out.xml.push(raw.xml(c));
            }
            // w:cnfStyle, w:headers, w:tcPrChange, …
            _ => out.xml.push(raw.xml(c)),
        }
    }
    out
}

/// The `w:val` of a child, for one-liners.
pub(crate) fn child_val<'a>(node: Option<&'a XmlElement>, name: &str) -> Option<&'a str> {
    attr_of(node.and_then(|n| n.child(name)), "w:val")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::xml::parse_fragment;

    fn el(xml: &str) -> XmlElement {
        parse_fragment(xml).expect("fragment")
    }

    fn with_raw<T>(f: impl FnOnce(Raw<'_>) -> T) -> T {
        let scope = Attrs::new();
        f(Raw { scope: &scope })
    }

    #[test]
    fn colours_read_values_themes_tints_and_argb() {
        let c = parse_color(
            &el(r#"<w:color w:val="1F3864" w:themeColor="accent1" w:themeShade="80"/>"#),
            "w:val",
        )
        .expect("colour");
        assert_eq!(
            c,
            DocColor::theme(0x1F3864, ThemeColorName::Accent1).with_shade(0x80)
        );
        let argb = parse_color(&el(r#"<w:color w:val="FF00FF00"/>"#), "w:val").expect("argb");
        assert_eq!(argb.val, ColorValue::hex(0x00FF00));
        let bad = parse_color(&el(r#"<w:color w:val="nope"/>"#), "w:val").expect("auto");
        assert_eq!(bad.val, ColorValue::Auto);
        assert!(parse_color(&el("<w:color/>"), "w:val").is_none());
    }

    #[test]
    fn run_toggles_sizes_and_raw_extras() {
        let node = el(
            r#"<w:rPr><w:b/><w:i w:val="0"/><w:sz w:val="12pt"/><w:spacing w:val="1pt"/><w:position w:val="2pt"/><w:noProof/><w:em w:val="dot"/><w:highlight w:val="bogus"/><w:u w:val="dash" w:color="FF0000"/><w:w w:val="150"/></w:rPr>"#,
        );
        let r = with_raw(|raw| parse_run_props(Some(&node), raw));
        assert_eq!(r.props.bold, Some(true));
        assert_eq!(r.props.italic, Some(false));
        assert_eq!(r.props.size, Some(24));
        assert_eq!(r.props.spacing, Some(20));
        assert_eq!(r.props.position, Some(4));
        assert_eq!(r.props.scale, Some(150));
        assert_eq!(
            r.props.underline,
            Some(Underline {
                style: UnderlineStyle::Dash,
                color: Some(DocColor::rgb(0xFF0000))
            })
        );
        assert_eq!(
            r.xml,
            [
                "<w:noProof/>",
                r#"<w:em w:val="dot"/>"#,
                r#"<w:highlight w:val="bogus"/>"#
            ]
        );
    }

    #[test]
    fn paragraph_indents_prefer_hanging_and_normalise_start_end() {
        let node = el(
            r#"<w:pPr><w:ind w:start="720" w:end="360" w:firstLine="200" w:hanging="400"/></w:pPr>"#,
        );
        let p = with_raw(|raw| parse_paragraph_props(Some(&node), raw).props);
        assert_eq!(
            p.indent,
            Some(Indent {
                left: Some(720),
                right: Some(360),
                first_line: None,
                hanging: Some(400)
            })
        );
    }

    #[test]
    fn table_look_from_hex_and_attributes() {
        let hex = parse_table_look(&el(r#"<w:tblLook w:val="04A0"/>"#));
        assert!(hex.first_row && hex.first_column && hex.no_v_band && !hex.no_h_band);
        let attrs = parse_table_look(&el(
            r#"<w:tblLook w:val="04A0" w:firstRow="0" w:lastRow="1"/>"#,
        ));
        assert!(!attrs.first_row && attrs.last_row && !attrs.no_v_band);
    }

    #[test]
    fn borders_map_start_and_end() {
        let b = parse_table_borders(&el(
            r#"<w:tblBorders><w:start w:val="single" w:sz="4"/><w:end w:val="double"/><w:insideH w:val="nil"/></w:tblBorders>"#,
        ))
        .expect("borders");
        assert_eq!(b.left.map(|x| x.size), Some(Some(4)));
        assert_eq!(b.right.map(|x| x.style), Some(BorderStyle::Double));
        assert_eq!(b.inside_h.map(|x| x.style), Some(BorderStyle::Nil));
        assert!(parse_cell_borders(&el("<w:tcBorders/>")).is_none());
    }
}
