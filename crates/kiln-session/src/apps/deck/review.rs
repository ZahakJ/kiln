// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review in Kiln Deck: comment threads on slides and objects — New Comment, replies,
//! edits, resolving, Delete (one, the slide's, every comment), Previous and Next across the
//! slides — and Spelling (F7) through the slides' text and their speaker notes with `kiln-spell`.
//!
//! Comments are slide data of the model (`kiln_deck::Comment`), saved by `kiln-io-pptx` as
//! PowerPoint's threaded comments when they have replies or are resolved, else as classic ones.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, TextTarget, Transaction};
use kiln_deck::slide::{Comment, CommentReply};
use kiln_deck_layout::geom::{emu, px};
use kiln_doc::Pos;
use kiln_protocol::commands::deck_review::{
    DeckComment, DeckCommentReply, DeckCommentScope, DeckSpelling,
};

use super::DeckDoc;
use super::find::{shape_top, targets_of};
use crate::model::Change;

/// The longest comment Kiln takes.
const MAX_COMMENT_CHARS: usize = 10_000;

/// A place in the text in reading order: slide, text body, paragraph, offset.
type SpellKey = (usize, usize, usize, usize);

/// Spelling's state: the speller with the session's ignored and added words, the word it stopped
/// at, and where the round started.
#[derive(Debug, Default)]
pub(crate) struct Spelling {
    speller: Option<kiln_spell::Speller>,
    pub hit: Option<DeckSpelling>,
    pub done: bool,
    start: Option<SpellKey>,
    wrapped: bool,
}

impl Spelling {
    fn speller(&mut self) -> &mut kiln_spell::Speller {
        self.speller.get_or_insert_with(kiln_spell::Speller::new)
    }
}

fn check_text(text: &str) -> Result<String> {
    let t = text.trim_end();
    if t.trim().is_empty() {
        return Err(KilnError::InvalidInput("a comment needs some text".into()));
    }
    if t.chars().count() > MAX_COMMENT_CHARS {
        return Err(KilnError::InvalidInput(format!(
            "a comment is at most {MAX_COMMENT_CHARS} characters"
        )));
    }
    Ok(t.to_owned())
}

impl DeckDoc {
    /// The comment's time stamp: the host's clock as ISO 8601.
    fn comment_date(&self) -> String {
        super::save::w3cdtf(self.now_ms)
    }

    /// The slide and index of a thread, or of the thread a reply is in.
    fn find_comment(&self, id: NodeId) -> Option<(NodeId, usize, Option<usize>)> {
        self.pres.slides.iter().find_map(|s| {
            s.comments.iter().enumerate().find_map(|(i, c)| {
                if c.id == id {
                    Some((s.id, i, None))
                } else {
                    c.replies
                        .iter()
                        .position(|r| r.id == id)
                        .map(|r| (s.id, i, Some(r)))
                }
            })
        })
    }

    fn comment_at(&self, slide: NodeId, index: usize) -> Option<&Comment> {
        self.pres.slide(slide)?.comments.get(index)
    }

    /// Comment commands.
    pub(crate) fn comment_command(
        &mut self,
        cmd: &kiln_protocol::commands::DeckCommand,
    ) -> Result<Change> {
        use kiln_protocol::commands::DeckCommand as D;
        let label = cmd.meta().id;
        match cmd {
            D::NewComment {
                text,
                author,
                initials,
            } => {
                let text = check_text(text)?;
                let slide = self.slide_id()?;
                let s = self
                    .pres
                    .slide(slide)
                    .ok_or_else(|| KilnError::InvalidInput("no slide".into()))?;
                let shape = self.view.shapes.first().copied();
                // On an object: its top right corner; on the slide: down the left edge.
                let pos = shape
                    .and_then(|id| self.xfrm_of(id).map(|x| (x.x + x.cx, x.y)))
                    .unwrap_or_else(|| {
                        let n = s.comments.iter().filter(|c| c.shape.is_none()).count();
                        #[allow(clippy::cast_precision_loss)]
                        (emu(12.0), emu(12.0 + 36.0 * n as f64))
                    });
                let index = s.comments.len();
                let comment = Comment {
                    id: self.pres.alloc_id(),
                    author: author.trim().to_owned(),
                    initials: initials.trim().to_owned(),
                    created: self.comment_date(),
                    text,
                    shape,
                    pos,
                    replies: Vec::new(),
                    resolved: false,
                };
                let id = comment.id;
                self.apply(
                    label,
                    Transaction::new(vec![Op::InsertComment {
                        slide,
                        index,
                        comment: Box::new(comment),
                    }]),
                )?;
                self.view.comment = Some(id);
                Ok(Change::Content)
            }
            D::ReplyComment { id, text, author } => {
                let text = check_text(text)?;
                let (slide, index, _) = self
                    .find_comment(*id)
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                let mut c = self
                    .comment_at(slide, index)
                    .cloned()
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                c.replies.push(CommentReply {
                    id: self.pres.alloc_id(),
                    author: author.trim().to_owned(),
                    created: self.comment_date(),
                    text,
                });
                // A reply reopens a resolved thread, as in PowerPoint.
                c.resolved = false;
                self.set_comment(label, slide, index, c)?;
                self.view.comment = Some(*id);
                Ok(Change::Content)
            }
            D::EditComment { id, text } => {
                let text = check_text(text)?;
                let (slide, index, reply) = self
                    .find_comment(*id)
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                let mut c = self
                    .comment_at(slide, index)
                    .cloned()
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                match reply.and_then(|r| c.replies.get_mut(r)) {
                    Some(r) => r.text = text,
                    None => c.text = text,
                }
                self.set_comment(label, slide, index, c)?;
                Ok(Change::Content)
            }
            D::ResolveComment { id, resolved } => {
                let (slide, index, _) = self
                    .find_comment(*id)
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                let mut c = self
                    .comment_at(slide, index)
                    .cloned()
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                if c.resolved == *resolved {
                    return Ok(Change::None);
                }
                c.resolved = *resolved;
                self.set_comment(label, slide, index, c)?;
                Ok(Change::Content)
            }
            D::DeleteComment { id, scope } => self.delete_comments(label, *id, *scope),
            D::SelectComment { id } => {
                self.view.comment = id.filter(|id| {
                    self.current_slide()
                        .and_then(|s| self.pres.slide(s))
                        .is_some_and(|s| s.comments.iter().any(|c| c.id == *id))
                });
                // The object a thread is on is selected with it.
                if let Some(shape) = self
                    .view
                    .comment
                    .and_then(|c| self.find_comment(c))
                    .and_then(|(s, i, _)| self.comment_at(s, i))
                    .and_then(|c| c.shape)
                    .filter(|s| self.pres.shape(*s).is_some())
                {
                    self.end_text();
                    self.view.shapes = vec![shape_top(self, shape)];
                }
                Ok(Change::Selection)
            }
            D::NextComment { backward } => self.next_comment(*backward),
            _ => Ok(Change::None),
        }
    }

    fn set_comment(
        &mut self,
        label: &'static str,
        slide: NodeId,
        index: usize,
        comment: Comment,
    ) -> Result<bool> {
        self.apply(
            label,
            Transaction::new(vec![Op::SetComment {
                slide,
                index,
                comment: Box::new(comment),
            }]),
        )
    }

    fn delete_comments(
        &mut self,
        label: &'static str,
        id: Option<NodeId>,
        scope: DeckCommentScope,
    ) -> Result<Change> {
        let mut ops = Vec::new();
        match scope {
            DeckCommentScope::One => {
                let id = id
                    .or(self.view.comment)
                    .ok_or_else(|| KilnError::unavailable(label, "select a comment first"))?;
                let (slide, index, reply) = self
                    .find_comment(id)
                    .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                match reply {
                    Some(r) => {
                        let mut c = self
                            .comment_at(slide, index)
                            .cloned()
                            .ok_or_else(|| KilnError::InvalidInput("no such comment".into()))?;
                        c.replies.remove(r);
                        ops.push(Op::SetComment {
                            slide,
                            index,
                            comment: Box::new(c),
                        });
                    }
                    None => ops.push(Op::RemoveComment { slide, index }),
                }
            }
            DeckCommentScope::Slide | DeckCommentScope::All => {
                let slides: Vec<NodeId> = if scope == DeckCommentScope::Slide {
                    vec![self.slide_id()?]
                } else {
                    self.pres.slides.iter().map(|s| s.id).collect()
                };
                for slide in slides {
                    let n = self.pres.slide(slide).map_or(0, |s| s.comments.len());
                    // From the last, so the indexes stay right.
                    for index in (0..n).rev() {
                        ops.push(Op::RemoveComment { slide, index });
                    }
                }
            }
        }
        if ops.is_empty() {
            return Ok(Change::None);
        }
        self.apply(label, Transaction::new(ops))?;
        if self
            .view
            .comment
            .is_some_and(|c| self.find_comment(c).is_none())
        {
            self.view.comment = None;
        }
        Ok(Change::Content)
    }

    /// Previous and Next: the thread before or after the selected one, on this slide or the
    /// nearest slide with comments (wrapping round, as PowerPoint does).
    fn next_comment(&mut self, backward: bool) -> Result<Change> {
        let all: Vec<(usize, NodeId)> = self
            .pres
            .slides
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.comments.iter().map(move |c| (i, c.id)))
            .collect();
        if all.is_empty() {
            return Err(KilnError::unavailable(
                "deck.comment.next",
                "the presentation has no comments",
            ));
        }
        let at = self
            .view
            .comment
            .and_then(|c| all.iter().position(|(_, id)| *id == c));
        let current = self.view.current;
        let pick = match (at, backward) {
            (Some(i), false) => all[(i + 1) % all.len()],
            (Some(i), true) => all[(i + all.len() - 1) % all.len()],
            (None, false) => all
                .iter()
                .find(|(s, _)| *s >= current)
                .copied()
                .unwrap_or(all[0]),
            (None, true) => all
                .iter()
                .rev()
                .find(|(s, _)| *s <= current)
                .copied()
                .unwrap_or(all[all.len() - 1]),
        };
        if pick.0 != current {
            self.go_to(pick.0);
        }
        self.comment_command(&kiln_protocol::commands::DeckCommand::SelectComment {
            id: Some(pick.1),
        })
    }

    /// The current slide's threads as the Comments pane and the markers show them.
    pub(crate) fn comment_infos(&self) -> Vec<DeckComment> {
        let Some(slide) = self.current_slide().and_then(|s| self.pres.slide(s)) else {
            return Vec::new();
        };
        slide
            .comments
            .iter()
            .map(|c| {
                let on = c.shape.and_then(|s| self.pres.shape(s));
                // A marker follows the object it is on.
                let (x, y) = on
                    .and_then(|s| self.xfrm_of(s.id).map(|x| (x.x + x.cx, x.y)))
                    .unwrap_or(c.pos);
                DeckComment {
                    id: c.id,
                    author: c.author.clone(),
                    initials: if c.initials.is_empty() {
                        initials(&c.author)
                    } else {
                        c.initials.clone()
                    },
                    date: c.created.clone(),
                    text: c.text.clone(),
                    resolved: c.resolved,
                    shape: on.map(|s| s.id),
                    shape_name: on.map(|s| s.name.clone()),
                    x: px(x),
                    y: px(y),
                    color: kiln_doc_layout::annotations::author_hex(&c.author),
                    replies: c
                        .replies
                        .iter()
                        .map(|r| DeckCommentReply {
                            id: r.id,
                            author: r.author.clone(),
                            date: r.created.clone(),
                            text: r.text.clone(),
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// Comment threads in the presentation.
    pub(crate) fn comment_total(&self) -> u32 {
        let n: usize = self.pres.slides.iter().map(|s| s.comments.len()).sum();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    // ── Spelling ──────────────────────────────────────────────────────────────────────────

    /// Spelling commands.
    pub(crate) fn spell_command(
        &mut self,
        cmd: &kiln_protocol::commands::DeckCommand,
    ) -> Result<Change> {
        use kiln_protocol::commands::DeckCommand as D;
        match cmd {
            D::SpellCheck => self.spell_next(true),
            D::SpellChange { with, all } => {
                let Some(hit) = self.spelling.hit.clone() else {
                    return Err(KilnError::unavailable(
                        "deck.spell.change",
                        "start Spelling first (F7)",
                    ));
                };
                if *all {
                    self.replace(&hit.word, with, true, true, true)?;
                } else if self.selection_text().as_deref() == Some(hit.word.as_str()) {
                    self.type_text(with)?;
                }
                self.spell_next(false)?;
                Ok(Change::Content)
            }
            D::SpellIgnore { all, add } => {
                let Some(hit) = self.spelling.hit.clone() else {
                    return Ok(Change::None);
                };
                if *add {
                    self.spelling
                        .speller()
                        .add_user_word(&hit.word)
                        .map_err(|e| KilnError::InvalidInput(format!("{e:?}")))?;
                } else if *all {
                    self.spelling.speller().ignore_all(&hit.word);
                }
                self.spell_next(false)
            }
            D::LoadDictionary { aff, dic } => {
                kiln_spell::load_dictionary(aff, dic).map_err(KilnError::InvalidInput)?;
                Ok(Change::Selection)
            }
            D::SetUserDictionary { words } => {
                self.spelling
                    .speller()
                    .set_user_words(words.iter().map(String::as_str));
                Ok(Change::Selection)
            }
            _ => Ok(Change::None),
        }
    }

    /// The selected text while editing.
    fn selection_text(&self) -> Option<String> {
        let t = self.view.text.as_ref()?;
        let body = self.body(t.target)?;
        Some(super::text::range_text(body, t.anchor, t.head))
    }

    /// Where Spelling goes on from: the end of the selection, else the start of the current
    /// slide, as `(slide, target, paragraph, offset)` in reading order.
    fn spell_origin(&self) -> SpellKey {
        let (slide, at) = self.search_origin();
        let Some((target, para, off)) = at else {
            return (slide, 0, 0, 0);
        };
        let Some(s) = self.pres.slides.get(slide) else {
            return (slide, 0, 0, 0);
        };
        let ti = targets_of(s).iter().position(|t| *t == target).unwrap_or(0);
        let pi = self
            .body(target)
            .and_then(|b| b.paragraphs().position(|p| p.id == para))
            .unwrap_or(0);
        (slide, ti, pi, off)
    }

    /// Every misspelled word of the slides and their notes, in reading order.
    fn misspellings(&mut self) -> Vec<(SpellKey, TextTarget, NodeId, kiln_spell::Word)> {
        let mut texts = Vec::new();
        for (si, s) in self.pres.slides.iter().enumerate() {
            for (ti, target) in targets_of(s).into_iter().enumerate() {
                let Some(body) = self.body(target) else {
                    continue;
                };
                for (pi, p) in body.paragraphs().enumerate() {
                    texts.push(((si, ti, pi), target, p.id, p.content.raw_text()));
                }
            }
        }
        let speller = self.spelling.speller();
        texts
            .into_iter()
            .flat_map(|((si, ti, pi), target, para, text)| {
                speller
                    .misspellings(&text)
                    .into_iter()
                    .map(move |w| ((si, ti, pi, w.start), target, para, w))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Selects the next misspelled word after the selection, going on from the slides after it
    /// to the slides before it; `fresh` (F7) starts a new round from here, and the round ends
    /// where it started.
    fn spell_next(&mut self, fresh: bool) -> Result<Change> {
        let origin = self.spell_origin();
        if fresh || self.spelling.start.is_none() {
            self.spelling.start = Some(origin);
            self.spelling.wrapped = false;
        }
        let start = self.spelling.start.unwrap_or(origin);
        let all = self.misspellings();
        let mut pick = if self.spelling.wrapped {
            all.into_iter().find(|(k, ..)| *k >= origin && *k < start)
        } else {
            let after = all.iter().position(|(k, ..)| *k >= origin);
            match after {
                Some(i) => all.into_iter().nth(i),
                None => {
                    self.spelling.wrapped = true;
                    all.into_iter().find(|(k, ..)| *k < start)
                }
            }
        };
        if fresh && pick.is_none() && origin != (0, 0, 0, 0) {
            // Nothing after the selection: the slides before it.
            self.spelling.wrapped = true;
            pick = self.misspellings().into_iter().find(|(k, ..)| *k < start);
        }
        let Some(((si, ..), target, para, w)) = pick else {
            self.spelling.hit = None;
            self.spelling.done = true;
            self.spelling.start = None;
            self.end_text();
            return Ok(Change::Selection);
        };
        self.spelling.done = false;
        self.go_to(si);
        self.view.rail = false;
        let notes = matches!(target, TextTarget::Notes(_));
        if notes {
            self.view.notes = true;
        }
        let shape = match target {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } | TextTarget::Notes(s) => s,
        };
        self.view.text = Some(super::TextEdit {
            shape,
            target,
            anchor: Pos::new(para, w.start),
            head: Pos::new(para, w.end),
            goal_x: None,
            composition: None,
        });
        if !notes {
            self.view.shapes = vec![shape_top(self, shape)];
        }
        let suggestions = self.spelling.speller().suggest(&w.word);
        self.spelling.hit = Some(DeckSpelling {
            slide: u32::try_from(si).unwrap_or(0),
            notes,
            word: w.word,
            suggestions,
        });
        self.epoch += 1;
        Ok(Change::Selection)
    }
}

/// Two-letter initials of a name ("Zoë Okafor" → "ZO").
fn initials(name: &str) -> String {
    let mut out: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .take(2)
        .collect();
    if out.is_empty() {
        out.push('?');
    }
    out
}

#[cfg(test)]
mod tests {
    use kiln_protocol::commands::DeckCommand as D;
    use kiln_protocol::{Command, FileFormat};

    use super::super::DeckDoc;
    use super::*;
    use crate::model::DocModel;

    fn run(d: &mut DeckDoc, c: D) -> Change {
        d.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"))
    }

    fn doc() -> DeckDoc {
        let mut d = DeckDoc::new(kiln_deck::Presentation::new(), None, None);
        d.now_ms = 1_792_000_000_000;
        d
    }

    fn new_comment(d: &mut DeckDoc, text: &str, author: &str) -> NodeId {
        run(
            d,
            D::NewComment {
                text: text.into(),
                author: author.into(),
                initials: String::new(),
            },
        );
        d.view.comment.expect("the new thread is selected")
    }

    fn reopen(d: &DeckDoc) -> kiln_deck::Presentation {
        let bytes = d.save(FileFormat::Pptx).expect("save");
        kiln_io_pptx::read_pptx(&bytes)
            .expect("reopen")
            .presentation
    }

    /// A slide with a title, a body and notes (the given texts).
    fn text_slide(d: &mut DeckDoc, title: &str, body: &str, notes: &str) {
        run(d, D::NewSlide { layout: None });
        let shapes: Vec<NodeId> = d.pres.slides[d.view.current]
            .shapes
            .iter()
            .map(|s| s.id)
            .collect();
        for (id, text) in shapes.iter().zip([title, body]) {
            run(d, D::SelectShapes { ids: vec![*id] });
            run(d, D::EditText);
            run(d, D::InsertText { text: text.into() });
            run(d, D::Escape);
            run(d, D::Escape);
        }
        run(d, D::SetNotes { text: notes.into() });
    }

    #[test]
    fn comments_on_objects_and_slides_take_replies_and_are_saved() {
        let mut d = doc();
        let title = d.pres.slides[0].shapes[0].id;
        run(&mut d, D::SelectShapes { ids: vec![title] });
        let on_title = new_comment(&mut d, "Bigger title?", "Zoë Okafor");
        run(&mut d, D::SelectShapes { ids: vec![] });
        let on_slide = new_comment(&mut d, "Check the date", "Sam Price");
        let st = d.deck_state();
        assert_eq!(st.comment_total, 2);
        assert_eq!(st.comments[0].shape, Some(title));
        assert_eq!(st.comments[0].initials, "ZO");
        assert!(st.comments[1].shape.is_none());
        run(
            &mut d,
            D::ReplyComment {
                id: on_title,
                text: "Done.".into(),
                author: "Sam Price".into(),
            },
        );
        let reply = d.deck_state().comments[0].replies[0].id;
        run(
            &mut d,
            D::EditComment {
                id: reply,
                text: "Done, 54 pt.".into(),
            },
        );
        run(
            &mut d,
            D::ResolveComment {
                id: on_title,
                resolved: true,
            },
        );
        // The marker follows the object.
        let before = d.deck_state().comments[0].x;
        run(&mut d, D::SelectShapes { ids: vec![title] });
        run(
            &mut d,
            D::Nudge {
                direction: kiln_protocol::commands::DeckDirection::Right,
                fine: false,
            },
        );
        assert!(d.deck_state().comments[0].x > before);
        let p = reopen(&d);
        let c = &p.slides[0].comments;
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].author, "Zoë Okafor");
        assert!(c[0].resolved);
        assert_eq!(c[0].replies[0].text, "Done, 54 pt.");
        assert!(c[0].shape.is_some());
        assert_eq!(c[1].text, "Check the date");
        // Delete the reply, then the slide's comments, then undo.
        run(
            &mut d,
            D::DeleteComment {
                id: Some(reply),
                scope: DeckCommentScope::One,
            },
        );
        assert!(d.deck_state().comments[0].replies.is_empty());
        run(
            &mut d,
            D::DeleteComment {
                id: None,
                scope: DeckCommentScope::Slide,
            },
        );
        assert_eq!(d.deck_state().comment_total, 0);
        assert!(d.undo());
        assert_eq!(d.deck_state().comment_total, 2);
        assert_eq!(on_slide, d.deck_state().comments[1].id);
    }

    #[test]
    fn previous_and_next_go_across_slides_and_delete_all_empties_them() {
        let mut d = doc();
        let first = new_comment(&mut d, "One", "A");
        run(&mut d, D::NewSlide { layout: None });
        run(&mut d, D::NewSlide { layout: None });
        let third = new_comment(&mut d, "Three", "B");
        run(
            &mut d,
            D::SelectSlide {
                index: 1,
                extend: false,
                toggle: false,
            },
        );
        run(&mut d, D::NextComment { backward: false });
        assert_eq!((d.view.current, d.view.comment), (2, Some(third)));
        run(&mut d, D::NextComment { backward: false });
        assert_eq!((d.view.current, d.view.comment), (0, Some(first)));
        run(&mut d, D::NextComment { backward: true });
        assert_eq!(d.view.comment, Some(third));
        run(
            &mut d,
            D::DeleteComment {
                id: None,
                scope: DeckCommentScope::All,
            },
        );
        assert_eq!(d.deck_state().comment_total, 0);
        assert!(
            d.execute(&Command::Deck(D::NextComment { backward: false }))
                .is_err()
        );
    }

    #[test]
    fn spelling_goes_through_the_slides_and_their_notes() {
        let mut d = doc();
        text_slide(
            &mut d,
            "Firing schedule",
            "Bisque then glazze",
            "Remmember the cones",
        );
        text_slide(&mut d, "Kilns", "Two kilnz in the yard", "");
        run(
            &mut d,
            D::SelectSlide {
                index: 0,
                extend: false,
                toggle: false,
            },
        );
        run(&mut d, D::SpellCheck);
        let hit = d.deck_state().spelling.expect("a misspelling");
        assert_eq!(
            (hit.slide, hit.notes, hit.word.as_str()),
            (1, false, "glazze")
        );
        assert!(
            hit.suggestions.iter().any(|s| s == "glaze"),
            "{:?}",
            hit.suggestions
        );
        run(
            &mut d,
            D::SpellChange {
                with: "glaze".into(),
                all: false,
            },
        );
        let hit = d.deck_state().spelling.expect("the notes");
        assert_eq!(
            (hit.slide, hit.notes, hit.word.as_str()),
            (1, true, "Remmember")
        );
        run(
            &mut d,
            D::SpellIgnore {
                all: false,
                add: false,
            },
        );
        let hit = d.deck_state().spelling.expect("the next slide");
        assert_eq!((hit.slide, hit.word.as_str()), (2, "kilnz"));
        run(
            &mut d,
            D::SpellIgnore {
                all: true,
                add: false,
            },
        );
        let st = d.deck_state();
        assert!(st.spelling.is_none());
        assert!(st.spell_done);
        let body = d.pres.slides[1].shapes[1]
            .text()
            .map(kiln_deck::TextBody::text);
        assert_eq!(body.as_deref(), Some("Bisque then glaze"));
        // A new round from the last slide goes round to the first: the ignored word stays out.
        run(
            &mut d,
            D::SelectSlide {
                index: 2,
                extend: false,
                toggle: false,
            },
        );
        run(&mut d, D::SpellCheck);
        let hit = d.deck_state().spelling.expect("round to slide 2");
        assert_eq!(hit.word, "Remmember");
        run(
            &mut d,
            D::SpellChange {
                with: "Remember".into(),
                all: true,
            },
        );
        assert!(d.deck_state().spell_done);
        assert_eq!(d.pres.slides[1].notes_text(), "Remember the cones");
    }
}
