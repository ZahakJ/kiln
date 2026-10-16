// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Formula Lab's step tree (`Engine::explain`), ported from the v1.0 TypeScript suite
//! (`test/engine/explain.test.ts`) plus Kiln additions.

mod engine_support;

use engine_support::*;
use kiln_formula::engine::{CellInput, ExplainKind, ExplainStep, format_value};
use kiln_formula::values::{Array2D, ErrorValue, Value};

fn child(s: &ExplainStep, i: usize) -> &ExplainStep {
    s.children
        .get(i)
        .unwrap_or_else(|| panic!("step {:?} has no child {i}", s.text))
}

#[test]
fn returns_every_sub_expression_with_its_value_and_source_span() {
    let mut e = sheet(&[("A1", 3.into())]);
    e.recalc();
    let src = "=IF(A1>1, A1*2, 1/0)";
    let t = e.explain(src).expect("explain");
    assert_eq!(t.kind, ExplainKind::Call);
    assert_eq!(t.label, "IF");
    assert_eq!(t.display, "6");
    assert!(
        t.description.is_some_and(|d| d.contains("condition")),
        "{:?}",
        t.description
    );
    let (cond, yes, no) = (child(&t, 0), child(&t, 1), child(&t, 2));
    assert_eq!(cond.text, "A1>1");
    assert_eq!(cond.display, "TRUE");
    assert_eq!(child(cond, 0).reference.as_deref(), Some("A1"));
    assert_eq!(child(cond, 0).display, "A1 → 3");
    assert_eq!(yes.display, "6");
    assert_eq!(&src[yes.span.start as usize..yes.span.end as usize], "A1*2");
    assert!(!no.evaluated);
    assert!(no.children.iter().all(|c| !c.evaluated));
}

#[test]
fn counts_repeated_evaluation_inside_lambdas() {
    let mut e = sheet(&[]);
    let t = e.explain("=MAP({1,2,3},LAMBDA(x,x*10))").expect("explain");
    let lambda = child(&t, 1);
    let body = child(lambda, 1);
    assert_eq!(body.count, 3);
    assert_eq!(t.display, "{10,20,30}");
}

#[test]
fn previews_ranges_and_arrays_compactly() {
    let mut e = sheet(&[("A1", 1.into()), ("A2", "x".into())]);
    e.recalc();
    let t = e.explain("=SUM(A1:A3)").expect("explain");
    assert_eq!(child(&t, 0).display, "A1:A3 → {1;\"x\";(blank)}");
    let big = Value::array(Array2D::build(10, 10, |r, c| {
        Value::number((r * 10 + c) as f64)
    }));
    assert!(
        format_value(Some(&big)).starts_with("{0,1,2,3,4,5,…;"),
        "{}",
        format_value(Some(&big))
    );
    assert_eq!(format_value(Some(&Value::Blank)), "(blank)");
}

#[test]
fn labels_operators_errors_and_omitted_arguments() {
    let mut e = sheet(&[]);
    let t = e.explain("=IFERROR(1/0,)").expect("explain");
    assert_eq!(t.display, "0");
    assert_eq!(child(&t, 0).label, "/");
    assert_eq!(child(&t, 0).display, "#DIV/0!");
    assert_eq!(child(&t, 1).label, "(omitted)");
}

#[test]
fn shows_sheet_qualified_references_for_other_sheets() {
    let mut e = sheet(&[]);
    e.add_sheet("Data 2").expect("sheet");
    e.set_cell("Data 2", 0, 0, CellInput::Value(Value::number(5.0)))
        .expect("set");
    let t = e.explain("='Data 2'!A1+1").expect("explain");
    assert_eq!(child(&t, 0).reference.as_deref(), Some("'Data 2'!A1"));
    assert_eq!(t.display, "6");
}

// ── Kiln additions ──────────────────────────────────────────────────────────────────────────

#[test]
fn formula_lab_example_shows_each_level() {
    // The WP-72 Formula Lab acceptance formula.
    let mut e = sheet(&[("A1", 1.into()), ("A2", 2.into()), ("A3", 3.into())]);
    e.recalc();
    let src = r#"=IF(SUM(A1:A3)>5,"x","y")"#;
    let t = e.explain(src).expect("explain");
    assert_eq!(t.display, "\"x\"");
    assert_eq!(t.value, Some(Value::from("x")));
    let cond = child(&t, 0);
    assert_eq!((cond.kind, cond.label.as_str()), (ExplainKind::Binary, ">"));
    assert_eq!(cond.display, "TRUE");
    let sum = child(cond, 0);
    assert_eq!(sum.label, "SUM");
    assert_eq!(sum.display, "6");
    assert!(sum.description.is_some());
    assert_eq!(child(sum, 0).display, "A1:A3 → {1;2;3}");
    assert_eq!(child(cond, 1).display, "5");
    assert!(child(&t, 1).evaluated);
    assert!(!child(&t, 2).evaluated);
    assert_eq!(child(&t, 2).count, 0);
    assert_eq!(child(&t, 2).value, None);
    // Every span slices back to the step's text.
    fn spans(s: &ExplainStep, src: &str) {
        assert_eq!(&src[s.span.start as usize..s.span.end as usize], s.text);
        s.children.iter().for_each(|c| spans(c, src));
    }
    spans(&t, src);
}

#[test]
fn parentheses_share_the_value_of_their_contents() {
    let mut e = sheet(&[]);
    let t = e.explain("=(1+2)*-3%").expect("explain");
    assert_eq!(t.display, "-0.09");
    let paren = child(&t, 0);
    assert_eq!(
        (paren.kind, paren.label.as_str()),
        (ExplainKind::Paren, "( )")
    );
    assert_eq!(paren.display, "3");
    assert_eq!(child(paren, 0).display, "3");
    // Excel's negation binds tighter than percent: -3% is (-3)%.
    let pct = child(&t, 1);
    assert_eq!((pct.label.as_str(), pct.display.as_str()), ("%", "-0.03"));
    let neg = child(pct, 0);
    assert_eq!((neg.label.as_str(), neg.display.as_str()), ("-", "-3"));
}

#[test]
fn array_constant_elements_are_evaluated_literals() {
    let mut e = sheet(&[]);
    let t = e.explain("=SUM({1,2;3,\"a\"})").expect("explain");
    let arr = child(&t, 0);
    assert_eq!(arr.kind, ExplainKind::Array);
    assert_eq!(arr.display, "{1,2;3,\"a\"}");
    let labels: Vec<_> = arr.children.iter().map(|c| c.display.as_str()).collect();
    assert_eq!(labels, ["1", "2", "3", "\"a\""]);
    assert!(arr.children.iter().all(|c| c.evaluated && c.count == 1));
}

#[test]
fn whole_columns_names_and_lambda_values() {
    let mut e = sheet(&[("B1", 4.into()), ("B2", 6.into())]);
    e.define_name("Rate", "=0.5", None).expect("name");
    e.recalc();
    let t = e.explain("=SUM(B:B)*Rate").expect("explain");
    assert_eq!(t.display, "5");
    assert_eq!(child(child(&t, 0), 0).reference.as_deref(), Some("B:B"));
    assert_eq!(child(&t, 1).kind, ExplainKind::Name);
    assert_eq!(child(&t, 1).display, "0.5");
    let l = e.explain("=LAMBDA(x,y,x+y)").expect("explain");
    assert_eq!(l.display, "#CALC!");
    let inner = e.explain("=LET(f,LAMBDA(x,y,x+y),f)").expect("explain");
    assert_eq!(child(&inner, 1).display, "LAMBDA(x,y)");
}

#[test]
fn explain_at_uses_the_given_cell_and_does_not_store_anything() {
    let mut e = sheet(&[("B2", 7.into())]);
    e.recalc();
    let t = e.explain_at("=B2+A1", "Sheet1", 1, 1).expect("explain");
    // A self-reference from B2 is read, not stored: no cycle, nothing written.
    assert_eq!(t.display, "7");
    assert_eq!(get(&e, "B2"), num(7.0));
    assert!(e.explain_at("=1", "Nope", 0, 0).is_err());
    assert!(e.explain("=1+").is_err());
    let r = e.explain("=#N/A").expect("explain");
    assert_eq!((r.kind, r.display.as_str()), (ExplainKind::Error, "#N/A"));
    assert_eq!(r.value, Some(Value::Error(ErrorValue::NA)));
}
