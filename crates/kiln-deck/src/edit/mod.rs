// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transaction builders for every Deck command: slides ([`slides`]), sections ([`sections`]),
//! shapes ([`shapes`]), arranging ([`arrange`]), text ([`text`]), transitions and animations
//! ([`motion`]), comments ([`comments`]) and themes ([`theme`]).
//!
//! Builders read the presentation (and allocate fresh ids from it) but never change its
//! content: they return a [`Transaction`] that the caller applies, usually through
//! [`crate::UndoStack::apply`]. A builder that has nothing to do returns an empty transaction.

pub mod arrange;
pub mod comments;
pub mod motion;
pub mod sections;
pub mod shapes;
pub mod slides;
pub mod text;
pub mod theme;

pub use arrange::{
    Align, AlignTo, align, bring_forward, bring_to_front, distribute, flip, group, rotate_by,
    send_backward, send_to_back, ungroup,
};
pub use comments::{add_comment, delete_comment, reply_to_comment, resolve_comment};
pub use motion::{
    add_effect, apply_transition_to_all, move_effect, remove_effects, set_transition, update_effect,
};
pub use sections::{add_section, move_section, remove_section, rename_section};
pub use shapes::{
    delete_shapes, insert_shapes, move_shapes, new_chart, new_connector, new_picture, new_table,
    set_crop, set_shape_info, set_shape_props, set_transform,
};
pub use slides::{
    ScaleMode, change_layout, delete_slides, duplicate_slides, insert_slide, move_slides,
    move_slides_to_section, new_slide, reset_slides, set_background, set_hidden, set_notes_text,
    set_slide_size,
};
pub use text::{
    edit_text, insert_text, set_body_props, set_para_extras, set_paragraph_level, set_text,
};
pub use theme::{apply_theme, apply_variant, set_theme_colors, set_theme_fonts};

use kiln_common::NodeId;

use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::Presentation;

/// A transaction of a kind that selects `after`.
pub(crate) fn tx(ops: Vec<Op>, kind: EditKind, after: Vec<NodeId>) -> Transaction {
    Transaction::new(ops)
        .with_kind(kind)
        .with_selection(Vec::new(), after)
}

/// The indices of slides, in presentation order, failing on unknown ids.
pub(crate) fn slide_indices(pres: &Presentation, ids: &[NodeId]) -> Result<Vec<usize>> {
    let mut out = ids
        .iter()
        .map(|id| pres.slide_index(*id).ok_or(DeckError::UnknownNode(*id)))
        .collect::<Result<Vec<_>>>()?;
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests;
