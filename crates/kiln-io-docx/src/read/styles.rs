// SPDX-License-Identifier: AGPL-3.0-or-later
//! `styles.xml` → [`StyleSheet`] (ECMA-376 Part 1 §17.7), plus `numbering.xml` → [`Numbering`]
//! (§17.9) and `settings.xml` → [`Settings`] (§17.15).

use std::collections::BTreeMap;

use kiln_doc::numbering::{AbstractNum, Level, LevelOverride, NumInstance, Numbering};
use kiln_doc::props::Lang;
use kiln_doc::settings::{NoteSettings, Settings};
use kiln_doc::st::{
    LevelAlignment, LevelSuffix, MultiLevelType, NumberFormat, StyleType, TableCondition,
    ThemeColorName,
};
use kiln_doc::style::{DocDefaults, Style, StyleSheet, TableStyleCondition};
use kiln_doc::theme::ColorSchemeMapping;
use kiln_ooxml::xml::XmlElement;

use super::props::{
    Raw, child_val, parse_cell_props, parse_paragraph_props, parse_row_props, parse_run_props,
    parse_table_props,
};
use crate::common::extras::{Protection, SettingsExtras};
use crate::common::units::{parse_decimal, parse_on_off, parse_twips, parse_u32};
use crate::common::xml::{attr, val};

fn flag(node: Option<&XmlElement>) -> bool {
    node.is_some_and(|n| parse_on_off(val(Some(n)), true))
}

fn parse_style(node: &XmlElement, raw: Raw<'_>) -> Option<Style> {
    let id = attr(node, "w:styleId").filter(|s| !s.is_empty())?;
    let kind = attr(node, "w:type")
        .and_then(StyleType::parse)
        .unwrap_or_default();
    let mut style = Style::new(id, kind, child_val(Some(node), "w:name").unwrap_or(id));
    let s = |name: &str| child_val(Some(node), name).map(str::to_owned);
    style.based_on = s("w:basedOn");
    style.next = s("w:next");
    style.link = s("w:link");
    style.is_default = attr(node, "w:default").is_some_and(|v| parse_on_off(Some(v), true));
    style.custom = attr(node, "w:customStyle").is_some_and(|v| parse_on_off(Some(v), true));
    style.quick_style = flag(node.child("w:qFormat"));
    style.ui_priority = parse_u32(child_val(Some(node), "w:uiPriority"));
    style.semi_hidden = flag(node.child("w:semiHidden"));
    style.unhide_when_used = flag(node.child("w:unhideWhenUsed"));
    style.hidden = flag(node.child("w:hidden"));
    style.locked = flag(node.child("w:locked"));
    style.aliases = child_val(Some(node), "w:aliases")
        .map(|a| {
            a.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    style.ppr = node
        .child("w:pPr")
        .map(|p| parse_paragraph_props(Some(p), raw).props);
    style.rpr = node
        .child("w:rPr")
        .map(|r| parse_run_props(Some(r), raw).props);
    if kind == StyleType::Table {
        style.tbl_pr = node
            .child("w:tblPr")
            .map(|t| parse_table_props(Some(t), raw).0);
        style.tr_pr = node
            .child("w:trPr")
            .map(|t| parse_row_props(Some(t), raw).props);
        style.tc_pr = node
            .child("w:tcPr")
            .map(|t| parse_cell_props(Some(t), raw).props);
        for c in node.children_named("w:tblStylePr") {
            let Some(cond) = attr(c, "w:type").and_then(TableCondition::parse) else {
                continue;
            };
            style.conditions.insert(
                cond,
                TableStyleCondition {
                    ppr: c
                        .child("w:pPr")
                        .map(|p| parse_paragraph_props(Some(p), raw).props),
                    rpr: c
                        .child("w:rPr")
                        .map(|r| parse_run_props(Some(r), raw).props),
                    tbl_pr: c
                        .child("w:tblPr")
                        .map(|t| parse_table_props(Some(t), raw).0),
                    tr_pr: c
                        .child("w:trPr")
                        .map(|t| parse_row_props(Some(t), raw).props),
                    tc_pr: c
                        .child("w:tcPr")
                        .map(|t| parse_cell_props(Some(t), raw).props),
                },
            );
        }
    }
    Some(style)
}

/// A parsed `styles.xml`.
pub(crate) struct ParsedStyles {
    pub sheet: StyleSheet,
    pub latent_styles: Option<String>,
    pub styles_xml: BTreeMap<String, String>,
}

/// Parses the root of `styles.xml`. Duplicate ids keep the first definition, as Word does.
pub(crate) fn parse_styles(root: &XmlElement, raw: Raw<'_>) -> ParsedStyles {
    let defaults = root.child("w:docDefaults");
    let ppr = parse_paragraph_props(
        defaults.and_then(|d| d.child_path(&["w:pPrDefault", "w:pPr"])),
        raw,
    )
    .props;
    let rpr = parse_run_props(
        defaults.and_then(|d| d.child_path(&["w:rPrDefault", "w:rPr"])),
        raw,
    )
    .props;
    let mut styles: Vec<Style> = Vec::new();
    let mut styles_xml = BTreeMap::new();
    for s in root.children_named("w:style") {
        if let Some(style) = parse_style(s, raw)
            && !styles_xml.contains_key(&style.id)
        {
            styles_xml.insert(style.id.clone(), raw.xml(s));
            styles.push(style);
        }
    }
    // `StyleSheet::add` lets the last default of a type win; Word uses the first.
    let mut seen_default: Vec<StyleType> = Vec::new();
    for s in &mut styles {
        if s.is_default {
            if seen_default.contains(&s.kind) {
                s.is_default = false;
            } else {
                seen_default.push(s.kind);
            }
        }
    }
    ParsedStyles {
        sheet: StyleSheet::new(DocDefaults { ppr, rpr }, styles),
        latent_styles: root.child("w:latentStyles").map(|l| raw.xml(l)),
        styles_xml,
    }
}

// ------------------------------------------------------------------ numbering

fn parse_level(node: &XmlElement, level: u8, raw: Raw<'_>) -> Level {
    let num_fmt = node.child("w:numFmt");
    let lvl_text = node.child("w:lvlText");
    Level {
        level,
        // ECMA-376 §17.9.26: an omitted w:start means the level starts at zero.
        start: parse_decimal(child_val(Some(node), "w:start")).unwrap_or(0),
        format: NumberFormat::parse(val(num_fmt).unwrap_or("decimal")),
        custom_format: num_fmt.and_then(|f| attr(f, "w:format")).map(str::to_owned),
        // w:lvlText absent = no label text; w:null="1" = explicitly empty.
        text: match lvl_text {
            Some(t) if !parse_on_off(attr(t, "w:null"), false) => {
                val(Some(t)).unwrap_or_default().to_owned()
            }
            _ => String::new(),
        },
        restart_after: parse_decimal(child_val(Some(node), "w:lvlRestart"))
            .and_then(|n| u8::try_from(n.clamp(0, 9)).ok()),
        is_legal: flag(node.child("w:isLgl")),
        suffix: child_val(Some(node), "w:suff")
            .and_then(LevelSuffix::parse)
            .unwrap_or_default(),
        alignment: match child_val(Some(node), "w:lvlJc") {
            Some("center") => LevelAlignment::Center,
            Some("right" | "end") => LevelAlignment::Right,
            _ => LevelAlignment::Left,
        },
        p_style: child_val(Some(node), "w:pStyle").map(str::to_owned),
        pic_bullet_id: parse_u32(child_val(Some(node), "w:lvlPicBulletId")),
        ppr: node
            .child("w:pPr")
            .map(|p| parse_paragraph_props(Some(p), raw).props),
        rpr: node
            .child("w:rPr")
            .map(|r| parse_run_props(Some(r), raw).props),
    }
}

fn level_index(node: &XmlElement) -> Option<u8> {
    let n = parse_decimal(attr(node, "w:ilvl"))?;
    u8::try_from(n).ok().filter(|l| *l <= 8)
}

fn parse_abstract(node: &XmlElement, raw: Raw<'_>) -> Option<AbstractNum> {
    let id = parse_u32(attr(node, "w:abstractNumId"))?;
    let mut levels: Vec<Level> = Vec::new();
    for l in node.children_named("w:lvl") {
        if let Some(i) = level_index(l)
            && !levels.iter().any(|x| x.level == i)
        {
            levels.push(parse_level(l, i, raw));
        }
    }
    levels.sort_by_key(|l| l.level);
    Some(AbstractNum {
        id,
        nsid: child_val(Some(node), "w:nsid").map(str::to_owned),
        multi_level_type: child_val(Some(node), "w:multiLevelType").and_then(MultiLevelType::parse),
        name: child_val(Some(node), "w:name").map(str::to_owned),
        style_link: child_val(Some(node), "w:styleLink").map(str::to_owned),
        num_style_link: child_val(Some(node), "w:numStyleLink").map(str::to_owned),
        levels,
    })
}

fn parse_instance(node: &XmlElement, raw: Raw<'_>) -> Option<NumInstance> {
    let id = parse_u32(attr(node, "w:numId"))?;
    let abstract_id = parse_u32(child_val(Some(node), "w:abstractNumId"))?;
    let overrides = node
        .children_named("w:lvlOverride")
        .filter_map(|o| {
            let level = level_index(o)?;
            Some(LevelOverride {
                level,
                start_override: parse_decimal(child_val(Some(o), "w:startOverride")),
                definition: o.child("w:lvl").map(|d| parse_level(d, level, raw)),
            })
        })
        .collect();
    Some(NumInstance {
        id,
        abstract_id,
        overrides,
    })
}

/// A parsed `numbering.xml`.
pub(crate) struct ParsedNumbering {
    pub numbering: Numbering,
    pub pic_bullets: Vec<String>,
    pub abstracts_xml: BTreeMap<u32, String>,
}

/// Parses the root of `numbering.xml`. Instances whose abstract definition is missing are
/// dropped (Word ignores them); duplicate ids keep the first definition.
pub(crate) fn parse_numbering(root: &XmlElement, raw: Raw<'_>) -> ParsedNumbering {
    let mut numbering = Numbering::new();
    let mut abstracts_xml = BTreeMap::new();
    for a in root.children_named("w:abstractNum") {
        if let Some(abs) = parse_abstract(a, raw)
            && !numbering.abstracts.contains_key(&abs.id)
        {
            abstracts_xml.insert(abs.id, raw.xml(a));
            numbering.abstracts.insert(abs.id, abs);
        }
    }
    for n in root.children_named("w:num") {
        if let Some(inst) = parse_instance(n, raw)
            && numbering.abstracts.contains_key(&inst.abstract_id)
            && !numbering.instances.contains_key(&inst.id)
        {
            numbering.instances.insert(inst.id, inst);
        }
    }
    ParsedNumbering {
        numbering,
        pic_bullets: root
            .children_named("w:numPicBullet")
            .map(|p| raw.xml(p))
            .collect(),
        abstracts_xml,
    }
}

// ------------------------------------------------------------------ settings

/// `w:footnotePr` / `w:endnotePr` (shared by settings and section properties).
pub(crate) fn parse_note_settings(node: Option<&XmlElement>) -> Option<NoteSettings> {
    let node = node?;
    let s = NoteSettings {
        format: child_val(Some(node), "w:numFmt").map(NumberFormat::parse),
        start: parse_decimal(child_val(Some(node), "w:numStart")),
        restart: child_val(Some(node), "w:numRestart").map(str::to_owned),
        position: child_val(Some(node), "w:pos").map(str::to_owned),
    };
    (s != NoteSettings::default()).then_some(s)
}

fn parse_mapping(node: &XmlElement) -> ColorSchemeMapping {
    let mut m = ColorSchemeMapping::default();
    let get =
        |k: &str, d: ThemeColorName| attr(node, k).and_then(ThemeColorName::parse).unwrap_or(d);
    m.bg1 = get("w:bg1", m.bg1);
    m.t1 = get("w:t1", m.t1);
    m.bg2 = get("w:bg2", m.bg2);
    m.t2 = get("w:t2", m.t2);
    m
}

/// Parses the root of `settings.xml`. Missing values take Word's defaults.
pub(crate) fn parse_settings(root: Option<&XmlElement>) -> (Settings, SettingsExtras) {
    let mut s = Settings::default();
    let mut x = SettingsExtras::default();
    let Some(root) = root else {
        return (s, x);
    };
    let on = |name: &str| root.child(name).map(|n| parse_on_off(val(Some(n)), true));
    if let Some(tab) = parse_twips(child_val(Some(root), "w:defaultTabStop")).filter(|t| *t > 0) {
        s.default_tab_stop = tab;
    }
    s.even_and_odd_headers = on("w:evenAndOddHeaders").unwrap_or(false);
    s.mirror_margins = on("w:mirrorMargins").unwrap_or(false);
    s.track_revisions = on("w:trackRevisions").unwrap_or(false);
    s.update_fields = on("w:updateFields").unwrap_or(false);
    s.auto_hyphenation = on("w:autoHyphenation").unwrap_or(false);
    s.decimal_symbol = child_val(Some(root), "w:decimalSymbol").map(str::to_owned);
    s.list_separator = child_val(Some(root), "w:listSeparator").map(str::to_owned);
    if let Some(m) = root.child("w:clrSchemeMapping") {
        s.color_scheme_mapping = parse_mapping(m);
    }
    let compat = root.child("w:compat");
    let mode = compat.and_then(|c| {
        c.children_named("w:compatSetting")
            .find(|cs| attr(cs, "w:name") == Some("compatibilityMode"))
            .and_then(|cs| parse_u32(attr(cs, "w:val")))
    });
    // No compatSetting in a compat block means a Word 2007 (mode 12) document.
    s.compatibility_mode = mode.unwrap_or(if compat.is_some() {
        12
    } else {
        s.compatibility_mode
    });
    if let Some(n) = parse_note_settings(root.child("w:footnotePr")) {
        s.footnotes = n;
    }
    if let Some(n) = parse_note_settings(root.child("w:endnotePr")) {
        s.endnotes = n;
    }

    x.do_not_track_moves = on("w:doNotTrackMoves");
    x.do_not_track_formatting = on("w:doNotTrackFormatting");
    x.theme_font_lang = root.child("w:themeFontLang").map(|l| Lang {
        val: attr(l, "w:val").map(str::to_owned),
        east_asia: attr(l, "w:eastAsia").map(str::to_owned),
        bidi: attr(l, "w:bidi").map(str::to_owned),
    });
    x.protection = root.child("w:documentProtection").map(|p| Protection {
        edit: attr(p, "w:edit").map(str::to_owned),
        enforced: parse_on_off(attr(p, "w:enforcement"), false),
    });
    (s, x)
}
