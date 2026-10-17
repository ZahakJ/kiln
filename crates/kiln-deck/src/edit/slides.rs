// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slide commands: New Slide, Duplicate, Delete, reorder (the rail's drag and Move to Section),
//! Hide Slide, Layout, Reset, Format Background, Slide Size, notes.

use std::collections::{HashMap, HashSet};

use kiln_common::NodeId;
use kiln_doc::{Op as DocOp, ParagraphProps, RunProps, RunSpan, Transaction as DocTransaction};

use super::{slide_indices, tx};
use crate::error::{DeckError, Result};
use crate::geom::{Transform, round_emu};
use crate::inherit::match_layout_placeholder;
use crate::ops::{EditKind, Op, TextTarget, Transaction};
use crate::presentation::Presentation;
use crate::shape::{AutoShape, Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps};
use crate::slide::{Background, Layout, Notes, Section, Slide, SlideSize};
use crate::text::{BodyProps, ListStyle, TextBody};

fn placeholder_display(kind: PlaceholderType) -> &'static str {
    match kind {
        PlaceholderType::Title | PlaceholderType::CenteredTitle => "Title",
        PlaceholderType::Subtitle => "Subtitle",
        PlaceholderType::Body => "Text Placeholder",
        PlaceholderType::Picture => "Picture Placeholder",
        PlaceholderType::Chart => "Chart Placeholder",
        PlaceholderType::Table => "Table Placeholder",
        PlaceholderType::Media => "Media Placeholder",
        PlaceholderType::Date => "Date Placeholder",
        PlaceholderType::Footer => "Footer Placeholder",
        PlaceholderType::SlideNumber => "Slide Number Placeholder",
        PlaceholderType::Header => "Header Placeholder",
        _ => "Content Placeholder",
    }
}

/// An empty slide placeholder matching a layout placeholder: no position (inherited), empty
/// text.
fn empty_placeholder(pres: &mut Presentation, ph: Placeholder) -> Shape {
    let id = pres.alloc_id();
    let mut s = Shape::new(
        id,
        format!("{} {}", placeholder_display(ph.kind), id.0),
        None,
        ShapeKind::Geometry(AutoShape {
            text_box: false,
            props: ShapeProps::default(),
            text: ph.kind.holds_text().then(TextBody::new),
        }),
    );
    s.placeholder = Some(ph);
    s
}

/// A new slide on a layout with an empty placeholder for each of the layout's content
/// placeholders (PowerPoint's New Slide). The slide is not inserted.
#[must_use]
pub fn new_slide(pres: &mut Presentation, layout: NodeId) -> Slide {
    let phs: Vec<Placeholder> = pres
        .layout(layout)
        .map(|(_, l)| {
            Presentation::new_slide_placeholders(l)
                .iter()
                .filter_map(|s| s.placeholder)
                .collect()
        })
        .unwrap_or_default();
    let shapes = phs
        .into_iter()
        .map(|p| empty_placeholder(pres, p))
        .collect();
    let mut s = Slide::new(pres.alloc_id(), layout);
    s.shapes = shapes;
    s
}

/// The sections after inserting `count` slides at `index`: they join the section of the slide
/// before (or the first section at the start).
fn sections_after_insert(pres: &Presentation, index: usize, count: usize) -> Option<Vec<Section>> {
    if pres.sections.is_empty() {
        return None;
    }
    let target = if index == 0 {
        // The first section, even if empty.
        0
    } else {
        pres.section_of_index(index - 1).unwrap_or(0)
    };
    let mut s = pres.sections.clone();
    s[target].len += count;
    Some(s)
}

/// New Slide: inserts a slide on a layout at an index (in the section of the slide before it).
///
/// # Errors
/// An unknown layout or an index past the end.
pub fn insert_slide(pres: &mut Presentation, layout: NodeId, index: usize) -> Result<Transaction> {
    if pres.layout(layout).is_none() {
        return Err(DeckError::UnknownNode(layout));
    }
    if index > pres.slides.len() {
        return Err(DeckError::OutOfRange {
            index,
            len: pres.slides.len(),
        });
    }
    let slide = new_slide(pres, layout);
    let id = slide.id;
    let mut ops = vec![Op::InsertSlide {
        index,
        slide: Box::new(slide),
    }];
    if let Some(sections) = sections_after_insert(pres, index, 1) {
        ops.push(Op::SetSections { sections });
    }
    Ok(tx(ops, EditKind::Structure, vec![id]))
}

/// A deep copy of a slide with fresh ids for the slide, its shapes, effects and comments, and
/// every reference between them rewritten.
pub(crate) fn copy_slide(pres: &mut Presentation, s: &Slide) -> Slide {
    let mut map: HashMap<NodeId, NodeId> = HashMap::new();
    let mut out = s.clone();
    out.id = pres.alloc_id();
    fn remap_shapes(
        pres: &mut Presentation,
        shapes: &mut [Shape],
        map: &mut HashMap<NodeId, NodeId>,
    ) {
        for sh in shapes {
            let new = pres.alloc_id();
            map.insert(sh.id, new);
            sh.id = new;
            if let Some(c) = sh.children_mut() {
                remap_shapes(pres, c, map);
            }
        }
    }
    remap_shapes(pres, &mut out.shapes, &mut map);
    if let Some(n) = out.notes.as_mut() {
        remap_shapes(pres, &mut n.shapes, &mut map);
    }
    fn fix_refs(shapes: &mut [Shape], map: &HashMap<NodeId, NodeId>) {
        for sh in shapes {
            if let ShapeKind::Connector(c) = &mut sh.kind {
                for end in [&mut c.start, &mut c.end].into_iter().flatten() {
                    if let Some(n) = map.get(&end.shape) {
                        end.shape = *n;
                    }
                }
            }
            if let Some(c) = sh.children_mut() {
                fix_refs(c, map);
            }
        }
    }
    fix_refs(&mut out.shapes, &map);
    let remap = |id: &mut NodeId| {
        if let Some(n) = map.get(id) {
            *id = *n;
        }
    };
    for e in out.timeline.sequence.iter_mut().chain(
        out.timeline
            .triggers
            .iter_mut()
            .flat_map(|t| t.effects.iter_mut()),
    ) {
        e.id = pres.alloc_id();
        remap(&mut e.target.shape);
    }
    for t in &mut out.timeline.triggers {
        remap(&mut t.trigger);
    }
    for c in &mut out.comments {
        c.id = pres.alloc_id();
        if let Some(s) = c.shape.as_mut() {
            remap(s);
        }
        for r in &mut c.replies {
            r.id = pres.alloc_id();
        }
    }
    out
}

/// Duplicate Slide (Ctrl+D in the rail): copies of the slides, in order, after the last of them,
/// in its section.
///
/// # Errors
/// Unknown slides.
pub fn duplicate_slides(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let Some(&last) = idx.last() else {
        return Ok(Transaction::default());
    };
    let mut ops = Vec::new();
    let mut new_ids = Vec::new();
    for (k, i) in idx.iter().enumerate() {
        let src = pres.slides[*i].clone();
        let copy = copy_slide(pres, &src);
        new_ids.push(copy.id);
        ops.push(Op::InsertSlide {
            index: last + 1 + k,
            slide: Box::new(copy),
        });
    }
    if let Some(sections) = sections_after_insert(pres, last + 1, idx.len()) {
        ops.push(Op::SetSections { sections });
    }
    Ok(tx(ops, EditKind::Structure, new_ids))
}

/// Delete Slide. Sections keep their place (and may become empty, as in PowerPoint).
///
/// # Errors
/// Unknown slides.
pub fn delete_slides(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let mut sections = pres.sections.clone();
    for i in &idx {
        if let Some(s) = pres.section_of_index(*i) {
            sections[s].len -= 1;
        }
    }
    let mut ops: Vec<Op> = idx
        .iter()
        .rev()
        .map(|i| Op::RemoveSlide { index: *i })
        .collect();
    if !pres.sections.is_empty() {
        ops.push(Op::SetSections { sections });
    }
    // Select the slide that takes the first deleted one's place.
    let after = idx
        .first()
        .and_then(|first| {
            let remaining: Vec<NodeId> = pres
                .slides
                .iter()
                .enumerate()
                .filter(|(i, _)| !idx.contains(i))
                .map(|(_, s)| s.id)
                .collect();
            let before = idx.iter().filter(|i| *i < first).count();
            remaining
                .get((*first - before).min(remaining.len().saturating_sub(1)))
                .copied()
        })
        .into_iter()
        .collect();
    Ok(tx(ops, EditKind::Structure, after))
}

/// The ops that reorder the slides into `order` with each slide in `section_of` (section index).
pub(crate) fn reorder(
    pres: &Presentation,
    order: &[NodeId],
    section_of: &HashMap<NodeId, usize>,
) -> Vec<Op> {
    let mut work: Vec<NodeId> = pres.slides.iter().map(|s| s.id).collect();
    let mut ops = Vec::new();
    for (i, id) in order.iter().enumerate() {
        if let Some(pos) = work.iter().position(|x| x == id)
            && pos != i
        {
            let s = work.remove(pos);
            work.insert(i, s);
            ops.push(Op::MoveSlide { from: pos, to: i });
        }
    }
    if !pres.sections.is_empty() {
        let mut sections = pres.sections.clone();
        for s in &mut sections {
            s.len = 0;
        }
        for id in order {
            if let Some(s) = section_of.get(id).and_then(|i| sections.get_mut(*i)) {
                s.len += 1;
            }
        }
        if sections != pres.sections {
            ops.push(Op::SetSections { sections });
        }
    }
    ops
}

fn current_sections(pres: &Presentation) -> HashMap<NodeId, usize> {
    pres.slides
        .iter()
        .enumerate()
        .filter_map(|(i, s)| pres.section_of_index(i).map(|sec| (s.id, sec)))
        .collect()
}

/// Moves slides (keeping their order) so they come before the slide now at index `to`
/// (`to == len`: at the end). They join the section of the slide before them.
///
/// # Errors
/// Unknown slides or an index past the end.
pub fn move_slides(pres: &mut Presentation, ids: &[NodeId], to: usize) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    if to > pres.slides.len() {
        return Err(DeckError::OutOfRange {
            index: to,
            len: pres.slides.len(),
        });
    }
    let moving: HashSet<NodeId> = idx.iter().map(|i| pres.slides[*i].id).collect();
    let others: Vec<NodeId> = pres
        .slides
        .iter()
        .map(|s| s.id)
        .filter(|id| !moving.contains(id))
        .collect();
    let k = pres.slides[..to]
        .iter()
        .filter(|s| !moving.contains(&s.id))
        .count();
    let moved: Vec<NodeId> = idx.iter().map(|i| pres.slides[*i].id).collect();
    let mut order = others[..k].to_vec();
    order.extend(&moved);
    order.extend(&others[k..]);
    let mut section_of = current_sections(pres);
    if !pres.sections.is_empty() {
        let target = match k {
            0 => others
                .first()
                .and_then(|o| section_of.get(o).copied())
                .unwrap_or(0),
            _ => section_of.get(&others[k - 1]).copied().unwrap_or(0),
        };
        for id in &moved {
            section_of.insert(*id, target);
        }
    }
    Ok(tx(
        reorder(pres, &order, &section_of),
        EditKind::Structure,
        moved,
    ))
}

/// Move to Section: moves slides to the end of a section.
///
/// # Errors
/// Unknown slides or section.
pub fn move_slides_to_section(
    pres: &mut Presentation,
    ids: &[NodeId],
    section: NodeId,
) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let target = pres
        .sections
        .iter()
        .position(|s| s.id == section)
        .ok_or(DeckError::UnknownNode(section))?;
    let moved: Vec<NodeId> = idx.iter().map(|i| pres.slides[*i].id).collect();
    let mut section_of = current_sections(pres);
    for id in &moved {
        section_of.insert(*id, target);
    }
    // Stable order: by section, then by current position, with the moved slides last in theirs.
    let pos: HashMap<NodeId, usize> = pres
        .slides
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id, i))
        .collect();
    let mut order: Vec<NodeId> = pres.slides.iter().map(|s| s.id).collect();
    order.sort_by_key(|id| {
        (
            section_of.get(id).copied().unwrap_or(0),
            moved.contains(id),
            pos[id],
        )
    });
    Ok(tx(
        reorder(pres, &order, &section_of),
        EditKind::Structure,
        moved,
    ))
}

/// Hide Slide.
///
/// # Errors
/// Unknown slides.
pub fn set_hidden(pres: &mut Presentation, ids: &[NodeId], hidden: bool) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let ops = idx
        .iter()
        .map(|i| &pres.slides[*i])
        .filter(|s| s.hidden != hidden)
        .map(|s| Op::SetSlideHidden {
            slide: s.id,
            hidden,
        })
        .collect();
    Ok(tx(ops, EditKind::Structure, ids.to_vec()))
}

/// Format Background (`Some`) or Reset Background (`None`) for slides.
///
/// # Errors
/// Unknown slides.
pub fn set_background(
    pres: &mut Presentation,
    ids: &[NodeId],
    background: Option<Background>,
) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let ops = idx
        .iter()
        .map(|i| &pres.slides[*i])
        .filter(|s| s.background != background)
        .map(|s| Op::SetBackground {
            owner: s.id,
            background: background.clone(),
        })
        .collect();
    Ok(tx(ops, EditKind::Format, ids.to_vec()))
}

/// Replaces a slide's speaker notes with plain text (kept shapes of the notes page stay).
///
/// # Errors
/// An unknown slide.
pub fn set_notes_text(pres: &mut Presentation, slide: NodeId, text: &str) -> Result<Transaction> {
    let s = pres.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
    let shapes = s
        .notes
        .as_ref()
        .map(|n| n.shapes.clone())
        .unwrap_or_default();
    let notes = if text.is_empty() && shapes.is_empty() {
        None
    } else {
        Some(Box::new(Notes {
            text: TextBody::from_text(text),
            shapes,
        }))
    };
    Ok(tx(
        vec![Op::SetNotes { slide, notes }],
        EditKind::Other,
        vec![slide],
    ))
}

fn is_empty_placeholder(s: &Shape) -> bool {
    match &s.kind {
        ShapeKind::Geometry(a) => a.text.as_ref().is_none_or(TextBody::is_empty),
        _ => false,
    }
}

fn delete_ops(pres: &Presentation, slide: &Slide, shape_ids: &[NodeId]) -> Vec<Op> {
    let gone: HashSet<NodeId> = shape_ids.iter().copied().collect();
    let mut ops: Vec<Op> = slide
        .timeline
        .sequence
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, e)| gone.contains(&e.target.shape))
        .map(|(index, _)| Op::RemoveEffect {
            slide: slide.id,
            index,
        })
        .collect();
    let _ = pres;
    let mut idx: Vec<usize> = slide
        .shapes
        .iter()
        .enumerate()
        .filter(|(_, s)| gone.contains(&s.id))
        .map(|(i, _)| i)
        .collect();
    idx.sort_unstable();
    ops.extend(idx.into_iter().rev().map(|index| Op::RemoveShapes {
        parent: slide.id,
        index,
        count: 1,
    }));
    ops
}

/// Layout: puts slides on another layout. Placeholders are matched to the new layout's by type
/// and index and move to its positions; empty placeholders without a match go, placeholders
/// with content and no match stay where they are; the new layout's other placeholders are added
/// empty.
///
/// # Errors
/// Unknown slides or layout.
pub fn change_layout(
    pres: &mut Presentation,
    ids: &[NodeId],
    layout: NodeId,
) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let new_layout: Layout = pres
        .layout(layout)
        .ok_or(DeckError::UnknownNode(layout))?
        .1
        .clone();
    let content: Vec<Placeholder> = Presentation::new_slide_placeholders(&new_layout)
        .iter()
        .filter_map(|s| s.placeholder)
        .collect();
    let mut ops = Vec::new();
    for i in idx {
        let slide = pres.slides[i].clone();
        if slide.layout == layout {
            continue;
        }
        let mut taken = vec![false; content.len()];
        let mut remove = Vec::new();
        let mut later = Vec::new();
        for s in slide.shapes.iter().filter(|s| s.placeholder.is_some()) {
            let Some(ph) = s.placeholder else { continue };
            if ph.kind.is_footer_area() {
                continue;
            }
            let free = |pred: &dyn Fn(&Placeholder) -> bool, taken: &[bool]| {
                content
                    .iter()
                    .enumerate()
                    .find(|(k, p)| !taken[*k] && pred(p))
                    .map(|(k, _)| k)
            };
            let found = if ph.kind.is_title() {
                free(&|p| p.kind.is_title(), &taken)
            } else {
                free(&|p| p.kind == ph.kind && p.idx == ph.idx, &taken)
                    .or_else(|| free(&|p| p.kind == ph.kind, &taken))
                    .or_else(|| {
                        free(
                            &|p| {
                                !p.kind.is_title()
                                    && p.kind.master_type() == ph.kind.master_type()
                                    && p.kind != PlaceholderType::Picture
                            },
                            &taken,
                        )
                    })
            };
            match found {
                Some(k) => {
                    taken[k] = true;
                    if content[k] != ph {
                        later.push(Op::SetPlaceholder {
                            shape: s.id,
                            placeholder: Some(content[k]),
                        });
                    }
                    if s.xfrm.is_some() {
                        later.push(Op::SetTransform {
                            shape: s.id,
                            xfrm: None,
                        });
                    }
                }
                None if is_empty_placeholder(s) => remove.push(s.id),
                None => {
                    if s.xfrm.is_none()
                        && let Some(x) = pres.resolved_xfrm(s.id)
                    {
                        later.push(Op::SetTransform {
                            shape: s.id,
                            xfrm: Some(x),
                        });
                    }
                }
            }
        }
        ops.extend(delete_ops(pres, &slide, &remove));
        ops.extend(later);
        ops.push(Op::SetSlideLayout {
            slide: slide.id,
            layout,
        });
        let added: Vec<Shape> = content
            .iter()
            .zip(&taken)
            .filter(|(_, t)| !**t)
            .map(|(p, _)| empty_placeholder(pres, *p))
            .collect();
        if !added.is_empty() {
            let index = slide.shapes.len() - remove.len();
            ops.push(Op::InsertShapes {
                parent: slide.id,
                index,
                shapes: added,
            });
        }
    }
    Ok(tx(ops, EditKind::Structure, ids.to_vec()))
}

/// The kiln-doc ops that remove a body's direct formatting (runs, paragraph properties except
/// the list level, paragraph mark).
fn clear_direct_formatting(body: &TextBody) -> DocTransaction {
    let mut ops = Vec::new();
    for p in body.paragraphs() {
        let len = p.content.len();
        if len > 0 && p.content.runs().iter().any(|r| !r.props.is_empty()) {
            ops.push(DocOp::SetRuns {
                para: p.id,
                offset: 0,
                runs: vec![RunSpan::new(len, RunProps::default())],
            });
        }
        let keep = ParagraphProps {
            outline_level: p.props.outline_level,
            ..ParagraphProps::default()
        };
        if p.props != keep {
            ops.push(DocOp::SetParagraphProps {
                para: p.id,
                props: Box::new(keep),
            });
        }
        if !p.mark_props.is_empty() {
            ops.push(DocOp::SetParagraphMarkProps {
                para: p.id,
                props: Box::new(RunProps::default()),
            });
        }
    }
    DocTransaction::new(ops)
}

/// Reset: placeholders return to the layout's position, size and formatting (text stays, its
/// direct formatting goes), and missing layout placeholders come back empty.
///
/// # Errors
/// Unknown slides.
pub fn reset_slides(pres: &mut Presentation, ids: &[NodeId]) -> Result<Transaction> {
    let idx = slide_indices(pres, ids)?;
    let mut ops = Vec::new();
    for i in idx {
        let slide = pres.slides[i].clone();
        let Some((_, layout)) = pres.layout(slide.layout) else {
            continue;
        };
        let layout = layout.clone();
        for s in slide.shapes.iter().filter(|s| s.placeholder.is_some()) {
            if s.xfrm.is_some() {
                ops.push(Op::SetTransform {
                    shape: s.id,
                    xfrm: None,
                });
            }
            if s.props().is_some_and(|p| !p.is_empty()) {
                ops.push(Op::SetShapeProps {
                    shape: s.id,
                    props: Box::default(),
                });
            }
            if let Some(t) = s.text() {
                let target = TextTarget::Shape(s.id);
                if !t.body.is_empty() {
                    ops.push(Op::SetBodyProps {
                        target,
                        body: Box::new(BodyProps::default()),
                    });
                }
                if !t.list_style.is_empty() {
                    ops.push(Op::SetListStyle {
                        target,
                        style: Box::new(ListStyle::default()),
                    });
                }
                let doc_tx = clear_direct_formatting(t);
                if !doc_tx.is_empty() || !t.all_deck_props().is_empty() {
                    ops.push(Op::EditText {
                        target,
                        tx: doc_tx,
                        para_props: Some(Default::default()),
                    });
                }
            }
        }
        // Layout placeholders the slide lost.
        let present: Vec<Placeholder> = slide.shapes.iter().filter_map(|s| s.placeholder).collect();
        let missing: Vec<Placeholder> = Presentation::new_slide_placeholders(&layout)
            .iter()
            .filter_map(|l| l.placeholder)
            .filter(|lp| {
                !present.iter().any(|p| {
                    match_layout_placeholder(std::slice::from_ref(&placeholder_probe(*lp)), p)
                        .is_some()
                })
            })
            .collect();
        if !missing.is_empty() {
            let shapes = missing
                .into_iter()
                .map(|p| empty_placeholder(pres, p))
                .collect();
            ops.push(Op::InsertShapes {
                parent: slide.id,
                index: slide.shapes.len(),
                shapes,
            });
        }
    }
    Ok(tx(ops, EditKind::Format, ids.to_vec()))
}

/// A throwaway shape carrying a placeholder, for matching.
fn placeholder_probe(p: Placeholder) -> Shape {
    let mut s = Shape::new(
        NodeId(0),
        "",
        None,
        ShapeKind::Geometry(AutoShape::default()),
    );
    s.placeholder = Some(p);
    s
}

/// How content follows a slide size change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleMode {
    /// Leave shapes where they are.
    DontScale,
    /// Scale content up as far as it goes (it may run off the slide): PowerPoint's Maximize.
    Maximize,
    /// Scale content down so it fits: PowerPoint's Ensure Fit.
    EnsureFit,
}

fn scale_xfrm(x: &Transform, sx: f64, sy: f64, ox: f64, oy: f64) -> Transform {
    Transform {
        x: round_emu(x.x as f64 * sx + ox),
        y: round_emu(x.y as f64 * sy + oy),
        cx: round_emu(x.cx as f64 * sx),
        cy: round_emu(x.cy as f64 * sy),
        ..*x
    }
    .clamped()
}

/// Slide Size: changes the size; masters and layouts stretch to the new size, slide content
/// follows `mode` (uniformly scaled and centred).
///
/// # Errors
/// A non-positive size.
pub fn set_slide_size(
    pres: &mut Presentation,
    size: SlideSize,
    mode: ScaleMode,
) -> Result<Transaction> {
    if !size.is_valid() {
        return Err(DeckError::InvalidOp(
            "the slide size is outside 1 EMU to 56 inches".into(),
        ));
    }
    let old = pres.slide_size.clone();
    let sx = size.cx as f64 / old.cx as f64;
    let sy = size.cy as f64 / old.cy as f64;
    let mut ops = vec![Op::SetSlideSize { size: size.clone() }];
    let scale_list = |shapes: &[Shape], sx: f64, sy: f64, ox: f64, oy: f64, ops: &mut Vec<Op>| {
        for s in shapes {
            if let Some(x) = &s.xfrm {
                let n = scale_xfrm(x, sx, sy, ox, oy);
                if n != *x {
                    ops.push(Op::SetTransform {
                        shape: s.id,
                        xfrm: Some(n),
                    });
                }
            }
        }
    };
    if (sx - 1.0).abs() > f64::EPSILON || (sy - 1.0).abs() > f64::EPSILON {
        for m in &pres.masters {
            scale_list(&m.shapes, sx, sy, 0.0, 0.0, &mut ops);
            for l in &m.layouts {
                scale_list(&l.shapes, sx, sy, 0.0, 0.0, &mut ops);
            }
        }
        let k = match mode {
            ScaleMode::DontScale => None,
            ScaleMode::Maximize => Some(sx.max(sy)),
            ScaleMode::EnsureFit => Some(sx.min(sy)),
        };
        if let Some(k) = k {
            let ox = (size.cx as f64 - old.cx as f64 * k) / 2.0;
            let oy = (size.cy as f64 - old.cy as f64 * k) / 2.0;
            for s in &pres.slides {
                scale_list(&s.shapes, k, k, ox, oy, &mut ops);
            }
        }
    }
    Ok(tx(ops, EditKind::Structure, Vec::new()))
}
