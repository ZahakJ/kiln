// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shapes → `p:spTree` children: `p:sp` (geometry, text, placeholders), `p:pic` (pictures,
//! video and audio), `p:graphicFrame` (tables, charts), `p:grpSp`, `p:cxnSp`, and the original
//! XML of what the model approximates (SmartArt, OLE objects, alternate content, ink and other
//! kept shapes) — the inverse of the reader's `shapes` module.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::geom::{ChildSpace, Transform};
use kiln_deck::shape::{
    AutoShape, ChartRef, Connector, FontCollectionRef, Geometry, Group, Media, MediaKind,
    MediaSource, Picture, Placeholder, PlaceholderSize, PlaceholderType, Shape, ShapeKind,
    ShapeProps, ShapeStyle, StyleRef, Table,
};
use kiln_ooxml::drawingml::color::serialize_color;
use kiln_ooxml::drawingml::effects::serialize_effect_list;
use kiln_ooxml::drawingml::fill::{Fill, serialize_fill};
use kiln_ooxml::drawingml::geometry::serialize_custom_geometry;
use kiln_ooxml::drawingml::line::{Line, serialize_line, serialize_line_named};
use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, XmlNode};

use super::context::WriteContext;
use super::xml::{NS_C, NS_CX, NS_P14, kept_node, node_refs, parse_kept, shape_ids_in};
use crate::common::extras::{PptxExtras, ShapeSource};
use crate::common::kind::{SUPPORTED_MC, rel, uri};

/// Writes the content of one part: its shapes, text and the relationships they need.
pub(crate) struct PartWriter<'c, 'a> {
    pub ctx: &'c mut WriteContext<'a>,
    /// The part being written.
    pub part: String,
    pub extras: &'c PptxExtras,
    /// Slide id → its part, for jumps to slides.
    pub slide_parts: &'c BTreeMap<NodeId, String>,
    /// `p:cNvPr/@id` of every shape of the part.
    pub ids: HashMap<NodeId, u32>,
    /// Counter for field ids.
    pub field_seq: u64,
    /// Relationship ids kept XML of this part refers to.
    pub refs: BTreeSet<String>,
    /// The theme colours and colour map in force, so run colours keep their exact rendering.
    pub color: Option<kiln_ooxml::drawingml::color::ColorContext>,
    /// Exact run colours found so far: `scheme:rgb` → the colour to write.
    pub color_cache: HashMap<String, Option<kiln_ooxml::drawingml::color::DrawingColor>>,
}

impl std::fmt::Debug for PartWriter<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartWriter")
            .field("part", &self.part)
            .finish_non_exhaustive()
    }
}

/// `a:off`/`a:ext` (and a group's `a:chOff`/`a:chExt`) as `name` (`a:xfrm`, `p:xfrm`).
pub(crate) fn xfrm_element(name: &str, x: &Transform, child: Option<&ChildSpace>) -> XmlElement {
    let mut el = XmlElement::new(name)
        .with_opt_attr("rot", (x.rot != 0).then_some(x.rot))
        .with_opt_attr("flipH", x.flip_h.then_some("1"))
        .with_opt_attr("flipV", x.flip_v.then_some("1"))
        .with_child(
            XmlElement::new("a:off")
                .with_attr("x", x.x)
                .with_attr("y", x.y),
        )
        .with_child(
            XmlElement::new("a:ext")
                .with_attr("cx", x.cx)
                .with_attr("cy", x.cy),
        );
    if let Some(c) = child {
        el = el
            .with_child(
                XmlElement::new("a:chOff")
                    .with_attr("x", c.x)
                    .with_attr("y", c.y),
            )
            .with_child(
                XmlElement::new("a:chExt")
                    .with_attr("cx", c.cx)
                    .with_attr("cy", c.cy),
            );
    }
    el
}

fn placeholder_element(ph: &Placeholder) -> XmlElement {
    XmlElement::new("p:ph")
        .with_opt_attr(
            "type",
            (ph.kind != PlaceholderType::Object).then_some(ph.kind.as_str()),
        )
        .with_opt_attr("orient", ph.vertical.then_some("vert"))
        .with_opt_attr(
            "sz",
            match ph.size {
                PlaceholderSize::Full => None,
                PlaceholderSize::Half => Some("half"),
                PlaceholderSize::Quarter => Some("quarter"),
            },
        )
        .with_opt_attr("idx", (ph.idx != 0).then_some(ph.idx))
        .with_opt_attr("hasCustomPrompt", ph.custom_prompt.then_some("1"))
}

fn style_ref(name: &str, r: &StyleRef) -> XmlElement {
    XmlElement::new(name)
        .with_attr("idx", r.idx)
        .with_opt_child(r.color.as_ref().map(serialize_color))
}

fn style_element(s: &ShapeStyle) -> XmlElement {
    XmlElement::new("p:style")
        .with_child(style_ref("a:lnRef", &s.line))
        .with_child(style_ref("a:fillRef", &s.fill))
        .with_child(style_ref("a:effectRef", &s.effect))
        .with_child(
            XmlElement::new("a:fontRef")
                .with_attr(
                    "idx",
                    match s.font {
                        FontCollectionRef::Major => "major",
                        FontCollectionRef::Minor => "minor",
                        FontCollectionRef::None => "none",
                    },
                )
                .with_opt_child(s.font_color.as_ref().map(serialize_color)),
        )
}

fn geometry_element(g: &Geometry) -> XmlElement {
    match g {
        Geometry::Preset { name, adjust } => XmlElement::new("a:prstGeom")
            .with_attr("prst", name.as_str())
            .with_child(
                XmlElement::new("a:avLst").with_children(adjust.iter().map(|(n, v)| {
                    XmlElement::new("a:gd")
                        .with_attr("name", n.as_str())
                        .with_attr("fmla", format!("val {v}"))
                })),
            ),
        Geometry::Custom(def) => serialize_custom_geometry(def),
    }
}

/// Is a kept shape fragment part of the non-visual properties (`p:nvPr`)?
fn is_nv_fragment(xml: &str) -> bool {
    let head = xml.trim_start();
    head.starts_with("<p:extLst") || head.starts_with("<p:custDataLst")
}

/// The local name of a fragment's root.
fn fragment_root(xml: &str) -> &str {
    let head = xml.trim_start().trim_start_matches('<');
    let end = head
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(head.len());
    local_name(&head[..end])
}

impl<'c, 'a> PartWriter<'c, 'a> {
    pub(crate) fn new(
        ctx: &'c mut WriteContext<'a>,
        part: &str,
        extras: &'c PptxExtras,
        slide_parts: &'c BTreeMap<NodeId, String>,
    ) -> Self {
        Self {
            ctx,
            part: part.to_owned(),
            extras,
            slide_parts,
            ids: HashMap::new(),
            field_seq: 0,
            refs: BTreeSet::new(),
            color: None,
            color_cache: HashMap::new(),
        }
    }

    /// With the colour context of the slide, layout or master the part is.
    pub(crate) fn with_colors(mut self, owner: kiln_deck::Owner) -> Self {
        self.color = self.ctx.pres.color_context(owner);
        self
    }

    /// Gives every shape of the part (`owners` lists the shape lists of the part, opaque shapes
    /// filed under those ids and their groups included) its `p:cNvPr/@id`: the id it was read
    /// with when that is still unique, a fresh one otherwise.
    pub(crate) fn assign_ids(&mut self, shapes: &[Shape], owners: &[NodeId]) {
        let mut all: Vec<&Shape> = Vec::new();
        for s in shapes {
            let mut stack = vec![s];
            while let Some(x) = stack.pop() {
                all.push(x);
                if !self.extras.is_smart_art(x.id)
                    && let Some(c) = x.children()
                {
                    stack.extend(c.iter().rev());
                }
            }
        }
        let mut used: BTreeSet<u32> = BTreeSet::from([1]);
        // Ids inside kept XML of this part (opaque shapes, SmartArt and OLE frames).
        let mut lists: BTreeSet<NodeId> = owners.iter().copied().collect();
        lists.extend(all.iter().map(|s| s.id));
        for o in &self.extras.opaque_shapes {
            if lists.contains(&o.parent) {
                shape_ids_in(&o.xml, &mut used);
            }
        }
        for s in &all {
            if let Some(src) = self.extras.source_of(s.id) {
                shape_ids_in(src.xml(), &mut used);
            }
        }
        let mut pending = Vec::new();
        for s in &all {
            match self.extras.shape_ids.get(&s.id) {
                Some(&id) if id > 1 && !self.ids.values().any(|v| *v == id) => {
                    // An id kept XML uses for another shape would clash: only the shape's own
                    // source may share it.
                    let own = self
                        .extras
                        .source_of(s.id)
                        .is_some_and(|src| src.xml().contains(&format!("id=\"{id}\"")));
                    if used.contains(&id) && !own {
                        pending.push(s.id);
                    } else {
                        used.insert(id);
                        self.ids.insert(s.id, id);
                    }
                }
                _ => pending.push(s.id),
            }
        }
        let mut next = used.iter().next_back().copied().unwrap_or(1);
        for id in pending {
            next = next.saturating_add(1);
            self.ids.insert(id, next);
        }
    }

    /// A kept fragment as a node; its relationship ids are recorded for repair. Kept text
    /// that is not well-formed XML is left out and reported.
    pub(crate) fn kept(&mut self, xml: &str) -> Option<XmlNode> {
        let Some(node) = kept_node(xml) else {
            self.ctx.issue(
                "xml.invalid",
                "Kept content that is not well-formed XML was left out.",
                &self.part.clone(),
            );
            return None;
        };
        node_refs(&node, &mut self.refs);
        Some(node)
    }

    /// Re-creates the relationships kept XML refers to.
    pub(crate) fn finish(&mut self) {
        let refs = std::mem::take(&mut self.refs);
        self.ctx.repair_refs(&self.part.clone(), &refs);
    }

    /// The `r:id` and `action` of a click link (`#slide:<id>`, a `ppaction://` verb, a part name
    /// or a URL); `None` when the link leads nowhere.
    pub(crate) fn link_target(&mut self, link: &str) -> Option<(String, Option<String>)> {
        if let Some(n) = link.strip_prefix("#slide:") {
            let target = n
                .parse::<u64>()
                .ok()
                .and_then(|id| self.slide_parts.get(&NodeId(id)))
                .cloned();
            let Some(target) = target else {
                self.ctx.issue(
                    "link.broken",
                    "A link jumps to a slide that is no longer in the presentation; it is left out.",
                    &self.part.clone(),
                );
                return None;
            };
            let id = self.ctx.rel(&self.part, rel::SLIDE, &target, false);
            return Some((id, Some("ppaction://hlinksldjump".to_owned())));
        }
        if link.starts_with("ppaction://") {
            return Some((String::new(), Some(link.to_owned())));
        }
        if link.starts_with('/') {
            if self.ctx.pkg.has(link) {
                let id = self.ctx.rel(&self.part, rel::HYPERLINK, link, false);
                return Some((id, None));
            }
            if let Some(id) = self.ctx.original_internal_link(&self.part.clone(), link) {
                return Some((id, None));
            }
        }
        let id = self.ctx.rel(&self.part, rel::HYPERLINK, link, true);
        Some((id, None))
    }

    fn c_nv_pr(&mut self, s: &Shape) -> XmlElement {
        let id = self.ids.get(&s.id).copied().unwrap_or(0);
        let mut el = XmlElement::new("p:cNvPr")
            .with_attr("id", id)
            .with_attr("name", s.name.as_str())
            .with_opt_attr("descr", s.alt_text.as_deref())
            .with_opt_attr("hidden", s.hidden.then_some("1"))
            .with_opt_attr("title", s.title.as_deref());
        if let Some(link) = &s.link
            && let Some((rid, action)) = self.link_target(link)
        {
            el = el.with_child(
                XmlElement::new("a:hlinkClick")
                    .with_attr("r:id", rid.as_str())
                    .with_opt_attr("action", action.as_deref()),
            );
        }
        el
    }

    /// `p:nvPr`: the placeholder, media files, then kept custom data and extensions.
    fn nv_pr(&mut self, s: &Shape, media: Vec<XmlElement>, kept_nv: &[String]) -> XmlElement {
        let mut el = XmlElement::new("p:nvPr")
            .with_opt_child(s.placeholder.as_ref().map(placeholder_element))
            .with_children(media);
        for x in kept_nv {
            if let Some(node) = self.kept(x) {
                el.children.push(node);
            }
        }
        el
    }

    /// Replaces picture fills' part keys with relationship ids of this part.
    fn fill_rel(&mut self, fill: &mut Fill) {
        let Fill::Blip(b) = fill else { return };
        if let Some(key) = b.embed.clone() {
            match self.ctx.media_part(&key) {
                Some(target) => {
                    b.embed = Some(self.ctx.rel(&self.part, rel::IMAGE, &target, false))
                }
                None => {
                    b.embed = None;
                    self.ctx.issue(
                        "image.missing",
                        "A picture fill's image is missing.",
                        &self.part.clone(),
                    );
                }
            }
        }
        if let Some(url) = b.link.clone() {
            b.link = Some(self.ctx.rel(&self.part, rel::IMAGE, &url, true));
        }
    }

    pub(crate) fn fill_element(&mut self, fill: &Fill) -> XmlElement {
        let mut f = fill.clone();
        self.fill_rel(&mut f);
        serialize_fill(&f)
    }

    pub(crate) fn line_element(&mut self, line: &Line, name: &str) -> XmlElement {
        let mut l = line.clone();
        if let Some(f) = l.fill.as_mut() {
            self.fill_rel(f);
        }
        if name == "a:ln" {
            serialize_line(&l)
        } else {
            serialize_line_named(&l, name)
        }
    }

    /// `p:spPr` (or `p:grpSpPr` with `group`): transform, geometry, fill, outline, effects and
    /// the kept 3-D and extension elements.
    fn sp_pr(
        &mut self,
        name: &str,
        xfrm: Option<XmlElement>,
        props: &ShapeProps,
        kept: &[String],
        group: bool,
    ) -> XmlElement {
        let mut el = XmlElement::new(name).with_opt_child(xfrm);
        if !group && let Some(g) = &props.geometry {
            el = el.with_child(geometry_element(g));
        }
        if let Some(f) = &props.fill {
            let f = self.fill_element(f);
            el = el.with_child(f);
        }
        if !group && let Some(l) = &props.line {
            let l = self.line_element(l, "a:ln");
            el = el.with_child(l);
        }
        if let Some(fx) = &props.effects {
            el = el.with_child(serialize_effect_list(fx));
        }
        for x in kept {
            if let Some(node) = self.kept(x) {
                el.children.push(node);
            }
        }
        el
    }

    /// The shape's kept XML split into non-visual and shape-property fragments.
    fn kept_xml(&self, id: NodeId) -> (Vec<String>, Vec<String>) {
        let mut nv = Vec::new();
        let mut sp = Vec::new();
        for x in self.extras.shape_xml.get(&id).into_iter().flatten() {
            if is_nv_fragment(x) {
                nv.push(x.clone());
            } else {
                sp.push(x.clone());
            }
        }
        (nv, sp)
    }

    /// A shape list with the kept shapes filed under `parent` in their places.
    pub(crate) fn shape_list(&mut self, parent: NodeId, shapes: &[Shape]) -> Vec<XmlNode> {
        let mut opaque: Vec<(usize, String)> = self
            .extras
            .opaque_shapes
            .iter()
            .filter(|o| o.parent == parent)
            .map(|o| (o.index, o.xml.clone()))
            .collect();
        opaque.sort_by_key(|(i, _)| *i);
        let mut out = Vec::new();
        let mut k = 0;
        for (i, s) in shapes.iter().enumerate() {
            while k < opaque.len() && opaque[k].0 <= i {
                if let Some(node) = self.kept(&opaque[k].1) {
                    out.push(node);
                }
                k += 1;
            }
            if let Some(n) = self.shape(s) {
                out.push(n);
            }
        }
        while k < opaque.len() {
            if let Some(node) = self.kept(&opaque[k].1) {
                out.push(node);
            }
            k += 1;
        }
        out
    }

    /// `p:spTree` of a slide, layout, master or notes page.
    pub(crate) fn sp_tree(&mut self, owner: NodeId, shapes: &[Shape]) -> XmlElement {
        let zero = Transform::default();
        let mut tree = XmlElement::new("p:spTree")
            .with_child(
                XmlElement::new("p:nvGrpSpPr")
                    .with_child(
                        XmlElement::new("p:cNvPr")
                            .with_attr("id", 1)
                            .with_attr("name", ""),
                    )
                    .with_child(XmlElement::new("p:cNvGrpSpPr"))
                    .with_child(XmlElement::new("p:nvPr")),
            )
            .with_child(XmlElement::new("p:grpSpPr").with_child(xfrm_element(
                "a:xfrm",
                &zero,
                Some(&ChildSpace::default()),
            )));
        tree.children.extend(self.shape_list(owner, shapes));
        tree
    }

    /// One shape (`None` when it cannot be written; reported).
    pub(crate) fn shape(&mut self, s: &Shape) -> Option<XmlNode> {
        if let Some(ShapeSource::SmartArt { xml } | ShapeSource::Ole { xml }) =
            self.extras.source_of(s.id)
        {
            return self.kept_frame(s, xml);
        }
        let el = match &s.kind {
            ShapeKind::Geometry(a) => self.sp(s, a),
            ShapeKind::Picture(p) => self.pic(s, p),
            ShapeKind::Media(m) => self.media(s, m),
            ShapeKind::Group(g) => self.group(s, g),
            ShapeKind::Table(t) => self.table(s, t),
            ShapeKind::Chart(c) => self.chart(s, c)?,
            ShapeKind::Connector(c) => self.connector(s, c),
        };
        if let Some(ShapeSource::AlternateContent { xml }) = self.extras.source_of(s.id) {
            let xml = xml.clone();
            return Some(self.alternate(&xml, el));
        }
        Some(XmlNode::Element(el))
    }

    /// Puts a regenerated shape back into the `mc:AlternateContent` it was read from, in the
    /// branch the reader chose; the other branches stay as they were.
    fn alternate(&mut self, xml: &str, el: XmlElement) -> XmlNode {
        let Some(mut ac) = parse_kept(xml) else {
            return XmlNode::Element(el);
        };
        let chosen = ac.children.iter().position(|c| {
            matches!(c, XmlNode::Element(e) if e.name == "mc:Choice"
                && e.attr("Requires").unwrap_or("").split_whitespace().all(|p| SUPPORTED_MC.contains(&p)))
        });
        let branch = chosen.or_else(|| {
            ac.children
                .iter()
                .position(|c| matches!(c, XmlNode::Element(e) if e.name == "mc:Fallback"))
        });
        let Some(i) = branch else {
            return XmlNode::Element(el);
        };
        if let Some(b) = ac.children[i].as_element_mut() {
            match b.children.iter().position(|c| c.as_element().is_some()) {
                Some(j) => b.children[j] = XmlNode::Element(el),
                None => b.children.push(XmlNode::Element(el)),
            }
        }
        let node = XmlNode::Element(ac);
        node_refs(&node, &mut self.refs);
        node
    }

    /// SmartArt and OLE frames: the original XML with the model's position and name.
    fn kept_frame(&mut self, s: &Shape, xml: &str) -> Option<XmlNode> {
        let Some(mut root) = parse_kept(xml) else {
            return self.kept(xml);
        };
        let file_id = self.ids.get(&s.id).copied();
        let mut stack: Vec<&mut XmlElement> = vec![&mut root];
        while let Some(e) = stack.pop() {
            match local_name(&e.name) {
                "graphicFrame" | "pic" if e.name.starts_with("p:") => {
                    for c in e.elements_mut() {
                        let n = local_name(&c.name).to_owned();
                        if n.starts_with("nv")
                            && let Some(c_nv) = c.child_mut("p:cNvPr")
                        {
                            c_nv.set_attr("name", Some(s.name.as_str()));
                            c_nv.set_attr("descr", s.alt_text.as_deref());
                            c_nv.set_attr("hidden", s.hidden.then_some("1"));
                            c_nv.set_attr("title", s.title.as_deref());
                            if let Some(id) = file_id {
                                c_nv.set_attr("id", Some(id));
                            }
                        }
                        if let Some(x) = &s.xfrm {
                            if c.name == "p:xfrm" {
                                *c = xfrm_element("p:xfrm", x, None);
                            } else if c.name == "p:spPr"
                                && let Some(ax) = c.child_mut("a:xfrm")
                            {
                                *ax = xfrm_element("a:xfrm", x, None);
                            }
                        }
                    }
                    if e.name == "p:graphicFrame" {
                        // An OLE object's preview picture lives inside it.
                        stack.extend(e.elements_mut().filter(|c| c.name == "a:graphic"));
                    }
                }
                _ => stack.extend(e.elements_mut()),
            }
        }
        let node = XmlNode::Element(root);
        node_refs(&node, &mut self.refs);
        Some(node)
    }

    fn sp(&mut self, s: &Shape, a: &AutoShape) -> XmlElement {
        let (kept_nv, kept_sp) = self.kept_xml(s.id);
        let c_nv = self.c_nv_pr(s);
        let mut c_nv_sp =
            XmlElement::new("p:cNvSpPr").with_opt_attr("txBox", a.text_box.then_some("1"));
        if s.placeholder.is_some() {
            c_nv_sp = c_nv_sp.with_child(XmlElement::new("a:spLocks").with_attr("noGrp", "1"));
        }
        let nv_pr = self.nv_pr(s, Vec::new(), &kept_nv);
        let xfrm = s.xfrm.as_ref().map(|x| xfrm_element("a:xfrm", x, None));
        let sp_pr = self.sp_pr("p:spPr", xfrm, &a.props, &kept_sp, false);
        let mut el = XmlElement::new("p:sp")
            .with_child(
                XmlElement::new("p:nvSpPr")
                    .with_child(c_nv)
                    .with_child(c_nv_sp)
                    .with_child(nv_pr),
            )
            .with_child(sp_pr)
            .with_opt_child(a.props.style.as_ref().map(style_element));
        if let Some(t) = &a.text {
            let body = self.text_body("p:txBody", t);
            el = el.with_child(body);
        }
        el
    }

    /// `p:blipFill` for an image part key (empty `a:blip` when there is none).
    fn blip_fill(
        &mut self,
        key: Option<&str>,
        crop: Option<&kiln_deck::Crop>,
        alpha: Option<u32>,
    ) -> XmlElement {
        let mut blip = XmlElement::new("a:blip");
        if let Some(key) = key.filter(|k| !k.is_empty()) {
            match self.ctx.media_part(key) {
                Some(target) => {
                    let id = self.ctx.rel(&self.part, rel::IMAGE, &target, false);
                    blip = blip.with_attr("r:embed", id.as_str());
                }
                None => self.ctx.issue(
                    "image.missing",
                    format!("A picture's image ({key}) is missing."),
                    &self.part.clone(),
                ),
            }
        }
        if let Some(a) = alpha {
            blip = blip.with_child(XmlElement::new("a:alphaModFix").with_attr("amt", a));
        }
        let src = crop.filter(|c| !c.is_none()).map(|c| {
            XmlElement::new("a:srcRect")
                .with_opt_attr("l", (c.l != 0).then_some(c.l))
                .with_opt_attr("t", (c.t != 0).then_some(c.t))
                .with_opt_attr("r", (c.r != 0).then_some(c.r))
                .with_opt_attr("b", (c.b != 0).then_some(c.b))
        });
        XmlElement::new("p:blipFill")
            .with_child(blip)
            .with_opt_child(src)
            .with_child(XmlElement::new("a:stretch").with_child(XmlElement::new("a:fillRect")))
    }

    fn pic_frame(
        &mut self,
        s: &Shape,
        media: Vec<XmlElement>,
        kept_nv: &[String],
        blip_fill: XmlElement,
        props: &ShapeProps,
        kept_sp: &[String],
    ) -> XmlElement {
        let c_nv = self.c_nv_pr(s);
        let nv_pr = self.nv_pr(s, media, kept_nv);
        let xfrm = s.xfrm.as_ref().map(|x| xfrm_element("a:xfrm", x, None));
        let sp_pr = self.sp_pr("p:spPr", xfrm, props, kept_sp, false);
        XmlElement::new("p:pic")
            .with_child(
                XmlElement::new("p:nvPicPr")
                    .with_child(c_nv)
                    .with_child(
                        XmlElement::new("p:cNvPicPr").with_child(
                            XmlElement::new("a:picLocks").with_attr("noChangeAspect", "1"),
                        ),
                    )
                    .with_child(nv_pr),
            )
            .with_child(blip_fill)
            .with_child(sp_pr)
            .with_opt_child(props.style.as_ref().map(style_element))
    }

    fn pic(&mut self, s: &Shape, p: &Picture) -> XmlElement {
        let (kept_nv, kept_sp) = self.kept_xml(s.id);
        let blip = self.blip_fill(Some(&p.media), Some(&p.crop), p.alpha);
        self.pic_frame(s, Vec::new(), &kept_nv, blip, &p.props, &kept_sp)
    }

    fn media(&mut self, s: &Shape, m: &Media) -> XmlElement {
        let (kept_nv, kept_sp) = self.kept_xml(s.id);
        let (file_rel, file_name) = match m.kind {
            MediaKind::Video => (rel::VIDEO, "a:videoFile"),
            MediaKind::Audio => (rel::AUDIO, "a:audioFile"),
        };
        // a:videoFile/@r:link and p14:media/@r:embed (or r:link for a linked clip).
        let (file_id, media_attr) = match &m.source {
            MediaSource::Embedded(key) => match self.ctx.media_part(key) {
                Some(target) => (
                    Some(self.ctx.rel(&self.part, file_rel, &target, false)),
                    Some((
                        "r:embed",
                        self.ctx.rel(&self.part, rel::MEDIA, &target, false),
                    )),
                ),
                None => {
                    self.ctx.issue(
                        "media.missing",
                        format!("A clip ({key}) is missing; it is saved without its file."),
                        &self.part.clone(),
                    );
                    (None, None)
                }
            },
            MediaSource::Linked(url) if !url.is_empty() => {
                let file = self.ctx.rel(&self.part, file_rel, url, true);
                // A linked clip carries the PowerPoint 2010 extension only when it had one.
                let had_ext = kept_nv.iter().any(|x| x.contains(uri::MEDIA_EXT));
                let ext =
                    had_ext.then(|| ("r:link", self.ctx.rel(&self.part, rel::MEDIA, url, true)));
                (Some(file), ext)
            }
            MediaSource::Linked(_) => {
                self.ctx.issue(
                    "media.missing",
                    "A linked clip has no file name; it is saved without its file.",
                    &self.part.clone(),
                );
                (None, None)
            }
        };
        let file = XmlElement::new(file_name).with_opt_attr("r:link", file_id.as_deref());
        // The p14:media extension: in the kept p:extLst when there is one, else a new one.
        let p14 = media_attr.map(|(attr, id)| {
            let trim = (m.trim_start_ms != 0 || m.trim_end_ms != 0).then(|| {
                XmlElement::new("p14:trim")
                    .with_opt_attr("st", (m.trim_start_ms != 0).then_some(m.trim_start_ms))
                    .with_opt_attr("end", (m.trim_end_ms != 0).then_some(m.trim_end_ms))
            });
            XmlElement::new("p14:media")
                .with_attr("xmlns:p14", NS_P14)
                .with_attr(attr, id.as_str())
                .with_opt_child(trim)
        });
        let mut kept_nv_out: Vec<String> = Vec::new();
        let mut ext_list: Option<XmlElement> = None;
        for x in kept_nv {
            if fragment_root(&x) == "extLst"
                && ext_list.is_none()
                && let Some(e) = parse_kept(&x)
            {
                ext_list = Some(e);
            } else {
                kept_nv_out.push(x);
            }
        }
        let mut ext_list = ext_list.unwrap_or_else(|| XmlElement::new("p:extLst"));
        ext_list
            .children
            .retain(|c| !matches!(c, XmlNode::Element(e) if e.attr("uri") == Some(uri::MEDIA_EXT)));
        if let Some(p14) = p14 {
            ext_list.children.insert(
                0,
                XmlNode::Element(
                    XmlElement::new("p:ext")
                        .with_attr("uri", uri::MEDIA_EXT)
                        .with_child(p14),
                ),
            );
        }
        let mut media_children = vec![file];
        let has_ext = ext_list.elements().next().is_some();
        let blip = self.blip_fill(m.poster.as_deref(), None, None);
        let mut el = self.pic_frame(s, Vec::new(), &kept_nv_out, blip, &m.props, &kept_sp);
        // Media files go right after the placeholder, the extension list last.
        if let Some(nv_pr) = el
            .child_mut("p:nvPicPr")
            .and_then(|n| n.child_mut("p:nvPr"))
        {
            let at = usize::from(s.placeholder.is_some());
            for (k, f) in media_children.drain(..).enumerate() {
                nv_pr.children.insert(at + k, XmlNode::Element(f));
            }
            if has_ext {
                node_refs(&XmlNode::Element(ext_list.clone()), &mut self.refs);
                nv_pr.children.push(XmlNode::Element(ext_list));
            }
        }
        el
    }

    fn group(&mut self, s: &Shape, g: &Group) -> XmlElement {
        let (kept_nv, kept_sp) = self.kept_xml(s.id);
        let c_nv = self.c_nv_pr(s);
        let nv_pr = self.nv_pr(s, Vec::new(), &kept_nv);
        let xfrm = s
            .xfrm
            .as_ref()
            .map(|x| xfrm_element("a:xfrm", x, Some(&g.child_space)));
        let gp = self.sp_pr("p:grpSpPr", xfrm, &g.props, &kept_sp, true);
        let mut el = XmlElement::new("p:grpSp")
            .with_child(
                XmlElement::new("p:nvGrpSpPr")
                    .with_child(c_nv)
                    .with_child(XmlElement::new("p:cNvGrpSpPr"))
                    .with_child(nv_pr),
            )
            .with_child(gp);
        el.children.extend(self.shape_list(s.id, &g.children));
        el
    }

    /// `p:graphicFrame` around graphic data.
    fn frame(&mut self, s: &Shape, data: XmlElement, locks: bool) -> XmlElement {
        let (kept_nv, _) = self.kept_xml(s.id);
        let c_nv = self.c_nv_pr(s);
        let nv_pr = self.nv_pr(s, Vec::new(), &kept_nv);
        let x = s
            .xfrm
            .or_else(|| self.ctx.pres.resolved_xfrm(s.id))
            .unwrap_or_default();
        let mut c_nv_frame = XmlElement::new("p:cNvGraphicFramePr");
        if locks {
            c_nv_frame = c_nv_frame
                .with_child(XmlElement::new("a:graphicFrameLocks").with_attr("noGrp", "1"));
        }
        XmlElement::new("p:graphicFrame")
            .with_child(
                XmlElement::new("p:nvGraphicFramePr")
                    .with_child(c_nv)
                    .with_child(c_nv_frame)
                    .with_child(nv_pr),
            )
            .with_child(xfrm_element("p:xfrm", &x, None))
            .with_child(XmlElement::new("a:graphic").with_child(data))
    }

    fn table(&mut self, s: &Shape, t: &Table) -> XmlElement {
        let look = &t.look;
        let flag = |on: bool| on.then_some("1");
        let pr = XmlElement::new("a:tblPr")
            .with_opt_attr("rtl", flag(t.rtl))
            .with_opt_attr("firstRow", flag(look.first_row))
            .with_opt_attr("firstCol", flag(look.first_col))
            .with_opt_attr("lastRow", flag(look.last_row))
            .with_opt_attr("lastCol", flag(look.last_col))
            .with_opt_attr("bandRow", flag(look.band_row))
            .with_opt_attr("bandCol", flag(look.band_col))
            .with_opt_child(
                t.style_id
                    .as_ref()
                    .map(|id| XmlElement::new("a:tableStyleId").with_text(id.as_str())),
            );
        let grid = XmlElement::new("a:tblGrid").with_children(
            t.grid
                .iter()
                .map(|w| XmlElement::new("a:gridCol").with_attr("w", *w)),
        );
        let mut tbl = XmlElement::new("a:tbl").with_child(pr).with_child(grid);
        for row in &t.rows {
            let mut tr = XmlElement::new("a:tr").with_attr("h", row.height);
            for cell in &row.cells {
                let body = self.text_body("a:txBody", &cell.text);
                let p = &cell.props;
                let mut tc_pr = XmlElement::new("a:tcPr")
                    .with_opt_attr("marL", p.margins[0])
                    .with_opt_attr("marR", p.margins[2])
                    .with_opt_attr("marT", p.margins[1])
                    .with_opt_attr("marB", p.margins[3])
                    .with_opt_attr(
                        "vert",
                        p.direction.map(kiln_deck::text::TextDirection::as_str),
                    )
                    .with_opt_attr("anchor", p.anchor.map(kiln_deck::TextAnchor::as_str));
                for (b, name) in p.borders.iter().zip(["a:lnL", "a:lnR", "a:lnT", "a:lnB"]) {
                    if let Some(l) = &b.0 {
                        let l = self.line_element(l, name);
                        tc_pr = tc_pr.with_child(l);
                    }
                }
                if let Some(f) = &p.fill {
                    let f = self.fill_element(f);
                    tc_pr = tc_pr.with_child(f);
                }
                let tc = XmlElement::new("a:tc")
                    .with_opt_attr("gridSpan", (cell.grid_span > 1).then_some(cell.grid_span))
                    .with_opt_attr("rowSpan", (cell.row_span > 1).then_some(cell.row_span))
                    .with_opt_attr("hMerge", flag(cell.h_merge))
                    .with_opt_attr("vMerge", flag(cell.v_merge))
                    .with_child(body)
                    .with_child(tc_pr);
                tr = tr.with_child(tc);
            }
            tbl = tbl.with_child(tr);
        }
        let data = XmlElement::new("a:graphicData")
            .with_attr("uri", uri::TABLE)
            .with_child(tbl);
        self.frame(s, data, true)
    }

    fn chart(&mut self, s: &Shape, c: &ChartRef) -> Option<XmlElement> {
        let ex = self
            .ctx
            .pres
            .parts
            .get(&c.part)
            .is_some_and(|p| p.content_type.contains("chartex"));
        let Some(target) = self.ctx.media_part(&c.part) else {
            self.ctx.issue(
                "chart.missing",
                format!(
                    "A chart's part ({}) is missing; the chart is left out.",
                    c.part
                ),
                &self.part.clone(),
            );
            return None;
        };
        let (rel_type, data_uri, el) = if ex {
            (
                rel::CHART_EX,
                uri::CHART_EX,
                XmlElement::new("cx:chart").with_attr("xmlns:cx", NS_CX),
            )
        } else {
            (
                rel::CHART,
                uri::CHART,
                XmlElement::new("c:chart").with_attr("xmlns:c", NS_C),
            )
        };
        let id = self.ctx.rel(&self.part, rel_type, &target, false);
        let data = XmlElement::new("a:graphicData")
            .with_attr("uri", data_uri)
            .with_child(el.with_attr("r:id", id.as_str()));
        Some(self.frame(s, data, false))
    }

    fn connector(&mut self, s: &Shape, c: &Connector) -> XmlElement {
        let (kept_nv, kept_sp) = self.kept_xml(s.id);
        let c_nv = self.c_nv_pr(s);
        let end = |w: &Self, name: &str, e: Option<&kiln_deck::Connection>| {
            e.and_then(|e| {
                w.ids.get(&e.shape).map(|id| {
                    XmlElement::new(name)
                        .with_attr("id", *id)
                        .with_attr("idx", e.site)
                })
            })
        };
        let c_nv_cxn = XmlElement::new("p:cNvCxnSpPr")
            .with_opt_child(end(self, "a:stCxn", c.start.as_ref()))
            .with_opt_child(end(self, "a:endCxn", c.end.as_ref()));
        let nv_pr = self.nv_pr(s, Vec::new(), &kept_nv);
        let xfrm = s.xfrm.as_ref().map(|x| xfrm_element("a:xfrm", x, None));
        let sp_pr = self.sp_pr("p:spPr", xfrm, &c.props, &kept_sp, false);
        XmlElement::new("p:cxnSp")
            .with_child(
                XmlElement::new("p:nvCxnSpPr")
                    .with_child(c_nv)
                    .with_child(c_nv_cxn)
                    .with_child(nv_pr),
            )
            .with_child(sp_pr)
            .with_opt_child(c.props.style.as_ref().map(style_element))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_are_classified_by_their_root() {
        assert!(is_nv_fragment("<p:extLst><p:ext/></p:extLst>"));
        assert!(is_nv_fragment("<p:custDataLst/>"));
        assert!(!is_nv_fragment("<a:extLst/>"));
        assert!(!is_nv_fragment("<a:scene3d/>"));
        assert_eq!(fragment_root("<p:extLst xmlns:p=\"x\">"), "extLst");
        assert_eq!(fragment_root("<a:sp3d/>"), "sp3d");
    }

    #[test]
    fn transforms_and_placeholders_write_like_powerpoint() {
        let x = Transform {
            x: 1,
            y: 2,
            cx: 3,
            cy: 4,
            rot: 5_400_000,
            flip_h: true,
            flip_v: false,
        };
        let e = xfrm_element("a:xfrm", &x, None);
        assert_eq!(e.attr("rot"), Some("5400000"));
        assert_eq!(e.attr("flipH"), Some("1"));
        assert!(e.attr("flipV").is_none());
        let ph = placeholder_element(&Placeholder::new(PlaceholderType::Object, 1));
        assert!(ph.attr("type").is_none());
        assert_eq!(ph.attr("idx"), Some("1"));
        let t = placeholder_element(&Placeholder::new(PlaceholderType::Title, 0));
        assert_eq!(t.attr("type"), Some("title"));
        assert!(t.attr("idx").is_none());
    }
}
