// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parser tests: precedence and associativity, calls, literals, spans and errors (ported from the
//! v1.0 TypeScript suite).

use kiln_formula::ErrorValue;
use kiln_formula::parser::{
    BinaryOp, MAX_AST_DEPTH, Node, NodeKind, ParseOptions, PrintOptions, Span, parse, print,
};

/// Fully parenthesised rendering to check precedence and associativity.
fn shape(n: &Node) -> String {
    match &n.kind {
        NodeKind::Binary { op, left, right } => {
            let o = if *op == BinaryOp::Intersect {
                "∩"
            } else {
                op.as_str()
            };
            format!("({}{o}{})", shape(left), shape(right))
        }
        NodeKind::Unary { op, operand } => format!("({}{})", op.as_str(), shape(operand)),
        NodeKind::Postfix { op, operand } => format!("({}{})", shape(operand), op.as_str()),
        NodeKind::Paren(e) => shape(e),
        NodeKind::Call { name, args } => format!(
            "{name}[{}]",
            args.iter().map(shape).collect::<Vec<_>>().join(";")
        ),
        NodeKind::Invoke { callee, args } => format!(
            "{}<{}>",
            shape(callee),
            args.iter().map(shape).collect::<Vec<_>>().join(";")
        ),
        NodeKind::Empty => "∅".to_owned(),
        _ => print(n, &PrintOptions::default()),
    }
}

fn p(s: &str) -> String {
    match parse(s, &ParseOptions::default()) {
        Ok(n) => shape(&n),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn precedence_and_associativity() {
    for (src, want) in [
        ("=1+2*3", "(1+(2*3))"),
        ("=(1+2)*3", "((1+2)*3)"),
        ("=-2^2", "((-2)^2)"),
        ("=2^-2", "(2^(-2))"),
        ("=2^3^2", "((2^3)^2)"),
        ("=-5%", "((-5)%)"),
        ("=5%^2", "((5%)^2)"),
        ("=1-2-3", "((1-2)-3)"),
        ("=8/4/2", "((8/4)/2)"),
        ("=\"a\"&1+2", "(\"a\"&(1+2))"),
        ("=1+2&\"x\"", "((1+2)&\"x\")"),
        ("=1&2=12", "((1&2)=12)"),
        ("=1<2=TRUE", "((1<2)=TRUE)"),
        ("=A1:B2 B1:C3", "(A1:B2∩B1:C3)"),
        ("=SUM(A1:B2 B2:C3)", "SUM[(A1:B2∩B2:C3)]"),
        ("=(A1,B2)", "(A1,B2)"),
        ("=SUM((A1,B2),C3)", "SUM[(A1,B2);C3]"),
        ("=A1:A3:C2", "(A1:A3:C2)"),
        ("=A1:INDEX(B:B,3)", "(A1:INDEX[B:B;3])"),
        ("=-A1:B2", "(-A1:B2)"),
        ("=@A1:A10", "(@A1:A10)"),
        ("=A1#", "(A1#)"),
        ("=SUM(A1#)", "SUM[(A1#)]"),
        ("=+A1", "(+A1)"),
        ("=--A1", "(-(-A1))"),
        ("=1--1", "(1-(-1))"),
        ("=A1 :B2", "(A1:B2)"),
        ("= 1 + 2 ", "(1+2)"),
    ] {
        assert_eq!(p(src), want, "{src}");
    }
}

#[test]
fn empty_argument_lists_and_empty_arguments() {
    assert_eq!(p("=NOW()"), "NOW[]");
    assert_eq!(p("=IF(A1,,2)"), "IF[A1;∅;2]");
    assert_eq!(p("=F(,)"), "F[∅;∅]");
    assert_eq!(p("=F(1,)"), "F[1;∅]");
}

#[test]
fn upper_cases_function_names() {
    assert_eq!(p("=sum(1)"), "SUM[1]");
}

#[test]
fn lambda_invocation() {
    assert_eq!(p("=LAMBDA(x,x+1)(2)"), "LAMBDA[x;(x+1)]<2>");
    assert_eq!(p("=(LAMBDA(x,x))(3)"), "LAMBDA[x;x]<3>");
}

#[test]
fn nested_calls() {
    assert_eq!(
        p("=IF(AND(A1>0,B1<1),SUM(C:C),\"\")"),
        "IF[AND[(A1>0);(B1<1)];SUM[C:C];\"\"]"
    );
}

fn lit(kind: NodeKind) -> Node {
    Node::new(kind, Span::default())
}

#[test]
fn array_constants_with_negative_numbers_and_mixed_types() {
    let ast =
        parse("={1,-2;\"a\",TRUE;#N/A,0.5}", &ParseOptions::default()).map(|n| n.without_spans());
    let want = lit(NodeKind::Array(vec![
        vec![lit(NodeKind::Number(1.0)), lit(NodeKind::Number(-2.0))],
        vec![lit(NodeKind::Str("a".into())), lit(NodeKind::Bool(true))],
        vec![
            lit(NodeKind::Error {
                code: ErrorValue::NA,
                prefix: None,
            }),
            lit(NodeKind::Number(0.5)),
        ],
    ]));
    assert_eq!(ast, Ok(want));
}

#[test]
fn quoted_sheet_names_and_3d_references() {
    let ast = parse("='Q1 Data'!B2", &ParseOptions::default());
    assert!(matches!(
        ast.map(|n| n.kind),
        Ok(NodeKind::Ref { prefix: Some(p), .. }) if p.sheet == "Q1 Data"
    ));
    let ast = parse("=SUM(Jan:Mar!B2)", &ParseOptions::default());
    let Ok(NodeKind::Call { args, .. }) = ast.map(|n| n.kind) else {
        panic!("expected a call");
    };
    assert!(matches!(
        &args[0].kind,
        NodeKind::Ref { prefix: Some(p), .. } if p.sheet == "Jan" && p.sheet2.as_deref() == Some("Mar")
    ));
}

#[test]
fn an_overflowing_number_literal_is_num_error() {
    let ast = parse("=1e400", &ParseOptions::default()).map(|n| n.kind);
    assert_eq!(
        ast,
        Ok(NodeKind::Error {
            code: ErrorValue::Num,
            prefix: None
        })
    );
}

#[test]
fn accepts_formulas_with_or_without_the_leading_equals() {
    let a = parse("1+1", &ParseOptions::default()).map(|n| n.without_spans());
    let b = parse("=1+1", &ParseOptions::default()).map(|n| n.without_spans());
    assert_eq!(a, b);
}

#[test]
fn records_spans_in_the_original_text() {
    let ast = parse("=SUM(A1, 2)", &ParseOptions::default());
    let Ok(n) = ast else { panic!("parse") };
    assert_eq!(n.span, Span::new(1, 11));
    let NodeKind::Call { args, .. } = &n.kind else {
        panic!("call")
    };
    assert_eq!(args[0].span, Span::new(5, 7));
    assert_eq!(args[1].span, Span::new(9, 10));
}

#[test]
fn spans_binary_expressions_from_left_to_right_operand() {
    let src = "=A1 + B1*2";
    let Ok(n) = parse(src, &ParseOptions::default()) else {
        panic!("parse")
    };
    assert_eq!(
        &src[n.span.start as usize..n.span.end as usize],
        "A1 + B1*2"
    );
}

#[test]
fn places_empty_arguments_right_after_the_separator() {
    let Ok(n) = parse("=IF(1,,2)", &ParseOptions::default()) else {
        panic!("parse")
    };
    let NodeKind::Call { args, .. } = &n.kind else {
        panic!("call")
    };
    assert_eq!(args[1], Node::new(NodeKind::Empty, Span::new(6, 6)));
}

#[test]
fn spans_are_byte_offsets_with_multibyte_text() {
    let src = "=\"日本\"&A1";
    let Ok(n) = parse(src, &ParseOptions::default()) else {
        panic!("parse")
    };
    let NodeKind::Binary { right, .. } = &n.kind else {
        panic!("binary")
    };
    assert_eq!(
        &src[right.span.start as usize..right.span.end as usize],
        "A1"
    );
}

#[test]
fn rejects_malformed_formulas_with_a_message() {
    for (src, msg) in [
        ("=", "empty"),
        ("=1+", "end"),
        ("=SUM(1", "parenthesis"),
        ("=1)", "closing parenthesis"),
        ("=(1", "\")\""),
        ("={1,2;3}", "same length"),
        ("={A1}", "Array constants"),
        ("=\"abc", "quotation"),
        ("=1 2 +", ""),
        ("=*3", "Unexpected"),
    ] {
        match parse(src, &ParseOptions::default()) {
            Ok(_) => panic!("{src} should not parse"),
            Err(e) => assert!(e.message.contains(msg), "{src}: {}", e.message),
        }
    }
}

#[test]
fn reports_the_offending_span() {
    let e = parse("=1+*2", &ParseOptions::default()).err();
    assert_eq!(e.map(|e| e.span), Some(Span::new(3, 4)));
}

#[test]
fn refuses_runaway_nesting_without_overflowing_the_stack() {
    let deep = format!("={}1{}", "(".repeat(3000), ")".repeat(3000));
    let e = parse(&deep, &ParseOptions::default()).err();
    assert!(e.is_some_and(|e| e.message.contains("nested too deeply")));
    let chain = format!("=1{}", "+1".repeat(MAX_AST_DEPTH + 10));
    assert!(parse(&chain, &ParseOptions::default()).is_err());
    let ok = format!("=1{}", "+1".repeat(500));
    assert!(parse(&ok, &ParseOptions::default()).is_ok());
    let negs = format!("={}1", "-".repeat(3000));
    assert!(parse(&negs, &ParseOptions::default()).is_err());
    let long = format!("=\"{}\"", "x".repeat(9000));
    assert!(parse(&long, &ParseOptions::default()).is_err());
}

#[test]
fn never_panics_on_garbage() {
    let samples = [
        "=",
        "==",
        "=(",
        "=)",
        "={",
        "=}",
        "={1,",
        "={1;",
        "=SUM(,",
        "=A1#",
        "=#",
        "=@",
        "=1%%%",
        "=-",
        "=+",
        "=A1:",
        "=:A1",
        "=,",
        "=(,)",
        "=F(",
        "=LAMBDA(",
        "=1 2",
        "=A1 B1 C1",
        "=@@A1",
        "='a'!",
        "=[",
        "=T[",
        "=Sheet1!",
        "=😀",
        "=\"😀\"",
        "={\"a\";",
        "=(1)(2)(3)",
    ];
    for s in samples {
        let _ = parse(s, &ParseOptions::default());
        let _ = parse(s, &ParseOptions::r1c1(3, 3));
    }
}
