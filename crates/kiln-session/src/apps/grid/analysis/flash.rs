// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flash Fill (Data ▸ Flash Fill, Ctrl+E): fills a column from one or two examples typed beside
//! the data, by finding a program that turns each example's row into its text.
//!
//! A program is a sequence of pieces, each a constant or a part of one source column: the whole
//! text, a word, a run of letters or digits (counted from the start or the end), its first
//! letter, or the text before or after a separator — with upper, lower or proper case. The first
//! example's text is split into pieces every way such parts can produce (a search over its
//! positions); the programs that reproduce every other example are kept, and the simplest one
//! fills the empty cells. Rows where a part is missing stay empty, as in Excel.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::GridFlashCell;
use kiln_sheet::commands::CellInput;
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};

use super::super::GridDoc;
use crate::model::Change;

/// How a part is cased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Same,
    Upper,
    Lower,
    Proper,
}

impl Case {
    fn apply(self, s: &str) -> String {
        match self {
            Case::Same => s.to_owned(),
            Case::Upper => s.to_uppercase(),
            Case::Lower => s.to_lowercase(),
            Case::Proper => {
                let mut out = String::with_capacity(s.len());
                let mut start = true;
                for c in s.chars() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = !c.is_alphanumeric();
                }
                out
            }
        }
    }
}

/// Which tokens a part counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tokens {
    /// Runs of letters and digits.
    Words,
    /// Runs of letters.
    Letters,
    /// Runs of digits.
    Digits,
}

/// A part of a source text.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Whole,
    /// The n-th token (negative: from the end).
    Token(Tokens, i32),
    /// The first character of the n-th token.
    Initial(Tokens, i32),
    /// The tokens from the n-th to the last, with what lies between them.
    From(Tokens, i32),
    /// The text before the first (or last) separator.
    Before(char, bool),
    /// The text after the first (or last) separator.
    After(char, bool),
    /// Characters at fixed positions (start, count) of the text.
    Chars(u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Const(String),
    Part { col: usize, part: Part, case: Case },
}

fn tokens(s: &str, kind: Tokens) -> Vec<(usize, usize)> {
    let class = |c: char| match kind {
        Tokens::Words => c.is_alphanumeric(),
        Tokens::Letters => c.is_alphabetic(),
        Tokens::Digits => c.is_ascii_digit(),
    };
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        match (class(c), start) {
            (true, None) => start = Some(i),
            (false, Some(a)) => {
                out.push((a, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(a) = start {
        out.push((a, s.len()));
    }
    out
}

fn nth<T: Copy>(v: &[T], n: i32) -> Option<T> {
    let len = i32::try_from(v.len()).ok()?;
    let i = if n < 0 { len + n } else { n };
    usize::try_from(i).ok().and_then(|i| v.get(i).copied())
}

impl Part {
    fn eval(&self, s: &str) -> Option<String> {
        match self {
            Part::Whole => (!s.is_empty()).then(|| s.to_owned()),
            Part::Token(k, n) => nth(&tokens(s, *k), *n).map(|(a, b)| s[a..b].to_owned()),
            Part::Initial(k, n) => {
                nth(&tokens(s, *k), *n).and_then(|(a, _)| s[a..].chars().next().map(String::from))
            }
            Part::From(k, n) => nth(&tokens(s, *k), *n).map(|(a, _)| {
                let end = tokens(s, *k).last().map_or(s.len(), |t| t.1);
                s[a..end].to_owned()
            }),
            Part::Before(c, last) => {
                let i = if *last { s.rfind(*c) } else { s.find(*c) }?;
                (i > 0).then(|| s[..i].to_owned())
            }
            Part::After(c, last) => {
                let i = if *last { s.rfind(*c) } else { s.find(*c) }?;
                let rest = &s[i + c.len_utf8()..];
                (!rest.is_empty()).then(|| rest.to_owned())
            }
            Part::Chars(start, count) => {
                let v: String = s
                    .chars()
                    .skip(usize::from(*start))
                    .take(usize::from(*count))
                    .collect();
                (v.chars().count() == usize::from(*count)).then_some(v)
            }
        }
    }

    /// The parts a text has, with their values.
    fn all(s: &str) -> Vec<(Part, String)> {
        let mut out = Vec::new();
        let mut push = |p: Part| {
            if let Some(v) = p.eval(s) {
                out.push((p, v));
            }
        };
        push(Part::Whole);
        for k in [Tokens::Words, Tokens::Letters, Tokens::Digits] {
            let n = i32::try_from(tokens(s, k).len().min(12)).unwrap_or(0);
            for i in 0..n {
                push(Part::Token(k, i));
                push(Part::Token(k, i - n));
                push(Part::Initial(k, i));
                push(Part::Initial(k, i - n));
                if i > 0 {
                    push(Part::From(k, i));
                }
            }
        }
        // Fixed positions inside a run of digits (phone numbers, codes).
        let len = s.chars().count().min(24);
        if s.chars().take(len).all(|c| c.is_ascii_digit()) {
            for start in 0..len {
                for count in 1..=(len - start).min(12) {
                    push(Part::Chars(
                        u8::try_from(start).unwrap_or(0),
                        u8::try_from(count).unwrap_or(0),
                    ));
                }
            }
        }
        let mut seps: Vec<char> = s.chars().filter(|c| !c.is_alphanumeric()).collect();
        seps.sort_unstable();
        seps.dedup();
        for c in seps.into_iter().take(8) {
            for last in [false, true] {
                push(Part::Before(c, last));
                push(Part::After(c, last));
            }
        }
        out
    }
}

fn run(program: &[Piece], row: &[String]) -> Option<String> {
    let mut out = String::new();
    for p in program {
        match p {
            Piece::Const(c) => out.push_str(c),
            Piece::Part { col, part, case } => {
                out.push_str(&case.apply(&part.eval(row.get(*col)?)?));
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The cost of a program: fewer pieces first; parts before constants; whole texts and words
/// before letters, digits and separators.
fn cost(p: &[Piece]) -> u32 {
    p.iter()
        .map(|x| match x {
            // Letters and digits typed as constants rarely generalise.
            Piece::Const(c) => {
                10 + c
                    .chars()
                    .map(|ch| if ch.is_alphanumeric() { 6 } else { 1 })
                    .sum::<u32>()
            }
            Piece::Part { part, case, .. } => {
                let base = match part {
                    Part::Whole => 4,
                    Part::Token(Tokens::Words, n) if *n >= 0 => 5,
                    Part::Token(Tokens::Words, _) => 6,
                    Part::Token(..) => 7,
                    Part::Initial(..) => 7,
                    Part::From(..) => 8,
                    Part::Before(..) | Part::After(..) => 9,
                    Part::Chars(..) => 10,
                };
                base + u32::from(*case != Case::Same)
            }
        })
        .sum::<u32>()
        + 5 * u32::try_from(p.len()).unwrap_or(99)
}

/// The programs that produce `out` from `row` (at most `limit`, roughly simplest first).
fn programs(row: &[String], out: &str, limit: usize) -> Vec<Vec<Piece>> {
    let chars: Vec<(usize, char)> = out.char_indices().collect();
    let n = chars.len();
    let byte = |i: usize| chars.get(i).map_or(out.len(), |c| c.0);
    // For each start position, the pieces that match there and where they end.
    let mut edges: Vec<Vec<(usize, Piece)>> = vec![Vec::new(); n];
    let parts: Vec<(usize, Part, String)> = row
        .iter()
        .enumerate()
        .flat_map(|(c, s)| Part::all(s).into_iter().map(move |(p, v)| (c, p, v)))
        .collect();
    for i in 0..n {
        let rest = &out[byte(i)..];
        for (col, part, v) in &parts {
            for case in [Case::Same, Case::Upper, Case::Lower, Case::Proper] {
                let t = case.apply(v);
                if case != Case::Same && t == *v {
                    continue;
                }
                if !t.is_empty() && rest.starts_with(&t) {
                    let end = i + t.chars().count();
                    edges[i].push((
                        end,
                        Piece::Part {
                            col: *col,
                            part: part.clone(),
                            case,
                        },
                    ));
                }
            }
        }
        // Constants: a run of separators, or any single character.
        let mut j = i;
        while j < n && !chars[j].1.is_alphanumeric() {
            j += 1;
        }
        if j > i {
            edges[i].push((j, Piece::Const(out[byte(i)..byte(j)].to_owned())));
        }
        edges[i].push((i + 1, Piece::Const(out[byte(i)..byte(i + 1)].to_owned())));
    }
    // Uniform-cost search over the positions: programs come out cheapest first.
    let mut found: Vec<Vec<Piece>> = Vec::new();
    let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(u32, usize, usize)>> =
        std::collections::BinaryHeap::new();
    // Partial programs by id: (position, pieces).
    let mut partial: Vec<(usize, Vec<Piece>)> = vec![(0, Vec::new())];
    heap.push(std::cmp::Reverse((0, 0, 0)));
    let mut steps = 0;
    while let Some(std::cmp::Reverse((c, at, id))) = heap.pop() {
        steps += 1;
        if steps > 30_000 || found.len() >= limit {
            break;
        }
        let prog = partial[id].1.clone();
        if at == n {
            // Adjacent constants merge.
            let mut merged: Vec<Piece> = Vec::new();
            for p in prog {
                match (merged.last_mut(), p) {
                    (Some(Piece::Const(a)), Piece::Const(b)) => a.push_str(&b),
                    (_, p) => merged.push(p),
                }
            }
            if !found.contains(&merged) {
                found.push(merged);
            }
            continue;
        }
        if prog.len() >= 8 {
            continue;
        }
        for (end, piece) in &edges[at] {
            let mut p = prog.clone();
            let step = cost(std::slice::from_ref(piece));
            p.push(piece.clone());
            partial.push((*end, p));
            heap.push(std::cmp::Reverse((c + step, *end, partial.len() - 1)));
        }
    }
    found.sort_by_key(|p| cost(p));
    found
}

/// The texts Flash Fill gives `targets` (`None` where the program has no answer), from
/// `examples` (source columns, output); `None` when no program fits every example.
pub(crate) fn infer(
    examples: &[(Vec<String>, String)],
    targets: &[Vec<String>],
) -> Option<Vec<Option<String>>> {
    let (first_row, first_out) = examples.first()?;
    let candidates = programs(first_row, first_out, 400);
    let program = candidates.into_iter().find(|p| {
        examples
            .iter()
            .skip(1)
            .all(|(row, out)| run(p, row).as_deref() == Some(out.as_str()))
            // A program that only copies constants is no pattern.
            && p.iter().any(|x| matches!(x, Piece::Part { .. }))
    })?;
    Some(targets.iter().map(|t| run(&program, t)).collect())
}

/// What Flash Fill would do at a cell: the rows of the list, the source columns, the examples
/// and the cells to fill.
struct Plan {
    targets: Vec<(u32, String)>,
}

impl GridDoc {
    /// The plan for the column of `at`: the list is the rows beside it that have data in the
    /// neighbouring columns; examples are its filled cells, targets its empty ones.
    fn flash_plan(&self, sid: SheetId, at: CellPos) -> Option<Plan> {
        let col = at.col;
        let has = |r: u32, c: u32| !self.workbook.value(sid, r, c).is_empty_text_or_blank();
        // Source columns: the contiguous data columns to the left, then to the right.
        let mut cols: Vec<u32> = Vec::new();
        let mut c = col;
        while c > 0 && cols.len() < 16 {
            c -= 1;
            if (at.row.saturating_sub(3)..=at.row + 3).any(|r| has(r, c)) {
                cols.push(c);
            } else {
                break;
            }
        }
        cols.reverse();
        let mut c = col + 1;
        while c < kiln_sheet::MAX_COLS && cols.len() < 16 {
            if (at.row.saturating_sub(3)..=at.row + 3).any(|r| has(r, c)) {
                cols.push(c);
                c += 1;
            } else {
                break;
            }
        }
        if cols.is_empty() {
            return None;
        }
        let row_has_source = |r: u32| cols.iter().any(|&c| has(r, c));
        // The list's rows around the active cell; a header row (text above the examples with no
        // pattern) is left out by needing source data and treating the top row as a header when
        // the column's top cell is its title.
        let mut top = at.row;
        while top > 0 && row_has_source(top - 1) {
            top -= 1;
        }
        let mut bottom = at.row;
        while bottom + 1 < kiln_sheet::MAX_ROWS
            && row_has_source(bottom + 1)
            && bottom - top < 100_000
        {
            bottom += 1;
        }
        let row_of =
            |r: u32| -> Vec<String> { cols.iter().map(|&c| self.shown_text(sid, r, c)).collect() };
        let mut examples = Vec::new();
        let mut targets = Vec::new();
        for r in top..=bottom {
            if has(r, col) {
                let out = self.workbook.value(sid, r, col).display_general();
                examples.push((r, row_of(r), out));
            } else {
                targets.push(r);
            }
        }
        // The column's title: the top row when it is filled and the next one is the first
        // example (Excel does not learn from headers).
        if examples.len() > 1 && examples[0].0 == top && top < at.row {
            let header_like = self
                .workbook
                .sheet(sid)
                .is_some_and(|s| s.style_at(top, col) != s.style_at(top + 1, col))
                || examples[0].2.chars().all(|c| c.is_alphabetic() || c == ' ')
                    && examples[0]
                        .1
                        .iter()
                        .all(|s| s.chars().all(|c| c.is_alphabetic() || c == ' '))
                    && examples.len() > 1
                    && infer(
                        &examples[1..]
                            .iter()
                            .map(|e| (e.1.clone(), e.2.clone()))
                            .collect::<Vec<_>>(),
                        &[examples[0].1.clone()],
                    )
                    .is_some_and(|v| v[0].as_deref() != Some(examples[0].2.as_str()));
            if header_like {
                examples.remove(0);
            }
        }
        if examples.is_empty() || targets.is_empty() {
            return None;
        }
        let ex: Vec<(Vec<String>, String)> = examples
            .iter()
            .map(|e| (e.1.clone(), e.2.clone()))
            .collect();
        let rows: Vec<Vec<String>> = targets.iter().map(|&r| row_of(r)).collect();
        let out = infer(&ex, &rows)?;
        let targets: Vec<(u32, String)> = targets
            .into_iter()
            .zip(out)
            .filter_map(|(r, v)| v.map(|v| (r, v)))
            .collect();
        (!targets.is_empty()).then_some(Plan { targets })
    }

    /// Flash Fill.
    pub(crate) fn flash_fill(&mut self) -> Result<Change> {
        let sid = self.sid()?;
        let at = self.view().sel.active;
        let plan = self.flash_plan(sid, at).ok_or_else(|| {
            KilnError::InvalidInput(
                "Flash Fill didn't recognise a pattern. Type a few more examples in the column \
                 beside your data, then try again."
                    .into(),
            )
        })?;
        let n = plan.targets.len();
        let r0 = plan.targets.first().map_or(at.row, |t| t.0);
        let r1 = plan.targets.last().map_or(at.row, |t| t.0);
        let cells: Vec<CellInput> = plan
            .targets
            .into_iter()
            .map(|(r, v)| {
                // Numbers stay numbers (Excel fills text, but digits it reads as numbers).
                let value = v
                    .parse::<f64>()
                    .ok()
                    .filter(|_| !v.starts_with('0') || v == "0" || v.contains('.'))
                    .map_or_else(|| CellValue::text(v.clone()), CellValue::number);
                CellInput::value(r, at.col, value)
            })
            .collect();
        self.exec(
            "grid.editing.flash_fill",
            Command::SetCells { sheet: sid, cells },
        )?;
        self.analysis.flash.clear();
        self.features.message = Some(format!(
            "Flash Fill changed {n} cell{}",
            if n == 1 { "" } else { "s" }
        ));
        // Excel selects the filled cells.
        self.select_range_of(Range::new(r0, at.col, r1, at.col), at);
        Ok(Change::Content)
    }

    /// After an entry: Flash Fill's suggestions for its column, shown until the next command.
    pub(crate) fn flash_suggest(&mut self, at: CellPos) {
        self.analysis.flash.clear();
        let Ok(sid) = self.sid() else { return };
        // Only from two examples on (one is too often a coincidence), within reach.
        let examples = (at.row.saturating_sub(2)..at.row)
            .filter(|&r| !self.workbook.value(sid, r, at.col).is_empty_text_or_blank())
            .count();
        if examples == 0
            || self
                .workbook
                .value(sid, at.row, at.col)
                .is_empty_text_or_blank()
        {
            return;
        }
        let Some(plan) = self.flash_plan(sid, at) else {
            return;
        };
        if plan.targets.iter().all(|t| t.0 < at.row) {
            return;
        }
        self.analysis.flash = plan
            .targets
            .into_iter()
            .filter(|t| t.0 > at.row)
            .map(|(row, text)| GridFlashCell {
                row,
                col: at.col,
                text,
                rect: [0.0; 4],
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(examples: &[(&[&str], &str)], targets: &[&[&str]]) -> Vec<Option<String>> {
        let ex: Vec<(Vec<String>, String)> = examples
            .iter()
            .map(|(r, o)| (r.iter().map(|s| (*s).to_owned()).collect(), (*o).to_owned()))
            .collect();
        let t: Vec<Vec<String>> = targets
            .iter()
            .map(|r| r.iter().map(|s| (*s).to_owned()).collect())
            .collect();
        infer(&ex, &t).unwrap_or_default()
    }

    fn some(v: &[&str]) -> Vec<Option<String>> {
        v.iter().map(|s| Some((*s).to_owned())).collect()
    }

    #[test]
    fn splits_names_and_emails() {
        assert_eq!(
            fill(
                &[(&["Ana Lima"], "Ana")],
                &[&["Ben Okoro"], &["Caro de Wit"]]
            ),
            some(&["Ben", "Caro"])
        );
        assert_eq!(
            fill(
                &[(&["Ana Lima"], "Lima"), (&["Caro de Wit"], "Wit")],
                &[&["Ben Okoro"], &["Dev Patel"]]
            ),
            some(&["Okoro", "Patel"])
        );
        assert_eq!(
            fill(
                &[(&["ana.lima@kiln.example"], "ana.lima")],
                &[&["ben@kiln.example"]]
            ),
            some(&["ben"])
        );
    }

    #[test]
    fn joins_columns_and_changes_case() {
        assert_eq!(
            fill(
                &[(&["ana", "lima"], "Lima, Ana")],
                &[&["ben", "okoro"], &["fay", "ng"]]
            ),
            some(&["Okoro, Ben", "Ng, Fay"])
        );
        assert_eq!(
            fill(
                &[(&["Ana Lima"], "AL"), (&["Ben Okoro"], "BO")],
                &[&["Caro Wit"]]
            ),
            some(&["CW"])
        );
        assert_eq!(
            fill(&[(&["kiln grid"], "KILN GRID")], &[&["pivot table"]]),
            some(&["PIVOT TABLE"])
        );
    }

    #[test]
    fn number_patterns() {
        assert_eq!(
            fill(&[(&["5551234567"], "(555) 123-4567")], &[&["2125550199"]]),
            some(&["(212) 555-0199"])
        );
        assert_eq!(
            fill(
                &[
                    (&["Order 1042 (paid)"], "1042"),
                    (&["Order 77 (open)"], "77")
                ],
                &[&["Order 3 (paid)"]]
            ),
            some(&["3"])
        );
        assert_eq!(
            fill(&[(&["2026-10-18"], "18/10/2026")], &[&["2025-01-02"]]),
            some(&["02/01/2025"])
        );
    }

    #[test]
    fn missing_parts_stay_empty() {
        assert_eq!(
            fill(
                &[(&["Ana Maria Lima"], "Maria")],
                &[&["Ben Okoro Xu"], &["Caro"]]
            ),
            vec![Some("Okoro".to_owned()), None]
        );
    }
}
