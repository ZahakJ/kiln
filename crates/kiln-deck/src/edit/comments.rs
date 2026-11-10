// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comments: new, reply, resolve, delete.

use kiln_common::NodeId;

use super::tx;
use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::Presentation;
use crate::slide::{Comment, CommentReply};

/// Who wrote something and when.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Byline<'a> {
    /// Display name.
    pub author: &'a str,
    /// Initials.
    pub initials: &'a str,
    /// ISO 8601 time (the host's clock).
    pub created: &'a str,
}

/// New Comment on a slide, optionally attached to a shape on it.
///
/// # Errors
/// An unknown slide, or a shape that is not on it.
pub fn add_comment(
    pres: &mut Presentation,
    slide: NodeId,
    shape: Option<NodeId>,
    pos: (i64, i64),
    by: &Byline<'_>,
    text: &str,
) -> Result<Transaction> {
    let s = pres.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
    if let Some(sh) = shape
        && pres.slide_of_shape(sh).is_none_or(|x| x.id != slide)
    {
        return Err(DeckError::WrongKind {
            id: sh,
            expected: "shape on the slide",
        });
    }
    let index = s.comments.len();
    let c = Comment {
        id: pres.alloc_id(),
        author: by.author.to_owned(),
        initials: by.initials.to_owned(),
        created: by.created.to_owned(),
        text: text.to_owned(),
        shape,
        pos,
        replies: Vec::new(),
        resolved: false,
    };
    let id = c.id;
    Ok(tx(
        vec![Op::InsertComment {
            slide,
            index,
            comment: Box::new(c),
        }],
        EditKind::Other,
        vec![id],
    ))
}

fn find(pres: &Presentation, id: NodeId) -> Result<(NodeId, usize, Comment)> {
    pres.slides
        .iter()
        .find_map(|s| {
            s.comments
                .iter()
                .position(|c| c.id == id)
                .map(|i| (s.id, i, s.comments[i].clone()))
        })
        .ok_or(DeckError::UnknownNode(id))
}

/// Reply to a comment.
///
/// # Errors
/// An unknown comment.
pub fn reply_to_comment(
    pres: &mut Presentation,
    id: NodeId,
    by: &Byline<'_>,
    text: &str,
) -> Result<Transaction> {
    let (slide, index, mut c) = find(pres, id)?;
    c.replies.push(CommentReply {
        id: pres.alloc_id(),
        author: by.author.to_owned(),
        created: by.created.to_owned(),
        text: text.to_owned(),
    });
    Ok(tx(
        vec![Op::SetComment {
            slide,
            index,
            comment: Box::new(c),
        }],
        EditKind::Other,
        vec![id],
    ))
}

/// Resolve (or reopen) a comment thread.
///
/// # Errors
/// An unknown comment.
pub fn resolve_comment(pres: &mut Presentation, id: NodeId, resolved: bool) -> Result<Transaction> {
    let (slide, index, mut c) = find(pres, id)?;
    if c.resolved == resolved {
        return Ok(Transaction::default());
    }
    c.resolved = resolved;
    Ok(tx(
        vec![Op::SetComment {
            slide,
            index,
            comment: Box::new(c),
        }],
        EditKind::Other,
        vec![id],
    ))
}

/// Delete a comment thread.
///
/// # Errors
/// An unknown comment.
pub fn delete_comment(pres: &mut Presentation, id: NodeId) -> Result<Transaction> {
    let (slide, index, _) = find(pres, id)?;
    Ok(tx(
        vec![Op::RemoveComment { slide, index }],
        EditKind::Other,
        Vec::new(),
    ))
}
