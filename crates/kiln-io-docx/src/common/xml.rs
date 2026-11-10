// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small XML helpers for the DOCX layer: attribute lookup tolerant of unprefixed
//! WordprocessingML attributes, the raw XML fragments kept for what the model does not cover,
//! and location tracking for diagnostics.

use kiln_ooxml::xml::namespaces::{NAMESPACES, namespace_for_prefix};
use kiln_ooxml::xml::{
    Attrs, Declaration, ParseOptions, WriteOptions, XmlElement, XmlParseError, parse_str_with,
    undeclared_prefixes, write_element,
};

/// Attribute lookup for WordprocessingML. Some producers write the main namespace as the
/// default namespace and leave attributes unprefixed (`<commentRangeStart id="0"/>`); Word
/// accepts that, so a `w:` attribute falls back to its unprefixed name.
#[must_use]
pub fn attr<'a>(node: &'a XmlElement, name: &str) -> Option<&'a str> {
    node.attr(name)
        .or_else(|| name.strip_prefix("w:").and_then(|local| node.attr(local)))
}

/// [`attr`] on an optional element.
#[must_use]
pub fn attr_of<'a>(node: Option<&'a XmlElement>, name: &str) -> Option<&'a str> {
    node.and_then(|n| attr(n, name))
}

/// `w:val` of an optional element.
#[must_use]
pub fn val(node: Option<&XmlElement>) -> Option<&str> {
    attr_of(node, "w:val")
}

/// Serialises `node` as a fragment (no XML declaration) for the raw-XML extras.
///
/// Contract: fragments use the stable Office prefixes (`w:`, `a:`, `wp:`, `r:`, `m:`, `v:`, …)
/// without declaring them — whoever embeds a fragment declares those namespaces on the part's
/// root, as every writer does — which keeps the many small fragments compact. Prefixes outside
/// the stable table are declared on the fragment, taken from `scope` (normally the attributes of
/// the part's root element).
#[must_use]
pub fn fragment_xml(node: &XmlElement, scope: &Attrs) -> String {
    let missing: Vec<String> = undeclared_prefixes(node)
        .into_iter()
        .filter(|p| namespace_for_prefix(p).is_none())
        .collect();
    let options = WriteOptions {
        declaration: Declaration::Omit,
        declare_namespaces: false,
    };
    let written = if missing.is_empty() {
        write_element(node, &options)
    } else {
        let mut root = node.clone();
        let mut attrs = Attrs::new();
        for p in &missing {
            let key = format!("xmlns:{p}");
            if let Some(uri) = scope.get(&key) {
                attrs.set(key, uri);
            }
        }
        for (k, v) in node.attrs.iter() {
            attrs.set(k, v);
        }
        root.attrs = attrs;
        write_element(&root, &options)
    };
    // Writing an in-memory tree only fails for names XML cannot represent, which the parser
    // never produces; an empty fragment is the safe answer.
    written.unwrap_or_default()
}

/// Parses a fragment produced by [`fragment_xml`], declaring the stable Office prefixes around
/// it.
///
/// # Errors
///
/// [`XmlParseError`] for malformed XML or an empty fragment.
pub fn parse_fragment(xml: &str) -> Result<XmlElement, XmlParseError> {
    let mut wrapper = String::with_capacity(xml.len() + 4096);
    wrapper.push_str("<kiln-fragment");
    let mut seen: Vec<&str> = Vec::new();
    for (prefix, uri) in NAMESPACES {
        if *prefix == "xml" || seen.contains(prefix) {
            continue;
        }
        seen.push(prefix);
        wrapper.push_str(" xmlns:");
        wrapper.push_str(prefix);
        wrapper.push_str("=\"");
        wrapper.push_str(uri);
        wrapper.push('"');
    }
    wrapper.push('>');
    wrapper.push_str(xml);
    wrapper.push_str("</kiln-fragment>");
    let doc = parse_str_with(&wrapper, ParseOptions::default())?;
    // The wrapper's declarations stay on the wrapper; the fragment comes back as written.
    doc.root
        .elements()
        .next()
        .cloned()
        .ok_or_else(|| XmlParseError("empty XML fragment".to_owned()))
}

/// XPath-like location tracking: `/w:document/w:body[1]/w:p[3]`.
#[derive(Debug, Clone, Default)]
pub struct XmlPath {
    steps: Vec<String>,
}

impl XmlPath {
    /// A path starting at `root` (no index), or empty.
    #[must_use]
    pub fn new(root: &str) -> Self {
        Self {
            steps: if root.is_empty() {
                Vec::new()
            } else {
                vec![root.to_owned()]
            },
        }
    }

    /// A path from its string form (`/a/b[2]/c`).
    #[must_use]
    pub fn parse(path: &str) -> Self {
        Self {
            steps: path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Adds a step.
    pub fn push(&mut self, step: String) {
        self.steps.push(step);
    }

    /// Removes the last step.
    pub fn pop(&mut self) {
        self.steps.pop();
    }

    /// The number of steps.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.steps.len()
    }

    /// Drops steps beyond `depth`.
    pub fn truncate(&mut self, depth: usize) {
        self.steps.truncate(depth);
    }
}

impl std::fmt::Display for XmlPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.steps.is_empty() {
            return f.write_str("/");
        }
        for s in &self.steps {
            f.write_str("/")?;
            f.write_str(s)?;
        }
        Ok(())
    }
}

/// The child elements of `parent` with their XPath step (`w:p[2]`, 1-based among siblings of
/// the same name).
#[must_use]
pub fn stepped(parent: &XmlElement) -> Vec<(&XmlElement, String)> {
    let mut counts: Vec<(&str, u32)> = Vec::new();
    parent
        .elements()
        .map(|c| {
            let n = match counts.iter_mut().find(|(name, _)| *name == c.name) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    counts.push((c.name.as_str(), 1));
                    1
                }
            };
            (c, format!("{}[{n}]", c.name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn fragments_omit_stable_prefix_declarations_and_round_trip() {
        let el = parse_fragment("<w:rPr><w:b/><w14:ligatures w14:val=\"standard\"/></w:rPr>")
            .expect("parses");
        assert_eq!(el.name, "w:rPr");
        assert_eq!(
            fragment_xml(&el, &Attrs::new()),
            "<w:rPr><w:b/><w14:ligatures w14:val=\"standard\"/></w:rPr>"
        );
        assert!(parse_fragment("").is_err());
        assert!(parse_fragment("<w:b>").is_err());
    }

    #[test]
    fn foreign_prefixes_are_declared_from_the_part_scope() {
        let el = XmlElement::new("foo:bar").with_attr("foo:x", "1");
        let scope: Attrs = [("xmlns:foo", "urn:foo")].into_iter().collect();
        assert_eq!(
            fragment_xml(&el, &scope),
            "<foo:bar xmlns:foo=\"urn:foo\" foo:x=\"1\"/>"
        );
    }

    #[test]
    fn unprefixed_attributes_fall_back() {
        let doc = parse_xml_str("<x id=\"3\" xmlns:w=\"urn:w\" w:val=\"v\"/>").expect("parses");
        assert_eq!(attr(&doc.root, "w:id"), Some("3"));
        assert_eq!(attr(&doc.root, "w:val"), Some("v"));
        assert_eq!(attr(&doc.root, "r:id"), None);
        assert_eq!(val(Some(&doc.root)), Some("v"));
        assert_eq!(val(None), None);
    }

    #[test]
    fn paths_and_steps() {
        let doc = parse_xml_str("<r><p/><t/><p/></r>").expect("parses");
        let steps: Vec<String> = stepped(&doc.root).into_iter().map(|(_, s)| s).collect();
        assert_eq!(steps, ["p[1]", "t[1]", "p[2]"]);
        let mut p = XmlPath::new("w:document");
        p.push("w:body[1]".into());
        assert_eq!(p.to_string(), "/w:document/w:body[1]");
        assert_eq!(p.depth(), 2);
        p.truncate(1);
        assert_eq!(p.to_string(), "/w:document");
        p.pop();
        assert_eq!(p.to_string(), "/");
        assert_eq!(XmlPath::parse("/a/b[2]").to_string(), "/a/b[2]");
    }
}
