// SPDX-License-Identifier: AGPL-3.0-or-later
//! The clipboard: Copy and Cut write three flavours — Kiln's rich format (the paragraphs as the
//! model holds them, with the styles and list definitions they use), HTML (inline formatting,
//! headings, lists and links, for other apps) and plain text — and Paste reads them back with
//! Word's options:
//!
//! - **Keep Source Formatting**: Kiln's format when the clipboard holds what Kiln copied, else
//!   the HTML (sanitised and mapped by [`super::html`]), else the text;
//! - **Merge Formatting**: the destination's paragraph and character formatting, keeping bold,
//!   italic, underline and list structure from the source;
//! - **Keep Text Only**: the text in the destination's formatting.
//!
//! A paste is one undo step. Pasted content gets fresh node ids; comments, footnote references
//! and bookmarks of the source are not carried over (they belong to the source document).

use std::collections::{BTreeMap, BTreeSet};

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::st::BreakKind;
use kiln_doc::{
    AbstractNum, Block, Break, Cell, Document, EditKind, Fragment, InlineObject, Mark, MarkSpan,
    NumInstance, NumberingRef, OBJECT_CHAR, Op, Paragraph, ParagraphProps, Pos, Range, Row,
    RunProps, RunSpan, Style, Table, TableCellProps, TableLook, TableProps, TableRowProps,
    TableWidth, Transaction, edit,
};
use kiln_protocol::commands::{ClipboardData, PasteMode};
use serde::{Deserialize, Serialize};

use super::doc_err;
use super::html::{self, HtmlBlock, HtmlInline, HtmlPara};
use crate::apps::write::WriteDoc;
use crate::model::DocModel;

/// Version of Kiln's clipboard format.
const VERSION: u32 = 1;

/// Kiln's clipboard format (`application/x-kiln-fragment`, JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KilnClip {
    v: u32,
    paras: Vec<ClipPara>,
    styles: Vec<Style>,
    lists: Vec<ClipList>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipPara {
    props: ParagraphProps,
    mark: RunProps,
    content: Fragment,
    /// The paragraph mark was copied (every paragraph but the last of a selection).
    whole: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipList {
    num: NumInstance,
    abs: AbstractNum,
}

/// A paragraph to paste.
#[derive(Debug, Clone)]
struct ParaIn {
    props: ParagraphProps,
    mark: RunProps,
    content: Fragment,
    whole: bool,
}

/// A block to paste.
#[derive(Debug, Clone)]
enum PasteBlock {
    Para(Box<ParaIn>),
    Table(Vec<Vec<Vec<ParaIn>>>),
}

/// Appends a fragment to another.
fn append(to: &mut Fragment, more: Fragment) {
    let shift = to.len();
    to.text.push_str(&more.text);
    to.runs.extend(more.runs);
    to.marks.extend(
        more.marks
            .into_iter()
            .map(|m| MarkSpan::new(m.start + shift, m.end + shift, m.mark)),
    );
    to.objects.extend(more.objects);
}

/// Keeps the objects `keep` accepts (and the text around them), adjusting runs and marks.
fn filter_objects(f: Fragment, keep: impl Fn(&InlineObject) -> bool) -> Fragment {
    if f.objects.iter().all(&keep) {
        return f;
    }
    let mut drop = Vec::new();
    let mut obj = 0;
    for (i, c) in f.text.chars().enumerate() {
        if c == OBJECT_CHAR {
            if !f.objects.get(obj).is_some_and(&keep) {
                drop.push(i);
            }
            obj += 1;
        }
    }
    let dropped = |i: usize| drop.binary_search(&i).is_ok();
    let new_index = |i: usize| i - drop.partition_point(|d| *d < i);
    let text: String = f
        .text
        .chars()
        .enumerate()
        .filter(|(i, _)| !dropped(*i))
        .map(|(_, c)| c)
        .collect();
    let objects = f.objects.into_iter().filter(|o| keep(o)).collect();
    let mut runs = Vec::new();
    let mut pos = 0;
    for r in f.runs {
        let kept = (pos..pos + r.len).filter(|i| !dropped(*i)).count();
        if kept > 0 {
            runs.push(RunSpan::new(kept, r.props));
        }
        pos += r.len;
    }
    let marks = f
        .marks
        .into_iter()
        .filter_map(|m| {
            let (a, b) = (new_index(m.start), new_index(m.end));
            (a < b).then(|| MarkSpan::new(a, b, m.mark))
        })
        .collect();
    Fragment {
        text,
        runs,
        marks,
        objects,
    }
}

/// What a pasted fragment may keep: no comment anchors or tracked changes of another document.
fn clean_marks(mut f: Fragment) -> Fragment {
    f.marks
        .retain(|m| matches!(m.mark, Mark::Link(_) | Mark::Sdt(_)));
    filter_objects(f, |o| {
        !matches!(
            o,
            InlineObject::NoteRef(_)
                | InlineObject::BookmarkStart { .. }
                | InlineObject::BookmarkEnd { .. }
        )
    })
}

/// Fresh ids for the drawings (and their text boxes) of a fragment.
fn fresh_fragment(doc: &mut Document, mut f: Fragment) -> Result<Fragment> {
    for o in &mut f.objects {
        if let InlineObject::Drawing(d) = o {
            d.id = doc.alloc_id();
            if let Some(tb) = &d.text_box {
                let blocks = tb.clone();
                d.text_box = Some(fresh_blocks(doc, &blocks)?);
            }
        }
    }
    Ok(f)
}

/// A copy of blocks with fresh ids throughout (paragraphs, tables, rows, cells, controls,
/// drawings).
pub(super) fn fresh_blocks(doc: &mut Document, blocks: &[Block]) -> Result<Vec<Block>> {
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        out.push(match b {
            Block::Paragraph(p) => {
                let content =
                    fresh_fragment(doc, p.content.slice(0, p.content.len()).map_err(doc_err)?)?;
                Block::Paragraph(Paragraph {
                    id: doc.alloc_id(),
                    props: p.props.clone(),
                    mark_props: p.mark_props.clone(),
                    revision: p.revision.clone(),
                    content: kiln_doc::Inlines::from_fragment(content).map_err(doc_err)?,
                })
            }
            Block::Table(t) => {
                let mut rows = Vec::with_capacity(t.rows.len());
                for r in &t.rows {
                    let mut cells = Vec::with_capacity(r.cells.len());
                    for c in &r.cells {
                        cells.push(Cell {
                            id: doc.alloc_id(),
                            props: c.props.clone(),
                            grid_span: c.grid_span,
                            v_merge: c.v_merge,
                            blocks: fresh_blocks(doc, &c.blocks)?,
                        });
                    }
                    rows.push(Row {
                        id: doc.alloc_id(),
                        props: r.props.clone(),
                        revision: r.revision.clone(),
                        cells,
                    });
                }
                Block::Table(Table {
                    id: doc.alloc_id(),
                    props: t.props.clone(),
                    grid: t.grid.clone(),
                    rows,
                })
            }
            Block::Sdt(s) => {
                let mut s2 = s.clone();
                s2.id = doc.alloc_id();
                s2.blocks = fresh_blocks(doc, &s.blocks)?;
                Block::Sdt(s2)
            }
            Block::Field(f) => {
                let mut f2 = f.clone();
                f2.id = doc.alloc_id();
                f2.blocks = fresh_blocks(doc, &f.blocks)?;
                Block::Field(f2)
            }
            Block::Opaque(o) => {
                let mut o2 = o.clone();
                o2.id = doc.alloc_id();
                Block::Opaque(o2)
            }
        });
    }
    Ok(out)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&emsp;"),
            c => out.push(c),
        }
    }
    out
}

impl WriteDoc {
    /// The selection as the three clipboard flavours (`None` when it is empty).
    pub(super) fn clipboard_data(&self) -> Result<Option<ClipboardData>> {
        let (s, e) = self.span();
        if s == e {
            return Ok(None);
        }
        let segs = edit::segments(&self.doc, s, e).map_err(doc_err)?;
        let n = segs.len();
        let mut paras = Vec::with_capacity(n);
        let mut style_ids = BTreeSet::new();
        let mut num_ids = BTreeSet::new();
        for (i, (id, a, b)) in segs.into_iter().enumerate() {
            let p = self.para(id)?;
            let content = p.content.slice(a, b).map_err(doc_err)?;
            for r in &content.runs {
                if let Some(st) = &r.props.style_id {
                    style_ids.insert(st.clone());
                }
            }
            let resolved = self.doc.resolve_paragraph(p);
            style_ids.insert(resolved.style_id.clone());
            let mut props = p.props.clone();
            if props.style_id.is_none() {
                props.style_id = Some(resolved.style_id.clone());
            }
            if let Some(l) = resolved.numbering {
                num_ids.insert(l.num_id);
                props.numbering = Some(NumberingRef::new(l.num_id, l.level));
            }
            paras.push(ClipPara {
                props,
                mark: p.mark_props.clone(),
                content,
                whole: i + 1 < n,
            });
        }
        // The styles used, with their bases.
        let mut styles = Vec::new();
        let mut seen = BTreeSet::new();
        for id in style_ids {
            for s in self.doc.styles.chain(Some(&id)) {
                if seen.insert(s.id.clone()) {
                    styles.push(s.clone());
                }
            }
        }
        let lists = num_ids
            .into_iter()
            .filter_map(|id| {
                let num = self.doc.numbering.instances.get(&id)?.clone();
                let abs = self.doc.numbering.abstracts.get(&num.abstract_id)?.clone();
                Some(ClipList { num, abs })
            })
            .collect();
        let html = self.render_html(&paras);
        let clip = KilnClip {
            v: VERSION,
            paras,
            styles,
            lists,
        };
        let kiln = serde_json::to_string(&clip)
            .map_err(|e| KilnError::Internal(format!("clipboard: {e}")))?;
        Ok(Some(ClipboardData {
            text: DocModel::selected_text(self).unwrap_or_default(),
            html: Some(html),
            kiln: Some(kiln),
        }))
    }

    /// Copy: the session takes the flavours and writes them to the system clipboard.
    pub(super) fn copy(&mut self) {
        self.editor.copied = self.clipboard_data().ok().flatten();
    }

    /// Cut: Copy, then delete the selection as one step.
    pub(super) fn cut(&mut self) -> Result<()> {
        let data = self.clipboard_data()?;
        if data.is_none() {
            return Ok(());
        }
        let (s, e) = self.span();
        let tx = edit::delete_range(&self.doc, s, e)
            .map_err(|_| {
                KilnError::InvalidInput(
                    "a selection that runs into or out of a table cannot be cut; cut the cells' text or delete the rows"
                        .into(),
                )
            })?
            .with_kind(EditKind::Delete)
            .with_selection(Some(Range::new(self.anchor, self.head)), Some(Range::caret(s)));
        self.commit(tx)?;
        self.editor.copied = data;
        Ok(())
    }

    /// The HTML flavour of copied paragraphs (effective formatting inline, headings as `h1`–
    /// `h6`, lists as `ul`/`ol`, links as `a`).
    fn render_html(&self, paras: &[ClipPara]) -> String {
        let mut out = String::from("<meta charset=\"utf-8\"><div class=\"kiln-fragment\">");
        let mut open_list: Option<bool> = None;
        for cp in paras {
            let probe = Paragraph {
                props: cp.props.clone(),
                mark_props: cp.mark.clone(),
                ..Paragraph::new(NodeId(0))
            };
            let resolved =
                kiln_doc::resolve::resolve_paragraph(&self.doc.context(), &probe.props, None);
            let list = resolved
                .numbering
                .map(|l| self.is_bullet_list(l.num_id))
                .map(|bullet| !bullet);
            if open_list != list {
                if let Some(ordered) = open_list {
                    out.push_str(if ordered { "</ol>" } else { "</ul>" });
                }
                if let Some(ordered) = list {
                    out.push_str(if ordered { "<ol>" } else { "<ul>" });
                }
                open_list = list;
            }
            let tag = if list.is_some() {
                "li".to_owned()
            } else if resolved.outline_level < 6 {
                format!("h{}", resolved.outline_level + 1)
            } else {
                "p".to_owned()
            };
            let align = match resolved.alignment {
                kiln_doc::st::Alignment::Center => " style=\"text-align:center\"",
                kiln_doc::st::Alignment::Right => " style=\"text-align:right\"",
                kiln_doc::st::Alignment::Both => " style=\"text-align:justify\"",
                _ => "",
            };
            out.push_str(&format!("<{tag}{align}>"));
            out.push_str(&self.render_inline(&probe, &cp.content));
            out.push_str(&format!("</{tag}>"));
        }
        if let Some(ordered) = open_list {
            out.push_str(if ordered { "</ol>" } else { "</ul>" });
        }
        out.push_str("</div>");
        out
    }

    fn render_inline(&self, para: &Paragraph, f: &Fragment) -> String {
        let chars: Vec<char> = f.text.chars().collect();
        // Boundaries: run edges and link edges.
        let mut cuts = BTreeSet::from([0, chars.len()]);
        let mut pos = 0;
        for r in &f.runs {
            pos += r.len;
            cuts.insert(pos.min(chars.len()));
        }
        for m in &f.marks {
            cuts.insert(m.start.min(chars.len()));
            cuts.insert(m.end.min(chars.len()));
        }
        let cuts: Vec<usize> = cuts.into_iter().collect();
        let mut out = String::new();
        let mut obj = 0;
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= b {
                continue;
            }
            let props = {
                let mut p = 0;
                let mut found = RunProps::default();
                for r in &f.runs {
                    if a < p + r.len {
                        found = r.props.clone();
                        break;
                    }
                    p += r.len;
                }
                found
            };
            let link = f.marks.iter().find_map(|m| match &m.mark {
                Mark::Link(h) if m.start <= a && b <= m.end => match &h.target {
                    kiln_doc::LinkTarget::Url(u) => Some(u.clone()),
                    kiln_doc::LinkTarget::Bookmark(b) => Some(format!("#{b}")),
                },
                _ => None,
            });
            let mut text = String::new();
            for c in &chars[a..b] {
                if *c == OBJECT_CHAR {
                    if let Some(InlineObject::Break(_)) = f.objects.get(obj) {
                        text.push_str("<br>");
                    } else if let Some(o) = f.objects.get(obj) {
                        text.push_str(&escape(&o.plain_text().replace(OBJECT_CHAR, "")));
                    }
                    obj += 1;
                } else {
                    text.push_str(&escape(&c.to_string()));
                }
            }
            let r = self.resolve_with(para, &props);
            let mut css = Vec::new();
            if let Some(c) = r.color.rgb() {
                css.push(format!("color:#{}", super::format::hex(c)));
            }
            css.push(format!("font-size:{}pt", r.size as f32 / 2.0));
            css.push(format!("font-family:'{}'", r.fonts.ascii.replace('\'', "")));
            if let Some(c) = r.highlight.as_ref().and_then(|h| h.rgb()) {
                css.push(format!("background-color:#{}", super::format::hex(c)));
            }
            let mut s = format!("<span style=\"{}\">{text}</span>", css.join(";"));
            let wrap = |s: String, t: &str| format!("<{t}>{s}</{t}>");
            if r.bold {
                s = wrap(s, "b");
            }
            if r.italic {
                s = wrap(s, "i");
            }
            if r.underline.is_some() {
                s = wrap(s, "u");
            }
            if r.strike || r.dstrike {
                s = wrap(s, "s");
            }
            match r.vert_align {
                kiln_doc::st::VertAlign::Superscript => s = wrap(s, "sup"),
                kiln_doc::st::VertAlign::Subscript => s = wrap(s, "sub"),
                _ => {}
            }
            if let Some(href) = link.filter(|l| super::insert::safe_url(l)) {
                s = format!("<a href=\"{}\">{s}</a>", escape(&href));
            }
            out.push_str(&s);
        }
        out
    }

    // ----- paste -------------------------------------------------------------------------

    /// Paste with one of Word's options.
    pub(super) fn paste(&mut self, data: Option<&ClipboardData>, mode: PasteMode) -> Result<()> {
        let Some(data) = data else {
            return Err(KilnError::InvalidInput("the clipboard is empty".into()));
        };
        let kiln = data
            .kiln
            .as_deref()
            .filter(|_| matches!(mode, PasteMode::KeepSource | PasteMode::Merge))
            .and_then(|k| serde_json::from_str::<KilnClip>(k).ok())
            .filter(|c| c.v == VERSION && !c.paras.is_empty());
        let html = data
            .html
            .as_deref()
            .filter(|h| !h.trim().is_empty() && h.len() <= html::MAX_HTML)
            .filter(|_| mode != PasteMode::TextOnly);
        let mut batch = self.batch(EditKind::Structure);
        let (s, e) = self.span();
        if s != e {
            let tx = edit::delete_range(&self.doc, s, e).map_err(|_| {
                KilnError::InvalidInput(
                    "paste over a selection that runs into or out of a table is not possible; select within the cell"
                        .into(),
                )
            })?;
            self.batch_apply(&mut batch, tx)?;
            self.anchor = s;
            self.head = s;
        }
        let blocks = if let Some(clip) = kiln {
            self.blocks_from_kiln(&mut batch, clip, mode == PasteMode::KeepSource)?
        } else if let Some(h) = html {
            let parsed = html::parse(h);
            if parsed.is_empty() {
                self.blocks_from_text(&data.text)?
            } else {
                self.blocks_from_html(
                    &mut batch,
                    parsed,
                    mode == PasteMode::KeepSource || mode == PasteMode::Html,
                )?
            }
        } else {
            self.blocks_from_text(&data.text)?
        };
        let keep = mode != PasteMode::TextOnly && mode != PasteMode::Merge;
        let blocks = if mode == PasteMode::Merge {
            self.merge_formatting(blocks)?
        } else {
            blocks
        };
        let after = self.insert_blocks(&mut batch, blocks, keep)?;
        self.batch_commit(batch, Range::caret(after));
        Ok(())
    }

    fn blocks_from_text(&self, text: &str) -> Result<Vec<PasteBlock>> {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let p = self.para(self.head.para)?;
        let props = self.caret_props()?;
        Ok(text
            .split('\n')
            .map(|line| {
                PasteBlock::Para(Box::new(ParaIn {
                    props: p.props.clone(),
                    mark: p.mark_props.clone(),
                    content: Fragment::text(line, props.clone()),
                    whole: false,
                }))
            })
            .collect())
    }

    /// Kiln's format: styles the destination lacks are added (Keep Source), lists get their own
    /// definitions (ids remapped when they clash).
    fn blocks_from_kiln(
        &mut self,
        batch: &mut super::Batch,
        clip: KilnClip,
        keep_source: bool,
    ) -> Result<Vec<PasteBlock>> {
        if keep_source {
            let mut ops = Vec::new();
            let mut index = self.doc.styles.len();
            for s in clip.styles {
                if !self.doc.styles.contains(&s.id) {
                    ops.push(Op::PutStyle {
                        style: Box::new(s),
                        index,
                    });
                    index += 1;
                }
            }
            self.batch_apply(batch, Transaction::new(ops))?;
        }
        // Lists: the same definition under the same id stays; anything else is added anew.
        let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
        let mut ops = Vec::new();
        let mut next_abs = self
            .doc
            .numbering
            .abstracts
            .keys()
            .next_back()
            .map_or(0, |k| k + 1);
        let mut next_num = self
            .doc
            .numbering
            .instances
            .keys()
            .next_back()
            .map_or(1, |k| k + 1);
        for l in clip.lists {
            let same = self
                .doc
                .numbering
                .instances
                .get(&l.num.id)
                .is_some_and(|n| self.doc.numbering.abstracts.get(&n.abstract_id) == Some(&l.abs));
            if same {
                remap.insert(l.num.id, l.num.id);
                continue;
            }
            let mut abs = l.abs.clone();
            abs.id = next_abs;
            let num = NumInstance {
                id: next_num,
                abstract_id: next_abs,
                overrides: l.num.overrides.clone(),
            };
            ops.push(Op::SetAbstractNum {
                id: abs.id,
                def: Some(Box::new(abs)),
            });
            ops.push(Op::SetNumInstance {
                id: num.id,
                instance: Some(num),
            });
            remap.insert(l.num.id, next_num);
            next_abs += 1;
            next_num += 1;
        }
        self.batch_apply(batch, Transaction::new(ops))?;
        let mut out = Vec::with_capacity(clip.paras.len());
        for mut p in clip.paras {
            if let Some(n) = &mut p.props.numbering
                && let Some(id) = n.num_id
                && id != 0
            {
                n.num_id = remap.get(&id).copied().or(Some(0));
            }
            if p.props
                .style_id
                .as_deref()
                .is_some_and(|s| !self.doc.styles.contains(s))
            {
                p.props.style_id = None;
            }
            let content = fresh_fragment(&mut self.doc, clean_marks(p.content))?;
            content
                .validate()
                .map_err(|e| KilnError::InvalidInput(format!("clipboard: {e}")))?;
            out.push(PasteBlock::Para(Box::new(ParaIn {
                props: p.props,
                mark: p.mark,
                content,
                whole: p.whole,
            })));
        }
        Ok(out)
    }

    /// HTML: headings, quotes, lists (one bulleted and one numbered list for the paste) and
    /// tables.
    fn blocks_from_html(
        &mut self,
        batch: &mut super::Batch,
        parsed: Vec<HtmlBlock>,
        keep_source: bool,
    ) -> Result<Vec<PasteBlock>> {
        // The styles HTML maps to.
        let mut wanted = BTreeSet::new();
        let mut needs_bullets = false;
        let mut needs_numbers = false;
        let mut visit = |p: &HtmlPara| {
            if let Some(s) = &p.style {
                wanted.insert(s.clone());
            }
            if let Some((ordered, _)) = p.list {
                if ordered {
                    needs_numbers = true;
                } else {
                    needs_bullets = true;
                }
            }
            for i in &p.inlines {
                if let HtmlInline::Text { props, .. } = i
                    && let Some(s) = &props.style_id
                {
                    wanted.insert(s.clone());
                }
            }
        };
        for b in &parsed {
            match b {
                HtmlBlock::Para(p) => visit(p),
                HtmlBlock::Table(rows) => {
                    for c in rows.iter().flatten().flatten() {
                        visit(c);
                    }
                }
            }
        }
        if keep_source {
            for s in &wanted {
                self.ensure_style(s)?;
            }
        }
        if needs_bullets || needs_numbers {
            self.ensure_style("ListParagraph")?;
        }
        let bullets = if needs_bullets {
            Some(self.add_list_in(batch, |n| n.create_bullet_list().id)?)
        } else {
            None
        };
        let numbers = if needs_numbers {
            Some(self.add_list_in(batch, |n| n.create_numbered_list().id)?)
        } else {
            None
        };
        let base = self.caret_props()?;
        let styles = self.doc.styles.clone();
        let convert = |p: HtmlPara| -> ParaIn {
            let mut props = ParagraphProps::default();
            if keep_source {
                props.style_id = p.style.filter(|s| styles.contains(s));
                props.alignment = p.align;
            }
            if let Some((ordered, level)) = p.list {
                let id = if ordered { numbers } else { bullets };
                if let Some(id) = id {
                    props.numbering = Some(NumberingRef::new(id, level));
                    if props.style_id.is_none() && styles.contains("ListParagraph") {
                        props.style_id = Some("ListParagraph".into());
                    }
                }
            }
            let mut content = Fragment::default();
            for i in p.inlines {
                match i {
                    HtmlInline::Text {
                        text,
                        props: rp,
                        link,
                    } => {
                        let mut rp = kiln_doc::merge::merge_run_props(&base, Some(&*rp));
                        if rp.style_id.as_deref().is_some_and(|s| !styles.contains(s)) {
                            rp.style_id = None;
                        }
                        let mut f = Fragment::text(&text, rp);
                        if let Some(url) = link {
                            let target = match url.strip_prefix('#') {
                                Some(b) => kiln_doc::LinkTarget::Bookmark(b.to_owned()),
                                None => kiln_doc::LinkTarget::Url(url),
                            };
                            f = f.with_mark(Mark::Link(kiln_doc::Hyperlink {
                                target,
                                tooltip: None,
                                frame: None,
                                history: true,
                            }));
                        }
                        append(&mut content, f);
                    }
                    HtmlInline::Break => append(
                        &mut content,
                        Fragment::object(
                            InlineObject::Break(Break {
                                kind: BreakKind::TextWrapping,
                                clear: None,
                            }),
                            base.clone(),
                        ),
                    ),
                }
            }
            ParaIn {
                props,
                mark: RunProps::default(),
                content,
                whole: true,
            }
        };
        let mut out = Vec::with_capacity(parsed.len());
        for b in parsed {
            out.push(match b {
                HtmlBlock::Para(p) => PasteBlock::Para(Box::new(convert(p))),
                HtmlBlock::Table(rows) => PasteBlock::Table(
                    rows.into_iter()
                        .map(|r| {
                            r.into_iter()
                                .map(|c| c.into_iter().map(&convert).collect())
                                .collect()
                        })
                        .collect(),
                ),
            });
        }
        // The last paragraph's mark is not part of a paste (it joins the destination's).
        if let Some(PasteBlock::Para(p)) = out.last_mut() {
            p.whole = false;
        }
        Ok(out)
    }

    /// Merge Formatting: destination formatting, keeping bold, italic and underline and lists.
    fn merge_formatting(&self, blocks: Vec<PasteBlock>) -> Result<Vec<PasteBlock>> {
        let dest = self.para(self.head.para)?.props.clone();
        let base = self.caret_props()?;
        let merge = |p: ParaIn| -> ParaIn {
            let mut props = dest.clone();
            props.numbering = p.props.numbering.or(dest.numbering);
            let runs = p
                .content
                .runs
                .iter()
                .map(|r| {
                    let mut props = base.clone();
                    if r.props.bold.is_some() {
                        props.bold = r.props.bold;
                    }
                    if r.props.italic.is_some() {
                        props.italic = r.props.italic;
                    }
                    if r.props.underline.is_some() {
                        props.underline.clone_from(&r.props.underline);
                    }
                    RunSpan::new(r.len, props)
                })
                .collect();
            ParaIn {
                props,
                mark: p.mark,
                content: Fragment { runs, ..p.content },
                whole: p.whole,
            }
        };
        Ok(blocks
            .into_iter()
            .map(|b| match b {
                PasteBlock::Para(p) => PasteBlock::Para(Box::new(merge(*p))),
                PasteBlock::Table(rows) => PasteBlock::Table(
                    rows.into_iter()
                        .map(|r| {
                            r.into_iter()
                                .map(|c| c.into_iter().map(merge).collect())
                                .collect()
                        })
                        .collect(),
                ),
            })
            .collect())
    }

    fn paragraph_from(
        &mut self,
        p: ParaIn,
        keep: bool,
        dest: &ParagraphProps,
    ) -> Result<Paragraph> {
        let mut para = Paragraph::new(self.doc.alloc_id());
        para.props = if keep { p.props } else { dest.clone() };
        para.mark_props = p.mark;
        para.content = kiln_doc::Inlines::from_fragment(p.content).map_err(doc_err)?;
        Ok(para)
    }

    fn table_from(
        &mut self,
        rows: Vec<Vec<Vec<ParaIn>>>,
        keep: bool,
        dest: &ParagraphProps,
        width: i32,
    ) -> Result<Table> {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).clamp(1, 63);
        let w = width / i32::try_from(cols).unwrap_or(1);
        let mut out_rows = Vec::with_capacity(rows.len());
        for r in rows {
            let mut cells = Vec::with_capacity(cols);
            let n = r.len();
            for (i, c) in r.into_iter().enumerate() {
                let mut blocks = Vec::new();
                for p in c {
                    blocks.push(Block::Paragraph(self.paragraph_from(p, keep, dest)?));
                }
                if blocks.is_empty() {
                    blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
                }
                // The last cell takes the columns the row lacks.
                let span = if i + 1 == n {
                    cols - n.min(cols) + 1
                } else {
                    1
                };
                cells.push(Cell {
                    id: self.doc.alloc_id(),
                    props: TableCellProps {
                        width: Some(TableWidth::dxa(w * i32::try_from(span).unwrap_or(1))),
                        ..TableCellProps::default()
                    },
                    grid_span: u32::try_from(span).unwrap_or(1),
                    v_merge: None,
                    blocks,
                });
            }
            if cells.is_empty() {
                continue;
            }
            out_rows.push(Row {
                id: self.doc.alloc_id(),
                props: TableRowProps::default(),
                revision: None,
                cells,
            });
        }
        Ok(Table {
            id: self.doc.alloc_id(),
            props: TableProps {
                style_id: self
                    .doc
                    .styles
                    .contains("TableGrid")
                    .then(|| "TableGrid".to_owned()),
                width: Some(TableWidth {
                    kind: kiln_doc::st::WidthType::Auto,
                    value: 0,
                }),
                look: Some(TableLook::default()),
                ..TableProps::default()
            },
            grid: vec![w; cols],
            rows: out_rows,
        })
    }

    /// Inserts paste blocks at the caret; returns where the caret goes.
    fn insert_blocks(
        &mut self,
        batch: &mut super::Batch,
        mut blocks: Vec<PasteBlock>,
        keep: bool,
    ) -> Result<Pos> {
        let at = self.head;
        if blocks.is_empty() {
            return Ok(at);
        }
        let dest = self.para(at.para)?.props.clone();
        let dest_empty = self.para(at.para)?.content.is_empty();
        if blocks.len() == 1
            && let Some(PasteBlock::Para(_)) = blocks.first()
        {
            let Some(PasteBlock::Para(p)) = blocks.pop() else {
                return Ok(at);
            };
            let len = p.content.len();
            let mut ops = Vec::new();
            if keep && p.whole && dest_empty {
                ops.push(Op::SetParagraphProps {
                    para: at.para,
                    props: Box::new(p.props),
                });
            }
            if len > 0 {
                ops.push(Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: p.content,
                });
            }
            self.batch_apply(batch, Transaction::new(ops))?;
            return Ok(Pos::new(at.para, at.offset + len));
        }
        let width = self
            .doc
            .section_of(at.para)
            .map_or(9360, |s| s.props.content_width());
        if blocks.iter().any(|b| matches!(b, PasteBlock::Table(_))) {
            self.ensure_style("TableGrid")?;
        }
        let (container, index) = self
            .doc
            .parent_of(at.para)
            .ok_or_else(|| KilnError::Internal("the caret's paragraph has no container".into()))?;
        let tail = self.doc.alloc_id();
        self.batch_apply(
            batch,
            Transaction::new(vec![
                Op::split_paragraph(&self.doc, at.para, at.offset, tail).map_err(doc_err)?,
            ]),
        )?;
        // First block: into the head paragraph.
        let first_para = matches!(blocks.first(), Some(PasteBlock::Para(_)));
        if first_para {
            let PasteBlock::Para(p) = blocks.remove(0) else {
                return Ok(at);
            };
            let mut ops = Vec::new();
            if keep && p.whole {
                ops.push(Op::SetParagraphProps {
                    para: at.para,
                    props: Box::new(p.props),
                });
            }
            if !p.content.is_empty() {
                ops.push(Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: p.content,
                });
            }
            self.batch_apply(batch, Transaction::new(ops))?;
        }
        // Last block: into the tail paragraph.
        let mut caret = Pos::new(tail, 0);
        if matches!(blocks.last(), Some(PasteBlock::Para(p)) if !p.whole)
            && let Some(PasteBlock::Para(p)) = blocks.pop()
        {
            let len = p.content.len();
            if len > 0 {
                self.batch_apply(
                    batch,
                    Transaction::new(vec![Op::InsertInline {
                        para: tail,
                        offset: 0,
                        fragment: p.content,
                    }]),
                )?;
            }
            caret = Pos::new(tail, len);
        }
        let mut middle = Vec::with_capacity(blocks.len());
        for b in blocks {
            middle.push(match b {
                PasteBlock::Para(p) => Block::Paragraph(self.paragraph_from(*p, keep, &dest)?),
                PasteBlock::Table(rows) => Block::Table(self.table_from(rows, keep, &dest, width)?),
            });
        }
        if !middle.is_empty() {
            self.batch_apply(
                batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index: index + 1,
                    blocks: middle,
                }]),
            )?;
        }
        Ok(caret)
    }

    /// Adds a list made by `make` within a batch.
    fn add_list_in(
        &mut self,
        batch: &mut super::Batch,
        make: impl FnOnce(&mut kiln_doc::Numbering) -> u32,
    ) -> Result<u32> {
        let mut n = self.doc.numbering.clone();
        let id = make(&mut n);
        let inst = n
            .instances
            .get(&id)
            .cloned()
            .ok_or_else(|| KilnError::Internal("a new list has no instance".into()))?;
        let abs = n
            .abstracts
            .get(&inst.abstract_id)
            .cloned()
            .ok_or_else(|| KilnError::Internal("a new list has no definition".into()))?;
        self.batch_apply(
            batch,
            Transaction::new(vec![
                Op::SetAbstractNum {
                    id: abs.id,
                    def: Some(Box::new(abs)),
                },
                Op::SetNumInstance {
                    id,
                    instance: Some(inst),
                },
            ]),
        )?;
        Ok(id)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn filtering_objects_keeps_runs_and_marks_aligned() {
        let mut f = Fragment::text("ab", RunProps::default());
        append(
            &mut f,
            Fragment::object(
                InlineObject::BookmarkStart { name: "x".into() },
                RunProps {
                    bold: Some(true),
                    ..RunProps::default()
                },
            ),
        );
        append(
            &mut f,
            Fragment::text("cd", RunProps::default()).with_mark(Mark::Comment(NodeId(9))),
        );
        let g = clean_marks(f);
        assert_eq!(g.text, "abcd");
        assert!(g.validate().is_ok());
        assert!(g.marks.is_empty() && g.objects.is_empty());
        assert_eq!(g.runs.iter().map(|r| r.len).sum::<usize>(), 4);
    }

    #[test]
    fn html_escaping() {
        assert_eq!(escape("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }
}
