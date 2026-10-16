// SPDX-License-Identifier: AGPL-3.0-or-later
//! The layout context: caches that survive between layouts (shaped paragraphs, resolved styles,
//! decoded pictures) and the stacking of blocks into boxes (table cells, headers and footers,
//! notes, text boxes).

use std::collections::HashMap;
use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::numbering::ListLabel;
use kiln_doc::resolve::TableCellContext;
use kiln_doc::{Block, Document, Paragraph};
use kiln_text::{Affinity, TextEngine};
use rustc_hash::{FxHashMap, FxHasher};

use crate::media::MediaCache;
use crate::page::{DrawingItem, Item, Layer, LineItem, StoryKind};
use crate::para::ParaBox;
use crate::source::{self, BuildCx, FieldEnv, ParaSource, StyleCache};
use crate::style::ParaFormat;

/// What a layout needs to know besides the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Type scale (1.0; Read Mode uses a larger one).
    pub scale: f32,
    /// Show tracked changes as markup (insertions underlined, deletions struck through).
    pub markup: bool,
    /// The clock for DATE and TIME fields.
    pub now: Option<kiln_doc::fields::FieldDateTime>,
    /// The file name for FILENAME fields.
    pub file_name: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: 1.0,
            markup: true,
            now: None,
            file_name: None,
        }
    }
}

/// A cached paragraph layout.
#[derive(Debug, Clone)]
struct CachedPara {
    key: u64,
    para: Arc<ParaBox>,
}

/// Caches kept between layouts.
#[derive(Debug)]
pub struct Caches {
    /// The text engine (fonts, shaping cache).
    pub engine: TextEngine,
    /// Resolved styles.
    pub styles: StyleCache,
    /// Decoded pictures.
    pub media: MediaCache,
    paras: FxHashMap<NodeId, CachedPara>,
}

impl Caches {
    /// Fresh caches over a text engine.
    #[must_use]
    pub fn new(engine: TextEngine) -> Self {
        Self {
            engine,
            styles: StyleCache::default(),
            media: MediaCache::default(),
            paras: FxHashMap::default(),
        }
    }

    /// Forgets the layout of these paragraphs.
    pub fn invalidate(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.paras.remove(id);
        }
    }

    /// Forgets every cached layout and style (styles, theme, settings or fonts changed).
    pub fn invalidate_all(&mut self) {
        self.paras.clear();
        self.styles.clear();
    }

    /// Paragraph layouts held.
    #[must_use]
    pub fn cached_paragraphs(&self) -> usize {
        self.paras.len()
    }
}

/// A box of stacked content (a table cell, a header, a note): bands that can be split between
/// pages, in box coordinates.
#[derive(Debug, Clone, Default)]
pub struct BoxContent {
    /// Bands in order (a line, or a row of a nested table).
    pub bands: Vec<Band>,
    /// Height.
    pub height: f32,
    /// Floating objects anchored in the box: (paragraph top in box coordinates, drawing item
    /// with an unpositioned rectangle).
    pub anchors: Vec<(f32, DrawingItem)>,
    /// Widest content (for autofit tables).
    pub max_width: f32,
    /// Widest unbreakable piece (for autofit tables).
    pub min_width: f32,
}

/// A horizontal slice of a box.
#[derive(Debug, Clone, Default)]
pub struct Band {
    /// Top in box coordinates.
    pub y: f32,
    /// Height.
    pub h: f32,
    /// Items in box coordinates.
    pub items: Vec<Item>,
}

/// The layout context of one pass over a document.
pub struct Cx<'a> {
    /// The document.
    pub doc: &'a Document,
    /// Caches.
    pub caches: &'a mut Caches,
    /// List labels of body paragraphs.
    pub labels: &'a HashMap<NodeId, ListLabel>,
    /// Displayed note numbers.
    pub note_numbers: &'a FxHashMap<NodeId, String>,
    /// Options.
    pub options: &'a Options,
}

impl std::fmt::Debug for Cx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cx").finish_non_exhaustive()
    }
}

fn hash<T: std::hash::Hash>(v: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = FxHasher::default();
    v.hash(&mut h);
    h.finish()
}

impl Cx<'_> {
    /// The text-engine input of a paragraph.
    pub fn source(
        &mut self,
        para: &Paragraph,
        width: f32,
        table: Option<&TableCellContext>,
        env: &FieldEnv,
    ) -> ParaSource {
        let label = self.labels.get(&para.id);
        source::build(
            BuildCx {
                doc: self.doc,
                table,
                label,
                note_numbers: self.note_numbers,
                fields: env,
                scale: self.options.scale,
                width,
                markup: self.options.markup,
            },
            &mut self.caches.styles,
            para,
        )
    }

    /// The layout of a paragraph without obstacles, cached unless it shows page-dependent
    /// fields.
    pub fn para(
        &mut self,
        para: &Paragraph,
        width: f32,
        table: Option<&TableCellContext>,
        env: &FieldEnv,
    ) -> Arc<ParaBox> {
        let key = hash(&(
            width.to_bits(),
            self.labels.get(&para.id).map(|l| (&l.text, l.level)),
            table.map(|t| (&t.table_style_id, &t.conditions)),
            self.options.scale.to_bits(),
            self.options.markup,
        ));
        if let Some(c) = self.caches.paras.get(&para.id)
            && c.key == key
        {
            return c.para.clone();
        }
        let src = Arc::new(self.source(para, width, table, env));
        let page_fields = src.page_fields;
        let pb = Arc::new(ParaBox::simple(src, &mut self.caches.engine));
        if !page_fields {
            self.caches.paras.insert(
                para.id,
                CachedPara {
                    key,
                    para: pb.clone(),
                },
            );
        }
        pb
    }

    /// Stacks blocks into a box `width` wide (cells, headers, footers, notes, text boxes).
    pub fn blocks_box(
        &mut self,
        blocks: &[Block],
        width: f32,
        table: Option<&TableCellContext>,
        story: StoryKind,
        env: &FieldEnv,
    ) -> BoxContent {
        let mut out = BoxContent::default();
        let mut y = 0.0_f32;
        let mut prev: Option<(String, f32, bool)> = None;
        let mut flat = Vec::new();
        flatten(blocks, &mut flat);
        for block in flat {
            match block {
                Block::Paragraph(p) => {
                    let pb = self.para(p, width, table, env);
                    let f = &pb.src.format;
                    let gap = spacing_gap(prev.as_ref(), f);
                    y += gap + f.border_above();
                    let top = y;
                    let n = pb.lines.len();
                    for i in 0..n {
                        let li = &pb.lines[i];
                        let mut band = Band {
                            y: top + li.top,
                            h: li.height,
                            items: Vec::new(),
                        };
                        decorations(&pb, f, i, 0.0, top, width, &mut band.items);
                        line_items(&pb, i, 0.0, top, story, &mut band.items);
                        out.bands.push(band);
                    }
                    for a in &pb.src.anchors {
                        out.anchors.push((
                            top,
                            DrawingItem {
                                rect: kiln_scene::Bounds::ZERO,
                                drawing: a.drawing.clone(),
                                layer: Layer::Front,
                                para: p.id,
                                offset: a.doc,
                            },
                        ));
                    }
                    let (minw, maxw) = content_widths(&pb);
                    out.min_width = out.min_width.max(minw);
                    out.max_width = out.max_width.max(maxw);
                    y = top + pb.height() + f.border_below();
                    prev = Some((f.style_id.clone(), f.space_after, f.contextual_spacing));
                }
                Block::Table(t) => {
                    let tb = self.table(t, width, env, story);
                    y += 0.0;
                    let top = y;
                    let mut ry = 0.0;
                    for (r, row) in tb.rows.iter().enumerate() {
                        let mut band = Band {
                            y: top + ry,
                            h: row.height,
                            items: Vec::new(),
                        };
                        tb.paint_row(r, tb.x, top + ry, row.height, &mut band.items);
                        out.bands.push(band);
                        ry += row.height;
                    }
                    out.min_width = out.min_width.max(tb.min_width);
                    out.max_width = out.max_width.max(tb.width);
                    y = top + ry;
                    prev = None;
                }
                _ => {}
            }
        }
        // The last paragraph's space after counts inside cells (Word keeps it).
        if let Some((_, after, _)) = prev {
            y += after;
        }
        out.height = y;
        out
    }
}

/// Paragraphs and tables of a block list, looking through content controls and field results.
pub fn flatten<'b>(blocks: &'b [Block], out: &mut Vec<&'b Block>) {
    for b in blocks {
        match b {
            Block::Paragraph(_) | Block::Table(_) => out.push(b),
            Block::Sdt(s) => flatten(&s.blocks, out),
            Block::Field(f) => flatten(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

/// The gap between the previous paragraph (style, space after, contextual) and this one: the
/// larger of the previous paragraph's space after and this one's space before, where contextual
/// spacing drops a paragraph's own spacing toward a neighbour of the same style.
#[must_use]
pub fn spacing_gap(prev: Option<&(String, f32, bool)>, f: &ParaFormat) -> f32 {
    let Some((style, after, contextual)) = prev else {
        return f.space_before;
    };
    let same = *style == f.style_id;
    let before = if f.contextual_spacing && same {
        0.0
    } else {
        f.space_before
    };
    let after = if *contextual && same { 0.0 } else { *after };
    // Word uses the larger of the two (as LibreOffice does for Word documents).
    before.max(after)
}

/// The narrowest and widest the paragraph's content can be, including indents: the widest
/// line and the widest run of non-space clusters. Exact when the paragraph was laid out wide
/// enough not to wrap (autofit tables measure cells that way).
#[must_use]
pub fn content_widths(pb: &ParaBox) -> (f32, f32) {
    let f = &pb.src.format;
    let indents = f.indent_left + f.indent_right;
    let text = pb.src.map.text();
    let mut maxw = 0.0_f32;
    let mut minw = 0.0_f32;
    for l in &pb.lines {
        let Some(piece) = pb.pieces.get(l.piece as usize) else {
            continue;
        };
        let Some(line) = piece.layout.lines.get(l.line as usize) else {
            continue;
        };
        maxw = maxw.max(line.width);
        let mut word: Option<(f32, f32)> = None;
        for c in &line.clusters {
            let s = text
                .get(c.text_range.start + piece.byte_start..c.text_range.end + piece.byte_start)
                .unwrap_or("");
            if s.chars().all(char::is_whitespace) {
                if let Some((a, e)) = word.take() {
                    minw = minw.max(e - a);
                }
            } else {
                word = Some(word.map_or((c.x, c.x + c.advance), |(a, e)| {
                    (a.min(c.x), e.max(c.x + c.advance))
                }));
            }
        }
        if let Some((a, e)) = word {
            minw = minw.max(e - a);
        }
    }
    (minw + indents, maxw + indents + 1.0)
}

/// Shading and borders of the paragraph around line `i` (placed with the paragraph origin at
/// `ox, oy` and a text area `width` wide).
pub fn decorations(
    pb: &ParaBox,
    f: &ParaFormat,
    i: usize,
    ox: f32,
    oy: f32,
    width: f32,
    out: &mut Vec<Item>,
) {
    if !f.has_box() {
        return;
    }
    let Some(li) = pb.lines.get(i) else { return };
    let first = i == 0;
    let last = i + 1 == pb.lines.len();
    let [top, left, bottom, right, _between] = f.borders;
    let pad_l = left.map_or(0.0, |r| r.space + r.width);
    let pad_r = right.map_or(0.0, |r| r.space + r.width);
    let x0 = ox + f.indent_left - pad_l;
    let x1 = ox + width - f.indent_right + pad_r;
    let mut y0 = oy + li.top;
    let mut y1 = oy + li.bottom();
    if first {
        y0 -= top.map_or(0.0, |r| r.space + r.width);
    }
    if last {
        y1 += bottom.map_or(0.0, |r| r.space + r.width);
    }
    if let Some(c) = f.shading {
        out.push(Item::Fill {
            rect: kiln_scene::Bounds::new(x0, y0, x1 - x0, y1 - y0),
            color: c,
        });
    }
    if let (true, Some(r)) = (first, top) {
        let y = y0 + r.width / 2.0;
        out.push(Item::Rule {
            x0,
            y0: y,
            x1,
            y1: y,
            rule: r,
        });
    }
    if let (true, Some(r)) = (last, bottom) {
        let y = y1 - r.width / 2.0;
        out.push(Item::Rule {
            x0,
            y0: y,
            x1,
            y1: y,
            rule: r,
        });
    }
    if let Some(r) = left {
        let x = x0 + r.width / 2.0;
        out.push(Item::Rule {
            x0: x,
            y0,
            x1: x,
            y1,
            rule: r,
        });
    }
    if let Some(r) = right {
        let x = x1 - r.width / 2.0;
        out.push(Item::Rule {
            x0: x,
            y0,
            x1: x,
            y1,
            rule: r,
        });
    }
}

/// The items of line `i`: highlight backgrounds, the text, pictures in the line.
pub fn line_items(
    pb: &Arc<ParaBox>,
    i: usize,
    ox: f32,
    oy: f32,
    story: StoryKind,
    out: &mut Vec<Item>,
) {
    let Some(li) = pb.lines.get(i) else { return };
    let Some(piece) = pb.pieces.get(li.piece as usize) else {
        return;
    };
    let Some(line) = piece.layout.lines.get(li.line as usize) else {
        return;
    };
    let src = &pb.src;
    let px = ox + piece.x;
    let py = oy + piece.y;
    for h in &src.highlights {
        if h.end <= li.bytes.start || h.start >= li.bytes.end {
            continue;
        }
        let a = h.start.max(li.bytes.start) - piece.byte_start;
        let b = h.end.min(li.bytes.end) - piece.byte_start;
        for r in piece.layout.selection_rects(a..b) {
            if (r.y - line.top).abs() > 0.01 {
                continue;
            }
            out.push(Item::Fill {
                rect: kiln_scene::Bounds::new(px + r.x, py + r.y, r.width, r.height),
                color: h.color,
            });
        }
    }
    out.push(Item::Line(LineItem {
        para: pb.clone(),
        line: u32::try_from(i).unwrap_or(u32::MAX),
        x: ox,
        y: oy,
        story,
        fill: src.format.shading,
    }));
    for ib in &src.inline {
        if ib.byte < li.bytes.start || ib.byte >= li.bytes.end {
            continue;
        }
        let local = ib.byte - piece.byte_start;
        let x = line
            .clusters
            .iter()
            .find(|c| c.text_range.start == local)
            .map_or_else(
                || piece.layout.caret_rect_with(local, Affinity::Downstream).x,
                |c| c.x,
            );
        let top = line.baseline - ib.height;
        out.push(Item::Drawing(DrawingItem {
            rect: kiln_scene::Bounds::new(px + x, py + top, ib.width, ib.height),
            drawing: ib.drawing.clone(),
            layer: Layer::Inline,
            para: src.id,
            offset: ib.doc,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(style: &str, before: f32, after: f32, contextual: bool) -> ParaFormat {
        ParaFormat {
            style_id: style.into(),
            space_before: before,
            space_after: after,
            contextual_spacing: contextual,
            keep_next: false,
            keep_lines: false,
            page_break_before: false,
            widow_control: true,
            indent_left: 0.0,
            indent_right: 0.0,
            outline_level: 9,
            bidi: false,
            borders: [None; 5],
            shading: None,
        }
    }

    #[test]
    fn spacing_takes_the_larger_and_contextual_spacing_drops_same_style_gaps() {
        let f = fmt("Normal", 4.0, 10.0, false);
        assert_eq!(spacing_gap(None, &f), 4.0);
        assert_eq!(spacing_gap(Some(&("Normal".into(), 10.0, false)), &f), 10.0);
        let list = fmt("ListParagraph", 4.0, 10.0, true);
        assert_eq!(
            spacing_gap(Some(&("ListParagraph".into(), 10.0, true)), &list),
            0.0
        );
        assert_eq!(
            spacing_gap(Some(&("Normal".into(), 10.0, false)), &list),
            10.0
        );
    }
}
