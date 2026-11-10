// SPDX-License-Identifier: AGPL-3.0-or-later
//! Spreadsheets (`.ods`, `.ots`) ⇄ [`kiln_sheet::Workbook`].

pub mod headers;
pub mod read;
pub mod rules;
pub mod style;
pub mod write;

pub use read::{OdsRead, read_ods};
pub use write::{ods_issues, write_ods};
