// SPDX-License-Identifier: AGPL-3.0-or-later
//! "Never destroy what you don't understand" (PLAN.md §3.10).
//!
//! A reader records every part and relationship it did not consume in a [`PassthroughSet`]; the
//! writer re-emits them into the new package with relationships fixed up. The bytes of every
//! passthrough part are carried unchanged, so they are byte-identical in the saved file.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::opc::{
    OpcError, OpcPackage, Relationship, basename_of, content_type, directory_of, extension_of,
    part_key, relative_target, resolve_target,
};

/// A part the reader did not model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassthroughPart {
    /// The part name in the original package.
    pub name: String,
    /// Its content type.
    pub content_type: String,
    /// Its bytes, exactly as stored.
    pub bytes: Vec<u8>,
    /// The part's own outgoing relationships, as written.
    pub relationships: Vec<Relationship>,
}

/// A relationship from a consumed part (or the package) that the writer will not recreate:
/// one that targets a passthrough part or an external resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassthroughRelationship {
    /// The source part in the original package (`/` = package relationships).
    pub source: String,
    /// The relationship as written.
    pub rel: Relationship,
}

/// What the reader consumed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectOptions {
    /// Parts the reader fully modelled; the writer regenerates them.
    pub consumed_parts: Vec<String>,
    /// Relationships the reader consumed although their target is not a consumed part (for
    /// example hyperlinks it modelled), as `(source, ids)`.
    pub consumed_relationships: Vec<(String, Vec<String>)>,
}

impl CollectOptions {
    /// Options with these consumed parts and no consumed relationships.
    #[must_use]
    pub fn consumed<'a>(parts: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            consumed_parts: parts.into_iter().map(str::to_owned).collect(),
            consumed_relationships: Vec::new(),
        }
    }
}

/// What [`PassthroughSet::emit`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmitResult {
    /// Passthrough parts renamed to avoid a part the writer already produced: original → new.
    pub renamed_parts: HashMap<String, String>,
    /// For each (new) source part: original relationship id → id used in the new package.
    pub relationship_ids: HashMap<String, HashMap<String, String>>,
    /// Relationships that could not be re-emitted because their source no longer exists.
    pub dropped: Vec<PassthroughRelationship>,
}

/// Parts and relationships carried from a read package to a written one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PassthroughSet {
    parts: IndexMap<String, PassthroughPart>,
    rels: Vec<PassthroughRelationship>,
}

impl PassthroughSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records everything in `pkg` that the reader did not consume.
    ///
    /// # Errors
    /// [`OpcError`] when a part or `.rels` part of `pkg` cannot be read.
    pub fn collect(pkg: &OpcPackage, options: &CollectOptions) -> Result<Self, OpcError> {
        let consumed: HashSet<String> =
            options.consumed_parts.iter().map(|p| part_key(p)).collect();
        let consumed_rels: HashMap<String, HashSet<&str>> = options
            .consumed_relationships
            .iter()
            .map(|(src, ids)| (part_key(src), ids.iter().map(String::as_str).collect()))
            .collect();

        let mut set = Self::new();
        for name in pkg.part_names() {
            if consumed.contains(&part_key(name)) {
                continue;
            }
            set.record_part(PassthroughPart {
                name: name.to_owned(),
                content_type: pkg
                    .content_type(name)
                    .unwrap_or(content_type::OCTET_STREAM)
                    .to_owned(),
                bytes: pkg.part_bytes(name)?.into_owned(),
                relationships: pkg.relationships(name)?.all().to_vec(),
            });
        }
        let sources = std::iter::once("/").chain(pkg.part_names());
        for source in sources {
            if source != "/" && !consumed.contains(&part_key(source)) {
                continue;
            }
            let rels = pkg.relationships(source)?;
            let skip = consumed_rels.get(&part_key(source));
            for r in rels.all() {
                if skip.is_some_and(|s| s.contains(r.id.as_str())) {
                    continue;
                }
                if let Some(target) = rels.resolve(r) {
                    if consumed.contains(&part_key(&target)) {
                        continue;
                    }
                    if !pkg.has(&target) {
                        continue; // dangling: nothing to preserve
                    }
                }
                set.record_relationship(source, r.clone());
            }
        }
        Ok(set)
    }

    /// Records a part.
    pub fn record_part(&mut self, part: PassthroughPart) {
        self.parts.insert(part_key(&part.name), part);
    }

    /// Records a relationship from a consumed source.
    pub fn record_relationship(&mut self, source: &str, rel: Relationship) {
        self.rels.push(PassthroughRelationship {
            source: source.to_owned(),
            rel,
        });
    }

    /// The recorded parts, in package order.
    pub fn parts(&self) -> impl Iterator<Item = &PassthroughPart> {
        self.parts.values()
    }

    /// The recorded relationships.
    #[must_use]
    pub fn relationships(&self) -> &[PassthroughRelationship] {
        &self.rels
    }

    /// True when `part_name` is carried.
    #[must_use]
    pub fn has(&self, part_name: &str) -> bool {
        self.parts.contains_key(&part_key(part_name))
    }

    /// Number of parts carried.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// True when nothing is carried.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.rels.is_empty()
    }

    /// Total bytes held, for diagnostics and the Compatibility Checker.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.parts.values().map(|p| p.bytes.len()).sum()
    }

    /// Re-emits the recorded parts and relationships into `target`.
    ///
    /// `map_part` maps an original part name to its name in the new package, for consumed parts
    /// the writer renamed (`/xl/worksheets/sheet3.xml` → `/xl/worksheets/sheet1.xml`); it returns
    /// `None` when the part no longer exists, and relationships from it are then dropped and
    /// reported. Pass `|n| Some(n.to_owned())` for the identity.
    ///
    /// # Errors
    /// [`OpcError`] when a part or relationship cannot be added (for example a relationship id
    /// clash inside a passthrough part's own relationships).
    pub fn emit(
        &self,
        target: &mut OpcPackage,
        map_part: impl Fn(&str) -> Option<String>,
    ) -> Result<EmitResult, OpcError> {
        let mut result = EmitResult::default();

        // 1. Names: keep the original unless the writer already produced a part with that name.
        let mut new_names: HashMap<String, String> = HashMap::new();
        let mut taken: HashSet<String> = self.parts.keys().cloned().collect();
        for (key, p) in &self.parts {
            let mut name = p.name.clone();
            if target.has(&name) {
                let ext = extension_of(&name);
                let base = basename_of(&name);
                let stem = if ext.is_empty() {
                    base
                } else {
                    &base[..base.len() - ext.len() - 1]
                };
                let dot_ext = if ext.is_empty() {
                    String::new()
                } else {
                    format!(".{}", &base[base.len() - ext.len()..])
                };
                let dir = directory_of(&name).to_owned();
                name = (1u64..)
                    .map(|n| format!("{dir}{stem}_{n}{dot_ext}"))
                    .find(|c| !target.has(c) && !taken.contains(&part_key(c)))
                    .unwrap_or_else(|| format!("{dir}{stem}_0{dot_ext}"));
                taken.insert(part_key(&name));
                result.renamed_parts.insert(p.name.clone(), name.clone());
            }
            new_names.insert(key.clone(), name);
        }
        let map_target = |original: &str| -> Option<String> {
            new_names
                .get(&part_key(original))
                .cloned()
                .or_else(|| map_part(original))
        };

        // 2. Parts and their own relationships.
        for (key, p) in &self.parts {
            let name = new_names
                .get(key)
                .cloned()
                .unwrap_or_else(|| p.name.clone());
            target.add_part(&name, &p.content_type, p.bytes.clone())?;
            for r in &p.relationships {
                let stored = if r.is_external() {
                    r.target.clone()
                } else {
                    let original = resolve_target(&p.name, &r.target);
                    let mapped = map_target(&original).unwrap_or_else(|| original.clone());
                    if mapped == original && name == p.name {
                        r.target.clone()
                    } else {
                        relative_target(&name, &mapped)
                    }
                };
                target.add_relationship_raw(
                    &name,
                    &r.rel_type,
                    &stored,
                    Some(&r.id),
                    r.target_mode,
                )?;
            }
        }

        // 3. Relationships from consumed parts (or the package) to passthrough parts and
        //    external targets.
        for pr in &self.rels {
            let source = if pr.source == "/" {
                Some("/".to_owned())
            } else {
                map_part(&pr.source)
            };
            let Some(source) = source.filter(|s| s == "/" || target.has(s)) else {
                result.dropped.push(pr.clone());
                continue;
            };
            let id = {
                let rels = target.relationships(&source)?;
                if rels.get(&pr.rel.id).is_some() {
                    rels.next_id("rId")
                } else {
                    pr.rel.id.clone()
                }
            };
            let stored = if pr.rel.is_external() {
                pr.rel.target.clone()
            } else {
                let original = resolve_target(&pr.source, &pr.rel.target);
                let mapped = map_target(&original).unwrap_or_else(|| original.clone());
                if mapped == original && source == pr.source {
                    pr.rel.target.clone()
                } else {
                    relative_target(&source, &mapped)
                }
            };
            target.add_relationship_raw(
                &source,
                &pr.rel.rel_type,
                &stored,
                Some(&id),
                pr.rel.target_mode,
            )?;
            result
                .relationship_ids
                .entry(source)
                .or_default()
                .insert(pr.rel.id.clone(), id);
        }
        Ok(result)
    }
}
