// SPDX-License-Identifier: AGPL-3.0-or-later
//! Room persistence in one redb file: each room's latest snapshot, the updates received after
//! it (in order) and its metadata (kind, peer lanes). A room is restored by merging its
//! snapshot and updates; compaction folds the updates into a new snapshot.

use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

/// `(room, sequence)` → update.
const UPDATES: TableDefinition<'_, (&str, u64), &[u8]> = TableDefinition::new("updates");
/// room → snapshot.
const SNAPSHOTS: TableDefinition<'_, &str, &[u8]> = TableDefinition::new("snapshots");
/// room → `RoomMeta` (postcard).
const META: TableDefinition<'_, &str, &[u8]> = TableDefinition::new("meta");

/// A storage failure.
pub type StoreError = redb::Error;

/// What is stored for one room.
#[derive(Debug, Default)]
pub struct Stored {
    /// The latest snapshot.
    pub snapshot: Option<Vec<u8>>,
    /// The updates after it, with their sequence numbers, in order.
    pub updates: Vec<(u64, Vec<u8>)>,
    /// The metadata, if any was stored.
    pub meta: Option<Vec<u8>>,
}

/// Where a relay keeps its rooms ([`Store`] in redb; tests put failures in between).
pub trait Storage: Send + Sync + 'static {
    /// Everything stored for a room (nothing for a new one).
    ///
    /// # Errors
    ///
    /// A read failure.
    fn load(&self, room: &str) -> Result<Stored, StoreError>;

    /// Appends an update.
    ///
    /// # Errors
    ///
    /// A write failure (nothing is stored then).
    fn append(&self, room: &str, seq: u64, update: &[u8]) -> Result<(), StoreError>;

    /// Removes an update appended by mistake.
    ///
    /// # Errors
    ///
    /// A write failure.
    fn remove(&self, room: &str, seq: u64) -> Result<(), StoreError>;

    /// Stores a room's metadata.
    ///
    /// # Errors
    ///
    /// A write failure.
    fn put_meta(&self, room: &str, meta: &[u8]) -> Result<(), StoreError>;

    /// Replaces a room's snapshot with `snapshot`, which holds every update up to `upto`, and
    /// drops those updates, in one transaction.
    ///
    /// # Errors
    ///
    /// A write failure (nothing changes then).
    fn compact(&self, room: &str, snapshot: &[u8], upto: u64) -> Result<(), StoreError>;
}

/// The rooms' database.
#[derive(Debug)]
pub struct Store {
    db: Database,
}

impl Store {
    /// Opens (or creates) `rooms.redb` in `dir`.
    ///
    /// # Errors
    ///
    /// The directory or the database cannot be created or opened.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir).map_err(redb::Error::Io)?;
        let db = Database::create(dir.join("rooms.redb"))?;
        let w = db.begin_write()?;
        {
            w.open_table(UPDATES)?;
            w.open_table(SNAPSHOTS)?;
            w.open_table(META)?;
        }
        w.commit()?;
        Ok(Self { db })
    }
}

impl Storage for Store {
    fn load(&self, room: &str) -> Result<Stored, StoreError> {
        let r = self.db.begin_read()?;
        let snapshot = r
            .open_table(SNAPSHOTS)?
            .get(room)?
            .map(|v| v.value().to_vec());
        let meta = r.open_table(META)?.get(room)?.map(|v| v.value().to_vec());
        let mut updates = Vec::new();
        for entry in r.open_table(UPDATES)?.range((room, 0)..=(room, u64::MAX))? {
            let (k, v) = entry?;
            updates.push((k.value().1, v.value().to_vec()));
        }
        Ok(Stored {
            snapshot,
            updates,
            meta,
        })
    }

    fn append(&self, room: &str, seq: u64, update: &[u8]) -> Result<(), StoreError> {
        let w = self.db.begin_write()?;
        w.open_table(UPDATES)?.insert((room, seq), update)?;
        w.commit()?;
        Ok(())
    }

    fn remove(&self, room: &str, seq: u64) -> Result<(), StoreError> {
        let w = self.db.begin_write()?;
        w.open_table(UPDATES)?.remove((room, seq))?;
        w.commit()?;
        Ok(())
    }

    fn put_meta(&self, room: &str, meta: &[u8]) -> Result<(), StoreError> {
        let w = self.db.begin_write()?;
        w.open_table(META)?.insert(room, meta)?;
        w.commit()?;
        Ok(())
    }

    fn compact(&self, room: &str, snapshot: &[u8], upto: u64) -> Result<(), StoreError> {
        let w = self.db.begin_write()?;
        w.open_table(SNAPSHOTS)?.insert(room, snapshot)?;
        {
            let mut t = w.open_table(UPDATES)?;
            let keys: Vec<u64> = t
                .range((room, 0)..=(room, upto))?
                .map(|e| e.map(|(k, _)| k.value().1))
                .collect::<Result<_, _>>()?;
            for seq in keys {
                t.remove((room, seq))?;
            }
        }
        w.commit()?;
        Ok(())
    }
}
