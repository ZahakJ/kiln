// SPDX-License-Identifier: AGPL-3.0-or-later
//! The registry of document apps. Each app work package adds one line to [`builtin`] (an
//! append-only shared file, PLAN.md §4.7).

pub mod scratch;

use crate::model::AppFactory;

/// The apps every session starts with.
#[must_use]
pub fn builtin() -> Vec<Box<dyn AppFactory>> {
    vec![
        Box::new(scratch::ScratchApp), // WP-52: plain paragraphs, proves the bridge loop
    ]
}
