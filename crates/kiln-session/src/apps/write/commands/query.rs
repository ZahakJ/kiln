// SPDX-License-Identifier: AGPL-3.0-or-later
//! `Query::Write`: what the editor's ribbon states, rulers, panes and dialogs show, computed
//! from the model and the layout (the UI never computes it).

use std::collections::BTreeSet;

use kiln_common::{KilnError, Result};
use kiln_doc::resolve::{RunInput, RunTarget, resolve_run};
use kiln_doc::st::{HeaderFooterKind, LineRule, Orientation, StyleType, VertAlign};
use kiln_doc::{HeaderFooterPart, Mark, Paragraph, RunProps, Story};
use kiln_protocol::commands::{
    CommandLogEntry, DocPropertiesInfo, FormatState, HeaderFooterState, HeadingInfo, ListState,
    PageSetupSpec, PagesInfo, RulerInfo, RulerTabInfo, SectionState, StoryKind, StyleInfo,
    StyleKindInfo, TabStopSpec, TableState, WordCountInfo, WriteQuery, WriteQueryResult,
};
use kiln_protocol::geom::Rect;

use super::format::{hex, underline_kind};
use super::paragraph::{points, tab_align, text_align};
use crate::apps::write::WriteDoc;

fn rect(b: kiln_scene::Bounds) -> Rect {
    Rect::new(b.x, b.y, b.w, b.h)
}

impl WriteDoc {
    /// Answers a Write query.
    pub(in crate::apps::write) fn write_query(&self, q: &WriteQuery) -> Result<WriteQueryResult> {
        Ok(match q {
            WriteQuery::Format => WriteQueryResult::Format(Box::new(self.format_state()?)),
            WriteQuery::Rulers => WriteQueryResult::Rulers(self.rulers().map(Box::new)),
            WriteQuery::Styles => WriteQueryResult::Styles(self.styles_info()),
            WriteQuery::Headings => WriteQueryResult::Headings(self.headings()),
            WriteQuery::Pages => WriteQueryResult::Pages(Box::new(self.pages_info())),
            WriteQuery::FindAll { query } => WriteQueryResult::FindAll(self.find_all(query)),
            WriteQuery::WordCount { include_notes } => {
                WriteQueryResult::WordCount(Box::new(self.word_count(*include_notes)))
            }
            WriteQuery::Properties => {
                WriteQueryResult::Properties(Box::new(self.properties_info()))
            }
            WriteQuery::Clipboard => WriteQueryResult::Clipboard(self.clipboard_data()?),
            WriteQuery::PageSetup => WriteQueryResult::PageSetup(Box::new(self.page_setup_info())),
            WriteQuery::Bookmarks => WriteQueryResult::Bookmarks(
                self.doc
                    .bookmarks()
                    .into_iter()
                    .map(|b| b.name)
                    .filter(|n| !n.starts_with('_'))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            ),
            WriteQuery::CommandLog => WriteQueryResult::CommandLog(
                self.editor
                    .log
                    .iter()
                    .cloned()
                    .collect::<Vec<CommandLogEntry>>(),
            ),
        })
    }

    fn section_index(&self) -> usize {
        if let Some(hf) = self.editor.header_footer {
            return hf.section;
        }
        match self.doc.story_of(self.span().0.para) {
            Some(Story::Body(i)) => i,
            _ => 0,
        }
    }

    fn format_state(&self) -> Result<FormatState> {
        let (s, e) = self.span();
        let p = self
            .doc
            .paragraph(s.para)
            .ok_or_else(|| KilnError::Internal("the caret's paragraph is gone".into()))?;
        let collapsed = s == e;
        let direct = if collapsed {
            self.caret_props()?
        } else {
            p.content.props_at(s.offset).cloned().unwrap_or_default()
        };
        let r = self.resolve_with(p, &direct);
        let mut mixed = Vec::new();
        if !collapsed {
            self.mixed_runs(&r, &mut mixed);
        }
        let pr = self.doc.resolve_paragraph(p);
        let line_multiple =
            (pr.spacing.line_rule == LineRule::Auto).then(|| pr.spacing.line as f32 / 240.0);
        let line_points = (pr.spacing.line_rule != LineRule::Auto).then(|| points(pr.spacing.line));
        let style_name = self
            .doc
            .styles
            .get(&pr.style_id)
            .map_or_else(|| pr.style_id.clone(), kiln_doc::Style::display_name);
        let list = pr.numbering.map(|l| {
            let bullet = self.is_bullet_list(l.num_id);
            let label = if bullet {
                self.doc
                    .numbering
                    .level(l.num_id, l.level, Some(&self.doc.styles))
                    .map(|lvl| {
                        kiln_doc::numbering::formats::bullet_display_text(
                            &lvl.text,
                            lvl.rpr
                                .as_ref()
                                .and_then(|r| r.fonts.as_ref())
                                .and_then(|f| f.ascii.as_deref()),
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            ListState {
                num_id: l.num_id,
                level: l.level,
                bullet,
                label,
            }
        });
        let table = self.table_of(s.para).map(|t| {
            let look = t.table.props.look.unwrap_or_default();
            let multi = self
                .table_of(e.para)
                .is_some_and(|u| u.table.id == t.table.id && (u.row, u.cell) != (t.row, t.cell));
            TableState {
                rows: u32::try_from(t.table.rows.len()).unwrap_or(u32::MAX),
                cols: u32::try_from(t.table.grid_width()).unwrap_or(u32::MAX),
                row: u32::try_from(t.row).unwrap_or(0),
                col: u32::try_from(t.col).unwrap_or(0),
                style: t.table.props.style_id.clone(),
                header_row: look.first_row,
                total_row: look.last_row,
                banded_rows: !look.no_h_band,
                first_column: look.first_column,
                banded_columns: !look.no_v_band,
                repeat_header: t.table.rows[t.row].props.header == Some(true),
                multi_cell: multi,
            }
        });
        let story = match self.doc.story_of(s.para) {
            Some(Story::HeaderFooter(i)) => match self.doc.headers_footers.get(i).map(|h| h.part) {
                Some(HeaderFooterPart::Footer) => StoryKind::Footer,
                _ => StoryKind::Header,
            },
            Some(Story::Note(_)) => StoryKind::Note,
            Some(Story::Comment(_)) => StoryKind::Comment,
            _ => StoryKind::Body,
        };
        let section = self.section_index();
        let header_footer = self.editor.header_footer.map(|hf| {
            let sec = &self.doc.sections[hf.section.min(self.doc.sections.len() - 1)];
            let refs = match hf.part {
                HeaderFooterPart::Header => sec.headers,
                HeaderFooterPart::Footer => sec.footers,
            };
            let part = match hf.part {
                HeaderFooterPart::Header => "Header",
                HeaderFooterPart::Footer => "Footer",
            };
            let kind = match hf.kind {
                HeaderFooterKind::First => "First Page ",
                HeaderFooterKind::Even => "Even Page ",
                HeaderFooterKind::Default if self.doc.settings.even_and_odd_headers => "Odd Page ",
                HeaderFooterKind::Default => "",
            };
            let label = if self.doc.sections.len() > 1 {
                format!("{kind}{part} -Section {}-", hf.section + 1)
            } else {
                format!("{kind}{part}")
            };
            HeaderFooterState {
                different_first: sec.props.title_page,
                different_odd_even: self.doc.settings.even_and_odd_headers,
                linked: hf.section > 0 && refs.get(hf.kind).is_none(),
                has_previous: hf.section > 0,
                label,
            }
        });
        let link = p.content.marks().iter().find_map(|m| match &m.mark {
            Mark::Link(h) if m.start <= s.offset && s.offset <= m.end && m.start < m.end => {
                Some(match &h.target {
                    kiln_doc::LinkTarget::Url(u) => u.clone(),
                    kiln_doc::LinkTarget::Bookmark(b) => format!("#{b}"),
                })
            }
            _ => None,
        });
        Ok(FormatState {
            font: r.fonts.ascii.clone(),
            size: r.size as f32 / 2.0,
            bold: r.bold,
            italic: r.italic,
            underline: r.underline.as_ref().and_then(|u| underline_kind(&u.style)),
            strike: r.strike,
            double_strike: r.dstrike,
            subscript: r.vert_align == VertAlign::Subscript,
            superscript: r.vert_align == VertAlign::Superscript,
            all_caps: r.caps,
            small_caps: r.small_caps,
            hidden: r.hidden,
            color: r.color.rgb().map(hex),
            highlight: r.highlight.as_ref().and_then(|h| h.rgb()).map(hex),
            spacing: r.spacing as f32 / 20.0,
            mixed,
            alignment: Some(text_align(&pr.alignment)),
            line_multiple,
            line_points,
            indent_left: points(pr.indent.left),
            indent_right: points(pr.indent.right),
            first_line: points(if pr.indent.hanging > 0 {
                -pr.indent.hanging
            } else {
                pr.indent.first_line
            }),
            space_before: points(pr.spacing.before),
            space_after: points(pr.spacing.after),
            keep_next: pr.keep_next,
            keep_lines: pr.keep_lines,
            widow_control: pr.widow_control,
            page_break_before: pr.page_break_before,
            contextual_spacing: pr.contextual_spacing,
            outline_level: if pr.outline_level < 9 {
                pr.outline_level + 1
            } else {
                0
            },
            style_id: pr.style_id.clone(),
            style_name,
            char_style_id: r.style_id.clone(),
            list,
            table,
            story,
            header_footer,
            section: self.section_state(section),
            show_marks: self.editor.show_marks,
            collapsed,
            link,
            can_repeat: self.editor.repeat.is_some(),
            tabs: pr
                .tabs
                .iter()
                .filter(|t| t.align != kiln_doc::st::TabAlignment::Clear)
                .map(|t| TabStopSpec {
                    pos: points(t.pos),
                    align: tab_align(&t.align),
                    leader: match t.leader {
                        Some(kiln_doc::st::TabLeader::Dot) => {
                            kiln_protocol::commands::TabLeaderKind::Dot
                        }
                        Some(kiln_doc::st::TabLeader::Hyphen) => {
                            kiln_protocol::commands::TabLeaderKind::Hyphen
                        }
                        Some(kiln_doc::st::TabLeader::Underscore) => {
                            kiln_protocol::commands::TabLeaderKind::Underscore
                        }
                        _ => kiln_protocol::commands::TabLeaderKind::None,
                    },
                })
                .collect(),
            shading: pr
                .shading
                .as_ref()
                .and_then(|s| s.fill.as_ref())
                .and_then(|c| c.val.rgb())
                .map(hex),
            borders: pr.borders != kiln_doc::ParagraphBorders::default(),
        })
    }

    /// Lists the run properties that differ across the selection (scanning at most 4,000 runs).
    fn mixed_runs(&self, first: &kiln_doc::ResolvedRun, mixed: &mut Vec<String>) {
        let (s, e) = self.span();
        let Ok(segs) = kiln_doc::edit::segments(&self.doc, s, e) else {
            return;
        };
        let mut seen = 0;
        let add = |k: &str, mixed: &mut Vec<String>| {
            if !mixed.iter().any(|m| m == k) {
                mixed.push(k.to_owned());
            }
        };
        for (id, a, b) in segs {
            let Some(p) = self.doc.paragraph(id) else {
                continue;
            };
            let mut pos = 0;
            for run in p.content.runs() {
                if pos < b && pos + run.len > a {
                    seen += 1;
                    if seen > 4000 {
                        return;
                    }
                    let r = self.resolve_with(p, &run.props);
                    if r.bold != first.bold {
                        add("bold", mixed);
                    }
                    if r.italic != first.italic {
                        add("italic", mixed);
                    }
                    if r.underline != first.underline {
                        add("underline", mixed);
                    }
                    if r.fonts.ascii != first.fonts.ascii {
                        add("font", mixed);
                    }
                    if r.size != first.size {
                        add("size", mixed);
                    }
                    if r.color != first.color {
                        add("color", mixed);
                    }
                }
                pos += run.len;
            }
        }
    }

    fn section_state(&self, i: usize) -> SectionState {
        let Some(s) = self.doc.sections.get(i) else {
            return SectionState::default();
        };
        let p = &s.props;
        SectionState {
            width: points(p.page_size.width),
            height: points(p.page_size.height),
            landscape: p.page_size.orientation == Orientation::Landscape,
            top: points(p.margins.top.abs()),
            bottom: points(p.margins.bottom.abs()),
            left: points(p.margins.left),
            right: points(p.margins.right),
            columns: u8::try_from(p.columns.count.clamp(1, 45)).unwrap_or(1),
            paper: p
                .paper()
                .map_or_else(|| "Custom".to_owned(), |paper| paper.label().to_owned()),
        }
    }

    fn page_setup_info(&self) -> PageSetupSpec {
        let i = self.section_index();
        let Some(s) = self.doc.sections.get(i) else {
            return PageSetupSpec::default();
        };
        let p = &s.props;
        PageSetupSpec {
            top: Some(points(p.margins.top)),
            bottom: Some(points(p.margins.bottom)),
            left: Some(points(p.margins.left)),
            right: Some(points(p.margins.right)),
            gutter: Some(points(p.margins.gutter)),
            header: Some(points(p.margins.header)),
            footer: Some(points(p.margins.footer)),
            width: Some(points(p.page_size.width)),
            height: Some(points(p.page_size.height)),
            landscape: Some(p.page_size.orientation == Orientation::Landscape),
            columns: Some(u8::try_from(p.columns.count.clamp(1, 45)).unwrap_or(1)),
            column_space: Some(points(p.columns.space)),
            vertical_align: Some(match p.v_align {
                Some(kiln_doc::st::PageVAlign::Center) => {
                    kiln_protocol::commands::CellVerticalAlign::Center
                }
                Some(kiln_doc::st::PageVAlign::Bottom) => {
                    kiln_protocol::commands::CellVerticalAlign::Bottom
                }
                _ => kiln_protocol::commands::CellVerticalAlign::Top,
            }),
        }
    }

    fn rulers(&self) -> Option<RulerInfo> {
        let v = self.view.as_ref()?;
        let r = kiln_doc_layout::rulers::rulers(&v.layout, &self.doc, self.head)?;
        let page_rect = r.page_rect;
        let page = v.layout.pages().get(r.page)?;
        let body_top = page.body.y;
        let body_bottom = page.body.y + page.body.h;
        Some(RulerInfo {
            page: u32::try_from(r.page).unwrap_or(0),
            page_rect: rect(page_rect),
            text_rect: rect(r.text_rect),
            columns: r
                .columns
                .iter()
                .map(|(x, w)| Rect::new(*x, page_rect.y, *w, page_rect.h))
                .collect(),
            indent_left: r.indent_left,
            indent_right: r.indent_right,
            first_line: r.first_line,
            tabs: r
                .tabs
                .iter()
                .map(|t| RulerTabInfo {
                    pos: t.pos,
                    align: match t.kind {
                        "center" => kiln_protocol::commands::TabAlign::Center,
                        "right" => kiln_protocol::commands::TabAlign::Right,
                        "decimal" => kiln_protocol::commands::TabAlign::Decimal,
                        "bar" => kiln_protocol::commands::TabAlign::Bar,
                        _ => kiln_protocol::commands::TabAlign::Left,
                    },
                })
                .collect(),
            default_tab: r.default_tab,
            margin_top: body_top,
            margin_bottom: body_bottom,
        })
    }

    /// The preview formatting of a style.
    fn style_preview(&self, s: &kiln_doc::Style) -> (String, f32, bool, bool, Option<String>) {
        let ctx = self.doc.context();
        let (para, run) = match s.kind {
            StyleType::Character => (
                kiln_doc::ParagraphProps::default(),
                RunProps {
                    style_id: Some(s.id.clone()),
                    ..RunProps::default()
                },
            ),
            _ => (
                kiln_doc::ParagraphProps {
                    style_id: Some(s.id.clone()),
                    ..kiln_doc::ParagraphProps::default()
                },
                RunProps::default(),
            ),
        };
        let r = resolve_run(
            &ctx,
            &RunInput {
                paragraph: &para,
                mark_props: &RunProps::default(),
                direct: &run,
                table: None,
                target: RunTarget::Text,
            },
        );
        (
            r.fonts.ascii.clone(),
            r.size as f32 / 2.0,
            r.bold,
            r.italic,
            r.color.rgb().map(hex),
        )
    }

    fn styles_info(&self) -> Vec<StyleInfo> {
        let mut used = BTreeSet::new();
        for (_, p) in self.doc.all_paragraphs() {
            if let Some(s) = &p.props.style_id {
                used.insert(s.clone());
            }
            for r in p.content.runs() {
                if let Some(s) = &r.props.style_id {
                    used.insert(s.clone());
                }
            }
        }
        let (s, _) = self.span();
        let current_para = self
            .doc
            .paragraph(s.para)
            .map(|p| self.doc.resolve_paragraph(p).style_id);
        let current_char = self.doc.paragraph(s.para).and_then(|p| {
            p.content
                .props_at(s.offset)
                .and_then(|r| r.style_id.clone())
        });
        let default_para = self
            .doc
            .styles
            .default_style(StyleType::Paragraph)
            .map(|st| st.id.clone());
        let mut out: Vec<StyleInfo> = self
            .doc
            .styles
            .iter()
            .filter(|st| matches!(st.kind, StyleType::Paragraph | StyleType::Character))
            .filter(|st| {
                let in_use = used.contains(&st.id) || Some(&st.id) == default_para.as_ref();
                in_use || st.quick_style || !(st.hidden || st.semi_hidden)
            })
            // A linked character style shows with its paragraph style.
            .filter(|st| {
                !(st.kind == StyleType::Character
                    && st
                        .link
                        .as_ref()
                        .and_then(|l| self.doc.styles.get(l))
                        .is_some_and(|o| o.kind == StyleType::Paragraph))
            })
            .map(|st| {
                let (font, size, bold, italic, color) = self.style_preview(st);
                let linked = st
                    .link
                    .as_ref()
                    .is_some_and(|l| self.doc.styles.contains(l));
                StyleInfo {
                    id: st.id.clone(),
                    name: st.display_name(),
                    kind: match st.kind {
                        StyleType::Character => StyleKindInfo::Character,
                        StyleType::Table => StyleKindInfo::Table,
                        StyleType::Numbering => StyleKindInfo::Numbering,
                        StyleType::Paragraph if linked => StyleKindInfo::Linked,
                        StyleType::Paragraph => StyleKindInfo::Paragraph,
                    },
                    quick: st.quick_style,
                    hidden: st.hidden || st.semi_hidden,
                    priority: st.ui_priority.unwrap_or(99),
                    based_on: st.based_on.clone(),
                    next: st.next.clone(),
                    font,
                    size,
                    bold,
                    italic,
                    color,
                    current: Some(&st.id) == current_para.as_ref()
                        || Some(&st.id) == current_char.as_ref(),
                    in_use: used.contains(&st.id) || Some(&st.id) == default_para.as_ref(),
                }
            })
            .collect();
        out.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    fn headings(&self) -> Vec<HeadingInfo> {
        let head = self.head;
        let mut out: Vec<HeadingInfo> = Vec::new();
        let body: Vec<&Paragraph> = self.doc.body_paragraphs();
        let caret_rank = body.iter().position(|p| p.id == head.para);
        let mut current: Option<usize> = None;
        for (i, p) in body.iter().enumerate() {
            let r = self.doc.resolve_paragraph(p);
            if r.outline_level >= 9 {
                continue;
            }
            let text = p
                .content
                .plain_text(true)
                .replace(kiln_doc::OBJECT_CHAR, "")
                .trim()
                .to_owned();
            if text.is_empty() {
                continue;
            }
            if caret_rank.is_some_and(|c| i <= c) {
                current = Some(out.len());
            }
            let page = self.view.as_ref().and_then(|v| {
                v.caret(kiln_doc::Pos::new(p.id, 0))
                    .and_then(|c| u32::try_from(c.page + 1).ok())
            });
            out.push(HeadingInfo {
                para: p.id,
                level: r.outline_level + 1,
                text,
                page,
                current: false,
            });
        }
        if let Some(c) = current
            && let Some(h) = out.get_mut(c)
        {
            h.current = true;
        }
        out
    }

    fn pages_info(&self) -> PagesInfo {
        let Some(v) = &self.view else {
            return PagesInfo::default();
        };
        let n = v.layout.pages().len();
        PagesInfo {
            count: u32::try_from(n).unwrap_or(u32::MAX),
            complete: v.layout.is_complete(),
            current: v
                .caret(self.head)
                .map_or(0, |c| u32::try_from(c.page).unwrap_or(0)),
            rects: (0..n.min(2000))
                .filter_map(|i| v.layout.page_rect(i).map(rect))
                .collect(),
            extent: rect(v.layout.extent()),
        }
    }

    fn word_count(&self, include_notes: bool) -> WordCountInfo {
        let (s, e) = self.span();
        let lines = self.view.as_ref().map_or(0, |v| {
            u32::try_from(v.layout.reading_order().len()).unwrap_or(u32::MAX)
        });
        let pages = self.view.as_ref().map_or(1, |v| {
            u32::try_from(v.layout.pages().len()).unwrap_or(u32::MAX)
        });
        if s != e {
            let text = crate::model::DocModel::selected_text(self).unwrap_or_default();
            let st = kiln_doc::Document::statistics_of_text(&text);
            return WordCountInfo {
                pages,
                words: u32::try_from(st.words).unwrap_or(u32::MAX),
                characters: u32::try_from(st.characters).unwrap_or(u32::MAX),
                characters_with_spaces: u32::try_from(st.characters_with_spaces)
                    .unwrap_or(u32::MAX),
                paragraphs: u32::try_from(st.paragraphs).unwrap_or(u32::MAX),
                lines: 0,
                selection: true,
            };
        }
        let st = self
            .doc
            .statistics(kiln_doc::query::StatisticsOptions { include_notes });
        WordCountInfo {
            pages,
            words: u32::try_from(st.words).unwrap_or(u32::MAX),
            characters: u32::try_from(st.characters).unwrap_or(u32::MAX),
            characters_with_spaces: u32::try_from(st.characters_with_spaces).unwrap_or(u32::MAX),
            paragraphs: u32::try_from(st.paragraphs).unwrap_or(u32::MAX),
            lines,
            selection: false,
        }
    }

    fn properties_info(&self) -> DocPropertiesInfo {
        let p = &self.doc.properties;
        let st = self
            .doc
            .statistics(kiln_doc::query::StatisticsOptions::default());
        let pages = self.view.as_ref().map_or(1, |v| {
            u32::try_from(v.layout.pages().len()).unwrap_or(u32::MAX)
        });
        DocPropertiesInfo {
            title: p.title.clone(),
            subject: p.subject.clone(),
            author: p.creator.clone(),
            keywords: p.keywords.clone(),
            description: p.description.clone(),
            last_modified_by: p.last_modified_by.clone(),
            created: p.created.clone(),
            modified: p.modified.clone(),
            revision: p.revision.map(|r| r.to_string()),
            application: None,
            pages,
            words: u32::try_from(st.words).unwrap_or(u32::MAX),
            characters: u32::try_from(st.characters_with_spaces).unwrap_or(u32::MAX),
            paragraphs: u32::try_from(st.paragraphs).unwrap_or(u32::MAX),
        }
    }
}
