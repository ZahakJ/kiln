// SPDX-License-Identifier: AGPL-3.0-or-later
//! A shared Write (or Notes) document: the transactions the editor applies go to the
//! collaboration engine, others' edits come back as transactions and the selection follows them,
//! Undo and Redo are the engine's, others' carets and selections are drawn in the overlay, and a
//! version of Version History can be shown read-only in place of the document.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Document, Op, Pos, Transaction};
use kiln_protocol::DocKind;
use kiln_protocol::collab::engine::CollabChange;
use kiln_protocol::collab::{CollabCaret, CollabPeer};
use kiln_protocol::geom::Rect;
use kiln_scene::{Bounds, DrawOp, Paint};

use super::WriteDoc;
use crate::collab::{Collaborative, Whereabouts};

/// What a shared document keeps.
#[derive(Debug, Default)]
pub(super) struct SharedDoc {
    /// Transactions applied since the session last took them (postcard).
    applied: Vec<Vec<u8>>,
    /// The others.
    peers: Vec<CollabPeer>,
    can_undo: bool,
    can_redo: bool,
    /// The document while a version is shown in its place.
    live: Option<Box<Document>>,
    /// The selection while a version is shown.
    live_selection: Option<(Pos, Pos)>,
}

fn doc_err(e: impl std::fmt::Display) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// `RRGGBB` as straight-alpha `RRGGBBAA`.
fn rgba(hex: &str, alpha: u8) -> Paint {
    let rgb = u32::from_str_radix(hex, 16).unwrap_or(0x3159C6);
    Paint::Rgba((rgb << 8) | u32::from(alpha))
}

/// Where a position goes when `op` is applied to `doc` (the document before it).
fn map_pos(doc: &Document, op: &Op, p: Pos) -> Pos {
    match op {
        Op::InsertInline {
            para,
            offset,
            fragment,
        } if *para == p.para && *offset < p.offset => Pos::new(p.para, p.offset + fragment.len()),
        Op::DeleteInline { para, offset, len } if *para == p.para && *offset < p.offset => {
            Pos::new(p.para, p.offset.saturating_sub(*len).max(*offset))
        }
        Op::SplitParagraph {
            para,
            offset,
            new_para,
            ..
        } if *para == p.para && *offset <= p.offset && p.offset > 0 => {
            Pos::new(*new_para, p.offset - offset)
        }
        Op::MergeParagraphs { first, second } if *second == p.para => {
            let at = doc.paragraph(*first).map_or(0, |f| f.content.len());
            Pos::new(*first, at + p.offset)
        }
        _ => p,
    }
}

impl WriteDoc {
    /// Records a transaction the editor applied, for the engine.
    pub(super) fn record_shared(&mut self, tx: &Transaction) {
        // the autosave journal takes every transaction too.
        self.record_journal(tx);
        if let Some(s) = &mut self.shared
            && s.live.is_none()
            && let Ok(bytes) = tx.to_bytes()
        {
            s.applied.push(bytes);
        }
    }

    /// The document itself, also while a version is shown in its place.
    pub(super) fn live_document(&self) -> &Document {
        self.shared
            .as_ref()
            .and_then(|s| s.live.as_deref())
            .unwrap_or(&self.doc)
    }

    /// Whether the document is shared, and the engine's undo state.
    pub(super) fn shared_undo(&self) -> Option<(bool, bool)> {
        self.shared.as_ref().map(|s| (s.can_undo, s.can_redo))
    }

    /// A position kept inside the document: its paragraph (or the first one) and length.
    fn clamp(&self, p: Pos) -> Pos {
        match self.doc.paragraph(p.para) {
            Some(para) => Pos::new(p.para, p.offset.min(para.content.len())),
            None => {
                let first = self
                    .doc
                    .body_paragraphs()
                    .first()
                    .map_or(NodeId(0), |p| p.id);
                Pos::new(first, 0)
            }
        }
    }

    /// Applies others' ops one at a time, the selection (and an IME composition, which the
    /// engine has too) following each.
    fn apply_remote(&mut self, tx: Transaction) -> Result<()> {
        let (mut anchor, mut head) = (self.anchor, self.head);
        let mut composing = self.composition.as_ref().map(|c| c.at);
        for op in tx.ops {
            anchor = map_pos(&self.doc, &op, anchor);
            head = map_pos(&self.doc, &op, head);
            composing = composing.map(|at| map_pos(&self.doc, &op, at));
            let one = Transaction::new(vec![op]);
            self.doc.apply(one.clone()).map_err(doc_err)?;
            self.touched.record(&self.doc, &one);
            self.note_edit(&one);
        }
        self.anchor = self.clamp(anchor);
        self.head = self.clamp(head);
        if let (Some(c), Some(at)) = (&mut self.composition, composing) {
            c.at = at;
            c.inverse = Transaction::new(vec![Op::DeleteInline {
                para: at.para,
                offset: at.offset,
                len: c.len,
            }])
            .with_kind(kiln_doc::EditKind::Other);
        }
        Ok(())
    }

    /// Others' carets and selections, for the overlay.
    pub(super) fn shared_overlay(&self) -> Vec<DrawOp> {
        let (Some(s), Some(v)) = (&self.shared, &self.view) else {
            return Vec::new();
        };
        if s.live.is_some() {
            return Vec::new();
        }
        let mut ops = Vec::new();
        let pos = |c: &CollabCaret| {
            self.doc
                .paragraph(c.para)
                .map(|p| Pos::new(c.para, (c.offset as usize).min(p.content.len())))
        };
        for p in &s.peers {
            let Some(head) = p.caret.as_ref().and_then(pos) else {
                continue;
            };
            if let Some(anchor) = p.anchor.as_ref().and_then(pos)
                && anchor != head
            {
                let (a, b) = match self.doc.compare(anchor, head) {
                    Some(std::cmp::Ordering::Greater) => (head, anchor),
                    _ => (anchor, head),
                };
                ops.push(DrawOp::SelectionRects {
                    rects: kiln_doc_layout::geometry::selection_rects(&v.layout, &self.doc, a, b),
                    paint: rgba(&p.colour, 0x40),
                });
            }
            if let Some(c) = v.caret(head) {
                let r = c.rect;
                let x = r.x + r.w / 2.0 - 1.0;
                ops.push(DrawOp::FillRect {
                    rect: Bounds::new(x, r.y, 2.0, r.h),
                    paint: rgba(&p.colour, 0xFF),
                    radius: 0.0,
                });
                ops.push(DrawOp::FillRect {
                    rect: Bounds::new(x - 2.0, r.y - 2.0, 6.0, 4.0),
                    paint: rgba(&p.colour, 0xFF),
                    radius: 1.0,
                });
            }
        }
        ops
    }
}

impl Collaborative for WriteDoc {
    fn version_compare(&self, old: &[u8]) -> Result<Vec<kiln_protocol::collab::VersionParagraph>> {
        super::storage::compare(old, self.live_document())
    }

    fn collab_kind(&self) -> DocKind {
        crate::model::DocModel::kind(self)
    }

    fn model_bytes(&self) -> Result<Vec<u8>> {
        let doc = self
            .shared
            .as_ref()
            .and_then(|s| s.live.as_deref())
            .unwrap_or(&self.doc);
        doc.to_bytes().map_err(doc_err)
    }

    fn set_shared(&mut self, shared: bool) {
        // The editor's own history does not know the others' edits: it starts afresh both
        // ways (while shared, Undo is the engine's).
        self.history = kiln_doc::UndoStack::default();
        if shared {
            self.shared.get_or_insert_with(Box::default);
        } else if let Some(mut s) = self.shared.take()
            && let Some(live) = s.live.take()
        {
            self.doc = *live;
            self.view = None;
        }
    }

    fn take_applied(&mut self) -> Vec<Vec<u8>> {
        self.shared
            .as_mut()
            .map(|s| std::mem::take(&mut s.applied))
            .unwrap_or_default()
    }

    fn apply_change(&mut self, change: &CollabChange, own: bool) -> Result<()> {
        // While a version is shown, edits go to the document behind it.
        if let Some(live) = self.shared.as_mut().and_then(|s| s.live.as_mut()) {
            match change {
                CollabChange::None => {}
                CollabChange::Apply(bytes) => {
                    let tx = Transaction::from_bytes(bytes).map_err(doc_err)?;
                    live.apply(tx).map_err(doc_err)?;
                }
                CollabChange::Replace(bytes) => {
                    let lanes = live.id_lanes();
                    **live = Document::from_bytes(bytes).map_err(doc_err)?;
                    live.set_id_lanes(lanes);
                }
            }
            return Ok(());
        }
        match change {
            CollabChange::None => {}
            CollabChange::Apply(bytes) => {
                let tx = Transaction::from_bytes(bytes).map_err(doc_err)?;
                let after = tx.selection_after;
                self.apply_remote(tx)?;
                if own && let Some(r) = after {
                    self.anchor = self.clamp(r.anchor);
                    self.head = self.clamp(r.head);
                }
            }
            CollabChange::Replace(bytes) => {
                let lanes = self.doc.id_lanes();
                self.doc = Document::from_bytes(bytes).map_err(doc_err)?;
                self.doc.set_id_lanes(lanes);
                // The engine's model has no composition: it is gone with the old document.
                self.composition = None;
                self.touched.everything = true;
                self.view = None;
                self.anchor = self.clamp(self.anchor);
                self.head = self.clamp(self.head);
            }
        }
        self.goal_x = None;
        self.features.generation += 1;
        Ok(())
    }

    fn set_lanes(&mut self, lane: u64, lanes: u64) {
        let l = kiln_doc::IdLanes::new(lane, lanes);
        self.doc.set_id_lanes(l);
        if let Some(live) = self.shared.as_mut().and_then(|s| s.live.as_mut()) {
            live.set_id_lanes(l);
        }
    }

    fn whereabouts(&self) -> Whereabouts {
        let at = |p: Pos| CollabCaret {
            shape: None,
            para: p.para,
            offset: u32::try_from(p.offset).unwrap_or(u32::MAX),
        };
        Whereabouts {
            caret: Some(at(self.head)),
            anchor: (self.anchor != self.head).then(|| at(self.anchor)),
            slide: None,
            shapes: Vec::new(),
        }
    }

    fn set_peers(&mut self, peers: &[CollabPeer]) {
        if let Some(s) = &mut self.shared {
            s.peers = peers.to_vec();
        }
    }

    fn locate(&mut self, peer: &CollabPeer) -> Option<Rect> {
        let c = peer.caret?;
        let para = self.doc.paragraph(c.para)?;
        let pos = Pos::new(c.para, (c.offset as usize).min(para.content.len()));
        let v = self.view.as_ref()?;
        let r = v.caret(pos)?.rect;
        Some(Rect::new(r.x, r.y, r.w, r.h))
    }

    fn set_undo_state(&mut self, can_undo: bool, can_redo: bool) {
        if let Some(s) = &mut self.shared {
            s.can_undo = can_undo;
            s.can_redo = can_redo;
        }
    }

    fn preview(&mut self, model: Option<&[u8]>) -> Result<()> {
        self.end_composition();
        let Some(s) = &mut self.shared else {
            return Err(KilnError::InvalidInput("the document is not shared".into()));
        };
        match model {
            Some(bytes) => {
                let version = Document::from_bytes(bytes).map_err(doc_err)?;
                let live = std::mem::replace(&mut self.doc, version);
                if s.live.is_none() {
                    s.live = Some(Box::new(live));
                    s.live_selection = Some((self.anchor, self.head));
                }
                let first = self
                    .doc
                    .body_paragraphs()
                    .first()
                    .map_or(NodeId(0), |p| p.id);
                self.anchor = Pos::new(first, 0);
                self.head = self.anchor;
            }
            None => {
                let Some(live) = s.live.take() else {
                    return Ok(());
                };
                self.doc = *live;
                if let Some((a, h)) = s.live_selection.take() {
                    self.anchor = self.clamp(a);
                    self.head = self.clamp(h);
                }
            }
        }
        self.view = None;
        self.goal_x = None;
        self.features.generation += 1;
        Ok(())
    }
}
