// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck's comments and spelling: what the Comments pane, the slide's comment markers
//! and the Spelling pane show ([`super::DeckState::comments`], [`super::DeckState::spelling`]).

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A reply in a comment thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckCommentReply {
    /// The reply.
    pub id: NodeId,
    /// Author.
    pub author: String,
    /// When (ISO 8601).
    pub date: String,
    /// The text.
    pub text: String,
}

/// A comment thread of the current slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckComment {
    /// The thread.
    pub id: NodeId,
    /// Author.
    pub author: String,
    /// Author's initials.
    pub initials: String,
    /// When (ISO 8601).
    pub date: String,
    /// The text.
    pub text: String,
    /// The thread is resolved.
    pub resolved: bool,
    /// The object it is on (`None`: the slide).
    pub shape: Option<NodeId>,
    /// That object's name.
    pub shape_name: Option<String>,
    /// Where its marker is on the slide, slide px.
    pub x: f32,
    /// Where its marker is on the slide, slide px.
    pub y: f32,
    /// The author's colour, `RRGGBB`.
    pub color: String,
    /// Replies, oldest first.
    pub replies: Vec<DeckCommentReply>,
}

/// What Delete Comment removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckCommentScope {
    /// The thread (or the reply) given.
    One,
    /// Delete All Comments on This Slide.
    Slide,
    /// Delete All Comments in This Presentation.
    All,
}

/// The misspelled word Spelling stopped at (it is selected on the slide or in the notes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSpelling {
    /// The slide (index).
    pub slide: u32,
    /// The word is in the speaker notes.
    pub notes: bool,
    /// The word.
    pub word: String,
    /// Corrections, best first.
    pub suggestions: Vec<String>,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of these types (see `crate::samples`).

    use super::*;
    use crate::samples::Samples;

    macro_rules! samples_of {
        ($($ty:ty => [$($v:expr),* $(,)?];)*) => {
            $(impl Samples for $ty {
                fn samples() -> Vec<Self> {
                    vec![$($v),*]
                }
            })*
        };
    }

    /// A thread with a reply.
    #[must_use]
    pub fn comment() -> DeckComment {
        DeckComment {
            id: NodeId(40),
            author: "Zoë Okafor".into(),
            initials: "ZO".into(),
            date: "2026-10-22T09:30:00Z".into(),
            text: "Can we show last year here?".into(),
            resolved: false,
            shape: Some(NodeId(12)),
            shape_name: Some("Title 1".into()),
            x: 1020.0,
            y: 64.0,
            color: "C44D27".into(),
            replies: vec![reply()],
        }
    }

    /// A reply.
    #[must_use]
    pub fn reply() -> DeckCommentReply {
        DeckCommentReply {
            id: NodeId(41),
            author: "Sam Price".into(),
            date: "2026-10-22T10:02:00Z".into(),
            text: "Added it.".into(),
        }
    }

    /// A misspelling in the notes.
    #[must_use]
    pub fn spelling() -> DeckSpelling {
        DeckSpelling {
            slide: 2,
            notes: true,
            word: "glazze".into(),
            suggestions: vec!["glaze".into(), "glazes".into()],
        }
    }

    samples_of! {
        DeckComment => [comment()];
        DeckCommentReply => [reply()];
        DeckCommentScope => [DeckCommentScope::One, DeckCommentScope::Slide, DeckCommentScope::All];
        DeckSpelling => [spelling()];
    }

    /// Registers the types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v; DeckComment, DeckCommentReply, DeckCommentScope, DeckSpelling);
    }
}
