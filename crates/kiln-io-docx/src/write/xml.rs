// SPDX-License-Identifier: AGPL-3.0-or-later
//! String-based XML emission for the DOCX writer.
//!
//! WordprocessingML parts are large and made of many small elements, and the reader keeps
//! unmodelled markup as raw XML fragments ([`crate::common::xml::fragment_xml`]); building strings
//! directly (rather than element trees) keeps writing linear and lets fragments be embedded
//! verbatim. Every attribute value and text goes through `kiln-ooxml`'s escaping, which also
//! drops characters XML 1.0 forbids, so no model content can produce a malformed part.

use std::fmt::Display;

use kiln_ooxml::xml::namespaces::namespace_for_prefix;
use kiln_ooxml::xml::{escape_attr, escape_text};

/// The XML declaration Office writes (with CRLF).
pub(crate) const XML_DECLARATION: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// An element under construction: `El::new("w:jc").attr("w:val", "center").empty()`.
#[derive(Debug)]
pub(crate) struct El {
    name: String,
    attrs: String,
}

impl El {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            attrs: String::new(),
        }
    }

    /// Adds an attribute (the value is escaped).
    pub(crate) fn attr(mut self, name: &str, value: impl Display) -> Self {
        push_attr(&mut self.attrs, name, &value.to_string());
        self
    }

    /// Adds an attribute when the value is present.
    pub(crate) fn opt(self, name: &str, value: Option<impl Display>) -> Self {
        match value {
            Some(v) => self.attr(name, v),
            None => self,
        }
    }

    /// Adds an `ST_OnOff` attribute (`1`/`0`) when present.
    pub(crate) fn flag(self, name: &str, value: Option<bool>) -> Self {
        self.opt(name, value.map(|b| if b { "1" } else { "0" }))
    }

    /// `<name attrs/>`.
    pub(crate) fn empty(self) -> String {
        format!("<{}{}/>", self.name, self.attrs)
    }

    /// `<name attrs>inner</name>`, or `<name attrs/>` when `inner` is empty.
    pub(crate) fn wrap(self, inner: &str) -> String {
        if inner.is_empty() {
            return self.empty();
        }
        format!("<{0}{1}>{2}</{0}>", self.name, self.attrs, inner)
    }

    /// `<name attrs>` (the caller closes it).
    pub(crate) fn open(self) -> String {
        format!("<{}{}>", self.name, self.attrs)
    }
}

/// Appends ` name="value"` with the value escaped.
pub(crate) fn push_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape_attr(value));
    out.push('"');
}

/// `<name w:val="value"/>`.
pub(crate) fn val(name: &str, value: impl Display) -> String {
    El::new(name).attr("w:val", value).empty()
}

/// A toggle property: `<w:b/>` when on, `<w:b w:val="0"/>` when off.
pub(crate) fn toggle(name: &str, on: bool) -> String {
    if on {
        format!("<{name}/>")
    } else {
        format!("<{name} w:val=\"0\"/>")
    }
}

/// Escaped character data.
pub(crate) fn text(s: &str) -> std::borrow::Cow<'_, str> {
    escape_text(s)
}

/// Whether `w:t` text needs `xml:space="preserve"` (leading, trailing or repeated whitespace).
pub(crate) fn needs_preserve(s: &str) -> bool {
    let starts = s.chars().next().is_some_and(char::is_whitespace);
    let ends = s.chars().last().is_some_and(char::is_whitespace);
    if starts || ends {
        return true;
    }
    let mut prev_space = false;
    for c in s.chars() {
        let space = c.is_whitespace();
        if space && (prev_space || c != ' ') {
            return true;
        }
        prev_space = space;
    }
    false
}

/// Two-digit upper-case hex (theme tint/shade bytes).
pub(crate) fn hex_byte(b: u8) -> String {
    format!("{b:02X}")
}

/// The qualified name of a fragment's root element (`<w:numPr …>` → `w:numPr`).
pub(crate) fn fragment_name(xml: &str) -> &str {
    let s = xml.trim_start();
    let Some(rest) = s.strip_prefix('<') else {
        return "";
    };
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Orders `(element name, xml)` entries by a schema sequence. Names not in the sequence keep their
/// relative order and go where the `#unknown` (or `#ext`) entry of the sequence is, or at the end.
pub(crate) fn order_by_sequence(entries: Vec<(String, String)>, sequence: &[&str]) -> String {
    let unknown = sequence
        .iter()
        .position(|n| n.starts_with('#'))
        .unwrap_or(sequence.len());
    let rank = |name: &str| sequence.iter().position(|n| *n == name).unwrap_or(unknown);
    let mut ranked: Vec<(usize, usize, String)> = entries
        .into_iter()
        .enumerate()
        .map(|(i, (name, xml))| (rank(&name), i, xml))
        .collect();
    ranked.sort_by_key(|(r, i, _)| (*r, *i));
    let mut out = String::new();
    for (_, _, xml) in ranked {
        out.push_str(&xml);
    }
    out
}

/// Namespaces Word declares as ignorable (Word 2010+ extensions).
const IGNORABLE: [&str; 10] = [
    "w14", "w15", "w16se", "w16cid", "w16", "w16cex", "w16sdtdh", "w16du", "wp14", "w16sdtfl",
];

/// Prefixes with known URIs beyond `kiln-ooxml`'s stable table.
const EXTRA_NS: [(&str, &str); 13] = [
    (
        "w16du",
        "http://schemas.microsoft.com/office/word/2023/wordml/word16du",
    ),
    (
        "w16sdtfl",
        "http://schemas.microsoft.com/office/word/2024/wordml/sdtformatlock",
    ),
    (
        "aink",
        "http://schemas.microsoft.com/office/drawing/2016/ink",
    ),
    (
        "am3d",
        "http://schemas.microsoft.com/office/drawing/2017/model3d",
    ),
    (
        "cx1",
        "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex",
    ),
    (
        "cx2",
        "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex",
    ),
    (
        "cx3",
        "http://schemas.microsoft.com/office/drawing/2016/5/9/chartex",
    ),
    (
        "cx4",
        "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex",
    ),
    (
        "cx5",
        "http://schemas.microsoft.com/office/drawing/2016/5/11/chartex",
    ),
    (
        "cx6",
        "http://schemas.microsoft.com/office/drawing/2016/5/12/chartex",
    ),
    (
        "cx7",
        "http://schemas.microsoft.com/office/drawing/2016/5/13/chartex",
    ),
    (
        "cx8",
        "http://schemas.microsoft.com/office/drawing/2016/5/14/chartex",
    ),
    (
        "adec",
        "http://schemas.microsoft.com/office/drawing/2017/decorative",
    ),
];

/// Root prefixes in the order Word 2019+ declares them on `w:document`.
const ROOT_ORDER: [&str; 34] = [
    "wpc", "cx", "cx1", "cx2", "cx3", "cx4", "cx5", "cx6", "cx7", "cx8", "mc", "aink", "am3d", "o",
    "r", "m", "v", "wp14", "wp", "w10", "w", "w14", "w15", "w16cex", "w16cid", "w16", "w16du",
    "w16sdtdh", "w16sdtfl", "w16se", "wpg", "wpi", "wne", "wps",
];

/// The namespace URI of a prefix the writer may declare.
pub(crate) fn uri_of(prefix: &str) -> Option<&'static str> {
    namespace_for_prefix(prefix)
        .or_else(|| EXTRA_NS.iter().find(|(p, _)| *p == prefix).map(|(_, u)| *u))
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':')
}

/// Walks the markup of `xml` (outside text, comments and processing instructions), calling
/// `element` for every element name and `attribute` for every attribute.
fn scan_markup(xml: &str, mut element: impl FnMut(&str), mut attribute: impl FnMut(&str, &str)) {
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'?' | b'!') {
            // Comments, processing instructions, CDATA: skip to the end of the construct.
            i = xml[i..].find('>').map_or(bytes.len(), |e| i + e + 1);
            continue;
        }
        if i < bytes.len() && bytes[i] == b'/' {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && is_name_char(bytes[i]) {
            i += 1;
        }
        element(&xml[start..i]);
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || matches!(bytes[i], b'>' | b'/') {
                if i < bytes.len() && bytes[i] == b'/' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'>' {
                    i += 1;
                }
                break;
            }
            let name_start = i;
            while i < bytes.len() && is_name_char(bytes[i]) {
                i += 1;
            }
            let name = &xml[name_start..i];
            if name.is_empty() {
                i += 1;
                continue;
            }
            while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\'' && bytes[i] != b'>' {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                continue;
            }
            let quote = char::from(bytes[i]);
            let value_start = i + 1;
            let value_end = xml[value_start..]
                .find(quote)
                .map_or(bytes.len(), |e| value_start + e);
            attribute(name, &xml[value_start..value_end]);
            i = (value_end + 1).min(bytes.len());
        }
    }
}

/// Every namespace prefix `xml` uses in element and attribute names and in the values of the
/// markup-compatibility list attributes (`mc:Ignorable`, `Requires`, …), and (separately) the
/// prefixes the fragments declare themselves, which need no root declaration.
pub(crate) fn used_prefixes(xml: &str) -> (Vec<String>, Vec<String>) {
    fn add(list: &mut Vec<String>, p: &str) {
        if !p.is_empty() && p != "xml" && p != "xmlns" && !list.iter().any(|x| x == p) {
            list.push(p.to_owned());
        }
    }
    let mut used: Vec<String> = Vec::new();
    let mut declared: Vec<String> = Vec::new();
    let mut attrs: Vec<(String, String)> = Vec::new();
    scan_markup(
        xml,
        |name| {
            if let Some((p, _)) = name.split_once(':') {
                add(&mut used, p);
            }
        },
        |name, value| attrs.push((name.to_owned(), value.to_owned())),
    );
    for (name, value) in &attrs {
        if let Some(p) = name.strip_prefix("xmlns:") {
            add(&mut declared, p);
            continue;
        }
        if let Some((p, _)) = name.split_once(':') {
            add(&mut used, p);
        }
        if kiln_ooxml::xml::namespaces::is_prefix_list_attribute(name) {
            for item in value.split_whitespace() {
                add(&mut used, item.split(':').next().unwrap_or_default());
            }
        }
    }
    (used, declared)
}

/// Root namespace declarations for a part body: every prefix with a known URI that the body
/// uses, plus `always`, in Word's order, followed by `mc:Ignorable` listing the Word extension
/// namespaces that are declared. Prefixes the fragments declare themselves (foreign namespaces,
/// see [`crate::common::xml::fragment_xml`]) are left to them.
pub(crate) fn root_namespaces(body: &str, always: &[&str]) -> String {
    let (mut used, _) = used_prefixes(body);
    for p in always {
        if !used.iter().any(|u| u == p) {
            used.push((*p).to_owned());
        }
    }
    let mut ordered: Vec<&str> = ROOT_ORDER
        .iter()
        .copied()
        .filter(|p| used.iter().any(|u| u == p))
        .collect();
    let mut rest: Vec<&str> = used
        .iter()
        .map(String::as_str)
        .filter(|p| !ROOT_ORDER.contains(p))
        .collect();
    rest.sort_unstable();
    ordered.extend(rest);
    let mut declared: Vec<&str> = Vec::new();
    let mut out = String::new();
    for p in ordered {
        if let Some(uri) = uri_of(p) {
            declared.push(p);
            push_attr(&mut out, &format!("xmlns:{p}"), uri);
        }
    }
    let ignorable: Vec<&str> = IGNORABLE
        .iter()
        .copied()
        .filter(|p| declared.contains(p))
        .collect();
    if !ignorable.is_empty() {
        if !declared.contains(&"mc") {
            let mut with_mc = String::new();
            push_attr(
                &mut with_mc,
                "xmlns:mc",
                namespace_for_prefix("mc").unwrap_or_default(),
            );
            out = with_mc + &out;
        }
        push_attr(&mut out, "mc:Ignorable", &ignorable.join(" "));
    }
    out
}

/// Root attributes that keep the original root's declarations and attributes (`root_attrs`, in
/// their order) and add declarations for every prefix the body uses that the original did not
/// declare; Word extension prefixes the body uses are listed in `mc:Ignorable`.
pub(crate) fn root_attrs_from(root_attrs: &[(String, String)], body: &str) -> String {
    let (used, _) = used_prefixes(body);
    let declared: Vec<&str> = root_attrs
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("xmlns:"))
        .collect();
    let mut missing: Vec<&str> = used
        .iter()
        .map(String::as_str)
        .filter(|p| !declared.contains(p) && uri_of(p).is_some())
        .collect();
    let ignorable_used: Vec<&str> = IGNORABLE
        .iter()
        .copied()
        .filter(|p| used.iter().any(|u| u == p))
        .collect();
    let listed: Vec<&str> = root_attrs
        .iter()
        .find(|(k, _)| k == "mc:Ignorable")
        .map(|(_, v)| v.split_whitespace().collect())
        .unwrap_or_default();
    let unlisted: Vec<&str> = ignorable_used
        .iter()
        .copied()
        .filter(|p| !listed.contains(p))
        .collect();
    if (!missing.is_empty() || !unlisted.is_empty())
        && !declared.contains(&"mc")
        && !missing.contains(&"mc")
    {
        missing.push("mc");
    }
    missing.sort_by_key(|p| ROOT_ORDER.iter().position(|o| o == p).unwrap_or(usize::MAX));
    let mut out = String::new();
    let mut ignorable_written = false;
    for (k, v) in root_attrs {
        if k == "mc:Ignorable" {
            let mut list: Vec<&str> = v.split_whitespace().collect();
            for p in &unlisted {
                if !list.contains(p) {
                    list.push(p);
                }
            }
            push_attr(&mut out, k, &list.join(" "));
            ignorable_written = true;
            continue;
        }
        push_attr(&mut out, k, v);
    }
    for p in &missing {
        if let Some(uri) = uri_of(p) {
            push_attr(&mut out, &format!("xmlns:{p}"), uri);
        }
    }
    if !ignorable_written && !unlisted.is_empty() {
        push_attr(&mut out, "mc:Ignorable", &unlisted.join(" "));
    }
    out
}

/// A complete part: declaration, root with namespace declarations, body.
pub(crate) fn part(root: &str, body: &str, always: &[&str]) -> String {
    let ns = root_namespaces(body, always);
    let mut out = String::with_capacity(body.len() + ns.len() + 128);
    out.push_str(XML_DECLARATION);
    out.push('<');
    out.push_str(root);
    out.push_str(&ns);
    if body.is_empty() {
        out.push_str("/>");
    } else {
        out.push('>');
        out.push_str(body);
        out.push_str("</");
        out.push_str(root);
        out.push('>');
    }
    out
}

/// A complete part whose root keeps given attributes (see [`root_attrs_from`]).
pub(crate) fn part_with_root_attrs(
    root: &str,
    root_attrs: &[(String, String)],
    body: &str,
) -> String {
    let attrs = root_attrs_from(root_attrs, body);
    let mut out = String::with_capacity(body.len() + attrs.len() + 128);
    out.push_str(XML_DECLARATION);
    out.push('<');
    out.push_str(root);
    out.push_str(&attrs);
    out.push('>');
    out.push_str(body);
    out.push_str("</");
    out.push_str(root);
    out.push('>');
    out
}

/// Parses many fragments (as [`crate::common::xml::fragment_xml`] writes them) in one pass;
/// fragments that do not parse are `None`.
pub(crate) fn parse_many(fragments: &[&str]) -> Vec<Option<kiln_ooxml::xml::XmlElement>> {
    use kiln_ooxml::xml::namespaces::NAMESPACES;
    use kiln_ooxml::xml::{ParseOptions, parse_str_with};
    let total: usize = fragments.iter().map(|f| f.len() + 32).sum();
    let mut wrapper = String::with_capacity(total + 4096);
    wrapper.push_str("<kiln-fragments");
    let mut seen: Vec<&str> = Vec::new();
    for (prefix, uri) in NAMESPACES {
        if *prefix == "xml" || seen.contains(prefix) {
            continue;
        }
        seen.push(prefix);
        push_attr(&mut wrapper, &format!("xmlns:{prefix}"), uri);
    }
    wrapper.push('>');
    for f in fragments {
        wrapper.push_str("<kiln-item>");
        wrapper.push_str(f);
        wrapper.push_str("</kiln-item>");
    }
    wrapper.push_str("</kiln-fragments>");
    match parse_str_with(&wrapper, ParseOptions::default()) {
        Ok(doc) => doc
            .root
            .elements()
            .map(|item| item.elements().next().cloned())
            .collect(),
        // One malformed fragment spoils the batch: parse them one by one.
        Err(_) => fragments
            .iter()
            .map(|f| crate::common::xml::parse_fragment(f).ok())
            .collect(),
    }
}

/// Every `r:…="id"` attribute value in `xml`, in order of appearance, without duplicates.
pub(crate) fn relationship_refs(xml: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    scan_markup(
        xml,
        |_| {},
        |name, value| {
            if name.starts_with("r:") && !value.is_empty() && !out.iter().any(|x| x == value) {
                out.push(value.to_owned());
            }
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_escape_and_close() {
        assert_eq!(
            El::new("w:t").attr("a", "x\"<&").empty(),
            "<w:t a=\"x&quot;&lt;&amp;\"/>"
        );
        assert_eq!(El::new("w:p").wrap(""), "<w:p/>");
        assert_eq!(El::new("w:p").wrap("<w:r/>"), "<w:p><w:r/></w:p>");
        assert_eq!(
            El::new("x")
                .opt("a", None::<u32>)
                .flag("b", Some(false))
                .empty(),
            "<x b=\"0\"/>"
        );
        assert_eq!(val("w:jc", "center"), "<w:jc w:val=\"center\"/>");
        assert_eq!(toggle("w:b", true), "<w:b/>");
        assert_eq!(toggle("w:b", false), "<w:b w:val=\"0\"/>");
        assert_eq!(hex_byte(0xBF), "BF");
    }

    #[test]
    fn preserve_detects_significant_whitespace() {
        assert!(needs_preserve(" a"));
        assert!(needs_preserve("a "));
        assert!(needs_preserve("a  b"));
        assert!(needs_preserve("a\u{a0}b"));
        assert!(!needs_preserve("a b c"));
        assert!(!needs_preserve(""));
    }

    #[test]
    fn sequence_ordering_is_stable() {
        let e = vec![
            ("w:jc".to_owned(), "J".to_owned()),
            ("w:foo".to_owned(), "F".to_owned()),
            ("w:pStyle".to_owned(), "S".to_owned()),
            ("w:bar".to_owned(), "B".to_owned()),
        ];
        assert_eq!(
            order_by_sequence(e, &["w:pStyle", "w:jc", "#unknown", "w:rPr"]),
            "SJFB"
        );
        assert_eq!(fragment_name("<w:numPr><w:ilvl/></w:numPr>"), "w:numPr");
        assert_eq!(fragment_name("<w:b/>"), "w:b");
        assert_eq!(fragment_name("text"), "");
    }

    #[test]
    fn namespaces_cover_names_attributes_and_ignorable_lists() {
        let body = r#"<w:p w14:paraId="1"><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:t>a:b="c"</w:t></mc:Choice></mc:AlternateContent></w:r><foo:x xmlns:foo="urn:f"/></w:p>"#;
        let (used, declared) = used_prefixes(body);
        assert!(used.contains(&"w14".to_owned()) && used.contains(&"wps".to_owned()));
        assert!(
            !used.contains(&"a".to_owned()),
            "text is not markup: {used:?}"
        );
        assert_eq!(declared, ["foo"]);
        let ns = root_namespaces(body, &["w", "r"]);
        assert!(ns.contains("xmlns:w14=") && ns.contains("xmlns:wps=") && ns.contains("xmlns:r="));
        assert!(!ns.contains("xmlns:foo"));
        assert!(ns.ends_with("mc:Ignorable=\"w14\""), "{ns}");
        assert!(ns.find("xmlns:mc").unwrap() < ns.find("xmlns:w=").unwrap());
    }

    #[test]
    fn original_root_attributes_are_kept_and_completed() {
        let attrs = vec![
            ("xmlns:w".to_owned(), "urn:w".to_owned()),
            ("xmlns:w14".to_owned(), uri_of("w14").unwrap().to_owned()),
            ("xmlns:mc".to_owned(), uri_of("mc").unwrap().to_owned()),
            ("mc:Ignorable".to_owned(), "w14".to_owned()),
        ];
        let out = root_attrs_from(&attrs, "<w:p w15:x=\"1\" w14:paraId=\"2\"/>");
        assert!(out.contains("xmlns:w=\"urn:w\""));
        assert!(out.contains("mc:Ignorable=\"w14 w15\""), "{out}");
        assert!(out.contains("xmlns:w15="));
        assert_eq!(out.matches("xmlns:w14=").count(), 1);
        // Used Word extensions that the original did not list become ignorable.
        let bare = root_attrs_from(&attrs[..2], "<w:p w14:paraId=\"2\"/>");
        assert!(
            bare.contains("xmlns:mc=") && bare.contains("mc:Ignorable=\"w14\""),
            "{bare}"
        );
    }

    #[test]
    fn relationship_references_are_found_once() {
        let xml = r#"<a:blip r:embed="rId5"/><c:chart r:id="rId9"/><v:imagedata r:id="rId5" o:title=""/>"#;
        assert_eq!(relationship_refs(xml), ["rId5", "rId9"]);
        assert!(relationship_refs("<w:t> r:id=\"x\"</w:t>").is_empty());
    }

    #[test]
    fn parts_have_a_declaration_and_namespaces() {
        let p = part("w:hdr", "<w:p/>", &["w", "r"]);
        assert!(p.starts_with(XML_DECLARATION));
        assert!(p.contains("<w:hdr xmlns:r=") || p.contains("<w:hdr xmlns:w="));
        assert!(p.ends_with("<w:p/></w:hdr>"));
    }
}
