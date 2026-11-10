// SPDX-License-Identifier: AGPL-3.0-or-later
//! The native app storage: one file per value in the app's data directory, shared by every
//! Kiln process of the user.
//!
//! Values live in `<dir>/store/<area>/<key>` (the key with every byte other than letters,
//! digits, `-` and `_` written `%XX`); the journals a process has open are held with an OS file
//! lock on `<dir>/locks/<journal>.lock`, so two processes never write or recover the same
//! journal (see [`NativeStorage`]).
//!
//! **Fsync policy.** Writes go to a writer thread, in the order given, so the session never
//! waits for the disk. A replaced value is written to a temporary file, flushed (`fsync`) and
//! renamed over the old one, and the folder is flushed: a crash leaves the old value or the new
//! one, never a mix. An append is written at the end of the value and flushed; appends queued
//! together for one value are written and flushed once. Every write is on the disk before the
//! next one starts, so a journal's marker never reaches the disk ahead of the checkpoint it
//! names. A write's result is reported when it is on the disk ([`NativeStorage::put_then`]);
//! reads wait until the writes queued before them are done.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

use kiln_common::{KilnError, Result};
use kiln_protocol::StoreArea;

use crate::journal::journal_of;
use crate::store::{StorageHost, check_key};

/// What a finished write is reported to.
pub type Done = Box<dyn FnOnce(Result<()>) + Send>;

enum Job {
    Put(PathBuf, Vec<u8>, Option<Done>),
    Append(PathBuf, Vec<u8>, Option<Done>),
    Delete(PathBuf, Option<Done>),
    /// Answered once everything queued before it is done.
    Flush(Sender<()>),
}

/// The app storage in a directory (see the module documentation for the fsync policy).
///
/// The `Journal` area is shared with other processes journal by journal: writing a journal's
/// values, or reading one of its checkpoints (opening it from Document Recovery), takes its
/// lock for as long as this storage lives or until its marker is deleted; another process's
/// attempt then fails with `Unavailable` ("open in another window"), and listing the area
/// leaves out the journals another process holds.
pub struct NativeStorage {
    root: PathBuf,
    locks_dir: PathBuf,
    jobs: Mutex<Sender<Job>>,
    failure: Arc<Mutex<Option<String>>>,
    writer: Option<std::thread::JoinHandle<()>>,
    /// The journals this storage holds, with their lock files.
    held: Mutex<BTreeMap<String, File>>,
}

impl std::fmt::Debug for NativeStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeStorage")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

fn io(e: impl std::fmt::Display) -> KilnError {
    KilnError::Io(format!("app storage: {e}"))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The error for a journal another process holds.
fn elsewhere() -> KilnError {
    KilnError::unavailable("app storage", "the document is open in another Kiln window")
}

/// A key as a file name: letters, digits, `-` and `_` as they are, every other byte `%XX`.
fn encode(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = name.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Longest file name the storage writes (file systems allow 255 bytes).
const MAX_NAME: usize = 240;

/// Numbers the temporary files of replaced values.
static TMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl NativeStorage {
    /// Opens (or creates) the storage in `dir`. Several processes may open the same directory.
    ///
    /// # Errors
    ///
    /// `Io` when the directory cannot be created.
    pub fn open(dir: &Path) -> Result<Self> {
        let root = dir.join("store");
        let locks_dir = dir.join("locks");
        std::fs::create_dir_all(&root).map_err(io)?;
        std::fs::create_dir_all(&locks_dir).map_err(io)?;
        let (jobs, rx) = channel();
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let writer = {
            let failure = Arc::clone(&failure);
            std::thread::Builder::new()
                .name("kiln-storage".into())
                .spawn(move || write_loop(&rx, &failure))
                .map_err(io)?
        };
        Ok(Self {
            root,
            locks_dir,
            jobs: Mutex::new(jobs),
            failure,
            writer: Some(writer),
            held: Mutex::default(),
        })
    }

    fn path(&self, area: StoreArea, key: &str) -> Result<PathBuf> {
        check_key(key)?;
        let name = encode(key);
        if name.len() > MAX_NAME {
            return Err(KilnError::InvalidInput(format!(
                "{key:?} is too long for a storage key"
            )));
        }
        Ok(self.root.join(format!("{area:?}")).join(name))
    }

    fn send(&self, job: Job) -> Result<()> {
        self.check()?;
        lock(&self.jobs)
            .send(job)
            .map_err(|_| io("the writer stopped"))
    }

    /// The failure of an earlier write that had no one to report to, once.
    fn check(&self) -> Result<()> {
        match lock(&self.failure).take() {
            Some(e) => Err(io(e)),
            None => Ok(()),
        }
    }

    /// Waits until every write queued so far is on the disk.
    ///
    /// # Errors
    ///
    /// `Io` when a write failed or the writer stopped.
    pub fn flush(&self) -> Result<()> {
        let (tx, rx) = channel();
        self.send(Job::Flush(tx))?;
        rx.recv().map_err(|_| io("the writer stopped"))?;
        self.check()
    }

    /// Takes the lock of `journal` (at once if this storage has it already).
    ///
    /// # Errors
    ///
    /// `Unavailable` when another process holds it, `Io` when the lock file cannot be made.
    fn claim(&self, journal: &str) -> Result<()> {
        let mut held = lock(&self.held);
        if held.contains_key(journal) {
            return Ok(());
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.lock_file(journal))
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => {
                held.insert(journal.to_owned(), file);
                Ok(())
            }
            Err(std::fs::TryLockError::WouldBlock) => Err(elsewhere()),
            Err(std::fs::TryLockError::Error(e)) => Err(io(e)),
        }
    }

    fn lock_file(&self, journal: &str) -> PathBuf {
        self.locks_dir.join(format!("{}.lock", encode(journal)))
    }

    /// Whether another process holds `journal`.
    fn held_elsewhere(&self, journal: &str) -> bool {
        if lock(&self.held).contains_key(journal) {
            return false;
        }
        let Ok(file) = File::open(self.lock_file(journal)) else {
            return false;
        };
        // Probing takes the lock for a moment; it goes with the file.
        matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
    }

    /// Lets a journal go (its marker was deleted): the lock file goes with it.
    fn release(&self, journal: &str) {
        if let Some(file) = lock(&self.held).remove(journal) {
            let _ = std::fs::remove_file(self.lock_file(journal));
            drop(file);
        }
    }

    /// The lock a write to `key` of `area` needs.
    fn claim_for(&self, area: StoreArea, key: &str) -> Result<()> {
        if area == StoreArea::Journal {
            self.claim(journal_of(key))
        } else {
            Ok(())
        }
    }

    /// Replaces a value and reports to `done` once it is on the disk.
    ///
    /// # Errors
    ///
    /// As [`StorageHost::put`] (a failure of the write itself goes to `done`).
    pub fn put_then(&self, area: StoreArea, key: &str, bytes: &[u8], done: Done) -> Result<()> {
        self.claim_for(area, key)?;
        let path = self.path(area, key)?;
        self.send(Job::Put(path, bytes.to_vec(), Some(done)))
    }

    /// Appends to a value and reports to `done` once it is on the disk.
    ///
    /// # Errors
    ///
    /// As [`StorageHost::append`].
    pub fn append_then(&self, area: StoreArea, key: &str, bytes: &[u8], done: Done) -> Result<()> {
        self.claim_for(area, key)?;
        let path = self.path(area, key)?;
        self.send(Job::Append(path, bytes.to_vec(), Some(done)))
    }

    /// Removes a value and reports to `done` once that is on the disk.
    ///
    /// # Errors
    ///
    /// As [`StorageHost::delete`].
    pub fn delete_then(&self, area: StoreArea, key: &str, done: Done) -> Result<()> {
        self.delete_job(area, key, Some(done))
    }

    fn delete_job(&self, area: StoreArea, key: &str, done: Option<Done>) -> Result<()> {
        let marker = area == StoreArea::Journal && key.ends_with(".meta");
        if marker {
            // Removing a journal (or Document Recovery's Delete) needs it to be this process's.
            self.claim(journal_of(key))?;
        }
        let path = self.path(area, key)?;
        self.send(Job::Delete(path, done))?;
        if marker {
            self.flush()?;
            self.release(journal_of(key));
        }
        Ok(())
    }
}

impl Drop for NativeStorage {
    fn drop(&mut self) {
        // Closing the channel ends the writer once it has written what is queued.
        let (dead, _) = channel();
        drop(std::mem::replace(&mut *lock(&self.jobs), dead));
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
    }
}

fn write_loop(rx: &Receiver<Job>, failure: &Mutex<Option<String>>) {
    let mut next = None;
    loop {
        let Some(job) = next.take().or_else(|| rx.recv().ok()) else {
            return;
        };
        match job {
            Job::Flush(done) => {
                let _ = done.send(());
            }
            Job::Put(path, bytes, done) => report(failure, done, put_file(&path, &bytes)),
            Job::Delete(path, done) => report(failure, done, delete_file(&path)),
            Job::Append(path, mut bytes, done) => {
                // The appends to this value queued behind it go in the same flush.
                let mut dones = vec![done];
                while let Ok(job) = rx.try_recv() {
                    match job {
                        Job::Append(p, more, d) if p == path => {
                            bytes.extend_from_slice(&more);
                            dones.push(d);
                        }
                        other => {
                            next = Some(other);
                            break;
                        }
                    }
                }
                let result = append_file(&path, &bytes);
                for done in dones {
                    report(failure, done, result.clone());
                }
            }
        }
    }
}

fn report(failure: &Mutex<Option<String>>, done: Option<Done>, result: Result<()>) {
    match (done, result) {
        (Some(done), result) => done(result),
        (None, Err(e)) => *lock(failure) = Some(e.to_string()),
        (None, Ok(())) => {}
    }
}

fn put_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io("a value outside the storage"))?;
    let fresh = !dir.exists();
    std::fs::create_dir_all(dir).map_err(io)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let n = TMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let written = (|| {
        let mut f = File::create(&tmp)?;
        trace("write", path);
        f.write_all(bytes)?;
        f.sync_all()?;
        trace("sync", path);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    sync_dir(dir)?;
    if fresh && let Some(up) = dir.parent() {
        sync_dir(up)?;
    }
    Ok(())
}

fn append_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io("a value outside the storage"))?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let fresh = !path.exists();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io)?;
    trace("write", path);
    f.write_all(bytes).map_err(io)?;
    f.sync_data().map_err(io)?;
    trace("sync", path);
    if fresh {
        sync_dir(dir)?;
    }
    Ok(())
}

fn delete_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => path.parent().map_or(Ok(()), sync_dir),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}

/// Test builds: every write and flush, in order, for the test that the policy holds.
#[cfg(test)]
static TRACE: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());

#[cfg(test)]
fn trace(what: &str, path: &Path) {
    lock(&TRACE).push((what.to_owned(), path.to_owned()));
}

#[cfg(not(test))]
#[inline]
fn trace(_: &str, _: &Path) {}

/// Flushes a folder's entries (a rename or a new file in it). Windows has no such call: its
/// renames are flushed with the file.
fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(dir).and_then(|d| d.sync_all()).map_err(io)?;
        trace("sync folder", dir);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

impl StorageHost for NativeStorage {
    fn get(&self, area: StoreArea, key: &str) -> Result<Option<Vec<u8>>> {
        if area == StoreArea::Journal && key.contains(".base") {
            // Reading a checkpoint is opening the journal: it becomes this process's.
            self.claim(journal_of(key))?;
        }
        self.flush()?;
        let path = self.path(area, key)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn put(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()> {
        self.claim_for(area, key)?;
        let path = self.path(area, key)?;
        self.send(Job::Put(path, bytes.to_vec(), None))
    }

    fn append(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()> {
        self.claim_for(area, key)?;
        let path = self.path(area, key)?;
        self.send(Job::Append(path, bytes.to_vec(), None))
    }

    fn delete(&mut self, area: StoreArea, key: &str) -> Result<()> {
        self.delete_job(area, key, None)
    }

    fn list(&self, area: StoreArea, prefix: &str) -> Result<Vec<String>> {
        self.flush()?;
        let dir = self.root.join(format!("{area:?}"));
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(e)),
        };
        let mut out = Vec::new();
        for entry in entries {
            let name = entry.map_err(io)?.file_name();
            let Some(key) = name
                .to_str()
                .filter(|n| !n.starts_with('.'))
                .and_then(decode)
            else {
                continue;
            };
            if key.starts_with(prefix) {
                out.push(key);
            }
        }
        if area == StoreArea::Journal {
            // The journals another process has open are not this process's to list.
            let mut open = BTreeMap::new();
            out.retain(|k| {
                let j = journal_of(k).to_owned();
                !*open
                    .entry(j.clone())
                    .or_insert_with(|| self.held_elsewhere(&j))
            });
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kiln-storage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// The fsync policy as it happens: each write is flushed to the disk before the next one
    /// starts, a replaced value's folder is flushed after its rename, and a write is reported
    /// only once it is flushed.
    #[test]
    fn every_write_is_on_the_disk_before_the_next_starts() -> Result<()> {
        let d = dir("fsync");
        let s = NativeStorage::open(&d)?;
        let reported = Arc::new(Mutex::new(Vec::new()));
        let done = |what: &'static str| -> Done {
            let reported = Arc::clone(&reported);
            Box::new(move |r| {
                let synced = lock(&TRACE).iter().filter(|(w, _)| w == "sync").count();
                lock(&reported).push((what, r.is_ok(), synced));
            })
        };
        s.put_then(StoreArea::Journal, "k.base.1", b"base", done("base"))?;
        s.put_then(StoreArea::Journal, "k.log.1", b"", done("log"))?;
        for i in 0..5u8 {
            s.append_then(StoreArea::Journal, "k.log.1", &[i], done("append"))?;
            s.flush()?;
        }
        s.put_then(StoreArea::Journal, "k.meta", b"{}", done("meta"))?;
        s.flush()?;
        let trace: Vec<(String, PathBuf)> = lock(&TRACE)
            .iter()
            .filter(|(_, p)| p.starts_with(&d))
            .cloned()
            .collect();
        println!("FSYNC {} steps", trace.len());
        for (w, p) in &trace {
            println!("FSYNC {w} {}", p.strip_prefix(&d).unwrap_or(p).display());
        }
        // Writes and flushes alternate, each flush the file just written.
        let files: Vec<&(String, PathBuf)> =
            trace.iter().filter(|(w, _)| w != "sync folder").collect();
        assert_eq!(files.len(), 2 * 8, "{trace:?}");
        for pair in files.chunks(2) {
            assert_eq!(pair[0].0, "write");
            assert_eq!(pair[1].0, "sync");
            assert_eq!(pair[0].1, pair[1].1);
        }
        // A replaced value's folder is flushed before the next write.
        for (i, (w, p)) in trace.iter().enumerate() {
            if w == "sync" && (p.ends_with("k%2Ebase%2E1") || p.ends_with("k%2Emeta")) {
                assert_eq!(trace.get(i + 1).map(|t| t.0.as_str()), Some("sync folder"));
            }
        }
        // Reported once flushed: in order, every write flushed by then.
        let reported = lock(&reported).clone();
        assert_eq!(reported.len(), 8);
        assert!(reported.iter().all(|(_, ok, _)| *ok));
        assert!(reported.windows(2).all(|w| w[0].2 < w[1].2), "{reported:?}");
        drop(s);
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn values_and_appends_survive_a_reopen() -> Result<()> {
        let d = dir("reopen");
        {
            let mut s = NativeStorage::open(&d)?;
            s.put(StoreArea::Journal, "k.base.1", b"base")?;
            s.put(StoreArea::Journal, "k.log.1", b"")?;
            for i in 0..100u8 {
                s.append(StoreArea::Journal, "k.log.1", &[i])?;
            }
            s.put(StoreArea::Recents, "r/1", b"{}")?;
            s.append(StoreArea::Versions, "only-appended", b"ab")?;
            // Dropped without a flush: the writer writes what was queued.
        }
        let mut s = NativeStorage::open(&d)?;
        assert_eq!(
            s.get(StoreArea::Journal, "k.base.1")?.as_deref(),
            Some(&b"base"[..])
        );
        let log = s.get(StoreArea::Journal, "k.log.1")?.unwrap_or_default();
        assert_eq!(log, (0..100u8).collect::<Vec<_>>());
        assert_eq!(s.list(StoreArea::Journal, "")?, ["k.base.1", "k.log.1"]);
        assert_eq!(s.list(StoreArea::Journal, "k.l")?, ["k.log.1"]);
        assert_eq!(s.list(StoreArea::Recents, "")?, ["r/1"]);
        assert_eq!(s.list(StoreArea::Versions, "")?, ["only-appended"]);
        assert_eq!(
            s.get(StoreArea::Prefs, "k.base.1")?,
            None,
            "areas are apart"
        );
        // A put replaces what was appended.
        s.put(StoreArea::Journal, "k.log.1", b"")?;
        assert_eq!(
            s.get(StoreArea::Journal, "k.log.1")?.as_deref(),
            Some(&b""[..])
        );
        s.delete(StoreArea::Journal, "k.log.1")?;
        s.delete(StoreArea::Journal, "k.base.1")?;
        assert!(s.list(StoreArea::Journal, "")?.is_empty());
        assert!(s.put(StoreArea::Journal, "", b"x").is_err());
        assert!(s.put(StoreArea::Recents, &"x".repeat(300), b"x").is_err());
        drop(s);
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn writes_report_once_they_are_on_the_disk_in_order() -> Result<()> {
        let d = dir("then");
        let s = NativeStorage::open(&d)?;
        let (tx, rx) = channel();
        for (i, key) in ["k.base.2", "k.log.2", "k.meta"].into_iter().enumerate() {
            let tx = tx.clone();
            s.put_then(
                StoreArea::Journal,
                key,
                &[u8::try_from(i).unwrap_or(0)],
                Box::new(move |r| {
                    let _ = tx.send((i, r.is_ok()));
                }),
            )?;
        }
        let got: Vec<(usize, bool)> = (0..3).filter_map(|_| rx.recv().ok()).collect();
        assert_eq!(got, [(0, true), (1, true), (2, true)]);
        drop(s);
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn two_processes_share_the_storage_but_never_one_journal() -> Result<()> {
        let d = dir("shared");
        let mut a = NativeStorage::open(&d)?;
        let mut b = NativeStorage::open(&d)?;
        // Both keep recent files, preferences and versions.
        a.put(StoreArea::Recents, "ra", b"a")?;
        b.put(StoreArea::Recents, "rb", b"b")?;
        b.flush()?;
        assert_eq!(a.list(StoreArea::Recents, "")?, ["ra", "rb"]);
        // A journal A writes is A's: B neither lists, opens, writes nor deletes it.
        a.put(StoreArea::Journal, "j1.base.1", b"cp")?;
        a.put(StoreArea::Journal, "j1.meta", b"{}")?;
        a.flush()?;
        assert!(b.list(StoreArea::Journal, "")?.is_empty());
        let e = b.get(StoreArea::Journal, "j1.base.1").unwrap_err();
        assert!(e.to_string().contains("open in another Kiln window"), "{e}");
        assert!(b.append(StoreArea::Journal, "j1.log.1", b"x").is_err());
        assert!(b.delete(StoreArea::Journal, "j1.meta").is_err());
        // A crash (A gone without removing it): B finds it and may open it.
        drop(a);
        assert_eq!(b.list(StoreArea::Journal, "")?, ["j1.base.1", "j1.meta"]);
        assert_eq!(
            b.get(StoreArea::Journal, "j1.base.1")?.as_deref(),
            Some(&b"cp"[..])
        );
        // Now B holds it, and a third process is refused.
        let c = NativeStorage::open(&d)?;
        assert!(c.get(StoreArea::Journal, "j1.base.1").is_err());
        // B removes it (the marker first): the lock goes with it.
        b.delete(StoreArea::Journal, "j1.meta")?;
        b.delete(StoreArea::Journal, "j1.base.1")?;
        b.flush()?;
        assert!(c.list(StoreArea::Journal, "")?.is_empty());
        assert!(!d.join("locks").join("j1.lock").exists());
        drop((b, c));
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn keys_are_file_names_and_back() {
        for key in ["k.meta", "a/b c", "handle:x/Plan.docx", "ü%"] {
            assert_eq!(decode(&encode(key)).as_deref(), Some(key));
        }
        assert!(!encode("../x").contains('/') && !encode("a.b").contains('.'));
    }
}
