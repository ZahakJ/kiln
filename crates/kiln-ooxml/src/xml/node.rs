// SPDX-License-Identifier: AGPL-3.0-or-later
//! The XML tree used throughout Kiln's file-format code.
//!
//! Order is significant in OOXML, so an element's children are an ordered list and its attributes
//! keep their document order. Names are qualified with the canonical Office prefix (`w:p`,
//! `a:srgbClr`, `sheetData`; see [`super::namespaces`]).

use std::fmt;

/// An element's attributes in document order. Keys are unique; setting an existing key replaces
/// its value in place, so the order stays stable across edits.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Attrs(Vec<(String, String)>);

impl Attrs {
    /// No attributes.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// The value of `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// True when `name` is present.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|(k, _)| k == name)
    }

    /// Sets `name`, replacing an existing value in place or appending a new attribute.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(slot) = self.0.iter_mut().find(|(k, _)| *k == name) {
            slot.1 = value;
        } else {
            self.0.push((name, value));
        }
    }

    /// Inserts `name` at `index` (clamped), or replaces its value in place if present.
    pub fn insert(&mut self, index: usize, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(slot) = self.0.iter_mut().find(|(k, _)| *k == name) {
            slot.1 = value;
        } else {
            let at = index.min(self.0.len());
            self.0.insert(at, (name, value));
        }
    }

    /// Removes `name`; returns its value.
    pub fn remove(&mut self, name: &str) -> Option<String> {
        let i = self.0.iter().position(|(k, _)| k == name)?;
        Some(self.0.remove(i).1)
    }

    /// Attributes in document order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Mutable access to the values, in document order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut String)> {
        self.0.iter_mut().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of attributes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when there are no attributes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends without checking for an existing key. The parser uses it after it has rejected
    /// duplicates.
    pub(crate) fn push_unchecked(&mut self, name: String, value: String) {
        self.0.push((name, value));
    }
}

impl fmt::Debug for Attrs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for Attrs {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut a = Attrs::new();
        for (k, v) in iter {
            a.set(k, v);
        }
        a
    }
}

/// Character data. Text is held unescaped; the writer escapes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlText {
    /// The characters.
    pub text: String,
    /// Written as a CDATA section when true.
    pub cdata: bool,
}

/// A processing instruction (`<?mso-application progid="Word.Document"?>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlPi {
    /// The target (`mso-application`).
    pub target: String,
    /// Everything after the target, exactly as written (`progid="Word.Document"`).
    pub content: String,
}

/// A node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlNode {
    /// An element.
    Element(XmlElement),
    /// Character data (or a CDATA section).
    Text(XmlText),
    /// A comment, without the `<!--`/`-->` delimiters.
    Comment(String),
    /// A processing instruction.
    Pi(XmlPi),
    /// Well-formed markup kept verbatim and written back unchanged: an element nested deeper than
    /// the parser builds into the tree (see `ParseOptions::keep_too_deep_as_raw`). It declares
    /// every namespace prefix it uses.
    Raw(String),
}

impl XmlNode {
    /// A text node.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        XmlNode::Text(XmlText {
            text: text.into(),
            cdata: false,
        })
    }

    /// The element, if this node is one.
    #[must_use]
    pub fn as_element(&self) -> Option<&XmlElement> {
        match self {
            XmlNode::Element(e) => Some(e),
            _ => None,
        }
    }

    /// The element, mutably, if this node is one.
    pub fn as_element_mut(&mut self) -> Option<&mut XmlElement> {
        match self {
            XmlNode::Element(e) => Some(e),
            _ => None,
        }
    }
}

impl From<XmlElement> for XmlNode {
    fn from(e: XmlElement) -> Self {
        XmlNode::Element(e)
    }
}

/// An element: qualified name, ordered attributes, ordered children.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XmlElement {
    /// Qualified name with the canonical prefix (`w:p`, `a:srgbClr`, `sheetData`).
    pub name: String,
    /// Attributes in document order, including `xmlns`/`xmlns:*` declarations.
    pub attrs: Attrs,
    /// Children in document order.
    pub children: Vec<XmlNode>,
}

/// The XML declaration (`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDeclaration {
    /// `version`, normally `1.0`.
    pub version: String,
    /// `encoding`, if written.
    pub encoding: Option<String>,
    /// `standalone`, if written.
    pub standalone: Option<String>,
}

impl XmlDeclaration {
    /// The declaration Office writes: `version="1.0" encoding="UTF-8" standalone="yes"`.
    #[must_use]
    pub fn office() -> Self {
        Self {
            version: "1.0".into(),
            encoding: Some("UTF-8".into()),
            standalone: Some("yes".into()),
        }
    }
}

/// A parsed XML part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDocument {
    /// The XML declaration, if the part had one.
    pub declaration: Option<XmlDeclaration>,
    /// Comments and processing instructions before the root element.
    pub prolog: Vec<XmlNode>,
    /// The root element.
    pub root: XmlElement,
}

impl XmlDocument {
    /// A document with Office's declaration and no prolog.
    #[must_use]
    pub fn new(root: XmlElement) -> Self {
        Self {
            declaration: Some(XmlDeclaration::office()),
            prolog: Vec::new(),
            root,
        }
    }
}

/// A value that can be written as an attribute: strings, integers, floats and booleans (`1`/`0`,
/// as Office writes them).
pub trait AttrValue {
    /// The attribute text.
    fn to_attr(&self) -> String;
}

impl AttrValue for str {
    fn to_attr(&self) -> String {
        self.to_owned()
    }
}
impl AttrValue for String {
    fn to_attr(&self) -> String {
        self.clone()
    }
}
impl AttrValue for bool {
    fn to_attr(&self) -> String {
        if *self { "1" } else { "0" }.to_owned()
    }
}
impl AttrValue for f64 {
    fn to_attr(&self) -> String {
        format_number(*self)
    }
}
impl<T: AttrValue + ?Sized> AttrValue for &T {
    fn to_attr(&self) -> String {
        (**self).to_attr()
    }
}
macro_rules! int_attr {
    ($($t:ty),*) => {$(
        impl AttrValue for $t {
            fn to_attr(&self) -> String {
                self.to_string()
            }
        }
    )*};
}
int_attr!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

/// Formats a number the way JavaScript's `String(n)` does for the values OOXML uses: integers
/// without a fraction, `-0` as `0`, other values in the shortest round-tripping form.
#[must_use]
pub fn format_number(n: f64) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        // Exact: the value is an integer well inside the i64 range.
        #[allow(clippy::cast_possible_truncation)]
        return (n as i64).to_string();
    }
    format!("{n}")
}

impl XmlElement {
    /// An element with no attributes and no children.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Attrs::new(),
            children: Vec::new(),
        }
    }

    // ------------------------------------------------------------------ builder

    /// Adds an attribute (builder style).
    #[must_use]
    pub fn with_attr(mut self, name: &str, value: impl AttrValue) -> Self {
        self.attrs.set(name, value.to_attr());
        self
    }

    /// Adds an attribute when `value` is `Some` (builder style).
    #[must_use]
    pub fn with_opt_attr<V: AttrValue>(mut self, name: &str, value: Option<V>) -> Self {
        if let Some(v) = value {
            self.attrs.set(name, v.to_attr());
        }
        self
    }

    /// Appends a child element (builder style).
    #[must_use]
    pub fn with_child(mut self, child: XmlElement) -> Self {
        self.children.push(XmlNode::Element(child));
        self
    }

    /// Appends a child element when `Some` (builder style).
    #[must_use]
    pub fn with_opt_child(mut self, child: Option<XmlElement>) -> Self {
        if let Some(c) = child {
            self.children.push(XmlNode::Element(c));
        }
        self
    }

    /// Appends child elements (builder style).
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = XmlElement>) -> Self {
        self.children
            .extend(children.into_iter().map(XmlNode::Element));
        self
    }

    /// Appends a text node (builder style).
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.children.push(XmlNode::text(text));
        self
    }

    // ------------------------------------------------------------------ queries

    /// Child elements in document order.
    pub fn elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(XmlNode::as_element)
    }

    /// Child elements, mutably.
    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut XmlElement> {
        self.children.iter_mut().filter_map(XmlNode::as_element_mut)
    }

    /// The first child element named `name`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.elements().find(|e| e.name == name)
    }

    /// The first child element named `name`, mutably.
    pub fn child_mut(&mut self, name: &str) -> Option<&mut XmlElement> {
        self.elements_mut().find(|e| e.name == name)
    }

    /// Child elements named `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlElement> {
        self.elements().filter(move |e| e.name == name)
    }

    /// Child elements whose name is any of `names`.
    pub fn children_named_any<'a>(
        &'a self,
        names: &'a [&'a str],
    ) -> impl Iterator<Item = &'a XmlElement> {
        self.elements()
            .filter(move |e| names.contains(&e.name.as_str()))
    }

    /// The first child element of any name.
    #[must_use]
    pub fn first_element(&self) -> Option<&XmlElement> {
        self.elements().next()
    }

    /// Follows a chain of child names: `sp.child_path(&["p:spPr", "a:xfrm", "a:off"])`.
    #[must_use]
    pub fn child_path(&self, names: &[&str]) -> Option<&XmlElement> {
        let mut cur = self;
        for n in names {
            cur = cur.child(n)?;
        }
        Some(cur)
    }

    /// All descendant elements named `name`, in document order.
    #[must_use]
    pub fn descendants(&self, name: &str) -> Vec<&XmlElement> {
        let mut out = Vec::new();
        let mut stack: Vec<std::slice::Iter<'_, XmlNode>> = vec![self.children.iter()];
        while let Some(top) = stack.last_mut() {
            match top.next() {
                Some(XmlNode::Element(e)) => {
                    if e.name == name {
                        out.push(e);
                    }
                    stack.push(e.children.iter());
                }
                Some(_) => {}
                None => {
                    stack.pop();
                }
            }
        }
        out
    }

    /// The value of attribute `name`.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name)
    }

    /// An integer attribute; `None` when absent or not an integer (`"42"`, `" -7 "`, `"+3"`).
    #[must_use]
    pub fn attr_int(&self, name: &str) -> Option<i64> {
        parse_int(self.attr(name)?)
    }

    /// A numeric attribute; `None` when absent, empty or not a finite number.
    #[must_use]
    pub fn attr_num(&self, name: &str) -> Option<f64> {
        let v = self.attr(name)?.trim();
        if v.is_empty() {
            return None;
        }
        v.parse::<f64>().ok().filter(|n| n.is_finite())
    }

    /// A boolean attribute per ST_OnOff / `xsd:boolean` (`true|false|1|0|on|off`, any case).
    /// Unrecognised values read as absent.
    #[must_use]
    pub fn attr_bool(&self, name: &str) -> Option<bool> {
        crate::st::parse_on_off(self.attr(name)?)
    }

    /// WordprocessingML toggle: `<w:b/>` is on, `<w:b w:val="0"/>` is off, absent is `None`.
    #[must_use]
    pub fn toggle(&self, name: &str, val_attr: &str) -> Option<bool> {
        let c = self.child(name)?;
        Some(c.attr_bool(val_attr).unwrap_or(true))
    }

    /// Concatenated text of all descendant text nodes.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        collect_text(self, &mut s);
        s
    }

    // ------------------------------------------------------------------ edits

    /// Sets an attribute (`None` removes it).
    pub fn set_attr<V: AttrValue>(&mut self, name: &str, value: Option<V>) {
        match value {
            Some(v) => self.attrs.set(name, v.to_attr()),
            None => {
                self.attrs.remove(name);
            }
        }
    }

    /// Removes child elements named `name`; returns how many were removed.
    pub fn remove_children(&mut self, name: &str) -> usize {
        let before = self.children.len();
        self.children
            .retain(|c| !matches!(c, XmlNode::Element(e) if e.name == name));
        before - self.children.len()
    }

    /// Replaces the first child named like `replacement` in place, or inserts it before the first
    /// child whose name appears in `before` (schema order), or appends it.
    pub fn upsert_child(&mut self, replacement: XmlElement, before: &[&str]) {
        let same = self
            .children
            .iter()
            .position(|c| matches!(c, XmlNode::Element(e) if e.name == replacement.name));
        if let Some(i) = same {
            self.children[i] = XmlNode::Element(replacement);
            return;
        }
        let at = self
            .children
            .iter()
            .position(|c| matches!(c, XmlNode::Element(e) if before.contains(&e.name.as_str())));
        match at {
            Some(j) => self.children.insert(j, XmlNode::Element(replacement)),
            None => self.children.push(XmlNode::Element(replacement)),
        }
    }

    /// Resolves an `mc:AlternateContent` block: the children of the first `mc:Choice` whose
    /// `Requires` prefixes are all in `supported`, else those of `mc:Fallback` (empty when neither
    /// applies).
    #[must_use]
    pub fn select_alternate_content(&self, supported: &[&str]) -> Vec<&XmlElement> {
        for choice in self.children_named("mc:Choice") {
            let req = choice.attr("Requires").unwrap_or("");
            if req.split_whitespace().all(|p| supported.contains(&p)) {
                return choice.elements().collect();
            }
        }
        self.child("mc:Fallback")
            .map(|f| f.elements().collect())
            .unwrap_or_default()
    }
}

fn collect_text(e: &XmlElement, out: &mut String) {
    for c in &e.children {
        match c {
            XmlNode::Text(t) => out.push_str(&t.text),
            XmlNode::Element(child) => collect_text(child, out),
            _ => {}
        }
    }
}

/// Parses an XML integer the way OOXML attributes are written: optional whitespace, optional sign,
/// decimal digits.
#[must_use]
pub fn parse_int(v: &str) -> Option<i64> {
    let t = v.trim();
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_keep_order_and_replace_in_place() {
        let mut a = Attrs::new();
        a.set("b", "1");
        a.set("a", "2");
        a.set("b", "3");
        assert_eq!(a.iter().collect::<Vec<_>>(), vec![("b", "3"), ("a", "2")]);
        assert_eq!(a.remove("b").as_deref(), Some("3"));
        assert!(!a.contains("b"));
        a.insert(0, "z", "9");
        assert_eq!(a.iter().next(), Some(("z", "9")));
    }

    #[test]
    fn numbers_format_like_javascript() {
        assert_eq!(format_number(5.0), "5");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(0.925), "0.925");
        assert_eq!(format_number(-342_900.0), "-342900");
        assert_eq!(parse_int(" -7 "), Some(-7));
        assert_eq!(parse_int("+3"), Some(3));
        assert_eq!(parse_int("1.5"), None);
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("99999999999999999999"), None);
    }

    #[test]
    fn descendants_walk_in_document_order() {
        let tree = XmlElement::new("r")
            .with_child(XmlElement::new("t").with_text("A"))
            .with_child(XmlElement::new("x").with_child(XmlElement::new("t").with_text("B")))
            .with_child(XmlElement::new("t").with_text("C"));
        let texts: Vec<String> = tree.descendants("t").iter().map(|t| t.text()).collect();
        assert_eq!(texts, ["A", "B", "C"]);
    }
}
