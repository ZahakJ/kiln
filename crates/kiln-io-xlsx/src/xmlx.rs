// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small helpers over `kiln-ooxml`'s element tree: attribute access with SpreadsheetML's
//! defaults, colours, schema-ordered child replacement and element builders.

use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::Color;
use kiln_sheet::num::F64;

use crate::text::{parse_bool, parse_num};

/// A boolean attribute (`None` when absent or not a boolean).
pub(crate) fn abool(e: &XmlElement, name: &str) -> Option<bool> {
    e.attr(name).and_then(parse_bool)
}

/// A boolean attribute with its schema default.
pub(crate) fn abool_or(e: &XmlElement, name: &str, default: bool) -> bool {
    abool(e, name).unwrap_or(default)
}

/// A finite number attribute.
pub(crate) fn anum(e: &XmlElement, name: &str) -> Option<f64> {
    e.attr(name).and_then(parse_num)
}

/// An unsigned integer attribute (fractions and negatives refused).
pub(crate) fn au32(e: &XmlElement, name: &str) -> Option<u32> {
    e.attr(name).and_then(|v| v.trim().parse::<u32>().ok())
}

/// A signed integer attribute.
pub(crate) fn ai64(e: &XmlElement, name: &str) -> Option<i64> {
    e.attr(name).and_then(|v| v.trim().parse::<i64>().ok())
}

/// A child element by local name, whatever its prefix (`x14:dataBar` matches `dataBar`).
pub(crate) fn child_local<'a>(e: &'a XmlElement, local: &str) -> Option<&'a XmlElement> {
    e.elements().find(|c| local_name(&c.name) == local)
}

/// Child elements by local name.
pub(crate) fn children_local<'a>(
    e: &'a XmlElement,
    local: &'a str,
) -> impl Iterator<Item = &'a XmlElement> + 'a {
    e.elements().filter(move |c| local_name(&c.name) == local)
}

/// A SpreadsheetML colour (`rgb`, `theme`, `indexed`, `auto`, each with an optional `tint`).
pub(crate) fn parse_color(e: &XmlElement) -> Option<Color> {
    let tint = F64(anum(e, "tint").unwrap_or(0.0));
    if let Some(rgb) = e.attr("rgb") {
        let h = rgb.trim().trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).ok()?;
        let argb = match h.len() {
            8 => v,
            6 => 0xFF00_0000 | v,
            _ => return None,
        };
        return Some(Color::Rgb { argb, tint });
    }
    if let Some(t) = au32(e, "theme") {
        return Some(Color::Theme {
            index: u8::try_from(t).ok()?,
            tint,
        });
    }
    if let Some(i) = au32(e, "indexed") {
        return Some(Color::Indexed {
            index: u16::try_from(i).ok()?,
            tint,
        });
    }
    if abool(e, "auto") == Some(true) {
        return Some(Color::Auto);
    }
    None
}

/// A colour element (`<color rgb="FFFF0000"/>`).
pub(crate) fn color_el(name: &str, c: &Color) -> XmlElement {
    let e = XmlElement::new(name);
    let with_tint = |e: XmlElement, t: F64| {
        if t.0 == 0.0 {
            e
        } else {
            e.with_attr("tint", t.0)
        }
    };
    match *c {
        Color::Rgb { argb, tint } => with_tint(e.with_attr("rgb", format!("{argb:08X}")), tint),
        Color::Theme { index, tint } => with_tint(e.with_attr("theme", u32::from(index)), tint),
        Color::Indexed { index, tint } => with_tint(e.with_attr("indexed", u32::from(index)), tint),
        Color::Auto => e.with_attr("auto", "1"),
    }
}

/// Replaces every child whose name is `name` by `new` (none = removal), keeping the position of
/// the first one; when there was none, inserts at the schema position given by `order` (the
/// complete child sequence of the parent type).
pub(crate) fn replace_children(
    parent: &mut XmlElement,
    name: &str,
    new: Vec<XmlElement>,
    order: &[&str],
) {
    let first = parent
        .children
        .iter()
        .position(|c| matches!(c, XmlNode::Element(e) if e.name == name));
    parent
        .children
        .retain(|c| !matches!(c, XmlNode::Element(e) if e.name == name));
    let at = first.unwrap_or_else(|| insertion_point(parent, name, order));
    let at = at.min(parent.children.len());
    for (k, e) in new.into_iter().enumerate() {
        parent.children.insert(at + k, XmlNode::Element(e));
    }
}

/// Where a new child named `name` goes: before the first existing child that comes after it in
/// `order` (unknown children keep their places), else at the end.
fn insertion_point(parent: &XmlElement, name: &str, order: &[&str]) -> usize {
    let rank = |n: &str| order.iter().position(|o| *o == n);
    let Some(mine) = rank(name) else {
        return parent.children.len();
    };
    parent
        .children
        .iter()
        .position(|c| match c {
            XmlNode::Element(e) => rank(&e.name).is_some_and(|r| r > mine),
            _ => false,
        })
        .unwrap_or(parent.children.len())
}

/// Sets or removes an attribute, keeping its position when present.
pub(crate) fn set_attr(e: &mut XmlElement, name: &str, value: Option<String>) {
    match value {
        Some(v) => e.attrs.set(name, v),
        None => {
            e.attrs.remove(name);
        }
    }
}

/// `Some("1")` for true, `None` (the default) for false.
pub(crate) fn flag(v: bool) -> Option<String> {
    v.then(|| "1".to_owned())
}

/// `Some("0")` for false, `None` (the default) for true.
pub(crate) fn flag_default_true(v: bool) -> Option<String> {
    (!v).then(|| "0".to_owned())
}

/// An element with text content (`xml:space="preserve"` when the text has edge whitespace).
pub(crate) fn text_el(name: &str, text: &str) -> XmlElement {
    let mut e = XmlElement::new(name);
    let trimmed = text.trim();
    if !text.is_empty() && trimmed.len() != text.len() {
        e.attrs.set("xml:space", "preserve");
    }
    e.children.push(XmlNode::text(text));
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip() {
        for c in [
            Color::Rgb {
                argb: 0xFF12_3456,
                tint: F64(0.0),
            },
            Color::Theme {
                index: 4,
                tint: F64(-0.25),
            },
            Color::Indexed {
                index: 64,
                tint: F64(0.0),
            },
            Color::Auto,
        ] {
            assert_eq!(parse_color(&color_el("color", &c)), Some(c));
        }
        assert_eq!(parse_color(&XmlElement::new("color")), None);
        assert_eq!(
            parse_color(&XmlElement::new("color").with_attr("rgb", "zz")),
            None
        );
    }

    #[test]
    fn children_are_replaced_in_schema_order() {
        let order = ["a", "b", "c", "d"];
        let mut p = XmlElement::new("p")
            .with_child(XmlElement::new("a"))
            .with_child(XmlElement::new("x"))
            .with_child(XmlElement::new("d"));
        replace_children(&mut p, "c", vec![XmlElement::new("c")], &order);
        let names: Vec<&str> = p.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "x", "c", "d"]);
        replace_children(&mut p, "a", vec![], &order);
        replace_children(
            &mut p,
            "d",
            vec![XmlElement::new("d"), XmlElement::new("d")],
            &order,
        );
        let names: Vec<&str> = p.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["x", "c", "d", "d"]);
    }
}
