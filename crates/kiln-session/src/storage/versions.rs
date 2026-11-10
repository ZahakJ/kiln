// SPDX-License-Identifier: AGPL-3.0-or-later
//! Version History of documents that are not shared (local files): versions on every save and
//! every ten minutes of editing (`kiln_storage::versions`), kept in the bundle of a `.kwrite`,
//! `.kgrid` or `.kdeck` and in the app storage for other files, as a history of its own that
//! the file finds by its path and contents (a different document at the same path never
//! inherits it; a file moved by another program finds it by its contents; Save As takes it
//! along); shown read-only in place of the document, named, compared word by word (Write),
//! their slides marked (Deck), and restored as a new version. The commands and queries are
//! those of shared documents' Version History. At startup the histories are kept within their
//! total size, and those whose files are gone are removed.

use std::collections::BTreeMap;

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::collab::{
    VersionEntry, VersionOrigin, VersionParagraph, VersionSlide, VersionSlides,
};
use kiln_protocol::{
    ExecResult, ExecValue, FileFormat, HostOp, OpenHint, Outcome, Query, QueryResult, StoreArea,
    SuiteCommand,
};
use kiln_storage::Bundle;
use kiln_storage::bundle::StoredVersion;
use kiln_storage::versions::{
    HistoryIndex, LocalHistory, PathRecord, TOTAL_BYTES, history_of, index_key, model_key,
    path_key, print_key,
};

use super::{AUTHOR, Preview, Then};
use crate::model::DocModel;
use crate::session::{Pending, Session};

/// What a version command does once the models it needs are read.
#[derive(Debug, Clone)]
pub(crate) enum Next {
    /// Show the version.
    Preview(String),
    /// Restore it.
    Restore(String),
    /// Write the bundle (every version's model is needed).
    SaveBundle {
        path: String,
        format: FileFormat,
        auto: bool,
    },
}

fn unavailable(id: &str, why: &str) -> KilnError {
    KilnError::unavailable(id, why)
}

fn put(key: String, bytes: Vec<u8>) -> HostOp {
    HostOp::StorePut {
        area: StoreArea::Versions,
        key,
        bytes,
    }
}

fn delete(key: String) -> HostOp {
    HostOp::StoreDelete {
        area: StoreArea::Versions,
        key,
    }
}

impl Session {
    /// A document opened: the records that lead its file to its history are read (a bundle
    /// brought its own versions). `print` is the fingerprint of the file's contents.
    pub(crate) fn versions_opened(&mut self, id: DocId, print: Option<String>) {
        let Some(d) = self.docs.get(&id) else { return };
        let (Some(path), Some(format), Some(print)) = (d.path.clone(), d.format, print) else {
            return;
        };
        if format.is_bundle() {
            return;
        }
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.print = Some(print.clone());
        }
        self.gather(
            StoreArea::Versions,
            vec![path_key(&path), print_key(&print)],
            Then::VersionFile {
                doc: id,
                path,
                print,
            },
        );
    }

    /// The records read: the file's history is the path's when the contents are those Kiln
    /// last saw there, otherwise the one of these contents (a file moved or renamed); none for
    /// another document at the same path, which starts its own.
    pub(crate) fn version_file_read(
        &mut self,
        id: DocId,
        path: &str,
        print: &str,
        values: &BTreeMap<String, Option<Vec<u8>>>,
    ) {
        let value = |k: String| values.get(&k).and_then(Option::as_deref);
        let record = value(path_key(path)).and_then(PathRecord::from_bytes);
        let by_print = value(print_key(print)).and_then(|b| std::str::from_utf8(b).ok());
        let Some(history) = history_of(print, record.as_ref(), by_print) else {
            return;
        };
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if st.history_id.is_some() {
            return;
        }
        st.history_id = Some(history.clone());
        self.gather(
            StoreArea::Versions,
            vec![index_key(&history)],
            Then::VersionIndex { doc: id, history },
        );
    }

    /// The versions a bundle brought.
    pub(crate) fn versions_from_bundle(&mut self, id: DocId, versions: Vec<StoredVersion>) {
        if let Some(st) = self.storage.docs.get_mut(&id) {
            let entries = versions.iter().map(|v| v.entry.clone()).collect();
            st.history = LocalHistory::new(entries);
            st.sizes = versions
                .iter()
                .map(|v| (v.entry.id.clone(), v.model.len() as u64))
                .collect();
            st.models = versions
                .into_iter()
                .map(|v| (v.entry.id, v.model))
                .collect();
        }
    }

    /// The index of the file's history read: its versions join those taken since it opened,
    /// and the file's records are written again (the history follows a file found by its
    /// contents to its new name).
    pub(crate) fn versions_read(
        &mut self,
        id: DocId,
        history: &str,
        values: &BTreeMap<String, Option<Vec<u8>>>,
    ) {
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if st.history_id.as_deref() != Some(history) {
            return;
        }
        if let Some(bytes) = values.values().flatten().next()
            && let Ok(index) = HistoryIndex::from_bytes(bytes)
        {
            // Versions taken before the list arrived stay.
            let mut all = index.versions;
            for v in st.history.list() {
                if !all.iter().any(|x| x.id == v.id) {
                    all.push(v.clone());
                }
            }
            for (v, size) in index.sizes {
                st.sizes.entry(v).or_insert(size);
            }
            let edited = st.history.edited();
            st.history = LocalHistory::new(all);
            if edited {
                st.history.note_edit(st.last_edit_ms);
            }
        }
        let writes = self.history_writes(id, None);
        self.store_for(Some(id), writes);
    }

    /// The writes that keep a document's history in the app storage: its index, and the
    /// records that lead its file to it (`moved_from`: the records of the file it was before
    /// Save As, which no longer lead here).
    fn history_writes(
        &self,
        id: DocId,
        moved_from: Option<(String, Option<String>)>,
    ) -> Vec<HostOp> {
        let (Some(d), Some(st)) = (self.docs.get(&id), self.storage.docs.get(&id)) else {
            return Vec::new();
        };
        let (Some(history), Some(path)) = (st.history_id.clone(), d.path.clone()) else {
            return Vec::new();
        };
        let index = HistoryIndex {
            format: kiln_storage::versions::INDEX_FORMAT,
            versions: st.history.list().to_vec(),
            sizes: st.sizes.clone(),
            file: Some(path.clone()),
            print: st.print.clone(),
            used_ms: self.host.now_ms(),
        };
        let mut out = Vec::new();
        if let Ok(bytes) = index.to_bytes() {
            out.push(put(index_key(&history), bytes));
        }
        if let Some(print) = &st.print {
            let record = PathRecord {
                path: path.clone(),
                history: history.clone(),
                print: print.clone(),
            };
            out.push(put(path_key(&path), record.to_bytes()));
            out.push(put(print_key(print), history.into_bytes()));
        }
        if let Some((old_path, old_print)) = moved_from {
            if old_path != path {
                out.push(delete(path_key(&old_path)));
            }
            if let Some(old) = old_print.filter(|o| st.print.as_ref() != Some(o)) {
                out.push(delete(print_key(&old)));
            }
        }
        out
    }

    /// Whether a document keeps local versions: a Write or Deck document (the kinds with the
    /// Version History pane) that is not shared.
    fn keeps_versions(&self, id: DocId) -> bool {
        !self.collab.is_shared(id)
            && self.storage.docs.contains_key(&id)
            && self.doc(id).is_ok_and(|d| {
                matches!(
                    d.kind,
                    kiln_protocol::DocKind::Write | kiln_protocol::DocKind::Deck
                ) && d.model.bundles()
            })
    }

    /// An edit: ten minutes of editing make a version.
    pub(crate) fn versions_edit(&mut self, id: DocId) {
        let now = self.host.now_ms();
        if self.keeps_versions(id)
            && self
                .storage
                .docs
                .get(&id)
                .is_some_and(|st| st.history.due(now))
        {
            self.take_version(id, VersionOrigin::Auto);
        }
    }

    /// The model of the document as it is now (`model.bin`).
    fn model_now(&self, id: DocId) -> Result<Vec<u8>> {
        let d = self.doc(id)?;
        Ok(self
            .storage
            .live(id)
            .unwrap_or(d.model.as_ref())
            .to_bundle()?
            .model)
    }

    /// Takes a version of the document as it is now.
    pub(crate) fn take_version(&mut self, id: DocId, origin: VersionOrigin) -> Option<String> {
        let bytes = self.model_now(id).ok()?;
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.history.clear_edits();
        }
        self.add_version(id, origin, bytes)
    }

    /// Adds a version of `bytes` (the edit clock is the caller's).
    fn add_version(&mut self, id: DocId, origin: VersionOrigin, bytes: Vec<u8>) -> Option<String> {
        let now = self.host.now_ms();
        let st = self.storage.docs.get_mut(&id)?;
        let (entry, removed) = st.history.add(AUTHOR, now, origin);
        st.sizes.insert(entry.id.clone(), bytes.len() as u64);
        st.models.insert(entry.id.clone(), bytes.clone());
        for r in &removed {
            st.models.remove(r);
            st.sizes.remove(r);
        }
        let mut writes = Vec::new();
        if let Some(history) = st.history_id.clone() {
            writes.push(put(model_key(&history, &entry.id), bytes));
            writes.extend(removed.iter().map(|r| delete(model_key(&history, r))));
            writes.extend(self.history_writes(id, None));
        }
        self.store_for(Some(id), writes);
        Some(entry.id)
    }

    /// Before an explicit save is written: the version it takes (unless nothing changed since
    /// the latest). A bundle takes it now, so it is in the file; another file once it is
    /// written: the model it will be is returned, to come back with the write.
    pub(crate) fn versions_before_save(
        &mut self,
        id: DocId,
        format: FileFormat,
    ) -> Option<Vec<u8>> {
        if !self.keeps_versions(id) {
            return None;
        }
        let st = self.storage.docs.get(&id)?;
        if !st.history.edited() && !st.history.list().is_empty() {
            return None;
        }
        if format.is_bundle() {
            self.take_version(id, VersionOrigin::Save);
            return None;
        }
        let bytes = self.model_now(id).ok()?;
        self.storage.docs.get_mut(&id)?.history.clear_edits();
        Some(bytes)
    }

    /// The file was written (`print`: the fingerprint of what was written, for a file that is
    /// not a bundle): its version, and the history follows the file, also to a new name (Save
    /// As). A document saved to a file for the first time starts its history in the app
    /// storage with the versions taken so far.
    pub(crate) fn versions_saved(
        &mut self,
        id: DocId,
        version: Option<Vec<u8>>,
        print: Option<String>,
    ) {
        if !self.keeps_versions(id) {
            return;
        }
        let Some(d) = self.docs.get(&id) else { return };
        let target = match (&d.path, d.format) {
            (Some(p), Some(f)) if !f.is_bundle() => Some(p.clone()),
            _ => None,
        };
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        let Some(target) = target else {
            // A bundle keeps its versions in the file.
            if let Some(bytes) = version {
                self.add_version(id, VersionOrigin::Save, bytes);
            }
            return;
        };
        let before = st.recorded.replace((target.clone(), print.clone()));
        st.print = print;
        let mut writes = Vec::new();
        if st.history_id.is_none() {
            let history = super::new_key();
            for (v, m) in &st.models {
                writes.push(put(model_key(&history, v), m.clone()));
            }
            st.history_id = Some(history);
        }
        let changed = before.as_ref() != st.recorded.as_ref();
        if changed {
            writes.extend(self.history_writes(id, before));
        }
        self.store_for(Some(id), writes);
        if let Some(bytes) = version {
            self.add_version(id, VersionOrigin::Save, bytes);
        }
    }

    // ----- keeping the app storage tidy ----------------------------------------------------

    /// Every key of the `Versions` area: the indexes and records are read.
    pub(crate) fn versions_listed(&mut self, keys: &[String]) {
        let wanted: Vec<String> = keys
            .iter()
            .filter(|k| {
                !matches!(
                    kiln_storage::versions::parse_key(k),
                    kiln_storage::versions::VersionsKey::Model(..)
                )
            })
            .cloned()
            .collect();
        self.gather(
            StoreArea::Versions,
            wanted,
            Then::VersionIndexes {
                keys: keys.to_vec(),
            },
        );
    }

    /// Keeps the version histories within their limits, and checks that the files they belong
    /// to are still there.
    pub(crate) fn versions_tidy(
        &mut self,
        keys: &[String],
        values: &BTreeMap<String, Option<Vec<u8>>>,
    ) {
        let open = self
            .storage
            .docs
            .values()
            .filter_map(|s| s.history_id.clone())
            .collect();
        let t = kiln_storage::versions::tidy(keys, values, &open, TOTAL_BYTES);
        let mut writes = super::versions_deletes(t.delete);
        for (key, index) in t.rewrite {
            if let Ok(bytes) = index.to_bytes() {
                writes.push(put(key, bytes));
            }
        }
        self.store(writes);
        for record in t.check {
            let index = values
                .get(&index_key(&record.history))
                .and_then(Option::as_deref)
                .and_then(|b| HistoryIndex::from_bytes(b).ok())
                .map(Box::new);
            self.request(
                HostOp::StatFile {
                    path: record.path.clone(),
                },
                Pending::Storage(super::Wait::VersionFileThere { record, index }),
            );
        }
    }

    // ----- commands ------------------------------------------------------------------------

    /// The Version History commands of a document that is not shared.
    pub(crate) fn version_command(&mut self, id: DocId, cmd: SuiteCommand) -> Result<ExecResult> {
        if !self.keeps_versions(id) {
            return Err(unavailable(
                "suite.collab.version_preview",
                "this document keeps no versions",
            ));
        }
        match cmd {
            SuiteCommand::VersionPreview { id: None } => {
                self.storage_show_document(id);
            }
            SuiteCommand::VersionPreview { id: Some(v) } => {
                self.version_get(id, &v)?;
                let mut wanted = vec![v.clone()];
                if let Some(p) = self
                    .storage
                    .docs
                    .get(&id)
                    .and_then(|s| s.history.previous(&v))
                {
                    wanted.push(p.id.clone());
                }
                self.version_models(id, wanted, Next::Preview(v));
            }
            SuiteCommand::VersionRestore { id: v } => {
                if self.doc(id)?.read_only {
                    return Err(unavailable(
                        "suite.collab.version_restore",
                        "the document is read-only",
                    ));
                }
                self.version_get(id, &v)?;
                self.version_models(id, vec![v.clone()], Next::Restore(v));
            }
            SuiteCommand::VersionRename { id: v, name } => {
                let Some(st) = self.storage.docs.get_mut(&id) else {
                    return Err(KilnError::not_found(id));
                };
                st.history.rename(&v, &name)?;
                let writes = self.history_writes(id, None);
                self.store_for(Some(id), writes);
            }
            _ => return Err(KilnError::Internal("not a version command".into())),
        }
        Ok(ExecResult {
            outcome: Outcome::Applied,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    fn version_get(&self, id: DocId, v: &str) -> Result<VersionEntry> {
        self.storage
            .docs
            .get(&id)
            .and_then(|s| s.history.get(v))
            .cloned()
            .ok_or_else(|| KilnError::not_found(format!("version {v}")))
    }

    /// Reads the models of `ids` not in memory yet, then runs `next`.
    pub(crate) fn version_models(&mut self, id: DocId, ids: Vec<String>, next: Next) {
        let Some(st) = self.storage.docs.get(&id) else {
            return;
        };
        let history = st.history_id.clone();
        let missing: Vec<String> = ids
            .into_iter()
            .filter(|v| !st.models.contains_key(v))
            .filter_map(|v| history.as_ref().map(|h| model_key(h, &v)))
            .collect();
        self.gather(
            StoreArea::Versions,
            missing,
            Then::VersionModels { doc: id, next },
        );
    }

    pub(crate) fn version_models_read(
        &mut self,
        id: DocId,
        values: BTreeMap<String, Option<Vec<u8>>>,
        next: Next,
    ) {
        if let Some(st) = self.storage.docs.get_mut(&id) {
            for (key, value) in values {
                if let (Some(v), Some(bytes)) = (key.rsplit('/').next(), value) {
                    st.models.insert(v.to_owned(), bytes);
                }
            }
        }
        let result = match next {
            Next::Preview(v) => self.show_version(id, &v),
            Next::Restore(v) => self.restore_version(id, &v),
            Next::SaveBundle { path, format, auto } => {
                self.write_bundle(id, path, format, auto).map(|_| ())
            }
        };
        if let Err(e) = result {
            self.events.push_back(super::error_event(Some(id), &e));
        }
    }

    /// A model of version `v`, beside what the document keeps outside its model (the
    /// original file), so it renders and saves like the document.
    fn version_model(&self, id: DocId, v: &str) -> Result<Box<dyn DocModel>> {
        let d = self.doc(id)?;
        let st = self
            .storage
            .docs
            .get(&id)
            .ok_or_else(|| KilnError::not_found(id))?;
        let bytes = st
            .models
            .get(v)
            .ok_or_else(|| KilnError::not_found(format!("the model of version {v}")))?;
        let live = self.storage.live(id).unwrap_or(d.model.as_ref());
        let base = live.to_bundle()?;
        let bundle = Bundle {
            model: bytes.clone(),
            ..base
        };
        let hint = OpenHint {
            name: None,
            path: d.path.clone(),
            format: d.format,
            read_only: false,
            kind: Some(d.kind),
            template: false,
        };
        self.app_for_kind(d.kind)?.open_bundle(&bundle, &hint)
    }

    /// Shows version `v` read-only in place of the document.
    fn show_version(&mut self, id: DocId, v: &str) -> Result<()> {
        if self
            .storage
            .docs
            .get(&id)
            .and_then(|s| s.preview.as_ref())
            .is_some_and(|p| p.id == v)
        {
            return Ok(());
        }
        self.storage_show_document(id);
        let model = self.version_model(id, v)?;
        let d = self.doc_mut(id)?;
        let live = std::mem::replace(&mut d.model, model);
        d.frames.reset();
        d.revision += 1;
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.preview = Some(Preview {
                id: v.to_owned(),
                live,
            });
        }
        self.emit_changed(id);
        Ok(())
    }

    /// Shows the document again in place of a version on show.
    pub(crate) fn storage_show_document(&mut self, id: DocId) {
        let Some(p) = self
            .storage
            .docs
            .get_mut(&id)
            .and_then(|s| s.preview.take())
        else {
            return;
        };
        if let Some(d) = self.docs.get_mut(&id) {
            d.model = p.live;
            d.frames.reset();
            d.revision += 1;
        }
        self.emit_changed(id);
    }

    /// Restores version `v`: the document becomes that version as a new change (marked as
    /// changed, Undo starts afresh) and a version of it is taken, so nothing is lost.
    fn restore_version(&mut self, id: DocId, v: &str) -> Result<()> {
        self.storage_show_document(id);
        let mut model = self.version_model(id, v)?;
        model.set_journaling(true);
        let d = self.doc_mut(id)?;
        d.model = model;
        d.frames.reset();
        d.revision += 1;
        d.saved_state = crate::undo::StateId::MAX;
        let state = d.model.state();
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.journaled = state;
        }
        self.checkpoint(id);
        self.take_version(id, VersionOrigin::Restore(v.to_owned()));
        self.emit_changed(id);
        Ok(())
    }

    /// Writes a native bundle with every version (reading the models not in memory first).
    pub(crate) fn write_bundle(
        &mut self,
        id: DocId,
        path: String,
        format: FileFormat,
        auto: bool,
    ) -> Result<ExecResult> {
        let missing: Vec<String> = self
            .storage
            .docs
            .get(&id)
            .map(|st| {
                st.history
                    .list()
                    .iter()
                    .filter(|v| !st.models.contains_key(&v.id))
                    .map(|v| v.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let readable = self
            .storage
            .docs
            .get(&id)
            .is_some_and(|st| st.history_id.is_some());
        if !missing.is_empty() && readable {
            self.version_models(id, missing, Next::SaveBundle { path, format, auto });
            return Ok(ExecResult {
                outcome: Outcome::Applied,
                summary: Some(self.summary(id)?),
                value: ExecValue::None,
            });
        }
        let now = self.host.now_ms();
        let bytes = self.bundle_of(id, now, true)?;
        self.write_bytes(id, path, format, bytes, auto, None)
    }

    // ----- queries -------------------------------------------------------------------------

    /// The Version History queries of a document that is not shared (`None` for others).
    pub(crate) fn storage_query(
        &mut self,
        doc: Option<DocId>,
        query: &Query,
    ) -> Option<Result<QueryResult>> {
        let id = doc?;
        if !matches!(
            query,
            Query::Versions | Query::VersionCompare { .. } | Query::VersionSlides { .. }
        ) || self.collab.is_shared(id)
            || !self.storage.docs.contains_key(&id)
        {
            return None;
        }
        Some(match query {
            Query::Versions => Ok(QueryResult::Versions(
                self.storage.docs[&id].history.list().to_vec(),
            )),
            Query::VersionCompare { id: v } => self.compare(id, v).map(QueryResult::VersionCompare),
            Query::VersionSlides { id: v, width, dpr } => self
                .slides(id, v, *width, *dpr)
                .map(|s| QueryResult::VersionSlides(Box::new(s))),
            _ => return None,
        })
    }

    /// Write: the word-level comparison of version `v` with the document.
    fn compare(&mut self, id: DocId, v: &str) -> Result<Vec<VersionParagraph>> {
        let old = self.storage.docs[&id]
            .models
            .get(v)
            .cloned()
            .ok_or_else(|| KilnError::not_found(format!("the model of version {v}")))?;
        // The document itself, also while a version is shown in its place.
        let model: &mut dyn DocModel = match self
            .storage
            .docs
            .get_mut(&id)
            .and_then(|s| s.preview.as_mut())
        {
            Some(p) => p.live.as_mut(),
            None => self
                .docs
                .get_mut(&id)
                .ok_or_else(|| KilnError::not_found(id))?
                .model
                .as_mut(),
        };
        model
            .collab()
            .ok_or_else(|| KilnError::Unsupported("comparing versions of this document".into()))?
            .version_compare(&old)
    }

    /// Deck: the slides of version `v`, marked against the version before it, with their
    /// thumbnails.
    fn slides(&mut self, id: DocId, v: &str, width: f32, dpr: f32) -> Result<VersionSlides> {
        let st = &self.storage.docs[&id];
        let bytes = st
            .models
            .get(v)
            .cloned()
            .ok_or_else(|| KilnError::not_found(format!("the model of version {v}")))?;
        let before = st
            .history
            .previous(v)
            .and_then(|p| st.models.get(&p.id))
            .cloned();
        let d = self.doc_mut(id)?;
        let m = d
            .model
            .collab()
            .ok_or_else(|| KilnError::Unsupported("slides of this document".into()))?;
        let slides: Vec<VersionSlide> = m.version_marks(before.as_deref(), &bytes)?;
        let scene = m.version_slides(&bytes, &slides, width, dpr)?;
        Ok(VersionSlides { slides, scene })
    }
}
