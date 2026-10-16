// SPDX-License-Identifier: AGPL-3.0-or-later
//! The host: everything outside the core (files, dialogs, clipboard, preferences, time).
//!
//! The operations are the [`HostOp`] variants (read/write file, pick open/save, recents, store and
//! load preferences, clipboard read/write, open external). A host answers each one either at once
//! ([`Host::request`] returns `Some`) or later: the session then emits `Event::HostRequest` and
//! continues when the shell calls `Session::host_reply`. The desktop answers file reads and writes
//! itself and defers dialogs to the UI; the web host defers everything to the main thread.

use kiln_protocol::{HostOp, HostReply};

/// The outside world, as the session sees it.
pub trait Host: Send {
    /// Performs `op` now and returns the reply, or returns `None` to have the session forward it
    /// to the UI as `Event::HostRequest`.
    fn request(&mut self, op: &HostOp) -> Option<HostReply>;

    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
}

/// A host that defers every operation to the UI and reads the clock through a function (the web
/// build passes `Date.now`, which is all `wasm32-unknown-unknown` has).
#[derive(Debug, Clone, Copy)]
pub struct DeferredHost {
    clock: fn() -> u64,
}

impl DeferredHost {
    /// A deferring host with the given clock.
    #[must_use]
    pub const fn new(clock: fn() -> u64) -> Self {
        Self { clock }
    }
}

impl Host for DeferredHost {
    fn request(&mut self, _op: &HostOp) -> Option<HostReply> {
        None
    }
    fn now_ms(&self) -> u64 {
        (self.clock)()
    }
}

/// The native host used by the desktop shell: files through `std::fs` (writes are atomic: a
/// temporary file in the same directory, flushed, then renamed over the target); dialogs,
/// clipboard, preferences and URLs go to the UI, which owns those plugins.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeHost;

#[cfg(not(target_arch = "wasm32"))]
impl NativeHost {
    fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let name = path
            .file_name()
            .map_or_else(|| "kiln".into(), |n| n.to_string_lossy().into_owned());
        let tmp = dir.join(format!(".{name}.kiln-{}.tmp", std::process::id()));
        let result = (|| {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            // Best effort: never leave the temporary file behind.
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Host for NativeHost {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        let failed = |e: std::io::Error| HostReply::Failed(kiln_common::KilnError::from(e).into());
        match op {
            HostOp::ReadFile { path } => {
                Some(std::fs::read(path).map_or_else(failed, HostReply::Bytes))
            }
            HostOp::WriteFile { path, bytes } => Some(
                Self::write_atomic(std::path::Path::new(path), bytes)
                    .map_or_else(failed, |()| HostReply::Done),
            ),
            _ => None,
        }
    }

    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use kiln_common::ErrorKind;

    #[test]
    fn native_host_reads_and_writes_files_atomically() {
        let dir = std::env::temp_dir().join(format!("kiln-host-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.txt").to_string_lossy().into_owned();
        let mut host = NativeHost;
        let w = host.request(&HostOp::WriteFile {
            path: path.clone(),
            bytes: b"hello".to_vec(),
        });
        assert_eq!(w, Some(HostReply::Done));
        let r = host.request(&HostOp::ReadFile { path: path.clone() });
        assert_eq!(r, Some(HostReply::Bytes(b"hello".to_vec())));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file is left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn native_host_reports_missing_files_and_defers_dialogs() {
        let mut host = NativeHost;
        match host.request(&HostOp::ReadFile {
            path: "/definitely/not/here.txt".into(),
        }) {
            Some(HostReply::Failed(e)) => assert_eq!(e.kind, ErrorKind::NotFound),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(host.request(&HostOp::ClipboardRead), None);
        assert!(host.now_ms() > 1_700_000_000_000);
        let mut deferred = DeferredHost::new(|| 42);
        assert_eq!(deferred.request(&HostOp::Recents), None);
        assert_eq!(deferred.now_ms(), 42);
    }
}
