// SPDX-License-Identifier: AGPL-3.0-or-later
//! The relationships of one source part (or of the package, source `/`).

use std::collections::HashMap;

use super::OpcError;
use super::part_name::{part_key, resolve_target};
use super::rel_types::rel_type_matches;
use crate::st::TargetMode;
use crate::xml::{XmlElement, XmlParseError, element_to_bytes, namespaces::ns, parse_xml};

/// One relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// The relationship id (`rId3`), unique within its source.
    pub id: String,
    /// The relationship type URI.
    pub rel_type: String,
    /// The target exactly as written in the `.rels` part.
    pub target: String,
    /// Whether the target is inside the package.
    pub target_mode: TargetMode,
}

impl Relationship {
    /// True for a target outside the package (a URL or a linked file).
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.target_mode == TargetMode::External
    }
}

/// The relationships of one source part, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationships {
    source: String,
    list: Vec<Relationship>,
    by_id: HashMap<String, usize>,
}

impl Relationships {
    /// No relationships from `source` (`/` for the package).
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            list: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Parses a `.rels` part. Entries missing `Id`, `Type` or `Target` are skipped, as Office
    /// does.
    ///
    /// # Errors
    /// [`XmlParseError`] when the part is not XML.
    pub fn parse(source: &str, bytes: &[u8]) -> Result<Self, XmlParseError> {
        let doc = parse_xml(bytes)?;
        let mut rels = Self::new(source);
        for r in doc.root.children_named("Relationship") {
            let (Some(id), Some(t), Some(target)) =
                (r.attr("Id"), r.attr("Type"), r.attr("Target"))
            else {
                continue;
            };
            if rels.by_id.contains_key(id) {
                continue; // a duplicate id is unreachable; the first one wins
            }
            let mode = if r.attr("TargetMode") == Some("External") {
                TargetMode::External
            } else {
                TargetMode::Internal
            };
            rels.push(Relationship {
                id: id.to_owned(),
                rel_type: t.to_owned(),
                target: target.to_owned(),
                target_mode: mode,
            });
        }
        Ok(rels)
    }

    fn push(&mut self, rel: Relationship) {
        self.by_id.insert(rel.id.clone(), self.list.len());
        self.list.push(rel);
    }

    /// The source part (`/` for package relationships).
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Number of relationships.
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// True when there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// All relationships in document order.
    #[must_use]
    pub fn all(&self) -> &[Relationship] {
        &self.list
    }

    /// The relationship with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Relationship> {
        self.by_id.get(id).map(|&i| &self.list[i])
    }

    /// Relationships of a type (Strict type URIs match their transitional equivalents).
    pub fn by_type<'a>(&'a self, rel_type: &'a str) -> impl Iterator<Item = &'a Relationship> {
        self.list
            .iter()
            .filter(move |r| rel_type_matches(&r.rel_type, rel_type))
    }

    /// The first relationship of a type.
    #[must_use]
    pub fn first_by_type(&self, rel_type: &str) -> Option<&Relationship> {
        self.list
            .iter()
            .find(|r| rel_type_matches(&r.rel_type, rel_type))
    }

    /// The absolute part name of an internal relationship's target; `None` for external targets.
    #[must_use]
    pub fn resolve(&self, rel: &Relationship) -> Option<String> {
        (!rel.is_external()).then(|| resolve_target(&self.source, &rel.target))
    }

    /// The next unused id with `prefix` (`rId1`, `rId2`, …).
    #[must_use]
    pub fn next_id(&self, prefix: &str) -> String {
        (1..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| !self.by_id.contains_key(id))
            .unwrap_or_else(|| format!("{prefix}0"))
    }

    /// Adds a relationship with the target stored as given. `id` defaults to the next free
    /// `rIdN`.
    ///
    /// # Errors
    /// [`OpcError::DuplicateRelationshipId`] when `id` is taken.
    pub fn add(
        &mut self,
        rel_type: &str,
        target: &str,
        id: Option<&str>,
        target_mode: TargetMode,
    ) -> Result<Relationship, OpcError> {
        let id = match id {
            Some(id) => {
                if self.by_id.contains_key(id) {
                    return Err(OpcError::DuplicateRelationshipId {
                        id: id.to_owned(),
                        source_part: self.source.clone(),
                    });
                }
                id.to_owned()
            }
            None => self.next_id("rId"),
        };
        let rel = Relationship {
            id,
            rel_type: rel_type.to_owned(),
            target: target.to_owned(),
            target_mode,
        };
        self.push(rel.clone());
        Ok(rel)
    }

    /// Removes the relationship with `id`; returns it.
    pub fn remove(&mut self, id: &str) -> Option<Relationship> {
        let i = self.by_id.remove(id)?;
        let rel = self.list.remove(i);
        for (j, r) in self.list.iter().enumerate().skip(i) {
            self.by_id.insert(r.id.clone(), j);
        }
        Some(rel)
    }

    /// Removes every internal relationship whose target resolves to `part_name`
    /// (case-insensitively); returns them.
    pub fn remove_targeting(&mut self, part_name: &str) -> Vec<Relationship> {
        let key = part_key(part_name);
        let ids: Vec<String> = self
            .list
            .iter()
            .filter(|r| self.resolve(r).is_some_and(|t| part_key(&t) == key))
            .map(|r| r.id.clone())
            .collect();
        ids.iter().filter_map(|id| self.remove(id)).collect()
    }

    /// The `.rels` part as an XML tree.
    #[must_use]
    pub fn to_element(&self) -> XmlElement {
        XmlElement::new("Relationships")
            .with_attr("xmlns", ns::REL)
            .with_children(self.list.iter().map(|r| {
                XmlElement::new("Relationship")
                    .with_attr("Id", r.id.as_str())
                    .with_attr("Type", r.rel_type.as_str())
                    .with_attr("Target", r.target.as_str())
                    .with_opt_attr("TargetMode", r.is_external().then_some("External"))
            }))
    }

    /// The `.rels` part bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        // No prefixes are used, so writing cannot fail.
        element_to_bytes(&self.to_element()).unwrap_or_default()
    }
}
