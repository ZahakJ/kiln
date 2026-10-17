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
#[derive(Debug)]
struct Document {
    kind: DocKind,
    title: String,
    path: Option<String>,
    format: Option<FileFormat>,
    read_only: bool,
    /// An internal error (a panic in the core) interrupted a change to this document, so its model
    /// may be half-edited: it only accepts commands that cannot change it, and Save asks for a new
    /// location instead of overwriting the original file.
    damaged: bool,
    revision: u64,
    saved_state: StateId,
    viewport: Viewport,
    /// What the document's painter holds (WP-54).
    frames: FrameState,
    model: Box<dyn DocModel>,
}

/// What a host request continues with when its reply arrives.
#[derive(Debug)]
enum Pending {
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
    },
    /// A request whose reply only needs to be acknowledged (preferences, clipboard writes).
    Ignore,
    /// Export ▸ PDF: save dialog → write the PDF (WP-67).
    PdfPick {
        doc: DocId,
        settings: Box<PrintSettings>,
    },
    /// The PDF was written → `Event::Exported`.
    PdfWrite { doc: DocId, path: String },
    /// Print handed to the host: only a failure is reported.
    Printed { doc: DocId },
    /// Clipboard read → paste into the document (WP-64).
    Paste { doc: DocId, cmd: EditCommand },
    /// Picture dialog → read the chosen image (WP-64).
    PicturePick { doc: DocId },
    /// Read → insert the picture.
    PictureRead { doc: DocId, path: String },
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
            Pending::OpenPick | Pending::OpenRead { .. } | Pending::Ignore => None,
        }
    }
}

/// The facade over the whole core: the only API the bridges use (PLAN.md §3.3).
///
/// A session is single-threaded: natively it lives on its own thread behind
/// [`SessionHandle`](crate::SessionHandle); on the web it lives inside the Worker.
pub struct Session {
    host: Box<dyn Host>,
    apps: Vec<Box<dyn AppFactory>>,
    docs: BTreeMap<DocId, Document>,
    next_doc: u32,
    untitled: BTreeMap<DocKind, u32>,
    events: VecDeque<Event>,
    pending: BTreeMap<RequestId, Pending>,
    next_request: u32,
    /// The document a command or host reply is working on, while it runs (see
    /// [`Session::recover_from_panic`]).
    in_flight: Option<DocId>,
    /// The last content copied in Kiln, with its rich format: a paste whose plain text matches
    /// it pastes the rich format (the system clipboard only carries text and HTML).
    clipboard: Option<ClipboardData>,
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
        Self {
            host,
            apps,
            docs: BTreeMap::new(),
            next_doc: 1,
            untitled: BTreeMap::new(),
            events: VecDeque::new(),
            pending: BTreeMap::new(),
            next_request: 1,
            in_flight: None,
            clipboard: None,
        }
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

    fn app_for_kind(&self, kind: DocKind) -> Result<&dyn AppFactory> {
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

    fn insert(&mut self, mut doc: Document) -> DocSummary {
        let id = DocId(self.next_doc);
        self.next_doc = self.next_doc.saturating_add(1);
        doc.saved_state = doc.model.state();
        self.docs.insert(id, doc);
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
        let model = self.app_for_kind(kind)?.create();
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
        let name = hint.name.clone().or_else(|| hint.path.clone());
        let format = hint
            .format
            .or_else(|| name.as_deref().and_then(FileFormat::from_path))
            .ok_or_else(|| KilnError::Unsupported("a file without a known extension".into()))?;
        let app = self
            .apps
            .iter()
            .find(|a| a.opens(format))
            .ok_or_else(|| KilnError::Unsupported(format!(".{} files", format.extension())))?;
        let kind = app.kind();
        let model = app.open(bytes, format, &hint)?;
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

    fn doc(&self, id: DocId) -> Result<&Document> {
        self.docs.get(&id).ok_or_else(|| KilnError::not_found(id))
    }

    fn doc_mut(&mut self, id: DocId) -> Result<&mut Document> {
        self.docs
            .get_mut(&id)
            .ok_or_else(|| KilnError::not_found(id))
    }

    fn summary_of(&self, id: DocId, d: &Document) -> DocSummary {
        let _ = self;
        DocSummary {
            doc: id,
            kind: d.kind,
            app: d.kind.app(),
            title: d.title.clone(),
            path: d.path.clone(),
            format: d.format,
            revision: d.revision,
            dirty: d.model.state() != d.saved_state,
            read_only: d.read_only,
            can_undo: d.model.can_undo(),
            can_redo: d.model.can_redo(),
            undo_label: d.model.undo_label().map(str::to_owned),
            redo_label: d.model.redo_label().map(str::to_owned),
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

    fn emit_changed(&mut self, doc: DocId) {
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
            Availability::Editable => !d.read_only,
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
    pub fn execute(&mut self, doc: Option<DocId>, cmd: Command) -> Result<ExecResult> {
        self.in_flight = doc;
        let result = self.execute_checked(doc, cmd);
        self.in_flight = None;
        result
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
            SuiteCommand::New { kind } => {
                let summary = self.new_document(kind)?;
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
                let d = self.doc(id)?;
                match (d.path.clone(), d.format) {
                    // A damaged document never overwrites its original file.
                    (Some(path), Some(format))
                        if !d.damaged && d.model.save_formats().contains(&format) =>
                    {
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
            SuiteCommand::Close => {
                let id = require(doc)?;
                self.docs.remove(&id);
                self.pending.retain(|_, p| !matches!(p, Pending::SavePick { doc, .. } | Pending::SaveWrite { doc, .. } if *doc == id));
                self.events.push_back(Event::Closed { doc: id });
                Ok(Self::session_result(Outcome::Applied, ExecValue::None))
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
            // WP-73: chart commands go to the document model, which applies them to its chart
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
            | SuiteCommand::ChartFormatAxis { .. }) => {
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

    // ----- shared editing commands (WP-64) ------------------------------------------------------

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
            .filter(|f| self.apps.iter().any(|a| a.opens(*f)))
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
        let d = self.doc(id)?;
        if let Some(f) = format
            && !d.model.save_formats().contains(&f)
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
                let default = format.unwrap_or(d.model.save_formats()[0]);
                let op = HostOp::PickSave {
                    suggested_name: format!("{}.{}", d.title, default.extension()),
                    filters: d
                        .model
                        .save_formats()
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
        let d = self.doc(id)?;
        let formats = d.model.save_formats();
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

    /// Serialises the document and asks the host to write it.
    fn write(&mut self, id: DocId, path: String, format: FileFormat) -> Result<ExecResult> {
        let now = self.host.now_ms();
        let d = self.doc_mut(id)?;
        let bytes = d.model.save_at(format, now)?;
        d.model.seal_history();
        let state = d.model.state();
        let outcome = self.request(
            HostOp::WriteFile {
                path: path.clone(),
                bytes,
            },
            Pending::SaveWrite {
                doc: id,
                path,
                format,
                state,
            },
        );
        Ok(ExecResult {
            outcome,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    // ----- print and PDF (WP-67) -----------------------------------------------------------------

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
    fn request(&mut self, op: HostOp, pending: Pending) -> Outcome {
        let id = RequestId(self.next_request);
        self.next_request = self.next_request.wrapping_add(1).max(1);
        if let Some(reply) = self.host.request(&op) {
            self.continue_with(pending, reply);
            Outcome::Applied
        } else {
            self.pending.insert(id, pending);
            self.events
                .push_back(Event::HostRequest(HostRequest { id, op }));
            Outcome::Pending(id)
        }
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
        self.continue_with(pending, reply);
        self.in_flight = None;
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
        match (pending, reply) {
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
                },
                HostReply::Done,
            ) => {
                if let Some(d) = self.docs.get_mut(&doc) {
                    d.title = file_stem(&path);
                    d.path = Some(path);
                    d.format = Some(format);
                    d.saved_state = state;
                    let summary = self.summary_of(doc, &self.docs[&doc]);
                    self.events.push_back(Event::Saved { summary });
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
                let cmd = Command::Write(WriteCommand::InsertPicture {
                    name,
                    bytes,
                    floating: false,
                });
                if let Err(e) = self.apply_to_model(doc, &cmd) {
                    self.fail(Some(doc), &e);
                }
            }
            (_, other) => self.fail(
                doc,
                &KilnError::InvalidInput(format!("unexpected host reply {other:?}")),
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
        if !query.needs_document() {
            return Ok(QueryResult::Documents(
                self.docs
                    .iter()
                    .map(|(id, d)| self.summary_of(*id, d))
                    .collect(),
            ));
        }
        let id =
            doc.ok_or_else(|| KilnError::InvalidInput(format!("{query:?} needs a document")))?;
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
            Query::Write(_) => d.model.app_query(query)?,
            Query::Grid(_) => d.model.app_query(query)?,
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
