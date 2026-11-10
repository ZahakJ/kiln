// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`kiln_deck::Presentation`] → `.odp`.
//!
//! Every layout becomes a master page (as LibreOffice does with PowerPoint files) holding the
//! master's decoration, the layout's placeholders and shapes, its background and the theme
//! colours (`loext:theme`); the master and layout text styles become the page's presentation
//! styles (`<page>-title`, `<page>-outline1` … `9`). Slides are `draw:page`s with their
//! background, visibility and transition; shapes are written with the position, fill,
//! outline, body properties and text they resolve to through the placeholder chain, so every
//! consumer shows the slide as Kiln does. Pictures and media go to `Pictures/` and `Media/`.

use std::collections::{BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::geom::Transform;
use kiln_deck::shape::{Geometry, PlaceholderType, Shape, ShapeKind};
use kiln_deck::slide::{Background, Layout, Master};
use kiln_deck::text::{Autofit, TextAnchor, TextBody, TextWrap};
use kiln_deck::transition::{Direction8, ShapeForm, Side, Transition, TransitionEffect};
use kiln_deck::{Owner, Presentation};
use kiln_ooxml::drawingml::color::ColorContext;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::theme::Theme;

use super::graphic::{GraphicDefs, effective, markers, write_fill, write_line};
use super::text::{Autos, ResolvedPara, TextWriter, flatten_run};
use crate::el::El;
use crate::package::{OdfKind, PackageWriter};
use crate::props::{para_props_el, run_props_el};
use crate::text::write::ncname;
use crate::units::emu_str;
use crate::xml::XmlOut;
use crate::{GENERATOR, OdfError, WriteOptions, meta};

const PREFIXES: &[&str] = &[
    "office",
    "style",
    "text",
    "table",
    "draw",
    "fo",
    "xlink",
    "dc",
    "meta",
    "number",
    "presentation",
    "svg",
    "chart",
    "dr3d",
    "math",
    "form",
    "script",
    "ooo",
    "ooow",
    "oooc",
    "smil",
    "anim",
    "loext",
    "officeooo",
    "drawooo",
];

/// PowerPoint's default table style, Medium Style 2 – Accent 1.
const DEFAULT_TABLE_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";

fn class_of(kind: PlaceholderType, in_notes: bool) -> &'static str {
    match kind {
        PlaceholderType::Title | PlaceholderType::CenteredTitle => "title",
        PlaceholderType::Subtitle => "subtitle",
        PlaceholderType::Body | PlaceholderType::Object if in_notes => "notes",
        PlaceholderType::Body | PlaceholderType::Object => "outline",
        PlaceholderType::Date => "date-time",
        PlaceholderType::SlideNumber => "page-number",
        PlaceholderType::Footer => "footer",
        PlaceholderType::Header => "header",
        PlaceholderType::Chart => "chart",
        PlaceholderType::Table => "table",
        PlaceholderType::ClipArt | PlaceholderType::Picture | PlaceholderType::Media => "graphic",
        PlaceholderType::Diagram => "orgchart",
        PlaceholderType::SlideImage => "page",
    }
}

/// The presentation style a placeholder class inherits from on a master page.
fn class_style(page: &str, class: &str) -> String {
    match class {
        "title" => format!("{page}-title"),
        "subtitle" => format!("{page}-subtitle"),
        "outline" => format!("{page}-outline1"),
        "notes" => format!("{page}-notes"),
        _ => format!("{page}-backgroundobjects"),
    }
}

struct Writer<'p> {
    pres: &'p Presentation,
    content: Autos,
    styles: Autos,
    defs: GraphicDefs,
    pictures: HashMap<String, String>,
    files: Vec<(String, String, Vec<u8>)>,
    common: Vec<El>,
    page_names: HashMap<NodeId, String>,
    warnings: Vec<String>,
}

/// What saving a presentation as `.odp` cannot keep (the Compatibility Checker's list).
#[must_use]
pub fn odp_issues(pres: &Presentation) -> Vec<String> {
    let mut w = Writer {
        pres,
        content: Autos::default(),
        styles: Autos::default(),
        defs: GraphicDefs::default(),
        pictures: HashMap::new(),
        files: Vec::new(),
        common: Vec::new(),
        page_names: HashMap::new(),
        warnings: Vec::new(),
    };
    let styles_body = w.master_styles();
    let _ = w.content();
    let _ = w.styles_xml(&styles_body);
    w.warnings
}

/// Writes a presentation as `.odp`.
///
/// # Errors
///
/// [`OdfError::Write`] when the archive cannot be written.
pub fn write_odp(pres: &Presentation, options: &WriteOptions) -> Result<Vec<u8>, OdfError> {
    let mut w = Writer {
        pres,
        content: Autos::default(),
        styles: Autos::default(),
        defs: GraphicDefs::default(),
        pictures: HashMap::new(),
        files: Vec::new(),
        common: Vec::new(),
        page_names: HashMap::new(),
        warnings: Vec::new(),
    };
    let styles_body = w.master_styles();
    let content = w.content();
    let styles = w.styles_xml(&styles_body);
    let mut pkg = PackageWriter::new(OdfKind::Presentation);
    pkg.set_template(options.template);
    pkg.add("content.xml", "text/xml", content);
    pkg.add("styles.xml", "text/xml", styles);
    let props = options.properties.clone().unwrap_or_default();
    pkg.add("meta.xml", "text/xml", meta::write_meta(&props, GENERATOR));
    pkg.add("settings.xml", "text/xml", settings_xml());
    for (path, media, bytes) in w.files {
        if path.ends_with('/') {
            pkg.add_entry(&path, &media);
        } else {
            pkg.add(&path, &media, bytes);
        }
    }
    pkg.finish()
}

fn settings_xml() -> Vec<u8> {
    let mut w = XmlOut::new();
    w.root("office:document-settings", &["office", "config"])
        .attr("office:version", "1.3");
    w.start("office:settings").end();
    w.finish()
}

/// The `draw:transform` of a rotated box: rotation about the top-left corner, then a move.
fn rotate_attr(x: &Transform) -> String {
    let theta = -(f64::from(x.rot) / 60_000.0).to_radians();
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (x.cx as f64, x.cy as f64);
    #[allow(clippy::cast_precision_loss)]
    let (cx, cy) = (x.x as f64 + w / 2.0, x.y as f64 + h / 2.0);
    let (s, c) = theta.sin_cos();
    let tx = cx - w / 2.0 * c - h / 2.0 * s;
    let ty = cy + w / 2.0 * s - h / 2.0 * c;
    #[allow(clippy::cast_possible_truncation)]
    let (tx, ty) = (tx.round() as i64, ty.round() as i64);
    format!(
        "rotate ({theta}) translate ({} {})",
        emu_str(tx),
        emu_str(ty)
    )
}

fn position(w: &mut XmlOut, x: &Transform) {
    w.attr("svg:width", emu_str(x.cx.max(0)))
        .attr("svg:height", emu_str(x.cy.max(0)));
    if x.rot == 0 {
        w.attr("svg:x", emu_str(x.x)).attr("svg:y", emu_str(x.y));
    } else {
        w.attr("draw:transform", rotate_attr(x));
    }
}

/// A child's transform with its group's rotation and flips applied about the group's centre.
fn rotate_about(group: &Transform, mut child: Transform) -> Transform {
    #[allow(clippy::cast_precision_loss)]
    let (gx, gy) = (
        group.x as f64 + group.cx as f64 / 2.0,
        group.y as f64 + group.cy as f64 / 2.0,
    );
    #[allow(clippy::cast_precision_loss)]
    let (mut cx, mut cy) = (
        child.x as f64 + child.cx as f64 / 2.0,
        child.y as f64 + child.cy as f64 / 2.0,
    );
    if group.flip_h {
        cx = 2.0 * gx - cx;
        child.flip_h = !child.flip_h;
    }
    if group.flip_v {
        cy = 2.0 * gy - cy;
        child.flip_v = !child.flip_v;
    }
    if group.rot != 0 {
        let a = (f64::from(group.rot) / 60_000.0).to_radians();
        let (s, c) = a.sin_cos();
        let (dx, dy) = (cx - gx, cy - gy);
        cx = gx + dx * c - dy * s;
        cy = gy + dx * s + dy * c;
        child.rot = (child.rot + group.rot).rem_euclid(21_600_000);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    {
        child.x = (cx - child.cx as f64 / 2.0).round() as i64;
        child.y = (cy - child.cy as f64 / 2.0).round() as i64;
    }
    child
}

/// Maps a group child's transform from the group's child space onto the page.
fn to_page(group: &Transform, cs: &kiln_deck::geom::ChildSpace, child: &Transform) -> Transform {
    #[allow(clippy::cast_precision_loss)]
    let sx = if cs.cx != 0 {
        group.cx as f64 / cs.cx as f64
    } else {
        1.0
    };
    #[allow(clippy::cast_precision_loss)]
    let sy = if cs.cy != 0 {
        group.cy as f64 / cs.cy as f64
    } else {
        1.0
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    Transform {
        x: group.x + ((child.x - cs.x) as f64 * sx).round() as i64,
        y: group.y + ((child.y - cs.y) as f64 * sy).round() as i64,
        cx: (child.cx as f64 * sx).round() as i64,
        cy: (child.cy as f64 * sy).round() as i64,
        ..*child
    }
}

impl<'p> Writer<'p> {
    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_owned());
        }
    }

    fn picture(&mut self, key: &str, dir: &str) -> Option<String> {
        if let Some(p) = self.pictures.get(key) {
            return Some(p.clone());
        }
        let part = self.pres.parts.get(key)?;
        let base = key.rsplit('/').next().unwrap_or(key);
        let safe: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let mut path = format!("{dir}/{safe}");
        let mut n = 1;
        while self.files.iter().any(|(p, _, _)| *p == path) {
            n += 1;
            path = format!("{dir}/{n}-{safe}");
        }
        self.files
            .push((path.clone(), part.content_type.clone(), part.bytes.clone()));
        self.pictures.insert(key.to_owned(), path.clone());
        Some(path)
    }

    fn ctx(&self, owner: Owner) -> ColorContext {
        self.pres.color_context(owner).unwrap_or_default()
    }

    fn theme(&self, owner: Owner) -> Theme {
        self.pres
            .theme_of(owner)
            .cloned()
            .unwrap_or_else(kiln_ooxml::drawingml::theme::office_theme)
    }

    /// Graphic properties of a shape (fill, outline, text box).
    fn graphic_props(&mut self, shape: &Shape, owner: Owner) -> El {
        let ctx = self.ctx(owner);
        let theme = self.theme(owner);
        let props = self
            .pres
            .resolved_props(shape.id)
            .or_else(|| shape.props().cloned())
            .unwrap_or_default();
        let (fill, line, _) = effective(&props, &theme, &ctx);
        let mut gp = El::new("style:graphic-properties");
        let mut defs = std::mem::take(&mut self.defs);
        {
            let mut image = |k: &str| self.picture(k, "Pictures");
            match &fill {
                Some(f) => write_fill(&mut gp, Some(f), &ctx, &mut defs, &mut image),
                None => {
                    // Shapes without a fill of their own are not filled; pictures and text
                    // boxes neither.
                    gp.set("draw:fill", "none");
                }
            }
        }
        match &line {
            Some(l) => write_line(&mut gp, Some(l), &ctx, &mut defs),
            None => gp.set("draw:stroke", "none"),
        }
        self.defs = defs;
        if let Some(body) = self.pres.resolved_body(shape.id) {
            let names = ["left", "top", "right", "bottom"];
            for (i, n) in names.iter().enumerate() {
                gp.set(&format!("fo:padding-{n}"), emu_str(body.insets[i]));
            }
            gp.set(
                "draw:textarea-vertical-align",
                match body.anchor {
                    TextAnchor::Middle => "middle",
                    TextAnchor::Bottom => "bottom",
                    TextAnchor::Justified | TextAnchor::Distributed => "justify",
                    TextAnchor::Top => "top",
                },
            );
            match body.autofit {
                Autofit::ResizeShape => gp.set("draw:auto-grow-height", "true"),
                Autofit::Shrink { .. } => {
                    gp.set("draw:auto-grow-height", "false");
                    gp.set("draw:fit-to-size", "shrink-to-fit");
                    gp.set("style:shrink-to-fit", "true");
                }
                Autofit::Off => gp.set("draw:auto-grow-height", "false"),
            }
            gp.set("draw:auto-grow-width", "false");
            match body.direction {
                kiln_deck::text::TextDirection::Rotate270 => {
                    gp.set("style:writing-mode", "lr-tb");
                    gp.set("loext:writing-mode", "bt-lr");
                }
                kiln_deck::text::TextDirection::Horizontal => {}
                _ => gp.set("style:writing-mode", "tb-rl"),
            }
            gp.set(
                "fo:wrap-option",
                if body.wrap == TextWrap::None {
                    "no-wrap"
                } else {
                    "wrap"
                },
            );
        }
        let (effects, ectx) = super::graphic::effective_effects(&props, &theme, &ctx);
        super::graphic::write_effects(&mut gp, effects.as_ref(), &ectx);
        gp
    }

    fn text_body(
        &mut self,
        w: &mut XmlOut,
        shape: &Shape,
        body: &TextBody,
        owner: Owner,
        master_part: bool,
    ) {
        let ctx = self.ctx(owner);
        let theme = self.theme(owner);
        let pres = self.pres;
        let id = shape.id;
        let resolve = |p: &kiln_doc::model::Paragraph| -> ResolvedPara {
            match pres.resolved_paragraph(id, body, p) {
                Some(r) => ResolvedPara {
                    para: r.para,
                    deck: r.deck,
                    run: r.run,
                },
                None => ResolvedPara {
                    para: p.props.clone(),
                    deck: body.deck_props(p.id).cloned().unwrap_or_default(),
                    run: RunPropsDefault::run(),
                },
            }
        };
        let mut autos = if master_part {
            std::mem::take(&mut self.styles)
        } else {
            std::mem::take(&mut self.content)
        };
        let mut pictures = std::mem::take(&mut self.pictures);
        let mut files = std::mem::take(&mut self.files);
        {
            let parts = &pres.parts;
            let mut image = |k: &str| -> Option<String> {
                if let Some(p) = pictures.get(k) {
                    return Some(p.clone());
                }
                let part = parts.get(k)?;
                let base = k.rsplit('/').next().unwrap_or(k);
                let path = format!("Pictures/bullet-{}-{base}", files.len());
                files.push((path.clone(), part.content_type.clone(), part.bytes.clone()));
                pictures.insert(k.to_owned(), path.clone());
                Some(path)
            };
            let mut tw = TextWriter {
                ctx: &ctx,
                theme: &theme,
                autos: &mut autos,
                image: &mut image,
            };
            tw.write(w, body, &resolve);
        }
        self.pictures = pictures;
        self.files = files;
        if master_part {
            self.styles = autos;
        } else {
            self.content = autos;
        }
    }

    fn add_style(&mut self, master_part: bool, prefix: &str, el: El) -> String {
        if master_part {
            self.styles.add(&format!("M{prefix}"), el)
        } else {
            self.content.add(prefix, el)
        }
    }

    /// Writes one shape (`page` = the master page the owner's text styles come from).
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn shape(
        &mut self,
        w: &mut XmlOut,
        shape: &Shape,
        owner: Owner,
        page: &str,
        master_part: bool,
        xfrm_override: Option<Transform>,
        in_notes: bool,
    ) {
        let resolved = xfrm_override
            .or(shape.xfrm)
            .or_else(|| self.pres.resolved_xfrm(shape.id));
        let Some(x) = resolved else {
            return;
        };
        let layer = if master_part {
            "backgroundobjects"
        } else {
            "layout"
        };
        let gp = self.graphic_props(shape, owner);
        match &shape.kind {
            ShapeKind::Geometry(a) => {
                let preset = a
                    .props
                    .geometry
                    .as_ref()
                    .and_then(Geometry::preset_name)
                    .unwrap_or("rect")
                    .to_owned();
                if let Some(ph) = shape.placeholder {
                    let class = class_of(ph.kind, in_notes);
                    let mut st = El::new("style:style")
                        .with("style:family", "presentation")
                        .with("style:parent-style-name", class_style(page, class));
                    st.children.push(gp);
                    let name = self.add_style(master_part, "pr", st);
                    w.start("draw:frame")
                        .attr("draw:name", &shape.name)
                        .attr("presentation:style-name", name)
                        .attr("draw:layer", layer);
                    position(w, &x);
                    w.attr("presentation:class", class);
                    if shape.xfrm.is_some() {
                        w.attr("presentation:user-transformed", "true");
                    }
                    let empty = a.text.as_ref().is_none_or(TextBody::is_empty);
                    if empty && !master_part {
                        w.attr("presentation:placeholder", "true");
                    }
                    w.start("draw:text-box");
                    if let Some(t) = &a.text
                        && !empty
                    {
                        self.text_body(w, shape, t, owner, master_part);
                    } else if master_part {
                        match class {
                            "date-time" => {
                                w.start("text:p")
                                    .start("presentation:date-time")
                                    .end()
                                    .end();
                            }
                            "footer" => {
                                w.start("text:p").start("presentation:footer").end().end();
                            }
                            "page-number" => {
                                w.start("text:p")
                                    .start("text:page-number")
                                    .text("<number>")
                                    .end()
                                    .end();
                            }
                            _ => {}
                        }
                    }
                    w.end();
                    alt(w, shape);
                    w.end();
                    return;
                }
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                let frame = a.text_box && preset == "rect" && x.rot == 0 && !x.flip_h && !x.flip_v;
                if frame {
                    w.start("draw:frame")
                        .attr("draw:name", &shape.name)
                        .attr("draw:style-name", name)
                        .attr("draw:layer", layer);
                    position(w, &x);
                    w.start("draw:text-box");
                    if let Some(t) = &a.text {
                        self.text_body(w, shape, t, owner, master_part);
                    }
                    w.end();
                    alt(w, shape);
                    w.end();
                } else {
                    w.start("draw:custom-shape")
                        .attr("draw:name", &shape.name)
                        .attr("draw:style-name", name)
                        .attr("draw:layer", layer);
                    position(w, &x);
                    alt(w, shape);
                    if let Some(t) = &a.text
                        && !t.is_empty()
                    {
                        self.text_body(w, shape, t, owner, master_part);
                    }
                    let adjust: Vec<(String, i64)> = match &a.props.geometry {
                        Some(Geometry::Preset { adjust, .. }) => adjust.clone(),
                        _ => Vec::new(),
                    };
                    if matches!(a.props.geometry, Some(Geometry::Custom(_))) {
                        self.warn("custom shape outlines are saved as rectangles in ODF");
                    }
                    enhanced(w, &preset, &adjust, &x);
                    w.end();
                }
            }
            ShapeKind::Picture(p) => {
                let Some(path) = self.picture(&p.media, "Pictures") else {
                    self.warn("a picture whose image data is missing was not saved");
                    return;
                };
                let mut gp = gp;
                gp.set("draw:fill", "none");
                if p.crop != kiln_deck::shape::Crop::default() {
                    match self.pres.parts.get(&p.media).and_then(|m| super::graphic::image_size(&m.bytes)) {
                        Some(px) => gp.set("fo:clip", super::graphic::clip_attr(&p.crop, px)),
                        None => self.warn("cropping of pictures in formats other than PNG, JPEG, GIF and BMP is not saved in ODF"),
                    }
                }
                if x.flip_h {
                    gp.set("style:mirror", "horizontal");
                }
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                w.start("draw:frame")
                    .attr("draw:name", &shape.name)
                    .attr("draw:style-name", name)
                    .attr("draw:layer", layer);
                position(w, &x);
                if let Some(ph) = shape.placeholder {
                    w.attr("presentation:class", class_of(ph.kind, in_notes));
                    if shape.xfrm.is_some() {
                        w.attr("presentation:user-transformed", "true");
                    }
                }
                w.start("draw:image")
                    .attr("xlink:href", path)
                    .attr("xlink:type", "simple")
                    .attr("xlink:show", "embed")
                    .attr("xlink:actuate", "onLoad")
                    .end();
                alt(w, shape);
                w.end();
            }
            ShapeKind::Table(t) => {
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                w.start("draw:frame")
                    .attr("draw:name", &shape.name)
                    .attr("draw:style-name", name)
                    .attr("draw:layer", layer);
                position(w, &x);
                self.table(w, shape, t, owner, master_part);
                alt(w, shape);
                w.end();
            }
            ShapeKind::Group(g) => {
                w.start("draw:g").attr("draw:name", &shape.name);
                for c in &g.children {
                    // ODF groups cannot rotate: their rotation goes to each child, about the
                    // group's centre.
                    let cx = c
                        .xfrm
                        .map(|cx| rotate_about(&x, to_page(&x, &g.child_space, &cx)));
                    self.shape(w, c, owner, page, master_part, cx, in_notes);
                }
                w.end();
            }
            ShapeKind::Connector(_) => {
                // Lines show no fill: one is written only when the model states it.
                let mut gp = gp;
                if shape.props().is_none_or(|p| p.fill.is_none()) {
                    gp.attrs.retain(|(k, _)| !k.starts_with("draw:fill"));
                }
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                let (mut x1, mut y1, mut x2, mut y2) = (x.x, x.y, x.x + x.cx, x.y + x.cy);
                if x.flip_h {
                    std::mem::swap(&mut x1, &mut x2);
                }
                if x.flip_v {
                    std::mem::swap(&mut y1, &mut y2);
                }
                let preset = shape
                    .props()
                    .and_then(|p| p.geometry.as_ref())
                    .and_then(Geometry::preset_name)
                    .unwrap_or("line");
                let (element, ty) = if preset.starts_with("bent") {
                    ("draw:connector", Some("standard"))
                } else if preset.starts_with("curved") {
                    ("draw:connector", Some("curve"))
                } else if preset.contains("Connector") {
                    ("draw:connector", Some("line"))
                } else {
                    ("draw:line", None)
                };
                w.start(element);
                if let Some(t) = ty {
                    w.attr("draw:type", t);
                }
                w.attr("draw:name", &shape.name)
                    .attr("draw:style-name", name)
                    .attr("draw:layer", layer)
                    .attr("svg:x1", emu_str(x1))
                    .attr("svg:y1", emu_str(y1))
                    .attr("svg:x2", emu_str(x2))
                    .attr("svg:y2", emu_str(y2));
                alt(w, shape);
                w.end();
            }
            ShapeKind::Media(m) => {
                let href = match &m.source {
                    kiln_deck::shape::MediaSource::Embedded(k) => self.picture(k, "Media"),
                    kiln_deck::shape::MediaSource::Linked(u) => Some(u.clone()),
                };
                let Some(href) = href else {
                    self.warn("a video or sound whose data is missing was not saved");
                    return;
                };
                let mime = match &m.source {
                    kiln_deck::shape::MediaSource::Embedded(k) => self
                        .pres
                        .parts
                        .get(k)
                        .map(|p| p.content_type.clone())
                        .unwrap_or_default(),
                    kiln_deck::shape::MediaSource::Linked(_) => String::new(),
                };
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                w.start("draw:frame")
                    .attr("draw:name", &shape.name)
                    .attr("draw:style-name", name)
                    .attr("draw:layer", layer);
                position(w, &x);
                w.start("draw:plugin")
                    .attr("xlink:href", href)
                    .attr("xlink:type", "simple")
                    .attr("xlink:show", "embed")
                    .attr("xlink:actuate", "onLoad")
                    .attr("draw:mime-type", mime)
                    .end();
                alt(w, shape);
                w.end();
            }
            ShapeKind::Chart(c) => {
                let Some(cs) = self
                    .pres
                    .parts
                    .get(&c.part)
                    .and_then(|p| crate::chart::from_drawingml(&p.bytes))
                    .filter(crate::chart::supported)
                else {
                    self.warn("a chart of a type ODF has no class for was not saved");
                    return;
                };
                let theme = self.theme(owner);
                let colors = kiln_charts::palette::ChartColors::new(
                    Some(&theme),
                    kiln_charts::palette::ColorMode::Document,
                );
                let n = (1u32..)
                    .find(|n| {
                        let p = format!("Object {n}/");
                        !self.files.iter().any(|(f, _, _)| f.starts_with(&p))
                    })
                    .unwrap_or(1);
                let dir = format!("Object {n}");
                self.files.push((
                    format!("{dir}/content.xml"),
                    "text/xml".to_owned(),
                    crate::chart::write_object(
                        &cs,
                        &colors,
                        (x.cx, x.cy),
                        crate::chart::DataMode::Local,
                    ),
                ));
                self.files.push((
                    format!("{dir}/styles.xml"),
                    "text/xml".to_owned(),
                    crate::chart::object_styles(),
                ));
                self.files.push((
                    format!("{dir}/"),
                    crate::chart::CHART_MEDIA_TYPE.to_owned(),
                    Vec::new(),
                ));
                let mut st = El::new("style:style").with("style:family", "graphic");
                st.children.push(gp);
                let name = self.add_style(master_part, "gr", st);
                w.start("draw:frame")
                    .attr("draw:name", &shape.name)
                    .attr("draw:style-name", name)
                    .attr("draw:layer", layer);
                position(w, &x);
                w.start("draw:object")
                    .attr("xlink:href", format!("./{dir}"))
                    .attr("xlink:type", "simple")
                    .attr("xlink:show", "embed")
                    .attr("xlink:actuate", "onLoad")
                    .end();
                alt(w, shape);
                w.end();
            }
        }
    }

    fn table(
        &mut self,
        w: &mut XmlOut,
        shape: &Shape,
        t: &kiln_deck::shape::Table,
        owner: Owner,
        master_part: bool,
    ) {
        let ctx = self.ctx(owner);
        // PowerPoint's default table style (Medium Style 2 – Accent 1) is written out as the
        // cell fills, borders and header text it gives: ODF has no table styles.
        let styled = t.style_id.as_deref() == Some(DEFAULT_TABLE_STYLE);
        let accent = kiln_ooxml::drawingml::color::resolve_color(
            &kiln_ooxml::drawingml::color::DrawingColor::scheme("accent1"),
            &ctx,
        )
        .to_color();
        let tint = |k: f64| {
            let m = |c: u8| (f64::from(c) + (255.0 - f64::from(c)) * (1.0 - k)).round() as u8;
            format!("#{:02x}{:02x}{:02x}", m(accent.r), m(accent.g), m(accent.b))
        };
        let rows_n = t.rows.len();
        w.start("table:table");
        for g in &t.grid {
            let st = El::new("style:style")
                .with("style:family", "table-column")
                .with_child(
                    El::new("style:table-column-properties")
                        .with("style:column-width", emu_str(*g)),
                );
            let n = self.add_style(master_part, "co", st);
            w.start("table:table-column")
                .attr("table:style-name", n)
                .end();
        }
        for (ri, row) in t.rows.iter().enumerate() {
            let header = styled && t.look.first_row && ri == 0;
            let footer = styled && t.look.last_row && ri + 1 == rows_n && rows_n > 1;
            let data = ri.saturating_sub(usize::from(styled && t.look.first_row));
            let band = styled && t.look.band_row && !header && !footer && data % 2 == 0;
            let st = El::new("style:style")
                .with("style:family", "table-row")
                .with_child(
                    El::new("style:table-row-properties")
                        .with("style:min-row-height", emu_str(row.height)),
                );
            let n = self.add_style(master_part, "ro", st);
            w.start("table:table-row").attr("table:style-name", n);
            for cell in &row.cells {
                if cell.h_merge || cell.v_merge {
                    w.start("table:covered-table-cell").end();
                    continue;
                }
                let mut gp = El::new("style:graphic-properties");
                let mut defs = std::mem::take(&mut self.defs);
                {
                    let mut image = |k: &str| self.picture(k, "Pictures");
                    match &cell.props.fill {
                        Some(f) => write_fill(&mut gp, Some(f), &ctx, &mut defs, &mut image),
                        None if styled => {
                            gp.set("draw:fill", "solid");
                            gp.set(
                                "draw:fill-color",
                                if header || footer {
                                    tint(1.0)
                                } else if band {
                                    tint(0.4)
                                } else {
                                    tint(0.2)
                                },
                            );
                        }
                        None => gp.set("draw:fill", "none"),
                    }
                }
                self.defs = defs;
                let mut tc = El::new("style:table-cell-properties");
                for (i, side) in ["left", "right", "top", "bottom"].iter().enumerate() {
                    if cell.props.borders[i].0.is_none() && styled {
                        tc.set(&format!("fo:border-{side}"), "1pt solid #ffffff");
                    }
                    if let Some(l) = &cell.props.borders[i].0 {
                        let width = l.width.unwrap_or(12_700);
                        let border = match &l.fill {
                            Some(Fill::Solid(c)) => format!(
                                "{} solid {}",
                                emu_str(width),
                                super::graphic::css(c, &ctx).0
                            ),
                            _ => "none".to_owned(),
                        };
                        tc.set(&format!("fo:border-{side}"), border);
                    }
                    // DrawingML's default cell margins: 0.1 in left and right, 0.05 in top and
                    // bottom.
                    let default = if i < 2 { 91_440 } else { 45_720 };
                    tc.set(
                        &format!("fo:padding-{side}"),
                        emu_str(cell.props.margins[i].unwrap_or(default)),
                    );
                }
                if let Some(a) = cell.props.anchor {
                    tc.set(
                        "style:vertical-align",
                        match a {
                            TextAnchor::Middle => "middle",
                            TextAnchor::Bottom => "bottom",
                            _ => "top",
                        },
                    );
                }
                let mut st = El::new("style:style").with("style:family", "table-cell");
                st.push_nonempty(gp);
                st.push_nonempty(tc);
                let n = self.add_style(master_part, "ce", st);
                w.start("table:table-cell").attr("table:style-name", n);
                if cell.grid_span > 1 {
                    w.attr("table:number-columns-spanned", cell.grid_span.to_string());
                }
                if cell.row_span > 1 {
                    w.attr("table:number-rows-spanned", cell.row_span.to_string());
                }
                if cell.text.is_empty() {
                    w.start("text:p").end();
                } else {
                    // Cell text resolves like the table shape's other text.
                    let emphasis = (header || footer).then(|| kiln_doc::props::RunProps {
                        bold: Some(true),
                        color: Some(kiln_doc::props::DocColor {
                            val: kiln_doc::props::ColorValue::Rgb(kiln_common::Color::rgb(
                                255, 255, 255,
                            )),
                            theme_color: None,
                            theme_tint: None,
                            theme_shade: None,
                        }),
                        ..kiln_doc::props::RunProps::default()
                    });
                    self.cell_text(w, shape, &cell.text, owner, master_part, emphasis.as_ref());
                }
                w.end();
            }
            w.end();
        }
        w.end();
    }

    fn cell_text(
        &mut self,
        w: &mut XmlOut,
        shape: &Shape,
        body: &TextBody,
        owner: Owner,
        master_part: bool,
        emphasis: Option<&kiln_doc::props::RunProps>,
    ) {
        let ctx = self.ctx(owner);
        let theme = self.theme(owner);
        let mut base = self
            .pres
            .level_style(shape.id, 0)
            .map(|l| l.run)
            .unwrap_or_default();
        if let Some(e) = emphasis {
            base = kiln_doc::merge::merge_run_props(&base, Some(e));
        }
        let resolve = |p: &kiln_doc::model::Paragraph| ResolvedPara {
            para: p.props.clone(),
            deck: body.deck_props(p.id).cloned().unwrap_or_default(),
            run: base.clone(),
        };
        let mut autos = if master_part {
            std::mem::take(&mut self.styles)
        } else {
            std::mem::take(&mut self.content)
        };
        {
            let mut image = |_: &str| -> Option<String> { None };
            let mut tw = TextWriter {
                ctx: &ctx,
                theme: &theme,
                autos: &mut autos,
                image: &mut image,
            };
            tw.write(w, body, &resolve);
        }
        if master_part {
            self.styles = autos;
        } else {
            self.content = autos;
        }
    }

    fn background_style(
        &mut self,
        bg: Option<&Background>,
        owner: Owner,
        master_part: bool,
        extra: &[(&str, String)],
    ) -> String {
        let ctx = self.ctx(owner);
        let mut dp = El::new("style:drawing-page-properties");
        match bg {
            Some(Background::Fill { fill, .. }) => {
                let mut defs = std::mem::take(&mut self.defs);
                {
                    let mut image = |k: &str| self.picture(k, "Pictures");
                    write_fill(&mut dp, Some(fill), &ctx, &mut defs, &mut image);
                }
                self.defs = defs;
            }
            Some(Background::Theme { idx, color }) => {
                let theme = self.theme(owner);
                let mut c = ctx;
                c.ph_clr = color
                    .as_ref()
                    .map(|d| kiln_ooxml::drawingml::color::resolve_color(d, &ctx));
                let fill = theme
                    .format_scheme
                    .bg_fill_styles
                    .get(idx.saturating_sub(1001) as usize)
                    .cloned()
                    .unwrap_or(Fill::None);
                let flat = super::graphic::flatten_fill(&fill, &c);
                let mut defs = std::mem::take(&mut self.defs);
                {
                    let mut image = |k: &str| self.picture(k, "Pictures");
                    write_fill(&mut dp, Some(&flat), &c, &mut defs, &mut image);
                }
                self.defs = defs;
            }
            None => {}
        }
        if !master_part && bg.is_some() {
            dp.set("presentation:background-visible", "true");
        }
        for (k, v) in extra {
            dp.set(k, v.clone());
        }
        let st = El::new("style:style")
            .with("style:family", "drawing-page")
            .with_child(dp);
        self.add_style(master_part, "dp", st)
    }

    /// The master pages (one per layout) and the presentation styles they need.
    fn master_styles(&mut self) -> String {
        let mut out = XmlOut::fragment();
        let mut used: BTreeSet<String> = BTreeSet::new();
        let masters: Vec<&Master> = self.pres.masters.iter().collect();
        for m in &masters {
            for l in &m.layouts {
                let mut name = ncname(&l.name);
                let mut n = 2;
                while used.contains(&name) {
                    name = format!("{}{n}", ncname(&l.name));
                    n += 1;
                }
                used.insert(name.clone());
                self.page_names.insert(l.id, name.clone());
                self.page_presentation_styles(m, l, &name);
                let bg = l.background.as_ref().or(Some(&m.background));
                let dp = self.background_style(bg, Owner::Layout(l.id), true, &[]);
                out.start("style:master-page")
                    .attr("style:name", &name)
                    .attr("style:display-name", &l.name)
                    .attr("style:page-layout-name", "PM1")
                    .attr("draw:style-name", dp);
                theme_el(&m.theme).write(&mut out);
                let owner_m = Owner::Master(m.id);
                let owner_l = Owner::Layout(l.id);
                if l.show_master_shapes {
                    for s in &m.shapes {
                        // The layout's own placeholders replace the master's.
                        if let Some(ph) = s.placeholder {
                            let has = l.shapes.iter().any(|x| {
                                x.placeholder.is_some_and(|p| {
                                    class_of(p.kind, false) == class_of(ph.kind, false)
                                })
                            });
                            if has
                                || !matches!(
                                    ph.kind,
                                    PlaceholderType::Date
                                        | PlaceholderType::Footer
                                        | PlaceholderType::SlideNumber
                                )
                            {
                                continue;
                            }
                        }
                        self.shape(&mut out, s, owner_m, &name, true, None, false);
                    }
                }
                for s in &l.shapes {
                    self.shape(&mut out, s, owner_l, &name, true, None, false);
                }
                out.end();
            }
        }
        out.into_string()
    }

    /// `<page>-title`, `-subtitle`, `-outline1` … `9`, `-notes`, `-backgroundobjects`.
    fn page_presentation_styles(&mut self, m: &Master, l: &Layout, page: &str) {
        let owner = Owner::Layout(l.id);
        let ctx = self.ctx(owner);
        let theme = m.theme.clone();
        let doc_theme = kiln_doc::theme::DocTheme::default();
        let find = |k: &[PlaceholderType]| -> Option<NodeId> {
            l.shapes
                .iter()
                .chain(&m.shapes)
                .find(|s| s.placeholder.is_some_and(|p| k.contains(&p.kind)))
                .map(|s| s.id)
        };
        let title = find(&[PlaceholderType::Title, PlaceholderType::CenteredTitle]);
        let body = find(&[PlaceholderType::Body, PlaceholderType::Object]);
        let sub = find(&[PlaceholderType::Subtitle]);
        let make =
            |suffix: &str, shape: Option<NodeId>, level: usize, kind: kiln_deck::TextStyleKind| {
                let lvl = match shape.and_then(|s| self.pres.level_style(s, level)) {
                    Some(l) => l,
                    None => {
                        let ms = match kind {
                            kiln_deck::TextStyleKind::Title => &m.title_style,
                            kiln_deck::TextStyleKind::Body => &m.body_style,
                            kiln_deck::TextStyleKind::Other => &m.other_style,
                        };
                        ms.level(level)
                            .over(&self.pres.default_text_style.level(level))
                    }
                };
                let run = flatten_run(&lvl.run, &ctx, &theme);
                let mut para = lvl.para.clone();
                para.outline_level = None;
                para.indent = None;
                let mut st = El::new("style:style")
                    .with("style:name", format!("{page}-{suffix}"))
                    .with("style:display-name", format!("{}-{suffix}", l.name))
                    .with("style:family", "presentation");
                st.children.push(
                    El::new("style:graphic-properties")
                        .with("draw:stroke", "none")
                        .with("draw:fill", "none"),
                );
                st.push_nonempty(para_props_el(&para, &doc_theme));
                st.push_nonempty(run_props_el(&run, &doc_theme));
                st
            };
        let mut els = vec![
            make("title", title, 0, kiln_deck::TextStyleKind::Title),
            make("subtitle", sub.or(body), 0, kiln_deck::TextStyleKind::Body),
        ];
        for n in 1..=9 {
            els.push(make(
                &format!("outline{n}"),
                body,
                n - 1,
                kiln_deck::TextStyleKind::Body,
            ));
        }
        els.push(make("notes", None, 0, kiln_deck::TextStyleKind::Other));
        els.push(make(
            "backgroundobjects",
            None,
            0,
            kiln_deck::TextStyleKind::Other,
        ));
        // Outline levels inherit from the level above in ODF.
        for (i, el) in els.iter_mut().enumerate().take(12).skip(3) {
            el.set(
                "style:parent-style-name",
                format!("{page}-outline{}", i - 2),
            );
        }
        self.common.extend(els);
    }

    #[allow(clippy::too_many_lines)]
    fn content(&mut self) -> Vec<u8> {
        let mut body = XmlOut::fragment();
        body.start("office:body").start("office:presentation");
        let slides: Vec<&kiln_deck::Slide> = self.pres.slides.iter().collect();
        let mut names: BTreeSet<String> = BTreeSet::new();
        for (i, s) in slides.iter().enumerate() {
            let owner = Owner::Slide(s.id);
            let page = self
                .page_names
                .get(&s.layout)
                .cloned()
                .unwrap_or_else(|| "Default".to_owned());
            let mut extra: Vec<(&str, String)> = Vec::new();
            // Footer-area placeholders show where the slide has them, as in PowerPoint.
            let has = |k: PlaceholderType| {
                s.shapes
                    .iter()
                    .any(|x| x.placeholder.is_some_and(|p| p.kind == k))
            };
            extra.push(("presentation:display-header", "false".to_owned()));
            extra.push((
                "presentation:display-footer",
                has(PlaceholderType::Footer).to_string(),
            ));
            extra.push((
                "presentation:display-page-number",
                has(PlaceholderType::SlideNumber).to_string(),
            ));
            extra.push((
                "presentation:display-date-time",
                has(PlaceholderType::Date).to_string(),
            ));
            if s.hidden {
                extra.push(("presentation:visibility", "hidden".to_owned()));
            }
            let trans = s.transition.as_ref().and_then(transition_attrs);
            if let Some((ty, sub, rev, _)) = &trans {
                extra.push(("smil:type", (*ty).to_owned()));
                if !sub.is_empty() {
                    extra.push(("smil:subtype", (*sub).to_owned()));
                }
                if *rev {
                    extra.push(("smil:direction", "reverse".to_owned()));
                }
                if sub.starts_with("fadeOver") || sub.starts_with("fadeFrom") {
                    extra.push((
                        "smil:fadeColor",
                        if *sub == "fadeFromColor" {
                            "#ffffff"
                        } else {
                            "#000000"
                        }
                        .to_owned(),
                    ));
                }
            }
            if let Some(t) = &s.transition
                && let Some(ms) = t.advance_after_ms
            {
                extra.push(("presentation:transition-type", "automatic".to_owned()));
                extra.push((
                    "presentation:duration",
                    crate::units::duration_str(f64::from(ms) / 86_400_000.0),
                ));
            }
            let dp = self.background_style(s.background.as_ref(), owner, false, &extra);
            let mut name = s
                .name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("page{}", i + 1));
            if names.contains(&name) {
                name = format!("{name} ({})", i + 1);
            }
            names.insert(name.clone());
            body.start("draw:page")
                .attr("draw:name", &name)
                .attr("draw:style-name", dp)
                .attr("draw:master-page-name", &page);
            for sh in &s.shapes {
                self.shape(&mut body, sh, owner, &page, false, None, false);
            }
            // Comments as LibreOffice's annotations; replies follow as annotations of their own.
            for c in &s.comments {
                let mut items = vec![(
                    c.author.as_str(),
                    c.initials.as_str(),
                    c.created.as_str(),
                    c.text.as_str(),
                )];
                items.extend(
                    c.replies
                        .iter()
                        .map(|r| (r.author.as_str(), "", r.created.as_str(), r.text.as_str())),
                );
                for (author, initials, created, text) in items {
                    body.start("officeooo:annotation")
                        .attr("svg:x", emu_str(c.pos.0.max(0)))
                        .attr("svg:y", emu_str(c.pos.1.max(0)))
                        .attr("svg:width", "0.6cm")
                        .attr("svg:height", "0.6cm");
                    body.text_element("dc:creator", author);
                    if !created.is_empty() {
                        body.text_element("dc:date", created);
                    }
                    if !initials.is_empty() {
                        body.text_element("meta:creator-initials", initials);
                    }
                    for line in text.split('\n') {
                        body.text_element("text:p", line);
                    }
                    body.end();
                }
                if c.resolved {
                    self.warn("whether a comment is resolved is not saved in ODF");
                }
            }
            if !s.timeline.sequence.is_empty() || !s.timeline.triggers.is_empty() {
                self.warn("slide animations are not saved in ODF");
            }
            if let Some((ty, sub, rev, dur)) = trans {
                body.start("anim:par")
                    .attr("presentation:node-type", "timing-root");
                body.start("anim:seq")
                    .attr("presentation:node-type", "main-sequence")
                    .end();
                body.start("anim:transitionFilter")
                    .attr("smil:begin", "0s")
                    .attr(
                        "smil:dur",
                        format!("{}s", crate::units::number_str(f64::from(dur) / 1000.0)),
                    )
                    .attr("smil:type", ty);
                if !sub.is_empty() {
                    body.attr("smil:subtype", sub);
                }
                if rev {
                    body.attr("smil:direction", "reverse");
                }
                body.end().end();
            }
            if let Some(n) = &s.notes {
                body.start("presentation:notes");
                body.start("draw:page-thumbnail")
                    .attr("draw:layer", "layout")
                    .attr("svg:width", "14cm")
                    .attr("svg:height", "7.9cm")
                    .attr("svg:x", "3.5cm")
                    .attr("svg:y", "2.5cm")
                    .attr("draw:page-number", (i + 1).to_string())
                    .attr("presentation:class", "page")
                    .end();
                let st = El::new("style:style")
                    .with("style:family", "presentation")
                    .with("style:parent-style-name", format!("{page}-notes"));
                let pr = self.content.add("pr", st);
                body.start("draw:frame")
                    .attr("presentation:style-name", pr)
                    .attr("draw:layer", "layout")
                    .attr("svg:width", "16cm")
                    .attr("svg:height", "12cm")
                    .attr("svg:x", "2.5cm")
                    .attr("svg:y", "12.5cm")
                    .attr("presentation:class", "notes");
                body.start("draw:text-box");
                self.notes_text(&mut body, &n.text, owner);
                body.end().end();
                body.end();
            }
            body.end();
        }
        body.end().end();
        let mut w = XmlOut::new();
        w.root("office:document-content", PREFIXES)
            .attr("office:version", "1.3");
        w.start("office:scripts").end();
        font_decls(
            &mut w,
            &[&self.content.list, &self.styles.list, &self.common],
        );
        w.start("office:automatic-styles");
        for e in &self.content.list {
            e.write(&mut w);
        }
        w.end();
        w.raw(&body.into_string());
        w.finish()
    }

    fn notes_text(&mut self, w: &mut XmlOut, body: &TextBody, owner: Owner) {
        let ctx = self.ctx(owner);
        let theme = self.theme(owner);
        let base = kiln_doc::props::RunProps {
            size: Some(24),
            ..kiln_doc::props::RunProps::default()
        };
        let resolve = |p: &kiln_doc::model::Paragraph| ResolvedPara {
            para: p.props.clone(),
            deck: body.deck_props(p.id).cloned().unwrap_or_default(),
            run: base.clone(),
        };
        let mut autos = std::mem::take(&mut self.content);
        {
            let mut image = |_: &str| -> Option<String> { None };
            let mut tw = TextWriter {
                ctx: &ctx,
                theme: &theme,
                autos: &mut autos,
                image: &mut image,
            };
            tw.write(w, body, &resolve);
        }
        self.content = autos;
    }

    fn styles_xml(&mut self, masters: &str) -> Vec<u8> {
        let mut w = XmlOut::new();
        w.root("office:document-styles", PREFIXES)
            .attr("office:version", "1.3");
        font_decls(
            &mut w,
            &[&self.content.list, &self.styles.list, &self.common],
        );
        w.start("office:styles");
        for d in &self.defs.list {
            d.write(&mut w);
        }
        for m in markers() {
            m.write(&mut w);
        }
        El::new("style:default-style")
            .with("style:family", "graphic")
            .with_child(
                El::new("style:graphic-properties")
                    .with("draw:fill", "none")
                    .with("draw:stroke", "none"),
            )
            .write(&mut w);
        El::new("style:style")
            .with("style:name", "standard")
            .with("style:family", "graphic")
            .with_child(El::new("style:graphic-properties").with("draw:shadow", "hidden"))
            .write(&mut w);
        for e in &self.common {
            e.write(&mut w);
        }
        w.end();
        w.start("office:automatic-styles");
        let size = &self.pres.slide_size;
        El::new("style:page-layout")
            .with("style:name", "PM1")
            .with_child(
                El::new("style:page-layout-properties")
                    .with("fo:margin-top", "0cm")
                    .with("fo:margin-bottom", "0cm")
                    .with("fo:margin-left", "0cm")
                    .with("fo:margin-right", "0cm")
                    .with("fo:page-width", emu_str(size.cx))
                    .with("fo:page-height", emu_str(size.cy))
                    .with(
                        "style:print-orientation",
                        if size.cx >= size.cy {
                            "landscape"
                        } else {
                            "portrait"
                        },
                    ),
            )
            .write(&mut w);
        for e in &self.styles.list {
            e.write(&mut w);
        }
        w.end();
        w.start("office:master-styles");
        w.raw(masters);
        w.end();
        w.finish()
    }
}

/// A placeholder resolution fallback.
struct RunPropsDefault;

impl RunPropsDefault {
    fn run() -> kiln_doc::props::RunProps {
        kiln_doc::props::RunProps::default()
    }
}

fn alt(w: &mut XmlOut, s: &Shape) {
    if let Some(t) = &s.title {
        w.text_element("svg:title", t);
    }
    if let Some(d) = &s.alt_text {
        w.text_element("svg:desc", d);
    }
}

/// `draw:enhanced-geometry` with the preset's outline at the shape's size, adjust values as
/// `draw:modifiers` and flips.
fn enhanced(w: &mut XmlOut, preset: &str, adjust: &[(String, i64)], x: &Transform) {
    let preset = if kiln_ooxml::drawingml::presets::is_preset_shape_type(preset) {
        preset
    } else {
        "rect"
    };
    #[allow(clippy::cast_precision_loss)]
    let (vw, vh) = (
        (x.cx as f64 / 360.0).max(1.0).round(),
        (x.cy as f64 / 360.0).max(1.0).round(),
    );
    let adj: Vec<(&str, f64)> = adjust
        .iter()
        .map(|(n, v)| {
            #[allow(clippy::cast_precision_loss)]
            (n.as_str(), *v as f64)
        })
        .collect();
    w.start("draw:enhanced-geometry")
        .attr("svg:viewBox", format!("0 0 {vw} {vh}"))
        .attr("draw:type", format!("ooxml-{preset}"));
    if x.flip_h {
        w.attr("draw:mirror-horizontal", "true");
    }
    if x.flip_v {
        w.attr("draw:mirror-vertical", "true");
    }
    if !adjust.is_empty()
        && let Ok(g) = kiln_ooxml::drawingml::presets::compiled_preset(preset)
    {
        let vals = g.evaluate(21_600.0, 21_600.0, &adj);
        let mods: Vec<String> = g
            .adjust_names()
            .map(|n| format!("{}", vals.get(n).unwrap_or(0.0).round()))
            .collect();
        w.attr("draw:modifiers", mods.join(" "));
    }
    if let Ok(g) = kiln_ooxml::drawingml::presets::preset_geometry(preset, vw, vh, &adj) {
        let mut path = String::new();
        for p in &g.paths {
            let bez = p.to_bez_path(0.5);
            if bez.elements().is_empty() {
                continue;
            }
            if !path.is_empty() {
                path.push(' ');
            }
            if p.fill == kiln_ooxml::st::PathFillMode::None {
                path.push_str("F ");
            }
            if !p.stroke {
                path.push_str("S ");
            }
            let n = |v: f64| {
                let r = v.round();
                if r == 0.0 {
                    "0".to_owned()
                } else {
                    format!("{r}")
                }
            };
            for el in bez.elements() {
                match *el {
                    kurbo::PathEl::MoveTo(a) => path.push_str(&format!("M {} {} ", n(a.x), n(a.y))),
                    kurbo::PathEl::LineTo(a) => path.push_str(&format!("L {} {} ", n(a.x), n(a.y))),
                    kurbo::PathEl::QuadTo(a, b) => {
                        path.push_str(&format!("Q {} {} {} {} ", n(a.x), n(a.y), n(b.x), n(b.y)));
                    }
                    kurbo::PathEl::CurveTo(a, b, c) => path.push_str(&format!(
                        "C {} {} {} {} {} {} ",
                        n(a.x),
                        n(a.y),
                        n(b.x),
                        n(b.y),
                        n(c.x),
                        n(c.y)
                    )),
                    kurbo::PathEl::ClosePath => path.push_str("Z "),
                }
            }
            path.push('N');
        }
        w.attr("draw:enhanced-path", path);
    }
    w.end();
}

fn theme_el(t: &Theme) -> El {
    let ctx = kiln_ooxml::drawingml::theme::theme_color_context(
        t,
        kiln_ooxml::drawingml::color::ColorMap::default(),
        None,
    );
    let mut colors = El::new("loext:theme-colors").with("loext:name", t.color_scheme.name.clone());
    for (i, name) in super::read::THEME_SLOTS.iter().enumerate() {
        let c = &t.color_scheme.colors[i];
        colors.children.push(
            El::new("loext:color")
                .with("loext:name", *name)
                .with("loext:color", super::graphic::css(c, &ctx).0),
        );
    }
    El::new("loext:theme")
        .with("loext:name", t.name.clone())
        .with_child(colors)
}

fn font_decls(w: &mut XmlOut, lists: &[&Vec<El>]) {
    fn collect(e: &El, out: &mut BTreeSet<String>) {
        for (k, v) in &e.attrs {
            if k.starts_with("style:font-name") {
                out.insert(v.clone());
            }
        }
        for c in &e.children {
            collect(c, out);
        }
    }
    let mut fonts = BTreeSet::new();
    for l in lists {
        for e in l.iter() {
            collect(e, &mut fonts);
        }
    }
    w.start("office:font-face-decls");
    for f in fonts {
        w.start("style:font-face")
            .attr("style:name", &f)
            .attr(
                "svg:font-family",
                crate::sheet::style::font_family_value(&f),
            )
            .end();
    }
    w.end();
}

/// `(smil:type, smil:subtype, reverse, duration ms)` of a transition.
fn transition_attrs(t: &Transition) -> Option<(&'static str, &'static str, bool, u32)> {
    let dir8 = |d: Direction8| match d {
        Direction8::Left => "fromLeft",
        Direction8::Right => "fromRight",
        Direction8::Up => "fromTop",
        Direction8::Down => "fromBottom",
        Direction8::LeftUp => "fromTopLeft",
        Direction8::RightUp => "fromTopRight",
        Direction8::LeftDown => "fromBottomLeft",
        Direction8::RightDown => "fromBottomRight",
    };
    let d = t.duration_ms;
    Some(match t.effect {
        TransitionEffect::None => return None,
        TransitionEffect::Fade { through_black } => (
            "fade",
            if through_black {
                "fadeOverColor"
            } else {
                "crossfade"
            },
            false,
            d,
        ),
        TransitionEffect::Cut { through_black } => (
            "fade",
            if through_black {
                "fadeOverColor"
            } else {
                "crossfade"
            },
            false,
            0,
        ),
        TransitionEffect::Push { dir } => (
            "pushWipe",
            match dir {
                Side::Left => "fromRight",
                Side::Right => "fromLeft",
                Side::Up => "fromBottom",
                Side::Down => "fromTop",
            },
            false,
            d,
        ),
        TransitionEffect::Wipe { dir } => match dir {
            Side::Left => ("barWipe", "leftToRight", false, d),
            Side::Right => ("barWipe", "leftToRight", true, d),
            Side::Down => ("barWipe", "topToBottom", false, d),
            Side::Up => ("barWipe", "topToBottom", true, d),
        },
        TransitionEffect::Split { vertical, inward } => (
            "barnDoorWipe",
            if vertical { "vertical" } else { "horizontal" },
            inward,
            d,
        ),
        TransitionEffect::Reveal { through_black, .. } => (
            "fade",
            if through_black {
                "fadeOverColor"
            } else {
                "crossfade"
            },
            false,
            d,
        ),
        TransitionEffect::RandomBars { vertical } => (
            "randomBarWipe",
            if vertical { "vertical" } else { "horizontal" },
            false,
            d,
        ),
        TransitionEffect::Shape { form, inward } => match form {
            ShapeForm::Circle => ("ellipseWipe", "circle", inward, d),
            ShapeForm::Diamond => ("irisWipe", "diamond", inward, d),
            ShapeForm::Plus => ("fourBoxWipe", "cornersIn", inward, d),
        },
        TransitionEffect::Uncover { dir } => ("slideWipe", dir8(dir), true, d),
        TransitionEffect::Cover { dir } => ("slideWipe", dir8(dir), false, d),
        TransitionEffect::Flash => ("fade", "fadeFromColor", false, d),
        TransitionEffect::Morph { .. } => ("fade", "crossfade", false, d),
    })
}
