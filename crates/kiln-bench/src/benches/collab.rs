// SPDX-License-Identifier: AGPL-3.0-or-later
//! Collaboration benchmark, budget B14: `collab-apply-1000` merges 1,000 remote
//! operations into a shared Write document, one update per operation as they arrive over the
//! network, each turned into model ops: the time until the model shows all of them.
//!
//! The input is deterministic: a 60-paragraph document shared by peer A, and 1,000 edits A makes
//! after peer B joined — typing at random places (70 %), deletions (15 %), bold on a word
//! (10 %) and paragraph splits (5 %). Each timed run starts B from the same joined state.
//! Extra figures: `batch_ms` (the same 1,000 operations in one update), `per_op_us` (the
//! average per operation), `update_bytes` (the size of the 1,000 updates together).
//!
//! In quick mode it merges 100 operations.

use std::time::Instant;

use kiln_collab::{Crdt, WriteBinding};
use kiln_doc::{Document, IdLanes, Pos, ToggleProp, Transaction, edit};

use crate::harness::{Sample, summarize};
use crate::scenarios::RunOptions;
use crate::synth::Lcg;

const PARAGRAPHS: usize = 60;

fn starting_document() -> Result<Document, String> {
    let mut doc = Document::new();
    let mut lcg = Lcg::new(82);
    let first = doc.body_paragraphs()[0].id;
    let text: String = (0..PARAGRAPHS)
        .map(|i| {
            let words = 8 + lcg.below(20);
            let line: Vec<&str> = (0..words)
                .map(|w| {
                    [
                        "kiln", "glaze", "clay", "fire", "ash", "slip", "wheel", "ember",
                    ][(i + w) % 8]
                })
                .collect();
            line.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tx = edit::insert_text(&mut doc, Pos::new(first, 0), &text).map_err(|e| e.to_string())?;
    doc.apply(tx).map_err(|e| e.to_string())?;
    Ok(doc)
}

/// One edit at a deterministic place.
fn next_edit(doc: &mut Document, lcg: &mut Lcg) -> Result<Option<Transaction>, String> {
    let paras: Vec<(kiln_common::NodeId, usize)> = doc
        .body_paragraphs()
        .iter()
        .map(|p| (p.id, p.content.len()))
        .collect();
    let (id, len) = paras[lcg.below(paras.len())];
    let at = lcg.below(len + 1);
    let roll = lcg.below(100);
    let tx = if roll < 70 {
        edit::insert_text(
            doc,
            Pos::new(id, at),
            if roll.is_multiple_of(7) { " " } else { "e" },
        )
    } else if roll < 85 {
        let end = (at + 1 + lcg.below(3)).min(len);
        edit::delete_range(doc, Pos::new(id, at), Pos::new(id, end))
    } else if roll < 95 {
        let end = (at + 5).min(len);
        edit::toggle(doc, Pos::new(id, at), Pos::new(id, end), ToggleProp::Bold)
    } else {
        edit::split_paragraph(doc, Pos::new(id, at))
    };
    Ok(tx.ok().filter(|t| !t.is_empty()))
}

/// What B merges: the state it joined from, A's updates one per edit, and all of them as one.
struct Recorded {
    joined: Vec<u8>,
    updates: Vec<Vec<u8>>,
    all: Vec<u8>,
}

fn record(ops: usize) -> Result<Recorded, String> {
    let e = |e: kiln_collab::CollabError| e.to_string();
    let mut doc = starting_document()?;
    let mut a = WriteBinding::share(&mut doc, 1, IdLanes::new(0, 4)).map_err(e)?;
    let joined = a.crdt().snapshot().map_err(e)?;
    let mut updates = Vec::with_capacity(ops);
    let mut lcg = Lcg::new(1000);
    while updates.len() < ops {
        let before = a.crdt().version();
        let Some(tx) = next_edit(&mut doc, &mut lcg)? else {
            continue;
        };
        if doc.apply(tx.clone()).is_err() {
            continue;
        }
        a.commit(&doc, &tx).map_err(e)?;
        updates.push(a.crdt().export_from(&before).map_err(e)?);
    }
    let (base, _) = WriteBinding::join(&joined, 9, IdLanes::default()).map_err(e)?;
    let all = a.crdt().export_from(&base.crdt().version()).map_err(e)?;
    Ok(Recorded {
        joined,
        updates,
        all,
    })
}

/// `collab-apply-1000` (B14).
pub fn apply_1000(options: &RunOptions) -> Result<Sample, String> {
    let e = |e: kiln_collab::CollabError| e.to_string();
    let ops = if options.quick { 100 } else { 1000 };
    let Recorded {
        joined,
        updates,
        all,
    } = record(ops)?;
    let (warmup, iterations) = if options.quick { (0, 2) } else { (2, 15) };
    let mut times = Vec::with_capacity(iterations);
    let mut batch = Vec::with_capacity(iterations);
    let mut expected: Option<Document> = None;
    for i in 0..warmup + iterations {
        let (mut b, mut doc) = WriteBinding::join(&joined, 2, IdLanes::new(1, 4)).map_err(e)?;
        let t = Instant::now();
        for u in &updates {
            b.apply_remote(&mut doc, u).map_err(e)?;
        }
        let elapsed = t.elapsed();
        let (mut c, mut cdoc) = WriteBinding::join(&joined, 3, IdLanes::new(2, 4)).map_err(e)?;
        let t = Instant::now();
        c.apply_remote(&mut cdoc, &all).map_err(e)?;
        let batch_elapsed = t.elapsed();
        // Both ways must give the same model as the sender's.
        if doc != cdoc {
            return Err("one-by-one and batched merges differ".into());
        }
        if let Some(x) = &expected {
            if *x != doc {
                return Err("runs differ".into());
            }
        } else {
            expected = Some(doc);
        }
        if i >= warmup {
            times.push(elapsed);
            batch.push(batch_elapsed);
        }
    }
    let mut sample = summarize("collab-apply-1000", &times, None);
    let b = summarize("batch", &batch, None);
    sample.extra.insert("batch_ms".into(), b.p95_ms);
    #[allow(clippy::cast_precision_loss)]
    let per_op = sample.p50_ms * 1000.0 / ops as f64;
    sample.extra.insert("per_op_us".into(), per_op);
    #[allow(clippy::cast_precision_loss)]
    let bytes = updates.iter().map(Vec::len).sum::<usize>() as f64;
    sample.extra.insert("update_bytes".into(), bytes);
    Ok(sample)
}
