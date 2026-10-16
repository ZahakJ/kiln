// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stable identifiers.

use std::fmt;

/// Identifies an open document inside one session. Allocated by the session, never reused while
/// the session lives, and meaningless across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DocId(pub u32);

impl DocId {
    /// The raw value, as sent over the bridge.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "doc#{}", self.0)
    }
}

/// Identifies a node (paragraph, run, cell block, shape, slide…) inside one document model. Node
/// ids are stable across edits, so layout, selection, comments and CRDT operations can refer to
/// them; a deleted node's id is never handed out again in the same document.
///
/// Ids are allocated sequentially from 1 by [`NodeIdGen`] and stay below 2^53, so they survive a
/// JSON round trip through JavaScript numbers exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodeId(#[cfg_attr(feature = "ts", ts(type = "number"))] pub u64);

impl NodeId {
    /// The largest id [`NodeIdGen`] hands out (2^53 − 1, `Number.MAX_SAFE_INTEGER`).
    pub const MAX: NodeId = NodeId((1 << 53) - 1);

    /// The raw value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node#{}", self.0)
    }
}

/// Allocates [`NodeId`]s for one document: sequential, starting at 1, never repeating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdGen {
    next: u64,
}

impl Default for NodeIdGen {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeIdGen {
    /// A generator whose first id is 1.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// A generator that continues after `last` (for models loaded from storage).
    #[must_use]
    pub fn after(last: NodeId) -> Self {
        Self {
            next: last.0.saturating_add(1).min(NodeId::MAX.0),
        }
    }

    /// The next id. After 2^53 − 1 allocations (not reachable in practice: one per microsecond
    /// takes 285 years) the generator saturates at [`NodeId::MAX`] instead of wrapping.
    pub fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next);
        if self.next < NodeId::MAX.0 {
            self.next += 1;
        }
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_ids_are_sequential_and_unique() {
        let mut ids = NodeIdGen::new();
        assert_eq!(ids.next_id(), NodeId(1));
        assert_eq!(ids.next_id(), NodeId(2));
        let mut resumed = NodeIdGen::after(NodeId(41));
        assert_eq!(resumed.next_id(), NodeId(42));
    }

    #[test]
    fn generator_saturates_instead_of_wrapping() {
        let mut ids = NodeIdGen::after(NodeId(u64::MAX));
        assert_eq!(ids.next_id(), NodeId::MAX);
        assert_eq!(ids.next_id(), NodeId::MAX);
    }

    #[test]
    fn ids_display_with_their_kind() {
        assert_eq!(DocId(3).to_string(), "doc#3");
        assert_eq!(NodeId(9).to_string(), "node#9");
        assert_eq!(DocId(3).get(), 3);
        assert_eq!(NodeId(9).get(), 9);
    }
}
