// SPDX-License-Identifier: AGPL-3.0-or-later
//! `.odp` → [`kiln_deck::Presentation`].
//!
//! - **Masters and layouts:** ODF has master pages but no layouts of the PowerPoint kind
//!   (LibreOffice writes one master page per PowerPoint layout). One Kiln master is built (its
//!   theme from LibreOffice's `loext:theme` when present, its title and body text styles from
//!   the first master page's presentation styles) and every master page becomes one of its
//!   layouts with the page's placeholders, decoration and background. The layout kind follows
//!   from the placeholders it has.
//! - **Slides** (`draw:page`): layout (`draw:master-page-name`), name, background, hidden,
//!   transition (`smil:type`/`smil:subtype` and the transition filter's duration, automatic
//!   advance), speaker notes (`presentation:notes`).
//! - **Shapes:** placeholders (`presentation:class`, matched to the layout's in order),
//!   text frames, pictures (with `fo:clip` cropping), tables, custom shapes (the preset named by
//!   `draw:type`, adjust values from `draw:modifiers`), rectangles and ellipses, lines and
//!   connectors, groups, media; position, size and rotation (`draw:transform`), fills and
//!   outlines ([`super::graphic`]), text with its body properties ([`super::text`]).

use std::collections::{BTreeMap, HashMap};

use kiln_common::NodeId;
use kiln_deck::geom::{ChildSpace, Transform};
use kiln_deck::shape::{
    AutoShape, CellProps, Connector, Crop, Geometry, Group, Media, MediaKind, MediaSource, Picture,
    Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps, Table, TableCell, TableRow,
};
use kiln_deck::slide::{
    Background, ColorMapping, HeaderFooter, Layout, LayoutKind, Master, Notes, Slide, SlideSize,
    SlideSizeKind,
};
use kiln_deck::text::{
    Autofit, BodyProps, LevelStyle, ListStyle, TextAnchor, TextBody, TextDirection, TextWrap,
};
use kiln_deck::transition::{Direction8, ShapeForm, Side, Transition, TransitionEffect};
use kiln_deck::{Part, Presentation};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::xml::XmlElement;

use super::graphic::{graphic_chain, read_fill, read_line};
use super::text::TextReader;
use crate::package::{OdfKind, Package};
use crate::props::{FontFaces, read_para_props, read_run_props};
use crate::text::styles::{Scope, StyleIndex};
use crate::units::emu;
use crate::xml::{kid, kids, parse_part};
use crate::{OdfError, meta};

/// A presentation as read.
#[derive(Debug)]
pub struct OdpRead {
    /// The presentation ([`Presentation::validate`] passes).
    pub presentation: Presentation,
    /// Document properties (`meta.xml`).
    pub properties: kiln_doc::DocProperties,
    /// What could not be represented exactly.
    pub warnings: Vec<String>,
    /// The file was a template (`.otp`).
    pub template: bool,
}

/// Reads an `.odp` (or `.otp`) file.
///
/// # Errors
///
/// [`OdfError`] when the file is not a readable ODF presentation.
pub fn read_odp(bytes: &[u8]) -> Result<OdpRead, OdfError> {
    let pkg = Package::open(bytes)?;
    read_package(&pkg)
}

fn placeholder_kind(class: &str) -> Option<PlaceholderType> {
    Some(match class {
        "title" => PlaceholderType::Title,
        "subtitle" => PlaceholderType::Subtitle,
        "outline" | "text" | "notes" => PlaceholderType::Body,
        "graphic" => PlaceholderType::Picture,
        "object" => PlaceholderType::Object,
        "chart" => PlaceholderType::Chart,
        "table" => PlaceholderType::Table,
        "orgchart" => PlaceholderType::Diagram,
        "date-time" => PlaceholderType::Date,
        "footer" => PlaceholderType::Footer,
        "page-number" => PlaceholderType::SlideNumber,
        "header" => PlaceholderType::Header,
        "page" => PlaceholderType::SlideImage,
        _ => return None,
    })
}

/// A `draw:transform` (`rotate (a) translate (x y)`, other operations ignored) applied to a
/// `w` × `h` box: the unrotated box's top-left corner and the clockwise rotation in 60 000ths
/// of a degree.
fn transform_of(t: &str, w: i64, h: i64) -> Option<(i64, i64, i32)> {
    let mut angle = 0.0f64;
    let mut tx = 0.0f64;
    let mut ty = 0.0f64;
    let mut rest = t;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let close = rest[open..].find(')')? + open;
        let args: Vec<&str> = rest[open + 1..close]
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|a| !a.is_empty())
            .collect();
        match name {
            "rotate" => {
                let a: f64 = args.first()?.parse().ok()?;
                angle += a;
            }
            "translate" => {
                tx += args.first().and_then(|a| emu(a)).unwrap_or(0) as f64;
                ty += args.get(1).and_then(|a| emu(a)).unwrap_or(0) as f64;
            }
            _ => {}
        }
        rest = &rest[close + 1..];
    }
    if !angle.is_finite() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (wf, hf) = (w as f64, h as f64);
    let (s, c) = angle.sin_cos();
    // The box is rotated counter-clockwise about its top-left corner, then moved: its centre.
    let cx = tx + wf / 2.0 * c + hf / 2.0 * s;
    let cy = ty - wf / 2.0 * s + hf / 2.0 * c;
    let deg = (-angle.to_degrees()).rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation)]
    let rot = ((deg * 60_000.0).round() as i64 % 21_600_000) as i32;
    #[allow(clippy::cast_possible_truncation)]
    Some((
        (cx - wf / 2.0).round() as i64,
        (cy - hf / 2.0).round() as i64,
        rot,
    ))
}

struct Reader<'a> {
    pkg: &'a Package,
    ix: StyleIndex<'a>,
    fonts: FontFaces,
    styles_root: Option<&'a XmlElement>,
    pres: Presentation,
    media: HashMap<String, String>,
    warnings: Vec<String>,
}

impl<'a> Reader<'a> {
    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_owned());
        }
    }

    fn id(&mut self) -> NodeId {
        self.pres.alloc_id()
    }

    fn media_key(&mut self, href: &str) -> Option<String> {
        let path = href.trim_start_matches("./").to_owned();
        if let Some(k) = self.media.get(&path) {
            return Some(k.clone());
        }
        let bytes = self.pkg.file(&path)?.to_vec();
        let base = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let mut key = format!("/ppt/media/{base}");
        let mut n = 1;
        while self.pres.parts.contains_key(&key) {
            n += 1;
            key = format!("/ppt/media/{n}-{base}");
        }
        self.pres.parts.insert(
            key.clone(),
            Part {
                content_type: self.pkg.media_type_of(&path),
                bytes,
            },
        );
        self.media.insert(path, key.clone());
        Some(key)
    }

    fn graphic(&self, scope: Scope, e: &XmlElement) -> Vec<&'a XmlElement> {
        let mut chain = graphic_chain(&self.ix, scope, "graphic", e.attr("draw:style-name"));
        chain.extend(graphic_chain(
            &self.ix,
            scope,
            "presentation",
            e.attr("presentation:style-name"),
        ));
        chain
    }

    fn props(&mut self, chain: &[&XmlElement], geometry: Option<Geometry>) -> ShapeProps {
        let styles = self.styles_root;
        let mut media = |h: &str| self.media_key(h);
        let fill = read_fill(chain, styles, &mut media);
        ShapeProps {
            geometry,
            fill,
            line: read_line(chain, styles),
            effects: super::graphic::read_effects(chain),
            ..ShapeProps::default()
        }
    }

    fn body_props(chain: &[&XmlElement]) -> BodyProps {
        let a = |n: &str| super::graphic::chain_attr(chain, n);
        let mut b = BodyProps::default();
        for (i, side) in ["left", "top", "right", "bottom"].iter().enumerate() {
            if let Some(v) = a(&format!("fo:padding-{side}")).and_then(emu) {
                b.insets[i] = Some(v);
            }
        }
        b.anchor = a("draw:textarea-vertical-align").and_then(|v| match v {
            "top" => Some(TextAnchor::Top),
            "middle" => Some(TextAnchor::Middle),
            "bottom" => Some(TextAnchor::Bottom),
            "justify" => Some(TextAnchor::Justified),
            _ => None,
        });
        if a("style:shrink-to-fit") == Some("true")
            || a("draw:fit-to-size") == Some("shrink-to-fit")
        {
            b.autofit = Some(Autofit::SHRINK);
        } else if a("draw:auto-grow-height") == Some("true") {
            b.autofit = Some(Autofit::ResizeShape);
        } else if a("draw:auto-grow-height") == Some("false") {
            b.autofit = Some(Autofit::Off);
        }
        match a("fo:wrap-option") {
            Some("no-wrap") => b.wrap = Some(TextWrap::None),
            Some("wrap") => b.wrap = Some(TextWrap::Square),
            _ => {}
        }
        if a("loext:writing-mode") == Some("bt-lr") {
            b.direction = Some(TextDirection::Rotate270);
        } else if a("style:writing-mode") == Some("tb-rl") {
            b.direction = Some(TextDirection::Rotate90);
        }
        b
    }

    fn text(&mut self, scope: Scope, el: &XmlElement) -> Option<TextBody> {
        let ix = &self.ix;
        let fonts = &self.fonts;
        let reader = TextReader { ix, scope, fonts };
        // Ids and media come from the reader's own state: collect them first.
        let mut ids: Vec<NodeId> = Vec::new();
        let start = self.pres.peek_next_id().0;
        let mut next = start;
        let mut alloc = || {
            let id = NodeId(next);
            next += 1;
            ids.push(id);
            id
        };
        let mut wanted: Vec<String> = Vec::new();
        let mut media = |h: &str| {
            wanted.push(h.to_owned());
            Some(format!("odf-media:{h}"))
        };
        let body = reader.body(el, &mut alloc, &mut media);
        for _ in start..next {
            self.pres.alloc_id();
        }
        let mut body = body?;
        // Picture bullets: the placeholders become part keys.
        if !wanted.is_empty() {
            let mut keys = HashMap::new();
            for h in wanted {
                if let Some(k) = self.media_key(&h) {
                    keys.insert(format!("odf-media:{h}"), k);
                }
            }
            let mut fixed = Vec::new();
            let mut props = BTreeMap::new();
            for p in body.paragraphs() {
                fixed.push(p.clone());
                if let Some(d) = body.deck_props(p.id) {
                    let mut d = d.clone();
                    if let Some(kiln_deck::text::Bullet::Picture(k)) = &d.bullet
                        && let Some(real) = keys.get(k)
                    {
                        d.bullet = Some(kiln_deck::text::Bullet::Picture(real.clone()));
                    }
                    props.insert(p.id, d);
                }
            }
            if let Ok(b) = TextBody::from_paragraphs(fixed, props) {
                body = b;
            }
        }
        Some(body)
    }

    fn xfrm(e: &XmlElement) -> Option<Transform> {
        let w = e.attr("svg:width").and_then(emu).unwrap_or(0).max(0);
        let h = e.attr("svg:height").and_then(emu).unwrap_or(0).max(0);
        let (mut x, mut y) = (
            e.attr("svg:x").and_then(emu).unwrap_or(0),
            e.attr("svg:y").and_then(emu).unwrap_or(0),
        );
        let mut rot = 0;
        if let Some(t) = e.attr("draw:transform")
            && let Some((tx, ty, r)) = transform_of(t, w, h)
        {
            x = tx;
            y = ty;
            rot = r;
        }
        Some(Transform {
            x,
            y,
            cx: w,
            cy: h,
            rot,
            flip_h: false,
            flip_v: false,
        })
    }

    fn base_shape(&mut self, e: &XmlElement, kind: ShapeKind) -> Shape {
        let id = self.id();
        let mut s = Shape::new(
            id,
            e.attr("draw:name").unwrap_or_default(),
            Self::xfrm(e),
            kind,
        );
        s.title = kid(e, "svg:title")
            .map(crate::xml::all_text)
            .filter(|t| !t.is_empty());
        s.alt_text = kid(e, "svg:desc")
            .map(crate::xml::all_text)
            .filter(|t| !t.is_empty());
        s
    }

    #[allow(clippy::too_many_lines)]
    fn shape(
        &mut self,
        e: &'a XmlElement,
        scope: Scope,
        counters: &mut HashMap<PlaceholderType, u32>,
    ) -> Option<Shape> {
        let chain = self.graphic(scope, e);
        let mut shape = match e.name.as_str() {
            "draw:frame" => {
                let class = e.attr("presentation:class");
                let ph = class.and_then(placeholder_kind);
                // A table (or an object) frame may also hold a replacement picture: the table
                // comes first.
                let table = kid(e, "table:table").and_then(|t| self.table(t, scope));
                if let Some(table) = table {
                    self.base_shape(e, ShapeKind::Table(table))
                } else if let Some(img) = kid(e, "draw:image").filter(|_| {
                    kid(e, "draw:object").is_none() && kid(e, "draw:object-ole").is_none()
                }) {
                    let media = img.attr("xlink:href").and_then(|h| self.media_key(h));
                    match media {
                        Some(m) => {
                            let crop = match (
                                super::graphic::chain_attr(&chain, "fo:clip"),
                                self.pres
                                    .parts
                                    .get(&m)
                                    .and_then(|p| super::graphic::image_size(&p.bytes)),
                            ) {
                                (Some(c), Some(px)) => super::graphic::crop_of(c, px),
                                _ => Crop::default(),
                            };
                            let props = self.props(&chain, Some(Geometry::preset("rect")));
                            let mut s = self.base_shape(
                                e,
                                ShapeKind::Picture(Picture {
                                    media: m,
                                    crop,
                                    props,
                                    alpha: None,
                                }),
                            );
                            if let Some(k) = ph {
                                s.placeholder = Some(self.placeholder(k, counters));
                            }
                            s
                        }
                        None if ph.is_none() => {
                            self.warn("a picture without its image data was skipped");
                            return None;
                        }
                        None => {
                            let body = Self::body_props(&chain);
                            let mut text = TextBody::new();
                            text.body = body;
                            let props = self.props(&chain, None);
                            self.base_shape(
                                e,
                                ShapeKind::Geometry(AutoShape {
                                    text_box: false,
                                    props,
                                    text: Some(text),
                                }),
                            )
                        }
                    }
                } else if let Some(p) = kid(e, "draw:plugin") {
                    let href = p.attr("xlink:href").unwrap_or_default();
                    let mime = p.attr("draw:mime-type").unwrap_or_default();
                    let source = if href.starts_with("http")
                        || href.starts_with('/')
                        || href.contains("://")
                    {
                        MediaSource::Linked(href.to_owned())
                    } else {
                        match self.media_key(href) {
                            Some(k) => MediaSource::Embedded(k),
                            None => MediaSource::Linked(href.to_owned()),
                        }
                    };
                    let props = self.props(&chain, Some(Geometry::preset("rect")));
                    self.base_shape(
                        e,
                        ShapeKind::Media(Media {
                            kind: if mime.starts_with("audio") {
                                MediaKind::Audio
                            } else {
                                MediaKind::Video
                            },
                            source,
                            poster: None,
                            trim_start_ms: 0,
                            trim_end_ms: 0,
                            props,
                        }),
                    )
                } else if let Some(cs) = kid(e, "draw:object")
                    .and_then(|o| o.attr("xlink:href"))
                    .and_then(|h| crate::chart::read_linked(self.pkg, h))
                {
                    let mut n = self.pres.parts.len() + 1;
                    let mut part = format!("/ppt/charts/chart{n}.xml");
                    while self.pres.parts.contains_key(&part) {
                        n += 1;
                        part = format!("/ppt/charts/chart{n}.xml");
                    }
                    self.pres.parts.insert(
                        part.clone(),
                        Part {
                            content_type: kiln_charts::model::PartFormat::Classic
                                .content_type()
                                .to_owned(),
                            bytes: crate::chart::to_drawingml(&cs),
                        },
                    );
                    self.base_shape(e, ShapeKind::Chart(kiln_deck::shape::ChartRef { part }))
                } else if kid(e, "draw:object").is_some() || kid(e, "draw:object-ole").is_some() {
                    self.warn("embedded objects in presentations are imported as their replacement picture");
                    let fallback = kids(e, "draw:image")
                        .next()
                        .and_then(|i| i.attr("xlink:href"))
                        .and_then(|h| self.media_key(h))?;
                    let props = self.props(&chain, Some(Geometry::preset("rect")));
                    self.base_shape(
                        e,
                        ShapeKind::Picture(Picture {
                            media: fallback,
                            crop: Crop::default(),
                            props,
                            alpha: None,
                        }),
                    )
                } else {
                    let tb = kid(e, "draw:text-box");
                    let mut text = tb.and_then(|t| self.text(scope, t));
                    let body = Self::body_props(&chain);
                    if ph.is_some() || tb.is_some() {
                        let t = text.get_or_insert_with(TextBody::new);
                        t.body = body;
                    }
                    let props = self.props(
                        &chain,
                        if ph.is_some() {
                            None
                        } else {
                            Some(Geometry::preset("rect"))
                        },
                    );
                    let mut s = self.base_shape(
                        e,
                        ShapeKind::Geometry(AutoShape {
                            text_box: ph.is_none(),
                            props,
                            text,
                        }),
                    );
                    if let Some(k) = ph {
                        s.placeholder = Some(self.placeholder(k, counters));
                    }
                    s
                }
            }
            "draw:custom-shape"
            | "draw:rect"
            | "draw:ellipse"
            | "draw:polygon"
            | "draw:path"
            | "draw:regular-polygon" => {
                let geo = kid(e, "draw:enhanced-geometry");
                let preset = match e.name.as_str() {
                    "draw:ellipse" => "ellipse".to_owned(),
                    "draw:custom-shape" => geo
                        .and_then(|g| g.attr("draw:type"))
                        .map_or_else(|| "rect".to_owned(), super::shapes::preset_of_odf),
                    "draw:rect" => {
                        if e.attr("draw:corner-radius").is_some() {
                            "roundRect".to_owned()
                        } else {
                            "rect".to_owned()
                        }
                    }
                    _ => {
                        self.warn("freeform shapes are imported as rectangles");
                        "rect".to_owned()
                    }
                };
                let adjust = geo
                    .and_then(|g| g.attr("draw:modifiers"))
                    .map(|m| modifiers(&preset, m))
                    .unwrap_or_default();
                let flip_h = geo.and_then(|g| g.attr("draw:mirror-horizontal")) == Some("true");
                let flip_v = geo.and_then(|g| g.attr("draw:mirror-vertical")) == Some("true");
                // LibreOffice writes an empty paragraph into every shape: that is no text.
                let mut text = self.text(scope, e).filter(|t| !t.is_empty());
                if let Some(t) = &mut text {
                    t.body = Self::body_props(&chain);
                }
                let props = self.props(
                    &chain,
                    Some(Geometry::Preset {
                        name: preset,
                        adjust,
                    }),
                );
                let mut s = self.base_shape(
                    e,
                    ShapeKind::Geometry(AutoShape {
                        text_box: false,
                        props,
                        text,
                    }),
                );
                if let Some(x) = &mut s.xfrm {
                    x.flip_h = flip_h;
                    x.flip_v = flip_v;
                }
                s
            }
            "draw:line" | "draw:connector" => {
                let p = |n: &str| e.attr(n).and_then(emu).unwrap_or(0);
                let (x1, y1, x2, y2) = (p("svg:x1"), p("svg:y1"), p("svg:x2"), p("svg:y2"));
                let preset = if e.name == "draw:line" {
                    "line"
                } else {
                    match e.attr("draw:type") {
                        Some("standard") => "bentConnector3",
                        Some("curve") => "curvedConnector3",
                        _ => "straightConnector1",
                    }
                };
                let props = self.props(&chain, Some(Geometry::preset(preset)));
                let id = self.id();
                let mut s = Shape::new(
                    id,
                    e.attr("draw:name").unwrap_or_default(),
                    Some(Transform {
                        x: x1.min(x2),
                        y: y1.min(y2),
                        cx: (x2 - x1).abs(),
                        cy: (y2 - y1).abs(),
                        rot: 0,
                        flip_h: x2 < x1,
                        flip_v: y2 < y1,
                    }),
                    ShapeKind::Connector(Connector {
                        props,
                        start: None,
                        end: None,
                    }),
                );
                s.title = kid(e, "svg:title").map(crate::xml::all_text);
                s
            }
            "draw:g" => {
                let mut children = Vec::new();
                for c in e.elements() {
                    if let Some(s) = self.shape(c, scope, counters) {
                        children.push(s);
                    }
                }
                if children.is_empty() {
                    return None;
                }
                let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
                for c in &children {
                    if let Some(t) = c.xfrm {
                        x0 = x0.min(t.x);
                        y0 = y0.min(t.y);
                        x1 = x1.max(t.x + t.cx);
                        y1 = y1.max(t.y + t.cy);
                    }
                }
                if x0 > x1 {
                    (x0, y0, x1, y1) = (0, 0, 0, 0);
                }
                let id = self.id();
                Shape::new(
                    id,
                    e.attr("draw:name").unwrap_or_default(),
                    Some(Transform {
                        x: x0,
                        y: y0,
                        cx: x1 - x0,
                        cy: y1 - y0,
                        rot: 0,
                        flip_h: false,
                        flip_v: false,
                    }),
                    ShapeKind::Group(Group {
                        child_space: ChildSpace {
                            x: x0,
                            y: y0,
                            cx: x1 - x0,
                            cy: y1 - y0,
                        },
                        children,
                        props: ShapeProps::default(),
                    }),
                )
            }
            _ => return None,
        };
        // Placeholders that were not moved on the slide keep the layout's position.
        if shape.placeholder.is_some()
            && e.attr("presentation:user-transformed") != Some("true")
            && scope == Scope::Content
        {
            shape.xfrm = None;
        }
        if let Some(href) = kid(e, "office:event-listeners")
            .and_then(|l| {
                l.descendants("presentation:event-listener")
                    .into_iter()
                    .next()
            })
            .and_then(|l| l.attr("xlink:href"))
        {
            shape.link = Some(href.to_owned());
        }
        Some(shape)
    }

    fn placeholder(
        &self,
        kind: PlaceholderType,
        counters: &mut HashMap<PlaceholderType, u32>,
    ) -> Placeholder {
        let idx = if kind == PlaceholderType::Title {
            0
        } else {
            let n = counters.entry(kind).or_insert(0);
            *n += 1;
            *n
        };
        Placeholder::new(kind, idx)
    }

    fn table(&mut self, t: &XmlElement, scope: Scope) -> Option<Table> {
        let mut grid = Vec::new();
        for c in t.descendants("table:table-column") {
            let n = c
                .attr("table:number-columns-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, kiln_deck::shape::MAX_TABLE_COLS);
            let w = c
                .attr("table:style-name")
                .and_then(|s| self.ix.get(scope, "table-column", s))
                .and_then(|s| kid(s, "style:table-column-properties"))
                .and_then(|p| p.attr("style:column-width"))
                .and_then(emu)
                .unwrap_or(914_400);
            for _ in 0..n {
                grid.push(w);
            }
        }
        let mut rows = Vec::new();
        for r in t.descendants("table:table-row") {
            let height = r
                .attr("table:style-name")
                .and_then(|s| self.ix.get(scope, "table-row", s))
                .and_then(|s| kid(s, "style:table-row-properties"))
                .and_then(|p| {
                    p.attr("style:row-height")
                        .or_else(|| p.attr("style:min-row-height"))
                })
                .and_then(emu)
                .unwrap_or(370_840);
            let row_style = r.attr("table:default-cell-style-name");
            let mut cells = Vec::new();
            for c in r.elements() {
                let covered = c.name == "table:covered-table-cell";
                if !covered && c.name != "table:table-cell" {
                    continue;
                }
                let mut cell = TableCell::new();
                cell.text = self.text(scope, c).unwrap_or_default();
                if covered {
                    cell.h_merge = true;
                }
                cell.grid_span = c
                    .attr("table:number-columns-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1);
                cell.row_span = c
                    .attr("table:number-rows-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1);
                // The cell's style, or the row's default (how LibreOffice writes a table's
                // look); covered cells' `standard` style says nothing.
                if let Some(st) = c
                    .attr("table:style-name")
                    .or(row_style)
                    .filter(|_| !covered)
                    .and_then(|s| self.ix.get(scope, "table-cell", s))
                {
                    // Fill and padding in `loext:graphic-properties`, borders in the
                    // paragraph properties (LibreOffice), or the standard elements.
                    let gp = kid(st, "style:graphic-properties")
                        .or_else(|| kid(st, "loext:graphic-properties"));
                    let tc = kid(st, "style:table-cell-properties");
                    let pp = kid(st, "style:paragraph-properties");
                    let chain: Vec<&XmlElement> = gp.into_iter().collect();
                    let styles = self.styles_root;
                    let mut media = |h: &str| self.media_key(h);
                    let mut props = CellProps {
                        fill: read_fill(&chain, styles, &mut media),
                        ..CellProps::default()
                    };
                    if props.fill.is_none()
                        && let Some(bg) = tc
                            .and_then(|t| t.attr("fo:background-color"))
                            .and_then(|v| v.strip_prefix('#'))
                    {
                        props.fill = Some(kiln_ooxml::drawingml::fill::Fill::Solid(
                            DrawingColor::srgb(bg),
                        ));
                    }
                    let sources: Vec<&XmlElement> = tc.into_iter().chain(pp).chain(gp).collect();
                    let find = |name: &str| sources.iter().find_map(|e| e.attr(name));
                    for (i, side) in ["left", "right", "top", "bottom"].iter().enumerate() {
                        let b = find(&format!("fo:border-{side}")).or_else(|| find("fo:border"));
                        props.borders[i] = kiln_deck::shape::CellBorder(b.and_then(border_line));
                        props.margins[i] = find(&format!("fo:padding-{side}"))
                            .or_else(|| find("fo:padding"))
                            .and_then(emu);
                    }
                    props.anchor = find("style:vertical-align")
                        .or_else(|| find("draw:textarea-vertical-align"))
                        .and_then(|v| match v {
                            "middle" => Some(TextAnchor::Middle),
                            "bottom" => Some(TextAnchor::Bottom),
                            "top" => Some(TextAnchor::Top),
                            _ => None,
                        });
                    cell.props = props;
                }
                cells.push(cell);
            }
            rows.push(TableRow { height, cells });
        }
        if rows.is_empty() || grid.is_empty() {
            return None;
        }
        let width = grid.len();
        for r in &mut rows {
            r.cells.resize_with(width, TableCell::new);
            r.cells.truncate(width);
        }
        // Rows covered by a cell above are vertical merges.
        for ri in 0..rows.len() {
            for ci in 0..width {
                let span = rows[ri].cells[ci].row_span;
                for k in 1..span as usize {
                    if let Some(r) = rows.get_mut(ri + k) {
                        r.cells[ci].v_merge = true;
                        r.cells[ci].h_merge = false;
                    }
                }
            }
        }
        Some(Table {
            grid,
            rows,
            style_id: None,
            look: Default::default(),
            rtl: false,
        })
    }

    fn background(&mut self, scope: Scope, style: Option<&str>) -> Option<Background> {
        let st = style.and_then(|n| self.ix.get(scope, "drawing-page", n))?;
        let dp = kid(st, "style:drawing-page-properties")?;
        let fill_kind = dp.attr("draw:fill")?;
        if fill_kind == "none" {
            return None;
        }
        let styles = self.styles_root;
        let mut media = |h: &str| self.media_key(h);
        let fill = read_fill(&[dp], styles, &mut media)?;
        Some(Background::Fill {
            fill,
            shade_to_title: false,
        })
    }

    fn level_style(&self, style: &str) -> Option<LevelStyle> {
        let st = self.ix.get(Scope::Styles, "presentation", style)?;
        let mut l = LevelStyle::default();
        if let Some(p) = kid(st, "style:paragraph-properties") {
            let mut pp = read_para_props(p);
            pp.spacing = None;
            l.para = pp;
        }
        if let Some(t) = kid(st, "style:text-properties") {
            l.run = read_run_props(t, &self.fonts);
        }
        (!l.is_empty()).then_some(l)
    }

    fn master_styles(&self, page: &str) -> (ListStyle, ListStyle) {
        let mut title = ListStyle::default();
        if let Some(l) = self.level_style(&format!("{page}-title")) {
            title.set_level(0, l);
        }
        let mut body = ListStyle::default();
        for n in 1..=9 {
            if let Some(l) = self.level_style(&format!("{page}-outline{n}")) {
                body.set_level(n - 1, l);
            }
        }
        (title, body)
    }

    #[allow(clippy::too_many_lines)]
    fn read(&mut self, content: &'a XmlElement) -> Result<(), OdfError> {
        let body = kid(content, "office:body")
            .and_then(|b| kid(b, "office:presentation"))
            .ok_or_else(|| OdfError::Corrupt("content.xml has no office:presentation".into()))?;
        let masters: Vec<&XmlElement> = self.ix.master_pages.clone();
        // Slide size from the first master's page layout.
        if let Some(pl) = masters
            .first()
            .and_then(|m| m.attr("style:page-layout-name"))
            .and_then(|n| self.ix.page_layouts.get(n))
            .and_then(|l| kid(l, "style:page-layout-properties"))
        {
            let w = pl.attr("fo:page-width").and_then(emu);
            let h = pl.attr("fo:page-height").and_then(emu);
            if let (Some(w), Some(h)) = (w, h)
                && w > 0
                && h > 0
            {
                self.pres.slide_size = size_of(w, h);
            }
        }
        let master_id = self.id();
        let mut theme = kiln_ooxml::drawingml::theme::office_theme();
        if let Some(t) = masters.first().and_then(|m| kid(m, "loext:theme")) {
            read_theme(t, &mut theme);
        }
        let first_name = masters
            .first()
            .and_then(|m| m.attr("style:name"))
            .unwrap_or("Default")
            .to_owned();
        let (title_style, body_style) = self.master_styles(&first_name);
        let mut layouts = Vec::new();
        let mut layout_ids: HashMap<String, NodeId> = HashMap::new();
        let mut master_bg = None;
        for m in &masters {
            let name = m.attr("style:name").unwrap_or_default().to_owned();
            let display = m
                .attr("style:display-name")
                .map_or_else(|| name.replace("_20_", " "), str::to_owned);
            let id = self.id();
            let mut layout = Layout::new(id, LayoutKind::Custom);
            layout.name = display;
            let mut counters = HashMap::new();
            for e in m.elements() {
                if let Some(s) = self.shape(e, Scope::Styles, &mut counters) {
                    layout.shapes.push(s);
                }
            }
            // Per-page text styles that differ from the master's go on the placeholders.
            let (t, b) = self.master_styles(&name);
            for s in &mut layout.shapes {
                if let (Some(ph), ShapeKind::Geometry(a)) = (s.placeholder, &mut s.kind)
                    && let Some(text) = &mut a.text
                {
                    match ph.kind {
                        PlaceholderType::Title if t != title_style => text.list_style = t.clone(),
                        PlaceholderType::Body | PlaceholderType::Object if b != body_style => {
                            text.list_style = b.clone();
                        }
                        _ => {}
                    }
                }
            }
            layout.kind = layout_kind(&layout.shapes);
            layout.background = self.background(Scope::Styles, m.attr("draw:style-name"));
            if master_bg.is_none() {
                master_bg.clone_from(&layout.background);
            }
            layout_ids.insert(name, id);
            layouts.push(layout);
        }
        if layouts.is_empty() {
            let id = self.id();
            layouts.push(Layout::new(id, LayoutKind::Blank));
        }
        let first_layout = layouts[0].id;
        self.pres.masters.push(Master {
            id: master_id,
            name: theme.name.clone(),
            theme,
            color_map: ColorMapping::default(),
            background: master_bg.unwrap_or_else(|| Background::solid(DrawingColor::scheme("bg1"))),
            shapes: Vec::new(),
            title_style,
            body_style,
            other_style: ListStyle::default(),
            layouts,
            header_footer: HeaderFooter::default(),
            preserve: true,
        });
        // Slides.
        for page in kids(body, "draw:page") {
            let layout = page
                .attr("draw:master-page-name")
                .and_then(|n| layout_ids.get(n).copied())
                .unwrap_or(first_layout);
            let id = self.id();
            let mut slide = Slide::new(id, layout);
            slide.name = page.attr("draw:name").map(str::to_owned);
            let mut counters = HashMap::new();
            for e in page.elements() {
                if e.name == "officeooo:annotation" {
                    let pos = |n: &str| e.attr(n).and_then(emu).unwrap_or(0).max(0);
                    let text = kids(e, "text:p")
                        .map(crate::xml::all_text)
                        .collect::<Vec<_>>()
                        .join("\n");
                    let id = self.id();
                    slide.comments.push(kiln_deck::slide::Comment {
                        id,
                        author: kid(e, "dc:creator")
                            .map(crate::xml::all_text)
                            .unwrap_or_default(),
                        initials: kid(e, "meta:creator-initials")
                            .map(crate::xml::all_text)
                            .unwrap_or_default(),
                        created: kid(e, "dc:date")
                            .map(crate::xml::all_text)
                            .unwrap_or_default(),
                        text,
                        shape: None,
                        pos: (pos("svg:x"), pos("svg:y")),
                        replies: Vec::new(),
                        resolved: false,
                    });
                    continue;
                }
                if e.name == "presentation:notes" {
                    let notes = kids(e, "draw:frame")
                        .find(|f| f.attr("presentation:class") == Some("notes"))
                        .and_then(|f| kid(f, "draw:text-box"))
                        .and_then(|tb| self.text(Scope::Content, tb));
                    if let Some(text) = notes
                        && !text.is_empty()
                    {
                        slide.notes = Some(Notes {
                            text,
                            shapes: Vec::new(),
                        });
                    }
                    continue;
                }
                if let Some(s) = self.shape(e, Scope::Content, &mut counters) {
                    slide.shapes.push(s);
                }
            }
            slide.background = self.background(Scope::Content, page.attr("draw:style-name"));
            if let Some(dp) = page
                .attr("draw:style-name")
                .and_then(|n| self.ix.get(Scope::Content, "drawing-page", n))
                .and_then(|s| kid(s, "style:drawing-page-properties"))
            {
                slide.hidden = dp.attr("presentation:visibility") == Some("hidden");
                slide.transition = transition_of(dp, page);
            }
            self.pres.slides.push(slide);
        }
        if self.pres.slides.is_empty() {
            self.warn("the presentation has no slides");
        }
        Ok(())
    }
}

fn border_line(s: &str) -> Option<kiln_ooxml::drawingml::line::Line> {
    let mut l = kiln_ooxml::drawingml::line::Line::default();
    let mut any = false;
    for tok in s.split_whitespace() {
        if tok == "none" || tok == "hidden" {
            l.fill = Some(kiln_ooxml::drawingml::fill::Fill::None);
            return Some(l);
        }
        if let Some(hex) = tok.strip_prefix('#') {
            l.fill = Some(kiln_ooxml::drawingml::fill::Fill::Solid(
                DrawingColor::srgb(hex),
            ));
            any = true;
        } else if let Some(w) = emu(tok) {
            l.width = Some(w);
            any = true;
        }
    }
    any.then_some(l)
}

fn modifiers(preset: &str, m: &str) -> Vec<(String, i64)> {
    let Ok(g) = kiln_ooxml::drawingml::presets::compiled_preset(preset) else {
        return Vec::new();
    };
    let names: Vec<String> = g.adjust_names().map(str::to_owned).collect();
    let defaults = g.evaluate(21_600.0, 21_600.0, &[]);
    m.split_whitespace()
        .zip(names)
        .filter_map(|(v, n)| {
            let v: f64 = v.parse().ok()?;
            let d = defaults.get(&n).unwrap_or(f64::NAN);
            #[allow(clippy::cast_possible_truncation)]
            ((v - d).abs() > 0.5).then(|| (n, v.round() as i64))
        })
        .collect()
}

fn read_theme(t: &XmlElement, theme: &mut kiln_ooxml::drawingml::theme::Theme) {
    if let Some(n) = t.attr("loext:name") {
        theme.name = n.to_owned();
    }
    let Some(colors) = kid(t, "loext:theme-colors") else {
        return;
    };
    const SLOTS: [&str; 12] = [
        "dark1",
        "light1",
        "dark2",
        "light2",
        "accent1",
        "accent2",
        "accent3",
        "accent4",
        "accent5",
        "accent6",
        "hyperlink",
        "followed-hyperlink",
    ];
    for c in kids(colors, "loext:color") {
        let (Some(name), Some(v)) = (c.attr("loext:name"), c.attr("loext:color")) else {
            continue;
        };
        if let (Some(i), Some(hex)) = (SLOTS.iter().position(|s| *s == name), v.strip_prefix('#')) {
            theme.color_scheme.colors[i] = DrawingColor::srgb(hex);
        }
    }
    if let Some(n) = colors.attr("loext:name") {
        theme.color_scheme.name = n.to_owned();
    }
}

/// The theme colour slot names LibreOffice uses, in DrawingML scheme order.
pub const THEME_SLOTS: [&str; 12] = [
    "dark1",
    "light1",
    "dark2",
    "light2",
    "accent1",
    "accent2",
    "accent3",
    "accent4",
    "accent5",
    "accent6",
    "hyperlink",
    "followed-hyperlink",
];

fn size_of(w: i64, h: i64) -> SlideSize {
    for s in [SlideSize::WIDESCREEN, SlideSize::STANDARD] {
        if (s.cx - w).abs() <= 1_000 && (s.cy - h).abs() <= 1_000 {
            return s;
        }
    }
    let kind = if (w * 10 - h * 16).abs() < w / 50 {
        SlideSizeKind::Screen16x10
    } else {
        SlideSizeKind::Custom
    };
    SlideSize {
        cx: w.clamp(1, SlideSize::MAX_SIDE),
        cy: h.clamp(1, SlideSize::MAX_SIDE),
        kind,
    }
}

fn layout_kind(shapes: &[Shape]) -> LayoutKind {
    let count = |k: PlaceholderType| {
        shapes
            .iter()
            .filter(|s| s.placeholder.is_some_and(|p| p.kind == k))
            .count()
    };
    let title = count(PlaceholderType::Title);
    let sub = count(PlaceholderType::Subtitle);
    let body = count(PlaceholderType::Body) + count(PlaceholderType::Object);
    let pic = count(PlaceholderType::Picture);
    match (title, sub, body, pic) {
        (0, 0, 0, 0) => LayoutKind::Blank,
        (1, 1, 0, 0) => LayoutKind::Title,
        (1, 0, 0, 0) => LayoutKind::TitleOnly,
        (1, 0, 1, 0) => LayoutKind::TitleContent,
        (1, 0, 2, 0) => LayoutKind::TwoContent,
        (1, 0, 4, 0) => LayoutKind::Comparison,
        (1, 0, 1, 1) => LayoutKind::PictureCaption,
        _ => LayoutKind::Custom,
    }
}

fn duration_ms(s: &str) -> Option<u32> {
    let t = s.trim();
    let secs = if let Some(v) = t.strip_suffix("ms") {
        v.parse::<f64>().ok()? / 1000.0
    } else if let Some(v) = t.strip_suffix('s') {
        v.parse::<f64>().ok()?
    } else if t.starts_with('P') {
        crate::units::duration_days(t)? * 86_400.0
    } else {
        t.parse::<f64>().ok()?
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as u32)
}

fn transition_of(dp: &XmlElement, page: &XmlElement) -> Option<Transition> {
    let ty = dp.attr("smil:type");
    let auto = dp.attr("presentation:transition-type") == Some("automatic");
    let advance = if auto {
        dp.attr("presentation:duration").and_then(duration_ms)
    } else {
        None
    };
    let filter = page.descendants("anim:transitionFilter").into_iter().next();
    let dur = filter
        .and_then(|f| f.attr("smil:dur"))
        .and_then(duration_ms)
        .or_else(|| match dp.attr("presentation:transition-speed") {
            Some("slow") => Some(1_000),
            Some("fast") => Some(500),
            Some("medium") => Some(750),
            _ => None,
        });
    let ty = ty?;
    let sub = dp.attr("smil:subtype").unwrap_or_default();
    let reverse = dp.attr("smil:direction") == Some("reverse");
    let dir8 = |s: &str| match s {
        "fromRight" => Direction8::Right,
        "fromTop" => Direction8::Up,
        "fromBottom" => Direction8::Down,
        "fromTopLeft" => Direction8::LeftUp,
        "fromTopRight" => Direction8::RightUp,
        "fromBottomLeft" => Direction8::LeftDown,
        "fromBottomRight" => Direction8::RightDown,
        _ => Direction8::Left,
    };
    let effect = match ty {
        "fade" if dur == Some(0) => TransitionEffect::Cut {
            through_black: sub == "fadeOverColor",
        },
        "fade" => match sub {
            "fadeFromColor" => TransitionEffect::Flash,
            _ => TransitionEffect::Fade {
                through_black: sub == "fadeOverColor",
            },
        },
        "pushWipe" => TransitionEffect::Push {
            dir: match sub {
                "fromLeft" => Side::Right,
                "fromTop" => Side::Down,
                "fromBottom" => Side::Up,
                _ => Side::Left,
            },
        },
        "barWipe" => TransitionEffect::Wipe {
            dir: match (sub, reverse) {
                ("topToBottom", false) => Side::Down,
                ("topToBottom", true) => Side::Up,
                (_, true) => Side::Right,
                _ => Side::Left,
            },
        },
        "barnDoorWipe" => TransitionEffect::Split {
            vertical: sub == "vertical",
            inward: reverse,
        },
        "slideWipe" if reverse => TransitionEffect::Uncover { dir: dir8(sub) },
        "slideWipe" => TransitionEffect::Cover { dir: dir8(sub) },
        "randomBarWipe" => TransitionEffect::RandomBars {
            vertical: sub == "vertical",
        },
        "ellipseWipe" => TransitionEffect::Shape {
            form: ShapeForm::Circle,
            inward: reverse,
        },
        "irisWipe" => TransitionEffect::Shape {
            form: if sub == "diamond" {
                ShapeForm::Diamond
            } else {
                ShapeForm::Circle
            },
            inward: reverse,
        },
        "fourBoxWipe" => TransitionEffect::Shape {
            form: ShapeForm::Plus,
            inward: reverse,
        },
        // Nearest relatives, as the model maps PowerPoint's effects it has no exact one for.
        "zoom" => TransitionEffect::Shape {
            form: ShapeForm::Circle,
            inward: !reverse,
        },
        "blindsWipe" | "checkerBoardWipe" | "waterfallWipe" => TransitionEffect::RandomBars {
            vertical: sub == "vertical",
        },
        "clockWipe" | "pinWheelWipe" | "fanWipe" | "doubleFanWipe" => {
            TransitionEffect::Wipe { dir: Side::Left }
        }
        "dissolve" => TransitionEffect::Fade {
            through_black: false,
        },
        _ => TransitionEffect::Fade {
            through_black: false,
        },
    };
    let mut t = Transition::new(effect);
    if let Some(d) = dur.filter(|d| *d > 0) {
        t.duration_ms = d;
    }
    t.advance_after_ms = advance;
    t.advance_on_click = !auto;
    Some(t)
}

/// Reads an opened package.
///
/// # Errors
///
/// [`OdfError`] when the package is not a readable ODF presentation.
pub fn read_package(pkg: &Package) -> Result<OdpRead, OdfError> {
    if pkg.kind != Some(OdfKind::Presentation) {
        return Err(OdfError::NotOdf(format!(
            "{} is not a presentation",
            pkg.media_type
        )));
    }
    let content = parse_part(
        "content.xml",
        pkg.file("content.xml")
            .ok_or_else(|| OdfError::Corrupt("content.xml is missing".into()))?,
    )?;
    let styles = match pkg.file("styles.xml") {
        Some(b) => Some(parse_part("styles.xml", b)?),
        None => None,
    };
    let ix = StyleIndex::new(styles.as_ref(), Some(&content));
    let mut fonts = FontFaces::default();
    for root in [styles.as_ref(), Some(&content)].into_iter().flatten() {
        fonts.add_decls(root);
    }
    let mut r = Reader {
        pkg,
        ix,
        fonts,
        styles_root: styles.as_ref(),
        pres: Presentation::empty(),
        media: HashMap::new(),
        warnings: Vec::new(),
    };
    r.read(&content)?;
    let mut pres = r.pres;
    pres.refresh_id_counter();
    pres.validate()
        .map_err(|e| OdfError::Model(e.to_string()))?;
    Ok(OdpRead {
        presentation: pres,
        properties: meta::read_meta(pkg.file("meta.xml"))?,
        warnings: r.warnings,
        template: pkg.media_type.ends_with("-template"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_find_the_unrotated_box() {
        // A 100 × 50 box rotated 90° clockwise about its centre (500, 300).
        let w = 100 * 360_000;
        let h = 50 * 360_000;
        // Counter-clockwise -90° (= clockwise 90°): the top-left corner lands at
        // centre + R·(-w/2, -h/2).
        let a: f64 = -std::f64::consts::FRAC_PI_2;
        let (s, c) = a.sin_cos();
        let (cx, cy) = (500.0 * 360_000.0, 300.0 * 360_000.0);
        let (wf, hf) = (f64::from(w), f64::from(h));
        let tx = cx - wf / 2.0 * c - hf / 2.0 * s;
        let ty = cy + wf / 2.0 * s - hf / 2.0 * c;
        let t = format!(
            "rotate ({a}) translate ({}mm {}mm)",
            tx / 36_000.0,
            ty / 36_000.0
        );
        let (x, y, rot) = transform_of(&t, i64::from(w), i64::from(h)).unwrap();
        assert_eq!(rot, 90 * 60_000);
        assert!((x - (500 - 50) * 360_000).abs() <= 2, "{x}");
        assert!((y - (300 - 25) * 360_000).abs() <= 2, "{y}");
    }

    #[test]
    fn durations_parse() {
        assert_eq!(duration_ms("0.75s"), Some(750));
        assert_eq!(duration_ms("PT3S"), Some(3000));
        assert_eq!(duration_ms("250ms"), Some(250));
        assert_eq!(duration_ms("x"), None);
    }
}
