// SPDX-License-Identifier: AGPL-3.0-or-later
//! The model's error type. Commands validate before changing anything, so an error always means
//! the workbook is unchanged.

use thiserror::Error;

/// Why a model operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    /// A row or column outside the 1,048,576 × 16,384 grid.
    #[error("cell (row {row}, column {col}) is outside the sheet")]
    OutOfRange {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// No sheet with that id or name.
    #[error("no sheet {0}")]
    UnknownSheet(String),
    /// A formula that does not parse.
    #[error("invalid formula \"{text}\": {message}")]
    InvalidFormula {
        /// The formula as given.
        text: String,
        /// The parser's message.
        message: String,
    },
    /// A protected sheet or workbook refuses the change.
    #[error("{0}")]
    Protected(String),
    /// Anything else Excel would refuse, with the message to show.
    #[error("{0}")]
    Invalid(String),
}

impl ModelError {
    /// An [`ModelError::Invalid`] with a message.
    pub fn invalid(msg: impl Into<String>) -> ModelError {
        ModelError::Invalid(msg.into())
    }
}

/// Result alias for model operations.
pub type Result<T, E = ModelError> = std::result::Result<T, E>;
