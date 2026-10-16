// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serialising trees: no indentation (as Office writes), attributes in tree order, characters XML
//! forbids removed, and namespace declarations added on the root for prefixes the tree uses but
//! never declares, in the stable prefix-table order. The output is therefore deterministic and
//! diffable against Office's own.

use std::borrow::Cow;

use super::namespaces::{is_prefix_list_attribute, namespace_for_prefix, prefix_rank, split_qname};
use super::node::{XmlDeclaration, XmlDocument, XmlElement, XmlNode};

/// A tree that cannot be written as namespace-well-formed XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("XML writer: {0}")]
pub struct XmlWriteError(pub String);

/// Which XML declaration to write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Declaration {
    /// The document's own declaration (with the encoding corrected to UTF-8, which the writer
    /// always produces), or Office's `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` when
    /// it has none or when writing a bare element.
    #[default]
    Auto,
    /// No declaration.
    Omit,
    /// This declaration.
    Custom(XmlDeclaration),
}

/// Options for [`write_document`] and [`write_element`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    /// The XML declaration (followed by CRLF, as Office writes it).
    pub declaration: Declaration,
    /// Declare namespaces the tree uses but never declares, from the stable prefix table, on the
    /// root element. Prefixes named by `mc:Ignorable` and friends count as used. Default `true`.
    pub declare_namespaces: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            declaration: Declaration::Auto,
            declare_namespaces: true,
        }
    }
}

impl WriteOptions {
    /// Options for a fragment: no declaration, namespaces declared.
    #[must_use]
    pub fn fragment() -> Self {
        Self {
            declaration: Declaration::Omit,
            declare_namespaces: true,
        }
    }
}

/// True for a character XML 1.0 allows in content.
#[must_use]
pub fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Removes characters XML 1.0 forbids (most C0 controls, U+FFFE, U+FFFF) rather than producing a
/// part that Office refuses to open. (Rust strings cannot hold unpaired surrogates.)
#[must_use]
pub fn strip_invalid_xml_chars(s: &str) -> Cow<'_, str> {
    if s.chars().all(is_xml_char) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.chars().filter(|c| is_xml_char(*c)).collect())
    }
}

/// Escapes character data (`&`, `<`, `>`, and CR as a character reference so line-end
/// normalisation cannot turn it into LF), dropping forbidden characters.
#[must_use]
pub fn escape_text(s: &str) -> Cow<'_, str> {
    if s.chars()
        .all(|c| is_xml_char(c) && !matches!(c, '&' | '<' | '>' | '\r'))
    {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    Cow::Owned(out)
}

/// Escapes an attribute value (`&`, `<`, `"`, and tab/LF/CR as character references so they
/// survive attribute-value normalisation), dropping forbidden characters.
#[must_use]
pub fn escape_attr(s: &str) -> Cow<'_, str> {
    if s.chars()
        .all(|c| is_xml_char(c) && !matches!(c, '&' | '<' | '"' | '\n' | '\r' | '\t'))
    {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    Cow::Owned(out)
}

/// Prefixes used in `root`'s subtree but not declared where they are used, in stable-table order.
/// A declaration in one branch does not satisfy a use in a sibling.
#[must_use]
pub fn undeclared_prefixes(root: &XmlElement) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    // (element, number of declared entries to restore when leaving it)
    let mut declared: Vec<String> = vec!["xml".to_owned()];
    let mut stack: Vec<(&XmlElement, usize, bool)> = vec![(root, 0, false)];
    while let Some((node, mark, visited)) = stack.pop() {
        if visited {
            declared.truncate(mark);
            continue;
        }
        let mark = declared.len();
        for (k, _) in node.attrs.iter() {
            if k == "xmlns" {
                declared.push(String::new());
            } else if let Some(p) = k.strip_prefix("xmlns:") {
                declared.push(p.to_owned());
            }
        }
        let mut use_prefix = |p: &str| {
            if !p.is_empty()
                && p != "xml"
                && p != "xmlns"
                && !declared.iter().any(|d| d == p)
                && !missing.iter().any(|m| m == p)
            {
                missing.push(p.to_owned());
            }
        };
        use_prefix(split_qname(&node.name).0);
        for (k, v) in node.attrs.iter() {
            if k == "xmlns" || k.starts_with("xmlns:") {
                continue;
            }
            use_prefix(split_qname(k).0);
            if is_prefix_list_attribute(k) {
                for p in v.split_whitespace() {
                    use_prefix(p);
                }
            }
        }
        stack.push((node, mark, true));
        for c in node.children.iter().rev() {
            if let XmlNode::Element(e) = c {
                stack.push((e, 0, false));
            }
        }
    }
    missing.sort_by(|a, b| prefix_rank(a).cmp(&prefix_rank(b)).then_with(|| a.cmp(b)));
    missing
}

fn write_declaration(d: &XmlDeclaration, out: &mut String) {
    out.push_str("<?xml version=\"");
    out.push_str(&escape_attr(&d.version));
    out.push('"');
    if let Some(e) = &d.encoding {
        out.push_str(" encoding=\"");
        out.push_str(&escape_attr(e));
        out.push('"');
    }
    if let Some(s) = &d.standalone {
        out.push_str(" standalone=\"");
        out.push_str(&escape_attr(s));
        out.push('"');
    }
    out.push_str("?>\r\n");
}

fn write_leaf(node: &XmlNode, out: &mut String) {
    match node {
        XmlNode::Text(t) if t.cdata => {
            out.push_str("<![CDATA[");
            out.push_str(&strip_invalid_xml_chars(&t.text).replace("]]>", "]]]]><![CDATA[>"));
            out.push_str("]]>");
        }
        XmlNode::Text(t) => out.push_str(&escape_text(&t.text)),
        XmlNode::Comment(c) => {
            let mut body = strip_invalid_xml_chars(c).replace("--", "- -");
            if body.ends_with('-') {
                body.push(' ');
            }
            out.push_str("<!--");
            out.push_str(&body);
            out.push_str("-->");
        }
        XmlNode::Pi(p) => {
            out.push_str("<?");
            out.push_str(&p.target);
            if !p.content.is_empty() {
                out.push(' ');
                out.push_str(&strip_invalid_xml_chars(&p.content).replace("?>", "? >"));
            }
            out.push_str("?>");
        }
        XmlNode::Raw(markup) => out.push_str(markup),
        XmlNode::Element(_) => {}
    }
}

fn write_start(e: &XmlElement, extra: &[(String, &str)], out: &mut String) {
    out.push('<');
    out.push_str(&e.name);
    // Added declarations go after the element's own xmlns attributes, before ordinary ones.
    let split = e
        .attrs
        .iter()
        .position(|(k, _)| k != "xmlns" && !k.starts_with("xmlns:"))
        .unwrap_or(e.attrs.len());
    let write_attr = |out: &mut String, k: &str, v: &str| {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&escape_attr(v));
        out.push('"');
    };
    for (k, v) in e.attrs.iter().take(split) {
        write_attr(out, k, v);
    }
    for (k, v) in extra {
        write_attr(out, k, v);
    }
    for (k, v) in e.attrs.iter().skip(split) {
        write_attr(out, k, v);
    }
}

/// Serialises a document.
///
/// # Errors
/// [`XmlWriteError`] when the tree uses a namespace prefix that is neither declared nor in the
/// stable prefix table (with `declare_namespaces` on).
pub fn write_document(doc: &XmlDocument, options: &WriteOptions) -> Result<String, XmlWriteError> {
    let mut out = String::new();
    match &options.declaration {
        Declaration::Auto => {
            // The output is UTF-8 whatever the source was, so the declaration must say so.
            let mut d = doc
                .declaration
                .clone()
                .unwrap_or_else(XmlDeclaration::office);
            if d.encoding
                .as_deref()
                .is_some_and(|e| !e.eq_ignore_ascii_case("utf-8"))
            {
                d.encoding = Some("UTF-8".to_owned());
            }
            write_declaration(&d, &mut out);
        }
        Declaration::Omit => {}
        Declaration::Custom(d) => write_declaration(d, &mut out),
    }
    for n in &doc.prolog {
        write_leaf(n, &mut out);
    }
    write_root(&doc.root, options, &mut out)?;
    Ok(out)
}

/// Serialises an element as a complete part (with Office's declaration unless
/// [`Declaration::Omit`]).
///
/// # Errors
/// As [`write_document`].
pub fn write_element(root: &XmlElement, options: &WriteOptions) -> Result<String, XmlWriteError> {
    let mut out = String::new();
    match &options.declaration {
        Declaration::Auto => write_declaration(&XmlDeclaration::office(), &mut out),
        Declaration::Omit => {}
        Declaration::Custom(d) => write_declaration(d, &mut out),
    }
    write_root(root, options, &mut out)?;
    Ok(out)
}

fn write_root(
    root: &XmlElement,
    options: &WriteOptions,
    out: &mut String,
) -> Result<(), XmlWriteError> {
    let mut extra: Vec<(String, &str)> = Vec::new();
    if options.declare_namespaces {
        for p in undeclared_prefixes(root) {
            let uri = namespace_for_prefix(&p).ok_or_else(|| {
                XmlWriteError(format!(
                    "namespace prefix \"{p}\" is used but neither declared nor known"
                ))
            })?;
            extra.push((format!("xmlns:{p}"), uri));
        }
    }
    write_tree_with_leaves(root, &extra, out);
    Ok(())
}

fn write_tree_with_leaves(root: &XmlElement, extra: &[(String, &str)], out: &mut String) {
    enum Step<'a> {
        Open(&'a XmlElement, bool),
        Leaf(&'a XmlNode),
        Close(&'a str),
    }
    let mut stack = vec![Step::Open(root, true)];
    while let Some(step) = stack.pop() {
        match step {
            Step::Open(e, is_root) => {
                write_start(e, if is_root { extra } else { &[] }, out);
                if e.children.is_empty() {
                    out.push_str("/>");
                    continue;
                }
                out.push('>');
                stack.push(Step::Close(&e.name));
                for c in e.children.iter().rev() {
                    match c {
                        XmlNode::Element(child) => stack.push(Step::Open(child, false)),
                        leaf => stack.push(Step::Leaf(leaf)),
                    }
                }
            }
            Step::Leaf(n) => write_leaf(n, out),
            Step::Close(name) => {
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
    }
}

/// [`write_document`] with default options, as UTF-8 bytes ready for a package part.
///
/// # Errors
/// As [`write_document`].
pub fn document_to_bytes(doc: &XmlDocument) -> Result<Vec<u8>, XmlWriteError> {
    write_document(doc, &WriteOptions::default()).map(String::into_bytes)
}

/// [`write_element`] with default options, as UTF-8 bytes ready for a package part.
///
/// # Errors
/// As [`write_document`].
pub fn element_to_bytes(root: &XmlElement) -> Result<Vec<u8>, XmlWriteError> {
    write_element(root, &WriteOptions::default()).map(String::into_bytes)
}
