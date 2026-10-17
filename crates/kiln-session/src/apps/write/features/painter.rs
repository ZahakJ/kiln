// SPDX-License-Identifier: AGPL-3.0-or-later
//! Format Painter and Copy/Paste Formatting.
//!
//! Format Painter picks up the formatting of the selection's first character (with a collapsed
//! caret or a whole-paragraph selection, the paragraph's formatting too) and arms; the next
//! selection the user makes is painted ([`WriteDoc::paste_format`], which the editor runs when
//! the mouse selection ends) and the painter disarms — unless it was double-clicked (`sticky`),
//! which keeps it on until Esc or another click on the button. Ctrl+Shift+C copies the
//! formatting without arming; Ctrl+Shift+V pastes it. With a collapsed caret the word at the
//! caret is painted, as in Word.

use kiln_common::{KilnError, Result};
use kiln_doc::{EditKind, Op, ParagraphProps, Pos, Range, RunProps, RunSpan, Transaction, edit};

use super::super::WriteDoc;
use super::super::editing::word_at;

/// Formatting picked up by Format Painter or Copy Formatting.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::apps::write) struct Captured {
    /// Character formatting (direct properties and character style).
    pub run: RunProps,
    /// Paragraph formatting, when the source was a whole paragraph or a caret.
    pub para: Option<ParagraphProps>,
}

/// An armed Format Painter.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::apps::write) struct Painter {
    pub captured: Captured,
    /// Double-clicked: stays on after painting.
    pub sticky: bool,
}

impl WriteDoc {
    /// The selection's formatting.
    fn capture(&self) -> Result<Captured> {
        let (s, e) = self.span();
        let p = self.paragraph(s.para)?;
        let len = p.content.len();
        let run = if s == e || s.offset >= len {
            edit::typing_props(p, s.offset)
        } else {
            p.content.props_at(s.offset).cloned().unwrap_or_default()
        };
        let whole = s.offset == 0 && (e.para != s.para || e.offset >= len);
        let para = (s == e || whole).then(|| {
            let mut pp = p.props.clone();
            pp.drop_cap = None;
            pp
        });
        Ok(Captured { run, para })
    }

    /// Format Painter (again while armed: off).
    pub(super) fn format_painter(&mut self, sticky: bool) -> Result<()> {
        if self.features.painter.is_some() {
            self.features.painter = None;
            return Ok(());
        }
        let captured = self.capture()?;
        self.features.painter = Some(Painter { captured, sticky });
        Ok(())
    }

    /// Copy Formatting (Ctrl+Shift+C).
    pub(super) fn copy_format(&mut self) -> Result<()> {
        self.features.copied = Some(self.capture()?);
        Ok(())
    }

    /// Paints the selection with the armed painter's (or the copied) formatting.
    pub(super) fn paste_format(&mut self) -> Result<()> {
        let captured = match (&self.features.painter, &self.features.copied) {
            (Some(p), _) => p.captured.clone(),
            (None, Some(c)) => c.clone(),
            (None, None) => {
                return Err(KilnError::InvalidInput(
                    "copy formatting first (Format Painter or Ctrl+Shift+C)".into(),
                ));
            }
        };
        let (mut s, mut e) = self.span();
        if s == e {
            // The word at the caret.
            let text = self.para_text(s.para);
            let (a, b) = word_at(&text, s.offset.min(text.chars().count().saturating_sub(1)));
            if b <= a {
                self.disarm_after_paint();
                return Ok(());
            }
            s = Pos::new(s.para, a);
            e = Pos::new(s.para, b);
        }
        let segments =
            edit::segments(&self.doc, s, e).map_err(|x| KilnError::InvalidInput(x.to_string()))?;
        let mut ops = Vec::new();
        for (para, a, b) in &segments {
            if a < b {
                ops.push(Op::SetRuns {
                    para: *para,
                    offset: *a,
                    runs: vec![RunSpan::new(b - a, captured.run.clone())],
                });
            }
        }
        if let Some(pp) = &captured.para {
            let whole = segments.len() > 1
                || segments.iter().any(|(para, a, b)| {
                    *a == 0 && *b >= self.doc.paragraph(*para).map_or(0, |p| p.content.len())
                });
            if whole {
                for (para, _, _) in &segments {
                    let mut props = pp.clone();
                    if let Some(p) = self.doc.paragraph(*para) {
                        // Keep what painting a format never changes: section-like and
                        // drop-cap properties of the target.
                        props.drop_cap.clone_from(&p.props.drop_cap);
                        if p.props != props {
                            ops.push(Op::SetParagraphProps {
                                para: *para,
                                props: Box::new(props),
                            });
                        }
                    }
                }
            }
        }
        let sel = Range::new(self.anchor, self.head);
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        );
        self.disarm_after_paint();
        r
    }

    fn disarm_after_paint(&mut self) {
        if self.features.painter.as_ref().is_some_and(|p| !p.sticky) {
            self.features.painter = None;
        }
    }
}
