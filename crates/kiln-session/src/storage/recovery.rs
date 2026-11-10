// SPDX-License-Identifier: AGPL-3.0-or-later
//! Document Recovery: the journals a crash left behind (see `crate::storage`).

use std::collections::BTreeMap;

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    DocKind, ErrorInfo, Event, ExecResult, ExecValue, HostOp, HostReply, OpenHint, Outcome,
    RecoveryEntry, StoreArea,
};
use kiln_storage::Bundle;
use kiln_storage::journal::{self, Journal};

use super::{ListNext, Then, Wait};
use crate::frame::FrameState;
use crate::session::{Document, Pending, Session};

/// The command a recovery names when a change in the journal did not replay (the page says "a
/// change Kiln could not repeat").
pub(crate) const UNREPEATABLE: &str = "journal.unrepeatable";

impl Session {
    /// `suite.file.recovery_scan`: lists the journals (`Event::Recovery` follows).
    pub(crate) fn recovery_scan(&mut self) -> ExecResult {
        if !self.storage.enabled {
            self.events.push_back(Event::Recovery {
                entries: Vec::new(),
            });
            return Self::storage_result(Outcome::Applied);
        }
        let outcome = self.request(
            HostOp::StoreList {
                area: StoreArea::Journal,
                prefix: String::new(),
            },
            Pending::Storage(Wait::List(ListNext::Recovery)),
        );
        Self::storage_result(outcome)
    }

    pub(crate) fn storage_result(outcome: Outcome) -> ExecResult {
        ExecResult {
            outcome,
            summary: None,
            value: ExecValue::None,
        }
    }

    /// The keys of the journal area: leftovers of interrupted removals and checkpoints go,
    /// the markers and logs of journals no open document holds are read.
    pub(crate) fn recovery_listed(&mut self, keys: Vec<String>) {
        let (marked, leftovers) = journal::journals(&keys);
        let ours: Vec<String> = self
            .storage
            .docs
            .values()
            .map(|s| s.journal.key().to_owned())
            .collect();
        // A journal of this session whose first checkpoint is being written has no marker
        // yet: it is not a leftover. Nor is one whose first checkpoint never landed, with
        // changes in it: it is listed, so the user hears they could not be kept.
        let unkept = |j: &str| keys.contains(&journal::unkept_key(j));
        let leftovers: Vec<String> = leftovers
            .into_iter()
            .filter(|k| {
                let j = journal::journal_of(k);
                !ours.iter().any(|o| o == j) && !unkept(j)
            })
            .collect();
        self.store(journal::delete_all(leftovers));
        let wanted: Vec<String> = keys
            .iter()
            .filter(|k| {
                let j = journal::journal_of(k);
                let read =
                    marked.iter().any(|m| m == j) && (k.ends_with(".meta") || journal::is_log(k));
                let lost = !marked.iter().any(|m| m == j) && k.ends_with(".unkept");
                !ours.iter().any(|o| o == j) && (read || lost)
            })
            .cloned()
            .collect();
        self.gather(StoreArea::Journal, wanted, Then::RecoveryList { keys });
    }

    /// The markers and logs read: the dirty journals are listed, newest first; clean ones (a
    /// crash after a save) are removed, and so are the values of checkpoints a crash cut
    /// short.
    pub(crate) fn recovery_found(
        &mut self,
        values: &BTreeMap<String, Option<Vec<u8>>>,
        keys: &[String],
    ) {
        let mut entries = Vec::new();
        let mut removals = Vec::new();
        for (k, v) in values {
            if let Some(key) = k.strip_suffix(".unkept")
                && !values.contains_key(&journal::meta_key(key))
                && let Some((title, kind)) = v.as_deref().and_then(journal::unkept_about)
            {
                entries.push(RecoveryEntry {
                    key: key.to_owned(),
                    kind,
                    title,
                    path: None,
                    format: None,
                    saved_at_ms: 0,
                    problem: Some(
                        "Kiln closed before the first recovery copy of this document was \
                         written, so its changes could not be kept."
                            .into(),
                    ),
                });
                continue;
            }
            let Some(key) = k.strip_suffix(".meta") else {
                continue;
            };
            let Some(bytes) = v else { continue };
            match journal::decode_meta(bytes) {
                Ok(meta) if !meta.dirty => removals.extend(journal::keys_of(key, keys)),
                Ok(meta) => {
                    removals.extend(journal::stale_keys(key, meta.generation, keys));
                    let last = values
                        .iter()
                        .filter(|(k, _)| journal::journal_of(k) == key && journal::is_log(k))
                        .filter_map(|(_, v)| v.as_deref())
                        .flat_map(|log| journal::decode_log(log, meta.format).records)
                        .map(|r| r.time_ms)
                        .max()
                        .unwrap_or(0);
                    entries.push(RecoveryEntry {
                        key: key.to_owned(),
                        kind: meta.kind,
                        title: meta.title,
                        path: meta.path,
                        format: meta.file_format,
                        saved_at_ms: meta.saved_at_ms.max(last),
                        problem: None,
                    });
                }
                Err(e) => entries.push(RecoveryEntry {
                    key: key.to_owned(),
                    kind: DocKind::Write,
                    title: "Unknown document".into(),
                    path: None,
                    format: None,
                    saved_at_ms: 0,
                    problem: Some(match e {
                        KilnError::Format { message, .. } => message,
                        other => other.to_string(),
                    }),
                }),
            }
        }
        self.store(journal::delete_all(removals));
        entries.sort_by(|a, b| {
            b.saved_at_ms
                .cmp(&a.saved_at_ms)
                .then_with(|| a.title.cmp(&b.title))
        });
        self.events.push_back(Event::Recovery { entries });
    }

    /// `suite.file.recovery_open`: lists the journal's values, reads them, then replays it.
    pub(crate) fn recovery_open(&mut self, key: &str) -> Result<ExecResult> {
        if !self.storage.enabled {
            return Err(KilnError::unavailable(
                "suite.file.recovery_open",
                "this window keeps no recovery copies",
            ));
        }
        if self.storage.docs.values().any(|s| s.journal.key() == key) {
            return Err(KilnError::unavailable(
                "suite.file.recovery_open",
                "the document is open already",
            ));
        }
        kiln_storage::store::check_key(key)?;
        let outcome = self.request(
            HostOp::StoreList {
                area: StoreArea::Journal,
                prefix: format!("{key}."),
            },
            Pending::Storage(Wait::List(ListNext::RecoveryOpen {
                key: key.to_owned(),
            })),
        );
        Ok(Self::storage_result(outcome))
    }

    /// The values of a journal to open: read them all (reading its checkpoint makes it this
    /// window's; the host refuses when another window took it meanwhile).
    pub(crate) fn recovery_open_listed(&mut self, key: String, keys: Vec<String>) {
        if !keys.contains(&journal::meta_key(&key)) {
            self.recovery_refused(&taken());
            return;
        }
        self.gather(StoreArea::Journal, keys, Then::RecoveryOpen { key });
    }

    /// The values of a journal read: it is replayed, unless a read failed (another window
    /// opened it meanwhile).
    pub(crate) fn recovery_read(
        &mut self,
        key: &str,
        values: &BTreeMap<String, Option<Vec<u8>>>,
        failed: Option<ErrorInfo>,
    ) {
        if let Some(e) = failed {
            self.recovery_refused(&e);
            return;
        }
        if let Err(e) = self.recovery_replay(key, values) {
            self.recovery_refused(&(&e).into());
        }
    }

    /// Document Recovery cannot open or delete a journal (another window has it, or it is
    /// gone): the user hears why and the list is read again.
    fn recovery_refused(&mut self, e: &ErrorInfo) {
        self.events.push_back(Event::Error {
            doc: None,
            error: ErrorInfo {
                kind: e.kind,
                message: format!("Document Recovery: {}", e.message),
            },
        });
        self.recovery_scan();
    }

    /// Opens a journal: its checkpoint, then every operation after it. The document is marked
    /// as changed and its journal goes on under the same key.
    pub(crate) fn recovery_replay(
        &mut self,
        key: &str,
        values: &BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<()> {
        let get = |k: &str| values.get(k).and_then(Option::as_deref);
        let meta = get(&journal::meta_key(key))
            .ok_or_else(|| KilnError::not_found(format!("the recovery copy {key}")))?;
        let r = journal::recover(meta, get)?;
        let bundle = Bundle::from_bytes(&r.bundle)?;
        let hint = OpenHint {
            name: None,
            path: r.meta.path.clone(),
            format: r.meta.file_format,
            read_only: false,
            kind: Some(r.meta.kind),
            template: false,
        };
        let mut model = self
            .app_for_kind(r.meta.kind)?
            .open_bundle(&bundle, &hint)?;
        model.set_journal_format(r.format);
        let mut replayed = 0usize;
        let mut failed = None;
        for op in &r.ops {
            if let Err(e) = model.apply_journal(op, &super::JournalReplay(())) {
                failed = Some(e);
                break;
            }
            replayed += 1;
        }
        // Text that was being typed: named, not put back.
        let draft = r.draft.as_deref().and_then(|d| model.describe_draft(d));
        // The file's version history, found by the fingerprint the journal kept.
        self.storage.opening.clone_from(&r.meta.print);
        let summary = self.insert(Document {
            kind: r.meta.kind,
            title: r.meta.title.clone(),
            path: r.meta.path.clone(),
            format: r.meta.file_format,
            read_only: false,
            damaged: false,
            revision: 0,
            saved_state: 0,
            viewport: kiln_protocol::Viewport::default(),
            frames: FrameState::new(),
            model,
        });
        let id = summary.doc;
        self.adopt_journal(id, &r);
        let stopped = match &failed {
            // A change that did not replay: the document is as it was before it, and it and the
            // ones after it (in this journal, and past a stop) are named as not kept.
            Some(_) => Some(kiln_protocol::RecoveryStop {
                command: Some(UNREPEATABLE.into()),
                after: u32::try_from(
                    r.ops.len() - replayed - 1 + r.stopped.as_ref().map_or(0, |s| s.after + 1),
                )
                .unwrap_or(u32::MAX),
                at_ms: r.stopped.as_ref().map_or(0, |s| s.time_ms),
            }),
            None => r.stopped.as_ref().map(|s| kiln_protocol::RecoveryStop {
                command: s.missing.clone().filter(|m| !m.is_empty()),
                after: u32::try_from(s.after).unwrap_or(u32::MAX),
                at_ms: s.time_ms,
            }),
        };
        if stopped.is_some() || draft.is_some() {
            self.events.push_back(Event::Recovered {
                doc: id,
                note: kiln_protocol::RecoveryNote { stopped, draft },
            });
        }
        // The list without the document just opened.
        self.recovery_scan();
        Ok(())
    }

    /// A recovered document: its journal goes on under the recovered key, from a checkpoint
    /// of the document as recovered (a record cut short, or one that did not replay, is never
    /// followed by new ones), and it counts as changed until it is saved.
    fn adopt_journal(&mut self, id: DocId, r: &journal::Recovered) {
        if let Some(d) = self.docs.get_mut(&id) {
            d.saved_state = crate::undo::StateId::MAX;
            let state = d.model.state();
            if let Some(st) = self.storage.docs.get_mut(&id) {
                st.journal = Journal::adopt(r);
                #[cfg(test)]
                if let Some(bytes) = self.storage.inline_bytes {
                    st.journal.set_inline_bytes(bytes);
                }
                st.journaled = state;
            }
            // Recording starts afresh: what was replayed is in the checkpoint.
            let _ = d.model.take_journal();
        }
        self.checkpoint(id);
        self.emit_changed(id);
    }

    /// `suite.file.recovery_delete`: lists the journal's values, then removes them, the
    /// marker first (the host refuses when another window opened it meanwhile).
    pub(crate) fn recovery_delete(&mut self, key: &str) -> Result<ExecResult> {
        kiln_storage::store::check_key(key)?;
        if self.storage.docs.values().any(|s| s.journal.key() == key) {
            return Err(KilnError::unavailable(
                "suite.file.recovery_delete",
                "the document is open",
            ));
        }
        let outcome = self.request(
            HostOp::StoreList {
                area: StoreArea::Journal,
                prefix: format!("{key}."),
            },
            Pending::Storage(Wait::List(ListNext::RecoveryDelete {
                key: key.to_owned(),
            })),
        );
        Ok(Self::storage_result(outcome))
    }

    pub(crate) fn recovery_delete_listed(&mut self, key: &str, keys: &[String]) {
        if !keys.contains(&journal::meta_key(key)) {
            // A journal whose first checkpoint never landed: nothing to open, all to delete.
            if keys.contains(&journal::unkept_key(key)) {
                let rest: Vec<String> = keys
                    .iter()
                    .filter(|k| journal::journal_of(k) == key)
                    .cloned()
                    .collect();
                self.store(journal::delete_all(rest));
                self.recovery_scan();
                return;
            }
            self.recovery_refused(&taken());
            return;
        }
        let mut all = journal::keys_of(key, keys).into_iter();
        let Some(marker) = all.next() else { return };
        self.request(
            HostOp::StoreDelete {
                area: StoreArea::Journal,
                key: marker,
            },
            Pending::Storage(Wait::RecoveryDeleted {
                rest: all.collect(),
            }),
        );
    }

    /// The marker of a journal Document Recovery deletes is gone (or could not go).
    pub(crate) fn recovery_deleted(&mut self, rest: Vec<String>, reply: HostReply) {
        if let HostReply::Failed(e) = reply {
            self.recovery_refused(&e);
            return;
        }
        self.store(journal::delete_all(rest));
        self.recovery_scan();
    }
}

/// Why Document Recovery found a journal it listed gone.
fn taken() -> ErrorInfo {
    ErrorInfo {
        kind: kiln_common::ErrorKind::Unavailable,
        message: "the document is open in another window, or was recovered or deleted there".into(),
    }
}
