// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package-wide state of one write: the package being built, relationship allocation (reusing
//! the original ids where possible, so kept XML keeps pointing at the right targets), the parts
//! of the model's store written so far, and the issues met.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_deck::Presentation;
use kiln_ooxml::opc::{
    OpcPackage, Relationships, part_key, rel_type_matches, rels_part_name, resolve_target,
    validate_part_name,
};

/// Something the writer could not write exactly (also listed by the compatibility report).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteIssue {
    /// A stable code (`image.missing`, `relationship.missing`, …).
    pub code: String,
    /// One-line description.
    pub message: String,
    /// The part concerned.
    pub part: String,
}

impl std::fmt::Display for WriteIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.code, self.part, self.message)
    }
}

fn rel_key(rel_type: &str, target: &str, external: bool) -> String {
    // Strict and transitional spellings of a type are one relationship.
    let t = kiln_ooxml::opc::to_transitional_rel_type(rel_type);
    if external {
        format!("{t}|E|{target}")
    } else {
        format!("{t}|I|{}", part_key(target))
    }
}

/// The usual extension of a media type, for parts the writer has to name.
pub(crate) fn extension_for(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "image/x-emf" => "emf",
        "image/x-wmf" => "wmf",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/wav" | "audio/x-wav" => "wav",
        "video/quicktime" => "mov",
        "video/x-ms-wmv" => "wmv",
        _ => "bin",
    }
}

pub(crate) struct WriteContext<'a> {
    pub pkg: OpcPackage,
    pub pres: &'a Presentation,
    original: Option<&'a OpcPackage>,
    /// Relationships of the original package by source part key (parsed once).
    original_rels: HashMap<String, Option<Relationships>>,
    /// Per source part key: `type|mode|target` → id.
    by_key: HashMap<String, HashMap<String, String>>,
    /// Model part key → written part name.
    media: BTreeMap<String, String>,
    /// Parts the writer produced from the model (not copies), case-folded.
    pub produced: BTreeSet<String>,
    /// Model parts copied byte for byte from the original (untouched by edits), case-folded.
    pub preserved: BTreeSet<String>,
    pub issues: Vec<WriteIssue>,
}

impl std::fmt::Debug for WriteContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteContext")
            .field("parts", &self.pkg.len())
            .field("issues", &self.issues.len())
            .finish_non_exhaustive()
    }
}

impl<'a> WriteContext<'a> {
    pub(crate) fn new(
        pkg: OpcPackage,
        pres: &'a Presentation,
        original: Option<&'a OpcPackage>,
    ) -> Self {
        Self {
            pkg,
            pres,
            original,
            original_rels: HashMap::new(),
            by_key: HashMap::new(),
            media: BTreeMap::new(),
            produced: BTreeSet::new(),
            preserved: BTreeSet::new(),
            issues: Vec::new(),
        }
    }

    /// The main part of the original package.
    pub(crate) fn original_name_of_main(&self) -> Option<String> {
        self.original
            .and_then(|o| o.main_document_part().ok().flatten())
    }

    pub(crate) fn issue(&mut self, code: &str, message: impl Into<String>, part: &str) {
        let message = message.into();
        if self
            .issues
            .iter()
            .any(|i| i.code == code && i.part == part && i.message == message)
        {
            return;
        }
        self.issues.push(WriteIssue {
            code: code.to_owned(),
            message,
            part: part.to_owned(),
        });
    }

    /// The relationships the part of this name had in the original package.
    pub(crate) fn original_rels_of(&mut self, part: &str) -> Option<&Relationships> {
        let key = part_key(part);
        if !self.original_rels.contains_key(&key) {
            let parsed = self
                .original
                .and_then(|o| o.relationships(part).ok())
                .map(std::borrow::Cow::into_owned)
                .filter(|r| !r.is_empty());
            self.original_rels.insert(key.clone(), parsed);
        }
        self.original_rels.get(&key).and_then(Option::as_ref)
    }

    /// The original target of a part's first relationship of a type (a part name).
    pub(crate) fn original_target(&mut self, part: &str, rel_type: &str) -> Option<String> {
        let rels = self.original_rels_of(part)?;
        let r = rels
            .all()
            .iter()
            .find(|r| !r.is_external() && rel_type_matches(&r.rel_type, rel_type))?;
        rels.resolve(r)
    }

    fn existing_ids(&self, source: &str) -> HashSet<String> {
        self.pkg
            .relationships(source)
            .map(|r| r.all().iter().map(|x| x.id.clone()).collect())
            .unwrap_or_default()
    }

    fn keys_of(&mut self, source: &str) -> &mut HashMap<String, String> {
        let key = part_key(source);
        if !self.by_key.contains_key(&key) {
            let mut map = HashMap::new();
            if let Ok(rels) = self.pkg.relationships(source) {
                for r in rels.all() {
                    let target = if r.is_external() {
                        r.target.clone()
                    } else {
                        rels.resolve(r).unwrap_or_else(|| r.target.clone())
                    };
                    map.insert(rel_key(&r.rel_type, &target, r.is_external()), r.id.clone());
                }
            }
            self.by_key.insert(key.clone(), map);
        }
        self.by_key.entry(key).or_default()
    }

    /// The id of a relationship from `source` to `target` (a part name, or a URL when
    /// `external`), adding it when needed: an existing relationship is shared, the id the
    /// original part used for the same target is reused, otherwise a fresh `rIdN` that neither
    /// the package nor the original part uses.
    pub(crate) fn rel(
        &mut self,
        source: &str,
        rel_type: &str,
        target: &str,
        external: bool,
    ) -> String {
        let k = rel_key(rel_type, target, external);
        if let Some(id) = self.keys_of(source).get(&k) {
            return id.clone();
        }
        let existing = self.existing_ids(source);
        let original: Option<(Option<String>, HashSet<String>)> =
            self.original_rels_of(source).map(|rels| {
                let same = rels
                    .all()
                    .iter()
                    .find(|r| {
                        let t = if r.is_external() {
                            r.target.clone()
                        } else {
                            rels.resolve(r).unwrap_or_default()
                        };
                        rel_key(&r.rel_type, &t, r.is_external()) == k
                    })
                    .map(|r| r.id.clone());
                (same, rels.all().iter().map(|r| r.id.clone()).collect())
            });
        let (same, reserved) = original.unwrap_or_default();
        let id = same.filter(|id| !existing.contains(id)).unwrap_or_else(|| {
            (1u64..)
                .map(|n| format!("rId{n}"))
                .find(|c| !existing.contains(c) && !reserved.contains(c))
                .unwrap_or_else(|| "rId0".to_owned())
        });
        self.add_rel(source, rel_type, target, external, &id);
        self.keys_of(source).insert(k, id.clone());
        id
    }

    fn add_rel(&mut self, source: &str, rel_type: &str, target: &str, external: bool, id: &str) {
        let options = kiln_ooxml::opc::RelationshipOptions {
            id: Some(id),
            external,
        };
        if let Err(e) = self.pkg.add_relationship(source, rel_type, target, options) {
            self.issue(
                "relationship.failed",
                format!("A relationship could not be written ({e})."),
                source,
            );
        }
    }

    /// A hyperlink of the original part that resolves to `resolved` but whose target is not a
    /// part (`#_ftn1`, a link inside the document): its id and target as written. The reader
    /// keeps such links as the part name they resolve to; this puts the original target back.
    pub(crate) fn original_internal_link(&mut self, part: &str, resolved: &str) -> Option<String> {
        let found = self.original_rels_of(part).and_then(|rels| {
            rels.all()
                .iter()
                .find(|r| {
                    !r.is_external()
                        && rel_type_matches(&r.rel_type, kiln_ooxml::opc::rel_type::HYPERLINK)
                        && rels
                            .resolve(r)
                            .is_some_and(|t| part_key(&t) == part_key(resolved))
                })
                .map(|r| (r.id.clone(), r.target.clone(), r.rel_type.clone()))
        })?;
        let (id, raw, rel_type) = found;
        let k = format!("raw|{raw}");
        if let Some(existing) = self.keys_of(part).get(&k) {
            return Some(existing.clone());
        }
        let id = if self.existing_ids(part).contains(&id) {
            let existing = self.existing_ids(part);
            (1u64..)
                .map(|n| format!("rId{n}"))
                .find(|c| !existing.contains(c))
                .unwrap_or_else(|| "rId0".to_owned())
        } else {
            id
        };
        self.pkg
            .add_relationship_raw(
                part,
                &rel_type,
                &raw,
                Some(&id),
                kiln_ooxml::st::TargetMode::Internal,
            )
            .ok()?;
        self.keys_of(part).insert(k, id.clone());
        Some(id)
    }

    /// Makes sure every relationship id kept XML in `part` refers to exists: an id the package
    /// lacks is re-created from the original part's relationship of that id (its target written
    /// from the model's store when needed). Ids that lead nowhere are reported.
    pub(crate) fn repair_refs(&mut self, part: &str, ids: &BTreeSet<String>) {
        for id in ids {
            if self.existing_ids(part).contains(id) {
                continue;
            }
            let found = self.original_rels_of(part).and_then(|rels| {
                let r = rels.get(id)?;
                let target = if r.is_external() {
                    r.target.clone()
                } else {
                    rels.resolve(r)?
                };
                Some((r.rel_type.clone(), target, r.is_external()))
            });
            let Some((rel_type, target, external)) = found else {
                self.issue(
                    "relationship.missing",
                    format!("Kept content refers to a relationship ({id}) that no longer exists."),
                    part,
                );
                continue;
            };
            let target = if external || self.pkg.has(&target) {
                target
            } else if let Some(written) = self.media_part(&target) {
                written
            } else {
                self.issue(
                    "relationship.missing",
                    format!("Kept content refers to a part ({target}) that is not in the file."),
                    part,
                );
                continue;
            };
            self.add_rel(part, &rel_type, &target, external, id);
            let k = rel_key(&rel_type, &target, external);
            self.keys_of(part).insert(k, id.clone());
        }
    }

    /// Writes a part of the model's store (an image, a clip, a chart with its relationships and
    /// embedded workbook) and returns the name it has in the package. The part keeps its name
    /// when it can; a key that is not a part name, or a name already taken by other bytes, gets
    /// a fresh `/ppt/media/…` name. `None` when the store has no such part.
    pub(crate) fn media_part(&mut self, key: &str) -> Option<String> {
        if let Some(n) = self.media.get(key) {
            return Some(n.clone());
        }
        let part = self.pres.parts.get(key)?;
        let usable = key.starts_with('/') && validate_part_name(key).is_ok();
        let same_bytes = |pkg: &OpcPackage, name: &str| {
            pkg.part(name)
                .ok()
                .flatten()
                .is_some_and(|b| b.as_ref() == part.bytes.as_slice())
        };
        let name = if usable && same_bytes(&self.pkg, key) {
            // Already there (a picture shared with a kept SmartArt drawing).
            key.to_owned()
        } else if usable && !self.pkg.has(key) {
            if let Err(e) = self
                .pkg
                .add_part(key, &part.content_type, part.bytes.clone())
            {
                self.issue(
                    "media.failed",
                    format!("A part could not be written ({e})."),
                    key,
                );
                return None;
            }
            key.to_owned()
        } else {
            let ext = extension_for(&part.content_type);
            let stem = if part.content_type.starts_with("image/") {
                "image"
            } else {
                "media"
            };
            let name = self
                .pkg
                .next_part_name(&format!("/ppt/media/{stem}{{n}}.{ext}"))
                .ok()?;
            if let Err(e) = self
                .pkg
                .add_part(&name, &part.content_type, part.bytes.clone())
            {
                self.issue(
                    "media.failed",
                    format!("A part could not be written ({e})."),
                    key,
                );
                return None;
            }
            name
        };
        self.media.insert(key.to_owned(), name.clone());
        // A chart (or any stored part) brings its relationships part and their targets.
        let rels_name = rels_part_name(key);
        if name == key
            && let Some(rels) = self.pres.parts.get(&rels_name)
            && !self.pkg.has(&rels_name)
        {
            let bytes = rels.bytes.clone();
            if self
                .pkg
                .add_part(&rels_name, &rels.content_type, bytes)
                .is_ok()
            {
                let targets: Vec<String> = self
                    .pkg
                    .relationships(&name)
                    .map(|r| {
                        r.all()
                            .iter()
                            .filter(|x| !x.is_external())
                            .map(|x| resolve_target(&name, &x.target))
                            .collect()
                    })
                    .unwrap_or_default();
                for t in targets {
                    if !self.pkg.has(&t) && self.media_part(&t).is_none() {
                        self.issue(
                            "relationship.missing",
                            format!("A part ({t}) the chart refers to is missing."),
                            &name,
                        );
                    }
                }
            }
        }
        Some(name)
    }

    /// Copies an untouched part and its relationships from the original package, when every
    /// relationship still leads somewhere (a part of the new package or of the model's store).
    /// Returns false, changing nothing, when it cannot be copied faithfully.
    pub(crate) fn preserve(&mut self, part: &str) -> bool {
        let Some(orig) = self.original else {
            return false;
        };
        let Ok(Some(bytes)) = orig.part(part) else {
            return false;
        };
        let bytes = bytes.into_owned();
        let Ok(rels) = orig.relationships(part) else {
            return false;
        };
        let rels = rels.into_owned();
        let existing = self.existing_ids(part);
        for r in rels.all() {
            if r.is_external() || existing.contains(&r.id) {
                continue;
            }
            let Some(target) = rels.resolve(r) else {
                return false;
            };
            if !self.pkg.has(&target) && !self.pres.parts.contains_key(&target) {
                return false;
            }
        }
        for r in rels.all() {
            if existing.contains(&r.id) {
                continue;
            }
            if !r.is_external()
                && let Some(target) = rels.resolve(r)
                && !self.pkg.has(&target)
                && self.media_part(&target).as_deref() != Some(target.as_str())
            {
                // The store's part could not be written under its own name.
                return false;
            }
        }
        for r in rels.all() {
            if existing.contains(&r.id) {
                continue;
            }
            if let Err(e) = self.pkg.add_relationship_raw(
                part,
                &r.rel_type,
                &r.target,
                Some(&r.id),
                r.target_mode,
            ) {
                self.issue(
                    "relationship.failed",
                    format!("A relationship could not be copied ({e})."),
                    part,
                );
            }
        }
        if self.pkg.set_part(part, bytes).is_err() {
            return false;
        }
        self.preserved.insert(part_key(part));
        self.by_key.remove(&part_key(part));
        true
    }

    /// Records that a part was produced from the model.
    pub(crate) fn produced(&mut self, part: &str) {
        self.produced.insert(part_key(part));
    }
}
