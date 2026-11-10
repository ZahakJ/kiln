// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sections (Home ▸ Slides ▸ Section): add, rename, remove (keeping or deleting the slides) and
//! move up or down with their slides.

use std::collections::HashMap;

use kiln_common::NodeId;

use super::tx;
use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::Presentation;
use crate::slide::Section;

fn index_of(pres: &Presentation, id: NodeId) -> Result<usize> {
    pres.sections
        .iter()
        .position(|s| s.id == id)
        .ok_or(DeckError::UnknownNode(id))
}

/// Add Section: a new section starting at a slide index (the rest of the section it splits).
/// The first section added to a deck also creates "Default Section" for the slides before it,
/// as PowerPoint does.
///
/// # Errors
/// An index past the end.
pub fn add_section(pres: &mut Presentation, at: usize, name: &str) -> Result<Transaction> {
    let len = pres.slides.len();
    if at > len {
        return Err(DeckError::OutOfRange { index: at, len });
    }
    let mut sections = pres.sections.clone();
    let id = pres.alloc_id();
    let new = Section {
        id,
        name: name.to_owned(),
        len: 0,
    };
    if sections.is_empty() {
        if at > 0 {
            sections.push(Section {
                id: pres.alloc_id(),
                name: "Default Section".to_owned(),
                len: at,
            });
        }
        sections.push(Section {
            len: len - at,
            ..new
        });
    } else {
        // The section containing `at` (or the last one when adding at the end) is split.
        let k = pres.section_of_index(at).unwrap_or(sections.len() - 1);
        let start_k = pres.section_start(k);
        // The new section takes the slides from `at` to the end of section k.
        let tail = start_k.saturating_add(sections[k].len).saturating_sub(at);
        sections[k].len -= tail;
        sections.insert(k + 1, Section { len: tail, ..new });
    }
    Ok(tx(
        vec![Op::SetSections { sections }],
        EditKind::Structure,
        vec![id],
    ))
}

/// Rename Section.
///
/// # Errors
/// An unknown section.
pub fn rename_section(pres: &mut Presentation, id: NodeId, name: &str) -> Result<Transaction> {
    let i = index_of(pres, id)?;
    let mut sections = pres.sections.clone();
    sections[i].name = name.to_owned();
    Ok(tx(
        vec![Op::SetSections { sections }],
        EditKind::Structure,
        vec![id],
    ))
}

/// Remove Section: with `delete_slides`, its slides go too; otherwise they join the section
/// before (or after, for the first). Removing the last section removes the section list.
///
/// # Errors
/// An unknown section.
pub fn remove_section(
    pres: &mut Presentation,
    id: NodeId,
    delete_slides: bool,
) -> Result<Transaction> {
    let i = index_of(pres, id)?;
    let mut sections = pres.sections.clone();
    let removed = sections.remove(i);
    let mut ops = Vec::new();
    if delete_slides {
        let start = pres.section_start(i);
        for index in (start..start.saturating_add(removed.len)).rev() {
            ops.push(Op::RemoveSlide { index });
        }
    } else if let Some(prev) = i.checked_sub(1).and_then(|p| sections.get_mut(p)) {
        prev.len += removed.len;
    } else if let Some(next) = sections.get_mut(0) {
        next.len += removed.len;
    }
    if sections.len() == 1 && sections[0].name == "Default Section" && !delete_slides {
        sections.clear();
    }
    ops.push(Op::SetSections { sections });
    Ok(tx(ops, EditKind::Structure, Vec::new()))
}

/// Move Section Up (`delta` −1) or Down (+1), or further, with its slides.
///
/// # Errors
/// An unknown section.
pub fn move_section(pres: &mut Presentation, id: NodeId, delta: isize) -> Result<Transaction> {
    let i = index_of(pres, id)?;
    let Some(j) = i
        .checked_add_signed(delta)
        .filter(|j| *j < pres.sections.len())
    else {
        return Ok(Transaction::default());
    };
    if i == j {
        return Ok(Transaction::default());
    }
    // Slide blocks per section, then the sections in their new order.
    let mut blocks: Vec<Vec<NodeId>> = Vec::new();
    let mut start: usize = 0;
    for s in &pres.sections {
        let end = start.saturating_add(s.len);
        let block = pres.slides.get(start..end).ok_or_else(|| {
            DeckError::InvalidOp("the sections do not partition the slides".into())
        })?;
        blocks.push(block.iter().map(|x| x.id).collect());
        start = end;
    }
    let mut order_idx: Vec<usize> = (0..pres.sections.len()).collect();
    let moved = order_idx.remove(i);
    order_idx.insert(j, moved);
    let mut order = Vec::new();
    let mut section_of = HashMap::new();
    for (new_index, old_index) in order_idx.iter().enumerate() {
        for slide in &blocks[*old_index] {
            order.push(*slide);
            section_of.insert(*slide, new_index);
        }
    }
    let sections: Vec<_> = order_idx
        .iter()
        .map(|k| pres.sections[*k].clone())
        .collect();
    let mut ops: Vec<Op> = super::slides::reorder(pres, &order, &section_of)
        .into_iter()
        .filter(|op| matches!(op, Op::MoveSlide { .. }))
        .collect();
    ops.push(Op::SetSections { sections });
    Ok(tx(ops, EditKind::Structure, vec![id]))
}
