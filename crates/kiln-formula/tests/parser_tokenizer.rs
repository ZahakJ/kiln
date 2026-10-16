// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tokenizer tests (ported from the v1.0 TypeScript suite).

use kiln_formula::ErrorValue;
use kiln_formula::parser::{
    OperatorToken, ParseOptions, RefAddress, SheetPrefix, StructuredRef, StructuredSpecial, Token,
    TokenKind, tokenize,
};

fn toks(s: &str) -> Vec<Token> {
    tokenize(s, &ParseOptions::default(), 0)
}

fn first(s: &str) -> TokenKind {
    toks(s)
        .into_iter()
        .next()
        .map_or(TokenKind::Whitespace, |t| t.kind)
}

fn kind_names(s: &str) -> Vec<&'static str> {
    toks(s)
        .iter()
        .map(|t| match t.kind {
            TokenKind::Number(_) => "number",
            TokenKind::Str(_) => "string",
            TokenKind::Bool(_) => "bool",
            TokenKind::Error { .. } => "error",
            TokenKind::Ref { .. } => "ref",
            TokenKind::Name { .. } => "name",
            TokenKind::Function(_) => "function",
            TokenKind::Structured(_) => "structured",
            TokenKind::Operator(_) => "operator",
            TokenKind::LParen => "lparen",
            TokenKind::RParen => "rparen",
            TokenKind::LBrace => "lbrace",
            TokenKind::RBrace => "rbrace",
            TokenKind::Comma => "comma",
            TokenKind::Semicolon => "semicolon",
            TokenKind::Whitespace => "whitespace",
            TokenKind::Invalid(_) => "invalid",
        })
        .collect()
}

fn address(s: &str) -> Option<RefAddress> {
    match first(s) {
        TokenKind::Ref { address, .. } => Some(address),
        _ => None,
    }
}

fn prefix(s: &str) -> Option<SheetPrefix> {
    match first(s) {
        TokenKind::Ref { prefix, .. }
        | TokenKind::Name { prefix, .. }
        | TokenKind::Error { prefix, .. } => prefix,
        _ => None,
    }
}

fn structured(s: &str) -> Option<StructuredRef> {
    match first(s) {
        TokenKind::Structured(b) => Some(*b),
        _ => None,
    }
}

fn cell(row: u32, col: u32, row_abs: bool, col_abs: bool) -> RefAddress {
    RefAddress::Cell {
        row,
        col,
        row_abs,
        col_abs,
    }
}

#[test]
fn splits_a_simple_expression_with_spans() {
    let t = tokenize("A1+2", &ParseOptions::default(), 1);
    let got: Vec<(u32, u32)> = t.iter().map(|x| (x.start, x.end)).collect();
    assert_eq!(got, vec![(1, 3), (3, 4), (4, 5)]);
    assert_eq!(
        t[0].kind,
        TokenKind::Ref {
            prefix: None,
            address: RefAddress::cell(0, 0)
        }
    );
    assert_eq!(t[1].kind, TokenKind::Operator(OperatorToken::Plus));
    assert_eq!(t[2].kind, TokenKind::Number(2.0));
}

#[test]
fn recognises_absolute_and_mixed_references() {
    assert_eq!(address("$B3"), Some(cell(2, 1, false, true)));
    assert_eq!(address("B$3"), Some(cell(2, 1, true, false)));
    assert_eq!(address("$B$3"), Some(cell(2, 1, true, true)));
}

#[test]
fn reads_areas_whole_columns_and_whole_rows_as_single_tokens() {
    assert_eq!(
        address("A1:B2"),
        Some(RefAddress::area(0, 0, false, false, 1, 1, false, false))
    );
    assert_eq!(
        address("$A:C"),
        Some(RefAddress::Cols {
            c0: 0,
            c1: 2,
            c0_abs: true,
            c1_abs: false
        })
    );
    assert_eq!(
        address("2:$5"),
        Some(RefAddress::Rows {
            r0: 1,
            r1: 4,
            r0_abs: false,
            r1_abs: true
        })
    );
}

#[test]
fn normalises_reversed_areas() {
    assert_eq!(
        address("B5:A1"),
        Some(RefAddress::Area {
            r0: 0,
            c0: 0,
            r1: 4,
            c1: 1,
            r0_abs: false,
            c0_abs: false,
            r1_abs: false,
            c1_abs: false
        })
    );
}

#[test]
fn handles_sheet_prefixes_quoted_names_and_3d_ranges() {
    let p = |sheet: &str, sheet2: Option<&str>, book: Option<&str>| SheetPrefix {
        workbook: book.map(str::to_owned),
        sheet: sheet.to_owned(),
        sheet2: sheet2.map(str::to_owned),
    };
    assert_eq!(prefix("Sheet2!C3"), Some(p("Sheet2", None, None)));
    assert_eq!(prefix("'My Sheet'!A1"), Some(p("My Sheet", None, None)));
    assert_eq!(prefix("'It''s'!A1"), Some(p("It's", None, None)));
    assert_eq!(prefix("Jan:Dec!B2"), Some(p("Jan", Some("Dec"), None)));
    assert_eq!(prefix("'Q 1:Q 4'!B2"), Some(p("Q 1", Some("Q 4"), None)));
    assert_eq!(prefix("[1]Sheet1!A1"), Some(p("Sheet1", None, Some("1"))));
    assert_eq!(
        prefix("'[Book 2.xlsx]Data'!A1"),
        Some(p("Data", None, Some("Book 2.xlsx")))
    );
    assert_eq!(prefix("Données!A1"), Some(p("Données", None, None)));
}

#[test]
fn an_identifier_followed_by_a_paren_is_a_function_even_if_it_looks_like_a_cell() {
    assert_eq!(
        kind_names("LOG10(100)"),
        ["function", "lparen", "number", "rparen"]
    );
    assert_eq!(first("log10(1)"), TokenKind::Function("LOG10".into()));
}

#[test]
fn keeps_dotted_function_names_and_xlfn_prefixes() {
    assert_eq!(
        first("_xlfn.XLOOKUP(1)"),
        TokenKind::Function("_XLFN.XLOOKUP".into())
    );
    assert_eq!(first("STDEV.S(1)"), TokenKind::Function("STDEV.S".into()));
}

#[test]
fn distinguishes_names_booleans_and_cell_references() {
    assert_eq!(kind_names("Sales"), ["name"]);
    assert_eq!(first("true"), TokenKind::Bool(true));
    assert_eq!(kind_names("A1B"), ["name"]);
    assert_eq!(kind_names("XFD1"), ["ref"]);
    assert_eq!(kind_names("XFE1"), ["name"]);
    assert_eq!(kind_names("A1048577"), ["name"]);
    assert_eq!(kind_names("R1C1"), ["name"]);
}

#[test]
fn reads_sheet_scoped_names_and_sheet_ref_errors() {
    assert_eq!(
        first("Sheet1!Total"),
        TokenKind::Name {
            name: "Total".into(),
            prefix: Some(SheetPrefix::sheet("Sheet1"))
        }
    );
    assert_eq!(
        first("Sheet1!#REF!"),
        TokenKind::Error {
            code: ErrorValue::Ref,
            prefix: Some(SheetPrefix::sheet("Sheet1"))
        }
    );
}

#[test]
fn reads_strings_with_doubled_quotes() {
    assert_eq!(
        first("\"say \"\"hi\"\"\""),
        TokenKind::Str("say \"hi\"".into())
    );
    assert_eq!(first("\"日本\""), TokenKind::Str("日本".into()));
}

#[test]
fn reports_an_unterminated_string_as_invalid_instead_of_failing() {
    assert_eq!(kind_names("\"abc"), ["invalid"]);
}

#[test]
fn reads_every_error_literal_case_insensitively() {
    for e in [
        ErrorValue::Null,
        ErrorValue::Div0,
        ErrorValue::Value,
        ErrorValue::Ref,
        ErrorValue::Name,
        ErrorValue::Num,
        ErrorValue::NA,
        ErrorValue::Spill,
        ErrorValue::Calc,
    ] {
        assert_eq!(
            first(e.code()),
            TokenKind::Error {
                code: e,
                prefix: None
            }
        );
    }
    assert_eq!(
        first("#n/a"),
        TokenKind::Error {
            code: ErrorValue::NA,
            prefix: None
        }
    );
}

#[test]
fn reads_numbers_in_all_their_forms() {
    assert_eq!(first("1.5e3"), TokenKind::Number(1500.0));
    assert_eq!(first(".5"), TokenKind::Number(0.5));
    assert_eq!(first("2."), TokenKind::Number(2.0));
    assert_eq!(first("1E-2"), TokenKind::Number(0.01));
}

#[test]
fn tells_the_spill_operator_from_an_error_literal() {
    assert_eq!(kind_names("A1#"), ["ref", "operator"]);
    assert_eq!(kind_names("A1 #N/A"), ["ref", "whitespace", "error"]);
}

#[test]
fn reads_two_character_comparison_operators() {
    let src = "1<>2<=3>=4";
    let texts: Vec<&str> = toks(src)
        .iter()
        .map(|t| &src[t.start as usize..t.end as usize])
        .collect();
    assert_eq!(texts, ["1", "<>", "2", "<=", "3", ">=", "4"]);
}

#[test]
fn reads_structured_references() {
    let s = |table: Option<&str>,
             specials: Vec<StructuredSpecial>,
             this_row: bool,
             columns: Option<(&str, Option<&str>)>| StructuredRef {
        table: table.map(str::to_owned),
        specials,
        this_row,
        columns: columns.map(|(a, b)| (a.to_owned(), b.map(str::to_owned))),
    };
    assert_eq!(
        structured("Sales[Amount]"),
        Some(s(Some("Sales"), vec![], false, Some(("Amount", None))))
    );
    assert_eq!(
        structured("[@Qty]"),
        Some(s(None, vec![], true, Some(("Qty", None))))
    );
    assert_eq!(
        structured("T[[#Headers],[#Data],[A]:[C]]"),
        Some(s(
            Some("T"),
            vec![StructuredSpecial::Headers, StructuredSpecial::Data],
            false,
            Some(("A", Some("C")))
        ))
    );
    assert_eq!(
        structured("T[[#This Row],[Unit Price]]"),
        Some(s(Some("T"), vec![], true, Some(("Unit Price", None))))
    );
    assert_eq!(
        structured("T[[Col'#1]]").and_then(|x| x.columns),
        Some(("Col#1".to_owned(), None))
    );
    assert_eq!(
        structured("T[#All]").map(|x| x.specials),
        Some(vec![StructuredSpecial::All])
    );
    assert_eq!(structured("T[@]"), Some(s(Some("T"), vec![], true, None)));
}

#[test]
fn flags_malformed_structured_references() {
    assert_eq!(kind_names("T[[#Bogus]]"), ["invalid"]);
    assert_eq!(kind_names("T[[A]"), ["invalid"]);
}

#[test]
fn does_not_treat_numbers_inside_an_array_constant_as_rows() {
    assert_eq!(
        kind_names("{1,2}"),
        ["lbrace", "number", "comma", "number", "rbrace"]
    );
}

#[test]
fn tokenizes_r1c1_references_relative_to_an_anchor() {
    let t = tokenize("R[-1]C+RC[2]+R2C3", &ParseOptions::r1c1(4, 4), 0);
    let addr = |i: usize| match &t[i].kind {
        TokenKind::Ref { address, .. } => Some(*address),
        _ => None,
    };
    assert_eq!(addr(0), Some(cell(3, 4, false, false)));
    assert_eq!(addr(2), Some(cell(4, 6, false, false)));
    assert_eq!(addr(4), Some(cell(1, 2, true, true)));
}

#[test]
fn tokenizes_r1c1_rows_and_columns_and_keeps_names_like_rate() {
    let o = ParseOptions::r1c1(0, 0);
    let one = |s: &str| tokenize(s, &o, 0).into_iter().next().map(|t| t.kind);
    assert_eq!(
        one("R2:R3"),
        Some(TokenKind::Ref {
            prefix: None,
            address: RefAddress::Rows {
                r0: 1,
                r1: 2,
                r0_abs: true,
                r1_abs: true
            }
        })
    );
    assert_eq!(
        one("C[1]"),
        Some(TokenKind::Ref {
            prefix: None,
            address: RefAddress::Cols {
                c0: 1,
                c1: 1,
                c0_abs: false,
                c1_abs: false
            }
        })
    );
    assert_eq!(one("RATE(1)"), Some(TokenKind::Function("RATE".into())));
    assert!(matches!(one("Rate"), Some(TokenKind::Name { .. })));
}

#[test]
fn an_out_of_sheet_relative_r1c1_reference_is_ref_error() {
    let t = tokenize("R[-1]C", &ParseOptions::r1c1(0, 0), 0);
    assert_eq!(
        t[0].kind,
        TokenKind::Error {
            code: ErrorValue::Ref,
            prefix: None
        }
    );
}

#[test]
fn marks_unknown_characters_invalid() {
    assert!(kind_names("1 ~ 2").contains(&"invalid"));
    assert!(kind_names("1 ☃").contains(&"name") || kind_names("1 ☃").contains(&"invalid"));
}

#[test]
fn never_panics_on_arbitrary_input() {
    for s in [
        "'",
        "[",
        "[[",
        "'a",
        "''!",
        "$",
        "$$A$1",
        "A1:",
        ":",
        "#",
        "##",
        "R[",
        "C[-",
        "[1",
        "'[a]'!",
        "T[",
        "T[@",
        "{",
        "}",
        "\"",
        "1e",
        "1e+",
        "😀",
        "A😀1",
        "'😀'!A1",
        "[😀]S!A1",
    ] {
        let _ = tokenize(s, &ParseOptions::default(), 0);
        let _ = tokenize(s, &ParseOptions::r1c1(5, 5), 0);
    }
}
