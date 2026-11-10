// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck presentations: `.pptx`, `.potx` and `.pptm` read and written by
//! `kiln-io-pptx` (and `.odp` by `kiln-io-odf` with the `odf` feature) into the `kiln-deck`
//! model, drawn by `kiln-deck-layout` and edited here.
//!
//! - **Open and save** ([`save`]): the file's bytes and the reader's side data are kept, every
//!   edit's parts are recorded, and a save copies untouched parts byte for byte.
//! - **Stage** ([`view`]): the current slide as tiles on its pasteboard with the selection, its
//!   handles, smart guides, the marquee and the text caret in the overlay; thumbnails for the
//!   rail and the galleries; the accessibility tree of the slide.
//! - **Gestures** ([`stage`]): pointer presses, moves and releases become selection, moves,
//!   resizes and rotations with Shift/Ctrl/Alt, snapping and guides, marquee selection, drawing
//!   new shapes and placing the caret.
//! - **Commands** ([`slides`], [`shapes`], [`text`], [`format`], [`clipboard`]): every Deck row of
//!   the shortcut table and the Home, Insert and Design ribbons; each change is one undo step of the
//!   model's history.
//! - **Queries** ([`state`]): what the chrome shows (`Query::Deck`).
//! - **Motion** ([`features`]): transitions, animations, the Animation Pane and Painter.
//! - **Slide show** ([`show`]): the show and its frames, presenter view's information,
//!   Set Up Slide Show (kept in `presProps.xml`), Rehearse Timings and the previews.
//! - **Print and export, Design Ideas, Outline View, Reuse Slides** ([`features`]).
//! - **Objects** ([`objects`], [`crop`], [`cells`]): Paste as Picture, icons, code
//!   blocks and symbols; crop handles, Crop to aspect ratio, Fill and Fit; table cell selection,
//!   merge, split and cell margins.

mod cells;
mod chart_book;
mod charts;
mod clipboard;
#[cfg(test)]
mod command_tests;
mod crop;
mod design;
pub mod features;
mod find;
mod format;
mod master;
#[cfg(test)]
mod object_tests;
mod objects;
mod review;
mod save;
mod shapes;
mod shared;
mod show;
mod slides;
mod stage;
mod state;
/// native bundles and the autosave journal.
mod storage;
#[cfg(test)]
mod tests;
mod text;
mod view;
mod views;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{EditKind, TextTarget, Transaction};
use kiln_deck::{Owner, Presentation, UndoStack};
use kiln_deck_layout::Renderer;
use kiln_io_pptx::TouchedParts;
use kiln_protocol::commands::{ClipboardData, DeckCommand};
use kiln_protocol::{
    A11yTree, Command, CompatIssue, DocKind, FileFormat, OpenHint, Query, QueryResult, SelKind,
    Selection, StatusBar, SuiteCommand,
};
use rustc_hash::FxHashMap;

use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

pub use text::TextEdit;

/// The formats Deck opens and saves; `.odp` only with the `odf` feature (the web core leaves it
/// out).
#[cfg(feature = "odf")]
const FORMATS: [FileFormat; 5] = [
    FileFormat::Pptx,
    FileFormat::Potx,
    FileFormat::Pptm,
    FileFormat::Ppsx,
    FileFormat::Odp,
];
#[cfg(not(feature = "odf"))]
const FORMATS: [FileFormat; 4] = [
    FileFormat::Pptx,
    FileFormat::Potx,
    FileFormat::Pptm,
    FileFormat::Ppsx,
];

/// Creates and opens Deck presentations.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeckApp;

impl AppFactory for DeckApp {
    fn kind(&self) -> DocKind {
        DocKind::Deck
    }

    fn opens(&self, format: FileFormat) -> bool {
        FORMATS.contains(&format)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(DeckDoc::new(Presentation::new(), None, None))
    }

    /// a new presentation in the default editing language: its default text style is in
    /// it (`p:defaultTextStyle`, as PowerPoint writes it), so the status bar and proofing follow;
    /// in a right-to-left language the master and layouts are right to left.
    fn create_in(&self, language: &str) -> Box<dyn DocModel> {
        let mut pres = Presentation::new();
        let style = &mut pres.default_text_style;
        if style.default_level().is_none() {
            style.set_default_level(Some(kiln_deck::text::LevelStyle::default()));
        }
        let rtl = kiln_doc::is_rtl_language(language);
        for level in style.entries_mut() {
            let lang = level.run.lang.get_or_insert_with(Default::default);
            lang.val = Some(language.to_owned());
            if rtl {
                lang.bidi = Some(language.to_owned());
            }
        }
        // Arabic, Hebrew, Persian and Urdu: right-to-left, right-aligned text styles and a
        // mirrored master, as PowerPoint's own right-to-left presentations.
        if rtl {
            pres.make_right_to_left();
        }
        Box::new(DeckDoc::new(pres, None, None))
    }

    fn open_bundle(
        &self,
        bundle: &kiln_storage::Bundle,
        hint: &OpenHint,
    ) -> Result<Box<dyn DocModel>> {
        let name = hint.path.clone().or_else(|| hint.name.clone());
        Ok(Box::new(DeckDoc::from_bundle(bundle, name)?))
    }

    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>> {
        let name = hint.path.clone().or_else(|| hint.name.clone());
        let format_error = |message: String| KilnError::Format {
            format: format.extension().to_owned(),
            message,
        };
        match format {
            #[cfg(feature = "odf")]
            FileFormat::Odp => {
                let read = kiln_io_odf::read_odp(bytes).map_err(|e| format_error(e.to_string()))?;
                let mut doc = DeckDoc::new(read.presentation, None, name);
                doc.odf_warnings = read.warnings;
                doc.odf_properties = Some(read.properties);
                Ok(Box::new(doc))
            }
            FileFormat::Pptx | FileFormat::Potx | FileFormat::Pptm | FileFormat::Ppsx => {
                let read =
                    kiln_io_pptx::read_pptx(bytes).map_err(|e| format_error(e.to_string()))?;
                let (pres, original) = save::Original::from_read(read, bytes);
                Ok(Box::new(DeckDoc::new(pres, Some(original), name)))
            }
            other => Err(KilnError::Unsupported(format!(
                ".{} files in Kiln Deck",
                other.extension()
            ))),
        }
    }
}

/// The view of a presentation: the slide on the stage, the rail's selection, the selected
/// shapes, the text being edited and the View toggles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewState {
    /// Index of the slide on the stage.
    pub current: usize,
    /// Selected slides (sorted), the current one among them.
    pub slides: Vec<usize>,
    /// Where a Shift selection of slides started.
    pub anchor: usize,
    /// The selection is the slides (the rail has the focus), not shapes.
    pub rail: bool,
    /// Selected shapes of the current slide (top-level), in selection order.
    pub shapes: Vec<NodeId>,
    /// The text being edited.
    pub text: Option<TextEdit>,
    /// View ▸ Guides.
    pub guides: bool,
    /// View ▸ Gridlines.
    pub gridlines: bool,
    /// View ▸ Notes.
    pub notes: bool,
    /// Insert ▸ Shapes waiting for a drag on the stage.
    pub armed: Option<String>,
    /// Effects selected in the Animation Pane.
    pub effects: Vec<NodeId>,
    /// Effect numbers shown on the stage (the Animations tab).
    pub badges: bool,
    /// The selected comment thread.
    pub comment: Option<NodeId>,
    /// Slide Master view: the master or layout on the stage.
    pub master: Option<NodeId>,
    /// Picture Format ▸ Crop: the selected picture shows its crop handles.
    pub crop: bool,
    /// Cells selected across a table being edited: `(table, range)`.
    pub cells: Option<(NodeId, kiln_protocol::commands::deck_content::DeckCellRange)>,
}

/// One undo step as the session sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    state: StateId,
    label: &'static str,
}

/// A Deck presentation held by the session.
pub struct DeckDoc {
    pub(crate) pres: Presentation,
    original: Option<save::Original>,
    touched: TouchedParts,
    /// a bundle's original PowerPoint file could not be read.
    original_lost: bool,
    #[cfg_attr(not(feature = "odf"), allow(dead_code))]
    odf_warnings: Vec<String>,
    #[cfg_attr(not(feature = "odf"), allow(dead_code))]
    odf_properties: Option<kiln_doc::DocProperties>,
    /// the document properties set in Kiln (`None`: those of the file read).
    pub(crate) props: Option<crate::props::UserProps>,
    file_name: Option<String>,
    undo: UndoStack,
    done: Vec<Step>,
    undone: Vec<Step>,
    state: StateId,
    next_state: StateId,
    /// Nesting of grouped edits (a drag, a compound command): one undo step.
    group: u32,
    /// Whether the current group recorded its first step.
    group_started: bool,
    pub(crate) view: ViewState,
    pub(crate) drag: Option<stage::Drag>,
    /// Slide rendering and its caches (also used by queries, which borrow the model).
    pub(crate) renderer: std::cell::RefCell<Renderer>,
    pub(crate) stage: view::Stage,
    /// Bumped by every change of the presentation (the stage redraws).
    pub(crate) epoch: u64,
    /// Bumped when masters, layouts, themes or the slide size change (every slide changes).
    global_version: u64,
    /// Per-slide content versions (thumbnail cache keys).
    slide_versions: FxHashMap<NodeId, u64>,
    /// What Copy/Cut put on the clipboard, for the session to write.
    copied: Option<ClipboardData>,
    /// Kiln's last copy (shapes or slides).
    pub(crate) clip: Option<clipboard::Clip>,
    /// The Format Painter's formatting.
    pub(crate) format_clip: Option<format::FormatClip>,
    /// The host's clock.
    pub(crate) now_ms: u64,
    /// The last Find found nothing more.
    pub(crate) find_done: bool,
    /// The zoom of the last frame (handles keep their screen size).
    pub(crate) zoom: f32,
    /// The last command F4 (Repeat) repeats.
    pub(crate) repeat: Option<Command>,
    /// The running slide show.
    pub(crate) show: Option<show::Show>,
    /// Set Up Slide Show, and as the file had it.
    pub(crate) show_settings: kiln_protocol::commands::deck_show::DeckShowSettings,
    pub(crate) show_settings_read: kiln_protocol::commands::deck_show::DeckShowSettings,
    /// Rehearse Timings: the times of the last rehearsal, until kept or discarded.
    pub(crate) rehearsal: Option<Vec<(NodeId, u32)>>,
    /// The Animation Painter's effects.
    pub(crate) anim_clip: Option<Vec<kiln_deck::animation::Effect>>,
    /// The show frame prepared for the next `ShowFrame` query.
    pub(crate) show_frame:
        std::cell::RefCell<Option<kiln_protocol::commands::deck_show::DeckShowFrame>>,
    /// What every window has to hear, and what it heard last.
    pub(crate) notices: Vec<crate::model::ModelNotice>,
    pub(crate) notified: Option<kiln_protocol::commands::deck_show::DeckShowStatus>,
    /// Review ▸ Spelling.
    pub(crate) spelling: review::Spelling,
    /// View ▸ Guides: the drawing guides, and as the file had them.
    pub(crate) guides: Vec<kiln_protocol::commands::deck_content::DeckGuide>,
    pub(crate) guides_read: Vec<kiln_protocol::commands::deck_content::DeckGuide>,
    /// the presentation is shared (collaboration and Version History).
    pub(crate) shared: Option<Box<shared::SharedDeck>>,
    /// the transactions applied since the session last took them, for the autosave
    /// journal (while it records).
    pub(crate) journal: Option<Vec<Vec<u8>>>,
    /// the language of the placeholder prompts (the interface's).
    pub(crate) prompts: kiln_deck_layout::PromptLanguage,
    /// How many host-registered fonts the renderer has.
    pub(crate) fonts_seen: usize,
}

impl std::fmt::Debug for DeckDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckDoc")
            .field("slides", &self.pres.slides.len())
            .field("view", &self.view)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// The fonts of slides: the document fonts the host registered (the web core) or bundled
/// (native), and Kiln's brand fonts.
fn fonts() -> kiln_text::FontStore {
    let mut f = crate::apps::scratch::fonts::store();
    kiln_deck_layout::fonts::add_brand_fonts(&mut f);
    f
}

impl DeckDoc {
    fn new(
        pres: Presentation,
        original: Option<save::Original>,
        file_name: Option<String>,
    ) -> Self {
        let mut doc = Self {
            pres,
            original,
            touched: TouchedParts::new(),
            original_lost: false,
            odf_warnings: Vec::new(),
            odf_properties: None,
            props: None,
            file_name,
            undo: UndoStack::new(1_000),
            done: Vec::new(),
            undone: Vec::new(),
            state: 0,
            next_state: 1,
            group: 0,
            group_started: false,
            // PowerPoint opens with the notes pane shown.
            view: ViewState {
                notes: true,
                ..ViewState::default()
            },
            drag: None,
            renderer: std::cell::RefCell::new(Renderer::new(fonts(), fonts())),
            stage: view::Stage::default(),
            epoch: 1,
            global_version: 1,
            slide_versions: FxHashMap::default(),
            copied: None,
            clip: None,
            format_clip: None,
            now_ms: 0,
            find_done: false,
            zoom: 1.0,
            repeat: None,
            show: None,
            show_settings: Default::default(),
            show_settings_read: Default::default(),
            rehearsal: None,
            anim_clip: None,
            show_frame: std::cell::RefCell::new(None),
            notices: Vec::new(),
            notified: None,
            spelling: review::Spelling::default(),
            guides: Vec::new(),
            guides_read: Vec::new(),
            shared: None,
            journal: None,
            prompts: kiln_deck_layout::PromptLanguage::default(),
            fonts_seen: crate::apps::scratch::fonts::registered_count(),
        };
        let (w, h) = (
            kiln_deck_layout::geom::px(doc.pres.slide_size.cx),
            kiln_deck_layout::geom::px(doc.pres.slide_size.cy),
        );
        doc.guides = views::guides_of(
            doc.original.as_ref().map(|o| &o.read_ref().passthrough),
            w,
            h,
        );
        doc.guides_read = doc.guides.clone();
        let settings = doc
            .original
            .as_ref()
            .and_then(|o| show::settings::part(&o.read_ref().passthrough))
            .map(|p| show::settings::read(&p.bytes))
            .unwrap_or_default();
        doc.show_settings = settings.clone();
        doc.show_settings_read = settings;
        doc.view.slides = if doc.pres.slides.is_empty() {
            Vec::new()
        } else {
            vec![0]
        };
        doc
    }

    /// The presentation.
    #[must_use]
    pub fn presentation(&self) -> &Presentation {
        &self.pres
    }

    /// The file name, if the presentation came from a file.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    /// The slide on the stage (none in Slide Master view).
    pub(crate) fn current_slide(&self) -> Option<NodeId> {
        if self.view.master.is_some() {
            return None;
        }
        self.pres.slides.get(self.view.current).map(|s| s.id)
    }

    /// What the stage shows and new objects go on: the current slide, or in Slide Master view
    /// the master or layout being edited (an error for an empty presentation).
    pub(crate) fn slide_id(&self) -> Result<NodeId> {
        self.view
            .master
            .or_else(|| self.current_slide())
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no slides".into()))
    }

    /// The top-level shapes on the stage.
    pub(crate) fn stage_shapes(&self) -> Option<&Vec<kiln_deck::Shape>> {
        self.pres.owner_shapes(self.owner()?)
    }

    /// The ids of the selected slides.
    pub(crate) fn selected_slide_ids(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .view
            .slides
            .iter()
            .filter_map(|i| self.pres.slides.get(*i).map(|s| s.id))
            .collect();
        if out.is_empty()
            && let Some(id) = self.current_slide()
        {
            out.push(id);
        }
        out
    }

    /// Starts a group of edits that undo as one step.
    pub(crate) fn begin_group(&mut self) {
        if self.group == 0 {
            self.group_started = false;
        }
        self.group += 1;
        self.undo.begin_group();
    }

    /// Ends a group.
    pub(crate) fn end_group(&mut self) {
        self.group = self.group.saturating_sub(1);
        self.undo.end_group();
        if self.group == 0 {
            self.group_started = false;
        }
    }

    /// Applies a transaction as an undo step labelled with a command id. Returns whether
    /// anything changed.
    pub(crate) fn apply(&mut self, label: &'static str, tx: Transaction) -> Result<bool> {
        if tx.is_empty() {
            return Ok(false);
        }
        let kind = tx.kind;
        let before = self.undo.undo_len();
        self.undo
            .apply(&mut self.pres, tx.clone())
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.touched.record(&self.pres, &tx);
        self.note_change(&tx);
        self.record_shared(&tx);
        let after = self.undo.undo_len();
        let state = self.next_state;
        self.next_state += 1;
        let merged = if self.group > 0 {
            std::mem::replace(&mut self.group_started, true)
        } else {
            after == before && kind == EditKind::Typing && !self.done.is_empty()
        };
        if merged && let Some(top) = self.done.last_mut() {
            top.state = state;
        } else {
            self.done.push(Step { state, label });
            if self.done.len() > after.max(1) {
                let excess = self.done.len() - after.max(1);
                self.done.drain(..excess);
            }
        }
        self.undone.clear();
        self.state = state;
        Ok(true)
    }

    /// Applies a transaction built from the presentation (`build` may fail).
    pub(crate) fn apply_with(
        &mut self,
        label: &'static str,
        build: impl FnOnce(&mut Presentation) -> kiln_deck::Result<Transaction>,
    ) -> Result<bool> {
        let tx = build(&mut self.pres).map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply(label, tx)
    }

    /// Records what a transaction changed: the stage redraws, and the thumbnails of the slides
    /// it touched (all of them when a master, layout, theme or the slide size changed).
    pub(crate) fn note_change(&mut self, tx: &Transaction) {
        self.epoch += 1;
        let mut t = TouchedParts::new();
        t.record(&self.pres, tx);
        let slide_ids: Vec<NodeId> = self.pres.slides.iter().map(|s| s.id).collect();
        let mut global = t.everything
            || t.presentation
                && tx.ops.iter().any(|o| {
                    matches!(
                        o,
                        kiln_deck::Op::SetSlideSize { .. }
                            | kiln_deck::Op::SetDefaultTextStyle { .. }
                    )
                });
        for o in &t.owners {
            if slide_ids.contains(o) {
                *self.slide_versions.entry(*o).or_insert(0) += 1;
            } else {
                global = true;
            }
        }
        for s in t.notes.iter().chain(t.comments.iter()) {
            *self.slide_versions.entry(*s).or_insert(0) += 1;
        }
        if global {
            self.global_version += 1;
            self.renderer.get_mut().invalidate();
        }
        // Parts written by the edit (a new picture, a chart) may replace older bytes.
        for op in &tx.ops {
            if let kiln_deck::Op::SetPart { key: name, .. } = op {
                self.renderer.get_mut().forget_part(name);
            }
        }
    }

    /// Notes a change made outside the history (a drag's live preview).
    pub(crate) fn note_preview(&mut self, slide: NodeId) {
        self.epoch += 1;
        *self.slide_versions.entry(slide).or_insert(0) += 1;
    }

    /// The thumbnail version of a slide.
    pub(crate) fn slide_version(&self, id: NodeId) -> u64 {
        let own = self.slide_versions.get(&id).copied().unwrap_or(0);
        own.wrapping_mul(1_000_003)
            .wrapping_add(self.global_version)
            .wrapping_add(id.0.wrapping_mul(7))
            & kiln_scene::HASH_MASK
    }

    /// Keeps the view valid after the presentation changed.
    pub(crate) fn validate_view(&mut self) {
        let n = self.pres.slides.len();
        if n == 0 {
            self.view.current = 0;
            self.view.slides.clear();
            self.view.shapes.clear();
            self.view.text = None;
            return;
        }
        self.view.current = self.view.current.min(n - 1);
        self.view.slides.retain(|i| *i < n);
        if !self.view.slides.contains(&self.view.current) {
            self.view.slides = vec![self.view.current];
        }
        self.view.slides.sort_unstable();
        self.view.slides.dedup();
        self.view.anchor = self.view.anchor.min(n - 1);
        // Slide Master view: its master or layout still there, and the selection on it.
        if let Some(m) = self.view.master {
            if self.pres.master(m).is_none() && self.pres.layout(m).is_none() {
                self.view.master = self.pres.masters.first().map(|x| x.id);
            }
            let top: Vec<NodeId> = self
                .stage_shapes()
                .map(|v| v.iter().map(|s| s.id).collect())
                .unwrap_or_default();
            self.view.shapes.retain(|id| top.contains(id));
            if let Some(t) = &self.view.text
                && !top.contains(&t.shape)
            {
                self.view.text = None;
            }
            return;
        }
        let slide = &self.pres.slides[self.view.current];
        let top: Vec<NodeId> = slide.shapes.iter().map(|s| s.id).collect();
        self.view.shapes.retain(|id| top.contains(id));
        // The selected comment thread is one of this slide's.
        if let Some(c) = self.view.comment
            && !slide.comments.iter().any(|x| x.id == c)
        {
            self.view.comment = None;
        }
        // Effects selected in the Animation Pane stay selected while their shapes are.
        if !self.view.effects.is_empty() {
            let targets: Vec<(NodeId, NodeId)> = slide
                .timeline
                .sequence
                .iter()
                .map(|e| (e.id, e.target.shape))
                .collect();
            let keep: Vec<NodeId> = targets
                .iter()
                .filter(|(id, shape)| {
                    self.view.effects.contains(id)
                        && self.view.shapes.contains(&self.top_of(*shape))
                })
                .map(|(id, _)| *id)
                .collect();
            self.view.effects = keep;
        }
        let slide = &self.pres.slides[self.view.current];
        if let Some(t) = &self.view.text {
            let ok = match t.target {
                TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => {
                    self.pres.slide_of_shape(s).map(|sl| sl.id) == Some(slide.id)
                }
                TextTarget::Notes(_) => true,
            };
            if !ok || kiln_deck::edit::text::text_body(&self.pres, t.target).is_err() {
                self.view.text = None;
            } else {
                self.clamp_text();
            }
        }
    }

    fn after_history(&mut self, tx: &Transaction) {
        self.record_journal(tx);
        self.touched.record(&self.pres, tx);
        self.note_change(tx);
        // The selection the step restores, when its shapes are on a slide.
        let ids: Vec<NodeId> = tx
            .selection_after
            .iter()
            .copied()
            .filter(|id| self.pres.slide_of_shape(*id).is_some())
            .collect();
        if let Some(first) = ids.first()
            && let Some(slide) = self.pres.slide_of_shape(*first).map(|s| s.id)
            && let Some(i) = self.pres.slide_index(slide)
        {
            self.view.current = i;
            self.view.slides = vec![i];
            let slide_shapes: Vec<NodeId> =
                self.pres.slides[i].shapes.iter().map(|s| s.id).collect();
            self.view.shapes = ids
                .into_iter()
                .filter(|id| slide_shapes.contains(id))
                .collect();
            self.view.rail = false;
        }
        self.view.text = None;
        self.validate_view();
    }

    fn deck(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        // A gesture in progress ends before any other command.
        if self.drag.is_some()
            && !matches!(
                cmd,
                D::PointerMove { .. } | D::PointerUp { .. } | D::PointerDown { .. }
            )
        {
            self.cancel_drag();
        }
        match cmd {
            D::NewSlide { .. }
            | D::Duplicate
            | D::DeleteSlides
            | D::MoveSlides { .. }
            | D::SelectSlide { .. }
            | D::PreviousSlide
            | D::NextSlide
            | D::SetLayout { .. }
            | D::ResetSlides
            | D::HideSlides { .. }
            | D::AddSection { .. }
            | D::RenameSection { .. }
            | D::RemoveSection { .. }
            | D::MoveSection { .. }
            | D::SetNotes { .. }
            | D::ApplyTheme { .. }
            | D::ApplyVariant { .. }
            | D::SlideSize { .. }
            | D::SetBackground { .. }
            | D::HideBackgroundGraphics { .. }
            | D::SetHeaderFooter { .. } => self.slide_command(cmd),
            D::PointerDown { .. }
            | D::PointerMove { .. }
            | D::PointerUp { .. }
            | D::ArmInsert { .. }
            | D::SelectShapes { .. }
            | D::SelectAll
            | D::Escape
            | D::CycleSelection { .. } => self.stage_command(cmd),
            D::EditText
            | D::InsertText { .. }
            | D::DeleteText { .. }
            | D::NewParagraph
            | D::LineBreak
            | D::Tab { .. }
            | D::MoveCaret { .. }
            | D::SetComposition { .. }
            | D::SelectText { .. }
            | D::Promote
            | D::Demote
            | D::InsertSlideNumber
            | D::Find { .. }
            | D::Replace { .. }
            | D::ReplaceFont { .. } => self.text_command(cmd),
            D::ToggleGuides => {
                self.view.guides = !self.view.guides;
                Ok(Change::Selection)
            }
            D::ToggleGridlines => {
                self.view.gridlines = !self.view.gridlines;
                Ok(Change::Selection)
            }
            D::ToggleNotes => {
                self.view.notes = !self.view.notes;
                Ok(Change::Selection)
            }
            D::CopyFormat | D::PasteFormat => self.format_painter(cmd),
            D::SetTransition { .. }
            | D::TransitionOption { .. }
            | D::TransitionDuration { .. }
            | D::AdvanceOnClick { .. }
            | D::AdvanceAfter { .. }
            | D::ApplyTransitionToAll
            | D::SetAnimation { .. }
            | D::AddAnimation { .. }
            | D::AnimationOption { .. }
            | D::AnimationStart { .. }
            | D::AnimationDuration { .. }
            | D::AnimationDelay { .. }
            | D::AnimationRepeat { .. }
            | D::RemoveAnimations
            | D::MoveAnimation { .. }
            | D::ReorderAnimation { .. }
            | D::SelectAnimations { .. }
            | D::CopyAnimation
            | D::PasteAnimation
            | D::MotionBadges { .. } => self.motion_command(cmd),
            D::SetShowSettings { .. }
            | D::ShowStart { .. }
            | D::ShowNext
            | D::ShowPrevious
            | D::ShowFirst
            | D::ShowLast
            | D::ShowDigit { .. }
            | D::ShowGoToTyped
            | D::ShowGoTo { .. }
            | D::ShowBlack
            | D::ShowWhite
            | D::ShowLaser
            | D::ShowPen
            | D::ShowHighlighter
            | D::ShowEraser
            | D::ShowArrow
            | D::ShowErase
            | D::ShowPointer { .. }
            | D::ShowEnd
            | D::ShowKeepTimings => self.show_command(cmd),
            D::ApplyDesignIdea { id } => self.apply_design_idea(id),
            D::EditOutline { edit } => self.outline_command(edit),
            D::ReuseSlides { bytes, slides } => self.reuse_slides(bytes, slides),
            D::InsertMedia {
                kind,
                name,
                mime,
                bytes,
                poster,
                width,
                height,
            } => self.insert_media(*kind, name, mime, bytes, poster, (*width, *height)),
            D::InsertEquation { linear } => self.insert_equation(linear),
            D::SetChartData { cells } => self.set_chart_data(cells),
            D::NewComment { .. }
            | D::ReplyComment { .. }
            | D::EditComment { .. }
            | D::ResolveComment { .. }
            | D::DeleteComment { .. }
            | D::SelectComment { .. }
            | D::NextComment { .. } => self.comment_command(cmd),
            D::PastePicture { data } => self.paste_picture(data.as_ref()),
            D::InsertIcon { name, paths } => self.insert_icon(name, paths),
            D::InsertCodeBlock { code, language } => {
                self.insert_code_block(code, language.as_deref())
            }
            D::InsertSymbol { text, font } => self.insert_symbol(text, font.as_deref()),
            D::CropMode { .. } | D::CropTo { .. } => self.crop_command(cmd),
            D::ConvertToDiagram { layout } => self.convert_to_diagram(*layout),
            D::SelectCells { .. } | D::CellMargins { .. } => self.cells_command(cmd),
            D::AddGuide { .. }
            | D::MoveGuide { .. }
            | D::RemoveGuide { .. }
            | D::ParagraphIndent { .. } => self.view_command(cmd),
            D::MasterView { .. }
            | D::SelectMasterItem { .. }
            | D::InsertSlideMaster
            | D::InsertLayout
            | D::DeleteMasterItem
            | D::RenameMasterItem { .. }
            | D::PreserveMaster { .. }
            | D::InsertPlaceholder { .. }
            | D::LayoutTitle { .. }
            | D::LayoutFooters { .. } => self.master_command(cmd),
            D::SetThemeColors { .. }
            | D::SetThemeFonts { .. }
            | D::SetThemeEffects { .. }
            | D::SetBackgroundStyle { .. }
            | D::ApplyThemeFile { .. } => self.design_command(cmd),
            D::SpellCheck
            | D::SpellChange { .. }
            | D::SpellIgnore { .. }
            | D::LoadDictionary { .. }
            | D::SetUserDictionary { .. } => self.spell_command(cmd),
            _ => self.shape_command(cmd),
        }
    }
}

impl DocModel for DeckDoc {
    fn kind(&self) -> DocKind {
        DocKind::Deck
    }

    fn interface_language(&mut self, tag: &str) -> bool {
        let prompts = kiln_deck_layout::PromptLanguage::from_tag(tag);
        if prompts == self.prompts {
            return false;
        }
        self.prompts = prompts;
        self.epoch += 1;
        true
    }

    fn properties(&self) -> Option<kiln_protocol::commands::DocPropertiesInfo> {
        let mut info = self.user_props().info();
        info.slides = u32::try_from(self.pres.slides.len()).unwrap_or(u32::MAX);
        Some(info)
    }

    fn set_properties(
        &mut self,
        edit: &kiln_protocol::commands::write_features::PropertiesEdit,
    ) -> Result<bool> {
        let mut p = self.user_props();
        if !p.edit(edit)? {
            return Ok(false);
        }
        self.props = Some(p);
        // Office keeps no undo step for properties; the presentation has unsaved changes.
        self.state = self.next_state;
        self.next_state += 1;
        Ok(true)
    }

    fn inspect(&self) -> Vec<kiln_protocol::InspectItem> {
        use kiln_protocol::{InspectItem, InspectKind};
        let slides = &self.pres.slides;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        vec![
            InspectItem {
                kind: InspectKind::Comments,
                count: count(slides.iter().map(|s| s.comments.len()).sum()),
            },
            InspectItem {
                kind: InspectKind::SpeakerNotes,
                count: count(
                    slides
                        .iter()
                        .filter(|s| s.notes.as_ref().is_some_and(|n| !n.text.is_empty()))
                        .count(),
                ),
            },
            InspectItem {
                kind: InspectKind::HiddenSlides,
                count: count(slides.iter().filter(|s| s.hidden).count()),
            },
            InspectItem {
                kind: InspectKind::Properties,
                count: crate::props::personal_count(&self.user_props().info()),
            },
        ]
    }

    fn inspect_remove(&mut self, item: kiln_protocol::InspectKind) -> Result<bool> {
        use kiln_deck::ops::{Op, Transaction};
        use kiln_protocol::InspectKind;
        const LABEL: &str = "suite.file.inspect_remove";
        match item {
            InspectKind::Comments => {
                let ops: Vec<Op> = self
                    .pres
                    .slides
                    .iter()
                    .flat_map(|s| {
                        (0..s.comments.len())
                            .rev()
                            .map(move |index| Op::RemoveComment { slide: s.id, index })
                    })
                    .collect();
                self.end_text();
                self.apply(LABEL, Transaction::new(ops))
            }
            InspectKind::SpeakerNotes => {
                let ops: Vec<Op> = self
                    .pres
                    .slides
                    .iter()
                    .filter(|s| s.notes.is_some())
                    .map(|s| Op::SetNotes {
                        slide: s.id,
                        notes: None,
                    })
                    .collect();
                self.end_text();
                self.apply(LABEL, Transaction::new(ops))
            }
            InspectKind::HiddenSlides => {
                let ids: Vec<kiln_common::NodeId> = self
                    .pres
                    .slides
                    .iter()
                    .filter(|s| s.hidden)
                    .map(|s| s.id)
                    .collect();
                if ids.is_empty() {
                    return Ok(false);
                }
                let current = self.view.current;
                self.end_text();
                self.apply_with(LABEL, |p| kiln_deck::edit::delete_slides(p, &ids))?;
                let n = self.pres.slides.len();
                self.go_to(current.min(n.saturating_sub(1)));
                Ok(true)
            }
            InspectKind::Properties => self.set_properties(&crate::props::clear_personal()),
            other => Err(KilnError::Unsupported(format!(
                "Kiln Deck presentations have no {other:?}"
            ))),
        }
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        let before_state = self.state;
        let before_view = self.view.clone();
        let before_epoch = self.epoch;
        let result = match command {
            Command::Suite(SuiteCommand::Edit(kiln_protocol::commands::EditCommand::Repeat)) => {
                match self.repeat.clone() {
                    // The repeated command is the one repeated next time too.
                    Some(c) => return self.execute(&c),
                    None => Ok(Change::None),
                }
            }
            Command::Deck(d) => self.deck(d),
            Command::Suite(SuiteCommand::Edit(e)) => self.edit_command(e),
            Command::Suite(
                c @ (SuiteCommand::ChartInsert { .. }
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
                | SuiteCommand::ChartFormatArea { .. }),
            ) => self.chart_command(c),
            Command::Suite(other) => Err(KilnError::Unsupported(format!(
                "{} in Kiln Deck",
                other.meta().id
            ))),
            other => Err(KilnError::Unsupported(format!(
                "{} in Kiln Deck",
                other.id()
            ))),
        };
        self.validate_view();
        let change = result?;
        if self.state != before_state && repeatable(command) {
            self.repeat = Some(command.clone());
        }
        if self.state != before_state || change == Change::Content {
            return Ok(Change::Content);
        }
        if change == Change::Selection || self.view != before_view || self.epoch != before_epoch {
            return Ok(Change::Selection);
        }
        Ok(Change::None)
    }

    fn undo(&mut self) -> bool {
        self.cancel_drag();
        if self
            .view
            .text
            .as_ref()
            .is_some_and(|t| t.composition.is_some())
        {
            if let Some(t) = &mut self.view.text {
                t.composition = None;
            }
            self.epoch += 1;
            return true;
        }
        match self.undo.undo(&mut self.pres) {
            Ok(Some(tx)) => {
                if let Some(step) = self.done.pop() {
                    self.undone.push(step);
                }
                self.state = self.done.last().map_or(0, |s| s.state);
                self.after_history(&tx);
                true
            }
            _ => false,
        }
    }

    fn redo(&mut self) -> bool {
        self.cancel_drag();
        match self.undo.redo(&mut self.pres) {
            Ok(Some(tx)) => {
                if let Some(step) = self.undone.pop() {
                    self.state = step.state;
                    self.done.push(step);
                }
                self.after_history(&tx);
                true
            }
            _ => false,
        }
    }

    fn can_undo(&self) -> bool {
        // a shared presentation undoes through the engine.
        self.shared_undo()
            .map_or_else(|| self.undo.can_undo(), |(u, _)| u)
    }

    fn can_redo(&self) -> bool {
        self.shared_undo()
            .map_or_else(|| self.undo.can_redo(), |(_, r)| r)
    }

    fn collab(&mut self) -> Option<&mut dyn crate::collab::Collaborative> {
        Some(self)
    }

    fn to_bundle(&self) -> Result<kiln_storage::Bundle> {
        DeckDoc::to_bundle(self)
    }

    fn bundles(&self) -> bool {
        true
    }

    fn set_journaling(&mut self, on: bool) {
        DeckDoc::set_journaling(self, on);
    }

    fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        DeckDoc::take_journal(self)
    }

    fn apply_journal(&mut self, op: &[u8], _: &crate::JournalReplay) -> Result<()> {
        DeckDoc::apply_journal(self, op)
    }

    fn undo_label(&self) -> Option<&'static str> {
        self.done.last().map(|s| s.label)
    }

    fn redo_label(&self) -> Option<&'static str> {
        self.undone.last().map(|s| s.label)
    }

    fn state(&self) -> StateId {
        self.state
    }

    fn seal_history(&mut self) {
        self.undo.break_group();
    }

    fn selection(&self) -> Selection {
        let kind = match (&self.view.text, self.view.shapes.is_empty()) {
            (Some(t), _) => SelKind::Text {
                anchor: kiln_protocol::TextPos {
                    block: t.anchor.para,
                    offset: u32::try_from(t.anchor.offset).unwrap_or(u32::MAX),
                },
                head: kiln_protocol::TextPos {
                    block: t.head.para,
                    offset: u32::try_from(t.head.offset).unwrap_or(u32::MAX),
                },
            },
            (None, false) => SelKind::Shapes {
                ids: self.view.shapes.clone(),
            },
            (None, true) => SelKind::Shapes {
                ids: self.selected_slide_ids(),
            },
        };
        Selection {
            kind,
            ..Selection::default()
        }
    }

    fn status(&self) -> StatusBar {
        let total = u32::try_from(self.pres.slides.len()).unwrap_or(u32::MAX);
        StatusBar {
            slide: (total > 0).then(|| kiln_protocol::Position {
                current: u32::try_from(self.view.current + 1).unwrap_or(1),
                total,
            }),
            language: Some(self.language()),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<kiln_protocol::ParagraphText>> {
        let id = self.current_slide()?;
        let slide = self.pres.slide(id)?;
        let mut out = Vec::new();
        for s in &slide.shapes {
            s.walk_text(&mut |body| {
                for p in body.paragraphs() {
                    out.push(kiln_protocol::ParagraphText {
                        id: p.id,
                        text: p.content.plain_text(true),
                    });
                }
            });
        }
        Some(out)
    }

    fn a11y(&self, title: &str) -> A11yTree {
        let Some(slide) = self.current_slide() else {
            return A11yTree::default();
        };
        let active = self.view.shapes.first().copied();
        kiln_deck_layout::a11y::slide_tree(&self.pres, slide, title, active, None)
    }

    fn a11y_document(&self, title: &str) -> A11yTree {
        let mut tree =
            crate::a11y::presentation_tree(&self.pres, title, self.view.shapes.first().copied());
        // The text caret, on the shape's paragraph node, for the text view.
        if let Some(t) = &self.view.text
            && let kiln_deck::ops::TextTarget::Shape(shape) = t.target
            && let Some(body) = self.body(t.target)
            && let Some(i) = body.paragraphs().position(|p| p.id == t.head.para)
        {
            tree.caret = Some(kiln_protocol::selection::TextPos {
                block: kiln_deck_layout::a11y::paragraph_id(shape, i),
                offset: u32::try_from(t.head.offset).unwrap_or(u32::MAX),
            });
        }
        tree
    }

    fn selected_text(&self) -> Option<String> {
        self.selected_text_string()
    }

    fn app_query(&self, query: &Query) -> Result<QueryResult> {
        match query {
            Query::Deck(q) => Ok(QueryResult::Deck(self.deck_query(q)?)),
            _ => Err(KilnError::Unsupported(
                "the query for a Deck presentation".into(),
            )),
        }
    }

    fn set_clock(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    fn take_clipboard(&mut self) -> Option<ClipboardData> {
        self.copied.take()
    }

    fn frame_source(&mut self) -> Option<&mut dyn crate::frame::FrameSource> {
        Some(self)
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &FORMATS
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        self.write(format, None)
    }

    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        self.write(format, Some(now_ms))
    }

    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let mut issues = self.compat(format);
        if self.original_lost && save::pptx_kind(format).is_some() {
            issues.push(crate::storage::original_lost("PowerPoint"));
        }
        issues
    }

    fn print(&self, job: &crate::print::PrintJob<'_>) -> Result<crate::print::PrintOutput> {
        self.print_job(job)
    }

    fn prepare_query(&mut self, query: &Query) {
        if let Query::Deck(kiln_protocol::commands::DeckQuery::ShowFrame {
            now_ms,
            width,
            height,
            dpr,
        }) = query
        {
            let f = self.show_frame(*now_ms, *width, *height, *dpr);
            *self.show_frame.get_mut() = f;
        }
    }

    fn take_notices(&mut self) -> Vec<crate::model::ModelNotice> {
        std::mem::take(&mut self.notices)
    }
}

impl DeckDoc {
    /// The presentation's editing language: that of its default text style (`en-US` when the
    /// file names none, as PowerPoint assumes).
    pub(crate) fn language(&self) -> String {
        self.pres
            .default_text_style
            .level(0)
            .run
            .lang
            .and_then(|l| l.val)
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| "en-US".to_owned())
    }

    /// The owner of the current slide.
    pub(crate) fn owner(&self) -> Option<Owner> {
        match self.view.master {
            Some(id) if self.pres.master(id).is_some() => Some(Owner::Master(id)),
            Some(id) => Some(Owner::Layout(id)),
            None => self.current_slide().map(Owner::Slide),
        }
    }
}

/// Whether F4 (Repeat) repeats `command`: formatting, arranging and inserting, as in PowerPoint;
/// not gestures, selection, caret motion, the clipboard or history.
fn repeatable(command: &Command) -> bool {
    use kiln_protocol::commands::EditCommand as E;
    match command {
        Command::Deck(d) => matches!(
            d,
            DeckCommand::NewSlide { .. }
                | DeckCommand::Duplicate
                | DeckCommand::DeleteSlides
                | DeckCommand::SetLayout { .. }
                | DeckCommand::ResetSlides
                | DeckCommand::HideSlides { .. }
                | DeckCommand::InsertShape { .. }
                | DeckCommand::InsertTextBox { .. }
                | DeckCommand::InsertTable { .. }
                | DeckCommand::InsertSlideNumber
                | DeckCommand::Nudge { .. }
                | DeckCommand::Resize { .. }
                | DeckCommand::SetTransform { .. }
                | DeckCommand::Rotate { .. }
                | DeckCommand::Flip { .. }
                | DeckCommand::Align { .. }
                | DeckCommand::Distribute { .. }
                | DeckCommand::Group
                | DeckCommand::Ungroup
                | DeckCommand::Order { .. }
                | DeckCommand::Delete
                | DeckCommand::SetFill { .. }
                | DeckCommand::SetOutline { .. }
                | DeckCommand::SetEffect { .. }
                | DeckCommand::QuickStyle { .. }
                | DeckCommand::ChangeShape { .. }
                | DeckCommand::AlignText { .. }
                | DeckCommand::TextDirection { .. }
                | DeckCommand::Columns { .. }
                | DeckCommand::Autofit { .. }
                | DeckCommand::CharacterSpacing { .. }
                | DeckCommand::TextShadow { .. }
                | DeckCommand::PasteFormat
                | DeckCommand::Promote
                | DeckCommand::Demote
                | DeckCommand::SetBackground { .. }
                | DeckCommand::HideBackgroundGraphics { .. }
        ),
        Command::Suite(SuiteCommand::Edit(e)) => !matches!(
            e,
            E::Repeat
                | E::Cut
                | E::Copy
                | E::Paste { .. }
                | E::PasteKeepSource { .. }
                | E::PasteMerge { .. }
                | E::PasteTextOnly { .. }
                | E::PasteAs { .. }
        ),
        _ => false,
    }
}
