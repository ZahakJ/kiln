// SPDX-License-Identifier: AGPL-3.0-or-later
//! The autosave journal of each open document (see the module documentation of
//! `crate::storage`).

use kiln_common::{DocId, Result};
use kiln_protocol::HostOp;
use kiln_storage::Bundle;
use kiln_storage::journal::{JOURNAL_FORMAT, Journal, JournalMeta};

use super::{DocStore, Wait, new_key};
use crate::session::{Pending, Session};
use crate::undo::StateId;

/// The name, in the journal, of edits that came from others.
const REMOTE: &str = "collab.remote";

/// How long a checkpoint after a failed write waits.
pub(super) const SNAPSHOT_MS: u32 = 1_000;
/// How long after a file opens its first checkpoint is taken (unless an edit takes it first).
const PREPARE_MS: u32 = 1_500;

impl Session {
    /// A document was opened or created: it gets a journal (written at its first change), its
    /// versions, its place in the recent files and a watch on its file.
    pub(crate) fn storage_opened(&mut self, id: DocId) {
        // The fingerprint of the file just opened (or recovered) finds its version history.
        let print = self.storage.opening.take();
        if !self.storage.enabled {
            return;
        }
        let journal = !self.storage.no_recovery;
        let now = self.host.now_ms();
        let Some(d) = self.docs.get_mut(&id) else {
            return;
        };
        if !d.model.bundles() {
            return;
        }
        d.model.set_journaling(journal);
        let meta = JournalMeta {
            format: JOURNAL_FORMAT,
            key: new_key(),
            kind: d.kind,
            title: d.title.clone(),
            path: d.path.clone(),
            file_format: d.format,
            created_ms: now,
            saved_at_ms: now,
            dirty: false,
            generation: 0,
            print: print.clone(),
            seq: 0,
        };
        let state = d.model.state();
        let mut store = DocStore::new(Journal::new(meta), state);
        store.recovery = journal;
        #[cfg(test)]
        if let Some(bytes) = self.storage.inline_bytes {
            store.journal.set_inline_bytes(bytes);
        }
        self.storage.docs.insert(id, store);
        if d.kind == kiln_protocol::DocKind::Write && self.storage.dictionary.is_some() {
            self.apply_dictionary();
        }
        let path = self.docs.get(&id).and_then(|d| d.path.clone());
        if let (Some(st), Some(path)) = (self.storage.docs.get_mut(&id), path) {
            st.recorded = Some((path, print.clone()));
        }
        self.versions_opened(id, print);
        self.recents_opened(id);
        self.watch(id);
        self.stat(id);
        // A file's first checkpoint can be large: taken soon after it opens, before the first
        // keystroke has to wait for it.
        if self.docs.get(&id).is_some_and(|d| d.path.is_some()) {
            self.request(
                HostOp::Timer { ms: PREPARE_MS },
                Pending::Storage(Wait::Prepare { doc: id }),
            );
        }
    }

    /// The first checkpoint of a file just opened, with the marker clean, unless an edit took
    /// one already.
    pub(crate) fn prepare_journal(&mut self, id: DocId) {
        let now = self.host.now_ms();
        let untouched = match (self.storage.docs.get(&id), self.docs.get(&id)) {
            (Some(st), Some(d)) => {
                st.recovery
                    && !st.journal.started()
                    && st.preview.is_none()
                    && d.model.state() == st.journaled
            }
            _ => false,
        };
        if !untouched {
            return;
        }
        let Ok(bytes) = self.bundle_of(id, now, false) else {
            return;
        };
        if let Some(st) = self.storage.docs.get_mut(&id) {
            let writes = st.journal.prepare(&bytes, now).unwrap_or_default();
            self.store_checkpoint(id, writes);
        }
    }

    /// After a command or host reply: what changed goes to the journal, AutoSave and the
    /// ten-minute versions hear of the edit.
    pub(crate) fn storage_after(&mut self, id: DocId) {
        if !self.storage.enabled {
            return;
        }
        let now = self.host.now_ms();
        let (Some(d), Some(st)) = (self.docs.get_mut(&id), self.storage.docs.get_mut(&id)) else {
            return;
        };
        if st.preview.is_some() {
            return;
        }
        let ops = d.model.take_journal();
        let draft = d.model.take_draft();
        let state = d.model.state();
        let has_ops = ops.as_ref().is_some_and(|o| !o.is_empty());
        if state == st.journaled && !has_ops {
            self.store_draft(id, draft);
            return;
        }
        st.journaled = state;
        st.last_edit_ms = now;
        st.history.note_edit(now);
        if !st.recovery {
            // No recovery journal (Options ▸ Save): AutoSave and versions still hear of the edit.
            self.autosave_edit(id);
            self.versions_edit(id);
            return;
        }
        let mut writes = Vec::new();
        let checkpoint;
        // Before the first checkpoint lands, nothing can be recovered.
        let mut unkept = false;
        match ops {
            // Every change with operations is a record, whatever else is going on. The record
            // that reaches the count gets the checkpoint on top of it, and so does a record
            // made while one is wanted (a write failed, others edited).
            Some(ops) if has_ops && st.journal.started() => match st.journal.append(ops, now) {
                Ok(w) => {
                    writes = w;
                    checkpoint = st.journal.checkpoint_due()
                        || (st.checkpoint_wanted && st.journal.writing_key().is_none());
                }
                Err(_) => checkpoint = true,
            },
            // A change no operation holds: named in the log, and a checkpoint at once. Until it
            // lands, a recovery stops before it and says so.
            _ if st.journal.started() => {
                let what = self.storage.changing.clone().unwrap_or_default();
                if let Some(st) = self.storage.docs.get_mut(&id) {
                    writes = st.journal.missing(&what, now).unwrap_or_default();
                }
                checkpoint = true;
            }
            _ => {
                unkept = true;
                checkpoint = true;
            }
        }
        self.store_for(Some(id), writes);
        if checkpoint {
            self.take_checkpoint(id, unkept);
        }
        self.store_draft(id, draft);
        self.autosave_edit(id);
        self.versions_edit(id);
    }

    /// What the user is typing goes beside the journal, after what the same command committed.
    /// A document that has no journal yet starts one, with nothing to recover but this text.
    fn store_draft(&mut self, id: DocId, draft: Option<Option<Vec<u8>>>) {
        let Some(draft) = draft else { return };
        let now = self.host.now_ms();
        if self.storage.docs.get(&id).is_some_and(|st| !st.recovery) {
            return;
        }
        if self
            .storage
            .docs
            .get(&id)
            .is_some_and(|st| !st.journal.started())
        {
            if draft.is_none() {
                return;
            }
            let Ok(bytes) = self.bundle_of(id, now, false) else {
                return;
            };
            if let Some(st) = self.storage.docs.get_mut(&id) {
                let writes = st.journal.prepare(&bytes, now).unwrap_or_default();
                self.store_checkpoint(id, writes);
            }
        }
        if let Some(st) = self.storage.docs.get_mut(&id) {
            let writes = st.journal.draft(draft.as_deref(), now).unwrap_or_default();
            self.store_for(Some(id), writes);
        }
    }

    /// Writes a checkpoint of a document now.
    pub(crate) fn checkpoint(&mut self, id: DocId) {
        self.take_checkpoint(id, false);
    }

    /// Starts a checkpoint, or asks for one after the one under way. With `unkept`, a change that
    /// has no operations rests on it.
    fn take_checkpoint(&mut self, id: DocId, unkept: bool) {
        match self.storage.docs.get_mut(&id) {
            Some(st) if !st.recovery => return,
            Some(st) if st.journal.writing_key().is_some() => {
                st.checkpoint_wanted = true;
                st.unkept_wanted |= unkept;
                return;
            }
            _ => {}
        }
        let now = self.host.now_ms();
        let bytes = match self.bundle_of(id, now, false) {
            Ok(b) => b,
            Err(e) => {
                self.events.push_back(super::error_event(Some(id), &e));
                return;
            }
        };
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        st.checkpoint_wanted = false;
        let unkept = unkept || std::mem::take(&mut st.unkept_wanted);
        let writes = st
            .journal
            .checkpoint(&bytes, now, unkept)
            .unwrap_or_default();
        self.store_checkpoint(id, writes);
    }

    /// Sends a checkpoint's writes; the end of the large one moves the marker.
    fn store_checkpoint(&mut self, id: DocId, ops: Vec<HostOp>) {
        let base = self
            .storage
            .docs
            .get(&id)
            .and_then(|st| st.journal.writing_key());
        for op in ops {
            let last = matches!(&op, HostOp::StorePut { key, .. } if Some(key) == base.as_ref());
            let wait = if last {
                Wait::Checkpoint { doc: id }
            } else {
                Wait::Write { doc: Some(id) }
            };
            self.request(op, Pending::Storage(wait));
        }
    }

    /// The checkpoint under way is on the disk, or could not be written.
    pub(crate) fn checkpoint_landed(&mut self, id: DocId, written: bool) {
        let now = self.host.now_ms();
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if !written {
            st.journal.checkpoint_failed();
            return;
        }
        let writes = st.journal.checkpoint_written(now).unwrap_or_default();
        let again = st.checkpoint_wanted;
        self.store_for(Some(id), writes);
        if again {
            self.checkpoint(id);
        }
    }

    /// The checkpoint timer.
    pub(crate) fn snapshot_timer(&mut self, id: DocId) {
        let wanted = match self.storage.docs.get_mut(&id) {
            Some(st) => {
                st.snapshot_timer = false;
                st.checkpoint_wanted
            }
            None => false,
        };
        if wanted {
            self.checkpoint(id);
        }
    }

    /// A shared document changed through others' edits (they record no operations here): they
    /// go in the log by name, and a checkpoint keeps them at once. Until it lands, a recovery
    /// stops before them and says so.
    pub(crate) fn storage_remote(&mut self, id: DocId) {
        if let (Some(st), Some(d)) = (self.storage.docs.get_mut(&id), self.docs.get_mut(&id)) {
            let _ = d.model.take_journal();
            st.journaled = d.model.state();
            if st.journal.started() {
                let now = self.host.now_ms();
                let writes = st.journal.missing(REMOTE, now).unwrap_or_default();
                self.store_for(Some(id), writes);
            }
            self.checkpoint(id);
        }
    }

    /// The document as a bundle: its model and passthrough, the title and dates, and with
    /// `versions` its version history (a `.kwrite`, `.kgrid` or `.kdeck` the user saves; a
    /// checkpoint has none).
    pub(crate) fn bundle_of(&self, id: DocId, now: u64, versions: bool) -> Result<Vec<u8>> {
        let d = self.doc(id)?;
        let model = self.storage.live(id).unwrap_or(d.model.as_ref());
        let mut b: Bundle = model.to_bundle()?;
        b.title = Some(d.title.clone());
        b.modified_ms = now;
        if let Some(st) = self.storage.docs.get(&id) {
            b.created_ms = st.journal.meta().created_ms;
            if versions {
                b.versions = st
                    .history
                    .list()
                    .iter()
                    .filter_map(|v| {
                        st.models
                            .get(&v.id)
                            .map(|m| kiln_storage::bundle::StoredVersion {
                                entry: v.clone(),
                                model: m.clone(),
                            })
                    })
                    .collect();
            }
        }
        // A checkpoint is written often and read after a crash: stored, not deflated.
        b.to_bytes_with(versions)
    }

    /// The document was written as it was in `state`: unless it changed while the file was
    /// written, the journal holds nothing the file does not; an explicit save takes its
    /// version, puts the file on top of the recent files with a thumbnail and watches it.
    pub(crate) fn storage_saved(
        &mut self,
        id: DocId,
        auto: bool,
        version: Option<Vec<u8>>,
        state: StateId,
        print: Option<String>,
    ) {
        if !self.storage.enabled {
            return;
        }
        let now = self.host.now_ms();
        let Some(d) = self.docs.get(&id) else { return };
        let (title, path, format) = (d.title.clone(), d.path.clone(), d.format);
        let model = self.storage.live(id).unwrap_or(d.model.as_ref());
        let clean = model.state() == state;
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if st.journal.started() {
            let writes = st
                .journal
                .saved(
                    &title,
                    path.as_deref(),
                    format,
                    print.as_deref(),
                    clean,
                    now,
                )
                .unwrap_or_default();
            self.store_for(Some(id), writes);
        }
        self.stat(id);
        self.versions_saved(id, version, print);
        if !auto {
            self.recents_saved(id);
        }
        self.watch(id);
    }

    /// The document closed: with `clean` (nothing unsaved, or the user chose not to save) its
    /// journal goes, the marker first; otherwise the journal stays for Document Recovery.
    pub(crate) fn storage_closed(&mut self, id: DocId, clean: bool) {
        let Some(st) = self.storage.docs.remove(&id) else {
            return;
        };
        if clean && st.journal.started() {
            self.store(st.journal.remove());
        }
        if let Some(path) = st.watched {
            self.request(HostOp::UnwatchFile { path }, Pending::Ignore);
        }
    }
}
