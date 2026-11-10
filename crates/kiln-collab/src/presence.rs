// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presence (awareness): who is in a shared document, their colour and where their caret,
//! selection, slide and selected shapes are.
//!
//! Positions travel as Loro cursors ([`Anchor`]), which follow concurrent edits: a peer's caret
//! stays on the same character while others type before it. Presence is not part of the
//! document and is not stored; each peer sends its own on change and the others keep the
//! newest per peer ([`Peers`]), dropping peers they have not heard from for a while.

use std::collections::BTreeMap;

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};

/// A position in a paragraph that follows edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// The shape whose text holds the paragraph (Deck), or none (Write).
    pub shape: Option<NodeId>,
    /// The paragraph.
    pub para: NodeId,
    /// The character offset when the anchor was made (used when the cursor cannot be resolved).
    pub offset: u32,
    /// The encoded Loro cursor.
    pub cursor: Vec<u8>,
}

/// One peer's presence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    /// The peer's CRDT peer id.
    pub peer: u64,
    /// The display name.
    pub name: String,
    /// The colour of the peer's caret, selection and avatar (`0xRRGGBB`).
    pub colour: u32,
    /// Where the caret is.
    pub caret: Option<Anchor>,
    /// The other end of a text selection (the caret is its head).
    pub anchor: Option<Anchor>,
    /// The slide shown (Deck).
    pub slide: Option<NodeId>,
    /// The selected shapes (Deck).
    pub shapes: Vec<NodeId>,
    /// Increases with every change the peer sends; a lower one is stale.
    pub clock: u64,
}

impl Presence {
    /// A peer with nothing selected yet, in its default colour.
    #[must_use]
    pub fn new(peer: u64, name: impl Into<String>) -> Self {
        Self {
            peer,
            name: name.into(),
            colour: colour_for(peer),
            caret: None,
            anchor: None,
            slide: None,
            shapes: Vec::new(),
            clock: 0,
        }
    }
}

/// Presence colours: the app colours and the brand's earth tones, each readable as a caret on a
/// white page and a dark one.
pub const COLOURS: [u32; 8] = [
    0x3159C6, 0x1D8A60, 0xE05A27, 0xC98A12, 0xB4462A, 0x7A4A33, 0x8E3B8F, 0x1F7A8C,
];

/// The default colour of a peer (stable for a peer id).
#[must_use]
pub fn colour_for(peer: u64) -> u32 {
    let i = usize::try_from(crate::value::fnv64(&peer.to_le_bytes()) % COLOURS.len() as u64)
        .unwrap_or(0);
    COLOURS[i]
}

/// An anchor at `offset` in a paragraph's text.
pub(crate) fn anchor_in(
    text: &loro::LoroText,
    shape: Option<NodeId>,
    para: NodeId,
    offset: usize,
) -> Option<Anchor> {
    let cursor = text.get_cursor(offset, loro::cursor::Side::Middle)?;
    Some(Anchor {
        shape,
        para,
        offset: u32::try_from(offset).unwrap_or(u32::MAX),
        cursor: cursor.encode(),
    })
}

/// Where an anchor's character is now, if its paragraph still holds it.
pub(crate) fn resolve_in(doc: &loro::LoroDoc, anchor: &Anchor) -> Option<usize> {
    let cursor = loro::cursor::Cursor::decode(&anchor.cursor).ok()?;
    doc.get_cursor_pos(&cursor).ok().map(|r| r.current.pos)
}

#[derive(Debug, Clone)]
struct Seen {
    presence: Presence,
    at_ms: u64,
}

/// The newest presence of every other peer.
#[derive(Debug, Clone, Default)]
pub struct Peers {
    peers: BTreeMap<u64, Seen>,
}

impl Peers {
    /// How long a peer that sends nothing stays listed.
    pub const TIMEOUT_MS: u64 = 60_000;

    /// Records a presence received at `now_ms`. Returns `false` for a stale one.
    pub fn update(&mut self, presence: Presence, now_ms: u64) -> bool {
        match self.peers.get_mut(&presence.peer) {
            Some(s) if s.presence.clock > presence.clock => false,
            Some(s) => {
                s.presence = presence;
                s.at_ms = now_ms;
                true
            }
            None => {
                self.peers.insert(
                    presence.peer,
                    Seen {
                        presence,
                        at_ms: now_ms,
                    },
                );
                true
            }
        }
    }

    /// Marks a peer as still there (any frame from it counts).
    pub fn touch(&mut self, peer: u64, now_ms: u64) {
        if let Some(s) = self.peers.get_mut(&peer) {
            s.at_ms = now_ms;
        }
    }

    /// A peer left.
    pub fn remove(&mut self, peer: u64) -> bool {
        self.peers.remove(&peer).is_some()
    }

    /// Drops the peers not heard from for [`Self::TIMEOUT_MS`]; returns them.
    pub fn expire(&mut self, now_ms: u64) -> Vec<u64> {
        let gone: Vec<u64> = self
            .peers
            .iter()
            .filter(|(_, s)| now_ms.saturating_sub(s.at_ms) > Self::TIMEOUT_MS)
            .map(|(p, _)| *p)
            .collect();
        for p in &gone {
            self.peers.remove(p);
        }
        gone
    }

    /// The peers, by peer id.
    pub fn iter(&self) -> impl Iterator<Item = &Presence> {
        self.peers.values().map(|s| &s.presence)
    }

    /// How many peers are listed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// True when nobody else is here.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_presence_wins_and_silent_peers_expire() {
        let mut peers = Peers::default();
        let mut p = Presence::new(5, "Ada");
        p.clock = 2;
        assert!(peers.update(p.clone(), 0));
        let mut old = p.clone();
        old.clock = 1;
        old.name = "Stale".into();
        assert!(!peers.update(old, 10));
        assert_eq!(peers.iter().next().map(|p| p.name.as_str()), Some("Ada"));
        peers.touch(5, 30_000);
        assert!(peers.expire(80_000).is_empty());
        assert_eq!(peers.expire(100_000), vec![5]);
        assert!(peers.is_empty());
    }

    #[test]
    fn colours_are_stable_per_peer() {
        assert_eq!(colour_for(42), colour_for(42));
        assert!(COLOURS.contains(&colour_for(7)));
    }
}
