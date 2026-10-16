// SPDX-License-Identifier: AGPL-3.0-or-later
//! Prints an AST back to formula text: canonical spacing (none, except the intersection
//! operator), upper-case function names, normalised areas. `parse(print(ast))` reproduces the same
//! tree.

use std::fmt::Write as _;

use super::ast::{Node, NodeKind, RefAddress, StructuredRef};
use super::tokenizer::RefStyle;
use crate::refs::{format_address, format_prefix};
use crate::values::shortest_number_text;

/// Printing options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrintOptions {
    /// Reference style; R1C1 prints relative parts against `anchor`.
    pub style: RefStyle,
    /// The cell the formula belongs to, `(row, col)`.
    pub anchor: (u32, u32),
    /// Prefix the output with `=`.
    pub leading_equals: bool,
}

impl PrintOptions {
    /// A1 notation with a leading `=`.
    pub fn formula() -> PrintOptions {
        PrintOptions {
            leading_equals: true,
            ..PrintOptions::default()
        }
    }
}

/// Formats a number literal so that it re-parses to the same double (`1E+21`, `0.1`).
pub fn format_number_literal(n: f64) -> String {
    shortest_number_text(n)
}

fn r1c1_part(out: &mut String, letter: char, index: u32, abs: bool, base: u32) {
    out.push(letter);
    if abs {
        let _ = write!(out, "{}", u64::from(index) + 1);
    } else {
        let d = i64::from(index) - i64::from(base);
        if d != 0 {
            let _ = write!(out, "[{d}]");
        }
    }
}

/// Formats a reference address in R1C1 notation relative to `anchor` `(row, col)`.
pub fn format_address_r1c1(a: &RefAddress, anchor: (u32, u32)) -> String {
    let (ar, ac) = anchor;
    let mut out = String::new();
    match *a {
        RefAddress::Cell {
            row,
            col,
            row_abs,
            col_abs,
        } => {
            r1c1_part(&mut out, 'R', row, row_abs, ar);
            r1c1_part(&mut out, 'C', col, col_abs, ac);
        }
        RefAddress::Area {
            r0,
            c0,
            r1,
            c1,
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
        } => {
            r1c1_part(&mut out, 'R', r0, r0_abs, ar);
            r1c1_part(&mut out, 'C', c0, c0_abs, ac);
            out.push(':');
            r1c1_part(&mut out, 'R', r1, r1_abs, ar);
            r1c1_part(&mut out, 'C', c1, c1_abs, ac);
        }
        RefAddress::Rows {
            r0,
            r1,
            r0_abs,
            r1_abs,
        } => {
            let mut p0 = String::new();
            let mut p1 = String::new();
            r1c1_part(&mut p0, 'R', r0, r0_abs, ar);
            r1c1_part(&mut p1, 'R', r1, r1_abs, ar);
            out = if p0 == p1 { p0 } else { format!("{p0}:{p1}") };
        }
        RefAddress::Cols {
            c0,
            c1,
            c0_abs,
            c1_abs,
        } => {
            let mut p0 = String::new();
            let mut p1 = String::new();
            r1c1_part(&mut p0, 'C', c0, c0_abs, ac);
            r1c1_part(&mut p1, 'C', c1, c1_abs, ac);
            out = if p0 == p1 { p0 } else { format!("{p0}:{p1}") };
        }
    }
    out
}

const COLUMN_SPECIAL: &[char] = &[
    '\t', '\n', '\r', ',', ':', '.', '[', ']', '#', '\'', '"', '{', '}', '$', '^', '&', '*', '+',
    '=', '-', '<', '>', '/', '@',
];

/// True when a column name can be written without inner brackets (`T[Amount]`).
fn simple_column(name: &str) -> bool {
    let (Some(first), Some(last)) = (name.chars().next(), name.chars().last()) else {
        return false;
    };
    if first == ' ' || last == ' ' {
        return false;
    }
    !name.chars().any(|c| COLUMN_SPECIAL.contains(&c))
}

fn escape_column(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '\'' | '[' | ']' | '#') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// Prints a structured reference in Excel's canonical form.
pub fn format_structured(n: &StructuredRef) -> String {
    let table = n.table.as_deref().unwrap_or("");
    let col_items = match &n.columns {
        Some((a, Some(b))) => format!("[{}]:[{}]", escape_column(a), escape_column(b)),
        Some((a, None)) => format!("[{}]", escape_column(a)),
        None => String::new(),
    };
    let single_simple = match &n.columns {
        Some((a, None)) if simple_column(a) => Some(a.as_str()),
        _ => None,
    };
    if n.this_row {
        return match (&n.columns, single_simple) {
            (None, _) => format!("{table}[@]"),
            (_, Some(a)) => format!("{table}[@{a}]"),
            _ => format!("{table}[@{col_items}]"),
        };
    }
    let mut items: Vec<String> = n
        .specials
        .iter()
        .map(|s| format!("[{}]", s.as_str()))
        .collect();
    if items.is_empty() {
        return match (&n.columns, single_simple) {
            (None, _) => format!("{table}[]"),
            (_, Some(a)) => format!("{table}[{a}]"),
            _ => format!("{table}[{col_items}]"),
        };
    }
    if n.columns.is_none() && items.len() == 1 {
        return format!("{table}[{}]", n.specials[0].as_str());
    }
    if n.columns.is_some() {
        items.push(col_items);
    }
    format!("{table}[{}]", items.join(","))
}

fn print_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
}

/// Prints a node.
pub fn print(node: &Node, opts: &PrintOptions) -> String {
    let mut out = String::new();
    if opts.leading_equals {
        out.push('=');
    }
    print_node(&mut out, node, opts);
    out
}

fn print_args(out: &mut String, args: &[Node], opts: &PrintOptions) {
    out.push('(');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        print_node(out, a, opts);
    }
    out.push(')');
}

fn print_node(out: &mut String, node: &Node, opts: &PrintOptions) {
    match &node.kind {
        NodeKind::Number(n) => out.push_str(&format_number_literal(*n)),
        NodeKind::Str(s) => print_string(out, s),
        NodeKind::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
        NodeKind::Error { code, prefix } => {
            if let Some(p) = prefix {
                out.push_str(&format_prefix(p));
            }
            out.push_str(code.code());
        }
        NodeKind::Ref { prefix, address } => {
            if let Some(p) = prefix {
                out.push_str(&format_prefix(p));
            }
            match opts.style {
                RefStyle::A1 => out.push_str(&format_address(address)),
                RefStyle::R1C1 => out.push_str(&format_address_r1c1(address, opts.anchor)),
            }
        }
        NodeKind::Name { name, prefix } => {
            if let Some(p) = prefix {
                out.push_str(&format_prefix(p));
            }
            out.push_str(name);
        }
        NodeKind::Structured(s) => out.push_str(&format_structured(s)),
        NodeKind::Array(rows) => {
            out.push('{');
            for (r, row) in rows.iter().enumerate() {
                if r > 0 {
                    out.push(';');
                }
                for (c, e) in row.iter().enumerate() {
                    if c > 0 {
                        out.push(',');
                    }
                    print_node(out, e, opts);
                }
            }
            out.push('}');
        }
        NodeKind::Unary { op, operand } => {
            out.push_str(op.as_str());
            print_node(out, operand, opts);
        }
        NodeKind::Postfix { op, operand } => {
            print_node(out, operand, opts);
            out.push_str(op.as_str());
        }
        NodeKind::Binary { op, left, right } => {
            print_node(out, left, opts);
            out.push_str(op.as_str());
            print_node(out, right, opts);
        }
        NodeKind::Call { name, args } => {
            out.push_str(name);
            print_args(out, args, opts);
        }
        NodeKind::Invoke { callee, args } => {
            print_node(out, callee, opts);
            print_args(out, args, opts);
        }
        NodeKind::Paren(e) => {
            out.push('(');
            print_node(out, e, opts);
            out.push(')');
        }
        NodeKind::Empty => {}
    }
}
