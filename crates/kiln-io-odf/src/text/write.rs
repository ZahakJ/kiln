// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`kiln_doc::Document`] → `.odt`.
//!
//! The inverse of [`super::read`]: named paragraph and character styles go to `styles.xml`
//! (with the document defaults and the default paragraph style's formatting as the default
//! style), every list instance becomes a list style `L<n>`, direct formatting becomes automatic
//! styles shared by equal formatting, sections become master pages (new page) or text sections
//! (continuous, with their columns), headers and footers go to the master pages, comments,
//! tracked changes, notes, fields, bookmarks, hyperlinks, pictures, text boxes, shapes and
//! tables are written as their ODF elements. Tab stops are written as Word measures them
//! (from the page margin) and `settings.xml` says so (`TabsRelativeToIndent` false).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_common::NodeId;
use kiln_doc::fields::parse_field_instruction;
use kiln_doc::inline::{Hyperlink, InlineObject, LinkTarget, Mark};
use kiln_doc::model::{
    Block, Comment, Document, Drawing, DrawingKind, FieldBlock, HeaderFooterPart, Paragraph,
    Placement, PosH, PosV, Table,
};
use kiln_doc::props::{ParagraphProps, RunProps, TableCellProps, TableProps};
use kiln_doc::section::{Columns, HeaderFooterRefs, SectionProps, resolve_header_footer};
use kiln_doc::st::{
    AlignH, AlignV, BreakKind, CellVAlign, HeaderFooterKind, HeightRule, NoteKind, Orientation,
    RelativeFromH, RelativeFromV, SectionBreak, StyleType, TableAlignment, VMerge, WidthType,
    WrapSide, WrapType,
};
use kiln_doc::style::Style;
use kiln_doc::theme::DocTheme;
use kiln_doc::{OBJECT_CHAR, merge::merge_run_props};

use super::lists::list_style_el;
use crate::el::El;
use crate::package::{OdfKind, PackageWriter, extension_for};
use crate::props::{border_str, para_props_el, rgb_of, run_props_el, shading_rgb};
use crate::units::{color_str, emu_str, twips_str};
use crate::xml::XmlOut;
use crate::{GENERATOR, OdfError, WriteOptions, meta};

/// Every prefix the parts declare.
pub(crate) const PREFIXES: &[&str] = &[
    "office", "style", "text", "table", "draw", "fo", "xlink", "dc", "meta", "number", "svg",
    "chart", "dr3d", "math", "form", "script", "ooo", "ooow", "oooc", "of", "xhtml", "loext",
    "officeooo", "field", "presentation", "smil", "anim", "calcext", "tableooo", "drawooo",
];

/// Writes a document as `.odt`.
///
/// # Errors
///
/// [`OdfError::Write`] when the archive cannot be written.
pub fn write_odt(doc: &Document, options: &WriteOptions) -> Result<Vec<u8>, OdfError> {
    let mut w = Writer::new(doc);
    let content = w.content();
    let styles = w.styles();
    let mut pkg = PackageWriter::new(OdfKind::Text);
    pkg.set_template(options.template);
    pkg.add("content.xml", "text/xml", content);
    pkg.add("styles.xml", "text/xml", styles);
    pkg.add("meta.xml", "text/xml", meta::write_meta(&doc.properties, GENERATOR));
    pkg.add("settings.xml", "text/xml", settings_xml());
    for (key, path) in &w.pictures {
        if let Some(m) = doc.media.get(key) {
            pkg.add(path, &m.content_type, m.data.clone());
        }
    }
    pkg.finish()
}

fn settings_xml() -> Vec<u8> {
    let mut w = XmlOut::new();
    w.root("office:document-settings", &["office", "config", "ooo"])
        .attr("office:version", "1.3");
    w.start("office:settings");
    w.start("config:config-item-set")
        .attr("config:name", "ooo:configuration-settings");
    for (name, value) in [
        ("TabsRelativeToIndent", "false"),
        ("AddParaTableSpacing", "false"),
    ] {
        w.start("config:config-item")
            .attr("config:name", name)
            .attr("config:type", "boolean")
            .text(value)
            .end();
    }
    w.finish()
}

/// Encodes a style id as an ODF style name (an NCName), as LibreOffice does (`_20_` for a space).
#[must_use]
pub fn ncname(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for (i, c) in id.chars().enumerate() {
        let ok = c.is_alphanumeric() || c == '_' || (i > 0 && (c == '-' || c == '.'));
        if ok && !(i == 0 && c.is_numeric()) {
            out.push(c);
        } else {
            out.push_str(&format!("_{:x}_", u32::from(c)));
        }
    }
    if out.is_empty() {
        out.push_str("_5f_");
    }
    out
}

/// Automatic styles of one part, shared by equal definitions.
#[derive(Debug, Default)]
struct AutoStyles {
    prefix: &'static str,
    list: Vec<El>,
    index: HashMap<El, String>,
    counters: HashMap<String, usize>,
}

impl AutoStyles {
    fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            ..Self::default()
        }
    }

    /// Registers an element (without its `style:name`) and returns its name.
    fn add(&mut self, kind: &str, el: El) -> String {
        if let Some(n) = self.index.get(&el) {
            return n.clone();
        }
        let c = self.counters.entry(kind.to_owned()).or_insert(0);
        *c += 1;
        let name = format!("{}{kind}{c}", self.prefix);
        let mut named = el.clone();
        named.attrs.insert(0, ("style:name".to_owned(), name.clone()));
        self.list.push(named);
        self.index.insert(el, name.clone());
        name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Content,
    Styles,
}

/// The first and last paragraph a comment's range touches.
#[derive(Debug, Clone, Copy)]
struct CommentSpan {
    first: NodeId,
    last: NodeId,
}

struct Writer<'d> {
    doc: &'d Document,
    theme: &'d DocTheme,
    content_autos: AutoStyles,
    styles_autos: AutoStyles,
    part: Part,
    pictures: BTreeMap<String, String>,
    comment_spans: HashMap<NodeId, CommentSpan>,
    comments_written: HashSet<NodeId>,
    changes: Vec<String>,
    lists_seen: HashSet<u32>,
    note_counters: [usize; 2],
    counters: HashMap<&'static str, usize>,
    master_pages: Vec<(String, usize)>,
    section_master: Vec<Option<String>>,
    warnings: Vec<String>,
}

impl<'d> Writer<'d> {
    fn new(doc: &'d Document) -> Self {
        let mut w = Self {
            doc,
            theme: &doc.theme,
            content_autos: AutoStyles::new(""),
            styles_autos: AutoStyles::new("M"),
            part: Part::Content,
            pictures: BTreeMap::new(),
            comment_spans: HashMap::new(),
            comments_written: HashSet::new(),
            changes: Vec::new(),
            lists_seen: HashSet::new(),
            note_counters: [0, 0],
            counters: HashMap::new(),
            master_pages: Vec::new(),
            section_master: Vec::new(),
            warnings: Vec::new(),
        };
        w.scan_comments();
        w.plan_masters();
        w
    }

    fn next(&mut self, what: &'static str) -> usize {
        let c = self.counters.entry(what).or_insert(0);
        *c += 1;
        *c
    }

    fn autos(&mut self) -> &mut AutoStyles {
        match self.part {
            Part::Content => &mut self.content_autos,
            Part::Styles => &mut self.styles_autos,
        }
    }

    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_owned());
        }
    }

    fn scan_comments(&mut self) {
        fn walk(blocks: &[Block], spans: &mut HashMap<NodeId, CommentSpan>) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        for m in p.content.marks() {
                            if let Mark::Comment(c) = m.mark {
                                spans
                                    .entry(c)
                                    .and_modify(|s| s.last = p.id)
                                    .or_insert(CommentSpan {
                                        first: p.id,
                                        last: p.id,
                                    });
                            }
                        }
                    }
                    Block::Table(t) => {
                        for r in &t.rows {
                            for c in &r.cells {
                                walk(&c.blocks, spans);
                            }
                        }
                    }
                    Block::Sdt(s) => walk(&s.blocks, spans),
                    Block::Field(f) => walk(&f.blocks, spans),
                    Block::Opaque(_) => {}
                }
            }
        }
        for s in &self.doc.sections {
            walk(&s.blocks, &mut self.comment_spans);
        }
    }

    /// One master page per distinct page setup and header/footer set.
    fn plan_masters(&mut self) {
        let refs_h: Vec<HeaderFooterRefs> = self.doc.sections.iter().map(|s| s.headers).collect();
        let refs_f: Vec<HeaderFooterRefs> = self.doc.sections.iter().map(|s| s.footers).collect();
        let mut keys: Vec<(SectionProps, [Option<NodeId>; 6])> = Vec::new();
        for (i, s) in self.doc.sections.iter().enumerate() {
            let continuous = i > 0
                && matches!(
                    s.props.break_type,
                    SectionBreak::Continuous | SectionBreak::NextColumn
                )
                && same_page(&s.props, &self.doc.sections[i - 1].props);
            if continuous {
                self.section_master.push(None);
                continue;
            }
            let mut props = s.props.clone();
            props.break_type = SectionBreak::NextPage;
            props.line_numbering = None;
            props.doc_grid = None;
            let r = |refs: &[HeaderFooterRefs], k| resolve_header_footer(refs, i, k);
            let stories = [
                r(&refs_h, HeaderFooterKind::Default),
                r(&refs_h, HeaderFooterKind::Even),
                r(&refs_h, HeaderFooterKind::First),
                r(&refs_f, HeaderFooterKind::Default),
                r(&refs_f, HeaderFooterKind::Even),
                r(&refs_f, HeaderFooterKind::First),
            ];
            let key = (props, stories);
            let idx = keys.iter().position(|k| *k == key).unwrap_or_else(|| {
                keys.push(key);
                keys.len() - 1
            });
            let name = if idx == 0 {
                "Standard".to_owned()
            } else {
                format!("Kiln{idx}")
            };
            if !self.master_pages.iter().any(|(n, _)| *n == name) {
                self.master_pages.push((name.clone(), i));
            }
            self.section_master.push(Some(name));
        }
    }

    // ------------------------------------------------------------ content.xml

    fn content(&mut self) -> Vec<u8> {
        self.part = Part::Content;
        let mut body = XmlOut::fragment();
        let doc = self.doc;
        let unanchored: Vec<&Comment> = doc
            .comments
            .iter()
            .filter(|c| !self.comment_spans.contains_key(&c.id))
            .collect();
        let mut pending_comments = unanchored;
        for (i, s) in doc.sections.iter().enumerate() {
            match self.section_master.get(i).cloned().flatten() {
                Some(master) => {
                    let blocks = &s.blocks;
                    if pending_comments.is_empty() {
                        self.blocks(&mut body, blocks, Some(master));
                    } else {
                        // Comments without a range are anchored at the start of the body.
                        let comments = std::mem::take(&mut pending_comments);
                        self.blocks_with_lead(&mut body, blocks, Some(master), &comments);
                    }
                }
                None => {
                    let n = self.next("section");
                    let style = self.section_style(&s.props.columns);
                    body.start("text:section")
                        .attr("text:style-name", style)
                        .attr("text:name", format!("Section{n}"));
                    self.blocks(&mut body, &s.blocks, None);
                    body.end();
                }
            }
        }
        let body = body.into_string();
        let mut w = XmlOut::new();
        w.root("office:document-content", PREFIXES)
            .attr("office:version", "1.3");
        w.start("office:scripts").end();
        self.font_decls(&mut w);
        w.start("office:automatic-styles");
        for e in &self.content_autos.list {
            e.write(&mut w);
        }
        w.end();
        w.start("office:body").start("office:text");
        if !self.changes.is_empty() {
            w.start("text:tracked-changes");
            for c in &self.changes {
                w.raw(c);
            }
            w.end();
        }
        w.raw(&body);
        w.finish()
    }

    fn section_style(&mut self, cols: &Columns) -> String {
        let mut sp = El::new("style:section-properties").with("text:dont-balance-text-columns", "false");
        sp.children.push(columns_el(cols));
        let el = El::new("style:style")
            .with("style:family", "section")
            .with_child(sp);
        self.autos().add("Sect", el)
    }

    fn font_decls(&self, w: &mut XmlOut) {
        let mut fonts = BTreeSet::new();
        fn collect(e: &El, out: &mut BTreeSet<String>) {
            for (k, v) in &e.attrs {
                if k.starts_with("style:font-name") {
                    out.insert(v.clone());
                }
            }
            for c in &e.children {
                collect(c, out);
            }
        }
        for e in self
            .content_autos
            .list
            .iter()
            .chain(&self.styles_autos.list)
        {
            collect(e, &mut fonts);
        }
        for e in self.named_style_els() {
            collect(&e, &mut fonts);
        }
        w.start("office:font-face-decls");
        for f in fonts {
            let family = if f.contains(' ') {
                format!("'{f}'")
            } else {
                f.clone()
            };
            w.start("style:font-face")
                .attr("style:name", &f)
                .attr("svg:font-family", family)
                .end();
        }
        w.end();
    }

    // ------------------------------------------------------------ styles.xml

    fn default_paragraph_style(&self) -> Option<&'d Style> {
        self.doc
            .styles
            .by_type(StyleType::Paragraph)
            .find(|s| s.is_default)
    }

    fn named_style_els(&self) -> Vec<El> {
        let mut out = Vec::new();
        let doc = self.doc;
        // The default style: document defaults plus the default paragraph style's own
        // formatting, so paragraphs without a style look as in Word.
        let mut dppr = doc.styles.doc_defaults.ppr.clone();
        let mut drpr = doc.styles.doc_defaults.rpr.clone();
        if let Some(normal) = self.default_paragraph_style() {
            if let Some(p) = &normal.ppr {
                dppr = kiln_doc::merge::merge_paragraph_props(&dppr, Some(p));
            }
            if let Some(r) = &normal.rpr {
                drpr = merge_run_props(&drpr, Some(r));
            }
        }
        dppr.numbering = None;
        dppr.style_id = None;
        dppr.outline_level = None;
        let mut def = El::new("style:default-style").with("style:family", "paragraph");
        let mut pp = para_props_el(&dppr, self.theme);
        pp.set(
            "style:tab-stop-distance",
            twips_str(i64::from(doc.settings.default_tab_stop.max(1))),
        );
        def.children.push(pp);
        def.push_nonempty(run_props_el(&drpr, self.theme));
        out.push(def);
        for s in doc.styles.iter() {
            let family = match s.kind {
                StyleType::Paragraph => "paragraph",
                StyleType::Character => "text",
                _ => continue,
            };
            let mut e = El::new("style:style")
                .with("style:name", ncname(&s.id))
                .with("style:family", family);
            if s.name != s.id && !s.name.is_empty() {
                e.set("style:display-name", s.name.clone());
            }
            if let Some(b) = s.based_on.as_ref().filter(|b| doc.styles.contains(b)) {
                e.set("style:parent-style-name", ncname(b));
            }
            if let Some(n) = s.next.as_ref().filter(|n| doc.styles.contains(n)) {
                e.set("style:next-style-name", ncname(n));
            }
            if s.hidden {
                e.set("style:hidden", "true");
            }
            if let Some(p) = &s.ppr {
                if let Some(l) = p.outline_level.filter(|l| *l < 9) {
                    e.set("style:default-outline-level", (l + 1).to_string());
                }
                if let Some(id) = p.numbering.and_then(|n| n.num_id).filter(|id| {
                    *id > 0 && doc.numbering.instances.contains_key(id)
                }) {
                    e.set("style:list-style-name", format!("L{id}"));
                }
                let mut clean = p.clone();
                clean.numbering = None;
                clean.outline_level = None;
                clean.style_id = None;
                e.push_nonempty(para_props_el(&clean, self.theme));
            }
            if let Some(r) = &s.rpr {
                let mut r = r.clone();
                r.style_id = None;
                e.push_nonempty(run_props_el(&r, self.theme));
            }
            out.push(e);
        }
        for (id, inst) in &doc.numbering.instances {
            if let Some(abs) = doc.numbering.abstracts.get(&inst.abstract_id) {
                let mut abs = abs.clone();
                if abs.name.as_deref() == Some(&format!("L{id}")) {
                    abs.name = None;
                }
                out.push(list_style_el(&format!("L{id}"), &abs, Some(inst)));
            }
        }
        out
    }

    fn styles(&mut self) -> Vec<u8> {
        self.part = Part::Styles;
        let doc = self.doc;
        let mut masters = XmlOut::fragment();
        let mut layouts: Vec<El> = Vec::new();
        let plan = self.master_pages.clone();
        for (n, (name, sec)) in plan.iter().enumerate() {
            let s = &doc.sections[*sec];
            let refs_h: Vec<HeaderFooterRefs> = doc.sections.iter().map(|s| s.headers).collect();
            let refs_f: Vec<HeaderFooterRefs> = doc.sections.iter().map(|s| s.footers).collect();
            let h = |k| resolve_header_footer(&refs_h, *sec, k);
            let f = |k| resolve_header_footer(&refs_f, *sec, k);
            let even = doc.settings.even_and_odd_headers;
            let title = s.props.title_page;
            let has_header = h(HeaderFooterKind::Default).is_some()
                || (even && h(HeaderFooterKind::Even).is_some())
                || (title && h(HeaderFooterKind::First).is_some());
            let has_footer = f(HeaderFooterKind::Default).is_some()
                || (even && f(HeaderFooterKind::Even).is_some())
                || (title && f(HeaderFooterKind::First).is_some());
            let layout = format!("Mpm{}", n + 1);
            layouts.push(page_layout_el(&layout, &s.props, has_header, has_footer));
            masters
                .start("style:master-page")
                .attr("style:name", name)
                .attr("style:page-layout-name", &layout);
            for (el, id, part) in [
                ("style:header", h(HeaderFooterKind::Default), HeaderFooterPart::Header),
                (
                    "style:header-left",
                    h(HeaderFooterKind::Even).filter(|_| even),
                    HeaderFooterPart::Header,
                ),
                (
                    "style:header-first",
                    h(HeaderFooterKind::First).filter(|_| title),
                    HeaderFooterPart::Header,
                ),
                ("style:footer", f(HeaderFooterKind::Default), HeaderFooterPart::Footer),
                (
                    "style:footer-left",
                    f(HeaderFooterKind::Even).filter(|_| even),
                    HeaderFooterPart::Footer,
                ),
                (
                    "style:footer-first",
                    f(HeaderFooterKind::First).filter(|_| title),
                    HeaderFooterPart::Footer,
                ),
            ] {
                let Some(id) = id else { continue };
                let Some(story) = doc
                    .headers_footers
                    .iter()
                    .find(|x| x.id == id && x.part == part)
                else {
                    continue;
                };
                masters.start(el);
                self.blocks(&mut masters, &story.blocks, None);
                masters.end();
            }
            masters.end();
        }
        let masters = masters.into_string();
        let named = self.named_style_els();
        let mut w = XmlOut::new();
        w.root("office:document-styles", PREFIXES)
            .attr("office:version", "1.3");
        self.font_decls(&mut w);
        w.start("office:styles");
        for e in &named {
            e.write(&mut w);
        }
        // Notes configuration (Arabic numbering, as Word's default).
        w.start("text:notes-configuration")
            .attr("text:note-class", "footnote")
            .attr("style:num-format", "1")
            .attr("text:start-value", "0")
            .attr("text:footnotes-position", "page")
            .attr("text:start-numbering-at", "document")
            .end();
        w.start("text:notes-configuration")
            .attr("text:note-class", "endnote")
            .attr("style:num-format", "i")
            .attr("text:start-value", "0")
            .end();
        w.end();
        w.start("office:automatic-styles");
        for e in &layouts {
            e.write(&mut w);
        }
        for e in &self.styles_autos.list {
            e.write(&mut w);
        }
        w.end();
        w.start("office:master-styles").raw(&masters).end();
        w.finish()
    }

    // ------------------------------------------------------------ blocks

    fn blocks_with_lead(
        &mut self,
        w: &mut XmlOut,
        blocks: &[Block],
        master: Option<String>,
        comments: &[&Comment],
    ) {
        if let Some(Block::Paragraph(first)) = blocks.first() {
            // Point comments go at the start of the first paragraph.
            let lead: Vec<NodeId> = comments.iter().map(|c| c.id).collect();
            self.paragraph_with(w, first, master, &lead);
            self.blocks(w, &blocks[1..], None);
        } else {
            let mut ids = Vec::new();
            for c in comments {
                ids.push(c.id);
            }
            // No paragraph to hang them on: an empty one.
            let p = Paragraph::new(NodeId(u64::MAX));
            self.paragraph_with(w, &p, master, &ids);
            self.blocks(w, blocks, None);
        }
    }

    fn blocks(&mut self, w: &mut XmlOut, blocks: &[Block], master: Option<String>) {
        let mut master = master;
        let mut i = 0;
        while i < blocks.len() {
            match &blocks[i] {
                Block::Paragraph(p) if self.list_of(p).is_some() => {
                    let id = self.list_of(p).unwrap_or_default();
                    let mut j = i;
                    let mut group = Vec::new();
                    while let Some(Block::Paragraph(q)) = blocks.get(j) {
                        if self.list_of(q) != Some(id) {
                            break;
                        }
                        group.push(q);
                        j += 1;
                    }
                    self.list_group(w, &group, id, master.take());
                    i = j;
                    continue;
                }
                Block::Paragraph(p) => {
                    for piece in split_breaks(p) {
                        self.paragraph(w, &piece, master.take(), false);
                    }
                }
                Block::Table(t) => self.table(w, t, master.take()),
                Block::Field(f) => self.index(w, f, master.take()),
                Block::Sdt(s) => self.blocks(w, &s.blocks, master.take()),
                Block::Opaque(o) => {
                    self.warn("content kept from a Word file (alternative chunks, custom XML) is not saved in ODF");
                    let p = Paragraph::new(o.id);
                    self.paragraph(w, &p, master.take(), false);
                }
            }
            i += 1;
        }
    }

    fn list_of(&self, p: &Paragraph) -> Option<u32> {
        p.props
            .numbering
            .and_then(|n| n.num_id)
            .filter(|id| *id > 0 && self.doc.numbering.instances.contains_key(id))
    }

    fn list_group(&mut self, w: &mut XmlOut, paras: &[&Paragraph], id: u32, master: Option<String>) {
        let mut master = master;
        w.start("text:list").attr("text:style-name", format!("L{id}"));
        if !self.lists_seen.insert(id) {
            w.attr("text:continue-numbering", "true");
        }
        let mut cur: Option<usize> = None;
        for p in paras {
            let l = usize::from(p.props.numbering.and_then(|n| n.level).unwrap_or(0).min(8));
            match cur {
                None => {
                    for k in 0..=l {
                        if k > 0 {
                            w.start("text:list");
                        }
                        w.start("text:list-item");
                    }
                }
                Some(c) if l > c => {
                    for _ in c + 1..=l {
                        w.start("text:list");
                        w.start("text:list-item");
                    }
                }
                Some(c) => {
                    w.end();
                    for _ in l..c {
                        w.end();
                        w.end();
                    }
                    w.start("text:list-item");
                }
            }
            cur = Some(l);
            let pieces = split_breaks(p);
            for (k, piece) in pieces.iter().enumerate() {
                if k > 0 {
                    // The text after a page break inside a list item: an unnumbered item.
                    w.end();
                    w.start("text:list-item");
                }
                self.paragraph(w, piece, master.take(), true);
            }
        }
        if let Some(c) = cur {
            w.end();
            for _ in 0..c {
                w.end();
                w.end();
            }
        }
        w.end();
    }

    // ------------------------------------------------------------ paragraphs

    fn outline_level(&self, p: &Paragraph) -> Option<u8> {
        if let Some(l) = p.props.outline_level {
            return Some(l);
        }
        let mut cur = p.props.style_id.as_deref();
        let mut guard = 0;
        while let Some(id) = cur {
            let s = self.doc.styles.get(id)?;
            if let Some(l) = s.ppr.as_ref().and_then(|pp| pp.outline_level) {
                return Some(l);
            }
            guard += 1;
            if guard > 64 {
                return None;
            }
            cur = s.based_on.as_deref();
        }
        None
    }

    fn paragraph(&mut self, w: &mut XmlOut, p: &Paragraph, master: Option<String>, in_list: bool) {
        let _ = in_list;
        self.paragraph_with(w, p, master, &[]);
    }

    fn paragraph_with(
        &mut self,
        w: &mut XmlOut,
        p: &Paragraph,
        master: Option<String>,
        lead_comments: &[NodeId],
    ) {
        let level = self.outline_level(p).filter(|l| *l < 9);
        let mut clean: ParagraphProps = p.props.clone();
        clean.style_id = None;
        clean.numbering = None;
        clean.outline_level = None;
        // A column break at the start is a property in ODF.
        let starts_with_column_break = matches!(
            p.content.object_at(0),
            Some(InlineObject::Break(b)) if b.kind == BreakKind::Column
        ) && p.content.char_at(0) == Some(OBJECT_CHAR);
        let mut pp = para_props_el(&clean, self.theme);
        if starts_with_column_break {
            pp.set("fo:break-before", "column");
        }
        let parent = p
            .props
            .style_id
            .as_ref()
            .filter(|s| self.doc.styles.contains(s))
            .map(|s| ncname(s));
        let style_name = if pp.is_empty() && master.is_none() {
            parent
        } else {
            let mut st = El::new("style:style").with("style:family", "paragraph");
            if let Some(par) = &parent {
                st.set("style:parent-style-name", par.clone());
            }
            if let Some(m) = &master {
                st.set("style:master-page-name", m.clone());
            }
            st.push_nonempty(pp);
            Some(self.autos().add("P", st))
        };
        match level {
            Some(l) => {
                w.start("text:h").attr("text:outline-level", (l + 1).to_string());
            }
            None => {
                w.start("text:p");
            }
        }
        if let Some(s) = style_name {
            w.attr("text:style-name", s);
        }
        for c in lead_comments {
            if let Some(comment) = self.doc.comments.iter().find(|x| x.id == *c) {
                self.annotation(w, comment, None);
            }
        }
        self.inlines(w, p, starts_with_column_break);
        w.end();
    }

    fn span_style(&mut self, props: &RunProps) -> Option<String> {
        let mut own = props.clone();
        let style = own
            .style_id
            .take()
            .filter(|s| self.doc.styles.contains(s))
            .map(|s| ncname(&s));
        if own == RunProps::default() {
            return style;
        }
        let mut st = El::new("style:style").with("style:family", "text");
        if let Some(s) = style {
            st.set("style:parent-style-name", s);
        }
        st.push_nonempty(run_props_el(&own, self.theme));
        Some(self.autos().add("T", st))
    }

    #[allow(clippy::too_many_lines)]
    fn inlines(&mut self, w: &mut XmlOut, p: &Paragraph, skip_first_object: bool) {
        let content = &p.content;
        let chars: Vec<char> = content.raw_text().chars().collect();
        let len = chars.len();
        let mut bounds: BTreeSet<usize> = BTreeSet::new();
        bounds.insert(0);
        bounds.insert(len);
        let mut pos = 0;
        for r in content.runs() {
            pos += r.len;
            bounds.insert(pos.min(len));
        }
        let marks = content.marks();
        for m in marks {
            bounds.insert(m.start.min(len));
            bounds.insert(m.end.min(len));
        }
        let bounds: Vec<usize> = bounds.into_iter().collect();
        let objects: HashMap<usize, &InlineObject> = content.objects_with_offsets().collect();
        let mut last_space = true;
        let mut link: Option<(usize, usize)> = None;
        // The open deletion: its end and the region being written.
        let mut deletion: Option<(usize, XmlOut, XmlOut, bool)> = None;
        for (bi, &a) in bounds.iter().enumerate() {
            let b = bounds.get(bi + 1).copied().unwrap_or(a);
            // Close a link that ends here.
            if let Some((_, end)) = link
                && end <= a
            {
                w.end();
                link = None;
            }
            // Close a deletion that ends here.
            if let Some((end, _, _, _)) = &deletion
                && *end <= a
                && let Some(d) = deletion.take()
            {
                self.finish_deletion(d);
            }
            for m in marks.iter().filter(|m| m.end == a && m.start < a) {
                match &m.mark {
                    Mark::Comment(c) => {
                        if self.comment_spans.get(c).is_some_and(|s| s.last == p.id) {
                            w.start("office:annotation-end")
                                .attr("office:name", annotation_name(*c))
                                .end();
                        }
                    }
                    Mark::Insertion(_) | Mark::FormatChange { .. } | Mark::MoveTo(_) => {
                        w.start("text:change-end")
                            .attr("text:change-id", change_id(p.id, m.start, m.end))
                            .end();
                    }
                    _ => {}
                }
            }
            for m in marks.iter().filter(|m| m.start == a && m.end > a) {
                match &m.mark {
                    Mark::Comment(c) => {
                        if self.comment_spans.get(c).is_some_and(|s| s.first == p.id)
                            && let Some(comment) = self.doc.comments.iter().find(|x| x.id == *c)
                        {
                            self.annotation(w, comment, Some(*c));
                        }
                    }
                    Mark::Insertion(info) | Mark::MoveTo(info) => {
                        let id = change_id(p.id, m.start, m.end);
                        self.change_region(&id, "text:insertion", &info.author, info.date.as_deref(), None);
                        w.start("text:change-start").attr("text:change-id", id).end();
                    }
                    Mark::FormatChange { info, .. } => {
                        let id = change_id(p.id, m.start, m.end);
                        self.change_region(&id, "text:format-change", &info.author, info.date.as_deref(), None);
                        w.start("text:change-start").attr("text:change-id", id).end();
                    }
                    Mark::Deletion(info) | Mark::MoveFrom(info) => {
                        if deletion.is_none() {
                            let id = change_id(p.id, m.start, m.end);
                            w.start("text:change").attr("text:change-id", &id).end();
                            let mut head = XmlOut::fragment();
                            head.start("text:changed-region")
                                .attr("xml:id", &id)
                                .attr("text:id", &id)
                                .start("text:deletion")
                                .start("office:change-info");
                            head.text_element("dc:creator", &info.author);
                            if let Some(d) = &info.date {
                                head.text_element("dc:date", d);
                            }
                            head.end();
                            let mut body = XmlOut::fragment();
                            body.start("text:p");
                            deletion = Some((m.end, head, body, true));
                        }
                    }
                    _ => {}
                }
            }
            if a >= b {
                continue;
            }
            // Open a link covering this segment.
            if link.is_none()
                && let Some(lm) = marks
                    .iter()
                    .find(|m| m.start <= a && m.end >= b && matches!(m.mark, Mark::Link(_)))
                && deletion.is_none()
                && let Mark::Link(h) = &lm.mark
            {
                link_start(w, h);
                link = Some((lm.start, lm.end));
            }
            let props = content.props_at(a).cloned().unwrap_or_default();
            let style = self.span_style(&props);
            let target: &mut XmlOut = match deletion.as_mut() {
                Some((_, _, body, _)) => body,
                None => w,
            };
            let mut in_del_space = true;
            let space_state = if deletion.is_some() {
                &mut in_del_space
            } else {
                &mut last_space
            };
            if let Some(s) = &style {
                target.start("text:span").attr("text:style-name", s);
            }
            let mut buf = String::new();
            let mut spaces = 0usize;
            let flush = |t: &mut XmlOut, buf: &mut String, spaces: &mut usize| {
                if !buf.is_empty() {
                    t.text(buf);
                    buf.clear();
                }
                if *spaces > 0 {
                    t.start("text:s");
                    if *spaces > 1 {
                        t.attr("text:c", spaces.to_string());
                    }
                    t.end();
                    *spaces = 0;
                }
            };
            for (off, c) in chars.iter().enumerate().take(b).skip(a) {
                match *c {
                    ' ' => {
                        if *space_state {
                            if !buf.is_empty() {
                                target.text(&buf);
                                buf.clear();
                            }
                            spaces += 1;
                        } else {
                            buf.push(' ');
                            *space_state = true;
                        }
                    }
                    '\t' => {
                        flush(target, &mut buf, &mut spaces);
                        target.start("text:tab").end();
                        *space_state = false;
                    }
                    OBJECT_CHAR => {
                        flush(target, &mut buf, &mut spaces);
                        if !(skip_first_object && off == 0)
                            && let Some(o) = objects.get(&off)
                        {
                            self.object(target, o, &props);
                        }
                        *space_state = false;
                    }
                    '\n' | '\r' | '\u{b}' | '\u{c}' => {
                        flush(target, &mut buf, &mut spaces);
                        target.start("text:line-break").end();
                        *space_state = false;
                    }
                    c => {
                        if spaces > 0 {
                            flush(target, &mut buf, &mut spaces);
                        }
                        buf.push(c);
                        *space_state = false;
                    }
                }
            }
            flush(target, &mut buf, &mut spaces);
            if style.is_some() {
                target.end();
            }
        }
        if link.is_some() {
            w.end();
        }
        if let Some(d) = deletion.take() {
            self.finish_deletion(d);
        }
    }

    fn finish_deletion(&mut self, d: (usize, XmlOut, XmlOut, bool)) {
        let (_, mut head, body, _) = d;
        head.raw(&body.into_string());
        self.changes.push(head.into_string());
    }

    fn change_region(&mut self, id: &str, kind: &str, author: &str, date: Option<&str>, body: Option<&str>) {
        let mut r = XmlOut::fragment();
        r.start("text:changed-region")
            .attr("xml:id", id)
            .attr("text:id", id)
            .start(kind)
            .start("office:change-info");
        r.text_element("dc:creator", author);
        if let Some(d) = date {
            r.text_element("dc:date", d);
        }
        r.end();
        if let Some(b) = body {
            r.raw(b);
        }
        self.changes.push(r.into_string());
    }

    fn annotation(&mut self, w: &mut XmlOut, c: &Comment, named: Option<NodeId>) {
        if !self.comments_written.insert(c.id) {
            return;
        }
        w.start("office:annotation");
        if named.is_some() {
            w.attr("office:name", annotation_name(c.id));
        }
        if c.meta.resolved {
            w.attr("loext:resolved", "true");
        }
        if let Some(p) = c.meta.parent {
            w.attr("loext:parent-name", annotation_name(p));
        }
        w.text_element("dc:creator", &c.meta.author);
        if let Some(d) = &c.meta.date {
            w.text_element("dc:date", d);
        }
        if let Some(i) = &c.meta.initials {
            w.text_element("meta:creator-initials", i);
        }
        // Comment bodies hold paragraphs only (no nested comments or notes).
        let saved = std::mem::take(&mut self.comment_spans);
        self.blocks(w, &c.blocks, None);
        self.comment_spans = saved;
        w.end();
    }

    #[allow(clippy::too_many_lines)]
    fn object(&mut self, w: &mut XmlOut, o: &InlineObject, props: &RunProps) {
        match o {
            InlineObject::Break(b) => match b.kind {
                BreakKind::TextWrapping => {
                    w.start("text:line-break").end();
                }
                // Page and column breaks inside a paragraph were split off before.
                BreakKind::Page | BreakKind::Column => {}
            },
            InlineObject::Drawing(d) => self.drawing(w, d),
            InlineObject::Field(f) => self.field(w, f),
            InlineObject::NoteRef(n) => {
                let Some(note) = self.doc.notes.iter().find(|x| x.id == n.note) else {
                    return;
                };
                let k = usize::from(n.kind == NoteKind::Endnote);
                self.note_counters[k] += 1;
                let num = self.note_counters[k];
                let (class, prefix) = if k == 1 {
                    ("endnote", "edn")
                } else {
                    ("footnote", "ftn")
                };
                w.start("text:note")
                    .attr("text:id", format!("{prefix}{num}"))
                    .attr("text:note-class", class);
                w.start("text:note-citation");
                match &n.custom_mark {
                    Some(m) => {
                        w.attr("text:label", m).text(m);
                    }
                    None => {
                        w.text(&num.to_string());
                    }
                }
                w.end();
                w.start("text:note-body");
                let blocks = note.blocks.clone();
                let saved = std::mem::take(&mut self.comment_spans);
                self.blocks(w, &blocks, None);
                self.comment_spans = saved;
                w.end();
                w.end();
            }
            InlineObject::Symbol(s) => {
                if let Some(c) = char::from_u32(s.code) {
                    let mut p = props.clone();
                    p.fonts = Some(kiln_doc::props::RunFonts::latin(&s.font));
                    let style = self.span_style(&p);
                    if let Some(st) = style {
                        w.start("text:span").attr("text:style-name", st);
                        w.text(&c.to_string());
                        w.end();
                    } else {
                        w.text(&c.to_string());
                    }
                }
            }
            InlineObject::Equation(_) => {
                self.warn("equations are not saved in ODF yet");
            }
            InlineObject::BookmarkStart { name } => {
                w.start("text:bookmark-start").attr("text:name", name).end();
            }
            InlineObject::BookmarkEnd { name } => {
                w.start("text:bookmark-end").attr("text:name", name).end();
            }
            InlineObject::Opaque { label, .. } => {
                if !label.is_empty() {
                    w.text(label);
                }
            }
        }
    }

    fn field(&mut self, w: &mut XmlOut, f: &kiln_doc::fields::Field) {
        let instr = parse_field_instruction(&f.instr);
        let kw = instr.keyword.as_str();
        let el = match kw {
            "PAGE" => Some("text:page-number"),
            "NUMPAGES" | "SECTIONPAGES" => Some("text:page-count"),
            "DATE" => Some("text:date"),
            "TIME" => Some("text:time"),
            "CREATEDATE" => Some("text:creation-date"),
            "SAVEDATE" => Some("text:modification-date"),
            "PRINTDATE" => Some("text:print-date"),
            "TITLE" => Some("text:title"),
            "SUBJECT" => Some("text:subject"),
            "AUTHOR" => Some("text:initial-creator"),
            "LASTSAVEDBY" => Some("text:creator"),
            "KEYWORDS" => Some("text:keywords"),
            "COMMENTS" => Some("text:description"),
            "FILENAME" => Some("text:file-name"),
            "TEMPLATE" => Some("text:template-name"),
            "NUMWORDS" => Some("text:word-count"),
            "NUMCHARS" => Some("text:character-count"),
            "REVNUM" => Some("text:editing-cycles"),
            "DOCPROPERTY" if !instr.args.is_empty() => Some("text:user-defined"),
            "REF" | "PAGEREF" if !instr.args.is_empty() => Some("text:bookmark-ref"),
            "SEQ" if !instr.args.is_empty() => Some("text:sequence"),
            _ => None,
        };
        let Some(el) = el else {
            if !f.result.is_empty() {
                w.text(&f.result);
            }
            let msg = format!("{kw} fields are saved as their result text");
            self.warn(&msg);
            return;
        };
        w.start(el);
        match kw {
            "PAGE" => {
                w.attr("text:select-page", "current");
            }
            "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => {
                if let Some(pic) = &instr.date_format {
                    let code = crate::numfmt::word_picture_to_excel(pic);
                    if let Some(style) = crate::numfmt::data_style_el(&code, kw == "TIME") {
                        let name = self.autos().add("N", style);
                        w.attr("style:data-style-name", name);
                    }
                }
                if f.locked {
                    w.attr("text:fixed", "true");
                }
            }
            "DOCPROPERTY" => {
                w.attr("text:name", &instr.args[0]);
            }
            "REF" | "PAGEREF" => {
                w.attr(
                    "text:reference-format",
                    if kw == "PAGEREF" { "page" } else { "text" },
                )
                .attr("text:ref-name", &instr.args[0]);
            }
            "SEQ" => {
                let name = &instr.args[0];
                w.attr("text:name", name)
                    .attr("text:formula", format!("ooow:{name}+1"))
                    .attr("style:num-format", "1");
            }
            _ => {}
        }
        w.text(&f.result);
        w.end();
    }

    fn index(&mut self, w: &mut XmlOut, fb: &FieldBlock, master: Option<String>) {
        let instr = parse_field_instruction(&fb.field.instr);
        let has = |s: &str| instr.switches.iter().any(|x| x.name == s);
        let switch = |s: &str| {
            instr
                .switches
                .iter()
                .find(|x| x.name == s)
                .and_then(|x| x.value.clone())
        };
        let n = self.next("index");
        let (el, source) = match instr.keyword.as_str() {
            "TOC" if has("\\c") => {
                let caption = switch("\\c").unwrap_or_else(|| "Figure".into());
                if caption == "Table" {
                    ("text:table-index", "text:table-index-source")
                } else {
                    ("text:illustration-index", "text:illustration-index-source")
                }
            }
            "TOC" => ("text:table-of-content", "text:table-of-content-source"),
            "INDEX" => ("text:alphabetical-index", "text:alphabetical-index-source"),
            "BIBLIOGRAPHY" => ("text:bibliography", "text:bibliography-source"),
            _ => {
                self.warn("multi-paragraph fields other than indexes are saved as their result");
                self.blocks(w, &fb.blocks, master);
                return;
            }
        };
        if let Some(m) = master {
            // An index cannot start a page style; an empty paragraph before it does.
            let p = Paragraph::new(fb.id);
            self.paragraph(w, &p, Some(m), false);
        }
        w.start(el)
            .attr("text:name", format!("Index{n}"))
            .attr("text:protected", "false");
        w.start(source);
        match el {
            "text:table-of-content" => {
                let levels = switch("\\o")
                    .and_then(|r| r.split('-').nth(1).and_then(|e| e.trim().parse::<u8>().ok()))
                    .unwrap_or(3)
                    .clamp(1, 9);
                w.attr("text:outline-level", levels.to_string());
            }
            "text:illustration-index" | "text:table-index" => {
                let caption = switch("\\c").unwrap_or_else(|| "Figure".into());
                w.attr("text:caption-sequence-name", caption);
            }
            _ => {}
        }
        w.end();
        w.start("text:index-body");
        self.blocks(w, &fb.blocks, None);
        w.end();
        w.end();
    }

    // ------------------------------------------------------------ drawings

    fn picture_path(&mut self, key: &str) -> Option<String> {
        if let Some(p) = self.pictures.get(key) {
            return Some(p.clone());
        }
        let media = self.doc.media.get(key)?;
        let base = key.rsplit('/').next().unwrap_or(key);
        let stem = base.rsplit_once('.').map_or(base, |(s, _)| s);
        let ext = base
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .filter(|e| !e.is_empty() && e.len() <= 5)
            .unwrap_or_else(|| extension_for(&media.content_type).to_owned());
        let safe: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let mut path = format!("Pictures/{safe}.{ext}");
        let mut n = 1;
        while self.pictures.values().any(|p| *p == path) {
            n += 1;
            path = format!("Pictures/{safe}-{n}.{ext}");
        }
        self.pictures.insert(key.to_owned(), path.clone());
        Some(path)
    }

    fn graphic_style(&mut self, d: &Drawing, frame: bool) -> String {
        let mut gp = El::new("style:graphic-properties");
        match &d.placement {
            Placement::Inline => {
                gp.set("style:vertical-pos", "top");
                gp.set("style:vertical-rel", "baseline");
            }
            Placement::Anchor(a) => {
                let (wrap, run) = match (a.wrap, a.wrap_side) {
                    (WrapType::TopAndBottom, _) => ("none", None),
                    (WrapType::Behind, _) => ("run-through", Some("background")),
                    (WrapType::InFront | WrapType::None, _) => ("run-through", Some("foreground")),
                    (_, Some(WrapSide::Left)) => ("left", None),
                    (_, Some(WrapSide::Right)) => ("right", None),
                    (_, Some(WrapSide::Largest)) => ("biggest", None),
                    _ => ("parallel", None),
                };
                gp.set("style:wrap", wrap);
                if let Some(r) = run {
                    gp.set("style:run-through", r);
                }
                if a.wrap == WrapType::Tight || a.wrap == WrapType::Through {
                    gp.set("style:wrap-contour", "true");
                    gp.set("style:wrap-contour-mode", "outside");
                }
                for (n, v) in [
                    ("fo:margin-left", a.dist_left),
                    ("fo:margin-right", a.dist_right),
                    ("fo:margin-top", a.dist_top),
                    ("fo:margin-bottom", a.dist_bottom),
                ] {
                    if v != 0 {
                        gp.set(n, emu_str(v));
                    }
                }
                let (hp, hr) = (
                    match a.h {
                        PosH::Offset(_) => "from-left",
                        PosH::Align(AlignH::Left) => "left",
                        PosH::Align(AlignH::Center) => "center",
                        PosH::Align(AlignH::Right) => "right",
                        PosH::Align(AlignH::Inside) => "inside",
                        PosH::Align(AlignH::Outside) => "outside",
                    },
                    match a.relative_h {
                        RelativeFromH::Page => "page",
                        RelativeFromH::Margin => "page-content",
                        RelativeFromH::LeftMargin | RelativeFromH::InsideMargin => "page-start-margin",
                        RelativeFromH::RightMargin | RelativeFromH::OutsideMargin => "page-end-margin",
                        RelativeFromH::Character => "char",
                        RelativeFromH::Column => "paragraph",
                    },
                );
                let (vp, vr) = (
                    match a.v {
                        PosV::Offset(_) => "from-top",
                        PosV::Align(AlignV::Top | AlignV::Inside) => "top",
                        PosV::Align(AlignV::Center) => "middle",
                        PosV::Align(AlignV::Bottom | AlignV::Outside) => "bottom",
                    },
                    match a.relative_v {
                        RelativeFromV::Page => "page",
                        RelativeFromV::Margin | RelativeFromV::TopMargin | RelativeFromV::BottomMargin | RelativeFromV::InsideMargin | RelativeFromV::OutsideMargin => "page-content",
                        RelativeFromV::Line => "line",
                        RelativeFromV::Paragraph => "paragraph",
                    },
                );
                gp.set("style:horizontal-pos", hp);
                gp.set("style:horizontal-rel", hr);
                gp.set("style:vertical-pos", vp);
                gp.set("style:vertical-rel", vr);
            }
        }
        if frame && d.text_box.is_some() {
            gp.set("fo:border", "0.75pt solid #000000");
            gp.set("fo:background-color", "#ffffff");
            gp.set("draw:fill", "solid");
            gp.set("draw:fill-color", "#ffffff");
        }
        let el = El::new("style:style")
            .with("style:family", "graphic")
            .with_child(gp);
        self.autos().add("fr", el)
    }

    fn drawing(&mut self, w: &mut XmlOut, d: &Drawing) {
        let anchor = match &d.placement {
            Placement::Inline => "as-char",
            Placement::Anchor(a) => match (a.relative_h, a.relative_v) {
                (RelativeFromH::Page, RelativeFromV::Page) => "paragraph",
                (RelativeFromH::Character, _) => "char",
                _ => "paragraph",
            },
        };
        let n = self.next("frame");
        let name = if d.name.is_empty() {
            format!("Shape{n}")
        } else {
            d.name.clone()
        };
        let pos = |w: &mut XmlOut| {
            if let Placement::Anchor(a) = &d.placement {
                if let PosH::Offset(x) = a.h {
                    w.attr("svg:x", emu_str(x));
                }
                if let PosV::Offset(y) = a.v {
                    w.attr("svg:y", emu_str(y));
                }
                w.attr("draw:z-index", a.z_order.clamp(0, i64::from(i32::MAX)).to_string());
            }
        };
        let rotate = |w: &mut XmlOut| {
            if d.rotation != 0 {
                let rad = -(f64::from(d.rotation) / 60_000.0).to_radians();
                w.attr("draw:transform", format!("rotate ({rad})"));
            }
        };
        let alt = |w: &mut XmlOut| {
            if let Some(t) = &d.title {
                w.text_element("svg:title", t);
            }
            if let Some(t) = &d.description {
                w.text_element("svg:desc", t);
            }
        };
        match &d.kind {
            DrawingKind::Picture { media, .. } => {
                let Some(path) = self.picture_path(media) else {
                    self.warn("a picture whose image data is missing was not saved");
                    return;
                };
                let style = self.graphic_style(d, true);
                w.start("draw:frame")
                    .attr("draw:style-name", style)
                    .attr("draw:name", &name)
                    .attr("text:anchor-type", anchor);
                pos(w);
                w.attr("svg:width", emu_str(d.width))
                    .attr("svg:height", emu_str(d.height));
                rotate(w);
                if d.decorative {
                    w.attr("loext:decorative", "true");
                }
                w.start("draw:image")
                    .attr("xlink:href", path)
                    .attr("xlink:type", "simple")
                    .attr("xlink:show", "embed")
                    .attr("xlink:actuate", "onLoad")
                    .end();
                alt(w);
                w.end();
            }
            DrawingKind::Shape { preset } if preset == "rect" && d.text_box.is_some() => {
                let style = self.graphic_style(d, true);
                w.start("draw:frame")
                    .attr("draw:style-name", style)
                    .attr("draw:name", &name)
                    .attr("text:anchor-type", anchor);
                pos(w);
                w.attr("svg:width", emu_str(d.width))
                    .attr("svg:height", emu_str(d.height));
                rotate(w);
                w.start("draw:text-box");
                let blocks = d.text_box.clone().unwrap_or_default();
                let saved = std::mem::take(&mut self.comment_spans);
                self.blocks(w, &blocks, None);
                self.comment_spans = saved;
                w.end();
                alt(w);
                w.end();
            }
            DrawingKind::Shape { preset } => {
                let style = self.graphic_style(d, false);
                w.start("draw:custom-shape")
                    .attr("draw:style-name", style)
                    .attr("draw:name", &name)
                    .attr("text:anchor-type", anchor);
                pos(w);
                w.attr("svg:width", emu_str(d.width))
                    .attr("svg:height", emu_str(d.height));
                rotate(w);
                alt(w);
                if let Some(blocks) = d.text_box.clone() {
                    let saved = std::mem::take(&mut self.comment_spans);
                    self.blocks(w, &blocks, None);
                    self.comment_spans = saved;
                }
                crate::deck::shapes::enhanced_geometry(w, preset);
                w.end();
            }
            _ => {
                self.warn("charts, groups and other drawings in text documents are not saved in ODF yet");
            }
        }
    }

    // ------------------------------------------------------------ tables

    #[allow(clippy::too_many_lines)]
    fn table(&mut self, w: &mut XmlOut, t: &Table, master: Option<String>) {
        let n = self.next("table");
        let width = t.grid_width().max(1);
        let mut grid: Vec<i32> = t.grid.clone();
        let avg = if grid.is_empty() {
            1440
        } else {
            let sum: i64 = grid.iter().map(|g| i64::from(*g)).sum();
            i32::try_from(sum / i64::try_from(grid.len()).unwrap_or(1)).unwrap_or(1440)
        };
        grid.resize(width, avg.max(1));
        let total: i64 = grid.iter().map(|g| i64::from((*g).max(0))).sum();
        let tstyle = self.table_style(&t.props, total, master);
        w.start("table:table")
            .attr("table:name", format!("Table{n}"))
            .attr("table:style-name", tstyle);
        for g in &grid {
            let col = El::new("style:style")
                .with("style:family", "table-column")
                .with_child(
                    El::new("style:table-column-properties")
                        .with("style:column-width", twips_str(i64::from((*g).max(1)))),
                );
            let name = self.autos().add("TC", col);
            w.start("table:table-column")
                .attr("table:style-name", name)
                .end();
        }
        let slots = t.cell_map();
        let header_rows = t.header_rows();
        for (ri, row) in t.rows.iter().enumerate() {
            if ri == 0 && header_rows > 0 {
                w.start("table:table-header-rows");
            }
            let mut rp = El::new("style:table-row-properties");
            if let Some(h) = &row.props.height {
                match h.rule {
                    HeightRule::Exact => rp.set("style:row-height", twips_str(i64::from(h.value))),
                    _ => rp.set("style:min-row-height", twips_str(i64::from(h.value))),
                }
            }
            if row.props.cant_split == Some(true) {
                rp.set("fo:keep-together", "always");
            }
            w.start("table:table-row");
            if !rp.is_empty() {
                let st = El::new("style:style")
                    .with("style:family", "table-row")
                    .with_child(rp);
                let name = self.autos().add("TR", st);
                w.attr("table:style-name", name);
            }
            let mut col = 0usize;
            let before = row.props.grid_before.unwrap_or(0) as usize;
            for _ in 0..before.min(width) {
                w.start("table:table-cell").start("text:p").end().end();
                col += 1;
            }
            for (ci, cell) in row.cells.iter().enumerate() {
                let slot = slots.iter().find(|s| s.row == ri && s.cell == ci);
                let span = cell.grid_span.max(1) as usize;
                let continued = cell.v_merge == Some(VMerge::Continue)
                    && slot.is_some_and(|s| s.row_span == 0);
                if continued {
                    for _ in 0..span {
                        w.start("table:covered-table-cell").end();
                    }
                    col += span;
                    continue;
                }
                let style = self.cell_style(&cell.props);
                w.start("table:table-cell");
                if let Some(s) = style {
                    w.attr("table:style-name", s);
                }
                if span > 1 {
                    w.attr("table:number-columns-spanned", span.to_string());
                }
                if let Some(s) = slot
                    && s.row_span > 1
                {
                    w.attr("table:number-rows-spanned", s.row_span.to_string());
                }
                w.attr("office:value-type", "string");
                self.blocks(w, &cell.blocks, None);
                w.end();
                for _ in 1..span {
                    w.start("table:covered-table-cell").end();
                }
                col += span;
            }
            while col < width {
                w.start("table:table-cell").start("text:p").end().end();
                col += 1;
            }
            w.end();
            if ri + 1 == header_rows {
                w.end();
            }
        }
        w.end();
    }

    fn table_style(&mut self, p: &TableProps, total: i64, master: Option<String>) -> String {
        let mut tp = El::new("style:table-properties");
        match &p.width {
            Some(wd) if wd.kind == WidthType::Pct => {
                tp.set("style:width", twips_str(total));
                tp.set("style:rel-width", format!("{}%", wd.value / 50));
            }
            Some(wd) if wd.kind == WidthType::Dxa && wd.value > 0 => {
                tp.set("style:width", twips_str(i64::from(wd.value)));
            }
            _ => tp.set("style:width", twips_str(total)),
        }
        match p.alignment {
            Some(TableAlignment::Center) => tp.set("table:align", "center"),
            Some(TableAlignment::Right) => tp.set("table:align", "right"),
            _ => {
                tp.set("table:align", "left");
                if let Some(i) = p.indent {
                    tp.set("fo:margin-left", twips_str(i64::from(i)));
                }
            }
        }
        if let Some(s) = &p.shading
            && let Some(c) = shading_rgb(s, self.theme)
        {
            tp.set("fo:background-color", color_str(c));
        }
        if p.bidi_visual == Some(true) {
            tp.set("style:writing-mode", "rl-tb");
        }
        let mut st = El::new("style:style").with("style:family", "table");
        if let Some(m) = master {
            st.set("style:master-page-name", m);
        }
        st.children.push(tp);
        self.autos().add("Table", st)
    }

    fn cell_style(&mut self, p: &TableCellProps) -> Option<String> {
        let mut cp = El::new("style:table-cell-properties");
        if let Some(b) = &p.borders {
            for (slot, name) in [
                (&b.top, "fo:border-top"),
                (&b.bottom, "fo:border-bottom"),
                (&b.left, "fo:border-left"),
                (&b.right, "fo:border-right"),
            ] {
                if let Some(border) = slot {
                    cp.set(name, border_str(border, self.theme));
                }
            }
        }
        if let Some(m) = &p.margins {
            for (v, name) in [
                (m.top, "fo:padding-top"),
                (m.bottom, "fo:padding-bottom"),
                (m.left, "fo:padding-left"),
                (m.right, "fo:padding-right"),
            ] {
                if let Some(v) = v {
                    cp.set(name, twips_str(i64::from(v)));
                }
            }
        }
        if let Some(s) = &p.shading
            && let Some(c) = shading_rgb(s, self.theme)
        {
            cp.set("fo:background-color", color_str(c));
        }
        if let Some(v) = p.v_align {
            cp.set(
                "style:vertical-align",
                match v {
                    CellVAlign::Top => "top",
                    CellVAlign::Center => "middle",
                    CellVAlign::Bottom => "bottom",
                },
            );
        }
        if cp.is_empty() {
            return None;
        }
        let st = El::new("style:style")
            .with("style:family", "table-cell")
            .with_child(cp);
        Some(self.autos().add("TCe", st))
    }
}

impl El {
    /// Builder: appends a child.
    #[must_use]
    pub fn with_child(mut self, child: El) -> Self {
        self.children.push(child);
        self
    }
}

fn annotation_name(id: NodeId) -> String {
    format!("__Annotation__{}", id.0)
}

fn change_id(para: NodeId, start: usize, end: usize) -> String {
    format!("ct{}_{start}_{end}", para.0)
}

fn link_start(w: &mut XmlOut, h: &Hyperlink) {
    let href = match &h.target {
        LinkTarget::Url(u) => u.clone(),
        LinkTarget::Bookmark(b) => format!("#{b}"),
    };
    w.start("text:a")
        .attr("xlink:type", "simple")
        .attr("xlink:href", href);
    if let Some(t) = &h.tooltip {
        w.attr("office:title", t);
    }
    if let Some(f) = &h.frame {
        w.attr("office:target-frame-name", f);
    }
}

/// The page setup two sections share when one continues the other on the same page.
fn same_page(a: &SectionProps, b: &SectionProps) -> bool {
    a.page_size == b.page_size && a.margins == b.margins && a.bidi == b.bidi
}

/// A paragraph split at page and column breaks that are not at its start: the text after a
/// break becomes a paragraph that starts on a new page (or column).
fn split_breaks(p: &Paragraph) -> Vec<Paragraph> {
    let cuts: Vec<(usize, BreakKind)> = p
        .content
        .objects_with_offsets()
        .filter_map(|(off, o)| match o {
            InlineObject::Break(b) if off > 0 && b.kind != BreakKind::TextWrapping => {
                Some((off, b.kind))
            }
            _ => None,
        })
        .collect();
    if cuts.is_empty() {
        return vec![p.clone()];
    }
    let len = p.content.len();
    let mut out = Vec::new();
    let mut start = 0;
    let mut kind: Option<BreakKind> = None;
    for (off, k) in cuts.iter().copied().chain(std::iter::once((len, BreakKind::TextWrapping))) {
        // The break itself is dropped (it becomes the next paragraph's property).
        let piece_start = if kind.is_some() { start + 1 } else { start };
        let end = off.max(piece_start);
        let mut q = p.clone();
        if let Ok(f) = p.content.slice(piece_start.min(len), end.min(len)) {
            q.content = kiln_doc::Inlines::from_fragment(f).unwrap_or_default();
        }
        match kind {
            Some(BreakKind::Page) => {
                q.props.page_break_before = Some(true);
                q.props.numbering = None;
            }
            Some(BreakKind::Column) => {
                // Re-insert the column break at the start; the writer turns it into a property.
                let mut f = kiln_doc::Fragment::object(
                    InlineObject::Break(kiln_doc::Break {
                        kind: BreakKind::Column,
                        clear: None,
                    }),
                    RunProps::default(),
                );
                if let Ok(rest) = q.content.slice(0, q.content.len()) {
                    f.text.push_str(&rest.text);
                    f.runs.extend(rest.runs);
                    f.objects.extend(rest.objects);
                    f.marks.extend(rest.marks.into_iter().map(|mut m| {
                        m.start += 1;
                        m.end += 1;
                        m
                    }));
                }
                q.content = kiln_doc::Inlines::from_fragment(f).unwrap_or_default();
                q.props.numbering = None;
            }
            _ => {}
        }
        if kind.is_some() {
            q.id = NodeId(q.id.0 ^ (1 << 62) ^ (off as u64));
        }
        out.push(q);
        start = off;
        kind = Some(k);
    }
    out
}

fn columns_el(c: &Columns) -> El {
    let mut e = El::new("style:columns")
        .with("fo:column-count", c.count.max(1).to_string())
        .with("fo:column-gap", twips_str(i64::from(c.space.max(0))));
    if c.separator {
        e.children.push(
            El::new("style:column-sep")
                .with("style:style", "solid")
                .with("style:width", "0.75pt")
                .with("style:color", "#000000")
                .with("style:height", "100%"),
        );
    }
    if !c.equal_width && !c.cols.is_empty() {
        for col in &c.cols {
            e.children.push(
                El::new("style:column")
                    .with("style:rel-width", format!("{}*", col.width.max(1)))
                    .with("fo:start-indent", "0in")
                    .with("fo:end-indent", twips_str(i64::from(col.space.max(0)))),
            );
        }
    }
    e
}

fn page_layout_el(name: &str, p: &SectionProps, header: bool, footer: bool) -> El {
    let m = &p.margins;
    let mut pl = El::new("style:page-layout-properties")
        .with("fo:page-width", twips_str(i64::from(p.page_size.width)))
        .with("fo:page-height", twips_str(i64::from(p.page_size.height)))
        .with(
            "style:print-orientation",
            if p.page_size.orientation == Orientation::Landscape {
                "landscape"
            } else {
                "portrait"
            },
        );
    let top = m.top.abs();
    let bottom = m.bottom.abs();
    let (mt, hh) = if header {
        (m.header.max(0), (top - m.header.max(0)).max(0))
    } else {
        (top, 0)
    };
    let (mb, fh) = if footer {
        (m.footer.max(0), (bottom - m.footer.max(0)).max(0))
    } else {
        (bottom, 0)
    };
    pl.set("fo:margin-top", twips_str(i64::from(mt)));
    pl.set("fo:margin-bottom", twips_str(i64::from(mb)));
    pl.set("fo:margin-left", twips_str(i64::from(m.left)));
    pl.set("fo:margin-right", twips_str(i64::from(m.right)));
    if m.gutter > 0 {
        pl.set("loext:margin-gutter", twips_str(i64::from(m.gutter)));
    }
    pl.set(
        "style:writing-mode",
        if p.bidi == Some(true) { "rl-tb" } else { "lr-tb" },
    );
    if p.columns.count > 1 {
        pl.children.push(columns_el(&p.columns));
    }
    let mut e = El::new("style:page-layout").with("style:name", name);
    e.children.push(pl);
    if header {
        e.children.push(El::new("style:header-style").with_child(
            El::new("style:header-footer-properties")
                .with("fo:min-height", twips_str(i64::from(hh)))
                .with("fo:margin-bottom", "0in")
                .with("style:dynamic-spacing", "false"),
        ));
    }
    if footer {
        e.children.push(El::new("style:footer-style").with_child(
            El::new("style:header-footer-properties")
                .with("fo:min-height", twips_str(i64::from(fh)))
                .with("fo:margin-top", "0in")
                .with("style:dynamic-spacing", "false"),
        ));
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_are_ncnames() {
        assert_eq!(ncname("Heading1"), "Heading1");
        assert_eq!(ncname("Heading 1"), "Heading_20_1");
        assert_eq!(ncname("1st"), "_31_st");
        assert_eq!(ncname(""), "_5f_");
    }
}
