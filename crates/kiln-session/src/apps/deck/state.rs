// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the Deck chrome asks (`Query::Deck`): the rail's slides and sections, the selection and
//! its formatting, the layouts and themes, notes and the View toggles; thumbnails of slides,
//! layouts and themes as Scenes; the pointer's cursor; the outline.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::themes::bundled;
use kiln_deck::{
    Background, Owner, PlaceholderType, Presentation, ShapeKind, SlideSizeKind, TextBody,
};
use kiln_deck_layout::geom::{px, rect_px};
use kiln_deck_layout::select::{Grip, Handle, grip_at};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_protocol::commands::{
    DeckAnchor, DeckAutofit, DeckBox, DeckCursor, DeckFormat, DeckHeaderFooter, DeckLayoutInfo,
    DeckOutlineSlide, DeckQuery, DeckQueryResult, DeckRect, DeckSectionInfo, DeckShapeInfo,
    DeckSlideInfo, DeckState, DeckTextDirection,
};

use super::DeckDoc;

fn hex(c: u32) -> String {
    format!("{:06X}", c >> 8)
}

fn kind_label(s: &kiln_deck::Shape) -> &'static str {
    if s.placeholder.is_some() {
        return "placeholder";
    }
    match &s.kind {
        ShapeKind::Geometry(a) if a.text_box => "text box",
        ShapeKind::Geometry(_) => "shape",
        ShapeKind::Picture(_) => "picture",
        ShapeKind::Group(_) => "group",
        ShapeKind::Table(_) => "table",
        ShapeKind::Chart(_) => "chart",
        ShapeKind::Connector(_) => "connector",
        ShapeKind::Media(_) => "media",
    }
}

/// The resize cursor of a handle on a shape turned by `deg`: 0 `ns`, 1 `nesw`, 2 `ew`, 3 `nwse`.
fn resize_cursor(h: Handle, deg: f64) -> u8 {
    let base = match h {
        Handle::N | Handle::S => 0.0,
        Handle::NE | Handle::SW => 45.0,
        Handle::E | Handle::W => 90.0,
        Handle::NW | Handle::SE => 135.0,
    };
    let a = (base + deg).rem_euclid(180.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let k = ((a + 22.5) / 45.0).floor() as u8 % 4;
    k
}

/// Every preset as an icon outline, by gallery category.
fn shape_icons(size: f32) -> Vec<kiln_protocol::commands::DeckShapeIcon> {
    use kiln_ooxml::drawingml::presets::{
        PresetCategory as C, preset_category, preset_names, preset_to_svg_path,
    };
    let s = f64::from(size);
    preset_names()
        .filter_map(|name| {
            let cat = preset_category(name)?;
            // Lines are drawn corner to corner; other shapes fill the box with a margin.
            let (w, h) = match name {
                n if n.contains("Connector") || n == "line" => (s, s),
                _ => (s, s * 0.75),
            };
            let path = preset_to_svg_path(name, w, h, &[]).ok()?;
            Some(kiln_protocol::commands::DeckShapeIcon {
                preset: name.to_owned(),
                label: kiln_deck_layout::names::preset_display_name(name),
                category: match cat {
                    C::LinesConnectors => "lines",
                    C::Basic => "basic",
                    C::Arrows => "arrows",
                    C::Math => "math",
                    C::Flowchart => "flowchart",
                    C::StarsBanners => "stars",
                    C::Callouts => "callouts",
                    C::ActionButtons => "actions",
                }
                .to_owned(),
                path,
            })
        })
        .collect()
}

impl DeckDoc {
    /// Answers a Deck query.
    pub(crate) fn deck_query(&self, q: &DeckQuery) -> Result<DeckQueryResult> {
        Ok(match q {
            DeckQuery::State => DeckQueryResult::State(Box::new(self.deck_state())),
            DeckQuery::Thumbnails {
                from,
                count,
                width,
                dpr,
            } => {
                let from = usize::try_from(*from).unwrap_or(0);
                let count = usize::try_from(*count).unwrap_or(0).min(64);
                let owners: Vec<(u32, Owner)> = self
                    .pres
                    .slides
                    .iter()
                    .enumerate()
                    .skip(from)
                    .take(count)
                    .map(|(i, s)| (u32::try_from(i).unwrap_or(0), Owner::Slide(s.id)))
                    .collect();
                let delta = kiln_deck_layout::scene::thumbnails(
                    &mut self.renderer.borrow_mut(),
                    &self.pres,
                    &owners,
                    width.clamp(16.0, 1600.0),
                    *dpr,
                    self.epoch,
                );
                DeckQueryResult::Scene(Box::new(delta))
            }
            DeckQuery::LayoutThumbnails { width, dpr } => {
                let owners: Vec<(u32, Owner)> = self
                    .master()
                    .map(|m| {
                        m.layouts
                            .iter()
                            .enumerate()
                            .map(|(i, l)| (u32::try_from(i).unwrap_or(0), Owner::Layout(l.id)))
                            .collect()
                    })
                    .unwrap_or_default();
                let delta = kiln_deck_layout::scene::thumbnails(
                    &mut self.renderer.borrow_mut(),
                    &self.pres,
                    &owners,
                    width.clamp(16.0, 800.0),
                    *dpr,
                    self.epoch,
                );
                DeckQueryResult::Scene(Box::new(delta))
            }
            DeckQuery::ThemeThumbnails { width, dpr } => {
                DeckQueryResult::Scene(Box::new(self.theme_thumbnails(*width, *dpr)))
            }
            DeckQuery::VariantThumbnails { width, dpr } => {
                DeckQueryResult::Scene(Box::new(self.variant_thumbnails(*width, *dpr)))
            }
            DeckQuery::MasterThumbnails { width, dpr } => {
                DeckQueryResult::Scene(Box::new(self.master_thumbnails(*width, *dpr)))
            }
            DeckQuery::HitTest { x, y } => DeckQueryResult::Cursor(self.cursor_at(*x, *y)),
            DeckQuery::Outline => DeckQueryResult::Outline(self.outline()),
            DeckQuery::ShapeIcons { size } => {
                DeckQueryResult::Icons(shape_icons(size.clamp(8.0, 128.0)))
            }
            DeckQuery::ShowFrame { .. } => match self.show_frame.borrow_mut().take() {
                Some(f) => DeckQueryResult::ShowFrame(Box::new(f)),
                None => {
                    return Err(KilnError::InvalidInput("no slide show is running".into()));
                }
            },
            DeckQuery::ShowInfo => DeckQueryResult::ShowInfo(Box::new(self.show_info())),
            DeckQuery::Snapshot => {
                DeckQueryResult::Bytes(self.write(kiln_protocol::FileFormat::Pptx, None)?)
            }
            // Design Ideas, picture export, Outline View, Reuse Slides.
            DeckQuery::DesignIdeas { width, dpr } => {
                DeckQueryResult::DesignIdeas(self.design_ideas(*width, *dpr)?)
            }
            DeckQuery::ExportPictures {
                slides,
                format,
                width,
            } => DeckQueryResult::File(self.export_pictures(slides.as_deref(), *format, *width)?),
            DeckQuery::OutlineView => DeckQueryResult::OutlineView(self.outline_view()),
            DeckQuery::ReuseList { bytes, width, dpr } => {
                DeckQueryResult::ReuseList(self.reuse_list(bytes, *width, *dpr)?)
            }
            DeckQuery::SlideMedia => DeckQueryResult::SlideMedia(self.slide_media()?),
            DeckQuery::MediaBytes { part } => DeckQueryResult::Bytes(self.media_bytes(part)?),
            DeckQuery::ChartData => DeckQueryResult::ChartData(self.chart_data()),
            DeckQuery::NotesPage => DeckQueryResult::NotesPage(self.notes_page()),
            DeckQuery::SlideText => DeckQueryResult::SlideText(self.slide_text()),
        })
    }

    /// Each bundled theme drawn on a title slide ("Aa", as PowerPoint's gallery).
    fn theme_thumbnails(&self, width: f32, dpr: f32) -> kiln_scene::SceneDelta {
        let size = self.pres.slide_size.clone();
        let samples = bundled()
            .iter()
            .enumerate()
            .filter_map(|(i, t)| Some((u32::try_from(i).ok()?, title_sample(t, &size, None)?)))
            .collect();
        self.sample_thumbnails(samples, width, dpr)
    }

    /// The current bundled theme in each of its variants (none for another theme).
    pub(crate) fn variant_thumbnails(&self, width: f32, dpr: f32) -> kiln_scene::SceneDelta {
        let size = self.pres.slide_size.clone();
        let samples = self
            .bundled_of()
            .map(|t| {
                (0..t.variants.len())
                    .filter_map(|v| {
                        Some((u32::try_from(v).ok()?, title_sample(&t, &size, Some(v))?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.sample_thumbnails(samples, width, dpr)
    }

    /// The first slide of each sample presentation, `width` CSS px wide, stacked into one scene
    /// (tile index = the sample's number) with one glyph atlas.
    fn sample_thumbnails(
        &self,
        samples: Vec<(u32, Presentation)>,
        width: f32,
        dpr: f32,
    ) -> kiln_scene::SceneDelta {
        let size = self.pres.slide_size.clone();
        let mut tiles = Vec::new();
        let mut images = Vec::new();
        let mut extent = kiln_scene::Bounds::ZERO;
        let mut renderer = self.renderer.borrow_mut();
        let slide_w = px(size.cx).max(1.0);
        let slide_h = px(size.cy).max(1.0);
        let scale = (width.max(16.0) / slide_w) * dpr.clamp(0.5, 4.0);
        let mut glyphs = kiln_deck_layout::Glyphs::new(scale);
        let mut uploads = kiln_deck_layout::picture::Uploads::default();
        for (k, (index, p)) in samples.iter().enumerate() {
            let Some(slide) = p.slides.first().map(|s| s.id) else {
                continue;
            };
            let drawn = renderer.draw(
                p,
                Owner::Slide(slide),
                &kiln_deck_layout::Options::default(),
                &mut glyphs,
            );
            #[allow(clippy::cast_precision_loss)]
            let y = k as f32 * (slide_h + kiln_deck_layout::scene::THUMB_GAP);
            let rect = kiln_scene::Bounds::new(0.0, y, slide_w, slide_h);
            extent = extent.union(&rect);
            images.extend(uploads.take(&renderer.pictures, &drawn.images, false));
            tiles.push(kiln_scene::Tile::new(
                kiln_scene::TileId::content(*index),
                rect,
                kiln_deck_layout::scene::slide_ops(&drawn),
            ));
        }
        let atlas = glyphs.updates(true);
        kiln_scene::SceneDelta {
            revision: self.epoch,
            reset: true,
            scale,
            extent,
            background: kiln_scene::Paint::Rgba(0),
            tiles,
            removed: Vec::new(),
            atlas,
            images,
            overlay: None,
        }
    }

    /// The pointer's cursor at a slide point.
    fn cursor_at(&self, x: f32, y: f32) -> DeckCursor {
        if self.view.armed.is_some() {
            return DeckCursor::Crosshair;
        }
        if let Some(sx) = self.selection_xfrm()
            && self.view.text.is_none()
        {
            let rotatable =
                self.view.shapes.len() == 1 && !self.view.shapes.iter().any(|id| self.is_line(*id));
            match grip_at(&sx, x, y, self.zoom, rotatable) {
                Some(Grip::Size(h)) => {
                    return DeckCursor::Resize(resize_cursor(h, f64::from(sx.rot) / 60_000.0));
                }
                Some(Grip::Rotate) => return DeckCursor::Rotate,
                _ => {}
            }
        }
        if let Some(t) = &self.view.text
            && let Some(g) = self.renderer.borrow().shape_geom(&self.pres, t.shape)
            && g.hit(x, y, 0.0)
        {
            return DeckCursor::Text;
        }
        match self.shape_at(x, y) {
            Some(g) => {
                let text_like = self.pres.shape(g.id).is_some_and(|s| match &s.kind {
                    ShapeKind::Geometry(a) => a.text_box || s.placeholder.is_some(),
                    ShapeKind::Table(_) => true,
                    _ => false,
                });
                let r = rect_px(&g.xfrm);
                let inside = kiln_deck_layout::render::invert(g.matrix).is_some_and(|inv| {
                    let q = inv * kurbo::Point::new(f64::from(x), f64::from(y));
                    let b = 6.0 / f64::from(self.zoom.max(0.05));
                    q.x > b && q.y > b && q.x < f64::from(r.w) - b && q.y < f64::from(r.h) - b
                });
                if text_like && inside {
                    DeckCursor::Text
                } else {
                    DeckCursor::Move
                }
            }
            None => DeckCursor::Default,
        }
    }

    /// The outline view's text.
    fn outline(&self) -> Vec<DeckOutlineSlide> {
        self.pres
            .slides
            .iter()
            .map(|s| {
                let title = kiln_deck_layout::a11y::slide_title(&self.pres, s.id);
                let mut lines = Vec::new();
                for sh in &s.shapes {
                    let is_body = sh
                        .placeholder
                        .is_some_and(|p| !p.kind.is_title() && !p.kind.is_footer_area());
                    if !is_body {
                        continue;
                    }
                    if let Some(t) = sh.text() {
                        for p in t.paragraphs() {
                            let text = p.content.plain_text(true);
                            if !text.trim().is_empty() {
                                lines
                                    .push((u8::try_from(TextBody::level_of(p)).unwrap_or(0), text));
                            }
                        }
                    }
                }
                DeckOutlineSlide { title, lines }
            })
            .collect()
    }

    /// The text area of the shape being edited (its geometry's text box less the insets), slide
    /// px, for the ruler.
    fn text_area(&self) -> Option<DeckRect> {
        let t = self.view.text.as_ref()?;
        if !matches!(t.target, kiln_deck::ops::TextTarget::Shape(_)) {
            return None;
        }
        let g = self.renderer.borrow().shape_geom(&self.pres, t.shape)?;
        let tr = g.text_rect?;
        let r = rect_px(&g.xfrm);
        let insets = self
            .pres
            .resolved_body(t.shape)
            .map_or([91_440, 45_720, 91_440, 45_720], |b| b.insets);
        let [l, top, right, bottom] = insets.map(px);
        Some(DeckRect {
            x: r.x + tr.x + l,
            y: r.y + tr.y + top,
            width: (tr.w - l - right).max(0.0),
            height: (tr.h - top - bottom).max(0.0),
        })
    }

    /// The formatting the ribbon shows.
    fn deck_format(&self) -> Option<DeckFormat> {
        let run = self.effective_run();
        let para = self.effective_paragraph();
        let shape = self.view.shapes.first().copied();
        if run.is_none() && shape.is_none() {
            return None;
        }
        let run = run.unwrap_or_default();
        let ctx = self
            .owner()
            .and_then(|o| self.pres.color_context(o))
            .unwrap_or_default();
        let theme = self.master().map(|m| m.theme.clone());
        let body = shape.and_then(|s| self.pres.resolved_body(s));
        let props = shape.and_then(|s| self.pres.resolved_props(s));
        let look = theme
            .as_ref()
            .map(|t| kiln_deck_layout::paint::Look { theme: t, ctx });
        let paintable = match (props.as_ref(), look.as_ref()) {
            (Some(p), Some(l)) => {
                kiln_deck_layout::paint::resolve_fill(p.fill.as_ref(), p.style.as_ref(), l)
            }
            _ => kiln_deck_layout::paint::Paintable::None,
        };
        let (fill, fill_kind) = {
            use kiln_deck_layout::paint::Paintable as P;
            match paintable {
                P::Solid(c) => (Some(hex(c)), "solid"),
                P::Gradient { .. } => (None, "gradient"),
                P::Picture { .. } => (None, "picture"),
                P::Pattern { .. } => (None, "pattern"),
                P::None => (None, "none"),
            }
        };
        let line = match (props.as_ref(), look.as_ref()) {
            (Some(p), Some(l)) => {
                kiln_deck_layout::paint::resolve_line(p.line.as_ref(), p.style.as_ref(), l)
            }
            _ => None,
        };
        let (bullets, numbering) =
            super::format::bullet_kind(para.as_ref().and_then(|p| p.deck.bullet.as_ref()));
        let align = match para.as_ref().and_then(|p| p.para.alignment) {
            Some(kiln_doc::st::Alignment::Center) => "center",
            Some(kiln_doc::st::Alignment::Right) => "right",
            Some(kiln_doc::st::Alignment::Both | kiln_doc::st::Alignment::Distribute) => "justify",
            _ => "left",
        };
        #[allow(clippy::cast_precision_loss)]
        Some(DeckFormat {
            font_family: theme
                .as_ref()
                .map(|t| kiln_deck_layout::text::family(&run, t))
                .unwrap_or_else(|| "Calibri".into()),
            font_size: run.size.map_or(18.0, |h| h as f32 / 2.0),
            bold: run.bold == Some(true),
            italic: run.italic == Some(true),
            underline: run
                .underline
                .as_ref()
                .is_some_and(|u| u.style != kiln_doc::st::UnderlineStyle::None),
            strike: run.strike == Some(true),
            superscript: run.vert_align == Some(kiln_doc::st::VertAlign::Superscript),
            subscript: run.vert_align == Some(kiln_doc::st::VertAlign::Subscript),
            shadow: run.shadow == Some(true),
            color: Some(hex(kiln_deck_layout::text::run_color(&run, &ctx))),
            align: align.into(),
            bullets,
            numbering,
            anchor: match body.map(|b| b.anchor) {
                Some(kiln_deck::TextAnchor::Middle) => DeckAnchor::Middle,
                Some(kiln_deck::TextAnchor::Bottom) => DeckAnchor::Bottom,
                _ => DeckAnchor::Top,
            },
            direction: match body.map(|b| b.direction) {
                Some(
                    kiln_deck::text::TextDirection::Rotate90
                    | kiln_deck::text::TextDirection::EastAsianVertical,
                ) => DeckTextDirection::Rotate90,
                Some(
                    kiln_deck::text::TextDirection::Rotate270
                    | kiln_deck::text::TextDirection::MongolianVertical,
                ) => DeckTextDirection::Rotate270,
                Some(
                    kiln_deck::text::TextDirection::Stacked
                    | kiln_deck::text::TextDirection::StackedRtl,
                ) => DeckTextDirection::Stacked,
                _ => DeckTextDirection::Horizontal,
            },
            autofit: match body.map(|b| b.autofit) {
                Some(kiln_deck::Autofit::Shrink { .. }) => DeckAutofit::Shrink,
                Some(kiln_deck::Autofit::ResizeShape) => DeckAutofit::ResizeShape,
                _ => DeckAutofit::Off,
            },
            columns: body.map_or(1, |b| b.columns),
            spacing: run.spacing.map_or(0.0, |tw| tw as f32 / 20.0),
            fill,
            fill_kind: fill_kind.into(),
            outline: line.as_ref().map(|l| hex(l.paint)),
            outline_width: line.as_ref().map(|l| l.width * 0.75),
            indent_left: para
                .as_ref()
                .and_then(|p| p.para.indent.as_ref())
                .and_then(|i| i.left)
                .map_or(0.0, |t| t as f32 / 20.0),
            indent_first: para
                .as_ref()
                .and_then(|p| p.para.indent.as_ref())
                .and_then(|i| i.first_line)
                .map_or(0.0, |t| t as f32 / 20.0),
        })
    }

    /// The chrome's state.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn deck_state(&self) -> DeckState {
        let pres = &self.pres;
        let slides = pres
            .slides
            .iter()
            .map(|s| DeckSlideInfo {
                id: s.id,
                title: kiln_deck_layout::a11y::slide_title(pres, s.id),
                hidden: s.hidden,
                layout: pres
                    .layout(s.layout)
                    .map(|(_, l)| l.name.clone())
                    .unwrap_or_default(),
                notes: s
                    .notes
                    .as_ref()
                    .is_some_and(|n| !n.text.text().trim().is_empty()),
                motion: Self::has_motion(s),
                version: self.slide_version(s.id),
            })
            .collect();
        let mut start = 0u32;
        let sections = pres
            .sections
            .iter()
            .map(|s| {
                let count = u32::try_from(s.len).unwrap_or(0);
                let info = DeckSectionInfo {
                    name: s.name.clone(),
                    start,
                    count,
                };
                start += count;
                info
            })
            .collect();
        let slide = self.current_slide().and_then(|id| pres.slide(id));
        // The shapes on the stage: the slide's, or the master's or layout's in Slide Master view.
        let shapes = self
            .stage_shapes()
            .map(|s| {
                s.iter()
                    .rev()
                    .map(|sh| DeckShapeInfo {
                        id: sh.id,
                        name: sh.name.clone(),
                        kind: kind_label(sh).into(),
                        hidden: sh.hidden,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut contexts = Vec::new();
        for id in &self.view.shapes {
            let Some(s) = pres.shape(*id) else { continue };
            let c = match &s.kind {
                ShapeKind::Picture(_) => "picture",
                ShapeKind::Table(_) => "table",
                ShapeKind::Chart(_) => "chart",
                _ => "shape",
            };
            if !contexts.iter().any(|x: &String| x == c) {
                contexts.push(c.to_owned());
            }
        }
        let selection_box = self.selection_xfrm().map(|x| {
            let r = rect_px(&x);
            DeckBox {
                x: r.x * 0.75,
                y: r.y * 0.75,
                width: r.w * 0.75,
                height: r.h * 0.75,
                #[allow(clippy::cast_precision_loss)]
                rotation: x.rot as f32 / 60_000.0,
            }
        });
        let theme = self
            .master()
            .map(|m| m.theme.name.clone())
            .unwrap_or_default();
        let layouts = self
            .master()
            .map(|m| {
                m.layouts
                    .iter()
                    .map(|l| DeckLayoutInfo {
                        id: l.id,
                        name: l.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has = |k: PlaceholderType| {
            slide.and_then(|s| {
                s.shapes
                    .iter()
                    .find(|x| x.placeholder.is_some_and(|p| p.kind == k))
            })
        };
        let header_footer = DeckHeaderFooter {
            date: has(PlaceholderType::Date).is_some(),
            slide_number: has(PlaceholderType::SlideNumber).is_some(),
            footer: has(PlaceholderType::Footer).and_then(|s| s.text().map(TextBody::text)),
        };
        let background = self.owner().and_then(|o| {
            let ctx = pres.color_context(o)?;
            match pres.resolved_background(o)? {
                Background::Fill {
                    fill: Fill::Solid(c),
                    ..
                } => Some(hex(kiln_deck_layout::paint::color(c, &ctx))),
                _ => None,
            }
        });
        DeckState {
            slides,
            sections,
            current: u32::try_from(self.view.current).unwrap_or(0),
            selected: self
                .view
                .slides
                .iter()
                .map(|i| u32::try_from(*i).unwrap_or(0))
                .collect(),
            slide_width: px(pres.slide_size.cx),
            slide_height: px(pres.slide_size.cy),
            size_kind: match pres.slide_size.kind {
                SlideSizeKind::Widescreen => "widescreen",
                SlideSizeKind::Standard => "standard",
                _ => "custom",
            }
            .into(),
            shapes,
            selection: self.view.shapes.clone(),
            editing: self.view.text.is_some(),
            contexts,
            format: self.deck_format(),
            selection_box,
            alt_text: self
                .view
                .shapes
                .first()
                .and_then(|id| pres.shape(*id))
                .and_then(|s| s.alt_text.clone()),
            theme,
            themes: bundled().into_iter().map(|t| t.name).collect(),
            layouts,
            layout: slide.map(|s| s.layout),
            notes: slide
                .and_then(|s| s.notes.as_ref())
                .map(|n| n.text.text())
                .unwrap_or_default(),
            guides: self.view.guides,
            gridlines: self.view.gridlines,
            notes_visible: self.view.notes,
            header_footer,
            background,
            hide_graphics: slide.is_some_and(|s| !s.show_master_shapes),
            armed: self.view.armed.clone(),
            find_done: self.find_done,
            caret: self.caret_rect().map(|b| DeckRect {
                x: b.x,
                y: b.y,
                width: b.w,
                height: b.h,
            }),
            motion: self.motion_state(),
            chart: self.chart_ui_state(),
            comments: self.comment_infos(),
            comment_total: self.comment_total(),
            comment: self.view.comment,
            spelling: self.spelling.hit.clone(),
            spell_done: self.spelling.done,
            master_view: self.master_view_state(),
            guide_list: self.guides.clone(),
            text_area: self.text_area(),
            cropping: self.cropping_picture().is_some(),
            cells: self.cell_range().map(|(_, r)| r),
        }
    }

    /// The id of the slide at an index (for tests and the chrome).
    #[allow(dead_code)]
    pub(crate) fn slide_at(&self, i: usize) -> Result<NodeId> {
        self.pres
            .slides
            .get(i)
            .map(|s| s.id)
            .ok_or_else(|| KilnError::InvalidInput(format!("there is no slide {}", i + 1)))
    }
}

/// A theme (in a variant) drawn on a title slide that reads "Aa".
fn title_sample(
    t: &kiln_deck::themes::ThemeFile,
    size: &kiln_deck::SlideSize,
    variant: Option<usize>,
) -> Option<Presentation> {
    let mut p = Presentation::with_theme(t, size.clone(), variant);
    let layout = p.masters.first().and_then(|m| {
        m.layout_of_kind(&kiln_deck::LayoutKind::Title)
            .or_else(|| m.layouts.first())
            .map(|l| l.id)
    })?;
    let mut slide = kiln_deck::edit::new_slide(&mut p, layout);
    for s in &mut slide.shapes {
        if s.placeholder.is_some_and(|ph| ph.kind.is_title())
            && let Some(t) = s.text_mut()
        {
            let body = std::mem::take(&mut t.body);
            *t = TextBody::from_text("Aa");
            t.body = body;
        }
    }
    p.slides.push(slide);
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_cursors_turn_with_the_shape() {
        assert_eq!(resize_cursor(Handle::E, 0.0), 2);
        assert_eq!(resize_cursor(Handle::N, 0.0), 0);
        assert_eq!(resize_cursor(Handle::N, 90.0), 2);
        assert_eq!(resize_cursor(Handle::SE, 0.0), 3);
    }
}
