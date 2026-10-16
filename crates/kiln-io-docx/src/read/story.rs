// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading of one *story* (the main body, a header or footer, a footnote, a comment, a text box):
//! block content (paragraphs, tables, content controls), inline content (runs, hyperlinks,
//! revisions, fields, symbols, bookmarks, comment ranges, drawings, equations) and the state that
//! spans paragraphs inside a story (open complex fields, open comment ranges, block-level
//! bookmarks).
//!
//! Mapping notes:
//! - Complex fields (`w:fldChar`) and simple fields (`w:fldSimple`) inside a paragraph become a
//!   [`Field`] object holding the instruction and the cached result text. Nested fields inside an
//!   instruction are written into the instruction the way Word displays field codes
//!   (`IF { MERGEFIELD Name } = …`).
//! - Generated-table fields (TOC, INDEX, BIBLIOGRAPHY, TOA) span paragraphs; they become a
//!   [`FieldBlock`] holding the result paragraphs.
//! - `HYPERLINK` fields become link marks on their result, exactly like `w:hyperlink`.
//! - `w:ins`/`w:del`/`w:moveTo`/`w:moveFrom` → revision marks; `w:rPrChange` → a format-change
//!   mark; `w:pPrChange` and paragraph-mark revisions → the paragraph's revision.
//! - Block content controls become [`SdtBlock`]s, inline ones [`Mark::Sdt`] marks; row- and
//!   cell-level controls are unwrapped and kept in the extras.

use std::rc::Rc;

use kiln_doc::fields::{FieldForm, FieldKind, parse_field_instruction};
use kiln_doc::revision::{ParagraphPropsChange, ParagraphRevision, RunPropsChange};
use kiln_doc::st::{BreakClear, BreakKind, NoteKind};
use kiln_doc::{
    Block, Break, Equation, Field, FieldBlock, Fragment, Hyperlink, InlineObject, Inlines,
    LinkTarget, Mark, MarkSpan, NodeId, NoteRef, OBJECT_CHAR, OpaqueBlock, Paragraph, RunProps,
    RunSpan, SdtBlock, SdtInfo, Symbol,
};
use kiln_ooxml::xml::{Attrs, XmlElement};

use super::context::{MAX_DEPTH, MAX_TEXT_BOX_DEPTH, ReadContext};
use super::drawing::{read_drawing, read_object, read_pict};
use super::props::{Raw, parse_paragraph_props, parse_revision, parse_run_props};
use super::sdt::parse_sdt_info;
use super::tables::read_table;
use crate::common::extras::{ParagraphExtras, RunExtras};
use crate::common::symbols::parse_symbol_code;
use crate::common::units::parse_on_off;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{XmlPath, attr, stepped};

/// Markup-compatibility namespaces whose `mc:Choice` branches the reader understands.
pub(crate) const SUPPORTED_MC: [&str; 29] = [
    "w14", "w15", "w16", "w16cex", "w16cid", "w16se", "w16sdtdh", "wp14", "wps", "wpg", "wpc",
    "wpi", "a14", "a16", "cx", "cx1", "cx2", "cx3", "cx4", "cx5", "cx6", "cx7", "cx8", "aink",
    "am3d", "v", "o", "w10", "m",
];

/// Fields whose result is a run of whole paragraphs.
const BLOCK_FIELDS: [&str; 4] = ["TOC", "INDEX", "BIBLIOGRAPHY", "TOA"];

/// The keyword of a field instruction (`TOC`, `PAGE`, `=` for formulas), upper-cased.
pub(crate) fn field_keyword(instr: &str) -> String {
    let t = instr.trim();
    if t.starts_with('=') {
        return "=".to_owned();
    }
    t.chars()
        .take_while(|c| !c.is_whitespace() && *c != '\\' && *c != '"' && *c != '{')
        .collect::<String>()
        .to_uppercase()
}

// ------------------------------------------------------------------ pieces

/// Unmodelled run properties shared by the pieces of one run.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RunXml {
    pub xml: Vec<String>,
    pub font_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum PieceKind {
    Text(String),
    Object(InlineObject),
}

/// A piece of paragraph content with its formatting, before it is assembled into [`Inlines`].
#[derive(Debug, Clone)]
pub(crate) struct Piece {
    pub kind: PieceKind,
    pub props: RunProps,
    pub marks: Vec<Mark>,
    pub run_xml: Option<Rc<RunXml>>,
}

impl Piece {
    pub(crate) fn object(object: InlineObject, props: RunProps, marks: Vec<Mark>) -> Self {
        Self {
            kind: PieceKind::Object(object),
            props,
            marks,
            run_xml: None,
        }
    }

    fn marker(object: InlineObject) -> Self {
        Self::object(object, RunProps::default(), Vec::new())
    }

    /// The text this piece contributes to a field's cached result, `None` for objects that are
    /// not text-like.
    fn result_text(&self) -> Option<String> {
        match &self.kind {
            PieceKind::Text(t) => Some(t.clone()),
            PieceKind::Object(InlineObject::Break(b)) if b.kind == BreakKind::TextWrapping => {
                Some("\n".to_owned())
            }
            PieceKind::Object(InlineObject::Field(f)) => Some(f.result.clone()),
            PieceKind::Object(
                InlineObject::BookmarkStart { .. } | InlineObject::BookmarkEnd { .. },
            ) => Some(String::new()),
            PieceKind::Object(_) => None,
        }
    }
}

/// Assembles pieces into paragraph content and the run extras of their ranges.
pub(crate) fn assemble(pieces: Vec<Piece>) -> (Inlines, Vec<RunExtras>) {
    let mut fragment = Fragment::default();
    let mut extras: Vec<RunExtras> = Vec::new();
    let mut pos = 0usize;
    for piece in pieces {
        let len = match piece.kind {
            PieceKind::Text(t) => {
                let mut n = 0;
                for c in t.chars().filter(|c| *c != OBJECT_CHAR) {
                    fragment.text.push(c);
                    n += 1;
                }
                n
            }
            PieceKind::Object(o) => {
                fragment.text.push(OBJECT_CHAR);
                fragment.objects.push(o);
                1
            }
        };
        if len == 0 {
            continue;
        }
        match fragment.runs.last_mut() {
            Some(last) if last.props == piece.props => last.len += len,
            _ => fragment.runs.push(RunSpan::new(len, piece.props)),
        }
        for m in piece.marks {
            fragment.marks.push(MarkSpan::new(pos, pos + len, m));
        }
        if let Some(rx) = piece.run_xml {
            match extras.last_mut() {
                Some(last)
                    if last.end == pos && last.xml == rx.xml && last.font_hint == rx.font_hint =>
                {
                    last.end += len;
                }
                _ => extras.push(RunExtras {
                    start: pos,
                    end: pos + len,
                    xml: rx.xml.clone(),
                    font_hint: rx.font_hint.clone(),
                }),
            }
        }
        pos += len;
    }
    // The fragment is consistent by construction (one object per object char, runs covering
    // every char); an error here would be a reader bug, and an empty paragraph is the safe
    // fallback.
    let inlines = Inlines::from_fragment(fragment).unwrap_or_default();
    (inlines, extras)
}

// ------------------------------------------------------------------ block sink

#[derive(Debug, Default)]
pub(crate) struct Group {
    pub blocks: Vec<Block>,
    pub sect_pr: Option<XmlElement>,
}

#[derive(Debug)]
struct TocState {
    field: Field,
    blocks: Vec<Block>,
}

/// Collects the blocks of a container, grouping generated-table paragraphs into field blocks
/// and splitting the body at paragraph-level section breaks.
#[derive(Debug)]
pub(crate) struct BlockSink {
    pub groups: Vec<Group>,
    toc: Option<TocState>,
}

impl Default for BlockSink {
    fn default() -> Self {
        Self {
            groups: vec![Group::default()],
            toc: None,
        }
    }
}

impl BlockSink {
    fn current(&mut self) -> &mut Vec<Block> {
        if self.groups.is_empty() {
            self.groups.push(Group::default());
        }
        let last = self.groups.len() - 1;
        &mut self.groups[last].blocks
    }

    fn add_paragraph(
        &mut self,
        ctx: &mut ReadContext<'_>,
        p: Block,
        toc_start: Option<Field>,
        toc_end: bool,
    ) {
        if let Some(field) = toc_start {
            self.close_toc(ctx);
            self.toc = Some(TocState {
                field,
                blocks: Vec::new(),
            });
        }
        match &mut self.toc {
            Some(t) => t.blocks.push(p),
            None => self.current().push(p),
        }
        if toc_end {
            self.close_toc(ctx);
        }
    }

    pub(crate) fn add_block(&mut self, ctx: &mut ReadContext<'_>, block: Block) {
        self.close_toc(ctx);
        self.current().push(block);
    }

    /// Ends the current section (body only): the blocks so far belong to `sect_pr`.
    fn section_break(&mut self, ctx: &mut ReadContext<'_>, sect_pr: XmlElement) {
        self.close_toc(ctx);
        if let Some(last) = self.groups.last_mut() {
            last.sect_pr = Some(sect_pr);
        }
        self.groups.push(Group::default());
    }

    pub(crate) fn close_toc(&mut self, ctx: &mut ReadContext<'_>) {
        let Some(t) = self.toc.take() else {
            return;
        };
        if t.blocks.is_empty() {
            return;
        }
        let block = Block::Field(FieldBlock {
            id: ctx.doc.alloc_id(),
            field: t.field,
            blocks: t.blocks,
        });
        self.current().push(block);
    }

    /// Moves the content of a nested sink (a block content control) into this one as content
    /// controls, keeping any section breaks inside it.
    fn absorb_sdt(&mut self, ctx: &mut ReadContext<'_>, info: &SdtInfo, mut inner: BlockSink) {
        inner.close_toc(ctx);
        for g in inner.groups {
            if !g.blocks.is_empty() {
                let block = Block::Sdt(SdtBlock {
                    id: ctx.doc.alloc_id(),
                    info: info.clone(),
                    blocks: g.blocks,
                });
                self.add_block(ctx, block);
            }
            if let Some(s) = g.sect_pr {
                self.section_break(ctx, s);
            }
        }
    }

    /// All blocks (for containers without sections).
    pub(crate) fn into_blocks(mut self, ctx: &mut ReadContext<'_>) -> Vec<Block> {
        self.close_toc(ctx);
        self.groups.into_iter().flat_map(|g| g.blocks).collect()
    }
}

// ------------------------------------------------------------------ fields

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Instr,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The result is buffered and summarised into a field object.
    Collect,
    /// The result paragraphs form a field block.
    Block,
    /// The result flows into the paragraph with a link mark.
    Link,
    /// The field broke across paragraphs; its result flows into the paragraph unwrapped.
    Flat,
}

#[derive(Debug)]
struct FieldFrame {
    instr: String,
    phase: Phase,
    mode: Mode,
    result: Vec<Piece>,
    props: RunProps,
    marks: Vec<Mark>,
    dirty: bool,
    locked: bool,
    form_field: bool,
    nested_in_instr: bool,
    simple: bool,
    link: Option<Mark>,
    path: String,
}

// ------------------------------------------------------------------ story reader

/// Options of one story.
#[derive(Debug, Clone)]
pub(crate) struct StoryOptions {
    /// The part (`/word/document.xml`).
    pub part: String,
    /// Attributes of the part's root element (namespace declarations for raw XML).
    pub scope: Attrs,
    /// The path of the container being read.
    pub path: XmlPath,
    /// Set when reading a footnote or endnote body (for `w:footnoteRef`).
    pub note: Option<(NodeId, NoteKind)>,
}

pub(crate) struct StoryReader<'c, 'p> {
    pub ctx: &'c mut ReadContext<'p>,
    pub part: String,
    pub scope: Attrs,
    pub path: XmlPath,
    note: Option<(NodeId, NoteKind)>,
    /// Open comment ranges, in start order.
    active_comments: Vec<NodeId>,
    fields: Vec<FieldFrame>,
    /// Block-level bookmarks waiting for the next paragraph.
    pending: Vec<Piece>,
    /// Inline output of the paragraph being read.
    out: Vec<Piece>,
    toc_start: Option<Field>,
    toc_end: bool,
    custom_mark_pending: Option<(NoteRef, RunProps, Vec<Mark>)>,
    /// The raw XML of the `mc:AlternateContent` being read at run level: a drawing chosen from
    /// it keeps the whole block (with its fallback) so Word gets back exactly what it wrote.
    alternate_xml: Option<String>,
}

impl<'c, 'p> StoryReader<'c, 'p> {
    pub(crate) fn new(ctx: &'c mut ReadContext<'p>, options: StoryOptions) -> Self {
        Self {
            ctx,
            part: options.part,
            scope: options.scope,
            path: options.path,
            note: options.note,
            active_comments: Vec::new(),
            fields: Vec::new(),
            pending: Vec::new(),
            out: Vec::new(),
            toc_start: None,
            toc_end: false,
            custom_mark_pending: None,
            alternate_xml: None,
        }
    }

    pub(crate) fn raw(&self) -> Raw<'_> {
        Raw { scope: &self.scope }
    }

    pub(crate) fn raw_xml(&self, el: &XmlElement) -> String {
        Raw { scope: &self.scope }.xml(el)
    }

    pub(crate) fn warn(&mut self, code: WarningCode, severity: Severity, message: &str) {
        let path = self.path.to_string();
        let part = self.part.clone();
        self.ctx.warn(code, severity, message, &part, &path);
    }

    /// Reads a container's blocks into a fresh story (a text box), sharing this story's part.
    pub(crate) fn read_nested_story(&mut self, container: &XmlElement) -> Option<Vec<Block>> {
        if self.ctx.text_box_depth >= MAX_TEXT_BOX_DEPTH {
            self.warn(
                WarningCode::NestingTooDeep,
                Severity::Warning,
                "A text box inside text boxes is kept in the file, but its text is not shown.",
            );
            return None;
        }
        self.ctx.text_box_depth += 1;
        let options = StoryOptions {
            part: self.part.clone(),
            scope: self.scope.clone(),
            path: self.path.clone(),
            note: self.note,
        };
        let mut inner = StoryReader::new(&mut *self.ctx, options);
        let mut sink = BlockSink::default();
        inner.read_blocks(container, &mut sink);
        let mut blocks = sink.into_blocks(inner.ctx);
        inner.finish_story(&mut blocks);
        self.ctx.text_box_depth -= 1;
        Some(blocks)
    }

    // ---------------------------------------------------------------- blocks

    /// Reads the block children of a container (body, cell, header, note, comment,
    /// `w:sdtContent`, text box).
    pub(crate) fn read_blocks(&mut self, container: &XmlElement, sink: &mut BlockSink) {
        if !self.enter() {
            return;
        }
        for (el, step) in stepped(container) {
            self.path.push(step);
            self.read_block(el, sink);
            self.path.pop();
        }
        self.leave();
    }

    /// Enters one container level; `false` (with a diagnostic) beyond [`MAX_DEPTH`].
    pub(crate) fn enter(&mut self) -> bool {
        if self.ctx.depth >= MAX_DEPTH {
            self.warn(
                WarningCode::NestingTooDeep,
                Severity::Warning,
                "Content nested deeper than Kiln follows was left out.",
            );
            return false;
        }
        self.ctx.depth += 1;
        true
    }

    /// Leaves a level entered with [`Self::enter`].
    pub(crate) fn leave(&mut self) {
        self.ctx.depth = self.ctx.depth.saturating_sub(1);
    }

    fn read_block(&mut self, el: &XmlElement, sink: &mut BlockSink) {
        match el.name.as_str() {
            "w:p" => self.read_paragraph(el, sink),
            "w:tbl" => {
                let table = read_table(self, el);
                sink.add_block(self.ctx, table);
            }
            "w:sdt" => {
                let info = parse_sdt_info(el, self.raw());
                if let Some(content) = el.child("w:sdtContent") {
                    self.path.push("w:sdtContent[1]".to_owned());
                    let mut inner = BlockSink::default();
                    self.read_blocks(content, &mut inner);
                    self.path.pop();
                    sink.absorb_sdt(self.ctx, &info, inner);
                }
            }
            "w:customXml" | "w:smartTag" | "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo" => {
                // Wrappers (and block-level revision containers) only hold further blocks.
                self.read_blocks(el, sink);
            }
            "w:customXmlPr"
            | "w:smartTagPr"
            | "w:sectPr"
            | "w:tcPr"
            | "w:proofErr"
            | "w:moveFromRangeStart"
            | "w:moveFromRangeEnd"
            | "w:moveToRangeStart"
            | "w:moveToRangeEnd"
            | "w:customXmlInsRangeStart"
            | "w:customXmlInsRangeEnd"
            | "w:customXmlDelRangeStart"
            | "w:customXmlDelRangeEnd"
            | "w:customXmlMoveFromRangeStart"
            | "w:customXmlMoveFromRangeEnd"
            | "w:customXmlMoveToRangeStart"
            | "w:customXmlMoveToRangeEnd" => {}
            "w:bookmarkStart" | "w:bookmarkEnd" | "w:commentRangeStart" | "w:commentRangeEnd" => {
                self.read_marker(el);
            }
            "mc:AlternateContent" => {
                if self.enter() {
                    for c in el.select_alternate_content(&SUPPORTED_MC) {
                        self.read_block(c, sink);
                    }
                    self.leave();
                }
            }
            "w:permStart" | "w:permEnd" => self.warn(
                WarningCode::Permission,
                Severity::Warning,
                "Editing permission ranges are not supported and were removed.",
            ),
            "w:altChunk" => {
                self.warn(
                    WarningCode::AltChunk,
                    Severity::Info,
                    "Imported content (altChunk) is kept in the file but not shown; Word merges it on open.",
                );
                if let Some(id) = attr(el, "r:id") {
                    let part = self.part.clone();
                    let _ = self.ctx.relationship(&part, id);
                }
                let block = Block::Opaque(OpaqueBlock {
                    id: self.ctx.doc.alloc_id(),
                    xml: self.raw_xml(el),
                    label: "Imported content".to_owned(),
                });
                sink.add_block(self.ctx, block);
            }
            name => {
                let msg = format!("Unsupported element {name} was skipped.");
                self.warn(WarningCode::ElementUnknown, Severity::Warning, &msg);
            }
        }
    }

    // ---------------------------------------------------------------- paragraphs

    fn read_paragraph(&mut self, p: &XmlElement, sink: &mut BlockSink) {
        let parsed = parse_paragraph_props(p.child("w:pPr"), self.raw());
        self.out = std::mem::take(&mut self.pending);
        self.toc_start = None;
        self.toc_end = false;

        for (el, step) in stepped(p) {
            if el.name == "w:pPr" {
                continue;
            }
            self.path.push(step);
            self.read_inline(el, &[]);
            self.path.pop();
        }
        self.break_open_fields();

        let mut revision = ParagraphRevision::default();
        if let Some((info, before)) = parsed.change {
            revision.props_change = Some(Box::new(ParagraphPropsChange { info, before }));
        }
        if let Some(mr) = parsed.mark.mark_revision {
            if mr.moved {
                self.warn_move();
            }
            revision.mark = Some(mr.revision);
        }
        if let Some((info, before)) = parsed.mark.change {
            revision.mark_props_change = Some(Box::new(RunPropsChange { info, before }));
        }

        let (content, runs) = assemble(std::mem::take(&mut self.out));
        let id = self.ctx.doc.alloc_id();
        let extras = ParagraphExtras {
            ppr_xml: parsed.xml,
            mark_xml: parsed.mark.xml,
            para_id: p.attr("w14:paraId").map(str::to_owned),
            text_id: p.attr("w14:textId").map(str::to_owned),
            runs,
        };
        if !extras.is_empty() {
            self.ctx.extras.paragraphs.insert(id, extras);
        }
        let paragraph = Paragraph {
            id,
            props: parsed.props,
            mark_props: parsed.mark.props,
            revision,
            content,
        };
        let toc_start = self.toc_start.take();
        let toc_end = std::mem::take(&mut self.toc_end);
        sink.add_paragraph(self.ctx, Block::Paragraph(paragraph), toc_start, toc_end);
        if let Some(sect) = parsed.sect_pr {
            sink.section_break(self.ctx, sect.clone());
        }
    }

    /// The paragraph ended inside a field that collects its result: release it into the text.
    fn break_open_fields(&mut self) {
        let mut warned = false;
        for i in 0..self.fields.len() {
            let f = &mut self.fields[i];
            if f.phase == Phase::Result && f.mode == Mode::Collect {
                f.mode = Mode::Flat;
                let buffered = std::mem::take(&mut f.result);
                let instr = f.instr.trim().to_owned();
                let path = f.path.clone();
                for piece in buffered {
                    self.emit_from(i.checked_sub(1), piece);
                }
                if !warned {
                    warned = true;
                    let part = self.part.clone();
                    self.ctx.warn(
                        WarningCode::FieldMultiParagraph,
                        Severity::Warning,
                        format!(
                            "The field \"{instr}\" spans several paragraphs; its result was kept as plain text."
                        ),
                        &part,
                        &path,
                    );
                }
            }
        }
    }

    /// Ends the story: reports fields that never ended and attaches trailing bookmarks to the
    /// last paragraph. Guarantees at least one block.
    pub(crate) fn finish_story(&mut self, blocks: &mut Vec<Block>) {
        if let Some(f) = self.fields.first() {
            let msg = format!(
                "The field \"{}\" has no end; its result was kept as plain text.",
                f.instr.trim()
            );
            let path = f.path.clone();
            let part = self.part.clone();
            self.ctx.warn(
                WarningCode::FieldUnterminated,
                Severity::Warning,
                msg,
                &part,
                &path,
            );
            self.fields.clear();
        }
        self.active_comments.clear();
        let pending = std::mem::take(&mut self.pending);
        if !pending.is_empty() {
            if let Some(Block::Paragraph(p)) = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, Block::Paragraph(_)))
            {
                let (tail, _) = assemble(pending);
                if let Ok(fragment) = tail.slice(0, tail.len()) {
                    let at = p.content.len();
                    // Markers carry default formatting and no marks; inserting them cannot
                    // fail on a valid paragraph.
                    let _ = p.content.insert(at, fragment);
                }
            } else {
                let (content, _) = assemble(pending);
                let mut p = Paragraph::new(self.ctx.doc.alloc_id());
                p.content = content;
                blocks.push(Block::Paragraph(p));
            }
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Paragraph::new(self.ctx.doc.alloc_id())));
        }
    }

    // ---------------------------------------------------------------- emission

    /// Context marks that apply to everything emitted now (comments, hyperlink fields).
    fn ambient_marks(&self) -> Vec<Mark> {
        let mut out: Vec<Mark> = self
            .active_comments
            .iter()
            .map(|c| Mark::Comment(*c))
            .collect();
        for f in &self.fields {
            if f.phase == Phase::Result
                && let Some(link) = &f.link
            {
                out.push(link.clone());
            }
        }
        out
    }

    fn emit(&mut self, piece: Piece) {
        self.emit_from(self.fields.len().checked_sub(1), piece);
    }

    /// Routes output to the innermost collecting field at or below `index`, else the paragraph.
    fn emit_from(&mut self, index: Option<usize>, piece: Piece) {
        if let Some(top) = index {
            for i in (0..=top.min(self.fields.len().saturating_sub(1))).rev() {
                let f = &mut self.fields[i];
                if f.phase == Phase::Instr {
                    return; // display text inside a field code is not part of the document
                }
                if f.mode == Mode::Collect {
                    f.result.push(piece);
                    return;
                }
            }
        }
        self.note_comments(&piece);
        self.out.push(piece);
    }

    fn note_comments(&mut self, piece: &Piece) {
        for m in &piece.marks {
            if let Mark::Comment(id) = m {
                self.ctx.anchored_comments.insert(*id);
            }
        }
    }

    /// Emits an already-built object (drawings, equations) with the ambient marks.
    pub(crate) fn emit_object(&mut self, object: InlineObject, props: RunProps, marks: Vec<Mark>) {
        self.emit(Piece::object(object, props, marks));
    }

    fn with_ambient(&self, mut marks: Vec<Mark>) -> Vec<Mark> {
        for m in self.ambient_marks() {
            if !marks.contains(&m) {
                marks.push(m);
            }
        }
        marks
    }

    // ---------------------------------------------------------------- inline

    fn read_inline_children(&mut self, el: &XmlElement, ctx_marks: &[Mark]) {
        if !self.enter() {
            return;
        }
        for (c, step) in stepped(el) {
            if c.name == "w:sdtPr" || c.name == "w:sdtEndPr" {
                continue;
            }
            self.path.push(step);
            self.read_inline(c, ctx_marks);
            self.path.pop();
        }
        self.leave();
    }

    fn with_mark(ctx_marks: &[Mark], mark: Option<Mark>) -> Vec<Mark> {
        let mut v = ctx_marks.to_vec();
        if let Some(m) = mark {
            v.push(m);
        }
        v
    }

    pub(crate) fn read_inline(&mut self, el: &XmlElement, ctx_marks: &[Mark]) {
        match el.name.as_str() {
            "w:r" => self.read_run(el, ctx_marks),
            "w:hyperlink" => {
                let link = self.hyperlink_mark(el);
                let marks = Self::with_mark(ctx_marks, link);
                self.read_inline_children(el, &marks);
            }
            "w:ins" | "w:moveTo" | "w:del" | "w:moveFrom" => {
                let info = parse_revision(el);
                let mark = match el.name.as_str() {
                    "w:ins" => Mark::Insertion(info),
                    "w:del" => Mark::Deletion(info),
                    "w:moveTo" => Mark::MoveTo(info),
                    _ => Mark::MoveFrom(info),
                };
                if matches!(el.name.as_str(), "w:moveTo" | "w:moveFrom") {
                    self.warn_move();
                }
                let marks = Self::with_mark(ctx_marks, Some(mark));
                self.read_inline_children(el, &marks);
            }
            "w:fldSimple" => self.simple_field(el, ctx_marks),
            "w:sdt" => {
                let info = parse_sdt_info(el, self.raw());
                if let Some(content) = el.child("w:sdtContent") {
                    let marks = Self::with_mark(ctx_marks, Some(Mark::Sdt(Box::new(info))));
                    self.path.push("w:sdtContent[1]".to_owned());
                    self.read_inline_children(content, &marks);
                    self.path.pop();
                }
            }
            "w:smartTag" | "w:customXml" | "w:dir" | "w:bdo" => {
                self.read_inline_children(el, ctx_marks);
            }
            "w:bookmarkStart" | "w:bookmarkEnd" => {
                // Bookmarks mark positions in the document, never inside a field's cached
                // result.
                if let Some(piece) = self.bookmark(el) {
                    self.out.push(piece);
                }
            }
            "w:commentRangeStart" | "w:commentRangeEnd" => self.comment_range(el),
            "m:oMath" | "m:oMathPara" => {
                let eq = InlineObject::Equation(Equation {
                    omml: self.raw_xml(el),
                    display: el.name == "m:oMathPara",
                });
                let marks = self.with_ambient(ctx_marks.to_vec());
                self.emit_object(eq, RunProps::default(), marks);
            }
            "mc:AlternateContent" => {
                if self.enter() {
                    for c in el.select_alternate_content(&SUPPORTED_MC) {
                        self.read_inline(c, ctx_marks);
                    }
                    self.leave();
                }
            }
            "w:permStart" | "w:permEnd" => self.warn(
                WarningCode::Permission,
                Severity::Warning,
                "Editing permission ranges are not supported and were removed.",
            ),
            "w:subDoc" => self.warn(
                WarningCode::SubDocument,
                Severity::Warning,
                "Links to subdocuments (master documents) are not supported.",
            ),
            "w:customXmlPr"
            | "w:smartTagPr"
            | "w:proofErr"
            | "w:moveFromRangeStart"
            | "w:moveFromRangeEnd"
            | "w:moveToRangeStart"
            | "w:moveToRangeEnd"
            | "w:customXmlInsRangeStart"
            | "w:customXmlInsRangeEnd"
            | "w:customXmlDelRangeStart"
            | "w:customXmlDelRangeEnd"
            | "w:customXmlMoveFromRangeStart"
            | "w:customXmlMoveFromRangeEnd"
            | "w:customXmlMoveToRangeStart"
            | "w:customXmlMoveToRangeEnd" => {}
            name => {
                let msg = format!("Unsupported element {name} was skipped.");
                self.warn(WarningCode::ElementUnknown, Severity::Warning, &msg);
            }
        }
    }

    fn warn_move(&mut self) {
        self.warn(
            WarningCode::RevisionMove,
            Severity::Info,
            "A tracked move is kept as moved-away and moved-here text; Word will no longer link the two halves.",
        );
    }

    fn hyperlink_mark(&mut self, el: &XmlElement) -> Option<Mark> {
        let mut url = None;
        if let Some(id) = attr(el, "r:id") {
            let part = self.part.clone();
            if let Some(rel) = self.ctx.relationship(&part, id) {
                url = Some(rel.target);
                self.ctx.consume_rel(&part, id);
            }
        }
        let anchor = attr(el, "w:anchor")
            .or_else(|| attr(el, "w:docLocation"))
            .filter(|a| !a.is_empty());
        let target = match (url, anchor) {
            (Some(u), Some(a)) => LinkTarget::Url(format!("{u}#{a}")),
            (Some(u), None) => LinkTarget::Url(u),
            (None, Some(a)) => LinkTarget::Bookmark(a.to_owned()),
            (None, None) => return None,
        };
        Some(Mark::Link(Hyperlink {
            target,
            tooltip: attr(el, "w:tooltip").map(str::to_owned),
            frame: attr(el, "w:tgtFrame").map(str::to_owned),
            history: parse_on_off(attr(el, "w:history"), false),
        }))
    }

    fn bookmark(&mut self, el: &XmlElement) -> Option<Piece> {
        let id = attr(el, "w:id").unwrap_or_default().to_owned();
        if el.name == "w:bookmarkStart" {
            let name = attr(el, "w:name").unwrap_or_default().to_owned();
            self.ctx.bookmark_names.insert(id, name.clone());
            Some(Piece::marker(InlineObject::BookmarkStart { name }))
        } else {
            let name = self.ctx.bookmark_names.get(&id).cloned()?;
            Some(Piece::marker(InlineObject::BookmarkEnd { name }))
        }
    }

    /// A bookmark or comment-range marker outside any paragraph (between blocks, rows or
    /// cells): bookmarks go to the next paragraph.
    pub(crate) fn read_marker(&mut self, el: &XmlElement) {
        if el.name == "w:bookmarkStart" || el.name == "w:bookmarkEnd" {
            if let Some(piece) = self.bookmark(el) {
                self.pending.push(piece);
            }
        } else {
            self.comment_range(el);
        }
    }

    fn comment_range(&mut self, el: &XmlElement) {
        let Some(id) = attr(el, "w:id") else {
            return;
        };
        let Some(node) = self.ctx.comment_ids.get(id).copied() else {
            return;
        };
        if el.name == "w:commentRangeStart" {
            if !self.active_comments.contains(&node) {
                self.active_comments.push(node);
            }
        } else {
            self.active_comments.retain(|c| *c != node);
        }
    }

    // ---------------------------------------------------------------- runs

    fn read_run(&mut self, r: &XmlElement, ctx_marks: &[Mark]) {
        let parsed = parse_run_props(r.child("w:rPr"), self.raw());
        let mut marks = ctx_marks.to_vec();
        if let Some((info, before)) = &parsed.change {
            marks.push(Mark::FormatChange {
                info: info.clone(),
                before: Box::new(before.clone()),
            });
        }
        let run_xml = (!parsed.xml.is_empty() || parsed.font_hint.is_some()).then(|| {
            Rc::new(RunXml {
                xml: parsed.xml.clone(),
                font_hint: parsed.font_hint.clone(),
            })
        });
        let run = RunCtx {
            props: parsed.props,
            marks,
            run_xml,
        };
        for (c, step) in stepped(r) {
            self.path.push(step);
            self.read_run_child(c, &run);
            self.path.pop();
        }
        if let Some((note, props, marks)) = self.custom_mark_pending.take() {
            // A custom mark announced without text: the reference stands alone.
            self.emit_note_ref(note, props, marks);
        }
    }

    fn text_piece(&self, text: String, run: &RunCtx) -> Piece {
        Piece {
            kind: PieceKind::Text(text),
            props: run.props.clone(),
            marks: self.with_ambient(run.marks.clone()),
            run_xml: run.run_xml.clone(),
        }
    }

    fn emit_text(&mut self, text: String, run: &RunCtx) {
        if text.is_empty() {
            return;
        }
        let piece = self.text_piece(text, run);
        self.emit(piece);
    }

    fn emit_run_object(&mut self, object: InlineObject, run: &RunCtx) {
        let piece = Piece {
            kind: PieceKind::Object(object),
            props: run.props.clone(),
            marks: self.with_ambient(run.marks.clone()),
            run_xml: run.run_xml.clone(),
        };
        self.emit(piece);
    }

    fn emit_note_ref(&mut self, note: NoteRef, props: RunProps, marks: Vec<Mark>) {
        self.emit(Piece::object(InlineObject::NoteRef(note), props, marks));
    }

    fn read_run_child(&mut self, c: &XmlElement, run: &RunCtx) {
        match c.name.as_str() {
            "w:rPr" => {}
            "w:t" | "w:delText" => {
                let t = c.text();
                if let Some((mut note, props, marks)) = self.custom_mark_pending.take() {
                    // w:customMarkFollows: this text is the note's custom reference mark.
                    note.custom_mark = Some(t);
                    self.emit_note_ref(note, props, marks);
                    return;
                }
                self.emit_text(t, run);
            }
            "w:tab" => self.emit_text("\t".to_owned(), run),
            "w:ptab" => {
                self.warn(
                    WarningCode::ElementUnknown,
                    Severity::Warning,
                    "An alignment tab (positional tab) was converted to an ordinary tab.",
                );
                self.emit_text("\t".to_owned(), run);
            }
            "w:br" => {
                let kind = match attr(c, "w:type") {
                    Some("page") => BreakKind::Page,
                    Some("column") => BreakKind::Column,
                    _ => BreakKind::TextWrapping,
                };
                let clear = attr(c, "w:clear")
                    .and_then(BreakClear::parse)
                    .filter(|c| *c != BreakClear::None);
                self.emit_run_object(InlineObject::Break(Break { kind, clear }), run);
            }
            "w:cr" => self.emit_run_object(
                InlineObject::Break(Break {
                    kind: BreakKind::TextWrapping,
                    clear: None,
                }),
                run,
            ),
            "w:noBreakHyphen" => self.emit_text("\u{2011}".to_owned(), run),
            "w:softHyphen" => self.emit_text("\u{00AD}".to_owned(), run),
            "w:sym" => {
                let font = attr(c, "w:font").unwrap_or_default().to_owned();
                match attr(c, "w:char").and_then(parse_symbol_code) {
                    Some(code) => {
                        self.emit_run_object(InlineObject::Symbol(Symbol { font, code }), run);
                    }
                    None => self.warn(
                        WarningCode::ElementUnknown,
                        Severity::Warning,
                        "A symbol character without a valid code was skipped.",
                    ),
                }
            }
            "w:fldChar" => self.field_char(c, run),
            "w:instrText" | "w:delInstrText" => {
                if let Some(top) = self.fields.last_mut()
                    && top.phase == Phase::Instr
                {
                    top.instr.push_str(&c.text());
                }
            }
            "w:footnoteReference" | "w:endnoteReference" => {
                let kind = if c.name == "w:footnoteReference" {
                    NoteKind::Footnote
                } else {
                    NoteKind::Endnote
                };
                let id = attr(c, "w:id").unwrap_or_default().to_owned();
                let Some(node) = self.ctx.note_ids.get(&(kind, id.clone())).copied() else {
                    let msg = format!(
                        "A {kind} reference points to {kind} {id}, which the file does not contain; the reference was removed."
                    );
                    self.warn(WarningCode::NoteMissing, Severity::Warning, &msg);
                    return;
                };
                let note = NoteRef {
                    note: node,
                    kind,
                    custom_mark: None,
                };
                let marks = self.with_ambient(run.marks.clone());
                if parse_on_off(attr(c, "w:customMarkFollows"), false) {
                    self.custom_mark_pending = Some((note, run.props.clone(), marks));
                    return;
                }
                self.emit_note_ref(note, run.props.clone(), marks);
            }
            "w:footnoteRef" | "w:endnoteRef" => {
                if let Some((note, kind)) = self.note {
                    self.emit_run_object(
                        InlineObject::NoteRef(NoteRef {
                            note,
                            kind,
                            custom_mark: None,
                        }),
                        run,
                    );
                }
            }
            "w:separator"
            | "w:continuationSeparator"
            | "w:annotationRef"
            | "w:lastRenderedPageBreak" => {}
            "w:commentReference" => {
                if let Some(id) = attr(c, "w:id") {
                    self.ctx.referenced_comments.insert(id.to_owned());
                }
            }
            "w:pgNum" => self.emit_run_object(
                InlineObject::Field(Field {
                    instr: "PAGE".to_owned(),
                    result: String::new(),
                    form: FieldForm::Simple,
                    dirty: false,
                    locked: false,
                }),
                run,
            ),
            "w:drawing" => {
                let marks = self.with_ambient(run.marks.clone());
                if let Some(o) = read_drawing(self, c) {
                    self.emit(Piece {
                        kind: PieceKind::Object(o),
                        props: run.props.clone(),
                        marks,
                        run_xml: run.run_xml.clone(),
                    });
                }
            }
            "w:pict" => {
                let marks = self.with_ambient(run.marks.clone());
                if let Some(o) = read_pict(self, c) {
                    self.emit(Piece {
                        kind: PieceKind::Object(o),
                        props: run.props.clone(),
                        marks,
                        run_xml: run.run_xml.clone(),
                    });
                }
            }
            "w:object" => {
                let o = read_object(self, c);
                self.emit_run_object(o, run);
            }
            "w:contentPart" => {
                self.warn(
                    WarningCode::DrawingInk,
                    Severity::Info,
                    "Ink is kept in the file but cannot be edited.",
                );
                let xml = self
                    .alternate_xml
                    .clone()
                    .unwrap_or_else(|| self.raw_xml(c));
                self.emit_run_object(
                    InlineObject::Opaque {
                        xml,
                        label: "Ink".to_owned(),
                    },
                    run,
                );
            }
            "mc:AlternateContent" => {
                if !self.enter() {
                    return;
                }
                let outer = self.alternate_xml.take();
                self.alternate_xml = Some(outer.clone().unwrap_or_else(|| self.raw_xml(c)));
                for k in c.select_alternate_content(&SUPPORTED_MC) {
                    self.read_run_child(k, run);
                }
                self.alternate_xml = outer;
                self.leave();
            }
            "w:ruby" => {
                self.warn(
                    WarningCode::ElementUnknown,
                    Severity::Warning,
                    "Phonetic guide (ruby) text was dropped; the base text is kept.",
                );
                if let Some(base) = c.child("w:rubyBase") {
                    let marks = run.marks.clone();
                    self.read_inline_children(base, &marks);
                }
            }
            name => {
                let msg = format!("Unsupported run content {name} was skipped.");
                self.warn(WarningCode::ElementUnknown, Severity::Warning, &msg);
            }
        }
    }

    /// The raw XML of the `mc:AlternateContent` being read, if any.
    pub(crate) fn alternate_xml(&self) -> Option<&str> {
        self.alternate_xml.as_deref()
    }

    // ---------------------------------------------------------------- fields

    fn field_char(&mut self, c: &XmlElement, run: &RunCtx) {
        match attr(c, "w:fldCharType") {
            Some("begin") => {
                let nested_in_instr = self.fields.last().is_some_and(|f| f.phase == Phase::Instr);
                let marks = self.with_ambient(run.marks.clone());
                self.fields.push(FieldFrame {
                    instr: String::new(),
                    phase: Phase::Instr,
                    mode: Mode::Collect,
                    result: Vec::new(),
                    props: run.props.clone(),
                    marks,
                    dirty: parse_on_off(attr(c, "w:dirty"), false),
                    locked: parse_on_off(attr(c, "w:fldLock"), false),
                    form_field: c.child("w:ffData").is_some(),
                    nested_in_instr,
                    simple: false,
                    link: None,
                    path: self.path.to_string(),
                });
            }
            Some("separate") => {
                let depth = self.fields.len();
                if let Some(top) = self.fields.last_mut()
                    && top.phase == Phase::Instr
                {
                    top.phase = Phase::Result;
                    self.choose_mode(depth - 1);
                }
            }
            Some("end") => {
                if let Some(top) = self.fields.pop() {
                    self.end_field(top);
                }
            }
            _ => {}
        }
    }

    fn choose_mode(&mut self, index: usize) {
        let depth = self.fields.len();
        let Some(f) = self.fields.get_mut(index) else {
            return;
        };
        if f.nested_in_instr {
            return; // the result is part of the parent's code
        }
        let kw = field_keyword(&f.instr);
        if kw == "HYPERLINK"
            && let FieldKind::Hyperlink(h) = parse_field_instruction(f.instr.trim()).kind
        {
            let target = match (h.url, h.anchor) {
                (Some(u), Some(a)) => Some(LinkTarget::Url(format!("{u}#{a}"))),
                (Some(u), None) => Some(LinkTarget::Url(u)),
                (None, Some(a)) => Some(LinkTarget::Bookmark(a)),
                (None, None) => None,
            };
            if let Some(target) = target {
                f.mode = Mode::Link;
                f.link = Some(Mark::Link(Hyperlink {
                    target,
                    tooltip: h.tooltip,
                    frame: h
                        .target
                        .or_else(|| h.new_window.then(|| "_blank".to_owned())),
                    history: true,
                }));
                return;
            }
        }
        if BLOCK_FIELDS.contains(&kw.as_str()) && depth == 1 && !f.simple {
            f.mode = Mode::Block;
            self.toc_start = Some(Field {
                instr: f.instr.trim().to_owned(),
                result: String::new(),
                form: FieldForm::Complex,
                dirty: f.dirty,
                locked: f.locked,
            });
        }
    }

    fn end_field(&mut self, mut f: FieldFrame) {
        let instr = f.instr.trim().to_owned();
        if f.nested_in_instr
            && let Some(parent) = self.fields.last_mut()
            && parent.phase == Phase::Instr
        {
            parent.instr.push_str(&format!("{{ {instr} }}"));
            return;
        }
        // A field without a separator has no cached result (a hidden TC or XE entry).
        f.phase = Phase::Result;
        match f.mode {
            Mode::Block => {
                self.toc_end = true;
                return;
            }
            Mode::Link | Mode::Flat => return,
            Mode::Collect => {}
        }
        let kw = field_keyword(&instr);
        if kw == "HYPERLINK" || BLOCK_FIELDS.contains(&kw.as_str()) {
            // A HYPERLINK without a usable target, or a generated table nested in another
            // field: keep the result as it is shown.
            for piece in f.result {
                self.emit(piece);
            }
            return;
        }
        let texts: Option<Vec<String>> = f.result.iter().map(Piece::result_text).collect();
        let Some(texts) = texts else {
            let what = if f
                .result
                .iter()
                .any(|p| matches!(p.kind, PieceKind::Object(InlineObject::Drawing(_))))
            {
                "a picture"
            } else {
                "an object"
            };
            let msg = format!(
                "The result of the field \"{instr}\" contains {what}; it was kept and the field code removed."
            );
            let part = self.part.clone();
            self.ctx.warn(
                WarningCode::FieldUnwrapped,
                Severity::Warning,
                msg,
                &part,
                &f.path,
            );
            for piece in f.result {
                self.emit(piece);
            }
            return;
        };
        if f.form_field {
            let part = self.part.clone();
            self.ctx.warn(
                WarningCode::FieldFormField,
                Severity::Warning,
                format!(
                    "Legacy form field {kw} was converted to a plain field; its form settings were dropped."
                ),
                &part,
                &f.path,
            );
        }
        let first_text = f
            .result
            .iter()
            .find(|p| matches!(p.kind, PieceKind::Text(_)));
        let (props, marks, run_xml) = match first_text {
            Some(p) => (p.props.clone(), p.marks.clone(), p.run_xml.clone()),
            None => (f.props.clone(), f.marks.clone(), None),
        };
        let field = Field {
            instr,
            result: texts.concat(),
            form: if f.simple {
                FieldForm::Simple
            } else {
                FieldForm::Complex
            },
            dirty: f.dirty,
            locked: f.locked,
        };
        self.emit(Piece {
            kind: PieceKind::Object(InlineObject::Field(field)),
            props,
            marks,
            run_xml,
        });
    }

    fn simple_field(&mut self, el: &XmlElement, ctx_marks: &[Mark]) {
        let instr = attr(el, "w:instr").unwrap_or_default().to_owned();
        let marks = self.with_ambient(ctx_marks.to_vec());
        let frame = FieldFrame {
            instr,
            phase: Phase::Result,
            mode: Mode::Collect,
            result: Vec::new(),
            props: RunProps::default(),
            marks,
            dirty: parse_on_off(attr(el, "w:dirty"), false),
            locked: parse_on_off(attr(el, "w:fldLock"), false),
            form_field: false,
            nested_in_instr: false,
            simple: true,
            link: None,
            path: self.path.to_string(),
        };
        self.fields.push(frame);
        let index = self.fields.len() - 1;
        if field_keyword(&self.fields[index].instr) == "HYPERLINK" {
            self.choose_mode(index);
        }
        self.read_inline_children(el, ctx_marks);
        // A simple field never spans paragraphs; pop it whatever happened inside.
        if self.fields.len() > index {
            let frame = self.fields.remove(index);
            self.fields.truncate(index);
            self.end_field(frame);
        }
    }
}

/// The formatting of the run being read.
pub(crate) struct RunCtx {
    pub props: RunProps,
    pub marks: Vec<Mark>,
    pub run_xml: Option<Rc<RunXml>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_keywords() {
        assert_eq!(field_keyword(" toc \\o \"1-3\""), "TOC");
        assert_eq!(field_keyword("= 2 + 3"), "=");
        assert_eq!(field_keyword("MERGEFIELD{x}"), "MERGEFIELD");
        assert_eq!(field_keyword(""), "");
    }

    #[test]
    fn assembling_merges_equal_runs_and_records_extras() {
        let bold = RunProps {
            bold: Some(true),
            ..RunProps::default()
        };
        let rx = Some(Rc::new(RunXml {
            xml: vec!["<w:noProof/>".into()],
            font_hint: None,
        }));
        let text = |t: &str, props: &RunProps, run_xml: &Option<Rc<RunXml>>| Piece {
            kind: PieceKind::Text(t.into()),
            props: props.clone(),
            marks: Vec::new(),
            run_xml: run_xml.clone(),
        };
        let (c, extras) = assemble(vec![
            text("ab", &bold, &rx),
            text("c\u{FFFC}d", &bold, &rx),
            Piece::marker(InlineObject::BookmarkStart { name: "x".into() }),
            text("", &bold, &None),
        ]);
        assert_eq!(c.raw_text(), "abcd\u{FFFC}");
        assert_eq!(c.runs().len(), 2);
        assert_eq!(c.objects().len(), 1);
        assert_eq!(extras.len(), 1);
        assert_eq!((extras[0].start, extras[0].end), (0, 4));
        assert!(c.validate().is_ok());
    }
}
