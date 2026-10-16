// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pratt parser for Excel formulas.
//!
//! Precedence (loosest → tightest): comparison (`= <> < > <= >=`), `&`, `+ -`, `* /`, `^`
//! (left-associative, as in Excel), postfix `%`, prefix `- +` (so `-2^2` is 4 and `-5%` is -0.05),
//! union `,` (only inside parentheses), intersection (whitespace), range `:`, spill `#`. Implicit
//! intersection `@` applies to a whole range expression (`@A1:A10`).

use std::fmt;

use super::ast::{BinaryOp, Node, NodeKind, PostfixOp, Span, UnaryOp};
use super::tokenizer::{OperatorToken, ParseOptions, Token, TokenKind, tokenize};
use crate::values::ErrorValue;

/// Excel's limit on the length of a formula, in characters.
pub const MAX_FORMULA_CHARS: usize = 8192;
/// Deepest AST accepted (parentheses, function nesting and operator chains all count); this
/// bound keeps the recursion of printing, compiling and evaluating small.
pub const MAX_AST_DEPTH: usize = 1024;
/// Deepest nesting of parentheses, function calls, array constants and prefix operators. Excel
/// allows 64 levels of function nesting.
pub const MAX_NESTING: usize = 256;

/// A syntax error with the offending span (byte offsets into the formula text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// What is wrong, in words for the formula bar.
    pub message: String,
    /// Where.
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(message: impl Into<String>, start: u32, end: u32) -> Result<T, ParseError> {
    Err(ParseError {
        message: message.into(),
        span: Span::new(start, end),
    })
}

const PERCENT_BP: u8 = 60;
const PREFIX_BP: u8 = 70;
const UNION_BP: u8 = 80;
const INTERSECT_BP: u8 = 90;
const IMPLICIT_BP: u8 = 95;
const SPILL_BP: u8 = 110;
const CALL_BP: u8 = 120;

fn binary_bp(op: OperatorToken) -> Option<(u8, BinaryOp)> {
    Some(match op {
        OperatorToken::Eq => (10, BinaryOp::Eq),
        OperatorToken::Ne => (10, BinaryOp::Ne),
        OperatorToken::Lt => (10, BinaryOp::Lt),
        OperatorToken::Gt => (10, BinaryOp::Gt),
        OperatorToken::Le => (10, BinaryOp::Le),
        OperatorToken::Ge => (10, BinaryOp::Ge),
        OperatorToken::Amp => (20, BinaryOp::Concat),
        OperatorToken::Plus => (30, BinaryOp::Add),
        OperatorToken::Minus => (30, BinaryOp::Sub),
        OperatorToken::Star => (40, BinaryOp::Mul),
        OperatorToken::Slash => (40, BinaryOp::Div),
        OperatorToken::Caret => (50, BinaryOp::Pow),
        OperatorToken::Colon => (100, BinaryOp::Range),
        _ => return None,
    })
}

struct Parser<'a> {
    text: &'a str,
    toks: Vec<Token>,
    pos: usize,
    end_pos: u32,
    depth: usize,
}

/// What the next significant token is, without its payload (keeps recursive frames small).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Op(OperatorToken),
    Comma,
    LParen,
    RParen,
    /// Something that may start an operand after whitespace (intersection).
    Operand,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct Peek {
    idx: usize,
    start: u32,
    end: u32,
    class: Class,
}

impl Parser<'_> {
    fn next_index(&self) -> usize {
        let mut i = self.pos;
        while i < self.toks.len() && self.toks[i].kind == TokenKind::Whitespace {
            i += 1;
        }
        i
    }

    fn peek(&self) -> Option<Peek> {
        let idx = self.next_index();
        let t = self.toks.get(idx)?;
        let class = match &t.kind {
            TokenKind::Operator(op) => Class::Op(*op),
            TokenKind::Comma => Class::Comma,
            TokenKind::LParen => Class::LParen,
            TokenKind::RParen => Class::RParen,
            TokenKind::Ref { .. }
            | TokenKind::Name { .. }
            | TokenKind::Function(_)
            | TokenKind::Structured(_) => Class::Operand,
            _ => Class::Other,
        };
        Some(Peek {
            idx,
            start: t.start,
            end: t.end,
            class,
        })
    }

    fn ws_before(&self) -> bool {
        self.toks
            .get(self.pos)
            .is_some_and(|t| t.kind == TokenKind::Whitespace)
    }

    /// Consumes the next significant token, moving its payload out.
    fn take(&mut self) -> Result<(TokenKind, u32, u32), ParseError> {
        let i = self.next_index();
        match self.toks.get_mut(i) {
            Some(t) => {
                self.pos = i + 1;
                let kind = std::mem::replace(&mut t.kind, TokenKind::Whitespace);
                Ok((kind, t.start, t.end))
            }
            None => err("Unexpected end of formula", self.end_pos, self.end_pos),
        }
    }

    #[inline(never)]
    fn unexpected_kind(&self, kind: &TokenKind, start: u32, end: u32) -> ParseError {
        let (message, start, end) = match kind {
            TokenKind::Invalid(msg) => (msg.clone(), start, end),
            TokenKind::RParen => ("Unmatched closing parenthesis".to_owned(), start, end),
            _ => (
                format!(
                    "Unexpected \"{}\"",
                    self.text
                        .get(start as usize..end as usize)
                        .unwrap_or_default()
                ),
                start,
                end,
            ),
        };
        ParseError {
            message,
            span: Span::new(start, end),
        }
    }

    #[inline(never)]
    fn unexpected_at(&self, p: Peek) -> ParseError {
        match self.toks.get(p.idx) {
            Some(t) => self.unexpected_kind(&t.kind, t.start, t.end),
            None => ParseError {
                message: "Unexpected end of formula".to_owned(),
                span: Span::new(self.end_pos, self.end_pos),
            },
        }
    }

    fn expect_rparen(&mut self) -> Result<u32, ParseError> {
        match self.peek() {
            Some(p) if p.class == Class::RParen => {
                self.pos = p.idx + 1;
                Ok(p.end)
            }
            Some(p) => err("Expected \")\"", p.start, p.end),
            None => err("Expected \")\"", self.end_pos, self.end_pos),
        }
    }

    fn parse_formula(&mut self) -> Result<Node, ParseError> {
        if self.peek().is_none() {
            return err("The formula is empty", self.end_pos, self.end_pos);
        }
        let node = self.expression(0, false)?;
        if let Some(p) = self.peek() {
            return Err(self.unexpected_at(p));
        }
        Ok(node)
    }

    fn expression(&mut self, min_bp: u8, union: bool) -> Result<Node, ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            let at = self.peek().map_or(self.end_pos, |p| p.start);
            return err("The formula is nested too deeply", at, at);
        }
        let mut left = self.prefix(union);
        if left.is_ok() {
            loop {
                match self.infix(left, min_bp, union) {
                    Step::More(n) => left = Ok(n),
                    Step::Done(n) => {
                        left = n;
                        break;
                    }
                }
            }
        }
        self.depth -= 1;
        left
    }

    /// Applies one infix/postfix operator to `left`, or stops.
    fn infix(&mut self, left: Result<Node, ParseError>, min_bp: u8, union: bool) -> Step {
        let Ok(left) = left else {
            return Step::Done(left);
        };
        let ws = self.ws_before();
        let Some(p) = self.peek() else {
            return Step::Done(Ok(left));
        };
        match p.class {
            Class::Op(OperatorToken::Percent) => {
                if PERCENT_BP < min_bp {
                    return Step::Done(Ok(left));
                }
                self.pos = p.idx + 1;
                Step::More(postfix(PostfixOp::Percent, left, p.end))
            }
            Class::Op(OperatorToken::Hash) => {
                if SPILL_BP < min_bp || ws {
                    return Step::Done(Ok(left));
                }
                if is_literal(&left) {
                    return Step::Done(err(
                        "The # operator needs a reference to a cell",
                        left.span.start,
                        p.end,
                    ));
                }
                self.pos = p.idx + 1;
                Step::More(postfix(PostfixOp::Spill, left, p.end))
            }
            Class::Op(op) => match binary_bp(op) {
                Some((bp, bop)) => {
                    if bp < min_bp {
                        return Step::Done(Ok(left));
                    }
                    self.pos = p.idx + 1;
                    self.binary_rhs(bop, left, bp + 1, union)
                }
                None if op == OperatorToken::At && ws && INTERSECT_BP >= min_bp => {
                    self.binary_rhs(BinaryOp::Intersect, left, INTERSECT_BP + 1, false)
                }
                None => Step::Done(Err(self.unexpected_at(p))),
            },
            Class::Comma if union => {
                if UNION_BP < min_bp {
                    return Step::Done(Ok(left));
                }
                self.pos = p.idx + 1;
                self.binary_rhs(BinaryOp::Union, left, UNION_BP + 1, union)
            }
            Class::LParen if !ws => {
                if CALL_BP < min_bp {
                    return Step::Done(Ok(left));
                }
                match self.argument_list() {
                    Ok((args, end)) => {
                        let start = left.span.start;
                        Step::More(Node::new(
                            NodeKind::Invoke {
                                callee: Box::new(left),
                                args,
                            },
                            Span::new(start, end),
                        ))
                    }
                    Err(e) => Step::Done(Err(e)),
                }
            }
            Class::Operand | Class::LParen if ws => {
                if INTERSECT_BP < min_bp {
                    return Step::Done(Ok(left));
                }
                self.binary_rhs(BinaryOp::Intersect, left, INTERSECT_BP + 1, false)
            }
            _ => Step::Done(Ok(left)),
        }
    }

    fn binary_rhs(&mut self, op: BinaryOp, left: Node, min_bp: u8, union: bool) -> Step {
        match self.expression(min_bp, union) {
            Ok(right) if op == BinaryOp::Range && (is_literal(&left) || is_literal(&right)) => {
                // Excel refuses `=1:1048577` or `=A1:5`: both sides of `:` must be references.
                Step::Done(err(
                    "Both sides of : must be cell references",
                    left.span.start,
                    right.span.end,
                ))
            }
            Ok(right) => Step::More(binary(op, left, right)),
            Err(e) => Step::Done(Err(e)),
        }
    }

    fn prefix(&mut self, union: bool) -> Result<Node, ParseError> {
        let (kind, start, end) = self.take()?;
        match kind {
            TokenKind::Function(name) => self.call(name, start),
            TokenKind::LParen => self.paren(start),
            TokenKind::LBrace => self.array_constant(start),
            TokenKind::Operator(op) => self.prefix_operator(op, start, end, union),
            other => self.atom(other, start, end),
        }
    }

    #[inline(never)]
    fn atom(&self, kind: TokenKind, start: u32, end: u32) -> Result<Node, ParseError> {
        let span = Span::new(start, end);
        let kind = match kind {
            TokenKind::Number(v) => return Ok(number_or_num_error(v, span)),
            TokenKind::Str(s) => NodeKind::Str(s),
            TokenKind::Bool(b) => NodeKind::Bool(b),
            TokenKind::Error { code, prefix } => NodeKind::Error { code, prefix },
            TokenKind::Ref { prefix, address } => NodeKind::Ref { prefix, address },
            TokenKind::Name { name, prefix } => NodeKind::Name { name, prefix },
            TokenKind::Structured(s) => NodeKind::Structured(s),
            other => return Err(self.unexpected_kind(&other, start, end)),
        };
        Ok(Node::new(kind, span))
    }

    fn call(&mut self, name: String, start: u32) -> Result<Node, ParseError> {
        let (args, end) = self.argument_list()?;
        Ok(Node::new(
            NodeKind::Call { name, args },
            Span::new(start, end),
        ))
    }

    fn paren(&mut self, start: u32) -> Result<Node, ParseError> {
        let expr = self.expression(0, true)?;
        let end = self.expect_rparen()?;
        Ok(Node::new(
            NodeKind::Paren(Box::new(expr)),
            Span::new(start, end),
        ))
    }

    fn prefix_operator(
        &mut self,
        op: OperatorToken,
        start: u32,
        end: u32,
        union: bool,
    ) -> Result<Node, ParseError> {
        let (uop, bp) = match op {
            OperatorToken::Minus => (UnaryOp::Neg, PREFIX_BP),
            OperatorToken::Plus => (UnaryOp::Plus, PREFIX_BP),
            OperatorToken::At => (UnaryOp::ImplicitIntersection, IMPLICIT_BP),
            other => {
                return Err(self.unexpected_kind(&TokenKind::Operator(other), start, end));
            }
        };
        let operand = self.expression(bp, union)?;
        let span = Span::new(start, operand.span.end);
        Ok(Node::new(
            NodeKind::Unary {
                op: uop,
                operand: Box::new(operand),
            },
            span,
        ))
    }

    fn argument_list(&mut self) -> Result<(Vec<Node>, u32), ParseError> {
        let open = match self.peek() {
            Some(p) if p.class == Class::LParen => {
                self.pos = p.idx + 1;
                p
            }
            Some(p) => return err("Expected \"(\"", p.start, p.end),
            None => return err("Expected \"(\"", self.end_pos, self.end_pos),
        };
        let mut args = Vec::new();
        if let Some(p) = self.peek()
            && p.class == Class::RParen
        {
            self.pos = p.idx + 1;
            return Ok((args, p.end));
        }
        let mut last_sep_end = open.end;
        loop {
            let Some(p) = self.peek() else {
                return err("Missing closing parenthesis", open.start, self.end_pos);
            };
            if matches!(p.class, Class::Comma | Class::RParen) {
                args.push(Node::new(
                    NodeKind::Empty,
                    Span::new(last_sep_end, last_sep_end),
                ));
            } else {
                args.push(self.expression(0, false)?);
            }
            let Some(p) = self.peek() else {
                return err("Missing closing parenthesis", open.start, self.end_pos);
            };
            match p.class {
                Class::Comma => {
                    self.pos = p.idx + 1;
                    last_sep_end = p.end;
                }
                Class::RParen => {
                    self.pos = p.idx + 1;
                    return Ok((args, p.end));
                }
                _ => return Err(self.unexpected_at(p)),
            }
        }
    }

    #[inline(never)]
    fn array_constant(&mut self, open_start: u32) -> Result<Node, ParseError> {
        let mut rows: Vec<Vec<Node>> = vec![Vec::new()];
        loop {
            let (kind, start, end) = self.take()?;
            let el = match kind {
                TokenKind::Operator(op @ (OperatorToken::Minus | OperatorToken::Plus)) => {
                    let (n, _, nend) = self.take()?;
                    let TokenKind::Number(v) = n else {
                        return err("Expected a number in the array constant", start, nend);
                    };
                    let v = if op == OperatorToken::Minus { -v } else { v };
                    number_or_num_error(v, Span::new(start, nend))
                }
                TokenKind::Number(v) => number_or_num_error(v, Span::new(start, end)),
                TokenKind::Str(s) => Node::new(NodeKind::Str(s), Span::new(start, end)),
                TokenKind::Bool(b) => Node::new(NodeKind::Bool(b), Span::new(start, end)),
                TokenKind::Error { code, prefix: None } => Node::new(
                    NodeKind::Error { code, prefix: None },
                    Span::new(start, end),
                ),
                _ => {
                    return err(
                        "Array constants may only contain numbers, text, logical values and errors",
                        start,
                        end,
                    );
                }
            };
            if let Some(last) = rows.last_mut() {
                last.push(el);
            }
            let (sep, sstart, send) = self.take()?;
            match sep {
                TokenKind::Comma => {}
                TokenKind::Semicolon => rows.push(Vec::new()),
                TokenKind::RBrace => {
                    let width = rows[0].len();
                    if rows.iter().any(|r| r.len() != width) {
                        return err(
                            "Every row of an array constant must have the same length",
                            open_start,
                            send,
                        );
                    }
                    return Ok(Node::new(
                        NodeKind::Array(rows),
                        Span::new(open_start, send),
                    ));
                }
                _ => {
                    return err(
                        "Expected \",\", \";\" or \"}\" in the array constant",
                        sstart,
                        send,
                    );
                }
            }
        }
    }
}

/// One step of the operator loop: keep going with a new left operand, or stop.
enum Step {
    More(Node),
    Done(Result<Node, ParseError>),
}

fn number_or_num_error(v: f64, span: Span) -> Node {
    if v.is_finite() {
        Node::new(NodeKind::Number(v), span)
    } else {
        Node::new(
            NodeKind::Error {
                code: ErrorValue::Num,
                prefix: None,
            },
            span,
        )
    }
}

fn postfix(op: PostfixOp, operand: Node, end: u32) -> Node {
    let start = operand.span.start;
    Node::new(
        NodeKind::Postfix {
            op,
            operand: Box::new(operand),
        },
        Span::new(start, end),
    )
}

/// A number, text, logical or array constant: never a reference.
fn is_literal(n: &Node) -> bool {
    matches!(
        n.kind,
        NodeKind::Number(_) | NodeKind::Str(_) | NodeKind::Bool(_) | NodeKind::Array(_)
    )
}

fn binary(op: BinaryOp, left: Node, right: Node) -> Node {
    let span = Span::new(left.span.start, right.span.end);
    Node::new(
        NodeKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    )
}

/// Depth of the deepest path in a tree (computed without recursion).
fn tree_depth(root: &Node) -> usize {
    let mut max = 0;
    let mut stack = vec![(root, 1usize)];
    while let Some((n, d)) = stack.pop() {
        max = max.max(d);
        for k in n.children() {
            stack.push((k, d + 1));
        }
    }
    max
}

/// Length of the leading `\s*=` (0 when the text does not start with `=`).
pub(crate) fn leading_equals_len(text: &str) -> usize {
    let trimmed = text.trim_start();
    if trimmed.starts_with('=') {
        text.len() - trimmed.len() + 1
    } else {
        0
    }
}

/// Parses a formula. A leading `=` is optional. Spans are byte offsets into `text` as given.
pub fn parse(text: &str, opts: &ParseOptions) -> Result<Node, ParseError> {
    if text.chars().count() > MAX_FORMULA_CHARS {
        return err(
            "The formula is longer than 8,192 characters",
            0,
            text.len() as u32,
        );
    }
    let offset = leading_equals_len(text);
    let tokens = tokenize(&text[offset..], opts, offset as u32);
    let mut p = Parser {
        text,
        toks: tokens,
        pos: 0,
        end_pos: text.len() as u32,
        depth: 0,
    };
    let node = p.parse_formula()?;
    if tree_depth(&node) > MAX_AST_DEPTH {
        return err("The formula is nested too deeply", 0, text.len() as u32);
    }
    Ok(node)
}
