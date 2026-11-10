// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text documents (`.odt`, `.ott`) ⇄ [`kiln_doc::Document`].

pub mod lists;
pub mod read;
pub mod styles;
pub mod write;

pub use read::{OdtRead, read_odt};
pub use write::{odt_issues, write_odt};
