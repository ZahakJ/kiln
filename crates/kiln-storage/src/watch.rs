// SPDX-License-Identifier: AGPL-3.0-or-later
//! Files changed by other programs (the prompt "This file was changed by another program").
//!
//! A [`FileWatcher`] watches the folders of the files it is given (watching the folder, not the
//! file, sees a file that an editor replaces by renaming a new one over it) and calls back once
//! per change of a watched file whose size or modification time differs from the last one
//! [`FileWatcher::note_written`] recorded, so Kiln's own saves never prompt. A change another
//! program made just before Kiln writes is caught when the write begins
//! ([`FileWatcher::begin_write`] compares the stamp first), so it is reported even though
//! Kiln's write follows it at once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use kiln_common::{KilnError, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// What the watcher compares: size and modification time.
pub type Stamp = Option<(u64, SystemTime)>;

/// The stamp of a file now (`None` when it is gone).
#[must_use]
pub fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.modified().ok()?))
}

type Callback = Box<dyn FnMut() + Send>;

#[derive(Default)]
struct Watched {
    /// The last stamp known to be Kiln's (opened, or written by Kiln).
    known: Stamp,
    /// Kiln is writing the file: its events are Kiln's own.
    writing: bool,
    callback: Option<Callback>,
}

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, Watched>,
}

impl State {
    /// Kiln is about to write `path`: a stamp that moved since the last known one is another
    /// program's change, reported now (Kiln's write would hide it).
    fn begin_write(&mut self, path: &Path) {
        let Some(w) = self.files.get_mut(path) else {
            return;
        };
        let now = stamp(path);
        if !w.writing && now.is_some() && now != w.known {
            w.known = now;
            if let Some(cb) = w.callback.as_mut() {
                cb();
            }
        }
        w.writing = true;
    }
}

/// Watches files for changes by other programs.
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    state: Arc<Mutex<State>>,
    folders: BTreeMap<PathBuf, usize>,
}

impl std::fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWatcher")
            .field("folders", &self.folders.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FileWatcher {
    /// A watcher with its own thread (the platform's notification API).
    ///
    /// # Errors
    ///
    /// `Io` when the platform refuses (too many watches).
    pub fn new() -> Result<Self> {
        let state: Arc<Mutex<State>> = Arc::default();
        let events = Arc::clone(&state);
        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };
            if !matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                return;
            }
            let mut state = lock(&events);
            for path in &event.paths {
                let Some(w) = state.files.get_mut(path) else {
                    continue;
                };
                if w.writing {
                    continue;
                }
                let now = stamp(path);
                // A file being replaced is briefly gone; its new version raises another event.
                if now.is_none() || now == w.known {
                    continue;
                }
                w.known = now;
                if let Some(cb) = w.callback.as_mut() {
                    cb();
                }
            }
        })
        .map_err(|e| KilnError::Io(format!("file watching: {e}")))?;
        Ok(Self {
            watcher,
            state,
            folders: BTreeMap::new(),
        })
    }

    /// Watches `path`; `on_change` runs (on the watcher's thread) after each change by another
    /// program. Watching a file again replaces its callback.
    ///
    /// # Errors
    ///
    /// `Io` when its folder cannot be watched.
    pub fn watch(&mut self, path: &Path, on_change: impl FnMut() + Send + 'static) -> Result<()> {
        let path = absolute(path);
        let folder = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let fresh = !lock(&self.state).files.contains_key(&path);
        if fresh {
            if !self.folders.contains_key(&folder) {
                self.watcher
                    .watch(&folder, RecursiveMode::NonRecursive)
                    .map_err(|e| KilnError::Io(format!("watching {}: {e}", folder.display())))?;
            }
            *self.folders.entry(folder).or_insert(0) += 1;
        }
        let mut state = lock(&self.state);
        let w = state.files.entry(path.clone()).or_default();
        if fresh {
            w.known = stamp(&path);
        }
        w.callback = Some(Box::new(on_change));
        Ok(())
    }

    /// Kiln is about to write `path`: changes until [`FileWatcher::note_written`] are its own.
    /// A change by another program since the last known stamp is reported first.
    pub fn begin_write(&self, path: &Path) {
        lock(&self.state).begin_write(&absolute(path));
    }

    /// Records that Kiln itself wrote `path` (or gave up): its stamp now is Kiln's.
    pub fn note_written(&self, path: &Path) {
        let path = absolute(path);
        if let Some(w) = lock(&self.state).files.get_mut(&path) {
            w.known = stamp(&path);
            w.writing = false;
        }
    }

    /// Stops watching `path`.
    pub fn unwatch(&mut self, path: &Path) {
        let path = absolute(path);
        if lock(&self.state).files.remove(&path).is_none() {
            return;
        }
        let folder = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        if let Some(n) = self.folders.get_mut(&folder) {
            *n -= 1;
            if *n == 0 {
                self.folders.remove(&folder);
                let _ = self.watcher.unwatch(&folder);
            }
        }
    }
}

/// The path made absolute (events name absolute paths), without resolving links.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn a_change_by_another_program_is_reported_and_kilns_own_write_is_not() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("kiln-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("Plan.docx");
        std::fs::write(&file, b"one")?;
        let mut w = FileWatcher::new()?;
        let (tx, rx) = channel();
        w.watch(&file, move || {
            let _ = tx.send(());
        })?;
        // Kiln saves: its write does not count.
        w.begin_write(&file);
        std::fs::write(&file, b"kiln's own save")?;
        w.note_written(&file);
        // Events of that write may still arrive; they match the noted stamp.
        std::thread::sleep(Duration::from_millis(300));
        assert!(rx.try_recv().is_err(), "Kiln's own save is not reported");
        // Another program changes it (a different size).
        std::fs::write(&file, b"changed elsewhere, longer")?;
        let got = rx.recv_timeout(Duration::from_secs(5));
        w.unwatch(&file);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(got.is_ok(), "the change was reported");
        Ok(())
    }

    #[test]
    fn a_change_just_before_kilns_own_write_is_still_reported() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("kiln-watch-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("Plan.docx");
        std::fs::write(&file, b"one")?;
        let calls = Arc::new(Mutex::new(0));
        let mut state = State::default();
        let counted = Arc::clone(&calls);
        state.files.insert(
            file.clone(),
            Watched {
                known: stamp(&file),
                writing: false,
                callback: Some(Box::new(move || {
                    *counted.lock().unwrap_or_else(PoisonError::into_inner) += 1;
                })),
            },
        );
        // Another program writes; Kiln's write begins before the watcher's event is handled.
        std::fs::write(&file, b"another program's version")?;
        state.begin_write(&file);
        assert_eq!(*calls.lock().unwrap_or_else(PoisonError::into_inner), 1);
        // The event of that change arriving now changes nothing: Kiln is writing.
        state.begin_write(&file);
        assert_eq!(*calls.lock().unwrap_or_else(PoisonError::into_inner), 1);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
