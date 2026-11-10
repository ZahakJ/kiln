// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recent and pinned files (`Query::Recents`), their thumbnails, and files changed by other
//! programs (`HostOp::WatchFile`, `Event::FileChanged`, `suite.file.reload`).

use std::collections::BTreeMap;

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::print::PrintSettings;
use kiln_protocol::{
    DocKind, Event, ExecResult, HostOp, HostReply, OpenHint, Outcome, RecentFile, StoreArea,
};
use kiln_storage::recents::Recents;

use super::{ListNext, Wait};
use crate::session::{Pending, Session};

/// The width of a thumbnail, in pixels.
const THUMBNAIL_PX: f32 = 240.0;

/// A change to the recent files, applied to them as they are stored (another window may have
/// changed them since this one read them).
#[derive(Debug, Clone)]
pub(crate) enum Change {
    /// A file was opened or saved: on top (a thumbnail replaces the one kept).
    Touch {
        path: String,
        name: String,
        kind: DocKind,
        thumbnail: Option<String>,
    },
    /// Pinned or unpinned.
    Pin { path: String, pinned: bool },
    /// Removed from the list.
    Remove { path: String },
}

impl Session {
    /// `suite.home.recents_load`: reads the recent files (`Event::Recents` follows).
    pub(crate) fn recents_load(&mut self) -> ExecResult {
        if !self.storage.enabled {
            self.storage.recents.get_or_insert_with(Recents::default);
            self.recents_changed();
            return Self::storage_result(Outcome::Applied);
        }
        if self.storage.recents_loading {
            return Self::storage_result(Outcome::Applied);
        }
        self.storage.recents_loading = true;
        let outcome = self.request(
            HostOp::StoreList {
                area: StoreArea::Recents,
                prefix: String::new(),
            },
            Pending::Storage(Wait::List(ListNext::Recents)),
        );
        Self::storage_result(outcome)
    }

    /// The recent files as stored: the changes waiting for them are applied and written.
    pub(crate) fn recents_read(&mut self, values: &BTreeMap<String, Option<Vec<u8>>>) {
        let mut recents = Recents::from_values(values.values().flatten().map(Vec::as_slice));
        self.storage.recents_loading = false;
        let now = self.host.now_ms();
        let mut writes = Vec::new();
        for c in std::mem::take(&mut self.storage.recents_waiting) {
            let ops = match c {
                Change::Touch {
                    path,
                    name,
                    kind,
                    thumbnail,
                } => recents.touch(&path, &name, kind, now, thumbnail),
                // A file another window removed meanwhile stays removed.
                Change::Pin { path, pinned } => recents.pin(&path, pinned),
                Change::Remove { path } => Ok(recents.remove(&path)),
            };
            writes.extend(ops.unwrap_or_default());
        }
        self.storage.recents = Some(recents);
        self.store(writes);
        self.recents_changed();
    }

    fn recents_changed(&mut self) {
        let files = self.recent_files();
        self.events.push_back(Event::Recents { files });
    }

    /// The recent files, pinned first (`Query::Recents`).
    pub(crate) fn recent_files(&self) -> Vec<RecentFile> {
        self.storage
            .recents
            .as_ref()
            .map(Recents::sorted)
            .unwrap_or_default()
    }

    /// Applies a change to the recent files as they are stored now.
    fn recents_change(&mut self, change: Change) {
        if !self.storage.enabled {
            return;
        }
        self.storage.recents_waiting.push(change);
        self.recents_load();
    }

    fn touch_of(&self, id: DocId, thumbnail: Option<String>) -> Option<Change> {
        let d = self.docs.get(&id)?;
        let path = d.path.clone()?;
        // The name shown, with its extension (Office shows "Report.docx").
        let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
        Some(Change::Touch {
            path,
            name,
            kind: d.kind,
            thumbnail,
        })
    }

    /// A file was opened: it goes on top of the recent files.
    pub(crate) fn recents_opened(&mut self, id: DocId) {
        if let Some(c) = self.touch_of(id, None) {
            self.recents_change(c);
        }
    }

    /// A file was saved: on top, with a picture of its first page or slide.
    pub(crate) fn recents_saved(&mut self, id: DocId) {
        let thumbnail = self.thumbnail(id);
        if let Some(c) = self.touch_of(id, thumbnail) {
            self.recents_change(c);
        }
    }

    /// `suite.home.recent_pin`.
    pub(crate) fn recent_pin(&mut self, path: &str, pinned: bool) -> ExecResult {
        self.recents_change(Change::Pin {
            path: path.to_owned(),
            pinned,
        });
        Self::storage_result(Outcome::Applied)
    }

    /// `suite.home.recent_remove`.
    pub(crate) fn recent_remove(&mut self, path: &str) -> ExecResult {
        self.recents_change(Change::Remove {
            path: path.to_owned(),
        });
        Self::storage_result(Outcome::Applied)
    }

    /// A PNG of the first page (sheet, slide) as the print preview draws it, as a data URL.
    /// The web core leaves the raster painter out (budget B12): its recent files have none.
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unused_self)]
    fn thumbnail(&mut self, _id: DocId) -> Option<String> {
        None
    }

    /// A PNG of the first page (sheet, slide) as the print preview draws it, as a data URL.
    #[cfg(not(target_arch = "wasm32"))]
    fn thumbnail(&mut self, id: DocId) -> Option<String> {
        use base64::Engine as _;
        let d = self.docs.get_mut(&id)?;
        d.model.prepare_print();
        let settings = PrintSettings::default();
        // A first pass at a small scale tells the page size; the picture is drawn to width.
        let job = crate::print::PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 0.25,
        };
        let crate::print::PrintOutput::Preview(p) = d.model.print(&job).ok()? else {
            return None;
        };
        let scale = (THUMBNAIL_PX / p.scene.extent.w.max(1.0) * 0.25).clamp(0.05, 1.0);
        let job = crate::print::PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale,
        };
        let crate::print::PrintOutput::Preview(p) = d.model.print(&job).ok()? else {
            return None;
        };
        let s = &p.scene;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            (s.extent.w * s.scale).round().max(1.0) as u32,
            (s.extent.h * s.scale).round().max(1.0) as u32,
        );
        if w > 2048 || h > 4096 {
            return None;
        }
        let view = kiln_raster::View {
            width: w,
            height: h,
            scroll_x: s.extent.x,
            scroll_y: s.extent.y,
            options: kiln_raster::PaintOptions {
                caret_visible: false,
                touch: false,
            },
        };
        let image =
            kiln_raster::render_delta(&p.scene, &view, &kiln_scene::Palette::light()).ok()?;
        let png = image.encode_png().ok()?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    }

    // ----- files changed by other programs ---------------------------------------------------

    /// Watches the document's file (natively; the web cannot watch files), and stops watching
    /// the one it had.
    pub(crate) fn watch(&mut self, id: DocId) {
        let Some(path) = self.docs.get(&id).and_then(|d| d.path.clone()) else {
            return;
        };
        if path.starts_with("opfs:") || path.starts_with("handle:") {
            return;
        }
        let Some(st) = self.storage.docs.get_mut(&id) else {
            return;
        };
        if st.watched.as_deref() == Some(path.as_str()) {
            return;
        }
        let old = st.watched.replace(path.clone());
        if let Some(old) = old {
            self.request(HostOp::UnwatchFile { path: old }, Pending::Ignore);
        }
        self.request(
            HostOp::WatchFile { path: path.clone() },
            Pending::Storage(Wait::Watch { doc: id, path }),
        );
    }

    pub(crate) fn watch_reply(&mut self, id: DocId, path: String, reply: &HostReply) {
        let current = self
            .storage
            .docs
            .get(&id)
            .is_some_and(|s| s.watched.as_deref() == Some(path.as_str()));
        if !current || !matches!(reply, HostReply::Done) {
            // Closed, saved elsewhere, or a host that cannot watch.
            return;
        }
        // AutoSave pauses until the question is answered.
        self.file_changed(id, None, false);
        self.request(
            HostOp::WatchFile { path: path.clone() },
            Pending::Storage(Wait::Watch { doc: id, path }),
        );
    }

    /// `suite.file.reload`: reads the file again (`Event::Changed` follows).
    pub(crate) fn reload(&mut self, id: DocId) -> Result<ExecResult> {
        let path = self.doc(id)?.path.clone().ok_or_else(|| {
            KilnError::unavailable("suite.file.reload", "the document has no file")
        })?;
        let outcome = self.request(
            HostOp::ReadFile { path },
            Pending::Storage(Wait::Reload { doc: id }),
        );
        Ok(ExecResult {
            outcome,
            summary: Some(self.summary(id)?),
            value: kiln_protocol::ExecValue::None,
        })
    }

    /// The file read again replaces the document's model; the changes made here go (the
    /// journal starts afresh).
    pub(crate) fn reloaded(&mut self, id: DocId, bytes: &[u8]) -> Result<()> {
        self.storage_show_document(id);
        let d = self.doc(id)?;
        let format = d.format.ok_or_else(|| {
            KilnError::unavailable("suite.file.reload", "the document has no format")
        })?;
        let hint = OpenHint {
            name: None,
            path: d.path.clone(),
            format: Some(format),
            read_only: d.read_only,
            kind: Some(d.kind),
            template: false,
        };
        let mut model = if format.is_bundle() {
            let b = kiln_storage::Bundle::from_bytes(bytes)?;
            let versions = b.versions.clone();
            let m = self.app_for_kind(d.kind)?.open_bundle(&b, &hint)?;
            self.versions_from_bundle(id, versions);
            m
        } else {
            self.app_for_kind(d.kind)?.open(bytes, format, &hint)?
        };
        if self.storage.docs.contains_key(&id) {
            model.set_journaling(true);
        }
        let d = self.doc_mut(id)?;
        d.model = model;
        d.frames.reset();
        d.revision += 1;
        d.saved_state = d.model.state();
        let state = d.model.state();
        if let Some(st) = self.storage.docs.get_mut(&id) {
            st.journaled = state;
            if st.journal.started() {
                let ops = st.journal.remove();
                st.journal =
                    kiln_storage::journal::Journal::new(kiln_storage::journal::JournalMeta {
                        key: super::new_key(),
                        dirty: false,
                        ..st.journal.meta().clone()
                    });
                self.store(ops);
            }
        }
        self.file_reloaded(id);
        self.emit_changed(id);
        Ok(())
    }
}
