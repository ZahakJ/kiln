// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property writers: `w:rPr`, `w:pPr`, `w:tblPr`, `w:trPr`, `w:tcPr`, `w:sectPr` and their building
//! blocks — the inverse of `read::props` and `read::sections`. Children come out in the order of
//! the ECMA-376 schema sequence (Word rejects files whose property children are out of order);
//! raw XML the reader kept for unmodelled children is slotted into its schema position by
//! element name.
//!
//! The `*_modelled` classifiers say which children of a raw property element the reader maps into
//! the model. They let the writer patch raw elements kept whole (styles, numbering levels): the
//! modelled children are regenerated from the model, everything else is kept verbatim.

use kiln_doc::props::{
    Border, CellBorders, CellMargins, DocColor, DropCap, Indent, ParagraphBorders, ParagraphProps,
    RunProps, Shading, Spacing, TabStop, TableBorders, TableCellProps, TableLook, TablePosition,
    TableProps, TableRowProps,
};
use kiln_doc::revision::{InsDel, InsDelRevision};
use kiln_doc::section::SectionProps;
use kiln_doc::settings::NoteSettings;
use kiln_doc::st::{
    Alignment, DropCapKind, HeaderFooterKind, HeightRule, HighlightColor, LineNumberRestart,
    Orientation, SectionBreak, TextDirection, UnderlineStyle, VMerge, WidthType,
};
use kiln_doc::{RevisionInfo, TableWidth};
use kiln_ooxml::xml::XmlElement;

use super::xml::{El, fragment_name, hex_byte, order_by_sequence, toggle, val};
use crate::common::extras::{PageBorders, SectionExtras};
use crate::common::units::{parse_decimal, parse_table_width};
use crate::common::xml::{attr, fragment_xml, parse_fragment};

type Entries = Vec<(String, String)>;

fn push(e: &mut Entries, name: &str, xml: String) {
    if !xml.is_empty() {
        e.push((name.to_owned(), xml));
    }
}

fn push_raw(e: &mut Entries, raw: &[String]) {
    for x in raw {
        e.push((fragment_name(x).to_owned(), x.clone()));
    }
}

fn has_raw(raw: &[String], name: &str) -> bool {
    raw.iter().any(|x| fragment_name(x) == name)
}

// ------------------------------------------------------------------ building blocks

/// A colour as `value_attr` plus `{prefix}Color`/`Tint`/`Shade`.
fn color_attrs(el: El, c: &DocColor, value_attr: &str, prefix: &str) -> El {
    el.attr(value_attr, c.val)
        .opt(&format!("{prefix}Color"), c.theme_color.map(|t| t.as_str()))
        .opt(&format!("{prefix}Tint"), c.theme_tint.map(hex_byte))
        .opt(&format!("{prefix}Shade"), c.theme_shade.map(hex_byte))
}

/// One border edge.
pub(crate) fn border_xml(name: &str, b: &Border) -> String {
    let mut el = El::new(name)
        .attr("w:val", b.style.as_str())
        .opt("w:sz", b.size)
        .opt("w:space", b.space);
    if let Some(c) = &b.color {
        el = color_attrs(el, c, "w:color", "w:theme");
    }
    el.flag("w:shadow", b.shadow)
        .flag("w:frame", b.frame)
        .empty()
}

fn edges(name: &str, list: &[(&str, &Option<Border>)]) -> String {
    let inner: String = list
        .iter()
        .filter_map(|(n, b)| b.as_ref().map(|b| border_xml(&format!("w:{n}"), b)))
        .collect();
    if inner.is_empty() {
        String::new()
    } else {
        El::new(name).wrap(&inner)
    }
}

pub(crate) fn paragraph_borders_xml(b: &ParagraphBorders) -> String {
    edges(
        "w:pBdr",
        &[
            ("top", &b.top),
            ("left", &b.left),
            ("bottom", &b.bottom),
            ("right", &b.right),
            ("between", &b.between),
            ("bar", &b.bar),
        ],
    )
}

pub(crate) fn table_borders_xml(b: &TableBorders) -> String {
    edges(
        "w:tblBorders",
        &[
            ("top", &b.top),
            ("left", &b.left),
            ("bottom", &b.bottom),
            ("right", &b.right),
            ("insideH", &b.inside_h),
            ("insideV", &b.inside_v),
        ],
    )
}

pub(crate) fn cell_borders_xml(b: &CellBorders) -> String {
    edges(
        "w:tcBorders",
        &[
            ("top", &b.top),
            ("left", &b.left),
            ("bottom", &b.bottom),
            ("right", &b.right),
            ("insideH", &b.inside_h),
            ("insideV", &b.inside_v),
            ("tl2br", &b.tl2br),
            ("tr2bl", &b.tr2bl),
        ],
    )
}

/// `w:shd`.
pub(crate) fn shading_xml(s: &Shading) -> String {
    let mut el = El::new("w:shd").attr("w:val", s.pattern.as_str());
    if let Some(c) = &s.color {
        el = color_attrs(el, c, "w:color", "w:theme");
    }
    if let Some(f) = &s.fill {
        el = el
            .attr("w:fill", f.val)
            .opt("w:themeFill", f.theme_color.map(|t| t.as_str()))
            .opt("w:themeFillTint", f.theme_tint.map(hex_byte))
            .opt("w:themeFillShade", f.theme_shade.map(hex_byte));
    }
    el.empty()
}

/// `w:id`/`w:author`/`w:date` of a revision element.
pub(crate) fn revision_el(name: &str, r: &RevisionInfo) -> El {
    El::new(name)
        .attr("w:id", r.id)
        .attr("w:author", &r.author)
        .opt("w:date", r.date.as_deref())
}

fn width_xml(name: &str, w: &TableWidth) -> String {
    El::new(name)
        .attr("w:w", w.value)
        .attr("w:type", w.kind.as_str())
        .empty()
}

fn margins_xml(name: &str, m: &CellMargins) -> String {
    let inner: String = [
        ("w:top", m.top),
        ("w:left", m.left),
        ("w:bottom", m.bottom),
        ("w:right", m.right),
    ]
    .iter()
    .filter_map(|(n, v)| v.map(|v| width_xml(n, &TableWidth::dxa(v))))
    .collect();
    if inner.is_empty() {
        String::new()
    } else {
        El::new(name).wrap(&inner)
    }
}

// ------------------------------------------------------------------ run properties

/// EG_RPrBase (ECMA-376 Part 1 §17.3.2.28) after the paragraph-mark revisions, then the Word 2010+
/// extensions (`w14:*`) and `w:rPrChange`.
pub(crate) const RPR_SEQUENCE: &[&str] = &[
    "w:ins",
    "w:del",
    "w:moveFrom",
    "w:moveTo",
    "w:rStyle",
    "w:rFonts",
    "w:b",
    "w:bCs",
    "w:i",
    "w:iCs",
    "w:caps",
    "w:smallCaps",
    "w:strike",
    "w:dstrike",
    "w:outline",
    "w:shadow",
    "w:emboss",
    "w:imprint",
    "w:noProof",
    "w:snapToGrid",
    "w:vanish",
    "w:webHidden",
    "w:color",
    "w:spacing",
    "w:w",
    "w:kern",
    "w:position",
    "w:sz",
    "w:szCs",
    "w:highlight",
    "w:u",
    "w:effect",
    "w:bdr",
    "w:shd",
    "w:fitText",
    "w:vertAlign",
    "w:rtl",
    "w:cs",
    "w:em",
    "w:lang",
    "w:eastAsianLayout",
    "w:specVanish",
    "w:oMath",
    "#ext",
    "w:rPrChange",
];

/// What a run's `w:rPr` holds besides its [`RunProps`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RunPropsExtra<'a> {
    /// Unmodelled children, raw.
    pub raw: &'a [String],
    /// `w:rFonts/@w:hint`.
    pub font_hint: Option<&'a str>,
    /// `w:rPrChange`: the revision and the properties before it.
    pub change: Option<(&'a RevisionInfo, &'a RunProps)>,
    /// A paragraph-mark revision (`w:pPr/w:rPr/w:ins|w:del`).
    pub mark_revision: Option<&'a InsDelRevision>,
}

fn toggles(p: &RunProps) -> [(&'static str, Option<bool>); 14] {
    [
        ("w:b", p.bold),
        ("w:bCs", p.bold_cs),
        ("w:i", p.italic),
        ("w:iCs", p.italic_cs),
        ("w:caps", p.caps),
        ("w:smallCaps", p.small_caps),
        ("w:strike", p.strike),
        ("w:dstrike", p.dstrike),
        ("w:outline", p.outline),
        ("w:shadow", p.shadow),
        ("w:emboss", p.emboss),
        ("w:imprint", p.imprint),
        ("w:vanish", p.hidden),
        ("w:rtl", p.rtl),
    ]
}

/// The children of `w:rPr` (without the wrapper).
pub(crate) fn run_props_inner(p: &RunProps, x: RunPropsExtra<'_>) -> String {
    let mut e: Entries = Vec::new();
    if let Some(r) = x.mark_revision {
        let n = match r.kind {
            InsDel::Insertion => "w:ins",
            InsDel::Deletion => "w:del",
        };
        push(&mut e, n, revision_el(n, &r.info).empty());
    }
    if let Some(s) = &p.style_id {
        push(&mut e, "w:rStyle", val("w:rStyle", s));
    }
    if p.fonts.is_some() || x.font_hint.is_some() {
        let f = p.fonts.clone().unwrap_or_default();
        let el = El::new("w:rFonts")
            .opt("w:hint", x.font_hint)
            .opt("w:ascii", f.ascii.as_deref())
            .opt("w:hAnsi", f.h_ansi.as_deref())
            .opt("w:eastAsia", f.east_asia.as_deref())
            .opt("w:cs", f.cs.as_deref())
            .opt("w:asciiTheme", f.ascii_theme.map(|t| t.as_str()))
            .opt("w:hAnsiTheme", f.h_ansi_theme.map(|t| t.as_str()))
            .opt("w:eastAsiaTheme", f.east_asia_theme.map(|t| t.as_str()))
            .opt("w:cstheme", f.cs_theme.map(|t| t.as_str()));
        push(&mut e, "w:rFonts", el.empty());
    }
    for (name, v) in toggles(p) {
        if let Some(on) = v {
            push(&mut e, name, toggle(name, on));
        }
    }
    if let Some(c) = &p.color {
        push(
            &mut e,
            "w:color",
            color_attrs(El::new("w:color"), c, "w:val", "w:theme").empty(),
        );
    }
    if let Some(v) = p.spacing {
        push(&mut e, "w:spacing", val("w:spacing", v));
    }
    if let Some(v) = p.scale {
        push(&mut e, "w:w", val("w:w", v));
    }
    if let Some(v) = p.kern {
        push(&mut e, "w:kern", val("w:kern", v));
    }
    if let Some(v) = p.position {
        push(&mut e, "w:position", val("w:position", v));
    }
    if let Some(v) = p.size {
        push(&mut e, "w:sz", val("w:sz", v));
    }
    if let Some(v) = p.size_cs {
        push(&mut e, "w:szCs", val("w:szCs", v));
    }
    if let Some(h) = p.highlight {
        push(&mut e, "w:highlight", val("w:highlight", h.as_str()));
    }
    if let Some(u) = &p.underline {
        let mut el = El::new("w:u").attr("w:val", u.style.as_str());
        if let Some(c) = &u.color {
            el = color_attrs(el, c, "w:color", "w:theme");
        }
        push(&mut e, "w:u", el.empty());
    }
    if let Some(b) = &p.border {
        push(&mut e, "w:bdr", border_xml("w:bdr", b));
    }
    if let Some(s) = &p.shading {
        push(&mut e, "w:shd", shading_xml(s));
    }
    if let Some(v) = p.vert_align {
        push(&mut e, "w:vertAlign", val("w:vertAlign", v.as_str()));
    }
    if let Some(l) = &p.lang {
        let el = El::new("w:lang")
            .opt("w:val", l.val.as_deref())
            .opt("w:eastAsia", l.east_asia.as_deref())
            .opt("w:bidi", l.bidi.as_deref());
        push(&mut e, "w:lang", el.empty());
    }
    for xml in x.raw {
        let name = fragment_name(xml);
        let key = if name.starts_with("w:") { name } else { "#ext" };
        e.push((key.to_owned(), xml.clone()));
    }
    if let Some((info, before)) = x.change {
        let inner = run_props_inner(before, RunPropsExtra::default());
        push(
            &mut e,
            "w:rPrChange",
            revision_el("w:rPrChange", info).wrap(&El::new("w:rPr").wrap(&inner)),
        );
    }
    order_by_sequence(e, RPR_SEQUENCE)
}

/// `w:rPr` with its content, or `""` when empty.
pub(crate) fn run_props_xml(p: &RunProps, x: RunPropsExtra<'_>) -> String {
    let inner = run_props_inner(p, x);
    if inner.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{inner}</w:rPr>")
    }
}

/// Whether the reader maps this `w:rPr` child into [`RunProps`] (in a style or numbering level,
/// where revisions are not modelled, pass `in_style`).
pub(crate) fn rpr_child_modelled(c: &XmlElement, in_style: bool) -> bool {
    let v = attr(c, "w:val");
    match c.name.as_str() {
        "w:b" | "w:bCs" | "w:i" | "w:iCs" | "w:caps" | "w:smallCaps" | "w:strike" | "w:dstrike"
        | "w:vanish" | "w:emboss" | "w:imprint" | "w:outline" | "w:shadow" | "w:rtl"
        | "w:rStyle" | "w:rFonts" | "w:color" | "w:spacing" | "w:w" | "w:kern" | "w:position"
        | "w:sz" | "w:szCs" | "w:shd" | "w:bdr" | "w:vertAlign" | "w:lang" => true,
        "w:highlight" => v.and_then(HighlightColor::parse).is_some(),
        "w:u" => UnderlineStyle::parse(v.unwrap_or("single")).is_some(),
        "w:rPrChange" | "w:ins" | "w:moveTo" | "w:del" | "w:moveFrom" => !in_style,
        _ => false,
    }
}

// ------------------------------------------------------------------ paragraph properties

/// CT_PPr (ECMA-376 Part 1 §17.3.1.26).
pub(crate) const PPR_SEQUENCE: &[&str] = &[
    "w:pStyle",
    "w:keepNext",
    "w:keepLines",
    "w:pageBreakBefore",
    "w:framePr",
    "w:widowControl",
    "w:numPr",
    "w:suppressLineNumbers",
    "w:pBdr",
    "w:shd",
    "w:tabs",
    "w:suppressAutoHyphens",
    "w:kinsoku",
    "w:wordWrap",
    "w:overflowPunct",
    "w:topLinePunct",
    "w:autoSpaceDE",
    "w:autoSpaceDN",
    "w:bidi",
    "w:adjustRightInd",
    "w:snapToGrid",
    "w:spacing",
    "w:ind",
    "w:contextualSpacing",
    "w:mirrorIndents",
    "w:suppressOverlap",
    "w:jc",
    "w:textDirection",
    "w:textAlignment",
    "w:textboxTightWrap",
    "w:outlineLvl",
    "w:divId",
    "w:cnfStyle",
    "#unknown",
    "w:rPr",
    "w:sectPr",
    "w:pPrChange",
];

fn indent_xml(ind: &Indent) -> String {
    El::new("w:ind")
        .opt("w:left", ind.left)
        .opt("w:right", ind.right)
        .opt("w:hanging", ind.hanging)
        .opt(
            "w:firstLine",
            if ind.hanging.is_some() {
                None
            } else {
                ind.first_line
            },
        )
        .empty()
}

fn spacing_xml(sp: &Spacing) -> String {
    El::new("w:spacing")
        .opt("w:before", sp.before)
        .flag("w:beforeAutospacing", sp.before_autospacing)
        .opt("w:after", sp.after)
        .flag("w:afterAutospacing", sp.after_autospacing)
        .opt("w:line", sp.line)
        .opt("w:lineRule", sp.line_rule.map(|r| r.as_str()))
        .empty()
}

fn tabs_xml(tabs: &[TabStop]) -> String {
    let inner: String = tabs
        .iter()
        .map(|t| {
            El::new("w:tab")
                .attr("w:val", t.align.as_str())
                .opt("w:leader", t.leader.map(|l| l.as_str()))
                .attr("w:pos", t.pos)
                .empty()
        })
        .collect();
    El::new("w:tabs").wrap(&inner)
}

/// A drop cap without a kept frame: Word's own drop-cap frame.
fn drop_cap_frame(p: &ParagraphProps) -> Option<String> {
    let d = p.drop_cap?;
    Some(
        El::new("w:framePr")
            .attr("w:dropCap", d.kind.as_str())
            .attr("w:lines", d.lines)
            .attr("w:wrap", "around")
            .attr("w:vAnchor", "text")
            .attr("w:hAnchor", "text")
            .empty(),
    )
}

/// Applies the model's drop cap to a kept `w:framePr` (the reader keeps the frame verbatim and
/// models its drop cap).
fn patch_frame(raw: &str, p: &ParagraphProps) -> String {
    let Ok(mut el) = parse_fragment(raw) else {
        return raw.to_owned();
    };
    let derived = attr(&el, "w:dropCap")
        .and_then(DropCapKind::parse)
        .map(|kind| DropCap {
            kind,
            lines: u8::try_from(
                parse_decimal(attr(&el, "w:lines"))
                    .unwrap_or(3)
                    .clamp(1, 10),
            )
            .unwrap_or(3),
        });
    if derived == p.drop_cap {
        return raw.to_owned();
    }
    let kind = p.drop_cap.map(|d| d.kind.as_str());
    let lines = p.drop_cap.map(|d| d.lines.to_string());
    el.set_attr("w:dropCap", kind);
    el.set_attr("w:lines", lines.as_deref());
    fragment_xml(&el, &kiln_ooxml::xml::Attrs::new())
}

/// What a paragraph's `w:pPr` holds besides its [`ParagraphProps`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ParagraphPropsExtra<'a> {
    /// Unmodelled children, raw.
    pub raw: &'a [String],
    /// The paragraph mark's `w:rPr` content (already written).
    pub mark: Option<&'a str>,
    /// A complete `w:sectPr` ending a section at this paragraph.
    pub sect_pr: Option<&'a str>,
    /// `w:pPrChange`: the revision and the properties before it.
    pub change: Option<(&'a RevisionInfo, &'a ParagraphProps)>,
}

/// The children of `w:pPr` (without the wrapper).
pub(crate) fn paragraph_props_inner(p: &ParagraphProps, x: ParagraphPropsExtra<'_>) -> String {
    let mut e: Entries = Vec::new();
    if let Some(s) = &p.style_id {
        push(&mut e, "w:pStyle", val("w:pStyle", s));
    }
    for (name, v) in [
        ("w:keepNext", p.keep_next),
        ("w:keepLines", p.keep_lines),
        ("w:pageBreakBefore", p.page_break_before),
        ("w:widowControl", p.widow_control),
        ("w:suppressLineNumbers", p.suppress_line_numbers),
        ("w:suppressAutoHyphens", p.suppress_auto_hyphens),
        ("w:bidi", p.bidi),
        ("w:snapToGrid", p.snap_to_grid),
        ("w:contextualSpacing", p.contextual_spacing),
        ("w:mirrorIndents", p.mirror_indents),
    ] {
        if let Some(on) = v {
            push(&mut e, name, toggle(name, on));
        }
    }
    if !has_raw(x.raw, "w:framePr")
        && let Some(frame) = drop_cap_frame(p)
    {
        push(&mut e, "w:framePr", frame);
    }
    if let Some(n) = p.numbering {
        let inner = format!(
            "{}{}",
            n.level.map(|l| val("w:ilvl", l)).unwrap_or_default(),
            n.num_id.map(|i| val("w:numId", i)).unwrap_or_default()
        );
        push(&mut e, "w:numPr", El::new("w:numPr").wrap(&inner));
    }
    if let Some(b) = &p.borders {
        push(&mut e, "w:pBdr", paragraph_borders_xml(b));
    }
    if let Some(s) = &p.shading {
        push(&mut e, "w:shd", shading_xml(s));
    }
    if let Some(t) = &p.tabs
        && !t.is_empty()
    {
        push(&mut e, "w:tabs", tabs_xml(t));
    }
    if let Some(sp) = &p.spacing {
        push(&mut e, "w:spacing", spacing_xml(sp));
    }
    if let Some(ind) = &p.indent {
        push(&mut e, "w:ind", indent_xml(ind));
    }
    if let Some(a) = p.alignment {
        push(&mut e, "w:jc", val("w:jc", a.as_str()));
    }
    if let Some(t) = p.text_alignment {
        push(
            &mut e,
            "w:textAlignment",
            val("w:textAlignment", t.as_str()),
        );
    }
    if let Some(l) = p.outline_level {
        push(&mut e, "w:outlineLvl", val("w:outlineLvl", l));
    }
    for raw in x.raw {
        let name = fragment_name(raw);
        let xml = if name == "w:framePr" {
            patch_frame(raw, p)
        } else {
            raw.clone()
        };
        e.push((name.to_owned(), xml));
    }
    if let Some(m) = x.mark
        && !m.is_empty()
    {
        push(&mut e, "w:rPr", format!("<w:rPr>{m}</w:rPr>"));
    }
    if let Some(s) = x.sect_pr {
        push(&mut e, "w:sectPr", s.to_owned());
    }
    if let Some((info, before)) = x.change {
        let inner = paragraph_props_inner(before, ParagraphPropsExtra::default());
        push(
            &mut e,
            "w:pPrChange",
            revision_el("w:pPrChange", info).wrap(&El::new("w:pPr").wrap(&inner)),
        );
    }
    order_by_sequence(e, PPR_SEQUENCE)
}

/// Whether the reader maps this `w:pPr` child into [`ParagraphProps`] (or the paragraph's mark,
/// section or revision; in a style or numbering level, those are not modelled: pass `in_style`).
pub(crate) fn ppr_child_modelled(c: &XmlElement, in_style: bool) -> bool {
    match c.name.as_str() {
        "w:pStyle"
        | "w:ind"
        | "w:spacing"
        | "w:numPr"
        | "w:keepNext"
        | "w:keepLines"
        | "w:pageBreakBefore"
        | "w:widowControl"
        | "w:contextualSpacing"
        | "w:suppressAutoHyphens"
        | "w:suppressLineNumbers"
        | "w:mirrorIndents"
        | "w:snapToGrid"
        | "w:bidi"
        | "w:outlineLvl"
        | "w:tabs"
        | "w:pBdr"
        | "w:shd"
        | "w:textAlignment" => true,
        "w:jc" => attr(c, "w:val").is_some_and(|v| {
            matches!(
                v,
                "left" | "start" | "center" | "right" | "end" | "both" | "justify"
            ) || Alignment::parse(v).is_some()
        }),
        "w:rPr" | "w:sectPr" | "w:pPrChange" => !in_style,
        // `w:framePr` is kept raw (and patched with the drop cap).
        _ => false,
    }
}

// ------------------------------------------------------------------ tables

/// CT_TblPr (ECMA-376 Part 1 §17.4.60).
pub(crate) const TBLPR_SEQUENCE: &[&str] = &[
    "w:tblStyle",
    "w:tblpPr",
    "w:tblOverlap",
    "w:bidiVisual",
    "w:tblStyleRowBandSize",
    "w:tblStyleColBandSize",
    "w:tblW",
    "w:jc",
    "w:tblCellSpacing",
    "w:tblInd",
    "w:tblBorders",
    "w:shd",
    "w:tblLayout",
    "w:tblCellMar",
    "w:tblLook",
    "w:tblCaption",
    "w:tblDescription",
    "#unknown",
    "w:tblPrChange",
];

/// `w:tblLook` with the Word 2007 bitmask and the Word 2010 attributes.
pub(crate) fn table_look_xml(look: &TableLook) -> String {
    let mut mask = 0u32;
    for (on, bit) in [
        (look.first_row, 0x20),
        (look.last_row, 0x40),
        (look.first_column, 0x80),
        (look.last_column, 0x100),
        (look.no_h_band, 0x200),
        (look.no_v_band, 0x400),
    ] {
        if on {
            mask |= bit;
        }
    }
    let b = |v: bool| if v { "1" } else { "0" };
    El::new("w:tblLook")
        .attr("w:val", format!("{mask:04X}"))
        .attr("w:firstRow", b(look.first_row))
        .attr("w:lastRow", b(look.last_row))
        .attr("w:firstColumn", b(look.first_column))
        .attr("w:lastColumn", b(look.last_column))
        .attr("w:noHBand", b(look.no_h_band))
        .attr("w:noVBand", b(look.no_v_band))
        .empty()
}

fn table_position_xml(p: &TablePosition) -> String {
    El::new("w:tblpPr")
        .opt("w:leftFromText", p.left_from_text)
        .opt("w:rightFromText", p.right_from_text)
        .opt("w:topFromText", p.top_from_text)
        .opt("w:bottomFromText", p.bottom_from_text)
        .opt("w:vertAnchor", p.vert_anchor.as_deref())
        .opt("w:horzAnchor", p.horz_anchor.as_deref())
        .opt("w:tblpXSpec", p.x_spec.as_deref())
        .opt("w:tblpX", p.x)
        .opt("w:tblpYSpec", p.y_spec.as_deref())
        .opt("w:tblpY", p.y)
        .empty()
}

/// The children of `w:tblPr`; `raw` holds unmodelled children.
pub(crate) fn table_props_inner(p: &TableProps, raw: &[String]) -> String {
    let mut e: Entries = Vec::new();
    if let Some(s) = &p.style_id {
        push(&mut e, "w:tblStyle", val("w:tblStyle", s));
    }
    if let Some(pos) = &p.position {
        push(&mut e, "w:tblpPr", table_position_xml(pos));
    }
    if let Some(o) = p.allow_overlap {
        push(
            &mut e,
            "w:tblOverlap",
            val("w:tblOverlap", if o { "overlap" } else { "never" }),
        );
    }
    if let Some(b) = p.bidi_visual {
        push(&mut e, "w:bidiVisual", toggle("w:bidiVisual", b));
    }
    if let Some(n) = p.row_band_size {
        push(
            &mut e,
            "w:tblStyleRowBandSize",
            val("w:tblStyleRowBandSize", n),
        );
    }
    if let Some(n) = p.col_band_size {
        push(
            &mut e,
            "w:tblStyleColBandSize",
            val("w:tblStyleColBandSize", n),
        );
    }
    if let Some(w) = &p.width {
        push(&mut e, "w:tblW", width_xml("w:tblW", w));
    }
    if let Some(a) = p.alignment {
        push(&mut e, "w:jc", val("w:jc", a.as_str()));
    }
    if let Some(s) = p.cell_spacing {
        push(
            &mut e,
            "w:tblCellSpacing",
            width_xml("w:tblCellSpacing", &TableWidth::dxa(s)),
        );
    }
    if let Some(i) = p.indent
        && !has_raw(raw, "w:tblInd")
    {
        push(
            &mut e,
            "w:tblInd",
            width_xml("w:tblInd", &TableWidth::dxa(i)),
        );
    }
    if let Some(b) = &p.borders {
        push(&mut e, "w:tblBorders", table_borders_xml(b));
    }
    if let Some(s) = &p.shading {
        push(&mut e, "w:shd", shading_xml(s));
    }
    if let Some(l) = p.layout {
        push(
            &mut e,
            "w:tblLayout",
            El::new("w:tblLayout").attr("w:type", l.as_str()).empty(),
        );
    }
    if let Some(m) = &p.cell_margins {
        push(&mut e, "w:tblCellMar", margins_xml("w:tblCellMar", m));
    }
    if let Some(l) = &p.look {
        push(&mut e, "w:tblLook", table_look_xml(l));
    }
    if let Some(c) = &p.caption {
        push(&mut e, "w:tblCaption", val("w:tblCaption", c));
    }
    if let Some(d) = &p.description {
        push(&mut e, "w:tblDescription", val("w:tblDescription", d));
    }
    push_raw(&mut e, raw);
    order_by_sequence(e, TBLPR_SEQUENCE)
}

fn dxa_width(c: &XmlElement) -> bool {
    matches!(
        parse_table_width(attr(c, "w:w"), attr(c, "w:type")),
        Some(TableWidth {
            kind: WidthType::Dxa,
            ..
        })
    )
}

fn table_alignment_parses(v: Option<&str>) -> bool {
    matches!(
        v,
        Some("center" | "right" | "end" | "left" | "start" | "both")
    )
}

/// Whether the reader maps this `w:tblPr` child into [`TableProps`].
pub(crate) fn tblpr_child_modelled(c: &XmlElement) -> bool {
    match c.name.as_str() {
        "w:tblStyle"
        | "w:tblW"
        | "w:tblBorders"
        | "w:shd"
        | "w:tblCellMar"
        | "w:tblLayout"
        | "w:tblLook"
        | "w:tblStyleRowBandSize"
        | "w:tblStyleColBandSize"
        | "w:bidiVisual"
        | "w:tblpPr"
        | "w:tblOverlap"
        | "w:tblCaption"
        | "w:tblDescription" => true,
        "w:jc" => table_alignment_parses(attr(c, "w:val")),
        "w:tblInd" | "w:tblCellSpacing" => dxa_width(c),
        _ => false,
    }
}

/// CT_TrPr: the row property choice in Word's order, then the revision elements.
pub(crate) const TRPR_SEQUENCE: &[&str] = &[
    "w:cnfStyle",
    "w:divId",
    "w:gridBefore",
    "w:gridAfter",
    "w:wBefore",
    "w:wAfter",
    "w:cantSplit",
    "w:trHeight",
    "w:tblHeader",
    "w:tblCellSpacing",
    "w:jc",
    "w:hidden",
    "#unknown",
    "w:ins",
    "w:del",
    "w:trPrChange",
];

/// The children of `w:trPr`.
pub(crate) fn row_props_inner(
    p: &TableRowProps,
    raw: &[String],
    revision: Option<&InsDelRevision>,
) -> String {
    let mut e: Entries = Vec::new();
    if let Some(n) = p.grid_before {
        push(&mut e, "w:gridBefore", val("w:gridBefore", n));
    }
    if let Some(n) = p.grid_after {
        push(&mut e, "w:gridAfter", val("w:gridAfter", n));
    }
    if let Some(v) = p.cant_split {
        push(&mut e, "w:cantSplit", toggle("w:cantSplit", v));
    }
    if let Some(h) = &p.height {
        let rule = match h.rule {
            HeightRule::AtLeast => None,
            other => Some(other.as_str()),
        };
        push(
            &mut e,
            "w:trHeight",
            El::new("w:trHeight")
                .attr("w:val", h.value)
                .opt("w:hRule", rule)
                .empty(),
        );
    }
    if let Some(v) = p.header {
        push(&mut e, "w:tblHeader", toggle("w:tblHeader", v));
    }
    if let Some(a) = p.alignment {
        push(&mut e, "w:jc", val("w:jc", a.as_str()));
    }
    if let Some(v) = p.hidden {
        push(&mut e, "w:hidden", toggle("w:hidden", v));
    }
    push_raw(&mut e, raw);
    if let Some(r) = revision {
        let n = match r.kind {
            InsDel::Insertion => "w:ins",
            InsDel::Deletion => "w:del",
        };
        push(&mut e, n, revision_el(n, &r.info).empty());
    }
    order_by_sequence(e, TRPR_SEQUENCE)
}

/// Whether the reader maps this `w:trPr` child into [`TableRowProps`].
pub(crate) fn trpr_child_modelled(c: &XmlElement, in_style: bool) -> bool {
    match c.name.as_str() {
        "w:trHeight" | "w:tblHeader" | "w:cantSplit" | "w:hidden" | "w:gridBefore"
        | "w:gridAfter" => true,
        "w:jc" => table_alignment_parses(attr(c, "w:val")),
        "w:ins" | "w:del" => !in_style,
        _ => false,
    }
}

/// CT_TcPr (ECMA-376 Part 1 §17.4.70).
pub(crate) const TCPR_SEQUENCE: &[&str] = &[
    "w:cnfStyle",
    "w:tcW",
    "w:gridSpan",
    "w:hMerge",
    "w:vMerge",
    "w:tcBorders",
    "w:shd",
    "w:noWrap",
    "w:tcMar",
    "w:textDirection",
    "w:tcFitText",
    "w:vAlign",
    "w:hideMark",
    "w:headers",
    "#unknown",
    "w:cellIns",
    "w:cellDel",
    "w:cellMerge",
    "w:tcPrChange",
];

/// The children of `w:tcPr`.
pub(crate) fn cell_props_inner(
    p: &TableCellProps,
    grid_span: u32,
    v_merge: Option<VMerge>,
    raw: &[String],
) -> String {
    let mut e: Entries = Vec::new();
    if let Some(w) = &p.width {
        push(&mut e, "w:tcW", width_xml("w:tcW", w));
    }
    if grid_span > 1 {
        push(&mut e, "w:gridSpan", val("w:gridSpan", grid_span));
    }
    match v_merge {
        Some(VMerge::Restart) => push(&mut e, "w:vMerge", val("w:vMerge", "restart")),
        Some(VMerge::Continue) => push(&mut e, "w:vMerge", "<w:vMerge/>".to_owned()),
        None => {}
    }
    if let Some(b) = &p.borders {
        push(&mut e, "w:tcBorders", cell_borders_xml(b));
    }
    if let Some(s) = &p.shading {
        push(&mut e, "w:shd", shading_xml(s));
    }
    if let Some(v) = p.no_wrap {
        push(&mut e, "w:noWrap", toggle("w:noWrap", v));
    }
    if let Some(m) = &p.margins {
        push(&mut e, "w:tcMar", margins_xml("w:tcMar", m));
    }
    if let Some(d) = p.text_direction {
        push(
            &mut e,
            "w:textDirection",
            val("w:textDirection", d.as_str()),
        );
    }
    if let Some(v) = p.fit_text {
        push(&mut e, "w:tcFitText", toggle("w:tcFitText", v));
    }
    if let Some(a) = p.v_align {
        push(&mut e, "w:vAlign", val("w:vAlign", a.as_str()));
    }
    if let Some(v) = p.hide_mark {
        push(&mut e, "w:hideMark", toggle("w:hideMark", v));
    }
    push_raw(&mut e, raw);
    order_by_sequence(e, TCPR_SEQUENCE)
}

/// Whether the reader maps this `w:tcPr` child into [`TableCellProps`] (or the cell's spans).
pub(crate) fn tcpr_child_modelled(c: &XmlElement) -> bool {
    match c.name.as_str() {
        "w:tcW" | "w:gridSpan" | "w:vMerge" | "w:hMerge" | "w:tcBorders" | "w:shd" | "w:tcMar"
        | "w:vAlign" | "w:noWrap" | "w:tcFitText" | "w:hideMark" => true,
        "w:textDirection" => attr(c, "w:val")
            .is_some_and(|v| matches!(v, "lr" | "tb" | "rl") || TextDirection::parse(v).is_some()),
        _ => false,
    }
}

// ------------------------------------------------------------------ notes and sections

/// `w:footnotePr`/`w:endnotePr`; `separators` are the `(element, id)` references Word lists in
/// `settings.xml`.
pub(crate) fn note_props_xml(
    name: &str,
    p: &NoteSettings,
    separators: &[(String, String)],
) -> String {
    let mut inner = String::new();
    if let Some(v) = &p.position {
        inner.push_str(&val("w:pos", v));
    }
    if let Some(v) = &p.format {
        inner.push_str(&val("w:numFmt", v.as_str()));
    }
    if let Some(v) = p.start {
        inner.push_str(&val("w:numStart", v));
    }
    if let Some(v) = &p.restart {
        inner.push_str(&val("w:numRestart", v));
    }
    for (el, id) in separators {
        inner.push_str(&El::new(el).attr("w:id", id).empty());
    }
    if inner.is_empty() {
        String::new()
    } else {
        El::new(name).wrap(&inner)
    }
}

fn page_borders_xml(pb: &PageBorders) -> String {
    let mut inner = String::new();
    for (n, b) in [
        ("w:top", &pb.top),
        ("w:left", &pb.left),
        ("w:bottom", &pb.bottom),
        ("w:right", &pb.right),
    ] {
        if let Some(b) = b {
            inner.push_str(&border_xml(n, b));
        }
    }
    El::new("w:pgBorders")
        .opt("w:zOrder", pb.z_order.as_deref())
        .opt("w:display", pb.display.as_deref())
        .opt("w:offsetFrom", pb.offset_from.as_deref())
        .wrap(&inner)
}

/// CT_SectPr (ECMA-376 Part 1 §17.6.17).
pub(crate) const SECTPR_SEQUENCE: &[&str] = &[
    "w:headerReference",
    "w:footerReference",
    "w:footnotePr",
    "w:endnotePr",
    "w:type",
    "w:pgSz",
    "w:pgMar",
    "w:paperSrc",
    "w:pgBorders",
    "w:lnNumType",
    "w:pgNumType",
    "w:cols",
    "w:formProt",
    "w:vAlign",
    "w:noEndnote",
    "w:titlePg",
    "w:textDirection",
    "w:bidi",
    "w:rtlGutter",
    "w:docGrid",
    "w:printerSettings",
    "#unknown",
    "w:sectPrChange",
];

/// A header or footer reference already resolved to a relationship id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeaderFooterRef {
    pub footer: bool,
    pub kind: HeaderFooterKind,
    pub rel_id: String,
}

/// Applies the model's page numbering to a kept `w:pgNumType` (chapter numbering).
fn patch_page_numbering(raw: &str, p: &SectionProps) -> String {
    let Ok(mut el) = parse_fragment(raw) else {
        return raw.to_owned();
    };
    let start = p
        .page_numbering
        .as_ref()
        .and_then(|n| n.start)
        .map(|s| s.to_string());
    let fmt = p
        .page_numbering
        .as_ref()
        .and_then(|n| n.format.as_ref())
        .map(|f| f.as_str().to_owned());
    if attr(&el, "w:start") == start.as_deref() && attr(&el, "w:fmt") == fmt.as_deref() {
        return raw.to_owned();
    }
    el.set_attr("w:start", start.as_deref());
    el.set_attr("w:fmt", fmt.as_deref());
    fragment_xml(&el, &kiln_ooxml::xml::Attrs::new())
}

/// `w:sectPr` of a section.
pub(crate) fn section_props_xml(
    p: &SectionProps,
    x: Option<&SectionExtras>,
    refs: &[HeaderFooterRef],
) -> String {
    let default_extras = SectionExtras::default();
    let x = x.unwrap_or(&default_extras);
    let mut e: Entries = Vec::new();
    for r in refs {
        let n = if r.footer {
            "w:footerReference"
        } else {
            "w:headerReference"
        };
        push(
            &mut e,
            n,
            El::new(n)
                .attr("w:type", r.kind.as_str())
                .attr("r:id", &r.rel_id)
                .empty(),
        );
    }
    if let Some(n) = &x.footnote_pr {
        push(
            &mut e,
            "w:footnotePr",
            note_props_xml("w:footnotePr", n, &[]),
        );
    }
    if let Some(n) = &x.endnote_pr {
        push(&mut e, "w:endnotePr", note_props_xml("w:endnotePr", n, &[]));
    }
    if p.break_type != SectionBreak::NextPage {
        push(&mut e, "w:type", val("w:type", p.break_type.as_str()));
    }
    let sz = &p.page_size;
    let orient = match sz.orientation {
        Orientation::Landscape => Some("landscape"),
        Orientation::Portrait if sz.width > sz.height => Some("portrait"),
        Orientation::Portrait => None,
    };
    push(
        &mut e,
        "w:pgSz",
        El::new("w:pgSz")
            .attr("w:w", sz.width)
            .attr("w:h", sz.height)
            .opt("w:orient", orient)
            .opt("w:code", sz.code)
            .empty(),
    );
    let m = &p.margins;
    push(
        &mut e,
        "w:pgMar",
        El::new("w:pgMar")
            .attr("w:top", m.top)
            .attr("w:right", m.right)
            .attr("w:bottom", m.bottom)
            .attr("w:left", m.left)
            .attr("w:header", m.header)
            .attr("w:footer", m.footer)
            .attr("w:gutter", m.gutter)
            .empty(),
    );
    if let Some(pb) = &x.page_borders {
        push(&mut e, "w:pgBorders", page_borders_xml(pb));
    }
    if let Some(ln) = &p.line_numbering {
        let restart = match ln.restart {
            LineNumberRestart::NewPage => None,
            other => Some(other.as_str()),
        };
        push(
            &mut e,
            "w:lnNumType",
            El::new("w:lnNumType")
                .attr("w:countBy", ln.count_by)
                .opt("w:start", ln.start)
                .opt("w:distance", ln.distance)
                .opt("w:restart", restart)
                .empty(),
        );
    }
    let raw_pg_num = x.xml.iter().find(|r| fragment_name(r) == "w:pgNumType");
    if raw_pg_num.is_none()
        && let Some(n) = &p.page_numbering
    {
        push(
            &mut e,
            "w:pgNumType",
            El::new("w:pgNumType")
                .opt("w:fmt", n.format.as_ref().map(|f| f.as_str().to_owned()))
                .opt("w:start", n.start)
                .empty(),
        );
    }
    let c = &p.columns;
    let cols: String = c
        .cols
        .iter()
        .map(|col| {
            El::new("w:col")
                .attr("w:w", col.width)
                .attr("w:space", col.space)
                .empty()
        })
        .collect();
    let inferred_equal = c.cols.is_empty();
    push(
        &mut e,
        "w:cols",
        El::new("w:cols")
            .opt(
                "w:num",
                (c.count != 1 || !c.cols.is_empty()).then_some(c.count),
            )
            .opt("w:sep", c.separator.then_some("1"))
            .attr("w:space", c.space)
            .opt(
                "w:equalWidth",
                (c.equal_width != inferred_equal).then_some(if c.equal_width { "1" } else { "0" }),
            )
            .wrap(&cols),
    );
    if x.form_prot {
        push(&mut e, "w:formProt", "<w:formProt/>".to_owned());
    }
    if let Some(v) = p.v_align {
        push(&mut e, "w:vAlign", val("w:vAlign", v.as_str()));
    }
    if x.no_endnote {
        push(&mut e, "w:noEndnote", "<w:noEndnote/>".to_owned());
    }
    if p.title_page {
        push(&mut e, "w:titlePg", "<w:titlePg/>".to_owned());
    }
    if let Some(t) = &x.text_direction {
        push(&mut e, "w:textDirection", val("w:textDirection", t));
    }
    if let Some(b) = p.bidi {
        push(&mut e, "w:bidi", toggle("w:bidi", b));
    }
    if x.rtl_gutter {
        push(&mut e, "w:rtlGutter", "<w:rtlGutter/>".to_owned());
    }
    if let Some(g) = &p.doc_grid {
        push(
            &mut e,
            "w:docGrid",
            El::new("w:docGrid")
                .opt("w:type", g.kind.as_deref())
                .opt("w:linePitch", g.line_pitch)
                .opt("w:charSpace", g.char_space)
                .empty(),
        );
    }
    for raw in &x.xml {
        let name = fragment_name(raw);
        let xml = if name == "w:pgNumType" {
            patch_page_numbering(raw, p)
        } else {
            raw.clone()
        };
        e.push((name.to_owned(), xml));
    }
    if let Some(c) = &x.sect_pr_change {
        push(&mut e, "w:sectPrChange", c.clone());
    }
    let mut el = El::new("w:sectPr");
    for (k, v) in &x.attrs {
        el = el.attr(k, v);
    }
    let inner = order_by_sequence(e, SECTPR_SEQUENCE);
    // An empty `w:sectPr` is never produced: page size and margins are always written.
    el.wrap(&inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::props::{ColorValue, Lang, RunFonts, Underline};
    use kiln_doc::section::{Column, Columns, Paper};
    use kiln_doc::st::{BorderStyle, ShadingPattern, ThemeColorName, ThemeFontRef};

    #[test]
    fn run_properties_follow_the_schema_order_with_raw_children_in_place() {
        let p = RunProps {
            style_id: Some("Strong".into()),
            bold: Some(true),
            italic: Some(false),
            size: Some(24),
            color: Some(DocColor::theme(0x2F5496, ThemeColorName::Accent1).with_shade(0xBF)),
            underline: Some(Underline::new(UnderlineStyle::Double)),
            fonts: Some(RunFonts {
                ascii_theme: Some(ThemeFontRef::MajorHAnsi),
                ..RunFonts::latin("Arial")
            }),
            lang: Some(Lang {
                val: Some("en-GB".into()),
                ..Lang::default()
            }),
            ..RunProps::default()
        };
        let raw = vec![
            "<w14:ligatures w14:val=\"standard\"/>".to_owned(),
            "<w:noProof/>".to_owned(),
        ];
        let xml = run_props_inner(
            &p,
            RunPropsExtra {
                raw: &raw,
                font_hint: Some("eastAsia"),
                ..RunPropsExtra::default()
            },
        );
        assert_eq!(
            xml,
            concat!(
                "<w:rStyle w:val=\"Strong\"/>",
                "<w:rFonts w:hint=\"eastAsia\" w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:asciiTheme=\"majorHAnsi\"/>",
                "<w:b/><w:i w:val=\"0\"/><w:noProof/>",
                "<w:color w:val=\"2F5496\" w:themeColor=\"accent1\" w:themeShade=\"BF\"/>",
                "<w:sz w:val=\"24\"/><w:u w:val=\"double\"/><w:lang w:val=\"en-GB\"/>",
                "<w14:ligatures w14:val=\"standard\"/>"
            )
        );
    }

    #[test]
    fn paragraph_properties_include_mark_section_and_change() {
        let p = ParagraphProps {
            style_id: Some("Heading1".into()),
            alignment: Some(Alignment::Center),
            indent: Some(Indent {
                left: Some(720),
                first_line: Some(100),
                hanging: Some(360),
                ..Indent::default()
            }),
            ..ParagraphProps::default()
        };
        let info = RevisionInfo::new(4, "Ed", Some("2026-01-01T00:00:00Z"));
        let before = ParagraphProps::default();
        let raw = vec!["<w:cnfStyle w:val=\"1\"/>".to_owned()];
        let xml = paragraph_props_inner(
            &p,
            ParagraphPropsExtra {
                raw: &raw,
                mark: Some("<w:b/>"),
                sect_pr: Some("<w:sectPr/>"),
                change: Some((&info, &before)),
            },
        );
        assert_eq!(
            xml,
            concat!(
                "<w:pStyle w:val=\"Heading1\"/><w:ind w:left=\"720\" w:hanging=\"360\"/>",
                "<w:jc w:val=\"center\"/><w:cnfStyle w:val=\"1\"/><w:rPr><w:b/></w:rPr><w:sectPr/>",
                "<w:pPrChange w:id=\"4\" w:author=\"Ed\" w:date=\"2026-01-01T00:00:00Z\"><w:pPr/></w:pPrChange>"
            )
        );
    }

    #[test]
    fn shading_and_borders_write_theme_attributes() {
        let s = Shading {
            pattern: ShadingPattern::Clear,
            color: Some(DocColor::auto()),
            fill: Some(DocColor::theme(0xD9E2F3, ThemeColorName::Accent1).with_tint(0x33)),
        };
        assert_eq!(
            shading_xml(&s),
            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"D9E2F3\" w:themeFill=\"accent1\" w:themeFillTint=\"33\"/>"
        );
        let b = Border {
            style: BorderStyle::Single,
            size: Some(4),
            space: Some(0),
            color: Some(DocColor {
                val: ColorValue::Auto,
                ..DocColor::default()
            }),
            shadow: None,
            frame: Some(true),
        };
        assert_eq!(
            border_xml("w:top", &b),
            "<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\" w:frame=\"1\"/>"
        );
    }

    #[test]
    fn table_look_writes_mask_and_attributes() {
        let look = TableLook::default();
        assert_eq!(
            table_look_xml(&look),
            "<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>"
        );
    }

    #[test]
    fn sections_write_orientation_columns_and_kept_children() {
        let mut p = SectionProps::new(Paper::Letter).with_orientation(Orientation::Landscape);
        p.columns = Columns {
            count: 2,
            space: 720,
            equal_width: false,
            separator: true,
            cols: vec![
                Column {
                    width: 4000,
                    space: 720,
                },
                Column {
                    width: 8000,
                    space: 0,
                },
            ],
        };
        p.title_page = true;
        p.break_type = SectionBreak::Continuous;
        let x = SectionExtras {
            xml: vec!["<w:paperSrc w:first=\"1\"/>".to_owned()],
            attrs: vec![("w:rsidR".to_owned(), "00A1".to_owned())],
            ..SectionExtras::default()
        };
        let refs = [HeaderFooterRef {
            footer: false,
            kind: HeaderFooterKind::Default,
            rel_id: "rId7".into(),
        }];
        let xml = section_props_xml(&p, Some(&x), &refs);
        assert!(xml.starts_with("<w:sectPr w:rsidR=\"00A1\"><w:headerReference w:type=\"default\" r:id=\"rId7\"/><w:type w:val=\"continuous\"/>"), "{xml}");
        assert!(xml.contains("<w:pgSz w:w=\"15840\" w:h=\"12240\" w:orient=\"landscape\"/>"));
        assert!(xml.contains("<w:paperSrc w:first=\"1\"/><w:cols w:num=\"2\" w:sep=\"1\" w:space=\"720\"><w:col w:w=\"4000\" w:space=\"720\"/>"), "{xml}");
        assert!(xml.contains("<w:titlePg/>"));
        assert!(xml.find("w:pgMar").unwrap() < xml.find("w:paperSrc").unwrap());
    }

    #[test]
    fn classifiers_follow_the_reader() {
        let el = |x: &str| parse_fragment(x).unwrap();
        assert!(rpr_child_modelled(&el("<w:b/>"), false));
        assert!(!rpr_child_modelled(&el("<w:noProof/>"), false));
        assert!(!rpr_child_modelled(
            &el("<w:highlight w:val=\"bogus\"/>"),
            false
        ));
        assert!(rpr_child_modelled(
            &el("<w:highlight w:val=\"yellow\"/>"),
            false
        ));
        assert!(!rpr_child_modelled(&el("<w:rPrChange/>"), true));
        assert!(ppr_child_modelled(&el("<w:jc w:val=\"both\"/>"), false));
        assert!(!ppr_child_modelled(&el("<w:framePr/>"), false));
        assert!(!ppr_child_modelled(&el("<w:rPr/>"), true));
        assert!(!tblpr_child_modelled(&el(
            "<w:tblInd w:w=\"10%\" w:type=\"pct\"/>"
        )));
        assert!(tblpr_child_modelled(&el(
            "<w:tblInd w:w=\"10\" w:type=\"dxa\"/>"
        )));
        assert!(tcpr_child_modelled(&el(
            "<w:textDirection w:val=\"btLr\"/>"
        )));
        assert!(!tcpr_child_modelled(&el("<w:textDirection w:val=\"x\"/>")));
        assert!(!trpr_child_modelled(&el("<w:ins/>"), true));
    }
}
