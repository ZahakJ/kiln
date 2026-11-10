// SPDX-License-Identifier: AGPL-3.0-or-later
//! Find, Replace, Replace All and Go To.
//!
//! A query is compiled to a regular expression (the `regex` crate: linear time, no
//! backtracking blow-ups) over the text of the caret's story, its paragraphs joined by `\n`:
//!
//! - plain text, with Word's special characters `^p` (paragraph mark), `^t` (tab), `^?` (any
//!   character), `^#` (digit), `^$` (letter), `^w` (white space), `^l` (line break), `^^`;
//! - Word's wildcards: `?`, `*`, `@`, `{n}`, `{n,}`, `{n,m}`, `[abc]`, `[a-z]`, `[!x]`, `<`, `>`,
//!   `(…)` groups and `\` escapes (always case-sensitive, as in Word);
//! - a regular expression (a Kiln extra).
//!
//! Match case and Find whole words only apply to plain text and regular expressions.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{EditKind, InlineObject, Pos, Range, Story, edit};
use kiln_protocol::commands::{FindHit, FindQuery, FindResults, GoToTarget};
use regex::{Regex, RegexBuilder};

use super::doc_err;
use crate::apps::write::WriteDoc;

/// The most hits a Find All lists.
const MAX_HITS: usize = 1000;

/// The text of a story with the paragraph of every byte.
pub(super) struct StoryText {
    text: String,
    /// (paragraph, byte start).
    paras: Vec<(NodeId, usize)>,
}

impl StoryText {
    /// The model position of a byte offset.
    fn pos(&self, byte: usize) -> Pos {
        let i = self
            .paras
            .partition_point(|(_, b)| *b <= byte)
            .saturating_sub(1);
        let (id, start) = self.paras[i];
        let end = byte.min(self.text.len()).max(start);
        Pos::new(id, self.text[start..end].chars().count())
    }

    /// The byte offset of a model position.
    fn byte(&self, p: Pos) -> Option<usize> {
        let (_, start) = self.paras.iter().find(|(id, _)| *id == p.para)?;
        let tail = &self.text[*start..];
        Some(
            start
                + tail
                    .char_indices()
                    .nth(p.offset)
                    .map_or_else(|| tail.find('\n').unwrap_or(tail.len()), |(b, _)| b),
        )
    }
}

fn plain_pattern(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^'
            && let Some(&n) = chars.peek()
        {
            chars.next();
            out.push_str(match n {
                'p' => "\\n",
                't' => "\\t",
                '?' => ".",
                '#' => "\\d",
                '$' => "\\p{L}",
                'w' => "\\s+",
                'l' => "\\x{000B}",
                '^' => "\\^",
                other => {
                    out.push_str(&regex::escape(&format!("^{other}")));
                    continue;
                }
            });
            continue;
        }
        out.push_str(&regex::escape(&c.to_string()));
    }
    out
}

/// Word's wildcard syntax as a regular expression.
fn wildcard_pattern(text: &str) -> std::result::Result<String, String> {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                i += 1;
                let n = chars.get(i).ok_or("a \\ at the end escapes nothing")?;
                out.push_str(&regex::escape(&n.to_string()));
            }
            '?' => out.push('.'),
            '*' => out.push_str(".*?"),
            '@' => out.push('+'),
            '<' | '>' => out.push_str("\\b"),
            '(' | ')' => out.push(c),
            '{' => {
                let end = chars[i..]
                    .iter()
                    .position(|x| *x == '}')
                    .ok_or("an unclosed {")?;
                let inner: String = chars[i + 1..i + end].iter().collect();
                let inner = inner.replace(';', ",");
                if inner.is_empty() || !inner.chars().all(|x| x.is_ascii_digit() || x == ',') {
                    return Err(format!("{{{inner}}} is not a repeat count"));
                }
                out.push('{');
                out.push_str(&inner);
                out.push('}');
                i += end;
            }
            '[' => {
                let end = chars[i..]
                    .iter()
                    .position(|x| *x == ']')
                    .ok_or("an unclosed [")?;
                let inner: Vec<char> = chars[i + 1..i + end].to_vec();
                out.push('[');
                let mut k = 0;
                if inner.first() == Some(&'!') {
                    out.push('^');
                    k = 1;
                }
                while k < inner.len() {
                    let x = inner[k];
                    if x == '-' && k > 0 && k + 1 < inner.len() {
                        out.push('-');
                    } else if x == '\\' && k + 1 < inner.len() {
                        k += 1;
                        out.push_str(&regex::escape(&inner[k].to_string()));
                    } else {
                        out.push_str(&regex::escape(&x.to_string()));
                    }
                    k += 1;
                }
                out.push(']');
                i += end;
            }
            '^' => {
                i += 1;
                let n = chars.get(i).ok_or("a ^ at the end")?;
                out.push_str(match n {
                    'p' | '1' => "\\n",
                    't' => "\\t",
                    '^' => "\\^",
                    other => return Err(format!("^{other} is not a wildcard special character")),
                });
            }
            other => out.push_str(&regex::escape(&other.to_string())),
        }
        i += 1;
    }
    Ok(out)
}

/// Compiles a query (an error message for an invalid pattern).
pub(super) fn compile(q: &FindQuery) -> std::result::Result<Regex, String> {
    if q.text.is_empty() {
        return Err("nothing to find".into());
    }
    if q.text.chars().count() > 255 {
        return Err("Find looks for up to 255 characters".into());
    }
    let (mut pattern, case) = if q.regex {
        (q.text.clone(), q.match_case)
    } else if q.wildcards {
        (wildcard_pattern(&q.text)?, true)
    } else {
        (plain_pattern(&q.text), q.match_case)
    };
    if q.whole_word && !q.wildcards {
        pattern = format!("\\b(?:{pattern})\\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!case)
        .size_limit(1 << 22)
        .dfa_size_limit(1 << 22)
        .build()
        .map_err(|e| match e {
            regex::Error::Syntax(s) => s.lines().last().unwrap_or("invalid pattern").to_owned(),
            regex::Error::CompiledTooBig(_) => "the pattern is too complex".to_owned(),
            _ => "invalid pattern".to_owned(),
        })
}

/// A replacement text: `^&` (the match), `^p`, `^t`, `^^`, and groups `\1`–`\9` (wildcards,
/// regular expressions; `$1` also works with regular expressions).
fn expand(replacement: &str, caps: &regex::Captures<'_>) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('^', Some('&')) => {
                chars.next();
                out.push_str(caps.get(0).map_or("", |m| m.as_str()));
            }
            ('^', Some('p')) => {
                chars.next();
                out.push('\n');
            }
            ('^', Some('t')) => {
                chars.next();
                out.push('\t');
            }
            ('^', Some('^')) => {
                chars.next();
                out.push('^');
            }
            ('\\' | '$', Some(d)) if d.is_ascii_digit() => {
                chars.next();
                let n = d.to_digit(10).unwrap_or(0) as usize;
                out.push_str(caps.get(n).map_or("", |m| m.as_str()));
            }
            _ => out.push(c),
        }
    }
    out
}

impl WriteDoc {
    /// The text of the story the caret is in.
    pub(super) fn story_text(&self) -> StoryText {
        let (s, _) = self.span();
        let ids = self.story_paragraphs(s.para);
        let mut text = String::new();
        let mut paras = Vec::with_capacity(ids.len());
        for (i, id) in ids.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            paras.push((*id, text.len()));
            text.push_str(&self.para_text(*id));
        }
        StoryText { text, paras }
    }

    fn compiled(q: &FindQuery) -> Result<Regex> {
        compile(q).map_err(KilnError::InvalidInput)
    }

    /// Selects the next (previous) match, wrapping around the story.
    pub(super) fn find_next(&mut self, q: &FindQuery, backward: bool) -> Result<()> {
        let re = Self::compiled(q)?;
        let st = self.story_text();
        let (s, e) = self.span();
        let from = st.byte(if backward { s } else { e }).unwrap_or(0);
        let mut matches = re.find_iter(&st.text).filter(|m| !m.is_empty());
        let found = if backward {
            let all: Vec<_> = matches.collect();
            all.iter()
                .rev()
                .find(|m| m.end() <= from && m.start() < from)
                .or(all.last())
                .map(|m| (m.start(), m.end()))
        } else {
            let first = re
                .find_iter(&st.text)
                .find(|m| !m.is_empty())
                .map(|m| (m.start(), m.end()));
            matches
                .find(|m| m.start() >= from)
                .map(|m| (m.start(), m.end()))
                .or(first)
        };
        if let Some((a, b)) = found {
            self.set_selection(st.pos(a), st.pos(b));
        }
        Ok(())
    }

    /// Replaces the selection when it is a match, then finds the next one.
    pub(super) fn replace_one(&mut self, q: &FindQuery, replacement: &str) -> Result<()> {
        let re = Self::compiled(q)?;
        let st = self.story_text();
        let (s, e) = self.span();
        if let (Some(a), Some(b)) = (st.byte(s), st.byte(e))
            && a < b
            && let Some(caps) = re.captures_at(&st.text, a)
            && caps.get(0).is_some_and(|m| m.start() == a && m.end() == b)
        {
            let text = expand(replacement, &caps);
            let mut batch = self.batch(EditKind::Structure);
            let end = self.replace_range(&mut batch, s, e, &text)?;
            self.batch_commit(batch, Range::caret(end));
        }
        self.find_next(q, false)
    }

    /// Replaces `s..e` with text in the formatting of its first character; returns the end of
    /// the inserted text.
    fn replace_range(
        &mut self,
        batch: &mut super::Batch,
        s: Pos,
        e: Pos,
        text: &str,
    ) -> Result<Pos> {
        let tx = edit::delete_range(&self.doc, s, e).map_err(doc_err)?;
        let p = self.para(s.para)?;
        let props = p
            .content
            .props_at(s.offset)
            .cloned()
            .unwrap_or_else(|| edit::typing_props(p, s.offset));
        self.batch_apply(batch, tx)?;
        if text.is_empty() {
            return Ok(s);
        }
        let mut ins = edit::insert_text(&mut self.doc, s, text).map_err(doc_err)?;
        for op in &mut ins.ops {
            if let kiln_doc::Op::InsertInline { fragment, .. } = op {
                for r in &mut fragment.runs {
                    r.props = props.clone();
                }
            }
        }
        let end = ins.selection_after.map_or(s, |r| r.head);
        self.batch_apply(batch, ins)?;
        Ok(end)
    }

    /// Replace All as one undo step.
    pub(super) fn replace_all(&mut self, q: &FindQuery, replacement: &str) -> Result<()> {
        let re = Self::compiled(q)?;
        let st = self.story_text();
        let found: Vec<(usize, usize, String)> = re
            .captures_iter(&st.text)
            .filter_map(|c| {
                let m = c.get(0)?;
                (!m.is_empty()).then(|| (m.start(), m.end(), expand(replacement, &c)))
            })
            .collect();
        if found.is_empty() {
            return Ok(());
        }
        let mut batch = self.batch(EditKind::Structure);
        // From the end, so earlier positions stay valid.
        for (a, b, text) in found.into_iter().rev() {
            let (s, e) = (st.pos(a), st.pos(b));
            // A match across a table boundary cannot be replaced; Word skips it too.
            let _ = self.replace_range(&mut batch, s, e, &text);
        }
        let caret = self
            .doc
            .clamp(self.head)
            .unwrap_or_else(|| self.doc_start());
        self.batch_commit(batch, Range::caret(caret));
        Ok(())
    }

    /// Every match of a query (the Navigation pane's results).
    pub(super) fn find_all(&self, q: &FindQuery) -> FindResults {
        let re = match compile(q) {
            Ok(re) => re,
            Err(error) => {
                return FindResults {
                    hits: Vec::new(),
                    total: 0,
                    error: Some(error),
                };
            }
        };
        let st = self.story_text();
        let mut hits = Vec::new();
        let mut total = 0u32;
        for m in re.find_iter(&st.text).filter(|m| !m.is_empty()) {
            total = total.saturating_add(1);
            if hits.len() >= MAX_HITS {
                continue;
            }
            let (s, e) = (st.pos(m.start()), st.pos(m.end()));
            let before: String = st.text[..m.start()]
                .chars()
                .rev()
                .take_while(|c| *c != '\n')
                .take(40)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let after: String = st.text[m.end()..]
                .chars()
                .take_while(|c| *c != '\n')
                .take(40)
                .collect();
            let page = self
                .view
                .as_ref()
                .and_then(|v| v.caret(s).and_then(|c| u32::try_from(c.page + 1).ok()));
            hits.push(FindHit {
                start: Self::wire(s),
                end: Self::wire(e),
                before: before.replace(kiln_doc::OBJECT_CHAR, ""),
                text: m.as_str().replace(kiln_doc::OBJECT_CHAR, ""),
                after: after.replace(kiln_doc::OBJECT_CHAR, ""),
                page,
            });
        }
        FindResults {
            hits,
            total,
            error: None,
        }
    }

    /// Go To.
    pub(super) fn go_to(&mut self, target: &GoToTarget) -> Result<()> {
        let pos = match target {
            GoToTarget::Page { number, relative } => {
                let current = {
                    let head = self.head;
                    let v = self.view_synced();
                    v.caret(head).map_or(0, |c| c.page)
                };
                let v = self.view_synced();
                let n = v.layout.pages().len();
                if n == 0 {
                    return Ok(());
                }
                let index = if *relative {
                    i64::try_from(current).unwrap_or(0) + i64::from(*number)
                } else {
                    i64::from(*number) - 1
                };
                let index =
                    usize::try_from(index.clamp(0, i64::try_from(n - 1).unwrap_or(0))).unwrap_or(0);
                // The first body line of the page whose start shows the caret on that page: the
                // empty line after a page break at the end of a paragraph belongs to the previous
                // page as far as the caret is concerned.
                let starts: Vec<Pos> = v
                    .layout
                    .page_lines(index)
                    .iter()
                    .filter(|l| l.story == kiln_doc_layout::StoryKind::Body)
                    .map(|l| {
                        let byte = l
                            .para
                            .lines
                            .get(l.line as usize)
                            .map_or(0, |i| i.bytes.start);
                        Pos::new(l.para.id(), l.para.src.map.doc_of(byte))
                    })
                    .collect();
                starts
                    .iter()
                    .copied()
                    .find(|p| v.caret(*p).is_some_and(|c| c.page == index))
                    .or_else(|| starts.first().copied())
            }
            GoToTarget::Section { number } => {
                let i = usize::try_from(number.saturating_sub(1)).unwrap_or(0);
                let paras = self.doc.paragraphs_in(Story::Body(
                    i.min(self.doc.sections.len().saturating_sub(1)),
                ));
                paras.first().map(|p| Pos::new(p.id, 0))
            }
            GoToTarget::Line { number } => {
                let v = self.view_synced();
                let order = v.layout.reading_order();
                let i = usize::try_from(number.saturating_sub(1))
                    .unwrap_or(0)
                    .min(order.len().saturating_sub(1));
                order.get(i).and_then(|r| {
                    let l = v.layout.line(*r)?;
                    let byte = l.para.lines.get(l.line as usize)?.bytes.start;
                    Some(Pos::new(l.para.id(), l.para.src.map.doc_of(byte)))
                })
            }
            GoToTarget::Bookmark { name } => self
                .doc
                .bookmarks()
                .into_iter()
                .find(|b| b.name.eq_ignore_ascii_case(name))
                .map(|b| Pos::new(b.para, b.offset + 1)),
            GoToTarget::Paragraph { para } => self.doc.paragraph(*para).map(|p| Pos::new(p.id, 0)),
            GoToTarget::Footnote { backward } => {
                self.next_object(*backward, |o| matches!(o, InlineObject::NoteRef(_)))
            }
            GoToTarget::Comment { backward } => self.next_comment(*backward),
        };
        let Some(pos) = pos.and_then(|p| self.doc.clamp(p)) else {
            return Err(KilnError::InvalidInput(
                "no such place in the document".into(),
            ));
        };
        if self.editor.header_footer.is_some()
            && matches!(self.doc.story_of(pos.para), Some(Story::Body(_)))
        {
            self.editor.header_footer = None;
        }
        self.set_selection(pos, pos);
        Ok(())
    }

    fn next_object(&self, backward: bool, test: impl Fn(&InlineObject) -> bool) -> Option<Pos> {
        let body: Vec<NodeId> = self.doc.body_paragraphs().iter().map(|p| p.id).collect();
        let mut all = Vec::new();
        for id in &body {
            if let Some(p) = self.doc.paragraph(*id) {
                for (off, o) in p.content.objects_with_offsets() {
                    if test(o) {
                        all.push(Pos::new(*id, off));
                    }
                }
            }
        }
        self.next_of(all, backward, &body)
    }

    fn next_comment(&self, backward: bool) -> Option<Pos> {
        let body: Vec<NodeId> = self.doc.body_paragraphs().iter().map(|p| p.id).collect();
        let mut all = Vec::new();
        for id in &body {
            if let Some(p) = self.doc.paragraph(*id) {
                for m in p.content.marks() {
                    if matches!(m.mark, kiln_doc::Mark::Comment(_)) {
                        all.push(Pos::new(*id, m.start));
                    }
                }
            }
        }
        self.next_of(all, backward, &body)
    }

    fn next_of(&self, all: Vec<Pos>, backward: bool, order: &[NodeId]) -> Option<Pos> {
        let key = |p: &Pos| {
            (
                order.iter().position(|x| *x == p.para).unwrap_or(0),
                p.offset,
            )
        };
        let here = key(&self.head);
        if backward {
            all.iter()
                .rev()
                .find(|p| key(p) < here)
                .or(all.last())
                .copied()
        } else {
            all.iter().find(|p| key(p) > here).or(all.first()).copied()
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn q(text: &str) -> FindQuery {
        FindQuery {
            text: text.into(),
            ..FindQuery::default()
        }
    }

    #[test]
    fn plain_queries_escape_and_map_specials() {
        let re = compile(&q("a.b^tc")).unwrap_or_else(|_| Regex::new("x").expect("re"));
        assert!(re.is_match("A.B\tc") && !re.is_match("axb\tc"));
        let whole = FindQuery {
            whole_word: true,
            match_case: true,
            ..q("kiln")
        };
        let re = compile(&whole).unwrap_or_else(|_| Regex::new("x").expect("re"));
        assert!(re.is_match("the kiln.") && !re.is_match("kilns") && !re.is_match("Kiln"));
    }

    #[test]
    fn word_wildcards() {
        let w = |t: &str| FindQuery {
            wildcards: true,
            ..q(t)
        };
        let re = compile(&w("<(k)?ln>")).unwrap_or_else(|_| Regex::new("x").expect("re"));
        assert!(re.is_match("a kiln here") && !re.is_match("akilns"));
        let re = compile(&w("[!a-c]@x{2}")).unwrap_or_else(|_| Regex::new("x").expect("re"));
        assert!(re.is_match("zzxx") && !re.is_match("abxx"));
        assert!(compile(&w("[abc")).is_err());
        assert!(compile(&w("a{x}")).is_err());
        assert!(
            compile(&FindQuery {
                regex: true,
                ..q("(")
            })
            .is_err()
        );
        assert!(compile(&q("")).is_err());
    }

    #[test]
    fn replacement_expansion() {
        let re = Regex::new("(k)(i)ln").expect("re");
        let caps = re.captures("kiln").expect("caps");
        assert_eq!(expand("^& \\2\\1 $1 ^^^t^p", &caps), "kiln ik k ^\t\n");
    }
}
