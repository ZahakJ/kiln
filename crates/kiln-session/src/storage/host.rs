// SPDX-License-Identifier: AGPL-3.0-or-later
//! A native host with app storage: [`StoredHost`] wraps a host (the desktop's) and answers the
//! app storage requests from a [`NativeStorage`], the preferences from its `Prefs` area, the
//! file watches with a [`FileWatcher`] and the stamps of files itself; everything else goes to
//! the wrapped host. Once the session thread attaches (`Host::attach`), a write to the app
//! storage is answered when it is on the disk, so a failure is reported for the write that
//! failed. Every Kiln process of the user opens the same storage; a journal one of them has
//! open is not listed, opened or deleted by another (`NativeStorage`'s journal locks).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use kiln_common::Result;
use kiln_protocol::{FileStamp, HostOp, HostReply, RequestId, StoreArea};
use kiln_storage::NativeStorage;
use kiln_storage::store::{StorageHost as _, answer};
use kiln_storage::watch::FileWatcher;

use crate::host::{Deliver, Host};

/// A host with app storage in a directory (see the module documentation).
pub struct StoredHost {
    inner: Box<dyn Host>,
    storage: NativeStorage,
    watcher: Option<FileWatcher>,
    deliver: Option<Deliver>,
    /// The request waiting for each watched file's next change.
    waiting: Arc<Mutex<BTreeMap<String, RequestId>>>,
}

impl std::fmt::Debug for StoredHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredHost")
            .field("storage", &self.storage)
            .finish_non_exhaustive()
    }
}

impl StoredHost {
    /// Wraps `inner` with the app storage in `dir` (the app's data directory).
    ///
    /// # Errors
    ///
    /// `Io` when the storage directory cannot be created.
    pub fn open(inner: Box<dyn Host>, dir: &Path) -> Result<Self> {
        Ok(Self {
            inner,
            storage: NativeStorage::open(dir)?,
            watcher: None,
            deliver: None,
            waiting: Arc::default(),
        })
    }

    fn watch(&mut self, id: RequestId, path: &str) -> bool {
        let Some(deliver) = self.deliver.clone() else {
            return false;
        };
        if self.watcher.is_none() {
            match FileWatcher::new() {
                Ok(w) => self.watcher = Some(w),
                Err(_) => return false,
            }
        }
        let Some(w) = self.watcher.as_mut() else {
            return false;
        };
        lock(&self.waiting).insert(path.to_owned(), id);
        let waiting = Arc::clone(&self.waiting);
        let key = path.to_owned();
        w.watch(Path::new(path), move || {
            // One answer per request: the core asks again after each change.
            if let Some(id) = lock(&waiting).remove(&key) {
                deliver(id, HostReply::Done);
            }
        })
        .is_ok()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The stamp of a file (`Empty` when there is none).
pub(crate) fn stat(path: &str) -> HostReply {
    match std::fs::metadata(path) {
        Ok(m) => HostReply::Stamp(FileStamp {
            size: m.len(),
            modified_ms: m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HostReply::Empty,
        Err(e) => HostReply::Failed((&kiln_common::KilnError::from(e)).into()),
    }
}

/// Whether an operation writes the app storage.
fn is_store_write(op: &HostOp) -> bool {
    matches!(
        op,
        HostOp::StorePut { .. } | HostOp::StoreAppend { .. } | HostOp::StoreDelete { .. }
    )
}

impl Host for StoredHost {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        // Writes are answered once they are on the disk (`request_later`).
        if self.deliver.is_some() && is_store_write(op) {
            return None;
        }
        if let Some(reply) = answer(&mut self.storage, op) {
            return Some(reply);
        }
        match op {
            HostOp::StorePref { key, value } => Some(
                self.storage
                    .put(StoreArea::Prefs, key, value.as_bytes())
                    .map_or_else(|e| HostReply::Failed((&e).into()), |()| HostReply::Done),
            ),
            HostOp::LoadPref { key } => Some(match self.storage.get(StoreArea::Prefs, key) {
                Ok(Some(v)) => HostReply::Text(String::from_utf8_lossy(&v).into_owned()),
                Ok(None) => HostReply::Empty,
                Err(e) => HostReply::Failed((&e).into()),
            }),
            HostOp::WatchFile { .. } => None,
            HostOp::StatFile { path } => Some(stat(path)),
            HostOp::UnwatchFile { path } => {
                lock(&self.waiting).remove(path);
                if let Some(w) = self.watcher.as_mut() {
                    w.unwatch(Path::new(path));
                }
                Some(HostReply::Done)
            }
            HostOp::WriteFile { path, .. } => {
                // Kiln's own save is not a change by another program.
                if let Some(w) = &self.watcher {
                    w.begin_write(Path::new(path));
                }
                let reply = self.inner.request(op);
                if let Some(w) = &self.watcher {
                    w.note_written(Path::new(path));
                }
                reply
            }
            other => self.inner.request(other),
        }
    }

    fn request_later(&mut self, id: RequestId, op: &HostOp) -> bool {
        match op {
            HostOp::WatchFile { path } => self.watch(id, path),
            HostOp::StorePut { .. } | HostOp::StoreAppend { .. } | HostOp::StoreDelete { .. } => {
                let Some(deliver) = self.deliver.clone() else {
                    return false;
                };
                let reply = Arc::clone(&deliver);
                let done: kiln_storage::native::Done = Box::new(move |r: Result<()>| {
                    reply(
                        id,
                        r.map_or_else(|e| HostReply::Failed((&e).into()), |()| HostReply::Done),
                    );
                });
                let queued = match op {
                    HostOp::StorePut { area, key, bytes } => {
                        self.storage.put_then(*area, key, bytes, done)
                    }
                    HostOp::StoreAppend { area, key, bytes } => {
                        self.storage.append_then(*area, key, bytes, done)
                    }
                    HostOp::StoreDelete { area, key } => self.storage.delete_then(*area, key, done),
                    _ => Ok(()),
                };
                if let Err(e) = queued {
                    deliver(id, HostReply::Failed((&e).into()));
                }
                true
            }
            other => self.inner.request_later(id, other),
        }
    }

    fn attach(&mut self, deliver: Deliver) {
        self.deliver = Some(Arc::clone(&deliver));
        self.inner.attach(deliver);
    }

    fn has_storage(&self) -> bool {
        true
    }

    fn now_ms(&self) -> u64 {
        self.inner.now_ms()
    }
}
