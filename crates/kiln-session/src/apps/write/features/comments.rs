// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comments: New Comment on the selection (or the word at the caret), replies, editing,
//! Resolve and Reopen, Delete and Delete All, selecting a comment's text; the Comments pane's
//! data with the balloons of All Markup. Comments are not tracked changes (as in Word), and a
//! reply belongs to its thread's first comment.

use std::cmp::Ordering;
use std::collections::HashMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{
    Block, CommentMeta, EditKind, Fragment, Inlines, Mark, Op, Paragraph, Pos, Range, RunProps,
    Transaction, edit,
};
use kiln_doc_layout::annotations::{self, BalloonPlacement};
use kiln_protocol::commands::write_features::{CommentInfo, MarkupMode};
use kiln_protocol::geom::{Point, Rect};

use super::super::WriteDoc;
use super::super::editing::word_at;
use super::super::view::WriteView;
use super::{initials, iso, missing_styles};

/// Longest comment text accepted.
const MAX_COMMENT: usize = 32 * 1024;

fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

fn check_text(text: &str) -> Result<()> {
    if text.chars().count() > MAX_COMMENT {
        return Err(KilnError::InvalidInput(
            "a comment holds at most 32,768 characters".into(),
        ));
    }
    Ok(())
}

/// The balloon height a comment needs (header, text lines, replies), px.
fn balloon_height(text: &str, replies: &[String]) -> f32 {
    let lines = |t: &str| -> f32 {
        let n: usize = t
            .split('\n')
            .map(|l| l.chars().count().div_ceil(34).max(1))
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let n = n.min(12) as f32;
        n
    };
    let mut h = 34.0 + 18.0 * lines(text);
    for r in replies.iter().take(6) {
        h += 30.0 + 18.0 * lines(r);
    }
    h
}

impl WriteDoc {
    /// Comment paragraphs for a text (one per line), in the Comment Text style.
    fn comment_blocks(&mut self, text: &str) -> Result<Vec<Block>> {
        let style = self
            .doc
            .styles
            .contains("CommentText")
            .then(|| "CommentText".to_owned());
        let mut out = Vec::new();
        for line in text.replace('\r', "").split('\n') {
            let mut p = Paragraph::new(self.doc.alloc_id());
            p.props.style_id.clone_from(&style);
            if !line.is_empty() {
                p.content = Inlines::from_fragment(Fragment::text(line, RunProps::default()))
                    .map_err(doc_err)?;
            }
            out.push(Block::Paragraph(p));
        }
        Ok(out)
    }

    fn meta(&self, parent: Option<NodeId>) -> CommentMeta {
        let author = self.features.review.author.clone();
        CommentMeta {
            initials: Some(initials(&author)),
            author,
            date: Some(iso(&self.now())),
            parent,
            resolved: false,
        }
    }

    /// The range New Comment anchors to: the selection, else the word at the caret, else the
    /// character before (or after) it.
    fn comment_range(&self) -> Result<(Pos, Pos)> {
        let (s, e) = self.span();
        if s != e {
            return Ok((s, e));
        }
        let text = self.para_text(s.para);
        let len = text.chars().count();
        if len == 0 {
            return Err(KilnError::InvalidInput(
                "select some text to comment on (the paragraph is empty)".into(),
            ));
        }
        let (a, b) = word_at(&text, s.offset.min(len.saturating_sub(1)));
        let word: String = text.chars().skip(a).take(b - a).collect();
        if b > a && word.chars().any(char::is_alphanumeric) {
            return Ok((Pos::new(s.para, a), Pos::new(s.para, b)));
        }
        let at = s.offset.min(len);
        if at > 0 {
            Ok((Pos::new(s.para, at - 1), Pos::new(s.para, at)))
        } else {
            Ok((Pos::new(s.para, 0), Pos::new(s.para, 1)))
        }
    }

    /// The ops that add a comment anchored to `s..e` (no anchor when they are equal).
    fn comment_ops(
        &mut self,
        s: Pos,
        e: Pos,
        meta: CommentMeta,
        text: &str,
    ) -> Result<(Vec<Op>, NodeId)> {
        let segments = edit::segments(&self.doc, s, e).map_err(doc_err)?;
        let id = self.doc.alloc_id();
        let blocks = self.comment_blocks(text)?;
        let mut ops = vec![Op::InsertComment {
            index: self.doc.comments.len(),
            comment: Box::new(kiln_doc::Comment { id, meta, blocks }),
        }];
        for (para, start, end) in segments {
            if start < end {
                ops.push(Op::AddMark {
                    para,
                    start,
                    end,
                    mark: Mark::Comment(id),
                });
            }
        }
        Ok((ops, id))
    }

    /// Applies comment ops (with the comment styles the document lacks) as one undo step,
    /// never tracked.
    fn commit_comment(
        &mut self,
        build: impl FnOnce(&mut Self) -> Result<Vec<Op>>,
        after: Range,
    ) -> Result<()> {
        let styles = missing_styles(&self.doc, &["CommentText", "CommentReference"]);
        let mut batch = self.batch(EditKind::Other);
        self.features.untracked = true;
        let r = (|| {
            self.batch_apply(&mut batch, Transaction::new(styles))?;
            let ops = build(self)?;
            self.batch_apply(&mut batch, Transaction::new(ops))
        })();
        self.features.untracked = false;
        self.batch_commit(batch, after);
        r
    }

    /// New Comment.
    pub(super) fn new_comment(&mut self, text: &str) -> Result<()> {
        check_text(text)?;
        let (s, e) = self.comment_range()?;
        let meta = self.meta(None);
        self.commit_comment(
            |d| d.comment_ops(s, e, meta, text).map(|(ops, _)| ops),
            Range::new(s, e),
        )
    }

    /// The first comment of a comment's thread.
    fn thread_root(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = id;
        for _ in 0..=self.doc.comments.len() {
            let c = self.doc.comments.iter().find(|c| c.id == cur)?;
            match c.meta.parent {
                Some(p) => cur = p,
                None => return Some(cur),
            }
        }
        Some(cur)
    }

    /// Reply.
    pub(super) fn reply_comment(&mut self, parent: NodeId, text: &str) -> Result<()> {
        check_text(text)?;
        if text.trim().is_empty() {
            return Err(KilnError::InvalidInput("a reply needs text".into()));
        }
        let root = self
            .thread_root(parent)
            .ok_or_else(|| KilnError::InvalidInput(format!("no comment {parent}")))?;
        let at = self.span().0;
        let meta = self.meta(Some(root));
        let sel = self.selection_range();
        self.commit_comment(
            |d| d.comment_ops(at, at, meta, text).map(|(ops, _)| ops),
            sel,
        )
    }

    /// Edit a comment's text.
    pub(super) fn edit_comment(&mut self, id: NodeId, text: &str) -> Result<()> {
        check_text(text)?;
        let old = self
            .doc
            .comments
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.blocks.len())
            .ok_or_else(|| KilnError::InvalidInput(format!("no comment {id}")))?;
        let blocks = self.comment_blocks(text)?;
        let n = blocks.len();
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(vec![
                Op::InsertBlocks {
                    container: id,
                    index: 0,
                    blocks,
                },
                Op::RemoveBlocks {
                    container: id,
                    index: n,
                    count: old,
                },
            ])
            .with_kind(EditKind::Other)
            .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// Resolve or reopen a thread.
    pub(super) fn resolve_comment(&mut self, id: NodeId, resolved: bool) -> Result<()> {
        let root = self
            .thread_root(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no comment {id}")))?;
        let Some(c) = self.doc.comments.iter().find(|c| c.id == root) else {
            return Err(KilnError::InvalidInput(format!("no comment {id}")));
        };
        if c.meta.resolved == resolved {
            return Ok(());
        }
        let meta = CommentMeta {
            resolved,
            ..c.meta.clone()
        };
        let sel = self.selection_range();
        self.commit(
            Transaction::new(vec![Op::SetCommentMeta { id: root, meta }])
                .with_kind(EditKind::Other)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Delete (the comment at the caret when `id` is `None`).
    pub(super) fn delete_comment(&mut self, id: Option<NodeId>) -> Result<()> {
        let id = match id {
            Some(i) => i,
            None => self.comment_at(self.span().0).ok_or_else(|| {
                KilnError::InvalidInput("there is no comment at the insertion point".into())
            })?,
        };
        if !self.doc.comments.iter().any(|c| c.id == id) {
            return Err(KilnError::InvalidInput(format!("no comment {id}")));
        }
        let tx = edit::delete_comment(&self.doc, id).map_err(doc_err)?;
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            tx.with_kind(EditKind::Other)
                .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// Delete All Comments in Document, as one undo step.
    pub(super) fn delete_all_comments(&mut self) -> Result<()> {
        let roots: Vec<NodeId> = self
            .doc
            .comments
            .iter()
            .filter(|c| c.meta.parent.is_none())
            .map(|c| c.id)
            .collect();
        if roots.is_empty() {
            return Ok(());
        }
        let after = self.selection_range();
        let mut batch = self.batch(EditKind::Other);
        self.features.untracked = true;
        let mut result = Ok(());
        for id in roots {
            if !self.doc.comments.iter().any(|c| c.id == id) {
                continue;
            }
            match edit::delete_comment(&self.doc, id) {
                Ok(tx) => {
                    if let Err(e) = self.batch_apply(&mut batch, tx) {
                        result = Err(e);
                        break;
                    }
                }
                Err(e) => {
                    result = Err(doc_err(e));
                    break;
                }
            }
        }
        // Orphaned replies (a parent missing in the file) go too.
        let orphans: Vec<NodeId> = self.doc.comments.iter().map(|c| c.id).collect();
        for id in orphans {
            if result.is_err() {
                break;
            }
            if let Ok(tx) = edit::delete_comment(&self.doc, id)
                && let Err(e) = self.batch_apply(&mut batch, tx)
            {
                result = Err(e);
            }
        }
        self.features.untracked = false;
        self.batch_commit(batch, after);
        result
    }

    /// Selects a comment's text (a click on its balloon or its entry in the Comments pane).
    pub(super) fn select_comment(&mut self, id: NodeId) -> Result<()> {
        let root = self.thread_root(id).unwrap_or(id);
        let (a, b) = self
            .comment_ranges()
            .into_iter()
            .find(|(c, _, _)| *c == root)
            .map(|(_, a, b)| (a, b))
            .ok_or_else(|| {
                KilnError::InvalidInput("the comment is not anchored in the text".into())
            })?;
        self.set_selection(a, b);
        Ok(())
    }

    /// Top-level comments with their anchors (first and last position), in document order.
    pub(super) fn comment_ranges(&self) -> Vec<(NodeId, Pos, Pos)> {
        let mut found: HashMap<NodeId, (Pos, Pos)> = HashMap::new();
        for (_, p) in self.doc.all_paragraphs() {
            for m in p.content.marks() {
                if let Mark::Comment(c) = m.mark {
                    let (a, b) = (Pos::new(p.id, m.start), Pos::new(p.id, m.end));
                    found
                        .entry(c)
                        .and_modify(|(s, e)| {
                            if self.doc.compare(a, *s) == Some(Ordering::Less) {
                                *s = a;
                            }
                            if self.doc.compare(b, *e) == Some(Ordering::Greater) {
                                *e = b;
                            }
                        })
                        .or_insert((a, b));
                }
            }
        }
        let mut out: Vec<(NodeId, Pos, Pos)> = found
            .into_iter()
            .filter(|(c, _)| {
                self.doc
                    .comments
                    .iter()
                    .any(|x| x.id == *c && x.meta.parent.is_none())
            })
            .map(|(c, (a, b))| (c, a, b))
            .collect();
        out.sort_by(|x, y| {
            self.doc
                .compare(x.1, y.1)
                .unwrap_or(Ordering::Equal)
                .then(x.0.cmp(&y.0))
        });
        out
    }

    /// The thread whose text holds a position (the innermost, latest one).
    pub(super) fn comment_at(&self, pos: Pos) -> Option<NodeId> {
        let p = self.doc.paragraph(pos.para)?;
        let hit = p
            .content
            .marks()
            .iter()
            .filter(|m| m.start <= pos.offset && pos.offset <= m.end)
            .filter_map(|m| match m.mark {
                Mark::Comment(c) => Some((m.end - m.start, c)),
                _ => None,
            })
            .min_by_key(|(len, _)| *len)?;
        self.thread_root(hit.1)
    }

    /// Plain text of a comment.
    fn comment_plain(&self, id: NodeId) -> String {
        self.doc
            .comments
            .iter()
            .find(|c| c.id == id)
            .map(|c| {
                c.blocks
                    .iter()
                    .filter_map(Block::as_paragraph)
                    .map(|p| p.content.plain_text(true))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    /// Replies of a thread in creation order (nested replies flattened after their parent).
    fn replies_of(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(parent) = stack.pop() {
            let kids: Vec<NodeId> = self
                .doc
                .comments
                .iter()
                .filter(|c| c.meta.parent == Some(parent))
                .map(|c| c.id)
                .collect();
            for k in kids.iter().rev() {
                stack.push(*k);
            }
            out.extend(kids);
            if out.len() > self.doc.comments.len() {
                break;
            }
        }
        out.sort_by_key(|id| self.doc.comments.iter().position(|c| c.id == *id));
        out
    }

    /// Balloons of the laid-out pages (Simple and All Markup with comments shown).
    pub(super) fn balloon_placements(&self, v: &WriteView) -> Vec<BalloonPlacement> {
        let r = &self.features.review;
        if !matches!(r.markup, MarkupMode::All | MarkupMode::Simple) || !r.show_comments {
            return Vec::new();
        }
        let height = |id: NodeId| {
            let replies: Vec<String> = self
                .replies_of(id)
                .into_iter()
                .map(|r| self.comment_plain(r))
                .collect();
            balloon_height(&self.comment_plain(id), &replies)
        };
        annotations::place_balloons(&v.layout, &height)
    }

    /// The Comments pane: threads in document order, each followed by its replies.
    pub(super) fn comment_infos(&self) -> Vec<CommentInfo> {
        let ranges = self.comment_ranges();
        let balloons = self
            .view
            .as_ref()
            .map(|v| self.balloon_placements(v))
            .unwrap_or_default();
        let (s, _) = self.span();
        let current = self.comment_at(s);
        let mut roots: Vec<NodeId> = ranges.iter().map(|(c, _, _)| *c).collect();
        for c in &self.doc.comments {
            if c.meta.parent.is_none() && !roots.contains(&c.id) {
                roots.push(c.id);
            }
        }
        let mut out = Vec::new();
        for root in roots {
            let range = ranges.iter().find(|(c, _, _)| *c == root);
            let anchor_text = range.map(|(_, a, b)| {
                let p = self.doc.paragraph(a.para);
                let end = if b.para == a.para {
                    b.offset
                } else {
                    p.map_or(0, |x| x.content.len())
                };
                p.and_then(|x| x.content.slice(a.offset, end).ok())
                    .map(|f| f.text.replace(kiln_doc::OBJECT_CHAR, ""))
                    .unwrap_or_default()
            });
            let (page, anchor) = range
                .and_then(|(_, a, _)| {
                    let v = self.view.as_ref()?;
                    let c = v.caret(*a)?;
                    let number = u32::try_from(c.page + 1).ok()?;
                    Some((Some(number), Some(Point::new(c.rect.x, c.rect.y))))
                })
                .unwrap_or((None, None));
            let balloon = balloons
                .iter()
                .find(|b| b.comment == root)
                .map(|b| Rect::new(b.rect.x, b.rect.y, b.rect.w, b.rect.h));
            let mut ids = vec![root];
            ids.extend(self.replies_of(root));
            let resolved = self
                .doc
                .comments
                .iter()
                .find(|c| c.id == root)
                .is_some_and(|c| c.meta.resolved);
            for (k, id) in ids.into_iter().enumerate() {
                let Some(c) = self.doc.comments.iter().find(|c| c.id == id) else {
                    continue;
                };
                let author = c.meta.author.clone();
                out.push(CommentInfo {
                    id,
                    parent: (k > 0).then_some(root),
                    initials: c
                        .meta
                        .initials
                        .clone()
                        .filter(|i| !i.is_empty())
                        .unwrap_or_else(|| initials(&author)),
                    color: annotations::author_hex(&author),
                    author,
                    date: c.meta.date.clone(),
                    text: self.comment_plain(id),
                    resolved,
                    anchor_text: if k == 0 { anchor_text.clone() } else { None },
                    page,
                    balloon: if k == 0 { balloon } else { None },
                    anchor: if k == 0 { anchor } else { None },
                    current: current == Some(root),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod unit {
    use super::balloon_height;

    #[test]
    fn balloons_grow_with_their_text_and_replies() {
        let short = balloon_height("Fine.", &[]);
        let long = balloon_height(&"word ".repeat(40), &[]);
        let with_reply = balloon_height("Fine.", &["Agreed.".into()]);
        assert!(long > short);
        assert!(with_reply > short);
        assert!(balloon_height(&"x\n".repeat(100), &[]) <= 34.0 + 18.0 * 12.0);
    }
}
