// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arrange: order (bring forward/to front, send backward/to back), group and ungroup, align,
//! distribute, rotate and flip.

use std::collections::HashSet;

use kiln_common::NodeId;

use super::shapes::unanimate_ops;
use super::tx;
use crate::error::{DeckError, Result};
use crate::geom::{ChildSpace, Rect, Transform, normalize_rotation};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::{Owner, Presentation, ShapePath};
use crate::shape::{Group, Shape, ShapeKind, ShapeProps};

/// The shapes' common parent and their indices in it (sorted).
fn siblings(pres: &Presentation, ids: &[NodeId]) -> Result<(NodeId, Vec<usize>, Vec<ShapePath>)> {
    let mut parent = None;
    let mut idx = Vec::new();
    let mut paths = Vec::new();
    for id in ids {
        let p = pres.locate(*id).ok_or(DeckError::UnknownNode(*id))?;
        let par = p.parent_id(pres).ok_or(DeckError::UnknownNode(*id))?;
        if parent.is_some_and(|q| q != par) {
            return Err(DeckError::InvalidOp(
                "the shapes are not in the same slide or group".into(),
            ));
        }
        parent = Some(par);
        idx.push(p.index());
        paths.push(p);
    }
    idx.sort_unstable();
    idx.dedup();
    let parent = parent.ok_or_else(|| DeckError::InvalidOp("nothing selected".into()))?;
    Ok((parent, idx, paths))
}

fn list_len(pres: &Presentation, parent: NodeId) -> usize {
    pres.shape_list(parent).map_or(0, Vec::len)
}

/// Applies index moves to a working order and records them.
fn moves_to(order_now: &[usize], target: &[usize], parent: NodeId) -> Vec<Op> {
    // `order_now[i]` is the original index of the shape at position i; `target` the desired
    // sequence of original indices.
    let mut work = order_now.to_vec();
    let mut ops = Vec::new();
    for (i, want) in target.iter().enumerate() {
        if let Some(pos) = work.iter().position(|x| x == want)
            && pos != i
        {
            let v = work.remove(pos);
            work.insert(i, v);
            ops.push(Op::MoveShape {
                parent,
                from: pos,
                to: i,
            });
        }
    }
    ops
}

fn reorder(
    pres: &Presentation,
    ids: &[NodeId],
    f: impl Fn(&[bool]) -> Vec<usize>,
) -> Result<Transaction> {
    let (parent, idx, _) = siblings(pres, ids)?;
    let n = list_len(pres, parent);
    let mut selected = vec![false; n];
    for i in &idx {
        selected[*i] = true;
    }
    let target = f(&selected);
    let now: Vec<usize> = (0..n).collect();
    Ok(tx(
        moves_to(&now, &target, parent),
        EditKind::Arrange,
        ids.to_vec(),
    ))
}

/// Bring to Front.
///
/// # Errors
/// Unknown shapes or shapes in different lists.
pub fn bring_to_front(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    reorder(pres, ids, |sel| {
        let mut v: Vec<usize> = (0..sel.len()).filter(|i| !sel[*i]).collect();
        v.extend((0..sel.len()).filter(|i| sel[*i]));
        v
    })
}

/// Send to Back.
///
/// # Errors
/// Unknown shapes or shapes in different lists.
pub fn send_to_back(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    reorder(pres, ids, |sel| {
        let mut v: Vec<usize> = (0..sel.len()).filter(|i| sel[*i]).collect();
        v.extend((0..sel.len()).filter(|i| !sel[*i]));
        v
    })
}

/// Bring Forward: each selected shape moves above the next unselected shape.
///
/// # Errors
/// Unknown shapes or shapes in different lists.
pub fn bring_forward(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    reorder(pres, ids, |sel| {
        let mut v: Vec<usize> = (0..sel.len()).collect();
        for i in (0..v.len().saturating_sub(1)).rev() {
            if sel[v[i]] && !sel[v[i + 1]] {
                v.swap(i, i + 1);
            }
        }
        v
    })
}

/// Send Backward: each selected shape moves below the previous unselected shape.
///
/// # Errors
/// Unknown shapes or shapes in different lists.
pub fn send_backward(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    reorder(pres, ids, |sel| {
        let mut v: Vec<usize> = (0..sel.len()).collect();
        for i in 1..v.len() {
            if sel[v[i]] && !sel[v[i - 1]] {
                v.swap(i, i - 1);
            }
        }
        v
    })
}

fn xfrm_of(pres: &Presentation, id: NodeId) -> Result<Transform> {
    pres.resolved_xfrm(id)
        .ok_or(DeckError::InvalidOp(format!("shape {id} has no position")))
}

/// Group: puts shapes of one slide or group into a new group at the topmost one's place. Their
/// animations are removed, as in PowerPoint. Placeholders cannot be grouped.
///
/// # Errors
/// Fewer than two shapes, shapes in different lists, placeholders, or shapes without a
/// position.
pub fn group(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let (parent, idx, paths) = siblings(pres, ids)?;
    if idx.len() < 2 {
        return Err(DeckError::InvalidOp(
            "grouping needs two or more shapes".into(),
        ));
    }
    let list = pres
        .shape_list(parent)
        .ok_or(DeckError::UnknownNode(parent))?
        .clone();
    let mut bounds: Option<Rect> = None;
    let mut children: Vec<Shape> = Vec::new();
    for i in &idx {
        let s = &list[*i];
        if s.placeholder.is_some() && matches!(paths[0].owner, Owner::Slide(_)) {
            return Err(DeckError::InvalidOp(
                "placeholders cannot be grouped".into(),
            ));
        }
        let x = s
            .xfrm
            .ok_or_else(|| DeckError::InvalidOp(format!("shape {} has no position", s.id)))?;
        let b = x.bounds();
        bounds = Some(bounds.map_or(b, |u| u.union(&b)));
        children.push(s.clone());
    }
    let bounds = bounds.unwrap_or_default();
    let mut gone = HashSet::new();
    for c in &children {
        c.walk(&mut |x| {
            gone.insert(x.id);
        });
    }
    let mut ops = match paths[0].owner {
        Owner::Slide(s) => unanimate_ops(pres, s, &gone),
        _ => Vec::new(),
    };
    for i in idx.iter().rev() {
        ops.push(Op::RemoveShapes {
            parent,
            index: *i,
            count: 1,
        });
    }
    let id = pres.alloc_id();
    let top = idx.last().copied().unwrap_or(0) + 1 - idx.len();
    let group = Shape::new(
        id,
        format!("Group {}", id.0),
        Some(Transform::from_rect(bounds)),
        ShapeKind::Group(Group {
            child_space: ChildSpace::identity(bounds),
            children,
            props: ShapeProps::default(),
        }),
    );
    ops.push(Op::InsertShapes {
        parent,
        index: top,
        shapes: vec![group],
    });
    Ok(tx(ops, EditKind::Arrange, vec![id]))
}

/// Ungroup: the group's children take its place, mapped into the parent's coordinates (the
/// group's scale, rotation and flips applied). The group's animations are removed.
///
/// # Errors
/// An unknown shape or not a group.
pub fn ungroup(pres: &mut Presentation, id: NodeId) -> Result<Transaction> {
    let path = pres.locate(id).ok_or(DeckError::UnknownNode(id))?;
    let parent = path.parent_id(pres).ok_or(DeckError::UnknownNode(id))?;
    let s = pres
        .shape_at(&path)
        .ok_or(DeckError::UnknownNode(id))?
        .clone();
    let ShapeKind::Group(g) = &s.kind else {
        return Err(DeckError::WrongKind {
            id,
            expected: "group",
        });
    };
    let gx = s.xfrm.unwrap_or_else(|| {
        Transform::from_rect(Rect::new(
            g.child_space.x,
            g.child_space.y,
            g.child_space.cx,
            g.child_space.cy,
        ))
    });
    let children: Vec<Shape> = g
        .children
        .iter()
        .map(|c| {
            let mut c = c.clone();
            if let Some(x) = c.xfrm {
                c.xfrm = Some(g.child_space.child_to_parent(&gx, &x));
            }
            c
        })
        .collect();
    let mut ops = match path.owner {
        Owner::Slide(slide) => unanimate_ops(pres, slide, &HashSet::from([id])),
        _ => Vec::new(),
    };
    let ids: Vec<NodeId> = children.iter().map(|c| c.id).collect();
    ops.push(Op::RemoveShapes {
        parent,
        index: path.index(),
        count: 1,
    });
    ops.push(Op::InsertShapes {
        parent,
        index: path.index(),
        shapes: children,
    });
    Ok(tx(ops, EditKind::Arrange, ids))
}

/// Align options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    /// Align Left.
    Left,
    /// Align Center.
    Center,
    /// Align Right.
    Right,
    /// Align Top.
    Top,
    /// Align Middle.
    Middle,
    /// Align Bottom.
    Bottom,
}

/// What shapes align or distribute against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignTo {
    /// The slide.
    Slide,
    /// The selection's bounds (a single shape aligns to the slide).
    Selection,
}

fn slide_rect(pres: &Presentation) -> Rect {
    Rect::new(0, 0, pres.slide_size.cx, pres.slide_size.cy)
}

/// Align: moves shapes so their (rotated) bounds line up.
///
/// # Errors
/// Unknown shapes or shapes without a position.
pub fn align(
    pres: &mut Presentation,
    ids: &[NodeId],
    how: Align,
    to: AlignTo,
) -> Result<Transaction> {
    let xs: Vec<(NodeId, Transform)> = ids
        .iter()
        .map(|id| xfrm_of(pres, *id).map(|x| (*id, x)))
        .collect::<Result<_>>()?;
    let reference = if to == AlignTo::Slide || xs.len() < 2 {
        slide_rect(pres)
    } else {
        xs.iter()
            .map(|(_, x)| x.bounds())
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default()
    };
    let mut ops = Vec::new();
    for (id, x) in xs {
        let b = x.bounds();
        let (dx, dy) = match how {
            Align::Left => (reference.x - b.x, 0),
            Align::Center => (reference.center().0 - b.center().0, 0),
            Align::Right => (reference.right() - b.right(), 0),
            Align::Top => (0, reference.y - b.y),
            Align::Middle => (0, reference.center().1 - b.center().1),
            Align::Bottom => (0, reference.bottom() - b.bottom()),
        };
        if dx != 0 || dy != 0 {
            ops.push(Op::SetTransform {
                shape: id,
                xfrm: Some(x.translated(dx, dy)),
            });
        }
    }
    Ok(tx(ops, EditKind::Arrange, ids.to_vec()))
}

/// Distribute Horizontally (`horizontal`) or Vertically: equal gaps between the shapes' bounds,
/// within the selection's extent (the outer shapes stay) or across the slide.
///
/// # Errors
/// Unknown shapes or shapes without a position.
pub fn distribute(
    pres: &mut Presentation,
    ids: &[NodeId],
    horizontal: bool,
    to: AlignTo,
) -> Result<Transaction> {
    let mut xs: Vec<(NodeId, Transform, Rect)> = ids
        .iter()
        .map(|id| xfrm_of(pres, *id).map(|x| (*id, x, x.bounds())))
        .collect::<Result<_>>()?;
    let n = xs.len();
    if n < 2 || (to == AlignTo::Selection && n < 3) {
        return Ok(Transaction::default());
    }
    let axis = |r: &Rect| if horizontal { (r.x, r.cx) } else { (r.y, r.cy) };
    xs.sort_by_key(|(_, _, b)| axis(b).0);
    let total: i64 = xs.iter().map(|(_, _, b)| axis(b).1).sum();
    let (start, end) = match to {
        AlignTo::Slide => {
            let s = slide_rect(pres);
            if horizontal { (0, s.cx) } else { (0, s.cy) }
        }
        AlignTo::Selection => {
            let first = &xs[0].2;
            let last = &xs[n - 1].2;
            let (fp, _) = axis(first);
            let (lp, ls) = axis(last);
            (fp, lp + ls)
        }
    };
    let gaps = i64::try_from(n - 1).unwrap_or(1);
    let space = end - start - total;
    let mut cursor = start;
    let mut ops = Vec::new();
    for (k, (id, x, b)) in xs.iter().enumerate() {
        let k_i = i64::try_from(k).unwrap_or(0);
        // Spread the rounding remainder over the first gaps.
        let gap_before = if k == 0 {
            0
        } else {
            space * k_i / gaps - space * (k_i - 1) / gaps
        };
        cursor += gap_before;
        let (p, s) = axis(b);
        let d = cursor - p;
        cursor += s;
        if d != 0 {
            let n = if horizontal {
                x.translated(d, 0)
            } else {
                x.translated(0, d)
            };
            ops.push(Op::SetTransform {
                shape: *id,
                xfrm: Some(n),
            });
        }
    }
    Ok(tx(ops, EditKind::Arrange, ids.to_vec()))
}

/// Rotates shapes by an angle (60 000ths of a degree; Rotate Right 90° is 5 400 000).
///
/// # Errors
/// Unknown shapes or shapes without a position.
pub fn rotate_by(pres: &mut Presentation, ids: &[NodeId], delta: i32) -> Result<Transaction> {
    let mut ops = Vec::new();
    for id in ids {
        let mut x = xfrm_of(pres, *id)?;
        x.rot = normalize_rotation(x.rot.saturating_add(delta));
        ops.push(Op::SetTransform {
            shape: *id,
            xfrm: Some(x),
        });
    }
    Ok(tx(ops, EditKind::Arrange, ids.to_vec()))
}

/// Flip Horizontal (`horizontal`) or Vertical.
///
/// # Errors
/// Unknown shapes or shapes without a position.
pub fn flip(pres: &mut Presentation, ids: &[NodeId], horizontal: bool) -> Result<Transaction> {
    let mut ops = Vec::new();
    for id in ids {
        let mut x = xfrm_of(pres, *id)?;
        if horizontal {
            x.flip_h = !x.flip_h;
        } else {
            x.flip_v = !x.flip_v;
        }
        // A flip mirrors the rotation too, as in PowerPoint.
        x.rot = normalize_rotation(-x.rot);
        ops.push(Op::SetTransform {
            shape: *id,
            xfrm: Some(x),
        });
    }
    Ok(tx(ops, EditKind::Arrange, ids.to_vec()))
}
