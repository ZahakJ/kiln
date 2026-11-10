// SPDX-License-Identifier: AGPL-3.0-or-later
//! The one way a workbook changes. Every change to a workbook's model (sheet commands, their undo
//! and redo, transactions, formats, ids, people, lists, recalculation, charts, pictures and
//! properties) goes through [`Ledger`], which writes it in the autosave journal as it applies it.
//! Nothing else in Kiln Grid mutates the workbook: `the_workbook_is_changed_only_here` fails on a
//! call that goes round it, and the storage tests replay every command of the catalog from its
//! journal and compare.
//!
//! The ledger is also the undo history. It keeps each step as the command and its inverse, as
//! the sheet model's own log does, without a copy of anything: an undo takes its step off the top.
//!
//! A command whose result depends on values cells show (a sort, Remove Duplicates, a fill, a
//! filter, Paste Values, Text to Columns) goes in the journal as what it came to
//! ([`Capture`]): replaying it recalculates, and `RAND` would give another order.

use std::collections::VecDeque;

use kiln_sheet::command_log::LogEntry;
use kiln_sheet::styles::StyleTable;
use kiln_sheet::{Command, ModelError, SheetId, Workbook};

use super::storage::{Capture, GridOp, Replaying};

/// A workbook, which only the ledger can change: everyone else reads it (`Deref`), and no
/// `&mut Workbook` leaves this module. A change that does not go through the ledger does not
/// compile: there is no `DerefMut`, the workbook inside is private, a `Book` is made only here
/// (as a document opens, `GridDoc::open_doc`), and a replay's entry points take a
/// `Replaying` that only the journal's replay (`storage.rs`) can make. The doc tests below are
/// another crate; what this crate's own command modules can reach is checked by `cargo xtask
/// verify`, which builds the bypasses of `structure.rs` (`ownership-plants`) and requires each to
/// fail, and at run time by the workbook's mutation stamp ([`Ledger::holds`]).
///
/// ```compile_fail,E0308
/// # fn f(book: &mut kiln_session::grid_ledger::Book) {
/// // There is no way to the workbook but a shared one.
/// let wb: &mut kiln_sheet::Workbook = book;
/// # }
/// ```
///
/// ```compile_fail,E0596
/// # fn f(book: &mut kiln_session::grid_ledger::Book) {
/// // A rebound name changes nothing: the workbook is read-only through it too.
/// let other = &mut *book;
/// let _ = other.apply(&kiln_sheet::Command::noop());
/// # }
/// ```
///
/// ```compile_fail,E0616
/// # fn f(book: &mut kiln_session::grid_ledger::Book) {
/// // Nor is the workbook inside it.
/// book.inner.recalc_all();
/// # }
/// ```
///
/// ```compile_fail,E0624
/// // Review 4's B2: a new `Book` swapped in, with a change made to its workbook first. Only
/// // the ledger makes one.
/// let book = kiln_session::grid_ledger::Book::open(kiln_sheet::Workbook::new());
/// ```
///
/// ```compile_fail,E0423
/// // Review 4's B1: the replay's entry point used for an edit. It takes a `Replaying`, which
/// // only the journal's replay can make.
/// let token = kiln_session::grid_ledger::Replaying(());
/// ```
///
/// ```
/// # fn f(book: &kiln_session::grid_ledger::Book) {
/// // Reading is open to all.
/// let _ = book.sheets().len();
/// # }
/// ```
#[derive(Debug)]
pub struct Book {
    inner: Workbook,
    /// The workbook's mutation stamp as the ledger left it: a stamp that moved without the
    /// ledger is a change made round it (`Ledger::holds`).
    seen: (u64, u64),
}

impl std::ops::Deref for Book {
    type Target = Workbook;
    fn deref(&self) -> &Workbook {
        &self.inner
    }
}

impl Book {
    /// The workbook of a document as it opens: only `GridDoc::open_doc`, below.
    fn open(wb: Workbook) -> Self {
        let seen = wb.mutation_stamp();
        Self { inner: wb, seen }
    }

    /// The ledger made a change: the stamp it leaves is the one it knows.
    fn settle(&mut self) {
        self.seen = self.inner.mutation_stamp();
    }

    /// The model's notes of what changed since the last call (what to repaint): not the
    /// document's content.
    pub(super) fn take_changes(&mut self) -> kiln_sheet::workbook::Changes {
        self.inner.take_changes()
    }

    /// Work that only reads the workbook but fills its caches as it goes. Checking builds make
    /// sure the document is the same afterwards; nothing outside this module gets the
    /// `&mut Workbook`, only the reads below.
    fn with_caches<R>(&mut self, f: impl FnOnce(&mut Workbook) -> R) -> R {
        #[cfg(any(test, feature = "journal-check"))]
        let before = self.inner.to_bytes().ok();
        let out = f(&mut self.inner);
        #[cfg(any(test, feature = "journal-check"))]
        assert!(
            before == self.inner.to_bytes().ok(),
            "a read of the workbook changed it: that goes through the ledger"
        );
        self.settle();
        out
    }

    /// Draws the visible cells into the scene.
    pub(super) fn prepare_scene(
        &mut self,
        scene: &mut kiln_sheet_layout::GridScene,
        sheet_index: usize,
        geo: &kiln_sheet_layout::SheetGeometry,
        req: kiln_sheet_layout::SceneRequest<'_>,
    ) {
        self.with_caches(|wb| scene.prepare(wb, sheet_index, geo, req));
    }

    /// Whether a value passes the cell's validation.
    pub(super) fn validate(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        value: &kiln_sheet::CellValue,
    ) -> kiln_sheet::validation::Validity {
        self.with_caches(|wb| wb.validate(sheet, row, col, value))
    }

    /// The cell's validation list.
    pub(super) fn validation_list(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> Option<Vec<kiln_sheet::CellValue>> {
        self.with_caches(|wb| wb.validation_list(sheet, row, col))
    }

    /// Which rows of `data` a filter's columns let through.
    pub(super) fn filter_pass(
        &mut self,
        sheet: SheetId,
        data: kiln_sheet::Range,
        columns: &[kiln_sheet::meta::FilterColumn],
    ) -> Vec<bool> {
        self.with_caches(|wb| wb.filter_pass(sheet, data, columns))
    }

    /// The conditional formats' looks over an area (tests).
    #[cfg(test)]
    pub(super) fn conditional_formats(
        &mut self,
        sheet: SheetId,
        area: kiln_sheet::Range,
    ) -> rustc_hash::FxHashMap<kiln_sheet::CellPos, kiln_sheet::cf::CellVisual> {
        self.with_caches(|wb| wb.conditional_formats(sheet, area))
    }

    /// The conditional formats of the pages to print.
    pub(super) fn print_conditional_formats(
        &mut self,
        opts: &kiln_sheet_layout::print::pages::PrintOptions,
    ) -> Vec<rustc_hash::FxHashMap<kiln_sheet::CellPos, kiln_sheet::cf::CellVisual>> {
        self.with_caches(|wb| kiln_sheet_layout::print::pages::conditional_formats(wb, opts))
    }

    /// A formula worked out at a cell, without entering it.
    pub(super) fn evaluate_at(
        &mut self,
        formula: &str,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> kiln_sheet::Result<kiln_formula::Value> {
        self.with_caches(|wb| wb.evaluate_at(formula, sheet, row, col))
    }

    /// Evaluate Formula's first step.
    pub(super) fn explain(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> kiln_sheet::Result<kiln_formula::engine::ExplainStep> {
        self.with_caches(|wb| wb.explain(sheet, row, col))
    }

    /// The cells that break their validation.
    pub(super) fn invalid_cells(
        &mut self,
        sheet: SheetId,
        area: kiln_sheet::Range,
    ) -> Vec<kiln_sheet::CellPos> {
        self.with_caches(|wb| wb.invalid_cells(sheet, area))
    }

    /// Tests: the style table swapped for another (a workbook with fewer formats).
    #[cfg(test)]
    pub(super) fn swap_styles_for_test(&mut self, styles: &mut StyleTable) {
        std::mem::swap(self.inner.styles_mut(), styles);
        self.settle();
    }

    /// The command a paste would apply (it may add formats to the table, which go in the journal
    /// ahead of the command).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paste_command(
        &mut self,
        sheet: SheetId,
        at: kiln_sheet::Range,
        clip: &kiln_sheet::clipboard::ClipData,
        what: kiln_sheet::commands::PasteWhat,
        op: kiln_sheet::commands::PasteOp,
        transpose: bool,
        skip_blanks: bool,
    ) -> kiln_sheet::Result<Command> {
        let out = self
            .inner
            .paste_command(sheet, at, clip, what, op, transpose, skip_blanks);
        self.settle();
        out
    }
}

impl super::GridDoc {
    /// Tests: a document of `workbook`, as a file opens.
    #[cfg(test)]
    pub(super) fn new(
        workbook: Workbook,
        source: Option<kiln_io_xlsx::XlsxSource>,
        warnings: Vec<String>,
        file_name: Option<String>,
    ) -> super::GridDoc {
        Self::open_doc(workbook, source, warnings, file_name)
    }

    /// A document of `workbook`: the one place a [`Book`] is made. Private to this module, as
    /// [`GridDoc::from_bundle`] is: a command cannot make a second document to take its
    /// workbook from (`cargo xtask verify` checks it with the `ownership-plants` feature).
    fn open_doc(
        workbook: Workbook,
        source: Option<kiln_io_xlsx::XlsxSource>,
        warnings: Vec<String>,
        file_name: Option<String>,
    ) -> super::GridDoc {
        let active = workbook
            .props()
            .active_sheet
            .min(workbook.sheets().len().saturating_sub(1));
        let mut views = rustc_hash::FxHashMap::default();
        for s in workbook.sheets() {
            views.insert(s.id(), super::ViewState::from_meta(&s.meta().view));
        }
        let charts = super::charts::Charts::from_source(source.as_ref());
        let workbook = Book::open(workbook);
        let book = workbook.seen.0;
        let mut doc = super::GridDoc {
            workbook,
            source,
            original_lost: false,
            warnings,
            file_name,
            log: Ledger {
                book,
                ..Ledger::default()
            },
            journal: super::storage::JournalState::default(),
            undo_floor: 0,
            #[cfg(any(test, feature = "journal-check"))]
            shadow: None,
            #[cfg(any(test, feature = "journal-check"))]
            shadow_state: 0,
            #[cfg(any(test, feature = "journal-check"))]
            check_sources: None,
            #[cfg(test)]
            auto_check: false,
            done: Vec::new(),
            undone: Vec::new(),
            state: 0,
            next_state: 1,
            views,
            active,
            edit: None,
            scene: None,
            fonts_seen: 0,
            geo: None,
            last_view: kiln_sheet_layout::View {
                width: 0.0,
                height: 0.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                zoom: 1.0,
            },
            copy: None,
            clip: None,
            copied: None,
            fill_preview: None,
            reveal: None,
            now_ms: 0,
            selection_epoch: 0,
            tab_origin: None,
            charts,
            features: super::features::FeatureState::default(),
            analysis: super::analysis::AnalysisState::default(),
            home: super::home::HomeState::default(),
            surface: super::surface::SurfaceState::default(),
            props: None,
        };
        // A hidden active sheet (or an empty workbook made by a reader) shows the first visible one.
        if doc
            .workbook
            .sheets()
            .get(doc.active)
            .is_some_and(|s| s.meta().visibility != kiln_sheet::meta::SheetVisibility::Visible)
        {
            doc.active = doc
                .workbook
                .sheets()
                .iter()
                .position(|s| s.meta().visibility == kiln_sheet::meta::SheetVisibility::Visible)
                .unwrap_or(0);
        }
        doc.show_fresh_charts();
        // Grid's own tests: every document journals with the shadow check on, and each command
        // is checked as it ends, unless a session takes the journal over.
        #[cfg(test)]
        if !super::storage::MAKING_SHADOW.with(std::cell::Cell::get) {
            doc.start_journal(true);
            doc.auto_check = true;
        }
        doc
    }
}

/// Kiln Grid's documents are made here and nowhere else.
mod factory {
    use kiln_common::{KilnError, Result};
    use kiln_io_csv::{ImportOptions, read_csv};
    use kiln_io_xlsx::read_xlsx;
    use kiln_protocol::{DocKind, FileFormat, OpenHint};
    use kiln_sheet::Workbook;

    use super::super::{FORMATS, GridApp, GridDoc};
    use super::Ledger;
    use crate::model::{AppFactory, DocModel};

    impl AppFactory for GridApp {
        fn kind(&self) -> DocKind {
            DocKind::Grid
        }

        fn opens(&self, format: FileFormat) -> bool {
            FORMATS.contains(&format)
        }

        fn create(&self) -> Box<dyn DocModel> {
            Box::new(GridDoc::open_doc(Workbook::new(), None, Vec::new(), None))
        }

        /// a new workbook in the default editing language: its first sheet is named in it
        /// (Tabelle1, ورقة1), and in a language written from right to left the sheet runs from the
        /// right, as Excel's do.
        fn create_in(&self, language: &str) -> Box<dyn DocModel> {
            let mut wb = Workbook::with_options(kiln_sheet::WorkbookOptions {
                sheets: Some(vec![crate::props::first_sheet_name(language)]),
                ..kiln_sheet::WorkbookOptions::default()
            });
            if kiln_doc::is_rtl_language(language)
                && let Some(sheet) = wb.sheets().first()
            {
                let id = sheet.id();
                let mut view = sheet.meta().view.clone();
                view.right_to_left = true;
                // A new workbook: its direction is not an edit to undo.
                Ledger::before_any_journal(
                    &mut wb,
                    &kiln_sheet::Command::SetSheetMeta {
                        sheet: id,
                        field: Box::new(kiln_sheet::commands::MetaField::View(view)),
                    },
                );
                let _ = wb.take_changes();
            }
            Box::new(GridDoc::open_doc(wb, None, Vec::new(), None))
        }

        fn open_bundle(
            &self,
            bundle: &kiln_storage::Bundle,
            hint: &OpenHint,
        ) -> Result<Box<dyn DocModel>> {
            let name = hint.path.clone().or_else(|| hint.name.clone());
            Ok(Box::new(GridDoc::open_bundle_doc(bundle, name, false)?))
        }

        fn open(
            &self,
            bytes: &[u8],
            format: FileFormat,
            hint: &OpenHint,
        ) -> Result<Box<dyn DocModel>> {
            let name = hint.path.clone().or_else(|| hint.name.clone());
            let format_error = |message: String| KilnError::Format {
                format: format.extension().to_owned(),
                message,
            };
            match format {
                FileFormat::Csv => {
                    let stem = name
                        .as_deref()
                        .and_then(|n| std::path::Path::new(n).file_stem())
                        .and_then(|s| s.to_str())
                        .map(|s| s.chars().take(31).collect());
                    let imp = read_csv(
                        bytes,
                        &ImportOptions {
                            sheet_name: stem,
                            ..ImportOptions::default()
                        },
                    )
                    .map_err(|e| format_error(e.to_string()))?;
                    Ok(Box::new(GridDoc::open_doc(
                        imp.workbook,
                        None,
                        imp.report.warnings,
                        name,
                    )))
                }
                #[cfg(feature = "odf")]
                FileFormat::Ods => {
                    let read =
                        kiln_io_odf::read_ods(bytes).map_err(|e| format_error(e.to_string()))?;
                    let mut doc = GridDoc::open_doc(read.workbook, None, read.warnings, name);
                    doc.charts.load(read.charts);
                    Ok(Box::new(doc))
                }
                FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xltx => {
                    let read = read_xlsx(bytes).map_err(|e| format_error(e.to_string()))?;
                    Ok(Box::new(GridDoc::open_doc(
                        read.workbook,
                        Some(read.source),
                        read.warnings,
                        name,
                    )))
                }
                other => Err(KilnError::Unsupported(format!(
                    ".{} files in Kiln Grid",
                    other.extension()
                ))),
            }
        }
    }
}

impl super::GridDoc {
    /// Tests: the document of a bundle.
    #[cfg(test)]
    pub(super) fn from_bundle(
        bundle: &kiln_storage::Bundle,
        name: Option<String>,
    ) -> kiln_common::Result<super::GridDoc> {
        Self::open_bundle_doc(bundle, name, false)
    }

    /// The document of a bundle (a recovery, a `.kgrid`); `as_shown` keeps every formula's
    /// stored result, for the shadow check's copy. Private, as [`GridDoc::open_doc`] is.
    fn open_bundle_doc(
        bundle: &kiln_storage::Bundle,
        name: Option<String>,
        as_shown: bool,
    ) -> kiln_common::Result<super::GridDoc> {
        use super::storage::{CHARTS, ORIGINAL, err};
        let opts = kiln_sheet::WorkbookOptions::default();
        let wb = if as_shown {
            Workbook::from_bytes_as_shown(&bundle.model, opts)
        } else {
            Workbook::from_bytes(&bundle.model, opts)
        }
        .map_err(err)?;
        // Saving as Excel compares the workbook with the original as it was read, so every
        // part the edits left alone is copied byte for byte; an original that cannot be read is
        // a loss the Compatibility Checker reports.
        let read = bundle
            .passthrough
            .get(ORIGINAL)
            .map(|bytes| kiln_io_xlsx::read_xlsx(bytes).map(|read| read.source));
        let lost = matches!(read, Some(Err(_)));
        let source = read.and_then(std::result::Result::ok);
        let mut d = Self::open_doc(wb, source, Vec::new(), name);
        d.original_lost = lost;
        d.props = crate::props::UserProps::from_bundle(bundle);
        let charts: Vec<(String, kiln_charts::ChartSpace)> = bundle
            .passthrough
            .iter()
            .filter_map(|(n, bytes)| {
                let part = n.strip_prefix(CHARTS)?;
                let chart = kiln_charts::read::read_chart(bytes).ok()?;
                Some((format!("/{part}"), chart))
            })
            .collect();
        d.charts.load(charts);
        d.features.objects.restore_media(
            bundle
                .media
                .iter()
                .map(|(k, v)| (format!("/{}", k.trim_start_matches('/')), v.clone())),
        );
        Ok(d)
    }

    /// Checking builds: the shadow copy the journal's records are replayed onto, made from the
    /// document as it is. Both draw `RAND` and read the clock alike ([`CheckSources`]), so the
    /// volatile functions' values match by construction and every cell is compared.
    #[cfg(any(test, feature = "journal-check"))]
    pub(super) fn install_shadow(&mut self) {
        let sources = self
            .check_sources
            .get_or_insert_with(|| std::sync::Arc::new(CheckSources::new()))
            .clone();
        self.workbook
            .inner
            .set_volatile_sources(Some(sources.clock()), Some(sources.recording()));
        self.workbook.settle();
        super::storage::MAKING_SHADOW.with(|m| m.set(true));
        let shadow = self
            .to_bundle()
            .ok()
            .and_then(|b| Self::open_bundle_doc(&b, None, true).ok())
            .map(|mut s| {
                s.workbook
                    .inner
                    .set_volatile_sources(Some(sources.clock()), Some(sources.replaying()));
                // Its volatile cells take numbers of their own, as a recovery's would.
                if s.workbook.props().calc.mode != kiln_sheet::meta::CalcMode::Manual {
                    s.workbook.inner.recalc();
                    s.absorb_changes();
                }
                s.workbook.settle();
                s.active = self.active;
                Box::new(s)
            });
        super::storage::MAKING_SHADOW.with(|m| m.set(false));
        self.shadow = shadow;
    }
}

/// Checking builds: one clock for the document and its shadow, which moves on only between
/// commands, and the random numbers each formula cell of the document drew, which the shadow's
/// same cell draws again once its records are replayed. A volatile cell the shadow works out is
/// then what the document showed, whatever order the two calculate in. While the shadow replays
/// a command it draws numbers of its own, as a recovery does: a record that depends on what
/// `RAND` showed (a sort recorded as the command, not as the rows it wrote) gives another result
/// and is caught. Under manual calculation nothing is drawn again but what was entered, so it
/// replays with the document's numbers throughout.
/// The numbers each formula cell drew: (sheet, row, column, draw) to the number.
#[cfg(any(test, feature = "journal-check"))]
type Tape = rustc_hash::FxHashMap<(u32, u32, u32, u32), f64>;

#[cfg(any(test, feature = "journal-check"))]
#[derive(Debug)]
pub(super) struct CheckSources {
    now: std::sync::Arc<std::sync::atomic::AtomicU64>,
    tape: std::sync::Arc<std::sync::Mutex<Tape>>,
    /// The shadow draws the document's numbers (else numbers of its own).
    use_tape: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(any(test, feature = "journal-check"))]
impl CheckSources {
    fn new() -> Self {
        let s = Self {
            now: std::sync::Arc::default(),
            tape: std::sync::Arc::default(),
            use_tape: std::sync::Arc::default(),
        };
        s.tick();
        s
    }

    /// The clock moves on: between commands, never within one.
    pub(super) fn tick(&self) {
        self.now.store(
            kiln_formula::engine::system_clock().to_bits(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn clock(&self) -> kiln_formula::engine::Clock {
        let now = std::sync::Arc::clone(&self.now);
        std::sync::Arc::new(move || f64::from_bits(now.load(std::sync::atomic::Ordering::Relaxed)))
    }

    fn recording(&self) -> kiln_formula::engine::RandomSource {
        let tape = std::sync::Arc::clone(&self.tape);
        std::sync::Arc::new(move |key| {
            let v = kiln_formula::functions::math::random();
            if let Some((s, r, c, n)) = key {
                tape.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert((s.0, r, c, n), v);
            }
            v
        })
    }

    /// Whether the shadow draws the document's numbers from now on.
    pub(super) fn use_tape(&self, on: bool) {
        self.use_tape
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    fn replaying(&self) -> kiln_formula::engine::RandomSource {
        let tape = std::sync::Arc::clone(&self.tape);
        let use_tape = std::sync::Arc::clone(&self.use_tape);
        std::sync::Arc::new(move |key| {
            key.filter(|_| use_tape.load(std::sync::atomic::Ordering::Relaxed))
                .and_then(|(s, r, c, n)| {
                    tape.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get(&(s.0, r, c, n))
                        .copied()
                })
                .unwrap_or_else(kiln_formula::functions::math::random)
        })
    }
}

/// The undo steps kept.
pub(super) const UNDO_STEPS: usize = 1_000;

#[derive(Debug, Default)]
struct Txn {
    label: String,
    forward: Vec<Command>,
    inverse: Vec<Command>,
    /// What the journal gets if the transaction is committed.
    ops: Vec<GridOp>,
}

/// The undo history and the journal's entry point (see the module documentation).
#[derive(Debug, Default)]
pub(super) struct Ledger {
    undo: VecDeque<LogEntry>,
    redo: Vec<LogEntry>,
    txn: Vec<Txn>,
    /// Steps added so far: a count that only grows, so an edit is told from a command that
    /// changed nothing whatever the history's limit.
    pushed: u64,
    /// The operations since the journal last took them, while it records.
    recorded: Option<Vec<GridOp>>,
    /// The identity of the workbook this ledger changes ([`Workbook::mutation_stamp`]).
    book: u64,
}

type Result<T> = std::result::Result<T, ModelError>;

/// Whether a command replays to the same result from the command alone.
fn plain(wb: &Workbook, cmd: &Command) -> bool {
    matches!(Capture::of(wb, cmd), Capture::Command)
}

impl Ledger {
    /// Whether every change to the workbook since it opened came through here: its stamp is the
    /// one the ledger left, on the workbook it opened with. The journal checks it after each
    /// command (`GridDoc::take_journal`), as a guard on what privacy cannot see.
    pub(super) fn holds(&self, wb: &Book) -> bool {
        wb.seen.0 == self.book && wb.inner.mutation_stamp() == wb.seen
    }

    /// What [`Ledger::holds`] found, in words.
    pub(super) fn describe(&self, wb: &Book) -> String {
        let now = wb.inner.mutation_stamp();
        if now.0 != self.book {
            "another workbook took its place".to_owned()
        } else {
            format!(
                "{} changes the ledger did not make",
                now.1.saturating_sub(wb.seen.1)
            )
        }
    }

    /// The journal goes on from the workbook as it is: after a replay, which writes nothing
    /// again, or after a change round the ledger, which a checkpoint keeps.
    pub(super) fn adopt(&mut self, wb: &mut Book, _: &Replaying) {
        wb.settle();
        self.book = wb.seen.0;
    }

    /// Starts or stops writing the journal.
    pub(super) fn set_recording(&mut self, on: bool) {
        self.recorded = on.then(Vec::new);
    }

    /// The operations since the last call; `None` when nothing records.
    pub(super) fn take_recorded(&mut self) -> Option<Vec<GridOp>> {
        self.recorded.as_mut().map(std::mem::take)
    }

    /// Steps added so far (it never goes down).
    pub(super) fn pushed(&self) -> u64 {
        self.pushed
    }

    /// An operation, in the order the changes were made: with the open transaction's, if any.
    pub(super) fn record(&mut self, op: GridOp) {
        if let Some(t) = self.txn.last_mut() {
            t.ops.push(op);
        } else if let Some(r) = &mut self.recorded {
            r.push(op);
        }
    }

    /// Applies one command and writes it in the journal, returning its inverse. A batch goes
    /// command by command when one of its commands has to be recorded as its outcome.
    fn apply(&mut self, wb: &mut Book, cmd: &Command) -> Result<Command> {
        if let Command::Batch { commands, label } = cmd
            && !commands.iter().all(|c| plain(wb, c))
        {
            let mut inverses = Vec::with_capacity(commands.len());
            for c in commands {
                match self.apply(wb, c) {
                    Ok(i) => inverses.push(i),
                    Err(e) => {
                        for i in inverses.iter().rev() {
                            let _ = self.apply(wb, i);
                        }
                        return Err(e);
                    }
                }
            }
            inverses.reverse();
            return Ok(Command::Batch {
                commands: inverses,
                label: label.clone(),
            });
        }
        let capture = Capture::of(wb, cmd);
        let inverse = wb.inner.apply(cmd);
        wb.settle();
        let inverse = inverse?;
        #[cfg(test)]
        if plant::on(plant::Plant::CopyUnrecorded) && matches!(cmd, Command::CopyRange { .. }) {
            // Planted (the third review's P2): a copy the journal never hears of.
            return Ok(inverse);
        }
        if !inverse.is_noop() {
            let done = capture.outcome(&mut wb.inner, cmd.clone(), &inverse);
            wb.settle();
            self.record(GridOp::Command(done));
        }
        Ok(inverse)
    }

    /// A change that is no undo step of its own (it goes with the step under way, if any).
    pub(super) fn apply_untracked(&mut self, wb: &mut Book, cmd: &Command) -> Result<()> {
        let cmd = wb.resolve(cmd);
        self.apply(wb, &cmd).map(|_| ())
    }

    /// Applies a command as one undo step (or part of the open transaction).
    pub(super) fn exec_labelled(
        &mut self,
        wb: &mut Book,
        command: Command,
        label: &str,
    ) -> Result<()> {
        // With its ids decided, so a replay makes the same sheet and table.
        let cmd = wb.resolve(&command);
        let inverse = self.apply(wb, &cmd)?;
        if let Some(open) = self.txn.last_mut() {
            open.forward.push(cmd);
            open.inverse.push(inverse);
            return Ok(());
        }
        // A command whose inverse does nothing changed nothing: no undo step.
        if inverse.is_noop() {
            return Ok(());
        }
        self.push(LogEntry {
            label: label.to_owned(),
            forward: cmd,
            inverse,
        });
        Ok(())
    }

    fn push(&mut self, entry: LogEntry) {
        self.undo.push_back(entry);
        while self.undo.len() > UNDO_STEPS {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.pushed += 1;
    }

    /// Opens a transaction: commands until the matching [`Ledger::commit`] are one undo step.
    pub(super) fn begin(&mut self, label: &str) {
        self.txn.push(Txn {
            label: label.to_owned(),
            ..Txn::default()
        });
    }

    /// Closes the innermost transaction; its operations go to the journal with it.
    pub(super) fn commit(&mut self, wb: &mut Book) -> Result<()> {
        let t = self
            .txn
            .pop()
            .ok_or_else(|| ModelError::invalid("no open transaction"))?;
        for op in t.ops {
            self.record(op);
        }
        if t.forward.is_empty() {
            return Ok(());
        }
        let one = |mut v: Vec<Command>, label: &str| {
            if v.len() == 1 {
                v.pop().unwrap_or_else(Command::noop)
            } else {
                Command::Batch {
                    commands: v,
                    label: Some(label.to_owned()),
                }
            }
        };
        // Nothing changed: no undo step (and nothing for the journal).
        if t.inverse.iter().all(Command::is_noop) {
            return Ok(());
        }
        let forward = one(t.forward, &t.label);
        let inverse = one(t.inverse.into_iter().rev().collect(), &t.label);
        if let Some(outer) = self.txn.last_mut() {
            outer.forward.push(forward);
            outer.inverse.push(inverse);
            return Ok(());
        }
        // Recalculate once at the end of the outermost transaction (automatic mode).
        let done = wb.inner.apply(&Command::noop());
        wb.settle();
        done?;
        self.push(LogEntry {
            label: t.label,
            forward,
            inverse,
        });
        Ok(())
    }

    /// Reverts everything applied in the innermost open transaction and closes it; the journal
    /// gets none of it.
    pub(super) fn rollback(&mut self, wb: &mut Book) -> Result<()> {
        let t = self
            .txn
            .pop()
            .ok_or_else(|| ModelError::invalid("no open transaction"))?;
        let mut result = Ok(());
        for inv in t.inverse.iter().rev() {
            if let Err(e) = wb.inner.apply(inv) {
                result = Err(e);
            }
        }
        wb.settle();
        // What the inverses do not take back happened all the same (an id handed out, people,
        // lists, a chart, a picture, properties, a recalculation): the journal gets it, so it
        // holds what the workbook holds. The commands' own records go with the commands.
        for op in t.ops {
            if !matches!(op, GridOp::Command(_)) {
                self.record(op);
            }
        }
        result
    }

    /// Runs `f` in a transaction; rolls back and returns the error if it fails.
    pub(super) fn transaction<T>(
        &mut self,
        wb: &mut Book,
        label: &str,
        f: impl FnOnce(&mut Ledger, &mut Book) -> Result<T>,
    ) -> Result<T> {
        self.begin(label);
        match f(self, wb) {
            Ok(v) => {
                self.commit(wb)?;
                Ok(v)
            }
            Err(e) => {
                self.rollback(wb)?;
                Err(e)
            }
        }
    }

    /// Undo. `Ok(false)` when there is nothing to undo.
    pub(super) fn undo(&mut self, wb: &mut Book) -> Result<bool> {
        if !self.txn.is_empty() {
            return Err(ModelError::invalid("cannot undo during a transaction"));
        }
        let Some(entry) = self.undo.pop_back() else {
            return Ok(false);
        };
        match self.apply(wb, &entry.inverse) {
            Ok(_) => {
                // The step moved, changed cells or not: that is a record too.
                self.record(GridOp::Step);
                self.redo.push(entry);
                Ok(true)
            }
            Err(e) => {
                self.undo.push_back(entry);
                Err(e)
            }
        }
    }

    /// Redo. `Ok(false)` when there is nothing to redo. It goes in the journal as what it did
    /// now, not as the command: a sort over `RAND` sorts by today's values.
    pub(super) fn redo(&mut self, wb: &mut Book) -> Result<bool> {
        if !self.txn.is_empty() {
            return Err(ModelError::invalid("cannot redo during a transaction"));
        }
        let Some(entry) = self.redo.pop() else {
            return Ok(false);
        };
        #[cfg(test)]
        if plant::on(plant::Plant::RedoBareForward) {
            // Planted (the third review's P1): a redo that goes round the capture.
            let applied = wb.inner.apply(&entry.forward);
            wb.settle();
            return match applied {
                Ok(inverse) => {
                    self.record(GridOp::Command(entry.forward.clone()));
                    self.undo.push_back(LogEntry { inverse, ..entry });
                    Ok(true)
                }
                Err(e) => {
                    self.redo.push(entry);
                    Err(e)
                }
            };
        }
        match self.apply(wb, &entry.forward) {
            Ok(inverse) => {
                self.record(GridOp::Step);
                self.undo.push_back(LogEntry { inverse, ..entry });
                Ok(true)
            }
            Err(e) => {
                self.redo.push(entry);
                Err(e)
            }
        }
    }

    /// True when Undo is available.
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty() && self.txn.is_empty()
    }

    /// True when Redo is available.
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.txn.is_empty()
    }

    /// Number of undo steps.
    pub(super) fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// A new id for a conditional format or validation rule.
    pub(super) fn alloc_rule_id(&mut self, wb: &mut Book) -> u32 {
        self.record(GridOp::RuleIds(1));
        let id = wb.inner.alloc_rule_id();
        wb.settle();
        id
    }

    /// A cell format, added to the style table if it is new. The table only grows, and what
    /// was added goes in the journal ahead of the operations that use it
    /// (`GridDoc::take_journal`). Nothing else of the table (named and table styles) is open to
    /// a command: a change to those goes through a sheet command.
    pub(super) fn intern_style(
        &mut self,
        wb: &mut Book,
        style: &kiln_sheet::styles::CellStyle,
    ) -> u32 {
        let i = wb.inner.styles_mut().intern(style);
        wb.settle();
        i
    }

    /// A format made from `index` with `patch` applied, added if it is new.
    pub(super) fn patch_style(
        &mut self,
        wb: &mut Book,
        index: u32,
        patch: &kiln_sheet::styles::StylePatch,
    ) -> u32 {
        let i = wb.inner.styles_mut().patch(index, patch);
        wb.settle();
        i
    }

    /// A differential format (conditional formats, table styles), added if it is new.
    pub(super) fn intern_dxf(&mut self, wb: &mut Book, dxf: kiln_sheet::styles::Dxf) -> u32 {
        let i = wb.inner.styles_mut().intern_dxf(dxf);
        wb.settle();
        i
    }

    /// The people of threaded comments.
    pub(super) fn set_persons(&mut self, wb: &mut Book, people: Vec<kiln_sheet::meta::Person>) {
        self.record(GridOp::Persons(people.clone()));
        wb.inner.set_persons(people);
        wb.settle();
    }

    /// Custom fill and sort lists.
    pub(super) fn set_custom_lists(&mut self, wb: &mut Book, lists: Vec<Vec<String>>) {
        self.record(GridOp::CustomLists(lists.clone()));
        wb.inner.set_custom_lists(lists);
        wb.settle();
    }

    /// Calculate Now (`None`) or Calculate Sheet: in manual mode it changes what cells show.
    pub(super) fn recalc(&mut self, wb: &mut Book, sheet: Option<SheetId>) -> Result<()> {
        #[cfg(test)]
        let planted = plant::on(plant::Plant::RecalcUnrecorded);
        #[cfg(not(test))]
        let planted = false;
        // Planted (review 5): Calculate Now applied and not recorded.
        if !planted {
            self.record(GridOp::Recalc(sheet.map(|s| s.0)));
        }
        let done = match sheet {
            None => {
                wb.inner.recalc_all();
                Ok(())
            }
            Some(s) => wb.inner.recalc_sheet(s).map(|_| ()),
        };
        wb.settle();
        done
    }

    /// The view the workbook keeps for its file: each sheet's selection and the sheet on show.
    /// Nothing else changes (`set_view_changes_only_the_view`), and neither is journalled: a
    /// recovered document opens with its own selection.
    pub(super) fn set_view(
        &mut self,
        wb: &mut Book,
        selections: Vec<(SheetId, kiln_sheet::meta::Selection)>,
        active_sheet: Option<usize>,
    ) {
        for (id, selection) in selections {
            if let Some(mut meta) = wb.inner.sheet(id).map(|s| s.meta().clone()) {
                meta.view.selection = selection;
                let _ = wb.inner.load_sheet_meta(id, meta);
            }
        }
        if let Some(active) = active_sheet {
            let mut props = wb.inner.props().clone();
            props.active_sheet = active;
            let _ = wb.inner.apply(&Command::SetProps {
                props: Box::new(props),
            });
        }
        wb.settle();
    }

    /// A change to a workbook nobody has seen yet (a new workbook's direction).
    pub(super) fn before_any_journal(wb: &mut Workbook, cmd: &Command) {
        let _ = wb.apply(cmd);
    }

    /// Applies a journalled operation as a replay does: through here, without writing it again.
    pub(super) fn replay(&mut self, wb: &mut Book, cmd: &Command, _: &Replaying) -> Result<()> {
        wb.inner.apply(cmd).map(|_| ())
    }

    /// Replays a recalculation.
    pub(super) fn replay_recalc(wb: &mut Book, sheet: Option<u32>, _: &Replaying) {
        match sheet {
            None => {
                wb.inner.recalc_all();
            }
            Some(s) => {
                let _ = wb.inner.recalc_sheet(SheetId(s));
            }
        }
    }

    /// Replays formats, ids, people and lists.
    pub(super) fn replay_styles<'a>(wb: &'a mut Book, _: &Replaying) -> &'a mut StyleTable {
        wb.inner.styles_mut()
    }

    /// Checking builds: the shadow's volatile cells, and what reads them, worked out again (with
    /// the document's random numbers): automatic calculation only.
    #[cfg(any(test, feature = "journal-check"))]
    pub(super) fn replay_refresh(wb: &mut Book, _: &Replaying) {
        wb.inner.recalc();
    }

    /// Replays rule ids.
    pub(super) fn replay_rule_ids(wb: &mut Book, count: u32, _: &Replaying) {
        for _ in 0..count {
            wb.inner.alloc_rule_id();
        }
    }

    /// Replays people.
    pub(super) fn replay_persons(
        wb: &mut Book,
        people: Vec<kiln_sheet::meta::Person>,
        _: &Replaying,
    ) {
        wb.inner.set_persons(people);
    }

    /// Replays custom lists.
    pub(super) fn replay_custom_lists(wb: &mut Book, lists: Vec<Vec<String>>, _: &Replaying) {
        wb.inner.set_custom_lists(lists);
    }
}

#[cfg(test)]
mod tests {
    /// `Ledger::set_view` writes each sheet's selection and the sheet on show into the workbook,
    /// and nothing else: what the journal holds is the same before and after.
    #[test]
    fn set_view_changes_only_the_view() {
        use kiln_sheet::Command;
        use kiln_sheet::meta::Selection;
        let mut wb = kiln_sheet::Workbook::new();
        let s1 = wb.sheets()[0].id();
        let _ = wb.apply(&Command::AddSheet {
            name: Some("Two".into()),
            index: None,
            id: None,
        });
        let _ = wb.apply(&Command::SetCells {
            sheet: s1,
            cells: vec![kiln_sheet::commands::CellInput::formula(0, 0, "=1+1")],
        });
        let mut book = super::Book::open(wb);
        let without_view = |b: &super::Book| {
            let mut snap = b.snapshot();
            for sheet in &mut snap.sheets {
                sheet.meta.view.selection = Default::default();
            }
            snap.props.active_sheet = 0;
            postcard::to_allocvec(&snap).unwrap()
        };
        let before = without_view(&book);
        let selection = Selection {
            ranges: vec![kiln_sheet::Range::new(2, 1, 4, 3)],
            active: kiln_sheet::CellPos::new(2, 1),
        };
        super::Ledger::default().set_view(&mut book, vec![(s1, selection.clone())], Some(1));
        assert_eq!(book.sheets()[0].meta().view.selection, selection);
        assert_eq!(book.props().active_sheet, 1);
        assert_eq!(without_view(&book), before);
    }

    /// The calls that change a workbook, which only this module makes.
    const MUTATORS: &[&str] = &[
        ".apply(",
        ".styles_mut(",
        ".set_persons(",
        ".set_custom_lists(",
        ".alloc_rule_id(",
        ".load_sheet_meta(",
        ".recalc_all(",
        ".recalc_sheet(",
        ".write_record(",
        ".set_theme(",
        ".set_styles(",
        ".passthrough_mut(",
        ".load_style(",
        ".load_name(",
        ".set_max_digit_width(",
        ".keep_stored_results(",
        ".take_stored_results(",
        ".sheet_mut(",
    ];

    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn the_workbook_is_changed_only_here() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/grid");
        let mut files = Vec::new();
        sources(&dir, &mut files);
        let mut found = Vec::new();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            if name == "ledger.rs" || name.contains("tests") {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap();
            for line in joined(&text).lines() {
                // A cell's own value, a style table built aside or a copy are not the workbook.
                let on_workbook =
                    (line.contains("workbook") || line.contains("wb.")) && !line.contains("log.");
                if on_workbook && MUTATORS.iter().any(|m| line.contains(m)) {
                    found.push(format!("{name}: {}", line.trim()));
                }
            }
        }
        assert!(
            found.is_empty(),
            "go through the ledger:\n{}",
            found.join("\n")
        );
        assert!(
            files_named_in(&dir, "mod.rs"),
            "the scan read the Grid sources"
        );
    }

    /// The code without comments, with a call chain broken over lines on one line. The planted
    /// bypasses of `structure.rs` (`// PLANT`, built only to fail to compile) are left out.
    fn joined(text: &str) -> String {
        let code: Vec<&str> = text
            .lines()
            .filter(|l| !l.contains("// PLANT"))
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect();
        let mut out = String::with_capacity(text.len());
        for line in code {
            let t = line.trim_start();
            if !(t.starts_with('.') || t.starts_with('?')) && !out.is_empty() {
                out.push('\n');
            }
            out.push_str(if t.starts_with('.') { t } else { line });
        }
        out
    }

    #[test]
    fn the_scan_sees_a_call_split_over_lines() {
        let text = "        self.workbook\n            .apply(&x)\n            .ok();\n";
        assert!(joined(text).contains("self.workbook.apply(&x).ok();"));
    }

    fn files_named_in(dir: &std::path::Path, name: &str) -> bool {
        dir.join(name).exists()
    }
}

/// Test builds, and builds with the `ownership-escape` feature (a door for tests, never in a
/// shipped build): review 5's bypasses B3 to B6 done at run time, through what the types keep
/// shut, on the Sheet Tab Color command. The stamp guard catches each one: in tests it stops
/// (`storage::tests::crash::guards`), in a release build it keeps the document with a named stop
/// and a checkpoint (`tests/ownership_runtime.rs`).
#[cfg(any(test, feature = "ownership-escape"))]
pub(crate) mod escape {
    use std::cell::Cell;

    use kiln_sheet::commands::MetaField;
    use kiln_sheet::{Command, SheetId};

    use super::super::GridDoc;
    use super::super::storage::{GridOp, Replaying};

    thread_local! {
        static PLANT: Cell<Option<u8>> = const { Cell::new(None) };
    }

    /// Plants bypass `n` (3 to 6) on this thread, or none.
    pub fn set(n: Option<u8>) {
        PLANT.with(|c| c.set(n));
    }

    /// The planted bypass, if one is set: it changes the tab colour round the ledger.
    pub(crate) fn run(doc: &mut GridDoc, sheet: SheetId, color: Option<kiln_sheet::Color>) -> bool {
        let Some(n) = PLANT.with(Cell::get) else {
            return false;
        };
        let cmd = Command::SetSheetMeta {
            sheet,
            field: Box::new(MetaField::TabColor(color)),
        };
        let changed = || {
            let bytes = doc.workbook.to_bytes().ok()?;
            let mut wb =
                kiln_sheet::Workbook::from_bytes(&bytes, kiln_sheet::WorkbookOptions::default())
                    .ok()?;
            wb.apply(&cmd).ok()?;
            Some(wb)
        };
        match n {
            // B3: a document made aside, its workbook moved in.
            3 => {
                if let Some(wb) = changed() {
                    doc.workbook = GridDoc::open_doc(wb, None, Vec::new(), None).workbook;
                }
            }
            // B4: the replay fed a made-up record.
            4 => {
                if let Ok(op) = postcard::to_allocvec(&GridOp::Command(cmd)) {
                    let _ = doc.apply_journal(&op, &Replaying::for_plant());
                }
            }
            // B5: the style table taken, a named style replaced (no record), and the tab colour
            // as a plain change.
            5 => {
                let t = doc.workbook.inner.styles_mut();
                if let Some(mut s) = t.named_styles().first().cloned() {
                    s.builtin_id = None;
                    s.name = format!("{} (planted)", s.name);
                    t.set_named_style(s);
                }
            }
            // B6: the whole workbook swapped for a fresh document's.
            _ => {
                if let Some(wb) = changed() {
                    let mut other = GridDoc::open_doc(wb, None, Vec::new(), None);
                    std::mem::swap(&mut doc.workbook, &mut other.workbook);
                }
            }
        }
        doc.absorb_changes();
        true
    }
}

/// Test builds only: the third review's planted bugs, switched on one test at a time, so each
/// guard is shown to catch them (`storage::tests::guards`).
#[cfg(test)]
pub(crate) mod plant {
    use std::cell::Cell;

    /// A planted bug.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum Plant {
        /// P1: a redo applies its command round the capture and records it bare.
        RedoBareForward,
        /// P2: a copy of cells is applied and not recorded.
        CopyUnrecorded,
        /// The second record a replay reads does not apply (a recovery's own error path).
        ReplayFails,
        /// Review 5's plant: Calculate Now is applied and not recorded.
        RecalcUnrecorded,
    }

    thread_local! {
        static ON: Cell<Option<Plant>> = const { Cell::new(None) };
        static NO_SHADOW: Cell<bool> = const { Cell::new(false) };
        static RELEASE_GUARD: Cell<bool> = const { Cell::new(false) };
        static REPLAYED: Cell<usize> = const { Cell::new(0) };
    }

    /// With [`Plant::ReplayFails`]: whether this record is the one that does not apply.
    pub(crate) fn replay_fails() -> bool {
        on(Plant::ReplayFails)
            && REPLAYED.with(|c| {
                c.set(c.get() + 1);
                c.get() == 2
            })
    }

    pub(crate) fn on(p: Plant) -> bool {
        ON.with(|c| c.get() == Some(p))
    }

    /// Plants `p` (or none) on this thread.
    pub(crate) fn set(p: Option<Plant>) {
        ON.with(|c| c.set(p));
        REPLAYED.with(|c| c.set(0));
    }

    /// Switches the shadow replay check off on this thread, so another guard is shown alone.
    pub(crate) fn set_no_shadow(off: bool) {
        NO_SHADOW.with(|c| c.set(off));
    }

    pub(crate) fn no_shadow() -> bool {
        NO_SHADOW.with(Cell::get)
    }

    /// The stamp guard acts as in a release build (a named stop and a checkpoint) instead of
    /// stopping the test, so that path is tested too.
    pub(crate) fn set_release_guard(on: bool) {
        RELEASE_GUARD.with(|c| c.set(on));
    }

    pub(crate) fn release_guard() -> bool {
        RELEASE_GUARD.with(Cell::get)
    }
}
