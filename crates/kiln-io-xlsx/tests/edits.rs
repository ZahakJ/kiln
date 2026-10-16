// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saves after edits: every kind of change the model makes is written so that reading the file
//! back gives the edited model, parts that depended on removed content go with it, everything
//! else stays byte-identical, and LibreOffice opens the result (where it is installed).

use kiln_io_xlsx::{
    WriteOptions, XlsxKind, XlsxRead, read_xlsx, snapshot_cells, snapshot_facts, write_xlsx,
};
use kiln_ooxml::opc::OpcPackage;
use kiln_sheet::commands::{CellInput, ColPatch, FormatFrom, MergeMode, MetaField, RowPatch};
use kiln_sheet::meta::{
    AxisPosition, BarDirection, CfKind, CfRule, Cfvo, CfvoType, ConditionalFormat, DataBar,
    DataValidation, Hyperlink, Note, Table, TableColumn, TableStyleInfo, ValidationType,
};
use kiln_sheet::num::F64;
use kiln_sheet::styles::{FontPatch, Patch, StylePatch};
use kiln_sheet::{CellPos, CellValue, Color, Command, Range, SheetId, Workbook};
use kiln_testing::oracle::Oracle;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        kiln_testing::fixtures::fixtures_dir()
            .join("xlsx/features")
            .join(name),
    )
    .expect("fixture")
}

fn open(name: &str) -> XlsxRead {
    read_xlsx(&fixture(name)).expect("read")
}

fn sheet(wb: &Workbook, name: &str) -> SheetId {
    wb.sheet_by_name(name).expect("sheet").id()
}

/// Saves, checks LibreOffice opens the file, and reads it back.
fn save_and_reread(read: &XlsxRead, label: &str) -> (Vec<u8>, XlsxRead) {
    let written =
        write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default()).expect("write");
    let back = read_xlsx(&written.bytes).expect("re-read");
    // The model read back is the model saved.
    assert_eq!(
        snapshot_facts(&back.workbook),
        snapshot_facts(&read.workbook),
        "{label}: facts"
    );
    assert_eq!(
        snapshot_cells(&back.workbook),
        snapshot_cells(&read.workbook),
        "{label}: cells"
    );
    if let Ok(oracle) = Oracle::locate() {
        let dir =
            std::env::temp_dir().join(format!("kiln-xlsx-edit-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join(format!("{label}.xlsx"));
        std::fs::write(&file, &written.bytes).expect("write file");
        assert!(
            oracle.opens(&file, &dir.join("pdf")),
            "{label}: LibreOffice cannot open the output"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    (written.bytes, back)
}

#[test]
fn cell_edits_formats_and_dynamic_arrays() {
    let mut r = open("formulas.xlsx");
    let s = sheet(&r.workbook, "Formulas");
    let wb = &mut r.workbook;
    wb.apply(&Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput::value(320, 0, CellValue::text("Edited")),
            CellInput::value(320, 1, CellValue::number(2.5)),
            CellInput::formula(320, 2, "=B321*4"),
            CellInput::formula(321, 0, "=SEQUENCE(3,2)"),
            CellInput::formula(325, 0, "=XLOOKUP(3,Data!A1:A20,Data!B1:B20)"),
            CellInput::formula(326, 0, "=LET(x,2,x*3)"),
            CellInput::value(327, 0, CellValue::Bool(true)),
            CellInput::value(327, 1, CellValue::Error(kiln_formula::ErrorValue::NA)),
            CellInput::value(327, 2, CellValue::text("  spaced & <escaped> \"text\"\r\n")),
        ],
    })
    .expect("cells");
    wb.apply(&Command::SetStyle {
        sheet: s,
        ranges: vec![Range::new(320, 0, 320, 2)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0.000\" kg\"".into()),
            font: Some(FontPatch {
                bold: Some(true),
                color: Some(Color::rgb(0xC00000)),
                ..FontPatch::default()
            }),
            fill: Patch::Set(kiln_sheet::styles::Fill::solid(Color::theme(4, 0.6))),
            ..StylePatch::default()
        }),
    })
    .expect("style");
    assert_eq!(wb.value(s, 320, 2), CellValue::Number(10.0));
    assert_eq!(wb.value(s, 323, 1), CellValue::Number(6.0));
    let (bytes, back) = save_and_reread(&r, "cells");
    let b = &back.workbook;
    let s2 = sheet(b, "Formulas");
    assert_eq!(b.formula(s2, 321, 0).as_deref(), Some("SEQUENCE(3,2)"));
    assert_eq!(b.value(s2, 323, 1), CellValue::Number(6.0));
    assert_eq!(b.value(s2, 325, 0), CellValue::text("gamma"));
    assert_eq!(b.value(s2, 326, 0), CellValue::Number(6.0));
    assert_eq!(
        &*b.styles()
            .get(b.sheet(s2).expect("s").style_at(320, 1))
            .num_fmt,
        "0.000\" kg\""
    );
    // The dynamic array is saved as one (with the metadata part), the prefixes are written.
    let pkg = OpcPackage::open(&bytes).expect("pkg");
    assert!(pkg.has("/xl/metadata.xml"));
    let sheet_xml = String::from_utf8(
        pkg.part_bytes("/xl/worksheets/sheet1.xml")
            .expect("sheet")
            .into_owned(),
    )
    .expect("utf-8");
    assert!(
        sheet_xml
            .contains(r#"<c r="A322" cm="1"><f t="array" ref="A322:B324">_xlfn.SEQUENCE(3,2)</f>"#),
        "dynamic array"
    );
    assert!(sheet_xml.contains("_xlfn._xlws") || sheet_xml.contains("_xlfn.XLOOKUP"));
    assert!(sheet_xml.contains("_xlfn.LET(_xlpm.x,2,_xlpm.x*3)"));
}

#[test]
fn structure_edits_rewrite_tables_names_and_sheets() {
    let mut r = open("tables.xlsx");
    let wb = &mut r.workbook;
    let s = wb.sheets()[0].id();
    let before = wb.sheet(s).expect("s").tables()[0].range;
    wb.apply(&Command::InsertRows {
        sheet: s,
        at: 2,
        count: 3,
        format_from: FormatFrom::Before,
    })
    .expect("insert");
    let after = wb.sheet(s).expect("s").tables()[0].range;
    assert_eq!(after.r1, before.r1 + 3);
    let added = wb
        .apply(&Command::AddSheet {
            name: Some("Added".into()),
            index: Some(0),
            id: None,
        })
        .expect("add");
    let _ = added;
    let second = wb.sheets()[2].id();
    wb.apply(&Command::RenameSheet {
        sheet: second,
        name: "Renamed Scores".into(),
    })
    .expect("rename");
    let new_id = sheet(wb, "Added");
    wb.apply(&Command::SetCells {
        sheet: new_id,
        cells: vec![CellInput::formula(0, 0, "=SUM('Renamed Scores'!B2:B11)")],
    })
    .expect("formula");
    wb.apply(&Command::AddTable {
        sheet: new_id,
        table: Box::new(Table {
            id: 99,
            name: "NewTable".into(),
            display_name: None,
            range: Range::new(2, 0, 4, 1),
            header_row: true,
            totals_row: false,
            columns: vec![TableColumn::named("K"), TableColumn::named("V")],
            style: Some(TableStyleInfo::default()),
            auto_filter: Some(Default::default()),
            comment: None,
        }),
    })
    .expect("table");
    let (bytes, back) = save_and_reread(&r, "structure");
    assert_eq!(
        back.workbook.sheet_names(),
        ["Added", "Tables", "Renamed Scores"]
    );
    let pkg = OpcPackage::open(&bytes).expect("pkg");
    assert_eq!(
        pkg.parts_by_content_type(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml"
        )
        .len(),
        3
    );
    // Removing a sheet removes its table part too.
    let mut r2 = back;
    let gone = sheet(&r2.workbook, "Renamed Scores");
    r2.workbook
        .apply(&Command::RemoveSheet { sheet: gone })
        .expect("remove");
    let written =
        write_xlsx(&r2.workbook, Some(&r2.source), &WriteOptions::default()).expect("write");
    let pkg = OpcPackage::open(&written.bytes).expect("pkg");
    assert_eq!(
        pkg.parts_by_content_type(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml"
        )
        .len(),
        2
    );
    assert!(!written.dropped.is_empty());
    let back = read_xlsx(&written.bytes).expect("read");
    let a = sheet(&back.workbook, "Added");
    // The formula that referred to the removed sheet now says so.
    let f = back.workbook.formula(a, 0, 0).unwrap_or_default();
    assert!(f.contains("#REF!"), "{f}");
}

#[test]
fn metadata_edits_round_trip() {
    let mut r = open("comments.xlsx");
    let wb = &mut r.workbook;
    let s = wb.sheets()[0].id();
    let mut notes = wb.sheet(s).expect("s").meta().notes.clone();
    notes.remove(0);
    notes.push(Note {
        at: CellPos::new(9, 4),
        author: "Kiln".into(),
        text: "Added in Kiln\nsecond line".into(),
        visible: true,
        size: Some((180, 60)),
    });
    wb.apply(&Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Notes(notes)),
    })
    .expect("notes");
    wb.apply(&Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Hyperlinks(vec![Hyperlink {
            range: Range::cell(0, 2),
            target: Some("https://example.org/kiln?a=1&b=2".into()),
            location: None,
            display: Some("Kiln".into()),
            tooltip: Some("Open".into()),
        }])),
    })
    .expect("links");
    let id = wb.alloc_rule_id();
    wb.apply(&Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Cf(vec![ConditionalFormat {
            ranges: vec![Range::new(0, 1, 9, 1)],
            rules: vec![CfRule {
                id,
                priority: 1,
                stop_if_true: false,
                kind: CfKind::DataBar(DataBar {
                    min: Cfvo::of(CfvoType::AutoMin),
                    max: Cfvo::of(CfvoType::AutoMax),
                    color: Color::rgb(0x638EC6),
                    show_value: true,
                    min_length: 0,
                    max_length: 100,
                    gradient: false,
                    border_color: Some(Color::rgb(0x2F5597)),
                    negative_color: Some(Color::rgb(0xFF0000)),
                    negative_border_color: None,
                    axis_color: Some(Color::rgb(0)),
                    axis_position: AxisPosition::Middle,
                    direction: BarDirection::LeftToRight,
                }),
            }],
        }])),
    })
    .expect("cf");
    let vid = wb.alloc_rule_id();
    let mut v = DataValidation::new(vid, vec![Range::new(0, 3, 9, 3)], ValidationType::List);
    v.formula1 = Some("\"Low,High\"".into());
    wb.apply(&Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Validations(vec![v])),
    })
    .expect("dv");
    wb.apply(&Command::Merge {
        sheet: s,
        range: Range::new(12, 0, 13, 2),
        mode: MergeMode::Merge,
    })
    .expect("merge");
    wb.apply(&Command::SetFreeze {
        sheet: s,
        rows: 1,
        cols: 1,
    })
    .expect("freeze");
    wb.apply(&Command::SetColProps {
        sheet: s,
        at: 0,
        count: 2,
        patch: ColPatch {
            width: Patch::Set(F64(22.5)),
            custom_width: Some(true),
            ..ColPatch::default()
        },
    })
    .expect("cols");
    wb.apply(&Command::SetRowProps {
        sheet: s,
        at: 4,
        count: 1,
        patch: RowPatch {
            height: Patch::Set(F64(30.0)),
            custom_height: Some(true),
            hidden: Some(true),
            ..RowPatch::default()
        },
    })
    .expect("rows");
    wb.apply(&Command::DefineName {
        name: "Edited".into(),
        formula: "Comments!$A$1:$A$3".into(),
        scope: Some(s),
        hidden: false,
        comment: Some("from Kiln".into()),
    })
    .expect("name");
    let (bytes, _) = save_and_reread(&r, "meta");
    let pkg = OpcPackage::open(&bytes).expect("pkg");
    let rels = pkg
        .relationships("/xl/worksheets/sheet1.xml")
        .expect("rels");
    assert!(
        rels.all()
            .iter()
            .any(|r| r.target == "https://example.org/kiln?a=1&b=2")
    );
}

#[test]
fn saving_as_xlsx_drops_macros_and_xlsm_keeps_them() {
    // A macro-enabled copy of a fixture: a VBA part and the xlsm main content type.
    let mut pkg = OpcPackage::open(&fixture("multi-sheet.xlsx")).expect("pkg");
    pkg.set_content_type("/xl/workbook.xml", XlsxKind::MacroEnabled.content_type())
        .expect("ct");
    pkg.add_part(
        "/xl/vbaProject.bin",
        "application/vnd.ms-office.vbaProject",
        b"\xd0\xcf\x11\xe0fake vba".to_vec(),
    )
    .expect("vba");
    pkg.add_relationship(
        "/xl/workbook.xml",
        "http://schemas.microsoft.com/office/2006/relationships/vbaProject",
        "/xl/vbaProject.bin",
        kiln_ooxml::opc::RelationshipOptions::default(),
    )
    .expect("rel");
    let bytes = pkg.save().expect("save");
    let r = read_xlsx(&bytes).expect("read");
    assert_eq!(r.source.kind(), XlsxKind::MacroEnabled);
    let keep = write_xlsx(&r.workbook, Some(&r.source), &WriteOptions::default()).expect("xlsm");
    let out = OpcPackage::open(&keep.bytes).expect("pkg");
    assert!(out.has("/xl/vbaProject.bin"));
    assert_eq!(
        out.part_bytes("/xl/vbaProject.bin").expect("vba").as_ref(),
        b"\xd0\xcf\x11\xe0fake vba"
    );
    let drop = write_xlsx(
        &r.workbook,
        Some(&r.source),
        &WriteOptions {
            kind: Some(XlsxKind::Workbook),
            ..WriteOptions::default()
        },
    )
    .expect("xlsx");
    let out = OpcPackage::open(&drop.bytes).expect("pkg");
    assert!(!out.has("/xl/vbaProject.bin"));
    assert_eq!(
        out.content_type("/xl/workbook.xml"),
        Some(XlsxKind::Workbook.content_type())
    );
    assert!(drop.issues.iter().any(|i| i.contains("macros")));
    assert!(read_xlsx(&drop.bytes).is_ok());
}

#[test]
fn document_properties_are_stamped_when_asked() {
    let r = open("styles.xlsx");
    let w = write_xlsx(
        &r.workbook,
        Some(&r.source),
        &WriteOptions {
            now: Some("2026-10-16T09:30:00Z".into()),
            author: Some("Kiln Test".into()),
            ..WriteOptions::default()
        },
    )
    .expect("write");
    let pkg = OpcPackage::open(&w.bytes).expect("pkg");
    let props = kiln_ooxml::props::read_document_properties(&pkg).expect("props");
    assert_eq!(props.core.modified.as_deref(), Some("2026-10-16T09:30:00Z"));
    assert_eq!(props.core.last_modified_by.as_deref(), Some("Kiln Test"));
    assert_eq!(props.app.application.as_deref(), Some("Kiln"));
}
