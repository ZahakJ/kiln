// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hostile and damaged input: the reader returns an error or a workbook with warnings — never a
//! panic — and whatever it reads, the writer saves into a package that reads again.

use kiln_io_xlsx::{WriteOptions, read_xlsx, write_xlsx};
use kiln_ooxml::opc::OpcPackage;
use proptest::prelude::*;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        kiln_testing::fixtures::fixtures_dir()
            .join("xlsx/features")
            .join(name),
    )
    .expect("fixture")
}

/// A fixture with one part replaced.
fn with_part(name: &str, part: &str, bytes: &[u8]) -> Vec<u8> {
    let mut pkg = OpcPackage::open(&fixture(name)).expect("pkg");
    pkg.set_part(part, bytes.to_vec()).expect("part");
    pkg.save().expect("save")
}

fn survives(bytes: &[u8]) {
    if let Ok(r) = read_xlsx(bytes) {
        let w = write_xlsx(&r.workbook, Some(&r.source), &WriteOptions::default())
            .expect("a workbook that was read can be written");
        read_xlsx(&w.bytes).expect("and read again");
    }
}

#[test]
fn damaged_worksheets_are_errors_or_warnings() {
    let sheet = "/xl/worksheets/sheet1.xml";
    let head = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#;
    for (body, ok) in [
        // Unclosed, mismatched, DTD, unknown entity: refused.
        ("<row r=\"1\"><c r=\"A1\"><v>1</v></c>", false),
        (
            "<row r=\"1\"><c r=\"A1\"><v>1</v></row></sheetData></worksheet>",
            false,
        ),
        (
            "<row><c r=\"A1\" t=\"s\"><v>&bogus;</v></c></row></sheetData></worksheet>",
            false,
        ),
        // Odd but readable: warnings.
        (
            "<row r=\"0\"><c r=\"ZZZZ1\"><v>1</v></c><c r=\"A1\" s=\"99999\"><v>x</v></c><c t=\"s\"><v>9999</v></c><c t=\"d\"><v>not a date</v></c><c t=\"e\"><v>#WHAT?</v></c></row><row r=\"99999999\"><c r=\"B2\"><f t=\"shared\" si=\"7\"/></c><c r=\"C2\"><f t=\"array\" ref=\"bad\">1</f></c></row></sheetData></worksheet>",
            true,
        ),
        (
            "<row r=\"1\"><c r=\"A1\"><is><r><rPr><b/></rPr><t>x</t></r></is></c><c r=\"B1\"><v>1e999</v></c><c r=\"C1\"><f>SUM(</f><v>3</v></c></row></sheetData></worksheet>",
            true,
        ),
    ] {
        let xml = format!("{head}{body}");
        let bytes = with_part("multi-sheet.xlsx", sheet, xml.as_bytes());
        let r = read_xlsx(&bytes);
        assert_eq!(r.is_ok(), ok, "{body}: {:?}", r.as_ref().err());
        if let Ok(r) = r {
            assert!(!r.warnings.is_empty(), "{body}");
        }
        survives(&bytes);
    }
    let laughs = r#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;">]><worksheet><sheetData><row><c><v>&lol2;</v></c></row></sheetData></worksheet>"#;
    assert!(read_xlsx(&with_part("multi-sheet.xlsx", sheet, laughs.as_bytes())).is_err());
    // Unknown markup inside a cell is skipped; absurd nesting is refused (no recursion).
    let nested = |depth: usize| {
        format!(
            "{head}<row r=\"1\"><c r=\"A1\">{}{}<v>5</v></c></row></sheetData></worksheet>",
            "<x>".repeat(depth),
            "</x>".repeat(depth)
        )
    };
    let r = read_xlsx(&with_part(
        "multi-sheet.xlsx",
        sheet,
        nested(100).as_bytes(),
    ))
    .expect("nested");
    let s = r.workbook.sheets()[0].id();
    assert_eq!(
        r.workbook.value(s, 0, 0),
        kiln_sheet::CellValue::Number(5.0)
    );
    assert!(
        read_xlsx(&with_part(
            "multi-sheet.xlsx",
            sheet,
            nested(50_000).as_bytes()
        ))
        .is_err()
    );
}

#[test]
fn damaged_parts_around_the_sheets() {
    for (part, xml) in [
        (
            "/xl/styles.xml",
            "<styleSheet><cellXfs><xf fontId=\"7\"/></cellXfs></styleSheet>",
        ),
        ("/xl/styles.xml", "<notStyles/>"),
        (
            "/xl/workbook.xml",
            "<workbook><sheets><sheet name=\"a:b\" sheetId=\"1\" r:id=\"rId1\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/><sheet name=\"A:B\" r:id=\"rId2\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/></sheets><definedNames><definedName name=\"bad name\">1</definedName><definedName name=\"x\" localSheetId=\"42\">1</definedName></definedNames></workbook>",
        ),
        (
            "/xl/theme/theme1.xml",
            "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/>",
        ),
    ] {
        let bytes = with_part("multi-sheet.xlsx", part, xml.as_bytes());
        survives(&bytes);
    }
    // Not a workbook at all.
    assert!(read_xlsx(b"").is_err());
    assert!(read_xlsx(b"PK\x03\x04garbage").is_err());
    assert!(
        read_xlsx(
            &std::fs::read(kiln_testing::fixtures::fixtures_dir().join("docx/manual/minimal.docx"))
                .expect("docx")
        )
        .is_err()
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random edits to a worksheet's XML (truncation, inserted bytes) never panic the reader.
    #[test]
    fn mutated_worksheets_never_panic(cut in 0usize..6000, at in 0usize..6000, junk in proptest::collection::vec(any::<u8>(), 0..12)) {
        let pkg = OpcPackage::open(&fixture("conditional-formatting.xlsx")).expect("pkg");
        let mut xml = pkg.part_bytes("/xl/worksheets/sheet1.xml").expect("sheet").into_owned();
        let at = at.min(xml.len());
        xml.splice(at..at, junk);
        let cut = cut.min(xml.len());
        if cut > 100 {
            xml.truncate(xml.len() - cut / 10);
        }
        let bytes = with_part("conditional-formatting.xlsx", "/xl/worksheets/sheet1.xml", &xml);
        survives(&bytes);
    }
}
