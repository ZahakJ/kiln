// SPDX-License-Identifier: AGPL-3.0-or-later
//! The XML layer: ODF parts parsed into kiln-ooxml's element tree with every namespace prefix
//! renamed to the canonical ODF prefix (a producer may bind `urn:…:text:1.0` to any prefix, so
//! readers query `text:p` regardless), and a small streaming writer for the parts we produce.
//!
//! Parsing goes through `kiln_ooxml::xml` (no DTDs, no external or unknown entities, a depth
//! limit), with inter-element whitespace kept: in ODF paragraphs a space between two spans is
//! content.

use std::fmt::Write as _;

use kiln_ooxml::xml::{
    ParseOptions, XmlElement, XmlNode, decode_xml_bytes, escape_attr, escape_text, parse_str_with,
};

use crate::OdfError;

/// `(canonical prefix, namespace URI)` of every namespace the reader queries or the writer emits.
pub const NAMESPACES: &[(&str, &str)] = &[
    ("office", "urn:oasis:names:tc:opendocument:xmlns:office:1.0"),
    ("style", "urn:oasis:names:tc:opendocument:xmlns:style:1.0"),
    ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
    ("table", "urn:oasis:names:tc:opendocument:xmlns:table:1.0"),
    ("draw", "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"),
    (
        "fo",
        "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    ),
    ("xlink", "http://www.w3.org/1999/xlink"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("meta", "urn:oasis:names:tc:opendocument:xmlns:meta:1.0"),
    ("number", "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0"),
    (
        "presentation",
        "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0",
    ),
    (
        "svg",
        "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    ),
    ("chart", "urn:oasis:names:tc:opendocument:xmlns:chart:1.0"),
    ("dr3d", "urn:oasis:names:tc:opendocument:xmlns:dr3d:1.0"),
    ("math", "http://www.w3.org/1998/Math/MathML"),
    ("form", "urn:oasis:names:tc:opendocument:xmlns:form:1.0"),
    ("script", "urn:oasis:names:tc:opendocument:xmlns:script:1.0"),
    ("config", "urn:oasis:names:tc:opendocument:xmlns:config:1.0"),
    (
        "manifest",
        "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0",
    ),
    (
        "smil",
        "urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0",
    ),
    ("anim", "urn:oasis:names:tc:opendocument:xmlns:animation:1.0"),
    ("of", "urn:oasis:names:tc:opendocument:xmlns:of:1.2"),
    ("xhtml", "http://www.w3.org/1999/xhtml"),
    (
        "loext",
        "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0",
    ),
    (
        "calcext",
        "urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0",
    ),
    ("officeooo", "http://openoffice.org/2009/office"),
    ("ooo", "http://openoffice.org/2004/office"),
    ("oooc", "http://openoffice.org/2004/calc"),
    ("ooow", "http://openoffice.org/2004/writer"),
    ("tableooo", "http://openoffice.org/2009/table"),
    ("drawooo", "http://openoffice.org/2010/draw"),
    (
        "field",
        "urn:openoffice:names:experimental:ooo-ms-interop:xmlns:field:1.0",
    ),
    ("css3t", "http://www.w3.org/TR/css3-text/"),
    ("xsd", "http://www.w3.org/2001/XMLSchema"),
    ("xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    ("xml", "http://www.w3.org/XML/1998/namespace"),
];

/// The URI of a canonical prefix.
#[must_use]
pub fn namespace_uri(prefix: &str) -> Option<&'static str> {
    NAMESPACES
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, u)| *u)
}

fn canonical_prefix(uri: &str) -> Option<&'static str> {
    NAMESPACES
        .iter()
        .find(|(_, u)| *u == uri)
        .map(|(p, _)| *p)
}

/// The prefix an element or attribute in an unknown namespace gets: `?` cannot appear in an XML
/// name, so such names never collide with the names readers query.
const UNKNOWN: &str = "?";

/// One scope's prefix bindings (source prefix → canonical prefix).
type Bindings = Vec<(String, &'static str)>;

fn rename(name: &str, scopes: &[Bindings], is_attr: bool) -> String {
    let (prefix, local) = match name.find(':') {
        Some(i) => (&name[..i], &name[i + 1..]),
        None => ("", name),
    };
    if prefix == "xml" {
        return name.to_owned();
    }
    // Unprefixed attributes are in no namespace; unprefixed elements use the default namespace.
    if prefix.is_empty() && is_attr {
        return local.to_owned();
    }
    let bound = scopes
        .iter()
        .rev()
        .flat_map(|s| s.iter().rev())
        .find(|(p, _)| p == prefix)
        .map(|(_, c)| *c);
    match bound {
        Some(c) if !c.is_empty() => format!("{c}:{local}"),
        Some(_) => local.to_owned(),
        None if prefix.is_empty() => local.to_owned(),
        None => format!("{UNKNOWN}:{local}"),
    }
}

fn canonicalise(el: &mut XmlElement, scopes: &mut Vec<Bindings>) {
    let mut bindings: Bindings = Vec::new();
    for (k, v) in el.attrs.iter() {
        let src = if k == "xmlns" {
            ""
        } else if let Some(p) = k.strip_prefix("xmlns:") {
            p
        } else {
            continue;
        };
        // Known URIs map to their canonical prefix; a default namespace that is known maps to
        // its prefix too; unknown URIs make names unqueryable.
        let target = canonical_prefix(v).unwrap_or(UNKNOWN);
        bindings.push((src.to_owned(), target));
    }
    scopes.push(bindings);
    el.name = rename(&el.name, scopes, false);
    let attrs: Vec<(String, String)> = el
        .attrs
        .iter()
        .filter(|(k, _)| *k != "xmlns" && !k.starts_with("xmlns:"))
        .map(|(k, v)| (rename(k, scopes, true), v.to_owned()))
        .collect();
    el.attrs = attrs.into_iter().collect();
    for c in &mut el.children {
        if let XmlNode::Element(e) = c {
            canonicalise(e, scopes);
        }
    }
    scopes.pop();
}

/// Parses an ODF XML part with canonical prefixes and every text node kept.
///
/// # Errors
///
/// [`OdfError::Xml`] naming the part.
pub fn parse_part(part: &str, bytes: &[u8]) -> Result<XmlElement, OdfError> {
    let text = decode_xml_bytes(bytes);
    let doc = parse_str_with(
        &text,
        ParseOptions {
            normalize_prefixes: false,
            drop_inter_element_whitespace: false,
            ..ParseOptions::default()
        },
    )
    .map_err(|e| OdfError::Xml {
        part: part.to_owned(),
        message: e.to_string(),
    })?;
    let mut root = doc.root;
    canonicalise(&mut root, &mut Vec::new());
    Ok(root)
}

/// Element children with a name.
pub fn kids<'a>(el: &'a XmlElement, name: &'a str) -> impl Iterator<Item = &'a XmlElement> + 'a {
    el.elements().filter(move |e| e.name == name)
}

/// The first child with a name.
#[must_use]
pub fn kid<'a>(el: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    el.elements().find(|e| e.name == name)
}

/// The concatenated character data below an element (no whitespace processing).
#[must_use]
pub fn all_text(el: &XmlElement) -> String {
    let mut out = String::new();
    fn walk(el: &XmlElement, out: &mut String) {
        for c in &el.children {
            match c {
                XmlNode::Text(t) => out.push_str(&t.text),
                XmlNode::Element(e) => walk(e, out),
                _ => {}
            }
        }
    }
    walk(el, &mut out);
    out
}

/// A streaming XML writer producing deterministic output (attributes in call order, no
/// indentation). Names are written as given; the caller declares namespaces on the root.
#[derive(Debug, Default)]
pub struct XmlOut {
    buf: String,
    stack: Vec<String>,
    open: bool,
}

impl XmlOut {
    /// A writer that starts with the XML declaration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            stack: Vec::new(),
            open: false,
        }
    }

    /// A writer for a fragment (no XML declaration), spliced into a part with [`XmlOut::raw`].
    #[must_use]
    pub fn fragment() -> Self {
        Self::default()
    }

    /// Appends already-serialised markup (a fragment written by another [`XmlOut`]).
    pub fn raw(&mut self, markup: &str) -> &mut Self {
        self.close_start();
        self.buf.push_str(markup);
        self
    }

    /// The markup written so far, with every element closed.
    #[must_use]
    pub fn into_string(mut self) -> String {
        while !self.stack.is_empty() {
            self.end();
        }
        self.buf
    }

    fn close_start(&mut self) {
        if self.open {
            self.buf.push('>');
            self.open = false;
        }
    }

    /// Opens an element.
    pub fn start(&mut self, name: &str) -> &mut Self {
        self.close_start();
        self.buf.push('<');
        self.buf.push_str(name);
        self.stack.push(name.to_owned());
        self.open = true;
        self
    }

    /// Opens the root element and declares the given canonical prefixes on it.
    pub fn root(&mut self, name: &str, prefixes: &[&str]) -> &mut Self {
        self.start(name);
        for p in prefixes {
            if let Some(uri) = namespace_uri(p) {
                let _ = write!(self.buf, " xmlns:{p}=\"{uri}\"");
            }
        }
        self
    }

    /// Adds an attribute to the element just opened.
    pub fn attr(&mut self, name: &str, value: impl AsRef<str>) -> &mut Self {
        debug_assert!(self.open, "attribute {name} after content");
        if self.open {
            let _ = write!(self.buf, " {name}=\"{}\"", escape_attr(value.as_ref()));
        }
        self
    }

    /// Adds an attribute when the value is present.
    pub fn opt_attr<S: AsRef<str>>(&mut self, name: &str, value: Option<S>) -> &mut Self {
        if let Some(v) = value {
            self.attr(name, v);
        }
        self
    }

    /// Writes character data.
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.close_start();
        self.buf.push_str(&escape_text(text));
        self
    }

    /// Closes the innermost open element.
    pub fn end(&mut self) -> &mut Self {
        if let Some(name) = self.stack.pop() {
            if self.open {
                self.buf.push_str("/>");
                self.open = false;
            } else {
                self.buf.push_str("</");
                self.buf.push_str(&name);
                self.buf.push('>');
            }
        }
        self
    }

    /// An element with text content.
    pub fn text_element(&mut self, name: &str, text: &str) -> &mut Self {
        self.start(name).text(text).end()
    }

    /// The depth of open elements.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Closes everything still open and returns the bytes.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        while !self.stack.is_empty() {
            self.end();
        }
        self.buf.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_canonicalised_by_uri() {
        let xml = br#"<x:document-content xmlns:x="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:z="urn:example"><x:body><t:p t:style-name="P1">a <t:span>b</t:span> c</t:p><z:thing z:a="1"/></x:body></x:document-content>"#;
        let root = parse_part("content.xml", xml).unwrap();
        assert_eq!(root.name, "office:document-content");
        let body = kid(&root, "office:body").unwrap();
        let p = kid(body, "text:p").unwrap();
        assert_eq!(p.attr("text:style-name"), Some("P1"));
        assert_eq!(all_text(p), "a b c");
        assert!(body.elements().any(|e| e.name == "?:thing"));
    }

    #[test]
    fn default_namespace_maps_to_its_prefix() {
        let xml = br#"<document-content xmlns="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><body/></document-content>"#;
        let root = parse_part("content.xml", xml).unwrap();
        assert_eq!(root.name, "office:document-content");
        assert!(kid(&root, "office:body").is_some());
    }

    #[test]
    fn dtds_and_entities_are_refused() {
        let bomb = br#"<?xml version="1.0"?><!DOCTYPE l [<!ENTITY a "aaaa">]><l>&a;</l>"#;
        assert!(parse_part("content.xml", bomb).is_err());
    }

    #[test]
    fn the_writer_escapes_and_self_closes() {
        let mut w = XmlOut::new();
        w.root("office:document-meta", &["office", "meta"]);
        w.start("meta:generator").attr("a", "x\"<&").text("1 < 2").end();
        w.start("meta:empty").end();
        let s = String::from_utf8(w.finish()).unwrap();
        assert!(s.contains("xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\""));
        assert!(s.contains("<meta:generator a=\"x&quot;&lt;&amp;\">1 &lt; 2</meta:generator>"));
        assert!(s.contains("<meta:empty/>"));
        assert!(s.ends_with("</office:document-meta>"));
    }
}
