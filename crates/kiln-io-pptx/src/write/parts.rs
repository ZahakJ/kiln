// SPDX-License-Identifier: AGPL-3.0-or-later
//! The parts of a presentation: `presentation.xml`, slide masters with their themes, slide
//! layouts, slides, the notes master, notes pages, comments and comment authors — each written
//! from the model with the XML the reader kept merged back in its schema place.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::Presentation;
use kiln_deck::dml::serde_xml::normalize_theme;
use kiln_deck::shape::{AutoShape, Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps};
use kiln_deck::slide::{
    Background, ColorMapping, HeaderFooter, Layout, Master, Notes, NotesMaster, Slide,
};
use kiln_ooxml::drawingml::color::{DrawingColor, serialize_color};
use kiln_ooxml::drawingml::theme::{parse_theme_bytes, serialize_theme};
use kiln_ooxml::opc::{OpcError, OpcPackage, part_key, rel_type};
use kiln_ooxml::xml::{WriteOptions, XmlElement, XmlNode, write_document, write_element};

use super::Plan;
use super::comments::{self, Author};
use super::context::WriteContext;
use super::motion::{TimingInput, timing_node, transition_node};
use super::shapes::PartWriter;
use super::touched::TouchedParts;
use super::xml::{hoist_namespaces, node_refs, parse_kept, part_root, stable_guid};
use crate::common::extras::PptxExtras;
use crate::common::kind::{rel, uri};

/// Serialises a part root into its (already created) part.
fn put(ctx: &mut WriteContext<'_>, part: &str, mut root: XmlElement) -> Result<(), OpcError> {
    hoist_namespaces(&mut root);
    let text = write_element(&root, &WriteOptions::default())?;
    ctx.pkg.set_part(part, text.into_bytes())
}

/// `p:clrMap` (or `a:overrideClrMapping`).
fn color_map_element(name: &str, m: &ColorMapping) -> XmlElement {
    let s = |x: kiln_deck::slide::SchemeSlot| x.to_ooxml().to_string();
    let mut el = XmlElement::new(name)
        .with_attr("bg1", s(m.bg1))
        .with_attr("tx1", s(m.tx1))
        .with_attr("bg2", s(m.bg2))
        .with_attr("tx2", s(m.tx2));
    for (i, a) in m.accents.iter().enumerate() {
        el = el.with_attr(&format!("accent{}", i + 1), s(*a));
    }
    el.with_attr("hlink", s(m.hlink))
        .with_attr("folHlink", s(m.fol_hlink))
}

fn color_override(m: Option<&ColorMapping>) -> XmlElement {
    XmlElement::new("p:clrMapOvr").with_child(match m {
        Some(m) => color_map_element("a:overrideClrMapping", m),
        None => XmlElement::new("a:masterClrMapping"),
    })
}

fn background(w: &mut PartWriter<'_, '_>, b: &Background) -> XmlElement {
    let inner = match b {
        Background::Fill {
            fill,
            shade_to_title,
        } => XmlElement::new("p:bgPr")
            .with_opt_attr("shadeToTitle", shade_to_title.then_some("1"))
            .with_child(w.fill_element(fill))
            .with_child(XmlElement::new("a:effectLst")),
        Background::Theme { idx, color } => XmlElement::new("p:bgRef")
            .with_attr("idx", *idx)
            .with_opt_child(color.as_ref().map(serialize_color)),
    };
    XmlElement::new("p:bg").with_child(inner)
}

fn header_footer(hf: &HeaderFooter) -> Option<XmlElement> {
    (*hf != HeaderFooter::default()).then(|| {
        XmlElement::new("p:hf")
            .with_opt_attr("sldNum", (!hf.slide_number).then_some("0"))
            .with_opt_attr("hdr", (!hf.header).then_some("0"))
            .with_opt_attr("ftr", (!hf.footer).then_some("0"))
            .with_opt_attr("dt", (!hf.date).then_some("0"))
    })
}

/// Kept children of a slide-like root, split into those of `p:cSld` (custom data, controls and,
/// when there are two extension lists, the first one) and those of the root.
fn owner_kept(extras: &PptxExtras, owner: NodeId) -> (Vec<String>, Vec<String>) {
    let all = extras.owner_xml.get(&owner).cloned().unwrap_or_default();
    let root_name = |x: &str| {
        let head = x.trim_start().trim_start_matches('<');
        let end = head
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(head.len());
        head[..end].to_owned()
    };
    let ext_lists = all.iter().filter(|x| root_name(x) == "p:extLst").count();
    let mut csld = Vec::new();
    let mut root = Vec::new();
    let mut seen_ext = false;
    for x in all {
        match root_name(&x).as_str() {
            "p:custDataLst" | "p:controls" => csld.push(x),
            "p:extLst" if ext_lists > 1 && !seen_ext => {
                seen_ext = true;
                csld.push(x);
            }
            _ => root.push(x),
        }
    }
    (csld, root)
}

/// Inserts a child by schema rank: before the first child ranked after it. Names outside
/// `order` go just before the extension list.
fn place(parent: &mut XmlElement, order: &[&str], node: XmlNode) {
    let rank = |n: &XmlNode| -> usize {
        let name = match n {
            XmlNode::Element(e) => e.name.clone(),
            XmlNode::Raw(r) => {
                let head = r.trim_start().trim_start_matches('<');
                let end = head
                    .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                    .unwrap_or(head.len());
                head[..end].to_owned()
            }
            _ => return usize::MAX,
        };
        let key = if name == "mc:AlternateContent" {
            // A kept block around newer root content sits with the transition and timing.
            "p:timing"
        } else {
            name.as_str()
        };
        match order.iter().position(|o| *o == key) {
            Some(i) if key == "p:timing" && name == "mc:AlternateContent" => i * 2,
            Some(i) => i * 2 + 1,
            None => order
                .iter()
                .position(|o| *o == "p:extLst")
                .map_or(usize::MAX - 1, |i| i * 2),
        }
    };
    let r = rank(&node);
    let at = parent
        .children
        .iter()
        .position(|c| rank(c) > r)
        .unwrap_or(parent.children.len());
    parent.children.insert(at, node);
}

const SLIDE_ORDER: [&str; 5] = [
    "p:cSld",
    "p:clrMapOvr",
    "p:transition",
    "p:timing",
    "p:extLst",
];
const LAYOUT_ORDER: [&str; 6] = [
    "p:cSld",
    "p:clrMapOvr",
    "p:transition",
    "p:timing",
    "p:hf",
    "p:extLst",
];
const MASTER_ORDER: [&str; 8] = [
    "p:cSld",
    "p:clrMap",
    "p:sldLayoutIdLst",
    "p:transition",
    "p:timing",
    "p:hf",
    "p:txStyles",
    "p:extLst",
];
const NOTES_MASTER_ORDER: [&str; 5] = ["p:cSld", "p:clrMap", "p:hf", "p:notesStyle", "p:extLst"];
const NOTES_ORDER: [&str; 3] = ["p:cSld", "p:clrMapOvr", "p:extLst"];
const CSLD_ORDER: [&str; 5] = [
    "p:bg",
    "p:spTree",
    "p:custDataLst",
    "p:controls",
    "p:extLst",
];
const PRESENTATION_ORDER: [&str; 15] = [
    "p:sldMasterIdLst",
    "p:notesMasterIdLst",
    "p:handoutMasterIdLst",
    "p:sldIdLst",
    "p:sldSz",
    "p:notesSz",
    "p:smartTags",
    "p:embeddedFontLst",
    "p:custShowLst",
    "p:photoAlbum",
    "p:custDataLst",
    "p:kinsoku",
    "p:defaultTextStyle",
    "p:modifyVerifier",
    "p:extLst",
];

/// `p:cSld` with the name, background, shape tree and kept children.
fn common_slide_data(
    w: &mut PartWriter<'_, '_>,
    owner: NodeId,
    name: Option<&str>,
    bg: Option<&Background>,
    shapes: &[Shape],
    kept: &[String],
) -> XmlElement {
    let mut csld = XmlElement::new("p:cSld").with_opt_attr("name", name.filter(|n| !n.is_empty()));
    if let Some(b) = bg {
        let b = background(w, b);
        csld = csld.with_child(b);
    }
    let tree = w.sp_tree(owner, shapes);
    csld = csld.with_child(tree);
    for x in kept {
        if let Some(node) = w.kept(x) {
            place(&mut csld, &CSLD_ORDER, node);
        }
    }
    csld
}

/// Adds the root's kept children in their schema places.
fn add_kept(w: &mut PartWriter<'_, '_>, root: &mut XmlElement, order: &[&str], kept: &[String]) {
    for x in kept {
        if let Some(node) = w.kept(x) {
            place(root, order, node);
        }
    }
}

fn write_master(
    ctx: &mut WriteContext<'_>,
    extras: &PptxExtras,
    plan: &Plan,
    m: &Master,
    preserve: Option<&TouchedParts>,
) -> Result<(), OpcError> {
    let part = plan.masters[&m.id].clone();
    let theme_part = plan.themes[&m.id].clone();
    // The theme, from the model (put back byte for byte later when it did not change).
    let theme_doc = serialize_theme(&m.theme);
    let text = write_document(&theme_doc, &WriteOptions::default())?;
    ctx.pkg.set_part(&theme_part, text.into_bytes())?;

    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides)
        .with_colors(kiln_deck::Owner::Master(m.id));
    w.assign_ids(&m.shapes, &[m.id]);
    let mut layout_ids = XmlElement::new("p:sldLayoutIdLst");
    for l in &m.layouts {
        let target = plan.layouts[&l.id].clone();
        let rid = w.ctx.rel(&part, rel::SLIDE_LAYOUT, &target, false);
        layout_ids = layout_ids.with_child(
            XmlElement::new("p:sldLayoutId")
                .with_attr("id", plan.list_ids.get(&l.id).copied().unwrap_or(0))
                .with_attr("r:id", rid.as_str()),
        );
    }
    w.ctx.rel(&part, rel_type::THEME, &theme_part, false);
    let (csld_kept, root_kept) = owner_kept(extras, m.id);
    let csld = common_slide_data(
        &mut w,
        m.id,
        Some(m.name.as_str()),
        Some(&m.background),
        &m.shapes,
        &csld_kept,
    );
    let title = w.list_style("p:titleStyle", &m.title_style);
    let body = w.list_style("p:bodyStyle", &m.body_style);
    let other = w.list_style("p:otherStyle", &m.other_style);
    let mut root = part_root("p:sldMaster")
        .with_opt_attr("preserve", m.preserve.then_some("1"))
        .with_child(csld)
        .with_child(color_map_element("p:clrMap", &m.color_map))
        .with_child(layout_ids)
        .with_opt_child(header_footer(&m.header_footer))
        .with_child(
            XmlElement::new("p:txStyles")
                .with_child(title)
                .with_child(body)
                .with_child(other),
        );
    add_kept(&mut w, &mut root, &MASTER_ORDER, &root_kept);
    w.finish();
    put(ctx, &part, root)?;
    for l in &m.layouts {
        let lp = &plan.layouts[&l.id];
        if !keep(
            ctx,
            preserve,
            |t| t.owner(l.id),
            extras.part_names.get(&l.id),
            lp,
        ) {
            write_layout(ctx, extras, plan, &part, l)?;
        }
    }
    Ok(())
}

fn write_layout(
    ctx: &mut WriteContext<'_>,
    extras: &PptxExtras,
    plan: &Plan,
    master_part: &str,
    l: &Layout,
) -> Result<(), OpcError> {
    let part = plan.layouts[&l.id].clone();
    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides)
        .with_colors(kiln_deck::Owner::Layout(l.id));
    w.ctx.rel(&part, rel::SLIDE_MASTER, master_part, false);
    w.assign_ids(&l.shapes, &[l.id]);
    let (csld_kept, root_kept) = owner_kept(extras, l.id);
    let csld = common_slide_data(
        &mut w,
        l.id,
        Some(l.name.as_str()),
        l.background.as_ref(),
        &l.shapes,
        &csld_kept,
    );
    let mut root = part_root("p:sldLayout")
        .with_opt_attr("showMasterSp", (!l.show_master_shapes).then_some("0"))
        .with_opt_attr(
            "type",
            (l.kind != kiln_deck::LayoutKind::Custom).then(|| l.kind.as_ooxml().to_owned()),
        )
        .with_opt_attr("preserve", l.preserve.then_some("1"))
        .with_child(csld)
        .with_child(color_override(l.color_map.as_ref()));
    add_kept(&mut w, &mut root, &LAYOUT_ORDER, &root_kept);
    w.finish();
    put(ctx, &part, root)
}

/// A notes master for a presentation that has notes but did not have one.
fn default_notes_master() -> NotesMaster {
    NotesMaster {
        id: NodeId(0),
        shapes: Vec::new(),
        background: Background::Theme {
            idx: 1001,
            color: Some(DrawingColor::scheme("bg1")),
        },
        notes_style: kiln_deck::ListStyle::default(),
        color_map: ColorMapping::default(),
    }
}

fn write_notes_master(
    ctx: &mut WriteContext<'_>,
    pres: &Presentation,
    extras: &PptxExtras,
    plan: &Plan,
) -> Result<(), OpcError> {
    let Some(part) = plan.notes_master.clone() else {
        return Ok(());
    };
    let generated;
    let nm = match &pres.notes_master {
        Some(n) => n,
        None => {
            generated = default_notes_master();
            &generated
        }
    };
    // Its theme: kept from the file (in the passthrough), else one written now.
    if let Some(t) = &plan.notes_master_theme
        && let Some(m) = pres.masters.first()
    {
        let text = write_document(&serialize_theme(&m.theme), &WriteOptions::default())?;
        ctx.pkg.set_part(t, text.into_bytes())?;
        ctx.rel(&part, rel_type::THEME, t, false);
    }
    let has_theme = ctx.pkg.relationships(&part).is_ok_and(|r| {
        r.all()
            .iter()
            .any(|x| kiln_ooxml::opc::rel_type_matches(&x.rel_type, rel_type::THEME))
    });
    if !has_theme && let Some(first) = pres.masters.first() {
        let t = plan.themes[&first.id].clone();
        ctx.rel(&part, rel_type::THEME, &t, false);
    }
    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides);
    w.color = pres.notes_master.as_ref().and_then(|n| {
        pres.masters.first().map(|m| {
            kiln_ooxml::drawingml::theme::theme_color_context(
                &m.theme,
                n.color_map.to_ooxml(),
                None,
            )
        })
    });
    w.assign_ids(&nm.shapes, &[nm.id]);
    let (csld_kept, root_kept) = owner_kept(extras, nm.id);
    let csld = common_slide_data(
        &mut w,
        nm.id,
        None,
        Some(&nm.background),
        &nm.shapes,
        &csld_kept,
    );
    let style = w.list_style("p:notesStyle", &nm.notes_style);
    let mut root = part_root("p:notesMaster")
        .with_child(csld)
        .with_child(color_map_element("p:clrMap", &nm.color_map))
        .with_child(style);
    add_kept(&mut w, &mut root, &NOTES_MASTER_ORDER, &root_kept);
    w.finish();
    put(ctx, &part, root)
}

/// A shape id no model shape has, for the notes page's body placeholder.
const NOTES_BODY: NodeId = NodeId(u64::MAX - 1);

fn write_notes(
    ctx: &mut WriteContext<'_>,
    extras: &PptxExtras,
    plan: &Plan,
    slide: &Slide,
    notes: &Notes,
) -> Result<(), OpcError> {
    let Some(part) = plan.notes.get(&slide.id).cloned() else {
        return Ok(());
    };
    let key = extras
        .notes_keys
        .get(&slide.id)
        .copied()
        .unwrap_or(NodeId(0));
    let mut shapes = notes.shapes.clone();
    let body = Shape {
        id: NOTES_BODY,
        name: "Notes Placeholder 2".to_owned(),
        alt_text: None,
        title: None,
        hidden: false,
        placeholder: Some(Placeholder::new(PlaceholderType::Body, 1)),
        xfrm: None,
        kind: ShapeKind::Geometry(AutoShape {
            text_box: false,
            props: ShapeProps::default(),
            text: Some(notes.text.clone()),
        }),
        link: None,
    };
    let at = shapes
        .iter()
        .rposition(|s| {
            s.placeholder
                .is_some_and(|p| p.kind == PlaceholderType::SlideImage)
        })
        .map_or(shapes.len(), |i| i + 1);
    shapes.insert(at, body);
    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides);
    if let Some(nm) = &plan.notes_master {
        w.ctx.rel(&part, rel::NOTES_MASTER, nm, false);
    }
    let slide_part = plan.slides[&slide.id].clone();
    w.ctx.rel(&part, rel::SLIDE, &slide_part, false);
    w.assign_ids(&shapes, &[key]);
    let (csld_kept, root_kept) = owner_kept(extras, key);
    let csld = common_slide_data(&mut w, key, None, None, &shapes, &csld_kept);
    let mut root = part_root("p:notes")
        .with_child(csld)
        .with_child(color_override(None));
    add_kept(&mut w, &mut root, &NOTES_ORDER, &root_kept);
    w.finish();
    put(ctx, &part, root)
}

/// The `p14:creationId` in a slide's kept extension list (comments are anchored to it).
fn creation_id(root_kept: &[String]) -> u32 {
    for x in root_kept {
        if let Some(e) = parse_kept(x) {
            for c in super::xml::descendants_local(&e, "creationId") {
                if let Some(v) = c.attr_int("val").and_then(|v| u32::try_from(v).ok()) {
                    return v;
                }
            }
        }
    }
    0
}

/// State shared by the slides' comment parts.
struct CommentState<'p> {
    authors: &'p [Author],
    next_idx: BTreeMap<usize, u32>,
}

fn write_slide(
    ctx: &mut WriteContext<'_>,
    pres: &Presentation,
    extras: &PptxExtras,
    plan: &Plan,
    slide: &Slide,
    comment_state: &mut CommentState<'_>,
    preserve: Option<&TouchedParts>,
) -> Result<(), OpcError> {
    let part = plan.slides[&slide.id].clone();
    let kept = keep(
        ctx,
        preserve,
        |t| t.owner(slide.id),
        extras.part_names.get(&slide.id),
        &part,
    );
    if kept {
        write_slide_companions(ctx, extras, plan, slide, comment_state, preserve)?;
        return Ok(());
    }
    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides)
        .with_colors(kiln_deck::Owner::Slide(slide.id));
    if let Some(l) = plan.layouts.get(&slide.layout) {
        w.ctx.rel(&part, rel::SLIDE_LAYOUT, l, false);
    }
    w.assign_ids(&slide.shapes, &[slide.id]);
    let (csld_kept, mut root_kept) = owner_kept(extras, slide.id);
    let csld = common_slide_data(
        &mut w,
        slide.id,
        slide.name.as_deref(),
        slide.background.as_ref(),
        &slide.shapes,
        &csld_kept,
    );
    let mut root = part_root("p:sld")
        .with_opt_attr("showMasterSp", (!slide.show_master_shapes).then_some("0"))
        .with_opt_attr("show", slide.hidden.then_some("0"))
        .with_child(csld)
        .with_child(color_override(slide.color_map.as_ref()));
    if let Some(t) = &slide.transition
        && let Some(node) =
            transition_node(t, extras.transitions.get(&slide.id).map(String::as_str))
    {
        node_refs(&node, &mut w.refs);
        root.children.push(node);
    }
    // Animations: the shapes on the slide with the ids they are written with and were read with.
    let mut original_ids: HashMap<NodeId, u32> = HashMap::new();
    let mut text_shapes: BTreeSet<NodeId> = BTreeSet::new();
    for s in &slide.shapes {
        s.walk(&mut |x| {
            if let Some(id) = extras.shape_ids.get(&x.id) {
                original_ids.insert(x.id, *id);
            }
            if x.text().is_some() {
                text_shapes.insert(x.id);
            }
        });
    }
    let input = TimingInput {
        timeline: &slide.timeline,
        ids: &w.ids,
        original_ids: &original_ids,
        text_shapes: &text_shapes,
    };
    if let Some(node) = timing_node(&input, extras.timing.get(&slide.id).map(String::as_str)) {
        root.children.push(node);
    }

    // Notes and comments.
    if let (Some(n), Some(target)) = (&slide.notes, plan.notes.get(&slide.id)) {
        w.ctx.rel(&part, rel::NOTES_SLIDE, target, false);
        let _ = n;
    }
    let comment_rel = match (slide.comments.is_empty(), plan.comments.get(&slide.id)) {
        (false, Some(c)) => {
            let rt = if plan.modern_comments {
                rel::MODERN_COMMENTS
            } else {
                rel::COMMENTS
            };
            Some((c.clone(), w.ctx.rel(&part, rt, c, false)))
        }
        _ => None,
    };
    // The root extension list: the modern comment link is the writer's.
    let creation = creation_id(&root_kept);
    let mut ext_list: Option<XmlElement> = None;
    root_kept.retain(|x| {
        if ext_list.is_none()
            && x.trim_start().starts_with("<p:extLst")
            && let Some(e) = parse_kept(x)
        {
            ext_list = Some(e);
            return false;
        }
        true
    });
    let mut ext_list = ext_list.unwrap_or_else(|| XmlElement::new("p:extLst"));
    ext_list.children.retain(
        |c| !matches!(c, XmlNode::Element(e) if e.attr("uri") == Some(uri::MODERN_COMMENTS_EXT)),
    );
    if plan.modern_comments
        && let Some((_, rid)) = &comment_rel
    {
        ext_list.children.push(XmlNode::Element(
            XmlElement::new("p:ext")
                .with_attr("uri", uri::MODERN_COMMENTS_EXT)
                .with_child(
                    XmlElement::new("p188:commentRel")
                        .with_attr("xmlns:p188", super::xml::NS_P188)
                        .with_attr("r:id", rid.as_str()),
                ),
        ));
    }
    if ext_list.elements().next().is_some() {
        let node = XmlNode::Element(ext_list);
        node_refs(&node, &mut w.refs);
        place(&mut root, &SLIDE_ORDER, node);
    }
    add_kept(&mut w, &mut root, &SLIDE_ORDER, &root_kept);
    w.finish();
    put(ctx, &part, root)?;
    let _ = (pres, comment_rel, creation);
    write_slide_companions(ctx, extras, plan, slide, comment_state, preserve)
}

/// A slide's notes page (copied when untouched) and comments (always written: their author
/// ids are the writer's).
fn write_slide_companions(
    ctx: &mut WriteContext<'_>,
    extras: &PptxExtras,
    plan: &Plan,
    slide: &Slide,
    comment_state: &mut CommentState<'_>,
    preserve: Option<&TouchedParts>,
) -> Result<(), OpcError> {
    if let (Some(n), Some(np)) = (&slide.notes, plan.notes.get(&slide.id)) {
        let original = extras.notes_parts.get(&slide.id);
        if !keep(ctx, preserve, |t| t.notes_of(slide.id), original, np) {
            write_notes(ctx, extras, plan, slide, n)?;
        }
    }
    if let (false, Some(cpart)) = (slide.comments.is_empty(), plan.comments.get(&slide.id)) {
        let xml = if plan.modern_comments {
            let sid = plan.list_ids.get(&slide.id).copied().unwrap_or(0);
            let creation = creation_id(&owner_kept(extras, slide.id).1);
            comments::modern_comments(slide, sid, creation)
        } else {
            comments::legacy_comments(slide, comment_state.authors, &mut comment_state.next_idx)
        };
        put(ctx, cpart, xml)?;
    }
    Ok(())
}

fn write_presentation(
    ctx: &mut WriteContext<'_>,
    pres: &Presentation,
    extras: &PptxExtras,
    plan: &Plan,
) -> Result<(), OpcError> {
    let part = plan.main.clone();
    let mut w = PartWriter::new(ctx, &part, extras, &plan.slides);
    w.color = pres
        .masters
        .first()
        .and_then(|m| pres.color_context(kiln_deck::Owner::Master(m.id)));
    let mut masters = XmlElement::new("p:sldMasterIdLst");
    for m in &pres.masters {
        let rid = w
            .ctx
            .rel(&part, rel::SLIDE_MASTER, &plan.masters[&m.id], false);
        masters = masters.with_child(
            XmlElement::new("p:sldMasterId")
                .with_attr("id", plan.list_ids.get(&m.id).copied().unwrap_or(0))
                .with_attr("r:id", rid.as_str()),
        );
    }
    let notes_master = plan.notes_master.clone().map(|n| {
        let rid = w.ctx.rel(&part, rel::NOTES_MASTER, &n, false);
        XmlElement::new("p:notesMasterIdLst")
            .with_child(XmlElement::new("p:notesMasterId").with_attr("r:id", rid.as_str()))
    });
    let mut slides = XmlElement::new("p:sldIdLst");
    for s in &pres.slides {
        let rid = w.ctx.rel(&part, rel::SLIDE, &plan.slides[&s.id], false);
        slides = slides.with_child(
            XmlElement::new("p:sldId")
                .with_attr("id", plan.list_ids.get(&s.id).copied().unwrap_or(0))
                .with_attr("r:id", rid.as_str()),
        );
    }
    if let Some(a) = &plan.authors {
        let rt = if plan.modern_comments {
            rel::AUTHORS
        } else {
            rel::COMMENT_AUTHORS
        };
        w.ctx.rel(&part, rt, a, false);
    }
    if let Some(first) = pres.masters.first() {
        let t = plan.themes[&first.id].clone();
        w.ctx.rel(&part, rel_type::THEME, &t, false);
    }
    let size = &pres.slide_size;
    let notes = &pres.notes_size;
    let default_style = w.list_style("p:defaultTextStyle", &pres.default_text_style);
    let mut root = part_root("p:presentation");
    // Attributes as read (namespace declarations for kept content and mc:Ignorable included).
    for (k, v) in &extras.presentation_attrs {
        if matches!(k.as_str(), "xmlns:a" | "xmlns:r" | "xmlns:p") {
            continue;
        }
        root.attrs.set(k.clone(), v.clone());
    }
    if pres.first_slide_number != 1 {
        root.attrs
            .set("firstSlideNum", pres.first_slide_number.to_string());
    }
    root = root
        .with_child(masters)
        .with_opt_child(notes_master)
        .with_opt_child((!pres.slides.is_empty()).then_some(slides))
        .with_child(
            XmlElement::new("p:sldSz")
                .with_attr("cx", size.cx)
                .with_attr("cy", size.cy)
                .with_opt_attr("type", size.kind.as_ooxml()),
        )
        .with_child(
            XmlElement::new("p:notesSz")
                .with_attr("cx", notes.cx)
                .with_attr("cy", notes.cy),
        )
        .with_child(default_style);
    // Kept children; the sections join the kept extension list.
    let mut ext_list: Option<XmlElement> = None;
    for x in &extras.presentation_xml {
        if ext_list.is_none() && x.trim_start().starts_with("<p:extLst") {
            ext_list = parse_kept(x);
            if ext_list.is_some() {
                continue;
            }
        }
        if let Some(node) = w.kept(x) {
            place(&mut root, &PRESENTATION_ORDER, node);
        }
    }
    if !pres.sections.is_empty() {
        let mut start = 0usize;
        let mut list = XmlElement::new("p14:sectionLst").with_attr("xmlns:p14", super::xml::NS_P14);
        for sec in &pres.sections {
            let guid = extras
                .section_guids
                .get(&sec.id)
                .cloned()
                .unwrap_or_else(|| stable_guid(&format!("section:{}", sec.id.0)));
            let end = start.saturating_add(sec.len).min(pres.slides.len());
            let ids = pres.slides[start..end].iter().map(|s| {
                XmlElement::new("p14:sldId")
                    .with_attr("id", plan.list_ids.get(&s.id).copied().unwrap_or(0))
            });
            list = list.with_child(
                XmlElement::new("p14:section")
                    .with_attr("name", sec.name.as_str())
                    .with_attr("id", guid)
                    .with_child(XmlElement::new("p14:sldIdLst").with_children(ids)),
            );
            start = end;
        }
        let ext = XmlElement::new("p:ext")
            .with_attr("uri", uri::SECTIONS_EXT)
            .with_child(list);
        let mut l = ext_list
            .take()
            .unwrap_or_else(|| XmlElement::new("p:extLst"));
        l.children.insert(0, XmlNode::Element(ext));
        ext_list = Some(l);
    }
    if let Some(l) = ext_list {
        let node = XmlNode::Element(l);
        node_refs(&node, &mut w.refs);
        place(&mut root, &PRESENTATION_ORDER, node);
    }
    w.finish();
    put(ctx, &part, root)
}

/// Copies an owner's part from the original when no edit touched it and it keeps its name.
fn keep(
    ctx: &mut WriteContext<'_>,
    preserve: Option<&TouchedParts>,
    touched: impl Fn(&TouchedParts) -> bool,
    original: Option<&String>,
    part: &str,
) -> bool {
    match (preserve, original) {
        (Some(t), Some(o)) if !touched(t) && part_key(o) == part_key(part) => ctx.preserve(part),
        _ => false,
    }
}

/// Writes every model part; with `preserve`, parts no edit touched are copied instead.
pub(crate) fn write_all(
    ctx: &mut WriteContext<'_>,
    pres: &Presentation,
    extras: &PptxExtras,
    plan: &Plan,
    preserve: Option<&TouchedParts>,
) -> Result<(), OpcError> {
    let original = |id: NodeId| extras.part_names.get(&id);
    for m in &pres.masters {
        let part = &plan.masters[&m.id];
        if keep(ctx, preserve, |t| t.owner(m.id), original(m.id), part) {
            // The theme is still written from the model (and put back when unchanged).
            let text = write_document(&serialize_theme(&m.theme), &WriteOptions::default())?;
            ctx.pkg.set_part(&plan.themes[&m.id], text.into_bytes())?;
            for l in &m.layouts {
                let lp = &plan.layouts[&l.id];
                if !keep(ctx, preserve, |t| t.owner(l.id), original(l.id), lp) {
                    write_layout(ctx, extras, plan, part, l)?;
                }
            }
            continue;
        }
        write_master(ctx, extras, plan, m, preserve)?;
    }
    let notes_master_kept = match (&pres.notes_master, &plan.notes_master) {
        (Some(nm), Some(part)) => keep(ctx, preserve, |t| t.owner(nm.id), original(nm.id), part),
        _ => false,
    };
    if !notes_master_kept {
        write_notes_master(ctx, pres, extras, plan)?;
    }
    let authors = comments::authors(pres);
    let mut state = CommentState {
        authors: &authors,
        next_idx: BTreeMap::new(),
    };
    for s in &pres.slides {
        write_slide(ctx, pres, extras, plan, s, &mut state, preserve)?;
    }
    if let Some(a) = &plan.authors {
        let xml = if plan.modern_comments {
            comments::modern_authors(&authors)
        } else {
            comments::legacy_authors(&authors)
        };
        put(ctx, a, xml)?;
    }
    let main_kept = keep(
        ctx,
        preserve,
        TouchedParts::presentation_changed,
        ctx.original_name_of_main().as_ref(),
        &plan.main,
    );
    if main_kept {
        // The relationships the writer adds beside the kept list (comment authors).
        if let Some(a) = &plan.authors {
            let rt = if plan.modern_comments {
                rel::AUTHORS
            } else {
                rel::COMMENT_AUTHORS
            };
            ctx.rel(&plan.main, rt, a, false);
        }
        return Ok(());
    }
    write_presentation(ctx, pres, extras, plan)
}

/// Puts back the original bytes of every master theme the model did not change; returns the
/// part keys copied.
pub(crate) fn restore_unchanged_themes(
    ctx: &mut WriteContext<'_>,
    pres: &Presentation,
    extras: &PptxExtras,
    plan: &Plan,
    orig: &OpcPackage,
) -> Result<BTreeSet<String>, OpcError> {
    let mut out = BTreeSet::new();
    for m in &pres.masters {
        let (Some(o), Some(n)) = (extras.theme_parts.get(&m.id), plan.themes.get(&m.id)) else {
            continue;
        };
        if part_key(o) != part_key(n) || out.contains(&part_key(n)) {
            continue;
        }
        let Ok(bytes) = orig.part_bytes(o) else {
            continue;
        };
        let Ok(mut theme) = parse_theme_bytes(&bytes) else {
            continue;
        };
        normalize_theme(&mut theme);
        if theme == m.theme {
            let bytes = bytes.into_owned();
            ctx.pkg.set_part(n, bytes)?;
            out.insert(part_key(n));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_children_take_their_schema_place() {
        let mut root = XmlElement::new("p:sld")
            .with_child(XmlElement::new("p:cSld"))
            .with_child(XmlElement::new("p:clrMapOvr"))
            .with_child(XmlElement::new("p:timing"));
        place(
            &mut root,
            &SLIDE_ORDER,
            XmlNode::Element(XmlElement::new("p:extLst")),
        );
        place(
            &mut root,
            &SLIDE_ORDER,
            XmlNode::Element(XmlElement::new("p:transition")),
        );
        place(
            &mut root,
            &SLIDE_ORDER,
            XmlNode::Element(XmlElement::new("mc:AlternateContent")),
        );
        let names: Vec<_> = root.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "p:cSld",
                "p:clrMapOvr",
                "p:transition",
                "mc:AlternateContent",
                "p:timing",
                "p:extLst"
            ]
        );
    }

    #[test]
    fn owner_kept_splits_by_root() {
        let mut x = PptxExtras::default();
        x.push_owner_xml(NodeId(1), "<p:custDataLst/>".into());
        x.push_owner_xml(NodeId(1), "<p:extLst><p:ext uri=\"a\"/></p:extLst>".into());
        x.push_owner_xml(NodeId(1), "<p:extLst><p:ext uri=\"b\"/></p:extLst>".into());
        let (csld, root) = owner_kept(&x, NodeId(1));
        assert_eq!(csld.len(), 2);
        assert_eq!(root.len(), 1);
        assert!(root[0].contains("\"b\""));
    }

    #[test]
    fn colour_maps_write_every_slot() {
        let e = color_map_element("p:clrMap", &ColorMapping::default());
        assert_eq!(e.attr("bg1"), Some("lt1"));
        assert_eq!(e.attr("accent6"), Some("accent6"));
        assert_eq!(e.attr("folHlink"), Some("folHlink"));
        assert_eq!(e.attrs.len(), 12);
    }
}
