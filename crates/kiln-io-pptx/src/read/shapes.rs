// SPDX-License-Identifier: AGPL-3.0-or-later
//! `p:spTree` → shapes: `p:sp` (geometry, text, placeholders), `p:pic` (pictures with crop,
//! video and audio), `p:graphicFrame` (tables, charts, SmartArt, OLE objects), `p:grpSp`,
//! `p:cxnSp`, `mc:AlternateContent`. What the model cannot hold is kept as XML in the extras.
//!
//! Container elements are matched by local name, so the same code reads SmartArt drawings
//! (`dsp:sp`, `dsp:spPr`, … with DrawingML inside).

use kiln_common::NodeId;
use kiln_deck::geom::{ChildSpace, FULL_TURN, Transform};
use kiln_deck::shape::{
    AutoShape, ChartRef, Connection, Connector, Crop, FontCollectionRef, Geometry, Group, Media,
    MediaKind, MediaSource, Picture, Placeholder, PlaceholderSize, PlaceholderType, Shape,
    ShapeKind, ShapeProps, ShapeStyle, StyleRef,
};
use kiln_ooxml::drawingml::color::parse_color_child;
use kiln_ooxml::drawingml::effects::parse_effect_list;
use kiln_ooxml::drawingml::fill::find_fill;
use kiln_ooxml::drawingml::geometry::parse_geometry;
use kiln_ooxml::drawingml::line::parse_line;
use kiln_ooxml::xml::{XmlElement, XmlNode};

use super::context::{MediaRef, PartCx};
use super::{MAX_GROUP_DEPTH, Reader};
use crate::common::extras::{OpaqueShape, ShapeSource};
use crate::common::kind::{SUPPORTED_MC, uri};
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{attr_i32, attr_u32, child_local, fragment, local};

/// The non-visual properties of a shape (`p:nvSpPr` and friends).
#[derive(Default)]
pub(crate) struct NonVisual {
    pub file_id: Option<u32>,
    pub name: String,
    pub descr: Option<String>,
    pub title: Option<String>,
    pub hidden: bool,
    pub link: Option<String>,
    pub placeholder: Option<Placeholder>,
    pub text_box: bool,
    /// `p:nvPr` (media files, extension lists).
    pub nv_pr: Option<XmlElement>,
    /// `a:stCxn`, `a:endCxn` of a connector.
    pub start: Option<(u32, u32)>,
    pub end: Option<(u32, u32)>,
}

/// A transform read from `a:xfrm`/`p:xfrm`, with whether it had to be clamped.
pub(crate) struct ReadXfrm {
    pub xfrm: Transform,
    pub child: Option<ChildSpace>,
    pub clamped: bool,
}

/// Reads `a:xfrm` (or `p:xfrm`): offset, extent, rotation, flips and a group's child space.
pub(crate) fn read_xfrm(e: &XmlElement) -> ReadXfrm {
    let off = child_local(e, "off");
    let ext = child_local(e, "ext");
    let get = |n: Option<&XmlElement>, a: &str| n.and_then(|n| n.attr_int(a)).unwrap_or(0);
    let raw_rot = e.attr_int("rot").unwrap_or(0);
    let rot = i32::try_from(raw_rot.rem_euclid(i64::from(FULL_TURN))).unwrap_or(0);
    let x = Transform {
        x: get(off, "x"),
        y: get(off, "y"),
        cx: get(ext, "cx"),
        cy: get(ext, "cy"),
        rot,
        flip_h: e.attr_bool("flipH").unwrap_or(false),
        flip_v: e.attr_bool("flipV").unwrap_or(false),
    };
    let clamped_x = x.clamped();
    let mut clamped = clamped_x != x;
    let child = match (child_local(e, "chOff"), child_local(e, "chExt")) {
        (None, None) => None,
        (o, x) => {
            let raw = ChildSpace {
                x: get(o, "x"),
                y: get(o, "y"),
                cx: get(x, "cx"),
                cy: get(x, "cy"),
            };
            let fixed = ChildSpace {
                x: kiln_deck::geom::clamp_coord(raw.x),
                y: kiln_deck::geom::clamp_coord(raw.y),
                cx: kiln_deck::geom::clamp_extent(raw.cx),
                cy: kiln_deck::geom::clamp_extent(raw.cy),
            };
            clamped |= fixed != raw;
            Some(fixed)
        }
    };
    ReadXfrm {
        xfrm: clamped_x,
        child,
        clamped,
    }
}

fn placeholder(ph: &XmlElement) -> (Placeholder, bool) {
    let (kind, known) = match ph.attr("type") {
        None => (PlaceholderType::Object, true),
        Some(t) => {
            PlaceholderType::parse(t).map_or((PlaceholderType::Object, false), |k| (k, true))
        }
    };
    (
        Placeholder {
            kind,
            idx: attr_u32(ph, "idx").unwrap_or(0),
            vertical: ph.attr("orient") == Some("vert"),
            size: match ph.attr("sz") {
                Some("half") => PlaceholderSize::Half,
                Some("quarter") => PlaceholderSize::Quarter,
                _ => PlaceholderSize::Full,
            },
            custom_prompt: ph.attr_bool("hasCustomPrompt").unwrap_or(false),
        },
        known,
    )
}

/// `val 50000` → 50000.
fn guide_value(fmla: &str) -> Option<i64> {
    let mut it = fmla.split_whitespace();
    if it.next()? != "val" {
        return None;
    }
    let v = it.next()?.parse().ok()?;
    it.next().is_none().then_some(v)
}

fn style_ref(e: Option<&XmlElement>) -> StyleRef {
    StyleRef {
        idx: e.and_then(|e| attr_u32(e, "idx")).unwrap_or(0),
        color: parse_color_child(e),
    }
}

/// Reads `p:style`.
pub(crate) fn read_style(e: &XmlElement) -> ShapeStyle {
    let font = child_local(e, "fontRef");
    ShapeStyle {
        line: style_ref(child_local(e, "lnRef")),
        fill: style_ref(child_local(e, "fillRef")),
        effect: style_ref(child_local(e, "effectRef")),
        font: match font.and_then(|f| f.attr("idx")) {
            Some("major") => FontCollectionRef::Major,
            Some("none") => FontCollectionRef::None,
            _ => FontCollectionRef::Minor,
        },
        font_color: parse_color_child(font),
    }
}

fn location(nv: &NonVisual) -> String {
    match nv.file_id {
        Some(id) => format!("shape {id} \"{}\"", nv.name),
        None => format!("shape \"{}\"", nv.name),
    }
}

/// Elements of an `spPr` the model holds.
const SP_PR_MODELLED: [&str; 11] = [
    "xfrm",
    "prstGeom",
    "custGeom",
    "noFill",
    "solidFill",
    "gradFill",
    "pattFill",
    "blipFill",
    "grpFill",
    "ln",
    "effectLst",
];

impl Reader<'_> {
    /// Reads the shapes of a `p:spTree` or `p:grpSp` (skipping their own properties).
    pub fn read_tree(
        &mut self,
        cx: &mut PartCx,
        tree: &XmlElement,
        parent: NodeId,
        depth: usize,
    ) -> Vec<Shape> {
        let mut out = Vec::new();
        for node in &tree.children {
            let c = match node {
                XmlNode::Element(e) => e,
                XmlNode::Raw(markup) => {
                    self.extras.opaque_shapes.push(OpaqueShape {
                        parent,
                        index: out.len(),
                        xml: markup.clone(),
                    });
                    self.warn(
                        WarningCode::NestingTooDeep,
                        Severity::Warning,
                        "content nested too deeply is kept for saving but not shown",
                        &cx.part,
                        "",
                    );
                    continue;
                }
                _ => continue,
            };
            match local(c) {
                "nvGrpSpPr" | "grpSpPr" | "extLst" => {}
                "AlternateContent" => self.read_alternate(cx, c, parent, depth, &mut out),
                "contentPart" => {
                    self.opaque(cx, c, parent, out.len());
                    self.warn(
                        WarningCode::ContentPart,
                        Severity::Info,
                        "ink is kept for saving but not shown",
                        &cx.part,
                        "",
                    );
                }
                _ => match self.read_shape(cx, c, depth) {
                    Some(s) => out.push(s),
                    None => self.opaque(cx, c, parent, out.len()),
                },
            }
        }
        out
    }

    fn opaque(&mut self, cx: &PartCx, e: &XmlElement, parent: NodeId, index: usize) {
        self.extras.opaque_shapes.push(OpaqueShape {
            parent,
            index,
            xml: fragment(e, &cx.scope),
        });
    }

    fn read_alternate(
        &mut self,
        cx: &mut PartCx,
        ac: &XmlElement,
        parent: NodeId,
        depth: usize,
        out: &mut Vec<Shape>,
    ) {
        let chosen = ac.select_alternate_content(&SUPPORTED_MC);
        let xml = fragment(ac, &cx.scope);
        let before = out.len();
        for c in chosen {
            if let Some(s) = self.read_shape(cx, c, depth) {
                // An OLE object keeps the whole block as its source; anything else is marked as
                // read from alternate content.
                self.extras
                    .shape_sources
                    .entry(s.id)
                    .and_modify(|src| {
                        if let ShapeSource::Ole { xml: x } = src {
                            x.clone_from(&xml);
                        }
                    })
                    .or_insert_with(|| ShapeSource::AlternateContent { xml: xml.clone() });
                out.push(s);
            }
        }
        if out.len() == before {
            self.opaque(cx, ac, parent, out.len());
            self.warn(
                WarningCode::ShapeUnknown,
                Severity::Info,
                "content for newer versions of PowerPoint is kept for saving but not shown",
                &cx.part,
                "",
            );
        }
    }

    /// Reads one shape element; `None` for an element the model has no shape for (the caller
    /// keeps it).
    pub fn read_shape(&mut self, cx: &mut PartCx, e: &XmlElement, depth: usize) -> Option<Shape> {
        match local(e) {
            "sp" => Some(self.read_sp(cx, e)),
            "pic" => Some(self.read_pic(cx, e)),
            "cxnSp" => Some(self.read_cxn(cx, e)),
            "grpSp" => {
                if depth >= MAX_GROUP_DEPTH {
                    self.warn(
                        WarningCode::NestingTooDeep,
                        Severity::Warning,
                        "groups nested this deeply are kept for saving but not shown",
                        &cx.part,
                        "",
                    );
                    return None;
                }
                Some(self.read_group(cx, e, depth))
            }
            "graphicFrame" => self.read_frame(cx, e),
            _ => {
                self.warn(
                    WarningCode::ShapeUnknown,
                    Severity::Info,
                    format!("a {} is kept for saving but not shown", e.name),
                    &cx.part,
                    "",
                );
                None
            }
        }
    }

    /// Reads the non-visual properties (`p:nvSpPr`, `p:nvPicPr`, …).
    fn read_nv(&mut self, cx: &PartCx, shape: &XmlElement) -> NonVisual {
        let mut nv = NonVisual::default();
        let Some(nvx) = shape.elements().find(|c| local(c).starts_with("nv")) else {
            return nv;
        };
        if let Some(c) = child_local(nvx, "cNvPr") {
            nv.file_id = attr_u32(c, "id");
            nv.name = c.attr("name").unwrap_or_default().to_owned();
            nv.descr = c.attr("descr").filter(|s| !s.is_empty()).map(str::to_owned);
            nv.title = c.attr("title").filter(|s| !s.is_empty()).map(str::to_owned);
            nv.hidden = c.attr_bool("hidden").unwrap_or(false);
            if let Some(h) = c.child("a:hlinkClick") {
                nv.link = self.link(cx, h);
            }
        }
        for c in nvx.elements() {
            match local(c) {
                "cNvSpPr" => nv.text_box = c.attr_bool("txBox").unwrap_or(false),
                "cNvCxnSpPr" => {
                    let end = |n: &str| {
                        c.child(n).map(|x| {
                            (
                                attr_u32(x, "id").unwrap_or(0),
                                attr_u32(x, "idx").unwrap_or(0),
                            )
                        })
                    };
                    nv.start = end("a:stCxn");
                    nv.end = end("a:endCxn");
                }
                "nvPr" => {
                    if let Some(ph) = c.child("p:ph") {
                        let (p, known) = placeholder(ph);
                        if !known {
                            self.warn(
                                WarningCode::PlaceholderUnknown,
                                Severity::Warning,
                                format!(
                                    "the placeholder type {:?} is unknown; read as content",
                                    ph.attr("type").unwrap_or_default()
                                ),
                                &cx.part,
                                location(&nv),
                            );
                        }
                        nv.placeholder = Some(p);
                    }
                    nv.nv_pr = Some(c.clone());
                }
                _ => {}
            }
        }
        nv
    }

    /// A new shape with its id registered.
    pub(crate) fn new_shape(
        &mut self,
        cx: &mut PartCx,
        nv: &NonVisual,
        xfrm: Option<Transform>,
        kind: ShapeKind,
    ) -> Shape {
        let id = self.pres.alloc_id();
        if let Some(f) = nv.file_id {
            cx.ids.entry(f).or_insert(id);
            self.extras.shape_ids.insert(id, f);
        }
        if let Some(nvpr) = &nv.nv_pr {
            for c in nvpr.elements() {
                if matches!(local(c), "extLst" | "custDataLst") {
                    let x = fragment(c, &cx.scope);
                    self.extras.push_shape_xml(id, x);
                }
            }
        }
        Shape {
            id,
            name: nv.name.clone(),
            alt_text: nv.descr.clone(),
            title: nv.title.clone(),
            hidden: nv.hidden,
            placeholder: nv.placeholder,
            xfrm,
            kind,
            link: nv.link.clone(),
        }
    }

    fn xfrm_of(&mut self, cx: &PartCx, e: Option<&XmlElement>, nv: &NonVisual) -> Option<ReadXfrm> {
        let x = read_xfrm(e?);
        if x.clamped {
            self.warn(
                WarningCode::ShapeClamped,
                Severity::Warning,
                "a position or size outside the DrawingML range was clamped",
                &cx.part,
                location(nv),
            );
        }
        Some(x)
    }

    /// Reads `p:spPr` (or `p:grpSpPr`) into shape properties; unmodelled children are returned
    /// as fragments.
    fn read_sp_pr(
        &mut self,
        cx: &PartCx,
        sp_pr: Option<&XmlElement>,
        style: Option<&XmlElement>,
        loc: &str,
    ) -> (ShapeProps, Vec<String>) {
        let mut props = ShapeProps {
            style: style.map(read_style),
            ..ShapeProps::default()
        };
        let mut kept = Vec::new();
        let Some(sp_pr) = sp_pr else {
            return (props, kept);
        };
        for c in sp_pr.elements() {
            match local(c) {
                "prstGeom" => {
                    let adjust = c
                        .child("a:avLst")
                        .map(|l| {
                            l.children_named("a:gd")
                                .filter_map(|g| {
                                    Some((
                                        g.attr("name")?.to_owned(),
                                        guide_value(g.attr("fmla")?)?,
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    props.geometry = Some(Geometry::Preset {
                        name: c.attr("prst").unwrap_or("rect").to_owned(),
                        adjust,
                    });
                }
                "custGeom" => props.geometry = Some(Geometry::Custom(parse_geometry(c))),
                "ln" => {
                    let mut line = parse_line(c);
                    if let Some(f) = line.fill.as_mut() {
                        self.fix_fill(cx, f, loc);
                    }
                    props.line = Some(line);
                }
                "effectLst" => props.effects = Some(parse_effect_list(c)),
                "xfrm" => {}
                n if SP_PR_MODELLED.contains(&n) => {}
                _ => kept.push(fragment(c, &cx.scope)),
            }
        }
        if let Some(mut f) = find_fill(sp_pr) {
            self.fix_fill(cx, &mut f, loc);
            props.fill = Some(f);
        }
        (props, kept)
    }

    fn read_sp(&mut self, cx: &mut PartCx, e: &XmlElement) -> Shape {
        let nv = self.read_nv(cx, e);
        let loc = location(&nv);
        let sp_pr = child_local(e, "spPr");
        let xfrm = self
            .xfrm_of(cx, sp_pr.and_then(|s| child_local(s, "xfrm")), &nv)
            .map(|x| x.xfrm);
        let (props, kept) = self.read_sp_pr(cx, sp_pr, child_local(e, "style"), &loc);
        let text = child_local(e, "txBody").map(|t| self.read_text_body(cx, t, &loc));
        let kind = ShapeKind::Geometry(AutoShape {
            text_box: nv.text_box,
            props,
            text,
        });
        let shape = self.new_shape(cx, &nv, xfrm, kind);
        for k in kept {
            self.extras.push_shape_xml(shape.id, k);
        }
        shape
    }

    fn read_pic(&mut self, cx: &mut PartCx, e: &XmlElement) -> Shape {
        let nv = self.read_nv(cx, e);
        let loc = location(&nv);
        let sp_pr = child_local(e, "spPr");
        let xfrm = self
            .xfrm_of(cx, sp_pr.and_then(|s| child_local(s, "xfrm")), &nv)
            .map(|x| x.xfrm);
        let (props, kept) = self.read_sp_pr(cx, sp_pr, child_local(e, "style"), &loc);
        let blip_fill = child_local(e, "blipFill");
        let blip = blip_fill.and_then(|b| b.child("a:blip"));
        let mut image = None;
        if let Some(id) = blip
            .and_then(|b| b.attr("r:embed"))
            .filter(|s| !s.is_empty())
        {
            match self.media(cx, id) {
                MediaRef::Part(key) => image = Some(key),
                _ => self.warn(
                    WarningCode::ImageMissing,
                    Severity::Warning,
                    "a picture's image is missing",
                    &cx.part,
                    loc.clone(),
                ),
            }
        } else if let Some(id) = blip
            .and_then(|b| b.attr("r:link"))
            .filter(|s| !s.is_empty())
        {
            let what = match self.media(cx, id) {
                MediaRef::External(url) => url,
                _ => id.to_owned(),
            };
            self.warn(
                WarningCode::ImageLinked,
                Severity::Info,
                format!("a picture is linked to {what} and not embedded; it is not shown"),
                &cx.part,
                loc.clone(),
            );
        }
        let media = self.read_media(cx, &nv, &loc);
        let kind = if let Some((kind, source, trim)) = media {
            ShapeKind::Media(Media {
                kind,
                source,
                poster: image,
                trim_start_ms: trim.0,
                trim_end_ms: trim.1,
                props,
            })
        } else {
            let crop = blip_fill
                .and_then(|b| b.child("a:srcRect"))
                .map(|r| Crop {
                    l: attr_i32(r, "l").unwrap_or(0),
                    t: attr_i32(r, "t").unwrap_or(0),
                    r: attr_i32(r, "r").unwrap_or(0),
                    b: attr_i32(r, "b").unwrap_or(0),
                })
                .unwrap_or_default();
            let alpha = blip
                .and_then(|b| b.child("a:alphaModFix"))
                .map(|a| attr_u32(a, "amt").unwrap_or(100_000));
            ShapeKind::Picture(Picture {
                media: image.unwrap_or_default(),
                crop,
                props,
                alpha,
            })
        };
        let shape = self.new_shape(cx, &nv, xfrm, kind);
        for k in kept {
            self.extras.push_shape_xml(shape.id, k);
        }
        shape
    }

    /// Video or audio on a `p:pic`: (kind, source, (trim start, trim end)).
    fn read_media(
        &mut self,
        cx: &PartCx,
        nv: &NonVisual,
        loc: &str,
    ) -> Option<(MediaKind, MediaSource, (u32, u32))> {
        let nvpr = nv.nv_pr.as_ref()?;
        let file = nvpr.elements().find(|c| {
            matches!(
                local(c),
                "videoFile" | "audioFile" | "quickTimeFile" | "wavAudioFile" | "audioCd"
            )
        })?;
        let kind = if matches!(local(file), "audioFile" | "wavAudioFile" | "audioCd") {
            MediaKind::Audio
        } else {
            MediaKind::Video
        };
        // PowerPoint 2010+ embeds the clip through p14:media; the a:videoFile link is then a
        // pointer to the same part (or to the original file for older readers).
        let p14 = nvpr.descendants("p14:media").into_iter().next().cloned();
        let mut trim = (0, 0);
        if let Some(t) = p14.as_ref().and_then(|m| m.child("p14:trim")) {
            let ms = |a: &str| {
                t.attr_num(a)
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map_or(0, |v| v.round().min(f64::from(u32::MAX)) as u32)
            };
            trim = (ms("st"), ms("end"));
        }
        let ids = p14
            .as_ref()
            .and_then(|m| m.attr("r:embed").or_else(|| m.attr("r:link")))
            .into_iter()
            .chain(file.attr("r:link").or_else(|| file.attr("r:embed")))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut source = None;
        for id in &ids {
            match self.media(cx, id) {
                MediaRef::Part(key) => {
                    source = Some(MediaSource::Embedded(key));
                    break;
                }
                MediaRef::External(url) => {
                    if source.is_none() {
                        source = Some(MediaSource::Linked(url));
                    }
                }
                MediaRef::Missing => {}
            }
        }
        let source = source.unwrap_or_else(|| MediaSource::Linked(String::new()));
        if let MediaSource::Linked(url) = &source {
            self.warn(
                WarningCode::MediaLinked,
                Severity::Info,
                format!(
                    "a clip is linked to {} and not embedded",
                    if url.is_empty() {
                        "a missing file"
                    } else {
                        url
                    }
                ),
                &cx.part,
                loc.to_owned(),
            );
        }
        Some((kind, source, trim))
    }

    fn read_cxn(&mut self, cx: &mut PartCx, e: &XmlElement) -> Shape {
        let nv = self.read_nv(cx, e);
        let loc = location(&nv);
        let sp_pr = child_local(e, "spPr");
        let xfrm = self
            .xfrm_of(cx, sp_pr.and_then(|s| child_local(s, "xfrm")), &nv)
            .map(|x| x.xfrm);
        let (props, kept) = self.read_sp_pr(cx, sp_pr, child_local(e, "style"), &loc);
        let kind = ShapeKind::Connector(Connector {
            props,
            start: None,
            end: None,
        });
        let shape = self.new_shape(cx, &nv, xfrm, kind);
        if nv.start.is_some() || nv.end.is_some() {
            cx.connections.push((shape.id, nv.start, nv.end));
        }
        for k in kept {
            self.extras.push_shape_xml(shape.id, k);
        }
        shape
    }

    fn read_group(&mut self, cx: &mut PartCx, e: &XmlElement, depth: usize) -> Shape {
        let nv = self.read_nv(cx, e);
        let loc = location(&nv);
        let gp = child_local(e, "grpSpPr");
        let x = self.xfrm_of(cx, gp.and_then(|s| child_local(s, "xfrm")), &nv);
        let (props, kept) = self.read_sp_pr(cx, gp, None, &loc);
        let mut shape = self.new_shape(
            cx,
            &nv,
            x.as_ref().map(|x| x.xfrm),
            ShapeKind::Group(Group::default()),
        );
        let children = self.read_tree(cx, e, shape.id, depth + 1);
        let child_space = x.as_ref().and_then(|x| x.child).unwrap_or_else(|| {
            // No child space: children are in the group's own coordinates.
            x.as_ref()
                .map_or_else(ChildSpace::default, |x| ChildSpace::identity(x.xfrm.rect()))
        });
        shape.kind = ShapeKind::Group(Group {
            child_space,
            children,
            props,
        });
        for k in kept {
            self.extras.push_shape_xml(shape.id, k);
        }
        shape
    }

    fn read_frame(&mut self, cx: &mut PartCx, e: &XmlElement) -> Option<Shape> {
        let nv = self.read_nv(cx, e);
        let loc = location(&nv);
        let xfrm = self
            .xfrm_of(cx, child_local(e, "xfrm"), &nv)
            .map(|x| x.xfrm);
        let data = child_local(e, "graphic").and_then(|g| child_local(g, "graphicData"));
        let uri = data.and_then(|d| d.attr("uri")).unwrap_or_default();
        let data = data?;
        match uri {
            uri::TABLE => {
                let tbl = data.child("a:tbl")?;
                let t = self.read_table(cx, tbl, &loc)?;
                Some(self.new_shape(cx, &nv, xfrm, ShapeKind::Table(t)))
            }
            uri::CHART | uri::CHART_EX => {
                let id = data
                    .elements()
                    .find(|c| local(c) == "chart")
                    .and_then(|c| c.attr("r:id"))?;
                let part = cx.target(id)?;
                if !self.store_chart(&part) {
                    self.warn(
                        WarningCode::PartMissing,
                        Severity::Warning,
                        "a chart's part is missing",
                        &cx.part,
                        loc,
                    );
                    return None;
                }
                if let Err(err) = kiln_charts::package::read_part(self.pkg, &part) {
                    self.warn(
                        WarningCode::ChartUnreadable,
                        Severity::Warning,
                        format!("the chart cannot be read ({err}); it is kept for saving"),
                        &part,
                        "",
                    );
                }
                Some(self.new_shape(cx, &nv, xfrm, ShapeKind::Chart(ChartRef { part })))
            }
            uri::DIAGRAM => Some(self.read_smart_art(cx, e, data, &nv, xfrm)),
            uri::OLE => Some(self.read_ole(cx, e, data, &nv, xfrm)),
            _ => {
                self.warn(
                    WarningCode::ShapeUnknown,
                    Severity::Info,
                    format!("a graphic of type {uri} is kept for saving but not shown"),
                    &cx.part,
                    loc,
                );
                None
            }
        }
    }

    /// An OLE object: its preview picture (`p:oleObj/p:pic`), else an empty picture.
    fn read_ole(
        &mut self,
        cx: &mut PartCx,
        frame: &XmlElement,
        data: &XmlElement,
        nv: &NonVisual,
        xfrm: Option<Transform>,
    ) -> Shape {
        let ole = data.child("p:oleObj");
        let preview = ole.and_then(|o| o.child("p:pic"));
        let mut shape = match preview {
            Some(pic) => {
                let mut s = self.read_pic(cx, pic);
                // The frame, not the preview, names and places the object.
                if let Some(f) = nv.file_id {
                    cx.ids.insert(f, s.id);
                    self.extras.shape_ids.insert(s.id, f);
                }
                s.name.clone_from(&nv.name);
                s.alt_text.clone_from(&nv.descr);
                s.title.clone_from(&nv.title);
                s.hidden = nv.hidden;
                s.placeholder = nv.placeholder;
                s.link.clone_from(&nv.link);
                if xfrm.is_some() {
                    s.xfrm = xfrm;
                }
                s
            }
            None => self.new_shape(cx, nv, xfrm, ShapeKind::Picture(Picture::default())),
        };
        if let ShapeKind::Media(_) = shape.kind {
            shape.kind = ShapeKind::Picture(Picture::default());
        }
        self.extras.shape_sources.insert(
            shape.id,
            ShapeSource::Ole {
                xml: fragment(frame, &cx.scope),
            },
        );
        let what = ole
            .and_then(|o| o.attr("progId"))
            .filter(|s| !s.is_empty())
            .unwrap_or("an embedded object");
        self.warn(
            WarningCode::OleObject,
            Severity::Info,
            format!("{what} is shown as its picture and kept for saving"),
            &cx.part,
            location(nv),
        );
        shape
    }

    /// Resolves connector ends to shape ids once the whole part is read.
    pub fn resolve_connections(&mut self, cx: &PartCx, shapes: &mut [Shape]) {
        if cx.connections.is_empty() {
            return;
        }
        let resolve = |end: Option<(u32, u32)>| {
            end.and_then(|(id, site)| cx.ids.get(&id).map(|s| Connection { shape: *s, site }))
        };
        for (connector, start, end) in &cx.connections {
            let (s, e) = (resolve(*start), resolve(*end));
            for sh in shapes.iter_mut() {
                set_connection(sh, *connector, s, e);
            }
        }
    }
}

fn set_connection(sh: &mut Shape, id: NodeId, s: Option<Connection>, e: Option<Connection>) {
    if sh.id == id {
        if let ShapeKind::Connector(c) = &mut sh.kind {
            c.start = s;
            c.end = e;
        }
        return;
    }
    if let Some(children) = sh.children_mut() {
        for c in children {
            set_connection(c, id, s, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn guides_and_transforms_read() {
        assert_eq!(guide_value("val 50000"), Some(50_000));
        assert_eq!(guide_value("val -3"), Some(-3));
        assert_eq!(guide_value("*/ w 1 2"), None);
        assert_eq!(guide_value("val"), None);
        let x = parse_xml_str(
            r#"<a:xfrm xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" rot="-5400000" flipH="1"><a:off x="10" y="20"/><a:ext cx="30" cy="40"/><a:chOff x="1" y="2"/><a:chExt cx="3" cy="4"/></a:xfrm>"#,
        )
        .unwrap()
        .root;
        let r = read_xfrm(&x);
        assert_eq!(r.xfrm.rot, 16_200_000);
        assert!(r.xfrm.flip_h && !r.xfrm.flip_v);
        assert_eq!((r.xfrm.x, r.xfrm.cy), (10, 40));
        assert_eq!(r.child.unwrap().cx, 3);
        assert!(!r.clamped);
        let huge = parse_xml_str(
            r#"<a:xfrm xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:off x="99999999999999999" y="0"/><a:ext cx="-5" cy="1"/></a:xfrm>"#,
        )
        .unwrap()
        .root;
        let r = read_xfrm(&huge);
        assert!(r.clamped && r.xfrm.is_valid());
    }
}
