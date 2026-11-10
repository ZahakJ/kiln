// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write documents: `.docx`, `.dotx` and `.docm` files read by `kiln-io-docx` into
//! the `kiln-doc` model, laid out and painted by `kiln-doc-layout`.
//!
//! - **Viewport and frames**: the document paints through the Scene (pages as tiles with the
//!   theme's page shadow, pictures, shapes); print layout, web layout, read mode and draft
//!   follow the viewport's mode; zoom and DPR scale the tiles. The first frame lays out the
//!   first pages only; later frames finish the rest.
//! - **Editing** ([`editing`]): typing, deletion, caret motion that follows lines across pages,
//!   columns and tables, word/paragraph selection by clicks, IME composition — every change a
//!   `kiln-doc` transaction through the undo history, laid out incrementally from the first
//!   changed paragraph.
//! - **Hit testing, caret and selection geometry, accessibility** of the visible pages ± 1.
//!
//! - **Saving** ([`save`]): `.docx`, `.dotx` and `.docm` through the DOCX writer; the
//!   parts no edit touched are copied byte for byte from the file that was opened. The
//!   Compatibility Checker lists what a save loses or changes.
//!
//! - **The editor** ([`commands`]): formatting, styles, lists, tables, page setup,
//!   headers and footers, the clipboard, Find and Replace, and the queries of the editor's
//!   panes and rulers (`Query::Write`).

mod commands;
mod editing;
mod export;
pub(crate) mod features;
pub(crate) mod office;
#[cfg(test)]
mod rtl_tests;
mod save;
mod shared;
/// native bundles and the autosave journal.
mod storage;
#[cfg(test)]
mod tests;
mod textio;
mod view;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Block, Document, Paragraph, Pos, Transaction, UndoStack};
use kiln_io_docx::{DocxError, DocxWarning, TouchedParts, read_docx};
use kiln_protocol::geom::Rect;
use kiln_protocol::{
    A11yTree, Caret, Command, DocKind, FileFormat, Handle, HandleKind, OpenHint, ParagraphText,
    Position, Selection, StatusBar, TextPos,
};
use kiln_scene::{AtlasUpdate, Bounds, ColorRole, ImageUpload, Paint, Scene};

use crate::frame::{FrameRequest, FrameSource};
use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

pub use view::WriteView;

/// The formats Write opens: Word's, plain text and Markdown; `.odt` only with the `odf` feature
/// (the web core leaves it out).
#[cfg(feature = "odf")]
const OPEN_FORMATS: [FileFormat; 7] = [
    FileFormat::Docx,
    FileFormat::Dotx,
    FileFormat::Docm,
    FileFormat::Dotm,
    FileFormat::Odt,
    FileFormat::Txt,
    FileFormat::Md,
];
#[cfg(not(feature = "odf"))]
const OPEN_FORMATS: [FileFormat; 6] = [
    FileFormat::Docx,
    FileFormat::Dotx,
    FileFormat::Docm,
    FileFormat::Dotm,
    FileFormat::Txt,
    FileFormat::Md,
];

/// The formats Write saves, the default first; Markdown and HTML are exports of the structure.
#[cfg(feature = "odf")]
const SAVE_FORMATS: [FileFormat; 8] = [
    FileFormat::Docx,
    FileFormat::Dotx,
    FileFormat::Docm,
    FileFormat::Dotm,
    FileFormat::Odt,
    FileFormat::Txt,
    FileFormat::Md,
    FileFormat::Html,
];
#[cfg(not(feature = "odf"))]
const SAVE_FORMATS: [FileFormat; 7] = [
    FileFormat::Docx,
    FileFormat::Dotx,
    FileFormat::Docm,
    FileFormat::Dotm,
    FileFormat::Txt,
    FileFormat::Md,
    FileFormat::Html,
];

/// Creates and opens Write documents.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteApp;

impl AppFactory for WriteApp {
    fn kind(&self) -> DocKind {
        DocKind::Write
    }

    fn opens(&self, format: FileFormat) -> bool {
        OPEN_FORMATS.contains(&format)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(WriteDoc::new(Document::new(), Vec::new(), None))
    }

    fn create_in(&self, language: &str) -> Box<dyn DocModel> {
        // The language only: the paper stays that of a new document.
        let doc = Document::with_options(&kiln_doc::NewDocumentOptions {
            locale: Some(language.to_owned()),
            paper: Some(kiln_doc::Paper::for_locale("en-US")),
        });
        Box::new(WriteDoc::new(doc, Vec::new(), None))
    }

    fn open_bundle(
        &self,
        bundle: &kiln_storage::Bundle,
        hint: &OpenHint,
    ) -> Result<Box<dyn DocModel>> {
        let name = hint.path.clone().or_else(|| hint.name.clone());
        Ok(Box::new(WriteDoc::from_bundle(bundle, name)?))
    }

    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>> {
        if !self.opens(format) {
            return Err(KilnError::Unsupported(format!(
                ".{} files in Kiln Write",
                format.extension()
            )));
        }
        #[cfg(feature = "odf")]
        if format == FileFormat::Odt {
            let read = kiln_io_odf::read_odt(bytes).map_err(|e| KilnError::Format {
                format: format.extension().to_owned(),
                message: e.to_string(),
            })?;
            let name = hint.path.clone().or_else(|| hint.name.clone());
            return Ok(Box::new(WriteDoc::new(read.document, Vec::new(), name)));
        }
        let name = hint.path.clone().or_else(|| hint.name.clone());
        if format == FileFormat::Txt {
            let detection = textio::detect(bytes);
            let (document, text_format) = textio::read_as(bytes, detection.encoding);
            let mut doc = WriteDoc::new(document, Vec::new(), name);
            doc.text_format = Some(text_format);
            doc.text_source = Some(TextSource {
                bytes: bytes.into(),
                detection,
            });
            return Ok(Box::new(doc));
        }
        if format == FileFormat::Md {
            let (text, _) = textio::decode(bytes);
            let mut doc = WriteDoc::new(Document::new(), Vec::new(), name);
            doc.import_markdown(&text)?;
            return Ok(Box::new(doc));
        }
        let read = read_docx(bytes).map_err(|e| KilnError::Format {
            format: format.extension().to_owned(),
            message: match e {
                DocxError::Package(p) => p.to_string(),
                DocxError::NotWord(m) => format!("not a Word document ({m})"),
            },
        })?;
        let (document, warnings, original) = save::Original::from_read(read, bytes);
        let mut doc = WriteDoc::new(document, warnings, name);
        doc.original = Some(original);
        Ok(Box::new(doc))
    }
}

/// An IME composition in progress: its text is in the document, undone by `inverse`.
#[derive(Debug, Clone)]
struct Composition {
    at: Pos,
    len: usize,
    inverse: Transaction,
}

/// The plain text file a document opened from.
#[derive(Debug)]
struct TextSource {
    bytes: std::sync::Arc<[u8]>,
    detection: textio::Detection,
}

/// A Write document held by the session.
#[derive(Debug)]
pub struct WriteDoc {
    doc: Document,
    /// What the reader reported (the Compatibility Checker's input).
    warnings: Vec<DocxWarning>,
    file_name: Option<String>,
    history: UndoStack,
    anchor: Pos,
    head: Pos,
    goal_x: Option<f32>,
    composition: Option<Composition>,
    /// Layout and scene, created when a frame or geometry is first needed.
    view: Option<Box<WriteView>>,
    /// History state ids: the applied steps, then the undone ones.
    states: Vec<StateId>,
    undone: Vec<StateId>,
    state: StateId,
    next_state: StateId,
    /// The file the document was opened from, for saving.
    original: Option<save::Original>,
    /// The parts the edits since opening touched.
    touched: TouchedParts,
    /// a bundle's original Word file could not be read (saving as Word loses the parts
    /// the model does not cover).
    original_lost: bool,
    /// The layout for print, kept between previews.
    print: crate::print::PrintCache,
    /// The editor's state: formatting marks, pending formatting, the command log.
    editor: commands::EditorState,
    /// Comments, track changes, references, objects, spelling.
    features: features::Features,
    /// How the plain text file the document was opened from was written (encoding, line ends).
    text_format: Option<textio::TextFormat>,
    /// The text file's bytes and what its detection found (File Conversion).
    text_source: Option<TextSource>,
    /// AutoCorrect, multiple selected objects, page movement, balloons.
    office: office::OfficeState,
    /// the document is shared (collaboration and Version History).
    shared: Option<Box<shared::SharedDoc>>,
    /// the transactions applied since the session last took them, for the autosave
    /// journal (while it records).
    journal: Option<Vec<Vec<u8>>>,
}

impl WriteDoc {
    fn new(doc: Document, warnings: Vec<DocxWarning>, file_name: Option<String>) -> Self {
        let first = doc.body_paragraphs().first().map_or(NodeId(0), |p| p.id);
        Self {
            doc,
            warnings,
            file_name,
            history: UndoStack::default(),
            anchor: Pos::new(first, 0),
            head: Pos::new(first, 0),
            goal_x: None,
            composition: None,
            view: None,
            states: Vec::new(),
            undone: Vec::new(),
            state: 0,
            next_state: 1,
            original: None,
            touched: TouchedParts::new(),
            original_lost: false,
            print: crate::print::PrintCache::new(),
            editor: commands::EditorState::default(),
            features: features::Features::default(),
            text_format: None,
            text_source: None,
            office: office::OfficeState::default(),
            shared: None,
            journal: None,
        }
    }

    /// The document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The import diagnostics.
    #[must_use]
    pub fn warnings(&self) -> &[DocxWarning] {
        &self.warnings
    }

    /// The layout view, if one exists.
    #[must_use]
    pub fn view(&self) -> Option<&WriteView> {
        self.view.as_deref()
    }

    /// The layout view, created and brought up to date (all pages).
    pub(crate) fn view_synced(&mut self) -> &mut WriteView {
        let doc = &self.doc;
        let file = self.file_name.clone();
        let now = view::date_time(self.editor.now_ms.unwrap_or(0));
        // the review display (markup modes) decides the layout's options.
        let (markup, review) = self.review_options();
        let v = self
            .view
            .get_or_insert_with(|| Box::new(WriteView::new(doc, file, now)));
        let mut o = v.layout.options().clone();
        if o.markup != markup || o.review != review || o.side_to_side != self.office.side_to_side {
            o.markup = markup;
            o.review = review;
            o.side_to_side = self.office.side_to_side;
            v.layout.set_options(o);
        }
        v.sync(doc, true);
        v
    }

    fn bump_state(&mut self, new_step: bool) {
        let id = self.next_state;
        self.next_state += 1;
        if new_step || self.states.is_empty() {
            self.states.push(id);
        } else if let Some(top) = self.states.last_mut() {
            // Typing merged into the open step: a new state all the same.
            *top = id;
        }
        self.undone.clear();
        self.state = id;
    }

    fn wire(p: Pos) -> TextPos {
        TextPos {
            block: p.para,
            offset: u32::try_from(p.offset).unwrap_or(u32::MAX),
        }
    }

    fn overlay(&self) -> Vec<kiln_scene::DrawOp> {
        let Some(v) = &self.view else {
            return Vec::new();
        };
        let comp = self.composition.as_ref().map(|c| (c.at, c.len));
        let mut ops = Vec::new();
        if self.editor.show_marks {
            ops.extend(commands::marks::marks(v, &self.doc));
        }
        // change bars, balloon connectors and spelling squiggles under the selection.
        ops.extend(self.feature_overlay());
        // A selected picture, shape or text box shows its frame (the chrome's), not a tint.
        if self.selected_object().is_none() {
            ops.extend(v.overlay(&self.doc, self.anchor, self.head, comp));
        }
        // the others' carets and selections.
        ops.extend(self.shared_overlay());
        ops
    }
}

fn flatten<'a>(blocks: &'a [Block], out: &mut Vec<&'a Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        flatten(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => flatten(&s.blocks, out),
            Block::Field(f) => flatten(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

impl WriteDoc {
    /// Body paragraphs in reading order (text boxes excluded).
    fn body(&self) -> Vec<&Paragraph> {
        let mut out = Vec::new();
        for s in &self.doc.sections {
            flatten(&s.blocks, &mut out);
        }
        out
    }

    /// One step of the undo history.
    fn undo_step(&mut self) -> bool {
        match self.history.undo(&mut self.doc) {
            Ok(Some(tx)) => {
                self.record_journal(&tx);
                self.touched.record(&self.doc, &tx);
                self.note_edit(&tx);
                if let Some(r) = tx.selection_after {
                    self.anchor = r.anchor;
                    self.head = r.head;
                }
                if let Some(s) = self.states.pop() {
                    self.undone.push(s);
                }
                self.state = self.states.last().copied().unwrap_or(0);
                true
            }
            _ => false,
        }
    }

    /// One step of the redo history.
    fn redo_step(&mut self) -> bool {
        match self.history.redo(&mut self.doc) {
            Ok(Some(tx)) => {
                self.record_journal(&tx);
                self.touched.record(&self.doc, &tx);
                self.note_edit(&tx);
                if let Some(r) = tx.selection_after {
                    self.anchor = r.anchor;
                    self.head = r.head;
                }
                let s = self.undone.pop().unwrap_or_else(|| {
                    self.next_state += 1;
                    self.next_state
                });
                self.states.push(s);
                self.state = s;
                true
            }
            _ => false,
        }
    }
}

fn rect(b: Bounds) -> Rect {
    Rect::new(b.x, b.y, b.w, b.h)
}

impl FrameSource for WriteDoc {
    fn prepare(&mut self, request: &FrameRequest) -> Result<()> {
        if self.view.is_none() {
            let now = view::date_time(self.editor.now_ms.unwrap_or(0));
            self.view = Some(Box::new(WriteView::new(
                &self.doc,
                self.file_name.clone(),
                now,
            )));
        }
        // the review display (markup modes) decides the layout's options.
        self.sync_review_display();
        // Side to Side page movement.
        self.sync_page_movement();
        // The overlay needs the layout of this frame: prepare twice is wasteful, so lay out
        // first, then build the overlay and the scene.
        let extra = self.split_regions();
        if let Some(v) = &mut self.view {
            v.prepare_with(&self.doc, request, Vec::new(), &extra);
        }
        let overlay = self.overlay();
        if let Some(v) = &mut self.view {
            v.scene.set_overlay(overlay);
        }
        Ok(())
    }

    fn scene(&self) -> &Scene {
        static EMPTY: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
        match &self.view {
            Some(v) => v.scene.scene(),
            None => {
                EMPTY.get_or_init(|| Scene::new(1.0, Bounds::ZERO, Paint::Role(ColorRole::Canvas)))
            }
        }
    }

    fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.view
            .as_mut()
            .map_or_else(Vec::new, |v| v.scene.atlas_updates(all))
    }

    fn image_updates(&mut self, all: bool) -> Vec<ImageUpload> {
        self.view.as_mut().map_or_else(Vec::new, |v| {
            let WriteView { layout, scene, .. } = &mut **v;
            scene.image_updates(all, layout)
        })
    }

    fn extent(&self) -> Bounds {
        self.view
            .as_ref()
            .map_or(Bounds::new(0.0, 0.0, 944.0, 1184.0), |v| v.layout.extent())
    }

    fn extra_regions(&self, _request: &FrameRequest) -> Vec<(Bounds, Bounds)> {
        self.split_regions()
    }
}

impl WriteDoc {
    /// View ▸ Split's second pane as `(send, keep)` regions: tiles half a pane beyond it are
    /// sent, a whole pane beyond it kept (the viewport's margins).
    fn split_regions(&self) -> Vec<(Bounds, Bounds)> {
        self.office
            .split
            .map(|r| {
                let b = Bounds::new(r.x, r.y, r.width, r.height);
                (
                    b.inflate(r.width / 2.0, r.height / 2.0),
                    b.inflate(r.width, r.height),
                )
            })
            .into_iter()
            .collect()
    }
}

impl DocModel for WriteDoc {
    fn kind(&self) -> DocKind {
        DocKind::Write
    }

    fn properties(&self) -> Option<kiln_protocol::commands::DocPropertiesInfo> {
        Some(self.properties_info())
    }

    fn set_properties(
        &mut self,
        edit: &kiln_protocol::commands::write_features::PropertiesEdit,
    ) -> Result<bool> {
        let change = self.execute_logged(&Command::Write(
            kiln_protocol::WriteCommand::SetProperties {
                props: Box::new(edit.clone()),
            },
        ))?;
        Ok(change != Change::None)
    }

    fn inspect(&self) -> Vec<kiln_protocol::InspectItem> {
        use kiln_protocol::{InspectItem, InspectKind};
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let changes = self.revisions_info();
        vec![
            InspectItem {
                kind: InspectKind::Comments,
                count: count(self.comment_infos().len()),
            },
            InspectItem {
                kind: InspectKind::Revisions,
                count: count(changes.entries.len()),
            },
            InspectItem {
                kind: InspectKind::Properties,
                count: crate::props::personal_count(&self.properties_info()),
            },
        ]
    }

    fn inspect_remove(&mut self, item: kiln_protocol::InspectKind) -> Result<bool> {
        use kiln_protocol::InspectKind;
        use kiln_protocol::commands::write_features::ChangeScope;
        let command = match item {
            InspectKind::Comments => kiln_protocol::WriteCommand::DeleteAllComments,
            InspectKind::Revisions => kiln_protocol::WriteCommand::AcceptChange {
                scope: ChangeScope::All,
            },
            InspectKind::Properties => kiln_protocol::WriteCommand::SetProperties {
                props: Box::new(crate::props::clear_personal()),
            },
            other => {
                return Err(KilnError::Unsupported(format!(
                    "Kiln Write documents have no {other:?}"
                )));
            }
        };
        Ok(self.execute_logged(&Command::Write(command))? != Change::None)
    }

    fn spelling_changed(&mut self) {
        self.features.spell.invalidate();
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        self.execute_logged(command)
    }

    fn app_query(&self, query: &kiln_protocol::Query) -> Result<kiln_protocol::QueryResult> {
        match query {
            kiln_protocol::Query::Write(q) => {
                Ok(kiln_protocol::QueryResult::Write(self.write_query(q)?))
            }
            _ => Err(KilnError::Unsupported(
                "the query for a Write document".into(),
            )),
        }
    }

    fn take_clipboard(&mut self) -> Option<kiln_protocol::commands::ClipboardData> {
        self.take_copied()
    }

    fn collab(&mut self) -> Option<&mut dyn crate::collab::Collaborative> {
        Some(self)
    }

    fn to_bundle(&self) -> Result<kiln_storage::Bundle> {
        WriteDoc::to_bundle(self)
    }

    fn bundles(&self) -> bool {
        true
    }

    fn set_journaling(&mut self, on: bool) {
        WriteDoc::set_journaling(self, on);
    }

    fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        WriteDoc::take_journal(self)
    }

    fn apply_journal(&mut self, op: &[u8], _: &crate::JournalReplay) -> Result<()> {
        WriteDoc::apply_journal(self, op)
    }

    fn set_clock(&mut self, now_ms: u64) {
        self.editor.now_ms = Some(now_ms);
    }

    fn undo(&mut self) -> bool {
        self.end_composition();
        // an AutoFormat made of several steps (`# ` → Heading 1) undoes as one.
        let steps = self.grouped_undo_steps();
        let mut done = false;
        for _ in 0..steps {
            if !self.undo_step() {
                break;
            }
            done = true;
        }
        done
    }

    fn redo(&mut self) -> bool {
        self.end_composition();
        let steps = self.grouped_redo_steps();
        let mut done = false;
        for _ in 0..steps {
            if !self.redo_step() {
                break;
            }
            done = true;
        }
        done
    }

    fn can_undo(&self) -> bool {
        // a shared document undoes through the engine.
        self.shared_undo()
            .map_or_else(|| self.history.can_undo(), |(u, _)| u)
    }

    fn can_redo(&self) -> bool {
        self.shared_undo()
            .map_or_else(|| self.history.can_redo(), |(_, r)| r)
    }

    fn undo_label(&self) -> Option<&'static str> {
        self.history.undo_kind().map(|k| match k {
            kiln_doc::EditKind::Typing => "write.edit.insert_text",
            kiln_doc::EditKind::Delete => "write.edit.delete_backward",
            _ => "suite.edit.undo",
        })
    }

    fn redo_label(&self) -> Option<&'static str> {
        self.can_redo().then_some("suite.edit.redo")
    }

    fn state(&self) -> StateId {
        self.state
    }

    fn seal_history(&mut self) {
        self.history.break_group();
    }

    fn selection(&self) -> Selection {
        let mut sel = Selection::text(Self::wire(self.anchor), Self::wire(self.head));
        let Some(v) = &self.view else {
            return sel;
        };
        let l = &v.layout;
        if self.anchor != self.head {
            sel.rects =
                kiln_doc_layout::geometry::selection_rects(l, &self.doc, self.anchor, self.head)
                    .into_iter()
                    .map(rect)
                    .collect();
        }
        if let Some(c) = v.caret(self.head) {
            sel.caret = Some(Caret {
                rect: rect(c.rect),
                rtl: c.rtl,
            });
        }
        if self.anchor != self.head {
            let (s, e) = kiln_doc::Range::new(self.anchor, self.head).ordered(&self.doc);
            let size = view::HANDLE_SIZE;
            for (p, kind) in [
                (s, HandleKind::SelectionStart),
                (e, HandleKind::SelectionEnd),
            ] {
                if let Some(c) = v.caret(p) {
                    let x = c.rect.x + c.rect.w / 2.0;
                    let left = if kind == HandleKind::SelectionStart {
                        x - size
                    } else {
                        x
                    };
                    sel.handles.push(Handle {
                        kind,
                        rect: Rect::new(left, c.rect.bottom(), size, size),
                    });
                }
            }
        }
        sel
    }

    fn status(&self) -> StatusBar {
        let stats = self.body_statistics();
        let page = self.view.as_ref().and_then(|v| {
            let total = u32::try_from(v.layout.pages().len()).ok()?;
            let c = v.caret(self.head)?;
            Some(Position {
                current: u32::try_from(c.page + 1).ok()?,
                total,
            })
        });
        StatusBar {
            page,
            words: Some(u32::try_from(stats.words).unwrap_or(u32::MAX)),
            chars: Some(u32::try_from(stats.characters_with_spaces).unwrap_or(u32::MAX)),
            language: self.caret_language(),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<ParagraphText>> {
        Some(
            self.body()
                .into_iter()
                .map(|p| ParagraphText {
                    id: p.id,
                    text: p.content.plain_text(false),
                })
                .collect(),
        )
    }

    fn a11y(&self, title: &str) -> A11yTree {
        let root = self.doc.sections.first().map_or(NodeId(0), |s| s.id);
        let Some(v) = &self.view else {
            return self.a11y_document(title);
        };
        let (first, last) = v.visible_pages();
        crate::a11y::document_tree(
            root,
            title,
            Some(rect(v.layout.extent())),
            kiln_doc_layout::a11y::visible_nodes(&v.layout, &self.doc, first, last),
            self.head.para,
            Self::wire(self.head),
        )
    }

    fn a11y_document(&self, title: &str) -> A11yTree {
        let root = self.doc.sections.first().map_or(NodeId(0), |s| s.id);
        crate::a11y::document_tree(
            root,
            title,
            None,
            kiln_doc_layout::a11y::document_nodes(&self.doc, self.revision_view()),
            self.head.para,
            Self::wire(self.head),
        )
    }

    fn selected_text(&self) -> Option<String> {
        let (s, e) = kiln_doc::Range::new(self.anchor, self.head).ordered(&self.doc);
        if s == e {
            return Some(String::new());
        }
        let paras = self.story_paragraphs(s.para);
        let a = paras.iter().position(|p| *p == s.para)?;
        let b = paras.iter().position(|p| *p == e.para)?;
        let mut parts = Vec::new();
        for (k, id) in paras[a..=b].iter().enumerate() {
            let p = self.doc.paragraph(*id)?;
            let len = p.content.len();
            let from = if k == 0 { s.offset } else { 0 };
            let to = if a + k == b { e.offset } else { len };
            let text = p.content.slice(from.min(len), to.min(len)).ok()?.text;
            parts.push(text.replace(kiln_doc::OBJECT_CHAR, ""));
        }
        Some(parts.join("\n"))
    }

    fn frame_source(&mut self) -> Option<&mut dyn FrameSource> {
        Some(self)
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &SAVE_FORMATS
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        self.save_in(format, None)
    }

    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        self.save_in(format, Some(now_ms))
    }

    fn compat_report(&self, format: FileFormat) -> Vec<kiln_protocol::CompatIssue> {
        let mut issues = save::compat(&self.doc, self.original.as_ref(), &self.warnings, format);
        if self.original_lost && save::docx_kind(format).is_some() {
            issues.push(crate::storage::original_lost("Word"));
        }
        // A text file keeps its encoding: characters it cannot hold are saved as `?`.
        if format == FileFormat::Txt
            && let Some(f) = self.text_format
        {
            let lost = textio::unrepresentable(&self.doc, f.encoding, 20);
            if !lost.is_empty() {
                let list: String = lost
                    .iter()
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|c| format!("{c} "))
                    .collect();
                issues.push(kiln_protocol::CompatIssue {
                    code: "text.unrepresentable".into(),
                    severity: kiln_protocol::CompatSeverity::Loss,
                    message: format!(
                        "{} cannot hold these characters; they are saved as ?: {}",
                        f.encoding.title(),
                        list.trim_end()
                    ),
                    count: u32::try_from(lost.len()).unwrap_or(u32::MAX),
                    origin: kiln_protocol::CompatOrigin::Save,
                });
            }
        }
        issues
    }

    fn print(&self, job: &crate::print::PrintJob<'_>) -> Result<crate::print::PrintOutput> {
        let name = self
            .file_name
            .as_deref()
            .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n).to_owned());
        let title = match job {
            crate::print::PrintJob::Pdf { title, .. } => (*title).to_owned(),
            crate::print::PrintJob::Preview { .. } => name.as_deref().map_or_else(
                || "Document".to_owned(),
                |n| n.trim_end_matches(".docx").to_owned(),
            ),
        };
        self.print
            .print_write(&self.doc, self.state, name.as_deref(), &title, job)
    }
}

impl WriteDoc {
    /// Writes the document in a format: Word's through the DOCX writer, plain text in the
    /// encoding it was opened with, Markdown and HTML as exports.
    fn save_in(&self, format: FileFormat, now_ms: Option<u64>) -> Result<Vec<u8>> {
        match format {
            FileFormat::Txt => Ok(textio::write(
                &self.doc,
                self.text_format.unwrap_or_default(),
            )),
            FileFormat::Md => Ok(export::markdown(&self.doc).into_bytes()),
            FileFormat::Html => Ok(export::html(&self.doc).into_bytes()),
            _ => save::save(
                &self.doc,
                self.original.as_ref(),
                &self.touched,
                format,
                now_ms,
            ),
        }
    }

    /// Fills a new document from Markdown (opening a `.md` file): the editor's Paste Markdown
    /// into the empty document, then a fresh history.
    fn import_markdown(&mut self, text: &str) -> Result<()> {
        if !text.trim().is_empty() {
            let cmd = Command::Write(kiln_protocol::commands::WriteCommand::PasteMarkdown {
                text: text.to_owned(),
            });
            self.execute_logged(&cmd)?;
        }
        self.history = UndoStack::default();
        self.states.clear();
        self.undone.clear();
        self.state = 0;
        self.touched = TouchedParts::new();
        let first = self
            .doc
            .body_paragraphs()
            .first()
            .map_or(NodeId(0), |p| p.id);
        self.anchor = Pos::new(first, 0);
        self.head = self.anchor;
        Ok(())
    }
}
