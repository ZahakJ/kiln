// SPDX-License-Identifier: AGPL-3.0-or-later
//! Footnotes and endnotes: Insert Footnote (Ctrl+Alt+F) and Insert Endnote (Ctrl+Alt+D) at the
//! insertion point, with the caret in the new note; Next and Previous; Show Notes (between a
//! reference and its note); the Footnote and Endnote dialog's numbering; Convert all footnotes
//! to endnotes and back. Notes are numbered by the layout in document order, so inserting one
//! renumbers the ones after it, as in Word.

use std::cmp::Ordering;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::settings::NoteSettings;
use kiln_doc::st::{NoteKind, NoteType, NumberFormat};
use kiln_doc::{
    Block, EditKind, Fragment, InlineObject, Inlines, Note, NoteRef, Op, Paragraph, Pos, Range,
    RunProps, Story, Transaction,
};
use kiln_protocol::commands::write_features::NoteOptionsSpec;

use super::super::WriteDoc;
use super::missing_styles;

fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

fn styles(kind: NoteKind) -> (&'static str, &'static str) {
    match kind {
        NoteKind::Endnote => ("EndnoteText", "EndnoteReference"),
        _ => ("FootnoteText", "FootnoteReference"),
    }
}

impl WriteDoc {
    /// Insert Footnote or Insert Endnote.
    pub(super) fn insert_note(&mut self, kind: NoteKind) -> Result<()> {
        let (_, at) = self.span();
        if !matches!(self.doc.story_of(at.para), Some(Story::Body(_))) {
            return Err(KilnError::InvalidInput(
                "footnotes and endnotes go in the body text".into(),
            ));
        }
        let (text_style, ref_style) = styles(kind);
        let mut batch = self.batch(EditKind::Structure);
        let r = (|| {
            self.batch_apply(
                &mut batch,
                Transaction::new(missing_styles(&self.doc, &[text_style, ref_style])),
            )?;
            let note_id = self.doc.alloc_id();
            let ref_props = RunProps {
                style_id: self
                    .doc
                    .styles
                    .contains(ref_style)
                    .then(|| ref_style.to_owned()),
                vert_align: (!self.doc.styles.contains(ref_style))
                    .then_some(kiln_doc::st::VertAlign::Superscript),
                ..RunProps::default()
            };
            let mut body = Paragraph::new(self.doc.alloc_id());
            body.props.style_id = self
                .doc
                .styles
                .contains(text_style)
                .then(|| text_style.to_owned());
            // The note's own reference mark goes in once the note exists.
            body.content = Inlines::from_fragment(Fragment::text(" ", RunProps::default()))
                .map_err(doc_err)?;
            let body_id = body.id;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertNote {
                    index: self.doc.notes.len(),
                    note: Box::new(Note {
                        id: note_id,
                        kind,
                        note_type: NoteType::Normal,
                        blocks: vec![Block::Paragraph(body)],
                    }),
                }]),
            )?;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertInline {
                    para: body_id,
                    offset: 0,
                    fragment: Fragment::object(
                        InlineObject::NoteRef(NoteRef {
                            note: note_id,
                            kind,
                            custom_mark: None,
                        }),
                        ref_props.clone(),
                    ),
                }]),
            )?;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: Fragment::object(
                        InlineObject::NoteRef(NoteRef {
                            note: note_id,
                            kind,
                            custom_mark: None,
                        }),
                        ref_props,
                    ),
                }]),
            )?;
            Ok(Pos::new(body_id, 2))
        })();
        match r {
            Ok(caret) => {
                self.batch_commit(batch, Range::caret(caret));
                Ok(())
            }
            Err(e) => {
                let here = self.doc.clamp(at).unwrap_or(at);
                self.batch_commit(batch, Range::caret(here));
                Err(e)
            }
        }
    }

    /// Note references in the body, in document order: (position, note, kind).
    fn note_refs(&self) -> Vec<(Pos, NodeId, NoteKind)> {
        let mut out = Vec::new();
        for p in self.doc.body_paragraphs() {
            for (off, o) in p.content.objects_with_offsets() {
                if let InlineObject::NoteRef(n) = o
                    && matches!(self.doc.story_of(p.id), Some(Story::Body(_)))
                {
                    out.push((Pos::new(p.id, off), n.note, n.kind));
                }
            }
        }
        out
    }

    /// Next (or previous) footnote or endnote reference, from the caret (from a note: from its
    /// reference).
    pub(super) fn next_note(&mut self, backward: bool) -> Result<()> {
        let refs = self.note_refs();
        if refs.is_empty() {
            return Err(KilnError::InvalidInput(
                "the document has no footnotes or endnotes".into(),
            ));
        }
        let (s, e) = self.span();
        let from = match self.doc.story_of(s.para) {
            Some(Story::Note(i)) => {
                let id = self.doc.notes.get(i).map(|n| n.id);
                refs.iter()
                    .find(|r| Some(r.1) == id)
                    .map_or(s, |r| Pos::new(r.0.para, r.0.offset + 1))
            }
            _ => {
                if backward {
                    s
                } else {
                    e
                }
            }
        };
        let cmp = |p: Pos| self.doc.compare(p, from).unwrap_or(Ordering::Equal);
        let pick = if backward {
            refs.iter()
                .rev()
                .find(|r| cmp(Pos::new(r.0.para, r.0.offset + 1)) == Ordering::Less)
                .or_else(|| refs.last())
        } else {
            refs.iter()
                .find(|r| cmp(r.0) != Ordering::Less)
                .or_else(|| refs.first())
        };
        if let Some(r) = pick {
            let after = Pos::new(r.0.para, r.0.offset + 1);
            self.set_selection(after, after);
        }
        Ok(())
    }

    /// Show Notes: from a reference (or the caret just after one) to its note, from a note back
    /// to its reference; elsewhere, to the first note.
    pub(super) fn show_notes(&mut self) -> Result<()> {
        let (s, _) = self.span();
        if let Some(Story::Note(i)) = self.doc.story_of(s.para) {
            let id = self.doc.notes.get(i).map(|n| n.id);
            if let Some(r) = self.note_refs().into_iter().find(|r| Some(r.1) == id) {
                let after = Pos::new(r.0.para, r.0.offset + 1);
                self.set_selection(after, after);
                return Ok(());
            }
            return Ok(());
        }
        let refs = self.note_refs();
        let near = refs
            .iter()
            .find(|r| r.0.para == s.para && (r.0.offset == s.offset || r.0.offset + 1 == s.offset))
            .or_else(|| {
                refs.iter().find(|r| {
                    self.doc
                        .compare(r.0, s)
                        .is_some_and(|o| o != Ordering::Less)
                })
            })
            .or_else(|| refs.first());
        let Some(&(_, note, _)) = near else {
            return Err(KilnError::InvalidInput(
                "the document has no footnotes or endnotes".into(),
            ));
        };
        let target = self
            .doc
            .notes
            .iter()
            .find(|n| n.id == note)
            .and_then(|n| {
                let mut ps = Vec::new();
                super::paragraphs_of(&n.blocks, &mut ps);
                ps.first().map(|p| Pos::new(p.id, p.content.len()))
            })
            .ok_or_else(|| KilnError::InvalidInput("the note has no text".into()))?;
        self.set_selection(target, target);
        Ok(())
    }

    /// Footnote and Endnote dialog: number format, start and restart.
    pub(super) fn note_options(&mut self, spec: &NoteOptionsSpec) -> Result<()> {
        // The formats of Word's Footnote and Endnote dialog.
        const FORMATS: [&str; 7] = [
            "decimal",
            "upperRoman",
            "lowerRoman",
            "upperLetter",
            "lowerLetter",
            "chicago",
            "decimalEnclosedCircle",
        ];
        let wanted = spec.format.trim();
        let format = NumberFormat::parse(wanted);
        if !FORMATS.contains(&wanted) {
            return Err(KilnError::InvalidInput(format!(
                "'{}' is not a number format",
                spec.format
            )));
        }
        if spec.start == 0 || spec.start > 32_767 {
            return Err(KilnError::InvalidInput("notes start at 1 to 32,767".into()));
        }
        let restart = match spec.restart.as_str() {
            "continuous" | "eachSect" | "eachPage" => spec.restart.clone(),
            other => {
                return Err(KilnError::InvalidInput(format!(
                    "'{other}' is not a restart rule (continuous, eachSect, eachPage)"
                )));
            }
        };
        let mut settings = self.doc.settings.clone();
        let target: &mut NoteSettings = if spec.endnotes {
            &mut settings.endnotes
        } else {
            &mut settings.footnotes
        };
        target.format = Some(format);
        target.start = Some(i64::from(spec.start));
        target.restart = Some(restart);
        let sel = self.selection_range();
        self.commit(
            Transaction::new(vec![Op::SetSettings {
                settings: Box::new(settings),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Convert all footnotes to endnotes (or endnotes to footnotes). Each note gets a new
    /// record of the other kind (the model's notes keep their kind), its references in the text
    /// follow it, and its styles change with it.
    pub(super) fn convert_notes(&mut self, to_endnotes: bool) -> Result<()> {
        let (from, to) = if to_endnotes {
            (NoteKind::Footnote, NoteKind::Endnote)
        } else {
            (NoteKind::Endnote, NoteKind::Footnote)
        };
        let (from_text, from_ref) = styles(from);
        let (to_text, to_ref) = styles(to);
        let notes: Vec<Note> = self
            .doc
            .notes
            .iter()
            .filter(|n| n.kind == from && n.note_type == NoteType::Normal)
            .cloned()
            .collect();
        if notes.is_empty() {
            return Ok(());
        }
        let sel = self.selection_range();
        let mut batch = self.batch(EditKind::Structure);
        let r = (|| {
            self.batch_apply(
                &mut batch,
                Transaction::new(missing_styles(&self.doc, &[to_text, to_ref])),
            )?;
            let restyle = |props: &RunProps| {
                let mut p = props.clone();
                if p.style_id.as_deref() == Some(from_ref) {
                    p.style_id = Some(to_ref.to_owned());
                }
                p
            };
            for old in notes {
                let new_id = self.doc.alloc_id();
                // The new note's paragraphs: fresh ids, the other kind's styles, no reference
                // mark yet (a note may only point to itself once it exists).
                let mut blocks = Vec::new();
                let mut first_para = None;
                let mut self_props = RunProps::default();
                for b in &old.blocks {
                    let Block::Paragraph(p) = b else {
                        continue;
                    };
                    let mut np = Paragraph::new(self.doc.alloc_id());
                    np.props = p.props.clone();
                    np.mark_props = p.mark_props.clone();
                    if np.props.style_id.as_deref() == Some(from_text) {
                        np.props.style_id = Some(to_text.to_owned());
                    }
                    let mut c = Inlines::new();
                    for (range, props, _) in p.content.run_texts() {
                        let Ok(mut f) = p.content.slice(range.start, range.end) else {
                            continue;
                        };
                        if f.objects
                            .iter()
                            .any(|o| matches!(o, InlineObject::NoteRef(n) if n.note == old.id))
                        {
                            self_props = restyle(props);
                            continue;
                        }
                        for r in &mut f.runs {
                            r.props = restyle(props);
                        }
                        let at = c.len();
                        c.insert(at, f).map_err(doc_err)?;
                    }
                    np.content = c;
                    first_para.get_or_insert(np.id);
                    blocks.push(Block::Paragraph(np));
                }
                if blocks.is_empty() {
                    let np = Paragraph::new(self.doc.alloc_id());
                    first_para = Some(np.id);
                    blocks.push(Block::Paragraph(np));
                }
                let new_ref = InlineObject::NoteRef(NoteRef {
                    note: new_id,
                    kind: to,
                    custom_mark: None,
                });
                let mut ops = vec![Op::InsertNote {
                    index: self.doc.notes.len(),
                    note: Box::new(Note {
                        id: new_id,
                        kind: to,
                        note_type: NoteType::Normal,
                        blocks,
                    }),
                }];
                if let Some(fp) = first_para {
                    ops.push(Op::InsertInline {
                        para: fp,
                        offset: 0,
                        fragment: Fragment::object(new_ref.clone(), self_props),
                    });
                }
                // The references in the text follow the new note.
                for (pos, note, _) in self.note_refs() {
                    if note != old.id {
                        continue;
                    }
                    let props = self.props_at(Pos::new(pos.para, pos.offset + 1));
                    ops.push(Op::SetObject {
                        para: pos.para,
                        offset: pos.offset,
                        object: new_ref.clone(),
                    });
                    ops.push(Op::SetRuns {
                        para: pos.para,
                        offset: pos.offset,
                        runs: vec![kiln_doc::RunSpan::new(1, restyle(&props))],
                    });
                }
                // The old note goes: its own reference mark first, then the record.
                for b in &old.blocks {
                    if let Block::Paragraph(p) = b {
                        let mut marks: Vec<usize> = p
                            .content
                            .objects_with_offsets()
                            .filter(
                                |(_, o)| matches!(o, InlineObject::NoteRef(n) if n.note == old.id),
                            )
                            .map(|(off, _)| off)
                            .collect();
                        marks.sort_unstable_by(|a, b| b.cmp(a));
                        for off in marks {
                            ops.push(Op::DeleteInline {
                                para: p.id,
                                offset: off,
                                len: 1,
                            });
                        }
                    }
                }
                ops.push(Op::RemoveNote { id: old.id });
                self.batch_apply(&mut batch, Transaction::new(ops))?;
            }
            Ok(())
        })();
        let after = if self.doc.paragraph(sel.head.para).is_some() {
            sel
        } else {
            Range::caret(self.doc_start())
        };
        self.batch_commit(batch, after);
        r
    }
}
