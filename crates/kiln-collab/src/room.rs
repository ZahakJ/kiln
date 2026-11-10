// SPDX-License-Identifier: AGPL-3.0-or-later
//! A room as the relay keeps it: the merged CRDT state of one shared document, its kind and
//! the next id lane to give out. Transport, storage and authentication are the server's; this
//! is the part both the server and the tests drive.
//!
//! Every connection that does not already hold a lane of the room gets the next one, and a lane
//! is never given out twice, so two replicas never allocate the same model id
//! (`kiln_doc::IdLanes`): a reload or a new device is a new replica with a lane of its own, and
//! a replica that reconnects keeps the lane it had. Lane 0 is kept for the ids every replica
//! derives alike (the empty paragraph of a list emptied by concurrent deletions). Once all
//! lanes are given out the room admits nobody new ([`Refusal::Full`]).

use serde::{Deserialize, Serialize};

use crate::crdt::{Crdt, LoroCrdt};
use crate::error::Result;
use crate::protocol::{Frame, Refusal, RoomKind};

/// How many id lanes the id space is cut into (`kiln_doc::IdLanes`); a room gives out lanes 1
/// to `LANES - 1`.
pub const LANES: u32 = 1 << 20;

/// What a room stores besides its CRDT state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomMeta {
    /// The kind of document, set by the first client.
    pub kind: Option<RoomKind>,
    /// The lane the next new replica gets (lanes below it have been given out).
    pub next_lane: u32,
}

impl Default for RoomMeta {
    fn default() -> Self {
        Self {
            kind: None,
            next_lane: 1,
        }
    }
}

impl RoomMeta {
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

/// One shared document on the relay.
#[derive(Debug)]
pub struct Room {
    crdt: LoroCrdt,
    meta: RoomMeta,
}

impl Room {
    /// An empty room.
    ///
    /// # Errors
    ///
    /// Loro refused the relay's peer id.
    pub fn new() -> Result<Self> {
        Ok(Self {
            crdt: LoroCrdt::new(0)?,
            meta: RoomMeta::default(),
        })
    }

    /// A room from what was stored: a snapshot (or nothing), the updates after it in order, and
    /// its metadata.
    ///
    /// # Errors
    ///
    /// Stored bytes Loro cannot import.
    pub fn restore(snapshot: Option<&[u8]>, updates: &[Vec<u8>], meta: RoomMeta) -> Result<Self> {
        let mut room = Self::new()?;
        if let Some(s) = snapshot {
            room.crdt.apply(s)?;
        }
        for u in updates {
            room.crdt.apply(u)?;
        }
        room.crdt.take_changes();
        room.meta = meta;
        Ok(room)
    }

    /// The room's metadata.
    #[must_use]
    pub fn meta(&self) -> &RoomMeta {
        &self.meta
    }

    /// The room's version.
    #[must_use]
    pub fn version(&self) -> Vec<u8> {
        self.crdt.version()
    }

    /// The whole state, for storage and late joiners.
    ///
    /// # Errors
    ///
    /// Loro failed to encode.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.crdt.snapshot()
    }

    /// Admits a peer: the welcome (what it lacks, the room's version, its lane) or a refusal.
    /// `lane` is the lane the peer holds from an earlier welcome of this room, if any. Returns
    /// whether the metadata changed (a lane given out or the room's kind), to be stored.
    ///
    /// # Errors
    ///
    /// A version the CRDT cannot decode.
    pub fn hello(
        &mut self,
        kind: RoomKind,
        version: &[u8],
        lane: Option<u32>,
    ) -> Result<(Frame, bool)> {
        let mut changed = false;
        match self.meta.kind {
            Some(k) if k != kind => {
                return Ok((
                    Frame::Refused {
                        reason: Refusal::WrongKind(k),
                    },
                    false,
                ));
            }
            Some(_) => {}
            None => {
                self.meta.kind = Some(kind);
                changed = true;
            }
        }
        let lane = match lane {
            Some(l) if l >= 1 && l < self.meta.next_lane => l,
            _ if self.meta.next_lane >= LANES => {
                return Ok((
                    Frame::Refused {
                        reason: Refusal::Full,
                    },
                    changed,
                ));
            }
            _ => {
                let l = self.meta.next_lane;
                self.meta.next_lane += 1;
                changed = true;
                l
            }
        };
        let state = if self.crdt.ahead_of(version) {
            self.crdt.export_from(version)?
        } else {
            Vec::new()
        };
        let welcome = Frame::Welcome {
            kind,
            state,
            version: self.crdt.version(),
            lane,
            lanes: LANES,
            peers: Vec::new(),
        };
        Ok((welcome, changed))
    }

    /// Merges an update from a peer. An update that is not a valid one is refused and the room
    /// is unchanged.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import.
    pub fn update(&mut self, bytes: &[u8]) -> Result<()> {
        self.crdt.apply(bytes)?;
        self.crdt.take_changes();
        Ok(())
    }

    /// The answer to a snapshot request.
    ///
    /// # Errors
    ///
    /// A version the CRDT cannot decode, or a failure to encode.
    pub fn snapshot_for(&self, version: &[u8]) -> Result<Frame> {
        let bytes = if version.is_empty() {
            self.crdt.snapshot()?
        } else {
            self.crdt.export_from(version)?
        };
        Ok(Frame::Snapshot {
            bytes,
            version: self.crdt.version(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use kiln_doc::{Document, IdLanes, Pos, edit};

    use super::*;
    use crate::write::WriteBinding;

    fn lane_of(f: Frame) -> (u32, u32) {
        match f {
            Frame::Welcome { lane, lanes, .. } => (lane, lanes),
            other => panic!("not admitted: {other:?}"),
        }
    }

    fn split(doc: &mut Document, b: &mut WriteBinding, at: usize) -> Result<()> {
        let p = doc.body_paragraphs()[0].id;
        let tx = edit::split_paragraph(doc, Pos::new(p, at))?;
        doc.apply(tx.clone())?;
        b.commit(doc, &tx)
    }

    #[test]
    fn a_lane_is_never_given_out_twice() -> Result<()> {
        let mut room = Room::new()?;
        let mut a_doc = Document::new();
        let mut a = WriteBinding::share(&mut a_doc, 1, IdLanes::default())?;
        let (first, lanes) = lane_of(room.hello(RoomKind::Write, &[], None)?.0);
        a_doc.set_id_lanes(IdLanes::new(u64::from(first), u64::from(lanes)));
        room.update(&a.crdt().snapshot()?)?;
        let mut seen = HashSet::from([first]);
        for _ in 0..1_100 {
            let (lane, _) = lane_of(room.hello(RoomKind::Write, &[], None)?.0);
            assert!(
                lane != 0 && seen.insert(lane),
                "lane {lane} given out twice"
            );
        }
        // Reconnecting keeps the lane.
        let again = room.hello(RoomKind::Write, &room.version(), Some(first))?.0;
        assert_eq!(lane_of(again).0, first);
        // The 1,102nd person and the creator add a paragraph at the same moment: both stay.
        let (lane, _) = lane_of(room.hello(RoomKind::Write, &[], None)?.0);
        let (mut b, mut b_doc) = WriteBinding::join(
            &room.snapshot()?,
            2,
            IdLanes::new(u64::from(lane), u64::from(lanes)),
        )?;
        split(&mut a_doc, &mut a, 0)?;
        split(&mut b_doc, &mut b, 0)?;
        let to_b = a.crdt().export_from(&b.crdt().version())?;
        let to_a = b.crdt().export_from(&a.crdt().version())?;
        b.apply_remote(&mut b_doc, &to_b)?;
        a.apply_remote(&mut a_doc, &to_a)?;
        assert_eq!(a_doc.body_paragraphs().len(), 3);
        assert_eq!(a_doc, b_doc);
        Ok(())
    }

    #[test]
    fn a_room_out_of_lanes_admits_nobody_new() -> Result<()> {
        let meta = RoomMeta {
            kind: Some(RoomKind::Write),
            next_lane: LANES - 1,
        };
        let mut room = Room::restore(None, &[], meta)?;
        let (last, _) = lane_of(room.hello(RoomKind::Write, &[], None)?.0);
        assert_eq!(last, LANES - 1);
        assert_eq!(
            room.hello(RoomKind::Write, &[], None)?.0,
            Frame::Refused {
                reason: Refusal::Full
            }
        );
        assert_eq!(
            lane_of(room.hello(RoomKind::Write, &[], Some(last))?.0).0,
            last
        );
        Ok(())
    }
}
