// SPDX-License-Identifier: AGPL-3.0-or-later
//! The slides of a Deck version marked against the version before it (Version History's
//! thumbnails). A pure comparison of two presentations: it needs no CRDT, so Version History of
//! files that are not shared (`kiln-session`'s storage) uses it as well.

use kiln_common::NodeId;
use kiln_deck::Presentation;
use serde::{Deserialize, Serialize};

/// How a slide of a version differs from the version before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideMark {
    /// Unchanged.
    Same,
    /// New in this version.
    Added,
    /// Edited.
    Changed,
}

/// One slide of a version, for its thumbnail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideChange {
    /// The slide.
    pub slide: NodeId,
    /// How it differs from the version before.
    pub mark: SlideMark,
}

/// The slides of `now` marked against `before` (all added when there is none).
#[must_use]
pub fn changed_slides(before: Option<&Presentation>, now: &Presentation) -> Vec<SlideChange> {
    now.slides
        .iter()
        .map(|s| SlideChange {
            slide: s.id,
            mark: match before.and_then(|b| b.slide(s.id)) {
                None => SlideMark::Added,
                Some(old) if old == s => SlideMark::Same,
                Some(_) => SlideMark::Changed,
            },
        })
        .collect()
}
