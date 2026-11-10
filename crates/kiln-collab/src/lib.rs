// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-collab
//!
//! Collaboration and version history on Loro 1.16 (docs/adr/0004-crdt.md).
//!
//! - [`crdt`]: the small [`Crdt`] trait (merge, version, export, local updates, undo/redo of the
//!   local peer, snapshots) and [`LoroCrdt`], its Loro implementation with an undo manager scoped
//!   to the local peer.
//! - [`write`] and [`deck`]: the model mappings. [`WriteBinding`] (Kiln Write and Kiln Notes)
//!   and [`DeckBinding`] record each applied model transaction as one CRDT change and turn
//!   remote changes back into model ops, so layout invalidation stays incremental.
//! - [`protocol`], [`client`], [`room`], [`presence`]: the versioned sync frames, the
//!   transport-free client with its [`OfflineQueue`], the relay's per-room state, and
//!   awareness (carets that follow edits, names, colours).
//! - [`history`] and [`diff`]: Version History (taken on save and every ten minutes of editing,
//!   at most 200, named, previewed, restored as a new version), the word-level comparison for
//!   Write and the changed slides of a Deck version.
//! - [`replica`]: both mappings behind one byte-level API, for the web's collaboration module
//!   (`kiln-collab-wasm`) and any host that holds the model elsewhere.
//!
//! Everything on Loro is behind the `crdt` feature (on by default). Without it the crate is
//! the two comparisons, [`diff`] and [`slides`], which Version History of documents that are
//! not shared uses too (the web's app cores leave Loro out, budget B12).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "crdt")]
pub mod client;
#[cfg(feature = "crdt")]
pub mod crdt;
#[cfg(feature = "crdt")]
pub mod deck;
pub mod diff;
#[cfg(feature = "crdt")]
pub mod engine;
#[cfg(feature = "crdt")]
pub mod error;
#[cfg(feature = "crdt")]
pub mod history;
#[cfg(feature = "crdt")]
mod moves;
#[cfg(feature = "crdt")]
pub mod presence;
#[cfg(feature = "crdt")]
pub mod protocol;
#[cfg(feature = "crdt")]
pub mod replica;
#[cfg(feature = "crdt")]
pub mod room;
pub mod slides;
#[cfg(feature = "crdt")]
mod value;
#[cfg(feature = "crdt")]
pub mod write;

#[cfg(feature = "crdt")]
pub use client::{Incoming, OfflineQueue, SyncClient};
#[cfg(feature = "crdt")]
pub use crdt::{Crdt, LoroCrdt, UpdateSink};
#[cfg(feature = "crdt")]
pub use deck::{DeckBinding, DeckChange};
pub use diff::{Change, ParagraphDiff, Span, text_diff, word_diff};
#[cfg(feature = "crdt")]
pub use engine::{Engine, Engines};
#[cfg(feature = "crdt")]
pub use error::{CollabError, Result};
#[cfg(feature = "crdt")]
pub use history::{Origin, VersionHistory, VersionInfo};
#[cfg(feature = "crdt")]
pub use loro::Subscription;
#[cfg(feature = "crdt")]
pub use presence::{Anchor, Peers, Presence};
#[cfg(feature = "crdt")]
pub use protocol::{Frame, PROTOCOL, Refusal, RoomKind};
#[cfg(feature = "crdt")]
pub use replica::{CoreChange, Replica};
#[cfg(feature = "crdt")]
pub use room::{Room, RoomMeta};
pub use slides::{SlideChange, SlideMark, changed_slides};
#[cfg(feature = "crdt")]
pub use write::{WriteBinding, WriteChange};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
