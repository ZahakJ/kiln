// SPDX-License-Identifier: AGPL-3.0-or-later
//! Character formatting with Word's rules:
//!
//! - a selection is formatted as a whole; a toggle (Ctrl+B) turns the property off only when
//!   every selected character already shows it;
//! - with a collapsed caret inside a word, the whole word is formatted (the caret stays);
//! - with a collapsed caret elsewhere, the formatting is *pending*: the next typed text gets it
//!   (and moving the caret forgets it);
//! - Enter ends an empty list item's list and starts the paragraph after a heading in the
//!   heading's "next" style.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::resolve::{RunInput, RunTarget, resolve_run};
use kiln_doc::st::{HighlightColor, UnderlineStyle, VertAlign};
use kiln_doc::{
    DocColor, EditKind, Fragment, Op, Paragraph, Pos, Range, ResolvedRun, RunFonts, RunProps,
    RunSpan, ToggleProp, Transaction, Underline, edit,
};
use kiln_protocol::commands::{FontFormat, TextCase, UnderlineKind};

use super::doc_err;
use crate::apps::write::WriteDoc;
use crate::apps::write::editing::word_at;

/// Word's font size list (Grow Font / Shrink Font step through it).
const SIZES: [f32; 17] = [
    8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0,
    72.0,
];

/// Word's sizes are half points between 1 and 1638 pt.
fn half_points(pt: f32) -> Result<u32> {
    if !pt.is_finite() || !(1.0..=1638.0).contains(&pt) {
        return Err(KilnError::InvalidInput(format!(
            "font size {pt} pt is outside 1–1638 pt"
        )));
    }
    // In range, so the cast is exact enough (half points up to 3276).
    Ok((pt * 2.0).round().clamp(2.0, 3276.0) as u32)
}

/// What a character command formats.
enum Target {
    /// A range (the selection, or the word under the caret).
    Range(Pos, Pos),
    /// The caret's pending formatting.
    Pending(NodeId),
}

/// Parses `RRGGBB` (with or without `#`).
pub(super) fn parse_rgb(hex: &str) -> Result<kiln_common::Color> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return Err(KilnError::InvalidInput(format!(
            "colour '{hex}' is not RRGGBB"
        )));
    }
    kiln_common::Color::parse_hex(h)
        .map_err(|_| KilnError::InvalidInput(format!("colour '{hex}' is not RRGGBB")))
}

/// The DOCX colour of `RRGGBB`.
pub(super) fn doc_color(hex: &str) -> Result<DocColor> {
    let c = parse_rgb(hex)?;
    Ok(DocColor::rgb(
        (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
    ))
}

/// `RRGGBB` of a colour.
pub(super) fn hex(c: kiln_common::Color) -> String {
    format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

/// The Word highlight colour nearest to an RGB colour (the 15 colours of the Highlight menu).
pub(super) fn nearest_highlight(c: kiln_common::Color) -> HighlightColor {
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

/// The model's underline style for the protocol's.
pub(super) fn underline_style(kind: UnderlineKind) -> UnderlineStyle {
    match kind {
        UnderlineKind::Single => UnderlineStyle::Single,
        UnderlineKind::Words => UnderlineStyle::Words,
        UnderlineKind::Double => UnderlineStyle::Double,
        UnderlineKind::Thick => UnderlineStyle::Thick,
        UnderlineKind::Dotted => UnderlineStyle::Dotted,
        UnderlineKind::Dash => UnderlineStyle::Dash,
        UnderlineKind::DotDash => UnderlineStyle::DotDash,
        UnderlineKind::Wave => UnderlineStyle::Wave,
    }
}

/// The protocol's underline kind for a model style (the closest of the menu's).
pub(super) fn underline_kind(style: &UnderlineStyle) -> Option<UnderlineKind> {
    Some(match style {
        UnderlineStyle::None => return None,
        UnderlineStyle::Words => UnderlineKind::Words,
        UnderlineStyle::Double | UnderlineStyle::WavyDouble => UnderlineKind::Double,
        UnderlineStyle::Thick => UnderlineKind::Thick,
        UnderlineStyle::Dotted | UnderlineStyle::DottedHeavy => UnderlineKind::Dotted,
        UnderlineStyle::Dash
        | UnderlineStyle::DashedHeavy
        | UnderlineStyle::DashLong
        | UnderlineStyle::DashLongHeavy => UnderlineKind::Dash,
        UnderlineStyle::DotDash
        | UnderlineStyle::DashDotHeavy
        | UnderlineStyle::DotDotDash
        | UnderlineStyle::DashDotDotHeavy => UnderlineKind::DotDash,
        UnderlineStyle::Wave | UnderlineStyle::WavyHeavy => UnderlineKind::Wave,
        _ => UnderlineKind::Single,
    })
}

/// Applies a case change to a text, keeping its length in `char`s (characters whose case
/// mapping would change the length, like `ß`, are kept).
pub(super) fn recase(text: &str, case: TextCase) -> String {
    let one = |mapped: String, c: char| {
        let mut it = mapped.chars();
        match (it.next(), it.next()) {
            (Some(m), None) => m,
            _ => c,
        }
    };
    let upper = |c: char| one(c.to_uppercase().collect(), c);
    let lower = |c: char| one(c.to_lowercase().collect(), c);
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    let mut sentence_start = true;
    for c in text.chars() {
        let mapped = match case {
            TextCase::Upper => upper(c),
            TextCase::Lower => lower(c),
            TextCase::Toggle => {
                if c.is_uppercase() {
                    lower(c)
                } else {
                    upper(c)
                }
            }
            TextCase::Title => {
                if word_start {
                    upper(c)
                } else {
                    lower(c)
                }
            }
            TextCase::Sentence => {
                if sentence_start && c.is_alphabetic() {
                    upper(c)
                } else {
                    lower(c)
                }
            }
        };
        out.push(mapped);
        if c.is_alphanumeric() {
            word_start = false;
            sentence_start = false;
        } else if c.is_whitespace() || c == '-' || c == '\u{FFFC}' {
            word_start = true;
        }
        if matches!(c, '.' | '!' | '?' | '\n') {
            sentence_start = true;
        }
    }
    out
}

impl WriteDoc {
    /// The paragraph with an id, as an error when it is gone.
    pub(super) fn para(&self, id: NodeId) -> Result<&Paragraph> {
        self.doc
            .paragraph(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no paragraph {id}")))
    }

    /// The effective run formatting of `direct` in a paragraph.
    pub(super) fn resolve_with(&self, para: &Paragraph, direct: &RunProps) -> ResolvedRun {
        let table = self.doc.table_context(para.id);
        resolve_run(
            &self.doc.context(),
            &RunInput {
                paragraph: &para.props,
                mark_props: &para.mark_props,
                direct,
                table: table.as_ref(),
                target: RunTarget::Text,
            },
        )
    }

    /// The direct formatting text typed at the caret gets (the pending formatting first).
    pub(super) fn caret_props(&self) -> Result<RunProps> {
        if let Some((at, props)) = &self.editor.pending
            && *at == self.head
        {
            return Ok(props.clone());
        }
        let p = self.para(self.head.para)?;
        Ok(edit::typing_props(p, self.head.offset))
    }

    /// Whether typed text takes the pending formatting.
    pub(super) fn pending_applies(&self) -> bool {
        self.anchor == self.head
            && self
                .editor
                .pending
                .as_ref()
                .is_some_and(|(at, _)| *at == self.head)
    }

    fn target(&self) -> Result<Target> {
        let (s, e) = self.span();
        if s != e {
            return Ok(Target::Range(s, e));
        }
        let text = self.para_text(s.para);
        let chars: Vec<char> = text.chars().collect();
        let word = |i: usize| chars.get(i).is_some_and(|c| c.is_alphanumeric());
        if s.offset > 0 && word(s.offset - 1) && word(s.offset) && !self.pending_applies() {
            let (a, b) = word_at(&text, s.offset);
            if a < s.offset && s.offset < b {
                return Ok(Target::Range(Pos::new(s.para, a), Pos::new(s.para, b)));
            }
        }
        Ok(Target::Pending(s.para))
    }

    /// Formats the target with `f`, which sees each run's direct and effective formatting.
    fn format_chars(&mut self, f: impl Fn(&mut RunProps, &ResolvedRun)) -> Result<()> {
        match self.target()? {
            Target::Range(s, e) => {
                let mut ops = Vec::new();
                for (id, start, end) in edit::segments(&self.doc, s, e).map_err(doc_err)? {
                    if start >= end {
                        continue;
                    }
                    let p = self.para(id)?;
                    let mut runs = Vec::new();
                    let mut pos = 0;
                    for r in p.content.runs() {
                        let (a, b) = (pos.max(start), (pos + r.len).min(end));
                        if a < b {
                            let resolved = self.resolve_with(p, &r.props);
                            let mut props = r.props.clone();
                            f(&mut props, &resolved);
                            runs.push(RunSpan::new(b - a, props));
                        }
                        pos += r.len;
                    }
                    ops.push(Op::SetRuns {
                        para: id,
                        offset: start,
                        runs,
                    });
                }
                let sel = Range::new(self.anchor, self.head);
                self.commit(
                    Transaction::new(ops)
                        .with_kind(EditKind::Format)
                        .with_selection(Some(sel), Some(sel)),
                )
            }
            Target::Pending(para) => {
                let mut props = self.caret_props()?;
                let resolved = self.resolve_with(self.para(para)?, &props);
                f(&mut props, &resolved);
                self.editor.pending = Some((self.head, props));
                Ok(())
            }
        }
    }

    /// Whether every character of the target shows a property (the toggle rule).
    fn all_show(&self, test: impl Fn(&ResolvedRun) -> bool) -> Result<bool> {
        match self.target()? {
            Target::Range(s, e) => {
                let mut any = false;
                for (id, start, end) in edit::segments(&self.doc, s, e).map_err(doc_err)? {
                    let p = self.para(id)?;
                    let mut pos = 0;
                    for r in p.content.runs() {
                        if pos < end && pos + r.len > start {
                            any = true;
                            if !test(&self.resolve_with(p, &r.props)) {
                                return Ok(false);
                            }
                        }
                        pos += r.len;
                    }
                }
                Ok(any)
            }
            Target::Pending(para) => {
                let props = self.caret_props()?;
                Ok(test(&self.resolve_with(self.para(para)?, &props)))
            }
        }
    }

    /// Bold, italic, caps, strikethrough… with Word's toggle rule.
    pub(super) fn toggle_prop(&mut self, prop: ToggleProp) -> Result<()> {
        match self.target()? {
            Target::Range(s, e) => {
                let sel = Range::new(self.anchor, self.head);
                let tx = edit::toggle(&self.doc, s, e, prop)
                    .map_err(doc_err)?
                    .with_selection(Some(sel), Some(sel));
                self.commit(tx)
            }
            Target::Pending(para) => {
                let mut props = self.caret_props()?;
                let p = self.para(para)?;
                let on = toggle_value(&self.resolve_with(p, &props), prop);
                props.set_toggle(prop, None);
                if toggle_value(&self.resolve_with(p, &props), prop) == on {
                    props.set_toggle(prop, Some(!on));
                }
                self.editor.pending = Some((self.head, props));
                Ok(())
            }
        }
    }

    /// Underline (Ctrl+U) and double underline (Ctrl+Shift+D) as toggles.
    pub(super) fn toggle_underline(&mut self, style: UnderlineStyle) -> Result<()> {
        let on = self.all_show(|r| r.underline.as_ref().is_some_and(|u| u.style == style))?;
        self.format_chars(move |p, r| {
            if on {
                p.underline = None;
                if r.underline.is_some() && !p_has_direct_underline(p) {
                    // A style underlines this text: say "none" explicitly.
                    p.underline = Some(Underline::new(UnderlineStyle::None));
                }
            } else {
                let color = p.underline.as_ref().and_then(|u| u.color.clone());
                p.underline = Some(Underline { style, color });
            }
        })
    }

    /// Underline ▾ with a style, or no underline.
    pub(super) fn underline_style(&mut self, kind: Option<UnderlineKind>) -> Result<()> {
        self.format_chars(move |p, _| {
            p.underline = Some(match kind {
                Some(k) => Underline {
                    style: underline_style(k),
                    color: p.underline.as_ref().and_then(|u| u.color.clone()),
                },
                None => Underline::new(UnderlineStyle::None),
            });
        })
    }

    /// Subscript and superscript as toggles.
    pub(super) fn toggle_vert_align(&mut self, align: VertAlign) -> Result<()> {
        let on = self.all_show(|r| r.vert_align == align)?;
        self.format_chars(move |p, _| {
            p.vert_align = if on {
                Some(VertAlign::Baseline)
            } else {
                Some(align)
            };
        })
    }

    /// The font box.
    pub(super) fn font_family(&mut self, family: &str) -> Result<()> {
        let name = family.trim();
        if name.is_empty() || name.chars().count() > 31 {
            return Err(KilnError::InvalidInput(format!(
                "'{family}' is not a font name (1–31 characters)"
            )));
        }
        let name = name.to_owned();
        self.format_chars(move |p, _| {
            let mut fonts = RunFonts::latin(&name);
            fonts.cs = Some(name.clone());
            if let Some(old) = &p.fonts {
                fonts.east_asia.clone_from(&old.east_asia);
                fonts.east_asia_theme = old.east_asia_theme;
            }
            p.fonts = Some(fonts);
        })
    }

    /// The size box.
    pub(super) fn font_size(&mut self, pt: f32) -> Result<()> {
        let hp = half_points(pt)?;
        self.format_chars(move |p, _| {
            p.size = Some(hp);
            p.size_cs = Some(hp);
        })
    }

    /// Grow Font / Shrink Font, run by run from each run's own size.
    pub(super) fn grow_font(&mut self, grow: bool, one_point: bool) -> Result<()> {
        self.format_chars(move |p, r| {
            let current = r.size as f32 / 2.0;
            let next = if one_point {
                if grow { current + 1.0 } else { current - 1.0 }
            } else if grow {
                SIZES
                    .iter()
                    .copied()
                    .find(|s| *s > current + 0.01)
                    .unwrap_or((current / 10.0).floor() * 10.0 + 10.0)
            } else {
                SIZES
                    .iter()
                    .rev()
                    .copied()
                    .find(|s| *s < current - 0.01)
                    .unwrap_or(1.0)
            };
            let hp = (next.clamp(1.0, 1638.0) * 2.0).round() as u32;
            p.size = Some(hp);
            p.size_cs = Some(hp);
        })
    }

    /// Font colour (`None`: Automatic).
    pub(super) fn font_color(&mut self, color: Option<&str>) -> Result<()> {
        let c = match color {
            Some(h) => doc_color(h)?,
            None => DocColor::auto(),
        };
        self.format_chars(move |p, _| p.color = Some(c.clone()))
    }

    /// Text highlight (`None` removes it).
    pub(super) fn highlight(&mut self, color: Option<&str>) -> Result<()> {
        let h = match color {
            Some(hex) => Some(nearest_highlight(parse_rgb(hex)?)),
            None => None,
        };
        self.format_chars(move |p, r| {
            p.highlight = match &h {
                Some(h) => Some(*h),
                None if r.highlight.is_some() && p.highlight.is_none() => {
                    Some(HighlightColor::None)
                }
                None => None,
            };
        })
    }

    /// Change Case; `None` cycles lower → Capitalize Each Word → UPPER → lower (Shift+F3).
    pub(super) fn change_case(&mut self, case: Option<TextCase>) -> Result<()> {
        let (s, e) = match self.target()? {
            Target::Range(s, e) => (s, e),
            Target::Pending(_) => return Ok(()),
        };
        let segs = edit::segments(&self.doc, s, e).map_err(doc_err)?;
        let case = match case {
            Some(c) => c,
            None => {
                let text: String = segs
                    .iter()
                    .map(|(id, a, b)| {
                        self.para_text(*id)
                            .chars()
                            .skip(*a)
                            .take(b - a)
                            .collect::<String>()
                    })
                    .collect();
                let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
                if !letters.is_empty() && letters.iter().all(|c| c.is_uppercase()) {
                    TextCase::Lower
                } else if !letters.is_empty() && letters.iter().all(|c| c.is_lowercase()) {
                    TextCase::Title
                } else {
                    TextCase::Upper
                }
            }
        };
        let mut ops = Vec::new();
        for (id, a, b) in segs {
            if a >= b {
                continue;
            }
            let p = self.para(id)?;
            let mut frag = p.content.slice(a, b).map_err(doc_err)?;
            let changed = recase(&frag.text, case);
            if changed == frag.text {
                continue;
            }
            frag.text = changed;
            ops.push(Op::DeleteInline {
                para: id,
                offset: a,
                len: b - a,
            });
            ops.push(Op::InsertInline {
                para: id,
                offset: a,
                fragment: frag,
            });
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Clear Formatting: no direct character formatting and the paragraphs in Normal (Word keeps
    /// highlighting).
    pub(super) fn clear_all_formatting(&mut self) -> Result<()> {
        self.format_chars(|p, _| {
            let highlight = p.highlight;
            *p = RunProps {
                highlight,
                ..RunProps::default()
            };
        })?;
        let normal = self
            .doc
            .styles
            .default_style(kiln_doc::st::StyleType::Paragraph)
            .map_or_else(|| "Normal".to_owned(), |s| s.id.clone());
        let (s, e) = self.span();
        let sel = Range::new(self.anchor, self.head);
        let tx = edit::format_paragraphs(&self.doc, s, e, |pp| {
            *pp = kiln_doc::ParagraphProps {
                style_id: Some(normal.clone()),
                ..kiln_doc::ParagraphProps::default()
            };
        })
        .map_err(doc_err)?
        .with_selection(Some(sel), Some(sel));
        self.commit(tx)
    }

    /// Ctrl+Space: no direct character formatting (character styles stay).
    pub(super) fn clear_character_formatting(&mut self) -> Result<()> {
        self.format_chars(|p, _| {
            *p = RunProps {
                style_id: p.style_id.clone(),
                ..RunProps::default()
            };
        })
    }

    /// The Font dialog.
    pub(super) fn set_font(&mut self, f: &FontFormat) -> Result<()> {
        if let Some(size) = f.size {
            half_points(size)?;
        }
        let color = match f.color.as_deref() {
            Some(c) if c.eq_ignore_ascii_case("auto") => Some(DocColor::auto()),
            Some(c) => Some(doc_color(c)?),
            None => None,
        };
        let underline_color = f.underline_color.as_deref().map(doc_color).transpose()?;
        if let Some(scale) = f.scale
            && !(1..=600).contains(&scale)
        {
            return Err(KilnError::InvalidInput(format!(
                "scale {scale} % is outside 1–600 %"
            )));
        }
        let f = f.clone();
        self.format_chars(move |p, _| {
            if let Some(name) = &f.family
                && !name.trim().is_empty()
            {
                let mut fonts = RunFonts::latin(name.trim());
                fonts.cs = Some(name.trim().to_owned());
                p.fonts = Some(fonts);
            }
            if let Some(size) = f.size {
                let hp = (size * 2.0).round() as u32;
                p.size = Some(hp);
                p.size_cs = Some(hp);
            }
            if let Some(b) = f.bold {
                p.bold = Some(b);
                p.bold_cs = Some(b);
            }
            if let Some(i) = f.italic {
                p.italic = Some(i);
                p.italic_cs = Some(i);
            }
            if f.underline_none {
                p.underline = Some(Underline::new(UnderlineStyle::None));
            } else if let Some(u) = f.underline {
                p.underline = Some(Underline {
                    style: underline_style(u),
                    color: underline_color.clone(),
                });
            } else if let (Some(c), Some(u)) = (&underline_color, &mut p.underline) {
                u.color = Some(c.clone());
            }
            if let Some(c) = &color {
                p.color = Some(c.clone());
            }
            if let Some(v) = f.strike {
                p.strike = Some(v);
            }
            if let Some(v) = f.double_strike {
                p.dstrike = Some(v);
            }
            match (f.superscript, f.subscript) {
                (Some(true), _) => p.vert_align = Some(VertAlign::Superscript),
                (_, Some(true)) => p.vert_align = Some(VertAlign::Subscript),
                (Some(false), _) | (_, Some(false)) => p.vert_align = Some(VertAlign::Baseline),
                _ => {}
            }
            if let Some(v) = f.small_caps {
                p.small_caps = Some(v);
            }
            if let Some(v) = f.all_caps {
                p.caps = Some(v);
            }
            if let Some(v) = f.hidden {
                p.hidden = Some(v);
            }
            if let Some(s) = f.spacing {
                p.spacing = Some((s * 20.0).round() as i32);
            }
            if let Some(s) = f.scale {
                p.scale = Some(s);
            }
            if let Some(pos) = f.position {
                p.position = Some((pos * 2.0).round() as i32);
            }
            if let Some(k) = f.kerning {
                p.kern = if k <= 0.0 {
                    Some(0)
                } else {
                    Some((k * 2.0).round() as u32)
                };
            }
        })
    }

    /// Typing with the pending formatting.
    pub(super) fn insert_pending(&mut self, text: &str) -> Result<()> {
        let Some((at, props)) = self.editor.pending.clone() else {
            return Ok(());
        };
        if text.contains(['\n', '\r']) {
            self.insert_plain(text)?;
            return Ok(());
        }
        let p = self.para(at.para)?;
        let marks = edit::typing_marks(p, at.offset);
        let fragment = marks
            .into_iter()
            .fold(Fragment::text(text, props.clone()), |f, m| f.with_mark(m));
        let len = fragment.len();
        if len == 0 {
            return Ok(());
        }
        let end = Pos::new(at.para, at.offset + len);
        let tx = Transaction::new(vec![Op::InsertInline {
            para: at.para,
            offset: at.offset,
            fragment,
        }])
        .with_kind(EditKind::Typing)
        .with_selection(Some(Range::caret(at)), Some(Range::caret(end)));
        self.commit(tx)?;
        self.editor.pending = Some((end, props));
        Ok(())
    }

    /// Inserts text (lines become paragraphs) at the caret, replacing the selection.
    pub(super) fn insert_plain(&mut self, text: &str) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let tx = edit::insert_text(&mut self.doc, at, text).map_err(doc_err)?;
        self.commit(tx)
    }

    /// Enter.
    pub(super) fn enter(&mut self) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let p = self.para(at.para)?;
        let resolved = self.doc.resolve_paragraph(p);
        if p.content.is_empty() && resolved.numbering.is_some() {
            // Enter in an empty list item ends the list (Word).
            return self.remove_numbering(&[at.para]);
        }
        let tx = edit::split_paragraph(&mut self.doc, at).map_err(doc_err)?;
        let carried = self
            .editor
            .pending
            .as_ref()
            .filter(|(pos, _)| *pos == at)
            .map(|(_, props)| props.clone());
        self.commit(tx)?;
        if let Some(props) = carried {
            self.editor.pending = Some((self.head, props));
        }
        Ok(())
    }
}

fn p_has_direct_underline(p: &RunProps) -> bool {
    p.underline.is_some()
}

/// The effective value of a toggle property.
fn toggle_value(r: &ResolvedRun, prop: ToggleProp) -> bool {
    match prop {
        ToggleProp::Bold => r.bold,
        ToggleProp::BoldCs => r.bold_cs,
        ToggleProp::Italic => r.italic,
        ToggleProp::ItalicCs => r.italic_cs,
        ToggleProp::Caps => r.caps,
        ToggleProp::SmallCaps => r.small_caps,
        ToggleProp::Strike => r.strike,
        ToggleProp::Dstrike => r.dstrike,
        ToggleProp::Hidden => r.hidden,
        ToggleProp::Emboss => r.emboss,
        ToggleProp::Imprint => r.imprint,
        ToggleProp::Outline => r.outline,
        ToggleProp::Shadow => r.shadow,
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn case_changes_keep_the_length() {
        assert_eq!(
            recase("the kiln. it glows", TextCase::Sentence),
            "The kiln. It glows"
        );
        assert_eq!(
            recase("the kiln-fired pot", TextCase::Title),
            "The Kiln-Fired Pot"
        );
        assert_eq!(recase("Straße", TextCase::Upper), "STRAßE");
        assert_eq!(recase("KiLn", TextCase::Toggle), "kIlN");
        assert_eq!(recase("ÉTÉ", TextCase::Lower), "été");
    }

    #[test]
    fn colours_and_highlights() {
        assert!(parse_rgb("C8352B").is_ok() && parse_rgb("#c8352b").is_ok());
        assert!(parse_rgb("red").is_err());
        let c = parse_rgb("FFE066").unwrap_or(kiln_common::Color::rgb(0, 0, 0));
        assert_eq!(nearest_highlight(c), HighlightColor::Yellow);
        assert_eq!(half_points(10.5).ok(), Some(21));
        assert!(half_points(0.5).is_err() && half_points(f32::NAN).is_err());
    }
}
