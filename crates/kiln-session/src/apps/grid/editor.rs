// SPDX-License-Identifier: AGPL-3.0-or-later
//! In-cell editing (Excel's Enter, Edit and Point modes): starting an entry, the editor's text
//! from the input proxy or the formula bar, committing with movement, Ctrl+Enter, autocomplete of
//! function names (with their documentation) and defined names, AutoComplete of text from the
//! column, the argument hint, F4's `$` cycle, point mode (clicks and arrows insert references),
//! the pick list, AutoSum and the date and time.

use kiln_common::{KilnError, Result};
use kiln_formula::parser::{ParseOptions, TokenKind, tokenize};
use kiln_formula::values::number_to_text;
use kiln_protocol::commands::{
    CompletionKind, GridCommand, GridCompletion, GridDirection, GridFormulaHint, GridHintArg,
};
use kiln_sheet::address::{cell_a1, parse_cell_a1};
use kiln_sheet::commands::{CellInput, Content};
use kiln_sheet::styles::StylePatch;
use kiln_sheet::{CellPos, CellValue, Range, SheetId};

use super::GridDoc;
use crate::model::Change;

/// The editor's mode (the status bar shows it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Typing replaced the cell; arrows commit and move (or point in a formula).
    Enter,
    /// F2 or a click in the formula bar: arrows move the caret.
    Edit,
    /// Pointing at cells for a formula.
    Point,
}

/// A reference inserted by pointing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointRef {
    /// Byte span of the reference in the text.
    pub span: (usize, usize),
    /// Where the pointed range starts.
    pub anchor: CellPos,
    /// Its moving corner.
    pub head: CellPos,
}

/// The entry being typed.
#[derive(Debug, Clone, PartialEq)]
pub struct EditState {
    /// The sheet.
    pub sheet: SheetId,
    /// Row of the edited cell.
    pub row: u32,
    /// Column of the edited cell.
    pub col: u32,
    /// The text.
    pub text: String,
    /// Selection start (bytes).
    pub start: usize,
    /// Selection end, the caret (bytes).
    pub end: usize,
    /// Mode.
    pub mode: EditMode,
    /// The formula bar is editing.
    pub formula_bar: bool,
    /// Suggestions.
    pub completions: Vec<GridCompletion>,
    /// The highlighted suggestion.
    pub completion_index: usize,
    /// AutoComplete's proposed rest of a text entry (shown selected after the caret).
    pub inline: Option<String>,
    /// The pointed reference.
    pub point: Option<PointRef>,
    /// The pick list, when open.
    pub pick_list: Option<Vec<String>>,
}

impl EditState {
    /// Whether the editor is pointing at cells.
    #[must_use]
    pub fn pointing(&self) -> bool {
        self.mode == EditMode::Point
    }

    /// The text with the inline completion.
    #[must_use]
    pub fn display_text(&self) -> String {
        match &self.inline {
            Some(rest) => format!("{}{rest}", self.text),
            None => self.text.clone(),
        }
    }

    /// The selection in the displayed text (an inline completion is selected).
    #[must_use]
    pub fn byte_selection(&self) -> (usize, usize) {
        match &self.inline {
            Some(rest) => (self.text.len(), self.text.len() + rest.len()),
            None => (self.start, self.end),
        }
    }

    fn is_formula(&self) -> bool {
        self.text.starts_with('=') || (self.text.starts_with('+') && self.text.len() > 1)
    }
}

/// UTF-16 offset → byte offset (clamped to a char boundary).
#[must_use]
pub fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut n = 0u32;
    for (i, c) in text.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16() as u32;
    }
    text.len()
}

/// Byte offset → UTF-16 offset.
#[must_use]
pub fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].encode_utf16().count() as u32
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '.' || c == '_'
}

/// The identifier being typed at `caret` in formula text: its byte start and text, when it can
/// be a function or a name (not inside a string, not a sheet-qualified reference).
fn token_at(text: &str, caret: usize) -> Option<(usize, &str)> {
    let caret = caret.min(text.len());
    if !text.is_char_boundary(caret) {
        return None;
    }
    // Inside a string literal?
    if text[..caret].matches('"').count() % 2 == 1 {
        return None;
    }
    let before = &text[..caret];
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_name_char(*c))
        .last()
        .map_or(caret, |(i, _)| i);
    let tok = &text[start..caret];
    let first = tok.chars().next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    let prev = text[..start].chars().next_back();
    if prev.is_some_and(|c| matches!(c, '!' | '\'' | '$' | ':') || c.is_alphanumeric()) {
        return None;
    }
    Some((start, tok))
}

/// The function call the caret is inside and the index of the argument it is in.
#[must_use]
pub fn call_at(text: &str, caret: usize) -> Option<(String, u32)> {
    let body_start = usize::from(text.starts_with('=') || text.starts_with('+'));
    let caret = caret.min(text.len());
    if !text.is_char_boundary(caret) || caret < body_start {
        return None;
    }
    let toks = tokenize(
        &text[body_start..caret],
        &ParseOptions::default(),
        body_start as u32,
    );
    let mut stack: Vec<(Option<String>, u32)> = Vec::new();
    let mut pending: Option<String> = None;
    for t in toks {
        match t.kind {
            TokenKind::Function(name) => pending = Some(name),
            TokenKind::LParen => stack.push((pending.take(), 0)),
            TokenKind::LBrace => {
                pending = None;
                stack.push((None, 0));
            }
            TokenKind::RParen | TokenKind::RBrace => {
                pending = None;
                stack.pop();
            }
            TokenKind::Comma => {
                pending = None;
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            TokenKind::Whitespace => {}
            _ => pending = None,
        }
    }
    // A function name typed with its parenthesis counts at once; the innermost named call wins.
    stack
        .into_iter()
        .rev()
        .find_map(|(name, arg)| name.map(|n| (n.trim_start_matches("_xlfn.").to_owned(), arg)))
}

/// The argument hint for formula text with the caret at a byte offset.
#[must_use]
pub fn formula_hint(text: &str, caret: usize) -> Option<GridFormulaHint> {
    let (name, arg) = call_at(text, caret)?;
    let doc = kiln_formula::functions::doc(&name)?;
    let n = doc.args.len() as u32;
    let current = if n == 0 {
        0
    } else if arg >= n {
        // Repeating arguments (`number1, number2, …`) map to the last entry.
        if doc.args.last().is_some_and(|a| a.repeating) {
            n - 1
        } else {
            arg
        }
    } else {
        arg
    };
    Some(GridFormulaHint {
        function: doc.name.clone(),
        syntax: doc.syntax.clone(),
        summary: doc.summary.clone(),
        args: doc
            .args
            .iter()
            .map(|a| GridHintArg {
                name: a.name.clone(),
                description: a.description.clone(),
                optional: a.optional,
            })
            .collect(),
        current,
        since: doc.since.clone(),
    })
}

/// Closes the parentheses a formula left open (Excel's formula autocorrect).
#[must_use]
pub fn close_parens(text: &str) -> String {
    if !(text.starts_with('=') || text.starts_with('+')) {
        return text.to_owned();
    }
    let mut depth: i64 = 0;
    let mut quoted = false;
    let mut sheet_quote = false;
    for c in text.chars() {
        match c {
            '"' if !sheet_quote => quoted = !quoted,
            '\'' if !quoted => sheet_quote = !sheet_quote,
            '(' if !quoted && !sheet_quote => depth += 1,
            ')' if !quoted && !sheet_quote => depth -= 1,
            _ => {}
        }
    }
    let mut out = text.to_owned();
    if quoted {
        out.push('"');
    }
    for _ in 0..depth.clamp(0, 64) {
        out.push(')');
    }
    out
}

/// Whether the formula text expects a reference at the caret (after `=`, `(`, `,`, an
/// operator), so arrows and clicks point.
#[must_use]
pub fn expects_reference(text: &str, caret: usize) -> bool {
    if !(text.starts_with('=') || text.starts_with('+')) {
        return false;
    }
    let before = text[..caret.min(text.len())].trim_end();
    before.chars().next_back().is_some_and(|c| {
        matches!(
            c,
            '=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';'
        )
    })
}

impl GridDoc {
    /// What the formula bar shows for a cell (and what F2 edits).
    pub(crate) fn edit_text(&self, sheet: SheetId, row: u32, col: u32) -> String {
        if let Some(f) = self.workbook.formula(sheet, row, col) {
            return if f.starts_with('=') {
                f
            } else {
                format!("={f}")
            };
        }
        let Some(s) = self.workbook.sheet(sheet) else {
            return String::new();
        };
        let style = s.style_at(row, col);
        let code = self.workbook.styles().num_fmt(style).to_owned();
        let fmt = kiln_formula::numfmt::NumberFormat::parse(&code);
        match self.workbook.value(sheet, row, col) {
            CellValue::Blank => String::new(),
            CellValue::Number(n) => {
                if fmt.is_date() {
                    let has_date = n.abs() >= 1.0;
                    let has_time = n.fract() != 0.0;
                    let pattern = match (has_date, has_time) {
                        (true, true) => "m/d/yyyy h:mm:ss AM/PM",
                        (true, false) => "m/d/yyyy",
                        (false, _) => "h:mm:ss AM/PM",
                    };
                    kiln_sheet::format::format_value(
                        &CellValue::Number(n),
                        pattern,
                        self.workbook.props().date1904,
                    )
                    .text
                } else if fmt.is_percent() {
                    format!("{}%", number_to_text(n * 100.0))
                } else {
                    number_to_text(n)
                }
            }
            CellValue::Text(t) => {
                // Text that typing would turn into something else keeps its apostrophe, so F2
                // and Enter never change the value.
                let s = t.trim();
                let ambiguous = s.parse::<f64>().is_ok()
                    || s.eq_ignore_ascii_case("true")
                    || s.eq_ignore_ascii_case("false")
                    || s.starts_with('=')
                    || (s.starts_with('#') && s.ends_with(['!', '?', 'A']))
                    || (s.ends_with('%') && s[..s.len() - 1].trim().parse::<f64>().is_ok());
                if self.workbook.styles().xf(style).quote_prefix || ambiguous {
                    format!("'{t}")
                } else {
                    t.to_string()
                }
            }
            CellValue::RichText(r) => r.plain().to_string(),
            CellValue::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_owned(),
            CellValue::Error(e) => e.code().to_owned(),
        }
    }

    fn start_edit(
        &mut self,
        text: String,
        mode: super::editor::EditMode,
        formula_bar: bool,
    ) -> Result<()> {
        let sid = self.sid()?;
        let v = self.view();
        let mut at = v.sel.active;
        if let Some(m) = self.geometry().merge_at(at.row, at.col) {
            at = CellPos::new(m.r0, m.c0);
        }
        let end = text.len();
        self.edit = Some(EditState {
            sheet: sid,
            row: at.row,
            col: at.col,
            text,
            start: end,
            end,
            mode,
            formula_bar,
            completions: Vec::new(),
            completion_index: 0,
            inline: None,
            point: None,
            pick_list: None,
        });
        self.fill_preview = None;
        self.refresh_completions(true);
        Ok(())
    }

    /// Recomputes the suggestions after the text changed (`typed`: the change was typing at the
    /// end, which may propose an inline completion).
    fn refresh_completions(&mut self, typed: bool) {
        let Some(e) = &self.edit else {
            return;
        };
        let mut completions = Vec::new();
        let mut inline = None;
        if e.is_formula() {
            if let Some((_, tok)) = token_at(&e.text, e.end) {
                let prefix = tok.to_uppercase();
                let mut funcs: Vec<&kiln_formula::functions::FunctionDoc> =
                    kiln_formula::functions::docs()
                        .iter()
                        .filter(|d| d.name.starts_with(&prefix))
                        .collect();
                funcs.sort_by(|a, b| a.name.cmp(&b.name));
                for d in funcs.into_iter().take(40) {
                    completions.push(GridCompletion {
                        label: d.name.clone(),
                        kind: CompletionKind::Function,
                        detail: Some(d.summary.clone()),
                        syntax: Some(d.syntax.clone()),
                    });
                }
                let mut names: Vec<String> = self
                    .workbook
                    .names()
                    .iter()
                    .filter(|n| {
                        !n.hidden
                            && n.name.to_uppercase().starts_with(&prefix)
                            && !n.name.starts_with("_xlnm")
                    })
                    .map(|n| n.name.clone())
                    .collect();
                for s in self.workbook.sheets() {
                    for t in s.tables() {
                        if t.name.to_uppercase().starts_with(&prefix) {
                            names.push(t.name.clone());
                        }
                    }
                }
                names.sort();
                names.dedup();
                for n in names.into_iter().take(20) {
                    completions.push(GridCompletion {
                        label: n,
                        kind: CompletionKind::Name,
                        detail: None,
                        syntax: None,
                    });
                }
            }
        } else if typed
            && e.mode == EditMode::Enter
            && e.start == e.end
            && e.end == e.text.len()
            && !e.text.is_empty()
            && e.text.parse::<f64>().is_err()
        {
            inline = self.autocomplete_value(e.sheet, e.row, e.col, &e.text);
        }
        if let Some(e) = &mut self.edit {
            e.completion_index = 0;
            e.completions = completions;
            e.inline = inline;
        }
    }

    /// The text values of the column's data block around a cell.
    fn column_values(&self, sheet: SheetId, row: u32, col: u32, limit: u32) -> Vec<String> {
        let mut out = Vec::new();
        let value = |r: u32| self.workbook.value(sheet, r, col);
        let mut r = row;
        let mut n = 0;
        while r > 0 && n < limit {
            r -= 1;
            let v = value(r);
            if v.is_blank() {
                break;
            }
            if let Some(t) = v.as_text() {
                out.push(t.to_owned());
            }
            n += 1;
        }
        let mut r = row;
        n = 0;
        while r + 1 < kiln_sheet::MAX_ROWS && n < limit {
            r += 1;
            let v = value(r);
            if v.is_blank() {
                break;
            }
            if let Some(t) = v.as_text() {
                out.push(t.to_owned());
            }
            n += 1;
        }
        out
    }

    /// Excel's AutoComplete: the rest of the one text value of the column that starts with what
    /// was typed (case-insensitive).
    fn autocomplete_value(
        &self,
        sheet: SheetId,
        row: u32,
        col: u32,
        typed: &str,
    ) -> Option<String> {
        let lower = typed.to_lowercase();
        let mut matches: Vec<String> = self
            .column_values(sheet, row, col, 5_000)
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&lower) && v.len() > typed.len())
            .collect();
        matches.sort_by_key(|v| v.to_lowercase());
        matches.dedup_by_key(|v| v.to_lowercase());
        if matches.len() != 1 {
            return None;
        }
        let m = &matches[0];
        // Keep what was typed, complete with the rest of the value.
        let rest: String = m.chars().skip(typed.chars().count()).collect();
        (!rest.is_empty()).then_some(rest)
    }

    /// Commits the entry. `advance` moves the active cell (within a multi-cell selection, as
    /// Enter does).
    pub(crate) fn commit_edit(&mut self, advance: Option<GridDirection>) -> Result<()> {
        let Some(e) = self.edit.clone() else {
            return Ok(());
        };
        let text = close_parens(&e.display_text());
        self.validate_entry(&e, &text)?;
        self.edit = None;
        let cmd = kiln_sheet::Command::SetCells {
            sheet: e.sheet,
            cells: vec![CellInput {
                row: e.row,
                col: e.col,
                content: if text.is_empty() {
                    Content::Clear
                } else {
                    Content::Typed(text.clone())
                },
                style: None,
            }],
        };
        self.exec("grid.editing.commit", cmd)?;
        if text.contains('\n') {
            self.wrap_and_fit(e.sheet, e.row, e.col)?;
        } else {
            self.autofit_row_if_auto(e.row);
        }
        if let Some(dir) = advance {
            self.advance_within_selection(dir);
        }
        Ok(())
    }

    /// Data validation of an entry: a "Stop" rule refuses invalid input.
    fn validate_entry(&mut self, e: &EditState, text: &str) -> Result<()> {
        let has_rule = self
            .workbook
            .sheet(e.sheet)
            .is_some_and(|s| s.validation_at(e.row, e.col).is_some());
        if !has_rule || text.starts_with('=') {
            return Ok(());
        }
        let value = match text.trim().parse::<f64>() {
            Ok(n) => CellValue::Number(n),
            Err(_) if text.is_empty() => CellValue::Blank,
            Err(_) => CellValue::text(text),
        };
        match self.workbook.validate(e.sheet, e.row, e.col, &value) {
            kiln_sheet::validation::Validity::Invalid {
                style: kiln_sheet::meta::ErrorStyle::Stop,
                message,
                ..
            } => Err(KilnError::InvalidInput(if message.is_empty() {
                "This value doesn't match the data validation restrictions defined for this cell."
                    .into()
            } else {
                message
            })),
            _ => Ok(()),
        }
    }

    /// A line break in a cell turns on Wrap Text (as Excel does) and fits the row.
    fn wrap_and_fit(&mut self, sheet: SheetId, row: u32, col: u32) -> Result<()> {
        let wraps = self.workbook.sheet(sheet).is_some_and(|s| {
            self.workbook
                .styles()
                .xf(s.style_at(row, col))
                .alignment
                .wrap_text
        });
        if !wraps {
            let patch = StylePatch {
                alignment: Some(kiln_sheet::styles::AlignmentPatch {
                    wrap_text: Some(true),
                    ..Default::default()
                }),
                ..StylePatch::default()
            };
            self.exec(
                "grid.editing.commit",
                kiln_sheet::Command::SetStyle {
                    sheet,
                    ranges: vec![Range::cell(row, col)],
                    patch: Box::new(patch),
                },
            )?;
        }
        self.autofit_row_if_auto(row);
        Ok(())
    }

    /// Moves the active cell after an entry: inside a multi-cell selection it cycles through
    /// it; otherwise it moves one cell.
    pub(crate) fn advance_within_selection(&mut self, dir: GridDirection) {
        let v = self.view();
        let multi =
            v.sel.ranges.len() > 1 || v.sel.ranges.last().is_some_and(|r| !r.is_single_cell());
        if multi {
            let (backward, across) = match dir {
                GridDirection::Down => (false, false),
                GridDirection::Up => (true, false),
                GridDirection::Right => (false, true),
                GridDirection::Left => (true, true),
            };
            self.advance(backward, across);
        } else {
            self.step_single(dir);
        }
    }

    fn insert_at_caret(&mut self, s: &str) {
        if let Some(e) = &mut self.edit {
            let (a, b) = (e.start.min(e.end), e.start.max(e.end));
            e.text.replace_range(a..b, s);
            e.start = a + s.len();
            e.end = e.start;
            e.inline = None;
            e.point = None;
            if e.mode == EditMode::Point {
                e.mode = EditMode::Enter;
            }
        }
    }

    /// Inserts or moves the pointed reference.
    fn point_at(&mut self, row: u32, col: u32, extend: bool) {
        let shown = self.sid().ok();
        let shown_name = self.active_sheet().map(|(_, s)| s.name().to_owned());
        let Some(e) = &mut self.edit else {
            return;
        };
        let (anchor, head) = match (e.point, extend) {
            (Some(p), true) => (p.anchor, CellPos::new(row, col)),
            _ => (CellPos::new(row, col), CellPos::new(row, col)),
        };
        let r = Range::new(anchor.row, anchor.col, head.row, head.col);
        let mut text_ref = if r.is_single_cell() {
            cell_a1(r.r0, r.c0, false)
        } else {
            r.to_a1(false)
        };
        // A reference to another sheet carries its name.
        if shown.is_some_and(|sid| sid != e.sheet)
            && let Some(name) = shown_name
        {
            text_ref = format!("{}!{text_ref}", quote_sheet_name(&name));
        }
        let Some(e) = &mut self.edit else {
            return;
        };
        let (a, b) = match e.point {
            Some(p) => p.span,
            None => (e.start.min(e.end), e.start.max(e.end)),
        };
        e.text.replace_range(a..b, &text_ref);
        let end = a + text_ref.len();
        e.start = end;
        e.end = end;
        e.point = Some(PointRef {
            span: (a, end),
            anchor,
            head,
        });
        e.mode = EditMode::Point;
        e.inline = None;
        e.completions.clear();
    }

    /// The editor commands.
    pub(crate) fn editing(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        match cmd {
            G::EditCell => {
                if let Some(e) = &mut self.edit {
                    // F2 while editing toggles Enter and Edit modes, as in Excel.
                    e.mode = if e.mode == EditMode::Edit {
                        EditMode::Enter
                    } else {
                        EditMode::Edit
                    };
                    return Ok(Change::Selection);
                }
                let sid = self.sid()?;
                let mut at = self.view().sel.active;
                if let Some(m) = self.geometry().merge_at(at.row, at.col) {
                    at = CellPos::new(m.r0, m.c0);
                }
                let text = self.edit_text(sid, at.row, at.col);
                self.start_edit(text, EditMode::Edit, false)?;
            }
            G::BeginEdit { text } => {
                if self.edit.is_some() {
                    self.insert_at_caret(text);
                } else {
                    self.start_edit(text.clone(), EditMode::Enter, false)?;
                }
                self.refresh_completions(true);
            }
            G::SetEditText {
                text,
                start,
                end,
                formula_bar,
            } => {
                if text.len() > 32_767 * 4 {
                    return Err(KilnError::InvalidInput(
                        "a cell holds at most 32,767 characters".into(),
                    ));
                }
                if self.edit.is_none() {
                    let mode = if *formula_bar {
                        EditMode::Edit
                    } else {
                        EditMode::Enter
                    };
                    self.start_edit(String::new(), mode, *formula_bar)?;
                }
                let typed = self.edit.as_ref().is_some_and(|e| {
                    text.len() > e.text.len() && text.starts_with(e.text.as_str())
                });
                if let Some(e) = &mut self.edit {
                    let changed = e.text != *text;
                    e.text.clone_from(text);
                    e.start = utf16_to_byte(text, *start);
                    e.end = utf16_to_byte(text, *end);
                    e.formula_bar = *formula_bar;
                    if changed {
                        e.point = None;
                        e.pick_list = None;
                        if e.mode == EditMode::Point {
                            e.mode = EditMode::Enter;
                        }
                    }
                }
                self.refresh_completions(typed);
            }
            G::CommitEdit { advance } => {
                if self.edit.is_some() {
                    self.commit_edit(*advance)?;
                } else if let Some(dir) = advance {
                    self.advance_within_selection(*dir);
                }
            }
            G::FillSelection => self.fill_selection(false)?,
            G::ArrayFormula => self.fill_selection(true)?,
            G::CancelEdit => {
                if self.edit.take().is_none() {
                    self.copy = None;
                    self.fill_preview = None;
                }
            }
            G::NewLineInCell => {
                if self.edit.is_none() {
                    let sid = self.sid()?;
                    let a = self.view().sel.active;
                    let text = self.edit_text(sid, a.row, a.col);
                    self.start_edit(text, EditMode::Edit, false)?;
                }
                self.insert_at_caret("\n");
            }
            G::ToggleAbsolute => {
                let Some(e) = &mut self.edit else {
                    return Ok(Change::None);
                };
                if !e.is_formula() {
                    return Ok(Change::None);
                }
                let r = kiln_formula::refs::toggle_absolute(
                    &e.text,
                    e.start.min(e.end),
                    e.start.max(e.end),
                    &ParseOptions::default(),
                );
                e.text = r.text;
                e.start = r.selection_start;
                e.end = r.selection_end;
                e.point = None;
            }
            G::AcceptCompletion { index } => self.accept_completion(*index as usize)?,
            G::MoveCompletion { down } => {
                if let Some(e) = &mut self.edit {
                    let n = e.completions.len().max(1);
                    e.completion_index = if *down {
                        (e.completion_index + 1) % n
                    } else {
                        (e.completion_index + n - 1) % n
                    };
                    if let Some(list) = &e.pick_list {
                        let _ = list;
                    }
                }
            }
            G::PointRef { row, col, extend } => {
                let can_point = self.edit.as_ref().is_some_and(|e| {
                    e.pointing() || (e.is_formula() && expects_reference(&e.text, e.end))
                });
                if can_point {
                    self.point_at(*row, *col, *extend);
                } else {
                    if self.edit.is_some() {
                        self.commit_edit(None)?;
                    }
                    self.select_cell(*row, *col, *extend, false);
                }
            }
            G::PointMove { dir, extend } => {
                let Some(e) = self.edit.clone() else {
                    self.move_active(*dir, *extend);
                    return Ok(Change::Selection);
                };
                if e.mode == EditMode::Edit {
                    return Ok(Change::None);
                }
                if e.pointing() || (e.is_formula() && expects_reference(&e.text, e.end)) {
                    // The pointed reference's moving corner, or the active cell for a new one.
                    let from = e.point.map_or(self.view().sel.active, |p| p.head);
                    let geo = self.geometry().clone();
                    let (r, c) = step(&geo, from, *dir);
                    self.point_at(r, c, *extend && e.point.is_some());
                } else {
                    self.commit_edit(Some(*dir))?;
                }
            }
            G::OpenDropdown => {
                let sid = self.sid()?;
                let a = self.view().sel.active;
                let list = self.pick_list(sid, a.row, a.col);
                if self.edit.is_none() {
                    let text = self.edit_text(sid, a.row, a.col);
                    self.start_edit(text, EditMode::Enter, false)?;
                }
                if let Some(e) = &mut self.edit {
                    e.pick_list = Some(list);
                }
            }
            G::InsertDateTime {
                time,
                utc_offset_minutes,
            } => self.insert_date_time(*time, *utc_offset_minutes)?,
            G::AutoSum { function } => self.auto_sum(function.as_deref().unwrap_or("SUM"))?,
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} in the Grid editor",
                    cmd.meta().id
                )));
            }
        }
        Ok(Change::Selection)
    }

    fn accept_completion(&mut self, index: usize) -> Result<()> {
        let Some(e) = &mut self.edit else {
            return Ok(());
        };
        if let Some(list) = e.pick_list.take() {
            let Some(item) = list.get(index) else {
                return Err(KilnError::InvalidInput(format!(
                    "no item {index} in the list"
                )));
            };
            e.text.clone_from(item);
            e.start = e.text.len();
            e.end = e.start;
            e.inline = None;
            return Ok(());
        }
        let Some(c) = e.completions.get(index).cloned() else {
            return Err(KilnError::InvalidInput(format!("no suggestion {index}")));
        };
        let Some((start, _)) = token_at(&e.text, e.end) else {
            return Ok(());
        };
        let end = e.end;
        let insert = match c.kind {
            CompletionKind::Function => {
                // Keep a parenthesis already there.
                if e.text[end..].starts_with('(') {
                    c.label.clone()
                } else {
                    format!("{}(", c.label)
                }
            }
            _ => c.label.clone(),
        };
        e.text.replace_range(start..end, &insert);
        e.start = start + insert.len();
        if c.kind == CompletionKind::Function
            && e.text[e.start..].starts_with('(')
            && !insert.ends_with('(')
        {
            e.start += 1;
        }
        e.end = e.start;
        e.completions.clear();
        e.inline = None;
        Ok(())
    }

    /// The pick list of a cell: its validation list, or the column's distinct text values.
    fn pick_list(&mut self, sheet: SheetId, row: u32, col: u32) -> Vec<String> {
        if let Some(list) = self.workbook.validation_list(sheet, row, col) {
            return list
                .into_iter()
                .map(|v| match v {
                    CellValue::Number(n) => number_to_text(n),
                    other => other.display_general(),
                })
                .collect();
        }
        let mut values = self.column_values(sheet, row, col, 10_000);
        values.sort_by_key(|v| v.to_lowercase());
        values.dedup_by_key(|v| v.to_lowercase());
        values
    }

    /// Ctrl+Enter (and Ctrl+Shift+Enter): the entry into every selected cell, formulas adjusted
    /// relative to the active cell.
    fn fill_selection(&mut self, array: bool) -> Result<()> {
        let sid = self.sid()?;
        let v = self.view();
        let Some(e) = self.edit.clone() else {
            // Without an entry: the active cell's content into the rest of the selection.
            if v.sel.ranges.iter().any(|r| r.cell_count() > 1_000_000) {
                return Err(KilnError::InvalidInput(
                    "the selection is too large to fill".into(),
                ));
            }
            let src = Range::cell(v.sel.active.row, v.sel.active.col);
            let cmds: Vec<kiln_sheet::Command> = v
                .sel
                .ranges
                .iter()
                .map(|r| kiln_sheet::Command::CopyRange {
                    sheet: sid,
                    range: src,
                    to_sheet: None,
                    to: *r,
                    what: kiln_sheet::commands::PasteWhat::Formulas,
                    op: kiln_sheet::commands::PasteOp::None,
                    transpose: false,
                    skip_blanks: false,
                })
                .collect();
            self.exec_all("grid.editing.fill_selection", cmds)?;
            return Ok(());
        };
        let text = close_parens(&e.display_text());
        self.validate_entry(&e, &text)?;
        self.edit = None;
        if array {
            let cmd = kiln_sheet::Command::SetCells {
                sheet: e.sheet,
                cells: vec![CellInput {
                    row: e.row,
                    col: e.col,
                    content: Content::Typed(text),
                    style: None,
                }],
            };
            self.exec("grid.editing.array_formula", cmd)?;
            return Ok(());
        }
        let formula = text.strip_prefix('=').filter(|_| text.len() > 1);
        let mut cells = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for r in &v.sel.ranges {
            if r.cell_count() > 1_000_000 {
                return Err(KilnError::InvalidInput(
                    "the selection is too large to fill".into(),
                ));
            }
            for p in r.cells() {
                if !seen.insert(p) {
                    continue;
                }
                let content = match formula {
                    Some(f) => {
                        let dr = i64::from(p.row) - i64::from(e.row);
                        let dc = i64::from(p.col) - i64::from(e.col);
                        let adjusted = kiln_formula::refs::copy_formula(f, dr, dc)
                            .unwrap_or_else(|_| f.to_owned());
                        Content::Typed(format!("={adjusted}"))
                    }
                    None if text.is_empty() => Content::Clear,
                    None => Content::Typed(text.clone()),
                };
                cells.push(CellInput {
                    row: p.row,
                    col: p.col,
                    content,
                    style: None,
                });
            }
        }
        self.exec(
            "grid.editing.fill_selection",
            kiln_sheet::Command::SetCells { sheet: sid, cells },
        )?;
        Ok(())
    }

    /// Ctrl+; and Ctrl+Shift+;: while typing, the text; otherwise the value with a date or time
    /// format.
    fn insert_date_time(&mut self, time: bool, offset_minutes: i32) -> Result<()> {
        let ms = self.now_ms as f64 + f64::from(offset_minutes.clamp(-24 * 60, 24 * 60)) * 60_000.0;
        let serial = ms / 86_400_000.0 + 25_569.0;
        let (value, code) = if time {
            // Excel's Ctrl+Shift+; puts the time of day (to the minute).
            let t = (serial.fract() * 1440.0).floor() / 1440.0;
            (t, "h:mm AM/PM")
        } else {
            (serial.floor(), "m/d/yyyy")
        };
        let text = kiln_sheet::format::format_value(&CellValue::Number(value), code, false).text;
        if self.edit.is_some() {
            self.insert_at_caret(&text);
            return Ok(());
        }
        let sid = self.sid()?;
        let a = self.view().sel.active;
        let cmds = vec![
            kiln_sheet::Command::SetCells {
                sheet: sid,
                cells: vec![CellInput::value(a.row, a.col, CellValue::Number(value))],
            },
            kiln_sheet::Command::SetStyle {
                sheet: sid,
                ranges: vec![Range::cell(a.row, a.col)],
                patch: Box::new(StylePatch {
                    num_fmt: Some(code.to_owned()),
                    ..StylePatch::default()
                }),
            },
        ];
        self.exec_all("grid.editing.insert_date_time", cmds)?;
        Ok(())
    }

    /// AutoSum: below a selected block of numbers, the totals of each column; for one cell, the
    /// formula over the numbers above it (or left of it), ready to confirm with Enter.
    fn auto_sum(&mut self, function: &str) -> Result<()> {
        let function = function.to_uppercase();
        if !["SUM", "AVERAGE", "COUNT", "MAX", "MIN"].contains(&function.as_str()) {
            return Err(KilnError::InvalidInput(format!(
                "AutoSum does not offer {function}"
            )));
        }
        let sid = self.sid()?;
        if let Some(e) = &self.edit {
            if e.is_formula() || e.text.is_empty() {
                let insert = format!("{function}(");
                let text = if e.text.is_empty() {
                    format!("={insert}")
                } else {
                    insert
                };
                self.insert_at_caret(&text);
            }
            return Ok(());
        }
        let v = self.view();
        let r = *v
            .sel
            .ranges
            .last()
            .ok_or_else(|| KilnError::InvalidInput("nothing is selected".into()))?;
        let is_number =
            |row: u32, col: u32| matches!(self.workbook.value(sid, row, col), CellValue::Number(_));
        if !r.is_single_cell() {
            // Totals under each column of the block (Excel's AutoSum over a selected range).
            let below = r.r1 + 1;
            if below >= kiln_sheet::MAX_ROWS {
                return Err(KilnError::InvalidInput(
                    "there is no row below the selection".into(),
                ));
            }
            let cells: Vec<CellInput> = (r.c0..=r.c1)
                .map(|c| {
                    let area = Range::new(r.r0, c, r.r1, c).to_a1(false);
                    CellInput {
                        row: below,
                        col: c,
                        content: Content::Formula(format!("{function}({area})")),
                        style: None,
                    }
                })
                .collect();
            self.exec(
                "grid.editing.auto_sum",
                kiln_sheet::Command::SetCells { sheet: sid, cells },
            )?;
            return Ok(());
        }
        let (row, col) = (r.r0, r.c0);
        // Numbers directly above (skipping one blank row of headers is not Excel's way: the
        // block must touch the cell), else to the left.
        let mut top = row;
        while top > 0 && is_number(top - 1, col) {
            top -= 1;
        }
        let area = if top < row {
            Some(Range::new(top, col, row - 1, col))
        } else {
            let mut left = col;
            while left > 0 && is_number(row, left - 1) {
                left -= 1;
            }
            (left < col).then(|| Range::new(row, left, row, col - 1))
        };
        let refs = area.map_or_else(String::new, |a| a.to_a1(false));
        let text = format!("={function}({refs})");
        self.start_edit(text, EditMode::Point, false)?;
        if let (Some(e), Some(a)) = (&mut self.edit, area) {
            let start = function.len() + 2;
            let end = start + refs.len();
            e.point = Some(PointRef {
                span: (start, end),
                anchor: CellPos::new(a.r0, a.c0),
                head: CellPos::new(a.r1, a.c1),
            });
            e.start = start;
            e.end = end;
        }
        Ok(())
    }
}

/// A sheet name as a reference writes it: quoted when it is not a plain identifier.
pub(crate) fn quote_sheet_name(name: &str) -> String {
    let plain = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && parse_cell_a1(name).is_none();
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// One cell in a direction, skipping hidden rows and columns.
pub(crate) fn step(
    geo: &kiln_sheet_layout::SheetGeometry,
    from: CellPos,
    dir: GridDirection,
) -> (u32, u32) {
    match dir {
        GridDirection::Up => (geo.rows.step(from.row, -1), from.col),
        GridDirection::Down => (geo.rows.step(from.row, 1), from.col),
        GridDirection::Left => (from.row, geo.cols.step(from.col, -1)),
        GridDirection::Right => (from.row, geo.cols.step(from.col, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tokens_calls_and_hints() {
        assert_eq!(token_at("=SU", 3), Some((1, "SU")));
        assert_eq!(token_at("=A1+vlo", 7), Some((4, "vlo")));
        assert_eq!(token_at("=\"SU", 4), None, "inside a string");
        assert_eq!(token_at("=Sheet1!A", 9), None, "after a sheet name");
        assert_eq!(call_at("=SUM(A1,", 8), Some(("SUM".into(), 1)));
        assert_eq!(call_at("=IF(A1>2,SUM(B1", 15), Some(("SUM".into(), 0)));
        assert_eq!(call_at("=IF(A1>2,SUM(B1),", 18), Some(("IF".into(), 2)));
        assert_eq!(call_at("=1+2", 4), None);
        let h = formula_hint("=VLOOKUP(A1,B:C,", 16).expect("hint");
        assert_eq!(h.function, "VLOOKUP");
        assert_eq!(h.current, 2);
        let h = formula_hint("=SUM(1,2,3,4,", 13).expect("hint");
        assert_eq!(h.current as usize, h.args.len() - 1, "repeating arguments");
    }

    #[test]
    fn closes_parentheses_and_spots_references() {
        assert_eq!(close_parens("=SUM(A1:A3"), "=SUM(A1:A3)");
        assert_eq!(close_parens("=IF(A1>0,ROUND(B1,2"), "=IF(A1>0,ROUND(B1,2))");
        assert_eq!(close_parens("=\"(\""), "=\"(\"");
        assert_eq!(close_parens("plain ("), "plain (");
        assert!(expects_reference("=", 1));
        assert!(expects_reference("=SUM(", 5));
        assert!(expects_reference("=A1+ ", 5));
        assert!(!expects_reference("=A1", 3));
        assert!(!expects_reference("text+", 5));
    }

    #[test]
    fn converts_utf16_offsets() {
        let s = "a😀b";
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 1), 1);
        assert_eq!(utf16_to_byte(s, 3), 5);
        assert_eq!(utf16_to_byte(s, 99), s.len());
        assert_eq!(byte_to_utf16(s, 5), 3);
        assert_eq!(quote_sheet_name("Sheet2"), "Sheet2");
        assert_eq!(quote_sheet_name("Q1 data"), "'Q1 data'");
        assert_eq!(quote_sheet_name("A1"), "'A1'");
    }
}
