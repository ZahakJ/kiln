// SPDX-License-Identifier: AGPL-3.0-or-later
//! AutoSave to file: the title bar's switch (Office's AutoSave). With it on, the document's
//! file is written two seconds after the last edit, the way Save writes it. It cannot be on
//! for a document without a file or one that is read-only, and it refuses (or, when a later
//! edit makes it so, turns itself off with a message) where saving in the file's format would
//! lose content: the Compatibility Checker's losses for that format are what Save would warn
//! about (a macro-enabled document saved as `.docx`, a workbook as `.csv`, formatting as
//! `.txt`).
//!
//! Kiln never writes over another program's change of its file: it keeps the stamp (size and
//! modification time) of the file as it last opened, read or wrote it, and before writing the
//! file again (AutoSave or Save) asks for the stamp now. When it moved, nothing is written:
//! AutoSave pauses and `Event::FileChanged` asks the question (Reload, or Keep my version,
//! `suite.file.keep_mine`, after which the waiting save goes ahead). A native host's file
//! watcher asks the same question as soon as the file changes, and AutoSave pauses then too.
//!
//! Closing a document with AutoSave on writes it first (`Close` returns at once; the document
//! closes, `Event::Closed`, once the file is written, and stays open with the error if it could
//! not be).

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    AutoSave, CompatOrigin, CompatSeverity, Event, ExecResult, ExecValue, FileFormat, FileStamp,
    HostOp, HostReply, Outcome,
};

use super::{PendingWrite, Wait};
use crate::session::{Pending, Session};

/// How long AutoSave waits after the last edit, unless Options ▸ Save says otherwise.
pub(crate) const DELAY_MS: u64 = 2_000;

impl Session {
    /// The AutoSave state the title bar shows.
    pub(crate) fn autosave_state(&self, id: DocId) -> AutoSave {
        if !self.storage.enabled {
            return AutoSave::Unavailable("AutoSave is not available here".into());
        }
        if self.storage.docs.get(&id).is_some_and(|s| s.paused) {
            return AutoSave::Unavailable(
                "AutoSave is paused: another program changed the file. Reload it, or keep your \
                 version to go on"
                    .into(),
            );
        }
        if self.storage.autosave(id) {
            return AutoSave::On;
        }
        match self.autosave_blocker(id, false) {
            Some(why) => AutoSave::Unavailable(why),
            None => AutoSave::Off,
        }
    }

    /// Why AutoSave cannot be on (with `losses`, also what saving in the format would lose).
    fn autosave_blocker(&self, id: DocId, losses: bool) -> Option<String> {
        let Ok(d) = self.doc(id) else {
            return Some("the document is closed".into());
        };
        let (Some(_), Some(format)) = (d.path.as_ref(), d.format) else {
            return Some("Save the document to turn on AutoSave".into());
        };
        if d.read_only || d.damaged {
            return Some("The document is read-only".into());
        }
        if !self.save_formats_of(id).contains(&format) {
            return Some(format!(
                "Kiln cannot save .{} files; save it in another format to turn on AutoSave",
                format.extension()
            ));
        }
        if losses {
            let lost: Vec<kiln_protocol::CompatIssue> = d
                .model
                .compat_report(format)
                .into_iter()
                .filter(|i| i.severity == CompatSeverity::Loss && i.origin == CompatOrigin::Save)
                .collect();
            if let Some(first) = lost.first() {
                let macros = lost.iter().any(|i| i.code == "macros.removed");
                return Some(format!(
                    "Saving as .{} would lose content ({}); save it as .{} to use AutoSave",
                    format.extension(),
                    first.message,
                    suggested(d.kind, macros).extension()
                ));
            }
        }
        None
    }

    /// `suite.file.autosave`.
    pub(crate) fn set_autosave(&mut self, id: DocId, on: bool) -> Result<()> {
        if !self.storage.enabled || !self.storage.docs.contains_key(&id) {
            return Err(KilnError::unavailable(
                "suite.file.autosave",
                "AutoSave is not available here",
            ));
        }
        if on && let Some(why) = self.autosave_blocker(id, true) {
            return Err(KilnError::unavailable("suite.file.autosave", why));
        }
        let dirty = self.summary(id).is_ok_and(|s| s.dirty);
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.autosave = on;
        }
        // Unsaved changes are written as soon as it is turned on.
        if on && dirty {
            self.autosave_write(id);
        }
        Ok(())
    }

    /// An edit: the timer starts (or keeps running).
    pub(crate) fn autosave_edit(&mut self, id: DocId) {
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if !st.autosave || st.autosave_timer {
            return;
        }
        st.autosave_timer = true;
        let delay = self.storage.autosave_delay_ms.unwrap_or(DELAY_MS);
        self.arm_autosave(id, delay);
    }

    fn arm_autosave(&mut self, id: DocId, ms: u64) {
        self.request(
            HostOp::Timer {
                ms: u32::try_from(ms).unwrap_or(u32::MAX),
            },
            Pending::Storage(Wait::AutoSave { doc: id }),
        );
    }

    /// The timer: writes the file once two seconds passed since the last edit, otherwise waits
    /// for the rest.
    pub(crate) fn autosave_timer(&mut self, id: DocId) {
        let now = self.host.now_ms();
        let delay = self.storage.autosave_delay_ms.unwrap_or(DELAY_MS);
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if !st.autosave || st.paused {
            st.autosave_timer = false;
            return;
        }
        let since = now.saturating_sub(st.last_edit_ms);
        if since < delay {
            let rest = delay - since;
            self.arm_autosave(id, rest);
            return;
        }
        st.autosave_timer = false;
        self.autosave_write(id);
    }

    /// Writes the file as Save does, unless that would now lose content (AutoSave then turns
    /// off and says why) or another program changed it (the question is waiting).
    pub(crate) fn autosave_write(&mut self, id: DocId) {
        if !self.summary(id).is_ok_and(|s| s.dirty)
            || self.storage.docs.get(&id).is_some_and(|s| s.paused)
        {
            return;
        }
        // A version on show: the document is written, not the version (Save does the same).
        self.storage_show_document(id);
        if let Some(why) = self.autosave_blocker(id, true) {
            if let Some(st) = self.storage.docs.get_mut(&id) {
                st.autosave = false;
            }
            self.events.push_back(Event::Error {
                doc: Some(id),
                error: (&KilnError::unavailable(
                    "suite.file.autosave",
                    format!("AutoSave is off. {why}"),
                ))
                    .into(),
            });
            self.emit_changed(id);
            return;
        }
        let Ok(d) = self.doc(id) else { return };
        let (Some(path), Some(format)) = (d.path.clone(), d.format) else {
            return;
        };
        if let Err(e) = self.write_file(id, path, format, true) {
            if let Some(st) = self.storage.docs.get_mut(&id) {
                st.closing = false;
            }
            self.events.push_back(super::error_event(Some(id), &e));
        }
    }

    // ----- the file's stamp ------------------------------------------------------------------

    /// Asks for the stamp of the document's file (after Kiln opened, read or wrote it).
    pub(crate) fn stat(&mut self, id: DocId) {
        let Some(path) = self.docs.get(&id).and_then(|d| d.path.clone()) else {
            return;
        };
        if !self.storage.docs.contains_key(&id) {
            return;
        }
        self.request(
            HostOp::StatFile { path: path.clone() },
            Pending::Storage(Wait::Stamp { doc: id, path }),
        );
    }

    /// The stamp Kiln left (none from a host that cannot tell).
    pub(crate) fn stamp_read(&mut self, id: DocId, path: &str, reply: &HostReply) {
        let current = self
            .docs
            .get(&id)
            .is_some_and(|d| d.path.as_deref() == Some(path));
        if let Some(st) = self.storage.docs.get_mut(&id)
            && current
        {
            st.stamp = match reply {
                HostReply::Stamp(s) => Some(*s),
                _ => None,
            };
        }
    }

    /// Whether a write of `path` waits for the check of its stamp: Kiln writes the document's
    /// own file and knows the stamp it left there.
    pub(crate) fn write_guarded(&self, id: DocId, path: &str) -> bool {
        self.docs
            .get(&id)
            .is_some_and(|d| d.path.as_deref() == Some(path))
            && self
                .storage
                .docs
                .get(&id)
                .is_some_and(|s| s.stamp.is_some())
    }

    /// Asks for the file's stamp; the write follows when no other program changed it.
    pub(crate) fn check_then_write(&mut self, id: DocId, write: PendingWrite) -> Outcome {
        self.request(
            HostOp::StatFile {
                path: write.path.clone(),
            },
            Pending::Storage(Wait::WriteCheck {
                doc: id,
                write: Box::new(write),
            }),
        )
    }

    /// The stamp before a write: unchanged (or the file is gone, or the host cannot tell), the
    /// file is written; moved, the write waits for the question.
    pub(crate) fn write_checked(&mut self, id: DocId, write: PendingWrite, reply: &HostReply) {
        let known = self.storage.docs.get(&id).and_then(|s| s.stamp);
        let theirs = match reply {
            HostReply::Stamp(now) if known.is_some_and(|k| k != *now) => Some(*now),
            _ => None,
        };
        let Some(theirs) = theirs else {
            self.send_write(id, write);
            return;
        };
        let now = self.host.now_ms();
        if let Some(st) = self.storage.docs.get_mut(&id)
            && write.version.is_some()
        {
            // The save's version is taken when the save goes ahead.
            st.history.note_edit(now);
        }
        self.file_changed(id, Some(theirs), !write.auto);
    }

    /// Another program changed the document's file: nothing is written to it until the
    /// question is answered (`suite.file.reload` or `suite.file.keep_mine`), and the question is
    /// asked. `save_waiting`: an explicit save goes ahead once the user keeps their version.
    pub(crate) fn file_changed(
        &mut self,
        id: DocId,
        theirs: Option<FileStamp>,
        save_waiting: bool,
    ) {
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.paused = true;
            if theirs.is_some() {
                st.theirs = theirs;
            }
            st.save_waiting |= save_waiting;
            st.closing = false;
        }
        self.events.push_back(Event::FileChanged { doc: id });
        self.emit_changed(id);
    }

    /// `suite.file.keep_mine`: this window's version stays; the next write goes over the other
    /// program's change, and a save that waited goes ahead now.
    pub(crate) fn keep_mine(&mut self, id: DocId) -> Result<ExecResult> {
        let Some(st) = self.storage.docs.get_mut(&id).filter(|s| s.paused) else {
            return Ok(ExecResult {
                outcome: Outcome::Unchanged,
                summary: Some(self.summary(id)?),
                value: ExecValue::None,
            });
        };
        st.paused = false;
        st.stamp = st.theirs.take();
        let save = std::mem::take(&mut st.save_waiting);
        let autosave = st.autosave;
        let d = self.doc(id)?;
        if save && let (Some(path), Some(format)) = (d.path.clone(), d.format) {
            self.write_file(id, path, format, false)?;
        } else if autosave {
            self.autosave_write(id);
        }
        self.emit_changed(id);
        Ok(ExecResult {
            outcome: Outcome::Applied,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    /// The question is answered with Reload (the file was read again).
    pub(crate) fn file_reloaded(&mut self, id: DocId) {
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.paused = false;
            st.theirs = None;
            st.save_waiting = false;
        }
        self.stat(id);
    }

    // ----- closing -------------------------------------------------------------------------

    /// `Close`: whether the document does not close now. It waits for its file to be written
    /// (a write already under way, or AutoSave on with changes not written yet, which are
    /// written now), or stays open when those changes could not be written (the error says
    /// why). `false` also when the write finished and closed it at once.
    pub(crate) fn close_waits(&mut self, id: DocId, writing: bool) -> bool {
        let Some(st) = self.storage.docs.get(&id) else {
            return false;
        };
        if st.paused {
            return false;
        }
        let unsaved = st.autosave && self.summary(id).is_ok_and(|s| s.dirty);
        if !writing && !unsaved {
            return false;
        }
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.closing = true;
        }
        if !writing {
            self.autosave_write(id);
        }
        if !self.docs.contains_key(&id) {
            return false;
        }
        if !self.pending.values().any(|p| p.writes(id))
            && let Some(st) = self.storage.docs.get_mut(&id)
        {
            st.closing = false;
        }
        true
    }
}

/// The format that keeps everything of a kind of document (with macros, when it has them).
fn suggested(kind: kiln_protocol::DocKind, macros: bool) -> FileFormat {
    use kiln_protocol::DocKind as K;
    match (kind, macros) {
        (K::Grid, true) => FileFormat::Xlsm,
        (K::Grid, false) => FileFormat::Xlsx,
        (K::Deck, true) => FileFormat::Pptm,
        (K::Deck, false) => FileFormat::Pptx,
        (_, true) => FileFormat::Docm,
        (_, false) => FileFormat::Docx,
    }
}
