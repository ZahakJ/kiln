// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine's core functions (`SUM`, `IF`, `AND`, `OR`, `NOT`, `TRUE`, `FALSE`) and helpers
//! shared by function implementations.

use super::eval::CallContext;
use super::types::{Arg, EvalValue};
use crate::registry::{
    ArgDoc, ArgMask, DocsHandle, FunctionCategory, FunctionDocs, FunctionSpec, MAX_ARGS,
};
use crate::values::{ErrorValue, Value, parse_number_text, to_bool, to_number};

/// Evaluates an argument (forcing a lazy one); a missing argument is [`EvalValue::Missing`].
pub fn force(arg: Option<&mut Arg<'_>>, ctx: &mut dyn CallContext) -> EvalValue {
    match arg {
        Some(a) => ctx.force(a),
        None => EvalValue::Missing,
    }
}

/// A scalar from an argument (implicit intersection for ranges); omitted → blank.
pub fn scalar_arg(arg: Option<&mut Arg<'_>>, ctx: &mut dyn CallContext) -> Value {
    let v = force(arg, ctx);
    ctx.to_scalar(v)
}

/// A number from an argument; `default` when the argument is absent.
pub fn number_arg(
    arg: Option<&mut Arg<'_>>,
    ctx: &mut dyn CallContext,
    default: Option<f64>,
) -> Result<f64, ErrorValue> {
    if arg.is_none()
        && let Some(d) = default
    {
        return Ok(d);
    }
    let v = scalar_arg(arg, ctx);
    to_number(&v, &ctx.coercion())
}

/// Collects numbers the way `SUM`/`AVERAGE`/`MIN`/`MAX` do: inside ranges and arrays only numbers
/// count (text, logicals and blanks are skipped); typed-in arguments also accept logicals and
/// numeric text. The first error is returned.
pub fn collect_numbers(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
) -> Result<Vec<f64>, ErrorValue> {
    let mut out = Vec::new();
    let opts = ctx.coercion();
    for a in args.iter_mut() {
        let v = ctx.force(a);
        let mut error: Option<ErrorValue> = None;
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            match x {
                Value::Number(n) => out.push(*n),
                Value::Error(e) => {
                    error = Some(*e);
                    return false;
                }
                _ if from_ref => {}
                Value::Bool(b) => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Str(s) => match parse_number_text(s, &opts) {
                    Some(n) => out.push(n),
                    None => {
                        error = Some(ErrorValue::Value);
                        return false;
                    }
                },
                Value::Blank => out.push(0.0),
                _ => {}
            }
            true
        });
        if let Some(e) = error {
            return Err(e);
        }
    }
    Ok(out)
}

fn val(v: Value) -> EvalValue {
    EvalValue::Value(v)
}

fn sum(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    match collect_numbers(args, ctx) {
        Err(e) => e.into(),
        Ok(nums) => val(Value::number(nums.iter().sum())),
    }
}

/// Value of an `IF` branch: an empty argument is 0.
fn branch(arg: Option<&mut Arg<'_>>, ctx: &mut dyn CallContext) -> EvalValue {
    let Some(a) = arg else {
        return 0.0.into();
    };
    if a.is_omitted() {
        return 0.0.into();
    }
    match ctx.force(a) {
        EvalValue::Missing | EvalValue::Value(Value::Blank) => 0.0.into(),
        v => v,
    }
}

fn if_(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let n = args.len();
    let test = scalar_arg(args.first_mut(), ctx);
    match to_bool(&test) {
        Err(e) => e.into(),
        Ok(true) => branch(args.get_mut(1), ctx),
        Ok(false) if n < 3 => false.into(),
        Ok(false) => branch(args.get_mut(2), ctx),
    }
}

fn logical_fold(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, is_and: bool) -> EvalValue {
    let mut seen = false;
    let mut acc = is_and;
    for a in args.iter_mut() {
        let v = ctx.force(a);
        let mut error: Option<ErrorValue> = None;
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            let b = match x {
                Value::Bool(b) => Some(*b),
                Value::Number(n) => Some(*n != 0.0),
                Value::Error(e) => {
                    error = Some(*e);
                    return false;
                }
                Value::Str(_) if !from_ref => match to_bool(x) {
                    Ok(b) => Some(b),
                    Err(e) => {
                        error = Some(e);
                        return false;
                    }
                },
                _ => None,
            };
            if let Some(b) = b {
                seen = true;
                acc = if is_and { acc && b } else { acc || b };
            }
            true
        });
        if let Some(e) = error {
            return e.into();
        }
    }
    if seen {
        acc.into()
    } else {
        ErrorValue::Value.into()
    }
}

fn and(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    logical_fold(args, ctx, true)
}

fn or(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    logical_fold(args, ctx, false)
}

fn not(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let v = scalar_arg(args.first_mut(), ctx);
    match to_bool(&v) {
        Ok(b) => (!b).into(),
        Err(e) => e.into(),
    }
}

fn true_(_: &mut [Arg<'_>], _: &mut dyn CallContext) -> EvalValue {
    true.into()
}

fn false_(_: &mut [Arg<'_>], _: &mut dyn CallContext) -> EvalValue {
    false.into()
}

const fn arg(name: &'static str, description: &'static str) -> ArgDoc {
    ArgDoc {
        name,
        description,
        optional: false,
        repeating: false,
    }
}

const fn more(name: &'static str, description: &'static str) -> ArgDoc {
    ArgDoc {
        name,
        description,
        optional: true,
        repeating: true,
    }
}

static SUM_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::MathTrig,
    summary: "Adds numbers.",
    syntax: "SUM(number1, [number2], ...)",
    args: &[
        arg("number1", "A number, cell reference or range to add."),
        more("number2", "Further numbers or ranges."),
    ],
    examples: &[("=SUM(1,2,3)", "6")],
};

static IF_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Returns one value if a condition is TRUE and another if it is FALSE.",
    syntax: "IF(logical_test, value_if_true, [value_if_false])",
    args: &[
        arg("logical_test", "The condition to check."),
        arg("value_if_true", "Result when the condition is TRUE."),
        ArgDoc {
            name: "value_if_false",
            description: "Result when the condition is FALSE (FALSE if omitted).",
            optional: true,
            repeating: false,
        },
    ],
    examples: &[("=IF(2>1,\"yes\",\"no\")", "\"yes\"")],
};

static AND_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Returns TRUE if all of its arguments are TRUE.",
    syntax: "AND(logical1, [logical2], ...)",
    args: &[
        arg("logical1", "A condition, logical value or range."),
        more("logical2", "Further conditions."),
    ],
    examples: &[("=AND(TRUE,1>2)", "FALSE")],
};

static OR_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Returns TRUE if any argument is TRUE.",
    syntax: "OR(logical1, [logical2], ...)",
    args: &[
        arg("logical1", "A condition, logical value or range."),
        more("logical2", "Further conditions."),
    ],
    examples: &[("=OR(FALSE,1<2)", "TRUE")],
};

static NOT_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Reverses a logical value.",
    syntax: "NOT(logical)",
    args: &[arg(
        "logical",
        "A value or condition that is TRUE or FALSE.",
    )],
    examples: &[("=NOT(TRUE)", "FALSE")],
};

static TRUE_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Returns the logical value TRUE.",
    syntax: "TRUE()",
    args: &[],
    examples: &[("=TRUE()", "TRUE")],
};

static FALSE_DOCS: FunctionDocs = FunctionDocs {
    category: FunctionCategory::Logical,
    summary: "Returns the logical value FALSE.",
    syntax: "FALSE()",
    args: &[],
    examples: &[("=FALSE()", "FALSE")],
};

pub(crate) fn core_specs() -> Vec<FunctionSpec> {
    vec![
        FunctionSpec {
            docs: DocsHandle::Inline(&SUM_DOCS),
            ..FunctionSpec::new("SUM", 1, MAX_ARGS, sum)
        },
        FunctionSpec {
            lazy: ArgMask::of(&[1, 2]),
            lift: ArgMask::of(&[0, 1, 2]),
            docs: DocsHandle::Inline(&IF_DOCS),
            ..FunctionSpec::new("IF", 2, 3, if_)
        },
        FunctionSpec {
            docs: DocsHandle::Inline(&AND_DOCS),
            ..FunctionSpec::new("AND", 1, MAX_ARGS, and)
        },
        FunctionSpec {
            docs: DocsHandle::Inline(&OR_DOCS),
            ..FunctionSpec::new("OR", 1, MAX_ARGS, or)
        },
        FunctionSpec {
            lift: ArgMask::of(&[0]),
            docs: DocsHandle::Inline(&NOT_DOCS),
            ..FunctionSpec::new("NOT", 1, 1, not)
        },
        FunctionSpec {
            docs: DocsHandle::Inline(&TRUE_DOCS),
            ..FunctionSpec::new("TRUE", 0, 0, true_)
        },
        FunctionSpec {
            docs: DocsHandle::Inline(&FALSE_DOCS),
            ..FunctionSpec::new("FALSE", 0, 0, false_)
        },
    ]
}
