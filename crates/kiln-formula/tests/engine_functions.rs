// SPDX-License-Identifier: AGPL-3.0-or-later
//! The core functions, operators, reference operators, the registry as an extension point and
//! the evaluation limits (ported from the v1.0 TypeScript suite where the functions are core).

mod engine_support;

use std::sync::Arc;

use engine_support::*;
use kiln_formula::engine::{Arg, CallContext, EngineOptions, EvalValue, scalar_arg};
use kiln_formula::registry::{
    ArgMask, DocsHandle, FunctionRegistry, FunctionSpec, FunctionTable, core_functions,
};
use kiln_formula::values::{ErrorValue, Value, to_number};

fn data() -> Vec<(&'static str, Lit)> {
    vec![
        ("A1", 10.into()),
        ("A2", 20.into()),
        ("A3", "text".into()),
        ("A4", true.into()),
        ("A5", 30.into()),
        ("B1", "apple".into()),
        ("B2", "Banana".into()),
        ("B3", "cherry".into()),
        ("C1", 1.into()),
        ("C2", 2.into()),
        ("C3", 3.into()),
        ("D1", "a".into()),
        ("D2", "b".into()),
        ("D3", "c".into()),
    ]
}

fn b(v: bool) -> Value {
    Value::Bool(v)
}

#[test]
fn sum_follows_excel_rules() {
    for (f, want) in [
        ("=SUM(A1:A5)", num(60.0)),
        ("=SUM(A1:A5,5)", num(65.0)),
        ("=SUM(1,\"2\",TRUE)", num(4.0)),
        ("=SUM({1,\"2\",TRUE})", num(1.0)),
        ("=SUM(A1:A5,A1)", num(70.0)),
        ("=SUM(A9:A12)", num(0.0)),
        ("=SUM(1,\"x\")", error(ErrorValue::Value)),
        ("=SUM(A1,1/0)", error(ErrorValue::Div0)),
        ("=SUM(A:A)", num(60.0)),
        ("=SUM(1:1)", num(11.0)),
        ("=SUM((A1,A2),A5)", num(60.0)),
        ("=SUM(1E+308,1E+308)", error(ErrorValue::Num)),
    ] {
        assert_eq!(calc(f, &data()), want, "{f}");
    }
}

#[test]
fn logical_functions() {
    for (f, want) in [
        ("=IF(TRUE,1,2)", num(1.0)),
        ("=IF(FALSE,1,2)", num(2.0)),
        ("=IF(FALSE,1)", b(false)),
        ("=IF(TRUE,,2)", num(0.0)),
        ("=IF(FALSE,1,)", num(0.0)),
        ("=IF(\"TRUE\",1,2)", num(1.0)),
        ("=IF(\"x\",1,2)", error(ErrorValue::Value)),
        ("=IF(1/0,1,2)", error(ErrorValue::Div0)),
        ("=IF(TRUE,\"ok\",1/0)", text("ok")),
        ("=IF(0,1/0,\"lazy\")", text("lazy")),
        ("=AND(TRUE,1,A1)", b(true)),
        ("=AND(TRUE,0)", b(false)),
        ("=AND(A3)", error(ErrorValue::Value)),
        ("=AND(\"x\")", error(ErrorValue::Value)),
        ("=OR(FALSE,0,A4)", b(true)),
        ("=OR(FALSE,1/0)", error(ErrorValue::Div0)),
        ("=AND({TRUE,FALSE})", b(false)),
        ("=OR({FALSE,TRUE})", b(true)),
        ("=NOT(0)", b(true)),
        ("=NOT(\"FALSE\")", b(true)),
        ("=TRUE()", b(true)),
        ("=FALSE()", b(false)),
        ("=IF(TRUE(),FALSE(),1)", b(false)),
    ] {
        assert_eq!(calc(f, &data()), want, "{f}");
    }
}

#[test]
fn if_and_not_lift_over_arrays_element_wise() {
    assert_eq!(
        calc("=IF({1,0,1},\"y\",\"n\")", &[]),
        arr(vec![vec![text("y"), text("n"), text("y")]])
    );
    assert_eq!(
        calc("=IF(C1:C3>1,D1:D3,\"-\")", &data()),
        arr(vec![vec![text("-")], vec![text("b")], vec![text("c")]])
    );
    assert_eq!(
        calc("=NOT({TRUE,FALSE})", &[]),
        arr(vec![vec![b(false), b(true)]])
    );
    assert_eq!(
        calc("=IFERROR(1/{1,0,2},\"x\")", &[]),
        arr(vec![vec![num(1.0), text("x"), num(0.5)]])
    );
}

#[test]
fn operators() {
    for (f, want) in [
        ("=0.1+0.2=0.3", b(true)),
        ("=-2^2", num(4.0)),
        ("=2^3^2", num(64.0)),
        ("=2^-1", num(0.5)),
        ("=0^0", error(ErrorValue::Num)),
        ("=(-8)^(1/3)", error(ErrorValue::Num)),
        ("=0^-1", error(ErrorValue::Div0)),
        ("=1/0", error(ErrorValue::Div0)),
        ("=5%", num(0.05)),
        ("=200%%", num(0.02)),
        ("=\"3\"+\"4\"", num(7.0)),
        ("=\"1,234\"+1", num(1235.0)),
        ("=\"50%\"*2", num(1.0)),
        ("=\"abc\"+1", error(ErrorValue::Value)),
        ("=TRUE+TRUE", num(2.0)),
        ("=\"a\"=\"A\"", b(true)),
        ("=\"a\"<\"B\"", b(true)),
        ("=1<\"a\"", b(true)),
        ("=\"z\"<TRUE", b(true)),
        ("=A1=0", b(true)),
        ("=A1=\"\"", b(true)),
        ("=#N/A=1", error(ErrorValue::NA)),
        ("=10^308*10", error(ErrorValue::Num)),
        ("=+\"text\"", text("text")),
        ("=-\"2\"", num(-2.0)),
        ("=\"a\"&1.5&TRUE", text("a1.5TRUE")),
        ("=1&2=12", b(false)),
        ("=\"2024-01-16\"-\"2024-01-15\"", num(1.0)),
        (
            "={1,2,3}+{10;20}",
            arr(vec![
                vec![num(11.0), num(12.0), num(13.0)],
                vec![num(21.0), num(22.0), num(23.0)],
            ]),
        ),
        (
            "={1,2,3}+{1,2}",
            arr(vec![vec![num(2.0), num(4.0), error(ErrorValue::NA)]]),
        ),
        ("={1,2}&\"x\"", arr(vec![vec![text("1x"), text("2x")]])),
        ("=-{1,2}", arr(vec![vec![num(-1.0), num(-2.0)]])),
        ("={10,20}%", arr(vec![vec![num(0.1), num(0.2)]])),
    ] {
        assert_eq!(calc(f, &[]), want, "{f}");
    }
}

#[test]
fn reference_operators() {
    let cells: Vec<(&str, Lit)> = vec![
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("B1", 3.into()),
        ("B2", 4.into()),
        ("C3", 9.into()),
    ];
    assert_eq!(calc("=SUM(A1:B2 B1:C3)", &cells), num(7.0));
    assert_eq!(calc("=A1:A2 C1:C2", &cells), error(ErrorValue::Null));
    assert_eq!(calc("=SUM((A1,C3))", &cells), num(10.0));
    assert_eq!(calc("=SUM(A1:A2:B2)", &cells), num(10.0));
    assert_eq!(calc("=(A1,B1)", &cells), error(ErrorValue::Value));
    assert_eq!(calc("=1+(A1,B1)", &cells), error(ErrorValue::Value));
    assert_eq!(calc("=SUM(A1:INDEX(A1:A2,2))", &cells), num(3.0));
    assert_eq!(calc("=@A1:A2", &cells), num(1.0));
}

#[test]
fn implicit_intersection_picks_the_formula_row_or_column() {
    let mut e = sheet(&[("A1", 5.into()), ("A2", 6.into()), ("A3", 7.into())]);
    e.recalc();
    assert_eq!(
        e.evaluate_at("=@A1:A3", "Sheet1", 2, 4, false).ok(),
        Some(num(7.0))
    );
    assert_eq!(
        e.evaluate_at("=@A1:A3", "Sheet1", 5, 4, false).ok(),
        Some(error(ErrorValue::Value))
    );
    assert_eq!(
        e.evaluate_at("=A1:A3+1", "Sheet1", 1, 4, true).ok(),
        Some(num(7.0))
    );
}

#[test]
fn unknown_functions_and_names_are_name_errors() {
    assert_eq!(calc("=NOPE(1)", &[]), error(ErrorValue::Name));
    assert_eq!(calc("=Unknown+1", &[]), error(ErrorValue::Name));
}

#[test]
fn wrong_argument_counts_are_value_errors() {
    assert_eq!(calc("=NOT(1,2)", &[]), error(ErrorValue::Value));
    assert_eq!(calc("=IF(1)", &[]), error(ErrorValue::Value));
    assert_eq!(calc("=TRUE(1)", &[]), error(ErrorValue::Value));
}

#[test]
fn xlfn_prefixes_are_ignored_when_calling_functions() {
    assert_eq!(calc("=_xlfn.SUM(2,3)", &[]), num(5.0));
    assert_eq!(calc("=_xlfn._xlws.SUM(2,3)", &[]), num(5.0));
}

#[test]
fn the_core_set_is_documented_and_its_examples_hold() {
    let names: Vec<&str> = core_functions().iter().map(|s| s.name).collect();
    assert_eq!(names, ["SUM", "IF", "AND", "OR", "NOT", "TRUE", "FALSE"]);
    let mut e = engine();
    for spec in core_functions() {
        let DocsHandle::Inline(docs) = spec.docs else {
            panic!("{} has no docs", spec.name);
        };
        assert!(!docs.summary.is_empty());
        for (formula, shown) in docs.examples {
            let v = e.evaluate(formula).unwrap_or(Value::Blank);
            let got = match &v {
                Value::Str(s) => format!("\"{s}\""),
                Value::Bool(x) => (if *x { "TRUE" } else { "FALSE" }).to_owned(),
                Value::Number(n) => kiln_formula::values::number_to_text(*n),
                other => format!("{other:?}"),
            };
            assert_eq!(&got, shown, "{}: {formula}", spec.name);
        }
    }
    assert!(core_functions().iter().all(|s| !s.volatile));
}

fn double_it(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let v = scalar_arg(args.first_mut(), ctx);
    match to_number(&v, &ctx.coercion()) {
        Ok(n) => EvalValue::from(n * 2.0),
        Err(e) => EvalValue::from(e),
    }
}

#[test]
fn accepts_new_registrations_through_the_registry() {
    let mut table = FunctionTable::with_core();
    table.register(FunctionSpec {
        lift: ArgMask::ALL,
        ..FunctionSpec::new("DOUBLE.IT", 1, 1, double_it)
    });
    let registry: Arc<dyn FunctionRegistry> = Arc::new(table.clone());
    let mut e = engine_with(EngineOptions {
        registry: Some(registry),
        ..options()
    });
    assert_eq!(e.evaluate("=DOUBLE.IT(21)").ok(), Some(num(42.0)));
    assert_eq!(
        e.evaluate("=DOUBLE.IT({1,2})").ok(),
        Some(arr(vec![vec![num(2.0), num(4.0)]]))
    );
    assert!(table.unregister("double.it"));
    let registry: Arc<dyn FunctionRegistry> = Arc::new(table);
    let mut e = engine_with(EngineOptions {
        registry: Some(registry),
        ..options()
    });
    assert_eq!(
        e.evaluate("=DOUBLE.IT(21)").ok(),
        Some(error(ErrorValue::Name))
    );
}

#[test]
fn deep_formulas_stay_within_a_small_thread_stack() {
    // Hosts may evaluate on threads with 2 MB stacks; the parser and evaluator limits keep the
    // worst cases (deep nesting, long operator chains, runaway lambda recursion) inside that.
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut e = engine();
            e.define_name("LOOP", "=LAMBDA(n,LOOP(n+1))", None)
                .expect("name");
            e.define_name("FACT", "=LAMBDA(n,IF(n<=1,1,n*FACT(n-1)))", None)
                .expect("name");
            let parens = format!("={}1{}", "(".repeat(250), ")".repeat(250));
            let chain = format!("=1{}", "+1".repeat(1000));
            let ifs = format!("={}1{}", "IF(TRUE,".repeat(250), ")".repeat(250));
            let negs = format!("={}1", "-".repeat(250));
            let sums = format!("={}1{}", "SUM(".repeat(250), ")".repeat(250));
            vec![
                e.evaluate(&parens).ok(),
                e.evaluate(&chain).ok(),
                e.evaluate(&ifs).ok(),
                e.evaluate(&negs).ok(),
                e.evaluate(&sums).ok(),
                e.evaluate("=LOOP(1)").ok(),
                e.evaluate("=FACT(150)").ok(),
            ]
        })
        .expect("spawn");
    let results = handle.join().expect("no stack overflow");
    assert_eq!(results[0], Some(num(1.0)));
    assert_eq!(results[1], Some(num(1001.0)));
    assert_eq!(results[2], Some(num(1.0)));
    assert_eq!(results[3], Some(num(1.0)));
    assert_eq!(results[4], Some(num(1.0)));
    assert_eq!(results[5], Some(error(ErrorValue::Num)));
    assert!(matches!(results[6], Some(Value::Number(n)) if n > 5.7e262));
}
