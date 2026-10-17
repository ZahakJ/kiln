// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid workbooks: `.xlsx`, `.xlsm` and `.xltx` files read and written by `kiln-io-xlsx`
//! and `.csv` files by `kiln-io-csv` (WP-70), into the `kiln-sheet` model (WP-69), painted and
//! edited through `kiln-sheet-layout` (WP-71).
//!
//! - **Open and save**: the file's values, formulas, formats and metadata; the reader's source is
//!   kept so a save carries every part the model does not cover. `.csv` saves the active sheet
//!   as displayed. The Compatibility Checker says what a format loses.
//! - **Surface** ([`view`]): tiles of cells and headers for every pane of the view (frozen panes
//!   share one tile cache), the selection overlay and the in-cell editor, the accessibility tree
//!   of the visible cells.
//! - **Commands** ([`nav`], [`editor`], [`format`], [`structure`], [`clipboard`]): navigation
//!   and selection for every Grid row of PLAN.md §1.7, in-cell editing with autocomplete of
//!   functions, names and column values, argument hints, point mode and `$` toggling, fill, rows
//!   and columns, sheets, number formats, fonts, fills, borders, alignment, merges, frozen
//!   panes, Paste Special; every change one undo step of the sheet model's command log.
//! - **Charts** ([`charts`]): charts read from the file, inserted, selected, moved, resized,
//!   edited and deleted, with undo; written back on save.
//! - **Queries** ([`state`]): what the chrome shows (`Query::Grid`): sheet tabs, the name box
//!   and formula bar, the mode, the editor, the active cell's format, hit testing, Format Cells.

mod charts;
mod clipboard;
#[cfg(test)]
mod command_tests;
mod editor;
mod format;
mod nav;
mod state;
mod structure;
#[cfg(test)]
mod tests;
mod view;

use kiln_common::{KilnError, Result};
use kiln_io_csv::{ExportOptions, ImportOptions, read_csv, write_delimited};
use kiln_io_xlsx::{WriteOptions, XlsxKind, XlsxSource, read_xlsx, write_xlsx};
use kiln_protocol::commands::{ClipboardData, GridCommand};
use kiln_protocol::{
    A11yTree, CellAddr, CellRange, CellStats, Command, CompatIssue, CompatOrigin, CompatSeverity,
    DocKind, FileFormat, OpenHint, Query, QueryResult, SelKind, Selection, StatusBar, SuiteCommand,
};
use kiln_sheet::meta::Selection as SheetSelection;
use kiln_sheet::{CellValue, CommandLog, Range, SheetId, Workbook};
use kiln_sheet_layout::{GridScene, SheetGeometry};
use rustc_hash::FxHashMap;

use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

pub use editor::EditState;
pub use view::ViewState;

/// The formats Grid opens; the first four it also saves.
const FORMATS: [FileFormat; 4] = [
    FileFormat::Xlsx,
    FileFormat::Xlsm,
    FileFormat::Xltx,
    FileFormat::Csv,
];

/// Creates and opens Grid workbooks.
#[derive(Debug, Clone, Copy, Default)]
pub struct GridApp;

impl AppFactory for GridApp {
    fn kind(&self) -> DocKind {
        DocKind::Grid
    }

    fn opens(&self, format: FileFormat) -> bool {
        FORMATS.contains(&format)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(GridDoc::new(Workbook::new(), None, Vec::new(), None))
    }

    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>> {
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
                Ok(Box::new(GridDoc::new(
                    imp.workbook,
                    None,
                    imp.report.warnings,
                    name,
                )))
            }
            FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xltx => {
                let read = read_xlsx(bytes).map_err(|e| format_error(e.to_string()))?;
                Ok(Box::new(GridDoc::new(
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

/// Mark of a Copy or Cut (the marquee; Cut moves on the next paste).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyMark {
    /// Sheet.
    pub sheet: SheetId,
    /// Range.
    pub range: Range,
    /// Cut.
    pub cut: bool,
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
    pub(crate) workbook: Workbook,
    /// The file read, for a faithful save.
    source: Option<XlsxSource>,
    /// What the reader reported.
    warnings: Vec<String>,
    file_name: Option<String>,
    /// Undo and redo of the sheet model.
    log: CommandLog,
    done: Vec<Step>,
    undone: Vec<Step>,
    state: StateId,
    next_state: StateId,
    /// The view of each sheet (selection, scroll).
    views: FxHashMap<SheetId, ViewState>,
    /// The sheet shown.
    active: usize,
    /// The entry being typed.
    pub(crate) edit: Option<EditState>,
    /// Painting.
    scene: Option<GridScene>,
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

/// A model error as a session error.
pub(crate) fn model_err(e: kiln_sheet::ModelError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

impl GridDoc {
    fn new(
        workbook: Workbook,
        source: Option<XlsxSource>,
        warnings: Vec<String>,
        file_name: Option<String>,
    ) -> Self {
        let active = workbook
            .props()
            .active_sheet
            .min(workbook.sheets().len().saturating_sub(1));
        let mut views = FxHashMap::default();
        for s in workbook.sheets() {
            views.insert(s.id(), ViewState::from_meta(&s.meta().view));
        }
        let charts = charts::Charts::from_source(source.as_ref());
        let mut doc = Self {
            workbook,
            source,
            warnings,
            file_name,
            log: CommandLog::new(),
            done: Vec::new(),
            undone: Vec::new(),
            state: 0,
            next_state: 1,
            views,
            active,
            edit: None,
            scene: None,
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
        doc
    }

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

    /// Applies a model command as one undo step labelled with the Grid command's id.
    pub(crate) fn exec(&mut self, label: &'static str, cmd: kiln_sheet::Command) -> Result<bool> {
        let depth = self.log.undo_depth();
        let redo = self.log.redo_depth();
        self.log
            .exec_labelled(&mut self.workbook, cmd, label)
            .map_err(model_err)?;
        let recorded = self.log.undo_depth() != depth || self.log.redo_depth() != redo;
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
        let depth = self.log.undo_depth();
        let redo = self.log.redo_depth();
        let result = self.log.transaction(&mut self.workbook, label, |log, wb| {
            for c in cmds {
                log.exec_labelled(wb, c, label)?;
            }
            Ok(())
        });
        let recorded = self.log.undo_depth() != depth || self.log.redo_depth() != redo;
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
            // The log keeps at most 1,000 steps; so does this mirror (its oldest model steps go
            // with the log's, chart steps before them too).
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
                self.done.drain(..cut);
            }
            self.undone.clear();
            self.state = state;
        }
        self.absorb_changes();
    }

    /// Records an edit of a chart's content as one undo step.
    pub(crate) fn record_chart_step(&mut self, label: &'static str, edit: charts::ChartEdit) {
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
        let mut touched = false;
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
            let mut meta = s.meta().clone();
            meta.view.selection = SheetSelection {
                ranges: v.sel.ranges.clone(),
                active: v.sel.active,
            };
            let _ = self.workbook.load_sheet_meta(id, meta);
            touched = true;
        }
        if self.workbook.props().active_sheet != self.active {
            let mut props = self.workbook.props().clone();
            props.active_sheet = self.active;
            let _ = self.workbook.apply(&kiln_sheet::Command::SetProps {
                props: Box::new(props),
            });
            touched = true;
        }
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
            },
        )
        .map(|w| w.bytes)
        .map_err(|e| failed(e.to_string()))
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
        // Commands other than the editor's own end the entry first (Excel commits the cell
        // before a ribbon command runs; navigation commits and moves).
        let editing_cmd = matches!(
            cmd,
            G::SetEditText { .. }
                | G::CommitEdit { .. }
                | G::CancelEdit
                | G::NewLineInCell
                | G::ToggleAbsolute
                | G::AcceptCompletion { .. }
                | G::MoveCompletion { .. }
                | G::PointRef { .. }
                | G::PointMove { .. }
                | G::OpenDropdown
                | G::FillSelection
                | G::ArrayFormula
                | G::BeginEdit { .. }
                | G::EditCell
                | G::InsertDateTime { .. }
                | G::AutoSum { .. }
        );
        if !editing_cmd && self.edit.is_some() {
            self.commit_edit(None)?;
        }
        // A chart stays selected for its own commands and the view's; Delete and Esc act on it;
        // anything else goes back to the cells.
        if self.charts.selected.is_some() {
            match cmd {
                G::ClearContents | G::DeleteDrawing => return self.delete_drawing(),
                G::CancelEdit => return self.select_drawing(None),
                G::SelectDrawing { .. }
                | G::PlaceDrawing { .. }
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
            | G::Headings => self.structure(cmd),
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
            Command::Grid(g) => self.grid(g),
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
                | SuiteCommand::ChartFormatAxis { .. }),
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
        if self.view().sel != before_view.sel || self.active != before_sheet {
            self.sync_views_into_meta();
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
            self.restore_chart(&edit.part, edit.before);
            if let Some(step) = self.done.pop() {
                self.undone.push(step);
            }
            self.state = self.done.last().map_or(0, |s| s.state);
            return true;
        }
        match self.log.undo(&mut self.workbook) {
            Ok(true) => {
                if let Some(step) = self.done.pop() {
                    self.undone.push(step);
                }
                self.state = self.done.last().map_or(0, |s| s.state);
                self.absorb_changes();
                true
            }
            _ => false,
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(edit) = self.undone.last().and_then(|s| s.chart.clone()) {
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
        for r in &v.sel.ranges {
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

    fn selected_text(&self) -> Option<String> {
        let sid = self.sid().ok()?;
        let r = *self.view().sel.ranges.last()?;
        self.workbook.copy(sid, r, false).ok().map(|c| c.to_tsv())
    }

    fn app_query(&self, query: &Query) -> Result<QueryResult> {
        match query {
            Query::Grid(q) => Ok(QueryResult::Grid(self.grid_query(q)?)),
            _ => Err(KilnError::Unsupported(format!(
                "{query:?} for a Grid workbook"
            ))),
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
        self.write(format, Some(w3cdtf(now_ms)))
    }

    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let mut out = Vec::new();
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
