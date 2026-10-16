// SPDX-License-Identifier: AGPL-3.0-or-later
//! The formula AST. Every node carries a source [`Span`] (byte offsets into the text given to the
//! parser) so the formula bar can highlight it and the Formula Lab can show each step against the
//! source.

use crate::values::ErrorValue;

/// Byte offsets `start..end` into the formula text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// First byte.
    pub start: u32,
    /// One past the last byte.
    pub end: u32,
}

impl Span {
    /// A span from `start` to `end`.
    pub const fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }
}

/// A sheet qualifier: `Sheet1!`, `'My Sheet'!`, `Sheet1:Sheet3!` (3-D) or `[1]Sheet1!` (external).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SheetPrefix {
    /// External workbook (`[1]` or `[Book.xlsx]`).
    pub workbook: Option<String>,
    /// The (first) sheet.
    pub sheet: String,
    /// Last sheet of a 3-D reference.
    pub sheet2: Option<String>,
}

impl SheetPrefix {
    /// A plain single-sheet prefix.
    pub fn sheet(name: impl Into<String>) -> SheetPrefix {
        SheetPrefix {
            workbook: None,
            sheet: name.into(),
            sheet2: None,
        }
    }
}

/// The address part of a reference. Rows and columns are 0-based; `*_abs` flags record `$`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefAddress {
    /// One cell: `B3`.
    Cell {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// `$` before the row.
        row_abs: bool,
        /// `$` before the column.
        col_abs: bool,
    },
    /// A rectangle `A1:B2`, normalised so `r0 <= r1` and `c0 <= c1`.
    Area {
        /// First row.
        r0: u32,
        /// First column.
        c0: u32,
        /// Last row.
        r1: u32,
        /// Last column.
        c1: u32,
        /// `$` on the first row.
        r0_abs: bool,
        /// `$` on the first column.
        c0_abs: bool,
        /// `$` on the last row.
        r1_abs: bool,
        /// `$` on the last column.
        c1_abs: bool,
    },
    /// Whole rows `2:5`.
    Rows {
        /// First row.
        r0: u32,
        /// Last row.
        r1: u32,
        /// `$` on the first row.
        r0_abs: bool,
        /// `$` on the last row.
        r1_abs: bool,
    },
    /// Whole columns `A:C`.
    Cols {
        /// First column.
        c0: u32,
        /// Last column.
        c1: u32,
        /// `$` on the first column.
        c0_abs: bool,
        /// `$` on the last column.
        c1_abs: bool,
    },
}

impl RefAddress {
    /// A relative single-cell address.
    pub const fn cell(row: u32, col: u32) -> RefAddress {
        RefAddress::Cell {
            row,
            col,
            row_abs: false,
            col_abs: false,
        }
    }

    /// An area with corners in any order, normalised so the first corner is the top left; the `$`
    /// flags travel with their coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn area(
        ra: u32,
        ca: u32,
        ra_abs: bool,
        ca_abs: bool,
        rb: u32,
        cb: u32,
        rb_abs: bool,
        cb_abs: bool,
    ) -> RefAddress {
        let (r0, r0_abs, r1, r1_abs) = if ra <= rb {
            (ra, ra_abs, rb, rb_abs)
        } else {
            (rb, rb_abs, ra, ra_abs)
        };
        let (c0, c0_abs, c1, c1_abs) = if ca <= cb {
            (ca, ca_abs, cb, cb_abs)
        } else {
            (cb, cb_abs, ca, ca_abs)
        };
        RefAddress::Area {
            r0,
            c0,
            r1,
            c1,
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
        }
    }
}

/// Special items of a structured reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuredSpecial {
    /// `[#All]`: headers, data and totals.
    All,
    /// `[#Data]`: the data rows (the default).
    Data,
    /// `[#Headers]`: the header row.
    Headers,
    /// `[#Totals]`: the totals row.
    Totals,
}

impl StructuredSpecial {
    /// The item as written (`#Headers`).
    pub const fn as_str(self) -> &'static str {
        match self {
            StructuredSpecial::All => "#All",
            StructuredSpecial::Data => "#Data",
            StructuredSpecial::Headers => "#Headers",
            StructuredSpecial::Totals => "#Totals",
        }
    }
}

/// A structured (table) reference: `Table1[[#Headers],[Qty]]`, `[@Qty]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructuredRef {
    /// The table; `None` for an unqualified reference inside a table (`[@Qty]`).
    pub table: Option<String>,
    /// Special items in the order written.
    pub specials: Vec<StructuredSpecial>,
    /// `@` / `[#This Row]`.
    pub this_row: bool,
    /// A single column (`Some((a, None))`), a column range `[Jan]:[Mar]` (`Some((a, Some(b)))`),
    /// or all columns (`None`).
    pub columns: Option<(String, Option<String>)>,
}

/// Unary prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `+x` (identity)
    Plus,
    /// `@x` implicit intersection
    ImplicitIntersection,
}

impl UnaryOp {
    /// The operator as written.
    pub const fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Plus => "+",
            UnaryOp::ImplicitIntersection => "@",
        }
    }
}

/// Postfix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostfixOp {
    /// `x%`
    Percent,
    /// `A1#` spill-range operator
    Spill,
}

impl PostfixOp {
    /// The operator as written.
    pub const fn as_str(self) -> &'static str {
        match self {
            PostfixOp::Percent => "%",
            PostfixOp::Spill => "#",
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `^`
    Pow,
    /// `&`
    Concat,
    /// `=`
    Eq,
    /// `<>`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `:` range
    Range,
    /// ` ` intersection
    Intersect,
    /// `,` union (inside parentheses)
    Union,
}

impl BinaryOp {
    /// The operator as written.
    pub const fn as_str(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Pow => "^",
            BinaryOp::Concat => "&",
            BinaryOp::Eq => "=",
            BinaryOp::Ne => "<>",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::Range => ":",
            BinaryOp::Intersect => " ",
            BinaryOp::Union => ",",
        }
    }
}

/// What a node is.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A number literal.
    Number(f64),
    /// A text literal.
    Str(String),
    /// TRUE / FALSE.
    Bool(bool),
    /// An error literal; the prefix is kept for `Sheet1!#REF!`.
    Error {
        /// The error.
        code: ErrorValue,
        /// Sheet qualifier, if written.
        prefix: Option<SheetPrefix>,
    },
    /// A cell, area, row or column reference.
    Ref {
        /// Sheet qualifier, if written.
        prefix: Option<SheetPrefix>,
        /// The address.
        address: RefAddress,
    },
    /// A defined name, table name or LET/LAMBDA variable.
    Name {
        /// The name as written.
        name: String,
        /// Sheet qualifier of a sheet-scoped name (`Sheet1!Total`).
        prefix: Option<SheetPrefix>,
    },
    /// A structured reference.
    Structured(Box<StructuredRef>),
    /// An array constant; every row has the same length and elements are literals.
    Array(Vec<Vec<Node>>),
    /// A prefix operator.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Node>,
    },
    /// A postfix operator.
    Postfix {
        /// The operator.
        op: PostfixOp,
        /// The operand.
        operand: Box<Node>,
    },
    /// A binary operator.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Node>,
        /// Right operand.
        right: Box<Node>,
    },
    /// A function call; the name is upper-cased as written, including any `_xlfn.` prefix.
    Call {
        /// Function name.
        name: String,
        /// Arguments ([`NodeKind::Empty`] for omitted ones).
        args: Vec<Node>,
    },
    /// Calling the result of an expression: `LAMBDA(x,x+1)(2)`.
    Invoke {
        /// The expression producing a lambda.
        callee: Box<Node>,
        /// Arguments.
        args: Vec<Node>,
    },
    /// Parentheses.
    Paren(Box<Node>),
    /// An omitted function argument.
    Empty,
}

/// An AST node with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// What the node is.
    pub kind: NodeKind,
    /// Where it is in the source.
    pub span: Span,
}

impl Node {
    /// A node.
    pub fn new(kind: NodeKind, span: Span) -> Node {
        Node { kind, span }
    }

    /// Direct children in source order.
    pub fn children(&self) -> Vec<&Node> {
        match &self.kind {
            NodeKind::Unary { operand, .. } | NodeKind::Postfix { operand, .. } => {
                vec![operand.as_ref()]
            }
            NodeKind::Binary { left, right, .. } => vec![left.as_ref(), right.as_ref()],
            NodeKind::Call { args, .. } => args.iter().collect(),
            NodeKind::Invoke { callee, args } => {
                let mut v = vec![callee.as_ref()];
                v.extend(args.iter());
                v
            }
            NodeKind::Paren(e) => vec![e.as_ref()],
            NodeKind::Array(rows) => rows.iter().flatten().collect(),
            _ => Vec::new(),
        }
    }

    /// Depth-first pre-order walk. Return `false` from the visitor to skip a node's children.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Node) -> bool) {
        let mut stack: Vec<&'a Node> = vec![self];
        while let Some(n) = stack.pop() {
            if !visit(n) {
                continue;
            }
            let kids = n.children();
            for k in kids.into_iter().rev() {
                stack.push(k);
            }
        }
    }

    /// A deep copy with every span set to `0..0` (for structural comparison).
    pub fn without_spans(&self) -> Node {
        let kind = match &self.kind {
            NodeKind::Array(rows) => NodeKind::Array(
                rows.iter()
                    .map(|r| r.iter().map(Node::without_spans).collect())
                    .collect(),
            ),
            NodeKind::Unary { op, operand } => NodeKind::Unary {
                op: *op,
                operand: Box::new(operand.without_spans()),
            },
            NodeKind::Postfix { op, operand } => NodeKind::Postfix {
                op: *op,
                operand: Box::new(operand.without_spans()),
            },
            NodeKind::Binary { op, left, right } => NodeKind::Binary {
                op: *op,
                left: Box::new(left.without_spans()),
                right: Box::new(right.without_spans()),
            },
            NodeKind::Call { name, args } => NodeKind::Call {
                name: name.clone(),
                args: args.iter().map(Node::without_spans).collect(),
            },
            NodeKind::Invoke { callee, args } => NodeKind::Invoke {
                callee: Box::new(callee.without_spans()),
                args: args.iter().map(Node::without_spans).collect(),
            },
            NodeKind::Paren(e) => NodeKind::Paren(Box::new(e.without_spans())),
            other => other.clone(),
        };
        Node {
            kind,
            span: Span::default(),
        }
    }
}
