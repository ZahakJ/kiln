// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tables (`w:tbl`, ECMA-376 Part 1 §17.4). The model keeps Word's cell model (`w:gridSpan`,
//! `w:vMerge`, `w:gridBefore`/`w:gridAfter`), so the reader mostly copies it, normalising what
//! Word tolerates but the model does not:
//! - legacy `w:hMerge` runs become one cell with a wider `gridSpan`;
//! - a `w:vMerge` continuation with no merge to continue (nothing above it in that grid column
//!   with the same span) becomes an ordinary cell, as Word shows it;
//! - ragged rows (fewer grid columns than the table) get a `w:gridAfter`, which is how Word lays
//!   them out;
//! - row- and cell-level content controls and custom XML are unwrapped (their properties go to
//!   the extras), and bookmarks or comment ranges between rows and cells go to the next
//!   paragraph.

use std::collections::HashMap;

use kiln_doc::st::VMerge;
use kiln_doc::{Block, Cell, OpaqueBlock, Paragraph, Row, SdtInfo, Table};
use kiln_ooxml::xml::XmlElement;

use super::context::MAX_DEPTH;
use super::props::{parse_cell_props, parse_row_props, parse_table_props};
use super::sdt::parse_sdt_info;
use super::story::{BlockSink, SUPPORTED_MC, StoryReader};
use crate::common::extras::{CellExtras, RowExtras};
use crate::common::units::parse_twips;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{XmlPath, attr, stepped};

struct RowSource<'a> {
    tr: &'a XmlElement,
    sdt: Vec<SdtInfo>,
    path: String,
}

struct CellSource<'a> {
    tc: &'a XmlElement,
    sdt: Vec<SdtInfo>,
    path: String,
}

fn collect_rows<'a>(
    story: &mut StoryReader<'_, '_>,
    parent: &'a XmlElement,
    sdt: &[SdtInfo],
    out: &mut Vec<RowSource<'a>>,
) {
    for (el, step) in stepped(parent) {
        story.path.push(step);
        match el.name.as_str() {
            "w:tr" => out.push(RowSource {
                tr: el,
                sdt: sdt.to_vec(),
                path: story.path.to_string(),
            }),
            "w:sdt" => {
                let info = parse_sdt_info(el, story.raw());
                if let Some(content) = el.child("w:sdtContent")
                    && story.enter()
                {
                    story.path.push("w:sdtContent[1]".to_owned());
                    let mut chain = sdt.to_vec();
                    chain.push(info);
                    collect_rows(story, content, &chain, out);
                    story.path.pop();
                    story.leave();
                }
            }
            "w:customXml" => {
                if story.enter() {
                    collect_rows(story, el, sdt, out);
                    story.leave();
                }
            }
            "mc:AlternateContent" => {
                for c in el.select_alternate_content(&SUPPORTED_MC) {
                    if c.name == "w:tr" {
                        out.push(RowSource {
                            tr: c,
                            sdt: sdt.to_vec(),
                            path: story.path.to_string(),
                        });
                    }
                }
            }
            "w:bookmarkStart" | "w:bookmarkEnd" | "w:commentRangeStart" | "w:commentRangeEnd" => {
                story.read_marker(el);
            }
            _ => {}
        }
        story.path.pop();
    }
}

fn collect_cells<'a>(
    story: &mut StoryReader<'_, '_>,
    parent: &'a XmlElement,
    sdt: &[SdtInfo],
    base: &str,
    out: &mut Vec<CellSource<'a>>,
) {
    for (el, step) in stepped(parent) {
        let path = format!("{base}/{step}");
        match el.name.as_str() {
            "w:tc" => out.push(CellSource {
                tc: el,
                sdt: sdt.to_vec(),
                path,
            }),
            "w:sdt" => {
                let info = parse_sdt_info(el, story.raw());
                if let Some(content) = el.child("w:sdtContent")
                    && story.enter()
                {
                    let mut chain = sdt.to_vec();
                    chain.push(info);
                    collect_cells(
                        story,
                        content,
                        &chain,
                        &format!("{path}/w:sdtContent[1]"),
                        out,
                    );
                    story.leave();
                }
            }
            "w:customXml" => {
                if story.enter() {
                    collect_cells(story, el, sdt, &path, out);
                    story.leave();
                }
            }
            "w:bookmarkStart" | "w:bookmarkEnd" | "w:commentRangeStart" | "w:commentRangeEnd" => {
                story.read_marker(el);
            }
            _ => {}
        }
    }
}

fn empty_paragraph(story: &mut StoryReader<'_, '_>) -> Block {
    Block::Paragraph(Paragraph::new(story.ctx.doc.alloc_id()))
}

fn has_content(b: &Block) -> bool {
    match b {
        Block::Paragraph(p) => !p.content.is_empty(),
        _ => true,
    }
}

/// Reads a `w:tbl` into a table block.
pub(crate) fn read_table(story: &mut StoryReader<'_, '_>, tbl: &XmlElement) -> Block {
    // A table is only followed when everything in it — rows, cells, paragraphs and any inline
    // wrappers (content controls, hyperlinks, smart tags) — fits under the depth limit too;
    // otherwise the limit would cut the table open and drop its inner content. Nested tables are
    // measured on their own when they are reached. Too deep to follow: keep the whole table
    // verbatim so a save loses nothing.
    if story.ctx.depth.saturating_add(levels_within(tbl)) >= MAX_DEPTH {
        story.warn(
            WarningCode::NestingTooDeep,
            Severity::Info,
            "A table nested this deeply is kept unchanged in the file but not shown.",
        );
        return opaque_table(story, tbl);
    }
    if !story.enter() {
        return opaque_table(story, tbl);
    }
    let block = read_table_inner(story, tbl);
    story.leave();
    block
}

/// An upper bound on the container levels the reader enters below (and including) `tbl`, not
/// counting what nested tables need: the element depth of its subtree with every nested
/// `w:tbl` counted as one level. Every container level the reader enters is an element it
/// descends into, so the element depth bounds them. Iterative, so hostile nesting cannot exhaust
/// the stack; every element is visited by its nearest enclosing table only.
fn levels_within(tbl: &XmlElement) -> usize {
    let mut deepest = 1;
    let mut stack: Vec<(&XmlElement, usize)> = vec![(tbl, 1)];
    while let Some((el, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        for child in el.elements() {
            if child.name == "w:tbl" {
                deepest = deepest.max(depth.saturating_add(1));
            } else {
                stack.push((child, depth.saturating_add(1)));
            }
        }
    }
    // Verbatim over-deep markup (`XmlNode::Raw`) is kept as it is and never entered.
    deepest
}

fn opaque_table(story: &mut StoryReader<'_, '_>, tbl: &XmlElement) -> Block {
    Block::Opaque(OpaqueBlock {
        id: story.ctx.doc.alloc_id(),
        xml: story.raw_xml(tbl),
        label: "Nested table".to_owned(),
    })
}

fn read_table_inner(story: &mut StoryReader<'_, '_>, tbl: &XmlElement) -> Block {
    let (props, table_xml) = parse_table_props(tbl.child("w:tblPr"), story.raw());
    let grid: Vec<i32> = tbl
        .child("w:tblGrid")
        .map(|g| {
            g.children_named("w:gridCol")
                .map(|c| parse_twips(attr(c, "w:w")).unwrap_or(0).max(0))
                .collect()
        })
        .unwrap_or_default();

    let mut sources = Vec::new();
    collect_rows(story, tbl, &[], &mut sources);

    let mut rows: Vec<Row> = Vec::new();
    // Open vertical merges: first grid column → span of the cell that restarts or continues it.
    let mut open: HashMap<usize, u32> = HashMap::new();
    let mut widths: Vec<usize> = Vec::new();

    for src in sources {
        let parsed = parse_row_props(src.tr.child("w:trPr"), story.raw());
        let row_id = story.ctx.doc.alloc_id();
        let row_extras = RowExtras {
            xml: parsed.xml,
            tbl_pr_ex: src.tr.child("w:tblPrEx").map(|e| story.raw_xml(e)),
            sdt: src.sdt,
        };
        if row_extras != RowExtras::default() {
            story.ctx.extras.rows.insert(row_id, row_extras);
        }
        // Grid counts are clamped by the props reader; the sums below still saturate so that no
        // input can overflow `usize` (32 bits on wasm).
        let grid_before = usize::try_from(parsed.props.grid_before.unwrap_or(0)).unwrap_or(0);
        let mut cells: Vec<Cell> = Vec::new();
        let mut col = grid_before;
        let mut touched: Vec<usize> = Vec::new();

        let mut cell_sources = Vec::new();
        collect_cells(story, src.tr, &[], &src.path, &mut cell_sources);
        for cs in cell_sources {
            let cp = parse_cell_props(cs.tc.child("w:tcPr"), story.raw());
            let saved = std::mem::replace(&mut story.path, XmlPath::parse(&cs.path));
            let mut sink = BlockSink::default();
            story.read_blocks(cs.tc, &mut sink);
            if cp.revision {
                story.warn(
                    WarningCode::RevisionCell,
                    Severity::Info,
                    "A tracked cell insertion, deletion or merge is kept but not shown.",
                );
            }
            story.path = saved;
            let mut blocks = sink.into_blocks(story.ctx);
            let span = cp.grid_span;
            let span_cols = usize::try_from(span).unwrap_or(usize::MAX);

            if cp.h_merge == Some(VMerge::Continue)
                && let Some(prev) = cells.last_mut()
            {
                prev.grid_span = prev.grid_span.saturating_add(span);
                prev.blocks.extend(blocks.into_iter().filter(has_content));
                col = col.saturating_add(span_cols);
                continue;
            }

            let mut v_merge = cp.v_merge;
            if v_merge == Some(VMerge::Continue) && open.get(&col) != Some(&span) {
                // Nothing to continue: Word shows an ordinary cell.
                v_merge = None;
            }
            for k in 0..span_cols {
                touched.push(col.saturating_add(k));
                if k > 0 {
                    open.remove(&col.saturating_add(k));
                }
            }
            match v_merge {
                Some(_) => {
                    open.insert(col, span);
                }
                None => {
                    open.remove(&col);
                }
            }
            if !matches!(blocks.last(), Some(Block::Paragraph(_))) {
                // Word ends every cell with a paragraph (the end-of-cell mark), also when the
                // file's cell is empty or ends with a nested table. After a table the file's
                // form is remembered so that an unedited cell is written back without it.
                let id = story.ctx.doc.alloc_id();
                if !blocks.is_empty() {
                    story.ctx.extras.cell_end_paragraphs.insert(id);
                }
                blocks.push(Block::Paragraph(Paragraph::new(id)));
            }
            let id = story.ctx.doc.alloc_id();
            let extras = CellExtras {
                xml: cp.xml,
                sdt: cs.sdt,
            };
            if extras != CellExtras::default() {
                story.ctx.extras.cells.insert(id, extras);
            }
            cells.push(Cell {
                id,
                props: cp.props,
                grid_span: span,
                v_merge,
                blocks,
            });
            col = col.saturating_add(span_cols);
        }
        // A grid column without a cell in this row ends any vertical merge through it.
        open.retain(|c, _| touched.contains(c));
        let grid_after = usize::try_from(parsed.props.grid_after.unwrap_or(0)).unwrap_or(0);
        widths.push(col.saturating_add(grid_after));
        rows.push(Row {
            id: row_id,
            props: parsed.props,
            revision: parsed.revision,
            cells,
        });
    }

    let max_width = widths.iter().copied().max().unwrap_or(0).max(grid.len());
    let mut irregular = false;
    for (row, width) in rows.iter_mut().zip(widths) {
        if row.cells.is_empty() {
            // A row without cells (invalid, but Word opens it): one empty cell over the grid.
            let before = usize::try_from(row.props.grid_before.unwrap_or(0)).unwrap_or(0);
            let span = u32::try_from(max_width.saturating_sub(before).max(1)).unwrap_or(1);
            let blocks = vec![empty_paragraph(story)];
            row.cells.push(Cell {
                id: story.ctx.doc.alloc_id(),
                props: Default::default(),
                grid_span: span,
                v_merge: None,
                blocks,
            });
            row.props.grid_after = None;
            irregular = true;
        } else if width < max_width {
            // A ragged row: Word lays it out as if it had w:gridAfter, which the writer emits.
            let missing = u32::try_from(max_width - width).unwrap_or(u32::MAX);
            row.props.grid_after = Some(row.props.grid_after.unwrap_or(0).saturating_add(missing));
            irregular = true;
        }
    }
    if irregular {
        story.warn(
            WarningCode::TableIrregular,
            Severity::Info,
            "Rows with fewer cells than the table grid were completed as Word lays them out.",
        );
    }
    if rows.is_empty() {
        let blocks = vec![empty_paragraph(story)];
        let cell = Cell {
            id: story.ctx.doc.alloc_id(),
            props: Default::default(),
            grid_span: u32::try_from(grid.len().max(1)).unwrap_or(1),
            v_merge: None,
            blocks,
        };
        rows.push(Row {
            id: story.ctx.doc.alloc_id(),
            props: Default::default(),
            revision: None,
            cells: vec![cell],
        });
        story.warn(
            WarningCode::TableIrregular,
            Severity::Info,
            "A table without rows was given one empty row.",
        );
    }

    let id = story.ctx.doc.alloc_id();
    if !table_xml.is_empty() {
        story.ctx.extras.tables.insert(id, table_xml);
    }
    Block::Table(Table {
        id,
        props,
        grid,
        rows,
    })
}
