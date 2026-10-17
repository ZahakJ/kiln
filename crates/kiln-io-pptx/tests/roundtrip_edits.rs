// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edited-then-saved presentations, the way a session saves them: each case opens a feature
//! fixture, applies one Deck command through a transaction (recording the parts it touches),
//! saves, and checks that
//! - the edit reads back from the saved file,
//! - every part the edit did not touch is the original's bytes (slides, layouts, masters, notes,
//!   media, passthrough parts),
//! - every part the save rewrote equals the expected output committed under
//!   `fixtures/pptx/roundtrip/<case>/`,
//! - LibreOffice opens the result (when it is installed).
//!
//! After an intended writer change, regenerate the expected files with
//! `KILN_BLESS=1 cargo nextest run -p kiln-io-pptx -E 'binary(roundtrip_edits)'` and review
//! the diff.

mod read_support;

use std::path::PathBuf;

use kiln_deck::edit;
use kiln_deck::edit::comments::Byline;
use kiln_deck::ops::TextTarget;
use kiln_deck::transition::{MorphBy, Transition, TransitionEffect};
use kiln_deck::{Presentation, Transaction};
use kiln_io_pptx::{PptxRead, TouchedParts, WriteOptions, WriteSource, read_pptx, write_pptx};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fixtures::fixtures_dir;

/// The save time the tests stamp.
const SAVE_TIME: &str = "2026-10-16T12:00:00Z";

/// Opens `fixture`, applies `edit`, saves with touched-part preservation; returns the original
/// package, the saved one and its read.
fn save_edited(
    fixture: &str,
    edit: impl FnOnce(&mut Presentation) -> Transaction,
) -> (OpcPackage, OpcPackage, PptxRead, Vec<String>) {
    let bytes = std::fs::read(fixtures_dir().join(fixture)).expect("fixture");
    let pkg = OpcPackage::open(&bytes).expect("opens");
    let read = kiln_io_pptx::read_package(&pkg).expect("reads");
    let mut pres = read.presentation.clone();
    let tx = edit(&mut pres);
    pres.apply(tx.clone()).expect("edit applies");
    let mut touched = TouchedParts::new();
    touched.record(&pres, &tx);
    assert!(
        !touched.everything,
        "{fixture}: the edit could not be placed"
    );
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let out = write_pptx(
        &pres,
        Some(&source),
        &WriteOptions {
            now: Some(SAVE_TIME.to_owned()),
            preserve: Some(&touched),
            ..WriteOptions::default()
        },
    )
    .expect("writes");
    let back = read_pptx(&out.bytes).expect("the saved file reads");
    (
        pkg,
        OpcPackage::open(&out.bytes).expect("saved file opens"),
        back,
        out.preserved,
    )
}

/// The golden file name of a part (`/ppt/slides/slide1.xml` → `ppt_slides_slide1.xml`).
fn golden_name(part: &str) -> String {
    part.trim_start_matches('/').replace('/', "_")
}

/// Compares every part the save changed with its golden file; returns the read back.
fn check(
    case: &str,
    fixture: &str,
    edit: impl FnOnce(&mut Presentation) -> Transaction,
) -> (PptxRead, Vec<String>, Vec<String>) {
    let (original, saved, back, preserved) = save_edited(fixture, edit);
    let dir: PathBuf = fixtures_dir().join("pptx/roundtrip").join(case);
    let bless = std::env::var_os("KILN_BLESS").is_some();
    let mut compared = 0;
    let mut changed = Vec::new();
    for name in saved.all_part_names() {
        let now = saved.part(name).expect("readable").expect("present");
        let before = original.part(name).expect("readable");
        if before.as_deref() == Some(&*now) {
            continue;
        }
        changed.push(name.to_owned());
        if name.starts_with("/docProps/") {
            // Stamped on every save; covered by write_package.rs.
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
    let goldens = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .count();
    assert_eq!(
        compared,
        goldens,
        "{case}: stale golden files in {}",
        dir.display()
    );
    // LibreOffice opens the result.
    if let Ok(oracle) = kiln_testing::oracle::Oracle::locate() {
        let scratch =
            std::env::temp_dir().join(format!("kiln-pptx-edit-{case}-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("scratch");
        let file = scratch.join(format!("{case}.pptx"));
        std::fs::write(&file, saved.save().expect("save")).expect("write");
        assert!(
            oracle.opens(&file, &scratch),
            "{case}: LibreOffice cannot open the result"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
    (back, changed, preserved)
}

fn first_title(p: &Presentation) -> kiln_deck::NodeId {
    p.slides[0]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|ph| ph.kind.is_title()))
        .map(|s| s.id)
        .expect("a title")
}

#[test]
fn retitling_a_slide_rewrites_that_slide_only() {
    let (back, changed, preserved) = check("retitle", "pptx/features/placeholders.pptx", |p| {
        let title = first_title(p);
        edit::set_text(p, TextTarget::Shape(title), "Quarterly review").expect("edit")
    });
    let title = back.presentation.slides[0]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|ph| ph.kind.is_title()))
        .and_then(|s| s.text())
        .map(kiln_io_pptx::body_text);
    assert_eq!(title.as_deref(), Some("Quarterly review"));
    assert!(
        changed.contains(&"/ppt/slides/slide1.xml".to_owned()),
        "{changed:?}"
    );
    for part in &changed {
        assert!(
            part == "/ppt/slides/slide1.xml"
                || part == "/ppt/presentation.xml"
                || part.starts_with("/docProps/"),
            "{part} changed although the edit did not touch it"
        );
    }
    assert!(
        preserved.iter().any(|p| p.contains("slideLayout")),
        "{preserved:?}"
    );
    assert!(
        preserved.iter().any(|p| p.contains("slideMaster")),
        "{preserved:?}"
    );
}

#[test]
fn deleting_a_slide_drops_its_part_and_keeps_the_others() {
    let (back, changed, _) = check("delete-slide", "pptx/features/many-slides.pptx", |p| {
        let second = p.slides[1].id;
        edit::delete_slides(p, &[second]).expect("edit")
    });
    let original = kiln_io_pptx::read_pptx(
        &std::fs::read(fixtures_dir().join("pptx/features/many-slides.pptx")).expect("fixture"),
    )
    .expect("reads");
    assert_eq!(
        back.presentation.slides.len() + 1,
        original.presentation.slides.len()
    );
    // Only the presentation (its slide list) and its relationships change.
    for part in &changed {
        assert!(
            part == "/ppt/presentation.xml"
                || part == "/ppt/_rels/presentation.xml.rels"
                || part.starts_with("/docProps/"),
            "{part} changed although the edit did not touch it"
        );
    }
}

#[test]
fn inserting_a_slide_adds_a_part() {
    let (back, changed, _) = check("insert-slide", "pptx/features/aspect-4x3.pptx", |p| {
        let layout = p.slides[0].layout;
        edit::insert_slide(p, layout, 1).expect("edit")
    });
    assert_eq!(back.presentation.slides.len(), 3);
    assert!(
        changed.iter().any(|p| p.starts_with("/ppt/slides/slide3")),
        "{changed:?}"
    );
}

#[test]
fn a_new_transition_is_written_with_its_fallback() {
    let (back, _, _) = check("morph", "pptx/features/transitions.pptx", |p| {
        let id = p.slides[0].id;
        let mut t = Transition::new(TransitionEffect::Morph { by: MorphBy::Words });
        t.duration_ms = 1600;
        edit::set_transition(p, &[id], Some(t)).expect("edit")
    });
    let t = back.presentation.slides[0]
        .transition
        .as_ref()
        .expect("a transition");
    assert_eq!(t.effect, TransitionEffect::Morph { by: MorphBy::Words });
    assert_eq!(t.duration_ms, 1600);
}

#[test]
fn a_comment_with_a_reply_is_saved_as_a_threaded_comment() {
    let (back, changed, _) = check("comment-reply", "pptx/features/notes.pptx", |p| {
        let slide = p.slides[0].id;
        let ana = Byline {
            author: "Ana Lima",
            initials: "AL",
            created: "2026-10-16T09:00:00.000",
        };
        let tx = edit::add_comment(
            p,
            slide,
            None,
            (914_400, 457_200),
            &ana,
            "Check the numbers",
        )
        .expect("comment");
        p.apply(tx).expect("applies");
        let cid = p.slides[0].comments[0].id;
        let bea = Byline {
            author: "Bea Ruiz",
            initials: "BR",
            created: "2026-10-16T10:00:00.000",
        };
        edit::reply_to_comment(p, cid, &bea, "Done").expect("reply")
    });
    let c = &back.presentation.slides[0].comments;
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].author, "Ana Lima");
    assert_eq!(c[0].text, "Check the numbers");
    assert_eq!(c[0].replies.len(), 1);
    assert_eq!(c[0].replies[0].author, "Bea Ruiz");
    assert!(
        changed.iter().any(|p| p.contains("modernComment")),
        "{changed:?}"
    );
    assert!(
        changed.iter().any(|p| p == "/ppt/authors.xml"),
        "{changed:?}"
    );
}
