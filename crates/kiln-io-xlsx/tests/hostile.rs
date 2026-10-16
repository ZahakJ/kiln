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

/// A worksheet whose A1 holds `formula` (escaped) and whose B1 holds a plain number.
fn with_formula(formula: &str) -> Vec<u8> {
    let escaped = formula
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let xml = format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>{escaped}</f><v>1</v></c><c r="B1"><v>2</v></c></row></sheetData></worksheet>"#
    );
    with_part(
        "multi-sheet.xlsx",
        "/xl/worksheets/sheet1.xml",
        xml.as_bytes(),
    )
}

/// Deeply nested formulas (far beyond Excel's 64 levels and 8,192 characters) are saved in
/// linear time and memory, with and without the source package, and read back unchanged.
#[test]
fn deep_formulas_save_without_blowing_up() {
    let deep = |open: &str, inner: &str, depth: usize| {
        format!("{}{inner}{}", open.repeat(depth), ")".repeat(depth))
    };
    for formula in [
        deep("(", "1", 100_000),
        deep("ABS(", "1", 50_000),
        deep("XLOOKUP(", "1", 20_000),
        // Parameters in scope across tens of thousands of open frames.
        format!("LET(x,1,{})", deep("SUM(x,", "LAMBDA(y,x+y)(x)", 30_000)),
        // Unbalanced in both directions.
        "(".repeat(60_000),
        ")".repeat(60_000),
    ] {
        let bytes = with_formula(&formula);
        let r = read_xlsx(&bytes).expect("a deep formula is read");
        for source in [Some(&r.source), None] {
            let w = write_xlsx(&r.workbook, source, &WriteOptions::default())
                .expect("a deep formula is written");
            let again = read_xlsx(&w.bytes).expect("and read again");
            let s = again.workbook.sheets()[0].id();
            assert_eq!(
                again.workbook.value(s, 0, 1),
                kiln_sheet::CellValue::Number(2.0)
            );
        }
    }
    // The prefixing pass itself stays linear on a 200 KB formula.
    let formula = deep("(", "1", 100_000);
    let started = std::time::Instant::now();
    let file = kiln_io_xlsx::formula_text::to_file_formula(&formula, None);
    assert_eq!(file, formula);
    let file = kiln_io_xlsx::formula_text::to_file_formula(
        &format!("LET(x,1,{})", deep("SUM(x,", "x", 30_000)),
        None,
    );
    assert!(file.starts_with("_xlfn.LET(_xlpm.x,1,SUM(_xlpm.x,"));
    assert!(file.contains("SUM(_xlpm.x,_xlpm.x)"));
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}
