// SPDX-License-Identifier: AGPL-3.0-or-later
//! Blocks, tables, sections and stories ↔ Loro maps in movable lists.
//!
//! Every node with an id is a map holding its kind (`k`) and id (`id`); its children are a
//! movable list (`blocks`, `rows`, `cells`), its property sets child maps with one key per field
//! (`props`, `mark`, `rev`, `meta`), and a paragraph's content a `LoroText` (`text`).

use std::collections::{HashMap, HashSet};

use kiln_common::NodeId;
use kiln_doc::{
    Block, Cell, Comment, FieldBlock, HeaderFooter, Note, OpaqueBlock, Paragraph, Row, SdtBlock,
    Section, Table,
};
use loro::{Container, ContainerTrait, LoroMap, LoroMovableList};

use super::text::{decode_text, sync_text};
use crate::error::{Result, corrupt};
use crate::value::{
    child_list, child_map, child_text, find_list, find_map, find_text, get_child_fields,
    get_fields, get_id, get_json, get_json_or_default, get_str, id_value, map_at, maps_of,
    put_fields, put_i64, put_json, put_str,
};

/// The kind key.
pub(crate) const K: &str = "k";
/// The id key.
pub(crate) const ID: &str = "id";
/// A container's blocks.
pub(crate) const BLOCKS: &str = "blocks";
/// A table's rows.
pub(crate) const ROWS: &str = "rows";
/// A row's cells.
pub(crate) const CELLS: &str = "cells";

/// Node kinds.
pub(crate) mod kind {
    pub const PARAGRAPH: &str = "p";
    pub const TABLE: &str = "t";
    pub const ROW: &str = "row";
    pub const CELL: &str = "cell";
    pub const SDT: &str = "s";
    pub const FIELD: &str = "f";
    pub const OPAQUE: &str = "x";
    pub const SECTION: &str = "sec";
    pub const HEADER_FOOTER: &str = "hf";
    pub const NOTE: &str = "note";
    pub const COMMENT: &str = "cmt";
}

/// The Loro map of every node, by id.
#[derive(Debug, Default, Clone)]
pub(crate) struct Nodes(HashMap<NodeId, LoroMap>);

impl Nodes {
    pub fn get(&self, id: NodeId) -> Option<&LoroMap> {
        self.0.get(&id)
    }

    pub fn insert(&mut self, id: NodeId, map: &LoroMap) {
        self.0.insert(id, map.clone());
    }

    /// Forgets a removed map and everything below it.
    pub fn forget(&mut self, map: &LoroMap) {
        if let Ok(id) = get_id(map, ID)
            && self.0.get(&id).is_some_and(|m| m.id() == map.id())
        {
            self.0.remove(&id);
        }
        for key in [BLOCKS, ROWS, CELLS] {
            if let Some(list) = find_list(map, key) {
                for m in maps_of(&list) {
                    self.forget(&m);
                }
            }
        }
    }
}

/// The kind of a node map.
pub(crate) fn kind_of(map: &LoroMap) -> Option<String> {
    get_str(map, K)
}

fn head(map: &LoroMap, k: &str, id: NodeId, nodes: &mut Nodes) -> Result<()> {
    put_str(map, K, k)?;
    put_i64(map, ID, id_value(id))?;
    nodes.insert(id, map);
    Ok(())
}

// ------------------------------------------------------------------ writing

/// Writes a paragraph's properties and content.
pub(crate) fn write_paragraph(map: &LoroMap, p: &Paragraph, nodes: &mut Nodes) -> Result<()> {
    head(map, kind::PARAGRAPH, p.id, nodes)?;
    put_fields(&child_map(map, "props")?, &p.props)?;
    put_fields(&child_map(map, "mark")?, &p.mark_props)?;
    put_fields(&child_map(map, "rev")?, &p.revision)?;
    sync_text(&child_text(map, "text")?, &p.content)
}

/// Writes a block, children included.
pub(crate) fn write_block(map: &LoroMap, block: &Block, nodes: &mut Nodes) -> Result<()> {
    match block {
        Block::Paragraph(p) => write_paragraph(map, p, nodes),
        Block::Table(t) => {
            write_table_shallow(map, t, nodes)?;
            rewrite(&child_list(map, ROWS)?, &t.rows, |r| r.id, write_row, nodes)
        }
        Block::Sdt(s) => {
            head(map, kind::SDT, s.id, nodes)?;
            put_json(map, "info", &s.info)?;
            write_blocks(map, &s.blocks, nodes)
        }
        Block::Field(f) => {
            head(map, kind::FIELD, f.id, nodes)?;
            put_json(map, "field", &f.field)?;
            write_blocks(map, &f.blocks, nodes)
        }
        Block::Opaque(o) => {
            head(map, kind::OPAQUE, o.id, nodes)?;
            put_str(map, "xml", &o.xml)?;
            put_str(map, "label", &o.label)
        }
    }
}

/// Writes a container's block list (blocks already there with the right id are kept).
pub(crate) fn write_blocks(map: &LoroMap, blocks: &[Block], nodes: &mut Nodes) -> Result<()> {
    rewrite(
        &child_list(map, BLOCKS)?,
        blocks,
        Block::id,
        write_block,
        nodes,
    )
}

/// Brings a list level with `items` and writes every entry in full (entries already there
/// write only what changed).
pub(crate) fn rewrite<T>(
    list: &LoroMovableList,
    items: &[T],
    id: impl Fn(&T) -> NodeId,
    write: impl Fn(&LoroMap, &T, &mut Nodes) -> Result<()>,
    nodes: &mut Nodes,
) -> Result<()> {
    remove_missing(list, &items.iter().map(&id).collect(), nodes)?;
    let created = place(list, items, &id, &write, nodes)?;
    for (i, item) in items.iter().enumerate() {
        if !created[i]
            && let Some(m) = map_at(list, i)
        {
            write(&m, item, nodes)?;
        }
    }
    Ok(())
}

/// A table's own values (not its rows).
pub(crate) fn write_table_shallow(map: &LoroMap, t: &Table, nodes: &mut Nodes) -> Result<()> {
    head(map, kind::TABLE, t.id, nodes)?;
    put_fields(&child_map(map, "props")?, &t.props)?;
    put_json(map, "grid", &t.grid)
}

/// A row's own values (not its cells).
pub(crate) fn write_row_shallow(map: &LoroMap, r: &Row, nodes: &mut Nodes) -> Result<()> {
    head(map, kind::ROW, r.id, nodes)?;
    put_fields(&child_map(map, "props")?, &r.props)?;
    put_json(map, "rev", &r.revision)
}

/// A cell's own values (not its blocks).
pub(crate) fn write_cell_shallow(map: &LoroMap, c: &Cell, nodes: &mut Nodes) -> Result<()> {
    head(map, kind::CELL, c.id, nodes)?;
    put_fields(&child_map(map, "props")?, &c.props)?;
    put_i64(map, "span", i64::from(c.grid_span))?;
    put_json(map, "vmerge", &c.v_merge)
}

fn write_row(map: &LoroMap, r: &Row, nodes: &mut Nodes) -> Result<()> {
    write_row_shallow(map, r, nodes)?;
    rewrite(
        &child_list(map, CELLS)?,
        &r.cells,
        |c| c.id,
        write_cell,
        nodes,
    )
}

fn write_cell(map: &LoroMap, c: &Cell, nodes: &mut Nodes) -> Result<()> {
    write_cell_shallow(map, c, nodes)?;
    write_blocks(map, &c.blocks, nodes)
}

/// A section's own values (not its blocks).
pub(crate) fn write_section_shallow(map: &LoroMap, s: &Section, nodes: &mut Nodes) -> Result<()> {
    head(map, kind::SECTION, s.id, nodes)?;
    put_fields(&child_map(map, "props")?, &s.props)?;
    put_json(map, "headers", &s.headers)?;
    put_json(map, "footers", &s.footers)
}

/// Writes a section with its blocks.
pub(crate) fn write_section(map: &LoroMap, s: &Section, nodes: &mut Nodes) -> Result<()> {
    write_section_shallow(map, s, nodes)?;
    write_blocks(map, &s.blocks, nodes)
}

/// The stories other than the body.
pub(crate) enum StoryRef<'a> {
    HeaderFooter(&'a HeaderFooter),
    Note(&'a Note),
    Comment(&'a Comment),
}

/// A story's own values (not its blocks).
pub(crate) fn write_story_shallow(
    map: &LoroMap,
    s: &StoryRef<'_>,
    nodes: &mut Nodes,
) -> Result<()> {
    match s {
        StoryRef::HeaderFooter(h) => {
            head(map, kind::HEADER_FOOTER, h.id, nodes)?;
            put_json(map, "part", &h.part)
        }
        StoryRef::Note(n) => {
            head(map, kind::NOTE, n.id, nodes)?;
            put_json(map, "kind", &n.kind)?;
            put_json(map, "type", &n.note_type)
        }
        StoryRef::Comment(c) => {
            head(map, kind::COMMENT, c.id, nodes)?;
            put_fields(&child_map(map, "meta")?, &c.meta)
        }
    }
}

/// Writes a story with its blocks.
pub(crate) fn write_story(map: &LoroMap, s: &StoryRef<'_>, nodes: &mut Nodes) -> Result<()> {
    write_story_shallow(map, s, nodes)?;
    let blocks = match s {
        StoryRef::HeaderFooter(h) => &h.blocks,
        StoryRef::Note(n) => &n.blocks,
        StoryRef::Comment(c) => &c.blocks,
    };
    write_blocks(map, blocks, nodes)
}

/// Deletes the entries of a list whose id is not in `keep`, and later duplicates of an id.
pub(crate) fn remove_missing(
    list: &LoroMovableList,
    keep: &HashSet<NodeId>,
    nodes: &mut Nodes,
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut doomed = Vec::new();
    for i in 0..list.len() {
        let id = map_at(list, i).and_then(|m| get_id(&m, ID).ok());
        match id {
            Some(id) if keep.contains(&id) && seen.insert(id) => {}
            _ => doomed.push(i),
        }
    }
    for i in doomed.into_iter().rev() {
        if let Some(m) = map_at(list, i) {
            nodes.forget(&m);
        }
        list.delete(i, 1)?;
    }
    Ok(())
}

/// Puts a list's entries in the order of `items`: entries with the right id are moved into
/// place, missing ones written with `write`. Call [`remove_missing`] first. Returns, per item,
/// whether its entry was created here.
pub(crate) fn place<T>(
    list: &LoroMovableList,
    items: &[T],
    id: impl Fn(&T) -> NodeId,
    write: impl Fn(&LoroMap, &T, &mut Nodes) -> Result<()>,
    nodes: &mut Nodes,
) -> Result<Vec<bool>> {
    let mut created = vec![false; items.len()];
    let mut have: Vec<Option<NodeId>> = (0..list.len())
        .map(|i| map_at(list, i).and_then(|m| get_id(&m, ID).ok()))
        .collect();
    for (i, item) in items.iter().enumerate() {
        let want = id(item);
        if have.get(i) == Some(&Some(want)) {
            continue;
        }
        if let Some(j) = (i + 1..have.len()).find(|&j| have[j] == Some(want)) {
            list.mov(j, i)?;
            let v = have.remove(j);
            have.insert(i, v);
        } else {
            let m = list.insert_container(i, LoroMap::new())?;
            write(&m, item, nodes)?;
            have.insert(i, Some(want));
            created[i] = true;
        }
    }
    while list.len() > items.len() {
        let last = list.len() - 1;
        if let Some(m) = map_at(list, last) {
            nodes.forget(&m);
        }
        list.delete(last, 1)?;
    }
    Ok(created)
}

// ------------------------------------------------------------------ reading

/// A paragraph read back.
pub(crate) fn read_paragraph(map: &LoroMap, nodes: &mut Nodes) -> Result<Paragraph> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Paragraph {
        id,
        props: get_child_fields(map, "props")?,
        mark_props: get_child_fields(map, "mark")?,
        revision: get_child_fields(map, "rev")?,
        content: match find_text(map, "text") {
            Some(t) => decode_text(&t)?,
            None => kiln_doc::Inlines::new(),
        },
    })
}

/// A block read back, children included.
pub(crate) fn read_block(map: &LoroMap, nodes: &mut Nodes) -> Result<Block> {
    let k = kind_of(map).ok_or_else(|| corrupt("a block without a kind"))?;
    let id = get_id(map, ID)?;
    let block = match k.as_str() {
        kind::PARAGRAPH => return Ok(Block::Paragraph(read_paragraph(map, nodes)?)),
        kind::TABLE => Block::Table(Table {
            id,
            props: get_child_fields(map, "props")?,
            grid: get_json_or_default(map, "grid")?,
            rows: read_list(map, ROWS, nodes, read_row)?,
        }),
        kind::SDT => Block::Sdt(SdtBlock {
            id,
            info: get_json(map, "info")?,
            blocks: read_blocks(map, nodes)?,
        }),
        kind::FIELD => Block::Field(FieldBlock {
            id,
            field: get_json(map, "field")?,
            blocks: read_blocks(map, nodes)?,
        }),
        kind::OPAQUE => Block::Opaque(OpaqueBlock {
            id,
            xml: get_str(map, "xml").unwrap_or_default(),
            label: get_str(map, "label").unwrap_or_default(),
        }),
        other => return Err(corrupt(format!("unknown block kind {other}"))),
    };
    nodes.insert(id, map);
    Ok(block)
}

/// A row read back, cells included.
pub(crate) fn read_row(map: &LoroMap, nodes: &mut Nodes) -> Result<Row> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Row {
        id,
        props: get_child_fields(map, "props")?,
        revision: get_json_or_default(map, "rev")?,
        cells: read_list(map, CELLS, nodes, read_cell)?,
    })
}

/// A cell read back, blocks included.
pub(crate) fn read_cell(map: &LoroMap, nodes: &mut Nodes) -> Result<Cell> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Cell {
        id,
        props: get_child_fields(map, "props")?,
        grid_span: crate::value::get_i64(map, "span")
            .and_then(|s| u32::try_from(s).ok())
            .unwrap_or(1),
        v_merge: get_json_or_default(map, "vmerge")?,
        blocks: read_blocks(map, nodes)?,
    })
}

/// A container's blocks read back. Later duplicates of an id are left out (the next local
/// sync removes them from the CRDT).
pub(crate) fn read_blocks(map: &LoroMap, nodes: &mut Nodes) -> Result<Vec<Block>> {
    read_list(map, BLOCKS, nodes, read_block)
}

/// The entries of the list under `key`, read with `read`, first occurrence of each id only.
pub(crate) fn read_list<T>(
    map: &LoroMap,
    key: &str,
    nodes: &mut Nodes,
    read: impl Fn(&LoroMap, &mut Nodes) -> Result<T>,
) -> Result<Vec<T>> {
    let Some(list) = find_list(map, key) else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for m in maps_of(&list) {
        if let Ok(id) = get_id(&m, ID)
            && seen.insert(id)
        {
            out.push(read(&m, nodes)?);
        }
    }
    Ok(out)
}

/// The ids of a list's entries in order (first occurrence of each).
pub(crate) fn list_ids(list: &LoroMovableList) -> Vec<NodeId> {
    let mut seen = HashSet::new();
    maps_of(list)
        .iter()
        .filter_map(|m| get_id(m, ID).ok())
        .filter(|id| seen.insert(*id))
        .collect()
}

/// A section read back, blocks included.
pub(crate) fn read_section(map: &LoroMap, nodes: &mut Nodes) -> Result<Section> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Section {
        id,
        props: get_fields(
            &find_map(map, "props").ok_or_else(|| corrupt("a section without properties"))?,
        )?,
        headers: get_json_or_default(map, "headers")?,
        footers: get_json_or_default(map, "footers")?,
        blocks: read_blocks(map, nodes)?,
    })
}

/// A header or footer read back.
pub(crate) fn read_header_footer(map: &LoroMap, nodes: &mut Nodes) -> Result<HeaderFooter> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(HeaderFooter {
        id,
        part: get_json(map, "part")?,
        blocks: read_blocks(map, nodes)?,
    })
}

/// A note read back.
pub(crate) fn read_note(map: &LoroMap, nodes: &mut Nodes) -> Result<Note> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Note {
        id,
        kind: get_json(map, "kind")?,
        note_type: get_json(map, "type")?,
        blocks: read_blocks(map, nodes)?,
    })
}

/// A comment read back.
pub(crate) fn read_comment(map: &LoroMap, nodes: &mut Nodes) -> Result<Comment> {
    let id = get_id(map, ID)?;
    nodes.insert(id, map);
    Ok(Comment {
        id,
        meta: get_child_fields(map, "meta")?,
        blocks: read_blocks(map, nodes)?,
    })
}

/// The node map a container is, or the nearest one above it, with the key of the child the
/// change went through (for a property map or a list of the node).
pub(crate) fn owner_of(
    doc: &loro::LoroDoc,
    path: &[(loro::ContainerID, loro::Index)],
) -> Option<(LoroMap, Option<String>)> {
    for (i, (cid, _)) in path.iter().enumerate().rev() {
        if let Some(Container::Map(m)) = doc.get_container(cid.clone())
            && kind_of(&m).is_some()
        {
            let via = path.get(i + 1).and_then(|(_, idx)| match idx {
                loro::Index::Key(k) => Some(k.to_string()),
                _ => None,
            });
            return Some((m, via));
        }
    }
    None
}
