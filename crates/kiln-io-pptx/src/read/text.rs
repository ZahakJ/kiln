// SPDX-License-Identifier: AGPL-3.0-or-later
//! `p:txBody`/`a:txBody` → [`TextBody`]: body properties, the list style, paragraphs with their
//! DrawingML extras, runs, line breaks, fields, hyperlinks and equations.

use std::collections::BTreeMap;

use kiln_common::NodeId;
use kiln_deck::dml::text::{
    parse_body_props, parse_list_style, parse_paragraph_props, parse_run_props,
};
use kiln_deck::text::LEVELS;
use kiln_deck::{Bullet, DeckParaProps, ListStyle, TextBody};
use kiln_doc::fields::{Field, FieldForm};
use kiln_doc::st::BreakKind;
use kiln_doc::{Break, Equation, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark};
use kiln_doc::{Paragraph, RunProps};
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::{MediaRef, PartCx};
use crate::common::kind::SUPPORTED_MC;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{fragment, local};

/// Concatenates two fragments.
fn concat(mut a: Fragment, b: Fragment) -> Fragment {
    let base = a.text.chars().count();
    a.text.push_str(&b.text);
    a.runs.extend(b.runs);
    a.marks.extend(b.marks.into_iter().map(|mut m| {
        m.start += base;
        m.end += base;
        m
    }));
    a.objects.extend(b.objects);
    a
}

/// Text as it may appear in `a:t`: the Office escape `_xHHHH_` of control characters is decoded
/// (a vertical tab is a line break PowerPoint wrote as text), other control characters dropped.
fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("_x") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let decoded = tail
            .get(2..6)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .filter(|_| tail.as_bytes().get(6) == Some(&b'_'))
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32);
        if let Some(c) = decoded {
            out.push(c);
            rest = &tail[7..];
        } else {
            out.push_str("_x");
            rest = &tail[2..];
        }
    }
    out.push_str(rest);
    out.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\u{b}'))
        .collect()
}

impl Reader<'_> {
    /// Reads a text body. Never fails: an unusable body becomes one empty paragraph with a
    /// diagnostic.
    pub fn read_text_body(&mut self, cx: &PartCx, tx: &XmlElement, location: &str) -> TextBody {
        let mut paragraphs = Vec::new();
        let mut deck = BTreeMap::new();
        for (n, p) in (1u64..).zip(tx.elements().filter(|e| local(e) == "p")) {
            let id = NodeId(n);
            let (para, extras) = self.read_paragraph(cx, p, id, location);
            if !extras.is_empty() {
                deck.insert(id, extras);
            }
            paragraphs.push(para);
        }
        if paragraphs.is_empty() {
            paragraphs.push(Paragraph::new(NodeId(1)));
        }
        let mut body = match TextBody::from_paragraphs(paragraphs, deck) {
            Ok(b) => b,
            Err(e) => {
                self.warn(
                    WarningCode::ShapeUnknown,
                    Severity::Warning,
                    format!("text that could not be read was dropped ({e})"),
                    &cx.part,
                    location.to_owned(),
                );
                TextBody::new()
            }
        };
        if let Some(b) = tx.elements().find(|e| local(e) == "bodyPr") {
            body.body = parse_body_props(b);
        }
        if let Some(l) = tx.elements().find(|e| local(e) == "lstStyle") {
            body.list_style = self.read_list_style(cx, l);
        }
        body
    }

    /// Reads a list style (`a:lstStyle`, the master's text styles), loading picture bullets.
    pub fn read_list_style(&mut self, cx: &PartCx, e: &XmlElement) -> ListStyle {
        let mut s = parse_list_style(e, cx.color.as_ref());
        let mut def = s.default_level().cloned();
        if let Some(d) = def.as_mut() {
            self.fix_bullet(cx, &mut d.deck);
        }
        s.set_default_level(def);
        for i in 0..LEVELS {
            if let Some(mut l) = s.get_level(i).cloned() {
                self.fix_bullet(cx, &mut l.deck);
                s.set_level(i, l);
            }
        }
        s
    }

    fn fix_bullet(&mut self, cx: &PartCx, d: &mut DeckParaProps) {
        if let Some(Bullet::Picture(id)) = &d.bullet {
            d.bullet = match self.media(cx, &id.clone()) {
                MediaRef::Part(key) => Some(Bullet::Picture(key)),
                // A bullet picture that is not there: PowerPoint shows the default bullet.
                _ => None,
            };
        }
    }

    fn read_paragraph(
        &mut self,
        cx: &PartCx,
        p: &XmlElement,
        id: NodeId,
        location: &str,
    ) -> (Paragraph, DeckParaProps) {
        let mut para = Paragraph::new(id);
        let mut extras = DeckParaProps::default();
        if let Some(ppr) = p.child("a:pPr") {
            let (props, mut deck) = parse_paragraph_props(ppr);
            self.fix_bullet(cx, &mut deck);
            para.props = props;
            extras = deck;
        }
        if let Some(end) = p.child("a:endParaRPr") {
            para.mark_props = parse_run_props(end, cx.color.as_ref());
        }
        let mut frag = Fragment::default();
        let children: Vec<&XmlElement> = p.elements().collect();
        self.read_runs(cx, &children, &mut frag, 0);
        match Inlines::from_fragment(frag) {
            Ok(c) => para.content = c,
            Err(e) => self.warn(
                WarningCode::ShapeUnknown,
                Severity::Warning,
                format!("a paragraph's text could not be read ({e})"),
                &cx.part,
                location.to_owned(),
            ),
        }
        (para, extras)
    }

    fn run_props(&mut self, cx: &PartCx, rpr: Option<&XmlElement>) -> (RunProps, Option<Mark>) {
        let Some(rpr) = rpr else {
            return (RunProps::default(), None);
        };
        let props = parse_run_props(rpr, cx.color.as_ref());
        let link = rpr.child("a:hlinkClick").and_then(|h| {
            let url = self.link(cx, h)?;
            Some(Mark::Link(Hyperlink {
                target: LinkTarget::Url(url),
                tooltip: h
                    .attr("tooltip")
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned),
                frame: h
                    .attr("tgtFrame")
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned),
                history: h.attr_bool("history").unwrap_or(true),
            }))
        });
        (props, link)
    }

    fn read_runs(
        &mut self,
        cx: &PartCx,
        children: &[&XmlElement],
        frag: &mut Fragment,
        depth: usize,
    ) {
        for &c in children {
            let piece = match c.name.as_str() {
                "a:r" => {
                    let (props, link) = self.run_props(cx, c.child("a:rPr"));
                    let text =
                        clean_text(&c.child("a:t").map(XmlElement::text).unwrap_or_default());
                    run_fragment(&text, props, link)
                }
                "a:br" => {
                    let (props, link) = self.run_props(cx, c.child("a:rPr"));
                    let f = Fragment::object(
                        InlineObject::Break(Break {
                            kind: BreakKind::TextWrapping,
                            clear: None,
                        }),
                        props,
                    );
                    with_link(f, link)
                }
                "a:fld" => {
                    let (props, link) = self.run_props(cx, c.child("a:rPr"));
                    let field = Field {
                        instr: c.attr("type").unwrap_or_default().to_owned(),
                        result: clean_text(
                            &c.child("a:t").map(XmlElement::text).unwrap_or_default(),
                        ),
                        form: FieldForm::Simple,
                        dirty: false,
                        locked: false,
                    };
                    with_link(Fragment::object(InlineObject::Field(field), props), link)
                }
                "mc:AlternateContent" if depth < 4 => {
                    let chosen = c.select_alternate_content(&SUPPORTED_MC);
                    if let Some(math) = chosen.iter().find(|e| local(e) == "m") {
                        let omml = math
                            .elements()
                            .find(|e| e.name == "m:oMathPara" || e.name == "m:oMath");
                        if let Some(o) = omml {
                            let eq = Equation {
                                omml: fragment(o, &cx.scope),
                                display: o.name == "m:oMathPara",
                            };
                            *frag = concat(
                                std::mem::take(frag),
                                Fragment::object(InlineObject::Equation(eq), RunProps::default()),
                            );
                        }
                        continue;
                    }
                    self.read_runs(cx, &chosen, frag, depth + 1);
                    continue;
                }
                _ => continue,
            };
            *frag = concat(std::mem::take(frag), piece);
        }
    }
}

fn with_link(f: Fragment, link: Option<Mark>) -> Fragment {
    match link {
        Some(m) => f.with_mark(m),
        None => f,
    }
}

/// A text run; vertical tabs (line breaks written as text) become breaks.
fn run_fragment(text: &str, props: RunProps, link: Option<Mark>) -> Fragment {
    let mut out = Fragment::default();
    for (i, part) in text.split('\u{b}').enumerate() {
        if i > 0 {
            out = concat(
                out,
                Fragment::object(
                    InlineObject::Break(Break {
                        kind: BreakKind::TextWrapping,
                        clear: None,
                    }),
                    props.clone(),
                ),
            );
        }
        out = concat(out, Fragment::text(part, props.clone()));
    }
    with_link(out, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_escapes_are_decoded() {
        assert_eq!(clean_text("a_x000B_b"), "a\u{b}b");
        assert_eq!(clean_text("_x0009_x"), "\tx");
        assert_eq!(clean_text("keep _x and _xZZZZ_"), "keep _x and _xZZZZ_");
        assert_eq!(clean_text("bell\u{7}"), "bell");
        assert_eq!(clean_text("_x00"), "_x00");
    }

    #[test]
    fn runs_split_at_vertical_tabs() {
        let f = run_fragment("a\u{b}b", RunProps::default(), None);
        assert_eq!(f.objects.len(), 1);
        assert!(f.validate().is_ok());
        assert_eq!(f.len(), 3);
        let empty = run_fragment("", RunProps::default(), None);
        assert!(empty.is_empty());
    }
}
