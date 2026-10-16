// SPDX-License-Identifier: AGPL-3.0-or-later
//! OPC: the repack guarantee over the fixture corpus, package editing, limits and part names.
//! Ported from `reference/ts-core/ooxml/test/opc.test.ts`, plus the repack test of WP-55.

mod common;

use common::{all_feature_fixtures, feature_fixtures, fixture, make_zip, unzip};
use kiln_ooxml::opc::{
    ContentTypes, OpcError, OpcLimits, OpcPackage, RelationshipOptions, content_type, extension_of,
    rel_type, rel_type_matches, relative_target, rels_part_name, resolve_target,
    source_part_of_rels, validate_part_name,
};
use kiln_ooxml::xml::{XmlElement, XmlNode};

/// The archive order Office uses: `[Content_Types].xml`, `_rels/.rels`, then the rest as found.
fn expected_order(names: &[String]) -> Vec<String> {
    let mut out = vec!["[Content_Types].xml".to_owned()];
    if names.iter().any(|n| n == "_rels/.rels") {
        out.push("_rels/.rels".to_owned());
    }
    out.extend(
        names
            .iter()
            .filter(|n| *n != "[Content_Types].xml" && *n != "_rels/.rels")
            .cloned(),
    );
    out
}

/// open → save (no edits): every part's bytes identical (inflated and compressed), the entry
/// order stable, and a second save identical to the first.
fn assert_repack_identical(name: &str, original: &[u8]) {
    let src = unzip(original);
    let pkg = OpcPackage::open(original).unwrap_or_else(|e| panic!("{name}: {e}"));
    let saved = pkg.save().unwrap_or_else(|e| panic!("{name}: {e}"));
    let out = unzip(&saved);
    let src_names: Vec<String> = src.iter().map(|e| e.name.clone()).collect();
    let out_names: Vec<String> = out.iter().map(|e| e.name.clone()).collect();
    assert_eq!(out_names, expected_order(&src_names), "{name}: entry order");
    for e in &src {
        let o = out
            .iter()
            .find(|o| o.name == e.name)
            .unwrap_or_else(|| panic!("{name}: {} missing", e.name));
        assert!(o.data == e.data, "{name}: {} changed", e.name);
        assert!(o.raw == e.raw, "{name}: {} recompressed", e.name);
    }
    let reopened = OpcPackage::open(&saved).unwrap();
    assert_eq!(
        reopened.all_part_names().collect::<Vec<_>>(),
        pkg.all_part_names().collect::<Vec<_>>(),
        "{name}: part list"
    );
    assert_eq!(
        reopened.save().unwrap(),
        saved,
        "{name}: second save differs"
    );
}

#[test]
fn the_corpus_has_58_feature_fixtures() {
    assert_eq!(feature_fixtures("docx").len(), 19);
    assert_eq!(feature_fixtures("xlsx").len(), 20);
    assert_eq!(feature_fixtures("pptx").len(), 19);
    assert_eq!(all_feature_fixtures().len(), 58);
}

#[test]
fn every_feature_fixture_repacks_byte_identically() {
    let all = all_feature_fixtures();
    assert_eq!(all.len(), 58);
    for name in &all {
        assert_repack_identical(name, &fixture(name));
    }
}

#[test]
fn every_part_of_every_feature_fixture_reads_and_inflated_bytes_round_trip() {
    for name in all_feature_fixtures() {
        let original = fixture(&name);
        let src = unzip(&original);
        let pkg = OpcPackage::open(&original).unwrap();
        for e in &src {
            if e.name == "[Content_Types].xml" {
                continue;
            }
            let part = pkg
                .part(&format!("/{}", e.name))
                .unwrap()
                .unwrap_or_else(|| panic!("{name}: {} not found", e.name));
            assert_eq!(part.as_ref(), e.data.as_slice(), "{name}: {}", e.name);
        }
    }
}

#[test]
fn other_parts_stay_identical_when_one_part_changes() {
    let original = fixture("docx/features/styles.docx");
    let src = unzip(&original);
    let mut pkg = OpcPackage::open(&original).unwrap();
    let main = pkg.main_document_part().unwrap().unwrap();
    assert_eq!(main, "/word/document.xml");
    let mut doc = pkg.part_xml(&main).unwrap().unwrap();
    let body = doc.root.child_mut("w:body").unwrap();
    body.children.insert(
        0,
        XmlNode::Element(
            XmlElement::new("w:p").with_child(
                XmlElement::new("w:r")
                    .with_child(XmlElement::new("w:t").with_text("Inserted by the test")),
            ),
        ),
    );
    pkg.set_part_xml(&main, &doc).unwrap();
    assert!(!pkg.is_untouched(&main));
    assert!(pkg.is_untouched("/word/styles.xml"));
    let saved = pkg.save().unwrap();
    let out = unzip(&saved);
    for e in &src {
        if e.name == "word/document.xml" {
            continue;
        }
        let o = out.iter().find(|o| o.name == e.name).unwrap();
        assert!(o.data == e.data && o.raw == e.raw, "{}", e.name);
    }
    let reread = OpcPackage::open(&saved)
        .unwrap()
        .part_xml("/word/document.xml")
        .unwrap()
        .unwrap();
    assert!(
        reread
            .root
            .child("w:body")
            .unwrap()
            .text()
            .contains("Inserted by the test")
    );
}

#[test]
fn resolves_typed_relationships_in_each_format() {
    let docx = OpcPackage::open(&fixture("docx/features/comments.docx")).unwrap();
    assert_eq!(
        docx.related_part("/word/document.xml", rel_type::STYLES)
            .unwrap()
            .as_deref(),
        Some("/word/styles.xml")
    );
    assert_eq!(
        docx.related_part("/word/document.xml", rel_type::COMMENTS)
            .unwrap()
            .as_deref(),
        Some("/word/comments.xml")
    );
    assert_eq!(
        docx.related_part("/", rel_type::CORE_PROPERTIES)
            .unwrap()
            .as_deref(),
        Some("/docProps/core.xml")
    );
    assert_eq!(
        docx.content_type("/word/document.xml"),
        Some(content_type::DOCX_DOCUMENT)
    );

    let xlsx = OpcPackage::open(&fixture("xlsx/features/multi-sheet.xlsx")).unwrap();
    let wb = xlsx.main_document_part().unwrap().unwrap();
    assert_eq!(wb, "/xl/workbook.xml");
    assert_eq!(
        xlsx.related_parts(&wb, rel_type::WORKSHEET).unwrap().len(),
        6
    );
    assert_eq!(
        xlsx.related_part(&wb, rel_type::THEME).unwrap().as_deref(),
        Some("/xl/theme/theme1.xml")
    );

    let pptx = OpcPackage::open(&fixture("pptx/features/pictures.pptx")).unwrap();
    let pres = pptx.main_document_part().unwrap().unwrap();
    let slides = pptx.related_parts(&pres, rel_type::SLIDE).unwrap();
    assert_eq!(slides.len(), 1);
    let images = pptx.related_parts(&slides[0], rel_type::IMAGE).unwrap();
    assert!(images.len() >= 3);
    for img in &images {
        assert!(pptx.content_type(img).unwrap().starts_with("image/"));
    }
    let layout = pptx
        .related_part(&slides[0], rel_type::SLIDE_LAYOUT)
        .unwrap()
        .unwrap();
    assert!(layout.starts_with("/ppt/slideLayouts/slideLayout") && layout.ends_with(".xml"));
    assert_eq!(
        pptx.related_part(&layout, rel_type::SLIDE_MASTER)
            .unwrap()
            .as_deref(),
        Some("/ppt/slideMasters/slideMaster1.xml")
    );
}

#[test]
fn reports_external_targets_without_resolving_them() {
    let pkg = OpcPackage::open(&fixture("docx/features/hyperlinks.docx")).unwrap();
    let rels = pkg.relationships("/word/document.xml").unwrap();
    let links: Vec<_> = rels.by_type(rel_type::HYPERLINK).collect();
    assert_eq!(links.len(), 3);
    assert!(links.iter().all(|r| r.is_external()));
    assert_eq!(rels.resolve(links[0]), None);
    assert!(links.iter().any(|l| l.target == "https://example.org/"));
}

#[test]
fn adds_names_and_removes_parts_and_relationships() {
    let mut pkg = OpcPackage::open(&fixture("docx/features/images-inline.docx")).unwrap();
    let media = pkg
        .part_names()
        .filter(|n| n.starts_with("/word/media/"))
        .count();
    assert!(media >= 2);
    let next = pkg.next_part_name("/word/media/image{n}.png").unwrap();
    assert!(!pkg.has(&next));
    pkg.add_part(&next, "image/png", vec![1, 2, 3]).unwrap();
    let rel = pkg
        .add_relationship(
            "/word/document.xml",
            rel_type::IMAGE,
            &next,
            RelationshipOptions::default(),
        )
        .unwrap();
    assert_eq!(
        rel.target,
        format!("media/{}", next.rsplit('/').next().unwrap())
    );
    assert_eq!(
        pkg.resolve_relationship("/word/document.xml", &rel.id)
            .unwrap()
            .as_deref(),
        Some(next.as_str())
    );
    assert!(matches!(
        pkg.add_part(&next, "image/png", Vec::new()),
        Err(OpcError::PartExists(_))
    ));

    let mut reopened = OpcPackage::open(&pkg.save().unwrap()).unwrap();
    assert_eq!(reopened.part(&next).unwrap().unwrap().as_ref(), &[1, 2, 3]);
    assert_eq!(
        reopened
            .resolve_relationship("/word/document.xml", &rel.id)
            .unwrap()
            .as_deref(),
        Some(next.as_str())
    );
    assert!(reopened.remove_part(&next, true).unwrap());
    assert!(
        reopened
            .relationships("/word/document.xml")
            .unwrap()
            .get(&rel.id)
            .is_none()
    );
    let again = OpcPackage::open(&reopened.save().unwrap()).unwrap();
    assert!(!again.has(&next));
    assert!(
        again
            .relationships("/word/document.xml")
            .unwrap()
            .get(&rel.id)
            .is_none()
    );
}

#[test]
fn removing_a_part_removes_its_relationships_part_and_override() {
    let mut pkg = OpcPackage::open(&fixture("docx/features/comments.docx")).unwrap();
    assert!(pkg.content_type("/word/comments.xml").is_some());
    pkg.remove_part("/word/document.xml", true).unwrap();
    assert!(!pkg.has("/word/_rels/document.xml.rels"));
    assert_eq!(pkg.main_document_part().unwrap(), None);
    assert_eq!(pkg.content_types().override_for("/word/document.xml"), None);
    // The package relationship to it went too, and the package still saves and reopens.
    let reopened = OpcPackage::open(&pkg.save().unwrap()).unwrap();
    assert_eq!(reopened.main_document_part().unwrap(), None);
}

#[test]
fn builds_a_package_from_scratch_with_office_ordering_and_content_types() {
    let mut pkg = OpcPackage::new();
    pkg.add_xml_part(
        "/word/document.xml",
        content_type::DOCX_DOCUMENT,
        &XmlElement::new("w:document").with_child(XmlElement::new("w:body")),
    )
    .unwrap();
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/word/document.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.add_part("/word/media/image1.png", "image/png", vec![137, 80, 78, 71])
        .unwrap();
    pkg.add_relationship(
        "/word/document.xml",
        rel_type::IMAGE,
        "/word/media/image1.png",
        RelationshipOptions::default(),
    )
    .unwrap();
    let bytes = pkg.save().unwrap();
    let names: Vec<String> = unzip(&bytes).into_iter().map(|e| e.name).collect();
    assert_eq!(&names[..2], ["[Content_Types].xml", "_rels/.rels"]);
    let reopened = OpcPackage::open(&bytes).unwrap();
    assert_eq!(
        reopened.main_document_part().unwrap().as_deref(),
        Some("/word/document.xml")
    );
    assert_eq!(
        reopened.content_types().default_for("png"),
        Some("image/png")
    );
    assert_eq!(
        reopened
            .content_types()
            .override_for("/word/media/image1.png"),
        None
    );
    assert_eq!(
        reopened.content_type("/word/document.xml"),
        Some(content_type::DOCX_DOCUMENT)
    );
    assert_eq!(
        reopened
            .related_part("/word/document.xml", rel_type::IMAGE)
            .unwrap()
            .as_deref(),
        Some("/word/media/image1.png")
    );
    // Deterministic output.
    assert_eq!(pkg.save().unwrap(), bytes);
    // The written XML parts carry Office's declaration.
    let doc = reopened.part("/word/document.xml").unwrap().unwrap();
    assert!(doc.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n"));
}

#[test]
fn looks_up_part_names_case_insensitively() {
    let pkg = OpcPackage::open(&fixture("docx/features/styles.docx")).unwrap();
    assert!(pkg.has("/WORD/Document.xml"));
    assert!(pkg.part("/word/STYLES.xml").unwrap().is_some());
}

#[test]
fn refuses_zip_bombs_before_inflating_them() {
    let zeros = vec![0u8; 4 * 1024 * 1024];
    let bomb = make_zip(&[
        ("[Content_Types].xml", b"<Types/>", false),
        ("big.bin", &zeros, true),
    ]);
    let with = |l: OpcLimits| OpcPackage::open_with_limits(&bomb, l);
    let d = OpcLimits::default();
    assert!(matches!(
        with(OpcLimits {
            max_compression_ratio: 100,
            ..d
        }),
        Err(OpcError::ZipBomb(_))
    ));
    assert!(matches!(
        with(OpcLimits {
            max_entry_size: 1024 * 1024,
            ..d
        }),
        Err(OpcError::ZipBomb(_))
    ));
    assert!(matches!(
        with(OpcLimits {
            max_total_size: 1024,
            ..d
        }),
        Err(OpcError::ZipBomb(_))
    ));
    assert!(matches!(
        with(OpcLimits {
            max_entries: 1,
            ..d
        }),
        Err(OpcError::ZipBomb(_))
    ));
    // With a permissive ratio the same archive opens: the limits, not the archive, decide.
    let pkg = with(OpcLimits {
        max_compression_ratio: 1_000_000_000,
        ..d
    })
    .unwrap();
    assert!(pkg.has("/big.bin"));
    assert_eq!(pkg.part("/big.bin").unwrap().unwrap().len(), zeros.len());
}

#[test]
fn recognises_encrypted_and_non_zip_input() {
    let cfb = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    assert_eq!(
        OpcPackage::open(&cfb).unwrap_err(),
        OpcError::EncryptedOrLegacy
    );
    assert_eq!(OpcPackage::open(b"hello").unwrap_err(), OpcError::NotZip);
    let no_ct = make_zip(&[("a.xml", b"<a/>", true)]);
    let err = OpcPackage::open(&no_ct).unwrap_err();
    assert_eq!(err, OpcError::MissingContentTypes);
    assert!(err.to_string().contains("Content_Types"));
    assert!(matches!(
        OpcPackage::open(&[0x50, 0x4b, 3, 4, 0, 0]),
        Err(OpcError::CorruptZip(_))
    ));
}

#[test]
fn rejects_parts_that_differ_only_in_case() {
    let zip = make_zip(&[
        ("[Content_Types].xml", b"<Types/>", true),
        ("word/a.xml", b"<a/>", true),
        ("Word/A.xml", b"<b/>", true),
    ]);
    assert!(matches!(
        OpcPackage::open(&zip),
        Err(OpcError::DuplicatePart(_))
    ));
}

#[test]
fn resolves_and_relativises_targets() {
    assert_eq!(
        resolve_target("/word/document.xml", "media/image1.png"),
        "/word/media/image1.png"
    );
    assert_eq!(
        resolve_target("/xl/worksheets/sheet1.xml", "../drawings/drawing1.xml"),
        "/xl/drawings/drawing1.xml"
    );
    assert_eq!(
        resolve_target("/", "word/document.xml"),
        "/word/document.xml"
    );
    assert_eq!(
        resolve_target("/ppt/slides/slide1.xml", "/ppt/media/a%20b.png"),
        "/ppt/media/a b.png"
    );
    assert_eq!(
        relative_target("/xl/worksheets/sheet1.xml", "/xl/drawings/drawing1.xml"),
        "../drawings/drawing1.xml"
    );
    assert_eq!(
        relative_target("/word/document.xml", "/word/media/image1.png"),
        "media/image1.png"
    );
    assert_eq!(
        relative_target("/", "/docProps/core.xml"),
        "docProps/core.xml"
    );
}

#[test]
fn maps_relationship_parts_to_sources() {
    assert_eq!(
        rels_part_name("/word/document.xml"),
        "/word/_rels/document.xml.rels"
    );
    assert_eq!(rels_part_name("/"), "/_rels/.rels");
    assert_eq!(source_part_of_rels("/_rels/.rels").as_deref(), Some("/"));
    assert_eq!(
        source_part_of_rels("/word/_rels/document.xml.rels").as_deref(),
        Some("/word/document.xml")
    );
    assert_eq!(source_part_of_rels("/word/document.xml"), None);
    assert_eq!(extension_of("/word/media/IMAGE1.PNG"), "png");
}

#[test]
fn validates_part_names() {
    assert!(validate_part_name("word/document.xml").is_err());
    assert!(validate_part_name("/word//document.xml").is_err());
    assert!(validate_part_name("/word/").is_err());
    assert!(validate_part_name("/word/doc.").is_err());
    assert!(validate_part_name("/word/./doc.xml").is_err());
    assert!(validate_part_name("/word/document.xml").is_ok());
}

#[test]
fn matches_strict_relationship_types() {
    assert!(rel_type_matches(
        "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument",
        rel_type::OFFICE_DOCUMENT
    ));
    assert!(!rel_type_matches(rel_type::STYLES, rel_type::THEME));
}

#[test]
fn registers_content_types_the_way_office_does() {
    let mut ct = ContentTypes::new();
    ct.set("/word/media/image1.emf", "image/x-emf");
    assert_eq!(ct.default_for("emf"), Some("image/x-emf"));
    ct.set("/word/document.xml", content_type::DOCX_DOCUMENT);
    assert_eq!(
        ct.override_for("/word/document.xml"),
        Some(content_type::DOCX_DOCUMENT)
    );
    ct.prune_defaults(["/word/document.xml"]);
    assert_eq!(ct.default_for("emf"), None);
    assert_eq!(
        ContentTypes::parse(&ct.to_bytes())
            .unwrap()
            .get("/word/document.xml"),
        Some(content_type::DOCX_DOCUMENT)
    );
}

#[test]
fn relationship_edits_show_in_the_rels_part_before_saving() {
    let mut pkg = OpcPackage::new();
    pkg.add_part("/a.xml", content_type::XML, b"<a/>".to_vec())
        .unwrap();
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/a.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    let rels = pkg.part("/_rels/.rels").unwrap().unwrap();
    let text = String::from_utf8(rels.into_owned()).unwrap();
    assert!(text.contains(r#"Target="a.xml""#), "{text}");
    let id = pkg.relationships("/").unwrap().all()[0].id.clone();
    assert!(pkg.remove_relationship("/", &id).unwrap());
    assert!(!pkg.has("/_rels/.rels"), "an empty .rels part is dropped");
    assert!(!pkg.remove_relationship("/", &id).unwrap());
}

#[test]
fn a_damaged_entry_fails_on_access_not_on_open() {
    let good = make_zip(&[
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#,
            true,
        ),
        ("a.xml", b"<a>hello hello hello hello hello</a>", true),
    ]);
    // Flip a byte inside the compressed data of a.xml (after its local header).
    let mut bad = good.clone();
    let at = bad
        .windows(5)
        .rposition(|w| w == b"a.xml")
        .map(|i| i + 5 + 2)
        .unwrap();
    // The match above is the central directory; find the local header occurrence instead.
    let local = bad.windows(5).position(|w| w == b"a.xml").unwrap() + 5 + 2;
    assert!(local < at);
    bad[local] ^= 0xff;
    let pkg = OpcPackage::open(&bad).unwrap();
    assert!(matches!(pkg.part("/a.xml"), Err(OpcError::CorruptZip(_))));
    // Saving copies the damaged entry verbatim instead of failing: nothing is lost.
    let saved = pkg.save().unwrap();
    assert!(OpcPackage::open(&saved).is_ok());
}
