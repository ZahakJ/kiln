// SPDX-License-Identifier: AGPL-3.0-or-later
//! Collaboration and Version History in the session.
//!
//! A shared document has an engine (`kiln-collab`, reached through a [`CollabBackend`]): the
//! session hands it every transaction the document's model applies, sends the frames it
//! returns on the document's WebSocket (`HostOp::Socket*`), and applies what it says about
//! frames from the server (other people's edits, presence). While a document is shared,
//! Undo and Redo are the engine's, so they undo only this person's changes. Version History
//! lists, previews (the version read-only in place of the document, through the same Scene),
//! names and restores versions, and compares a Write version with the document word by word.
//!
//! Natively the engine is linked in (feature `collab`); the web core leaves `kiln-collab` out
//! (budget B12) and the page gives it the collaboration module through
//! [`crate::Session::set_collab`] once a document is shared or joined.

mod session;

#[cfg(all(feature = "collab", not(target_arch = "wasm32")))]
mod net;

#[cfg(all(feature = "collab", not(target_arch = "wasm32")))]
pub use net::NetHost;

use std::collections::BTreeMap;

use kiln_common::{DocId, Result};
use kiln_protocol::DocKind;
use kiln_protocol::collab::engine::{CollabChange, EngineReply, EngineRequest};
use kiln_protocol::collab::{CollabCaret, CollabLink, CollabPeer, VersionSlide};
use kiln_protocol::geom::Rect;
use kiln_protocol::scene::SceneDelta;

/// `Send` natively, where the session may be created on its own thread; nothing on the web.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}
/// `Send` natively, where the session may be created on its own thread; nothing on the web.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// Where the collaboration engines run: `kiln_collab::Engines` natively, the web's
/// collaboration module on the web.
pub trait CollabBackend: MaybeSend {
    /// One call to an engine.
    ///
    /// # Errors
    ///
    /// The engine's error.
    fn request(&mut self, request: EngineRequest) -> Result<EngineReply>;
}

#[cfg(feature = "collab")]
impl CollabBackend for kiln_collab::Engines {
    fn request(&mut self, request: EngineRequest) -> Result<EngineReply> {
        kiln_collab::Engines::request(self, request)
            .map_err(|e| kiln_common::KilnError::InvalidInput(e.to_string()))
    }
}

/// The backend a session starts with: the linked engine natively, none on the web.
fn default_backend() -> Option<Box<dyn CollabBackend>> {
    #[cfg(feature = "collab")]
    {
        Some(Box::new(kiln_collab::Engines::default()))
    }
    #[cfg(not(feature = "collab"))]
    {
        None
    }
}

/// Where this person is in a document, for the others.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Whereabouts {
    /// The caret.
    pub caret: Option<CollabCaret>,
    /// The other end of a text selection.
    pub anchor: Option<CollabCaret>,
    /// The slide shown (Deck).
    pub slide: Option<u64>,
    /// The selected shapes (Deck).
    pub shapes: Vec<u64>,
}

/// What a document model does for collaboration (Write, Notes and Deck implement it).
pub trait Collaborative {
    /// The kind of document.
    fn collab_kind(&self) -> DocKind;
    /// The model as shared: postcard of the document or presentation (the live one while a
    /// version is previewed).
    ///
    /// # Errors
    ///
    /// Encoding failed.
    fn model_bytes(&self) -> Result<Vec<u8>>;
    /// Starts or stops recording the transactions the model applies.
    fn set_shared(&mut self, shared: bool);
    /// The transactions (postcard) applied since the last call, in order.
    fn take_applied(&mut self) -> Vec<Vec<u8>>;
    /// Applies what the engine returned: others' edits (`own: false`; the selection follows
    /// them), or this person's undo or redo (`own: true`; the selection is the step's).
    ///
    /// # Errors
    ///
    /// Bytes that are not a transaction or model of this kind, or one the model refuses.
    fn apply_change(&mut self, change: &CollabChange, own: bool) -> Result<()>;
    /// The id lane the server gave this replica.
    fn set_lanes(&mut self, lane: u64, lanes: u64);
    /// Where this person is.
    fn whereabouts(&self) -> Whereabouts;
    /// The others, to draw their carets and selections.
    fn set_peers(&mut self, peers: &[CollabPeer]);
    /// Where a person's caret or selection is drawn (document or slide coordinates).
    fn locate(&mut self, peer: &CollabPeer) -> Option<Rect>;
    /// Whether this person can undo and redo (the engine knows while the document is shared).
    fn set_undo_state(&mut self, can_undo: bool, can_redo: bool);
    /// Shows a version (postcard of its model) read-only in place of the document, or the
    /// document again (`None`).
    ///
    /// # Errors
    ///
    /// Bytes that are not a model of this kind.
    fn preview(&mut self, model: Option<&[u8]>) -> Result<()>;
    /// the word-level comparison of a version (postcard of its model) with the document
    /// (Write), for Version History of documents that are not shared.
    ///
    /// # Errors
    ///
    /// `Unsupported` for documents without text comparisons; bytes that are not a model of
    /// this kind.
    fn version_compare(&self, old: &[u8]) -> Result<Vec<kiln_protocol::collab::VersionParagraph>> {
        let _ = old;
        Err(kiln_common::KilnError::Unsupported(
            "comparing versions of this kind of document".into(),
        ))
    }

    /// the slides of a version marked against the version before it (Deck).
    ///
    /// # Errors
    ///
    /// `Unsupported` for documents without slides; bytes that are not a model of this kind.
    fn version_marks(&self, before: Option<&[u8]>, version: &[u8]) -> Result<Vec<VersionSlide>> {
        let _ = (before, version);
        Err(kiln_common::KilnError::Unsupported(
            "slides of this kind of document".into(),
        ))
    }

    /// Thumbnails of a version's slides (Deck), one tile per entry of `slides`.
    ///
    /// # Errors
    ///
    /// `Unsupported` for documents without slides.
    fn version_slides(
        &mut self,
        model: &[u8],
        slides: &[VersionSlide],
        width: f32,
        dpr: f32,
    ) -> Result<SceneDelta> {
        let _ = (model, slides, width, dpr);
        Err(kiln_common::KilnError::Unsupported(
            "slide thumbnails of this kind of document".into(),
        ))
    }
}

/// A shared document as the session follows it.
#[derive(Debug)]
pub(crate) struct Shared {
    /// The engine's handle.
    handle: u32,
    link: CollabLink,
    name: String,
    /// The current connection.
    socket: u32,
    /// The socket is open.
    connected: bool,
    /// A connection is being opened.
    opening: bool,
    /// The server admitted this replica.
    online: bool,
    /// The joiner's document has not arrived yet.
    joining: bool,
    /// The server refused the document: no more reconnecting.
    refused: bool,
    problem: Option<String>,
    peers: Vec<CollabPeer>,
    queued: u32,
    colour: u32,
    /// The version shown instead of the document.
    preview: Option<String>,
    /// The document was read-only before it was shared (opened read-only).
    read_only: bool,
    /// The whereabouts last sent.
    sent: Option<Whereabouts>,
    /// The delay before the next reconnection attempt.
    backoff_ms: u32,
    /// A timer is running.
    timer: bool,
}

/// What a host request for a shared document continues with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// The socket opened (or did not).
    Open { doc: DocId, socket: u32 },
    /// The socket's next message (or its close).
    Receive { doc: DocId, socket: u32 },
    /// A message was sent.
    Send { doc: DocId, socket: u32 },
    /// The document's timer.
    Timer { doc: DocId },
}

impl Wait {
    /// The document the continuation works on.
    pub(crate) fn doc(self) -> DocId {
        match self {
            Self::Open { doc, .. }
            | Self::Receive { doc, .. }
            | Self::Send { doc, .. }
            | Self::Timer { doc } => doc,
        }
    }
}

/// The session's collaboration state: the backend and the shared documents.
pub(crate) struct Collab {
    backend: Option<Box<dyn CollabBackend>>,
    shared: BTreeMap<DocId, Shared>,
    next_socket: u32,
}

impl std::fmt::Debug for Collab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collab")
            .field("backend", &self.backend.is_some())
            .field("shared", &self.shared.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Default for Collab {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            shared: BTreeMap::new(),
            next_socket: 1,
        }
    }
}

impl Collab {
    /// Whether a document is shared.
    pub(crate) fn is_shared(&self, doc: DocId) -> bool {
        self.shared.contains_key(&doc)
    }

    /// Whether a shared document shows something other than itself (a version, or nothing yet
    /// while joining), so it takes no edits.
    pub(crate) fn locks(&self, doc: DocId) -> bool {
        self.shared
            .get(&doc)
            .is_some_and(|s| s.joining || s.preview.is_some())
    }
}

/// A random peer id below 2⁵³ (JavaScript numbers hold it exactly), never 0.
fn random_peer() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    (u64::from_le_bytes(b) & ((1 << 53) - 1)).max(1)
}

/// A random room id: ten lowercase letters and digits, starting with a letter (a link's `doc`
/// is never a number, which addresses an open document).
fn random_room() -> String {
    const LETTERS: &[u8] = b"abcdefghijkmnpqrstuvwxyz";
    const ALL: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut b = [0u8; 10];
    let _ = getrandom::fill(&mut b);
    b.iter()
        .enumerate()
        .map(|(i, x)| {
            let set = if i == 0 { LETTERS } else { ALL };
            char::from(set[usize::from(*x) % set.len()])
        })
        .collect()
}

/// The WebSocket URL of a room.
fn room_url(link: &CollabLink) -> String {
    format!("{}/rooms/{}", link.server.trim_end_matches('/'), link.room)
}

/// Initials for an avatar (the engine's rule).
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = |w: &str| w.chars().find(|c| c.is_alphanumeric());
    let out: String = match words.as_slice() {
        [] => String::new(),
        [one] => one
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(2)
            .collect(),
        [a, .., b] => first(a).into_iter().chain(first(b)).collect(),
    };
    out.to_uppercase()
}

#[cfg(all(test, feature = "collab"))]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn rooms_are_never_numbers_and_peers_fit_javascript() {
        for _ in 0..64 {
            let r = random_room();
            assert_eq!(r.len(), 10);
            assert!(r.starts_with(|c: char| c.is_ascii_lowercase()));
            assert!(r.parse::<u64>().is_err());
            let p = random_peer();
            assert!(p > 0 && p < 1 << 53);
        }
    }

    #[test]
    fn initials_take_the_first_and_last_words() {
        assert_eq!(initials("Rana Okafor"), "RO");
        assert_eq!(initials("Ana María Martín"), "AM");
        assert_eq!(initials("zoë"), "ZO");
        assert_eq!(initials("  "), "");
    }

    #[test]
    fn a_room_url_is_under_the_server() {
        let link = CollabLink {
            server: "ws://127.0.0.1:8640/".into(),
            room: "k7p2q9xw".into(),
            token: String::new(),
        };
        assert_eq!(room_url(&link), "ws://127.0.0.1:8640/rooms/k7p2q9xw");
    }
}
