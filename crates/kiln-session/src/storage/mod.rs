// SPDX-License-Identifier: AGPL-3.0-or-later
//! Storage in the session: the autosave journal, Document Recovery,
//! AutoSave to file, native bundles, recents, local version history, the user dictionary and
//! files changed by other programs.
//!
//! Everything goes through the host's app storage (`HostOp::Store*`; natively `redb`, on the
//! web the Origin Private File System), so it works the same with hosts that answer at once
//! and hosts that answer later. A session keeps storage only when its host has it
//! ([`crate::Host::has_storage`]); a session without it (the command line, most tests) never
//! asks.
//!
//! - **Journal** (`journal.rs`): the first change to a document writes a checkpoint (the
//!   document as a bundle) and the dirty marker; every command after that appends the
//!   operations it applied (Write and Deck transactions, Kiln Grid's sheet commands); every 200
//!   operations, and whenever a change has no operations (an undo in Kiln Grid, a chart's content, a few Deck settings;
//!   others' edits to a shared document, a second later), a new checkpoint takes over from the
//!   log. Nothing about an edit waits: its journal writes leave with the command that made it. The
//!   text of a cell still being typed goes beside the journal as its draft, and Document Recovery
//!   opens the cell again with it. A write to the app storage that fails is reported and followed
//!   by a checkpoint, so the journal never goes on from a lost record.
//!   Saving clears the marker when nothing changed while the file was written; closing a
//!   document that has no unsaved changes, or closing it with Don't Save, removes the journal.
//! - **Recovery** (`recovery.rs`): `suite.file.recovery_scan` lists the journals whose marker
//!   is dirty and that no open window holds (`Event::Recovery`); opening one replays it into a
//!   new window, marked as changed, and writes a checkpoint of it at once; deleting one removes
//!   it. A journal another window or process opened in the meantime is refused, and the list
//!   is read again.
//! - **AutoSave** (`autosave.rs`): with the switch on, the file is written two seconds after the
//!   last edit, as Save does, unless saving in its format would lose something (then the switch
//!   cannot be turned on, or turns itself off with a message). Before Kiln writes its own file
//!   it compares the file's stamp with the one it last saw; a file another program changed is
//!   not written over: AutoSave pauses and the changed-file question is asked.
//! - **Versions** (`versions.rs`): documents that are not shared keep versions on every save and
//!   every ten minutes of editing, in their bundle or, for other files, in the app storage; the
//!   Version History pane lists, shows, names, compares and restores them as it does for shared
//!   documents.
//! - **Recents** (`recents.rs`): opened and saved files, with a thumbnail of the first page or
//!   slide made with `kiln-raster` on save; `Query::Recents`.

mod autosave;
#[cfg(not(target_arch = "wasm32"))]
mod host;
mod journal;
mod recents;
mod recovery;
#[cfg(test)]
mod tests;
mod versions;

#[cfg(not(target_arch = "wasm32"))]
pub use host::StoredHost;

/// What a recovery holds as it replays a journal ([`crate::DocModel::apply_journal`]): only the
/// storage modules make one, so a document's replay entry is not open to its commands.
#[derive(Debug)]
pub struct JournalReplay(pub(in crate::storage) ());
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use host::stat;

use std::collections::BTreeMap;

use kiln_common::{DocId, KilnError};
use kiln_protocol::{ErrorInfo, Event, FileFormat, FileStamp, HostOp, HostReply, StoreArea};
use kiln_storage::journal::Journal;
use kiln_storage::recents::Recents;
use kiln_storage::versions::LocalHistory;

use crate::model::DocModel;
use crate::session::{Pending, Session};
use crate::undo::StateId;

/// The author of versions kept on this device.
pub(crate) const AUTHOR: &str = "You";

/// The session's storage state.
#[derive(Default)]
pub(crate) struct Storage {
    /// The host keeps app storage.
    pub(crate) enabled: bool,
    /// The command being executed (its id names a change no record can hold).
    pub(crate) changing: Option<String>,
    /// Tests: the largest checkpoint written as one run of writes.
    #[cfg(test)]
    pub(crate) inline_bytes: Option<usize>,
    /// The open documents' journals, versions and AutoSave.
    pub(crate) docs: BTreeMap<DocId, DocStore>,
    /// The recent files, once read.
    pub(crate) recents: Option<Recents>,
    recents_loading: bool,
    /// Changes waiting for the recent files to be read.
    recents_waiting: Vec<recents::Change>,
    /// The user dictionary, once read.
    pub(crate) dictionary: Option<Vec<String>>,
    gathers: BTreeMap<u32, Gather>,
    next_gather: u32,
    /// A write to the app storage that belongs to no document failed, and was reported (until
    /// one succeeds again).
    failing: bool,
    /// The fingerprint of the file being opened (it finds the file's version history).
    pub(crate) opening: Option<String>,
    /// Options ▸ Save: AutoSave's wait after the last edit (`None`: two seconds).
    pub(crate) autosave_delay_ms: Option<u64>,
    /// Options ▸ Save ▸ Keep AutoRecover information off: documents opened from now on
    /// keep no journal.
    pub(crate) no_recovery: bool,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("enabled", &self.enabled)
            .field("docs", &self.docs.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Storage {
    /// The storage of a session whose host keeps app storage (`enabled`) or not.
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// Whether a document shows a version in its place (it takes no edits).
    pub(crate) fn locks(&self, doc: DocId) -> bool {
        self.docs.get(&doc).is_some_and(|s| s.preview.is_some())
    }

    /// The document's own model while a version is shown in its place.
    pub(crate) fn live(&self, doc: DocId) -> Option<&dyn DocModel> {
        self.docs
            .get(&doc)
            .and_then(|s| s.preview.as_ref())
            .map(|p| p.live.as_ref())
    }

    /// Whether AutoSave is on for a document.
    pub(crate) fn autosave(&self, doc: DocId) -> bool {
        self.docs.get(&doc).is_some_and(|s| s.autosave)
    }

    /// Kiln writes the document's file: the stamp it knew is no longer the file's (the write's
    /// own stamp follows once it is done).
    pub(crate) fn forget_stamp(&mut self, doc: DocId) {
        if let Some(st) = self.docs.get_mut(&doc) {
            st.stamp = None;
        }
    }

    /// Whether a document waits to be closed once its file is written.
    pub(crate) fn closing(&self, doc: DocId) -> bool {
        self.docs.get(&doc).is_some_and(|s| s.closing)
    }
}

/// What the session keeps for one open document.
pub(crate) struct DocStore {
    pub(crate) journal: Journal,
    /// the journal is kept (Options ▸ Save ▸ Keep AutoRecover information, as it was when
    /// the document opened).
    pub(crate) recovery: bool,
    /// The model's state the journal has.
    journaled: StateId,
    /// A checkpoint is due (others' edits, a failed write, or one asked for while another was
    /// under way).
    checkpoint_wanted: bool,
    /// The checkpoint that is due carries a change without operations.
    unkept_wanted: bool,
    /// The checkpoint timer runs.
    snapshot_timer: bool,
    /// AutoSave to file.
    pub(crate) autosave: bool,
    autosave_timer: bool,
    /// When the document last changed.
    last_edit_ms: u64,
    /// Local version history (documents that are not shared).
    pub(crate) history: LocalHistory,
    /// The history in the app storage (`None`: kept in memory until the document is saved to
    /// a file, or in its bundle).
    history_id: Option<String>,
    /// The size of each version's model.
    sizes: BTreeMap<String, u64>,
    /// The fingerprint of the document's file as Kiln last opened or wrote it.
    print: Option<String>,
    /// The path and fingerprint the history's records were last written for.
    recorded: Option<(String, Option<String>)>,
    /// The models of versions read so far (all of them for a bundle).
    pub(crate) models: BTreeMap<String, Vec<u8>>,
    /// A version shown in place of the document.
    pub(crate) preview: Option<Preview>,
    /// The file watched for changes by other programs.
    watched: Option<String>,
    /// The stamp of the document's file as Kiln last saw it (opened, read or written).
    stamp: Option<FileStamp>,
    /// Another program changed the file: nothing is written to it until the question is
    /// answered (Reload, or Keep my version).
    paused: bool,
    /// The other program's stamp, when a check before a write found it.
    theirs: Option<FileStamp>,
    /// An explicit save waits for the answer.
    save_waiting: bool,
    /// The document closes once its file is written (Close with AutoSave on).
    pub(crate) closing: bool,
    /// A write of this document's journal failed and was reported (until one succeeds again).
    failing: bool,
}

impl DocStore {
    fn new(journal: Journal, state: StateId) -> Self {
        Self {
            journal,
            recovery: true,
            journaled: state,
            checkpoint_wanted: false,
            unkept_wanted: false,
            snapshot_timer: false,
            autosave: false,
            autosave_timer: false,
            last_edit_ms: 0,
            history: LocalHistory::default(),
            history_id: None,
            sizes: BTreeMap::new(),
            print: None,
            recorded: None,
            models: BTreeMap::new(),
            preview: None,
            watched: None,
            stamp: None,
            paused: false,
            theirs: None,
            save_waiting: false,
            closing: false,
            failing: false,
        }
    }
}

/// A version shown read-only in place of the document.
pub(crate) struct Preview {
    /// The version.
    pub(crate) id: String,
    /// The document's own model, back when the preview ends.
    pub(crate) live: Box<dyn DocModel>,
}

/// A write of a document's file that waits for the check of its stamp.
#[derive(Debug)]
pub(crate) struct PendingWrite {
    pub(crate) path: String,
    pub(crate) format: FileFormat,
    pub(crate) bytes: Vec<u8>,
    pub(crate) auto: bool,
    pub(crate) version: Option<Vec<u8>>,
    pub(crate) state: StateId,
}

/// What a storage request continues with.
#[derive(Debug)]
pub(crate) enum Wait {
    /// A write: a failure is reported, and for a document's journal followed by a checkpoint.
    Write { doc: Option<DocId> },
    /// One value of a gather.
    Gather { gather: u32, key: String },
    /// The keys of an area.
    List(ListNext),
    /// AutoSave's timer.
    AutoSave { doc: DocId },
    /// The checkpoint timer (others' edits, a failed write).
    Snapshot { doc: DocId },
    /// The large write of a checkpoint.
    Checkpoint { doc: DocId },
    /// The first checkpoint of a file just opened, taken while nothing else goes on.
    Prepare { doc: DocId },
    /// The watched file changed.
    Watch { doc: DocId, path: String },
    /// The file read again (`suite.file.reload`).
    Reload { doc: DocId },
    /// The stamp of the document's file before Kiln writes it: written when no other program
    /// changed it.
    WriteCheck {
        doc: DocId,
        write: Box<PendingWrite>,
    },
    /// The stamp of the document's file after Kiln opened, read or wrote it.
    Stamp { doc: DocId, path: String },
    /// Whether a file with a local version history is still there.
    VersionFileThere {
        record: kiln_storage::versions::PathRecord,
        index: Option<Box<kiln_storage::versions::HistoryIndex>>,
    },
    /// Document Recovery ▸ Delete: the marker is gone, the rest goes next.
    RecoveryDeleted { rest: Vec<String> },
}

impl Wait {
    /// The document the continuation works on.
    pub(crate) fn doc(&self) -> Option<DocId> {
        match self {
            Wait::AutoSave { doc }
            | Wait::Snapshot { doc }
            | Wait::Checkpoint { doc }
            | Wait::Prepare { doc }
            | Wait::Watch { doc, .. }
            | Wait::Reload { doc }
            | Wait::WriteCheck { doc, .. }
            | Wait::Stamp { doc, .. } => Some(*doc),
            Wait::Write { doc } => *doc,
            Wait::Gather { .. }
            | Wait::List(_)
            | Wait::RecoveryDeleted { .. }
            | Wait::VersionFileThere { .. } => None,
        }
    }
}

/// What the keys of an area are for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ListNext {
    /// The recovery scan.
    Recovery,
    /// Reading the recent files.
    Recents,
    /// The values of one journal, to open it.
    RecoveryOpen { key: String },
    /// The values of one journal, to delete it.
    RecoveryDelete { key: String },
    /// The local version histories, to keep them within their limits.
    Versions,
}

/// Several values read together, then one continuation.
struct Gather {
    remaining: usize,
    values: BTreeMap<String, Option<Vec<u8>>>,
    /// The first read that failed (rather than found nothing).
    failed: Option<ErrorInfo>,
    then: Then,
}

/// What a gather continues with.
#[derive(Debug)]
pub(crate) enum Then {
    /// The markers and logs of the recovery scan, among every key of the area.
    RecoveryList { keys: Vec<String> },
    /// One journal to open.
    RecoveryOpen { key: String },
    /// The recent files.
    Recents,
    /// The user dictionary.
    Dictionary,
    /// The records that lead a file just opened to its history.
    VersionFile {
        doc: DocId,
        path: String,
        print: String,
    },
    /// The index of a document's history.
    VersionIndex { doc: DocId, history: String },
    /// Every index and record of the `Versions` area (keeping it tidy).
    VersionIndexes { keys: Vec<String> },
    /// Version models, then a version command.
    VersionModels { doc: DocId, next: versions::Next },
}

impl Session {
    /// the host keeps app storage and answers its requests (later, through
    /// `Event::HostRequest`: the web page keeps it in the Origin Private File System). Call it
    /// before the first document opens; documents opened before keep no journal.
    pub fn enable_storage(&mut self) {
        if !self.storage.enabled {
            self.storage.enabled = true;
            self.storage_start();
        }
    }

    /// The storage requests a session starts with (the user dictionary, and keeping the local
    /// version histories tidy).
    pub(crate) fn storage_start(&mut self) {
        if self.storage.enabled {
            self.gather(
                StoreArea::Dictionary,
                vec![kiln_storage::dictionary::KEY.to_owned()],
                Then::Dictionary,
            );
            self.request(
                HostOp::StoreList {
                    area: StoreArea::Versions,
                    prefix: String::new(),
                },
                Pending::Storage(Wait::List(ListNext::Versions)),
            );
        }
    }

    /// Sends app storage writes that belong to no document (a failure is reported).
    pub(crate) fn store(&mut self, ops: Vec<HostOp>) {
        self.store_for(None, ops);
    }

    /// Sends app storage writes for a document's journal or versions (a failure is reported,
    /// and a checkpoint follows).
    pub(crate) fn store_for(&mut self, doc: Option<DocId>, ops: Vec<HostOp>) {
        for op in ops {
            self.request(op, Pending::Storage(Wait::Write { doc }));
        }
    }

    /// A write to the app storage failed: the user hears of it (once until writes succeed
    /// again), and a document's journal takes a checkpoint next, so it never goes on from a
    /// record that was lost.
    fn storage_failed(&mut self, doc: Option<DocId>, e: ErrorInfo, record: bool) {
        let st = doc.and_then(|d| self.storage.docs.get_mut(&d));
        let mut lost = Vec::new();
        let first = match st {
            Some(st) => {
                st.checkpoint_wanted = true;
                // A record that never reached the disk: a replay stops there until a checkpoint
                // after it lands, and the page asks before it goes.
                if record && st.journal.started() {
                    lost = st.journal.record_lost();
                }
                !std::mem::replace(&mut st.failing, true)
            }
            None => !std::mem::replace(&mut self.storage.failing, true),
        };
        self.store_for(doc, lost);
        if !first {
            return;
        }
        let title = doc.and_then(|d| self.docs.get(&d)).map(|d| d.title.clone());
        let message = match &title {
            Some(t) => format!(
                "Kiln could not keep a recovery copy of {t}: {}. Save the document to keep \
                 your work.",
                e.message
            ),
            None => format!("Kiln could not write its app storage: {}", e.message),
        };
        self.events.push_back(Event::Error {
            doc,
            error: ErrorInfo { message, ..e },
        });
        // A checkpoint shortly, so a document left alone is covered again.
        if let Some(id) = doc
            && let Some(st) = self.storage.docs.get_mut(&id)
            && !st.snapshot_timer
        {
            st.snapshot_timer = true;
            self.request(
                HostOp::Timer {
                    ms: journal::SNAPSHOT_MS,
                },
                Pending::Storage(Wait::Snapshot { doc: id }),
            );
        }
    }

    /// A write to the app storage succeeded: a later failure is reported again.
    fn storage_ok(&mut self, doc: Option<DocId>) {
        match doc.and_then(|d| self.storage.docs.get_mut(&d)) {
            Some(st) => st.failing = false,
            None => self.storage.failing = false,
        }
    }

    /// Reads `keys` of `area`, then runs `then` with their values.
    pub(crate) fn gather(&mut self, area: StoreArea, keys: Vec<String>, then: Then) {
        let id = self.storage.next_gather;
        self.storage.next_gather = self.storage.next_gather.wrapping_add(1);
        if keys.is_empty() {
            self.gathered(then, BTreeMap::new(), None);
            return;
        }
        self.storage.gathers.insert(
            id,
            Gather {
                remaining: keys.len(),
                values: BTreeMap::new(),
                failed: None,
                then,
            },
        );
        for key in keys {
            self.request(
                HostOp::StoreGet {
                    area,
                    key: key.clone(),
                },
                Pending::Storage(Wait::Gather { gather: id, key }),
            );
        }
    }

    /// The reply to a storage request.
    pub(crate) fn storage_reply(&mut self, wait: Wait, reply: HostReply) {
        match wait {
            Wait::Write { doc } => match reply {
                HostReply::Failed(e) => self.storage_failed(doc, e, true),
                _ => self.storage_ok(doc),
            },
            Wait::Gather { gather, key } => {
                let Some(g) = self.storage.gathers.get_mut(&gather) else {
                    return;
                };
                let value = match reply {
                    HostReply::Bytes(b) => Some(b),
                    HostReply::Failed(e) => {
                        g.failed.get_or_insert(e);
                        None
                    }
                    _ => None,
                };
                g.values.insert(key, value);
                g.remaining = g.remaining.saturating_sub(1);
                if g.remaining == 0
                    && let Some(g) = self.storage.gathers.remove(&gather)
                {
                    self.gathered(g.then, g.values, g.failed);
                }
            }
            Wait::List(next) => {
                let keys = match reply {
                    HostReply::Keys(k) => k,
                    _ => Vec::new(),
                };
                match next {
                    ListNext::Recovery => self.recovery_listed(keys),
                    ListNext::Recents => self.gather(StoreArea::Recents, keys, Then::Recents),
                    ListNext::RecoveryOpen { key } => self.recovery_open_listed(key, keys),
                    ListNext::RecoveryDelete { key } => self.recovery_delete_listed(&key, &keys),
                    ListNext::Versions => self.versions_listed(&keys),
                }
            }
            Wait::AutoSave { doc } => self.autosave_timer(doc),
            Wait::Snapshot { doc } => self.snapshot_timer(doc),
            Wait::Checkpoint { doc } => match reply {
                HostReply::Failed(e) => {
                    self.checkpoint_landed(doc, false);
                    self.storage_failed(Some(doc), e, false);
                }
                _ => {
                    self.storage_ok(Some(doc));
                    self.checkpoint_landed(doc, true);
                }
            },
            Wait::Prepare { doc } => self.prepare_journal(doc),
            Wait::Watch { doc, path } => self.watch_reply(doc, path, &reply),
            Wait::Reload { doc } => match reply {
                HostReply::Bytes(bytes) => {
                    if let Err(e) = self.reloaded(doc, &bytes) {
                        self.events.push_back(Event::Error {
                            doc: Some(doc),
                            error: (&e).into(),
                        });
                    }
                }
                HostReply::Failed(e) => self.events.push_back(Event::Error {
                    doc: Some(doc),
                    error: e,
                }),
                _ => {}
            },
            Wait::WriteCheck { doc, write } => self.write_checked(doc, *write, &reply),
            Wait::Stamp { doc, path } => self.stamp_read(doc, &path, &reply),
            Wait::RecoveryDeleted { rest } => self.recovery_deleted(rest, reply),
            Wait::VersionFileThere { record, index } => {
                if matches!(reply, HostReply::Empty) {
                    let now = self.host.now_ms();
                    let keys = kiln_storage::versions::gone(&record, index.as_deref(), now);
                    self.store(versions_deletes(keys));
                }
            }
        }
    }

    fn gathered(
        &mut self,
        then: Then,
        values: BTreeMap<String, Option<Vec<u8>>>,
        failed: Option<ErrorInfo>,
    ) {
        match then {
            Then::RecoveryList { keys } => self.recovery_found(&values, &keys),
            Then::RecoveryOpen { key } => self.recovery_read(&key, &values, failed),
            Then::Recents => self.recents_read(&values),
            Then::Dictionary => self.dictionary_read(&values),
            Then::VersionIndex { doc, history } => self.versions_read(doc, &history, &values),
            Then::VersionModels { doc, next } => self.version_models_read(doc, values, next),
            Then::VersionFile { doc, path, print } => {
                self.version_file_read(doc, &path, &print, &values);
            }
            Then::VersionIndexes { keys } => self.versions_tidy(&keys, &values),
        }
    }

    // ----- the user dictionary ---------------------------------------------------------------

    fn dictionary_read(&mut self, values: &BTreeMap<String, Option<Vec<u8>>>) {
        let stored = values
            .values()
            .flatten()
            .next()
            .map(|b| kiln_storage::dictionary::decode(b))
            .unwrap_or_default();
        // Words added before the dictionary was read are kept too.
        let mut words = self.storage.dictionary.take().unwrap_or_default();
        for w in stored {
            kiln_storage::dictionary::add(&mut words, &w);
        }
        self.storage.dictionary = Some(words);
        self.apply_dictionary();
    }

    /// Gives every open Write document the user dictionary.
    fn apply_dictionary(&mut self) {
        let Some(words) = self.storage.dictionary.clone() else {
            return;
        };
        let cmd =
            kiln_protocol::Command::Write(kiln_protocol::WriteCommand::SetUserDictionary { words });
        for d in self.docs.values_mut() {
            if d.kind == kiln_protocol::DocKind::Write {
                let _ = d.model.execute(&cmd);
            }
        }
    }

    /// Before a command runs: Add to Dictionary keeps the word, and the words an editor sends
    /// (`write.spell.user_dictionary`) join the stored ones, so the document gets all of them.
    pub(crate) fn storage_command(&mut self, cmd: &mut kiln_protocol::Command) {
        use kiln_protocol::WriteCommand as W;
        if !self.storage.enabled {
            return;
        }
        let kiln_protocol::Command::Write(w) = cmd else {
            return;
        };
        match w {
            W::SpellAddWord { word } => {
                let words = self.storage.dictionary.get_or_insert_with(Vec::new);
                if kiln_storage::dictionary::add(words, word) {
                    self.save_dictionary();
                }
            }
            W::SetUserDictionary { words } => {
                let stored = self.storage.dictionary.get_or_insert_with(Vec::new);
                let mut changed = false;
                for word in words.iter() {
                    changed |= kiln_storage::dictionary::add(stored, word);
                }
                words.clone_from(stored);
                if changed {
                    self.save_dictionary();
                }
            }
            _ => {}
        }
    }

    fn save_dictionary(&mut self) {
        let Some(words) = &self.storage.dictionary else {
            return;
        };
        let op = HostOp::StorePut {
            area: StoreArea::Dictionary,
            key: kiln_storage::dictionary::KEY.into(),
            bytes: kiln_storage::dictionary::encode(words),
        };
        self.store(vec![op]);
    }
}

/// A fresh journal key: twelve lowercase letters and digits.
pub(crate) fn new_key() -> String {
    const ALL: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut b = [0u8; 12];
    let _ = getrandom::fill(&mut b);
    b.iter()
        .map(|x| char::from(ALL[usize::from(*x) % ALL.len()]))
        .collect()
}

/// The Compatibility Checker's loss for a bundle (or recovered document) whose original
/// Office file could not be read: saving in that format writes everything from the model, and
/// what the model does not cover (macros, custom XML, markup Kiln does not read) is not kept.
pub(crate) fn original_lost(app: &str) -> kiln_protocol::CompatIssue {
    kiln_protocol::CompatIssue {
        code: "bundle.original_unreadable".into(),
        severity: kiln_protocol::CompatSeverity::Loss,
        message: format!(
            "The original {app} file kept with this document could not be read: saving as {app} \
             writes only what Kiln shows, without the parts it does not edit (macros, custom \
             XML)"
        ),
        count: 1,
        origin: kiln_protocol::CompatOrigin::Save,
    }
}

/// Deletes keys of the `Versions` area.
pub(crate) fn versions_deletes(keys: Vec<String>) -> Vec<HostOp> {
    keys.into_iter()
        .map(|key| HostOp::StoreDelete {
            area: StoreArea::Versions,
            key,
        })
        .collect()
}

/// A storage error as an event.
pub(crate) fn error_event(doc: Option<DocId>, e: &KilnError) -> Event {
    Event::Error {
        doc,
        error: e.into(),
    }
}
