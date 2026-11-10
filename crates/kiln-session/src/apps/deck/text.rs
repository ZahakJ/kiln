// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text in shapes and table cells: entering and leaving the text, the caret and selection
//! (kiln-doc positions in the body's paragraphs), typing, deleting, Enter and Shift+Enter, Tab
//! levels, caret motion through the laid-out lines, the IME composition, Promote and Demote,
//! the slide number field, and Find, Replace and Replace Fonts across slides and notes.

use std::sync::Arc;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::TextTarget;
use kiln_deck::{ShapeKind, TextBody};
use kiln_deck_layout::render::{Options, absolute_xfrm, invert};
use kiln_deck_layout::text::BodyLayout;
use kiln_doc::st::BreakKind;
use kiln_doc::{
    Break, DocError, Document, Field, Fragment, InlineObject, Pos, Range as DocRange,
    Transaction as DocTransaction,
};
use kiln_protocol::commands::{CaretMotion, DeckCommand};
use kiln_scene::Bounds;
use kurbo::Affine;

use super::DeckDoc;
use crate::model::Change;

/// The text being edited: where, the selection's ends and the IME composition.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    /// The shape (for a cell, the table).
    pub shape: NodeId,
    /// The text body.
    pub target: TextTarget,
    /// Where the selection started.
    pub anchor: Pos,
    /// The caret.
    pub head: Pos,
    /// The x a run of Up/Down keeps (flow px).
    pub goal_x: Option<f32>,
    /// The IME's composition at the caret.
    pub composition: Option<String>,
}

impl TextEdit {
    /// Whether the selection is empty.
    #[must_use]
    pub fn collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// The paragraph index of a position in a body (for ordering).
fn para_index(body: &TextBody, id: NodeId) -> usize {
    body.paragraphs().position(|p| p.id == id).unwrap_or(0)
}

/// The two ends of a selection in reading order.
pub(crate) fn ordered(body: &TextBody, a: Pos, b: Pos) -> (Pos, Pos) {
    let ka = (para_index(body, a.para), a.offset);
    let kb = (para_index(body, b.para), b.offset);
    if ka <= kb { (a, b) } else { (b, a) }
}

/// The start and end of a body.
pub(crate) fn body_ends(body: &TextBody) -> Option<(Pos, Pos)> {
    let first = body.paragraphs().next()?;
    let last = body.paragraphs().last()?;
    Some((Pos::new(first.id, 0), Pos::new(last.id, last.content.len())))
}

/// The plain text of a range of a body.
pub(crate) fn range_text(body: &TextBody, from: Pos, to: Pos) -> String {
    let (a, b) = ordered(body, from, to);
    let (ia, ib) = (para_index(body, a.para), para_index(body, b.para));
    let mut out = String::new();
    for (i, p) in body.paragraphs().enumerate() {
        if i < ia || i > ib {
            continue;
        }
        let text: Vec<char> = p.content.plain_text(true).chars().collect();
        let s = if i == ia { a.offset.min(text.len()) } else { 0 };
        let e = if i == ib {
            b.offset.min(text.len())
        } else {
            text.len()
        };
        if i > ia {
            out.push('\n');
        }
        out.extend(text.get(s..e.max(s)).unwrap_or(&[]));
    }
    out
}

impl DeckDoc {
    /// The body of a text target.
    pub(crate) fn body(&self, target: TextTarget) -> Option<&TextBody> {
        kiln_deck::edit::text::text_body(&self.pres, target).ok()
    }

    /// The options the stage draws with.
    pub(crate) fn draw_options(&self) -> Options {
        let today = {
            let days = i64::try_from(self.now_ms / 86_400_000).unwrap_or(0);
            let (y, m, d) = super::save::civil(days);
            (self.now_ms > 0).then(|| {
                (
                    i32::try_from(y).unwrap_or(1970),
                    u32::try_from(m).unwrap_or(1),
                    u32::try_from(d).unwrap_or(1),
                )
            })
        };
        Options {
            edit: true,
            editing: self.view.text.as_ref().map(|t| t.shape),
            today,
            prompts: self.prompts,
        }
    }

    /// The layout of a text target as drawn, and the matrix from its flow coordinates to slide
    /// px.
    pub(crate) fn text_geometry(&self, target: TextTarget) -> Option<(Arc<BodyLayout>, Affine)> {
        let opts = self.draw_options();
        match target {
            TextTarget::Shape(id) => {
                let layout = self
                    .renderer
                    .borrow_mut()
                    .text_layout(&self.pres, id, &opts)?;
                let xfrm = absolute_xfrm(&self.pres, id)?;
                let m = kiln_deck_layout::geom::text_matrix(&xfrm) * layout.flow_matrix();
                Some((layout, m))
            }
            TextTarget::Cell { shape, row, col } => {
                let table = self.renderer.borrow_mut().table_layout(&self.pres, shape)?;
                let cell = table.cells.iter().find(|c| c.row == row && c.col == col)?;
                let xfrm = absolute_xfrm(&self.pres, shape)?;
                let m = kiln_deck_layout::geom::text_matrix(&xfrm) * cell.text.flow_matrix();
                Some((cell.text.clone(), m))
            }
            TextTarget::Notes(_) => None,
        }
    }

    /// The text position under a slide point, for a target.
    pub(crate) fn text_hit(&self, target: TextTarget, x: f32, y: f32) -> Option<Pos> {
        let (layout, m) = self.text_geometry(target)?;
        let inv = invert(m)?;
        let q = inv * kurbo::Point::new(f64::from(x), f64::from(y));
        #[allow(clippy::cast_possible_truncation)]
        let (para, offset) = layout.hit(q.x as f32, q.y as f32)?;
        Some(Pos::new(para, offset))
    }

    /// Starts editing a shape's text (or a table cell's), with the caret at a slide point, at the
    /// end, or with all the text selected.
    pub(crate) fn begin_text(
        &mut self,
        target: TextTarget,
        at: Option<(f32, f32)>,
        select_all: bool,
    ) -> Result<()> {
        let shape = match target {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => s,
            TextTarget::Notes(s) => s,
        };
        // A geometry shape without a text body gets one.
        if let TextTarget::Shape(id) = target
            && self.body(target).is_none()
        {
            let has_room = matches!(
                self.pres.shape(id).map(|s| &s.kind),
                Some(ShapeKind::Geometry(_))
            );
            if !has_room {
                return Err(KilnError::InvalidInput("this object has no text".into()));
            }
            self.add_text_body(id)?;
        }
        let body = self
            .body(target)
            .ok_or_else(|| KilnError::InvalidInput("this object has no text".into()))?;
        let (start, end) = body_ends(body)
            .ok_or_else(|| KilnError::InvalidInput("the text has no paragraphs".into()))?;
        let mut edit = TextEdit {
            shape,
            target,
            anchor: end,
            head: end,
            goal_x: None,
            composition: None,
        };
        if select_all {
            edit.anchor = start;
        }
        self.view.text = Some(edit);
        self.view.shapes = vec![shape];
        self.view.rail = false;
        if let Some((x, y)) = at
            && let Some(p) = self.text_hit(target, x, y)
            && let Some(t) = &mut self.view.text
        {
            t.anchor = p;
            t.head = p;
        }
        self.undo.break_group();
        self.epoch += 1;
        Ok(())
    }

    /// Leaves the text (the shape stays selected).
    pub(crate) fn end_text(&mut self) {
        if self.view.text.take().is_some() {
            self.undo.break_group();
            self.epoch += 1;
        }
    }

    /// Keeps the text selection inside its body.
    pub(crate) fn clamp_text(&mut self) {
        let Some(t) = self.view.text.clone() else {
            return;
        };
        let Some(body) = self.body(t.target) else {
            self.view.text = None;
            return;
        };
        let fix = |p: Pos| -> Option<Pos> {
            match body.paragraph(p.para) {
                Some(para) => Some(Pos::new(p.para, p.offset.min(para.content.len()))),
                None => body_ends(body).map(|(_, e)| e),
            }
        };
        let (Some(a), Some(h)) = (fix(t.anchor), fix(t.head)) else {
            self.view.text = None;
            return;
        };
        if let Some(edit) = &mut self.view.text {
            edit.anchor = a;
            edit.head = h;
        }
    }

    /// Gives a geometry shape an empty text body (typing into a shape that had none).
    fn add_text_body(&mut self, id: NodeId) -> Result<()> {
        let mut body = TextBody::new();
        body.body.anchor = Some(kiln_deck::TextAnchor::Middle);
        let tx = kiln_deck::Transaction::new(vec![kiln_deck::Op::SetTextBody {
            shape: id,
            text: Some(Box::new(body)),
        }]);
        self.apply("deck.text.insert", tx).map(|_| ())
    }

    /// Runs a kiln-doc builder on the edited body and moves the selection to where the
    /// builder left it.
    pub(crate) fn edit_body(
        &mut self,
        label: &'static str,
        target: TextTarget,
        build: impl FnOnce(&mut Document) -> std::result::Result<DocTransaction, DocError>,
    ) -> Result<Option<DocRange>> {
        let mut after = None;
        let tx = kiln_deck::edit::edit_text(&self.pres, target, |doc| {
            let t = build(doc)?;
            after = t.selection_after;
            Ok(t)
        })
        .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply(label, tx)?;
        if let (Some(r), Some(edit)) = (after, self.view.text.as_mut())
            && edit.target == target
        {
            edit.anchor = r.anchor;
            edit.head = r.head;
            edit.goal_x = None;
        }
        self.after_text_edit(target);
        Ok(after)
    }

    /// After a text edit: "Resize shape to fit text" grows or shrinks the shape.
    fn after_text_edit(&mut self, target: TextTarget) {
        let TextTarget::Shape(id) = target else {
            return;
        };
        let Some(props) = self.pres.resolved_body(id) else {
            return;
        };
        if matches!(props.autofit, kiln_deck::Autofit::Shrink { .. }) {
            // Shrink text on overflow: store the scale that fits, as PowerPoint does.
            let fit = self.renderer.get_mut().fit_autofit(&self.pres, id);
            if let Some(fit) = fit
                && fit != props.autofit
                && let Ok(tx) = kiln_deck::edit::set_body_props(&self.pres, target, |b| {
                    b.autofit = Some(fit);
                })
            {
                let _ = self.apply("deck.format.autofit", tx);
            }
            return;
        }
        if props.autofit != kiln_deck::Autofit::ResizeShape {
            return;
        }
        let Some((layout, _)) = self.text_geometry(target) else {
            return;
        };
        let Some(xfrm) = absolute_xfrm(&self.pres, id) else {
            return;
        };
        let insets = kiln_deck_layout::geom::px(props.insets[1] + props.insets[3]);
        let want = layout.content_height + insets;
        let have = kiln_deck_layout::geom::px(xfrm.cy);
        if (want - have).abs() < 0.5 || layout.turn != 0 {
            return;
        }
        let mut x = xfrm;
        x.cy = kiln_deck_layout::geom::emu(f64::from(want.max(8.0)));
        if let Ok(tx) = kiln_deck::edit::set_transform(&mut self.pres, id, x) {
            let _ = self.apply("deck.format.autofit", tx);
        }
    }

    /// Deletes the selection when it is not empty; returns where the caret is then.
    fn delete_selection(&mut self, label: &'static str) -> Result<Option<Pos>> {
        let Some(t) = self.view.text.clone() else {
            return Ok(None);
        };
        if t.collapsed() {
            return Ok(Some(t.head));
        }
        let body = self
            .body(t.target)
            .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
        let (a, b) = ordered(body, t.anchor, t.head);
        self.edit_body(label, t.target, |doc| {
            kiln_doc::edit::delete_range(doc, a, b)
        })?;
        if let Some(e) = &mut self.view.text {
            e.anchor = a;
            e.head = a;
        }
        Ok(Some(a))
    }

    /// Types text at the caret (replacing the selection). Selected shapes without text editing
    /// take the typing as their new text, as in PowerPoint.
    pub(crate) fn type_text(&mut self, text: &str) -> Result<Change> {
        if text.is_empty() {
            return Ok(Change::None);
        }
        if self.view.text.is_none() {
            let Some(&id) = self.view.shapes.last() else {
                return Ok(Change::None);
            };
            if self.view.shapes.len() != 1 {
                return Ok(Change::None);
            }
            self.begin_text(TextTarget::Shape(id), None, true)?;
        }
        // Typing at a caret merges into the word being typed; replacing a selection is one
        // step of its own.
        let replacing = self.view.text.as_ref().is_some_and(|t| !t.collapsed());
        if replacing {
            self.begin_group();
        }
        let result = (|| {
            let at = self.delete_selection("deck.text.insert")?;
            let Some(t) = self.view.text.clone() else {
                return Ok(());
            };
            let at = at.unwrap_or(t.head);
            self.edit_body("deck.text.insert", t.target, |doc| {
                kiln_doc::edit::insert_text(doc, at, text)
            })?;
            Ok::<(), KilnError>(())
        })();
        if replacing {
            self.end_group();
        }
        if let Some(t) = &mut self.view.text {
            t.composition = None;
        }
        result?;
        Ok(Change::Content)
    }

    /// Inserts an inline object (a line break, a field) at the caret.
    pub(crate) fn insert_object(
        &mut self,
        label: &'static str,
        object: InlineObject,
    ) -> Result<Change> {
        let Some(t) = self.view.text.clone() else {
            return Ok(Change::None);
        };
        self.begin_group();
        let result = (|| {
            let at = self.delete_selection(label)?.unwrap_or(t.head);
            let body = self
                .body(t.target)
                .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
            let props = body
                .paragraph(at.para)
                .map(|p| kiln_doc::edit::typing_props(p, at.offset))
                .unwrap_or_default();
            self.edit_body(label, t.target, |_| {
                Ok(DocTransaction::new(vec![kiln_doc::Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: Fragment::object(object, props),
                }])
                .with_kind(kiln_doc::EditKind::Structure)
                .with_selection(
                    Some(DocRange::caret(at)),
                    Some(DocRange::caret(Pos::new(at.para, at.offset + 1))),
                ))
            })?;
            Ok::<(), KilnError>(())
        })();
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// The position one step from `p` (chars, words, lines, paragraphs).
    fn moved(&self, t: &TextEdit, motion: CaretMotion) -> Option<(Pos, Option<f32>)> {
        let p = t.head;
        // The arrows follow the paragraph's direction (logical movement, as in Write).
        let rtl = matches!(
            motion,
            CaretMotion::Left | CaretMotion::Right | CaretMotion::WordLeft | CaretMotion::WordRight
        ) && self
            .text_geometry(t.target)
            .is_some_and(|(l, _)| l.is_rtl(p.para));
        let motion = motion.logical(rtl);
        let body = self.body(t.target)?;
        let paras: Vec<&kiln_doc::Paragraph> = body.paragraphs().collect();
        let i = paras.iter().position(|q| q.id == p.para)?;
        let len = paras[i].content.len();
        let text: Vec<char> = paras[i].content.raw_text().chars().collect();
        let prev_para = |i: usize| (i > 0).then(|| paras[i - 1]);
        let next_para = |i: usize| paras.get(i + 1).copied();
        let at = |q: &kiln_doc::Paragraph, o: usize| Pos::new(q.id, o);
        let word_start = |o: usize| -> usize {
            let mut k = o;
            while k > 0 && text[k - 1].is_whitespace() {
                k -= 1;
            }
            while k > 0 && !text[k - 1].is_whitespace() {
                k -= 1;
            }
            k
        };
        let word_end = |o: usize| -> usize {
            let mut k = o;
            while k < text.len() && !text[k].is_whitespace() {
                k += 1;
            }
            while k < text.len() && text[k].is_whitespace() {
                k += 1;
            }
            k
        };
        Some(match motion {
            // Mapped to logical motions above.
            CaretMotion::Left
            | CaretMotion::Right
            | CaretMotion::WordLeft
            | CaretMotion::WordRight => {
                return None;
            }
            CaretMotion::Backward => {
                if !t.collapsed() {
                    return Some((ordered(body, t.anchor, t.head).0, None));
                }
                if p.offset > 0 {
                    (at(paras[i], p.offset - 1), None)
                } else {
                    (prev_para(i).map_or(p, |q| at(q, q.content.len())), None)
                }
            }
            CaretMotion::Forward => {
                if !t.collapsed() {
                    return Some((ordered(body, t.anchor, t.head).1, None));
                }
                if p.offset < len {
                    (at(paras[i], p.offset + 1), None)
                } else {
                    (next_para(i).map_or(p, |q| at(q, 0)), None)
                }
            }
            CaretMotion::WordBackward => {
                if p.offset == 0 {
                    (prev_para(i).map_or(p, |q| at(q, q.content.len())), None)
                } else {
                    (at(paras[i], word_start(p.offset)), None)
                }
            }
            CaretMotion::WordForward => {
                if p.offset >= len {
                    (next_para(i).map_or(p, |q| at(q, 0)), None)
                } else {
                    (at(paras[i], word_end(p.offset)), None)
                }
            }
            CaretMotion::ParagraphBackward => {
                if p.offset > 0 {
                    (at(paras[i], 0), None)
                } else {
                    (prev_para(i).map_or(p, |q| at(q, 0)), None)
                }
            }
            CaretMotion::ParagraphForward => {
                (next_para(i).map_or(at(paras[i], len), |q| at(q, 0)), None)
            }
            CaretMotion::DocumentStart | CaretMotion::PageUp => (at(paras[0], 0), None),
            CaretMotion::DocumentEnd | CaretMotion::PageDown => {
                let last = paras[paras.len() - 1];
                (at(last, last.content.len()), None)
            }
            CaretMotion::LineStart | CaretMotion::LineEnd => {
                let (layout, _) = self.text_geometry(t.target)?;
                let o = layout.line_edge(p.para, p.offset, motion == CaretMotion::LineEnd)?;
                (Pos::new(p.para, o), None)
            }
            CaretMotion::LineUp | CaretMotion::LineDown => {
                let (layout, _) = self.text_geometry(t.target)?;
                let x = t
                    .goal_x
                    .or_else(|| layout.caret(p.para, p.offset).map(|c| c.0))?;
                let down = motion == CaretMotion::LineDown;
                match layout.line_step(p.para, p.offset, down, x) {
                    Some((para, o)) => (Pos::new(para, o), Some(x)),
                    None if down => {
                        let last = paras[paras.len() - 1];
                        (at(last, last.content.len()), Some(x))
                    }
                    None => (at(paras[0], 0), Some(x)),
                }
            }
        })
    }

    /// Text commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn text_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        match cmd {
            D::EditText => {
                let Some(&id) = self.view.shapes.last() else {
                    return Ok(Change::None);
                };
                let target = match self.pres.shape(id).map(|s| &s.kind) {
                    Some(ShapeKind::Table(_)) => TextTarget::Cell {
                        shape: id,
                        row: 0,
                        col: 0,
                    },
                    _ => TextTarget::Shape(id),
                };
                self.begin_text(target, None, false)?;
                Ok(Change::Selection)
            }
            D::SelectText {
                shape,
                anchor_para,
                anchor_offset,
                head_para,
                head_offset,
            } => {
                if self.view.text.as_ref().is_none_or(|t| t.shape != *shape) {
                    self.end_text();
                    self.begin_text(TextTarget::Shape(*shape), None, false)?;
                }
                let Some(t) = self.view.text.clone() else {
                    return Ok(Change::None);
                };
                let Some(body) = self.body(t.target) else {
                    return Ok(Change::None);
                };
                let at = |para: u32, offset: u32| {
                    let paras: Vec<&kiln_doc::Paragraph> = body.paragraphs().collect();
                    let p = paras.get(para as usize).or_else(|| paras.last()).copied()?;
                    Some(Pos::new(p.id, (offset as usize).min(p.content.len())))
                };
                let (Some(anchor), Some(head)) = (
                    at(*anchor_para, *anchor_offset),
                    at(*head_para, *head_offset),
                ) else {
                    return Ok(Change::None);
                };
                if let Some(t) = &mut self.view.text {
                    t.anchor = anchor;
                    t.head = head;
                    t.goal_x = None;
                    t.composition = None;
                }
                self.epoch += 1;
                Ok(Change::Selection)
            }
            D::InsertText { text } => {
                if let Some(t) = &mut self.view.text {
                    t.composition = None;
                }
                self.type_text(text)
            }
            D::SetComposition { text } => {
                if self.view.text.is_none()
                    && !text.is_empty()
                    && let Some(&id) = self.view.shapes.last()
                {
                    self.begin_text(TextTarget::Shape(id), None, true)?;
                }
                if let Some(t) = &mut self.view.text {
                    t.composition = (!text.is_empty()).then(|| text.clone());
                    self.epoch += 1;
                }
                Ok(Change::Selection)
            }
            D::DeleteText { forward, word } => {
                let Some(t) = self.view.text.clone() else {
                    return Ok(Change::None);
                };
                if !t.collapsed() {
                    self.delete_selection("deck.text.delete")?;
                    return Ok(Change::Content);
                }
                let motion = match (forward, word) {
                    (true, true) => CaretMotion::WordForward,
                    (true, false) => CaretMotion::Forward,
                    (false, true) => CaretMotion::WordBackward,
                    (false, false) => CaretMotion::Backward,
                };
                let Some((to, _)) = self.moved(&t, motion) else {
                    return Ok(Change::None);
                };
                if to == t.head {
                    return Ok(Change::None);
                }
                // Backspace at the start of a bulleted or indented paragraph promotes it first
                // (PowerPoint).
                if !forward
                    && t.head.offset == 0
                    && let Some(body) = self.body(t.target)
                    && let Some(p) = body.paragraph(t.head.para)
                    && TextBody::level_of(p) > 0
                {
                    let tx = kiln_deck::edit::set_paragraph_level(
                        &self.pres,
                        t.target,
                        &[t.head.para],
                        -1,
                    )
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                    self.apply("deck.text.delete", tx)?;
                    return Ok(Change::Content);
                }
                let (a, b) = (t.head, to);
                self.edit_body("deck.text.delete", t.target, |doc| {
                    kiln_doc::edit::delete_range(doc, a, b)
                })?;
                if let Some(body) = self.body(t.target) {
                    let (lo, _) = ordered(body, a, b);
                    if let Some(e) = &mut self.view.text {
                        e.anchor = lo;
                        e.head = lo;
                    }
                }
                Ok(Change::Content)
            }
            D::NewParagraph => {
                if self.view.text.is_none() {
                    return Ok(Change::None);
                }
                self.begin_group();
                let result = (|| {
                    let at = self.delete_selection("deck.text.new_paragraph")?;
                    let Some(t) = self.view.text.clone() else {
                        return Ok(());
                    };
                    let at = at.unwrap_or(t.head);
                    self.edit_body("deck.text.new_paragraph", t.target, |doc| {
                        kiln_doc::edit::split_paragraph(doc, at)
                    })?;
                    Ok::<(), KilnError>(())
                })();
                self.end_group();
                result?;
                Ok(Change::Content)
            }
            D::LineBreak => self.insert_object(
                "deck.text.line_break",
                InlineObject::Break(Break {
                    kind: BreakKind::TextWrapping,
                    clear: None,
                }),
            ),
            D::InsertSlideNumber => {
                if self.view.text.is_some() {
                    let n = self.view.current + 1;
                    return self.insert_object(
                        "deck.insert.slide_number",
                        InlineObject::Field(Field::new("slidenum", &n.to_string())),
                    );
                }
                self.set_header_footer_slide_number()
            }
            D::Tab { shift } => {
                let Some(t) = self.view.text.clone() else {
                    return Ok(Change::None);
                };
                // In a table, Tab goes to the next cell (a new row after the last one) and
                // Shift+Tab to the previous one, with the cell's text selected.
                if let TextTarget::Cell { shape, row, col } = t.target {
                    let Some(ShapeKind::Table(tbl)) = self.pres.shape(shape).map(|s| &s.kind)
                    else {
                        return Ok(Change::None);
                    };
                    let cols = tbl.grid.len().max(1);
                    let rows = tbl.rows.len();
                    let at = row * cols + col;
                    let next = if *shift {
                        at.checked_sub(1)
                    } else {
                        Some(at + 1)
                    };
                    let Some(next) = next else {
                        return Ok(Change::Selection);
                    };
                    if next >= rows * cols {
                        self.edit_command(
                            &kiln_protocol::commands::EditCommand::TableInsertRowBelow,
                        )?;
                    }
                    self.begin_text(
                        TextTarget::Cell {
                            shape,
                            row: next / cols,
                            col: next % cols,
                        },
                        None,
                        true,
                    )?;
                    return Ok(Change::Content);
                }
                let body = self
                    .body(t.target)
                    .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
                let (a, b) = ordered(body, t.anchor, t.head);
                let at_start = a.offset == 0 || a.para != b.para;
                if *shift || at_start {
                    let paras = self.paragraphs_between(t.target, a, b);
                    let tx = kiln_deck::edit::set_paragraph_level(
                        &self.pres,
                        t.target,
                        &paras,
                        if *shift { -1 } else { 1 },
                    )
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                    self.apply("deck.text.tab", tx)?;
                    return Ok(Change::Content);
                }
                self.type_text("\t")
            }
            D::Promote | D::Demote => {
                let Some(t) = self.view.text.clone() else {
                    return Ok(Change::None);
                };
                let body = self
                    .body(t.target)
                    .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
                let (a, b) = ordered(body, t.anchor, t.head);
                let paras = self.paragraphs_between(t.target, a, b);
                let delta = if matches!(cmd, D::Demote) { 1 } else { -1 };
                let tx = kiln_deck::edit::set_paragraph_level(&self.pres, t.target, &paras, delta)
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                let label = if delta > 0 {
                    "deck.slides.demote"
                } else {
                    "deck.slides.promote"
                };
                Ok(if self.apply(label, tx)? {
                    Change::Content
                } else {
                    Change::None
                })
            }
            D::MoveCaret { motion, extend } => {
                let Some(t) = self.view.text.clone() else {
                    return Ok(Change::None);
                };
                let Some((to, goal)) = self.moved(&t, *motion) else {
                    return Ok(Change::None);
                };
                if let Some(e) = &mut self.view.text {
                    e.head = to;
                    if !*extend {
                        e.anchor = to;
                    }
                    e.goal_x = goal;
                }
                self.undo.break_group();
                self.epoch += 1;
                Ok(Change::Selection)
            }
            D::Find {
                text,
                match_case,
                whole_word,
                backward,
            } => self.find(text, *match_case, *whole_word, *backward),
            D::Replace {
                text,
                with,
                match_case,
                whole_word,
                all,
            } => self.replace(text, with, *match_case, *whole_word, *all),
            D::ReplaceFont { from, to } => self.replace_font(from, to),
            _ => Ok(Change::None),
        }
    }

    /// The paragraphs a selection touches.
    pub(crate) fn paragraphs_between(&self, target: TextTarget, a: Pos, b: Pos) -> Vec<NodeId> {
        let Some(body) = self.body(target) else {
            return Vec::new();
        };
        let (ia, ib) = (para_index(body, a.para), para_index(body, b.para));
        body.paragraphs()
            .enumerate()
            .filter(|(i, _)| *i >= ia.min(ib) && *i <= ia.max(ib))
            .map(|(_, p)| p.id)
            .collect()
    }

    /// The selected text (Copy), or the text of the selected shapes.
    pub(crate) fn selected_text_string(&self) -> Option<String> {
        if let Some(t) = &self.view.text {
            let body = self.body(t.target)?;
            if t.collapsed() {
                return None;
            }
            return Some(range_text(body, t.anchor, t.head));
        }
        let mut parts = Vec::new();
        for id in &self.view.shapes {
            if let Some(s) = self.pres.shape(*id) {
                s.walk_text(&mut |b| parts.push(b.text()));
            }
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    /// The caret and selection in slide px for the overlay: `(caret quad corners, selection
    /// rectangles as quads, composition underline)`.
    #[allow(clippy::type_complexity)]
    pub(crate) fn text_marks(
        &self,
    ) -> Option<(Vec<(f32, f32)>, Vec<Vec<(f32, f32)>>, Option<f32>)> {
        let t = self.view.text.as_ref()?;
        let (layout, m) = self.text_geometry(t.target)?;
        let (cx, top, h) = layout.caret(t.head.para, t.head.offset)?;
        let map = |x: f32, y: f32| {
            let p = m * kurbo::Point::new(f64::from(x), f64::from(y));
            #[allow(clippy::cast_possible_truncation)]
            (p.x as f32, p.y as f32)
        };
        let caret = vec![map(cx, top), map(cx, top + h)];
        let mut sel = Vec::new();
        if !t.collapsed() {
            for r in layout.selection(
                (t.anchor.para, t.anchor.offset),
                (t.head.para, t.head.offset),
            ) {
                sel.push(vec![
                    map(r.x, r.y),
                    map(r.right(), r.y),
                    map(r.right(), r.bottom()),
                    map(r.x, r.bottom()),
                ]);
            }
        }
        Some((caret, sel, t.composition.as_ref().map(|_| h)))
    }

    /// The caret rectangle in slide px (axis-aligned), for the input proxy.
    pub(crate) fn caret_rect(&self) -> Option<Bounds> {
        let (caret, _, _) = self.text_marks()?;
        let (a, b) = (caret[0], caret[1]);
        Some(Bounds::from_corners(
            a.0.min(b.0),
            a.1.min(b.1),
            a.0.max(b.0) + 1.0,
            a.1.max(b.1),
        ))
    }
}
