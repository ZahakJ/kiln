// SPDX-License-Identifier: AGPL-3.0-or-later
//! One shared document's collaboration engine, behind the calls of
//! [`kiln_protocol::collab::engine`]: the [`Replica`], the [`SyncClient`] with its offline queue,
//! the other people's presence and Version History.
//!
//! The core holds the document; it hands the engine each transaction it applies and applies the
//! [`CollabChange`]s the engine returns. Natively the session calls [`Engine::call`] directly;
//! on the web the collaboration module (`kiln-collab-wasm`) does, with the calls and replies in
//! their postcard form, so both run the same code.

use std::sync::{Arc, Mutex, PoisonError};

use kiln_common::NodeId;
use kiln_doc::{IdLanes, Pos};
use kiln_protocol::DocKind;
use kiln_protocol::collab::engine::{
    CollabChange, EngineCall, EngineReply, EngineRequest, EngineValue,
};
use kiln_protocol::collab::{
    CollabCaret, CollabPeer, VersionChange, VersionEntry, VersionOrigin, VersionParagraph,
    VersionSlide, VersionSlideMark, VersionSpan,
};
use loro::Subscription;

use crate::client::{Incoming, SyncClient};
use crate::crdt::{Crdt, LoroCrdt};
use crate::diff::{Change, word_diff};
use crate::error::{CollabError, Result};
use crate::history::{Origin, SlideMark, VersionHistory};
use crate::presence::{Anchor, Peers, Presence, colour_for};
use crate::protocol::RoomKind;
use crate::replica::{CoreChange, Replica};

/// The CRDT before the server's welcome brought a joiner its document, or the replica.
enum State {
    Joining {
        crdt: LoroCrdt,
        /// The lane of the welcome, while the document is still on its way.
        lanes: Option<IdLanes>,
        /// Updates that arrived before the document.
        early: Vec<Vec<u8>>,
    },
    Ready {
        replica: Box<Replica>,
        _updates: Subscription,
    },
}

/// One shared document.
pub struct Engine {
    kind: RoomKind,
    peer: u64,
    name: String,
    state: State,
    client: SyncClient,
    peers: Peers,
    history: VersionHistory,
    outbox: Arc<Mutex<Vec<Vec<u8>>>>,
    /// The others changed since the last reply.
    peers_changed: bool,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("kind", &self.kind)
            .field("peer", &self.peer)
            .field("online", &self.client.is_online())
            .finish_non_exhaustive()
    }
}

fn room_kind(kind: DocKind) -> Result<RoomKind> {
    match kind {
        DocKind::Write => Ok(RoomKind::Write),
        DocKind::Deck => Ok(RoomKind::Deck),
        DocKind::Notes => Ok(RoomKind::Notes),
        other => Err(CollabError::Encoding(format!(
            "{other:?} documents cannot be shared"
        ))),
    }
}

fn core_change(c: CoreChange) -> CollabChange {
    match c {
        CoreChange::None => CollabChange::None,
        CoreChange::Apply(b) => CollabChange::Apply(b),
        CoreChange::Replace(b) => CollabChange::Replace(b),
    }
}

/// Initials for an avatar: the first letters of the first and last words (two letters of a
/// single word).
#[must_use]
pub fn initials(name: &str) -> String {
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

/// A colour as `RRGGBB`.
#[must_use]
pub fn colour_hex(colour: u32) -> String {
    format!("{:06X}", colour & 0xFF_FFFF)
}

impl Engine {
    /// Creates the engine of a `Share` or `Join` call.
    ///
    /// # Errors
    ///
    /// Another call, a kind that cannot be shared, or a model that is not of that kind.
    pub fn create(call: EngineCall) -> Result<Self> {
        match call {
            EngineCall::Share {
                kind,
                model,
                room,
                token,
                peer,
                name,
                now_ms,
            } => {
                let k = room_kind(kind)?;
                let replica = Replica::share(k, &model, peer, IdLanes::default())?;
                let mut e = Self::new(k, peer, name, &room, &token, Self::joining(peer)?);
                e.ready(replica);
                if let State::Ready { replica, .. } = &e.state {
                    e.history.on_share(replica.crdt(), &e.name, now_ms)?;
                }
                // The version is part of what the server gets on connecting.
                e.outbox
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clear();
                Ok(e)
            }
            EngineCall::Join {
                kind,
                room,
                token,
                peer,
                name,
            } => {
                let k = room_kind(kind)?;
                Ok(Self::new(
                    k,
                    peer,
                    name,
                    &room,
                    &token,
                    Self::joining(peer)?,
                ))
            }
            _ => Err(CollabError::Encoding(
                "an engine is created by Share or Join".into(),
            )),
        }
    }

    fn joining(peer: u64) -> Result<State> {
        Ok(State::Joining {
            crdt: LoroCrdt::new(peer)?,
            lanes: None,
            early: Vec::new(),
        })
    }

    fn new(kind: RoomKind, peer: u64, name: String, room: &str, token: &str, state: State) -> Self {
        Self {
            kind,
            peer,
            name,
            state,
            client: SyncClient::new(room, token, peer, kind),
            peers: Peers::default(),
            history: VersionHistory::default(),
            outbox: Arc::default(),
            peers_changed: false,
        }
    }

    fn ready(&mut self, replica: Replica) {
        let sink = Arc::clone(&self.outbox);
        let updates = replica.crdt().subscribe(Box::new(move |u| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(u.to_vec());
        }));
        self.state = State::Ready {
            replica: Box::new(replica),
            _updates: updates,
        };
    }

    fn crdt(&self) -> &LoroCrdt {
        match &self.state {
            State::Joining { crdt, .. } => crdt,
            State::Ready { replica, .. } => replica.crdt(),
        }
    }

    fn replica(&self) -> Result<&Replica> {
        match &self.state {
            State::Ready { replica, .. } => Ok(replica),
            State::Joining { .. } => Err(not_arrived()),
        }
    }

    fn replica_mut(&mut self) -> Result<&mut Replica> {
        match &mut self.state {
            State::Ready { replica, .. } => Ok(replica),
            State::Joining { .. } => Err(not_arrived()),
        }
    }

    /// Answers one call.
    ///
    /// # Errors
    ///
    /// A frame, update or transaction that cannot be used, an unknown version, or a call that
    /// needs the document before it arrived.
    pub fn call(&mut self, call: EngineCall) -> Result<EngineReply> {
        let mut reply = EngineReply::default();
        match call {
            EngineCall::Share { .. } | EngineCall::Join { .. } | EngineCall::Close => {
                return Err(CollabError::Encoding(
                    "Share, Join and Close are calls of the engines".into(),
                ));
            }
            EngineCall::Hello => {
                let hello = match &self.state {
                    State::Joining { crdt, .. } => self.client.hello(crdt)?,
                    State::Ready { replica, .. } => self.client.hello(replica.crdt())?,
                };
                reply.frames.push(hello);
            }
            EngineCall::Disconnected => self.client.disconnected(),
            EngineCall::Commit { tx, now_ms } => {
                self.replica_mut()?.commit(&tx)?;
                self.history.note_edit(now_ms);
            }
            EngineCall::Receive { frame, now_ms } => self.receive(&frame, now_ms, &mut reply)?,
            EngineCall::Undo => reply.change = core_change(self.replica_mut()?.undo()?),
            EngineCall::Redo => reply.change = core_change(self.replica_mut()?.redo()?),
            EngineCall::BreakUndoGroup => {
                if let State::Ready { replica, .. } = &mut self.state {
                    replica.break_undo_group();
                }
            }
            EngineCall::Presence {
                caret,
                anchor,
                slide,
                shapes,
            } => {
                let mut p = Presence::new(self.peer, self.name.clone());
                if let State::Ready { replica, .. } = &self.state {
                    p.caret = caret.and_then(|c| anchor_of(replica, c));
                    p.anchor = anchor.and_then(|c| anchor_of(replica, c));
                }
                p.slide = slide.map(NodeId);
                p.shapes = shapes.into_iter().map(NodeId).collect();
                reply.frames.extend(self.client.presence(p)?);
            }
            EngineCall::Tick { now_ms } => {
                if !self.peers.expire(now_ms).is_empty() {
                    self.peers_changed = true;
                }
                match &self.state {
                    State::Ready { replica, .. } => {
                        let id = self.history.tick(replica.crdt(), &self.name, now_ms)?;
                        reply.value = EngineValue::Version(id);
                    }
                    // A room whose creator had not sent the document yet answered with nothing:
                    // ask again.
                    State::Joining { .. } if self.client.is_online() => {
                        reply.frames.push(self.client.snapshot_request(&[])?);
                    }
                    State::Joining { .. } => {}
                }
            }
            EngineCall::Saved { now_ms } => {
                let State::Ready { replica, .. } = &self.state else {
                    return Err(not_arrived());
                };
                let id = self.history.on_save(replica.crdt(), &self.name, now_ms)?;
                reply.value = EngineValue::Version(id);
            }
            EngineCall::Versions => {
                reply.value = EngineValue::Versions(
                    VersionHistory::list(self.crdt())
                        .into_iter()
                        .map(|v| VersionEntry {
                            id: v.id,
                            name: v.name,
                            author: v.author,
                            time_ms: v.time_ms,
                            origin: match v.origin {
                                Origin::Save => VersionOrigin::Save,
                                Origin::Auto => VersionOrigin::Auto,
                                Origin::Shared => VersionOrigin::Shared,
                                Origin::Restore(id) => VersionOrigin::Restore(id),
                            },
                        })
                        .collect(),
                );
            }
            EngineCall::RenameVersion { id, name } => {
                VersionHistory::rename(self.replica()?.crdt(), &id, Some(&name))?;
            }
            EngineCall::Checkout { id } => {
                reply.value = EngineValue::Model(self.replica()?.checkout(&id)?);
            }
            EngineCall::Restore { id, now_ms } => {
                let State::Ready { replica, .. } = &mut self.state else {
                    return Err(not_arrived());
                };
                let change = replica.restore(&mut self.history, &id, &self.name, now_ms)?;
                reply.change = core_change(change);
            }
            EngineCall::Compare { id } => {
                let Replica::Write { binding, doc } = self.replica()? else {
                    return Err(CollabError::Encoding(
                        "a comparison is for Write documents".into(),
                    ));
                };
                let old = VersionHistory::checkout_write(binding.crdt(), &id)?;
                reply.value = EngineValue::Compare(
                    word_diff(&old, doc)
                        .into_iter()
                        .map(|p| VersionParagraph {
                            change: version_change(p.change),
                            spans: p
                                .spans
                                .into_iter()
                                .map(|s| VersionSpan {
                                    change: version_change(s.change),
                                    text: s.text,
                                })
                                .collect(),
                        })
                        .collect(),
                );
            }
            EngineCall::SlideChanges { id } => {
                let changes = VersionHistory::deck_changes(self.replica()?.crdt(), &id)?;
                reply.value = EngineValue::Slides(
                    changes
                        .into_iter()
                        .map(|c| VersionSlide {
                            slide: c.slide,
                            mark: match c.mark {
                                SlideMark::Same => VersionSlideMark::Same,
                                SlideMark::Added => VersionSlideMark::Added,
                                SlideMark::Changed => VersionSlideMark::Changed,
                            },
                        })
                        .collect(),
                );
            }
            EngineCall::State => self.peers_changed = true,
        }
        self.finish(&mut reply)?;
        Ok(reply)
    }

    fn receive(&mut self, frame: &[u8], now_ms: u64, reply: &mut EngineReply) -> Result<()> {
        let crdt = match &self.state {
            State::Joining { crdt, .. } => crdt,
            State::Ready { replica, .. } => replica.crdt(),
        };
        let (incoming, replies) = self.client.receive(frame, crdt)?;
        reply.frames.extend(replies);
        match incoming {
            Incoming::Joined {
                state,
                lanes,
                peers,
                ..
            } => {
                for p in peers {
                    self.peers.update(p, now_ms);
                }
                self.peers_changed = true;
                reply.lanes = Some((lanes.lane, lanes.lanes));
                match &mut self.state {
                    State::Joining { lanes: l, .. } => {
                        *l = Some(lanes);
                        if state.is_empty() {
                            reply.frames.push(self.client.snapshot_request(&[])?);
                        } else {
                            self.arrive(&state, reply)?;
                        }
                    }
                    State::Ready { replica, .. } => {
                        replica.set_id_lanes(lanes);
                        if !state.is_empty() {
                            reply.change = core_change(replica.apply_remote(&state)?);
                            self.history.note_edit(now_ms);
                        }
                    }
                }
            }
            Incoming::Update(bytes) => match &mut self.state {
                State::Joining { early, .. } => early.push(bytes),
                State::Ready { replica, .. } => {
                    reply.change = core_change(replica.apply_remote(&bytes)?);
                    self.history.note_edit(now_ms);
                }
            },
            Incoming::Snapshot(bytes) => match &mut self.state {
                State::Joining { .. } if bytes.is_empty() => {}
                State::Joining { .. } => self.arrive(&bytes, reply)?,
                State::Ready { replica, .. } => {
                    reply.change = core_change(replica.apply_remote(&bytes)?);
                }
            },
            Incoming::Presence(p) => {
                if p.peer != self.peer && self.peers.update(p, now_ms) {
                    self.peers_changed = true;
                }
            }
            Incoming::Left(peer) => {
                if self.peers.remove(peer) {
                    self.peers_changed = true;
                }
            }
            Incoming::Refused(r @ crate::protocol::Refusal::Unavailable(_)) => {
                reply.unavailable = Some(r.to_string());
            }
            Incoming::Refused(r) => reply.refused = Some(r.to_string()),
        }
        Ok(())
    }

    /// The joiner's document arrived.
    fn arrive(&mut self, state: &[u8], reply: &mut EngineReply) -> Result<()> {
        let State::Joining { lanes, early, .. } = &mut self.state else {
            return Ok(());
        };
        let lanes = lanes.unwrap_or_default();
        let early = std::mem::take(early);
        let mut replica = Replica::join(self.kind, state, self.peer, lanes)?;
        for u in &early {
            replica.apply_remote(u)?;
        }
        reply.change = CollabChange::Replace(replica.model()?);
        reply.joined = true;
        self.ready(replica);
        Ok(())
    }

    fn finish(&mut self, reply: &mut EngineReply) -> Result<()> {
        let updates =
            std::mem::take(&mut *self.outbox.lock().unwrap_or_else(PoisonError::into_inner));
        for u in updates {
            reply.frames.extend(self.client.local_update(&u)?);
        }
        reply.online = self.client.is_online();
        reply.queued = u32::try_from(self.client.queue().len()).unwrap_or(u32::MAX);
        reply.can_undo = self.crdt().can_undo();
        reply.can_redo = self.crdt().can_redo();
        reply.colour = colour_for(self.peer);
        if std::mem::take(&mut self.peers_changed) {
            reply.peers = Some(self.peer_list());
        }
        Ok(())
    }

    /// The others, with their carets resolved in this replica's document.
    fn peer_list(&self) -> Vec<CollabPeer> {
        let replica = match &self.state {
            State::Ready { replica, .. } => Some(replica.as_ref()),
            State::Joining { .. } => None,
        };
        let resolve = |a: &Anchor| replica.and_then(|r| resolve_in(r, a));
        self.peers
            .iter()
            .filter(|p| p.peer != self.peer)
            .map(|p| CollabPeer {
                peer: p.peer.to_string(),
                name: p.name.clone(),
                initials: initials(&p.name),
                colour: colour_hex(p.colour),
                caret: p.caret.as_ref().and_then(resolve),
                anchor: p.anchor.as_ref().and_then(resolve),
                slide: p.slide,
                shapes: p.shapes.clone(),
                at: None,
            })
            .collect()
    }
}

fn not_arrived() -> CollabError {
    CollabError::Encoding("the shared document has not arrived yet".into())
}

fn anchor_of(replica: &Replica, c: CollabCaret) -> Option<Anchor> {
    let pos = Pos::new(c.para, c.offset as usize);
    match (replica, c.shape) {
        (Replica::Write { binding, .. }, _) => binding.anchor(pos),
        (Replica::Deck { binding, .. }, Some(shape)) => binding.anchor(shape, pos),
        (Replica::Deck { .. }, None) => None,
    }
}

fn resolve_in(replica: &Replica, a: &Anchor) -> Option<CollabCaret> {
    let (shape, pos) = match replica {
        Replica::Write { binding, doc } => (None, binding.resolve(doc, a)?),
        Replica::Deck { binding, pres } => {
            let (shape, pos) = binding.resolve(pres, a)?;
            (Some(shape), pos)
        }
    };
    Some(CollabCaret {
        shape,
        para: pos.para,
        offset: u32::try_from(pos.offset).unwrap_or(u32::MAX),
    })
}

fn version_change(c: Change) -> VersionChange {
    match c {
        Change::Same => VersionChange::Same,
        Change::Added => VersionChange::Added,
        Change::Removed => VersionChange::Removed,
    }
}

/// The engines of a session's shared documents, by handle: what the session calls natively, and
/// what the web's collaboration module serves.
#[derive(Debug, Default)]
pub struct Engines {
    engines: std::collections::BTreeMap<u32, Engine>,
    next: u32,
}

impl Engines {
    /// Answers a request: `Share` and `Join` create an engine (its handle is the reply's
    /// `Created` value), `Close` forgets one, the rest go to the engine of the handle.
    ///
    /// # Errors
    ///
    /// An unknown handle, or the engine's error.
    pub fn request(&mut self, request: EngineRequest) -> Result<EngineReply> {
        match request.call {
            call @ (EngineCall::Share { .. } | EngineCall::Join { .. }) => {
                let mut engine = Engine::create(call)?;
                self.next = self.next.wrapping_add(1).max(1);
                let handle = self.next;
                let mut reply = engine.call(EngineCall::State)?;
                reply.value = EngineValue::Created(handle);
                self.engines.insert(handle, engine);
                Ok(reply)
            }
            EngineCall::Close => {
                self.engines.remove(&request.handle);
                Ok(EngineReply::default())
            }
            call => self
                .engines
                .get_mut(&request.handle)
                .ok_or_else(|| CollabError::Encoding(format!("no engine {}", request.handle)))?
                .call(call),
        }
    }

    /// [`Engines::request`] in postcard: an `EngineRequest` in, a `Result<EngineReply, String>`
    /// out (the web's collaboration module).
    #[must_use]
    pub fn request_bytes(&mut self, request: &[u8]) -> Vec<u8> {
        let result: std::result::Result<EngineReply, String> =
            postcard::from_bytes::<EngineRequest>(request)
                .map_err(|e| e.to_string())
                .and_then(|r| self.request(r).map_err(|e| e.to_string()));
        postcard::to_allocvec(&result).unwrap_or_default()
    }
}
