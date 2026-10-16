// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transaction builders for common edits: typing, deleting across paragraphs, Enter, character
//! and paragraph formatting with Word's toggle rules, tables, comments, section breaks and
//! accepting or rejecting tracked changes.
//!
//! Builders read the document (and allocate ids for new nodes) but do not change content; the
//! caller applies the returned [`Transaction`], usually through [`crate::UndoStack::apply`].

use crate::DocError;
use crate::inline::{Fragment, Mark, MarkSpan, RunSpan};
use crate::model::{Block, Cell, Comment, CommentMeta, Document, Paragraph, Row, Table};
use crate::ops::{EditKind, Op, Transaction};
use crate::pos::{Pos, Range};
use crate::props::{
    ParagraphProps, RunProps, TableCellProps, TableLook, TableProps, TableRowProps, TableWidth,
    ToggleProp,
};
use crate::query::RevisionLocation;
use crate::resolve::{RunInput, RunTarget, resolve_run};
use crate::revision::{InsDel, ParagraphRevision, RevisionKind};
use crate::st::SectionBreak;
use kiln_common::NodeId;

fn para(doc: &Document, id: NodeId) -> Result<&Paragraph, DocError> {
    doc.paragraph(id).ok_or(DocError::UnknownNode(id))
}

fn check_pos(doc: &Document, pos: Pos) -> Result<&Paragraph, DocError> {
    let p = para(doc, pos.para)?;
    if pos.offset > p.content.len() {
        return Err(DocError::OutOfRange {
            offset: pos.offset,
            len: p.content.len(),
        });
    }
    Ok(p)
}

/// The formatting typed text gets at a position: the char before it, else the char after it,
/// else (an empty paragraph) the paragraph mark's.
#[must_use]
pub fn typing_props(p: &Paragraph, offset: usize) -> RunProps {
    if offset > 0
        && let Some(r) = p.content.props_at(offset - 1)
    {
        return r.clone();
    }
    p.content
        .props_at(offset)
        .cloned()
        .unwrap_or_else(|| p.mark_props.clone())
}

/// The marks typed text joins at a position: those that continue on both sides of it (typing
/// inside a hyperlink or a commented range extends it; typing at its edge does not), except
/// tracked deletions.
#[must_use]
pub fn typing_marks(p: &Paragraph, offset: usize) -> Vec<Mark> {
    p.content
        .marks()
        .iter()
        .filter(|m| m.start < offset && offset < m.end)
        .filter(|m| !matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_)))
        .map(|m| m.mark.clone())
        .collect()
}

/// Inserts text at a position. `\n` starts a new paragraph (as Enter does); `\r` is ignored and
/// `U+FFFC` is dropped. The text takes [`typing_props`] and [`typing_marks`]. A single-line
/// insertion is a [`EditKind::Typing`] transaction.
///
/// # Errors
///
/// [`DocError::UnknownNode`] or [`DocError::OutOfRange`] for a bad position.
pub fn insert_text(doc: &mut Document, at: Pos, text: &str) -> Result<Transaction, DocError> {
    let p = check_pos(doc, at)?;
    let props = typing_props(p, at.offset);
    let marks = typing_marks(p, at.offset);
    let lines: Vec<String> = text
        .replace('\r', "")
        .split('\n')
        .map(str::to_owned)
        .collect();
    let mut ops = Vec::new();
    let mut cur = at;
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            let new_para = doc.alloc_id();
            let template = para(doc, at.para)?;
            ops.push(Op::SplitParagraph {
                para: cur.para,
                offset: cur.offset,
                new_para,
                props: Box::new(template.props.clone()),
                mark_props: Box::new(template.mark_props.clone()),
                revision: Box::default(),
            });
            cur = Pos::new(new_para, 0);
        }
        let fragment = marks
            .iter()
            .fold(Fragment::text(line, props.clone()), |f, m| {
                f.with_mark(m.clone())
            });
        let len = fragment.len();
        if len > 0 {
            ops.push(Op::InsertInline {
                para: cur.para,
                offset: cur.offset,
                fragment,
            });
            cur.offset += len;
        }
    }
    let kind = if lines.len() == 1 {
        EditKind::Typing
    } else {
        EditKind::Structure
    };
    Ok(Transaction::new(ops)
        .with_kind(kind)
        .with_selection(Some(Range::caret(at)), Some(Range::caret(cur))))
}

/// Deletes from `from` to `to` (in either order). Within one paragraph this deletes text;
/// across paragraphs of one container it deletes the tail of the first, the blocks between and
/// the head of the last, and joins the two paragraphs (the first keeps its formatting, as in
/// Word).
///
/// # Errors
///
/// Unknown paragraphs, bad offsets, or a range whose ends are in different containers (across a
/// table boundary), which Word handles by deleting cell contents instead.
pub fn delete_range(doc: &Document, from: Pos, to: Pos) -> Result<Transaction, DocError> {
    check_pos(doc, from)?;
    check_pos(doc, to)?;
    let (a, b) = Range::new(from, to).ordered(doc);
    let mut ops = Vec::new();
    if a.para == b.para {
        if a.offset < b.offset {
            ops.push(Op::DeleteInline {
                para: a.para,
                offset: a.offset,
                len: b.offset - a.offset,
            });
        }
    } else {
        let (ca, ia) = doc.parent_of(a.para).ok_or(DocError::UnknownNode(a.para))?;
        let (cb, ib) = doc.parent_of(b.para).ok_or(DocError::UnknownNode(b.para))?;
        if ca != cb || ib < ia {
            return Err(DocError::InvalidOp(
                "the range crosses a container boundary".into(),
            ));
        }
        let first_len = para(doc, a.para)?.content.len();
        if first_len > a.offset {
            ops.push(Op::DeleteInline {
                para: a.para,
                offset: a.offset,
                len: first_len - a.offset,
            });
        }
        if b.offset > 0 {
            ops.push(Op::DeleteInline {
                para: b.para,
                offset: 0,
                len: b.offset,
            });
        }
        if ib > ia + 1 {
            ops.push(Op::RemoveBlocks {
                container: ca,
                index: ia + 1,
                count: ib - ia - 1,
            });
        }
        ops.push(Op::MergeParagraphs {
            first: a.para,
            second: b.para,
        });
    }
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Delete)
        .with_selection(Some(Range::new(from, to)), Some(Range::caret(a))))
}

/// Enter: splits the paragraph. At the end of a paragraph whose style names a "next" style, the
/// new paragraph takes that style (Heading 1 → Normal), as in Word.
///
/// # Errors
///
/// [`DocError::UnknownNode`] or [`DocError::OutOfRange`] for a bad position.
pub fn split_paragraph(doc: &mut Document, at: Pos) -> Result<Transaction, DocError> {
    let p = check_pos(doc, at)?;
    let mut props = p.props.clone();
    if at.offset == p.content.len() {
        let current = doc.resolve_paragraph(p).style_id;
        let next = doc.styles.next_style_id(&current);
        if next != current {
            props.style_id = Some(next);
            props.numbering = None;
        }
    }
    let mark_props = p.mark_props.clone();
    let new_para = doc.alloc_id();
    Ok(Transaction::new(vec![Op::SplitParagraph {
        para: at.para,
        offset: at.offset,
        new_para,
        props: Box::new(props),
        mark_props: Box::new(mark_props),
        revision: Box::new(ParagraphRevision::default()),
    }])
    .with_kind(EditKind::Structure)
    .with_selection(
        Some(Range::caret(at)),
        Some(Range::caret(Pos::new(new_para, 0))),
    ))
}

/// The paragraph segments a range covers, in document order: (paragraph, start, end).
///
/// # Errors
///
/// Unknown paragraphs or positions in different stories.
pub fn segments(
    doc: &Document,
    from: Pos,
    to: Pos,
) -> Result<Vec<(NodeId, usize, usize)>, DocError> {
    check_pos(doc, from)?;
    check_pos(doc, to)?;
    let (a, b) = Range::new(from, to).ordered(doc);
    if a.para == b.para {
        return Ok(vec![(a.para, a.offset, b.offset)]);
    }
    let story = doc.story_of(a.para).ok_or(DocError::UnknownNode(a.para))?;
    if doc.story_of(b.para) != Some(story) {
        return Err(DocError::InvalidOp("the range spans two stories".into()));
    }
    let mut out = Vec::new();
    let mut inside = false;
    for p in doc.paragraphs_in(story) {
        if p.id == a.para {
            inside = true;
            out.push((p.id, a.offset, p.content.len()));
        } else if p.id == b.para {
            out.push((p.id, 0, b.offset));
            break;
        } else if inside {
            out.push((p.id, 0, p.content.len()));
        }
    }
    Ok(out)
}

/// Changes the direct formatting of every run in a range with `f`.
///
/// # Errors
///
/// As [`segments`].
pub fn format_runs(
    doc: &Document,
    from: Pos,
    to: Pos,
    f: impl Fn(&mut RunProps),
) -> Result<Transaction, DocError> {
    let mut ops = Vec::new();
    for (id, start, end) in segments(doc, from, to)? {
        if start >= end {
            continue;
        }
        let p = para(doc, id)?;
        let mut runs = Vec::new();
        let mut pos = 0;
        for r in p.content.runs() {
            let (s, e) = (pos.max(start), (pos + r.len).min(end));
            if s < e {
                let mut props = r.props.clone();
                f(&mut props);
                runs.push(RunSpan::new(e - s, props));
            }
            pos += r.len;
        }
        ops.push(Op::SetRuns {
            para: id,
            offset: start,
            runs,
        });
    }
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Format)
        .with_selection(Some(Range::new(from, to)), Some(Range::new(from, to))))
}

/// Ctrl+B, Ctrl+I, …: when every selected char already shows the property, turns it off
/// (removing direct formatting, and writing an explicit "off" where a style turns it on, like
/// `<w:b w:val="0"/>`); otherwise turns it on everywhere. A collapsed range changes nothing here
/// (the command layer keeps pending formatting for the caret).
///
/// # Errors
///
/// As [`segments`].
pub fn toggle(
    doc: &Document,
    from: Pos,
    to: Pos,
    prop: ToggleProp,
) -> Result<Transaction, DocError> {
    let ctx = doc.context();
    let segs = segments(doc, from, to)?;
    let resolved_on = |p: &Paragraph, direct: &RunProps| {
        let table = doc.table_context(p.id);
        let r = resolve_run(
            &ctx,
            &RunInput {
                paragraph: &p.props,
                mark_props: &p.mark_props,
                direct,
                table: table.as_ref(),
                target: RunTarget::Text,
            },
        );
        match prop {
            ToggleProp::Bold => r.bold,
            ToggleProp::BoldCs => r.bold_cs,
            ToggleProp::Italic => r.italic,
            ToggleProp::ItalicCs => r.italic_cs,
            ToggleProp::Caps => r.caps,
            ToggleProp::SmallCaps => r.small_caps,
            ToggleProp::Strike => r.strike,
            ToggleProp::Dstrike => r.dstrike,
            ToggleProp::Hidden => r.hidden,
            ToggleProp::Emboss => r.emboss,
            ToggleProp::Imprint => r.imprint,
            ToggleProp::Outline => r.outline,
            ToggleProp::Shadow => r.shadow,
        }
    };
    let mut all_on = true;
    let mut any = false;
    for (id, start, end) in &segs {
        let p = para(doc, *id)?;
        let mut pos = 0;
        for r in p.content.runs() {
            if pos < *end && pos + r.len > *start {
                any = true;
                all_on &= resolved_on(p, &r.props);
            }
            pos += r.len;
        }
    }
    let turn_on = !(any && all_on);
    let mut ops = Vec::new();
    for (id, start, end) in segs {
        if start >= end {
            continue;
        }
        let p = para(doc, id)?;
        let mut runs = Vec::new();
        let mut pos = 0;
        for r in p.content.runs() {
            let (s, e) = (pos.max(start), (pos + r.len).min(end));
            if s < e {
                let mut props = r.props.clone();
                if turn_on {
                    props.set_toggle(prop, Some(true));
                } else {
                    props.set_toggle(prop, None);
                    if resolved_on(p, &props) {
                        props.set_toggle(prop, Some(false));
                    }
                }
                runs.push(RunSpan::new(e - s, props));
            }
            pos += r.len;
        }
        ops.push(Op::SetRuns {
            para: id,
            offset: start,
            runs,
        });
    }
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Format)
        .with_selection(Some(Range::new(from, to)), Some(Range::new(from, to))))
}

/// Changes the direct properties of every paragraph in a range with `f`.
///
/// # Errors
///
/// As [`segments`].
pub fn format_paragraphs(
    doc: &Document,
    from: Pos,
    to: Pos,
    f: impl Fn(&mut ParagraphProps),
) -> Result<Transaction, DocError> {
    let mut ops = Vec::new();
    for (id, _, _) in segments(doc, from, to)? {
        let mut props = para(doc, id)?.props.clone();
        f(&mut props);
        ops.push(Op::SetParagraphProps {
            para: id,
            props: Box::new(props),
        });
    }
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Format)
        .with_selection(Some(Range::new(from, to)), Some(Range::new(from, to))))
}

fn empty_cell(doc: &mut Document, width: i32) -> Cell {
    let p = Paragraph::new(doc.alloc_id());
    Cell {
        id: doc.alloc_id(),
        props: TableCellProps {
            width: Some(TableWidth::dxa(width)),
            ..TableCellProps::default()
        },
        grid_span: 1,
        v_merge: None,
        blocks: vec![Block::Paragraph(p)],
    }
}

fn empty_row(doc: &mut Document, widths: &[i32]) -> Row {
    let cells = widths.iter().map(|w| empty_cell(doc, *w)).collect();
    Row {
        id: doc.alloc_id(),
        props: TableRowProps::default(),
        revision: None,
        cells,
    }
}

/// Inserts a Table Grid table of `rows` × `cols` (at least 1 × 1, at most 32,767 × 63 as in
/// Word) sized to the text column. At the start of a paragraph the table goes before it;
/// elsewhere the paragraph is split and the table goes between.
///
/// # Errors
///
/// A bad position or a position outside any container.
pub fn insert_table(
    doc: &mut Document,
    at: Pos,
    rows: usize,
    cols: usize,
) -> Result<Transaction, DocError> {
    check_pos(doc, at)?;
    let rows = rows.clamp(1, 32_767);
    let cols = cols.clamp(1, 63);
    let width = doc
        .section_of(at.para)
        .map_or(9360, |s| s.props.content_width());
    let col_w = width / i32::try_from(cols).unwrap_or(1);
    let widths = vec![col_w; cols];
    let (container, index) = doc
        .parent_of(at.para)
        .ok_or(DocError::UnknownNode(at.para))?;
    let mut ops = Vec::new();
    let insert_at = if at.offset == 0 {
        index
    } else {
        let new_para = doc.alloc_id();
        ops.push(Op::split_paragraph(doc, at.para, at.offset, new_para)?);
        index + 1
    };
    let table_rows = (0..rows)
        .map(|_| empty_row(doc, &widths))
        .collect::<Vec<_>>();
    let first_cell_para = table_rows[0].cells[0].blocks[0].id();
    let table = Table {
        id: doc.alloc_id(),
        props: TableProps {
            style_id: doc
                .styles
                .contains("TableGrid")
                .then(|| "TableGrid".to_owned()),
            width: Some(TableWidth {
                kind: crate::st::WidthType::Auto,
                value: 0,
            }),
            look: Some(TableLook::default()),
            ..TableProps::default()
        },
        grid: widths,
        rows: table_rows,
    };
    ops.push(Op::InsertBlocks {
        container,
        index: insert_at,
        blocks: vec![Block::Table(table)],
    });
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Structure)
        .with_selection(
            Some(Range::caret(at)),
            Some(Range::caret(Pos::new(first_cell_para, 0))),
        ))
}

/// Inserts an empty row above or below a row, copying its cell widths and spans.
///
/// # Errors
///
/// [`DocError::UnknownNode`] for an unknown row.
pub fn insert_row(doc: &mut Document, row: NodeId, below: bool) -> Result<Transaction, DocError> {
    let (table, r) = doc.row(row).ok_or(DocError::UnknownNode(row))?;
    let table_id = table.id;
    let index = table.rows.iter().position(|x| x.id == row).unwrap_or(0) + usize::from(below);
    let template: Vec<(TableCellProps, u32)> = r
        .cells
        .iter()
        .map(|c| (c.props.clone(), c.grid_span))
        .collect();
    let row_props = TableRowProps {
        header: None,
        ..r.props.clone()
    };
    let cells = template
        .into_iter()
        .map(|(props, grid_span)| Cell {
            id: doc.alloc_id(),
            props,
            grid_span,
            v_merge: None,
            blocks: vec![Block::Paragraph(Paragraph::new(doc.alloc_id()))],
        })
        .collect::<Vec<_>>();
    let new_row = Row {
        id: doc.alloc_id(),
        props: row_props,
        revision: None,
        cells,
    };
    Ok(Transaction::new(vec![Op::InsertRows {
        table: table_id,
        index,
        rows: vec![new_row],
    }])
    .with_kind(EditKind::Structure))
}

/// Deletes a row (a table's last row cannot be deleted this way; delete the table).
///
/// # Errors
///
/// [`DocError::UnknownNode`] for an unknown row.
pub fn delete_row(doc: &Document, row: NodeId) -> Result<Transaction, DocError> {
    let (table, _) = doc.row(row).ok_or(DocError::UnknownNode(row))?;
    let index = table.rows.iter().position(|x| x.id == row).unwrap_or(0);
    Ok(Transaction::new(vec![Op::RemoveRows {
        table: table.id,
        index,
        count: 1,
    }])
    .with_kind(EditKind::Structure))
}

/// Inserts a grid column left or right of grid column `col` in every row: a new single-span
/// cell where the boundary falls between cells, and a wider span where a merged cell covers it.
///
/// # Errors
///
/// [`DocError::UnknownNode`] for an unknown table, [`DocError::OutOfRange`] for a bad column.
pub fn insert_column(
    doc: &mut Document,
    table: NodeId,
    col: usize,
    right: bool,
) -> Result<Transaction, DocError> {
    let t = doc
        .table(table)
        .cloned()
        .ok_or(DocError::UnknownNode(table))?;
    let width = t.grid_width();
    if col >= width {
        return Err(DocError::OutOfRange {
            offset: col,
            len: width,
        });
    }
    let boundary = col + usize::from(right);
    let new_w = t.grid.get(col).copied().unwrap_or(1440);
    let mut ops = Vec::new();
    let map = t.cell_map();
    for (r, row) in t.rows.iter().enumerate() {
        let slots: Vec<_> = map.iter().filter(|s| s.row == r).collect();
        // A cell that starts at the boundary: insert before it. A cell that spans across it:
        // widen it. Otherwise (boundary at the row end): append.
        if let Some(s) = slots.iter().find(|s| s.col == boundary) {
            let cell = empty_cell(doc, new_w);
            ops.push(Op::InsertCells {
                row: row.id,
                index: s.cell,
                cells: vec![cell],
            });
        } else if let Some(s) = slots
            .iter()
            .find(|s| s.col < boundary && boundary < s.col + s.col_span)
        {
            let c = &row.cells[s.cell];
            ops.push(Op::SetCell {
                cell: c.id,
                props: Box::new(c.props.clone()),
                grid_span: c.grid_span + 1,
                v_merge: c.v_merge,
            });
        } else {
            let cell = empty_cell(doc, new_w);
            ops.push(Op::InsertCells {
                row: row.id,
                index: row.cells.len(),
                cells: vec![cell],
            });
        }
    }
    let mut grid = t.grid.clone();
    grid.insert(boundary.min(grid.len()), new_w);
    ops.push(Op::SetTableGrid { table, grid });
    Ok(Transaction::new(ops).with_kind(EditKind::Structure))
}

/// Deletes grid column `col`: single-span cells there are removed, wider cells shrink by one.
///
/// # Errors
///
/// [`DocError::UnknownNode`] for an unknown table, [`DocError::OutOfRange`] for a bad column,
/// [`DocError::InvalidOp`] when a row would lose its last cell (delete the table instead).
pub fn delete_column(doc: &Document, table: NodeId, col: usize) -> Result<Transaction, DocError> {
    let t = doc.table(table).ok_or(DocError::UnknownNode(table))?;
    let width = t.grid_width();
    if col >= width {
        return Err(DocError::OutOfRange {
            offset: col,
            len: width,
        });
    }
    let map = t.cell_map();
    let mut ops = Vec::new();
    for (r, row) in t.rows.iter().enumerate() {
        let Some(s) = map
            .iter()
            .find(|s| s.row == r && s.col <= col && col < s.col + s.col_span)
        else {
            continue;
        };
        let c = &row.cells[s.cell];
        if c.grid_span > 1 {
            ops.push(Op::SetCell {
                cell: c.id,
                props: Box::new(c.props.clone()),
                grid_span: c.grid_span - 1,
                v_merge: c.v_merge,
            });
        } else {
            ops.push(Op::RemoveCells {
                row: row.id,
                index: s.cell,
                count: 1,
            });
        }
    }
    let mut grid = t.grid.clone();
    if col < grid.len() {
        grid.remove(col);
    }
    ops.push(Op::SetTableGrid { table, grid });
    Ok(Transaction::new(ops).with_kind(EditKind::Structure))
}

/// Adds a comment on a range (or a reply when `meta.parent` is set and the range is empty).
///
/// # Errors
///
/// As [`segments`].
pub fn add_comment(
    doc: &mut Document,
    from: Pos,
    to: Pos,
    meta: CommentMeta,
    text: &str,
) -> Result<(Transaction, NodeId), DocError> {
    let segs = segments(doc, from, to)?;
    let id = doc.alloc_id();
    let mut body = Paragraph::new(doc.alloc_id());
    body.props.style_id = doc
        .styles
        .contains("CommentText")
        .then(|| "CommentText".to_owned());
    body.content =
        crate::inline::Inlines::from_fragment(Fragment::text(text, RunProps::default()))?;
    let mut ops = vec![Op::InsertComment {
        index: doc.comments.len(),
        comment: Box::new(Comment {
            id,
            meta,
            blocks: vec![Block::Paragraph(body)],
        }),
    }];
    for (p, start, end) in segs {
        if start < end {
            ops.push(Op::AddMark {
                para: p,
                start,
                end,
                mark: Mark::Comment(id),
            });
        }
    }
    Ok((Transaction::new(ops).with_kind(EditKind::Other), id))
}

/// Deletes a comment, its replies and its marks.
///
/// # Errors
///
/// [`DocError::UnknownNode`] for an unknown comment.
pub fn delete_comment(doc: &Document, id: NodeId) -> Result<Transaction, DocError> {
    if !doc.comments.iter().any(|c| c.id == id) {
        return Err(DocError::UnknownNode(id));
    }
    // The thread, deepest replies first.
    let mut thread = vec![id];
    let mut i = 0;
    while i < thread.len() {
        let parent = thread[i];
        thread.extend(
            doc.comments
                .iter()
                .filter(|c| c.meta.parent == Some(parent))
                .map(|c| c.id),
        );
        i += 1;
    }
    let mut ops = Vec::new();
    for (_, p) in doc.all_paragraphs() {
        for m in p.content.marks() {
            if let Mark::Comment(c) = m.mark
                && thread.contains(&c)
            {
                ops.push(Op::RemoveMark {
                    para: p.id,
                    start: m.start,
                    end: m.end,
                    mark: m.mark.clone(),
                });
            }
        }
    }
    for c in thread.iter().rev() {
        ops.push(Op::RemoveComment { id: *c });
    }
    Ok(Transaction::new(ops))
}

/// Inserts a section break of `kind` at a position: the paragraph is split there and the text
/// after the break starts a new section with the current page setup, starting as `kind` says
/// (`w:type` belongs to the section that starts at the break). Headers and footers link to the
/// previous section.
///
/// # Errors
///
/// A position outside the body, or not directly in a section (inside a table: Word puts the
/// break after the table; callers move the position first).
pub fn insert_section_break(
    doc: &mut Document,
    at: Pos,
    kind: SectionBreak,
) -> Result<Transaction, DocError> {
    check_pos(doc, at)?;
    let (container, index) = doc
        .parent_of(at.para)
        .ok_or(DocError::UnknownNode(at.para))?;
    let (_, section) = doc.section(container).ok_or(DocError::InvalidOp(
        "section breaks go between body paragraphs".into(),
    ))?;
    let section_id = section.id;
    let mut props = section.props.clone();
    props.break_type = kind;
    let new_para = doc.alloc_id();
    let new_section = doc.alloc_id();
    let ops = vec![
        Op::split_paragraph(doc, at.para, at.offset, new_para)?,
        Op::SplitSection {
            section: section_id,
            block_index: index + 1,
            new_section,
            props: Box::new(props),
            headers: crate::section::HeaderFooterRefs::default(),
            footers: crate::section::HeaderFooterRefs::default(),
        },
    ];
    Ok(Transaction::new(ops)
        .with_kind(EditKind::Structure)
        .with_selection(
            Some(Range::caret(at)),
            Some(Range::caret(Pos::new(new_para, 0))),
        ))
}

/// Accepts (`accept`) or rejects a tracked change listed by [`Document::revisions`].
///
/// Accepting an insertion keeps the text and drops the mark; rejecting it deletes the text.
/// Accepting a deletion deletes the text; rejecting it keeps the text. Format changes keep the
/// new formatting when accepted and restore the old one when rejected. Paragraph mark
/// insertions/deletions and row changes are resolved by the command layer (WP-66), which joins
/// paragraphs and removes rows.
///
/// # Errors
///
/// [`DocError::InvalidOp`] for a change this builder does not handle, or an unknown paragraph.
pub fn resolve_revision(
    doc: &Document,
    kind: RevisionKind,
    location: RevisionLocation,
    accept: bool,
) -> Result<Transaction, DocError> {
    let mut ops = Vec::new();
    match (kind, location) {
        (
            RevisionKind::Insertion
            | RevisionKind::Deletion
            | RevisionKind::MoveFrom
            | RevisionKind::MoveTo,
            RevisionLocation::Text {
                para: id,
                start,
                end,
            },
        ) => {
            let p = para(doc, id)?;
            let mark = p
                .content
                .marks()
                .iter()
                .find(|m| {
                    m.start == start
                        && m.end == end
                        && matches!(
                            (&m.mark, kind),
                            (Mark::Insertion(_), RevisionKind::Insertion)
                                | (Mark::Deletion(_), RevisionKind::Deletion)
                                | (Mark::MoveFrom(_), RevisionKind::MoveFrom)
                                | (Mark::MoveTo(_), RevisionKind::MoveTo)
                        )
                })
                .ok_or(DocError::InvalidOp("no such revision".into()))?;
            let removes_text = matches!(
                (kind, accept),
                (RevisionKind::Insertion | RevisionKind::MoveTo, false)
                    | (RevisionKind::Deletion | RevisionKind::MoveFrom, true)
            );
            if removes_text {
                ops.push(Op::DeleteInline {
                    para: id,
                    offset: start,
                    len: end - start,
                });
            } else {
                ops.push(Op::RemoveMark {
                    para: id,
                    start,
                    end,
                    mark: mark.mark.clone(),
                });
            }
        }
        (
            RevisionKind::RunFormat,
            RevisionLocation::Text {
                para: id,
                start,
                end,
            },
        ) => {
            let p = para(doc, id)?;
            let found: Vec<&MarkSpan> = p
                .content
                .marks()
                .iter()
                .filter(|m| {
                    m.start == start && m.end == end && matches!(m.mark, Mark::FormatChange { .. })
                })
                .collect();
            let m = found
                .first()
                .ok_or(DocError::InvalidOp("no such revision".into()))?;
            if !accept && let Mark::FormatChange { before, .. } = &m.mark {
                ops.push(Op::SetRuns {
                    para: id,
                    offset: start,
                    runs: vec![RunSpan::new(end - start, (**before).clone())],
                });
            }
            ops.push(Op::RemoveMark {
                para: id,
                start,
                end,
                mark: m.mark.clone(),
            });
        }
        (RevisionKind::ParagraphFormat, RevisionLocation::Paragraph(id)) => {
            let p = para(doc, id)?;
            let change = p
                .revision
                .props_change
                .clone()
                .ok_or(DocError::InvalidOp("no paragraph change".into()))?;
            if !accept {
                ops.push(Op::SetParagraphProps {
                    para: id,
                    props: Box::new(change.before),
                });
            }
            ops.push(Op::SetParagraphRevision {
                para: id,
                revision: Box::new(ParagraphRevision {
                    props_change: None,
                    ..p.revision.clone()
                }),
            });
        }
        (RevisionKind::ParagraphMarkFormat, RevisionLocation::Paragraph(id)) => {
            let p = para(doc, id)?;
            let change = p
                .revision
                .mark_props_change
                .clone()
                .ok_or(DocError::InvalidOp("no paragraph mark change".into()))?;
            if !accept {
                ops.push(Op::SetParagraphMarkProps {
                    para: id,
                    props: Box::new(change.before),
                });
            }
            ops.push(Op::SetParagraphRevision {
                para: id,
                revision: Box::new(ParagraphRevision {
                    mark_props_change: None,
                    ..p.revision.clone()
                }),
            });
        }
        (
            RevisionKind::ParagraphInsertion | RevisionKind::ParagraphDeletion,
            RevisionLocation::Paragraph(id),
        ) => {
            let p = para(doc, id)?;
            let joins = match p.revision.mark.as_ref().map(|r| r.kind) {
                Some(InsDel::Insertion) => !accept,
                Some(InsDel::Deletion) => accept,
                None => return Err(DocError::InvalidOp("no paragraph mark revision".into())),
            };
            let (container, index) = doc.parent_of(id).ok_or(DocError::UnknownNode(id))?;
            let next = doc
                .container(container)?
                .get(index + 1)
                .and_then(Block::as_paragraph)
                .map(|n| n.id);
            ops.push(Op::SetParagraphRevision {
                para: id,
                revision: Box::new(ParagraphRevision {
                    mark: None,
                    ..p.revision.clone()
                }),
            });
            if joins && let Some(next) = next {
                ops.push(Op::MergeParagraphs {
                    first: id,
                    second: next,
                });
            }
        }
        _ => return Err(DocError::InvalidOp("unsupported revision".into())),
    }
    Ok(Transaction::new(ops).with_kind(EditKind::Other))
}

#[cfg(test)]
mod tests;
