// SPDX-License-Identifier: AGPL-3.0-or-later
//! LET and LAMBDA through the registry's lazy arguments and the call context (ported from the
//! v1.0 TypeScript suite; MAP/REDUCE/… belong to the function library).

mod engine_support;

use engine_support::*;
use kiln_formula::values::{ErrorValue, Value};

fn data() -> Vec<(&'static str, Lit)> {
    vec![("A1", 1.into()), ("A2", 2.into()), ("A3", 3.into())]
}

#[test]
fn let_binds_names_in_order_and_lazily() {
    for (f, want) in [
        ("=LET(x,2,y,3,x*y)", num(6.0)),
        ("=LET(x,2,y,x+1,x*y)", num(6.0)),
        ("=LET(x,1,LET(x,2,x)+x)", num(3.0)),
        ("=LET(r,A1:A3,SUM(r)*2)", num(12.0)),
        (
            "=LET(r,A1:A3,ROW(r))",
            arr(vec![vec![num(1.0)], vec![num(2.0)], vec![num(3.0)]]),
        ),
        ("=LET(x,1/0,5)", num(5.0)),
        ("=LET(x,5)", error(ErrorValue::Value)),
        ("=LET(1,5,1)", error(ErrorValue::Name)),
    ] {
        assert_eq!(calc(f, &data()), want, "{f}");
    }
}

#[test]
fn local_names_shadow_defined_names() {
    let mut e = engine();
    e.define_name("x", "=100", None).expect("name");
    assert_eq!(e.evaluate("=LET(x,1,x)+x").ok(), Some(num(101.0)));
}

#[test]
fn lambdas_close_over_their_scope_and_check_arity() {
    for (f, want) in [
        ("=LAMBDA(x,x*2)(21)", num(42.0)),
        ("=LAMBDA(a,b,a+b)(1,2)", num(3.0)),
        ("=LAMBDA(42)()", num(42.0)),
        ("=LAMBDA(x,LAMBDA(y,x+y))(1)(2)", num(3.0)),
        ("=LAMBDA(x,x)", error(ErrorValue::Calc)),
        ("=LAMBDA(x,x)(1,2)", error(ErrorValue::Value)),
        ("=LAMBDA(a,b,ISOMITTED(b))(1,)", Value::Bool(true)),
        ("=LAMBDA(a,b,ISOMITTED(b))(1)", Value::Bool(true)),
        ("=LAMBDA(a,b,ISOMITTED(b))(1,2)", Value::Bool(false)),
        ("=LAMBDA(x,x,1)(1)", error(ErrorValue::Value)),
        ("=LET(f,LAMBDA(n,n*n),f(4)+f(3))", num(25.0)),
        ("=LET(add,LAMBDA(a,LAMBDA(b,a+b)),add(10)(5))", num(15.0)),
        ("=5(1)", error(ErrorValue::Value)),
    ] {
        assert_eq!(calc(f, &[]), want, "{f}");
    }
}

#[test]
fn named_lambdas_are_callable_like_functions_and_may_recurse() {
    let mut e = sheet(&[("A1", 5.into())]);
    e.define_name("SQUARE", "=LAMBDA(x,x*x)", None)
        .expect("name");
    e.define_name("FACT", "=LAMBDA(n,IF(n<=1,1,n*FACT(n-1)))", None)
        .expect("name");
    e.set_formula("Sheet1", 0, 1, "=SQUARE(A1)").expect("f");
    e.set_formula("Sheet1", 0, 2, "=FACT(10)").expect("f");
    e.recalc();
    assert_eq!(get(&e, "B1"), num(25.0));
    assert_eq!(get(&e, "C1"), num(3_628_800.0));
}

#[test]
fn runaway_recursion_is_num_instead_of_a_crash() {
    let mut e = engine();
    e.define_name("LOOP", "=LAMBDA(n,LOOP(n+1))", None)
        .expect("name");
    assert_eq!(e.evaluate("=LOOP(1)").ok(), Some(error(ErrorValue::Num)));
}

#[test]
fn redefining_a_named_lambda_recalculates_its_callers() {
    let mut e = engine();
    e.define_name("F", "=LAMBDA(x,x+1)", None).expect("name");
    e.set_formula("Sheet1", 0, 0, "=F(1)").expect("f");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(2.0));
    e.define_name("F", "=LAMBDA(x,x*10)", None).expect("name");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(10.0));
}

#[test]
fn a_lambda_defined_in_a_cell_keeps_its_relative_references() {
    // The lambda body's A1 is relative to B2, where the formula lives: it means A1.
    let mut e = sheet(&[("A1", 7.into())]);
    e.set_formula("Sheet1", 1, 1, "=LET(f,LAMBDA(x,A1+x),f(1))")
        .expect("f");
    e.recalc();
    assert_eq!(get(&e, "B2"), num(8.0));
}
