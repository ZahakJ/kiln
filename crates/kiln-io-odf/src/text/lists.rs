// SPDX-License-Identifier: AGPL-3.0-or-later
//! List styles (`text:list-style`) ⇄ list definitions (`kiln_doc::AbstractNum`).
//!
//! | ODF level | Model level |
//! |---|---|
//! | `text:list-level-style-number`: `style:num-format` `1 a A i I` (or empty), `style:num-prefix`, `style:num-suffix`, `text:display-levels`, `text:start-value` | format decimal, lower/upper letter, lower/upper roman (or none), template such as `%1.%2.`, start |
//! | `text:list-level-style-bullet`: `text:bullet-char`, bullet font | bullet with its character and font |
//! | `text:list-level-style-image` | bullet `•` (the picture is not kept) |
//! | `style:list-level-label-alignment`: `fo:margin-left`, `fo:text-indent`, `text:label-followed-by`, `text:list-tab-stop-position`; `fo:text-align` | level indent and hanging indent, suffix (tab, space, nothing), alignment |

use kiln_doc::numbering::{AbstractNum, Level, NumInstance};
use kiln_doc::props::{Indent, ParagraphProps, RunFonts, RunProps, TabStop};
use kiln_doc::st::{LevelAlignment, LevelSuffix, NumberFormat, TabAlignment};
use kiln_ooxml::xml::XmlElement;

use crate::el::El;
use crate::props::FontFaces;
use crate::units::{twips, twips_str};
use crate::xml::{kid, kids};

fn format_of(f: &str) -> NumberFormat {
    match f {
        "" => NumberFormat::None,
        "a" => NumberFormat::LowerLetter,
        "A" => NumberFormat::UpperLetter,
        "i" => NumberFormat::LowerRoman,
        "I" => NumberFormat::UpperRoman,
        "01, 02, 03, ..." | "001, 002, 003, ..." => NumberFormat::DecimalZero,
        "①, ②, ③, ..." => NumberFormat::DecimalEnclosedCircle,
        "１, ２, ３, ..." => NumberFormat::DecimalFullWidth,
        "一, 二, 三, ..." => NumberFormat::ChineseCounting,
        "а, б, .., аа, аб, ... (ru)" => NumberFormat::RussianLower,
        "А, Б, .., Аа, Аб, ... (ru)" => NumberFormat::RussianUpper,
        "א, ב, .., יא, יב, ..." => NumberFormat::Hebrew1,
        "أ, ب, ت, ..." => NumberFormat::ArabicAlpha,
        "ก, ข, ฃ, ..." => NumberFormat::ThaiNumbers,
        "1st" => NumberFormat::Ordinal,
        "One" => NumberFormat::CardinalText,
        "First" => NumberFormat::OrdinalText,
        _ => NumberFormat::Decimal,
    }
}

fn format_str(f: &NumberFormat) -> &'static str {
    match f {
        NumberFormat::None => "",
        NumberFormat::LowerLetter => "a",
        NumberFormat::UpperLetter => "A",
        NumberFormat::LowerRoman => "i",
        NumberFormat::UpperRoman => "I",
        NumberFormat::DecimalZero => "01, 02, 03, ...",
        NumberFormat::DecimalEnclosedCircle => "①, ②, ③, ...",
        NumberFormat::DecimalFullWidth => "１, ２, ３, ...",
        NumberFormat::ChineseCounting => "一, 二, 三, ...",
        NumberFormat::RussianLower => "а, б, .., аа, аб, ... (ru)",
        NumberFormat::RussianUpper => "А, Б, .., Аа, Аб, ... (ru)",
        NumberFormat::Hebrew1 => "א, ב, .., יא, יב, ...",
        NumberFormat::ArabicAlpha => "أ, ب, ت, ...",
        NumberFormat::Ordinal => "1st",
        NumberFormat::CardinalText => "One",
        NumberFormat::OrdinalText => "First",
        _ => "1",
    }
}

fn level_props(e: &XmlElement, level: &mut Level) {
    let Some(lp) = kid(e, "style:list-level-properties") else {
        return;
    };
    level.alignment = match lp.attr("fo:text-align") {
        Some("center") => LevelAlignment::Center,
        Some("end" | "right") => LevelAlignment::Right,
        _ => LevelAlignment::Left,
    };
    let mut indent = Indent::default();
    let mut tabs = None;
    if let Some(la) = kid(lp, "style:list-level-label-alignment") {
        indent.left = la.attr("fo:margin-left").and_then(twips);
        if let Some(ti) = la.attr("fo:text-indent").and_then(twips) {
            if ti < 0 {
                indent.hanging = Some(-ti);
            } else if ti > 0 {
                indent.first_line = Some(ti);
            }
        }
        level.suffix = match la.attr("text:label-followed-by") {
            Some("space") => LevelSuffix::Space,
            Some("nothing") => LevelSuffix::Nothing,
            _ => LevelSuffix::Tab,
        };
        if level.suffix == LevelSuffix::Tab
            && let Some(pos) = la.attr("text:list-tab-stop-position").and_then(twips)
            && Some(pos) != indent.left
        {
            tabs = Some(vec![TabStop {
                pos,
                align: TabAlignment::Num,
                leader: None,
            }]);
        }
    } else {
        // The older "position and space" mode: the label starts at space-before, the text at
        // space-before + min-label-width.
        let before = lp.attr("text:space-before").and_then(twips).unwrap_or(0);
        let width = lp.attr("text:min-label-width").and_then(twips).unwrap_or(0);
        indent.left = Some(before + width);
        if width > 0 {
            indent.hanging = Some(width);
        }
    }
    if indent != Indent::default() || tabs.is_some() {
        level.ppr = Some(ParagraphProps {
            indent: Some(indent).filter(|i| *i != Indent::default()),
            tabs,
            ..ParagraphProps::default()
        });
    }
}

/// Reads a list style into a definition with nine levels (levels the style leaves out are
/// decimal with the default indents).
#[must_use]
pub fn read_list_style(el: &XmlElement, id: u32, fonts: &FontFaces) -> AbstractNum {
    let mut levels: Vec<Level> = (0..9u8)
        .map(|l| {
            let mut lv = Level::new(l, NumberFormat::Decimal, &format!("%{}.", l + 1));
            lv.ppr = Some(ParagraphProps {
                indent: Some(Indent {
                    left: Some(720 * (i32::from(l) + 1)),
                    hanging: Some(360),
                    ..Indent::default()
                }),
                ..ParagraphProps::default()
            });
            lv
        })
        .collect();
    for e in el.elements() {
        let Some(n) = e
            .attr("text:level")
            .and_then(|l| l.parse::<u8>().ok())
            .filter(|l| (1..=9).contains(l))
        else {
            continue;
        };
        let li = n - 1;
        let level = &mut levels[usize::from(li)];
        level.ppr = None;
        match e.name.as_str() {
            "text:list-level-style-number" => {
                level.format = format_of(e.attr("style:num-format").unwrap_or("1"));
                level.start = e
                    .attr("text:start-value")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(1);
                let display = e
                    .attr("text:display-levels")
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, n);
                let prefix = e.attr("style:num-prefix").unwrap_or_default();
                let suffix = e.attr("style:num-suffix").unwrap_or_default();
                let body: Vec<String> = ((n - display + 1)..=n).map(|k| format!("%{k}")).collect();
                level.text = if level.format == NumberFormat::None {
                    format!("{prefix}{suffix}")
                } else {
                    format!("{prefix}{}{suffix}", body.join("."))
                };
            }
            "text:list-level-style-bullet" => {
                level.format = NumberFormat::Bullet;
                level.text = e
                    .attr("text:bullet-char")
                    .filter(|c| !c.is_empty())
                    .unwrap_or("•")
                    .to_owned();
                if let Some(tp) = kid(e, "style:text-properties") {
                    let font = tp
                        .attr("style:font-name")
                        .map(|n| fonts.family(n))
                        .or_else(|| tp.attr("fo:font-family").map(crate::props::unquote));
                    if let Some(f) = font {
                        level.rpr = Some(RunProps {
                            fonts: Some(RunFonts {
                                ascii: Some(f.clone()),
                                h_ansi: Some(f),
                                ..RunFonts::default()
                            }),
                            ..RunProps::default()
                        });
                    }
                }
            }
            "text:list-level-style-image" => {
                level.format = NumberFormat::Bullet;
                level.text = "•".to_owned();
            }
            _ => continue,
        }
        level_props(e, level);
    }
    AbstractNum {
        id,
        nsid: None,
        multi_level_type: None,
        name: el.attr("style:display-name").or(el.attr("style:name")).map(str::to_owned),
        style_link: None,
        num_style_link: None,
        levels,
    }
}

/// Splits a level template (`%1.%2.`) into prefix, number of displayed levels and suffix when
/// it is the shape ODF can express (consecutive levels ending at this one, joined by `.`).
fn split_template(text: &str, level: u8) -> (String, u8, String) {
    let n = level + 1;
    let Some(first) = text.find('%') else {
        return (text.to_owned(), 0, String::new());
    };
    let prefix = text[..first].to_owned();
    let mut rest = &text[first..];
    let mut levels: Vec<u8> = Vec::new();
    while let Some(r) = rest.strip_prefix('%') {
        let Some(d) = r.chars().next().and_then(|c| c.to_digit(10)) else {
            break;
        };
        // A single digit (levels are 1–9).
        #[allow(clippy::cast_possible_truncation)]
        levels.push(d as u8);
        rest = &r[1..];
        match rest.strip_prefix('.') {
            Some(after) if after.starts_with('%') => rest = after,
            _ => break,
        }
    }
    let consecutive = !levels.is_empty()
        && levels.last() == Some(&n)
        && levels.windows(2).all(|w| w[1] == w[0] + 1);
    if consecutive {
        // At most nine levels.
        #[allow(clippy::cast_possible_truncation)]
        let shown = levels.len() as u8;
        (prefix, shown, rest.to_owned())
    } else {
        // Not expressible: show this level alone with the literal text around it.
        let suffix = text.rsplit(&format!("%{n}")).next().unwrap_or_default();
        (prefix, 1, suffix.to_owned())
    }
}

/// Writes a list definition (with an instance's overrides) as `text:list-style`.
#[must_use]
pub fn list_style_el(name: &str, abs: &AbstractNum, inst: Option<&NumInstance>) -> El {
    let mut e = El::new("text:list-style").with("style:name", name);
    if let Some(n) = &abs.name
        && n != name
    {
        e.set("style:display-name", n.clone());
    }
    for level in &abs.levels {
        let ov = inst.and_then(|i| i.overrides.iter().find(|o| o.level == level.level));
        let level = ov.and_then(|o| o.definition.as_ref()).unwrap_or(level);
        let start = ov.and_then(|o| o.start_override).unwrap_or(level.start);
        let n = level.level.min(8) + 1;
        let mut le = if level.format == NumberFormat::Bullet {
            let ch = level.text.chars().next().unwrap_or('•');
            let mut b = El::new("text:list-level-style-bullet")
                .with("text:level", n.to_string())
                .with("text:bullet-char", ch.to_string());
            if let Some(font) = level
                .rpr
                .as_ref()
                .and_then(|r| r.fonts.as_ref())
                .and_then(|f| f.ascii.clone().or_else(|| f.h_ansi.clone()))
            {
                b.children.push(
                    El::new("style:text-properties")
                        .with("fo:font-family", font.clone())
                        .with("style:font-name", font),
                );
            }
            b
        } else {
            let (prefix, shown, suffix) = split_template(&level.text, level.level.min(8));
            let mut num = El::new("text:list-level-style-number").with("text:level", n.to_string());
            if !prefix.is_empty() {
                num.set("style:num-prefix", prefix);
            }
            if !suffix.is_empty() {
                num.set("style:num-suffix", suffix);
            }
            num.set(
                "style:num-format",
                if shown == 0 { "" } else { format_str(&level.format) },
            );
            if shown > 1 {
                num.set("text:display-levels", shown.to_string());
            }
            if start != 1 {
                num.set("text:start-value", start.to_string());
            }
            num
        };
        let indent = level.ppr.as_ref().and_then(|p| p.indent.as_ref());
        let left = indent.and_then(|i| i.left).unwrap_or(0);
        let text_indent = indent
            .and_then(|i| i.hanging.map(|h| -h).or(i.first_line))
            .unwrap_or(0);
        let mut la = El::new("style:list-level-label-alignment").with(
            "text:label-followed-by",
            match level.suffix {
                LevelSuffix::Tab => "listtab",
                LevelSuffix::Space => "space",
                LevelSuffix::Nothing => "nothing",
            },
        );
        if level.suffix == LevelSuffix::Tab {
            let tab = level
                .ppr
                .as_ref()
                .and_then(|p| p.tabs.as_ref())
                .and_then(|t| t.iter().find(|t| t.align == TabAlignment::Num))
                .map_or(left, |t| t.pos);
            la.set("text:list-tab-stop-position", twips_str(i64::from(tab)));
        }
        la.set("fo:text-indent", twips_str(i64::from(text_indent)));
        la.set("fo:margin-left", twips_str(i64::from(left)));
        let mut lp = El::new("style:list-level-properties")
            .with("text:list-level-position-and-space-mode", "label-alignment");
        match level.alignment {
            LevelAlignment::Center => lp.set("fo:text-align", "center"),
            LevelAlignment::Right => lp.set("fo:text-align", "end"),
            LevelAlignment::Left => {}
        }
        lp.children.push(la);
        le.children.insert(0, lp);
        e.children.push(le);
    }
    e
}

/// The paragraph-level `text:list-level-style-*` element names.
pub const LEVEL_ELEMENTS: [&str; 3] = [
    "text:list-level-style-number",
    "text:list-level-style-bullet",
    "text:list-level-style-image",
];

/// Whether a list style has any level element.
#[must_use]
pub fn has_levels(el: &XmlElement) -> bool {
    LEVEL_ELEMENTS.iter().any(|n| kids(el, n).next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::XmlNode;

    fn to_xml(e: &El) -> XmlElement {
        let mut x = XmlElement::new(&e.name);
        for (k, v) in &e.attrs {
            x.attrs.set(k.clone(), v.clone());
        }
        for c in &e.children {
            x.children.push(XmlNode::Element(to_xml(c)));
        }
        x
    }

    #[test]
    fn templates_split() {
        assert_eq!(split_template("%1.%2.", 1), (String::new(), 2, ".".into()));
        assert_eq!(split_template("(%1)", 0), ("(".into(), 1, ")".into()));
        assert_eq!(split_template("Step %3:", 2), ("Step ".into(), 1, ":".into()));
        assert_eq!(split_template("%1-%2", 1), (String::new(), 1, String::new()));
    }

    #[test]
    fn list_styles_round_trip() {
        let mut abs = read_list_style(&XmlElement::new("text:list-style"), 0, &FontFaces::default());
        abs.levels[0].format = NumberFormat::Bullet;
        abs.levels[0].text = "◦".into();
        abs.levels[1].format = NumberFormat::LowerRoman;
        abs.levels[1].text = "%1.%2)".into();
        abs.levels[1].start = 3;
        abs.levels[2].suffix = LevelSuffix::Space;
        abs.name = Some("L1".into());
        let el = to_xml(&list_style_el("L1", &abs, None));
        let back = read_list_style(&el, 0, &FontFaces::default());
        assert_eq!(back.levels, abs.levels);
    }
}
