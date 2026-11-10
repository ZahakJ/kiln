// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid's stamp guard in a build without test code (run it with `--release`): review 5's
//! bypasses of the ledger, B3 to B6, planted at run time on Sheet Tab Color
//! (`kiln_session::grid_plants`, the `ownership-escape` feature). The document is kept as it
//! is, the change goes in the autosave journal by the command's name and a checkpoint follows
//! at once, so a recovery gives what was on screen.

use std::path::PathBuf;

use kiln_protocol::commands::{GridCommand as G, GridQuery, GridQueryResult};
use kiln_protocol::{Command, DocKind, Event, HostOp, HostReply, Query, QueryResult, SuiteCommand};
use std::sync::{Arc, Mutex};

use kiln_session::{Deliver, Host, NativeHost, Session, StoredHost};

/// App storage in a directory, with what the journal appended kept aside.
struct Recording {
    inner: StoredHost,
    appended: Arc<Mutex<Vec<u8>>>,
}

impl Host for Recording {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        if let HostOp::StoreAppend { bytes, .. } = op {
            self.appended
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
        }
        self.inner.request(op)
    }
    fn request_later(&mut self, id: kiln_protocol::RequestId, op: &HostOp) -> bool {
        self.inner.request_later(id, op)
    }
    fn attach(&mut self, deliver: Deliver) {
        self.inner.attach(deliver);
    }
    fn has_storage(&self) -> bool {
        self.inner.has_storage()
    }
    fn now_ms(&self) -> u64 {
        self.inner.now_ms()
    }
}

fn session(dir: &std::path::Path) -> (Session, Arc<Mutex<Vec<u8>>>) {
    let appended = Arc::default();
    let host = Recording {
        inner: StoredHost::open(Box::new(NativeHost), dir).expect("storage"),
        appended: Arc::clone(&appended),
    };
    (Session::new(Box::new(host)), appended)
}

/// Answers timers until the session asks for nothing more; gives the other events.
fn settle(s: &mut Session) -> Vec<Event> {
    let mut out = Vec::new();
    for _ in 0..1000 {
        let events = s.poll();
        if events.is_empty() {
            return out;
        }
        for e in events {
            match e {
                Event::HostRequest(r) => match r.op {
                    HostOp::Timer { .. }
                    | HostOp::ClipboardWrite { .. }
                    | HostOp::WatchFile { .. }
                    | HostOp::UnwatchFile { .. } => {
                        let _ = s.host_reply(r.id, HostReply::Done);
                    }
                    other => panic!("unexpected request {other:?}"),
                },
                other => out.push(other),
            }
        }
    }
    panic!("the session never settled");
}

fn run(s: &mut Session, doc: kiln_common::DocId, cmd: impl Into<Command>) {
    s.execute(Some(doc), cmd.into()).expect("command");
    settle(s);
}

#[test]
fn each_bypass_of_the_ledger_is_kept_with_a_named_stop_and_a_checkpoint() {
    assert!(
        !cfg!(debug_assertions),
        "run with --release: this is the release build's guard"
    );
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ownership_runtime");
    // 0: no bypass, the plain command (a record of its own, which names nothing).
    for n in [0, 3, 4, 5, 6u8] {
        let dir = root.join(format!("b{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let (mut s, appended) = session(&dir);
        let doc = s.new_document(DocKind::Grid).expect("new").doc;
        settle(&mut s);
        run(
            &mut s,
            doc,
            G::BeginEdit {
                text: "kept".into(),
            },
        );
        run(&mut s, doc, G::CommitEdit { advance: None });
        appended
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        kiln_session::grid_plants::set((n > 0).then_some(n));
        run(
            &mut s,
            doc,
            G::SheetTabColor {
                index: 0,
                color: Some("#C0504D".into()),
            },
        );
        kiln_session::grid_plants::set(None);
        let shown = on_screen(&mut s, doc);
        // The journal names the change: a record of its own, by the command.
        let journal = appended
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let named = journal
            .windows(b"grid.sheet.tab_color".len())
            .any(|w| w == b"grid.sheet.tab_color");
        drop(s);

        let (mut s, _) = session(&dir);
        let events = settle_with(&mut s, SuiteCommand::RecoveryScan);
        let entries = events
            .iter()
            .find_map(|e| match e {
                Event::Recovery { entries } => Some(entries.clone()),
                _ => None,
            })
            .expect("a recovery list");
        assert_eq!(entries.len(), 1, "B{n}: {entries:?}");
        let events = settle_with(
            &mut s,
            SuiteCommand::RecoveryOpen {
                key: entries[0].key.clone(),
                kind: entries[0].kind,
            },
        );
        let id = events
            .iter()
            .find_map(|e| match e {
                Event::Opened { summary } => Some(summary.doc),
                _ => None,
            })
            .expect("recovered");
        let back = on_screen(&mut s, id);
        println!(
            "RELEASE B{n}: the journal names grid.sheet.tab_color: {named}; on screen {shown:?}; \
             recovered {back:?}"
        );
        assert_eq!(named, n > 0, "B{n}: the stop is named");
        assert_eq!(back, shown, "B{n}");
        if n != 5 {
            assert_eq!(shown.1.as_deref(), Some("C0504D"), "B{n}");
        }
    }
}

fn settle_with(s: &mut Session, cmd: SuiteCommand) -> Vec<Event> {
    let _ = s.execute(None, cmd.into());
    settle(s)
}

/// What the window shows: A1's content and the first sheet's tab colour.
fn on_screen(s: &mut Session, doc: kiln_common::DocId) -> (String, Option<String>) {
    s.execute(
        Some(doc),
        G::SelectCell {
            row: 0,
            col: 0,
            extend: false,
            add: false,
        }
        .into(),
    )
    .expect("select");
    settle(s);
    match s
        .query(Some(doc), &Query::Grid(GridQuery::State))
        .expect("state")
    {
        QueryResult::Grid(GridQueryResult::State(st)) => (st.formula, st.sheets[0].color.clone()),
        other => panic!("{other:?}"),
    }
}
