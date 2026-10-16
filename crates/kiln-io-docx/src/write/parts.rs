// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package-level parts generated from the model: `styles.xml`, `numbering.xml`, `settings.xml`,
//! `people.xml`, `commentsExtended.xml`, `commentsIds.xml`, `fontTable.xml` and the theme. Story
//! parts (body, headers, footers, notes, comments) are written with the story writer.
//!
//! Styles and list definitions are written from the model, merged with the raw XML the reader
//! kept for them: the children the model covers are regenerated, everything else (`w:rsid`,
//! `w:personal`, `w:tmpl`, `w15:tentative`, Word 2010 effects in style run properties, …) is
//! kept. `settings.xml` is the original element with every modelled value patched in.

use std::collections::BTreeSet;

use kiln_doc::Document;
use kiln_doc::numbering::{AbstractNum, Level, LevelOverride, NumInstance};
use kiln_doc::props::{ParagraphProps, RunProps, TableCellProps, TableProps, TableRowProps};
use kiln_doc::settings::Settings;
use kiln_doc::st::{LevelAlignment, LevelSuffix, StyleType, TableCondition, ThemeColorName};
use kiln_doc::style::{Style, StyleSheet, TableStyleCondition};
use kiln_doc::theme::{ColorSchemeMapping, DocTheme};
use kiln_ooxml::drawingml::color::{ColorModel, DrawingColor};
use kiln_ooxml::drawingml::theme::{Theme, office_theme, serialize_theme, theme_color_table};
use kiln_ooxml::xml::{Attrs, XmlElement, XmlNode, element_to_bytes};

use super::props::{
    ParagraphPropsExtra, RunPropsExtra, cell_props_inner, note_props_xml, paragraph_props_inner,
    ppr_child_modelled, row_props_inner, rpr_child_modelled, run_props_inner, table_props_inner,
    tblpr_child_modelled, tcpr_child_modelled, trpr_child_modelled,
};
use super::xml::{El, order_by_sequence, part, val};
use crate::common::extras::{DocxExtras, Person, SettingsExtras};
use crate::common::xml::{attr, fragment_xml, parse_fragment};

// ------------------------------------------------------------------ raw merging

/// The unmodelled children of a raw property element, serialised in the scope of `scope`.
fn unmodelled(
    raw: Option<&XmlElement>,
    scope: &Attrs,
    modelled: impl Fn(&XmlElement) -> bool,
) -> Vec<String> {
    raw.map(|r| {
        r.elements()
            .filter(|c| !modelled(c))
            .map(|c| fragment_xml(c, scope))
            .collect()
    })
    .unwrap_or_default()
}

fn wrap_present(name: &str, inner: &str) -> String {
    El::new(name).wrap(inner)
}

/// Property children of a style, a table style condition or a numbering level, merged with the
/// raw element's unmodelled children.
struct PropsSource<'r> {
    raw: Option<&'r XmlElement>,
    scope: &'r Attrs,
}

impl PropsSource<'_> {
    fn child(&self, name: &str) -> Option<&XmlElement> {
        self.raw.and_then(|r| r.child(name))
    }

    fn ppr(&self, p: &ParagraphProps) -> String {
        let raw = unmodelled(self.child("w:pPr"), self.scope, |c| {
            ppr_child_modelled(c, true)
        });
        wrap_present(
            "w:pPr",
            &paragraph_props_inner(
                p,
                ParagraphPropsExtra {
                    raw: &raw,
                    ..ParagraphPropsExtra::default()
                },
            ),
        )
    }

    fn rpr(&self, p: &RunProps) -> String {
        let raw = unmodelled(self.child("w:rPr"), self.scope, |c| {
            rpr_child_modelled(c, true)
        });
        wrap_present(
            "w:rPr",
            &run_props_inner(
                p,
                RunPropsExtra {
                    raw: &raw,
                    ..RunPropsExtra::default()
                },
            ),
        )
    }

    fn tbl_pr(&self, p: &TableProps) -> String {
        let raw = unmodelled(self.child("w:tblPr"), self.scope, tblpr_child_modelled);
        wrap_present("w:tblPr", &table_props_inner(p, &raw))
    }

    fn tr_pr(&self, p: &TableRowProps) -> String {
        let raw = unmodelled(self.child("w:trPr"), self.scope, |c| {
            trpr_child_modelled(c, true)
        });
        wrap_present("w:trPr", &row_props_inner(p, &raw, None))
    }

    fn tc_pr(&self, p: &TableCellProps) -> String {
        let raw = unmodelled(self.child("w:tcPr"), self.scope, tcpr_child_modelled);
        wrap_present("w:tcPr", &cell_props_inner(p, 1, None, &raw))
    }
}

/// Raw attributes of `raw` other than `modelled` ones (namespace declarations included).
fn raw_attrs(el: El, raw: Option<&XmlElement>, modelled: &[&str]) -> El {
    let mut el = el;
    if let Some(r) = raw {
        for (k, v) in r.attrs.iter() {
            if !modelled.contains(&k) {
                el = el.attr(k, v);
            }
        }
    }
    el
}

// ------------------------------------------------------------------ styles

const CONDITION_ORDER: [TableCondition; 13] = [
    TableCondition::WholeTable,
    TableCondition::Band1Horz,
    TableCondition::Band2Horz,
    TableCondition::Band1Vert,
    TableCondition::Band2Vert,
    TableCondition::FirstCol,
    TableCondition::FirstRow,
    TableCondition::LastCol,
    TableCondition::LastRow,
    TableCondition::NeCell,
    TableCondition::NwCell,
    TableCondition::SeCell,
    TableCondition::SwCell,
];

/// CT_Style.
const STYLE_SEQUENCE: &[&str] = &[
    "w:name",
    "w:aliases",
    "w:basedOn",
    "w:next",
    "w:link",
    "w:autoRedefine",
    "w:hidden",
    "w:uiPriority",
    "w:semiHidden",
    "w:unhideWhenUsed",
    "w:qFormat",
    "w:locked",
    "w:personal",
    "w:personalCompose",
    "w:personalReply",
    "w:rsid",
    "#unknown",
    "w:pPr",
    "w:rPr",
    "w:tblPr",
    "w:trPr",
    "w:tcPr",
    "w:tblStylePr",
];

fn condition_xml(
    kind: TableCondition,
    c: &TableStyleCondition,
    raw: Option<&XmlElement>,
    scope: &Attrs,
) -> String {
    let src = PropsSource { raw, scope };
    let mut inner = String::new();
    if let Some(p) = &c.ppr {
        inner.push_str(&src.ppr(p));
    }
    if let Some(p) = &c.rpr {
        inner.push_str(&src.rpr(p));
    }
    if let Some(p) = &c.tbl_pr {
        inner.push_str(&src.tbl_pr(p));
    }
    if let Some(p) = &c.tr_pr {
        inner.push_str(&src.tr_pr(p));
    }
    if let Some(p) = &c.tc_pr {
        inner.push_str(&src.tc_pr(p));
    }
    El::new("w:tblStylePr")
        .attr("w:type", kind.as_str())
        .wrap(&inner)
}

/// One `w:style`, from the model merged with the raw element (whose foreign namespace
/// prefixes are declared in `scope`).
pub(crate) fn style_xml(s: &Style, raw: Option<&XmlElement>, scope: &Attrs) -> String {
    let src = PropsSource { raw, scope };
    let table = s.kind == StyleType::Table;
    let mut e: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, xml: String| {
        if !xml.is_empty() {
            e.push((name.to_owned(), xml));
        }
    };
    add("w:name", val("w:name", &s.name));
    if !s.aliases.is_empty() {
        add("w:aliases", val("w:aliases", s.aliases.join(",")));
    }
    if let Some(b) = &s.based_on {
        add("w:basedOn", val("w:basedOn", b));
    }
    if let Some(n) = &s.next {
        add("w:next", val("w:next", n));
    }
    if let Some(l) = &s.link {
        add("w:link", val("w:link", l));
    }
    if s.hidden {
        add("w:hidden", "<w:hidden/>".to_owned());
    }
    if let Some(p) = s.ui_priority {
        add("w:uiPriority", val("w:uiPriority", p));
    }
    if s.semi_hidden {
        add("w:semiHidden", "<w:semiHidden/>".to_owned());
    }
    if s.unhide_when_used {
        add("w:unhideWhenUsed", "<w:unhideWhenUsed/>".to_owned());
    }
    if s.quick_style {
        add("w:qFormat", "<w:qFormat/>".to_owned());
    }
    if s.locked {
        add("w:locked", "<w:locked/>".to_owned());
    }
    if let Some(p) = &s.ppr {
        add("w:pPr", src.ppr(p));
    }
    if let Some(p) = &s.rpr {
        add("w:rPr", src.rpr(p));
    }
    if table {
        if let Some(p) = &s.tbl_pr {
            add("w:tblPr", src.tbl_pr(p));
        }
        if let Some(p) = &s.tr_pr {
            add("w:trPr", src.tr_pr(p));
        }
        if let Some(p) = &s.tc_pr {
            add("w:tcPr", src.tc_pr(p));
        }
        for kind in CONDITION_ORDER {
            if let Some(c) = s.conditions.get(&kind) {
                let raw_c = raw.and_then(|r| {
                    r.children_named("w:tblStylePr")
                        .find(|x| attr(x, "w:type") == Some(kind.as_str()))
                });
                add("w:tblStylePr", condition_xml(kind, c, raw_c, scope));
            }
        }
    }
    // Unmodelled children of the raw style.
    let modelled = |c: &XmlElement| -> bool {
        match c.name.as_str() {
            "w:name" | "w:aliases" | "w:basedOn" | "w:next" | "w:link" | "w:hidden"
            | "w:uiPriority" | "w:semiHidden" | "w:unhideWhenUsed" | "w:qFormat" | "w:locked"
            | "w:pPr" | "w:rPr" => true,
            "w:tblPr" | "w:trPr" | "w:tcPr" => table,
            "w:tblStylePr" => table && attr(c, "w:type").and_then(TableCondition::parse).is_some(),
            _ => false,
        }
    };
    if let Some(r) = raw {
        for c in r.elements().filter(|c| !modelled(c)) {
            e.push((c.name.clone(), fragment_xml(c, scope)));
        }
    }
    let inner = order_by_sequence(e, STYLE_SEQUENCE);
    let el = El::new("w:style")
        .attr("w:type", s.kind.as_str())
        .opt("w:default", s.is_default.then_some("1"))
        .opt("w:customStyle", s.custom.then_some("1"));
    let el = raw_attrs(
        el,
        raw,
        &["w:type", "w:default", "w:customStyle", "w:styleId"],
    );
    el.attr("w:styleId", &s.id).wrap(&inner)
}

/// `styles.xml`. `original` is the original part's root (for `w:docDefaults` children the model
/// does not hold), when the original package is at hand.
pub(crate) fn styles_part(
    sheet: &StyleSheet,
    extras: &DocxExtras,
    original: Option<&XmlElement>,
) -> String {
    let empty = Attrs::new();
    let scope = original.map_or(&empty, |o| &o.attrs);
    let defaults_raw = original.and_then(|o| o.child("w:docDefaults"));
    let rpr_src = PropsSource {
        raw: defaults_raw.and_then(|d| d.child("w:rPrDefault")),
        scope,
    };
    let ppr_src = PropsSource {
        raw: defaults_raw.and_then(|d| d.child("w:pPrDefault")),
        scope,
    };
    let rpr = rpr_src.rpr(&sheet.doc_defaults.rpr);
    let ppr = ppr_src.ppr(&sheet.doc_defaults.ppr);
    let mut body = format!(
        "<w:docDefaults>{}{}</w:docDefaults>",
        El::new("w:rPrDefault").wrap(if rpr == "<w:rPr/>" { "" } else { &rpr }),
        El::new("w:pPrDefault").wrap(if ppr == "<w:pPr/>" { "" } else { &ppr }),
    );
    if let Some(l) = &extras.latent_styles {
        body.push_str(l);
    }
    // The raw styles: from the original part when at hand (one parse), else from the extras.
    let mut raw: std::collections::HashMap<String, (XmlElement, bool)> =
        std::collections::HashMap::new();
    if let Some(o) = original {
        for st in o.children_named("w:style") {
            if let Some(id) = attr(st, "w:styleId") {
                raw.entry(id.to_owned())
                    .or_insert_with(|| (st.clone(), true));
            }
        }
    } else {
        let ids: Vec<&String> = sheet
            .iter()
            .filter_map(|s| extras.styles_xml.get_key_value(&s.id).map(|(k, _)| k))
            .collect();
        let texts: Vec<&str> = ids
            .iter()
            .filter_map(|id| extras.styles_xml.get(*id).map(String::as_str))
            .collect();
        for (id, el) in ids.into_iter().zip(super::xml::parse_many(&texts)) {
            if let Some(el) = el {
                raw.insert(id.clone(), (el, false));
            }
        }
    }
    for s in sheet.iter() {
        match raw.get(&s.id) {
            Some((el, from_original)) => {
                let scope = if *from_original { scope } else { &el.attrs };
                body.push_str(&style_xml(s, Some(el), scope));
            }
            None => body.push_str(&style_xml(s, None, &empty)),
        }
    }
    part(
        "w:styles",
        &body,
        &[
            "w", "r", "w14", "w15", "w16se", "w16cid", "w16", "w16cex", "w16sdtdh", "mc",
        ],
    )
}

// ------------------------------------------------------------------ numbering

const LEVEL_SEQUENCE: &[&str] = &[
    "w:start",
    "w:numFmt",
    "w:lvlRestart",
    "w:pStyle",
    "w:isLgl",
    "w:suff",
    "w:lvlText",
    "w:lvlPicBulletId",
    "w:legacy",
    "w:lvlJc",
    "#unknown",
    "w:pPr",
    "w:rPr",
];

const ABSTRACT_SEQUENCE: &[&str] = &[
    "w:nsid",
    "w:multiLevelType",
    "w:tmpl",
    "w:name",
    "w:styleLink",
    "w:numStyleLink",
    "#unknown",
    "w:lvl",
];

fn level_xml(l: &Level, raw: Option<&XmlElement>, scope: &Attrs) -> String {
    let src = PropsSource { raw, scope };
    let mut e: Vec<(String, String)> = Vec::new();
    e.push(("w:start".into(), val("w:start", l.start)));
    e.push((
        "w:numFmt".into(),
        El::new("w:numFmt")
            .attr("w:val", l.format.as_str())
            .opt("w:format", l.custom_format.as_deref())
            .empty(),
    ));
    if let Some(r) = l.restart_after {
        e.push(("w:lvlRestart".into(), val("w:lvlRestart", r)));
    }
    if let Some(s) = &l.p_style {
        e.push(("w:pStyle".into(), val("w:pStyle", s)));
    }
    if l.is_legal {
        e.push(("w:isLgl".into(), "<w:isLgl/>".into()));
    }
    if l.suffix != LevelSuffix::Tab {
        e.push(("w:suff".into(), val("w:suff", l.suffix.as_str())));
    }
    e.push(("w:lvlText".into(), val("w:lvlText", &l.text)));
    if let Some(p) = l.pic_bullet_id {
        e.push(("w:lvlPicBulletId".into(), val("w:lvlPicBulletId", p)));
    }
    let jc = match l.alignment {
        LevelAlignment::Left => "left",
        LevelAlignment::Center => "center",
        LevelAlignment::Right => "right",
    };
    e.push(("w:lvlJc".into(), val("w:lvlJc", jc)));
    if let Some(p) = &l.ppr {
        e.push(("w:pPr".into(), src.ppr(p)));
    }
    if let Some(p) = &l.rpr {
        e.push(("w:rPr".into(), src.rpr(p)));
    }
    if let Some(r) = raw {
        for c in r.elements() {
            if !matches!(
                c.name.as_str(),
                "w:start"
                    | "w:numFmt"
                    | "w:lvlRestart"
                    | "w:pStyle"
                    | "w:isLgl"
                    | "w:suff"
                    | "w:lvlText"
                    | "w:lvlPicBulletId"
                    | "w:lvlJc"
                    | "w:pPr"
                    | "w:rPr"
            ) {
                e.push((c.name.clone(), fragment_xml(c, scope)));
            }
        }
    }
    let el = raw_attrs(El::new("w:lvl").attr("w:ilvl", l.level), raw, &["w:ilvl"]);
    el.wrap(&order_by_sequence(e, LEVEL_SEQUENCE))
}

fn abstract_xml(a: &AbstractNum, raw: Option<&XmlElement>, scope: &Attrs) -> String {
    let mut e: Vec<(String, String)> = Vec::new();
    if let Some(n) = &a.nsid {
        e.push(("w:nsid".into(), val("w:nsid", n)));
    }
    if let Some(m) = a.multi_level_type {
        e.push((
            "w:multiLevelType".into(),
            val("w:multiLevelType", m.as_str()),
        ));
    }
    if let Some(n) = &a.name {
        e.push(("w:name".into(), val("w:name", n)));
    }
    if let Some(s) = &a.style_link {
        e.push(("w:styleLink".into(), val("w:styleLink", s)));
    }
    if let Some(s) = &a.num_style_link {
        e.push(("w:numStyleLink".into(), val("w:numStyleLink", s)));
    }
    let raw_level = |i: u8| {
        raw.and_then(|r| {
            r.children_named("w:lvl")
                .find(|l| attr(l, "w:ilvl").and_then(|v| v.trim().parse::<u8>().ok()) == Some(i))
        })
    };
    let mut levels: Vec<&Level> = a.levels.iter().collect();
    levels.sort_by_key(|l| l.level);
    for l in levels {
        e.push(("w:lvl".into(), level_xml(l, raw_level(l.level), scope)));
    }
    if let Some(r) = raw {
        for c in r.elements() {
            if !matches!(
                c.name.as_str(),
                "w:nsid"
                    | "w:multiLevelType"
                    | "w:name"
                    | "w:styleLink"
                    | "w:numStyleLink"
                    | "w:lvl"
            ) {
                e.push((c.name.clone(), fragment_xml(c, scope)));
            }
        }
    }
    let el = raw_attrs(
        El::new("w:abstractNum").attr("w:abstractNumId", a.id),
        raw,
        &["w:abstractNumId"],
    );
    el.wrap(&order_by_sequence(e, ABSTRACT_SEQUENCE))
}

fn override_xml(o: &LevelOverride) -> String {
    let mut inner = String::new();
    if let Some(s) = o.start_override {
        inner.push_str(&val("w:startOverride", s));
    }
    if let Some(d) = &o.definition {
        let l = Level {
            level: o.level,
            ..d.clone()
        };
        inner.push_str(&level_xml(&l, None, &Attrs::new()));
    }
    El::new("w:lvlOverride")
        .attr("w:ilvl", o.level)
        .wrap(&inner)
}

fn instance_xml(n: &NumInstance, raw: Option<&XmlElement>) -> String {
    let mut inner = val("w:abstractNumId", n.abstract_id);
    for o in &n.overrides {
        inner.push_str(&override_xml(o));
    }
    raw_attrs(El::new("w:num").attr("w:numId", n.id), raw, &["w:numId"]).wrap(&inner)
}

/// `numbering.xml`: picture bullets, abstract definitions, instances. `original` is the original
/// part's root, when at hand (for `w:num` attributes and `w:numIdMacAtCleanup`).
pub(crate) fn numbering_part(
    doc: &Document,
    extras: &DocxExtras,
    original: Option<&XmlElement>,
) -> String {
    let mut body: String = extras.num_pic_bullets.concat();
    let root_scope = o_attrs(original);
    let from_original = |id: u32| {
        original.and_then(|o| {
            o.children_named("w:abstractNum").find(|a| {
                attr(a, "w:abstractNumId").and_then(|v| v.trim().parse::<u32>().ok()) == Some(id)
            })
        })
    };
    let ids: Vec<u32> = doc.numbering.abstracts.keys().copied().collect();
    let texts: Vec<&str> = if original.is_some() {
        Vec::new()
    } else {
        ids.iter()
            .map(|id| extras.abstract_nums_xml.get(id).map_or("", String::as_str))
            .collect()
    };
    let parsed = if original.is_some() {
        Vec::new()
    } else {
        super::xml::parse_many(&texts)
    };
    for (i, a) in doc.numbering.abstracts.values().enumerate() {
        match from_original(a.id) {
            Some(el) => body.push_str(&abstract_xml(a, Some(el), &root_scope)),
            None => {
                let el = parsed.get(i).and_then(Option::as_ref);
                let scope = el.map(|e| e.attrs.clone()).unwrap_or_default();
                body.push_str(&abstract_xml(a, el, &scope));
            }
        }
    }
    for n in doc.numbering.instances.values() {
        let raw = original.and_then(|o| {
            o.children_named("w:num").find(|x| {
                attr(x, "w:numId").and_then(|v| v.trim().parse::<u32>().ok()) == Some(n.id)
            })
        });
        body.push_str(&instance_xml(n, raw));
    }
    if let Some(m) = original.and_then(|o| o.child("w:numIdMacAtCleanup")) {
        body.push_str(&fragment_xml(m, &o_attrs(original)));
    }
    part(
        "w:numbering",
        &body,
        &["w", "r", "w14", "w15", "w16se", "w16cid", "mc"],
    )
}

fn o_attrs(o: Option<&XmlElement>) -> Attrs {
    o.map(|o| o.attrs.clone()).unwrap_or_default()
}

// ------------------------------------------------------------------ settings

/// CT_Settings (ECMA-376 Part 1 §17.15.1.78).
const SETTINGS_SEQUENCE: &[&str] = &[
    "w:writeProtection",
    "w:view",
    "w:zoom",
    "w:removePersonalInformation",
    "w:removeDateAndTime",
    "w:doNotDisplayPageBoundaries",
    "w:displayBackgroundShape",
    "w:printPostScriptOverText",
    "w:printFractionalCharacterWidth",
    "w:printFormsData",
    "w:embedTrueTypeFonts",
    "w:embedSystemFonts",
    "w:saveSubsetFonts",
    "w:saveFormsData",
    "w:mirrorMargins",
    "w:alignBordersAndEdges",
    "w:bordersDoNotSurroundHeader",
    "w:bordersDoNotSurroundFooter",
    "w:gutterAtTop",
    "w:hideSpellingErrors",
    "w:hideGrammaticalErrors",
    "w:activeWritingStyle",
    "w:proofState",
    "w:formsDesign",
    "w:attachedTemplate",
    "w:linkStyles",
    "w:stylePaneFormatFilter",
    "w:stylePaneSortMethod",
    "w:documentType",
    "w:mailMerge",
    "w:revisionView",
    "w:trackRevisions",
    "w:doNotTrackMoves",
    "w:doNotTrackFormatting",
    "w:documentProtection",
    "w:autoFormatOverride",
    "w:styleLockTheme",
    "w:styleLockQFSet",
    "w:defaultTabStop",
    "w:autoHyphenation",
    "w:consecutiveHyphenLimit",
    "w:hyphenationZone",
    "w:doNotHyphenateCaps",
    "w:showEnvelope",
    "w:summaryLength",
    "w:clickAndTypeStyle",
    "w:defaultTableStyle",
    "w:evenAndOddHeaders",
    "w:bookFoldRevPrinting",
    "w:bookFoldPrinting",
    "w:bookFoldPrintingSheets",
    "w:drawingGridHorizontalSpacing",
    "w:drawingGridVerticalSpacing",
    "w:displayHorizontalDrawingGridEvery",
    "w:displayVerticalDrawingGridEvery",
    "w:doNotUseMarginsForDrawingGridOrigin",
    "w:drawingGridHorizontalOrigin",
    "w:drawingGridVerticalOrigin",
    "w:doNotShadeFormData",
    "w:noPunctuationKerning",
    "w:characterSpacingControl",
    "w:printTwoOnOne",
    "w:strictFirstAndLastChars",
    "w:noLineBreaksAfter",
    "w:noLineBreaksBefore",
    "w:savePreviewPicture",
    "w:doNotValidateAgainstSchema",
    "w:saveInvalidXml",
    "w:ignoreMixedContent",
    "w:alwaysShowPlaceholderText",
    "w:doNotDemarcateInvalidXml",
    "w:saveXmlDataOnly",
    "w:useXSLTWhenSaving",
    "w:saveThroughXslt",
    "w:showXMLTags",
    "w:alwaysMergeEmptyNamespace",
    "w:updateFields",
    "w:hdrShapeDefaults",
    "w:footnotePr",
    "w:endnotePr",
    "w:compat",
    "w:docVars",
    "w:rsids",
    "m:mathPr",
    "w:attachedSchema",
    "w:themeFontLang",
    "w:clrSchemeMapping",
    "w:doNotIncludeSubdocsInStats",
    "w:doNotAutoCompressPictures",
    "w:forceUpgrade",
    "w:captions",
    "w:readModeInkLockDown",
    "w:smartTagType",
    "sl:schemaLibrary",
    "w:shapeDefaults",
    "w:doNotEmbedSmartTags",
    "w:decimalSymbol",
    "w:listSeparator",
];

fn rank(name: &str) -> usize {
    SETTINGS_SEQUENCE
        .iter()
        .position(|n| *n == name)
        .unwrap_or(SETTINGS_SEQUENCE.len())
}

/// Replaces (or removes, with `None`) the `name` children of `root`, keeping schema order.
fn upsert(root: &mut XmlElement, name: &str, replacement: Option<XmlElement>) {
    let first = root
        .children
        .iter()
        .position(|c| c.as_element().is_some_and(|e| e.name == name));
    root.remove_children(name);
    let Some(rep) = replacement else {
        return;
    };
    if let Some(i) = first {
        root.children
            .insert(i.min(root.children.len()), XmlNode::Element(rep));
        return;
    }
    let r = rank(name);
    let at = root
        .children
        .iter()
        .position(|c| c.as_element().is_some_and(|e| rank(&e.name) > r));
    match at {
        Some(j) => root.children.insert(j, XmlNode::Element(rep)),
        None => root.children.push(XmlNode::Element(rep)),
    }
}

fn el(xml: &str) -> Option<XmlElement> {
    parse_fragment(xml).ok()
}

fn on(name: &str, v: bool) -> Option<XmlElement> {
    v.then(|| XmlElement::new(name))
}

/// What `settings.xml` is written from.
pub(crate) struct SettingsInput<'a> {
    /// A document created in Kiln (not read from a file): Word's defaults are written.
    pub new_document: bool,
    pub settings: &'a Settings,
    pub extras: &'a SettingsExtras,
    /// The settings part as read, patched rather than regenerated.
    pub original: Option<&'a str>,
    /// Separator notes Word lists in `w:footnotePr`/`w:endnotePr` (`w:id`s).
    pub footnote_separators: &'a [String],
    pub endnote_separators: &'a [String],
}

fn mapping_value(m: &ColorSchemeMapping, key: &str) -> ThemeColorName {
    match key {
        "w:bg1" => m.bg1,
        "w:t1" => m.t1,
        "w:bg2" => m.bg2,
        _ => m.t2,
    }
}

/// `settings.xml`: the original settings with every modelled value written from the model.
pub(crate) fn settings_part(input: &SettingsInput<'_>) -> String {
    let s = input.settings;
    let x = input.extras;
    let fresh = input.new_document && input.original.is_none();
    let mut root = input
        .original
        .and_then(el)
        .unwrap_or_else(|| XmlElement::new("w:settings"));
    if fresh {
        root.children.push(
            XmlElement::new("w:zoom")
                .with_attr("w:percent", "100")
                .into(),
        );
        root.children.push(
            XmlElement::new("w:characterSpacingControl")
                .with_attr("w:val", "doNotCompress")
                .into(),
        );
    }
    upsert(
        &mut root,
        "w:mirrorMargins",
        on("w:mirrorMargins", s.mirror_margins),
    );
    upsert(
        &mut root,
        "w:trackRevisions",
        on("w:trackRevisions", s.track_revisions),
    );
    upsert(
        &mut root,
        "w:doNotTrackMoves",
        on("w:doNotTrackMoves", x.do_not_track_moves == Some(true)),
    );
    upsert(
        &mut root,
        "w:doNotTrackFormatting",
        on(
            "w:doNotTrackFormatting",
            x.do_not_track_formatting == Some(true),
        ),
    );
    match &x.protection {
        Some(p) => {
            let mut prot = root
                .child("w:documentProtection")
                .cloned()
                .unwrap_or_else(|| XmlElement::new("w:documentProtection"));
            prot.set_attr("w:edit", p.edit.as_deref());
            prot.set_attr("w:enforcement", Some(if p.enforced { "1" } else { "0" }));
            upsert(&mut root, "w:documentProtection", Some(prot));
        }
        None => upsert(&mut root, "w:documentProtection", None),
    }
    let has_tab = root.child("w:defaultTabStop").is_some();
    let tab_matches = root
        .child("w:defaultTabStop")
        .and_then(|t| attr(t, "w:val"))
        .and_then(|v| crate::common::units::parse_twips(Some(v)))
        == Some(s.default_tab_stop);
    if (has_tab && !tab_matches) || (!has_tab && (fresh || s.default_tab_stop != 720)) {
        upsert(
            &mut root,
            "w:defaultTabStop",
            el(&val("w:defaultTabStop", s.default_tab_stop)),
        );
    }
    upsert(
        &mut root,
        "w:autoHyphenation",
        on("w:autoHyphenation", s.auto_hyphenation),
    );
    upsert(
        &mut root,
        "w:evenAndOddHeaders",
        on("w:evenAndOddHeaders", s.even_and_odd_headers),
    );
    upsert(
        &mut root,
        "w:updateFields",
        s.update_fields
            .then(|| XmlElement::new("w:updateFields").with_attr("w:val", "true")),
    );
    let seps = |name: &str, ids: &[String]| -> Vec<(String, String)> {
        ids.iter().map(|i| (name.to_owned(), i.clone())).collect()
    };
    upsert(
        &mut root,
        "w:footnotePr",
        el(&note_props_xml(
            "w:footnotePr",
            &s.footnotes,
            &seps("w:footnote", input.footnote_separators),
        )),
    );
    upsert(
        &mut root,
        "w:endnotePr",
        el(&note_props_xml(
            "w:endnotePr",
            &s.endnotes,
            &seps("w:endnote", input.endnote_separators),
        )),
    );
    // Compatibility mode: Word 2007 files (12) have no compatSetting; keep them that way.
    let had_compat = root.child("w:compat").is_some();
    let mut compat = root
        .child("w:compat")
        .cloned()
        .unwrap_or_else(|| XmlElement::new("w:compat"));
    let mode = s.compatibility_mode.to_string();
    let mut found = false;
    for c in compat
        .children
        .iter_mut()
        .filter_map(XmlNode::as_element_mut)
    {
        if c.name == "w:compatSetting" && attr(c, "w:name") == Some("compatibilityMode") {
            found = true;
            if attr(c, "w:val") != Some(mode.as_str()) {
                c.set_attr("w:val", Some(mode.as_str()));
            }
        }
    }
    if !found && !(s.compatibility_mode == 12 && had_compat) {
        let setting = |name: &str, v: &str| {
            XmlElement::new("w:compatSetting")
                .with_attr("w:name", name)
                .with_attr("w:uri", "http://schemas.microsoft.com/office/word")
                .with_attr("w:val", v)
        };
        compat
            .children
            .push(setting("compatibilityMode", &mode).into());
        if fresh && s.compatibility_mode >= 15 {
            for name in [
                "overrideTableStyleFontSizeAndJustification",
                "enableOpenTypeFeatures",
                "doNotFlipMirrorIndents",
                "differentiateMultirowTableHeaders",
            ] {
                compat.children.push(setting(name, "1").into());
            }
        }
    }
    if had_compat || compat.elements().next().is_some() {
        upsert(&mut root, "w:compat", Some(compat));
    }
    if let Some(l) = &x.theme_font_lang {
        let mut t = root
            .child("w:themeFontLang")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("w:themeFontLang"));
        t.set_attr("w:val", l.val.as_deref());
        t.set_attr("w:eastAsia", l.east_asia.as_deref());
        t.set_attr("w:bidi", l.bidi.as_deref());
        upsert(&mut root, "w:themeFontLang", Some(t));
    }
    let mapping = &s.color_scheme_mapping;
    match root.child("w:clrSchemeMapping").cloned() {
        Some(mut m) => {
            for key in ["w:bg1", "w:t1", "w:bg2", "w:t2"] {
                let want = mapping_value(mapping, key);
                let have = attr(&m, key).and_then(ThemeColorName::parse);
                let default = mapping_value(&ColorSchemeMapping::default(), key);
                if have.unwrap_or(default) != want {
                    m.set_attr(key, Some(want.as_str()));
                }
            }
            upsert(&mut root, "w:clrSchemeMapping", Some(m));
        }
        None if fresh || *mapping != ColorSchemeMapping::default() => {
            let m = XmlElement::new("w:clrSchemeMapping")
                .with_attr("w:bg1", mapping.bg1.as_str())
                .with_attr("w:t1", mapping.t1.as_str())
                .with_attr("w:bg2", mapping.bg2.as_str())
                .with_attr("w:t2", mapping.t2.as_str())
                .with_attr("w:accent1", "accent1")
                .with_attr("w:accent2", "accent2")
                .with_attr("w:accent3", "accent3")
                .with_attr("w:accent4", "accent4")
                .with_attr("w:accent5", "accent5")
                .with_attr("w:accent6", "accent6")
                .with_attr("w:hyperlink", "hyperlink")
                .with_attr("w:followedHyperlink", "followedHyperlink");
            upsert(&mut root, "w:clrSchemeMapping", Some(m));
        }
        None => {}
    }
    upsert(
        &mut root,
        "w:decimalSymbol",
        s.decimal_symbol
            .as_deref()
            .and_then(|v| el(&val("w:decimalSymbol", v))),
    );
    upsert(
        &mut root,
        "w:listSeparator",
        s.list_separator
            .as_deref()
            .and_then(|v| el(&val("w:listSeparator", v))),
    );

    if root.attrs.iter().any(|(k, _)| k.starts_with("xmlns:")) {
        let bytes = element_to_bytes(&root).unwrap_or_default();
        return String::from_utf8(bytes).unwrap_or_default();
    }
    let body: String = root
        .elements()
        .map(|c| fragment_xml(c, &root.attrs))
        .collect();
    part(
        "w:settings",
        &body,
        &["w", "r", "m", "w14", "w15", "w16cid", "w16se", "mc"],
    )
}

// ------------------------------------------------------------------ comments extras, people

pub(crate) fn people_part(people: &[Person]) -> String {
    let body: String = people
        .iter()
        .map(|p| {
            let info = if p.provider_id.is_some() || p.user_id.is_some() {
                El::new("w15:presenceInfo")
                    .opt("w15:providerId", p.provider_id.as_deref())
                    .opt("w15:userId", p.user_id.as_deref())
                    .empty()
            } else {
                String::new()
            };
            El::new("w15:person")
                .attr("w15:author", &p.author)
                .wrap(&info)
        })
        .collect();
    part("w15:people", &body, &["w", "w15", "w14", "mc"])
}

/// One entry of `commentsExtended.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommentEx {
    pub para_id: String,
    pub parent: Option<String>,
    pub done: bool,
}

pub(crate) fn comments_extended_part(entries: &[CommentEx]) -> String {
    let body: String = entries
        .iter()
        .map(|e| {
            El::new("w15:commentEx")
                .attr("w15:paraId", &e.para_id)
                .opt("w15:paraIdParent", e.parent.as_deref())
                .attr("w15:done", if e.done { "1" } else { "0" })
                .empty()
        })
        .collect();
    part("w15:commentsEx", &body, &["w", "w15", "w14", "mc"])
}

/// `commentsIds.xml` (Word 2016+): paraId → durable id.
pub(crate) fn comments_ids_part(entries: &[(String, String)]) -> String {
    let body: String = entries
        .iter()
        .map(|(para, durable)| {
            El::new("w16cid:commentId")
                .attr("w16cid:paraId", para)
                .attr("w16cid:durableId", durable)
                .empty()
        })
        .collect();
    part("w16cid:commentsIds", &body, &["w", "w16cid", "mc"])
}

/// Deterministic 8-hex-digit id below `0x7FFFFFFF` (Word's range for `w14:paraId` and durable
/// ids) that is not in `used`.
pub(crate) fn make_hex_id(seed: &str, used: &mut BTreeSet<String>) -> String {
    let mut h: u32 = 2_166_136_261;
    for b in seed.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
    }
    loop {
        let v = (h >> 1).max(1);
        let id = format!("{v:08X}");
        if used.insert(id.clone()) {
            return id;
        }
        h = (h ^ 0x9E37_79B9).wrapping_mul(16_777_619);
    }
}

/// `fontTable.xml` for a new document: the fonts the theme and the styles name.
pub(crate) fn font_table_part(doc: &Document) -> String {
    let mut fonts: BTreeSet<String> = BTreeSet::new();
    for f in [&doc.theme.major.latin, &doc.theme.minor.latin] {
        if !f.is_empty() {
            fonts.insert(f.clone());
        }
    }
    let mut add = |rp: Option<&RunProps>| {
        if let Some(f) = rp.and_then(|r| r.fonts.as_ref()) {
            for name in [&f.ascii, &f.h_ansi, &f.east_asia, &f.cs]
                .into_iter()
                .flatten()
            {
                fonts.insert(name.clone());
            }
        }
    };
    add(Some(&doc.styles.doc_defaults.rpr));
    for s in doc.styles.iter() {
        add(s.rpr.as_ref());
    }
    let body: String = fonts
        .iter()
        .map(|f| El::new("w:font").attr("w:name", f).empty())
        .collect();
    part("w:fonts", &body, &["w", "r"])
}

// ------------------------------------------------------------------ theme

/// The theme part: the original bytes when the model's theme is the original's, else the
/// original (or the Office theme) with the model's name, colours and fonts.
pub(crate) fn theme_part(theme: &DocTheme, original: Option<&[u8]>) -> Vec<u8> {
    let parsed = original.and_then(|b| kiln_ooxml::drawingml::theme::parse_theme_bytes(b).ok());
    if let (Some(bytes), Some(t)) = (original, &parsed)
        && DocTheme::from_ooxml(t) == *theme
    {
        return bytes.to_vec();
    }
    let mut t: Theme = parsed.unwrap_or_else(office_theme);
    t.name.clone_from(&theme.name);
    let current = theme_color_table(&t);
    for (i, c) in theme.colors.iter().enumerate() {
        let now = current[i].to_color();
        if (now.r, now.g, now.b) != (c.r, c.g, c.b)
            && let Some(slot) = t.color_scheme.colors.get_mut(i)
        {
            *slot = DrawingColor {
                model: ColorModel::Srgb(format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)),
                transforms: Vec::new(),
            };
        }
    }
    let fonts = |f: &mut kiln_ooxml::drawingml::theme::FontCollection,
                 m: &kiln_doc::theme::ThemeFonts| {
        f.latin.typeface.clone_from(&m.latin);
        f.ea.typeface.clone_from(&m.east_asia);
        f.cs.typeface.clone_from(&m.complex);
    };
    fonts(&mut t.font_scheme.major, &theme.major);
    fonts(&mut t.font_scheme.minor, &theme.minor);
    kiln_ooxml::xml::document_to_bytes(&serialize_theme(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::st::StyleType;

    #[test]
    fn styles_keep_unmodelled_children() {
        let raw = parse_fragment(
            r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rsid w:val="00A1"/><w:pPr><w:keepNext/><w:snapToGrid w:val="0"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w14:ligatures w14:val="standard"/></w:rPr></w:style>"#,
        )
        .unwrap();
        let mut s = Style::new("Heading1", StyleType::Paragraph, "heading 1");
        s.based_on = Some("Normal".into());
        s.ppr = Some(ParagraphProps {
            keep_next: Some(true),
            outline_level: Some(0),
            ..ParagraphProps::default()
        });
        s.rpr = Some(RunProps {
            bold: Some(true),
            italic: Some(true),
            ..RunProps::default()
        });
        let xml = style_xml(&s, Some(&raw), &raw.attrs.clone());
        assert!(xml.contains("<w:rsid w:val=\"00A1\"/>"), "{xml}");
        assert!(
            xml.contains("<w14:ligatures w14:val=\"standard\"/>"),
            "{xml}"
        );
        // The model removed snapToGrid (not set) and added italic.
        assert!(!xml.contains("snapToGrid"), "{xml}");
        assert!(xml.contains("<w:i/>"), "{xml}");
        assert!(xml.find("w:rsid").unwrap() < xml.find("w:pPr").unwrap());
    }

    #[test]
    fn settings_patch_keeps_unmodelled_values() {
        let original = r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="120"/><w:trackRevisions/><w:defaultTabStop w:val="720"/><w:rsids><w:rsidRoot w:val="00A1"/></w:rsids><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;
        let settings = Settings {
            track_revisions: false,
            even_and_odd_headers: true,
            compatibility_mode: 14,
            ..Settings::default()
        };
        let x = settings_part(&SettingsInput {
            new_document: false,
            settings: &settings,
            extras: &SettingsExtras::default(),
            original: Some(original),
            footnote_separators: &["-1".to_owned(), "0".to_owned()],
            endnote_separators: &[],
        });
        assert!(x.contains("<w:zoom w:percent=\"120\"/>"));
        assert!(!x.contains("trackRevisions"));
        assert!(x.contains("<w:evenAndOddHeaders/>"));
        assert!(x.contains("w:val=\"14\""));
        assert!(x.contains(
            "<w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr>"
        ));
        assert!(x.contains("<w:rsidRoot w:val=\"00A1\"/>"));
        // Schema order: evenAndOddHeaders after defaultTabStop, footnotePr before compat.
        assert!(x.find("defaultTabStop").unwrap() < x.find("evenAndOddHeaders").unwrap());
        assert!(x.find("footnotePr").unwrap() < x.find("w:compat").unwrap());
    }

    #[test]
    fn hex_ids_are_unique_and_deterministic() {
        let mut used = BTreeSet::new();
        let a = make_hex_id("x", &mut used);
        let b = make_hex_id("x", &mut used);
        assert_ne!(a, b);
        assert_eq!(a.len(), 8);
        assert!(u32::from_str_radix(&a, 16).unwrap() < 0x8000_0000);
        let mut fresh = BTreeSet::new();
        assert_eq!(make_hex_id("x", &mut fresh), a);
    }

    #[test]
    fn unchanged_themes_are_kept_byte_for_byte() {
        let office = office_theme();
        let bytes = kiln_ooxml::xml::document_to_bytes(&serialize_theme(&office)).unwrap();
        let theme = DocTheme::from_ooxml(&office);
        assert_eq!(theme_part(&theme, Some(&bytes)), bytes);
        let mut changed = theme.clone();
        changed.minor.latin = "Aptos".into();
        let out = theme_part(&changed, Some(&bytes));
        let back = kiln_ooxml::drawingml::theme::parse_theme_bytes(&out).unwrap();
        assert_eq!(DocTheme::from_ooxml(&back), changed);
    }
}
