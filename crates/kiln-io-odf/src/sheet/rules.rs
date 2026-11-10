// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional formatting and data validation.
//!
//! - **Conditional formats** are LibreOffice's `calcext:conditional-formats` (what LibreOffice
//!   and Kiln write; ODF 1.3 has no richer standard form): value conditions (`>50`,
//!   `between(0,25)`), `formula-is(…)`, text conditions, duplicates and uniques, top/bottom N
//!   and percent, above/below average, errors, `calcext:date-is`, colour scales, data bars and
//!   icon sets. The format a condition applies is a named cell style in `styles.xml`
//!   (`ConditionalStyle_N`), mapped to and from the Grid's differential formats.
//! - **Data validation** is `table:content-validation` with its `table:condition`
//!   (`of:cell-content-is-whole-number() and cell-content-is-between(1,10)`, lists of values or
//!   a range, text length, custom formulas), input help and error alert.

use std::sync::Arc;

use kiln_ooxml::xml::XmlElement;
use kiln_sheet::Theme;
use kiln_sheet::color::Color;
use kiln_sheet::meta::{
    AxisPosition, CfKind, CfRule, Cfvo, CfvoType, ConditionalFormat, DataBar, DataValidation,
    ErrorStyle, IconSet, IconSetName, Operator, TimePeriod, ValidationType,
};
use kiln_sheet::styles::{Dxf, Fill, FontPatch, Underline};
use kiln_sheet::{Range, num::F64};

use crate::el::El;
use crate::formula::{excel_to_odf, odf_to_excel};
use crate::props::FontFaces;
use crate::sheet::style::{background, color_hex, points_exact};
use crate::xml::{XmlOut, all_text, kid, kids};

/// Splits `a,b` on commas outside parentheses, brackets, braces and quotes.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let (mut dq, mut sq) = (false, false);
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '"' if !sq => dq = !dq,
            '\'' if !dq => sq = !sq,
            '(' | '[' | '{' if !dq && !sq => depth += 1,
            ')' | ']' | '}' if !dq && !sq => depth -= 1,
            ',' if depth == 0 && !dq && !sq => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out.into_iter().map(|a| a.trim().to_owned()).collect()
}

/// `name(args)` → `args` when `s` starts with `name(` and ends with `)`.
fn call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    s.strip_prefix(name)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
}

fn excel(f: &str) -> String {
    odf_to_excel(&format!("of:={}", f.trim()))
}

fn odf(f: &str) -> String {
    excel_to_odf(f).trim_start_matches("of:=").to_owned()
}

fn unquote_text(s: &str) -> String {
    let t = s.trim();
    match t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => t.to_owned(),
    }
}

fn quote_text(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

const OPERATORS: &[(&str, Operator)] = &[
    (">=", Operator::GreaterThanOrEqual),
    ("<=", Operator::LessThanOrEqual),
    ("!=", Operator::NotEqual),
    ("<>", Operator::NotEqual),
    (">", Operator::GreaterThan),
    ("<", Operator::LessThan),
    ("=", Operator::Equal),
];

fn op_str(op: Operator) -> &'static str {
    match op {
        Operator::GreaterThanOrEqual => ">=",
        Operator::LessThanOrEqual => "<=",
        Operator::NotEqual => "!=",
        Operator::GreaterThan => ">",
        Operator::LessThan => "<",
        Operator::Equal => "=",
        Operator::Between | Operator::NotBetween => "",
    }
}

/// A `calcext:condition` value as a rule kind (`dxf` = the applied style's format).
fn condition_kind(value: &str, dxf: u32) -> Option<CfKind> {
    let v = value.trim();
    if let Some(a) = call(v, "between") {
        let args = split_args(a);
        return Some(CfKind::CellIs {
            operator: Operator::Between,
            formulas: args.iter().map(|x| excel(x)).collect(),
            dxf,
        });
    }
    if let Some(a) = call(v, "not-between") {
        let args = split_args(a);
        return Some(CfKind::CellIs {
            operator: Operator::NotBetween,
            formulas: args.iter().map(|x| excel(x)).collect(),
            dxf,
        });
    }
    if let Some(a) = call(v, "formula-is") {
        // Blank tests as LibreOffice (and Kiln) write them: `LEN(TRIM([.O1]))=0`.
        let compact = a.replace(' ', "");
        if let Some(rest) = compact.strip_prefix("LEN(TRIM([")
            && let Some((cell, test)) = rest.split_once("]))")
            && !cell.contains([':', ';'])
        {
            match test {
                "=0" => return Some(CfKind::ContainsBlanks { dxf }),
                ">0" => return Some(CfKind::NotContainsBlanks { dxf }),
                _ => {}
            }
        }
        return Some(CfKind::Expression {
            formula: excel(a),
            dxf,
        });
    }
    for (name, make) in [
        ("begins-with", 0),
        ("ends-with", 1),
        ("contains-text", 2),
        ("not-contains-text", 3),
    ] {
        if let Some(a) = call(v, name) {
            let text = unquote_text(a);
            return Some(match make {
                0 => CfKind::BeginsWith { text, dxf },
                1 => CfKind::EndsWith { text, dxf },
                2 => CfKind::ContainsText { text, dxf },
                _ => CfKind::NotContainsText { text, dxf },
            });
        }
    }
    for (name, bottom, percent) in [
        ("top-elements", false, false),
        ("bottom-elements", true, false),
        ("top-percent", false, true),
        ("bottom-percent", true, true),
    ] {
        if let Some(a) = call(v, name) {
            return Some(CfKind::Top10 {
                rank: a.trim().parse().unwrap_or(10),
                percent,
                bottom,
                dxf,
            });
        }
    }
    Some(match v {
        "duplicate" => CfKind::DuplicateValues { dxf },
        "unique" => CfKind::UniqueValues { dxf },
        "above-average" | "above-equal-average" | "below-average" | "below-equal-average" => {
            CfKind::AboveAverage {
                above: v.starts_with("above"),
                equal: v.contains("equal"),
                std_dev: 0,
                dxf,
            }
        }
        "is-error" | "error" => CfKind::ContainsErrors { dxf },
        "is-no-error" | "no-error" => CfKind::NotContainsErrors { dxf },
        _ => {
            let (op, rest) = OPERATORS
                .iter()
                .find_map(|(p, op)| v.strip_prefix(p).map(|r| (*op, r)))?;
            CfKind::CellIs {
                operator: op,
                formulas: vec![excel(rest)],
                dxf,
            }
        }
    })
}

/// The `calcext:value` of a condition; `cell` is the first cell of the range (`[.O1]`), which
/// relative references are relative to.
fn condition_value(kind: &CfKind, cell: &str) -> Option<String> {
    Some(match kind {
        CfKind::CellIs {
            operator, formulas, ..
        } => match operator {
            Operator::Between | Operator::NotBetween => {
                let args: Vec<String> = formulas.iter().map(|f| odf(f)).collect();
                format!(
                    "{}({})",
                    if *operator == Operator::Between {
                        "between"
                    } else {
                        "not-between"
                    },
                    args.join(",")
                )
            }
            op => format!("{}{}", op_str(*op), odf(formulas.first()?)),
        },
        CfKind::Expression { formula, .. } => format!("formula-is({})", odf(formula)),
        CfKind::BeginsWith { text, .. } => format!("begins-with({})", quote_text(text)),
        CfKind::EndsWith { text, .. } => format!("ends-with({})", quote_text(text)),
        CfKind::ContainsText { text, .. } => format!("contains-text({})", quote_text(text)),
        CfKind::NotContainsText { text, .. } => format!("not-contains-text({})", quote_text(text)),
        CfKind::Top10 {
            rank,
            percent,
            bottom,
            ..
        } => format!(
            "{}-{}({rank})",
            if *bottom { "bottom" } else { "top" },
            if *percent { "percent" } else { "elements" }
        ),
        CfKind::DuplicateValues { .. } => "duplicate".to_owned(),
        CfKind::UniqueValues { .. } => "unique".to_owned(),
        CfKind::AboveAverage { above, equal, .. } => format!(
            "{}{}-average",
            if *above { "above" } else { "below" },
            if *equal { "-equal" } else { "" }
        ),
        CfKind::ContainsErrors { .. } => "is-error".to_owned(),
        CfKind::NotContainsErrors { .. } => "is-no-error".to_owned(),
        CfKind::ContainsBlanks { .. } => format!("formula-is(LEN(TRIM({cell}))=0)"),
        CfKind::NotContainsBlanks { .. } => format!("formula-is(LEN(TRIM({cell}))>0)"),
        CfKind::TimePeriod { .. }
        | CfKind::ColorScale { .. }
        | CfKind::DataBar(_)
        | CfKind::IconSet(_) => {
            return None;
        }
    })
}

const PERIODS: &[(&str, TimePeriod)] = &[
    ("today", TimePeriod::Today),
    ("yesterday", TimePeriod::Yesterday),
    ("tomorrow", TimePeriod::Tomorrow),
    ("last-7-days", TimePeriod::Last7Days),
    ("this-week", TimePeriod::ThisWeek),
    ("last-week", TimePeriod::LastWeek),
    ("next-week", TimePeriod::NextWeek),
    ("this-month", TimePeriod::ThisMonth),
    ("last-month", TimePeriod::LastMonth),
    ("next-month", TimePeriod::NextMonth),
];

const CFVO_TYPES: &[(&str, CfvoType)] = &[
    ("minimum", CfvoType::Min),
    ("maximum", CfvoType::Max),
    ("number", CfvoType::Num),
    ("percent", CfvoType::Percent),
    ("percentile", CfvoType::Percentile),
    ("formula", CfvoType::Formula),
    ("auto-minimum", CfvoType::AutoMin),
    ("auto-maximum", CfvoType::AutoMax),
];

const ICON_SETS: &[IconSetName] = &[
    IconSetName::Arrows3,
    IconSetName::ArrowsGray3,
    IconSetName::Flags3,
    IconSetName::TrafficLights3,
    IconSetName::TrafficLightsRimmed3,
    IconSetName::Signs3,
    IconSetName::Symbols3,
    IconSetName::SymbolsUncircled3,
    IconSetName::Stars3,
    IconSetName::Triangles3,
    IconSetName::Arrows4,
    IconSetName::ArrowsGray4,
    IconSetName::RedToBlack4,
    IconSetName::Rating4,
    IconSetName::TrafficLights4,
    IconSetName::Arrows5,
    IconSetName::ArrowsGray5,
    IconSetName::Rating5,
    IconSetName::Quarters5,
    IconSetName::Boxes5,
];

fn cfvo_of(e: &XmlElement) -> Cfvo {
    let kind = e
        .attr("calcext:type")
        .and_then(|t| CFVO_TYPES.iter().find(|(n, _)| *n == t))
        .map_or(CfvoType::Num, |(_, k)| *k);
    let value = e.attr("calcext:value").map(|v| match kind {
        CfvoType::Formula => excel(v),
        _ => v.to_owned(),
    });
    Cfvo {
        value: if matches!(
            kind,
            CfvoType::Min | CfvoType::Max | CfvoType::AutoMin | CfvoType::AutoMax
        ) {
            None
        } else {
            value
        },
        kind,
        // ODF entries have no "greater than or equal" flag: the threshold includes the value.
        gte: true,
    }
}

fn cfvo_el(name: &str, c: &Cfvo) -> El {
    let t = CFVO_TYPES
        .iter()
        .find(|(_, k)| *k == c.kind)
        .map_or("number", |(n, _)| *n);
    let value = match (&c.value, c.kind) {
        (Some(v), CfvoType::Formula) => odf(v),
        (Some(v), _) => v.clone(),
        (None, _) => "0".to_owned(),
    };
    El::new(name)
        .with("calcext:value", value)
        .with("calcext:type", t)
}

fn hex_color(s: Option<&str>) -> Option<Color> {
    s.and_then(background)
}

/// Reads one `calcext:conditional-format` (its rules get ids from `next_id`; `dxf_of` maps an
/// applied style name to a differential format).
pub fn read_cf(
    el: &XmlElement,
    ranges: Vec<Range>,
    dxf_of: &mut dyn FnMut(&str) -> u32,
    next_id: &mut u32,
    priority: &mut u32,
) -> Option<ConditionalFormat> {
    let mut rules = Vec::new();
    for c in el.elements() {
        let kind = match c.name.as_str() {
            "calcext:condition" => {
                let dxf = dxf_of(c.attr("calcext:apply-style-name").unwrap_or_default());
                condition_kind(c.attr("calcext:value").unwrap_or_default(), dxf)
            }
            "calcext:date-is" => {
                let dxf = dxf_of(c.attr("calcext:style").unwrap_or_default());
                c.attr("calcext:date")
                    .and_then(|d| PERIODS.iter().find(|(n, _)| *n == d))
                    .map(|(_, p)| CfKind::TimePeriod { period: *p, dxf })
            }
            "calcext:color-scale" => {
                let entries: Vec<&XmlElement> = kids(c, "calcext:color-scale-entry").collect();
                Some(CfKind::ColorScale {
                    cfvos: entries.iter().map(|e| cfvo_of(e)).collect(),
                    colors: entries
                        .iter()
                        .map(|e| hex_color(e.attr("calcext:color")).unwrap_or(Color::rgb(0)))
                        .collect(),
                })
            }
            "calcext:data-bar" => {
                let entries: Vec<&XmlElement> = kids(c, "calcext:formatting-entry").collect();
                let pick = |i: usize, k: CfvoType| {
                    entries.get(i).map_or_else(|| Cfvo::of(k), |e| cfvo_of(e))
                };
                Some(CfKind::DataBar(DataBar {
                    min: pick(0, CfvoType::Min),
                    max: pick(1, CfvoType::Max),
                    color: hex_color(c.attr("calcext:positive-color"))
                        .unwrap_or(Color::rgb(0x63_8ec6)),
                    show_value: c.attr("calcext:show-value") != Some("false"),
                    min_length: c
                        .attr("calcext:min-length")
                        .and_then(|v| v.parse::<f64>().ok())
                        .map_or(10, |v| v.clamp(0.0, 100.0) as u32),
                    max_length: c
                        .attr("calcext:max-length")
                        .and_then(|v| v.parse::<f64>().ok())
                        .map_or(90, |v| v.clamp(0.0, 100.0) as u32),
                    gradient: c.attr("calcext:gradient") != Some("false"),
                    border_color: None,
                    negative_color: hex_color(c.attr("calcext:negative-color")),
                    negative_border_color: None,
                    axis_color: hex_color(c.attr("calcext:axis-color")),
                    axis_position: match c.attr("calcext:axis-position") {
                        Some("middle") => AxisPosition::Middle,
                        Some("none") => AxisPosition::None,
                        _ => AxisPosition::Automatic,
                    },
                    direction: Default::default(),
                }))
            }
            "calcext:icon-set" => {
                let name = c.attr("calcext:icon-set-type").unwrap_or("3Arrows");
                let set = ICON_SETS.iter().copied().find(|s| s.as_str() == name)?;
                Some(CfKind::IconSet(IconSet {
                    icon_set: set,
                    cfvos: kids(c, "calcext:formatting-entry").map(cfvo_of).collect(),
                    reverse: false,
                    show_value: c.attr("calcext:show-value") != Some("false"),
                    custom_icons: Vec::new(),
                }))
            }
            _ => None,
        };
        if let Some(kind) = kind {
            *priority += 1;
            *next_id += 1;
            rules.push(CfRule {
                id: *next_id,
                priority: *priority,
                stop_if_true: false,
                kind,
            });
        }
    }
    (!rules.is_empty() && !ranges.is_empty()).then_some(ConditionalFormat { ranges, rules })
}

/// Writes one conditional format (`dxf_style` names the style of a differential format).
pub fn write_cf(
    w: &mut XmlOut,
    cf: &ConditionalFormat,
    target: &str,
    base: &str,
    dxf_style: &dyn Fn(u32) -> String,
) {
    let mut rules: Vec<&CfRule> = cf.rules.iter().collect();
    rules.sort_by_key(|r| r.priority);
    w.start("calcext:conditional-format")
        .attr("calcext:target-range-address", target);
    for r in rules {
        match &r.kind {
            CfKind::ColorScale { cfvos, colors } => {
                w.start("calcext:color-scale");
                for (c, col) in cfvos.iter().zip(colors) {
                    let mut e = cfvo_el("calcext:color-scale-entry", c);
                    e.set("calcext:color", color_hex(col, &Theme::default(), 0));
                    e.write(w);
                }
                w.end();
            }
            CfKind::DataBar(b) => {
                w.start("calcext:data-bar")
                    .attr(
                        "calcext:positive-color",
                        color_hex(&b.color, &Theme::default(), 0),
                    )
                    .attr("calcext:min-length", b.min_length.to_string())
                    .attr("calcext:max-length", b.max_length.to_string())
                    .attr("calcext:gradient", b.gradient.to_string())
                    .attr(
                        "calcext:axis-position",
                        match b.axis_position {
                            AxisPosition::Middle => "middle",
                            AxisPosition::None => "none",
                            AxisPosition::Automatic => "automatic",
                        },
                    );
                // Shown values are the default; LibreOffice writes the attribute only to hide them.
                if !b.show_value {
                    w.attr("calcext:show-value", "false");
                }
                if let Some(n) = &b.negative_color {
                    w.attr("calcext:negative-color", color_hex(n, &Theme::default(), 0));
                }
                if let Some(a) = &b.axis_color {
                    w.attr("calcext:axis-color", color_hex(a, &Theme::default(), 0));
                }
                cfvo_el("calcext:formatting-entry", &b.min).write(w);
                cfvo_el("calcext:formatting-entry", &b.max).write(w);
                w.end();
            }
            CfKind::IconSet(s) => {
                w.start("calcext:icon-set")
                    .attr("calcext:icon-set-type", s.icon_set.as_str());
                if !s.show_value {
                    w.attr("calcext:show-value", "false");
                }
                for c in &s.cfvos {
                    cfvo_el("calcext:formatting-entry", c).write(w);
                }
                w.end();
            }
            CfKind::TimePeriod { period, dxf } => {
                let d = PERIODS
                    .iter()
                    .find(|(_, p)| p == period)
                    .map_or("today", |(n, _)| *n);
                w.start("calcext:date-is")
                    .attr("calcext:date", d)
                    .attr("calcext:style", dxf_style(*dxf))
                    .end();
            }
            kind => {
                let cell = format!("[.{}]", base.rsplit('.').next().unwrap_or("A1"));
                let (Some(value), Some(dxf)) = (condition_value(kind, &cell), kind.dxf()) else {
                    continue;
                };
                w.start("calcext:condition")
                    .attr("calcext:apply-style-name", dxf_style(dxf))
                    .attr("calcext:value", value)
                    .attr("calcext:base-cell-address", base)
                    .end();
            }
        }
    }
    w.end();
}

// ------------------------------------------------------------------ differential formats

/// A differential format from a (conditional) named style: only what the style states.
#[must_use]
pub fn dxf_from_style(style: &XmlElement, fonts: &FontFaces, data_code: Option<String>) -> Dxf {
    let mut d = Dxf {
        num_fmt: data_code.map(|c| Arc::from(c.as_str())),
        ..Dxf::default()
    };
    if let Some(tp) = kid(style, "style:text-properties") {
        let mut f = FontPatch::default();
        if let Some(n) = tp.attr("style:font-name") {
            f.name = Some(Arc::from(fonts.family(n).as_str()));
        }
        if let Some(s) = tp.attr("fo:font-size").and_then(points_exact) {
            f.size = Some(F64(s));
        }
        if let Some(w) = tp.attr("fo:font-weight") {
            f.bold = Some(w == "bold" || w.parse::<u32>().is_ok_and(|v| v >= 600));
        }
        if let Some(s) = tp.attr("fo:font-style") {
            f.italic = Some(s == "italic" || s == "oblique");
        }
        if let Some(u) = tp.attr("style:text-underline-style") {
            f.underline = Some(if u == "none" {
                Underline::None
            } else if tp.attr("style:text-underline-type") == Some("double") {
                Underline::Double
            } else {
                Underline::Single
            });
        }
        if let Some(s) = tp.attr("style:text-line-through-style") {
            f.strike = Some(s != "none");
        }
        if let Some(c) = hex_color(tp.attr("fo:color")) {
            f.color = Some(c);
        }
        if f != FontPatch::default() {
            d.font = Some(f);
        }
    }
    if let Some(tc) = kid(style, "style:table-cell-properties") {
        if let Some(bg) = tc.attr("fo:background-color") {
            d.fill = Some(match background(bg) {
                Some(c) => Fill::solid(c),
                None => Fill::NONE,
            });
        }
        let mut probe = kiln_sheet::styles::CellStyle::default();
        let mut has_border = false;
        for a in [
            "fo:border",
            "fo:border-left",
            "fo:border-right",
            "fo:border-top",
            "fo:border-bottom",
        ] {
            has_border |= tc.attr(a).is_some();
        }
        if has_border {
            let mut only = XmlElement::new("style:style");
            only.children
                .push(kiln_ooxml::xml::XmlNode::Element(tc.clone()));
            crate::sheet::style::apply_style(&mut probe, &only, fonts);
            d.border = Some(probe.border.normalized());
        }
    }
    d
}

/// A differential format as a named cell style (`data_style` = its number format's data style).
#[must_use]
pub fn dxf_style_el(name: &str, d: &Dxf, data_style: Option<&str>, theme: &Theme) -> El {
    let mut st = El::new("style:style")
        .with("style:name", name)
        .with("style:family", "table-cell")
        .with("style:parent-style-name", "Default");
    if let Some(ds) = data_style {
        st.set("style:data-style-name", ds);
    }
    let mut tc = El::new("style:table-cell-properties");
    if let Some(fill) = &d.fill {
        match fill.main_color() {
            Some(c) => tc.set("fo:background-color", color_hex(&c, theme, 0xFF_FFFF)),
            None => tc.set("fo:background-color", "transparent"),
        }
    }
    if let Some(b) = &d.border {
        let full = kiln_sheet::styles::CellStyle {
            border: *b,
            ..kiln_sheet::styles::CellStyle::default()
        };
        let (cell, _) = crate::sheet::style::cell_props_els(&full, theme);
        for (k, v) in cell.attrs {
            if k.starts_with("fo:border") {
                tc.set(&k, v);
            }
        }
    }
    st.push_nonempty(tc);
    if let Some(f) = &d.font {
        let mut tp = El::new("style:text-properties");
        if let Some(n) = &f.name {
            tp.set("style:font-name", n.to_string());
        }
        if let Some(s) = f.size {
            tp.set("fo:font-size", crate::units::pt_str(s.0));
        }
        if let Some(b) = f.bold {
            tp.set("fo:font-weight", if b { "bold" } else { "normal" });
        }
        if let Some(i) = f.italic {
            tp.set("fo:font-style", if i { "italic" } else { "normal" });
        }
        if let Some(u) = f.underline {
            match u {
                Underline::None => tp.set("style:text-underline-style", "none"),
                Underline::Double | Underline::DoubleAccounting => {
                    tp.set("style:text-underline-style", "solid");
                    tp.set("style:text-underline-type", "double");
                }
                _ => tp.set("style:text-underline-style", "solid"),
            }
        }
        if let Some(s) = f.strike {
            tp.set(
                "style:text-line-through-style",
                if s { "solid" } else { "none" },
            );
        }
        if let Some(c) = &f.color {
            tp.set("fo:color", color_hex(c, theme, 0));
        }
        st.push_nonempty(tp);
    }
    st
}

// ------------------------------------------------------------------ validation

/// A `table:content-validation` as a rule without ranges (they come from the cells naming it).
#[must_use]
pub fn read_validation(el: &XmlElement, id: u32) -> DataValidation {
    let mut dv = DataValidation::new(id, Vec::new(), ValidationType::Any);
    let cond = el.attr("table:condition").unwrap_or_default();
    let cond = cond
        .strip_prefix("of:")
        .or_else(|| cond.strip_prefix("oooc:"))
        .unwrap_or(cond)
        .trim();
    // `type-test() and value-test`, or a value test alone.
    let (kind, test) = match cond.split_once(" and ") {
        Some((k, t)) => (k.trim(), Some(t.trim())),
        None => (cond, None),
    };
    let typed = |t: ValidationType, dv: &mut DataValidation| dv.kind = t;
    match kind {
        "cell-content-is-whole-number()" => typed(ValidationType::Whole, &mut dv),
        "cell-content-is-decimal-number()" => typed(ValidationType::Decimal, &mut dv),
        "cell-content-is-date()" => typed(ValidationType::Date, &mut dv),
        "cell-content-is-time()" => typed(ValidationType::Time, &mut dv),
        k => {
            if let Some(a) = call(k, "cell-content-is-in-list") {
                dv.kind = ValidationType::List;
                let items = split_odf_args(a);
                if items.len() == 1 && !items[0].trim_start().starts_with('"') {
                    dv.formula1 = Some(excel(&items[0]));
                } else {
                    let values: Vec<String> = items.iter().map(|i| unquote_text(i)).collect();
                    dv.formula1 = Some(quote_text(&values.join(",")));
                }
            } else if let Some(a) = call(k, "is-true-formula") {
                dv.kind = ValidationType::Custom;
                dv.formula1 = Some(excel(a));
            } else if k.starts_with("cell-content-text-length") {
                dv.kind = ValidationType::TextLength;
                read_test(k, "cell-content-text-length", &mut dv);
            } else {
                dv.kind = ValidationType::Custom;
                dv.formula1 = Some(excel(k));
            }
        }
    }
    if let Some(t) = test {
        read_test(t, "cell-content", &mut dv);
    }
    dv.allow_blank = el.attr("table:allow-empty-cell") != Some("false");
    dv.in_cell_dropdown = el.attr("table:display-list") != Some("none");
    if let Some(h) = kid(el, "table:help-message") {
        dv.show_input_message = h.attr("table:display") != Some("false");
        dv.prompt_title = h
            .attr("table:title")
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        let text: Vec<String> = kids(h, "text:p").map(all_text).collect();
        dv.prompt = Some(text.join("\n")).filter(|t| !t.is_empty());
    } else {
        dv.show_input_message = false;
    }
    if let Some(e) = kid(el, "table:error-message") {
        dv.show_error_message = e.attr("table:display") != Some("false");
        dv.error_style = match e.attr("table:message-type") {
            Some("warning") => ErrorStyle::Warning,
            Some("information") => ErrorStyle::Information,
            _ => ErrorStyle::Stop,
        };
        dv.error_title = e
            .attr("table:title")
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        let text: Vec<String> = kids(e, "text:p").map(all_text).collect();
        dv.error = Some(text.join("\n")).filter(|t| !t.is_empty());
    } else {
        dv.show_error_message = false;
    }
    dv
}

/// Splits OpenFormula arguments (`;`) outside quotes and brackets.
fn split_odf_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut dq = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '"' => dq = !dq,
            '(' | '[' | '{' if !dq => depth += 1,
            ')' | ']' | '}' if !dq => depth -= 1,
            ';' if depth == 0 && !dq => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur);
    }
    out
}

fn read_test(t: &str, subject: &str, dv: &mut DataValidation) {
    if let Some(a) = call(t, &format!("{subject}-is-between")) {
        let args = split_args(a);
        dv.operator = Operator::Between;
        dv.formula1 = args.first().map(|x| excel(x));
        dv.formula2 = args.get(1).map(|x| excel(x));
        return;
    }
    if let Some(a) = call(t, &format!("{subject}-is-not-between")) {
        let args = split_args(a);
        dv.operator = Operator::NotBetween;
        dv.formula1 = args.first().map(|x| excel(x));
        dv.formula2 = args.get(1).map(|x| excel(x));
        return;
    }
    if let Some(rest) = t.strip_prefix(&format!("{subject}()")) {
        let rest = rest.trim();
        if let Some((op, v)) = OPERATORS
            .iter()
            .find_map(|(p, op)| rest.strip_prefix(p).map(|r| (*op, r)))
        {
            dv.operator = op;
            dv.formula1 = Some(excel(v));
        }
    }
}

fn write_test(subject: &str, dv: &DataValidation) -> String {
    let f1 = dv
        .formula1
        .as_deref()
        .map(odf)
        .unwrap_or_else(|| "0".to_owned());
    let f2 = dv
        .formula2
        .as_deref()
        .map(odf)
        .unwrap_or_else(|| "0".to_owned());
    match dv.operator {
        Operator::Between => format!("{subject}-is-between({f1},{f2})"),
        Operator::NotBetween => format!("{subject}-is-not-between({f1},{f2})"),
        op => format!("{subject}(){}{f1}", op_str(op)),
    }
}

/// The `table:condition` of a rule (`None` for "any value").
#[must_use]
pub fn validation_condition(dv: &DataValidation) -> Option<String> {
    let typed = |t: &str| format!("of:{t} and {}", write_test("cell-content", dv));
    Some(match dv.kind {
        ValidationType::Any => return None,
        ValidationType::Whole => typed("cell-content-is-whole-number()"),
        ValidationType::Decimal => typed("cell-content-is-decimal-number()"),
        ValidationType::Date => typed("cell-content-is-date()"),
        ValidationType::Time => typed("cell-content-is-time()"),
        ValidationType::TextLength => format!("of:{}", write_test("cell-content-text-length", dv)),
        ValidationType::Custom => format!(
            "of:is-true-formula({})",
            dv.formula1.as_deref().map(odf).unwrap_or_default()
        ),
        ValidationType::List => {
            let f = dv.formula1.clone().unwrap_or_default();
            let t = f.trim();
            let inner = if let Some(list) = t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
                list.split(',')
                    .map(|v| quote_text(v.trim()))
                    .collect::<Vec<_>>()
                    .join(";")
            } else {
                odf(t)
            };
            format!("of:cell-content-is-in-list({inner})")
        }
    })
}

/// Writes a validation rule named `name` (`base` = its base cell address).
pub fn write_validation(w: &mut XmlOut, name: &str, dv: &DataValidation, base: &str) {
    w.start("table:content-validation").attr("table:name", name);
    if let Some(c) = validation_condition(dv) {
        w.attr("table:condition", c);
    }
    w.attr("table:allow-empty-cell", dv.allow_blank.to_string());
    if dv.kind == ValidationType::List {
        w.attr(
            "table:display-list",
            if dv.in_cell_dropdown {
                "unsorted"
            } else {
                "none"
            },
        );
    }
    w.attr("table:base-cell-address", base);
    w.start("table:help-message")
        .attr("table:title", dv.prompt_title.clone().unwrap_or_default())
        .attr("table:display", dv.show_input_message.to_string());
    if let Some(p) = &dv.prompt {
        for line in p.split('\n') {
            w.text_element("text:p", line);
        }
    }
    w.end();
    w.start("table:error-message")
        .attr(
            "table:message-type",
            match dv.error_style {
                ErrorStyle::Stop => "stop",
                ErrorStyle::Warning => "warning",
                ErrorStyle::Information => "information",
            },
        )
        .attr("table:title", dv.error_title.clone().unwrap_or_default())
        .attr("table:display", dv.show_error_message.to_string());
    if let Some(p) = &dv.error {
        for line in p.split('\n') {
            w.text_element("text:p", line);
        }
    }
    w.end();
    w.end();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_part;

    fn parse(xml: &str) -> XmlElement {
        parse_part(
            "content.xml",
            format!(
                "<r xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\">{xml}</r>"
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn conditions_round_trip() {
        for v in [
            ">50",
            "between(0,25)",
            "not-between(1,[.$B$1])",
            "formula-is(MOD(ROW();2)=0)",
            "top-elements(3)",
            "bottom-percent(10)",
            "above-average",
            "below-equal-average",
            "duplicate",
            "unique",
            "is-error",
            "begins-with(\"ab\")",
            "contains-text(\"x\"\"y\")",
            "!=\"no\"",
        ] {
            let k = condition_kind(v, 3).unwrap_or_else(|| panic!("{v}"));
            assert_eq!(condition_value(&k, "[.A1]").as_deref(), Some(v), "{v}");
        }
    }

    #[test]
    fn formats_read_and_write() {
        let root = parse(
            r##"<calcext:conditional-format calcext:target-range-address="CF.D1:CF.D20"><calcext:color-scale><calcext:color-scale-entry calcext:value="0" calcext:type="minimum" calcext:color="#f8696b"/><calcext:color-scale-entry calcext:value="50" calcext:type="percentile" calcext:color="#ffeb84"/><calcext:color-scale-entry calcext:value="0" calcext:type="maximum" calcext:color="#63be7b"/></calcext:color-scale><calcext:condition calcext:apply-style-name="S1" calcext:value="&gt;5" calcext:base-cell-address="CF.D1"/><calcext:icon-set calcext:icon-set-type="3TrafficLights1"><calcext:formatting-entry calcext:value="0" calcext:type="percent"/><calcext:formatting-entry calcext:value="33" calcext:type="percent"/><calcext:formatting-entry calcext:value="67" calcext:type="percent"/></calcext:icon-set><calcext:date-is calcext:date="last-week" calcext:style="S1"/></calcext:conditional-format>"##,
        );
        let el = root.elements().next().unwrap();
        let (mut id, mut prio) = (0, 0);
        let cf = read_cf(
            el,
            vec![Range::new(0, 3, 19, 3)],
            &mut |_| 7,
            &mut id,
            &mut prio,
        )
        .unwrap();
        assert_eq!(cf.rules.len(), 4);
        let mut w = XmlOut::fragment();
        write_cf(&mut w, &cf, "CF.D1:CF.D20", "CF.D1", &|_| "S1".to_owned());
        let again = parse(&w.into_string());
        let (mut id2, mut prio2) = (0, 0);
        let cf2 = read_cf(
            again.elements().next().unwrap(),
            cf.ranges.clone(),
            &mut |_| 7,
            &mut id2,
            &mut prio2,
        )
        .unwrap();
        assert_eq!(cf, cf2);
    }

    #[test]
    fn validations_round_trip() {
        let root = parse(
            r#"<table:content-validation table:name="val1" table:condition="of:cell-content-is-in-list(&quot;Red&quot;;&quot;Green&quot;)" table:allow-empty-cell="true" table:display-list="unsorted" table:base-cell-address="V.B1"><table:help-message table:title="List" table:display="true"><text:p>Pick</text:p></table:help-message><table:error-message table:message-type="warning" table:title="Bad" table:display="true"><text:p>No</text:p></table:error-message></table:content-validation><table:content-validation table:name="val3" table:condition="of:cell-content-is-whole-number() and cell-content-is-between(1,10)" table:allow-empty-cell="false" table:base-cell-address="V.B3"/><table:content-validation table:name="val4" table:condition="of:cell-content-is-decimal-number() and cell-content()&gt;0.5" table:base-cell-address="V.B4"/><table:content-validation table:name="v5" table:condition="of:cell-content-is-in-list([.$H$1:.$H$4])" table:base-cell-address="V.B4"/><table:content-validation table:name="v6" table:condition="of:cell-content-text-length()&lt;=5" table:base-cell-address="V.B4"/>"#,
        );
        let all: Vec<DataValidation> = root.elements().map(|e| read_validation(e, 1)).collect();
        assert_eq!(all[0].kind, ValidationType::List);
        assert_eq!(all[0].formula1.as_deref(), Some("\"Red,Green\""));
        assert_eq!(all[0].error_style, ErrorStyle::Warning);
        assert_eq!(all[1].kind, ValidationType::Whole);
        assert_eq!(
            (all[1].formula1.as_deref(), all[1].formula2.as_deref()),
            (Some("1"), Some("10"))
        );
        assert_eq!(all[2].operator, Operator::GreaterThan);
        assert_eq!(all[3].formula1.as_deref(), Some("$H$1:$H$4"));
        assert_eq!(all[4].kind, ValidationType::TextLength);
        for dv in &all {
            let mut w = XmlOut::fragment();
            write_validation(&mut w, "v", dv, "V.A1");
            let back = parse(&w.into_string());
            assert_eq!(&read_validation(back.elements().next().unwrap(), 1), dv);
        }
    }
}
