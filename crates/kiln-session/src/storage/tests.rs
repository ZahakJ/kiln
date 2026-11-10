// SPDX-License-Identifier: AGPL-3.0-or-later
//! Storage in the session: crash recovery of Write, Grid and Deck documents from their
//! journals (also with a torn last record and from an older journal), clean closes, AutoSave to
//! file and its refusals, native bundles, local Version History and the recent files.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use kiln_common::DocId;
use kiln_protocol::collab::{VersionChange, VersionOrigin, VersionSlideMark};
use kiln_protocol::commands::{DeckCommand, GridCommand, GridCommand as G, GridDirection};
use kiln_protocol::{
    AutoSave, Command, DocKind, Event, FileFormat, HostOp, HostReply, OpenHint, Query, QueryResult,
    RecoveryEntry, StoreArea, SuiteCommand, WriteCommand,
};
use kiln_storage::journal;
use kiln_storage::store::{MemoryStorage, StorageHost, answer};

use crate::{Host, Session};

#[path = "replay_tests.rs"]
mod replay;

#[path = "crash_tests.rs"]
mod crash;

/// What survives a crash: the app storage and the files.
#[derive(Default)]
struct Disk {
    store: MemoryStorage,
    files: BTreeMap<String, Vec<u8>>,
    /// Each file's modification time (a counter: every write moves it).
    modified: BTreeMap<String, u64>,
    clock: u64,
    /// The next appends to the app storage fail (a full disk).
    fail_appends: usize,
    /// Checkpoints wait for the test to answer them (a large write under way).
    hold_checkpoints: bool,
    /// The writes still to reach the disk before the power goes: past it, writes are answered
    /// and lost (`None`: no end).
    budget: Option<usize>,
}

impl Disk {
    /// A file written (by Kiln, or by another program in a test).
    fn write(&mut self, path: &str, bytes: Vec<u8>) {
        self.clock += 1;
        self.modified.insert(path.to_owned(), self.clock);
        self.files.insert(path.to_owned(), bytes);
    }

    fn stamp(&self, path: &str) -> HostReply {
        match self.files.get(path) {
            Some(b) => HostReply::Stamp(kiln_protocol::FileStamp {
                size: b.len() as u64,
                modified_ms: self.modified.get(path).copied().unwrap_or(0),
            }),
            None => HostReply::Empty,
        }
    }
}

/// A host with app storage on a shared [`Disk`]; timers are answered by [`settle`].
struct TestHost {
    disk: Arc<Mutex<Disk>>,
    clock: Arc<AtomicU64>,
    /// File writes wait for the test to answer them (a slow disk, the web's async writes).
    defer_writes: Arc<std::sync::atomic::AtomicBool>,
    /// So do the stamps of files.
    defer_stats: Arc<std::sync::atomic::AtomicBool>,
}

fn lock(d: &Mutex<Disk>) -> std::sync::MutexGuard<'_, Disk> {
    d.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Host for TestHost {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        let mut disk = lock(&self.disk);
        if let HostOp::StoreAppend { .. } = op
            && disk.fail_appends > 0
        {
            disk.fail_appends -= 1;
            return Some(HostReply::Failed(
                (&kiln_common::KilnError::Io("the disk is full".into())).into(),
            ));
        }
        if disk.hold_checkpoints
            && matches!(op, HostOp::StorePut { key, .. } if key.contains(".base."))
        {
            return None;
        }
        if matches!(
            op,
            HostOp::StorePut { .. } | HostOp::StoreAppend { .. } | HostOp::StoreDelete { .. }
        ) && let Some(left) = disk.budget.as_mut()
        {
            if *left == 0 {
                return Some(HostReply::Done);
            }
            *left -= 1;
        }
        if let Some(r) = answer(&mut disk.store, op) {
            return Some(r);
        }
        match op {
            HostOp::WriteFile { .. } if self.defer_writes.load(Ordering::Relaxed) => None,
            HostOp::WriteFile { path, bytes } => {
                disk.write(path, bytes.clone());
                Some(HostReply::Done)
            }
            HostOp::StatFile { .. } if self.defer_stats.load(Ordering::Relaxed) => None,
            HostOp::StatFile { path } => Some(disk.stamp(path)),
            HostOp::ReadFile { path } => Some(disk.files.get(path).map_or_else(
                || HostReply::Failed((&kiln_common::KilnError::not_found(path)).into()),
                |b| HostReply::Bytes(b.clone()),
            )),
            _ => None,
        }
    }

    fn has_storage(&self) -> bool {
        true
    }

    fn now_ms(&self) -> u64 {
        self.clock.load(Ordering::Relaxed)
    }
}

/// One run of Kiln over a disk.
struct Run {
    s: Session,
    clock: Arc<AtomicU64>,
    defer_writes: Arc<std::sync::atomic::AtomicBool>,
    defer_stats: Arc<std::sync::atomic::AtomicBool>,
    /// The file watches asked for (answered by the test: another program changed the file).
    watches: Vec<(kiln_protocol::RequestId, String)>,
    /// The disk.
    disk: Option<Arc<Mutex<Disk>>>,
    /// The checkpoints under way (`Disk::hold_checkpoints`).
    held: Vec<kiln_protocol::HostRequest>,
}

impl Run {
    fn new(disk: &Arc<Mutex<Disk>>) -> Self {
        Self::at(disk, 1_792_000_000_000)
    }

    /// A run whose clock starts at `now_ms`.
    fn at(disk: &Arc<Mutex<Disk>>, now_ms: u64) -> Self {
        let clock = Arc::new(AtomicU64::new(now_ms));
        let defer_writes = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let defer_stats = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = Session::new(Box::new(TestHost {
            disk: Arc::clone(disk),
            clock: Arc::clone(&clock),
            defer_writes: Arc::clone(&defer_writes),
            defer_stats: Arc::clone(&defer_stats),
        }));
        Self {
            s,
            clock,
            defer_writes,
            defer_stats,
            watches: Vec::new(),
            disk: Some(Arc::clone(disk)),
            held: Vec::new(),
        }
    }

    fn later(&self, ms: u64) {
        self.clock.fetch_add(ms, Ordering::Relaxed);
    }

    /// Answers the timers (time passes) until none is asked; returns the other events.
    fn settle(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        for _ in 0..1000 {
            let events = self.s.poll();
            if events.is_empty() {
                return out;
            }
            for e in events {
                match e {
                    Event::HostRequest(r) => match r.op {
                        HostOp::Timer { ms } => {
                            self.later(u64::from(ms));
                            let _ = self.s.host_reply(r.id, HostReply::Done);
                        }
                        HostOp::WatchFile { path } => self.watches.push((r.id, path)),
                        HostOp::UnwatchFile { .. } => {}
                        HostOp::StorePut { .. } => self.held.push(r),
                        // The system clipboard is not this test's: the core keeps its own copy.
                        HostOp::ClipboardWrite { .. } | HostOp::ClipboardRead => {
                            let _ = self.s.host_reply(r.id, HostReply::Done);
                        }
                        other => panic!("unexpected request {other:?}"),
                    },
                    other => out.push(other),
                }
            }
        }
        panic!("the session never settled");
    }

    fn run(&mut self, doc: DocId, cmd: Command) {
        self.s
            .execute(Some(doc), cmd)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    fn suite(&mut self, doc: Option<DocId>, cmd: SuiteCommand) -> Vec<Event> {
        self.s
            .execute(doc, Command::Suite(cmd))
            .unwrap_or_else(|e| panic!("{e}"));
        self.settle()
    }

    fn type_text(&mut self, doc: DocId, text: &str) {
        self.run(
            doc,
            WriteCommand::InsertText {
                text: text.to_owned(),
            }
            .into(),
        );
    }

    fn text(&self, doc: DocId) -> Vec<String> {
        match self.s.query(Some(doc), &Query::Paragraphs) {
            Ok(QueryResult::Paragraphs(p)) => p.into_iter().map(|p| p.text).collect(),
            other => panic!("{other:?}"),
        }
    }

    fn recovery(&mut self) -> Vec<RecoveryEntry> {
        let events = self.suite(None, SuiteCommand::RecoveryScan);
        events
            .into_iter()
            .rev()
            .find_map(|e| match e {
                Event::Recovery { entries } => Some(entries),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no Event::Recovery"))
    }

    /// Opens the only recoverable document; returns it.
    fn recover_only(&mut self) -> DocId {
        let entries = self.recovery();
        assert_eq!(entries.len(), 1, "{entries:?}");
        let (key, kind) = (entries[0].key.clone(), entries[0].kind);
        let events = self.suite(None, SuiteCommand::RecoveryOpen { key, kind });
        let id = events
            .iter()
            .find_map(|e| match e {
                Event::Opened { summary } => Some(summary.doc),
                _ => None,
            })
            .unwrap_or_else(|| panic!("nothing opened: {events:?}"));
        let errors: Vec<&Event> = events
            .iter()
            .filter(|e| matches!(e, Event::Error { .. }))
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        id
    }

    fn summary(&self, doc: DocId) -> kiln_protocol::DocSummary {
        self.s.summary(doc).unwrap_or_else(|e| panic!("{e}"))
    }

    fn open_file(&mut self, disk: &Arc<Mutex<Disk>>, path: &str) -> DocId {
        let bytes = lock(disk).files[path].clone();
        let id = self
            .s
            .open(
                &bytes,
                OpenHint {
                    path: Some(path.to_owned()),
                    ..OpenHint::default()
                },
            )
            .unwrap_or_else(|e| panic!("{e}"))
            .doc;
        self.settle();
        id
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(kiln_testing::fixtures::fixtures_dir().join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The body text of a Word file.
fn docx_text(bytes: &[u8]) -> String {
    let read = kiln_io_docx::read_docx(bytes).unwrap();
    read.document
        .body_paragraphs()
        .iter()
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n")
}

fn disk() -> Arc<Mutex<Disk>> {
    Arc::default()
}

#[test]
fn a_write_document_is_recovered_after_a_crash_past_several_checkpoints() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.settle();
    // More than two checkpoints' worth of operations, with Undo and Redo among them.
    for i in 0..230 {
        a.type_text(doc, &format!("w{i} "));
        if i % 50 == 49 {
            a.type_text(doc, "\n");
        }
    }
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    a.type_text(doc, "the end");
    a.settle();
    let before = a.text(doc);
    // The crash: no close, the session is gone.
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    assert_eq!(entries[0].kind, DocKind::Write);
    assert_eq!(entries[0].title, "Document1");
    assert!(entries[0].problem.is_none());
    let id = b.recover_only();
    assert_eq!(b.text(id), before);
    let s = b.summary(id);
    assert!(s.dirty, "a recovered document counts as changed");
    // It keeps its journal: listed no more while it is open, and recovered again after a
    // second crash with the edits made since.
    assert!(b.recovery().is_empty());
    b.type_text(id, " again");
    let again = b.text(id);
    drop(b);
    let mut c = Run::new(&d);
    let id = c.recover_only();
    assert_eq!(c.text(id), again);
}

#[test]
fn a_workbook_is_recovered_after_a_crash() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for (i, v) in ["Glaze", "12", "=B1*2", "cone 10"].iter().enumerate() {
        a.run(
            doc,
            GridCommand::BeginEdit {
                text: (*v).to_owned(),
            }
            .into(),
        );
        a.run(
            doc,
            GridCommand::CommitEdit {
                advance: Some(if i % 2 == 0 {
                    GridDirection::Right
                } else {
                    GridDirection::Down
                }),
            }
            .into(),
        );
    }
    let before = a.s.docs[&doc].model.save(FileFormat::Csv).unwrap();
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.s.docs[&id].model.save(FileFormat::Csv).unwrap(), before);
    assert!(String::from_utf8_lossy(&before).contains("Glaze"));
    assert!(b.summary(id).dirty);
}

#[test]
fn the_first_change_to_a_workbook_is_kept_at_once() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.run(
        doc,
        GridCommand::BeginEdit {
            text: "first".into(),
        }
        .into(),
    );
    a.run(doc, GridCommand::CommitEdit { advance: None }.into());
    // Crash before any timer ran.
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    let csv = b.s.docs[&id].model.save(FileFormat::Csv).unwrap();
    assert!(String::from_utf8_lossy(&csv).contains("first"));
}

/// Types `text` into the active cell and commits it, moving down.
fn enter_cell(r: &mut Run, doc: DocId, text: &str) {
    r.run(
        doc,
        GridCommand::BeginEdit {
            text: text.to_owned(),
        }
        .into(),
    );
    r.run(
        doc,
        GridCommand::CommitEdit {
            advance: Some(GridDirection::Down),
        }
        .into(),
    );
}

fn csv_of(r: &Run, doc: DocId) -> String {
    String::from_utf8_lossy(&r.s.docs[&doc].model.save(FileFormat::Csv).unwrap()).into_owned()
}

/// The page goes (or crashes) the moment after an edit: no timer ever runs.
#[test]
fn the_last_grid_edit_is_in_the_journal_before_any_timer_runs() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "1111");
    a.settle();
    enter_cell(&mut a, doc, "4242");
    // Crash here: the checkpoint timer of the second edit has not fired.
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert!(csv_of(&b, id).contains("4242"), "{}", csv_of(&b, id));
}

/// The workbook as the journal restores it, the selections aside (the view is no edit).
fn grid_model(s: &Session, doc: DocId) -> Vec<u8> {
    let b = s.docs[&doc].model.to_bundle().unwrap();
    let mut wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
    let ids: Vec<_> = wb.sheets().iter().map(kiln_sheet::Sheet::id).collect();
    for id in ids {
        let mut meta = wb.sheet(id).unwrap().meta().clone();
        meta.view.selection = Default::default();
        wb.load_sheet_meta(id, meta).unwrap();
    }
    let mut props = wb.props().clone();
    props.active_sheet = 0;
    wb.apply(&kiln_sheet::Command::SetProps {
        props: Box::new(props),
    })
    .unwrap();
    wb.to_bytes().unwrap()
}

#[test]
fn every_kind_of_grid_edit_replays_to_the_same_workbook() {
    use kiln_protocol::commands::EditCommand as E;
    let grid = |c: GridCommand| Command::Grid(c);
    let edit = |e: E| Command::Suite(SuiteCommand::Edit(e));
    let select = |row, col| {
        grid(GridCommand::SelectCell {
            row,
            col,
            extend: false,
            add: false,
        })
    };
    let range = |r0, c0, r1, c1| {
        grid(GridCommand::SelectRange {
            range: kiln_protocol::commands::GridRange { r0, c0, r1, c1 },
            add: false,
        })
    };
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for v in [
        "30",
        "10",
        "=A1+A2",
        "a much longer entry that widens its column",
    ] {
        enter_cell(&mut a, doc, v);
    }
    a.settle();
    let key = only_journal(&d);
    let generation = marker(&d, &key).generation;
    // Not one timer from here on: the crash comes with the last edit.
    a.run(doc, select(0, 0));
    a.run(doc, edit(E::Bold));
    a.run(doc, grid(GridCommand::InsertRows));
    a.run(doc, grid(GridCommand::DeleteRows));
    a.run(doc, select(1, 0));
    a.run(doc, grid(GridCommand::SortAscending));
    a.run(doc, range(0, 1, 3, 2));
    a.run(doc, grid(GridCommand::MergeCells));
    a.run(doc, select(0, 3));
    enter_cell(&mut a, doc, "=SUM(A1:A3)");
    a.run(doc, select(0, 3));
    a.run(doc, range(0, 3, 2, 3));
    a.run(doc, grid(GridCommand::FillDown));
    a.run(doc, grid(GridCommand::InsertSheet));
    enter_cell(&mut a, doc, "second sheet");
    assert_eq!(
        marker(&d, &key).generation,
        generation,
        "records, not checkpoints"
    );
    // An undo and a redo write a checkpoint; what follows goes on from it.
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    enter_cell(&mut a, doc, "after the undo");
    let before = grid_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(grid_model(&b.s, id), before);
}

/// A session whose checkpoints all take the slow path, on a disk that holds them back.
fn slow_checkpoints(d: &Arc<Mutex<Disk>>) -> (Run, DocId) {
    let mut a = Run::new(d);
    a.s.storage.inline_bytes = Some(0);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "1");
    land_checkpoint(&mut a);
    a.settle();
    lock(d).hold_checkpoints = true;
    (a, doc)
}

/// The checkpoint under way reaches the disk.
fn land_checkpoint(a: &mut Run) {
    a.settle();
    for r in std::mem::take(&mut a.held) {
        let HostOp::StorePut { area, key, bytes } = &r.op else {
            continue;
        };
        let disk = a.disk.as_ref().expect("the run's disk");
        lock(disk).store.put(*area, key, bytes).unwrap();
        a.s.host_reply(r.id, HostReply::Done).unwrap();
    }
}

#[test]
fn an_edit_during_a_large_checkpoint_is_kept_at_every_stage_of_it() {
    // The crash comes with the checkpoint nowhere, then written, then the marker moved.
    for stage in 0..3 {
        let d = disk();
        let (mut a, doc) = slow_checkpoints(&d);
        for i in 0..200 {
            select_cell(&mut a, doc, 0, 1);
            enter_cell(&mut a, doc, &format!("e{i}"));
        }
        // The 200th edit is a record, with the checkpoint on top of it.
        assert!(csv_of(&a, doc).contains("e199"));
        select_cell(&mut a, doc, 0, 2);
        enter_cell(&mut a, doc, "after it started");
        a.run(doc, GridCommand::Theme { dark: false }.into());
        a.run(
            doc,
            GridCommand::BeginEdit {
                text: "typing".into(),
            }
            .into(),
        );
        if stage > 0 {
            // On the disk, the marker not moved.
            a.settle();
            let r = a.held.first().unwrap();
            let HostOp::StorePut { area, key, bytes } = &r.op else {
                panic!("{r:?}");
            };
            lock(&d).store.put(*area, key, bytes).unwrap();
        }
        if stage == 2 {
            lock(&d).hold_checkpoints = false;
            land_checkpoint(&mut a);
        }
        drop(a);
        let mut b = Run::new(&d);
        let (id, note) = recover_with_note(&mut b);
        let csv = csv_of(&b, id);
        assert!(
            csv.contains("e199") && csv.contains("after it started"),
            "{stage}: {csv}"
        );
        let draft = note.and_then(|n| n.draft).map(|d| d.text);
        assert_eq!(draft.as_deref(), Some("typing"), "stage {stage}");
    }
}

#[test]
fn undo_redo_and_a_new_format_are_records_not_checkpoints() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    let key = only_journal(&d);
    let generation = marker(&d, &key).generation;
    select_cell(&mut a, doc, 0, 1);
    enter_cell(&mut a, doc, "undone");
    select_cell(&mut a, doc, 0, 2);
    enter_cell(&mut a, doc, "kept");
    select_cell(&mut a, doc, 0, 3);
    enter_cell(&mut a, doc, "redone");
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    select_cell(&mut a, doc, 0, 1);
    a.run(
        doc,
        Command::Suite(SuiteCommand::Edit(
            kiln_protocol::commands::EditCommand::Bold,
        )),
    );
    a.run(doc, GridCommand::ClearContents.into());
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    let before = grid_model(&a.s, doc);
    assert_eq!(marker(&d, &key).generation, generation);
    assert!(
        lock(&d)
            .store
            .list(StoreArea::Journal, "")
            .unwrap()
            .iter()
            .all(|k| !k.ends_with(".unkept"))
    );
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(grid_model(&b.s, id), before);
}

#[test]
fn properties_are_a_record_too() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "1");
    a.settle();
    let key = only_journal(&d);
    let generation = marker(&d, &key).generation;
    a.run(
        doc,
        Command::Suite(SuiteCommand::SetProperties {
            props: Box::new(kiln_protocol::commands::write_features::PropertiesEdit {
                title: Some("Kept".into()),
                ..Default::default()
            }),
        }),
    );
    assert_eq!(marker(&d, &key).generation, generation);
    let before = a.s.docs[&doc].model.properties();
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.s.docs[&id].model.properties(), before);
}

#[test]
fn a_grid_journal_of_the_format_before_replays_its_bare_commands() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "1");
    a.settle();
    let key = only_journal(&d);
    let sheet = kiln_sheet::Workbook::from_bytes(
        &a.s.docs[&doc].model.to_bundle().unwrap().model,
        Default::default(),
    )
    .unwrap()
    .sheets()[0]
        .id();
    drop(a);
    let mut m = marker(&d, &key);
    m.format = 2;
    let bare = kiln_sheet::Command::SetCells {
        sheet,
        cells: vec![kiln_sheet::commands::CellInput::value(4, 4, 77.0)],
    };
    let record = journal::encode_record_before4(&journal::RecordBefore4 {
        generation: m.generation,
        time_ms: 1,
        ops: vec![bare.to_bytes().unwrap()],
    })
    .unwrap();
    {
        let mut disk = lock(&d);
        let log = journal::log_key(&key, m.generation);
        disk.store.put(StoreArea::Journal, &log, &record).unwrap();
        let meta = serde_json::to_vec(&m).unwrap();
        disk.store
            .put(StoreArea::Journal, &journal::meta_key(&key), &meta)
            .unwrap();
    }
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert!(csv_of(&b, id).contains("77"));
}

/// Changing how a workbook's operations encode strands every journal of the version before.
/// When this fails, raise `JOURNAL_FORMAT` and read the old format, then update the bytes.
#[test]
fn the_journal_encoding_of_a_workbook_is_frozen() {
    use crate::apps::grid::tests_support::samples;
    let hex: Vec<String> = samples()
        .iter()
        .map(|b| b.iter().map(|x| format!("{x:02x}")).collect())
        .collect();
    assert_eq!(hex, FROZEN);
}

const FROZEN: [&str; 10] = [
    "0000010101020101000000000000084000",
    "002700000000",
    "00260000",
    "0101000200",
    "0201",
    "0300",
    "0400",
    "0501700101",
    "06016d0102",
    "0803",
];

#[test]
fn the_sheet_on_show_comes_back() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "first");
    a.run(doc, G::InsertSheet.into());
    enter_cell(&mut a, doc, "second");
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    // The sheet on show is what a CSV is written from.
    assert!(csv_of(&b, id).contains("second"), "{}", csv_of(&b, id));
}

/// A one-pixel PNG.
const PIXEL: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0a, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0d, 0x0a, 0x2d, 0xb4, 0, 0, 0, 0, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn a_chart_and_a_picture_are_small_records_too() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    let key = only_journal(&d);
    let generation = marker(&d, &key).generation;
    select_cell(&mut a, doc, 0, 1);
    enter_cell(&mut a, doc, "2");
    enter_cell(&mut a, doc, "3");
    select_cell(&mut a, doc, 0, 1);
    a.run(
        doc,
        Command::Suite(SuiteCommand::ChartInsert { kind: None }),
    );
    a.run(
        doc,
        Command::Suite(SuiteCommand::ChartSetTitle {
            text: "Spend".into(),
        }),
    );
    select_cell(&mut a, doc, 9, 9);
    a.run(
        doc,
        G::InsertPicture {
            name: "pixel.png".into(),
            bytes: PIXEL.to_vec(),
        }
        .into(),
    );
    assert_eq!(marker(&d, &key).generation, generation);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    let bundle = b.s.docs[&id].model.to_bundle().unwrap();
    let charts: Vec<&Vec<u8>> = bundle
        .passthrough
        .iter()
        .filter(|(k, _)| k.starts_with("charts/"))
        .map(|(_, v)| v)
        .collect();
    assert!(
        charts
            .iter()
            .any(|x| String::from_utf8_lossy(x).contains("Spend")),
        "{:?}",
        bundle.passthrough.keys()
    );
    assert_eq!(bundle.media.len(), 1);
}

#[test]
fn volatile_cells_are_fresh_after_recovery_and_the_rest_is_what_was_on_screen() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "=RAND()");
    enter_cell(&mut a, doc, "=A1*0+7");
    enter_cell(&mut a, doc, "plain");
    let csv = csv_of(&a, doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    let back = csv_of(&b, id);
    let lines = |t: &str| t.lines().skip(1).map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(lines(&back), lines(&csv), "only the volatile cell is new");
    let first = back.lines().next().unwrap_or("").parse::<f64>().unwrap();
    assert!((0.0..1.0).contains(&first));
}

#[test]
fn undoing_past_the_steps_kept_is_not_the_saved_document() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    let first = a.s.docs[&doc].model.state();
    for i in 0..2100 {
        select_cell(&mut a, doc, 0, 0);
        enter_cell(&mut a, doc, &format!("v{i}"));
    }
    while a.s.docs[&doc].model.can_undo() {
        a.run(doc, Command::Suite(SuiteCommand::Undo));
    }
    assert_ne!(a.s.docs[&doc].model.state(), first);
}

#[test]
fn a_filter_and_duplicates_over_volatile_cells_replay_what_was_shown() {
    let (d, mut a, doc) = four_cells();
    select_cell(&mut a, doc, 0, 3);
    for i in 0..12 {
        enter_cell(&mut a, doc, &format!("=INT(RAND()*3)+{i}*0"));
    }
    a.settle();
    a.run(
        doc,
        Command::Grid(G::SelectRange {
            range: kiln_protocol::commands::GridRange {
                r0: 0,
                c0: 3,
                r1: 11,
                c1: 3,
            },
            add: false,
        }),
    );
    a.run(
        doc,
        Command::Grid(G::RemoveDuplicates {
            columns: vec![3],
            header: Some(false),
        }),
    );
    // Column D's cells are kept as they came out.
    let shape = |r: &Run, id: DocId| {
        let b = r.s.docs[&id].model.to_bundle().unwrap();
        let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
        let s = wb.sheets()[0].id();
        (0..12)
            .map(|row| wb.record(s, row, 3).formula.is_some())
            .collect::<Vec<_>>()
    };
    let before = shape(&a, doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(shape(&b, id), before);
}

#[test]
fn a_sort_of_plain_values_stays_one_small_record() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for i in 0..200 {
        enter_cell(&mut a, doc, &format!("{}", 200 - i));
    }
    a.settle();
    let key = only_journal(&d);
    let before = log_of(&d, &key).1.len();
    select_cell(&mut a, doc, 0, 0);
    a.run(doc, G::SortAscending.into());
    let grown = log_of(&d, &key).1.len() - before;
    assert!(grown < 200, "{grown} bytes");
}

/// Opens the only recoverable document; returns it and what its recovery said.
fn recover_with_note(r: &mut Run) -> (DocId, Option<kiln_protocol::RecoveryNote>) {
    let entries = r.recovery();
    assert_eq!(entries.len(), 1, "{entries:?}");
    let events = r.suite(
        None,
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
        .unwrap_or_else(|| panic!("nothing opened: {events:?}"));
    let note = events.iter().find_map(|e| match e {
        Event::Recovered { note, .. } => Some(note.clone()),
        _ => None,
    });
    (id, note)
}

fn grid_state(r: &Run, id: DocId) -> kiln_protocol::commands::GridState {
    let state =
        r.s.query(
            Some(id),
            &Query::Grid(kiln_protocol::commands::GridQuery::State),
        )
        .unwrap();
    let QueryResult::Grid(kiln_protocol::commands::GridQueryResult::State(state)) = state else {
        panic!("no state");
    };
    *state
}

#[test]
fn a_cell_still_being_typed_is_named_and_offered_never_reopened() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "5");
    select_cell(&mut a, doc, 0, 1);
    enter_cell(&mut a, doc, "=A1*2");
    select_cell(&mut a, doc, 0, 0);
    a.run(doc, G::BeginEdit { text: "9".into() }.into());
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    // Committed values only, and no editor open.
    assert!(csv_of(&b, id).contains("5,10"), "{}", csv_of(&b, id));
    assert!(grid_state(&b, id).editor.is_none());
    let draft = note.and_then(|n| n.draft).expect("the text is named");
    assert_eq!(
        (draft.cell.as_str(), draft.text.as_str()),
        ("Sheet1!A1", "9")
    );
    // Typing goes where the user puts it, not into the old entry.
    select_cell(&mut b, id, 4, 4);
    enter_cell(&mut b, id, "55");
    assert!(csv_of(&b, id).contains("5,10"));
    assert!(csv_of(&b, id).contains("55"));
    // Put back, by the user: Go To the cell, the text in the editor, Enter.
    b.run(
        id,
        G::GoTo {
            reference: draft.cell.clone(),
            define: false,
        }
        .into(),
    );
    b.run(id, G::BeginEdit { text: draft.text }.into());
    b.run(id, G::CommitEdit { advance: None }.into());
    assert!(csv_of(&b, id).contains("9,18"), "{}", csv_of(&b, id));
}

#[test]
fn a_draft_left_unanswered_is_offered_again_after_a_second_crash() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.run(doc, G::BeginEdit { text: "x".into() }.into());
    drop(a);
    let mut b = Run::new(&d);
    let (_, note) = recover_with_note(&mut b);
    assert!(note.and_then(|n| n.draft).is_some());
    b.settle();
    drop(b);
    let mut c = Run::new(&d);
    let (id, note) = recover_with_note(&mut c);
    assert_eq!(
        note.and_then(|n| n.draft).map(|d| d.text).as_deref(),
        Some("x")
    );
    assert!(grid_state(&c, id).editor.is_none());
}

#[test]
fn a_committed_cell_is_not_offered_as_a_draft() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    a.run(
        doc,
        G::BeginEdit {
            text: "7100".into(),
        }
        .into(),
    );
    a.run(
        doc,
        G::CommitEdit {
            advance: Some(GridDirection::Down),
        }
        .into(),
    );
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    assert!(note.and_then(|n| n.draft).is_none());
    assert!(grid_state(&b, id).editor.is_none());
    assert!(csv_of(&b, id).contains("7100"));
}

#[test]
fn text_to_columns_over_formulas_is_a_record() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    let key = only_journal(&d);
    let generation = marker(&d, &key).generation;
    select_cell(&mut a, doc, 0, 0);
    enter_cell(&mut a, doc, "=\"a,\"&INT(RAND()*1000)");
    a.run(
        doc,
        Command::Grid(G::SelectRange {
            range: kiln_protocol::commands::GridRange {
                r0: 0,
                c0: 0,
                r1: 0,
                c1: 0,
            },
            add: false,
        }),
    );
    a.run(
        doc,
        Command::Grid(G::TextToColumns {
            options: kiln_protocol::commands::grid_features::GridTextToColumns {
                delimited: true,
                delimiters: ",".into(),
                ..Default::default()
            },
        }),
    );
    select_cell(&mut a, doc, 5, 5);
    enter_cell(&mut a, doc, "and the edit after it");
    assert_eq!(marker(&d, &key).generation, generation);
    let before = grid_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    assert!(note.is_none(), "{note:?}");
    assert_eq!(grid_model(&b.s, id), before);
}

fn column_of(r: &Run, doc: DocId, col: usize) -> Vec<String> {
    csv_of(r, doc)
        .lines()
        .map(|l| l.split(',').nth(col).unwrap_or("").to_owned())
        .collect()
}

/// Sort over RAND, Undo, Redo: the order after Redo is the one on screen.
#[test]
fn a_redo_of_a_sort_on_rand_replays_the_order_on_screen() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    select_cell(&mut a, doc, 0, 0);
    for i in 0..30 {
        enter_cell(&mut a, doc, &format!("=RAND()+{i}*0"));
    }
    select_cell(&mut a, doc, 0, 1);
    for i in 0..30 {
        enter_cell(&mut a, doc, &format!("row{i}"));
    }
    a.settle();
    select_range(&mut a, doc, 0, 0, 29, 1);
    a.run(doc, Command::Grid(G::SortAscending));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    let before = column_of(&a, doc, 1);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(
        column_of(&b, id, 1),
        before,
        "column B after Redo differs from the screen"
    );
}

/// RemoveDuplicates over RAND-fed cells, Undo, Redo.
#[test]
fn a_redo_of_remove_duplicates_on_rand_replays_what_was_shown() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    select_cell(&mut a, doc, 0, 0);
    for i in 0..20 {
        enter_cell(&mut a, doc, &format!("=INT(RAND()*3)+{i}*0"));
    }
    select_cell(&mut a, doc, 0, 1);
    for i in 0..20 {
        enter_cell(&mut a, doc, &format!("r{i}"));
    }
    select_range(&mut a, doc, 0, 0, 19, 1);
    a.run(
        doc,
        Command::Grid(G::RemoveDuplicates {
            columns: vec![0],
            header: Some(false),
        }),
    );
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    let before = column_of(&a, doc, 1);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(
        column_of(&b, id, 1),
        before,
        "column B after Redo differs from the screen"
    );
}

/// Copy formula cells, Paste Values on another sheet (a change with no record) in a workbook
/// whose checkpoints are large, then one more edit; the page crashes before the checkpoint lands.
#[test]
fn paste_values_to_another_sheet_in_a_large_workbook_then_crash() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    select_cell(&mut a, doc, 0, 0);
    for v in ["=1+1", "=2+2", "=3+3"] {
        enter_cell(&mut a, doc, v);
    }
    select_range(&mut a, doc, 0, 0, 2, 0);
    a.run(
        doc,
        Command::Suite(SuiteCommand::Edit(
            kiln_protocol::commands::EditCommand::Copy,
        )),
    );
    a.run(doc, Command::Grid(G::InsertSheet));
    select_cell(&mut a, doc, 0, 0);
    a.run(doc, Command::Grid(G::PasteValues { data: None }));
    let unkept = journal_keys(&d).iter().any(|k| k.ends_with(".unkept"));
    select_cell(&mut a, doc, 0, 1);
    enter_cell(&mut a, doc, "=SUM(A1:A3)");
    let shown = csv_of(&a, doc);
    let before = grid_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let back = csv_of(&b, id);
    assert!(!unkept, "Paste Values is a record");
    assert!(note.is_none(), "{note:?}");
    assert_eq!(
        grid_model(&b.s, id),
        before,
        "shown {shown:?} back {back:?}"
    );
}

/// 450 edits while the first large checkpoint is held, then a crash at each stage of the two
/// checkpoints that follow.
#[test]
fn edits_through_two_held_checkpoints_and_a_crash_at_every_stage() {
    for stage in 0..5 {
        let d = disk();
        let (mut a, doc) = slow_checkpoints(&d);
        for i in 0..450u32 {
            select_cell(&mut a, doc, i % 40, 1 + i % 3);
            enter_cell(&mut a, doc, &format!("e{i}"));
        }
        a.settle();
        match stage {
            0 => {}
            1 => land_checkpoint(&mut a),
            2 => {
                land_checkpoint(&mut a);
                land_checkpoint(&mut a);
            }
            3 => {
                // The base on disk, no reply (the marker never moved).
                for r in &a.held {
                    if let HostOp::StorePut { area, key, bytes } = &r.op {
                        lock(&d).store.put(*area, key, bytes).unwrap();
                    }
                }
            }
            _ => {
                land_checkpoint(&mut a);
                for i in 0..250u32 {
                    select_cell(&mut a, doc, i % 40, 5);
                    enter_cell(&mut a, doc, &format!("f{i}"));
                }
                a.settle();
                for r in &a.held {
                    if let HostOp::StorePut { area, key, bytes } = &r.op {
                        lock(&d).store.put(*area, key, bytes).unwrap();
                    }
                }
            }
        }
        for i in 0..5u32 {
            select_cell(&mut a, doc, 60 + i, 0);
            enter_cell(&mut a, doc, &format!("tail{i}"));
        }
        let before = grid_model(&a.s, doc);
        let keys = journal_keys(&d);
        drop(a);
        let mut b = Run::new(&d);
        let id = b.recover_only();
        assert_eq!(grid_model(&b.s, id), before, "stage {stage}: {keys:?}");
    }
}

/// The checkpoint write fails, edits go on, the retry is held; crash.
#[test]
fn a_failed_checkpoint_then_edits_then_a_crash() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    for i in 0..205u32 {
        select_cell(&mut a, doc, i % 30, 1);
        enter_cell(&mut a, doc, &format!("e{i}"));
    }
    a.settle();
    for r in std::mem::take(&mut a.held) {
        let _ = a.s.host_reply(
            r.id,
            HostReply::Failed((&kiln_common::KilnError::Io("the disk is full".into())).into()),
        );
    }
    for i in 0..30u32 {
        select_cell(&mut a, doc, i, 4);
        enter_cell(&mut a, doc, &format!("g{i}"));
    }
    a.settle();
    let before = grid_model(&a.s, doc);
    let keys = journal_keys(&d);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(grid_model(&b.s, id), before, "{keys:?}");
}

/// Recovery twice with nothing done (idempotent), then a crash while the recovered document's
/// own checkpoint is held, with edits after it, then a third run.
#[test]
fn recovery_is_idempotent_and_survives_a_crash_during_its_checkpoint() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    for i in 0..230u32 {
        select_cell(&mut a, doc, i % 30, 1);
        enter_cell(&mut a, doc, &format!("e{i}"));
    }
    a.run(
        doc,
        GridCommand::BeginEdit {
            text: "open".into(),
        }
        .into(),
    );
    let first = grid_model(&a.s, doc);
    drop(a);
    // b: recovers, its checkpoint is held, crash with nothing done.
    let mut b = Run::new(&d);
    b.s.storage.inline_bytes = Some(0);
    let id = b.recover_only();
    assert_eq!(grid_model(&b.s, id), first, "b");
    drop(b);
    // c: recovers, edits during its held checkpoint, crash.
    let mut c = Run::new(&d);
    c.s.storage.inline_bytes = Some(0);
    let id = c.recover_only();
    assert_eq!(grid_model(&c.s, id), first, "c");
    c.run(id, GridCommand::CommitEdit { advance: None }.into());
    for i in 0..12u32 {
        select_cell(&mut c, id, 50 + i, 2);
        enter_cell(&mut c, id, &format!("c{i}"));
    }
    let second = grid_model(&c.s, id);
    drop(c);
    let mut e = Run::new(&d);
    let id = e.recover_only();
    assert_eq!(grid_model(&e.s, id), second, "e");
}

/// Save at several points around the batched undo trim; dirty, undo past the trim, recover.
#[test]
fn saves_around_the_undo_limit_keep_dirty_right() {
    for save_at in [500u32, 1000, 1999, 2000, 2001] {
        let d = disk();
        let mut a = Run::new(&d);
        let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
        a.settle();
        let path = format!("/docs/T{save_at}.xlsx");
        for i in 0..2300u32 {
            select_cell(&mut a, doc, i % 7, 0);
            enter_cell(&mut a, doc, &format!("v{i}"));
            if i + 1 == save_at {
                a.suite(
                    Some(doc),
                    SuiteCommand::SaveAs {
                        path: Some(path.clone()),
                        format: None,
                    },
                );
                assert!(
                    !a.summary(doc).dirty,
                    "{save_at}: dirty right after the save"
                );
            }
        }
        a.settle();
        assert!(
            a.summary(doc).dirty,
            "{save_at}: edits after the save are not dirty"
        );
        // Undo all the way; dirty must be true unless the content is the saved one.
        let mut n = 0;
        while a.s.docs[&doc].model.can_undo() {
            a.run(doc, Command::Suite(SuiteCommand::Undo));
            n += 1;
            let at = 2300 - n; // edits still applied
            let dirty = a.summary(doc).dirty;
            if at == save_at {
                assert!(!dirty, "{save_at}: back at the saved content but dirty");
            } else {
                assert!(
                    dirty,
                    "{save_at}: not dirty after {n} undos (content after edit {at})"
                );
            }
            a.settle();
        }
        let before = grid_model(&a.s, doc);
        drop(a);
        let mut b = Run::new(&d);
        let entries = b.recovery();
        if entries.is_empty() {
            panic!("{save_at}: nothing to recover after undoing to an unsaved state");
        }
        let id = b.recover_only();
        assert_eq!(grid_model(&b.s, id), before, "{save_at}");
    }
}

/// Others' edits want a checkpoint; this window types within the second, in a document whose
/// checkpoints are large; the page crashes before that checkpoint lands.
#[test]
fn an_edit_after_others_edits_in_a_large_document_then_crash() {
    let d = disk();
    let mut a = Run::new(&d);
    a.s.storage.inline_bytes = Some(0);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "mine ");
    land_checkpoint(&mut a);
    a.settle();
    lock(&d).hold_checkpoints = true;
    a.s.docs
        .get_mut(&id)
        .unwrap()
        .model
        .execute(
            &WriteCommand::InsertText {
                text: "theirs ".into(),
            }
            .into(),
        )
        .unwrap();
    a.s.storage_remote(id);
    a.type_text(id, "and mine again");
    let text = a.text(id);
    let keys = journal_keys(&d);
    drop(a);
    // Others' edits are no record: until the checkpoint after them lands, a recovery stops
    // before them, says so, and the page asks before it goes.
    assert!(keys.iter().any(|k| k.ends_with(".unkept")), "{keys:?}");
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let stop = note.and_then(|n| n.stopped).expect("named");
    assert_eq!(stop.command.as_deref(), Some("collab.remote"));
    assert_eq!(stop.after, 1, "the edit after them");
    assert_eq!(b.text(id), vec!["mine ".to_owned()], "all text {text:?}");
}

#[test]
fn a_document_whose_first_copy_never_landed_is_listed_with_the_reason() {
    let d = disk();
    let mut a = Run::new(&d);
    a.s.storage.inline_bytes = Some(0);
    lock(&d).hold_checkpoints = true;
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.settle();
    a.type_text(doc, "lost");
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].title, "Document1");
    assert!(
        entries[0]
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("could not be kept"))
    );
    b.suite(
        None,
        SuiteCommand::RecoveryDelete {
            key: entries[0].key.clone(),
        },
    );
    assert!(b.recovery().is_empty());
    assert!(journal_keys(&d).is_empty(), "{:?}", journal_keys(&d));
}

/// A checkpoint that fails, edits after it, the retry held: Write and Deck keep every edit too.
#[test]
fn write_and_deck_keep_edits_made_after_a_checkpoint_failed() {
    for kind in [DocKind::Write, DocKind::Deck] {
        let d = disk();
        let mut a = Run::new(&d);
        a.s.storage.inline_bytes = Some(0);
        let doc = a.s.new_document(kind).unwrap().doc;
        a.settle();
        let edit = |a: &mut Run| match kind {
            DocKind::Write => a.type_text(doc, "x"),
            _ => a.run(doc, Command::Deck(DeckCommand::NewSlide { layout: None })),
        };
        // The first checkpoint lands; the next ones are held.
        edit(&mut a);
        a.settle();
        lock(&d).hold_checkpoints = true;
        for _ in 0..205 {
            edit(&mut a);
        }
        a.settle();
        for r in std::mem::take(&mut a.held) {
            let _ = a.s.host_reply(
                r.id,
                HostReply::Failed((&kiln_common::KilnError::Io("the disk is full".into())).into()),
            );
        }
        for _ in 0..30 {
            edit(&mut a);
        }
        a.settle();
        let before = match kind {
            DocKind::Write => write_model(&a.s, doc),
            _ => deck_model(&a.s, doc),
        };
        drop(a);
        let mut b = Run::new(&d);
        let (id, note) = recover_with_note(&mut b);
        assert!(note.is_none(), "{kind:?}: {note:?}");
        let after = match kind {
            DocKind::Write => write_model(&b.s, id),
            _ => deck_model(&b.s, id),
        };
        assert_eq!(after, before, "{kind:?}");
    }
}

#[test]
fn a_cell_opened_and_left_as_it_was_is_nothing_to_recover() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    a.run(doc, GridCommand::EditCell.into());
    a.run(doc, GridCommand::BeginEdit { text: "n".into() }.into());
    a.run(doc, GridCommand::CancelEdit.into());
    drop(a);
    let mut b = Run::new(&d);
    assert!(b.recovery().is_empty(), "Escape took the text back");
    // A double-click on a filled cell, with the journal already going, changes nothing either.
    let doc = b.s.new_document(DocKind::Grid).unwrap().doc;
    b.settle();
    enter_cell(&mut b, doc, "5");
    b.run(
        doc,
        GridCommand::SelectCell {
            row: 0,
            col: 0,
            extend: false,
            add: false,
        }
        .into(),
    );
    b.run(doc, GridCommand::EditCell.into());
    assert!(
        !journal_keys(&d).iter().any(|k| k.ends_with(".draft")),
        "{:?}",
        journal_keys(&d)
    );
}

fn select_cell(a: &mut Run, doc: DocId, row: u32, col: u32) {
    a.run(
        doc,
        Command::Grid(GridCommand::SelectCell {
            row,
            col,
            extend: false,
            add: false,
        }),
    );
}
fn select_range(a: &mut Run, doc: DocId, r0: u32, c0: u32, r1: u32, c1: u32) {
    a.run(
        doc,
        Command::Grid(GridCommand::SelectRange {
            range: kiln_protocol::commands::GridRange { r0, c0, r1, c1 },
            add: false,
        }),
    );
}
fn four_cells() -> (Arc<Mutex<Disk>>, Run, DocId) {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for v in ["30", "10", "=A1+A2", "20"] {
        enter_cell(&mut a, doc, v);
    }
    a.settle();
    (d, a, doc)
}
fn assert_recovers(d: &Arc<Mutex<Disk>>, a: Run, doc: DocId) {
    let before = grid_model(&a.s, doc);
    let csv = csv_of(&a, doc);
    drop(a);
    let mut b = Run::new(d);
    let id = b.recover_only();
    let _ = csv;
    assert_eq!(grid_model(&b.s, id), before, "model bytes differ");
}

#[test]
fn grid_edits_past_the_undo_limit_reach_the_journal() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    let mut first_missing = None;
    for i in 0..1100u32 {
        select_cell(&mut a, doc, 0, 0);
        let before = a.s.docs[&doc].model.state();
        enter_cell(&mut a, doc, &format!("v{i}"));
        if a.s.docs[&doc].model.state() == before && first_missing.is_none() {
            first_missing = Some(i);
        }
        a.settle();
    }
    let csv = csv_of(&a, doc);
    assert!(csv.contains("v1099"), "live: {csv}");
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    let got = csv_of(&b, id);
    assert!(
        got.contains("v1099"),
        "first edit that did not move the state: {first_missing:?}; recovered: {got:?}"
    );
}

#[test]
fn a_sort_on_rand_replays_the_order_on_screen() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    select_cell(&mut a, doc, 0, 0);
    for i in 0..30 {
        enter_cell(&mut a, doc, &format!("=RAND()+{i}*0"));
    }
    select_cell(&mut a, doc, 0, 1);
    for i in 0..30 {
        enter_cell(&mut a, doc, &format!("row{i}"));
    }
    a.settle();
    select_range(&mut a, doc, 0, 0, 29, 1);
    a.run(doc, Command::Grid(GridCommand::SortAscending));
    let col_b = |csv: String| {
        csv.lines()
            .map(|l| l.rsplit(',').next().unwrap_or("").to_owned())
            .collect::<Vec<_>>()
    };
    let before = col_b(csv_of(&a, doc));
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(col_b(csv_of(&b, id)), before);
}

macro_rules! replays {
    ($name:ident, |$a:ident, $doc:ident| $body:block) => {
        #[test]
        fn $name() {
            let (d, mut $a, $doc) = four_cells();
            $body
            assert_recovers(&d, $a, $doc);
        }
    };
}
replays!(replay_structure_refs, |a, doc| {
    select_cell(&mut a, doc, 1, 0);
    a.run(doc, Command::Grid(G::InsertRows));
    select_cell(&mut a, doc, 0, 0);
    a.run(doc, Command::Grid(G::InsertColumns));
    select_cell(&mut a, doc, 3, 1);
    a.run(doc, Command::Grid(G::DeleteRows));
});
replays!(replay_sheets, |a, doc| {
    a.run(doc, Command::Grid(G::InsertSheet));
    enter_cell(&mut a, doc, "=Sheet1!A1*2");
    a.run(
        doc,
        Command::Grid(G::RenameSheet {
            index: 0,
            name: "Data".into(),
        }),
    );
    a.run(doc, Command::Grid(G::DuplicateSheet { index: 0 }));
    a.run(doc, Command::Grid(G::MoveSheet { from: 2, to: 0 }));
    a.run(doc, Command::Grid(G::ActivateSheet { index: 1 }));
    enter_cell(&mut a, doc, "on the duplicate");
    a.run(doc, Command::Grid(G::DeleteSheet { index: 0 }));
    a.run(doc, Command::Grid(G::InsertSheet));
    enter_cell(&mut a, doc, "after delete");
});
replays!(replay_two_sheets_order, |a, doc| {
    a.run(doc, Command::Grid(G::InsertSheet));
    enter_cell(&mut a, doc, "5");
    a.run(doc, Command::Grid(G::ActivateSheet { index: 0 }));
    select_cell(&mut a, doc, 0, 3);
    enter_cell(&mut a, doc, "=Sheet2!A1+A1");
    a.run(doc, Command::Grid(G::ActivateSheet { index: 1 }));
    select_cell(&mut a, doc, 0, 0);
    enter_cell(&mut a, doc, "6");
});
replays!(replay_fill_autofill, |a, doc| {
    select_cell(&mut a, doc, 0, 2);
    enter_cell(&mut a, doc, "1");
    enter_cell(&mut a, doc, "2");
    select_range(&mut a, doc, 0, 2, 1, 2);
    a.run(doc, Command::Grid(G::DragFill { row: 9, col: 2 }));
    a.run(doc, Command::Grid(G::FillDown));
    select_range(&mut a, doc, 0, 0, 3, 0);
    a.run(doc, Command::Grid(G::FillRight));
    a.run(doc, Command::Grid(G::AutoSum { function: None }));
});
replays!(replay_format_wrap_fit, |a, doc| {
    select_cell(&mut a, doc, 0, 0);
    a.run(
        doc,
        Command::Grid(G::FillColor {
            color: Some("FF0000".into()),
        }),
    );
    a.run(doc, Command::Grid(G::WrapText));
    enter_cell(
        &mut a,
        doc,
        "a very long text that wraps over a few lines in a narrow column for sure yes yes",
    );
    a.run(doc, Command::Grid(G::MergeCenter));
    a.run(doc, Command::Grid(G::UnmergeCells));
    a.run(doc, Command::Grid(G::HideRows));
    a.run(doc, Command::Grid(G::UnhideRows));
});
replays!(replay_after_undo_edit, |a, doc| {
    select_cell(&mut a, doc, 0, 3);
    a.run(doc, Command::Grid(G::WrapText));
    enter_cell(
        &mut a,
        doc,
        "a very long text that wraps over a few lines in a narrow column for sure yes yes",
    );
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    enter_cell(&mut a, doc, "next");
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
});
replays!(replay_two_hundred_boundary, |a, doc| {
    for i in 0..205 {
        select_cell(&mut a, doc, 5, 5);
        enter_cell(&mut a, doc, &format!("x{i}"));
    }
});
replays!(replay_array_spill, |a, doc| {
    select_cell(&mut a, doc, 0, 4);
    enter_cell(&mut a, doc, "=SORT(A1:A4)");
    enter_cell(&mut a, doc, "");
    select_cell(&mut a, doc, 0, 6);
    enter_cell(&mut a, doc, "=SEQUENCE(3)");
    select_cell(&mut a, doc, 1, 6);
    enter_cell(&mut a, doc, "blocks");
});
replays!(replay_calc_order, |a, doc| {
    select_cell(&mut a, doc, 0, 5);
    enter_cell(&mut a, doc, "=F2+1");
    enter_cell(&mut a, doc, "=F3+1");
    enter_cell(&mut a, doc, "=F1+1");
    enter_cell(&mut a, doc, "fix");
    select_cell(&mut a, doc, 2, 5);
    enter_cell(&mut a, doc, "1");
});

#[test]
fn a_grid_edit_past_the_undo_limit_marks_the_document_changed() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for i in 0..1000u32 {
        select_cell(&mut a, doc, 0, 0);
        enter_cell(&mut a, doc, &format!("v{i}"));
    }
    a.settle();
    let s0 = a.s.docs[&doc].model.state();
    a.s.docs.get_mut(&doc).unwrap().saved_state = s0; // what Save does
    select_cell(&mut a, doc, 0, 0);
    enter_cell(&mut a, doc, "after the save");
    a.settle();
    let s1 = a.s.docs[&doc].model.state();
    assert!(csv_of(&a, doc).contains("after the save"));
    assert_ne!(
        s0, s1,
        "the model says the edit changed nothing: not dirty, no AutoSave, no question at close"
    );
}

#[test]
fn the_theme_the_window_sends_leaves_a_recovered_entry_open() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "kept");
    a.run(
        doc,
        GridCommand::BeginEdit {
            text: "half".into(),
        }
        .into(),
    );
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert!(!csv_of(&b, id).contains("half"));
    b.run(id, GridCommand::Theme { dark: false }.into());
    assert!(
        !csv_of(&b, id).contains("half"),
        "committed by Theme: {:?}",
        csv_of(&b, id)
    );
}

#[test]
fn write_and_deck_edits_past_the_undo_limit_are_journalled_and_mark_the_document() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.settle();
    for _ in 0..1100 {
        a.type_text(doc, "x");
    }
    a.settle();
    let state = a.s.docs[&doc].model.state();
    a.s.docs.get_mut(&doc).unwrap().saved_state = state;
    a.type_text(doc, "after the save");
    assert_ne!(a.s.docs[&doc].model.state(), state);
    assert!(a.summary(doc).dirty);
    let text = a.text(doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.text(id), text);

    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Deck).unwrap().doc;
    a.settle();
    for _ in 0..1100 {
        a.run(doc, Command::Deck(DeckCommand::NewSlide { layout: None }));
    }
    a.settle();
    let state = a.s.docs[&doc].model.state();
    a.s.docs.get_mut(&doc).unwrap().saved_state = state;
    a.run(doc, Command::Deck(DeckCommand::NewSlide { layout: None }));
    assert_ne!(a.s.docs[&doc].model.state(), state);
    assert!(a.summary(doc).dirty);
    let slides = deck_slides(&a.s, doc).len();
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(deck_slides(&b.s, id).len(), slides);
}

fn deck_slides(s: &Session, doc: DocId) -> Vec<kiln_deck::Slide> {
    let b = s.docs[&doc].model.to_bundle().unwrap();
    kiln_deck::Presentation::from_bytes(&b.model)
        .unwrap()
        .slides
}

#[test]
fn a_presentation_is_recovered_after_a_crash() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Deck).unwrap().doc;
    a.settle();
    a.run(doc, Command::Deck(DeckCommand::NewSlide { layout: None }));
    a.run(
        doc,
        Command::Deck(DeckCommand::InsertTextBox { rect: None }),
    );
    a.run(
        doc,
        Command::Deck(DeckCommand::InsertText {
            text: "Kiln firing".into(),
        }),
    );
    a.run(doc, Command::Deck(DeckCommand::NewSlide { layout: None }));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.settle();
    let before = deck_slides(&a.s, doc);
    assert_eq!(before.len(), 2);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(deck_slides(&b.s, id), before);
}

/// The key of the only journal on the disk.
fn only_journal(d: &Arc<Mutex<Disk>>) -> String {
    let keys = lock(d).store.list(StoreArea::Journal, "").unwrap();
    let (marked, _) = journal::journals(&keys);
    assert_eq!(marked.len(), 1, "{keys:?}");
    marked[0].clone()
}

/// The marker of a journal on the disk.
fn marker(d: &Arc<Mutex<Disk>>, key: &str) -> journal::JournalMeta {
    let bytes = lock(d)
        .store
        .get(StoreArea::Journal, &journal::meta_key(key))
        .unwrap()
        .unwrap_or_else(|| panic!("no marker of {key}"));
    journal::decode_meta(&bytes).unwrap()
}

/// The log of a journal's current checkpoint.
fn log_of(d: &Arc<Mutex<Disk>>, key: &str) -> (String, Vec<u8>) {
    let log = journal::log_key(key, marker(d, key).generation);
    let bytes = lock(d)
        .store
        .get(StoreArea::Journal, &log)
        .unwrap()
        .unwrap_or_default();
    (log, bytes)
}

/// Cuts the last bytes off a journal's log (a crash in the middle of a record).
fn tear(d: &Arc<Mutex<Disk>>, key: &str, bytes: usize) {
    let (log, b) = log_of(d, key);
    lock(d)
        .store
        .put(StoreArea::Journal, &log, &b[..b.len() - bytes])
        .unwrap();
}

#[test]
fn a_record_the_crash_cut_short_is_dropped_and_the_rest_recovered() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "kept ");
    a.type_text(doc, "kept too ");
    let kept = a.text(doc);
    a.type_text(doc, "lost in the crash");
    a.settle();
    drop(a);
    // The crash came while the last record was being written.
    let key = only_journal(&d);
    tear(&d, &key, 3);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.text(id), kept);
    // Work goes on after the recovery, and a second crash loses none of it: the recovered
    // document was checkpointed, so new records never follow the torn one.
    b.type_text(id, "after the recovery");
    let again = b.text(id);
    drop(b);
    let mut c = Run::new(&d);
    let id = c.recover_only();
    assert_eq!(c.text(id), again);
}

#[test]
fn a_failed_write_of_the_journal_is_reported_and_followed_by_a_checkpoint() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "one ");
    a.settle();
    // The disk is full for one record: the user hears of it, naming the document.
    lock(&d).fail_appends = 1;
    a.type_text(doc, "two ");
    let events = a.settle();
    let errors: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error {
                doc: Some(d),
                error,
            } if *d == doc => Some(error.message.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "{events:?}");
    assert!(
        errors[0].contains("could not keep a recovery copy of Document1"),
        "{errors:?}"
    );
    // The next change goes on from a checkpoint, not after the lost record.
    a.type_text(doc, "three");
    let text = a.text(doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.text(id), text);
}

#[test]
fn a_journal_from_an_older_kiln_is_recovered_and_one_from_a_newer_is_refused_clearly() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "an older journal");
    let text = a.text(doc);
    drop(a);
    let key = only_journal(&d);
    // The journal as the first format wrote it: `k.base` and `k.log`, a marker without the
    // file fields or a generation.
    {
        let g = marker(&d, &key).generation;
        let mut disk = lock(&d);
        for (new, old) in [
            (journal::base_key(&key, g), journal::base_key(&key, 0)),
            (journal::log_key(&key, g), journal::log_key(&key, 0)),
        ] {
            let bytes = disk.store.get(StoreArea::Journal, &new).unwrap().unwrap();
            disk.store.delete(StoreArea::Journal, &new).unwrap();
            disk.store.put(StoreArea::Journal, &old, &bytes).unwrap();
        }
        let meta = format!(
            r#"{{"format":1,"key":"{key}","kind":"Write","title":"Old","created_ms":1,"saved_at_ms":2,"dirty":true}}"#
        );
        disk.store
            .put(
                StoreArea::Journal,
                &journal::meta_key(&key),
                meta.as_bytes(),
            )
            .unwrap();
    }
    let mut b = Run::new(&d);
    let entries = b.recovery();
    assert_eq!(entries[0].title, "Old");
    let id = b.recover_only();
    assert_eq!(b.text(id), text);
    drop(b);

    // A journal written by a newer Kiln: listed with the reason, cannot open, can be deleted.
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "from the future");
    drop(a);
    let key = only_journal(&d);
    {
        let mut disk = lock(&d);
        let meta = format!(
            r#"{{"format":9,"key":"{key}","kind":"Write","title":"New","created_ms":1,"saved_at_ms":2,"dirty":true,"x":1}}"#
        );
        disk.store
            .put(
                StoreArea::Journal,
                &journal::meta_key(&key),
                meta.as_bytes(),
            )
            .unwrap();
    }
    let mut b = Run::new(&d);
    let entries = b.recovery();
    let problem = entries[0].problem.clone().unwrap_or_default();
    assert!(problem.contains("newer version of Kiln"), "{problem}");
    let events = b.suite(
        None,
        SuiteCommand::RecoveryOpen {
            key: key.clone(),
            kind: DocKind::Write,
        },
    );
    assert!(
        events.iter().any(
            |e| matches!(e, Event::Error { error, .. } if error.message.contains("newer version"))
        ),
        "{events:?}"
    );
    assert!(!events.iter().any(|e| matches!(e, Event::Opened { .. })));
    let events = b.suite(None, SuiteCommand::RecoveryDelete { key });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Recovery { entries } if entries.is_empty()))
    );
    assert!(
        lock(&d)
            .store
            .list(StoreArea::Journal, "")
            .unwrap()
            .is_empty()
    );
}

fn journal_keys(d: &Arc<Mutex<Disk>>) -> Vec<String> {
    lock(d).store.list(StoreArea::Journal, "").unwrap()
}

#[test]
fn a_clean_close_removes_the_journal_and_a_save_clears_the_marker() {
    let d = disk();
    let mut a = Run::new(&d);
    // Saved, then closed: nothing is left.
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "closed cleanly");
    a.suite(
        Some(doc),
        SuiteCommand::SaveAs {
            path: Some("/docs/Clean.docx".into()),
            format: None,
        },
    );
    a.suite(Some(doc), SuiteCommand::Close);
    assert!(journal_keys(&d).is_empty());
    // Closed with Don't Save: nothing is left either.
    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "thrown away");
    a.suite(Some(doc), SuiteCommand::CloseWithoutSaving);
    assert!(journal_keys(&d).is_empty());

    let doc = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(doc, "saved");
    a.suite(
        Some(doc),
        SuiteCommand::SaveAs {
            path: Some("/docs/Saved.docx".into()),
            format: None,
        },
    );
    assert!(!a.summary(doc).dirty);
    drop(a);
    let mut b = Run::new(&d);
    assert!(b.recovery().is_empty(), "nothing unsaved");
    assert!(
        lock(&d)
            .store
            .list(StoreArea::Journal, "")
            .unwrap()
            .is_empty(),
        "the clean journal is removed by the scan"
    );
}

#[test]
fn autosave_writes_the_file_two_seconds_after_the_last_edit() {
    let d = disk();
    lock(&d).files.insert(
        "/docs/Plan.docx".into(),
        fixture("docx/features/tables.docx"),
    );
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Plan.docx");
    assert_eq!(a.summary(id).autosave, AutoSave::Off);
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    assert_eq!(a.summary(id).autosave, AutoSave::On);
    let original = lock(&d).files["/docs/Plan.docx"].clone();
    a.type_text(id, "Autosaved words ");
    // The timer runs out: the file is written and the document is clean.
    let events = a.settle();
    assert!(
        events.iter().any(|e| matches!(e, Event::Saved { .. })),
        "{events:?}"
    );
    assert!(!a.summary(id).dirty);
    let written = lock(&d).files["/docs/Plan.docx"].clone();
    assert_ne!(written, original);
    assert!(docx_text(&written).contains("Autosaved words"));
    // AutoSave takes no version: only explicit saves (and ten minutes of editing) do.
    match a.s.query_mut(Some(id), &Query::Versions).unwrap() {
        QueryResult::Versions(v) => assert!(v.is_empty(), "{v:?}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn autosave_refuses_a_file_without_a_file_or_one_that_would_lose_content() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    assert!(
        matches!(a.summary(id).autosave, AutoSave::Unavailable(ref why) if why.contains("Save"))
    );
    assert!(
        a.s.execute(
            Some(id),
            Command::Suite(SuiteCommand::SetAutoSave { on: true })
        )
        .is_err()
    );
    // A workbook of two sheets saved as CSV would lose one: Save would warn, so AutoSave cannot.
    lock(&d)
        .files
        .insert("/docs/Two.csv".into(), b"a,b\n1,2\n".to_vec());
    let g = a.open_file(&d, "/docs/Two.csv");
    a.run(g, GridCommand::InsertSheet.into());
    let e =
        a.s.execute(
            Some(g),
            Command::Suite(SuiteCommand::SetAutoSave { on: true }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("would lose content"), "{e}");
}

#[test]
fn a_bundle_round_trips_through_the_session_with_its_versions() {
    let d = disk();
    lock(&d).files.insert(
        "/docs/Tables.docx".into(),
        fixture("docx/features/tables.docx"),
    );
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Tables.docx");
    let text = a.text(id);
    assert!(a.summary(id).save_formats.contains(&FileFormat::KWrite));
    a.type_text(id, "Version one. ");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Tables.kwrite".into()),
            format: None,
        },
    );
    assert_eq!(a.summary(id).format, Some(FileFormat::KWrite));
    a.type_text(id, "Version two. ");
    a.suite(Some(id), SuiteCommand::Save);
    let edited = a.text(id);
    let b = kiln_storage::Bundle::from_bytes(&lock(&d).files["/docs/Tables.kwrite"]).unwrap();
    assert_eq!(b.versions.len(), 2, "both saves are in the file");
    assert!(b.passthrough.contains_key("original.docx"));
    drop(a);
    // Another run opens the bundle: the text and the versions are there; as .docx again.
    let mut c = Run::new(&d);
    let id = c.open_file(&d, "/docs/Tables.kwrite");
    assert_eq!(c.text(id), edited);
    assert_ne!(c.text(id), text);
    match c.s.query_mut(Some(id), &Query::Versions).unwrap() {
        QueryResult::Versions(v) => assert_eq!(v.len(), 2),
        other => panic!("{other:?}"),
    }
    c.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Back.docx".into()),
            format: None,
        },
    );
    assert!(docx_text(&lock(&d).files["/docs/Back.docx"]).contains("Version two."));
}

#[test]
fn a_template_opens_as_a_new_untitled_document_and_its_file_is_never_written() {
    let d = disk();
    lock(&d).files.insert(
        "/docs/Tables.docx".into(),
        fixture("docx/features/tables.docx"),
    );
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Tables.docx");
    a.type_text(id, "From the template. ");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/templates/Letter.kwrite".into()),
            format: None,
        },
    );
    let template = lock(&d).files["/templates/Letter.kwrite"].clone();
    let text = a.text(id);
    drop(a);

    let mut b = Run::new(&d);
    let hint = OpenHint {
        name: Some("Letter.kwrite".into()),
        template: true,
        ..OpenHint::default()
    };
    let first = b.s.open(&template, hint.clone()).unwrap();
    let second = b.s.open(&template, hint).unwrap();
    b.settle();
    // The template's content, as a new document: no path, no format, an untitled name.
    assert_eq!(b.text(first.doc), text);
    assert_eq!((first.path.as_deref(), first.format), (None, None));
    assert_eq!(
        (first.title.as_str(), second.title.as_str()),
        ("Document1", "Document2")
    );
    assert!(!first.dirty);
    match b.s.query_mut(Some(first.doc), &Query::Versions).unwrap() {
        QueryResult::Versions(v) => assert!(v.is_empty(), "the template's history stays behind"),
        other => panic!("{other:?}"),
    }
    // Save asks where to put it (the host's Save dialog), and the template is left as it was.
    b.type_text(first.doc, "Mine. ");
    b.s.execute(Some(first.doc), Command::Suite(SuiteCommand::Save))
        .unwrap();
    let asked = b.s.poll().into_iter().any(|e| {
        matches!(
            e,
            Event::HostRequest(kiln_protocol::HostRequest {
                op: HostOp::PickSave { .. },
                ..
            })
        )
    });
    assert!(asked, "Save of a document from a template asks for a place");
    assert_eq!(lock(&d).files["/templates/Letter.kwrite"], template);
    assert!(
        !b.s.query(None, &Query::Recents)
            .map(|r| format!("{r:?}"))
            .unwrap_or_default()
            .contains("Letter.kwrite"),
        "a template is not a recent file"
    );
    // Bytes that are not a template are refused plainly.
    let bad = b.s.open(
        b"not a bundle",
        OpenHint {
            template: true,
            ..OpenHint::default()
        },
    );
    assert!(bad.is_err());
}

fn versions(r: &mut Run, id: DocId) -> Vec<kiln_protocol::collab::VersionEntry> {
    match r.s.query_mut(Some(id), &Query::Versions).unwrap() {
        QueryResult::Versions(v) => v,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_local_file_keeps_versions_that_preview_compare_name_and_restore() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "First draft");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Story.docx".into()),
            format: None,
        },
    );
    a.type_text(id, " grows");
    a.suite(Some(id), SuiteCommand::Save);
    // Saving again without a change takes no version.
    a.suite(Some(id), SuiteCommand::Save);
    let v = versions(&mut a, id);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(
        v.iter()
            .all(|x| x.origin == VersionOrigin::Save && x.author == "You")
    );
    let first = v[0].id.clone();
    a.type_text(id, " and grows");
    let now = a.text(id);

    // Shown read-only in place of the document.
    a.suite(
        Some(id),
        SuiteCommand::VersionPreview {
            id: Some(first.clone()),
        },
    );
    assert_eq!(a.text(id), ["First draft"]);
    assert!(a.summary(id).read_only);
    assert!(
        a.s.execute(
            Some(id),
            WriteCommand::InsertText { text: "x".into() }.into()
        )
        .is_err(),
        "a version takes no edits"
    );
    // Compared word by word with the document.
    match a
        .s
        .query_mut(Some(id), &Query::VersionCompare { id: first.clone() })
        .unwrap()
    {
        QueryResult::VersionCompare(p) => {
            let added: String = p
                .iter()
                .flat_map(|p| &p.spans)
                .filter(|s| s.change == VersionChange::Added)
                .map(|s| s.text.as_str())
                .collect();
            assert!(added.contains("grows"), "{p:?}");
        }
        other => panic!("{other:?}"),
    }
    a.suite(
        Some(id),
        SuiteCommand::VersionRename {
            id: first.clone(),
            name: "Before the edits".into(),
        },
    );
    a.suite(Some(id), SuiteCommand::VersionPreview { id: None });
    assert_eq!(a.text(id), now, "back to the document");
    assert!(!a.summary(id).read_only);

    // Restored as a new version.
    a.suite(Some(id), SuiteCommand::VersionRestore { id: first.clone() });
    assert_eq!(a.text(id), ["First draft"]);
    assert!(a.summary(id).dirty);
    let v = versions(&mut a, id);
    assert_eq!(v.len(), 3);
    assert_eq!(v[2].origin, VersionOrigin::Restore(first.clone()));
    drop(a);

    // The versions of a .docx live in the app storage: another run sees them, named.
    let mut b = Run::new(&d);
    let id = b.open_file(&d, "/docs/Story.docx");
    let v = versions(&mut b, id);
    assert_eq!(v.len(), 3);
    assert_eq!(v[0].name.as_deref(), Some("Before the edits"));
    b.suite(
        Some(id),
        SuiteCommand::VersionPreview {
            id: Some(v[1].id.clone()),
        },
    );
    assert_eq!(b.text(id), ["First draft grows"]);
}

#[test]
fn ten_minutes_of_editing_take_a_version() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "start");
    a.later(kiln_storage::versions::LocalHistory::INTERVAL_MS - 1);
    a.type_text(id, " still");
    assert!(versions(&mut a, id).is_empty());
    a.later(1);
    a.type_text(id, " now");
    let v = versions(&mut a, id);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].origin, VersionOrigin::Auto);
}

#[test]
fn a_presentation_version_marks_the_slides_that_changed() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Deck).unwrap().doc;
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Pitch.kdeck".into()),
            format: None,
        },
    );
    a.run(id, Command::Deck(DeckCommand::NewSlide { layout: None }));
    a.suite(Some(id), SuiteCommand::Save);
    let v = versions(&mut a, id);
    assert_eq!(v.len(), 2);
    a.suite(
        Some(id),
        SuiteCommand::VersionPreview {
            id: Some(v[1].id.clone()),
        },
    );
    match a
        .s
        .query_mut(
            Some(id),
            &Query::VersionSlides {
                id: v[1].id.clone(),
                width: 120.0,
                dpr: 1.0,
            },
        )
        .unwrap()
    {
        QueryResult::VersionSlides(s) => {
            let marks: Vec<VersionSlideMark> = s.slides.iter().map(|x| x.mark).collect();
            assert_eq!(marks, [VersionSlideMark::Same, VersionSlideMark::Added]);
            assert_eq!(s.scene.tiles.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn opened_and_saved_files_are_recent_and_can_be_pinned() {
    let d = disk();
    lock(&d).files.insert(
        "/docs/Budget.xlsx".into(),
        fixture("xlsx/features/formulas.xlsx"),
    );
    let mut a = Run::new(&d);
    let g = a.open_file(&d, "/docs/Budget.xlsx");
    let w = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(w, "Letter");
    a.later(5);
    a.suite(
        Some(w),
        SuiteCommand::SaveAs {
            path: Some("/docs/Letter.docx".into()),
            format: None,
        },
    );
    let recents = |r: &Run| match r.s.query(None, &Query::Recents).unwrap() {
        QueryResult::Recents(f) => f,
        other => panic!("{other:?}"),
    };
    let list = recents(&a);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "Letter.docx");
    assert_eq!(list[0].kind, DocKind::Write);
    assert!(
        list[0]
            .thumbnail
            .as_deref()
            .is_some_and(|t| t.starts_with("data:image/png;base64,")),
        "a saved file has a thumbnail"
    );
    assert_eq!(list[1].kind, DocKind::Grid);
    let _ = g;
    a.suite(
        None,
        SuiteCommand::RecentPin {
            path: "/docs/Budget.xlsx".into(),
            pinned: true,
        },
    );
    drop(a);
    let mut b = Run::new(&d);
    let events = b.suite(None, SuiteCommand::RecentsLoad);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Recents { files } if files.len() == 2))
    );
    let list = recents(&b);
    assert_eq!(list[0].path, "/docs/Budget.xlsx");
    assert!(list[0].pinned);
    b.suite(
        None,
        SuiteCommand::RecentRemove {
            path: "/docs/Letter.docx".into(),
        },
    );
    assert_eq!(recents(&b).len(), 1);
}

#[test]
fn the_user_dictionary_is_kept_for_every_document() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.run(
        id,
        WriteCommand::SpellAddWord {
            word: "Kilnwright".into(),
        }
        .into(),
    );
    // An editor sends the words it kept itself: they join the stored ones.
    a.run(
        id,
        WriteCommand::SetUserDictionary {
            words: vec!["bisqueware".into()],
        }
        .into(),
    );
    drop(a);
    let stored = lock(&d)
        .store
        .get(StoreArea::Dictionary, kiln_storage::dictionary::KEY)
        .unwrap()
        .unwrap();
    assert_eq!(
        kiln_storage::dictionary::decode(&stored),
        ["Kilnwright", "bisqueware"]
    );
}

#[test]
fn a_session_without_storage_asks_for_none() {
    let mut s = Session::new(Box::new(crate::DeferredHost::new(|| 0)));
    let id = s.new_document(DocKind::Write).unwrap().doc;
    s.execute(
        Some(id),
        WriteCommand::InsertText { text: "x".into() }.into(),
    )
    .unwrap();
    assert!(
        !s.poll().iter().any(|e| matches!(e, Event::HostRequest(_))),
        "no storage requests"
    );
    assert!(matches!(
        s.summary(id).unwrap().autosave,
        AutoSave::Unavailable(_)
    ));
}

/// The document as the journal would restore it, its id allocator aside (ids handed out but
/// never used are not in any operation).
fn write_model(s: &Session, doc: DocId) -> Vec<u8> {
    let b = s.docs[&doc].model.to_bundle().unwrap();
    let mut d = kiln_doc::Document::from_bytes(&b.model).unwrap();
    d.reserve_ids_below(kiln_common::NodeId(1 << 40));
    d.to_bytes().unwrap()
}

fn deck_model(s: &Session, doc: DocId) -> Vec<u8> {
    let b = s.docs[&doc].model.to_bundle().unwrap();
    let mut p = kiln_deck::Presentation::from_bytes(&b.model).unwrap();
    p.reserve_ids_below(kiln_common::NodeId(1 << 40));
    p.to_bytes().unwrap()
}

#[test]
fn every_kind_of_write_edit_replays_to_the_same_document() {
    use kiln_protocol::commands::EditCommand as E;
    let d = disk();
    lock(&d).files.insert(
        "/docs/Styles.docx".into(),
        fixture("docx/features/styles.docx"),
    );
    let mut a = Run::new(&d);
    let doc = a.open_file(&d, "/docs/Styles.docx");
    let edit = |e: E| Command::Suite(SuiteCommand::Edit(e));
    a.type_text(doc, "Heading words\nBody text that gets formatted\n");
    a.run(
        doc,
        WriteCommand::ApplyStyle {
            style_id: "Heading1".into(),
        }
        .into(),
    );
    a.run(doc, WriteCommand::SelectAll.into());
    a.run(doc, edit(E::Bold));
    a.run(doc, edit(E::AlignCenter));
    a.run(doc, edit(E::Bullets { glyph: None }));
    a.run(
        doc,
        WriteCommand::MoveCaret {
            motion: kiln_protocol::commands::CaretMotion::DocumentEnd,
            extend: false,
        }
        .into(),
    );
    a.run(doc, WriteCommand::DeleteBackward.into());
    a.run(
        doc,
        WriteCommand::InsertTable {
            rows: 2,
            cols: 3,
            autofit: kiln_protocol::commands::TableAutofit::Fixed,
        }
        .into(),
    );
    a.type_text(doc, "in a cell");
    a.run(
        doc,
        WriteCommand::NewComment {
            text: "Check this".into(),
        }
        .into(),
    );
    a.run(doc, WriteCommand::TrackChanges.into());
    a.type_text(doc, " tracked");
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, Command::Suite(SuiteCommand::Redo));
    a.settle();
    let before = write_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(write_model(&b.s, id), before);
    // Saved as Word again, the recovered document holds what was typed.
    b.suite(Some(id), SuiteCommand::Save);
    let text = docx_text(&lock(&d).files["/docs/Styles.docx"]);
    assert!(text.contains("Body text that gets formatted"), "{text}");
}

#[test]
fn every_kind_of_deck_edit_replays_to_the_same_presentation() {
    use kiln_protocol::commands::{DeckDirection, DeckFill};
    let d = disk();
    lock(&d).files.insert(
        "/docs/Shapes.pptx".into(),
        fixture("pptx/features/preset-shapes.pptx"),
    );
    let mut a = Run::new(&d);
    let doc = a.open_file(&d, "/docs/Shapes.pptx");
    let deck = |c: DeckCommand| Command::Deck(c);
    a.run(doc, deck(DeckCommand::NewSlide { layout: None }));
    a.run(doc, deck(DeckCommand::InsertTextBox { rect: None }));
    a.run(
        doc,
        deck(DeckCommand::InsertText {
            text: "Firing schedule".into(),
        }),
    );
    a.run(
        doc,
        deck(DeckCommand::Nudge {
            direction: DeckDirection::Right,
            fine: false,
        }),
    );
    a.run(
        doc,
        deck(DeckCommand::SetFill {
            fill: DeckFill::Solid {
                color: "D93D12".into(),
                brightness: None,
                transparency: None,
            },
        }),
    );
    a.run(doc, deck(DeckCommand::InsertTable { rows: 2, cols: 2 }));
    a.run(doc, deck(DeckCommand::Duplicate));
    a.run(doc, Command::Suite(SuiteCommand::Undo));
    a.run(doc, deck(DeckCommand::Delete));
    a.settle();
    let before = deck_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(deck_model(&b.s, id), before);
}

#[test]
fn a_presentation_saved_as_powerpoint_keeps_a_version_per_save() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Deck).unwrap().doc;
    a.settle();
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("opfs:documents/Pitch.pptx".into()),
            format: Some(FileFormat::Pptx),
        },
    );
    a.run(id, Command::Deck(DeckCommand::NewSlide { layout: None }));
    a.settle();
    a.suite(Some(id), SuiteCommand::Save);
    assert_eq!(versions(&mut a, id).len(), 2);
}

#[test]
fn saves_written_later_each_keep_the_version_they_saved() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "one");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Two.docx".into()),
            format: None,
        },
    );
    // From here the disk is slow: the second save starts before the first is written.
    a.defer_writes.store(true, Ordering::Relaxed);
    let save = |a: &mut Run, writes: &mut Vec<kiln_protocol::RequestId>| {
        a.s.execute(Some(id), Command::Suite(SuiteCommand::Save))
            .unwrap();
        for e in a.s.poll() {
            if let Event::HostRequest(r) = e
                && matches!(r.op, HostOp::WriteFile { .. })
            {
                writes.push(r.id);
            }
        }
    };
    let mut writes = Vec::new();
    a.type_text(id, " two");
    save(&mut a, &mut writes);
    a.type_text(id, " three");
    save(&mut a, &mut writes);
    assert_eq!(writes.len(), 2);
    for w in writes {
        a.s.host_reply(w, HostReply::Done).unwrap();
    }
    a.settle();
    let v = versions(&mut a, id);
    assert_eq!(v.len(), 3, "{v:?}");
    a.s.execute(
        Some(id),
        Command::Suite(SuiteCommand::VersionPreview {
            id: Some(v[1].id.clone()),
        }),
    )
    .unwrap();
    a.settle();
    assert_eq!(
        a.text(id),
        ["one two"],
        "each version is what its save wrote"
    );
}

#[test]
fn an_opened_file_is_checkpointed_before_its_first_edit_and_recovered_from_it() {
    let d = disk();
    lock(&d).files.insert(
        "/docs/Tables.docx".into(),
        fixture("docx/features/tables.docx"),
    );
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Tables.docx");
    // The checkpoint is there, the marker clean: nothing to recover yet.
    let key = only_journal(&d);
    let meta = marker(&d, &key);
    assert!(
        lock(&d)
            .store
            .get(
                StoreArea::Journal,
                &journal::base_key(&key, meta.generation)
            )
            .unwrap()
            .is_some()
    );
    assert!(!meta.dirty);
    // The first edit only appends.
    a.type_text(id, "Edited ");
    assert!(!log_of(&d, &key).1.is_empty());
    let text = a.text(id);
    drop(a);
    let mut c = Run::new(&d);
    let entries = c.recovery();
    assert_eq!(entries[0].path.as_deref(), Some("/docs/Tables.docx"));
    let id = c.recover_only();
    assert_eq!(c.text(id), text);
}

// ----- review fixes: nothing lost to a save race, another program, a close, or a crash -------

/// The deferred `WriteFile` requests among the session's next events.
fn file_writes(a: &mut Run) -> Vec<kiln_protocol::RequestId> {
    a.s.poll()
        .into_iter()
        .filter_map(|e| match e {
            Event::HostRequest(r) if matches!(r.op, HostOp::WriteFile { .. }) => Some(r.id),
            _ => None,
        })
        .collect()
}

#[test]
fn edits_made_while_the_file_is_written_are_recovered_after_a_crash() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "saved ");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Race.docx".into()),
            format: None,
        },
    );
    // A slow disk (the web's writes are asynchronous): typing goes on while Save writes.
    a.defer_writes.store(true, Ordering::Relaxed);
    a.type_text(id, "before the save ");
    a.s.execute(Some(id), Command::Suite(SuiteCommand::Save))
        .unwrap();
    let writes = file_writes(&mut a);
    assert_eq!(writes.len(), 1);
    a.type_text(id, "typed while it was written");
    a.s.host_reply(writes[0], HostReply::Done).unwrap();
    a.settle();
    assert!(a.summary(id).dirty, "the last words are not in the file");
    let text = a.text(id);
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.text(id), text);
}

#[test]
fn others_edits_and_then_this_windows_are_recovered_together() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "mine ");
    a.settle();
    // Another window's edit arrives (collaboration applies it without a journal record).
    a.s.docs
        .get_mut(&id)
        .unwrap()
        .model
        .execute(
            &WriteCommand::InsertText {
                text: "theirs ".into(),
            }
            .into(),
        )
        .unwrap();
    a.s.storage_remote(id);
    // This window types before the checkpoint's second is up; then the crash.
    a.type_text(id, "and mine again");
    let text = a.text(id);
    assert!(text.concat().contains("theirs"));
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert_eq!(b.text(id), text);
}

#[test]
fn autosave_never_writes_over_another_programs_change() {
    let d = disk();
    lock(&d).write("/docs/Plan.docx", fixture("docx/features/tables.docx"));
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    // Another program saves the file (the web cannot watch files: the stamp tells).
    let theirs = fixture("docx/features/styles.docx");
    lock(&d).write("/docs/Plan.docx", theirs.clone());
    a.type_text(id, "Mine ");
    let events = a.settle();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FileChanged { doc } if *doc == id)),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, Event::Saved { .. })),
        "nothing written over their change"
    );
    assert_eq!(lock(&d).files["/docs/Plan.docx"], theirs);
    assert!(
        matches!(a.summary(id).autosave, AutoSave::Unavailable(ref why) if why.contains("paused")),
        "{:?}",
        a.summary(id).autosave
    );
    // More typing writes nothing either until the question is answered.
    a.type_text(id, "still mine ");
    a.settle();
    assert_eq!(lock(&d).files["/docs/Plan.docx"], theirs);
    // Keep my version: AutoSave goes on and writes this window's document.
    let events = a.suite(Some(id), SuiteCommand::KeepMine);
    assert!(
        events.iter().any(|e| matches!(e, Event::Saved { .. })),
        "{events:?}"
    );
    assert_eq!(a.summary(id).autosave, AutoSave::On);
    assert!(docx_text(&lock(&d).files["/docs/Plan.docx"]).contains("Mine still mine"));

    // An explicit Save is held the same way, and goes ahead once the user keeps their version.
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: false });
    let theirs = fixture("docx/features/sections.docx");
    lock(&d).write("/docs/Plan.docx", theirs.clone());
    a.type_text(id, "saved by hand ");
    let events = a.suite(Some(id), SuiteCommand::Save);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FileChanged { .. }))
    );
    assert_eq!(lock(&d).files["/docs/Plan.docx"], theirs);
    let events = a.suite(Some(id), SuiteCommand::KeepMine);
    assert!(
        events.iter().any(|e| matches!(e, Event::Saved { .. })),
        "{events:?}"
    );
    assert!(docx_text(&lock(&d).files["/docs/Plan.docx"]).contains("saved by hand"));
    assert!(!a.summary(id).dirty);
}

#[test]
fn a_save_while_kilns_own_write_is_under_way_is_not_taken_for_another_programs_change() {
    let d = disk();
    lock(&d).write("/docs/Plan.docx", fixture("docx/features/tables.docx"));
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    // A slow disk: writes and stamps come back when the test says.
    a.defer_writes.store(true, Ordering::Relaxed);
    a.defer_stats.store(true, Ordering::Relaxed);
    let mut asked: Vec<kiln_protocol::HostRequest> = Vec::new();
    let pump = |a: &mut Run, asked: &mut Vec<kiln_protocol::HostRequest>| {
        for e in a.s.poll() {
            match e {
                Event::HostRequest(r) => asked.push(r),
                Event::FileChanged { .. } => panic!("Kiln's own write taken for another program's"),
                _ => {}
            }
        }
    };
    a.type_text(id, "AutoSaved ");
    pump(&mut a, &mut asked);
    // AutoSave's timer runs out: its check of the stamp, then its write.
    let timer = asked
        .iter()
        .position(|r| matches!(r.op, HostOp::Timer { .. }))
        .unwrap();
    let timer = asked.remove(timer);
    a.later(2_000);
    a.s.host_reply(timer.id, HostReply::Done).unwrap();
    pump(&mut a, &mut asked);
    let check = asked
        .iter()
        .position(|r| matches!(r.op, HostOp::StatFile { .. }))
        .unwrap();
    let check = asked.remove(check);
    let stamp = lock(&d).stamp("/docs/Plan.docx");
    a.s.host_reply(check.id, stamp).unwrap();
    pump(&mut a, &mut asked);
    let first = asked
        .iter()
        .position(|r| matches!(r.op, HostOp::WriteFile { .. }))
        .unwrap();
    let first = asked.remove(first);
    // Save while that write is under way; then the first write lands.
    a.type_text(id, "and saved ");
    a.s.execute(Some(id), Command::Suite(SuiteCommand::Save))
        .unwrap();
    pump(&mut a, &mut asked);
    if let HostOp::WriteFile { path, bytes } = &first.op {
        lock(&d).write(path, bytes.clone());
    }
    a.s.host_reply(first.id, HostReply::Done).unwrap();
    pump(&mut a, &mut asked);
    // Every stamp and write asked for since is answered as the disk has it.
    // In the order they were asked, as the host answers them.
    for _ in 0..1000 {
        if asked.is_empty() {
            break;
        }
        let r = asked.remove(0);
        let reply = match &r.op {
            HostOp::Timer { ms } => {
                a.later(u64::from(*ms));
                HostReply::Done
            }
            HostOp::StatFile { path } => lock(&d).stamp(path),
            HostOp::WriteFile { path, bytes } => {
                lock(&d).write(path, bytes.clone());
                HostReply::Done
            }
            _ => HostReply::Done,
        };
        a.s.host_reply(r.id, reply).unwrap();
        pump(&mut a, &mut asked);
    }
    assert!(docx_text(&lock(&d).files["/docs/Plan.docx"]).contains("AutoSaved and saved"));
}

#[test]
fn a_change_the_watcher_reports_pauses_autosave_until_the_file_is_reloaded() {
    let d = disk();
    lock(&d).write("/docs/Plan.docx", fixture("docx/features/tables.docx"));
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    let (watch, path) = a.watches.pop().expect("the file is watched");
    assert_eq!(path, "/docs/Plan.docx");
    // The native watcher: another program changed the file.
    let theirs = fixture("docx/features/styles.docx");
    lock(&d).write("/docs/Plan.docx", theirs.clone());
    a.s.host_reply(watch, HostReply::Done).unwrap();
    let events = a.settle();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FileChanged { .. }))
    );
    a.type_text(id, "Mine ");
    a.settle();
    assert_eq!(
        lock(&d).files["/docs/Plan.docx"],
        theirs,
        "AutoSave is paused"
    );
    // Reload: their version, and AutoSave goes on from it.
    a.suite(Some(id), SuiteCommand::Reload);
    assert_eq!(a.summary(id).autosave, AutoSave::On);
    assert!(!a.text(id).concat().contains("Mine"));
    a.type_text(id, "After the reload ");
    a.settle();
    assert!(docx_text(&lock(&d).files["/docs/Plan.docx"]).contains("After the reload"));
}

#[test]
fn close_writes_an_autosaved_document_first_and_keeps_the_recovery_copy_of_unsaved_work() {
    let d = disk();
    lock(&d).write("/docs/Plan.docx", fixture("docx/features/tables.docx"));
    let mut a = Run::new(&d);
    // AutoSave on: Close writes the change first, then closes cleanly.
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    a.type_text(id, "Written on close ");
    let events = a.suite(Some(id), SuiteCommand::Close);
    let saved = events.iter().position(|e| matches!(e, Event::Saved { .. }));
    let closed = events
        .iter()
        .position(|e| matches!(e, Event::Closed { .. }));
    assert!(saved.is_some() && saved < closed, "{events:?}");
    assert!(docx_text(&lock(&d).files["/docs/Plan.docx"]).contains("Written on close"));
    assert!(journal_keys(&d).is_empty());

    // A write under way when Close comes: the document closes once it is written.
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.type_text(id, "Saved by hand ");
    a.defer_writes.store(true, Ordering::Relaxed);
    a.s.execute(Some(id), Command::Suite(SuiteCommand::Save))
        .unwrap();
    let writes = file_writes(&mut a);
    a.s.execute(Some(id), Command::Suite(SuiteCommand::Close))
        .unwrap();
    assert!(
        !a.s.poll().iter().any(|e| matches!(e, Event::Closed { .. })),
        "not before the file is written"
    );
    a.s.host_reply(writes[0], HostReply::Done).unwrap();
    assert!(a.settle().iter().any(|e| matches!(e, Event::Closed { .. })));
    assert!(journal_keys(&d).is_empty());

    // AutoSave on, but the write fails: the document stays open with the error.
    let id = a.open_file(&d, "/docs/Plan.docx");
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: true });
    a.type_text(id, "Not written ");
    a.s.execute(Some(id), Command::Suite(SuiteCommand::Close))
        .unwrap();
    let writes = file_writes(&mut a);
    a.s.host_reply(
        writes[0],
        HostReply::Failed((&kiln_common::KilnError::Io("the disk is full".into())).into()),
    )
    .unwrap();
    let events = a.settle();
    assert!(events.iter().any(|e| matches!(e, Event::Error { .. })));
    assert!(!events.iter().any(|e| matches!(e, Event::Closed { .. })));
    assert!(a.s.docs.contains_key(&id) && a.summary(id).dirty);
    a.defer_writes.store(false, Ordering::Relaxed);

    // AutoSave off and unsaved changes: closed, but the recovery copy stays.
    a.suite(Some(id), SuiteCommand::SetAutoSave { on: false });
    a.suite(Some(id), SuiteCommand::Close);
    assert!(!a.s.docs.contains_key(&id));
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    assert!(b.text(id).concat().contains("Not written"));
}

// ----- Office files saved after a crash or a bundle keep what an ordinary save keeps --------

/// An Office file with custom XML, macros and markup Kiln does not model in parts it rewrites
/// only when they are touched.
fn office_with_extras(
    fixture_name: &str,
    main: &str,
    macro_type: &str,
    extra_in: &[&str],
) -> Vec<u8> {
    use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, rel_type};
    let mut pkg = OpcPackage::open(&fixture(fixture_name)).unwrap();
    pkg.set_content_type(main, macro_type).unwrap();
    let dir = &main[..main.rfind('/').unwrap()];
    pkg.add_part(
        "/customXml/item9.xml",
        "application/xml",
        "<root xmlns=\"urn:kiln-test\"><v>kept</v></root>",
    )
    .unwrap();
    pkg.add_relationship(
        main,
        rel_type::CUSTOM_XML,
        "/customXml/item9.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    let vba = format!("{dir}/vbaProject.bin");
    pkg.add_part(
        &vba,
        "application/vnd.ms-office.vbaProject",
        vec![0xD0, 0xCF, 1, 2, 3],
    )
    .unwrap();
    pkg.add_relationship(
        main,
        rel_type::VBA_PROJECT,
        &vba,
        RelationshipOptions::default(),
    )
    .unwrap();
    for part in extra_in {
        let xml = String::from_utf8(pkg.part(part).unwrap().unwrap().into_owned()).unwrap();
        let close = xml.rfind("</").unwrap();
        let xml = format!(
            "{}<kt:extra xmlns:kt=\"urn:kiln-test\">kept as it was</kt:extra>{}",
            &xml[..close],
            &xml[close..]
        );
        pkg.set_part(part, xml.into_bytes()).unwrap();
    }
    pkg.save().unwrap()
}

/// The parts of `saved` that differ from `original` (and those that are gone).
fn changed_parts(original: &[u8], saved: &[u8]) -> Vec<String> {
    let (a, b) = (
        kiln_ooxml::opc::OpcPackage::open(original).unwrap(),
        kiln_ooxml::opc::OpcPackage::open(saved).unwrap(),
    );
    let mut out: Vec<String> = a
        .all_part_names()
        .filter(|n| a.part(n).unwrap() != b.part(n).ok().flatten())
        .map(str::to_owned)
        .collect();
    out.sort();
    out
}

/// One edit, then three ways to save as Office: straight away, after a crash and recovery,
/// and through a bundle. All three change the same parts, and the original's other parts are
/// byte for byte as they were.
fn saves_like_an_ordinary_save(
    original: &[u8],
    path: &str,
    format: FileFormat,
    edit: &dyn Fn(&mut Run, DocId),
) {
    let ext = format.extension();
    // Saved straight away.
    let d = disk();
    lock(&d).write(path, original.to_vec());
    let mut a = Run::new(&d);
    let id = a.open_file(&d, path);
    edit(&mut a, id);
    a.suite(Some(id), SuiteCommand::Save);
    let ordinary = changed_parts(original, &lock(&d).files[path]);
    drop(a);

    // Saved after a crash and recovery.
    let d = disk();
    lock(&d).write(path, original.to_vec());
    let mut a = Run::new(&d);
    let id = a.open_file(&d, path);
    edit(&mut a, id);
    a.settle();
    drop(a);
    let mut b = Run::new(&d);
    let id = b.recover_only();
    b.suite(Some(id), SuiteCommand::Save);
    let recovered = changed_parts(original, &lock(&d).files[path]);
    assert_eq!(recovered, ordinary, "recovered .{ext}");

    // Saved as a bundle, opened again and saved as Office.
    let d = disk();
    lock(&d).write(path, original.to_vec());
    let mut a = Run::new(&d);
    let id = a.open_file(&d, path);
    edit(&mut a, id);
    let bundle = format!(
        "{path}.bundle.{}",
        match format {
            FileFormat::Docm => "kwrite",
            FileFormat::Xlsm => "kgrid",
            _ => "kdeck",
        }
    );
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some(bundle.clone()),
            format: None,
        },
    );
    drop(a);
    let mut c = Run::new(&d);
    let id = c.open_file(&d, &bundle);
    let back = format!("/docs/back.{ext}");
    c.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some(back.clone()),
            format: Some(format),
        },
    );
    let bundled = changed_parts(original, &lock(&d).files[&back]);
    assert_eq!(bundled, ordinary, "through a bundle, .{ext}");
    // The parts Kiln does not model are among the untouched ones.
    assert!(
        !ordinary
            .iter()
            .any(|p| p.starts_with("/customXml/") || p.ends_with("vbaProject.bin")),
        "{ordinary:?}"
    );
}

#[test]
fn a_recovered_or_bundled_word_document_saves_its_untouched_parts_byte_for_byte() {
    let original = office_with_extras(
        "docx/features/styles.docx",
        "/word/document.xml",
        "application/vnd.ms-word.document.macroEnabled.main+xml",
        &["/word/styles.xml", "/word/settings.xml"],
    );
    saves_like_an_ordinary_save(
        &original,
        "/docs/Report.docm",
        FileFormat::Docm,
        &|a, id| {
            a.type_text(id, "One edited paragraph. ");
        },
    );
    // What an ordinary save rewrites is the body and the properties only.
    let d = disk();
    lock(&d).write("/docs/Report.docm", original.clone());
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Report.docm");
    a.type_text(id, "One edited paragraph. ");
    a.suite(Some(id), SuiteCommand::Save);
    let changed = changed_parts(&original, &lock(&d).files["/docs/Report.docm"]);
    assert!(
        changed
            .iter()
            .all(|p| p == "/word/document.xml" || p.starts_with("/docProps/")),
        "{changed:?}"
    );
}

#[test]
fn a_recovered_or_bundled_workbook_saves_its_untouched_parts_byte_for_byte() {
    let original = office_with_extras(
        "xlsx/features/styles.xlsx",
        "/xl/workbook.xml",
        "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
        &["/xl/styles.xml"],
    );
    saves_like_an_ordinary_save(&original, "/docs/Book.xlsm", FileFormat::Xlsm, &|a, id| {
        a.run(
            id,
            GridCommand::BeginEdit {
                text: "edited".into(),
            }
            .into(),
        );
        a.run(id, GridCommand::CommitEdit { advance: None }.into());
    });
}

#[test]
fn a_recovered_or_bundled_presentation_saves_its_untouched_parts_byte_for_byte() {
    let original = office_with_extras(
        "pptx/features/text-formatting.pptx",
        "/ppt/presentation.xml",
        "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml",
        &["/ppt/presProps.xml"],
    );
    saves_like_an_ordinary_save(&original, "/docs/Talk.pptm", FileFormat::Pptm, &|a, id| {
        a.run(id, Command::Deck(DeckCommand::NewSlide { layout: None }));
    });
}

#[test]
fn a_bundle_whose_original_cannot_be_read_says_what_saving_as_office_loses() {
    let d = disk();
    lock(&d).write("/docs/Tables.docx", fixture("docx/features/tables.docx"));
    let mut a = Run::new(&d);
    let id = a.open_file(&d, "/docs/Tables.docx");
    a.type_text(id, "x");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Tables.kwrite".into()),
            format: None,
        },
    );
    drop(a);
    // The original kept in the bundle is damaged.
    let mut b = kiln_storage::Bundle::from_bytes(&lock(&d).files["/docs/Tables.kwrite"]).unwrap();
    b.passthrough
        .insert("original.docx".into(), b"PK damaged".to_vec());
    lock(&d).write("/docs/Tables.kwrite", b.to_bytes().unwrap());
    let mut c = Run::new(&d);
    let id = c.open_file(&d, "/docs/Tables.kwrite");
    let issues = c.s.docs[&id].model.compat_report(FileFormat::Docx);
    assert!(
        issues.iter().any(|i| i.code == "bundle.original_unreadable"
            && i.severity == kiln_protocol::CompatSeverity::Loss),
        "{issues:?}"
    );
}

// ----- local version history follows the document, never the path alone ---------------------

#[test]
fn versions_follow_the_document_not_the_path() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "Plan one");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Plan.docx".into()),
            format: None,
        },
    );
    a.type_text(id, " grows");
    a.suite(Some(id), SuiteCommand::Save);
    assert_eq!(versions(&mut a, id).len(), 2);
    a.suite(Some(id), SuiteCommand::Close);
    drop(a);

    // Moved by another program: found by its contents.
    let plan = lock(&d).files.remove("/docs/Plan.docx").unwrap();
    lock(&d).write("/elsewhere/Plan.docx", plan.clone());
    let mut b = Run::new(&d);
    let id = b.open_file(&d, "/elsewhere/Plan.docx");
    assert_eq!(versions(&mut b, id).len(), 2, "found after the move");
    b.suite(Some(id), SuiteCommand::Close);

    // Another document at the old path: it starts its own history.
    lock(&d).write("/docs/Plan.docx", fixture("docx/features/tables.docx"));
    let id = b.open_file(&d, "/docs/Plan.docx");
    assert!(
        versions(&mut b, id).is_empty(),
        "never another document's versions"
    );
    b.suite(Some(id), SuiteCommand::Close);

    // Save As takes the history along, every version (also those not read yet).
    let id = b.open_file(&d, "/elsewhere/Plan.docx");
    b.type_text(id, " more");
    b.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Renamed.docx".into()),
            format: None,
        },
    );
    assert_eq!(versions(&mut b, id).len(), 3);
    b.suite(Some(id), SuiteCommand::Close);
    drop(b);
    let mut c = Run::new(&d);
    let id = c.open_file(&d, "/docs/Renamed.docx");
    let v = versions(&mut c, id);
    assert_eq!(v.len(), 3);
    c.suite(
        Some(id),
        SuiteCommand::VersionPreview {
            id: Some(v[0].id.clone()),
        },
    );
    assert_eq!(c.text(id), ["Plan one"], "the oldest version reads back");
}

#[test]
fn the_histories_of_files_that_are_gone_are_removed_and_the_total_is_capped() {
    let d = disk();
    let mut a = Run::new(&d);
    let id = a.s.new_document(DocKind::Write).unwrap().doc;
    a.type_text(id, "Gone soon");
    a.suite(
        Some(id),
        SuiteCommand::SaveAs {
            path: Some("/docs/Gone.docx".into()),
            format: None,
        },
    );
    a.suite(Some(id), SuiteCommand::Close);
    drop(a);
    let has_versions = |d: &Arc<Mutex<Disk>>| {
        lock(d)
            .store
            .list(StoreArea::Versions, "h/")
            .unwrap()
            .iter()
            .any(|k| !k.ends_with("/index"))
    };
    assert!(has_versions(&d));
    // The file is deleted. A start soon after keeps its history (the file may have moved);
    // one a month later removes it.
    lock(&d).files.remove("/docs/Gone.docx");
    let mut b = Run::new(&d);
    b.settle();
    assert!(has_versions(&d));
    drop(b);
    let month = kiln_storage::versions::GONE_AFTER_MS;
    let mut c = Run::at(&d, 1_792_000_000_000 + month);
    c.settle();
    assert!(
        lock(&d)
            .store
            .list(StoreArea::Versions, "")
            .unwrap()
            .is_empty(),
        "{:?}",
        lock(&d).store.list(StoreArea::Versions, "")
    );
}

#[test]
fn version_histories_are_kept_within_their_total_across_files() {
    let d = disk();
    // A store already over its total: two files' histories of large versions.
    {
        use kiln_storage::versions::{
            HistoryIndex, INDEX_FORMAT, PathRecord, index_key, model_key, path_key,
        };
        let mut disk = lock(&d);
        for (h, file, n) in [("ha", "/docs/A.docx", 5u64), ("hb", "/docs/B.docx", 5)] {
            let versions: Vec<kiln_protocol::collab::VersionEntry> = (0..n)
                .map(|i| kiln_protocol::collab::VersionEntry {
                    id: format!("v{i}"),
                    name: (i == 0).then(|| "Named".to_owned()),
                    author: "You".into(),
                    time_ms: i * 10 + u64::from(h == "hb"),
                    origin: VersionOrigin::Save,
                })
                .collect();
            let size = kiln_storage::versions::TOTAL_BYTES / 6;
            let index = HistoryIndex {
                format: INDEX_FORMAT,
                sizes: versions.iter().map(|v| (v.id.clone(), size)).collect(),
                versions,
                file: Some(file.into()),
                print: Some("p".into()),
                used_ms: 1,
            };
            disk.store
                .put(
                    StoreArea::Versions,
                    &index_key(h),
                    &index.to_bytes().unwrap(),
                )
                .unwrap();
            for v in &index.versions {
                disk.store
                    .put(StoreArea::Versions, &model_key(h, &v.id), b"model")
                    .unwrap();
            }
            let record = PathRecord {
                path: file.into(),
                history: h.into(),
                print: "p".into(),
            };
            disk.store
                .put(StoreArea::Versions, &path_key(file), &record.to_bytes())
                .unwrap();
            disk.write(file, b"exists".to_vec());
        }
    }
    let mut a = Run::new(&d);
    a.settle();
    let left = lock(&d).store.list(StoreArea::Versions, "h/").unwrap();
    let models: Vec<&String> = left.iter().filter(|k| !k.ends_with("/index")).collect();
    // Ten versions of a sixth of the total each: six stay, the oldest unnamed went first.
    assert_eq!(models.len(), 6, "{left:?}");
    for kept in ["h/ha/v0", "h/hb/v0", "h/ha/v4", "h/hb/v4"] {
        assert!(
            left.iter().any(|k| k == kept),
            "{kept} (named or latest): {left:?}"
        );
    }
    assert!(!left.iter().any(|k| k == "h/ha/v1" || k == "h/hb/v1"));
}

// ----- two Kiln processes on one app storage --------------------------------------------------

/// Answers the timers of a session on a native host.
fn settle_native(s: &mut Session) -> Vec<Event> {
    let mut out = Vec::new();
    for _ in 0..1000 {
        let events = s.poll();
        if events.is_empty() {
            return out;
        }
        for e in events {
            match e {
                Event::HostRequest(r) => {
                    if let HostOp::Timer { .. } = r.op {
                        let _ = s.host_reply(r.id, HostReply::Done);
                    }
                }
                other => out.push(other),
            }
        }
    }
    panic!("the session never settled");
}

fn native_session(dir: &std::path::Path) -> Session {
    Session::new(Box::new(
        crate::StoredHost::open(Box::new(crate::NativeHost), dir).unwrap(),
    ))
}

fn recovery_of(s: &mut Session) -> Vec<RecoveryEntry> {
    s.execute(None, Command::Suite(SuiteCommand::RecoveryScan))
        .unwrap();
    settle_native(s)
        .into_iter()
        .rev()
        .find_map(|e| match e {
            Event::Recovery { entries } => Some(entries),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_second_kiln_process_keeps_storage_and_never_takes_the_firsts_journal() {
    let dir = std::env::temp_dir().join(format!("kiln-two-processes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut a = native_session(&dir);
    let doc = a.new_document(DocKind::Write).unwrap().doc;
    a.execute(
        Some(doc),
        WriteCommand::InsertText {
            text: "first process".into(),
        }
        .into(),
    )
    .unwrap();
    settle_native(&mut a);
    // The second process has storage too (its own documents are journaled) ...
    let mut b = native_session(&dir);
    let own = b.new_document(DocKind::Write).unwrap().doc;
    b.execute(
        Some(own),
        WriteCommand::InsertText {
            text: "second".into(),
        }
        .into(),
    )
    .unwrap();
    settle_native(&mut b);
    // ... but the first one's open document is not its to recover or delete.
    assert!(recovery_of(&mut b).is_empty());
    let key = a.storage.docs[&doc].journal.key().to_owned();
    for cmd in [
        SuiteCommand::RecoveryOpen {
            key: key.clone(),
            kind: DocKind::Write,
        },
        SuiteCommand::RecoveryDelete { key: key.clone() },
    ] {
        b.execute(None, Command::Suite(cmd)).unwrap();
        let events = settle_native(&mut b);
        assert!(
            events.iter().any(|e| matches!(e, Event::Error { error, .. }
                if error.message.contains("open in another window"))),
            "{events:?}"
        );
        assert!(!events.iter().any(|e| matches!(e, Event::Opened { .. })));
    }
    // The first process crashes: the second recovers its document, and a third cannot take it.
    drop(a);
    let entries = recovery_of(&mut b);
    assert_eq!(entries.len(), 1, "{entries:?}");
    b.execute(
        None,
        Command::Suite(SuiteCommand::RecoveryOpen {
            key: key.clone(),
            kind: DocKind::Write,
        }),
    )
    .unwrap();
    let opened = settle_native(&mut b)
        .into_iter()
        .find_map(|e| match e {
            Event::Opened { summary } => Some(summary.doc),
            _ => None,
        })
        .unwrap();
    match b.query(Some(opened), &Query::Paragraphs) {
        Ok(QueryResult::Paragraphs(p)) => assert_eq!(p[0].text, "first process"),
        other => panic!("{other:?}"),
    }
    let mut c = native_session(&dir);
    assert!(recovery_of(&mut c).is_empty());
    drop((b, c));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- The third review's cases ----
fn rv3_draft_on(name: &str) -> (String, String, String) {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "5");
    a.run(doc, G::InsertSheet.into());
    a.run(
        doc,
        G::RenameSheet {
            index: 1,
            name: name.into(),
        }
        .into(),
    );
    a.run(doc, G::ActivateSheet { index: 1 }.into());
    select_cell(&mut a, doc, 3, 2);
    enter_cell(&mut a, doc, "on two");
    select_cell(&mut a, doc, 6, 4);
    a.run(doc, G::BeginEdit { text: "42".into() }.into());
    a.settle();
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let draft = note.and_then(|n| n.draft).expect("named");
    // Put it back from the first sheet, as the button does.
    b.run(id, G::ActivateSheet { index: 0 }.into());
    let go = b.s.execute(
        Some(id),
        G::GoTo {
            reference: draft.cell.clone(),
            define: false,
        }
        .into(),
    );
    b.run(
        id,
        G::BeginEdit {
            text: draft.text.clone(),
        }
        .into(),
    );
    b.run(id, G::CommitEdit { advance: None }.into());
    b.run(id, G::ActivateSheet { index: 1 }.into());
    let two = csv_of(&b, id);
    (draft.cell, format!("{go:?}"), two)
}

#[test]
fn rv3_a_draft_on_sheets_with_awkward_names_is_named_and_goes_back() {
    for name in ["Q1 'plan' x", "الربع الأول", "Übersicht 2026", "A1", "R1C1"] {
        let (cell, go, two) = rv3_draft_on(name);
        println!("RV3DRAFT {name:?} -> {cell:?} go={go} sheet2={two:?}");
        let lines: Vec<&str> = two.lines().collect();
        let row7 = lines.get(6).copied().unwrap_or("");
        assert!(
            row7.split(',').nth(4) == Some("42"),
            "{name}: put back went elsewhere: {two:?} ({cell})"
        );
    }
}

fn rv3_meta_of(s: &Session, doc: DocId) -> String {
    let b = s.docs[&doc].model.to_bundle().unwrap();
    let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
    let m = wb.sheets()[0].meta();
    format!(
        "merges {:?} notes {:?}",
        m.merges,
        m.notes
            .iter()
            .map(|n| (n.at.row, n.at.col))
            .collect::<Vec<_>>()
    )
}

#[test]
fn rv3_a_paste_of_cells_with_a_formula_keeps_its_merges_and_notes() {
    use kiln_protocol::commands::EditCommand as E;
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    enter_cell(&mut a, doc, "x");
    select_range(&mut a, doc, 0, 0, 0, 1);
    a.run(doc, G::MergeCells.into());
    select_cell(&mut a, doc, 1, 1);
    a.run(
        doc,
        G::SetNote {
            text: Some("a note".into()),
            author: "R".into(),
            cell: None,
        }
        .into(),
    );
    select_cell(&mut a, doc, 1, 3);
    enter_cell(&mut a, doc, "=1+1");
    a.settle();
    select_range(&mut a, doc, 0, 0, 4, 3);
    a.run(doc, Command::Suite(SuiteCommand::Edit(E::Copy)));
    let mut text = String::new();
    for _ in 0..50 {
        let ev = a.s.poll();
        if ev.is_empty() {
            break;
        }
        for e in ev {
            if let Event::HostRequest(r) = e {
                if let HostOp::ClipboardWrite { text: t, .. } = &r.op {
                    text = t.clone();
                }
                if let HostOp::Timer { ms } = &r.op {
                    a.later(u64::from(*ms));
                }
                let _ = a.s.host_reply(r.id, HostReply::Done);
            }
        }
    }
    assert!(!text.is_empty(), "no copy");
    select_cell(&mut a, doc, 0, 5);
    a.run(
        doc,
        Command::Suite(SuiteCommand::Edit(E::Paste {
            data: Some(kiln_protocol::commands::ClipboardData {
                text,
                html: None,
                kiln: None,
            }),
        })),
    );
    a.settle();
    let live = format!("{} csv {:?}", rv3_meta_of(&a.s, doc), csv_of(&a, doc));
    let model = grid_model(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let back = format!("{} csv {:?}", rv3_meta_of(&b.s, id), csv_of(&b, id));
    println!("RV3PASTE live: {live}\nRV3PASTE back: {back}\nnote {note:?}");
    assert_eq!(back, live);
    assert!(grid_model(&b.s, id) == model, "model differs");
}

fn rv3_cf(s: &Session, d: DocId) -> String {
    let bb = s.docs[&d].model.to_bundle().unwrap();
    let wb = kiln_sheet::Workbook::from_bytes(&bb.model, Default::default()).unwrap();
    let st = wb.styles();
    let cfs: Vec<String> = wb
        .sheets()
        .iter()
        .map(|sh| format!("{:?}", sh.meta().cf))
        .collect();
    format!(
        "dxfs {} {:?} cf {:?}",
        st.dxf_count(),
        (0..st.dxf_count() as u32)
            .map(|i| format!("{:?}", st.dxf(i)))
            .collect::<Vec<_>>(),
        cfs
    )
}

#[test]
fn rv3_the_first_edit_after_a_file_opens_is_a_new_conditional_format() {
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfStyle, GridCompare,
    };
    let d = disk();
    lock(&d).write("/docs/Budget.xlsx", fixture("xlsx/features/formulas.xlsx"));
    let mut a = Run::new(&d);
    let g = a.open_file(&d, "/docs/Budget.xlsx");
    a.later(5000);
    a.settle();
    let rule = GridCfRule {
        id: 0,
        applies_to: "$A$1:$D$20".into(),
        kind: GridCfRuleKind::CellValue {
            op: GridCompare::GreaterThan,
            values: vec!["1".into()],
        },
        style: Some(GridCfStyle {
            fill: Some("FF0000".into()),
            bold: true,
            ..Default::default()
        }),
        stop_if_true: false,
    };
    a.run(g, G::AddConditionalFormat { rule }.into());
    a.settle();
    let model = grid_model(&a.s, g);
    let live = rv3_cf(&a.s, g);
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    let events = b.suite(
        None,
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
        .unwrap();
    let note = events.iter().find_map(|e| match e {
        Event::Recovered { note, .. } => Some(note.clone()),
        _ => None,
    });
    let errs: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error { error, .. } => Some(error.message.clone()),
            _ => None,
        })
        .collect();
    println!("RV3CF errors {errs:?}");
    let back = rv3_cf(&b.s, id);
    println!("RV3CF note {note:?}\nRV3CF live {live}\nRV3CF back {back}");
    // A second edit with a format after recovery, then crash again.
    assert!(
        note.as_ref().and_then(|n| n.stopped.as_ref()).is_none(),
        "stopped: {note:?}"
    );
    assert_eq!(back, live);
    assert!(
        grid_model(&b.s, id) == model,
        "model differs after recovery"
    );
}

#[test]
fn rv3_cfv_select_first() {
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfStyle, GridCompare,
    };
    let d = disk();
    lock(&d).write("/docs/Budget.xlsx", fixture("xlsx/features/formulas.xlsx"));
    let mut a = Run::new(&d);
    let g = a.open_file(&d, "/docs/Budget.xlsx");
    a.later(5000);
    a.settle();
    let rule = GridCfRule {
        id: 0,
        applies_to: "$A$1:$D$20".into(),
        kind: GridCfRuleKind::CellValue {
            op: GridCompare::GreaterThan,
            values: vec!["1".into()],
        },
        style: Some(GridCfStyle {
            fill: Some("FF0000".into()),
            bold: true,
            ..Default::default()
        }),
        stop_if_true: false,
    };
    select_cell(&mut a, g, 2, 2);
    a.settle();
    a.run(g, G::AddConditionalFormat { rule }.into());
    a.settle();
    let model = grid_model(&a.s, g);
    let live = rv3_cf(&a.s, g);
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    let events = b.suite(
        None,
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
        .unwrap();
    let note = events.iter().find_map(|e| match e {
        Event::Recovered { note, .. } => Some(note.clone()),
        _ => None,
    });
    let errs: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error { error, .. } => Some(error.message.clone()),
            _ => None,
        })
        .collect();
    println!("RV3CF errors {errs:?}");
    let back = rv3_cf(&b.s, id);
    println!("RV3CF note {note:?}\nRV3CF live {live}\nRV3CF back {back}");
    // A second edit with a format after recovery, then crash again.
    assert!(
        note.as_ref().and_then(|n| n.stopped.as_ref()).is_none(),
        "stopped: {note:?}"
    );
    assert_eq!(back, live);
    assert!(
        grid_model(&b.s, id) == model,
        "model differs after recovery"
    );
}

#[test]
fn rv3_cfv_new_document() {
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfStyle, GridCompare,
    };
    let d = disk();
    let mut a = Run::new(&d);
    let g = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.later(5000);
    a.settle();
    let rule = GridCfRule {
        id: 0,
        applies_to: "$A$1:$D$20".into(),
        kind: GridCfRuleKind::CellValue {
            op: GridCompare::GreaterThan,
            values: vec!["1".into()],
        },
        style: Some(GridCfStyle {
            fill: Some("FF0000".into()),
            bold: true,
            ..Default::default()
        }),
        stop_if_true: false,
    };
    a.run(g, G::AddConditionalFormat { rule }.into());
    a.settle();
    let model = grid_model(&a.s, g);
    let live = rv3_cf(&a.s, g);
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    let events = b.suite(
        None,
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
        .unwrap();
    let note = events.iter().find_map(|e| match e {
        Event::Recovered { note, .. } => Some(note.clone()),
        _ => None,
    });
    let errs: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error { error, .. } => Some(error.message.clone()),
            _ => None,
        })
        .collect();
    println!("RV3CF errors {errs:?}");
    let back = rv3_cf(&b.s, id);
    println!("RV3CF note {note:?}\nRV3CF live {live}\nRV3CF back {back}");
    // A second edit with a format after recovery, then crash again.
    assert!(
        note.as_ref().and_then(|n| n.stopped.as_ref()).is_none(),
        "stopped: {note:?}"
    );
    assert_eq!(back, live);
    assert!(
        grid_model(&b.s, id) == model,
        "model differs after recovery"
    );
}

#[test]
fn rv3_a_redo_that_changes_nothing_keeps_the_edits_after_it() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    for r in 0..6u32 {
        select_cell(&mut a, doc, r, 0);
        enter_cell(&mut a, doc, "=INT(RAND()*2)");
        select_cell(&mut a, doc, r, 1);
        enter_cell(&mut a, doc, &format!("row{r}"));
    }
    a.settle();
    land_checkpoint(&mut a);
    a.settle();
    let mut tries = 0;
    let mut hit = false;
    while tries < 200 {
        tries += 1;
        select_range(&mut a, doc, 0, 0, 5, 1);
        a.run(doc, G::SortAscending.into());
        a.settle();
        a.suite(Some(doc), SuiteCommand::Undo);
        a.suite(Some(doc), SuiteCommand::Redo);
        if !a.held.is_empty() {
            hit = true;
            break;
        }
        land_checkpoint(&mut a);
        a.settle();
    }
    println!("RV3NOOPREDO hit {hit} after {tries} tries");
    select_cell(&mut a, doc, 0, 3);
    enter_cell(&mut a, doc, "typed after");
    a.settle();
    let live = csv_of(&a, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let back = csv_of(&b, id);
    println!("RV3NOOPREDO note {note:?}\nlive {live:?}\nback {back:?}");
    assert!(
        back.contains("typed after"),
        "the edit after the redo is gone"
    );
}

// ---- The fourth round ----

/// A recovered entry on a sheet that is renamed after recovery, and one on a sheet deleted
/// since: Put it back finds the first by its id, and says the second is gone.
#[test]
fn rv4_put_it_back_finds_the_sheet_by_its_id_after_a_rename() {
    let d = disk();
    let mut a = Run::new(&d);
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    a.run(doc, G::InsertSheet.into());
    a.run(doc, G::ActivateSheet { index: 1 }.into());
    select_cell(&mut a, doc, 6, 4);
    a.run(doc, G::BeginEdit { text: "42".into() }.into());
    a.settle();
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    let draft = note.and_then(|n| n.draft).expect("named");
    assert_eq!(draft.cell, "Sheet2!E7");
    b.run(
        id,
        G::RenameSheet {
            index: 1,
            name: "Renamed since".into(),
        }
        .into(),
    );
    b.run(id, G::MoveSheet { from: 1, to: 0 }.into());
    b.run(id, G::ActivateSheet { index: 1 }.into());
    let put = G::PutBackDraft {
        sheet: draft.sheet,
        row: draft.row,
        col: draft.col,
        text: draft.text.clone(),
    };
    b.run(id, put.clone().into());
    let state = grid_state(&b, id);
    assert_eq!(state.active_sheet, 0, "the renamed sheet, moved first");
    assert!(
        state.editor.is_some(),
        "the text is in the editor, not entered"
    );
    b.run(id, G::CommitEdit { advance: None }.into());
    let lines: Vec<String> = csv_of(&b, id).lines().map(str::to_owned).collect();
    assert_eq!(lines.get(6).and_then(|l| l.split(',').nth(4)), Some("42"));
    // The sheet deleted: a plain refusal the page puts in words, nothing typed anywhere.
    b.run(id, G::DeleteSheet { index: 0 }.into());
    let gone = b.s.execute(Some(id), put.into());
    assert!(
        matches!(&gone, Err(kiln_common::KilnError::NotFound { .. })),
        "{gone:?}"
    );
    assert!(grid_state(&b, id).editor.is_none());
}

#[test]
fn rv4_a_draft_on_a_sheet_named_like_a_reference_is_named_in_quotes() {
    for (name, want) in [
        ("A1", "'A1'!E7"),
        ("R1C1", "'R1C1'!E7"),
        ("r2c", "'r2c'!E7"),
        ("XFD1", "'XFD1'!E7"),
        ("Plan", "Plan!E7"),
    ] {
        let d = disk();
        let mut a = Run::new(&d);
        let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
        a.settle();
        a.run(
            doc,
            G::RenameSheet {
                index: 0,
                name: name.into(),
            }
            .into(),
        );
        select_cell(&mut a, doc, 6, 4);
        a.run(doc, G::BeginEdit { text: "7".into() }.into());
        a.settle();
        drop(a);
        let mut b = Run::new(&d);
        let (_, note) = recover_with_note(&mut b);
        let draft = note.and_then(|n| n.draft).expect("named");
        assert_eq!(draft.cell, want, "{name}");
    }
}

/// A record that does not apply: the document opens as it was before it, the note names it as
/// a change Kiln could not repeat and counts the ones after it, and no error text goes out.
#[test]
fn rv4_a_change_that_does_not_replay_is_named_not_raised() {
    use crate::apps::grid::ledger::plant::{self, Plant};
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    for (row, v) in ["a", "b", "c", "d"].iter().enumerate() {
        select_cell(&mut a, doc, row as u32 + 1, 0);
        enter_cell(&mut a, doc, v);
    }
    a.settle();
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    plant::set(Some(Plant::ReplayFails));
    let events = b.suite(
        None,
        SuiteCommand::RecoveryOpen {
            key: entries[0].key.clone(),
            kind: entries[0].kind,
        },
    );
    plant::set(None);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Error { .. })),
        "{events:?}"
    );
    let stop = events
        .iter()
        .find_map(|e| match e {
            Event::Recovered { note, .. } => note.stopped.clone(),
            _ => None,
        })
        .expect("a note");
    println!("RV4STOP {stop:?}");
    assert_eq!(
        stop.command.as_deref(),
        Some(crate::storage::recovery::UNREPEATABLE)
    );
    assert!(stop.after > 0);
}

/// A table made over a header row of volatile formulas: its column names are what the header
/// showed, and recovery gives the same names (the record holds them; nothing is worked out
/// again).
#[test]
fn rv4_a_table_whose_headers_are_volatile_formulas_comes_back_with_its_names() {
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    for (col, v) in ["=NOW()", "=RAND()", "Plain"].iter().enumerate() {
        select_cell(&mut a, doc, 0, col as u32);
        enter_cell(&mut a, doc, v);
        for row in 1..4 {
            select_cell(&mut a, doc, row, col as u32);
            enter_cell(&mut a, doc, &format!("{}", row * 10 + col as u32));
        }
    }
    select_range(&mut a, doc, 0, 0, 3, 2);
    a.run(
        doc,
        G::CreateTable {
            range: None,
            header: true,
            style: "TableStyleMedium2".into(),
        }
        .into(),
    );
    a.settle();
    let names = |s: &Session, id: DocId| {
        let b = s.docs[&id].model.to_bundle().unwrap();
        let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
        let t = &wb.sheets()[0].meta().tables[0];
        let header: Vec<String> = (0..3)
            .map(|c| {
                let r = wb.record(wb.sheets()[0].id(), 0, c);
                format!("{:?} {:?}", r.formula, r.value)
            })
            .collect();
        format!(
            "{:?} {header:?}",
            t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
        )
    };
    let live = names(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, _) = recover_with_note(&mut b);
    let back = names(&b.s, id);
    println!("RV4TABLE live {live}\nRV4TABLE back {back}");
    assert_eq!(live, back);
}

/// A chart that came with the file, moved and resized: recovery puts it where it was put.
#[test]
fn rv4_a_chart_from_the_file_moved_comes_back_where_it_was_put() {
    let d = disk();
    let mut a = Run::new(&d);
    a.s.storage.inline_bytes = Some(0);
    lock(&d).write("/docs/Charts.xlsx", fixture("xlsx/features/charts.xlsx"));
    let doc = a.open_file(&d, "/docs/Charts.xlsx");
    land_checkpoint(&mut a);
    a.settle();
    let anchors = |s: &Session, id: DocId| {
        let b = s.docs[&id].model.to_bundle().unwrap();
        let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
        format!("{:?}", wb.sheets()[0].meta().drawings)
    };
    let before = anchors(&a.s, doc);
    lock(&d).hold_checkpoints = true;
    a.run(doc, G::SelectDrawing { index: Some(0) }.into());
    a.run(
        doc,
        G::PlaceDrawing {
            index: 0,
            x: 333.0,
            y: 77.0,
            width: 410.0,
            height: 250.0,
        }
        .into(),
    );
    a.settle();
    let live = anchors(&a.s, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, _) = recover_with_note(&mut b);
    let back = anchors(&b.s, id);
    assert_ne!(before, live, "the chart moved");
    assert_eq!(live, back);
}
