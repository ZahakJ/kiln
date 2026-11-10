// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pages and sections: page, column and line breaks, Blank Page, section breaks, margins,
//! orientation, paper size, columns and the Page Setup dialog; headers and footers (Edit Header,
//! double-click in the header area, Different First Page, Different Odd & Even, Link to
//! Previous, Close) and Insert ▸ Page Number.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::section::{HeaderFooterRefs, resolve_header_footer};
use kiln_doc::st::{BreakKind, HeaderFooterKind, Orientation, PageVAlign, SectionBreak};
use kiln_doc::{
    Block, Break, EditKind, Field, Fragment, HeaderFooter, HeaderFooterPart, InlineObject, Op,
    Paper, Paragraph, Pos, Range, SectionProps, Story, Transaction, edit,
};
use kiln_protocol::commands::{
    CellVerticalAlign, MarginsPreset, PageNumberPosition, PageOrientation, PageSetupSpec,
    PaperSize, SectionStart,
};
use kiln_protocol::{SelectUnit, WriteCommand};

use super::paragraph::twips;
use super::{HeaderFooterEdit, doc_err};
use crate::apps::write::WriteDoc;

fn paper(size: PaperSize) -> Paper {
    match size {
        PaperSize::Letter => Paper::Letter,
        PaperSize::Legal => Paper::Legal,
        PaperSize::Executive => Paper::Executive,
        PaperSize::A3 => Paper::A3,
        PaperSize::A4 => Paper::A4,
        PaperSize::A5 => Paper::A5,
        PaperSize::B5 => Paper::B5,
        PaperSize::Tabloid => Paper::Tabloid,
    }
}

fn refs(s: &kiln_doc::Section, part: HeaderFooterPart) -> HeaderFooterRefs {
    match part {
        HeaderFooterPart::Header => s.headers,
        HeaderFooterPart::Footer => s.footers,
    }
}

impl WriteDoc {
    /// The sections the selection touches (the edited header's section in header mode).
    fn selected_sections(&self) -> Vec<usize> {
        if let Some(hf) = self.editor.header_footer {
            return vec![hf.section];
        }
        let (s, e) = self.span();
        let index = |p: NodeId| match self.doc.story_of(p) {
            Some(Story::Body(i)) => Some(i),
            _ => None,
        };
        match (index(s.para), index(e.para)) {
            (Some(a), Some(b)) => (a.min(b)..=a.max(b)).collect(),
            (Some(a), None) | (None, Some(a)) => vec![a],
            (None, None) => vec![0],
        }
    }

    /// Changes the properties of the selected sections.
    fn format_sections(&mut self, f: impl Fn(&mut SectionProps)) -> Result<()> {
        let mut ops = Vec::new();
        for i in self.selected_sections() {
            let Some(s) = self.doc.sections.get(i) else {
                continue;
            };
            let mut props = s.props.clone();
            f(&mut props);
            if props != s.props {
                ops.push(Op::SetSectionProps {
                    section: s.id,
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

    /// Inserts a line, page or column break at the caret.
    pub(super) fn insert_break(&mut self, kind: BreakKind) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let props = self.caret_props()?;
        let fragment = Fragment::object(InlineObject::Break(Break { kind, clear: None }), props);
        let insert = Op::InsertInline {
            para: at.para,
            offset: at.offset,
            fragment,
        };
        if kind == BreakKind::TextWrapping {
            let after = Pos::new(at.para, at.offset + 1);
            let tx = Transaction::new(vec![insert])
                .with_kind(EditKind::Structure)
                .with_selection(Some(Range::caret(at)), Some(Range::caret(after)));
            return self.commit(tx);
        }
        // Page and column breaks end their paragraph, as in Word (Ctrl+Enter): what follows the
        // caret starts a new paragraph at the top of the next page or column, where the caret goes.
        let mut batch = self.batch(EditKind::Structure);
        self.batch_apply(&mut batch, Transaction::new(vec![insert]))?;
        let new_para = self.doc.alloc_id();
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![
                Op::split_paragraph(&self.doc, at.para, at.offset + 1, new_para)
                    .map_err(doc_err)?,
            ]),
        )?;
        self.batch_commit(batch, Range::caret(Pos::new(new_para, 0)));
        Ok(())
    }

    /// Blank Page: a page break, an empty paragraph and another page break.
    pub(super) fn blank_page(&mut self) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let props = self.caret_props()?;
        let brk = || {
            InlineObject::Break(Break {
                kind: BreakKind::Page,
                clear: None,
            })
        };
        let mut batch = self.batch(EditKind::Structure);
        let mut frag = Fragment::object(brk(), props.clone());
        let second = Fragment::object(brk(), props);
        frag.text.push_str(&second.text);
        frag.runs.extend(second.runs);
        frag.objects.extend(second.objects);
        let new_para = self.doc.alloc_id();
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![
                Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: frag,
                },
                Op::split_paragraph(&self.doc, at.para, at.offset + 1, new_para)
                    .map_err(doc_err)?,
            ]),
        )?;
        self.batch_commit(batch, Range::caret(Pos::new(new_para, 0)));
        Ok(())
    }

    /// Layout ▸ Breaks ▸ section break at the caret (after the table when the caret is in one).
    pub(super) fn section_break(&mut self, kind: SectionStart) -> Result<()> {
        if self.editor.header_footer.is_some() {
            return Err(KilnError::InvalidInput(
                "section breaks go in the body, not in a header or footer".into(),
            ));
        }
        self.delete_selection()?;
        let kind = match kind {
            SectionStart::NextPage => SectionBreak::NextPage,
            SectionStart::Continuous => SectionBreak::Continuous,
            SectionStart::EvenPage => SectionBreak::EvenPage,
            SectionStart::OddPage => SectionBreak::OddPage,
        };
        let at = self.head;
        let tx = edit::insert_section_break(&mut self.doc, at, kind).map_err(|_| {
            KilnError::InvalidInput(
                "a section break goes between paragraphs of the body, not inside a table".into(),
            )
        })?;
        self.commit(tx)
    }

    /// Layout ▸ Columns.
    pub(super) fn set_columns(&mut self, count: u8) -> Result<()> {
        if !(1..=12).contains(&count) {
            return Err(KilnError::InvalidInput("1–12 columns".into()));
        }
        self.format_sections(move |p| {
            p.columns.count = u32::from(count);
            p.columns.equal_width = true;
            p.columns.cols.clear();
        })
    }

    /// Layout ▸ Margins presets.
    pub(super) fn margins(&mut self, preset: MarginsPreset) -> Result<()> {
        let mirror = preset == MarginsPreset::Mirrored;
        let m = match preset {
            MarginsPreset::Normal => kiln_doc::section::MarginPreset::Normal.margins(),
            MarginsPreset::Narrow => kiln_doc::section::MarginPreset::Narrow.margins(),
            MarginsPreset::Moderate => kiln_doc::section::MarginPreset::Moderate.margins(),
            MarginsPreset::Wide => kiln_doc::section::MarginPreset::Wide.margins(),
            MarginsPreset::Mirrored => kiln_doc::section::PageMargins {
                left: 1800,
                right: 1440,
                ..kiln_doc::section::MarginPreset::Normal.margins()
            },
        };
        let mut batch = self.batch(EditKind::Format);
        if mirror != self.doc.settings.mirror_margins {
            let mut settings = self.doc.settings.clone();
            settings.mirror_margins = mirror;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::SetSettings {
                    settings: Box::new(settings),
                }]),
            )?;
        }
        let mut ops = Vec::new();
        for i in self.selected_sections() {
            if let Some(s) = self.doc.sections.get(i) {
                let mut props = s.props.clone();
                props.margins.top = m.top;
                props.margins.bottom = m.bottom;
                props.margins.left = m.left;
                props.margins.right = m.right;
                ops.push(Op::SetSectionProps {
                    section: s.id,
                    props: Box::new(props),
                });
            }
        }
        self.batch_apply(&mut batch, Transaction::new(ops))?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Layout ▸ Orientation.
    pub(super) fn orientation(&mut self, o: PageOrientation) -> Result<()> {
        let o = match o {
            PageOrientation::Portrait => Orientation::Portrait,
            PageOrientation::Landscape => Orientation::Landscape,
        };
        self.format_sections(move |p| *p = p.with_orientation(o))
    }

    /// Layout ▸ Size.
    pub(super) fn paper(&mut self, size: PaperSize) -> Result<()> {
        let (w, h, code) = paper(size).dimensions();
        self.format_sections(move |p| {
            let landscape = p.page_size.orientation == Orientation::Landscape;
            p.page_size.width = if landscape { h } else { w };
            p.page_size.height = if landscape { w } else { h };
            p.page_size.code = Some(code);
        })
    }

    /// The Page Setup dialog.
    pub(super) fn page_setup(&mut self, s: &PageSetupSpec) -> Result<()> {
        let tw = |v: Option<f32>| v.map(twips).transpose();
        let (top, bottom, left, right) = (tw(s.top)?, tw(s.bottom)?, tw(s.left)?, tw(s.right)?);
        let (gutter, header, footer) = (tw(s.gutter)?, tw(s.header)?, tw(s.footer)?);
        let (width, height, space) = (tw(s.width)?, tw(s.height)?, tw(s.column_space)?);
        for v in [left, right, gutter, header, footer, space]
            .into_iter()
            .flatten()
        {
            if v < 0 {
                return Err(KilnError::InvalidInput(
                    "page lengths cannot be negative".into(),
                ));
            }
        }
        if width.is_some_and(|w| !(1440..=31_680).contains(&w))
            || height.is_some_and(|h| !(1440..=31_680).contains(&h))
        {
            return Err(KilnError::InvalidInput("page sizes are 1–22 inches".into()));
        }
        if s.columns.is_some_and(|c| !(1..=12).contains(&c)) {
            return Err(KilnError::InvalidInput("1–12 columns".into()));
        }
        let s = s.clone();
        let check = self.doc.sections.clone();
        for i in self.selected_sections() {
            if let Some(sec) = check.get(i) {
                let w = width.unwrap_or(sec.props.page_size.width);
                let l = left.unwrap_or(sec.props.margins.left);
                let r = right.unwrap_or(sec.props.margins.right);
                let g = gutter.unwrap_or(sec.props.margins.gutter);
                if l + r + g >= w - 720 {
                    return Err(KilnError::InvalidInput(
                        "the margins leave less than half an inch for text".into(),
                    ));
                }
            }
        }
        self.format_sections(move |p| {
            if let Some(v) = top {
                p.margins.top = v;
            }
            if let Some(v) = bottom {
                p.margins.bottom = v;
            }
            if let Some(v) = left {
                p.margins.left = v;
            }
            if let Some(v) = right {
                p.margins.right = v;
            }
            if let Some(v) = gutter {
                p.margins.gutter = v;
            }
            if let Some(v) = header {
                p.margins.header = v;
            }
            if let Some(v) = footer {
                p.margins.footer = v;
            }
            if let Some(v) = width {
                p.page_size.width = v;
                p.page_size.code = None;
            }
            if let Some(v) = height {
                p.page_size.height = v;
                p.page_size.code = None;
            }
            if let Some(l) = s.landscape {
                *p = p.with_orientation(if l {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                });
            }
            if let Some(c) = s.columns {
                p.columns.count = u32::from(c);
                p.columns.equal_width = true;
                p.columns.cols.clear();
            }
            if let Some(v) = space {
                p.columns.space = v;
            }
            if let Some(a) = s.vertical_align {
                p.v_align = Some(match a {
                    CellVerticalAlign::Top => PageVAlign::Top,
                    CellVerticalAlign::Center => PageVAlign::Center,
                    CellVerticalAlign::Bottom => PageVAlign::Bottom,
                });
            }
        })
    }

    // ----- headers and footers ------------------------------------------------------------

    /// The page the caret is on (the first page before any layout).
    fn caret_page(&mut self) -> usize {
        let head = self.editor.header_footer.map_or(self.head, |hf| hf.body);
        let v = self.view_synced();
        v.caret(head).map_or(0, |c| c.page)
    }

    /// Opens the header or footer of a page for editing, creating an empty one when the page
    /// has none.
    fn open_header_footer(&mut self, page: usize, part: HeaderFooterPart) -> Result<()> {
        let (section, first, number) = {
            let v = self.view_synced();
            let p = v.layout.pages().get(page);
            p.map_or((0, true, 1), |p| {
                (p.section, p.index_in_section == 0, p.number)
            })
        };
        let Some(sec) = self.doc.sections.get(section).cloned() else {
            return Ok(());
        };
        let kind =
            sec.props
                .header_kind_for_page(self.doc.settings.even_and_odd_headers, first, number);
        let all: Vec<HeaderFooterRefs> = self.doc.sections.iter().map(|s| refs(s, part)).collect();
        let body = self.editor.header_footer.map_or(self.head, |hf| hf.body);
        let story = match resolve_header_footer(&all, section, kind) {
            Some(id) => id,
            None => self.create_header_footer(section, part, kind)?,
        };
        let first_para = self
            .doc
            .headers_footers
            .iter()
            .find(|h| h.id == story)
            .and_then(|h| {
                h.blocks.iter().find_map(|b| match b {
                    Block::Paragraph(p) => Some(p.id),
                    _ => None,
                })
            });
        if let Some(p) = first_para {
            self.editor.header_footer = Some(HeaderFooterEdit {
                section,
                part,
                kind,
                body,
            });
            self.set_selection(Pos::new(p, 0), Pos::new(p, 0));
        }
        Ok(())
    }

    /// Adds an empty header or footer story to a section.
    fn create_header_footer(
        &mut self,
        section: usize,
        part: HeaderFooterPart,
        kind: HeaderFooterKind,
    ) -> Result<NodeId> {
        let sec = self
            .doc
            .sections
            .get(section)
            .cloned()
            .ok_or_else(|| KilnError::Internal("no such section".into()))?;
        let style = match part {
            HeaderFooterPart::Header => "Header",
            HeaderFooterPart::Footer => "Footer",
        };
        self.ensure_style(style)?;
        let mut p = Paragraph::new(self.doc.alloc_id());
        if self.doc.styles.contains(style) {
            p.props.style_id = Some(style.to_owned());
        }
        let id = self.doc.alloc_id();
        let story = HeaderFooter {
            id,
            part,
            blocks: vec![Block::Paragraph(p)],
        };
        let (mut headers, mut footers) = (sec.headers, sec.footers);
        match part {
            HeaderFooterPart::Header => headers.set(kind, Some(id)),
            HeaderFooterPart::Footer => footers.set(kind, Some(id)),
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(vec![
                Op::InsertHeaderFooter {
                    index: self.doc.headers_footers.len(),
                    story: Box::new(story),
                },
                Op::SetSectionRefs {
                    section: sec.id,
                    headers,
                    footers,
                },
            ])
            .with_kind(EditKind::Structure)
            .with_selection(Some(sel), Some(sel)),
        )?;
        Ok(id)
    }

    /// Insert ▸ Header / Footer: edits the caret page's header or footer.
    pub(super) fn edit_header_footer(&mut self, part: HeaderFooterPart) -> Result<()> {
        let page = self.caret_page();
        self.open_header_footer(page, part)
    }

    /// Close Header and Footer: back to where the caret was in the body.
    pub(super) fn close_header_footer(&mut self) {
        if let Some(hf) = self.editor.header_footer.take() {
            let back = self
                .doc
                .clamp(hf.body)
                .filter(|p| matches!(self.doc.story_of(p.para), Some(Story::Body(_))))
                .unwrap_or_else(|| self.doc_start());
            self.set_selection(back, back);
        }
    }

    /// A click while editing a header or footer: in a header or footer it moves the caret there;
    /// a double click in the body closes header editing (Word).
    pub(super) fn select_in_header_footer(&mut self, cmd: &WriteCommand) -> Result<()> {
        let WriteCommand::SelectAt {
            point,
            extend,
            unit,
        } = cmd
        else {
            return Ok(());
        };
        let hit = {
            let v = self.view_synced();
            kiln_doc_layout::geometry::hit_test(&v.layout, point.x, point.y, None)
        };
        let Some(hit) = hit else {
            return Ok(());
        };
        match hit.story {
            kiln_doc_layout::StoryKind::HeaderFooter(_) => {
                let p = hit.pos;
                let (a, b) = match unit {
                    SelectUnit::Char => (p, p),
                    SelectUnit::Word => {
                        let (s, e) =
                            crate::apps::write::editing::word_at(&self.para_text(p.para), p.offset);
                        (Pos::new(p.para, s), Pos::new(p.para, e))
                    }
                    SelectUnit::Paragraph => {
                        (Pos::new(p.para, 0), Pos::new(p.para, self.para_len(p.para)))
                    }
                };
                if *extend && self.doc.story_of(self.anchor.para) == self.doc.story_of(p.para) {
                    let anchor = self.anchor;
                    self.set_selection(anchor, b);
                } else {
                    self.set_selection(a, b);
                }
                if let Some(hf) = &mut self.editor.header_footer {
                    // Another section's header: edit that one now.
                    let section = self
                        .view
                        .as_ref()
                        .and_then(|v| v.layout.pages().get(hit.page).map(|pg| pg.section));
                    if let Some(s) = section {
                        hf.section = s;
                    }
                }
            }
            kiln_doc_layout::StoryKind::Body if *unit == SelectUnit::Word => {
                self.editor.header_footer = None;
                self.set_selection(hit.pos, hit.pos);
            }
            _ => {}
        }
        Ok(())
    }

    /// A double click in a page's top or bottom margin opens its header or footer.
    pub(super) fn open_header_footer_at(&mut self, x: f32, y: f32) -> Result<()> {
        let found = {
            let v = self.view_synced();
            v.layout.page_at(x, y).and_then(|i| {
                let page = v.layout.pages().get(i)?;
                let rect = page.rect();
                let body = page.body;
                let local = y - rect.y;
                if local < body.y {
                    Some((i, HeaderFooterPart::Header))
                } else if local > body.y + body.h {
                    Some((i, HeaderFooterPart::Footer))
                } else {
                    None
                }
            })
        };
        match found {
            Some((page, part)) => self.open_header_footer(page, part),
            None => Ok(()),
        }
    }

    /// Different First Page.
    pub(super) fn different_first_page(&mut self) -> Result<()> {
        let section = self.selected_sections().first().copied().unwrap_or(0);
        let Some(s) = self.doc.sections.get(section) else {
            return Ok(());
        };
        let mut props = s.props.clone();
        props.title_page = !props.title_page;
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(vec![Op::SetSectionProps {
                section: s.id,
                props: Box::new(props),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )?;
        self.reopen_header_footer()
    }

    /// Different Odd & Even Pages (a document setting).
    pub(super) fn different_odd_even(&mut self) -> Result<()> {
        let mut settings = self.doc.settings.clone();
        settings.even_and_odd_headers = !settings.even_and_odd_headers;
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(vec![Op::SetSettings {
                settings: Box::new(settings),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )?;
        self.reopen_header_footer()
    }

    /// After an option change, edit the header or footer the caret's page now shows.
    fn reopen_header_footer(&mut self) -> Result<()> {
        if let Some(hf) = self.editor.header_footer {
            let page = self.caret_page();
            self.open_header_footer(page, hf.part)?;
        }
        Ok(())
    }

    /// Link to Previous: linking drops this section's own header or footer (Word asks first;
    /// the UI does); unlinking starts it as a copy of the previous one.
    pub(super) fn link_to_previous(&mut self) -> Result<()> {
        let Some(hf) = self.editor.header_footer else {
            return Err(KilnError::InvalidInput(
                "Link to Previous applies while editing a header or footer".into(),
            ));
        };
        if hf.section == 0 {
            return Ok(());
        }
        let sec = self.doc.sections[hf.section].clone();
        let mut r = refs(&sec, hf.part);
        let all: Vec<HeaderFooterRefs> =
            self.doc.sections.iter().map(|s| refs(s, hf.part)).collect();
        let mut batch = self.batch(EditKind::Structure);
        let caret_story;
        if let Some(own) = r.get(hf.kind) {
            r.set(hf.kind, None);
            let (headers, footers) = match hf.part {
                HeaderFooterPart::Header => (r, sec.footers),
                HeaderFooterPart::Footer => (sec.headers, r),
            };
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::SetSectionRefs {
                    section: sec.id,
                    headers,
                    footers,
                }]),
            )?;
            let still_used = self
                .doc
                .sections
                .iter()
                .any(|s| s.headers.ids().chain(s.footers.ids()).any(|i| i == own));
            if !still_used {
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::RemoveHeaderFooter { id: own }]),
                )?;
            }
            caret_story = resolve_header_footer(&all, hf.section - 1, hf.kind);
        } else {
            let source = resolve_header_footer(&all, hf.section - 1, hf.kind);
            let blocks =
                match source.and_then(|id| self.doc.headers_footers.iter().find(|h| h.id == id)) {
                    Some(h) => {
                        let blocks = h.blocks.clone();
                        super::clipboard::fresh_blocks(&mut self.doc, &blocks)?
                    }
                    None => vec![Block::Paragraph(Paragraph::new(self.doc.alloc_id()))],
                };
            let id = self.doc.alloc_id();
            r.set(hf.kind, Some(id));
            let (headers, footers) = match hf.part {
                HeaderFooterPart::Header => (r, sec.footers),
                HeaderFooterPart::Footer => (sec.headers, r),
            };
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![
                    Op::InsertHeaderFooter {
                        index: self.doc.headers_footers.len(),
                        story: Box::new(HeaderFooter {
                            id,
                            part: hf.part,
                            blocks,
                        }),
                    },
                    Op::SetSectionRefs {
                        section: sec.id,
                        headers,
                        footers,
                    },
                ]),
            )?;
            caret_story = Some(id);
        }
        let caret = caret_story
            .and_then(|id| self.doc.headers_footers.iter().find(|h| h.id == id))
            .and_then(|h| {
                h.blocks.iter().find_map(|b| match b {
                    Block::Paragraph(p) => Some(Pos::new(p.id, 0)),
                    _ => None,
                })
            });
        match caret {
            Some(c) => self.batch_commit(batch, Range::caret(c)),
            None => {
                let back = hf.body;
                self.batch_commit(batch, Range::caret(back));
                self.editor.header_footer = None;
            }
        }
        Ok(())
    }

    /// Insert ▸ Page Number.
    pub(super) fn page_number(&mut self, position: PageNumberPosition) -> Result<()> {
        let field = || InlineObject::Field(Field::new("PAGE", "1"));
        if position == PageNumberPosition::Caret {
            self.delete_selection()?;
            let at = self.head;
            let props = self.caret_props()?;
            let tx = Transaction::new(vec![Op::InsertInline {
                para: at.para,
                offset: at.offset,
                fragment: Fragment::object(field(), props),
            }])
            .with_kind(EditKind::Structure)
            .with_selection(
                Some(Range::caret(at)),
                Some(Range::caret(Pos::new(at.para, at.offset + 1))),
            );
            return self.commit(tx);
        }
        let (part, align) = match position {
            PageNumberPosition::TopLeft => {
                (HeaderFooterPart::Header, kiln_doc::st::Alignment::Left)
            }
            PageNumberPosition::TopCenter => {
                (HeaderFooterPart::Header, kiln_doc::st::Alignment::Center)
            }
            PageNumberPosition::TopRight => {
                (HeaderFooterPart::Header, kiln_doc::st::Alignment::Right)
            }
            PageNumberPosition::BottomLeft => {
                (HeaderFooterPart::Footer, kiln_doc::st::Alignment::Left)
            }
            PageNumberPosition::BottomCenter => {
                (HeaderFooterPart::Footer, kiln_doc::st::Alignment::Center)
            }
            _ => (HeaderFooterPart::Footer, kiln_doc::st::Alignment::Right),
        };
        let page = self.caret_page();
        self.open_header_footer(page, part)?;
        // The caret is at the start of the story now: a paragraph with the number replaces an
        // empty story's paragraph, or comes first.
        let p = self.para(self.head.para)?.clone();
        let mut props = p.props.clone();
        props.alignment = Some(align);
        let mut batch = self.batch(EditKind::Structure);
        if p.content.is_empty() {
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![
                    Op::SetParagraphProps {
                        para: p.id,
                        props: Box::new(props),
                    },
                    Op::InsertInline {
                        para: p.id,
                        offset: 0,
                        fragment: Fragment::object(field(), kiln_doc::RunProps::default()),
                    },
                ]),
            )?;
        } else {
            let (container, index) = self
                .doc
                .parent_of(p.id)
                .ok_or_else(|| KilnError::Internal("a header paragraph without a story".into()))?;
            let mut np = Paragraph::new(self.doc.alloc_id());
            np.props = props;
            np.content = kiln_doc::Inlines::from_fragment(Fragment::object(
                field(),
                kiln_doc::RunProps::default(),
            ))
            .map_err(doc_err)?;
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index,
                    blocks: vec![Block::Paragraph(np)],
                }]),
            )?;
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }
}
