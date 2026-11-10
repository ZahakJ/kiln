// SPDX-License-Identifier: AGPL-3.0-or-later
//! Random concurrent edits on two and three replicas of a presentation, exchanged in random
//! orders, converge to one valid presentation equal to the shared state, whatever the order in
//! which a replica merges the others' updates; every slide and text shape someone made and
//! nobody deleted is in it once, and nothing deleted came back (except a shape someone grouped
//! at the same moment, which stays).

use kiln_collab::{Crdt, DeckBinding, Result};
use kiln_common::NodeId;
use kiln_deck::{Presentation, Rect, Shape, ShapeKind, TextTarget, Transaction, edit};
use kiln_doc::{IdLanes, Pos};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Edit {
    AddBox {
        slide: usize,
        text: String,
    },
    Type {
        shape: usize,
        at: usize,
        text: String,
    },
    Erase {
        shape: usize,
        at: usize,
        len: usize,
    },
    Split {
        shape: usize,
        at: usize,
    },
    Join {
        shape: usize,
    },
    Nudge {
        shape: usize,
        dx: i64,
    },
    DeleteShape {
        shape: usize,
    },
    AddSlide {
        at: usize,
    },
    DeleteSlide {
        slide: usize,
    },
    MoveSlide {
        slide: usize,
        to: usize,
    },
    Hide {
        slide: usize,
        hidden: bool,
    },
    Section {
        at: usize,
        name: String,
    },
    Notes {
        slide: usize,
        text: String,
    },
    Group {
        shape: usize,
    },
    Animate {
        shape: usize,
    },
}

fn edit_strategy() -> impl Strategy<Value = Edit> {
    let i = 0..6usize;
    prop_oneof![
        3 => (i.clone(), "[a-z]{1,5}").prop_map(|(slide, text)| Edit::AddBox { slide, text }),
        4 => (i.clone(), 0..20usize, "[a-z ]{1,5}").prop_map(|(shape, at, text)| Edit::Type { shape, at, text }),
        2 => (i.clone(), 0..20usize, 1..4usize).prop_map(|(shape, at, len)| Edit::Erase { shape, at, len }),
        2 => (i.clone(), 0..20usize).prop_map(|(shape, at)| Edit::Split { shape, at }),
        1 => i.clone().prop_map(|shape| Edit::Join { shape }),
        2 => (i.clone(), -5000..5000i64).prop_map(|(shape, dx)| Edit::Nudge { shape, dx }),
        1 => i.clone().prop_map(|shape| Edit::DeleteShape { shape }),
        2 => i.clone().prop_map(|at| Edit::AddSlide { at }),
        1 => i.clone().prop_map(|slide| Edit::DeleteSlide { slide }),
        1 => (i.clone(), i.clone()).prop_map(|(slide, to)| Edit::MoveSlide { slide, to }),
        1 => (i.clone(), any::<bool>()).prop_map(|(slide, hidden)| Edit::Hide { slide, hidden }),
        1 => (i.clone(), "[A-Z][a-z]{1,4}").prop_map(|(at, name)| Edit::Section { at, name }),
        1 => (i.clone(), "[a-z ]{0,8}").prop_map(|(slide, text)| Edit::Notes { slide, text }),
        1 => i.clone().prop_map(|shape| Edit::Group { shape }),
        2 => i.clone().prop_map(|shape| Edit::Animate { shape }),
    ]
}

/// Text boxes on all slides, in order.
fn boxes(pres: &Presentation) -> Vec<(NodeId, NodeId, usize)> {
    let mut out = Vec::new();
    for s in &pres.slides {
        for sh in &s.shapes {
            if let ShapeKind::Geometry(g) = &sh.kind
                && let Some(p) = g.text.as_ref().and_then(|t| t.paragraphs().next())
            {
                out.push((sh.id, p.id, p.content.len()));
            }
        }
    }
    out
}

fn build(pres: &mut Presentation, e: &Edit) -> Option<Transaction> {
    let slides: Vec<NodeId> = pres.slides.iter().map(|s| s.id).collect();
    if slides.is_empty() {
        return None;
    }
    let slide = |i: usize| slides[i % slides.len()];
    let shapes = boxes(pres);
    let shape = |i: usize| (!shapes.is_empty()).then(|| shapes[i % shapes.len()]);
    let tx = match e {
        Edit::AddBox { slide: s, text } => {
            let id = pres.alloc_id();
            let rect = Rect {
                x: 0,
                y: 0,
                cx: 1_000_000,
                cy: 400_000,
            };
            edit::insert_shapes(pres, slide(*s), vec![Shape::text_box(id, rect, text)], None)
        }
        Edit::Type { shape: i, at, text } => {
            let (id, para, len) = shape(*i)?;
            edit::insert_text(
                pres,
                TextTarget::Shape(id),
                Pos::new(para, at % (len + 1)),
                text,
            )
        }
        Edit::Erase {
            shape: i,
            at,
            len: n,
        } => {
            let (id, para, len) = shape(*i)?;
            let s = at % (len + 1);
            let end = (s + n).min(len);
            edit::edit_text(pres, TextTarget::Shape(id), |d| {
                kiln_doc::edit::delete_range(d, Pos::new(para, s), Pos::new(para, end))
            })
        }
        Edit::Split { shape: i, at } => {
            let (id, para, len) = shape(*i)?;
            edit::edit_text(pres, TextTarget::Shape(id), |d| {
                kiln_doc::edit::split_paragraph(d, Pos::new(para, at % (len + 1)))
            })
        }
        Edit::Join { shape: i } => {
            let (id, para, len) = shape(*i)?;
            let next = pres.shape(id).and_then(|s| match &s.kind {
                ShapeKind::Geometry(g) => g.text.as_ref()?.paragraphs().nth(1).map(|p| p.id),
                _ => None,
            })?;
            edit::edit_text(pres, TextTarget::Shape(id), |d| {
                kiln_doc::edit::delete_range(d, Pos::new(para, len), Pos::new(next, 0))
            })
        }
        Edit::Nudge { shape: i, dx } => {
            let (id, _, _) = shape(*i)?;
            edit::move_shapes(pres, &[id], *dx, 0)
        }
        Edit::DeleteShape { shape: i } => {
            let (id, _, _) = shape(*i)?;
            edit::delete_shapes(pres, &[id])
        }
        Edit::AddSlide { at } => {
            let layout = pres.slides[0].layout;
            let at = at % (slides.len() + 1);
            edit::insert_slide(pres, layout, at)
        }
        Edit::DeleteSlide { slide: s } => {
            if slides.len() < 2 {
                return None;
            }
            edit::delete_slides(pres, &[slide(*s)])
        }
        Edit::MoveSlide { slide: s, to } => {
            edit::move_slides(pres, &[slide(*s)], to % (slides.len() + 1))
        }
        Edit::Hide { slide: s, hidden } => edit::set_hidden(pres, &[slide(*s)], *hidden),
        Edit::Section { at, name } => edit::add_section(pres, at % (slides.len() + 1), name),
        Edit::Notes { slide: s, text } => edit::set_notes_text(pres, slide(*s), text),
        Edit::Group { shape: i } => {
            // Two text boxes on the same slide.
            let (a, _, _) = shape(*i)?;
            let on = pres.slide_of_shape(a)?;
            let other = on.shapes.iter().find(|s| s.id != a && s.xfrm.is_some())?.id;
            edit::group(pres, &[a, other])
        }
        Edit::Animate { shape: i } => {
            let (id, _, _) = shape(*i)?;
            edit::add_effect(pres, id, kiln_deck::AnimationEffect::Fade, None)
        }
    };
    tx.ok().filter(|t| !t.is_empty())
}

struct Replica {
    binding: DeckBinding,
    pres: Presentation,
}

impl Replica {
    fn edit(&mut self, e: &Edit) -> Result<()> {
        let mut scratch = self.pres.clone();
        let Some(tx) = build(&mut scratch, e) else {
            return Ok(());
        };
        if scratch.apply(tx.clone()).is_err() {
            return Ok(());
        }
        self.pres = scratch;
        self.binding.commit(&self.pres, &tx)
    }

    fn receive(&mut self, from: &Replica) -> Result<()> {
        let update = from
            .binding
            .crdt()
            .export_from(&self.binding.crdt().version())?;
        self.binding.apply_remote(&mut self.pres, &update)?;
        Ok(())
    }
}

fn replicas(n: u64) -> Result<Vec<Replica>> {
    let mut pres = Presentation::new();
    let first = DeckBinding::share(&mut pres, 100, IdLanes::new(0, 8))?;
    let snapshot = first.crdt().snapshot()?;
    let mut out = vec![Replica {
        binding: first,
        pres,
    }];
    for i in 1..n {
        let (binding, pres) = DeckBinding::join(&snapshot, 100 + i, IdLanes::new(i, 8))?;
        out.push(Replica { binding, pres });
    }
    Ok(out)
}

fn exchange(rs: &mut [Replica], to: usize, from: usize) -> Result<()> {
    if to == from {
        return Ok(());
    }
    let (a, b) = if to < from {
        let (l, r) = rs.split_at_mut(from);
        (&mut l[to], &r[0])
    } else {
        let (l, r) = rs.split_at_mut(to);
        (&mut r[0], &l[from])
    };
    a.receive(b)
}

/// The slides and the text shapes (at any depth) of a presentation, each as often as it occurs,
/// with the slide each is on.
fn made(pres: &Presentation) -> Vec<(NodeId, NodeId)> {
    fn walk(slide: NodeId, shapes: &[Shape], out: &mut Vec<(NodeId, NodeId)>) {
        for sh in shapes {
            match &sh.kind {
                ShapeKind::Geometry(_) => out.push((sh.id, slide)),
                ShapeKind::Group(g) => walk(slide, &g.children, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for s in &pres.slides {
        out.push((s.id, s.id));
        walk(s.id, &s.shapes, &mut out);
    }
    out
}

fn run(
    n: usize,
    rounds: Vec<Vec<(usize, Edit)>>,
    syncs: Vec<(usize, usize)>,
) -> std::result::Result<(), TestCaseError> {
    use std::collections::{BTreeMap, BTreeSet};
    let fail = |e: kiln_collab::CollabError| TestCaseError::fail(e.to_string());
    let mut rs = replicas(n as u64).map_err(fail)?;
    let mut alive: BTreeMap<NodeId, NodeId> = made(&rs[0].pres).into_iter().collect();
    let mut deleted: BTreeSet<NodeId> = BTreeSet::new();
    let mut moved: BTreeSet<NodeId> = BTreeSet::new();
    for (round, edits) in rounds.iter().enumerate() {
        for (who, e) in edits {
            let before: BTreeMap<NodeId, NodeId> = made(&rs[who % n].pres).into_iter().collect();
            if matches!(e, Edit::Group { .. }) {
                moved.extend(
                    before
                        .keys()
                        .filter(|id| !before.values().any(|s| s == *id)),
                );
            }
            rs[who % n].edit(e).map_err(fail)?;
            let after: BTreeMap<NodeId, NodeId> = made(&rs[who % n].pres).into_iter().collect();
            for (id, slide) in &after {
                alive.entry(*id).or_insert(*slide);
            }
            deleted.extend(before.keys().filter(|id| !after.contains_key(id)));
        }
        for (to, from) in syncs.iter().skip(round * 3).take(3) {
            exchange(&mut rs, to % n, from % n).map_err(fail)?;
        }
    }
    // Everyone gets everything, until no replica has anything another lacks (merging can add a
    // change of its own: a repair, or text moved after a split or join).
    for _ in 0..8 {
        for to in 0..n {
            for from in 0..n {
                exchange(&mut rs, to, from).map_err(fail)?;
            }
        }
        let v = rs[0].binding.crdt().version();
        if rs.iter().all(|r| r.binding.crdt().version() == v) {
            break;
        }
    }
    for r in &rs {
        r.pres
            .validate()
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let shared = r.binding.decode().map_err(fail)?;
        prop_assert!(
            shared == r.pres,
            "a replica's model differs from its shared state"
        );
    }
    for r in &rs[1..] {
        prop_assert!(r.pres == rs[0].pres, "replicas diverged");
    }
    let mut count: BTreeMap<NodeId, usize> = BTreeMap::new();
    for (id, _) in made(&rs[0].pres) {
        *count.entry(id).or_default() += 1;
    }
    // A shape goes with its slide.
    let gone = |id: &NodeId| {
        deleted.contains(id) || alive.get(id).is_some_and(|slide| deleted.contains(slide))
    };
    let lost: Vec<&NodeId> = alive
        .keys()
        .filter(|id| !gone(id) && !count.contains_key(id))
        .collect();
    prop_assert!(lost.is_empty(), "slides or shapes were lost: {lost:?}");
    let twice: Vec<&NodeId> = count
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, _)| id)
        .collect();
    prop_assert!(
        twice.is_empty(),
        "slides or shapes are there twice: {twice:?}"
    );
    let back: Vec<&NodeId> = count
        .keys()
        .filter(|id| gone(id) && !moved.contains(id))
        .collect();
    prop_assert!(
        back.is_empty(),
        "deleted slides or shapes came back: {back:?}"
    );
    Ok(())
}

/// 128 cases, or `KILN_PROPTEST_CASES` for a longer run.
fn cases() -> u32 {
    std::env::var("KILN_PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128)
}

fn rounds(n: usize) -> impl Strategy<Value = Vec<Vec<(usize, Edit)>>> {
    prop::collection::vec(prop::collection::vec((0..n, edit_strategy()), 1..8), 1..4)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    #[test]
    fn two_replicas_converge(r in rounds(2), s in prop::collection::vec((0..2usize, 0..2usize), 0..12)) {
        run(2, r, s)?;
    }

    #[test]
    fn three_replicas_converge(r in rounds(3), s in prop::collection::vec((0..3usize, 0..3usize), 0..12)) {
        run(3, r, s)?;
    }

    #[test]
    fn merge_order_does_not_matter(
        edits in prop::collection::vec((0..3usize, edit_strategy()), 1..12),
        order in Just(vec![0usize, 1, 2]).prop_shuffle(),
    ) {
        let fail = |e: kiln_collab::CollabError| TestCaseError::fail(e.to_string());
        let mut rs = replicas(3).map_err(fail)?;
        for (who, e) in &edits {
            rs[*who].edit(e).map_err(fail)?;
        }
        let updates: Vec<Vec<u8>> = rs
            .iter()
            .map(|r| r.binding.crdt().export_from(&[]))
            .collect::<Result<_>>()
            .map_err(fail)?;
        let merged = |order: &[usize], peer: u64| -> Result<(DeckBinding, Presentation)> {
            let (mut b, mut p) = DeckBinding::join(&updates[order[0]], peer, IdLanes::new(peer % 8, 8))?;
            for &i in &order[1..] {
                b.apply_remote(&mut p, &updates[i])?;
            }
            Ok((b, p))
        };
        let (mut bx, mut px) = merged(&[0, 1, 2], 901).map_err(fail)?;
        let (mut by, mut py) = merged(&order, 902).map_err(fail)?;
        // A merge that left the presentation invalid is repaired by the replica that merged
        // it, as a change of its own; once the two have exchanged those, they agree.
        for _ in 0..3 {
            let to_y = bx.crdt().export_from(&by.crdt().version()).map_err(fail)?;
            let to_x = by.crdt().export_from(&bx.crdt().version()).map_err(fail)?;
            by.apply_remote(&mut py, &to_y).map_err(fail)?;
            bx.apply_remote(&mut px, &to_x).map_err(fail)?;
        }
        prop_assert!(px == py, "merge order changed the result");
        prop_assert!(by.decode().map_err(fail)? == py);
        py.validate().map_err(|e| TestCaseError::fail(e.to_string()))?;
    }
}
