// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scene types on the wire.
//!
//! **Placeholder until WP-54.** WP-54 implements `kiln-scene` (tiles, draw operations, atlas
//! pages, overlays) and replaces this module with re-exports of its `SceneDelta` and friends. Until
//! then a frame carries only the document revision it was produced for, so the binary path
//! (postcard bytes → `Uint8Array` → the TypeScript decoder) already runs end to end on both
//! bridges.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The changes since the previous frame of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SceneDelta {
    /// The document revision this frame shows.
    #[ts(type = "number")]
    pub revision: u64,
}
