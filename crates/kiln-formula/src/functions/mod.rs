// SPDX-License-Identifier: AGPL-3.0-or-later
//! The worksheet function library: 497 Excel functions in twelve families, each with
//! documentation (signature, arguments, examples) used by autocompletion, the Insert Function
//! dialog and the Formula Lab.
//!
//! [`library`] is the shared registry with every function; pass it to the engine through
//! [`EngineOptions::registry`](crate::EngineOptions::registry), or use
//! [`engine_options`]. [`docs`] lists the documentation, [`doc`] finds one entry.
//!
//! The library is a port of Kiln's v1.0 TypeScript library: names, arities, volatility and which
//! arguments are lazy or lifted over arrays are identical (`tests/conformance_registry.rs` checks
//! them against a snapshot of the v1.0 registry), plus 13 functions v1.0 lacked; the 5,400-row
//! conformance suite in `conformance/` is the oracle (`tests/conformance.rs`). Known differences from Excel are listed
//! in `docs/formula-deviations.md`.
//!
//! | Module | Family |
//! |---|---|
//! | [`math`] | Math & Trig, including `SUBTOTAL`, `AGGREGATE`, `SUMPRODUCT` and matrices |
//! | [`conditional`] | `COUNTIF(S)`, `SUMIF(S)`, `AVERAGEIF(S)`, `MAXIFS`, `MINIFS`, `COUNTBLANK` |
//! | [`statistical`] | Descriptive statistics, regression and tests |
//! | [`distributions`] | Probability distributions and their compatibility names |
//! | [`logical`] | `IF`, `IFS`, `SWITCH`, `LET`, `LAMBDA`, `MAP`, `REDUCE`, `SCAN`, `MAKEARRAY` … |
//! | [`information`] | `IS*`, `CELL`, `INFO`, `TYPE`, `SHEET` … |
//! | [`text`] | `TEXT`, `TEXTSPLIT`, `TEXTBEFORE`, `REGEXEXTRACT` … |
//! | [`datetime`] | `DATE`, `EDATE`, `NETWORKDAYS.INTL`, `WORKDAY.INTL`, `DATEDIF`, `YEARFRAC` … |
//! | [`lookup`] | `XLOOKUP`, `INDEX`, `FILTER`, `SORT`, `UNIQUE`, `TAKE`, `HSTACK`, `WRAPROWS` … |
//! | [`financial`] | Time value of money, cash flows, depreciation, securities |
//! | [`engineering`] | Base conversions, bitwise operations, complex numbers, `CONVERT`, Bessel |
//! | [`database`] | `DSUM`, `DGET`, `DAVERAGE` … and the Web function `ENCODEURL` |

use std::sync::{Arc, OnceLock};

use crate::engine::EngineOptions;
use crate::registry::{FunctionRegistry, FunctionSpec, FunctionTable};

#[macro_use]
mod support;

mod agg;
mod bessel;
mod criteria;
mod docs;
mod grid;
mod matching;
mod special;
mod textutil;
mod thai;
mod units;
mod widths;

pub mod conditional;
pub mod database;
pub mod datetime;
pub mod distributions;
pub mod engineering;
pub mod financial;
pub mod information;
pub mod logical;
pub mod lookup;
pub mod math;
pub mod statistical;
pub mod text;

pub use criteria::{Criterion, parse_criterion};
pub use docs::{ArgDocEntry, DocFamily, ExampleEntry, FunctionDoc, doc, docs};
pub use matching::Wildcard;

/// Every function of the library, family by family (the order of the Insert Function dialog is
/// given by the documentation, not by this list).
pub fn all_specs() -> Vec<FunctionSpec> {
    let mut v = Vec::with_capacity(500);
    v.extend(math::specs());
    v.extend(conditional::specs());
    v.extend(statistical::specs());
    v.extend(distributions::specs());
    v.extend(logical::specs());
    v.extend(information::specs());
    v.extend(text::specs());
    v.extend(datetime::specs());
    v.extend(lookup::specs());
    v.extend(financial::specs());
    v.extend(engineering::specs());
    v.extend(database::specs());
    v
}

/// A new registry with every library function.
pub fn new_table() -> FunctionTable {
    let mut t = FunctionTable::new();
    t.register_all(all_specs());
    t
}

/// The shared registry with every library function (built once).
pub fn library() -> Arc<dyn FunctionRegistry> {
    static LIBRARY: OnceLock<Arc<FunctionTable>> = OnceLock::new();
    let table = LIBRARY.get_or_init(|| Arc::new(new_table()));
    let shared: Arc<dyn FunctionRegistry> = table.clone();
    shared
}

/// Engine options using the full library (and defaults otherwise).
pub fn engine_options() -> EngineOptions {
    EngineOptions {
        registry: Some(library()),
        ..EngineOptions::default()
    }
}

/// Upper-case names of every library function, sorted.
pub fn function_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = all_specs().iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_every_function_once() {
        let specs = all_specs();
        let names = function_names();
        assert_eq!(specs.len(), names.len(), "a function is registered twice");
        assert!(names.len() >= 484, "{} functions", names.len());
        assert_eq!(library().specs().len(), names.len());
    }
}
