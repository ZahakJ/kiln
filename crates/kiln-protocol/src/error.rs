// SPDX-License-Identifier: AGPL-3.0-or-later
//! Errors on the wire.

use kiln_common::{ErrorKind, KilnError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An error as the UI receives it: a stable kind to branch on and an English message for logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorInfo {
    /// Machine-readable kind.
    pub kind: ErrorKind,
    /// English description (the UI localises from `kind`).
    pub message: String,
}

impl From<KilnError> for ErrorInfo {
    fn from(e: KilnError) -> Self {
        Self {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}

impl From<&KilnError> for ErrorInfo {
    fn from(e: &KilnError) -> Self {
        Self {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}

impl std::fmt::Display for ErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ErrorInfo {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_kind_and_message() {
        let info = ErrorInfo::from(KilnError::not_found("doc#9"));
        assert_eq!(info.kind, ErrorKind::NotFound);
        assert_eq!(info.to_string(), "doc#9 not found");
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"kind":"NotFound","message":"doc#9 not found"}"#
        );
        assert_eq!(ErrorInfo::from(&KilnError::Closed).kind, ErrorKind::Closed);
    }
}
