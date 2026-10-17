// SPDX-License-Identifier: AGPL-3.0-or-later
//! Captions and cross-references.
//!
//! A caption is a paragraph in the Caption style holding the label, a `SEQ Label` field (and,
//! with chapter numbers, a `STYLEREF n \s` field and a hyphen) and the caption text. Inserting
//! one renumbers every caption of its label in document order.
//!
//! A cross-reference is a REF, PAGEREF or NOTEREF field to a hidden `_Ref…` bookmark the
//! command adds around its target (a heading's text, a caption's label and number or its whole
//! text, a note reference), or to one of the user's bookmarks; with "Insert as hyperlink" the
//! field carries `\h` and Ctrl+click follows it. The results are computed now (page numbers from
//! the layout), and every reference field of the document is brought up to date with them.

use std::cmp::Ordering;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::fields::FieldKind;
use kiln_doc::{
    Block, EditKind, Field, Fragment, InlineObject, Inlines, Op, Paragraph, Pos, Range, RunProps,
    Story, Transaction,
};
use kiln_protocol::commands::write_features::{
    CaptionPosition, CaptionSpec, CrossRefItem, CrossRefShow, CrossRefSpec, CrossRefTarget,
};

use super::super::WriteDoc;
use super::missing_styles;

fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// The SEQ identifier of a caption paragraph and the offset of its SEQ field.
fn seq_of(p: &Paragraph) -> Option<(String, usize)> {
    p.content
        .objects_with_offsets()
        .find_map(|(off, o)| match o {
            InlineObject::Field(f) => match f.parsed().kind {
                FieldKind::Seq(s) if !s.hidden => Some((s.identifier, off)),
                _ => None,
            },
            _ => None,
        })
}

/// A body paragraph's id, outline level and fields (offset, field).
type ParaFields = (NodeId, u8, Vec<(usize, Field)>);

/// A position after ops that insert into its paragraph at or before it.
fn shifted(ops: &[Op], mut pos: Pos) -> Pos {
    for op in ops {
        if let Op::InsertInline {
            para,
            offset,
            fragment,
        } = op
            && *para == pos.para
            && *offset <= pos.offset
        {
            pos.offset += fragment.len();
        }
    }
    pos
}

impl WriteDoc {
    /// The block a caption goes beside: the selected object's paragraph, the table at the
    /// caret, else the caret's paragraph.
    fn caption_target(&self) -> Result<NodeId> {
        if let Some((para, _, _)) = self.selected_object() {
            return Ok(para);
        }
        let (s, _) = self.span();
        if let Some((t, _, _)) = self.doc.enclosing_cell(s.para) {
            return Ok(t.id);
        }
        Ok(s.para)
    }

    /// Insert Caption.
    pub(super) fn insert_caption(&mut self, spec: &CaptionSpec) -> Result<()> {
        let label = spec.label.trim();
        if label.is_empty()
            || label.chars().count() > 40
            || label.chars().any(|c| c.is_whitespace() && c != ' ')
            || label.contains(['"', '\\'])
        {
            return Err(KilnError::InvalidInput(
                "a caption label of 1–40 characters without quotes or backslashes is needed".into(),
            ));
        }
        if spec.chapter_level.is_some_and(|l| !(1..=9).contains(&l)) {
            return Err(KilnError::InvalidInput(
                "chapter numbers come from heading levels 1 to 9".into(),
            ));
        }
        let target = self.caption_target()?;
        let (container, index) = self
            .doc
            .parent_of(target)
            .ok_or_else(|| KilnError::InvalidInput("no place for a caption here".into()))?;
        if matches!(self.doc.story_of(target), Some(Story::Comment(_))) {
            return Err(KilnError::InvalidInput("comments have no captions".into()));
        }
        let insert_at = match spec.position {
            CaptionPosition::Below => index + 1,
            CaptionPosition::Above => index,
        };
        let seq_name = label.replace(' ', "_");
        let mut batch = self.batch(EditKind::Structure);
        let r = (|| {
            self.batch_apply(
                &mut batch,
                Transaction::new(missing_styles(&self.doc, &["Caption"])),
            )?;
            let mut p = Paragraph::new(self.doc.alloc_id());
            p.props.style_id = Some("Caption".into());
            let mut c = Inlines::new();
            let push_text = |c: &mut Inlines, t: &str| -> Result<()> {
                let at = c.len();
                c.insert(at, Fragment::text(t, RunProps::default()))
                    .map_err(doc_err)
            };
            if !spec.exclude_label {
                push_text(&mut c, &format!("{label} "))?;
            }
            let mut seq = format!("SEQ {seq_name} \\* ARABIC");
            if let Some(level) = spec.chapter_level {
                let at = c.len();
                c.insert(
                    at,
                    Fragment::object(
                        InlineObject::Field(Field::new(&format!("STYLEREF {level} \\s"), "1")),
                        RunProps::default(),
                    ),
                )
                .map_err(doc_err)?;
                push_text(&mut c, "-")?;
                seq.push_str(&format!(" \\s {level}"));
            }
            let at = c.len();
            c.insert(
                at,
                Fragment::object(
                    InlineObject::Field(Field::new(&seq, "1")),
                    RunProps::default(),
                ),
            )
            .map_err(doc_err)?;
            let text = spec.text.replace(['\r', '\n'], " ");
            if !text.is_empty() {
                push_text(&mut c, &text)?;
            }
            p.content = c;
            let id = p.id;
            let len = p.content.len();
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index: insert_at,
                    blocks: vec![Block::Paragraph(p)],
                }]),
            )?;
            let ops = self.reference_field_updates();
            self.batch_apply(&mut batch, Transaction::new(ops))?;
            Ok(Pos::new(id, len))
        })();
        match r {
            Ok(end) => {
                self.batch_commit(batch, Range::caret(end));
                Ok(())
            }
            Err(e) => {
                let sel = self.selection_range();
                self.batch_commit(batch, sel);
                Err(e)
            }
        }
    }

    /// New results of every SEQ, STYLEREF, REF, PAGEREF and NOTEREF field in the body (after
    /// captions or references changed).
    #[allow(clippy::too_many_lines)]
    pub(super) fn reference_field_updates(&mut self) -> Vec<Op> {
        let labels = self.doc.list_labels();
        let notes = kiln_doc_layout::engine::note_numbers(&self.doc);
        let bookmarks = self.doc.bookmarks();
        let mut counters: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        // Heading numbers per level for chapter numbering and SEQ resets.
        let mut chapter: [String; 9] = Default::default();
        let mut heading_count = [0i64; 9];
        let mut fields: Vec<(NodeId, usize, Field, String)> = Vec::new();
        let paras: Vec<ParaFields> = self
            .doc
            .body_paragraphs()
            .iter()
            .map(|p| {
                let level = self.doc.resolve_paragraph(p).outline_level;
                let fs = p
                    .content
                    .objects_with_offsets()
                    .filter_map(|(o, x)| match x {
                        InlineObject::Field(f) if !f.locked => Some((o, f.clone())),
                        _ => None,
                    })
                    .collect();
                (p.id, level, fs)
            })
            .collect();
        let mut resets: Vec<(String, u32)> = Vec::new();
        for (id, level, fs) in &paras {
            if *level < 9 {
                let l = usize::from(*level);
                heading_count[l] += 1;
                for c in heading_count.iter_mut().skip(l + 1) {
                    *c = 0;
                }
                chapter[l] = labels
                    .get(id)
                    .map(|x| x.text.trim_end_matches(['.', ')']).to_owned())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| heading_count[l].to_string());
                // A heading at or above a SEQ's reset level restarts it.
                for (name, n) in &resets {
                    if u32::from(*level) < *n {
                        counters.insert(name.clone(), 0);
                    }
                }
            }
            for (off, f) in fs {
                let parsed = f.parsed();
                let result = match &parsed.kind {
                    FieldKind::Seq(s) => {
                        if let Some(n) = s.reset_at_heading
                            && !resets.iter().any(|(x, _)| *x == s.identifier)
                        {
                            resets.push((s.identifier.clone(), n));
                        }
                        let c = counters.entry(s.identifier.clone()).or_insert(0);
                        if let Some(r) = s.reset_to {
                            *c = r - 1;
                        }
                        if !s.repeat {
                            *c += 1;
                        }
                        Some(kiln_doc::fields::apply_number_switches(*c, &parsed.general))
                    }
                    _ if parsed.keyword == "STYLEREF" => {
                        let level = parsed
                            .args
                            .first()
                            .and_then(|a| a.trim().parse::<usize>().ok())
                            .filter(|l| (1..=9).contains(l));
                        level.map(|l| chapter[l - 1].clone())
                    }
                    _ => None,
                };
                if let Some(r) = result {
                    fields.push((*id, *off, f.clone(), r));
                }
            }
        }
        // REF / PAGEREF / NOTEREF from the bookmarks, the layout and the note numbers.
        self.view_synced();
        let Some(v) = self.view.as_deref() else {
            return Vec::new();
        };
        let layout = &v.layout;
        let doc = &self.doc;
        for (id, _, fs) in &paras {
            for (off, f) in fs {
                let parsed = f.parsed();
                let result = match &parsed.kind {
                    FieldKind::Ref(r) => {
                        let b = bookmarks.iter().find(|b| b.name == r.bookmark);
                        if parsed.switches.iter().any(|s| s.name == "\\p") {
                            b.and_then(|b| {
                                doc.compare(Pos::new(b.para, b.offset), Pos::new(*id, *off))
                            })
                            .map(|o| {
                                if o == Ordering::Less {
                                    "above"
                                } else {
                                    "below"
                                }
                                .to_owned()
                            })
                        } else if r.paragraph_number.is_some() {
                            b.and_then(|b| labels.get(&b.para))
                                .map(|l| l.text.trim_end_matches(['.', ')']).to_owned())
                        } else {
                            doc.bookmark_text(&r.bookmark)
                        }
                    }
                    FieldKind::PageRef(r) => bookmarks
                        .iter()
                        .find(|b| b.name == r.bookmark)
                        .and_then(|b| kiln_doc_layout::fields::page_of(layout, b.para))
                        .map(|(_, t)| t),
                    FieldKind::NoteRef(r) => bookmarks
                        .iter()
                        .find(|b| b.name == r.bookmark)
                        .and_then(|b| {
                            let p = doc.paragraph(b.para)?;
                            p.content.objects_with_offsets().find_map(|(o, x)| match x {
                                InlineObject::NoteRef(n) if o >= b.offset => {
                                    notes.get(&n.note).cloned()
                                }
                                _ => None,
                            })
                        }),
                    _ => None,
                };
                if let Some(r) = result {
                    fields.push((*id, *off, f.clone(), r));
                }
            }
        }
        // SEQ results may feed REF results through bookmarks: REF to a caption reads the
        // caption text as it is now, which is fine for a single pass after the numbering.
        fields
            .into_iter()
            .filter(|(_, _, f, r)| f.result != *r)
            .map(|(para, offset, f, result)| Op::SetObject {
                para,
                offset,
                object: InlineObject::Field(Field {
                    result,
                    dirty: false,
                    ..f
                }),
            })
            .collect()
    }

    /// Insert Cross-reference at the caret.
    pub(super) fn insert_cross_reference(&mut self, spec: &CrossRefSpec) -> Result<()> {
        let (s, _) = self.span();
        let mut batch = self.batch(EditKind::Structure);
        let mut caret = s;
        let r = (|| {
            let mut taken = Vec::new();
            let (bookmark, kind) = match &spec.target {
                CrossRefTarget::Bookmark { name } => {
                    if !self.doc.bookmarks().iter().any(|b| b.name == *name) {
                        return Err(KilnError::InvalidInput(format!(
                            "the bookmark '{name}' does not exist"
                        )));
                    }
                    (name.clone(), "REF")
                }
                CrossRefTarget::Heading { para } => {
                    let (name, ops) = self.heading_bookmark(*para, "_Ref", &mut taken)?;
                    caret = shifted(&ops, caret);
                    self.batch_apply(&mut batch, Transaction::new(ops))?;
                    (name, "REF")
                }
                CrossRefTarget::Caption { para } => {
                    let p = self.paragraph(*para)?;
                    let (_, seq_off) = seq_of(p).ok_or_else(|| {
                        KilnError::InvalidInput("that paragraph is not a caption".into())
                    })?;
                    let len = p.content.len();
                    let end = if spec.show == CrossRefShow::Number {
                        seq_off + 1
                    } else {
                        len
                    };
                    let (name, ops) = self.range_bookmark(*para, 0, end, &mut taken)?;
                    caret = shifted(&ops, caret);
                    self.batch_apply(&mut batch, Transaction::new(ops))?;
                    (name, "REF")
                }
                CrossRefTarget::Note { note } => {
                    let at = self
                        .doc
                        .body_paragraphs()
                        .iter()
                        .find_map(|p| {
                            p.content.objects_with_offsets().find_map(|(o, x)| match x {
                                InlineObject::NoteRef(n) if n.note == *note => {
                                    Some(Pos::new(p.id, o))
                                }
                                _ => None,
                            })
                        })
                        .ok_or_else(|| {
                            KilnError::InvalidInput("that note has no reference".into())
                        })?;
                    let (name, ops) =
                        self.range_bookmark(at.para, at.offset, at.offset + 1, &mut taken)?;
                    caret = shifted(&ops, caret);
                    self.batch_apply(&mut batch, Transaction::new(ops))?;
                    (name, "NOTEREF")
                }
            };
            let h = if spec.hyperlink { " \\h" } else { "" };
            let instr = match (spec.show, kind) {
                (CrossRefShow::PageNumber, _) => format!("PAGEREF {bookmark}{h}"),
                (CrossRefShow::AboveBelow, _) => format!("REF {bookmark} \\p{h}"),
                (_, "NOTEREF") => format!("NOTEREF {bookmark}{h}"),
                (CrossRefShow::Number, _)
                    if matches!(spec.target, CrossRefTarget::Heading { .. }) =>
                {
                    format!("REF {bookmark} \\r{h}")
                }
                _ => format!("REF {bookmark}{h}"),
            };
            // The caret moved right by the bookmark markers added before it in its paragraph.
            let at = self.doc.clamp(caret).unwrap_or(s);
            let props = self.props_at(at);
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: Fragment::object(InlineObject::Field(Field::new(&instr, "")), props),
                }]),
            )?;
            let ops = self.reference_field_updates();
            self.batch_apply(&mut batch, Transaction::new(ops))?;
            Ok(Pos::new(at.para, at.offset + 1))
        })();
        match r {
            Ok(after) => {
                self.batch_commit(batch, Range::caret(after));
                Ok(())
            }
            Err(e) => {
                let sel = self.selection_range();
                self.batch_commit(batch, sel);
                Err(e)
            }
        }
    }

    /// A `_Ref` bookmark around `start..end` of a paragraph (an existing one when it covers
    /// exactly that range): its name and the ops that add it.
    fn range_bookmark(
        &self,
        para: NodeId,
        start: usize,
        end: usize,
        taken: &mut Vec<String>,
    ) -> Result<(String, Vec<Op>)> {
        let p = self.paragraph(para)?;
        let starts: Vec<(usize, String)> = p
            .content
            .objects_with_offsets()
            .filter_map(|(o, x)| match x {
                InlineObject::BookmarkStart { name } if name.starts_with("_Ref") => {
                    Some((o, name.clone()))
                }
                _ => None,
            })
            .collect();
        for (o, name) in starts {
            let end_at = p.content.objects_with_offsets().find_map(|(e, x)| match x {
                InlineObject::BookmarkEnd { name: n } if *n == name => Some(e),
                _ => None,
            });
            if o + 1 == start && end_at == Some(end) {
                return Ok((name, Vec::new()));
            }
        }
        let mut n = 300_000_000u64 + taken.len() as u64 + self.doc.bookmarks().len() as u64;
        let name = loop {
            let c = format!("_Ref{n}");
            if !taken.contains(&c) && !self.doc.bookmarks().iter().any(|b| b.name == c) {
                taken.push(c.clone());
                break c;
            }
            n += 1;
        };
        let ops = vec![
            Op::InsertInline {
                para,
                offset: end,
                fragment: Fragment::object(
                    InlineObject::BookmarkEnd { name: name.clone() },
                    RunProps::default(),
                ),
            },
            Op::InsertInline {
                para,
                offset: start,
                fragment: Fragment::object(
                    InlineObject::BookmarkStart { name: name.clone() },
                    RunProps::default(),
                ),
            },
        ];
        Ok((name, ops))
    }

    /// The Cross-reference dialog's targets: headings, bookmarks, captions by label, notes.
    pub(super) fn cross_ref_targets(&self) -> Vec<CrossRefItem> {
        let labels = self.doc.list_labels();
        let mut out = Vec::new();
        for e in self.doc.outline() {
            if e.text.is_empty() {
                continue;
            }
            let number = labels
                .get(&e.para)
                .map(|l| format!("{} ", l.text))
                .unwrap_or_default();
            out.push(CrossRefItem {
                target: CrossRefTarget::Heading { para: e.para },
                label: format!("{number}{}", e.text),
                category: "Heading".into(),
                level: e.level + 1,
            });
        }
        let mut names: Vec<String> = self
            .doc
            .bookmarks()
            .into_iter()
            .map(|b| b.name)
            .filter(|n| !n.starts_with('_'))
            .collect();
        names.sort();
        names.dedup();
        for name in names {
            out.push(CrossRefItem {
                label: name.clone(),
                target: CrossRefTarget::Bookmark { name },
                category: "Bookmark".into(),
                level: 1,
            });
        }
        for p in self.doc.body_paragraphs() {
            if let Some((label, _)) = seq_of(p) {
                out.push(CrossRefItem {
                    target: CrossRefTarget::Caption { para: p.id },
                    label: p.content.plain_text(true).trim().to_owned(),
                    category: label.replace('_', " "),
                    level: 1,
                });
            }
        }
        let numbers = kiln_doc_layout::engine::note_numbers(&self.doc);
        for n in &self.doc.notes {
            if n.note_type != kiln_doc::st::NoteType::Normal {
                continue;
            }
            let Some(number) = numbers.get(&n.id) else {
                continue;
            };
            let mut ps = Vec::new();
            super::paragraphs_of(&n.blocks, &mut ps);
            let text: String = ps
                .first()
                .map(|p| p.content.plain_text(true))
                .unwrap_or_default()
                .trim()
                .chars()
                .take(60)
                .collect();
            out.push(CrossRefItem {
                target: CrossRefTarget::Note { note: n.id },
                label: format!("{number} {text}"),
                category: match n.kind {
                    kiln_doc::st::NoteKind::Endnote => "Endnote".into(),
                    _ => "Footnote".into(),
                },
                level: 1,
            });
        }
        out
    }
}
