// SPDX-License-Identifier: AGPL-3.0-or-later
//! Styles: applying a paragraph, linked or character style (Word's built-in styles are added to
//! documents that do not define them yet), Update Style to Match Selection, New Style from
//! Formatting and the Modify Style dialog.

use kiln_common::{KilnError, Result};
use kiln_doc::merge::{merge_paragraph_props, merge_run_props};
use kiln_doc::st::StyleType;
use kiln_doc::{EditKind, Op, ParagraphProps, Range, RunProps, Style, Transaction, edit};
use kiln_protocol::commands::StyleSpec;

use super::doc_err;
use crate::apps::write::WriteDoc;

/// Word's built-in style for an id, with its linked character style.
fn builtin(id: &str) -> Vec<Style> {
    let all = kiln_doc::template::default_styles();
    let Some(s) = all.iter().find(|s| s.id == id).cloned() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(base) = &s.based_on
        && let Some(b) = all.iter().find(|x| x.id == *base)
    {
        out.push(b.clone());
    }
    if let Some(link) = &s.link
        && let Some(l) = all.iter().find(|x| x.id == *link)
    {
        out.push(l.clone());
    }
    out.insert(0, s);
    out
}

impl WriteDoc {
    /// Adds Word's built-in style `id` (and its base and linked style) when the document lacks
    /// it; returns whether the document has the style now.
    pub(super) fn ensure_style(&mut self, id: &str) -> Result<bool> {
        if self.doc.styles.contains(id) {
            return Ok(true);
        }
        let mut ops = Vec::new();
        let mut index = self.doc.styles.len();
        for s in builtin(id) {
            if !self.doc.styles.contains(&s.id) {
                ops.push(Op::PutStyle {
                    style: Box::new(s),
                    index,
                });
                index += 1;
            }
        }
        if ops.is_empty() {
            return Ok(false);
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )?;
        Ok(true)
    }

    /// Applies a style: paragraph and linked styles to the selected paragraphs (the paragraph
    /// style's own list replaces direct list formatting), character styles to the selected text
    /// or the word at the caret.
    pub(super) fn apply_style(&mut self, id: &str) -> Result<()> {
        let id = id.trim();
        if !self.ensure_style(id)? {
            return Err(KilnError::InvalidInput(format!("no style '{id}'")));
        }
        let kind = self.doc.styles.get(id).map(|s| s.kind).unwrap_or_default();
        match kind {
            StyleType::Character => {
                let style = id.to_owned();
                self.apply_char_style(style)
            }
            StyleType::Paragraph => {
                let style = id.to_owned();
                let style_has_list = self.doc.styles.style_numbering(Some(id)).is_some();
                self.format_paras(move |p, _| {
                    p.style_id = Some(style.clone());
                    if style_has_list {
                        p.numbering = None;
                    }
                })
            }
            other => Err(KilnError::InvalidInput(format!(
                "'{id}' is a {other} style; apply it from Table Design"
            ))),
        }
    }

    fn apply_char_style(&mut self, style: String) -> Result<()> {
        let (s, e) = self.span();
        if s == e {
            let text = self.para_text(s.para);
            let (a, b) = crate::apps::write::editing::word_at(&text, s.offset);
            if a < s.offset && s.offset < b {
                let sel = Range::new(self.anchor, self.head);
                let tx = edit::format_runs(
                    &self.doc,
                    kiln_doc::Pos::new(s.para, a),
                    kiln_doc::Pos::new(s.para, b),
                    |r| r.style_id = Some(style.clone()),
                )
                .map_err(doc_err)?
                .with_selection(Some(sel), Some(sel));
                return self.commit(tx);
            }
            let mut props = self.caret_props()?;
            props.style_id = Some(style);
            self.editor.pending = Some((self.head, props));
            return Ok(());
        }
        let sel = Range::new(self.anchor, self.head);
        let tx = edit::format_runs(&self.doc, s, e, |r| r.style_id = Some(style.clone()))
            .map_err(doc_err)?
            .with_selection(Some(sel), Some(sel));
        self.commit(tx)
    }

    /// The direct formatting of the selection's first paragraph and character.
    fn selection_formatting(&self) -> Result<(ParagraphProps, RunProps)> {
        let (s, _) = self.span();
        let p = self.para(s.para)?;
        let mut ppr = p.props.clone();
        ppr.style_id = None;
        ppr.numbering = None;
        let mut rpr = p
            .content
            .props_at(s.offset)
            .or_else(|| s.offset.checked_sub(1).and_then(|o| p.content.props_at(o)))
            .cloned()
            .unwrap_or_else(|| p.mark_props.clone());
        rpr.style_id = None;
        Ok((ppr, rpr))
    }

    /// Update Style to Match Selection: the selection's direct formatting moves into the style
    /// (and off the selected paragraphs).
    pub(super) fn update_style_from_selection(&mut self, id: Option<&str>) -> Result<()> {
        let (s, e) = self.span();
        let id = match id {
            Some(id) => id.to_owned(),
            None => self.doc.resolve_paragraph(self.para(s.para)?).style_id,
        };
        let Some(style) = self.doc.styles.get(&id).cloned() else {
            return Err(KilnError::InvalidInput(format!("no style '{id}'")));
        };
        let (ppr, rpr) = self.selection_formatting()?;
        let mut updated = style.clone();
        if matches!(style.kind, StyleType::Paragraph) {
            updated.ppr = Some(merge_paragraph_props(
                &style.ppr.clone().unwrap_or_default(),
                Some(&ppr),
            ));
        }
        updated.rpr = Some(merge_run_props(
            &style.rpr.clone().unwrap_or_default(),
            Some(&rpr),
        ));
        let mut batch = self.batch(EditKind::Format);
        let index = self.doc.styles.index_of(&id).unwrap_or(0);
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::PutStyle {
                style: Box::new(updated),
                index,
            }]),
        )?;
        if matches!(style.kind, StyleType::Paragraph) {
            let mut ops = Vec::new();
            for (pid, a, b) in edit::segments(&self.doc, s, e).map_err(doc_err)? {
                let p = self.para(pid)?;
                if self.doc.resolve_paragraph(p).style_id != id {
                    continue;
                }
                let keep = ParagraphProps {
                    style_id: p.props.style_id.clone(),
                    numbering: p.props.numbering,
                    ..ParagraphProps::default()
                };
                ops.push(Op::SetParagraphProps {
                    para: pid,
                    props: Box::new(keep),
                });
                let len = p.content.len();
                let (a, b) = if a == b { (0, len) } else { (a, b) };
                if a < b {
                    let runs = clear_direct_runs(p, a, b);
                    ops.push(Op::SetRuns {
                        para: pid,
                        offset: a,
                        runs,
                    });
                }
            }
            self.batch_apply(&mut batch, Transaction::new(ops))?;
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// New Style from Formatting: a paragraph style based on the current one with the
    /// selection's direct formatting, applied to the selection.
    pub(super) fn new_style_from_selection(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 253 {
            return Err(KilnError::InvalidInput(
                "a style name has 1–253 characters".into(),
            ));
        }
        if self.doc.styles.get_by_name(name).is_some() {
            return Err(KilnError::InvalidInput(format!(
                "a style named '{name}' exists already"
            )));
        }
        let (s, _) = self.span();
        let base = self.doc.resolve_paragraph(self.para(s.para)?).style_id;
        let (ppr, rpr) = self.selection_formatting()?;
        let id = self.doc.styles.unique_id(name);
        let style = Style {
            based_on: Some(base),
            next: Some(id.clone()),
            custom: true,
            quick_style: true,
            ui_priority: Some(1),
            ppr: Some(ppr),
            rpr: Some(rpr),
            ..Style::new(&id, StyleType::Paragraph, name)
        };
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(vec![Op::PutStyle {
                style: Box::new(style),
                index: self.doc.styles.len(),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )?;
        self.apply_style(&id)
    }

    /// The Modify Style dialog.
    pub(super) fn modify_style(&mut self, id: &str, spec: &StyleSpec) -> Result<()> {
        if !self.ensure_style(id)? {
            return Err(KilnError::InvalidInput(format!("no style '{id}'")));
        }
        let Some(mut style) = self.doc.styles.get(id).cloned() else {
            return Err(KilnError::InvalidInput(format!("no style '{id}'")));
        };
        if let Some(name) = &spec.name {
            let name = name.trim();
            if name.is_empty() {
                return Err(KilnError::InvalidInput("a style needs a name".into()));
            }
            if self
                .doc
                .styles
                .get_by_name(name)
                .is_some_and(|other| other.id != id)
            {
                return Err(KilnError::InvalidInput(format!(
                    "a style named '{name}' exists already"
                )));
            }
            style.name = name.to_owned();
        }
        if let Some(base) = &spec.based_on {
            if base.is_empty() {
                style.based_on = None;
            } else {
                if !self.doc.styles.contains(base) {
                    return Err(KilnError::InvalidInput(format!("no style '{base}'")));
                }
                if self.doc.styles.inherits_from(base, id) {
                    return Err(KilnError::InvalidInput(format!(
                        "'{id}' cannot be based on '{base}', which is based on it"
                    )));
                }
                style.based_on = Some(base.clone());
            }
        }
        if let Some(next) = &spec.next {
            if !next.is_empty() && !self.doc.styles.contains(next) {
                return Err(KilnError::InvalidInput(format!("no style '{next}'")));
            }
            style.next = (!next.is_empty()).then(|| next.clone());
        }
        if let Some(q) = spec.quick {
            style.quick_style = q;
        }
        // The font and paragraph fields go through the same code as the dialogs: format a
        // scratch paragraph and keep what it got.
        let rpr = font_patch(&spec.font)?;
        if rpr != RunProps::default() {
            style.rpr = Some(merge_run_props(&style.rpr.unwrap_or_default(), Some(&rpr)));
        }
        if matches!(style.kind, StyleType::Paragraph) {
            let ppr = paragraph_patch(&spec.paragraph)?;
            if ppr != ParagraphProps::default() {
                style.ppr = Some(merge_paragraph_props(
                    &style.ppr.unwrap_or_default(),
                    Some(&ppr),
                ));
            }
        }
        let index = self.doc.styles.index_of(id).unwrap_or(0);
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(vec![Op::PutStyle {
                style: Box::new(style),
                index,
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )
    }
}

/// The runs of `a..b` with only their character style kept.
fn clear_direct_runs(p: &kiln_doc::Paragraph, a: usize, b: usize) -> Vec<kiln_doc::RunSpan> {
    let mut out = Vec::new();
    let mut pos = 0;
    for r in p.content.runs() {
        let (s, e) = (pos.max(a), (pos + r.len).min(b));
        if s < e {
            out.push(kiln_doc::RunSpan::new(
                e - s,
                RunProps {
                    style_id: r.props.style_id.clone(),
                    ..RunProps::default()
                },
            ));
        }
        pos += r.len;
    }
    out
}

/// The run properties a Font dialog sets.
fn font_patch(f: &kiln_protocol::commands::FontFormat) -> Result<RunProps> {
    let mut p = RunProps::default();
    if let Some(name) = f.family.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        p.fonts = Some(kiln_doc::RunFonts::latin(name));
    }
    if let Some(size) = f.size {
        if !size.is_finite() || !(1.0..=1638.0).contains(&size) {
            return Err(KilnError::InvalidInput(format!(
                "font size {size} pt is outside 1–1638 pt"
            )));
        }
        let hp = (size * 2.0).round() as u32;
        p.size = Some(hp);
        p.size_cs = Some(hp);
    }
    p.bold = f.bold;
    p.italic = f.italic;
    if let Some(u) = f.underline {
        p.underline = Some(kiln_doc::Underline::new(super::format::underline_style(u)));
    }
    if let Some(c) = f.color.as_deref() {
        p.color = Some(if c.eq_ignore_ascii_case("auto") {
            kiln_doc::DocColor::auto()
        } else {
            super::format::doc_color(c)?
        });
    }
    p.strike = f.strike;
    p.caps = f.all_caps;
    p.small_caps = f.small_caps;
    Ok(p)
}

/// The paragraph properties a Paragraph dialog sets.
fn paragraph_patch(f: &kiln_protocol::commands::ParagraphFormat) -> Result<ParagraphProps> {
    use super::paragraph::twips;
    let mut p = ParagraphProps {
        alignment: f.alignment.map(super::paragraph::alignment),
        keep_next: f.keep_next,
        keep_lines: f.keep_lines,
        page_break_before: f.page_break_before,
        widow_control: f.widow_control,
        contextual_spacing: f.contextual_spacing,
        ..ParagraphProps::default()
    };
    if f.indent_left.is_some() || f.indent_right.is_some() || f.first_line.is_some() {
        let first = f.first_line.map(twips).transpose()?;
        p.indent = Some(kiln_doc::Indent {
            left: f.indent_left.map(twips).transpose()?,
            right: f.indent_right.map(twips).transpose()?,
            first_line: first.filter(|v| *v >= 0),
            hanging: first.filter(|v| *v < 0).map(|v| -v),
        });
    }
    if f.space_before.is_some() || f.space_after.is_some() || f.line_multiple.is_some() {
        p.spacing = Some(kiln_doc::Spacing {
            before: f.space_before.map(twips).transpose()?,
            after: f.space_after.map(twips).transpose()?,
            line: f.line_multiple.map(|m| (m * 240.0).round() as i32),
            line_rule: f.line_multiple.map(|_| kiln_doc::st::LineRule::Auto),
            ..kiln_doc::Spacing::default()
        });
    }
    Ok(p)
}
