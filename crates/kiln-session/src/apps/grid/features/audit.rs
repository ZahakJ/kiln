// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formula auditing: Trace Precedents and Trace Dependents (arrows drawn over the sheet, one
//! level further on each click, dashed to a sheet icon for other sheets), Remove Arrows, Error
//! Checking, Evaluate Formula and Formula Lab (the formula's evaluation tree with each node's
//! value, and Excel's step-by-step reduction), the Watch Window and Calculation Options.

use kiln_common::{KilnError, Result};
use kiln_formula::engine::{ExplainKind, ExplainStep};
use kiln_formula::parser::{ParseOptions, TokenKind, tokenize};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{
    GridArrows, GridCalcMode, GridErrorInfo, GridEvalStep, GridFormulaTrace, GridTraceNode,
    GridWatch,
};
use kiln_scene::{DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::address::cell_a1;
use kiln_sheet::meta::CalcMode;
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};
use kiln_sheet_layout::SheetGeometry;
use kiln_sheet_layout::overlay::formula_refs;

use super::super::GridDoc;
use super::super::editor::{byte_to_utf16, quote_sheet_name};
use super::Arrow;
use crate::model::Change;

/// Arrow colours: precedents and dependents blue, references to errors red.
const ARROW: u32 = 0x31_59_C6_FF;
const ARROW_ERROR: u32 = 0xD9_2D_20_FF;

pub(crate) fn calc_mode(m: CalcMode) -> GridCalcMode {
    match m {
        CalcMode::Auto => GridCalcMode::Automatic,
        CalcMode::AutoNoTable => GridCalcMode::AutomaticExceptTables,
        CalcMode::Manual => GridCalcMode::Manual,
    }
}

/// What an error value means (Error Checking).
pub(crate) fn error_help(e: &str) -> (&'static str, &'static str) {
    match e {
        "#DIV/0!" => (
            "Divide by Zero Error",
            "The formula or function divides by zero or by an empty cell.",
        ),
        "#N/A" => (
            "Value Not Available Error",
            "A value is not available to the formula or function (a lookup found nothing).",
        ),
        "#NAME?" => (
            "Invalid Name Error",
            "The formula contains text that is not a function, defined name or reference.",
        ),
        "#NULL!" => ("Null Error", "The ranges in the formula do not intersect."),
        "#NUM!" => (
            "Number Error",
            "The formula or function has an invalid number, or a result too large or small.",
        ),
        "#REF!" => (
            "Invalid Cell Reference Error",
            "The formula refers to cells that were deleted or replaced.",
        ),
        "#VALUE!" => (
            "Error in Value",
            "A value used in the formula is of the wrong data type.",
        ),
        "#SPILL!" => (
            "Spill Range Isn't Blank",
            "The array result cannot spill because cells in its way are not empty.",
        ),
        "#CALC!" => (
            "Calculation Error",
            "The calculation engine met a case it cannot calculate (an empty array).",
        ),
        _ => ("Error", "The formula returns an error."),
    }
}

fn kind_name(k: ExplainKind) -> &'static str {
    match k {
        ExplainKind::Number | ExplainKind::Str | ExplainKind::Bool | ExplainKind::Error => {
            "literal"
        }
        ExplainKind::Ref => "reference",
        ExplainKind::Name => "name",
        ExplainKind::Structured => "structured",
        ExplainKind::Array => "array",
        ExplainKind::Unary | ExplainKind::Postfix | ExplainKind::Binary => "operator",
        ExplainKind::Call => "call",
        ExplainKind::Invoke => "invoke",
        ExplainKind::Paren => "group",
        _ => "other",
    }
}

/// The value part of a step's display (`A1:A3 → {1;2;3}` shows `{1;2;3}`).
fn value_of(step: &ExplainStep) -> String {
    step.display
        .rsplit(" → ")
        .next()
        .unwrap_or(&step.display)
        .to_owned()
}

fn node(step: &ExplainStep, text: &str) -> GridTraceNode {
    GridTraceNode {
        kind: kind_name(step.kind).into(),
        label: step.label.clone(),
        text: step.text.clone(),
        start: byte_to_utf16(text, step.span.start as usize),
        end: byte_to_utf16(text, step.span.end as usize),
        value: if step.evaluated {
            value_of(step)
        } else {
            String::new()
        },
        evaluated: step.evaluated,
        reference: step.reference.clone(),
        summary: step.description.map(str::to_owned).or_else(|| {
            step.docs_key
                .and_then(kiln_formula::functions::doc)
                .map(|d| d.summary.clone())
        }),
        children: step.children.iter().map(|c| node(c, text)).collect(),
    }
}

/// Evaluate Formula's steps: the evaluated sub-expressions innermost first, left to right (the
/// literals are skipped), each replaced by its value in the text of the next step.
fn eval_steps(root: &ExplainStep, formula: &str) -> Vec<GridEvalStep> {
    fn order<'a>(s: &'a ExplainStep, out: &mut Vec<&'a ExplainStep>) {
        if !s.evaluated {
            return;
        }
        for c in &s.children {
            order(c, out);
        }
        let literal = matches!(
            s.kind,
            ExplainKind::Number
                | ExplainKind::Str
                | ExplainKind::Bool
                | ExplainKind::Error
                | ExplainKind::Paren
                | ExplainKind::Empty
                | ExplainKind::Array
        );
        if !literal {
            out.push(s);
        }
    }
    let mut nodes = Vec::new();
    order(root, &mut nodes);
    // Replacements of original byte spans, kept sorted and non-overlapping.
    let mut reps: Vec<(usize, usize, String)> = Vec::new();
    let current = |reps: &[(usize, usize, String)]| {
        let mut out = String::new();
        let mut at = 0;
        for (s, e, r) in reps {
            out.push_str(formula.get(at..*s).unwrap_or(""));
            out.push_str(r);
            at = *e;
        }
        out.push_str(formula.get(at..).unwrap_or(""));
        out
    };
    let map = |reps: &[(usize, usize, String)], pos: usize| -> usize {
        let mut p = pos as isize;
        for (s, e, r) in reps {
            if *e <= pos {
                p += r.len() as isize - (*e - *s) as isize;
            }
        }
        p.max(0) as usize
    };
    let mut steps = Vec::new();
    for n in nodes.iter().take(400) {
        let (s, e) = (n.span.start as usize, n.span.end as usize);
        if s >= e || e > formula.len() {
            continue;
        }
        let text = current(&reps);
        let (cs, ce) = (map(&reps, s), map(&reps, e));
        steps.push(GridEvalStep {
            start: byte_to_utf16(&text, cs),
            end: byte_to_utf16(&text, ce),
            value: value_of(n),
            text,
        });
        reps.retain(|(a, b, _)| !(*a >= s && *b <= e));
        reps.push((s, e, value_of(n)));
        reps.sort_by_key(|(a, _, _)| *a);
    }
    let last = format!("={}", value_of(root));
    if steps.last().is_none_or(|l| l.text != last) {
        let _ = current(&reps);
        steps.push(GridEvalStep {
            text: last,
            start: 0,
            end: 0,
            value: value_of(root),
        });
    }
    steps
}

impl GridDoc {
    /// The cells in circular references as `Sheet1!A1`, the active sheet's first (at most 20).
    pub(crate) fn circular_refs(&self) -> Vec<String> {
        let active = self.sid().ok();
        let mut cells = self.workbook.circular_cells();
        cells.sort_by_key(|c| (Some(c.sheet) != active, c.sheet, c.row, c.col));
        cells
            .iter()
            .take(20)
            .filter_map(|c| {
                let name = self.workbook.sheet(c.sheet)?.name().to_owned();
                Some(format!(
                    "{}!{}",
                    crate::apps::grid::editor::quote_sheet_name(&name),
                    kiln_sheet::Range::new(c.row, c.col, c.row, c.col).to_a1(false)
                ))
            })
            .collect()
    }

    /// The active cell's evaluation, now.
    pub(crate) fn trace_now(&mut self) -> GridFormulaTrace {
        let Ok(sid) = self.sid() else {
            return GridFormulaTrace {
                cell: String::new(),
                formula: String::new(),
                value: String::new(),
                root: None,
                steps: Vec::new(),
                message: Some("There is no sheet.".into()),
            };
        };
        let a = self.view().sel.active;
        let sheet = self
            .workbook
            .sheet(sid)
            .map(|s| quote_sheet_name(s.name()))
            .unwrap_or_default();
        let cell = format!("{sheet}!{}", cell_a1(a.row, a.col, true));
        let value = self.workbook.display_text(sid, a.row, a.col);
        let Some(f) = self.workbook.formula(sid, a.row, a.col) else {
            return GridFormulaTrace {
                cell,
                formula: String::new(),
                value,
                root: None,
                steps: Vec::new(),
                message: Some(if self.has_content(sid, a.row, a.col) {
                    "The cell holds a value, not a formula.".into()
                } else {
                    "Select a cell with a formula.".into()
                }),
            };
        };
        let formula = format!("={}", f.trim_start_matches('='));
        match self.workbook.explain(sid, a.row, a.col) {
            Ok(step) => GridFormulaTrace {
                cell,
                steps: eval_steps(&step, &formula),
                root: Some(node(&step, &formula)),
                formula,
                value,
                message: None,
            },
            Err(e) => GridFormulaTrace {
                cell,
                formula,
                value,
                root: None,
                steps: Vec::new(),
                message: Some(format!("The formula cannot be evaluated: {e}")),
            },
        }
    }

    /// The ranges a formula on `on` refers to, on sheet `target` (`None`: the formula's own
    /// sheet), with whether each is on another sheet.
    pub(crate) fn refs_of(&self, on: SheetId, text: &str) -> Vec<(SheetId, Range)> {
        let mut out = Vec::new();
        let opts = ParseOptions::default();
        let body = text.trim_start_matches('=');
        for t in tokenize(body, &opts, 0) {
            let s = t.start as usize;
            let e = t.end as usize;
            let Some(tok) = body.get(s..e) else {
                continue;
            };
            match &t.kind {
                TokenKind::Ref { prefix, .. } => {
                    let sheet = match prefix {
                        Some(p) if p.workbook.is_none() && p.sheet2.is_none() => self
                            .workbook
                            .sheet_by_name(&p.sheet)
                            .map(kiln_sheet::Sheet::id),
                        Some(_) => None,
                        None => Some(on),
                    };
                    let Some(sheet) = sheet else { continue };
                    let name = self
                        .workbook
                        .sheet(sheet)
                        .map(|s| s.name().to_owned())
                        .unwrap_or_default();
                    for r in formula_refs(&format!("={tok}"), &name) {
                        if let Some(range) = r.range {
                            out.push((sheet, range));
                        }
                    }
                }
                TokenKind::Name { .. } => {
                    if let Some(n) = self.workbook.resolve_name(tok, on) {
                        let f = n.formula.clone();
                        if !f.contains('(') {
                            for (s2, r) in self.refs_of(on, &f) {
                                out.push((s2, r));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn trace(&mut self, dependents: bool) -> Result<()> {
        let sid = self.sid()?;
        let a = self.view().sel.active;
        let root = (sid, a);
        let frontier_empty = if dependents {
            self.features.dep_frontier.is_empty()
        } else {
            self.features.prec_frontier.is_empty()
        };
        let traced_here = self
            .features
            .arrows
            .iter()
            .any(|x| x.dependent == dependents && (x.to == a || x.from.contains(a.row, a.col)));
        let frontier: Vec<(SheetId, CellPos)> = if frontier_empty || !traced_here {
            vec![root]
        } else if dependents {
            self.features.dep_frontier.clone()
        } else {
            self.features.prec_frontier.clone()
        };
        let mut next = Vec::new();
        let mut added = 0usize;
        if dependents {
            // Every formula that refers to a frontier cell.
            let sheets: Vec<(SheetId, String)> = self
                .workbook
                .sheets()
                .iter()
                .map(|s| (s.id(), s.name().to_owned()))
                .collect();
            for (fs, _) in &sheets {
                let Some(sheet) = self.workbook.sheet(*fs) else {
                    continue;
                };
                let formulas: Vec<(u32, u32)> = sheet
                    .cells()
                    .iter()
                    .filter(|(_, _, d)| d.kind.is_formula())
                    .map(|(r, c, _)| (r, c))
                    .take(500_000)
                    .collect();
                for (r, c) in formulas {
                    let Some(f) = self.workbook.formula(*fs, r, c) else {
                        continue;
                    };
                    let refs = self.refs_of(*fs, &f);
                    for &(ts, tc) in &frontier {
                        if refs
                            .iter()
                            .any(|(s, range)| *s == ts && range.contains(tc.row, tc.col))
                        {
                            let arrow = if *fs == ts {
                                Arrow {
                                    from: Range::cell(r, c),
                                    to: tc,
                                    sheet: ts,
                                    dependent: true,
                                    external: false,
                                }
                            } else {
                                Arrow {
                                    from: Range::cell(tc.row, tc.col),
                                    to: tc,
                                    sheet: ts,
                                    dependent: true,
                                    external: true,
                                }
                            };
                            if !self.features.arrows.contains(&arrow) {
                                self.features.arrows.push(arrow);
                                added += 1;
                            }
                            if *fs == ts {
                                next.push((*fs, CellPos::new(r, c)));
                            }
                        }
                    }
                }
            }
        } else {
            for &(fs, cell) in &frontier {
                let Some(f) = self.workbook.formula(fs, cell.row, cell.col) else {
                    continue;
                };
                for (s, range) in self.refs_of(fs, &f) {
                    let arrow = Arrow {
                        from: if s == fs {
                            range
                        } else {
                            Range::cell(cell.row, cell.col)
                        },
                        to: cell,
                        sheet: fs,
                        dependent: false,
                        external: s != fs,
                    };
                    if !self.features.arrows.contains(&arrow) {
                        self.features.arrows.push(arrow);
                        added += 1;
                    }
                    if s == fs && range.cell_count() <= 10_000 {
                        for p in range.cells() {
                            if self
                                .workbook
                                .sheet(s)
                                .and_then(|sh| sh.cells().get(p.row, p.col))
                                .is_some_and(|d| d.kind.is_formula())
                            {
                                next.push((s, p));
                            }
                        }
                    }
                }
            }
        }
        next.sort_unstable_by_key(|(s, p)| (s.0, p.row, p.col));
        next.dedup();
        next.truncate(500);
        if dependents {
            self.features.dep_frontier = next;
        } else {
            self.features.prec_frontier = next;
        }
        if added == 0 && frontier.len() == 1 && frontier[0] == root {
            let has_formula = self.workbook.formula(sid, a.row, a.col).is_some();
            return Err(KilnError::InvalidInput(if dependents {
                "No formulas refer to the active cell.".into()
            } else if has_formula {
                "The formula has no cell references to trace.".into()
            } else {
                "The active cell holds no formula with references to trace.".into()
            }));
        }
        Ok(())
    }

    pub(crate) fn audit_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        match cmd {
            GridCommand::TracePrecedents => self.trace(false)?,
            GridCommand::TraceDependents => self.trace(true)?,
            GridCommand::RemoveArrows { which } => {
                match which {
                    GridArrows::All => self.features.arrows.clear(),
                    GridArrows::Precedents => self.features.arrows.retain(|a| a.dependent),
                    GridArrows::Dependents => self.features.arrows.retain(|a| !a.dependent),
                }
                if !matches!(which, GridArrows::Dependents) {
                    self.features.prec_frontier.clear();
                }
                if !matches!(which, GridArrows::Precedents) {
                    self.features.dep_frontier.clear();
                }
            }
            GridCommand::CalculationMode { mode } => {
                let mut props = self.workbook.props().clone();
                props.calc.mode = match mode {
                    GridCalcMode::Automatic => CalcMode::Auto,
                    GridCalcMode::AutomaticExceptTables => CalcMode::AutoNoTable,
                    GridCalcMode::Manual => CalcMode::Manual,
                };
                self.exec(
                    "grid.formulas.calculation_mode",
                    Command::SetProps {
                        props: Box::new(props),
                    },
                )?;
                return Ok(Change::Content);
            }
            GridCommand::IterativeCalculation {
                enabled,
                max_iterations,
                max_change,
            } => {
                if !(1..=32_767).contains(max_iterations) {
                    return Err(KilnError::InvalidInput(
                        "Maximum Iterations is a whole number from 1 to 32,767".into(),
                    ));
                }
                if !max_change.is_finite() || *max_change < 0.0 {
                    return Err(KilnError::InvalidInput(
                        "Maximum Change is a number of 0 or more".into(),
                    ));
                }
                let mut props = self.workbook.props().clone();
                props.calc.iterate = *enabled;
                props.calc.iterate_count = *max_iterations;
                props.calc.iterate_delta = *max_change;
                self.exec(
                    "grid.formulas.iterative",
                    Command::SetProps {
                        props: Box::new(props),
                    },
                )?;
                return Ok(Change::Content);
            }
            GridCommand::DateSystem { date1904 } => {
                let mut props = self.workbook.props().clone();
                if props.date1904 == *date1904 {
                    return Ok(Change::None);
                }
                props.date1904 = *date1904;
                self.exec(
                    "grid.formulas.date_1904",
                    Command::SetProps {
                        props: Box::new(props),
                    },
                )?;
                return Ok(Change::Content);
            }
            GridCommand::FormulaLab { open } => {
                self.features.lab = *open;
                self.features.trace = if *open { Some(self.trace_now()) } else { None };
            }
            GridCommand::Watch {
                add,
                sheet,
                row,
                col,
            } => {
                let sid = self
                    .workbook
                    .sheets()
                    .get(*sheet as usize)
                    .map(kiln_sheet::Sheet::id)
                    .ok_or_else(|| KilnError::InvalidInput(format!("there is no sheet {sheet}")))?;
                let w = (sid, *row, *col);
                if *add {
                    if !self.features.watches.contains(&w) {
                        self.features.watches.push(w);
                    }
                } else {
                    self.features.watches.retain(|x| *x != w);
                }
            }
            _ => return Ok(Change::None),
        }
        Ok(Change::Selection)
    }

    /// The Watch Window's rows.
    pub(crate) fn watch_rows(&self) -> Vec<GridWatch> {
        self.features
            .watches
            .iter()
            .filter_map(|&(sid, row, col)| {
                let sheet = self.workbook.sheet(sid)?;
                let index = self.workbook.sheet_index(sid)?;
                let abs = format!(
                    "{}!{}",
                    quote_sheet_name(sheet.name()),
                    cell_a1(row, col, true)
                );
                let name = self
                    .workbook
                    .names()
                    .iter()
                    .find(|n| n.formula.replace('\'', "") == abs.replace('\'', ""))
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                Some(GridWatch {
                    sheet: sheet.name().to_owned(),
                    cell: cell_a1(row, col, false),
                    sheet_index: u32::try_from(index).unwrap_or(0),
                    row,
                    col,
                    name,
                    value: self.workbook.display_text(sid, row, col),
                    formula: self
                        .workbook
                        .formula(sid, row, col)
                        .map(|f| format!("={}", f.trim_start_matches('=')))
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    /// The error cells of the sheet for Error Checking, from the active cell on (wrapping).
    pub(crate) fn error_cells(&self) -> Result<Vec<GridErrorInfo>> {
        let sid = self.sid()?;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let mut cells: Vec<(u32, u32)> = sheet
            .cells()
            .iter()
            .filter(|(_, _, d)| d.kind.is_formula())
            .map(|(r, c, _)| (r, c))
            .filter(|&(r, c)| matches!(self.workbook.value(sid, r, c), CellValue::Error(_)))
            .collect();
        cells.sort_unstable();
        let a = self.view().sel.active;
        let at = cells.partition_point(|&p| p < (a.row, a.col));
        cells.rotate_left(at);
        Ok(cells
            .into_iter()
            .take(10_000)
            .map(|(row, col)| {
                let error = self.workbook.display_text(sid, row, col);
                let (title, help) = error_help(&error);
                GridErrorInfo {
                    row,
                    col,
                    cell: cell_a1(row, col, false),
                    formula: self
                        .workbook
                        .formula(sid, row, col)
                        .map(|f| format!("={}", f.trim_start_matches('=')))
                        .unwrap_or_default(),
                    error,
                    title: title.into(),
                    help: help.into(),
                }
            })
            .collect())
    }

    /// The trace arrows (sheet pixels).
    pub(crate) fn arrow_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Ok(sid) = self.sid() else {
            return;
        };
        for a in self.features.arrows.iter().filter(|a| a.sheet == sid) {
            let to = geo.display_rect(a.to.row, a.to.col);
            let error = matches!(
                self.workbook.value(sid, a.from.r0, a.from.c0),
                CellValue::Error(_)
            );
            let paint = Paint::Rgba(if error { ARROW_ERROR } else { ARROW });
            let (tx, ty) = (to.x + to.w * 0.5, to.y + to.h * 0.5);
            if a.external {
                // A dashed arrow from a small sheet icon up-left of the cell.
                let (ix, iy) = (to.x - 34.0, to.y - 22.0);
                out.push(DrawOp::StrokeRect {
                    rect: kiln_scene::Bounds::new(ix - 7.0, iy - 6.0, 14.0, 12.0),
                    paint,
                    width: 1.2,
                    radius: 1.0,
                });
                for k in 0..3 {
                    let y = iy - 3.0 + k as f32 * 3.0;
                    let mut p = PathData::new();
                    p.move_to(ix - 4.0, y).line_to(ix + 4.0, y);
                    out.push(DrawOp::Path {
                        path: p,
                        fill: None,
                        stroke: Some(Stroke::solid(paint, 0.8)),
                        rule: FillRule::NonZero,
                    });
                }
                push_arrow(out, (ix + 7.0, iy + 6.0), (tx, ty), paint, true);
                continue;
            }
            let from = geo.range_rect(&a.from);
            if !a.from.is_single_cell() {
                out.push(DrawOp::StrokeRect {
                    rect: from.inflate(-1.0, -1.0),
                    paint,
                    width: 1.5,
                    radius: 0.0,
                });
            }
            let (fx, fy) = if a.from.is_single_cell() {
                (from.x + from.w * 0.5, from.y + from.h * 0.5)
            } else {
                (from.x + 6.0, from.y + 6.0)
            };
            // A dot at the precedent's end.
            out.push(DrawOp::FillRect {
                rect: kiln_scene::Bounds::new(fx - 3.0, fy - 3.0, 6.0, 6.0),
                paint,
                radius: 3.0,
            });
            let (fx, fy, tx, ty) = if a.dependent && a.from != Range::cell(a.to.row, a.to.col) {
                (tx, ty, fx, fy)
            } else {
                (fx, fy, tx, ty)
            };
            push_arrow(out, (fx, fy), (tx, ty), paint, false);
        }
    }
}

/// A line with an arrowhead at `to`.
fn push_arrow(out: &mut Vec<DrawOp>, from: (f32, f32), to: (f32, f32), paint: Paint, dashed: bool) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let head = 9.0_f32.min(len * 0.5);
    let (bx, by) = (to.0 - ux * head, to.1 - uy * head);
    let mut p = PathData::new();
    p.move_to(from.0, from.1).line_to(bx, by);
    out.push(DrawOp::Path {
        path: p,
        fill: None,
        stroke: Some(Stroke {
            dash: if dashed { vec![4.0, 3.0] } else { Vec::new() },
            ..Stroke::solid(paint, 1.5)
        }),
        rule: FillRule::NonZero,
    });
    let mut h = PathData::new();
    h.move_to(to.0, to.1)
        .line_to(bx - uy * head * 0.45, by + ux * head * 0.45)
        .line_to(bx + uy * head * 0.45, by - ux * head * 0.45)
        .close();
    out.push(DrawOp::Path {
        path: h,
        fill: Some(paint),
        stroke: None,
        rule: FillRule::NonZero,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_titles() {
        assert_eq!(error_help("#DIV/0!").0, "Divide by Zero Error");
        assert_eq!(error_help("#weird").0, "Error");
    }
}
