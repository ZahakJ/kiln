// SPDX-License-Identifier: AGPL-3.0-or-later
//! The status bar's word and character counts, kept per paragraph and refreshed only for the
//! paragraphs an edit touched: counting the whole of a 300-page document on every keystroke
//! cost more than the typing budget (B5). The sums equal `Document::statistics` (counts are
//! additive over paragraphs: words never span a paragraph mark, and the mark is not counted).

use rustc_hash::FxHashMap as HashMap;

use kiln_common::NodeId;
use kiln_doc::query::Statistics;
use kiln_doc::{Block, Document, Paragraph, Transaction};

use crate::apps::write::view::{is_global, touched};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ParaStats {
    words: usize,
    characters: usize,
    characters_with_spaces: usize,
    has_text: bool,
}

/// Per-paragraph counts of the body.
#[derive(Debug, Default)]
pub(in crate::apps::write) struct StatsCache {
    per: HashMap<NodeId, ParaStats>,
}

fn count(p: &Paragraph) -> ParaStats {
    let text = p.content.plain_text(true);
    let st = Document::statistics_of_text(&text);
    ParaStats {
        words: st.words,
        characters: st.characters,
        characters_with_spaces: st.characters_with_spaces,
        has_text: text
            .chars()
            .any(|c| !c.is_whitespace() && c != kiln_doc::OBJECT_CHAR),
    }
}

impl StatsCache {
    /// Forgets the counts of the paragraphs an edit touched.
    pub fn note(&mut self, tx: &Transaction) {
        if is_global(tx) {
            return;
        }
        for id in touched(tx) {
            self.per.remove(&id);
        }
    }

    fn blocks(&mut self, blocks: &[Block], out: &mut Statistics, seen: &mut usize) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    *seen += 1;
                    let s = *self.per.entry(p.id).or_insert_with(|| count(p));
                    out.words += s.words;
                    out.characters += s.characters;
                    out.characters_with_spaces += s.characters_with_spaces;
                    out.paragraphs += usize::from(s.has_text);
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            self.blocks(&cell.blocks, out, seen);
                        }
                    }
                }
                Block::Sdt(s) => self.blocks(&s.blocks, out, seen),
                Block::Field(f) => self.blocks(&f.blocks, out, seen),
                Block::Opaque(_) => {}
            }
        }
    }

    /// The body's counts (as `Document::statistics` without notes).
    pub fn body(&mut self, doc: &Document) -> Statistics {
        let mut out = Statistics::default();
        let mut seen = 0;
        for s in &doc.sections {
            self.blocks(&s.blocks, &mut out, &mut seen);
        }
        // Drop the counts of paragraphs that are gone.
        if self.per.len() > seen.saturating_mul(2).max(64) {
            self.per.clear();
        }
        out
    }
}
