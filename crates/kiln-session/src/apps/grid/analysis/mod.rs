// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid's analysis features:
//!
//! - [`pivot`]: PivotTables — Insert ▸ PivotTable, the Fields pane, Value and Field Settings,
//!   PivotTable Analyze and Design, Refresh, Show Details, the field buttons;
//! - [`sparkline`]: Insert ▸ Sparklines and the Sparkline tab;
//! - [`goal_seek`], [`subtotal`] and [`flash`]: Goal Seek, Data ▸ Subtotal, Flash Fill with its
//!   suggestions;
//! - [`protect`]: Protect Sheet and Protect Workbook with SHA-512 password hashes, Lock Cell,
//!   and the message a refused change gets;
//! - [`themes`]: Page Layout ▸ Themes.
//!
//! As with the features, every change is one labelled step of the sheet model's command
//! log; what the chrome shows comes from [`GridDoc::analysis_state`].

mod flash;
mod goal_seek;
mod outline;
pub(crate) mod pivot;
pub(crate) mod protect;
pub(crate) mod sparkline;
mod subtotal;
#[cfg(test)]
mod tests;
mod themes;

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::{
    GridAnalysisState, GridFlashCell, GridGoalSeekStatus,
};
use kiln_protocol::commands::{GridCommand, GridHit, GridQuery, GridQueryResult};
use kiln_scene::{Bounds, DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::{CellPos, Range};
use kiln_sheet_layout::SheetGeometry;

use super::GridDoc;
use crate::model::Change;

/// The error command id of a change a protected sheet or workbook refuses.
pub(crate) const PROTECTED: &str = "grid.protected";
/// What a chart sheet says when cells are typed into or changed (it has none).
pub(crate) const CHART_SHEET_MESSAGE: &str =
    "This is a chart sheet: it holds a chart and no cells. To enter data, go to a worksheet.";
/// Excel's message for it.
pub(crate) const PROTECTED_MESSAGE: &str = "The cell or chart you're trying to change is on a \
     protected sheet. To make a change, unprotect the sheet. You might be requested to enter a \
     password.";
/// Excel's message when the workbook's structure is protected.
pub(crate) const STRUCTURE_MESSAGE: &str = "Workbook is protected and can't be changed.";

/// View state of the analysis features.
#[derive(Debug, Default)]
pub(crate) struct AnalysisState {
    /// The last Goal Seek.
    pub goal_seek: Option<GridGoalSeekStatus>,
    /// Flash Fill's suggestions for the last entry's column.
    pub flash: Vec<GridFlashCell>,
    /// A message box to show.
    pub alert: Option<String>,
    /// Message boxes raised so far.
    pub alert_seq: u32,
}

/// A sheet name as references write it (`'My sheet'`).
pub(crate) fn quote_sheet(name: &str) -> String {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !looks_like_a_reference(name)
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// A name a formula would read as a cell (`A1`, `XFD1048576`) or an R1C1 reference (`R1C1`,
/// `R`, `C3`): it needs quotes, as Excel writes it.
fn looks_like_a_reference(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    let letters = up.bytes().take_while(u8::is_ascii_uppercase).count();
    let rest = &up[letters..];
    let a1 =
        (1..=3).contains(&letters) && !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    let r1c1 = {
        let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
        let mut s = up.as_str();
        let mut any = false;
        if let Some(t) = s.strip_prefix('R') {
            s = &t[digits(t)..];
            any = true;
        }
        if let Some(t) = s.strip_prefix('C') {
            s = &t[digits(t)..];
            any = true;
        }
        any && s.is_empty()
    };
    a1 || r1c1
}

impl GridDoc {
    /// Runs a command, or `None` when the command is not one of them.
    pub(crate) fn analysis(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        use GridCommand as G;
        // A message box, Goal Seek's status and Flash Fill's suggestions last one command.
        self.analysis.alert = None;
        if !matches!(cmd, G::CommitEdit { .. } | G::Advance { .. }) {
            self.analysis.goal_seek = None;
        }
        self.analysis.flash.clear();
        if let Some(r) = self.guard(cmd) {
            return Some(r);
        }
        Some(match cmd {
            G::CreatePivot { source, to } => self.create_pivot(source, to.as_deref()),
            G::Pivot { edit } => self.pivot_edit(edit),
            G::RefreshPivots { all } => self.refresh_pivots(*all),
            G::PivotDetails { row, col } => self.pivot_details(*row, *col),
            G::InsertSparklines {
                kind,
                data,
                location,
            } => self.insert_sparklines(*kind, data, location),
            G::Sparklines { edit } => self.sparkline_edit(edit),
            G::GoalSeek {
                set_cell,
                to_value,
                by_changing,
            } => self.goal_seek(set_cell, to_value, by_changing),
            G::Subtotal { spec } => self.subtotal(spec.as_ref()),
            G::FlashFill => self.flash_fill(),
            G::ProtectSheet {
                protect,
                password,
                allow,
            } => self.protect_sheet(*protect, password, allow.as_ref()),
            G::ProtectWorkbook { protect, password } => self.protect_workbook(*protect, password),
            G::LockCells { locked } => self.lock_cells(*locked),
            G::SetTheme { theme, part } => self.set_theme(theme, *part),
            G::OutlineLevel { columns, level } => self.outline_level(*columns, *level),
            G::OutlineToggle { columns, index } => self.outline_toggle(*columns, *index),
            // Data ▸ Refresh All refreshes the PivotTables too.
            G::RefreshAll => {
                let r = self.feature(cmd)?;
                if r.is_ok()
                    && self
                        .workbook
                        .sheets()
                        .iter()
                        .any(|s| !s.pivots().is_empty())
                {
                    if let Err(e) = self.refresh_pivots(true) {
                        return Some(Err(e));
                    }
                    return Some(Ok(Change::Content));
                }
                r
            }
            _ => return None,
        })
    }

    /// After an entry is committed: Flash Fill's suggestions for its column.
    pub(crate) fn analysis_after_commit(&mut self, at: CellPos) {
        self.flash_suggest(at);
    }

    /// Commands a PivotTable report, a protected sheet or a chart sheet refuses, with a message.
    fn guard(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        use GridCommand as G;
        let sid = self.sid().ok()?;
        let sel = self.view().sel.clone();
        // Cells of a PivotTable report change only through the PivotTable.
        let content = match cmd {
            G::CommitEdit { .. } => self.edit.as_ref().map(|e| vec![Range::cell(e.row, e.col)]),
            G::ClearContents
            | G::FillDown
            | G::FillRight
            | G::FillUp
            | G::FillLeft
            | G::FillSelection
            | G::ArrayFormula
            | G::PasteSpecial { .. }
            | G::PasteValues { .. }
            | G::PasteFormulas { .. }
            | G::PasteTranspose { .. }
            | G::PasteLink { .. }
            | G::FillSeries { .. }
            | G::AutoFillOption { .. }
            | G::InsertCells { .. }
            | G::DeleteCells { .. }
            | G::SortAscending
            | G::SortDescending
            | G::ApplySort { .. }
            | G::TextToColumns { .. }
            | G::RemoveDuplicates { .. }
            | G::AutoSum { .. }
            | G::FlashFill => Some(sel.ranges.clone()),
            G::Clear { what } if !matches!(what, kiln_protocol::commands::GridClear::Formats) => {
                Some(sel.ranges.clone())
            }
            G::DragFill { .. } | G::AutoFill { .. } => Some(sel.ranges.clone()),
            _ => None,
        };
        // A chart sheet has no cells to type into or change.
        if self
            .workbook
            .sheet(sid)
            .is_some_and(|s| s.meta().chart_sheet)
            && (content.is_some() || matches!(cmd, G::BeginEdit { .. } | G::EditCell))
        {
            self.edit = None;
            self.raise_alert(CHART_SHEET_MESSAGE);
            return Some(Ok(Change::Selection));
        }
        if let Some(ranges) = content
            && let Some(sheet) = self.workbook.sheet(sid)
            && sheet.pivots().iter().any(|p| {
                p.whole_range()
                    .is_some_and(|w| ranges.iter().any(|r| r.intersects(&w)))
            })
        {
            if matches!(cmd, G::CommitEdit { .. }) {
                self.edit = None;
            }
            self.raise_alert(pivot::PIVOT_LOCKED);
            return Some(Ok(Change::Selection));
        }
        // Sparklines live in the sheet, not in its cells, so the model does not check them: a
        // protected sheet refuses them here, as Excel greys the commands out.
        if matches!(cmd, G::InsertSparklines { .. } | G::Sparklines { .. })
            && self
                .workbook
                .sheet(sid)
                .is_some_and(|s| s.meta().protection.is_some())
        {
            self.raise_alert(PROTECTED_MESSAGE);
            return Some(Ok(Change::Selection));
        }
        // Typing into a report or a locked cell of a protected sheet: the message, before any
        // typing.
        if matches!(cmd, G::BeginEdit { .. } | G::EditCell) && self.edit.is_none() {
            let at = Range::cell(sel.active.row, sel.active.col);
            if self.pivot_index_at(sel.active).is_some() {
                self.raise_alert(pivot::PIVOT_LOCKED);
                return Some(Ok(Change::Selection));
            }
            if self.workbook.check_editable(sid, &[at]).is_err() {
                self.raise_alert(PROTECTED_MESSAGE);
                return Some(Ok(Change::Selection));
            }
        }
        None
    }

    /// Shows a message box (once per refusal).
    pub(crate) fn raise_alert(&mut self, message: impl Into<String>) {
        self.analysis.alert = Some(message.into());
        self.analysis.alert_seq = self.analysis.alert_seq.wrapping_add(1);
    }

    /// Turns a refusal by sheet or workbook protection into Excel's message box.
    pub(crate) fn protection_refusal(&mut self, r: Result<Change>) -> Result<Change> {
        match r {
            Err(KilnError::Unavailable { command, reason }) if command == PROTECTED => {
                self.edit = None;
                self.raise_alert(reason);
                Ok(Change::Selection)
            }
            other => other,
        }
    }

    /// Answers a query.
    pub(crate) fn analysis_query(&self, q: &GridQuery) -> Option<Result<GridQueryResult>> {
        Some(match q {
            GridQuery::PivotItems { field } => {
                let a = self.view().sel.active;
                let p = self
                    .pivot_index_at(a)
                    .and_then(|(sid, i)| self.workbook.sheet(sid).and_then(|s| s.pivots().get(i)));
                match p {
                    Some(p) => Ok(GridQueryResult::Items(pivot::items(p, *field as usize))),
                    None => Err(KilnError::InvalidInput(
                        "Select a cell in the PivotTable report first.".into(),
                    )),
                }
            }
            GridQuery::PivotStyles => Ok(GridQueryResult::TableStyles(self.pivot_styles())),
            GridQuery::Themes => Ok(GridQueryResult::Themes(themes::infos())),
            _ => return None,
        })
    }

    /// The PivotTable Styles gallery: the built-in styles with their header, band and line
    /// colours in this workbook's theme.
    fn pivot_styles(&self) -> Vec<kiln_protocol::commands::grid_features::GridTableStyle> {
        use kiln_sheet::pivot::style::{PivotStyleElement as E, builtin_element, builtin_names};
        let theme = self.workbook.theme();
        let hex = |c: &kiln_sheet::color::Color| format!("{:06X}", c.resolve(theme, 0));
        builtin_names()
            .into_iter()
            .map(|name| {
                let header = builtin_element(&name, E::HeaderRow);
                let whole = builtin_element(&name, E::WholeTable);
                let band = builtin_element(&name, E::RowSubheading1)
                    .or_else(|| builtin_element(&name, E::FirstRowStripe));
                let fill = |d: &Option<kiln_sheet::styles::Dxf>| {
                    d.as_ref()
                        .and_then(|d| d.fill.as_ref())
                        .and_then(kiln_sheet::styles::Fill::main_color)
                        .map(|c| hex(&c))
                };
                let family = kiln_sheet::pivot::style::builtin_family(&name)
                    .map_or("Light", |f| f.0)
                    .to_owned();
                kiln_protocol::commands::grid_features::GridTableStyle {
                    header: fill(&header).or_else(|| fill(&whole)),
                    header_text: header
                        .as_ref()
                        .and_then(|d| d.font.as_ref())
                        .and_then(|f| f.color)
                        .map(|c| hex(&c)),
                    band: fill(&band),
                    line: whole
                        .as_ref()
                        .and_then(|d| d.border.as_ref())
                        .and_then(|b| b.top.or(b.bottom))
                        .and_then(|s| s.color)
                        .map(|c| hex(&c)),
                    family,
                    name,
                }
            })
            .collect()
    }

    /// The analysis part of the chrome's state.
    pub(crate) fn analysis_state(&self) -> GridAnalysisState {
        let mut s = GridAnalysisState {
            goal_seek: self.analysis.goal_seek.clone(),
            alert: self.analysis.alert.clone(),
            alert_seq: self.analysis.alert_seq,
            theme: self.workbook.theme().name.clone(),
            ..GridAnalysisState::default()
        };
        if let Some(p) = self.workbook.protection() {
            s.workbook_protected = p.lock_structure;
            s.workbook_password = p.password.is_some();
        }
        let Ok(sid) = self.sid() else {
            return s;
        };
        let a = self.view().sel.active;
        if let Some(sheet) = self.workbook.sheet(sid) {
            let prot = sheet.meta().protection.as_ref();
            s.sheet_protected = prot.is_some();
            s.sheet_password = prot.is_some_and(|p| p.password.is_some());
            s.permissions = protect::permissions_out(prot);
            s.locked = self
                .workbook
                .styles()
                .xf(sheet.style_at(a.row, a.col))
                .protection
                .locked;
            if let Some(p) = sheet
                .pivots()
                .iter()
                .find(|p| p.whole_range().is_some_and(|r| r.contains(a.row, a.col)))
            {
                s.pivot = Some(pivot::info(p, a));
            }
            // The first empty report in view gets its prompt.
            if let Some(p) = sheet.pivots().iter().find(|p| p.output.empty)
                && let Some(r) = p.output.range
            {
                s.placeholder = Some((p.name.clone(), self.range_rect_surface(r)));
            }
        }
        s.sparkline = self.sparkline_info();
        s.flash_count = u32::try_from(self.analysis.flash.len()).unwrap_or(0);
        s.flash = self
            .analysis
            .flash
            .iter()
            .take(200)
            .map(|c| GridFlashCell {
                rect: self.range_rect_surface(Range::cell(c.row, c.col)),
                ..c.clone()
            })
            .collect();
        s
    }

    /// A range's rectangle on the surface (x, y, width, height in CSS pixels).
    pub(crate) fn range_rect_surface(&self, r: Range) -> [f32; 4] {
        self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let rect = geo.range_rect(&r);
            let r = geo.surface_rect(&self.last_view, rect, hw);
            [r.x, r.y, r.w, r.h]
        })
    }

    /// The overlay of the analysis features (sheet pixels): the PivotTable field buttons and
    /// the frame of an empty report.
    pub(crate) fn analysis_overlay(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Some((_, sheet)) = self.active_sheet() else {
            return;
        };
        for p in sheet.pivots() {
            if p.output.empty {
                if let Some(r) = p.output.range {
                    let b = geo.range_rect(&r);
                    out.push(DrawOp::FillRect {
                        rect: b,
                        paint: Paint::Role(kiln_scene::ColorRole::Page),
                        radius: 0.0,
                    });
                    out.push(DrawOp::StrokeRect {
                        rect: b.inflate(-0.5, -0.5),
                        paint: Paint::Role(kiln_scene::ColorRole::Line),
                        width: 1.0,
                        radius: 0.0,
                    });
                }
                continue;
            }
            for (row, col) in pivot_buttons(p) {
                if geo.rows.is_hidden(row) || geo.cols.is_hidden(col) {
                    continue;
                }
                let b = button_rect(geo, row, col);
                out.push(DrawOp::FillRect {
                    rect: b,
                    paint: Paint::Role(kiln_scene::ColorRole::Page),
                    radius: 3.0,
                });
                out.push(DrawOp::StrokeRect {
                    rect: b.inflate(-0.5, -0.5),
                    paint: Paint::Role(kiln_scene::ColorRole::Line),
                    width: 1.0,
                    radius: 3.0,
                });
                let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                let mut path = PathData::new();
                path.move_to(cx - 3.0, cy - 1.5)
                    .line_to(cx, cy + 1.5)
                    .line_to(cx + 3.0, cy - 1.5);
                out.push(DrawOp::Path {
                    path,
                    fill: None,
                    stroke: Some(Stroke::solid(
                        Paint::Role(kiln_scene::ColorRole::Muted),
                        1.4,
                    )),
                    rule: FillRule::NonZero,
                });
            }
        }
    }

    /// The outline or PivotTable field button under a point.
    pub(crate) fn analysis_hit(&self, x: f32, y: f32) -> Option<GridHit> {
        if let Some(hit) = self.outline_hit_at(x, y, false) {
            return Some(hit);
        }
        let (_, sheet) = self.active_sheet()?;
        if sheet.pivots().is_empty() {
            return None;
        }
        self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
            for p in sheet.pivots() {
                for (row, col) in pivot_buttons(p) {
                    if button_rect(geo, row, col)
                        .inflate(1.0, 1.0)
                        .contains(sx, sy)
                    {
                        return Some(GridHit::PivotButton { row, col });
                    }
                }
            }
            None
        })
    }

    /// Several commands of the analysis features as one undo step: `f` may run any number of
    /// `exec` and `exec_all` calls.
    pub(crate) fn exec_group(
        &mut self,
        label: &'static str,
        f: impl FnOnce(&mut GridDoc) -> Result<()>,
    ) -> Result<bool> {
        let pushed = self.log.pushed();
        self.log.begin(label);
        if let Err(e) = f(self) {
            let _ = self.log.rollback(&mut self.workbook);
            self.absorb_changes();
            return Err(e);
        }
        self.log
            .commit(&mut self.workbook)
            .map_err(super::model_err)?;
        let recorded = self.log.pushed() != pushed;
        self.after_change(label, recorded);
        Ok(recorded)
    }

    /// Selects a range with the active cell in it.
    pub(crate) fn select_range_of(&mut self, r: Range, active: CellPos) {
        let active = if r.contains(active.row, active.col) {
            active
        } else {
            CellPos::new(r.r0, r.c0)
        };
        let v = Self::view_with(super::view::Sel {
            ranges: vec![r],
            active,
        });
        self.set_view(v);
    }
}

/// The cells with a field button: the filters' values, Row Labels and Column Labels.
fn pivot_buttons(p: &kiln_sheet::pivot::PivotTable) -> Vec<(u32, u32)> {
    let out = &p.output;
    let mut v = Vec::new();
    if let Some(pages) = out.pages {
        for row in pages.r0..=pages.r1 {
            v.push((row, pages.c0 + 1));
        }
    }
    let Some(r) = out.range else {
        return v;
    };
    let caption = r.r0 + out.header_rows.max(1) - 1;
    if !p.rows.is_empty() {
        if p.options.layout == kiln_sheet::pivot::PivotLayout::Compact {
            v.push((caption, r.c0));
        } else {
            for k in 0..out.label_cols {
                if matches!(
                    p.rows.get(k as usize),
                    Some(kiln_sheet::pivot::AxisItem::Field(_))
                ) {
                    v.push((caption, r.c0 + k));
                }
            }
        }
    }
    if out.col_label_rows > 0
        && p.cols
            .iter()
            .any(|a| matches!(a, kiln_sheet::pivot::AxisItem::Field(_)))
    {
        v.push((r.r0, r.c0 + out.label_cols));
    }
    v
}

fn button_rect(geo: &SheetGeometry, row: u32, col: u32) -> Bounds {
    let cell = geo.display_rect(row, col);
    let s = (cell.h - 4.0).clamp(10.0, 17.0);
    Bounds::new(cell.right() - s - 2.0, cell.bottom() - s - 2.0, s, s)
}
