// SPDX-License-Identifier: AGPL-3.0-or-later
//! Spelling as you type (`kiln-spell`): squiggles under misspelled words of the visible pages,
//! checked incrementally (a paragraph is checked again only when its text, formatting or
//! revisions change), suggestions for the context menu, Ignore Once, Ignore All, Add to
//! Dictionary, Change and Change All, and the Spelling pane's next misspelling (F7).
//!
//! Text is checked in the language of its run (`w:lang`, through styles and document defaults);
//! only English (United States) has a dictionary, so text in other languages is left unchecked.
//! Deleted text, hidden text and code are not checked, and neither is the word being typed at
//! the caret.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{EditKind, Fragment, Mark, Op, Paragraph, Pos, Range, Transaction};
use kiln_doc_layout::annotations;
use kiln_protocol::TextPos;
use kiln_protocol::commands::write_features::SpellingHit;
use kiln_scene::DrawOp;
use kiln_spell::Speller;

use super::super::WriteDoc;
use super::super::view::WriteView;
use super::blocks::is_code;

/// Checked paragraphs: content fingerprint and misspelled ranges.
type Cache = HashMap<NodeId, (u64, Vec<(usize, usize)>)>;

/// The speller and its per-paragraph results.
#[derive(Debug)]
pub(in crate::apps::write) struct SpellState {
    /// Check spelling as you type.
    pub on: bool,
    pub speller: Speller,
    cache: RefCell<Cache>,
    /// Ignore Once: (paragraph, word).
    pub ignored_once: HashSet<(NodeId, String)>,
    /// Bumped when the results may change without an edit (dictionary, ignore lists).
    pub revision: u64,
}

impl Default for SpellState {
    fn default() -> Self {
        Self {
            on: true,
            speller: Speller::new(),
            cache: RefCell::new(HashMap::new()),
            ignored_once: HashSet::new(),
            revision: 0,
        }
    }
}

impl SpellState {
    /// Forgets every result (the dictionaries or ignore lists changed).
    pub fn invalidate(&mut self) {
        self.cache.get_mut().clear();
        self.revision += 1;
    }

    /// Misspellings found so far (in the paragraphs checked).
    pub fn count(&self) -> u32 {
        self.cache.try_borrow().map_or(0, |c| {
            u32::try_from(c.values().map(|(_, v)| v.len()).sum::<usize>()).unwrap_or(u32::MAX)
        })
    }
}

fn fingerprint(p: &Paragraph) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    for chunk in p.content.rope().chunks() {
        h.write(chunk.as_bytes());
    }
    for r in p.content.runs() {
        (r.len, &r.props).hash(&mut h);
    }
    p.props.style_id.hash(&mut h);
    for m in p.content.marks() {
        if matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_)) {
            (m.start, m.end).hash(&mut h);
        }
    }
    h.finish()
}

impl WriteDoc {
    /// The chars of a paragraph the speller may look at: not deleted, hidden, code or in a
    /// language without a dictionary.
    fn checkable(&self, p: &Paragraph) -> Vec<bool> {
        let len = p.content.len();
        let mut ok = vec![true; len];
        let mut at = 0usize;
        for r in p.content.runs() {
            let (s, e) = (at, (at + r.len).min(len));
            at += r.len;
            if s >= e {
                continue;
            }
            let resolved = self.doc.resolve_run_at(p, s);
            let lang = resolved.lang.val.as_deref();
            let skip = resolved.hidden
                || !kiln_spell::checks_language(lang)
                || is_code(&r.props)
                || resolved.fonts.ascii.eq_ignore_ascii_case("Consolas")
                || resolved.fonts.ascii.eq_ignore_ascii_case("Courier New");
            if skip {
                ok[s..e].iter_mut().for_each(|x| *x = false);
            }
        }
        for m in p.content.marks() {
            if matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_)) {
                let e = m.end.min(len);
                if m.start < e {
                    ok[m.start..e].iter_mut().for_each(|x| *x = false);
                }
            }
        }
        if p.props
            .style_id
            .as_deref()
            .is_some_and(|s| s.starts_with("KilnCode"))
        {
            ok.iter_mut().for_each(|x| *x = false);
        }
        ok
    }

    /// The misspelled ranges of a paragraph (cached until it changes).
    fn misspellings_of(&self, p: &Paragraph) -> Vec<(usize, usize)> {
        let s = &self.features.spell;
        if !s.on || !kiln_spell::has_dictionary() {
            return Vec::new();
        }
        let fp = fingerprint(p);
        if let Ok(cache) = s.cache.try_borrow()
            && let Some((f, v)) = cache.get(&p.id)
            && *f == fp
        {
            return v.clone();
        }
        let text = p.content.rope().to_string();
        let ok = self.checkable(p);
        let found: Vec<(usize, usize)> = s
            .speller
            .misspellings(&text)
            .into_iter()
            .filter(|w| ok.get(w.start..w.end).is_some_and(|r| r.iter().all(|x| *x)))
            .filter(|w| !s.ignored_once.contains(&(p.id, w.word.clone())))
            .map(|w| (w.start, w.end))
            .collect();
        if let Ok(mut cache) = s.cache.try_borrow_mut() {
            cache.insert(p.id, (fp, found.clone()));
        }
        found
    }

    /// Squiggles under the misspelled words of the visible pages (not under the word being
    /// typed at the caret).
    pub(super) fn spell_overlay(&self, v: &WriteView) -> Vec<DrawOp> {
        if !self.features.spell.on || !kiln_spell::has_dictionary() {
            return Vec::new();
        }
        let (first, last) = v.visible_pages();
        let mut seen: HashSet<NodeId> = HashSet::new();
        let mut words: Vec<(NodeId, Vec<(usize, usize)>)> = Vec::new();
        let typing = self.anchor == self.head
            && self.history.undo_kind() == Some(kiln_doc::EditKind::Typing);
        for page in first..=last.min(v.layout.pages().len().saturating_sub(1)) {
            for line in v.layout.page_lines(page) {
                let id = line.para.id();
                if !seen.insert(id) {
                    continue;
                }
                let Some(p) = self.doc.paragraph(id) else {
                    continue;
                };
                let mut ranges = self.misspellings_of(p);
                // The word being typed is not judged until the caret leaves it.
                if typing && self.head.para == id {
                    ranges.retain(|&(_, b)| b != self.head.offset);
                }
                if !ranges.is_empty() {
                    words.push((id, ranges));
                }
            }
        }
        annotations::squiggles(&v.layout, &words)
    }

    /// The misspelled word covering a position.
    fn misspelling_at(&self, pos: Pos) -> Option<(usize, usize)> {
        let p = self.doc.paragraph(pos.para)?;
        self.misspellings_of(p)
            .into_iter()
            .find(|&(a, b)| a <= pos.offset && pos.offset <= b)
    }

    fn hit(&self, para: NodeId, a: usize, b: usize) -> Option<SpellingHit> {
        let text: Vec<char> = self.para_text(para).chars().collect();
        let word: String = text.get(a..b)?.iter().collect();
        // The sentence around it (at most 60 chars each side).
        let from = a.saturating_sub(60);
        let to = (b + 60).min(text.len());
        let context: String = text[from..to]
            .iter()
            .map(|c| if *c == kiln_doc::OBJECT_CHAR { ' ' } else { *c })
            .collect();
        Some(SpellingHit {
            suggestions: self.features.spell.speller.suggest(&word),
            word,
            start: TextPos {
                block: para,
                offset: u32::try_from(a).unwrap_or(u32::MAX),
            },
            len: u32::try_from(b - a).unwrap_or(u32::MAX),
            context,
            context_offset: u32::try_from(a - from).unwrap_or(0),
        })
    }

    /// The context menu: the misspelled word at a position, with suggestions.
    pub(super) fn spelling_at(&self, pos: TextPos) -> Result<Option<SpellingHit>> {
        let p = Pos::new(pos.block, pos.offset as usize);
        if self.doc.paragraph(p.para).is_none() {
            return Err(KilnError::InvalidInput(format!(
                "no paragraph {}",
                pos.block
            )));
        }
        Ok(self
            .misspelling_at(p)
            .and_then(|(a, b)| self.hit(p.para, a, b)))
    }

    /// The Spelling pane: the next misspelling from a position (wrapping to the start).
    pub(super) fn next_misspelling(&self, from: Option<TextPos>) -> Result<Option<SpellingHit>> {
        let from = from.map_or_else(|| self.span().1, |t| Pos::new(t.block, t.offset as usize));
        let body: Vec<&Paragraph> = self.doc.body_paragraphs();
        let start = body.iter().position(|p| p.id == from.para).unwrap_or(0);
        let n = body.len();
        for k in 0..=n {
            let p = body[(start + k) % n.max(1)];
            for (a, b) in self.misspellings_of(p) {
                let before = k == 0 && a < from.offset;
                let wrapped_past = k == n && a >= from.offset;
                if before || wrapped_past {
                    continue;
                }
                return Ok(self.hit(p.id, a, b));
            }
        }
        Ok(None)
    }

    /// F7: selects the next misspelled word.
    pub(super) fn spell_next(&mut self) -> Result<()> {
        let hit = self.next_misspelling(None)?;
        let Some(h) = hit else {
            return Err(KilnError::InvalidInput(
                "the spelling check is complete: no misspelled words".into(),
            ));
        };
        let a = Pos::new(h.start.block, h.start.offset as usize);
        let b = Pos::new(a.para, a.offset + h.len as usize);
        self.set_selection(a, b);
        Ok(())
    }

    /// Add to Dictionary.
    pub(super) fn spell_add_word(&mut self, word: &str) -> Result<()> {
        self.features
            .spell
            .speller
            .add_user_word(word)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.features.spell.invalidate();
        Ok(())
    }

    /// Ignore Once (the word in its paragraph).
    pub(super) fn spell_ignore_once(&mut self, pos: TextPos) -> Result<()> {
        let p = Pos::new(pos.block, pos.offset as usize);
        let Some((a, b)) = self.misspelling_at(p) else {
            return Ok(());
        };
        let word: String = self.para_text(p.para).chars().skip(a).take(b - a).collect();
        self.features
            .spell
            .ignored_once
            .insert((p.para, word.replace('\u{2019}', "'")));
        self.features.spell.invalidate();
        Ok(())
    }

    /// Change / Change All.
    pub(super) fn spell_replace(&mut self, pos: TextPos, with: &str, all: bool) -> Result<()> {
        if with.is_empty() || with.chars().count() > 256 || with.contains(['\n', '\r']) {
            return Err(KilnError::InvalidInput(
                "a replacement of 1–256 characters on one line is needed".into(),
            ));
        }
        let p = Pos::new(pos.block, pos.offset as usize);
        let (a, b) = self
            .misspelling_at(p)
            .ok_or_else(|| KilnError::InvalidInput("there is no misspelled word there".into()))?;
        let word: String = self.para_text(p.para).chars().skip(a).take(b - a).collect();
        let mut targets: Vec<(NodeId, usize, usize)> = vec![(p.para, a, b)];
        if all {
            for q in self.doc.body_paragraphs() {
                let text: Vec<char> = q.content.rope().to_string().chars().collect();
                for (s, e) in self.misspellings_of(q) {
                    let w: String = text[s..e].iter().collect();
                    if w == word && !(q.id == p.para && s == a) {
                        targets.push((q.id, s, e));
                    }
                }
            }
        }
        // From the end of each paragraph, so earlier offsets stay valid.
        targets.sort_by(|x, y| x.0.cmp(&y.0).then(y.1.cmp(&x.1)));
        let caret_para = p.para;
        let mut caret = Pos::new(p.para, a + with.chars().count());
        let mut batch = self.batch(EditKind::Typing);
        let mut result = Ok(());
        for (para, s, e) in targets {
            let props = self.props_at(Pos::new(para, s + 1));
            let tx = Transaction::new(vec![
                Op::DeleteInline {
                    para,
                    offset: s,
                    len: e - s,
                },
                Op::InsertInline {
                    para,
                    offset: s,
                    fragment: Fragment::text(with, props),
                },
            ]);
            if let Err(err) = self.batch_apply(&mut batch, tx) {
                result = Err(err);
                break;
            }
            if para == caret_para && s < a {
                let delta = with.chars().count() as isize - (e - s) as isize;
                caret.offset = caret.offset.saturating_add_signed(delta);
            }
        }
        let caret = self.doc.clamp(caret).unwrap_or(caret);
        self.batch_commit(batch, Range::caret(caret));
        result
    }
}
