// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session: open documents, command dispatch, host continuations and the event queue.

use std::collections::{BTreeMap, VecDeque};

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    A11yTree, App, Availability, Command, CoreInfo, DocKind, DocSummary, Event, ExecResult,
    ExecValue, FileFilter, FileFormat, HostOp, HostReply, HostRequest, OpenHint, Outcome, Query,
    QueryResult, RequestId, SceneDelta, StatusBar, SuiteCommand, ViewMode, Viewport,
};

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
    revision: u64,
    saved_state: StateId,
    viewport: Viewport,
    /// The revision of the last frame handed out.
    framed: Option<u64>,
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
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            framed: None,
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
            revision: 0,
            saved_state: 0,
            viewport: Viewport::default(),
            framed: None,
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
        self.check(doc, &cmd)?;
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
                    (Some(path), Some(format)) if d.model.save_formats().contains(&format) => {
                        self.write(id, path, format)
                    }
                    _ => self.save_as(id, None, None),
                }
            }
            SuiteCommand::SaveAs { path, format } => self.save_as(require(doc)?, path, format),
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
        let d = self.doc_mut(id)?;
        let bytes = d.model.save(format)?;
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
        self.continue_with(pending, reply);
        Ok(())
    }

    fn fail(&mut self, doc: Option<DocId>, error: &KilnError) {
        self.events.push_back(Event::Error {
            doc,
            error: error.into(),
        });
    }

    fn continue_with(&mut self, pending: Pending, reply: HostReply) {
        let doc = match &pending {
            Pending::SavePick { doc, .. } | Pending::SaveWrite { doc, .. } => Some(*doc),
            _ => None,
        };
        match (pending, reply) {
            (_, HostReply::Cancelled) | (Pending::Ignore, _) => {}
            (_, HostReply::Failed(e)) => self.events.push_back(Event::Error { doc, error: e }),
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

    /// The next frame of a document. Until the Scene exists (WP-54) the delta carries only the
    /// revision it was produced for.
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown document.
    pub fn frame(&mut self, doc: DocId) -> Result<SceneDelta> {
        let d = self.doc_mut(doc)?;
        d.framed = Some(d.revision);
        Ok(SceneDelta {
            revision: d.revision,
        })
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
