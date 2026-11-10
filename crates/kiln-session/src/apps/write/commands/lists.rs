// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lists: Bullets and Numbering (toggles that continue the list just above, as Word does),
//! Multilevel lists, levels (Tab / Shift+Tab, Increase / Decrease Indent), Restart Numbering,
//! Continue Numbering and Set Numbering Value. List definitions are added with the paragraph
//! changes in one undo step.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::st::NumberFormat;
use kiln_doc::{EditKind, Numbering, NumberingRef, Op, Range, Transaction};
use kiln_protocol::commands::ListNumberStyle;

use super::doc_err;
use crate::apps::write::WriteDoc;

/// The list kind a paragraph's list has.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Bullet(String),
    Number(NumberFormat, String),
}

fn number_format(style: ListNumberStyle) -> (NumberFormat, &'static str) {
    match style {
        ListNumberStyle::Decimal => (NumberFormat::Decimal, "%1."),
        ListNumberStyle::DecimalParen => (NumberFormat::Decimal, "%1)"),
        ListNumberStyle::UpperRoman => (NumberFormat::UpperRoman, "%1."),
        ListNumberStyle::UpperLetter => (NumberFormat::UpperLetter, "%1."),
        ListNumberStyle::LowerLetterParen => (NumberFormat::LowerLetter, "%1)"),
        ListNumberStyle::LowerLetter => (NumberFormat::LowerLetter, "%1."),
        ListNumberStyle::LowerRoman => (NumberFormat::LowerRoman, "%1."),
    }
}

impl WriteDoc {
    /// The resolved list of a paragraph.
    pub(super) fn list_of(&self, id: NodeId) -> Option<(u32, u8)> {
        let p = self.doc.paragraph(id)?;
        self.doc
            .resolve_paragraph(p)
            .numbering
            .map(|l| (l.num_id, l.level))
    }

    fn kind_of(&self, num_id: u32) -> Option<Kind> {
        let lvl = self
            .doc
            .numbering
            .level(num_id, 0, Some(&self.doc.styles))?;
        Some(if lvl.format == NumberFormat::Bullet {
            Kind::Bullet(lvl.text.clone())
        } else {
            Kind::Number(lvl.format.clone(), lvl.text.clone())
        })
    }

    /// Whether a list is bulleted.
    pub(super) fn is_bullet_list(&self, num_id: u32) -> bool {
        matches!(self.kind_of(num_id), Some(Kind::Bullet(_)))
    }

    /// The paragraph before `id` in its story.
    fn previous_paragraph(&self, id: NodeId) -> Option<NodeId> {
        let order = self.story_paragraphs(id);
        let i = order.iter().position(|p| *p == id)?;
        order.get(i.checked_sub(1)?).copied()
    }

    /// Adds the list `make` creates to the document (in the batch) and returns its `numId`.
    fn add_list(
        &mut self,
        batch: &mut super::Batch,
        make: impl FnOnce(&mut Numbering) -> u32,
    ) -> Result<u32> {
        let mut n = self.doc.numbering.clone();
        let id = make(&mut n);
        let inst = n
            .instances
            .get(&id)
            .cloned()
            .ok_or_else(|| KilnError::Internal("a new list has no instance".into()))?;
        let abs = n
            .abstracts
            .get(&inst.abstract_id)
            .cloned()
            .ok_or_else(|| KilnError::Internal("a new list has no definition".into()))?;
        self.batch_apply(
            batch,
            Transaction::new(vec![
                Op::SetAbstractNum {
                    id: abs.id,
                    def: Some(Box::new(abs)),
                },
                Op::SetNumInstance {
                    id,
                    instance: Some(inst),
                },
            ]),
        )?;
        Ok(id)
    }

    /// Puts paragraphs into list `num_id` (keeping their levels when they are in a list).
    fn assign(&mut self, batch: &mut super::Batch, ids: &[NodeId], num_id: u32) -> Result<()> {
        let list_paragraph = self.doc.styles.contains("ListParagraph");
        let default = self
            .doc
            .styles
            .default_style(kiln_doc::st::StyleType::Paragraph)
            .map(|s| s.id.clone());
        let mut ops = Vec::new();
        for id in ids {
            let level = self.list_of(*id).map_or(0, |(_, l)| l);
            let p = self.para(*id)?;
            let mut props = p.props.clone();
            props.numbering = Some(NumberingRef::new(num_id, level));
            // Word puts list items in List Paragraph and lets the list indent them.
            if list_paragraph && (props.style_id.is_none() || props.style_id == default) {
                props.style_id = Some("ListParagraph".into());
            }
            if let Some(ind) = &mut props.indent {
                ind.left = None;
                ind.hanging = None;
                ind.first_line = None;
                if ind.right.is_none() {
                    props.indent = None;
                }
            }
            ops.push(Op::SetParagraphProps {
                para: *id,
                props: Box::new(props),
            });
        }
        self.batch_apply(batch, Transaction::new(ops))
    }

    /// Takes paragraphs out of their lists.
    pub(super) fn remove_numbering(&mut self, ids: &[NodeId]) -> Result<()> {
        let default = self
            .doc
            .styles
            .default_style(kiln_doc::st::StyleType::Paragraph)
            .map(|s| s.id.clone());
        let mut ops = Vec::new();
        for id in ids {
            let p = self.para(*id)?;
            let mut props = p.props.clone();
            let from_style = self
                .doc
                .styles
                .style_numbering(props.style_id.as_deref())
                .is_some();
            props.numbering = from_style.then_some(NumberingRef {
                num_id: Some(0),
                level: None,
            });
            if props.style_id.as_deref() == Some("ListParagraph") {
                props.style_id.clone_from(&default);
            }
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

    /// Applies a list kind to the selection, continuing the list just above when it has the same
    /// kind; removes the list when every paragraph already has that kind (toggle).
    fn apply_list(
        &mut self,
        toggle_off: bool,
        matches: impl Fn(&Kind) -> bool,
        make: impl FnOnce(&mut Numbering) -> u32,
    ) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let Some(first) = ids.first().copied() else {
            return Ok(());
        };
        let all_same = ids.iter().all(|id| {
            self.list_of(*id)
                .and_then(|(n, _)| self.kind_of(n))
                .is_some_and(|k| matches(&k))
        });
        if all_same && toggle_off {
            return self.remove_numbering(&ids);
        }
        self.ensure_style("ListParagraph")?;
        let mut batch = self.batch(EditKind::Format);
        let above = self
            .previous_paragraph(first)
            .and_then(|p| self.list_of(p))
            .filter(|(n, _)| self.kind_of(*n).is_some_and(|k| matches(&k)))
            .map(|(n, _)| n);
        let num_id = match above {
            Some(n) => n,
            None => self.add_list(&mut batch, make)?,
        };
        self.assign(&mut batch, &ids, num_id)?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Bullets (Ctrl+Shift+L): Word's bullet list, or one with `glyph` at the first level.
    pub(super) fn bullets(&mut self, glyph: Option<&str>) -> Result<()> {
        let glyph = glyph.map(str::trim).filter(|g| !g.is_empty());
        if glyph.is_some_and(|g| g.chars().count() > 4) {
            return Err(KilnError::InvalidInput("a bullet is one symbol".into()));
        }
        let want = glyph.map(str::to_owned);
        let want2 = want.clone();
        self.apply_list(
            want.is_none(),
            move |k| match (k, &want) {
                (Kind::Bullet(_), None) => true,
                (Kind::Bullet(t), Some(g)) => t == g,
                _ => false,
            },
            move |n| {
                let id = n.create_bullet_list().id;
                if let Some(g) = &want2
                    && let Some(abs_id) = n.instances.get(&id).map(|i| i.abstract_id)
                    && let Some(abs) = n.abstracts.get_mut(&abs_id)
                    && let Some(l0) = abs.levels.first_mut()
                {
                    l0.text.clone_from(g);
                    l0.rpr = None;
                }
                id
            },
        )
    }

    /// Numbering: Word's numbered list, or one in `style`.
    pub(super) fn numbering(&mut self, style: Option<ListNumberStyle>) -> Result<()> {
        let (format, text) = number_format(style.unwrap_or(ListNumberStyle::Decimal));
        let explicit = style.is_some();
        let (f2, t2) = (format.clone(), text.to_owned());
        self.apply_list(
            !explicit,
            move |k| match k {
                Kind::Number(f, t) => !explicit || (*f == format && t == text),
                Kind::Bullet(_) => false,
            },
            move |n| {
                let id = n.create_numbered_list().id;
                if explicit
                    && let Some(abs_id) = n.instances.get(&id).map(|i| i.abstract_id)
                    && let Some(abs) = n.abstracts.get_mut(&abs_id)
                    && let Some(l0) = abs.levels.first_mut()
                {
                    l0.format = f2;
                    l0.text = t2;
                }
                id
            },
        )
    }

    /// Multilevel list (`1. 1.1. 1.1.1.`), legal numbering optional.
    pub(super) fn multilevel(&mut self, legal: bool) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        self.ensure_style("ListParagraph")?;
        let mut batch = self.batch(EditKind::Format);
        let num_id = self.add_list(&mut batch, |n| n.create_multilevel_list(legal).id)?;
        self.assign(&mut batch, &ids, num_id)?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Moves list paragraphs `delta` levels (clamped to 0–8).
    pub(super) fn shift_list_level(&mut self, ids: &[NodeId], delta: i32) -> Result<()> {
        let mut ops = Vec::new();
        for id in ids {
            let Some((num, level)) = self.list_of(*id) else {
                continue;
            };
            let next = kiln_doc::numbering::clamp_level(i64::from(level) + i64::from(delta));
            if next == level {
                continue;
            }
            let p = self.para(*id)?;
            let mut props = p.props.clone();
            props.numbering = Some(NumberingRef::new(num, next));
            ops.push(Op::SetParagraphProps {
                para: *id,
                props: Box::new(props),
            });
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Sets the level of the selected list paragraphs.
    pub(super) fn set_list_level(&mut self, level: u8) -> Result<()> {
        if level > kiln_doc::numbering::MAX_LEVEL {
            return Err(KilnError::InvalidInput(format!(
                "list levels are 1–9, not {}",
                u32::from(level) + 1
            )));
        }
        let ids = self.selected_paragraphs()?;
        let mut ops = Vec::new();
        for id in ids {
            let Some((num, _)) = self.list_of(id) else {
                continue;
            };
            let mut props = self.para(id)?.props.clone();
            props.numbering = Some(NumberingRef::new(num, level));
            ops.push(Op::SetParagraphProps {
                para: id,
                props: Box::new(props),
            });
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// The caret's paragraph and the following ones in the same list.
    fn list_run_from_caret(&self) -> Option<(u32, u8, Vec<NodeId>)> {
        let (s, _) = self.span();
        let (num, level) = self.list_of(s.para)?;
        let order = self.story_paragraphs(s.para);
        let start = order.iter().position(|p| *p == s.para)?;
        let ids = order[start..]
            .iter()
            .copied()
            .filter(|p| self.list_of(*p).is_some_and(|(n, _)| n == num))
            .collect();
        Some((num, level, ids))
    }

    /// Restart Numbering at 1 (or Set Numbering Value) from the caret's list item.
    pub(super) fn restart_numbering(&mut self, value: Option<u32>) -> Result<()> {
        let Some((num, level, ids)) = self.list_run_from_caret() else {
            return Ok(());
        };
        if value == Some(0) {
            return Err(KilnError::InvalidInput(
                "numbering starts at 1 or more".into(),
            ));
        }
        let mut batch = self.batch(EditKind::Format);
        let start = value.map(i64::from);
        let mut make_err = None;
        let new_id = self.add_list(&mut batch, |n| match n.restart(num, level, start) {
            Ok(inst) => inst.id,
            Err(e) => {
                make_err = Some(e);
                0
            }
        });
        if let Some(e) = make_err {
            return Err(doc_err(e));
        }
        let new_id = new_id?;
        let mut ops = Vec::new();
        for id in ids {
            let mut props = self.para(id)?.props.clone();
            let lvl = self.list_of(id).map_or(level, |(_, l)| l);
            props.numbering = Some(NumberingRef::new(new_id, lvl));
            ops.push(Op::SetParagraphProps {
                para: id,
                props: Box::new(props),
            });
        }
        self.batch_apply(&mut batch, Transaction::new(ops))?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Continue Numbering: the caret's list joins the nearest list of the same kind above it.
    pub(super) fn continue_numbering(&mut self) -> Result<()> {
        let Some((num, _, ids)) = self.list_run_from_caret() else {
            return Ok(());
        };
        let Some(kind) = self.kind_of(num) else {
            return Ok(());
        };
        let (s, _) = self.span();
        let order = self.story_paragraphs(s.para);
        let Some(start) = order.iter().position(|p| *p == s.para) else {
            return Ok(());
        };
        let previous = order[..start].iter().rev().find_map(|p| {
            let (n, _) = self.list_of(*p)?;
            (n != num
                && self
                    .kind_of(n)
                    .is_some_and(|k| std::mem::discriminant(&k) == std::mem::discriminant(&kind)))
            .then_some(n)
        });
        let Some(target) = previous else {
            return Ok(());
        };
        let mut ops = Vec::new();
        for id in ids {
            let lvl = self.list_of(id).map_or(0, |(_, l)| l);
            let mut props = self.para(id)?.props.clone();
            props.numbering = Some(NumberingRef::new(target, lvl));
            ops.push(Op::SetParagraphProps {
                para: id,
                props: Box::new(props),
            });
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }
}
