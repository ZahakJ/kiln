// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data validation: whether a value may go into a cell (whole numbers, decimals, lists, dates,
//! times, text lengths and custom formulas, with blanks ignored when allowed), the in-cell
//! dropdown items, the input message, and Circle Invalid Data.

use std::cmp::Ordering;

use kiln_formula::SheetId;
use kiln_formula::refs::copy_formula;
use kiln_formula::values::{CoercionOptions, Value, compare_text, parse_date_time_text};

use crate::address::{CellPos, Range};
use crate::meta::{DataValidation, ErrorStyle, Operator, ValidationType};
use crate::store::CellKind;
use crate::value::CellValue;
use crate::workbook::Workbook;

/// The outcome of validating a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validity {
    /// Accepted.
    Valid,
    /// Refused or questioned, with the alert to show.
    Invalid {
        /// Stop, warning or information.
        style: ErrorStyle,
        /// Alert title.
        title: String,
        /// Alert message.
        message: String,
    },
}

impl Validity {
    /// True for [`Validity::Valid`].
    pub fn is_valid(&self) -> bool {
        matches!(self, Validity::Valid)
    }
}

/// Items of a literal list source (`"Yes,No"`, with or without the quotes).
pub fn literal_list(source: &str) -> Option<Vec<String>> {
    let t = source.trim();
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    Some(
        inner
            .replace("\"\"", "\"")
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect(),
    )
}

fn compare_num(op: Operator, x: f64, a: f64, b: Option<f64>) -> bool {
    match op {
        Operator::Between | Operator::NotBetween => {
            let Some(b) = b else { return false };
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            (x >= lo && x <= hi) == (op == Operator::Between)
        }
        Operator::Equal => x == a,
        Operator::NotEqual => x != a,
        Operator::GreaterThan => x > a,
        Operator::LessThan => x < a,
        Operator::GreaterThanOrEqual => x >= a,
        Operator::LessThanOrEqual => x <= a,
    }
}

fn flatten(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => (0..a.rows())
            .flat_map(|r| (0..a.cols()).map(move |c| (r, c)))
            .map(|(r, c)| a.get(r, c).clone())
            .collect(),
        other => vec![other.clone()],
    }
}

impl Workbook {
    /// Evaluates a validation criterion written for the rule's first cell, as if at `at`.
    fn criterion(
        &mut self,
        rule: &DataValidation,
        formula: &str,
        sheet: SheetId,
        at: CellPos,
    ) -> Value {
        let anchor = rule.ranges.first().map_or(at, |r| CellPos::new(r.r0, r.c0));
        let dr = i64::from(at.row) - i64::from(anchor.row);
        let dc = i64::from(at.col) - i64::from(anchor.col);
        let text = if dr == 0 && dc == 0 {
            formula.to_owned()
        } else {
            copy_formula(formula, dr, dc).unwrap_or_else(|_| formula.to_owned())
        };
        let v = match self.evaluate_at(&text, sheet, at.row, at.col) {
            Ok(v) => v,
            Err(_) => return Value::Error(kiln_formula::values::ErrorValue::Name),
        };
        // Dates and times typed as text in the dialog (`1/1/2024`, `9:00`).
        if let Value::Str(s) = &v
            && matches!(rule.kind, ValidationType::Date | ValidationType::Time)
        {
            let opts = CoercionOptions {
                date1904: self.props.date1904,
                ..CoercionOptions::default()
            };
            if let Some(n) = parse_date_time_text(s, &opts) {
                return Value::Number(n);
            }
        }
        v
    }

    fn criterion_number(
        &mut self,
        rule: &DataValidation,
        f: Option<&str>,
        sheet: SheetId,
        at: CellPos,
    ) -> Option<f64> {
        let f = f?;
        match self.criterion(rule, f, sheet, at) {
            Value::Number(n) => Some(n),
            Value::Bool(b) => Some(f64::from(u8::from(b))),
            Value::Array(a) => a.get(0, 0).as_number(),
            _ => None,
        }
    }

    /// The sheet and area of a list source that is a single reference (`$A$1:$A$9`,
    /// `Lists!$B:$B`), adjusted for the cell's offset from the rule's first cell.
    fn reference_area(
        &self,
        rule: &DataValidation,
        source: &str,
        sheet: SheetId,
        at: CellPos,
    ) -> Option<(SheetId, Range)> {
        let anchor = rule.ranges.first().map_or(at, |r| CellPos::new(r.r0, r.c0));
        let text = copy_formula(
            source,
            i64::from(at.row) - i64::from(anchor.row),
            i64::from(at.col) - i64::from(anchor.col),
        )
        .ok()?;
        let ast = kiln_formula::parse(
            &format!("={}", crate::workbook::strip_equals(&text)),
            &Default::default(),
        )
        .ok()?;
        let kiln_formula::parser::NodeKind::Ref { prefix, address } = &ast.kind else {
            return None;
        };
        let target = match prefix {
            None => sheet,
            Some(p) if p.workbook.is_none() && p.sheet2.is_none() => {
                self.sheet_by_name(&p.sheet)?.id()
            }
            Some(_) => return None,
        };
        let b = kiln_formula::refs::address_bounds(address);
        // Whole columns stop at the used area.
        let used = self.sheet(target)?.cells.extent();
        let r1 = if b.r1 == crate::address::MAX_ROWS - 1 {
            b.r1.min(used.0.saturating_sub(1))
        } else {
            b.r1
        };
        Some((target, Range::new(b.r0, b.c0, r1.max(b.r0), b.c1)))
    }

    /// The dropdown items of a list rule applying to a cell.
    pub fn validation_list(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> Option<Vec<CellValue>> {
        let rule = self.sheet(sheet)?.validation_at(row, col)?.clone();
        if rule.kind != ValidationType::List {
            return None;
        }
        let source = rule.formula1.clone()?;
        if let Some(items) = literal_list(&source) {
            return Some(items.into_iter().map(CellValue::from).collect());
        }
        // A plain reference reads its cells directly (blank cells are not items).
        if let Some((target, area)) =
            self.reference_area(&rule, &source, sheet, CellPos::new(row, col))
        {
            let mut items = Vec::new();
            for r in area.r0..=area.r1.min(area.r0.saturating_add(100_000)) {
                for c in area.c0..=area.c1 {
                    let v = self.value(target, r, c);
                    if !v.is_empty_text_or_blank() {
                        items.push(v);
                    }
                }
            }
            return Some(items);
        }
        let v = self.criterion(&rule, &source, sheet, CellPos::new(row, col));
        Some(
            flatten(&v)
                .iter()
                .map(CellValue::from_value)
                .filter(|x| !x.is_blank())
                .collect(),
        )
    }

    /// The input message of the rule applying to a cell (title, text).
    pub fn validation_prompt(
        &self,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> Option<(String, String)> {
        let r = self.sheet(sheet)?.validation_at(row, col)?;
        if !r.show_input_message || (r.prompt.is_none() && r.prompt_title.is_none()) {
            return None;
        }
        Some((
            r.prompt_title.clone().unwrap_or_default(),
            r.prompt.clone().unwrap_or_default(),
        ))
    }

    /// Whether `value` may be entered at (row, col).
    pub fn validate(&mut self, sheet: SheetId, row: u32, col: u32, value: &CellValue) -> Validity {
        let Some(rule) = self
            .sheet(sheet)
            .and_then(|s| s.validation_at(row, col))
            .cloned()
        else {
            return Validity::Valid;
        };
        if self.passes(&rule, sheet, CellPos::new(row, col), value) {
            return Validity::Valid;
        }
        if !rule.show_error_message {
            return Validity::Valid;
        }
        Validity::Invalid {
            style: rule.error_style,
            title: rule
                .error_title
                .clone()
                .unwrap_or_else(|| "Kiln Grid".into()),
            message: rule.error.clone().unwrap_or_else(|| {
                "This value doesn't match the data validation restrictions defined for this cell."
                    .into()
            }),
        }
    }

    fn passes(
        &mut self,
        rule: &DataValidation,
        sheet: SheetId,
        at: CellPos,
        value: &CellValue,
    ) -> bool {
        if value.is_empty_text_or_blank() {
            return rule.allow_blank || rule.kind == ValidationType::Any;
        }
        let (f1, f2) = (rule.formula1.clone(), rule.formula2.clone());
        match rule.kind {
            ValidationType::Any => true,
            ValidationType::Whole
            | ValidationType::Decimal
            | ValidationType::Date
            | ValidationType::Time => {
                let Some(x) = value.as_number() else {
                    return false;
                };
                if rule.kind == ValidationType::Whole && x.fract() != 0.0 {
                    return false;
                }
                let x = if rule.kind == ValidationType::Time {
                    x.fract()
                } else {
                    x
                };
                let Some(a) = self.criterion_number(rule, f1.as_deref(), sheet, at) else {
                    return false;
                };
                let b = self.criterion_number(rule, f2.as_deref(), sheet, at);
                let (a, b) = if rule.kind == ValidationType::Time {
                    (a.fract(), b.map(f64::fract))
                } else {
                    (a, b)
                };
                compare_num(rule.operator, x, a, b)
            }
            ValidationType::TextLength => {
                let len = value.display_general().chars().count() as f64;
                let Some(a) = self.criterion_number(rule, f1.as_deref(), sheet, at) else {
                    return false;
                };
                let b = self.criterion_number(rule, f2.as_deref(), sheet, at);
                compare_num(rule.operator, len, a, b)
            }
            ValidationType::List => {
                let Some(items) = self.validation_list(sheet, at.row, at.col) else {
                    return false;
                };
                items.iter().any(|it| match (it, value) {
                    (CellValue::Number(a), CellValue::Number(b)) => a == b,
                    _ => {
                        compare_text(&it.display_general(), &value.display_general())
                            == Ordering::Equal
                    }
                })
            }
            ValidationType::Custom => {
                let Some(f) = f1 else { return false };
                // The formula sees the proposed value in the cell.
                let saved = self
                    .sheet(sheet)
                    .and_then(|s| s.cells.get(at.row, at.col))
                    .map(|d| (d.value.clone(), d.kind, d.style));
                if let Some(s) = self.host_mut().get_mut(sheet) {
                    let style = saved.as_ref().map_or(0, |x| x.2);
                    s.cells
                        .set(at.row, at.col, value.clone(), CellKind::Value, style);
                }
                let v = self.criterion(rule, &f, sheet, at);
                if let Some(s) = self.host_mut().get_mut(sheet) {
                    match &saved {
                        Some((v0, k, st)) => s.cells.set(at.row, at.col, v0.clone(), *k, *st),
                        None => {
                            s.cells.remove(at.row, at.col);
                        }
                    }
                }
                matches!(v, Value::Bool(true)) || v.as_number().is_some_and(|n| n != 0.0)
            }
        }
    }

    /// Circle Invalid Data: the cells in `area` whose current value breaks their rule.
    pub fn invalid_cells(&mut self, sheet: SheetId, area: Range) -> Vec<CellPos> {
        let Some(s) = self.sheet(sheet) else {
            return Vec::new();
        };
        let rules: Vec<DataValidation> = s.meta.validations.clone();
        let mut out = Vec::new();
        for rule in rules.iter().filter(|r| r.kind != ValidationType::Any) {
            for r in &rule.ranges {
                let Some(x) = r.intersection(&area) else {
                    continue;
                };
                let cells: Vec<(u32, u32)> = if x.cell_count() <= 100_000 {
                    x.cells().map(|p| (p.row, p.col)).collect()
                } else {
                    self.sheet(sheet)
                        .map(|s| s.cells.iter_range(x).map(|(a, b, _)| (a, b)).collect())
                        .unwrap_or_default()
                };
                for (row, col) in cells {
                    // Only the rule that applies to the cell counts (the last listed).
                    if self
                        .sheet(sheet)
                        .and_then(|s| s.validation_at(row, col))
                        .map(|v| v.id)
                        != Some(rule.id)
                    {
                        continue;
                    }
                    let v = self.value(sheet, row, col);
                    if v.is_blank() {
                        continue;
                    }
                    if !self.passes(rule, sheet, CellPos::new(row, col), &v) {
                        out.push(CellPos::new(row, col));
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literal_lists() {
        assert_eq!(
            literal_list("\"Yes, No ,Maybe\""),
            Some(vec!["Yes".into(), "No".into(), "Maybe".into()])
        );
        assert_eq!(literal_list("$A$1:$A$3"), None);
    }

    #[test]
    fn compares_with_operators() {
        assert!(compare_num(Operator::Between, 5.0, 1.0, Some(10.0)));
        assert!(compare_num(Operator::Between, 5.0, 10.0, Some(1.0)));
        assert!(!compare_num(Operator::NotBetween, 5.0, 1.0, Some(10.0)));
        assert!(compare_num(Operator::GreaterThanOrEqual, 5.0, 5.0, None));
        assert!(!compare_num(Operator::Between, 5.0, 1.0, None));
    }
}
