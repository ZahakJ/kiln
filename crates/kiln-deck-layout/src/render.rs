// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slides to Scene ops: the background, the master's and layout's
//! decoration when the slide shows it, then the slide's shapes in z-order — preset and custom
//! geometry with fills, outlines, arrowheads and effects, pictures with cropping, groups,
//! tables, charts through `kiln-charts`, connectors and media posters — each shape's text
//! through [`crate::text`], and, in edit mode, the prompt text and dashed outline of empty
//! placeholders.
//!
//! Coordinates are slide px (EMU / 9525) with the slide's top-left at the origin. Each shape's
//! ops come with its bounds, so a tile takes only the shapes it meets.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kiln_charts::model::ChartSpace;
use kiln_charts::palette::ColorMode;
use kiln_charts::text::TextMeasurer;
use kiln_common::NodeId;
use kiln_deck::text::ResolvedBodyProps;
use kiln_deck::{
    Background, Geometry, Owner, PlaceholderType, Presentation, Shape, ShapeKind, ShapeProps,
    TextBody, Transform,
};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::presets::preset_geometry;
use kiln_ooxml::st::PathFillMode;
use kiln_scene::{Bounds, DrawOp, FillRule, ImageId, LineCap, LineJoin, Paint, PathData, Stroke};
use kiln_text::{FontStore, TextEngine};
use kurbo::Affine;
use rustc_hash::FxHashMap;

use crate::geom::{
    coeffs, is_transformed, px, rect_px, shape_matrix, text_matrix, transform_bounds,
    transform_path,
};
use crate::glyphs::{ChartGlyphs, Glyphs};
use crate::paint::{
    Look, Paintable, PictureSource, ShapeLook, background_style, effects_behind, fill_path,
    paintable, shade, shape_look, stroke_path,
};
use crate::picture::Pictures;
use crate::table::{TableBox, draw_table, layout_table};
use crate::text::{BodyInput, BodyLayout, TextEnv, draw_body, layout_body};

/// How a slide is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Options {
    /// The editing view: empty placeholders show their prompt and a dashed outline.
    pub edit: bool,
    /// The shape whose text is being edited (no prompt, bullets of empty paragraphs shown).
    pub editing: Option<NodeId>,
    /// Today's date for date fields.
    pub today: Option<(i32, u32, u32)>,
    /// The language of the empty placeholders' prompts (the interface's).
    pub prompts: PromptLanguage,
}

/// The interface language placeholder prompts are shown in, as Office shows them: prompts are
/// never part of the presentation, so they follow the person using it, not the file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PromptLanguage {
    /// English.
    #[default]
    English,
    /// German.
    German,
    /// Arabic.
    Arabic,
}

impl PromptLanguage {
    /// The prompt language of an interface language tag (`de`, `ar-SA`, …); English for the
    /// languages Kiln has no prompts in.
    #[must_use]
    pub fn from_tag(tag: &str) -> Self {
        let primary = tag.split(['-', '_']).next().unwrap_or_default();
        if primary.eq_ignore_ascii_case("de") {
            Self::German
        } else if primary.eq_ignore_ascii_case("ar") {
            Self::Arabic
        } else {
            Self::English
        }
    }

    /// The prompt an empty placeholder of a kind shows in the editing view (empty for the
    /// placeholders that show none).
    #[must_use]
    pub fn prompt(self, kind: PlaceholderType) -> &'static str {
        use PlaceholderType as P;
        #[derive(Clone, Copy)]
        enum K {
            Title,
            Subtitle,
            Picture,
            Chart,
            Table,
            Media,
            Text,
        }
        let k = match kind {
            P::Title | P::CenteredTitle => K::Title,
            P::Subtitle => K::Subtitle,
            P::Picture => K::Picture,
            P::Chart => K::Chart,
            P::Table => K::Table,
            P::Media => K::Media,
            P::Date | P::Footer | P::SlideNumber | P::Header | P::SlideImage => return "",
            _ => K::Text,
        };
        match (self, k) {
            (Self::English, K::Title) => "Click to add title",
            (Self::English, K::Subtitle) => "Click to add subtitle",
            (Self::English, K::Picture) => "Click icon to add picture",
            (Self::English, K::Chart) => "Click icon to add chart",
            (Self::English, K::Table) => "Click icon to add table",
            (Self::English, K::Media) => "Click icon to add media",
            (Self::English, K::Text) => "Click to add text",
            (Self::German, K::Title) => "Titel durch Klicken hinzufügen",
            (Self::German, K::Subtitle) => "Untertitel durch Klicken hinzufügen",
            (Self::German, K::Picture) => "Bild durch Klicken auf Symbol hinzufügen",
            (Self::German, K::Chart) => "Diagramm durch Klicken auf Symbol hinzufügen",
            (Self::German, K::Table) => "Tabelle durch Klicken auf Symbol hinzufügen",
            (Self::German, K::Media) => "Medien durch Klicken auf Symbol hinzufügen",
            (Self::German, K::Text) => "Text durch Klicken hinzufügen",
            (Self::Arabic, K::Title) => "انقر لإضافة عنوان",
            (Self::Arabic, K::Subtitle) => "انقر لإضافة عنوان فرعي",
            (Self::Arabic, K::Picture) => "انقر فوق الأيقونة لإضافة صورة",
            (Self::Arabic, K::Chart) => "انقر فوق الأيقونة لإضافة مخطط",
            (Self::Arabic, K::Table) => "انقر فوق الأيقونة لإضافة جدول",
            (Self::Arabic, K::Media) => "انقر فوق الأيقونة لإضافة وسائط",
            (Self::Arabic, K::Text) => "انقر لإضافة نص",
        }
    }
}

/// One shape's ops.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnShape {
    /// The top-level shape (master and layout decoration included).
    pub id: NodeId,
    /// What it can touch, in slide px.
    pub bounds: Bounds,
    /// The ops, in slide px (glyph bitmaps positioned for the scene's device scale).
    pub ops: Vec<DrawOp>,
}

/// A drawn slide.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnSlide {
    /// Slide size in px.
    pub width: f32,
    /// Slide height in px.
    pub height: f32,
    /// The background ops (cover the slide rectangle).
    pub background: Vec<DrawOp>,
    /// Shapes in z-order: master decoration, layout decoration, the slide's own.
    pub shapes: Vec<DrawnShape>,
    /// Pictures drawn.
    pub images: Vec<ImageId>,
}

impl DrawnSlide {
    /// Every op in order (background first).
    #[must_use]
    pub fn all_ops(&self) -> Vec<DrawOp> {
        let mut out = self.background.clone();
        for s in &self.shapes {
            out.extend(s.ops.iter().cloned());
        }
        out
    }
}

/// The geometry of one shape for hit testing and selection.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGeom {
    /// The shape.
    pub id: NodeId,
    /// Its transform (absolute, inheritance resolved).
    pub xfrm: Transform,
    /// Local px → slide px.
    pub matrix: Affine,
    /// Outline paths in local px (empty for pictures, tables, charts: the box is the outline).
    pub paths: Vec<PathData>,
    /// Lines and connectors (hit along the stroke, not inside).
    pub is_line: bool,
    /// The text rectangle in local px (before insets), when the shape holds text.
    pub text_rect: Option<Bounds>,
}

impl ShapeGeom {
    /// Whether a slide point hits the shape (`slop` px of tolerance).
    #[must_use]
    pub fn hit(&self, x: f32, y: f32, slop: f32) -> bool {
        let Some(inv) = invert(self.matrix) else {
            return false;
        };
        let q = inv * kurbo::Point::new(f64::from(x), f64::from(y));
        #[allow(clippy::cast_possible_truncation)]
        let (lx, ly) = (q.x as f32, q.y as f32);
        let r = rect_px(&self.xfrm);
        let local = Bounds::new(0.0, 0.0, r.w, r.h);
        if self.is_line {
            return self
                .paths
                .iter()
                .any(|p| crate::geom::distance_to_path(p, lx, ly) <= slop.max(3.0));
        }
        if !local.inflate(slop, slop).contains(lx, ly) {
            return false;
        }
        // Thin boxes (a text box with one line, a rule) hit within their box.
        if self.paths.is_empty() || r.w < 6.0 || r.h < 6.0 {
            return true;
        }
        self.paths.iter().any(|p| {
            crate::geom::path_contains(p, lx, ly)
                || crate::geom::distance_to_path(p, lx, ly) <= slop
        }) || self.text_rect.is_some_and(|t| t.contains(lx, ly))
    }

    /// The axis-aligned bounds on the slide.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        let r = rect_px(&self.xfrm);
        transform_bounds(Bounds::new(0.0, 0.0, r.w, r.h), self.matrix)
    }
}

/// The inverse of a matrix, when it has one.
#[must_use]
pub fn invert(m: Affine) -> Option<Affine> {
    let d = m.determinant();
    if d.abs() < 1e-12 || !d.is_finite() {
        None
    } else {
        Some(m.inverse())
    }
}

/// Slide rendering with its caches (text layouts, decoded pictures and parsed charts).
pub struct Renderer {
    /// The text engine (fonts and shaping cache).
    pub engine: TextEngine,
    chart_text: TextMeasurer,
    /// Decoded pictures.
    pub pictures: Pictures,
    texts: FxHashMap<u64, Arc<BodyLayout>>,
    tables: FxHashMap<u64, Arc<TableBox>>,
    charts: FxHashMap<String, Option<Arc<ChartSpace>>>,
    /// Bumped when masters, layouts or themes change (cached text depends on them).
    epoch: u64,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("texts", &self.texts.len())
            .field("tables", &self.tables.len())
            .field("charts", &self.charts.len())
            .finish_non_exhaustive()
    }
}

/// The prompt an empty placeholder shows in the editing view, in English.
#[must_use]
pub fn prompt_for(kind: PlaceholderType) -> &'static str {
    PromptLanguage::English.prompt(kind)
}

/// Whether a text body holds no text.
#[must_use]
pub fn body_is_empty(body: Option<&TextBody>) -> bool {
    body.is_none_or(|b| b.paragraphs().all(|p| p.content.is_empty()))
}

/// The local geometry of a shape: `(paths with fill modes, stroke flags and closedness, text
/// rectangle)`.
#[must_use]
pub fn geometry(
    props: &ShapeProps,
    w: f32,
    h: f32,
    connector: bool,
) -> (Vec<(PathData, PathFillMode, bool, bool)>, Bounds) {
    let full = Bounds::new(0.0, 0.0, w, h);
    let resolved = match &props.geometry {
        Some(Geometry::Preset { name, adjust }) => {
            #[allow(clippy::cast_precision_loss)]
            let adj: Vec<(&str, f64)> = adjust
                .iter()
                .map(|(n, v)| (n.as_str(), *v as f64))
                .collect();
            preset_geometry(name, f64::from(w), f64::from(h), &adj).ok()
        }
        Some(Geometry::Custom(def)) => def
            .compile()
            .ok()
            .map(|c| c.resolve(f64::from(w), f64::from(h), &[])),
        None if connector => preset_geometry("line", f64::from(w), f64::from(h), &[]).ok(),
        None => None,
    };
    let Some(g) = resolved else {
        return (
            vec![(PathData::rect(full), PathFillMode::Norm, true, true)],
            full,
        );
    };
    let paths = g
        .paths
        .iter()
        .map(|p| {
            let path = PathData::from(&p.to_bez_path(0.1));
            // Closed when the path ends with a close (arrowheads go on open paths only).
            let closed = path.verbs.last() == Some(&kiln_scene::verb::CLOSE);
            (path, p.fill, p.stroke, closed)
        })
        .collect();
    #[allow(clippy::cast_possible_truncation)]
    let tr = Bounds::from_corners(
        g.text_rect.x0 as f32,
        g.text_rect.y0 as f32,
        g.text_rect.x1 as f32,
        g.text_rect.y1 as f32,
    );
    (paths, if tr.w > 0.0 && tr.h > 0.0 { tr } else { full })
}

/// The corner radius of a rounded rectangle preset (for the Scene's box shadow), `None` for
/// other shapes.
fn box_radius(props: &ShapeProps, w: f32, h: f32) -> Option<f32> {
    match &props.geometry {
        None => Some(0.0),
        Some(Geometry::Preset { name, adjust }) => match name.as_str() {
            "rect" => Some(0.0),
            "roundRect" => {
                #[allow(clippy::cast_precision_loss)]
                let adj = adjust
                    .iter()
                    .find(|(n, _)| n == "adj")
                    .map_or(16_667.0, |(_, v)| *v as f32);
                Some(w.min(h) * adj / 100_000.0)
            }
            _ => None,
        },
        Some(Geometry::Custom(_)) => None,
    }
}

struct PictureCx<'a> {
    pres: &'a Presentation,
    pictures: &'a mut Pictures,
    used: &'a mut Vec<ImageId>,
}

impl PictureSource for PictureCx<'_> {
    fn picture(&mut self, part: &str) -> Option<(ImageId, u32, u32)> {
        let d = self.pictures.get(self.pres, part)?;
        if !self.used.contains(&d.id) {
            self.used.push(d.id);
        }
        Some((d.id, d.width, d.height))
    }

    fn natural_size(&mut self, part: &str) -> Option<(f32, f32)> {
        self.pictures.get(self.pres, part).map(|d| d.natural)
    }

    fn pattern(
        &mut self,
        preset: kiln_ooxml::st::PresetPatternVal,
        fg: u32,
        bg: u32,
    ) -> Option<(ImageId, u32, u32)> {
        let d = self.pictures.pattern(preset, fg, bg);
        if !self.used.contains(&d.id) {
            self.used.push(d.id);
        }
        Some((d.id, d.width, d.height))
    }
}

/// What a shape draws with.
struct Cx<'a> {
    pres: &'a Presentation,
    look: Look<'a>,
    env: TextEnv<'a>,
    opts: Options,
}

impl Renderer {
    /// A renderer over two font stores (text, chart labels).
    #[must_use]
    pub fn new(text_fonts: FontStore, chart_fonts: FontStore) -> Self {
        Self {
            engine: TextEngine::new(text_fonts),
            chart_text: TextMeasurer::new(chart_fonts),
            pictures: Pictures::new(),
            texts: FxHashMap::default(),
            tables: FxHashMap::default(),
            charts: FxHashMap::default(),
            epoch: 0,
        }
    }

    /// Forgets cached text and tables (masters, layouts, the theme or the parts changed).
    pub fn invalidate(&mut self) {
        self.epoch += 1;
        self.texts.clear();
        self.tables.clear();
        self.charts.clear();
    }

    /// Takes fonts the host registered after the renderer was made (the web core fetches its
    /// Arabic, symbol and mathematics fonts when text needs them) and lays text out again.
    pub fn add_fonts(&mut self, fonts: &[std::sync::Arc<[u8]>]) {
        if fonts.is_empty() {
            return;
        }
        for store in [
            self.engine.fonts_mut(),
            self.chart_text.engine_mut().fonts_mut(),
        ] {
            for f in fonts {
                store.add_font_data(f.to_vec());
            }
            store.refresh_fallbacks();
        }
        self.invalidate();
    }

    /// Forgets a changed part (a picture or chart).
    pub fn forget_part(&mut self, part: &str) {
        self.pictures.forget(part);
        self.charts.remove(part);
    }

    /// The fonts.
    #[must_use]
    pub fn fonts(&self) -> &FontStore {
        self.engine.fonts()
    }

    /// Cached text entries (diagnostics, tests).
    #[must_use]
    pub fn cached_texts(&self) -> usize {
        self.texts.len()
    }

    fn look_for(pres: &Presentation, owner: Owner) -> Option<Look<'_>> {
        Some(Look {
            theme: pres.theme_of(owner)?,
            ctx: pres.color_context(owner)?,
        })
    }

    /// What the chart chrome shows about `chart` drawn `w` × `h` px on `owner` (the slide's
    /// theme gives its colours and fonts).
    pub fn chart_state(
        &mut self,
        pres: &Presentation,
        owner: Owner,
        chart: &ChartSpace,
        w: f32,
        h: f32,
    ) -> Option<kiln_protocol::commands::ChartState> {
        let look = Self::look_for(pres, owner)?;
        let colors = kiln_charts::palette::ChartColors::new(Some(look.theme), ColorMode::Document)
            .with_color_map(look.ctx.color_map);
        let layout = kiln_charts::layout::layout_chart(chart, w, h, &colors, &mut self.chart_text);
        Some(kiln_charts::state::chart_state(chart, &layout, &colors))
    }

    fn slide_number(pres: &Presentation, owner: Owner) -> usize {
        match owner {
            Owner::Slide(id) => {
                pres.slide_index(id).unwrap_or(0)
                    + usize::try_from(pres.first_slide_number.max(1)).unwrap_or(1)
            }
            _ => 1,
        }
    }

    /// Draws a slide (or a layout or master, for galleries).
    pub fn draw(
        &mut self,
        pres: &Presentation,
        owner: Owner,
        opts: &Options,
        glyphs: &mut Glyphs,
    ) -> DrawnSlide {
        let width = px(pres.slide_size.cx);
        let height = px(pres.slide_size.cy);
        let mut out = DrawnSlide {
            width,
            height,
            ..DrawnSlide::default()
        };
        let Some(look) = Self::look_for(pres, owner) else {
            out.background.push(DrawOp::FillRect {
                rect: Bounds::new(0.0, 0.0, width, height),
                paint: Paint::Rgba(0xffff_ffff),
                radius: 0.0,
            });
            return out;
        };
        let cx = Cx {
            pres,
            look,
            env: TextEnv {
                theme: look.theme,
                ctx: look.ctx,
                slide_number: Self::slide_number(pres, owner),
                today: opts.today,
                editing: false,
                fallback_color: None,
                fallback_bold: false,
            },
            opts: *opts,
        };
        let mut used = Vec::new();
        // Background.
        let slide_rect = Bounds::new(0.0, 0.0, width, height);
        let bg =
            pres.resolved_background(owner)
                .map_or(Paintable::Solid(0xffff_ffff), |b| match b {
                    Background::Fill { fill, .. } => paintable(fill, &look.ctx),
                    Background::Theme { idx, color } => {
                        background_style(*idx, color.as_ref(), &look)
                    }
                });
        out.background.push(DrawOp::FillRect {
            rect: slide_rect,
            paint: Paint::Rgba(0xffff_ffff),
            radius: 0.0,
        });
        {
            let mut pics = PictureCx {
                pres,
                pictures: &mut self.pictures,
                used: &mut used,
            };
            fill_path(
                &mut out.background,
                &PathData::rect(slide_rect),
                slide_rect,
                &bg,
                &mut pics,
            );
        }
        // Decoration of the master and the layout.
        let mut decor: Vec<(Owner, &Shape)> = Vec::new();
        match owner {
            Owner::Slide(id) => {
                if pres.shows_master_shapes(id)
                    && let Some(m) = pres.master_of(owner)
                {
                    decor.extend(
                        m.shapes
                            .iter()
                            .filter(|s| s.placeholder.is_none())
                            .map(|s| (Owner::Master(m.id), s)),
                    );
                }
                let show_layout = pres.slide(id).is_some_and(|s| s.show_master_shapes);
                if show_layout && let Some(l) = pres.layout_of(owner) {
                    decor.extend(
                        l.shapes
                            .iter()
                            .filter(|s| s.placeholder.is_none())
                            .map(|s| (Owner::Layout(l.id), s)),
                    );
                }
            }
            Owner::Layout(id) => {
                if let Some((m, l)) = pres.layout(id)
                    && l.show_master_shapes
                {
                    decor.extend(
                        m.shapes
                            .iter()
                            .filter(|s| s.placeholder.is_none())
                            .map(|s| (Owner::Master(m.id), s)),
                    );
                }
            }
            Owner::Master(_) | Owner::NotesMaster(_) => {}
        }
        for (_, s) in decor {
            if s.hidden {
                continue;
            }
            if let Some(d) = self.draw_top(&cx, s, false, glyphs, &mut used) {
                out.shapes.push(d);
            }
        }
        // The owner's own shapes: on layouts and masters, placeholders show as such.
        let own = pres.owner_shapes(owner).cloned().unwrap_or_default();
        let gallery = !matches!(owner, Owner::Slide(_));
        for s in &own {
            if s.hidden {
                continue;
            }
            if gallery && s.placeholder.is_some_and(|p| p.kind.is_footer_area()) {
                continue;
            }
            if let Some(d) = self.draw_top(&cx, s, true, glyphs, &mut used) {
                out.shapes.push(d);
            }
        }
        out.images = used;
        out
    }

    /// The geometry of every top-level shape of a slide, layout or master, in z-order (hit
    /// testing).
    #[must_use]
    pub fn hit_shapes(&self, pres: &Presentation, owner: Owner) -> Vec<ShapeGeom> {
        let Some(shapes) = pres.owner_shapes(owner) else {
            return Vec::new();
        };
        shapes
            .iter()
            .filter(|sh| !sh.hidden)
            .filter_map(|sh| self.shape_geom(pres, sh.id))
            .collect()
    }

    /// The geometry of one shape (any depth).
    #[must_use]
    pub fn shape_geom(&self, pres: &Presentation, id: NodeId) -> Option<ShapeGeom> {
        let shape = pres.shape(id)?;
        let xfrm = absolute_xfrm(pres, id)?;
        let r = rect_px(&xfrm);
        let props = pres
            .resolved_props(id)
            .or_else(|| shape.props().cloned())
            .unwrap_or_default();
        let is_connector = matches!(shape.kind, ShapeKind::Connector(_));
        let (paths, text_rect) = match &shape.kind {
            ShapeKind::Geometry(_) | ShapeKind::Connector(_) => {
                let (p, tr) = geometry(&props, r.w, r.h, is_connector);
                (p.into_iter().map(|(p, ..)| p).collect(), Some(tr))
            }
            _ => (Vec::new(), None),
        };
        let is_line = is_connector
            || matches!(&props.geometry, Some(Geometry::Preset { name, .. }) if is_line_preset(name))
            || (r.w < 0.5 || r.h < 0.5) && !matches!(shape.kind, ShapeKind::Group(_));
        Some(ShapeGeom {
            id,
            xfrm,
            matrix: shape_matrix(&xfrm),
            paths,
            is_line,
            text_rect: shape
                .text()
                .map(|_| text_rect.unwrap_or(Bounds::new(0.0, 0.0, r.w, r.h))),
        })
    }

    /// The text layout of a shape as drawn (for carets and hit tests while editing).
    pub fn text_layout(
        &mut self,
        pres: &Presentation,
        id: NodeId,
        opts: &Options,
    ) -> Option<Arc<BodyLayout>> {
        let shape = pres.shape(id)?;
        let owner = pres.locate(id)?.owner;
        let look = Self::look_for(pres, owner)?;
        let cx = Cx {
            pres,
            look,
            env: TextEnv {
                theme: look.theme,
                ctx: look.ctx,
                slide_number: Self::slide_number(pres, owner),
                today: opts.today,
                editing: opts.editing == Some(id),
                fallback_color: None,
                fallback_bold: false,
            },
            opts: *opts,
        };
        let xfrm = absolute_xfrm(pres, id)?;
        let r = rect_px(&xfrm);
        let props = pres
            .resolved_props(id)
            .or_else(|| shape.props().cloned())
            .unwrap_or_default();
        let (_, tr) = geometry(&props, r.w, r.h, false);
        let body = shape.text()?;
        Some(self.body(&cx, id, body, tr))
    }

    /// The autofit a shape's text needs after an edit ("Shrink text on overflow"), `None`
    /// when its text does not shrink.
    pub fn fit_autofit(&mut self, pres: &Presentation, id: NodeId) -> Option<kiln_deck::Autofit> {
        let shape = pres.shape(id)?;
        let owner = pres.locate(id)?.owner;
        let look = Self::look_for(pres, owner)?;
        let env = TextEnv {
            theme: look.theme,
            ctx: look.ctx,
            slide_number: Self::slide_number(pres, owner),
            today: None,
            editing: true,
            fallback_color: None,
            fallback_bold: false,
        };
        let xfrm = absolute_xfrm(pres, id)?;
        let r = rect_px(&xfrm);
        let props = pres
            .resolved_props(id)
            .or_else(|| shape.props().cloned())
            .unwrap_or_default();
        let (_, tr) = geometry(&props, r.w, r.h, false);
        let body = shape.text()?;
        let bp = pres
            .resolved_body(id)
            .unwrap_or_else(|| body.body.resolve());
        crate::text::fit_autofit(
            &mut self.engine,
            pres,
            &BodyInput {
                shape: id,
                body,
                props: &bp,
                rect: tr,
            },
            &env,
        )
    }

    /// The table layout of a table shape.
    pub fn table_layout(&mut self, pres: &Presentation, id: NodeId) -> Option<Arc<TableBox>> {
        let shape = pres.shape(id)?;
        let ShapeKind::Table(t) = &shape.kind else {
            return None;
        };
        let owner = pres.locate(id)?.owner;
        let look = Self::look_for(pres, owner)?;
        let env = TextEnv {
            theme: look.theme,
            ctx: look.ctx,
            slide_number: Self::slide_number(pres, owner),
            today: None,
            editing: false,
            fallback_color: None,
            fallback_bold: false,
        };
        Some(self.table(pres, id, t, &look, &env))
    }

    fn table(
        &mut self,
        pres: &Presentation,
        id: NodeId,
        t: &kiln_deck::Table,
        look: &Look<'_>,
        env: &TextEnv<'_>,
    ) -> Arc<TableBox> {
        let mut h = rustc_hash::FxHasher::default();
        id.hash(&mut h);
        self.epoch.hash(&mut h);
        if let Ok(bytes) = postcard::to_allocvec(t) {
            bytes.hash(&mut h);
        }
        env.slide_number.hash(&mut h);
        let key = h.finish();
        if let Some(c) = self.tables.get(&key) {
            return c.clone();
        }
        let b = Arc::new(layout_table(&mut self.engine, pres, id, t, look, env));
        if self.tables.len() > 512 {
            self.tables.clear();
        }
        self.tables.insert(key, b.clone());
        b
    }

    fn body(&mut self, cx: &Cx<'_>, id: NodeId, body: &TextBody, rect: Bounds) -> Arc<BodyLayout> {
        let props: ResolvedBodyProps = cx
            .pres
            .resolved_body(id)
            .unwrap_or_else(|| body.body.resolve());
        let mut h = rustc_hash::FxHasher::default();
        id.hash(&mut h);
        self.epoch.hash(&mut h);
        if let Ok(bytes) = postcard::to_allocvec(body) {
            bytes.hash(&mut h);
        }
        rect.x.to_bits().hash(&mut h);
        rect.y.to_bits().hash(&mut h);
        rect.w.to_bits().hash(&mut h);
        rect.h.to_bits().hash(&mut h);
        props.hash(&mut h);
        cx.env.slide_number.hash(&mut h);
        cx.env.editing.hash(&mut h);
        cx.env.today.hash(&mut h);
        let key = h.finish();
        if let Some(c) = self.texts.get(&key) {
            return c.clone();
        }
        let laid = Arc::new(layout_body(
            &mut self.engine,
            cx.pres,
            &BodyInput {
                shape: id,
                body,
                props: &props,
                rect,
            },
            &cx.env,
        ));
        if self.texts.len() > 4096 {
            self.texts.clear();
        }
        self.texts.insert(key, laid.clone());
        laid
    }

    fn draw_top(
        &mut self,
        cx: &Cx<'_>,
        shape: &Shape,
        own: bool,
        glyphs: &mut Glyphs,
        used: &mut Vec<ImageId>,
    ) -> Option<DrawnShape> {
        let xfrm = if own {
            cx.pres.resolved_xfrm(shape.id).or(shape.xfrm)?
        } else {
            shape.xfrm?
        };
        let mut ops = Vec::new();
        let mut bounds = Bounds::ZERO;
        self.draw_shape(
            cx,
            shape,
            &xfrm,
            None,
            own,
            glyphs,
            used,
            &mut ops,
            &mut bounds,
        );
        if ops.is_empty() {
            return None;
        }
        Some(DrawnShape {
            id: shape.id,
            bounds,
            ops,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn draw_shape(
        &mut self,
        cx: &Cx<'_>,
        shape: &Shape,
        xfrm: &Transform,
        group_fill: Option<&Fill>,
        top: bool,
        glyphs: &mut Glyphs,
        used: &mut Vec<ImageId>,
        out: &mut Vec<DrawOp>,
        bounds: &mut Bounds,
    ) {
        let r = rect_px(xfrm);
        let m = shape_matrix(xfrm);
        let turned = is_transformed(xfrm);
        let mut props = if top {
            cx.pres
                .resolved_props(shape.id)
                .or_else(|| shape.props().cloned())
                .unwrap_or_default()
        } else {
            shape.props().cloned().unwrap_or_default()
        };
        if matches!(props.fill, Some(Fill::Group))
            && let Some(g) = group_fill
        {
            props.fill = Some(g.clone());
        }
        let local = Bounds::new(0.0, 0.0, r.w, r.h);
        let scene_box = transform_bounds(local, m);
        let grow = |b: &mut Bounds, extra: f32| {
            let e = scene_box.inflate(extra, extra);
            *b = if b.w <= 0.0 && b.h <= 0.0 {
                e
            } else {
                b.union(&e)
            };
        };
        match &shape.kind {
            ShapeKind::Group(g) => {
                for child in &g.children {
                    if child.hidden {
                        continue;
                    }
                    let Some(cx_xfrm) = child.xfrm else { continue };
                    let abs = g.child_space.child_to_parent(xfrm, &cx_xfrm);
                    self.draw_shape(
                        cx,
                        child,
                        &abs,
                        props.fill.as_ref().or(group_fill),
                        false,
                        glyphs,
                        used,
                        out,
                        bounds,
                    );
                }
            }
            ShapeKind::Geometry(auto) => {
                let look = shape_look(&props, &cx.look);
                let (paths, text_rect) = geometry(&props, r.w, r.h, false);
                let reach = self.draw_geometry(cx, &paths, &look, &props, r, m, used, out);
                grow(bounds, reach);
                // Text.
                let placeholder = shape.placeholder;
                let empty = body_is_empty(auto.text.as_ref());
                let editing = cx.opts.editing == Some(shape.id);
                if let Some(body) = &auto.text
                    && (!empty || editing)
                {
                    let mut env = cx.env;
                    env.editing = editing;
                    let cxe = Cx { env, ..*cx };
                    let laid = self.body(&cxe, shape.id, body, text_rect);
                    let tm = text_matrix(xfrm);
                    draw_body(
                        out,
                        &laid,
                        self.engine.fonts(),
                        glyphs,
                        tm,
                        !turned || !xfrm_text_turns(xfrm),
                    );
                    grow(bounds, laid.content_height.max(r.h));
                } else if let Some(ph) = placeholder
                    && cx.opts.edit
                    && top
                    && empty
                    && !editing
                    && matches!(
                        cx.pres.locate(shape.id).map(|p| p.owner),
                        Some(Owner::Slide(_))
                    )
                {
                    self.prompt(cx, shape, ph.kind, text_rect, xfrm, glyphs, out);
                }
            }
            ShapeKind::Connector(_) => {
                let look = ShapeLook {
                    fill: Paintable::None,
                    ..shape_look(&props, &cx.look)
                };
                let (paths, _) = geometry(&props, r.w, r.h, true);
                let reach = self.draw_geometry(cx, &paths, &look, &props, r, m, used, out);
                grow(bounds, reach + 12.0);
            }
            ShapeKind::Picture(pic) => {
                let look = shape_look(&props, &cx.look);
                let (paths, _) = if props.geometry.is_some() {
                    geometry(&props, r.w, r.h, false)
                } else {
                    (
                        vec![(PathData::rect(local), PathFillMode::Norm, true, true)],
                        local,
                    )
                };
                let clip_local = paths
                    .first()
                    .map_or_else(|| PathData::rect(local), |p| p.0.clone());
                effects_behind(
                    out,
                    &transform_path(&clip_local, m),
                    (!turned).then_some((scene_box, 0.0)),
                    &look.effects,
                    true,
                );
                let crop = pic.crop;
                #[allow(clippy::cast_precision_loss)]
                let f = |v: i32| v as f32 / 100_000.0;
                let fill = Paintable::Picture {
                    part: pic.media.clone(),
                    crop: (f(crop.l), f(crop.t), f(crop.r), f(crop.b)),
                    tile: None,
                    #[allow(clippy::cast_precision_loss)]
                    alpha: pic
                        .alpha
                        .map_or(1.0, |a| (a as f32 / 100_000.0).clamp(0.0, 1.0)),
                };
                let empty = pic.media.is_empty();
                if empty {
                    if cx.opts.edit && shape.placeholder.is_some() {
                        self.prompt(
                            cx,
                            shape,
                            PlaceholderType::Picture,
                            local,
                            xfrm,
                            glyphs,
                            out,
                        );
                    }
                } else {
                    out.push(DrawOp::PushTransform { matrix: coeffs(m) });
                    let mut pics = PictureCx {
                        pres: cx.pres,
                        pictures: &mut self.pictures,
                        used,
                    };
                    fill_path(out, &clip_local, local, &fill, &mut pics);
                    out.push(DrawOp::PopTransform);
                }
                if let Some(line) = &look.line {
                    stroke_path(out, &transform_path(&clip_local, m), line, true);
                }
                grow(
                    bounds,
                    look.line.as_ref().map_or(0.0, |l| l.width) + shadow_reach(&look),
                );
            }
            ShapeKind::Table(t) => {
                let env = cx.env;
                let tb = self.table(cx.pres, shape.id, t, &cx.look, &env);
                let mut pics = PictureCx {
                    pres: cx.pres,
                    pictures: &mut self.pictures,
                    used,
                };
                let matrix = turned.then_some(m);
                draw_table(
                    out,
                    &tb,
                    (r.x, r.y),
                    self.engine.fonts(),
                    glyphs,
                    &mut pics,
                    matrix,
                );
                let tb_box = Bounds::new(
                    r.x,
                    r.y,
                    tb.xs.last().copied().unwrap_or(r.w),
                    tb.height().max(r.h),
                );
                *bounds = if bounds.w <= 0.0 {
                    tb_box.inflate(4.0, 4.0)
                } else {
                    bounds.union(&tb_box.inflate(4.0, 4.0))
                };
            }
            ShapeKind::Chart(c) => {
                let chart = self.chart(cx.pres, &c.part);
                match chart {
                    Some(chart) => {
                        out.push(DrawOp::PushTransform { matrix: coeffs(m) });
                        let mut sink = ChartGlyphs {
                            glyphs,
                            offset: (r.x, r.y),
                            matrix: turned.then_some(m),
                        };
                        // The slide's colour map: text and lines on a dark slide stay light.
                        let colors = kiln_charts::palette::ChartColors::new(
                            Some(cx.look.theme),
                            ColorMode::Document,
                        )
                        .with_color_map(cx.look.ctx.color_map);
                        let layout = kiln_charts::layout::layout_chart(
                            &chart,
                            r.w,
                            r.h,
                            &colors,
                            &mut self.chart_text,
                        );
                        out.extend(kiln_charts::render::draw(
                            &layout,
                            &self.chart_text,
                            &mut sink,
                        ));
                        out.push(DrawOp::PopTransform);
                    }
                    None => {
                        out.push(DrawOp::Path {
                            path: transform_path(&PathData::rect(local), m),
                            fill: Some(Paint::Rgba(0xf2f2_f2ff)),
                            stroke: Some(Stroke::solid(Paint::Rgba(0xbfbf_bfff), 1.0)),
                            rule: FillRule::NonZero,
                        });
                    }
                }
                grow(bounds, 4.0);
            }
            ShapeKind::Media(media) => {
                let poster = media.poster.clone();
                let mut drew = false;
                if let Some(p) = poster {
                    out.push(DrawOp::PushTransform { matrix: coeffs(m) });
                    let mut pics = PictureCx {
                        pres: cx.pres,
                        pictures: &mut self.pictures,
                        used,
                    };
                    let fill = Paintable::Picture {
                        part: p,
                        crop: (0.0, 0.0, 0.0, 0.0),
                        tile: None,
                        alpha: 1.0,
                    };
                    fill_path(out, &PathData::rect(local), local, &fill, &mut pics);
                    out.push(DrawOp::PopTransform);
                    drew = true;
                }
                if !drew {
                    out.push(DrawOp::Path {
                        path: transform_path(&PathData::rect(local), m),
                        fill: Some(Paint::Rgba(0x2a21_1cff)),
                        stroke: None,
                        rule: FillRule::NonZero,
                    });
                }
                // A play badge in the middle.
                let s = (r.w.min(r.h) * 0.18).clamp(10.0, 48.0);
                let (ccx, ccy) = (r.w / 2.0, r.h / 2.0);
                let mut tri = PathData::new();
                tri.move_to(ccx - s * 0.35, ccy - s * 0.5)
                    .line_to(ccx + s * 0.55, ccy)
                    .line_to(ccx - s * 0.35, ccy + s * 0.5)
                    .close();
                let circle = kiln_scene::shapes::rounded_rect(
                    Bounds::new(ccx - s, ccy - s, s * 2.0, s * 2.0),
                    s,
                );
                out.push(DrawOp::Path {
                    path: transform_path(&circle, m),
                    fill: Some(Paint::Rgba(0x0000_0099)),
                    stroke: None,
                    rule: FillRule::NonZero,
                });
                out.push(DrawOp::Path {
                    path: transform_path(&tri, m),
                    fill: Some(Paint::Rgba(0xffff_ffff)),
                    stroke: None,
                    rule: FillRule::NonZero,
                });
                grow(bounds, 2.0);
            }
        }
    }

    /// Fills, outlines and effects of geometry paths. Returns how far the drawing reaches
    /// beyond the shape's box.
    #[allow(clippy::too_many_arguments)]
    fn draw_geometry(
        &mut self,
        cx: &Cx<'_>,
        paths: &[(PathData, PathFillMode, bool, bool)],
        look: &ShapeLook,
        props: &ShapeProps,
        r: Bounds,
        m: Affine,
        used: &mut Vec<ImageId>,
        out: &mut Vec<DrawOp>,
    ) -> f32 {
        let turned =
            m.as_coeffs()[1].abs() > 1e-9 || m.as_coeffs()[0] < 0.0 || m.as_coeffs()[3] < 0.0;
        let filled_any = !matches!(look.fill, Paintable::None);
        // Effects behind: the union of the filled outlines.
        if look.effects.shadow.is_some() || look.effects.glow.is_some() {
            let mut outline = PathData::new();
            for (p, mode, _, _) in paths {
                if *mode != PathFillMode::None || !filled_any {
                    outline.extend(p);
                }
            }
            let outline = transform_path(&outline, m);
            let rect = box_radius(props, r.w, r.h)
                .filter(|_| !turned)
                .map(|rad| (Bounds::new(r.x, r.y, r.w, r.h), rad));
            effects_behind(out, &outline, rect, &look.effects, filled_any);
        }
        let mut pics = PictureCx {
            pres: cx.pres,
            pictures: &mut self.pictures,
            used,
        };
        for (p, mode, _, _) in paths {
            if *mode == PathFillMode::None {
                continue;
            }
            let fill = shade(&look.fill, *mode);
            let scene_path = transform_path(p, m);
            let local_box = p.bounds();
            let fill_box = if matches!(fill, Paintable::Gradient { angle: Some(_), .. }) && turned {
                // Gradients turn with the shape: draw them in local space.
                out.push(DrawOp::PushTransform { matrix: coeffs(m) });
                fill_path(out, p, Bounds::new(0.0, 0.0, r.w, r.h), &fill, &mut pics);
                out.push(DrawOp::PopTransform);
                continue;
            } else {
                transform_bounds(Bounds::new(0.0, 0.0, r.w, r.h), m)
            };
            let _ = local_box;
            if let Some(soft) = look.effects.soft_edge
                && soft_fill(out, p, r, m, soft, &fill, &mut pics)
            {
                continue;
            }
            fill_path(out, &scene_path, fill_box, &fill, &mut pics);
        }
        let mut reach = shadow_reach(look);
        if let (Some((st, end, len, gap)), false) = (look.effects.reflection, turned) {
            reach = reach.max(reflect(
                out,
                paths,
                look,
                r,
                m,
                (st, end, len, gap),
                &mut pics,
            ));
        }
        // Soft edges fade the outline away with the fill.
        if let Some(line) = look
            .line
            .as_ref()
            .filter(|_| look.effects.soft_edge.is_none())
        {
            reach = reach.max(
                line.width * 2.0
                    + if line.head.is_some() || line.tail.is_some() {
                        line.width * 6.0
                    } else {
                        0.0
                    },
            );
            for (p, _, stroke, closed) in paths {
                if !*stroke {
                    continue;
                }
                stroke_path(out, &transform_path(p, m), line, *closed);
            }
        }
        reach
    }

    #[allow(clippy::too_many_arguments)]
    fn prompt(
        &mut self,
        cx: &Cx<'_>,
        shape: &Shape,
        kind: PlaceholderType,
        text_rect: Bounds,
        xfrm: &Transform,
        glyphs: &mut Glyphs,
        out: &mut Vec<DrawOp>,
    ) {
        let r = rect_px(xfrm);
        let m = shape_matrix(xfrm);
        // The dashed outline of an empty placeholder.
        out.push(DrawOp::Path {
            path: transform_path(&PathData::rect(Bounds::new(0.0, 0.0, r.w, r.h)), m),
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Rgba(0x8c84_7dff),
                width: 1.0,
                join: LineJoin::Miter,
                cap: LineCap::Butt,
                dash: vec![4.0, 3.0],
            }),
            rule: FillRule::NonZero,
        });
        // A custom prompt is the layout placeholder's own text.
        let chain = cx.pres.chain_of(shape.id);
        let custom = chain
            .and_then(|c| c.layout)
            .filter(|l| l.placeholder.is_some_and(|p| p.custom_prompt))
            .and_then(Shape::text)
            .filter(|t| !body_is_empty(Some(t)))
            .map(TextBody::text);
        let text = custom.unwrap_or_else(|| cx.opts.prompts.prompt(kind).to_owned());
        if text.is_empty() {
            return;
        }
        let mut body = TextBody::from_text(&text);
        if let Some(own) = shape.text() {
            body.body = own.body.clone();
        }
        let laid = self.body(cx, shape.id, &body, text_rect);
        draw_body(
            out,
            &laid,
            self.engine.fonts(),
            glyphs,
            text_matrix(xfrm),
            !is_transformed(xfrm),
        );
    }

    fn chart(&mut self, pres: &Presentation, part: &str) -> Option<Arc<ChartSpace>> {
        if let Some(c) = self.charts.get(part) {
            return c.clone();
        }
        let parsed = pres
            .parts
            .get(part)
            .and_then(|p| kiln_charts::read::read_chart(&p.bytes).ok())
            .map(Arc::new);
        self.charts.insert(part.to_owned(), parsed.clone());
        parsed
    }
}

fn xfrm_text_turns(x: &Transform) -> bool {
    x.rot != 0 || x.flip_v
}

/// A fill with soft edges: layers of the shape shrunk towards its centre, each partly opaque, so
/// the fill fades out over `soft` px at the edge. `false` for fills that cannot fade.
fn soft_fill(
    out: &mut Vec<DrawOp>,
    p: &PathData,
    r: Bounds,
    m: Affine,
    soft: f32,
    fill: &Paintable,
    pics: &mut dyn PictureSource,
) -> bool {
    const LAYERS: u8 = 6;
    let a = 1.0 - 0.03f32.powf(1.0 / f32::from(LAYERS));
    let Some(layer) = crate::paint::faded(fill, a) else {
        return false;
    };
    let (w, h) = (r.w.max(1.0), r.h.max(1.0));
    let whole = transform_bounds(Bounds::new(0.0, 0.0, w, h), m);
    for i in 0..LAYERS {
        let d = (soft * f32::from(i) / f32::from(LAYERS)).min(w.min(h) / 2.0 - 0.5);
        let (sx, sy) = ((w - 2.0 * d) / w, (h - 2.0 * d) / h);
        let shrink = Affine::translate((f64::from(w) / 2.0, f64::from(h) / 2.0))
            * Affine::scale_non_uniform(f64::from(sx), f64::from(sy))
            * Affine::translate((-f64::from(w) / 2.0, -f64::from(h) / 2.0));
        fill_path(out, &transform_path(p, m * shrink), whole, &layer, pics);
    }
    true
}

/// The shape's reflection under it: its filled paths mirrored at the bottom edge, fading from
/// `st` to `end` opacity over `len` of its height, `gap` px below. Returns its reach below.
fn reflect(
    out: &mut Vec<DrawOp>,
    paths: &[(PathData, PathFillMode, bool, bool)],
    look: &ShapeLook,
    r: Bounds,
    m: Affine,
    (st, end, len, gap): (f32, f32, f32, f32),
    pics: &mut dyn PictureSource,
) -> f32 {
    if crate::paint::faded(&look.fill, 1.0).is_none() {
        return 0.0;
    }
    let scene = transform_bounds(Bounds::new(0.0, 0.0, r.w, r.h), m);
    let axis = f64::from(scene.bottom() + gap / 2.0);
    let mirror = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 2.0 * axis]);
    let mut mirrored = PathData::new();
    for (p, mode, _, _) in paths {
        if *mode != PathFillMode::None {
            mirrored.extend(&transform_path(p, mirror * m));
        }
    }
    let flipped = transform_bounds(scene, mirror);
    // A gradient mirrors with the shape: its stops run the other way.
    let fill = match &look.fill {
        Paintable::Gradient {
            stops,
            angle,
            focus,
        } => Paintable::Gradient {
            stops: stops.iter().rev().map(|(o, c)| (1.0 - o, *c)).collect(),
            angle: *angle,
            focus: *focus,
        },
        other => other.clone(),
    };
    let top = scene.bottom() + gap;
    let depth = scene.h * len;
    const BANDS: u8 = 8;
    for i in 0..BANDS {
        let t0 = f32::from(i) / f32::from(BANDS);
        let alpha = st + (end - st) * (t0 + 0.5 / f32::from(BANDS));
        let Some(band_fill) = crate::paint::faded(&fill, alpha) else {
            break;
        };
        let band = Bounds::new(
            scene.x - 1.0,
            top + depth * t0,
            scene.w + 2.0,
            depth / f32::from(BANDS) + 0.5,
        );
        out.push(DrawOp::PushClip {
            rect: band,
            path: None,
        });
        fill_path(out, &mirrored, flipped, &band_fill, pics);
        out.push(DrawOp::PopClip);
    }
    gap + depth
}

fn shadow_reach(look: &ShapeLook) -> f32 {
    let s = look
        .effects
        .shadow
        .map_or(0.0, |(dx, dy, blur, _)| dx.abs().max(dy.abs()) + blur * 2.0);
    let g = look.effects.glow.map_or(0.0, |(r, _)| r * 2.0);
    s.max(g)
}

/// Presets drawn as open lines.
#[must_use]
pub fn is_line_preset(name: &str) -> bool {
    matches!(
        name,
        "line"
            | "lineInv"
            | "straightConnector1"
            | "bentConnector2"
            | "bentConnector3"
            | "bentConnector4"
            | "bentConnector5"
            | "curvedConnector2"
            | "curvedConnector3"
            | "curvedConnector4"
            | "curvedConnector5"
            | "arc"
    )
}

/// The absolute transform of a shape at any depth (groups mapped through their child spaces;
/// top-level placeholders inherit their position).
#[must_use]
pub fn absolute_xfrm(pres: &Presentation, id: NodeId) -> Option<Transform> {
    let path = pres.locate(id)?;
    let shapes = pres.owner_shapes(path.owner)?;
    let first = *path.path.first()?;
    let top = shapes.get(first)?;
    let mut xfrm = if path.path.len() == 1 {
        return pres.resolved_xfrm(id).or(top.xfrm);
    } else {
        pres.resolved_xfrm(top.id).or(top.xfrm)?
    };
    let mut cur = top;
    for &i in &path.path[1..] {
        let ShapeKind::Group(g) = &cur.kind else {
            return None;
        };
        let child = g.children.get(i)?;
        xfrm = g.child_space.child_to_parent(&xfrm, &child.xfrm?);
        cur = child;
    }
    Some(xfrm)
}

#[cfg(test)]
mod prompt_tests {
    use super::{PlaceholderType, PromptLanguage};

    #[test]
    fn prompts_follow_the_interface_language() {
        assert_eq!(PromptLanguage::from_tag("ar-SA"), PromptLanguage::Arabic);
        assert_eq!(PromptLanguage::from_tag("de"), PromptLanguage::German);
        assert_eq!(PromptLanguage::from_tag("fr"), PromptLanguage::English);
        assert_eq!(PromptLanguage::from_tag(""), PromptLanguage::English);
        let ar = PromptLanguage::Arabic;
        assert_eq!(
            ar.prompt(PlaceholderType::CenteredTitle),
            "انقر لإضافة عنوان"
        );
        assert_eq!(
            ar.prompt(PlaceholderType::Subtitle),
            "انقر لإضافة عنوان فرعي"
        );
        assert_eq!(ar.prompt(PlaceholderType::Body), "انقر لإضافة نص");
        let de = PromptLanguage::German;
        assert_eq!(
            de.prompt(PlaceholderType::Title),
            "Titel durch Klicken hinzufügen"
        );
        assert_eq!(
            de.prompt(PlaceholderType::Body),
            "Text durch Klicken hinzufügen"
        );
        assert_eq!(
            super::prompt_for(PlaceholderType::Title),
            "Click to add title"
        );
        for lang in [PromptLanguage::English, de, ar] {
            assert_eq!(lang.prompt(PlaceholderType::SlideNumber), "");
        }
    }
}
