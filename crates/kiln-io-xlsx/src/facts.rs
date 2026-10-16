// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canonical descriptions of a workbook for the round-trip fidelity scorer
//! (`kiln-testing::formats::xlsx`): model facts (one string per sheet property, styled cell,
//! row or column, merge, rule, name…) and cell values with their formulas.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use kiln_sheet::{CellValue, Workbook};

use crate::text::{cell_ref, col_letters, fmt_num};

fn digest(text: &str) -> String {
    let mut h = rustc_hash::FxHasher::default();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A cell value as canonical text: numbers in shortest round-trip form, `TRUE`/`FALSE`, error
/// codes, plain text (rich text by its characters).
#[must_use]
pub fn value_text(v: &CellValue) -> String {
    match v {
        CellValue::Blank => String::new(),
        CellValue::Number(n) => fmt_num(*n),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        CellValue::Error(e) => e.code().to_owned(),
        CellValue::Text(s) => s.to_string(),
        CellValue::RichText(r) => r.plain().to_string(),
    }
}

/// Every cell with content, keyed `Sheet!A1`: (value text, formula without `=`). Values spilled
/// by dynamic arrays are included.
#[must_use]
pub fn snapshot_cells(wb: &Workbook) -> BTreeMap<String, (String, Option<String>)> {
    let mut out = BTreeMap::new();
    for s in wb.sheets() {
        let id = s.id();
        for (r, c, d) in s.cells().iter() {
            let formula = if d.kind.is_formula() {
                wb.formula(id, r, c)
            } else {
                None
            };
            let value = wb.value(id, r, c);
            if formula.is_none() && value.is_blank() {
                continue;
            }
            out.insert(
                format!("{}!{}", s.name(), cell_ref(r, c)),
                (value_text(&value), formula),
            );
            if d.kind.is_engine_formula()
                && let Some(sp) = wb.spill_range(id, r, c)
                && !sp.blocked
            {
                let b = sp.bounds;
                if u64::from(b.r1 - b.r0 + 1) * u64::from(b.c1 - b.c0 + 1) <= 100_000 {
                    for rr in b.r0..=b.r1 {
                        for cc in b.c0..=b.c1 {
                            if (rr, cc) == (r, c) {
                                continue;
                            }
                            let v = wb.value(id, rr, cc);
                            if !v.is_blank() {
                                out.insert(
                                    format!("{}!{}", s.name(), cell_ref(rr, cc)),
                                    (value_text(&v), None),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Model facts: what a faithful round trip must keep, one string per item. Rule and validation
/// ids (allocated while reading) are left out; formats are described by content, not index.
#[must_use]
pub fn snapshot_facts(wb: &Workbook) -> Vec<String> {
    let mut f = Vec::new();
    let props = wb.props();
    f.push(format!("props/date1904={}", props.date1904));
    f.push(format!("props/calc={:?}", props.calc));
    f.push(format!("props/active={}", props.active_sheet));
    f.push(format!("props/code_name={:?}", props.code_name));
    f.push(format!("protection={:?}", wb.protection()));
    f.push(format!("theme={:?}", wb.theme()));
    for p in wb.persons() {
        f.push(format!("person={p:?}"));
    }
    for n in wb.names() {
        let scope = n
            .scope
            .and_then(|id| wb.sheet(id))
            .map_or_else(String::new, |s| s.name().to_owned());
        f.push(format!(
            "name/{scope}!{}={}|hidden={}|{:?}",
            n.name, n.formula, n.hidden, n.comment
        ));
    }
    let styles = wb.styles();
    let style_digest = |i: u32| digest(&format!("{:?}", styles.get(i)));
    for d in 0..styles.dxf_count() {
        if let Some(x) = styles.dxf(d as u32) {
            f.push(format!("dxf[{d}]={x:?}"));
        }
    }
    for t in styles.table_styles() {
        f.push(format!("table_style={t:?}"));
    }
    for (i, s) in wb.sheets().iter().enumerate() {
        let name = s.name();
        let m = s.meta();
        f.push(format!(
            "sheet[{i}]={name}|{:?}|{:?}",
            m.visibility, m.tab_color
        ));
        f.push(format!(
            "{name}/sizes={}|{:?}|{}|{}",
            m.default_row_height, m.default_col_width, m.summary_below, m.summary_right
        ));
        f.push(format!("{name}/view={:?}", m.view));
        f.push(format!("{name}/print={:?}", m.print));
        f.push(format!("{name}/protection={:?}", m.protection));
        f.push(format!("{name}/autofilter={:?}", m.auto_filter));
        for r in &m.merges {
            f.push(format!("{name}/merge={}", r.to_a1(false)));
        }
        for cf in &m.cf {
            let ranges: Vec<String> = cf.ranges.iter().map(|r| r.to_a1(false)).collect();
            for rule in &cf.rules {
                f.push(format!(
                    "{name}/cf[{}]={}|{}|{:?}",
                    ranges.join(" "),
                    rule.priority,
                    rule.stop_if_true,
                    rule.kind
                ));
            }
        }
        for v in &m.validations {
            let mut v = v.clone();
            v.id = 0;
            f.push(format!("{name}/dv={v:?}"));
        }
        for h in &m.hyperlinks {
            f.push(format!("{name}/link={h:?}"));
        }
        for n in &m.notes {
            f.push(format!("{name}/note={n:?}"));
        }
        for t in &m.threads {
            f.push(format!("{name}/thread={t:?}"));
        }
        for t in &m.tables {
            f.push(format!("{name}/table={t:?}"));
        }
        for d in &m.drawings {
            f.push(format!("{name}/drawing={d:?}"));
        }
        for (r, p) in s.rows().to_vec() {
            f.push(format!("{name}/row{}={p:?}", r + 1));
        }
        for (c, p) in s.cols().to_vec() {
            f.push(format!("{name}/col{}={p:?}", col_letters(c)));
        }
        for (r, c, d) in s.cells().iter() {
            if d.style != 0 {
                f.push(format!(
                    "{name}!{}@style={}",
                    cell_ref(r, c),
                    style_digest(d.style)
                ));
            }
            if let CellValue::RichText(rt) = d.value {
                f.push(format!("{name}!{}@rich={:?}", cell_ref(r, c), rt.runs()));
            }
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_canonical() {
        assert_eq!(value_text(&CellValue::Number(0.1)), "0.1");
        assert_eq!(value_text(&CellValue::Bool(true)), "TRUE");
        assert_eq!(
            value_text(&CellValue::Error(kiln_formula::ErrorValue::NA)),
            "#N/A"
        );
        let wb = Workbook::new();
        assert!(snapshot_cells(&wb).is_empty());
        assert!(
            snapshot_facts(&wb)
                .iter()
                .any(|x| x.starts_with("sheet[0]=Sheet1"))
        );
    }
}
