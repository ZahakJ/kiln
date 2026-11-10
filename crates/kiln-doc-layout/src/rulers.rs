// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ruler data for the caret's page and paragraph: margins, columns, indents and tab stops on
//! the horizontal ruler, margins and the text area on the vertical one (all in document px; the
//! UI draws them, Word-style).

use kiln_doc::{Document, Pos};

use crate::engine::DocLayout;
use crate::geometry::caret_in;

/// A tab stop on the ruler.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulerTab {
    /// Position from the text area's start edge, px.
    pub pos: f32,
    /// `left`, `center`, `right`, `decimal` or `bar`.
    pub kind: &'static str,
}

/// What the rulers show.
#[derive(Debug, Clone, PartialEq)]
pub struct Rulers {
    /// Page index.
    pub page: usize,
    /// The page rectangle (document coordinates).
    pub page_rect: kiln_scene::Bounds,
    /// The text area (document coordinates).
    pub text_rect: kiln_scene::Bounds,
    /// Columns (x, width) in document coordinates.
    pub columns: Vec<(f32, f32)>,
    /// Paragraph left indent from the text area's start edge, px.
    pub indent_left: f32,
    /// Paragraph right indent from the text area's end edge, px.
    pub indent_right: f32,
    /// First-line indent relative to the left indent (negative: hanging), px.
    pub first_line: f32,
    /// Custom tab stops.
    pub tabs: Vec<RulerTab>,
    /// Interval of the default tab stops, px.
    pub default_tab: f32,
}

/// The rulers at a caret position.
#[must_use]
pub fn rulers(layout: &DocLayout, doc: &Document, pos: Pos) -> Option<Rulers> {
    let caret = caret_in(layout, pos)?;
    let page = layout.pages().get(caret.page)?;
    let page_rect = layout.page_rect(caret.page)?;
    let line = layout.line(caret.line)?;
    let o = (page_rect.x, page_rect.y);
    let body = page.body.translate(o.0, o.1);
    let p = doc.paragraph(pos.para)?;
    let r = kiln_doc::resolve::resolve_paragraph(
        &doc.context(),
        &p.props,
        doc.table_context(p.id).as_ref(),
    );
    let spec = &line.para.src.spec.props;
    let tabs = r
        .tabs
        .iter()
        .filter(|t| t.align != kiln_doc::st::TabAlignment::Clear)
        .map(|t| RulerTab {
            pos: crate::units::twips(t.pos),
            kind: match t.align {
                kiln_doc::st::TabAlignment::Center => "center",
                kiln_doc::st::TabAlignment::Right => "right",
                kiln_doc::st::TabAlignment::Decimal => "decimal",
                kiln_doc::st::TabAlignment::Bar => "bar",
                _ => "left",
            },
        })
        .collect();
    // In a table cell the ruler measures from the cell's text area.
    let text_rect = kiln_scene::Bounds::new(o.0 + line.x, body.y, line.para.width, body.h);
    Some(Rulers {
        page: caret.page,
        page_rect,
        text_rect,
        columns: page.columns.iter().map(|(x, w)| (x + o.0, *w)).collect(),
        indent_left: spec.indent_start,
        indent_right: spec.indent_end,
        first_line: spec.first_line,
        tabs,
        default_tab: spec.default_tab,
    })
}
