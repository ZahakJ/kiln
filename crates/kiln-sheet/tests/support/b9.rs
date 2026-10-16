// SPDX-License-Identifier: AGPL-3.0-or-later
//! The synthetic workbook of budgets B8 and B9 (PLAN.md §3.9), shared by
//! `examples/edit_bench.rs` (which times it) and `tests/incremental.rs` (which checks exactly
//! which formulas an edit recalculates), so the test covers the shape the budget measures.
//!
//! A `Model` sheet with an input in column A and nine formulas per row in B..J — arithmetic,
//! IF/AND/OR, a row SUM, an INDEX lookup into a 1,000-row `Data` table, and a sliding 10-row SUM
//! window over column G — so `rows` rows hold `9 × rows` formulas.

use std::sync::Arc;

use kiln_sheet::commands::Command;
use kiln_sheet::meta::CalcMode;
use kiln_sheet::{CellValue, SheetId, Workbook, WorkbookOptions};

/// Formula columns B..J for row `r` (1-based).
fn row_formulas(r: u32) -> [String; 9] {
    let window = if r >= 10 {
        format!("SUM(G{}:G{r})", r - 9)
    } else {
        format!("SUM(G1:G{r})")
    };
    [
        format!("A{r}*2+1"),
        format!("IF(B{r}>1000,B{r}-1000,B{r}+0.5)"),
        format!("SUM(A{r}:C{r})"),
        format!("INDEX(Data!$B$1:$B$1000,A{r}+1)"),
        format!("E{r}*C{r}-D{r}"),
        format!("IF(AND(F{r}>0,E{r}<500),F{r},-F{r})"),
        window,
        format!("H{r}/2+D{r}"),
        format!("IF(OR(I{r}<0,G{r}>100),1,0)*I{r}"),
    ]
}

/// Builds the workbook with `rows` model rows, loaded as a file reader does (no recalculation
/// per cell); switching back to automatic calculation at the end computes every formula once.
pub fn build(rows: u32) -> (Workbook, SheetId) {
    let mut wb = Workbook::with_options(WorkbookOptions {
        sheets: Some(vec!["Model".into(), "Data".into()]),
        clock: Some(Arc::new(|| 45306.5)),
        ..WorkbookOptions::default()
    });
    let (model, data) = (wb.sheets()[0].id(), wb.sheets()[1].id());
    let mut props = wb.props().clone();
    // Build without recalculating after every cell, as a file reader does.
    props.calc.mode = CalcMode::Manual;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .expect("props");
    for i in 0..1000u32 {
        wb.load_value(data, i, 0, CellValue::Number(f64::from(i)))
            .expect("data");
        wb.load_value(
            data,
            i,
            1,
            CellValue::Number(f64::from((i * 7919) % 1000) / 4.0),
        )
        .expect("data");
    }
    for r in 1..=rows {
        wb.load_value(
            model,
            r - 1,
            0,
            CellValue::Number(f64::from((r * 37) % 1000)),
        )
        .expect("input");
        for (i, f) in row_formulas(r).iter().enumerate() {
            wb.load_formula(model, r - 1, 1 + i as u32, f, CellValue::Blank, false)
                .expect("formula");
        }
    }
    let mut props = wb.props().clone();
    props.calc.mode = CalcMode::Auto;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .expect("props");
    (wb, model)
}
