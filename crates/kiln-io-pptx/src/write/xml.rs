// SPDX-License-Identifier: AGPL-3.0-or-later
//! XML helpers of the writer: the namespaces every PresentationML part declares, embedding kept
//! fragments (with their relationship ids and namespace declarations), text escaping and stable
//! GUIDs.

use std::collections::BTreeSet;

use kiln_ooxml::xml::namespaces::ns;
use kiln_ooxml::xml::{ParseOptions, XmlElement, XmlNode, parse_str_with};

/// The `p159` namespace (Morph, PowerPoint 2019), not in Office's prefix table.
pub(crate) const NS_P159: &str = "http://schemas.microsoft.com/office/powerpoint/2015/09/main";
/// The `p14` namespace (PowerPoint 2010 extensions).
pub(crate) const NS_P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
/// The `p188` namespace (modern comments).
pub(crate) const NS_P188: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";
/// The `pc` namespace (comment anchors).
pub(crate) const NS_PC: &str = "http://schemas.microsoft.com/office/powerpoint/2013/main/command";
/// The `a14` namespace (equations).
pub(crate) const NS_A14: &str = "http://schemas.microsoft.com/office/drawing/2010/main";
/// The chart namespace.
pub(crate) const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
/// The ChartEx namespace.
pub(crate) const NS_CX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// A part root with the three namespaces every PresentationML part uses (`a`, `r`, `p`), in the
/// order PowerPoint writes them.
pub(crate) fn part_root(name: &str) -> XmlElement {
    XmlElement::new(name)
        .with_attr("xmlns:a", ns::A)
        .with_attr("xmlns:r", ns::R)
        .with_attr("xmlns:p", ns::P)
}

/// How kept fragments are parsed: content nested too deeply for the tree stays verbatim.
fn fragment_options() -> ParseOptions {
    ParseOptions {
        keep_too_deep_as_raw: true,
        ..ParseOptions::default()
    }
}

/// Parses a kept fragment; `None` (the caller embeds it verbatim) when it does not parse.
pub(crate) fn parse_kept(xml: &str) -> Option<XmlElement> {
    parse_str_with(xml, fragment_options()).ok().map(|d| d.root)
}

/// A fragment as a node (parsed, so its namespaces can be hoisted and its relationship ids
/// repaired). `None` for text that is not well-formed XML: writing it would corrupt the part.
pub(crate) fn kept_node(xml: &str) -> Option<XmlNode> {
    parse_kept(xml).map(XmlNode::Element)
}

/// Is `name` an attribute holding a relationship id (`r:id`, `r:embed`, `r:link`, `r:dm`, …)?
fn is_rel_attr(name: &str) -> bool {
    name.starts_with("r:")
}

/// Every relationship id an element tree refers to (attributes in the `r:` namespace), without
/// recursion.
pub(crate) fn relationship_refs(root: &XmlElement, out: &mut BTreeSet<String>) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        for (k, v) in e.attrs.iter() {
            if is_rel_attr(k) && !v.is_empty() {
                out.insert(v.to_owned());
            }
        }
        stack.extend(e.elements());
    }
}

/// The relationship ids referred to by raw markup (`r:id="…"` and friends).
pub(crate) fn raw_relationship_refs(markup: &str, out: &mut BTreeSet<String>) {
    let mut rest = markup;
    while let Some(i) = rest.find(" r:") {
        rest = &rest[i + 3..];
        let Some(eq) = rest.find('=') else { break };
        let name = &rest[..eq];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
            continue;
        }
        let after = &rest[eq + 1..];
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let body = &after[1..];
        if let Some(end) = body.find(quote) {
            let v = &body[..end];
            if !v.is_empty() {
                out.insert(v.to_owned());
            }
        }
    }
}

/// Relationship ids in a node.
pub(crate) fn node_refs(node: &XmlNode, out: &mut BTreeSet<String>) {
    match node {
        XmlNode::Element(e) => {
            relationship_refs(e, out);
            // Raw descendants (content nested too deeply) carry ids too.
            let mut stack = vec![e];
            while let Some(x) = stack.pop() {
                for c in &x.children {
                    match c {
                        XmlNode::Raw(m) => raw_relationship_refs(m, out),
                        XmlNode::Element(child) => stack.push(child),
                        _ => {}
                    }
                }
            }
        }
        XmlNode::Raw(m) => raw_relationship_refs(m, out),
        _ => {}
    }
}

/// Moves the namespace declarations of embedded fragments up to the part root: a declaration
/// the root already makes with the same URI is dropped, a new prefix is declared on the root, and
/// a prefix the root binds differently stays where it is. The part then reads like Office's
/// (every namespace on the root) and kept elements come out byte for byte as they were read.
pub(crate) fn hoist_namespaces(root: &mut XmlElement) {
    let mut declared: Vec<(String, String)> = root
        .attrs
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("xmlns:")
                .map(|p| (p.to_owned(), v.to_owned()))
        })
        .collect();
    let mut added: Vec<(String, String)> = Vec::new();
    let mut stack: Vec<&mut XmlElement> = root.elements_mut().collect();
    while let Some(e) = stack.pop() {
        let decls: Vec<(String, String)> = e
            .attrs
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("xmlns:")
                    .map(|p| (p.to_owned(), v.to_owned()))
            })
            .collect();
        for (p, uri) in decls {
            match declared.iter().find(|(q, _)| *q == p) {
                Some((_, u)) if *u == uri => {
                    e.attrs.remove(&format!("xmlns:{p}"));
                }
                Some(_) => {}
                None => {
                    e.attrs.remove(&format!("xmlns:{p}"));
                    declared.push((p.clone(), uri.clone()));
                    added.push((p, uri));
                }
            }
        }
        // Elements that keep a conflicting binding scope their own subtree: their children are
        // still visited, which only ever removes declarations equal to the root's.
        stack.extend(e.elements_mut());
    }
    // Office's prefix order, so the root reads the same whatever order the content came in.
    added.sort_by(|a, b| {
        kiln_ooxml::xml::namespaces::prefix_rank(&a.0)
            .cmp(&kiln_ooxml::xml::namespaces::prefix_rank(&b.0))
            .then_with(|| a.0.cmp(&b.0))
    });
    for (p, uri) in added {
        root.attrs.set(format!("xmlns:{p}"), uri);
    }
}

/// Text for `a:t`: control characters other than tab become Office's `_xHHHH_` escape, and an
/// underscore that would read as the start of one is escaped itself (`_x005F_`), the inverse of
/// the reader's decoding.
pub(crate) fn encode_text(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' && looks_like_escape(&chars[i..]) {
            out.push_str("_x005F_");
        } else if c.is_control() && c != '\t' {
            out.push_str(&format!("_x{:04X}_", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// `_xHHHH_` at the start of `rest`.
fn looks_like_escape(rest: &[char]) -> bool {
    rest.len() >= 7
        && rest[1] == 'x'
        && rest[2..6].iter().all(char::is_ascii_hexdigit)
        && rest[6] == '_'
}

/// A stable GUID (`{8-4-4-4-12}`, upper case) from a seed, for ids Office requires but the
/// model does not keep (field ids, section ids, comment ids). The same seed gives the same GUID,
/// so writes are deterministic.
pub(crate) fn stable_guid(seed: &str) -> String {
    // Two FNV-1a passes with different offsets give 128 bits.
    let fnv = |offset: u64| {
        let mut h = offset;
        for b in seed.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    let a = fnv(0xcbf2_9ce4_8422_2325);
    let b = fnv(0x84222325_cbf29ce4);
    let hex = format!("{a:016X}{b:016X}");
    format!(
        "{{{}-{}-4{}-{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Every descendant element (not the root) with this local name, without recursion.
pub(crate) fn descendants_local<'a>(e: &'a XmlElement, name: &str) -> Vec<&'a XmlElement> {
    let mut out = Vec::new();
    let mut stack: Vec<&'a XmlElement> = e.elements().collect();
    stack.reverse();
    while let Some(c) = stack.pop() {
        if kiln_ooxml::xml::namespaces::local_name(&c.name) == name {
            out.push(c);
        }
        let at = stack.len();
        stack.extend(c.elements());
        stack[at..].reverse();
    }
    out
}

/// The `p:cNvPr/@id`s in a kept fragment (so new shapes do not reuse them).
pub(crate) fn shape_ids_in(xml: &str, out: &mut BTreeSet<u32>) {
    if let Some(e) = parse_kept(xml) {
        let mut all = descendants_local(&e, "cNvPr");
        if kiln_ooxml::xml::namespaces::local_name(&e.name) == "cNvPr" {
            all.push(&e);
        }
        for c in all {
            if let Some(id) = c.attr_int("id").and_then(|v| u32::try_from(v).ok()) {
                out.insert(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_invert_the_reader() {
        assert_eq!(encode_text("a\tb"), "a\tb");
        assert_eq!(encode_text("bell\u{7}"), "bell_x0007_");
        assert_eq!(encode_text("_x0041_"), "_x005F_x0041_");
        assert_eq!(encode_text("snake_case _x"), "snake_case _x");
    }

    #[test]
    fn guids_are_stable_and_well_formed() {
        let g = stable_guid("slide 3");
        assert_eq!(g, stable_guid("slide 3"));
        assert_ne!(g, stable_guid("slide 4"));
        assert_eq!(g.len(), 38);
        assert!(g.starts_with('{') && g.ends_with('}'));
        assert_eq!(g.matches('-').count(), 4);
        assert_eq!(&g[15..16], "4");
    }

    #[test]
    fn relationship_references_are_found_in_trees_and_raw_markup() {
        let e = parse_kept(
            r#"<p:x xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:y r:id="rId3"/><p:z r:embed="rId9" other="rId1"/></p:x>"#,
        )
        .unwrap();
        let mut ids = BTreeSet::new();
        relationship_refs(&e, &mut ids);
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["rId3", "rId9"]);
        let mut raw = BTreeSet::new();
        raw_relationship_refs(
            r#"<a r:id="rId2" b:id="x"><c r:link='rId5'/></a>"#,
            &mut raw,
        );
        assert_eq!(raw.into_iter().collect::<Vec<_>>(), ["rId2", "rId5"]);
    }

    #[test]
    fn hoisting_moves_declarations_to_the_root() {
        let mut root = part_root("p:sld");
        let kept = parse_kept(
            r#"<p:extLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main"><p14:x/></p:extLst>"#,
        )
        .unwrap();
        root.children.push(XmlNode::Element(kept));
        hoist_namespaces(&mut root);
        assert_eq!(root.attr("xmlns:p14"), Some(NS_P14));
        let child = root.first_element().unwrap();
        assert!(child.attrs.iter().all(|(k, _)| !k.starts_with("xmlns")));
    }

    #[test]
    fn shape_ids_are_found_in_kept_xml() {
        let mut ids = BTreeSet::new();
        shape_ids_in(
            r#"<p:sp xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:nvSpPr><p:cNvPr id="12" name="x"/></p:nvSpPr></p:sp>"#,
            &mut ids,
        );
        assert!(ids.contains(&12));
    }
}
