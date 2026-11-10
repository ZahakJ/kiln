// SPDX-License-Identifier: AGPL-3.0-or-later
//! Version history of documents that are not shared (local files).
//!
//! A version is taken on every save and once ten minutes have passed since the first edit after
//! the latest version; each is the document's model at that moment. A `.kwrite`, `.kgrid` or
//! `.kdeck` keeps its versions inside the bundle (`versions/`). Any other file keeps them in
//! the app storage as a *history* with an id of its own (area `Versions`):
//!
//! - `h/<history>/index`: the [`HistoryIndex`] (the versions, their sizes, the file and the
//!   fingerprint of its contents as Kiln last saw them);
//! - `h/<history>/<version>`: each version's model;
//! - `p/<key of the path>`: the [`PathRecord`] of a file (its history and fingerprint);
//! - `f/<fingerprint>`: the history of a file with these exact contents.
//!
//! A file opened finds its history by its path when its contents are the ones Kiln last saved
//! or opened there, and otherwise by its contents alone (the file was moved or renamed by
//! another program, or picked again in the browser); a different document at the same path
//! starts a history of its own, so a version shown or restored is always one of the same
//! document. Save As takes the history along to the new name. [`tidy`] keeps the whole store
//! within [`TOTAL_BYTES`] (the oldest unnamed versions go first, across files) and finds the
//! histories no file leads to any more; the history of a file that is gone goes once it has
//! not been used for [`GONE_AFTER_MS`] ([`gone`]).
//!
//! A document keeps at most [`LocalHistory::MAX`] versions: past that the automatic version
//! whose neighbours are closest in time goes first, then the oldest unnamed one; named versions,
//! the first and the latest are never removed. Restoring a version makes the document that
//! version as a new change and takes a version of it, so nothing is lost.

use std::collections::{BTreeMap, BTreeSet};

use kiln_common::{KilnError, Result};
use kiln_protocol::collab::{VersionEntry, VersionOrigin};
use serde::{Deserialize, Serialize};

/// The most the local version histories of all files may take together.
pub const TOTAL_BYTES: u64 = 512 << 20;

/// How long the history of a file that is gone is kept (the file may only have been moved:
/// opened again from its new place, it finds its history by its contents).
pub const GONE_AFTER_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// The format of [`HistoryIndex`] this version writes.
pub const INDEX_FORMAT: u32 = 2;

/// The key of a history's index.
#[must_use]
pub fn index_key(history: &str) -> String {
    format!("h/{history}/index")
}

/// The key of one version's model.
#[must_use]
pub fn model_key(history: &str, id: &str) -> String {
    format!("h/{history}/{id}")
}

/// The key of a file's [`PathRecord`].
#[must_use]
pub fn path_key(path: &str) -> String {
    format!("p/{}", crate::key_of(path))
}

/// The key of the history of a file with these contents.
#[must_use]
pub fn print_key(print: &str) -> String {
    format!("f/{print}")
}

/// What a key of the `Versions` area is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionsKey<'a> {
    /// A history's index.
    Index(&'a str),
    /// A version's model.
    Model(&'a str, &'a str),
    /// A file's record.
    Path,
    /// A fingerprint's history.
    Print,
    /// Something this version does not write (it can go).
    Other,
}

/// Reads a key of the `Versions` area.
#[must_use]
pub fn parse_key(key: &str) -> VersionsKey<'_> {
    if let Some(rest) = key.strip_prefix("h/")
        && let Some((history, item)) = rest.split_once('/')
        && !history.is_empty()
        && !item.is_empty()
        && !item.contains('/')
    {
        return if item == "index" {
            VersionsKey::Index(history)
        } else {
            VersionsKey::Model(history, item)
        };
    }
    if key.starts_with("p/") {
        VersionsKey::Path
    } else if key.starts_with("f/") {
        VersionsKey::Print
    } else {
        VersionsKey::Other
    }
}

/// The fingerprint of a file's contents: its length and checksum.
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}-{:08x}", bytes.len(), crc32fast::hash(bytes))
}

/// `p/<key>`: the history of the file at a path and the fingerprint of its contents as Kiln
/// last saw them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRecord {
    /// The path (the key is its hash).
    pub path: String,
    /// The history.
    pub history: String,
    /// The fingerprint.
    pub print: String,
}

impl PathRecord {
    /// As stored (JSON).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Read back (`None` for bytes that are not a record).
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// The history a file opened with contents `print` at `path` goes on with: the one of the
/// path when the contents are those Kiln last saw there, otherwise the one of the contents.
#[must_use]
pub fn history_of(
    print: &str,
    at_path: Option<&PathRecord>,
    by_print: Option<&str>,
) -> Option<String> {
    match at_path {
        Some(r) if r.print == print => Some(r.history.clone()),
        _ => by_print.map(str::to_owned),
    }
}

/// `h/<history>/index`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryIndex {
    /// [`INDEX_FORMAT`].
    pub format: u32,
    /// The versions, oldest first.
    pub versions: Vec<VersionEntry>,
    /// The size of each version's model.
    #[serde(default)]
    pub sizes: BTreeMap<String, u64>,
    /// The file the history was last kept for.
    #[serde(default)]
    pub file: Option<String>,
    /// The fingerprint of that file's contents as Kiln last saw them.
    #[serde(default)]
    pub print: Option<String>,
    /// When the file was last opened or saved with this history.
    #[serde(default)]
    pub used_ms: u64,
}

impl HistoryIndex {
    /// As stored (JSON).
    ///
    /// # Errors
    ///
    /// `Internal` if it cannot be encoded.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| KilnError::Internal(e.to_string()))
    }

    /// Read back.
    ///
    /// # Errors
    ///
    /// `Format` for bytes that are not an index of this format or an older one.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let index: Self = serde_json::from_slice(bytes).map_err(|e| KilnError::Format {
            format: "version history".into(),
            message: e.to_string(),
        })?;
        if index.format > INDEX_FORMAT {
            return Err(KilnError::Format {
                format: "version history".into(),
                message: format!(
                    "written by a newer version of Kiln (format {})",
                    index.format
                ),
            });
        }
        Ok(index)
    }

    /// Bytes the history's models take.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.sizes.values().sum()
    }
}

/// What [`tidy`] decided.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tidied {
    /// Keys to delete.
    pub delete: Vec<String>,
    /// Indexes to write again (versions thinned).
    pub rewrite: BTreeMap<String, HistoryIndex>,
    /// Files to check: when one is gone, its history goes ([`gone`]).
    pub check: Vec<PathRecord>,
}

/// Looks after the whole `Versions` area: keys this version does not write, records that lead
/// nowhere and histories nothing leads to go; past [`TOTAL_BYTES`] (`cap`) the oldest unnamed
/// versions go across all histories, never a history's latest; `open` histories (of documents
/// open now) are left alone. `values` holds every index and record (the models are not read).
#[must_use]
pub fn tidy(
    keys: &[String],
    values: &BTreeMap<String, Option<Vec<u8>>>,
    open: &BTreeSet<String>,
    cap: u64,
) -> Tidied {
    let mut out = Tidied::default();
    let mut indexes: BTreeMap<String, HistoryIndex> = BTreeMap::new();
    let mut models: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut referenced: BTreeSet<String> = open.clone();
    let mut records: Vec<(String, PathRecord)> = Vec::new();
    let mut prints: Vec<(String, String)> = Vec::new();
    for key in keys {
        let value = values.get(key).and_then(Option::as_deref);
        match parse_key(key) {
            VersionsKey::Index(h) => match value.map(HistoryIndex::from_bytes) {
                Some(Ok(index)) => {
                    indexes.insert(h.to_owned(), index);
                }
                _ => out.delete.push(key.clone()),
            },
            VersionsKey::Model(h, v) => models.entry(h.to_owned()).or_default().push(v.to_owned()),
            VersionsKey::Path => match value.and_then(PathRecord::from_bytes) {
                Some(r) => records.push((key.clone(), r)),
                None => out.delete.push(key.clone()),
            },
            VersionsKey::Print => match value.and_then(|b| std::str::from_utf8(b).ok()) {
                Some(h) => prints.push((key.clone(), h.to_owned())),
                None => out.delete.push(key.clone()),
            },
            VersionsKey::Other => out.delete.push(key.clone()),
        }
    }
    for (key, r) in records {
        if indexes.contains_key(&r.history) || open.contains(&r.history) {
            referenced.insert(r.history.clone());
            out.check.push(r);
        } else {
            out.delete.push(key);
        }
    }
    for (key, h) in prints {
        if indexes.contains_key(&h) || open.contains(&h) {
            referenced.insert(h);
        } else {
            out.delete.push(key);
        }
    }
    // Histories nothing leads to, and models no index lists.
    for (h, vs) in &models {
        let listed: BTreeSet<&str> = indexes
            .get(h)
            .filter(|_| referenced.contains(h))
            .map(|i| i.versions.iter().map(|v| v.id.as_str()).collect())
            .unwrap_or_default();
        if open.contains(h) {
            continue;
        }
        for v in vs {
            if !listed.contains(v.as_str()) {
                out.delete.push(model_key(h, v));
            }
        }
    }
    indexes.retain(|h, _| {
        let keep = referenced.contains(h);
        if !keep {
            out.delete.push(index_key(h));
        }
        keep
    });
    // The cap, across files: the oldest unnamed version that is not its history's latest.
    let mut total: u64 = indexes.values().map(HistoryIndex::bytes).sum();
    while total > cap {
        let victim = indexes
            .iter()
            .filter(|(h, _)| !open.contains(*h))
            .flat_map(|(h, i)| {
                let last = i.versions.len().saturating_sub(1);
                i.versions[..last]
                    .iter()
                    .filter(|v| v.name.is_none())
                    .map(move |v| (v.time_ms, h.clone(), v.id.clone()))
            })
            .min();
        let Some((_, h, v)) = victim else { break };
        if let Some(index) = indexes.get_mut(&h) {
            index.versions.retain(|x| x.id != v);
            total = total.saturating_sub(index.sizes.remove(&v).unwrap_or(0));
            out.rewrite.insert(index_key(&h), index.clone());
        }
        out.delete.push(model_key(&h, &v));
    }
    out
}

/// A file with a history is gone (at `now_ms`): the keys to delete. Its record goes; when the
/// history was last kept for that file and has not been used for [`GONE_AFTER_MS`], the
/// history goes too (a file moved by another program and opened again within that time finds
/// it by its contents).
#[must_use]
pub fn gone(record: &PathRecord, index: Option<&HistoryIndex>, now_ms: u64) -> Vec<String> {
    let Some(index) = index.filter(|i| i.file.as_deref() == Some(record.path.as_str())) else {
        return vec![path_key(&record.path)];
    };
    if now_ms.saturating_sub(index.used_ms) < GONE_AFTER_MS {
        return Vec::new();
    }
    let mut out = vec![path_key(&record.path)];
    {
        out.push(print_key(&record.print));
        out.extend(
            index
                .versions
                .iter()
                .map(|v| model_key(&record.history, &v.id)),
        );
        out.push(index_key(&record.history));
    }
    out
}

/// The versions of one document and when it was first edited since the latest one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalHistory {
    versions: Vec<VersionEntry>,
    edited_since: Option<u64>,
}

impl LocalHistory {
    /// The most versions a document keeps (named ones aside).
    pub const MAX: usize = 200;
    /// How long editing runs before an automatic version.
    pub const INTERVAL_MS: u64 = 10 * 60 * 1000;

    /// A history of these versions (oldest first; they are sorted by time).
    #[must_use]
    pub fn new(mut versions: Vec<VersionEntry>) -> Self {
        versions.sort_by(|a, b| a.time_ms.cmp(&b.time_ms).then_with(|| a.id.cmp(&b.id)));
        Self {
            versions,
            edited_since: None,
        }
    }

    /// The versions, oldest first.
    #[must_use]
    pub fn list(&self) -> &[VersionEntry] {
        &self.versions
    }

    /// One version.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.id == id)
    }

    /// The version before `id` (what its changes are measured against).
    #[must_use]
    pub fn previous(&self, id: &str) -> Option<&VersionEntry> {
        let i = self.versions.iter().position(|v| v.id == id)?;
        i.checked_sub(1).map(|p| &self.versions[p])
    }

    /// Notes an edit made at `now_ms`.
    pub fn note_edit(&mut self, now_ms: u64) {
        self.edited_since.get_or_insert(now_ms);
    }

    /// Whether the document changed since the latest version.
    #[must_use]
    pub fn edited(&self) -> bool {
        self.edited_since.is_some()
    }

    /// Whether ten minutes of editing call for an automatic version now.
    #[must_use]
    pub fn due(&self, now_ms: u64) -> bool {
        self.edited_since
            .is_some_and(|t| now_ms.saturating_sub(t) >= Self::INTERVAL_MS)
    }

    /// Adds a version taken at `time_ms` and returns it with the ids thinning removed (their
    /// models can go). The edit clock starts again.
    pub fn take(
        &mut self,
        author: &str,
        time_ms: u64,
        origin: VersionOrigin,
    ) -> (VersionEntry, Vec<String>) {
        self.edited_since = None;
        self.add(author, time_ms, origin)
    }

    /// Adds a version of the document as it was when [`LocalHistory::clear_edits`] was called
    /// (a save whose file is written later): edits made since still count.
    pub fn add(
        &mut self,
        author: &str,
        time_ms: u64,
        origin: VersionOrigin,
    ) -> (VersionEntry, Vec<String>) {
        let entry = VersionEntry {
            id: self.new_id(time_ms),
            name: None,
            author: author.to_owned(),
            time_ms,
            origin,
        };
        self.versions.push(entry.clone());
        let removed = self.thin();
        (entry, removed)
    }

    /// The document as it is now is about to become a version: the edit clock starts again.
    pub fn clear_edits(&mut self) {
        self.edited_since = None;
    }

    /// Names a version (an empty name removes the name).
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown version.
    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        let v = self
            .versions
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or_else(|| KilnError::not_found(format!("version {id}")))?;
        let name = name.trim();
        v.name = (!name.is_empty()).then(|| name.chars().take(120).collect());
        Ok(())
    }

    /// Removes a version (the history's cap took it).
    pub fn remove(&mut self, id: &str) {
        self.versions.retain(|v| v.id != id);
    }

    /// A fresh id: the time in base 36, made unique among the versions.
    fn new_id(&self, time_ms: u64) -> String {
        let base = format!("v{}", base36(time_ms));
        let mut id = base.clone();
        let mut n = 1;
        while self.versions.iter().any(|v| v.id == id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        id
    }

    /// Keeps at most [`Self::MAX`] versions (see the module documentation). Returns the
    /// removed ids.
    fn thin(&mut self) -> Vec<String> {
        let mut removed = Vec::new();
        while self.versions.len() > Self::MAX {
            let list = &self.versions;
            let n = list.len();
            let auto = (1..n - 1)
                .filter(|&i| list[i].name.is_none() && list[i].origin == VersionOrigin::Auto)
                .min_by_key(|&i| list[i + 1].time_ms.saturating_sub(list[i - 1].time_ms));
            let Some(victim) = auto.or_else(|| (1..n - 1).find(|&i| list[i].name.is_none())) else {
                break;
            };
            removed.push(self.versions.remove(victim).id);
        }
        removed
    }
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_ten_minutes_of_editing_take_versions() {
        let mut h = LocalHistory::default();
        assert!(!h.due(0) && !h.edited());
        h.note_edit(1_000);
        h.note_edit(5_000);
        assert!(!h.due(60_000));
        assert!(h.due(1_000 + LocalHistory::INTERVAL_MS));
        let (v, removed) = h.take(
            "You",
            1_000 + LocalHistory::INTERVAL_MS,
            VersionOrigin::Auto,
        );
        assert!(removed.is_empty());
        assert!(
            !h.due(3 * LocalHistory::INTERVAL_MS),
            "the clock starts again"
        );
        let (w, _) = h.take(
            "You",
            1_000 + LocalHistory::INTERVAL_MS,
            VersionOrigin::Save,
        );
        assert_ne!(v.id, w.id, "ids are unique even at the same time");
        assert_eq!(h.previous(&w.id).map(|p| p.id.clone()), Some(v.id.clone()));
        assert!(h.previous(&v.id).is_none());
    }

    #[test]
    fn named_versions_are_never_thinned() {
        let mut h = LocalHistory::default();
        let (first, _) = h.take("You", 0, VersionOrigin::Save);
        h.rename(&first.id, "Original").unwrap();
        let mut named = Vec::new();
        for i in 1..=(LocalHistory::MAX as u64 + 50) {
            let (v, removed) = h.take("You", i * 1000, VersionOrigin::Auto);
            if i % 10 == 0 {
                h.rename(&v.id, &format!("Milestone {i}")).unwrap();
                named.push(v.id.clone());
            }
            for r in removed {
                assert!(!named.contains(&r), "{r} was named");
            }
        }
        assert_eq!(h.list().len(), LocalHistory::MAX);
        assert_eq!(h.list()[0].id, first.id, "the first version stays");
        for id in &named {
            assert!(h.get(id).is_some(), "{id}");
        }
        // Every version named: nothing can go, so more than MAX are kept.
        let mut all = LocalHistory::default();
        for i in 0..(LocalHistory::MAX as u64 + 3) {
            let (v, _) = all.take("You", i, VersionOrigin::Save);
            all.rename(&v.id, "kept").unwrap();
        }
        assert_eq!(all.list().len(), LocalHistory::MAX + 3);
    }

    #[test]
    fn a_history_is_stored_and_renamed() {
        let mut h = LocalHistory::default();
        let (v, _) = h.take("Ana", 42, VersionOrigin::Restore("v1".into()));
        h.rename(&v.id, "  Before review  ").unwrap();
        let index = HistoryIndex {
            format: INDEX_FORMAT,
            versions: h.list().to_vec(),
            sizes: BTreeMap::from([(v.id.clone(), 10)]),
            file: Some("/a.docx".into()),
            print: Some(fingerprint(b"abc")),
            used_ms: 7,
        };
        let back = HistoryIndex::from_bytes(&index.to_bytes().unwrap()).unwrap();
        assert_eq!(back, index);
        assert_eq!(
            LocalHistory::new(back.versions)
                .get(&v.id)
                .and_then(|v| v.name.clone())
                .as_deref(),
            Some("Before review")
        );
        h.rename(&v.id, "").unwrap();
        assert_eq!(h.get(&v.id).unwrap().name, None);
        assert!(h.rename("nope", "x").is_err());
        assert!(HistoryIndex::from_bytes(b"{").is_err());
        assert!(HistoryIndex::from_bytes(br#"{"format":9,"versions":[]}"#).is_err());
        assert_eq!(index_key("k2"), "h/k2/index");
        assert_eq!(parse_key("h/k2/v1"), VersionsKey::Model("k2", "v1"));
        assert_eq!(parse_key(&index_key("k2")), VersionsKey::Index("k2"));
        assert_eq!(parse_key(&path_key("/a.docx")), VersionsKey::Path);
        assert_eq!(parse_key("0123456789abcdef/index"), VersionsKey::Other);
        assert_eq!(base36(0), "0");
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
    }

    #[test]
    fn a_file_finds_its_history_by_path_and_contents_and_another_document_never_does() {
        let saved = PathRecord {
            path: "/d/Plan.docx".into(),
            history: "h1".into(),
            print: fingerprint(b"plan as saved"),
        };
        // The same file: its history.
        assert_eq!(
            history_of(&fingerprint(b"plan as saved"), Some(&saved), None).as_deref(),
            Some("h1")
        );
        // Another document saved over it: none, even though the path is the same.
        assert_eq!(
            history_of(&fingerprint(b"another"), Some(&saved), None),
            None
        );
        // Moved or renamed by another program (or picked again): found by its contents.
        assert_eq!(
            history_of(&fingerprint(b"plan as saved"), None, Some("h1")).as_deref(),
            Some("h1")
        );
        assert_ne!(fingerprint(b"ab"), fingerprint(b"ba"));
    }

    fn entry(id: &str, time_ms: u64, name: Option<&str>) -> VersionEntry {
        VersionEntry {
            id: id.into(),
            name: name.map(str::to_owned),
            author: "You".into(),
            time_ms,
            origin: VersionOrigin::Save,
        }
    }

    #[test]
    fn tidy_keeps_the_store_within_its_cap_and_drops_what_leads_nowhere() {
        let index = |file: &str, vs: Vec<VersionEntry>| HistoryIndex {
            format: INDEX_FORMAT,
            sizes: vs.iter().map(|v| (v.id.clone(), 100)).collect(),
            versions: vs,
            file: Some(file.into()),
            print: Some("p".into()),
            used_ms: 0,
        };
        let a = index(
            "/a.docx",
            vec![
                entry("a1", 1, None),
                entry("a2", 5, Some("Draft")),
                entry("a3", 9, None),
            ],
        );
        let b = index("/b.docx", vec![entry("b1", 2, None), entry("b2", 3, None)]);
        let orphan = index("/gone.docx", vec![entry("o1", 0, None)]);
        let rec = |path: &str, h: &str| PathRecord {
            path: path.into(),
            history: h.into(),
            print: "p".into(),
        };
        let mut values: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        values.insert(index_key("ha"), Some(a.to_bytes().unwrap()));
        values.insert(index_key("hb"), Some(b.to_bytes().unwrap()));
        values.insert(index_key("ho"), Some(orphan.to_bytes().unwrap()));
        values.insert(path_key("/a.docx"), Some(rec("/a.docx", "ha").to_bytes()));
        values.insert(path_key("/b.docx"), Some(rec("/b.docx", "hb").to_bytes()));
        values.insert(
            path_key("/x.docx"),
            Some(rec("/x.docx", "missing").to_bytes()),
        );
        values.insert(print_key("p"), Some(b"hb".to_vec()));
        let mut keys: Vec<String> = values.keys().cloned().collect();
        keys.extend(
            [
                "h/ha/a1",
                "h/ha/a2",
                "h/ha/a3",
                "h/hb/b1",
                "h/hb/b2",
                "h/ho/o1",
                "h/ha/stray",
            ]
            .map(str::to_owned),
        );
        keys.push("0123456789abcdef/index".into());
        // 600 bytes of models; a cap of 300 takes the three oldest unnamed that are not latest.
        let t = tidy(&keys, &values, &BTreeSet::new(), 300);
        for k in [
            "h/ho/o1",
            "h/ho/index",
            "h/ha/stray",
            "0123456789abcdef/index",
            "h/ha/a1",
            "h/hb/b1",
        ] {
            assert!(t.delete.contains(&k.to_owned()), "{k}: {:?}", t.delete);
        }
        assert!(
            t.delete.contains(&path_key("/x.docx")),
            "a record that leads nowhere"
        );
        for k in ["h/ha/a2", "h/ha/a3", "h/hb/b2"] {
            assert!(
                !t.delete.contains(&k.to_owned()),
                "{k} stays (named or latest)"
            );
        }
        assert_eq!(t.rewrite.len(), 2);
        assert_eq!(
            t.check.len(),
            2,
            "the files of /a.docx and /b.docx are checked"
        );
        // A file that is gone takes its history with it, once it has gone unused a while.
        assert!(gone(&rec("/a.docx", "ha"), Some(&a), GONE_AFTER_MS - 1).is_empty());
        let g = gone(&rec("/a.docx", "ha"), Some(&a), GONE_AFTER_MS);
        assert!(g.contains(&index_key("ha")) && g.contains(&"h/ha/a2".to_owned()));
        assert_eq!(
            gone(&rec("/old.docx", "ha"), Some(&a), GONE_AFTER_MS),
            [path_key("/old.docx")],
            "kept for another file"
        );
    }
}
