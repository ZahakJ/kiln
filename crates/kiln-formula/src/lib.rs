// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-formula
//!
//! Kiln Grid's spreadsheet formula engine.
//!
//! - [`parser`]: tokenizer, Pratt parser and AST with byte spans, and a printer that reproduces
//!   canonical text (A1 and R1C1).
//! - [`refs`]: A1 addresses and sheet qualifiers, and reference rewriting for inserted and deleted
//!   rows and columns, copy, move, sheet renames, A1↔R1C1 and the F4 toggle.
//! - [`values`]: the value model (numbers, text, logicals, errors, blanks, arrays, lambdas),
//!   coercion and comparison rules, Excel's 15-significant-digit behaviour and date serials.
//! - [`engine`]: compiled formulas, the dependency graph, incremental recalculation by topological
//!   levels (independent cells evaluated in parallel with deterministic results), cycles,
//!   dynamic arrays and spilling, defined names, tables, structural edits.
//! - [`registry`]: the [`FunctionRegistry`](registry::FunctionRegistry) the function library plugs
//!   into; an engine without options knows the core set (`SUM`, `IF`, `AND`, `OR`, `NOT`, `TRUE`,
//!   `FALSE`).
//! - [`functions`]: the function library — 484 Excel worksheet functions with their documentation
//!   ([`functions::engine_options`] gives an engine all of them, [`functions::doc`] the docs).
//! - [`numfmt`]: Excel number format codes (sections, conditions, colours, dates and times,
//!   fractions, scientific, `General`).
//!
//! Without the default `parallel` feature the crate builds for `wasm32-unknown-unknown` and
//! evaluates every level sequentially, with identical results.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod engine;
pub mod functions;
pub mod numfmt;
pub mod parser;
pub mod refs;
pub mod registry;
pub mod values;

pub use engine::{Engine, EngineOptions, MemoryHost, SheetId};
pub use parser::{ParseError, ParseOptions, parse, print};
pub use values::{ErrorValue, Value};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
