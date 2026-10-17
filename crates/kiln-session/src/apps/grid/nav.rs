// SPDX-License-Identifier: AGPL-3.0-or-later
//! Navigation and selection (PLAN.md §1.7, Grid column): arrows, Ctrl+arrows to the edges of
//! the data, Home, Ctrl+Home/End, Page Up/Down, Enter and Tab inside a selection, clicks with
//! Shift and Ctrl, whole rows and columns, Ctrl+A (current region, then the sheet), Go To and a
//! basic Find. Every move brings the moving cell into view (a scroll request the surface
//! follows) and treats merged cells as one.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::{GridCommand, GridDirection, GridReveal};
use kiln_sheet::address::parse_cell_a1;
use kiln_sheet::{CellPos, MAX_COLS, MAX_ROWS, Range, SheetId};

use super::GridDoc;
use super::editor::step;
use super::view::{Sel, ViewState};
use crate::model::Change;

impl GridDoc {
    /// Whether a cell shows something (a value, a formula, a spilled result).
    pub(crate) fn has_content(&self, sheet: SheetId, row: u32, col: u32) -> bool {
        if let Some(d) = self
            .workbook
            .sheet(sheet)
            .and_then(|s| s.cells().get(row, col))
        {
            return d.has_content();
        }
        !self.workbook.value(sheet, row, col).is_blank()
    }

    /// Ctrl+arrow from `from`: to the end of the run of filled cells, or across the gap to the
    /// next filled cell, or to the edge of the sheet.
    pub(crate) fn jump_from(&self, from: CellPos, dir: GridDirection) -> CellPos {
        let Ok(sid) = self.sid() else {
            return from;
        };
        let Some(sheet) = self.workbook.sheet(sid) else {
            return from;
        };
        let filled = |r: u32, c: u32| self.has_content(sid, r, c);
        let (dr, dc): (i64, i64) = match dir {
            GridDirection::Up => (-1, 0),
            GridDirection::Down => (1, 0),
            GridDirection::Left => (0, -1),
            GridDirection::Right => (0, 1),
        };
        let inside = |r: i64, c: i64| {
            (0..i64::from(MAX_ROWS)).contains(&r) && (0..i64::from(MAX_COLS)).contains(&c)
        };
        let (r0, c0) = (i64::from(from.row), i64::from(from.col));
        if !inside(r0 + dr, c0 + dc) {
            return from;
        }
        let at = |r: i64, c: i64| CellPos::new(r as u32, c as u32);
        let here = filled(from.row, from.col);
        let next = filled((r0 + dr) as u32, (c0 + dc) as u32);
        if here && next {
            // Walk the run.
            let (mut r, mut c) = (r0 + dr, c0 + dc);
            while inside(r + dr, c + dc) && filled((r + dr) as u32, (c + dc) as u32) {
                r += dr;
                c += dc;
            }
            return at(r, c);
        }
        // Across the gap: the next filled cell (from the stored cells, so a million empty rows
        // cost nothing), else the edge.
        match dir {
            GridDirection::Right | GridDirection::Left => {
                let row = from.row;
                let stored = sheet.cells().chunk(row).and_then(|ch| {
                    if dir == GridDirection::Right {
                        let i = ch.lower_bound(from.col + 1);
                        (i..ch.len())
                            .map(|k| ch.at(k))
                            .find(|(_, d)| d.has_content())
                            .map(|(c, _)| c)
                    } else {
                        let i = ch.lower_bound(from.col);
                        (0..i)
                            .rev()
                            .map(|k| ch.at(k))
                            .find(|(_, d)| d.has_content())
                            .map(|(c, _)| c)
                    }
                });
                // Spilled results are not stored: look at the cells before the stored one.
                let spill = (1..=64i64)
                    .map(|k| c0 + dc * k)
                    .take_while(|&c| inside(r0, c))
                    .find(|&c| {
                        stored.is_none_or(|s| {
                            if dc > 0 {
                                c < i64::from(s)
                            } else {
                                c > i64::from(s)
                            }
                        }) && filled(row, c as u32)
                    });
                match (spill, stored) {
                    (Some(c), _) => CellPos::new(row, c as u32),
                    (None, Some(c)) => CellPos::new(row, c),
                    (None, None) => CellPos::new(row, if dc > 0 { MAX_COLS - 1 } else { 0 }),
                }
            }
            GridDirection::Down | GridDirection::Up => {
                let col = from.col;
                let rows = sheet.cells().rows();
                let stored = if dir == GridDirection::Down {
                    let i = rows.partition_point(|&r| r <= from.row);
                    rows[i..]
                        .iter()
                        .copied()
                        .find(|&r| sheet.cells().get(r, col).is_some_and(|d| d.has_content()))
                } else {
                    let i = rows.partition_point(|&r| r < from.row);
                    rows[..i]
                        .iter()
                        .rev()
                        .copied()
                        .find(|&r| sheet.cells().get(r, col).is_some_and(|d| d.has_content()))
                };
                let spill = (1..=64i64)
                    .map(|k| r0 + dr * k)
                    .take_while(|&r| inside(r, c0))
                    .find(|&r| {
                        stored.is_none_or(|s| {
                            if dr > 0 {
                                r < i64::from(s)
                            } else {
                                r > i64::from(s)
                            }
                        }) && filled(r as u32, col)
                    });
                match (spill, stored) {
                    (Some(r), _) => CellPos::new(r as u32, col),
                    (None, Some(r)) => CellPos::new(r, col),
                    (None, None) => CellPos::new(if dr > 0 { MAX_ROWS - 1 } else { 0 }, col),
                }
            }
        }
    }

    /// Sets the selection to the range from the active cell to `head` (or a new single cell),
    /// growing it to whole merges, and brings `head` into view.
    pub(crate) fn select_to(&mut self, head: CellPos, extend: bool, add: bool) {
        let mut v = self.view();
        let geo = self.geometry().clone();
        if extend {
            let a = v.sel.active;
            let r = geo.expand_to_merges(Range::new(a.row, a.col, head.row, head.col));
            if let Some(last) = v.sel.ranges.last_mut() {
                *last = r;
            } else {
                v.sel.ranges.push(r);
            }
            v.head = head;
        } else {
            let (active, r) = match geo.merge_at(head.row, head.col) {
                Some(m) => (CellPos::new(m.r0, m.c0), m),
                None => (head, Range::cell(head.row, head.col)),
            };
            if add {
                v.sel.ranges.push(r);
            } else {
                v.sel.ranges = vec![r];
            }
            v.sel.active = active;
            v.head = head;
        }
        self.set_view(v);
        self.reveal_cell(head);
    }

    /// Asks the surface to scroll so `cell` is visible.
    pub(crate) fn reveal_cell(&mut self, cell: CellPos) {
        let geo = self.geometry().clone();
        let hw = self.header_w(&geo);
        if self.last_view.width <= 0.0 {
            return;
        }
        let (x, y) = geo.scroll_to_cell(&self.last_view, cell.row, cell.col, hw);
        let (cx, cy) = geo.clamp_scroll(self.last_view.scroll_x, self.last_view.scroll_y);
        if (x - cx).abs() < 0.5 && (y - cy).abs() < 0.5 {
            return;
        }
        let seq = self.reveal.map_or(1, |r| r.seq.wrapping_add(1));
        self.reveal = Some(GridReveal { seq, x, y });
        // The next frame shows the new position even before the surface scrolls.
        self.last_view.scroll_x = x;
        self.last_view.scroll_y = y;
    }

    /// One cell in a direction (Shift extends from the moving corner); merged cells are crossed
    /// as one.
    pub(crate) fn move_active(&mut self, dir: GridDirection, extend: bool) {
        let v = self.view();
        let geo = self.geometry().clone();
        let from = if extend { v.head } else { v.sel.active };
        // Leave a merge by its far edge.
        let from = match (geo.merge_at(from.row, from.col), dir) {
            (Some(m), GridDirection::Down) => CellPos::new(m.r1, from.col),
            (Some(m), GridDirection::Right) => CellPos::new(from.row, m.c1),
            (Some(m), GridDirection::Up) => CellPos::new(m.r0, from.col),
            (Some(m), GridDirection::Left) => CellPos::new(from.row, m.c0),
            (None, _) => from,
        };
        let (r, c) = step(&geo, from, dir);
        self.select_to(CellPos::new(r, c), extend, false);
    }

    /// One cell on from a single-cell selection after an entry or Enter/Tab: Tab remembers the
    /// column a run of entries started in, and Enter goes back to it on the next row (Excel).
    pub(crate) fn step_single(&mut self, dir: GridDirection) {
        let a = self.view().sel.active;
        match dir {
            GridDirection::Right => {
                if self.tab_origin.is_none_or(|o| o.row != a.row) {
                    self.tab_origin = Some(a);
                }
                self.move_active(dir, false);
            }
            GridDirection::Down => match self.tab_origin.take().filter(|o| o.row == a.row) {
                Some(o) => {
                    self.move_active(dir, false);
                    let row = self.view().sel.active.row;
                    self.select_to(CellPos::new(row, o.col), false, false);
                }
                None => self.move_active(dir, false),
            },
            _ => {
                self.tab_origin = None;
                self.move_active(dir, false);
            }
        }
    }

    /// Enter/Tab through the cells of a multi-cell selection, or one cell when it is a single
    /// cell.
    pub(crate) fn advance(&mut self, backward: bool, across: bool) {
        let mut v = self.view();
        let multi =
            v.sel.ranges.len() > 1 || v.sel.ranges.last().is_some_and(|r| !r.is_single_cell());
        if !multi {
            let dir = match (across, backward) {
                (false, false) => GridDirection::Down,
                (false, true) => GridDirection::Up,
                (true, false) => GridDirection::Right,
                (true, true) => GridDirection::Left,
            };
            self.step_single(dir);
            return;
        }
        let geo = self.geometry().clone();
        let Some(mut idx) = v
            .sel
            .ranges
            .iter()
            .rposition(|r| r.contains(v.sel.active.row, v.sel.active.col))
        else {
            return;
        };
        let mut a = v.sel.active;
        // Skip the hidden cells and the non-anchor cells of merges.
        for _ in 0..4 {
            let r = v.sel.ranges[idx];
            let next = if across {
                if !backward {
                    if a.col < r.c1 {
                        Some(CellPos::new(a.row, a.col + 1))
                    } else if a.row < r.r1 {
                        Some(CellPos::new(a.row + 1, r.c0))
                    } else {
                        None
                    }
                } else if a.col > r.c0 {
                    Some(CellPos::new(a.row, a.col - 1))
                } else if a.row > r.r0 {
                    Some(CellPos::new(a.row - 1, r.c1))
                } else {
                    None
                }
            } else if !backward {
                if a.row < r.r1 {
                    Some(CellPos::new(a.row + 1, a.col))
                } else if a.col < r.c1 {
                    Some(CellPos::new(r.r0, a.col + 1))
                } else {
                    None
                }
            } else if a.row > r.r0 {
                Some(CellPos::new(a.row - 1, a.col))
            } else if a.col > r.c0 {
                Some(CellPos::new(r.r1, a.col - 1))
            } else {
                None
            };
            a = match next {
                Some(p) => p,
                None => {
                    // The next range (wrapping).
                    let n = v.sel.ranges.len();
                    idx = if backward {
                        (idx + n - 1) % n
                    } else {
                        (idx + 1) % n
                    };
                    let r = v.sel.ranges[idx];
                    if backward {
                        CellPos::new(r.r1, r.c1)
                    } else {
                        CellPos::new(r.r0, r.c0)
                    }
                }
            };
            let hidden = geo.rows.is_hidden(a.row) || geo.cols.is_hidden(a.col);
            let covered = geo
                .merge_at(a.row, a.col)
                .is_some_and(|m| m.r0 != a.row || m.c0 != a.col);
            if !hidden && !covered {
                break;
            }
        }
        // The range holding the active cell goes last (Excel keeps the selection order for
        // painting; the active range is the one Shift extends).
        v.sel.active = a;
        let r = v.sel.ranges.remove(idx);
        v.sel.ranges.push(r);
        v.head = a;
        self.set_view(v);
        self.reveal_cell(a);
    }

    /// Selects a cell like a click.
    pub(crate) fn select_cell(&mut self, row: u32, col: u32, extend: bool, add: bool) {
        let row = row.min(MAX_ROWS - 1);
        let col = col.min(MAX_COLS - 1);
        self.select_to(CellPos::new(row, col), extend, add && !extend);
    }

    /// The current region around a cell (Ctrl+A): the block of filled cells reached through
    /// filled neighbours, as Excel's CurrentRegion.
    pub(crate) fn current_region(&self, at: CellPos) -> Option<Range> {
        let sid = self.sid().ok()?;
        let filled = |r: u32, c: u32| self.has_content(sid, r, c);
        let mut reg = Range::cell(at.row, at.col);
        // Grow while a border row or column (including diagonals) holds something.
        for _ in 0..10_000 {
            let mut grown = reg;
            let top = reg.r0.checked_sub(1);
            let bottom = (reg.r1 + 1 < MAX_ROWS).then_some(reg.r1 + 1);
            let left = reg.c0.checked_sub(1);
            let right = (reg.c1 + 1 < MAX_COLS).then_some(reg.c1 + 1);
            let c_lo = left.unwrap_or(reg.c0);
            let c_hi = right.unwrap_or(reg.c1);
            let r_lo = top.unwrap_or(reg.r0);
            let r_hi = bottom.unwrap_or(reg.r1);
            if let Some(t) = top
                && (c_lo..=c_hi).any(|c| filled(t, c))
            {
                grown.r0 = t;
            }
            if let Some(b) = bottom
                && (c_lo..=c_hi).any(|c| filled(b, c))
            {
                grown.r1 = b;
            }
            if let Some(l) = left
                && (r_lo..=r_hi).any(|r| filled(r, l))
            {
                grown.c0 = l;
            }
            if let Some(rg) = right
                && (r_lo..=r_hi).any(|r| filled(r, rg))
            {
                grown.c1 = rg;
            }
            if grown == reg {
                break;
            }
            reg = grown;
        }
        (!reg.is_single_cell() || filled(at.row, at.col)).then_some(reg)
    }

    /// The name box: a new name for the selection (workbook scope, absolute references), as
    /// Excel defines one when a name that does not exist is typed there.
    fn define_name_for_selection(&mut self, name: &str) -> Result<()> {
        let (_, sheet) = self
            .active_sheet()
            .ok_or_else(|| KilnError::InvalidInput("there is no sheet".into()))?;
        let sheet_name = super::editor::quote_sheet_name(sheet.name());
        let refs: Vec<String> = self
            .view()
            .sel
            .ranges
            .iter()
            .map(|r| format!("{sheet_name}!{}", r.to_a1(true)))
            .collect();
        self.exec(
            "grid.nav.go_to",
            kiln_sheet::Command::DefineName {
                name: name.to_owned(),
                formula: refs.join(","),
                scope: None,
                hidden: false,
                comment: None,
            },
        )
        .map_err(|_| {
            KilnError::InvalidInput(format!(
                "“{name}” is not a cell reference, and it cannot be a name: names start with a \
                 letter or an underscore and do not look like a cell"
            ))
        })?;
        Ok(())
    }

    /// A reference typed in the name box or Go To: a cell or range, on this or another sheet, or
    /// a defined name.
    fn go_to(&mut self, reference: &str, define: bool) -> Result<()> {
        let text = reference.trim();
        if text.is_empty() {
            return Err(KilnError::InvalidInput(
                "type a cell reference or a name".into(),
            ));
        }
        let (sheet_part, addr) = match text.rsplit_once('!') {
            Some((s, a)) => (Some(s.trim_matches('\'').replace("''", "'")), a.to_owned()),
            None => (None, text.to_owned()),
        };
        let resolved = |addr: &str| -> Option<Range> {
            Range::parse_a1(&addr.replace('$', "")).or_else(|| {
                parse_cell_a1(&addr.replace('$', "")).map(|p| Range::cell(p.row, p.col))
            })
        };
        let (sheet_name, range) = match resolved(&addr) {
            Some(r) => (sheet_part, r),
            None => {
                let sid = self.sid()?;
                if define && self.workbook.resolve_name(text, sid).is_none() {
                    return self.define_name_for_selection(text);
                }
                let name = self.workbook.resolve_name(text, sid).ok_or_else(|| {
                    KilnError::InvalidInput(format!(
                        "“{text}” is not a cell reference or a name in this workbook"
                    ))
                })?;
                let formula = name.formula.clone();
                let (s, a) = match formula.rsplit_once('!') {
                    Some((s, a)) => (Some(s.trim_matches('\'').replace("''", "'")), a.to_owned()),
                    None => (None, formula),
                };
                let r = resolved(&a).ok_or_else(|| {
                    KilnError::InvalidInput(format!("“{text}” does not refer to cells"))
                })?;
                (s, r)
            }
        };
        if let Some(name) = sheet_name {
            let idx = self
                .workbook
                .sheets()
                .iter()
                .position(|s| s.name().eq_ignore_ascii_case(&name))
                .ok_or_else(|| KilnError::InvalidInput(format!("there is no sheet “{name}”")))?;
            self.activate(idx);
        }
        let mut v = self.view();
        v.sel = Sel {
            ranges: vec![range],
            active: CellPos::new(range.r0, range.c0),
        };
        v.head = CellPos::new(range.r1, range.c1);
        self.set_view(v);
        self.reveal_cell(CellPos::new(range.r0, range.c0));
        Ok(())
    }

    /// Shows another sheet.
    pub(crate) fn activate(&mut self, index: usize) {
        if index < self.workbook.sheets().len() && index != self.active {
            self.active = index;
            self.geo = None;
            self.edit = None;
            self.fill_preview = None;
            if let Some(s) = &mut self.scene {
                s.invalidate_all();
            }
            self.selection_epoch += 1;
            // The new sheet opens at its top-left cell.
            self.last_view.scroll_x = 0.0;
            self.last_view.scroll_y = 0.0;
            let seq = self.reveal.map_or(1, |r| r.seq.wrapping_add(1));
            let geo = self.geometry().clone();
            let (x, y) = geo.clamp_scroll(0.0, 0.0);
            self.reveal = Some(GridReveal { seq, x, y });
        }
    }

    /// Basic Find: the next cell (row by row from the active cell, wrapping) whose text contains
    /// (or is) the text.
    fn find_next(
        &mut self,
        text: &str,
        match_case: bool,
        whole: bool,
        backward: bool,
        in_formulas: bool,
    ) -> Result<bool> {
        if text.is_empty() {
            return Ok(false);
        }
        let sid = self.sid()?;
        let Some(sheet) = self.workbook.sheet(sid) else {
            return Ok(false);
        };
        let needle = if match_case {
            text.to_owned()
        } else {
            text.to_lowercase()
        };
        let matches = |hay: String| {
            let hay = if match_case { hay } else { hay.to_lowercase() };
            if whole {
                hay == needle
            } else {
                hay.contains(&needle)
            }
        };
        let mut cells: Vec<(u32, u32)> = sheet
            .cells()
            .iter()
            .filter(|(_, _, d)| d.has_content())
            .map(|(r, c, _)| (r, c))
            .collect();
        cells.sort_unstable();
        let at = self.view().sel.active;
        let pos = cells.partition_point(|&p| p <= (at.row, at.col));
        let order: Vec<(u32, u32)> = if backward {
            let before = pos.saturating_sub(usize::from(
                cells.get(pos.wrapping_sub(1)) == Some(&(at.row, at.col)),
            ));
            cells[..before]
                .iter()
                .rev()
                .chain(cells[before..].iter().rev())
                .copied()
                .collect()
        } else {
            cells[pos..]
                .iter()
                .chain(cells[..pos].iter())
                .copied()
                .collect()
        };
        for (r, c) in order {
            let hay = if in_formulas {
                self.edit_text(sid, r, c)
            } else {
                self.workbook.display_text(sid, r, c)
            };
            if matches(hay) {
                self.select_cell(r, c, false, false);
                return Ok(true);
            }
        }
        Err(KilnError::InvalidInput(format!("Kiln can't find “{text}”")))
    }

    /// The navigation commands.
    pub(crate) fn navigate(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        self.fill_preview = None;
        match cmd {
            G::Move { dir, extend } => self.move_active(*dir, *extend),
            G::Jump { dir, extend } => {
                let v = self.view();
                let from = if *extend { v.head } else { v.sel.active };
                let to = self.jump_from(from, *dir);
                self.select_to(to, *extend, false);
            }
            G::Page { dir, extend } => {
                let geo = self.geometry().clone();
                let hw = self.header_w(&geo);
                let v = self.view();
                let from = if *extend { v.head } else { v.sel.active };
                let n = i64::from(geo.rows_per_page(&self.last_view, hw));
                let to = match dir {
                    GridDirection::Down => CellPos::new(geo.rows.step(from.row, n), from.col),
                    GridDirection::Up => CellPos::new(geo.rows.step(from.row, -n), from.col),
                    GridDirection::Right => CellPos::new(from.row, geo.cols.step(from.col, 10)),
                    GridDirection::Left => CellPos::new(from.row, geo.cols.step(from.col, -10)),
                };
                // The view moves by the same page, as Excel's does.
                if !*extend && matches!(dir, GridDirection::Down | GridDirection::Up) {
                    let dy = geo.rows.offset(to.row) as f32 - geo.rows.offset(from.row) as f32;
                    let (x, y) =
                        geo.clamp_scroll(self.last_view.scroll_x, self.last_view.scroll_y + dy);
                    let seq = self.reveal.map_or(1, |r| r.seq.wrapping_add(1));
                    self.reveal = Some(GridReveal { seq, x, y });
                    self.last_view.scroll_x = x;
                    self.last_view.scroll_y = y;
                }
                self.select_to(to, *extend, false);
            }
            G::RowStart { extend } => {
                let geo = self.geometry().clone();
                let v = self.view();
                let from = if *extend { v.head } else { v.sel.active };
                let col = if from.col >= geo.freeze_cols {
                    geo.freeze_cols
                } else {
                    0
                };
                let col = geo.cols.next_visible(col, true).unwrap_or(0);
                self.select_to(CellPos::new(from.row, col), *extend, false);
            }
            G::FirstCell { extend } => {
                let geo = self.geometry().clone();
                let r = geo.rows.next_visible(geo.freeze_rows, true).unwrap_or(0);
                let c = geo.cols.next_visible(geo.freeze_cols, true).unwrap_or(0);
                self.select_to(CellPos::new(r, c), *extend, false);
            }
            G::LastCell { extend } => {
                let sid = self.sid()?;
                let used = self
                    .workbook
                    .sheet(sid)
                    .and_then(|s| s.cells().used_range(false))
                    .unwrap_or_else(|| Range::cell(0, 0));
                self.select_to(CellPos::new(used.r1, used.c1), *extend, false);
            }
            G::Advance { backward, across } => self.advance(*backward, *across),
            G::SelectCell {
                row,
                col,
                extend,
                add,
            } => self.select_cell(*row, *col, *extend, *add),
            G::SelectRange { range, add } => {
                let r = Range::new(
                    range.r0.min(MAX_ROWS - 1),
                    range.c0.min(MAX_COLS - 1),
                    range.r1.min(MAX_ROWS - 1),
                    range.c1.min(MAX_COLS - 1),
                );
                let r = self.geometry().clone().expand_to_merges(r);
                let mut v = self.view();
                if *add {
                    v.sel.ranges.push(r);
                } else {
                    v.sel.ranges = vec![r];
                }
                v.sel.active = CellPos::new(range.r0.min(MAX_ROWS - 1), range.c0.min(MAX_COLS - 1));
                v.head = CellPos::new(range.r1.min(MAX_ROWS - 1), range.c1.min(MAX_COLS - 1));
                self.set_view(v);
            }
            G::SelectRows {
                from,
                to,
                extend,
                add,
            } => {
                let mut v = self.view();
                let (a, b) = if *extend {
                    (v.sel.active.row, *to)
                } else {
                    (*from, *to)
                };
                let r = Range::new(a.min(MAX_ROWS - 1), 0, b.min(MAX_ROWS - 1), MAX_COLS - 1);
                if *add {
                    v.sel.ranges.push(r);
                } else {
                    v.sel.ranges = vec![r];
                }
                if !*extend {
                    let col = self
                        .geometry()
                        .clone()
                        .cols
                        .next_visible(0, true)
                        .unwrap_or(0);
                    v.sel.active = CellPos::new(a.min(MAX_ROWS - 1), col);
                }
                v.head = CellPos::new(b.min(MAX_ROWS - 1), MAX_COLS - 1);
                self.set_view(v);
            }
            G::SelectColumns {
                from,
                to,
                extend,
                add,
            } => {
                let mut v = self.view();
                let (a, b) = if *extend {
                    (v.sel.active.col, *to)
                } else {
                    (*from, *to)
                };
                let r = Range::new(0, a.min(MAX_COLS - 1), MAX_ROWS - 1, b.min(MAX_COLS - 1));
                if *add {
                    v.sel.ranges.push(r);
                } else {
                    v.sel.ranges = vec![r];
                }
                if !*extend {
                    let row = self
                        .geometry()
                        .clone()
                        .rows
                        .next_visible(0, true)
                        .unwrap_or(0);
                    v.sel.active = CellPos::new(row, a.min(MAX_COLS - 1));
                }
                v.head = CellPos::new(MAX_ROWS - 1, b.min(MAX_COLS - 1));
                self.set_view(v);
            }
            G::SelectAll => {
                let mut v = self.view();
                let whole = Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1);
                let current = v.sel.ranges.last().copied();
                let region = self.current_region(v.sel.active);
                // First Ctrl+A: the current region; again (or outside data): the whole sheet.
                let target = match region {
                    Some(r) if Some(r) != current && current != Some(whole) => r,
                    _ => whole,
                };
                v.sel.ranges = vec![target];
                v.head = CellPos::new(target.r1, target.c1);
                if !target.contains(v.sel.active.row, v.sel.active.col) {
                    v.sel.active = CellPos::new(target.r0, target.c0);
                }
                self.set_view(v);
            }
            G::SelectRow => {
                let mut v = self.view();
                v.sel.ranges = v
                    .sel
                    .ranges
                    .iter()
                    .map(|r| Range::new(r.r0, 0, r.r1, MAX_COLS - 1))
                    .collect();
                self.set_view(v);
            }
            G::SelectColumn => {
                let mut v = self.view();
                v.sel.ranges = v
                    .sel
                    .ranges
                    .iter()
                    .map(|r| Range::new(0, r.c0, MAX_ROWS - 1, r.c1))
                    .collect();
                self.set_view(v);
            }
            G::GoTo { reference, define } => self.go_to(reference, *define)?,
            G::FindNext {
                text,
                match_case,
                whole_cell,
                backward,
                in_formulas,
            } => {
                self.find_next(text, *match_case, *whole_cell, *backward, *in_formulas)?;
            }
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} as navigation",
                    cmd.meta().id
                )));
            }
        }
        Ok(Change::Selection)
    }

    /// Makes a fresh view state from a selection (tests and structural edits).
    #[allow(dead_code)]
    pub(crate) fn view_with(sel: Sel) -> ViewState {
        let head = sel.active;
        ViewState { sel, head }
    }
}
