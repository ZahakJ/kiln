// SPDX-License-Identifier: AGPL-3.0-or-later
//! Random concurrent edits on two and three replicas of a Write document, exchanged in random
//! orders, converge: every replica ends with the same model, equal to the shared state, valid,
//! and the order in which a replica merges the others' updates does not matter.

use kiln_collab::{Crdt, Result, WriteBinding};
use kiln_doc::st::Alignment;
use kiln_doc::{CommentMeta, Document, IdLanes, Pos, ToggleProp, Transaction, edit};
use proptest::prelude::*;

/// One edit, positions chosen modulo the document it lands on.
#[derive(Debug, Clone)]
enum Edit {
    Type {
        para: usize,
        at: usize,
        text: String,
    },
    Delete {
        para: usize,
        at: usize,
        len: usize,
    },
    Split {
        para: usize,
        at: usize,
    },
    Join {
        para: usize,
    },
    Bold {
        para: usize,
        at: usize,
        len: usize,
    },
    Align {
        para: usize,
        centre: bool,
    },
    Table {
        para: usize,
    },
    Comment {
        para: usize,
    },
    Uncomment {
        which: usize,
    },
}

fn edit_strategy() -> impl Strategy<Value = Edit> {
    let p = 0..8usize;
    let at = 0..40usize;
    prop_oneof![
        4 => (p.clone(), at.clone(), "[a-zé😀 ]{1,6}").prop_map(|(para, at, text)| Edit::Type { para, at, text }),
        2 => (p.clone(), at.clone(), 1..6usize).prop_map(|(para, at, len)| Edit::Delete { para, at, len }),
        2 => (p.clone(), at.clone()).prop_map(|(para, at)| Edit::Split { para, at }),
        1 => p.clone().prop_map(|para| Edit::Join { para }),
        2 => (p.clone(), at.clone(), 1..8usize).prop_map(|(para, at, len)| Edit::Bold { para, at, len }),
        1 => (p.clone(), any::<bool>()).prop_map(|(para, centre)| Edit::Align { para, centre }),
        1 => p.clone().prop_map(|para| Edit::Table { para }),
        1 => p.clone().prop_map(|para| Edit::Comment { para }),
        1 => (0..4usize).prop_map(|which| Edit::Uncomment { which }),
    ]
}

/// Builds the transaction an edit makes, if it applies to this document.
fn build(doc: &mut Document, e: &Edit) -> Option<Transaction> {
    let paras: Vec<(kiln_common::NodeId, usize)> = doc
        .body_paragraphs()
        .iter()
        .map(|p| (p.id, p.content.len()))
        .collect();
    let pick = |i: usize| paras[i % paras.len()];
    let tx = match e {
        Edit::Type { para, at, text } => {
            let (id, len) = pick(*para);
            edit::insert_text(doc, Pos::new(id, at % (len + 1)), text)
        }
        Edit::Delete { para, at, len: n } => {
            let (id, len) = pick(*para);
            let s = at % (len + 1);
            edit::delete_range(doc, Pos::new(id, s), Pos::new(id, (s + n).min(len)))
        }
        Edit::Split { para, at } => {
            let (id, len) = pick(*para);
            edit::split_paragraph(doc, Pos::new(id, at % (len + 1)))
        }
        Edit::Join { para } => {
            let i = para % paras.len();
            let (id, len) = paras[i];
            let (next, _) = *paras.get(i + 1)?;
            edit::delete_range(doc, Pos::new(id, len), Pos::new(next, 0))
        }
        Edit::Bold { para, at, len: n } => {
            let (id, len) = pick(*para);
            let s = at % (len + 1);
            edit::toggle(
                doc,
                Pos::new(id, s),
                Pos::new(id, (s + n).min(len)),
                ToggleProp::Bold,
            )
        }
        Edit::Align { para, centre } => {
            let (id, _) = pick(*para);
            let centre = *centre;
            edit::format_paragraphs(doc, Pos::new(id, 0), Pos::new(id, 0), move |p| {
                p.alignment = centre.then_some(Alignment::Center);
            })
        }
        Edit::Table { para } => {
            let (id, len) = pick(*para);
            edit::insert_table(doc, Pos::new(id, len), 2, 2)
        }
        Edit::Comment { para } => {
            let (id, len) = pick(*para);
            let meta = CommentMeta {
                author: "Tester".into(),
                ..CommentMeta::default()
            };
            edit::add_comment(doc, Pos::new(id, 0), Pos::new(id, len.min(3)), meta, "note")
                .map(|(tx, _)| tx)
        }
        Edit::Uncomment { which } => {
            let id = doc.comments.get(which % doc.comments.len().max(1))?.id;
            edit::delete_comment(doc, id)
        }
    };
    tx.ok().filter(|t| !t.is_empty())
}

struct Replica {
    binding: WriteBinding,
    doc: Document,
}

impl Replica {
    fn edit(&mut self, e: &Edit) -> Result<()> {
        let mut scratch = self.doc.clone();
        let Some(tx) = build(&mut scratch, e) else {
            return Ok(());
        };
        // Builders allocate ids on the document they are given; keep the allocator in step.
        self.doc = scratch;
        if self.doc.apply(tx.clone()).is_err() {
            return Ok(());
        }
        self.binding.commit(&self.doc, &tx)
    }

    fn receive(&mut self, from: &Replica) -> Result<()> {
        let update = from
            .binding
            .crdt()
            .export_from(&self.binding.crdt().version())?;
        self.binding.apply_remote(&mut self.doc, &update)?;
        Ok(())
    }
}

fn replicas(n: u64) -> Result<Vec<Replica>> {
    let mut doc = Document::new();
    let first = WriteBinding::share(&mut doc, 100, IdLanes::new(0, 8))?;
    let snapshot = first.crdt().snapshot()?;
    let mut out = vec![Replica {
        binding: first,
        doc,
    }];
    for i in 1..n {
        let (binding, doc) = WriteBinding::join(&snapshot, 100 + i, IdLanes::new(i, 8))?;
        out.push(Replica { binding, doc });
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

fn check_converged(rs: &[Replica]) -> std::result::Result<(), TestCaseError> {
    for r in rs {
        r.doc
            .validate()
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let shared = r
            .binding
            .decode()
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!(
            shared == r.doc,
            "a replica's model differs from its shared state"
        );
    }
    for r in &rs[1..] {
        prop_assert!(r.doc == rs[0].doc, "replicas diverged");
    }
    Ok(())
}

fn run(
    n: usize,
    rounds: Vec<Vec<(usize, Edit)>>,
    syncs: Vec<(usize, usize)>,
) -> std::result::Result<(), TestCaseError> {
    let fail = |e: kiln_collab::CollabError| TestCaseError::fail(e.to_string());
    let mut rs = replicas(n as u64).map_err(fail)?;
    for (round, edits) in rounds.iter().enumerate() {
        for (who, e) in edits {
            rs[who % n].edit(e).map_err(fail)?;
        }
        // Partial exchanges between rounds, in a random order.
        for (to, from) in syncs.iter().skip(round * 3).take(3) {
            exchange(&mut rs, to % n, from % n).map_err(fail)?;
        }
    }
    settle(&mut rs).map_err(fail)?;
    check_converged(&rs)
}

/// Everyone gets everything, until no replica has anything another lacks (merging can add a
/// change of its own: a repair, or text moved after a split or join).
fn settle(rs: &mut [Replica]) -> Result<()> {
    let n = rs.len();
    for _ in 0..8 {
        for to in 0..n {
            for from in 0..n {
                exchange(rs, to, from)?;
            }
        }
        let v = rs[0].binding.crdt().version();
        if rs.iter().all(|r| r.binding.crdt().version() == v) {
            break;
        }
    }
    Ok(())
}

/// Typing, deletions, splits and joins, each typed character unique, and optionally one replica
/// that leaves for good after a round (the others have its edits; it gets nothing after that):
/// every character typed on any replica and deleted by none is in the converged document once,
/// and nothing else is.
fn run_typing(
    n: usize,
    rounds: Vec<Vec<(usize, Edit)>>,
    syncs: Vec<(usize, usize)>,
    leaves: Option<(usize, usize)>,
) -> std::result::Result<(), TestCaseError> {
    let fail = |e: kiln_collab::CollabError| TestCaseError::fail(e.to_string());
    let mut rs = replicas(n as u64).map_err(fail)?;
    let mut typed: Vec<char> = Vec::new();
    let mut deleted: std::collections::HashSet<char> = std::collections::HashSet::new();
    let mut gone: Option<usize> = None;
    for (round, edits) in rounds.iter().enumerate() {
        for (who, e) in edits {
            let who = who % n;
            if gone == Some(who) {
                continue;
            }
            let e = match e {
                Edit::Type { para, at, text } => {
                    let text: String = text
                        .chars()
                        .filter_map(|_| {
                            let c = u32::try_from(typed.len())
                                .ok()
                                .and_then(|k| char::from_u32(0x4E00 + k))?;
                            typed.push(c);
                            Some(c)
                        })
                        .collect();
                    Edit::Type {
                        para: *para,
                        at: *at,
                        text,
                    }
                }
                Edit::Delete { para, at, len } => {
                    let paras = rs[who].doc.body_paragraphs();
                    let text: Vec<char> = paras[para % paras.len()].text().chars().collect();
                    let s = at % (text.len() + 1);
                    deleted.extend(text[s..(s + len).min(text.len())].iter().copied());
                    e.clone()
                }
                other => other.clone(),
            };
            rs[who].edit(&e).map_err(fail)?;
        }
        for (to, from) in syncs.iter().skip(round * 3).take(3) {
            let (to, from) = (to % n, from % n);
            if gone != Some(to) {
                exchange(&mut rs, to, from).map_err(fail)?;
            }
        }
        if let Some((who, after)) = leaves
            && after == round
            && gone.is_none()
        {
            // Its edits reached the others; nothing reaches it any more.
            let who = who % n;
            for to in (0..n).filter(|&t| t != who) {
                exchange(&mut rs, to, who).map_err(fail)?;
            }
            gone = Some(who);
        }
    }
    if let Some(g) = gone {
        rs.remove(g);
    }
    settle(&mut rs).map_err(fail)?;
    check_converged(&rs)?;
    let mut count: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in rs[0]
        .doc
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.text().chars().collect::<Vec<_>>())
    {
        *count.entry(c).or_default() += 1;
    }
    let lost: String = typed
        .iter()
        .filter(|c| !deleted.contains(c) && !count.contains_key(c))
        .collect();
    prop_assert!(lost.is_empty(), "typed text was lost: {lost:?}");
    let twice: String = count
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(c, _)| *c)
        .collect();
    prop_assert!(twice.is_empty(), "typed text was duplicated: {twice:?}");
    let back: String = count
        .keys()
        .filter(|c| deleted.contains(c))
        .copied()
        .collect();
    prop_assert!(back.is_empty(), "deleted text came back: {back:?}");
    prop_assert!(
        count.keys().all(|c| typed.contains(c)),
        "text nobody typed appeared"
    );
    Ok(())
}

fn typing_strategy() -> impl Strategy<Value = Edit> {
    let p = 0..6usize;
    let at = 0..40usize;
    prop_oneof![
        4 => (p.clone(), at.clone(), "[a-z]{1,4}").prop_map(|(para, at, text)| Edit::Type { para, at, text }),
        2 => (p.clone(), at.clone(), 1..4usize).prop_map(|(para, at, len)| Edit::Delete { para, at, len }),
        3 => (p.clone(), at.clone()).prop_map(|(para, at)| Edit::Split { para, at }),
        2 => p.prop_map(|para| Edit::Join { para }),
    ]
}

/// 128 cases, or `KILN_PROPTEST_CASES` for a longer run.
fn cases() -> u32 {
    std::env::var("KILN_PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128)
}

fn typing_rounds(n: usize) -> impl Strategy<Value = Vec<Vec<(usize, Edit)>>> {
    prop::collection::vec(prop::collection::vec((0..n, typing_strategy()), 1..8), 1..4)
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
    fn typed_text_survives_concurrent_splits_and_joins(
        r in typing_rounds(2),
        s in prop::collection::vec((0..2usize, 0..2usize), 0..12),
    ) {
        run_typing(2, r, s, None)?;
    }

    #[test]
    fn typed_text_survives_on_three_replicas(
        r in typing_rounds(3),
        s in prop::collection::vec((0..3usize, 0..3usize), 0..12),
    ) {
        run_typing(3, r, s, None)?;
    }

    #[test]
    fn typed_text_survives_a_replica_that_left(
        r in typing_rounds(3),
        s in prop::collection::vec((0..3usize, 0..3usize), 0..12),
        who in 0..3usize,
        after in 0..3usize,
    ) {
        run_typing(3, r, s, Some((who, after)))?;
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
        // Each replica's whole history, merged by two fresh replicas in two orders.
        let updates: Vec<Vec<u8>> = rs
            .iter()
            .map(|r| r.binding.crdt().export_from(&[]))
            .collect::<Result<_>>()
            .map_err(fail)?;
        let merged = |order: &[usize], peer: u64| -> Result<(WriteBinding, Document)> {
            let (mut b, mut d) = WriteBinding::join(&updates[order[0]], peer, IdLanes::new(peer % 8, 8))?;
            for &i in &order[1..] {
                b.apply_remote(&mut d, &updates[i])?;
            }
            Ok((b, d))
        };
        let (mut bx, mut dx) = merged(&[0, 1, 2], 901).map_err(fail)?;
        let (mut by, mut dy) = merged(&order, 902).map_err(fail)?;
        // A merge that left the document invalid is repaired by the replica that merged it, as
        // a change of its own; once the two have exchanged those, they agree.
        for _ in 0..3 {
            let to_y = bx.crdt().export_from(&by.crdt().version()).map_err(fail)?;
            let to_x = by.crdt().export_from(&bx.crdt().version()).map_err(fail)?;
            by.apply_remote(&mut dy, &to_y).map_err(fail)?;
            bx.apply_remote(&mut dx, &to_x).map_err(fail)?;
        }
        prop_assert!(dx == dy, "merge order changed the result");
        prop_assert!(by.decode().map_err(fail)? == dy);
        dy.validate().map_err(|e| TestCaseError::fail(e.to_string()))?;
    }
}
