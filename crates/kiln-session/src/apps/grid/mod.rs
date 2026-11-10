// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid workbooks: `.xlsx`, `.xlsm` and `.xltx` files read and written by `kiln-io-xlsx`
//! and `.csv` files by `kiln-io-csv`, into the `kiln-sheet` model, painted and
//! edited through `kiln-sheet-layout`.
//!
//! - **Open and save**: the file's values, formulas, formats and metadata; the reader's source is
//!   kept so a save carries every part the model does not cover. `.csv` saves the active sheet
//!   as displayed. The Compatibility Checker says what a format loses.
//! - **Surface** ([`view`]): tiles of cells and headers for every pane of the view (frozen panes
//!   share one tile cache), the selection overlay and the in-cell editor, the accessibility tree
//!   of the visible cells.
//! - **Commands** ([`nav`], [`editor`], [`format`], [`structure`], [`clipboard`]): navigation
//!   and selection for every Grid row, in-cell editing with autocomplete of
//!   functions, names and column values, argument hints, point mode and `$` toggling, fill, rows
//!   and columns, sheets, number formats, fonts, fills, borders, alignment, merges, frozen
//!   panes, Paste Special; every change one undo step of the sheet model's command log.
//! - **Charts** ([`charts`]): charts read from the file, inserted, selected, moved, resized,
//!   edited and deleted, with undo; written back on save.
//! - **Features** (`features`): sort and filter, conditional formatting, validation,
//!   tables, find and replace, notes and comments, links, page setup and printing, names,
//!   auditing, Formula Lab, imports and data tools, outlines, spelling.
//! - **Queries** ([`state`]): what the chrome shows (`Query::Grid`): sheet tabs, the name box
//!   and formula bar, the mode, the editor, the active cell's format, hit testing, Format Cells.

mod analysis;
mod charts;
mod clipboard;
#[cfg(test)]
mod command_tests;
mod editor;
mod features;
mod format;
mod home;
#[cfg(test)]
mod home_tests;
/// native bundles.
pub(crate) mod ledger;
mod nav;
#[cfg(test)]
mod rtl_tests;
mod state;
pub(crate) mod storage;
#[cfg(test)]
pub(crate) mod tests_support {
    //! What the storage tests need of a workbook's journal.
    /// One encoded sample of each kind of operation.
    #[must_use]
    pub(crate) fn samples() -> Vec<Vec<u8>> {
        super::storage::encoded_samples()
    }
}
mod structure;
mod surface;
#[cfg(test)]
mod surface_tests;
#[cfg(test)]
mod tests;
mod view;

use kiln_common::{KilnError, Result};
use kiln_io_csv::{ExportOptions, write_delimited};
use kiln_io_xlsx::{WriteOptions, XlsxKind, XlsxSource, write_xlsx};
use kiln_protocol::commands::{ClipboardData, GridCommand};
use kiln_protocol::{
    A11yTree, CellAddr, CellRange, CellStats, Command, CompatIssue, CompatOrigin, CompatSeverity,
    DocKind, FileFormat, Query, QueryResult, SelKind, Selection, StatusBar, SuiteCommand,
};
use kiln_sheet::meta::Selection as SheetSelection;
use kiln_sheet::{CellValue, Range, SheetId, Workbook};
use kiln_sheet_layout::{GridScene, SheetGeometry};
use rustc_hash::FxHashMap;

use crate::model::{Change, DocModel};
use crate::undo::StateId;

pub use editor::EditState;
pub use view::ViewState;

/// The formats Grid opens and saves; `.ods` only with the `odf` feature (the web core leaves
/// it out).
#[cfg(feature = "odf")]
const FORMATS: [FileFormat; 5] = [
    FileFormat::Xlsx,
    FileFormat::Xlsm,
    FileFormat::Xltx,
    FileFormat::Csv,
    FileFormat::Ods,
];
#[cfg(not(feature = "odf"))]
const FORMATS: [FileFormat; 4] = [
    FileFormat::Xlsx,
    FileFormat::Xlsm,
    FileFormat::Xltx,
    FileFormat::Csv,
];

/// The most cells the selection's text (`SelectedText`) is built for: as many as Copy takes.
const SELECTED_TEXT_CELLS: u64 = 4_000_000;

/// Creates and opens Grid workbooks.
#[derive(Debug, Clone, Copy, Default)]
pub struct GridApp;

// The factory is in `ledger.rs`: only it makes a document (`GridDoc::open_doc`).

/// Mark of a Copy or Cut (the marquee; Cut moves on the next paste).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyMark {
    /// Sheet.
    pub sheet: SheetId,
    /// Range.
    pub range: Range,
    /// Cut.
    pub cut: bool,
    /// The shown rows when a filter hides some of the range: only they were copied, and a paste
    /// lays them one under another.
    pub runs: Option<Vec<Range>>,
}

/// One step of the undo history as the session sees it: a step of the sheet model's command
/// log, or an edit of a chart's content (kept here, before and after).
#[derive(Debug, Clone)]
struct Step {
    state: StateId,
    label: &'static str,
    chart: Option<Box<charts::ChartEdit>>,
}

/// A Grid workbook held by the session.
#[derive(Debug)]
pub struct GridDoc {
    workbook: ledger::Book,
    /// The file read, for a faithful save.
    source: Option<XlsxSource>,
    /// a bundle's original Excel file could not be read.
    pub(crate) original_lost: bool,
    /// What the reader reported.
    warnings: Vec<String>,
    file_name: Option<String>,
    /// Undo and redo of the sheet model.
    log: ledger::Ledger,
    done: Vec<Step>,
    undone: Vec<Step>,
    state: StateId,
    next_state: StateId,
    /// The view of each sheet (selection, scroll).
    views: FxHashMap<SheetId, ViewState>,
    /// The sheet shown.
    active: usize,
    /// What the autosave journal has been told.
    journal: storage::JournalState,
    /// Checking builds: the document as its records give it, and the state last compared.
    #[cfg(any(test, feature = "journal-check"))]
    shadow: Option<Box<GridDoc>>,
    #[cfg(any(test, feature = "journal-check"))]
    shadow_state: StateId,
    /// Checking builds: the clock and random numbers the document shares with its shadow.
    #[cfg(any(test, feature = "journal-check"))]
    check_sources: Option<std::sync::Arc<ledger::CheckSources>>,
    /// Grid's own tests: the journal is taken and checked after each command (no session).
    #[cfg(test)]
    auto_check: bool,
    /// The state undoing everything kept reaches: 0 until old steps have gone.
    undo_floor: StateId,
    /// The entry being typed.
    pub(crate) edit: Option<EditState>,
    /// Painting.
    scene: Option<GridScene>,
    /// Host fonts the scene has (the web registers Arabic, symbol and mathematics fonts when a
    /// sheet needs them).
    fonts_seen: usize,
    /// Geometry of the active sheet (rebuilt when rows, columns or merges change).
    geo: Option<SheetGeometry>,
    /// The view of the last frame.
    pub(crate) last_view: kiln_sheet_layout::View,
    /// Copy/Cut marquee.
    copy: Option<CopyMark>,
    /// Kiln's last copy (Paste Special without the system clipboard).
    clip: Option<kiln_sheet::clipboard::ClipData>,
    /// What Copy/Cut put on the clipboard, for the session to write.
    copied: Option<ClipboardData>,
    /// Fill-handle drag preview.
    fill_preview: Option<Range>,
    /// The last scroll request.
    reveal: Option<kiln_protocol::commands::GridReveal>,
    /// The host's clock.
    now_ms: u64,
    /// The headers changed since the last frame (selection marks).
    selection_epoch: u64,
    /// Where a run of Tab entries started: Enter returns to its column on the next row (Excel).
    pub(crate) tab_origin: Option<kiln_sheet::CellPos>,
    /// The charts of the workbook and the selected one.
    charts: charts::Charts,
    /// view state of the features (arrows, circles, Formula Lab, the Watch Window).
    features: features::FeatureState,
    /// view state of the analysis features (Goal Seek status, Flash Fill suggestions).
    analysis: analysis::AnalysisState,
    /// the border pen, the last fill for Auto Fill Options, deleted number formats.
    home: home::HomeState,
    /// F8 and Shift+F8, a drag of cells.
    surface: surface::SurfaceState,
    /// the document properties set in Kiln (`None`: those of the file read).
    pub(crate) props: Option<crate::props::UserProps>,
}

fn w3cdtf(unix_ms: u64) -> String {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// A model error as a session error; a refusal by protection carries Excel's message
/// (`analysis::PROTECTED`), which the editor shows in a message box.
pub(crate) fn model_err(e: kiln_sheet::ModelError) -> KilnError {
    match e {
        kiln_sheet::ModelError::Protected(m) => KilnError::Unavailable {
            command: analysis::PROTECTED.into(),
            reason: if m.contains("workbook") {
                analysis::STRUCTURE_MESSAGE.into()
            } else {
                analysis::PROTECTED_MESSAGE.into()
            },
        },
        e => KilnError::InvalidInput(e.to_string()),
    }
}

impl GridDoc {
    /// The workbook.
    #[must_use]
    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    /// The reader's diagnostics.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The file name, if the workbook came from a file.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    /// The active sheet's index.
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    fn active_sheet(&self) -> Option<(usize, &kiln_sheet::Sheet)> {
        self.workbook
            .sheets()
            .get(self.active)
            .map(|s| (self.active, s))
            .or_else(|| self.workbook.sheets().first().map(|s| (0, s)))
    }

    /// The active sheet's id.
    pub(crate) fn sid(&self) -> Result<SheetId> {
        self.active_sheet()
            .map(|(_, s)| s.id())
            .ok_or_else(|| KilnError::InvalidInput("the workbook has no sheet".into()))
    }

    /// The view of the active sheet.
    pub(crate) fn view(&self) -> ViewState {
        self.sid()
            .ok()
            .and_then(|id| self.views.get(&id).cloned())
            .unwrap_or_default()
    }

    /// Replaces the view of the active sheet.
    pub(crate) fn set_view(&mut self, v: ViewState) {
        if let Ok(id) = self.sid() {
            if self.views.get(&id).is_none_or(|old| old.sel != v.sel) {
                self.selection_epoch += 1;
            }
            self.views.insert(id, v);
        }
    }

    /// Tests: a change made as a file would have it, through the ledger (no undo step).
    #[cfg(test)]
    pub(crate) fn apply_for_test(&mut self, cmd: &kiln_sheet::Command) -> kiln_sheet::Result<()> {
        let r = self.log.apply_untracked(&mut self.workbook, cmd);
        self.absorb_changes();
        r
    }

    /// Applies a model command as one undo step labelled with the Grid command's id.
    pub(crate) fn exec(&mut self, label: &'static str, cmd: kiln_sheet::Command) -> Result<bool> {
        let pushed = self.log.pushed();
        self.log
            .exec_labelled(&mut self.workbook, cmd, label)
            .map_err(model_err)?;
        let recorded = self.log.pushed() != pushed;
        self.after_change(label, recorded);
        Ok(recorded)
    }

    /// Applies several model commands as one undo step.
    pub(crate) fn exec_all(
        &mut self,
        label: &'static str,
        cmds: Vec<kiln_sheet::Command>,
    ) -> Result<bool> {
        if cmds.is_empty() {
            return Ok(false);
        }
        let pushed = self.log.pushed();
        let result = self.log.transaction(&mut self.workbook, label, |log, wb| {
            for c in cmds {
                log.exec_labelled(wb, c, label)?;
            }
            Ok(())
        });
        let recorded = self.log.pushed() != pushed;
        self.after_change(label, recorded);
        result.map_err(model_err)?;
        Ok(recorded)
    }

    fn after_change(&mut self, label: &'static str, recorded: bool) {
        if recorded {
            let state = self.next_state;
            self.next_state += 1;
            self.done.push(Step {
                state,
                label,
                chart: None,
            });
            // Its oldest model steps go with the log's, chart steps before them too.
            let keep = self.log.undo_depth();
            let model_steps = self.done.iter().filter(|s| s.chart.is_none()).count();
            if model_steps > keep {
                let mut excess = model_steps - keep;
                let cut = self
                    .done
                    .iter()
                    .position(|s| {
                        if s.chart.is_none() {
                            if excess == 0 {
                                return true;
                            }
                            excess -= 1;
                        }
                        false
                    })
                    .unwrap_or(self.done.len());
                if let Some(last) = self.done.drain(..cut).next_back() {
                    self.undo_floor = last.state;
                }
            }
            self.undone.clear();
            self.state = state;
        }
        self.absorb_changes();
    }

    /// Records an edit of a chart's content as one undo step.
    pub(crate) fn record_chart_step(&mut self, label: &'static str, edit: charts::ChartEdit) {
        self.log
            .record(storage::GridOp::chart(&edit.part, &edit.after));
        let state = self.next_state;
        self.next_state += 1;
        self.done.push(Step {
            state,
            label,
            chart: Some(Box::new(edit)),
        });
        if self.done.len() > 2_000 {
            self.done.remove(0);
        }
        self.undone.clear();
        self.state = state;
    }

    /// Takes the model's change list: repaints what changed, rebuilds geometry when rows,
    /// columns, merges or sheets changed, keeps the views valid.
    pub(crate) fn absorb_changes(&mut self) {
        let changes = self.workbook.take_changes();
        if changes.is_empty() {
            return;
        }
        // Charts read the cells: their caches follow every change of values.
        if !changes.cells.is_empty() || !changes.recalculated.is_empty() || changes.sheets {
            self.refresh_charts();
        }
        self.validate_chart_selection();
        let Ok(sid) = self.sid() else {
            return;
        };
        let full = changes.structure.contains(&sid)
            || changes.layout.contains(&sid)
            || changes.meta.contains(&sid)
            || changes.sheets
            || changes.props
            || changes.names;
        if changes.sheets {
            // Sheets added or removed: keep a view for each, and a valid active index.
            let ids: Vec<SheetId> = self
                .workbook
                .sheets()
                .iter()
                .map(kiln_sheet::Sheet::id)
                .collect();
            self.views.retain(|id, _| ids.contains(id));
            for s in self.workbook.sheets() {
                self.views
                    .entry(s.id())
                    .or_insert_with(|| ViewState::from_meta(&s.meta().view));
            }
            self.active = self.active.min(ids.len().saturating_sub(1));
        }
        if full {
            self.geo = None;
            if let Some(s) = &mut self.scene {
                s.invalidate_all();
            }
            self.clamp_selection();
            return;
        }
        let Some(sheet) = self.workbook.sheets().get(self.active) else {
            return;
        };
        let mut areas: Vec<Range> = changes
            .cells
            .iter()
            .filter(|(s, _)| *s == sid)
            .map(|(_, r)| *r)
            .collect();
        for c in &changes.recalculated {
            if c.sheet == sid {
                areas.push(self.workbook.result_area(sid, c.row, c.col));
            }
        }
        // Sparklines repaint with their data: a group whose data changed, all of it
        // (a shared axis moves every member); data on another sheet repaints on any change.
        let other_sheet_changed = changes.cells.iter().any(|(s, _)| *s != sid)
            || changes.recalculated.iter().any(|c| c.sheet != sid);
        let mut spark: Vec<Range> = Vec::new();
        for g in &sheet.meta().sparklines {
            let hit = g.sparklines.iter().any(|s| {
                if s.sheet.is_some() {
                    other_sheet_changed
                } else {
                    areas.iter().any(|a| a.intersects(&s.data))
                }
            });
            if hit {
                spark.extend(g.sparklines.iter().map(|s| Range::cell(s.at.row, s.at.col)));
            }
        }
        areas.extend(spark);
        if areas.is_empty() {
            return;
        }
        if self.geo.is_none() {
            return;
        }
        let geo = self.geo.clone();
        if let (Some(scene), Some(geo)) = (&mut self.scene, geo.as_ref()) {
            if areas.len() > 256 {
                scene.invalidate_all();
            } else {
                for a in areas {
                    scene.invalidate_cells(geo, sheet, a);
                }
            }
        }
    }

    /// Keeps the selection inside the sheet after structural changes.
    fn clamp_selection(&mut self) {
        let mut v = self.view();
        let clamp = |r: Range| {
            Range::new(
                r.r0.min(kiln_sheet::MAX_ROWS - 1),
                r.c0.min(kiln_sheet::MAX_COLS - 1),
                r.r1.min(kiln_sheet::MAX_ROWS - 1),
                r.c1.min(kiln_sheet::MAX_COLS - 1),
            )
        };
        v.sel.ranges = v.sel.ranges.into_iter().map(clamp).collect();
        if v.sel.ranges.is_empty() {
            v.sel.ranges.push(Range::cell(0, 0));
        }
        self.set_view(v);
    }

    /// Stores the views (selection, active sheet) in the model as Excel saves them. Not an
    /// edit: no undo step, and the change notes it makes are dropped (nothing to repaint).
    fn sync_views_into_meta(&mut self) {
        self.absorb_changes();
        let mut metas = Vec::new();
        let ids: Vec<SheetId> = self
            .workbook
            .sheets()
            .iter()
            .map(kiln_sheet::Sheet::id)
            .collect();
        for id in ids {
            let (Some(v), Some(s)) = (self.views.get(&id), self.workbook.sheet(id)) else {
                continue;
            };
            let sel = &s.meta().view.selection;
            if sel.ranges == v.sel.ranges && sel.active == v.sel.active {
                continue;
            }
            metas.push((
                id,
                SheetSelection {
                    ranges: v.sel.ranges.clone(),
                    active: v.sel.active,
                },
            ));
        }
        let props = (self.workbook.props().active_sheet != self.active).then_some(self.active);
        let touched = !metas.is_empty() || props.is_some();
        self.log.set_view(&mut self.workbook, metas, props);
        if touched {
            let _ = self.workbook.take_changes();
        }
    }

    fn write(&self, format: FileFormat, now: Option<String>) -> Result<Vec<u8>> {
        let failed = |e: String| KilnError::Format {
            format: format.extension().to_owned(),
            message: e,
        };
        let wb = &self.workbook;
        let kind = match format {
            FileFormat::Csv => {
                let (_, sheet) = self
                    .active_sheet()
                    .ok_or_else(|| failed("the workbook has no sheet".into()))?;
                return Ok(write_delimited(
                    &self.workbook,
                    sheet.id(),
                    &ExportOptions::default(),
                ));
            }
            #[cfg(feature = "odf")]
            FileFormat::Ods => {
                return kiln_io_odf::write_ods(
                    &self.workbook,
                    &kiln_io_odf::WriteOptions {
                        charts: self.charts.all(),
                        // Pictures of a workbook read from .xlsx are in its package.
                        media: self
                            .workbook
                            .sheets()
                            .iter()
                            .flat_map(|s| s.meta().drawings.iter())
                            .filter(|d| d.kind == kiln_sheet::meta::DrawingKind::Picture)
                            .filter_map(|d| {
                                Some((d.reference.clone(), self.picture_bytes(&d.reference)?))
                            })
                            .collect(),
                        ..kiln_io_odf::WriteOptions::default()
                    },
                )
                .map_err(|e| failed(e.to_string()));
            }
            FileFormat::Xlsx => XlsxKind::Workbook,
            FileFormat::Xlsm => XlsxKind::MacroEnabled,
            FileFormat::Xltx => XlsxKind::Template,
            other => {
                return Err(KilnError::Unsupported(format!(
                    "saving Kiln Grid workbooks as .{}",
                    other.extension()
                )));
            }
        };
        write_xlsx(
            wb,
            self.source.as_ref(),
            &WriteOptions {
                kind: Some(kind),
                now,
                author: None,
                charts: self.charts.to_write(),
                media: self.features.objects.media(),
            },
        )
        .map_err(|e| failed(e.to_string()))
        .and_then(|w| match &self.props {
            Some(p) => p.patch_package(&w.bytes).map_err(failed),
            None => Ok(w.bytes),
        })
    }

    /// the document properties: those set in Kiln, else those of the file read.
    fn user_props(&self) -> crate::props::UserProps {
        self.props.clone().unwrap_or_else(|| {
            self.source
                .as_ref()
                .and_then(|s| kiln_ooxml::props::read_document_properties(s.package()).ok())
                .map(|p| crate::props::UserProps::from_ooxml(&p))
                .unwrap_or_default()
        })
    }

    /// Runs a Grid command.
    fn grid(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        // A Tab run lasts while entries are typed and committed with Tab, Enter or the advance
        // keys; anything else forgets where it started.
        if !matches!(
            cmd,
            G::SetEditText { .. }
                | G::CommitEdit { .. }
                | G::BeginEdit { .. }
                | G::EditCell
                | G::Advance { .. }
                | G::AcceptCompletion { .. }
                | G::MoveCompletion { .. }
        ) {
            self.tab_origin = None;
        }
        // A command that acts on the sheet ends the entry first (Excel commits the cell before a
        // ribbon command runs; navigation commits and moves). The editor's own commands and
        // what only changes how the window looks leave it open.
        if editor::ends_entry(cmd) && self.edit.is_some() {
            self.commit_edit(None)?;
        }
        if let G::PutBackDraft {
            sheet,
            row,
            col,
            text,
        } = cmd
        {
            return self.put_back_draft(*sheet, *row, *col, text);
        }
        // A chart stays selected for its own commands and the view's; Delete and Esc act on it;
        // anything else goes back to the cells.
        if self.charts.selected.is_some() {
            match cmd {
                G::ClearContents | G::DeleteDrawing => return self.delete_drawing(),
                G::CancelEdit => return self.select_drawing(None),
                G::SelectDrawing { .. }
                | G::PlaceDrawing { .. }
                | G::FormatObject { .. }
                | G::ResetPicture { .. }
                | G::ArrangeDrawing { .. }
                | G::Theme { .. }
                | G::Gridlines
                | G::Headings
                | G::ShowFormulas
                | G::CalculateNow
                | G::CalculateSheet => {}
                _ => {
                    self.charts.selected = None;
                    self.selection_epoch += 1;
                }
            }
        }
        match cmd {
            G::SelectDrawing { index } => return self.select_drawing(*index),
            G::PlaceDrawing {
                index,
                x,
                y,
                width,
                height,
            } => {
                return self
                    .place_drawing(*index, kiln_scene::Bounds::new(*x, *y, *width, *height));
            }
            G::DeleteDrawing => return self.delete_drawing(),
            G::Theme { dark } => return Ok(self.set_dark(*dark)),
            _ => {}
        }
        let committing = if matches!(cmd, G::CommitEdit { .. }) {
            self.edit
                .as_ref()
                .map(|e| kiln_sheet::CellPos::new(e.row, e.col))
        } else {
            None
        };
        if let Some(r) = self.home(cmd) {
            return r;
        }
        if let Some(r) = self.surface_command(cmd) {
            return r;
        }
        if let Some(r) = self.analysis(cmd) {
            return r;
        }
        if let Some(r) = self.feature(cmd) {
            return r;
        }
        if let Some(at) = committing {
            let r = self.editing(cmd);
            if r.is_ok() && self.edit.is_none() {
                self.analysis_after_commit(at);
            }
            return r;
        }
        match cmd {
            G::Move { .. }
            | G::Jump { .. }
            | G::Page { .. }
            | G::RowStart { .. }
            | G::FirstCell { .. }
            | G::LastCell { .. }
            | G::Advance { .. }
            | G::SelectCell { .. }
            | G::SelectRange { .. }
            | G::SelectRows { .. }
            | G::SelectColumns { .. }
            | G::SelectAll
            | G::SelectRow
            | G::SelectColumn
            | G::GoTo { .. }
            | G::FindNext { .. } => self.navigate(cmd),
            G::EditCell
            | G::BeginEdit { .. }
            | G::SetEditText { .. }
            | G::CommitEdit { .. }
            | G::FillSelection
            | G::ArrayFormula
            | G::CancelEdit
            | G::NewLineInCell
            | G::ToggleAbsolute
            | G::AcceptCompletion { .. }
            | G::MoveCompletion { .. }
            | G::PointRef { .. }
            | G::PointMove { .. }
            | G::OpenDropdown
            | G::InsertDateTime { .. }
            | G::AutoSum { .. } => self.editing(cmd),
            G::ActivateSheet { .. }
            | G::PreviousSheet
            | G::NextSheet
            | G::InsertSheet
            | G::RenameSheet { .. }
            | G::DeleteSheet { .. }
            | G::MoveSheet { .. }
            | G::DuplicateSheet { .. }
            | G::SheetTabColor { .. }
            | G::HideSheet { .. }
            | G::UnhideSheet { .. }
            | G::InsertCells { .. }
            | G::DeleteCells { .. }
            | G::InsertRows
            | G::InsertColumns
            | G::DeleteRows
            | G::DeleteColumns
            | G::RowHeight { .. }
            | G::ColumnWidth { .. }
            | G::ResizeRows { .. }
            | G::ResizeColumns { .. }
            | G::AutoFitRows { .. }
            | G::AutoFitColumns { .. }
            | G::HideRows
            | G::HideColumns
            | G::UnhideRows
            | G::UnhideColumns
            | G::ClearContents
            | G::Clear { .. }
            | G::FillDown
            | G::FillRight
            | G::FillUp
            | G::FillLeft
            | G::DragFill { .. }
            | G::AutoFill { .. }
            | G::CalculateNow
            | G::CalculateSheet
            | G::ShowFormulas
            | G::FreezePanes
            | G::FreezeTopRow
            | G::FreezeFirstColumn
            | G::Split
            | G::Gridlines
            | G::Headings
            | G::SheetRightToLeft => self.structure(cmd),
            G::PasteSpecial { .. }
            | G::PasteValues { .. }
            | G::PasteFormulas { .. }
            | G::PasteFormatting { .. }
            | G::PasteTranspose { .. } => self.paste_special(cmd),
            _ => self.format(cmd),
        }
    }

    /// The selection as the protocol describes it.
    fn protocol_selection(&self) -> Selection {
        let Some((index, _)) = self.active_sheet() else {
            return Selection::default();
        };
        let v = self.view();
        let sheet_no = u32::try_from(index).unwrap_or(0);
        let addr = |row, col| CellAddr {
            sheet: sheet_no,
            row,
            col,
        };
        Selection {
            kind: SelKind::Cells {
                ranges: v
                    .sel
                    .ranges
                    .iter()
                    .map(|r| CellRange {
                        start: addr(r.r0, r.c0),
                        end: addr(r.r1, r.c1),
                    })
                    .collect(),
                active: addr(v.sel.active.row, v.sel.active.col),
            },
            ..Selection::default()
        }
    }
}

fn issue(
    code: &str,
    severity: CompatSeverity,
    message: String,
    count: u32,
    origin: CompatOrigin,
) -> CompatIssue {
    CompatIssue {
        code: code.to_owned(),
        severity,
        message,
        count,
        origin,
    }
}

impl DocModel for GridDoc {
    fn to_bundle(&self) -> Result<kiln_storage::Bundle> {
        GridDoc::to_bundle(self)
    }

    fn properties(&self) -> Option<kiln_protocol::commands::DocPropertiesInfo> {
        let mut info = self.user_props().info();
        info.sheets = u32::try_from(self.workbook.sheets().len()).unwrap_or(u32::MAX);
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
        self.log.record(storage::GridOp::Properties(p.clone()));
        self.props = Some(p);
        // Office keeps no undo step for properties; the workbook has unsaved changes.
        self.state = self.next_state;
        self.next_state += 1;
        Ok(true)
    }

    fn inspect(&self) -> Vec<kiln_protocol::InspectItem> {
        use kiln_protocol::{InspectItem, InspectKind};
        let sheets = self.workbook.sheets();
        let comments: usize = sheets
            .iter()
            .map(|s| s.meta().notes.len() + s.meta().threads.len())
            .sum();
        let hidden = sheets
            .iter()
            .filter(|s| s.meta().visibility != kiln_sheet::meta::SheetVisibility::Visible)
            .count();
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        vec![
            InspectItem {
                kind: InspectKind::Comments,
                count: count(comments),
            },
            InspectItem {
                kind: InspectKind::HiddenSheets,
                count: count(hidden),
            },
            InspectItem {
                kind: InspectKind::Properties,
                count: crate::props::personal_count(&self.user_props().info()),
            },
        ]
    }

    fn inspect_remove(&mut self, item: kiln_protocol::InspectKind) -> Result<bool> {
        use kiln_protocol::InspectKind;
        use kiln_sheet::commands::MetaField;
        const LABEL: &str = "suite.file.inspect_remove";
        match item {
            InspectKind::Comments => {
                let mut cmds = Vec::new();
                for s in self.workbook.sheets() {
                    if !s.meta().notes.is_empty() {
                        cmds.push(kiln_sheet::Command::SetSheetMeta {
                            sheet: s.id(),
                            field: Box::new(MetaField::Notes(Vec::new())),
                        });
                    }
                    if !s.meta().threads.is_empty() {
                        cmds.push(kiln_sheet::Command::SetSheetMeta {
                            sheet: s.id(),
                            field: Box::new(MetaField::Threads(Vec::new())),
                        });
                    }
                }
                self.exec_all(LABEL, cmds)
            }
            InspectKind::HiddenSheets => {
                let active = self.sid()?;
                let cmds: Vec<kiln_sheet::Command> = self
                    .workbook
                    .sheets()
                    .iter()
                    .filter(|s| {
                        s.meta().visibility != kiln_sheet::meta::SheetVisibility::Visible
                            && s.id() != active
                    })
                    .map(|s| kiln_sheet::Command::RemoveSheet { sheet: s.id() })
                    .collect();
                let removed = self.exec_all(LABEL, cmds)?;
                if let Some(i) = self.workbook.sheets().iter().position(|s| s.id() == active) {
                    self.active = i;
                }
                Ok(removed)
            }
            InspectKind::Properties => self.set_properties(&crate::props::clear_personal()),
            other => Err(KilnError::Unsupported(format!(
                "Kiln Grid workbooks have no {other:?}"
            ))),
        }
    }

    fn bundles(&self) -> bool {
        true
    }

    fn set_journaling(&mut self, on: bool) {
        #[cfg(test)]
        {
            self.auto_check = false;
        }
        self.start_journal(on);
    }

    fn set_journal_format(&mut self, format: u32) {
        self.journal.set_format(format);
    }

    fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        GridDoc::take_journal(self)
    }

    fn apply_journal(&mut self, op: &[u8], replay: &crate::JournalReplay) -> Result<()> {
        storage::replay(self, op, replay)
    }

    fn take_draft(&mut self) -> Option<Option<Vec<u8>>> {
        GridDoc::take_draft(self)
    }

    fn describe_draft(&self, draft: &[u8]) -> Option<kiln_protocol::RecoveredDraft> {
        GridDoc::describe_draft(self, draft)
    }

    fn kind(&self) -> DocKind {
        DocKind::Grid
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        let before_state = self.state;
        let before_view = self.view();
        let before_sheet = self.active;
        let before_edit = self.edit.clone();
        let before_epoch = self.selection_epoch;
        let result = match command {
            Command::Grid(g) => {
                let r = self.grid(g);
                self.protection_refusal(r)
            }
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
            ) => {
                if self.edit.is_some() {
                    self.commit_edit(None)?;
                }
                self.chart_command(c)
            }
            other => Err(KilnError::Unsupported(format!(
                "{} in Kiln Grid",
                other.id()
            ))),
        };
        self.absorb_changes();
        self.features_after();
        if self.view().sel != before_view.sel || self.active != before_sheet {
            self.sync_views_into_meta();
        }
        // Grid's own tests: the command's records, replayed onto the shadow and compared.
        #[cfg(test)]
        if self.auto_check {
            let _ = self.take_journal();
            let _ = self.take_draft();
        }
        let change = result?;
        if self.state != before_state || change == Change::Content {
            return Ok(Change::Content);
        }
        if change == Change::Selection
            || self.view() != before_view
            || self.active != before_sheet
            || self.edit != before_edit
            || self.selection_epoch != before_epoch
        {
            return Ok(Change::Selection);
        }
        Ok(Change::None)
    }

    fn undo(&mut self) -> bool {
        if self.edit.is_some() {
            // Undo while typing discards the entry, as Esc does.
            self.edit = None;
            return true;
        }
        if let Some(edit) = self.done.last().and_then(|s| s.chart.clone()) {
            // A chart edit: the chart as it was.
            self.log
                .record(storage::GridOp::chart(&edit.part, &edit.before));
            self.restore_chart(&edit.part, edit.before);
            if let Some(step) = self.done.pop() {
                self.undone.push(step);
            }
            self.state = self.done.last().map_or(self.undo_floor, |s| s.state);
            return true;
        }
        match self.log.undo(&mut self.workbook) {
            Ok(true) => {
                if let Some(step) = self.done.pop() {
                    self.undone.push(step);
                }
                self.state = self.done.last().map_or(self.undo_floor, |s| s.state);
                self.absorb_changes();
                self.features_after();
                true
            }
            _ => false,
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(edit) = self.undone.last().and_then(|s| s.chart.clone()) {
            self.log
                .record(storage::GridOp::chart(&edit.part, &edit.after));
            self.restore_chart(&edit.part, edit.after);
            if let Some(step) = self.undone.pop() {
                self.state = step.state;
                self.done.push(step);
            }
            return true;
        }
        if self.undone.is_empty() {
            return false;
        }
        match self.log.redo(&mut self.workbook) {
            Ok(true) => {
                if let Some(step) = self.undone.pop() {
                    self.state = step.state;
                    self.done.push(step);
                }
                self.absorb_changes();
                self.features_after();
                true
            }
            _ => false,
        }
    }

    fn can_undo(&self) -> bool {
        self.edit.is_some()
            || self.done.last().is_some_and(|s| s.chart.is_some())
            || self.log.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.edit.is_none()
            && (self.undone.last().is_some_and(|s| s.chart.is_some())
                || (!self.undone.is_empty() && self.log.can_redo()))
    }

    fn undo_label(&self) -> Option<&'static str> {
        if self.edit.is_some() {
            return Some("grid.editing.cancel_edit");
        }
        self.done.last().map(|s| s.label)
    }

    fn redo_label(&self) -> Option<&'static str> {
        self.undone.last().map(|s| s.label)
    }

    fn state(&self) -> StateId {
        self.state
    }

    fn seal_history(&mut self) {}

    fn selection(&self) -> Selection {
        self.protocol_selection()
    }

    fn status(&self) -> StatusBar {
        let Some((_, sheet)) = self.active_sheet() else {
            return StatusBar::default();
        };
        let id = sheet.id();
        let mut stats = CellStats {
            count: 0,
            numeric_count: 0,
            sum: 0.0,
            average: None,
            min: None,
            max: None,
        };
        let v = self.view();
        let mut seen = rustc_hash::FxHashSet::default();
        // Rows a filter hides do not count (Excel's status bar sums what is shown).
        for r in &self.visible_ranges(&v.sel.ranges) {
            // Small ranges are read cell by cell (spilled results count); large ones by the
            // stored cells (an entire column is no slower than its cells).
            let mut visit = |row: u32, col: u32| {
                if !seen.insert((row, col)) {
                    return;
                }
                let val = self.workbook.value(id, row, col);
                if val.is_blank() {
                    return;
                }
                stats.count += 1;
                if let CellValue::Number(n) = val {
                    stats.numeric_count += 1;
                    stats.sum += n;
                    stats.min = Some(stats.min.map_or(n, |m: f64| m.min(n)));
                    stats.max = Some(stats.max.map_or(n, |m: f64| m.max(n)));
                }
            };
            if r.cell_count() <= 4_096 {
                for p in r.cells() {
                    visit(p.row, p.col);
                }
            } else {
                for (row, col, _) in sheet.cells().iter_range(*r) {
                    visit(row, col);
                }
            }
        }
        if stats.numeric_count > 0 {
            stats.average = Some(stats.sum / f64::from(stats.numeric_count));
        }
        StatusBar {
            cells: Some(stats),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<kiln_protocol::ParagraphText>> {
        None
    }

    fn a11y(&self, title: &str) -> A11yTree {
        self.a11y_tree(title)
    }

    fn a11y_document(&self, title: &str) -> A11yTree {
        self.a11y_document_tree(title)
    }

    fn selected_text(&self) -> Option<String> {
        let sid = self.sid().ok()?;
        let r = *self.view().sel.ranges.last()?;
        // Whole rows and columns (the whole sheet: 1,048,576 × 16,384 cells) give the text of
        // their used part, as Copy does; the tab-separated text has a cell for every position.
        let r = if r.is_whole_rows() || r.is_whole_cols() {
            let used = self
                .workbook
                .sheet(sid)
                .and_then(|s| s.cells().used_range(false))
                .unwrap_or_else(|| Range::cell(0, 0));
            Range::new(
                r.r0,
                r.c0,
                r.r1.min(used.r1.max(r.r0)),
                r.c1.min(used.c1.max(r.c0)),
            )
        } else {
            r
        };
        if r.cell_count() > SELECTED_TEXT_CELLS {
            return None;
        }
        self.workbook.copy(sid, r, false).ok().map(|c| c.to_tsv())
    }

    fn app_query(&self, query: &Query) -> Result<QueryResult> {
        match query {
            Query::Grid(q) => Ok(QueryResult::Grid(self.grid_query(q)?)),
            _ => Err(KilnError::Unsupported(
                "the query for a Grid workbook".into(),
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

    fn print(&self, job: &crate::print::PrintJob<'_>) -> Result<crate::print::PrintOutput> {
        self.print_job(job)
    }

    fn prepare_print(&mut self) {
        GridDoc::prepare_print(self);
    }

    fn prepare_query(&mut self, query: &Query) {
        match query {
            Query::Grid(kiln_protocol::commands::GridQuery::Evaluate { formula }) => {
                self.prepare_evaluate(formula);
            }
            Query::Grid(kiln_protocol::commands::GridQuery::FilterMenu { row, col }) => {
                self.prepare_filter_menu(*row, *col);
            }
            _ => {}
        }
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        self.write(format, None)
    }

    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        self.write(format, Some(w3cdtf(now_ms)))
    }

    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let mut out = Vec::new();
        if self.original_lost
            && matches!(
                format,
                FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xltx
            )
        {
            out.push(crate::storage::original_lost("Excel"));
        }
        let wb = &self.workbook;
        match format {
            FileFormat::Csv => {
                let others = wb.sheets().len().saturating_sub(1);
                if others > 0 {
                    out.push(issue(
                        "csv.sheets",
                        CompatSeverity::Loss,
                        "Only the active sheet is saved; the other sheets are not".into(),
                        u32::try_from(others).unwrap_or(u32::MAX),
                        CompatOrigin::Save,
                    ));
                }
                let formulas: usize = wb
                    .sheets()
                    .iter()
                    .map(|s| {
                        s.cells()
                            .iter()
                            .filter(|(_, _, d)| d.kind.is_formula())
                            .count()
                    })
                    .sum();
                if formulas > 0 {
                    out.push(issue(
                        "csv.formulas",
                        CompatSeverity::Loss,
                        "Formulas are saved as their results".into(),
                        u32::try_from(formulas).unwrap_or(u32::MAX),
                        CompatOrigin::Save,
                    ));
                }
                out.push(issue(
                    "csv.formatting",
                    CompatSeverity::Loss,
                    "Formatting, merged cells, notes and everything but the displayed values are not saved"
                        .into(),
                    1,
                    CompatOrigin::Save,
                ));
            }
            #[cfg(feature = "odf")]
            FileFormat::Ods => {
                // What the ODF writer reports it cannot keep (charts of other types, shapes, …).
                let options = kiln_io_odf::WriteOptions {
                    charts: self.charts.all(),
                    ..kiln_io_odf::WriteOptions::default()
                };
                for m in kiln_io_odf::ods_issues(wb, &options) {
                    out.push(issue(
                        "odf.loss",
                        CompatSeverity::Loss,
                        m,
                        1,
                        CompatOrigin::Save,
                    ));
                }
            }
            FileFormat::Xlsx | FileFormat::Xltx
                if self.source.as_ref().is_some_and(|s| s.kind().has_macros()) =>
            {
                out.push(issue(
                    "macros.removed",
                    CompatSeverity::Loss,
                    "Macros are not kept in this file type (save as .xlsm to keep them)".into(),
                    1,
                    CompatOrigin::Save,
                ));
            }
            _ => {}
        }
        if !self.warnings.is_empty() {
            out.push(issue(
                "open.warnings",
                CompatSeverity::Info,
                format!(
                    "Opening the file reported {} problem(s), such as: {}",
                    self.warnings.len(),
                    self.warnings[0]
                ),
                u32::try_from(self.warnings.len()).unwrap_or(u32::MAX),
                CompatOrigin::Open,
            ));
        }
        out
    }
}
