// SPDX-License-Identifier: AGPL-3.0-or-later
//! The error type shared by the core crates.

/// A stable, machine-readable classification of a [`KilnError`]. The protocol sends it to the UI,
/// which chooses wording and recovery from it; never match on message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ErrorKind {
    /// A document, node, file or request id does not exist.
    NotFound,
    /// The input was malformed (bad JSON, out-of-range position, unknown option).
    InvalidInput,
    /// The command exists but cannot run in the current state (wrong app, read-only, nothing
    /// to undo).
    Unavailable,
    /// The feature or format is not supported (yet) by this build.
    Unsupported,
    /// A file could not be parsed or written in the requested format.
    Format,
    /// Reading or writing failed in the host (file system, clipboard, network).
    Io,
    /// The user cancelled a dialog or the operation was superseded.
    Cancelled,
    /// The session has shut down.
    Closed,
    /// A bug: an invariant inside the core did not hold.
    Internal,
}

/// The error type of the Kiln core.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KilnError {
    /// Something that was referenced does not exist.
    #[error("{what} not found")]
    NotFound {
        /// What was looked up, e.g. `doc#3`.
        what: String,
    },
    /// The input was malformed.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The command cannot run now.
    #[error("{command} is not available: {reason}")]
    Unavailable {
        /// The command id, e.g. `write.edit.insert_text`.
        command: String,
        /// Why, in English (the UI shows its own localised text).
        reason: String,
    },
    /// Not supported by this build.
    #[error("not supported: {0}")]
    Unsupported(String),
    /// A file format error.
    #[error("{format}: {message}")]
    Format {
        /// The format, e.g. `docx`.
        format: String,
        /// What went wrong.
        message: String,
    },
    /// A host I/O error.
    #[error("i/o error: {0}")]
    Io(String),
    /// Cancelled by the user.
    #[error("cancelled")]
    Cancelled,
    /// The session has shut down.
    #[error("the session is closed")]
    Closed,
    /// An internal invariant failed.
    #[error("internal error: {0}")]
    Internal(String),
}

impl KilnError {
    /// The stable classification of this error.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            KilnError::NotFound { .. } => ErrorKind::NotFound,
            KilnError::InvalidInput(_) => ErrorKind::InvalidInput,
            KilnError::Unavailable { .. } => ErrorKind::Unavailable,
            KilnError::Unsupported(_) => ErrorKind::Unsupported,
            KilnError::Format { .. } => ErrorKind::Format,
            KilnError::Io(_) => ErrorKind::Io,
            KilnError::Cancelled => ErrorKind::Cancelled,
            KilnError::Closed => ErrorKind::Closed,
            KilnError::Internal(_) => ErrorKind::Internal,
        }
    }

    /// Shorthand for [`KilnError::NotFound`].
    pub fn not_found(what: impl std::fmt::Display) -> Self {
        KilnError::NotFound {
            what: what.to_string(),
        }
    }

    /// Shorthand for [`KilnError::Unavailable`].
    pub fn unavailable(command: impl Into<String>, reason: impl Into<String>) -> Self {
        KilnError::Unavailable {
            command: command.into(),
            reason: reason.into(),
        }
    }
}

impl From<std::io::Error> for KilnError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => KilnError::NotFound {
                what: e.to_string(),
            },
            _ => KilnError::Io(e.to_string()),
        }
    }
}

/// `Result` with [`KilnError`].
pub type Result<T, E = KilnError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_a_kind_and_a_message() {
        let cases = [
            (
                KilnError::not_found("doc#3"),
                ErrorKind::NotFound,
                "doc#3 not found",
            ),
            (
                KilnError::InvalidInput("bad".into()),
                ErrorKind::InvalidInput,
                "invalid input: bad",
            ),
            (
                KilnError::unavailable("write.edit.insert_text", "read-only"),
                ErrorKind::Unavailable,
                "write.edit.insert_text is not available: read-only",
            ),
            (
                KilnError::Unsupported("vba".into()),
                ErrorKind::Unsupported,
                "not supported: vba",
            ),
            (
                KilnError::Format {
                    format: "docx".into(),
                    message: "no body".into(),
                },
                ErrorKind::Format,
                "docx: no body",
            ),
            (
                KilnError::Io("disk full".into()),
                ErrorKind::Io,
                "i/o error: disk full",
            ),
            (KilnError::Cancelled, ErrorKind::Cancelled, "cancelled"),
            (
                KilnError::Closed,
                ErrorKind::Closed,
                "the session is closed",
            ),
            (
                KilnError::Internal("x".into()),
                ErrorKind::Internal,
                "internal error: x",
            ),
        ];
        for (err, kind, msg) in cases {
            assert_eq!(err.kind(), kind);
            assert_eq!(err.to_string(), msg);
        }
    }

    #[test]
    fn io_errors_map_to_not_found_or_io() {
        let nf = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        assert_eq!(KilnError::from(nf).kind(), ErrorKind::NotFound);
        let other = std::io::Error::other("boom");
        assert_eq!(KilnError::from(other).kind(), ErrorKind::Io);
    }
}
