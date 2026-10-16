// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parsing XML parts into the ordered tree.
//!
//! The parser runs on `quick-xml` with no DTD support at all: a `<!DOCTYPE` is refused, so
//! entity-expansion attacks ("billion laughs", external entities) have nothing to work with. Only
//! the five predefined entities and character references are expanded. Element nesting is capped
//! ([`ParseOptions::max_depth`]) so a hostile part cannot exhaust the stack of code that walks the
//! tree.

use std::borrow::Cow;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use super::namespaces::{canonical_prefix, is_prefix_list_attribute, split_qname};
use super::node::{Attrs, XmlDeclaration, XmlDocument, XmlElement, XmlNode, XmlPi, XmlText};

/// Why a part is not usable XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("XML: {0}")]
pub struct XmlParseError(pub String);

impl XmlParseError {
    fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Options for [`parse_xml_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Rename namespace prefixes to the canonical Office prefixes (a document that binds `ns0` to
    /// the WordprocessingML namespace comes back with `w:` names). Default `true`.
    pub normalize_prefixes: bool,
    /// Drop whitespace-only text between elements (indentation). Text in leaf elements and inside
    /// `xml:space="preserve"` scopes is always kept. Default `true`.
    pub drop_inter_element_whitespace: bool,
    /// Maximum element nesting depth. Word nests tables inside tables, so real documents go well
    /// past 100 levels; hostile files go to tens of thousands. Default 2 000.
    pub max_depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            normalize_prefixes: true,
            drop_inter_element_whitespace: true,
            max_depth: 2_000,
        }
    }
}

/// Decodes XML bytes to text: a UTF-8 or UTF-16 byte order mark, UTF-16 without a mark (detected
/// from the leading `<`), or the encoding named by the XML declaration (`windows-1252`,
/// `ISO-8859-1`, `Shift_JIS`, … through `encoding_rs`). Anything else is UTF-8, with invalid
/// sequences replaced.
#[must_use]
pub fn decode_xml_bytes(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return Cow::Owned(decode_utf16(rest, true));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return Cow::Owned(decode_utf16(rest, false));
    }
    if bytes.starts_with(&[0x3c, 0x00]) {
        return Cow::Owned(decode_utf16(bytes, true));
    }
    if bytes.starts_with(&[0x00, 0x3c]) {
        return Cow::Owned(decode_utf16(bytes, false));
    }
    let body = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Some(label) = declared_encoding(body)
        && let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes())
        && enc != encoding_rs::UTF_8
        && enc != encoding_rs::UTF_16LE
        && enc != encoding_rs::UTF_16BE
    {
        let (text, _) = enc.decode_without_bom_handling(body);
        return text;
    }
    String::from_utf8_lossy(body)
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|p| {
        if little_endian {
            u16::from_le_bytes(*p)
        } else {
            u16::from_be_bytes(*p)
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// The `encoding` pseudo-attribute of an ASCII-compatible XML declaration, if any.
fn declared_encoding(bytes: &[u8]) -> Option<String> {
    if !bytes.starts_with(b"<?xml") {
        return None;
    }
    let head = &bytes[..bytes.len().min(256)];
    let end = head.windows(2).position(|w| w == b"?>")?;
    let decl = std::str::from_utf8(&head[..end]).ok()?;
    let at = decl.find("encoding")?;
    let rest = decl[at + "encoding".len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &rest[1..];
    let close = value.find(quote)?;
    Some(value[..close].trim().to_owned())
}

/// Parses an XML part with the default options.
///
/// # Errors
/// [`XmlParseError`] for malformed XML, a DTD, text or a second element outside the root, an
/// unknown entity, or nesting deeper than [`ParseOptions::max_depth`].
pub fn parse_xml(input: &[u8]) -> Result<XmlDocument, XmlParseError> {
    parse_xml_with(input, ParseOptions::default())
}

/// Parses XML text with the default options. See [`parse_xml`].
///
/// # Errors
/// As [`parse_xml`].
pub fn parse_xml_str(input: &str) -> Result<XmlDocument, XmlParseError> {
    parse_str_with(input, ParseOptions::default())
}

/// Parses an XML part (bytes in any supported encoding, see [`decode_xml_bytes`]).
///
/// # Errors
/// As [`parse_xml`].
pub fn parse_xml_with(input: &[u8], options: ParseOptions) -> Result<XmlDocument, XmlParseError> {
    let text = decode_xml_bytes(input);
    parse_str_with(&text, options)
}

/// Namespace bookkeeping for one element scope (see the module docs of `namespaces`).
#[derive(Clone, Default)]
struct Scope {
    /// prefix in the source document → prefix in our tree
    rename: Vec<(String, String)>,
    /// prefix in our tree → namespace URI (so one prefix is never bound to two URIs)
    out: Vec<(String, String)>,
}

impl Scope {
    fn rename_of(&self, prefix: &str) -> Option<&str> {
        self.rename
            .iter()
            .rev()
            .find(|(p, _)| p == prefix)
            .map(|(_, t)| t.as_str())
    }
    fn out_uri(&self, prefix: &str) -> Option<&str> {
        self.out
            .iter()
            .rev()
            .find(|(p, _)| p == prefix)
            .map(|(_, u)| u.as_str())
    }
}

struct Frame {
    element: XmlElement,
    /// Index into the scope stack this element uses.
    scope: usize,
    preserve: bool,
}

/// Parses XML text. See [`parse_xml`].
///
/// # Errors
/// As [`parse_xml`].
pub fn parse_str_with(xml: &str, options: ParseOptions) -> Result<XmlDocument, XmlParseError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let config = reader.config_mut();
    config.check_end_names = true;
    config.expand_empty_elements = false;
    config.trim_text(false);

    let mut declaration = None;
    let mut prolog = Vec::new();
    let mut root: Option<XmlElement> = None;
    let mut stack: Vec<Frame> = Vec::new();
    let mut scopes: Vec<Scope> = vec![Scope {
        rename: Vec::new(),
        out: vec![(
            "xml".to_owned(),
            "http://www.w3.org/XML/1998/namespace".to_owned(),
        )],
    }];

    loop {
        let event = reader.read_event().map_err(|e| {
            XmlParseError::new(format!(
                "malformed XML at byte {}: {e}",
                reader.error_position()
            ))
        })?;
        match event {
            Event::Decl(d) => {
                if root.is_some() || !stack.is_empty() || declaration.is_some() {
                    return Err(XmlParseError::new("misplaced XML declaration"));
                }
                let version = d
                    .version()
                    .map_err(|e| XmlParseError::new(format!("bad XML declaration: {e}")))?
                    .into_owned();
                let encoding = match d.encoding() {
                    Some(Ok(v)) => Some(v.into_owned()),
                    Some(Err(e)) => {
                        return Err(XmlParseError::new(format!("bad XML declaration: {e}")));
                    }
                    None => None,
                };
                let standalone = match d.standalone() {
                    Some(Ok(v)) => Some(v.into_owned()),
                    Some(Err(e)) => {
                        return Err(XmlParseError::new(format!("bad XML declaration: {e}")));
                    }
                    None => None,
                };
                declaration = Some(XmlDeclaration {
                    version,
                    encoding,
                    standalone,
                });
            }
            Event::DocType(_) => {
                return Err(XmlParseError::new(
                    "DTDs are not allowed in Office Open XML parts",
                ));
            }
            Event::Start(s) => {
                if stack.len() >= options.max_depth {
                    return Err(XmlParseError::new(format!(
                        "elements nested deeper than {} levels",
                        options.max_depth
                    )));
                }
                if root.is_some() && stack.is_empty() {
                    return Err(XmlParseError::new("more than one root element"));
                }
                let parent = stack.last();
                let parent_scope = parent.map_or(0, |f| f.scope);
                let parent_preserve = parent.is_some_and(|f| f.preserve);
                let (element, scope) = open_element(&s, &mut scopes, parent_scope, options)?;
                let preserve = match element.attr("xml:space") {
                    Some("preserve") => true,
                    Some("default") => false,
                    _ => parent_preserve,
                };
                stack.push(Frame {
                    element,
                    scope,
                    preserve,
                });
            }
            Event::Empty(s) => {
                if stack.len() >= options.max_depth {
                    return Err(XmlParseError::new(format!(
                        "elements nested deeper than {} levels",
                        options.max_depth
                    )));
                }
                let parent_scope = stack.last().map_or(0, |f| f.scope);
                let (element, _) = open_element(&s, &mut scopes, parent_scope, options)?;
                // An empty element's own declarations go out of scope immediately.
                scopes.truncate(parent_scope + 1);
                attach(element, &mut stack, &mut root)?;
            }
            Event::End(_) => {
                let Some(frame) = stack.pop() else {
                    return Err(XmlParseError::new("unexpected end tag"));
                };
                let mut element = frame.element;
                if options.drop_inter_element_whitespace
                    && !frame.preserve
                    && element
                        .children
                        .iter()
                        .any(|c| matches!(c, XmlNode::Element(_)))
                {
                    element.children.retain(|c| match c {
                        XmlNode::Text(t) => t.cdata || !is_xml_whitespace(&t.text),
                        _ => true,
                    });
                }
                scopes.truncate(scope_floor(&stack) + 1);
                attach(element, &mut stack, &mut root)?;
            }
            Event::Text(t) => {
                let content = t.xml10_content();
                push_text(&mut stack, &content, false)?;
            }
            Event::GeneralRef(r) => {
                let resolved: Cow<'_, str> = match r.resolve_char_ref() {
                    Ok(Some(ch)) => Cow::Owned(ch.to_string()),
                    Ok(None) => match quick_xml::escape::resolve_predefined_entity(&r) {
                        Some(s) => Cow::Borrowed(s),
                        None => {
                            return Err(XmlParseError::new(format!(
                                "unknown entity &{};",
                                r.as_ref()
                            )));
                        }
                    },
                    Err(e) => {
                        return Err(XmlParseError::new(format!("bad character reference: {e}")));
                    }
                };
                push_text(&mut stack, &resolved, false)?;
            }
            Event::CData(c) => {
                let content = c.xml10_content();
                if stack.is_empty() {
                    return Err(XmlParseError::new("CDATA outside the root element"));
                }
                push_cdata(&mut stack, &content);
            }
            Event::Comment(c) => {
                let node = XmlNode::Comment(c.xml10_content().into_owned());
                match stack.last_mut() {
                    Some(f) => f.element.children.push(node),
                    None if root.is_none() => prolog.push(node),
                    None => {}
                }
            }
            Event::PI(p) => {
                let node = XmlNode::Pi(XmlPi {
                    target: p.target().to_owned(),
                    content: p.content().trim_start().to_owned(),
                });
                match stack.last_mut() {
                    Some(f) => f.element.children.push(node),
                    None if root.is_none() => prolog.push(node),
                    None => {}
                }
            }
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(XmlParseError::new("unclosed element at end of input"));
    }
    let root = root.ok_or_else(|| XmlParseError::new("no root element"))?;
    Ok(XmlDocument {
        declaration,
        prolog,
        root,
    })
}

/// The highest scope index still referenced by an open element.
fn scope_floor(stack: &[Frame]) -> usize {
    stack.last().map_or(0, |f| f.scope)
}

fn is_xml_whitespace(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
}

fn push_text(stack: &mut [Frame], text: &str, cdata: bool) -> Result<(), XmlParseError> {
    let Some(frame) = stack.last_mut() else {
        if is_xml_whitespace(text) {
            return Ok(());
        }
        return Err(XmlParseError::new("text outside the root element"));
    };
    if let Some(XmlNode::Text(last)) = frame.element.children.last_mut()
        && last.cdata == cdata
        && !cdata
    {
        last.text.push_str(text);
        return Ok(());
    }
    frame.element.children.push(XmlNode::Text(XmlText {
        text: text.to_owned(),
        cdata,
    }));
    Ok(())
}

fn push_cdata(stack: &mut [Frame], text: &str) {
    if let Some(frame) = stack.last_mut() {
        frame.element.children.push(XmlNode::Text(XmlText {
            text: text.to_owned(),
            cdata: true,
        }));
    }
}

fn attach(
    element: XmlElement,
    stack: &mut [Frame],
    root: &mut Option<XmlElement>,
) -> Result<(), XmlParseError> {
    match stack.last_mut() {
        Some(parent) => {
            parent.element.children.push(XmlNode::Element(element));
            Ok(())
        }
        None => {
            if root.is_some() {
                return Err(XmlParseError::new("more than one root element"));
            }
            *root = Some(element);
            Ok(())
        }
    }
}

/// Builds the element for a start tag, binding its namespace declarations. Returns the element
/// and the index of the scope it (and its children) use.
fn open_element(
    s: &BytesStart<'_>,
    scopes: &mut Vec<Scope>,
    parent_scope: usize,
    options: ParseOptions,
) -> Result<(XmlElement, usize), XmlParseError> {
    let raw_name: &str = s.name().0;
    let mut src_attrs: Vec<(&str, String)> = Vec::new();
    for a in s.attributes() {
        let a = a.map_err(|e| XmlParseError::new(format!("bad attribute: {e}")))?;
        let key: &str = a.key.0;
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| XmlParseError::new(format!("bad attribute value for {key}: {e}")))?
            .into_owned();
        src_attrs.push((key, value));
    }

    let mut attrs = Attrs::new();
    if !options.normalize_prefixes {
        for (k, v) in src_attrs {
            attrs.push_unchecked(k.to_owned(), v);
        }
        return Ok((
            XmlElement {
                name: raw_name.to_owned(),
                attrs,
                children: Vec::new(),
            },
            parent_scope,
        ));
    }

    let declares = src_attrs
        .iter()
        .any(|(k, _)| *k == "xmlns" || k.starts_with("xmlns:"));
    let scope_index = if declares {
        scopes.truncate(parent_scope + 1);
        let mut scope = scopes[parent_scope].clone();
        for (k, v) in &src_attrs {
            let src_prefix = if *k == "xmlns" {
                ""
            } else if let Some(p) = k.strip_prefix("xmlns:") {
                p
            } else {
                continue;
            };
            let mut target = src_prefix.to_owned();
            if let Some(canon) = canonical_prefix(v)
                && canon != src_prefix
            {
                match scope.out_uri(canon) {
                    None => target = canon.to_owned(),
                    Some(bound) if bound == v => target = canon.to_owned(),
                    Some(_) => {}
                }
            }
            scope.rename.push((src_prefix.to_owned(), target.clone()));
            scope.out.push((target, v.clone()));
        }
        scopes.push(scope);
        scopes.len() - 1
    } else {
        parent_scope
    };
    let scope = &scopes[scope_index];

    for (k, v) in src_attrs {
        if k == "xmlns" || k.starts_with("xmlns:") {
            let src_prefix = k.strip_prefix("xmlns:").unwrap_or("");
            let target = scope.rename_of(src_prefix).unwrap_or(src_prefix);
            let out_key = if target.is_empty() {
                "xmlns".to_owned()
            } else {
                format!("xmlns:{target}")
            };
            // A default-namespace declaration that moved to a prefix no longer applies to
            // unprefixed names; keep the first binding of each output prefix.
            if !attrs.contains(&out_key) {
                attrs.push_unchecked(out_key, v);
            }
            continue;
        }
        let name = rename_qname(k, scope, true).into_owned();
        let value = if is_prefix_list_attribute(k) {
            v.split_whitespace()
                .map(|p| scope.rename_of(p).unwrap_or(p))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            v
        };
        if attrs.contains(&name) {
            return Err(XmlParseError::new(format!(
                "attribute {name} given twice after namespace normalisation"
            )));
        }
        attrs.push_unchecked(name, value);
    }
    let name = rename_qname(raw_name, scope, false).into_owned();
    Ok((
        XmlElement {
            name,
            attrs,
            children: Vec::new(),
        },
        scope_index,
    ))
}

fn rename_qname<'a>(qname: &'a str, scope: &Scope, is_attribute: bool) -> Cow<'a, str> {
    let (prefix, local) = split_qname(qname);
    if prefix.is_empty() && is_attribute {
        return Cow::Borrowed(qname); // unprefixed attributes are in no namespace
    }
    if prefix == "xml" || prefix == "xmlns" {
        return Cow::Borrowed(qname);
    }
    match scope.rename_of(prefix) {
        None => Cow::Borrowed(qname),
        Some(t) if t == prefix => Cow::Borrowed(qname),
        Some("") => Cow::Borrowed(local),
        Some(t) => Cow::Owned(format!("{t}:{local}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_encodings_are_found() {
        assert_eq!(
            declared_encoding(br#"<?xml version="1.0" encoding="windows-1252"?><a/>"#).as_deref(),
            Some("windows-1252")
        );
        assert_eq!(
            declared_encoding(b"<?xml version='1.0' encoding = 'ISO-8859-1' ?><a/>").as_deref(),
            Some("ISO-8859-1")
        );
        assert_eq!(declared_encoding(b"<a/>"), None);
    }

    #[test]
    fn windows_1252_parts_decode() {
        let bytes = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?><a>caf\xe9 \x80</a>";
        assert!(decode_xml_bytes(bytes).contains("café €"));
        let doc = parse_xml(bytes).unwrap();
        assert_eq!(doc.root.text(), "café €");
    }
}
