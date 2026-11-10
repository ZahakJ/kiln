// SPDX-License-Identifier: AGPL-3.0-or-later
//! Find and Replace (Ctrl+F, Ctrl+H) with Excel's options — match case, match entire cell
//! contents, within the sheet or the workbook, by rows or columns, look in formulas, values or
//! notes, `*` and `?` wildcards with `~` escapes — Find All's list, Replace, Replace All, and
//! Go To Special.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{GridFindHit, GridFindSpec, GridLookIn, GridSpecial};
use kiln_sheet::address::cell_a1;
use kiln_sheet::commands::{CellInput, Content};
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};

use super::super::GridDoc;
use crate::model::Change;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Lit(char),
    One,
    Star,
}

/// A Find pattern: literal text with `*`, `?` and `~` escapes.
struct Pattern {
    toks: Vec<Tok>,
    case: bool,
}

impl Pattern {
    fn new(text: &str, case: bool) -> Pattern {
        let chars: Vec<char> = text.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '~' if i + 1 < chars.len() => {
                    toks.push(Tok::Lit(chars[i + 1]));
                    i += 2;
                    continue;
                }
                '*' => toks.push(Tok::Star),
                '?' => toks.push(Tok::One),
                c => toks.push(Tok::Lit(c)),
            }
            i += 1;
        }
        Pattern { toks, case }
    }

    fn eq(&self, a: char, b: char) -> bool {
        if self.case {
            a == b
        } else {
            a == b || a.to_lowercase().eq(b.to_lowercase())
        }
    }

    /// The pattern as its `*`-free segments, and whether it starts and ends with `*`.
    fn segments(&self) -> (Vec<&[Tok]>, bool, bool) {
        let segs: Vec<&[Tok]> = self.toks.split(|t| *t == Tok::Star).collect();
        let lead = self.toks.first() == Some(&Tok::Star);
        let trail = self.toks.last() == Some(&Tok::Star);
        (segs, lead, trail)
    }

    /// Whether a `*`-free segment matches `t` at `at`.
    fn seg_at(&self, seg: &[Tok], t: &[char], at: usize) -> bool {
        at + seg.len() <= t.len()
            && seg.iter().zip(&t[at..]).all(|(tok, &c)| match *tok {
                Tok::Lit(l) => self.eq(l, c),
                _ => true,
            })
    }

    /// The earliest place at or after `from` where `seg` matches.
    fn seg_first(&self, seg: &[Tok], t: &[char], from: usize) -> Option<usize> {
        (from..=t.len().saturating_sub(seg.len())).find(|&i| self.seg_at(seg, t, i))
    }

    /// The latest place at or after `from` where `seg` matches, ending by `end`.
    fn seg_last(&self, seg: &[Tok], t: &[char], from: usize, end: usize) -> Option<usize> {
        let top = end.checked_sub(seg.len())?;
        (from..=top).rev().find(|&i| self.seg_at(seg, t, i))
    }

    /// Where the rest of the pattern (segments after the first) ends when its first segment
    /// ended at `pos`: middle segments at their earliest, the last at its latest (longest
    /// match). `None` when it cannot match from `pos` (nor from any later place).
    fn rest_end(
        &self,
        segs: &[&[Tok]],
        trail: bool,
        t: &[char],
        mut pos: usize,
        end: usize,
    ) -> Option<usize> {
        if segs.len() <= 1 {
            return Some(if trail { end } else { pos });
        }
        for seg in &segs[1..segs.len() - 1] {
            pos = self.seg_first(seg, t, pos)? + seg.len();
        }
        let last = segs[segs.len() - 1];
        if trail || last.is_empty() {
            return (pos <= end).then_some(end);
        }
        self.seg_last(last, t, pos, end).map(|i| i + last.len())
    }

    /// Whether the whole of `t` matches (Match entire cell contents). Linear in the text for
    /// each segment: no backtracking.
    fn full(&self, t: &[char]) -> bool {
        let (segs, _lead, trail) = self.segments();
        let first = segs[0];
        if !self.seg_at(first, t, 0) {
            return false;
        }
        if segs.len() == 1 {
            return first.len() == t.len();
        }
        match self.rest_end(&segs, trail, t, first.len(), t.len()) {
            Some(e) => e == t.len(),
            None => false,
        }
    }

    /// The first match in `t` (char offsets) at or after `from`, longest at the earliest start.
    fn find(&self, t: &[char], from: usize) -> Option<(usize, usize)> {
        if self.toks.is_empty() || from > t.len() {
            return None;
        }
        let (segs, lead, trail) = self.segments();
        let first = segs[0];
        if lead {
            // A leading `*` takes the text from `from`.
            let e = self.rest_end(&segs, trail, t, from, t.len())?;
            return (e > from).then_some((from, e));
        }
        let mut s = from;
        loop {
            let at = self.seg_first(first, t, s)?;
            let end = self.rest_end(&segs, trail, t, at + first.len(), t.len())?;
            if end > at {
                return Some((at, end));
            }
            s = at + 1;
            if s > t.len() {
                return None;
            }
        }
    }

    fn matches(&self, text: &str, whole: bool) -> bool {
        let t: Vec<char> = text.chars().collect();
        if whole {
            self.full(&t)
        } else {
            self.find(&t, 0).is_some()
        }
    }

    /// `text` with every match replaced by `with`; the count.
    fn replace(&self, text: &str, with: &str, whole: bool) -> (String, usize) {
        let t: Vec<char> = text.chars().collect();
        if whole {
            return if self.full(&t) {
                (with.to_owned(), 1)
            } else {
                (text.to_owned(), 0)
            };
        }
        let mut out = String::new();
        let mut at = 0;
        let mut n = 0;
        while let Some((s, e)) = self.find(&t, at) {
            out.extend(&t[at..s]);
            out.push_str(with);
            at = e;
            n += 1;
            if at >= t.len() {
                break;
            }
        }
        out.extend(&t[at.min(t.len())..]);
        (out, n)
    }
}

impl GridDoc {
    /// The text Find searches in a cell.
    fn find_text(&self, sid: SheetId, row: u32, col: u32, look: GridLookIn) -> Option<String> {
        match look {
            GridLookIn::Formulas => {
                let t = self.edit_text(sid, row, col);
                (!t.is_empty()).then_some(t)
            }
            GridLookIn::Values => {
                let t = self.workbook.display_text(sid, row, col);
                (!t.is_empty()).then_some(t)
            }
            GridLookIn::Notes => {
                let s = self.workbook.sheet(sid)?;
                let mut t = s
                    .note_at(row, col)
                    .map(|n| n.text.clone())
                    .unwrap_or_default();
                for c in s.threads_at(row, col) {
                    t.push('\n');
                    t.push_str(&c.text);
                }
                (!t.trim().is_empty()).then_some(t)
            }
        }
    }

    /// The cells to search on a sheet, in the spec's order.
    fn search_cells(&self, sid: SheetId, spec: &GridFindSpec) -> Vec<(u32, u32)> {
        let Some(sheet) = self.workbook.sheet(sid) else {
            return Vec::new();
        };
        let mut cells: Vec<(u32, u32)> = match spec.look_in {
            GridLookIn::Notes => sheet
                .meta()
                .notes
                .iter()
                .map(|n| (n.at.row, n.at.col))
                .chain(sheet.meta().threads.iter().map(|t| (t.at.row, t.at.col)))
                .collect(),
            _ => sheet
                .cells()
                .iter()
                .filter(|(_, _, d)| d.has_content())
                .map(|(r, c, _)| (r, c))
                .collect(),
        };
        if spec.by_columns {
            cells.sort_unstable_by_key(|&(r, c)| (c, r));
        } else {
            cells.sort_unstable();
        }
        cells.dedup();
        cells
    }

    fn sheets_for(&self, spec: &GridFindSpec) -> Vec<(usize, SheetId)> {
        if spec.workbook {
            // From the active sheet on, wrapping.
            let n = self.workbook.sheets().len();
            (0..n)
                .map(|k| (self.active + k) % n)
                .filter_map(|i| self.workbook.sheets().get(i).map(|s| (i, s.id())))
                .collect()
        } else {
            self.active_sheet()
                .map(|(i, s)| (i, s.id()))
                .into_iter()
                .collect()
        }
    }

    /// Find All.
    pub(crate) fn find_all(&self, spec: &GridFindSpec) -> Result<Vec<GridFindHit>> {
        if spec.text.is_empty() {
            return Ok(Vec::new());
        }
        let p = Pattern::new(&spec.text, spec.match_case);
        let mut out = Vec::new();
        let mut sheets = self.sheets_for(spec);
        sheets.sort_by_key(|(i, _)| *i);
        for (i, sid) in sheets {
            let name = self
                .workbook
                .sheet(sid)
                .map(|s| s.name().to_owned())
                .unwrap_or_default();
            for (row, col) in self.search_cells(sid, spec) {
                let Some(t) = self.find_text(sid, row, col, spec.look_in) else {
                    continue;
                };
                if p.matches(&t, spec.whole_cell) {
                    out.push(GridFindHit {
                        sheet: u32::try_from(i).unwrap_or(0),
                        sheet_name: name.clone(),
                        row,
                        col,
                        cell: cell_a1(row, col, true),
                        value: self.workbook.display_text(sid, row, col),
                        formula: self.workbook.formula(sid, row, col).map(|f| {
                            if f.starts_with('=') {
                                f
                            } else {
                                format!("={f}")
                            }
                        }),
                    });
                    if out.len() >= 100_000 {
                        return Ok(out);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Selects the next match after the active cell (wrapping, across sheets for the
    /// workbook); false when there is none.
    fn find_next_match(&mut self, spec: &GridFindSpec) -> bool {
        let p = Pattern::new(&spec.text, spec.match_case);
        let a = self.view().sel.active;
        let key = |r: u32, c: u32| if spec.by_columns { (c, r) } else { (r, c) };
        let here = key(a.row, a.col);
        let sheets = self.sheets_for(spec);
        for (k, (index, sid)) in sheets.iter().enumerate() {
            let cells = self.search_cells(*sid, spec);
            let start = if k == 0 {
                cells.partition_point(|&(r, c)| key(r, c) <= here)
            } else {
                0
            };
            let hit = cells[start..].iter().copied().find(|&(r, c)| {
                self.find_text(*sid, r, c, spec.look_in)
                    .is_some_and(|t| p.matches(&t, spec.whole_cell))
            });
            if let Some((r, c)) = hit {
                if *index != self.active {
                    self.activate(*index);
                }
                self.select_cell(r, c, false, false);
                return true;
            }
        }
        // Wrap around on the first sheet.
        if let Some(&(index, sid)) = sheets.first() {
            let cells = self.search_cells(sid, spec);
            let end = cells.partition_point(|&(r, c)| key(r, c) <= here);
            if let Some((r, c)) = cells[..end].iter().copied().find(|&(r, c)| {
                self.find_text(sid, r, c, spec.look_in)
                    .is_some_and(|t| p.matches(&t, spec.whole_cell))
            }) {
                if index != self.active {
                    self.activate(index);
                }
                self.select_cell(r, c, false, false);
                return true;
            }
        }
        false
    }

    pub(crate) fn find_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        match cmd {
            GridCommand::Replace { spec, with, all } => {
                if spec.text.is_empty() {
                    return Err(KilnError::InvalidInput("enter what to find".into()));
                }
                let p = Pattern::new(&spec.text, spec.match_case);
                // Replace works on what is typed in the cells (formulas and constants).
                let look = GridLookIn::Formulas;
                let spec = GridFindSpec {
                    look_in: look,
                    ..spec.clone()
                };
                if *all {
                    let mut count = 0usize;
                    let mut cmds = Vec::new();
                    for (_, sid) in self.sheets_for(&spec) {
                        let mut cells = Vec::new();
                        for (row, col) in self.search_cells(sid, &spec) {
                            let Some(t) = self.find_text(sid, row, col, look) else {
                                continue;
                            };
                            let (next, n) = p.replace(&t, with, spec.whole_cell);
                            if n > 0 {
                                count += n;
                                cells.push(CellInput {
                                    row,
                                    col,
                                    content: Content::Typed(next),
                                    style: None,
                                });
                            }
                        }
                        if !cells.is_empty() {
                            cmds.push(Command::SetCells { sheet: sid, cells });
                        }
                    }
                    if count == 0 {
                        self.features.message =
                            Some("We couldn't find anything to replace.".into());
                        return Ok(Change::Selection);
                    }
                    self.exec_all("grid.editing.replace", cmds)?;
                    self.features.message = Some(format!(
                        "All done. We made {count} replacement{}.",
                        if count == 1 { "" } else { "s" }
                    ));
                    return Ok(Change::Content);
                }
                // Replace: the active cell when it matches, then the next match.
                let sid = self.sid()?;
                let a = self.view().sel.active;
                let mut changed = false;
                if let Some(t) = self.find_text(sid, a.row, a.col, look)
                    && p.matches(&t, spec.whole_cell)
                {
                    let (next, _) = p.replace(&t, with, spec.whole_cell);
                    self.exec(
                        "grid.editing.replace",
                        Command::SetCells {
                            sheet: sid,
                            cells: vec![CellInput {
                                row: a.row,
                                col: a.col,
                                content: Content::Typed(next),
                                style: None,
                            }],
                        },
                    )?;
                    changed = true;
                }
                if !self.find_next_match(&spec) && !changed {
                    self.features.message = Some(format!(
                        "We couldn't find what you were looking for: \"{}\".",
                        spec.text
                    ));
                }
                Ok(if changed {
                    Change::Content
                } else {
                    Change::Selection
                })
            }
            GridCommand::SelectSpecial { special } => {
                let ranges = self.special_cells(*special)?;
                if ranges.is_empty() {
                    return Err(KilnError::InvalidInput("No cells were found.".into()));
                }
                let active = ranges
                    .first()
                    .map(|r| CellPos::new(r.r0, r.c0))
                    .unwrap_or_default();
                let mut v = self.view();
                // The active cell's range goes last.
                let mut ranges = ranges;
                let first = ranges.remove(0);
                ranges.push(first);
                v.sel.ranges = ranges;
                v.sel.active = active;
                v.head = active;
                self.set_view(v);
                self.reveal_cell(active);
                Ok(Change::Selection)
            }
            _ => Ok(Change::None),
        }
    }

    /// The cells Go To Special selects (within the selection when it is several cells).
    fn special_cells(&self, special: GridSpecial) -> Result<Vec<Range>> {
        let sid = self.sid()?;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let v = self.view();
        let used = sheet.cells().used_range(false).unwrap_or(Range::cell(0, 0));
        let sel = v.sel.ranges.last().copied().unwrap_or(Range::cell(0, 0));
        let scope = if sel.is_single_cell() {
            used
        } else {
            sel.intersection(&used).unwrap_or(sel)
        };
        let a = v.sel.active;
        let cells = |pred: &dyn Fn(u32, u32) -> bool| -> Vec<Range> {
            let mut list: Vec<(u32, u32)> = sheet
                .cells()
                .iter_range(scope)
                .filter(|(r, c, _)| pred(*r, *c))
                .map(|(r, c, _)| (r, c))
                .collect();
            list.sort_unstable();
            runs(&list)
        };
        Ok(match special {
            GridSpecial::Notes => {
                let mut list: Vec<(u32, u32)> = sheet
                    .meta()
                    .notes
                    .iter()
                    .map(|n| (n.at.row, n.at.col))
                    .chain(sheet.meta().threads.iter().map(|t| (t.at.row, t.at.col)))
                    .filter(|&(r, c)| scope.contains(r, c) || sel.is_single_cell())
                    .collect();
                list.sort_unstable();
                list.dedup();
                runs(&list)
            }
            GridSpecial::Constants => cells(&|r, c| {
                sheet
                    .cells()
                    .get(r, c)
                    .is_some_and(|d| d.has_content() && !d.kind.is_formula())
            }),
            GridSpecial::Formulas => {
                cells(&|r, c| sheet.cells().get(r, c).is_some_and(|d| d.kind.is_formula()))
            }
            GridSpecial::Errors => {
                cells(&|r, c| matches!(self.workbook.value(sid, r, c), CellValue::Error(_)))
            }
            GridSpecial::Blanks => {
                let mut list = Vec::new();
                if scope.cell_count() <= 4_000_000 {
                    for p in scope.cells() {
                        if !self.has_content(sid, p.row, p.col) {
                            list.push((p.row, p.col));
                        }
                    }
                }
                runs(&list)
            }
            GridSpecial::CurrentRegion => self.current_region(a).into_iter().collect(),
            GridSpecial::CurrentArray => self
                .workbook
                .spill_range(sid, a.row, a.col)
                .map(|s| Range::from_bounds(s.bounds))
                .into_iter()
                .collect(),
            GridSpecial::LastCell => vec![Range::cell(used.r1, used.c1)],
            GridSpecial::VisibleCells => {
                let mut list = Vec::new();
                if scope.cell_count() <= 4_000_000 {
                    for r in scope.r0..=scope.r1 {
                        if sheet.is_row_hidden(r) {
                            continue;
                        }
                        for c in scope.c0..=scope.c1 {
                            if !sheet.is_col_hidden(c) {
                                list.push((r, c));
                            }
                        }
                    }
                }
                runs(&list)
            }
            GridSpecial::ConditionalFormats => sheet
                .meta()
                .cf
                .iter()
                .flat_map(|b| b.ranges.iter().copied())
                .filter(|r| sel.is_single_cell() || r.intersects(&scope))
                .collect(),
            GridSpecial::DataValidation => sheet
                .meta()
                .validations
                .iter()
                .flat_map(|b| b.ranges.iter().copied())
                .filter(|r| sel.is_single_cell() || r.intersects(&scope))
                .collect(),
            GridSpecial::Precedents => {
                // The ranges the selected formulas refer to on this sheet (Excel's Ctrl+[).
                let mut out: Vec<Range> = Vec::new();
                let picked = if sel.is_single_cell() {
                    Range::cell(a.row, a.col)
                } else {
                    sel
                };
                for (r, c, d) in sheet.cells().iter_range(picked) {
                    if !d.kind.is_formula() {
                        continue;
                    }
                    if let Some(f) = self.workbook.formula(sid, r, c) {
                        for (s2, range) in self.refs_of(sid, &f) {
                            if s2 == sid && !out.contains(&range) {
                                out.push(range);
                            }
                        }
                    }
                }
                out
            }
            GridSpecial::Dependents => {
                // The formulas on this sheet that refer to the selection (Excel's Ctrl+]).
                let picked = if sel.is_single_cell() {
                    Range::cell(a.row, a.col)
                } else {
                    sel
                };
                let mut list: Vec<(u32, u32)> = sheet
                    .cells()
                    .iter()
                    .filter(|(_, _, d)| d.kind.is_formula())
                    .take(500_000)
                    .filter(|&(r, c, _)| {
                        self.workbook.formula(sid, r, c).is_some_and(|f| {
                            self.refs_of(sid, &f)
                                .iter()
                                .any(|(s2, range)| *s2 == sid && range.intersects(&picked))
                        })
                    })
                    .map(|(r, c, _)| (r, c))
                    .collect();
                list.sort_unstable();
                runs(&list)
            }
        })
    }
}

/// Cells (sorted by row, then column) as ranges: runs along each row, then runs of rows with
/// the same column span merged (at most 10,000 ranges).
fn runs(cells: &[(u32, u32)]) -> Vec<Range> {
    let mut row_runs: Vec<Range> = Vec::new();
    for &(r, c) in cells {
        match row_runs.last_mut() {
            Some(last) if last.r0 == r && last.c1 + 1 == c => last.c1 = c,
            _ => row_runs.push(Range::cell(r, c)),
        }
    }
    let mut out: Vec<Range> = Vec::new();
    for run in row_runs {
        if let Some(prev) = out
            .iter_mut()
            .rev()
            .take(64)
            .find(|p| p.c0 == run.c0 && p.c1 == run.c1 && p.r1 + 1 == run.r0)
        {
            prev.r1 = run.r0;
            continue;
        }
        out.push(run);
        if out.len() >= 10_000 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_with_wildcards_case_and_escapes() {
        let p = Pattern::new("gl*e", false);
        assert!(p.matches("Blue GLAZE", false));
        assert!(!p.matches("Blue GLAZE", true));
        assert!(Pattern::new("?lue*", false).matches("Blue GLAZE", true));
        assert!(Pattern::new("50~%", false).matches("50%", true));
        assert!(!Pattern::new("Glaze", true).matches("glaze", false));
        assert_eq!(
            Pattern::new("kiln", false).replace("Kiln and kiln", "Grid", false),
            ("Grid and Grid".to_owned(), 2)
        );
        assert_eq!(
            Pattern::new("a?c", false).replace("abc-axc", "!", false),
            ("!-!".to_owned(), 2)
        );
    }

    #[test]
    fn wildcards_match_without_backtracking_blowups() {
        // The review's cases: many stars against a long run of one letter, and long cells.
        let a40 = "a".repeat(40);
        let t = std::time::Instant::now();
        assert!(!Pattern::new("*a*a*a*a*a*a*b", false).matches(&a40, false));
        assert!(!Pattern::new("*a*a*a*a*a*a*b", false).matches(&a40, true));
        assert!(Pattern::new("*a*a*a*a*a*a*", false).matches(&a40, true));
        let long = "lorem ipsum ".repeat(1500);
        assert!(!Pattern::new("zzz", false).matches(&long, false));
        assert!(!Pattern::new("*?*?*?*z", false).matches(&long, true));
        let (out, n) = Pattern::new("ipsum", false).replace(&long, "x", false);
        assert_eq!(n, 1500);
        assert!(out.starts_with("lorem x lorem x"));
        let aaa = "a".repeat(5_000);
        assert!(!Pattern::new("a*a*a*a*a*a*a*a*a*b", false).matches(&aaa, false));
        assert!(t.elapsed().as_millis() < 500, "{:?}", t.elapsed());
    }

    #[test]
    fn matches_are_leftmost_and_longest() {
        let p = Pattern::new("a*b", false);
        let t: Vec<char> = "xaybzb-ab".chars().collect();
        assert_eq!(p.find(&t, 0), Some((1, 9)));
        assert_eq!(
            Pattern::new("*b", false).find(&"aab".chars().collect::<Vec<_>>(), 0),
            Some((0, 3))
        );
        assert_eq!(
            Pattern::new("b*", false).find(&"aabcc".chars().collect::<Vec<_>>(), 0),
            Some((2, 5))
        );
        assert!(Pattern::new("a*c*e", false).matches("abcde", true));
        assert!(!Pattern::new("a*c*e", false).matches("abcdef", true));
        assert!(Pattern::new("a*c*e", false).matches("xxabcdexx", false));
        assert!(Pattern::new("*", false).matches("", true));
        assert!(Pattern::new("a?", false).matches("ab", true));
        assert!(!Pattern::new("a?", false).matches("a", true));
    }

    #[test]
    fn cells_become_ranges() {
        let c = [(0, 0), (0, 1), (1, 0), (1, 1), (3, 4)];
        assert_eq!(runs(&c), vec![Range::new(0, 0, 1, 1), Range::cell(3, 4)]);
    }
}
