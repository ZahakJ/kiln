// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two sessions sharing documents through an in-process relay that answers their socket
//! requests: share and join, concurrent typing (at the end of a paragraph the other side
//! splits), presence, undo of one's own edits, an offline stretch, and Version History
//! (preview, restore, comparison, Deck's changed slides).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use kiln_collab::Room;
use kiln_collab::protocol::Frame;
use kiln_common::{DocId, Result};
use kiln_protocol::collab::{CollabStatus, VersionChange, VersionOrigin, VersionSlideMark};
use kiln_protocol::{
    Command, DocKind, Event, HostOp, HostReply, Query, QueryResult, RequestId, SuiteCommand,
    TextPos, WriteCommand,
};

use crate::{DeferredHost, Session};

static NOW: AtomicU64 = AtomicU64::new(1_792_000_000_000);

fn clock() -> u64 {
    NOW.load(Ordering::Relaxed)
}

/// What the relay keeps of one connection.
#[derive(Default)]
struct Conn {
    inbox: VecDeque<Vec<u8>>,
    waiting: Option<RequestId>,
    closed: bool,
}

struct Side {
    session: Session,
    /// The network is up for this side.
    up: bool,
    /// Socket opens wait here instead of being answered (a slow connection).
    held: Vec<(RequestId, u32, String)>,
    hold: bool,
    conns: BTreeMap<u32, Conn>,
    timers: Vec<RequestId>,
    /// The waits the side asked for, in order.
    waits: Vec<u32>,
    /// The relay cannot store this side's updates (it refuses them for now).
    refuse_updates: bool,
    /// The relay closes this side's connections on their hello, without a welcome.
    drop_hello: bool,
    /// The files the side wrote.
    files: Vec<(String, Vec<u8>)>,
}

/// The sessions and a relay holding one room per room id.
struct Net {
    sides: Vec<Side>,
    rooms: BTreeMap<String, Room>,
    /// Which room each (side, socket) is in.
    joined: BTreeMap<(usize, u32), String>,
    /// The last presence of each connection (the relay gives a joiner the others').
    presence: BTreeMap<(usize, u32), kiln_collab::Presence>,
    /// Parts of a large update, per connection.
    parts: BTreeMap<(usize, u32), Vec<u8>>,
}

impl Net {
    fn new(n: usize) -> Self {
        Self {
            sides: (0..n)
                .map(|_| Side {
                    session: Session::new(Box::new(DeferredHost::new(clock))),
                    up: true,
                    held: Vec::new(),
                    hold: false,
                    conns: BTreeMap::new(),
                    timers: Vec::new(),
                    waits: Vec::new(),
                    refuse_updates: false,
                    drop_hello: false,
                    files: Vec::new(),
                })
                .collect(),
            rooms: BTreeMap::new(),
            joined: BTreeMap::new(),
            presence: BTreeMap::new(),
            parts: BTreeMap::new(),
        }
    }

    fn reply(&mut self, side: usize, id: RequestId, reply: HostReply) {
        self.sides[side]
            .session
            .host_reply(id, reply)
            .unwrap_or_else(|e| panic!("host reply {id:?}: {e}"));
    }

    /// Answers every host request and delivers frames until nothing moves.
    fn settle(&mut self) {
        for _ in 0..200 {
            let mut moved = false;
            for side in 0..self.sides.len() {
                let events = self.sides[side].session.poll();
                for e in events {
                    if let Event::HostRequest(r) = e {
                        moved = true;
                        self.serve(side, r.id, r.op);
                    }
                }
                // Frames waiting for a receive.
                let ready: Vec<(u32, RequestId, HostReply)> = self.sides[side]
                    .conns
                    .iter_mut()
                    .filter_map(|(s, c)| {
                        let w = c.waiting?;
                        if let Some(f) = c.inbox.pop_front() {
                            c.waiting = None;
                            Some((*s, w, HostReply::Bytes(f)))
                        } else if c.closed {
                            c.waiting = None;
                            Some((*s, w, HostReply::Empty))
                        } else {
                            None
                        }
                    })
                    .collect();
                for (_, id, reply) in ready {
                    moved = true;
                    self.reply(side, id, reply);
                }
            }
            if !moved {
                return;
            }
        }
        panic!("the sessions never settled");
    }

    fn serve(&mut self, side: usize, id: RequestId, op: HostOp) {
        match op {
            HostOp::SocketOpen { socket, url } if self.sides[side].hold => {
                self.sides[side].held.push((id, socket, url));
            }
            HostOp::SocketOpen { socket, url } => {
                if self.sides[side].up {
                    let room = url.rsplit('/').next().unwrap_or_default().to_owned();
                    self.joined.insert((side, socket), room);
                    self.sides[side].conns.insert(socket, Conn::default());
                    self.reply(side, id, HostReply::Done);
                } else {
                    let e = kiln_common::KilnError::Io("offline".into());
                    self.reply(side, id, HostReply::Failed((&e).into()));
                }
            }
            HostOp::SocketSend { socket, bytes } => {
                self.relay(side, socket, &bytes);
                self.reply(side, id, HostReply::Done);
            }
            HostOp::SocketReceive { socket } => {
                let c = self.sides[side].conns.entry(socket).or_default();
                match c.inbox.pop_front() {
                    Some(f) => self.reply(side, id, HostReply::Bytes(f)),
                    None if c.closed => self.reply(side, id, HostReply::Empty),
                    None => c.waiting = Some(id),
                }
            }
            HostOp::SocketClose { socket } => {
                if let Some(c) = self.sides[side].conns.get_mut(&socket) {
                    c.closed = true;
                }
                self.joined.remove(&(side, socket));
                self.reply(side, id, HostReply::Done);
            }
            HostOp::WriteFile { path, bytes } => {
                self.sides[side].files.push((path, bytes));
                self.reply(side, id, HostReply::Done);
            }
            HostOp::Timer { ms } => {
                self.sides[side].timers.push(id);
                self.sides[side].waits.push(ms);
            }
            _ => self.reply(side, id, HostReply::Done),
        }
    }

    /// A frame from a client: the room answers it or passes it to the others.
    fn relay(&mut self, side: usize, socket: u32, bytes: &[u8]) {
        let Some(room_id) = self.joined.get(&(side, socket)).cloned() else {
            return;
        };
        let room = self
            .rooms
            .entry(room_id.clone())
            .or_insert_with(|| Room::new().unwrap_or_else(|e| panic!("{e}")));
        let mut to_self = Vec::new();
        let mut to_others = Vec::new();
        let mut frame = Frame::decode(bytes).unwrap_or_else(|e| panic!("{e}"));
        if let Frame::Part { bytes: part, last } = frame {
            let parts = self.parts.entry((side, socket)).or_default();
            parts.extend(part);
            if !last {
                return;
            }
            frame = Frame::Update {
                bytes: std::mem::take(parts),
            };
        }
        let refuse = match &frame {
            Frame::Hello { .. } => self.sides[side].drop_hello.then_some(None),
            Frame::Update { .. } if self.sides[side].refuse_updates => {
                Some(Some(kiln_collab::protocol::Refusal::Unavailable(
                    "the change could not be stored".into(),
                )))
            }
            _ => None,
        };
        if let Some(reason) = refuse {
            if let Some(c) = self.sides[side].conns.get_mut(&socket) {
                if let Some(reason) = reason {
                    c.inbox
                        .push_back(Frame::Refused { reason }.encode().unwrap_or_default());
                }
                c.closed = true;
            }
            self.joined.remove(&(side, socket));
            return;
        }
        match frame {
            Frame::Hello {
                kind,
                version,
                lane,
                ..
            } => {
                let (mut welcome, _) = room
                    .hello(kind, &version, lane)
                    .unwrap_or_else(|e| panic!("{e}"));
                if let Frame::Welcome { peers, .. } = &mut welcome {
                    peers.extend(
                        self.presence
                            .iter()
                            .filter(|(k, _)| {
                                self.joined.get(k) == Some(&room_id) && **k != (side, socket)
                            })
                            .map(|(_, p)| p.clone()),
                    );
                }
                to_self.push(welcome.encode().unwrap_or_default());
            }
            Frame::Update { bytes: u } => {
                room.update(&u).unwrap_or_else(|e| panic!("{e}"));
                to_others.push(Frame::Update { bytes: u }.encode().unwrap_or_default());
            }
            Frame::Awareness { presence } => {
                self.presence.insert((side, socket), presence);
                to_others.push(bytes.to_vec());
            }
            Frame::SnapshotRequest { version } => {
                let f = room
                    .snapshot_for(&version)
                    .unwrap_or_else(|e| panic!("{e}"));
                to_self.push(f.encode().unwrap_or_default());
            }
            other => panic!("a server frame from a client: {other:?}"),
        }
        if let Some(c) = self.sides[side].conns.get_mut(&socket) {
            c.inbox.extend(to_self);
        }
        let others: Vec<(usize, u32)> = self
            .joined
            .iter()
            .filter(|((s, k), r)| **r == room_id && !(*s == side && *k == socket))
            .map(|(k, _)| *k)
            .collect();
        for (s, k) in others {
            if let Some(c) = self.sides[s].conns.get_mut(&k) {
                c.inbox.extend(to_others.iter().cloned());
            }
        }
    }

    /// The network of a side goes down: its connections close.
    fn cut(&mut self, side: usize) {
        self.sides[side].up = false;
        let sockets: Vec<u32> = self.sides[side].conns.keys().copied().collect();
        for s in sockets {
            if let Some(c) = self.sides[side].conns.get_mut(&s) {
                c.closed = true;
            }
            self.joined.remove(&(side, s));
        }
        self.settle();
    }

    /// Fires a side's timers (reconnecting, ticks).
    fn tick(&mut self, side: usize) {
        let timers = std::mem::take(&mut self.sides[side].timers);
        for id in timers {
            self.reply(side, id, HostReply::Done);
        }
        self.settle();
    }

    fn s(&mut self, side: usize) -> &mut Session {
        &mut self.sides[side].session
    }
}

fn exec(s: &mut Session, doc: DocId, cmd: impl Into<Command>) -> Result<()> {
    s.execute(Some(doc), cmd.into()).map(|_| ())
}

fn type_text(s: &mut Session, doc: DocId, text: &str) -> Result<()> {
    exec(
        s,
        doc,
        WriteCommand::InsertText {
            text: text.to_owned(),
        },
    )
}

fn caret(s: &mut Session, doc: DocId, para: kiln_common::NodeId, offset: u32) -> Result<()> {
    exec(
        s,
        doc,
        WriteCommand::SelectTo {
            pos: TextPos {
                block: para,
                offset,
            },
            extend: false,
        },
    )
}

fn texts(s: &Session, doc: DocId) -> Vec<String> {
    match s.query(Some(doc), &Query::Paragraphs) {
        Ok(QueryResult::Paragraphs(p)) => p.into_iter().map(|p| p.text).collect(),
        other => panic!("no paragraphs: {other:?}"),
    }
}

fn paras(s: &Session, doc: DocId) -> Vec<kiln_common::NodeId> {
    match s.query(Some(doc), &Query::Paragraphs) {
        Ok(QueryResult::Paragraphs(p)) => p.into_iter().map(|p| p.id).collect(),
        other => panic!("no paragraphs: {other:?}"),
    }
}

fn state(s: &mut Session, doc: DocId) -> kiln_protocol::collab::CollabState {
    match s.query_mut(Some(doc), &Query::Collab) {
        Ok(QueryResult::Collab(c)) => c,
        other => panic!("no collaboration state: {other:?}"),
    }
}

/// A shares a Write document with "Hello world"; B joins it.
fn shared_write(net: &mut Net) -> Result<(DocId, DocId)> {
    let a = net.s(0).new_document(DocKind::Write)?.doc;
    type_text(net.s(0), a, "Hello world")?;
    net.s(0).execute(
        Some(a),
        SuiteCommand::Share {
            server: "ws://relay".into(),
            token: "t".into(),
            name: "Ana Martín".into(),
        }
        .into(),
    )?;
    net.settle();
    let link = state(net.s(0), a).link.unwrap_or_else(|| panic!("no link"));
    let b = net
        .s(1)
        .execute(
            None,
            SuiteCommand::JoinShared {
                kind: DocKind::Write,
                server: link.server,
                room: link.room,
                token: link.token,
                name: "Rana Okafor".into(),
            }
            .into(),
        )?
        .summary
        .unwrap_or_else(|| panic!("no summary"))
        .doc;
    net.settle();
    Ok((a, b))
}

#[test]
fn a_joiner_gets_the_document_and_concurrent_typing_converges() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    assert_eq!(texts(net.s(1), b), ["Hello world"]);
    assert_eq!(state(net.s(1), b).status, CollabStatus::Online);
    // The joiner sees who is there, the sharer sees the joiner.
    assert_eq!(state(net.s(1), b).peers.len(), 1);
    assert_eq!(state(net.s(0), a).peers.len(), 1);
    assert!(!net.s(1).summary(b)?.read_only);
    // A types at the end of the paragraph while B splits it before "world".
    let p = paras(net.s(0), a)[0];
    caret(net.s(0), a, p, 11)?;
    caret(net.s(1), b, p, 6)?;
    type_text(net.s(0), a, "!")?;
    type_text(net.s(1), b, "\n")?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Hello ", "world!"]);
    assert_eq!(texts(net.s(1), b), ["Hello ", "world!"]);
    Ok(())
}

#[test]
fn people_see_each_other_and_undo_only_their_own_edits() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    let p = paras(net.s(0), a)[0];
    caret(net.s(1), b, p, 5)?;
    net.settle();
    let peers = state(net.s(0), a).peers;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].name, "Rana Okafor");
    assert_eq!(peers[0].initials, "RO");
    assert_eq!(peers[0].caret.map(|c| c.offset), Some(5));
    type_text(net.s(1), b, ",")?;
    net.settle();
    caret(net.s(0), a, p, 12)?;
    type_text(net.s(0), a, " again")?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Hello, world again"]);
    assert!(net.s(0).summary(a)?.can_undo);
    net.s(0).execute(Some(a), SuiteCommand::Undo.into())?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Hello, world"]);
    assert_eq!(texts(net.s(1), b), ["Hello, world"]);
    Ok(())
}

#[test]
fn edits_made_offline_merge_when_the_connection_returns() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    let p = paras(net.s(0), a)[0];
    net.cut(1);
    assert_eq!(state(net.s(1), b).status, CollabStatus::Offline);
    caret(net.s(1), b, p, 0)?;
    type_text(net.s(1), b, "Offline. ")?;
    caret(net.s(0), a, p, 11)?;
    type_text(net.s(0), a, " again")?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Hello world again"]);
    assert!(state(net.s(1), b).queued > 0, "the offline edit waits");
    net.sides[1].up = true;
    net.tick(1);
    assert_eq!(state(net.s(1), b).status, CollabStatus::Online);
    assert_eq!(texts(net.s(0), a), ["Offline. Hello world again"]);
    assert_eq!(texts(net.s(1), b), ["Offline. Hello world again"]);
    Ok(())
}

#[test]
fn a_version_previews_restores_and_compares() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    let p = paras(net.s(1), b)[0];
    caret(net.s(1), b, p, 5)?;
    type_text(net.s(1), b, " big")?;
    net.settle();
    let versions = match net.s(1).query_mut(Some(b), &Query::Versions)? {
        QueryResult::Versions(v) => v,
        other => panic!("{other:?}"),
    };
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].origin, VersionOrigin::Shared);
    let v = versions[0].id.clone();
    // A name everyone sees.
    net.s(1).execute(
        Some(b),
        SuiteCommand::VersionRename {
            id: v.clone(),
            name: "As shared".into(),
        }
        .into(),
    )?;
    net.settle();
    let QueryResult::Versions(named) = net.s(0).query_mut(Some(a), &Query::Versions)? else {
        panic!("no versions");
    };
    assert_eq!(named[0].name.as_deref(), Some("As shared"));
    // The comparison marks the added word.
    let QueryResult::VersionCompare(diff) = net
        .s(1)
        .query_mut(Some(b), &Query::VersionCompare { id: v.clone() })?
    else {
        panic!("no comparison");
    };
    assert!(
        diff.iter()
            .flat_map(|d| &d.spans)
            .any(|s| s.change == VersionChange::Added && s.text.contains("big"))
    );
    // Preview: the version, read-only; the document comes back after.
    net.s(1).execute(
        Some(b),
        SuiteCommand::VersionPreview {
            id: Some(v.clone()),
        }
        .into(),
    )?;
    assert_eq!(texts(net.s(1), b), ["Hello world"]);
    assert!(net.s(1).summary(b)?.read_only);
    assert!(
        type_text(net.s(1), b, "x").is_err(),
        "a version takes no edits"
    );
    // Others' edits still arrive while a version is shown.
    let pa = paras(net.s(0), a)[0];
    caret(net.s(0), a, pa, 15)?;
    type_text(net.s(0), a, "!")?;
    net.settle();
    net.s(1)
        .execute(Some(b), SuiteCommand::VersionPreview { id: None }.into())?;
    assert_eq!(texts(net.s(1), b), ["Hello big world!"]);
    assert!(!net.s(1).summary(b)?.read_only);
    // Restore: everyone has the version again, and the restore is a version itself.
    net.s(1)
        .execute(Some(b), SuiteCommand::VersionRestore { id: v }.into())?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Hello world"]);
    assert_eq!(texts(net.s(1), b), ["Hello world"]);
    let QueryResult::Versions(after) = net.s(0).query_mut(Some(a), &Query::Versions)? else {
        panic!("no versions");
    };
    assert!(
        after
            .iter()
            .any(|v| matches!(v.origin, VersionOrigin::Restore(_)))
    );
    Ok(())
}

#[test]
fn stop_sharing_leaves_an_ordinary_document() -> Result<()> {
    let mut net = Net::new(2);
    let (a, _) = shared_write(&mut net)?;
    net.s(0)
        .execute(Some(a), SuiteCommand::StopSharing.into())?;
    net.settle();
    assert!(!state(net.s(0), a).shared);
    type_text(net.s(0), a, "Mine. ")?;
    net.s(0).execute(Some(a), SuiteCommand::Undo.into())?;
    assert_eq!(texts(net.s(0), a), ["Hello world"]);
    Ok(())
}

#[test]
fn a_deck_version_marks_the_slides_that_changed() -> Result<()> {
    let mut net = Net::new(2);
    let a = net.s(0).new_document(DocKind::Deck)?.doc;
    net.s(0).execute(
        Some(a),
        SuiteCommand::Share {
            server: "ws://relay".into(),
            token: "t".into(),
            name: "Ana".into(),
        }
        .into(),
    )?;
    net.settle();
    let link = state(net.s(0), a).link.unwrap_or_else(|| panic!("no link"));
    let b = net
        .s(1)
        .execute(
            None,
            SuiteCommand::JoinShared {
                kind: DocKind::Deck,
                server: link.server,
                room: link.room,
                token: link.token,
                name: "Rana".into(),
            }
            .into(),
        )?
        .summary
        .unwrap_or_else(|| panic!("no summary"))
        .doc;
    net.settle();
    let slides = |s: &mut Session, d: DocId| match s.query_mut(
        Some(d),
        &Query::Deck(kiln_protocol::commands::DeckQuery::State),
    ) {
        Ok(QueryResult::Deck(kiln_protocol::commands::DeckQueryResult::State(st))) => {
            st.slides.len()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(slides(net.s(1), b), slides(net.s(0), a));
    let before = slides(net.s(0), a);
    net.s(1).execute(
        Some(b),
        Command::Deck(kiln_protocol::DeckCommand::NewSlide { layout: None }),
    )?;
    net.settle();
    assert_eq!(slides(net.s(0), a), before + 1);
    // Saving takes a version; its slides are marked against the one before.
    NOW.fetch_add(60_000, Ordering::Relaxed);
    net.s(0).collab_saved(a);
    net.settle();
    let QueryResult::Versions(v) = net.s(0).query_mut(Some(a), &Query::Versions)? else {
        panic!("no versions");
    };
    let last = v.last().map(|v| v.id.clone()).unwrap_or_default();
    let QueryResult::VersionSlides(vs) = net.s(0).query_mut(
        Some(a),
        &Query::VersionSlides {
            id: last,
            width: 120.0,
            dpr: 1.0,
        },
    )?
    else {
        panic!("no slides");
    };
    assert_eq!(vs.slides.len(), before + 1);
    let added = vs
        .slides
        .iter()
        .filter(|s| s.mark == VersionSlideMark::Added)
        .count();
    assert_eq!(added, 1, "{:?}", vs.slides);
    assert!(vs.slides.iter().any(|s| s.mark == VersionSlideMark::Same));
    assert!(!vs.scene.tiles.is_empty(), "the thumbnails are drawn");
    Ok(())
}

#[test]
fn a_slow_connection_is_not_opened_twice_and_a_stale_one_is_closed() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    net.cut(1);
    net.sides[1].up = true;
    net.sides[1].hold = true;
    // The reconnection timer fires while the connection is still opening: no second one.
    net.tick(1);
    net.tick(1);
    assert_eq!(net.sides[1].held.len(), 1, "one connection is opening");
    // It opens: B is back, and an edit made meanwhile reaches A.
    net.sides[1].hold = false;
    let (id, socket, url) = net.sides[1].held.remove(0);
    net.serve(1, id, HostOp::SocketOpen { socket, url });
    net.settle();
    assert_eq!(state(net.s(1), b).status, CollabStatus::Online);
    let p = paras(net.s(1), b)[0];
    caret(net.s(1), b, p, 0)?;
    type_text(net.s(1), b, "Back. ")?;
    net.settle();
    assert_eq!(texts(net.s(0), a), ["Back. Hello world"]);
    Ok(())
}

#[test]
fn a_change_the_relay_could_not_store_is_sent_again() -> Result<()> {
    let mut net = Net::new(2);
    let (a, b) = shared_write(&mut net)?;
    net.sides[1].refuse_updates = true;
    let p = paras(net.s(1), b)[0];
    caret(net.s(1), b, p, 0)?;
    type_text(net.s(1), b, "Kept. ")?;
    net.settle();
    let st = state(net.s(1), b);
    assert_ne!(
        st.status,
        CollabStatus::Refused,
        "a storage failure is not a refusal"
    );
    assert!(st.problem.is_some());
    assert_eq!(texts(net.s(0), a), ["Hello world"]);
    // The relay can store again: the next attempt sends the change.
    net.sides[1].refuse_updates = false;
    net.tick(1);
    assert_eq!(state(net.s(1), b).status, CollabStatus::Online);
    assert_eq!(texts(net.s(0), a), ["Kept. Hello world"]);
    Ok(())
}

#[test]
fn reconnecting_waits_longer_until_the_relay_welcomes_again() -> Result<()> {
    let mut net = Net::new(2);
    let (_, b) = shared_write(&mut net)?;
    // The connection opens every time but the relay drops it before a welcome.
    net.sides[1].drop_hello = true;
    net.cut(1);
    net.sides[1].up = true;
    net.sides[1].waits.clear();
    for _ in 0..4 {
        net.tick(1);
    }
    let waits = net.sides[1].waits.clone();
    assert!(
        waits.windows(2).all(|w| w[1] >= w[0]) && waits.last() > waits.first(),
        "the waits grow: {waits:?}"
    );
    // A welcome resets the wait.
    net.sides[1].drop_hello = false;
    net.tick(1);
    assert_eq!(state(net.s(1), b).status, CollabStatus::Online);
    net.cut(1);
    net.sides[1].up = true;
    net.sides[1].waits.clear();
    net.tick(1);
    assert_eq!(
        net.sides[1].waits.first(),
        Some(&500),
        "{:?}",
        net.sides[1].waits
    );
    Ok(())
}

#[test]
fn saving_while_a_version_is_shown_saves_the_document() -> Result<()> {
    let mut net = Net::new(2);
    let (a, _) = shared_write(&mut net)?;
    let p = paras(net.s(0), a)[0];
    caret(net.s(0), a, p, 11)?;
    type_text(net.s(0), a, " and more")?;
    net.settle();
    let QueryResult::Versions(versions) = net.s(0).query_mut(Some(a), &Query::Versions)? else {
        panic!("no versions");
    };
    net.s(0).execute(
        Some(a),
        SuiteCommand::VersionPreview {
            id: Some(versions[0].id.clone()),
        }
        .into(),
    )?;
    net.settle();
    assert_eq!(
        texts(net.s(0), a),
        ["Hello world"],
        "the version is on show"
    );
    assert!(
        type_text(net.s(0), a, "no").is_err(),
        "a version takes no edits"
    );
    net.s(0).execute(
        Some(a),
        SuiteCommand::SaveAs {
            path: Some("report.txt".into()),
            format: Some(kiln_protocol::FileFormat::Txt),
        }
        .into(),
    )?;
    net.settle();
    let (_, bytes) = net.sides[0].files.last().cloned().unwrap_or_default();
    assert!(
        String::from_utf8_lossy(&bytes).contains("Hello world and more"),
        "the file is the document, not the version"
    );
    assert_eq!(
        state(net.s(0), a).preview,
        None,
        "saving shows the document again"
    );
    Ok(())
}
