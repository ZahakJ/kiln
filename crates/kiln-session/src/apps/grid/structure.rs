// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rows, columns, sheets and the rest of the sheet-level commands: insert and delete cells,
//! rows and columns, sizes (drag, dialog, AutoFit), hide and unhide, sheet tabs (add, rename,
//! move, colour, hide, delete, duplicate), Clear, Fill Down/Right/Up/Left, the fill handle,
//! calculation, frozen panes, split and the view toggles.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::{CellShift, GridClear, GridCommand, GridFill};
use kiln_sheet::axis::{chars_to_col_width, px_to_col_width, px_to_pt};
use kiln_sheet::commands::{
    ClearWhat, ColPatch, FormatFrom, MetaField, PasteOp, PasteWhat, RowPatch,
};
use kiln_sheet::fill::FillMode;
use kiln_sheet::meta::SheetVisibility;
use kiln_sheet::num::F64;
use kiln_sheet::styles::Patch;
use kiln_sheet::{CellPos, MAX_COLS, MAX_ROWS, Range, SheetId};
use kiln_sheet_layout::celltext::{PAD, layout_line, layout_wrapped, runs_for, text_width};

use super::GridDoc;
use super::view::Sel;
use crate::model::Change;

/// Tallest row Excel allows (409 pt).
const MAX_ROW_PT: f64 = 409.0;

fn fill_mode(m: GridFill) -> FillMode {
    match m {
        GridFill::Auto => FillMode::Auto,
        GridFill::Copy => FillMode::Copy,
        GridFill::Series => FillMode::Series,
        GridFill::FormatsOnly => FillMode::FormatsOnly,
        GridFill::WithoutFormatting => FillMode::WithoutFormatting,
        GridFill::Days => FillMode::Days,
        GridFill::Weekdays => FillMode::Weekdays,
        GridFill::Months => FillMode::Months,
        GridFill::Years => FillMode::Years,
        GridFill::LinearTrend => FillMode::LinearTrend,
        GridFill::Growth => FillMode::Growth,
    }
}

/// The range the fill handle covers when dragged from `source` to a cell: the source grown
/// along the axis the pointer left it furthest on.
#[must_use]
pub fn fill_target(source: Range, row: u32, col: u32) -> Option<Range> {
    let dr = if row > source.r1 {
        i64::from(row - source.r1)
    } else if row < source.r0 {
        i64::from(source.r0 - row)
    } else {
        0
    };
    let dc = if col > source.c1 {
        i64::from(col - source.c1)
    } else if col < source.c0 {
        i64::from(source.c0 - col)
    } else {
        0
    };
    if dr == 0 && dc == 0 {
        return None;
    }
    Some(if dr >= dc {
        Range::new(source.r0.min(row), source.c0, source.r1.max(row), source.c1)
    } else {
        Range::new(source.r0, source.c0.min(col), source.r1, source.c1.max(col))
    })
}

impl GridDoc {
    fn sel_ranges(&self) -> Vec<Range> {
        self.view().sel.ranges
    }

    fn last_range(&self) -> Result<Range> {
        self.sel_ranges()
            .last()
            .copied()
            .ok_or_else(|| KilnError::InvalidInput("nothing is selected".into()))
    }

    fn mdw(&self) -> f64 {
        self.active_sheet()
            .map_or(7.0, |(_, s)| s.max_digit_width())
    }

    /// The rows the selection covers, as `(at, count)` runs (descending for deletes).
    fn row_runs(&self) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = self
            .sel_ranges()
            .iter()
            .map(|r| (r.r0, r.r1 - r.r0 + 1))
            .collect();
        runs.sort_unstable();
        runs.dedup();
        runs
    }

    fn col_runs(&self) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = self
            .sel_ranges()
            .iter()
            .map(|r| (r.c0, r.c1 - r.c0 + 1))
            .collect();
        runs.sort_unstable();
        runs.dedup();
        runs
    }

    /// The last row holding anything in columns `c0..=c1`.
    fn last_row_in(&self, sid: SheetId, c0: u32, c1: u32) -> Option<u32> {
        let s = self.workbook.sheet(sid)?;
        s.cells().rows().iter().rev().copied().find(|&r| {
            s.cells().chunk(r).is_some_and(|ch| {
                let i = ch.lower_bound(c0);
                i < ch.len() && ch.at(i).0 <= c1
            })
        })
    }

    /// The last column holding anything in rows `r0..=r1`.
    fn last_col_in(&self, sid: SheetId, r0: u32, r1: u32) -> Option<u32> {
        let s = self.workbook.sheet(sid)?;
        s.cells()
            .iter_range(Range::new(r0, 0, r1, MAX_COLS - 1))
            .map(|(_, c, _)| c)
            .max()
    }

    /// Insert Cells / Delete Cells shifting the neighbours (moves keep references following
    /// the cells, as Excel's shift does).
    fn shift_cells(&mut self, insert: bool, shift: CellShift) -> Result<()> {
        let sid = self.sid()?;
        let ranges = self.sel_ranges();
        let shift = match shift {
            CellShift::Auto if ranges.iter().all(Range::is_whole_rows) => CellShift::Rows,
            CellShift::Auto if ranges.iter().all(Range::is_whole_cols) => CellShift::Columns,
            CellShift::Auto => CellShift::Vertical,
            s => s,
        };
        match shift {
            CellShift::Rows => return self.rows_or_cols(insert, true),
            CellShift::Columns => return self.rows_or_cols(insert, false),
            _ => {}
        }
        let r = self.last_range()?;
        let (h, w) = (r.height(), r.width());
        let mut cmds = Vec::new();
        let label = if insert {
            "grid.cells.insert_cells"
        } else {
            "grid.cells.delete_cells"
        };
        match (shift, insert) {
            (CellShift::Vertical, true) => {
                if let Some(last) = self.last_row_in(sid, r.c0, r.c1).filter(|&l| l >= r.r0) {
                    if last + h >= MAX_ROWS {
                        return Err(KilnError::InvalidInput(
                            "inserting would push cells off the end of the sheet".into(),
                        ));
                    }
                    cmds.push(kiln_sheet::Command::MoveRange {
                        sheet: sid,
                        range: Range::new(r.r0, r.c0, last, r.c1),
                        to_sheet: None,
                        to: CellPos::new(r.r0 + h, r.c0),
                    });
                }
            }
            (CellShift::Horizontal, true) => {
                if let Some(last) = self.last_col_in(sid, r.r0, r.r1).filter(|&l| l >= r.c0) {
                    if last + w >= MAX_COLS {
                        return Err(KilnError::InvalidInput(
                            "inserting would push cells off the end of the sheet".into(),
                        ));
                    }
                    cmds.push(kiln_sheet::Command::MoveRange {
                        sheet: sid,
                        range: Range::new(r.r0, r.c0, r.r1, last),
                        to_sheet: None,
                        to: CellPos::new(r.r0, r.c0 + w),
                    });
                }
            }
            (CellShift::Vertical, false) => {
                cmds.push(kiln_sheet::Command::ClearRange {
                    sheet: sid,
                    ranges: vec![r],
                    what: ClearWhat::All,
                });
                if let Some(last) = self.last_row_in(sid, r.c0, r.c1).filter(|&l| l > r.r1) {
                    cmds.push(kiln_sheet::Command::MoveRange {
                        sheet: sid,
                        range: Range::new(r.r1 + 1, r.c0, last, r.c1),
                        to_sheet: None,
                        to: CellPos::new(r.r0, r.c0),
                    });
                }
            }
            (CellShift::Horizontal, false) => {
                cmds.push(kiln_sheet::Command::ClearRange {
                    sheet: sid,
                    ranges: vec![r],
                    what: ClearWhat::All,
                });
                if let Some(last) = self.last_col_in(sid, r.r0, r.r1).filter(|&l| l > r.c1) {
                    cmds.push(kiln_sheet::Command::MoveRange {
                        sheet: sid,
                        range: Range::new(r.r0, r.c1 + 1, r.r1, last),
                        to_sheet: None,
                        to: CellPos::new(r.r0, r.c0),
                    });
                }
            }
            _ => {}
        }
        self.exec_all(label, cmds)?;
        Ok(())
    }

    fn rows_or_cols(&mut self, insert: bool, rows: bool) -> Result<()> {
        let sid = self.sid()?;
        let mut runs = if rows {
            self.row_runs()
        } else {
            self.col_runs()
        };
        // Later runs first, so earlier positions stay valid.
        runs.sort_unstable_by(|a, b| b.cmp(a));
        let cmds: Vec<kiln_sheet::Command> = runs
            .into_iter()
            .map(|(at, count)| match (insert, rows) {
                (true, true) => kiln_sheet::Command::InsertRows {
                    sheet: sid,
                    at,
                    count,
                    format_from: FormatFrom::Before,
                },
                (true, false) => kiln_sheet::Command::InsertCols {
                    sheet: sid,
                    at,
                    count,
                    format_from: FormatFrom::Before,
                },
                (false, true) => kiln_sheet::Command::DeleteRows {
                    sheet: sid,
                    at,
                    count,
                },
                (false, false) => kiln_sheet::Command::DeleteCols {
                    sheet: sid,
                    at,
                    count,
                },
            })
            .collect();
        let label = match (insert, rows) {
            (true, true) => "grid.cells.insert_rows",
            (true, false) => "grid.cells.insert_columns",
            (false, true) => "grid.cells.delete_rows",
            (false, false) => "grid.cells.delete_columns",
        };
        self.exec_all(label, cmds)?;
        Ok(())
    }

    fn set_rows(
        &mut self,
        label: &'static str,
        runs: Vec<(u32, u32)>,
        patch: RowPatch,
    ) -> Result<()> {
        let sid = self.sid()?;
        let cmds = runs
            .into_iter()
            .map(|(at, count)| kiln_sheet::Command::SetRowProps {
                sheet: sid,
                at,
                count,
                patch: patch.clone(),
            })
            .collect();
        self.exec_all(label, cmds)?;
        Ok(())
    }

    fn set_cols(
        &mut self,
        label: &'static str,
        runs: Vec<(u32, u32)>,
        patch: ColPatch,
    ) -> Result<()> {
        let sid = self.sid()?;
        let cmds = runs
            .into_iter()
            .map(|(at, count)| kiln_sheet::Command::SetColProps {
                sheet: sid,
                at,
                count,
                patch: patch.clone(),
            })
            .collect();
        self.exec_all(label, cmds)?;
        Ok(())
    }

    /// The height a row's content needs (pixels), or `None` when it has nothing taller than
    /// the default.
    fn content_height(&mut self, sid: SheetId, row: u32) -> Option<f64> {
        let cells: Vec<(u32, String, CellPosLook)> = {
            let s = self.workbook.sheet(sid)?;
            let ch = s.cells().chunk(row)?;
            let geo = self.geometry_ref();
            ch.iter()
                .filter(|(_, d)| d.has_content() || d.style != 0)
                .take(512)
                .map(|(c, d)| {
                    let text = if d.has_content() {
                        self.workbook.display_text(sid, row, c)
                    } else {
                        String::new()
                    };
                    (
                        c,
                        text,
                        CellPosLook {
                            style: d.style,
                            width: geo.cols.size(c) as f32,
                        },
                    )
                })
                .collect()
        };
        let colors = kiln_sheet_layout::look::Colors {
            theme: self.workbook.theme(),
        };
        let mut best: f64 = 0.0;
        let styles: Vec<(u32, String, kiln_sheet::styles::CellStyle, f32)> = cells
            .into_iter()
            .map(|(c, t, l)| (c, t, self.workbook.styles().get(l.style), l.width))
            .collect();
        let looks: Vec<(String, kiln_sheet_layout::look::CellLook, f32)> = styles
            .into_iter()
            .map(|(_, t, st, w)| {
                (
                    t,
                    kiln_sheet_layout::look::resolve(colors, &st, None, None),
                    w,
                )
            })
            .collect();
        let scene = self.scene.get_or_insert_with(|| {
            kiln_sheet_layout::GridScene::new(crate::apps::scratch::fonts::store())
        });
        let engine = scene.text_engine();
        for (text, look, width) in looks {
            let sample = if text.is_empty() {
                "Xg".to_owned()
            } else {
                text
            };
            let runs = runs_for(
                &sample,
                &kiln_sheet::CellValue::Blank,
                &look,
                look.ink,
                colors,
                1.0,
            );
            let layout = if look.align.wrap_text {
                layout_wrapped(
                    engine,
                    &runs,
                    (width - 2.0 * PAD).max(1.0),
                    look.align.horizontal,
                    false,
                )
            } else {
                layout_line(engine, &runs)
            };
            let mut h = f64::from(layout.height) + 2.0;
            if let Some(angle) = kiln_sheet_layout::celltext::rotation(look.align.text_rotation) {
                let w = f64::from(text_width(&layout));
                h = w * f64::from(angle.sin().abs())
                    + f64::from(layout.height) * f64::from(angle.cos().abs())
                    + 4.0;
            }
            best = best.max(h);
        }
        (best > 0.0).then_some(best)
    }

    /// Fits a row to its content unless the user sized it (Excel's automatic row height).
    pub(crate) fn autofit_row_if_auto(&mut self, row: u32) {
        let Ok(sid) = self.sid() else {
            return;
        };
        let custom = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.rows().get(row))
            .is_some_and(|p| p.custom_height);
        if custom {
            return;
        }
        let default_px = self
            .workbook
            .sheet(sid)
            .map_or(20.0, |s| s.rows().default_px());
        let Some(need) = self.content_height(sid, row) else {
            return;
        };
        let current = self
            .workbook
            .sheet(sid)
            .map_or(default_px, |s| s.rows().size_px(row));
        let target = need.max(default_px).min(MAX_ROW_PT / 0.75);
        if (target - current).abs() < 0.75 {
            return;
        }
        let height = if (target - default_px).abs() < 0.75 {
            Patch::Clear
        } else {
            Patch::Set(F64((px_to_pt(target) * 4.0).round() / 4.0))
        };
        // Part of the edit's undo step when there is one; never an undo step on its own.
        let _ = self.workbook.apply(&kiln_sheet::Command::SetRowProps {
            sheet: sid,
            at: row,
            count: 1,
            patch: RowPatch {
                height,
                custom_height: Some(false),
                ..RowPatch::default()
            },
        });
    }

    /// Fits the rows of the selection (after a font size, wrap or rotation change).
    pub(crate) fn autofit_selection_rows(&mut self) {
        let rows: Vec<u32> = self
            .sel_ranges()
            .iter()
            .filter(|r| r.height() <= 2_000)
            .flat_map(|r| r.r0..=r.r1)
            .collect();
        for r in rows {
            self.autofit_row_if_auto(r);
        }
    }

    /// The width a column's content needs (pixels).
    fn content_width(&mut self, sid: SheetId, col: u32) -> Option<f64> {
        let entries: Vec<(String, u32)> = {
            let s = self.workbook.sheet(sid)?;
            let used = s.cells().used_range(true)?;
            s.cells()
                .iter_range(Range::new(used.r0, col, used.r1, col))
                .filter(|(_, _, d)| d.has_content())
                .take(20_000)
                .map(|(r, c, d)| (self.workbook.display_text(sid, r, c), d.style))
                .collect()
        };
        let colors = kiln_sheet_layout::look::Colors {
            theme: self.workbook.theme(),
        };
        let looks: Vec<(String, kiln_sheet_layout::look::CellLook)> = entries
            .into_iter()
            .map(|(t, st)| {
                (
                    t,
                    kiln_sheet_layout::look::resolve(
                        colors,
                        &self.workbook.styles().get(st),
                        None,
                        None,
                    ),
                )
            })
            .collect();
        let scene = self.scene.get_or_insert_with(|| {
            kiln_sheet_layout::GridScene::new(crate::apps::scratch::fonts::store())
        });
        let engine = scene.text_engine();
        let mut best: f64 = 0.0;
        for (text, look) in looks {
            let runs = runs_for(
                &text,
                &kiln_sheet::CellValue::Blank,
                &look,
                look.ink,
                colors,
                1.0,
            );
            let w = f64::from(text_width(&layout_line(engine, &runs)))
                + 2.0 * f64::from(PAD)
                + 2.0
                + f64::from(look.align.indent) * 9.0;
            best = best.max(w);
        }
        (best > 0.0).then_some(best)
    }

    fn fill_dir(&mut self, label: &'static str, dir: u8) -> Result<()> {
        let sid = self.sid()?;
        let mut cmds = Vec::new();
        let used = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.cells().used_range(false))
            .unwrap_or_else(|| Range::cell(0, 0));
        for r in self.sel_ranges() {
            // Whole rows and columns fill their used part (as the clipboard copies it): filling
            // a million blank cells with blanks is work without an effect.
            let r = Range::new(
                r.r0,
                r.c0,
                if r.r0 == 0 && r.r1 + 1 >= MAX_ROWS {
                    r.r1.min(used.r1.max(r.r0 + 1))
                } else {
                    r.r1
                },
                if r.c0 == 0 && r.c1 + 1 >= MAX_COLS {
                    r.c1.min(used.c1.max(r.c0 + 1))
                } else {
                    r.c1
                },
            );
            // A single cell fills from its neighbour (Excel's Ctrl+D on one cell copies the cell
            // above).
            let (src, dst) = match dir {
                0 => {
                    if r.height() == 1 {
                        if r.r0 == 0 {
                            continue;
                        }
                        (Range::new(r.r0 - 1, r.c0, r.r0 - 1, r.c1), r)
                    } else {
                        (
                            Range::new(r.r0, r.c0, r.r0, r.c1),
                            Range::new(r.r0 + 1, r.c0, r.r1, r.c1),
                        )
                    }
                }
                1 => {
                    if r.width() == 1 {
                        if r.c0 == 0 {
                            continue;
                        }
                        (Range::new(r.r0, r.c0 - 1, r.r1, r.c0 - 1), r)
                    } else {
                        (
                            Range::new(r.r0, r.c0, r.r1, r.c0),
                            Range::new(r.r0, r.c0 + 1, r.r1, r.c1),
                        )
                    }
                }
                2 => {
                    if r.height() == 1 {
                        if r.r1 + 1 >= MAX_ROWS {
                            continue;
                        }
                        (Range::new(r.r1 + 1, r.c0, r.r1 + 1, r.c1), r)
                    } else {
                        (
                            Range::new(r.r1, r.c0, r.r1, r.c1),
                            Range::new(r.r0, r.c0, r.r1 - 1, r.c1),
                        )
                    }
                }
                _ => {
                    if r.width() == 1 {
                        if r.c1 + 1 >= MAX_COLS {
                            continue;
                        }
                        (Range::new(r.r0, r.c1 + 1, r.r1, r.c1 + 1), r)
                    } else {
                        (
                            Range::new(r.r0, r.c1, r.r1, r.c1),
                            Range::new(r.r0, r.c0, r.r1, r.c1 - 1),
                        )
                    }
                }
            };
            cmds.push(kiln_sheet::Command::CopyRange {
                sheet: sid,
                range: src,
                to_sheet: None,
                to: dst,
                what: PasteWhat::All,
                op: PasteOp::None,
                transpose: false,
                skip_blanks: false,
            });
        }
        self.exec_all(label, cmds)?;
        Ok(())
    }

    /// Changes the sheet's view (gridlines, headings, formulas, freeze): not an undo step.
    fn set_sheet_view(&mut self, f: impl FnOnce(&mut kiln_sheet::meta::SheetView)) -> Result<()> {
        let sid = self.sid()?;
        let mut view = self
            .workbook
            .sheet(sid)
            .map(|s| s.meta().view.clone())
            .unwrap_or_default();
        f(&mut view);
        self.workbook
            .apply(&kiln_sheet::Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::View(view)),
            })
            .map_err(super::model_err)?;
        self.absorb_changes();
        self.geo = None;
        Ok(())
    }

    fn freeze(&mut self, rows: u32, cols: u32) -> Result<()> {
        let sid = self.sid()?;
        self.workbook
            .apply(&kiln_sheet::Command::SetFreeze {
                sheet: sid,
                rows,
                cols,
            })
            .map_err(super::model_err)?;
        self.absorb_changes();
        self.geo = None;
        if let Some(s) = &mut self.scene {
            s.invalidate_all();
        }
        Ok(())
    }

    /// The sheet-level commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn structure(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        let change = match cmd {
            G::ActivateSheet { index } => {
                let i = *index as usize;
                if i >= self.workbook.sheets().len() {
                    return Err(KilnError::InvalidInput(format!(
                        "there is no sheet {index}"
                    )));
                }
                self.activate(i);
                Change::Selection
            }
            G::PreviousSheet | G::NextSheet => {
                let n = self.workbook.sheets().len();
                let forward = matches!(cmd, G::NextSheet);
                let mut i = self.active;
                // Hidden sheets are skipped; the ends stop (as Excel).
                loop {
                    let next = if forward { i + 1 } else { i.wrapping_sub(1) };
                    if next >= n {
                        break;
                    }
                    i = next;
                    if self.workbook.sheets()[i].meta().visibility == SheetVisibility::Visible {
                        self.activate(i);
                        break;
                    }
                }
                Change::Selection
            }
            G::InsertSheet => {
                let index = self.active + 1;
                self.exec(
                    "grid.cells.new_sheet",
                    kiln_sheet::Command::AddSheet {
                        name: None,
                        index: Some(index),
                        id: None,
                    },
                )?;
                self.activate(index);
                Change::Content
            }
            G::RenameSheet { index, name } => {
                let sid = self.sheet_at(*index)?;
                self.exec(
                    "grid.cells.rename_sheet",
                    kiln_sheet::Command::RenameSheet {
                        sheet: sid,
                        name: name.trim().to_owned(),
                    },
                )?;
                Change::Content
            }
            G::DeleteSheet { index } => {
                let sid = self.sheet_at(*index)?;
                if self.workbook.visible_sheet_count() <= 1
                    && self
                        .workbook
                        .sheet(sid)
                        .is_some_and(|s| s.meta().visibility == SheetVisibility::Visible)
                {
                    return Err(KilnError::InvalidInput(
                        "a workbook must contain at least one visible sheet".into(),
                    ));
                }
                let was_active = *index as usize == self.active;
                self.exec(
                    "grid.sheet.delete",
                    kiln_sheet::Command::RemoveSheet { sheet: sid },
                )?;
                let n = self.workbook.sheets().len();
                if was_active || self.active >= n {
                    let i = (*index as usize).min(n.saturating_sub(1));
                    self.active = usize::MAX;
                    self.activate(i);
                } else if (*index as usize) < self.active {
                    self.active -= 1;
                }
                Change::Content
            }
            G::MoveSheet { from, to } => {
                let sid = self.sheet_at(*from)?;
                let n = self.workbook.sheets().len() as u32;
                let to = (*to).min(n.saturating_sub(1));
                let active_id = self.sid()?;
                self.exec(
                    "grid.sheet.move",
                    kiln_sheet::Command::MoveSheet {
                        sheet: sid,
                        index: to as usize,
                    },
                )?;
                self.active = self.workbook.sheet_index(active_id).unwrap_or(0);
                Change::Content
            }
            G::DuplicateSheet { index } => {
                let sid = self.sheet_at(*index)?;
                let snap = self
                    .workbook
                    .snapshot_sheet(sid)
                    .map_err(super::model_err)?;
                let base = snap.name.clone();
                let mut k = 2;
                let name = loop {
                    let candidate = format!("{} ({k})", base.chars().take(26).collect::<String>());
                    if self.workbook.sheet_by_name(&candidate).is_none() {
                        break candidate;
                    }
                    k += 1;
                };
                let at = *index as usize + 1;
                self.exec(
                    "grid.sheet.duplicate",
                    kiln_sheet::Command::AddSheet {
                        name: Some(name.clone()),
                        index: Some(at),
                        id: None,
                    },
                )?;
                let new_id = self
                    .workbook
                    .sheet_by_name(&name)
                    .map(kiln_sheet::Sheet::id)
                    .ok_or_else(|| KilnError::Internal("copied sheet missing".into()))?;
                // Contents, layout and metadata of the source (tables are not duplicated: their
                // names must stay unique).
                let mut meta = snap.meta.clone();
                meta.tables.clear();
                let cells = kiln_sheet::commands::SheetCells {
                    sheet: new_id,
                    records: snap.cells.clone(),
                };
                self.exec(
                    "grid.sheet.duplicate",
                    kiln_sheet::Command::Restore {
                        cells: vec![cells],
                        meta: vec![kiln_sheet::commands::SheetMetaEntry {
                            sheet: new_id,
                            meta: Box::new(meta),
                        }],
                        layout: vec![kiln_sheet::commands::SheetLayoutEntry {
                            sheet: new_id,
                            layout: snap.layout.clone(),
                        }],
                        names: None,
                    },
                )?;
                self.activate(at);
                Change::Content
            }
            G::SheetTabColor { index, color } => {
                let sid = self.sheet_at(*index)?;
                let c = match color.as_deref().filter(|c| !c.is_empty()) {
                    Some(c) => Some(super::format::parse_color(c).ok_or_else(|| {
                        KilnError::InvalidInput(format!("“{c}” is not a colour"))
                    })?),
                    None => None,
                };
                self.exec(
                    "grid.sheet.tab_color",
                    kiln_sheet::Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::TabColor(c)),
                    },
                )?;
                Change::Content
            }
            G::HideSheet { index } => {
                let sid = self.sheet_at(*index)?;
                if self.workbook.visible_sheet_count() <= 1 {
                    return Err(KilnError::InvalidInput(
                        "a workbook must contain at least one visible sheet".into(),
                    ));
                }
                self.exec(
                    "grid.sheet.hide",
                    kiln_sheet::Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::Visibility(SheetVisibility::Hidden)),
                    },
                )?;
                if *index as usize == self.active {
                    let next = self
                        .workbook
                        .sheets()
                        .iter()
                        .position(|s| s.meta().visibility == SheetVisibility::Visible)
                        .unwrap_or(0);
                    self.active = usize::MAX;
                    self.activate(next);
                }
                Change::Content
            }
            G::UnhideSheet { index } => {
                let sid = self.sheet_at(*index)?;
                self.exec(
                    "grid.sheet.unhide",
                    kiln_sheet::Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::Visibility(SheetVisibility::Visible)),
                    },
                )?;
                self.activate(*index as usize);
                Change::Content
            }
            G::InsertCells { shift } => {
                self.shift_cells(true, *shift)?;
                Change::Content
            }
            G::DeleteCells { shift } => {
                self.shift_cells(false, *shift)?;
                Change::Content
            }
            G::InsertRows => {
                self.rows_or_cols(true, true)?;
                Change::Content
            }
            G::InsertColumns => {
                self.rows_or_cols(true, false)?;
                Change::Content
            }
            G::DeleteRows => {
                self.rows_or_cols(false, true)?;
                Change::Content
            }
            G::DeleteColumns => {
                self.rows_or_cols(false, false)?;
                Change::Content
            }
            G::RowHeight { points } => {
                if !(0.0..=MAX_ROW_PT).contains(points) {
                    return Err(KilnError::InvalidInput(
                        "row height is 0 to 409 points".into(),
                    ));
                }
                let patch = if *points == 0.0 {
                    RowPatch {
                        hidden: Some(true),
                        ..RowPatch::default()
                    }
                } else {
                    RowPatch {
                        height: Patch::Set(F64(*points)),
                        custom_height: Some(true),
                        hidden: Some(false),
                        ..RowPatch::default()
                    }
                };
                self.set_rows("grid.cells.row_height", self.row_runs(), patch)?;
                Change::Content
            }
            G::ColumnWidth { chars } => {
                if !(0.0..=255.0).contains(chars) {
                    return Err(KilnError::InvalidInput(
                        "column width is 0 to 255 characters".into(),
                    ));
                }
                let patch = if *chars == 0.0 {
                    ColPatch {
                        hidden: Some(true),
                        ..ColPatch::default()
                    }
                } else {
                    ColPatch {
                        width: Patch::Set(F64(chars_to_col_width(*chars, self.mdw()))),
                        custom_width: Some(true),
                        hidden: Some(false),
                        ..ColPatch::default()
                    }
                };
                self.set_cols("grid.cells.column_width", self.col_runs(), patch)?;
                Change::Content
            }
            G::ResizeRows { from, to, px } => {
                if !px.is_finite() || *px < 0.0 {
                    return Err(KilnError::InvalidInput(
                        "a row height is a positive number".into(),
                    ));
                }
                let pt = (px_to_pt(*px) * 4.0).round() / 4.0;
                let pt = pt.min(MAX_ROW_PT);
                let runs = self.resize_runs(*from, *to, true);
                let patch = if pt <= 0.0 {
                    RowPatch {
                        hidden: Some(true),
                        ..RowPatch::default()
                    }
                } else {
                    RowPatch {
                        height: Patch::Set(F64(pt)),
                        custom_height: Some(true),
                        hidden: Some(false),
                        ..RowPatch::default()
                    }
                };
                self.set_rows("grid.cells.resize_rows", runs, patch)?;
                Change::Content
            }
            G::ResizeColumns { from, to, px } => {
                if !px.is_finite() || *px < 0.0 {
                    return Err(KilnError::InvalidInput(
                        "a column width is a positive number".into(),
                    ));
                }
                let runs = self.resize_runs(*from, *to, false);
                let width = px_to_col_width(px.min(1790.0), self.mdw());
                let patch = if *px <= 0.0 {
                    ColPatch {
                        hidden: Some(true),
                        ..ColPatch::default()
                    }
                } else {
                    ColPatch {
                        width: Patch::Set(F64(width)),
                        custom_width: Some(true),
                        hidden: Some(false),
                        ..ColPatch::default()
                    }
                };
                self.set_cols("grid.cells.resize_columns", runs, patch)?;
                Change::Content
            }
            G::AutoFitRows { from, to } => {
                let sid = self.sid()?;
                let runs = match (from, to) {
                    (Some(a), Some(b)) => self.resize_runs(*a, *b, true),
                    _ => self.row_runs(),
                };
                let default_px = self
                    .workbook
                    .sheet(sid)
                    .map_or(20.0, |s| s.rows().default_px());
                let mut cmds = Vec::new();
                for (at, count) in runs {
                    for row in at..at.saturating_add(count).min(at.saturating_add(5_000)) {
                        let need = self
                            .content_height(sid, row)
                            .unwrap_or(default_px)
                            .max(default_px);
                        let height = if (need - default_px).abs() < 0.75 {
                            Patch::Clear
                        } else {
                            Patch::Set(F64((px_to_pt(need) * 4.0).round() / 4.0))
                        };
                        cmds.push(kiln_sheet::Command::SetRowProps {
                            sheet: sid,
                            at: row,
                            count: 1,
                            patch: RowPatch {
                                height,
                                custom_height: Some(false),
                                hidden: Some(false),
                                ..RowPatch::default()
                            },
                        });
                    }
                }
                self.exec_all("grid.cells.autofit_rows", cmds)?;
                Change::Content
            }
            G::AutoFitColumns { from, to } => {
                let sid = self.sid()?;
                let runs = match (from, to) {
                    (Some(a), Some(b)) => self.resize_runs(*a, *b, false),
                    _ => self.col_runs(),
                };
                let mdw = self.mdw();
                let mut cmds = Vec::new();
                for (at, count) in runs {
                    for col in at..at.saturating_add(count).min(at.saturating_add(2_000)) {
                        // Columns without content keep their width (Excel leaves them).
                        let Some(need) = self.content_width(sid, col) else {
                            continue;
                        };
                        cmds.push(kiln_sheet::Command::SetColProps {
                            sheet: sid,
                            at: col,
                            count: 1,
                            patch: ColPatch {
                                width: Patch::Set(F64(px_to_col_width(
                                    need.ceil().min(1790.0),
                                    mdw,
                                ))),
                                custom_width: Some(true),
                                best_fit: Some(true),
                                hidden: Some(false),
                                ..ColPatch::default()
                            },
                        });
                    }
                }
                self.exec_all("grid.cells.autofit_columns", cmds)?;
                Change::Content
            }
            G::HideRows | G::UnhideRows => {
                let hide = matches!(cmd, G::HideRows);
                self.set_rows(
                    cmd.meta().id,
                    self.row_runs(),
                    RowPatch {
                        hidden: Some(hide),
                        ..RowPatch::default()
                    },
                )?;
                Change::Content
            }
            G::HideColumns | G::UnhideColumns => {
                let hide = matches!(cmd, G::HideColumns);
                self.set_cols(
                    cmd.meta().id,
                    self.col_runs(),
                    ColPatch {
                        hidden: Some(hide),
                        ..ColPatch::default()
                    },
                )?;
                Change::Content
            }
            G::ClearContents | G::Clear { .. } => {
                let sid = self.sid()?;
                let what = match cmd {
                    G::Clear { what } => match what {
                        GridClear::All => ClearWhat::All,
                        GridClear::Contents => ClearWhat::Contents,
                        GridClear::Formats => ClearWhat::Formats,
                        GridClear::Comments => ClearWhat::Notes,
                        GridClear::Hyperlinks => ClearWhat::Hyperlinks,
                    },
                    _ => ClearWhat::Contents,
                };
                let ranges = self.sel_ranges();
                self.exec(
                    cmd.meta().id,
                    kiln_sheet::Command::ClearRange {
                        sheet: sid,
                        ranges,
                        what,
                    },
                )?;
                Change::Content
            }
            G::FillDown => {
                self.fill_dir("grid.editing.fill_down", 0)?;
                Change::Content
            }
            G::FillRight => {
                self.fill_dir("grid.editing.fill_right", 1)?;
                Change::Content
            }
            G::FillUp => {
                self.fill_dir("grid.editing.fill_up", 2)?;
                Change::Content
            }
            G::FillLeft => {
                self.fill_dir("grid.editing.fill_left", 3)?;
                Change::Content
            }
            G::DragFill { row, col } => {
                let src = self.last_range()?;
                self.fill_preview =
                    fill_target(src, (*row).min(MAX_ROWS - 1), (*col).min(MAX_COLS - 1));
                Change::Selection
            }
            G::AutoFill { row, col, mode } => {
                self.fill_preview = None;
                let sid = self.sid()?;
                let src = self.last_range()?;
                let Some(target) =
                    fill_target(src, (*row).min(MAX_ROWS - 1), (*col).min(MAX_COLS - 1))
                else {
                    return Ok(Change::Selection);
                };
                self.exec(
                    "grid.editing.auto_fill",
                    kiln_sheet::Command::AutoFill {
                        sheet: sid,
                        source: src,
                        target,
                        mode: fill_mode(*mode),
                    },
                )?;
                let mut v = self.view();
                v.sel = Sel {
                    ranges: vec![target],
                    active: v.sel.active,
                };
                v.head = CellPos::new(target.r1, target.c1);
                self.set_view(v);
                Change::Content
            }
            G::CalculateNow => {
                self.workbook.recalc_all();
                Change::Selection
            }
            G::CalculateSheet => {
                let sid = self.sid()?;
                self.workbook.recalc_sheet(sid).map_err(super::model_err)?;
                Change::Selection
            }
            G::ShowFormulas => {
                self.set_sheet_view(|v| v.show_formulas = !v.show_formulas)?;
                Change::Selection
            }
            G::Gridlines => {
                self.set_sheet_view(|v| v.show_gridlines = !v.show_gridlines)?;
                Change::Selection
            }
            G::Headings => {
                self.set_sheet_view(|v| v.show_headings = !v.show_headings)?;
                Change::Selection
            }
            G::FreezePanes => {
                let frozen = self.geometry().freeze_rows > 0 || self.geometry().freeze_cols > 0;
                if frozen {
                    self.freeze(0, 0)?;
                } else {
                    let a = self.view().sel.active;
                    let (rows, cols) = if a.row == 0 && a.col == 0 {
                        // From A1 Excel freezes at the middle of the window.
                        let geo = self.geometry().clone();
                        let z = self.last_view.zoom.max(0.01);
                        let mid_y = self.last_view.height / z / 2.0;
                        let mid_x = (self.last_view.width / z - self.header_w(&geo)) / 2.0;
                        (
                            geo.rows.index_at(f64::from(mid_y.max(1.0))),
                            geo.cols.index_at(f64::from(mid_x.max(1.0))),
                        )
                    } else {
                        (a.row, a.col)
                    };
                    self.freeze(rows, cols)?;
                }
                Change::Selection
            }
            G::FreezeTopRow => {
                self.freeze(1, 0)?;
                Change::Selection
            }
            G::FreezeFirstColumn => {
                self.freeze(0, 1)?;
                Change::Selection
            }
            G::Split => {
                // A split is two scrolling regions; Kiln shows it as panes at the active cell
                // that stay put (the frozen-pane renderer), which is Excel's split with locked
                // scrolling. Toggling again removes it.
                let a = self.view().sel.active;
                let split = self
                    .workbook
                    .sheet(self.sid()?)
                    .and_then(|s| s.meta().view.split);
                if split.is_some() {
                    self.set_sheet_view(|v| {
                        v.split = None;
                        v.freeze = None;
                    })?;
                } else {
                    let geo = self.geometry().clone();
                    let x = geo.cols.offset(a.col) as u32;
                    let y = geo.rows.offset(a.row) as u32;
                    self.set_sheet_view(|v| {
                        v.split = Some((x, y));
                        v.freeze = Some(kiln_sheet::meta::Freeze {
                            rows: a.row,
                            cols: a.col,
                        });
                    })?;
                }
                if let Some(s) = &mut self.scene {
                    s.invalidate_all();
                }
                Change::Selection
            }
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} in Kiln Grid",
                    cmd.meta().id
                )));
            }
        };
        Ok(change)
    }

    /// The rows (or columns) a header-border drag resizes: the whole-row (column) selection
    /// when it holds the dragged one, else just those.
    fn resize_runs(&self, from: u32, to: u32, rows: bool) -> Vec<(u32, u32)> {
        let (a, b) = (from.min(to), from.max(to));
        let sel = self.sel_ranges();
        let in_sel = sel.iter().any(|r| {
            if rows {
                r.is_whole_rows() && r.r0 <= a && b <= r.r1
            } else {
                r.is_whole_cols() && r.c0 <= a && b <= r.c1
            }
        });
        if in_sel {
            if rows {
                self.row_runs()
            } else {
                self.col_runs()
            }
        } else {
            vec![(a, b - a + 1)]
        }
    }

    fn sheet_at(&self, index: u32) -> Result<SheetId> {
        self.workbook
            .sheets()
            .get(index as usize)
            .map(kiln_sheet::Sheet::id)
            .ok_or_else(|| KilnError::InvalidInput(format!("there is no sheet {index}")))
    }
}

struct CellPosLook {
    style: u32,
    width: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fill_handle_extends_along_the_longer_axis() {
        let src = Range::new(2, 2, 3, 3);
        assert_eq!(fill_target(src, 10, 4), Some(Range::new(2, 2, 10, 3)));
        assert_eq!(fill_target(src, 2, 9), Some(Range::new(2, 2, 3, 9)));
        assert_eq!(fill_target(src, 0, 2), Some(Range::new(0, 2, 3, 3)));
        assert_eq!(fill_target(src, 3, 3), None);
    }
}
