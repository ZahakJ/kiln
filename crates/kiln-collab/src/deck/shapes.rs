// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slides, shapes and text bodies ↔ Loro.
//!
//! A slide is the metadata map of a tree node: small values as JSON keys, comments and shapes
//! as movable lists. A shape map holds its identity, placement, placeholder and kind as JSON
//! (the kind without its text and children), its text body (when it has one) as a map whose
//! paragraphs use the Write mapping, and a group's children as a movable list of shape maps.

use kiln_common::NodeId;
use kiln_deck::ops::ShapeInfo;
use kiln_deck::{Comment, DeckParaProps, Notes, Shape, ShapeKind, Slide, TextBody};
use kiln_doc::Paragraph;
use loro::LoroMap;
use serde_json::{Value, json};

use crate::error::{Result, corrupt};
use crate::value::{
    child_list, child_map, find_list, find_map, from_json, get_i64, get_id, get_json,
    get_json_or_default, get_str, id_value, map_at, maps_of, put_i64, put_json, put_str,
};
use crate::write::blocks::{
    ID, K, Nodes, place, read_list, read_paragraph, remove_missing, rewrite, write_paragraph,
};

/// Node kinds of the Deck mapping.
pub(crate) const SLIDE: &str = "slide";
pub(crate) const SECTION: &str = "section";
pub(crate) const SHAPE: &str = "shape";
const SHAPES: &str = "shapes";
pub(crate) const PARAS: &str = "paras";

/// A shape's kind as stored: without its text body and group children (they have containers
/// of their own).
fn bare_kind(kind: &ShapeKind) -> ShapeKind {
    let mut k = kind.clone();
    match &mut k {
        ShapeKind::Geometry(a) => a.text = None,
        ShapeKind::Group(g) => g.children.clear(),
        _ => {}
    }
    k
}

fn text_of(s: &Shape) -> Option<&TextBody> {
    match &s.kind {
        ShapeKind::Geometry(a) => a.text.as_ref(),
        _ => None,
    }
}

// ------------------------------------------------------------------ text bodies

/// Writes a text body: its properties as JSON, its paragraphs with the Write mapping (and
/// each paragraph's DrawingML extras beside it).
pub(crate) fn write_body(map: &LoroMap, body: &TextBody) -> Result<()> {
    put_json(map, "body", &body.body)?;
    put_json(map, "list", &body.list_style)?;
    put_i64(map, "sec", id_value(body.container_id()))?;
    let list = child_list(map, PARAS)?;
    let paras: Vec<&Paragraph> = body.paragraphs().collect();
    // Paragraph ids are unique within a body only: they get an index of their own.
    let mut nodes = Nodes::default();
    remove_missing(&list, &paras.iter().map(|p| p.id).collect(), &mut nodes)?;
    place(
        &list,
        &paras,
        |p| p.id,
        |m, p, n| write_paragraph(m, p, n),
        &mut nodes,
    )?;
    for (i, p) in paras.iter().enumerate() {
        let m = map_at(&list, i).ok_or_else(|| corrupt("a paragraph entry that is not a map"))?;
        write_paragraph(&m, p, &mut nodes)?;
        match body.deck_props(p.id) {
            Some(d) => put_json(&m, "deck", d)?,
            None if m.get("deck").is_some() => m.delete("deck")?,
            None => {}
        }
    }
    Ok(())
}

/// A text body read back.
pub(crate) fn read_body(map: &LoroMap) -> Result<TextBody> {
    let mut nodes = Nodes::default();
    let mut paragraphs: Vec<Value> = Vec::new();
    let mut para_props = serde_json::Map::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(list) = find_list(map, PARAS) {
        for m in maps_of(&list) {
            let p = read_paragraph(&m, &mut nodes)?;
            if !seen.insert(p.id) {
                continue;
            }
            if let Some(d) = get_str(&m, "deck") {
                let d: DeckParaProps = from_json(&d)?;
                para_props.insert(p.id.0.to_string(), serde_json::to_value(d)?);
            }
            paragraphs.push(serde_json::to_value(p)?);
        }
    }
    if paragraphs.is_empty() {
        // Concurrent deletes emptied the body; a body keeps one paragraph.
        paragraphs.push(serde_json::to_value(Paragraph::new(NodeId(1)))?);
    }
    let section = get_i64(map, "sec").unwrap_or(0);
    Ok(serde_json::from_value(json!({
        "body": serde_json::from_str::<Value>(&get_str(map, "body").unwrap_or_else(|| "{}".into()))?,
        "list_style": serde_json::from_str::<Value>(&get_str(map, "list").unwrap_or_else(|| "{}".into()))?,
        "section": section,
        "paragraphs": paragraphs,
        "para_props": para_props,
    }))?)
}

// ------------------------------------------------------------------ shapes

/// Writes a shape and everything in it.
pub(crate) fn write_shape(map: &LoroMap, s: &Shape, nodes: &mut Nodes) -> Result<()> {
    put_str(map, K, SHAPE)?;
    put_i64(map, ID, id_value(s.id))?;
    nodes.insert(s.id, map);
    put_json(map, "info", &ShapeInfo::of(s))?;
    put_json(map, "ph", &s.placeholder)?;
    put_json(map, "xfrm", &s.xfrm)?;
    put_json(map, "kind", &bare_kind(&s.kind))?;
    match text_of(s) {
        Some(body) => {
            put_json(map, "has_text", &true)?;
            write_body(&child_map(map, "text")?, body)?;
        }
        None => put_json(map, "has_text", &false)?,
    }
    if let ShapeKind::Group(g) = &s.kind {
        write_shapes(map, &g.children, nodes)?;
    }
    Ok(())
}

/// Writes a shape list (of a slide or a group), each shape in full.
pub(crate) fn write_shapes(map: &LoroMap, shapes: &[Shape], nodes: &mut Nodes) -> Result<()> {
    rewrite(
        &child_list(map, SHAPES)?,
        shapes,
        |s| s.id,
        write_shape,
        nodes,
    )
}

/// The order of a shape list only: entries missing are removed (`remove`) or written
/// (`!remove`); shapes already there are left alone.
pub(crate) fn order_shapes(
    map: &LoroMap,
    shapes: &[Shape],
    nodes: &mut Nodes,
    remove: bool,
) -> Result<()> {
    let list = child_list(map, SHAPES)?;
    if remove {
        remove_missing(&list, &shapes.iter().map(|s| s.id).collect(), nodes)
    } else {
        place(&list, shapes, |s| s.id, write_shape, nodes).map(|_| ())
    }
}

/// A shape read back.
pub(crate) fn read_shape(map: &LoroMap, nodes: &mut Nodes) -> Result<Shape> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    let info: ShapeInfo = get_json_or_default(map, "info")?;
    let mut kind: ShapeKind = get_json(map, "kind")?;
    match &mut kind {
        ShapeKind::Geometry(a) => {
            a.text = if get_json_or_default::<bool>(map, "has_text")? {
                find_map(map, "text").map(|m| read_body(&m)).transpose()?
            } else {
                None
            };
        }
        ShapeKind::Group(g) => g.children = read_shapes(map, nodes)?,
        _ => {}
    }
    let mut s = Shape::new(id, info.name, get_json_or_default(map, "xfrm")?, kind);
    s.alt_text = info.alt_text;
    s.title = info.title;
    s.hidden = info.hidden;
    s.link = info.link;
    s.placeholder = get_json_or_default(map, "ph")?;
    Ok(s)
}

/// A shape list read back.
pub(crate) fn read_shapes(map: &LoroMap, nodes: &mut Nodes) -> Result<Vec<Shape>> {
    read_list(map, SHAPES, nodes, read_shape)
}

// ------------------------------------------------------------------ slides

/// Writes a slide's own values: everything but its shapes.
pub(crate) fn write_slide_shallow(map: &LoroMap, s: &Slide, nodes: &mut Nodes) -> Result<()> {
    put_str(map, K, SLIDE)?;
    put_i64(map, ID, id_value(s.id))?;
    nodes.insert(s.id, map);
    put_i64(map, "layout", id_value(s.layout))?;
    put_json(map, "name", &s.name)?;
    put_json(map, "hidden", &s.hidden)?;
    put_json(map, "show_master", &s.show_master_shapes)?;
    put_json(map, "background", &s.background)?;
    put_json(map, "color_map", &s.color_map)?;
    put_json(map, "transition", &s.transition)?;
    // Effects merge as a list (two people animating different shapes both keep theirs);
    // the trigger sequences as a whole.
    rewrite(
        &child_list(map, "effects")?,
        &s.timeline.sequence,
        |e| e.id,
        |m, e, _| {
            put_i64(m, ID, id_value(e.id))?;
            put_json(m, "v", e)
        },
        &mut Nodes::default(),
    )?;
    put_json(map, "triggers", &s.timeline.triggers)?;
    rewrite(
        &child_list(map, "comments")?,
        &s.comments,
        |c| c.id,
        write_comment,
        &mut Nodes::default(),
    )?;
    match &s.notes {
        Some(n) => {
            put_json(map, "has_notes", &true)?;
            let m = child_map(map, "notes")?;
            put_json(&m, "shapes", &n.shapes)?;
            write_body(&child_map(&m, "text")?, &n.text)?;
        }
        None => put_json(map, "has_notes", &false)?,
    }
    Ok(())
}

fn write_comment(map: &LoroMap, c: &Comment, _: &mut Nodes) -> Result<()> {
    put_i64(map, ID, id_value(c.id))?;
    put_json(map, "v", c)
}

/// Writes a slide with its shapes.
pub(crate) fn write_slide(map: &LoroMap, s: &Slide, nodes: &mut Nodes) -> Result<()> {
    write_slide_shallow(map, s, nodes)?;
    write_shapes(map, &s.shapes, nodes)
}

/// A slide read back.
pub(crate) fn read_slide(map: &LoroMap, nodes: &mut Nodes) -> Result<Slide> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    let layout = NodeId(
        get_i64(map, "layout")
            .and_then(|l| u64::try_from(l).ok())
            .unwrap_or(0),
    );
    let mut s = Slide::new(id, layout);
    s.name = get_json_or_default(map, "name")?;
    s.hidden = get_json_or_default(map, "hidden")?;
    s.show_master_shapes = get_json(map, "show_master").unwrap_or(true);
    s.background = get_json_or_default(map, "background")?;
    s.color_map = get_json_or_default(map, "color_map")?;
    s.transition = get_json_or_default(map, "transition")?;
    s.timeline.sequence = read_list(map, "effects", &mut Nodes::default(), |m, _| {
        get_json(m, "v")
    })?;
    s.timeline.triggers = get_json_or_default(map, "triggers")?;
    s.comments = read_list(map, "comments", &mut Nodes::default(), |m, _| {
        get_json(m, "v")
    })?;
    s.notes = if get_json_or_default::<bool>(map, "has_notes")? {
        match find_map(map, "notes") {
            Some(m) => Some(Notes {
                text: match find_map(&m, "text") {
                    Some(t) => read_body(&t)?,
                    None => TextBody::new(),
                },
                shapes: get_json_or_default(&m, "shapes")?,
            }),
            None => None,
        }
    } else {
        None
    };
    s.shapes = read_shapes(map, nodes)?;
    Ok(s)
}
