// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formula text → tokens → AST (with spans) → canonical text.
//!
//! The tokenizer covers A1 and R1C1 references (`A1`, `$A$1`, `Sheet1!A1`, `'My Sheet'!A1:B2`,
//! 3-D `Sheet1:Sheet3!A1`, `A:A`, `1:1`, external `[1]Sheet1!A1`), structured references
//! (`Table1[[#Headers],[Col]]`, `[@Qty]`), error literals, `@`, `%`, `&`, comparisons, the range,
//! union and intersection operators, the spill operator `#` and array constants. The Pratt
//! parser builds a [`Node`] tree with byte spans; [`print`] reproduces canonical text.

mod ast;
#[allow(clippy::module_inception)]
mod parser;
mod printer;
mod tokenizer;

pub use ast::{
    BinaryOp, Node, NodeKind, PostfixOp, RefAddress, SheetPrefix, Span, StructuredRef,
    StructuredSpecial, UnaryOp,
};
pub(crate) use parser::leading_equals_len;
pub use parser::{MAX_AST_DEPTH, MAX_FORMULA_CHARS, MAX_NESTING, ParseError, parse};
pub use printer::{
    PrintOptions, format_address_r1c1, format_number_literal, format_structured, print,
};
pub use tokenizer::{
    OperatorToken, ParseOptions, R1C1Match, RefStyle, Token, TokenKind, is_name_char,
    is_name_start, match_a1_address, match_r1c1_address, tokenize,
};
