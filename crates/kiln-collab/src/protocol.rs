// SPDX-License-Identifier: AGPL-3.0-or-later
//! The sync protocol between a Kiln client and `kiln-collab-server`, one [`Frame`] per
//! WebSocket binary message.
//!
//! A frame is `K C` (two bytes), the protocol version (`u16`, little-endian), then the frame in
//! postcard. A peer refuses frames of a protocol version it does not know; a later version adds
//! frame kinds and fields after the existing ones, so version 1 frames stay readable.
//!
//! A session: the client sends [`Frame::Hello`] with the room, the shared token and the CRDT
//! version it already has (empty for nothing). The server answers [`Frame::Welcome`] with the
//! changes the client lacks, the server's own version and the client's id lane, or
//! [`Frame::Refused`] and closes. The client then sends what the server lacks as one update:
//! the whole document when it creates the room, the edits made offline when it reconnects.
//! After that both sides send [`Frame::Update`]s as edits happen and [`Frame::Awareness`] when a
//! cursor moves; the server relays both to the room's other clients and stores updates.

use serde::{Deserialize, Serialize};

use crate::error::{CollabError, Result};
use crate::presence::Presence;

/// The protocol version this build speaks.
pub const PROTOCOL: u16 = 1;

const MAGIC: [u8; 2] = *b"KC";

/// The kind of document a room holds (set by the client that created the room).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoomKind {
    /// A Kiln Write document.
    Write,
    /// A Kiln Deck presentation.
    Deck,
    /// A Kiln Notes page (a Write document).
    Notes,
}

/// Why the server refused a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The token does not open this room.
    BadToken,
    /// The client speaks a protocol version the server does not.
    Protocol(u16),
    /// The room holds another kind of document.
    WrongKind(RoomKind),
    /// The server could not serve the room or store a change (a storage failure): try again
    /// later.
    Unavailable(String),
    /// The room has given out all its id lanes and admits nobody new.
    Full,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadToken => f.write_str("the token does not open this room"),
            Self::Protocol(v) => write!(f, "protocol version {v} is not supported"),
            Self::WrongKind(k) => write!(f, "the room holds a {k:?} document"),
            Self::Unavailable(m) => write!(f, "the room is unavailable: {m}"),
            Self::Full => f.write_str(
                "the room admits nobody new: share the document again to start a new room",
            ),
        }
    }
}

/// One message of the sync protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    /// Client → server, first: who joins which room with what.
    Hello {
        /// The room id (from the share link's `doc`).
        room: String,
        /// The shared token (from the share link's `token`).
        token: String,
        /// The client's CRDT peer id.
        peer: u64,
        /// The kind of document the client expects or creates.
        kind: RoomKind,
        /// The client's CRDT version (empty: it has nothing yet).
        version: Vec<u8>,
        /// The id lane the client holds from an earlier welcome of this room.
        lane: Option<u32>,
    },
    /// Server → client: joined.
    Welcome {
        /// The kind of document the room holds.
        kind: RoomKind,
        /// What the client lacks: updates since its version, or the room's snapshot.
        state: Vec<u8>,
        /// The server's CRDT version (the client sends what it lacks).
        version: Vec<u8>,
        /// The client's id lane and the number of lanes (`kiln_doc::IdLanes`).
        lane: u32,
        /// The number of lanes.
        lanes: u32,
        /// Presence of the peers already in the room.
        peers: Vec<Presence>,
    },
    /// Server → client: not admitted (the server closes the connection).
    Refused {
        /// Why.
        reason: Refusal,
    },
    /// Either direction: a CRDT update.
    Update {
        /// The update.
        bytes: Vec<u8>,
    },
    /// Either direction: one peer's cursor, selection, name and colour.
    Awareness {
        /// The presence.
        presence: Presence,
    },
    /// Server → client: a peer left the room.
    Left {
        /// Its peer id.
        peer: u64,
    },
    /// Client → server: send everything after `version` (empty: the whole snapshot).
    SnapshotRequest {
        /// The version the client has.
        version: Vec<u8>,
    },
    /// Server → client: the answer to [`Frame::SnapshotRequest`].
    Snapshot {
        /// A snapshot or the updates after the requested version.
        bytes: Vec<u8>,
        /// The server's version.
        version: Vec<u8>,
    },
    /// Client → server: a piece of an update too large for one message
    /// ([`crate::client::PART`]); the pieces up to the last make one [`Frame::Update`].
    Part {
        /// The piece.
        bytes: Vec<u8>,
        /// Whether it is the last.
        last: bool,
    },
}

impl Frame {
    /// The frame as sent.
    ///
    /// # Errors
    ///
    /// postcard failed to encode (it does not for these types).
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&PROTOCOL.to_le_bytes());
        Ok(postcard::to_extend(self, out)?)
    }

    /// A frame as received.
    ///
    /// # Errors
    ///
    /// [`CollabError::Encoding`] for bytes that are not a Kiln frame, a protocol version this
    /// build does not speak ([`protocol_of`] tells which) or a damaged frame.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let version = protocol_of(bytes)
            .ok_or_else(|| CollabError::Encoding("not a Kiln sync frame".into()))?;
        if version != PROTOCOL {
            return Err(CollabError::Encoding(format!(
                "protocol version {version} (this build speaks {PROTOCOL})"
            )));
        }
        let (frame, rest): (Frame, &[u8]) = postcard::take_from_bytes(&bytes[4..])?;
        if !rest.is_empty() {
            return Err(CollabError::Encoding(format!(
                "{} trailing bytes",
                rest.len()
            )));
        }
        Ok(frame)
    }
}

/// The protocol version of a frame, if the bytes are one.
#[must_use]
pub fn protocol_of(bytes: &[u8]) -> Option<u16> {
    match bytes {
        [b'K', b'C', lo, hi, ..] => Some(u16::from_le_bytes([*lo, *hi])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_carry_their_version() -> Result<()> {
        let frames = [
            Frame::Hello {
                room: "r1".into(),
                token: "t".into(),
                peer: 7,
                kind: RoomKind::Write,
                version: vec![1, 2],
                lane: Some(3),
            },
            Frame::Refused {
                reason: Refusal::BadToken,
            },
            Frame::Update { bytes: vec![9; 40] },
            Frame::Left { peer: 3 },
            Frame::SnapshotRequest {
                version: Vec::new(),
            },
        ];
        for f in frames {
            let bytes = f.encode()?;
            assert_eq!(protocol_of(&bytes), Some(PROTOCOL));
            assert_eq!(Frame::decode(&bytes)?, f);
        }
        Ok(())
    }

    #[test]
    fn foreign_versions_and_damage_are_refused() -> Result<()> {
        let mut bytes = Frame::Left { peer: 1 }.encode()?;
        bytes[2] = 9;
        assert!(Frame::decode(&bytes).is_err());
        assert_eq!(protocol_of(&bytes), Some(9));
        assert!(Frame::decode(b"{json}").is_err());
        let mut long = Frame::Left { peer: 1 }.encode()?;
        long.push(0);
        assert!(Frame::decode(&long).is_err());
        Ok(())
    }
}
