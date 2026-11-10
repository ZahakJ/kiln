// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content real-world workbooks carry that the reader must see. From files of the fidelity corpus.

use kiln_io_xlsx::{WriteOptions, rewrite_xlsx};
use kiln_ooxml::opc::OpcPackage;

#[test]
fn shared_strings_of_the_2006_preview_format_are_read() {
    let path = format!(
        "{}/../../fixtures/xlsx/manual/beta-strings.xlsx",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    let pkg = OpcPackage::open(&written.bytes).expect("package");
    let strings = pkg.part_bytes("/xl/sharedStrings.xml").expect("strings");
    let strings = String::from_utf8_lossy(&strings);
    assert!(
        strings.contains("Lorem") && strings.contains("ipsum"),
        "{strings}"
    );
    let sheet = pkg.part_bytes("/xl/worksheets/sheet1.xml").expect("sheet");
    let sheet = String::from_utf8_lossy(&sheet);
    assert!(
        sheet.contains("r=\"A1\"") && sheet.contains("r=\"B1\""),
        "{sheet}"
    );
}

#[test]
fn a_sheet_without_format_properties_gets_none() {
    // Writing Excel's default row height where the file named none changes how other readers
    // size its rows, and with them its pages.
    let path = format!(
        "{}/../../fixtures/xlsx/manual/minimal.xlsx",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    let pkg = OpcPackage::open(&written.bytes).expect("package");
    let sheet = pkg.part_bytes("/xl/worksheets/sheet1.xml").expect("sheet");
    let sheet = String::from_utf8_lossy(&sheet);
    assert!(!sheet.contains("<sheetFormatPr"), "{sheet}");
}

/// `minimal.xlsx` with other sheet data (and calculation settings) as written bytes.
fn workbook(cells: &str, calc: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../fixtures/xlsx/manual/minimal.xlsx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    let sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{cells}</sheetData></worksheet>"
    );
    pkg.set_part("/xl/worksheets/sheet1.xml", sheet.into_bytes())
        .expect("sheet");
    if !calc.is_empty() {
        let wb = String::from_utf8(pkg.part_bytes("/xl/workbook.xml").expect("wb").to_vec())
            .expect("utf-8");
        let at = wb.find("</workbook>").expect("end");
        let wb = format!("{}{calc}{}", &wb[..at], &wb[at..]);
        pkg.set_part("/xl/workbook.xml", wb.into_bytes())
            .expect("wb");
    }
    pkg.save().expect("save")
}

fn sheet_xml(bytes: &[u8]) -> String {
    let pkg = OpcPackage::open(bytes).expect("package");
    let xml = pkg.part_bytes("/xl/worksheets/sheet1.xml").expect("sheet");
    String::from_utf8_lossy(&xml).into_owned()
}

fn number(read: &kiln_io_xlsx::XlsxRead, cell: (u32, u32)) -> String {
    let id = read.workbook.first_sheet().expect("sheet");
    format!("{:?}", read.workbook.value(id, cell.0, cell.1))
}

const UNKNOWN: &str = "<row r=\"1\"><c r=\"A1\"><v>2</v></c><c r=\"B1\"><f>NotAFunctionKilnKnows(A1)</f><v>42</v></c><c r=\"C1\"><f>B1+1</f><v>43</v></c><c r=\"D1\"><v>9</v></c></row>";

#[test]
fn a_function_kiln_cannot_evaluate_keeps_the_stored_result() {
    let bytes = workbook(UNKNOWN, "");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    assert!(
        number(&read, (0, 1)).contains("42"),
        "{}",
        number(&read, (0, 1))
    );
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    let xml = sheet_xml(&written.bytes);
    assert!(
        xml.contains("NotAFunctionKilnKnows(A1)</f><v>42</v>"),
        "{xml}"
    );
    assert!(xml.contains("B1+1</f><v>43</v>"), "{xml}");
}

#[test]
fn a_stored_result_stays_until_something_it_depends_on_changes() {
    use kiln_sheet::commands::{CellInput, Command};
    let bytes = workbook(UNKNOWN, "");
    let mut read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    let id = read.workbook.first_sheet().expect("sheet");
    let set = |row, col, n: f64| Command::SetCells {
        sheet: id,
        cells: vec![CellInput::value(row, col, n)],
    };
    // Something else changes: the result stays, and a dependent reads it.
    read.workbook.apply(&set(0, 3, 10.0)).expect("edit");
    assert!(number(&read, (0, 1)).contains("42"));
    // What it depends on changes: Kiln can no longer vouch for it.
    read.workbook.apply(&set(0, 0, 3.0)).expect("edit");
    assert!(
        number(&read, (0, 1)).contains("Name"),
        "{}",
        number(&read, (0, 1))
    );
}

#[test]
fn stored_results_survive_an_unedited_save_even_when_they_look_wrong() {
    let cells = "<row r=\"1\"><c r=\"A1\"><f>1+1</f><v>5</v></c><c r=\"B1\"><f>A1+1</f></c></row>";
    let bytes = workbook(cells, "");
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    let xml = sheet_xml(&written.bytes);
    assert!(xml.contains("1+1</f><v>5</v>"), "{xml}");
    // A formula saved without a result gets one.
    assert!(xml.contains("A1+1</f><v>"), "{xml}");
}

#[test]
fn full_calc_on_load_recalculates() {
    let cells = "<row r=\"1\"><c r=\"A1\"><f>1+1</f><v>5</v></c></row>";
    let bytes = workbook(cells, "<calcPr fullCalcOnLoad=\"1\"/>");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    assert!(
        number(&read, (0, 0)).contains('2'),
        "{}",
        number(&read, (0, 0))
    );
}

#[test]
fn a_string_table_no_cell_uses_stays_a_part_of_the_package() {
    let path = format!(
        "{}/../../fixtures/xlsx/manual/beta-strings.xlsx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    pkg.set_part(
        "/xl/worksheets/sheet1.xml",
        b"<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>".to_vec(),
    )
    .expect("sheet");
    let bytes = pkg.save().expect("save");
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    assert!(written.dropped.is_empty(), "{:?}", written.dropped);
}

#[test]
fn a_string_table_nothing_refers_to_is_carried_over() {
    let numbers = workbook("<row r=\"1\"><c r=\"A1\"><v>1</v></c></row>", "");
    let mut pkg = OpcPackage::open(&numbers).expect("package");
    pkg.add_part(
        "/xl/sharedStrings.xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
        b"<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"0\" uniqueCount=\"0\"/>".to_vec(),
    )
    .expect("part");
    let bytes = pkg.save().expect("save");
    let written = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("round trip");
    assert!(written.dropped.is_empty(), "{:?}", written.dropped);
}
