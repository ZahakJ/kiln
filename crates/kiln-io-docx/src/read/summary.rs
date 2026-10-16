// SPDX-License-Identifier: AGPL-3.0-or-later
//! What `kiln inspect --docx` prints (the heading outline and counts of a read document) and the
//! canonical facts the fidelity scorer compares before and after a round trip.

use std::fmt::Write as _;

use kiln_doc::st::NoteKind;
use kiln_doc::st::NoteType;
use kiln_doc::{Block, Document, DrawingKind, InlineObject, Mark, Paragraph, Placement};

use super::DocxRead;

/// A heading in the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Outline level 0–8 (Heading 1 = 0).
    pub level: u8,
    /// The heading text.
    pub text: String,
}

/// Counts and the heading outline of a read document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// `docx`, `docm`, `dotx` or `dotm`.
    pub kind: String,
    /// Sections.
    pub sections: usize,
    /// Body paragraphs (including those in tables, content controls and field results).
    pub paragraphs: usize,
    /// Body tables (including nested ones).
    pub tables: usize,
    /// Numbered or bulleted body paragraphs.
    pub list_paragraphs: usize,
    /// Header and footer stories.
    pub headers_footers: usize,
    /// Footnotes (separators excluded).
    pub footnotes: usize,
    /// Endnotes (separators excluded).
    pub endnotes: usize,
    /// Comments.
    pub comments: usize,
    /// Tracked changes.
    pub revisions: usize,
    /// Pictures.
    pub pictures: usize,
    /// Other drawings (shapes, text boxes, charts, groups, SmartArt).
    pub drawings: usize,
    /// Inline fields plus multi-paragraph field blocks.
    pub fields: usize,
    /// Bookmarks.
    pub bookmarks: usize,
    /// Equations.
    pub equations: usize,
    /// Styles in the style sheet.
    pub styles: usize,
    /// List definitions (`w:abstractNum`).
    pub lists: usize,
    /// Words in the body (Word's count).
    pub words: usize,
    /// Parts kept verbatim for saving.
    pub passthrough_parts: usize,
    /// Diagnostics.
    pub warnings: usize,
    /// Headings in document order.
    pub outline: Vec<OutlineEntry>,
}

#[derive(Default)]
struct Counts {
    paragraphs: usize,
    tables: usize,
    pictures: usize,
    drawings: usize,
    fields: usize,
    bookmarks: usize,
    equations: usize,
}

fn count_paragraph(p: &Paragraph, c: &mut Counts) {
    c.paragraphs += 1;
    for o in p.content.objects() {
        match o {
            InlineObject::Drawing(d) => {
                if matches!(d.kind, DrawingKind::Picture { .. }) {
                    c.pictures += 1;
                } else {
                    c.drawings += 1;
                }
            }
            InlineObject::Field(_) => c.fields += 1,
            InlineObject::BookmarkStart { .. } => c.bookmarks += 1,
            InlineObject::Equation(_) => c.equations += 1,
            _ => {}
        }
    }
}

fn count_blocks(blocks: &[Block], c: &mut Counts) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => count_paragraph(p, c),
            Block::Table(t) => {
                c.tables += 1;
                for row in &t.rows {
                    for cell in &row.cells {
                        count_blocks(&cell.blocks, c);
                    }
                }
            }
            Block::Sdt(s) => count_blocks(&s.blocks, c),
            Block::Field(f) => {
                c.fields += 1;
                count_blocks(&f.blocks, c);
            }
            Block::Opaque(_) => {}
        }
    }
}

impl Summary {
    /// Summarises a read document.
    #[must_use]
    pub fn of(read: &DocxRead) -> Self {
        let doc = &read.document;
        let mut c = Counts::default();
        for s in &doc.sections {
            count_blocks(&s.blocks, &mut c);
        }
        let notes = |kind: NoteKind| {
            doc.notes
                .iter()
                .filter(|n| n.kind == kind && n.note_type == NoteType::Normal)
                .count()
        };
        Self {
            kind: read.kind.extension().to_owned(),
            sections: doc.sections.len(),
            paragraphs: c.paragraphs,
            tables: c.tables,
            list_paragraphs: doc.list_labels().len(),
            headers_footers: doc.headers_footers.len(),
            footnotes: notes(NoteKind::Footnote),
            endnotes: notes(NoteKind::Endnote),
            comments: doc.comments.len(),
            revisions: doc.revisions().len(),
            pictures: c.pictures,
            drawings: c.drawings,
            fields: c.fields,
            bookmarks: c.bookmarks,
            equations: c.equations,
            styles: doc.styles.len(),
            lists: doc.numbering.abstracts.len(),
            words: doc.statistics(Default::default()).words,
            passthrough_parts: read.passthrough.len(),
            warnings: read.warnings.len(),
            outline: doc
                .outline()
                .into_iter()
                .map(|e| OutlineEntry {
                    level: e.level,
                    text: e.text,
                })
                .collect(),
        }
    }

    /// A human-readable report: the outline, then the counts.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Outline\n");
        if self.outline.is_empty() {
            out.push_str("  (no headings)\n");
        }
        for e in &self.outline {
            let indent = "  ".repeat(usize::from(e.level) + 1);
            let _ = writeln!(out, "{indent}{}", e.text);
        }
        out.push_str("\nCounts\n");
        let rows: [(&str, usize); 19] = [
            ("sections", self.sections),
            ("paragraphs", self.paragraphs),
            ("tables", self.tables),
            ("list paragraphs", self.list_paragraphs),
            ("headers/footers", self.headers_footers),
            ("footnotes", self.footnotes),
            ("endnotes", self.endnotes),
            ("comments", self.comments),
            ("tracked changes", self.revisions),
            ("pictures", self.pictures),
            ("other drawings", self.drawings),
            ("fields", self.fields),
            ("bookmarks", self.bookmarks),
            ("equations", self.equations),
            ("styles", self.styles),
            ("list definitions", self.lists),
            ("words", self.words),
            ("passthrough parts", self.passthrough_parts),
            ("warnings", self.warnings),
        ];
        let _ = writeln!(out, "  {:<18} {}", "format", self.kind);
        for (label, n) in rows {
            let _ = writeln!(out, "  {label:<18} {n}");
        }
        out
    }
}

// ------------------------------------------------------------------ fidelity facts

fn mark_fact(m: &Mark) -> String {
    match m {
        Mark::Link(l) => format!("link={:?}", l.target),
        Mark::Comment(_) => "comment".to_owned(),
        Mark::Insertion(r) => format!("ins={}", r.author),
        Mark::Deletion(r) => format!("del={}", r.author),
        Mark::MoveFrom(r) => format!("moveFrom={}", r.author),
        Mark::MoveTo(r) => format!("moveTo={}", r.author),
        Mark::FormatChange { info, .. } => format!("rPrChange={}", info.author),
        Mark::Sdt(s) => format!("sdt={}", s.kind),
    }
}

fn paragraph_facts(prefix: &str, p: &Paragraph, out: &mut Vec<String>) {
    out.push(format!("{prefix}/p@style={:?}", p.props.style_id));
    if let Some(a) = p.props.alignment {
        out.push(format!("{prefix}/p@jc={a}"));
    }
    if let Some(n) = p.props.numbering {
        out.push(format!("{prefix}/p@num={:?}/{:?}", n.num_id, n.level));
    }
    for (_, props, text) in p.content.run_texts() {
        let text = text.replace(kiln_doc::OBJECT_CHAR, "");
        if text.is_empty() {
            continue;
        }
        let mut flags = String::new();
        for (on, f) in [
            (props.bold, "b"),
            (props.italic, "i"),
            (props.strike, "strike"),
            (props.caps, "caps"),
            (props.hidden, "vanish"),
        ] {
            if on == Some(true) {
                flags.push_str(f);
                flags.push(',');
            }
        }
        if let Some(u) = &props.underline {
            let _ = write!(flags, "u={},", u.style);
        }
        if let Some(sz) = props.size {
            let _ = write!(flags, "sz={sz},");
        }
        if let Some(s) = &props.style_id {
            let _ = write!(flags, "rStyle={s},");
        }
        out.push(format!("{prefix}/r[{text}]@{flags}"));
    }
    for m in p.content.marks() {
        out.push(format!("{prefix}/mark@{}", mark_fact(&m.mark)));
    }
    for o in p.content.objects() {
        let f = match o {
            InlineObject::Break(b) => format!("br={}", b.kind),
            InlineObject::Drawing(d) => format!(
                "drawing={:?}/{}x{}/{}",
                std::mem::discriminant(&d.kind),
                d.width,
                d.height,
                if matches!(d.placement, Placement::Inline) {
                    "inline"
                } else {
                    "anchor"
                }
            ),
            InlineObject::Field(f) => format!("field={}", f.instr.trim()),
            InlineObject::NoteRef(n) => format!("noteRef={}", n.kind),
            InlineObject::Symbol(s) => format!("sym={}:{:X}", s.font, s.code),
            InlineObject::Equation(_) => "equation".to_owned(),
            InlineObject::BookmarkStart { name } => format!("bookmark={name}"),
            InlineObject::BookmarkEnd { .. } => continue,
            InlineObject::Opaque { label, .. } => format!("opaque={label}"),
        };
        out.push(format!("{prefix}/obj@{f}"));
    }
}

fn block_facts(prefix: &str, blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => paragraph_facts(prefix, p, out),
            Block::Table(t) => {
                out.push(format!(
                    "{prefix}/tbl@{}x{}/style={:?}",
                    t.rows.len(),
                    t.grid_width(),
                    t.props.style_id
                ));
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        out.push(format!(
                            "{prefix}/tbl/tc@span={}/vmerge={:?}",
                            cell.grid_span, cell.v_merge
                        ));
                        block_facts(&format!("{prefix}/tbl/tr[{r}]/tc[{c}]"), &cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => {
                out.push(format!("{prefix}/sdt@{}", s.info.kind));
                block_facts(prefix, &s.blocks, out);
            }
            Block::Field(f) => {
                out.push(format!("{prefix}/fieldBlock@{}", f.field.instr));
                block_facts(prefix, &f.blocks, out);
            }
            Block::Opaque(o) => out.push(format!("{prefix}/opaque@{}", o.label)),
        }
    }
}

/// Canonical facts about a document, one string per node or attribute, for the fidelity
/// scorer's structural metric (order does not matter; duplicates count). Node ids are left out
/// so facts compare across reads.
#[must_use]
pub fn snapshot_facts(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for (i, s) in doc.sections.iter().enumerate() {
        out.push(format!(
            "section@{}x{}/{}/cols={}/titlePg={}",
            s.props.page_size.width,
            s.props.page_size.height,
            s.props.break_type,
            s.props.columns.count,
            s.props.title_page
        ));
        out.push(format!(
            "section@headers={}/footers={}",
            s.headers.ids().count(),
            s.footers.ids().count()
        ));
        block_facts(&format!("body{i}"), &s.blocks, &mut out);
    }
    for h in &doc.headers_footers {
        block_facts(&format!("{:?}", h.part), &h.blocks, &mut out);
    }
    for n in doc.notes.iter().filter(|n| n.note_type == NoteType::Normal) {
        block_facts(n.kind.as_str(), &n.blocks, &mut out);
    }
    for c in &doc.comments {
        out.push(format!(
            "comment@{}/{}/reply={}",
            c.meta.author,
            c.meta.resolved,
            c.meta.parent.is_some()
        ));
        block_facts("comment", &c.blocks, &mut out);
    }
    for s in doc.styles.iter() {
        out.push(format!(
            "style@{}/{}/basedOn={:?}",
            s.id, s.kind, s.based_on
        ));
    }
    for a in doc.numbering.abstracts.values() {
        for l in &a.levels {
            out.push(format!("list@{}/{}/{}/{}", a.id, l.level, l.format, l.text));
        }
    }
    out
}
