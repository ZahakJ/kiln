// SPDX-License-Identifier: AGPL-3.0-or-later
//! A shared presentation: the transactions the editor applies go to the collaboration
//! engine, others' edits come back as transactions (the stage and the selection follow them),
//! Undo and Redo are the engine's, the others' selected shapes and text carets are drawn on the
//! stage, and a version of Version History can be shown read-only in place of the presentation,
//! its slides as thumbnails with the changed ones marked.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::{Owner, Presentation, TextTarget, Transaction};
use kiln_deck_layout::geom::rect_px;
use kiln_deck_layout::select::unflipped_matrix;
use kiln_doc::Pos;
use kiln_protocol::DocKind;
use kiln_protocol::collab::engine::CollabChange;
use kiln_protocol::collab::{CollabCaret, CollabPeer, VersionSlide};
use kiln_protocol::geom::Rect;
use kiln_protocol::scene::SceneDelta;
use kiln_scene::{DrawOp, FillRule, Paint, PathData};

use super::DeckDoc;
use super::view::{quad, stroke};
use crate::collab::{Collaborative, Whereabouts};

/// What a shared presentation keeps.
#[derive(Debug, Default)]
pub(crate) struct SharedDeck {
    /// Transactions applied since the session last took them (postcard).
    applied: Vec<Vec<u8>>,
    /// The others.
    peers: Vec<CollabPeer>,
    can_undo: bool,
    can_redo: bool,
    /// The presentation while a version is shown in its place, and the slide shown then.
    live: Option<(Box<Presentation>, usize)>,
}

fn err(e: impl std::fmt::Display) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// `RRGGBB` as straight-alpha `RRGGBBAA`.
fn rgba(hex: &str, alpha: u8) -> Paint {
    let rgb = u32::from_str_radix(hex, 16).unwrap_or(0x3159C6);
    Paint::Rgba((rgb << 8) | u32::from(alpha))
}

/// Where a text position goes through one op of a body's text transaction.
fn map_text(doc: &kiln_doc::Document, op: &kiln_doc::Op, p: Pos) -> Pos {
    use kiln_doc::Op;
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

impl DeckDoc {
    /// Records a transaction the editor applied, for the engine.
    pub(crate) fn record_shared(&mut self, tx: &Transaction) {
        // the autosave journal takes every transaction too.
        self.record_journal(tx);
        if let Some(s) = &mut self.shared
            && s.live.is_none()
            && let Ok(bytes) = tx.to_bytes()
        {
            s.applied.push(bytes);
        }
    }

    /// The presentation itself, also while a version is shown in its place.
    pub(crate) fn live_presentation(&self) -> &Presentation {
        self.shared
            .as_ref()
            .and_then(|s| s.live.as_ref().map(|(p, _)| p.as_ref()))
            .unwrap_or(&self.pres)
    }

    /// Whether the presentation is shared, and the engine's undo state.
    pub(crate) fn shared_undo(&self) -> Option<(bool, bool)> {
        self.shared.as_ref().map(|s| (s.can_undo, s.can_redo))
    }

    /// Applies others' ops one at a time: the text being edited follows them, the stage stays
    /// on its slide.
    fn apply_remote(&mut self, tx: Transaction) -> Result<()> {
        let current = self.pres.slides.get(self.view.current).map(|s| s.id);
        let selected: Vec<NodeId> = self
            .view
            .slides
            .iter()
            .filter_map(|i| self.pres.slides.get(*i).map(|s| s.id))
            .collect();
        for op in tx.ops {
            if let (
                kiln_deck::Op::EditText {
                    target, tx: inner, ..
                },
                Some(t),
            ) = (&op, &mut self.view.text)
                && *target == t.target
                && let Ok(body) = kiln_deck::edit::text::text_body(&self.pres, *target)
            {
                let mut doc = body.doc().clone();
                for o in &inner.ops {
                    t.anchor = map_text(&doc, o, t.anchor);
                    t.head = map_text(&doc, o, t.head);
                    let _ = doc.apply(kiln_doc::Transaction::new(vec![o.clone()]));
                }
            }
            let one = Transaction::new(vec![op]);
            self.pres.apply(one.clone()).map_err(err)?;
            self.touched.record(&self.pres, &one);
            self.note_change(&one);
        }
        if let Some(i) = current.and_then(|id| self.pres.slide_index(id)) {
            self.view.current = i;
            self.view.slides = selected
                .iter()
                .filter_map(|id| self.pres.slide_index(*id))
                .collect();
        }
        self.validate_view();
        Ok(())
    }

    /// After the whole presentation changed: every slide redraws.
    fn after_replace(&mut self) {
        self.epoch += 1;
        self.global_version += 1;
        self.renderer.get_mut().invalidate();
        self.validate_view();
    }

    /// The others' selected shapes and text carets on the slide shown (slide px).
    pub(crate) fn shared_overlay(&self, z: f32) -> Vec<DrawOp> {
        let Some(s) = &self.shared else {
            return Vec::new();
        };
        if s.live.is_some() {
            return Vec::new();
        }
        let Some(slide) = self.pres.slides.get(self.view.current).map(|s| s.id) else {
            return Vec::new();
        };
        let mut ops = Vec::new();
        for p in &s.peers {
            let paint = rgba(&p.colour, 0xFF);
            if p.slide == Some(slide) {
                for id in &p.shapes {
                    if self.pres.slide_of_shape(*id).map(|s| s.id) != Some(slide) {
                        continue;
                    }
                    let Some(x) = self.xfrm_of(*id) else { continue };
                    let r = rect_px(&x);
                    let m = unflipped_matrix(&x);
                    let corner = |fx: f32, fy: f32| {
                        let pt = m * kurbo::Point::new(f64::from(fx * r.w), f64::from(fy * r.h));
                        #[allow(clippy::cast_possible_truncation)]
                        (pt.x as f32, pt.y as f32)
                    };
                    ops.push(DrawOp::Path {
                        path: quad(&[
                            corner(0.0, 0.0),
                            corner(1.0, 0.0),
                            corner(1.0, 1.0),
                            corner(0.0, 1.0),
                        ]),
                        fill: None,
                        stroke: Some(stroke(paint, 2.0 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                }
            }
            if let Some((a, b)) = p.caret.as_ref().and_then(|c| self.caret_line(c, slide)) {
                let mut path = PathData::new();
                path.move_to(a.0, a.1).line_to(b.0, b.1);
                ops.push(DrawOp::Path {
                    path,
                    fill: None,
                    stroke: Some(stroke(paint, 2.0 / z, Vec::new())),
                    rule: FillRule::NonZero,
                });
            }
        }
        ops
    }

    /// The ends of a caret in a shape's text on `slide` (slide px).
    fn caret_line(&self, c: &CollabCaret, slide: NodeId) -> Option<((f32, f32), (f32, f32))> {
        let shape = c.shape?;
        if self.pres.slide_of_shape(shape).map(|s| s.id) != Some(slide) {
            return None;
        }
        let (layout, m) = self.text_geometry(TextTarget::Shape(shape))?;
        let (cx, top, h) = layout.caret(c.para, c.offset as usize)?;
        let map = |x: f32, y: f32| {
            let p = m * kurbo::Point::new(f64::from(x), f64::from(y));
            #[allow(clippy::cast_possible_truncation)]
            (p.x as f32, p.y as f32)
        };
        Some((map(cx, top), map(cx, top + h)))
    }
}

impl Collaborative for DeckDoc {
    fn version_marks(
        &self,
        before: Option<&[u8]>,
        version: &[u8],
    ) -> Result<Vec<kiln_protocol::collab::VersionSlide>> {
        super::storage::marks(before, version)
    }

    fn collab_kind(&self) -> DocKind {
        DocKind::Deck
    }

    fn model_bytes(&self) -> Result<Vec<u8>> {
        let pres = self
            .shared
            .as_ref()
            .and_then(|s| s.live.as_ref().map(|(p, _)| p.as_ref()))
            .unwrap_or(&self.pres);
        pres.to_bytes().map_err(err)
    }

    fn set_shared(&mut self, shared: bool) {
        // The editor's own history does not know the others' edits: it starts afresh both
        // ways (while shared, Undo is the engine's).
        self.undo = kiln_deck::UndoStack::new(1_000);
        self.done.clear();
        self.undone.clear();
        if shared {
            self.shared.get_or_insert_with(Box::default);
        } else if let Some(mut s) = self.shared.take()
            && let Some((live, current)) = s.live.take()
        {
            self.pres = *live;
            self.view.current = current;
            self.after_replace();
        }
    }

    fn take_applied(&mut self) -> Vec<Vec<u8>> {
        self.shared
            .as_mut()
            .map(|s| std::mem::take(&mut s.applied))
            .unwrap_or_default()
    }

    fn apply_change(&mut self, change: &CollabChange, own: bool) -> Result<()> {
        if let Some((live, _)) = self.shared.as_mut().and_then(|s| s.live.as_mut()) {
            match change {
                CollabChange::None => {}
                CollabChange::Apply(bytes) => {
                    live.apply(Transaction::from_bytes(bytes).map_err(err)?)
                        .map_err(err)?;
                }
                CollabChange::Replace(bytes) => {
                    let lanes = live.id_lanes();
                    **live = Presentation::from_bytes(bytes).map_err(err)?;
                    live.set_id_lanes(lanes);
                }
            }
            return Ok(());
        }
        self.cancel_drag();
        match change {
            CollabChange::None => {}
            CollabChange::Apply(bytes) => {
                let tx = Transaction::from_bytes(bytes).map_err(err)?;
                if own {
                    self.pres.apply(tx.clone()).map_err(err)?;
                    self.after_history(&tx);
                } else {
                    self.apply_remote(tx)?;
                }
            }
            CollabChange::Replace(bytes) => {
                let lanes = self.pres.id_lanes();
                let current = self.pres.slides.get(self.view.current).map(|s| s.id);
                self.pres = Presentation::from_bytes(bytes).map_err(err)?;
                self.pres.set_id_lanes(lanes);
                if let Some(i) = current.and_then(|id| self.pres.slide_index(id)) {
                    self.view.current = i;
                }
                self.view.text = None;
                self.touched.everything = true;
                self.after_replace();
            }
        }
        Ok(())
    }

    fn set_lanes(&mut self, lane: u64, lanes: u64) {
        let l = kiln_doc::IdLanes::new(lane, lanes);
        self.pres.set_id_lanes(l);
        if let Some((live, _)) = self.shared.as_mut().and_then(|s| s.live.as_mut()) {
            live.set_id_lanes(l);
        }
    }

    fn whereabouts(&self) -> Whereabouts {
        let at = |shape: NodeId, p: Pos| CollabCaret {
            shape: Some(shape),
            para: p.para,
            offset: u32::try_from(p.offset).unwrap_or(u32::MAX),
        };
        let text = self.view.text.as_ref().and_then(|t| match t.target {
            TextTarget::Shape(s) => Some((s, t)),
            _ => None,
        });
        Whereabouts {
            caret: text.map(|(s, t)| at(s, t.head)),
            anchor: text
                .filter(|(_, t)| t.anchor != t.head)
                .map(|(s, t)| at(s, t.anchor)),
            slide: self.pres.slides.get(self.view.current).map(|s| s.id.0),
            shapes: self.view.shapes.iter().map(|id| id.0).collect(),
        }
    }

    fn set_peers(&mut self, peers: &[CollabPeer]) {
        if let Some(s) = &mut self.shared {
            s.peers = peers.to_vec();
        }
        self.epoch += 1;
    }

    fn locate(&mut self, peer: &CollabPeer) -> Option<Rect> {
        let slide = self.pres.slides.get(self.view.current)?.id;
        if let Some((a, b)) = peer.caret.as_ref().and_then(|c| self.caret_line(c, slide)) {
            return Some(Rect::new(
                a.0.min(b.0),
                a.1.min(b.1),
                (a.0 - b.0).abs().max(1.0),
                (a.1 - b.1).abs(),
            ));
        }
        if peer.slide != Some(slide) {
            return None;
        }
        let id = peer.shapes.first()?;
        let x = self.xfrm_of(*id)?;
        let r = rect_px(&x);
        Some(Rect::new(r.x, r.y, r.w, r.h))
    }

    fn set_undo_state(&mut self, can_undo: bool, can_redo: bool) {
        if let Some(s) = &mut self.shared {
            s.can_undo = can_undo;
            s.can_redo = can_redo;
        }
    }

    fn preview(&mut self, model: Option<&[u8]>) -> Result<()> {
        self.cancel_drag();
        let Some(s) = &mut self.shared else {
            return Err(KilnError::InvalidInput(
                "the presentation is not shared".into(),
            ));
        };
        match model {
            Some(bytes) => {
                let version = Presentation::from_bytes(bytes).map_err(err)?;
                let live = std::mem::replace(&mut self.pres, version);
                if s.live.is_none() {
                    s.live = Some((Box::new(live), self.view.current));
                }
            }
            None => {
                let Some((live, current)) = s.live.take() else {
                    return Ok(());
                };
                self.pres = *live;
                self.view.current = current;
            }
        }
        self.view.text = None;
        self.view.shapes.clear();
        self.after_replace();
        Ok(())
    }

    fn version_slides(
        &mut self,
        model: &[u8],
        slides: &[VersionSlide],
        width: f32,
        dpr: f32,
    ) -> Result<SceneDelta> {
        let pres = Presentation::from_bytes(model).map_err(err)?;
        let owners: Vec<(u32, Owner)> = slides
            .iter()
            .enumerate()
            .filter(|(_, s)| pres.slide(s.slide).is_some())
            .map(|(i, s)| (u32::try_from(i).unwrap_or(u32::MAX), Owner::Slide(s.slide)))
            .collect();
        // Its own renderer: the version's pictures are not the presentation's.
        let mut renderer = kiln_deck_layout::Renderer::new(super::fonts(), super::fonts());
        Ok(kiln_deck_layout::scene::thumbnails(
            &mut renderer,
            &pres,
            &owners,
            width.clamp(16.0, 800.0),
            dpr,
            self.epoch,
        ))
    }
}
