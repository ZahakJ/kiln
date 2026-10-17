// SPDX-License-Identifier: AGPL-3.0-or-later
//! `.odt` → [`kiln_doc::Document`].
//!
//! - **Body:** `text:p`, `text:h` (outline level), `text:list` (nesting depth = list level),
//!   `table:table`, `text:section` (a continuous section with its columns), indexes (tables of
//!   contents and others as multi-paragraph fields). A paragraph or table whose style names a
//!   master page starts a new section on a new page with that page style.
//! - **Paragraph content:** text with ODF white-space processing, `text:span` formatting,
//!   `text:s`, `text:tab`, `text:line-break`, hyperlinks (`text:a`), bookmarks, footnotes and
//!   endnotes (`text:note`), fields (page number and count, dates and times, document
//!   properties, references, sequences), comments (`office:annotation` with ranges, replies and
//!   resolved state), tracked changes (insertions, deletions with their text, format changes),
//!   frames with pictures or text boxes, custom shapes.
//! - **Pages:** master pages and their page layouts become section properties (size,
//!   orientation, margins with header and footer distances, columns, gutter); their headers and
//!   footers (default, even, first) become header and footer stories.
//! - **Styles:** named paragraph and text styles, the default style (document defaults), list
//!   styles (lists), document properties from `meta.xml`.

use std::collections::{BTreeMap, HashMap};

use kiln_common::NodeId;
use kiln_doc::fields::Field;
use kiln_doc::inline::{Break, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark};
use kiln_doc::inline::{MarkSpan, NoteRef, RunSpan};
use kiln_doc::merge::merge_run_props;
use kiln_doc::model::{
    Anchor, Block, Cell, Comment, CommentMeta, Document, Drawing, DrawingKind, FieldBlock,
    HeaderFooter, HeaderFooterPart, Note, Paragraph, Placement, PosH, PosV, Row, Section, Table,
};
use kiln_doc::numbering::NumInstance;
use kiln_doc::props::{
    CellBorders, CellMargins, NumberingRef, ParagraphProps, RowHeight, RunProps, Shading,
    TableCellProps, TableProps, TableRowProps, TableWidth,
};
use kiln_doc::revision::RevisionInfo;
use kiln_doc::section::{Column, Columns, HeaderFooterRefs, PageMargins, PageSize, SectionProps};
use kiln_doc::settings::Media;
use kiln_doc::st::{
    AlignH, AlignV, BreakKind, CellVAlign, HeightRule, NoteKind, NoteType, Orientation,
    RelativeFromH, RelativeFromV, SectionBreak, ShadingPattern, TableAlignment, VMerge, WidthType,
    WrapSide, WrapType,
};
use kiln_doc::style::StyleSheet;
use kiln_doc::{Paper, revision::ParagraphRevision};
use kiln_ooxml::xml::{XmlElement, XmlNode};

use super::lists::read_list_style;
use super::styles::{Scope, StyleIndex};
use crate::package::{OdfKind, Package};
use crate::props::{parse_border, read_para_props, read_run_props};
use crate::units::{color, degrees, emu, twips};
use crate::xml::{all_text, kid, kids, parse_part};
use crate::{OdfError, meta};

/// A text document as read.
#[derive(Debug)]
pub struct OdtRead {
    /// The document (valid: [`Document::validate`] passes).
    pub document: Document,
    /// What could not be represented exactly, for the Compatibility Checker.
    pub warnings: Vec<String>,
    /// The file was a template (`.ott`).
    pub template: bool,
}

/// Reads an `.odt` (or `.ott`) file.
///
/// # Errors
///
/// [`OdfError`] when the file is not a readable ODF text document.
pub fn read_odt(bytes: &[u8]) -> Result<OdtRead, OdfError> {
    let pkg = Package::open(bytes)?;
    read_package(&pkg)
}

/// Reads an opened package.
///
/// # Errors
///
/// [`OdfError`] when the package is not a readable ODF text document.
pub fn read_package(pkg: &Package) -> Result<OdtRead, OdfError> {
    if pkg.kind != Some(OdfKind::Text) {
        return Err(OdfError::NotOdf(format!(
            "{} is not a text document",
            pkg.media_type
        )));
    }
    let content_bytes = pkg
        .file("content.xml")
        .ok_or_else(|| OdfError::Corrupt("content.xml is missing".into()))?;
    let content = parse_part("content.xml", content_bytes)?;
    let styles = match pkg.file("styles.xml") {
        Some(b) => Some(parse_part("styles.xml", b)?),
        None => None,
    };
    let settings = match pkg.file("settings.xml") {
        Some(b) => parse_part("settings.xml", b).ok(),
        None => None,
    };
    let ix = StyleIndex::new(styles.as_ref(), Some(&content));
    let mut r = Reader::new(ix, pkg);
    r.tabs_relative = config_bool(settings.as_ref(), "TabsRelativeToIndent").unwrap_or(true);
    r.read(&content)?;
    let mut doc = r.doc;
    doc.properties = meta::read_meta(pkg.file("meta.xml"))?;
    doc.refresh_id_counter();
    doc.validate().map_err(|e| OdfError::Model(e.to_string()))?;
    Ok(OdtRead {
        document: doc,
        warnings: r.warnings,
        template: pkg.media_type.ends_with("-template"),
    })
}

/// A boolean `config:config-item` of `settings.xml`.
pub(crate) fn config_bool(settings: Option<&XmlElement>, name: &str) -> Option<bool> {
    let root = settings?;
    root.descendants("config:config-item")
        .into_iter()
        .find(|e| e.attr("config:name") == Some(name))
        .map(|e| all_text(e).trim() == "true")
}

// ------------------------------------------------------------------ paragraph builder

#[derive(Debug, Default)]
struct ParaBuilder {
    text: String,
    len: usize,
    runs: Vec<RunSpan>,
    marks: Vec<MarkSpan>,
    objects: Vec<InlineObject>,
    /// The last character was a collapsible space (or the paragraph is empty).
    skip_space: bool,
}

impl ParaBuilder {
    fn new() -> Self {
        Self {
            skip_space: true,
            ..Self::default()
        }
    }

    fn push_run(&mut self, n: usize, props: &RunProps) {
        if n == 0 {
            return;
        }
        match self.runs.last_mut() {
            Some(last) if last.props == *props => last.len += n,
            _ => self.runs.push(RunSpan::new(n, props.clone())),
        }
        self.len += n;
    }

    /// Text with ODF white-space collapsing.
    fn push_text(&mut self, s: &str, props: &RunProps) {
        let mut n = 0;
        for c in s.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                if self.skip_space {
                    continue;
                }
                self.text.push(' ');
                self.skip_space = true;
            } else if c == kiln_doc::OBJECT_CHAR {
                continue;
            } else {
                self.text.push(c);
                self.skip_space = false;
            }
            n += 1;
        }
        self.push_run(n, props);
    }

    /// Characters taken literally (`text:s`, `text:tab`).
    fn push_literal(&mut self, s: &str, props: &RunProps) {
        let n = s.chars().count();
        self.text.push_str(s);
        self.skip_space = false;
        self.push_run(n, props);
    }

    fn push_object(&mut self, o: InlineObject, props: &RunProps) {
        self.text.push(kiln_doc::OBJECT_CHAR);
        self.objects.push(o);
        self.skip_space = false;
        self.push_run(1, props);
    }

    fn finish(self) -> Inlines {
        let fragment = Fragment {
            text: self.text,
            runs: self.runs,
            marks: self.marks,
            objects: self.objects,
        };
        // A fragment built here is consistent by construction; fall back to plain text should
        // the model still refuse it.
        match Inlines::from_fragment(fragment.clone()) {
            Ok(i) => i,
            Err(_) => Inlines::from_fragment(Fragment::text(
                &fragment.text,
                RunProps::default(),
            ))
            .unwrap_or_default(),
        }
    }
}

/// A range annotation that may span paragraphs (a comment or a tracked change).
#[derive(Debug, Clone)]
struct OpenRange {
    key: String,
    mark: Mark,
    start: usize,
}

/// What the current paragraph inherits from its list.
#[derive(Debug, Clone, Copy)]
struct ListCtx {
    num_id: u32,
    level: u8,
}

// ------------------------------------------------------------------ the reader

struct Reader<'a> {
    ix: StyleIndex<'a>,
    pkg: &'a Package,
    doc: Document,
    warnings: Vec<String>,
    /// Num id per entry of `ix.list_styles`.
    list_ids: Vec<u32>,
    open: Vec<OpenRange>,
    regions: HashMap<String, &'a XmlElement>,
    comment_ids: HashMap<String, NodeId>,
    media: HashMap<String, String>,
    tabs_relative: bool,
    revision_id: u32,
    /// Header/footer stories per master page name.
    page_stories: HashMap<String, (HeaderFooterRefs, HeaderFooterRefs, bool)>,
    /// Frames anchored to the page, attached to the first body paragraph.
    page_frames: Vec<InlineObject>,
}

fn warn_once(warnings: &mut Vec<String>, w: &str) {
    if !warnings.iter().any(|x| x == w) {
        warnings.push(w.to_owned());
    }
}

impl<'a> Reader<'a> {
    fn new(ix: StyleIndex<'a>, pkg: &'a Package) -> Self {
        Self {
            ix,
            pkg,
            doc: Document::empty(),
            warnings: Vec::new(),
            list_ids: Vec::new(),
            open: Vec::new(),
            regions: HashMap::new(),
            comment_ids: HashMap::new(),
            media: HashMap::new(),
            tabs_relative: true,
            revision_id: 0,
            page_stories: HashMap::new(),
            page_frames: Vec::new(),
        }
    }

    fn warn(&mut self, w: &str) {
        warn_once(&mut self.warnings, w);
    }

    fn read(&mut self, content: &'a XmlElement) -> Result<(), OdfError> {
        // Lists: one definition and one instance per list style, in definition order.
        for (i, (_, _, el)) in self.ix.list_styles.iter().enumerate() {
            let abs_id = u32::try_from(i).unwrap_or(u32::MAX - 1);
            let num_id = abs_id + 1;
            let def = read_list_style(el, abs_id, &self.ix.fonts);
            self.doc.numbering.abstracts.insert(abs_id, def);
            self.doc.numbering.instances.insert(
                num_id,
                NumInstance {
                    id: num_id,
                    abstract_id: abs_id,
                    overrides: Vec::new(),
                },
            );
            self.list_ids.push(num_id);
        }
        let named = {
            let ix = &self.ix;
            let ids = &self.list_ids;
            ix.named_styles(|n| ix.list_style(Scope::Styles, n).map(|i| ids[i]))
        };
        self.doc.styles = StyleSheet::new(self.ix.doc_defaults(), named);
        if let Some(t) = self.ix.default_tab_stop() {
            self.doc.settings.default_tab_stop = t;
        }
        self.doc.settings.even_and_odd_headers = self.ix.master_pages.iter().any(|m| {
            ["style:header-left", "style:footer-left"]
                .iter()
                .any(|n| kid(m, n).is_some_and(|h| h.attr("style:display") != Some("false")))
        });
        let body = kid(content, "office:body")
            .and_then(|b| kid(b, "office:text"))
            .ok_or_else(|| OdfError::Corrupt("content.xml has no office:text body".into()))?;
        if let Some(tc) = kid(body, "text:tracked-changes") {
            for region in kids(tc, "text:changed-region") {
                if let Some(id) = region.attr("text:id").or_else(|| region.attr("xml:id")) {
                    self.regions.insert(id.to_owned(), region);
                }
            }
        }
        let first_master = self.first_master(body);
        let mut sb = SectionBuilder {
            sections: Vec::new(),
            blocks: Vec::new(),
            master: first_master.clone(),
            columns: None,
            break_type: SectionBreak::NextPage,
        };
        self.body(body, &mut sb, true);
        if !self.page_frames.is_empty() {
            let frames = std::mem::take(&mut self.page_frames);
            self.attach_page_frames(&mut sb, frames);
        }
        self.close_section(&mut sb);
        if sb.sections.is_empty() {
            // An empty document: one section with one empty paragraph.
            let p = Paragraph::new(self.doc.alloc_id());
            sb.blocks.push(Block::Paragraph(p));
            self.close_section(&mut sb);
        }
        self.doc.sections = sb.sections;
        Ok(())
    }

    /// The master page of the first paragraph or table, else `Standard`, else the first.
    fn first_master(&self, body: &XmlElement) -> String {
        fn first_styled(el: &XmlElement) -> Option<(&str, &str)> {
            for e in el.elements() {
                match e.name.as_str() {
                    "text:p" | "text:h" => return e.attr("text:style-name").map(|s| ("paragraph", s)),
                    "table:table" => return e.attr("table:style-name").map(|s| ("table", s)),
                    "text:list" | "text:list-item" | "text:section" => {
                        if let Some(f) = first_styled(e) {
                            return Some(f);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        if let Some((family, name)) = first_styled(body)
            && let Some(m) = self
                .ix
                .get(Scope::Content, family, name)
                .and_then(|s| s.attr("style:master-page-name"))
                .filter(|m| !m.is_empty() && self.ix.master_page(m).is_some())
        {
            return m.to_owned();
        }
        if self.ix.master_page("Standard").is_some() {
            return "Standard".to_owned();
        }
        self.ix
            .master_pages
            .first()
            .and_then(|m| m.attr("style:name"))
            .unwrap_or("Standard")
            .to_owned()
    }

    // ------------------------------------------------------------ sections

    fn master_of(&self, el: &XmlElement) -> Option<String> {
        let (family, attr) = match el.name.as_str() {
            "text:p" | "text:h" => ("paragraph", "text:style-name"),
            "table:table" => ("table", "table:style-name"),
            _ => return None,
        };
        let name = el.attr(attr)?;
        let style = self.ix.get(Scope::Content, family, name)?;
        style
            .attr("style:master-page-name")
            .filter(|m| !m.is_empty() && self.ix.master_page(m).is_some())
            .map(str::to_owned)
    }

    fn close_section(&mut self, sb: &mut SectionBuilder) {
        if sb.blocks.is_empty() {
            return;
        }
        let (mut props, headers, footers) = self.page_props(&sb.master.clone());
        if let Some(c) = &sb.columns {
            props.columns = c.clone();
        }
        props.break_type = sb.break_type;
        let id = self.doc.alloc_id();
        sb.sections.push(Section {
            id,
            props,
            headers,
            footers,
            blocks: std::mem::take(&mut sb.blocks),
        });
        sb.break_type = SectionBreak::Continuous;
    }

    /// The body's blocks, splitting sections at master page changes and text sections.
    fn body(&mut self, el: &'a XmlElement, sb: &mut SectionBuilder, top: bool) {
        for e in el.elements() {
            match e.name.as_str() {
                "text:p" | "text:h" | "table:table" | "text:list" => {
                    let lead = if e.name == "text:list" {
                        first_leaf(e)
                    } else {
                        Some(e)
                    };
                    if let Some(m) = lead.and_then(|l| self.master_of(l)) {
                        let starts_doc = sb.sections.is_empty() && sb.blocks.is_empty();
                        if !starts_doc || m != sb.master {
                            self.close_section(sb);
                            sb.break_type = SectionBreak::NextPage;
                        }
                        sb.master = m;
                    }
                    let mut blocks = Vec::new();
                    self.block(e, Scope::Content, None, &mut blocks);
                    if !self.page_frames.is_empty()
                        && let Some(Block::Paragraph(p)) = blocks.first_mut()
                    {
                        let frames = std::mem::take(&mut self.page_frames);
                        prepend_objects(p, frames);
                    }
                    sb.blocks.extend(blocks);
                }
                "text:section" => {
                    let cols = e
                        .attr("text:style-name")
                        .and_then(|s| self.ix.get(Scope::Content, "section", s))
                        .and_then(|s| kid(s, "style:section-properties"))
                        .and_then(|p| kid(p, "style:columns"))
                        .map(read_columns);
                    let outer = sb.columns.clone();
                    if top {
                        self.close_section(sb);
                        if sb.sections.is_empty() && sb.break_type == SectionBreak::NextPage {
                            // The document starts with a section: keep the first page break.
                        } else {
                            sb.break_type = SectionBreak::Continuous;
                        }
                        sb.columns = Some(cols.unwrap_or_default());
                    }
                    self.body(e, sb, false);
                    if top {
                        self.close_section(sb);
                        sb.columns = outer;
                    }
                }
                "draw:frame" | "draw:custom-shape" | "draw:rect" | "draw:ellipse" => {
                    // Shapes anchored to the page sit directly in the body.
                    if let Some(o) = self.drawing(e, Scope::Content) {
                        self.page_frames.push(o);
                    }
                }
                "text:table-of-content"
                | "text:illustration-index"
                | "text:table-index"
                | "text:alphabetical-index"
                | "text:user-index"
                | "text:object-index"
                | "text:bibliography" => {
                    let mut blocks = Vec::new();
                    self.block(e, Scope::Content, None, &mut blocks);
                    sb.blocks.extend(blocks);
                }
                "text:sequence-decls"
                | "text:variable-decls"
                | "text:user-field-decls"
                | "text:dde-connection-decls"
                | "text:tracked-changes"
                | "office:forms"
                | "text:soft-page-break" => {}
                other => {
                    if !other.starts_with("?:") {
                        let w = format!("{other} in the body is not supported and was skipped");
                        self.warn(&w);
                    }
                }
            }
        }
    }

    fn attach_page_frames(&mut self, sb: &mut SectionBuilder, frames: Vec<InlineObject>) {
        if let Some(Block::Paragraph(p)) = sb.blocks.first_mut() {
            prepend_objects(p, frames);
            return;
        }
        let mut p = Paragraph::new(self.doc.alloc_id());
        prepend_objects(&mut p, frames);
        sb.blocks.insert(0, Block::Paragraph(p));
    }

    /// Section properties and header/footer references of a master page.
    fn page_props(&mut self, master: &str) -> (SectionProps, HeaderFooterRefs, HeaderFooterRefs) {
        let mut props = SectionProps::new(Paper::Letter);
        let Some(mp) = self.ix.master_page(master) else {
            return (props, HeaderFooterRefs::default(), HeaderFooterRefs::default());
        };
        let layout = mp
            .attr("style:page-layout-name")
            .and_then(|l| self.ix.page_layouts.get(l).copied());
        let has = |names: &[&str]| {
            names.iter().any(|n| {
                kid(mp, n).is_some_and(|h| h.attr("style:display") != Some("false"))
            })
        };
        let has_header = has(&["style:header", "style:header-left", "style:header-first", "loext:header-first"]);
        let has_footer = has(&["style:footer", "style:footer-left", "style:footer-first", "loext:footer-first"]);
        if let Some(pl) = layout.and_then(|l| kid(l, "style:page-layout-properties")) {
            let a = |n: &str| pl.attr(n).and_then(twips);
            let width = a("fo:page-width").filter(|w| *w > 0).unwrap_or(12_240);
            let height = a("fo:page-height").filter(|h| *h > 0).unwrap_or(15_840);
            props.page_size = PageSize {
                width,
                height,
                orientation: if pl.attr("style:print-orientation") == Some("landscape") {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                },
                code: None,
            };
            let mt = a("fo:margin-top").unwrap_or(0);
            let mb = a("fo:margin-bottom").unwrap_or(0);
            let hf = |name: &str, spacing: &str| -> i32 {
                layout
                    .and_then(|l| kid(l, name))
                    .and_then(|h| kid(h, "style:header-footer-properties"))
                    .map_or(0, |p| {
                        let min = p
                            .attr("fo:min-height")
                            .or_else(|| p.attr("svg:height"))
                            .and_then(twips)
                            .unwrap_or(0);
                        min + p.attr(spacing).and_then(twips).unwrap_or(0)
                    })
            };
            let (top, header) = if has_header {
                (mt + hf("style:header-style", "fo:margin-bottom"), mt)
            } else {
                (mt, mt.min(720))
            };
            let (bottom, footer) = if has_footer {
                (mb + hf("style:footer-style", "fo:margin-top"), mb)
            } else {
                (mb, mb.min(720))
            };
            props.margins = PageMargins {
                top,
                right: a("fo:margin-right").unwrap_or(0),
                bottom,
                left: a("fo:margin-left").unwrap_or(0),
                header,
                footer,
                gutter: a("loext:margin-gutter").unwrap_or(0),
            };
            if let Some(c) = kid(pl, "style:columns") {
                props.columns = read_columns(c);
            }
            if pl.attr("style:writing-mode") == Some("rl-tb") {
                props.bidi = Some(true);
            }
        }
        let (headers, footers, title) = self.master_stories(master, mp);
        props.title_page = title;
        (props, headers, footers)
    }

    fn master_stories(
        &mut self,
        name: &str,
        mp: &'a XmlElement,
    ) -> (HeaderFooterRefs, HeaderFooterRefs, bool) {
        if let Some(s) = self.page_stories.get(name) {
            return s.clone();
        }
        let mut headers = HeaderFooterRefs::default();
        let mut footers = HeaderFooterRefs::default();
        let mut title = false;
        for (el, part, kind) in [
            ("style:header", HeaderFooterPart::Header, 0),
            ("style:header-left", HeaderFooterPart::Header, 1),
            ("style:header-first", HeaderFooterPart::Header, 2),
            ("loext:header-first", HeaderFooterPart::Header, 2),
            ("style:footer", HeaderFooterPart::Footer, 0),
            ("style:footer-left", HeaderFooterPart::Footer, 1),
            ("style:footer-first", HeaderFooterPart::Footer, 2),
            ("loext:footer-first", HeaderFooterPart::Footer, 2),
        ] {
            let Some(h) = kid(mp, el).filter(|h| h.attr("style:display") != Some("false")) else {
                continue;
            };
            let refs = match part {
                HeaderFooterPart::Header => &mut headers,
                HeaderFooterPart::Footer => &mut footers,
            };
            let slot = match kind {
                0 => &mut refs.default,
                1 => &mut refs.even,
                _ => &mut refs.first,
            };
            if slot.is_some() {
                continue;
            }
            let mut blocks = Vec::new();
            for b in h.elements() {
                self.block(b, Scope::Styles, None, &mut blocks);
            }
            if blocks.is_empty() {
                blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
            }
            let id = self.doc.alloc_id();
            self.doc.headers_footers.push(HeaderFooter { id, part, blocks });
            *slot = Some(id);
            if kind == 2 {
                title = true;
            }
        }
        let out = (headers, footers, title);
        self.page_stories.insert(name.to_owned(), out.clone());
        out
    }

    // ------------------------------------------------------------ blocks

    /// Reads one block-level element (several blocks for lists and indexes).
    fn block(
        &mut self,
        e: &'a XmlElement,
        scope: Scope,
        list: Option<ListCtx>,
        out: &mut Vec<Block>,
    ) {
        match e.name.as_str() {
            "text:p" | "text:h" => {
                let p = self.paragraph(e, scope, list);
                out.push(Block::Paragraph(p));
            }
            "text:list" => self.list(e, scope, list, out),
            "table:table" => {
                if let Some(t) = self.table(e, scope) {
                    out.push(Block::Table(t));
                }
            }
            "text:section" | "text:index-body" | "text:index-title" => {
                for c in e.elements() {
                    self.block(c, scope, list, out);
                }
            }
            "text:table-of-content"
            | "text:illustration-index"
            | "text:table-index"
            | "text:alphabetical-index"
            | "text:user-index"
            | "text:object-index"
            | "text:bibliography" => {
                let instr = index_instruction(e);
                let mut blocks = Vec::new();
                if let Some(body) = kid(e, "text:index-body") {
                    for c in body.elements() {
                        self.block(c, scope, list, &mut blocks);
                    }
                }
                if blocks.is_empty() {
                    blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
                }
                let id = self.doc.alloc_id();
                out.push(Block::Field(FieldBlock {
                    id,
                    field: Field::new(&instr, ""),
                    blocks,
                }));
            }
            "text:soft-page-break" | "text:numbered-paragraph" | "text:tracked-changes" => {}
            _ => {}
        }
    }

    fn list(
        &mut self,
        e: &'a XmlElement,
        scope: Scope,
        outer: Option<ListCtx>,
        out: &mut Vec<Block>,
    ) {
        let own = e
            .attr("text:style-name")
            .and_then(|n| self.ix.list_style(scope, n))
            .map(|i| self.list_ids[i]);
        let ctx = match (outer, own) {
            (Some(o), Some(id)) => ListCtx {
                num_id: id,
                level: o.level.saturating_add(1).min(8),
            },
            (Some(o), None) => ListCtx {
                num_id: o.num_id,
                level: o.level.saturating_add(1).min(8),
            },
            (None, Some(id)) => ListCtx {
                num_id: id,
                level: 0,
            },
            (None, None) => {
                // A list without a style uses the default list style of the paragraphs; keep
                // the paragraphs, unnumbered.
                for item in e.elements() {
                    for c in item.elements() {
                        self.block(c, scope, None, out);
                    }
                }
                return;
            }
        };
        for item in e.elements() {
            match item.name.as_str() {
                "text:list-item" => {
                    for c in item.elements() {
                        match c.name.as_str() {
                            "text:list" => self.list(c, scope, Some(ctx), out),
                            _ => self.block(c, scope, Some(ctx), out),
                        }
                    }
                }
                "text:list-header" => {
                    for c in item.elements() {
                        self.block(c, scope, None, out);
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------ paragraphs

    fn paragraph_style(&self, scope: Scope, name: Option<&str>) -> (Option<String>, ParagraphProps, RunProps, Option<&'a str>) {
        let Some(name) = name else {
            return (None, ParagraphProps::default(), RunProps::default(), None);
        };
        let Some(style) = self.ix.get(scope, "paragraph", name) else {
            return (None, ParagraphProps::default(), RunProps::default(), None);
        };
        if self.ix.get_auto(scope, "paragraph", name).is_none() {
            return (Some(name.to_owned()), ParagraphProps::default(), RunProps::default(), None);
        }
        let parent = style
            .attr("style:parent-style-name")
            .filter(|p| self.ix.is_common("paragraph", p))
            .map(str::to_owned);
        let mut props = kid(style, "style:paragraph-properties")
            .map(read_para_props)
            .unwrap_or_default();
        if self.tabs_relative
            && let Some(tabs) = props.tabs.as_mut()
        {
            let left = self
                .ix
                .chain_attr(scope, "paragraph", name, "style:paragraph-properties", "fo:margin-left")
                .and_then(twips)
                .unwrap_or(0);
            for t in tabs {
                t.pos += left;
            }
        }
        let base = kid(style, "style:text-properties")
            .map(|tp| read_run_props(tp, &self.ix.fonts))
            .unwrap_or_default();
        let list = style
            .attr("style:list-style-name")
            .filter(|l| !l.is_empty())
            .and_then(|l| self.ix.list_style(scope, l))
            .map(|i| self.list_ids[i]);
        if let Some(id) = list {
            props.numbering = Some(NumberingRef {
                num_id: Some(id),
                level: Some(0),
            });
        }
        (parent, props, base, style.attr("style:master-page-name"))
    }

    fn paragraph(&mut self, e: &'a XmlElement, scope: Scope, list: Option<ListCtx>) -> Paragraph {
        let (style_id, mut props, base, _) = self.paragraph_style(scope, e.attr("text:style-name"));
        props.style_id = style_id.clone();
        if let Some(l) = list {
            props.numbering = Some(NumberingRef {
                num_id: Some(l.num_id),
                level: Some(l.level),
            });
        }
        if e.name == "text:h" {
            let level = e
                .attr("text:outline-level")
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, 9);
            let from_style = style_id
                .as_deref()
                .and_then(|s| self.ix.outline_level(Scope::Styles, s));
            if from_style != Some(level) {
                props.outline_level = Some(level - 1);
            }
        }
        let mut pb = ParaBuilder::new();
        // Ranges still open from earlier paragraphs continue at the start of this one.
        for r in &mut self.open {
            r.start = 0;
        }
        // A column break before the paragraph is an object at its start.
        if e.attr("text:style-name")
            .and_then(|n| self.ix.get(scope, "paragraph", n))
            .and_then(|s| kid(s, "style:paragraph-properties"))
            .and_then(|p| p.attr("fo:break-before"))
            == Some("column")
        {
            pb.push_object(
                InlineObject::Break(Break {
                    kind: BreakKind::Column,
                    clear: None,
                }),
                &base,
            );
        }
        self.inline(e, scope, &base, &mut pb);
        let len = pb.len;
        for r in &self.open {
            if r.start < len {
                pb.marks.push(MarkSpan::new(r.start, len, r.mark.clone()));
            }
        }
        let id = self.doc.alloc_id();
        Paragraph {
            id,
            props,
            mark_props: base,
            revision: ParagraphRevision::default(),
            content: pb.finish(),
        }
    }

    fn span_props(&self, scope: Scope, name: &str, outer: &RunProps) -> RunProps {
        let Some(style) = self.ix.get(scope, "text", name) else {
            return outer.clone();
        };
        let auto = self.ix.get_auto(scope, "text", name).is_some();
        let (style_id, own) = if auto {
            let parent = style
                .attr("style:parent-style-name")
                .filter(|p| self.ix.is_common("text", p))
                .map(str::to_owned);
            let own = kid(style, "style:text-properties")
                .map(|tp| read_run_props(tp, &self.ix.fonts))
                .unwrap_or_default();
            (parent, own)
        } else {
            (Some(name.to_owned()), RunProps::default())
        };
        let mut merged = merge_run_props(outer, Some(&own));
        if style_id.is_some() {
            merged.style_id = style_id;
        } else {
            merged.style_id.clone_from(&outer.style_id);
        }
        merged
    }

    fn inline(&mut self, el: &'a XmlElement, scope: Scope, props: &RunProps, pb: &mut ParaBuilder) {
        for node in &el.children {
            match node {
                XmlNode::Text(t) => pb.push_text(&t.text, props),
                XmlNode::Element(e) => self.inline_element(e, scope, props, pb),
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn inline_element(&mut self, e: &'a XmlElement, scope: Scope, props: &RunProps, pb: &mut ParaBuilder) {
        match e.name.as_str() {
            "text:span" => {
                let p = e
                    .attr("text:style-name")
                    .map_or_else(|| props.clone(), |n| self.span_props(scope, n, props));
                self.inline(e, scope, &p, pb);
            }
            "text:s" => {
                let n = e
                    .attr("text:c")
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 10_000);
                pb.push_literal(&" ".repeat(n), props);
            }
            "text:tab" => pb.push_literal("\t", props),
            "text:line-break" => pb.push_object(
                InlineObject::Break(Break {
                    kind: BreakKind::TextWrapping,
                    clear: None,
                }),
                props,
            ),
            "text:soft-page-break" | "text:bookmark-ref-placeholder" => {}
            "text:a" => {
                let start = pb.len;
                self.inline(e, scope, props, pb);
                let href = e.attr("xlink:href").unwrap_or_default();
                let target = match href.strip_prefix('#') {
                    Some(b) => LinkTarget::Bookmark(b.split('|').next().unwrap_or(b).to_owned()),
                    None => LinkTarget::Url(href.to_owned()),
                };
                if pb.len > start && !href.is_empty() {
                    pb.marks.push(MarkSpan::new(
                        start,
                        pb.len,
                        Mark::Link(Hyperlink {
                            target,
                            tooltip: e.attr("office:title").map(str::to_owned),
                            frame: e
                                .attr("office:target-frame-name")
                                .filter(|f| *f != "_self")
                                .map(str::to_owned),
                            history: true,
                        }),
                    ));
                }
            }
            "text:bookmark" => {
                if let Some(n) = e.attr("text:name") {
                    pb.push_object(InlineObject::BookmarkStart { name: n.to_owned() }, props);
                    pb.push_object(InlineObject::BookmarkEnd { name: n.to_owned() }, props);
                }
            }
            "text:bookmark-start" => {
                if let Some(n) = e.attr("text:name") {
                    pb.push_object(InlineObject::BookmarkStart { name: n.to_owned() }, props);
                }
            }
            "text:bookmark-end" => {
                if let Some(n) = e.attr("text:name") {
                    pb.push_object(InlineObject::BookmarkEnd { name: n.to_owned() }, props);
                }
            }
            "text:note" => self.note(e, props, pb),
            "office:annotation" => self.annotation(e, pb),
            "office:annotation-end" => {
                if let Some(n) = e.attr("office:name") {
                    self.close_range(n, pb);
                }
            }
            "text:change-start" => {
                if let Some(id) = e.attr("text:change-id") {
                    let mark = self.change_mark(id);
                    if let Some(mark) = mark {
                        self.open.push(OpenRange {
                            key: id.to_owned(),
                            mark,
                            start: pb.len,
                        });
                    }
                }
            }
            "text:change-end" => {
                if let Some(id) = e.attr("text:change-id") {
                    self.close_range(id, pb);
                }
            }
            "text:change" => {
                if let Some(id) = e.attr("text:change-id") {
                    self.deletion(id, scope, props, pb);
                }
            }
            "draw:frame" | "draw:custom-shape" | "draw:rect" | "draw:ellipse" | "draw:line"
            | "draw:g" | "draw:polygon" | "draw:polyline" | "draw:path" | "draw:connector" => {
                if let Some(o) = self.drawing(e, scope) {
                    pb.push_object(o, props);
                }
            }
            "text:ruby" => {
                if let Some(base) = kid(e, "text:ruby-base") {
                    self.inline(base, scope, props, pb);
                }
            }
            "text:alphabetical-index-mark"
            | "text:alphabetical-index-mark-start"
            | "text:alphabetical-index-mark-end"
            | "text:toc-mark"
            | "text:toc-mark-start"
            | "text:toc-mark-end"
            | "text:user-index-mark"
            | "text:user-index-mark-start"
            | "text:user-index-mark-end"
            | "text:reference-mark"
            | "text:reference-mark-start"
            | "text:reference-mark-end"
            | "text:bibliography-mark-start"
            | "office:annotation-end-marker"
            | "draw:a" => {
                if e.name == "draw:a" {
                    for c in e.elements() {
                        self.inline_element(c, scope, props, pb);
                    }
                }
            }
            "text:hidden-text" | "text:hidden-paragraph" | "text:variable-decls" => {}
            name => {
                if let Some(field) = field_of(e, &self.ix) {
                    pb.push_object(InlineObject::Field(field), props);
                } else {
                    // text:meta, text:meta-field, text:variable-get, text:placeholder, …:
                    // keep the text.
                    if !matches!(name, "text:meta" | "text:meta-field" | "field:fieldmark-start" | "field:fieldmark-end" | "field:fieldmark") {
                        let w = format!("{name} is kept as plain text");
                        self.warn(&w);
                    }
                    self.inline(e, scope, props, pb);
                }
            }
        }
    }

    fn close_range(&mut self, key: &str, pb: &mut ParaBuilder) {
        if let Some(i) = self.open.iter().position(|r| r.key == key) {
            let r = self.open.remove(i);
            if pb.len > r.start {
                pb.marks.push(MarkSpan::new(r.start, pb.len, r.mark));
            }
        }
    }

    fn revision(&mut self, region: &XmlElement) -> RevisionInfo {
        let info = region
            .elements()
            .find_map(|c| kid(c, "office:change-info"));
        self.revision_id += 1;
        RevisionInfo {
            id: self.revision_id,
            author: info
                .and_then(|i| kid(i, "dc:creator"))
                .map(|c| all_text(c).trim().to_owned())
                .unwrap_or_default(),
            date: info
                .and_then(|i| kid(i, "dc:date"))
                .map(|c| all_text(c).trim().to_owned())
                .filter(|d| !d.is_empty()),
        }
    }

    fn change_mark(&mut self, id: &str) -> Option<Mark> {
        let region = *self.regions.get(id)?;
        let kind = region.first_element()?.name.clone();
        let info = self.revision(region);
        match kind.as_str() {
            "text:insertion" => Some(Mark::Insertion(info)),
            "text:format-change" => Some(Mark::FormatChange {
                info,
                before: Box::default(),
            }),
            "text:deletion" => Some(Mark::Deletion(info)),
            _ => None,
        }
    }

    /// A deletion point: the deleted text comes back with a deletion mark.
    fn deletion(&mut self, id: &str, scope: Scope, props: &RunProps, pb: &mut ParaBuilder) {
        let Some(region) = self.regions.get(id).copied() else {
            return;
        };
        let Some(del) = kid(region, "text:deletion") else {
            return;
        };
        let info = self.revision(region);
        let start = pb.len;
        let saved = pb.skip_space;
        pb.skip_space = false;
        let mut first = true;
        for p in del.elements().filter(|c| c.name == "text:p" || c.name == "text:h") {
            if !first {
                pb.push_literal(" ", props);
            }
            first = false;
            self.inline(p, scope, props, pb);
        }
        if pb.len > start {
            pb.marks.push(MarkSpan::new(start, pb.len, Mark::Deletion(info)));
        } else {
            pb.skip_space = saved;
        }
    }

    fn annotation(&mut self, e: &'a XmlElement, pb: &mut ParaBuilder) {
        let text_el = |n: &str| kid(e, n).map(|c| all_text(c).trim().to_owned());
        let mut blocks = Vec::new();
        for c in e.elements() {
            if matches!(c.name.as_str(), "text:p" | "text:h" | "text:list" | "table:table") {
                self.block(c, Scope::Content, None, &mut blocks);
            }
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
        }
        let parent = e
            .attr("loext:parent-name")
            .or_else(|| e.attr("office:parent-name"))
            .and_then(|p| self.comment_ids.get(p).copied());
        let id = self.doc.alloc_id();
        self.doc.comments.push(Comment {
            id,
            meta: CommentMeta {
                author: text_el("dc:creator").unwrap_or_default(),
                initials: text_el("meta:creator-initials").filter(|i| !i.is_empty()),
                date: text_el("dc:date").filter(|d| !d.is_empty()),
                parent,
                resolved: e.attr("loext:resolved") == Some("true")
                    || e.attr("office:resolved") == Some("true"),
            },
            blocks,
        });
        if let Some(name) = e.attr("office:name") {
            self.comment_ids.insert(name.to_owned(), id);
            self.open.push(OpenRange {
                key: name.to_owned(),
                mark: Mark::Comment(id),
                start: pb.len,
            });
        }
    }

    fn note(&mut self, e: &'a XmlElement, props: &RunProps, pb: &mut ParaBuilder) {
        let kind = if e.attr("text:note-class") == Some("endnote") {
            NoteKind::Endnote
        } else {
            NoteKind::Footnote
        };
        let custom = kid(e, "text:note-citation")
            .and_then(|c| c.attr("text:label"))
            .map(str::to_owned);
        let mut blocks = Vec::new();
        if let Some(body) = kid(e, "text:note-body") {
            // Ranges open around the note do not extend into it.
            let saved = std::mem::take(&mut self.open);
            for c in body.elements() {
                self.block(c, Scope::Content, None, &mut blocks);
            }
            self.open = saved;
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
        }
        let id = self.doc.alloc_id();
        self.doc.notes.push(Note {
            id,
            kind,
            note_type: NoteType::Normal,
            blocks,
        });
        pb.push_object(
            InlineObject::NoteRef(NoteRef {
                note: id,
                kind,
                custom_mark: custom,
            }),
            props,
        );
    }

    // ------------------------------------------------------------ drawings

    fn media_key(&mut self, href: &str) -> Option<String> {
        let path = href.trim_start_matches("./").to_owned();
        if let Some(k) = self.media.get(&path) {
            return Some(k.clone());
        }
        let bytes = self.pkg.file(&path)?;
        let base = path.rsplit('/').next().unwrap_or(&path);
        let mut key = format!("/word/media/{base}");
        let mut n = 1;
        while self.doc.media.contains_key(&key) {
            n += 1;
            key = format!("/word/media/{n}-{base}");
        }
        self.doc.media.insert(
            key.clone(),
            Media {
                content_type: self.pkg.media_type_of(&path),
                data: bytes.to_vec(),
            },
        );
        self.media.insert(path, key.clone());
        Some(key)
    }

    #[allow(clippy::too_many_lines)]
    fn drawing(&mut self, e: &'a XmlElement, scope: Scope) -> Option<InlineObject> {
        let width = e.attr("svg:width").and_then(emu).unwrap_or(0).max(0);
        let height = e.attr("svg:height").and_then(emu).unwrap_or(0).max(0);
        let (kind, text_box) = match e.name.as_str() {
            "draw:frame" => {
                if let Some(img) = kid(e, "draw:image") {
                    match img.attr("xlink:href").and_then(|h| self.media_key(h)) {
                        Some(media) => (DrawingKind::Picture { media, crop: None }, None),
                        None => {
                            self.warn("a picture without its image data was skipped");
                            return None;
                        }
                    }
                } else if let Some(tb) = kid(e, "draw:text-box") {
                    let mut blocks = Vec::new();
                    let saved = std::mem::take(&mut self.open);
                    for c in tb.elements() {
                        self.block(c, scope, None, &mut blocks);
                    }
                    self.open = saved;
                    if blocks.is_empty() {
                        blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
                    }
                    (
                        DrawingKind::Shape {
                            preset: "rect".to_owned(),
                        },
                        Some(blocks),
                    )
                } else if kid(e, "draw:object").is_some() || kid(e, "draw:object-ole").is_some() {
                    // Charts and OLE objects: their replacement picture, if any.
                    let fallback = kids(e, "draw:image")
                        .next()
                        .and_then(|i| i.attr("xlink:href"))
                        .and_then(|h| self.media_key(h));
                    self.warn("embedded objects (charts, OLE) in text documents are imported as their replacement picture");
                    match fallback {
                        Some(media) => (DrawingKind::Picture { media, crop: None }, None),
                        None => return None,
                    }
                } else {
                    return None;
                }
            }
            "draw:custom-shape" | "draw:rect" | "draw:ellipse" => {
                let preset = match e.name.as_str() {
                    "draw:rect" => "rect".to_owned(),
                    "draw:ellipse" => "ellipse".to_owned(),
                    _ => kid(e, "draw:enhanced-geometry")
                        .and_then(|g| g.attr("draw:type"))
                        .map_or_else(|| "rect".to_owned(), crate::deck::shapes::preset_of_odf),
                };
                let paras: Vec<&'a XmlElement> = e
                    .elements()
                    .filter(|c| matches!(c.name.as_str(), "text:p" | "text:h" | "text:list"))
                    .collect();
                let text_box = if paras.is_empty() {
                    None
                } else {
                    let mut blocks = Vec::new();
                    let saved = std::mem::take(&mut self.open);
                    for c in paras {
                        self.block(c, scope, None, &mut blocks);
                    }
                    self.open = saved;
                    Some(blocks)
                };
                (DrawingKind::Shape { preset }, text_box)
            }
            other => {
                let w = format!("{other} drawings in text documents are not imported");
                self.warn(&w);
                return None;
            }
        };
        let anchor = e.attr("text:anchor-type").unwrap_or("paragraph");
        let style = e
            .attr("draw:style-name")
            .and_then(|s| self.ix.get(scope, "graphic", s))
            .and_then(|s| kid(s, "style:graphic-properties"));
        let placement = if anchor == "as-char" {
            Placement::Inline
        } else {
            Placement::Anchor(Box::new(anchor_of(e, style, anchor)))
        };
        let rotation = e
            .attr("draw:transform")
            .and_then(rotation_of)
            .unwrap_or(0);
        let id = self.doc.alloc_id();
        Some(InlineObject::Drawing(Box::new(Drawing {
            id,
            kind,
            width,
            height,
            placement,
            name: e.attr("draw:name").unwrap_or_default().to_owned(),
            description: kid(e, "svg:desc").map(all_text).filter(|t| !t.is_empty()),
            title: kid(e, "svg:title").map(all_text).filter(|t| !t.is_empty()),
            decorative: e.attr("loext:decorative") == Some("true"),
            rotation,
            flip_h: false,
            flip_v: false,
            text_box,
            xml: None,
        })))
    }

    // ------------------------------------------------------------ tables

    #[allow(clippy::too_many_lines)]
    fn table(&mut self, e: &'a XmlElement, scope: Scope) -> Option<Table> {
        let tstyle = e
            .attr("table:style-name")
            .and_then(|s| self.ix.get(scope, "table", s))
            .and_then(|s| kid(s, "style:table-properties"));
        let mut props = TableProps::default();
        if let Some(tp) = tstyle {
            if let Some(w) = tp.attr("style:width").and_then(twips) {
                props.width = Some(TableWidth {
                    kind: WidthType::Dxa,
                    value: w,
                });
            }
            if let Some(rel) = tp.attr("style:rel-width").and_then(crate::units::percent) {
                let v = (rel * 50.0).round().clamp(0.0, 5000.0);
                // In range by the clamp.
                #[allow(clippy::cast_possible_truncation)]
                let v = v as i32;
                props.width = Some(TableWidth {
                    kind: WidthType::Pct,
                    value: v,
                });
            }
            props.alignment = match tp.attr("table:align") {
                Some("center") => Some(TableAlignment::Center),
                Some("right") => Some(TableAlignment::Right),
                Some("left") => Some(TableAlignment::Left),
                _ => None,
            };
            if props.alignment == Some(TableAlignment::Left) || tp.attr("table:align") == Some("margins") {
                props.indent = tp.attr("fo:margin-left").and_then(twips).filter(|m| *m != 0);
            }
            if let Some(c) = tp.attr("fo:background-color").and_then(color) {
                props.shading = Some(Shading::fill(kiln_doc::props::DocColor {
                    val: kiln_doc::props::ColorValue::Rgb(c),
                    ..Default::default()
                }));
            }
            if tp.attr("style:writing-mode") == Some("rl-tb") {
                props.bidi_visual = Some(true);
            }
        }
        // Column widths.
        let mut grid: Vec<i32> = Vec::new();
        let mut rows_src: Vec<(&'a XmlElement, bool)> = Vec::new();
        collect_table(e, &mut grid, &mut rows_src, false, self, scope);
        if rows_src.is_empty() {
            return None;
        }
        // (remaining rows, column span) of vertical merges by grid column.
        let mut vmerge: BTreeMap<usize, (usize, u32)> = BTreeMap::new();
        let mut rows = Vec::new();
        let mut max_cols = 0usize;
        for (row_el, header) in rows_src {
            let repeat = row_el
                .attr("table:number-rows-repeated")
                .and_then(|r| r.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 1000);
            for _ in 0..repeat {
                let mut rp = TableRowProps::default();
                if header {
                    rp.header = Some(true);
                }
                if let Some(rs) = row_el
                    .attr("table:style-name")
                    .and_then(|s| self.ix.get(scope, "table-row", s))
                    .and_then(|s| kid(s, "style:table-row-properties"))
                {
                    if let Some(h) = rs.attr("style:row-height").and_then(twips) {
                        rp.height = Some(RowHeight {
                            value: h,
                            rule: HeightRule::Exact,
                        });
                    } else if let Some(h) = rs.attr("style:min-row-height").and_then(twips) {
                        rp.height = Some(RowHeight {
                            value: h,
                            rule: HeightRule::AtLeast,
                        });
                    }
                    if rs.attr("fo:keep-together") == Some("always") {
                        rp.cant_split = Some(true);
                    }
                }
                let mut cells = Vec::new();
                let mut col = 0usize;
                let mut skip = 0usize;
                for c in row_el.elements() {
                    let n = c
                        .attr("table:number-columns-repeated")
                        .and_then(|r| r.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 1024);
                    for _ in 0..n {
                        match c.name.as_str() {
                            "table:table-cell" => {
                                skip = 0;
                                let span = c
                                    .attr("table:number-columns-spanned")
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .unwrap_or(1)
                                    .clamp(1, 1024);
                                let rspan = c
                                    .attr("table:number-rows-spanned")
                                    .and_then(|s| s.parse::<usize>().ok())
                                    .unwrap_or(1)
                                    .clamp(1, 100_000);
                                let mut cell = self.cell(c, scope);
                                cell.grid_span = span;
                                if rspan > 1 {
                                    cell.v_merge = Some(VMerge::Restart);
                                    vmerge.insert(col, (rspan - 1, span));
                                }
                                cells.push(cell);
                                col += span as usize;
                                skip = span as usize - 1;
                            }
                            "table:covered-table-cell" => {
                                if skip > 0 {
                                    skip -= 1;
                                    continue;
                                }
                                if let Some((left, span)) = vmerge.get_mut(&col)
                                    && *left > 0
                                {
                                    *left -= 1;
                                    let span = *span;
                                    let mut cell = self.cell(c, scope);
                                    cell.grid_span = span;
                                    cell.v_merge = Some(VMerge::Continue);
                                    cells.push(cell);
                                    col += span as usize;
                                    skip = span as usize - 1;
                                } else {
                                    // A covered cell outside any span: keep the grid intact.
                                    let cell = self.cell(c, scope);
                                    cells.push(cell);
                                    col += 1;
                                }
                            }
                            _ => break,
                        }
                    }
                }
                vmerge.retain(|_, (left, _)| *left > 0);
                if cells.is_empty() {
                    let blocks = vec![Block::Paragraph(Paragraph::new(self.doc.alloc_id()))];
                    cells.push(Cell {
                        id: self.doc.alloc_id(),
                        props: TableCellProps::default(),
                        grid_span: 1,
                        v_merge: None,
                        blocks,
                    });
                    col = 1;
                }
                max_cols = max_cols.max(col);
                rows.push(Row {
                    id: self.doc.alloc_id(),
                    props: rp,
                    revision: None,
                    cells,
                });
                if rows.len() > 100_000 {
                    break;
                }
            }
        }
        if grid.len() > max_cols {
            grid.truncate(max_cols);
        }
        Some(Table {
            id: self.doc.alloc_id(),
            props,
            grid,
            rows,
        })
    }

    fn cell(&mut self, c: &'a XmlElement, scope: Scope) -> Cell {
        let mut props = TableCellProps::default();
        if let Some(cp) = c
            .attr("table:style-name")
            .and_then(|s| self.ix.get(scope, "table-cell", s))
            .and_then(|s| kid(s, "style:table-cell-properties"))
        {
            let a = |n: &str| cp.attr(n);
            let side = |n: &str| a(n).or_else(|| a("fo:border")).and_then(parse_border);
            let borders = CellBorders {
                top: side("fo:border-top"),
                left: side("fo:border-left"),
                bottom: side("fo:border-bottom"),
                right: side("fo:border-right"),
                ..CellBorders::default()
            };
            if borders != CellBorders::default() {
                props.borders = Some(borders);
            }
            let pad = |n: &str| a(n).or_else(|| a("fo:padding")).and_then(twips);
            let margins = CellMargins {
                top: pad("fo:padding-top"),
                left: pad("fo:padding-left"),
                bottom: pad("fo:padding-bottom"),
                right: pad("fo:padding-right"),
            };
            if margins != CellMargins::default() {
                props.margins = Some(margins);
            }
            if let Some(col) = a("fo:background-color").and_then(color) {
                props.shading = Some(Shading {
                    pattern: ShadingPattern::Clear,
                    color: None,
                    fill: Some(kiln_doc::props::DocColor {
                        val: kiln_doc::props::ColorValue::Rgb(col),
                        ..Default::default()
                    }),
                });
            }
            props.v_align = match a("style:vertical-align") {
                Some("middle") => Some(CellVAlign::Center),
                Some("bottom") => Some(CellVAlign::Bottom),
                Some("top") => Some(CellVAlign::Top),
                _ => None,
            };
        }
        let mut blocks = Vec::new();
        for b in c.elements() {
            self.block(b, scope, None, &mut blocks);
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Paragraph::new(self.doc.alloc_id())));
        }
        Cell {
            id: self.doc.alloc_id(),
            props,
            grid_span: 1,
            v_merge: None,
            blocks,
        }
    }
}

struct SectionBuilder {
    sections: Vec<Section>,
    blocks: Vec<Block>,
    master: String,
    columns: Option<Columns>,
    break_type: SectionBreak,
}

fn first_leaf(e: &XmlElement) -> Option<&XmlElement> {
    for c in e.elements() {
        match c.name.as_str() {
            "text:p" | "text:h" => return Some(c),
            "text:list" | "text:list-item" | "text:list-header" => {
                if let Some(l) = first_leaf(c) {
                    return Some(l);
                }
            }
            _ => {}
        }
    }
    None
}

fn prepend_objects(p: &mut Paragraph, objects: Vec<InlineObject>) {
    let mut fragment = Fragment::default();
    for o in objects {
        fragment.text.push(kiln_doc::OBJECT_CHAR);
        fragment.objects.push(o);
    }
    let n = fragment.objects.len();
    if n == 0 {
        return;
    }
    fragment.runs.push(RunSpan::new(n, RunProps::default()));
    let _ = p.content.insert(0, fragment);
}

fn collect_table<'a>(
    e: &'a XmlElement,
    grid: &mut Vec<i32>,
    rows: &mut Vec<(&'a XmlElement, bool)>,
    header: bool,
    r: &Reader<'a>,
    scope: Scope,
) {
    for c in e.elements() {
        match c.name.as_str() {
            "table:table-column" => {
                let n = c
                    .attr("table:number-columns-repeated")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 1024);
                let w = c
                    .attr("table:style-name")
                    .and_then(|s| r.ix.get(scope, "table-column", s))
                    .and_then(|s| kid(s, "style:table-column-properties"))
                    .and_then(|p| p.attr("style:column-width"))
                    .and_then(twips)
                    .unwrap_or(1440);
                for _ in 0..n {
                    if grid.len() < 1024 {
                        grid.push(w);
                    }
                }
            }
            "table:table-columns" | "table:table-header-columns" | "table:table-column-group" => {
                collect_table(c, grid, rows, header, r, scope);
            }
            "table:table-header-rows" => collect_table(c, grid, rows, true, r, scope),
            "table:table-rows" | "table:table-row-group" => {
                collect_table(c, grid, rows, header, r, scope);
            }
            "table:table-row" => rows.push((c, header)),
            _ => {}
        }
    }
}

fn read_columns(c: &XmlElement) -> Columns {
    let count = c
        .attr("fo:column-count")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, 45);
    let space = c.attr("fo:column-gap").and_then(twips).unwrap_or(720);
    let rel: Vec<f64> = kids(c, "style:column")
        .filter_map(|k| {
            k.attr("style:rel-width")
                .and_then(|w| w.trim_end_matches('*').parse::<f64>().ok())
        })
        .collect();
    let equal = rel.len() < 2
        || rel
            .iter()
            .all(|w| (w - rel[0]).abs() <= rel[0].abs() * 0.001 + 1.0);
    let separator = kid(c, "style:column-sep")
        .is_some_and(|s| s.attr("style:style").unwrap_or("solid") != "none");
    let cols = if equal {
        Vec::new()
    } else {
        kids(c, "style:column")
            .map(|k| {
                let start = k.attr("fo:start-indent").and_then(twips).unwrap_or(0);
                let end = k.attr("fo:end-indent").and_then(twips).unwrap_or(0);
                Column {
                    width: 0,
                    space: start + end,
                }
            })
            .collect()
    };
    Columns {
        count,
        space,
        equal_width: equal,
        separator,
        cols,
    }
}

fn anchor_of(e: &XmlElement, style: Option<&XmlElement>, anchor: &str) -> Anchor {
    let g = |n: &str| style.and_then(|s| s.attr(n));
    let (wrap, wrap_side) = match g("style:wrap") {
        Some("none") => (WrapType::TopAndBottom, None),
        Some("left") => (WrapType::Square, Some(WrapSide::Left)),
        Some("right") => (WrapType::Square, Some(WrapSide::Right)),
        Some("biggest" | "dynamic") => (WrapType::Square, Some(WrapSide::Largest)),
        Some("parallel") => (
            if g("style:wrap-contour") == Some("true") {
                WrapType::Tight
            } else {
                WrapType::Square
            },
            Some(WrapSide::BothSides),
        ),
        Some("run-through") => (
            if g("style:run-through") == Some("background") {
                WrapType::Behind
            } else {
                WrapType::InFront
            },
            None,
        ),
        _ => (WrapType::Square, Some(WrapSide::BothSides)),
    };
    let relative_h = match g("style:horizontal-rel") {
        Some("page") => RelativeFromH::Page,
        Some("page-content") => RelativeFromH::Margin,
        Some("page-start-margin" | "paragraph-start-margin") => RelativeFromH::LeftMargin,
        Some("page-end-margin" | "paragraph-end-margin") => RelativeFromH::RightMargin,
        Some("char") => RelativeFromH::Character,
        _ if anchor == "page" => RelativeFromH::Page,
        _ => RelativeFromH::Column,
    };
    let x = e.attr("svg:x").and_then(emu).unwrap_or(0);
    let y = e.attr("svg:y").and_then(emu).unwrap_or(0);
    let h = match g("style:horizontal-pos") {
        Some("left") => PosH::Align(AlignH::Left),
        Some("center") => PosH::Align(AlignH::Center),
        Some("right") => PosH::Align(AlignH::Right),
        Some("inside") => PosH::Align(AlignH::Inside),
        Some("outside") => PosH::Align(AlignH::Outside),
        _ => PosH::Offset(x),
    };
    let relative_v = match g("style:vertical-rel") {
        Some("page") => RelativeFromV::Page,
        Some("page-content") => RelativeFromV::Margin,
        Some("line" | "baseline" | "text" | "char") => RelativeFromV::Line,
        _ if anchor == "page" => RelativeFromV::Page,
        _ => RelativeFromV::Paragraph,
    };
    let v = match g("style:vertical-pos") {
        Some("top") => PosV::Align(AlignV::Top),
        Some("middle") => PosV::Align(AlignV::Center),
        Some("bottom") => PosV::Align(AlignV::Bottom),
        _ => PosV::Offset(y),
    };
    let m = |n: &str| g(n).and_then(emu).unwrap_or(0).max(0);
    Anchor {
        wrap,
        wrap_side,
        relative_h,
        h,
        relative_v,
        v,
        dist_top: m("fo:margin-top"),
        dist_bottom: m("fo:margin-bottom"),
        dist_left: m("fo:margin-left"),
        dist_right: m("fo:margin-right"),
        allow_overlap: true,
        layout_in_cell: true,
        locked: false,
        z_order: e
            .attr("draw:z-index")
            .and_then(|z| z.parse::<i64>().ok())
            .unwrap_or(0),
        wrap_polygon_xml: None,
    }
}

/// The rotation in 60 000ths of a degree from `draw:transform="rotate (r) translate (…)"`
/// (radians, counter-clockwise in ODF; clockwise in the model).
fn rotation_of(t: &str) -> Option<i32> {
    let i = t.find("rotate")?;
    let rest = &t[i + 6..];
    let open = rest.find('(')?;
    let close = rest.find(')')?;
    let inner = rest.get(open + 1..close)?.trim();
    let rad: f64 = inner.parse().ok().or_else(|| degrees(inner).map(f64::to_radians))?;
    let deg = (-rad.to_degrees()).rem_euclid(360.0);
    let v = (deg * 60_000.0).round();
    if v.is_finite() {
        // Below 21.6 million by the rem_euclid above.
        #[allow(clippy::cast_possible_truncation)]
        Some(v as i32)
    } else {
        None
    }
}

/// The Word instruction an index stands for.
fn index_instruction(e: &XmlElement) -> String {
    match e.name.as_str() {
        "text:table-of-content" => {
            let levels = e
                .elements()
                .find(|c| c.name == "text:table-of-content-source")
                .and_then(|s| s.attr("text:outline-level"))
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(3)
                .clamp(1, 9);
            format!("TOC \\o \"1-{levels}\" \\h \\z \\u")
        }
        "text:illustration-index" | "text:table-index" => {
            let caption = e
                .elements()
                .find(|c| c.name.ends_with("-source"))
                .and_then(|s| s.attr("text:caption-sequence-name"))
                .unwrap_or(if e.name == "text:table-index" {
                    "Table"
                } else {
                    "Figure"
                });
            format!("TOC \\h \\z \\c \"{caption}\"")
        }
        "text:alphabetical-index" => "INDEX \\e \"\t\" \\c \"2\"".to_owned(),
        "text:bibliography" => "BIBLIOGRAPHY".to_owned(),
        _ => "TOC \\h \\z".to_owned(),
    }
}

/// The field an ODF field element stands for, with its displayed text as the result.
fn field_of(e: &XmlElement, ix: &StyleIndex<'_>) -> Option<Field> {
    let result = all_text(e);
    let instr = match e.name.as_str() {
        "text:page-number" => match e.attr("text:select-page") {
            Some("previous" | "next") => return None,
            _ => "PAGE".to_owned(),
        },
        "text:page-count" => "NUMPAGES".to_owned(),
        "text:date" | "text:time" => {
            let kw = if e.name == "text:date" { "DATE" } else { "TIME" };
            match e
                .attr("style:data-style-name")
                .and_then(|n| ix.data_styles.get(n))
                .and_then(|d| crate::numfmt::read_data_style(d))
                .map(|code| crate::numfmt::excel_to_word_picture(&code))
            {
                Some(pic) if !pic.is_empty() => format!("{kw} \\@ \"{pic}\""),
                _ => kw.to_owned(),
            }
        }
        "text:title" => "TITLE".to_owned(),
        "text:subject" => "SUBJECT".to_owned(),
        "text:initial-creator" | "text:author-name" => "AUTHOR".to_owned(),
        "text:creator" => "LASTSAVEDBY".to_owned(),
        "text:keywords" => "KEYWORDS".to_owned(),
        "text:description" => "COMMENTS".to_owned(),
        "text:file-name" => "FILENAME".to_owned(),
        "text:template-name" => "TEMPLATE".to_owned(),
        "text:word-count" => "NUMWORDS".to_owned(),
        "text:character-count" => "NUMCHARS".to_owned(),
        "text:creation-date" => "CREATEDATE".to_owned(),
        "text:modification-date" => "SAVEDATE".to_owned(),
        "text:print-date" => "PRINTDATE".to_owned(),
        "text:editing-cycles" => "REVNUM".to_owned(),
        "text:user-defined" => format!("DOCPROPERTY \"{}\"", e.attr("text:name")?),
        "text:bookmark-ref" | "text:reference-ref" => {
            let name = e.attr("text:ref-name")?;
            match e.attr("text:reference-format") {
                Some("page") => format!("PAGEREF {name} \\h"),
                _ => format!("REF {name} \\h"),
            }
        }
        "text:sequence" => format!("SEQ {} \\* ARABIC", e.attr("text:name")?),
        _ => return None,
    };
    let mut f = Field::new(&instr, &result);
    f.locked = e.attr("text:fixed") == Some("true");
    Some(f)
}

