// SPDX-License-Identifier: AGPL-3.0-or-later
//! Several objects at once: Shift or Ctrl+click adds a floating object to the selection, and
//! Align ▾ then lines the selected objects up with each other or spaces them evenly (Distribute),
//! as Word does; Group and Ungroup; the Selection pane's eye hides and shows objects.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::st::{WrapSide, WrapType};
use kiln_doc::{
    Drawing, DrawingKind, EditKind, Fragment, GroupMember, InlineObject, Op, Placement, Pos, Range,
    ShapeGroup, Transaction,
};
use kiln_protocol::commands::write_features::{ObjectAlign, RelativeTo};
use kiln_protocol::geom::Rect;

use super::super::WriteDoc;

/// The objects added to the selection, valid while the selection is the one they were added
/// to (any other selection change drops them).
#[derive(Debug, Clone, Default)]
pub(in crate::apps::write) struct MultiSelection {
    at: Option<(Pos, Pos)>,
    extra: Vec<NodeId>,
}

impl WriteDoc {
    /// The selected objects: the one the selection holds, then the ones added to it.
    pub(in crate::apps::write) fn selected_objects(&self) -> Vec<NodeId> {
        let Some((_, _, primary)) = self.selected_object() else {
            return Vec::new();
        };
        let mut out = vec![primary.id];
        let m = &self.office.multi;
        if m.at == Some((self.anchor, self.head)) {
            out.extend(m.extra.iter().copied().filter(|id| *id != primary.id));
        }
        out
    }

    /// Shift+click on an object: adds it to the selected objects, or removes it.
    pub(super) fn select_add(&mut self, id: NodeId) -> Result<()> {
        let current = self.selected_objects();
        if current.is_empty() {
            return self.select_object(id);
        }
        let (_, _, d) = self
            .find_drawing(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no object {id}")))?;
        if matches!(d.placement, Placement::Inline) {
            return Err(KilnError::InvalidInput(
                "only floating objects can be selected together: choose a text wrapping first"
                    .into(),
            ));
        }
        let mut extra: Vec<NodeId> = current[1..].to_vec();
        if current[0] == id {
            // The first object leaves the selection: the next one takes its place.
            let Some(next) = extra.first().copied() else {
                return Ok(());
            };
            extra.remove(0);
            self.select_object(next)?;
        } else if let Some(i) = extra.iter().position(|x| *x == id) {
            extra.remove(i);
        } else {
            extra.push(id);
        }
        self.office.multi = MultiSelection {
            at: Some((self.anchor, self.head)),
            extra,
        };
        Ok(())
    }

    /// Align ▾ with several objects selected and no explicit reference: to each other.
    pub(super) fn align_selected(
        &mut self,
        align: ObjectAlign,
        relative_to: Option<RelativeTo>,
    ) -> Result<()> {
        let ids = self.selected_objects();
        if ids.len() < 2 {
            return self.object_align(align, relative_to);
        }
        let distribute = matches!(
            align,
            ObjectAlign::DistributeHorizontally | ObjectAlign::DistributeVertically
        );
        if relative_to.is_some() && !distribute {
            // Align to Page or Margin with several objects: each of them, in one undo step
            // (distributing spaces them among themselves either way).
            let before = self.states.len();
            let extra = self.office.multi.extra.clone();
            for id in &ids {
                self.select_object(*id)?;
                self.object_align(align, relative_to)?;
            }
            self.group_undo_since(before);
            self.select_object(ids[0])?;
            self.office.multi = MultiSelection {
                at: Some((self.anchor, self.head)),
                extra,
            };
            return Ok(());
        }
        let rects: Vec<(NodeId, Rect)> = {
            self.view_synced();
            ids.iter()
                .filter_map(|id| self.drawing_rect(*id).map(|r| (*id, r)))
                .collect()
        };
        if rects.len() < 2 {
            return Ok(());
        }
        let left = rects.iter().map(|(_, r)| r.x).fold(f32::INFINITY, f32::min);
        let top = rects.iter().map(|(_, r)| r.y).fold(f32::INFINITY, f32::min);
        let right = rects
            .iter()
            .map(|(_, r)| r.x + r.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = rects
            .iter()
            .map(|(_, r)| r.y + r.height)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut moves: Vec<(NodeId, Rect)> = Vec::new();
        match align {
            ObjectAlign::DistributeHorizontally | ObjectAlign::DistributeVertically => {
                let horizontal = align == ObjectAlign::DistributeHorizontally;
                let mut sorted = rects.clone();
                sorted.sort_by(|a, b| {
                    let (ka, kb) = if horizontal {
                        (a.1.x, b.1.x)
                    } else {
                        (a.1.y, b.1.y)
                    };
                    ka.total_cmp(&kb)
                });
                let total: f32 = sorted
                    .iter()
                    .map(|(_, r)| if horizontal { r.width } else { r.height })
                    .sum();
                let span = if horizontal {
                    right - left
                } else {
                    bottom - top
                };
                #[allow(clippy::cast_precision_loss)]
                let gap = (span - total) / (sorted.len() - 1) as f32;
                let mut at = if horizontal { left } else { top };
                for (id, r) in sorted {
                    let mut n = r;
                    if horizontal {
                        n.x = at;
                        at += r.width + gap;
                    } else {
                        n.y = at;
                        at += r.height + gap;
                    }
                    moves.push((id, n));
                }
            }
            _ => {
                for (id, r) in &rects {
                    let mut n = *r;
                    match align {
                        ObjectAlign::Left => n.x = left,
                        ObjectAlign::Center => n.x = (left + right) / 2.0 - r.width / 2.0,
                        ObjectAlign::Right => n.x = right - r.width,
                        ObjectAlign::Top => n.y = top,
                        ObjectAlign::Middle => n.y = (top + bottom) / 2.0 - r.height / 2.0,
                        ObjectAlign::Bottom => n.y = bottom - r.height,
                        _ => {}
                    }
                    moves.push((*id, n));
                }
            }
        }
        let before = self.states.len();
        let extra = self.office.multi.extra.clone();
        let primary = ids[0];
        for (id, r) in moves {
            let old = rects.iter().find(|(i, _)| *i == id).map(|(_, r)| *r);
            if old.is_some_and(|o| (o.x - r.x).abs() < 0.01 && (o.y - r.y).abs() < 0.01) {
                continue;
            }
            self.object_transform(id, r)?;
        }
        // One undo step, and the same objects stay selected.
        self.group_undo_since(before);
        self.select_object(primary)?;
        self.office.multi = MultiSelection {
            at: Some((self.anchor, self.head)),
            extra,
        };
        Ok(())
    }

    /// The Selection pane's eye (every object when `id` is `None`).
    pub(super) fn set_object_visible(&mut self, id: Option<NodeId>, visible: bool) -> Result<()> {
        let ids: Vec<NodeId> = match id {
            Some(i) => vec![i],
            None => self
                .doc
                .body_paragraphs()
                .iter()
                .flat_map(|p| p.content.objects())
                .filter_map(|o| match o {
                    InlineObject::Drawing(d) if d.hidden == visible => Some(d.id),
                    _ => None,
                })
                .collect(),
        };
        let before = self.states.len();
        let sel = (self.anchor, self.head);
        for i in ids {
            self.update_object(Some(i), |_, d, _| {
                d.hidden = !visible;
                Ok(())
            })?;
        }
        self.group_undo_since(before);
        // Hiding an object keeps the selection where it was (the pane lists it still).
        self.anchor = sel.0;
        self.head = sel.1;
        Ok(())
    }
}

/// EMU per CSS pixel.
const EMU_PX: f64 = 9525.0;

#[allow(clippy::cast_possible_truncation)]
fn emu(px: f32) -> i64 {
    (f64::from(px) * EMU_PX).round() as i64
}

impl WriteDoc {
    /// Layout ▸ Arrange ▸ Group: the selected floating objects become one group (`wpg:wgp`) in
    /// their bounding box; Ungroup splits the selected group back into its objects, which stay
    /// selected. One undo step either way.
    pub(super) fn group_objects(&mut self, ungroup: bool) -> Result<()> {
        if ungroup {
            return self.ungroup_selected();
        }
        let ids = self.selected_objects();
        if ids.len() < 2 {
            return Err(KilnError::InvalidInput(
                "select two or more floating objects (Shift+click) to group them".into(),
            ));
        }
        self.view_synced();
        let mut items: Vec<(NodeId, usize, Drawing, Rect)> = Vec::new();
        for id in &ids {
            let (para, off, d) = self
                .find_drawing(*id)
                .ok_or_else(|| KilnError::InvalidInput(format!("no object {id}")))?;
            if matches!(d.placement, Placement::Inline)
                || matches!(d.kind, DrawingKind::Chart { .. } | DrawingKind::Other)
                || (d.kind == DrawingKind::Group && d.group.is_none())
            {
                return Err(KilnError::InvalidInput(
                    "only floating pictures, shapes, text boxes and groups can be grouped".into(),
                ));
            }
            let rect = self
                .drawing_rect(*id)
                .ok_or_else(|| KilnError::InvalidInput("the objects are not laid out".into()))?;
            items.push((para, off, d, rect));
        }
        let left = items.iter().map(|i| i.3.x).fold(f32::INFINITY, f32::min);
        let top = items.iter().map(|i| i.3.y).fold(f32::INFINITY, f32::min);
        let right = items
            .iter()
            .map(|i| i.3.x + i.3.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = items
            .iter()
            .map(|i| i.3.y + i.3.height)
            .fold(f32::NEG_INFINITY, f32::max);
        let bbox = Rect::new(left, top, right - left, bottom - top);
        // Members back to front, as the group draws them.
        let z = |d: &Drawing| match &d.placement {
            Placement::Anchor(a) => a.z_order,
            Placement::Inline => 0,
        };
        let mut by_z: Vec<&(NodeId, usize, Drawing, Rect)> = items.iter().collect();
        by_z.sort_by_key(|i| z(&i.2));
        let members: Vec<GroupMember> = by_z
            .iter()
            .map(|(_, _, d, r)| GroupMember {
                x: emu(r.x - left),
                y: emu(r.y - top),
                // Each member keeps its own placement, for Ungroup to restore its wrapping.
                drawing: d.clone(),
            })
            .collect();
        let (gw, gh) = (emu(bbox.width), emu(bbox.height));
        let top_item = by_z.last().map_or(&items[0], |i| *i);
        let Placement::Anchor(first) = &top_item.2.placement else {
            return Ok(());
        };
        let mut anchor = (**first).clone();
        // The group wraps as its members do (a group of In Front objects stays In Front); when
        // they differ, as the first selected one does.
        let wraps: Vec<(WrapType, Option<WrapSide>)> = items
            .iter()
            .filter_map(|i| match &i.2.placement {
                Placement::Anchor(a) => Some((a.wrap, a.wrap_side)),
                Placement::Inline => None,
            })
            .collect();
        let (wrap, side) = if wraps.windows(2).all(|w| w[0] == w[1]) {
            wraps[0]
        } else {
            match &items[0].2.placement {
                Placement::Anchor(a) => (a.wrap, a.wrap_side),
                Placement::Inline => wraps[0],
            }
        };
        anchor.wrap = wrap;
        anchor.wrap_side = side;
        if !matches!(wrap, WrapType::Tight | WrapType::Through) {
            anchor.wrap_polygon_xml = None;
        }
        let para = items[0].0;
        let at_para = self.anchor_at(&mut anchor, bbox, (bbox.width, bbox.height), para)?;
        let id = self.doc.alloc_id();
        let group = Drawing {
            id,
            kind: DrawingKind::Group,
            width: gw,
            height: gh,
            placement: Placement::Anchor(Box::new(anchor)),
            name: format!("Group {}", id.0),
            description: None,
            title: None,
            decorative: false,
            hidden: false,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            text_box: None,
            xml: None,
            group: Some(Box::new(ShapeGroup {
                child_x: 0,
                child_y: 0,
                child_width: gw,
                child_height: gh,
                members,
            })),
        };
        // Delete back to front so earlier offsets stay valid, then insert the group.
        let mut places: Vec<(NodeId, usize)> = items.iter().map(|i| (i.0, i.1)).collect();
        places.sort_by(|a, b| {
            let pa = self.para_order(a.0);
            let pb = self.para_order(b.0);
            pb.cmp(&pa).then(b.1.cmp(&a.1))
        });
        let mut ops: Vec<Op> = places
            .iter()
            .map(|(p, o)| Op::DeleteInline {
                para: *p,
                offset: *o,
                len: 1,
            })
            .collect();
        let props = self.props_at(Pos::new(at_para, 0));
        ops.push(Op::InsertInline {
            para: at_para,
            offset: 0,
            fragment: Fragment::object(InlineObject::Drawing(Box::new(group)), props),
        });
        let sel = Range::new(Pos::new(at_para, 0), Pos::new(at_para, 1));
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Structure)
                .with_selection(Some(self.selection_range()), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    fn ungroup_selected(&mut self) -> Result<()> {
        let Some((para, off, d)) = self.selected_object() else {
            return Err(KilnError::InvalidInput("select a group first".into()));
        };
        let (Some(g), Placement::Anchor(anchor)) = (&d.group, &d.placement) else {
            return Err(KilnError::InvalidInput(
                "the selected object is not a group".into(),
            ));
        };
        self.view_synced();
        let rect = self
            .drawing_rect(d.id)
            .ok_or_else(|| KilnError::InvalidInput("the group is not laid out".into()))?;
        #[allow(clippy::cast_precision_loss)]
        let (sx, sy) = (
            if g.child_width > 0 {
                d.width as f64 / g.child_width as f64
            } else {
                1.0
            },
            if g.child_height > 0 {
                d.height as f64 / g.child_height as f64
            } else {
                1.0
            },
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let px = |emu: f64| (emu / EMU_PX) as f32;
        let mut ops = vec![Op::DeleteInline {
            para,
            offset: off,
            len: 1,
        }];
        let props = self.props_at(Pos::new(para, off + 1));
        let mut ids = Vec::new();
        for (i, m) in g.members.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let (x, y, w, h) = (
                (m.x - g.child_x) as f64 * sx,
                (m.y - g.child_y) as f64 * sy,
                m.drawing.width as f64 * sx,
                m.drawing.height as f64 * sy,
            );
            let r = Rect::new(rect.x + px(x), rect.y + px(y), px(w), px(h));
            // A member that was grouped in Kiln gets its own wrapping back; one read from a
            // file (Word stores no wrapping for members) takes the group's.
            let mut a = match &m.drawing.placement {
                Placement::Anchor(own) => {
                    let mut own = (**own).clone();
                    own.z_order = anchor.z_order;
                    own
                }
                Placement::Inline => (**anchor).clone(),
            };
            a.z_order += i64::try_from(i).unwrap_or(0);
            let at = self.anchor_at(&mut a, r, (r.width, r.height), para)?;
            #[allow(clippy::cast_possible_truncation)]
            let member = Drawing {
                width: w.round() as i64,
                height: h.round() as i64,
                placement: Placement::Anchor(Box::new(a)),
                ..m.drawing.clone()
            };
            ids.push(member.id);
            // Members anchor in the group's paragraph when the layout puts them there.
            let at = if at == para { para } else { at };
            ops.push(Op::InsertInline {
                para: at,
                offset: if at == para { off } else { 0 },
                fragment: Fragment::object(InlineObject::Drawing(Box::new(member)), props.clone()),
            });
        }
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Structure)
                .with_selection(Some(self.selection_range()), None),
        );
        self.features.untracked = false;
        r?;
        if let Some(first) = ids.first() {
            self.select_object(*first)?;
            self.office.multi = MultiSelection {
                at: Some((self.anchor, self.head)),
                extra: ids[1..].to_vec(),
            };
        }
        Ok(())
    }

    /// A paragraph's place in the document, for ordering edits.
    fn para_order(&self, id: NodeId) -> usize {
        self.doc
            .all_paragraphs()
            .iter()
            .position(|(_, p)| p.id == id)
            .unwrap_or(usize::MAX)
    }
}
