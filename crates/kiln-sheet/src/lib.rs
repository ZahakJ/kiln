// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-sheet
//!
//! The Kiln Grid workbook model (PLAN.md §3.4).
//!
//! - [`Workbook`] / [`Sheet`] over a [`SparseStore`] per sheet: row chunks with columnar arrays,
//!   a lazily built per-row index for O(1) random access, iteration by used range, the full
//!   1,048,576 × 16,384 grid.
//! - [`StyleTable`]: fonts, fills, borders, number formats, alignment and protection deduplicated
//!   like `cellXfs`; differential formats for conditional formatting; named cell styles.
//! - Metadata ([`meta`]): merges, defined names, tables, conditional formatting, data validation,
//!   hyperlinks, notes and threaded comments, row/column properties, views, print settings,
//!   protection.
//! - [`Command`]s applied with exact inverses and the [`CommandLog`] (undo/redo, grouping).
//! - Formula binding ([`calc`]): formulas interned in `kiln-formula`'s engine, incremental
//!   parallel recalculation on edit, automatic and manual modes, names and structured references
//!   resolved through the engine, iterative calculation.
//! - Evaluation of conditional formats ([`cf`]) and data validation ([`validation`]); the
//!   clipboard model ([`clipboard`]); AutoFill series ([`fill`]); sort ([`sort`]) and filter
//!   ([`filter`]); remove duplicates and text to columns ([`data`]).
//! - Table styles ([`table_style`]): header, totals, first/last column and banded rows and
//!   columns of custom and built-in table styles.
//! - Serialisation to postcard ([`serial`]).
//!
//! Without the default features the crate builds for `wasm32-unknown-unknown`: recalculation is
//! sequential (same results) and sorting uses Excel's own text order instead of ICU collation.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod address;
mod apply;
pub mod axis;
pub mod calc;
pub mod cf;
pub mod clipboard;
pub mod color;
pub mod command_log;
pub mod commands;
pub mod data;
pub mod error;
pub mod fill;
pub mod filter;
pub mod format;
pub mod meta;
pub mod num;
pub mod serial;
pub mod sheet;
pub mod shift;
pub mod sort;
pub mod store;
pub mod structured;
pub mod styles;
pub mod table_style;
pub mod validation;
pub mod value;
pub mod workbook;

pub use address::{CellPos, MAX_COLS, MAX_ROWS, Range};
pub use color::{Color, Theme};
pub use command_log::CommandLog;
pub use commands::Command;
pub use error::{ModelError, Result};
pub use kiln_formula::SheetId;
pub use sheet::{Sheet, SheetLayout, SheetMeta};
pub use store::{CellData, CellKind, SparseStore};
pub use styles::StyleTable;
pub use value::CellValue;
pub use workbook::{CellRecord, Changes, Workbook, WorkbookOptions};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
