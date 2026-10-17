// SPDX-License-Identifier: AGPL-3.0-or-later
//! XML fragments for what the model keeps verbatim (an unknown transition, an animation Kiln does
//! not play, SmartArt and OLE frames, extension lists) and small attribute helpers.
//!
//! Fragments are self-contained: every prefix they use is declared on their root element, from
//! Office's stable prefix table or, for prefixes outside it, from the part's root element. So a
//! fragment parses on its own and can be embedded anywhere.

use kiln_ooxml::xml::namespaces::namespace_for_prefix;
use kiln_ooxml::xml::{
    Attrs, Declaration, WriteOptions, XmlElement, XmlParseError, escape_attr, parse_xml_str,
    undeclared_prefixes, write_element,
};

/// Serialises `node` as a self-contained fragment (no XML declaration). Prefixes outside Office's
/// table are declared from `scope` (normally the part root's attributes). The tree is neither
/// cloned nor walked recursively, so arbitrarily deep content is safe on a small stack.
#[must_use]
pub fn fragment(node: &XmlElement, scope: &Attrs) -> String {
    let mut decls = String::new();
    for p in undeclared_prefixes(node) {
        let key = format!("xmlns:{p}");
        let uri = namespace_for_prefix(&p).or_else(|| scope.get(&key));
        if let Some(uri) = uri {
            decls.push(' ');
            decls.push_str(&key);
            decls.push_str("=\"");
            decls.push_str(&escape_attr(uri));
            decls.push('"');
        }
    }
    let options = WriteOptions {
        declaration: Declaration::Omit,
        declare_namespaces: false,
    };
    // Writing an in-memory tree only fails for names XML cannot represent, which the parser
    // never produces; an empty fragment is the safe answer.
    let mut xml = write_element(node, &options).unwrap_or_default();
    if !decls.is_empty() && xml.starts_with('<') {
        let at = 1 + node.name.len();
        if xml.is_char_boundary(at) {
            xml.insert_str(at, &decls);
        }
    }
    xml
}

/// Parses a fragment made by [`fragment`].
///
/// # Errors
/// [`XmlParseError`] for malformed XML.
pub fn parse_fragment(xml: &str) -> Result<XmlElement, XmlParseError> {
    parse_xml_str(xml).map(|d| d.root)
}

/// An unsigned attribute that fits `u32`.
#[must_use]
pub fn attr_u32(e: &XmlElement, name: &str) -> Option<u32> {
    e.attr_int(name).and_then(|v| u32::try_from(v).ok())
}

/// A signed attribute that fits `i32`.
#[must_use]
pub fn attr_i32(e: &XmlElement, name: &str) -> Option<i32> {
    e.attr_int(name).and_then(|v| i32::try_from(v).ok())
}

/// A boolean attribute with its schema default.
#[must_use]
pub fn attr_bool_or(e: &XmlElement, name: &str, default: bool) -> bool {
    e.attr_bool(name).unwrap_or(default)
}

/// The local part of an element name (`p:sp` → `sp`, `dsp:sp` → `sp`).
#[must_use]
pub fn local(e: &XmlElement) -> &str {
    kiln_ooxml::xml::namespaces::local_name(&e.name)
}

/// The first child with this local name, whatever its prefix (SmartArt drawings use `dsp:`,
/// which Office's prefix table does not normalise).
#[must_use]
pub fn child_local<'a>(e: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    e.elements().find(|c| local(c) == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn fragments_declare_their_prefixes_and_parse_alone() {
        let doc = parse_xml_str(
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:zz="urn:zz"><p:cSld><p:spTree><zz:thing a:x="1"><p:fade/></zz:thing></p:spTree></p:cSld></p:sld>"#,
        )
        .unwrap();
        let thing = doc
            .root
            .child_path(&["p:cSld", "p:spTree"])
            .unwrap()
            .first_element()
            .unwrap();
        let xml = fragment(thing, &doc.root.attrs);
        assert!(xml.contains("xmlns:zz=\"urn:zz\""), "{xml}");
        assert!(xml.contains("xmlns:p="), "{xml}");
        let back = parse_fragment(&xml).unwrap();
        assert_eq!(back.name, "zz:thing");
        assert_eq!(back.attr("a:x"), Some("1"));
        assert_eq!(local(&back), "thing");
        assert!(child_local(&back, "fade").is_some());
        assert_eq!(attr_u32(&back, "a:x"), Some(1));
        assert_eq!(attr_i32(&back, "nope"), None);
        assert!(attr_bool_or(&back, "nope", true));
    }
}
