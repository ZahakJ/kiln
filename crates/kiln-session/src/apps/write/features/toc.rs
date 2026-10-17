// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tables of contents and tables of figures.
//!
//! A table is a TOC field whose result is its entry paragraphs (`TOC \o "1-3" \h \z \u`, a
//! `FieldBlock`), so Word updates it as its own. Each heading gets a hidden `_Toc…` bookmark;
//! an entry is a hyperlink to it in the TOC *n* style with a right tab and leader before a
//! `PAGEREF _Toc… \h` field. Inserting a table lays the document out again and fills in the page
//! numbers (the table itself may push headings to later pages). Update Table refreshes the page
//! numbers, or rebuilds the entries. A table of figures (`TOC \h \z \c "Figure"`) lists the
//! caption paragraphs of a label.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::fields::FieldKind;
use kiln_doc::props::{ParagraphProps, TabStop};
use kiln_doc::st::{TabAlignment, TabLeader};
use kiln_doc::{
    Block, Document, EditKind, Field, FieldBlock, Fragment, Hyperlink, InlineObject, Inlines,
    LinkTarget, Mark, Op, Paragraph, Pos, Range, RunProps, Story, Transaction,
};
use kiln_protocol::WriteCommand;
use kiln_protocol::commands::TabLeaderKind;
use kiln_protocol::commands::write_features::TocSpec;

use super::super::WriteDoc;
use super::missing_styles;

fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// Where a table of contents (or of figures, `figures`) is: its container, index there and the
/// field block's id.
pub(super) fn find_toc(doc: &Document, figures: bool) -> Option<(NodeId, usize, NodeId)> {
    fn search(
        blocks: &[Block],
        container: NodeId,
        figures: bool,
    ) -> Option<(NodeId, usize, NodeId)> {
        for (i, b) in blocks.iter().enumerate() {
            match b {
                Block::Field(f) => {
                    if let FieldKind::Toc(t) = f.field.parsed().kind
                        && t.caption_label.is_some() == figures
                    {
                        return Some((container, i, f.id));
                    }
                }
                Block::Sdt(s) => {
                    if let Some(found) = search(&s.blocks, s.id, figures) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    doc.sections
        .iter()
        .find_map(|s| search(&s.blocks, s.id, figures))
}

/// The TOC instruction for a spec.
fn instruction(spec: &TocSpec) -> String {
    let mut s = format!("TOC \\o \"1-{}\"", spec.levels.clamp(1, 9));
    if spec.hyperlinks {
        s.push_str(" \\h");
    }
    s.push_str(" \\z \\u");
    if !spec.page_numbers {
        s.push_str(" \\n");
    }
    s
}

/// The spec a TOC field was made with (for Update Table).
fn spec_of(field: &Field, title: Option<String>) -> TocSpec {
    let parsed = field.parsed();
    let FieldKind::Toc(t) = parsed.kind else {
        return TocSpec::default();
    };
    TocSpec {
        levels: u8::try_from(t.outline_levels.map_or(3, |l| l.to.clamp(1, 9))).unwrap_or(3),
        hyperlinks: t.hyperlinks,
        page_numbers: t.omit_page_numbers.is_none(),
        right_align: true,
        leader: TabLeaderKind::Dot,
        title,
    }
}

/// An entry of a table to build.
struct Entry {
    level: u8,
    text: String,
    bookmark: String,
    page: String,
}

impl WriteDoc {
    /// A fresh `_Toc` bookmark name.
    fn toc_bookmark_name(&self, taken: &mut Vec<String>) -> String {
        let mut n = 100_000_000u64 + self.doc.bookmarks().len() as u64 * 7 + taken.len() as u64;
        loop {
            let name = format!("_Toc{n}");
            if !taken.contains(&name) && !self.doc.bookmarks().iter().any(|b| b.name == name) {
                taken.push(name.clone());
                return name;
            }
            n += 1;
        }
    }

    /// The `_Toc` (or `_Ref`) bookmark around a paragraph's text, adding one when it has none:
    /// the bookmark's name and the ops that add it.
    pub(super) fn heading_bookmark(
        &self,
        para: NodeId,
        prefix: &str,
        taken: &mut Vec<String>,
    ) -> Result<(String, Vec<Op>)> {
        let p = self.paragraph(para)?;
        for (_, o) in p.content.objects_with_offsets() {
            if let InlineObject::BookmarkStart { name } = o
                && name.starts_with(prefix)
            {
                return Ok((name.clone(), Vec::new()));
            }
        }
        let name = if prefix == "_Toc" {
            self.toc_bookmark_name(taken)
        } else {
            let mut n = 200_000_000u64 + taken.len() as u64;
            loop {
                let candidate = format!("{prefix}{n}");
                if !taken.contains(&candidate)
                    && !self.doc.bookmarks().iter().any(|b| b.name == candidate)
                {
                    taken.push(candidate.clone());
                    break candidate;
                }
                n += 1;
            }
        };
        let len = p.content.len();
        let ops = vec![
            Op::InsertInline {
                para,
                offset: len,
                fragment: Fragment::object(
                    InlineObject::BookmarkEnd { name: name.clone() },
                    RunProps::default(),
                ),
            },
            Op::InsertInline {
                para,
                offset: 0,
                fragment: Fragment::object(
                    InlineObject::BookmarkStart { name: name.clone() },
                    RunProps::default(),
                ),
            },
        ];
        Ok((name, ops))
    }

    /// The text width of the section holding a paragraph, twips.
    fn text_width(&self, para: NodeId) -> i32 {
        self.doc
            .section_of(para)
            .or_else(|| self.doc.sections.first())
            .map_or(9360, |s| s.props.content_width())
    }

    /// An entry paragraph.
    fn entry_paragraph(
        &mut self,
        style: &str,
        e: &Entry,
        spec: &TocSpec,
        width: i32,
    ) -> Result<Paragraph> {
        let mut p = Paragraph::new(self.doc.alloc_id());
        let mut props = ParagraphProps {
            style_id: Some(style.to_owned()),
            ..ParagraphProps::default()
        };
        if spec.page_numbers && spec.right_align {
            props.tabs = Some(vec![TabStop {
                pos: width,
                align: TabAlignment::Right,
                leader: Some(match spec.leader {
                    TabLeaderKind::None => TabLeader::None,
                    TabLeaderKind::Dot => TabLeader::Dot,
                    TabLeaderKind::Hyphen => TabLeader::Hyphen,
                    TabLeaderKind::Underscore => TabLeader::Underscore,
                }),
            }]);
        }
        p.props = props;
        let mut c = Inlines::new();
        c.insert(0, Fragment::text(&e.text, RunProps::default()))
            .map_err(doc_err)?;
        if spec.page_numbers {
            let at = c.len();
            let sep = if spec.right_align { "\t" } else { " " };
            c.insert(at, Fragment::text(sep, RunProps::default()))
                .map_err(doc_err)?;
            let at = c.len();
            c.insert(
                at,
                Fragment::object(
                    InlineObject::Field(Field::new(
                        &format!("PAGEREF {} \\h", e.bookmark),
                        &e.page,
                    )),
                    RunProps::default(),
                ),
            )
            .map_err(doc_err)?;
        }
        let len = c.len();
        if spec.hyperlinks && len > 0 {
            c.add_mark(
                0,
                len,
                Mark::Link(Hyperlink {
                    target: LinkTarget::Bookmark(e.bookmark.clone()),
                    tooltip: None,
                    frame: None,
                    history: true,
                }),
            )
            .map_err(doc_err)?;
        }
        p.content = c;
        Ok(p)
    }

    /// The headings a table lists (outside any table of contents), with bookmark ops.
    fn toc_sources(&self, levels: u8, skip: Option<NodeId>) -> Vec<(NodeId, u8, String)> {
        let inside: Vec<NodeId> = skip
            .and_then(|id| {
                self.doc.sections.iter().find_map(|s| {
                    s.blocks.iter().find_map(|b| match b {
                        Block::Field(f) if f.id == id => {
                            let mut ps = Vec::new();
                            super::paragraphs_of(&f.blocks, &mut ps);
                            Some(ps.iter().map(|p| p.id).collect::<Vec<_>>())
                        }
                        _ => None,
                    })
                })
            })
            .unwrap_or_default();
        self.doc
            .outline()
            .into_iter()
            .filter(|e| e.level < levels && !inside.contains(&e.para) && !e.text.is_empty())
            .filter(|e| matches!(self.doc.story_of(e.para), Some(Story::Body(_))))
            .map(|e| (e.para, e.level + 1, e.text))
            .collect()
    }

    /// Caption paragraphs of a label (`SEQ Label` fields), with their text.
    fn figure_sources(&self, label: &str) -> Vec<(NodeId, String)> {
        let mut out = Vec::new();
        for p in self.doc.body_paragraphs() {
            let is_caption = p.content.objects().iter().any(|o| match o {
                InlineObject::Field(f) => match f.parsed().kind {
                    FieldKind::Seq(s) => s.identifier.eq_ignore_ascii_case(label),
                    _ => false,
                },
                _ => false,
            });
            if is_caption {
                out.push((p.id, p.content.plain_text(true).trim().to_owned()));
            }
        }
        out
    }

    /// Where a new table goes: before the caret's paragraph (or after it when the caret is
    /// inside its text), directly in a section.
    fn toc_slot(&self) -> Result<(NodeId, usize)> {
        let (s, _) = self.span();
        let (container, index) = self
            .doc
            .parent_of(s.para)
            .ok_or_else(|| KilnError::InvalidInput("no insertion point".into()))?;
        if self.doc.section(container).is_none() {
            return Err(KilnError::InvalidInput(
                "put the insertion point in the body text, outside tables, to insert a table of contents"
                    .into(),
            ));
        }
        let len = self.para_len(s.para);
        Ok((
            container,
            if s.offset == 0 || len == 0 {
                index
            } else {
                index + 1
            },
        ))
    }

    /// Table of Contents.
    pub(super) fn insert_toc(&mut self, spec: &TocSpec) -> Result<()> {
        if !(1..=9).contains(&spec.levels) {
            return Err(KilnError::InvalidInput(
                "a table of contents shows 1 to 9 levels".into(),
            ));
        }
        let existing = find_toc(&self.doc, false);
        self.features.untracked = true;
        let r = self.build_toc(spec, existing);
        self.features.untracked = false;
        r
    }

    fn build_toc(
        &mut self,
        spec: &TocSpec,
        existing: Option<(NodeId, usize, NodeId)>,
    ) -> Result<()> {
        let (container, mut index) = match existing {
            Some((c, i, _)) => (c, i),
            None => self.toc_slot()?,
        };
        let before = self.selection_range();
        let mut batch = self.batch(EditKind::Structure);
        let mut styles: Vec<String> = (1..=spec.levels).map(|l| format!("TOC{l}")).collect();
        styles.push("TOCHeading".into());
        styles.push("Hyperlink".into());
        let refs: Vec<&str> = styles.iter().map(String::as_str).collect();
        let r = (|| {
            self.batch_apply(
                &mut batch,
                Transaction::new(missing_styles(&self.doc, &refs)),
            )?;
            // Remove the old table (and its title).
            if let Some((c, i, _)) = existing {
                let title_before = i > 0
                    && self
                        .doc
                        .container(c)
                        .ok()
                        .and_then(|bl| bl.get(i - 1))
                        .and_then(Block::as_paragraph)
                        .is_some_and(|p| p.props.style_id.as_deref() == Some("TOCHeading"));
                let start = if title_before { i - 1 } else { i };
                index = start;
                let placeholder_id = self.doc.alloc_id();
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::InsertBlocks {
                        container: c,
                        index: start,
                        blocks: vec![Block::Paragraph(Paragraph::new(placeholder_id))],
                    }]),
                )?;
                let count = if title_before { 2 } else { 1 };
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::RemoveBlocks {
                        container: c,
                        index: start + 1,
                        count,
                    }]),
                )?;
                // The placeholder paragraph goes once the new table is in.
            }
            let placeholder = existing.map(|_| index);
            // Bookmarks on the headings.
            let mut taken = Vec::new();
            let mut entries = Vec::new();
            for (para, level, text) in self.toc_sources(spec.levels, None) {
                let (bookmark, ops) = self.heading_bookmark(para, "_Toc", &mut taken)?;
                self.batch_apply(&mut batch, Transaction::new(ops))?;
                entries.push(Entry {
                    level,
                    text,
                    bookmark,
                    page: String::new(),
                });
            }
            let width = self.text_width(
                self.doc
                    .container(container)
                    .ok()
                    .and_then(|b| b.first())
                    .map_or(NodeId(0), Block::id),
            );
            let mut blocks = Vec::new();
            for e in &entries {
                let style = format!("TOC{}", e.level);
                blocks.push(Block::Paragraph(
                    self.entry_paragraph(&style, e, spec, width)?,
                ));
            }
            if blocks.is_empty() {
                let mut p = Paragraph::new(self.doc.alloc_id());
                p.content = Inlines::from_fragment(Fragment::text(
                    "No table of contents entries found.",
                    RunProps::default(),
                ))
                .map_err(doc_err)?;
                blocks.push(Block::Paragraph(p));
            }
            let field_block = FieldBlock {
                id: self.doc.alloc_id(),
                field: Field::new(&instruction(spec), ""),
                blocks,
            };
            let mut new_blocks = Vec::new();
            if let Some(title) = spec.title.as_deref().filter(|t| !t.trim().is_empty()) {
                let mut p = Paragraph::new(self.doc.alloc_id());
                p.props.style_id = Some("TOCHeading".into());
                p.content =
                    Inlines::from_fragment(Fragment::text(title.trim(), RunProps::default()))
                        .map_err(doc_err)?;
                new_blocks.push(Block::Paragraph(p));
            }
            let field_id = field_block.id;
            new_blocks.push(Block::Field(field_block));
            let n = new_blocks.len();
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index,
                    blocks: new_blocks,
                }]),
            )?;
            if let Some(at) = placeholder {
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::RemoveBlocks {
                        container,
                        index: at + n,
                        count: 1,
                    }]),
                )?;
            }
            // Page numbers from the new layout (the table itself moves headings down).
            if spec.page_numbers {
                let ops = self.toc_page_ops(field_id);
                self.batch_apply(&mut batch, Transaction::new(ops))?;
            }
            Ok(())
        })();
        let after = if self.doc.paragraph(before.head.para).is_some() {
            before
        } else {
            Range::caret(self.doc_start())
        };
        self.batch_commit(batch, after);
        r
    }

    /// `SetObject` ops that bring the PAGEREF results of a field block's entries up to date.
    fn toc_page_ops(&mut self, field_block: NodeId) -> Vec<Op> {
        self.view_synced();
        let mut ops = Vec::new();
        let Some(v) = self.view.as_deref() else {
            return ops;
        };
        let layout = &v.layout;
        let doc = &self.doc;
        let bookmarks = doc.bookmarks();
        let Some(blocks) = doc.sections.iter().find_map(|s| {
            fn find(blocks: &[Block], id: NodeId) -> Option<&Vec<Block>> {
                blocks.iter().find_map(|b| match b {
                    Block::Field(f) if f.id == id => Some(&f.blocks),
                    Block::Sdt(s) => find(&s.blocks, id),
                    _ => None,
                })
            }
            find(&s.blocks, field_block)
        }) else {
            return ops;
        };
        let mut paras = Vec::new();
        super::paragraphs_of(blocks, &mut paras);
        for p in paras {
            for (offset, o) in p.content.objects_with_offsets() {
                let InlineObject::Field(f) = o else { continue };
                let FieldKind::PageRef(r) = f.parsed().kind else {
                    continue;
                };
                let page = bookmarks
                    .iter()
                    .find(|b| b.name == r.bookmark)
                    .and_then(|b| kiln_doc_layout::fields::page_of(layout, b.para))
                    .map(|(_, text)| text)
                    .unwrap_or_else(|| "Error! Bookmark not defined.".to_owned());
                if page != f.result {
                    ops.push(Op::SetObject {
                        para: p.id,
                        offset,
                        object: InlineObject::Field(Field {
                            result: page,
                            dirty: false,
                            ..f.clone()
                        }),
                    });
                }
            }
        }
        ops
    }

    /// Update Table (page numbers only, or the entire table).
    pub(super) fn update_toc(&mut self, entire: bool) -> Result<()> {
        // The table at the caret first (a table of figures there), else the first TOC.
        let (s, _) = self.span();
        let at_caret = [true, false].into_iter().find_map(|figures| {
            let (c, i, id) = find_toc(&self.doc, figures)?;
            let inside = self.doc.container(c).ok()?.get(i).is_some_and(|b| {
                let mut ps = Vec::new();
                super::paragraphs_of(std::slice::from_ref(b), &mut ps);
                ps.iter().any(|p| p.id == s.para)
            });
            inside.then_some((figures, id))
        });
        let target = at_caret.or_else(|| find_toc(&self.doc, false).map(|t| (false, t.2)));
        let Some((figures, id)) = target else {
            return Err(KilnError::InvalidInput(
                "the document has no table of contents to update".into(),
            ));
        };
        if entire {
            let field = self.field_block(id).map(|f| f.field.clone());
            let Some(field) = field else {
                return Ok(());
            };
            if figures {
                let label = match field.parsed().kind {
                    FieldKind::Toc(t) => t.caption_label.unwrap_or_else(|| "Figure".into()),
                    _ => "Figure".into(),
                };
                return self.insert_table_of_figures(&label);
            }
            let title = find_toc(&self.doc, false).and_then(|(c, i, _)| {
                (i > 0)
                    .then(|| {
                        self.doc
                            .container(c)
                            .ok()?
                            .get(i - 1)?
                            .as_paragraph()
                            .cloned()
                    })
                    .flatten()
                    .filter(|p| p.props.style_id.as_deref() == Some("TOCHeading"))
                    .map(|p| p.content.plain_text(true))
            });
            let mut spec = spec_of(&field, title);
            // Keep the leader and alignment the table was made with.
            if let Some(first) = self.field_block(id).and_then(|f| {
                let mut ps = Vec::new();
                super::paragraphs_of(&f.blocks, &mut ps);
                ps.first().map(|p| p.props.clone())
            }) {
                match first.tabs.as_ref().and_then(|t| t.first()) {
                    Some(t) => {
                        spec.leader = match t.leader {
                            Some(TabLeader::Hyphen) => TabLeaderKind::Hyphen,
                            Some(TabLeader::Underscore) => TabLeaderKind::Underscore,
                            Some(TabLeader::None) | None => TabLeaderKind::None,
                            _ => TabLeaderKind::Dot,
                        };
                    }
                    None => spec.right_align = false,
                }
            }
            return self.insert_toc(&spec);
        }
        let ops = self.toc_page_ops(id);
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Other)
                .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    fn field_block(&self, id: NodeId) -> Option<&FieldBlock> {
        fn find(blocks: &[Block], id: NodeId) -> Option<&FieldBlock> {
            blocks.iter().find_map(|b| match b {
                Block::Field(f) if f.id == id => Some(f),
                Block::Sdt(s) => find(&s.blocks, id),
                _ => None,
            })
        }
        self.doc.sections.iter().find_map(|s| find(&s.blocks, id))
    }

    /// Remove Table of Contents (and its title).
    pub(super) fn remove_toc(&mut self) -> Result<()> {
        let Some((c, i, _)) = find_toc(&self.doc, false) else {
            return Ok(());
        };
        let title_before = i > 0
            && self
                .doc
                .container(c)
                .ok()
                .and_then(|b| b.get(i - 1))
                .and_then(Block::as_paragraph)
                .is_some_and(|p| p.props.style_id.as_deref() == Some("TOCHeading"));
        let start = if title_before { i - 1 } else { i };
        let count = if title_before { 2 } else { 1 };
        let total = self.doc.container(c).map_err(doc_err)?.len();
        let mut ops = Vec::new();
        if total <= count {
            ops.push(Op::InsertBlocks {
                container: c,
                index: total,
                blocks: vec![Block::Paragraph(Paragraph::new(self.doc.alloc_id()))],
            });
        }
        ops.push(Op::RemoveBlocks {
            container: c,
            index: start,
            count,
        });
        let before = self.selection_range();
        let after_block = self
            .doc
            .container(c)
            .ok()
            .and_then(|b| {
                b.get(start + count)
                    .or_else(|| b.get(start.saturating_sub(1)))
            })
            .map(Block::id);
        let mut batch = self.batch(EditKind::Structure);
        let r = self.batch_apply(&mut batch, Transaction::new(ops));
        let caret = after_block
            .and_then(|id| self.doc.paragraph(id).map(|p| Pos::new(p.id, 0)))
            .or_else(|| self.doc.clamp(before.head))
            .unwrap_or_else(|| self.doc_start());
        self.batch_commit(batch, Range::caret(caret));
        r
    }

    /// Add Text: the selected paragraphs become headings of a level (1–9) or body text (0).
    pub(super) fn add_text(&mut self, level: u8) -> Result<()> {
        if level > 9 {
            return Err(KilnError::InvalidInput("TOC levels are 0 to 9".into()));
        }
        let style = if level == 0 {
            "Normal".to_owned()
        } else {
            format!("Heading{level}")
        };
        self.run_editor(&WriteCommand::ApplyStyle { style_id: style })
            .map(|_| ())
    }

    /// Insert Table of Figures for a caption label (replacing an existing one).
    pub(super) fn insert_table_of_figures(&mut self, label: &str) -> Result<()> {
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 40 || label.contains('"') {
            return Err(KilnError::InvalidInput(
                "a caption label of 1–40 characters without quotes is needed".into(),
            ));
        }
        let existing = find_toc(&self.doc, true);
        self.features.untracked = true;
        let r = self.build_figures(label, existing);
        self.features.untracked = false;
        r
    }

    fn build_figures(
        &mut self,
        label: &str,
        existing: Option<(NodeId, usize, NodeId)>,
    ) -> Result<()> {
        let (container, mut index) = match existing {
            Some((c, i, _)) => (c, i),
            None => self.toc_slot()?,
        };
        let before = self.selection_range();
        let spec = TocSpec {
            title: None,
            ..TocSpec::default()
        };
        let mut batch = self.batch(EditKind::Structure);
        let r = (|| {
            self.batch_apply(
                &mut batch,
                Transaction::new(missing_styles(&self.doc, &["TableofFigures", "Hyperlink"])),
            )?;
            if let Some((c, i, _)) = existing {
                index = i;
                let placeholder_id = self.doc.alloc_id();
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![
                        Op::InsertBlocks {
                            container: c,
                            index: i,
                            blocks: vec![Block::Paragraph(Paragraph::new(placeholder_id))],
                        },
                        Op::RemoveBlocks {
                            container: c,
                            index: i + 1,
                            count: 1,
                        },
                    ]),
                )?;
            }
            let mut taken = Vec::new();
            let mut entries = Vec::new();
            for (para, text) in self.figure_sources(label) {
                let (bookmark, ops) = self.heading_bookmark(para, "_Toc", &mut taken)?;
                self.batch_apply(&mut batch, Transaction::new(ops))?;
                entries.push(Entry {
                    level: 1,
                    text,
                    bookmark,
                    page: String::new(),
                });
            }
            let width = self.text_width(
                self.doc
                    .container(container)
                    .ok()
                    .and_then(|b| b.first())
                    .map_or(NodeId(0), Block::id),
            );
            let style = if self.doc.styles.contains("TableofFigures") {
                "TableofFigures"
            } else {
                "TOC1"
            };
            let mut blocks = Vec::new();
            for e in &entries {
                blocks.push(Block::Paragraph(
                    self.entry_paragraph(style, e, &spec, width)?,
                ));
            }
            if blocks.is_empty() {
                let mut p = Paragraph::new(self.doc.alloc_id());
                p.content = Inlines::from_fragment(Fragment::text(
                    "No table of figures entries found.",
                    RunProps::default(),
                ))
                .map_err(doc_err)?;
                blocks.push(Block::Paragraph(p));
            }
            let fb = FieldBlock {
                id: self.doc.alloc_id(),
                field: Field::new(&format!("TOC \\h \\z \\c \"{label}\""), ""),
                blocks,
            };
            let id = fb.id;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index,
                    blocks: vec![Block::Field(fb)],
                }]),
            )?;
            if existing.is_some() {
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::RemoveBlocks {
                        container,
                        index: index + 1,
                        count: 1,
                    }]),
                )?;
            }
            let ops = self.toc_page_ops(id);
            self.batch_apply(&mut batch, Transaction::new(ops))
        })();
        let after = if self.doc.paragraph(before.head.para).is_some() {
            before
        } else {
            Range::caret(self.doc_start())
        };
        self.batch_commit(batch, after);
        r
    }

    /// Follow the hyperlink at the caret: a link into the document (a TOC entry, a
    /// cross-reference) selects its bookmark. Web links are the host's to open.
    pub(super) fn follow_link(&mut self) -> Result<()> {
        let (s, _) = self.span();
        let p = self.paragraph(s.para)?;
        let target = p.content.marks().iter().find_map(|m| match &m.mark {
            Mark::Link(h) if m.start <= s.offset && s.offset <= m.end => Some(h.target.clone()),
            _ => None,
        });
        // A REF/PAGEREF field with \h at the caret links to its bookmark too.
        let field_target = || {
            p.content
                .objects_with_offsets()
                .find_map(|(off, o)| match o {
                    InlineObject::Field(f) if off == s.offset || off + 1 == s.offset => {
                        match f.parsed().kind {
                            FieldKind::Ref(r) | FieldKind::PageRef(r) | FieldKind::NoteRef(r)
                                if r.hyperlink =>
                            {
                                Some(LinkTarget::Bookmark(r.bookmark))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                })
        };
        match target.or_else(field_target) {
            Some(LinkTarget::Bookmark(name)) => {
                let b = self
                    .doc
                    .bookmarks()
                    .into_iter()
                    .find(|b| b.name == name)
                    .ok_or_else(|| {
                        KilnError::InvalidInput(format!("the bookmark '{name}' does not exist"))
                    })?;
                let at = Pos::new(b.para, b.offset + 1);
                self.set_selection(at, at);
                Ok(())
            }
            Some(LinkTarget::Url(url)) => Err(KilnError::Unsupported(format!(
                "opening {url} is the host's (Ctrl+click opens web links in the browser)"
            ))),
            None => Err(KilnError::InvalidInput(
                "there is no link at the insertion point".into(),
            )),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn instructions_round_trip_through_the_field_parser() {
        let spec = TocSpec {
            levels: 4,
            hyperlinks: true,
            page_numbers: false,
            ..TocSpec::default()
        };
        let i = instruction(&spec);
        assert_eq!(i, "TOC \\o \"1-4\" \\h \\z \\u \\n");
        let back = spec_of(&Field::new(&i, ""), None);
        assert_eq!(back.levels, 4);
        assert!(back.hyperlinks && !back.page_numbers);
    }
}
