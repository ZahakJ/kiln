// SPDX-License-Identifier: AGPL-3.0-or-later
//! The streaming layer under the worksheet and shared-string readers: `quick-xml` events over
//! the part text, entity and character references resolved, small subtrees (`<is>`, `<si>`)
//! built into elements on demand. DTDs are refused, as everywhere in Kiln.

use std::borrow::Cow;

use kiln_ooxml::xml::{XmlElement, XmlNode};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::XlsxError;

/// The local part of a qualified name (`x:c` → `c`).
pub(crate) fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

/// A reader over part text that keeps text exactly (no trimming).
pub(crate) fn reader(text: &str) -> Reader<&[u8]> {
    let mut r = Reader::from_str(text);
    let cfg = r.config_mut();
    cfg.trim_text(false);
    cfg.expand_empty_elements = false;
    r
}

pub(crate) fn xml_error(part: &str, r: &Reader<&[u8]>, e: impl std::fmt::Display) -> XlsxError {
    XlsxError::Xml {
        part: part.to_owned(),
        message: format!("at byte {}: {e}", r.error_position()),
    }
}

/// An attribute value with references resolved.
pub(crate) fn attr_value<'a>(a: &quick_xml::events::attributes::Attribute<'a>) -> Cow<'a, str> {
    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .unwrap_or(Cow::Borrowed(""))
}

/// The resolved text of a general reference (`&amp;`, `&#10;`); unknown entities are refused.
pub(crate) fn resolve_ref(
    part: &str,
    r: &Reader<&[u8]>,
    gr: &quick_xml::events::BytesRef<'_>,
) -> Result<String, XlsxError> {
    match gr.resolve_char_ref() {
        Ok(Some(c)) => Ok(c.to_string()),
        Ok(None) => quick_xml::escape::resolve_predefined_entity(gr)
            .map(str::to_owned)
            .ok_or_else(|| xml_error(part, r, format!("unknown entity &{};", gr.as_ref()))),
        Err(e) => Err(xml_error(part, r, e)),
    }
}

/// Element attributes as (qualified name, value) pairs.
pub(crate) fn attrs_of(s: &BytesStart<'_>) -> Vec<(String, String)> {
    s.attributes()
        .flatten()
        .map(|a| (a.key.0.to_owned(), attr_value(&a).into_owned()))
        .collect()
}

fn element_of(s: &BytesStart<'_>) -> XmlElement {
    let mut e = XmlElement::new(local(s.name().0));
    for (k, v) in attrs_of(s) {
        e.attrs.set(k, v);
    }
    e
}

/// Reads the rest of an element whose start tag was just read into a tree (names without
/// prefixes). Comments and processing instructions are dropped.
pub(crate) fn subtree(
    part: &str,
    r: &mut Reader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<XmlElement, XlsxError> {
    let mut stack = vec![element_of(start)];
    loop {
        let ev = r.read_event().map_err(|e| xml_error(part, r, e))?;
        match ev {
            Event::Start(s) => {
                if stack.len() > 256 {
                    return Err(xml_error(part, r, "elements nested too deeply"));
                }
                stack.push(element_of(&s));
            }
            Event::Empty(s) => {
                let e = element_of(&s);
                if let Some(top) = stack.last_mut() {
                    top.children.push(XmlNode::Element(e));
                }
            }
            Event::End(_) => {
                let Some(done) = stack.pop() else {
                    return Err(xml_error(part, r, "unbalanced end tag"));
                };
                match stack.last_mut() {
                    Some(top) => top.children.push(XmlNode::Element(done)),
                    None => return Ok(done),
                }
            }
            Event::Text(t) => push_text(&mut stack, &t.xml10_content()),
            Event::CData(t) => push_text(&mut stack, &t.xml10_content()),
            Event::GeneralRef(g) => {
                let s = resolve_ref(part, r, &g)?;
                push_text(&mut stack, &s);
            }
            Event::DocType(_) => return Err(xml_error(part, r, "DTDs are not allowed")),
            Event::Eof => return Err(xml_error(part, r, "unexpected end of the part")),
            _ => {}
        }
    }
}

fn push_text(stack: &mut [XmlElement], text: &str) {
    let Some(top) = stack.last_mut() else { return };
    if let Some(XmlNode::Text(last)) = top.children.last_mut() {
        last.text.push_str(text);
    } else {
        top.children.push(XmlNode::text(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtrees_and_text_resolve_references() {
        let xml = "<is><r><rPr><b/></rPr><t xml:space=\"preserve\">a &amp; b&#10;</t></r></is>";
        let mut r = reader(xml);
        let Ok(Event::Start(s)) = r.read_event() else {
            panic!("start");
        };
        let s = s.into_owned();
        let e = subtree("t", &mut r, &s).expect("subtree");
        assert_eq!(e.name, "is");
        assert_eq!(e.text(), "a & b\n");
    }
}
