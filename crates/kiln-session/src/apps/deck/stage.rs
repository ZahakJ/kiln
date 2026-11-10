// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pointer gestures on the stage, decided here (the chrome only reports presses, moves and
//! releases in slide px): selecting (click, Shift/Ctrl+click, marquee), moving with smart guides
//! and snapping (Shift constrains to an axis, Ctrl+drag duplicates, Alt turns snapping off),
//! resizing by the eight handles (Shift keeps proportions, Ctrl from the centre), rotating by
//! the rotation handle (Shift in 15° steps), drawing a new shape, and placing the caret and
//! selecting text with the pointer. A drag previews on the model directly and becomes one undo
//! step when the button is released.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::TextTarget;
use kiln_deck::{ShapeKind, Transform};
use kiln_deck_layout::geom::{emu, rect_px};
use kiln_deck_layout::guides::{Edges, Guide, snap};
use kiln_deck_layout::render::{ShapeGeom, absolute_xfrm, is_line_preset};
use kiln_deck_layout::select::{
    Grip, Handle, Modifiers, grip_at, moved, resize, rotate_to, scale_into, union_bounds,
};
use kiln_doc::Pos;
use kiln_protocol::commands::{DeckCommand, DeckDirection};
use kiln_scene::Bounds;

use super::DeckDoc;
use crate::model::Change;

/// How far the pointer moves before a press becomes a drag, in screen px.
const DRAG_START: f32 = 3.0;
/// Snapping distance, in screen px.
const SNAP: f32 = 6.0;
/// A nudge with the arrow keys, in slide px (PowerPoint's default grid, 1/12 inch).
const NUDGE: f32 = 8.0;

/// What a drag does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DragKind {
    /// Moves the selection (a duplicate with Ctrl).
    Move,
    /// Resizes by a handle.
    Resize(Handle),
    /// Rotates.
    Rotate,
    /// Selects the shapes inside a rectangle.
    Marquee {
        /// The selection before (Shift/Ctrl add to it).
        base: Vec<NodeId>,
    },
    /// Selects text.
    Text {
        /// Where the selection started.
        anchor: Pos,
    },
    /// Crops a picture by a crop handle, or moves it behind its frame (`None`).
    Crop {
        /// The crop handle.
        handle: Option<Handle>,
        /// The crop before.
        crop: kiln_deck::Crop,
    },
    /// Draws a new shape.
    Draw {
        /// The preset (`textBox` for a text box).
        preset: String,
    },
}

/// A pointer gesture in progress.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Drag {
    pub kind: DragKind,
    /// Where the button went down.
    pub start: (f32, f32),
    /// The pointer now.
    pub now: (f32, f32),
    /// The press became a drag.
    pub active: bool,
    /// The shapes and their transforms before the drag.
    pub originals: Vec<(NodeId, Transform)>,
    /// The selection's box before the drag (slide px).
    pub start_box: Option<Bounds>,
    /// Other shapes' boxes (snapping targets).
    pub others: Vec<Bounds>,
    /// Guides to show.
    pub guides: Vec<Guide>,
    /// Ctrl was down at the press (duplicate on move).
    pub ctrl: bool,
    /// The slide.
    pub slide: NodeId,
    /// A Shift/Ctrl+click on a selected shape deselects it when the button comes up without
    /// a drag.
    pub click_toggle: Option<NodeId>,
}

fn mods(shift: bool, ctrl: bool, alt: bool) -> Modifiers {
    Modifiers { shift, ctrl, alt }
}

impl DeckDoc {
    /// The geometry of the current slide's shapes, top first.
    pub(crate) fn hit_list(&self) -> Vec<ShapeGeom> {
        let Some(owner) = self.owner() else {
            return Vec::new();
        };
        let mut v = self.renderer.borrow().hit_shapes(&self.pres, owner);
        v.reverse();
        v
    }

    /// The topmost shape under a slide point.
    pub(crate) fn shape_at(&self, x: f32, y: f32) -> Option<ShapeGeom> {
        let slop = 4.0 / self.zoom.max(0.05);
        self.hit_list().into_iter().find(|g| g.hit(x, y, slop))
    }

    /// The absolute transform of a shape.
    pub(crate) fn xfrm_of(&self, id: NodeId) -> Option<Transform> {
        absolute_xfrm(&self.pres, id)
    }

    /// The box of the selection as one transform (a single shape's own, turned; several
    /// shapes' union, upright).
    pub(crate) fn selection_xfrm(&self) -> Option<Transform> {
        match self.view.shapes.as_slice() {
            [] => None,
            [one] => self.xfrm_of(*one),
            many => {
                let xs: Vec<Transform> = many.iter().filter_map(|id| self.xfrm_of(*id)).collect();
                let b = union_bounds(&xs)?;
                Some(Transform::new(
                    emu(f64::from(b.x)),
                    emu(f64::from(b.y)),
                    emu(f64::from(b.w)),
                    emu(f64::from(b.h)),
                ))
            }
        }
    }

    /// Whether a shape is a line (no rotation handle, two handles in PowerPoint).
    pub(crate) fn is_line(&self, id: NodeId) -> bool {
        let Some(s) = self.pres.shape(id) else {
            return false;
        };
        if matches!(s.kind, ShapeKind::Connector(_)) {
            return true;
        }
        s.props()
            .and_then(|p| p.geometry.as_ref())
            .and_then(kiln_deck::Geometry::preset_name)
            .is_some_and(is_line_preset)
    }

    /// Whether clicking a shape's text area edits the text at once (text boxes and text
    /// placeholders, as in PowerPoint) rather than selecting the shape.
    fn edits_on_click(&self, id: NodeId) -> bool {
        let Some(s) = self.pres.shape(id) else {
            return false;
        };
        match &s.kind {
            ShapeKind::Geometry(a) => {
                a.text_box
                    || s.placeholder
                        .is_some_and(|p| p.kind.holds_text() && !p.kind.is_footer_area())
            }
            ShapeKind::Table(_) => true,
            _ => false,
        }
    }

    /// The text target under a point of a shape (a table's cell, or the shape's own text).
    fn text_target_at(&self, id: NodeId, x: f32, y: f32) -> Option<TextTarget> {
        let s = self.pres.shape(id)?;
        match &s.kind {
            ShapeKind::Table(_) => {
                let t = self.renderer.borrow_mut().table_layout(&self.pres, id)?;
                let xfrm = self.xfrm_of(id)?;
                let inv =
                    kiln_deck_layout::render::invert(kiln_deck_layout::geom::shape_matrix(&xfrm))?;
                let q = inv * kurbo::Point::new(f64::from(x), f64::from(y));
                #[allow(clippy::cast_possible_truncation)]
                let c = t.cell_at(q.x as f32, q.y as f32)?;
                Some(TextTarget::Cell {
                    shape: id,
                    row: c.row,
                    col: c.col,
                })
            }
            ShapeKind::Geometry(_) => Some(TextTarget::Shape(id)),
            _ => None,
        }
    }

    /// Whether a point is well inside a shape's box (not on its border, where a drag moves it).
    fn inside_text_area(&self, g: &ShapeGeom, x: f32, y: f32) -> bool {
        let Some(inv) = kiln_deck_layout::render::invert(g.matrix) else {
            return false;
        };
        let q = inv * kurbo::Point::new(f64::from(x), f64::from(y));
        let r = rect_px(&g.xfrm);
        let border = 6.0 / self.zoom.max(0.05);
        #[allow(clippy::cast_possible_truncation)]
        let (lx, ly) = (q.x as f32, q.y as f32);
        lx > border && ly > border && lx < r.w - border && ly < r.h - border
    }

    fn start_drag(&mut self, kind: DragKind, x: f32, y: f32, ctrl: bool) {
        let Ok(slide) = self.slide_id() else {
            return;
        };
        let originals: Vec<(NodeId, Transform)> = self
            .view
            .shapes
            .iter()
            .filter_map(|id| self.xfrm_of(*id).map(|x| (*id, x)))
            .collect();
        let xs: Vec<Transform> = originals.iter().map(|(_, x)| *x).collect();
        // Other shapes, and the drawing guides when they are shown.
        let others = self
            .hit_list()
            .into_iter()
            .filter(|g| !self.view.shapes.contains(&g.id))
            .map(|g| g.bounds())
            .chain(self.guide_snaps())
            .collect();
        self.drag = Some(Drag {
            kind,
            start: (x, y),
            now: (x, y),
            active: false,
            originals,
            start_box: union_bounds(&xs),
            others,
            guides: Vec::new(),
            ctrl,
            slide,
            click_toggle: None,
        });
    }

    /// Puts the dragged shapes back (Esc, another command).
    pub(crate) fn cancel_drag(&mut self) {
        let Some(d) = self.drag.take() else {
            return;
        };
        if d.active {
            self.restore(&d);
        }
    }

    pub(super) fn restore(&mut self, d: &Drag) {
        let mut changed = false;
        if let (DragKind::Crop { crop, .. }, Some((id, _))) = (&d.kind, d.originals.first()) {
            self.preview_crop(*id, *crop);
            changed = true;
        }
        for (id, x) in &d.originals {
            if let Some(s) = self.pres.shape_mut(*id)
                && s.xfrm != Some(*x)
            {
                s.xfrm = Some(*x);
                changed = true;
            }
        }
        if changed {
            self.note_preview(d.slide);
        }
    }

    /// Previews transforms on the model (no undo step).
    pub(super) fn preview(&mut self, slide: NodeId, xfrms: &[(NodeId, Transform)]) {
        for (id, x) in xfrms {
            if let Some(s) = self.pres.shape_mut(*id) {
                s.xfrm = Some(*x);
            }
        }
        self.note_preview(slide);
    }

    #[allow(clippy::too_many_lines)]
    fn pointer_down(&mut self, x: f32, y: f32, m: Modifiers, clicks: u8) -> Result<Change> {
        self.cancel_drag();
        self.find_done = false;
        if let Some(preset) = self.view.armed.clone() {
            self.end_text();
            self.view.shapes.clear();
            self.start_drag(DragKind::Draw { preset }, x, y, false);
            return Ok(Change::Selection);
        }
        // Crop mode: the crop handles, or a drag inside the frame; elsewhere ends it.
        if self.view.crop {
            if let Some(kind) = self.crop_grip(x, y) {
                self.start_drag(kind, x, y, false);
                return Ok(Change::Selection);
            }
            self.view.crop = false;
        }
        // In the text being edited: the caret, words, paragraphs.
        if let Some(t) = self.view.text.clone()
            && let Some(g) = self.shape_geom(t.shape)
            && g.hit(x, y, 2.0 / self.zoom.max(0.05))
        {
            let target = self.text_target_at(t.shape, x, y).unwrap_or(t.target);
            // Shift+click in another cell selects the cells between.
            if let (
                true,
                TextTarget::Cell { row, col, .. },
                TextTarget::Cell {
                    row: r0, col: c0, ..
                },
            ) = (m.shift, target, t.target)
                && target != t.target
            {
                self.extend_cells((r0, c0), (row, col));
                return Ok(Change::Selection);
            }
            self.view.cells = None;
            if target != t.target {
                self.begin_text(target, Some((x, y)), false)?;
            } else if let Some(p) = self.text_hit(target, x, y) {
                self.select_text_unit(p, clicks, m.shift);
            }
            let anchor = self
                .view
                .text
                .as_ref()
                .map_or(Pos::new(NodeId(0), 0), |t| t.anchor);
            self.start_drag(DragKind::Text { anchor }, x, y, false);
            return Ok(Change::Selection);
        }
        // The selection's handles.
        if let Some(sx) = self.selection_xfrm() {
            let single = self.view.shapes.len() == 1;
            let rotatable = single && !self.view.shapes.iter().any(|id| self.is_line(*id));
            if let Some(grip) = grip_at(&sx, x, y, self.zoom, rotatable) {
                match grip {
                    Grip::Size(h) => {
                        self.end_text();
                        self.start_drag(DragKind::Resize(h), x, y, false);
                        return Ok(Change::Selection);
                    }
                    Grip::Rotate => {
                        self.end_text();
                        self.start_drag(DragKind::Rotate, x, y, false);
                        return Ok(Change::Selection);
                    }
                    Grip::Body => {}
                }
            }
        }
        match self.shape_at(x, y) {
            Some(g) => {
                let id = g.id;
                if m.shift || (m.ctrl && clicks == 1 && !self.view.shapes.contains(&id)) {
                    self.end_text();
                    let selected = self.view.shapes.contains(&id);
                    if !selected {
                        self.view.shapes.push(id);
                    }
                    self.view.rail = false;
                    self.start_drag(DragKind::Move, x, y, m.ctrl);
                    if selected && let Some(d) = &mut self.drag {
                        d.click_toggle = Some(id);
                    }
                    return Ok(Change::Selection);
                }
                let edits = self.edits_on_click(id) && self.inside_text_area(&g, x, y);
                if (clicks >= 2 || edits)
                    && let Some(target) = self.text_target_at(id, x, y)
                {
                    {
                        let had = self.view.text.as_ref().map(|t| t.target);
                        if had != Some(target) {
                            self.begin_text(target, Some((x, y)), false)?;
                        }
                        if let Some(p) = self.text_hit(target, x, y) {
                            self.select_text_unit(p, clicks, false);
                        }
                        let anchor = self
                            .view
                            .text
                            .as_ref()
                            .map_or(Pos::new(NodeId(0), 0), |t| t.anchor);
                        self.start_drag(DragKind::Text { anchor }, x, y, false);
                        return Ok(Change::Selection);
                    }
                }
                self.end_text();
                if !self.view.shapes.contains(&id) {
                    self.view.shapes = vec![id];
                }
                self.view.rail = false;
                self.start_drag(DragKind::Move, x, y, m.ctrl);
                Ok(Change::Selection)
            }
            None => {
                self.end_text();
                let base = if m.shift || m.ctrl {
                    self.view.shapes.clone()
                } else {
                    self.view.shapes.clear();
                    Vec::new()
                };
                self.view.rail = false;
                self.start_drag(DragKind::Marquee { base }, x, y, false);
                Ok(Change::Selection)
            }
        }
    }

    /// Places the caret, or selects the word (double click) or paragraph (triple click).
    fn select_text_unit(&mut self, p: Pos, clicks: u8, extend: bool) {
        let Some(t) = self.view.text.clone() else {
            return;
        };
        let Some(body) = self.body(t.target) else {
            return;
        };
        let (anchor, head) = match clicks {
            2 => {
                let text: Vec<char> = body
                    .paragraph(p.para)
                    .map(|q| q.content.raw_text().chars().collect())
                    .unwrap_or_default();
                let word = |c: char| c.is_alphanumeric() || c == '\'' || c == '_';
                let mut s = p.offset.min(text.len());
                let mut e = s;
                while s > 0 && word(text[s - 1]) {
                    s -= 1;
                }
                while e < text.len() && word(text[e]) {
                    e += 1;
                }
                // A trailing space belongs to the word (PowerPoint).
                if e < text.len() && text[e] == ' ' {
                    e += 1;
                }
                (Pos::new(p.para, s), Pos::new(p.para, e))
            }
            n if n >= 3 => {
                let len = body.paragraph(p.para).map_or(0, |q| q.content.len());
                (Pos::new(p.para, 0), Pos::new(p.para, len))
            }
            _ if extend => (t.anchor, p),
            _ => (p, p),
        };
        if let Some(e) = &mut self.view.text {
            e.anchor = anchor;
            e.head = head;
            e.goal_x = None;
        }
        self.undo.break_group();
        self.epoch += 1;
    }

    fn shape_geom(&self, id: NodeId) -> Option<ShapeGeom> {
        self.renderer.borrow().shape_geom(&self.pres, id)
    }

    #[allow(clippy::too_many_lines)]
    fn pointer_move(&mut self, x: f32, y: f32, m: Modifiers) -> Result<Change> {
        let Some(mut d) = self.drag.take() else {
            return Ok(Change::None);
        };
        d.now = (x, y);
        let zoom = self.zoom.max(0.05);
        let (dx, dy) = (x - d.start.0, y - d.start.1);
        if !d.active && (dx * dx + dy * dy).sqrt() * zoom >= DRAG_START {
            d.active = true;
        }
        let snapping = !m.alt;
        let threshold = SNAP / zoom;
        let slide_box = Bounds::new(
            0.0,
            0.0,
            kiln_deck_layout::geom::px(self.pres.slide_size.cx),
            kiln_deck_layout::geom::px(self.pres.slide_size.cy),
        );
        match d.kind.clone() {
            DragKind::Move if d.active => {
                let (mut ddx, mut ddy) = if m.shift {
                    if dx.abs() >= dy.abs() {
                        (dx, 0.0)
                    } else {
                        (0.0, dy)
                    }
                } else {
                    (dx, dy)
                };
                d.guides.clear();
                if snapping && let Some(b) = d.start_box {
                    let moving = b.translate(ddx, ddy);
                    let s = snap(moving, &d.others, slide_box, threshold, Edges::ALL);
                    if !(m.shift && dy.abs() > dx.abs()) {
                        ddx += s.dx;
                    }
                    if !(m.shift && dx.abs() >= dy.abs()) {
                        ddy += s.dy;
                    }
                    d.guides = s.guides;
                }
                let xfrms: Vec<(NodeId, Transform)> = d
                    .originals
                    .iter()
                    .map(|(id, x)| (*id, moved(x, ddx, ddy, Modifiers::default())))
                    .collect();
                self.preview(d.slide, &xfrms);
            }
            DragKind::Resize(h) if d.active => {
                d.guides.clear();
                let xfrms = self.resized(&d, h, dx, dy, m, snapping, threshold, slide_box);
                self.preview(d.slide, &xfrms.0);
                d.guides = xfrms.1;
            }
            DragKind::Crop { .. } if d.active => {
                self.crop_drag(&d, dx, dy, m);
            }
            DragKind::Rotate if d.active => {
                if let [(id, x0)] = d.originals.as_slice() {
                    let r = rotate_to(x0, x, y, m);
                    self.preview(d.slide, &[(*id, r)]);
                }
            }
            DragKind::Marquee { base } => {
                let rect = Bounds::from_corners(
                    d.start.0.min(x),
                    d.start.1.min(y),
                    d.start.0.max(x),
                    d.start.1.max(y),
                );
                let mut sel = base.clone();
                for g in self.hit_list().into_iter().rev() {
                    let b = g.bounds();
                    let inside = b.x >= rect.x
                        && b.y >= rect.y
                        && b.right() <= rect.right()
                        && b.bottom() <= rect.bottom();
                    if inside && !sel.contains(&g.id) {
                        sel.push(g.id);
                    }
                }
                self.view.shapes = sel;
                self.epoch += 1;
            }
            // A press that has not moved keeps the selection it made (a double-click's word).
            DragKind::Text { anchor } if d.active => {
                // Leaving the cell it started in selects cells.
                if let Some(t) = self.view.text.clone()
                    && let TextTarget::Cell { row, col, .. } = t.target
                    && let Some(TextTarget::Cell { row: r, col: c, .. }) =
                        self.text_target_at(t.shape, x, y)
                {
                    self.extend_cells((row, col), (r, c));
                }
                if self.view.cells.is_none()
                    && let Some(t) = self.view.text.clone()
                    && let Some(p) = self.text_hit(t.target, x, y)
                    && let Some(e) = &mut self.view.text
                {
                    e.anchor = anchor;
                    e.head = p;
                    self.epoch += 1;
                }
            }
            DragKind::Draw { preset } if kiln_deck_layout::render::is_line_preset(&preset) => {
                d.guides.clear();
                d.now = self.site_near(x, y, None).map_or((x, y), |(_, q)| q);
                self.epoch += 1;
            }
            DragKind::Draw { .. } => {
                d.guides.clear();
                if snapping {
                    let r = draw_rect(d.start, (x, y), m.shift);
                    let s = snap(
                        r,
                        &self
                            .hit_list()
                            .iter()
                            .map(ShapeGeom::bounds)
                            .collect::<Vec<_>>(),
                        slide_box,
                        threshold,
                        Edges {
                            left: false,
                            center_x: false,
                            right: x >= d.start.0,
                            top: false,
                            center_y: false,
                            bottom: y >= d.start.1,
                        },
                    );
                    d.now = (x + s.dx, y + s.dy);
                    d.guides = s.guides;
                }
                self.epoch += 1;
            }
            _ => {}
        }
        self.drag = Some(d);
        Ok(Change::Selection)
    }

    /// The transforms of a resize drag and the guides it snapped to.
    #[allow(clippy::too_many_arguments)]
    fn resized(
        &self,
        d: &Drag,
        h: Handle,
        dx: f32,
        dy: f32,
        m: Modifiers,
        snapping: bool,
        threshold: f32,
        slide_box: Bounds,
    ) -> (Vec<(NodeId, Transform)>, Vec<Guide>) {
        let (sx, sy) = h.sides();
        let edges = Edges {
            left: sx < 0,
            center_x: false,
            right: sx > 0,
            top: sy < 0,
            center_y: false,
            bottom: sy > 0,
        };
        match d.originals.as_slice() {
            [(id, x0)] => {
                let mut r = resize(x0, h, dx, dy, m);
                let mut guides = Vec::new();
                if snapping && r.rot == 0 && !m.shift {
                    let b = rect_px(&r);
                    let s = snap(b, &d.others, slide_box, threshold, edges);
                    if s.dx != 0.0 || s.dy != 0.0 {
                        r = resize(x0, h, dx + s.dx, dy + s.dy, m);
                    }
                    guides = s.guides;
                }
                (vec![(*id, r)], guides)
            }
            many => {
                let Some(b0) = d.start_box else {
                    return (Vec::new(), Vec::new());
                };
                let boxed = Transform::new(
                    emu(f64::from(b0.x)),
                    emu(f64::from(b0.y)),
                    emu(f64::from(b0.w)),
                    emu(f64::from(b0.h)),
                );
                let nb = rect_px(&resize(&boxed, h, dx, dy, m));
                (
                    many.iter()
                        .map(|(id, x)| (*id, scale_into(x, b0, nb)))
                        .collect(),
                    Vec::new(),
                )
            }
        }
    }

    fn pointer_up(&mut self, x: f32, y: f32, m: Modifiers) -> Result<Change> {
        if self.drag.is_some() {
            self.pointer_move(x, y, m)?;
        }
        let Some(d) = self.drag.take() else {
            return Ok(Change::None);
        };
        match &d.kind {
            DragKind::Crop { .. } if d.active => self.crop_done(&d),
            DragKind::Move | DragKind::Resize(_) | DragKind::Rotate if d.active => {
                // The final transforms, then back to the start so the edit records an undo step.
                let finals: Vec<(NodeId, Transform)> = d
                    .originals
                    .iter()
                    .filter_map(|(id, _)| {
                        self.pres.shape(*id).and_then(|s| s.xfrm).map(|x| (*id, x))
                    })
                    .collect();
                self.restore(&d);
                if d.ctrl && d.kind == DragKind::Move {
                    let (ox, oy) = d
                        .originals
                        .first()
                        .zip(finals.first())
                        .map_or((0, 0), |((_, a), (_, b))| (b.x - a.x, b.y - a.y));
                    return self.duplicate_shapes_by(ox, oy);
                }
                let label = match d.kind {
                    DragKind::Move => "deck.drawing.nudge",
                    DragKind::Resize(_) => "deck.drawing.resize",
                    _ => "deck.arrange.rotate",
                };
                self.begin_group();
                let mut result = Ok(());
                for (id, x) in finals {
                    match kiln_deck::edit::set_transform(&mut self.pres, id, x) {
                        Ok(tx) => {
                            if let Err(e) = self.apply(label, tx) {
                                result = Err(e);
                            }
                        }
                        Err(e) => result = Err(KilnError::InvalidInput(e.to_string())),
                    }
                }
                self.update_connectors(&d.originals.iter().map(|(id, _)| *id).collect::<Vec<_>>());
                if let (DragKind::Resize(_), [(id, _)]) = (&d.kind, d.originals.as_slice())
                    && self.is_line(*id)
                {
                    self.reattach(*id);
                }
                self.end_group();
                result?;
                Ok(Change::Content)
            }
            DragKind::Draw { preset } if kiln_deck_layout::render::is_line_preset(preset) => {
                self.view.armed = None;
                let zoom = self.zoom.max(0.05);
                let (dx, dy) = (d.now.0 - d.start.0, d.now.1 - d.start.1);
                // A click draws a one-inch line to the right.
                let to = if dx.abs() < 4.0 / zoom && dy.abs() < 4.0 / zoom {
                    (d.start.0 + 96.0, d.start.1)
                } else {
                    line_end(d.start, d.now, m.shift)
                };
                self.draw_line(preset, d.start, to)
            }
            DragKind::Draw { preset } => {
                let mut r = draw_rect(d.start, d.now, m.shift);
                let tiny = r.w < 4.0 / self.zoom.max(0.05) && r.h < 4.0 / self.zoom.max(0.05);
                if tiny {
                    // A click places the default size (1 inch, a text box one line).
                    let (w, h) = if preset == "textBox" {
                        (240.0, 40.0)
                    } else {
                        (96.0, 96.0)
                    };
                    r = Bounds::new(d.start.0, d.start.1, w, h);
                }
                self.view.armed = None;
                let rect = kiln_protocol::commands::DeckRect {
                    x: r.x,
                    y: r.y,
                    width: r.w.max(2.0),
                    height: r.h.max(2.0),
                };
                if preset == "textBox" {
                    self.insert_text_box(Some(rect))
                } else {
                    self.insert_shape(preset, Some(rect))
                }
            }
            DragKind::Move if !d.active => {
                if let Some(id) = d.click_toggle {
                    self.view.shapes.retain(|s| *s != id);
                }
                self.epoch += 1;
                Ok(Change::Selection)
            }
            _ => {
                self.epoch += 1;
                Ok(Change::Selection)
            }
        }
    }

    /// Stage commands.
    pub(crate) fn stage_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        match cmd {
            D::PointerDown {
                x,
                y,
                shift,
                ctrl,
                alt,
                clicks,
            } => self.pointer_down(*x, *y, mods(*shift, *ctrl, *alt), (*clicks).max(1)),
            D::PointerMove {
                x,
                y,
                shift,
                ctrl,
                alt,
            } => self.pointer_move(*x, *y, mods(*shift, *ctrl, *alt)),
            D::PointerUp {
                x,
                y,
                shift,
                ctrl,
                alt,
            } => self.pointer_up(*x, *y, mods(*shift, *ctrl, *alt)),
            D::ArmInsert { preset } => {
                self.view.armed = preset.clone();
                Ok(Change::Selection)
            }
            D::SelectShapes { ids } => {
                let Some(shapes) = self.stage_shapes() else {
                    return Ok(Change::None);
                };
                let top: Vec<NodeId> = shapes.iter().map(|s| s.id).collect();
                self.end_text();
                self.view.shapes = ids.iter().copied().filter(|i| top.contains(i)).collect();
                self.view.rail = false;
                Ok(Change::Selection)
            }
            D::SelectAll => {
                if let Some(t) = self.view.text.clone() {
                    if let Some((a, b)) = self.body(t.target).and_then(super::text::body_ends)
                        && let Some(e) = &mut self.view.text
                    {
                        e.anchor = a;
                        e.head = b;
                    }
                    return Ok(Change::Selection);
                }
                if self.view.rail {
                    self.view.slides = (0..self.pres.slides.len()).collect();
                    return Ok(Change::Selection);
                }
                let Some(shapes) = self.stage_shapes() else {
                    return Ok(Change::None);
                };
                self.view.shapes = shapes.iter().filter(|s| !s.hidden).map(|s| s.id).collect();
                Ok(Change::Selection)
            }
            D::Escape => {
                if self.drag.is_some() {
                    self.cancel_drag();
                } else if self.view.crop {
                    self.view.crop = false;
                } else if self.view.cells.is_some() {
                    self.view.cells = None;
                } else if self.view.armed.is_some() {
                    self.view.armed = None;
                } else if self.view.text.is_some() {
                    self.end_text();
                } else {
                    self.view.shapes.clear();
                }
                Ok(Change::Selection)
            }
            D::CycleSelection { backward } => {
                let Some(shapes) = self.stage_shapes() else {
                    return Ok(Change::None);
                };
                let ids: Vec<NodeId> = shapes.iter().filter(|s| !s.hidden).map(|s| s.id).collect();
                if ids.is_empty() {
                    return Ok(Change::None);
                }
                let at = self
                    .view
                    .shapes
                    .last()
                    .and_then(|s| ids.iter().position(|i| i == s));
                let n = ids.len();
                let next = match (at, backward) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                self.end_text();
                self.view.shapes = vec![ids[next]];
                Ok(Change::Selection)
            }
            _ => Ok(Change::None),
        }
    }

    /// Arrow keys: nudge (move) or resize the selection.
    pub(crate) fn nudge(
        &mut self,
        direction: DeckDirection,
        fine: bool,
        resize_it: bool,
    ) -> Result<Change> {
        if self.view.shapes.is_empty() {
            return Ok(Change::None);
        }
        let step = if fine { 1.0 } else { NUDGE };
        let (dx, dy) = match direction {
            DeckDirection::Up => (0.0, -step),
            DeckDirection::Down => (0.0, step),
            DeckDirection::Left => (-step, 0.0),
            DeckDirection::Right => (step, 0.0),
        };
        let label = if resize_it {
            "deck.drawing.resize"
        } else {
            "deck.drawing.nudge"
        };
        let ids = self.view.shapes.clone();
        self.begin_group();
        let mut result = Ok(());
        for id in &ids {
            let Some(x) = self.xfrm_of(*id) else { continue };
            let new = if resize_it {
                let mut n = x;
                n.cx = (x.cx + emu(f64::from(dx))).max(emu(1.0));
                n.cy = (x.cy + emu(f64::from(dy))).max(emu(1.0));
                n
            } else {
                x.translated(emu(f64::from(dx)), emu(f64::from(dy)))
            };
            match kiln_deck::edit::set_transform(&mut self.pres, *id, new) {
                Ok(tx) => {
                    if let Err(e) = self.apply(label, tx) {
                        result = Err(e);
                    }
                }
                Err(e) => result = Err(KilnError::InvalidInput(e.to_string())),
            }
        }
        self.update_connectors(&ids);
        self.end_group();
        result?;
        Ok(Change::Content)
    }
}

/// The rectangle of a shape being drawn from `a` to `b` (Shift: a square).
/// A line's far end; Shift keeps it horizontal, vertical or at 45°.
fn line_end(a: (f32, f32), b: (f32, f32), constrain: bool) -> (f32, f32) {
    if !constrain {
        return b;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    (a.0 + len * angle.cos(), a.1 + len * angle.sin())
}

fn draw_rect(a: (f32, f32), b: (f32, f32), square: bool) -> Bounds {
    let (mut w, mut h) = (b.0 - a.0, b.1 - a.1);
    if square {
        let s = w.abs().max(h.abs());
        w = s * w.signum();
        h = s * h.signum();
    }
    Bounds::from_corners(
        a.0.min(a.0 + w),
        a.1.min(a.1 + h),
        a.0.max(a.0 + w),
        a.1.max(a.1 + h),
    )
}
