// SPDX-License-Identifier: AGPL-3.0-or-later
//! Effective formatting: Word's property inheritance (ECMA-376 §17.7.2), in this order:
//!
//! 1. document defaults (`w:docDefaults`);
//! 2. table style (whole table, then conditional regions: bands, first/last column, header row,
//!    corners);
//! 3. paragraph style chain (`basedOn` root → leaf; the default paragraph style when none is set);
//! 4. numbering level `w:pPr` (paragraphs) / `w:rPr` (list labels only);
//! 5. character style chain (runs);
//! 6. direct formatting (paragraph properties, run properties; the paragraph mark's properties
//!    for list labels).
//!
//! Toggle properties (bold, italic, caps, … §17.7.3) flip when a *character style* turns them on
//! over text that already has them (Emphasis inside the italic Quote style is upright, as in
//! Word). Within one style's `basedOn` chain the nearest value wins; document defaults, the table
//! style, the paragraph style, numbering and direct formatting set values absolutely.

use serde::{Deserialize, Serialize};

use crate::merge::{merge_cell_props, merge_paragraph_props, merge_run_props, merge_table_props};
use crate::model::{Document, Paragraph, Table};
use crate::numbering::Numbering;
use crate::props::{
    Border, ColorValue, DropCap, Lang, ParagraphBorders, ParagraphProps, RunProps, Shading,
    TabStop, TableCellProps, TableLook, TableProps, ToggleProp, Underline,
};
use crate::settings::Settings;
use crate::st::{
    Alignment, HighlightColor, LineRule, StyleType, TableCondition, TextAlignment, VertAlign,
};
use crate::style::{Style, StyleSheet};
use crate::theme::{DocTheme, ResolvedFonts, resolve_color, resolve_run_fonts};

/// Everything resolution needs besides the node itself.
#[derive(Debug, Clone, Copy)]
pub struct DocContext<'a> {
    /// Styles.
    pub styles: &'a StyleSheet,
    /// Lists.
    pub numbering: Option<&'a Numbering>,
    /// The theme.
    pub theme: &'a DocTheme,
    /// Settings (the colour scheme mapping).
    pub settings: Option<&'a Settings>,
}

impl<'a> DocContext<'a> {
    /// A context over a style sheet alone (Office theme, no lists).
    #[must_use]
    pub fn new(styles: &'a StyleSheet, theme: &'a DocTheme) -> Self {
        Self {
            styles,
            numbering: None,
            theme,
            settings: None,
        }
    }

    /// The same context with lists.
    #[must_use]
    pub fn with_numbering(mut self, numbering: &'a Numbering) -> Self {
        self.numbering = Some(numbering);
        self
    }
}

impl Document {
    /// The resolution context of this document.
    #[must_use]
    pub fn context(&self) -> DocContext<'_> {
        DocContext {
            styles: &self.styles,
            numbering: Some(&self.numbering),
            theme: &self.theme,
            settings: Some(&self.settings),
        }
    }

    /// The table-style context of a paragraph inside a table, if it is in one.
    #[must_use]
    pub fn table_context(&self, para: kiln_common::NodeId) -> Option<TableCellContext> {
        let (table, row, col) = self.enclosing_cell(para)?;
        Some(table_cell_context(table, row, col))
    }

    /// The effective properties of a paragraph (including its table context).
    #[must_use]
    pub fn resolve_paragraph(&self, para: &Paragraph) -> ResolvedParagraph {
        let table = self.table_context(para.id);
        resolve_paragraph(&self.context(), &para.props, table.as_ref())
    }

    /// The effective properties of the text at `offset` in a paragraph.
    #[must_use]
    pub fn resolve_run_at(&self, para: &Paragraph, offset: usize) -> ResolvedRun {
        let table = self.table_context(para.id);
        let direct = para.content.props_at(offset).cloned().unwrap_or_default();
        resolve_run(
            &self.context(),
            &RunInput {
                paragraph: &para.props,
                mark_props: &para.mark_props,
                direct: &direct,
                table: table.as_ref(),
                target: RunTarget::Text,
            },
        )
    }
}

// ------------------------------------------------------------------ tables

/// Where a paragraph sits in a table, for table-style conditional formatting.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableCellContext {
    /// The table's style (`None` = the default table style).
    pub table_style_id: Option<String>,
    /// The regions that apply, in application order (see [`table_conditions`]).
    pub conditions: Vec<TableCondition>,
}

/// The inputs of [`table_conditions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPosition {
    /// Row index.
    pub row: usize,
    /// Grid column.
    pub col: usize,
    /// Rows in the table.
    pub row_count: usize,
    /// Grid columns.
    pub col_count: usize,
    /// `w:tblLook`.
    pub look: TableLook,
    /// Rows per horizontal band.
    pub row_band_size: usize,
    /// Columns per vertical band.
    pub col_band_size: usize,
    /// Repeating header rows (they all count as first rows).
    pub header_rows: usize,
}

/// The conditional regions of a table style that apply to a cell, in Word's application order:
/// whole table, vertical bands, horizontal bands, first/last column, first/last row, corners.
#[must_use]
pub fn table_conditions(p: &CellPosition) -> Vec<TableCondition> {
    let look = p.look;
    let mut out = vec![TableCondition::WholeTable];
    let header_rows = if look.first_row {
        p.header_rows.max(1)
    } else {
        0
    };
    let first_row = look.first_row && p.row < header_rows;
    let last_row = look.last_row && p.row + 1 == p.row_count;
    let first_col = look.first_column && p.col == 0;
    let last_col = look.last_column && p.col + 1 == p.col_count;
    if !look.no_v_band && !first_col && !last_col {
        let c = p.col.saturating_sub(usize::from(look.first_column));
        out.push(if (c / p.col_band_size.max(1)).is_multiple_of(2) {
            TableCondition::Band1Vert
        } else {
            TableCondition::Band2Vert
        });
    }
    if !look.no_h_band && !first_row && !last_row {
        let r = p.row.saturating_sub(header_rows);
        out.push(if (r / p.row_band_size.max(1)).is_multiple_of(2) {
            TableCondition::Band1Horz
        } else {
            TableCondition::Band2Horz
        });
    }
    if first_col {
        out.push(TableCondition::FirstCol);
    }
    if last_col {
        out.push(TableCondition::LastCol);
    }
    if first_row {
        out.push(TableCondition::FirstRow);
    }
    if last_row {
        out.push(TableCondition::LastRow);
    }
    if first_row && first_col {
        out.push(TableCondition::NwCell);
    }
    if first_row && last_col {
        out.push(TableCondition::NeCell);
    }
    if last_row && first_col {
        out.push(TableCondition::SwCell);
    }
    if last_row && last_col {
        out.push(TableCondition::SeCell);
    }
    out
}

/// The table-style context of the cell at (`row`, grid `col`) of a table.
#[must_use]
pub fn table_cell_context(table: &Table, row: usize, col: usize) -> TableCellContext {
    let p = &table.props;
    TableCellContext {
        table_style_id: p.style_id.clone(),
        conditions: table_conditions(&CellPosition {
            row,
            col,
            row_count: table.rows.len(),
            col_count: table.grid_width(),
            look: p.look.unwrap_or_default(),
            row_band_size: p.row_band_size.unwrap_or(1) as usize,
            col_band_size: p.col_band_size.unwrap_or(1) as usize,
            header_rows: table.header_rows().max(1),
        }),
    }
}

fn table_chain<'a>(styles: &'a StyleSheet, table_style: Option<&str>) -> Vec<&'a Style> {
    let id = table_style
        .filter(|id| styles.get(id).is_some_and(|s| s.kind == StyleType::Table))
        .or_else(|| {
            styles
                .default_style(StyleType::Table)
                .map(|s| s.id.as_str())
        });
    styles.chain(id)
}

/// Merges one property group across a table style chain and its applicable regions.
fn table_layer<T: Default + Clone>(
    styles: &StyleSheet,
    table: Option<&TableCellContext>,
    whole: impl Fn(&Style) -> Option<&T>,
    cond: impl Fn(&crate::style::TableStyleCondition) -> Option<&T>,
    merge: impl Fn(&T, Option<&T>) -> T,
) -> Option<T> {
    let table = table?;
    let chain = table_chain(styles, table.table_style_id.as_deref());
    let mut acc: Option<T> = None;
    for c in &table.conditions {
        for s in &chain {
            if *c == TableCondition::WholeTable
                && let Some(part) = whole(s)
            {
                acc = Some(merge(&acc.unwrap_or_default(), Some(part)));
            }
            if let Some(part) = s.conditions.get(c).and_then(&cond) {
                acc = Some(merge(&acc.unwrap_or_default(), Some(part)));
            }
        }
    }
    acc
}

/// Effective table properties: the table style chain, then direct properties.
#[must_use]
pub fn resolve_table(ctx: &DocContext<'_>, table: &TableProps) -> TableProps {
    let mut tp = TableProps::default();
    for s in table_chain(ctx.styles, table.style_id.as_deref()) {
        tp = merge_table_props(&tp, s.tbl_pr.as_ref());
    }
    let mut out = merge_table_props(&tp, Some(table));
    out.style_id.clone_from(&table.style_id);
    out
}

/// Effective cell properties: the table style (`w:tcPr` of the whole table and the applicable
/// regions), then the cell's own.
#[must_use]
pub fn resolve_cell(ctx: &DocContext<'_>, table: &Table, row: usize, col: usize) -> TableCellProps {
    let cell_ctx = table_cell_context(table, row, col);
    let base = table_layer(
        ctx.styles,
        Some(&cell_ctx),
        |s| s.tc_pr.as_ref(),
        |c| c.tc_pr.as_ref(),
        merge_cell_props,
    )
    .unwrap_or_default();
    let own = table
        .cell_map()
        .into_iter()
        .find(|s| s.row == row && s.col <= col && col < s.col + s.col_span)
        .and_then(|s| table.rows.get(s.row)?.cells.get(s.cell))
        .map(|c| &c.props);
    merge_cell_props(&base, own)
}

// ------------------------------------------------------------------ paragraphs

/// Effective indentation in twips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ResolvedIndent {
    /// Leading.
    pub left: i32,
    /// Trailing.
    pub right: i32,
    /// First line.
    pub first_line: i32,
    /// Hanging.
    pub hanging: i32,
}

/// Effective spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolvedSpacing {
    /// Before, twips.
    pub before: i32,
    /// After, twips.
    pub after: i32,
    /// Line: 240ths for auto, twips otherwise.
    pub line: i32,
    /// Rule.
    pub line_rule: LineRule,
    /// HTML auto spacing before.
    pub before_autospacing: bool,
    /// HTML auto spacing after.
    pub after_autospacing: bool,
}

/// A paragraph's list membership after style numbering and `numId` 0 cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ListRef {
    /// `w:numId` (never 0).
    pub num_id: u32,
    /// Level 0–8.
    pub level: u8,
}

/// Effective paragraph properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedParagraph {
    /// The paragraph style actually applied (the default style for unknown ids).
    pub style_id: String,
    /// Alignment.
    pub alignment: Alignment,
    /// Indents.
    pub indent: ResolvedIndent,
    /// Spacing.
    pub spacing: ResolvedSpacing,
    /// Keep with next.
    pub keep_next: bool,
    /// Keep lines together.
    pub keep_lines: bool,
    /// Page break before.
    pub page_break_before: bool,
    /// Widow/orphan control (on by default).
    pub widow_control: bool,
    /// Don't add space between paragraphs of the same style.
    pub contextual_spacing: bool,
    /// Don't hyphenate.
    pub suppress_auto_hyphens: bool,
    /// Suppress line numbers.
    pub suppress_line_numbers: bool,
    /// Mirror indents.
    pub mirror_indents: bool,
    /// Outline level 0–8, 9 = body text.
    pub outline_level: u8,
    /// Tab stops, sorted.
    pub tabs: Vec<TabStop>,
    /// Borders.
    pub borders: ParagraphBorders,
    /// Shading.
    pub shading: Option<Shading>,
    /// Right-to-left.
    pub bidi: bool,
    /// Vertical character alignment.
    pub text_alignment: TextAlignment,
    /// Drop cap.
    pub drop_cap: Option<DropCap>,
    /// List membership.
    pub numbering: Option<ListRef>,
}

fn paragraph_chain<'a>(styles: &'a StyleSheet, style_id: Option<&str>) -> Vec<&'a Style> {
    let id = style_id
        .filter(|id| {
            styles
                .get(id)
                .is_some_and(|s| s.kind == StyleType::Paragraph)
        })
        .or_else(|| {
            styles
                .default_style(StyleType::Paragraph)
                .map(|s| s.id.as_str())
        });
    styles.chain(id)
}

/// The list a paragraph belongs to: direct `numPr` (with `numId` 0 cancelling a style's list,
/// and a level-only override keeping the style's list), else the paragraph style's list, at the
/// style's level or the level whose `w:pStyle` links to the style.
#[must_use]
pub fn effective_numbering(ctx: &DocContext<'_>, direct: &ParagraphProps) -> Option<ListRef> {
    let chain = paragraph_chain(ctx.styles, direct.style_id.as_deref());
    let leaf = chain.last().map(|s| s.id.as_str());
    let from_style = ctx.styles.style_numbering(leaf);
    let d = direct.numbering.unwrap_or_default();
    match d.num_id {
        Some(0) => return None,
        Some(num_id) => {
            let level = d.level.or_else(|| from_style.map(|(_, l)| l)).unwrap_or(0);
            return Some(ListRef {
                num_id,
                level: level.min(crate::numbering::MAX_LEVEL),
            });
        }
        None => {}
    }
    let (num_id, style_level) = from_style?;
    let mut level = d.level.unwrap_or(style_level);
    let style_has_level = chain.iter().any(|s| {
        s.ppr
            .as_ref()
            .and_then(|p| p.numbering)
            .and_then(|n| n.level)
            .is_some()
    });
    if d.level.is_none()
        && !style_has_level
        && let (Some(numbering), Some(leaf)) = (ctx.numbering, leaf)
        && let Some(l) = numbering.level_for_style(num_id, leaf, Some(ctx.styles))
    {
        level = l;
    }
    Some(ListRef {
        num_id,
        level: level.min(crate::numbering::MAX_LEVEL),
    })
}

/// Resolves a paragraph's effective properties from its direct properties.
#[must_use]
pub fn resolve_paragraph(
    ctx: &DocContext<'_>,
    direct: &ParagraphProps,
    table: Option<&TableCellContext>,
) -> ResolvedParagraph {
    let mut p = merge_paragraph_props(
        &ParagraphProps::default(),
        Some(&ctx.styles.doc_defaults.ppr),
    );
    if let Some(layer) = table_layer(
        ctx.styles,
        table,
        |s| s.ppr.as_ref(),
        |c| c.ppr.as_ref(),
        merge_paragraph_props,
    ) {
        p = merge_paragraph_props(&p, Some(&layer));
    }
    let chain = paragraph_chain(ctx.styles, direct.style_id.as_deref());
    for s in &chain {
        p = merge_paragraph_props(&p, s.ppr.as_ref());
    }
    let num = effective_numbering(ctx, direct);
    if let (Some(n), Some(numbering)) = (num, ctx.numbering)
        && let Some(lvl) = numbering.level(n.num_id, n.level, Some(ctx.styles))
    {
        p = merge_paragraph_props(&p, lvl.ppr.as_ref());
    }
    let direct_no_num = ParagraphProps {
        numbering: None,
        ..direct.clone()
    };
    p = merge_paragraph_props(&p, Some(&direct_no_num));
    let ind = p.indent.unwrap_or_default();
    let sp = p.spacing.unwrap_or_default();
    ResolvedParagraph {
        style_id: chain
            .last()
            .map(|s| s.id.clone())
            .or_else(|| direct.style_id.clone())
            .unwrap_or_else(|| "Normal".to_owned()),
        alignment: p.alignment.unwrap_or_default(),
        indent: ResolvedIndent {
            left: ind.left.unwrap_or(0),
            right: ind.right.unwrap_or(0),
            first_line: ind.first_line.unwrap_or(0),
            hanging: ind.hanging.unwrap_or(0),
        },
        spacing: ResolvedSpacing {
            before: sp.before.unwrap_or(0),
            after: sp.after.unwrap_or(0),
            line: sp.line.unwrap_or(240),
            line_rule: sp.line_rule.unwrap_or_default(),
            before_autospacing: sp.before_autospacing.unwrap_or(false),
            after_autospacing: sp.after_autospacing.unwrap_or(false),
        },
        keep_next: p.keep_next.unwrap_or(false),
        keep_lines: p.keep_lines.unwrap_or(false),
        page_break_before: p.page_break_before.unwrap_or(false),
        widow_control: p.widow_control.unwrap_or(true),
        contextual_spacing: p.contextual_spacing.unwrap_or(false),
        suppress_auto_hyphens: p.suppress_auto_hyphens.unwrap_or(false),
        suppress_line_numbers: p.suppress_line_numbers.unwrap_or(false),
        mirror_indents: p.mirror_indents.unwrap_or(false),
        outline_level: p.outline_level.unwrap_or(9).min(9),
        tabs: p.tabs.unwrap_or_default(),
        borders: p.borders.unwrap_or_default(),
        shading: p.shading,
        bidi: p.bidi.unwrap_or(false),
        text_alignment: p.text_alignment.unwrap_or_default(),
        drop_cap: p.drop_cap,
        numbering: num,
    }
}

// ------------------------------------------------------------------ runs

/// What a run resolution is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RunTarget {
    /// Text of the paragraph.
    #[default]
    Text,
    /// The paragraph's list label: takes the numbering level's `w:rPr` and the paragraph mark's
    /// properties instead of a run's.
    ListLabel,
}

/// The inputs of [`resolve_run`].
#[derive(Debug, Clone, Copy)]
pub struct RunInput<'a> {
    /// The paragraph's direct properties.
    pub paragraph: &'a ParagraphProps,
    /// The paragraph mark's run properties (used for list labels).
    pub mark_props: &'a RunProps,
    /// The run's direct properties (ignored for list labels).
    pub direct: &'a RunProps,
    /// Table context.
    pub table: Option<&'a TableCellContext>,
    /// Text or list label.
    pub target: RunTarget,
}

/// Effective run properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRun {
    /// The character style applied (text runs only).
    pub style_id: Option<String>,
    /// Concrete fonts after theme resolution.
    pub fonts: ResolvedFonts,
    /// Bold.
    pub bold: bool,
    /// Complex-script bold.
    pub bold_cs: bool,
    /// Italic.
    pub italic: bool,
    /// Complex-script italic.
    pub italic_cs: bool,
    /// Underline (`None` for none).
    pub underline: Option<Underline>,
    /// Strikethrough.
    pub strike: bool,
    /// Double strikethrough.
    pub dstrike: bool,
    /// All caps.
    pub caps: bool,
    /// Small caps.
    pub small_caps: bool,
    /// Hidden.
    pub hidden: bool,
    /// Emboss.
    pub emboss: bool,
    /// Engrave.
    pub imprint: bool,
    /// Outline.
    pub outline: bool,
    /// Shadow.
    pub shadow: bool,
    /// Text colour after theme resolution.
    pub color: ColorValue,
    /// Highlight.
    pub highlight: Option<HighlightColor>,
    /// Shading.
    pub shading: Option<Shading>,
    /// Border.
    pub border: Option<Border>,
    /// Size in half-points (Word's default without any setting is 20 = 10 pt).
    pub size: u32,
    /// Complex-script size in half-points.
    pub size_cs: u32,
    /// Superscript/subscript.
    pub vert_align: VertAlign,
    /// Character spacing, twips.
    pub spacing: i32,
    /// Horizontal scale, percent.
    pub scale: u32,
    /// Kerning threshold, half-points.
    pub kern: Option<u32>,
    /// Baseline offset, half-points.
    pub position: i32,
    /// Right-to-left.
    pub rtl: bool,
    /// Languages.
    pub lang: Lang,
}

fn chain_run(chain: &[&Style]) -> Option<RunProps> {
    let mut acc: Option<RunProps> = None;
    for s in chain {
        if let Some(r) = &s.rpr {
            acc = Some(merge_run_props(&acc.unwrap_or_default(), Some(r)));
        }
    }
    acc
}

struct RunAcc {
    props: RunProps,
    toggles: [Option<bool>; 13],
}

impl RunAcc {
    fn absolute(&mut self, layer: Option<&RunProps>) {
        let Some(layer) = layer else { return };
        for t in ToggleProp::ALL {
            if let Some(v) = layer.toggle(t) {
                self.toggles[t.index()] = Some(v);
            }
        }
        self.props = merge_run_props(&self.props, Some(layer));
    }

    fn toggle_layer(&mut self, layer: Option<&RunProps>) {
        let Some(layer) = layer else { return };
        let mut rest = layer.clone();
        for t in ToggleProp::ALL {
            if layer.toggle(t) == Some(true) {
                let cur = self.toggles[t.index()].unwrap_or(false);
                self.toggles[t.index()] = Some(!cur);
            }
            rest.set_toggle(t, None);
        }
        self.props = merge_run_props(&self.props, Some(&rest));
    }
}

/// Resolves the effective run properties of a run or of a paragraph's list label.
#[must_use]
pub fn resolve_run(ctx: &DocContext<'_>, input: &RunInput<'_>) -> ResolvedRun {
    let mut acc = RunAcc {
        props: RunProps::default(),
        toggles: [None; 13],
    };
    acc.absolute(Some(&ctx.styles.doc_defaults.rpr));
    let table_rpr = table_layer(
        ctx.styles,
        input.table,
        |s| s.rpr.as_ref(),
        |c| c.rpr.as_ref(),
        merge_run_props,
    );
    acc.absolute(table_rpr.as_ref());
    let para_rpr = chain_run(&paragraph_chain(
        ctx.styles,
        input.paragraph.style_id.as_deref(),
    ));
    acc.absolute(para_rpr.as_ref());
    match input.target {
        RunTarget::ListLabel => {
            if let (Some(n), Some(numbering)) =
                (effective_numbering(ctx, input.paragraph), ctx.numbering)
                && let Some(lvl) = numbering.level(n.num_id, n.level, Some(ctx.styles))
            {
                acc.absolute(lvl.rpr.as_ref());
            }
            acc.absolute(Some(input.mark_props));
        }
        RunTarget::Text => {
            let char_style = input
                .direct
                .style_id
                .as_deref()
                .filter(|id| {
                    ctx.styles
                        .get(id)
                        .is_some_and(|s| s.kind == StyleType::Character)
                })
                .or_else(|| {
                    ctx.styles
                        .default_style(StyleType::Character)
                        .map(|s| s.id.as_str())
                });
            let char_rpr = chain_run(&ctx.styles.chain(char_style));
            acc.toggle_layer(char_rpr.as_ref());
            acc.absolute(Some(input.direct));
        }
    }
    let rp = acc.props;
    let t = |p: ToggleProp| acc.toggles[p.index()].unwrap_or(false);
    let mapping = ctx
        .settings
        .map(|s| s.color_scheme_mapping)
        .unwrap_or_default();
    let size = rp.size.unwrap_or(20);
    ResolvedRun {
        style_id: match input.target {
            RunTarget::Text => input.direct.style_id.clone(),
            RunTarget::ListLabel => None,
        },
        fonts: resolve_run_fonts(rp.fonts.as_ref(), ctx.theme),
        bold: t(ToggleProp::Bold),
        bold_cs: t(ToggleProp::BoldCs),
        italic: t(ToggleProp::Italic),
        italic_cs: t(ToggleProp::ItalicCs),
        underline: rp
            .underline
            .filter(|u| u.style != crate::st::UnderlineStyle::None),
        strike: t(ToggleProp::Strike),
        dstrike: t(ToggleProp::Dstrike),
        caps: t(ToggleProp::Caps),
        small_caps: t(ToggleProp::SmallCaps),
        hidden: t(ToggleProp::Hidden),
        emboss: t(ToggleProp::Emboss),
        imprint: t(ToggleProp::Imprint),
        outline: t(ToggleProp::Outline),
        shadow: t(ToggleProp::Shadow),
        color: resolve_color(rp.color.as_ref(), ctx.theme, &mapping),
        highlight: rp.highlight.filter(|h| *h != HighlightColor::None),
        shading: rp.shading,
        border: rp.border,
        size,
        size_cs: rp.size_cs.unwrap_or(size),
        vert_align: rp.vert_align.unwrap_or_default(),
        spacing: rp.spacing.unwrap_or(0),
        scale: rp.scale.unwrap_or(100),
        kern: rp.kern,
        position: rp.position.unwrap_or(0),
        rtl: rp.rtl.unwrap_or(false),
        lang: rp.lang.unwrap_or_default(),
    }
}

/// Convenience: resolves a run of text with no paragraph mark properties.
#[must_use]
pub fn resolve_text_run(
    ctx: &DocContext<'_>,
    paragraph: &ParagraphProps,
    direct: &RunProps,
    table: Option<&TableCellContext>,
) -> ResolvedRun {
    resolve_run(
        ctx,
        &RunInput {
            paragraph,
            mark_props: &RunProps::default(),
            direct,
            table,
            target: RunTarget::Text,
        },
    )
}

#[cfg(test)]
mod tests;
