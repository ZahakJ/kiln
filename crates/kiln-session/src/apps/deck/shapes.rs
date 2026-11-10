// SPDX-License-Identifier: AGPL-3.0-or-later
//! Objects on slides: inserting shapes, text boxes, pictures, tables and charts; deleting and
//! duplicating; Arrange (order, align, distribute, group, rotate, flip); exact size and
//! position; alt text; cropping; hyperlinks; and connectors that stay attached to the shapes
//! they join.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::edit::{Align, AlignTo};
use kiln_deck::ops::{EditKind, Op, TextTarget, Transaction};
use kiln_deck::{Crop, Geometry, Part, Rect, Shape, ShapeKind, Transform};
use kiln_deck_layout::geom::{emu, px, rect_px, shape_matrix};
use kiln_ooxml::drawingml::presets::preset_geometry;
use kiln_protocol::commands::{DeckAlign, DeckCommand, DeckOrder, DeckRect};

use super::DeckDoc;
use crate::model::Change;

/// How near a connection site a line's end attaches, in screen px.
const SITE_REACH: f32 = 10.0;

/// The picture formats a slide accepts, by extension.
fn media_type(name: &str) -> &'static str {
    let ext = name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => "image/png",
    }
}

fn ext_of(name: &str) -> String {
    let e = name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match e.as_str() {
        "jpg" | "jpeg" => "jpeg".into(),
        "gif" | "bmp" | "webp" | "svg" | "tiff" | "png" => e,
        "tif" => "tiff".into(),
        _ => "png".into(),
    }
}

/// The name of a new shape of a preset ("Rectangle: Rounded Corners").
fn preset_name(preset: &str) -> String {
    kiln_deck_layout::names::preset_display_name(preset)
}

pub(super) fn rect_emu(r: &DeckRect) -> Rect {
    Rect::new(
        emu(f64::from(r.x)),
        emu(f64::from(r.y)),
        emu(f64::from(r.width.max(1.0))),
        emu(f64::from(r.height.max(1.0))),
    )
}

impl DeckDoc {
    /// The slide size in px.
    pub(crate) fn slide_px(&self) -> (f32, f32) {
        (px(self.pres.slide_size.cx), px(self.pres.slide_size.cy))
    }

    /// A rectangle of `w` × `h` px centred on the slide.
    pub(super) fn centred(&self, w: f32, h: f32) -> DeckRect {
        let (sw, sh) = self.slide_px();
        DeckRect {
            x: (sw - w) / 2.0,
            y: (sh - h) / 2.0,
            width: w,
            height: h,
        }
    }

    /// An empty content placeholder of the current slide that an inserted table, chart or
    /// picture takes the place of, as in PowerPoint (`picture`: picture placeholders too).
    fn content_slot(&self, picture: bool) -> Option<(NodeId, DeckRect)> {
        use kiln_deck::PlaceholderType as P;
        let slide = self.current_slide().and_then(|s| self.pres.slide(s))?;
        slide.shapes.iter().find_map(|s| {
            let ph = s.placeholder.as_ref()?;
            let fits = match ph.kind {
                P::Object | P::Body | P::Chart | P::Table => true,
                P::Picture => picture,
                _ => false,
            };
            if !fits || !kiln_deck_layout::render::body_is_empty(s.text()) {
                return None;
            }
            if !matches!(s.kind, ShapeKind::Geometry(_)) {
                return None;
            }
            let r = rect_px(&self.xfrm_of(s.id)?);
            Some((
                s.id,
                DeckRect {
                    x: r.x,
                    y: r.y,
                    width: r.w,
                    height: r.h,
                },
            ))
        })
    }

    /// Where a new `w` × `h` object goes when there is no placeholder for it: the slide's centre,
    /// else the place among a few candidates that covers the least of what is on the slide.
    fn free_rect(&self, w: f32, h: f32) -> DeckRect {
        let (sw, sh) = self.slide_px();
        let others: Vec<kiln_scene::Bounds> = self
            .stage_shapes()
            .map(|shapes| {
                shapes
                    .iter()
                    .filter(|s| {
                        // Empty placeholders are only prompts.
                        s.placeholder.is_none()
                            || !kiln_deck_layout::render::body_is_empty(s.text())
                    })
                    .filter_map(|s| self.xfrm_of(s.id).map(|x| rect_px(&x)))
                    .collect()
            })
            .unwrap_or_default();
        let (w, h) = (w.min(sw), h.min(sh));
        let m = 24.0_f32;
        let xs = [(sw - w) / 2.0, sw - w - m, m];
        let ys = [(sh - h) / 2.0, sh - h - m, m];
        let mut best = (f32::INFINITY, (sw - w) / 2.0, (sh - h) / 2.0);
        for &y in &ys {
            for &x in &xs {
                let r = kiln_scene::Bounds::new(x.max(0.0), y.max(0.0), w, h);
                let covered: f32 = others
                    .iter()
                    .map(|o| {
                        let ix = (r.right().min(o.right()) - r.x.max(o.x)).max(0.0);
                        let iy = (r.bottom().min(o.bottom()) - r.y.max(o.y)).max(0.0);
                        ix * iy
                    })
                    .sum();
                if covered + 0.5 < best.0 {
                    best = (covered, r.x, r.y);
                }
            }
        }
        DeckRect {
            x: best.1,
            y: best.2,
            width: w,
            height: h,
        }
    }

    /// Takes the content placeholder (deleting it, in the open undo group) or a free place.
    pub(super) fn place_object(
        &mut self,
        w: f32,
        h: f32,
        picture: bool,
        fit: bool,
    ) -> Result<DeckRect> {
        if let Some((ph, slot)) = self.content_slot(picture) {
            let tx = kiln_deck::edit::delete_shapes(&mut self.pres, &[ph])
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            self.apply("deck.insert.placeholder", tx)?;
            if !fit {
                return Ok(slot);
            }
            // Keep the object's proportions inside the placeholder, centred.
            let k = (slot.width / w.max(1.0)).min(slot.height / h.max(1.0));
            let (pw, ph) = (w * k, h * k);
            return Ok(DeckRect {
                x: slot.x + (slot.width - pw) / 2.0,
                y: slot.y + (slot.height - ph) / 2.0,
                width: pw,
                height: ph,
            });
        }
        Ok(self.free_rect(w, h))
    }

    /// Inserts shapes on the current slide (on top) and selects them.
    pub(crate) fn insert_on_slide(
        &mut self,
        label: &'static str,
        shapes: Vec<Shape>,
    ) -> Result<Vec<NodeId>> {
        let slide = self.slide_id()?;
        let ids: Vec<NodeId> = shapes.iter().map(|s| s.id).collect();
        let tx = kiln_deck::edit::insert_shapes(&mut self.pres, slide, shapes, None)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply(label, tx)?;
        self.end_text();
        self.view.shapes = ids.clone();
        self.view.rail = false;
        Ok(ids)
    }

    /// Insert ▸ Shapes.
    pub(crate) fn insert_shape(&mut self, preset: &str, rect: Option<DeckRect>) -> Result<Change> {
        if preset == "textBox" {
            return self.insert_text_box(rect);
        }
        if kiln_ooxml::drawingml::presets::preset_category(preset).is_none() {
            return Err(KilnError::InvalidInput(format!(
                "'{preset}' is not a shape"
            )));
        }
        let r = rect.unwrap_or_else(|| self.centred(144.0, 144.0));
        let id = self.pres.alloc_id();
        let shape = if kiln_deck_layout::render::is_line_preset(preset) {
            let (x0, y0) = (emu(f64::from(r.x)), emu(f64::from(r.y)));
            let (x1, y1) = (
                emu(f64::from(r.x + r.width)),
                emu(f64::from(r.y + r.height)),
            );
            let mut s = kiln_deck::edit::new_connector(
                &mut self.pres,
                preset,
                (x0, y0),
                (x1, y1),
                (None, None),
            );
            s.name = format!("{} {}", preset_name(preset), s.id.0);
            s
        } else {
            let mut s = Shape::geometry(id, preset, rect_emu(&r));
            s.name = format!("{} {}", preset_name(preset), id.0);
            s
        };
        self.insert_on_slide("deck.insert.shape", vec![shape])?;
        Ok(Change::Content)
    }

    /// Insert ▸ Text Box: a box at `rect` (or in the middle) with its text being edited.
    pub(crate) fn insert_text_box(&mut self, rect: Option<DeckRect>) -> Result<Change> {
        let r = rect.unwrap_or_else(|| self.centred(320.0, 48.0));
        let id = self.pres.alloc_id();
        let mut shape = Shape::text_box(id, rect_emu(&r), "");
        shape.name = format!("TextBox {}", id.0);
        self.insert_on_slide("deck.insert.text_box", vec![shape])?;
        self.begin_text(TextTarget::Shape(id), None, false)?;
        Ok(Change::Content)
    }

    /// Insert ▸ Pictures (into a selected empty picture placeholder, or fitted on the slide).
    pub(crate) fn insert_picture(
        &mut self,
        name: &str,
        bytes: &[u8],
        rect: Option<DeckRect>,
    ) -> Result<Change> {
        let size = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok());
        let Some((w, h)) = size else {
            return Err(KilnError::InvalidInput(format!(
                "{name} is not a picture Kiln can read"
            )));
        };
        let part = (1u32..)
            .map(|n| format!("/ppt/media/image{n}.{}", ext_of(name)))
            .find(|p| !self.pres.parts.contains_key(p))
            .unwrap_or_else(|| format!("/ppt/media/kiln.{}", ext_of(name)));
        // Fit within 80 % of the slide, keeping the aspect ratio; pictures are 96 dpi.
        let (sw, sh) = self.slide_px();
        #[allow(clippy::cast_precision_loss)]
        let (mut pw, mut ph) = (w as f32, h as f32);
        let k = (sw * 0.8 / pw).min(sh * 0.8 / ph).min(1.0);
        pw *= k;
        ph *= k;
        self.end_text();
        self.begin_group();
        let placed = match rect {
            Some(r) => Ok(r),
            None => self.place_object(pw, ph, true, true),
        };
        let r = match placed {
            Ok(r) => r,
            Err(e) => {
                self.end_group();
                return Err(e);
            }
        };
        let mut shape = kiln_deck::edit::new_picture(&mut self.pres, &part, rect_emu(&r));
        shape.name = format!("Picture {}", shape.id.0);
        shape.alt_text = Some(
            name.rsplit(['/', '\\'])
                .next()
                .unwrap_or(name)
                .rsplit_once('.')
                .map_or(name, |(stem, _)| stem)
                .to_owned(),
        );
        let slide = self.slide_id()?;
        let id = shape.id;
        let mut tx = Transaction::new(vec![Op::SetPart {
            key: part.clone(),
            part: Some(Part {
                content_type: media_type(name).to_owned(),
                bytes: bytes.to_vec(),
            }),
        }]);
        let ins = kiln_deck::edit::insert_shapes(&mut self.pres, slide, vec![shape], None)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        tx.ops.extend(ins.ops);
        tx.kind = EditKind::Other;
        tx.selection_after = vec![id];
        let applied = self.apply("deck.insert.picture", tx);
        self.end_group();
        applied?;
        self.view.shapes = vec![id];
        Ok(Change::Content)
    }

    /// Insert ▸ Table.
    pub(crate) fn insert_table(&mut self, rows: u32, cols: u32) -> Result<Change> {
        let rows = usize::try_from(rows)
            .unwrap_or(1)
            .clamp(1, kiln_deck::MAX_TABLE_ROWS);
        let cols = usize::try_from(cols)
            .unwrap_or(1)
            .clamp(1, kiln_deck::MAX_TABLE_COLS);
        let (sw, _) = self.slide_px();
        #[allow(clippy::cast_precision_loss)]
        let h = 40.0 * rows as f32;
        let w = (sw * 0.8).min(160.0 * f32::from(u16::try_from(cols).unwrap_or(1)));
        self.end_text();
        self.begin_group();
        let result = (|| {
            let mut r = self.place_object(w, h, false, false)?;
            // A table is as wide as its placeholder and as tall as its rows.
            r.height = h.min(r.height.max(h));
            let shape = kiln_deck::edit::new_table(&mut self.pres, rows, cols, rect_emu(&r))
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            let id = shape.id;
            self.insert_on_slide("deck.insert.table", vec![shape])?;
            Ok::<NodeId, KilnError>(id)
        })();
        self.end_group();
        let id = result?;
        self.begin_text(
            TextTarget::Cell {
                shape: id,
                row: 0,
                col: 0,
            },
            None,
            false,
        )?;
        Ok(Change::Content)
    }

    /// Duplicates the selected shapes `dx`, `dy` EMU away (Ctrl+D, Ctrl+drag).
    pub(crate) fn duplicate_shapes_by(&mut self, dx: i64, dy: i64) -> Result<Change> {
        let slide = self.slide_id()?;
        let originals: Vec<Shape> = self
            .view
            .shapes
            .iter()
            .filter_map(|id| self.pres.shape(*id).cloned())
            .collect();
        let mut copies: Vec<Shape> = Vec::with_capacity(originals.len());
        for s in &originals {
            let mut c = self.fresh_copy(s);
            if let Some(x) = c.xfrm.or_else(|| self.pres.resolved_xfrm(s.id)) {
                c.xfrm = Some(x.translated(dx, dy));
            }
            copies.push(c);
        }
        if copies.is_empty() {
            return Ok(Change::None);
        }
        let ids: Vec<NodeId> = copies.iter().map(|c| c.id).collect();
        let tx = kiln_deck::edit::insert_shapes(&mut self.pres, slide, copies, None)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply("deck.slides.duplicate", tx)?;
        self.view.shapes = ids;
        Ok(Change::Content)
    }

    /// A copy of a shape with fresh ids (children included); placeholders become ordinary
    /// shapes with their inherited look made explicit.
    pub(crate) fn fresh_copy(&mut self, s: &Shape) -> Shape {
        let mut c = s.clone();
        if c.placeholder.is_some() {
            if c.xfrm.is_none() {
                c.xfrm = self.pres.resolved_xfrm(s.id);
            }
            if let Some(p) = self.pres.resolved_props(s.id)
                && let Some(own) = c.props_mut()
            {
                *own = p;
            }
            c.placeholder = None;
        }
        self.renumber(&mut c);
        c
    }

    fn renumber(&mut self, s: &mut Shape) {
        s.id = self.pres.alloc_id();
        if let ShapeKind::Group(g) = &mut s.kind {
            for ch in &mut g.children {
                self.renumber(ch);
            }
        }
    }

    /// Delete: the selected shapes.
    pub(crate) fn delete_shapes(&mut self) -> Result<Change> {
        let ids = self.view.shapes.clone();
        if ids.is_empty() {
            return Ok(Change::None);
        }
        self.end_text();
        let tx = kiln_deck::edit::delete_shapes(&mut self.pres, &ids)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply("deck.edit.delete", tx)?;
        self.view.shapes.clear();
        Ok(Change::Content)
    }

    /// Re-routes connectors attached to `moved` shapes so their ends stay on the connection
    /// sites (as one step with the move).
    pub(crate) fn update_connectors(&mut self, moved: &[NodeId]) {
        let Some(slide) = self
            .current_slide()
            .and_then(|s| self.pres.slide(s))
            .cloned()
        else {
            return;
        };
        let site = |doc: &Self, c: &kiln_deck::Connection| -> Option<(i64, i64)> {
            let x = doc.xfrm_of(c.shape)?;
            let r = rect_px(&x);
            let props = doc.pres.resolved_props(c.shape).unwrap_or_default();
            let preset = props
                .geometry
                .as_ref()
                .and_then(Geometry::preset_name)
                .unwrap_or("rect")
                .to_owned();
            let g = preset_geometry(&preset, f64::from(r.w), f64::from(r.h), &[]).ok()?;
            let s = g.connection_sites.get(usize::try_from(c.site).ok()?)?;
            let p = shape_matrix(&x) * s.pos;
            Some((emu(p.x), emu(p.y)))
        };
        for s in &slide.shapes {
            let ShapeKind::Connector(conn) = &s.kind else {
                continue;
            };
            let touches =
                |c: &Option<kiln_deck::Connection>| c.is_some_and(|c| moved.contains(&c.shape));
            if !(touches(&conn.start) || touches(&conn.end)) || moved.contains(&s.id) {
                continue;
            }
            let Some(x0) = s.xfrm else { continue };
            // The current ends in EMU (flips say which corner is the start).
            let (l, t, r, b) = (x0.x, x0.y, x0.x + x0.cx, x0.y + x0.cy);
            let mut from = (if x0.flip_h { r } else { l }, if x0.flip_v { b } else { t });
            let mut to = (if x0.flip_h { l } else { r }, if x0.flip_v { t } else { b });
            if let Some(c) = &conn.start
                && let Some(p) = site(self, c)
            {
                from = p;
            }
            if let Some(c) = &conn.end
                && let Some(p) = site(self, c)
            {
                to = p;
            }
            let mut x = Transform::new(
                from.0.min(to.0),
                from.1.min(to.1),
                (to.0 - from.0).abs(),
                (to.1 - from.1).abs(),
            );
            x.flip_h = to.0 < from.0;
            x.flip_v = to.1 < from.1;
            if x != x0
                && let Ok(tx) = kiln_deck::edit::set_transform(&mut self.pres, s.id, x)
            {
                let _ = self.apply("deck.drawing.nudge", tx);
            }
        }
    }

    /// The connection site nearest a point (slide px) within reach, on any shape but lines
    /// and `skip`: the connection and the site's point.
    pub(crate) fn site_near(
        &self,
        x: f32,
        y: f32,
        skip: Option<NodeId>,
    ) -> Option<(kiln_deck::Connection, (f32, f32))> {
        let reach = SITE_REACH / self.zoom.max(0.05);
        let mut best: Option<(f32, kiln_deck::Connection, (f32, f32))> = None;
        for g in self.hit_list() {
            if Some(g.id) == skip
                || self.is_line(g.id)
                || !g.bounds().inflate(reach, reach).contains(x, y)
            {
                continue;
            }
            let Some(xfrm) = self.xfrm_of(g.id) else {
                continue;
            };
            let r = rect_px(&xfrm);
            let props = self.pres.resolved_props(g.id).unwrap_or_default();
            let preset = props
                .geometry
                .as_ref()
                .and_then(Geometry::preset_name)
                .unwrap_or("rect")
                .to_owned();
            let Ok(geom) = preset_geometry(&preset, f64::from(r.w), f64::from(r.h), &[]) else {
                continue;
            };
            for (i, site) in geom.connection_sites.iter().enumerate() {
                let p = shape_matrix(&xfrm) * site.pos;
                #[allow(clippy::cast_possible_truncation)]
                let q = (p.x as f32, p.y as f32);
                let d = ((q.0 - x).powi(2) + (q.1 - y).powi(2)).sqrt();
                if d <= reach && best.as_ref().is_none_or(|b| d < b.0) {
                    let site = u32::try_from(i).unwrap_or(0);
                    best = Some((d, kiln_deck::Connection { shape: g.id, site }, q));
                }
            }
            // The topmost shape with a site in reach wins.
            if best.is_some() {
                break;
            }
        }
        best.map(|(_, c, q)| (c, q))
    }

    /// Draws a line or connector from `from` to `to` (slide px); an end that lands on a
    /// connection site attaches to it.
    pub(crate) fn draw_line(
        &mut self,
        preset: &str,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Result<Change> {
        let a = self.site_near(from.0, from.1, None);
        let b = self.site_near(to.0, to.1, None);
        let e = |q: (f32, f32)| (emu(f64::from(q.0)), emu(f64::from(q.1)));
        let from = e(a.map_or(from, |(_, q)| q));
        let to = e(b.map_or(to, |(_, q)| q));
        let mut s = kiln_deck::edit::new_connector(
            &mut self.pres,
            preset,
            from,
            to,
            (a.map(|(c, _)| c), b.map(|(c, _)| c)),
        );
        s.name = format!("{} {}", preset_name(preset), s.id.0);
        self.insert_on_slide("deck.insert.shape", vec![s])?;
        Ok(Change::Content)
    }

    /// After a connector's end is dragged: each end on a connection site attaches to it, an
    /// end anywhere else comes loose.
    pub(crate) fn reattach(&mut self, id: NodeId) {
        let Some(s) = self.pres.shape(id) else { return };
        let (ShapeKind::Connector(conn), Some(x)) = (&s.kind, s.xfrm) else {
            return;
        };
        let (l, t, r, b) = (x.x, x.y, x.x + x.cx, x.y + x.cy);
        let from = (if x.flip_h { r } else { l }, if x.flip_v { b } else { t });
        let to = (if x.flip_h { l } else { r }, if x.flip_v { t } else { b });
        let p = |q: (i64, i64)| (px(q.0), px(q.1));
        let (fa, fb) = (p(from), p(to));
        let a = self.site_near(fa.0, fa.1, Some(id));
        let b = self.site_near(fb.0, fb.1, Some(id));
        let mut conn = conn.clone();
        conn.start = a.map(|(c, _)| c);
        conn.end = b.map(|(c, _)| c);
        let e = |q: (f32, f32)| (emu(f64::from(q.0)), emu(f64::from(q.1)));
        let (from, to) = (e(a.map_or(fa, |(_, q)| q)), e(b.map_or(fb, |(_, q)| q)));
        let mut nx = Transform::new(
            from.0.min(to.0),
            from.1.min(to.1),
            (to.0 - from.0).abs(),
            (to.1 - from.1).abs(),
        );
        nx.flip_h = to.0 < from.0;
        nx.flip_v = to.1 < from.1;
        let mut ops = vec![Op::SetShapeKind {
            shape: id,
            kind: Box::new(ShapeKind::Connector(conn)),
        }];
        if let Ok(tx) = kiln_deck::edit::set_transform(&mut self.pres, id, nx) {
            ops.extend(tx.ops);
        }
        let _ = self.apply("deck.drawing.resize", Transaction::new(ops));
    }

    /// Shape commands (insert, arrange, size, picture, link).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn shape_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        let ids = self.view.shapes.clone();
        let need = |ids: &[NodeId], id: &'static str| -> Result<()> {
            if ids.is_empty() {
                Err(KilnError::unavailable(id, "select an object first"))
            } else {
                Ok(())
            }
        };
        match cmd {
            D::InsertShape { preset, rect } => self.insert_shape(preset, *rect),
            D::InsertTextBox { rect } => self.insert_text_box(*rect),
            D::InsertPicture { name, bytes, rect } => self.insert_picture(name, bytes, *rect),
            D::InsertTable { rows, cols } => self.insert_table(*rows, *cols),
            D::Link { url } => self.set_link(url.clone()),
            D::Nudge { direction, fine } => self.nudge(*direction, *fine, false),
            D::Resize { direction, fine } => self.nudge(*direction, *fine, true),
            D::SetTransform {
                x,
                y,
                width,
                height,
                rotation,
                lock_aspect,
            } => {
                need(&ids, "deck.format.size_position")?;
                self.begin_group();
                let mut result = Ok(());
                for id in &ids {
                    let Some(mut t) = self.xfrm_of(*id) else {
                        continue;
                    };
                    let pt = |v: f32| emu(f64::from(v) * 4.0 / 3.0);
                    let (w0, h0) = (t.cx.max(1), t.cy.max(1));
                    if let Some(w) = width {
                        t.cx = pt(*w).max(1);
                        if *lock_aspect && height.is_none() {
                            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                            {
                                t.cy = ((t.cx as f64) * (h0 as f64) / (w0 as f64)).round() as i64;
                            }
                        }
                    }
                    if let Some(h) = height {
                        t.cy = pt(*h).max(1);
                        if *lock_aspect && width.is_none() {
                            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                            {
                                t.cx = ((t.cy as f64) * (w0 as f64) / (h0 as f64)).round() as i64;
                            }
                        }
                    }
                    if let Some(v) = x {
                        t.x = pt(*v);
                    }
                    if let Some(v) = y {
                        t.y = pt(*v);
                    }
                    if let Some(r) = rotation {
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            t.rot = ((f64::from(*r).rem_euclid(360.0) * 60_000.0).round() as i32)
                                .rem_euclid(21_600_000);
                        }
                    }
                    match kiln_deck::edit::set_transform(&mut self.pres, *id, t.clamped()) {
                        Ok(tx) => {
                            if let Err(e) = self.apply("deck.format.size_position", tx) {
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
            D::Rotate { degrees } => {
                need(&ids, "deck.arrange.rotate")?;
                #[allow(clippy::cast_possible_truncation)]
                let delta = (f64::from(*degrees) * 60_000.0).round() as i32;
                self.apply_with("deck.arrange.rotate", |p| {
                    kiln_deck::edit::rotate_by(p, &ids, delta)
                })?;
                Ok(Change::Content)
            }
            D::Flip { horizontal } => {
                need(&ids, "deck.arrange.flip")?;
                self.apply_with("deck.arrange.flip", |p| {
                    kiln_deck::edit::flip(p, &ids, *horizontal)
                })?;
                Ok(Change::Content)
            }
            D::Align { how } => {
                need(&ids, "deck.arrange.align")?;
                let how = match how {
                    DeckAlign::Left => Align::Left,
                    DeckAlign::Center => Align::Center,
                    DeckAlign::Right => Align::Right,
                    DeckAlign::Top => Align::Top,
                    DeckAlign::Middle => Align::Middle,
                    DeckAlign::Bottom => Align::Bottom,
                };
                let to = if ids.len() == 1 {
                    AlignTo::Slide
                } else {
                    AlignTo::Selection
                };
                self.begin_group();
                let r = self.apply_with("deck.arrange.align", |p| {
                    kiln_deck::edit::align(p, &ids, how, to)
                });
                self.update_connectors(&ids);
                self.end_group();
                r?;
                Ok(Change::Content)
            }
            D::Distribute { horizontal } => {
                need(&ids, "deck.arrange.distribute")?;
                let to = if ids.len() < 3 {
                    AlignTo::Slide
                } else {
                    AlignTo::Selection
                };
                self.begin_group();
                let r = self.apply_with("deck.arrange.distribute", |p| {
                    kiln_deck::edit::distribute(p, &ids, *horizontal, to)
                });
                self.update_connectors(&ids);
                self.end_group();
                r?;
                Ok(Change::Content)
            }
            D::Group => {
                if ids.len() < 2 {
                    return Err(KilnError::unavailable(
                        "deck.drawing.group",
                        "select two or more objects",
                    ));
                }
                self.end_text();
                let before: Vec<NodeId> = self
                    .stage_shapes()
                    .map(|s| s.iter().map(|x| x.id).collect())
                    .unwrap_or_default();
                self.apply_with("deck.drawing.group", |p| kiln_deck::edit::group(p, &ids))?;
                let group = self
                    .stage_shapes()
                    .and_then(|s| s.iter().map(|x| x.id).find(|id| !before.contains(id)));
                self.view.shapes = group.into_iter().collect();
                Ok(Change::Content)
            }
            D::Ungroup => {
                let groups: Vec<NodeId> = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        matches!(
                            self.pres.shape(*id).map(|s| &s.kind),
                            Some(ShapeKind::Group(_))
                        )
                    })
                    .collect();
                if groups.is_empty() {
                    return Err(KilnError::unavailable(
                        "deck.drawing.ungroup",
                        "select a group first",
                    ));
                }
                self.end_text();
                let mut children = Vec::new();
                for g in &groups {
                    if let Some(ShapeKind::Group(gr)) = self.pres.shape(*g).map(|s| &s.kind) {
                        children.extend(gr.children.iter().map(|c| c.id));
                    }
                }
                self.begin_group();
                let mut r = Ok(false);
                for g in groups {
                    r = self.apply_with("deck.drawing.ungroup", |p| kiln_deck::edit::ungroup(p, g));
                    if r.is_err() {
                        break;
                    }
                }
                self.end_group();
                r?;
                self.view.shapes = children;
                self.validate_view();
                Ok(Change::Content)
            }
            D::Order { order } => {
                need(&ids, "deck.arrange.order")?;
                self.apply_with("deck.arrange.order", |p| match order {
                    DeckOrder::Front => kiln_deck::edit::bring_to_front(p, &ids),
                    DeckOrder::Forward => kiln_deck::edit::bring_forward(p, &ids),
                    DeckOrder::Backward => kiln_deck::edit::send_backward(p, &ids),
                    DeckOrder::Back => kiln_deck::edit::send_to_back(p, &ids),
                })?;
                Ok(Change::Content)
            }
            D::Delete => {
                if self.view.text.is_some() {
                    return self.text_command(&DeckCommand::DeleteText {
                        forward: true,
                        word: false,
                    });
                }
                if ids.is_empty() || self.view.rail {
                    return self.slide_command(&DeckCommand::DeleteSlides);
                }
                self.delete_shapes()
            }
            D::SetAltText { text } => {
                need(&ids, "deck.format.alt_text")?;
                let id = ids[0];
                self.apply_with("deck.format.alt_text", |p| {
                    kiln_deck::edit::set_shape_info(p, id, |i| {
                        i.alt_text = (!text.trim().is_empty()).then(|| text.clone());
                    })
                })?;
                Ok(Change::Content)
            }
            D::Crop {
                left,
                top,
                right,
                bottom,
            } => {
                let id = self.selected_picture("deck.picture.crop")?;
                #[allow(clippy::cast_possible_truncation)]
                let f = |v: f32| (f64::from(v.clamp(-1000.0, 99.0)) * 1000.0).round() as i32;
                let crop = Crop {
                    l: f(*left),
                    t: f(*top),
                    r: f(*right),
                    b: f(*bottom),
                };
                self.apply_with("deck.picture.crop", |p| {
                    kiln_deck::edit::set_crop(p, id, crop)
                })?;
                Ok(Change::Content)
            }
            D::ResetPicture => {
                let id = self.selected_picture("deck.picture.reset")?;
                self.apply_with("deck.picture.reset", |p| {
                    kiln_deck::edit::set_crop(p, id, Crop::default())
                })?;
                Ok(Change::Content)
            }
            D::ChangeShape { preset } => {
                need(&ids, "deck.drawing.change_shape")?;
                if kiln_ooxml::drawingml::presets::preset_category(preset).is_none() {
                    return Err(KilnError::InvalidInput(format!(
                        "'{preset}' is not a shape"
                    )));
                }
                let preset = preset.clone();
                self.apply_with("deck.drawing.change_shape", |p| {
                    kiln_deck::edit::set_shape_props(p, &ids, |sp| {
                        sp.geometry = Some(Geometry::preset(&preset));
                    })
                })?;
                Ok(Change::Content)
            }
            _ => self.format_command(cmd),
        }
    }

    pub(super) fn selected_picture(&self, id: &'static str) -> Result<NodeId> {
        self.view
            .shapes
            .iter()
            .copied()
            .find(|s| {
                matches!(
                    self.pres.shape(*s).map(|x| &x.kind),
                    Some(ShapeKind::Picture(_))
                )
            })
            .ok_or_else(|| KilnError::unavailable(id, "select a picture first"))
    }

    /// A hyperlink on the selected text, or on the selected shapes.
    fn set_link(&mut self, url: Option<String>) -> Result<Change> {
        let url = url.filter(|u| !u.trim().is_empty());
        if let Some(t) = self.view.text.clone() {
            let body = self
                .body(t.target)
                .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
            let (a, b) = super::text::ordered(body, t.anchor, t.head);
            if a == b {
                // A link with no text selected inserts the address as linked text.
                let Some(u) = url else {
                    return Ok(Change::None);
                };
                self.type_text(&u)?;
                let end = self.view.text.as_ref().map_or(a, |t| t.head);
                return self.link_range(t.target, a, end, Some(u));
            }
            return self.link_range(t.target, a, b, url);
        }
        let ids = self.view.shapes.clone();
        if ids.is_empty() {
            return Err(KilnError::unavailable(
                "deck.insert.link",
                "select text or an object first",
            ));
        }
        self.begin_group();
        let mut r = Ok(false);
        for id in ids {
            let u = url.clone();
            r = self.apply_with("deck.insert.link", |p| {
                kiln_deck::edit::set_shape_info(p, id, |i| i.link = u)
            });
            if r.is_err() {
                break;
            }
        }
        self.end_group();
        r?;
        Ok(Change::Content)
    }

    fn link_range(
        &mut self,
        target: TextTarget,
        a: kiln_doc::Pos,
        b: kiln_doc::Pos,
        url: Option<String>,
    ) -> Result<Change> {
        let segments = self.segments(target, a, b);
        self.edit_body("deck.insert.link", target, |doc| {
            let mut ops = Vec::new();
            for (para, s, e) in segments {
                let Some(p) = doc.paragraph(para) else {
                    continue;
                };
                let mut marks: Vec<kiln_doc::MarkSpan> = p
                    .content
                    .marks()
                    .iter()
                    .filter(|m| {
                        !(matches!(m.mark, kiln_doc::Mark::Link(_)) && m.start < e && m.end > s)
                    })
                    .cloned()
                    .collect();
                if let Some(u) = &url {
                    marks.push(kiln_doc::MarkSpan::new(
                        s,
                        e,
                        kiln_doc::Mark::Link(kiln_doc::Hyperlink {
                            target: kiln_doc::LinkTarget::Url(u.clone()),
                            tooltip: None,
                            frame: None,
                            history: true,
                        }),
                    ));
                }
                ops.push(kiln_doc::Op::SetMarks { para, marks });
            }
            Ok(kiln_doc::Transaction::new(ops).with_kind(kiln_doc::EditKind::Format))
        })?;
        Ok(Change::Content)
    }

    /// `(paragraph, start, end)` segments of a range of a body.
    pub(crate) fn segments(
        &self,
        target: TextTarget,
        a: kiln_doc::Pos,
        b: kiln_doc::Pos,
    ) -> Vec<(NodeId, usize, usize)> {
        let Some(body) = self.body(target) else {
            return Vec::new();
        };
        let (a, b) = super::text::ordered(body, a, b);
        let mut out = Vec::new();
        let mut on = false;
        for p in body.paragraphs() {
            if p.id == a.para {
                on = true;
            }
            if on {
                let s = if p.id == a.para { a.offset } else { 0 };
                let e = if p.id == b.para {
                    b.offset
                } else {
                    p.content.len()
                };
                out.push((p.id, s, e.max(s)));
            }
            if p.id == b.para {
                break;
            }
        }
        out
    }
}
