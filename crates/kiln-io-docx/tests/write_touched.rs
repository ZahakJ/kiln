// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving after edits: the parts the edits touched are written from the model, every other part
//! — styles, numbering, settings, headers, notes, comments, media, passthrough parts and their
//! relationships — is the original, byte for byte (PLAN.md §3.10).

mod roundtrip_support;

use std::collections::BTreeSet;

use kiln_doc::st::NoteKind;
use kiln_doc::{Block, CommentMeta, Document, Fragment, Op, RunProps, Transaction};
use kiln_io_docx::{DocxWritten, TouchedParts, WriteOptions, WriteSource, read_docx, write_docx};
use kiln_ooxml::opc::OpcPackage;
use roundtrip_support::SAVE_TIME;

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(kiln_testing::fixtures::fixtures_dir().join(rel)).expect("fixture")
}

/// Opens `original`, applies `edits` (recording what they touch), saves.
fn save_after(
    original: &[u8],
    edits: impl FnOnce(&mut Document) -> Vec<Transaction>,
) -> (DocxWritten, TouchedParts) {
    let pkg = OpcPackage::open(original).unwrap();
    let read = kiln_io_docx::read_package(&pkg).unwrap();
    let mut doc = read.document.clone();
    let mut touched = TouchedParts::new();
    for tx in edits(&mut doc) {
        doc.apply(tx.clone()).expect("edit applies");
        touched.record(&doc, &tx);
    }
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let out = write_docx(
        &doc,
        Some(&source),
        &WriteOptions {
            now: Some(SAVE_TIME.to_owned()),
            preserve: Some(&touched),
            ..WriteOptions::default()
        },
    )
    .unwrap();
    (out, touched)
}

/// Parts whose bytes differ between the original and the saved file (`[Content_Types].xml`
/// excluded: it is always regenerated).
fn changed(original: &[u8], saved: &[u8]) -> BTreeSet<String> {
    let (a, b) = (
        OpcPackage::open(original).unwrap(),
        OpcPackage::open(saved).unwrap(),
    );
    let mut out: BTreeSet<String> = a
        .all_part_names()
        .filter(|n| a.part(n).unwrap() != b.part(n).unwrap())
        .map(str::to_owned)
        .collect();
    out.extend(b.all_part_names().filter(|n| !a.has(n)).map(str::to_owned));
    out
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

fn first_body_paragraph(doc: &Document) -> kiln_doc::NodeId {
    doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| b.as_paragraph().map(|p| p.id))
        .expect("paragraph")
}

#[test]
fn an_unedited_save_changes_only_the_properties() {
    for rel in [
        "docx/features/sections.docx",
        "docx/features/footnotes.docx",
        "docx/features/comments.docx",
        "docx/features/images-anchored.docx",
        "external/docx/poi-headerPic.docx",
    ] {
        let path = kiln_testing::fixtures::fixtures_dir().join(rel);
        if !path.exists() {
            continue; // the external corpus is optional
        }
        let original = fixture(rel);
        let (out, _) = save_after(&original, |_| Vec::new());
        assert_eq!(
            changed(&original, &out.bytes),
            set(&["/docProps/app.xml", "/docProps/core.xml"]),
            "{rel}"
        );
        assert!(
            out.preserved.contains(&"/word/document.xml".to_owned()),
            "{rel}"
        );
    }
}

#[test]
fn typing_in_the_body_regenerates_only_the_body() {
    let original = fixture("docx/features/sections.docx");
    let stories = read_docx(&original).unwrap().document.headers_footers.len();
    let (out, touched) = save_after(&original, |doc| {
        let para = first_body_paragraph(doc);
        vec![Transaction::new(vec![Op::InsertInline {
            para,
            offset: 0,
            fragment: Fragment::text("Edited ", RunProps::default()),
        }])]
    });
    assert!(touched.body && !touched.styles && touched.headers_footers.is_empty());
    assert_eq!(
        changed(&original, &out.bytes),
        set(&[
            "/docProps/app.xml",
            "/docProps/core.xml",
            "/word/document.xml"
        ])
    );
    let back = read_docx(&out.bytes).unwrap();
    let first = back.document.sections[0].blocks[0].as_paragraph().unwrap();
    assert!(first.text().starts_with("Edited "));
    assert_eq!(back.document.headers_footers.len(), stories);
}

#[test]
fn editing_a_header_regenerates_that_header_only() {
    let original = fixture("docx/features/sections.docx");
    let read = read_docx(&original).unwrap();
    let header = read.document.headers_footers[0].id;
    let part = read.extras.header_footer_parts[&header].clone();
    let (out, touched) = save_after(&original, |doc| {
        let para = doc.headers_footers[0].blocks[0].id();
        vec![Transaction::new(vec![Op::InsertInline {
            para,
            offset: 0,
            fragment: Fragment::text("New ", RunProps::default()),
        }])]
    });
    assert!(touched.header_footer(header) && !touched.body);
    assert_eq!(
        changed(&original, &out.bytes),
        set(&["/docProps/app.xml", "/docProps/core.xml", &part])
    );
}

#[test]
fn changing_a_style_leaves_the_body_untouched() {
    let original = fixture("docx/features/styles.docx");
    let (out, _) = save_after(&original, |doc| {
        let mut style = doc.styles.get("Heading1").expect("Heading1").clone();
        style.rpr.get_or_insert_with(Default::default).italic = Some(true);
        let index = doc.styles.index_of("Heading1").unwrap();
        vec![Transaction::new(vec![Op::PutStyle {
            style: Box::new(style),
            index,
        }])]
    });
    assert_eq!(
        changed(&original, &out.bytes),
        set(&[
            "/docProps/app.xml",
            "/docProps/core.xml",
            "/word/styles.xml"
        ])
    );
    let back = read_docx(&out.bytes).unwrap();
    let h = back.document.styles.get("Heading1").unwrap();
    assert_eq!(h.rpr.as_ref().and_then(|r| r.italic), Some(true));
}

#[test]
fn resolving_a_comment_touches_only_the_comments() {
    let original = fixture("docx/features/comments.docx");
    let (out, touched) = save_after(&original, |doc| {
        let c = &doc.comments[0];
        vec![Transaction::new(vec![Op::SetCommentMeta {
            id: c.id,
            meta: CommentMeta {
                resolved: true,
                ..c.meta.clone()
            },
        }])]
    });
    assert!(touched.comments && !touched.body);
    let changed = changed(&original, &out.bytes);
    assert!(!changed.contains("/word/document.xml"), "{changed:?}");
    assert!(changed.contains("/word/comments.xml"));
    let back = read_docx(&out.bytes).unwrap();
    assert!(back.document.comments[0].meta.resolved);
}

#[test]
fn editing_a_footnote_regenerates_the_notes_part() {
    let original = fixture("docx/features/footnotes.docx");
    let (out, touched) = save_after(&original, |doc| {
        let note = doc
            .notes
            .iter()
            .find(|n| n.kind == NoteKind::Footnote && n.note_type == kiln_doc::st::NoteType::Normal)
            .expect("footnote");
        let para = note.blocks[0].id();
        vec![Transaction::new(vec![Op::InsertInline {
            para,
            offset: 1,
            fragment: Fragment::text("[edited]", RunProps::default()),
        }])]
    });
    assert!(touched.footnotes && !touched.endnotes && !touched.body);
    assert_eq!(
        changed(&original, &out.bytes),
        set(&[
            "/docProps/app.xml",
            "/docProps/core.xml",
            "/word/footnotes.xml"
        ])
    );
    let back = read_docx(&out.bytes).unwrap();
    assert!(back.document.notes.iter().any(|n| {
        n.blocks
            .iter()
            .any(|b| matches!(b, Block::Paragraph(p) if p.text().contains("[edited]")))
    }));
}

#[test]
fn everything_touched_regenerates_every_model_part() {
    let original = fixture("docx/features/sections.docx");
    let pkg = OpcPackage::open(&original).unwrap();
    let read = kiln_io_docx::read_package(&pkg).unwrap();
    let all = TouchedParts::all();
    let out = write_docx(
        &read.document,
        Some(&WriteSource::from_read(&read).with_package(&pkg)),
        &WriteOptions {
            preserve: Some(&all),
            ..WriteOptions::default()
        },
    )
    .unwrap();
    assert!(out.preserved.is_empty());
    assert!(out.regenerated.contains(&"/word/document.xml".to_owned()));
}
