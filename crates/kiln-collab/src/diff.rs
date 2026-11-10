// SPDX-License-Identifier: AGPL-3.0-or-later
//! The word-level difference between two versions of a Write document (Version History's
//! comparison view).
//!
//! Paragraphs are matched by id (a paragraph keeps its id while it is edited), in body order;
//! within a matched paragraph the text is compared word by word, where a word is a run of
//! letters and digits, a run of spaces, or one other character. Paragraphs only in the older
//! version come out removed, those only in the newer one added.

use kiln_common::NodeId;
use kiln_doc::Document;
use serde::{Deserialize, Serialize};

/// What happened to a stretch of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    /// In both versions.
    Same,
    /// Only in the newer version.
    Added,
    /// Only in the older version.
    Removed,
}

/// A stretch of text with one [`Change`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// What happened to it.
    pub change: Change,
    /// The text.
    pub text: String,
}

/// One paragraph of the comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphDiff {
    /// The paragraph (its id in the newer version, or the older one when it was removed).
    pub para: NodeId,
    /// The paragraph as a whole: [`Change::Same`] when it is in both (its spans may still
    /// differ), otherwise added or removed.
    pub change: Change,
    /// Its text, stretch by stretch.
    pub spans: Vec<Span>,
}

impl ParagraphDiff {
    /// Whether anything in the paragraph changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.change != Change::Same || self.spans.iter().any(|s| s.change != Change::Same)
    }
}

/// The comparison of two Write documents' body text, paragraph by paragraph.
#[must_use]
pub fn word_diff(old: &Document, new: &Document) -> Vec<ParagraphDiff> {
    let a: Vec<(NodeId, String)> = old
        .body_paragraphs()
        .iter()
        .map(|p| (p.id, p.text()))
        .collect();
    let b: Vec<(NodeId, String)> = new
        .body_paragraphs()
        .iter()
        .map(|p| (p.id, p.text()))
        .collect();
    let ai: Vec<NodeId> = a.iter().map(|(id, _)| *id).collect();
    let bi: Vec<NodeId> = b.iter().map(|(id, _)| *id).collect();
    let mut out = Vec::new();
    for step in lcs(&ai, &bi) {
        out.push(match step {
            Step::Same(i, j) => ParagraphDiff {
                para: b[j].0,
                change: Change::Same,
                spans: text_diff(&a[i].1, &b[j].1),
            },
            Step::Removed(i) => whole(a[i].0, Change::Removed, &a[i].1),
            Step::Added(j) => whole(b[j].0, Change::Added, &b[j].1),
        });
    }
    out
}

fn whole(para: NodeId, change: Change, text: &str) -> ParagraphDiff {
    ParagraphDiff {
        para,
        change,
        spans: if text.is_empty() {
            Vec::new()
        } else {
            vec![Span {
                change,
                text: text.to_owned(),
            }]
        },
    }
}

/// The word-level comparison of two strings.
#[must_use]
pub fn text_diff(old: &str, new: &str) -> Vec<Span> {
    let a = words(old);
    let b = words(new);
    let mut out: Vec<Span> = Vec::new();
    let mut push = |change: Change, text: &str| match out.last_mut() {
        Some(s) if s.change == change => s.text.push_str(text),
        _ => out.push(Span {
            change,
            text: text.to_owned(),
        }),
    };
    for step in lcs(&a, &b) {
        match step {
            Step::Same(_, j) => push(Change::Same, b[j]),
            Step::Removed(i) => push(Change::Removed, a[i]),
            Step::Added(j) => push(Change::Added, b[j]),
        }
    }
    out
}

/// Words, spaces and single other characters, in order.
fn words(s: &str) -> Vec<&str> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<Class> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        let joins = prev.as_ref().is_some_and(|p| *p == k && k != Class::Other);
        if !joins && i > start {
            out.push(&s[start..i]);
            start = i;
        }
        prev = Some(k);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

enum Step {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

/// A longest common subsequence alignment (removals before additions at each change). Past
/// four million cells it compares only the common prefix and suffix.
fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Step> {
    let p = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let s = a[p..]
        .iter()
        .rev()
        .zip(b[p..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[p..a.len() - s], &b[p..b.len() - s]);
    let mut out: Vec<Step> = (0..p).map(|i| Step::Same(i, i)).collect();
    if ma.len().saturating_mul(mb.len()) > 4_000_000 {
        out.extend((0..ma.len()).map(|i| Step::Removed(p + i)));
        out.extend((0..mb.len()).map(|j| Step::Added(p + j)));
    } else {
        let (n, m) = (ma.len(), mb.len());
        // len[i][j]: the LCS length of ma[i..] and mb[j..].
        let mut len = vec![0u32; (n + 1) * (m + 1)];
        let at = |i: usize, j: usize| i * (m + 1) + j;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                len[at(i, j)] = if ma[i] == mb[j] {
                    len[at(i + 1, j + 1)] + 1
                } else {
                    len[at(i + 1, j)].max(len[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                out.push(Step::Same(p + i, p + j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || len[at(i + 1, j)] >= len[at(i, j + 1)]) {
                out.push(Step::Removed(p + i));
                i += 1;
            } else {
                out.push(Step::Added(p + j));
                j += 1;
            }
        }
    }
    let (ea, eb) = (a.len() - s, b.len() - s);
    out.extend((0..s).map(|k| Step::Same(ea + k, eb + k)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::{Pos, edit};

    fn spans(v: &[Span]) -> Vec<(Change, &str)> {
        v.iter().map(|s| (s.change, s.text.as_str())).collect()
    }

    #[test]
    fn words_are_compared_whole() {
        let d = text_diff("The quick brown fox.", "The slow brown fox jumps.");
        assert_eq!(
            spans(&d),
            vec![
                (Change::Same, "The "),
                (Change::Removed, "quick"),
                (Change::Added, "slow"),
                (Change::Same, " brown fox"),
                (Change::Added, " jumps"),
                (Change::Same, "."),
            ]
        );
        assert_eq!(
            words("don't stop—now"),
            vec!["don't", " ", "stop", "—", "now"]
        );
    }

    #[test]
    fn paragraphs_match_by_id() -> Result<(), kiln_doc::DocError> {
        let mut old = Document::new();
        let p = old.body_paragraphs()[0].id;
        let tx = edit::insert_text(&mut old, Pos::new(p, 0), "Keep this. Cut that.")?;
        old.apply(tx)?;
        let mut new = old.clone();
        let tx = edit::delete_range(&new, Pos::new(p, 10), Pos::new(p, 20))?;
        new.apply(tx)?;
        let tx = edit::split_paragraph(&mut new, Pos::new(p, 10))?;
        new.apply(tx)?;
        let q = new.body_paragraphs()[1].id;
        let tx = edit::insert_text(&mut new, Pos::new(q, 0), "New words")?;
        new.apply(tx)?;
        let d = word_diff(&old, &new);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].change, Change::Same);
        assert_eq!(
            spans(&d[0].spans),
            vec![
                (Change::Same, "Keep this."),
                (Change::Removed, " Cut that.")
            ]
        );
        assert_eq!((d[1].para, d[1].change), (q, Change::Added));
        assert!(d.iter().all(ParagraphDiff::changed));
        assert!(word_diff(&old, &old).iter().all(|p| !p.changed()));
        Ok(())
    }
}
