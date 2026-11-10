// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape commands: insert (text boxes, shapes, pictures, tables, charts, connectors), delete,
//! move and size, formatting, names and alt text, picture crop.

use std::collections::{BTreeMap, HashSet};

use kiln_common::NodeId;

use super::tx;
use crate::error::{DeckError, Result};
use crate::geom::{Rect, Transform, clamp_coord};
use crate::ops::{EditKind, Op, ShapeInfo, Transaction};
use crate::presentation::{Owner, Presentation, shape_ids};
use crate::shape::{
    ChartRef, Connection, Connector, Crop, Geometry, MAX_TABLE_COLS, MAX_TABLE_ROWS, Picture,
    Shape, ShapeKind, ShapeProps, ShapeStyle, Table,
};

/// Inserts shapes (built with ids from [`Presentation::alloc_id`]) into a slide, layout,
/// master or group, on top (`index: None`) or at a z-index.
///
/// # Errors
/// An unknown parent or an index past the end.
pub fn insert_shapes(
    pres: &mut Presentation,
    parent: NodeId,
    shapes: Vec<Shape>,
    index: Option<usize>,
) -> Result<Transaction> {
    let len = pres
        .shape_list(parent)
        .ok_or(DeckError::UnknownNode(parent))?
        .len();
    let index = index.unwrap_or(len);
    if index > len {
        return Err(DeckError::OutOfRange { index, len });
    }
    let ids = shapes.iter().map(|s| s.id).collect();
    Ok(tx(
        vec![Op::InsertShapes {
            parent,
            index,
            shapes,
        }],
        EditKind::Structure,
        ids,
    ))
}

/// A picture shape (Insert ▸ Pictures) for an image already in the parts store.
#[must_use]
pub fn new_picture(pres: &mut Presentation, media: &str, rect: Rect) -> Shape {
    let id = pres.alloc_id();
    Shape::new(
        id,
        format!("Picture {}", id.0),
        Some(Transform::from_rect(rect)),
        ShapeKind::Picture(Picture {
            media: media.to_owned(),
            props: ShapeProps {
                geometry: Some(Geometry::preset("rect")),
                ..ShapeProps::default()
            },
            ..Picture::default()
        }),
    )
}

/// A table (Insert ▸ Table) of 1 to [`MAX_TABLE_ROWS`] rows and 1 to [`MAX_TABLE_COLS`]
/// columns, PowerPoint's limits.
///
/// # Errors
/// [`DeckError::InvalidOp`] for zero or too many rows or columns.
pub fn new_table(pres: &mut Presentation, rows: usize, cols: usize, rect: Rect) -> Result<Shape> {
    if !(1..=MAX_TABLE_ROWS).contains(&rows) || !(1..=MAX_TABLE_COLS).contains(&cols) {
        return Err(DeckError::InvalidOp(format!(
            "a table has 1 to {MAX_TABLE_ROWS} rows and 1 to {MAX_TABLE_COLS} columns, not \
             {rows} × {cols}"
        )));
    }
    let id = pres.alloc_id();
    Ok(Shape::new(
        id,
        format!("Table {}", id.0),
        Some(Transform::from_rect(rect)),
        ShapeKind::Table(Table::with_size(rows, cols, rect.cx, rect.cy)),
    ))
}

/// A chart frame for a chart part already in the parts store.
#[must_use]
pub fn new_chart(pres: &mut Presentation, part: &str, rect: Rect) -> Shape {
    let id = pres.alloc_id();
    Shape::new(
        id,
        format!("Chart {}", id.0),
        Some(Transform::from_rect(rect)),
        ShapeKind::Chart(ChartRef {
            part: part.to_owned(),
        }),
    )
}

/// A connector from one point to another (`straightConnector1`, `bentConnector3`,
/// `curvedConnector3`), optionally attached to shapes' connection sites.
#[must_use]
pub fn new_connector(
    pres: &mut Presentation,
    preset: &str,
    from: (i64, i64),
    to: (i64, i64),
    ends: (Option<Connection>, Option<Connection>),
) -> Shape {
    let id = pres.alloc_id();
    let from = (clamp_coord(from.0), clamp_coord(from.1));
    let to = (clamp_coord(to.0), clamp_coord(to.1));
    let mut x = Transform::new(
        from.0.min(to.0),
        from.1.min(to.1),
        (to.0 - from.0).abs(),
        (to.1 - from.1).abs(),
    )
    .clamped();
    x.flip_h = to.0 < from.0;
    x.flip_v = to.1 < from.1;
    Shape::new(
        id,
        format!("Connector {}", id.0),
        Some(x),
        ShapeKind::Connector(Connector {
            props: ShapeProps {
                geometry: Some(Geometry::preset(preset)),
                style: Some(ShapeStyle::default_line()),
                ..ShapeProps::default()
            },
            start: ends.0,
            end: ends.1,
        }),
    )
}

/// Every id in a shape and its descendants.
fn ids_in(s: &Shape) -> Vec<NodeId> {
    let mut out = Vec::new();
    shape_ids(std::slice::from_ref(s), &mut out);
    out
}

/// Ops that remove effects and trigger effects on `slide` that refer to any of `ids`.
pub(crate) fn unanimate_ops(pres: &Presentation, slide: NodeId, ids: &HashSet<NodeId>) -> Vec<Op> {
    let Some(s) = pres.slide(slide) else {
        return Vec::new();
    };
    let mut ops: Vec<Op> = s
        .timeline
        .sequence
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, e)| ids.contains(&e.target.shape))
        .map(|(index, _)| Op::RemoveEffect { slide, index })
        .collect();
    let refers = |t: &crate::animation::TriggerSequence| {
        ids.contains(&t.trigger) || t.effects.iter().any(|e| ids.contains(&e.target.shape))
    };
    if s.timeline.triggers.iter().any(refers) {
        let kept = s
            .timeline
            .triggers
            .iter()
            .filter(|t| !refers(t))
            .cloned()
            .collect();
        ops.push(Op::SetTriggers {
            slide,
            triggers: kept,
        });
    }
    ops
}

/// Delete: removes shapes (and their animations). Placeholders on slides can be deleted (the
/// layout's stays).
///
/// # Errors
/// Unknown shapes.
pub fn delete_shapes(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let mut by_parent: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
    let mut gone = HashSet::new();
    let mut slides = Vec::new();
    for id in ids {
        let path = pres.locate(*id).ok_or(DeckError::UnknownNode(*id))?;
        let parent = path.parent_id(pres).ok_or(DeckError::UnknownNode(*id))?;
        by_parent.entry(parent).or_default().push(path.index());
        if let Some(s) = pres.shape_at(&path) {
            gone.extend(ids_in(s));
        }
        if let Owner::Slide(s) = path.owner
            && !slides.contains(&s)
        {
            slides.push(s);
        }
    }
    let mut ops = Vec::new();
    for s in slides {
        ops.extend(unanimate_ops(pres, s, &gone));
    }
    // Inner lists first (a group's children before the group), each back to front.
    let mut parents: Vec<(NodeId, Vec<usize>)> = by_parent.into_iter().collect();
    parents.sort_by_key(|(p, _)| std::cmp::Reverse(pres.locate(*p).map_or(0, |l| l.path.len())));
    for (parent, mut idx) in parents {
        if gone.contains(&parent) {
            continue;
        }
        idx.sort_unstable();
        idx.dedup();
        for index in idx.into_iter().rev() {
            ops.push(Op::RemoveShapes {
                parent,
                index,
                count: 1,
            });
        }
    }
    Ok(tx(ops, EditKind::Structure, Vec::new()))
}

/// Sets a shape's transform.
///
/// # Errors
/// An unknown shape.
pub fn set_transform(pres: &mut Presentation, id: NodeId, xfrm: Transform) -> Result<Transaction> {
    let s = pres.shape(id).ok_or(DeckError::UnknownNode(id))?;
    let ops = if s.xfrm == Some(xfrm) {
        Vec::new()
    } else {
        vec![Op::SetTransform {
            shape: id,
            xfrm: Some(xfrm),
        }]
    };
    Ok(tx(ops, EditKind::Arrange, vec![id]))
}

/// Moves shapes by an offset (drag, arrow-key nudge). Placeholders that inherit their position
/// get their own.
///
/// # Errors
/// Unknown shapes, or a placeholder whose position cannot be resolved.
pub fn move_shapes(
    pres: &mut Presentation,
    ids: &[NodeId],
    dx: i64,
    dy: i64,
) -> Result<Transaction> {
    let mut ops = Vec::new();
    for id in ids {
        let x = pres.resolved_xfrm(*id).ok_or(DeckError::UnknownNode(*id))?;
        if dx != 0 || dy != 0 {
            ops.push(Op::SetTransform {
                shape: *id,
                xfrm: Some(x.translated(dx, dy)),
            });
        }
    }
    Ok(tx(ops, EditKind::Arrange, ids.to_vec()))
}

/// Changes shapes' properties (fill, outline, effects, geometry, quick style) with `f`.
///
/// # Errors
/// Unknown shapes, or shapes without shape properties (tables, charts).
pub fn set_shape_props(
    pres: &mut Presentation,
    ids: &[NodeId],
    f: impl Fn(&mut ShapeProps),
) -> Result<Transaction> {
    let mut ops = Vec::new();
    for id in ids {
        let s = pres.shape(*id).ok_or(DeckError::UnknownNode(*id))?;
        let old = s.props().ok_or(DeckError::WrongKind {
            id: *id,
            expected: "shape with shape properties",
        })?;
        let mut new = old.clone();
        f(&mut new);
        if new != *old {
            ops.push(Op::SetShapeProps {
                shape: *id,
                props: Box::new(new),
            });
        }
    }
    Ok(tx(ops, EditKind::Format, ids.to_vec()))
}

/// Changes a shape's name, alt text, title, hidden flag or link with `f`.
///
/// # Errors
/// An unknown shape.
pub fn set_shape_info(
    pres: &mut Presentation,
    id: NodeId,
    f: impl FnOnce(&mut ShapeInfo),
) -> Result<Transaction> {
    let s = pres.shape(id).ok_or(DeckError::UnknownNode(id))?;
    let old = ShapeInfo::of(s);
    let mut new = old.clone();
    f(&mut new);
    let ops = if new == old {
        Vec::new()
    } else {
        vec![Op::SetShapeInfo {
            shape: id,
            info: Box::new(new),
        }]
    };
    Ok(tx(ops, EditKind::Other, vec![id]))
}

/// Crop: sets a picture's crop.
///
/// # Errors
/// An unknown shape or not a picture.
pub fn set_crop(pres: &mut Presentation, id: NodeId, crop: Crop) -> Result<Transaction> {
    let s = pres.shape(id).ok_or(DeckError::UnknownNode(id))?;
    let ShapeKind::Picture(p) = &s.kind else {
        return Err(DeckError::WrongKind {
            id,
            expected: "picture",
        });
    };
    if p.crop == crop {
        return Ok(Transaction::default());
    }
    let mut p = p.clone();
    p.crop = crop;
    Ok(tx(
        vec![Op::SetShapeKind {
            shape: id,
            kind: Box::new(ShapeKind::Picture(p)),
        }],
        EditKind::Format,
        vec![id],
    ))
}
