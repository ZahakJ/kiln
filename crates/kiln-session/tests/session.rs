// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session loop end to end: documents, dispatch, availability, undo, host requests, events and
//! the session thread.

use kiln_common::{DocId, ErrorKind, KilnError, Result};
use kiln_protocol::{
    A11yTree, CaretMotion, Command, DocKind, DocSummary, Event, ExecValue, FileFormat, HostOp,
    HostReply, OpenHint, Outcome, ParagraphText, Query, QueryResult, Selection, StatusBar,
    SuiteCommand, ViewMode, Viewport, WriteCommand,
};
use kiln_session::undo::StateId;
use kiln_session::{
    AppFactory, Change, DeferredHost, DocModel, NativeHost, Session, SessionHandle,
};

fn deferred() -> Box<DeferredHost> {
    Box::new(DeferredHost::new(|| 1_792_000_000_000))
}

fn insert(text: &str) -> Command {
    WriteCommand::InsertText { text: text.into() }.into()
}

fn exec(s: &mut Session, doc: DocId, cmd: Command) -> Result<kiln_protocol::ExecResult> {
    s.execute(Some(doc), cmd)
}

fn paragraphs(s: &Session, doc: DocId) -> Vec<String> {
    match s.query(Some(doc), &Query::Paragraphs).unwrap() {
        QueryResult::Paragraphs(p) => p.into_iter().map(|p: ParagraphText| p.text).collect(),
        other => panic!("{other:?}"),
    }
}

fn new_scratch(s: &mut Session) -> DocSummary {
    s.execute(
        None,
        SuiteCommand::New {
            kind: DocKind::Scratch,
        }
        .into(),
    )
    .unwrap()
    .summary
    .unwrap()
}

#[test]
fn opens_a_scratch_document() {
    let mut s = Session::new(deferred());
    let summary = new_scratch(&mut s);
    assert_eq!(summary.kind, DocKind::Scratch);
    assert_eq!(summary.title, "Scratch1");
    assert!(!summary.dirty && !summary.can_undo && !summary.can_redo);
    assert_eq!(paragraphs(&s, summary.doc), vec![""]);
    assert!(matches!(s.poll().as_slice(), [Event::Opened { summary: x }] if *x == summary));

    let opened = s
        .open(
            b"First line\r\nSecond line",
            OpenHint {
                name: Some("Notes.txt".into()),
                ..OpenHint::default()
            },
        )
        .unwrap();
    assert_eq!(opened.title, "Notes");
    assert_eq!(opened.format, Some(FileFormat::Txt));
    assert_ne!(opened.doc, summary.doc);
    assert_eq!(
        paragraphs(&s, opened.doc),
        vec!["First line", "Second line"]
    );
    assert_eq!(new_scratch(&mut s).title, "Scratch2");

    match s.query(None, &Query::Documents).unwrap() {
        QueryResult::Documents(d) => assert_eq!(d.len(), 3),
        other => panic!("{other:?}"),
    }
    let err = s.open(
        b"PK",
        OpenHint {
            name: Some("a.xlsx".into()),
            ..OpenHint::default()
        },
    );
    assert_eq!(err.unwrap_err().kind(), ErrorKind::Unsupported);
    let err = s.open(b"x", OpenHint::default());
    assert_eq!(err.unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn edits_undo_and_redo_change_the_summary() {
    let mut s = Session::new(deferred());
    let doc = new_scratch(&mut s).doc;
    s.poll();

    let r = exec(&mut s, doc, insert("Hello")).unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    let after_insert = r.summary.unwrap();
    assert_eq!(after_insert.revision, 1);
    assert!(after_insert.dirty && after_insert.can_undo && !after_insert.can_redo);
    assert_eq!(
        after_insert.undo_label.as_deref(),
        Some("write.edit.insert_text")
    );
    assert_eq!(paragraphs(&s, doc), vec!["Hello"]);
    match s.poll().as_slice() {
        [
            Event::Changed {
                summary,
                status,
                selection,
            },
        ] => {
            assert_eq!(summary, &after_insert);
            assert_eq!(status.words, Some(1));
            assert_eq!(status.chars, Some(5));
            assert!(!selection.is_non_empty());
        }
        other => panic!("{other:?}"),
    }

    let r = exec(&mut s, doc, WriteCommand::DeleteBackward.into()).unwrap();
    let after_delete = r.summary.unwrap();
    assert_eq!(after_delete.revision, 2);
    assert_eq!(
        after_delete.undo_label.as_deref(),
        Some("write.edit.delete_backward")
    );
    assert_eq!(paragraphs(&s, doc), vec!["Hell"]);

    let undo = exec(&mut s, doc, SuiteCommand::Undo.into())
        .unwrap()
        .summary
        .unwrap();
    assert_eq!(paragraphs(&s, doc), vec!["Hello"]);
    assert_eq!(undo.revision, 3);
    assert!(undo.can_undo && undo.can_redo && undo.dirty);
    assert_eq!(
        undo.redo_label.as_deref(),
        Some("write.edit.delete_backward")
    );

    let undo = exec(&mut s, doc, SuiteCommand::Undo.into())
        .unwrap()
        .summary
        .unwrap();
    assert_eq!(paragraphs(&s, doc), vec![""]);
    assert!(!undo.can_undo && undo.can_redo);
    assert!(!undo.dirty, "undoing everything returns to the clean state");

    let err = exec(&mut s, doc, SuiteCommand::Undo.into()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);

    let redo = exec(&mut s, doc, SuiteCommand::Redo.into())
        .unwrap()
        .summary
        .unwrap();
    assert_eq!(paragraphs(&s, doc), vec!["Hello"]);
    assert!(redo.dirty && redo.can_redo);
    exec(&mut s, doc, SuiteCommand::Redo.into()).unwrap();
    assert_eq!(paragraphs(&s, doc), vec!["Hell"]);
    let err = exec(&mut s, doc, SuiteCommand::Redo.into()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);

    let moved = exec(
        &mut s,
        doc,
        WriteCommand::MoveCaret {
            motion: CaretMotion::LineStart,
            extend: true,
        }
        .into(),
    )
    .unwrap();
    assert_eq!(moved.outcome, Outcome::Applied);
    assert_eq!(
        moved.summary.unwrap().revision,
        6,
        "selection changes do not bump the revision"
    );
    let unchanged = exec(
        &mut s,
        doc,
        WriteCommand::MoveCaret {
            motion: CaretMotion::LineStart,
            extend: true,
        }
        .into(),
    )
    .unwrap();
    assert_eq!(unchanged.outcome, Outcome::Unchanged);
    match s.query(Some(doc), &Query::Status).unwrap() {
        QueryResult::Status(st) => assert_eq!(st.chars, Some(4), "counts the selection"),
        other => panic!("{other:?}"),
    }
}

/// A stand-in Grid app, so availability can be tested before Kiln Grid exists.
#[derive(Debug)]
struct FakeGridApp;
#[derive(Debug)]
struct FakeGrid;

impl DocModel for FakeGrid {
    fn kind(&self) -> DocKind {
        DocKind::Grid
    }
    fn execute(&mut self, _: &Command) -> Result<Change> {
        Err(KilnError::Internal(
            "the session must not dispatch here".into(),
        ))
    }
    fn undo(&mut self) -> bool {
        false
    }
    fn redo(&mut self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<&'static str> {
        None
    }
    fn redo_label(&self) -> Option<&'static str> {
        None
    }
    fn state(&self) -> StateId {
        0
    }
    fn seal_history(&mut self) {}
    fn selection(&self) -> Selection {
        Selection::default()
    }
    fn status(&self) -> StatusBar {
        StatusBar::default()
    }
    fn paragraphs(&self) -> Option<Vec<ParagraphText>> {
        None
    }
    fn a11y(&self, _: &str) -> A11yTree {
        A11yTree::default()
    }
    fn save_formats(&self) -> &'static [FileFormat] {
        &[FileFormat::Csv]
    }
    fn save(&self, _: FileFormat) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

impl AppFactory for FakeGridApp {
    fn kind(&self) -> DocKind {
        DocKind::Grid
    }
    fn opens(&self, f: FileFormat) -> bool {
        f == FileFormat::Csv
    }
    fn create(&self) -> Box<dyn DocModel> {
        Box::new(FakeGrid)
    }
    fn open(&self, _: &[u8], _: FileFormat, _: &OpenHint) -> Result<Box<dyn DocModel>> {
        Ok(Box::new(FakeGrid))
    }
}

#[test]
fn availability_rejects_a_write_command_on_a_non_write_document() {
    let mut s = Session::new(deferred());
    assert_eq!(
        s.execute(
            None,
            SuiteCommand::New {
                kind: DocKind::Grid
            }
            .into()
        )
        .unwrap_err()
        .kind(),
        ErrorKind::Unsupported,
        "no Grid app is built in yet"
    );
    s.register_app(Box::new(FakeGridApp));
    let grid = s.new_document(DocKind::Grid).unwrap();
    assert_eq!(grid.title, "Book1");
    let err = exec(&mut s, grid.doc, insert("x")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);
    assert_eq!(
        err.to_string(),
        "write.edit.insert_text is not available: not a command of Kiln Grid"
    );
    // Suite commands still work there, and views are checked per app.
    exec(
        &mut s,
        grid.doc,
        SuiteCommand::SetViewMode {
            mode: ViewMode::PageBreakPreview,
        }
        .into(),
    )
    .unwrap();
    assert!(
        exec(
            &mut s,
            grid.doc,
            SuiteCommand::SetViewMode {
                mode: ViewMode::Draft
            }
            .into()
        )
        .is_err()
    );
    assert_eq!(
        s.query(Some(grid.doc), &Query::Paragraphs)
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
}

#[test]
fn commands_are_checked_against_their_metadata() {
    let mut s = Session::new(deferred());
    // Document commands need a document that exists.
    let err = s.execute(None, insert("x")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let err = s.execute(Some(DocId(99)), insert("x")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    // Read-only documents reject edits but allow caret moves.
    let ro = s
        .open(
            b"text",
            OpenHint {
                name: Some("r.txt".into()),
                read_only: true,
                ..OpenHint::default()
            },
        )
        .unwrap();
    assert!(ro.read_only);
    let err = exec(&mut s, ro.doc, insert("x")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "write.edit.insert_text is not available: the document is read-only"
    );
    exec(
        &mut s,
        ro.doc,
        WriteCommand::MoveCaret {
            motion: CaretMotion::DocumentEnd,
            extend: false,
        }
        .into(),
    )
    .unwrap();
    // Session commands need no document.
    match s.execute(None, SuiteCommand::Ping.into()).unwrap().value {
        ExecValue::Pong(info) => {
            assert_eq!(info.version, kiln_common::VERSION);
            assert_eq!(info.protocol, kiln_protocol::PROTOCOL_VERSION);
        }
        other => panic!("{other:?}"),
    }
    let echo = s
        .execute(
            None,
            SuiteCommand::Echo {
                text: "héllo".into(),
            }
            .into(),
        )
        .unwrap();
    assert_eq!(echo.value, ExecValue::Echo("héllo".into()));
    assert_eq!(echo.outcome, Outcome::Unchanged);
}

#[test]
fn zoom_view_viewport_and_frames() {
    let mut s = Session::new(deferred());
    let doc = new_scratch(&mut s).doc;
    s.poll();
    let r = exec(&mut s, doc, SuiteCommand::SetZoom { zoom: 1.5 }.into()).unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    assert_eq!(
        exec(&mut s, doc, SuiteCommand::SetZoom { zoom: 1.5 }.into())
            .unwrap()
            .outcome,
        Outcome::Unchanged
    );
    for bad in [0.0, 5.5, f32::NAN] {
        assert_eq!(
            exec(&mut s, doc, SuiteCommand::SetZoom { zoom: bad }.into())
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
    }
    exec(
        &mut s,
        doc,
        SuiteCommand::SetViewMode {
            mode: ViewMode::Draft,
        }
        .into(),
    )
    .unwrap();
    match s.poll().last() {
        Some(Event::Changed { status, .. }) => {
            assert_eq!(status.zoom, 1.5);
            assert_eq!(status.view, ViewMode::Draft);
        }
        other => panic!("{other:?}"),
    }
    let vp = Viewport {
        width_px: 800.0,
        height_px: 600.0,
        dpr: 2.0,
        ..Viewport::default()
    };
    s.set_viewport(doc, vp).unwrap();
    assert!(s.set_viewport(doc, Viewport { zoom: -1.0, ..vp }).is_err());
    assert!(
        s.set_viewport(
            doc,
            Viewport {
                mode: ViewMode::SlideSorter,
                ..vp
            }
        )
        .is_err()
    );
    assert!(s.set_viewport(DocId(77), vp).is_err());
    assert_eq!(s.frame(doc).unwrap().revision, 0);
    exec(&mut s, doc, insert("a")).unwrap();
    assert_eq!(s.frame(doc).unwrap().revision, 1);
    let tree = s.a11y(doc).unwrap();
    assert_eq!(tree.nodes[1].text.as_deref(), Some("a"));
    assert!(matches!(s.query(Some(doc), &Query::A11y).unwrap(), QueryResult::A11y(t) if t == tree));
}

#[test]
fn save_and_open_go_through_deferred_host_requests() {
    let mut s = Session::new(deferred());
    let doc = new_scratch(&mut s).doc;
    exec(&mut s, doc, insert("Kiln")).unwrap();
    s.poll();

    // Save without a path asks for one.
    let r = exec(&mut s, doc, SuiteCommand::Save.into()).unwrap();
    let Outcome::Pending(pick) = r.outcome else {
        panic!("{r:?}")
    };
    let events = s.poll();
    match events.as_slice() {
        [Event::HostRequest(req)] => {
            assert_eq!(req.id, pick);
            assert!(
                matches!(&req.op, HostOp::PickSave { suggested_name, .. } if suggested_name == "Scratch1.txt")
            );
        }
        other => panic!("{other:?}"),
    }
    s.host_reply(pick, HostReply::Path("/docs/Kiln notes.txt".into()))
        .unwrap();
    let write = match s.poll().as_slice() {
        [Event::HostRequest(req)] => {
            assert_eq!(
                req.op,
                HostOp::WriteFile {
                    path: "/docs/Kiln notes.txt".into(),
                    bytes: b"Kiln".to_vec()
                }
            );
            req.id
        }
        other => panic!("{other:?}"),
    };
    // Typing while the write is in flight stays dirty after it completes.
    exec(&mut s, doc, insert("!")).unwrap();
    s.host_reply(write, HostReply::Done).unwrap();
    let saved = match s.poll().last() {
        Some(Event::Saved { summary }) => summary.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(saved.title, "Kiln notes");
    assert_eq!(saved.path.as_deref(), Some("/docs/Kiln notes.txt"));
    assert!(saved.dirty, "the edit made during the write is not saved");
    exec(&mut s, doc, SuiteCommand::Undo.into()).unwrap();
    assert!(
        !s.summary(doc).unwrap().dirty,
        "undoing to the saved state is clean"
    );
    assert!(
        s.host_reply(write, HostReply::Done).is_err(),
        "a reply is consumed once"
    );

    // Save again writes straight to the path; a host failure becomes an Error event.
    exec(&mut s, doc, insert("?")).unwrap();
    s.poll();
    let r = exec(&mut s, doc, SuiteCommand::Save.into()).unwrap();
    let Outcome::Pending(id) = r.outcome else {
        panic!()
    };
    s.host_reply(
        id,
        HostReply::Failed(KilnError::Io("disk full".into()).into()),
    )
    .unwrap();
    match s.poll().as_slice() {
        [
            _,
            Event::Error {
                doc: Some(d),
                error,
            },
        ] => {
            assert_eq!(*d, doc);
            assert_eq!(error.kind, ErrorKind::Io);
        }
        other => panic!("{other:?}"),
    }
    assert!(s.summary(doc).unwrap().dirty);

    // Open with a dialog: pick → read → opened; cancelling does nothing.
    let r = s
        .execute(None, SuiteCommand::Open { path: None }.into())
        .unwrap();
    let Outcome::Pending(cancel) = r.outcome else {
        panic!()
    };
    s.poll();
    s.host_reply(cancel, HostReply::Cancelled).unwrap();
    assert!(s.poll().is_empty());
    let r = s
        .execute(None, SuiteCommand::Open { path: None }.into())
        .unwrap();
    let Outcome::Pending(pick) = r.outcome else {
        panic!()
    };
    s.poll();
    s.host_reply(pick, HostReply::Path("/x/Todo.md".into()))
        .unwrap();
    let read = match s.poll().as_slice() {
        [Event::HostRequest(r)] => r.id,
        other => panic!("{other:?}"),
    };
    s.host_reply(read, HostReply::Bytes(b"- milk\n- bread".to_vec()))
        .unwrap();
    match s.poll().as_slice() {
        [Event::Opened { summary }] => {
            assert_eq!(summary.title, "Todo");
            assert_eq!(summary.format, Some(FileFormat::Md));
            assert_eq!(paragraphs(&s, summary.doc), vec!["- milk", "- bread"]);
        }
        other => panic!("{other:?}"),
    }
    // A reply of the wrong shape is reported, not trusted.
    let r = s
        .execute(
            None,
            SuiteCommand::Open {
                path: Some("/y.txt".into()),
            }
            .into(),
        )
        .unwrap();
    let Outcome::Pending(id) = r.outcome else {
        panic!()
    };
    s.poll();
    s.host_reply(id, HostReply::Done).unwrap();
    assert!(
        matches!(s.poll().as_slice(), [Event::Error { error, .. }] if error.kind == ErrorKind::InvalidInput)
    );
}

#[test]
fn save_as_rejects_formats_the_model_cannot_write_and_close_drops_the_document() {
    let mut s = Session::new(deferred());
    let doc = new_scratch(&mut s).doc;
    let err = exec(
        &mut s,
        doc,
        SuiteCommand::SaveAs {
            path: None,
            format: Some(FileFormat::Docx),
        }
        .into(),
    );
    assert_eq!(err.unwrap_err().kind(), ErrorKind::Unsupported);
    s.poll();
    let r = exec(
        &mut s,
        doc,
        SuiteCommand::SaveAs {
            path: None,
            format: None,
        }
        .into(),
    )
    .unwrap();
    let Outcome::Pending(pending) = r.outcome else {
        panic!()
    };
    exec(&mut s, doc, SuiteCommand::Close.into()).unwrap();
    assert!(matches!(s.poll().last(), Some(Event::Closed { doc: d }) if *d == doc));
    assert!(
        s.host_reply(pending, HostReply::Path("/a.txt".into()))
            .is_err(),
        "closing cancels its requests"
    );
    assert_eq!(s.summary(doc).unwrap_err().kind(), ErrorKind::NotFound);
}

#[test]
fn native_host_saves_and_opens_real_files() {
    let dir = std::env::temp_dir().join(format!("kiln-session-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hello.md").to_string_lossy().into_owned();

    let mut s = Session::new(Box::new(NativeHost));
    let doc = new_scratch(&mut s).doc;
    exec(&mut s, doc, insert("# Title\nBody")).unwrap();
    let r = exec(
        &mut s,
        doc,
        SuiteCommand::SaveAs {
            path: Some(path.clone()),
            format: None,
        }
        .into(),
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    let summary = r.summary.unwrap();
    assert!(!summary.dirty);
    assert_eq!(summary.format, Some(FileFormat::Md));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Title\nBody");

    s.poll();
    let r = s
        .execute(
            None,
            SuiteCommand::Open {
                path: Some(path.clone()),
            }
            .into(),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Applied);
    let opened = match s.poll().as_slice() {
        [Event::Opened { summary }] => summary.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(opened.path.as_deref(), Some(path.as_str()));
    assert_eq!(paragraphs(&s, opened.doc), vec!["# Title", "Body"]);

    s.execute(
        None,
        SuiteCommand::Open {
            path: Some(dir.join("missing.txt").to_string_lossy().into_owned()),
        }
        .into(),
    )
    .unwrap();
    assert!(
        matches!(s.poll().as_slice(), [Event::Error { error, .. }] if error.kind == ErrorKind::NotFound)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn session_thread_serves_calls_and_streams_events() {
    let (handle, thread) = SessionHandle::spawn(deferred()).unwrap();
    let events = handle.subscribe(None).unwrap();
    let summary = handle
        .call(|s| s.new_document(DocKind::Scratch))
        .unwrap()
        .unwrap();
    let doc = summary.doc;
    let other = handle.clone();
    let worker = std::thread::spawn(move || {
        for c in ["a", "b", "c"] {
            other
                .call(move |s| s.execute(Some(doc), insert(c)))
                .unwrap()
                .unwrap();
        }
    });
    worker.join().unwrap();
    let text = handle
        .call(move |s| s.query(Some(doc), &Query::Paragraphs))
        .unwrap()
        .unwrap();
    assert!(matches!(text, QueryResult::Paragraphs(p) if p[0].text == "abc"));

    let received: Vec<Event> = events.try_iter().collect();
    assert!(matches!(received.first(), Some(Event::Opened { .. })));
    assert_eq!(
        received
            .iter()
            .filter(|e| matches!(e, Event::Changed { .. }))
            .count(),
        3
    );

    // A panicking job fails alone; the session keeps working.
    let crashed = handle.call(|_| -> u32 { panic!("boom") });
    assert_eq!(crashed.unwrap_err().kind(), ErrorKind::Internal);
    // The error event follows the failed reply (the reply channel closes while unwinding).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut reported = false;
    while let Ok(e) = events.recv_deadline(deadline) {
        if matches!(&e, Event::Error { error, .. } if error.message.contains("boom")) {
            reported = true;
            break;
        }
    }
    assert!(reported, "the panic is reported as an Error event");
    assert!(handle.call(move |s| s.summary(doc)).unwrap().is_ok());

    handle.shutdown();
    thread.join().unwrap();
}

#[test]
fn session_handle_async_api_works_with_any_executor() {
    let (handle, _thread) = SessionHandle::spawn(deferred()).unwrap();
    // A minimal executor: the futures resolve once the session thread replies.
    let summary = block_on(handle.new_document(DocKind::Scratch)).unwrap();
    let doc = summary.doc;
    block_on(handle.execute(Some(doc), insert("hi"))).unwrap();
    let delta = block_on(handle.frame(doc)).unwrap();
    assert_eq!(delta.revision, 1);
    block_on(handle.set_viewport(doc, Viewport::default())).unwrap();
    assert_eq!(block_on(handle.a11y(doc)).unwrap().nodes.len(), 2);
    let q = block_on(handle.query(Some(doc), Query::Summary)).unwrap();
    assert!(matches!(q, QueryResult::Summary(s) if s.revision == 1));
    let opened = block_on(handle.open(
        b"x".to_vec(),
        OpenHint {
            name: Some("x.txt".into()),
            ..OpenHint::default()
        },
    ))
    .unwrap();
    assert_eq!(opened.title, "x");
    let err =
        block_on(handle.host_reply(kiln_protocol::RequestId(5), HostReply::Done)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

fn recv(events: &flume::Receiver<Event>) -> Event {
    events
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("an event within 5 s")
}

#[test]
fn a_late_subscriber_receives_the_host_requests_nobody_has_seen() {
    let (handle, _thread) = SessionHandle::spawn(deferred()).unwrap();
    // The UI has not subscribed yet (a webview still loading) when the core asks the host.
    handle
        .call(|s| s.execute(None, SuiteCommand::Open { path: None }.into()))
        .unwrap()
        .unwrap();
    let events = handle.subscribe(None).unwrap();
    assert!(matches!(
        recv(&events),
        Event::HostRequest(r) if matches!(r.op, HostOp::PickOpen { .. })
    ));
    // Delivered once: a second subscriber does not see it again.
    let second = handle.subscribe(None).unwrap();
    handle.call(|_| ()).unwrap();
    assert!(second.try_recv().is_err());
}

#[test]
fn a_document_subscriber_sees_only_its_document_and_session_events() {
    let (handle, _thread) = SessionHandle::spawn(deferred()).unwrap();
    let a = handle
        .call(|s| s.new_document(DocKind::Scratch))
        .unwrap()
        .unwrap()
        .doc;
    let b = handle
        .call(|s| s.new_document(DocKind::Scratch))
        .unwrap()
        .unwrap()
        .doc;
    let all = handle.subscribe(None).unwrap();
    let only_a = handle.subscribe(Some(a)).unwrap();
    for doc in [b, a] {
        handle
            .call(move |s| s.execute(Some(doc), insert("x")))
            .unwrap()
            .unwrap();
    }
    handle
        .call(|s| s.execute(None, SuiteCommand::Open { path: None }.into()))
        .unwrap()
        .unwrap();
    // Events are published after a job's reply; the next job starts after they are out.
    handle.call(|_| ()).unwrap();
    let mine: Vec<Event> = only_a.try_iter().collect();
    assert_eq!(mine.len(), 2, "{mine:?}");
    assert!(matches!(&mine[0], Event::Changed { summary, .. } if summary.doc == a));
    assert!(matches!(&mine[1], Event::HostRequest(_)));
    // `all` also got the two `Opened` events emitted before anyone subscribed.
    let everything: Vec<Event> = all.try_iter().collect();
    assert_eq!(everything.len(), 5, "{everything:?}");
    assert!(matches!(&everything[0], Event::Opened { summary } if summary.doc == a));
    assert!(matches!(&everything[1], Event::Opened { summary } if summary.doc == b));

    // A dropped subscriber is removed; the others keep receiving.
    drop(only_a);
    handle
        .call(move |s| s.execute(Some(a), insert("y")))
        .unwrap()
        .unwrap();
    assert!(matches!(recv(&all), Event::Changed { summary, .. } if summary.doc == a));
}

#[test]
fn subscribing_again_as_the_same_owner_replaces_the_previous_stream() {
    let (handle, _thread) = SessionHandle::spawn(deferred()).unwrap();
    let first = handle.subscribe_as("webview:main", None).unwrap();
    let doc = handle
        .call(|s| s.new_document(DocKind::Scratch))
        .unwrap()
        .unwrap()
        .doc;
    // The webview reloads and subscribes again; its old stream ends after what it already got.
    let second = handle.subscribe_as("webview:main", None).unwrap();
    let other_doc_stream = handle.subscribe_as("webview:main", Some(doc)).unwrap();
    handle
        .call(move |s| s.execute(Some(doc), insert("x")))
        .unwrap()
        .unwrap();
    assert!(matches!(recv(&first), Event::Opened { .. }));
    assert!(matches!(
        first.recv_timeout(std::time::Duration::from_secs(5)),
        Err(flume::RecvTimeoutError::Disconnected)
    ));
    assert!(matches!(recv(&second), Event::Changed { .. }));
    assert!(matches!(recv(&other_doc_stream), Event::Changed { .. }));
    assert!(second.try_recv().is_err());
}

/// A scratch app whose model panics after applying `InsertText { "boom" }`, half-way through a
/// change, as a bug in an app model would.
#[derive(Debug)]
struct PanickyApp;
#[derive(Debug)]
struct Panicky(Box<dyn DocModel>);

impl DocModel for Panicky {
    fn kind(&self) -> DocKind {
        self.0.kind()
    }
    fn execute(&mut self, command: &Command) -> Result<Change> {
        let change = self.0.execute(command)?;
        if *command == insert("boom") {
            panic!("model bug");
        }
        Ok(change)
    }
    fn undo(&mut self) -> bool {
        self.0.undo()
    }
    fn redo(&mut self) -> bool {
        self.0.redo()
    }
    fn can_undo(&self) -> bool {
        self.0.can_undo()
    }
    fn can_redo(&self) -> bool {
        self.0.can_redo()
    }
    fn undo_label(&self) -> Option<&'static str> {
        self.0.undo_label()
    }
    fn redo_label(&self) -> Option<&'static str> {
        self.0.redo_label()
    }
    fn state(&self) -> StateId {
        self.0.state()
    }
    fn seal_history(&mut self) {
        self.0.seal_history();
    }
    fn selection(&self) -> Selection {
        self.0.selection()
    }
    fn status(&self) -> StatusBar {
        self.0.status()
    }
    fn paragraphs(&self) -> Option<Vec<ParagraphText>> {
        self.0.paragraphs()
    }
    fn a11y(&self, title: &str) -> A11yTree {
        self.0.a11y(title)
    }
    fn save_formats(&self) -> &'static [FileFormat] {
        self.0.save_formats()
    }
    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        self.0.save(format)
    }
}

impl AppFactory for PanickyApp {
    fn kind(&self) -> DocKind {
        DocKind::Scratch
    }
    fn opens(&self, f: FileFormat) -> bool {
        kiln_session::apps::scratch::ScratchApp.opens(f)
    }
    fn create(&self) -> Box<dyn DocModel> {
        Box::new(Panicky(kiln_session::apps::scratch::ScratchApp.create()))
    }
    fn open(&self, bytes: &[u8], f: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>> {
        Ok(Box::new(Panicky(
            kiln_session::apps::scratch::ScratchApp.open(bytes, f, hint)?,
        )))
    }
}

#[test]
fn a_panic_during_an_edit_leaves_the_document_damaged_and_read_only() {
    let (handle, _thread) = SessionHandle::spawn_with(|| {
        let mut s = Session::new(deferred());
        s.register_app(Box::new(PanickyApp));
        s
    })
    .unwrap();
    let events = handle.subscribe(None).unwrap();
    let doc = handle
        .call(|s| {
            s.open(
                b"Original",
                OpenHint {
                    name: Some("a.txt".into()),
                    path: Some("/nowhere/a.txt".into()),
                    format: Some(FileFormat::Txt),
                    read_only: false,
                },
            )
        })
        .unwrap()
        .unwrap()
        .doc;
    assert!(matches!(recv(&events), Event::Opened { .. }));

    let crashed = handle.call(move |s| s.execute(Some(doc), insert("boom")));
    assert_eq!(crashed.unwrap_err().kind(), ErrorKind::Internal);
    assert!(matches!(
        recv(&events),
        Event::Error { doc: Some(d), error } if d == doc && error.message.contains("model bug")
    ));
    assert!(matches!(
        recv(&events),
        Event::Changed { summary, .. } if summary.read_only
    ));

    // No further edits (nor undo), but it still answers queries and can be saved elsewhere.
    for cmd in [insert("more"), SuiteCommand::Undo.into()] {
        let err = handle
            .call(move |s| s.execute(Some(doc), cmd))
            .unwrap()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unavailable, "{err}");
        assert!(err.to_string().contains("save a copy"), "{err}");
    }
    let summary = handle.call(move |s| s.summary(doc)).unwrap().unwrap();
    assert!(summary.read_only);
    handle
        .call(move |s| s.execute(Some(doc), SuiteCommand::Save.into()))
        .unwrap()
        .unwrap();
    assert!(
        matches!(recv(&events), Event::HostRequest(r) if matches!(r.op, HostOp::PickSave { .. })),
        "Save asks for a new location instead of overwriting the original"
    );
    // Other documents are unaffected.
    let other = handle
        .call(|s| s.new_document(DocKind::Scratch))
        .unwrap()
        .unwrap()
        .doc;
    handle
        .call(move |s| s.execute(Some(other), insert("fine")))
        .unwrap()
        .unwrap();
}
