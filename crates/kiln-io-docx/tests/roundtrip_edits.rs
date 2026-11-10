// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edited-then-saved golden files: each case opens a feature fixture, applies one edit through a
//! transaction (recording the parts it touches, as the session does), saves, and compares every
//! part the save regenerated with the expected output committed under
//! `fixtures/docx/roundtrip/<case>/`. Parts the edit did not touch must be the original bytes.
//!
//! After an intended writer change, regenerate the expected files with
//! `KILN_BLESS=1 cargo nextest run -p kiln-io-docx -E 'binary(roundtrip_edits)'` and review the diff.

mod roundtrip_support;

use std::path::PathBuf;

use kiln_doc::{Document, Fragment, NodeId, Op, RunProps, Transaction};
use kiln_io_docx::{TouchedParts, WriteOptions, WriteSource, write_docx};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fixtures::fixtures_dir;
use roundtrip_support::SAVE_TIME;

fn first_paragraph(doc: &Document) -> NodeId {
    doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| b.as_paragraph().map(|p| p.id))
        .expect("a body paragraph")
}

fn type_at_start(doc: &Document, text: &str) -> Transaction {
    Transaction::new(vec![Op::InsertInline {
        para: first_paragraph(doc),
        offset: 0,
        fragment: Fragment::text(text, RunProps::default()),
    }])
}

/// Opens `fixture`, applies `edit`, saves with touched-part preservation; returns the original
/// and saved packages.
fn save_edited(
    fixture: &str,
    edit: impl FnOnce(&Document) -> Transaction,
) -> (OpcPackage, OpcPackage) {
    let bytes = std::fs::read(fixtures_dir().join(fixture)).expect("fixture");
    let pkg = OpcPackage::open(&bytes).expect("opens");
    let read = kiln_io_docx::read_package(&pkg).expect("reads");
    let mut doc = read.document.clone();
    let tx = edit(&doc);
    doc.apply(tx.clone()).expect("edit applies");
    let mut touched = TouchedParts::new();
    touched.record(&doc, &tx);
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
    .expect("writes");
    (pkg, OpcPackage::open(&out.bytes).expect("saved file opens"))
}

/// The golden file name of a part (`/word/document.xml` → `word_document.xml`).
fn golden_name(part: &str) -> String {
    part.trim_start_matches('/').replace('/', "_")
}

fn check(case: &str, fixture: &str, edit: impl FnOnce(&Document) -> Transaction) {
    let (original, saved) = save_edited(fixture, edit);
    let dir: PathBuf = fixtures_dir().join("docx/roundtrip").join(case);
    let bless = std::env::var_os("KILN_BLESS").is_some();
    let mut compared = 0;
    for name in saved.all_part_names() {
        let now = saved.part(name).expect("readable").expect("present");
        let before = original.part(name).expect("readable");
        if name == kiln_ooxml::opc::CONTENT_TYPES_PART || name.starts_with("/docProps/") {
            // Regenerated on every save; the timestamps are covered by write_package.rs.
            continue;
        }
        if before.as_deref() == Some(&*now) {
            continue;
        }
        let golden = dir.join(golden_name(name));
        if bless {
            std::fs::create_dir_all(&dir).expect("golden directory");
            std::fs::write(&golden, &*now).expect("golden written");
        }
        let expected = std::fs::read(&golden).unwrap_or_else(|e| {
            panic!(
                "{case}: {name} changed but {} is missing: {e}",
                golden.display()
            )
        });
        assert!(
            expected == *now,
            "{case}: {name} differs from {} (KILN_BLESS=1 regenerates it)",
            golden.display()
        );
        compared += 1;
    }
    // Every committed golden file must still be produced.
    let goldens = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .count();
    assert_eq!(
        compared,
        goldens,
        "{case}: stale golden files in {}",
        dir.display()
    );
}

#[test]
fn typing_into_a_document_with_tracked_changes() {
    check(
        "tracked-changes-typed",
        "docx/features/tracked-changes.docx",
        |d| type_at_start(d, "Reviewed: "),
    );
}

#[test]
fn typing_into_a_document_with_tables() {
    check("tables-typed", "docx/features/tables.docx", |d| {
        type_at_start(d, "Updated ")
    });
}

#[test]
fn typing_into_a_document_with_comments() {
    check("comments-typed", "docx/features/comments.docx", |d| {
        type_at_start(d, "Note: ")
    });
}

#[test]
fn typing_into_a_document_with_notes() {
    check("footnotes-typed", "docx/features/footnotes.docx", |d| {
        type_at_start(d, "See below. ")
    });
}
