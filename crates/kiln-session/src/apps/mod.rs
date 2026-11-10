// SPDX-License-Identifier: AGPL-3.0-or-later
//! The registry of document apps. Each app adds one line to [`builtin`] (an
//! append-only shared file).

pub mod deck;
pub mod grid;
pub mod scratch;
pub mod write;

use crate::model::AppFactory;

/// The apps every session starts with.
#[must_use]
pub fn builtin() -> Vec<Box<dyn AppFactory>> {
    vec![
        Box::new(scratch::ScratchApp), // plain paragraphs, proves the bridge loop
        Box::new(write::WriteApp),     // .docx/.dotx/.docm through kiln-io-docx
        Box::new(grid::GridApp), // .xlsx/.xlsm/.xltx/.csv through kiln-io-xlsx and kiln-io-csv
        Box::new(deck::DeckApp), // .pptx/.potx/.pptm through kiln-io-pptx
    ]
}
