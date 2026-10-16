// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text in shapes, cells and notes. Edits are built by kiln-doc's own builders on a copy of the
//! body's document and wrapped in [`Op::EditText`], so typing, deleting, formatting and Enter
//! behave exactly as in Write.

use kiln_common::NodeId;
use kiln_doc::{
    Block, DocError, Document, Fragment, Op as DocOp, Paragraph, Pos, RunProps,
    Transaction as DocTransaction,
};

use super::tx;
use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, TextTarget, Transaction};
use crate::presentation::Presentation;
use crate::shape::ShapeKind;
use crate::text::{BodyProps, DeckParaProps, LEVELS, TextBody};

/// The text body a target names.
///
/// # Errors
/// Unknown targets or targets without text.
pub fn text_body(pres: &Presentation, target: TextTarget) -> Result<&TextBody> {
    match target {
        TextTarget::Shape(id) => pres
            .shape(id)
            .ok_or(DeckError::UnknownNode(id))?
            .text()
            .ok_or(DeckError::WrongKind {
                id,
                expected: "shape with text",
            }),
        TextTarget::Cell { shape, row, col } => {
            let s = pres.shape(shape).ok_or(DeckError::UnknownNode(shape))?;
            let ShapeKind::Table(t) = &s.kind else {
                return Err(DeckError::WrongKind {
                    id: shape,
                    expected: "table",
                });
            };
            t.cell(row, col)
                .map(|c| &c.text)
                .ok_or(DeckError::OutOfRange {
                    index: row,
                    len: t.rows.len(),
                })
        }
        TextTarget::Notes(slide) => pres
            .slide(slide)
            .ok_or(DeckError::UnknownNode(slide))?
            .notes
            .as_ref()
            .map(|n| &n.text)
            .ok_or(DeckError::WrongKind {
                id: slide,
                expected: "slide with notes",
            }),
    }
}

/// Builds a text edit with a kiln-doc builder (`kiln_doc::edit::*`) run on a copy of the body's
/// document. The transaction's kind is the kiln-doc transaction's (typing, delete, format, …).
///
/// # Errors
/// An unknown target, or the builder's error.
pub fn edit_text(
    pres: &Presentation,
    target: TextTarget,
    build: impl FnOnce(&mut Document) -> std::result::Result<DocTransaction, DocError>,
) -> Result<Transaction> {
    let mut doc = text_body(pres, target)?.doc().clone();
    let doc_tx = build(&mut doc)?;
    let kind = match doc_tx.kind {
        kiln_doc::EditKind::Typing => EditKind::Typing,
        kiln_doc::EditKind::Delete => EditKind::Delete,
        kiln_doc::EditKind::Format => EditKind::Format,
        kiln_doc::EditKind::Structure | kiln_doc::EditKind::Other => EditKind::Other,
    };
    if doc_tx.is_empty() {
        return Ok(Transaction::default());
    }
    Ok(tx(
        vec![Op::EditText {
            target,
            tx: doc_tx,
            para_props: None,
        }],
        kind,
        Vec::new(),
    ))
}

/// Types text at a position (`\n` starts a new paragraph).
///
/// # Errors
/// An unknown target or a bad position.
pub fn insert_text(
    pres: &Presentation,
    target: TextTarget,
    at: Pos,
    text: &str,
) -> Result<Transaction> {
    edit_text(pres, target, |doc| {
        kiln_doc::edit::insert_text(doc, at, text)
    })
}

/// Replaces all text of a body with plain text (one paragraph per line), keeping the first
/// paragraph's properties for every new paragraph.
///
/// # Errors
/// An unknown target.
pub fn set_text(pres: &Presentation, target: TextTarget, text: &str) -> Result<Transaction> {
    let body = text_body(pres, target)?;
    let mut doc = body.doc().clone();
    let first = body.paragraphs().next().cloned();
    let old = body.paragraph_count();
    let container = body.container_id();
    let mut blocks = Vec::new();
    for line in text.split('\n') {
        let mut p = Paragraph::new(doc.alloc_id());
        if let Some(f) = &first {
            p.props = f.props.clone();
            p.mark_props = f.mark_props.clone();
        }
        let props = first
            .as_ref()
            .and_then(|f| f.content.props_at(0).cloned())
            .unwrap_or_else(RunProps::default);
        p.content = kiln_doc::Inlines::from_fragment(Fragment::text(line, props))?;
        blocks.push(Block::Paragraph(p));
    }
    let doc_tx = DocTransaction::new(vec![
        DocOp::InsertBlocks {
            container,
            index: old,
            blocks,
        },
        DocOp::RemoveBlocks {
            container,
            index: 0,
            count: old,
        },
    ]);
    Ok(tx(
        vec![Op::EditText {
            target,
            tx: doc_tx,
            para_props: Some(Default::default()),
        }],
        EditKind::Other,
        Vec::new(),
    ))
}

/// Changes body properties (Align Text, Text Direction, columns, margins, autofit) with `f`.
///
/// # Errors
/// An unknown target.
pub fn set_body_props(
    pres: &Presentation,
    target: TextTarget,
    f: impl FnOnce(&mut BodyProps),
) -> Result<Transaction> {
    let old = &text_body(pres, target)?.body;
    let mut new = old.clone();
    f(&mut new);
    let ops = if new == *old {
        Vec::new()
    } else {
        vec![Op::SetBodyProps {
            target,
            body: Box::new(new),
        }]
    };
    Ok(tx(ops, EditKind::Format, Vec::new()))
}

/// Changes the DrawingML extras (bullets, numbering, spacing) of paragraphs with `f`.
///
/// # Errors
/// An unknown target or paragraph.
pub fn set_para_extras(
    pres: &Presentation,
    target: TextTarget,
    paras: &[NodeId],
    f: impl Fn(&mut DeckParaProps),
) -> Result<Transaction> {
    let body = text_body(pres, target)?;
    let mut ops = Vec::new();
    for p in paras {
        if body.paragraph(*p).is_none() {
            return Err(DeckError::UnknownNode(*p));
        }
        let old = body.deck_props(*p).cloned().unwrap_or_default();
        let mut new = old.clone();
        f(&mut new);
        if new != old {
            ops.push(Op::SetParaProps {
                target,
                para: *p,
                props: (!new.is_empty()).then(|| Box::new(new)),
            });
        }
    }
    Ok(tx(ops, EditKind::Format, Vec::new()))
}

/// Increase or Decrease List Level (Tab / Shift+Tab): moves paragraphs by `delta` levels,
/// within 0–8.
///
/// # Errors
/// An unknown target or paragraph.
pub fn set_paragraph_level(
    pres: &Presentation,
    target: TextTarget,
    paras: &[NodeId],
    delta: i32,
) -> Result<Transaction> {
    let body = text_body(pres, target)?;
    let mut ops = Vec::new();
    let max = i32::try_from(LEVELS - 1).unwrap_or(8);
    for id in paras {
        let p = body.paragraph(*id).ok_or(DeckError::UnknownNode(*id))?;
        let now = i32::try_from(TextBody::level_of(p)).unwrap_or(0);
        let next = (now + delta).clamp(0, max);
        if next != now {
            let mut props = p.props.clone();
            props.outline_level = u8::try_from(next).ok().filter(|l| *l > 0);
            ops.push(DocOp::SetParagraphProps {
                para: *id,
                props: Box::new(props),
            });
        }
    }
    if ops.is_empty() {
        return Ok(Transaction::default());
    }
    Ok(tx(
        vec![Op::EditText {
            target,
            tx: DocTransaction::new(ops).with_kind(kiln_doc::EditKind::Format),
            para_props: None,
        }],
        EditKind::Format,
        Vec::new(),
    ))
}
