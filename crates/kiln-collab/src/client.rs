// SPDX-License-Identifier: AGPL-3.0-or-later
//! The client side of the sync protocol, without a transport: the shell owns the WebSocket
//! (the browser's on the web, tokio-tungstenite natively) and passes frames in and out.
//!
//! While the connection is down, local updates wait in the [`OfflineQueue`] (which the
//! autosave journal can store). On reconnect the client says which version it has, merges what
//! the server sends back and answers with everything the server lacks in one update, so edits
//! made offline on both sides merge like any concurrent edits.

use kiln_doc::IdLanes;
use serde::{Deserialize, Serialize};

use crate::crdt::Crdt;
use crate::error::Result;
use crate::presence::Presence;
use crate::protocol::{Frame, Refusal, RoomKind};

/// Local changes the server has not received: how many, and the server's version they are
/// measured against (that of the last welcome). They are not kept separately: the replica has
/// them, and [`OfflineQueue::update`] exports them all as one update.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineQueue {
    count: usize,
    since: Vec<u8>,
}

impl OfflineQueue {
    /// Notes a local change made while offline.
    pub fn push(&mut self) {
        self.count += 1;
    }

    /// How many changes wait.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when nothing waits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Everything a server at the queue's version lacks, as one update (what the autosave
    /// journal keeps with the document).
    ///
    /// # Errors
    ///
    /// A version the CRDT cannot decode.
    pub fn update(&self, crdt: &impl Crdt) -> Result<Vec<u8>> {
        crdt.export_from(&self.since)
    }

    /// One update with everything a server at `version` lacks (the queued edits and anything
    /// else it missed), or none; the queue is emptied and measured from `version` on.
    ///
    /// # Errors
    ///
    /// A version the CRDT cannot decode.
    pub fn merge_for(&mut self, crdt: &impl Crdt, version: &[u8]) -> Result<Option<Vec<u8>>> {
        self.count = 0;
        version.clone_into(&mut self.since);
        if crdt.ahead_of(version) {
            Ok(Some(crdt.export_from(version)?))
        } else {
            Ok(None)
        }
    }

    /// The stored form.
    ///
    /// # Errors
    ///
    /// postcard failed to encode.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(postcard::to_allocvec(self)?)
    }

    /// Reads the stored form.
    ///
    /// # Errors
    ///
    /// Damaged bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(postcard::from_bytes(bytes)?)
    }
}

/// The largest update a client sends as one frame; a larger one goes as [`Frame::Part`]s, so
/// the relay never reads a large message from a connection it has not admitted.
pub const PART: usize = 256 * 1024;

/// An update as the frames that carry it.
///
/// # Errors
///
/// Encoding failed.
pub fn update_frames(update: &[u8]) -> Result<Vec<Vec<u8>>> {
    if update.len() <= PART {
        return Ok(vec![
            Frame::Update {
                bytes: update.to_vec(),
            }
            .encode()?,
        ]);
    }
    let parts: Vec<&[u8]> = update.chunks(PART).collect();
    let last = parts.len() - 1;
    parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            Frame::Part {
                bytes: p.to_vec(),
                last: i == last,
            }
            .encode()
        })
        .collect()
}

/// What a received frame means to the client.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// The server admitted the client: merge `state` (it may be empty), use `lanes` for new
    /// ids, show `peers`.
    Joined {
        /// The kind of document in the room.
        kind: RoomKind,
        /// What the client lacked.
        state: Vec<u8>,
        /// The client's id lane.
        lanes: IdLanes,
        /// Who else is there.
        peers: Vec<Presence>,
    },
    /// Another peer's update: merge it.
    Update(Vec<u8>),
    /// Another peer's presence.
    Presence(Presence),
    /// A peer left.
    Left(u64),
    /// The answer to a snapshot request.
    Snapshot(Vec<u8>),
    /// The server refused the client (and closes).
    Refused(Refusal),
}

/// One client's view of its connection to a room.
#[derive(Debug, Clone)]
pub struct SyncClient {
    room: String,
    token: String,
    peer: u64,
    kind: RoomKind,
    online: bool,
    queue: OfflineQueue,
    clock: u64,
    /// The lane of the room's first welcome, kept across reconnections.
    lane: Option<u32>,
}

impl SyncClient {
    /// A client of `room` (the share link's `doc`) with the shared `token`, offline until the
    /// first [`SyncClient::hello`] is answered.
    #[must_use]
    pub fn new(
        room: impl Into<String>,
        token: impl Into<String>,
        peer: u64,
        kind: RoomKind,
    ) -> Self {
        Self {
            room: room.into(),
            token: token.into(),
            peer,
            kind,
            online: false,
            queue: OfflineQueue::default(),
            clock: 0,
            lane: None,
        }
    }

    /// A client that keeps the queue a previous session left.
    #[must_use]
    pub fn with_queue(mut self, queue: OfflineQueue) -> Self {
        self.queue = queue;
        self
    }

    /// The first frame of a connection.
    ///
    /// # Errors
    ///
    /// Encoding failed.
    pub fn hello(&mut self, crdt: &impl Crdt) -> Result<Vec<u8>> {
        self.online = false;
        Frame::Hello {
            room: self.room.clone(),
            token: self.token.clone(),
            peer: self.peer,
            kind: self.kind,
            version: crdt.version(),
            lane: self.lane,
        }
        .encode()
    }

    /// The connection dropped: updates queue until the next welcome.
    pub fn disconnected(&mut self) {
        self.online = false;
    }

    /// Whether the server has admitted this connection.
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.online
    }

    /// The updates waiting for a connection.
    #[must_use]
    pub fn queue(&self) -> &OfflineQueue {
        &self.queue
    }

    /// A local update: the frames to send now, or none when offline (it waits in the
    /// replica, counted by the queue).
    ///
    /// # Errors
    ///
    /// Encoding failed.
    pub fn local_update(&mut self, update: &[u8]) -> Result<Vec<Vec<u8>>> {
        if self.online {
            update_frames(update)
        } else {
            self.queue.push();
            Ok(Vec::new())
        }
    }

    /// This peer's presence changed: the frame to send, or none when offline (presence is not
    /// queued; the next one after reconnecting replaces it).
    ///
    /// # Errors
    ///
    /// Encoding failed.
    pub fn presence(&mut self, mut presence: Presence) -> Result<Option<Vec<u8>>> {
        self.clock += 1;
        presence.peer = self.peer;
        presence.clock = self.clock;
        if self.online {
            Ok(Some(Frame::Awareness { presence }.encode()?))
        } else {
            Ok(None)
        }
    }

    /// A request for everything after `version` (empty: the whole snapshot).
    ///
    /// # Errors
    ///
    /// Encoding failed.
    pub fn snapshot_request(&self, version: &[u8]) -> Result<Vec<u8>> {
        Frame::SnapshotRequest {
            version: version.to_vec(),
        }
        .encode()
    }

    /// A frame from the server: what it means, and the frames to send in reply (after a
    /// welcome, what the server lacks).
    ///
    /// # Errors
    ///
    /// A frame that cannot be decoded, or a version the CRDT cannot decode.
    pub fn receive(&mut self, bytes: &[u8], crdt: &impl Crdt) -> Result<(Incoming, Vec<Vec<u8>>)> {
        let mut replies = Vec::new();
        let incoming = match Frame::decode(bytes)? {
            Frame::Welcome {
                kind,
                state,
                version,
                lane,
                lanes,
                peers,
            } => {
                self.online = true;
                self.lane = Some(lane);
                if let Some(update) = self.queue.merge_for(crdt, &version)? {
                    replies.extend(update_frames(&update)?);
                }
                Incoming::Joined {
                    kind,
                    state,
                    lanes: IdLanes::new(u64::from(lane), u64::from(lanes)),
                    peers,
                }
            }
            Frame::Refused { reason } => {
                self.online = false;
                Incoming::Refused(reason)
            }
            Frame::Update { bytes } => Incoming::Update(bytes),
            Frame::Awareness { presence } => Incoming::Presence(presence),
            Frame::Left { peer } => Incoming::Left(peer),
            Frame::Snapshot { bytes, .. } => Incoming::Snapshot(bytes),
            Frame::Hello { .. } | Frame::SnapshotRequest { .. } | Frame::Part { .. } => {
                return Err(crate::error::CollabError::Encoding(
                    "a client-to-server frame from the server".into(),
                ));
            }
        };
        Ok((incoming, replies))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::Room;
    use crate::{LoroCrdt, WriteBinding};
    use kiln_doc::{Document, Pos, edit};

    /// Delivers a client's frame to the room the way the server does.
    fn serve(room: &mut Room, frame: &[u8]) -> Result<Vec<Vec<u8>>> {
        Ok(match Frame::decode(frame)? {
            Frame::Hello {
                kind,
                version,
                lane,
                ..
            } => vec![room.hello(kind, &version, lane)?.0.encode()?],
            Frame::Update { bytes } => {
                room.update(&bytes)?;
                Vec::new()
            }
            _ => Vec::new(),
        })
    }

    fn type_text(doc: &mut Document, b: &mut WriteBinding, text: &str) -> Result<()> {
        let p = doc.body_paragraphs()[0].id;
        let end = doc.body_paragraphs()[0].content.len();
        let tx = edit::insert_text(doc, Pos::new(p, end), text)?;
        doc.apply(tx.clone())?;
        b.commit(doc, &tx)
    }

    #[test]
    fn offline_edits_merge_on_reconnect() -> Result<()> {
        let mut room = Room::new()?;
        let mut a_doc = Document::new();
        let mut a = WriteBinding::share(&mut a_doc, 1, IdLanes::default())?;
        let mut ca = SyncClient::new("r", "t", 1, RoomKind::Write);
        // A creates the room: the welcome is answered with the whole document.
        let welcome = serve(&mut room, &ca.hello(a.crdt())?)?;
        let (joined, replies) = ca.receive(&welcome[0], a.crdt())?;
        assert!(matches!(
            joined,
            Incoming::Joined {
                kind: RoomKind::Write,
                ..
            }
        ));
        assert_eq!(replies.len(), 1);
        serve(&mut room, &replies[0])?;
        // B joins from nothing.
        let mut cb = SyncClient::new("r", "t", 2, RoomKind::Write);
        let empty = LoroCrdt::new(2)?;
        let welcome = serve(&mut room, &cb.hello(&empty)?)?;
        let (Incoming::Joined { state, lanes, .. }, replies) = cb.receive(&welcome[0], &empty)?
        else {
            panic!("not admitted");
        };
        assert!(replies.is_empty());
        let (mut b, mut b_doc) = WriteBinding::join(&state, 2, lanes)?;
        assert_eq!(b_doc, a_doc);
        // Both go offline and edit.
        ca.disconnected();
        cb.disconnected();
        type_text(&mut a_doc, &mut a, "from A ")?;
        type_text(&mut b_doc, &mut b, "from B")?;
        assert!(ca.local_update(b"queued")?.is_empty());
        assert_eq!(ca.queue().len(), 1);
        // Reconnect: each merges what the room has and sends what it lacks.
        for (c, binding, doc) in [(&mut ca, &mut a, &mut a_doc), (&mut cb, &mut b, &mut b_doc)] {
            let welcome = serve(&mut room, &c.hello(binding.crdt())?)?;
            let (Incoming::Joined { state, .. }, replies) =
                c.receive(&welcome[0], binding.crdt())?
            else {
                panic!("not admitted");
            };
            if !state.is_empty() {
                binding.apply_remote(doc, &state)?;
            }
            for r in replies {
                serve(&mut room, &r)?;
            }
            assert!(c.queue().is_empty());
        }
        // A missed B's edits while B reconnected after it: one more round.
        let welcome = serve(&mut room, &ca.hello(a.crdt())?)?;
        let (Incoming::Joined { state, .. }, _) = ca.receive(&welcome[0], a.crdt())? else {
            panic!("not admitted");
        };
        a.apply_remote(&mut a_doc, &state)?;
        assert_eq!(a_doc, b_doc);
        let text = a_doc.body_paragraphs()[0].text();
        assert!(text.contains("from A") && text.contains("from B"));
        Ok(())
    }

    #[test]
    fn a_room_refuses_another_kind() -> Result<()> {
        let mut room = Room::new()?;
        room.hello(RoomKind::Deck, &[], None)?;
        let (f, _) = room.hello(RoomKind::Write, &[], None)?;
        assert_eq!(
            f,
            Frame::Refused {
                reason: Refusal::WrongKind(RoomKind::Deck)
            }
        );
        Ok(())
    }

    #[test]
    fn the_queue_survives_a_restart() -> Result<()> {
        let mut q = OfflineQueue::default();
        q.push();
        assert_eq!(OfflineQueue::from_bytes(&q.to_bytes()?)?, q);
        Ok(())
    }

    #[test]
    fn queued_edits_are_one_update_however_many_there_are() -> Result<()> {
        let mut doc = Document::new();
        let mut a = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let at_server = a.crdt().snapshot()?;
        let mut q = OfflineQueue::default();
        q.merge_for(a.crdt(), &a.crdt().version())?;
        for _ in 0..50 {
            type_text(&mut doc, &mut a, "x")?;
            q.push();
        }
        assert_eq!(q.len(), 50);
        let (mut b, mut b_doc) = WriteBinding::join(&at_server, 2, IdLanes::default())?;
        b.apply_remote(&mut b_doc, &q.update(a.crdt())?)?;
        assert_eq!(b_doc, doc);
        Ok(())
    }

    #[test]
    fn a_large_update_goes_in_parts() -> Result<()> {
        let big = vec![7u8; PART * 2 + 10];
        let frames = update_frames(&big)?;
        assert_eq!(frames.len(), 3);
        let mut joined = Vec::new();
        for (i, f) in frames.iter().enumerate() {
            assert!(f.len() < PART + 64);
            let Frame::Part { bytes, last } = Frame::decode(f)? else {
                panic!("not a part");
            };
            assert_eq!(last, i == 2);
            joined.extend(bytes);
        }
        assert_eq!(joined, big);
        assert_eq!(update_frames(b"small")?.len(), 1);
        Ok(())
    }
}
