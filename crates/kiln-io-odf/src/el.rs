// SPDX-License-Identifier: AGPL-3.0-or-later
//! A small owned element value for what the writers build before serialising: style property
//! sets are compared and deduplicated as values (two runs with the same formatting share one
//! automatic style).

use crate::xml::XmlOut;

/// An element with attributes in insertion order and child elements (no text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct El {
    /// Qualified name.
    pub name: String,
    /// Attributes.
    pub attrs: Vec<(String, String)>,
    /// Child elements.
    pub children: Vec<El>,
}

impl El {
    /// An empty element.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// Sets an attribute (replacing an earlier value of the same name).
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| k == name) {
            slot.1 = value;
        } else {
            self.attrs.push((name.to_owned(), value));
        }
    }

    /// Builder form of [`El::set`].
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.set(name, value);
        self
    }

    /// An attribute's value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// True when the element has neither attributes nor children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty() && self.children.is_empty()
    }

    /// Appends a child unless it is empty.
    pub fn push_nonempty(&mut self, child: El) {
        if !child.is_empty() {
            self.children.push(child);
        }
    }

    /// Writes the element.
    pub fn write(&self, w: &mut XmlOut) {
        w.start(&self.name);
        for (k, v) in &self.attrs {
            w.attr(k, v);
        }
        for c in &self.children {
            c.write(w);
        }
        w.end();
    }
}
