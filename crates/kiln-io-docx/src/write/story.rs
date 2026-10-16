// SPDX-License-Identifier: AGPL-3.0-or-later
//! Writing of one *story* (the body, a header or footer, a note, a comment, a text box): blocks
//! (paragraphs, tables, content controls, field results, kept blocks) and inline content (runs,
//! hyperlinks, tracked changes, fields, symbols, note references, bookmarks, comment ranges,
//! drawings, equations) — the inverse of `read::story`.
//!
//! Mapping:
//! - a paragraph's content is cut into segments wherever its formatting, marks, kept run
//!   properties or objects change; marks become wrappers (`w:sdt`, `w:hyperlink`, `w:ins`/`w:del`),
//!   shared by neighbouring segments, and `w:rPrChange` in the run properties;
//! - comment marks become `w:commentRangeStart`/`End` around the commented text of the story
//!   plus a `w:commentReference` run after the range;
//! - field objects become complex fields (`begin`, instruction, `separate`, cached result, `end`;
//!   `{ … }` in an instruction becomes a nested field) or `w:fldSimple`;
//! - field blocks (TOC, INDEX, …) become one field spanning their result paragraphs;
//! - tracked moves are written as an insertion and a deletion (the reader reports that the halves
//!   are no longer linked).

use std::collections::HashMap;

use kiln_doc::fields::FieldForm;
use kiln_doc::st::{BreakKind, NoteKind, NoteType};
use kiln_doc::{
    Block, Break, Cell, Field, FieldBlock, Hyperlink, InlineObject, LinkTarget, Mark, NodeId,
    NoteRef, OBJECT_CHAR, Paragraph, RevisionInfo, Row, RunProps, SdtInfo, Table,
};
use kiln_ooxml::opc::rel_type;

use super::context::WriteContext;
use super::props::{
    ParagraphPropsExtra, RunPropsExtra, cell_props_inner, paragraph_props_inner, revision_el,
    row_props_inner, run_props_inner, run_props_xml, table_props_inner,
};
use super::sdt::sdt_pr_xml;
use super::xml::{El, needs_preserve, text};
use crate::common::extras::{DocxExtras, RunExtras};

/// The deepest block nesting written (deeper content is kept as an empty paragraph); the reader
/// stops at 64 levels, so files Kiln reads never get near it.
const MAX_DEPTH: usize = 256;

/// Document-wide ids and lookups every story writer shares.
#[derive(Debug)]
pub(crate) struct StoryIds<'a> {
    /// `w:id` of every footnote and endnote.
    pub notes: HashMap<NodeId, String>,
    /// `w:id` of every comment.
    pub comments: HashMap<NodeId, String>,
    /// Whether the style sheet has the `CommentReference` character style.
    pub comment_reference_style: bool,
    /// The reader's extras.
    pub extras: &'a DocxExtras,
}

/// What a list of blocks must carry into its first or last paragraph.
#[derive(Debug, Default)]
pub(crate) struct BlockOpts {
    /// Run content prepended to the first paragraph (a comment's `w:annotationRef`, a field
    /// begin).
    pub first_prefix: Option<String>,
    /// Run content appended to the last paragraph (a field end).
    pub last_suffix: Option<String>,
    /// `w:sectPr` for the last paragraph (a section break).
    pub sect_pr: Option<String>,
    /// `w14:paraId` forced onto the last paragraph (comment threads).
    pub last_para_id: Option<String>,
}

#[derive(Debug, Default)]
struct ParaOpts {
    prefix: String,
    suffix: String,
    sect_pr: Option<String>,
    para_id: Option<String>,
}

/// A wrapper element implied by a mark.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Wrap<'p> {
    Sdt(&'p SdtInfo),
    Link(&'p Hyperlink),
    Ins(&'p RevisionInfo),
    Del(&'p RevisionInfo),
}

#[derive(Debug)]
enum Content<'p> {
    Text(String),
    Object(&'p InlineObject),
}

#[derive(Debug)]
struct Seg<'p> {
    start: usize,
    end: usize,
    props: &'p RunProps,
    change: Option<(&'p RevisionInfo, &'p RunProps)>,
    extra: Option<&'p RunExtras>,
    content: Content<'p>,
    chain: Vec<Wrap<'p>>,
}

impl Seg<'_> {
    fn deleted(&self) -> bool {
        self.chain.iter().any(|w| matches!(w, Wrap::Del(_)))
    }

    fn is_marker(&self) -> bool {
        matches!(
            self.content,
            Content::Object(InlineObject::BookmarkStart { .. } | InlineObject::BookmarkEnd { .. })
        )
    }
}

/// An open `<w:r>` being filled with simple content.
struct OpenRun {
    rpr: String,
    body: String,
}

/// A piece of a field instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstrPiece {
    /// Instruction text.
    Text(String),
    /// A nested field's instruction (written as `{ … }` by the reader).
    Nested(String),
}

/// Splits an instruction into text and nested-field pieces (`{ … }` as the reader writes them).
pub(crate) fn split_instruction(instr: &str) -> Vec<InstrPiece> {
    let chars: Vec<char> = instr.chars().collect();
    let mut out: Vec<InstrPiece> = Vec::new();
    let mut depth = 0usize;
    let mut buf = String::new();
    let mut nested = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&' ') {
            if depth > 0 {
                nested.push(c);
            }
            depth += 1;
        } else if c == '}' && depth > 0 && i > 0 && chars[i - 1] == ' ' {
            depth -= 1;
            if depth == 0 {
                if !buf.is_empty() {
                    out.push(InstrPiece::Text(std::mem::take(&mut buf)));
                }
                out.push(InstrPiece::Nested(nested.trim().to_owned()));
                nested.clear();
            } else {
                nested.push(c);
            }
        } else if depth > 0 {
            nested.push(c);
        } else {
            buf.push(c);
        }
        i += 1;
    }
    if depth > 0 {
        // Unbalanced: the braces were literal text.
        buf.push('{');
        buf.push_str(&nested);
    }
    if !buf.is_empty() {
        out.push(InstrPiece::Text(buf));
    }
    out
}

/// `w:t`/`w:delText`.
fn text_element(s: &str, deleted: bool) -> String {
    if s.is_empty() {
        return String::new();
    }
    let name = if deleted { "w:delText" } else { "w:t" };
    let space = if needs_preserve(s) {
        " xml:space=\"preserve\""
    } else {
        ""
    };
    format!("<{name}{space}>{}</{name}>", text(s))
}

/// Run content of plain text: `w:t` with tabs and line breaks as their elements.
fn text_content(s: &str, deleted: bool) -> String {
    let mut out = String::new();
    let mut buf = String::new();
    for c in s.chars() {
        match c {
            '\t' | '\n' | '\u{b}' | '\u{c}' => {
                out.push_str(&text_element(&buf, deleted));
                buf.clear();
                out.push_str(match c {
                    '\t' => "<w:tab/>",
                    '\u{b}' => "<w:br w:type=\"column\"/>",
                    '\u{c}' => "<w:br w:type=\"page\"/>",
                    _ => "<w:br/>",
                });
            }
            '\r' => {}
            _ => buf.push(c),
        }
    }
    out.push_str(&text_element(&buf, deleted));
    out
}

fn break_xml(b: &Break) -> String {
    let kind = match b.kind {
        BreakKind::TextWrapping => None,
        other => Some(other.as_str()),
    };
    El::new("w:br")
        .opt("w:type", kind)
        .opt("w:clear", b.clear.map(|c| c.as_str()))
        .empty()
}

fn sdt_open(info: &SdtInfo) -> String {
    format!(
        "<w:sdt>{}{}<w:sdtContent>",
        sdt_pr_xml(info),
        info.end_pr_xml.as_deref().unwrap_or_default()
    )
}

const SDT_CLOSE: &str = "</w:sdtContent></w:sdt>";

pub(crate) struct StoryWriter<'w, 'a> {
    pub ctx: &'w mut WriteContext<'a>,
    pub ids: &'w StoryIds<'a>,
    pub part: String,
    /// Inside a note: the note itself (its reference mark is `w:footnoteRef`/`w:endnoteRef`).
    note: Option<NodeId>,
    /// `(paragraph, offset)` → comment ids whose range starts / ends there.
    starts: HashMap<(NodeId, usize), Vec<String>>,
    ends: HashMap<(NodeId, usize), Vec<String>>,
    depth: usize,
}

impl std::fmt::Debug for StoryWriter<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoryWriter")
            .field("part", &self.part)
            .finish_non_exhaustive()
    }
}

/// Paragraphs of a story in document order (text boxes excluded: they are stories of their own).
fn story_paragraphs<'b>(blocks: &'b [Block], out: &mut Vec<&'b Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        story_paragraphs(&c.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => story_paragraphs(&s.blocks, out),
            Block::Field(f) => story_paragraphs(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

impl<'w, 'a> StoryWriter<'w, 'a> {
    pub(crate) fn new(
        ctx: &'w mut WriteContext<'a>,
        ids: &'w StoryIds<'a>,
        part: &str,
        note: Option<NodeId>,
    ) -> Self {
        Self {
            ctx,
            ids,
            part: part.to_owned(),
            note,
            starts: HashMap::new(),
            ends: HashMap::new(),
            depth: 0,
        }
    }

    /// Writes a story: its comment ranges are computed first.
    pub(crate) fn story(&mut self, blocks: &[Block], opts: BlockOpts) -> String {
        self.prepare_comments(&[blocks]);
        self.blocks(blocks, opts)
    }

    /// Writes the blocks of a nested story (a text box) in this part.
    pub(crate) fn nested_story(&mut self, blocks: &[Block]) -> String {
        let mut inner = StoryWriter::new(&mut *self.ctx, self.ids, &self.part, self.note);
        inner.depth = self.depth + 1;
        if inner.depth > MAX_DEPTH {
            return "<w:p/>".to_owned();
        }
        let mut xml = inner.story(blocks, BlockOpts::default());
        if !matches!(blocks.last(), Some(Block::Paragraph(_))) {
            xml.push_str("<w:p/>");
        }
        xml
    }

    /// Records where each comment range starts and ends in a story made of `parts` (the body's
    /// sections, or one block list).
    pub(crate) fn prepare_comments(&mut self, parts: &[&[Block]]) {
        let mut paras = Vec::new();
        for blocks in parts {
            story_paragraphs(blocks, &mut paras);
        }
        // comment id → ((paragraph index, start), (paragraph index, end), paragraph ids)
        let mut first: HashMap<String, (usize, usize, NodeId)> = HashMap::new();
        let mut last: HashMap<String, (usize, usize, NodeId)> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for (i, p) in paras.iter().enumerate() {
            for m in p.content.marks() {
                let Mark::Comment(c) = &m.mark else {
                    continue;
                };
                let Some(id) = self.ids.comments.get(c) else {
                    continue;
                };
                let f = first.entry(id.clone()).or_insert_with(|| {
                    order.push(id.clone());
                    (i, m.start, p.id)
                });
                if (i, m.start) < (f.0, f.1) {
                    *f = (i, m.start, p.id);
                }
                let l = last.entry(id.clone()).or_insert((i, m.end, p.id));
                if (i, m.end) > (l.0, l.1) {
                    *l = (i, m.end, p.id);
                }
            }
        }
        self.starts.clear();
        self.ends.clear();
        for id in order {
            if let Some((_, start, para)) = first.get(&id) {
                self.starts
                    .entry((*para, *start))
                    .or_default()
                    .push(id.clone());
            }
            if let Some((_, end, para)) = last.get(&id) {
                self.ends.entry((*para, *end)).or_default().push(id.clone());
            }
        }
    }

    /// A revision with an id (one is allocated for revisions that have none).
    fn revision(&mut self, r: &RevisionInfo) -> RevisionInfo {
        if r.id != 0 {
            return r.clone();
        }
        RevisionInfo {
            id: self.ctx.next_revision(),
            ..r.clone()
        }
    }

    // ---------------------------------------------------------------- blocks

    /// Writes blocks; `opts` content goes into the first/last paragraph, or into a paragraph of
    /// its own when the first/last block is not one.
    pub(crate) fn blocks(&mut self, blocks: &[Block], mut opts: BlockOpts) -> String {
        let mut out = String::new();
        if self.depth > MAX_DEPTH {
            return out;
        }
        self.depth += 1;
        if opts.first_prefix.is_some()
            && !matches!(
                blocks.first(),
                Some(Block::Paragraph(_) | Block::Sdt(_) | Block::Field(_))
            )
            && let Some(prefix) = opts.first_prefix.take()
        {
            out.push_str(&format!("<w:p>{prefix}</w:p>"));
        }
        let n = blocks.len();
        for (i, b) in blocks.iter().enumerate() {
            let last = i + 1 == n;
            let mut sub = BlockOpts {
                first_prefix: if i == 0 {
                    opts.first_prefix.take()
                } else {
                    None
                },
                last_suffix: if last { opts.last_suffix.take() } else { None },
                sect_pr: if last { opts.sect_pr.take() } else { None },
                last_para_id: if last { opts.last_para_id.take() } else { None },
            };
            out.push_str(&self.block(b, &mut sub));
            if last {
                opts.last_suffix = sub.last_suffix;
                opts.sect_pr = sub.sect_pr;
            }
        }
        if opts.last_suffix.is_some() || opts.sect_pr.is_some() {
            let ppr = opts
                .sect_pr
                .take()
                .map(|s| format!("<w:pPr>{s}</w:pPr>"))
                .unwrap_or_default();
            let suffix = opts.last_suffix.take().unwrap_or_default();
            out.push_str(&format!("<w:p>{ppr}{suffix}</w:p>"));
        }
        self.depth -= 1;
        out
    }

    /// Writes one block, taking from `opts` what it can carry.
    fn block(&mut self, b: &Block, opts: &mut BlockOpts) -> String {
        match b {
            Block::Paragraph(p) => self.paragraph(
                p,
                ParaOpts {
                    prefix: opts.first_prefix.take().unwrap_or_default(),
                    suffix: opts.last_suffix.take().unwrap_or_default(),
                    sect_pr: opts.sect_pr.take(),
                    para_id: opts.last_para_id.take(),
                },
            ),
            Block::Table(t) => self.table(t),
            Block::Sdt(s) => {
                let inner = self.blocks(&s.blocks, std::mem::take(opts));
                format!("{}{inner}{SDT_CLOSE}", sdt_open(&s.info))
            }
            Block::Field(f) => self.field_block(f, std::mem::take(opts)),
            Block::Opaque(o) => o.xml.clone(),
        }
    }

    /// A field whose result is whole paragraphs (TOC, INDEX, …).
    fn field_block(&mut self, f: &FieldBlock, mut opts: BlockOpts) -> String {
        let props = RunProps::default();
        let begin = self.field_begin(
            &f.field,
            &run_props_xml(&props, RunPropsExtra::default()),
            false,
        );
        let end = "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
        let prefix = opts.first_prefix.take().unwrap_or_default() + &begin;
        let suffix = format!("{end}{}", opts.last_suffix.take().unwrap_or_default());
        self.blocks(
            &f.blocks,
            BlockOpts {
                first_prefix: Some(prefix),
                last_suffix: Some(suffix),
                ..opts
            },
        )
    }

    // ---------------------------------------------------------------- paragraphs

    fn paragraph(&mut self, p: &Paragraph, o: ParaOpts) -> String {
        let extras = self.ids.extras.paragraphs.get(&p.id);
        let empty: &[String] = &[];
        let rev = &p.revision;
        let mark_rev = rev
            .mark
            .as_ref()
            .map(|m| kiln_doc::revision::InsDelRevision {
                info: self.revision(&m.info),
                kind: m.kind,
            });
        let mark_change = rev
            .mark_props_change
            .as_ref()
            .map(|c| (self.revision(&c.info), &c.before));
        let props_change = rev
            .props_change
            .as_ref()
            .map(|c| (self.revision(&c.info), &c.before));
        let mark = run_props_inner(
            &p.mark_props,
            RunPropsExtra {
                raw: extras.map_or(empty, |x| &x.mark_xml),
                font_hint: None,
                change: mark_change.as_ref().map(|(i, b)| (i, *b)),
                mark_revision: mark_rev.as_ref(),
            },
        );
        let ppr = paragraph_props_inner(
            &p.props,
            ParagraphPropsExtra {
                raw: extras.map_or(empty, |x| &x.ppr_xml),
                mark: Some(&mark),
                sect_pr: o.sect_pr.as_deref(),
                change: props_change.as_ref().map(|(i, b)| (i, *b)),
            },
        );
        let para_id = o
            .para_id
            .as_deref()
            .or_else(|| extras.and_then(|x| x.para_id.as_deref()));
        let text_id = extras.and_then(|x| x.text_id.as_deref());
        let content = format!("{}{}{}", o.prefix, self.inline(p), o.suffix);
        let el = El::new("w:p")
            .opt("w14:paraId", para_id)
            .opt("w14:textId", text_id);
        let ppr = if ppr.is_empty() {
            String::new()
        } else {
            format!("<w:pPr>{ppr}</w:pPr>")
        };
        el.wrap(&format!("{ppr}{content}"))
    }

    // ---------------------------------------------------------------- inline

    fn segments<'p>(&self, p: &'p Paragraph) -> Vec<Seg<'p>>
    where
        'a: 'p,
    {
        let c = &p.content;
        let len = c.len();
        if len == 0 {
            return Vec::new();
        }
        let run_extras: &'p [RunExtras] = self
            .ids
            .extras
            .paragraphs
            .get(&p.id)
            .map_or(&[], |x| &x.runs);
        let mut cuts: Vec<usize> = vec![0, len];
        let mut pos = 0;
        for r in c.runs() {
            pos += r.len;
            cuts.push(pos.min(len));
        }
        for m in c.marks() {
            cuts.push(m.start.min(len));
            cuts.push(m.end.min(len));
        }
        for x in run_extras {
            cuts.push(x.start.min(len));
            cuts.push(x.end.min(len));
        }
        for (i, _) in c.objects_with_offsets() {
            cuts.push(i);
            cuts.push(i + 1);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let rope = c.rope();
        let mut segs = Vec::with_capacity(cuts.len());
        let mut object_index = 0usize;
        let mut run_index = 0usize;
        let mut run_end = c.runs().first().map_or(len, |r| r.len);
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= b {
                continue;
            }
            while a >= run_end && run_index + 1 < c.runs().len() {
                run_index += 1;
                run_end += c.runs()[run_index].len;
            }
            let Some(run) = c.runs().get(run_index) else {
                continue;
            };
            let content = if b == a + 1 && rope.char(a) == OBJECT_CHAR {
                let Some(o) = c.objects().get(object_index) else {
                    continue;
                };
                object_index += 1;
                Content::Object(o)
            } else {
                Content::Text(
                    rope.slice(a..b)
                        .chars()
                        .filter(|ch| *ch != OBJECT_CHAR)
                        .collect(),
                )
            };
            let mut chain: Vec<Wrap<'p>> = Vec::new();
            let mut change = None;
            let covering = c.marks().iter().filter(|m| m.start <= a && m.end >= b);
            let mut link = None;
            let mut revs = Vec::new();
            for m in covering {
                match &m.mark {
                    Mark::Sdt(info) => chain.push(Wrap::Sdt(info)),
                    Mark::Link(l) => link = link.or(Some(l)),
                    Mark::Insertion(r) | Mark::MoveTo(r) => revs.push(Wrap::Ins(r)),
                    Mark::Deletion(r) | Mark::MoveFrom(r) => revs.push(Wrap::Del(r)),
                    Mark::FormatChange { info, before } => change = Some((info, &**before)),
                    Mark::Comment(_) => {}
                }
            }
            if let Some(l) = link {
                chain.push(Wrap::Link(l));
            }
            chain.extend(revs);
            let extra = run_extras.iter().find(|x| x.start <= a && x.end >= b);
            segs.push(Seg {
                start: a,
                end: b,
                props: &run.props,
                change,
                extra,
                content,
                chain,
            });
        }
        // Zero-width markers take the wrappers their neighbours share, so they do not split a
        // hyperlink or a tracked change.
        let chains: Vec<Vec<Wrap<'p>>> = segs.iter().map(|s| s.chain.clone()).collect();
        for i in 0..segs.len() {
            if !segs[i].is_marker() || !segs[i].chain.is_empty() {
                continue;
            }
            let prev = (0..i)
                .rev()
                .find(|k| !segs[*k].is_marker())
                .map(|k| &chains[k]);
            let next = (i + 1..segs.len())
                .find(|k| !segs[*k].is_marker())
                .map(|k| &chains[k]);
            if let (Some(prev), Some(next)) = (prev, next) {
                let common = prev
                    .iter()
                    .zip(next.iter())
                    .take_while(|(x, y)| x == y)
                    .count();
                segs[i].chain = prev[..common].to_vec();
            }
        }
        segs
    }

    /// The paragraph's inline content.
    fn inline(&mut self, p: &Paragraph) -> String {
        let segs = self.segments(p);
        let mut out = String::new();
        self.group(p.id, &segs, 0, &mut out);
        // Comment ranges that end after the last segment (or in an empty paragraph).
        let len = p.content.len();
        if segs.is_empty() {
            self.comment_starts(p.id, 0, &mut out);
        }
        self.comment_ends(p.id, len, &mut out);
        out
    }

    fn comment_starts(&mut self, para: NodeId, at: usize, out: &mut String) {
        if let Some(ids) = self.starts.remove(&(para, at)) {
            for id in ids {
                out.push_str(&El::new("w:commentRangeStart").attr("w:id", id).empty());
            }
        }
    }

    fn comment_ends(&mut self, para: NodeId, at: usize, out: &mut String) {
        if let Some(ids) = self.ends.remove(&(para, at)) {
            for id in ids {
                out.push_str(&El::new("w:commentRangeEnd").attr("w:id", &id).empty());
                let style = if self.ids.comment_reference_style {
                    "<w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr>"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "<w:r>{style}{}</w:r>",
                    El::new("w:commentReference").attr("w:id", &id).empty()
                ));
            }
        }
    }

    fn open_wrap(&mut self, w: Wrap<'_>) -> (String, &'static str) {
        match w {
            Wrap::Sdt(info) => (sdt_open(info), SDT_CLOSE),
            Wrap::Link(l) => (self.hyperlink_open(l), "</w:hyperlink>"),
            Wrap::Ins(r) => {
                let r = self.revision(r);
                (revision_el("w:ins", &r).open(), "</w:ins>")
            }
            Wrap::Del(r) => {
                let r = self.revision(r);
                (revision_el("w:del", &r).open(), "</w:del>")
            }
        }
    }

    fn hyperlink_open(&mut self, l: &Hyperlink) -> String {
        let (rel, anchor) = match &l.target {
            LinkTarget::Url(u) => {
                let internal = u.starts_with('/') && self.ctx.pkg.has(u);
                let part = self.part.clone();
                (
                    Some(self.ctx.rel(&part, rel_type::HYPERLINK, u, !internal)),
                    None,
                )
            }
            LinkTarget::Bookmark(a) => (None, Some(a.as_str())),
        };
        El::new("w:hyperlink")
            .opt("r:id", rel)
            .opt("w:anchor", anchor)
            .opt("w:tooltip", l.tooltip.as_deref())
            .opt("w:tgtFrame", l.frame.as_deref())
            .opt("w:history", l.history.then_some("1"))
            .open()
    }

    fn group(&mut self, para: NodeId, segs: &[Seg<'_>], depth: usize, out: &mut String) {
        let mut run: Option<OpenRun> = None;
        let flush = |run: &mut Option<OpenRun>, out: &mut String| {
            if let Some(r) = run.take() {
                out.push_str("<w:r>");
                out.push_str(&r.rpr);
                out.push_str(&r.body);
                out.push_str("</w:r>");
            }
        };
        let mut i = 0;
        while i < segs.len() {
            let s = &segs[i];
            if let Some(w) = s.chain.get(depth).copied() {
                flush(&mut run, out);
                let mut j = i + 1;
                while j < segs.len() && segs[j].chain.get(depth) == Some(&w) {
                    j += 1;
                }
                let (open, close) = self.open_wrap(w);
                out.push_str(&open);
                self.group(para, &segs[i..j], depth + 1, out);
                out.push_str(close);
                i = j;
                continue;
            }
            if self.starts.contains_key(&(para, s.start)) {
                flush(&mut run, out);
                self.comment_starts(para, s.start, out);
            }
            let deleted = s.deleted();
            let rpr = self.run_props(s);
            match &s.content {
                Content::Text(t) => {
                    let body = text_content(t, deleted);
                    match &mut run {
                        Some(r) if r.rpr == rpr => r.body.push_str(&body),
                        _ => {
                            flush(&mut run, out);
                            run = Some(OpenRun { rpr, body });
                        }
                    }
                }
                Content::Object(o) => match self.simple_object(o) {
                    Some(body) => match &mut run {
                        Some(r) if r.rpr == rpr => r.body.push_str(&body),
                        _ => {
                            flush(&mut run, out);
                            run = Some(OpenRun { rpr, body });
                        }
                    },
                    None => {
                        flush(&mut run, out);
                        let xml = self.object(o, &rpr, deleted);
                        out.push_str(&xml);
                    }
                },
            }
            if self.ends.contains_key(&(para, s.end)) {
                flush(&mut run, out);
                self.comment_ends(para, s.end, out);
            }
            i += 1;
        }
        flush(&mut run, out);
    }

    fn run_props(&mut self, s: &Seg<'_>) -> String {
        let change = s.change.map(|(info, before)| (self.revision(info), before));
        run_props_xml(
            s.props,
            RunPropsExtra {
                raw: s.extra.map_or(&[], |x| &x.xml),
                font_hint: s.extra.and_then(|x| x.font_hint.as_deref()),
                change: change.as_ref().map(|(i, b)| (i, *b)),
                mark_revision: None,
            },
        )
    }

    /// Run content of objects that share a run with text (breaks and symbols).
    fn simple_object(&self, o: &InlineObject) -> Option<String> {
        match o {
            InlineObject::Break(b) => Some(break_xml(b)),
            InlineObject::Symbol(s) => Some(
                El::new("w:sym")
                    .attr("w:font", &s.font)
                    .attr("w:char", format!("{:04X}", s.code))
                    .empty(),
            ),
            _ => None,
        }
    }

    /// Objects written on their own: fields, note references, drawings, equations, bookmarks,
    /// kept objects.
    fn object(&mut self, o: &InlineObject, rpr: &str, deleted: bool) -> String {
        match o {
            InlineObject::Field(f) => self.field(f, rpr, deleted),
            InlineObject::NoteRef(n) => self.note_ref(n, rpr),
            InlineObject::Drawing(d) => match self.drawing(d) {
                Some(xml) => format!("<w:r>{rpr}{xml}</w:r>"),
                None => String::new(),
            },
            InlineObject::Equation(e) => {
                if e.omml.trim().is_empty() {
                    let part = self.part.clone();
                    self.ctx.issue(
                        "equation.unsaved",
                        "An equation without Office Math markup was not saved.",
                        &part,
                    );
                }
                e.omml.clone()
            }
            InlineObject::BookmarkStart { name } => {
                let id = self.ctx.bookmark_start(name);
                El::new("w:bookmarkStart")
                    .attr("w:id", id)
                    .attr("w:name", name)
                    .empty()
            }
            InlineObject::BookmarkEnd { name } => match self.ctx.bookmark_end(name) {
                Some(id) => El::new("w:bookmarkEnd").attr("w:id", id).empty(),
                None => String::new(),
            },
            InlineObject::Opaque { xml, .. } => {
                if xml.is_empty() {
                    String::new()
                } else {
                    format!("<w:r>{rpr}{xml}</w:r>")
                }
            }
            InlineObject::Break(_) | InlineObject::Symbol(_) => {
                let body = self.simple_object(o).unwrap_or_default();
                format!("<w:r>{rpr}{body}</w:r>")
            }
        }
    }

    fn note_ref(&mut self, n: &NoteRef, rpr: &str) -> String {
        let kind = match n.kind {
            NoteKind::Footnote => "footnote",
            NoteKind::Endnote => "endnote",
        };
        if self.note == Some(n.note) {
            return format!("<w:r>{rpr}<w:{kind}Ref/></w:r>");
        }
        let Some(id) = self.ids.notes.get(&n.note).cloned() else {
            let part = self.part.clone();
            self.ctx.issue(
                "note.missing",
                format!("A {kind} reference without its {kind} was removed."),
                &part,
            );
            return String::new();
        };
        let reference = El::new(&format!("w:{kind}Reference"))
            .opt("w:customMarkFollows", n.custom_mark.as_ref().map(|_| "1"))
            .attr("w:id", id)
            .empty();
        let mark = n
            .custom_mark
            .as_deref()
            .map(|m| text_element(m, false))
            .unwrap_or_default();
        format!("<w:r>{rpr}{reference}{mark}</w:r>")
    }

    // ---------------------------------------------------------------- fields

    fn field_begin(&mut self, f: &Field, rpr: &str, deleted: bool) -> String {
        let begin = El::new("w:fldChar")
            .attr("w:fldCharType", "begin")
            .opt("w:dirty", f.dirty.then_some("true"))
            .opt("w:fldLock", f.locked.then_some("true"))
            .empty();
        let mut out = format!("<w:r>{rpr}{begin}</w:r>");
        out.push_str(&self.instruction(&format!(" {} ", f.instr.trim()), rpr, deleted));
        out.push_str(&format!(
            "<w:r>{rpr}<w:fldChar w:fldCharType=\"separate\"/></w:r>"
        ));
        out
    }

    fn instruction(&mut self, instr: &str, rpr: &str, deleted: bool) -> String {
        let name = if deleted {
            "w:delInstrText"
        } else {
            "w:instrText"
        };
        let mut out = String::new();
        for piece in split_instruction(instr) {
            match piece {
                InstrPiece::Text(t) => out.push_str(&format!(
                    "<w:r>{rpr}<{name} xml:space=\"preserve\">{}</{name}></w:r>",
                    text(&t)
                )),
                InstrPiece::Nested(nested) => {
                    out.push_str(&format!(
                        "<w:r>{rpr}<w:fldChar w:fldCharType=\"begin\"/></w:r>"
                    ));
                    out.push_str(&self.instruction(&format!(" {nested} "), rpr, deleted));
                    out.push_str(&format!(
                        "<w:r>{rpr}<w:fldChar w:fldCharType=\"end\"/></w:r>"
                    ));
                }
            }
        }
        out
    }

    fn field(&mut self, f: &Field, rpr: &str, deleted: bool) -> String {
        if f.form == FieldForm::Simple {
            if f.result.is_empty() && f.instr.trim().eq_ignore_ascii_case("PAGE") {
                // `w:pgNum`, as the reader found it.
                return format!("<w:r>{rpr}<w:pgNum/></w:r>");
            }
            let result = if f.result.is_empty() {
                String::new()
            } else {
                format!("<w:r>{rpr}{}</w:r>", text_content(&f.result, deleted))
            };
            return El::new("w:fldSimple")
                .attr("w:instr", format!(" {} ", f.instr.trim()))
                .opt("w:fldLock", f.locked.then_some("1"))
                .opt("w:dirty", f.dirty.then_some("1"))
                .wrap(&result);
        }
        let mut out = self.field_begin(f, rpr, deleted);
        if !f.result.is_empty() {
            out.push_str(&format!(
                "<w:r>{rpr}{}</w:r>",
                text_content(&f.result, deleted)
            ));
        }
        out.push_str(&format!(
            "<w:r>{rpr}<w:fldChar w:fldCharType=\"end\"/></w:r>"
        ));
        out
    }

    // ---------------------------------------------------------------- tables

    fn table(&mut self, t: &Table) -> String {
        let empty: Vec<String> = Vec::new();
        let raw = self.ids.extras.tables.get(&t.id).unwrap_or(&empty);
        let tblpr = table_props_inner(&t.props, raw);
        let grid = table_grid(t);
        let grid_xml: String = grid
            .iter()
            .map(|w| El::new("w:gridCol").attr("w:w", (*w).max(0)).empty())
            .collect();
        let rows: Vec<&Row> = t.rows.iter().collect();
        let extras = self.ids.extras;
        let body = group_sdt(
            &rows,
            &|r: &&Row| extras.rows.get(&r.id).map_or(&[][..], |x| &x.sdt[..]),
            0,
            &mut |r: &&Row| self.row(r),
        );
        format!(
            "<w:tbl>{}<w:tblGrid>{grid_xml}</w:tblGrid>{body}</w:tbl>",
            if tblpr.is_empty() {
                "<w:tblPr/>".to_owned()
            } else {
                format!("<w:tblPr>{tblpr}</w:tblPr>")
            }
        )
    }

    fn row(&mut self, r: &Row) -> String {
        let x = self.ids.extras.rows.get(&r.id);
        let empty: Vec<String> = Vec::new();
        let revision = r
            .revision
            .as_ref()
            .map(|rv| kiln_doc::revision::InsDelRevision {
                info: self.revision(&rv.info),
                kind: rv.kind,
            });
        let trpr = row_props_inner(&r.props, x.map_or(&empty, |x| &x.xml), revision.as_ref());
        let tbl_pr_ex = x.and_then(|x| x.tbl_pr_ex.clone()).unwrap_or_default();
        let cells: Vec<&Cell> = r.cells.iter().collect();
        let extras = self.ids.extras;
        let body = group_sdt(
            &cells,
            &|c: &&Cell| extras.cells.get(&c.id).map_or(&[][..], |x| &x.sdt[..]),
            0,
            &mut |c: &&Cell| self.cell(c),
        );
        let trpr = if trpr.is_empty() {
            String::new()
        } else {
            format!("<w:trPr>{trpr}</w:trPr>")
        };
        format!("<w:tr>{tbl_pr_ex}{trpr}{body}</w:tr>")
    }

    fn cell(&mut self, c: &Cell) -> String {
        let empty: Vec<String> = Vec::new();
        let raw = self.ids.extras.cells.get(&c.id).map_or(&empty, |x| &x.xml);
        let tcpr = cell_props_inner(&c.props, c.grid_span, c.v_merge, raw);
        let mut content;
        if self.keeps_file_cell_end(c) {
            content = self.blocks(&c.blocks[..c.blocks.len() - 1], BlockOpts::default());
        } else {
            content = self.blocks(&c.blocks, BlockOpts::default());
            if !matches!(c.blocks.last(), Some(Block::Paragraph(_))) {
                // Word requires every cell to end with a paragraph.
                content.push_str("<w:p/>");
            }
        }
        let tcpr = if tcpr.is_empty() {
            String::new()
        } else {
            format!("<w:tcPr>{tcpr}</w:tcPr>")
        };
        format!("<w:tc>{tcpr}{content}</w:tc>")
    }

    /// True when the cell's last block is an end-of-cell paragraph the reader added after a
    /// nested table (see [`DocxExtras::cell_end_paragraphs`]) and it is still empty, unformatted
    /// and after a table: the cell is then written as the file had it, ending with the table.
    fn keeps_file_cell_end(&self, c: &Cell) -> bool {
        matches!(
            &c.blocks[..],
            [.., Block::Table(_) | Block::Opaque(_), Block::Paragraph(p)]
                if self.ids.extras.cell_end_paragraphs.contains(&p.id)
                    && !self.ids.extras.paragraphs.contains_key(&p.id)
                    && *p == Paragraph::new(p.id)
        )
    }
}

/// Wraps runs of consecutive items sharing a content control at `depth` of their control chain
/// (outermost first) in `w:sdt`, recursively.
fn group_sdt<T>(
    items: &[T],
    chain: &dyn Fn(&T) -> &[SdtInfo],
    depth: usize,
    emit: &mut dyn FnMut(&T) -> String,
) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < items.len() {
        let Some(info) = chain(&items[i]).get(depth) else {
            out.push_str(&emit(&items[i]));
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < items.len() && chain(&items[j]).get(depth) == Some(info) {
            j += 1;
        }
        out.push_str(&sdt_open(info));
        out.push_str(&group_sdt(&items[i..j], chain, depth + 1, emit));
        out.push_str(SDT_CLOSE);
        i = j;
    }
    out
}

/// Grid column widths: the stored grid, completed (with the cells' widths, else an even share of
/// a 6.5" text width) when it has fewer columns than the rows use.
pub(crate) fn table_grid(t: &Table) -> Vec<i32> {
    let cols = t.grid_width().max(1);
    if t.grid.len() >= cols {
        return t.grid.clone();
    }
    let mut grid: Vec<Option<i32>> = (0..cols).map(|c| t.grid.get(c).copied()).collect();
    for slot in t.cell_map() {
        let Some(cell) = t.rows.get(slot.row).and_then(|r| r.cells.get(slot.cell)) else {
            continue;
        };
        if let Some(w) = cell.props.width
            && w.kind == kiln_doc::st::WidthType::Dxa
            && w.value > 0
            && slot.col_span > 0
        {
            let each = w.value / i32::try_from(slot.col_span).unwrap_or(1);
            let end = (slot.col + slot.col_span).min(cols);
            for g in grid.iter_mut().take(end).skip(slot.col) {
                if g.is_none() {
                    *g = Some(each);
                }
            }
        }
    }
    let used: i32 = grid.iter().flatten().sum();
    let missing = grid.iter().filter(|g| g.is_none()).count();
    let each = if missing == 0 {
        0
    } else {
        ((9360 - used).max(360 * i32::try_from(missing).unwrap_or(1)))
            / i32::try_from(missing).unwrap_or(1)
    };
    grid.into_iter().map(|g| g.unwrap_or(each)).collect()
}

/// Content of a note: separators get their separator run back (the reader does not keep it).
pub(crate) fn separator_run(t: NoteType) -> Option<&'static str> {
    match t {
        NoteType::Separator => Some("<w:r><w:separator/></w:r>"),
        NoteType::ContinuationSeparator => Some("<w:r><w:continuationSeparator/></w:r>"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_split_nested_fields() {
        let p = split_instruction(" IF { MERGEFIELD Name } = \"x\" \"a\" \"b\" ");
        let t = |s: &str| InstrPiece::Text(s.to_owned());
        let n = |s: &str| InstrPiece::Nested(s.to_owned());
        assert_eq!(
            p,
            vec![t(" IF "), n("MERGEFIELD Name"), t(" = \"x\" \"a\" \"b\" ")]
        );
        let deep = split_instruction("A { B { C } } D");
        assert_eq!(deep, vec![t("A "), n("B { C }"), t(" D")]);
        assert_eq!(split_instruction("{x}"), vec![t("{x}")]);
        assert_eq!(split_instruction("A { B"), vec![t("A { B")]);
    }

    #[test]
    fn text_uses_tab_and_break_elements() {
        assert_eq!(
            text_content("a\tb\nc", false),
            "<w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t>"
        );
        assert_eq!(
            text_content(" x", true),
            "<w:delText xml:space=\"preserve\"> x</w:delText>"
        );
        assert_eq!(text_content("", false), "");
    }
}
