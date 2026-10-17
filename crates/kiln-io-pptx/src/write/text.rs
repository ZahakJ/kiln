// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`TextBody`] → `p:txBody`/`a:txBody`: body properties, the list style, paragraphs with their
//! DrawingML extras, runs, line breaks, fields, hyperlinks and equations — the inverse of the
//! reader's `text` module.

use kiln_deck::dml::text::{
    body_props_to_element, list_style_to_element, paragraph_props_to_element, run_props_to_element,
};
use kiln_deck::text::LEVELS;
use kiln_deck::{Bullet, DeckParaProps, ListStyle, TextBody};
use kiln_doc::inline::OBJECT_CHAR;
use kiln_doc::props::ColorValue;
use kiln_doc::{DocColor, InlineObject, LinkTarget, Mark, Paragraph, RunProps};
use kiln_ooxml::drawingml::color::{DrawingColor, resolve_color, serialize_color};
use kiln_ooxml::xml::{XmlElement, XmlNode};

use super::shapes::PartWriter;
use super::xml::{NS_A14, encode_text, kept_node, stable_guid};
use crate::common::kind::rel;

/// Child elements of `a:rPr` that come after `a:hlinkClick` in the schema.
const AFTER_HLINK: [&str; 3] = ["a:hlinkMouseOver", "a:rtl", "a:extLst"];

impl PartWriter<'_, '_> {
    /// Writes a text body as `name` (`p:txBody` in shapes, `a:txBody` in table cells).
    pub(crate) fn text_body(&mut self, name: &str, body: &TextBody) -> XmlElement {
        let mut el = XmlElement::new(name)
            .with_child(body_props_to_element(&body.body))
            .with_child(self.list_style("a:lstStyle", &body.list_style));
        let mut any = false;
        for p in body.paragraphs() {
            let props = body.deck_props(p.id);
            el = el.with_child(self.paragraph(p, props));
            any = true;
        }
        if !any {
            el = el.with_child(XmlElement::new("a:p"));
        }
        el
    }

    /// A list style with picture bullets pointing at relationships of this part.
    pub(crate) fn list_style(&mut self, name: &str, style: &ListStyle) -> XmlElement {
        let mut s = style.clone();
        if let Some(mut d) = s.default_level().cloned() {
            self.bullet_rel(&mut d.deck);
            s.set_default_level(Some(d));
        }
        for i in 0..LEVELS {
            if let Some(mut l) = s.get_level(i).cloned() {
                self.bullet_rel(&mut l.deck);
                s.set_level(i, l);
            }
        }
        let mut el = list_style_to_element(name, &s);
        // Run colours of each level with their exact rendering.
        if let Some(d) = s.default_level()
            && let Some(lvl) = el.child_mut("a:defPPr")
            && let Some(rpr) = lvl.child_mut("a:defRPr")
        {
            self.exact_run_color(rpr, &d.run);
        }
        for (i, l) in s.levels() {
            if let Some(lvl) = el.child_mut(&format!("a:lvl{}pPr", i + 1))
                && let Some(rpr) = lvl.child_mut("a:defRPr")
            {
                self.exact_run_color(rpr, &l.run);
            }
        }
        el
    }

    /// The colour to write for a theme colour whose transform the model could not keep (Office's
    /// `tint`, `shade`, `satMod`, …): the theme colour with the transform that reproduces the
    /// rendering the reader cached, else that rendering as RGB. `None` when the model's own form
    /// is exact.
    fn exact_color(&mut self, c: &DocColor) -> Option<DrawingColor> {
        let ctx = self.color?;
        let (Some(theme), ColorValue::Rgb(rgb)) = (c.theme_color, c.val) else {
            return None;
        };
        if c.theme_tint.is_some() || c.theme_shade.is_some() {
            return None;
        }
        let name = kiln_deck::dml::color::scheme_name(theme)?;
        let key = format!("{name}:{rgb:?}");
        if let Some(hit) = self.color_cache.get(&key) {
            return hit.clone();
        }
        let base = DrawingColor::scheme(name);
        let renders = |d: &DrawingColor| resolve_color(d, &ctx).to_color() == rgb;
        let found = if renders(&base) {
            None
        } else {
            let steps = (1..100).map(|k| k * 1000);
            steps
                .clone()
                .map(|v| base.clone().with("tint", v))
                .chain(steps.clone().map(|v| base.clone().with("shade", v)))
                .chain(steps.clone().map(|v| base.clone().with("lumMod", v)))
                .chain(steps.map(|v| base.clone().with("lumMod", v).with("lumOff", 100_000 - v)))
                .find(|d| renders(d))
                .or_else(|| {
                    Some(DrawingColor::srgb(&format!(
                        "{:02X}{:02X}{:02X}",
                        rgb.r, rgb.g, rgb.b
                    )))
                })
        };
        self.color_cache.insert(key, found.clone());
        found
    }

    /// Replaces the colour of a run-properties element when the model's form is not exact.
    pub(crate) fn exact_run_color(&mut self, rpr: &mut XmlElement, props: &RunProps) {
        let Some(c) = &props.color else { return };
        let Some(exact) = self.exact_color(c) else {
            return;
        };
        if let Some(fill) = rpr.child_mut("a:solidFill") {
            fill.children = vec![XmlNode::Element(serialize_color(&exact))];
        }
    }

    /// Replaces a picture bullet's part key with a relationship id of this part.
    fn bullet_rel(&mut self, d: &mut DeckParaProps) {
        if let Some(Bullet::Picture(key)) = &d.bullet {
            let key = key.clone();
            d.bullet = match self.ctx.media_part(&key) {
                Some(target) => {
                    let id = self.ctx.rel(&self.part, rel::IMAGE, &target, false);
                    Some(Bullet::Picture(id))
                }
                None => {
                    self.ctx.issue(
                        "image.missing",
                        "A picture bullet's image is missing; the bullet is left out.",
                        &self.part.clone(),
                    );
                    None
                }
            };
        }
    }

    fn paragraph(&mut self, p: &Paragraph, deck: Option<&DeckParaProps>) -> XmlElement {
        let mut el = XmlElement::new("a:p");
        let mut d = deck.cloned().unwrap_or_default();
        self.bullet_rel(&mut d);
        let ppr = paragraph_props_to_element("a:pPr", &p.props, &d, None);
        if !ppr.attrs.is_empty() || !ppr.children.is_empty() {
            el = el.with_child(ppr);
        }
        for node in self.runs(p) {
            el.children.push(node);
        }
        if !p.mark_props.is_empty() {
            let mut end = run_props_to_element("a:endParaRPr", &p.mark_props);
            self.exact_run_color(&mut end, &p.mark_props);
            el = el.with_child(end);
        }
        el
    }

    /// `a:rPr` with a click hyperlink in its schema place.
    fn run_props(&mut self, props: &RunProps, link: Option<&Mark>) -> Option<XmlElement> {
        let mut rpr = run_props_to_element("a:rPr", props);
        self.exact_run_color(&mut rpr, props);
        if let Some(Mark::Link(h)) = link {
            let url = match &h.target {
                LinkTarget::Url(u) => u.clone(),
                LinkTarget::Bookmark(b) => format!("#{b}"),
            };
            if let Some((id, action)) = self.link_target(&url) {
                let mut hl = XmlElement::new("a:hlinkClick")
                    .with_attr("r:id", id.as_str())
                    .with_opt_attr("action", action.as_deref())
                    .with_opt_attr("tgtFrame", h.frame.as_deref())
                    .with_opt_attr("tooltip", h.tooltip.as_deref());
                if !h.history {
                    hl = hl.with_attr("history", "0");
                }
                let at = rpr.children.iter().position(
                    |c| matches!(c, XmlNode::Element(e) if AFTER_HLINK.contains(&e.name.as_str())),
                );
                match at {
                    Some(i) => rpr.children.insert(i, XmlNode::Element(hl)),
                    None => rpr.children.push(XmlNode::Element(hl)),
                }
            }
        }
        (!rpr.attrs.is_empty() || !rpr.children.is_empty()).then_some(rpr)
    }

    /// The runs, breaks, fields and equations of a paragraph.
    fn runs(&mut self, p: &Paragraph) -> Vec<XmlNode> {
        let content = &p.content;
        let chars: Vec<char> = content.raw_text().chars().collect();
        let objects = content.objects();
        // Boundaries: run spans, link marks and every object.
        let mut cuts: Vec<usize> = vec![0, chars.len()];
        let mut pos = 0;
        for r in content.runs() {
            pos += r.len;
            cuts.push(pos);
        }
        for m in content.marks() {
            if matches!(m.mark, Mark::Link(_)) {
                cuts.push(m.start);
                cuts.push(m.end);
            }
        }
        for (i, c) in chars.iter().enumerate() {
            if *c == OBJECT_CHAR {
                cuts.push(i);
                cuts.push(i + 1);
            }
        }
        cuts.retain(|c| *c <= chars.len());
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        let mut object_index = 0usize;
        for w in cuts.windows(2) {
            let (start, end) = (w[0], w[1]);
            if start >= end {
                continue;
            }
            let props = content.props_at(start).cloned().unwrap_or_default();
            let link = content
                .marks()
                .iter()
                .find(|m| matches!(m.mark, Mark::Link(_)) && m.start <= start && end <= m.end)
                .map(|m| &m.mark);
            if chars[start] == OBJECT_CHAR && end == start + 1 {
                let object = objects.get(object_index);
                object_index += 1;
                if let Some(node) = object.and_then(|o| self.object(o, &props, link)) {
                    out.push(node);
                }
                continue;
            }
            let text: String = chars[start..end].iter().collect();
            let mut r = XmlElement::new("a:r");
            if let Some(rpr) = self.run_props(&props, link) {
                r = r.with_child(rpr);
            }
            r = r.with_child(XmlElement::new("a:t").with_text(encode_text(&text)));
            out.push(XmlNode::Element(r));
        }
        out
    }

    fn object(
        &mut self,
        object: &InlineObject,
        props: &RunProps,
        link: Option<&Mark>,
    ) -> Option<XmlNode> {
        Some(match object {
            InlineObject::Break(_) => {
                let mut br = XmlElement::new("a:br");
                if let Some(rpr) = self.run_props(props, link) {
                    br = br.with_child(rpr);
                }
                XmlNode::Element(br)
            }
            InlineObject::Field(f) => {
                self.field_seq += 1;
                let id = stable_guid(&format!("{}#field{}", self.part, self.field_seq));
                let mut fld = XmlElement::new("a:fld")
                    .with_attr("id", id.as_str())
                    .with_opt_attr("type", (!f.instr.is_empty()).then_some(f.instr.as_str()));
                if let Some(rpr) = self.run_props(props, link) {
                    fld = fld.with_child(rpr);
                }
                XmlNode::Element(
                    fld.with_child(XmlElement::new("a:t").with_text(encode_text(&f.result))),
                )
            }
            InlineObject::Equation(eq) => {
                let math = kept_node(&eq.omml)?;
                let mut m = XmlElement::new("a14:m");
                m.children.push(math);
                XmlNode::Element(
                    XmlElement::new("mc:AlternateContent")
                        .with_attr("xmlns:a14", NS_A14)
                        .with_child(
                            XmlElement::new("mc:Choice")
                                .with_attr("Requires", "a14")
                                .with_child(m),
                        ),
                )
            }
            InlineObject::Symbol(s) => {
                let text = char::from_u32(s.code).map(String::from).unwrap_or_default();
                let mut r = XmlElement::new("a:r");
                if let Some(rpr) = self.run_props(props, link) {
                    r = r.with_child(rpr);
                }
                XmlNode::Element(r.with_child(XmlElement::new("a:t").with_text(text)))
            }
            InlineObject::Opaque { xml, .. } => return kept_node(xml),
            // Word-only objects (drawings, note references, bookmarks) have no place in slide
            // text.
            _ => return None,
        })
    }
}
