// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two engines and their cores through an in-process relay: share and join, concurrent typing
//! (at the end of a paragraph the other side splits), presence, an offline stretch merged on
//! reconnect, and Version History restore and comparison.

use kiln_collab::protocol::Frame;
use kiln_collab::{Engine, Result, Room};
use kiln_doc::{Document, Pos, Transaction, edit};
use kiln_protocol::DocKind;
use kiln_protocol::collab::engine::{CollabChange, EngineCall, EngineReply, EngineValue};
use kiln_protocol::collab::{CollabCaret, VersionChange, VersionOrigin};

/// A replica as the session holds it: the core's document and the engine.
struct Side {
    engine: Engine,
    doc: Document,
    online: bool,
}

impl Side {
    fn call(&mut self, call: EngineCall) -> Result<EngineReply> {
        let reply = self.engine.call(call)?;
        self.apply(&reply.change)?;
        if let Some((lane, lanes)) = reply.lanes {
            self.doc.set_id_lanes(kiln_doc::IdLanes::new(lane, lanes));
        }
        Ok(reply)
    }

    fn apply(&mut self, change: &CollabChange) -> Result<()> {
        match change {
            CollabChange::None => {}
            CollabChange::Apply(tx) => {
                self.doc.apply(Transaction::from_bytes(tx)?)?;
            }
            CollabChange::Replace(model) => {
                let lanes = self.doc.id_lanes();
                self.doc = Document::from_bytes(model)?;
                self.doc.set_id_lanes(lanes);
            }
        }
        Ok(())
    }

    /// Applies a transaction the way the core does, then hands it to the engine.
    fn edit(
        &mut self,
        build: impl FnOnce(&mut Document) -> Result<Transaction, kiln_doc::DocError>,
    ) -> Result<Vec<Vec<u8>>> {
        let tx = build(&mut self.doc)?;
        self.doc.apply(tx.clone())?;
        let reply = self.call(EngineCall::Commit {
            tx: tx.to_bytes()?,
            now_ms: 1_000,
        })?;
        Ok(reply.frames)
    }

    fn texts(&self) -> Vec<String> {
        self.doc
            .body_paragraphs()
            .iter()
            .map(|p| p.text())
            .collect()
    }
}

/// The relay: one room, frames routed between the sides.
struct Relay {
    room: Room,
    /// Frames waiting for each side.
    inbox: Vec<Vec<Vec<u8>>>,
}

impl Relay {
    fn new(sides: usize) -> Result<Self> {
        Ok(Self {
            room: Room::new()?,
            inbox: vec![Vec::new(); sides],
        })
    }

    /// A side's frames reach the relay.
    fn send(&mut self, from: usize, frames: Vec<Vec<u8>>, online: &[bool]) -> Result<()> {
        let mut parts = Vec::new();
        for mut f in frames {
            if let Frame::Part { bytes, last } = Frame::decode(&f)? {
                parts.extend(bytes);
                if !last {
                    continue;
                }
                f = Frame::Update {
                    bytes: std::mem::take(&mut parts),
                }
                .encode()?;
            }
            match Frame::decode(&f)? {
                Frame::Hello {
                    kind,
                    version,
                    lane,
                    ..
                } => {
                    let (welcome, _) = self.room.hello(kind, &version, lane)?;
                    self.inbox[from].push(welcome.encode()?);
                }
                Frame::Update { .. } | Frame::Awareness { .. } => {
                    if let Frame::Update { bytes } = Frame::decode(&f)? {
                        self.room.update(&bytes)?;
                    }
                    for (i, inbox) in self.inbox.iter_mut().enumerate() {
                        if i != from && online[i] {
                            inbox.push(f.clone());
                        }
                    }
                }
                Frame::SnapshotRequest { version } => {
                    self.inbox[from].push(self.room.snapshot_for(&version)?.encode()?);
                }
                other => panic!("a server frame from a client: {other:?}"),
            }
        }
        Ok(())
    }
}

/// Delivers frames both ways until nothing moves.
fn settle(sides: &mut [Side], relay: &mut Relay) -> Result<()> {
    for _ in 0..16 {
        let mut moved = false;
        for i in 0..sides.len() {
            if !sides[i].online {
                continue;
            }
            let frames = std::mem::take(&mut relay.inbox[i]);
            for f in frames {
                moved = true;
                let reply = sides[i].call(EngineCall::Receive {
                    frame: f,
                    now_ms: 2_000,
                })?;
                let online: Vec<bool> = sides.iter().map(|s| s.online).collect();
                relay.send(i, reply.frames, &online)?;
            }
        }
        if !moved {
            break;
        }
    }
    Ok(())
}

fn connect(sides: &mut [Side], relay: &mut Relay, i: usize) -> Result<()> {
    sides[i].online = true;
    let hello = sides[i].call(EngineCall::Hello)?;
    let online: Vec<bool> = sides.iter().map(|s| s.online).collect();
    relay.send(i, hello.frames, &online)?;
    settle(sides, relay)
}

fn send(sides: &mut [Side], relay: &mut Relay, i: usize, frames: Vec<Vec<u8>>) -> Result<()> {
    let online: Vec<bool> = sides.iter().map(|s| s.online).collect();
    if online[i] {
        relay.send(i, frames, &online)?;
    }
    Ok(())
}

fn pair() -> Result<(Vec<Side>, Relay)> {
    let mut doc = Document::new();
    let p = doc.body_paragraphs()[0].id;
    let tx = edit::insert_text(&mut doc, Pos::new(p, 0), "Hello world")?;
    doc.apply(tx)?;
    let a = Engine::create(EngineCall::Share {
        kind: DocKind::Write,
        model: doc.to_bytes()?,
        room: "room".into(),
        token: "t".into(),
        peer: 11,
        name: "Ana Martín".into(),
        now_ms: 500,
    })?;
    let b = Engine::create(EngineCall::Join {
        kind: DocKind::Write,
        room: "room".into(),
        token: "t".into(),
        peer: 22,
        name: "Rana Okafor".into(),
    })?;
    let mut sides = vec![
        Side {
            engine: a,
            doc,
            online: false,
        },
        Side {
            engine: b,
            doc: Document::empty(),
            online: false,
        },
    ];
    let mut relay = Relay::new(2)?;
    connect(&mut sides, &mut relay, 0)?;
    connect(&mut sides, &mut relay, 1)?;
    Ok((sides, relay))
}

#[test]
fn a_joiner_gets_the_document_and_concurrent_typing_converges() -> Result<()> {
    let (mut sides, mut relay) = pair()?;
    assert_eq!(sides[1].texts(), ["Hello world"]);
    assert_eq!(
        sides[1].doc.id_lanes().lanes,
        u64::from(kiln_collab::room::LANES),
        "the joiner got a lane"
    );
    assert_eq!(
        sides[0].doc.id_lanes().lanes,
        u64::from(kiln_collab::room::LANES),
        "so did the creator"
    );
    assert_ne!(sides[0].doc.id_lanes().lane, sides[1].doc.id_lanes().lane);
    let p = sides[0].doc.body_paragraphs()[0].id;
    // A types at the end of the paragraph while B splits it before "world".
    let fa = sides[0].edit(|d| edit::insert_text(d, Pos::new(p, 11), "!"))?;
    let fb = sides[1].edit(|d| edit::split_paragraph(d, Pos::new(p, 6)))?;
    send(&mut sides, &mut relay, 0, fa)?;
    send(&mut sides, &mut relay, 1, fb)?;
    settle(&mut sides, &mut relay)?;
    assert_eq!(sides[0].texts(), ["Hello ", "world!"]);
    assert_eq!(sides[0].doc, sides[1].doc);
    Ok(())
}

#[test]
fn presence_reaches_the_other_side_with_its_caret() -> Result<()> {
    let (mut sides, mut relay) = pair()?;
    let p = sides[0].doc.body_paragraphs()[0].id;
    let reply = sides[0].call(EngineCall::Presence {
        caret: Some(CollabCaret {
            shape: None,
            para: p,
            offset: 5,
        }),
        anchor: None,
        slide: None,
        shapes: Vec::new(),
    })?;
    assert_eq!(reply.frames.len(), 1);
    send(&mut sides, &mut relay, 0, reply.frames)?;
    let frames = std::mem::take(&mut relay.inbox[1]);
    let mut seen = None;
    for f in frames {
        let r = sides[1].call(EngineCall::Receive {
            frame: f,
            now_ms: 3_000,
        })?;
        seen = r.peers.or(seen);
    }
    let peers = seen.unwrap_or_default();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].name, "Ana Martín");
    assert_eq!(peers[0].initials, "AM");
    assert_eq!(peers[0].caret.map(|c| c.offset), Some(5));
    // Silent for over a minute: gone.
    let r = sides[1].call(EngineCall::Tick { now_ms: 120_000 })?;
    assert_eq!(r.peers.map(|p| p.len()), Some(0));
    Ok(())
}

#[test]
fn edits_made_offline_merge_on_reconnect() -> Result<()> {
    let (mut sides, mut relay) = pair()?;
    let p = sides[0].doc.body_paragraphs()[0].id;
    sides[1].online = false;
    sides[1].call(EngineCall::Disconnected)?;
    let fb = sides[1].edit(|d| edit::insert_text(d, Pos::new(p, 0), "Offline. "))?;
    assert!(fb.is_empty(), "offline edits wait");
    let fa = sides[0].edit(|d| edit::insert_text(d, Pos::new(p, 11), " again"))?;
    send(&mut sides, &mut relay, 0, fa)?;
    settle(&mut sides, &mut relay)?;
    assert_eq!(sides[0].texts(), ["Hello world again"]);
    connect(&mut sides, &mut relay, 1)?;
    settle(&mut sides, &mut relay)?;
    assert_eq!(sides[0].texts(), ["Offline. Hello world again"]);
    assert_eq!(sides[0].doc, sides[1].doc);
    Ok(())
}

#[test]
fn a_version_restores_for_everyone_and_compares_by_word() -> Result<()> {
    let (mut sides, mut relay) = pair()?;
    let p = sides[0].doc.body_paragraphs()[0].id;
    let f = sides[1].edit(|d| edit::insert_text(d, Pos::new(p, 5), " big"))?;
    send(&mut sides, &mut relay, 1, f)?;
    settle(&mut sides, &mut relay)?;
    let EngineValue::Versions(list) = sides[1].call(EngineCall::Versions)?.value else {
        panic!("no versions");
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].origin, VersionOrigin::Shared);
    assert_eq!(list[0].author, "Ana Martín");
    let id = list[0].id.clone();
    let EngineValue::Compare(diff) = sides[1].call(EngineCall::Compare { id: id.clone() })?.value
    else {
        panic!("no comparison");
    };
    let added: String = diff
        .iter()
        .flat_map(|p| p.spans.iter())
        .filter(|s| s.change == VersionChange::Added)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(added.trim(), "big");
    let r = sides[1].call(EngineCall::Restore { id, now_ms: 9_000 })?;
    assert!(matches!(r.change, CollabChange::Replace(_)));
    send(&mut sides, &mut relay, 1, r.frames)?;
    settle(&mut sides, &mut relay)?;
    assert_eq!(sides[0].texts(), ["Hello world"]);
    assert_eq!(sides[1].texts(), ["Hello world"]);
    let EngineValue::Versions(list) = sides[0].call(EngineCall::Versions)?.value else {
        panic!("no versions");
    };
    assert!(
        list.iter()
            .any(|v| matches!(v.origin, VersionOrigin::Restore(_))),
        "the restore is a version everyone sees: {list:?}"
    );
    Ok(())
}
