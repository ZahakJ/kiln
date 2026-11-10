// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers of the layout tests: documents built in code and page rendering.
#![allow(dead_code)]

use kiln_common::NodeId;
use kiln_doc::props::{ParagraphProps, RunProps};
use kiln_doc::{Block, Document, Fragment, Inlines, Paragraph};
use kiln_doc_layout::{DocLayout, DocScene, default_fonts};
use kiln_raster::{PaintOptions, Rgba, View};
use kiln_scene::{DeltaTracker, Palette};

/// A paragraph of `text` in `style` (`None` = Normal) with direct paragraph properties.
pub fn para(
    doc: &mut Document,
    text: &str,
    style: Option<&str>,
    props: Option<ParagraphProps>,
) -> Block {
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    if let Some(pp) = props {
        p.props = pp;
    }
    if let Some(s) = style {
        p.props.style_id = Some(s.to_owned());
    }
    if !text.is_empty() {
        p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default())).unwrap();
    }
    Block::Paragraph(p)
}

/// Replaces the body of the first section.
pub fn set_body(doc: &mut Document, blocks: Vec<Block>) {
    doc.sections[0].blocks = blocks;
}

/// Sentences of filler text (deterministic).
pub fn lorem(words: usize, seed: usize) -> String {
    const W: [&str; 24] = [
        "kiln",
        "glaze",
        "clay",
        "fire",
        "warm",
        "shelf",
        "cone",
        "ash",
        "slip",
        "bisque",
        "wheel",
        "porcelain",
        "stoneware",
        "temper",
        "vessel",
        "kiln-shed",
        "draft",
        "flue",
        "ember",
        "crackle",
        "celadon",
        "tenmoku",
        "shino",
        "wadding",
    ];
    let mut out = String::new();
    for i in 0..words {
        if i > 0 {
            out.push(' ');
        }
        let w = W[(i * 7 + seed * 13 + i / 5) % W.len()];
        if i % 12 == 0 {
            let mut c = w.chars();
            if let Some(f) = c.next() {
                out.extend(f.to_uppercase());
                out.push_str(c.as_str());
            }
        } else {
            out.push_str(w);
        }
        if i % 12 == 11 {
            out.push('.');
        }
    }
    out.push('.');
    out
}

/// A fresh layout of `doc`.
pub fn layout(doc: &Document) -> DocLayout {
    let mut l = DocLayout::new(default_fonts());
    l.layout(doc);
    l
}

/// Renders page `i` at `dpi` (light theme, no caret).
pub fn render_page(layout: &mut DocLayout, doc: &Document, i: usize, dpi: f32, dark: bool) -> Rgba {
    let mut scene = DocScene::new();
    let rect = layout.page_rect(i).expect("page");
    let scale = dpi / 96.0;
    scene.prepare(
        layout,
        doc,
        scale,
        &rect,
        &rect.inflate(1.0, 1.0),
        Vec::new(),
    );
    let mut tracker = DeltaTracker::default();
    let mut delta = tracker.delta(scene.scene(), &rect, &rect.inflate(1.0, 1.0), 1);
    delta.atlas = scene.atlas_updates(true);
    delta.images = scene.image_updates(true, layout);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let view = View {
        width: (rect.w * scale).round() as u32,
        height: (rect.h * scale).round() as u32,
        scroll_x: rect.x,
        scroll_y: rect.y,
        options: PaintOptions {
            caret_visible: false,
            touch: false,
        },
    };
    let palette = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    kiln_raster::render_delta(&delta, &view, &palette).expect("render")
}

/// Ids are allocated by the document; a stable id for assertions.
pub fn id_of(b: &Block) -> NodeId {
    b.id()
}
