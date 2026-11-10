// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which formulas are saved as dynamic arrays (`<f t="array">` with the `cm` cell metadata).
//!
//! Excel saves a formula as a dynamic array only when it would compute differently as a
//! pre-dynamic-array formula: Excel 2019 and earlier show such a formula in braces, as an array
//! formula, so marking plain formulas would change how they look there. A formula keeps its
//! meaning as a plain `<f>` unless a range (or a name standing for one) reaches a place that takes
//! one value, where the old semantics intersect it with the formula's row or column and the new
//! ones lift over it: an operator's operand (`A1:A3*2`), a function argument that takes one value
//! (`LEN(A1:A3)`), or the formula's result itself (`=A1:A3`, which spills). An array constant or
//! an array function at the result, a spill reference (`A1#`) and the dynamic-array functions
//! (`FILTER`, `SORT`, `SEQUENCE`…) are dynamic too. Ranges that functions take whole, as `SUM`,
//! `COUNTIF`, `VLOOKUP`, `INDEX` and `MATCH` do, keep the formula plain (`=SUM(B49:B52)`).
//!
//! The analysis follows the engine's legacy (plain-formula) semantics, so a file saved here and
//! opened again here computes the same values: argument positions are the library's `lift` masks.

use kiln_formula::parser::{
    BinaryOp, Node, NodeKind, ParseOptions, PostfixOp, RefAddress, UnaryOp, parse,
};
use kiln_formula::registry::FunctionRegistry;

/// What an expression gives, as far as implicit intersection is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Shape {
    /// One value (a number, a single cell, a scalar function's result).
    Scalar,
    /// An array of values (an array constant, a lifted call).
    Array,
    /// A reference that may cover more than one cell (a range, a name for one).
    Range,
}

/// Functions that return arrays whatever their arguments: a formula using one is a dynamic array
/// (Excel writes these with the `cm` metadata).
const ARRAY_FUNCTIONS: &[&str] = &[
    "FILTER",
    "SORT",
    "SORTBY",
    "UNIQUE",
    "SEQUENCE",
    "RANDARRAY",
    "TRANSPOSE",
    "MMULT",
    "MINVERSE",
    "MUNIT",
    "FREQUENCY",
    "LINEST",
    "LOGEST",
    "TREND",
    "GROWTH",
    "TEXTSPLIT",
    "VSTACK",
    "HSTACK",
    "TOCOL",
    "TOROW",
    "WRAPROWS",
    "WRAPCOLS",
    "TAKE",
    "DROP",
    "EXPAND",
    "CHOOSEROWS",
    "CHOOSECOLS",
    "GROUPBY",
    "PIVOTBY",
    "TRIMRANGE",
    "MAP",
    "SCAN",
    "BYROW",
    "BYCOL",
    "MAKEARRAY",
    "REGEXEXTRACT",
    "ANCHORARRAY",
];

/// Functions that return references of any size (`OFFSET(A1,0,0,3)`).
const RANGE_FUNCTIONS: &[&str] = &["OFFSET", "INDIRECT"];

/// Functions whose value arguments pass through to the result (`CHOOSE(2,A1:A3,B1:B3)`).
const PASS_THROUGH: &[&str] = &["CHOOSE", "IFS", "SWITCH", "IFERROR", "IFNA", "IF"];

/// The deepest a defined name is followed into the names it uses.
const NAME_DEPTH: u8 = 8;

/// Looks up a defined name's formula (without `=`).
pub(crate) type NameLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

struct Analysis<'a> {
    registry: &'a dyn FunctionRegistry,
    names: NameLookup<'a>,
    /// LET variables in scope, innermost last.
    scope: Vec<(String, Shape)>,
    /// The formula computes differently as a plain formula.
    dynamic: bool,
    depth: u8,
}

impl Analysis<'_> {
    /// A value used where one value is taken: a range there is intersected by plain formulas.
    fn scalar_use(&mut self, s: Shape) {
        if s == Shape::Range {
            self.dynamic = true;
        }
    }

    fn name_shape(&mut self, name: &str) -> Shape {
        if let Some((_, s)) = self
            .scope
            .iter()
            .rev()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return *s;
        }
        if self.depth < NAME_DEPTH
            && let Some(f) = (self.names)(name)
            && let Ok(node) = parse(&f, &ParseOptions::default())
        {
            self.depth += 1;
            let saved = std::mem::take(&mut self.scope);
            let s = self.shape(&node);
            self.scope = saved;
            self.depth -= 1;
            return s;
        }
        // Not a known name (a LAMBDA parameter, a name of another workbook): it may be a range.
        Shape::Range
    }

    fn shape(&mut self, n: &Node) -> Shape {
        match &n.kind {
            NodeKind::Number(_)
            | NodeKind::Str(_)
            | NodeKind::Bool(_)
            | NodeKind::Error { .. }
            | NodeKind::Empty => Shape::Scalar,
            NodeKind::Ref { address, .. } => match address {
                RefAddress::Cell { .. } => Shape::Scalar,
                RefAddress::Area { r0, c0, r1, c1, .. } if r0 == r1 && c0 == c1 => Shape::Scalar,
                _ => Shape::Range,
            },
            NodeKind::Name { name, .. } => self.name_shape(name),
            NodeKind::Structured(s) => {
                if s.this_row {
                    Shape::Scalar
                } else {
                    Shape::Range
                }
            }
            NodeKind::Array(rows) => {
                for row in rows {
                    for x in row {
                        self.shape(x);
                    }
                }
                Shape::Array
            }
            NodeKind::Paren(x) => self.shape(x),
            NodeKind::Unary { op, operand } => {
                let s = self.shape(operand);
                match op {
                    UnaryOp::ImplicitIntersection => Shape::Scalar,
                    UnaryOp::Neg | UnaryOp::Plus => {
                        self.scalar_use(s);
                        if s == Shape::Scalar {
                            Shape::Scalar
                        } else {
                            Shape::Array
                        }
                    }
                }
            }
            NodeKind::Postfix { op, operand } => {
                let s = self.shape(operand);
                match op {
                    PostfixOp::Spill => {
                        self.dynamic = true;
                        Shape::Range
                    }
                    PostfixOp::Percent => {
                        self.scalar_use(s);
                        if s == Shape::Scalar {
                            Shape::Scalar
                        } else {
                            Shape::Array
                        }
                    }
                }
            }
            NodeKind::Binary { op, left, right } => {
                let l = self.shape(left);
                let r = self.shape(right);
                if matches!(op, BinaryOp::Range | BinaryOp::Intersect | BinaryOp::Union) {
                    return Shape::Range;
                }
                self.scalar_use(l);
                self.scalar_use(r);
                if l == Shape::Scalar && r == Shape::Scalar {
                    Shape::Scalar
                } else {
                    Shape::Array
                }
            }
            NodeKind::Call { name, args } => self.call(name, args),
            NodeKind::Invoke { callee, args } => {
                // A lambda's body is not known here: keep the dynamic form.
                self.shape(callee);
                for a in args {
                    self.shape(a);
                }
                self.dynamic = true;
                Shape::Array
            }
        }
    }

    fn call(&mut self, raw: &str, args: &[Node]) -> Shape {
        let upper = raw.to_ascii_uppercase();
        let name = upper
            .strip_prefix("_XLFN._XLWS.")
            .or_else(|| upper.strip_prefix("_XLFN."))
            .or_else(|| upper.strip_prefix("_XLWS."))
            .unwrap_or(&upper);
        if name == "LET" {
            return self.let_(args);
        }
        if ARRAY_FUNCTIONS.contains(&name) {
            self.dynamic = true;
            for a in args {
                self.shape(a);
            }
            return Shape::Array;
        }
        let spec = self
            .registry
            .lookup(name)
            .and_then(|id| self.registry.spec(id));
        let pass = PASS_THROUGH.contains(&name);
        let mut out = Shape::Scalar;
        for (i, a) in args.iter().enumerate() {
            let s = self.shape(a);
            let lifted = spec.is_some_and(|sp| sp.lift.contains(i));
            if lifted {
                self.scalar_use(s);
                if s != Shape::Scalar {
                    out = out.max(Shape::Array);
                }
            }
            // A value argument of CHOOSE, IF… is the result (the first chooses).
            if pass && i > 0 {
                out = out.max(s);
            }
        }
        if spec.is_none() {
            // A function this library does not know (an add-in, a future function): its
            // arguments' meaning is unknown, so keep the dynamic form.
            self.dynamic = true;
        }
        if RANGE_FUNCTIONS.contains(&name) {
            return Shape::Range;
        }
        out
    }

    /// `LET(name1, value1, …, calculation)`: names bound to their values' shapes.
    fn let_(&mut self, args: &[Node]) -> Shape {
        let depth = self.scope.len();
        let mut i = 0;
        while i + 1 < args.len() {
            let v = self.shape(&args[i + 1]);
            if let NodeKind::Name { name, .. } = &args[i].kind {
                self.scope.push((name.clone(), v));
            }
            i += 2;
        }
        let out = args.last().map_or(Shape::Scalar, |b| self.shape(b));
        self.scope.truncate(depth);
        out
    }
}

/// Whether a formula (model text, `=` optional) must be saved as a dynamic array to keep its
/// meaning. `names` gives the formulas of the workbook's defined names.
pub(crate) fn needs_array_semantics_with(text: &str, names: NameLookup<'_>) -> bool {
    let Ok(node) = parse(text, &ParseOptions::default()) else {
        // Text the parser does not take (kept as read): the conservative scan.
        return fallback(text);
    };
    let library = kiln_formula::functions::library();
    let mut a = Analysis {
        registry: library.as_ref(),
        names,
        scope: Vec::new(),
        dynamic: false,
        depth: 0,
    };
    let top = a.shape(&node);
    // The result itself: a range or an array spills.
    a.dynamic || top != Shape::Scalar
}

/// [`needs_array_semantics_with`] without defined names (any name may stand for a range).
#[cfg(test)]
pub(crate) fn needs_array_semantics(text: &str) -> bool {
    needs_array_semantics_with(text, &|_| None)
}

/// A formula the parser rejects: dynamic when it holds a range, an array, a structured reference
/// or a name.
fn fallback(text: &str) -> bool {
    let b = text.as_bytes();
    let mut in_str = false;
    for &c in b {
        if c == b'"' {
            in_str = !in_str;
        } else if !in_str && matches!(c, b':' | b'{' | b'[') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_aggregates_over_ranges_stay_plain() {
        for f in [
            "A1+B2*2",
            "SUM(A1,B2)",
            "IF(TRUE,1,2)",
            "Data!A1*2",
            "'My Sheet'!A1*2",
            "\"a:b\"&A1",
            "SUM(B49:B52)",
            "AVERAGE(A1:A10)",
            "SUM(A1:A3)*2",
            "COUNTIF(A1:A10,\">5\")",
            "SUMIF(A1:A10,\">5\",B1:B10)",
            "COUNTIFS(A1:A10,\"x\",B1:B10,\">1\")",
            "INDEX(B1:B10,MATCH(D1,A1:A10,0))",
            "VLOOKUP(D1,A1:C10,3,FALSE)",
            "HLOOKUP(D1,A1:J3,2,FALSE)",
            "MAX(A:A)-MIN(A:A)",
            "SUM(Sheet2!A1:B5)",
            "ROUND(AVERAGE(A1:A4),2)",
            "IFERROR(VLOOKUP(D1,A1:C10,3,0),\"\")",
            "SUM(Total)",
            "SUM(Table1[Col])",
            "Table1[@Col]*2",
            "@A1:A10",
            "SUM({1,2,3})",
            "LET(x,A1:A3,SUM(x))",
            "ROWS(A1:A10)",
            "SUMPRODUCT(A1:A3,B1:B3)",
            "_xlfn.XLOOKUP(D1,A1:A10,B1:B10)",
            "A1:A1*2",
        ] {
            assert!(!needs_array_semantics(f), "{f} should be a plain formula");
        }
    }

    #[test]
    fn formulas_that_return_arrays_or_intersect_are_dynamic() {
        for f in [
            "A1:A3",
            "SUM(A1:A3*B1:B3)",
            "A1:A3*2",
            "{1,2}*2",
            "{1,2}",
            "Table1[Col]",
            "Total*2",
            "LEN(A1:A3)",
            "SUM(LEN(A1:A3))",
            "IF(A1:A3>1,1,0)",
            "IF(C1,A1:A3,0)",
            "A1#",
            "SORT(A1:A10)",
            "_xlfn._xlws.FILTER(A1:A10,B1:B10>1)",
            "_xlfn.UNIQUE(A1:A10)",
            "SEQUENCE(5)",
            "TRANSPOSE(A1:C1)",
            "CHOOSE(1,A1:A3,B1:B3)",
            "OFFSET(A1,0,0,3,1)*2",
            "LET(x,A1:A3,x*2)",
            "-A1:A3",
            "A1:A3%",
        ] {
            assert!(needs_array_semantics(f), "{f} should be a dynamic array");
        }
    }

    #[test]
    fn defined_names_are_followed() {
        let names = |n: &str| match n.to_ascii_lowercase().as_str() {
            "rate" => Some("Sheet1!$B$1".to_owned()),
            "half" => Some("0.5".to_owned()),
            "data" => Some("Sheet1!$A$1:$A$10".to_owned()),
            "twice" => Some("data".to_owned()),
            _ => None,
        };
        assert!(!needs_array_semantics_with("A1*Rate", &names));
        assert!(!needs_array_semantics_with("A1*Half+Rate", &names));
        assert!(!needs_array_semantics_with("SUM(Data)", &names));
        assert!(needs_array_semantics_with("Data*2", &names));
        assert!(needs_array_semantics_with("Twice*2", &names));
        assert!(needs_array_semantics_with("Unknown*2", &names));
    }

    #[test]
    fn unparsed_text_falls_back_to_the_scan() {
        assert!(fallback("SUM(A1:A3"));
        assert!(!fallback("A1+"));
    }
}
