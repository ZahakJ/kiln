// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transitions and animations: set a transition, Apply To All, Add Animation, remove, edit
//! (effect options, start, duration, delay) and reorder effects.

use kiln_common::NodeId;

use super::{slide_indices, tx};
use crate::animation::{AnimationEffect, Effect};
use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::Presentation;
use crate::transition::Transition;

/// Sets (or with `None` removes) the transition of slides.
///
/// # Errors
/// Unknown slides.
pub fn set_transition(
    pres: &mut Presentation,
    ids: &[NodeId],
    transition: Option<Transition>,
) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let ops = idx
        .iter()
        .map(|i| &pres.slides[*i])
        .filter(|s| s.transition != transition)
        .map(|s| Op::SetTransition {
            slide: s.id,
            transition: transition.clone(),
        })
        .collect();
    Ok(tx(ops, EditKind::Other, ids.to_vec()))
}

/// Apply To All: every slide gets this slide's transition and timing.
///
/// # Errors
/// An unknown slide.
pub fn apply_transition_to_all(pres: &mut Presentation, from: NodeId) -> Result<Transaction> {
    let t = pres
        .slide(from)
        .ok_or(DeckError::UnknownNode(from))?
        .transition
        .clone();
    let all: Vec<NodeId> = pres.slides.iter().map(|s| s.id).collect();
    set_transition(pres, &all, t)
}

/// Add Animation: appends an effect on a shape to its slide's sequence (or inserts it at
/// `index`), with PowerPoint's defaults (on click, default duration).
///
/// # Errors
/// A shape that is not on a slide, or an index past the end.
pub fn add_effect(
    pres: &mut Presentation,
    shape: NodeId,
    effect: AnimationEffect,
    index: Option<usize>,
) -> Result<Transaction> {
    let slide = pres.slide_of_shape(shape).ok_or(DeckError::WrongKind {
        id: shape,
        expected: "shape on a slide",
    })?;
    let slide_id = slide.id;
    let len = slide.timeline.sequence.len();
    let index = index.unwrap_or(len);
    if index > len {
        return Err(DeckError::OutOfRange { index, len });
    }
    let e = Effect::new(pres.alloc_id(), shape, effect);
    let id = e.id;
    Ok(tx(
        vec![Op::InsertEffect {
            slide: slide_id,
            index,
            effect: Box::new(e),
        }],
        EditKind::Other,
        vec![id],
    ))
}

fn find_effect(pres: &Presentation, id: NodeId) -> Result<(NodeId, usize)> {
    pres.slides
        .iter()
        .find_map(|s| s.timeline.position(id).map(|i| (s.id, i)))
        .ok_or(DeckError::UnknownNode(id))
}

/// Removes effects (the Animation Pane's Remove).
///
/// # Errors
/// Unknown effects.
pub fn remove_effects(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let mut found = ids
        .iter()
        .map(|id| find_effect(pres, *id))
        .collect::<Result<Vec<_>>>()?;
    found.sort_unstable();
    found.dedup();
    let ops = found
        .into_iter()
        .rev()
        .map(|(slide, index)| Op::RemoveEffect { slide, index })
        .collect();
    Ok(tx(ops, EditKind::Other, Vec::new()))
}

/// Changes an effect (Effect Options, Start, Duration, Delay, Repeat) with `f`; the id and
/// target stay.
///
/// # Errors
/// An unknown effect.
pub fn update_effect(
    pres: &mut Presentation,
    id: NodeId,
    f: impl FnOnce(&mut Effect),
) -> Result<Transaction> {
    let (slide, index) = find_effect(pres, id)?;
    let old = pres
        .slide(slide)
        .and_then(|s| s.timeline.sequence.get(index))
        .ok_or(DeckError::UnknownNode(id))?;
    let mut new = old.clone();
    f(&mut new);
    new.id = old.id;
    new.repeat = new.repeat.max(1);
    let ops = if new == *old {
        Vec::new()
    } else {
        vec![Op::SetEffect {
            slide,
            index,
            effect: Box::new(new),
        }]
    };
    Ok(tx(ops, EditKind::Other, vec![id]))
}

/// Reorder (Move Earlier/Later, drag in the Animation Pane): moves an effect to an index.
///
/// # Errors
/// An unknown effect or an index past the end.
pub fn move_effect(pres: &mut Presentation, id: NodeId, to: usize) -> Result<Transaction> {
    let (slide, from) = find_effect(pres, id)?;
    let len = pres.slide(slide).map_or(0, |s| s.timeline.sequence.len());
    if to >= len {
        return Err(DeckError::OutOfRange { index: to, len });
    }
    let ops = if from == to {
        Vec::new()
    } else {
        vec![Op::MoveEffect { slide, from, to }]
    };
    Ok(tx(ops, EditKind::Other, vec![id]))
}
