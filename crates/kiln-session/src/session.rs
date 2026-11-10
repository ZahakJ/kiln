// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session: open documents, command dispatch, host continuations and the event queue.

use std::collections::{BTreeMap, VecDeque};

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    A11yTree, App, Availability, Command, CoreInfo, DocKind, DocSummary, Event, ExecResult,
    ExecValue, FileFilter, FileFormat, HostOp, HostReply, HostRequest, OpenHint, Outcome, PdfFile,
    PrintSettings, Query, QueryResult, RequestId, SceneDelta, StatusBar, SuiteCommand, ViewMode,
    Viewport, WriteCommand,
};

use kiln_protocol::commands::{ClipboardData, EditCommand};

use crate::frame::FrameState;
use crate::host::Host;
use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

/// One open document.
pub(crate) struct Document {
    pub(crate) kind: DocKind,
    pub(crate) title: String,
    pub(crate) path: Option<String>,
    pub(crate) format: Option<FileFormat>,
    pub(crate) read_only: bool,
    /// An internal error (a panic in the core) interrupted a change to this document, so its model
    /// may be half-edited: it only accepts commands that cannot change it, and Save asks for a new
    /// location instead of overwriting the original file.
    pub(crate) damaged: bool,
    pub(crate) revision: u64,
    pub(crate) saved_state: StateId,
    pub(crate) viewport: Viewport,
    /// What the document's painter holds.
    pub(crate) frames: FrameState,
    pub(crate) model: Box<dyn DocModel>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("kind", &self.kind)
            .field("title", &self.title)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

/// What a host request continues with when its reply arrives.
#[derive(Debug)]
pub(crate) enum Pending {
    /// Open dialog → read the chosen file.
    OpenPick,
    /// Read → open the bytes.
    OpenRead { path: String },
    /// Save dialog → write the document.
    SavePick {
        doc: DocId,
        format: Option<FileFormat>,
    },
    /// Write → mark the document saved.
    SaveWrite {
        doc: DocId,
        path: String,
        format: FileFormat,
        state: StateId,
        /// Written by AutoSave.
        auto: bool,
        /// The version an explicit save takes once it is written.
        version: Option<Vec<u8>>,
        /// The fingerprint of what is written (the file's version history follows it).
        print: Option<String>,
    },
    /// A request whose reply only needs to be acknowledged (preferences, clipboard writes).
    Ignore,
    /// Export ▸ PDF: save dialog → write the PDF.
    PdfPick {
        doc: DocId,
        settings: Box<PrintSettings>,
    },
    /// The PDF was written → `Event::Exported`.
    PdfWrite { doc: DocId, path: String },
    /// Print handed to the host: only a failure is reported.
    Printed { doc: DocId },
    /// Clipboard read → paste into the document.
    Paste { doc: DocId, cmd: EditCommand },
    /// Picture dialog → read the chosen image.
    PicturePick { doc: DocId },
    /// Read → insert the picture.
    PictureRead { doc: DocId, path: String },
    /// a shared document's socket or timer.
    Collab(crate::collab::Wait),
    /// app storage, AutoSave and file watching.
    Storage(crate::storage::Wait),
}

impl Pending {
    /// The document the continuation works on.
    fn doc(&self) -> Option<DocId> {
        match self {
            Pending::SavePick { doc, .. }
            | Pending::SaveWrite { doc, .. }
            | Pending::PdfPick { doc, .. }
            | Pending::PdfWrite { doc, .. }
            | Pending::Printed { doc }
            | Pending::Paste { doc, .. }
            | Pending::PicturePick { doc }
            | Pending::PictureRead { doc, .. } => Some(*doc),
            Pending::Collab(w) => Some(w.doc()),
            Pending::Storage(w) => w.doc(),
            Pending::OpenPick | Pending::OpenRead { .. } | Pending::Ignore => None,
        }
    }

    /// Whether the continuation writes document `id`'s file (or checks it first).
    pub(crate) fn writes(&self, id: DocId) -> bool {
        match self {
            Pending::SaveWrite { doc, .. }
            | Pending::Storage(crate::storage::Wait::WriteCheck { doc, .. }) => *doc == id,
            _ => false,
        }
    }
}

/// The facade over the whole core: the only API the bridges use.
///
/// A session is single-threaded: natively it lives on its own thread behind
/// [`SessionHandle`](crate::SessionHandle); on the web it lives inside the Worker.
pub struct Session {
    pub(crate) host: Box<dyn Host>,
    apps: Vec<Box<dyn AppFactory>>,
    pub(crate) docs: BTreeMap<DocId, Document>,
    next_doc: u32,
    /// What each new document's id adds to the previous one (see [`Session::number_documents`]).
    doc_step: u32,
    untitled: BTreeMap<DocKind, u32>,
    pub(crate) events: VecDeque<Event>,
    pub(crate) pending: BTreeMap<RequestId, Pending>,
    next_request: u32,
    /// The document a command or host reply is working on, while it runs (see
    /// [`Session::recover_from_panic`]).
    in_flight: Option<DocId>,
    /// The last content copied in Kiln, with its rich format: a paste whose plain text matches
    /// it pastes the rich format (the system clipboard only carries text and HTML).
    clipboard: Option<ClipboardData>,
    /// the shared documents and their engines.
    pub(crate) collab: crate::collab::Collab,
    /// journals, versions, AutoSave and recents.
    pub(crate) storage: crate::storage::Storage,
    /// the interface language (`CoreOptions::ui_language`), told to every document.
    ui_language: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("docs", &self.docs.keys().collect::<Vec<_>>())
            .field("apps", &self.apps)
            .field("queued_events", &self.events.len())
            .field("pending_requests", &self.pending.len())
            .finish_non_exhaustive()
    }
}

/// The range of zoom factors (10 %–500 %).
const ZOOM: std::ops::RangeInclusive<f32> = SuiteCommand::ZOOM_RANGE;

impl Session {
    /// A session with the built-in apps.
    #[must_use]
    pub fn new(host: Box<dyn Host>) -> Self {
        Self::with_apps(host, crate::apps::builtin())
    }

    /// A session with a custom set of apps (tests, embedders).
    #[must_use]
    pub fn with_apps(host: Box<dyn Host>, apps: Vec<Box<dyn AppFactory>>) -> Self {
        let storage = crate::storage::Storage::new(host.has_storage());
        let mut session = Self {
            host,
            apps,
            docs: BTreeMap::new(),
            next_doc: 1,
            doc_step: 1,
            untitled: BTreeMap::new(),
            events: VecDeque::new(),
            pending: BTreeMap::new(),
            next_request: 1,
            in_flight: None,
            clipboard: None,
            collab: crate::collab::Collab::default(),
            storage,
            ui_language: String::new(),
        };
        session.storage_start();
        session
    }

    /// Numbers the documents opened from now on `first`, `first + step`, `first + 2 × step`, … (by
    /// default 1, 2, 3, …). The web runs one core per app in separate Workers; each numbers its
    /// documents in its own residue class so that ids stay unique across the page. A `first` or
    /// `step` of 0 is taken as 1.
    pub fn number_documents(&mut self, first: u32, step: u32) {
        self.next_doc = first.max(1);
        self.doc_step = step.max(1);
    }

    /// Adds an app (replacing one of the same kind).
    pub fn register_app(&mut self, app: Box<dyn AppFactory>) {
        self.apps.retain(|a| a.kind() != app.kind());
        self.apps.push(app);
    }

    /// The core's version, protocol version and build target.
    #[must_use]
    pub fn core_info() -> CoreInfo {
        CoreInfo {
            version: kiln_common::VERSION.to_owned(),
            protocol: kiln_protocol::PROTOCOL_VERSION,
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
                .trim_end_matches('-')
                .to_owned(),
        }
    }

    // ----- documents -------------------------------------------------------------------------

    pub(crate) fn app_for_kind(&self, kind: DocKind) -> Result<&dyn AppFactory> {
        self.apps
            .iter()
            .find(|a| a.kind() == kind)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                KilnError::Unsupported(format!(
                    "{kind:?} documents are not available in this build"
                ))
            })
    }

    pub(crate) fn insert(&mut self, mut doc: Document) -> DocSummary {
        let id = DocId(self.next_doc);
        self.next_doc = self.next_doc.saturating_add(self.doc_step);
        doc.model.interface_language(&self.ui_language);
        doc.saved_state = doc.model.state();
        self.docs.insert(id, doc);
        // its journal, versions, recent file and watch.
        self.storage_opened(id);
        let summary = self.summary_of(id, &self.docs[&id]);
        self.events.push_back(Event::Opened {
            summary: summary.clone(),
        });
        summary
    }

    /// Creates a new, empty document and emits `Event::Opened`.
    ///
    /// # Errors
    ///
    /// `Unsupported` when no app creates `kind`.
    pub fn new_document(&mut self, kind: DocKind) -> Result<DocSummary> {
        self.new_document_in(kind, None)
    }

    /// Creates a new, empty document whose text is in `language` (the default editing language,
    /// BCP 47) and emits `Event::Opened`. A Write document takes it as its document default and,
    /// for a language written from right to left, starts its paragraphs from the right.
    ///
    /// # Errors
    ///
    /// `Unsupported` when no app creates `kind`, `InvalidInput` for a malformed language tag.
    pub fn new_document_in(&mut self, kind: DocKind, language: Option<&str>) -> Result<DocSummary> {
        let app = self.app_for_kind(kind)?;
        let model = match language.map(str::trim).filter(|l| !l.is_empty()) {
            Some(language) => {
                if !is_language_tag(language) {
                    return Err(KilnError::InvalidInput(format!(
                        "{language:?} is not a language tag"
                    )));
                }
                app.create_in(language)
            }
            None => app.create(),
        };
        let n = self.untitled.entry(kind).or_insert(0);
        *n += 1;
        // the name in the document's language (Dokument1, مستند1).
        let title = format!("{}{n}", crate::props::untitled_stem(kind, language));
        Ok(self.insert(Document {
            kind,
            title,
            path: None,
            format: None,
            read_only: false,
            damaged: false,
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            frames: FrameState::new(),
            model,
        }))
    }

    /// Creates a new, untitled document from a template (a native bundle: `.kwrite`, `.kgrid`,
    /// `.kdeck`) and emits `Event::Opened`. The document takes the template's content but not
    /// its file: it has no path or format (Save asks where to put it), its title is the next
    /// untitled name of its kind, and the template's version history stays behind.
    ///
    /// # Errors
    ///
    /// `Format` for bytes that are not a bundle, `Unsupported` when no app opens its kind.
    pub fn new_from_template(&mut self, bytes: &[u8]) -> Result<DocSummary> {
        let bundle = kiln_storage::Bundle::from_bytes(bytes)?;
        let kind = bundle.kind;
        let model = self
            .app_for_kind(kind)?
            .open_bundle(&bundle, &OpenHint::default())?;
        let n = self.untitled.entry(kind).or_insert(0);
        *n += 1;
        let title = format!("{}{n}", kind.untitled_stem());
        Ok(self.insert(Document {
            kind,
            title,
            path: None,
            format: None,
            read_only: false,
            damaged: false,
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            frames: FrameState::new(),
            model,
        }))
    }

    /// Opens a file from its bytes and emits `Event::Opened`.
    ///
    /// # Errors
    ///
    /// `Unsupported` for an unknown or unsupported format, `Format` for a damaged file.
    pub fn open(&mut self, bytes: &[u8], hint: OpenHint) -> Result<DocSummary> {
        // File ▸ New from a template.
        if hint.template {
            return self.new_from_template(bytes);
        }
        let name = hint.name.clone().or_else(|| hint.path.clone());
        let format = hint
            .format
            .or_else(|| name.as_deref().and_then(FileFormat::from_path))
            .ok_or_else(|| KilnError::Unsupported("a file without a known extension".into()))?;
        // Kiln's own bundles name their kind inside.
        if format.is_bundle() {
            return self.open_bundle(bytes, hint, format, name.as_deref());
        }
        let app = match hint.kind {
            Some(kind) => self
                .apps
                .iter()
                .find(|a| a.kind() == kind && a.opens(format)),
            None => self
                .apps
                .iter()
                .find(|a| !a.internal() && a.opens(format))
                .or_else(|| self.apps.iter().find(|a| a.opens(format))),
        }
        .ok_or_else(|| KilnError::Unsupported(format!(".{} files", format.extension())))?;
        let kind = app.kind();
        let model = app.open(bytes, format, &hint)?;
        // the file's contents find its local version history.
        if self.storage.enabled && hint.path.is_some() {
            self.storage.opening = Some(kiln_storage::versions::fingerprint(bytes));
        }
        let title = name
            .as_deref()
            .map(file_stem)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| kind.untitled_stem().to_owned());
        Ok(self.insert(Document {
            kind,
            title,
            path: hint.path,
            format: Some(format),
            read_only: hint.read_only,
            damaged: false,
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            frames: FrameState::new(),
            model,
        }))
    }

    /// Opens a native bundle (`.kwrite`, `.kgrid`, `.kdeck`) with its versions.
    fn open_bundle(
        &mut self,
        bytes: &[u8],
        hint: OpenHint,
        format: FileFormat,
        name: Option<&str>,
    ) -> Result<DocSummary> {
        let bundle = kiln_storage::Bundle::from_bytes(bytes)?;
        let kind = bundle.kind;
        if hint.kind.is_some_and(|k| k != kind) {
            return Err(KilnError::Format {
                format: format.extension().to_owned(),
                message: format!("the file holds a {kind:?} document"),
            });
        }
        let model = self.app_for_kind(kind)?.open_bundle(&bundle, &hint)?;
        let title = name
            .map(file_stem)
            .filter(|t| !t.is_empty())
            .or_else(|| bundle.title.clone())
            .unwrap_or_else(|| kind.untitled_stem().to_owned());
        // The bundle of another kind keeps its own format (a .kgrid holds a workbook).
        let format = FileFormat::bundle_of(kind).unwrap_or(format);
        let summary = self.insert(Document {
            kind,
            title,
            path: hint.path,
            format: Some(format),
            read_only: hint.read_only,
            damaged: false,
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            frames: FrameState::new(),
            model,
        });
        self.versions_from_bundle(summary.doc, bundle.versions);
        Ok(summary)
    }

    /// The formats a document saves to: its model's, then its native bundle.
    pub(crate) fn save_formats_of(&self, id: DocId) -> Vec<FileFormat> {
        let Some(d) = self.docs.get(&id) else {
            return Vec::new();
        };
        let model = self.storage.live(id).unwrap_or(d.model.as_ref());
        let mut out = model.save_formats().to_vec();
        if model.bundles()
            && let Some(b) = FileFormat::bundle_of(d.kind)
        {
            out.push(b);
        }
        out
    }

    pub(crate) fn doc(&self, id: DocId) -> Result<&Document> {
        self.docs.get(&id).ok_or_else(|| KilnError::not_found(id))
    }

    pub(crate) fn doc_mut(&mut self, id: DocId) -> Result<&mut Document> {
        self.docs
            .get_mut(&id)
            .ok_or_else(|| KilnError::not_found(id))
    }

    fn summary_of(&self, id: DocId, d: &Document) -> DocSummary {
        // while a version is shown, the document's own model says what changed.
        let live = self.storage.live(id).unwrap_or(d.model.as_ref());
        DocSummary {
            doc: id,
            kind: d.kind,
            app: d.kind.app(),
            title: d.title.clone(),
            path: d.path.clone(),
            format: d.format,
            revision: d.revision,
            dirty: live.state() != d.saved_state,
            read_only: d.read_only || self.collab.locks(id) || self.storage.locks(id),
            can_undo: d.model.can_undo(),
            can_redo: d.model.can_redo(),
            undo_label: d.model.undo_label().map(str::to_owned),
            redo_label: d.model.redo_label().map(str::to_owned),
            save_formats: self.save_formats_of(id),
            autosave: self.autosave_state(id),
        }
    }

    /// The summary of a document.
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown document.
    pub fn summary(&self, doc: DocId) -> Result<DocSummary> {
        Ok(self.summary_of(doc, self.doc(doc)?))
    }

    fn status_of(d: &Document) -> StatusBar {
        StatusBar {
            zoom: d.viewport.zoom,
            view: d.viewport.mode,
            ..d.model.status()
        }
    }

    pub(crate) fn emit_changed(&mut self, doc: DocId) {
        if let Some(d) = self.docs.get(&doc) {
            let event = Event::Changed {
                summary: self.summary_of(doc, d),
                selection: d.model.selection(),
                status: Self::status_of(d),
            };
            self.events.push_back(event);
        }
    }

    // ----- commands --------------------------------------------------------------------------

    /// Checks a command against its metadata: target document, app and availability.
    fn check(&self, doc: Option<DocId>, cmd: &Command) -> Result<()> {
        let meta = cmd.meta();
        if !meta.availability.needs_document() {
            return Ok(());
        }
        let id =
            doc.ok_or_else(|| KilnError::InvalidInput(format!("{} needs a document", meta.id)))?;
        let d = self.doc(id)?;
        let app = d.kind.app();
        if d.damaged
            && !matches!(
                meta.availability,
                Availability::Always | Availability::Document
            )
        {
            return Err(KilnError::unavailable(
                meta.id,
                "an internal error interrupted an edit, so the document is read-only; save a copy to keep your work",
            ));
        }
        if !meta.apps.contains(app) {
            return Err(KilnError::unavailable(
                meta.id,
                format!("not a command of Kiln {}", app_name(app)),
            ));
        }
        let ok = match meta.availability {
            Availability::Always | Availability::Document => true,
            Availability::Editable => {
                !d.read_only && !self.collab.locks(id) && !self.storage.locks(id)
            }
            Availability::CanUndo => d.model.can_undo(),
            Availability::CanRedo => d.model.can_redo(),
            Availability::Selection => d.model.selection().is_non_empty(),
        };
        if ok {
            Ok(())
        } else {
            let reason = match meta.availability {
                Availability::Editable => "the document is read-only",
                Availability::CanUndo => "there is nothing to undo",
                Availability::CanRedo => "there is nothing to redo",
                _ => "nothing is selected",
            };
            Err(KilnError::unavailable(meta.id, reason))
        }
    }

    /// Executes a command on a document (`doc: Some`) or the session (`None`), after checking it
    /// against its `CommandMeta`. Emits `Changed`/`Opened`/`Closed`/`Saved` events as appropriate.
    ///
    /// # Errors
    ///
    /// `NotFound` (unknown document), `InvalidInput` (bad arguments, missing document),
    /// `Unavailable` (wrong app, read-only, nothing to undo), `Unsupported`.
    pub fn execute(&mut self, doc: Option<DocId>, mut cmd: Command) -> Result<ExecResult> {
        // saving, printing and exporting a shared document write the document, not a
        // version on show.
        if let (
            Some(id),
            Command::Suite(
                SuiteCommand::Save
                | SuiteCommand::SaveAs { .. }
                | SuiteCommand::PrintPdf { .. }
                | SuiteCommand::SavePdf { .. },
            ),
        ) = (doc, &cmd)
        {
            self.collab_show_document(id);
            // the same for a local version on show.
            self.storage_show_document(id);
        }
        // the user dictionary keeps the words a command adds.
        self.storage_command(&mut cmd);
        self.storage.changing = Some(cmd.id().to_owned());
        self.in_flight = doc;
        let result = self.execute_checked(doc, cmd);
        self.in_flight = None;
        if let Some(id) = doc {
            self.take_notices(id);
            // what the command changed goes to the others.
            self.collab_after(id);
            // and to the autosave journal.
            self.storage_after(id);
        }
        self.storage.changing = None;
        result
    }

    /// Turns what a model has to tell every window into events.
    fn take_notices(&mut self, id: DocId) {
        let Ok(d) = self.doc_mut(id) else { return };
        for n in d.model.take_notices() {
            let event = match n {
                crate::model::ModelNotice::Show(status) => Event::Show { doc: id, status },
            };
            self.events.push_back(event);
        }
    }

    fn execute_checked(&mut self, doc: Option<DocId>, cmd: Command) -> Result<ExecResult> {
        self.check(doc, &cmd)?;
        if let Some(id) = doc {
            let now = self.host.now_ms();
            if let Some(d) = self.docs.get_mut(&id) {
                d.model.set_clock(now);
            }
        }
        match cmd {
            Command::Suite(s) => self.execute_suite(doc, s),
            app_command => {
                let id = doc.ok_or_else(|| KilnError::InvalidInput("missing document".into()))?;
                let d = self.doc_mut(id)?;
                let change = d.model.execute(&app_command)?;
                if change == Change::Content {
                    d.revision += 1;
                }
                self.finish(id, change != Change::None)
            }
        }
    }

    /// Emits `Changed` when something changed and builds the result.
    fn finish(&mut self, doc: DocId, changed: bool) -> Result<ExecResult> {
        if changed {
            self.emit_changed(doc);
        }
        Ok(ExecResult {
            outcome: if changed {
                Outcome::Applied
            } else {
                Outcome::Unchanged
            },
            summary: Some(self.summary(doc)?),
            value: ExecValue::None,
        })
    }

    /// File ▸ Options as the core applies them: AutoSave's wait, the recovery journal of
    /// documents opened from now on, and the words the speller skips.
    fn set_options(&mut self, o: kiln_protocol::commands::CoreOptions) -> ExecResult {
        let delay = u64::from(o.autosave_delay_s.clamp(1, 600)) * 1_000;
        self.storage.autosave_delay_ms = Some(delay);
        self.storage.no_recovery = !o.recovery;
        let spelling = kiln_spell::set_skip_rules(kiln_spell::SkipRules {
            uppercase: o.spell_ignore_uppercase,
            numbers: o.spell_ignore_numbers,
            addresses: o.spell_ignore_addresses,
        });
        let language = o.ui_language != self.ui_language;
        if language {
            self.ui_language = o.ui_language;
        }
        let ids: Vec<DocId> = self.docs.keys().copied().collect();
        for id in ids {
            let Some(d) = self.docs.get_mut(&id) else {
                continue;
            };
            let mut changed = false;
            if spelling {
                d.model.spelling_changed();
                changed = true;
            }
            if language {
                changed |= d.model.interface_language(&self.ui_language);
            }
            if changed {
                d.revision += 1;
                self.emit_changed(id);
            }
        }
        Self::session_result(Outcome::Applied, ExecValue::None)
    }

    fn session_result(outcome: Outcome, value: ExecValue) -> ExecResult {
        ExecResult {
            outcome,
            summary: None,
            value,
        }
    }

    fn execute_suite(&mut self, doc: Option<DocId>, cmd: SuiteCommand) -> Result<ExecResult> {
        match cmd {
            SuiteCommand::Ping => Ok(Self::session_result(
                Outcome::Unchanged,
                ExecValue::Pong(Self::core_info()),
            )),
            SuiteCommand::Echo { text } => Ok(Self::session_result(
                Outcome::Unchanged,
                ExecValue::Echo(text),
            )),
            SuiteCommand::Repaint => {
                let id = require(doc)?;
                self.doc_mut(id)?.frames.reset();
                Ok(ExecResult {
                    outcome: Outcome::Applied,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                })
            }
            SuiteCommand::New { kind, language } => {
                let summary = self.new_document_in(kind, language.as_deref())?;
                Ok(ExecResult {
                    outcome: Outcome::Applied,
                    summary: Some(summary),
                    value: ExecValue::None,
                })
            }
            SuiteCommand::Open { path } => {
                let (op, pending) = match path {
                    Some(path) => (
                        HostOp::ReadFile { path: path.clone() },
                        Pending::OpenRead { path },
                    ),
                    None => (
                        HostOp::PickOpen {
                            filters: self.open_filters(),
                        },
                        Pending::OpenPick,
                    ),
                };
                let outcome = self.request(op, pending);
                Ok(Self::session_result(outcome, ExecValue::None))
            }
            // The commands below were checked to have a document.
            SuiteCommand::Save => {
                let id = require(doc)?;
                let formats = self.save_formats_of(id);
                let d = self.doc(id)?;
                match (d.path.clone(), d.format) {
                    // A damaged document never overwrites its original file.
                    (Some(path), Some(format)) if !d.damaged && formats.contains(&format) => {
                        self.write(id, path, format)
                    }
                    _ => self.save_as(id, None, None),
                }
            }
            SuiteCommand::SaveAs { path, format } => self.save_as(require(doc)?, path, format),
            SuiteCommand::PrintPdf { settings } => self.print_pdf(require(doc)?, &settings),
            SuiteCommand::SavePdf { path, settings } => {
                self.save_pdf(require(doc)?, path, settings)
            }
            // a second window on the document (the desktop's Presenter View).
            SuiteCommand::OpenWindow { window } => {
                let id = require(doc)?;
                let title = format!("{} - Slide Show", self.doc(id)?.title);
                let outcome = self.request(
                    HostOp::OpenWindow {
                        route: window.route(u64::from(id.0)),
                        title,
                    },
                    Pending::Ignore,
                );
                Ok(ExecResult {
                    outcome,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                })
            }
            // Version History of a document that is not shared is kept locally.
            version @ (SuiteCommand::VersionPreview { .. }
            | SuiteCommand::VersionRestore { .. }
            | SuiteCommand::VersionRename { .. })
                if doc.is_some_and(|d| !self.collab.is_shared(d)) =>
            {
                self.version_command(require(doc)?, version)
            }
            // recovery, AutoSave, recents.
            SuiteCommand::SetOptions { options } => Ok(self.set_options(options)),
            SuiteCommand::SetProperties { props } => {
                let id = require(doc)?;
                let changed = self.doc_mut(id)?.model.set_properties(&props)?;
                self.finish(id, changed)
            }
            SuiteCommand::InspectRemove { item } => {
                let id = require(doc)?;
                let d = self.doc_mut(id)?;
                let changed = d.model.inspect_remove(item)?;
                if changed {
                    d.revision += 1;
                }
                self.finish(id, changed)
            }
            SuiteCommand::RecoveryScan => Ok(self.recovery_scan()),
            SuiteCommand::RecoveryOpen { key, .. } => self.recovery_open(&key),
            SuiteCommand::RecoveryDelete { key } => self.recovery_delete(&key),
            SuiteCommand::SetAutoSave { on } => {
                let id = require(doc)?;
                self.set_autosave(id, on)?;
                self.finish(id, true)
            }
            SuiteCommand::RecentsLoad => Ok(self.recents_load()),
            SuiteCommand::RecentPin { path, pinned } => Ok(self.recent_pin(&path, pinned)),
            SuiteCommand::RecentRemove { path } => Ok(self.recent_remove(&path)),
            SuiteCommand::Reload => self.reload(require(doc)?),
            // collaboration and Version History.
            collab @ (SuiteCommand::Share { .. }
            | SuiteCommand::JoinShared { .. }
            | SuiteCommand::StopSharing
            | SuiteCommand::VersionPreview { .. }
            | SuiteCommand::VersionRestore { .. }
            | SuiteCommand::VersionRename { .. }) => self.collab_command(doc, collab),
            SuiteCommand::Close => self.close_doc(require(doc)?, false),
            SuiteCommand::CloseWithoutSaving => self.close_doc(require(doc)?, true),
            SuiteCommand::KeepMine => self.keep_mine(require(doc)?),
            SuiteCommand::Undo | SuiteCommand::Redo
                if doc.is_some_and(|d| self.collab.is_shared(d)) =>
            {
                let id = require(doc)?;
                let done = self.collab_undo(id, cmd == SuiteCommand::Redo)?;
                self.finish(id, done)
            }
            SuiteCommand::Undo | SuiteCommand::Redo => {
                let id = require(doc)?;
                let d = self.doc_mut(id)?;
                let done = if cmd == SuiteCommand::Undo {
                    d.model.undo()
                } else {
                    d.model.redo()
                };
                if done {
                    d.revision += 1;
                }
                self.finish(id, done)
            }
            SuiteCommand::SetZoom { zoom } => {
                if !zoom.is_finite() || !ZOOM.contains(&zoom) {
                    return Err(KilnError::InvalidInput(format!(
                        "zoom {zoom} is outside {}–{}",
                        ZOOM.start(),
                        ZOOM.end()
                    )));
                }
                let id = require(doc)?;
                let d = self.doc_mut(id)?;
                let changed = d.viewport.zoom != zoom;
                d.viewport.zoom = zoom;
                self.finish(id, changed)
            }
            // chart commands go to the document model, which applies them to its chart
            // through `kiln-charts` (Grid, Write and Deck host charts).
            chart @ (SuiteCommand::ChartInsert { .. }
            | SuiteCommand::ChartChangeType { .. }
            | SuiteCommand::ChartSwitchRowColumn
            | SuiteCommand::ChartSelectData { .. }
            | SuiteCommand::ChartSetElement { .. }
            | SuiteCommand::ChartSetLegend { .. }
            | SuiteCommand::ChartSetDataLabels { .. }
            | SuiteCommand::ChartQuickLayout { .. }
            | SuiteCommand::ChartSetStyle { .. }
            | SuiteCommand::ChartSetColors { .. }
            | SuiteCommand::ChartSetTitle { .. }
            | SuiteCommand::ChartMove { .. }
            | SuiteCommand::ChartFormatSeries { .. }
            | SuiteCommand::ChartFormatAxis { .. }
            | SuiteCommand::ChartEditSeries { .. }
            | SuiteCommand::ChartHiddenEmpty { .. }
            | SuiteCommand::ChartFormatArea { .. }) => {
                let id = require(doc)?;
                let d = self.doc_mut(id)?;
                let change = d.model.execute(&Command::Suite(chart))?;
                if change == Change::Content {
                    d.revision += 1;
                }
                self.finish(id, change != Change::None)
            }
            SuiteCommand::Edit(edit) => self.execute_edit(require(doc)?, edit),
            SuiteCommand::SetViewMode { mode } => {
                let id = require(doc)?;
                let d = self.doc_mut(id)?;
                if !view_fits(d.kind.app(), mode) {
                    return Err(KilnError::InvalidInput(format!(
                        "{mode:?} is not a view of Kiln {}",
                        app_name(d.kind.app())
                    )));
                }
                let changed = d.viewport.mode != mode;
                d.viewport.mode = mode;
                self.finish(id, changed)
            }
        }
    }

    // ----- shared editing commands ------------------------------------------------------

    /// Runs a shared editing command: clipboard reads and the picture dialog go through the host
    /// first; everything else goes to the document model, and what a copy produced goes to the
    /// system clipboard.
    fn execute_edit(&mut self, id: DocId, cmd: EditCommand) -> Result<ExecResult> {
        match cmd {
            EditCommand::InsertPictures => {
                let outcome = self.request(
                    HostOp::PickOpen {
                        filters: vec![FileFilter {
                            name: "Pictures".into(),
                            extensions: ["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"]
                                .into_iter()
                                .map(str::to_owned)
                                .collect(),
                        }],
                    },
                    Pending::PicturePick { doc: id },
                );
                return Ok(ExecResult {
                    outcome,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                });
            }
            ref paste if paste_data(paste).is_some_and(Option::is_none) => {
                let outcome = self.request(HostOp::ClipboardRead, Pending::Paste { doc: id, cmd });
                return Ok(ExecResult {
                    outcome,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                });
            }
            _ => {}
        }
        let cmd = self.with_rich_clipboard(cmd);
        self.apply_to_model(id, &Command::Suite(SuiteCommand::Edit(cmd)))
    }

    /// Adds Kiln's rich format to pasted data whose text is what Kiln copied last.
    fn with_rich_clipboard(&self, mut cmd: EditCommand) -> EditCommand {
        if let Some(Some(data)) = paste_data_mut(&mut cmd)
            && data.kiln.is_none()
            && let Some(own) = &self.clipboard
            && normalise_newlines(&own.text) == normalise_newlines(&data.text)
        {
            data.kiln.clone_from(&own.kiln);
        }
        cmd
    }

    /// Executes an app command on a document and sends what it copied to the clipboard.
    fn apply_to_model(&mut self, id: DocId, cmd: &Command) -> Result<ExecResult> {
        let d = self.doc_mut(id)?;
        let change = d.model.execute(cmd)?;
        if change == Change::Content {
            d.revision += 1;
        }
        let copied = d.model.take_clipboard();
        if let Some(c) = copied {
            let op = HostOp::ClipboardWrite {
                text: c.text.clone(),
                html: c.html.clone(),
            };
            self.clipboard = Some(c);
            self.request(op, Pending::Ignore);
        }
        self.finish(id, change != Change::None)
    }

    fn open_filters(&self) -> Vec<FileFilter> {
        let mut extensions: Vec<String> = FileFormat::ALL
            .into_iter()
            .filter(|f| {
                self.apps
                    .iter()
                    .any(|a| a.opens(*f) || FileFormat::bundle_of(a.kind()) == Some(*f))
            })
            .map(|f| f.extension().to_owned())
            .collect();
        extensions.dedup();
        vec![FileFilter {
            name: "All supported files".into(),
            extensions,
        }]
    }

    fn save_as(
        &mut self,
        id: DocId,
        path: Option<String>,
        format: Option<FileFormat>,
    ) -> Result<ExecResult> {
        let formats = self.save_formats_of(id);
        let d = self.doc(id)?;
        if let Some(f) = format
            && !formats.contains(&f)
        {
            return Err(KilnError::Unsupported(format!(
                "saving {:?} documents as .{}",
                d.kind,
                f.extension()
            )));
        }
        match path {
            Some(path) => {
                let format = self.resolve_save_format(id, &path, format)?;
                self.write(id, path, format)
            }
            None => {
                let default = format.unwrap_or(formats[0]);
                let op = HostOp::PickSave {
                    suggested_name: format!("{}.{}", d.title, default.extension()),
                    filters: formats
                        .iter()
                        .map(|f| FileFilter {
                            name: format!("{f:?}"),
                            extensions: vec![f.extension().to_owned()],
                        })
                        .collect(),
                };
                let outcome = self.request(op, Pending::SavePick { doc: id, format });
                Ok(ExecResult {
                    outcome,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                })
            }
        }
    }

    fn resolve_save_format(
        &self,
        id: DocId,
        path: &str,
        format: Option<FileFormat>,
    ) -> Result<FileFormat> {
        let formats = self.save_formats_of(id);
        let formats = formats.as_slice();
        let d = self.doc(id)?;
        let chosen = format
            .or_else(|| FileFormat::from_path(path).filter(|f| formats.contains(f)))
            .or(d.format.filter(|f| formats.contains(f)))
            .unwrap_or(formats[0]);
        if formats.contains(&chosen) {
            Ok(chosen)
        } else {
            Err(KilnError::Unsupported(format!(
                ".{} for this document",
                chosen.extension()
            )))
        }
    }

    /// Closes a document. Unless `discard` (Don't Save), a document with AutoSave on is
    /// written first and closes once it is, and a document with unsaved changes keeps
    /// its recovery copy; a clean close removes it.
    pub(crate) fn close_doc(&mut self, id: DocId, discard: bool) -> Result<ExecResult> {
        self.doc(id)?;
        if !discard {
            let writing = self.pending.values().any(|p| p.writes(id));
            if self.close_waits(id, writing) {
                return Ok(Self::session_result(Outcome::Applied, ExecValue::None));
            }
            if !self.docs.contains_key(&id) {
                // Written and closed at once.
                return Ok(Self::session_result(Outcome::Applied, ExecValue::None));
            }
        }
        let clean = discard || !self.summary(id).is_ok_and(|s| s.dirty);
        self.collab_close(id);
        // a clean close: the journal goes.
        self.storage_closed(id, clean);
        self.docs.remove(&id);
        self.pending.retain(|_, p| {
            !matches!(p, Pending::SavePick { doc, .. } if *doc == id) && !p.writes(id)
        });
        self.events.push_back(Event::Closed { doc: id });
        Ok(Self::session_result(Outcome::Applied, ExecValue::None))
    }

    /// Serialises the document and asks the host to write it.
    fn write(&mut self, id: DocId, path: String, format: FileFormat) -> Result<ExecResult> {
        self.write_file(id, path, format, false)
    }

    /// Serialises the document and asks the host to write it; `auto` for AutoSave (an
    /// explicit save takes a version and makes a thumbnail, AutoSave does neither).
    pub(crate) fn write_file(
        &mut self,
        id: DocId,
        path: String,
        format: FileFormat,
        auto: bool,
    ) -> Result<ExecResult> {
        let version = if auto {
            None
        } else {
            self.versions_before_save(id, format)
        };
        if format.is_bundle() {
            return self.write_bundle(id, path, format, auto);
        }
        let now = self.host.now_ms();
        let bytes = self.doc(id)?.model.save_at(format, now)?;
        self.write_bytes(id, path, format, bytes, auto, version)
    }

    /// Asks the host to write a serialised document.
    pub(crate) fn write_bytes(
        &mut self,
        id: DocId,
        path: String,
        format: FileFormat,
        bytes: Vec<u8>,
        auto: bool,
        version: Option<Vec<u8>>,
    ) -> Result<ExecResult> {
        let d = self.doc_mut(id)?;
        d.model.seal_history();
        let state = d.model.state();
        let write = crate::storage::PendingWrite {
            path,
            format,
            bytes,
            auto,
            version,
            state,
        };
        // Kiln's own file is written only if no other program changed it meanwhile.
        let outcome = if self.write_guarded(id, &write.path) {
            self.check_then_write(id, write)
        } else {
            self.send_write(id, write)
        };
        Ok(ExecResult {
            outcome,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    /// Asks the host to write a document's file.
    pub(crate) fn send_write(&mut self, id: DocId, w: crate::storage::PendingWrite) -> Outcome {
        let print = (self.storage.enabled && !w.format.is_bundle())
            .then(|| kiln_storage::versions::fingerprint(&w.bytes));
        // the file's stamp is Kiln's own until this write is done and stamped again; a
        // save meanwhile is not checked against the stamp before it.
        if self
            .docs
            .get(&id)
            .is_some_and(|d| d.path.as_deref() == Some(w.path.as_str()))
        {
            self.storage.forget_stamp(id);
        }
        self.request(
            HostOp::WriteFile {
                path: w.path.clone(),
                bytes: w.bytes,
            },
            Pending::SaveWrite {
                doc: id,
                path: w.path,
                format: w.format,
                state: w.state,
                auto: w.auto,
                version: w.version,
                print,
            },
        )
    }

    // ----- print and PDF -----------------------------------------------------------------

    /// [`Session::query`] for hosts that hold the session mutably: a print preview or PDF
    /// lets the model prepare first ([`DocModel::prepare_print`]).
    ///
    /// # Errors
    ///
    /// As [`Session::query`].
    pub fn query_mut(&mut self, doc: Option<DocId>, query: &Query) -> Result<QueryResult> {
        // Version History of a document that is not shared is kept locally.
        if let Some(result) = self.storage_query(doc, query) {
            return result;
        }
        // collaboration and Version History ask the engine.
        if let Some(result) = self.collab_query(doc, query) {
            return result;
        }
        if let (Some(id), Query::PrintPreview { .. } | Query::Pdf { .. }) = (doc, query) {
            self.collab_show_document(id);
            self.storage_show_document(id);
            if let Ok(d) = self.doc_mut(id) {
                d.model.prepare_print();
            }
        }
        if let Some(id) = doc
            && let Ok(d) = self.doc_mut(id)
        {
            d.model.prepare_query(query);
        }
        let result = self.query(doc, query);
        if let Some(id) = doc {
            self.take_notices(id);
        }
        result
    }

    /// The PDF of a document printed with `settings`.
    fn pdf(&self, id: DocId, settings: &PrintSettings) -> Result<PdfFile> {
        let d = self.doc(id)?;
        let job = crate::print::PrintJob::Pdf {
            settings,
            title: &d.title,
        };
        match d.model.print(&job)? {
            crate::print::PrintOutput::Pdf(mut f) => {
                f.name = crate::print::pdf_name(&d.title);
                Ok(f)
            }
            crate::print::PrintOutput::Preview(_) => {
                Err(KilnError::Internal("a preview instead of a PDF".into()))
            }
        }
    }

    /// Print: natively a temporary PDF the system viewer opens (and prints from its dialog); on
    /// the web the page prints the bytes from a hidden frame.
    fn print_pdf(&mut self, id: DocId, settings: &PrintSettings) -> Result<ExecResult> {
        self.doc_mut(id)?.model.prepare_print();
        let f = self.pdf(id, settings)?;
        #[cfg(not(target_arch = "wasm32"))]
        let op = HostOp::OpenFile {
            path: crate::print::write_temp(&f)?,
        };
        #[cfg(target_arch = "wasm32")]
        let op = HostOp::PrintPdf {
            name: f.name,
            bytes: f.bytes,
        };
        let outcome = self.request(op, Pending::Printed { doc: id });
        Ok(ExecResult {
            outcome,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    /// Export ▸ PDF to a path, or through the save dialog first.
    fn save_pdf(
        &mut self,
        id: DocId,
        path: Option<String>,
        settings: PrintSettings,
    ) -> Result<ExecResult> {
        match path {
            Some(path) => self.write_pdf(id, path, &settings),
            None => {
                let title = self.doc(id)?.title.clone();
                let op = HostOp::PickSave {
                    suggested_name: crate::print::pdf_name(&title),
                    filters: vec![FileFilter {
                        name: "PDF".into(),
                        extensions: vec!["pdf".into()],
                    }],
                };
                let outcome = self.request(
                    op,
                    Pending::PdfPick {
                        doc: id,
                        settings: Box::new(settings),
                    },
                );
                Ok(ExecResult {
                    outcome,
                    summary: Some(self.summary(id)?),
                    value: ExecValue::None,
                })
            }
        }
    }

    fn write_pdf(
        &mut self,
        id: DocId,
        path: String,
        settings: &PrintSettings,
    ) -> Result<ExecResult> {
        self.doc_mut(id)?.model.prepare_print();
        let f = self.pdf(id, settings)?;
        let outcome = self.request(
            HostOp::WriteFile {
                path: path.clone(),
                bytes: f.bytes,
            },
            Pending::PdfWrite { doc: id, path },
        );
        Ok(ExecResult {
            outcome,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    // ----- host --------------------------------------------------------------------------------

    /// Sends `op` to the host; continues at once when it answers synchronously, otherwise emits
    /// `Event::HostRequest` and returns `Outcome::Pending`.
    pub(crate) fn request(&mut self, op: HostOp, pending: Pending) -> Outcome {
        let id = RequestId(self.next_request);
        self.next_request = self.next_request.wrapping_add(1).max(1);
        if let Some(reply) = self.host.request(&op) {
            self.continue_with(pending, reply);
            Outcome::Applied
        } else {
            self.pending.insert(id, pending);
            // a host that answers later itself (a WebSocket's next message natively).
            if !self.host.request_later(id, &op) {
                self.events
                    .push_back(Event::HostRequest(HostRequest { id, op }));
            }
            Outcome::Pending(id)
        }
    }

    /// gives the host the way to deliver the replies it answers later (the session
    /// thread does this for its session).
    pub fn attach_host(&mut self, deliver: crate::host::Deliver) {
        self.host.attach(deliver);
    }

    /// Delivers the host's reply to request `id`.
    ///
    /// # Errors
    ///
    /// `NotFound` when no request with this id is pending.
    pub fn host_reply(&mut self, id: RequestId, reply: HostReply) -> Result<()> {
        let pending = self
            .pending
            .remove(&id)
            .ok_or_else(|| KilnError::not_found(format!("host request {}", id.0)))?;
        self.in_flight = pending.doc();
        let edit = match &pending {
            Pending::Collab(_) | Pending::Storage(_) => None,
            other => other.doc(),
        };
        self.continue_with(pending, reply);
        self.in_flight = None;
        // an edit a reply finished (a paste, a picture) goes to the others.
        if let Some(id) = edit {
            self.collab_after(id);
            // and to the autosave journal.
            self.storage_after(id);
        }
        Ok(())
    }

    /// Called by the session thread after a job panicked (natively panics unwind and are caught
    /// per job). If the panic interrupted a command or host reply on a document, that document's
    /// model may be half-edited: it is marked damaged and read-only (it still answers queries,
    /// renders and can be saved under a new name, but accepts no further edits). An `Error` event
    /// reports the failure either way.
    pub fn recover_from_panic(&mut self, message: &str) {
        let doc = self
            .in_flight
            .take()
            .filter(|id| self.docs.contains_key(id));
        let error = match doc {
            Some(id) => {
                if let Some(d) = self.docs.get_mut(&id) {
                    d.damaged = true;
                    d.read_only = true;
                }
                KilnError::Internal(format!(
                    "{message} (the document is now read-only; save a copy to keep your work)"
                ))
            }
            None => KilnError::Internal(message.to_owned()),
        };
        self.fail(doc, &error);
        if let Some(id) = doc {
            self.emit_changed(id);
        }
    }

    fn fail(&mut self, doc: Option<DocId>, error: &KilnError) {
        self.events.push_back(Event::Error {
            doc,
            error: error.into(),
        });
    }

    fn continue_with(&mut self, pending: Pending, reply: HostReply) {
        let doc = pending.doc();
        // a document waiting to close stays open when its file could not be written, and
        // the file's stamp is read again (the write may have changed it before it failed).
        if let (Pending::SaveWrite { doc, .. }, HostReply::Failed(_)) = (&pending, &reply)
            && let Some(st) = self.storage.docs.get_mut(doc)
        {
            st.closing = false;
            let doc = *doc;
            self.stat(doc);
        }
        match (pending, reply) {
            (Pending::Collab(wait), reply) => self.collab_reply(wait, reply),
            (Pending::Storage(wait), reply) => self.storage_reply(wait, reply),
            (Pending::PdfPick { doc, .. }, HostReply::Cancelled) => {
                self.events.push_back(Event::Exported { doc, path: None });
            }
            (_, HostReply::Cancelled) | (Pending::Ignore, _) => {}
            (_, HostReply::Failed(e)) => self.events.push_back(Event::Error { doc, error: e }),
            (Pending::PdfPick { doc, settings }, HostReply::Path(path)) => {
                if let Err(e) = self.write_pdf(doc, path, &settings) {
                    self.fail(Some(doc), &e);
                }
            }
            (Pending::PdfWrite { doc, path }, HostReply::Done) => {
                self.events.push_back(Event::Exported {
                    doc,
                    path: Some(path),
                });
            }
            (Pending::Printed { .. }, HostReply::Done) => {}
            (Pending::OpenPick, HostReply::Path(path)) => {
                self.request(
                    HostOp::ReadFile { path: path.clone() },
                    Pending::OpenRead { path },
                );
            }
            (Pending::OpenRead { path }, HostReply::Bytes(bytes)) => {
                let hint = OpenHint {
                    name: None,
                    path: Some(path),
                    format: None,
                    read_only: false,
                    kind: None,
                    template: false,
                };
                if let Err(e) = self.open(&bytes, hint) {
                    self.fail(None, &e);
                }
            }
            (Pending::SavePick { doc, format }, HostReply::Path(path)) => {
                let result = self
                    .resolve_save_format(doc, &path, format)
                    .and_then(|f| self.write(doc, path, f));
                if let Err(e) = result {
                    self.fail(Some(doc), &e);
                }
            }
            (
                Pending::SaveWrite {
                    doc,
                    path,
                    format,
                    state,
                    auto,
                    version,
                    print,
                },
                HostReply::Done,
            ) => {
                if let Some(d) = self.docs.get_mut(&doc) {
                    d.title = file_stem(&path);
                    d.path = Some(path);
                    d.format = Some(format);
                    d.saved_state = state;
                    // the journal, the version, the recent file and the watch.
                    self.storage_saved(doc, auto, version, state, print);
                    let summary = self.summary_of(doc, &self.docs[&doc]);
                    self.events.push_back(Event::Saved { summary });
                    // a save of a shared document is a version.
                    self.collab_saved(doc);
                    // Close with AutoSave on waited for this write.
                    if self.storage.closing(doc) {
                        let _ = self.close_doc(doc, false);
                    }
                }
            }
            (Pending::Paste { doc, mut cmd }, HostReply::Clipboard { text, html }) => {
                if let Some(slot) = paste_data_mut(&mut cmd) {
                    *slot = Some(ClipboardData {
                        text,
                        html,
                        kiln: None,
                    });
                }
                let cmd = self.with_rich_clipboard(cmd);
                if let Err(e) = self.apply_to_model(doc, &Command::Suite(SuiteCommand::Edit(cmd))) {
                    self.fail(Some(doc), &e);
                }
            }
            (Pending::PicturePick { doc }, HostReply::Path(path)) => {
                self.request(
                    HostOp::ReadFile { path: path.clone() },
                    Pending::PictureRead { doc, path },
                );
            }
            (Pending::PictureRead { doc, path }, HostReply::Bytes(bytes)) => {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
                // Kiln Deck puts pictures on the slide.
                let deck = self.docs.get(&doc).is_some_and(|d| d.kind == DocKind::Deck);
                let grid = self.docs.get(&doc).is_some_and(|d| d.kind == DocKind::Grid);
                let cmd = if grid {
                    // Kiln Grid puts pictures over the cells.
                    Command::Grid(kiln_protocol::commands::GridCommand::InsertPicture {
                        name,
                        bytes,
                    })
                } else if deck {
                    Command::Deck(kiln_protocol::commands::DeckCommand::InsertPicture {
                        name,
                        bytes,
                        rect: None,
                    })
                } else {
                    Command::Write(WriteCommand::InsertPicture {
                        name,
                        bytes,
                        floating: false,
                    })
                };
                if let Err(e) = self.apply_to_model(doc, &cmd) {
                    self.fail(Some(doc), &e);
                }
            }
            (_, other) => self.fail(
                doc,
                &KilnError::InvalidInput({
                    let _ = other;
                    "an unexpected host reply".to_owned()
                }),
            ),
        }
    }

    // ----- queries, viewport, frames, a11y, events ---------------------------------------------

    /// Answers a query about the session (`doc: None`) or a document.
    ///
    /// # Errors
    ///
    /// `InvalidInput` without a needed document, `NotFound`, `Unsupported` for paragraphs of a
    /// model without paragraphs.
    pub fn query(&self, doc: Option<DocId>, query: &Query) -> Result<QueryResult> {
        // the recent files.
        if matches!(query, Query::Recents) {
            return Ok(QueryResult::Recents(self.recent_files()));
        }
        if !query.needs_document() {
            return Ok(QueryResult::Documents(
                self.docs
                    .iter()
                    .map(|(id, d)| self.summary_of(*id, d))
                    .collect(),
            ));
        }
        let id = doc.ok_or_else(|| KilnError::InvalidInput("the query needs a document".into()))?;
        let d = self.doc(id)?;
        Ok(match query {
            Query::Documents | Query::Summary => QueryResult::Summary(self.summary_of(id, d)),
            Query::Paragraphs => {
                QueryResult::Paragraphs(d.model.paragraphs().ok_or_else(|| {
                    KilnError::Unsupported(format!("paragraphs of {:?} documents", d.kind))
                })?)
            }
            Query::Selection => QueryResult::Selection(d.model.selection()),
            Query::Status => QueryResult::Status(Self::status_of(d)),
            Query::A11y => QueryResult::A11y(d.model.a11y(&d.title)),
            Query::A11yDocument => QueryResult::A11y(d.model.a11y_document(&d.title)),
            Query::SelectedText => QueryResult::Text(d.model.selected_text().unwrap_or_default()),
            Query::PrintPreview {
                settings,
                sheet,
                scale,
            } => {
                let job = crate::print::PrintJob::Preview {
                    settings,
                    sheet: *sheet,
                    scale: *scale,
                };
                match d.model.print(&job)? {
                    crate::print::PrintOutput::Preview(p) => QueryResult::PrintPreview(p),
                    crate::print::PrintOutput::Pdf(_) => {
                        return Err(KilnError::Internal("a PDF instead of a preview".into()));
                    }
                }
            }
            Query::Pdf { settings } => QueryResult::Pdf(self.pdf(id, settings)?),
            Query::Recents => QueryResult::Recents(self.recent_files()),
            Query::Collab
            | Query::Versions
            | Query::VersionCompare { .. }
            | Query::VersionSlides { .. } => {
                return Err(KilnError::Unsupported(
                    "collaboration queries go through query_mut".into(),
                ));
            }
            Query::Write(_) => d.model.app_query(query)?,
            Query::Grid(_) => d.model.app_query(query)?,
            Query::Deck(_) => d.model.app_query(query)?,
            Query::Properties => {
                QueryResult::Properties(Box::new(d.model.properties().ok_or_else(|| {
                    KilnError::Unsupported(format!("properties of {:?} documents", d.kind))
                })?))
            }
            Query::Inspect => QueryResult::Inspect(d.model.inspect()),
            Query::CompatReport { format } => {
                let format = format
                    .or(d.format.filter(|f| d.model.save_formats().contains(f)))
                    .or_else(|| d.model.save_formats().first().copied());
                QueryResult::CompatReport(
                    format.map(|f| d.model.compat_report(f)).unwrap_or_default(),
                )
            }
        })
    }

    /// Sets the viewport of a document's surface.
    ///
    /// # Errors
    ///
    /// `NotFound`, or `InvalidInput` for non-finite or non-positive numbers.
    pub fn set_viewport(&mut self, doc: DocId, viewport: Viewport) -> Result<()> {
        if !viewport.is_valid() {
            return Err(KilnError::InvalidInput(format!(
                "invalid viewport {viewport:?}"
            )));
        }
        let d = self.doc_mut(doc)?;
        if !view_fits(d.kind.app(), viewport.mode) {
            return Err(KilnError::InvalidInput(format!(
                "{:?} is not a view of Kiln {}",
                viewport.mode,
                app_name(d.kind.app())
            )));
        }
        let changed = d.viewport != viewport;
        d.viewport = viewport;
        if changed {
            self.emit_changed(doc);
        }
        Ok(())
    }

    /// The next frame of a document for its current viewport: the tiles, atlas pages and overlay
    /// its painter lacks (see [`crate::frame`]). Documents that do not paint yet get frames that
    /// carry only the revision.
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown document.
    pub fn frame(&mut self, doc: DocId) -> Result<SceneDelta> {
        let now = self.host.now_ms();
        let d = self.doc_mut(doc)?;
        d.model.set_clock(now);
        let viewport = d.viewport;
        let revision = d.revision;
        d.frames
            .produce(d.model.frame_source(), &viewport, revision)
    }

    /// The accessibility tree of a document.
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown document.
    pub fn a11y(&self, doc: DocId) -> Result<A11yTree> {
        let d = self.doc(doc)?;
        Ok(d.model.a11y(&d.title))
    }

    /// Takes every queued event, oldest first.
    pub fn poll(&mut self) -> Vec<Event> {
        self.events.drain(..).collect()
    }

    /// Whether events are waiting.
    #[must_use]
    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Stores a UI preference through the host (fire and forget).
    pub fn store_pref(&mut self, key: String, value: String) {
        self.request(HostOp::StorePref { key, value }, Pending::Ignore);
    }

    /// Milliseconds since the Unix epoch, from the host.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.host.now_ms()
    }
}

/// The clipboard data of a paste command (`None` for other commands).
fn paste_data(cmd: &EditCommand) -> Option<&Option<ClipboardData>> {
    match cmd {
        EditCommand::Paste { data }
        | EditCommand::PasteKeepSource { data }
        | EditCommand::PasteMerge { data }
        | EditCommand::PasteTextOnly { data }
        | EditCommand::PasteAs { data, .. } => Some(data),
        _ => None,
    }
}

fn paste_data_mut(cmd: &mut EditCommand) -> Option<&mut Option<ClipboardData>> {
    match cmd {
        EditCommand::Paste { data }
        | EditCommand::PasteKeepSource { data }
        | EditCommand::PasteMerge { data }
        | EditCommand::PasteTextOnly { data }
        | EditCommand::PasteAs { data, .. } => Some(data),
        _ => None,
    }
}

/// Text with `\r\n` and `\r` as `\n` (clipboards differ per platform).
fn normalise_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// A BCP 47 tag's shape: subtags of 1–8 ASCII letters or digits joined by hyphens, a language
/// subtag of letters first (`ar-SA`, `de-DE`, `zh-Hant-TW`).
fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split(['-', '_']);
    let primary = parts.next().unwrap_or_default();
    tag.len() <= 64
        && (2..=8).contains(&primary.len())
        && primary.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn require(doc: Option<DocId>) -> Result<DocId> {
    doc.ok_or_else(|| KilnError::InvalidInput("the command needs a document".into()))
}

fn app_name(app: App) -> &'static str {
    match app {
        App::Write => "Write",
        App::Grid => "Grid",
        App::Deck => "Deck",
        App::Notes => "Notes",
        App::Home => "Home",
    }
}

/// Whether a view belongs to an app.
fn view_fits(app: App, mode: ViewMode) -> bool {
    use ViewMode as V;
    match app {
        App::Write | App::Notes => matches!(
            mode,
            V::PrintLayout | V::WebLayout | V::ReadMode | V::Outline | V::Draft
        ),
        App::Grid => matches!(mode, V::Normal | V::PageLayout | V::PageBreakPreview),
        App::Deck => matches!(
            mode,
            V::SlideNormal | V::SlideSorter | V::NotesPage | V::ReadingView
        ),
        App::Home => false,
    }
}

/// The file name without directory and extension.
fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("/a/b/Report.final.docx"), "Report.final");
        assert_eq!(file_stem("C:\\x\\notes.txt"), "notes");
        assert_eq!(file_stem(".bashrc"), ".bashrc");
        assert_eq!(file_stem("README"), "README");
    }

    #[test]
    fn documents_can_be_numbered_in_steps() {
        let mut s = Session::new(Box::new(crate::DeferredHost::new(|| 0)));
        s.number_documents(2, 4);
        let a = s.new_document(DocKind::Scratch).unwrap();
        let b = s.new_document(DocKind::Scratch).unwrap();
        assert_eq!((a.doc, b.doc), (DocId(2), DocId(6)));
        s.number_documents(0, 0);
        assert_eq!(s.new_document(DocKind::Scratch).unwrap().doc, DocId(1));
    }

    #[test]
    fn views_belong_to_apps() {
        assert!(view_fits(App::Write, ViewMode::Draft));
        assert!(!view_fits(App::Write, ViewMode::SlideSorter));
        assert!(view_fits(App::Grid, ViewMode::PageBreakPreview));
        assert!(view_fits(App::Deck, ViewMode::NotesPage));
        assert!(!view_fits(App::Home, ViewMode::Normal));
        assert_eq!(app_name(App::Notes), "Notes");
    }

    #[test]
    fn core_info_names_the_target() {
        let info = Session::core_info();
        assert_eq!(info.version, kiln_common::VERSION);
        assert_eq!(info.protocol, kiln_protocol::PROTOCOL_VERSION);
        assert!(info.target.contains(std::env::consts::ARCH));
    }
}
