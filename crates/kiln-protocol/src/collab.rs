// SPDX-License-Identifier: AGPL-3.0-or-later
//! Collaboration and Version History: what the Share dialog, the presence avatars, the
//! remote carets and the Version History pane show, and the calls between the core and the
//! collaboration engine ([`engine`]).
//!
//! Peer ids are random 64-bit numbers, so they travel as decimal strings.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Rect;
use crate::scene::SceneDelta;

/// A caret or the other end of a selection: an offset in a paragraph, of a shape's text in Deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CollabCaret {
    /// The shape whose text holds the paragraph (Deck), or none (Write and Notes).
    pub shape: Option<NodeId>,
    /// The paragraph.
    pub para: NodeId,
    /// The character offset.
    pub offset: u32,
}

/// Another person in a shared document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CollabPeer {
    /// Their peer id.
    pub peer: String,
    /// Their display name.
    pub name: String,
    /// Initials for the avatar.
    pub initials: String,
    /// Their colour (`RRGGBB`): avatar, caret and selection.
    pub colour: String,
    /// Where their caret is, in this replica's document.
    pub caret: Option<CollabCaret>,
    /// The other end of their text selection.
    pub anchor: Option<CollabCaret>,
    /// The slide they are on (Deck).
    pub slide: Option<NodeId>,
    /// The shapes they selected (Deck).
    pub shapes: Vec<NodeId>,
    /// Where their caret (or selection) is drawn, in document coordinates (Deck: in the slide's),
    /// for the name flag; none when it is not on a laid-out page or the current slide.
    pub at: Option<Rect>,
}

/// The connection of a shared document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CollabStatus {
    /// Connecting to the server.
    Connecting,
    /// Joined: edits are exchanged as they are made.
    Online,
    /// The connection is down: edits wait and merge when it comes back.
    Offline,
    /// The server refused the document (wrong token, another kind of document in the room).
    Refused,
}

/// Where a shared document lives: the room on a collaboration server, and the token it takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CollabLink {
    /// The collaboration server (`wss://…` or `ws://…`).
    pub server: String,
    /// The room (the link's `doc`).
    pub room: String,
    /// The shared token.
    pub token: String,
}

/// A document's collaboration state (`Query::Collab`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CollabState {
    /// Whether the document is shared.
    pub shared: bool,
    /// The connection (meaningful when shared).
    pub status: CollabStatus,
    /// Where it is shared.
    pub link: Option<CollabLink>,
    /// This person's display name, initials and colour.
    pub name: String,
    /// Initials for this person's avatar.
    pub initials: String,
    /// This person's colour (`RRGGBB`).
    pub colour: String,
    /// The others, in the order they joined.
    pub peers: Vec<CollabPeer>,
    /// Local changes waiting for the connection.
    pub queued: u32,
    /// The version shown read-only instead of the document, if any.
    pub preview: Option<String>,
    /// Why the server refused the document, or what went wrong with the connection.
    pub problem: Option<String>,
}

/// Why a version was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum VersionOrigin {
    /// Someone saved the document.
    Save,
    /// Ten minutes of editing.
    Auto,
    /// A version was restored (the id of the version).
    Restore(String),
    /// The document was shared: what it was then.
    Shared,
}

/// One version of a shared document (`Query::Versions`, oldest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VersionEntry {
    /// The version.
    pub id: String,
    /// The name someone gave it.
    pub name: Option<String>,
    /// Who made it.
    pub author: String,
    /// When (milliseconds since the Unix epoch).
    #[ts(type = "number")]
    pub time_ms: u64,
    /// Why it was taken.
    pub origin: VersionOrigin,
}

/// What happened to text between a version and the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum VersionChange {
    /// In both.
    Same,
    /// Only in the document (added since the version).
    Added,
    /// Only in the version (removed since).
    Removed,
}

/// A stretch of text in a comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VersionSpan {
    /// What happened to it.
    pub change: VersionChange,
    /// The text.
    pub text: String,
}

/// One paragraph of the word-level comparison of a Write version with the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VersionParagraph {
    /// The paragraph as a whole.
    pub change: VersionChange,
    /// Its text, stretch by stretch.
    pub spans: Vec<VersionSpan>,
}

/// How a slide of a Deck version differs from the version before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum VersionSlideMark {
    /// Unchanged.
    Same,
    /// New in this version.
    Added,
    /// Edited.
    Changed,
}

/// A slide of a Deck version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VersionSlide {
    /// The slide.
    pub slide: NodeId,
    /// How it differs from the version before.
    pub mark: VersionSlideMark,
}

/// The slides of a Deck version with their thumbnails (`Query::VersionSlides`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct VersionSlides {
    /// The slides, in order.
    pub slides: Vec<VersionSlide>,
    /// Their thumbnails as one self-contained scene (tile index = index in `slides`).
    pub scene: SceneDelta,
}

pub mod engine {
    //! The calls between the core and the collaboration engine (`kiln-collab`). Natively the
    //! session calls the engine directly; on the web the engine is its own module, loaded only
    //! when a document is shared, and these values cross between the two as postcard.

    use serde::{Deserialize, Serialize};

    use super::{CollabCaret, CollabPeer, VersionEntry, VersionParagraph, VersionSlide};
    use crate::app::DocKind;

    /// A call to the engine of one shared document (or one that creates it).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub enum EngineCall {
        /// Starts sharing a model (postcard of the document or presentation).
        Share {
            /// The kind of document.
            kind: DocKind,
            /// The model.
            model: Vec<u8>,
            /// The room.
            room: String,
            /// The token.
            token: String,
            /// This replica's peer id.
            peer: u64,
            /// This person's display name.
            name: String,
            /// The host's clock (the first version is the document as it was shared).
            now_ms: u64,
        },
        /// Joins a room; the model comes with the server's welcome.
        Join {
            /// The kind of document.
            kind: DocKind,
            /// The room.
            room: String,
            /// The token.
            token: String,
            /// This replica's peer id.
            peer: u64,
            /// This person's display name.
            name: String,
        },
        /// The first frame of a connection.
        Hello,
        /// The connection went down.
        Disconnected,
        /// The core applied a transaction (postcard).
        Commit {
            /// The transaction.
            tx: Vec<u8>,
            /// The host's clock.
            now_ms: u64,
        },
        /// A frame from the server.
        Receive {
            /// The frame.
            frame: Vec<u8>,
            /// The host's clock.
            now_ms: u64,
        },
        /// Undoes this person's latest change.
        Undo,
        /// Redoes this person's latest undone change.
        Redo,
        /// The caret moved: the next edit is a new undo step.
        BreakUndoGroup,
        /// This person's caret, selection, slide and selected shapes.
        Presence {
            /// The caret.
            caret: Option<CollabCaret>,
            /// The selection's other end.
            anchor: Option<CollabCaret>,
            /// The slide shown (Deck).
            slide: Option<u64>,
            /// The selected shapes (Deck).
            shapes: Vec<u64>,
        },
        /// Time passed: silent people leave, ten minutes of edits make a version.
        Tick {
            /// The host's clock.
            now_ms: u64,
        },
        /// The document was saved: a version.
        Saved {
            /// The host's clock.
            now_ms: u64,
        },
        /// The versions.
        Versions,
        /// Names a version (an empty name removes it).
        RenameVersion {
            /// The version.
            id: String,
            /// The name.
            name: String,
        },
        /// A version's model (postcard), to show read-only.
        Checkout {
            /// The version.
            id: String,
        },
        /// Restores a version as a new change.
        Restore {
            /// The version.
            id: String,
            /// The host's clock.
            now_ms: u64,
        },
        /// The word-level comparison of a Write version with the document.
        Compare {
            /// The version.
            id: String,
        },
        /// The slides of a Deck version, marked against the version before it.
        SlideChanges {
            /// The version.
            id: String,
        },
        /// The engine's state.
        State,
        /// Forgets the engine (sharing stopped, the document closed).
        Close,
    }

    /// What the core does with the model after a call.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum CollabChange {
        /// Nothing.
        #[default]
        None,
        /// Apply this transaction (postcard) and lay out what it touched.
        Apply(Vec<u8>),
        /// Replace the model with this one (postcard) and lay everything out again.
        Replace(Vec<u8>),
    }

    /// A value some calls return.
    #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
    pub enum EngineValue {
        /// Nothing.
        #[default]
        None,
        /// The engine that was created.
        Created(u32),
        /// The versions, oldest first.
        Versions(Vec<VersionEntry>),
        /// A model (postcard).
        Model(Vec<u8>),
        /// A comparison.
        Compare(Vec<VersionParagraph>),
        /// A version's slides.
        Slides(Vec<VersionSlide>),
        /// The version a save or tick took.
        Version(Option<String>),
    }

    /// The engine's answer to a call.
    #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
    pub struct EngineReply {
        /// What the core applies.
        pub change: CollabChange,
        /// Frames to send now, in order.
        pub frames: Vec<Vec<u8>>,
        /// The id lane the server gave this replica (`lane`, `lanes`), after its welcome.
        pub lanes: Option<(u64, u64)>,
        /// A joiner's model became available: the core loads `change` (a `Replace`).
        pub joined: bool,
        /// The server refused the document (for good: a wrong token, another kind of
        /// document, a full room).
        pub refused: Option<String>,
        /// The server could not serve the document for now (a storage failure): the connection
        /// closes and is tried again.
        pub unavailable: Option<String>,
        /// The server admitted this replica.
        pub online: bool,
        /// Local updates waiting for the connection.
        pub queued: u32,
        /// Whether this person has a change to undo.
        pub can_undo: bool,
        /// Whether this person has an undone change to redo.
        pub can_redo: bool,
        /// The others, when they changed (who is there, where they are).
        pub peers: Option<Vec<CollabPeer>>,
        /// This person's colour (`0xRRGGBB`).
        pub colour: u32,
        /// A value.
        pub value: EngineValue,
    }

    /// A call for the engine `handle` (0: a call that creates one).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct EngineRequest {
        /// The engine.
        pub handle: u32,
        /// The call.
        pub call: EngineCall,
    }
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

    fn caret() -> CollabCaret {
        CollabCaret {
            shape: None,
            para: NodeId(12),
            offset: 7,
        }
    }

    fn peer() -> CollabPeer {
        CollabPeer {
            peer: "1849302215568113".into(),
            name: "Rana Okafor".into(),
            initials: "RO".into(),
            colour: "1D8A60".into(),
            caret: Some(caret()),
            anchor: None,
            slide: Some(NodeId(4)),
            shapes: vec![NodeId(9)],
            at: Some(Rect::new(96.0, 240.5, 1.5, 18.0)),
        }
    }

    fn version() -> VersionEntry {
        VersionEntry {
            id: "v3".into(),
            name: Some("Draft for Saturday".into()),
            author: "Zoë Okafor".into(),
            time_ms: 1_792_000_000_000,
            origin: VersionOrigin::Save,
        }
    }

    fn paragraph() -> VersionParagraph {
        VersionParagraph {
            change: VersionChange::Same,
            spans: vec![
                VersionSpan {
                    change: VersionChange::Same,
                    text: "We load the ".into(),
                },
                VersionSpan {
                    change: VersionChange::Removed,
                    text: "small".into(),
                },
                VersionSpan {
                    change: VersionChange::Added,
                    text: "big".into(),
                },
            ],
        }
    }

    fn state() -> CollabState {
        CollabState {
            shared: true,
            status: CollabStatus::Online,
            link: Some(CollabLink {
                server: "wss://collab.example.org".into(),
                room: "k7p2q9xw".into(),
                token: "s3cret".into(),
            }),
            name: "Ana Martín".into(),
            initials: "AM".into(),
            colour: "3159C6".into(),
            peers: vec![peer()],
            queued: 0,
            preview: None,
            problem: None,
        }
    }

    samples_of! {
        CollabCaret => [caret(), CollabCaret { shape: Some(NodeId(5)), para: NodeId(1), offset: 0 }];
        CollabPeer => [peer()];
        CollabStatus => [CollabStatus::Connecting, CollabStatus::Online, CollabStatus::Offline, CollabStatus::Refused];
        CollabLink => [state().link.unwrap_or_else(|| unreachable!())];
        CollabState => [state(), CollabState {
            shared: false, status: CollabStatus::Offline, link: None, name: String::new(),
            initials: String::new(), colour: String::new(), peers: Vec::new(), queued: 3,
            preview: Some("v1".into()), problem: Some("The server refused the token.".into()),
        }];
        VersionOrigin => [VersionOrigin::Save, VersionOrigin::Auto, VersionOrigin::Restore("v1".into()), VersionOrigin::Shared];
        VersionEntry => [version()];
        VersionChange => [VersionChange::Same, VersionChange::Added, VersionChange::Removed];
        VersionSpan => [VersionSpan { change: VersionChange::Added, text: "Ünïcödé".into() }];
        VersionParagraph => [paragraph()];
        VersionSlideMark => [VersionSlideMark::Same, VersionSlideMark::Added, VersionSlideMark::Changed];
        VersionSlide => [VersionSlide { slide: NodeId(3), mark: VersionSlideMark::Changed }];
        VersionSlides => [VersionSlides {
            slides: vec![VersionSlide { slide: NodeId(3), mark: VersionSlideMark::Same }],
            scene: crate::scene::samples::small_delta(),
        }];
    }

    /// Registers the types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            CollabCaret, CollabPeer, CollabStatus, CollabLink, CollabState, VersionOrigin,
            VersionEntry, VersionChange, VersionSpan, VersionParagraph, VersionSlideMark,
            VersionSlide, VersionSlides,
        );
    }
}
