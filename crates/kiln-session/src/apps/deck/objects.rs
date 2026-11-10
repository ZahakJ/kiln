// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paste ▸ Picture, Insert ▸ Icons, Code Block and Symbol.
//!
//! - **Paste as Picture**: what was copied — objects, a slide or text — drawn by
//!   `kiln-deck-layout` on a transparent page, cropped to what it covers and inserted as a PNG
//!   where the objects were (a slide fills the slide).
//! - **Icons**: an icon's strokes become a custom geometry, so the icon is a shape whose outline
//!   colour and weight are Shape Outline's, as in PowerPoint after Convert to Shape.
//! - **Code Block**: the code in a monospaced, shaded text box, each token coloured by Write's
//!   highlighter, so it survives a `.pptx` round trip as formatted text.
//! - **Symbol**: the characters at the caret in their symbol font.
//! - **Convert to List Diagram**: a list's first-level items as blocks, a process of boxes and
//!   arrows or stacked bars, grouped in the list's place (plain shapes, editable anywhere).

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::{
    AutoShape, Background, Geometry, Presentation, Shape, ShapeKind, ShapeProps, Slide, TextBody,
};
use kiln_deck_layout::geom::{emu, px, rect_px};
use kiln_doc::{Fragment, Inlines, Paragraph, RunProps};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::geometry::{GeometryDef, PathCommand, PathDef};
use kiln_ooxml::drawingml::line::Line;
use kiln_ooxml::st::{LineCap, PathFillMode};
use kiln_protocol::commands::deck_content::DeckListDiagram;
use kiln_protocol::commands::{ClipboardData, DeckRect};
use kiln_scene::Bounds;

use super::DeckDoc;
use super::clipboard::Clip;
use crate::model::Change;

/// Device px per slide px a pasted picture is drawn at (twice the slide's resolution).
const PICTURE_SCALE: f32 = 2.0;
/// An icon's size as it lands on the slide (1 inch).
const ICON_PX: f32 = 96.0;
/// The most characters of code a block takes.
const MAX_CODE_CHARS: usize = 20_000;
/// Icon coordinates are written in thousandths of the 24-unit box.
const ICON_UNITS: f64 = 1000.0;

fn rgb(c: u32) -> DrawingColor {
    DrawingColor::srgb(&format!("{c:06X}"))
}

/// The DrawingML path of SVG path data in a 24 × 24 box (arcs become cubic Béziers).
fn icon_path(d: &str) -> Option<Vec<PathCommand>> {
    let path = kurbo::BezPath::from_svg(d).ok()?;
    let n = |v: f64| format!("{}", (v * ICON_UNITS).round() as i64);
    let pt = |p: kurbo::Point| [n(p.x), n(p.y)];
    let mut out = Vec::new();
    for el in path.elements() {
        out.push(match *el {
            kurbo::PathEl::MoveTo(p) => {
                let [x, y] = pt(p);
                PathCommand::MoveTo(x, y)
            }
            kurbo::PathEl::LineTo(p) => {
                let [x, y] = pt(p);
                PathCommand::LineTo(x, y)
            }
            kurbo::PathEl::QuadTo(a, b) => {
                let [ax, ay] = pt(a);
                let [bx, by] = pt(b);
                PathCommand::QuadBezTo([ax, ay, bx, by])
            }
            kurbo::PathEl::CurveTo(a, b, c) => {
                let [ax, ay] = pt(a);
                let [bx, by] = pt(b);
                let [cx, cy] = pt(c);
                PathCommand::CubicBezTo([ax, ay, bx, by, cx, cy])
            }
            kurbo::PathEl::ClosePath => PathCommand::Close,
        });
    }
    (!out.is_empty()).then_some(out)
}

impl DeckDoc {
    /// Paste ▸ Picture.
    pub(crate) fn paste_picture(&mut self, data: Option<&ClipboardData>) -> Result<Change> {
        const ID: &str = "deck.clipboard.paste_picture";
        let clip = data
            .and_then(|d| d.kiln.as_deref())
            .and_then(super::clipboard::decode)
            .or_else(|| self.clip.clone());
        let text_only = data.map(|d| d.text.clone()).filter(|t| !t.is_empty());
        let Some(clip) = clip.or_else(|| {
            text_only.map(|t| Clip::Text {
                paragraphs: t
                    .split('\n')
                    .map(|line| Fragment::text(line, RunProps::default()))
                    .collect(),
            })
        }) else {
            return Err(KilnError::unavailable(
                ID,
                "copy objects, a slide or text first",
            ));
        };
        let (png, rect) = self.picture_of(&clip)?;
        self.end_text();
        self.insert_picture("Picture.png", &png, rect)
    }

    /// The picture of a clip and where it goes (`None`: fitted in the middle).
    fn picture_of(&self, clip: &Clip) -> Result<(Vec<u8>, Option<DeckRect>)> {
        let mut pres: Presentation = self.pres.clone();
        let layout = pres
            .masters
            .first()
            .and_then(|m| {
                m.layout_of_kind(&kiln_deck::LayoutKind::Blank)
                    .or_else(|| m.layouts.first())
            })
            .map(|l| l.id)
            .ok_or_else(|| KilnError::InvalidInput("no layout".into()))?;
        let (shapes, whole_slide) = match clip {
            Clip::Shapes { shapes, parts } => {
                for (k, p) in parts {
                    pres.parts.entry(k.clone()).or_insert_with(|| p.clone());
                }
                (shapes.clone(), None)
            }
            Clip::Slides { slides, parts, .. } => {
                for (k, p) in parts {
                    pres.parts.entry(k.clone()).or_insert_with(|| p.clone());
                }
                let s = slides
                    .first()
                    .ok_or_else(|| KilnError::InvalidInput("no slide".into()))?;
                (Vec::new(), Some(s.clone()))
            }
            Clip::Text { paragraphs } => {
                // One text box, no wider than it needs (no wrap), at the slide's corner.
                let mut doc_paras = Vec::new();
                for (i, f) in paragraphs.iter().enumerate() {
                    let mut p = Paragraph::new(NodeId(i as u64 + 1));
                    p.content = Inlines::from_fragment(f.clone())
                        .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                    doc_paras.push(p);
                }
                let mut body = TextBody::from_paragraphs(doc_paras, Default::default())
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                body.body.wrap = Some(kiln_deck::text::TextWrap::None);
                let mut s = Shape::text_box(
                    NodeId(0),
                    kiln_deck::Rect::new(0, 0, emu(1.0), emu(1.0)),
                    "",
                );
                if let Some(t) = s.text_mut() {
                    *t = body;
                }
                (vec![s], None)
            }
        };
        let mut slide = match whole_slide {
            Some(mut s) => {
                s.id = pres.alloc_id();
                s.comments.clear();
                s
            }
            None => {
                let mut s = Slide::new(pres.alloc_id(), layout);
                // Only the objects: no background, no master graphics.
                s.background = Some(Background::Fill {
                    fill: Fill::None,
                    shade_to_title: false,
                });
                s.show_master_shapes = false;
                s.shapes = shapes;
                s
            }
        };
        let full = slide.shapes.is_empty() || matches!(clip, Clip::Slides { .. });
        for s in &mut slide.shapes {
            renumber(&mut pres, s);
        }
        let own: Vec<NodeId> = slide.shapes.iter().map(|s| s.id).collect();
        let id = slide.id;
        pres.slides.push(slide);
        let mut renderer = kiln_deck_layout::Renderer::new(super::fonts(), super::fonts());
        let mut glyphs = kiln_deck_layout::Glyphs::new(PICTURE_SCALE);
        let drawn = renderer.draw(
            &pres,
            kiln_deck::Owner::Slide(id),
            &kiln_deck_layout::Options::default(),
            &mut glyphs,
        );
        let (sw, sh) = (px(pres.slide_size.cx), px(pres.slide_size.cy));
        let area = if full {
            kiln_scene::Bounds::new(0.0, 0.0, sw, sh)
        } else {
            drawn
                .shapes
                .iter()
                .filter(|s| own.contains(&s.id))
                .map(|s| s.bounds)
                .reduce(|a, b| a.union(&b))
                .ok_or_else(|| KilnError::InvalidInput("nothing to draw".into()))?
        };
        let ops = if full {
            kiln_deck_layout::scene::slide_ops(&drawn)
        } else {
            drawn
                .shapes
                .iter()
                .filter(|s| own.contains(&s.id))
                .flat_map(|s| s.ops.iter().cloned())
                .collect()
        };
        let mut uploads = kiln_deck_layout::picture::Uploads::default();
        let images = uploads.take(&renderer.pictures, &drawn.images, true);
        let delta = kiln_scene::SceneDelta {
            revision: 1,
            reset: true,
            scale: PICTURE_SCALE,
            extent: area,
            background: kiln_scene::Paint::Rgba(0),
            tiles: vec![kiln_scene::Tile::new(
                kiln_scene::TileId::content(0),
                area,
                ops,
            )],
            removed: Vec::new(),
            atlas: glyphs.updates(true),
            images,
            overlay: None,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            (area.w * PICTURE_SCALE).ceil().clamp(1.0, 8192.0) as u32,
            (area.h * PICTURE_SCALE).ceil().clamp(1.0, 8192.0) as u32,
        );
        let rgba = kiln_raster::render_delta(
            &delta,
            &kiln_raster::View {
                width: w,
                height: h,
                scroll_x: area.x * PICTURE_SCALE,
                scroll_y: area.y * PICTURE_SCALE,
                options: kiln_raster::PaintOptions {
                    caret_visible: false,
                    touch: false,
                },
            },
            &kiln_scene::Palette::light().for_app("deck"),
        )
        .map_err(|e| KilnError::Internal(e.to_string()))?;
        let png = rgba.encode_png().map_err(KilnError::Internal)?;
        // Objects go back where they were; a slide or text is fitted in the middle.
        let rect = matches!(clip, Clip::Shapes { .. }).then_some(DeckRect {
            x: area.x,
            y: area.y,
            width: area.w,
            height: area.h,
        });
        Ok((png, rect))
    }

    /// Insert ▸ Icons.
    pub(crate) fn insert_icon(&mut self, name: &str, paths: &[String]) -> Result<Change> {
        let mut path = Vec::new();
        for d in paths.iter().take(64) {
            if d.len() > 16_384 {
                return Err(KilnError::InvalidInput("an icon's path is too long".into()));
            }
            path.extend(
                icon_path(d).ok_or_else(|| {
                    KilnError::InvalidInput(format!("{d:?} is not SVG path data"))
                })?,
            );
        }
        if path.is_empty() {
            return Err(KilnError::InvalidInput("an icon needs a path".into()));
        }
        let geometry = GeometryDef {
            path: vec![PathDef {
                w: Some(24.0 * ICON_UNITS),
                h: Some(24.0 * ICON_UNITS),
                fill: Some(PathFillMode::None),
                cmds: path,
                ..PathDef::default()
            }],
            ..GeometryDef::default()
        };
        // Two units of the 24 are the stroke: 6 pt at an inch, in the theme's text colour.
        let line = Line {
            width: Some(emu(f64::from(ICON_PX) / 12.0)),
            cap: Some(LineCap::Rnd),
            join: Some(kiln_ooxml::drawingml::line::LineJoin::Round),
            fill: Some(Fill::Solid(DrawingColor::scheme("tx1"))),
            ..Line::default()
        };
        // Icons inserted one after another go in a row from the middle: right, left, further.
        let n = self.stage_shapes().map_or(0, |v| {
            v.iter().filter(|s| s.name.starts_with("Graphic ")).count()
        });
        let k = i32::try_from(n % 7).unwrap_or(0);
        let slot = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
        let mut r = self.centred(ICON_PX, ICON_PX);
        #[allow(clippy::cast_precision_loss)]
        {
            r.x += slot as f32 * (ICON_PX + 24.0);
        }
        let id = self.pres.alloc_id();
        let mut shape = Shape::new(
            id,
            format!("Graphic {} {}", id.0, name.trim()),
            Some(kiln_deck::Transform::from_rect(super::shapes::rect_emu(&r))),
            ShapeKind::Geometry(AutoShape {
                text_box: false,
                props: ShapeProps {
                    geometry: Some(Geometry::Custom(geometry)),
                    fill: Some(Fill::None),
                    line: Some(line),
                    ..ShapeProps::default()
                },
                text: None,
            }),
        );
        shape.alt_text = Some(name.trim().to_owned());
        self.end_text();
        self.insert_on_slide("deck.insert.icon", vec![shape])?;
        Ok(Change::Content)
    }

    /// Insert ▸ Code Block.
    pub(crate) fn insert_code_block(
        &mut self,
        code: &str,
        language: Option<&str>,
    ) -> Result<Change> {
        let code = code.replace("\r\n", "\n").replace('\t', "    ");
        let code = code.trim_end_matches('\n');
        if code.chars().count() > MAX_CODE_CHARS {
            return Err(KilnError::InvalidInput(format!(
                "a code block holds at most {MAX_CODE_CHARS} characters"
            )));
        }
        let base = RunProps {
            fonts: Some(kiln_doc::RunFonts::latin(
                crate::apps::write::features::blocks::CODE_FONT,
            )),
            size: Some(32),
            color: Some(kiln_doc::DocColor::rgb(0x002E_2A27)),
            ..RunProps::default()
        };
        let lines: Vec<&str> = if code.is_empty() {
            vec![""]
        } else {
            code.split('\n').collect()
        };
        let mut paragraphs = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let len = line.chars().count();
            let runs = crate::apps::write::features::blocks::code_runs(line, len)
                .into_iter()
                .map(|span| {
                    kiln_doc::RunSpan::new(
                        span.len,
                        RunProps {
                            color: span.props.color.clone().or_else(|| base.color.clone()),
                            italic: span.props.italic,
                            ..base.clone()
                        },
                    )
                })
                .collect();
            let frag = Fragment {
                text: (*line).to_owned(),
                runs,
                marks: Vec::new(),
                objects: Vec::new(),
            };
            let mut p = Paragraph::new(NodeId(i as u64 + 1));
            p.content =
                Inlines::from_fragment(frag).map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            p.props.alignment = Some(kiln_doc::st::Alignment::Left);
            paragraphs.push(p);
        }
        let mut body = TextBody::from_paragraphs(paragraphs, Default::default())
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        body.body.wrap = Some(kiln_deck::text::TextWrap::Square);
        body.body.autofit = Some(kiln_deck::text::Autofit::ResizeShape);
        body.body.anchor = Some(kiln_deck::TextAnchor::Top);
        body.body.insets = [
            Some(emu(14.0)),
            Some(emu(10.0)),
            Some(emu(14.0)),
            Some(emu(10.0)),
        ];
        let (sw, _) = self.slide_px();
        #[allow(clippy::cast_precision_loss)]
        let height = (lines.len() as f32 * 30.0 + 24.0).min(600.0);
        let r = self.centred((sw * 0.6).max(320.0), height);
        let id = self.pres.alloc_id();
        let lang = language.map(str::trim).filter(|l| !l.is_empty());
        let mut shape = Shape::text_box(id, super::shapes::rect_emu(&r), "");
        shape.name = match lang {
            Some(l) => format!("Code Block {} ({l})", id.0),
            None => format!("Code Block {}", id.0),
        };
        if let ShapeKind::Geometry(a) = &mut shape.kind {
            a.props.fill = Some(Fill::Solid(rgb(0x00F6_F4F1)));
            a.props.line = Some(Line {
                width: Some(emu(1.0)),
                fill: Some(Fill::Solid(rgb(0x00D9_D3CB))),
                ..Line::default()
            });
            a.props.geometry = Some(Geometry::preset("rect"));
            a.text = Some(body);
        }
        self.end_text();
        self.insert_on_slide("deck.insert.code_block", vec![shape])?;
        Ok(Change::Content)
    }

    /// Insert ▸ Symbol: the characters at the caret (or in a new text box), in their font.
    pub(crate) fn insert_symbol(&mut self, text: &str, font: Option<&str>) -> Result<Change> {
        if text.is_empty() || text.chars().count() > 64 {
            return Err(KilnError::InvalidInput(
                "a symbol is 1 to 64 characters".into(),
            ));
        }
        self.begin_group();
        let r = (|| {
            if self.view.text.is_none() {
                self.insert_text_box(None)?;
            }
            let start = self
                .view
                .text
                .as_ref()
                .and_then(|t| Some(super::text::ordered(self.body(t.target)?, t.anchor, t.head).0))
                .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
            self.type_text(text)?;
            if let Some(f) = font.map(str::trim).filter(|f| !f.is_empty())
                && let Some(t) = self.view.text.as_mut()
            {
                let end = t.head;
                t.anchor = start;
                self.edit_command(&kiln_protocol::commands::EditCommand::FontFamily {
                    family: f.to_owned(),
                })?;
                if let Some(t) = self.view.text.as_mut() {
                    t.anchor = end;
                    t.head = end;
                }
            }
            Ok::<(), KilnError>(())
        })();
        self.end_group();
        r?;
        Ok(Change::Content)
    }

    /// Home ▸ Convert to List Diagram: the list of the selected shape (or the one being edited)
    /// as grouped shapes in its place, one per first-level item.
    pub(crate) fn convert_to_diagram(&mut self, layout: DeckListDiagram) -> Result<Change> {
        const ID: &str = "deck.paragraph.convert_to_diagram";
        let source = self
            .view
            .text
            .as_ref()
            .map(|t| t.shape)
            .or_else(|| self.view.shapes.first().copied())
            .ok_or_else(|| {
                KilnError::unavailable(ID, "select a text box or placeholder with a list")
            })?;
        let (Some(body), Some(frame)) = (
            self.pres.shape(source).and_then(Shape::text).cloned(),
            self.xfrm_of(source),
        ) else {
            return Err(KilnError::unavailable(
                ID,
                "select a text box or placeholder with a list",
            ));
        };
        let items = list_items(&body);
        if items.is_empty() {
            return Err(KilnError::unavailable(
                ID,
                "the shape has no text to convert",
            ));
        }
        let boxes = diagram_boxes(layout, rect_px(&frame), items.len());
        let mut shapes = Vec::new();
        for (i, (item, r)) in items.into_iter().zip(&boxes).enumerate() {
            if i > 0 && layout == DeckListDiagram::Process {
                // The arrow between this box and the one before.
                let prev = boxes[i - 1];
                let gap = r.x - (prev.x + prev.w);
                let (w, h) = (gap * 0.6, (r.h * 0.3).min(gap));
                let arrow = Bounds::new(prev.x + prev.w + gap * 0.2, r.y + (r.h - h) / 2.0, w, h);
                let id = self.pres.alloc_id();
                let mut a = Shape::geometry(id, "rightArrow", rect_of(arrow));
                a.name = format!("Arrow {}", id.0);
                if let ShapeKind::Geometry(g) = &mut a.kind {
                    g.props.fill = Some(Fill::Solid(super::slides::drawing_color(
                        "accent1",
                        Some(40.0),
                        None,
                    )));
                }
                shapes.push(a);
            }
            let id = self.pres.alloc_id();
            let preset = if layout == DeckListDiagram::Process {
                "rect"
            } else {
                "roundRect"
            };
            let mut shape = Shape::geometry(id, preset, rect_of(*r));
            shape.name = format!("Item {}", i + 1);
            if let ShapeKind::Geometry(g) = &mut shape.kind {
                let mut text = item;
                text.body.anchor = Some(kiln_deck::TextAnchor::Middle);
                text.body.autofit = Some(kiln_deck::text::Autofit::Shrink {
                    font_scale: 100_000,
                    line_reduction: 0,
                });
                g.text = Some(text);
            }
            shapes.push(shape);
        }
        self.end_text();
        self.begin_group();
        let r = (|| {
            let ids = self.insert_on_slide(ID, shapes)?;
            self.apply_with(ID, |p| kiln_deck::edit::delete_shapes(p, &[source]))?;
            if ids.len() > 1 {
                let before: Vec<NodeId> = self
                    .stage_shapes()
                    .map(|s| s.iter().map(|x| x.id).collect())
                    .unwrap_or_default();
                self.apply_with(ID, |p| kiln_deck::edit::group(p, &ids))?;
                let group = self
                    .stage_shapes()
                    .and_then(|s| s.iter().map(|x| x.id).find(|id| !before.contains(id)));
                if let Some(g) = group {
                    let n = self.stage_shapes().map_or(0, |v| {
                        v.iter()
                            .filter(|s| s.name.starts_with("List Diagram"))
                            .count()
                    });
                    let name = format!("List Diagram {}", n + 1);
                    self.apply_with(ID, |p| {
                        kiln_deck::edit::set_shape_info(p, g, |i| i.name = name.clone())
                    })?;
                }
                self.view.shapes = group.into_iter().collect();
            }
            Ok::<(), KilnError>(())
        })();
        self.end_group();
        r?;
        Ok(Change::Content)
    }
}

/// A list's first-level items, each with the sub-items under it (levels moved up one, centred).
fn list_items(body: &TextBody) -> Vec<TextBody> {
    let mut groups: Vec<Vec<(Paragraph, Option<kiln_deck::text::DeckParaProps>)>> = Vec::new();
    for p in body.paragraphs() {
        let level = TextBody::level_of(p);
        if p.content.is_empty() && level == 0 {
            continue;
        }
        if level == 0 || groups.is_empty() {
            groups.push(Vec::new());
        }
        let mut q = p.clone();
        q.props.outline_level = (level > 0).then(|| u8::try_from(level - 1).unwrap_or(0));
        if level == 0 {
            q.props.alignment = Some(kiln_doc::st::Alignment::Center);
        }
        let mut extra = body.deck_props(p.id).cloned();
        if let Some(e) = &mut extra {
            // Items carry no bullet; sub-items keep theirs.
            if level == 0 {
                e.bullet = Some(kiln_deck::text::Bullet::None);
            }
        }
        if let Some(g) = groups.last_mut() {
            g.push((q, extra));
        }
    }
    groups
        .into_iter()
        .filter_map(|g| {
            let mut paragraphs = Vec::new();
            let mut props = std::collections::BTreeMap::new();
            for (i, (mut p, extra)) in g.into_iter().enumerate() {
                p.id = NodeId(i as u64 + 1);
                if let Some(e) = extra {
                    props.insert(p.id, e);
                }
                paragraphs.push(p);
            }
            TextBody::from_paragraphs(paragraphs, props).ok()
        })
        .collect()
}

/// The boxes of a diagram's `n` items in an area (slide px).
fn diagram_boxes(layout: DeckListDiagram, area: Bounds, n: usize) -> Vec<Bounds> {
    #[allow(clippy::cast_precision_loss)]
    let count = n.max(1) as f32;
    match layout {
        DeckListDiagram::BlockList => {
            // Columns that make the blocks closest to 3:2.
            let cols = (1..=n.max(1))
                .min_by(|a, b| {
                    let ratio = |c: usize| {
                        let rows = n.div_ceil(c) as f32;
                        let (w, h) = (area.w / c as f32, area.h / rows);
                        ((w / h.max(1.0)) / 1.5).ln().abs()
                    };
                    ratio(*a).total_cmp(&ratio(*b))
                })
                .unwrap_or(1);
            let rows = n.div_ceil(cols);
            #[allow(clippy::cast_precision_loss)]
            let (cw, ch) = (area.w / cols as f32, area.h / rows as f32);
            let gap = cw.min(ch) * 0.08;
            (0..n)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let (c, r) = ((i % cols) as f32, (i / cols) as f32);
                    Bounds::new(
                        area.x + c * cw + gap / 2.0,
                        area.y + r * ch + gap / 2.0,
                        cw - gap,
                        ch - gap,
                    )
                })
                .collect()
        }
        DeckListDiagram::Process => {
            // Boxes and gaps of 2:1 across; the boxes no taller than they are wide.
            let unit = area.w / (count * 2.0 + (count - 1.0));
            let w = unit * 2.0;
            let h = (area.h * 0.6).min(w * 1.2);
            (0..n)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = area.x + i as f32 * unit * 3.0;
                    Bounds::new(x, area.y + (area.h - h) / 2.0, w, h)
                })
                .collect()
        }
        DeckListDiagram::VerticalList => {
            let ch = area.h / count;
            let gap = ch * 0.12;
            (0..n)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let y = area.y + i as f32 * ch;
                    Bounds::new(area.x, y + gap / 2.0, area.w, ch - gap)
                })
                .collect()
        }
    }
}

/// A box in slide px as a model rectangle.
fn rect_of(b: Bounds) -> kiln_deck::Rect {
    kiln_deck::Rect::new(
        emu(f64::from(b.x)),
        emu(f64::from(b.y)),
        emu(f64::from(b.w.max(1.0))),
        emu(f64::from(b.h.max(1.0))),
    )
}

/// New ids for a shape and its children.
fn renumber(pres: &mut Presentation, s: &mut Shape) {
    s.id = pres.alloc_id();
    if let Some(c) = s.children_mut() {
        for x in c {
            renumber(pres, x);
        }
    }
}
