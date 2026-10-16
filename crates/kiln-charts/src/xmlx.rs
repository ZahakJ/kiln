// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small helpers over the `kiln-ooxml` element tree for the chart reader and writer, and the
//! [`Extras`] mechanism that carries unknown elements through a round trip in their place.

use kiln_ooxml::xml::{AttrValue, XmlElement, XmlNode};

/// The `val` attribute of child `name` (`<c:barDir val="col"/>`).
pub(crate) fn val<'a>(parent: &'a XmlElement, name: &str) -> Option<&'a str> {
    parent.child(name).and_then(|c| c.attr("val"))
}

/// The `val` attribute of child `name` as a boolean. A present element without `val` means
/// `true` (CT_Boolean's default).
pub(crate) fn val_bool(parent: &XmlElement, name: &str) -> Option<bool> {
    let c = parent.child(name)?;
    Some(match c.attr("val") {
        None => true,
        Some(v) => kiln_ooxml::st::parse_on_off(v).unwrap_or(true),
    })
}

/// The `val` attribute of child `name` as an integer.
pub(crate) fn val_int(parent: &XmlElement, name: &str) -> Option<i64> {
    parent.child(name).and_then(|c| c.attr_int("val"))
}

/// The `val` attribute of child `name` as a finite number.
pub(crate) fn val_num(parent: &XmlElement, name: &str) -> Option<f64> {
    parent.child(name).and_then(|c| c.attr_num("val"))
}

/// `<name val="v"/>`.
pub(crate) fn val_el(name: &str, v: impl AttrValue) -> XmlElement {
    XmlElement::new(name).with_attr("val", v)
}

/// `<name val="v"/>` when `v` is set.
pub(crate) fn opt_val<V: AttrValue>(name: &str, v: Option<V>) -> Option<XmlElement> {
    v.map(|v| val_el(name, v))
}

/// The text of child `name` (`<c:f>Sheet1!$A$1</c:f>`).
pub(crate) fn child_text(parent: &XmlElement, name: &str) -> Option<String> {
    parent.child(name).map(XmlElement::text)
}

/// `<name>text</name>`.
pub(crate) fn text_el(name: &str, text: &str) -> XmlElement {
    XmlElement::new(name).with_text(text)
}

/// Unknown child elements of a modelled element, each remembered with the name of the element
/// that preceded it, so the writer can put it back in the same place among the regenerated
/// children ("never destroy what you don't understand", docs/FORMATS.md).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extras(Vec<Extra>);

#[derive(Debug, Clone, PartialEq)]
struct Extra {
    after: Option<String>,
    el: XmlElement,
}

impl Extras {
    /// Collects every child element of `parent` whose name is not in `known`.
    pub(crate) fn capture(parent: &XmlElement, known: &[&str]) -> Self {
        let mut out = Vec::new();
        let mut prev: Option<&str> = None;
        for el in parent.elements() {
            if !known.contains(&el.name.as_str()) {
                out.push(Extra {
                    after: prev.map(str::to_owned),
                    el: el.clone(),
                });
            }
            prev = Some(&el.name);
        }
        Self(out)
    }

    /// Whether nothing was kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of kept elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The kept elements, in document order.
    pub fn elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.0.iter().map(|e| &e.el)
    }

    /// Keeps an element at the end (after everything the writer emits).
    pub fn push(&mut self, el: XmlElement) {
        self.0.push(Extra { after: None, el });
        let last = self.0.len() - 1;
        self.0[last].after = Some("\u{0}end".into());
    }

    /// Removes the kept elements named `name` (an edit made them stale).
    pub fn remove_named(&mut self, name: &str) {
        self.0.retain(|e| e.el.name != name);
    }

    /// Puts the kept elements back among the regenerated `children`: each one right after the last
    /// element named like its original predecessor (an earlier kept element counts), at the start
    /// when it was the first child, or at the end when its predecessor is gone.
    pub(crate) fn merge(&self, mut children: Vec<XmlElement>) -> Vec<XmlElement> {
        let mut front = 0usize;
        for extra in &self.0 {
            match extra.after.as_deref() {
                None => {
                    children.insert(front.min(children.len()), extra.el.clone());
                    front += 1;
                }
                Some(after) => match children.iter().rposition(|c| c.name == after) {
                    Some(i) => children.insert(i + 1, extra.el.clone()),
                    None => children.push(extra.el.clone()),
                },
            }
        }
        children
    }
}

/// Builds an element from its attributes and children in order, merging `extras`.
pub(crate) fn build(
    name: &str,
    attrs: &[(&str, String)],
    children: Vec<Option<XmlElement>>,
    extras: &Extras,
) -> XmlElement {
    let mut el = XmlElement::new(name);
    for (k, v) in attrs {
        el.attrs.set(*k, v.clone());
    }
    let kids: Vec<XmlElement> = children.into_iter().flatten().collect();
    el.children = extras
        .merge(kids)
        .into_iter()
        .map(XmlNode::Element)
        .collect();
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn extras_go_back_where_they_were() {
        let doc = parse_xml_str(
            r#"<c:x xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:u1/><c:a/><c:u2/><c:b/><c:u3/></c:x>"#,
        )
        .unwrap();
        let ex = Extras::capture(&doc.root, &["c:a", "c:b"]);
        assert_eq!(ex.len(), 3);
        let out = ex.merge(vec![XmlElement::new("c:a"), XmlElement::new("c:b")]);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["c:u1", "c:a", "c:u2", "c:b", "c:u3"]);
        // A missing predecessor sends the element to the end.
        let out = ex.merge(vec![XmlElement::new("c:b")]);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["c:u1", "c:b", "c:u3", "c:u2"]);
    }

    #[test]
    fn boolean_values_default_to_true() {
        let doc = parse_xml_str(
            r#"<c:x xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:a/><c:b val="0"/><c:n val="12.5"/></c:x>"#,
        )
        .unwrap();
        assert_eq!(val_bool(&doc.root, "c:a"), Some(true));
        assert_eq!(val_bool(&doc.root, "c:b"), Some(false));
        assert_eq!(val_bool(&doc.root, "c:z"), None);
        assert_eq!(val_num(&doc.root, "c:n"), Some(12.5));
    }
}
