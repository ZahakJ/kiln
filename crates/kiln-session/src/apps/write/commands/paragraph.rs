// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paragraph formatting: alignment (Ctrl+L/E/R/J toggle back to left like Word), line spacing,
//! indents (Ctrl+M steps to the next default tab stop), spacing, the Paragraph and Tabs dialogs,
//! shading, borders, clearing paragraph formatting, moving paragraphs up and down and
//! promoting or demoting headings.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::st::{Alignment, BorderStyle, LineRule, TabAlignment, TabLeader};
use kiln_doc::{
    Block, Border, EditKind, Indent, Op, ParagraphBorders, ParagraphProps, Range,
    ResolvedParagraph, Shading, Spacing, TabStop, Transaction,
};
use kiln_protocol::commands::{
    BorderPreset, ParagraphFormat, TabAlign, TabLeaderKind, TabStopSpec, TextAlign,
};

use super::doc_err;
use super::format::doc_color;
use crate::apps::write::WriteDoc;

/// Points to twips.
pub(super) fn twips(pt: f32) -> Result<i32> {
    if !pt.is_finite() || pt.abs() > 1584.0 * 2.0 {
        return Err(KilnError::InvalidInput(format!(
            "{pt} pt is not a length Kiln accepts (±3168 pt)"
        )));
    }
    Ok((pt * 20.0).round() as i32)
}

/// Twips to points.
pub(super) fn points(tw: i32) -> f32 {
    tw as f32 / 20.0
}

/// The model's alignment for the protocol's.
pub(super) fn alignment(a: TextAlign) -> Alignment {
    match a {
        TextAlign::Left => Alignment::Left,
        TextAlign::Center => Alignment::Center,
        TextAlign::Right => Alignment::Right,
        TextAlign::Justify => Alignment::Both,
    }
}

/// The protocol's alignment for the model's.
pub(super) fn text_align(a: &Alignment) -> TextAlign {
    match a {
        Alignment::Center => TextAlign::Center,
        Alignment::Right => TextAlign::Right,
        Alignment::Left => TextAlign::Left,
        _ => TextAlign::Justify,
    }
}

/// A single ½-point border.
pub(super) fn single_border() -> Border {
    Border {
        space: Some(1),
        color: Some(kiln_doc::DocColor::auto()),
        ..Border::new(BorderStyle::Single, 4)
    }
}

/// The model's tab alignment for the protocol's.
pub(super) fn tab_alignment(a: TabAlign) -> TabAlignment {
    match a {
        TabAlign::Left => TabAlignment::Left,
        TabAlign::Center => TabAlignment::Center,
        TabAlign::Right => TabAlignment::Right,
        TabAlign::Decimal => TabAlignment::Decimal,
        TabAlign::Bar => TabAlignment::Bar,
    }
}

/// The protocol's tab alignment for the model's.
pub(super) fn tab_align(a: &TabAlignment) -> TabAlign {
    match a {
        TabAlignment::Center => TabAlign::Center,
        TabAlignment::Right => TabAlign::Right,
        TabAlignment::Decimal => TabAlign::Decimal,
        TabAlignment::Bar => TabAlign::Bar,
        _ => TabAlign::Left,
    }
}

impl WriteDoc {
    /// Changes the direct properties of the selected paragraphs; `f` also sees their effective
    /// properties.
    pub(super) fn format_paras(
        &mut self,
        f: impl Fn(&mut ParagraphProps, &ResolvedParagraph),
    ) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        self.format_para_ids(&ids, f)
    }

    /// Changes the direct properties of the given paragraphs as one undo step.
    pub(super) fn format_para_ids(
        &mut self,
        ids: &[NodeId],
        f: impl Fn(&mut ParagraphProps, &ResolvedParagraph),
    ) -> Result<()> {
        let mut ops = Vec::new();
        for id in ids {
            let p = self.para(*id)?;
            let resolved = self.doc.resolve_paragraph(p);
            let mut props = p.props.clone();
            f(&mut props, &resolved);
            if props != p.props {
                ops.push(Op::SetParagraphProps {
                    para: *id,
                    props: Box::new(props),
                });
            }
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Ctrl+L/E/R/J: a second press of Centre, Right or Justify goes back to left.
    pub(super) fn align(&mut self, target: Alignment) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let all = ids.iter().all(|id| {
            self.doc
                .paragraph(*id)
                .is_some_and(|p| self.doc.resolve_paragraph(p).alignment == target)
        });
        let value = if all && target != Alignment::Left {
            Alignment::Left
        } else {
            target
        };
        self.format_para_ids(&ids, |p, _| p.alignment = Some(value))
    }

    /// Line spacing as a multiple of single.
    pub(super) fn line_spacing(&mut self, multiple: f32) -> Result<()> {
        if !multiple.is_finite() || !(0.06..=132.0).contains(&multiple) {
            return Err(KilnError::InvalidInput(format!(
                "line spacing {multiple} is outside 0.06–132 lines"
            )));
        }
        let line = (multiple * 240.0).round() as i32;
        self.format_paras(|p, _| {
            let mut s = p.spacing.unwrap_or_default();
            s.line = Some(line);
            s.line_rule = Some(LineRule::Auto);
            p.spacing = Some(s);
        })
    }

    /// Increase/Decrease Indent: list items change level; other paragraphs move to the next
    /// (previous) multiple of the default tab stop, never below zero.
    pub(super) fn indent(&mut self, increase: bool) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let lists: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc
                    .paragraph(*id)
                    .is_some_and(|p| self.doc.resolve_paragraph(p).numbering.is_some())
            })
            .collect();
        if !lists.is_empty() && lists.len() == ids.len() {
            return self.shift_list_level(&ids, if increase { 1 } else { -1 });
        }
        let step = self.doc.settings.default_tab_stop.max(1);
        self.format_para_ids(&ids, move |p, r| {
            let left = r.indent.left;
            let next = if increase {
                (left.div_euclid(step) + 1) * step
            } else {
                ((left + step - 1).div_euclid(step) - 1).max(0) * step
            };
            let mut ind = p.indent.unwrap_or_default();
            ind.left = Some(next.clamp(0, 31_680));
            p.indent = Some(ind);
        })
    }

    /// Layout ▸ Indent Left / Right in centimetres.
    pub(super) fn indent_side(&mut self, left: bool, cm: f32) -> Result<()> {
        let tw = twips(cm * 72.0 / 2.54)?;
        self.format_paras(move |p, _| {
            let mut ind = p.indent.unwrap_or_default();
            if left {
                ind.left = Some(tw);
            } else {
                ind.right = Some(tw);
            }
            p.indent = Some(ind);
        })
    }

    /// Layout ▸ Spacing Before / After in points.
    pub(super) fn spacing_side(&mut self, before: bool, pt: f32) -> Result<()> {
        if pt < 0.0 {
            return Err(KilnError::InvalidInput("spacing cannot be negative".into()));
        }
        let tw = twips(pt)?;
        self.format_paras(move |p, _| {
            let mut s = p.spacing.unwrap_or_default();
            if before {
                s.before = Some(tw);
                s.before_autospacing = None;
            } else {
                s.after = Some(tw);
                s.after_autospacing = None;
            }
            p.spacing = Some(s);
        })
    }

    /// The Paragraph dialog.
    pub(super) fn set_paragraph(&mut self, f: &ParagraphFormat) -> Result<()> {
        let tw = |v: Option<f32>| v.map(twips).transpose();
        let (left, right, first) = (tw(f.indent_left)?, tw(f.indent_right)?, tw(f.first_line)?);
        let (before, after) = (tw(f.space_before)?, tw(f.space_after)?);
        let (exact, at_least) = (tw(f.line_exact)?, tw(f.line_at_least)?);
        if let Some(m) = f.line_multiple
            && (!m.is_finite() || !(0.06..=132.0).contains(&m))
        {
            return Err(KilnError::InvalidInput(format!(
                "line spacing {m} is outside 0.06–132 lines"
            )));
        }
        if f.outline_level.is_some_and(|l| l > 9) {
            return Err(KilnError::InvalidInput("outline levels are 1–9".into()));
        }
        let f = f.clone();
        self.format_paras(move |p, _| {
            if let Some(a) = f.alignment {
                p.alignment = Some(alignment(a));
            }
            if left.is_some() || right.is_some() || first.is_some() {
                let mut ind: Indent = p.indent.unwrap_or_default();
                if let Some(v) = left {
                    ind.left = Some(v);
                }
                if let Some(v) = right {
                    ind.right = Some(v);
                }
                if let Some(v) = first {
                    if v < 0 {
                        ind.hanging = Some(-v);
                        ind.first_line = None;
                    } else {
                        ind.first_line = Some(v);
                        ind.hanging = None;
                    }
                }
                p.indent = Some(ind);
            }
            if before.is_some()
                || after.is_some()
                || exact.is_some()
                || at_least.is_some()
                || f.line_multiple.is_some()
            {
                let mut s: Spacing = p.spacing.unwrap_or_default();
                if let Some(v) = before {
                    s.before = Some(v.max(0));
                }
                if let Some(v) = after {
                    s.after = Some(v.max(0));
                }
                if let Some(v) = exact {
                    s.line = Some(v.max(1));
                    s.line_rule = Some(LineRule::Exact);
                } else if let Some(v) = at_least {
                    s.line = Some(v.max(1));
                    s.line_rule = Some(LineRule::AtLeast);
                } else if let Some(m) = f.line_multiple {
                    s.line = Some((m * 240.0).round() as i32);
                    s.line_rule = Some(LineRule::Auto);
                }
                p.spacing = Some(s);
            }
            if let Some(v) = f.keep_next {
                p.keep_next = Some(v);
            }
            if let Some(v) = f.keep_lines {
                p.keep_lines = Some(v);
            }
            if let Some(v) = f.widow_control {
                p.widow_control = Some(v);
            }
            if let Some(v) = f.page_break_before {
                p.page_break_before = Some(v);
            }
            if let Some(v) = f.contextual_spacing {
                p.contextual_spacing = Some(v);
            }
            if let Some(l) = f.outline_level {
                // The dialog's 1–9; 0 is body text (stored as 9).
                p.outline_level = Some(if l == 0 { 9 } else { l - 1 });
            }
            if let Some(v) = f.rtl {
                p.bidi = Some(v);
            }
        })
    }

    /// The Tabs dialog: the given stops replace the paragraphs' own; inherited stops not listed
    /// are cleared.
    pub(super) fn set_tabs(&mut self, tabs: &[TabStopSpec]) -> Result<()> {
        let mut stops = Vec::with_capacity(tabs.len());
        for t in tabs {
            let pos = twips(t.pos)?;
            stops.push(TabStop {
                pos,
                align: tab_alignment(t.align),
                leader: match t.leader {
                    TabLeaderKind::None => None,
                    TabLeaderKind::Dot => Some(TabLeader::Dot),
                    TabLeaderKind::Hyphen => Some(TabLeader::Hyphen),
                    TabLeaderKind::Underscore => Some(TabLeader::Underscore),
                },
            });
        }
        stops.sort_by_key(|s| s.pos);
        stops.dedup_by_key(|s| s.pos);
        self.format_paras(move |p, r| {
            let mut own = stops.clone();
            for inherited in &r.tabs {
                let from_style = p
                    .tabs
                    .as_ref()
                    .is_none_or(|t| !t.iter().any(|x| x.pos == inherited.pos));
                if from_style && !own.iter().any(|s| s.pos == inherited.pos) {
                    own.push(TabStop::new(inherited.pos, TabAlignment::Clear));
                }
            }
            own.sort_by_key(|s| s.pos);
            p.tabs = Some(own);
        })
    }

    /// Paragraph shading (`None` removes it).
    pub(super) fn paragraph_shading(&mut self, color: Option<&str>) -> Result<()> {
        let shading = color.map(doc_color).transpose()?.map(Shading::fill);
        self.format_paras(move |p, _| p.shading.clone_from(&shading))
    }

    /// Paragraph borders.
    pub(super) fn paragraph_borders(&mut self, preset: BorderPreset) -> Result<()> {
        self.format_paras(move |p, _| {
            let mut b = p.borders.clone().unwrap_or_default();
            let line = Some(single_border());
            match preset {
                BorderPreset::None => b = ParagraphBorders::default(),
                BorderPreset::All | BorderPreset::Outside => {
                    b.top.clone_from(&line);
                    b.bottom.clone_from(&line);
                    b.left.clone_from(&line);
                    b.right.clone_from(&line);
                    if preset == BorderPreset::All {
                        b.between.clone_from(&line);
                    }
                }
                BorderPreset::Inside | BorderPreset::InsideHorizontal => {
                    b.between.clone_from(&line);
                }
                BorderPreset::InsideVertical => {}
                BorderPreset::Top => b.top.clone_from(&line),
                BorderPreset::Bottom => b.bottom.clone_from(&line),
                BorderPreset::Left => b.left.clone_from(&line),
                BorderPreset::Right => b.right.clone_from(&line),
            }
            p.borders = if b == ParagraphBorders::default() {
                None
            } else {
                Some(b)
            };
        })
    }

    /// Ctrl+Q: only the style's paragraph formatting remains (lists from direct formatting go).
    pub(super) fn clear_paragraph_formatting(&mut self) -> Result<()> {
        self.format_paras(|p, _| {
            *p = ParagraphProps {
                style_id: p.style_id.clone(),
                ..ParagraphProps::default()
            };
        })
    }

    /// Alt+Shift+Up/Down: the selected paragraphs swap with the paragraph (or table) before or
    /// after them in their container.
    pub(super) fn move_paragraphs(&mut self, down: bool) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let (Some(first), Some(last)) = (ids.first(), ids.last()) else {
            return Ok(());
        };
        let (Some((container, a)), Some((c2, b))) =
            (self.doc.parent_of(*first), self.doc.parent_of(*last))
        else {
            return Ok(());
        };
        if container != c2 {
            return Ok(());
        }
        let blocks = self.doc.container(container).map_err(doc_err)?;
        let (take, put) = if down {
            if b + 1 >= blocks.len() {
                return Ok(());
            }
            (b + 1, a)
        } else {
            if a == 0 {
                return Ok(());
            }
            (a - 1, b)
        };
        let moved: Block = blocks[take].clone();
        let mut batch = self.batch(EditKind::Structure);
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::RemoveBlocks {
                container,
                index: take,
                count: 1,
            }]),
        )?;
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::InsertBlocks {
                container,
                index: put,
                blocks: vec![moved],
            }]),
        )?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Alt+Shift+Left/Right: headings one level up or down; list items one level shallower or
    /// deeper.
    pub(super) fn promote(&mut self, up: bool) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let mut list_items = Vec::new();
        let mut headings = Vec::new();
        for id in &ids {
            let p = self.para(*id)?;
            let r = self.doc.resolve_paragraph(p);
            if r.numbering.is_some() {
                list_items.push(*id);
            } else if r.outline_level < 9 {
                headings.push((*id, r.outline_level));
            }
        }
        if !list_items.is_empty() {
            self.shift_list_level(&list_items, if up { -1 } else { 1 })?;
        }
        for (id, level) in headings {
            let next = if up {
                level.saturating_sub(1)
            } else {
                (level + 1).min(8)
            };
            if next != level {
                self.ensure_style(&format!("Heading{}", next + 1))?;
                let style = format!("Heading{}", next + 1);
                self.format_para_ids(&[id], move |p, _| p.style_id = Some(style.clone()))?;
            }
        }
        Ok(())
    }
}
