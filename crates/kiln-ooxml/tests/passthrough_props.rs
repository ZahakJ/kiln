// SPDX-License-Identifier: AGPL-3.0-or-later
//! Passthrough, document properties and units. Ported from
//! `reference/ts-core/ooxml/test/passthrough-props.test.ts`.

mod common;

use common::{fixture, unzip};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, content_type, rel_type};
use kiln_ooxml::passthrough::{CollectOptions, PassthroughSet};
use kiln_ooxml::props::{
    AppProperties, CoreProperties, CustomProperty, CustomPropertyValue, DocumentProperties,
    PropertiesUpdate, read_document_properties, write_document_properties,
};
use kiln_ooxml::units::{
    angle_to_degrees, degrees_to_angle, eighths_to_pt, emu_to_pt, emu_to_px, half_points_to_pt,
    parse_fiftieths_percent, parse_length_pt, parse_percentage, parse_universal_measure, pt_to_emu,
    pt_to_twips, px_to_emu, twips_to_emu, twips_to_pt,
};
use kiln_ooxml::xml::{XmlElement, element_to_bytes};

/// A "reader" that understands only the main document, styles and core properties.
const CONSUMED: [&str; 3] = [
    "/word/document.xml",
    "/word/styles.xml",
    "/docProps/core.xml",
];

fn identity(n: &str) -> Option<String> {
    Some(n.to_owned())
}

#[test]
fn records_unconsumed_parts_and_the_relationships_that_reach_them() {
    let src = OpcPackage::open(&fixture("docx/features/footnotes.docx")).unwrap();
    let set = PassthroughSet::collect(&src, &CollectOptions::consumed(CONSUMED)).unwrap();
    let names: Vec<&str> = set.parts().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"/word/footnotes.xml"));
    assert!(names.contains(&"/word/endnotes.xml"));
    assert!(names.contains(&"/word/theme/theme1.xml"));
    assert!(!names.contains(&"/word/document.xml"));
    assert!(set.has("/word/settings.xml"));
    let from_doc: Vec<&str> = set
        .relationships()
        .iter()
        .filter(|r| r.source == "/word/document.xml")
        .map(|r| r.rel.rel_type.as_str())
        .collect();
    assert!(from_doc.contains(&rel_type::FOOTNOTES));
    // Styles were consumed; the writer recreates that link.
    assert!(!from_doc.contains(&rel_type::STYLES));
    assert!(set.byte_len() > 0);
}

#[test]
fn re_emits_parts_byte_for_byte_with_relationships_fixed_up() {
    let original = fixture("docx/features/footnotes.docx");
    let src = OpcPackage::open(&original).unwrap();
    let set = PassthroughSet::collect(&src, &CollectOptions::consumed(CONSUMED)).unwrap();

    // A "writer" that regenerates only the consumed parts.
    let mut out = OpcPackage::new();
    for (name, ct) in [
        ("/word/document.xml", content_type::DOCX_DOCUMENT),
        ("/word/styles.xml", content_type::DOCX_STYLES),
        ("/docProps/core.xml", content_type::CORE_PROPERTIES),
    ] {
        out.add_part(name, ct, src.part_bytes(name).unwrap().into_owned())
            .unwrap();
    }
    let opts = RelationshipOptions::default();
    out.add_relationship("/", rel_type::OFFICE_DOCUMENT, "/word/document.xml", opts)
        .unwrap();
    out.add_relationship("/", rel_type::CORE_PROPERTIES, "/docProps/core.xml", opts)
        .unwrap();
    out.add_relationship(
        "/word/document.xml",
        rel_type::STYLES,
        "/word/styles.xml",
        opts,
    )
    .unwrap();

    let result = set.emit(&mut out, identity).unwrap();
    assert!(result.dropped.is_empty());
    let saved = out.save().unwrap();
    let reopened = OpcPackage::open(&saved).unwrap();
    for p in set.parts() {
        assert_eq!(
            reopened.part_bytes(&p.name).unwrap().as_ref(),
            p.bytes.as_slice(),
            "{}",
            p.name
        );
        assert_eq!(reopened.content_type(&p.name), src.content_type(&p.name));
    }
    assert_eq!(
        reopened
            .related_part("/word/document.xml", rel_type::FOOTNOTES)
            .unwrap()
            .as_deref(),
        Some("/word/footnotes.xml")
    );
    assert_eq!(
        reopened
            .related_part("/word/document.xml", rel_type::THEME)
            .unwrap()
            .as_deref(),
        Some("/word/theme/theme1.xml")
    );
    assert_eq!(
        reopened
            .related_part("/", rel_type::EXTENDED_PROPERTIES)
            .unwrap()
            .as_deref(),
        Some("/docProps/app.xml")
    );
    // Ids that the document body refers to (r:id) survive when they are free.
    let src_id = src
        .relationships("/word/document.xml")
        .unwrap()
        .first_by_type(rel_type::FOOTNOTES)
        .unwrap()
        .id
        .clone();
    assert_eq!(
        reopened
            .relationships("/word/document.xml")
            .unwrap()
            .first_by_type(rel_type::FOOTNOTES)
            .unwrap()
            .id,
        src_id
    );
    // Every original part is present again.
    let mut a: Vec<String> = unzip(&saved).into_iter().map(|e| e.name).collect();
    let mut b: Vec<String> = unzip(&original).into_iter().map(|e| e.name).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

#[test]
fn renames_colliding_parts_and_reallocates_taken_relationship_ids() {
    let src = OpcPackage::open(&fixture("pptx/features/pictures.pptx")).unwrap();
    let consumed: Vec<&str> = src
        .part_names()
        .filter(|n| !n.starts_with("/ppt/media/"))
        .collect();
    let set = PassthroughSet::collect(&src, &CollectOptions::consumed(consumed)).unwrap();
    assert!(set.parts().all(|p| p.name.starts_with("/ppt/media/")));

    let mut out = OpcPackage::new();
    out.add_xml_part(
        "/ppt/slides/slide1.xml",
        content_type::PPTX_SLIDE,
        &XmlElement::new("p:sld"),
    )
    .unwrap();
    // The writer already used the first media name and the first relationship id.
    let first_media = set.parts().next().unwrap().name.clone();
    out.add_part(&first_media, "image/png", vec![9]).unwrap();
    out.add_relationship(
        "/ppt/slides/slide1.xml",
        rel_type::IMAGE,
        &first_media,
        RelationshipOptions {
            id: Some("rId2"),
            external: false,
        },
    )
    .unwrap();

    let result = set.emit(&mut out, identity).unwrap();
    let renamed = &result.renamed_parts[&first_media];
    let stem_ext = renamed.rsplit('/').next().unwrap();
    assert!(stem_ext.contains("_1."), "{renamed}");
    let ids = &result.relationship_ids["/ppt/slides/slide1.xml"];
    assert!(!ids.is_empty());
    let rels = out
        .relationships("/ppt/slides/slide1.xml")
        .unwrap()
        .into_owned();
    for (old_id, new_id) in ids {
        let target = rels.resolve(rels.get(new_id).unwrap()).unwrap();
        let orig_target = src
            .resolve_relationship("/ppt/slides/slide1.xml", old_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            out.part_bytes(&target).unwrap(),
            src.part_bytes(&orig_target).unwrap()
        );
    }
    // The id the writer took was reallocated, not reused.
    assert_ne!(ids.get("rId2").map(String::as_str), Some("rId2"));
}

#[test]
fn maps_renamed_sources_and_reports_relationships_whose_source_disappeared() {
    let src = OpcPackage::open(&fixture("xlsx/features/comments.xlsx")).unwrap();
    let consumed = [
        "/xl/workbook.xml",
        "/xl/worksheets/sheet1.xml",
        "/xl/styles.xml",
    ];
    let set = PassthroughSet::collect(&src, &CollectOptions::consumed(consumed)).unwrap();
    let mut out = OpcPackage::new();
    out.add_xml_part(
        "/xl/workbook.xml",
        content_type::XLSX_WORKBOOK,
        &XmlElement::new("workbook"),
    )
    .unwrap();
    out.add_xml_part(
        "/xl/worksheets/sheet9.xml",
        content_type::XLSX_WORKSHEET,
        &XmlElement::new("worksheet"),
    )
    .unwrap();
    let r1 = set
        .emit(&mut out, |n| match n {
            "/xl/worksheets/sheet1.xml" => Some("/xl/worksheets/sheet9.xml".to_owned()),
            "/xl/styles.xml" => None,
            other => Some(other.to_owned()),
        })
        .unwrap();
    let comments = out
        .related_part("/xl/worksheets/sheet9.xml", rel_type::COMMENTS)
        .unwrap()
        .unwrap();
    assert!(comments.starts_with("/xl/comments"), "{comments}");
    assert!(r1.dropped.is_empty());

    let mut out2 = OpcPackage::new();
    let r2 = set.emit(&mut out2, |_| None).unwrap();
    assert!(!r2.dropped.is_empty());
}

#[test]
fn honours_relationships_the_reader_consumed() {
    let src = OpcPackage::open(&fixture("docx/features/hyperlinks.docx")).unwrap();
    let ids: Vec<String> = src
        .relationships("/word/document.xml")
        .unwrap()
        .by_type(rel_type::HYPERLINK)
        .map(|r| r.id.clone())
        .collect();
    let all = PassthroughSet::collect(&src, &CollectOptions::consumed(CONSUMED)).unwrap();
    assert_eq!(
        all.relationships()
            .iter()
            .filter(|r| r.rel.rel_type == rel_type::HYPERLINK)
            .count(),
        3
    );
    let some = PassthroughSet::collect(
        &src,
        &CollectOptions {
            consumed_parts: CONSUMED.map(str::to_owned).to_vec(),
            consumed_relationships: vec![("/word/document.xml".to_owned(), ids)],
        },
    )
    .unwrap();
    assert_eq!(
        some.relationships()
            .iter()
            .filter(|r| r.rel.rel_type == rel_type::HYPERLINK)
            .count(),
        0
    );
}

#[test]
fn reads_core_extended_and_custom_properties() {
    let props = read_document_properties(
        &OpcPackage::open(&fixture("docx/features/properties.docx")).unwrap(),
    )
    .unwrap();
    let c = &props.core;
    assert_eq!(c.title.as_deref(), Some("Document properties"));
    assert_eq!(c.subject.as_deref(), Some("Fixture subject"));
    // The generated corpus predates the rename to Kiln; its metadata says Officina.
    assert_eq!(c.creator.as_deref(), Some("Officina Fixtures"));
    assert_eq!(c.keywords.as_deref(), Some("officina; fixture; properties"));
    assert_eq!(
        c.description.as_deref(),
        Some("Description of the document")
    );
    assert_eq!(c.category.as_deref(), Some("Testing"));
    assert_eq!(c.content_status.as_deref(), Some("Draft"));
    assert_eq!(c.language.as_deref(), Some("en-GB"));
    assert_eq!(c.created.as_deref(), Some("2026-01-15T09:30:00Z"));
    assert_eq!(c.modified.as_deref(), Some("2026-01-15T09:30:00Z"));
    let p = |name: &str, value: CustomPropertyValue| CustomProperty {
        name: name.into(),
        value,
        link_target: None,
    };
    assert_eq!(
        props.custom,
        [
            p("Project", CustomPropertyValue::Text("Officina".into())),
            p("Build", CustomPropertyValue::Int(42)),
            p("Ratio", CustomPropertyValue::Number(0.75)),
            p("Reviewed", CustomPropertyValue::Bool(true)),
            p(
                "Due",
                CustomPropertyValue::FileTime("2026-02-01T00:00:00Z".into())
            ),
        ]
    );
    let xlsx =
        read_document_properties(&OpcPackage::open(&fixture("xlsx/features/styles.xlsx")).unwrap())
            .unwrap();
    assert_eq!(
        xlsx.app.application.as_deref(),
        Some("Microsoft Excel Compatible / Openpyxl 3.1.5")
    );
}

#[test]
fn writes_properties_into_a_new_package_and_reads_them_back() {
    let mut pkg = OpcPackage::new();
    let core = CoreProperties {
        title: Some(r#"Quarterly <report> & "notes""#.into()),
        creator: Some("Ada".into()),
        created: Some("2026-10-15T08:00:00Z".into()),
        modified: Some("2026-10-15T09:00:00Z".into()),
        ..CoreProperties::default()
    };
    let app = AppProperties {
        application: Some("Kiln".into()),
        app_version: Some("0.1".into()),
        pages: Some(3),
        words: Some(120),
        scale_crop: Some(false),
        company: Some("Workshop".into()),
        ..AppProperties::default()
    };
    write_document_properties(
        &mut pkg,
        &PropertiesUpdate {
            core: Some(core.clone()),
            app: Some(app.clone()),
            custom: Some(vec![
                CustomProperty {
                    name: "Client".into(),
                    value: CustomPropertyValue::Text("ACME".into()),
                    link_target: None,
                },
                CustomProperty {
                    name: "Approved".into(),
                    value: CustomPropertyValue::Bool(false),
                    link_target: None,
                },
            ]),
        },
    )
    .unwrap();
    let reread =
        read_document_properties(&OpcPackage::open(&pkg.save().unwrap()).unwrap()).unwrap();
    assert_eq!(reread.core, core);
    assert_eq!(reread.app, app);
    let names: Vec<&str> = reread.custom.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Client", "Approved"]);
    let xml =
        String::from_utf8(pkg.part_bytes("/docProps/core.xml").unwrap().into_owned()).unwrap();
    assert!(xml.contains(
        r#"<dcterms:created xsi:type="dcterms:W3CDTF">2026-10-15T08:00:00Z</dcterms:created>"#
    ));
}

#[test]
fn updates_existing_properties_in_place_and_preserves_unknown_app_elements() {
    let mut pkg = OpcPackage::open(&fixture("docx/features/properties.docx")).unwrap();
    let before = read_document_properties(&pkg).unwrap();
    write_document_properties(
        &mut pkg,
        &PropertiesUpdate {
            core: Some(CoreProperties {
                title: Some("Renamed".into()),
                ..before.core.clone()
            }),
            app: Some(before.app.clone()),
            custom: Some(Vec::new()),
        },
    )
    .unwrap();
    let after = read_document_properties(&OpcPackage::open(&pkg.save().unwrap()).unwrap()).unwrap();
    assert_eq!(after.core.title.as_deref(), Some("Renamed"));
    assert_eq!(after.core.subject.as_deref(), Some("Fixture subject"));
    assert_eq!(after.app, before.app);
    assert!(after.custom.is_empty());
    assert!(!pkg.has("/docProps/custom.xml"));
    assert_eq!(
        pkg.related_part("/", rel_type::CUSTOM_PROPERTIES).unwrap(),
        None
    );
}

#[test]
fn preserves_structured_app_elements_in_office_order() {
    let mut pkg = OpcPackage::new();
    let heading_pairs = XmlElement::new("HeadingPairs").with_child(XmlElement::new("vt:vector"));
    let app = AppProperties {
        application: Some("Kiln".into()),
        company: Some("Workshop".into()),
        extras: vec![
            heading_pairs.clone(),
            XmlElement::new("Unknown").with_text("x"),
        ],
        ..AppProperties::default()
    };
    write_document_properties(
        &mut pkg,
        &PropertiesUpdate {
            app: Some(app.clone()),
            ..PropertiesUpdate::default()
        },
    )
    .unwrap();
    let root = pkg.part_xml("/docProps/app.xml").unwrap().unwrap().root;
    let names: Vec<&str> = root.elements().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Application", "HeadingPairs", "Company", "Unknown"]);
    assert_eq!(read_document_properties(&pkg).unwrap().app, app);
}

#[test]
fn units_convert_between_emu_points_pixels_and_twips() {
    assert_eq!(emu_to_pt(12_700.0), 1.0);
    assert_eq!(pt_to_emu(72.0), 914_400);
    assert_eq!(emu_to_px(9_525.0), 1.0);
    assert_eq!(px_to_emu(96.0), 914_400);
    assert_eq!(twips_to_pt(240.0), 12.0);
    assert_eq!(pt_to_twips(11.0), 220);
    assert_eq!(twips_to_emu(1_440.0), 914_400);
    assert_eq!(half_points_to_pt(22.0), 11.0);
    assert_eq!(eighths_to_pt(4.0), 0.5);
    assert_eq!(angle_to_degrees(5_400_000.0), 90.0);
    assert_eq!(degrees_to_angle(45.0), 2_700_000);
}

#[test]
fn units_parse_percentages_and_universal_measures() {
    assert_eq!(parse_percentage("50000"), Some(0.5));
    assert_eq!(parse_percentage("50%"), Some(0.5));
    assert_eq!(parse_percentage("x"), None);
    assert_eq!(parse_fiftieths_percent("5000"), Some(1.0));
    assert_eq!(parse_fiftieths_percent("25%"), Some(0.25));
    assert_eq!(parse_universal_measure("1in"), Some(72.0));
    assert!((parse_universal_measure("2.54cm").unwrap() - 72.0).abs() < 1e-9);
    assert_eq!(parse_universal_measure("12pt"), Some(12.0));
    assert_eq!(parse_universal_measure("1pc"), Some(12.0));
    assert_eq!(parse_universal_measure("100"), None);
    assert_eq!(parse_length_pt("1440", 20.0), Some(72.0));
    assert_eq!(parse_length_pt("0.5in", 20.0), Some(36.0));
    assert_eq!(parse_length_pt("", 20.0), None);
}

#[test]
fn hand_authored_packages_normalise_foreign_prefixes() {
    let pkg = OpcPackage::open(&fixture("docx/manual/foreign-prefixes.docx")).unwrap();
    let doc = pkg.part_xml("/word/document.xml").unwrap().unwrap().root;
    assert_eq!(doc.name, "w:document");
    let comments_part = pkg
        .related_part("/word/document.xml", rel_type::COMMENTS)
        .unwrap()
        .unwrap();
    let comments = pkg.part_xml(&comments_part).unwrap().unwrap().root;
    assert_eq!(comments.name, "w:comments");
    let first = comments.first_element().unwrap();
    assert_eq!(first.name, "w:comment");
    assert_eq!(first.attr("w:author"), Some("Hand"));
}

#[test]
fn passes_custom_xml_unknown_relationship_types_and_binary_parts_through() {
    let src = OpcPackage::open(&fixture("docx/manual/foreign-prefixes.docx")).unwrap();
    let set = PassthroughSet::collect(
        &src,
        &CollectOptions::consumed(["/word/document.xml", "/word/comments.xml"]),
    )
    .unwrap();
    let mut names: Vec<&str> = set.parts().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "/customXml/item1.xml",
            "/customXml/itemProps1.xml",
            "/word/blobs/data.bin"
        ]
    );
    let mut out = OpcPackage::new();
    out.add_part(
        "/word/document.xml",
        content_type::DOCX_DOCUMENT,
        src.part_bytes("/word/document.xml").unwrap().into_owned(),
    )
    .unwrap();
    out.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/word/document.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    set.emit(&mut out, identity).unwrap();
    let re = OpcPackage::open(&out.save().unwrap()).unwrap();
    assert_eq!(
        re.related_part("/word/document.xml", rel_type::CUSTOM_XML)
            .unwrap()
            .as_deref(),
        Some("/customXml/item1.xml")
    );
    assert_eq!(
        re.related_part("/customXml/item1.xml", rel_type::CUSTOM_XML_PROPS)
            .unwrap()
            .as_deref(),
        Some("/customXml/itemProps1.xml")
    );
    assert_eq!(
        re.related_part(
            "/word/document.xml",
            "http://schemas.officina.example/relationships/blob"
        )
        .unwrap()
        .as_deref(),
        Some("/word/blobs/data.bin")
    );
    assert_eq!(
        re.part_bytes("/word/blobs/data.bin").unwrap().as_ref(),
        &[0, 1, 2, 3, 254, 255, 0x50, 0x4b]
    );
    assert_eq!(
        re.content_type("/word/blobs/data.bin"),
        Some("application/vnd.officina.test-binary")
    );
}

#[test]
fn opens_the_smallest_valid_packages() {
    for name in ["docx/manual/minimal.docx", "xlsx/manual/minimal.xlsx"] {
        let pkg = OpcPackage::open(&fixture(name)).unwrap();
        let main = pkg.main_document_part().unwrap().unwrap();
        assert!(pkg.part_xml(&main).unwrap().is_some());
        assert_eq!(
            read_document_properties(&pkg).unwrap(),
            DocumentProperties::default()
        );
    }
}

#[test]
fn custom_values_that_do_not_fit_their_type_are_preserved() {
    let root = kiln_ooxml::xml::parse_xml_str(
        r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Odd"><vt:i4>twelve</vt:i4></property><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3" name="Date"><vt:date>2026-01-01</vt:date></property></Properties>"#,
    )
    .unwrap()
    .root;
    let props = kiln_ooxml::props::parse_custom_properties(&root);
    assert!(matches!(&props[0].value, CustomPropertyValue::Other(e) if e.text() == "twelve"));
    assert!(matches!(&props[1].value, CustomPropertyValue::Other(e) if e.name == "vt:date"));
    let again = kiln_ooxml::props::parse_custom_properties(
        &kiln_ooxml::xml::parse_xml(
            &element_to_bytes(&kiln_ooxml::props::serialize_custom_properties(&props)).unwrap(),
        )
        .unwrap()
        .root,
    );
    assert_eq!(again, props);
}
