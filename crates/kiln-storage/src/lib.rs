// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-storage
//!
//! Everything Kiln keeps beside the documents themselves:
//!
//! - [`bundle`]: the native formats `.kwrite`, `.kgrid` and `.kdeck`, zip archives with a
//!   versioned `manifest.json`, the model (`model.bin`, postcard), the CRDT of a shared
//!   document (`crdt.bin`), the version history (`versions/`), media (`media/`) and the parts
//!   of the original file the model does not cover (`passthrough/`). A reader refuses a bundle
//!   of a newer version with a message that says so.
//! - [`journal`]: the autosave journal of an open document, a checkpoint (the document as a
//!   bundle) plus an append-only log of the operations applied since, in checksummed records;
//!   every [`journal::CHECKPOINT_OPS`] operations a new checkpoint is written beside the old one
//!   and the marker, written last, switches to it. The marker also says whether the journal
//!   holds unsaved work; a clean close removes it. [`journal::recover`] reads a journal back
//!   after a crash, dropping a record the crash cut short.
//! - [`versions`]: local version history (on every save and every ten minutes of editing, at
//!   most [`versions::LocalHistory::MAX`], named versions never thinned).
//! - [`recents`]: recent and pinned files; [`templates`]: the templates index;
//!   [`dictionary`]: the user dictionary.
//! - [`store`]: the [`StorageHost`] trait the core's app storage goes through
//!   (`HostOp::Store*`), with an in-memory store for tests and, natively, [`NativeStorage`]
//!   in files every Kiln process shares (see its fsync policy and journal locks) and [`watch::FileWatcher`] for files changed by other
//!   programs. The web answers the same operations from the Origin Private File System.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bundle;
pub mod dictionary;
pub mod journal;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod recents;
pub mod store;
pub mod templates;
pub mod versions;
#[cfg(not(target_arch = "wasm32"))]
pub mod watch;

pub use bundle::{BUNDLE_VERSION, Bundle};
#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeStorage;
pub use store::{MemoryStorage, StorageHost};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A stable, short key for a path or name: 16 hex digits of its 64-bit FNV-1a hash (keys of
/// recents and of the version history of files that are not bundles).
#[must_use]
pub fn key_of(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn keys_are_stable_and_short() {
        assert_eq!(super::key_of(""), "cbf29ce484222325");
        assert_eq!(super::key_of("/a/b.docx"), super::key_of("/a/b.docx"));
        assert_ne!(super::key_of("/a/b.docx"), super::key_of("/a/c.docx"));
        assert_eq!(super::key_of("x").len(), 16);
    }
}
