// SPDX-License-Identifier: AGPL-3.0-or-later
//! Documents are untrusted input (PLAN.md §3.12): the reader must never panic and never exhaust
//! the stack, whatever the bytes. Deep nesting is read on a 1 MiB stack (the web build's
//! default) in an unoptimised build.

mod read_support;

use kiln_io_docx::read_docx;
use kiln_testing::fixtures::find;
use proptest::prelude::*;
use read_support::{Build, build_docx, p, r};

/// Runs `f` on a thread with a small stack, as the wasm build has.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no stack overflow or panic");
}

fn nested(open: &str, close: &str, inner: &str, depth: usize) -> String {
    let mut s = String::with_capacity((open.len() + close.len()) * depth + inner.len());
    for _ in 0..depth {
        s.push_str(open);
    }
    s.push_str(inner);
    for _ in 0..depth {
        s.push_str(close);
    }
    s
}

fn read_nested(body: String) {
    on_small_stack(move || {
        let bytes = build_docx(Build {
            body,
            ..Build::default()
        });
        match read_docx(&bytes) {
            Ok(read) => read.document.validate().expect("valid"),
            // Beyond the XML parser's depth limit the main part is refused with a typed error.
            Err(e) => assert!(e.to_string().contains("nested deeper"), "{e}"),
        }
    });
}

#[test]
fn deeply_nested_tables_are_bounded() {
    let cell_open = "<w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc>";
    let cell_close = "<w:p/></w:tc></w:tr></w:tbl>";
    for depth in [10, 70, 600, 700] {
        read_nested(nested(cell_open, cell_close, &p(&r("deep")), depth));
    }
}

#[test]
fn deeply_nested_wrappers_are_bounded() {
    for depth in [100, 1_900, 2_100] {
        read_nested(nested(
            "<w:sdt><w:sdtContent>",
            "</w:sdtContent></w:sdt>",
            &p(&r("x")),
            depth / 2,
        ));
        read_nested(nested(
            "<w:customXml>",
            "</w:customXml>",
            &p(&r("x")),
            depth,
        ));
        read_nested(p(&nested(
            "<w:hyperlink w:anchor=\"a\">",
            "</w:hyperlink>",
            &r("x"),
            depth,
        )));
        read_nested(p(&nested("<w:smartTag>", "</w:smartTag>", &r("x"), depth)));
    }
}

#[test]
fn deeply_nested_text_boxes_are_bounded() {
    let open = "<w:p><w:r><w:pict><v:rect><v:textbox><w:txbxContent>";
    let close = "</w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>";
    for depth in [5, 80, 300] {
        read_nested(nested(open, close, &p(&r("x")), depth));
    }
}

#[test]
fn unbalanced_fields_and_markers_never_break_the_model() {
    let body = [
        p("<w:r><w:fldChar w:fldCharType=\"end\"/></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>"),
        p("<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText>TOC</w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>"),
        p("<w:bookmarkEnd w:id=\"9\"/><w:commentRangeEnd w:id=\"3\"/><w:r><w:t>text</w:t></w:r>"),
        "<w:fldSimple w:instr=\"PAGE\"><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r></w:fldSimple>".to_owned(),
        p("<w:fldSimple w:instr=\"TOC\"><w:r><w:t>t</w:t></w:r></w:fldSimple>"),
        "<w:tbl/>".to_owned(),
        "<w:tbl><w:tr/></w:tbl>".to_owned(),
        "<w:tbl><w:tr><w:tc><w:tcPr><w:vMerge/><w:gridSpan w:val=\"99999\"/></w:tcPr></w:tc></w:tr></w:tbl>".to_owned(),
        p("<w:r><w:sym w:font=\"Symbol\" w:char=\"zz\"/><w:sym/><w:footnoteReference w:id=\"7\"/></w:r>"),
        p("<w:r><w:t>\u{FFFC}</w:t></w:r>"),
    ]
    .concat();
    let read = read_docx(&build_docx(Build {
        body,
        ..Build::default()
    }))
    .expect("reads");
    read.document.validate().expect("valid");
    assert!(!read.warnings.is_empty());
}

/// The single table of a body that holds one table.
fn only_table(doc: &kiln_doc::Document) -> &kiln_doc::Table {
    let mut tables = doc
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter_map(|b| match b {
            kiln_doc::Block::Table(t) => Some(t),
            _ => None,
        });
    let table = tables.next().expect("a table");
    assert!(tables.next().is_none(), "one table");
    table
}

/// `w:gridBefore` / `w:gridAfter` far beyond Word's 63-column grid (up to `u32::MAX`) are
/// clamped, and completing ragged rows never overflows.
#[test]
fn huge_grid_before_and_after_are_clamped() {
    let cases = [
        // gridAfter = u32::MAX twice, the second row also skipping 10 columns: the ragged-row
        // completion used to add to u32::MAX.
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:trPr><w:gridAfter w:val=\"4294967295\"/></w:trPr><w:tc><w:p/></w:tc></w:tr><w:tr><w:trPr><w:gridBefore w:val=\"10\"/><w:gridAfter w:val=\"4294967295\"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>",
        // gridBefore of four billion: a table about four billion columns wide.
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:trPr><w:gridBefore w:val=\"4000000000\"/></w:trPr><w:tc><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>",
        // Both at the maximum on the same row, plus a ragged row and a cell-less row.
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:trPr><w:gridBefore w:val=\"4294967295\"/><w:gridAfter w:val=\"4294967295\"/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val=\"4294967295\"/></w:tcPr><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr><w:tr><w:trPr><w:gridBefore w:val=\"99999999999\"/></w:trPr></w:tr></w:tbl><w:p/>",
    ];
    for body in cases {
        let read = read_docx(&build_docx(Build {
            body: body.to_owned(),
            ..Build::default()
        }))
        .expect("reads");
        read.document.validate().expect("valid");
        let table = only_table(&read.document);
        // At most gridBefore + one cell's span + gridAfter, each within the 63-column limit.
        let width = table.grid_width();
        assert!(width <= 3 * 63, "grid width {width}");
        for row in &table.rows {
            assert!(row.props.grid_before.unwrap_or(0) <= 63, "{:?}", row.props);
            for cell in &row.cells {
                // A cell-less row is filled by one cell across the whole table.
                assert!(
                    (1..=width).contains(&(cell.grid_span as usize)),
                    "{}",
                    cell.grid_span
                );
            }
        }
        // Every row is completed to the same width.
        for row in &table.rows {
            let w = row.props.grid_before.unwrap_or(0)
                + row.cells.iter().map(|c| c.grid_span).sum::<u32>()
                + row.props.grid_after.unwrap_or(0);
            assert_eq!(w as usize, width, "{:?}", row.props);
        }
    }
}

fn fixture(name: &str) -> Vec<u8> {
    find(name).and_then(|f| f.read()).expect("fixture")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Flipping bytes of a real document either fails with a typed error or reads a valid
    /// document; it never panics.
    #[test]
    fn corrupted_bytes_never_panic(
        name in prop::sample::select(vec!["fields.docx", "tables.docx", "comments.docx", "text-boxes.docx"]),
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..16),
    ) {
        let mut bytes = fixture(name);
        for (i, b) in flips {
            let at = i.index(bytes.len());
            bytes[at] = b;
        }
        if let Ok(read) = read_docx(&bytes) {
            prop_assert!(read.document.validate().is_ok());
        }
    }

    /// Arbitrary text inside `w:t` and attribute values never breaks the model.
    #[test]
    fn arbitrary_text_and_values_read_into_valid_documents(
        text in "[^<>&\"]{0,40}",
        val in "[a-zA-Z0-9%.\\-]{0,12}",
    ) {
        let body = format!(
            "<w:p><w:pPr><w:ind w:left=\"{val}\" w:hanging=\"{val}\"/><w:spacing w:line=\"{val}\"/><w:jc w:val=\"{val}\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"{val}\"/><w:u w:val=\"{val}\"/><w:color w:val=\"{val}\"/></w:rPr><w:t>{text}</w:t><w:br w:type=\"{val}\"/></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w=\"{val}\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"{val}\"/><w:tcW w:w=\"{val}\" w:type=\"pct\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"
        );
        let read = read_docx(&build_docx(Build { body, ..Build::default() }));
        let read = read.expect("reads");
        prop_assert!(read.document.validate().is_ok());
    }
}
