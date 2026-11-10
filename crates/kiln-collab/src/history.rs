// SPDX-License-Identifier: AGPL-3.0-or-later
//! Version History: named, dated points of a shared document's history.
//!
//! A version is taken on every explicit save and once ten minutes have passed since the first
//! edit after the latest version ([`VersionHistory::tick`]). Versions live in the document's
//! CRDT (root map `versions`: author, time, origin, name and the CRDT frontiers), so everyone in
//! the document sees the same list, a rename reaches everyone, and the list travels and is
//! stored with the document. A version opens by forking the document's history at its
//! frontiers: [`VersionHistory::checkout_write`] and [`VersionHistory::checkout_deck`] give a
//! model to render read-only through the normal scene, and [`VersionHistory::export`] gives it
//! as a shallow snapshot (the state without history). Restoring writes the old state on top of
//! the current one as a new change, so nothing is lost and the restore is itself a version.
//!
//! A document keeps at most [`VersionHistory::MAX`] versions: past that, automatic versions are
//! thinned where they are densest; named versions, the first and the latest are kept.

use kiln_deck::Presentation;
use kiln_doc::Document;
use loro::{ExportMode, Frontiers, LoroMap};
use serde::{Deserialize, Serialize};

use crate::crdt::LoroCrdt;
use crate::deck::DeckBinding;
use crate::error::{CollabError, Result};
use crate::value::{
    find_map, get_bytes, get_i64, get_json, get_str, put_bytes, put_i64, put_json, put_str,
};
use crate::write::WriteBinding;

pub use crate::slides::{SlideChange, SlideMark, changed_slides};

/// The root map of versions.
pub(crate) const VERSIONS: &str = "versions";

/// Why a version was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    /// The document was saved.
    Save,
    /// Ten minutes of editing passed.
    Auto,
    /// A version was restored (its id).
    Restore(String),
    /// The document was shared: what it was then.
    Shared,
}

/// One version (what the Version History pane lists).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Stable id within the document.
    pub id: String,
    /// The name someone gave it.
    pub name: Option<String>,
    /// Who made it (the display name of the editing peer).
    pub author: String,
    /// When, in milliseconds since the Unix epoch.
    pub time_ms: u64,
    /// Why it was taken.
    pub origin: Origin,
}

/// One peer's part in Version History: it notes edits and takes versions; the versions
/// themselves are in the CRDT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VersionHistory {
    /// When the first edit since the latest version was made.
    edited_since: Option<u64>,
}

fn time_of(m: &LoroMap) -> u64 {
    get_i64(m, "time")
        .and_then(|t| u64::try_from(t).ok())
        .unwrap_or(0)
}

fn info(id: &str, m: &LoroMap) -> Option<VersionInfo> {
    Some(VersionInfo {
        id: id.to_owned(),
        name: get_str(m, "name").filter(|n| !n.is_empty()),
        author: get_str(m, "author").unwrap_or_default(),
        time_ms: time_of(m),
        origin: get_json(m, "origin").ok()?,
    })
}

impl VersionHistory {
    /// The most versions a document keeps.
    pub const MAX: usize = 200;
    /// How long editing runs before an automatic version.
    pub const INTERVAL_MS: u64 = 10 * 60 * 1000;

    /// The versions, oldest first.
    #[must_use]
    pub fn list(crdt: &LoroCrdt) -> Vec<VersionInfo> {
        let root = crdt.doc().get_map(VERSIONS);
        let mut out: Vec<VersionInfo> = root
            .keys()
            .filter_map(|k| find_map(&root, &k).and_then(|m| info(&k, &m)))
            .collect();
        out.sort_by(|a, b| a.time_ms.cmp(&b.time_ms).then_with(|| a.id.cmp(&b.id)));
        out
    }

    /// Notes an edit (local or remote) made at `now_ms`.
    pub fn note_edit(&mut self, now_ms: u64) {
        self.edited_since.get_or_insert(now_ms);
    }

    /// Takes a version because the document was saved, unless nothing changed since the
    /// latest one. Returns its id.
    ///
    /// # Errors
    ///
    /// The CRDT refused the write.
    pub fn on_save(
        &mut self,
        crdt: &LoroCrdt,
        author: &str,
        now_ms: u64,
    ) -> Result<Option<String>> {
        if let Some(v) = Self::list(crdt).pop()
            && Self::frontiers(crdt, &v.id).is_ok_and(|f| !crdt.changed_since(&f))
        {
            self.edited_since = None;
            return Ok(None);
        }
        self.take(crdt, author, now_ms, Origin::Save).map(Some)
    }

    /// Takes the first version of a document just shared. Returns its id.
    ///
    /// # Errors
    ///
    /// The CRDT refused the write.
    pub fn on_share(&mut self, crdt: &LoroCrdt, author: &str, now_ms: u64) -> Result<String> {
        self.take(crdt, author, now_ms, Origin::Shared)
    }

    /// Takes an automatic version once ten minutes have passed since the first edit after the
    /// latest version (call it on edits and on a timer). A version anyone took since that edit
    /// counts, so the people editing together do not each take one. Returns its id.
    ///
    /// # Errors
    ///
    /// The CRDT refused the write.
    pub fn tick(&mut self, crdt: &LoroCrdt, author: &str, now_ms: u64) -> Result<Option<String>> {
        let Some(since) = self.edited_since else {
            return Ok(None);
        };
        if Self::list(crdt).last().is_some_and(|v| v.time_ms > since) {
            self.edited_since = None;
            return Ok(None);
        }
        if now_ms.saturating_sub(since) < Self::INTERVAL_MS {
            return Ok(None);
        }
        self.take(crdt, author, now_ms, Origin::Auto).map(Some)
    }

    fn take(
        &mut self,
        crdt: &LoroCrdt,
        author: &str,
        now_ms: u64,
        origin: Origin,
    ) -> Result<String> {
        let frontiers = crdt.frontiers();
        let root = crdt.doc().get_map(VERSIONS);
        let mut id = format!("{:x}.{:x}", crdt.peer(), now_ms);
        // Two versions in one millisecond (a save straight after sharing) stay two.
        let mut n = 1;
        while root.get(&id).is_some() {
            id = format!("{:x}.{:x}.{n}", crdt.peer(), now_ms);
            n += 1;
        }
        let m = root.ensure_mergeable_map(&id)?;
        put_str(&m, "author", author)?;
        put_i64(&m, "time", i64::try_from(now_ms).unwrap_or(i64::MAX))?;
        put_json(&m, "origin", &origin)?;
        put_bytes(&m, "frontiers", &frontiers)?;
        Self::thin(crdt)?;
        crdt.commit_version();
        self.edited_since = None;
        Ok(id)
    }

    /// Names (or, with `None`, unnames) a version. Named versions are never thinned.
    ///
    /// # Errors
    ///
    /// [`CollabError::UnknownVersion`].
    pub fn rename(crdt: &LoroCrdt, id: &str, name: Option<&str>) -> Result<()> {
        let m = find_map(&crdt.doc().get_map(VERSIONS), id)
            .ok_or_else(|| CollabError::UnknownVersion(id.to_owned()))?;
        match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => put_str(&m, "name", n)?,
            None if m.get("name").is_some() => m.delete("name")?,
            None => {}
        }
        crdt.commit_version();
        Ok(())
    }

    fn frontiers(crdt: &LoroCrdt, id: &str) -> Result<Vec<u8>> {
        find_map(&crdt.doc().get_map(VERSIONS), id)
            .and_then(|m| get_bytes(&m, "frontiers"))
            .ok_or_else(|| CollabError::UnknownVersion(id.to_owned()))
    }

    /// A version as a shallow snapshot (its state without history), for saving it as a copy.
    ///
    /// # Errors
    ///
    /// An unknown version.
    pub fn export(crdt: &LoroCrdt, id: &str) -> Result<Vec<u8>> {
        let f = Frontiers::decode(&Self::frontiers(crdt, id)?)?;
        Ok(crdt.doc().export(ExportMode::state_only(Some(&f)))?)
    }

    /// A Write version as a model (read-only preview).
    ///
    /// # Errors
    ///
    /// An unknown version or a state that is not a Write document.
    pub fn checkout_write(crdt: &LoroCrdt, id: &str) -> Result<Document> {
        WriteBinding::read(crdt.fork_at(&Self::frontiers(crdt, id)?)?)
    }

    /// A Deck version as a model (read-only preview).
    ///
    /// # Errors
    ///
    /// An unknown version or a state that is not a presentation.
    pub fn checkout_deck(crdt: &LoroCrdt, id: &str) -> Result<Presentation> {
        DeckBinding::read(crdt.fork_at(&Self::frontiers(crdt, id)?)?)
    }

    /// Restores a Write version: the shared document becomes that version as a new change
    /// (sent to everyone like any edit, and undoable) and a new version records the restore.
    /// Returns the restored model; the caller replaces its document with it and lays
    /// everything out again.
    ///
    /// # Errors
    ///
    /// An unknown version, or a failure to write the CRDT.
    pub fn restore_write(
        &mut self,
        id: &str,
        binding: &mut WriteBinding,
        doc: &Document,
        author: &str,
        now_ms: u64,
    ) -> Result<Document> {
        let mut old = Self::checkout_write(binding.crdt(), id)?;
        old.set_id_lanes(doc.id_lanes());
        // Ids allocated since the version must not be handed out again.
        old.refresh_id_counter();
        old.reserve_ids_below(doc.peek_next_id());
        binding.replace(&old)?;
        self.take(
            binding.crdt(),
            author,
            now_ms,
            Origin::Restore(id.to_owned()),
        )?;
        Ok(old)
    }

    /// Restores a Deck version (see [`Self::restore_write`]).
    ///
    /// # Errors
    ///
    /// An unknown version, or a failure to write the CRDT.
    pub fn restore_deck(
        &mut self,
        id: &str,
        binding: &mut DeckBinding,
        pres: &Presentation,
        author: &str,
        now_ms: u64,
    ) -> Result<Presentation> {
        let mut old = Self::checkout_deck(binding.crdt(), id)?;
        old.set_id_lanes(pres.id_lanes());
        old.refresh_id_counter();
        old.reserve_ids_below(pres.peek_next_id());
        binding.replace(&old)?;
        self.take(
            binding.crdt(),
            author,
            now_ms,
            Origin::Restore(id.to_owned()),
        )?;
        Ok(old)
    }

    /// The version before `id`, if any (what its changes are measured against).
    #[must_use]
    pub fn previous(crdt: &LoroCrdt, id: &str) -> Option<String> {
        let list = Self::list(crdt);
        let i = list.iter().position(|v| v.id == id)?;
        i.checked_sub(1).map(|p| list[p].id.clone())
    }

    /// The slides of a Deck version with the ones that changed since the version before it
    /// marked (every slide is new in the first version).
    ///
    /// # Errors
    ///
    /// An unknown version or a state that is not a presentation.
    pub fn deck_changes(crdt: &LoroCrdt, id: &str) -> Result<Vec<SlideChange>> {
        let now = Self::checkout_deck(crdt, id)?;
        let before = match Self::previous(crdt, id) {
            Some(p) => Some(Self::checkout_deck(crdt, &p)?),
            None => None,
        };
        Ok(changed_slides(before.as_ref(), &now))
    }

    /// Keeps at most [`Self::MAX`] versions: removes the automatic version (not the first or
    /// the latest) whose neighbours are closest in time, then, if only named or saved versions
    /// are left, the oldest unnamed one. Named versions are never removed: a document whose
    /// versions are all named keeps more than [`Self::MAX`].
    fn thin(crdt: &LoroCrdt) -> Result<()> {
        let root = crdt.doc().get_map(VERSIONS);
        let mut list = Self::list(crdt);
        while list.len() > Self::MAX {
            let n = list.len();
            let auto = (1..n - 1)
                .filter(|&i| list[i].name.is_none() && list[i].origin == Origin::Auto)
                .min_by_key(|&i| list[i + 1].time_ms.saturating_sub(list[i - 1].time_ms));
            let Some(victim) = auto.or_else(|| (1..n - 1).find(|&i| list[i].name.is_none())) else {
                break;
            };
            root.delete(&list[victim].id)?;
            list.remove(victim);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::Crdt;
    use kiln_doc::{IdLanes, Pos, edit};

    fn typed(doc: &mut Document, b: &mut WriteBinding, text: &str) -> Result<()> {
        let p = doc.body_paragraphs()[0].id;
        let end = doc.body_paragraphs()[0].content.len();
        let tx = edit::insert_text(doc, Pos::new(p, end), text)?;
        doc.apply(tx.clone())?;
        b.commit(doc, &tx)
    }

    fn some<T>(v: Option<T>) -> Result<T> {
        v.ok_or_else(|| crate::error::corrupt("no version"))
    }

    #[test]
    fn saves_and_ten_minutes_of_editing_take_versions() -> Result<()> {
        let mut doc = Document::new();
        let mut b = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        assert_eq!(h.tick(b.crdt(), "Ada", 0)?, None, "nothing edited");
        typed(&mut doc, &mut b, "one")?;
        h.note_edit(1_000);
        assert_eq!(h.tick(b.crdt(), "Ada", 1_000)?, None, "too soon");
        typed(&mut doc, &mut b, " two")?;
        h.note_edit(60_000);
        assert_eq!(h.tick(b.crdt(), "Ada", 61_000)?, None, "still too soon");
        assert!(
            h.tick(b.crdt(), "Ada", 1_000 + VersionHistory::INTERVAL_MS)?
                .is_some()
        );
        assert_eq!(
            h.tick(b.crdt(), "Ada", 3 * VersionHistory::INTERVAL_MS)?,
            None,
            "no edits since"
        );
        assert_eq!(
            h.on_save(b.crdt(), "Ada", 2_000_000)?,
            None,
            "unchanged since"
        );
        typed(&mut doc, &mut b, " three")?;
        let saved = some(h.on_save(b.crdt(), "Ada", 2_000_001)?)?;
        VersionHistory::rename(b.crdt(), &saved, Some("Draft"))?;
        let names: Vec<_> = VersionHistory::list(b.crdt())
            .into_iter()
            .map(|v| (v.origin, v.name))
            .collect();
        assert_eq!(
            names,
            vec![(Origin::Auto, None), (Origin::Save, Some("Draft".into()))]
        );
        // The version records are not the user's changes: undo goes back through the typing.
        b.undo(&mut doc)?;
        assert_eq!(doc.body_paragraphs()[0].text(), "one two");
        Ok(())
    }

    #[test]
    fn everyone_sees_the_versions_and_restore_keeps_history() -> Result<()> {
        let mut doc = Document::new();
        let mut b = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        typed(&mut doc, &mut b, "first draft")?;
        let v1 = some(h.on_save(b.crdt(), "Ada", 10)?)?;
        typed(&mut doc, &mut b, " and more")?;
        h.on_save(b.crdt(), "Ada", 20)?;
        let old = VersionHistory::checkout_write(b.crdt(), &v1)?;
        assert_eq!(old.body_paragraphs()[0].text(), "first draft");
        let (mut other, mut other_doc) =
            WriteBinding::join(&b.crdt().snapshot()?, 2, IdLanes::default())?;
        assert_eq!(
            VersionHistory::list(other.crdt()).len(),
            2,
            "versions travel with the document"
        );
        VersionHistory::rename(other.crdt(), &v1, Some("Outline"))?;
        let restored = h.restore_write(&v1, &mut b, &doc, "Ada", 30)?;
        doc = restored;
        assert_eq!(doc.body_paragraphs()[0].text(), "first draft");
        assert_eq!(b.decode()?, doc);
        let to_other = b.crdt().export_from(&other.crdt().version())?;
        let to_b = other.crdt().export_from(&b.crdt().version())?;
        other.apply_remote(&mut other_doc, &to_other)?;
        let change = b.apply_remote(&mut doc, &to_b)?;
        assert!(change.tx.is_empty(), "a rename is not an edit");
        assert_eq!(other_doc, doc);
        let list = VersionHistory::list(b.crdt());
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name.as_deref(), Some("Outline"));
        assert_eq!(list[2].origin, Origin::Restore(v1.clone()));
        assert!(!VersionHistory::export(b.crdt(), &v1)?.is_empty());
        Ok(())
    }

    #[test]
    fn a_restore_never_hands_out_an_id_again() -> Result<()> {
        let mut doc = Document::new();
        let mut b = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        typed(&mut doc, &mut b, "one two")?;
        let v1 = some(h.on_save(b.crdt(), "Ada", 10)?)?;
        let p = doc.body_paragraphs()[0].id;
        let tx = edit::split_paragraph(&mut doc, Pos::new(p, 4))?;
        doc.apply(tx.clone())?;
        b.commit(&doc, &tx)?;
        let since = doc.body_paragraphs()[1].id;
        let mut restored = h.restore_write(&v1, &mut b, &doc, "Ada", 20)?;
        assert!(restored.alloc_id() > since);
        Ok(())
    }

    #[test]
    fn thinning_keeps_named_first_and_latest() -> Result<()> {
        let mut doc = Document::new();
        let mut b = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        let mut ids = Vec::new();
        for i in 0..(VersionHistory::MAX as u64 + 30) {
            typed(&mut doc, &mut b, "x")?;
            h.note_edit(i * VersionHistory::INTERVAL_MS);
            let id = some(h.tick(b.crdt(), "Ada", (i + 1) * VersionHistory::INTERVAL_MS)?)?;
            if i == 5 {
                VersionHistory::rename(b.crdt(), &id, Some("Keep"))?;
            }
            ids.push(id);
        }
        let list = VersionHistory::list(b.crdt());
        assert_eq!(list.len(), VersionHistory::MAX);
        assert_eq!(list[0].id, ids[0]);
        assert_eq!(list.last().map(|v| v.id.clone()), ids.last().cloned());
        assert!(list.iter().any(|v| v.name.as_deref() == Some("Keep")));
        Ok(())
    }

    #[test]
    fn named_versions_are_never_thinned() -> Result<()> {
        let mut doc = Document::new();
        let mut b = WriteBinding::share(&mut doc, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        let total = VersionHistory::MAX as u64 + 5;
        for i in 0..total {
            typed(&mut doc, &mut b, "x")?;
            let id = some(h.on_save(b.crdt(), "Ada", i + 1)?)?;
            VersionHistory::rename(b.crdt(), &id, Some(&format!("v{i}")))?;
        }
        typed(&mut doc, &mut b, "y")?;
        h.on_save(b.crdt(), "Ada", total + 1)?;
        let list = VersionHistory::list(b.crdt());
        let named = list.iter().filter(|v| v.name.is_some()).count();
        assert_eq!(named as u64, total, "every named version is kept");
        Ok(())
    }

    #[test]
    fn deck_versions_mark_changed_slides() -> Result<()> {
        let mut pres = Presentation::new();
        let mut b = DeckBinding::share(&mut pres, 1, IdLanes::default())?;
        let mut h = VersionHistory::default();
        let layout = pres.slides[0].layout;
        let tx = kiln_deck::edit::insert_slide(&mut pres, layout, 1)?;
        pres.apply(tx.clone())?;
        b.commit(&pres, &tx)?;
        let v1 = some(h.on_save(b.crdt(), "Ada", 1)?)?;
        let second = pres.slides[1].id;
        let tx = kiln_deck::edit::set_hidden(&mut pres, &[second], true)?;
        pres.apply(tx.clone())?;
        b.commit(&pres, &tx)?;
        let tx = kiln_deck::edit::insert_slide(&mut pres, layout, 2)?;
        pres.apply(tx.clone())?;
        b.commit(&pres, &tx)?;
        let v2 = some(h.on_save(b.crdt(), "Ada", 2)?)?;
        assert!(
            VersionHistory::deck_changes(b.crdt(), &v1)?
                .iter()
                .all(|c| c.mark == SlideMark::Added)
        );
        let marks: Vec<SlideMark> = VersionHistory::deck_changes(b.crdt(), &v2)?
            .iter()
            .map(|c| c.mark)
            .collect();
        assert_eq!(
            marks,
            vec![SlideMark::Same, SlideMark::Changed, SlideMark::Added]
        );
        let restored = h.restore_deck(&v1, &mut b, &pres, "Ada", 3)?;
        assert_eq!(restored.slides.len(), 2);
        assert!(b.decode()? == restored);
        Ok(())
    }
}
