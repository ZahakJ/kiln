// SPDX-License-Identifier: AGPL-3.0-or-later
//! Errors. A failed operation, transaction or edit leaves the presentation unchanged.

use kiln_common::NodeId;

/// Why a model operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeckError {
    /// No node (slide, layout, master, shape, effect, comment, section) has this id.
    #[error("no node {0}")]
    UnknownNode(NodeId),
    /// The node exists but is of another kind.
    #[error("{id} is not a {expected}")]
    WrongKind {
        /// The node.
        id: NodeId,
        /// What the operation needed.
        expected: &'static str,
    },
    /// An id that is already in use (or 0).
    #[error("{0} is already in use")]
    DuplicateId(NodeId),
    /// An index beyond a list.
    #[error("index {index} is outside 0..={len}")]
    OutOfRange {
        /// The offending index.
        index: usize,
        /// The valid length.
        len: usize,
    },
    /// An operation that would break an invariant of the model.
    #[error("invalid operation: {0}")]
    InvalidOp(String),
    /// A text edit that kiln-doc refused.
    #[error("text edit: {0}")]
    Text(#[from] kiln_doc::DocError),
    /// A theme file that cannot be read.
    #[error("theme: {0}")]
    Theme(String),
    /// Stored data that breaks an invariant, or bytes that are not a presentation.
    #[error("corrupt presentation: {0}")]
    Corrupt(String),
}

/// Shorthand.
pub type Result<T, E = DeckError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(
            DeckError::OutOfRange { index: 7, len: 3 }.to_string(),
            "index 7 is outside 0..=3"
        );
        assert_eq!(
            DeckError::WrongKind {
                id: NodeId(4),
                expected: "slide"
            }
            .to_string(),
            "node#4 is not a slide"
        );
        let e: DeckError = kiln_doc::DocError::UnknownNode(NodeId(2)).into();
        assert_eq!(e.to_string(), "text edit: no node node#2");
    }
}
