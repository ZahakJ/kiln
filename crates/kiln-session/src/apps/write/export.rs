// SPDX-License-Identifier: AGPL-3.0-or-later
//! Markdown and HTML export of Kiln Write documents (File ▸ Export, Save As): the structure of
//! the document, not its look. Headings, paragraphs, lists with their nesting, quotes, Kiln code
//! blocks, tables, links, pictures (embedded as `data:` URLs, so the file stands alone),
//! footnotes and endnotes, line breaks and horizontal rules; bold, italic, strikethrough, code,
//! underline, subscript and superscript. Deleted tracked text is left out (the document as if
//! every change were accepted), and so are comments, headers and footers.
//!
//! Markdown is CommonMark with GitHub's tables, strikethrough and footnotes; HTML is a single
//! HTML5 file with semantic elements and a short style sheet for reading.

use std::collections::HashMap;
use std::fmt::Write as _;

use kiln_common::NodeId;
use kiln_doc::st::{Alignment, BreakKind, NoteKind, VMerge, VertAlign};
use kiln_doc::{
    Block, Document, DrawingKind, InlineObject, LinkTarget, ListLabel, Mark, Paragraph, Table,
};

/// What a run of text looks like, as far as the structure goes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Look {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    code: bool,
    sup: bool,
    sub: bool,
    link: Option<String>,
}

/// A piece of paragraph content.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String, Look),
    LineBreak,
    Image { src: String, alt: String },
    Note(usize),
}

/// The kind of block a paragraph is, for grouping.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Heading(u8),
    Para,
    Quote,
    Code,
    Rule,
    Item { ordered: bool, level: u8 },
}

const MONO: [&str; 9] = [
    "consolas",
    "courier new",
    "courier",
    "liberation mono",
    "menlo",
    "monaco",
    "cascadia code",
    "cascadia mono",
    "source code pro",
];

/// The walk over the document shared by both formats.
struct Walker<'a> {
    doc: &'a Document,
    labels: HashMap<NodeId, ListLabel>,
    /// Notes in the order of their first reference: (note id, kind).
    notes: Vec<NodeId>,
    note_numbers: HashMap<NodeId, usize>,
    /// The note being written (its own reference mark is left out).
    in_note: Option<NodeId>,
    /// The anchor of each heading: GitHub's slug of its text, numbered when repeated (what
    /// Markdown renderers give headings; the HTML export uses the same ids).
    heading_ids: HashMap<NodeId, String>,
    /// Where links to a bookmark go: its heading's anchor, else the bookmark's own name.
    bookmark_targets: HashMap<String, String>,
    /// Bookmarks outside headings, written as empty anchors at their paragraph's start.
    para_anchors: HashMap<NodeId, Vec<String>>,
}

/// GitHub's heading slug: lower case, letters, digits, `-` and `_` kept, spaces as `-`.
fn slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

impl<'a> Walker<'a> {
    fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            labels: doc.list_labels(),
            notes: Vec::new(),
            note_numbers: HashMap::new(),
            in_note: None,
            heading_ids: HashMap::new(),
            bookmark_targets: HashMap::new(),
            para_anchors: HashMap::new(),
        }
        .with_anchors()
    }

    /// Anchors for headings and bookmarks, in document order.
    fn with_anchors(mut self) -> Self {
        let mut used: HashMap<String, usize> = HashMap::new();
        for (_, p) in self.doc.all_paragraphs() {
            let heading = matches!(self.kind(p), Kind::Heading(_));
            if heading {
                let text: String = p
                    .content
                    .plain_text(false)
                    .chars()
                    .filter(|c| *c != kiln_doc::OBJECT_CHAR)
                    .collect();
                let base = slug(&text);
                let n = used.entry(base.clone()).or_insert(0);
                let id = if *n == 0 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                };
                *n += 1;
                self.heading_ids.insert(p.id, id);
            }
            for o in p.content.objects() {
                if let InlineObject::BookmarkStart { name } = o {
                    match self.heading_ids.get(&p.id) {
                        Some(id) if heading => {
                            self.bookmark_targets.insert(name.clone(), id.clone());
                        }
                        _ => {
                            self.bookmark_targets.insert(name.clone(), name.clone());
                            self.para_anchors
                                .entry(p.id)
                                .or_default()
                                .push(name.clone());
                        }
                    }
                }
            }
        }
        self
    }

    /// The `#fragment` of a link to a bookmark.
    fn bookmark_href(&self, name: &str) -> String {
        format!(
            "#{}",
            self.bookmark_targets.get(name).map_or(name, String::as_str)
        )
    }

    /// Empty anchors for the bookmarks of a paragraph that is not a heading.
    fn anchor_html(&self, p: &Paragraph) -> String {
        let mut out = String::new();
        for name in self.para_anchors.get(&p.id).into_iter().flatten() {
            out.push_str("<a id=\"");
            esc(name, &mut out);
            out.push_str("\"></a>");
        }
        out
    }

    fn kind(&self, p: &Paragraph) -> Kind {
        let r = self.doc.resolve_paragraph(p);
        let style = r.style_id.as_str();
        if style == super::features::blocks::CODE_STYLE {
            return Kind::Code;
        }
        if let Some(l) = self.labels.get(&p.id) {
            return Kind::Item {
                ordered: !l.is_bullet,
                level: l.level,
            };
        }
        if style == "Title" {
            return Kind::Heading(1);
        }
        if r.outline_level < 9 {
            return Kind::Heading((r.outline_level + 1).min(6));
        }
        if matches!(style, "Quote" | "IntenseQuote") {
            return Kind::Quote;
        }
        let text = p.content.plain_text(true);
        if text.trim().is_empty() && p.props.borders.as_ref().is_some_and(|b| b.bottom.is_some()) {
            return Kind::Rule;
        }
        Kind::Para
    }

    /// The look of the paragraph's own text without direct formatting: a heading or a table's
    /// header row is bold by its style, which the structure already says.
    fn base(&self, p: &Paragraph) -> kiln_doc::ResolvedRun {
        let table = self.doc.table_context(p.id);
        kiln_doc::resolve::resolve_run(
            &self.doc.context(),
            &kiln_doc::resolve::RunInput {
                paragraph: &p.props,
                mark_props: &p.mark_props,
                direct: &kiln_doc::RunProps::default(),
                table: table.as_ref(),
                target: kiln_doc::resolve::RunTarget::Text,
            },
        )
    }

    /// The look at an offset (`None`: hidden text, left out).
    fn look_at(
        &self,
        p: &Paragraph,
        base: &kiln_doc::ResolvedRun,
        offset: usize,
        link: Option<&LinkTarget>,
    ) -> Option<Look> {
        let r = self.doc.resolve_run_at(p, offset);
        if r.hidden {
            return None;
        }
        let font = r.fonts.ascii.to_lowercase();
        Some(Look {
            bold: r.bold && !base.bold,
            italic: r.italic && !base.italic,
            strike: r.strike || r.dstrike,
            underline: r.underline.is_some() && link.is_none() && base.underline.is_none(),
            code: MONO.contains(&font.as_str()) || r.style_id.as_deref() == Some("HTMLCode"),
            sup: r.vert_align == VertAlign::Superscript,
            sub: r.vert_align == VertAlign::Subscript,
            link: link.map(|t| match t {
                LinkTarget::Url(u) => u.clone(),
                LinkTarget::Bookmark(b) => self.bookmark_href(b),
            }),
        })
    }

    /// The paragraph's content as pieces, deleted text left out.
    fn pieces(&mut self, p: &Paragraph) -> Vec<Piece> {
        let content = &p.content;
        let chars: Vec<char> = content.raw_text().chars().collect();
        let marks = content.marks();
        let deleted = |i: usize| {
            marks.iter().any(|m| {
                m.start <= i && i < m.end && matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_))
            })
        };
        let link_at = |i: usize| {
            marks.iter().find_map(|m| match &m.mark {
                Mark::Link(h) if m.start <= i && i < m.end => Some(&h.target),
                _ => None,
            })
        };
        let mut out: Vec<Piece> = Vec::new();
        let mut object = 0usize;
        // Where runs start: the look is resolved again there and where a link starts or ends.
        let mut starts = Vec::with_capacity(content.runs().len());
        let mut at = 0usize;
        for r in content.runs() {
            starts.push(at);
            at += r.len;
        }
        let base = self.base(p);
        let mut run_look: Option<Look> = None;
        let mut run_link: Option<&LinkTarget> = None;
        let push_text = |out: &mut Vec<Piece>, s: &str, look: &Look| {
            if let Some(Piece::Text(t, l)) = out.last_mut()
                && l == look
            {
                t.push_str(s);
                return;
            }
            out.push(Piece::Text(s.to_owned(), look.clone()));
        };
        for (i, &c) in chars.iter().enumerate() {
            let link = link_at(i);
            if i == 0 || link != run_link || starts.binary_search(&i).is_ok() {
                run_look = self.look_at(p, &base, i, link);
                run_link = link;
            }
            let Some(run_look) = run_look.as_ref() else {
                if c == kiln_doc::OBJECT_CHAR {
                    object += 1;
                }
                continue;
            };
            if c == kiln_doc::OBJECT_CHAR {
                let o = content.objects().get(object);
                object += 1;
                if deleted(i) {
                    continue;
                }
                match o {
                    Some(InlineObject::Break(b)) => {
                        if b.kind == BreakKind::TextWrapping {
                            out.push(Piece::LineBreak);
                        }
                    }
                    Some(InlineObject::Drawing(d)) => {
                        if let DrawingKind::Picture { media, .. } = &d.kind
                            && let Some(m) = self.doc.media.get(media)
                        {
                            out.push(Piece::Image {
                                src: format!(
                                    "data:{};base64,{}",
                                    m.content_type,
                                    crate::b64::encode(&m.data)
                                ),
                                alt: d
                                    .description
                                    .clone()
                                    .or_else(|| d.title.clone())
                                    .unwrap_or_default(),
                            });
                        } else if let Some(blocks) = &d.text_box {
                            // A text box's text joins the paragraph it is anchored in.
                            let text = blocks
                                .iter()
                                .filter_map(|b| match b {
                                    Block::Paragraph(q) => Some(q.content.plain_text(true)),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            if !text.trim().is_empty() {
                                push_text(&mut out, &text, run_look);
                            }
                        }
                    }
                    Some(InlineObject::NoteRef(n)) if Some(n.note) == self.in_note => {}
                    Some(InlineObject::NoteRef(n)) => {
                        let next = self.notes.len() + 1;
                        let k = *self.note_numbers.entry(n.note).or_insert(next);
                        if k == next {
                            self.notes.push(n.note);
                        }
                        out.push(Piece::Note(k));
                    }
                    Some(InlineObject::Symbol(s)) => {
                        let ch = kiln_io_docx::symbol_code_to_unicode(&s.font, s.code)
                            .unwrap_or('\u{FFFD}');
                        push_text(&mut out, &ch.to_string(), run_look);
                    }
                    Some(o) => {
                        let t = o.plain_text();
                        if !t.is_empty() {
                            push_text(&mut out, &t, run_look);
                        }
                    }
                    None => {}
                }
                continue;
            }
            if deleted(i) {
                continue;
            }
            let mut buf = [0u8; 4];
            let s: &str = if c == '\t' {
                " "
            } else {
                c.encode_utf8(&mut buf)
            };
            push_text(&mut out, s, run_look);
        }
        out
    }

    fn note(&self, id: NodeId) -> Option<&kiln_doc::Note> {
        self.doc.notes.iter().find(|n| n.id == id)
    }
}

fn note_is_end(w: &Walker<'_>, id: NodeId) -> bool {
    w.note(id).is_some_and(|n| n.kind == NoteKind::Endnote)
}

// ------------------------------------------------------------------------------------------
// Markdown
// ------------------------------------------------------------------------------------------

fn md_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '|' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
}

fn md_inline(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for p in pieces {
        match p {
            Piece::Text(t, l) => {
                // Emphasis markers must hug the text: spaces move outside them.
                let lead = t.len() - t.trim_start().len();
                let trail = t.len() - t.trim_end().len();
                let core = t.trim();
                out.push_str(&t[..lead]);
                if core.is_empty() {
                    continue;
                }
                let mut open = String::new();
                let mut close = String::new();
                if l.link.is_some() {
                    open.push('[');
                }
                if l.bold {
                    open.push_str("**");
                    close.insert_str(0, "**");
                }
                if l.italic {
                    open.push('*');
                    close.insert(0, '*');
                }
                if l.strike {
                    open.push_str("~~");
                    close.insert_str(0, "~~");
                }
                if l.sup {
                    open.push_str("<sup>");
                    close.insert_str(0, "</sup>");
                }
                if l.sub {
                    open.push_str("<sub>");
                    close.insert_str(0, "</sub>");
                }
                out.push_str(&open);
                if l.code {
                    let fence = if core.contains('`') { "`` " } else { "`" };
                    out.push_str(fence);
                    out.push_str(core);
                    out.push_str(&fence.chars().rev().collect::<String>());
                } else {
                    md_escape(core, &mut out);
                }
                out.push_str(&close);
                if let Some(href) = &l.link {
                    let _ = write!(out, "]({})", href.replace(' ', "%20").replace(')', "%29"));
                }
                out.push_str(&t[t.len() - trail..]);
            }
            Piece::LineBreak => out.push_str("\\\n"),
            Piece::Image { src, alt } => {
                out.push_str("![");
                md_escape(alt, &mut out);
                let _ = write!(out, "]({src})");
            }
            Piece::Note(n) => {
                let _ = write!(out, "[^{n}]");
            }
        }
    }
    out
}

/// Escapes what would start a block at the beginning of a paragraph's first line.
fn md_line_start(text: String) -> String {
    let t = text.trim_start();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let numbered = digits > 0 && matches!(t[digits..].chars().next(), Some('.' | ')'));
    if t.starts_with('#')
        || t.starts_with("- ")
        || t.starts_with("+ ")
        || t.starts_with("---")
        || t.starts_with("===")
        || numbered
    {
        if numbered {
            return format!("{}\\{}", &t[..digits], &t[digits..]);
        }
        return format!("\\{t}");
    }
    text
}

fn md_blocks(w: &mut Walker<'_>, blocks: &[Block], out: &mut String) {
    let mut prev: Option<Kind> = None;
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                let kind = w.kind(p);
                let pieces = w.pieces(p);
                // Blank lines separate blocks, except between items of a list and lines of
                // a code block.
                let tight = match (&prev, &kind) {
                    (Some(Kind::Code), Kind::Code) => true,
                    // A list of the other kind at the outer level starts a new list.
                    (Some(Kind::Item { ordered: a, .. }), Kind::Item { ordered: b, level }) => {
                        a == b || *level > 0
                    }
                    _ => false,
                };
                if matches!(prev, Some(Kind::Code)) && kind != Kind::Code {
                    out.push_str("```\n");
                }
                if !out.is_empty() && !tight {
                    out.push('\n');
                }
                match &kind {
                    Kind::Code => {
                        if prev != Some(Kind::Code) {
                            out.push_str("```\n");
                        }
                        out.push_str(&p.content.plain_text(true));
                        out.push('\n');
                    }
                    Kind::Rule => out.push_str("---\n"),
                    Kind::Heading(n) => {
                        let text = md_inline(&pieces);
                        let _ = writeln!(out, "{} {}", "#".repeat(usize::from(*n)), text.trim());
                    }
                    Kind::Quote => {
                        let text = md_inline(&pieces);
                        for line in text.lines() {
                            let _ = writeln!(out, "> {line}");
                        }
                    }
                    Kind::Item { ordered, level } => {
                        let indent = "    ".repeat(usize::from(*level));
                        let marker = if *ordered {
                            let n = w
                                .labels
                                .get(&p.id)
                                .and_then(|l| l.counters.last().copied())
                                .unwrap_or(1);
                            format!("{n}.")
                        } else {
                            "-".to_owned()
                        };
                        let text = md_inline(&pieces);
                        let body = text.replace('\n', &format!("\n{indent}  "));
                        let _ = writeln!(out, "{indent}{marker} {}", body.trim());
                    }
                    Kind::Para => {
                        let text =
                            format!("{}{}", w.anchor_html(p), md_line_start(md_inline(&pieces)));
                        if text.trim().is_empty() {
                            // An empty paragraph is spacing in Word; Markdown has none.
                            if out.ends_with("\n\n") {
                                out.pop();
                            }
                        } else {
                            out.push_str(text.trim_end());
                            out.push('\n');
                        }
                    }
                }
                prev = Some(kind);
            }
            Block::Table(t) => {
                if matches!(prev, Some(Kind::Code)) {
                    out.push_str("```\n");
                }
                if !out.is_empty() {
                    out.push('\n');
                }
                md_table(w, t, out);
                prev = Some(Kind::Para);
            }
            Block::Sdt(s) => md_blocks(w, &s.blocks, out),
            Block::Field(f) => md_blocks(w, &f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
    if matches!(prev, Some(Kind::Code)) {
        out.push_str("```\n");
    }
}

/// A cell's paragraphs as one line of Markdown (`<br>` between paragraphs).
fn md_cell(w: &mut Walker<'_>, blocks: &[Block]) -> String {
    let mut parts = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                let pieces = w.pieces(p);
                let s = md_inline(&pieces)
                    .replace("\\\n", "<br>")
                    .replace('\n', " ");
                if !s.trim().is_empty() {
                    parts.push(s.trim().to_owned());
                }
            }
            Block::Table(t) => {
                // A nested table flattens to its cells' text.
                for r in &t.rows {
                    for c in &r.cells {
                        let s = md_cell(w, &c.blocks);
                        if !s.is_empty() {
                            parts.push(s);
                        }
                    }
                }
            }
            Block::Sdt(s) => parts.push(md_cell(w, &s.blocks)),
            Block::Field(f) => parts.push(md_cell(w, &f.blocks)),
            Block::Opaque(_) => {}
        }
    }
    parts.join("<br>")
}

fn md_table(w: &mut Walker<'_>, t: &Table, out: &mut String) {
    let width = t
        .rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| c.grid_span.max(1) as usize)
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
        .max(1);
    let mut first = true;
    for r in &t.rows {
        let mut cells: Vec<String> = Vec::with_capacity(width);
        for c in &r.cells {
            let text = if c.v_merge == Some(VMerge::Continue) {
                String::new()
            } else {
                md_cell(w, &c.blocks)
            };
            cells.push(text);
            for _ in 1..c.grid_span.max(1) {
                cells.push(String::new());
            }
        }
        cells.resize(width, String::new());
        let _ = writeln!(out, "| {} |", cells.join(" | "));
        if first {
            let _ = writeln!(out, "|{}", " --- |".repeat(width));
            first = false;
        }
    }
}

fn md_notes(w: &mut Walker<'_>, out: &mut String) {
    let mut k = 0;
    while k < w.notes.len() {
        let id = w.notes[k];
        let blocks = w.note(id).map(|n| n.blocks.clone()).unwrap_or_default();
        let mut body = String::new();
        w.in_note = Some(id);
        md_blocks(w, &blocks, &mut body);
        w.in_note = None;
        let _ = writeln!(
            out,
            "\n[^{}]: {}",
            k + 1,
            body.trim().replace('\n', "\n    ")
        );
        k += 1;
    }
}

/// The document as Markdown.
pub(crate) fn markdown(doc: &Document) -> String {
    let mut w = Walker::new(doc);
    let mut out = String::new();
    for s in &doc.sections {
        md_blocks(&mut w, &s.blocks, &mut out);
    }
    md_notes(&mut w, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------------------------------
// HTML
// ------------------------------------------------------------------------------------------

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

fn html_inline(pieces: &[Piece], ends: bool) -> String {
    let mut out = String::new();
    for p in pieces {
        match p {
            Piece::Text(t, l) => {
                let tags: [(bool, &str); 7] = [
                    (l.bold, "strong"),
                    (l.italic, "em"),
                    (l.underline, "u"),
                    (l.strike, "s"),
                    (l.code, "code"),
                    (l.sup, "sup"),
                    (l.sub, "sub"),
                ];
                if let Some(href) = &l.link {
                    out.push_str("<a href=\"");
                    esc(href, &mut out);
                    out.push_str("\">");
                }
                for (on, tag) in tags {
                    if on {
                        let _ = write!(out, "<{tag}>");
                    }
                }
                esc(t, &mut out);
                for (on, tag) in tags.iter().rev() {
                    if *on {
                        let _ = write!(out, "</{tag}>");
                    }
                }
                if l.link.is_some() {
                    out.push_str("</a>");
                }
            }
            Piece::LineBreak => out.push_str("<br>"),
            Piece::Image { src, alt } => {
                out.push_str("<img src=\"");
                out.push_str(src);
                out.push_str("\" alt=\"");
                esc(alt, &mut out);
                out.push_str("\">");
            }
            Piece::Note(n) => {
                let pre = if ends { "en" } else { "fn" };
                let _ = write!(
                    out,
                    "<sup><a href=\"#{pre}{n}\" id=\"{pre}ref{n}\">{n}</a></sup>"
                );
            }
        }
    }
    out
}

fn align_attr(p: &Paragraph, doc: &Document) -> &'static str {
    match doc.resolve_paragraph(p).alignment {
        Alignment::Center => " style=\"text-align:center\"",
        Alignment::Right => " style=\"text-align:right\"",
        Alignment::Both | Alignment::Distribute => " style=\"text-align:justify\"",
        _ => "",
    }
}

fn html_blocks(w: &mut Walker<'_>, blocks: &[Block], out: &mut String) {
    // Open lists: (ordered, level).
    let mut lists: Vec<(bool, u8)> = Vec::new();
    let mut in_code = false;
    let close_lists = |lists: &mut Vec<(bool, u8)>, to: usize, out: &mut String| {
        while lists.len() > to {
            if let Some((o, _)) = lists.pop() {
                out.push_str("</li>");
                out.push_str(if o { "</ol>\n" } else { "</ul>\n" });
            }
        }
    };
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                let kind = w.kind(p);
                if in_code && kind != Kind::Code {
                    out.push_str("</code></pre>\n");
                    in_code = false;
                }
                if let Kind::Item { ordered, level } = kind {
                    let depth = usize::from(level) + 1;
                    if lists.len() > depth {
                        close_lists(&mut lists, depth, out);
                    }
                    if lists.len() == depth {
                        if lists.last().is_some_and(|(o, _)| *o != ordered) {
                            close_lists(&mut lists, depth - 1, out);
                        } else {
                            out.push_str("</li>\n");
                        }
                    }
                    while lists.len() < depth {
                        out.push_str(if ordered { "<ol>" } else { "<ul>" });
                        lists.push((ordered, level));
                        if lists.len() < depth {
                            out.push_str("<li>");
                        }
                    }
                    let pieces = w.pieces(p);
                    out.push_str("<li>");
                    out.push_str(&html_inline(&pieces, false));
                    continue;
                }
                close_lists(&mut lists, 0, out);
                let pieces = w.pieces(p);
                let inner = format!("{}{}", w.anchor_html(p), html_inline(&pieces, false));
                match kind {
                    Kind::Code => {
                        if !in_code {
                            out.push_str("<pre><code>");
                            in_code = true;
                        }
                        esc(&p.content.plain_text(true), out);
                        out.push('\n');
                    }
                    Kind::Rule => out.push_str("<hr>\n"),
                    Kind::Heading(n) => {
                        let mut id = String::new();
                        if let Some(h) = w.heading_ids.get(&p.id) {
                            id.push_str(" id=\"");
                            esc(h, &mut id);
                            id.push('"');
                        }
                        let _ = writeln!(out, "<h{n}{id}{}>{inner}</h{n}>", align_attr(p, w.doc));
                    }
                    Kind::Quote => {
                        let _ = writeln!(out, "<blockquote><p>{inner}</p></blockquote>");
                    }
                    Kind::Para => {
                        if !inner.is_empty() {
                            let _ = writeln!(out, "<p{}>{inner}</p>", align_attr(p, w.doc));
                        }
                    }
                    Kind::Item { .. } => {}
                }
            }
            Block::Table(t) => {
                close_lists(&mut lists, 0, out);
                if in_code {
                    out.push_str("</code></pre>\n");
                    in_code = false;
                }
                html_table(w, t, out);
            }
            Block::Sdt(s) => {
                close_lists(&mut lists, 0, out);
                html_blocks(w, &s.blocks, out);
            }
            Block::Field(f) => {
                close_lists(&mut lists, 0, out);
                html_blocks(w, &f.blocks, out);
            }
            Block::Opaque(_) => {}
        }
    }
    close_lists(&mut lists, 0, out);
    if in_code {
        out.push_str("</code></pre>\n");
    }
}

fn html_table(w: &mut Walker<'_>, t: &Table, out: &mut String) {
    // Rows a vertically merged cell spans, by (row, grid column).
    let mut spans: HashMap<(usize, usize), usize> = HashMap::new();
    let grid_cols = |r: &kiln_doc::Row| {
        let mut col = r.props.grid_before.unwrap_or(0) as usize;
        r.cells
            .iter()
            .map(|c| {
                let at = col;
                col += c.grid_span.max(1) as usize;
                at
            })
            .collect::<Vec<_>>()
    };
    for (ri, r) in t.rows.iter().enumerate() {
        for (c, col) in r.cells.iter().zip(grid_cols(r)) {
            if c.v_merge == Some(VMerge::Restart) {
                let mut n = 1;
                for later in &t.rows[ri + 1..] {
                    let cont = later
                        .cells
                        .iter()
                        .zip(grid_cols(later))
                        .any(|(lc, lcol)| lcol == col && lc.v_merge == Some(VMerge::Continue));
                    if !cont {
                        break;
                    }
                    n += 1;
                }
                spans.insert((ri, col), n);
            }
        }
    }
    out.push_str("<table>\n");
    let mut head_open = false;
    for (ri, r) in t.rows.iter().enumerate() {
        let header = r.props.header == Some(true) && (ri == 0 || head_open);
        if header && ri == 0 {
            out.push_str("<thead>\n");
            head_open = true;
        } else if head_open && !header {
            out.push_str("</thead>\n");
            head_open = false;
        }
        out.push_str("<tr>");
        let tag = if header { "th" } else { "td" };
        for (c, col) in r.cells.iter().zip(grid_cols(r)) {
            if c.v_merge == Some(VMerge::Continue) {
                continue;
            }
            let _ = write!(out, "<{tag}");
            if c.grid_span > 1 {
                let _ = write!(out, " colspan=\"{}\"", c.grid_span);
            }
            if let Some(n) = spans.get(&(ri, col)).filter(|n| **n > 1) {
                let _ = write!(out, " rowspan=\"{n}\"");
            }
            out.push('>');
            let mut inner = String::new();
            html_blocks(w, &c.blocks, &mut inner);
            // A cell of one paragraph needs no <p>.
            let trimmed = inner.trim();
            let single = trimmed.matches("<p").count() == 1
                && trimmed.starts_with("<p")
                && trimmed.ends_with("</p>");
            if single {
                let start = trimmed.find('>').map_or(0, |i| i + 1);
                out.push_str(&trimmed[start..trimmed.len() - 4]);
            } else {
                out.push_str(trimmed);
            }
            let _ = write!(out, "</{tag}>");
        }
        out.push_str("</tr>\n");
    }
    if head_open {
        out.push_str("</thead>\n");
    }
    out.push_str("</table>\n");
}

const CSS: &str = "body{max-width:46rem;margin:2rem auto;padding:0 1rem;font:1.05rem/1.55 \
Georgia,'Times New Roman',serif;color:#222}h1,h2,h3,h4,h5,h6{font-family:system-ui,sans-serif;\
line-height:1.25}img{max-width:100%;height:auto}table{border-collapse:collapse;margin:1rem 0}\
th,td{border:1px solid #bbb;padding:.3rem .5rem;vertical-align:top}th{background:#f2f2f2}\
blockquote{margin:1rem 0;padding-left:1rem;border-left:3px solid #ccc;color:#555}\
pre{background:#f6f6f6;padding:.75rem;overflow:auto}code{font-family:Consolas,monospace}\
.notes{font-size:.9em;border-top:1px solid #ccc;margin-top:2rem}";

/// The document as one HTML5 file.
pub(crate) fn html(doc: &Document) -> String {
    let mut w = Walker::new(doc);
    let mut body = String::new();
    for s in &doc.sections {
        html_blocks(&mut w, &s.blocks, &mut body);
    }
    // Notes, footnotes first then endnotes, numbered by first reference.
    let mut k = 0;
    let mut notes = String::new();
    while k < w.notes.len() {
        let id = w.notes[k];
        let ends = note_is_end(&w, id);
        let blocks = w.note(id).map(|n| n.blocks.clone()).unwrap_or_default();
        let mut inner = String::new();
        w.in_note = Some(id);
        html_blocks(&mut w, &blocks, &mut inner);
        w.in_note = None;
        let pre = if ends { "en" } else { "fn" };
        let n = k + 1;
        let _ = writeln!(
            notes,
            "<li id=\"{pre}{n}\">{} <a href=\"#{pre}ref{n}\" aria-label=\"Back\">↩</a></li>",
            inner.trim()
        );
        k += 1;
    }
    let title = doc
        .properties
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| doc.outline().first().map(|h| h.text.clone()))
        .unwrap_or_else(|| "Document".to_owned());
    let lang = doc
        .styles
        .doc_defaults
        .rpr
        .lang
        .as_ref()
        .and_then(|l| l.val.clone())
        .unwrap_or_else(|| "en".to_owned());
    let mut out = String::with_capacity(body.len() + 1024);
    out.push_str("<!doctype html>\n<html lang=\"");
    esc(&lang, &mut out);
    out.push_str("\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Kiln Write\">\n<title>");
    esc(&title, &mut out);
    let _ = write!(out, "</title>\n<style>{CSS}</style>\n</head>\n<body>\n");
    out.push_str(&body);
    if !notes.is_empty() {
        let _ = write!(
            out,
            "<section class=\"notes\">\n<ol>\n{notes}</ol>\n</section>\n"
        );
    }
    out.push_str("</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_that_would_make_blocks_are_escaped() {
        assert_eq!(md_line_start("# not".into()), "\\# not");
        assert_eq!(md_line_start("1. not".into()), "1\\. not");
        assert_eq!(md_line_start("plain".into()), "plain");
    }
}
