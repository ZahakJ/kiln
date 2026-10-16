// SPDX-License-Identifier: AGPL-3.0-or-later
//! Incremental recalculation through the workbook: in the 100,008-formula generated workbook (the
//! shape `examples/edit_bench.rs` times for budget B9), editing one input recalculates exactly
//! the formulas that depend on it.

use std::sync::Arc;

use kiln_sheet::commands::{CellInput, Command};
use kiln_sheet::meta::CalcMode;
use kiln_sheet::{CellValue, Workbook, WorkbookOptions};

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

#[test]
fn a_single_edit_recalculates_only_its_dependents() {
    const ROWS: u32 = 11_112;
    let mut wb = Workbook::with_options(WorkbookOptions {
        sheets: Some(vec!["Model".into(), "Data".into()]),
        clock: Some(Arc::new(|| 45306.5)),
        ..WorkbookOptions::default()
    });
    let (model, data) = (wb.sheets()[0].id(), wb.sheets()[1].id());
    let mut props = wb.props().clone();
    props.calc.mode = CalcMode::Manual;
    wb.apply(&Command::SetProps {
        props: Box::new(props.clone()),
    })
    .expect("manual");
    for i in 0..1000u32 {
        wb.load_value(data, i, 1, CellValue::Number(f64::from(i % 50)))
            .expect("data");
    }
    for r in 1..=ROWS {
        wb.load_value(model, r - 1, 0, CellValue::Number(f64::from(r % 900)))
            .expect("input");
        for (i, f) in row_formulas(r).iter().enumerate() {
            wb.load_formula(model, r - 1, 1 + i as u32, f, CellValue::Blank, false)
                .expect("formula");
        }
    }
    assert_eq!(wb.engine().formula_count(), 100_008);
    let full = wb.recalc_all();
    assert_eq!(full.evaluated, 100_008);
    props.calc.mode = CalcMode::Auto;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .expect("auto");

    // Editing A5000 dirties B..G of its row (6), and H, I, J of the ten rows whose 10-row
    // window covers G5000 (30): 36 evaluations, not 100,008.
    let before_j = wb.value(model, 5003, 9);
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(4999, 0, 123.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 36);
    assert_eq!(wb.value(model, 4999, 1), CellValue::Number(247.0));
    assert_ne!(wb.value(model, 5003, 9), CellValue::Blank);
    let _ = before_j;
    // An edit nothing depends on evaluates nothing.
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(4999, 20, 1.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 0);
    // The first row's window is shorter: H1…H10 depend on G1.
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(0, 0, 5.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 36);
    // Results equal a full recalculation.
    let snapshot: Vec<CellValue> = (0..ROWS)
        .step_by(997)
        .map(|r| wb.value(model, r, 9))
        .collect();
    wb.recalc_all();
    let again: Vec<CellValue> = (0..ROWS)
        .step_by(997)
        .map(|r| wb.value(model, r, 9))
        .collect();
    assert_eq!(snapshot, again);
}
