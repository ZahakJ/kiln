// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional formatting (Home ▸ Styles ▸ Conditional Formatting): Highlight Cells Rules,
//! Top/Bottom Rules, Data Bars, Colour Scales, Icon Sets, New Rule, Manage Rules (order, Stop
//! If True, Applies To, edit, delete) and Clear Rules, as rules of the sheet model that the
//! painter draws and the XLSX writer saves (x14 data bar options included).

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{
    GridCfRule, GridCfRuleKind, GridCfStyle, GridCfValue, GridCfValueKind, GridCompare,
    GridDatePeriod, GridTextRule,
};
use kiln_sheet::address::{format_sqref, parse_sqref};
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{
    AxisPosition, BarDirection, CfKind, CfRule, Cfvo, CfvoType, ConditionalFormat, DataBar,
    IconSet, IconSetName, Operator, TimePeriod,
};
use kiln_sheet::styles::{Border, BorderSide, BorderStyle, Dxf, Fill, FontPatch, Underline};
use kiln_sheet::{Command, Range, SheetId};

use super::super::GridDoc;
use super::super::format::{hex, parse_color};
use crate::model::Change;

pub(crate) fn operator(op: GridCompare) -> Operator {
    match op {
        GridCompare::Between => Operator::Between,
        GridCompare::NotBetween => Operator::NotBetween,
        GridCompare::Equal => Operator::Equal,
        GridCompare::NotEqual => Operator::NotEqual,
        GridCompare::GreaterThan => Operator::GreaterThan,
        GridCompare::LessThan => Operator::LessThan,
        GridCompare::GreaterThanOrEqual => Operator::GreaterThanOrEqual,
        GridCompare::LessThanOrEqual => Operator::LessThanOrEqual,
    }
}

pub(crate) fn compare(op: Operator) -> GridCompare {
    match op {
        Operator::Between => GridCompare::Between,
        Operator::NotBetween => GridCompare::NotBetween,
        Operator::Equal => GridCompare::Equal,
        Operator::NotEqual => GridCompare::NotEqual,
        Operator::GreaterThan => GridCompare::GreaterThan,
        Operator::LessThan => GridCompare::LessThan,
        Operator::GreaterThanOrEqual => GridCompare::GreaterThanOrEqual,
        Operator::LessThanOrEqual => GridCompare::LessThanOrEqual,
    }
}

fn period(p: GridDatePeriod) -> TimePeriod {
    use GridDatePeriod as G;
    match p {
        G::Today => TimePeriod::Today,
        G::Yesterday => TimePeriod::Yesterday,
        G::Tomorrow => TimePeriod::Tomorrow,
        G::Last7Days => TimePeriod::Last7Days,
        G::ThisWeek => TimePeriod::ThisWeek,
        G::LastWeek => TimePeriod::LastWeek,
        G::NextWeek => TimePeriod::NextWeek,
        G::ThisMonth => TimePeriod::ThisMonth,
        G::LastMonth => TimePeriod::LastMonth,
        G::NextMonth => TimePeriod::NextMonth,
    }
}

fn grid_period(p: TimePeriod) -> GridDatePeriod {
    use GridDatePeriod as G;
    match p {
        TimePeriod::Today => G::Today,
        TimePeriod::Yesterday => G::Yesterday,
        TimePeriod::Tomorrow => G::Tomorrow,
        TimePeriod::Last7Days => G::Last7Days,
        TimePeriod::ThisWeek => G::ThisWeek,
        TimePeriod::LastWeek => G::LastWeek,
        TimePeriod::NextWeek => G::NextWeek,
        TimePeriod::ThisMonth => G::ThisMonth,
        TimePeriod::LastMonth => G::LastMonth,
        TimePeriod::NextMonth => G::NextMonth,
    }
}

/// A value as typed in a rule dialog as the formula the rule stores: `=B1` → `B1`, a number
/// stays, other text is quoted.
pub(crate) fn operand(v: &str) -> String {
    let v = v.trim();
    if let Some(f) = v.strip_prefix('=') {
        return f.to_owned();
    }
    if v.parse::<f64>().is_ok() || (v.starts_with('"') && v.ends_with('"') && v.len() >= 2) {
        return v.to_owned();
    }
    if v.eq_ignore_ascii_case("TRUE") || v.eq_ignore_ascii_case("FALSE") {
        return v.to_ascii_uppercase();
    }
    format!("\"{}\"", v.replace('"', "\"\""))
}

/// A stored operand as the dialog shows it.
fn shown_operand(f: &str) -> String {
    if f.parse::<f64>().is_ok() {
        f.to_owned()
    } else if f.len() >= 2 && f.starts_with('"') && f.ends_with('"') {
        f[1..f.len() - 1].replace("\"\"", "\"")
    } else {
        format!("={f}")
    }
}

fn cfvo(v: &GridCfValue) -> Result<Cfvo> {
    let kind = match v.kind {
        GridCfValueKind::Min => CfvoType::Min,
        GridCfValueKind::Max => CfvoType::Max,
        GridCfValueKind::Number => CfvoType::Num,
        GridCfValueKind::Percent => CfvoType::Percent,
        GridCfValueKind::Percentile => CfvoType::Percentile,
        GridCfValueKind::Formula => CfvoType::Formula,
        GridCfValueKind::Automatic => CfvoType::AutoMin,
    };
    let value = match kind {
        CfvoType::Min | CfvoType::Max | CfvoType::AutoMin | CfvoType::AutoMax => None,
        CfvoType::Formula => Some(v.value.trim().trim_start_matches('=').to_owned()),
        _ => {
            let t = v.value.trim();
            if t.parse::<f64>().is_err() {
                return Err(KilnError::InvalidInput(format!("\"{t}\" is not a number")));
            }
            Some(t.to_owned())
        }
    };
    Ok(Cfvo {
        kind,
        value,
        gte: v.gte,
    })
}

fn grid_cfvo(c: &Cfvo) -> GridCfValue {
    let kind = match c.kind {
        CfvoType::Min => GridCfValueKind::Min,
        CfvoType::Max => GridCfValueKind::Max,
        CfvoType::Num => GridCfValueKind::Number,
        CfvoType::Percent => GridCfValueKind::Percent,
        CfvoType::Percentile => GridCfValueKind::Percentile,
        CfvoType::Formula => GridCfValueKind::Formula,
        CfvoType::AutoMin | CfvoType::AutoMax => GridCfValueKind::Automatic,
    };
    GridCfValue {
        kind,
        value: match c.kind {
            CfvoType::Formula => format!("={}", c.value.clone().unwrap_or_default()),
            _ => c.value.clone().unwrap_or_default(),
        },
        gte: c.gte,
    }
}

fn color(hexs: &str) -> Result<kiln_sheet::Color> {
    parse_color(hexs).ok_or_else(|| KilnError::InvalidInput(format!("not a colour: {hexs}")))
}

/// The differential format of a highlight style.
fn dxf_of(s: &GridCfStyle) -> Result<Dxf> {
    let font = FontPatch {
        color: s.font_color.as_deref().map(color).transpose()?,
        bold: s.bold.then_some(true),
        italic: s.italic.then_some(true),
        underline: s.underline.then_some(Underline::Single),
        strike: s.strike.then_some(true),
        ..FontPatch::default()
    };
    let side = s
        .border
        .as_deref()
        .map(color)
        .transpose()?
        .map(|c| BorderSide {
            style: BorderStyle::Thin,
            color: Some(c),
        });
    Ok(Dxf {
        num_fmt: s
            .number_format
            .as_deref()
            .filter(|f| !f.is_empty())
            .map(Into::into),
        font: (font != FontPatch::default()).then_some(font),
        fill: s.fill.as_deref().map(color).transpose()?.map(Fill::solid),
        border: side.map(|b| Border {
            left: Some(b),
            right: Some(b),
            top: Some(b),
            bottom: Some(b),
            ..Border::default()
        }),
        ..Dxf::default()
    })
}

impl GridDoc {
    fn style_of(&self, dxf: u32) -> GridCfStyle {
        let theme = self.workbook.theme();
        let Some(d) = self.workbook.styles().dxf(dxf) else {
            return GridCfStyle::default();
        };
        let font = d.font.clone().unwrap_or_default();
        GridCfStyle {
            fill: d
                .fill
                .as_ref()
                .and_then(|f| hex(f.main_color(), theme, 0xFF_FFFF)),
            font_color: hex(font.color, theme, 0),
            bold: font.bold == Some(true),
            italic: font.italic == Some(true),
            underline: font.underline.is_some_and(|u| u != Underline::None),
            strike: font.strike == Some(true),
            border: d
                .border
                .as_ref()
                .and_then(|b| b.bottom.as_ref())
                .and_then(|s| hex(s.color, theme, 0)),
            number_format: d.num_fmt.as_ref().map(ToString::to_string),
        }
    }

    /// A rule of the dialogs as the model's (`dxf` interned).
    fn model_kind(&mut self, rule: &GridCfRule) -> Result<CfKind> {
        let mut dxf = || -> Result<u32> {
            let style = rule.style.clone().unwrap_or(GridCfStyle {
                fill: Some("FFC7CE".into()),
                font_color: Some("9C0006".into()),
                ..GridCfStyle::default()
            });
            let d = dxf_of(&style)?;
            Ok(self.log.intern_dxf(&mut self.workbook, d))
        };
        Ok(match &rule.kind {
            GridCfRuleKind::CellValue { op, values } => {
                let need = if matches!(op, GridCompare::Between | GridCompare::NotBetween) {
                    2
                } else {
                    1
                };
                if values.len() < need || values.iter().take(need).any(|v| v.trim().is_empty()) {
                    return Err(KilnError::InvalidInput("enter a value for the rule".into()));
                }
                CfKind::CellIs {
                    operator: operator(*op),
                    formulas: values.iter().take(need).map(|v| operand(v)).collect(),
                    dxf: dxf()?,
                }
            }
            GridCfRuleKind::Text { rule: r, text } => {
                let dxf = dxf()?;
                let text = text.clone();
                match r {
                    GridTextRule::Contains => CfKind::ContainsText { text, dxf },
                    GridTextRule::NotContains => CfKind::NotContainsText { text, dxf },
                    GridTextRule::BeginsWith => CfKind::BeginsWith { text, dxf },
                    GridTextRule::EndsWith => CfKind::EndsWith { text, dxf },
                }
            }
            GridCfRuleKind::DateOccurring { period: p } => CfKind::TimePeriod {
                period: period(*p),
                dxf: dxf()?,
            },
            GridCfRuleKind::Duplicates { unique } => {
                let dxf = dxf()?;
                if *unique {
                    CfKind::UniqueValues { dxf }
                } else {
                    CfKind::DuplicateValues { dxf }
                }
            }
            GridCfRuleKind::TopBottom {
                bottom,
                percent,
                rank,
            } => {
                if *rank == 0 || (*percent && *rank > 100) || *rank > 1000 {
                    return Err(KilnError::InvalidInput(
                        "enter a rank from 1 to 1,000 (1 to 100 percent)".into(),
                    ));
                }
                CfKind::Top10 {
                    rank: *rank,
                    percent: *percent,
                    bottom: *bottom,
                    dxf: dxf()?,
                }
            }
            GridCfRuleKind::Average {
                below,
                equal,
                std_dev,
            } => CfKind::AboveAverage {
                above: !*below,
                equal: *equal,
                std_dev: (*std_dev).min(3),
                dxf: dxf()?,
            },
            GridCfRuleKind::Blanks { not } => {
                let dxf = dxf()?;
                if *not {
                    CfKind::NotContainsBlanks { dxf }
                } else {
                    CfKind::ContainsBlanks { dxf }
                }
            }
            GridCfRuleKind::Errors { not } => {
                let dxf = dxf()?;
                if *not {
                    CfKind::NotContainsErrors { dxf }
                } else {
                    CfKind::ContainsErrors { dxf }
                }
            }
            GridCfRuleKind::Formula { formula } => {
                let f = formula.trim().trim_start_matches('=').to_owned();
                if f.is_empty() {
                    return Err(KilnError::InvalidInput(
                        "enter a formula for the rule".into(),
                    ));
                }
                kiln_formula::parser::parse(
                    &format!("={f}"),
                    &kiln_formula::parser::ParseOptions::default(),
                )
                .map_err(|e| KilnError::InvalidInput(format!("the formula has a problem: {e}")))?;
                CfKind::Expression {
                    formula: f,
                    dxf: dxf()?,
                }
            }
            GridCfRuleKind::ColorScale { points, colors } => {
                if !(2..=3).contains(&points.len()) || points.len() != colors.len() {
                    return Err(KilnError::InvalidInput(
                        "a colour scale has two or three points, each with a colour".into(),
                    ));
                }
                CfKind::ColorScale {
                    cfvos: points.iter().map(cfvo).collect::<Result<_>>()?,
                    colors: colors.iter().map(|c| color(c)).collect::<Result<_>>()?,
                }
            }
            GridCfRuleKind::DataBar {
                min,
                max,
                color: c,
                gradient,
                show_value,
                negative_color,
                border_color,
            } => {
                let mut lo = cfvo(min)?;
                let mut hi = cfvo(max)?;
                if hi.kind == CfvoType::AutoMin {
                    hi.kind = CfvoType::AutoMax;
                }
                if lo.kind == CfvoType::AutoMax {
                    lo.kind = CfvoType::AutoMin;
                }
                CfKind::DataBar(DataBar {
                    min: lo,
                    max: hi,
                    color: color(c)?,
                    show_value: *show_value,
                    min_length: 0,
                    max_length: 100,
                    gradient: *gradient,
                    border_color: border_color.as_deref().map(color).transpose()?,
                    negative_color: Some(
                        negative_color
                            .as_deref()
                            .map(color)
                            .transpose()?
                            .unwrap_or_else(|| kiln_sheet::Color::rgb(0xFF_0000)),
                    ),
                    negative_border_color: None,
                    axis_color: Some(kiln_sheet::Color::rgb(0x00_0000)),
                    axis_position: AxisPosition::default(),
                    direction: BarDirection::default(),
                })
            }
            GridCfRuleKind::IconSet {
                set,
                thresholds,
                reverse,
                show_value,
            } => {
                let name: IconSetName =
                    serde_json::from_value(serde_json::Value::String(set.clone()))
                        .map_err(|_| KilnError::InvalidInput(format!("unknown icon set {set}")))?;
                let n = name.len();
                if n == 0 || thresholds.len() + 1 != n {
                    return Err(KilnError::InvalidInput(format!(
                        "the {set} icon set needs {} thresholds",
                        n.saturating_sub(1)
                    )));
                }
                let mut cfvos = vec![Cfvo::with(CfvoType::Percent, "0")];
                for t in thresholds {
                    cfvos.push(cfvo(t)?);
                }
                CfKind::IconSet(IconSet {
                    icon_set: name,
                    cfvos,
                    reverse: *reverse,
                    show_value: *show_value,
                    custom_icons: Vec::new(),
                })
            }
        })
    }

    fn grid_rule(&self, block: &ConditionalFormat, r: &CfRule) -> GridCfRule {
        let style = r.kind.dxf().map(|d| self.style_of(d));
        let kind = match &r.kind {
            CfKind::CellIs {
                operator: op,
                formulas,
                ..
            } => GridCfRuleKind::CellValue {
                op: compare(*op),
                values: formulas.iter().map(|f| shown_operand(f)).collect(),
            },
            CfKind::Expression { formula, .. } => GridCfRuleKind::Formula {
                formula: format!("={formula}"),
            },
            CfKind::ContainsText { text, .. } => GridCfRuleKind::Text {
                rule: GridTextRule::Contains,
                text: text.clone(),
            },
            CfKind::NotContainsText { text, .. } => GridCfRuleKind::Text {
                rule: GridTextRule::NotContains,
                text: text.clone(),
            },
            CfKind::BeginsWith { text, .. } => GridCfRuleKind::Text {
                rule: GridTextRule::BeginsWith,
                text: text.clone(),
            },
            CfKind::EndsWith { text, .. } => GridCfRuleKind::Text {
                rule: GridTextRule::EndsWith,
                text: text.clone(),
            },
            CfKind::ContainsBlanks { .. } => GridCfRuleKind::Blanks { not: false },
            CfKind::NotContainsBlanks { .. } => GridCfRuleKind::Blanks { not: true },
            CfKind::ContainsErrors { .. } => GridCfRuleKind::Errors { not: false },
            CfKind::NotContainsErrors { .. } => GridCfRuleKind::Errors { not: true },
            CfKind::TimePeriod { period: p, .. } => GridCfRuleKind::DateOccurring {
                period: grid_period(*p),
            },
            CfKind::DuplicateValues { .. } => GridCfRuleKind::Duplicates { unique: false },
            CfKind::UniqueValues { .. } => GridCfRuleKind::Duplicates { unique: true },
            CfKind::Top10 {
                rank,
                percent,
                bottom,
                ..
            } => GridCfRuleKind::TopBottom {
                bottom: *bottom,
                percent: *percent,
                rank: *rank,
            },
            CfKind::AboveAverage {
                above,
                equal,
                std_dev,
                ..
            } => GridCfRuleKind::Average {
                below: !*above,
                equal: *equal,
                std_dev: *std_dev,
            },
            CfKind::ColorScale { cfvos, colors } => {
                let theme = self.workbook.theme();
                GridCfRuleKind::ColorScale {
                    points: cfvos.iter().map(grid_cfvo).collect(),
                    colors: colors
                        .iter()
                        .map(|c| hex(Some(*c), theme, 0).unwrap_or_else(|| "000000".into()))
                        .collect(),
                }
            }
            CfKind::DataBar(b) => {
                let theme = self.workbook.theme();
                GridCfRuleKind::DataBar {
                    min: grid_cfvo(&b.min),
                    max: grid_cfvo(&b.max),
                    color: hex(Some(b.color), theme, 0).unwrap_or_else(|| "638EC6".into()),
                    gradient: b.gradient,
                    show_value: b.show_value,
                    negative_color: hex(b.negative_color, theme, 0),
                    border_color: hex(b.border_color, theme, 0),
                }
            }
            CfKind::IconSet(s) => GridCfRuleKind::IconSet {
                set: s.icon_set.as_str().to_owned(),
                thresholds: s.cfvos.iter().skip(1).map(grid_cfvo).collect(),
                reverse: s.reverse,
                show_value: s.show_value,
            },
        };
        GridCfRule {
            id: r.id,
            applies_to: absolute_sqref(&block.ranges),
            kind,
            style,
            stop_if_true: r.stop_if_true,
        }
    }

    /// The rules of the sheet (or touching the selection), highest priority first.
    pub(crate) fn cf_rules(&self, whole_sheet: bool) -> Result<Vec<GridCfRule>> {
        let sid = self.sid()?;
        let sel = self.view().sel.ranges;
        let Some(sheet) = self.workbook.sheet(sid) else {
            return Ok(Vec::new());
        };
        let mut rules: Vec<(u32, GridCfRule)> = Vec::new();
        for block in &sheet.meta().cf {
            if !whole_sheet
                && !block
                    .ranges
                    .iter()
                    .any(|r| sel.iter().any(|s| s.intersects(r)))
            {
                continue;
            }
            for r in &block.rules {
                rules.push((r.priority, self.grid_rule(block, r)));
            }
        }
        rules.sort_by_key(|(p, _)| *p);
        Ok(rules.into_iter().map(|(_, r)| r).collect())
    }

    fn ranges_of(&self, applies_to: &str) -> Result<Vec<Range>> {
        let text = applies_to.trim().trim_start_matches('=');
        if text.is_empty() {
            return Ok(self
                .view()
                .sel
                .ranges
                .iter()
                .map(|r| self.clip_to_used(*r))
                .collect());
        }
        let cleaned: String = text
            .split([',', ' '])
            .filter(|p| !p.is_empty())
            .map(|p| match p.rfind('!') {
                Some(i) => &p[i + 1..],
                None => p,
            })
            .collect::<Vec<_>>()
            .join(" ");
        parse_sqref(&cleaned).map_err(|_| {
            KilnError::InvalidInput(format!("\"{applies_to}\" is not a range on this sheet"))
        })
    }

    fn set_cf(
        &mut self,
        label: &'static str,
        sid: SheetId,
        cf: Vec<ConditionalFormat>,
    ) -> Result<()> {
        self.exec(
            label,
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Cf(cf)),
            },
        )?;
        Ok(())
    }

    pub(crate) fn cf_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        let current = self
            .workbook
            .sheet(sid)
            .map(|s| s.meta().cf.clone())
            .unwrap_or_default();
        match cmd {
            GridCommand::AddConditionalFormat { rule } => {
                let ranges = self.ranges_of(&rule.applies_to)?;
                let kind = self.model_kind(rule)?;
                let id = self.new_rule_id();
                // A new rule goes on top.
                let mut cf = current;
                for b in &mut cf {
                    for r in &mut b.rules {
                        r.priority += 1;
                    }
                }
                cf.push(ConditionalFormat {
                    ranges,
                    rules: vec![CfRule {
                        id,
                        priority: 1,
                        stop_if_true: rule.stop_if_true,
                        kind,
                    }],
                });
                self.set_cf("grid.styles.add_rule", sid, cf)?;
            }
            GridCommand::SetConditionalFormats { rules, sheet } => {
                let sel = self.view().sel.ranges;
                // Rules outside the dialog's scope stay, after the edited ones.
                let mut kept: Vec<(u32, ConditionalFormat)> = Vec::new();
                for b in &current {
                    if *sheet || !b.ranges.iter().any(|r| sel.iter().any(|s| s.intersects(r))) {
                        if *sheet {
                            continue;
                        }
                        for r in &b.rules {
                            kept.push((
                                r.priority,
                                ConditionalFormat {
                                    ranges: b.ranges.clone(),
                                    rules: vec![r.clone()],
                                },
                            ));
                        }
                    }
                }
                kept.sort_by_key(|(p, _)| *p);
                let mut cf = Vec::new();
                let mut priority = 1;
                for g in rules {
                    let ranges = self.ranges_of(&g.applies_to)?;
                    let kind = self.model_kind(g)?;
                    let id = if g.id != 0 { g.id } else { self.new_rule_id() };
                    cf.push(ConditionalFormat {
                        ranges,
                        rules: vec![CfRule {
                            id,
                            priority,
                            stop_if_true: g.stop_if_true,
                            kind,
                        }],
                    });
                    priority += 1;
                }
                for (_, mut b) in kept {
                    for r in &mut b.rules {
                        r.priority = priority;
                        priority += 1;
                    }
                    cf.push(b);
                }
                self.set_cf("grid.styles.set_rules", sid, cf)?;
            }
            GridCommand::ClearConditionalFormats { sheet } => {
                let cf = if *sheet {
                    Vec::new()
                } else {
                    let sel: Vec<Range> = self.view().sel.ranges;
                    current
                        .into_iter()
                        .filter_map(|mut b| {
                            let mut ranges = b.ranges.clone();
                            for s in &sel {
                                ranges = ranges.iter().flat_map(|r| subtract(*r, *s)).collect();
                            }
                            b.ranges = ranges;
                            (!b.ranges.is_empty()).then_some(b)
                        })
                        .collect()
                };
                self.set_cf("grid.styles.clear_rules", sid, cf)?;
            }
            _ => {}
        }
        Ok(Change::Content)
    }
}

/// `$A$1:$A$9 $C$2` for the Manage Rules dialog.
fn absolute_sqref(ranges: &[Range]) -> String {
    let _ = format_sqref;
    ranges
        .iter()
        .map(|r| {
            if r.is_single_cell() {
                kiln_sheet::address::cell_a1(r.r0, r.c0, true)
            } else {
                r.to_a1(true)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `r` without `hole` (up to four ranges).
pub(crate) fn subtract(r: Range, hole: Range) -> Vec<Range> {
    let Some(h) = r.intersection(&hole) else {
        return vec![r];
    };
    let mut out = Vec::new();
    if h.r0 > r.r0 {
        out.push(Range::new(r.r0, r.c0, h.r0 - 1, r.c1));
    }
    if h.r1 < r.r1 {
        out.push(Range::new(h.r1 + 1, r.c0, r.r1, r.c1));
    }
    if h.c0 > r.c0 {
        out.push(Range::new(h.r0, r.c0, h.r1, h.c0 - 1));
    }
    if h.c1 < r.c1 {
        out.push(Range::new(h.r0, h.c1 + 1, h.r1, r.c1));
    }
    out
}
