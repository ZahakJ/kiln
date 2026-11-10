// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid's features, on top of the sheet model's commands:
//!
//! - [`sort_filter`]: Sort A→Z/Z→A, the Sort dialog, AutoFilter dropdowns (value lists, text,
//!   number, date, colour and top 10 criteria, sort from the dropdown), Clear and Reapply.
//! - [`cf`]: conditional formatting rules (new, manage, clear) as the dialogs describe them.
//! - [`validation`]: Data Validation and Circle Invalid Data.
//! - [`tables`]: Format as Table, Table Design (styles, options, totals, name, resize, convert),
//!   Cell Styles and Format Painter.
//! - [`find`]: Replace, Replace All, Find All and Go To Special.
//! - [`comments`]: notes, threaded comments and hyperlinks.
//! - [`page`]: page setup, print area and titles, page breaks, Page Break Preview and printing.
//! - [`names`]: the Name Manager, Define Name and Create from Selection.
//! - [`audit`]: trace arrows, error checking, Formula Lab and Evaluate Formula, the Watch
//!   Window and calculation options.
//! - [`objects`]: pictures and shapes on sheets (Insert ▸ Pictures and Shapes, Picture Format,
//!   Shape Format).
//! - [`data`]: From Text/CSV and From JSON, Refresh All, Text to Columns, Remove Duplicates,
//!   outlines and spelling.
//!
//! Every change to the workbook is one undo step of the sheet model's command log, labelled with
//! the command's id; view-only state (arrows, circles, the Watch Window) lives here.

pub(crate) mod audit;
pub(crate) mod cf;
pub(crate) mod comments;
mod data;
mod find;
mod names;
pub(crate) mod objects;
pub(crate) mod page;
mod sort_filter;
mod tables;
#[cfg(test)]
mod tests;
pub(crate) mod validation;

use std::cell::RefCell;

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_features::{GridFeatureState, GridFormulaTrace};
use kiln_protocol::commands::{GridCommand, GridHit, GridQuery, GridQueryResult};
use kiln_scene::DrawOp;
use kiln_sheet::{CellPos, Range, SheetId};
use kiln_sheet_layout::SheetGeometry;

use super::GridDoc;
use crate::model::Change;
use crate::undo::StateId;

/// A trace arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Arrow {
    /// The precedent range (on this sheet) or the cell a dependent formula is in.
    pub from: Range,
    /// The formula cell (precedents) or the cell depended on.
    pub to: CellPos,
    /// The sheet.
    pub sheet: SheetId,
    /// Dependents (otherwise precedents).
    pub dependent: bool,
    /// The other end is on another sheet (a dashed arrow to a sheet icon).
    pub external: bool,
}

/// The pages of the active sheet, computed when a view or dialog needs them.
#[derive(Debug, Clone)]
pub(crate) struct PagesCache {
    pub state: StateId,
    pub sheet: SheetId,
    pub pages: kiln_sheet_layout::print::SheetPages,
}

/// View-only state of the features.
#[derive(Debug, Default)]
pub(crate) struct FeatureState {
    /// Trace arrows.
    pub arrows: Vec<Arrow>,
    /// The cells whose precedents the next Trace Precedents follows (one level further).
    pub prec_frontier: Vec<(SheetId, CellPos)>,
    /// The cells whose dependents the next Trace Dependents follows.
    pub dep_frontier: Vec<(SheetId, CellPos)>,
    /// Circle Invalid Data.
    pub circles: bool,
    /// The cells circled.
    pub invalid: Vec<CellPos>,
    /// Formula Lab is open.
    pub lab: bool,
    /// The last trace (Formula Lab, Evaluate Formula).
    pub trace: Option<GridFormulaTrace>,
    /// Watch Window cells.
    pub watches: Vec<(SheetId, u32, u32)>,
    /// The result of the last data command.
    pub message: Option<String>,
    /// Outline symbols hidden.
    pub hide_outline: bool,
    /// The view mode of the last frame (Page Break Preview draws the pages).
    pub view_mode: Option<kiln_protocol::ViewMode>,
    /// Pages of the active sheet.
    pub pages: RefCell<Option<PagesCache>>,
    /// Conditional formats of the printed cells (state, sheet index, one map per sheet).
    #[allow(clippy::type_complexity)]
    pub print_cf: RefCell<
        Option<(
            StateId,
            usize,
            Vec<rustc_hash::FxHashMap<CellPos, kiln_sheet::cf::CellVisual>>,
        )>,
    >,
    /// Spelling.
    pub speller: Option<kiln_spell::Speller>,
    /// Show All Notes.
    pub all_notes: bool,
    /// Where Spelling continues from (sheet index, row, column, byte offset).
    pub spell_from: Option<(usize, u32, u32, u32)>,
    /// Pictures and shapes.
    pub objects: objects::Objects,
    /// The last formula evaluated for Function Arguments, and its value.
    pub evaluated: Option<(String, String)>,
    /// The validation alert of the last refused entry.
    pub alert: Option<(
        kiln_protocol::commands::grid_features::GridErrorStyle,
        String,
        String,
    )>,
    /// The next commit keeps an entry a Warning or Information rule refuses.
    pub accept_invalid: bool,
    /// For the AutoFilter menu of (row, col): which data rows the other columns' filters let
    /// through (its list shows only their values, as Excel's does).
    pub filter_others: Option<(u32, u32, Vec<bool>)>,
}

// The cache is filled from `&self` queries; the document stays on one thread at a time.
// (`RefCell` is `Send`.)

impl GridDoc {
    /// Runs a command, or `None` when the command is not one of them.
    pub(crate) fn feature(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        use GridCommand as G;
        if let Some(r) = self.object_command(cmd) {
            return Some(r);
        }
        // An alert lasts until the next command; Yes on it keeps the refused entry.
        self.features.alert = None;
        if matches!(cmd, G::AcceptInvalid) {
            self.features.accept_invalid = true;
            return Some(Ok(Change::Selection));
        }
        // Yes holds for the commit that follows it only.
        if !matches!(cmd, G::CommitEdit { .. }) {
            self.features.accept_invalid = false;
        }
        self.features.message = None;
        Some(match cmd {
            G::SortAscending
            | G::SortDescending
            | G::ApplySort { .. }
            | G::ToggleFilter
            | G::FilterColumn { .. }
            | G::SortFilterColumn { .. }
            | G::ClearFilter
            | G::ReapplyFilter
            | G::FilterBySelection { .. }
            | G::SortBySelectionColor { .. } => self.sort_filter(cmd),
            G::AddConditionalFormat { .. }
            | G::SetConditionalFormats { .. }
            | G::ClearConditionalFormats { .. } => self.cf_command(cmd),
            G::SetValidation { .. } | G::CircleInvalid { .. } => self.validation_command(cmd),
            G::CreateTable { .. }
            | G::TableStyle { .. }
            | G::TableOptions { .. }
            | G::TableTotals { .. }
            | G::RenameTable { .. }
            | G::ResizeTable { .. }
            | G::ConvertToRange
            | G::ApplyCellStyle { .. }
            | G::PaintFormat { .. } => self.table_command(cmd),
            G::Replace { .. } | G::SelectSpecial { .. } => self.find_command(cmd),
            G::SetNote { .. }
            | G::ShowNote { .. }
            | G::ShowAllNotes { .. }
            | G::AddComment { .. }
            | G::EditComment { .. }
            | G::DeleteComment { .. }
            | G::ResolveComment { .. }
            | G::NextComment { .. }
            | G::ConvertNotes { .. }
            | G::SetLink { .. } => self.comment_command(cmd),
            G::PageSetup { .. }
            | G::PrintArea { .. }
            | G::PageBreak { .. }
            | G::MovePageBreak { .. }
            | G::PrintGridlines
            | G::PrintHeadings => self.page_command(cmd),
            G::DefineName { .. } | G::DeleteName { .. } | G::CreateNames { .. } => {
                self.names_command(cmd)
            }
            G::TracePrecedents
            | G::TraceDependents
            | G::RemoveArrows { .. }
            | G::CalculationMode { .. }
            | G::IterativeCalculation { .. }
            | G::DateSystem { .. }
            | G::FormulaLab { .. }
            | G::Watch { .. } => self.audit_command(cmd),
            G::Import { .. }
            | G::RefreshAll
            | G::TextToColumns { .. }
            | G::RemoveDuplicates { .. }
            | G::Group { .. }
            | G::Ungroup { .. }
            | G::OutlineDetail { .. }
            | G::OutlineSymbols
            | G::SpellChange { .. }
            | G::SpellIgnore { .. }
            | G::LoadDictionary { .. } => self.data_command(cmd),
            _ => return None,
        })
    }

    /// Answers a query.
    pub(crate) fn feature_query(&self, q: &GridQuery) -> Result<GridQueryResult> {
        match q {
            GridQuery::FilterMenu { row, col } => Ok(GridQueryResult::FilterMenu(Box::new(
                self.filter_menu(*row, *col)?,
            ))),
            GridQuery::ConditionalFormats { sheet } => {
                Ok(GridQueryResult::Rules(self.cf_rules(*sheet)?))
            }
            GridQuery::Validation => Ok(GridQueryResult::Validation(self.validation_info()?)),
            GridQuery::TableStyles => Ok(GridQueryResult::TableStyles(self.table_styles())),
            GridQuery::CellStyles => Ok(GridQueryResult::CellStyles(tables::cell_styles())),
            GridQuery::FindAll { spec } => Ok(GridQueryResult::Found(self.find_all(spec)?)),
            GridQuery::Comments => Ok(GridQueryResult::Comments(self.comments())),
            GridQuery::PageSetup => Ok(GridQueryResult::PageSetup(Box::new(self.page_info()?))),
            GridQuery::Names => Ok(GridQueryResult::Names(self.names_info())),
            GridQuery::FormulaTrace => Ok(GridQueryResult::Trace(Box::new(
                self.features
                    .trace
                    .clone()
                    .unwrap_or_else(|| GridFormulaTrace {
                        cell: String::new(),
                        formula: String::new(),
                        value: String::new(),
                        root: None,
                        steps: Vec::new(),
                        message: Some(
                            "Open Formula Lab (Formulas ▸ Formula Lab) to follow the active cell."
                                .into(),
                        ),
                    }),
            ))),
            GridQuery::Errors => Ok(GridQueryResult::Errors(self.error_cells()?)),
            GridQuery::ImportPreview { import } => Ok(GridQueryResult::ImportPreview(Box::new(
                data::import_preview(import)?,
            ))),
            GridQuery::Spelling => Ok(GridQueryResult::Spelling(self.next_misspelling())),
            GridQuery::DataRange => Ok(GridQueryResult::DataRange(Box::new(
                self.data_range_info()?,
            ))),
            GridQuery::Export { format, selection } => Ok(GridQueryResult::Export(Box::new(
                self.export(*format, *selection)?,
            ))),
            GridQuery::FunctionDoc { name } => {
                let d = kiln_formula::functions::doc(name).ok_or_else(|| {
                    KilnError::InvalidInput(format!("there is no function “{name}”"))
                })?;
                Ok(GridQueryResult::FunctionDoc(Box::new(
                    kiln_protocol::commands::grid_features::GridFunctionDoc {
                        name: d.name.clone(),
                        category: d.category_name.clone(),
                        syntax: d.syntax.clone(),
                        summary: d.summary.clone(),
                        args: d
                            .args
                            .iter()
                            .map(
                                |a| kiln_protocol::commands::grid_features::GridFunctionArg {
                                    name: a.name.clone(),
                                    description: a.description.clone(),
                                    optional: a.optional,
                                    repeat: a.repeating,
                                },
                            )
                            .collect(),
                    },
                )))
            }
            GridQuery::Evaluate { formula } => {
                Ok(GridQueryResult::Evaluated(match &self.features.evaluated {
                    Some((f, v)) if f == formula => v.clone(),
                    _ => String::new(),
                }))
            }
            _ => Err(KilnError::Unsupported("the query in Kiln Grid".into())),
        }
    }

    /// The features part of the chrome's state.
    pub(crate) fn feature_state(&self) -> GridFeatureState {
        let mut s = GridFeatureState {
            message: self.features.message.clone(),
            arrows: !self.features.arrows.is_empty(),
            circles: self.features.circles,
            lab: self.features.lab,
            outline_symbols: !self.features.hide_outline,
            calc: Some(audit::calc_mode(self.workbook.calc_mode())),
            calc_pending: self.workbook.calc_mode() == kiln_sheet::meta::CalcMode::Manual
                && self.workbook.is_calc_pending(),
            iterative: {
                let c = &self.workbook.props().calc;
                (c.iterate, c.iterate_count, c.iterate_delta)
            },
            circular: self.circular_refs(),
            date1904: self.workbook.props().date1904,
            alert: self.features.alert.clone(),
            ..GridFeatureState::default()
        };
        let Ok(sid) = self.sid() else {
            return s;
        };
        let a = self.view().sel.active;
        s.cell_rect = self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let r = geo.display_rect(a.row, a.col);
            let r = geo.surface_rect(&self.last_view, r, hw);
            [r.x, r.y, r.w, r.h]
        });
        if let Some(sheet) = self.workbook.sheet(sid) {
            let meta = sheet.meta();
            s.filter = meta.auto_filter.is_some();
            s.filtered = meta
                .auto_filter
                .as_ref()
                .is_some_and(|f| f.columns.iter().any(|c| c.filter.is_some()))
                || sheet.tables().iter().any(|t| {
                    t.auto_filter
                        .as_ref()
                        .is_some_and(|f| f.columns.iter().any(|c| c.filter.is_some()))
                });
            s.table = self.table_info(sid, a);
            s.link = comments::link_at(sheet, a);
            s.note = sheet.note_at(a.row, a.col).is_some();
            s.comment = !sheet.threads_at(a.row, a.col).is_empty();
            s.print_gridlines = meta.print.print_gridlines;
            s.print_headings = meta.print.print_headings;
            s.landscape = meta.print.orientation == kiln_sheet::meta::Orientation::Landscape;
            s.dropdown = sheet.validation_at(a.row, a.col).is_some_and(|v| {
                v.kind == kiln_sheet::meta::ValidationType::List && v.in_cell_dropdown
            });
            s.prompt = sheet
                .validation_at(a.row, a.col)
                .filter(|v| {
                    v.show_input_message && (v.prompt.is_some() || v.prompt_title.is_some())
                })
                .map(|v| {
                    [
                        v.prompt_title.clone().unwrap_or_default(),
                        v.prompt.clone().unwrap_or_default(),
                    ]
                });
            s.print_area =
                self.workbook.names().iter().any(|n| {
                    n.scope == Some(sid) && n.name.eq_ignore_ascii_case("_xlnm.Print_Area")
                });
        }
        s.any_comments = self
            .workbook
            .sheets()
            .iter()
            .any(|sh| !sh.meta().notes.is_empty() || !sh.meta().threads.is_empty());
        s.cards = self.note_cards();
        s.watches = self.watch_rows();
        if matches!(
            self.features.view_mode,
            Some(kiln_protocol::ViewMode::PageBreakPreview | kiln_protocol::ViewMode::PageLayout)
        ) {
            s.pages = self
                .pages()
                .map_or(0, |p| u32::try_from(p.pages.len()).unwrap_or(0));
        }
        s
    }

    /// The overlay of the features (sheet pixels): AutoFilter buttons, the in-cell dropdown
    /// button, trace arrows, invalid-data circles, Page Break Preview's pages.
    pub(crate) fn feature_overlay(&self, geo: &SheetGeometry) -> Vec<DrawOp> {
        let mut out = Vec::new();
        self.filter_buttons_ops(geo, &mut out);
        self.dropdown_button_ops(geo, &mut out);
        // the error indicator's button and a drag of cells.
        self.error_button_ops(geo, &mut out);
        self.drag_preview_ops(geo, &mut out);
        self.circle_ops(geo, &mut out);
        self.arrow_ops(geo, &mut out);
        if self.features.view_mode == Some(kiln_protocol::ViewMode::PageBreakPreview) {
            self.page_break_ops(geo, &mut out);
        }
        // PivotTable field buttons and empty reports.
        self.analysis_overlay(geo, &mut out);
        out
    }

    /// What a point hits among the features' buttons.
    pub(crate) fn feature_hit(&self, x: f32, y: f32) -> Option<GridHit> {
        self.filter_button_hit(x, y)
            .or_else(|| self.analysis_hit(x, y))
            .or_else(|| self.dropdown_hit(x, y))
            .or_else(|| self.error_button_hit(x, y))
            .or_else(|| self.page_break_hit(x, y))
    }

    /// After every command, undo and redo: Formula Lab follows the active cell.
    pub(crate) fn features_after(&mut self) {
        if self.features.lab {
            let t = self.trace_now();
            self.features.trace = Some(t);
        }
        if self.features.circles {
            self.refresh_circles();
        }
        // Arrows of a sheet that no longer exists go.
        let ids: Vec<SheetId> = self
            .workbook
            .sheets()
            .iter()
            .map(kiln_sheet::Sheet::id)
            .collect();
        self.features.arrows.retain(|a| ids.contains(&a.sheet));
        self.features.watches.retain(|w| ids.contains(&w.0));
    }

    /// The range a data command works on: the selection when it is several cells, else the
    /// table or filter range at the active cell, else its current region.
    pub(crate) fn data_range(&self) -> Result<Range> {
        let v = self.view();
        let sel = v.sel.ranges.last().copied().unwrap_or(Range::cell(0, 0));
        if !sel.is_single_cell() {
            let geo = self.geometry_ref();
            if geo.merge_at(sel.r0, sel.c0) != Some(sel) {
                return Ok(self.clip_to_data(sel));
            }
        }
        let sid = self.sid()?;
        let a = v.sel.active;
        if let Some(sheet) = self.workbook.sheet(sid) {
            if let Some(t) = sheet.table_at(a.row, a.col) {
                return Ok(t.range);
            }
            if let Some(f) = &sheet.meta().auto_filter
                && f.range.contains(a.row, a.col)
            {
                return Ok(f.range);
            }
        }
        self.current_region(a).ok_or_else(|| {
            KilnError::InvalidInput(
                "Select a cell in the data first: the active cell and its neighbours are empty."
                    .into(),
            )
        })
    }

    /// Runs `f` with the active sheet's geometry, without copying it when it is built.
    pub(crate) fn with_geometry<R>(&self, f: impl FnOnce(&SheetGeometry) -> R) -> R {
        match &self.geo {
            Some(g) => f(g),
            None => f(&self.geometry_ref()),
        }
    }

    /// Evaluates a formula at the active cell for [`GridQuery::Evaluate`] (nothing changes).
    pub(crate) fn prepare_evaluate(&mut self, formula: &str) {
        let text = formula.trim();
        let value = match self.sid() {
            Ok(sid) => {
                let a = self.view().sel.active;
                match self.workbook.evaluate_at(text, sid, a.row, a.col) {
                    Ok(v) => kiln_formula::engine::format_value(Some(&v)),
                    Err(e) => format!("Invalid: {e}"),
                }
            }
            Err(e) => e.to_string(),
        };
        self.features.evaluated = Some((formula.to_owned(), value));
    }

    /// Whole rows or columns limited to the used range (an empty sheet keeps them whole: a rule
    /// or a print area over whole columns covers them all).
    pub(crate) fn clip_to_used(&self, r: Range) -> Range {
        let Some(used) = self
            .active_sheet()
            .and_then(|(_, s)| s.cells().used_range(false))
        else {
            return r;
        };
        if r.is_whole_rows() || r.is_whole_cols() {
            r.intersection(&used.union(&Range::cell(r.r0.min(used.r1), r.c0.min(used.c1))))
                .unwrap_or(r)
        } else {
            r
        }
    }

    /// The cells a command that reads or writes each cell works on: whole rows or columns
    /// limited to the used range, and to their first cell when no data is in them (an empty
    /// sheet, or rows 2:1048576 below a header row), so it never walks a million rows or 16,384
    /// columns of nothing.
    pub(crate) fn clip_to_data(&self, r: Range) -> Range {
        if !(r.is_whole_rows() || r.is_whole_cols()) {
            return r;
        }
        let used = self
            .active_sheet()
            .and_then(|(_, s)| s.cells().used_range(false));
        match used {
            Some(u) if u.intersects(&r) => self.clip_to_used(r),
            _ => Range::cell(r.r0, r.c0),
        }
    }
}
