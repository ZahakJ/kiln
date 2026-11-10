// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recent and pinned files (Kiln Home, Backstage ▸ Open).
//!
//! Each file is one value of the app storage's `Recents` area, under [`crate::key_of`] its
//! path, so two windows that open different files never overwrite each other's entries. A file
//! goes to the top when it is opened or saved; pinned files come first; at most [`MAX`] files
//! that are not pinned are kept.

use kiln_common::{KilnError, Result};
use kiln_protocol::{DocKind, HostOp, RecentFile, StoreArea};

/// The most unpinned files kept.
pub const MAX: usize = 50;

/// The recent files in memory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recents {
    files: Vec<RecentFile>,
}

impl Recents {
    /// The list read from the stored values (values that do not parse are left out).
    #[must_use]
    pub fn from_values<'a>(values: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut files: Vec<RecentFile> = values
            .into_iter()
            .filter_map(|v| serde_json::from_slice(v).ok())
            .collect();
        files.dedup_by(|a, b| a.path == b.path);
        Self { files }
    }

    /// Pinned first, then the most recently opened.
    #[must_use]
    pub fn sorted(&self) -> Vec<RecentFile> {
        let mut out = self.files.clone();
        out.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.opened_at_ms.cmp(&a.opened_at_ms))
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    /// A file was opened or saved at `now_ms`: it goes to the top (a pin stays). A thumbnail
    /// replaces the one kept, `None` keeps it. Returns the storage operations.
    ///
    /// # Errors
    ///
    /// `Internal` if the entry cannot be encoded.
    pub fn touch(
        &mut self,
        path: &str,
        name: &str,
        kind: DocKind,
        now_ms: u64,
        thumbnail: Option<String>,
    ) -> Result<Vec<HostOp>> {
        let entry = match self.files.iter_mut().find(|f| f.path == path) {
            Some(f) => {
                name.clone_into(&mut f.name);
                f.kind = kind;
                f.opened_at_ms = now_ms;
                if thumbnail.is_some() {
                    f.thumbnail = thumbnail;
                }
                f.clone()
            }
            None => {
                let f = RecentFile {
                    path: path.to_owned(),
                    name: name.to_owned(),
                    kind,
                    opened_at_ms: now_ms,
                    pinned: false,
                    thumbnail,
                };
                self.files.push(f.clone());
                f
            }
        };
        let mut ops = vec![put(&entry)?];
        ops.extend(self.prune());
        Ok(ops)
    }

    /// Pins or unpins a file.
    ///
    /// # Errors
    ///
    /// `NotFound` for a file that is not in the list.
    pub fn pin(&mut self, path: &str, pinned: bool) -> Result<Vec<HostOp>> {
        let f = self
            .files
            .iter_mut()
            .find(|f| f.path == path)
            .ok_or_else(|| KilnError::not_found(format!("{path} in the recent files")))?;
        f.pinned = pinned;
        let f = f.clone();
        let mut ops = vec![put(&f)?];
        ops.extend(self.prune());
        Ok(ops)
    }

    /// Removes a file from the list.
    pub fn remove(&mut self, path: &str) -> Vec<HostOp> {
        self.files.retain(|f| f.path != path);
        vec![delete(path)]
    }

    /// Drops the oldest unpinned files past [`MAX`].
    fn prune(&mut self) -> Vec<HostOp> {
        let mut unpinned: Vec<(u64, String)> = self
            .files
            .iter()
            .filter(|f| !f.pinned)
            .map(|f| (f.opened_at_ms, f.path.clone()))
            .collect();
        if unpinned.len() <= MAX {
            return Vec::new();
        }
        unpinned.sort();
        let excess = unpinned.len() - MAX;
        let gone: Vec<String> = unpinned.into_iter().take(excess).map(|(_, p)| p).collect();
        self.files.retain(|f| !gone.contains(&f.path));
        gone.iter().map(|p| delete(p)).collect()
    }
}

fn put(f: &RecentFile) -> Result<HostOp> {
    Ok(HostOp::StorePut {
        area: StoreArea::Recents,
        key: crate::key_of(&f.path),
        bytes: serde_json::to_vec(f).map_err(|e| KilnError::Internal(e.to_string()))?,
    })
}

fn delete(path: &str) -> HostOp {
    HostOp::StoreDelete {
        area: StoreArea::Recents,
        key: crate::key_of(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MemoryStorage, StorageHost, answer};

    fn apply(store: &mut MemoryStorage, ops: Vec<HostOp>) {
        for op in ops {
            answer(store, &op);
        }
    }

    fn reload(store: &MemoryStorage) -> Recents {
        let keys = store.list(StoreArea::Recents, "").unwrap();
        let values: Vec<Vec<u8>> = keys
            .iter()
            .filter_map(|k| store.get(StoreArea::Recents, k).unwrap())
            .collect();
        Recents::from_values(values.iter().map(Vec::as_slice))
    }

    #[test]
    fn opening_puts_a_file_on_top_and_pins_come_first() {
        let mut store = MemoryStorage::default();
        let mut r = Recents::default();
        apply(
            &mut store,
            r.touch("/a.docx", "a", DocKind::Write, 1, None).unwrap(),
        );
        apply(
            &mut store,
            r.touch("/b.xlsx", "b", DocKind::Grid, 2, None).unwrap(),
        );
        apply(
            &mut store,
            r.touch("/c.pptx", "c", DocKind::Deck, 3, Some("data:x".into()))
                .unwrap(),
        );
        apply(&mut store, r.pin("/a.docx", true).unwrap());
        apply(
            &mut store,
            r.touch("/c.pptx", "c", DocKind::Deck, 4, None).unwrap(),
        );
        let names: Vec<String> = reload(&store)
            .sorted()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, ["a", "c", "b"]);
        let c = reload(&store).sorted()[1].clone();
        assert_eq!(
            c.thumbnail.as_deref(),
            Some("data:x"),
            "the thumbnail stays"
        );
        apply(&mut store, r.remove("/b.xlsx"));
        assert_eq!(reload(&store).sorted().len(), 2);
        assert!(r.pin("/zzz", true).is_err());
    }

    #[test]
    fn the_oldest_unpinned_files_go_past_the_limit() {
        let mut store = MemoryStorage::default();
        let mut r = Recents::default();
        apply(
            &mut store,
            r.touch("/pinned", "p", DocKind::Write, 0, None).unwrap(),
        );
        apply(&mut store, r.pin("/pinned", true).unwrap());
        for i in 1..=(MAX as u64 + 5) {
            apply(
                &mut store,
                r.touch(&format!("/f{i}"), "f", DocKind::Write, i, None)
                    .unwrap(),
            );
        }
        let list = reload(&store).sorted();
        assert_eq!(list.len(), MAX + 1);
        assert_eq!(list[0].path, "/pinned");
        assert!(!list.iter().any(|f| f.path == "/f1"));
        assert!(list.iter().any(|f| f.path == format!("/f{}", MAX + 5)));
        // A damaged value is left out.
        let bad = [b"{".as_slice()];
        assert!(Recents::from_values(bad).sorted().is_empty());
    }
}
