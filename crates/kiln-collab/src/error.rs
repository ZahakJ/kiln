// SPDX-License-Identifier: AGPL-3.0-or-later
//! Why a collaboration operation failed.

/// Why a collaboration operation failed. A failed import or commit leaves the model unchanged.
#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    /// The CRDT refused an update, a version or an edit.
    #[error("crdt: {0}")]
    Crdt(String),
    /// The shared state does not describe a valid model (a damaged or foreign document).
    #[error("shared state: {0}")]
    Corrupt(String),
    /// A model value could not be stored or read back.
    #[error("value: {0}")]
    Value(#[from] serde_json::Error),
    /// The Write model refused an operation.
    #[error(transparent)]
    Doc(#[from] kiln_doc::DocError),
    /// The Deck model refused an operation.
    #[error(transparent)]
    Deck(#[from] kiln_deck::DeckError),
    /// A sync frame or history record could not be decoded.
    #[error("encoding: {0}")]
    Encoding(String),
    /// A version that history does not hold.
    #[error("no version {0}")]
    UnknownVersion(String),
    /// Undo (or Redo) would remove text someone else has typed since; the earlier steps go
    /// too.
    #[error("this can't be undone: it would remove text someone else has typed since")]
    UndoBlocked,
}

impl From<loro::LoroError> for CollabError {
    fn from(e: loro::LoroError) -> Self {
        Self::Crdt(e.to_string())
    }
}

impl From<loro::LoroEncodeError> for CollabError {
    fn from(e: loro::LoroEncodeError) -> Self {
        Self::Crdt(e.to_string())
    }
}

impl From<postcard::Error> for CollabError {
    fn from(e: postcard::Error) -> Self {
        Self::Encoding(e.to_string())
    }
}

/// A collaboration result.
pub type Result<T, E = CollabError> = std::result::Result<T, E>;

/// A [`CollabError::Corrupt`] with a message.
pub(crate) fn corrupt(m: impl Into<String>) -> CollabError {
    CollabError::Corrupt(m.into())
}
