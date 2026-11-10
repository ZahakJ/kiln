// SPDX-License-Identifier: AGPL-3.0-or-later
//! The CRDT seam: the small [`Crdt`] trait and its Loro implementation, [`LoroCrdt`].
//!
//! The trait is what the rest of Kiln needs from a CRDT engine (docs/adr/0004-crdt.md): merge
//! an update, say what version this replica is at, export what another replica lacks, report
//! local changes, undo and redo the local peer's edits, and snapshots. The model mappings
//! ([`crate::write`], [`crate::deck`]) are written against Loro's containers directly.

use std::sync::{Arc, Mutex, PoisonError};

use loro::{
    ContainerID, ExportMode, Frontiers, Index, LoroDoc, LoroValue, StyleConfig, Subscription,
    UndoItemMeta, UndoManager, VersionVector,
};

use crate::error::{CollabError, Result};

/// Receives each local change, encoded as an update.
pub type UpdateSink = Box<dyn Fn(&[u8]) + Send + Sync>;

/// A replicated document.
pub trait Crdt {
    /// Merges an update or a snapshot from another replica. Merging the same update twice, or
    /// updates in any order, gives the same state.
    ///
    /// # Errors
    ///
    /// An update this engine cannot decode.
    fn apply(&mut self, update: &[u8]) -> Result<()>;

    /// This replica's version, encoded (what [`Crdt::export_from`] takes).
    fn version(&self) -> Vec<u8>;

    /// Every change this replica has that a replica at `version` lacks; an empty `version` is
    /// the empty document (everything).
    ///
    /// # Errors
    ///
    /// A version this engine cannot decode.
    fn export_from(&self, version: &[u8]) -> Result<Vec<u8>>;

    /// Whether a replica at `version` lacks changes this one has.
    fn ahead_of(&self, version: &[u8]) -> bool;

    /// Calls `f` with every local change, encoded as an update, when it is committed.
    fn subscribe(&self, f: UpdateSink) -> Subscription;

    /// Undoes the local peer's latest change (other peers' changes stay). `false` when there
    /// is nothing to undo.
    ///
    /// # Errors
    ///
    /// The engine failed to apply the undo.
    fn undo(&mut self) -> Result<bool>;

    /// Redoes the latest undone local change.
    ///
    /// # Errors
    ///
    /// The engine failed to apply the redo.
    fn redo(&mut self) -> Result<bool>;

    /// The whole document with its history (what a late joiner receives).
    ///
    /// # Errors
    ///
    /// The engine failed to encode.
    fn snapshot(&self) -> Result<Vec<u8>>;

    /// The current state without history (a version in Version History).
    ///
    /// # Errors
    ///
    /// The engine failed to encode.
    fn shallow_snapshot(&self) -> Result<Vec<u8>>;

    /// A replica in the state a snapshot holds (read-only use: previews of a version).
    ///
    /// # Errors
    ///
    /// A snapshot this engine cannot decode.
    fn checkout(snapshot: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

/// A container a change touched and its path from the document root.
#[derive(Debug, Clone)]
pub(crate) struct Changed {
    pub target: ContainerID,
    pub path: Vec<(ContainerID, Index)>,
    /// The change itself, for text (what the mappings turn into ops without reading the text).
    pub text: Option<Vec<loro::TextDelta>>,
    /// For a map, the keys the change set or removed.
    pub keys: Vec<String>,
    /// For a list, the containers it inserted (or moved there).
    pub inserted: Vec<ContainerID>,
    /// For a tree, the nodes that appeared (created, or alive again).
    pub created: Vec<loro::TreeID>,
}

type Changes = Arc<Mutex<Vec<Changed>>>;

/// The selection the next undo item records, and the one the last undo or redo restored
/// (JSON of the model's selection type).
#[derive(Debug, Default)]
struct Selections {
    next: Option<String>,
    restored: Option<String>,
}

/// [`Crdt`] on a Loro document, with an undo manager scoped to the local peer.
pub struct LoroCrdt {
    doc: LoroDoc,
    undo: UndoManager,
    changes: Changes,
    selections: Arc<Mutex<Selections>>,
    _events: Subscription,
}

impl std::fmt::Debug for LoroCrdt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoroCrdt")
            .field("peer", &self.doc.peer_id())
            .finish_non_exhaustive()
    }
}

/// The origin (and message) of commits undo must not revert: the initial state, repairs of a
/// merged one, Version History records.
pub(crate) const SYSTEM_ORIGIN: &str = "kiln-system";

/// The keys of an undo step's metadata: the counter its operations begin at, and the selection
/// it restores.
const STEP_START: &str = "start";
const STEP_SELECTION: &str = "selection";

impl LoroCrdt {
    /// An empty document for peer `peer` (a random 64-bit id, unique per replica and session).
    ///
    /// # Errors
    ///
    /// Loro refused the peer id.
    pub fn new(peer: u64) -> Result<Self> {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer)?;
        Self::wrap(doc)
    }

    fn wrap(doc: LoroDoc) -> Result<Self> {
        // Marks never grow over text typed at their edges: the mapping gives every inserted
        // character exactly the formatting the model gave it.
        doc.config_default_text_style(Some(StyleConfig {
            expand: loro::ExpandType::None,
        }));
        doc.set_record_timestamp(true);
        let mut undo = UndoManager::new(&doc);
        undo.add_exclude_origin_prefix(SYSTEM_ORIGIN);
        undo.set_merge_interval(0);
        let selections: Arc<Mutex<Selections>> = Arc::default();
        let pushed = Arc::clone(&selections);
        // Each step records where its operations begin (what Undo would revert) and the
        // selection to restore.
        undo.set_on_push(Some(Box::new(move |_, span, _| {
            let s = pushed.lock().unwrap_or_else(PoisonError::into_inner);
            let mut value = loro::LoroMapValue::default();
            let map = value.make_mut();
            map.insert(STEP_START.into(), LoroValue::I64(i64::from(span.start)));
            if let Some(sel) = s.next.clone() {
                map.insert(STEP_SELECTION.into(), LoroValue::from(sel));
            }
            UndoItemMeta {
                value: LoroValue::Map(value),
                cursors: Vec::new(),
            }
        })));
        let popped = Arc::clone(&selections);
        undo.set_on_pop(Some(Box::new(move |_, _, meta| {
            let mut s = popped.lock().unwrap_or_else(PoisonError::into_inner);
            s.restored = match &meta.value {
                LoroValue::Map(m) => match m.get(STEP_SELECTION) {
                    Some(LoroValue::String(v)) => Some(v.to_string()),
                    _ => None,
                },
                _ => None,
            };
        })));
        let changes: Changes = Arc::default();
        let sink = Arc::clone(&changes);
        let events = doc.subscribe_root(Arc::new(move |e| {
            let mut list = sink.lock().unwrap_or_else(PoisonError::into_inner);
            for c in e.events {
                list.push(Changed {
                    target: c.target.clone(),
                    path: c.path.to_vec(),
                    text: match &c.diff {
                        loro::event::Diff::Text(d) => Some(d.clone()),
                        _ => None,
                    },
                    keys: match &c.diff {
                        loro::event::Diff::Map(m) => {
                            m.updated.keys().map(ToString::to_string).collect()
                        }
                        _ => Vec::new(),
                    },
                    inserted: match &c.diff {
                        loro::event::Diff::List(items) => items
                            .iter()
                            .flat_map(|i| match i {
                                loro::event::ListDiffItem::Insert { insert, .. } => {
                                    insert.as_slice()
                                }
                                _ => &[],
                            })
                            .filter_map(|v| match v {
                                loro::ValueOrContainer::Container(c) => {
                                    Some(loro::ContainerTrait::id(c))
                                }
                                loro::ValueOrContainer::Value(_) => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                    created: match &c.diff {
                        loro::event::Diff::Tree(t) => t
                            .diff
                            .iter()
                            .filter(|d| matches!(d.action, loro::TreeExternalDiff::Create { .. }))
                            .map(|d| d.target)
                            .collect(),
                        _ => Vec::new(),
                    },
                });
            }
        }));
        Ok(Self {
            doc,
            undo,
            changes,
            selections,
            _events: events,
        })
    }

    /// The Loro document (the mappings edit its containers).
    #[must_use]
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// This replica's peer id.
    #[must_use]
    pub fn peer(&self) -> u64 {
        self.doc.peer_id()
    }

    /// Commits pending local edits as one change; the changes it reports are dropped (the
    /// model made them).
    pub(crate) fn commit_local(&self) {
        self.doc.commit();
        self.take_changes();
    }

    /// Commits pending edits that undo must not revert (the initial state, repairs).
    pub(crate) fn commit_system(&self) {
        self.doc.set_next_commit_origin(SYSTEM_ORIGIN);
        self.doc.set_next_commit_message(SYSTEM_ORIGIN);
        self.doc.commit();
        self.take_changes();
    }

    /// Commits a Version History record (not undoable, and no edit).
    /// Commits a repair made while merging, as a system change, and returns what it changed.
    pub(crate) fn commit_repair(&self) -> Vec<Changed> {
        self.doc.set_next_commit_origin(SYSTEM_ORIGIN);
        self.doc.set_next_commit_message(SYSTEM_ORIGIN);
        self.doc.commit();
        self.take_changes()
    }

    pub(crate) fn commit_version(&self) {
        self.commit_system();
    }

    /// Whether the document's content changed after `frontiers` (encoded): any change outside
    /// the Version History records counts.
    #[must_use]
    pub fn changed_since(&self, frontiers: &[u8]) -> bool {
        let Ok(from) = Frontiers::decode(frontiers) else {
            return true;
        };
        let to = self.doc.oplog_frontiers();
        if from == to {
            return false;
        }
        let Ok(diff) = self.doc.diff(&from, &to) else {
            return true;
        };
        diff.iter().any(|(cid, _)| {
            let root = match self.doc.get_path_to_container(cid) {
                Some(path) => path.first().map(|(c, _)| c.clone()),
                None => Some(cid.clone()),
            };
            !matches!(root, Some(ContainerID::Root { name, .. }) if name.as_str() == crate::history::VERSIONS)
        })
    }

    /// The containers changed since the last call.
    pub(crate) fn take_changes(&self) -> Vec<Changed> {
        std::mem::take(&mut *self.changes.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The selection the next undo step records (restored when it is undone).
    pub(crate) fn set_undo_selection(&self, selection: Option<String>) {
        self.selections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .next = selection;
    }

    /// The selection the last undo or redo restored.
    pub(crate) fn take_undo_selection(&self) -> Option<String> {
        self.selections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .restored
            .take()
    }

    /// Starts an undo group: everything committed until [`Self::end_undo_group`] undoes as one.
    pub(crate) fn begin_undo_group(&mut self) {
        // Loro refuses a nested group; one is already open then, which is what is wanted.
        let _ = self.undo.group_start();
    }

    /// Ends the open undo group, if any.
    pub(crate) fn end_undo_group(&mut self) {
        self.undo.group_end();
    }

    /// Whether there is a local change to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    /// Whether there is an undone local change to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// The current version as frontiers, encoded (what a version in history records).
    #[must_use]
    pub fn frontiers(&self) -> Vec<u8> {
        self.doc.oplog_frontiers().encode()
    }

    /// Whether a version (encoded version vector) has everything this replica has.
    #[must_use]
    pub fn covers(&self, version: &[u8]) -> bool {
        let have = self.doc.oplog_vv();
        if version.is_empty() {
            return have.is_empty();
        }
        VersionVector::decode(version).is_ok_and(|v| v >= have)
    }

    /// Whether reverting the undo step `meta` describes would remove text someone else typed:
    /// a paragraph (or other text) the step created that now holds characters it did not make
    /// (another person's, or moved there by a repair). Undo then refuses, as Word does when it
    /// cannot undo.
    fn would_remove_others(&self, meta: Option<UndoItemMeta>) -> bool {
        let start = match meta.map(|m| m.value) {
            Some(LoroValue::Map(m)) => match m.get(STEP_START) {
                Some(LoroValue::I64(c)) => i32::try_from(*c).ok(),
                _ => None,
            },
            _ => None,
        };
        let Some(start) = start else {
            return false;
        };
        let me = self.doc.peer_id();
        let end = self.doc.oplog_vv().get(&me).copied().unwrap_or(0);
        if end <= start {
            return false;
        }
        // The step's own operations (not the repairs and records made meanwhile) and the
        // containers they inserted.
        let mut own: Vec<(i32, i32)> = Vec::new();
        let mut inserted: Vec<ContainerID> = Vec::new();
        for change in self
            .doc
            .export_json_in_id_span(loro::IdSpan::new(me, start, end))
        {
            if change.msg.as_deref() == Some(SYSTEM_ORIGIN) {
                continue;
            }
            let last = change.ops.last().map_or(change.id.counter, |o| {
                o.counter + i32::try_from(o.content.op_len()).unwrap_or(1)
            });
            own.push((change.id.counter, last));
            for op in &change.ops {
                use loro::json::{JsonOpContent, ListOp, MapOp, MovableListOp, TreeOp};
                let values: Vec<&LoroValue> = match &op.content {
                    JsonOpContent::MovableList(MovableListOp::Insert { value, .. })
                    | JsonOpContent::List(ListOp::Insert { value, .. }) => value.iter().collect(),
                    JsonOpContent::Map(MapOp::Insert { value, .. }) => vec![value],
                    JsonOpContent::Tree(TreeOp::Create { target, .. }) => {
                        if let Ok(meta) = self.doc.get_tree(op.container.clone()).get_meta(*target)
                        {
                            inserted.push(loro::ContainerTrait::id(&meta));
                        }
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
                inserted.extend(values.into_iter().filter_map(|v| match v {
                    LoroValue::Container(c) => Some(c.clone()),
                    _ => None,
                }));
            }
        }
        let made_here = |id: loro::ID| {
            id.peer == me && own.iter().any(|&(a, b)| id.counter >= a && id.counter < b)
        };
        let mut texts: Vec<loro::LoroText> = Vec::new();
        for cid in inserted {
            if self.doc.has_container(&cid) {
                texts_in(&self.doc, cid, &mut texts, 0);
            }
        }
        texts.into_iter().any(|text| {
            !text.is_deleted()
                && (0..text.len_unicode()).any(|i| {
                    text.get_cursor(i, loro::cursor::Side::Middle)
                        .and_then(|c| c.id)
                        .is_some_and(|id| !made_here(id))
                })
        })
    }

    /// The state at a version of this document's history (encoded frontiers) as a replica of
    /// its own.
    ///
    /// # Errors
    ///
    /// Frontiers this document's history does not hold.
    pub fn fork_at(&self, frontiers: &[u8]) -> Result<Self> {
        let f = Frontiers::decode(frontiers)?;
        Self::wrap(self.doc.fork_at(&f)?)
    }
}

/// The texts in a container and the containers it holds, at any depth.
fn texts_in(doc: &LoroDoc, cid: ContainerID, out: &mut Vec<loro::LoroText>, depth: usize) {
    use loro::{ContainerType, ValueOrContainer};
    if depth > 16 {
        return;
    }
    let mut child = |v: Option<ValueOrContainer>| {
        if let Some(ValueOrContainer::Container(c)) = v {
            texts_in(doc, loro::ContainerTrait::id(&c), out, depth + 1);
        }
    };
    match cid.container_type() {
        ContainerType::Text => out.push(doc.get_text(cid)),
        ContainerType::Map => {
            let map = doc.get_map(cid);
            for k in map.keys() {
                child(map.get(&k));
            }
        }
        ContainerType::MovableList => {
            let list = doc.get_movable_list(cid);
            for i in 0..list.len() {
                child(list.get(i));
            }
        }
        ContainerType::List => {
            let list = doc.get_list(cid);
            for i in 0..list.len() {
                child(list.get(i));
            }
        }
        _ => {}
    }
}

impl Crdt for LoroCrdt {
    fn apply(&mut self, update: &[u8]) -> Result<()> {
        self.doc.commit();
        self.doc.import(update)?;
        Ok(())
    }

    fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    fn export_from(&self, version: &[u8]) -> Result<Vec<u8>> {
        let from = if version.is_empty() {
            VersionVector::default()
        } else {
            VersionVector::decode(version)?
        };
        Ok(self.doc.export(ExportMode::updates(&from))?)
    }

    fn ahead_of(&self, version: &[u8]) -> bool {
        !self.covers(version)
    }

    fn subscribe(&self, f: UpdateSink) -> Subscription {
        self.doc.subscribe_local_update(Box::new(move |bytes| {
            f(bytes);
            true
        }))
    }

    fn undo(&mut self) -> Result<bool> {
        self.doc.commit();
        if self.would_remove_others(self.undo.top_undo_meta()) {
            self.undo.clear();
            return Err(CollabError::UndoBlocked);
        }
        Ok(self.undo.undo()?)
    }

    fn redo(&mut self) -> Result<bool> {
        self.doc.commit();
        if self.would_remove_others(self.undo.top_redo_meta()) {
            self.undo.clear();
            return Err(CollabError::UndoBlocked);
        }
        Ok(self.undo.redo()?)
    }

    fn snapshot(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    fn shallow_snapshot(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::state_only(None))?)
    }

    fn checkout(snapshot: &[u8]) -> Result<Self> {
        let doc = LoroDoc::from_snapshot(snapshot).map_err(CollabError::from)?;
        Self::wrap(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::ContainerTrait;

    #[test]
    fn replicas_exchange_updates_and_report_remote_changes() -> Result<()> {
        let a = LoroCrdt::new(1)?;
        let mut b = LoroCrdt::new(2)?;
        a.doc().get_text("t").insert(0, "hello")?;
        a.commit_local();
        assert!(a.take_changes().is_empty(), "local changes are the model's");
        b.apply(&a.export_from(&b.version())?)?;
        assert_eq!(b.doc().get_text("t").to_string(), "hello");
        let changed = b.take_changes();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].target, b.doc().get_text("t").id());
        assert!(b.covers(&a.version()));
        Ok(())
    }

    #[test]
    fn undo_reverts_only_the_local_peer() -> Result<()> {
        let mut a = LoroCrdt::new(1)?;
        let mut b = LoroCrdt::new(2)?;
        a.doc().get_text("t").insert(0, "a")?;
        a.commit_local();
        b.apply(&a.snapshot()?)?;
        b.doc().get_text("t").insert(1, "b")?;
        b.commit_local();
        a.apply(&b.export_from(&a.version())?)?;
        a.take_changes();
        assert!(a.undo()?);
        assert_eq!(a.doc().get_text("t").to_string(), "b");
        assert!(
            !a.take_changes().is_empty(),
            "an undo reports what it changed"
        );
        assert!(a.redo()?);
        assert_eq!(a.doc().get_text("t").to_string(), "ab");
        Ok(())
    }

    #[test]
    fn snapshots_check_out_the_state() -> Result<()> {
        let a = LoroCrdt::new(1)?;
        a.doc().get_map("m").insert("k", 1)?;
        a.commit_local();
        let at = a.frontiers();
        a.doc().get_map("m").insert("k", 2)?;
        a.commit_local();
        let now = LoroCrdt::checkout(&a.shallow_snapshot()?)?;
        assert_eq!(
            now.doc().get_map("m").get_value(),
            a.doc().get_map("m").get_value()
        );
        let then = a.fork_at(&at)?;
        assert_eq!(
            then.doc().get_map("m").get("k").map(|v| format!("{v:?}")),
            Some(format!("{:?}", loro::ValueOrContainer::Value(1.into())))
        );
        Ok(())
    }
}
