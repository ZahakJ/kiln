// SPDX-License-Identifier: AGPL-3.0-or-later
//! The surface gestures of Kiln Grid: Extend Selection (F8) and Add or Remove
//! Selection (Shift+F8), drag and drop of cells by the selection's border (Ctrl copies, Shift
//! inserts), and the error indicator's button and its Ignore Error and Convert to Number.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::GridHit;
use kiln_protocol::commands::grid_surface::{
    GridDragInfo, GridDragInsert, GridDragPhase, GridErrorCheck, GridErrorRule, GridPageBand,
    GridPageLine, GridPageMap, GridSelectMode, GridSurfaceState,
};
use kiln_scene::{Bounds, ColorRole, DrawOp, FillRule, Paint, PathData};
use kiln_sheet::commands::{CellInput, PasteOp, PasteWhat};
use kiln_sheet::meta::IgnoredError;
use kiln_sheet::{CellPos, CellValue, MAX_COLS, MAX_ROWS, Range, SheetId};
use kiln_sheet_layout::SheetGeometry;
use kiln_sheet_layout::paint::{ErrorRule, error_rule};

use super::GridDoc;
use super::view::Sel;
use crate::model::Change;

/// Excel's question before a drop overwrites cells.
pub(crate) const REPLACE_QUESTION: &str = "There's already data here. Do you want to replace it?";

/// A drag of cells in progress: the destination the outline shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DragPreview {
    /// Where the block lands.
    pub dest: Range,
    /// Copy (Ctrl).
    pub copy: bool,
    /// Insert-drag (Shift): the gridline the block goes in at.
    pub insert: Option<GridDragInsert>,
    /// Cells a drop would overwrite hold data.
    pub occupied: bool,
}

/// The drag of the surface's gestures, kept on the document.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SurfaceState {
    /// F8 / Shift+F8.
    pub mode: GridSelectMode,
    /// In Add mode, the ranges that stay selected while the arrows move a new cell; the ranges
    /// past it are the one being made.
    pub add_base: usize,
    /// A drag of cells.
    pub drag: Option<DragPreview>,
}

fn rule_of(r: ErrorRule) -> GridErrorRule {
    match r {
        ErrorRule::ErrorValue => GridErrorRule::ErrorValue,
        ErrorRule::NumberAsText => GridErrorRule::NumberAsText,
    }
}

/// The error button's rectangle beside a cell (sheet pixels): left of it, as Excel's.
fn button_rect(cell: Bounds) -> Bounds {
    let h = cell.h.clamp(14.0, 18.0);
    Bounds::new(cell.x - h - 2.0, cell.y, h, h)
}

impl GridDoc {
    /// Handles the surface commands, and the selection commands while F8 or Shift+F8 is on.
    pub(crate) fn surface_command(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        use GridCommand as G;
        match cmd {
            G::SelectionMode { mode } => {
                let len = self.views_sel_len();
                let s = &mut self.surface;
                s.mode = if *mode == s.mode {
                    GridSelectMode::Normal
                } else {
                    *mode
                };
                s.add_base = len;
                return Some(Ok(Change::Selection));
            }
            G::DragCells {
                phase,
                row,
                col,
                grab_row,
                grab_col,
                copy,
                insert,
                replace,
            } => {
                return Some(self.drag_cells(
                    *phase,
                    CellPos::new(*row, *col),
                    CellPos::new(*grab_row, *grab_col),
                    *copy,
                    *insert,
                    *replace,
                ));
            }
            G::IgnoreError => return Some(self.ignore_error()),
            G::ConvertToNumber => return Some(self.convert_to_number()),
            _ => {}
        }
        match self.surface.mode {
            GridSelectMode::Normal => None,
            // Esc, and starting an entry, end the mode (Excel).
            _ if matches!(cmd, G::CancelEdit | G::BeginEdit { .. } | G::EditCell) => {
                self.surface.mode = GridSelectMode::Normal;
                None
            }
            GridSelectMode::Extend => {
                let c = match cmd.clone() {
                    G::Move { dir, .. } => G::Move { dir, extend: true },
                    G::Jump { dir, .. } => G::Jump { dir, extend: true },
                    G::Page { dir, .. } => G::Page { dir, extend: true },
                    G::RowStart { .. } => G::RowStart { extend: true },
                    G::FirstCell { .. } => G::FirstCell { extend: true },
                    G::LastCell { .. } => G::LastCell { extend: true },
                    G::SelectCell { row, col, .. } => G::SelectCell {
                        row,
                        col,
                        extend: true,
                        add: false,
                    },
                    G::SelectRows { from, to, .. } => G::SelectRows {
                        from,
                        to,
                        extend: true,
                        add: false,
                    },
                    G::SelectColumns { from, to, .. } => G::SelectColumns {
                        from,
                        to,
                        extend: true,
                        add: false,
                    },
                    _ => return None,
                };
                Some(self.navigate(&c))
            }
            GridSelectMode::Add => {
                // A click adds a range (as Ctrl+click); the arrows then move that new cell.
                let c = match cmd.clone() {
                    G::SelectCell {
                        row, col, extend, ..
                    } => G::SelectCell {
                        row,
                        col,
                        extend,
                        add: !extend,
                    },
                    G::SelectRows {
                        from, to, extend, ..
                    } => G::SelectRows {
                        from,
                        to,
                        extend,
                        add: !extend,
                    },
                    G::SelectColumns {
                        from, to, extend, ..
                    } => G::SelectColumns {
                        from,
                        to,
                        extend,
                        add: !extend,
                    },
                    G::SelectRange { range, .. } => G::SelectRange { range, add: true },
                    _ => return None,
                };
                let added = !matches!(
                    c,
                    G::SelectCell { extend: true, .. }
                        | G::SelectRows { extend: true, .. }
                        | G::SelectColumns { extend: true, .. }
                );
                let r = self.navigate(&c);
                if added {
                    self.surface.add_base = self.views_sel_len().saturating_sub(1);
                }
                Some(r)
            }
        }
    }

    fn views_sel_len(&self) -> usize {
        self.view().sel.ranges.len()
    }

    /// In Add mode, where a new single cell goes: replacing the range being made, or after the
    /// ranges that stay (`None`: not in Add mode, the selection is replaced).
    pub(crate) fn add_mode_slot(&self) -> Option<usize> {
        (self.surface.mode == GridSelectMode::Add).then_some(self.surface.add_base)
    }

    /// The part of [`kiln_protocol::commands::GridState`] these gestures own.
    pub(crate) fn surface_state(&self) -> GridSurfaceState {
        GridSurfaceState {
            select_mode: self.surface.mode,
            drag: self.surface.drag.map(|d| GridDragInfo {
                target: if d.insert.is_some() || !d.dest.is_single_cell() {
                    d.dest.to_a1(false)
                } else {
                    kiln_sheet::address::cell_a1(d.dest.r0, d.dest.c0, false)
                },
                occupied: d.occupied,
                copy: d.copy,
                insert: d.insert,
            }),
            error: self.error_check(),
        }
    }

    // ── the error indicator ──────────────────────────────────────────────────────────────

    /// The active cell's error indicator, while not editing and no chart is selected.
    pub(crate) fn error_check(&self) -> Option<GridErrorCheck> {
        if self.edit.is_some() || self.charts.selected.is_some() {
            return None;
        }
        let sid = self.sid().ok()?;
        let a = self.view().sel.active;
        let sheet = self.workbook.sheet(sid)?;
        let rule = error_rule(&self.workbook, sheet, a.row, a.col)?;
        let error = self.workbook.display_text(sid, a.row, a.col);
        let (title, help) = match rule {
            ErrorRule::ErrorValue => super::features::audit::error_help(&error),
            ErrorRule::NumberAsText => (
                "Number Stored as Text",
                "The number in this cell is formatted as text or preceded by an apostrophe, so \
                 formulas may not treat it as a number.",
            ),
        };
        Some(GridErrorCheck {
            rule: rule_of(rule),
            error,
            title: title.into(),
            help: help.into(),
        })
    }

    fn error_button(&self, geo: &SheetGeometry) -> Option<Bounds> {
        self.error_check()?;
        let a = self.view().sel.active;
        let b = button_rect(geo.display_rect(a.row, a.col));
        // In column A it goes right of the cell.
        Some(if b.x < 0.0 {
            Bounds::new(
                b.x + b.w + 2.0 + geo.display_rect(a.row, a.col).w,
                b.y,
                b.w,
                b.h,
            )
        } else {
            b
        })
    }

    /// The error button: a warning diamond with an exclamation mark (sheet pixels).
    pub(crate) fn error_button_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Some(b) = self.error_button(geo) else {
            return;
        };
        out.push(DrawOp::FillRect {
            rect: b,
            paint: Paint::Role(ColorRole::Panel),
            radius: 2.0,
        });
        out.push(DrawOp::StrokeRect {
            rect: b.inflate(-0.5, -0.5),
            paint: Paint::Role(ColorRole::Line),
            width: 1.0,
            radius: 2.0,
        });
        let (cx, cy, r) = (b.x + b.w / 2.0, b.y + b.h / 2.0, b.w * 0.36);
        let mut p = PathData::new();
        p.move_to(cx, cy - r)
            .line_to(cx + r, cy)
            .line_to(cx, cy + r)
            .line_to(cx - r, cy)
            .close();
        out.push(DrawOp::Path {
            path: p,
            fill: Some(Paint::Rgba(0xF2_C2_30_FF)),
            stroke: None,
            rule: FillRule::NonZero,
        });
        let ink = Paint::Rgba(0x3A_2E_00_FF);
        out.push(DrawOp::FillRect {
            rect: Bounds::new(cx - 0.75, cy - r * 0.55, 1.5, r * 0.75),
            paint: ink,
            radius: 0.0,
        });
        out.push(DrawOp::FillRect {
            rect: Bounds::new(cx - 0.75, cy + r * 0.35, 1.5, 1.5),
            paint: ink,
            radius: 0.0,
        });
    }

    pub(crate) fn error_button_hit(&self, x: f32, y: f32) -> Option<GridHit> {
        self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
            self.error_button(geo)?
                .contains(sx, sy)
                .then_some(GridHit::ErrorButton)
        })
    }

    /// The selection's flagged cells and their rules.
    fn flagged(&self, sid: SheetId) -> Vec<(u32, u32, ErrorRule)> {
        let Some(sheet) = self.workbook.sheet(sid) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in &self.view().sel.ranges {
            for (row, col, _) in sheet.cells().iter_range(*r) {
                if let Some(rule) = error_rule(&self.workbook, sheet, row, col) {
                    out.push((row, col, rule));
                }
            }
        }
        out
    }

    /// Ignore Error: the selection's flagged cells stop showing the triangle (saved in the file
    /// as `ignoredErrors`).
    fn ignore_error(&mut self) -> Result<Change> {
        let sid = self.sid()?;
        let flagged = self.flagged(sid);
        if flagged.is_empty() {
            return Ok(Change::None);
        }
        let mut list = self
            .workbook
            .sheet(sid)
            .map(|s| s.meta().ignored_errors.clone())
            .unwrap_or_default();
        for rule in [ErrorRule::ErrorValue, ErrorRule::NumberAsText] {
            let ranges: Vec<Range> = flagged
                .iter()
                .filter(|f| f.2 == rule)
                .map(|&(r, c, _)| Range::cell(r, c))
                .collect();
            if ranges.is_empty() {
                continue;
            }
            list.push(IgnoredError {
                ranges,
                eval_error: rule == ErrorRule::ErrorValue,
                number_as_text: rule == ErrorRule::NumberAsText,
                other: Vec::new(),
            });
        }
        self.exec(
            "grid.formulas.ignore_error",
            kiln_sheet::Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(kiln_sheet::commands::MetaField::IgnoredErrors(list)),
            },
        )?;
        Ok(Change::Content)
    }

    /// Convert to Number: the selection's numbers stored as text become numbers.
    fn convert_to_number(&mut self) -> Result<Change> {
        let sid = self.sid()?;
        let cells: Vec<CellInput> = self
            .flagged(sid)
            .into_iter()
            .filter(|f| f.2 == ErrorRule::NumberAsText)
            .filter_map(|(row, col, _)| match self.workbook.value(sid, row, col) {
                CellValue::Text(t) => t
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(|n| CellInput::value(row, col, CellValue::Number(n))),
                _ => None,
            })
            .collect();
        if cells.is_empty() {
            return Ok(Change::None);
        }
        self.exec(
            "grid.formulas.convert_to_number",
            kiln_sheet::Command::SetCells { sheet: sid, cells },
        )?;
        Ok(Change::Content)
    }

    // ── drag and drop of cells ───────────────────────────────────────────────────────────

    /// The selection's border under a surface point (a drag moves the cells): the selection's
    /// cell nearest the point. Only a single range, while not editing.
    pub(crate) fn selection_border_hit(
        &self,
        geo: &SheetGeometry,
        x: f32,
        y: f32,
    ) -> Option<GridHit> {
        if self.edit.is_some() || self.charts.selected.is_some() {
            return None;
        }
        let v = self.view();
        let [sel] = v.sel.ranges.as_slice() else {
            return None;
        };
        if sel.is_whole_rows() || sel.is_whole_cols() {
            return None;
        }
        let hw = self.header_w(geo);
        let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
        let z = self.last_view.zoom.max(0.01);
        let r = geo.range_rect(sel);
        let reach = 3.0 / z;
        let inside_x = sx >= r.x - reach && sx <= r.right() + reach;
        let inside_y = sy >= r.y - reach && sy <= r.bottom() + reach;
        let near_v = (sx - r.x).abs() <= reach || (sx - r.right()).abs() <= reach;
        let near_h = (sy - r.y).abs() <= reach || (sy - r.bottom()).abs() <= reach;
        if !((near_v && inside_y) || (near_h && inside_x)) {
            return None;
        }
        let row = geo
            .rows
            .index_at(f64::from(sy.clamp(r.y, r.bottom() - 0.01)))
            .clamp(sel.r0, sel.r1);
        let col = geo
            .cols
            .index_at(f64::from(sx.clamp(r.x, r.right() - 0.01)))
            .clamp(sel.c0, sel.c1);
        Some(GridHit::SelectionBorder { row, col })
    }

    /// The destination of a drag: the block moved by the pointer's offset from where it took
    /// hold, kept on the sheet; with Shift, the gridline it goes in at.
    fn drag_target(
        src: Range,
        at: CellPos,
        grab: CellPos,
        insert: bool,
    ) -> (Range, Option<GridDragInsert>) {
        let (h, w) = (src.height(), src.width());
        let dr = i64::from(at.row) - i64::from(grab.row.clamp(src.r0, src.r1));
        let dc = i64::from(at.col) - i64::from(grab.col.clamp(src.c0, src.c1));
        let place = |from: u32, d: i64, n: u32, max: u32| -> u32 {
            (i64::from(from) + d).clamp(0, i64::from(max - n)) as u32
        };
        let (dr, dc, kind) = if !insert {
            (dr, dc, None)
        } else if dr.abs() >= dc.abs() {
            (dr, 0, Some(GridDragInsert::Rows))
        } else {
            (0, dc, Some(GridDragInsert::Columns))
        };
        let r0 = place(src.r0, dr, h, MAX_ROWS);
        let c0 = place(src.c0, dc, w, MAX_COLS);
        (Range::new(r0, c0, r0 + h - 1, c0 + w - 1), kind)
    }

    fn occupied(&self, sid: SheetId, src: Range, dest: Range, copy: bool, insert: bool) -> bool {
        if insert {
            return false;
        }
        let Some(sheet) = self.workbook.sheet(sid) else {
            return false;
        };
        sheet
            .cells()
            .iter_range(dest)
            .any(|(r, c, d)| d.has_content() && (copy || !src.contains(r, c)))
    }

    #[allow(clippy::fn_params_excessive_bools)]
    fn drag_cells(
        &mut self,
        phase: GridDragPhase,
        at: CellPos,
        grab: CellPos,
        copy: bool,
        insert: bool,
        replace: bool,
    ) -> Result<Change> {
        if phase == GridDragPhase::Cancel {
            self.surface.drag = None;
            return Ok(Change::Selection);
        }
        let sid = self.sid()?;
        let v = self.view();
        let [src] = v.sel.ranges.as_slice() else {
            return Err(KilnError::InvalidInput(
                "That command cannot be used on multiple selections.".into(),
            ));
        };
        let src = *src;
        let at = CellPos::new(at.row.min(MAX_ROWS - 1), at.col.min(MAX_COLS - 1));
        let (dest, kind) = Self::drag_target(src, at, grab, insert);
        let occupied = self.occupied(sid, src, dest, copy, insert);
        if phase == GridDragPhase::Preview {
            self.surface.drag = Some(DragPreview {
                dest,
                copy,
                insert: kind,
                occupied,
            });
            return Ok(Change::Selection);
        }
        self.surface.drag = None;
        if dest == src && !copy {
            return Ok(Change::Selection);
        }
        if occupied && !replace {
            return Err(KilnError::InvalidInput(REPLACE_QUESTION.into()));
        }
        let label = "grid.editing.drag_cells";
        let mut landed = dest;
        let cmds = match kind {
            None if copy => vec![kiln_sheet::Command::CopyRange {
                sheet: sid,
                range: src,
                to_sheet: Some(sid),
                to: dest,
                what: PasteWhat::All,
                op: PasteOp::None,
                transpose: false,
                skip_blanks: false,
            }],
            None => vec![kiln_sheet::Command::MoveRange {
                sheet: sid,
                range: src,
                to_sheet: None,
                to: CellPos::new(dest.r0, dest.c0),
            }],
            Some(k) => match self.insert_drag(sid, src, dest, k, copy)? {
                Some((c, at)) => {
                    landed = at;
                    c
                }
                None => return Ok(Change::Selection),
            },
        };
        self.exec_all(label, cmds)?;
        let mut v = self.view();
        v.sel = Sel {
            ranges: vec![landed],
            active: CellPos::new(landed.r0, landed.c0),
        };
        v.head = CellPos::new(landed.r1, landed.c1);
        self.set_view(v);
        self.copy = None;
        Ok(Change::Content)
    }

    /// The model commands of an insert-drag: the cells at the gridline shift to make room, the
    /// block moves (or is copied) in, and a moved block's old place closes up, so the block ends
    /// at `dest`. `None` when it would go where it is.
    fn insert_drag(
        &self,
        sid: SheetId,
        src: Range,
        dest: Range,
        kind: GridDragInsert,
        copy: bool,
    ) -> Result<Option<(Vec<kiln_sheet::Command>, Range)>> {
        let rows = kind == GridDragInsert::Rows;
        // Positions along the axis that moves, and the span across it.
        let (s0, s1, d0) = if rows {
            (src.r0, src.r1, dest.r0)
        } else {
            (src.c0, src.c1, dest.c0)
        };
        let n = s1 - s0 + 1;
        // The gridline the block goes in at (the top or left of the preview), in the sheet as
        // it is now.
        let line = d0;
        if !copy && (s0..=s1 + 1).contains(&line) {
            return Ok(None);
        }
        let max = if rows { MAX_ROWS } else { MAX_COLS };
        let last = if rows {
            self.last_row_in(sid, src.c0, src.c1)
        } else {
            self.last_col_in(sid, src.r0, src.r1)
        }
        .unwrap_or(line);
        if last.max(line) + n >= max {
            return Err(KilnError::InvalidInput(
                "inserting would push cells off the end of the sheet".into(),
            ));
        }
        // A block of positions `a..=b` along the axis, across the source's span.
        let block = |a: u32, b: u32| {
            if rows {
                Range::new(a, src.c0, b, src.c1)
            } else {
                Range::new(src.r0, a, src.r1, b)
            }
        };
        let at = |p: u32| {
            if rows {
                CellPos::new(p, src.c0)
            } else {
                CellPos::new(src.r0, p)
            }
        };
        let mv = |range: Range, to: CellPos| kiln_sheet::Command::MoveRange {
            sheet: sid,
            range,
            to_sheet: None,
            to,
        };
        let mut cmds = Vec::new();
        // 1. Make room at the line.
        if last >= line {
            cmds.push(mv(block(line, last), at(line + n)));
        }
        // The source after that shift.
        let (m0, m1) = if s0 >= line {
            (s0 + n, s1 + n)
        } else {
            (s0, s1)
        };
        let moved = block(m0, m1);
        // 2. The block goes in.
        if copy {
            cmds.push(kiln_sheet::Command::CopyRange {
                sheet: sid,
                range: moved,
                to_sheet: Some(sid),
                to: block(line, line + n - 1),
                what: PasteWhat::All,
                op: PasteOp::None,
                transpose: false,
                skip_blanks: false,
            });
        } else {
            cmds.push(mv(moved, at(line)));
            // 3. Its old place closes up.
            let end = last.max(line) + n;
            if end > m1 {
                cmds.push(mv(block(m1 + 1, end), at(m0)));
            }
        }
        // Where the block ends: at the line, or a block higher once its old place closed up.
        let start = if !copy && line > s1 { line - n } else { line };
        Ok(Some((cmds, block(start, start + n - 1))))
    }

    /// The outline of a drag of cells (sheet pixels): Excel's grey frame where the block goes,
    /// or, for an insert-drag, the thick line it goes in at.
    pub(crate) fn drag_preview_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Some(d) = self.surface.drag else {
            return;
        };
        let r = geo.range_rect(&d.dest);
        let paint = Paint::Role(ColorRole::Muted);
        match d.insert {
            None => out.push(DrawOp::StrokeRect {
                rect: r.inflate(-1.0, -1.0),
                paint,
                width: 2.5,
                radius: 0.0,
            }),
            Some(GridDragInsert::Rows) => out.push(DrawOp::FillRect {
                rect: Bounds::new(r.x, r.y - 1.5, r.w, 3.0),
                paint,
                radius: 0.0,
            }),
            Some(GridDragInsert::Columns) => out.push(DrawOp::FillRect {
                rect: Bounds::new(r.x - 1.5, r.y, 3.0, r.h),
                paint,
                radius: 0.0,
            }),
        }
    }

    // ── Page Layout view ─────────────────────────────────────────────────────────────────

    /// Where the cells of printed page `page` (0-based) of the active sheet are, page px, as
    /// File ▸ Print lays them out (Page Layout view selects and edits cells on the page).
    pub(crate) fn page_map(&self, page: u32) -> Option<GridPageMap> {
        let mut pages = kiln_sheet_layout::print::paginate(&self.workbook, self.active)?;
        if pages.pages.is_empty() {
            // An empty sheet: one blank page from A1, whose cells can be typed into.
            pages.pages.push(kiln_sheet_layout::print::PageArea {
                rows: (0, 0),
                cols: (0, 0),
                title_rows: None,
                title_cols: None,
            });
            pages.areas = vec![Range::cell(0, 0)];
        }
        let sheet = self.workbook.sheets().get(self.active)?;
        let geo = kiln_sheet_layout::render::geometry_of(sheet, false);
        let bands = kiln_sheet_layout::print::page_bands(&pages, &geo, page as usize, true)?;
        let line = |&(index, at, size): &(u32, f32, f32)| GridPageLine { index, at, size };
        Some(GridPageMap {
            width: pages.width,
            height: pages.height,
            bands: bands
                .iter()
                .map(|b| GridPageBand {
                    x: b.x,
                    y: b.y,
                    rows: b.rows.iter().map(line).collect(),
                    cols: b.cols.iter().map(line).collect(),
                })
                .collect(),
        })
    }
}
