// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Formula Lab's step tree: a formula evaluated once with tracing on, then every
//! sub-expression of its AST paired with what it evaluated to (ported from the v1.0 TypeScript
//! `engine/explain.ts`).
//!
//! The interpreter runs on the compiled [`Expr`], which drops parentheses and spans; the tree is
//! rebuilt by walking the AST and the compiled expression side by side (a parenthesised node
//! shares the trace of its contents), so every step keeps its source span.

use super::eval::Evaluator;
use super::expr::{Expr, FuncRef};
use super::types::{Area, EvalValue, Reference};
use crate::parser::{BinaryOp, Node, NodeKind, RefAddress, SheetPrefix, Span, format_structured};
use crate::refs::{MAX_COLS, MAX_ROWS, format_address, format_prefix};
use crate::registry::DocsHandle;
use crate::values::{Array2D, Value, to_text};

/// Rows and columns shown by [`format_value`] before an array preview is cut off with `…`.
pub const MAX_ARRAY_PREVIEW: usize = 6;

/// What kind of AST node a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExplainKind {
    /// A number literal.
    Number,
    /// A text literal.
    Str,
    /// `TRUE` / `FALSE`.
    Bool,
    /// An error literal.
    Error,
    /// A cell, area, row or column reference.
    Ref,
    /// A defined name, table name or LET/LAMBDA variable.
    Name,
    /// A structured reference.
    Structured,
    /// An array constant.
    Array,
    /// A prefix operator.
    Unary,
    /// A postfix operator.
    Postfix,
    /// A binary operator.
    Binary,
    /// A function call.
    Call,
    /// Calling the result of an expression (`LAMBDA(x,x+1)(2)`).
    Invoke,
    /// Parentheses.
    Paren,
    /// An omitted argument.
    Empty,
}

impl ExplainKind {
    fn of(kind: &NodeKind) -> ExplainKind {
        match kind {
            NodeKind::Number(_) => ExplainKind::Number,
            NodeKind::Str(_) => ExplainKind::Str,
            NodeKind::Bool(_) => ExplainKind::Bool,
            NodeKind::Error { .. } => ExplainKind::Error,
            NodeKind::Ref { .. } => ExplainKind::Ref,
            NodeKind::Name { .. } => ExplainKind::Name,
            NodeKind::Structured(_) => ExplainKind::Structured,
            NodeKind::Array(_) => ExplainKind::Array,
            NodeKind::Unary { .. } => ExplainKind::Unary,
            NodeKind::Postfix { .. } => ExplainKind::Postfix,
            NodeKind::Binary { .. } => ExplainKind::Binary,
            NodeKind::Call { .. } => ExplainKind::Call,
            NodeKind::Invoke { .. } => ExplainKind::Invoke,
            NodeKind::Paren(_) => ExplainKind::Paren,
            NodeKind::Empty => ExplainKind::Empty,
        }
    }

    /// The lower-case name used by the v1.0 protocol (`call`, `binary`, `ref`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            ExplainKind::Number => "number",
            ExplainKind::Str => "string",
            ExplainKind::Bool => "boolean",
            ExplainKind::Error => "error",
            ExplainKind::Ref => "ref",
            ExplainKind::Name => "name",
            ExplainKind::Structured => "structured",
            ExplainKind::Array => "array",
            ExplainKind::Unary => "unary",
            ExplainKind::Postfix => "postfix",
            ExplainKind::Binary => "binary",
            ExplainKind::Call => "call",
            ExplainKind::Invoke => "invoke",
            ExplainKind::Paren => "paren",
            ExplainKind::Empty => "empty",
        }
    }
}

/// One sub-expression of an explained formula.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainStep {
    /// Node kind.
    pub kind: ExplainKind,
    /// Short label: the function name, operator, reference or literal.
    pub label: String,
    /// Source text of this sub-expression.
    pub text: String,
    /// Byte offsets of [`text`](Self::text) in the formula.
    pub span: Span,
    /// False for sub-expressions that were never evaluated (the untaken branch of `IF`…).
    pub evaluated: bool,
    /// How many times it was evaluated (more than once inside `LAMBDA`/`MAP`…).
    pub count: u32,
    /// The value, with references dereferenced (`None` when not evaluated, or for a
    /// multi-area reference).
    pub value: Option<Value>,
    /// Address text when the step evaluated to a reference (`A1:A3`, `'Data 2'!A1`).
    pub reference: Option<String>,
    /// Display text, e.g. `42`, `"abc"`, `{1,2;3,4}`, `#N/A`, `A1:A3 → {1;2;3}`.
    pub display: String,
    /// One-sentence function summary from the registry, for `Call` steps with inline docs.
    pub description: Option<&'static str>,
    /// Key into the function library's documentation table, for `Call` steps whose docs live
    /// there.
    pub docs_key: Option<&'static str>,
    /// Direct sub-expressions in source order.
    pub children: Vec<ExplainStep>,
}

/// Display text of one scalar: text is quoted, a blank shows as `(blank)`.
pub fn format_scalar(v: &Value) -> String {
    match v {
        Value::Error(e) => e.to_string(),
        Value::Blank => "(blank)".to_owned(),
        Value::Str(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        Value::Array(a) => format_array(a),
        Value::Lambda(l) => format!("LAMBDA({})", l.params().join(",")),
        other => to_text(other).map_or_else(|e| e.to_string(), |t| t.to_string()),
    }
}

fn format_array(a: &Array2D) -> String {
    let mut rows: Vec<String> = (0..a.rows().min(MAX_ARRAY_PREVIEW))
        .map(|r| {
            let mut cells: Vec<String> = (0..a.cols().min(MAX_ARRAY_PREVIEW))
                .map(|c| format_scalar(a.get(r, c)))
                .collect();
            if a.cols() > MAX_ARRAY_PREVIEW {
                cells.push("…".to_owned());
            }
            cells.join(",")
        })
        .collect();
    if a.rows() > MAX_ARRAY_PREVIEW {
        rows.push("…".to_owned());
    }
    format!("{{{}}}", rows.join(";"))
}

/// Compact display of any value: arrays are previewed as `{1,2,…;3,4,…}`.
pub fn format_value(v: Option<&Value>) -> String {
    v.map(format_scalar).unwrap_or_default()
}

fn label(n: &Node, source: &str) -> String {
    match &n.kind {
        NodeKind::Call { name, .. } => name.clone(),
        NodeKind::Binary {
            op: BinaryOp::Intersect,
            ..
        } => "∩".to_owned(),
        NodeKind::Binary { op, .. } => op.as_str().to_owned(),
        NodeKind::Unary { op, .. } => op.as_str().to_owned(),
        NodeKind::Postfix { op, .. } => op.as_str().to_owned(),
        NodeKind::Paren(_) => "( )".to_owned(),
        NodeKind::Empty => "(omitted)".to_owned(),
        NodeKind::Invoke { .. } => "call".to_owned(),
        NodeKind::Structured(s) => format_structured(s),
        _ => slice(source, n.span).to_owned(),
    }
}

fn slice(source: &str, span: Span) -> &str {
    source
        .get(span.start as usize..span.end as usize)
        .unwrap_or("")
}

/// The compiled counterparts of a node's children, in the order of [`Node::children`]; `None`
/// where the compiled tree has no counterpart (array elements are folded into one constant).
fn compiled_children<'e>(n: &Node, e: Option<&'e Expr>) -> Vec<Option<&'e Expr>> {
    let count = n.children().len();
    let found: Vec<&'e Expr> = match (&n.kind, e) {
        (NodeKind::Paren(_), Some(e)) => vec![e],
        (NodeKind::Unary { .. }, Some(Expr::Unary(_, x)))
        | (NodeKind::Postfix { .. }, Some(Expr::Postfix(_, x))) => vec![&**x],
        (NodeKind::Binary { .. }, Some(Expr::Binary(_, l, r))) => vec![&**l, &**r],
        (NodeKind::Call { .. }, Some(Expr::Call(_, args))) => args.iter().map(|a| &**a).collect(),
        (NodeKind::Invoke { .. }, Some(Expr::Invoke(callee, args))) => std::iter::once(&**callee)
            .chain(args.iter().map(|a| &**a))
            .collect(),
        _ => Vec::new(),
    };
    if found.len() == count {
        found.into_iter().map(Some).collect()
    } else {
        vec![None; count]
    }
}

/// Builds the step tree of `ast` (compiled as `code`) after `ev` evaluated `code` with tracing
/// on and produced `result`.
pub(crate) fn build_explain_tree(
    source: &str,
    ast: &Node,
    code: &Expr,
    result: &Value,
    ev: &mut Evaluator<'_>,
) -> ExplainStep {
    Builder { source, ev }.step(ast, Some(code), Root::Result(result))
}

enum Root<'a> {
    /// The formula itself: its value is the finalised result.
    Result(&'a Value),
    /// A sub-expression.
    Child,
    /// An element of an evaluated array constant (a literal, evaluated with its array).
    ArrayElement { count: u32 },
}

struct Builder<'a, 'b, 's> {
    source: &'a str,
    ev: &'b mut Evaluator<'s>,
}

impl Builder<'_, '_, '_> {
    fn step(&mut self, n: &Node, e: Option<&Expr>, role: Root<'_>) -> ExplainStep {
        let traced = e
            .and_then(|e| self.ev.traced(e))
            .map(|t| (t.value.clone(), t.count));
        let (mut evaluated, mut count, mut value, mut reference) = (false, 0, None, None);
        match (&role, traced) {
            (Root::Result(result), t) => {
                evaluated = true;
                count = t.map_or(1, |(_, c)| c.max(1));
                value = Some((*result).clone());
            }
            (Root::ArrayElement { count: c }, _) => {
                evaluated = true;
                count = *c;
                value = Some(literal_value(n));
            }
            (Root::Child, Some((raw, c))) => {
                evaluated = true;
                count = c;
                match raw {
                    EvalValue::Ref(r) => {
                        reference = Some(self.describe_ref(&r));
                        if r.single().is_some() {
                            value = Some(crate::engine::CallContext::to_value(
                                self.ev,
                                EvalValue::Ref(r),
                            ));
                        }
                    }
                    EvalValue::Missing => value = Some(Value::Blank),
                    EvalValue::Value(v) => value = Some(v),
                }
            }
            (Root::Child, None) => {}
        }
        let display = match &reference {
            Some(r) => format!("{r} → {}", format_value(value.as_ref())),
            None => format_value(value.as_ref()),
        };
        let kids = compiled_children(n, e);
        let element_count = matches!(n.kind, NodeKind::Array(_)).then_some(count);
        let children = n
            .children()
            .into_iter()
            .zip(kids)
            .map(|(child, ce)| match element_count {
                Some(c) if evaluated => self.step(child, None, Root::ArrayElement { count: c }),
                _ => self.step(child, ce, Root::Child),
            })
            .collect();
        let (description, docs_key) = match e {
            Some(Expr::Call(FuncRef::Known(id), _)) if matches!(n.kind, NodeKind::Call { .. }) => {
                match self.ev.registry().spec(*id).map(|s| s.docs) {
                    Some(DocsHandle::Inline(d)) => (Some(d.summary), None),
                    Some(DocsHandle::Key(k)) => (None, Some(k)),
                    _ => (None, None),
                }
            }
            _ => (None, None),
        };
        ExplainStep {
            kind: ExplainKind::of(&n.kind),
            label: label(n, self.source),
            text: slice(self.source, n.span).to_owned(),
            span: n.span,
            evaluated,
            count,
            value,
            reference,
            display,
            description,
            docs_key,
            children,
        }
    }

    fn describe_ref(&self, r: &Reference) -> String {
        let here = self.ev.formula_sheet();
        r.as_areas()
            .iter()
            .map(|a| {
                let addr = area_text(a);
                match crate::engine::CallContext::sheet_name(&*self.ev, a.sheet) {
                    Some(name) if a.sheet != here => {
                        format_prefix(&SheetPrefix::sheet(name)) + &addr
                    }
                    _ => addr,
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// `A1`, `A1:B2`, `A:A` or `1:1` for an evaluated area.
fn area_text(a: &Area) -> String {
    let b = a.bounds;
    let whole_cols = b.r0 == 0 && b.r1 == MAX_ROWS - 1;
    let whole_rows = b.c0 == 0 && b.c1 == MAX_COLS - 1;
    let addr = if b.r0 == b.r1 && b.c0 == b.c1 {
        RefAddress::Cell {
            row: b.r0,
            col: b.c0,
            row_abs: false,
            col_abs: false,
        }
    } else if whole_cols && !whole_rows {
        RefAddress::Cols {
            c0: b.c0,
            c1: b.c1,
            c0_abs: false,
            c1_abs: false,
        }
    } else if whole_rows && !whole_cols {
        RefAddress::Rows {
            r0: b.r0,
            r1: b.r1,
            r0_abs: false,
            r1_abs: false,
        }
    } else {
        RefAddress::Area {
            r0: b.r0,
            c0: b.c0,
            r1: b.r1,
            c1: b.c1,
            r0_abs: false,
            c0_abs: false,
            r1_abs: false,
            c1_abs: false,
        }
    };
    format_address(&addr)
}

/// The value of a literal array element.
fn literal_value(n: &Node) -> Value {
    match &n.kind {
        NodeKind::Number(x) => Value::number(*x),
        NodeKind::Str(s) => Value::from(s.as_str()),
        NodeKind::Bool(b) => Value::Bool(*b),
        NodeKind::Error { code, .. } => Value::Error(*code),
        _ => Value::Blank,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_display_like_the_formula_bar() {
        assert_eq!(format_scalar(&Value::Blank), "(blank)");
        assert_eq!(format_scalar(&Value::from("a\"b")), "\"a\"\"b\"");
        assert_eq!(format_scalar(&Value::Number(0.5)), "0.5");
        assert_eq!(format_scalar(&Value::Bool(true)), "TRUE");
        assert_eq!(format_value(None), "");
        let tall = Array2D::build(8, 1, |r, _| Value::number(r as f64));
        assert_eq!(format_scalar(&Value::array(tall)), "{0;1;2;3;4;5;…}");
    }

    #[test]
    fn areas_use_whole_row_and_column_forms() {
        let area = |r0, c0, r1, c1| Area {
            sheet: crate::engine::SheetId(0),
            bounds: crate::refs::Bounds { r0, c0, r1, c1 },
        };
        assert_eq!(area_text(&area(0, 0, 0, 0)), "A1");
        assert_eq!(area_text(&area(0, 0, 2, 1)), "A1:B3");
        assert_eq!(area_text(&area(0, 2, MAX_ROWS - 1, 3)), "C:D");
        assert_eq!(area_text(&area(4, 0, 4, MAX_COLS - 1)), "5:5");
    }
}
