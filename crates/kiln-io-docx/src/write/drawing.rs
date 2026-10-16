// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML for WordprocessingML (`w:drawing`): pictures (`pic:pic`), shapes and text boxes
//! (`wps:wsp`) and charts, placed inline (`wp:inline`) or floating (`wp:anchor`); legacy VML
//! (`w:pict`) as read.
//!
//! A drawing read from a file keeps its original XML ([`Drawing::xml`]). The writer derives the
//! model's view of that XML the way the reader does; when nothing changed the XML is written back
//! verbatim (an `mc:AlternateContent` block included), otherwise only what changed is patched —
//! size, placement and wrapping, alt text, rotation and flips, the picture's image and cropping,
//! the preset geometry — so fills, effects and Office extensions survive editing. A changed
//! drawing that came wrapped in `mc:AlternateContent` loses its (now stale) VML fallback. Text box
//! content is always written from the model. Drawings created in the editor are built from the
//! model.

use kiln_doc::st::{AlignH, AlignV, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{Anchor, Crop, Drawing, DrawingKind, Placement, PosH, PosV};
use kiln_ooxml::opc::rel_type;
use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{Attrs, XmlElement, XmlNode};

use super::story::StoryWriter;
use super::xml::El;
use crate::common::units::{parse_decimal, parse_emu, parse_on_off};
use crate::common::xml::{fragment_xml, parse_fragment};

const URI_PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const URI_SHAPE: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const URI_GROUP: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const URI_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const URI_CHARTEX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DECORATIVE_EXT: &str = "{C183D7F6-B498-43B3-948B-1728B52AA6E4}";
const DECORATIVE_NS: &str = "http://schemas.microsoft.com/office/drawing/2017/decorative";
/// The placeholder put where a text box's content goes while the rest is serialised.
const TXBX_PLACEHOLDER: &str = "<kilntxbx/>";
/// A tight or through wrap needs a polygon; this one is the object's bounding box (Word's).
const DEFAULT_POLYGON: &str = "<wp:wrapPolygon edited=\"0\"><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"0\" y=\"21600\"/><wp:lineTo x=\"21600\" y=\"21600\"/><wp:lineTo x=\"21600\" y=\"0\"/><wp:lineTo x=\"0\" y=\"0\"/></wp:wrapPolygon>";
/// Word's base for `relativeHeight` of new floating objects.
const Z_BASE: i64 = 251_658_240;

fn frag(el: &XmlElement) -> String {
    fragment_xml(el, &Attrs::new())
}

/// The model's view of a DrawingML drawing, derived the way the reader derives it.
#[derive(Debug, PartialEq)]
struct Derived {
    kind: DrawingKind,
    width: i64,
    height: i64,
    placement: Placement,
    name: String,
    description: Option<String>,
    title: Option<String>,
    decorative: bool,
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
}

fn view(d: &Drawing) -> Derived {
    Derived {
        kind: d.kind.clone(),
        width: d.width,
        height: d.height,
        placement: d.placement.clone(),
        name: d.name.clone(),
        description: d.description.clone(),
        title: d.title.clone(),
        decorative: d.decorative,
        rotation: d.rotation,
        flip_h: d.flip_h,
        flip_v: d.flip_v,
    }
}

fn pos_h(node: Option<&XmlElement>) -> PosH {
    let Some(node) = node else {
        return PosH::Offset(0);
    };
    if let Some(a) = node
        .child("wp:align")
        .and_then(|a| AlignH::parse(a.text().trim()))
    {
        return PosH::Align(a);
    }
    PosH::Offset(
        node.child("wp:posOffset")
            .and_then(|o| parse_emu(Some(&o.text())))
            .unwrap_or(0),
    )
}

fn pos_v(node: Option<&XmlElement>) -> PosV {
    let Some(node) = node else {
        return PosV::Offset(0);
    };
    if let Some(a) = node
        .child("wp:align")
        .and_then(|a| AlignV::parse(a.text().trim()))
    {
        return PosV::Align(a);
    }
    PosV::Offset(
        node.child("wp:posOffset")
            .and_then(|o| parse_emu(Some(&o.text())))
            .unwrap_or(0),
    )
}

/// `wp:anchor` → [`Anchor`], as the reader parses it.
fn derive_anchor(anchor: &XmlElement) -> Anchor {
    let behind = parse_on_off(anchor.attr("behindDoc"), false);
    let floating = if behind {
        WrapType::Behind
    } else {
        WrapType::InFront
    };
    let mut wrap = floating;
    let mut wrap_side = None;
    let mut wrap_polygon_xml = None;
    for c in anchor.elements() {
        let w = match c.name.as_str() {
            "wp:wrapSquare" => WrapType::Square,
            "wp:wrapTight" => WrapType::Tight,
            "wp:wrapThrough" => WrapType::Through,
            "wp:wrapTopAndBottom" => WrapType::TopAndBottom,
            "wp:wrapNone" => floating,
            _ => continue,
        };
        wrap = w;
        wrap_side = c.attr("wrapText").and_then(WrapSide::parse);
        wrap_polygon_xml = c.child("wp:wrapPolygon").map(frag);
        break;
    }
    let ph = anchor.child("wp:positionH");
    let pv = anchor.child("wp:positionV");
    let mut relative_h = ph
        .and_then(|p| p.attr("relativeFrom"))
        .and_then(RelativeFromH::parse)
        .unwrap_or_default();
    let mut relative_v = pv
        .and_then(|p| p.attr("relativeFrom"))
        .and_then(RelativeFromV::parse)
        .unwrap_or_default();
    let (h, v) = if parse_on_off(anchor.attr("simplePos"), false) {
        let sp = anchor.child("wp:simplePos");
        relative_h = RelativeFromH::Page;
        relative_v = RelativeFromV::Page;
        (
            PosH::Offset(parse_emu(sp.and_then(|s| s.attr("x"))).unwrap_or(0)),
            PosV::Offset(parse_emu(sp.and_then(|s| s.attr("y"))).unwrap_or(0)),
        )
    } else {
        (pos_h(ph), pos_v(pv))
    };
    let dist = |k: &str| parse_emu(anchor.attr(k)).unwrap_or(0);
    Anchor {
        wrap,
        wrap_side,
        relative_h,
        h,
        relative_v,
        v,
        dist_top: dist("distT"),
        dist_bottom: dist("distB"),
        dist_left: dist("distL"),
        dist_right: dist("distR"),
        allow_overlap: parse_on_off(anchor.attr("allowOverlap"), true),
        layout_in_cell: parse_on_off(anchor.attr("layoutInCell"), true),
        locked: parse_on_off(anchor.attr("locked"), false),
        z_order: parse_decimal(anchor.attr("relativeHeight")).unwrap_or(0),
        wrap_polygon_xml,
    }
}

fn xfrm_values(xfrm: Option<&XmlElement>) -> (i32, bool, bool) {
    let rot = parse_decimal(xfrm.and_then(|x| x.attr("rot"))).unwrap_or(0);
    (
        i32::try_from(rot.rem_euclid(21_600_000)).unwrap_or(0),
        parse_on_off(xfrm.and_then(|x| x.attr("flipH")), false),
        parse_on_off(xfrm.and_then(|x| x.attr("flipV")), false),
    )
}

fn derive_crop(src: Option<&XmlElement>) -> Option<Crop> {
    let src = src?;
    let f = |k: &str| {
        parse_decimal(src.attr(k))
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0)
    };
    let c = Crop {
        left: f("l"),
        top: f("t"),
        right: f("r"),
        bottom: f("b"),
    };
    (c != Crop::default()).then_some(c)
}

fn is_decorative(doc_pr: Option<&XmlElement>) -> bool {
    doc_pr.is_some_and(|d| {
        d.descendants("a:ext").iter().any(|e| {
            e.elements()
                .any(|c| local_name(&c.name) == "decorative" && parse_on_off(c.attr("val"), false))
        })
    })
}

/// The `w:drawing` of a kept drawing: the element itself, or the first `mc:Choice`'s.
fn find_drawing(root: &mut XmlElement) -> Option<&mut XmlElement> {
    if root.name == "w:drawing" {
        return Some(root);
    }
    if root.name == "mc:AlternateContent" {
        for choice in root.children.iter_mut().filter_map(XmlNode::as_element_mut) {
            if choice.name != "mc:Choice" {
                continue;
            }
            if let Some(d) = choice
                .children
                .iter_mut()
                .filter_map(XmlNode::as_element_mut)
                .find(|c| c.name == "w:drawing")
            {
                return Some(d);
            }
        }
    }
    None
}

fn child_mut<'e>(el: &'e mut XmlElement, path: &[&str]) -> Option<&'e mut XmlElement> {
    let mut cur = el;
    for name in path {
        cur = cur.child_mut(name)?;
    }
    Some(cur)
}

/// Replaces the content of every `w:txbxContent` with the placeholder.
fn mark_text_boxes(el: &mut XmlElement) -> usize {
    let mut n = 0;
    for c in el.children.iter_mut().filter_map(XmlNode::as_element_mut) {
        if c.name == "w:txbxContent" {
            c.children = vec![XmlNode::Element(XmlElement::new("kilntxbx"))];
            n += 1;
        } else {
            n += mark_text_boxes(c);
        }
    }
    n
}

fn axis_xml(name: &str, rel: &str, align: Option<&str>, offset: i64) -> String {
    let inner = match align {
        Some(a) => format!("<wp:align>{a}</wp:align>"),
        None => format!("<wp:posOffset>{offset}</wp:posOffset>"),
    };
    format!("<{name} relativeFrom=\"{rel}\">{inner}</{name}>")
}

fn wrap_xml(a: &Anchor) -> String {
    let side = a.wrap_side.unwrap_or(WrapSide::BothSides).as_str();
    let polygon = a.wrap_polygon_xml.as_deref().unwrap_or(DEFAULT_POLYGON);
    match a.wrap {
        WrapType::Square => format!("<wp:wrapSquare wrapText=\"{side}\"/>"),
        WrapType::Tight => format!("<wp:wrapTight wrapText=\"{side}\">{polygon}</wp:wrapTight>"),
        WrapType::Through => {
            format!("<wp:wrapThrough wrapText=\"{side}\">{polygon}</wp:wrapThrough>")
        }
        WrapType::TopAndBottom => "<wp:wrapTopAndBottom/>".to_owned(),
        WrapType::Behind | WrapType::InFront | WrapType::None | WrapType::Inline => {
            "<wp:wrapNone/>".to_owned()
        }
    }
}

/// The parts of a container that are kept when its placement is rebuilt.
struct Kept {
    effect_extent: Option<String>,
    doc_pr: String,
    frame_pr: Option<String>,
    graphic: String,
    /// `wp14:sizeRelH`/`wp14:sizeRelV` and other extensions after the graphic (anchors only).
    trailing: String,
}

fn anchor_xml(a: &Anchor, width: i64, height: i64, k: &Kept) -> String {
    let b = |v: bool| if v { "1" } else { "0" };
    let el = El::new("wp:anchor")
        .attr("distT", a.dist_top)
        .attr("distB", a.dist_bottom)
        .attr("distL", a.dist_left)
        .attr("distR", a.dist_right)
        .attr("simplePos", "0")
        .attr("relativeHeight", a.z_order)
        .attr("behindDoc", b(a.wrap == WrapType::Behind))
        .attr("locked", b(a.locked))
        .attr("layoutInCell", b(a.layout_in_cell))
        .attr("allowOverlap", b(a.allow_overlap));
    let (h_align, h_off) = match a.h {
        PosH::Align(x) => (Some(x.as_str()), 0),
        PosH::Offset(o) => (None, o),
    };
    let (v_align, v_off) = match a.v {
        PosV::Align(x) => (Some(x.as_str()), 0),
        PosV::Offset(o) => (None, o),
    };
    let inner = format!(
        "<wp:simplePos x=\"0\" y=\"0\"/>{}{}<wp:extent cx=\"{width}\" cy=\"{height}\"/>{}{}{}{}{}{}",
        axis_xml("wp:positionH", a.relative_h.as_str(), h_align, h_off),
        axis_xml("wp:positionV", a.relative_v.as_str(), v_align, v_off),
        k.effect_extent
            .as_deref()
            .unwrap_or("<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>"),
        wrap_xml(a),
        k.doc_pr,
        k.frame_pr.as_deref().unwrap_or("<wp:cNvGraphicFramePr/>"),
        k.graphic,
        k.trailing,
    );
    el.wrap(&inner)
}

fn inline_xml(width: i64, height: i64, k: &Kept) -> String {
    format!(
        "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width}\" cy=\"{height}\"/>{}{}{}{}</wp:inline>",
        k.effect_extent
            .as_deref()
            .unwrap_or("<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>"),
        k.doc_pr,
        k.frame_pr.as_deref().unwrap_or("<wp:cNvGraphicFramePr/>"),
        k.graphic,
    )
}

fn container_xml(d: &Drawing, k: &Kept) -> String {
    match &d.placement {
        Placement::Inline => inline_xml(d.width, d.height, k),
        Placement::Anchor(a) => anchor_xml(a, d.width, d.height, k),
    }
}

/// `wp:docPr` attributes and the decorative flag, patched or built.
fn doc_pr_xml(d: &Drawing, id: u64, kept: Option<&XmlElement>) -> String {
    let mut el = kept.cloned().unwrap_or_else(|| {
        XmlElement::new("wp:docPr")
            .with_attr("id", id)
            .with_attr("name", "")
    });
    let name = if d.name.is_empty() && kept.is_none() {
        format!("Picture {id}")
    } else {
        d.name.clone()
    };
    el.set_attr("name", Some(name.as_str()));
    el.set_attr("descr", d.description.as_deref());
    el.set_attr("title", d.title.as_deref());
    if is_decorative(Some(&el)) != d.decorative {
        // Remove any decorative extension, then add one when needed.
        if let Some(ext_lst) = el.child_mut("a:extLst") {
            ext_lst.children.retain(|c| {
                c.as_element()
                    .is_none_or(|e| e.attr("uri") != Some(DECORATIVE_EXT))
            });
        }
        if el
            .child("a:extLst")
            .is_some_and(|l| l.elements().next().is_none())
        {
            el.remove_children("a:extLst");
        }
        if d.decorative {
            let ext = XmlElement::new("a:ext")
                .with_attr("uri", DECORATIVE_EXT)
                .with_child(
                    XmlElement::new("adec:decorative")
                        .with_attr("xmlns:adec", DECORATIVE_NS)
                        .with_attr("val", "1"),
                );
            match el.child_mut("a:extLst") {
                Some(l) => l.children.push(ext.into()),
                None => el
                    .children
                    .push(XmlElement::new("a:extLst").with_child(ext).into()),
            }
        }
    }
    frag(&el)
}

fn xfrm_xml(rotation: i32, flip_h: bool, flip_v: bool, width: i64, height: i64) -> String {
    El::new("a:xfrm")
        .opt("rot", (rotation != 0).then_some(rotation))
        .opt("flipH", flip_h.then_some("1"))
        .opt("flipV", flip_v.then_some("1"))
        .wrap(&format!(
            "<a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/>"
        ))
}

fn crop_xml(crop: Option<&Crop>) -> String {
    let Some(c) = crop else {
        return String::new();
    };
    El::new("a:srcRect")
        .opt("l", (c.left != 0).then_some(c.left))
        .opt("t", (c.top != 0).then_some(c.top))
        .opt("r", (c.right != 0).then_some(c.right))
        .opt("b", (c.bottom != 0).then_some(c.bottom))
        .empty()
}

/// Sets `a:xfrm` rotation, flips and size.
fn patch_xfrm(xfrm: &mut XmlElement, d: &Drawing) {
    xfrm.set_attr("rot", (d.rotation != 0).then_some(d.rotation));
    xfrm.set_attr("flipH", d.flip_h.then_some("1"));
    xfrm.set_attr("flipV", d.flip_v.then_some("1"));
    if let Some(ext) = xfrm.child_mut("a:ext") {
        ext.set_attr("cx", Some(d.width));
        ext.set_attr("cy", Some(d.height));
    }
}

impl StoryWriter<'_, '_> {
    /// The run content of a drawing (`w:drawing`, `mc:AlternateContent` or `w:pict`), or `None`
    /// (with an issue) when it cannot be written.
    pub(crate) fn drawing(&mut self, d: &Drawing) -> Option<String> {
        let Some(xml) = d.xml.as_deref() else {
            return self.new_drawing(d);
        };
        let Ok(mut root) = parse_fragment(xml) else {
            return Some(xml.to_owned());
        };
        match root.name.as_str() {
            "w:pict" => self.vml(d, xml, &mut root),
            "w:drawing" | "mc:AlternateContent" => self.kept_drawing(d, xml, &mut root),
            _ => Some(self.with_text_box(d, xml, &mut root)),
        }
    }

    /// Writes the text box content (if any) into the kept XML.
    fn with_text_box(&mut self, d: &Drawing, xml: &str, root: &mut XmlElement) -> String {
        let Some(blocks) = &d.text_box else {
            return xml.to_owned();
        };
        if mark_text_boxes(root) == 0 {
            return xml.to_owned();
        }
        let content = self.nested_story(blocks);
        frag(root).replace(TXBX_PLACEHOLDER, &content)
    }

    fn derive(&self, drawing: &XmlElement) -> Option<Derived> {
        let container = drawing
            .elements()
            .next()
            .filter(|c| c.name == "wp:inline" || c.name == "wp:anchor")?;
        let extent = container.child("wp:extent");
        let doc_pr = container.child("wp:docPr");
        let placement = if container.name == "wp:anchor" {
            Placement::Anchor(Box::new(derive_anchor(container)))
        } else {
            Placement::Inline
        };
        let graphic = container.child_path(&["a:graphic", "a:graphicData"]);
        let uri = graphic.and_then(|g| g.attr("uri")).unwrap_or_default();
        let original = self.ctx.original_rels_of(&self.part);
        let target = |id: Option<&str>| {
            id.and_then(|id| original.and_then(|o| o.get(id)))
                .filter(|t| !t.external)
                .map(|t| t.target.clone())
        };
        let mut xf = (0, false, false);
        let kind = match uri {
            URI_PICTURE => {
                let pic = graphic.and_then(|g| g.child("pic:pic"));
                let blip_fill = pic.and_then(|p| p.child("pic:blipFill"));
                let blip = blip_fill.and_then(|b| b.child("a:blip"));
                xf = xfrm_values(pic.and_then(|p| p.child_path(&["pic:spPr", "a:xfrm"])));
                match target(blip.and_then(|b| b.attr("r:embed")))
                    .filter(|t| self.ctx.doc.media.contains_key(t))
                {
                    Some(media) => DrawingKind::Picture {
                        media,
                        crop: derive_crop(blip_fill.and_then(|b| b.child("a:srcRect"))),
                    },
                    None => DrawingKind::Other,
                }
            }
            URI_SHAPE => {
                let sp_pr = graphic.and_then(|g| g.child_path(&["wps:wsp", "wps:spPr"]));
                xf = xfrm_values(sp_pr.and_then(|s| s.child("a:xfrm")));
                let preset = match sp_pr {
                    Some(sp) if sp.child("a:custGeom").is_some() => "custom".to_owned(),
                    Some(sp) => sp
                        .child("a:prstGeom")
                        .and_then(|g| g.attr("prst"))
                        .unwrap_or("rect")
                        .to_owned(),
                    None => "rect".to_owned(),
                };
                DrawingKind::Shape { preset }
            }
            URI_CHART | URI_CHARTEX => DrawingKind::Chart {
                part: target(
                    graphic
                        .and_then(XmlElement::first_element)
                        .and_then(|r| r.attr("r:id")),
                )
                .unwrap_or_default(),
            },
            URI_GROUP => DrawingKind::Group,
            _ => DrawingKind::Other,
        };
        let attr = |k: &str| doc_pr.and_then(|d| d.attr(k)).map(str::to_owned);
        Some(Derived {
            kind,
            width: parse_emu(extent.and_then(|e| e.attr("cx")))
                .unwrap_or(0)
                .max(0),
            height: parse_emu(extent.and_then(|e| e.attr("cy")))
                .unwrap_or(0)
                .max(0),
            placement,
            name: attr("name").unwrap_or_default(),
            description: attr("descr"),
            title: attr("title"),
            decorative: is_decorative(doc_pr),
            rotation: xf.0,
            flip_h: xf.1,
            flip_v: xf.2,
        })
    }

    /// A kept DrawingML drawing: verbatim when unchanged, patched otherwise.
    fn kept_drawing(&mut self, d: &Drawing, xml: &str, root: &mut XmlElement) -> Option<String> {
        let derived = find_drawing(root).and_then(|dr| self.derive(dr));
        let Some(derived) = derived else {
            return Some(self.with_text_box(d, xml, root));
        };
        if derived == view(d) {
            return Some(self.with_text_box(d, xml, root));
        }
        // Changed: patch the chosen w:drawing and drop a stale fallback.
        let mut drawing = find_drawing(root)?.clone();
        self.patch_drawing(d, &derived, &mut drawing);
        Some(self.with_text_box(d, &frag(&drawing), &mut drawing))
    }

    fn patch_drawing(&mut self, d: &Drawing, derived: &Derived, drawing: &mut XmlElement) {
        let Some(container) = drawing
            .children
            .iter_mut()
            .filter_map(XmlNode::as_element_mut)
            .find(|c| c.name == "wp:inline" || c.name == "wp:anchor")
        else {
            return;
        };
        if let Some(e) = container.child_mut("wp:extent") {
            e.set_attr("cx", Some(d.width));
            e.set_attr("cy", Some(d.height));
        }
        // The graphic: size, rotation, flips, picture and geometry.
        let graphic_data = child_mut(container, &["a:graphic", "a:graphicData"]);
        if let Some(gd) = graphic_data {
            if let Some(pic) = gd.child_mut("pic:pic") {
                if let Some(xfrm) = child_mut(pic, &["pic:spPr", "a:xfrm"]) {
                    patch_xfrm(xfrm, d);
                }
                if let DrawingKind::Picture { media, crop } = &d.kind {
                    let part = self.part.clone();
                    let rel = self.ctx.image_rel(&part, media);
                    if let Some(bf) = pic.child_mut("pic:blipFill") {
                        if let Some(blip) = bf.child_mut("a:blip") {
                            blip.set_attr("r:embed", rel.as_deref());
                        }
                        let derived_crop = match &derived.kind {
                            DrawingKind::Picture { crop, .. } => *crop,
                            _ => None,
                        };
                        if derived_crop != *crop {
                            bf.remove_children("a:srcRect");
                            if let Some(c) = crop
                                && let Ok(el) = parse_fragment(&crop_xml(Some(c)))
                            {
                                bf.upsert_child(el, &["a:tile", "a:stretch"]);
                            }
                        }
                    }
                }
            }
            if let Some(sp_pr) = child_mut(gd, &["wps:wsp", "wps:spPr"]) {
                if let Some(xfrm) = sp_pr.child_mut("a:xfrm") {
                    patch_xfrm(xfrm, d);
                }
                if let DrawingKind::Shape { preset } = &d.kind
                    && preset != "custom"
                {
                    if let Some(g) = sp_pr.child_mut("a:prstGeom") {
                        g.set_attr("prst", Some(preset.as_str()));
                    } else if sp_pr.child("a:custGeom").is_some()
                        && let Ok(el) = parse_fragment(&format!(
                            "<a:prstGeom prst=\"{preset}\"><a:avLst/></a:prstGeom>"
                        ))
                    {
                        let at = sp_pr
                            .children
                            .iter()
                            .position(|c| c.as_element().is_some_and(|e| e.name == "a:custGeom"));
                        if let Some(at) = at {
                            sp_pr.children[at] = XmlNode::Element(el);
                        }
                    }
                }
            }
        }
        // docPr.
        let kept_doc_pr = container.child("wp:docPr").cloned();
        let doc_pr_id = kept_doc_pr
            .as_ref()
            .and_then(|p| p.attr("id"))
            .and_then(|i| i.parse().ok())
            .unwrap_or(1);
        let doc_pr = doc_pr_xml(d, doc_pr_id, kept_doc_pr.as_ref());
        if let (Ok(el), Some(slot)) = (
            parse_fragment(&doc_pr),
            container
                .children
                .iter_mut()
                .find(|c| c.as_element().is_some_and(|e| e.name == "wp:docPr")),
        ) {
            *slot = XmlNode::Element(el);
        }
        // Placement.
        if derived.placement == d.placement {
            return;
        }
        let keep = |name: &str| container.child(name).map(frag);
        let kept = Kept {
            effect_extent: keep("wp:effectExtent"),
            doc_pr: keep("wp:docPr").unwrap_or(doc_pr),
            frame_pr: keep("wp:cNvGraphicFramePr"),
            graphic: keep("a:graphic").unwrap_or_default(),
            trailing: if container.name == "wp:anchor" {
                container
                    .elements()
                    .filter(|c| c.name.starts_with("wp14:"))
                    .map(frag)
                    .collect()
            } else {
                String::new()
            },
        };
        let mut placement = d.placement.clone();
        // Keep the original polygon when the wrap style is unchanged and the model has none.
        if let (Placement::Anchor(new), Placement::Anchor(old)) =
            (&mut placement, &derived.placement)
            && new.wrap == old.wrap
            && new.wrap_polygon_xml.is_none()
        {
            new.wrap_polygon_xml.clone_from(&old.wrap_polygon_xml);
        }
        let rebuilt = container_xml(
            &Drawing {
                placement,
                ..d.clone()
            },
            &kept,
        );
        if let Ok(el) = parse_fragment(&rebuilt) {
            *container = el;
        }
    }

    /// A legacy VML drawing (`w:pict`): verbatim (with its text box written from the model) unless
    /// a plain picture changed, which is then written as DrawingML.
    fn vml(&mut self, d: &Drawing, xml: &str, root: &mut XmlElement) -> Option<String> {
        if let DrawingKind::Picture { media, .. } = &d.kind {
            let shapes: Vec<&XmlElement> = root
                .elements()
                .filter(|c| c.name.starts_with("v:") && c.name != "v:shapetype")
                .collect();
            let shape = (shapes.len() == 1).then(|| shapes[0]);
            let imagedata = shape.and_then(|s| s.child("v:imagedata"));
            let rel = imagedata.and_then(|i| i.attr("r:id").or_else(|| i.attr("r:pict")));
            let original = self.ctx.original_rels_of(&self.part);
            let target = rel
                .and_then(|r| original.and_then(|o| o.get(r)))
                .map(|t| t.target.clone());
            let style = vml_style(shape.and_then(|s| s.attr("style")));
            let width = vml_length(style_get(&style, "width")).unwrap_or(0).max(0);
            let height = vml_length(style_get(&style, "height")).unwrap_or(0).max(0);
            let title = imagedata.and_then(|i| i.attr("o:title")).map(str::to_owned);
            let description = shape
                .and_then(|s| s.attr("alt"))
                .map(str::to_owned)
                .or_else(|| title.clone());
            let unchanged = target.as_deref() == Some(media.as_str())
                && width == d.width
                && height == d.height
                && title == d.title
                && description == d.description
                && matches!(d.placement, Placement::Inline)
                && d.name.is_empty()
                && !d.decorative
                && d.rotation == 0
                && !d.flip_h
                && !d.flip_v;
            if unchanged {
                return Some(xml.to_owned());
            }
            return self.new_drawing(&Drawing {
                xml: None,
                ..d.clone()
            });
        }
        Some(self.with_text_box(d, xml, root))
    }

    /// A drawing created in the editor.
    fn new_drawing(&mut self, d: &Drawing) -> Option<String> {
        let id = self.ctx.next_doc_pr();
        let part = self.part.clone();
        let graphic = match &d.kind {
            DrawingKind::Picture { media, crop } => {
                let Some(rel) = self.ctx.image_rel(&part, media) else {
                    self.ctx.issue(
                        "image.missing",
                        "A picture whose image data is not available was not saved.",
                        &part,
                    );
                    return None;
                };
                format!(
                    "<a:graphic xmlns:a=\"{NS_A}\"><a:graphicData uri=\"{URI_PICTURE}\"><pic:pic xmlns:pic=\"{NS_PIC}\"><pic:nvPicPr>{}<pic:cNvPicPr><a:picLocks noChangeAspect=\"1\" noChangeArrowheads=\"1\"/></pic:cNvPicPr></pic:nvPicPr><pic:blipFill>{}{}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr bwMode=\"auto\">{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>",
                    El::new("pic:cNvPr")
                        .attr("id", id)
                        .attr(
                            "name",
                            if d.name.is_empty() {
                                format!("Picture {id}")
                            } else {
                                d.name.clone()
                            }
                        )
                        .empty(),
                    El::new("a:blip").attr("r:embed", rel).empty(),
                    crop_xml(crop.as_ref()),
                    xfrm_xml(d.rotation, d.flip_h, d.flip_v, d.width, d.height),
                )
            }
            DrawingKind::Shape { preset } => {
                let preset = if preset == "custom" { "rect" } else { preset };
                let text_box = match &d.text_box {
                    Some(blocks) => format!(
                        "<wps:txbx><w:txbxContent>{}</w:txbxContent></wps:txbx>",
                        self.nested_story(blocks)
                    ),
                    None => String::new(),
                };
                let cnv = if d.text_box.is_some() {
                    "<wps:cNvSpPr txBox=\"1\"/>"
                } else {
                    "<wps:cNvSpPr/>"
                };
                format!(
                    "<a:graphic xmlns:a=\"{NS_A}\"><a:graphicData uri=\"{URI_SHAPE}\"><wps:wsp>{cnv}<wps:spPr>{}<a:prstGeom prst=\"{}\"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"50000\"/></a:schemeClr></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></wps:style>{text_box}<wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" anchor=\"t\" anchorCtr=\"0\"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic>",
                    xfrm_xml(d.rotation, d.flip_h, d.flip_v, d.width, d.height),
                    super::xml::text(preset),
                )
            }
            DrawingKind::Chart { part: chart } if !chart.is_empty() && self.ctx.pkg.has(chart) => {
                let rel = self.ctx.rel(&part, rel_type::CHART, chart, false);
                format!(
                    "<a:graphic xmlns:a=\"{NS_A}\"><a:graphicData uri=\"{URI_CHART}\"><c:chart xmlns:c=\"{NS_C}\" r:id=\"{rel}\"/></a:graphicData></a:graphic>"
                )
            }
            _ => {
                self.ctx.issue(
                    "drawing.unsaved",
                    "A drawing without its original markup could not be saved.",
                    &part,
                );
                return None;
            }
        };
        let mut placement = d.placement.clone();
        if let Placement::Anchor(a) = &mut placement
            && a.z_order == 0
        {
            a.z_order = Z_BASE + i64::try_from(id).unwrap_or(0);
        }
        let kept = Kept {
            effect_extent: None,
            doc_pr: doc_pr_xml(d, id, None),
            frame_pr: Some(match d.kind {
                DrawingKind::Picture { .. } => format!(
                    "<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a=\"{NS_A}\" noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>"
                ),
                _ => "<wp:cNvGraphicFramePr/>".to_owned(),
            }),
            graphic,
            trailing: String::new(),
        };
        Some(format!(
            "<w:drawing>{}</w:drawing>",
            container_xml(
                &Drawing {
                    placement,
                    ..d.clone()
                },
                &kept
            )
        ))
    }
}

/// A VML length (`12pt`, `1.5in`, `96px`, `2cm`) in EMU (the reader's conversion).
fn vml_length(v: Option<&str>) -> Option<i64> {
    let s = v?.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().ok()?;
    let per = match unit.trim() {
        "pt" => 12_700.0,
        "in" => 914_400.0,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "pc" => 152_400.0,
        "emu" => 1.0,
        "" | "px" => 9_525.0,
        _ => return None,
    };
    let emu = (n * per).round();
    if !emu.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(emu as i64)
}

fn vml_style(style: Option<&str>) -> Vec<(String, String)> {
    style
        .unwrap_or_default()
        .split(';')
        .filter_map(|decl| {
            let (k, v) = decl.split_once(':')?;
            Some((k.trim().to_lowercase(), v.trim().to_owned()))
        })
        .collect()
}

fn style_get<'s>(style: &'s [(String, String)], key: &str) -> Option<&'s str> {
    style
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The largest `wp:docPr` id in kept drawing XML (new drawings are numbered above it).
pub(crate) fn max_doc_pr_id(xml: &str) -> u64 {
    let mut max = 0u64;
    let mut rest = xml;
    while let Some(i) = rest.find("<wp:docPr") {
        rest = &rest[i + 9..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let Some(j) = tag.find(" id=\"") {
            let v = &tag[j + 5..];
            if let Some(k) = v.find('"')
                && let Ok(n) = v[..k].parse::<u64>()
            {
                max = max.max(n);
            }
        }
        rest = &rest[end..];
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_derive_like_the_reader() {
        let el = parse_fragment(
            r#"<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="7" behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="1" cy="1"/><wp:wrapNone/><wp:docPr id="1" name="x"/></wp:anchor>"#,
        )
        .unwrap();
        let a = derive_anchor(&el);
        assert_eq!(a.wrap, WrapType::Behind);
        assert_eq!(a.h, PosH::Align(AlignH::Center));
        assert_eq!(a.v, PosV::Offset(914_400));
        assert_eq!(a.relative_h, RelativeFromH::Page);
        assert_eq!(a.dist_left, 114_300);
        assert_eq!(a.z_order, 7);
        let k = Kept {
            effect_extent: None,
            doc_pr: "<wp:docPr id=\"1\" name=\"x\"/>".into(),
            frame_pr: None,
            graphic: String::new(),
            trailing: String::new(),
        };
        let rebuilt = parse_fragment(&anchor_xml(&a, 1, 1, &k)).unwrap();
        assert_eq!(derive_anchor(&rebuilt), a);
    }

    #[test]
    fn every_wrap_round_trips() {
        for wrap in [
            WrapType::Square,
            WrapType::Tight,
            WrapType::Through,
            WrapType::TopAndBottom,
            WrapType::Behind,
            WrapType::InFront,
        ] {
            let a = Anchor {
                wrap,
                wrap_side: matches!(wrap, WrapType::Square | WrapType::Tight | WrapType::Through)
                    .then_some(WrapSide::Left),
                relative_h: RelativeFromH::Margin,
                h: PosH::Offset(-5),
                relative_v: RelativeFromV::Line,
                v: PosV::Align(AlignV::Bottom),
                dist_top: 1,
                dist_bottom: 2,
                dist_left: 3,
                dist_right: 4,
                allow_overlap: false,
                layout_in_cell: false,
                locked: true,
                z_order: 99,
                wrap_polygon_xml: matches!(wrap, WrapType::Tight | WrapType::Through)
                    .then(|| DEFAULT_POLYGON.to_owned()),
            };
            let k = Kept {
                effect_extent: None,
                doc_pr: "<wp:docPr id=\"1\" name=\"x\"/>".into(),
                frame_pr: None,
                graphic: String::new(),
                trailing: String::new(),
            };
            let el = parse_fragment(&anchor_xml(&a, 10, 20, &k)).unwrap();
            assert_eq!(derive_anchor(&el), a, "{wrap}");
        }
    }

    #[test]
    fn doc_pr_ids_are_found() {
        assert_eq!(
            max_doc_pr_id(r#"<wp:docPr id="12" name="a"/><x/><wp:docPr name="b" id="40"/>"#),
            40
        );
        assert_eq!(max_doc_pr_id("<w:p/>"), 0);
    }

    #[test]
    fn vml_lengths_match_the_reader() {
        assert_eq!(vml_length(Some("72pt")), Some(914_400));
        assert_eq!(vml_length(Some("96")), Some(914_400));
        assert_eq!(vml_length(Some("1em")), None);
    }
}
