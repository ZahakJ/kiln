// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands and the command log: validation, exact undo/redo, transactions, serialisation.

mod support;

use kiln_formula::values::ErrorValue;
use kiln_sheet::address::{MAX_COLS, MAX_ROWS};
use kiln_sheet::command_log::CommandLog;
use kiln_sheet::commands::{
    CellInput, ClearWhat, ColPatch, Content, FormatFrom, MergeMode, MetaField, PasteWhat, RowPatch,
    SortKey,
};
use kiln_sheet::meta::{
    CalcMode, CfKind, CfRule, ConditionalFormat, DataValidation, Hyperlink, Note, SheetVisibility,
    Table, TableColumn, ValidationType,
};
use kiln_sheet::num::F64;
use kiln_sheet::styles::{FontPatch, HAlign, Patch, StylePatch};
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId, Workbook};
use support::{Fixture, setup, state};

fn grid(wb: &Workbook, s: SheetId, r: Range) -> Vec<Vec<String>> {
    (r.r0..=r.r1)
        .map(|row| {
            (r.c0..=r.c1)
                .map(|col| match wb.formula(s, row, col) {
                    Some(f) => format!("={f}"),
                    None => wb
                        .sheet(s)
                        .map(|x| x.cells().value(row, col).display_general())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .collect()
}

fn g(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|x| (*x).to_owned()).collect())
        .collect()
}

fn row(values: &[CellValue]) -> Vec<Option<CellValue>> {
    values
        .iter()
        .map(|v| (!v.is_blank()).then(|| v.clone()))
        .collect()
}

fn n(x: f64) -> CellValue {
    CellValue::Number(x)
}

fn t(s: &str) -> CellValue {
    CellValue::text(s)
}

fn set_range(f: &mut Fixture, s: SheetId, row0: u32, col: u32, rows: &[&[CellValue]]) {
    f.exec(Command::SetRange {
        sheet: s,
        row: row0,
        col,
        values: rows.iter().map(|r| row(r)).collect(),
    });
}

fn bold(wb: &mut Workbook) -> u32 {
    wb.styles_mut().patch(
        0,
        &StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        },
    )
}

fn table(name: &str, range: Range, cols: &[&str]) -> Box<Table> {
    Box::new(Table {
        id: 0,
        name: name.into(),
        display_name: None,
        range,
        header_row: true,
        totals_row: false,
        columns: cols.iter().map(|c| TableColumn::named(*c)).collect(),
        style: None,
        auto_filter: None,
        comment: None,
    })
}

// ── setCells and setRange ─────────────────────────────────────────────────

#[test]
fn sets_values_formulas_and_styles_with_exact_undo() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(0, 0, 1), CellInput::formula(0, 1, "=A1+1")],
    });
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 0, 1)),
        g(&[&["1", "=A1+1"]])
    );
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(0, 1, "text")],
    });
    assert_eq!(f.formula(s, "B1"), None);
    f.undo();
    assert_eq!(f.formula(s, "B1").as_deref(), Some("A1+1"));
    f.undo();
    assert_eq!(state(&f.wb), before);
}

#[test]
fn rejects_invalid_formulas_styles_and_positions_without_changing_anything() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    assert!(
        f.try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(0, 0, 1), CellInput::formula(0, 1, "=SUM(")],
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput {
                row: 0,
                col: 0,
                content: Content::Keep,
                style: Some(99),
            }],
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(MAX_ROWS, 0, 1)],
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::SetCells {
            sheet: SheetId(999),
            cells: Vec::new(),
        })
        .is_err()
    );
    assert_eq!(state(&f.wb), before);
    assert!(!f.log.can_undo());
}

#[test]
fn gives_new_cells_the_format_of_their_row_or_column() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cols(2, 2)],
        patch: Box::new(StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        }),
    });
    f.set(s, "C51", "x");
    let st = f.wb.sheet(s).map_or(0, |x| x.cells().style(50, 2));
    assert!(f.wb.styles().font_of(st).bold);
}

#[test]
fn writes_2d_blocks() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(
        &mut f,
        s,
        1,
        1,
        &[
            &[n(1.0), n(2.0)],
            &[n(3.0), CellValue::Error(ErrorValue::NA)],
        ],
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(1, 1, 2, 2)),
        g(&[&["1", "2"], &["3", "#N/A"]])
    );
    f.undo();
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().len()), Some(0));
}

// ── clearRange ────────────────────────────────────────────────────────────

#[test]
fn clears_each_kind_and_undoes_exactly() {
    for what in [
        ClearWhat::All,
        ClearWhat::Contents,
        ClearWhat::Formats,
        ClearWhat::Notes,
        ClearWhat::Hyperlinks,
    ] {
        let mut f = setup(&["Sheet1"]);
        let s = f.id(0);
        let italic = f.wb.styles_mut().patch(
            0,
            &StylePatch {
                font: Some(FontPatch {
                    italic: Some(true),
                    ..FontPatch::default()
                }),
                ..StylePatch::default()
            },
        );
        f.exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput {
                row: 0,
                col: 0,
                content: Content::Value(n(5.0)),
                style: Some(italic),
            }],
        });
        f.exec(Command::Merge {
            sheet: s,
            range: Range::new(2, 0, 2, 1),
            mode: MergeMode::Merge,
        });
        f.exec(Command::SetSheetMeta {
            sheet: s,
            field: Box::new(MetaField::Notes(vec![Note {
                at: CellPos::new(0, 0),
                author: "A".into(),
                text: "n".into(),
                visible: false,
                size: None,
            }])),
        });
        f.exec(Command::SetSheetMeta {
            sheet: s,
            field: Box::new(MetaField::Hyperlinks(vec![Hyperlink {
                range: Range::cell(0, 0),
                target: Some("https://x".into()),
                location: None,
                display: None,
                tooltip: None,
            }])),
        });
        let before = state(&f.wb);
        f.exec(Command::ClearRange {
            sheet: s,
            ranges: vec![Range::new(0, 0, 5, 5)],
            what,
        });
        let sheet = f.wb.sheet(s).expect("sheet");
        let c = sheet.cells().get(0, 0);
        match what {
            ClearWhat::All => {
                assert!(c.is_none());
                assert!(sheet.meta().notes.is_empty());
                assert!(sheet.merges().is_empty());
                assert!(sheet.meta().hyperlinks.is_empty());
            }
            ClearWhat::Contents => {
                assert!(c.is_some_and(|c| c.value.is_blank() && c.style == italic));
            }
            ClearWhat::Formats => {
                assert!(c.is_some_and(|c| c.style == 0 && *c.value == n(5.0)));
                assert!(sheet.merges().is_empty());
            }
            ClearWhat::Notes => assert!(sheet.meta().notes.is_empty()),
            ClearWhat::Hyperlinks => assert!(sheet.meta().hyperlinks.is_empty()),
        }
        f.undo();
        assert_eq!(state(&f.wb), before, "{what:?}");
    }
}

// ── setStyle ──────────────────────────────────────────────────────────────

fn fill_yellow() -> Box<StylePatch> {
    Box::new(StylePatch {
        fill: Patch::Set(kiln_sheet::styles::Fill::solid(kiln_sheet::Color::rgb(
            0xFFFF00,
        ))),
        ..StylePatch::default()
    })
}

#[test]
fn formats_cells_and_reuses_style_indices() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::new(0, 0, 9, 9)],
        patch: fill_yellow(),
    });
    let sheet = f.wb.sheet(s).expect("sheet");
    assert_eq!(sheet.cells().len(), 100);
    let styles: std::collections::BTreeSet<u32> =
        sheet.cells().iter().map(|(_, _, d)| d.style).collect();
    assert_eq!(styles.len(), 1);
    assert_eq!(f.wb.styles().len(), 2);
    f.undo();
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().len()), Some(0));
}

#[test]
fn formats_whole_columns_through_column_properties() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "B101", 1);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cols(1, 1)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0.00".into()),
            ..StylePatch::default()
        }),
    });
    let sheet = f.wb.sheet(s).expect("sheet");
    assert_eq!(sheet.cells().len(), 1);
    let col_style = sheet.cols().get(1).and_then(|p| p.style).unwrap_or(0);
    assert_eq!(f.wb.styles().num_fmt(col_style), "0.00");
    assert_eq!(f.wb.styles().num_fmt(sheet.cells().style(100, 1)), "0.00");
    assert_eq!(f.wb.styles().num_fmt(sheet.style_at(5000, 1)), "0.00");
    f.undo();
    let sheet = f.wb.sheet(s).expect("sheet");
    assert!(sheet.cols().get(1).is_none());
    assert_eq!(sheet.cells().style(100, 1), 0);
}

#[test]
fn formats_whole_rows_and_their_intersections_with_formatted_columns() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cols(3, 3)],
        patch: Box::new(StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        }),
    });
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::rows(4, 4)],
        patch: Box::new(StylePatch {
            font: Some(FontPatch {
                italic: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        }),
    });
    let sheet = f.wb.sheet(s).expect("sheet");
    let cross = f.wb.styles().font_of(sheet.style_at(4, 3));
    assert!(cross.bold && cross.italic);
    assert!(f.wb.styles().font_of(sheet.style_at(4, 100)).italic);
}

#[test]
fn applies_a_style_index() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let st = f.wb.styles_mut().patch(
        0,
        &StylePatch {
            num_fmt: Some("0%".into()),
            ..StylePatch::default()
        },
    );
    f.exec(Command::SetStyleIndex {
        sheet: s,
        ranges: vec![Range::new(0, 0, 1, 0)],
        style: st,
    });
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().style(1, 0)), Some(st));
    assert!(
        f.try_exec(Command::SetStyleIndex {
            sheet: s,
            ranges: vec![Range::cell(0, 0)],
            style: 1234,
        })
        .is_err()
    );
}

// ── structural edits ──────────────────────────────────────────────────────

#[test]
fn inserts_rows_carrying_the_format_of_the_row_above() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let b = bold(&mut f.wb);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput {
                row: 2,
                col: 0,
                content: Content::Value(t("h")),
                style: Some(b),
            },
            CellInput::value(3, 0, "x"),
        ],
    });
    f.exec(Command::InsertRows {
        sheet: s,
        at: 3,
        count: 2,
        format_from: FormatFrom::Before,
    });
    let sheet = f.wb.sheet(s).expect("sheet");
    assert_eq!(sheet.cells().style(3, 0), b);
    assert_eq!(sheet.cells().style(4, 0), b);
    assert_eq!(sheet.cells().value(5, 0), &t("x"));
    f.exec(Command::InsertRows {
        sheet: s,
        at: 0,
        count: 1,
        format_from: FormatFrom::None,
    });
    assert!(f.wb.cell(s, 0, 0).is_none());
}

#[test]
fn refuses_inserts_that_would_push_data_off_the_sheet() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(MAX_ROWS - 1, 0, "end")],
    });
    let e = f
        .try_exec(Command::InsertRows {
            sheet: s,
            at: 0,
            count: 1,
            format_from: FormatFrom::Before,
        })
        .expect_err("refused");
    assert!(e.to_string().contains("off the end"));
    assert!(
        f.try_exec(Command::DeleteRows {
            sheet: s,
            at: MAX_ROWS - 1,
            count: 2,
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::InsertCols {
            sheet: s,
            at: 0,
            count: 0,
            format_from: FormatFrom::Before,
        })
        .is_err()
    );
    // Columns: a value in XFD blocks inserting columns.
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(0, MAX_COLS - 1, "end")],
    });
    assert!(
        f.try_exec(Command::InsertCols {
            sheet: s,
            at: 0,
            count: 1,
            format_from: FormatFrom::Before,
        })
        .is_err()
    );
}

#[test]
fn refuses_to_delete_a_table_header_row_and_leaves_everything_unchanged() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::AddTable {
        sheet: s,
        table: table("Tbl", Range::new(2, 0, 5, 1), &["A", "B"]),
    });
    let before = state(&f.wb);
    let e = f
        .try_exec(Command::DeleteRows {
            sheet: s,
            at: 2,
            count: 1,
        })
        .expect_err("refused");
    assert!(e.to_string().contains("header row"));
    assert_eq!(state(&f.wb), before);
}

#[test]
fn shifts_names_conditional_formats_and_validations_on_other_sheets_too() {
    let mut f = setup(&["A", "B"]);
    let (a, b) = (f.id(0), f.id(1));
    f.exec(Command::DefineName {
        name: "Input".into(),
        formula: "A!$C$5".into(),
        scope: None,
        hidden: false,
        comment: None,
    });
    let dxf = f.wb.styles_mut().intern_dxf(Default::default());
    f.exec(Command::SetSheetMeta {
        sheet: b,
        field: Box::new(MetaField::Cf(vec![ConditionalFormat {
            ranges: vec![Range::cell(0, 0)],
            rules: vec![CfRule {
                id: 1,
                priority: 1,
                stop_if_true: false,
                kind: CfKind::Expression {
                    formula: "A!$C$5>0".into(),
                    dxf,
                },
            }],
        }])),
    });
    let mut v = DataValidation::new(1, vec![Range::cell(9, 0)], ValidationType::List);
    v.formula1 = Some("$C$5:$C$9".into());
    f.exec(Command::SetSheetMeta {
        sheet: a,
        field: Box::new(MetaField::Validations(vec![v])),
    });
    f.exec(Command::InsertRows {
        sheet: a,
        at: 0,
        count: 3,
        format_from: FormatFrom::Before,
    });
    assert_eq!(f.wb.names()[0].formula, "A!$C$8");
    let rule = f.wb.sheet(b).map(|x| x.meta().cf[0].rules[0].kind.clone());
    assert_eq!(
        rule,
        Some(CfKind::Expression {
            formula: "A!$C$8>0".into(),
            dxf
        })
    );
    let va =
        f.wb.sheet(a)
            .map(|x| x.meta().validations[0].clone())
            .expect("rule");
    assert_eq!(va.formula1.as_deref(), Some("$C$8:$C$12"));
    assert_eq!(va.ranges[0], Range::cell(12, 0));
    f.undo();
    assert_eq!(f.wb.names()[0].formula, "A!$C$5");
    let va =
        f.wb.sheet(a)
            .map(|x| x.meta().validations[0].clone())
            .expect("rule");
    assert_eq!(va.formula1.as_deref(), Some("$C$5:$C$9"));
}

#[test]
fn deletes_columns_and_restores_everything_on_undo() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[n(1.0), n(2.0), n(3.0), n(4.0)]]);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput::formula(1, 3, "=SUM(A1:D1)"),
            CellInput::formula(1, 0, "=C1"),
        ],
    });
    f.exec(Command::SetColProps {
        sheet: s,
        at: 2,
        count: 1,
        patch: ColPatch {
            width: Patch::Set(F64(20.0)),
            custom_width: Some(true),
            ..ColPatch::default()
        },
    });
    let before = state(&f.wb);
    f.exec(Command::DeleteCols {
        sheet: s,
        at: 1,
        count: 2,
    });
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 1, 1)),
        g(&[&["1", "4"], &["=#REF!", "=SUM(A1:B1)"]])
    );
    assert_eq!(f.num(s, "B2"), 5.0);
    assert!(f.wb.sheet(s).is_some_and(|x| x.cols().get(2).is_none()));
    f.undo();
    assert_eq!(state(&f.wb), before);
    assert_eq!(f.num(s, "D2"), 10.0);
    f.redo();
    assert_eq!(
        grid(&f.wb, s, Range::new(1, 0, 1, 1)),
        g(&[&["=#REF!", "=SUM(A1:B1)"]])
    );
}

// ── move and copy ─────────────────────────────────────────────────────────

#[test]
fn moves_cells_with_notes_merges_and_links_while_references_follow() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[n(1.0), n(2.0)], &[n(3.0), n(4.0)]]);
    f.set(s, "F6", "=SUM(A1:B2)");
    f.exec(Command::Merge {
        sheet: s,
        range: Range::new(3, 0, 3, 1),
        mode: MergeMode::Merge,
    });
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Notes(vec![Note {
            at: CellPos::new(1, 1),
            author: "A".into(),
            text: "n".into(),
            visible: false,
            size: None,
        }])),
    });
    let before = state(&f.wb);
    f.exec(Command::MoveRange {
        sheet: s,
        range: Range::new(0, 0, 3, 1),
        to_sheet: None,
        to: CellPos::new(10, 2),
    });
    assert_eq!(
        grid(&f.wb, s, Range::new(10, 2, 11, 3)),
        g(&[&["1", "2"], &["3", "4"]])
    );
    assert_eq!(f.formula(s, "F6").as_deref(), Some("SUM(C11:D12)"));
    assert_eq!(f.num(s, "F6"), 10.0);
    let sheet = f.wb.sheet(s).expect("sheet");
    assert_eq!(sheet.merges(), &[Range::new(13, 2, 13, 3)]);
    assert_eq!(sheet.meta().notes[0].at, CellPos::new(11, 3));
    assert!(sheet.cells().get(0, 0).is_none());
    f.undo();
    assert_eq!(state(&f.wb), before);
    assert_eq!(f.num(s, "F6"), 10.0);
}

#[test]
fn moves_to_another_sheet_and_overwrites_the_destination() {
    let mut f = setup(&["A", "B"]);
    let (a, b) = (f.id(0), f.id(1));
    f.exec(Command::SetCells {
        sheet: a,
        cells: vec![
            CellInput::value(0, 0, "moved"),
            CellInput::formula(0, 1, "=A1"),
        ],
    });
    f.exec(Command::SetCells {
        sheet: b,
        cells: vec![
            CellInput::value(0, 0, "old"),
            CellInput::value(1, 0, "old2"),
        ],
    });
    let before = state(&f.wb);
    f.exec(Command::MoveRange {
        sheet: a,
        range: Range::cell(0, 0),
        to_sheet: Some(b),
        to: CellPos::new(0, 0),
    });
    assert_eq!(f.val(b, "A1"), t("moved"));
    assert_eq!(f.formula(a, "B1").as_deref(), Some("B!A1"));
    assert_eq!(f.val(a, "B1"), t("moved"));
    f.undo();
    assert_eq!(state(&f.wb), before);
    assert_eq!(f.val(a, "B1"), t("moved"));
}

#[test]
fn refuses_to_move_part_of_a_merge_or_table() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::Merge {
        sheet: s,
        range: Range::new(0, 0, 1, 1),
        mode: MergeMode::Merge,
    });
    assert!(
        f.try_exec(Command::MoveRange {
            sheet: s,
            range: Range::cell(0, 0),
            to_sheet: None,
            to: CellPos::new(5, 5),
        })
        .is_err()
    );
    f.exec(Command::AddTable {
        sheet: s,
        table: table("T", Range::new(5, 0, 7, 1), &["a", "b"]),
    });
    assert!(
        f.try_exec(Command::MoveRange {
            sheet: s,
            range: Range::new(5, 0, 6, 1),
            to_sheet: None,
            to: CellPos::new(20, 0),
        })
        .is_err()
    );
}

fn copy(
    f: &mut Fixture,
    s: SheetId,
    from: Range,
    to: Range,
    what: PasteWhat,
    transpose: bool,
    skip: bool,
) {
    f.exec(Command::CopyRange {
        sheet: s,
        range: from,
        to_sheet: None,
        to,
        what,
        op: Default::default(),
        transpose,
        skip_blanks: skip,
    });
}

#[test]
fn copies_with_relative_reference_adjustment() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "B1", "=A1*$A$1+A$1");
    copy(
        &mut f,
        s,
        Range::cell(0, 1),
        Range::cell(3, 2),
        PasteWhat::All,
        false,
        false,
    );
    assert_eq!(f.formula(s, "C4").as_deref(), Some("B4*$A$1+B$1"));
}

#[test]
fn tiles_the_source_over_a_larger_destination() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[n(1.0), n(2.0)]]);
    copy(
        &mut f,
        s,
        Range::new(0, 0, 0, 1),
        Range::new(2, 0, 4, 3),
        PasteWhat::All,
        false,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(2, 0, 4, 3)),
        g(&[
            &["1", "2", "1", "2"],
            &["1", "2", "1", "2"],
            &["1", "2", "1", "2"]
        ])
    );
}

#[test]
fn pastes_values_formulas_formats_and_transposed_blocks() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let b = bold(&mut f.wb);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput {
                row: 0,
                col: 0,
                content: Content::Value(n(2.0)),
                style: Some(b),
            },
            CellInput::formula(0, 1, "=A1*3"),
        ],
    });
    copy(
        &mut f,
        s,
        Range::new(0, 0, 0, 1),
        Range::cell(5, 0),
        PasteWhat::Values,
        false,
        false,
    );
    assert_eq!(grid(&f.wb, s, Range::new(5, 0, 5, 1)), g(&[&["2", "6"]]));
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().style(5, 0)), Some(0));
    copy(
        &mut f,
        s,
        Range::cell(0, 0),
        Range::cell(6, 0),
        PasteWhat::Formats,
        false,
        false,
    );
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().style(6, 0)), Some(b));
    assert!(f.val(s, "A7").is_blank());
    copy(
        &mut f,
        s,
        Range::new(0, 0, 0, 1),
        Range::cell(10, 5),
        PasteWhat::All,
        true,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(10, 5, 11, 5)),
        g(&[&["2"], &["=E12*3"]])
    );
    // Formulas only: no formats.
    copy(
        &mut f,
        s,
        Range::new(0, 0, 0, 1),
        Range::cell(20, 0),
        PasteWhat::Formulas,
        false,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(20, 0, 20, 1)),
        g(&[&["2", "=A21*3"]])
    );
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().style(20, 0)), Some(0));
}

#[test]
fn skips_blanks_and_applies_paste_operations() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[n(1.0), CellValue::Blank, n(3.0)]]);
    set_range(&mut f, s, 2, 0, &[&[n(9.0), n(9.0), n(9.0)]]);
    copy(
        &mut f,
        s,
        Range::new(0, 0, 0, 2),
        Range::cell(2, 0),
        PasteWhat::All,
        false,
        true,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(2, 0, 2, 2)),
        g(&[&["1", "9", "3"]])
    );
    set_range(&mut f, s, 4, 0, &[&[n(10.0), n(20.0)]]);
    f.exec(Command::CopyRange {
        sheet: s,
        range: Range::new(0, 0, 0, 1),
        to_sheet: None,
        to: Range::cell(4, 0),
        what: PasteWhat::Values,
        op: kiln_sheet::commands::PasteOp::Multiply,
        transpose: false,
        skip_blanks: false,
    });
    assert_eq!(grid(&f.wb, s, Range::new(4, 0, 4, 1)), g(&[&["10", "20"]]));
}

// ── sort ──────────────────────────────────────────────────────────────────

fn sort_data(f: &mut Fixture, s: SheetId) {
    set_range(
        f,
        s,
        0,
        0,
        &[
            &[t("Name"), t("Score")],
            &[t("bob"), n(70.0)],
            &[t("Ann"), n(90.0)],
            &[t("carl"), CellValue::Blank],
            &[t("dave"), n(70.0)],
            &[t("Eve"), t("n/a")],
        ],
    );
}

fn sort(f: &mut Fixture, s: SheetId, r: Range, keys: Vec<SortKey>, header: bool, by_columns: bool) {
    f.exec(Command::Sort {
        sheet: s,
        range: r,
        keys,
        header,
        case_sensitive: false,
        by_columns,
        locale: None,
    });
}

#[test]
fn sorts_rows_by_one_key_with_the_header_excluded_blanks_last() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    sort_data(&mut f, s);
    sort(
        &mut f,
        s,
        Range::new(0, 0, 5, 1),
        vec![SortKey::asc(1)],
        true,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 5, 1)),
        g(&[
            &["Name", "Score"],
            &["bob", "70"],
            &["dave", "70"],
            &["Ann", "90"],
            &["Eve", "n/a"],
            &["carl", ""],
        ])
    );
}

#[test]
fn sorts_descending_with_blanks_still_last_and_breaks_ties() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    sort_data(&mut f, s);
    sort(
        &mut f,
        s,
        Range::new(0, 0, 5, 1),
        vec![SortKey::desc(1), SortKey::desc(0)],
        true,
        false,
    );
    let names: Vec<String> = grid(&f.wb, s, Range::new(1, 0, 5, 0)).concat();
    assert_eq!(names, vec!["Eve", "Ann", "dave", "bob", "carl"]);
}

#[test]
fn sorts_text_case_insensitively_and_by_custom_lists() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(
        &mut f,
        s,
        0,
        0,
        &[&[t("Mar")], &[t("jan")], &[t("Feb")], &[t("Apr")]],
    );
    sort(
        &mut f,
        s,
        Range::new(0, 0, 3, 0),
        vec![SortKey::asc(0)],
        false,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 3, 0)).concat(),
        vec!["Apr", "Feb", "jan", "Mar"]
    );
    let mut k = SortKey::asc(0);
    k.custom_list = Some(vec!["Jan".into(), "Feb".into(), "Mar".into(), "Apr".into()]);
    sort(&mut f, s, Range::new(0, 0, 3, 0), vec![k], false, false);
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 3, 0)).concat(),
        vec!["jan", "Feb", "Mar", "Apr"]
    );
}

#[test]
fn sort_adjusts_relative_references_and_undoes_exactly() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[n(3.0)], &[n(1.0)], &[n(2.0)]]);
    f.exec(Command::SetCells {
        sheet: s,
        cells: (0..3)
            .map(|r| CellInput::formula(r, 1, format!("=A{}*10", r + 1)))
            .collect(),
    });
    let before = state(&f.wb);
    sort(
        &mut f,
        s,
        Range::new(0, 0, 2, 1),
        vec![SortKey::asc(0)],
        false,
        false,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 2, 1)),
        g(&[&["1", "=A1*10"], &["2", "=A2*10"], &["3", "=A3*10"]])
    );
    assert_eq!(f.num(s, "B3"), 30.0);
    f.undo();
    assert_eq!(state(&f.wb), before);
}

#[test]
fn sorts_columns_left_to_right() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(
        &mut f,
        s,
        0,
        0,
        &[&[n(3.0), n(1.0), n(2.0)], &[t("c"), t("a"), t("b")]],
    );
    sort(
        &mut f,
        s,
        Range::new(0, 0, 1, 2),
        vec![SortKey::asc(0)],
        false,
        true,
    );
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 1, 2)),
        g(&[&["1", "2", "3"], &["a", "b", "c"]])
    );
}

#[test]
fn refuses_sort_keys_outside_the_range_and_partial_merges() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    assert!(
        f.try_exec(Command::Sort {
            sheet: s,
            range: Range::new(0, 0, 3, 0),
            keys: vec![SortKey::asc(5)],
            header: false,
            case_sensitive: false,
            by_columns: false,
            locale: None,
        })
        .is_err()
    );
    f.exec(Command::Merge {
        sheet: s,
        range: Range::new(1, 0, 2, 0),
        mode: MergeMode::Merge,
    });
    let e = f
        .try_exec(Command::Sort {
            sheet: s,
            range: Range::new(0, 0, 3, 0),
            keys: vec![SortKey::asc(0)],
            header: false,
            case_sensitive: false,
            by_columns: false,
            locale: None,
        })
        .expect_err("refused");
    assert!(e.to_string().contains("merged"));
}

#[test]
fn sorts_by_fill_colour_and_case_sensitively() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(&mut f, s, 0, 0, &[&[t("a")], &[t("B")], &[t("c")]]);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(2, 0)],
        patch: fill_yellow(),
    });
    let mut k = SortKey::asc(0);
    k.sort_by = kiln_sheet::meta::SortBy::CellColor;
    k.color = Some(kiln_sheet::Color::rgb(0xFFFF00));
    sort(&mut f, s, Range::new(0, 0, 2, 0), vec![k], false, false);
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 2, 0)).concat(),
        vec!["c", "a", "B"]
    );
    set_range(&mut f, s, 5, 0, &[&[t("Apple")], &[t("apple")]]);
    f.exec(Command::Sort {
        sheet: s,
        range: Range::new(5, 0, 6, 0),
        keys: vec![SortKey::asc(0)],
        header: false,
        case_sensitive: true,
        by_columns: false,
        locale: None,
    });
    assert_eq!(
        grid(&f.wb, s, Range::new(5, 0, 6, 0)).concat(),
        vec!["apple", "Apple"]
    );
}

// ── sheets ────────────────────────────────────────────────────────────────

#[test]
fn adds_renames_reorders_and_removes_sheets_with_undo() {
    let mut f = setup(&["Sheet1"]);
    let before = state(&f.wb);
    f.exec(Command::AddSheet {
        name: None,
        index: None,
        id: None,
    });
    f.exec(Command::AddSheet {
        name: Some("Data".into()),
        index: Some(0),
        id: None,
    });
    assert_eq!(f.wb.sheet_names(), vec!["Data", "Sheet1", "Sheet2"]);
    let first = f.wb.sheets()[0].id();
    f.exec(Command::RenameSheet {
        sheet: first,
        name: "Inputs".into(),
    });
    f.exec(Command::MoveSheet {
        sheet: first,
        index: 2,
    });
    assert_eq!(f.wb.sheet_names(), vec!["Sheet1", "Sheet2", "Inputs"]);
    let second = f.wb.sheets()[1].id();
    f.exec(Command::RemoveSheet { sheet: second });
    assert_eq!(f.wb.sheet_names(), vec!["Sheet1", "Inputs"]);
    while f.log.undo(&mut f.wb).expect("undo") {}
    assert_eq!(state(&f.wb), before);
}

#[test]
fn undoing_a_rename_restores_dangling_references_to_the_new_name() {
    // A formula that already names the rename target (here a dangling reference to a removed
    // sheet) is rewritten by the inverse's reverse rename; undo must put it back verbatim.
    let mut f = setup(&["One", "Two"]);
    let one = f.id(0);
    let two = f.id(1);
    f.exec(Command::RemoveSheet { sheet: two });
    f.set(one, "A1", "=SUM(Two!B1:B5)");
    assert_eq!(f.err(one, "A1"), "#REF!");
    let before = state(&f.wb);
    f.exec(Command::RenameSheet {
        sheet: one,
        name: "Two".into(),
    });
    let after = state(&f.wb);
    f.undo();
    assert_eq!(f.formula(one, "A1").as_deref(), Some("SUM(Two!B1:B5)"));
    assert_eq!(state(&f.wb), before);
    support::assert_engine_in_sync(&f.wb);
    f.redo();
    assert_eq!(state(&f.wb), after);
    f.undo();
    assert_eq!(state(&f.wb), before);
}

#[test]
fn keeps_at_least_one_visible_sheet() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let e = f
        .try_exec(Command::RemoveSheet { sheet: s })
        .expect_err("refused");
    assert!(e.to_string().contains("at least one"));
    let e = f
        .try_exec(Command::SetSheetMeta {
            sheet: s,
            field: Box::new(MetaField::Visibility(SheetVisibility::Hidden)),
        })
        .expect_err("refused");
    assert!(e.to_string().contains("visible"));
    f.exec(Command::AddSheet {
        name: None,
        index: None,
        id: None,
    });
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Visibility(SheetVisibility::Hidden)),
    });
    let other = f.wb.sheets()[1].id();
    let e = f
        .try_exec(Command::RemoveSheet { sheet: other })
        .expect_err("refused");
    assert!(e.to_string().contains("visible"));
}

#[test]
fn restores_a_removed_sheet_with_its_content_tables_and_local_names() {
    let mut f = setup(&["Sheet1"]);
    f.exec(Command::AddSheet {
        name: Some("Gone".into()),
        index: None,
        id: None,
    });
    let g = f.wb.sheet_by_name("Gone").map(|x| x.id()).expect("added");
    set_range(&mut f, g, 0, 0, &[&[t("H")], &[n(1.0)]]);
    f.exec(Command::AddTable {
        sheet: g,
        table: table("GT", Range::new(0, 0, 1, 0), &["H"]),
    });
    f.exec(Command::DefineName {
        name: "Local".into(),
        formula: "1".into(),
        scope: Some(g),
        hidden: false,
        comment: None,
    });
    let s = f.id(0);
    f.set(s, "A1", "=SUM(GT[H])+Gone!A2");
    assert_eq!(f.num(s, "A1"), 2.0);
    let before = state(&f.wb);
    f.exec(Command::RemoveSheet { sheet: g });
    assert!(f.wb.names().is_empty());
    assert!(f.wb.find_table("GT").is_none());
    assert_eq!(f.err(s, "A1"), "#REF!");
    f.undo();
    assert_eq!(state(&f.wb), before);
    assert_eq!(f.num(s, "A1"), 2.0);
    support::assert_engine_in_sync(&f.wb);
}

#[test]
fn rejects_bad_sheet_names() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for bad in [
        "a:b",
        "",
        "x/y",
        "'quoted",
        "History",
        "a name that is far too long for excel",
    ] {
        assert!(
            f.try_exec(Command::RenameSheet {
                sheet: s,
                name: bad.into()
            })
            .is_err(),
            "{bad}"
        );
    }
    f.exec(Command::AddSheet {
        name: Some("Two".into()),
        index: None,
        id: None,
    });
    let e = f
        .try_exec(Command::RenameSheet {
            sheet: s,
            name: "two".into(),
        })
        .expect_err("refused");
    assert!(e.to_string().contains("already exists"));
}

// ── rows, columns, merges, views ──────────────────────────────────────────

#[test]
fn hides_rows_and_sets_heights_with_exact_undo() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    f.exec(Command::SetRowProps {
        sheet: s,
        at: 3,
        count: 4,
        patch: RowPatch {
            hidden: Some(true),
            ..RowPatch::default()
        },
    });
    f.exec(Command::SetRowProps {
        sheet: s,
        at: 4,
        count: 1,
        patch: RowPatch {
            height: Patch::Set(F64(30.0)),
            custom_height: Some(true),
            ..RowPatch::default()
        },
    });
    let sheet = f.wb.sheet(s).expect("sheet");
    assert!(sheet.is_row_hidden(5));
    let p = sheet.rows().get(4).copied().unwrap_or_default();
    assert!(p.hidden && p.custom_height && p.height == Some(F64(30.0)));
    f.exec(Command::SetRowProps {
        sheet: s,
        at: 3,
        count: 4,
        patch: RowPatch {
            hidden: Some(false),
            ..RowPatch::default()
        },
    });
    let p =
        f.wb.sheet(s)
            .and_then(|x| x.rows().get(4).copied())
            .unwrap_or_default();
    assert!(!p.hidden && p.height == Some(F64(30.0)));
    assert!(
        f.try_exec(Command::SetRowProps {
            sheet: s,
            at: 0,
            count: 1,
            patch: RowPatch {
                height: Patch::Set(F64(500.0)),
                ..RowPatch::default()
            },
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::SetColProps {
            sheet: s,
            at: 0,
            count: 1,
            patch: ColPatch {
                outline_level: Some(8),
                ..ColPatch::default()
            },
        })
        .is_err()
    );
    while f.log.undo(&mut f.wb).expect("undo") {}
    assert_eq!(state(&f.wb), before);
}

#[test]
fn merges_keeping_the_upper_left_value_and_centres() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    set_range(
        &mut f,
        s,
        0,
        0,
        &[&[CellValue::Blank, t("b")], &[t("c"), t("d")]],
    );
    let before = state(&f.wb);
    f.exec(Command::Merge {
        sheet: s,
        range: Range::new(0, 0, 1, 1),
        mode: MergeMode::Center,
    });
    assert_eq!(f.val(s, "A1"), t("b"));
    assert!(f.val(s, "B2").is_blank());
    let st = f.wb.sheet(s).map_or(0, |x| x.cells().style(0, 0));
    assert_eq!(f.wb.styles().xf(st).alignment.horizontal, HAlign::Center);
    assert_eq!(
        f.wb.sheet(s).map(|x| x.merges().to_vec()),
        Some(vec![Range::new(0, 0, 1, 1)])
    );
    f.undo();
    assert_eq!(state(&f.wb), before);
}

#[test]
fn merges_across_rows_separately() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::Merge {
        sheet: s,
        range: Range::new(0, 0, 2, 3),
        mode: MergeMode::Across,
    });
    assert_eq!(f.wb.sheet(s).map(|x| x.merges().len()), Some(3));
    f.exec(Command::Unmerge {
        sheet: s,
        range: Range::cell(1, 1),
    });
    assert_eq!(f.wb.sheet(s).map(|x| x.merges().len()), Some(2));
    let e = f
        .try_exec(Command::Merge {
            sheet: s,
            range: Range::new(0, 2, 0, 5),
            mode: MergeMode::Merge,
        })
        .expect_err("refused");
    assert!(e.to_string().contains("partly overlaps"));
}

#[test]
fn freezes_panes_and_changes_the_view() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetFreeze {
        sheet: s,
        rows: 1,
        cols: 2,
    });
    let view = f.wb.sheet(s).map(|x| x.meta().view.clone()).expect("view");
    assert_eq!(view.freeze.map(|x| (x.rows, x.cols)), Some((1, 2)));
    let mut v = view.clone();
    v.zoom = 150;
    v.show_gridlines = false;
    v.right_to_left = true;
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::View(v.clone())),
    });
    assert_eq!(f.wb.sheet(s).map(|x| x.meta().view.zoom), Some(150));
    v.zoom = 5;
    assert!(
        f.try_exec(Command::SetSheetMeta {
            sheet: s,
            field: Box::new(MetaField::View(v)),
        })
        .is_err()
    );
    f.undo();
    f.undo();
    assert!(
        f.wb.sheet(s)
            .is_some_and(|x| x.meta().view.freeze.is_none())
    );
}

// ── names and tables ──────────────────────────────────────────────────────

fn define(
    f: &mut Fixture,
    name: &str,
    formula: &str,
    scope: Option<SheetId>,
) -> kiln_sheet::Result<()> {
    f.try_exec(Command::DefineName {
        name: name.into(),
        formula: formula.into(),
        scope,
        hidden: false,
        comment: None,
    })
}

#[test]
fn validates_names() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    assert!(define(&mut f, "A1", "1", None).is_err());
    assert!(define(&mut f, "has space", "1", None).is_err());
    assert!(define(&mut f, "ok", "=SUM(", None).is_err());
    assert!(define(&mut f, "_xlnm.Print_Area", "Sheet1!$A$1:$D$20", Some(s)).is_ok());
    assert!(
        f.try_exec(Command::DeleteName {
            name: "missing".into(),
            scope: None
        })
        .is_err()
    );
}

#[test]
fn validates_tables() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let base = table("T", Range::new(0, 0, 3, 1), &["a", "b"]);
    f.exec(Command::AddTable {
        sheet: s,
        table: base.clone(),
    });
    let with = |name: &str, range: Range, cols: &[&str]| Command::AddTable {
        sheet: s,
        table: table(name, range, cols),
    };
    let msg = |f: &mut Fixture, c: Command| {
        f.try_exec(c)
            .map_err(|e| e.to_string())
            .expect_err("refused")
    };
    assert!(
        msg(&mut f, with("t", Range::new(10, 0, 12, 1), &["a", "b"])).contains("already exists")
    );
    assert!(msg(&mut f, with("U", Range::new(0, 0, 3, 1), &["a", "b"])).contains("overlap"));
    assert!(msg(&mut f, with("V", Range::new(10, 0, 12, 0), &["a", "b"])).contains("column names"));
    assert!(msg(&mut f, with("W", Range::new(10, 0, 12, 1), &["x", "X"])).contains("duplicate"));
    assert!(
        define(&mut f, "T", "1", None)
            .map_err(|e| e.to_string())
            .expect_err("refused")
            .contains("table")
    );
    let e = f
        .try_exec(Command::Merge {
            sheet: s,
            range: Range::new(1, 0, 1, 1),
            mode: MergeMode::Merge,
        })
        .expect_err("refused");
    assert!(e.to_string().contains("table"));
}

#[test]
fn writes_header_names_into_the_header_row() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::AddTable {
        sheet: s,
        table: table("T", Range::new(0, 0, 2, 1), &["Qty", "Cost"]),
    });
    assert_eq!(
        grid(&f.wb, s, Range::new(0, 0, 0, 1)),
        g(&[&["Qty", "Cost"]])
    );
    // Clearing a header cell gives the column a default name.
    f.set(s, "B1", ());
    let col = |f: &Fixture| {
        f.wb.sheet(s)
            .map(|x| x.tables()[0].columns[1].name.clone())
            .unwrap_or_default()
    };
    assert_eq!(col(&f), "Column2");
    assert_eq!(f.val(s, "B1"), t("Column2"));
    // A duplicate header name gets a number.
    f.set(s, "B1", "qty");
    assert_eq!(col(&f), "qty2");
}

// ── properties and batches ────────────────────────────────────────────────

#[test]
fn sets_properties_with_undo() {
    let mut f = setup(&["Sheet1"]);
    let mut p = f.wb.props().clone();
    p.calc.mode = CalcMode::Manual;
    p.calc.iterate = true;
    f.exec(Command::SetProps { props: Box::new(p) });
    assert_eq!(f.wb.props().calc.mode, CalcMode::Manual);
    assert!(f.wb.props().calc.iterate);
    assert_eq!(f.wb.props().calc.iterate_count, 100);
    f.undo();
    assert_eq!(f.wb.props().calc.mode, CalcMode::Auto);
    let mut bad = f.wb.props().clone();
    bad.active_sheet = 4;
    assert!(
        f.try_exec(Command::SetProps {
            props: Box::new(bad)
        })
        .is_err()
    );
}

#[test]
fn rolls_back_a_failing_batch_completely() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    let bad = Command::Batch {
        commands: vec![
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::value(0, 0, 1)],
            },
            Command::InsertRows {
                sheet: s,
                at: 0,
                count: 2,
                format_from: FormatFrom::Before,
            },
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::formula(0, 0, "=(")],
            },
        ],
        label: None,
    };
    assert!(f.wb.apply(&bad).is_err());
    assert_eq!(state(&f.wb), before);
}

#[test]
fn reports_changes_once_per_command() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.wb.take_changes();
    f.exec(Command::Batch {
        commands: vec![
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::value(0, 0, 1)],
            },
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::value(1, 0, 2)],
            },
        ],
        label: None,
    });
    let c = f.wb.take_changes();
    // Adjacent cells merge into one range.
    assert_eq!(c.cells, vec![(s, Range::new(0, 0, 1, 0))]);
    assert!(f.wb.take_changes().is_empty());
}

// ── serialisation ─────────────────────────────────────────────────────────

#[test]
fn round_trips_commands_through_json_and_replays_them() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(0, 0, CellValue::Error(ErrorValue::Div0))],
    });
    f.exec(Command::InsertRows {
        sheet: s,
        at: 0,
        count: 1,
        format_from: FormatFrom::Before,
    });
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::new(0, 0, 2, 2)],
        patch: Box::new(StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        }),
    });
    let mut g2 = setup(&["Sheet1"]);
    let history = f.log.history();
    for e in history.undo {
        let json = e.forward.to_json().expect("json");
        let back = Command::from_json(&json).expect("parse");
        let bytes = back.to_bytes().expect("postcard");
        g2.exec(Command::from_bytes(&bytes).expect("decode"));
    }
    assert_eq!(state(&g2.wb), state(&f.wb));
}

#[test]
fn rejects_unknown_commands() {
    assert!(Command::from_json(r#"{"formatDisk":{}}"#).is_err());
    assert!(Command::from_json(r#"{"batch":{"commands":[{"nope":{}}]}}"#).is_err());
    assert!(Command::from_bytes(&[200, 1, 2]).is_err());
}

#[test]
fn restores_a_serialised_history_and_can_undo_it() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    f.log
        .exec_labelled(
            &mut f.wb,
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::value(0, 0, 5)],
            },
            "Typing",
        )
        .expect("applies");
    let json = f.log.to_json().expect("json");
    let mut log2 = CommandLog::new();
    log2.load_json(&json).expect("loads");
    assert_eq!(log2.undo_label(), Some("Typing"));
    log2.undo(&mut f.wb).expect("undo");
    assert_eq!(state(&f.wb), before);
}

// ── the log ───────────────────────────────────────────────────────────────

#[test]
fn groups_transactions_into_one_undo_step() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    let Fixture { wb, log, .. } = &mut f;
    log.transaction(wb, "Fill", |log, wb| {
        for i in 0..5 {
            log.exec(
                wb,
                Command::SetCells {
                    sheet: s,
                    cells: vec![CellInput::value(i, 0, i as i32)],
                },
            )?;
        }
        log.exec(
            wb,
            Command::InsertRows {
                sheet: s,
                at: 0,
                count: 1,
                format_from: FormatFrom::Before,
            },
        )
    })
    .expect("commits");
    assert_eq!(f.log.undo_depth(), 1);
    assert_eq!(f.log.undo_label(), Some("Fill"));
    f.undo();
    assert_eq!(state(&f.wb), before);
    f.redo();
    assert_eq!(f.val(s, "A6"), n(4.0));
}

#[test]
fn rolls_back_a_transaction_that_fails() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let before = state(&f.wb);
    let Fixture { wb, log, .. } = &mut f;
    let r = log.transaction(wb, "Bad", |log, wb| {
        log.exec(
            wb,
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::value(0, 0, 1)],
            },
        )?;
        log.exec(
            wb,
            Command::SetCells {
                sheet: s,
                cells: vec![CellInput::formula(0, 1, "=(")],
            },
        )
    });
    assert!(r.is_err());
    assert_eq!(state(&f.wb), before);
    assert!(!f.log.can_undo());
}

#[test]
fn supports_nested_transactions() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.log.begin("outer");
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(0, 0, 1)],
    });
    f.log.begin("inner");
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(1, 0, 2)],
    });
    f.log.commit(&mut f.wb).expect("inner");
    assert!(f.log.undo(&mut f.wb).is_err());
    f.log.commit(&mut f.wb).expect("outer");
    assert_eq!(f.log.undo_depth(), 1);
    assert!(f.log.commit(&mut f.wb).is_err());
}

#[test]
fn clears_the_redo_stack_on_new_edits_and_respects_the_limit() {
    let mut f = setup(&["Sheet1"]);
    f.log = CommandLog::with_limit(3);
    let s = f.id(0);
    for i in 0..5 {
        f.exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(i, 0, i as i32)],
        });
    }
    assert_eq!(f.log.undo_depth(), 3);
    f.undo();
    assert!(f.log.can_redo());
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(9, 0, 9)],
    });
    assert!(!f.log.can_redo());
    assert!(!f.log.redo(&mut f.wb).expect("ok"));
    f.log.clear();
    assert!(!f.log.undo(&mut f.wb).expect("ok"));
}

#[test]
fn protected_sheets_refuse_edits_to_locked_cells() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let unlocked = f.wb.styles_mut().patch(
        0,
        &StylePatch {
            protection: Some(kiln_sheet::styles::ProtectionPatch {
                locked: Some(false),
                hidden: None,
            }),
            ..StylePatch::default()
        },
    );
    f.exec(Command::SetStyleIndex {
        sheet: s,
        ranges: vec![Range::cell(0, 1)],
        style: unlocked,
    });
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Protection(Some(Default::default()))),
    });
    let e = f
        .try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(0, 0, 1)],
        })
        .expect_err("locked");
    assert!(matches!(e, kiln_sheet::ModelError::Protected(_)));
    f.set(s, "B1", 2);
    assert_eq!(f.num(s, "B1"), 2.0);
    assert!(
        f.try_exec(Command::InsertRows {
            sheet: s,
            at: 0,
            count: 1,
            format_from: FormatFrom::Before,
        })
        .is_err()
    );
    // Undo still works on a protected sheet.
    f.undo();
    assert!(f.val(s, "B1").is_blank());
}
