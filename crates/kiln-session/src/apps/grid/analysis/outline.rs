// SPDX-License-Identifier: AGPL-3.0-or-later
//! The outline gutters' buttons (Data ▸ Group, Subtotal): a level button shows the rows (or
//! columns) up to its level and hides the deeper ones; a group's − hides its detail and its +
//! shows it again, keeping the nested groups that were collapsed collapsed, as Excel does. Each
//! click is one undo step; the hidden rows and the summaries' collapsed flags save to `.xlsx`.

use std::collections::BTreeMap;

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridHit;
use kiln_sheet::commands::{ColPatch, RowPatch};
use kiln_sheet::{Command, SheetId};
use kiln_sheet_layout::{OutlineBands, OutlineHit};

use super::super::GridDoc;
use crate::model::Change;

/// What a click changes on one row or column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Patch {
    hidden: Option<bool>,
    collapsed: Option<bool>,
}

/// The outline of one axis: levels, hidden and collapsed flags of the indices with properties.
struct Axis {
    bands: OutlineBands,
    columns: bool,
    hidden: BTreeMap<u32, bool>,
    collapsed: BTreeMap<u32, bool>,
}

impl Axis {
    fn level(&self, i: u32) -> u8 {
        if self.columns {
            self.bands.col_level(i)
        } else {
            self.bands.row_level(i)
        }
    }
    fn list(&self) -> &[(u32, u8)] {
        if self.columns {
            &self.bands.cols
        } else {
            &self.bands.rows
        }
    }
    fn is_hidden(&self, i: u32) -> bool {
        self.hidden.get(&i).copied().unwrap_or(false)
    }
    fn is_collapsed(&self, i: u32) -> bool {
        self.collapsed.get(&i).copied().unwrap_or(false)
    }
    fn after(&self) -> bool {
        if self.columns {
            self.bands.summary_right
        } else {
            self.bands.summary_below
        }
    }
    /// The summaries of every group: each index next to an outline run's end.
    fn summaries(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .list()
            .iter()
            .filter_map(|&(i, _)| {
                if self.after() {
                    i.checked_add(1)
                } else {
                    i.checked_sub(1)
                }
            })
            .filter(|&s| !self.bands.summary_levels(self.columns, s).is_empty())
            .collect();
        out.dedup();
        out
    }
    /// The run of detail of the group whose summary is `s` at `level`.
    fn run(&self, s: u32, level: u8) -> Vec<u32> {
        let mut out = Vec::new();
        let mut i = self.bands.detail_of(self.columns, s);
        while let Some(d) = i {
            if self.level(d) < level {
                break;
            }
            out.push(d);
            i = if self.after() {
                d.checked_sub(1)
            } else {
                d.checked_add(1)
            };
        }
        out.sort_unstable();
        out
    }
    /// Shows a group's detail at `level` and, inside it, the nested groups not collapsed.
    fn expand(&self, run: &[u32], level: u8, summary: u32, out: &mut BTreeMap<u32, Patch>) {
        let mut k = 0;
        while k < run.len() {
            let i = run[k];
            if self.level(i) == level {
                out.entry(i).or_default().hidden = Some(false);
                k += 1;
                continue;
            }
            // A nested run: up to the next index at this level.
            let start = k;
            while k < run.len() && self.level(run[k]) > level {
                k += 1;
            }
            let inner = &run[start..k];
            let first = inner[0];
            let last = inner[inner.len() - 1];
            let s = if self.after() {
                last.checked_add(1)
            } else {
                first.checked_sub(1)
            };
            let open = match s {
                Some(s) if s != summary => !self.is_collapsed(s),
                _ => true,
            };
            if open {
                self.expand(inner, level + 1, s.unwrap_or(summary), out);
            }
        }
    }
}

impl GridDoc {
    fn outline_axis(&self, columns: bool) -> Result<(SheetId, Axis)> {
        let (sid, sheet) = self
            .active_sheet()
            .map(|(_, s)| (s.id(), s))
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let (hidden, collapsed) = if columns {
            let e = sheet.cols().entries();
            let v: Vec<_> = e.map(|(i, p)| (i, p.hidden, p.collapsed)).collect();
            (
                v.iter().map(|x| (x.0, x.1)).collect(),
                v.iter().map(|x| (x.0, x.2)).collect(),
            )
        } else {
            let e = sheet.rows().entries();
            let v: Vec<_> = e.map(|(i, p)| (i, p.hidden, p.collapsed)).collect();
            (
                v.iter().map(|x| (x.0, x.1)).collect(),
                v.iter().map(|x| (x.0, x.2)).collect(),
            )
        };
        Ok((
            sid,
            Axis {
                bands: OutlineBands::of(sheet),
                columns,
                hidden,
                collapsed,
            },
        ))
    }

    /// Data ▸ outline level button `level` (1 = only the outermost summaries).
    pub(crate) fn outline_level(&mut self, columns: bool, level: u8) -> Result<Change> {
        let (sid, axis) = self.outline_axis(columns)?;
        if axis.list().is_empty() {
            return Err(KilnError::InvalidInput("there is no outline".into()));
        }
        let mut out: BTreeMap<u32, Patch> = BTreeMap::new();
        for &(i, l) in axis.list() {
            let hide = l >= level;
            if axis.is_hidden(i) != hide {
                out.entry(i).or_default().hidden = Some(hide);
            }
        }
        for s in axis.summaries() {
            let own = axis.level(s);
            let shut = own + 1 >= level;
            if axis.is_collapsed(s) != shut {
                out.entry(s).or_default().collapsed = Some(shut);
            }
        }
        self.apply_outline(sid, columns, out, "grid.data.show_outline_level")?;
        self.reselect_visible(columns);
        Ok(Change::Content)
    }

    /// A group's − or + at its summary `index`.
    pub(crate) fn outline_toggle(&mut self, columns: bool, index: u32) -> Result<Change> {
        let (sid, axis) = self.outline_axis(columns)?;
        let levels = axis.bands.summary_levels(columns, index);
        if levels.is_empty() {
            return Err(KilnError::InvalidInput("not the summary of a group".into()));
        }
        let level = *levels.start();
        let run = axis.run(index, level);
        let expanded = run.iter().any(|&i| !axis.is_hidden(i));
        let mut out: BTreeMap<u32, Patch> = BTreeMap::new();
        if expanded {
            for &i in &run {
                if !axis.is_hidden(i) {
                    out.entry(i).or_default().hidden = Some(true);
                }
            }
        } else {
            axis.expand(&run, level, index, &mut out);
        }
        out.entry(index).or_default().collapsed = Some(expanded);
        self.apply_outline(sid, columns, out, "grid.data.toggle_outline_group")?;
        self.reselect_visible(columns);
        Ok(Change::Content)
    }

    /// The patches as row (or column) property commands, consecutive equal ones merged.
    fn apply_outline(
        &mut self,
        sid: SheetId,
        columns: bool,
        patches: BTreeMap<u32, Patch>,
        label: &'static str,
    ) -> Result<()> {
        let mut cmds = Vec::new();
        let mut run: Option<(u32, u32, Patch)> = None;
        let push = |(at, count, p): (u32, u32, Patch), cmds: &mut Vec<Command>| {
            cmds.push(if columns {
                Command::SetColProps {
                    sheet: sid,
                    at,
                    count,
                    patch: ColPatch {
                        hidden: p.hidden,
                        collapsed: p.collapsed,
                        ..ColPatch::default()
                    },
                }
            } else {
                Command::SetRowProps {
                    sheet: sid,
                    at,
                    count,
                    patch: RowPatch {
                        hidden: p.hidden,
                        collapsed: p.collapsed,
                        ..RowPatch::default()
                    },
                }
            });
        };
        for (i, p) in patches {
            run = match run {
                Some((at, count, q)) if q == p && at + count == i => Some((at, count + 1, q)),
                Some(r) => {
                    push(r, &mut cmds);
                    Some((i, 1, p))
                }
                None => Some((i, 1, p)),
            };
        }
        if let Some(r) = run {
            push(r, &mut cmds);
        }
        if cmds.is_empty() {
            return Ok(());
        }
        self.exec_all(label, cmds).map(|_| ())
    }

    /// After rows (or columns) hid: the active cell moves to the next shown one, as Excel's does.
    fn reselect_visible(&mut self, columns: bool) {
        let a = self.view().sel.active;
        let Some((_, sheet)) = self.active_sheet() else {
            return;
        };
        let (axis_hidden, mut at) = if columns {
            (sheet.cols().is_hidden(a.col), a.col)
        } else {
            (sheet.rows().is_hidden(a.row), a.row)
        };
        if !axis_hidden {
            return;
        }
        let max = if columns {
            kiln_sheet::MAX_COLS
        } else {
            kiln_sheet::MAX_ROWS
        };
        while at + 1 < max
            && (if columns {
                sheet.cols().is_hidden(at)
            } else {
                sheet.rows().is_hidden(at)
            })
        {
            at += 1;
        }
        if columns {
            self.select_cell(a.row, at, false, false);
        } else {
            self.select_cell(at, a.col, false, false);
        }
    }

    /// The outline button under a surface point (`coarse`: for a finger, a larger target).
    pub(crate) fn outline_hit_at(&self, x: f32, y: f32, coarse: bool) -> Option<GridHit> {
        self.with_geometry(|geo| {
            if geo.outline.rows.is_empty() && geo.outline.cols.is_empty() {
                return None;
            }
            let hw = self.header_w(geo);
            match kiln_sheet_layout::outline_hit(geo, &self.last_view, x, y, hw, coarse)? {
                OutlineHit::Level { columns, level } => {
                    Some(GridHit::OutlineLevel { columns, level })
                }
                OutlineHit::Toggle { columns, index } => {
                    Some(GridHit::OutlineToggle { columns, index })
                }
            }
        })
    }
}
