// SPDX-License-Identifier: AGPL-3.0-or-later
//! new documents in the editing language (names, a right-to-left workbook), document
//! properties through `.docx`, `.xlsx`, `.pptx` and the native bundles, Inspect Document and
//! the options the core applies.

use kiln_common::DocId;
use kiln_protocol::commands::write_features::PropertiesEdit;
use kiln_protocol::commands::{
    CoreOptions, DeckCommand, DocPropertiesInfo, GridCommand, GridQuery, GridQueryResult,
};
use kiln_protocol::{
    Command, DocKind, Event, InspectItem, InspectKind, Outcome, Query, QueryResult, SuiteCommand,
    WriteCommand,
};
use kiln_session::{NativeHost, Session};

fn new_doc(s: &mut Session, kind: DocKind, language: Option<&str>) -> (DocId, String) {
    let summary = s
        .execute(
            None,
            SuiteCommand::New {
                kind,
                language: language.map(str::to_owned),
            }
            .into(),
        )
        .unwrap()
        .summary
        .unwrap();
    (summary.doc, summary.title)
}

fn run(s: &mut Session, doc: DocId, cmd: impl Into<Command>) -> Outcome {
    s.execute(Some(doc), cmd.into()).unwrap().outcome
}

fn props(s: &Session, doc: DocId) -> DocPropertiesInfo {
    match s.query(Some(doc), &Query::Properties).unwrap() {
        QueryResult::Properties(p) => *p,
        other => panic!("{other:?}"),
    }
}

fn inspect(s: &Session, doc: DocId) -> Vec<InspectItem> {
    match s.query(Some(doc), &Query::Inspect).unwrap() {
        QueryResult::Inspect(i) => i,
        other => panic!("{other:?}"),
    }
}

fn count(items: &[InspectItem], kind: InspectKind) -> u32 {
    items
        .iter()
        .find(|i| i.kind == kind)
        .map_or_else(|| panic!("{kind:?} not inspected: {items:?}"), |i| i.count)
}

fn grid_state(s: &Session, doc: DocId) -> kiln_protocol::commands::GridState {
    match s.query(Some(doc), &Query::Grid(GridQuery::State)).unwrap() {
        QueryResult::Grid(GridQueryResult::State(st)) => *st,
        other => panic!("{other:?}"),
    }
}

fn temp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("kiln-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

fn edit() -> PropertiesEdit {
    PropertiesEdit {
        title: Some("Firing schedule «2027»".into()),
        subject: Some("Kilns".into()),
        author: Some("Ana Ruiz".into()),
        keywords: Some("glaze; bisque".into()),
        description: Some("مرحبا".into()),
        category: Some("Studio".into()),
        status: Some("Draft".into()),
        company: Some("Kiln & Co".into()),
        manager: Some("Sam".into()),
        last_modified_by: None,
    }
}

/// Saves as `path`, opens it again and returns the reopened document.
fn save_and_reopen(s: &mut Session, doc: DocId, path: &str) -> DocId {
    let r = s
        .execute(
            Some(doc),
            SuiteCommand::SaveAs {
                path: Some(path.to_owned()),
                format: None,
            }
            .into(),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    s.poll();
    s.execute(
        None,
        SuiteCommand::Open {
            path: Some(path.to_owned()),
        }
        .into(),
    )
    .unwrap();
    s.poll()
        .iter()
        .find_map(|e| match e {
            Event::Opened { summary } => Some(summary.doc),
            _ => None,
        })
        .expect("opened")
}

#[test]
fn new_documents_are_named_in_their_language() {
    let mut s = Session::new(Box::new(NativeHost));
    assert_eq!(new_doc(&mut s, DocKind::Write, None).1, "Document1");
    assert_eq!(
        new_doc(&mut s, DocKind::Write, Some("de-DE")).1,
        "Dokument2"
    );
    assert_eq!(new_doc(&mut s, DocKind::Write, Some("ar-SA")).1, "مستند3");
    assert_eq!(new_doc(&mut s, DocKind::Grid, Some("de-DE")).1, "Mappe1");
    assert_eq!(new_doc(&mut s, DocKind::Grid, Some("ar-SA")).1, "مصنف2");
    assert_eq!(
        new_doc(&mut s, DocKind::Deck, Some("de-DE")).1,
        "Präsentation1"
    );
    assert_eq!(
        new_doc(&mut s, DocKind::Deck, Some("ar-EG")).1,
        "عرض تقديمي2"
    );
    assert_eq!(new_doc(&mut s, DocKind::Deck, None).1, "Presentation3");
}

fn status_language(s: &Session, doc: DocId) -> Option<String> {
    match s.query(Some(doc), &Query::Status).unwrap() {
        QueryResult::Status(st) => st.language,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_status_bar_shows_the_language_new_documents_are_written_in() {
    let mut s = Session::new(Box::new(NativeHost));
    for (lang, shown) in [
        (Some("ar-SA"), "ar-SA"),
        (Some("de-DE"), "de-DE"),
        (None, "en-US"),
    ] {
        let (write, _) = new_doc(&mut s, DocKind::Write, lang);
        assert_eq!(
            status_language(&s, write).as_deref(),
            Some(shown),
            "{lang:?}"
        );
        let (deck, _) = new_doc(&mut s, DocKind::Deck, lang);
        assert_eq!(
            status_language(&s, deck).as_deref(),
            Some(shown),
            "{lang:?}"
        );
        // The presentation's language is not an edit to undo or save.
        let summary = s.summary(deck).unwrap();
        assert!(!summary.can_undo && !summary.dirty, "{lang:?}");
    }
    // A presentation keeps its language through PowerPoint's format.
    let (deck, _) = new_doc(&mut s, DocKind::Deck, Some("ar-SA"));
    let path = temp("settings-language.pptx");
    let again = save_and_reopen(&mut s, deck, &path);
    assert_eq!(status_language(&s, again).as_deref(), Some("ar-SA"));
}

#[test]
fn a_new_workbook_in_a_right_to_left_language_opens_right_to_left() {
    let mut s = Session::new(Box::new(NativeHost));
    for (lang, sheet, rtl) in [
        (Some("ar-SA"), "ورقة1", true),
        (Some("he-IL"), "Sheet1", true),
        (Some("fa-IR"), "Sheet1", true),
        (Some("ur-PK"), "Sheet1", true),
        (Some("de-DE"), "Tabelle1", false),
        (None, "Sheet1", false),
    ] {
        let (doc, _) = new_doc(&mut s, DocKind::Grid, lang);
        let st = grid_state(&s, doc);
        assert_eq!(st.sheets[0].name, sheet, "{lang:?}");
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(json.contains("\"rtl\":true"), rtl, "{lang:?}");
        // A new workbook has nothing to undo or save.
        let summary = s.summary(doc).unwrap();
        assert!(!summary.can_undo && !summary.dirty, "{lang:?}");
    }
    // New sheets follow the first one's language.
    let (doc, _) = new_doc(&mut s, DocKind::Grid, Some("de-DE"));
    run(&mut s, doc, GridCommand::InsertSheet);
    let names: Vec<String> = grid_state(&s, doc)
        .sheets
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(names.contains(&"Tabelle2".to_owned()), "{names:?}");
}

#[test]
fn properties_round_trip_through_office_files_and_bundles() {
    let mut s = Session::new(Box::new(NativeHost));
    for (kind, ext, bundle) in [
        (DocKind::Write, "docx", "kwrite"),
        (DocKind::Grid, "xlsx", "kgrid"),
        (DocKind::Deck, "pptx", "kdeck"),
    ] {
        let (doc, _) = new_doc(&mut s, kind, None);
        assert_eq!(
            run(
                &mut s,
                doc,
                SuiteCommand::SetProperties {
                    props: Box::new(edit())
                }
            ),
            Outcome::Applied,
            "{kind:?}"
        );
        assert!(s.summary(doc).unwrap().dirty, "{kind:?}");
        let p = props(&s, doc);
        assert_eq!(p.title.as_deref(), Some("Firing schedule «2027»"));
        assert_eq!(p.company.as_deref(), Some("Kiln & Co"));
        // The same values again change nothing.
        assert_eq!(
            run(
                &mut s,
                doc,
                SuiteCommand::SetProperties {
                    props: Box::new(edit())
                }
            ),
            Outcome::Unchanged
        );

        let file = temp(&format!("props.{ext}"));
        let again = save_and_reopen(&mut s, doc, &file);
        // The core properties in the package, as Office reads them.
        let pkg = kiln_ooxml::opc::OpcPackage::open(&std::fs::read(&file).unwrap()).unwrap();
        let read = kiln_ooxml::props::read_document_properties(&pkg).unwrap();
        assert_eq!(
            read.core.title.as_deref(),
            Some("Firing schedule «2027»"),
            "{ext}"
        );
        assert_eq!(read.core.creator.as_deref(), Some("Ana Ruiz"), "{ext}");
        assert_eq!(
            read.core.keywords.as_deref(),
            Some("glaze; bisque"),
            "{ext}"
        );
        assert_eq!(read.core.category.as_deref(), Some("Studio"), "{ext}");
        assert_eq!(read.app.company.as_deref(), Some("Kiln & Co"), "{ext}");
        let p = props(&s, again);
        assert_eq!(p.subject.as_deref(), Some("Kilns"), "{ext}");
        assert_eq!(p.description.as_deref(), Some("مرحبا"), "{ext}");
        assert_eq!(p.manager.as_deref(), Some("Sam"), "{ext}");
        assert_eq!(p.status.as_deref(), Some("Draft"), "{ext}");

        // The native bundle keeps them too.
        let native = temp(&format!("props.{bundle}"));
        let third = save_and_reopen(&mut s, again, &native);
        assert_eq!(
            props(&s, third).title.as_deref(),
            Some("Firing schedule «2027»")
        );
        assert_eq!(props(&s, third).company.as_deref(), Some("Kiln & Co"));
    }
}

#[test]
fn inspect_document_finds_and_removes_hidden_data() {
    let mut s = Session::new(Box::new(NativeHost));

    // Write: comments, tracked changes, personal information.
    let (w, _) = new_doc(&mut s, DocKind::Write, None);
    run(
        &mut s,
        w,
        WriteCommand::InsertText {
            text: "Bisque at 950".into(),
        },
    );
    run(&mut s, w, WriteCommand::TrackChanges);
    run(
        &mut s,
        w,
        WriteCommand::InsertText {
            text: " degrees".into(),
        },
    );
    run(
        &mut s,
        w,
        SuiteCommand::SetProperties {
            props: Box::new(edit()),
        },
    );
    let found = inspect(&s, w);
    assert!(count(&found, InspectKind::Revisions) >= 1, "{found:?}");
    assert_eq!(count(&found, InspectKind::Properties), 3);
    for kind in [InspectKind::Revisions, InspectKind::Properties] {
        run(&mut s, w, SuiteCommand::InspectRemove { item: kind });
        assert_eq!(count(&inspect(&s, w), kind), 0, "{kind:?}");
    }
    assert_eq!(
        props(&s, w).title.as_deref(),
        Some("Firing schedule «2027»")
    );

    // Grid: notes, hidden sheets.
    let (g, _) = new_doc(&mut s, DocKind::Grid, None);
    run(
        &mut s,
        g,
        GridCommand::SetNote {
            text: Some("Check the cones".into()),
            author: "Ana".into(),
            cell: Some("B2".into()),
        },
    );
    run(&mut s, g, GridCommand::InsertSheet);
    run(&mut s, g, GridCommand::InsertSheet);
    let st = grid_state(&s, g);
    let last = u32::try_from(st.sheets.len() - 1).unwrap();
    run(&mut s, g, GridCommand::HideSheet { index: last });
    let found = inspect(&s, g);
    assert_eq!(count(&found, InspectKind::Comments), 1);
    assert_eq!(count(&found, InspectKind::HiddenSheets), 1);
    let sheets = grid_state(&s, g).sheets.len();
    run(
        &mut s,
        g,
        SuiteCommand::InspectRemove {
            item: InspectKind::Comments,
        },
    );
    run(
        &mut s,
        g,
        SuiteCommand::InspectRemove {
            item: InspectKind::HiddenSheets,
        },
    );
    let found = inspect(&s, g);
    assert_eq!(count(&found, InspectKind::Comments), 0);
    assert_eq!(count(&found, InspectKind::HiddenSheets), 0);
    assert_eq!(grid_state(&s, g).sheets.len(), sheets - 1);
    // One undo step each: Undo brings the hidden sheet back.
    run(&mut s, g, SuiteCommand::Undo);
    assert_eq!(count(&inspect(&s, g), InspectKind::HiddenSheets), 1);

    // Deck: speaker notes, comments, hidden slides.
    let (d, _) = new_doc(&mut s, DocKind::Deck, None);
    run(
        &mut s,
        d,
        Command::Deck(DeckCommand::NewSlide { layout: None }),
    );
    run(
        &mut s,
        d,
        Command::Deck(DeckCommand::SetNotes {
            text: "Smile.".into(),
        }),
    );
    run(
        &mut s,
        d,
        Command::Deck(DeckCommand::NewComment {
            text: "Bigger title?".into(),
            author: "Ana".into(),
            initials: "AR".into(),
        }),
    );
    run(
        &mut s,
        d,
        Command::Deck(DeckCommand::HideSlides { hidden: Some(true) }),
    );
    let found = inspect(&s, d);
    assert_eq!(count(&found, InspectKind::SpeakerNotes), 1, "{found:?}");
    assert_eq!(count(&found, InspectKind::Comments), 1, "{found:?}");
    assert_eq!(count(&found, InspectKind::HiddenSlides), 1, "{found:?}");
    for kind in [
        InspectKind::SpeakerNotes,
        InspectKind::Comments,
        InspectKind::HiddenSlides,
    ] {
        assert_eq!(
            run(&mut s, d, SuiteCommand::InspectRemove { item: kind }),
            Outcome::Applied,
            "{kind:?}"
        );
        assert_eq!(count(&inspect(&s, d), kind), 0, "{kind:?}");
    }
    // A kind the app does not have is refused.
    assert!(
        s.execute(
            Some(d),
            SuiteCommand::InspectRemove {
                item: InspectKind::HiddenSheets
            }
            .into()
        )
        .is_err()
    );
}

#[test]
fn options_set_the_autosave_wait_and_the_words_the_speller_skips() {
    let mut s = Session::new(Box::new(NativeHost));
    let r = s
        .execute(
            None,
            SuiteCommand::SetOptions {
                options: CoreOptions {
                    autosave_delay_s: 30,
                    recovery: false,
                    spell_ignore_uppercase: false,
                    spell_ignore_numbers: true,
                    spell_ignore_addresses: true,
                    ui_language: String::new(),
                },
            }
            .into(),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    assert!(!kiln_spell::skip_rules().uppercase);
    // Back to Word's defaults (the rules are the process's).
    s.execute(
        None,
        SuiteCommand::SetOptions {
            options: CoreOptions::default(),
        }
        .into(),
    )
    .unwrap();
    assert_eq!(kiln_spell::skip_rules(), kiln_spell::SkipRules::default());
}
