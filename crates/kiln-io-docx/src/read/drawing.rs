// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawings: DrawingML (`w:drawing` with `wp:inline`/`wp:anchor`), legacy VML (`w:pict`, read
//! only) and embedded objects (`w:object`).
//!
//! - `pic:pic` → a picture [`Drawing`] (the image part goes to the document's media);
//! - `wps:wsp` (shapes and text boxes) → a shape [`Drawing`] whose text box content is read as
//!   blocks; fill, line, effects and custom geometry stay in the kept XML;
//! - charts → a chart [`Drawing`] plus a [`ChartRef`] (the chart part stays passthrough);
//! - groups, SmartArt, canvases and ink → [`Drawing`]s of kind group/other that keep their XML;
//! - VML: a plain picture becomes a picture, a text box a drawing with its text, anything else an
//!   opaque object; OLE objects are opaque objects. Everything is reported to the
//!   Compatibility Checker.

use kiln_doc::settings::Media;
use kiln_doc::st::{AlignH, AlignV, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{Anchor, Crop, Drawing, DrawingKind, InlineObject, Placement, PosH, PosV};
use kiln_ooxml::xml::XmlElement;
use kiln_ooxml::xml::namespaces::local_name;

use super::context::ChartRef;
use super::story::StoryReader;
use crate::common::units::{parse_decimal, parse_emu, parse_on_off};
use crate::common::warnings::{Severity, WarningCode};

const URI_PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const URI_SHAPE: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const URI_GROUP: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const URI_CANVAS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas";
const URI_INK: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingInk";
const URI_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const URI_CHARTEX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
const URI_DIAGRAM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const URI_LOCKED_CANVAS: &str = "http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas";

/// Records every relationship id referenced from a kept XML fragment, so the writer can map it.
fn record_relationship_refs(story: &mut StoryReader<'_, '_>, el: &XmlElement) {
    let mut ids = Vec::new();
    let mut stack = vec![el];
    while let Some(n) = stack.pop() {
        for (k, v) in n.attrs.iter() {
            if k.starts_with("r:") && !v.is_empty() {
                ids.push(v.to_owned());
            }
        }
        stack.extend(n.elements());
    }
    let part = story.part.clone();
    for id in ids {
        let _ = story.ctx.relationship(&part, &id);
    }
}

fn parse_h(story: &mut StoryReader<'_, '_>, node: Option<&XmlElement>) -> PosH {
    let Some(node) = node else {
        return PosH::Offset(0);
    };
    if let Some(a) = node
        .child("wp:align")
        .and_then(|a| AlignH::parse(a.text().trim()))
    {
        return PosH::Align(a);
    }
    if let Some(off) = node.child("wp:posOffset") {
        return PosH::Offset(parse_emu(Some(&off.text())).unwrap_or(0));
    }
    if node
        .elements()
        .any(|c| local_name(&c.name).starts_with("pctPos"))
    {
        story.warn(
            WarningCode::DrawingUnsupported,
            Severity::Warning,
            "A position given as a percentage of the page was approximated as an offset of zero.",
        );
    }
    PosH::Offset(0)
}

fn parse_v(story: &mut StoryReader<'_, '_>, node: Option<&XmlElement>) -> PosV {
    let Some(node) = node else {
        return PosV::Offset(0);
    };
    if let Some(a) = node
        .child("wp:align")
        .and_then(|a| AlignV::parse(a.text().trim()))
    {
        return PosV::Align(a);
    }
    if let Some(off) = node.child("wp:posOffset") {
        return PosV::Offset(parse_emu(Some(&off.text())).unwrap_or(0));
    }
    if node
        .elements()
        .any(|c| local_name(&c.name).starts_with("pctPos"))
    {
        story.warn(
            WarningCode::DrawingUnsupported,
            Severity::Warning,
            "A position given as a percentage of the page was approximated as an offset of zero.",
        );
    }
    PosV::Offset(0)
}

fn parse_anchor(story: &mut StoryReader<'_, '_>, anchor: &XmlElement) -> Anchor {
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
        wrap_polygon_xml = c.child("wp:wrapPolygon").map(|p| story.raw_xml(p));
        break;
    }
    let pos_h = anchor.child("wp:positionH");
    let pos_v = anchor.child("wp:positionV");
    let mut relative_h = pos_h
        .and_then(|p| p.attr("relativeFrom"))
        .and_then(RelativeFromH::parse)
        .unwrap_or_default();
    let mut relative_v = pos_v
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
        (parse_h(story, pos_h), parse_v(story, pos_v))
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

struct Transform {
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
}

fn transform(xfrm: Option<&XmlElement>) -> Transform {
    let rot = parse_decimal(xfrm.and_then(|x| x.attr("rot"))).unwrap_or(0);
    Transform {
        rotation: i32::try_from(rot.rem_euclid(21_600_000)).unwrap_or(0),
        flip_h: parse_on_off(xfrm.and_then(|x| x.attr("flipH")), false),
        flip_v: parse_on_off(xfrm.and_then(|x| x.attr("flipV")), false),
    }
}

/// Loads an image part into the document's media; returns its part name.
fn load_media(story: &mut StoryReader<'_, '_>, rel_id: &str) -> Option<String> {
    let part = story.part.clone();
    let rel = story.ctx.relationship(&part, rel_id)?;
    if rel.external {
        return None;
    }
    let data = story.ctx.pkg.part(&rel.target).ok()??.into_owned();
    let content_type = story
        .ctx
        .pkg
        .content_type(&rel.target)
        .unwrap_or("application/octet-stream")
        .to_owned();
    story
        .ctx
        .doc
        .media
        .insert(rel.target.clone(), Media { content_type, data });
    story.ctx.consume(&rel.target);
    Some(rel.target)
}

fn crop(src: Option<&XmlElement>) -> Option<Crop> {
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

/// What a drawing shows, and its text box, from its `a:graphicData`.
fn graphic_kind(
    story: &mut StoryReader<'_, '_>,
    drawing: &XmlElement,
    graphic: Option<&XmlElement>,
) -> (DrawingKind, Option<Vec<kiln_doc::Block>>, Transform) {
    let uri = graphic.and_then(|g| g.attr("uri")).unwrap_or_default();
    let none = || Transform {
        rotation: 0,
        flip_h: false,
        flip_v: false,
    };
    match uri {
        URI_PICTURE => {
            let pic = graphic.and_then(|g| g.child("pic:pic"));
            let blip_fill = pic.and_then(|p| p.child("pic:blipFill"));
            let blip = blip_fill.and_then(|b| b.child("a:blip"));
            let xf = transform(pic.and_then(|p| p.child_path(&["pic:spPr", "a:xfrm"])));
            if blip.is_some_and(|b| {
                b.descendants("a:ext")
                    .iter()
                    .any(|e| e.elements().any(|c| local_name(&c.name) == "svgBlip"))
            }) {
                story.warn(
                    WarningCode::DrawingUnsupported,
                    Severity::Warning,
                    "An SVG picture is shown and saved using its PNG fallback.",
                );
            }
            let embed = blip.and_then(|b| b.attr("r:embed"));
            let link = blip.and_then(|b| b.attr("r:link"));
            if let Some(embed) = embed {
                if let Some(media) = load_media(story, embed) {
                    let kind = DrawingKind::Picture {
                        media,
                        crop: crop(blip_fill.and_then(|b| b.child("a:srcRect"))),
                    };
                    return (kind, None, xf);
                }
                story.warn(
                    WarningCode::ImageMissing,
                    Severity::Warning,
                    "A picture refers to an image that is missing from the file.",
                );
            } else if let Some(link) = link {
                let part = story.part.clone();
                let _ = story.ctx.relationship(&part, link);
                story.warn(
                    WarningCode::ImageLinked,
                    Severity::Warning,
                    "A picture linked to an external file is not loaded; save the picture into the document to keep it.",
                );
            }
            record_relationship_refs(story, drawing);
            (DrawingKind::Other, None, xf)
        }
        URI_SHAPE => {
            record_relationship_refs(story, drawing);
            let wsp = graphic.and_then(|g| g.child("wps:wsp"));
            let sp_pr = wsp.and_then(|w| w.child("wps:spPr"));
            let preset = match sp_pr {
                Some(sp) if sp.child("a:custGeom").is_some() => "custom".to_owned(),
                Some(sp) => sp
                    .child("a:prstGeom")
                    .and_then(|g| g.attr("prst"))
                    .unwrap_or("rect")
                    .to_owned(),
                None => "rect".to_owned(),
            };
            let xf = transform(sp_pr.and_then(|s| s.child("a:xfrm")));
            let text_box = wsp
                .and_then(|w| w.child_path(&["wps:txbx", "w:txbxContent"]))
                .and_then(|content| {
                    story.path.push("w:txbxContent[1]".to_owned());
                    let blocks = story.read_nested_story(content);
                    story.path.pop();
                    blocks
                });
            (DrawingKind::Shape { preset }, text_box, xf)
        }
        URI_CHART | URI_CHARTEX => {
            record_relationship_refs(story, drawing);
            let part = graphic
                .and_then(XmlElement::first_element)
                .and_then(|r| r.attr("r:id"))
                .and_then(|id| {
                    let source = story.part.clone();
                    story.ctx.relationship(&source, id)
                })
                .filter(|r| !r.external)
                .map(|r| r.target)
                .unwrap_or_default();
            (DrawingKind::Chart { part }, None, none())
        }
        URI_DIAGRAM => {
            story.warn(
                WarningCode::DrawingSmartArt,
                Severity::Info,
                "SmartArt is kept in the file but shown as a placeholder.",
            );
            record_relationship_refs(story, drawing);
            (DrawingKind::Other, None, none())
        }
        URI_GROUP => {
            story.warn(
                WarningCode::DrawingGroup,
                Severity::Info,
                "A group of shapes is kept in the file but shown as a placeholder.",
            );
            record_relationship_refs(story, drawing);
            (DrawingKind::Group, None, none())
        }
        URI_CANVAS | URI_LOCKED_CANVAS => {
            story.warn(
                WarningCode::DrawingCanvas,
                Severity::Info,
                "A drawing canvas is kept in the file but shown as a placeholder.",
            );
            record_relationship_refs(story, drawing);
            (DrawingKind::Other, None, none())
        }
        URI_INK => {
            story.warn(
                WarningCode::DrawingInk,
                Severity::Info,
                "Ink is kept in the file but cannot be edited.",
            );
            record_relationship_refs(story, drawing);
            (DrawingKind::Other, None, none())
        }
        other => {
            let msg = format!(
                "A drawing of type {} is kept but shown as a placeholder.",
                if other.is_empty() { "unknown" } else { other }
            );
            story.warn(WarningCode::DrawingUnsupported, Severity::Info, &msg);
            record_relationship_refs(story, drawing);
            (DrawingKind::Other, None, none())
        }
    }
}

fn is_decorative(doc_pr: Option<&XmlElement>) -> bool {
    doc_pr.is_some_and(|d| {
        d.descendants("a:ext").iter().any(|e| {
            e.elements()
                .any(|c| local_name(&c.name) == "decorative" && parse_on_off(c.attr("val"), false))
        })
    })
}

/// `w:drawing` → a drawing object.
pub(crate) fn read_drawing(
    story: &mut StoryReader<'_, '_>,
    drawing: &XmlElement,
) -> Option<InlineObject> {
    let Some(container) = drawing
        .first_element()
        .filter(|c| c.name == "wp:inline" || c.name == "wp:anchor")
    else {
        story.warn(
            WarningCode::DrawingUnsupported,
            Severity::Warning,
            "An empty or unknown drawing was skipped.",
        );
        return None;
    };
    let extent = container.child("wp:extent");
    let doc_pr = container.child("wp:docPr");
    let placement = if container.name == "wp:anchor" {
        Placement::Anchor(Box::new(parse_anchor(story, container)))
    } else {
        Placement::Inline
    };
    let graphic = container.child_path(&["a:graphic", "a:graphicData"]);
    let (kind, text_box, xf) = graphic_kind(story, drawing, graphic);
    let xml = story
        .alternate_xml()
        .map_or_else(|| story.raw_xml(drawing), str::to_owned);
    let id = story.ctx.doc.alloc_id();
    let width = parse_emu(extent.and_then(|e| e.attr("cx")))
        .unwrap_or(0)
        .max(0);
    let height = parse_emu(extent.and_then(|e| e.attr("cy")))
        .unwrap_or(0)
        .max(0);
    if let DrawingKind::Chart { part } = &kind {
        let rel_id = graphic
            .and_then(XmlElement::first_element)
            .and_then(|r| r.attr("r:id"))
            .unwrap_or_default()
            .to_owned();
        story.ctx.charts.push(ChartRef {
            source_part: story.part.clone(),
            rel_id,
            chart_part: (!part.is_empty()).then(|| part.clone()),
            drawing: id,
            width,
            height,
            anchored: matches!(placement, Placement::Anchor(_)),
        });
    }
    let attr = |k: &str| doc_pr.and_then(|d| d.attr(k)).map(str::to_owned);
    Some(InlineObject::Drawing(Box::new(Drawing {
        id,
        kind,
        width,
        height,
        placement,
        name: attr("name").unwrap_or_default(),
        description: attr("descr"),
        title: attr("title"),
        decorative: is_decorative(doc_pr),
        rotation: xf.rotation,
        flip_h: xf.flip_h,
        flip_v: xf.flip_v,
        text_box,
        xml: Some(xml),
    })))
}

/// A VML length (`12pt`, `1.5in`, `96px`, `2cm`) in EMU.
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

fn style_get<'a>(style: &'a [(String, String)], key: &str) -> Option<&'a str> {
    style
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn plain_drawing(id: kiln_doc::NodeId, kind: DrawingKind, width: i64, height: i64) -> Drawing {
    Drawing {
        id,
        kind,
        width,
        height,
        placement: Placement::Inline,
        name: String::new(),
        description: None,
        title: None,
        decorative: false,
        rotation: 0,
        flip_h: false,
        flip_v: false,
        text_box: None,
        xml: None,
    }
}

/// `w:pict` (VML). Plain pictures are read as pictures and text boxes keep their text; anything
/// else is preserved as an opaque object.
pub(crate) fn read_pict(
    story: &mut StoryReader<'_, '_>,
    pict: &XmlElement,
) -> Option<InlineObject> {
    let shapes: Vec<&XmlElement> = pict
        .elements()
        .filter(|c| c.name.starts_with("v:") && c.name != "v:shapetype")
        .collect();
    let shape = (shapes.len() == 1).then(|| shapes[0]);
    let imagedata = shape.and_then(|s| s.child("v:imagedata"));
    let text_box = pict.descendants("w:txbxContent").into_iter().next();
    let rel_id = imagedata.and_then(|i| i.attr("r:id").or_else(|| i.attr("r:pict")));
    let xml = story
        .alternate_xml()
        .map_or_else(|| story.raw_xml(pict), str::to_owned);
    let style = vml_style(shape.and_then(|s| s.attr("style")));
    let width = vml_length(style_get(&style, "width")).unwrap_or(0).max(0);
    let height = vml_length(style_get(&style, "height")).unwrap_or(0).max(0);

    if let (Some(shape), Some(imagedata), Some(rel_id), None) = (shape, imagedata, rel_id, text_box)
    {
        if style_get(&style, "position") == Some("absolute") {
            story.warn(
                WarningCode::DrawingVml,
                Severity::Warning,
                "A legacy (VML) floating picture is shown in line with the text.",
            );
        }
        let kind = match load_media(story, rel_id) {
            Some(media) => DrawingKind::Picture { media, crop: None },
            None => {
                story.warn(
                    WarningCode::ImageMissing,
                    Severity::Warning,
                    "A picture refers to an image that is missing from the file.",
                );
                DrawingKind::Other
            }
        };
        let title = imagedata.attr("o:title").map(str::to_owned);
        let mut d = plain_drawing(story.ctx.doc.alloc_id(), kind, width, height);
        d.description = shape
            .attr("alt")
            .map(str::to_owned)
            .or_else(|| title.clone());
        d.title = title;
        d.xml = Some(xml);
        return Some(InlineObject::Drawing(Box::new(d)));
    }

    story.warn(
        WarningCode::DrawingVml,
        Severity::Info,
        "A legacy (VML) drawing is kept in the file but shown as a placeholder.",
    );
    record_relationship_refs(story, pict);
    if let Some(content) = text_box {
        story.path.push("w:txbxContent[1]".to_owned());
        let blocks = story.read_nested_story(content);
        story.path.pop();
        let mut d = plain_drawing(story.ctx.doc.alloc_id(), DrawingKind::Other, width, height);
        d.text_box = blocks;
        d.xml = Some(xml);
        return Some(InlineObject::Drawing(Box::new(d)));
    }
    Some(InlineObject::Opaque {
        xml,
        label: "Drawing".to_owned(),
    })
}

/// `w:object` (an embedded or linked OLE object).
pub(crate) fn read_object(story: &mut StoryReader<'_, '_>, obj: &XmlElement) -> InlineObject {
    let prog = obj
        .descendants("o:OLEObject")
        .first()
        .and_then(|o| o.attr("ProgID"))
        .map(str::to_owned);
    let msg = format!(
        "An embedded object{} is kept in the file but shown as a placeholder.",
        prog.as_deref()
            .map(|p| format!(" ({p})"))
            .unwrap_or_default()
    );
    story.warn(WarningCode::ObjectEmbedded, Severity::Info, &msg);
    record_relationship_refs(story, obj);
    InlineObject::Opaque {
        xml: story
            .alternate_xml()
            .map_or_else(|| story.raw_xml(obj), str::to_owned),
        label: prog.map_or_else(|| "Embedded object".to_owned(), |p| format!("Embedded {p}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vml_lengths_in_every_unit() {
        assert_eq!(vml_length(Some("72pt")), Some(914_400));
        assert_eq!(vml_length(Some("0.5in")), Some(457_200));
        assert_eq!(vml_length(Some("96")), Some(914_400));
        assert_eq!(vml_length(Some("2cm")), Some(720_000));
        assert_eq!(vml_length(Some("1em")), None);
        assert_eq!(vml_length(Some("pt")), None);
        assert_eq!(vml_length(None), None);
    }

    #[test]
    fn vml_styles_split_into_declarations() {
        let s = vml_style(Some("Width:72pt; height:1in;position:absolute"));
        assert_eq!(style_get(&s, "width"), Some("72pt"));
        assert_eq!(style_get(&s, "position"), Some("absolute"));
        assert_eq!(style_get(&s, "left"), None);
    }
}
