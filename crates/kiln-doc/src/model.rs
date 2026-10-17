// SPDX-License-Identifier: AGPL-3.0-or-later
//! The document tree: `Document > Section+ > Block*`, with headers, footers, notes and comments
//! as separate stories, and the node lookup every operation uses.
//!
//! Every block-level node (section, paragraph, table, row, cell, content control, field result,
//! opaque block, header, footer, note, comment, drawing) has a stable [`NodeId`] that layout,
//! selection, comments and collaboration refer to. Ids are unique in a document and never reused.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ops::ControlFlow;

use crate::DocError;
use crate::fields::Field;
use crate::inline::{InlineObject, Inlines};
use crate::numbering::Numbering;
use crate::props::{ParagraphProps, RunProps, TableCellProps, TableProps, TableRowProps};
use crate::revision::{InsDelRevision, ParagraphRevision};
use crate::sdt::SdtInfo;
use crate::section::{HeaderFooterRefs, SectionProps};
use crate::settings::{DocProperties, Media, Settings};
use crate::st::{
    AlignH, AlignV, NoteKind, NoteType, RelativeFromH, RelativeFromV, VMerge, WrapSide, WrapType,
};
use crate::style::StyleSheet;
use crate::theme::DocTheme;
use kiln_common::NodeId;

// ------------------------------------------------------------------ blocks

/// A paragraph (`w:p`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    /// Stable id.
    pub id: NodeId,
    /// Direct paragraph properties (`w:pPr`).
    pub props: ParagraphProps,
    /// Run properties of the paragraph mark (`w:pPr/w:rPr`): the pilcrow's formatting, used for
    /// the list label and for an empty paragraph's line height.
    pub mark_props: RunProps,
    /// Paragraph-level tracked changes.
    pub revision: ParagraphRevision,
    /// Text, runs, marks and objects.
    pub content: Inlines,
}

impl Paragraph {
    /// An empty paragraph.
    #[must_use]
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            props: ParagraphProps::default(),
            mark_props: RunProps::default(),
            revision: ParagraphRevision::default(),
            content: Inlines::new(),
        }
    }

    /// The paragraph's plain text (objects as [`InlineObject::plain_text`]).
    #[must_use]
    pub fn text(&self) -> String {
        self.content.plain_text(false)
    }
}

/// A table cell (`w:tc`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    /// Stable id.
    pub id: NodeId,
    /// `w:tcPr` besides spans.
    pub props: TableCellProps,
    /// `w:gridSpan`: grid columns this cell covers (1 or more).
    pub grid_span: u32,
    /// `w:vMerge`: part of a vertically merged cell.
    pub v_merge: Option<VMerge>,
    /// Content (at least one block; Word requires a final paragraph).
    pub blocks: Vec<Block>,
}

/// A table row (`w:tr`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Stable id.
    pub id: NodeId,
    /// `w:trPr`.
    pub props: TableRowProps,
    /// A tracked insertion or deletion of the whole row.
    pub revision: Option<InsDelRevision>,
    /// Cells (at least one).
    pub cells: Vec<Cell>,
}

/// A table (`w:tbl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    /// Stable id.
    pub id: NodeId,
    /// `w:tblPr`.
    pub props: TableProps,
    /// `w:tblGrid`: grid column widths in twips.
    pub grid: Vec<i32>,
    /// Rows (at least one).
    pub rows: Vec<Row>,
}

/// One cell's place in the table grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellSlot {
    /// Row index.
    pub row: usize,
    /// Cell index within the row.
    pub cell: usize,
    /// First grid column.
    pub col: usize,
    /// Grid columns covered (`w:gridSpan`).
    pub col_span: usize,
    /// Rows covered, counting the `vMerge="continue"` cells below (1 for unmerged cells; 0 for a
    /// continuation cell, which belongs to the cell above).
    pub row_span: usize,
    /// The cell's id.
    pub id: NodeId,
}

impl Table {
    /// The number of grid columns (the grid, or the widest row when the grid is short).
    #[must_use]
    pub fn grid_width(&self) -> usize {
        let widest = self
            .rows
            .iter()
            .map(|r| {
                let before = r.props.grid_before.unwrap_or(0) as usize;
                let after = r.props.grid_after.unwrap_or(0) as usize;
                before
                    + after
                    + r.cells
                        .iter()
                        .map(|c| c.grid_span.max(1) as usize)
                        .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        widest.max(self.grid.len())
    }

    /// Every cell with its grid position and spans, row by row. Vertically merged continuation
    /// cells get `row_span` 0 and add to the span of the cell that starts the merge in the same
    /// grid column.
    #[must_use]
    pub fn cell_map(&self) -> Vec<CellSlot> {
        let mut out: Vec<CellSlot> = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            let mut col = row.props.grid_before.unwrap_or(0) as usize;
            for (c, cell) in row.cells.iter().enumerate() {
                let span = cell.grid_span.max(1) as usize;
                let continues = cell.v_merge == Some(VMerge::Continue);
                if continues
                    && let Some(top) = out
                        .iter_mut()
                        .rev()
                        .find(|s| s.col == col && s.row < r && s.row_span > 0)
                {
                    top.row_span += 1;
                }
                out.push(CellSlot {
                    row: r,
                    cell: c,
                    col,
                    col_span: span,
                    row_span: usize::from(!continues),
                    id: cell.id,
                });
                col += span;
            }
        }
        out
    }

    /// The number of leading rows marked as repeating header rows.
    #[must_use]
    pub fn header_rows(&self) -> usize {
        self.rows
            .iter()
            .take_while(|r| r.props.header == Some(true))
            .count()
    }
}

/// A block-level content control (`w:sdt` around paragraphs and tables).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdtBlock {
    /// Stable id.
    pub id: NodeId,
    /// The control's properties.
    pub info: SdtInfo,
    /// Content (at least one block).
    pub blocks: Vec<Block>,
}

/// A field whose result spans paragraphs (TOC, INDEX, BIBLIOGRAPHY): the instruction and the
/// result paragraphs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldBlock {
    /// Stable id.
    pub id: NodeId,
    /// The field (its `result` text is unused; the result is `blocks`).
    pub field: Field,
    /// The cached result (at least one block).
    pub blocks: Vec<Block>,
}

/// A block the model does not understand (`w:altChunk`, `w:customXml` wrappers), kept as XML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueBlock {
    /// Stable id.
    pub id: NodeId,
    /// The original XML.
    pub xml: String,
    /// What the editor shows.
    pub label: String,
}

/// A block-level node.
///
/// Paragraphs are by far the most common block and the largest variant, so they are stored
/// inline: boxing them would add an allocation per paragraph to save space only on the rare
/// opaque blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum Block {
    /// A paragraph.
    Paragraph(Paragraph),
    /// A table.
    Table(Table),
    /// A block content control.
    Sdt(SdtBlock),
    /// A multi-paragraph field result.
    Field(FieldBlock),
    /// Passthrough content.
    Opaque(OpaqueBlock),
}

impl Block {
    /// The block's id.
    #[must_use]
    pub fn id(&self) -> NodeId {
        match self {
            Self::Paragraph(p) => p.id,
            Self::Table(t) => t.id,
            Self::Sdt(s) => s.id,
            Self::Field(f) => f.id,
            Self::Opaque(o) => o.id,
        }
    }

    /// The paragraph, if this block is one.
    #[must_use]
    pub fn as_paragraph(&self) -> Option<&Paragraph> {
        match self {
            Self::Paragraph(p) => Some(p),
            _ => None,
        }
    }

    /// The table, if this block is one.
    #[must_use]
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Self::Table(t) => Some(t),
            _ => None,
        }
    }

    /// Nested blocks of a content control or field result.
    #[must_use]
    pub fn children(&self) -> Option<&Vec<Block>> {
        match self {
            Self::Sdt(s) => Some(&s.blocks),
            Self::Field(f) => Some(&f.blocks),
            _ => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Block>> {
        match self {
            Self::Sdt(s) => Some(&mut s.blocks),
            Self::Field(f) => Some(&mut f.blocks),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ drawings

/// A crop rectangle in thousandths of a percent of the picture (`a:srcRect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Crop {
    /// From the left.
    pub left: i32,
    /// From the top.
    pub top: i32,
    /// From the right.
    pub right: i32,
    /// From the bottom.
    pub bottom: i32,
}

/// What a drawing shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DrawingKind {
    /// A picture from the media store.
    Picture {
        /// Key into [`Document::media`] (the DOCX part name).
        media: String,
        /// Cropping.
        crop: Option<Crop>,
    },
    /// A preset shape (`wps:wsp`) such as `rect` or `ellipse`.
    Shape {
        /// The DrawingML preset name.
        preset: String,
    },
    /// A chart part.
    Chart {
        /// The chart part name.
        part: String,
    },
    /// A group of shapes (kept as XML).
    Group,
    /// Anything else (SmartArt, ink, OLE), kept as XML.
    Other,
}

/// A horizontal position: an offset in EMU or an alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PosH {
    /// `wp:posOffset`.
    Offset(i64),
    /// `wp:align`.
    Align(AlignH),
}

/// A vertical position: an offset in EMU or an alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PosV {
    /// `wp:posOffset`.
    Offset(i64),
    /// `wp:align`.
    Align(AlignV),
}

/// A floating drawing's position and wrapping (`wp:anchor`); distances in EMU.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchor {
    /// Text wrapping.
    pub wrap: WrapType,
    /// Which sides text flows on (square, tight, through).
    pub wrap_side: Option<WrapSide>,
    /// `wp:positionH/@relativeFrom`.
    pub relative_h: RelativeFromH,
    /// Horizontal position.
    pub h: PosH,
    /// `wp:positionV/@relativeFrom`.
    pub relative_v: RelativeFromV,
    /// Vertical position.
    pub v: PosV,
    /// `distT`.
    pub dist_top: i64,
    /// `distB`.
    pub dist_bottom: i64,
    /// `distL`.
    pub dist_left: i64,
    /// `distR`.
    pub dist_right: i64,
    /// `allowOverlap`.
    pub allow_overlap: bool,
    /// `layoutInCell`.
    pub layout_in_cell: bool,
    /// `locked`.
    pub locked: bool,
    /// `relativeHeight`: z-order.
    pub z_order: i64,
    /// The wrap polygon of tight/through wrapping, kept as XML.
    pub wrap_polygon_xml: Option<String>,
}

/// Inline in the text line or floating.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Placement {
    /// `wp:inline`.
    Inline,
    /// `wp:anchor`.
    Anchor(Box<Anchor>),
}

/// A drawing (`w:drawing`): picture, shape, chart or text box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawing {
    /// Stable id (also the container id of the text box, if any).
    pub id: NodeId,
    /// What it shows.
    pub kind: DrawingKind,
    /// Width in EMU (`wp:extent/@cx`).
    pub width: i64,
    /// Height in EMU (`wp:extent/@cy`).
    pub height: i64,
    /// Inline or floating.
    pub placement: Placement,
    /// `wp:docPr/@name`.
    pub name: String,
    /// `wp:docPr/@descr`: alt text.
    pub description: Option<String>,
    /// `wp:docPr/@title`.
    pub title: Option<String>,
    /// Marked decorative (no alt text needed).
    pub decorative: bool,
    /// Rotation in 60,000ths of a degree.
    pub rotation: i32,
    /// Flipped horizontally.
    pub flip_h: bool,
    /// Flipped vertically.
    pub flip_v: bool,
    /// A text box's content.
    pub text_box: Option<Vec<Block>>,
    /// The original `a:graphic` XML for what the model does not edit (fills, effects, SmartArt).
    pub xml: Option<String>,
}

impl Drawing {
    /// An inline picture.
    #[must_use]
    pub fn picture(id: NodeId, media: &str, width: i64, height: i64) -> Self {
        Self {
            id,
            kind: DrawingKind::Picture {
                media: media.to_owned(),
                crop: None,
            },
            width,
            height,
            placement: Placement::Inline,
            name: String::new(),
            description: None,
            title: None,
            decorative: false,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            text_box: None,
            xml: None,
        }
    }
}

// ------------------------------------------------------------------ stories

/// A section: its properties, header/footer references and body blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    /// Stable id.
    pub id: NodeId,
    /// `w:sectPr`.
    pub props: SectionProps,
    /// Header references (`None` = Link to Previous).
    pub headers: HeaderFooterRefs,
    /// Footer references.
    pub footers: HeaderFooterRefs,
    /// Body blocks (at least one).
    pub blocks: Vec<Block>,
}

/// Header or footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeaderFooterPart {
    /// A header part.
    Header,
    /// A footer part.
    Footer,
}

/// A header or footer story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderFooter {
    /// Stable id (what sections reference).
    pub id: NodeId,
    /// Header or footer.
    pub part: HeaderFooterPart,
    /// Content.
    pub blocks: Vec<Block>,
}

/// A footnote or endnote story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Stable id (what references point to).
    pub id: NodeId,
    /// Footnote or endnote.
    pub kind: NoteKind,
    /// Normal note or one of the separators.
    pub note_type: NoteType,
    /// Content.
    pub blocks: Vec<Block>,
}

/// A comment (`w:comment`) with its threading (`commentsExtended.xml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// Stable id (what comment marks point to).
    pub id: NodeId,
    /// The metadata.
    pub meta: CommentMeta,
    /// The body.
    pub blocks: Vec<Block>,
}

/// A comment's author, date and thread state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CommentMeta {
    /// `w:author`.
    pub author: String,
    /// `w:initials`.
    pub initials: Option<String>,
    /// `w:date`, ISO 8601.
    pub date: Option<String>,
    /// The comment this one replies to.
    pub parent: Option<NodeId>,
    /// Resolved ("done").
    pub resolved: bool,
}

/// Which story a node lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Story {
    /// The main body, section by index.
    Body(usize),
    /// A header or footer by index in [`Document::headers_footers`].
    HeaderFooter(usize),
    /// A note by index in [`Document::notes`].
    Note(usize),
    /// A comment by index in [`Document::comments`].
    Comment(usize),
}

/// One step of a path from a story to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Step {
    /// A block of the current block list (or of a content control / field result).
    Block(usize),
    /// A row of the current table.
    Row(usize),
    /// A cell of the current row.
    Cell(usize),
    /// The text box of the k-th object of the current paragraph.
    Object(usize),
}

/// The kind of node an id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// A section.
    Section,
    /// A header or footer.
    HeaderFooter,
    /// A note.
    Note,
    /// A comment.
    Comment,
    /// A paragraph.
    Paragraph,
    /// A table.
    Table,
    /// A row.
    Row,
    /// A cell.
    Cell,
    /// A content control.
    Sdt,
    /// A field result block.
    FieldBlock,
    /// An opaque block.
    Opaque,
    /// A drawing.
    Drawing,
}

/// Where a node is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    pub story: Story,
    pub steps: Vec<Step>,
    pub kind: NodeKind,
}

// ------------------------------------------------------------------ document

/// A Write document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Body sections in order (at least one).
    pub sections: Vec<Section>,
    /// Header and footer stories.
    pub headers_footers: Vec<HeaderFooter>,
    /// Footnote and endnote stories.
    pub notes: Vec<Note>,
    /// Comments in creation order.
    pub comments: Vec<Comment>,
    /// Styles.
    pub styles: StyleSheet,
    /// Lists.
    pub numbering: Numbering,
    /// Settings.
    pub settings: Settings,
    /// Properties.
    pub properties: DocProperties,
    /// The theme formatting resolves against.
    pub theme: DocTheme,
    /// Embedded media by part name.
    pub media: BTreeMap<String, Media>,
    next_id: u64,
    /// Where every node is, rebuilt on demand (see [`NodeIndex`]).
    #[serde(skip)]
    index: NodeIndex,
}

/// A cache of node locations: `locate` answers from it after checking that the cached path
/// still leads to the node, and rebuilds it with one walk of the document when the path is stale
/// or the node unknown. Edits that move nodes therefore cost one walk at the next lookup, and
/// lookups between them (typing, formatting) cost a path check instead of a walk (WP-64: the
/// 300-page document's keystrokes did dozens of whole-document walks).
#[derive(Default)]
pub(crate) struct NodeIndex(std::sync::Mutex<Option<std::collections::HashMap<NodeId, Location>>>);

impl Clone for NodeIndex {
    /// A copy starts without a cache (it is rebuilt on its first lookup).
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for NodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NodeIndex")
    }
}

impl PartialEq for Document {
    /// Two documents are equal when their content is; the id allocator is bookkeeping.
    fn eq(&self, other: &Self) -> bool {
        self.sections == other.sections
            && self.headers_footers == other.headers_footers
            && self.notes == other.notes
            && self.comments == other.comments
            && self.styles == other.styles
            && self.numbering == other.numbering
            && self.settings == other.settings
            && self.properties == other.properties
            && self.theme == other.theme
            && self.media == other.media
    }
}

impl Eq for Document {}

/// Immutable visit of one node during a walk.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Visit<'a> {
    Block(&'a Block),
    Row(&'a Row),
    Cell(&'a Cell),
    Drawing(&'a Drawing),
}

/// Walks blocks depth first, recording the path.
fn walk_blocks<'a, B>(
    blocks: &'a [Block],
    path: &mut Vec<Step>,
    f: &mut impl FnMut(Visit<'a>, &[Step]) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for (i, block) in blocks.iter().enumerate() {
        path.push(Step::Block(i));
        f(Visit::Block(block), path)?;
        match block {
            Block::Paragraph(p) => {
                for (k, o) in p.content.objects().iter().enumerate() {
                    if let InlineObject::Drawing(d) = o {
                        path.push(Step::Object(k));
                        f(Visit::Drawing(d), path)?;
                        if let Some(tb) = &d.text_box {
                            walk_blocks(tb, path, f)?;
                        }
                        path.pop();
                    }
                }
            }
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    path.push(Step::Row(r));
                    f(Visit::Row(row), path)?;
                    for (c, cell) in row.cells.iter().enumerate() {
                        path.push(Step::Cell(c));
                        f(Visit::Cell(cell), path)?;
                        walk_blocks(&cell.blocks, path, f)?;
                        path.pop();
                    }
                    path.pop();
                }
            }
            Block::Sdt(s) => walk_blocks(&s.blocks, path, f)?,
            Block::Field(fb) => walk_blocks(&fb.blocks, path, f)?,
            Block::Opaque(_) => {}
        }
        path.pop();
    }
    ControlFlow::Continue(())
}

/// A mutable cursor while following a path.
enum Cur<'a> {
    Blocks(&'a mut Vec<Block>),
    Block(&'a mut Block),
    Row(&'a mut Row),
    Cell(&'a mut Cell),
}

fn step<'a>(cur: Cur<'a>, s: Step) -> Option<Cur<'a>> {
    match (cur, s) {
        (Cur::Blocks(v), Step::Block(i)) => v.get_mut(i).map(Cur::Block),
        (Cur::Cell(c), Step::Block(i)) => c.blocks.get_mut(i).map(Cur::Block),
        (Cur::Block(b), Step::Block(i)) => b.children_mut()?.get_mut(i).map(Cur::Block),
        (Cur::Block(Block::Table(t)), Step::Row(r)) => t.rows.get_mut(r).map(Cur::Row),
        (Cur::Row(row), Step::Cell(c)) => row.cells.get_mut(c).map(Cur::Cell),
        (Cur::Block(Block::Paragraph(p)), Step::Object(k)) => p
            .content
            .objects_mut()
            .get_mut(k)
            .and_then(InlineObject::blocks_mut)
            .map(Cur::Blocks),
        _ => None,
    }
}

/// The ids that marks and objects may refer to.
#[derive(Debug, Default)]
pub(crate) struct RefTargets {
    pub comments: HashSet<NodeId>,
    pub notes: HashSet<NodeId>,
    pub headers_footers: HashSet<NodeId>,
}

fn check_visit(v: Visit<'_>, refs: &RefTargets) -> Option<String> {
    match v {
        Visit::Block(Block::Paragraph(p)) => p
            .content
            .validate()
            .err()
            .map(|e| format!("paragraph {}: {e}", p.id))
            .or_else(|| {
                p.content.marks().iter().find_map(|m| match m.mark {
                    crate::Mark::Comment(c) if !refs.comments.contains(&c) => {
                        Some(format!("paragraph {} marks missing comment {c}", p.id))
                    }
                    _ => None,
                })
            })
            .or_else(|| {
                p.content.objects().iter().find_map(|o| match o {
                    InlineObject::NoteRef(n) if !refs.notes.contains(&n.note) => Some(format!(
                        "paragraph {} refers to missing note {}",
                        p.id, n.note
                    )),
                    InlineObject::Drawing(d) if d.text_box.as_ref().is_some_and(Vec::is_empty) => {
                        Some(format!("text box {} is empty", d.id))
                    }
                    _ => None,
                })
            }),
        Visit::Block(Block::Table(t)) if t.rows.is_empty() => {
            Some(format!("table {} has no rows", t.id))
        }
        Visit::Block(b) if b.children().is_some_and(Vec::is_empty) => {
            Some(format!("block {} is empty", b.id()))
        }
        Visit::Row(r) if r.cells.is_empty() => Some(format!("row {} has no cells", r.id)),
        Visit::Cell(c) if c.blocks.is_empty() => Some(format!("cell {} is empty", c.id)),
        Visit::Cell(c) if c.grid_span == 0 => Some(format!("cell {} spans 0 columns", c.id)),
        _ => None,
    }
}

/// Checks the structure of blocks (non-empty containers, valid paragraph content, references to
/// existing comments and notes).
pub(crate) fn check_blocks(blocks: &[Block], refs: &RefTargets) -> Result<(), DocError> {
    let mut path = Vec::new();
    match walk_blocks(blocks, &mut path, &mut |v, _| match check_visit(v, refs) {
        Some(e) => ControlFlow::Break(e),
        None => ControlFlow::Continue(()),
    }) {
        ControlFlow::Break(e) => Err(DocError::Corrupt(e)),
        ControlFlow::Continue(()) => Ok(()),
    }
}

/// Every id inside some blocks (including rows, cells and drawings).
pub(crate) fn ids_in_blocks(blocks: &[Block], out: &mut Vec<NodeId>) {
    let mut path = Vec::new();
    let _ = walk_blocks::<()>(blocks, &mut path, &mut |v, _| {
        out.push(match v {
            Visit::Block(b) => b.id(),
            Visit::Row(r) => r.id,
            Visit::Cell(c) => c.id,
            Visit::Drawing(d) => d.id,
        });
        ControlFlow::Continue(())
    });
}

impl Document {
    /// An empty shell: no sections (not valid until one is added). Readers start here; editors
    /// use [`Document::new`].
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sections: Vec::new(),
            headers_footers: Vec::new(),
            notes: Vec::new(),
            comments: Vec::new(),
            styles: StyleSheet::default(),
            numbering: Numbering::default(),
            settings: Settings::default(),
            properties: DocProperties::default(),
            theme: DocTheme::office(),
            media: BTreeMap::new(),
            next_id: 1,
            index: NodeIndex::default(),
        }
    }

    /// A fresh id, never handed out before in this document.
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id.min(NodeId::MAX.0));
        if self.next_id < NodeId::MAX.0 {
            self.next_id += 1;
        }
        id
    }

    /// Moves the allocator past `id`.
    pub(crate) fn ensure_ids_above(&mut self, id: NodeId) {
        if id.0 >= self.next_id {
            self.next_id = id.0.saturating_add(1).min(NodeId::MAX.0);
        }
    }

    /// The next id [`Self::alloc_id`] will return.
    #[must_use]
    pub fn peek_next_id(&self) -> NodeId {
        NodeId(self.next_id)
    }

    /// Moves the id allocator past every id in the document. Readers that build nodes with
    /// their own ids call this once at the end.
    pub fn refresh_id_counter(&mut self) {
        let max = self.all_ids().into_iter().map(|i| i.0).max().unwrap_or(0);
        self.next_id = self.next_id.max(max.saturating_add(1)).min(NodeId::MAX.0);
    }

    /// The blocks of a story.
    #[must_use]
    pub fn story_blocks(&self, story: Story) -> Option<&Vec<Block>> {
        match story {
            Story::Body(i) => self.sections.get(i).map(|s| &s.blocks),
            Story::HeaderFooter(i) => self.headers_footers.get(i).map(|s| &s.blocks),
            Story::Note(i) => self.notes.get(i).map(|s| &s.blocks),
            Story::Comment(i) => self.comments.get(i).map(|s| &s.blocks),
        }
    }

    pub(crate) fn story_blocks_mut(&mut self, story: Story) -> Option<&mut Vec<Block>> {
        match story {
            Story::Body(i) => self.sections.get_mut(i).map(|s| &mut s.blocks),
            Story::HeaderFooter(i) => self.headers_footers.get_mut(i).map(|s| &mut s.blocks),
            Story::Note(i) => self.notes.get_mut(i).map(|s| &mut s.blocks),
            Story::Comment(i) => self.comments.get_mut(i).map(|s| &mut s.blocks),
        }
    }

    /// Every story in order: body sections, headers and footers, notes, comments.
    pub fn stories(&self) -> impl Iterator<Item = Story> + '_ {
        (0..self.sections.len())
            .map(Story::Body)
            .chain((0..self.headers_footers.len()).map(Story::HeaderFooter))
            .chain((0..self.notes.len()).map(Story::Note))
            .chain((0..self.comments.len()).map(Story::Comment))
    }

    fn story_id(&self, story: Story) -> Option<(NodeId, NodeKind)> {
        Some(match story {
            Story::Body(i) => (self.sections.get(i)?.id, NodeKind::Section),
            Story::HeaderFooter(i) => (self.headers_footers.get(i)?.id, NodeKind::HeaderFooter),
            Story::Note(i) => (self.notes.get(i)?.id, NodeKind::Note),
            Story::Comment(i) => (self.comments.get(i)?.id, NodeKind::Comment),
        })
    }

    /// Visits every node of every story (stories in [`Self::stories`] order, depth first).
    pub(crate) fn walk<'a, B>(
        &'a self,
        mut f: impl FnMut(Story, Visit<'a>, &[Step]) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let mut path = Vec::new();
        for story in self.stories() {
            if let Some(blocks) = self.story_blocks(story) {
                walk_blocks(blocks, &mut path, &mut |v, p| f(story, v, p))?;
            }
        }
        ControlFlow::Continue(())
    }

    /// Finds a node: from the location cache when its path still leads there, else with one
    /// walk that also rebuilds the cache.
    pub(crate) fn locate(&self, id: NodeId) -> Option<Location> {
        let cached = self
            .index
            .0
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|m| m.get(&id).cloned()));
        if let Some(loc) = cached
            && self.leads_to(&loc, id)
        {
            return Some(loc);
        }
        let all = self.index_all();
        let found = all.get(&id).cloned();
        if let Ok(mut g) = self.index.0.lock() {
            *g = Some(all);
        }
        found
    }

    /// Whether a location still names node `id`.
    fn leads_to(&self, loc: &Location, id: NodeId) -> bool {
        if loc.steps.is_empty() {
            return self.story_id(loc.story).is_some_and(|(sid, _)| sid == id);
        }
        let vid = match self.walk_to(loc.story, &loc.steps) {
            Some(Visit::Block(b)) => b.id(),
            Some(Visit::Row(r)) => r.id,
            Some(Visit::Cell(c)) => c.id,
            Some(Visit::Drawing(d)) => d.id,
            None => return false,
        };
        vid == id
    }

    /// Every node's location (one walk).
    fn index_all(&self) -> std::collections::HashMap<NodeId, Location> {
        let mut out = std::collections::HashMap::new();
        for story in self.stories() {
            if let Some((sid, kind)) = self.story_id(story) {
                out.entry(sid).or_insert(Location {
                    story,
                    steps: Vec::new(),
                    kind,
                });
            }
        }
        let _ = self.walk(|story, v, path| {
            let (vid, kind) = match v {
                Visit::Block(b) => (
                    b.id(),
                    match b {
                        Block::Paragraph(_) => NodeKind::Paragraph,
                        Block::Table(_) => NodeKind::Table,
                        Block::Sdt(_) => NodeKind::Sdt,
                        Block::Field(_) => NodeKind::FieldBlock,
                        Block::Opaque(_) => NodeKind::Opaque,
                    },
                ),
                Visit::Row(r) => (r.id, NodeKind::Row),
                Visit::Cell(c) => (c.id, NodeKind::Cell),
                Visit::Drawing(d) => (d.id, NodeKind::Drawing),
            };
            // The first occurrence wins, as the walk below would find it.
            out.entry(vid).or_insert_with(|| Location {
                story,
                steps: path.to_vec(),
                kind,
            });
            ControlFlow::<()>::Continue(())
        });
        out
    }

    /// Finds a node by walking the document (the cache's reference implementation).
    #[cfg(test)]
    pub(crate) fn locate_by_walking(&self, id: NodeId) -> Option<Location> {
        for story in self.stories() {
            if let Some((sid, kind)) = self.story_id(story)
                && sid == id
            {
                return Some(Location {
                    story,
                    steps: Vec::new(),
                    kind,
                });
            }
        }
        let found = self.walk(|story, v, path| {
            let (vid, kind) = match v {
                Visit::Block(b) => (
                    b.id(),
                    match b {
                        Block::Paragraph(_) => NodeKind::Paragraph,
                        Block::Table(_) => NodeKind::Table,
                        Block::Sdt(_) => NodeKind::Sdt,
                        Block::Field(_) => NodeKind::FieldBlock,
                        Block::Opaque(_) => NodeKind::Opaque,
                    },
                ),
                Visit::Row(r) => (r.id, NodeKind::Row),
                Visit::Cell(c) => (c.id, NodeKind::Cell),
                Visit::Drawing(d) => (d.id, NodeKind::Drawing),
            };
            if vid == id {
                ControlFlow::Break(Location {
                    story,
                    steps: path.to_vec(),
                    kind,
                })
            } else {
                ControlFlow::Continue(())
            }
        });
        match found {
            ControlFlow::Break(l) => Some(l),
            ControlFlow::Continue(()) => None,
        }
    }

    /// What kind of node an id names, if it exists.
    #[must_use]
    pub fn node_kind(&self, id: NodeId) -> Option<NodeKind> {
        self.locate(id).map(|l| l.kind)
    }

    /// The story a node lives in.
    #[must_use]
    pub fn story_of(&self, id: NodeId) -> Option<Story> {
        self.locate(id).map(|l| l.story)
    }

    fn follow(&mut self, story: Story, steps: &[Step]) -> Option<Cur<'_>> {
        let mut cur = Cur::Blocks(self.story_blocks_mut(story)?);
        for s in steps {
            cur = step(cur, *s)?;
        }
        Some(cur)
    }

    /// The block list a container id holds: a section, header/footer, note, comment, cell,
    /// content control, field result or text box.
    pub(crate) fn container_mut(&mut self, id: NodeId) -> Result<&mut Vec<Block>, DocError> {
        let loc = self.locate(id).ok_or(DocError::UnknownNode(id))?;
        match self.follow(loc.story, &loc.steps) {
            Some(Cur::Blocks(v)) => Ok(v),
            Some(Cur::Cell(c)) => Ok(&mut c.blocks),
            Some(Cur::Block(b)) => b.children_mut().ok_or(DocError::NotAContainer(id)),
            _ => Err(DocError::NotAContainer(id)),
        }
    }

    /// The block list a container id holds (read-only).
    ///
    /// # Errors
    ///
    /// [`DocError::UnknownNode`] or [`DocError::NotAContainer`].
    pub fn container(&self, id: NodeId) -> Result<&Vec<Block>, DocError> {
        let loc = self.locate(id).ok_or(DocError::UnknownNode(id))?;
        let found = self.walk_to(loc.story, &loc.steps);
        match found {
            Some(Visit::Block(b)) => b.children().ok_or(DocError::NotAContainer(id)),
            Some(Visit::Cell(c)) => Ok(&c.blocks),
            Some(Visit::Drawing(d)) => d.text_box.as_ref().ok_or(DocError::NotAContainer(id)),
            Some(Visit::Row(_)) => Err(DocError::NotAContainer(id)),
            None => self
                .story_blocks(loc.story)
                .ok_or(DocError::UnknownNode(id)),
        }
    }

    fn walk_to(&self, story: Story, steps: &[Step]) -> Option<Visit<'_>> {
        if steps.is_empty() {
            return None;
        }
        let mut blocks: &[Block] = self.story_blocks(story)?;
        let mut cur: Option<Visit<'_>> = None;
        for s in steps {
            cur = Some(match (cur, *s) {
                (None, Step::Block(i)) => Visit::Block(blocks.get(i)?),
                (Some(Visit::Cell(c)), Step::Block(i)) => Visit::Block(c.blocks.get(i)?),
                (Some(Visit::Drawing(_)), Step::Block(i)) => Visit::Block(blocks.get(i)?),
                (Some(Visit::Block(b)), Step::Block(i)) => Visit::Block(b.children()?.get(i)?),
                (Some(Visit::Block(Block::Table(t))), Step::Row(r)) => Visit::Row(t.rows.get(r)?),
                (Some(Visit::Row(row)), Step::Cell(c)) => Visit::Cell(row.cells.get(c)?),
                (Some(Visit::Block(Block::Paragraph(p))), Step::Object(k)) => {
                    match p.content.objects().get(k)? {
                        InlineObject::Drawing(d) => {
                            blocks = d.text_box.as_deref().unwrap_or(&[]);
                            Visit::Drawing(d)
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            });
        }
        cur
    }

    /// Splits a block path into (container path, index).
    fn parent_and_index(steps: &[Step]) -> Option<(&[Step], usize)> {
        match steps.split_last()? {
            (Step::Block(i), parent) => Some((parent, *i)),
            _ => None,
        }
    }

    /// The block list containing a block, and the block's index in it.
    pub(crate) fn block_slot_mut(
        &mut self,
        id: NodeId,
    ) -> Result<(&mut Vec<Block>, usize), DocError> {
        let loc = self.locate(id).ok_or(DocError::UnknownNode(id))?;
        let (parent, index) =
            Self::parent_and_index(&loc.steps).ok_or(DocError::UnknownNode(id))?;
        let blocks = match self.follow(loc.story, parent) {
            Some(Cur::Blocks(v)) => v,
            Some(Cur::Cell(c)) => &mut c.blocks,
            Some(Cur::Block(b)) => b.children_mut().ok_or(DocError::UnknownNode(id))?,
            _ => return Err(DocError::UnknownNode(id)),
        };
        Ok((blocks, index))
    }

    /// The container holding a block (section, header/footer, note, comment, cell, content
    /// control, field result or text box) and the block's index in it.
    #[must_use]
    pub fn parent_of(&self, id: NodeId) -> Option<(NodeId, usize)> {
        let loc = self.locate(id)?;
        let (parent, index) = Self::parent_and_index(&loc.steps)?;
        let container = match self.walk_to(loc.story, parent) {
            None => self.story_id(loc.story)?.0,
            Some(Visit::Block(b)) => b.id(),
            Some(Visit::Cell(c)) => c.id,
            Some(Visit::Drawing(d)) => d.id,
            Some(Visit::Row(_)) => return None,
        };
        Some((container, index))
    }

    /// A paragraph by id.
    #[must_use]
    pub fn paragraph(&self, id: NodeId) -> Option<&Paragraph> {
        let loc = self.locate(id)?;
        match self.walk_to(loc.story, &loc.steps)? {
            Visit::Block(Block::Paragraph(p)) => Some(p),
            _ => None,
        }
    }

    pub(crate) fn paragraph_mut(&mut self, id: NodeId) -> Result<&mut Paragraph, DocError> {
        let (blocks, i) = self.block_slot_mut(id)?;
        match blocks.get_mut(i) {
            Some(Block::Paragraph(p)) => Ok(p),
            _ => Err(DocError::WrongKind {
                id,
                expected: "paragraph",
            }),
        }
    }

    /// A table by id.
    #[must_use]
    pub fn table(&self, id: NodeId) -> Option<&Table> {
        let loc = self.locate(id)?;
        match self.walk_to(loc.story, &loc.steps)? {
            Visit::Block(Block::Table(t)) => Some(t),
            _ => None,
        }
    }

    pub(crate) fn table_mut(&mut self, id: NodeId) -> Result<&mut Table, DocError> {
        let (blocks, i) = self.block_slot_mut(id)?;
        match blocks.get_mut(i) {
            Some(Block::Table(t)) => Ok(t),
            _ => Err(DocError::WrongKind {
                id,
                expected: "table",
            }),
        }
    }

    /// A row by id, with the table that holds it.
    #[must_use]
    pub fn row(&self, id: NodeId) -> Option<(&Table, &Row)> {
        let loc = self.locate(id)?;
        let (Step::Row(r), parent) = loc.steps.split_last()? else {
            return None;
        };
        match self.walk_to(loc.story, parent)? {
            Visit::Block(Block::Table(t)) => Some((t, t.rows.get(*r)?)),
            _ => None,
        }
    }

    pub(crate) fn row_mut(&mut self, id: NodeId) -> Result<&mut Row, DocError> {
        let loc = self.locate(id).ok_or(DocError::UnknownNode(id))?;
        match self.follow(loc.story, &loc.steps) {
            Some(Cur::Row(r)) => Ok(r),
            _ => Err(DocError::WrongKind {
                id,
                expected: "row",
            }),
        }
    }

    /// A cell by id.
    #[must_use]
    pub fn cell(&self, id: NodeId) -> Option<&Cell> {
        let loc = self.locate(id)?;
        match self.walk_to(loc.story, &loc.steps)? {
            Visit::Cell(c) => Some(c),
            _ => None,
        }
    }

    pub(crate) fn cell_mut(&mut self, id: NodeId) -> Result<&mut Cell, DocError> {
        let loc = self.locate(id).ok_or(DocError::UnknownNode(id))?;
        match self.follow(loc.story, &loc.steps) {
            Some(Cur::Cell(c)) => Ok(c),
            _ => Err(DocError::WrongKind {
                id,
                expected: "cell",
            }),
        }
    }

    /// The table cell context of a paragraph: the innermost table, row index and grid column
    /// of the cell holding it.
    #[must_use]
    pub fn enclosing_cell(&self, para: NodeId) -> Option<(&Table, usize, usize)> {
        let loc = self.locate(para)?;
        // The innermost Row/Cell pair on the path.
        let pos = loc
            .steps
            .windows(2)
            .rposition(|w| matches!(w, [Step::Row(_), Step::Cell(_)]))?;
        let (Step::Row(r), Step::Cell(c)) = (loc.steps[pos], loc.steps[pos + 1]) else {
            return None;
        };
        // A text box inside the cell is its own story, outside the table's formatting.
        if loc.steps[pos + 2..]
            .iter()
            .any(|s| matches!(s, Step::Object(_)))
        {
            return None;
        }
        let table = match self.walk_to(loc.story, &loc.steps[..pos])? {
            Visit::Block(Block::Table(t)) => t,
            _ => return None,
        };
        let col = table
            .cell_map()
            .into_iter()
            .find(|s| s.row == r && s.cell == c)
            .map_or(0, |s| s.col);
        Some((table, r, col))
    }

    /// A section by id, with its index.
    #[must_use]
    pub fn section(&self, id: NodeId) -> Option<(usize, &Section)> {
        self.sections.iter().enumerate().find(|(_, s)| s.id == id)
    }

    /// The section a body node belongs to.
    #[must_use]
    pub fn section_of(&self, id: NodeId) -> Option<&Section> {
        match self.story_of(id)? {
            Story::Body(i) => self.sections.get(i),
            _ => None,
        }
    }

    /// Every id in the document.
    #[must_use]
    pub fn all_ids(&self) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = self
            .stories()
            .filter_map(|s| self.story_id(s).map(|(id, _)| id))
            .collect();
        let _ = self.walk::<()>(|_, v, _| {
            ids.push(match v {
                Visit::Block(b) => b.id(),
                Visit::Row(r) => r.id,
                Visit::Cell(c) => c.id,
                Visit::Drawing(d) => d.id,
            });
            ControlFlow::Continue(())
        });
        ids
    }

    pub(crate) fn reference_targets(&self) -> RefTargets {
        RefTargets {
            comments: self.comments.iter().map(|c| c.id).collect(),
            notes: self.notes.iter().map(|n| n.id).collect(),
            headers_footers: self.headers_footers.iter().map(|h| h.id).collect(),
        }
    }

    /// Paragraphs of a story in document order (including those in tables, content controls,
    /// field results and text boxes).
    #[must_use]
    pub fn paragraphs_in(&self, story: Story) -> Vec<&Paragraph> {
        let mut out = Vec::new();
        if let Some(blocks) = self.story_blocks(story) {
            let mut path = Vec::new();
            let _ = walk_blocks::<()>(blocks, &mut path, &mut |v, _| {
                if let Visit::Block(Block::Paragraph(p)) = v {
                    out.push(p);
                }
                ControlFlow::Continue(())
            });
        }
        out
    }

    /// Body paragraphs in document order.
    #[must_use]
    pub fn body_paragraphs(&self) -> Vec<&Paragraph> {
        (0..self.sections.len())
            .flat_map(|i| self.paragraphs_in(Story::Body(i)))
            .collect()
    }

    /// Checks every structural invariant: at least one section; non-empty block containers,
    /// tables, rows; unique ids below the allocator; valid paragraph content; section header
    /// references and comment marks pointing at existing stories; spans of at least 1.
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] describing the first violation.
    pub fn validate(&self) -> Result<(), DocError> {
        let corrupt = |m: String| Err(DocError::Corrupt(m));
        if self.sections.is_empty() {
            return corrupt("a document needs at least one section".into());
        }
        let ids = self.all_ids();
        let mut seen = HashSet::with_capacity(ids.len());
        for id in &ids {
            if !seen.insert(*id) {
                return corrupt(format!("duplicate id {id}"));
            }
            if id.0 >= self.next_id {
                return corrupt(format!("{id} is not below the id allocator"));
            }
        }
        for story in self.stories() {
            if self.story_blocks(story).is_some_and(Vec::is_empty) {
                return corrupt(format!("{story:?} has no blocks"));
            }
        }
        let refs = self.reference_targets();
        for s in &self.sections {
            if let Some(bad) = s
                .headers
                .ids()
                .chain(s.footers.ids())
                .find(|i| !refs.headers_footers.contains(i))
            {
                return corrupt(format!(
                    "section {} refers to missing header/footer {bad}",
                    s.id
                ));
            }
        }
        for c in &self.comments {
            if let Some(p) = c.meta.parent
                && !refs.comments.contains(&p)
            {
                return corrupt(format!("comment {} replies to missing {p}", c.id));
            }
        }
        for story in self.stories() {
            if let Some(blocks) = self.story_blocks(story) {
                check_blocks(blocks, &refs)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::Fragment;
    use crate::section::Paper;

    fn para(doc: &mut Document, text: &str) -> Result<Block, DocError> {
        let mut p = Paragraph::new(doc.alloc_id());
        p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default()))?;
        Ok(Block::Paragraph(p))
    }

    fn sample() -> Result<(Document, NodeId, NodeId, NodeId), DocError> {
        let mut doc = Document::empty();
        let p1 = para(&mut doc, "one")?;
        let inner = para(&mut doc, "in cell")?;
        let inner_id = inner.id();
        let cell = Cell {
            id: doc.alloc_id(),
            props: TableCellProps::default(),
            grid_span: 1,
            v_merge: None,
            blocks: vec![inner],
        };
        let cell2 = Cell {
            id: doc.alloc_id(),
            props: TableCellProps::default(),
            grid_span: 2,
            v_merge: None,
            blocks: vec![para(&mut doc, "wide")?],
        };
        let row = Row {
            id: doc.alloc_id(),
            props: TableRowProps::default(),
            revision: None,
            cells: vec![cell, cell2],
        };
        let table = Table {
            id: doc.alloc_id(),
            props: TableProps::default(),
            grid: vec![3000, 3000, 3000],
            rows: vec![row],
        };
        let table_id = table.id;
        let mut tb_para = Paragraph::new(doc.alloc_id());
        tb_para.content = Inlines::from_fragment(Fragment::text("boxed", RunProps::default()))?;
        let tb_id = tb_para.id;
        let mut drawing = Drawing::picture(doc.alloc_id(), "word/media/image1.png", 100, 100);
        drawing.text_box = Some(vec![Block::Paragraph(tb_para)]);
        let mut with_box = Paragraph::new(doc.alloc_id());
        with_box.content = Inlines::from_fragment(Fragment::object(
            InlineObject::Drawing(Box::new(drawing)),
            RunProps::default(),
        ))?;
        let sdt = SdtBlock {
            id: doc.alloc_id(),
            info: SdtInfo::default(),
            blocks: vec![Block::Paragraph(with_box)],
        };
        let section = Section {
            id: doc.alloc_id(),
            props: SectionProps::new(Paper::Letter),
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
            blocks: vec![p1, Block::Table(table), Block::Sdt(sdt)],
        };
        doc.sections.push(section);
        Ok((doc, inner_id, table_id, tb_id))
    }

    #[test]
    fn the_location_cache_agrees_with_walking_through_edits_and_direct_changes()
    -> Result<(), DocError> {
        let (mut doc, inner, table, tb) = sample()?;
        let check = |doc: &Document| {
            for id in doc.all_ids() {
                assert_eq!(doc.locate(id), doc.locate_by_walking(id), "{id}");
            }
            assert_eq!(doc.locate(NodeId(987_654)), None);
        };
        check(&doc);
        // Moving nodes (a paragraph inserted before everything) makes cached paths stale.
        let body = doc.sections[0].id;
        let fresh = para(&mut doc, "first")?;
        let fresh_id = fresh.id();
        doc.apply(crate::Transaction::new(vec![crate::Op::InsertBlocks {
            container: body,
            index: 0,
            blocks: vec![fresh],
        }]))?;
        check(&doc);
        assert_eq!(
            doc.paragraph(fresh_id).map(Paragraph::text).as_deref(),
            Some("first")
        );
        assert!(doc.paragraph(inner).is_some() && doc.table(table).is_some());
        // Direct changes to the public fields are seen too.
        let direct = para(&mut doc, "direct")?;
        let direct_id = direct.id();
        doc.sections[0].blocks.insert(0, direct);
        check(&doc);
        assert_eq!(
            doc.paragraph(direct_id).map(Paragraph::text).as_deref(),
            Some("direct")
        );
        assert_eq!(
            doc.paragraph(tb).map(Paragraph::text).as_deref(),
            Some("boxed")
        );
        // A copy rebuilds its own cache.
        let copy = doc.clone();
        check(&copy);
        Ok(())
    }

    #[test]
    fn locates_nodes_at_any_depth() -> Result<(), DocError> {
        let (mut doc, inner, table, tb) = sample()?;
        doc.validate()?;
        assert_eq!(
            doc.paragraph(inner).map(Paragraph::text).as_deref(),
            Some("in cell")
        );
        assert_eq!(
            doc.paragraph(tb).map(Paragraph::text).as_deref(),
            Some("boxed")
        );
        assert_eq!(doc.node_kind(table), Some(NodeKind::Table));
        assert_eq!(doc.table(table).map(|t| t.rows.len()), Some(1));
        assert!(doc.paragraph(table).is_none());
        assert_eq!(doc.story_of(inner), Some(Story::Body(0)));
        let (t, r, c) = doc
            .enclosing_cell(inner)
            .ok_or(DocError::UnknownNode(inner))?;
        assert_eq!((t.id, r, c), (table, 0, 0));
        assert!(doc.enclosing_cell(tb).is_none());
        doc.paragraph_mut(tb)?.props.keep_next = Some(true);
        assert_eq!(
            doc.paragraph(tb).and_then(|p| p.props.keep_next),
            Some(true)
        );
        let section_id = doc.sections[0].id;
        assert_eq!(doc.container(section_id)?.len(), 3);
        assert!(doc.container_mut(inner).is_err());
        assert!(matches!(
            doc.paragraph_mut(NodeId(999)),
            Err(DocError::UnknownNode(_))
        ));
        assert_eq!(doc.body_paragraphs().len(), 5);
        assert_eq!(doc.section_of(inner).map(|s| s.id), Some(section_id));
        assert_eq!(doc.section(section_id).map(|(i, _)| i), Some(0));
        Ok(())
    }

    #[test]
    fn rows_cells_and_cell_map() -> Result<(), DocError> {
        let (mut doc, _, table, _) = sample()?;
        let t = doc
            .table(table)
            .cloned()
            .ok_or(DocError::UnknownNode(table))?;
        let row_id = t.rows[0].id;
        let cell_id = t.rows[0].cells[1].id;
        assert_eq!(doc.row(row_id).map(|(tb, _)| tb.id), Some(table));
        assert_eq!(doc.cell(cell_id).map(|c| c.grid_span), Some(2));
        doc.row_mut(row_id)?.props.header = Some(true);
        doc.cell_mut(cell_id)?.props.no_wrap = Some(true);
        assert!(doc.cell_mut(row_id).is_err());
        assert!(doc.row_mut(cell_id).is_err());
        assert!(doc.table_mut(row_id).is_err());
        let t = doc.table(table).ok_or(DocError::UnknownNode(table))?;
        assert_eq!(t.header_rows(), 1);
        assert_eq!(t.grid_width(), 3);
        let map = t.cell_map();
        assert_eq!((map[1].col, map[1].col_span), (1, 2));
        assert_eq!(doc.container(cell_id)?.len(), 1);
        Ok(())
    }

    #[test]
    fn vertical_merges_extend_the_top_cell() {
        let mk = |id: u64, v: Option<VMerge>| Cell {
            id: NodeId(id),
            props: TableCellProps::default(),
            grid_span: 1,
            v_merge: v,
            blocks: Vec::new(),
        };
        let row = |id: u64, cells: Vec<Cell>| Row {
            id: NodeId(id),
            props: TableRowProps::default(),
            revision: None,
            cells,
        };
        let t = Table {
            id: NodeId(1),
            props: TableProps::default(),
            grid: vec![1, 1],
            rows: vec![
                row(2, vec![mk(3, Some(VMerge::Restart)), mk(4, None)]),
                row(5, vec![mk(6, Some(VMerge::Continue)), mk(7, None)]),
                row(8, vec![mk(9, Some(VMerge::Continue)), mk(10, None)]),
            ],
        };
        let map = t.cell_map();
        assert_eq!(map[0].row_span, 3);
        assert_eq!(map[2].row_span, 0);
        assert_eq!(map[1].row_span, 1);
    }

    #[test]
    fn validate_reports_broken_documents() -> Result<(), DocError> {
        assert!(Document::empty().validate().is_err());
        let (doc, inner, _, _) = sample()?;
        let mut dup = doc.clone();
        if let Some(Block::Paragraph(p)) = dup.sections[0].blocks.first_mut() {
            p.id = inner;
        }
        assert!(dup.validate().is_err());
        let mut high = doc.clone();
        high.next_id = 2;
        assert!(high.validate().is_err());
        high.refresh_id_counter();
        high.validate()?;
        let mut bad_ref = doc.clone();
        bad_ref.sections[0].headers.default = Some(NodeId(4242));
        assert!(bad_ref.validate().is_err());
        let mut empty_section = doc;
        empty_section.sections[0].blocks.clear();
        assert!(empty_section.validate().is_err());
        Ok(())
    }

    #[test]
    fn ids_are_never_reissued() {
        let mut doc = Document::empty();
        let a = doc.alloc_id();
        let b = doc.alloc_id();
        assert!(b.0 > a.0);
        assert_eq!(doc.peek_next_id(), NodeId(b.0 + 1));
        doc.next_id = NodeId::MAX.0;
        assert_eq!(doc.alloc_id(), NodeId::MAX);
        assert_eq!(doc.alloc_id(), NodeId::MAX);
    }
}
