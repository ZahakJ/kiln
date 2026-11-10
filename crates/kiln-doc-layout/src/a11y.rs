// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree: headings with their level, paragraphs, lists (nested
//! by level) of list items with their labels, tables → rows → cells (header rows as column
//! headers), pictures and shapes named by their alt text, text boxes with their paragraphs, and
//! the headers, footers and notes of the pages shown. [`visible_nodes`] covers the visible pages
//! ± 1 with bounds in document coordinates; [`document_nodes`] the whole body, for the Accessible
//! text view. The session turns either into the protocol's `A11yTree`.

use kiln_common::NodeId;
use kiln_doc::{Block, Document, Drawing, DrawingKind, InlineObject, ListLabel, Paragraph, Table};
use kiln_scene::Bounds;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::engine::DocLayout;
use crate::page::{Item, StoryKind};

/// The role of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// A heading (with its level).
    Heading,
    /// A paragraph.
    Paragraph,
    /// A list of list items (and nested lists).
    List,
    /// A list item.
    ListItem,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A cell of a header row.
    ColumnHeader,
    /// A picture or shape.
    Image,
    /// A text box: a shape with paragraphs.
    TextBox,
    /// A header, footer or the notes of a page.
    Group,
}

/// A node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The model node (synthetic for lists and note groups, see [`LIST_BIT`]).
    pub id: NodeId,
    /// Role.
    pub role: Role,
    /// Accessible name (alt text of pictures, the caption of tables, the group's name).
    pub name: Option<String>,
    /// Text (paragraphs).
    pub text: Option<String>,
    /// Bounds in document coordinates.
    pub bounds: Option<Bounds>,
    /// Heading level 1–9; the nesting level (1-based) of a list.
    pub level: Option<u8>,
    /// Children.
    pub children: Vec<NodeId>,
    /// In [`document_nodes`], paragraphs: `text` is the model's text (one `U+FFFC` per inline
    /// object) and this what each object reads as; a list item's label is then its `name`.
    pub objects: Option<Vec<String>>,
    /// In [`document_nodes`], paragraphs: the tracked insertions and deletions in `text`.
    pub revisions: Option<Vec<Revision>>,
}

/// How the review display shows tracked changes (Word's markup modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RevisionView {
    /// All Markup: insertions and deletions marked.
    #[default]
    Markup,
    /// Simple Markup and No Markup: the text after the changes.
    Final,
    /// Original: the text before them.
    Original,
}

/// How one tracked change shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RevisionShow {
    /// Marked as inserted or deleted.
    Marked,
    /// As ordinary text.
    Plain,
    /// Not shown.
    Hidden,
}

/// A tracked insertion or deletion of a paragraph (model offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    /// First char.
    pub start: usize,
    /// Past the last char.
    pub end: usize,
    /// An insertion (else a deletion).
    pub inserted: bool,
    /// How it shows.
    pub show: RevisionShow,
}

/// Lists have no model node: a list's id is its first item's id with this bit set (model ids
/// stay far below it, and the result stays an exact JavaScript number).
pub const LIST_BIT: u64 = 1 << 51;
/// The notes of page `n` are grouped under `NOTES_BASE + n`.
pub const NOTES_BASE: u64 = 1 << 52;

/// The name of a picture or shape: its alt text (description), else its title, else its name,
/// else what it is.
#[must_use]
pub fn drawing_name(d: &Drawing) -> String {
    d.description
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| d.title.clone().filter(|s| !s.trim().is_empty()))
        .or_else(|| Some(d.name.clone()).filter(|n| !n.trim().is_empty()))
        .unwrap_or_else(|| {
            match (&d.kind, &d.text_box) {
                (_, Some(_)) => "Text box",
                (DrawingKind::Picture { .. }, _) => "Picture",
                (DrawingKind::Chart { .. }, _) => "Chart",
                (DrawingKind::Group, _) => "Group",
                (DrawingKind::Shape { .. } | DrawingKind::Other, _) => "Shape",
            }
            .to_owned()
        })
}

fn always(_: NodeId) -> bool {
    true
}

fn no_bounds(_: NodeId) -> Option<Bounds> {
    None
}

struct Builder<'a> {
    doc: &'a Document,
    labels: std::collections::HashMap<NodeId, ListLabel>,
    on: &'a dyn Fn(NodeId) -> bool,
    bounds_of: &'a dyn Fn(NodeId) -> Option<Bounds>,
    drawing_bounds: FxHashMap<NodeId, Bounds>,
    nodes: Vec<Node>,
    seen: FxHashSet<NodeId>,
    /// Model-aligned paragraph text ([`document_nodes`]).
    aligned: bool,
    /// How tracked changes show ([`document_nodes`]).
    view: RevisionView,
}

impl Builder<'_> {
    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn paragraph(&mut self, p: &Paragraph) -> NodeId {
        self.paragraph_at(p);
        p.id
    }

    /// Pushes a paragraph's node (and its pictures); returns the node's index.
    fn paragraph_at(&mut self, p: &Paragraph) -> usize {
        let r = kiln_doc::resolve::resolve_paragraph(&self.doc.context(), &p.props, None);
        let label = self.labels.get(&p.id);
        let (role, level) = if r.outline_level < 9 {
            (Role::Heading, Some(r.outline_level + 1))
        } else if label.is_some() {
            (Role::ListItem, None)
        } else {
            (Role::Paragraph, None)
        };
        let revisions = self.aligned.then(|| {
            p.content
                .marks()
                .iter()
                .filter_map(|m| {
                    let inserted = match m.mark {
                        kiln_doc::Mark::Insertion(_) | kiln_doc::Mark::MoveTo(_) => true,
                        kiln_doc::Mark::Deletion(_) | kiln_doc::Mark::MoveFrom(_) => false,
                        _ => return None,
                    };
                    let show = match (self.view, inserted) {
                        (RevisionView::Markup, _) => RevisionShow::Marked,
                        (RevisionView::Final, true) | (RevisionView::Original, false) => {
                            RevisionShow::Plain
                        }
                        (RevisionView::Final, false) | (RevisionView::Original, true) => {
                            RevisionShow::Hidden
                        }
                    };
                    Some(Revision {
                        start: m.start,
                        end: m.end,
                        inserted,
                        show,
                    })
                })
                .collect()
        });
        let (name, text, objects) = if self.aligned {
            let objects = p
                .content
                .objects()
                .iter()
                .map(|o| match o {
                    InlineObject::Drawing(d) => drawing_name(d),
                    other => other.plain_text(),
                })
                .collect();
            (
                label.map(|l| l.text.clone()),
                p.content.raw_text(),
                Some(objects),
            )
        } else {
            let mut text = p.content.plain_text(true);
            if let Some(l) = label {
                text = format!("{} {text}", l.text);
            }
            (None, text, None)
        };
        let at = self.push(Node {
            id: p.id,
            role,
            name,
            text: Some(text),
            bounds: (self.bounds_of)(p.id),
            level,
            children: Vec::new(),
            objects,
            revisions,
        });
        let mut kids = Vec::new();
        for o in p.content.objects() {
            if let InlineObject::Drawing(d) = o
                && let Some(id) = self.drawing(d)
            {
                kids.push(id);
            }
        }
        self.nodes[at].children = kids;
        at
    }

    fn drawing(&mut self, d: &Drawing) -> Option<NodeId> {
        if d.decorative || d.hidden || !self.seen.insert(d.id) {
            return None;
        }
        let at = self.push(Node {
            id: d.id,
            role: if d.text_box.is_some() {
                Role::TextBox
            } else {
                Role::Image
            },
            name: Some(drawing_name(d)),
            text: None,
            bounds: self.drawing_bounds.get(&d.id).copied(),
            level: None,
            children: Vec::new(),
            objects: None,
            revisions: None,
        });
        if let Some(blocks) = &d.text_box {
            let mut kids = Vec::new();
            // A text box is shown with its anchor.
            let on = std::mem::replace(&mut self.on, &always);
            self.blocks(blocks, &mut kids);
            self.on = on;
            self.nodes[at].children = kids;
        }
        Some(d.id)
    }

    fn table_on(&self, t: &Table) -> bool {
        t.rows.iter().any(|r| {
            r.cells.iter().any(|c| {
                c.blocks.iter().any(|b| match b {
                    Block::Paragraph(p) => (self.on)(p.id),
                    Block::Table(t) => self.table_on(t),
                    _ => false,
                })
            })
        })
    }

    fn table(&mut self, t: &Table) -> NodeId {
        let ti = self.push(Node {
            id: t.id,
            role: Role::Table,
            name: t.props.caption.clone().filter(|c| !c.trim().is_empty()),
            text: None,
            bounds: None,
            level: None,
            children: Vec::new(),
            objects: None,
            revisions: None,
        });
        let header_rows = t.header_rows();
        let mut table_bounds: Option<Bounds> = None;
        for (r, row) in t.rows.iter().enumerate() {
            let ri = self.push(Node {
                id: row.id,
                role: Role::Row,
                name: None,
                text: None,
                bounds: None,
                level: None,
                children: Vec::new(),
                objects: None,
                revisions: None,
            });
            self.nodes[ti].children.push(row.id);
            let mut row_bounds: Option<Bounds> = None;
            for cell in &row.cells {
                let ci = self.push(Node {
                    id: cell.id,
                    role: if r < header_rows {
                        Role::ColumnHeader
                    } else {
                        Role::Cell
                    },
                    name: None,
                    text: None,
                    bounds: None,
                    level: None,
                    children: Vec::new(),
                    objects: None,
                    revisions: None,
                });
                self.nodes[ri].children.push(cell.id);
                let first = self.nodes.len();
                let mut kids = Vec::new();
                self.blocks(&cell.blocks, &mut kids);
                let cell_bounds = self.nodes[first..]
                    .iter()
                    .filter_map(|n| n.bounds)
                    .reduce(|a, b| a.union(&b));
                self.nodes[ci].children = kids;
                self.nodes[ci].bounds = cell_bounds;
                if let Some(cb) = cell_bounds {
                    row_bounds = Some(row_bounds.map_or(cb, |a| a.union(&cb)));
                }
            }
            self.nodes[ri].bounds = row_bounds;
            if let Some(rb) = row_bounds {
                table_bounds = Some(table_bounds.map_or(rb, |a| a.union(&rb)));
            }
        }
        self.nodes[ti].bounds = table_bounds;
        t.id
    }

    /// The nodes of `blocks` (those shown), with consecutive list items gathered into lists
    /// nested by level.
    fn blocks(&mut self, blocks: &[Block], out: &mut Vec<NodeId>) {
        // Open lists, innermost last: (node index, level, index of its last item).
        let mut lists: Vec<(usize, u8, usize)> = Vec::new();
        for b in blocks {
            match b {
                Block::Paragraph(p) if (self.on)(p.id) => {
                    let at = self.paragraph_at(p);
                    let id = p.id;
                    let level = self.labels.get(&id).map(|l| l.level);
                    let Some(level) = level.filter(|_| self.nodes[at].role == Role::ListItem)
                    else {
                        lists.clear();
                        out.push(id);
                        continue;
                    };
                    while lists.last().is_some_and(|&(_, l, _)| l > level) {
                        lists.pop();
                    }
                    if lists.last().is_none_or(|&(_, l, _)| l < level) {
                        let list_id = NodeId(id.0 | LIST_BIT);
                        let depth = u8::try_from(lists.len() + 1).unwrap_or(u8::MAX);
                        let li = self.push(Node {
                            id: list_id,
                            role: Role::List,
                            name: None,
                            text: None,
                            bounds: None,
                            level: Some(depth),
                            children: Vec::new(),
                            objects: None,
                            revisions: None,
                        });
                        match lists.last() {
                            // A deeper list belongs to the item before it.
                            Some(&(_, _, item)) => self.nodes[item].children.push(list_id),
                            None => out.push(list_id),
                        }
                        lists.push((li, level, at));
                    }
                    if let Some((li, _, last)) = lists.last_mut() {
                        *last = at;
                        let li = *li;
                        self.nodes[li].children.push(id);
                        if let Some(pb) = self.nodes[at].bounds {
                            let lb = &mut self.nodes[li].bounds;
                            *lb = Some(lb.map_or(pb, |a| a.union(&pb)));
                        }
                    }
                }
                Block::Table(t) if self.table_on(t) => {
                    lists.clear();
                    let id = self.table(t);
                    out.push(id);
                }
                Block::Sdt(s) => self.blocks(&s.blocks, out),
                Block::Field(f) => self.blocks(&f.blocks, out),
                _ => {}
            }
        }
    }

    fn body(&mut self) -> Vec<NodeId> {
        let mut top = Vec::new();
        for s in &self.doc.sections {
            self.blocks(&s.blocks, &mut top);
        }
        top
    }
}

/// The nodes of the whole body (top-level nodes first in reading order, then their
/// descendants) and the ids of the top-level ones, without bounds: the Accessible text view.
#[must_use]
pub fn document_nodes(doc: &Document, view: RevisionView) -> (Vec<Node>, Vec<NodeId>) {
    let mut b = Builder {
        doc,
        labels: doc.list_labels(),
        on: &always,
        bounds_of: &no_bounds,
        drawing_bounds: FxHashMap::default(),
        nodes: Vec::new(),
        seen: FxHashSet::default(),
        aligned: true,
        view,
    };
    let top = b.body();
    (b.nodes, top)
}

/// The nodes of pages `first..=last` (top-level nodes first, in reading order, then their
/// descendants) and the ids of the top-level ones: the body content shown there, then each
/// page's header, footer and notes.
#[must_use]
pub fn visible_nodes(
    layout: &DocLayout,
    doc: &Document,
    first: usize,
    last: usize,
) -> (Vec<Node>, Vec<NodeId>) {
    let on = |id: NodeId| {
        layout
            .lines_of(id)
            .iter()
            .any(|r| (first..=last).contains(&(r.page as usize)))
    };
    let bounds_of = |id: NodeId| -> Option<Bounds> {
        let mut b: Option<Bounds> = None;
        for r in layout.lines_of(id) {
            if !(first..=last).contains(&(r.page as usize)) {
                continue;
            }
            let l = layout.line(*r)?;
            let rect = l.rect();
            let (x, y) = layout.to_doc(r.page as usize, rect.x, rect.y);
            let rr = Bounds::new(x, y, rect.w, rect.h);
            b = Some(b.map_or(rr, |a| a.union(&rr)));
        }
        b
    };
    let mut drawing_bounds = FxHashMap::default();
    for (pi, page) in layout.pages().iter().enumerate().take(last + 1).skip(first) {
        for it in page.items.iter().chain(&page.behind).chain(&page.front) {
            if let Item::Drawing(d) = it {
                let (x, y) = layout.to_doc(pi, d.rect.x, d.rect.y);
                drawing_bounds
                    .entry(d.drawing.id)
                    .or_insert_with(|| Bounds::new(x, y, d.rect.w, d.rect.h));
            }
        }
    }
    let mut b = Builder {
        doc,
        labels: doc.list_labels(),
        on: &on,
        bounds_of: &bounds_of,
        drawing_bounds,
        nodes: Vec::new(),
        seen: FxHashSet::default(),
        aligned: false,
        view: RevisionView::Markup,
    };
    let mut top = b.body();
    // Floating objects anchored in paragraphs on other pages, shown here.
    for page in layout.pages().iter().take(last + 1).skip(first) {
        for it in page.items.iter().chain(&page.behind).chain(&page.front) {
            if let Item::Drawing(d) = it
                && let Some(id) = b.drawing(&d.drawing)
            {
                top.push(id);
            }
        }
    }
    // Each page's header, footer and notes (footnotes under the text, endnotes after it), once
    // per story.
    let mut groups: Vec<(NodeId, &str, Vec<NodeId>)> = Vec::new();
    for (pi, page) in layout.pages().iter().enumerate().take(last + 1).skip(first) {
        let in_header = |id: NodeId| {
            page.header
                .iter()
                .any(|it| matches!(it, Item::Line(l) if l.para.id() == id))
        };
        for l in layout.page_lines(pi) {
            let (gid, name) = match l.story {
                StoryKind::HeaderFooter(s) if in_header(l.para.id()) => (s, "Header"),
                StoryKind::HeaderFooter(s) => (s, "Footer"),
                StoryKind::Note(_) => (NodeId(NOTES_BASE + pi as u64), "Notes"),
                StoryKind::Body | StoryKind::TextBox(_) => continue,
            };
            let id = l.para.id();
            match groups.iter_mut().find(|g| g.0 == gid) {
                Some(g) if !g.2.contains(&id) => g.2.push(id),
                Some(_) => {}
                None => groups.push((gid, name, vec![id])),
            }
        }
    }
    for (gid, name, paras) in groups {
        let gi = b.push(Node {
            id: gid,
            role: Role::Group,
            name: Some(name.to_owned()),
            text: None,
            bounds: None,
            level: None,
            children: Vec::new(),
            objects: None,
            revisions: None,
        });
        let mut kids = Vec::new();
        for id in paras {
            if let Some(p) = doc.paragraph(id)
                && b.seen.insert(id)
            {
                kids.push(b.paragraph(p));
            }
        }
        b.nodes[gi].children = kids;
        top.push(gid);
    }
    (b.nodes, top)
}
