// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package-wide state of one write: the target package, relationship allocation (reusing the
//! original relationship ids where possible, so preserved raw XML keeps pointing at the right
//! targets), media parts, document-wide ids (drawings, revisions, bookmarks) and the issues met.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_doc::Document;
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, part_key, rel_type, rel_type_matches};

use crate::read::RelTarget;

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

/// Relationship bookkeeping of one source part.
#[derive(Debug, Default)]
struct RelState {
    /// `type|mode|target` → id of the relationships already in the package.
    by_key: HashMap<String, String>,
    /// Ids of the original part's relationships (kept for their own targets).
    reserved: HashSet<String>,
}

fn rel_key(rel_type: &str, target: &str, external: bool) -> String {
    if external {
        format!("{rel_type}|E|{target}")
    } else {
        format!("{rel_type}|I|{}", part_key(target))
    }
}

/// File extensions for image content types.
fn extension_for(content_type: &str) -> &'static str {
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
        _ => "bin",
    }
}

pub(crate) struct WriteContext<'a> {
    pub pkg: OpcPackage,
    pub doc: &'a Document,
    /// Relationship id → target per original source part, as read.
    original_rels: &'a BTreeMap<String, BTreeMap<String, RelTarget>>,
    /// New part name (case-folded) → the part it was read from.
    original_name: HashMap<String, String>,
    rels: HashMap<String, RelState>,
    /// Model media key → written part name.
    media: BTreeMap<String, String>,
    /// Part names the writer produces (placeholders included), case-folded.
    pub produced: BTreeSet<String>,
    next_doc_pr: u64,
    next_revision: u32,
    next_bookmark: u64,
    /// Open bookmarks: name → ids of starts written without their end yet.
    open_bookmarks: HashMap<String, Vec<u64>>,
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
        doc: &'a Document,
        original_rels: &'a BTreeMap<String, BTreeMap<String, RelTarget>>,
        first_doc_pr: u64,
        first_revision: u32,
        first_bookmark: u64,
    ) -> Self {
        Self {
            pkg,
            doc,
            original_rels,
            original_name: HashMap::new(),
            rels: HashMap::new(),
            media: BTreeMap::new(),
            produced: BTreeSet::new(),
            next_doc_pr: first_doc_pr,
            next_revision: first_revision,
            next_bookmark: first_bookmark,
            open_bookmarks: HashMap::new(),
            issues: Vec::new(),
        }
    }

    /// Records that `part` holds content read from `original`.
    pub(crate) fn set_original(&mut self, part: &str, original: &str) {
        self.original_name
            .insert(part_key(part), original.to_owned());
    }

    /// The part `part` was read from (itself when unknown).
    pub(crate) fn original_of(&self, part: &str) -> String {
        self.original_name
            .get(&part_key(part))
            .cloned()
            .unwrap_or_else(|| part.to_owned())
    }

    /// The original relationships of the part `part` was read from.
    pub(crate) fn original_rels_of(&self, part: &str) -> Option<&'a BTreeMap<String, RelTarget>> {
        let original = self.original_of(part);
        self.original_rels.get(&original).or_else(|| {
            self.original_rels
                .iter()
                .find(|(k, _)| part_key(k) == part_key(&original))
                .map(|(_, v)| v)
        })
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

    fn rel_state(&mut self, source: &str) -> &mut RelState {
        let key = part_key(source);
        if !self.rels.contains_key(&key) {
            let mut state = RelState::default();
            if let Ok(rels) = self.pkg.relationships(source) {
                for r in rels.all() {
                    let target = if r.is_external() {
                        r.target.clone()
                    } else {
                        rels.resolve(r).unwrap_or_else(|| r.target.clone())
                    };
                    state
                        .by_key
                        .insert(rel_key(&r.rel_type, &target, r.is_external()), r.id.clone());
                }
            }
            if let Some(orig) = self.original_rels_of(source) {
                state.reserved = orig.keys().cloned().collect();
            }
            self.rels.insert(key.clone(), state);
        }
        self.rels.entry(key).or_default()
    }

    /// The id of a relationship from `source` to `target` (a part name, or a URL when
    /// `external`), adding it if needed: an existing relationship is shared, an original id with
    /// the same target is reused, otherwise a fresh `rIdN` that no original relationship used.
    pub(crate) fn rel(
        &mut self,
        source: &str,
        rel_type: &str,
        target: &str,
        external: bool,
    ) -> String {
        let k = rel_key(rel_type, target, external);
        if let Some(id) = self.rel_state(source).by_key.get(&k) {
            return id.clone();
        }
        let existing: HashSet<String> = self
            .pkg
            .relationships(source)
            .map(|r| r.all().iter().map(|x| x.id.clone()).collect())
            .unwrap_or_default();
        let original_id = self.original_rels_of(source).and_then(|orig| {
            orig.iter()
                .find(|(id, t)| {
                    rel_type_matches(&t.rel_type, rel_type)
                        && t.external == external
                        && (if external {
                            t.target == target
                        } else {
                            part_key(&t.target) == part_key(target)
                        })
                        && !existing.contains(*id)
                })
                .map(|(id, _)| id.clone())
        });
        let id = original_id.unwrap_or_else(|| {
            let reserved = &self.rel_state(source).reserved;
            (1u64..)
                .map(|n| format!("rId{n}"))
                .find(|id| !existing.contains(id) && !reserved.contains(id))
                .unwrap_or_else(|| "rId0".to_owned())
        });
        let added = self.pkg.add_relationship(
            source,
            rel_type,
            target,
            RelationshipOptions {
                id: Some(&id),
                external,
            },
        );
        if let Err(e) = added {
            self.issue(
                "relationship.failed",
                format!("A relationship to {target} could not be added ({e})."),
                source,
            );
        }
        self.rel_state(source).by_key.insert(k, id.clone());
        id
    }

    /// Forgets what is known of `source`'s relationships (after they were replaced wholesale).
    pub(crate) fn forget_rels(&mut self, source: &str) {
        self.rels.remove(&part_key(source));
    }

    /// Adds a relationship with a fixed id (raw XML refers to it); false when the id is taken.
    pub(crate) fn rel_with_id(
        &mut self,
        source: &str,
        id: &str,
        rel_type: &str,
        target: &str,
        external: bool,
    ) -> bool {
        let taken = self
            .pkg
            .relationships(source)
            .is_ok_and(|r| r.get(id).is_some());
        if taken {
            return false;
        }
        let ok = self
            .pkg
            .add_relationship(
                source,
                rel_type,
                target,
                RelationshipOptions {
                    id: Some(id),
                    external,
                },
            )
            .is_ok();
        if ok {
            self.rel_state(source)
                .by_key
                .entry(rel_key(rel_type, target, external))
                .or_insert_with(|| id.to_owned());
        }
        ok
    }

    /// The part holding a model media item, written on first use (under its original name when
    /// free). `None` when the document has no such media.
    pub(crate) fn media_part(&mut self, key: &str) -> Option<String> {
        if let Some(done) = self.media.get(key) {
            return Some(done.clone());
        }
        let m = self.doc.media.get(key)?;
        let valid = key.starts_with('/')
            && !key.ends_with('/')
            && kiln_ooxml::opc::validate_part_name(key).is_ok();
        let name = if valid && !self.pkg.has(key) {
            key.to_owned()
        } else {
            let ext = extension_for(&m.content_type);
            self.pkg
                .next_part_name(&format!("/word/media/image{{n}}.{ext}"))
                .unwrap_or_else(|_| format!("/word/media/image0.{ext}"))
        };
        if let Err(e) = self.pkg.add_part(&name, &m.content_type, m.data.clone()) {
            self.issue(
                "image.failed",
                format!("A picture could not be saved ({e})."),
                &name,
            );
            return None;
        }
        self.media.insert(key.to_owned(), name.clone());
        Some(name)
    }

    /// The relationship id from `source` to the part holding a model media item.
    pub(crate) fn image_rel(&mut self, source: &str, media_key: &str) -> Option<String> {
        let part = self.media_part(media_key)?;
        Some(self.rel(source, rel_type::IMAGE, &part, false))
    }

    pub(crate) fn next_doc_pr(&mut self) -> u64 {
        self.next_doc_pr += 1;
        self.next_doc_pr
    }

    /// A revision id for a tracked change that has none.
    pub(crate) fn next_revision(&mut self) -> u32 {
        self.next_revision = self.next_revision.saturating_add(1);
        self.next_revision
    }

    /// The id for a bookmark start (a fresh id, remembered for its end).
    pub(crate) fn bookmark_start(&mut self, name: &str) -> u64 {
        let id = self.next_bookmark;
        self.next_bookmark += 1;
        self.open_bookmarks
            .entry(name.to_owned())
            .or_default()
            .push(id);
        id
    }

    /// The id of the most recent open bookmark of that name, if any.
    pub(crate) fn bookmark_end(&mut self, name: &str) -> Option<u64> {
        let ids = self.open_bookmarks.get_mut(name)?;
        let id = ids.pop();
        if ids.is_empty() {
            self.open_bookmarks.remove(name);
        }
        id
    }
}
