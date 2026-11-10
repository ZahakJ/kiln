// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drag and drop of selected text (move, or copy with Ctrl) and Review ▸ Language.

use kiln_common::{KilnError, Result};
use kiln_doc::Pos;
use kiln_doc::props::Lang;
use kiln_protocol::TextPos;
use kiln_protocol::commands::PasteMode;
use kiln_protocol::commands::write_office::DropTarget;

use super::super::WriteDoc;

impl WriteDoc {
    /// Where text dragged to a document point would drop.
    pub(super) fn drop_point(&self, x: f32, y: f32) -> Option<DropTarget> {
        let v = self.view.as_ref()?;
        let hit = v.hit(x, y)?;
        let caret = v.caret(hit.pos)?;
        let (s, e) = self.span();
        let order = self.story_paragraphs(s.para);
        let rank = |p: Pos| {
            order
                .iter()
                .position(|x| *x == p.para)
                .map(|i| (i, p.offset))
        };
        let inside = match (rank(s), rank(e), rank(hit.pos)) {
            (Some(a), Some(b), Some(t)) => s != e && t > a && t < b,
            _ => false,
        };
        Some(DropTarget {
            pos: Self::wire(hit.pos),
            caret: kiln_protocol::geom::Rect::new(
                caret.rect.x,
                caret.rect.y,
                caret.rect.w,
                caret.rect.h,
            ),
            inside,
        })
    }

    /// Drops the selection at `to`: the text moves there (or is copied with `copy`) with its
    /// formatting, as one undo step, and ends up selected.
    pub(super) fn move_selection(&mut self, to: TextPos, copy: bool) -> Result<()> {
        let (s, e) = self.span();
        if s == e {
            return Ok(());
        }
        let to = Pos::new(to.block, to.offset as usize);
        if self.doc.paragraph(to.para).is_none() || to.offset > self.para_len(to.para) {
            return Err(KilnError::InvalidInput(
                "the drop point is not in the text".into(),
            ));
        }
        // A drop inside the selection does nothing (Word).
        let order = self.story_paragraphs(s.para);
        let rank = |p: Pos| {
            order
                .iter()
                .position(|x| *x == p.para)
                .map(|i| (i, p.offset))
        };
        let (Some(rs), Some(re)) = (rank(s), rank(e)) else {
            return Ok(());
        };
        let Some(rt) = rank(to) else {
            return Err(KilnError::InvalidInput(
                "text is dropped in the same story it comes from".into(),
            ));
        };
        if rt >= rs && rt <= re {
            return Ok(());
        }
        let Some(data) = self.clipboard_data()? else {
            return Ok(());
        };
        let before = self.states.len();
        let mut at = to;
        if !copy {
            // Deleting first moves a drop point after the selection.
            if to.para == e.para && to.offset >= e.offset {
                let tail = to.offset - e.offset;
                at = if s.para == e.para {
                    Pos::new(s.para, to.offset - (e.offset - s.offset))
                } else {
                    Pos::new(s.para, s.offset + tail)
                };
            }
            self.delete_selection()?;
        }
        self.set_selection(at, at);
        self.paste(Some(&data), PasteMode::KeepSource)?;
        let end = self.head;
        self.group_undo_since(before);
        self.set_selection(at, end);
        Ok(())
    }

    /// Set Proofing Language: the language of the selected text (spelling follows it; Kiln has
    /// an English (US) dictionary and leaves other languages unchecked), and whether to check it.
    pub(super) fn set_language(&mut self, lang: &str, no_proof: bool) -> Result<()> {
        let tag = lang.trim();
        let valid = !tag.is_empty()
            && tag.len() <= 35
            && tag.split('-').all(|p| {
                !p.is_empty() && p.len() <= 8 && p.chars().all(|c| c.is_ascii_alphanumeric())
            });
        if !valid {
            return Err(KilnError::InvalidInput(format!(
                "{lang} is not a language tag"
            )));
        }
        let tag = tag.to_owned();
        self.format_chars(move |p, _| {
            let mut l = p.lang.clone().unwrap_or_default();
            l.val = Some(tag.clone());
            p.lang = Some(Lang { ..l });
            p.no_proof = no_proof.then_some(true);
        })
    }
}
