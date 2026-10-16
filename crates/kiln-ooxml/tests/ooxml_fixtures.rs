// SPDX-License-Identifier: AGPL-3.0-or-later
//! The hand-made edge-case packages in `fixtures/ooxml/` (built by
//! `fixtures/ooxml/make-fixtures.py`).

mod common;

use common::{fixture, unzip};
use kiln_ooxml::opc::{OpcError, OpcPackage, content_type, rel_type};
use kiln_ooxml::xml::XmlNode;

/// open → save → every part byte-identical (inflated and compressed), Office's entry order, and a
/// second save equal to the first.
fn assert_repacks(name: &str) {
    let bytes = fixture(name);
    let src = unzip(&bytes);
    let pkg = OpcPackage::open(&bytes).unwrap();
    let saved = pkg.save().unwrap();
    let out = unzip(&saved);
    assert_eq!(out[0].name, "[Content_Types].xml", "{name}");
    assert_eq!(out[1].name, "_rels/.rels", "{name}");
    assert_eq!(out.len(), src.len(), "{name}");
    for e in &src {
        let o = out.iter().find(|o| o.name == e.name).unwrap();
        assert!(o.data == e.data && o.raw == e.raw, "{name}: {}", e.name);
    }
    assert_eq!(
        OpcPackage::open(&saved).unwrap().save().unwrap(),
        saved,
        "{name}"
    );
}

#[test]
fn a_package_without_content_types_is_refused() {
    assert_eq!(
        OpcPackage::open(&fixture("ooxml/missing-content-types.docx")).unwrap_err(),
        OpcError::MissingContentTypes
    );
}

#[test]
fn an_encrypted_or_legacy_file_is_recognised() {
    assert_eq!(
        OpcPackage::open(&fixture("ooxml/encrypted.docx")).unwrap_err(),
        OpcError::EncryptedOrLegacy
    );
}

#[test]
fn absolute_and_unusual_relationship_targets_resolve() {
    assert_repacks("ooxml/absolute-rels.docx");
    let pkg = OpcPackage::open(&fixture("ooxml/absolute-rels.docx")).unwrap();
    assert_eq!(
        pkg.main_document_part().unwrap().as_deref(),
        Some("/word/document.xml")
    );
    let doc = "/word/document.xml";
    assert_eq!(
        pkg.related_part(doc, rel_type::STYLES).unwrap().as_deref(),
        Some("/word/styles.xml")
    );
    let images = pkg.related_parts(doc, rel_type::IMAGE).unwrap();
    assert_eq!(
        images,
        [
            "/word/media/image1.png",
            "/word/media/my image.png",
            "/word/media/image3.png",
            "/word/media/missing.png"
        ]
    );
    for img in &images[..3] {
        assert!(pkg.has(img), "{img}");
        assert_eq!(pkg.content_type(img), Some(content_type::PNG));
    }
    // A dangling relationship is reported as a name, not an error; the part simply is absent.
    assert!(!pkg.has(&images[3]));
    let rels = pkg.relationships(doc).unwrap();
    let link = rels.get("rId5").unwrap();
    assert!(link.is_external());
    assert_eq!(link.target, "https://example.org/a?b=c#d");
    assert_eq!(rels.resolve(link), None);
}

#[test]
fn parts_in_unusual_encodings_decode() {
    assert_repacks("ooxml/encodings.docx");
    let pkg = OpcPackage::open(&fixture("ooxml/encodings.docx")).unwrap();
    let doc = pkg.part_xml("/word/document.xml").unwrap().unwrap();
    assert_eq!(doc.root.text(), "UTF-16 «ünïcödé» ✓");
    assert_eq!(
        doc.declaration.as_ref().unwrap().encoding.as_deref(),
        Some("UTF-16")
    );
    // Written back, the part is UTF-8 and says so.
    let utf8 = kiln_ooxml::xml::document_to_bytes(&doc).unwrap();
    assert!(utf8.starts_with(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#));
    assert_eq!(kiln_ooxml::xml::parse_xml(&utf8).unwrap().root, doc.root);
    let styles = pkg.part_xml("/word/styles.xml").unwrap().unwrap().root;
    let style = styles.child("w:style").unwrap();
    assert_eq!(style.attr("w:styleId"), Some("Café"));
    assert_eq!(
        style.child("w:name").unwrap().attr("w:val"),
        Some("Café €uro")
    );
    let settings = pkg.part_xml("/word/settings.xml").unwrap().unwrap().root;
    assert_eq!(settings.name, "w:settings");
    let footnotes = pkg.part_xml("/word/footnotes.xml").unwrap().unwrap().root;
    assert_eq!(footnotes.first_element().unwrap().attr("w:id"), Some("1"));
}

#[test]
fn case_differences_and_entry_order_are_tolerated() {
    assert_repacks("ooxml/case-and-order.xlsx");
    let pkg = OpcPackage::open(&fixture("ooxml/case-and-order.xlsx")).unwrap();
    let main = pkg.main_document_part().unwrap().unwrap();
    assert_eq!(main, "/xl/workbook.xml");
    assert!(
        pkg.has(&main),
        "found despite the entry being xl/Workbook.xml"
    );
    assert_eq!(
        pkg.content_type(&main),
        Some(content_type::XLSX_WORKBOOK),
        "override PartName in another case"
    );
    assert_eq!(
        pkg.content_type("/xl/worksheets/sheet1.xml"),
        Some(content_type::XLSX_WORKSHEET)
    );
    // The upper-case RELS default applies to lower-case .rels parts.
    assert_eq!(
        pkg.content_type("/_rels/.rels"),
        Some(content_type::RELATIONSHIPS)
    );
    let sheets = pkg.related_parts(&main, rel_type::WORKSHEET).unwrap();
    assert_eq!(sheets, ["/xl/worksheets/sheet1.xml"]);
    assert_eq!(
        pkg.part_xml(&sheets[0]).unwrap().unwrap().root.text(),
        "case"
    );
    // Directory entries are not parts.
    assert!(pkg.all_part_names().all(|n| !n.ends_with('/')));
}

#[test]
fn strict_packages_resolve_through_transitional_names() {
    assert_repacks("ooxml/strict.xlsx");
    let pkg = OpcPackage::open(&fixture("ooxml/strict.xlsx")).unwrap();
    let main = pkg.main_document_part().unwrap().unwrap();
    let wb = pkg.part_xml(&main).unwrap().unwrap().root;
    assert_eq!(wb.name, "workbook");
    assert_eq!(wb.attr("conformance"), Some("strict"));
    assert_eq!(
        wb.child_path(&["sheets", "sheet"]).unwrap().attr("r:id"),
        Some("rId1")
    );
    assert_eq!(
        pkg.related_parts(&main, rel_type::WORKSHEET).unwrap(),
        ["/xl/worksheets/sheet1.xml"]
    );
}

#[test]
fn hostile_xml_is_refused_without_harm_and_still_repacks() {
    assert_repacks("ooxml/deep-nesting.docx");
    let pkg = OpcPackage::open(&fixture("ooxml/deep-nesting.docx")).unwrap();
    let err = pkg.part_xml("/word/document.xml").unwrap_err();
    assert!(err.to_string().contains("nested"), "{err}");

    assert_repacks("ooxml/billion-laughs.docx");
    let pkg = OpcPackage::open(&fixture("ooxml/billion-laughs.docx")).unwrap();
    let err = pkg.part_xml("/word/document.xml").unwrap_err();
    assert!(err.to_string().contains("DTD"), "{err}");
}

#[test]
fn a_damaged_entry_fails_on_access_and_is_carried_verbatim() {
    let bytes = fixture("ooxml/corrupt-entry.docx");
    let pkg = OpcPackage::open(&bytes).unwrap();
    assert!(matches!(
        pkg.part("/word/document.xml"),
        Err(OpcError::CorruptZip(_))
    ));
    // The intact parts read fine.
    assert!(pkg.relationships("/").unwrap().len() == 1);
    // Saving copies the damaged entry's bytes untouched: Kiln never makes a file worse.
    let saved = pkg.save().unwrap();
    let raw_of = |b: &[u8]| {
        unzip_raw(b)
            .into_iter()
            .find(|(n, _)| n == "word/document.xml")
            .unwrap()
            .1
    };
    assert_eq!(raw_of(&saved), raw_of(&bytes));
}

/// Raw (compressed) entries, without inflating (one entry here cannot be inflated).
fn unzip_raw(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut f = archive.by_index_raw(i).unwrap();
            let mut raw = Vec::new();
            f.read_to_end(&mut raw).unwrap();
            (f.name().unwrap().into_owned(), raw)
        })
        .collect()
}

#[test]
fn text_nodes_of_the_edge_cases_are_plain_text() {
    let pkg = OpcPackage::open(&fixture("ooxml/absolute-rels.docx")).unwrap();
    let doc = pkg.part_xml("/word/document.xml").unwrap().unwrap();
    let t = doc.root.descendants("w:t")[0];
    assert!(matches!(&t.children[0], XmlNode::Text(x) if !x.cdata));
}
