// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workbook construction and serialisation to postcard.

mod support;

use kiln_sheet::commands::{CellInput, Command, MetaField};
use kiln_sheet::meta::{DefinedName, Note, Table, TableColumn};
use kiln_sheet::serial::{FORMAT_VERSION, MAGIC};
use kiln_sheet::styles::{FontPatch, StylePatch};
use kiln_sheet::{CellPos, CellValue, Range, Workbook, WorkbookOptions};
use support::{assert_engine_in_sync, setup, state};

fn rich_workbook() -> support::Fixture {
    let mut f = setup(&["Data", "Report"]);
    let (d, r) = (f.id(0), f.id(1));
    for i in 0..50 {
        f.set(d, &format!("A{}", i + 2), i);
        f.set(d, &format!("B{}", i + 2), format!("item {i}").as_str());
    }
    f.set(d, "A1", "Qty");
    f.set(d, "B1", "Name");
    f.exec(Command::AddTable {
        sheet: d,
        table: Box::new(Table {
            id: 0,
            name: "Items".into(),
            display_name: None,
            range: Range::new(0, 0, 50, 1),
            header_row: true,
            totals_row: false,
            columns: vec![TableColumn::named("Qty"), TableColumn::named("Name")],
            style: None,
            auto_filter: None,
            comment: None,
        }),
    });
    f.exec(Command::DefineName {
        name: "Rate".into(),
        formula: "0.2".into(),
        scope: None,
        hidden: false,
        comment: Some("tax".into()),
    });
    f.set(r, "A1", "=SUM(Items[Qty])*Rate");
    f.set(r, "A2", "=SEQUENCE(3)");
    f.set(r, "B1", "=Data!A10&\"!\"");
    f.exec(Command::SetStyle {
        sheet: r,
        ranges: vec![Range::new(0, 0, 3, 1)],
        patch: Box::new(StylePatch {
            font: Some(FontPatch {
                italic: Some(true),
                ..FontPatch::default()
            }),
            num_fmt: Some("0.00".into()),
            ..StylePatch::default()
        }),
    });
    f.exec(Command::SetSheetMeta {
        sheet: r,
        field: Box::new(MetaField::Notes(vec![Note {
            at: CellPos::new(0, 0),
            author: "Kiln".into(),
            text: "Total".into(),
            visible: true,
            size: Some((120, 60)),
        }])),
    });
    f.exec(Command::SetFreeze {
        sheet: d,
        rows: 1,
        cols: 0,
    });
    f
}

#[test]
fn creates_sheet1_by_default_and_names_new_sheets_like_excel() {
    let mut wb = Workbook::new();
    assert_eq!(wb.sheet_names(), vec!["Sheet1"]);
    wb.apply(&Command::AddSheet {
        name: None,
        index: None,
        id: None,
    })
    .expect("adds");
    assert_eq!(wb.sheet_names(), vec!["Sheet1", "Sheet2"]);
    assert!(
        wb.add_sheet(Some("sheet2"), None, None).is_err(),
        "names ignore case"
    );
    assert_eq!(wb.next_sheet_name(), "Sheet3");
    assert_eq!(wb.next_table_name(), "Table1");
}

#[test]
fn round_trips_a_workbook_through_bytes_exactly() {
    let f = rich_workbook();
    let bytes = f.wb.to_bytes().expect("encodes");
    assert!(bytes.starts_with(MAGIC));
    let back = Workbook::from_bytes(&bytes, WorkbookOptions::default()).expect("decodes");
    assert_eq!(state(&back), state(&f.wb));
    assert_eq!(back.styles().to_parts(), f.wb.styles().to_parts());
    assert_engine_in_sync(&back);
    let (r0, r1) = (f.id(1), back.sheets()[1].id());
    assert_eq!(back.value(r1, 0, 0), f.wb.value(r0, 0, 0));
    assert_eq!(
        back.value(r1, 3, 0),
        CellValue::Number(3.0),
        "spills recalculate"
    );
    assert_eq!(back.display_text(r1, 0, 0), f.wb.display_text(r0, 0, 0));
    assert_eq!(back.names()[0].comment.as_deref(), Some("tax"));
}

#[test]
fn restored_workbooks_calculate_and_edit_like_the_original() {
    let f = rich_workbook();
    let mut back = Workbook::from_bytes(
        &f.wb.to_bytes().expect("encodes"),
        WorkbookOptions::default(),
    )
    .expect("decodes");
    let (d, r) = (back.sheets()[0].id(), back.sheets()[1].id());
    let before = back.value(r, 0, 0);
    back.apply(&Command::SetCells {
        sheet: d,
        cells: vec![CellInput::value(1, 0, 1000.0)],
    })
    .expect("edit");
    let after = back.value(r, 0, 0);
    assert_eq!(
        after.as_number().unwrap_or(0.0) - before.as_number().unwrap_or(0.0),
        200.0
    );
}

#[test]
fn keeps_id_counters_so_new_sheets_and_tables_do_not_collide() {
    let mut f = rich_workbook();
    f.exec(Command::AddSheet {
        name: Some("Temp".into()),
        index: None,
        id: None,
    });
    let temp = f.wb.sheet_by_name("Temp").map(|s| s.id()).expect("temp");
    f.exec(Command::RemoveSheet { sheet: temp });
    let mut back = Workbook::from_bytes(
        &f.wb.to_bytes().expect("encodes"),
        WorkbookOptions::default(),
    )
    .expect("decodes");
    let id = back.add_sheet(Some("New"), None, None).expect("adds");
    assert!(id.0 > temp.0, "removed ids are not reused");
}

#[test]
fn rejects_foreign_newer_or_damaged_data() {
    let f = rich_workbook();
    let bytes = f.wb.to_bytes().expect("encodes");
    assert!(Workbook::from_bytes(b"PK\x03\x04", WorkbookOptions::default()).is_err());
    let mut newer = bytes.clone();
    newer[MAGIC.len()] = FORMAT_VERSION + 1;
    let e = Workbook::from_bytes(&newer, WorkbookOptions::default()).expect_err("newer");
    assert!(e.to_string().contains("not supported"));
    let truncated = &bytes[..bytes.len() / 2];
    assert!(Workbook::from_bytes(truncated, WorkbookOptions::default()).is_err());
    assert!(Workbook::from_bytes(MAGIC, WorkbookOptions::default()).is_err());
    // Corrupt bytes in the middle must never panic.
    for i in (MAGIC.len() + 1..bytes.len()).step_by(97) {
        let mut b = bytes.clone();
        b[i] ^= 0x5A;
        let _ = Workbook::from_bytes(&b, WorkbookOptions::default());
    }
}

#[test]
fn snapshots_of_single_sheets_restore_through_commands() {
    let mut f = rich_workbook();
    let d = f.id(0);
    let snap = f.wb.snapshot_sheet(d).expect("snapshot");
    assert_eq!(snap.name, "Data");
    assert_eq!(snap.cells.len(), 102);
    let r = f.id(1);
    let total = f.num(r, "A1");
    f.exec(Command::RemoveSheet { sheet: d });
    assert_eq!(f.err(r, "A1"), "#REF!");
    f.undo();
    assert_eq!(f.num(r, "A1"), total);
    assert_eq!(f.wb.sheet_names(), vec!["Data", "Report"]);
}

#[test]
fn names_survive_loading() {
    let mut wb = Workbook::new();
    let s = wb.sheets()[0].id();
    wb.load_name(DefinedName {
        name: "_xlnm.Print_Area".into(),
        formula: "=Sheet1!$A$1:$C$9".into(),
        scope: Some(s),
        hidden: true,
        comment: None,
    })
    .expect("reserved names are accepted");
    wb.load_name(DefinedName {
        name: "Ext".into(),
        formula: "[1]Other!$A$1".into(),
        scope: None,
        hidden: false,
        comment: None,
    })
    .expect("external references are kept");
    assert!(
        wb.load_name(DefinedName {
            name: "bad name".into(),
            formula: "1".into(),
            scope: None,
            hidden: false,
            comment: None,
        })
        .is_err()
    );
    let back = Workbook::from_bytes(&wb.to_bytes().expect("encodes"), WorkbookOptions::default())
        .expect("decodes");
    assert_eq!(back.names().len(), 2);
    assert_eq!(back.names()[0].formula, "Sheet1!$A$1:$C$9");
}

/// Under manual calculation a formula shows what it last came to, also when it is out of date:
/// the workbook read back, or given new options, shows the same until the next calculation.
#[test]
fn manual_calculation_keeps_the_values_shown_through_a_reload() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 2);
    f.set(s, "B1", "=A1*10");
    let mut p = f.wb.props().clone();
    p.calc.mode = kiln_sheet::meta::CalcMode::Manual;
    f.exec(Command::SetProps {
        props: Box::new(p.clone()),
    });
    f.set(s, "A1", 3);
    // Stale, as Excel shows it under manual calculation.
    assert_eq!(f.wb.value(s, 0, 1), CellValue::Number(20.0));
    let back = Workbook::from_bytes(&f.wb.to_bytes().unwrap(), WorkbookOptions::default()).unwrap();
    assert_eq!(back.value(s, 0, 1), CellValue::Number(20.0));
    // New options rebuild the engine: still what showed.
    p.calc.iterate = true;
    f.exec(Command::SetProps { props: Box::new(p) });
    assert_eq!(f.wb.value(s, 0, 1), CellValue::Number(20.0));
    // Calculate Now brings it up to date.
    f.wb.recalc_all();
    assert_eq!(f.wb.value(s, 0, 1), CellValue::Number(30.0));
}

fn manual(f: &mut support::Fixture) -> kiln_sheet::meta::WorkbookProps {
    let mut p = f.wb.props().clone();
    p.calc.mode = kiln_sheet::meta::CalcMode::Manual;
    f.exec(Command::SetProps {
        props: Box::new(p.clone()),
    });
    p
}

fn column(wb: &Workbook, s: kiln_sheet::SheetId, rows: u32) -> Vec<CellValue> {
    (0..rows).map(|r| wb.value(s, r, 0)).collect()
}

/// A spilled array keeps every value it showed through a reload under manual calculation, as
/// Excel keeps a file's spilled values, and through new options.
#[test]
fn manual_calculation_keeps_a_spilled_array_through_a_reload() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=SEQUENCE(3)");
    let mut p = manual(&mut f);
    let shown = column(&f.wb, s, 3);
    assert_eq!(
        shown,
        vec![
            CellValue::Number(1.0),
            CellValue::Number(2.0),
            CellValue::Number(3.0)
        ]
    );
    let back = Workbook::from_bytes(&f.wb.to_bytes().unwrap(), WorkbookOptions::default()).unwrap();
    assert_eq!(column(&back, s, 3), shown);
    assert!(
        back.passthrough().is_empty(),
        "the stored spills are not a file part"
    );
    p.calc.iterate = true;
    f.exec(Command::SetProps { props: Box::new(p) });
    assert_eq!(column(&f.wb, s, 3), shown);
}

/// A stale spill stays what it showed, size and all, until the next calculation.
#[test]
fn manual_calculation_keeps_a_stale_spill_through_a_reload() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "B1", 3);
    f.set(s, "A1", "=SEQUENCE(B1)");
    manual(&mut f);
    f.set(s, "B1", 2);
    let shown = column(&f.wb, s, 3);
    assert_eq!(shown[2], CellValue::Number(3.0), "stale until F9");
    let mut back =
        Workbook::from_bytes(&f.wb.to_bytes().unwrap(), WorkbookOptions::default()).unwrap();
    assert_eq!(column(&back, s, 3), shown);
    back.recalc_all();
    assert_eq!(
        column(&back, s, 3),
        vec![
            CellValue::Number(1.0),
            CellValue::Number(2.0),
            CellValue::Blank
        ]
    );
}

/// Under manual calculation a formula is calculated as it is entered (Excel), and what depends
/// on it waits for the next calculation.
#[test]
fn manual_calculation_works_out_a_formula_as_it_is_entered() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 2);
    f.set(s, "C1", "=B1+1");
    manual(&mut f);
    f.set(s, "B1", "=A1*10");
    assert_eq!(f.wb.value(s, 0, 1), CellValue::Number(20.0));
    assert_eq!(
        f.wb.value(s, 0, 2),
        CellValue::Number(1.0),
        "a dependent waits"
    );
    assert!(f.wb.is_calc_pending());
    f.set(s, "A2", "=SEQUENCE(2)");
    assert_eq!(
        f.wb.value(s, 2, 0),
        CellValue::Number(2.0),
        "an entered array spills"
    );
    let back = Workbook::from_bytes(&f.wb.to_bytes().unwrap(), WorkbookOptions::default()).unwrap();
    assert_eq!(back.value(s, 0, 1), CellValue::Number(20.0));
    assert_eq!(back.value(s, 2, 0), CellValue::Number(2.0));
    f.wb.recalc();
    assert_eq!(f.wb.value(s, 0, 2), CellValue::Number(21.0));
}

/// Every public call that can change a workbook moves its stamp; reading does not.
#[test]
fn the_mutation_stamp_moves_with_every_change() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let a = f.wb.mutation_stamp();
    let _ = f.wb.value(s, 0, 0);
    let _ = f.wb.snapshot();
    assert_eq!(f.wb.mutation_stamp(), a);
    f.set(s, "A1", 1);
    let b = f.wb.mutation_stamp();
    assert_eq!(b.0, a.0);
    assert!(b.1 > a.1);
    let _ = f.wb.styles_mut();
    assert!(f.wb.mutation_stamp().1 > b.1);
    let other = Workbook::new();
    assert_ne!(other.mutation_stamp().0, a.0, "each workbook is its own");
}
