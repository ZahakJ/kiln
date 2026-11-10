// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formatting: the shared editing commands on text (the selection while editing, or all the
//! text of the selected shapes, as PowerPoint), Character Spacing and Text Shadow, shape fill,
//! outline, effects and Quick Styles, Align Text, Text Direction, Columns, Autofit, the Format
//! Painter, and the Table Design and Layout commands on a table.

use kiln_common::{Color, KilnError, NodeId, Result};
use kiln_deck::ops::{Op, TextTarget, Transaction};
use kiln_deck::shape::{FontCollectionRef, StyleRef};
use kiln_deck::text::{Autofit, Bullet, TextDirection, TextSpacing};
use kiln_deck::{ShapeKind, ShapeProps, ShapeStyle, TableCell, TableRow, TextAnchor};
use kiln_doc::st::{Alignment, HighlightColor, UnderlineStyle, VertAlign};
use kiln_doc::{DocColor, Pos, RunFonts, RunProps, Underline};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::effects::{EffectList, Glow, OuterShadow};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::{Line, LineDash, LineEnd};
use kiln_ooxml::st::{LineEndType, PresetLineDashVal, RectAlignment};
use kiln_protocol::commands::{
    BorderPreset, CellVerticalAlign, DeckAnchor, DeckAutofit, DeckCommand, DeckEffect, DeckOutline,
    DeckTextDirection, EditCommand, TableDeleteTarget, TextAlign, TextCase,
};

use super::DeckDoc;
use super::cells::{merge_cells, split_cell};
use super::slides::{drawing_color, fill_of};
use crate::model::Change;

/// What the Format Painter carries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FormatClip {
    /// The shape's look.
    pub shape: Option<ShapeProps>,
    /// The text's formatting.
    pub run: Option<RunProps>,
}

/// PowerPoint's Grow Font / Shrink Font steps, in points.
const SIZES: [f32; 25] = [
    8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0,
    48.0, 54.0, 60.0, 66.0, 72.0, 80.0, 88.0, 96.0, 4000.0,
];

fn parse_rgb(hex: &str) -> Result<Color> {
    Color::parse_hex(hex.trim().trim_start_matches('#'))
        .map_err(|_| KilnError::InvalidInput(format!("'{hex}' is not a colour")))
}

fn nearest_highlight(c: Color) -> HighlightColor {
    let mut best = (u32::MAX, HighlightColor::Yellow);
    for h in [
        HighlightColor::Yellow,
        HighlightColor::Green,
        HighlightColor::Cyan,
        HighlightColor::Magenta,
        HighlightColor::Blue,
        HighlightColor::Red,
        HighlightColor::DarkBlue,
        HighlightColor::DarkCyan,
        HighlightColor::DarkGreen,
        HighlightColor::DarkMagenta,
        HighlightColor::DarkRed,
        HighlightColor::DarkYellow,
        HighlightColor::DarkGray,
        HighlightColor::LightGray,
        HighlightColor::Black,
    ] {
        if let Some(rgb) = h.rgb() {
            let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).unsigned_abs().pow(2);
            let dist = d(rgb.r, c.r) + d(rgb.g, c.g) + d(rgb.b, c.b);
            if dist < best.0 {
                best = (dist, h);
            }
        }
    }
    best.1
}

/// Merges the fields `over` sets into `base`.
fn overlay(base: &mut RunProps, over: &RunProps) {
    *base = kiln_doc::merge::merge_run_props(base, Some(over));
}

impl DeckDoc {
    /// Where text formatting applies: the selection while editing (the word at a collapsed
    /// caret), else all the text of the selected shapes.
    pub(crate) fn format_targets(&self) -> Vec<(TextTarget, Pos, Pos)> {
        if let Some(t) = &self.view.text {
            let Some(body) = self.body(t.target) else {
                return Vec::new();
            };
            if !t.collapsed() {
                let (a, b) = super::text::ordered(body, t.anchor, t.head);
                return vec![(t.target, a, b)];
            }
            // The word around the caret.
            let text: Vec<char> = body
                .paragraph(t.head.para)
                .map(|p| p.content.raw_text().chars().collect())
                .unwrap_or_default();
            let word = |c: char| c.is_alphanumeric() || c == '\'';
            let o = t.head.offset.min(text.len());
            if o == 0 || o == text.len() || !word(text[o - 1]) || !word(text[o]) {
                return Vec::new();
            }
            let (mut s, mut e) = (o, o);
            while s > 0 && word(text[s - 1]) {
                s -= 1;
            }
            while e < text.len() && word(text[e]) {
                e += 1;
            }
            return vec![(t.target, Pos::new(t.head.para, s), Pos::new(t.head.para, e))];
        }
        let mut out = Vec::new();
        for id in &self.view.shapes {
            let Some(s) = self.pres.shape(*id) else {
                continue;
            };
            match &s.kind {
                ShapeKind::Table(t) => {
                    for (r, row) in t.rows.iter().enumerate() {
                        for (c, cell) in row.cells.iter().enumerate() {
                            if let Some((a, b)) = super::text::body_ends(&cell.text) {
                                out.push((
                                    TextTarget::Cell {
                                        shape: *id,
                                        row: r,
                                        col: c,
                                    },
                                    a,
                                    b,
                                ));
                            }
                        }
                    }
                }
                _ => {
                    if let Some(body) = s.text()
                        && let Some((a, b)) = super::text::body_ends(body)
                    {
                        out.push((TextTarget::Shape(*id), a, b));
                    }
                }
            }
        }
        out
    }

    /// The effective run formatting at the start of the first formatting target (or at the
    /// caret).
    pub(crate) fn effective_run(&self) -> Option<RunProps> {
        let (target, at) = match &self.view.text {
            Some(t) => {
                let body = self.body(t.target)?;
                let (a, _) = super::text::ordered(body, t.anchor, t.head);
                // At a caret, the formatting of the char before it (what typing gets).
                let at = if t.collapsed() && a.offset > 0 {
                    Pos::new(a.para, a.offset - 1)
                } else {
                    a
                };
                (t.target, at)
            }
            None => {
                let (t, a, _) = self.format_targets().into_iter().next()?;
                (t, a)
            }
        };
        let shape = match target {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => s,
            TextTarget::Notes(_) => return None,
        };
        let body = self.body(target)?;
        let para = body.paragraph(at.para)?;
        self.pres.resolved_run(shape, body, para, at.offset)
    }

    /// Changes run formatting over every target as one undo step.
    pub(crate) fn format_runs(
        &mut self,
        label: &'static str,
        f: impl Fn(&mut RunProps),
    ) -> Result<Change> {
        let targets = self.format_targets();
        if targets.is_empty() {
            return Ok(Change::None);
        }
        self.begin_group();
        let mut result = Ok(());
        for (target, a, b) in targets {
            if let Err(e) = self.edit_body(label, target, |doc| {
                kiln_doc::edit::format_runs(doc, a, b, &f)
            }) {
                result = Err(e);
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// Changes paragraph properties of the targets' paragraphs.
    pub(super) fn format_paragraphs(
        &mut self,
        label: &'static str,
        f: impl Fn(&mut kiln_doc::ParagraphProps),
    ) -> Result<Change> {
        let targets = self.paragraph_targets();
        if targets.is_empty() {
            return Ok(Change::None);
        }
        self.begin_group();
        let mut result = Ok(());
        for (target, a, b) in targets {
            if let Err(e) = self.edit_body(label, target, |doc| {
                kiln_doc::edit::format_paragraphs(doc, a, b, &f)
            }) {
                result = Err(e);
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// Where paragraph formatting applies: the selected paragraphs while editing (the caret's
    /// paragraph too), else every paragraph of the selected shapes.
    fn paragraph_targets(&self) -> Vec<(TextTarget, Pos, Pos)> {
        if let Some(t) = &self.view.text {
            let Some(body) = self.body(t.target) else {
                return Vec::new();
            };
            let (a, b) = super::text::ordered(body, t.anchor, t.head);
            return vec![(t.target, a, b)];
        }
        self.format_targets()
    }

    /// Changes the DrawingML extras (bullets, spacing) of the targets' paragraphs.
    fn para_extras(
        &mut self,
        label: &'static str,
        f: impl Fn(&mut kiln_deck::DeckParaProps),
    ) -> Result<Change> {
        let targets = self.paragraph_targets();
        if targets.is_empty() {
            return Ok(Change::None);
        }
        self.begin_group();
        let mut result = Ok(());
        for (target, a, b) in targets {
            let paras = self.paragraphs_between(target, a, b);
            match kiln_deck::edit::set_para_extras(&self.pres, target, &paras, &f) {
                Ok(tx) => {
                    if let Err(e) = self.apply(label, tx) {
                        result = Err(e);
                    }
                }
                Err(e) => result = Err(KilnError::InvalidInput(e.to_string())),
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// The resolved paragraph at the caret or of the first selected shape.
    pub(crate) fn effective_paragraph(&self) -> Option<kiln_deck::ResolvedParagraph> {
        let (target, at) = match &self.view.text {
            Some(t) => (t.target, t.head),
            None => {
                let (t, a, _) = self.format_targets().into_iter().next()?;
                (t, a)
            }
        };
        let shape = match target {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => s,
            TextTarget::Notes(_) => return None,
        };
        let body = self.body(target)?;
        let para = body.paragraph(at.para)?;
        self.pres.resolved_paragraph(shape, body, para)
    }

    /// Changes the case of the targets' text (keeping the formatting).
    fn change_case(&mut self, case: Option<TextCase>) -> Result<Change> {
        let targets = self.format_targets();
        if targets.is_empty() {
            return Ok(Change::None);
        }
        // Shift+F3 cycles: lower → UPPER → Title (from what the text is now).
        let case = case.unwrap_or_else(|| {
            let sample = targets
                .first()
                .and_then(|(t, a, b)| {
                    self.body(*t)
                        .map(|body| super::text::range_text(body, *a, *b))
                })
                .unwrap_or_default();
            if sample.chars().any(char::is_lowercase) && sample.chars().any(char::is_uppercase) {
                if sample == sample.to_lowercase() {
                    TextCase::Upper
                } else {
                    TextCase::Lower
                }
            } else if sample.chars().any(char::is_uppercase) {
                TextCase::Lower
            } else {
                TextCase::Upper
            }
        });
        self.begin_group();
        let mut result = Ok(());
        for (target, a, b) in targets {
            let segments = self.segments(target, a, b);
            let r = self.edit_body("suite.format.change_case", target, |doc| {
                let mut ops = Vec::new();
                for (para, s, e) in &segments {
                    let Some(p) = doc.paragraph(*para) else {
                        continue;
                    };
                    let Ok(mut frag) = p.content.slice(*s, *e) else {
                        continue;
                    };
                    let chars: Vec<char> = frag.text.chars().collect();
                    let mut out = String::with_capacity(frag.text.len());
                    let mut start_of_word =
                        *s == 0 || p.content.char_at(s - 1).is_none_or(char::is_whitespace);
                    let mut first_letter = true;
                    for c in &chars {
                        let mapped = if *c == kiln_doc::OBJECT_CHAR {
                            *c
                        } else {
                            match case {
                                TextCase::Upper => c.to_uppercase().next().unwrap_or(*c),
                                TextCase::Lower => c.to_lowercase().next().unwrap_or(*c),
                                TextCase::Title => {
                                    if start_of_word {
                                        c.to_uppercase().next().unwrap_or(*c)
                                    } else {
                                        c.to_lowercase().next().unwrap_or(*c)
                                    }
                                }
                                TextCase::Sentence => {
                                    if first_letter && c.is_alphabetic() {
                                        c.to_uppercase().next().unwrap_or(*c)
                                    } else {
                                        c.to_lowercase().next().unwrap_or(*c)
                                    }
                                }
                                TextCase::Toggle => {
                                    if c.is_uppercase() {
                                        c.to_lowercase().next().unwrap_or(*c)
                                    } else {
                                        c.to_uppercase().next().unwrap_or(*c)
                                    }
                                }
                            }
                        };
                        if c.is_alphabetic() {
                            first_letter = false;
                        }
                        start_of_word = c.is_whitespace();
                        out.push(mapped);
                    }
                    if out.chars().count() != chars.len() || out == frag.text {
                        continue;
                    }
                    frag.text = out;
                    ops.push(kiln_doc::Op::DeleteInline {
                        para: *para,
                        offset: *s,
                        len: e - s,
                    });
                    ops.push(kiln_doc::Op::InsertInline {
                        para: *para,
                        offset: *s,
                        fragment: frag,
                    });
                }
                Ok(kiln_doc::Transaction::new(ops).with_kind(kiln_doc::EditKind::Format))
            });
            if let Err(e) = r {
                result = Err(e);
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// The shared editing commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn edit_command(&mut self, cmd: &EditCommand) -> Result<Change> {
        use EditCommand as E;
        let eff = self.effective_run().unwrap_or_default();
        match cmd {
            E::Cut
            | E::Copy
            | E::Paste { .. }
            | E::PasteKeepSource { .. }
            | E::PasteMerge { .. }
            | E::PasteTextOnly { .. }
            | E::PasteAs { .. } => self.clipboard_command(cmd),
            E::Bold => {
                let on = eff.bold != Some(true);
                self.format_runs("suite.format.bold", |p| p.bold = Some(on))
            }
            E::Italic => {
                let on = eff.italic != Some(true);
                self.format_runs("suite.format.italic", |p| p.italic = Some(on))
            }
            E::Underline | E::DoubleUnderline => {
                let style = if matches!(cmd, E::DoubleUnderline) {
                    UnderlineStyle::Double
                } else {
                    UnderlineStyle::Single
                };
                let on = eff
                    .underline
                    .as_ref()
                    .is_none_or(|u| u.style == UnderlineStyle::None || u.style != style);
                self.format_runs("suite.format.underline", |p| {
                    p.underline = Some(Underline::new(if on {
                        style
                    } else {
                        UnderlineStyle::None
                    }));
                })
            }
            E::Strikethrough => {
                let on = eff.strike != Some(true);
                self.format_runs("suite.format.strikethrough", |p| p.strike = Some(on))
            }
            E::Subscript | E::Superscript => {
                let want = if matches!(cmd, E::Subscript) {
                    VertAlign::Subscript
                } else {
                    VertAlign::Superscript
                };
                let set = if eff.vert_align == Some(want) {
                    VertAlign::Baseline
                } else {
                    want
                };
                self.format_runs(cmd_id(cmd), |p| p.vert_align = Some(set))
            }
            E::AllCaps => {
                let on = eff.caps != Some(true);
                self.format_runs("suite.format.all_caps", |p| p.caps = Some(on))
            }
            E::FontFamily { family } => {
                let family = family.trim().to_owned();
                if family.is_empty() {
                    return Ok(Change::None);
                }
                self.format_runs("suite.format.font_family", |p| {
                    p.fonts = Some(RunFonts {
                        ascii: Some(family.clone()),
                        h_ansi: Some(family.clone()),
                        east_asia: Some(family.clone()),
                        cs: Some(family.clone()),
                        ..RunFonts::default()
                    });
                })
            }
            E::FontSize { size } => {
                if !(1.0..=4000.0).contains(size) {
                    return Err(KilnError::InvalidInput(format!(
                        "{size} is not a font size"
                    )));
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let hp = (size * 2.0).round() as u32;
                self.format_runs("suite.format.font_size", |p| p.size = Some(hp))
            }
            E::GrowFont { one_point } | E::ShrinkFont { one_point } => {
                let grow = matches!(cmd, E::GrowFont { .. });
                // Each run steps from its own size.
                let targets = self.format_targets();
                if targets.is_empty() {
                    return Ok(Change::None);
                }
                let step = *one_point;
                let label = if grow {
                    "suite.format.grow_font"
                } else {
                    "suite.format.shrink_font"
                };
                self.begin_group();
                let mut result = Ok(());
                for (target, a, b) in targets {
                    let shape = match target {
                        TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => s,
                        TextTarget::Notes(s) => s,
                    };
                    let segments = self.segments(target, a, b);
                    let mut sizes = Vec::new();
                    if let Some(body) = self.body(target) {
                        for (para, s, e) in &segments {
                            let Some(p) = body.paragraph(*para) else {
                                continue;
                            };
                            let mut pos = 0usize;
                            for r in p.content.runs() {
                                let (rs, re) = (pos.max(*s), (pos + r.len).min(*e));
                                if rs < re {
                                    let eff = self
                                        .pres
                                        .resolved_run(shape, body, p, rs)
                                        .unwrap_or_default();
                                    #[allow(clippy::cast_precision_loss)]
                                    let pt = eff.size.map_or(18.0, |h| h as f32 / 2.0);
                                    let next = if step {
                                        if grow { pt + 1.0 } else { (pt - 1.0).max(1.0) }
                                    } else if grow {
                                        SIZES.iter().copied().find(|v| *v > pt + 0.01).unwrap_or(pt)
                                    } else {
                                        SIZES
                                            .iter()
                                            .rev()
                                            .copied()
                                            .find(|v| *v < pt - 0.01)
                                            .unwrap_or(1.0f32.max(pt - 1.0))
                                    };
                                    sizes.push((*para, rs, re, next));
                                }
                                pos += r.len;
                            }
                        }
                    }
                    let r = self.edit_body(label, target, |doc| {
                        let mut ops = Vec::new();
                        for (para, s, e, pt) in &sizes {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let hp = (pt * 2.0).round() as u32;
                            let t = kiln_doc::edit::format_runs(
                                doc,
                                Pos::new(*para, *s),
                                Pos::new(*para, *e),
                                |p| {
                                    p.size = Some(hp);
                                },
                            )?;
                            ops.extend(t.ops);
                        }
                        Ok(kiln_doc::Transaction::new(ops).with_kind(kiln_doc::EditKind::Format))
                    });
                    if let Err(e) = r {
                        result = Err(e);
                    }
                }
                self.end_group();
                result?;
                Ok(Change::Content)
            }
            E::ChangeCase { case } => self.change_case(*case),
            E::ClearFormatting => self.format_runs("suite.format.clear_formatting", |p| {
                let lang = p.lang.clone();
                *p = RunProps::default();
                p.lang = lang;
            }),
            E::FontColor { color } => {
                let c = match color {
                    Some(hex) => Some(DocColor {
                        val: kiln_doc::ColorValue::Rgb(parse_rgb(hex)?),
                        ..DocColor::default()
                    }),
                    None => None,
                };
                self.format_runs("suite.format.font_color", |p| p.color = c.clone())
            }
            E::Highlight { color } => {
                let h = match color {
                    Some(hex) => Some(nearest_highlight(parse_rgb(hex)?)),
                    None => Some(HighlightColor::None),
                };
                self.format_runs("suite.format.highlight", |p| p.highlight = h)
            }
            E::Bullets { glyph } => {
                let on = !self
                    .effective_paragraph()
                    .is_some_and(|r| matches!(r.deck.bullet, Some(Bullet::Char(_))));
                let g = glyph.clone().unwrap_or_else(|| "•".into());
                self.para_extras("suite.format.bullets", |d| {
                    d.bullet = Some(if on {
                        Bullet::Char(g.clone())
                    } else {
                        Bullet::None
                    });
                })
            }
            E::Numbering { style } => {
                let on = !self
                    .effective_paragraph()
                    .is_some_and(|r| matches!(r.deck.bullet, Some(Bullet::AutoNumber { .. })));
                let scheme = match style {
                    Some(kiln_protocol::commands::ListNumberStyle::DecimalParen) => "arabicParenR",
                    Some(kiln_protocol::commands::ListNumberStyle::UpperRoman) => "romanUcPeriod",
                    Some(kiln_protocol::commands::ListNumberStyle::LowerRoman) => "romanLcPeriod",
                    Some(kiln_protocol::commands::ListNumberStyle::UpperLetter) => "alphaUcPeriod",
                    Some(kiln_protocol::commands::ListNumberStyle::LowerLetter) => "alphaLcPeriod",
                    Some(kiln_protocol::commands::ListNumberStyle::LowerLetterParen) => {
                        "alphaLcParenR"
                    }
                    _ => "arabicPeriod",
                };
                self.para_extras("suite.format.numbering", |d| {
                    d.bullet = Some(if on {
                        Bullet::AutoNumber {
                            scheme: scheme.into(),
                            start_at: 1,
                        }
                    } else {
                        Bullet::None
                    });
                })
            }
            E::Indent | E::Outdent => {
                let delta = if matches!(cmd, E::Indent) { 1 } else { -1 };
                let targets = self.paragraph_targets();
                self.begin_group();
                let mut result = Ok(());
                for (target, a, b) in targets {
                    let paras = self.paragraphs_between(target, a, b);
                    match kiln_deck::edit::set_paragraph_level(&self.pres, target, &paras, delta) {
                        Ok(tx) => {
                            if let Err(e) = self.apply(cmd_id(cmd), tx) {
                                result = Err(e);
                            }
                        }
                        Err(e) => result = Err(KilnError::InvalidInput(e.to_string())),
                    }
                }
                self.end_group();
                result?;
                Ok(Change::Content)
            }
            E::AlignLeft | E::AlignCenter | E::AlignRight | E::Justify => {
                let a = match cmd {
                    E::AlignCenter => Alignment::Center,
                    E::AlignRight => Alignment::Right,
                    E::Justify => Alignment::Both,
                    _ => Alignment::Left,
                };
                self.format_paragraphs(cmd_id(cmd), |p| p.alignment = Some(a))
            }
            E::LineSpacing { spacing } => {
                if !(0.0..=10.0).contains(spacing) {
                    return Err(KilnError::InvalidInput(format!(
                        "{spacing} is not a line spacing"
                    )));
                }
                #[allow(clippy::cast_possible_truncation)]
                let pc = (f64::from(*spacing) * 100_000.0).round() as i32;
                self.para_extras("suite.format.line_spacing", |d| {
                    d.line_spacing = Some(TextSpacing::Percent(pc));
                })
            }
            E::InsertDate | E::InsertTime => {
                let days = i64::try_from(self.now_ms / 86_400_000).unwrap_or(0);
                let (y, m, d) = super::save::civil(days);
                let secs = (self.now_ms / 1000) % 86_400;
                let text = if matches!(cmd, E::InsertDate) {
                    format!("{d:02}/{m:02}/{y:04}")
                } else {
                    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
                };
                self.type_text(&text)
            }
            E::Repeat => Ok(Change::None),
            E::TableInsertRowAbove
            | E::TableInsertRowBelow
            | E::TableInsertColumnLeft
            | E::TableInsertColumnRight
            | E::TableDelete { .. }
            | E::TableDistributeRows
            | E::TableDistributeColumns
            | E::TableCellAlignment { .. }
            | E::TableHeaderRow
            | E::TableTotalRow
            | E::TableBandedRows
            | E::TableFirstColumn
            | E::TableBandedColumns
            | E::TableStyle { .. }
            | E::TableShading { .. }
            | E::TableBorders { .. }
            | E::TableMergeCells
            | E::TableSplitCells { .. } => self.table_command(cmd),
            other => Err(KilnError::Unsupported(format!(
                "{} in Kiln Deck",
                kiln_protocol::Command::Suite(kiln_protocol::SuiteCommand::Edit(other.clone()))
                    .id()
            ))),
        }
    }

    /// The selected table and the cell of the caret (row 0, column 0 without text editing).
    pub(super) fn table_cell(&self) -> Option<(NodeId, usize, usize)> {
        if let Some(t) = &self.view.text
            && let TextTarget::Cell { shape, row, col } = t.target
        {
            return Some((shape, row, col));
        }
        self.view
            .shapes
            .iter()
            .find(|id| {
                matches!(
                    self.pres.shape(**id).map(|s| &s.kind),
                    Some(ShapeKind::Table(_))
                )
            })
            .map(|id| (*id, 0, 0))
    }

    /// Table Design and Layout.
    #[allow(clippy::too_many_lines)]
    fn table_command(&mut self, cmd: &EditCommand) -> Result<Change> {
        use EditCommand as E;
        let (id, row, col) = self.table_cell().ok_or_else(|| {
            KilnError::unavailable(cmd_id(cmd), "put the cursor in a table first")
        })?;
        let Some(ShapeKind::Table(old)) = self.pres.shape(id).map(|s| s.kind.clone()) else {
            return Ok(Change::None);
        };
        let mut t = old.clone();
        let cols = t.grid.len();
        let new_cell = || TableCell::new();
        // The selected cells, or the one with the caret.
        let (r0, c0, nr, nc) = self.cell_range().map_or((row, col, 1, 1), |(_, r)| {
            let n = |v: u32| usize::try_from(v).unwrap_or(0);
            (n(r.row), n(r.col), n(r.rows).max(1), n(r.cols).max(1))
        });
        let in_range = |r: usize, c: usize| r >= r0 && r < r0 + nr && c >= c0 && c < c0 + nc;
        let mut caret: Option<(usize, usize)> = None;
        let mut delete_shape = false;
        match cmd {
            E::TableInsertRowAbove | E::TableInsertRowBelow => {
                if t.rows.len() >= kiln_deck::MAX_TABLE_ROWS {
                    return Err(KilnError::InvalidInput(
                        "a table has at most 75 rows".into(),
                    ));
                }
                let at = if matches!(cmd, E::TableInsertRowAbove) {
                    row
                } else {
                    row + 1
                };
                let height = t.rows.get(row).map_or(370_840, |r| r.height);
                t.rows.insert(
                    at.min(t.rows.len()),
                    TableRow {
                        height,
                        cells: (0..cols).map(|_| new_cell()).collect(),
                    },
                );
                caret = Some((at, col));
            }
            E::TableInsertColumnLeft | E::TableInsertColumnRight => {
                if cols >= kiln_deck::MAX_TABLE_COLS {
                    return Err(KilnError::InvalidInput(
                        "a table has at most 75 columns".into(),
                    ));
                }
                let at = if matches!(cmd, E::TableInsertColumnLeft) {
                    col
                } else {
                    col + 1
                };
                let w = t.grid.get(col).copied().unwrap_or(1_219_200);
                t.grid.insert(at.min(cols), w);
                for r in &mut t.rows {
                    r.cells.insert(at.min(r.cells.len()), new_cell());
                }
                caret = Some((row, at));
            }
            E::TableDelete { what } => match what {
                TableDeleteTarget::Rows => {
                    let n = t.rows.len();
                    if r0 == 0 && nr >= n {
                        delete_shape = true;
                    } else {
                        let end = (r0 + nr).min(n);
                        t.rows.drain(r0.min(end)..end);
                        caret = Some((r0.min(t.rows.len() - 1), col));
                    }
                }
                TableDeleteTarget::Columns => {
                    if c0 == 0 && nc >= cols {
                        delete_shape = true;
                    } else {
                        let end = (c0 + nc).min(cols);
                        let start = c0.min(end);
                        t.grid.drain(start..end);
                        for r in &mut t.rows {
                            let e = end.min(r.cells.len());
                            r.cells.drain(start.min(e)..e);
                        }
                        caret = Some((row, start.min(t.grid.len() - 1)));
                    }
                }
                TableDeleteTarget::Table => delete_shape = true,
                TableDeleteTarget::CellContents => {
                    for (r, tr) in t.rows.iter_mut().enumerate() {
                        for (c, cell) in tr.cells.iter_mut().enumerate() {
                            if in_range(r, c) {
                                cell.text = kiln_deck::TextBody::new();
                            }
                        }
                    }
                }
            },
            E::TableDistributeRows => {
                let total: i64 = t.rows.iter().map(|r| r.height).sum();
                let n = i64::try_from(t.rows.len()).unwrap_or(1).max(1);
                for r in &mut t.rows {
                    r.height = total / n;
                }
            }
            E::TableDistributeColumns => {
                let total: i64 = t.grid.iter().sum();
                let n = i64::try_from(cols).unwrap_or(1).max(1);
                for w in &mut t.grid {
                    *w = total / n;
                }
            }
            E::TableCellAlignment {
                vertical,
                horizontal,
            } => {
                let anchor = match vertical {
                    CellVerticalAlign::Top => TextAnchor::Top,
                    CellVerticalAlign::Center => TextAnchor::Middle,
                    CellVerticalAlign::Bottom => TextAnchor::Bottom,
                };
                let mut targets = Vec::new();
                for (r, tr) in t.rows.iter_mut().enumerate() {
                    for (c, cell) in tr.cells.iter_mut().enumerate() {
                        if in_range(r, c) && !cell.h_merge && !cell.v_merge {
                            cell.props.anchor = Some(anchor);
                            targets.push(TextTarget::Cell {
                                shape: id,
                                row: r,
                                col: c,
                            });
                        }
                    }
                }
                let a = match horizontal {
                    TextAlign::Center => Alignment::Center,
                    TextAlign::Right => Alignment::Right,
                    TextAlign::Justify => Alignment::Both,
                    TextAlign::Left => Alignment::Left,
                };
                let tx = Transaction::new(vec![Op::SetShapeKind {
                    shape: id,
                    kind: Box::new(ShapeKind::Table(t)),
                }]);
                self.begin_group();
                let mut result = self.apply(cmd_id(cmd), tx).map(|_| ());
                for target in targets {
                    let Some((s, e)) = self.body(target).and_then(super::text::body_ends) else {
                        continue;
                    };
                    if let Err(err) = self.edit_body(cmd_id(cmd), target, |doc| {
                        kiln_doc::edit::format_paragraphs(doc, s, e, |p| p.alignment = Some(a))
                    }) {
                        result = Err(err);
                    }
                }
                self.end_group();
                result?;
                return Ok(Change::Content);
            }
            E::TableHeaderRow => t.look.first_row = !t.look.first_row,
            E::TableTotalRow => t.look.last_row = !t.look.last_row,
            E::TableBandedRows => t.look.band_row = !t.look.band_row,
            E::TableFirstColumn => t.look.first_col = !t.look.first_col,
            E::TableBandedColumns => t.look.band_col = !t.look.band_col,
            E::TableStyle { style } => {
                t.style_id = Some(
                    match style.as_str() {
                        "Plain" => kiln_deck_layout::table::PLAIN_STYLE,
                        "Grid" => kiln_deck_layout::table::GRID_STYLE,
                        "Banded" => kiln_deck_layout::table::NEUTRAL_STYLE,
                        "Accent" => kiln_deck_layout::table::DEFAULT_STYLE,
                        other if other.starts_with('{') => other,
                        _ => kiln_deck_layout::table::DEFAULT_STYLE,
                    }
                    .to_owned(),
                );
            }
            E::TableShading { color } => {
                let fill = match color {
                    Some(c) => Fill::Solid(drawing_color(c, None, None)),
                    None => Fill::None,
                };
                for (r, tr) in t.rows.iter_mut().enumerate() {
                    for (c, cell) in tr.cells.iter_mut().enumerate() {
                        if in_range(r, c) {
                            cell.props.fill = Some(fill.clone());
                        }
                    }
                }
            }
            E::TableBorders { preset } => {
                let line = |on: bool| -> kiln_deck::shape::CellBorder {
                    kiln_deck::shape::CellBorder(Some(Line {
                        width: Some(12_700),
                        fill: Some(if on {
                            Fill::Solid(DrawingColor::scheme("tx1"))
                        } else {
                            Fill::None
                        }),
                        ..Line::default()
                    }))
                };
                // Sides as the model keeps them: left, right, top, bottom; "outer" is the edge of
                // the selected cells (the whole table with only the caret in it).
                let (r0, c0, nr, nc) = if self.cell_range().is_some() {
                    (r0, c0, nr, nc)
                } else {
                    (0, 0, t.rows.len(), cols)
                };
                let (left, right, top, bottom) = (0, 1, 2, 3);
                for (r, tr) in t.rows.iter_mut().enumerate() {
                    for (c, cell) in tr.cells.iter_mut().enumerate() {
                        if r < r0 || r >= r0 + nr || c < c0 || c >= c0 + nc {
                            continue;
                        }
                        let outer = [c == c0, c + 1 == c0 + nc, r == r0, r + 1 == r0 + nr];
                        for (side, outer_side) in outer.iter().enumerate() {
                            let outer_side = *outer_side;
                            let set = match preset {
                                BorderPreset::None => Some(false),
                                BorderPreset::All => Some(true),
                                BorderPreset::Outside => outer_side.then_some(true),
                                BorderPreset::Inside => (!outer_side).then_some(true),
                                BorderPreset::Top => (side == top && outer_side).then_some(true),
                                BorderPreset::Bottom => {
                                    (side == bottom && outer_side).then_some(true)
                                }
                                BorderPreset::Left => (side == left && outer_side).then_some(true),
                                BorderPreset::Right => {
                                    (side == right && outer_side).then_some(true)
                                }
                                BorderPreset::InsideHorizontal => {
                                    ((side == top || side == bottom) && !outer_side).then_some(true)
                                }
                                BorderPreset::InsideVertical => {
                                    ((side == left || side == right) && !outer_side).then_some(true)
                                }
                            };
                            if let Some(on) = set {
                                cell.props.borders[side] = line(on);
                            }
                        }
                    }
                }
            }
            E::TableMergeCells => {
                if nr * nc < 2 {
                    return Err(KilnError::unavailable(
                        cmd_id(cmd),
                        "select more than one cell first",
                    ));
                }
                merge_cells(&mut t, r0, c0, nr, nc);
                self.view.cells = None;
                caret = Some((r0, c0));
            }
            E::TableSplitCells {
                rows,
                cols: split_cols,
            } => {
                let n = |v: u32| usize::try_from(v.clamp(1, 20)).unwrap_or(1);
                split_cell(&mut t, r0, c0, n(*rows), n(*split_cols));
                self.view.cells = None;
                caret = Some((r0, c0));
            }
            _ => return Ok(Change::None),
        }
        if delete_shape {
            self.end_text();
            self.apply_with(cmd_id(cmd), |p| kiln_deck::edit::delete_shapes(p, &[id]))?;
            self.view.shapes.clear();
            return Ok(Change::Content);
        }
        if t == old {
            return Ok(Change::None);
        }
        let tx = Transaction::new(vec![Op::SetShapeKind {
            shape: id,
            kind: Box::new(ShapeKind::Table(t)),
        }]);
        self.apply(cmd_id(cmd), tx)?;
        if let Some((r, c)) = caret {
            let target = TextTarget::Cell {
                shape: id,
                row: r,
                col: c,
            };
            if self.body(target).is_some() {
                self.begin_text(target, None, false)?;
            }
        }
        Ok(Change::Content)
    }

    /// Shape formatting commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn format_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        let ids = self.view.shapes.clone();
        let label = cmd.meta().id;
        match cmd {
            D::SetFill { fill } => {
                if ids.is_empty() {
                    return Err(KilnError::unavailable(label, "select an object first"));
                }
                let part = self.picture_part(fill);
                let name = part.as_ref().map(|(n, _)| n.clone());
                let f = fill_of(fill, name.as_deref());
                let mut ops = part.map(|(_, op)| vec![op]).unwrap_or_default();
                let set = kiln_deck::edit::set_shape_props(&mut self.pres, &ids, |p| {
                    p.fill = Some(f.clone())
                })
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                ops.extend(set.ops);
                self.apply(label, Transaction::new(ops))?;
                Ok(Change::Content)
            }
            D::SetOutline { outline } => {
                if ids.is_empty() {
                    return Err(KilnError::unavailable(label, "select an object first"));
                }
                let base = |doc: &Self, id: NodeId| -> Line {
                    doc.pres
                        .resolved_props(id)
                        .and_then(|p| p.line)
                        .unwrap_or_default()
                };
                let mut ops = Vec::new();
                for id in &ids {
                    let mut l = base(self, *id);
                    match outline {
                        DeckOutline::None => l.fill = Some(Fill::None),
                        DeckOutline::Color { color } => {
                            l.fill = Some(Fill::Solid(drawing_color(color, None, None)));
                            if l.width.is_none() {
                                l.width = Some(12_700);
                            }
                        }
                        DeckOutline::Weight { points } => {
                            l.width = Some(
                                kiln_deck_layout::geom::emu(f64::from(*points) * 4.0 / 3.0).max(0),
                            );
                            if matches!(l.fill, Some(Fill::None) | None) {
                                l.fill = Some(Fill::Solid(DrawingColor::scheme("tx1")));
                            }
                        }
                        DeckOutline::Dash { dash } => {
                            l.dash = Some(LineDash::Preset(
                                PresetLineDashVal::parse(dash).unwrap_or(PresetLineDashVal::Solid),
                            ));
                        }
                        DeckOutline::Arrows { head, tail } => {
                            let end = |s: &str| {
                                LineEndType::parse(s)
                                    .filter(|t| *t != LineEndType::None)
                                    .map(|t| LineEnd {
                                        end_type: t,
                                        w: None,
                                        len: None,
                                    })
                            };
                            l.head = end(head);
                            l.tail = end(tail);
                        }
                    }
                    let tx = kiln_deck::edit::set_shape_props(&mut self.pres, &[*id], |p| {
                        p.line = Some(l.clone())
                    })
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                    ops.extend(tx.ops);
                }
                self.apply(label, Transaction::new(ops))?;
                Ok(Change::Content)
            }
            D::SetEffect { effect } => {
                if ids.is_empty() {
                    return Err(KilnError::unavailable(label, "select an object first"));
                }
                let mut ops = Vec::new();
                for id in &ids {
                    let mut fx = self
                        .pres
                        .resolved_props(*id)
                        .and_then(|p| p.effects)
                        .unwrap_or_default();
                    let emu = |pt: f32| kiln_deck_layout::geom::emu(f64::from(pt) * 4.0 / 3.0);
                    match effect {
                        DeckEffect::None => fx = EffectList::default(),
                        DeckEffect::NoShadow => fx.outer_shadow = None,
                        DeckEffect::Shadow {
                            distance,
                            blur,
                            angle,
                        } => {
                            fx.outer_shadow = Some(OuterShadow {
                                blur_rad: emu(*blur),
                                dist: emu(*distance),
                                dir: f64::from(*angle),
                                sx: 1.0,
                                sy: 1.0,
                                kx: 0.0,
                                ky: 0.0,
                                algn: RectAlignment::Tl,
                                rot_with_shape: false,
                                color: DrawingColor::srgb("000000").with("alpha", 40_000),
                            });
                        }
                        DeckEffect::Glow { radius } => {
                            fx.glow = Some(Glow {
                                rad: emu(*radius),
                                color: DrawingColor::scheme("accent1").with("alpha", 60_000),
                            });
                        }
                        DeckEffect::SoftEdges { radius } => fx.soft_edge = Some(emu(*radius)),
                    }
                    let tx = kiln_deck::edit::set_shape_props(&mut self.pres, &[*id], |p| {
                        p.effects = Some(fx.clone())
                    })
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                    ops.extend(tx.ops);
                }
                self.apply(label, Transaction::new(ops))?;
                Ok(Change::Content)
            }
            D::QuickStyle { index } => {
                if ids.is_empty() {
                    return Err(KilnError::unavailable(label, "select an object first"));
                }
                let accent = format!("accent{}", index % 6 + 1);
                let row = (index / 6).min(6);
                let r = |idx: u32, c: DrawingColor| StyleRef {
                    idx,
                    color: Some(c),
                };
                let acc = DrawingColor::scheme(&accent);
                let style = ShapeStyle {
                    line: match row {
                        0 | 6 => r(2, acc.clone()),
                        1 => r(2, acc.clone().with("shade", 50_000)),
                        _ => r(1, DrawingColor::scheme("lt1")),
                    },
                    fill: match row {
                        0 => r(1, DrawingColor::scheme("lt1")),
                        3 => r(2, acc.clone()),
                        4 | 5 => r(3, acc.clone()),
                        6 => r(1, acc.clone().with("tint", 20_000)),
                        _ => r(1, acc.clone()),
                    },
                    effect: match row {
                        4 => r(2, acc.clone()),
                        5 => r(3, acc.clone()),
                        _ => r(0, acc.clone()),
                    },
                    font: FontCollectionRef::Minor,
                    font_color: Some(match row {
                        0 | 6 => acc.clone(),
                        _ => DrawingColor::scheme("lt1"),
                    }),
                };
                self.apply_with(label, |p| {
                    kiln_deck::edit::set_shape_props(p, &ids, |sp| {
                        sp.style = Some(style.clone());
                        sp.fill = None;
                        sp.line = None;
                        sp.effects = None;
                    })
                })?;
                Ok(Change::Content)
            }
            D::AlignText { anchor } => {
                let a = match anchor {
                    DeckAnchor::Top => TextAnchor::Top,
                    DeckAnchor::Middle => TextAnchor::Middle,
                    DeckAnchor::Bottom => TextAnchor::Bottom,
                };
                self.body_props(label, |b| b.anchor = Some(a))
            }
            D::TextDirection { direction } => {
                let d = match direction {
                    DeckTextDirection::Horizontal => TextDirection::Horizontal,
                    DeckTextDirection::Rotate90 => TextDirection::Rotate90,
                    DeckTextDirection::Rotate270 => TextDirection::Rotate270,
                    DeckTextDirection::Stacked => TextDirection::Stacked,
                };
                self.body_props(label, |b| b.direction = Some(d))
            }
            D::Columns { count, spacing } => {
                let n = (*count).clamp(1, 16);
                let gap = kiln_deck_layout::geom::emu(f64::from(*spacing) * 4.0 / 3.0).max(0);
                self.body_props(label, |b| {
                    b.columns = Some(n);
                    b.column_spacing = Some(gap);
                })
            }
            D::Autofit { mode } => {
                let a = match mode {
                    DeckAutofit::Off => Autofit::Off,
                    DeckAutofit::Shrink => Autofit::SHRINK,
                    DeckAutofit::ResizeShape => Autofit::ResizeShape,
                };
                self.body_props(label, |b| b.autofit = Some(a))
            }
            D::CharacterSpacing { points } => {
                #[allow(clippy::cast_possible_truncation)]
                let tw = (f64::from(points.clamp(-100.0, 1584.0)) * 20.0).round() as i32;
                self.format_runs(label, |p| p.spacing = (tw != 0).then_some(tw).or(Some(0)))
            }
            D::TextShadow { on } => {
                let on = on
                    .unwrap_or_else(|| self.effective_run().is_none_or(|r| r.shadow != Some(true)));
                self.format_runs(label, |p| p.shadow = Some(on))
            }
            _ => Ok(Change::None),
        }
    }

    /// Changes body properties of the edited text or of the selected shapes' text.
    fn body_props(
        &mut self,
        label: &'static str,
        f: impl Fn(&mut kiln_deck::BodyProps),
    ) -> Result<Change> {
        let targets: Vec<TextTarget> = match &self.view.text {
            Some(t) => vec![t.target],
            None => self
                .view
                .shapes
                .iter()
                .filter(|id| self.pres.shape(**id).and_then(|s| s.text()).is_some())
                .map(|id| TextTarget::Shape(*id))
                .collect(),
        };
        if targets.is_empty() {
            return Err(KilnError::unavailable(
                label,
                "select a shape with text first",
            ));
        }
        self.begin_group();
        let mut result = Ok(());
        for t in targets {
            match kiln_deck::edit::set_body_props(&self.pres, t, &f) {
                Ok(tx) => {
                    if let Err(e) = self.apply(label, tx) {
                        result = Err(e);
                    }
                }
                Err(e) => result = Err(KilnError::InvalidInput(e.to_string())),
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// The Format Painter: copy the selection's look, paste it on the next selection.
    pub(crate) fn format_painter(&mut self, cmd: &DeckCommand) -> Result<Change> {
        match cmd {
            DeckCommand::CopyFormat => {
                let shape = if self.view.text.is_none() {
                    self.view
                        .shapes
                        .first()
                        .and_then(|id| self.pres.resolved_props(*id))
                } else {
                    None
                };
                let run = self.effective_run().map(|mut r| {
                    r.lang = None;
                    r
                });
                self.format_clip = Some(FormatClip { shape, run });
                Ok(Change::Selection)
            }
            DeckCommand::PasteFormat => {
                let Some(clip) = self.format_clip.clone() else {
                    return Err(KilnError::unavailable(
                        "deck.clipboard.paste_format",
                        "copy formatting first",
                    ));
                };
                self.begin_group();
                let mut result = Ok(Change::None);
                if let (Some(props), None) = (&clip.shape, &self.view.text) {
                    let ids = self.view.shapes.clone();
                    let p = props.clone();
                    let r = self.apply_with("deck.clipboard.paste_format", |pres| {
                        kiln_deck::edit::set_shape_props(pres, &ids, |sp| {
                            sp.fill = p.fill.clone();
                            sp.line = p.line.clone();
                            sp.effects = p.effects.clone();
                            sp.style = p.style.clone();
                        })
                    });
                    if let Err(e) = r {
                        result = Err(e);
                    }
                }
                if let Some(run) = &clip.run {
                    let run = run.clone();
                    match self.format_runs("deck.clipboard.paste_format", |p| overlay(p, &run)) {
                        Ok(_) => {}
                        Err(e) => result = Err(e),
                    }
                }
                self.end_group();
                result?;
                Ok(Change::Content)
            }
            _ => Ok(Change::None),
        }
    }
}

/// The catalog id of a shared editing command.
fn cmd_id(cmd: &EditCommand) -> &'static str {
    kiln_protocol::Command::Suite(kiln_protocol::SuiteCommand::Edit(cmd.clone())).id()
}

/// Bullets, numbering, spacing and the anchor of a resolved paragraph and body (the ribbon's
/// state).
pub(crate) fn bullet_kind(b: Option<&Bullet>) -> (bool, bool) {
    match b {
        Some(Bullet::Char(_) | Bullet::Picture(_)) => (true, false),
        Some(Bullet::AutoNumber { .. }) => (false, true),
        _ => (false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_map_to_the_nearest_colour() {
        assert_eq!(
            nearest_highlight(Color::rgb(250, 240, 10)),
            HighlightColor::Yellow
        );
        assert_eq!(
            nearest_highlight(Color::rgb(10, 200, 220)),
            HighlightColor::Cyan
        );
    }

    #[test]
    fn grow_steps_follow_powerpoint() {
        let next = SIZES.iter().copied().find(|v| *v > 18.0 + 0.01);
        assert_eq!(next, Some(20.0));
    }
}
