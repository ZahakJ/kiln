// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package-level writer tests (ported from the v1.0 writer's `package.test.ts`): parts, content
//! types and relationships, document properties, determinism, passthrough of unmodelled parts
//! (custom XML, macros, glossary, unknown parts), formats, documents created in Kiln, and the
//! issues and compatibility report of what cannot be written.

mod read_support;
mod roundtrip_support;

use kiln_doc::{
    Block, Document, Drawing, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark,
    Paragraph, RunProps,
};
use kiln_io_docx::{DocxKind, WriteOptions, WriteSource, compat_report, read_docx, write_docx};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, content_type, rel_type};
use read_support::{Build, build_docx, p, r};
use roundtrip_support::{SAVE_TIME, differences, round_trip};

const MAIN: &str = "/word/document.xml";

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(kiln_testing::fixtures::fixtures_dir().join(rel)).expect("fixture")
}

fn write_read(bytes: &[u8], options: &WriteOptions<'_>) -> Vec<u8> {
    let pkg = OpcPackage::open(bytes).expect("opens");
    let read = kiln_io_docx::read_package(&pkg).expect("reads");
    let source = WriteSource::from_read(&read).with_package(&pkg);
    write_docx(&read.document, Some(&source), options)
        .expect("writes")
        .bytes
}

fn text(bytes: &[u8], part: &str) -> String {
    let pkg = OpcPackage::open(bytes).expect("opens");
    String::from_utf8_lossy(&pkg.part_bytes(part).expect("part")).into_owned()
}

// ------------------------------------------------------------------ structure

#[test]
fn every_written_part_has_its_content_type_and_relationship() {
    let bytes = write_read(
        &fixture("docx/features/footnotes.docx"),
        &WriteOptions::default(),
    );
    let pkg = OpcPackage::open(&bytes).unwrap();
    assert_eq!(pkg.main_document_part().unwrap().as_deref(), Some(MAIN));
    assert_eq!(pkg.content_type(MAIN), Some(content_type::DOCX_DOCUMENT));
    let wml =
        |k: &str| format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{k}+xml");
    for (rt, name, ct) in [
        (
            rel_type::STYLES,
            "/word/styles.xml",
            content_type::DOCX_STYLES.to_owned(),
        ),
        (
            rel_type::SETTINGS,
            "/word/settings.xml",
            content_type::DOCX_SETTINGS.to_owned(),
        ),
        (
            rel_type::NUMBERING,
            "/word/numbering.xml",
            content_type::DOCX_NUMBERING.to_owned(),
        ),
        (
            rel_type::THEME,
            "/word/theme/theme1.xml",
            content_type::THEME.to_owned(),
        ),
        (rel_type::FOOTNOTES, "/word/footnotes.xml", wml("footnotes")),
        (rel_type::ENDNOTES, "/word/endnotes.xml", wml("endnotes")),
        (
            rel_type::FONT_TABLE,
            "/word/fontTable.xml",
            wml("fontTable"),
        ),
    ] {
        assert_eq!(
            pkg.related_part(MAIN, rt).unwrap().as_deref(),
            Some(name),
            "{rt}"
        );
        assert_eq!(pkg.content_type(name), Some(ct.as_str()), "{name}");
    }
    assert_eq!(
        pkg.related_part("/", rel_type::CORE_PROPERTIES)
            .unwrap()
            .as_deref(),
        Some("/docProps/core.xml")
    );
    assert_eq!(
        pkg.related_part("/", rel_type::EXTENDED_PROPERTIES)
            .unwrap()
            .as_deref(),
        Some("/docProps/app.xml")
    );
    assert_eq!(pkg.all_part_names().next(), Some("/_rels/.rels"));
}

#[test]
fn new_documents_are_office_style_xml_with_only_the_namespaces_in_use() {
    let out = write_docx(&Document::new(), None, &WriteOptions::default()).unwrap();
    let xml = text(&out.bytes, MAIN);
    let decl = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
    assert!(xml.starts_with(&format!("{decl}<w:document ")), "{xml}");
    assert!(!xml[decl.len()..].contains(">\n"), "no indentation");
    assert!(!xml.contains("xmlns:wps"));
    assert!(!xml.contains("mc:Ignorable"));
    for part in [
        "/word/styles.xml",
        "/word/settings.xml",
        "/word/fontTable.xml",
        "/word/theme/theme1.xml",
    ] {
        assert!(OpcPackage::open(&out.bytes).unwrap().has(part), "{part}");
    }
}

#[test]
fn a_save_updates_modified_date_revision_application_and_statistics() {
    let original = fixture("docx/features/properties.docx");
    let a = read_docx(&original).unwrap();
    let bytes = write_read(
        &original,
        &WriteOptions {
            now: Some(SAVE_TIME.to_owned()),
            author: Some("Kiln Tester".to_owned()),
            pages: Some(1),
            ..WriteOptions::default()
        },
    );
    let b = read_docx(&bytes).unwrap();
    let (ca, cb) = (&a.package_properties.core, &b.package_properties.core);
    assert_eq!(cb.modified.as_deref(), Some(SAVE_TIME));
    assert_eq!(cb.last_modified_by.as_deref(), Some("Kiln Tester"));
    let rev = |c: &kiln_ooxml::props::CoreProperties| {
        c.revision
            .as_deref()
            .and_then(|r| r.parse::<u64>().ok())
            .unwrap_or(0)
    };
    assert_eq!(rev(cb), rev(ca) + 1);
    assert_eq!(cb.created, ca.created);
    assert_eq!(
        (&cb.title, &cb.subject, &cb.creator),
        (&ca.title, &ca.subject, &ca.creator)
    );
    let app = &b.package_properties.app;
    assert_eq!(app.application.as_deref(), Some("Kiln"));
    assert_eq!(app.app_version, None);
    assert_eq!(app.pages, Some(1));
    assert!(app.words.unwrap_or(0) > 0);
    assert_eq!(b.package_properties.custom, a.package_properties.custom);
    assert_eq!(b.document.properties.custom, a.document.properties.custom);
}

#[test]
fn a_rewrite_without_a_save_time_keeps_the_properties() {
    let original = fixture("docx/features/properties.docx");
    let a = read_docx(&original).unwrap();
    let b = read_docx(&write_read(&original, &WriteOptions::default())).unwrap();
    assert_eq!(a.document.properties, b.document.properties);
    assert_eq!(a.package_properties.core, b.package_properties.core);
}

#[test]
fn original_part_names_of_headers_footers_and_media_are_kept() {
    for rel in [
        "docx/features/headers-footers.docx",
        "docx/features/images-inline.docx",
    ] {
        let original = fixture(rel);
        let rt = round_trip(&original);
        let names = |bytes: &[u8]| -> Vec<String> {
            OpcPackage::open(bytes)
                .unwrap()
                .part_names()
                .filter(|n| n.contains("header") || n.contains("footer") || n.contains("media"))
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        assert_eq!(names(&rt.bytes), names(&original), "{rel}");
    }
}

// ------------------------------------------------------------------ determinism

#[test]
fn two_writes_of_one_model_are_identical() {
    for rel in [
        "docx/features/tables.docx",
        "docx/features/images-anchored.docx",
        "docx/features/comments.docx",
    ] {
        let o = WriteOptions {
            now: Some(SAVE_TIME.to_owned()),
            ..WriteOptions::default()
        };
        assert_eq!(
            write_read(&fixture(rel), &o),
            write_read(&fixture(rel), &o),
            "{rel}"
        );
    }
}

#[test]
fn new_documents_differ_between_saves_only_in_the_save_time() {
    let doc = Document::with_options(&kiln_doc::NewDocumentOptions {
        locale: Some("en-GB".into()),
        paper: None,
    });
    let at = |t: &str| {
        write_docx(
            &doc,
            None,
            &WriteOptions {
                now: Some(t.to_owned()),
                ..WriteOptions::default()
            },
        )
        .unwrap()
        .bytes
    };
    assert_eq!(at(SAVE_TIME), at(SAVE_TIME));
    let (a, b) = (at("2026-01-01T00:00:00Z"), at("2026-06-01T00:00:00Z"));
    let (pa, pb) = (OpcPackage::open(&a).unwrap(), OpcPackage::open(&b).unwrap());
    let differing: Vec<&str> = pa
        .all_part_names()
        .filter(|n| pa.part(n).unwrap() != pb.part(n).unwrap())
        .collect();
    assert_eq!(differing, ["/docProps/core.xml"]);
}

// ------------------------------------------------------------------ passthrough

/// A package with custom XML, a glossary, macros and an unknown part.
fn with_extras(main_content_type: &str) -> Vec<u8> {
    let mut pkg = OpcPackage::open(&build_docx(Build {
        body: p(&r("x")),
        ..Build::default()
    }))
    .unwrap();
    pkg.set_content_type(MAIN, main_content_type).unwrap();
    pkg.add_part(
        "/customXml/item1.xml",
        "application/xml",
        "<root xmlns=\"urn:x\"><v>1</v></root>",
    )
    .unwrap();
    pkg.add_part(
        "/customXml/itemProps1.xml",
        "application/vnd.openxmlformats-officedocument.customXmlProperties+xml",
        "<ds:datastoreItem ds:itemID=\"{1}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"/>",
    )
    .unwrap();
    pkg.add_relationship(
        "/customXml/item1.xml",
        rel_type::CUSTOM_XML_PROPS,
        "/customXml/itemProps1.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.add_relationship(
        MAIN,
        rel_type::CUSTOM_XML,
        "/customXml/item1.xml",
        RelationshipOptions {
            id: Some("rIdCx"),
            external: false,
        },
    )
    .unwrap();
    pkg.add_part(
        "/word/glossary/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml",
        "<w:glossaryDocument xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>",
    )
    .unwrap();
    pkg.add_relationship(
        MAIN,
        rel_type::GLOSSARY_DOCUMENT,
        "/word/glossary/document.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.add_part(
        "/word/vbaProject.bin",
        "application/vnd.ms-office.vbaProject",
        vec![1, 2, 3],
    )
    .unwrap();
    pkg.add_relationship(
        MAIN,
        rel_type::VBA_PROJECT,
        "/word/vbaProject.bin",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.add_part("/word/unknown/thing.dat", "application/x-thing", vec![9, 9])
        .unwrap();
    pkg.add_relationship(
        MAIN,
        "http://example.com/relationships/thing",
        "/word/unknown/thing.dat",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.save().unwrap()
}

const MACRO_DOC: &str = "application/vnd.ms-word.document.macroEnabled.main+xml";

#[test]
fn custom_xml_glossary_macros_and_unknown_parts_survive_with_their_relationships() {
    let original = with_extras(MACRO_DOC);
    let rt = round_trip(&original);
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    let before = OpcPackage::open(&original).unwrap();
    for part in [
        "/customXml/item1.xml",
        "/customXml/itemProps1.xml",
        "/word/glossary/document.xml",
        "/word/vbaProject.bin",
        "/word/unknown/thing.dat",
    ] {
        assert_eq!(
            pkg.part(part).unwrap(),
            before.part(part).unwrap(),
            "{part}"
        );
    }
    assert_eq!(
        pkg.resolve_relationship(MAIN, "rIdCx").unwrap().as_deref(),
        Some("/customXml/item1.xml")
    );
    assert_eq!(
        pkg.related_part("/customXml/item1.xml", rel_type::CUSTOM_XML_PROPS)
            .unwrap()
            .as_deref(),
        Some("/customXml/itemProps1.xml")
    );
    assert_eq!(rt.b.kind, DocxKind::MacroEnabledDocument);
    assert!(differences(&rt.a, &rt.b).is_empty());
}

#[test]
fn skipping_the_passthrough_writes_only_model_parts() {
    let bytes = write_read(
        &with_extras(content_type::DOCX_DOCUMENT),
        &WriteOptions {
            skip_passthrough: true,
            ..WriteOptions::default()
        },
    );
    let pkg = OpcPackage::open(&bytes).unwrap();
    assert!(!pkg.has("/customXml/item1.xml"));
    assert!(!pkg.has("/word/unknown/thing.dat"));
    assert!(read_docx(&bytes).is_ok());
}

#[test]
fn macros_are_removed_when_a_macro_document_is_saved_as_docx() {
    let original = with_extras(MACRO_DOC);
    let bytes = write_read(
        &original,
        &WriteOptions {
            kind: Some(DocxKind::Document),
            ..WriteOptions::default()
        },
    );
    let pkg = OpcPackage::open(&bytes).unwrap();
    assert!(!pkg.has("/word/vbaProject.bin"));
    assert!(
        pkg.related_part(MAIN, rel_type::VBA_PROJECT)
            .unwrap()
            .is_none()
    );
    assert_eq!(pkg.content_type(MAIN), Some(content_type::DOCX_DOCUMENT));
    assert!(pkg.has("/customXml/item1.xml"));
    let read = read_docx(&original).unwrap();
    let report = compat_report(
        &read.document,
        kiln_io_docx::CompatOptions {
            target: Some(DocxKind::Document),
            source: Some(read.kind),
            warnings: &read.warnings,
        },
    );
    assert!(report.iter().any(|i| i.code == "macros.removed"));
}

#[test]
fn templates_stay_templates() {
    let original = with_extras(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
    );
    let b = read_docx(&write_read(&original, &WriteOptions::default())).unwrap();
    assert_eq!(b.kind, DocxKind::Template);
    assert!(
        !b.passthrough.has("/word/vbaProject.bin"),
        "a template holds no macros"
    );
}

#[test]
fn strict_documents_are_written_as_transitional() {
    let strict = "http://purl.oclc.org/ooxml/wordprocessingml/main";
    let mut pkg = OpcPackage::new();
    pkg.add_part(
        MAIN,
        content_type::DOCX_DOCUMENT,
        format!("<w:document xmlns:w=\"{strict}\" w:conformance=\"strict\"><w:body><w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>Strict</w:t></w:r></w:p><w:sectPr><w:pgSz w:w=\"21cm\" w:h=\"29.7cm\"/></w:sectPr></w:body></w:document>"),
    )
    .unwrap();
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        MAIN,
        RelationshipOptions::default(),
    )
    .unwrap();
    let rt = round_trip(&pkg.save().unwrap());
    let xml = rt.xml(MAIN);
    assert!(!xml.contains("purl.oclc.org"), "{xml}");
    assert!(!xml.contains("conformance"));
    assert!(xml.contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/>"));
    assert!(differences(&rt.a, &rt.b).is_empty());
}

#[test]
fn foreign_prefixes_are_declared_on_the_fragments_that_use_them() {
    let rt = round_trip(&fixture("docx/manual/foreign-prefixes.docx"));
    assert!(differences(&rt.a, &rt.b).is_empty());
    let body = rt.xml(MAIN);
    assert!(body.contains("<w:commentRangeStart w:id=\"0\"/>"));
    let comments = rt.xml("/word/comments.xml");
    assert!(comments.contains("A comment in a foreign prefix."));
}

// ------------------------------------------------------------------ documents made in Kiln

#[test]
fn the_default_template_keeps_word_style_ids() {
    let doc = Document::new();
    let out = write_docx(&doc, None, &WriteOptions::default()).unwrap();
    let back = read_docx(&out.bytes).unwrap();
    let ids = |d: &Document| d.styles.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&back.document), ids(&doc));
    assert_eq!(back.document.styles, doc.styles);
    assert_eq!(back.document.settings, doc.settings);
    assert_eq!(back.document.theme, doc.theme);
    assert_eq!(back.document.sections[0].props, doc.sections[0].props);
    for id in ["Normal", "Heading1", "Title", "TableGrid", "Hyperlink"] {
        assert!(back.document.styles.contains(id), "{id}");
    }
}

fn paragraph(doc: &mut Document, fragment: Fragment, style: Option<&str>) -> Block {
    let mut p = Paragraph::new(doc.alloc_id());
    p.props.style_id = style.map(str::to_owned);
    p.content = Inlines::from_fragment(fragment).unwrap();
    Block::Paragraph(p)
}

#[test]
fn content_made_in_the_editor_is_written() {
    let mut doc = Document::new();
    doc.media.insert(
        "/word/media/pasted.png".into(),
        kiln_doc::Media {
            content_type: "image/png".into(),
            data: read_support::PNG.to_vec(),
        },
    );
    let heading = paragraph(
        &mut doc,
        Fragment::text("Heading", RunProps::default()),
        Some("Heading1"),
    );
    let link = Fragment::text("Kiln", RunProps::default()).with_mark(Mark::Link(Hyperlink {
        target: LinkTarget::Url("https://example.com/kiln".into()),
        tooltip: Some("Visit".into()),
        frame: None,
        history: true,
    }));
    let linked = paragraph(&mut doc, link, None);
    let pic = Drawing::picture(doc.alloc_id(), "/word/media/pasted.png", 914_400, 457_200);
    let picture = paragraph(
        &mut doc,
        Fragment::object(InlineObject::Drawing(Box::new(pic)), RunProps::default()),
        None,
    );
    doc.sections[0].blocks = vec![heading, linked, picture];
    doc.validate().unwrap();
    let out = write_docx(&doc, None, &WriteOptions::default()).unwrap();
    assert!(out.issues.is_empty(), "{:?}", out.issues);
    let back = read_docx(&out.bytes).unwrap();
    assert_eq!(back.document.sections[0].blocks.len(), 3);
    let texts: Vec<String> = back.document.sections[0]
        .blocks
        .iter()
        .filter_map(|b| b.as_paragraph().map(Paragraph::text))
        .collect();
    assert_eq!(texts[..2], ["Heading", "Kiln"]);
    let Block::Paragraph(lp) = &back.document.sections[0].blocks[1] else {
        panic!("paragraph");
    };
    assert!(lp.content.marks().iter().any(|m| matches!(&m.mark, Mark::Link(l) if l.target == LinkTarget::Url("https://example.com/kiln".into()) && l.tooltip.as_deref() == Some("Visit"))));
    let Block::Paragraph(pp) = &back.document.sections[0].blocks[2] else {
        panic!("paragraph");
    };
    let Some(InlineObject::Drawing(d)) = pp.content.objects().first() else {
        panic!("drawing");
    };
    assert_eq!((d.width, d.height), (914_400, 457_200));
    assert!(
        matches!(&d.kind, kiln_doc::DrawingKind::Picture { media, .. } if back.document.media.contains_key(media))
    );
}

#[test]
fn what_cannot_be_written_is_reported() {
    let mut doc = Document::new();
    let pic = Drawing::picture(doc.alloc_id(), "/word/media/missing.png", 10, 10);
    let block = paragraph(
        &mut doc,
        Fragment::object(InlineObject::Drawing(Box::new(pic)), RunProps::default()),
        None,
    );
    doc.sections[0].blocks = vec![block];
    let out = write_docx(&doc, None, &WriteOptions::default()).unwrap();
    assert!(
        out.issues.iter().any(|i| i.code == "image.missing"),
        "{:?}",
        out.issues
    );
    assert!(read_docx(&out.bytes).is_ok());
    let report = compat_report(&doc, Default::default());
    assert!(
        report
            .iter()
            .any(|i| i.code == "image.missing" && i.count == 1)
    );
}

#[test]
fn inconsistent_documents_are_refused() {
    let mut doc = Document::new();
    doc.sections[0].blocks.clear();
    assert!(matches!(
        write_docx(&doc, None, &WriteOptions::default()),
        Err(kiln_io_docx::DocxWriteError::InvalidDocument(_))
    ));
}
