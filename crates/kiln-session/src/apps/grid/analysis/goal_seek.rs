// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data ▸ What-If Analysis ▸ Goal Seek: the value of one cell that makes a formula reach a
//! target. The secant method from the cell's current value, then, if it stalls, a search for a
//! sign change followed by bisection; at most 100 steps each (Excel's Maximum Iterations) and a
//! result within 0.001 of the target (Excel's Maximum Change). The workbook is recalculated for
//! every trial; the solution is written as one undo step, and Goal Seek Status shows the result.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::GridGoalSeekStatus;
use kiln_sheet::commands::{CellInput, Content};
use kiln_sheet::meta::CalcMode;
use kiln_sheet::pivot::cache::split_reference;
use kiln_sheet::{CellValue, Command, SheetId};

use super::super::GridDoc;
use crate::model::Change;

const MAX_STEPS: usize = 100;
const TOLERANCE: f64 = 0.001;

fn bad(s: &str) -> KilnError {
    KilnError::InvalidInput(s.to_owned())
}

/// The secant method, then bracketing and bisection; the best x found and whether it is a
/// solution.
pub(crate) fn solve(mut f: impl FnMut(f64) -> Option<f64>, x0: f64, target: f64) -> (f64, bool) {
    let close = |y: f64| (y - target).abs() <= TOLERANCE;
    let mut best = (x0, f64::INFINITY);
    let note = |x: f64, y: f64, best: &mut (f64, f64)| {
        let e = (y - target).abs();
        if e < best.1 {
            *best = (x, e);
        }
    };
    let Some(y0) = f(x0) else {
        return (x0, false);
    };
    note(x0, y0, &mut best);
    if close(y0) {
        return (x0, true);
    }
    // Secant.
    let mut a = x0;
    let mut ya = y0 - target;
    let mut b = if x0 == 0.0 { 0.01 } else { x0 * 1.01 };
    for _ in 0..MAX_STEPS {
        let Some(yb) = f(b) else { break };
        note(b, yb, &mut best);
        if close(yb) {
            return (b, true);
        }
        let yb = yb - target;
        let d = yb - ya;
        if d == 0.0 || !d.is_finite() {
            break;
        }
        let next = b - yb * (b - a) / d;
        if !next.is_finite() {
            break;
        }
        a = b;
        ya = yb;
        b = next;
    }
    // A sign change around the start, then bisection.
    let g = |x: f64, f: &mut dyn FnMut(f64) -> Option<f64>| f(x).map(|y| y - target);
    let mut step = if x0 == 0.0 { 1.0 } else { x0.abs() * 0.1 };
    let mut bracket = None;
    for _ in 0..60 {
        for (lo, hi) in [(x0 - step, x0), (x0, x0 + step)] {
            if let (Some(l), Some(h)) = (g(lo, &mut f), g(hi, &mut f))
                && l.signum() != h.signum()
            {
                bracket = Some((lo, hi, l));
                break;
            }
        }
        if bracket.is_some() {
            break;
        }
        step *= 2.0;
        if !step.is_finite() {
            break;
        }
    }
    if let Some((mut lo, mut hi, mut ylo)) = bracket {
        for _ in 0..MAX_STEPS {
            let mid = (lo + hi) / 2.0;
            let Some(ym) = f(mid) else { break };
            note(mid, ym, &mut best);
            if close(ym) {
                return (mid, true);
            }
            let ym = ym - target;
            if ym.signum() == ylo.signum() {
                lo = mid;
                ylo = ym;
            } else {
                hi = mid;
            }
        }
    }
    (best.0, false)
}

impl GridDoc {
    fn cell_ref(&self, text: &str) -> Result<(SheetId, u32, u32)> {
        let (sheet, r) = split_reference(text).ok_or_else(|| bad("Reference isn't valid."))?;
        if !r.is_single_cell() {
            return Err(bad("Reference must be to a single cell."));
        }
        let sid = match sheet {
            Some(n) => self
                .workbook
                .sheet_by_name(&n)
                .map(kiln_sheet::Sheet::id)
                .ok_or_else(|| bad("Reference isn't valid."))?,
            None => self.sid()?,
        };
        Ok((sid, r.r0, r.c0))
    }

    /// Goal Seek.
    pub(crate) fn goal_seek(&mut self, set: &str, to: &str, by: &str) -> Result<Change> {
        let (ssid, sr, sc) = self.cell_ref(set)?;
        let (csid, cr, cc) = self.cell_ref(by)?;
        if self.workbook.formula(ssid, sr, sc).is_none() {
            return Err(bad("Cell must contain a formula."));
        }
        if self.workbook.formula(csid, cr, cc).is_some() {
            return Err(bad("Cell must contain a value."));
        }
        let target: f64 = to
            .trim()
            .replace(',', "")
            .parse()
            .map_err(|_| bad("Invalid input: To value must be a number."))?;
        let original = self.workbook.record(csid, cr, cc);
        let x0 = original.value.as_number().unwrap_or(0.0);
        let manual = self.workbook.calc_mode() == CalcMode::Manual;
        let (log, wb) = (&mut self.log, &mut self.workbook);
        let (x, found) = solve(
            |x| {
                let set = Command::SetCells {
                    sheet: csid,
                    cells: vec![CellInput {
                        row: cr,
                        col: cc,
                        content: Content::Value(CellValue::number(x)),
                        style: None,
                    }],
                };
                log.apply_untracked(wb, &set).ok()?;
                if manual {
                    let _ = log.recalc(wb, None);
                }
                wb.value(ssid, sr, sc).as_number()
            },
            x0,
            target,
        );
        // Back to the cell as it was, then the solution as one step.
        let restore = Command::SetCells {
            sheet: csid,
            cells: vec![CellInput {
                row: cr,
                col: cc,
                content: Content::Value(original.value.clone()),
                style: None,
            }],
        };
        let _ = self.log.apply_untracked(&mut self.workbook, &restore);
        if manual {
            let _ = self.log.recalc(&mut self.workbook, None);
        }
        let _ = self.workbook.take_changes();
        self.exec(
            "grid.data.run_goal_seek",
            Command::SetCells {
                sheet: csid,
                cells: vec![CellInput::value(cr, cc, CellValue::number(x))],
            },
        )?;
        if manual {
            let _ = self.log.recalc(&mut self.workbook, None);
        }
        let shown = self.shown_text(ssid, sr, sc);
        self.analysis.goal_seek = Some(GridGoalSeekStatus {
            cell: kiln_sheet::Range::cell(sr, sc).to_a1(false),
            found,
            target: to.trim().to_owned(),
            current: shown,
        });
        Ok(Change::Content)
    }

    /// A cell's value as it is displayed.
    pub(crate) fn shown_text(&self, sid: SheetId, row: u32, col: u32) -> String {
        let v = self.workbook.value(sid, row, col);
        let style = self.workbook.sheet(sid).map_or(0, |s| s.style_at(row, col));
        let code = self.workbook.styles().num_fmt(style);
        kiln_sheet::format::format_value(&v, code, self.workbook.props().date1904).text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secant_and_bisection_find_targets() {
        // A loan: payment of 1,000 over 10 years at 5 %: what principal?
        let pmt = |pv: f64| Some(pv * 0.05 / (1.0 - 1.05_f64.powi(-10)));
        let (x, ok) = solve(pmt, 1.0, 1_000.0);
        assert!(ok);
        assert!((pmt(x).unwrap_or(0.0) - 1_000.0).abs() <= TOLERANCE);
        // A kink the secant method misses: |x − 3| + 1 = 4 from 0.
        let (x, ok) = solve(|x| Some((x - 3.0).abs() + 1.0), 0.0, 4.0);
        assert!(ok, "{x}");
        // No solution: x² = −1 reports the closest value found.
        let (_, ok) = solve(|x| Some(x * x), 2.0, -1.0);
        assert!(!ok);
        // An error stops the search.
        let (x, ok) = solve(|_| None, 5.0, 1.0);
        assert_eq!((x, ok), (5.0, false));
    }
}
