// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only views of a document: the outline (Navigation pane), word and character counts
//! (status bar, Word Count dialog), comments with their threads, tracked changes, list labels,
//! bookmarks and plain text.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::inline::{InlineObject, Mark};
use crate::model::{Block, Document, Paragraph, Story};
use crate::numbering::{ListCounter, ListLabel};
use crate::resolve::effective_numbering;
use crate::revision::{InsDel, RevisionInfo, RevisionKind};
use kiln_common::NodeId;

/// A heading in the outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineEntry {
    /// The heading paragraph.
    pub para: NodeId,
    /// Outline level 0–8 (Heading 1 = 0).
    pub level: u8,
    /// Its text.
    pub text: String,
}

/// Document statistics as Word's Word Count dialog shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Statistics {
    /// Words.
    pub words: usize,
    /// Characters without spaces.
    pub characters: usize,
    /// Characters with spaces.
    pub characters_with_spaces: usize,
    /// Paragraphs with text.
    pub paragraphs: usize,
}

/// What [`Document::statistics`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatisticsOptions {
    /// Include footnotes and endnotes (Word's "Include textboxes, footnotes and endnotes").
    pub include_notes: bool,
}

fn is_east_asian(c: char) -> bool {
    matches!(u32::from(c),
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
        | 0x20000..=0x2FA1F)
}

fn is_word_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{00A0}' | '\u{2000}'..='\u{200B}' | '\u{3000}')
}

/// Counts words as Word does: runs of non-space characters that contain a letter or digit
/// (a lone "—" is not a word), and every CJK ideograph, kana or hangul syllable counts as a word.
#[must_use]
pub fn count_words(text: &str) -> usize {
    let mut count = 0;
    for token in text.split(is_word_space) {
        if token.is_empty() {
            continue;
        }
        if !token.chars().any(is_east_asian) {
            if token.chars().any(char::is_alphanumeric) {
                count += 1;
            }
            continue;
        }
        count += token.chars().filter(|c| is_east_asian(*c)).count();
        count += token
            .split(is_east_asian)
            .filter(|t| t.chars().any(char::is_alphanumeric))
            .count();
    }
    count
}

/// Characters as Word counts them: tabs, breaks and object placeholders are not characters.
fn count_chars(text: &str) -> (usize, usize) {
    let mut with_spaces = 0;
    let mut no_spaces = 0;
    for c in text.chars() {
        if matches!(
            c,
            '\n' | '\t' | '\u{000C}' | '\u{000B}' | crate::OBJECT_CHAR
        ) {
            continue;
        }
        with_spaces += 1;
        if !c.is_whitespace() {
            no_spaces += 1;
        }
    }
    (with_spaces, no_spaces)
}

/// A comment and its replies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentThread {
    /// The first comment.
    pub id: NodeId,
    /// Author.
    pub author: String,
    /// Date.
    pub date: Option<String>,
    /// Resolved.
    pub resolved: bool,
    /// Plain text of the body.
    pub text: String,
    /// The commented text (from the first paragraph it marks), if anchored.
    pub anchor_text: Option<String>,
    /// The first paragraph the comment marks.
    pub anchor_para: Option<NodeId>,
    /// Replies in order, each with its own replies flattened after it.
    pub replies: Vec<CommentThread>,
}

/// Where a tracked change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionLocation {
    /// A range of a paragraph's text.
    Text {
        /// The paragraph.
        para: NodeId,
        /// First char.
        start: usize,
        /// One past the last char.
        end: usize,
    },
    /// A paragraph's properties or mark.
    Paragraph(NodeId),
    /// A table row.
    Row(NodeId),
}

/// A tracked change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionEntry {
    /// What changed.
    pub kind: RevisionKind,
    /// Who and when.
    pub info: RevisionInfo,
    /// Where.
    pub location: RevisionLocation,
    /// The inserted or deleted text, for text changes.
    pub text: Option<String>,
}

/// A bookmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkEntry {
    /// The name.
    pub name: String,
    /// The paragraph holding its start.
    pub para: NodeId,
    /// Char offset of the start marker.
    pub offset: usize,
}

/// What [`Document::plain_text`] includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlainTextOptions {
    /// Headers and footers first.
    pub include_headers_footers: bool,
    /// Notes at the end.
    pub include_notes: bool,
    /// Leave out deleted text (as if all changes were accepted).
    pub accept_changes: bool,
    /// Prefix list paragraphs with their labels and a tab.
    pub list_labels: bool,
}

impl Document {
    /// The headings: body paragraphs with an outline level below 9, in document order.
    #[must_use]
    pub fn outline(&self) -> Vec<OutlineEntry> {
        self.body_paragraphs()
            .into_iter()
            .filter_map(|p| {
                let r = self.resolve_paragraph(p);
                (r.outline_level < 9).then(|| OutlineEntry {
                    para: p.id,
                    level: r.outline_level,
                    text: p.content.plain_text(true).trim().to_owned(),
                })
            })
            .collect()
    }

    fn statistics_blocks(&self, blocks: &[Block], text: &mut String, paragraphs: &mut usize) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let t = p.content.plain_text(true);
                    if t.chars()
                        .any(|c| !c.is_whitespace() && c != crate::OBJECT_CHAR)
                    {
                        *paragraphs += 1;
                    }
                    text.push_str(&t);
                    text.push('\n');
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            self.statistics_blocks(&cell.blocks, text, paragraphs);
                        }
                    }
                }
                Block::Sdt(s) => self.statistics_blocks(&s.blocks, text, paragraphs),
                Block::Field(f) => self.statistics_blocks(&f.blocks, text, paragraphs),
                Block::Opaque(_) => {}
            }
        }
    }

    /// Word, character and paragraph counts of the body (deleted text excluded).
    #[must_use]
    pub fn statistics(&self, options: StatisticsOptions) -> Statistics {
        let mut text = String::new();
        let mut paragraphs = 0;
        for s in &self.sections {
            self.statistics_blocks(&s.blocks, &mut text, &mut paragraphs);
        }
        if options.include_notes {
            for n in &self.notes {
                if n.note_type == crate::st::NoteType::Normal {
                    self.statistics_blocks(&n.blocks, &mut text, &mut paragraphs);
                }
            }
        }
        let (with_spaces, no_spaces) = count_chars(&text);
        Statistics {
            words: count_words(&text),
            characters: no_spaces,
            characters_with_spaces: with_spaces,
            paragraphs,
        }
    }

    /// Word, character and paragraph counts of one paragraph range (a selection inside one
    /// paragraph).
    #[must_use]
    pub fn statistics_of_text(text: &str) -> Statistics {
        let (with_spaces, no_spaces) = count_chars(text);
        Statistics {
            words: count_words(text),
            characters: no_spaces,
            characters_with_spaces: with_spaces,
            paragraphs: usize::from(text.chars().any(|c| !c.is_whitespace())),
        }
    }

    /// The list label of every numbered body paragraph (headers, footers and notes number
    /// separately in Word and are not included).
    #[must_use]
    pub fn list_labels(&self) -> HashMap<NodeId, ListLabel> {
        let ctx = self.context();
        let mut counter = ListCounter::new(&self.numbering, Some(&self.styles));
        let mut out = HashMap::new();
        for p in self.body_paragraphs() {
            if let Some(n) = effective_numbering(&ctx, &p.props)
                && let Some(label) = counter.next(n.num_id, n.level)
            {
                out.insert(p.id, label);
            }
        }
        out
    }

    fn comment_text(&self, id: NodeId) -> String {
        self.comments
            .iter()
            .find(|c| c.id == id)
            .map(|c| {
                c.blocks
                    .iter()
                    .filter_map(Block::as_paragraph)
                    .map(Paragraph::text)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    fn comment_anchor(&self, id: NodeId) -> Option<(NodeId, String)> {
        for story in self.stories() {
            for p in self.paragraphs_in(story) {
                if let Some(m) = p
                    .content
                    .marks()
                    .iter()
                    .find(|m| m.mark == Mark::Comment(id))
                {
                    let text: String = p
                        .content
                        .slice(m.start, m.end)
                        .map(|f| {
                            f.text
                                .chars()
                                .filter(|c| *c != crate::OBJECT_CHAR)
                                .collect()
                        })
                        .unwrap_or_default();
                    return Some((p.id, text));
                }
            }
        }
        None
    }

    fn thread(&self, id: NodeId, depth: usize) -> Option<CommentThread> {
        let c = self.comments.iter().find(|c| c.id == id)?;
        let anchor = self.comment_anchor(id);
        let replies = if depth > self.comments.len() {
            Vec::new()
        } else {
            self.comments
                .iter()
                .filter(|r| r.meta.parent == Some(id))
                .filter_map(|r| self.thread(r.id, depth + 1))
                .collect()
        };
        Some(CommentThread {
            id,
            author: c.meta.author.clone(),
            date: c.meta.date.clone(),
            resolved: c.meta.resolved,
            text: self.comment_text(id),
            anchor_text: anchor.as_ref().map(|a| a.1.clone()),
            anchor_para: anchor.map(|a| a.0),
            replies,
        })
    }

    /// Comment threads: top-level comments in creation order with their replies.
    #[must_use]
    pub fn comment_threads(&self) -> Vec<CommentThread> {
        self.comments
            .iter()
            .filter(|c| c.meta.parent.is_none())
            .filter_map(|c| self.thread(c.id, 0))
            .collect()
    }

    /// Every tracked change in document order (body, then headers/footers, notes, comments).
    #[must_use]
    pub fn revisions(&self) -> Vec<RevisionEntry> {
        let mut out = Vec::new();
        for story in self.stories() {
            let Some(blocks) = self.story_blocks(story) else {
                continue;
            };
            collect_revisions(blocks, &mut out);
        }
        out
    }

    /// Bookmarks in document order.
    #[must_use]
    pub fn bookmarks(&self) -> Vec<BookmarkEntry> {
        let mut out = Vec::new();
        for story in self.stories() {
            for p in self.paragraphs_in(story) {
                for (offset, o) in p.content.objects_with_offsets() {
                    if let InlineObject::BookmarkStart { name } = o {
                        out.push(BookmarkEntry {
                            name: name.clone(),
                            para: p.id,
                            offset,
                        });
                    }
                }
            }
        }
        out
    }

    fn plain_blocks(
        &self,
        blocks: &[Block],
        options: PlainTextOptions,
        labels: &HashMap<NodeId, ListLabel>,
        lines: &mut Vec<String>,
    ) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let mut line = String::new();
                    if options.list_labels
                        && let Some(l) = labels.get(&p.id)
                    {
                        line.push_str(&l.text);
                        line.push('\t');
                    }
                    line.push_str(&p.content.plain_text(options.accept_changes));
                    lines.push(line);
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        let cells: Vec<String> = row
                            .cells
                            .iter()
                            .map(|c| {
                                let mut inner = Vec::new();
                                self.plain_blocks(&c.blocks, options, labels, &mut inner);
                                inner.join(" ")
                            })
                            .collect();
                        lines.push(cells.join("\t"));
                    }
                }
                Block::Sdt(s) => self.plain_blocks(&s.blocks, options, labels, lines),
                Block::Field(f) => self.plain_blocks(&f.blocks, options, labels, lines),
                Block::Opaque(o) => {
                    if !o.label.is_empty() {
                        lines.push(o.label.clone());
                    }
                }
            }
        }
    }

    /// Plain text: one line per paragraph, table cells separated by tabs and rows by new lines,
    /// line breaks as `\n`, page breaks as form feeds.
    #[must_use]
    pub fn plain_text(&self, options: PlainTextOptions) -> String {
        let labels = if options.list_labels {
            self.list_labels()
        } else {
            HashMap::new()
        };
        let mut lines = Vec::new();
        if options.include_headers_footers {
            for h in &self.headers_footers {
                self.plain_blocks(&h.blocks, options, &labels, &mut lines);
            }
        }
        for s in &self.sections {
            self.plain_blocks(&s.blocks, options, &labels, &mut lines);
        }
        if options.include_notes {
            for n in &self.notes {
                if n.note_type == crate::st::NoteType::Normal {
                    self.plain_blocks(&n.blocks, options, &labels, &mut lines);
                }
            }
        }
        lines.join("\n")
    }

    /// The text of a bookmark (for REF fields): from its start marker to its end marker, within
    /// one story; `None` when it is missing.
    #[must_use]
    pub fn bookmark_text(&self, name: &str) -> Option<String> {
        for story in self.stories() {
            let mut inside = false;
            let mut out = String::new();
            for p in self.paragraphs_in(story) {
                let raw: Vec<char> = p.content.raw_text().chars().collect();
                let mut k = 0;
                for c in raw {
                    if c == crate::OBJECT_CHAR {
                        match p.content.objects().get(k) {
                            Some(InlineObject::BookmarkStart { name: n }) if n == name => {
                                inside = true;
                            }
                            Some(InlineObject::BookmarkEnd { name: n }) if n == name && inside => {
                                return Some(out);
                            }
                            Some(o) if inside => out.push_str(&o.plain_text()),
                            _ => {}
                        }
                        k += 1;
                    } else if inside {
                        out.push(c);
                    }
                }
                if inside {
                    out.push('\n');
                }
            }
            if inside {
                return Some(out.trim_end_matches('\n').to_owned());
            }
        }
        None
    }

    /// Paragraphs of the body with their story, for callers that iterate everything.
    #[must_use]
    pub fn all_paragraphs(&self) -> Vec<(Story, &Paragraph)> {
        self.stories()
            .flat_map(|s| self.paragraphs_in(s).into_iter().map(move |p| (s, p)))
            .collect()
    }
}

fn collect_revisions(blocks: &[Block], out: &mut Vec<RevisionEntry>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                if let Some(c) = &p.revision.props_change {
                    out.push(RevisionEntry {
                        kind: RevisionKind::ParagraphFormat,
                        info: c.info.clone(),
                        location: RevisionLocation::Paragraph(p.id),
                        text: None,
                    });
                }
                if let Some(c) = &p.revision.mark_props_change {
                    out.push(RevisionEntry {
                        kind: RevisionKind::ParagraphMarkFormat,
                        info: c.info.clone(),
                        location: RevisionLocation::Paragraph(p.id),
                        text: None,
                    });
                }
                for m in p.content.marks() {
                    let kind = match &m.mark {
                        Mark::Insertion(_) => RevisionKind::Insertion,
                        Mark::Deletion(_) => RevisionKind::Deletion,
                        Mark::MoveFrom(_) => RevisionKind::MoveFrom,
                        Mark::MoveTo(_) => RevisionKind::MoveTo,
                        Mark::FormatChange { .. } => RevisionKind::RunFormat,
                        _ => continue,
                    };
                    let Some(info) = m.mark.revision() else {
                        continue;
                    };
                    let text = p.content.slice(m.start, m.end).ok().map(|f| {
                        f.text
                            .chars()
                            .filter(|c| *c != crate::OBJECT_CHAR)
                            .collect()
                    });
                    out.push(RevisionEntry {
                        kind,
                        info: info.clone(),
                        location: RevisionLocation::Text {
                            para: p.id,
                            start: m.start,
                            end: m.end,
                        },
                        text,
                    });
                }
                if let Some(r) = &p.revision.mark {
                    out.push(RevisionEntry {
                        kind: match r.kind {
                            InsDel::Insertion => RevisionKind::ParagraphInsertion,
                            InsDel::Deletion => RevisionKind::ParagraphDeletion,
                        },
                        info: r.info.clone(),
                        location: RevisionLocation::Paragraph(p.id),
                        text: None,
                    });
                }
                for o in p.content.objects() {
                    if let Some(tb) = o.blocks() {
                        collect_revisions(tb, out);
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    if let Some(r) = &row.revision {
                        out.push(RevisionEntry {
                            kind: match r.kind {
                                InsDel::Insertion => RevisionKind::RowInsertion,
                                InsDel::Deletion => RevisionKind::RowDeletion,
                            },
                            info: r.info.clone(),
                            location: RevisionLocation::Row(row.id),
                            text: None,
                        });
                    }
                    for c in &row.cells {
                        collect_revisions(&c.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => collect_revisions(&s.blocks, out),
            Block::Field(f) => collect_revisions(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

#[cfg(test)]
mod tests;
