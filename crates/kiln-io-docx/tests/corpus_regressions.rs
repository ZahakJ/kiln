// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content real-world files carry that an unedited round trip must keep. Each test comes from a
//! file of the fidelity corpus that lost it; the fixtures reproduce the construct.

use kiln_io_docx::{WriteOptions, rewrite_docx};
use kiln_ooxml::opc::OpcPackage;

fn round_trip(rel: &str, part: &str) -> String {
    let path = format!("{}/../../fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).expect("fixture");
    let written = rewrite_docx(&bytes, &WriteOptions::default()).expect("round trip");
    let pkg = OpcPackage::open(&written.bytes).expect("package");
    let data = pkg.part(part).expect("part").expect("bytes");
    String::from_utf8(data.to_vec()).expect("utf-8")
}

#[test]
fn a_footer_that_ends_in_a_content_control_gets_no_extra_paragraph() {
    let xml = round_trip("docx/features/footer-control.docx", "/word/footer1.xml");
    assert!(xml.contains("</w:sdt></w:ftr>"), "{xml}");
}

#[test]
fn ruby_keeps_its_guide_text() {
    let xml = round_trip("docx/features/cjk-document.docx", "/word/document.xml");
    for tag in ["<w:ruby>", "<w:rubyPr>", "<w:rt>", "<w:rubyBase>", "かんじ"] {
        assert!(xml.contains(tag), "{tag}: {xml}");
    }
}

#[test]
fn tracked_moves_stay_moves() {
    let xml = round_trip(
        "docx/features/tracked-formatting.docx",
        "/word/document.xml",
    );
    for tag in [
        "<w:moveFromRangeStart",
        "<w:moveFrom ",
        "<w:moveFromRangeEnd",
        "<w:moveToRangeStart",
        "<w:moveTo ",
        "<w:moveToRangeEnd",
    ] {
        assert!(xml.contains(tag), "{tag}: {xml}");
    }
    assert_eq!(xml.matches("<w:del ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<w:ins ").count(), 1, "{xml}");
    // Moved-away text stays in `w:t`.
    assert!(
        xml.contains("<w:t>Moved away.</w:t></w:r></w:moveFrom>"),
        "{xml}"
    );
}

#[test]
fn form_fields_keep_their_settings() {
    let xml = round_trip("docx/features/form-fields.docx", "/word/document.xml");
    assert_eq!(xml.matches("<w:ffData>").count(), 2, "{xml}");
    assert!(xml.contains("<w:maxLength w:val=\"20\"/>"), "{xml}");
    assert!(xml.contains("<w:checkBox>"), "{xml}");
}

#[test]
fn permission_ranges_are_kept() {
    let xml = round_trip("docx/features/form-fields.docx", "/word/document.xml");
    assert_eq!(xml.matches("<w:permStart").count(), 2, "{xml}");
    assert_eq!(xml.matches("<w:permEnd").count(), 2, "{xml}");
    assert!(xml.contains("w:edGrp=\"everyone\""), "{xml}");
    // A marker is not run content.
    assert!(!xml.contains("<w:r><w:permStart"), "{xml}");
}

#[test]
fn alignment_tabs_stay_alignment_tabs() {
    let xml = round_trip("docx/features/form-fields.docx", "/word/document.xml");
    assert_eq!(xml.matches("<w:ptab ").count(), 2, "{xml}");
    assert!(xml.contains("w:alignment=\"center\""), "{xml}");
}

#[test]
fn a_part_named_under_another_part_is_left_out() {
    // OPC forbids it, and LibreOffice refuses such a package.
    let path = format!(
        "{}/../../fixtures/docx/manual/minimal.docx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    pkg.add_part(
        "/word/document.xml/image1.gif",
        "image/gif",
        b"GIF89a".to_vec(),
    )
    .expect("part");
    let bytes = pkg.save().expect("save");
    let written = rewrite_docx(&bytes, &WriteOptions::default()).expect("round trip");
    let out = OpcPackage::open(&written.bytes).expect("package");
    assert!(!out.all_part_names().any(|n| n.contains(".xml/")));
    assert!(written.dropped.iter().any(|n| n.contains(".xml/")));
}

#[test]
fn a_folder_named_like_an_xml_part_is_no_reason_to_drop_a_part() {
    let path = format!(
        "{}/../../fixtures/docx/manual/minimal.docx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    let name = "/word/embeddings/report.xml/oleObject1.bin";
    pkg.add_part(name, "application/vnd.ms-office.oleObject", b"OLE".to_vec())
        .expect("part");
    pkg.add_relationship(
        "/word/document.xml",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject",
        "embeddings/report.xml/oleObject1.bin",
        Default::default(),
    )
    .expect("relationship");
    let bytes = pkg.save().expect("save");
    let written = rewrite_docx(&bytes, &WriteOptions::default()).expect("round trip");
    let out = OpcPackage::open(&written.bytes).expect("package");
    assert!(out.has(name), "{:?}", written.dropped);
    assert!(!written.dropped.iter().any(|n| n == name));
}

/// `minimal.docx` with another body, read and rewritten.
fn with_body(body: &str) -> (kiln_io_docx::DocxRead, String) {
    let path = format!(
        "{}/../../fixtures/docx/manual/minimal.docx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>"
    );
    pkg.set_part("/word/document.xml", xml.into_bytes())
        .expect("part");
    let bytes = pkg.save().expect("save");
    let read = kiln_io_docx::read_docx(&bytes).expect("read");
    let written = rewrite_docx(&bytes, &WriteOptions::default()).expect("round trip");
    let out = OpcPackage::open(&written.bytes).expect("package");
    let data = out.part_bytes("/word/document.xml").expect("part");
    (read, String::from_utf8(data.to_vec()).expect("utf-8"))
}

fn field(result: &str) -> String {
    format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> REF bm \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{result}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>"
    )
}

const RUBY: &str = "<w:r><w:ruby><w:rubyPr><w:hps w:val=\"12\"/></w:rubyPr><w:rt><w:r><w:t>guide</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>base</w:t></w:r></w:rubyBase></w:ruby></w:r>";

#[test]
fn a_field_keeps_its_code_with_ruby_in_its_result() {
    let (_, xml) = with_body(&field(&format!("<w:r><w:t>a </w:t></w:r>{RUBY}")));
    assert!(xml.contains("REF bm"), "{xml}");
    assert!(xml.contains("<w:ruby>") && xml.contains("guide"), "{xml}");
    assert!(xml.contains("a "), "{xml}");
}

#[test]
fn a_field_keeps_its_code_with_an_alignment_tab_in_its_result() {
    let tab = "<w:r><w:ptab w:relativeTo=\"margin\" w:alignment=\"right\" w:leader=\"dot\"/></w:r>";
    let (_, xml) = with_body(&field(&format!(
        "<w:r><w:t>left</w:t></w:r>{tab}<w:r><w:t>right</w:t></w:r>"
    )));
    assert!(xml.contains("REF bm"), "{xml}");
    assert!(xml.contains("<w:ptab "), "{xml}");
    assert!(xml.find("left").unwrap() < xml.find("<w:ptab").unwrap());
    assert!(xml.find("<w:ptab").unwrap() < xml.find("right").unwrap());
}

#[test]
fn a_field_keeps_its_code_with_a_permission_range_in_its_result() {
    let (_, xml) = with_body(&field(
        "<w:permStart w:id=\"9\" w:edGrp=\"everyone\"/><w:r><w:t>guarded</w:t></w:r><w:permEnd w:id=\"9\"/>",
    ));
    assert!(xml.contains("REF bm"), "{xml}");
    assert!(
        xml.contains("<w:permStart") && xml.contains("<w:permEnd"),
        "{xml}"
    );
    assert!(!xml.contains("<w:r><w:permStart"), "{xml}");
}

#[test]
fn a_simple_field_keeps_ruby_in_its_result() {
    let (_, xml) = with_body(&format!(
        "<w:p><w:fldSimple w:instr=\" REF x \"><w:r><w:t>simple </w:t></w:r>{RUBY}</w:fldSimple></w:p>"
    ));
    assert!(
        xml.contains("fldSimple") && xml.contains("<w:ruby>"),
        "{xml}"
    );
}

#[test]
fn ruby_in_a_field_code_keeps_its_base_text() {
    let (read, xml) = with_body(&format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> IF 1=1 \"</w:instrText></w:r>{RUBY}<w:r><w:instrText xml:space=\"preserve\">\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>base</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>"
    ));
    assert!(xml.contains("\"base\""), "{xml}");
    assert_eq!(read.warnings.len(), 0, "{:?}", read.warnings);
}

#[test]
fn typing_before_a_form_field_keeps_its_settings() {
    let path = format!(
        "{}/../../fixtures/docx/features/form-fields.docx",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let pkg = OpcPackage::open(&bytes).expect("package");
    let read = kiln_io_docx::read_package(&pkg).expect("reads");
    let mut doc = read.document.clone();
    let first = doc
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(kiln_doc::Block::as_paragraph)
        .map(|p| p.id)
        .expect("a paragraph");
    let tx =
        kiln_doc::edit::insert_text(&mut doc, kiln_doc::Pos::new(first, 0), "Your ").expect("edit");
    doc.apply(tx).expect("applies");
    let source = kiln_io_docx::WriteSource::from_read(&read).with_package(&pkg);
    let written =
        kiln_io_docx::write_docx(&doc, Some(&source), &WriteOptions::default()).expect("writes");
    let out = OpcPackage::open(&written.bytes).expect("package");
    let xml = out.part_bytes("/word/document.xml").expect("part");
    let xml = String::from_utf8_lossy(&xml);
    assert_eq!(xml.matches("<w:ffData>").count(), 2, "{xml}");
    assert!(xml.contains("Your "), "{xml}");
    assert!(xml.find("<w:maxLength").unwrap() < xml.find("FORMTEXT").unwrap());
    assert!(xml.find("<w:checkBox>").unwrap() < xml.find("FORMCHECKBOX").unwrap());
}
