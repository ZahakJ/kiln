// SPDX-License-Identifier: AGPL-3.0-or-later
//! The PDF writer: every sheet of [`crate::sheet::impose`] becomes a PDF page painted through
//! `krilla`. Text is real text (subset fonts embedded, Unicode mapped, selectable and
//! searchable); paths, images (JPEG passed through, PNG re-compressed), clips, transforms and
//! gradients map one to one; shadows are approximated with translucent layers; on-screen-only
//! ops (caret, selection, guides, squiggles, page shadows) are not printed.
//!
//! Tagged PDF: content is marked against the printable's [`StructTree`]; structure elements
//! appear in the order their content is first painted, headers and footers are pagination
//! artifacts, shading and rules are layout artifacts, copies after the first are artifacts.

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use kiln_common::{KilnError, Result};
use kiln_scene::{
    Bounds, DrawOp, FillRule, Gradient, GradientKind, ImageId, LineStyle, Paint, Palette, PathData,
    Segment, Shadow, ShadowStyle, Stroke,
};
use kiln_text::FontId;
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::paint::{
    Fill, LineCap, LineJoin, LinearGradient, RadialGradient, SpreadMethod, Stop, StrokeDash,
};
use krilla::surface::Surface;
use krilla::tagging::{
    Artifact, ArtifactType, ContentTag, Identifier, Node, SpanTag, Tag, TagGroup, TagKind, TagTree,
};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Document, SerializeSettings};
use rustc_hash::FxHashMap;

use crate::options::PdfOptions;
use crate::printable::{
    ArtifactKind, DocInfo, ImageFormat, LabelStyle, LinkTarget, Mark, OutlineEntry, PageCanvas,
    PrintImage, PrintablePages, StructId, StructKind, StructTree, TextRun,
};
use crate::sheet::{Slot, impose};

/// The producer recorded in every PDF.
pub const PRODUCER: &str = "Kiln";

/// Writes `doc` as PDF.
///
/// # Errors
///
/// `InvalidInput` for invalid options or an empty selection; `Format` when `krilla` fails to
/// serialise (a font it cannot read, an invalid image).
pub fn export(doc: &dyn PrintablePages, options: &PdfOptions) -> Result<Vec<u8>> {
    export_report(doc, options).map(|e| e.pdf)
}

/// A PDF and how it came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    /// The file.
    pub pdf: Vec<u8>,
    /// Whether it is identified as PDF/UA-1 (it passed krilla's PDF/UA-1 checks).
    pub pdf_ua: bool,
    /// Why it is not PDF/UA-1 (missing alt text, glyphs missing from the fonts…), empty when it
    /// is or when tagging was off.
    pub ua_issues: Vec<String>,
}

/// Writes `doc` as PDF; tagged exports are made PDF/UA-1 when the content allows it (every
/// figure has alt text, every glyph exists in its font, …) and stay plain tagged PDF otherwise.
///
/// # Errors
///
/// As [`export`].
pub fn export_report(doc: &dyn PrintablePages, options: &PdfOptions) -> Result<Exported> {
    if options.tagged {
        let ua = krilla::configure::ConfigurationBuilder::new()
            .with_accessibility_validator(krilla::configure::Accessibility::UA1)
            .finish()
            .ok();
        if let Some(configuration) = ua {
            match write(doc, options, configuration) {
                Ok(pdf) => {
                    return Ok(Exported {
                        pdf,
                        pdf_ua: true,
                        ua_issues: Vec::new(),
                    });
                }
                Err(Failure::Validation(issues)) => {
                    let pdf = write(doc, options, krilla::configure::Configuration::default())
                        .map_err(Failure::into_error)?;
                    return Ok(Exported {
                        pdf,
                        pdf_ua: false,
                        ua_issues: issues,
                    });
                }
                Err(e) => return Err(e.into_error()),
            }
        }
    }
    let pdf = write(doc, options, krilla::configure::Configuration::default())
        .map_err(Failure::into_error)?;
    Ok(Exported {
        pdf,
        pdf_ua: false,
        ua_issues: Vec::new(),
    })
}

enum Failure {
    Error(KilnError),
    Validation(Vec<String>),
}

impl Failure {
    fn into_error(self) -> KilnError {
        match self {
            Failure::Error(e) => e,
            Failure::Validation(v) => KilnError::Format {
                format: "pdf".into(),
                message: v.join("; "),
            },
        }
    }
}

impl From<KilnError> for Failure {
    fn from(e: KilnError) -> Self {
        Failure::Error(e)
    }
}

/// A short English description of a validation error.
fn describe(e: &krilla::configure::ValidationError) -> String {
    use krilla::configure::ValidationError as V;
    match e {
        V::ContainsNotDefGlyph(_, _, text) => {
            format!("no font has a glyph for \"{text}\"")
        }
        V::MissingAltText(_) => "a figure has no alt text".into(),
        V::MissingHeadingTitle => "a heading has no text".into(),
        V::MissingAnnotationAltText(_) => "a link has no description".into(),
        V::MissingDocumentOutline => "the document has no bookmarks".into(),
        V::NoDocumentLanguage => "the document has no language".into(),
        V::NoDocumentTitle => "the document has no title".into(),
        other => format!("{other:?}"),
    }
}

fn write(
    doc: &dyn PrintablePages,
    options: &PdfOptions,
    configuration: krilla::configure::Configuration,
) -> std::result::Result<Vec<u8>, Failure> {
    let pages = doc.pages();
    let sheets = impose(&pages, options)?;
    let info = doc.info();
    let tagged = options.tagged;
    let settings = SerializeSettings {
        enable_tagging: tagged,
        xmp_metadata: true,
        configuration,
        ..SerializeSettings::default()
    };
    let mut document = Document::new_with(settings);
    let structure = if tagged {
        doc.structure()
    } else {
        StructTree::default()
    };
    let mut tags = Tagging::new(structure);
    let mut fonts = FontCache::default();
    let mut images: FxHashMap<ImageId, Option<(Image, u32, u32)>> = FxHashMap::default();
    // Where each printed page landed first: (sheet, slot), for links and bookmarks.
    let mut placed: FxHashMap<usize, (usize, Slot)> = FxHashMap::default();
    for (i, sheet) in sheets.iter().enumerate() {
        if sheet.copy == 0 {
            for slot in &sheet.slots {
                placed.entry(slot.page).or_insert((i, *slot));
            }
        }
    }
    let palette = Palette::light();
    for sheet in &sheets {
        let mut settings = krilla::page::PageSettings::new(size(sheet.width, sheet.height));
        if options.per_sheet == 1
            && let Some(slot) = sheet.slots.first()
            && let Some(label) = pages.get(slot.page).and_then(|p| page_label(&p.label))
        {
            settings = settings.with_page_label(label);
        }
        let mut page = document.start_page_with(settings);
        let mut links = Vec::new();
        {
            let mut surface = page.surface();
            for slot in &sheet.slots {
                surface.push_clip_path(
                    &rect_path(Bounds::new(
                        slot.clip[0],
                        slot.clip[1],
                        slot.clip[2],
                        slot.clip[3],
                    )),
                    &krilla::paint::FillRule::NonZero,
                );
                surface.push_transform(&Transform::from_row(
                    slot.scale, 0.0, 0.0, slot.scale, slot.dx, slot.dy,
                ));
                let mut canvas = KrillaCanvas {
                    surface: &mut surface,
                    fonts: &mut fonts,
                    images: &mut images,
                    doc,
                    tags: &mut tags,
                    links: &mut links,
                    slot: *slot,
                    grayscale: options.grayscale,
                    copy: sheet.copy > 0,
                    open: false,
                    palette: &palette,
                };
                doc.paint(slot.page, &mut canvas);
                canvas.close();
                surface.pop();
                surface.pop();
            }
            surface.finish();
        }
        for (rect, target, node) in links {
            let alt = match &target {
                LinkTarget::Uri(u) => format!("Link to {u}"),
                LinkTarget::Page { page: p, .. } => format!("Link to page {}", p + 1),
            };
            let target = match target {
                LinkTarget::Uri(uri) => krilla::annotation::Target::Action(
                    krilla::action::Action::Link(krilla::action::LinkAction::new(uri)),
                ),
                LinkTarget::Page { page: p, y } => {
                    let Some((sheet_index, slot)) = placed.get(&p) else {
                        continue;
                    };
                    let (x, y) = slot.map(0.0, y);
                    krilla::annotation::Target::Destination(krilla::destination::Destination::Xyz(
                        krilla::destination::XyzDestination::new(
                            *sheet_index,
                            Point::from_xy(x, y),
                        ),
                    ))
                }
            };
            let annotation = krilla::annotation::Annotation::new_link(
                krilla::annotation::LinkAnnotation::new(rect, target),
                Some(alt),
            );
            if tagged && sheet.copy == 0 {
                let id = page.add_tagged_annotation(annotation);
                tags.leaf(node, id);
            } else {
                page.add_annotation(annotation);
            }
        }
        page.finish();
    }
    if options.bookmarks {
        let mut entries = doc.outline();
        if entries.is_empty() {
            // A document without headings still gets one bookmark (PDF/UA wants an outline).
            let first = sheets.first().and_then(|s| s.slots.first()).map(|s| s.page);
            if let Some(page) = first {
                let title = options
                    .title
                    .clone()
                    .or_else(|| info.title.clone())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| format!("Page {}", page + 1));
                entries.push(OutlineEntry {
                    level: 1,
                    title,
                    page,
                    y: 0.0,
                });
            }
        }
        if let Some(outline) = outline(&entries, &placed) {
            document.set_outline(outline);
        }
    }
    let title = options.title.clone().or_else(|| info.title.clone());
    document.set_metadata(metadata(&info, title));
    if tagged {
        document.set_tag_tree(tags.finish(info.language.clone()));
    }
    document.finish().map_err(|e| match e {
        krilla::error::KrillaError::Validation(list) => {
            let mut missing = String::new();
            let mut issues: Vec<String> = Vec::new();
            for (v, _) in &list {
                if let krilla::configure::ValidationError::ContainsNotDefGlyph(_, _, text) = v {
                    for c in text.chars() {
                        if !missing.contains(c) {
                            missing.push(c);
                        }
                    }
                } else {
                    issues.push(describe(v));
                }
            }
            if !missing.is_empty() {
                issues.push(format!("no font has glyphs for \"{missing}\""));
            }
            issues.sort();
            issues.dedup();
            Failure::Validation(issues)
        }
        other => Failure::Error(KilnError::Format {
            format: "pdf".into(),
            message: format!("{other:?}"),
        }),
    })
}

fn size(w: f32, h: f32) -> Size {
    Size::from_wh(w.max(1.0), h.max(1.0)).unwrap_or_else(|| {
        // Unreachable for positive finite sizes; Letter as a safe fallback.
        Size::from_wh(612.0, 792.0).unwrap_or_else(|| unreachable!())
    })
}

fn page_label(label: &crate::printable::PageLabel) -> Option<krilla::page::PageLabel> {
    use krilla::page::NumberingStyle as N;
    let style = match label.style {
        LabelStyle::Decimal => Some(N::Arabic),
        LabelStyle::LowerRoman => Some(N::LowerRoman),
        LabelStyle::UpperRoman => Some(N::UpperRoman),
        LabelStyle::LowerLetter => Some(N::LowerAlpha),
        LabelStyle::UpperLetter => Some(N::UpperAlpha),
        LabelStyle::None => None,
    };
    let prefix = (!label.prefix.is_empty()).then(|| label.prefix.clone());
    if style.is_none() && prefix.is_none() {
        return None;
    }
    Some(krilla::page::PageLabel::new(
        style,
        prefix,
        NonZeroU32::new(label.number.max(1)),
    ))
}

fn metadata(info: &DocInfo, title: Option<String>) -> krilla::metadata::Metadata {
    let mut m = krilla::metadata::Metadata::new()
        .producer(PRODUCER.into())
        .creator(info.creator.clone().unwrap_or_else(|| PRODUCER.into()));
    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        m = m.title(t);
    }
    if !info.authors.is_empty() {
        m = m.authors(info.authors.clone());
    }
    if let Some(s) = info.subject.clone().filter(|s| !s.trim().is_empty()) {
        m = m.description(s);
    }
    if !info.keywords.is_empty() {
        m = m.keywords(info.keywords.clone());
    }
    if let Some(l) = info.language.clone().filter(|l| !l.is_empty()) {
        m = m.language(l);
    }
    if let Some(d) = info.created.as_deref().and_then(date_time) {
        m = m.creation_date(d);
    }
    m = m.text_direction(if info.rtl {
        krilla::metadata::TextDirection::RightToLeft
    } else {
        krilla::metadata::TextDirection::LeftToRight
    });
    m
}

/// Parses `YYYY-MM-DDTHH:MM:SSZ` (and its prefixes).
fn date_time(s: &str) -> Option<krilla::metadata::DateTime> {
    let num = |r: std::ops::Range<usize>| s.get(r).and_then(|v| v.parse::<u16>().ok());
    let year = num(0..4)?;
    let mut d = krilla::metadata::DateTime::new(year);
    let byte = |v: u16| u8::try_from(v).ok();
    if let Some(m) = num(5..7).and_then(byte) {
        d = d.month(m);
    }
    if let Some(v) = num(8..10).and_then(byte) {
        d = d.day(v);
    }
    if let Some(v) = num(11..13).and_then(byte) {
        d = d.hour(v);
    }
    if let Some(v) = num(14..16).and_then(byte) {
        d = d.minute(v);
    }
    if let Some(v) = num(17..19).and_then(byte) {
        d = d.second(v);
    }
    if s.ends_with('Z') {
        d = d.utc_offset_hour(0).utc_offset_minute(0);
    }
    Some(d)
}

/// Builds the nested outline from flat entries; entries on pages not printed are left out (their
/// children move up to the nearest printed ancestor).
fn outline(
    entries: &[OutlineEntry],
    placed: &FxHashMap<usize, (usize, Slot)>,
) -> Option<krilla::outline::Outline> {
    use krilla::destination::XyzDestination;
    use krilla::outline::{Outline, OutlineNode};
    // (level, node) stack; nodes are pushed into their parent when they are closed.
    let mut root = Outline::new();
    let mut stack: Vec<(u8, OutlineNode)> = Vec::new();
    let mut any = false;
    let close = |stack: &mut Vec<(u8, OutlineNode)>, root: &mut Outline, level: u8| {
        while stack.last().is_some_and(|(l, _)| *l >= level) {
            let Some((_, node)) = stack.pop() else { break };
            match stack.last_mut() {
                Some((_, parent)) => parent.push_child(node),
                None => root.push_child(node),
            }
        }
    };
    for e in entries {
        let Some((sheet, slot)) = placed.get(&e.page) else {
            continue;
        };
        let title = e.title.trim();
        if title.is_empty() {
            continue;
        }
        let level = e.level.max(1);
        close(&mut stack, &mut root, level);
        let (x, y) = slot.map(0.0, e.y);
        let node = OutlineNode::new(
            title.to_owned(),
            XyzDestination::new(*sheet, Point::from_xy(x, y)),
        );
        stack.push((level, node));
        any = true;
    }
    close(&mut stack, &mut root, 0);
    any.then_some(root)
}

/// Structure bookkeeping while pages are painted.
struct Tagging {
    tree: StructTree,
    children: Vec<Vec<Child>>,
    attached: Vec<bool>,
    root: Vec<Child>,
}

enum Child {
    Node(StructId),
    Leaf(Identifier),
}

impl Tagging {
    fn new(tree: StructTree) -> Self {
        let n = tree.nodes.len();
        Self {
            tree,
            children: (0..n).map(|_| Vec::new()).collect(),
            attached: vec![false; n],
            root: Vec::new(),
        }
    }

    fn valid(&self, id: StructId) -> bool {
        (id.0 as usize) < self.tree.nodes.len()
    }

    /// Attaches `id` and its unattached ancestors to their parents (at the end: reading order is
    /// the order of first use).
    fn attach(&mut self, id: StructId) {
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(i) = at {
            let idx = i.0 as usize;
            if idx >= self.attached.len() || self.attached[idx] {
                break;
            }
            chain.push(i);
            at = self.tree.nodes[idx].parent.filter(|p| p.0 < i.0);
        }
        for &i in chain.iter().rev() {
            let idx = i.0 as usize;
            self.attached[idx] = true;
            match self.tree.nodes[idx].parent.filter(|p| p.0 < i.0) {
                Some(p) => self.children[p.0 as usize].push(Child::Node(i)),
                None => self.root.push(Child::Node(i)),
            }
        }
    }

    fn leaf(&mut self, id: StructId, leaf: Identifier) {
        if !self.valid(id) {
            self.root.push(Child::Leaf(leaf));
            return;
        }
        self.attach(id);
        self.children[id.0 as usize].push(Child::Leaf(leaf));
    }

    fn kind(kind: &StructKind) -> TagKind {
        let span = |n: u32| NonZeroU32::new(n.max(1));
        match kind {
            StructKind::Section => Tag::Section.into(),
            StructKind::Div => Tag::Div.into(),
            StructKind::P => Tag::P.into(),
            StructKind::Heading { level, title } => Tag::Hn(
                NonZeroU16::new(u16::from((*level).clamp(1, 6))).unwrap_or(NonZeroU16::MIN),
                Some(title.clone()),
            )
            .into(),
            StructKind::Table => Tag::Table.into(),
            StructKind::TableRow => Tag::TR.into(),
            StructKind::TableHeader { rows, cols } => {
                Tag::TH(krilla::tagging::TableHeaderScope::Column)
                    .with_row_span(span(*rows).filter(|n| n.get() > 1))
                    .with_col_span(span(*cols).filter(|n| n.get() > 1))
                    .into()
            }
            StructKind::TableCell { rows, cols } => Tag::TD
                .with_row_span(span(*rows).filter(|n| n.get() > 1))
                .with_col_span(span(*cols).filter(|n| n.get() > 1))
                .into(),
            StructKind::Figure { alt } => Tag::Figure(alt.clone()).into(),
            StructKind::Link => Tag::Link.into(),
            StructKind::Note => Tag::Note.into(),
            StructKind::List => Tag::L(krilla::tagging::ListNumbering::None).into(),
            StructKind::ListItem => Tag::LI.into(),
            StructKind::Toc => Tag::TOC.into(),
            StructKind::TocItem => Tag::TOCI.into(),
        }
    }

    /// The tag tree. Children always have larger ids than their parents, so groups are built
    /// from the last node to the first without recursion.
    fn finish(mut self, lang: Option<String>) -> TagTree {
        let n = self.tree.nodes.len();
        let mut groups: Vec<Option<TagGroup>> = (0..n).map(|_| None).collect();
        for i in (0..n).rev() {
            if !self.attached[i] {
                continue;
            }
            let mut group = TagGroup::new(Self::kind(&self.tree.nodes[i].kind));
            for child in std::mem::take(&mut self.children[i]) {
                match child {
                    Child::Leaf(id) => group.push(id),
                    Child::Node(c) => {
                        if let Some(g) = groups.get_mut(c.0 as usize).and_then(Option::take) {
                            group.push(g);
                        }
                    }
                }
            }
            groups[i] = Some(group);
        }
        let mut tree = TagTree::new().with_lang(lang);
        for child in self.root {
            match child {
                Child::Leaf(id) => tree.push(Node::from(id)),
                Child::Node(c) => {
                    if let Some(g) = groups.get_mut(c.0 as usize).and_then(Option::take) {
                        tree.push(g);
                    }
                }
            }
        }
        tree
    }
}

/// krilla fonts per Kiln font instance.
#[derive(Default)]
struct FontCache {
    fonts: FxHashMap<FontId, Option<Font>>,
}

impl FontCache {
    fn get(&mut self, fonts: &kiln_text::FontStore, id: FontId) -> Option<Font> {
        self.fonts
            .entry(id)
            .or_insert_with(|| {
                let inst = fonts.font(id)?;
                let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(inst.data.clone());
                let coords = variation_coords(inst.bytes(), inst.index, &inst.coords);
                Font::new_variable(krilla::Data::from(data), inst.index, &coords)
            })
            .clone()
    }
}

/// Normalised 2.14 coordinates back to user-space axis values (linear, before `avar`).
fn variation_coords(data: &[u8], index: u32, coords: &[i16]) -> Vec<(krilla::text::Tag, f32)> {
    use skrifa::MetadataProvider;
    if coords.is_empty() {
        return Vec::new();
    }
    let Ok(font) = skrifa::FontRef::from_index(data, index) else {
        return Vec::new();
    };
    font.axes()
        .iter()
        .zip(coords)
        .map(|(axis, &c)| {
            let n = f32::from(c) / 16384.0;
            let (min, def, max) = (axis.min_value(), axis.default_value(), axis.max_value());
            let v = if n >= 0.0 {
                def + n * (max - def)
            } else {
                def + n * (def - min)
            };
            (krilla::text::Tag::new(&axis.tag().to_be_bytes()), v)
        })
        .collect()
}

/// The page canvas over a krilla surface (already transformed to page px).
struct KrillaCanvas<'a, 'b> {
    surface: &'a mut Surface<'b>,
    fonts: &'a mut FontCache,
    images: &'a mut FxHashMap<ImageId, Option<(Image, u32, u32)>>,
    doc: &'a dyn PrintablePages,
    tags: &'a mut Tagging,
    links: &'a mut Vec<(Rect, LinkTarget, StructId)>,
    slot: Slot,
    grayscale: bool,
    /// A later copy: everything is an artifact.
    copy: bool,
    open: bool,
    palette: &'a Palette,
}

impl KrillaCanvas<'_, '_> {
    fn close(&mut self) {
        if self.open {
            self.surface.end_tagged();
            self.open = false;
        }
    }

    fn rgba(&self, paint: Paint) -> (rgb::Color, NormalizedF32) {
        let [r, g, b, a] = self.palette.resolve(paint).to_be_bytes();
        let (r, g, b) = if self.grayscale {
            let l = luma(r, g, b);
            (l, l, l)
        } else {
            (r, g, b)
        };
        (
            rgb::Color::new(r, g, b),
            NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE),
        )
    }

    fn fill(&self, paint: Paint, rule: FillRule) -> Fill {
        let (color, opacity) = self.rgba(paint);
        Fill {
            paint: color.into(),
            opacity,
            rule: match rule {
                FillRule::NonZero => krilla::paint::FillRule::NonZero,
                FillRule::EvenOdd => krilla::paint::FillRule::EvenOdd,
            },
        }
    }

    fn stroke(&self, s: &Stroke) -> krilla::paint::Stroke {
        let (color, opacity) = self.rgba(s.paint);
        krilla::paint::Stroke {
            paint: color.into(),
            width: s.width.max(0.0),
            miter_limit: 4.0,
            line_cap: match s.cap {
                kiln_scene::LineCap::Butt => LineCap::Butt,
                kiln_scene::LineCap::Round => LineCap::Round,
                kiln_scene::LineCap::Square => LineCap::Square,
            },
            line_join: match s.join {
                kiln_scene::LineJoin::Miter => LineJoin::Miter,
                kiln_scene::LineJoin::Round => LineJoin::Round,
                kiln_scene::LineJoin::Bevel => LineJoin::Bevel,
            },
            opacity,
            dash: (!s.dash.is_empty() && s.dash.iter().all(|d| d.is_finite() && *d >= 0.0)).then(
                || StrokeDash {
                    array: s.dash.clone(),
                    offset: 0.0,
                },
            ),
        }
    }

    fn fill_path(&mut self, path: &Path, fill: Fill) {
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(fill));
        self.surface.draw_path(path);
    }

    fn stroke_path(&mut self, path: &Path, stroke: krilla::paint::Stroke) {
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(stroke));
        self.surface.draw_path(path);
        self.surface.set_stroke(None);
    }

    /// Content outside any mark becomes a layout artifact (everything is tagged).
    fn ensure_marked(&mut self) {
        if !self.open {
            self.begin(Mark::Artifact(ArtifactKind::Layout));
        }
    }

    fn image(&mut self, id: ImageId, src: Bounds, dst: Bounds, opacity: f32) {
        let gray = self.grayscale;
        let doc = self.doc;
        let entry = self
            .images
            .entry(id)
            .or_insert_with(|| {
                let img = doc.image(id)?;
                Some((krilla_image(&img, gray)?, img.width, img.height))
            })
            .clone();
        let Some((image, w, h)) = entry else {
            return;
        };
        // Source rectangles are in the decoded picture's pixels; a passthrough image may be
        // larger, so work in fractions of the picture.
        #[allow(clippy::cast_precision_loss)]
        let (dw, dh) = (w.max(1) as f32, h.max(1) as f32);
        let (fx, fy, fw, fh) = (src.x / dw, src.y / dh, src.w / dw, src.h / dh);
        if !(fw > 0.0 && fh > 0.0 && dst.w > 0.0 && dst.h > 0.0) {
            return;
        }
        // The whole image is drawn so that the source fraction lands on `dst`.
        let (sx, sy) = (dst.w / fw, dst.h / fh);
        let cropped = fx > 0.0 || fy > 0.0 || fw < 1.0 || fh < 1.0;
        if cropped {
            self.surface
                .push_clip_path(&rect_path(dst), &krilla::paint::FillRule::NonZero);
        }
        let alpha = opacity.clamp(0.0, 1.0);
        if alpha < 1.0 {
            self.surface
                .push_opacity(NormalizedF32::new(alpha).unwrap_or(NormalizedF32::ONE));
        }
        self.surface
            .push_transform(&Transform::from_translate(dst.x - fx * sx, dst.y - fy * sy));
        if let Some(size) = Size::from_wh(sx, sy) {
            self.surface.draw_image(image, size);
        }
        self.surface.pop();
        if alpha < 1.0 {
            self.surface.pop();
        }
        if cropped {
            self.surface.pop();
        }
    }

    fn gradient(&mut self, rect: Bounds, radius: f32, gradient: &Gradient) {
        let stops: Vec<Stop> = gradient
            .stops
            .iter()
            .map(|s| {
                let (color, opacity) = self.rgba(s.paint);
                Stop {
                    offset: NormalizedF32::new(s.offset.clamp(0.0, 1.0))
                        .unwrap_or(NormalizedF32::ZERO),
                    color: color.into(),
                    opacity,
                }
            })
            .collect();
        if stops.is_empty() {
            return;
        }
        let paint: krilla::paint::Paint = match gradient.kind {
            GradientKind::Linear { x0, y0, x1, y1 } => LinearGradient {
                x1: x0,
                y1: y0,
                x2: x1,
                y2: y1,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops,
                anti_alias: false,
            }
            .into(),
            GradientKind::Radial { cx, cy, r } => RadialGradient {
                fx: cx,
                fy: cy,
                fr: 0.0,
                cx,
                cy,
                cr: r.max(0.01),
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops,
                anti_alias: false,
            }
            .into(),
        };
        if let Some(path) = rounded_rect(rect, radius) {
            self.fill_path(
                &path,
                Fill {
                    paint,
                    opacity: NormalizedF32::ONE,
                    rule: krilla::paint::FillRule::NonZero,
                },
            );
        }
    }

    /// A drop shadow as stacked translucent layers (PDF has no blur).
    fn shadow(&mut self, s: &Shadow) {
        let ShadowStyle::Drop {
            dx,
            dy,
            blur,
            spread,
            paint,
        } = s.style
        else {
            // The page shadow belongs to the screen, not the paper.
            return;
        };
        let (color, opacity) = self.rgba(paint);
        let base = s.rect.translate(dx, dy).inflate(spread, spread);
        let steps = 6u8;
        for k in 0..steps {
            let t = f32::from(k) / f32::from(steps - 1);
            let grow = blur * (t - 0.5);
            let layer_opacity = opacity.get() / f32::from(steps);
            if let Some(path) = rounded_rect(base.inflate(grow, grow), s.radius + grow.max(0.0)) {
                self.fill_path(
                    &path,
                    Fill {
                        paint: color.into(),
                        opacity: NormalizedF32::new(layer_opacity).unwrap_or(NormalizedF32::ZERO),
                        rule: krilla::paint::FillRule::NonZero,
                    },
                );
            }
        }
    }
}

impl PageCanvas for KrillaCanvas<'_, '_> {
    fn op(&mut self, op: &DrawOp) {
        match op {
            DrawOp::FillRect {
                rect,
                paint,
                radius,
            } => {
                if let Some(path) = rounded_rect(*rect, *radius) {
                    self.ensure_marked();
                    let fill = self.fill(*paint, FillRule::NonZero);
                    self.fill_path(&path, fill);
                }
            }
            DrawOp::StrokeRect {
                rect,
                paint,
                width,
                radius,
            } => {
                if let Some(path) = rounded_rect(*rect, *radius) {
                    self.ensure_marked();
                    let stroke = self.stroke(&Stroke::solid(*paint, *width));
                    self.stroke_path(&path, stroke);
                }
            }
            DrawOp::Path {
                path,
                fill,
                stroke,
                rule,
            } => {
                let Some(p) = to_path(path) else { return };
                self.ensure_marked();
                if let Some(f) = fill {
                    let fill = self.fill(*f, *rule);
                    self.fill_path(&p, fill);
                }
                if let Some(s) = stroke {
                    let stroke = self.stroke(s);
                    self.stroke_path(&p, stroke);
                }
            }
            DrawOp::OutlineGlyphs { path, paint } => {
                if let Some(p) = to_path(path) {
                    self.ensure_marked();
                    let fill = self.fill(*paint, FillRule::NonZero);
                    self.fill_path(&p, fill);
                }
            }
            DrawOp::Image {
                image,
                src,
                dst,
                opacity,
            } => {
                self.ensure_marked();
                self.image(*image, *src, *dst, *opacity);
            }
            DrawOp::PushClip { rect, path } => {
                let p = path
                    .as_ref()
                    .and_then(to_path)
                    .or_else(|| rounded_rect(*rect, 0.0));
                match p {
                    Some(p) => self
                        .surface
                        .push_clip_path(&p, &krilla::paint::FillRule::NonZero),
                    // An empty clip hides everything until it is popped.
                    None => self.surface.push_clip_path(
                        &rect_path(Bounds::new(0.0, 0.0, 0.0, 0.0)),
                        &krilla::paint::FillRule::NonZero,
                    ),
                }
            }
            DrawOp::PushTransform { matrix } => {
                let [a, b, c, d, e, f] = *matrix;
                self.surface
                    .push_transform(&Transform::from_row(a, b, c, d, e, f));
            }
            DrawOp::PopClip | DrawOp::PopTransform => self.surface.pop(),
            DrawOp::Gradient {
                rect,
                radius,
                gradient,
            } => {
                self.ensure_marked();
                self.gradient(*rect, *radius, gradient);
            }
            DrawOp::Shadow(s) => {
                self.ensure_marked();
                self.shadow(s);
            }
            DrawOp::Underline {
                x,
                y,
                width,
                thickness,
                paint,
                style,
            } => {
                self.ensure_marked();
                match style {
                    LineStyle::Solid => {
                        if let Some(path) =
                            rounded_rect(Bounds::new(*x, *y, *width, *thickness), 0.0)
                        {
                            let fill = self.fill(*paint, FillRule::NonZero);
                            self.fill_path(&path, fill);
                        }
                    }
                    LineStyle::Dotted | LineStyle::Dashed => {
                        let mut p = PathData::new();
                        let cy = y + thickness / 2.0;
                        p.move_to(*x, cy).line_to(x + width, cy);
                        let unit = thickness.max(0.75);
                        let dash = if *style == LineStyle::Dotted {
                            vec![unit, unit * 2.0]
                        } else {
                            vec![unit * 4.0, unit * 3.0]
                        };
                        if let Some(path) = to_path(&p) {
                            let stroke = self.stroke(&Stroke {
                                dash,
                                ..Stroke::solid(*paint, *thickness)
                            });
                            self.stroke_path(&path, stroke);
                        }
                    }
                }
            }
            // Text arrives as `text`; the rest is on-screen only.
            DrawOp::Glyphs(_)
            | DrawOp::Squiggle { .. }
            | DrawOp::Caret { .. }
            | DrawOp::SelectionRects { .. }
            | DrawOp::Guide { .. }
            | DrawOp::Handle { .. } => {}
        }
    }

    fn text(&mut self, run: &TextRun<'_>) {
        let glyphs = &run.run.glyphs;
        let range = run.glyphs.start.min(glyphs.len())..run.glyphs.end.min(glyphs.len());
        if range.is_empty() || run.run.size <= 0.0 {
            return;
        }
        let Some(font) = self.fonts.get(run.fonts, run.run.font) else {
            return;
        };
        self.ensure_marked();
        let size = run.run.size;
        let text = run.actual.unwrap_or(run.text);
        let clusters = cluster_ends(run.run, run.text.len());
        let first = glyphs[range.start];
        let baseline = first.y;
        let mut out = Vec::with_capacity(range.len());
        for i in range.clone() {
            let g = glyphs[i];
            let next_x = if i + 1 < range.end {
                glyphs[i + 1].x
            } else {
                g.x + g.advance
            };
            let text_range = if run.actual.is_some() {
                0..text.len()
            } else {
                let start = (g.cluster as usize).min(run.text.len());
                let end = clusters
                    .iter()
                    .find(|&&c| c > start)
                    .copied()
                    .unwrap_or(run.run.text_range.end)
                    .clamp(start, run.text.len());
                start..end
            };
            out.push(KrillaGlyph::new(
                GlyphId::new(g.id),
                (next_x - g.x) / size,
                0.0,
                (baseline - g.y) / size,
                0.0,
                text_range,
                None,
            ));
        }
        let inst = run.fonts.font(run.run.font);
        let embolden = inst.is_some_and(|i| i.embolden);
        let skew = inst.and_then(|i| i.skew).filter(|a| a.abs() > 0.01);
        let (color, opacity) = self.rgba(run.paint);
        let start = Point::from_xy(run.origin.0 + first.x, run.origin.1 + baseline);
        if let Some(angle) = skew {
            let t = angle.to_radians().tan();
            let y0 = start.y;
            self.surface
                .push_transform(&Transform::from_row(1.0, 0.0, -t, 1.0, t * y0, 0.0));
        }
        self.surface.set_fill(Some(Fill {
            paint: color.into(),
            opacity,
            rule: krilla::paint::FillRule::NonZero,
        }));
        self.surface
            .set_stroke(embolden.then(|| krilla::paint::Stroke {
                paint: color.into(),
                width: size / 48.0,
                opacity,
                ..krilla::paint::Stroke::default()
            }));
        self.surface
            .draw_glyphs(start, &out, font, text, size, false);
        self.surface.set_stroke(None);
        if skew.is_some() {
            self.surface.pop();
        }
    }

    fn begin(&mut self, mark: Mark) {
        self.close();
        let tag = if self.copy {
            None
        } else {
            match mark {
                Mark::Content(id) if self.tags.valid(id) => Some(id),
                _ => None,
            }
        };
        match (tag, mark) {
            (Some(id), _) => {
                let leaf = self
                    .surface
                    .start_tagged(ContentTag::Span(SpanTag::empty()));
                self.tags.leaf(id, leaf);
            }
            (None, Mark::Artifact(kind)) => {
                let kind = match kind {
                    ArtifactKind::Header => ArtifactType::Header,
                    ArtifactKind::Footer => ArtifactType::Footer,
                    ArtifactKind::Layout => ArtifactType::Layout,
                };
                self.surface
                    .start_tagged(ContentTag::Artifact(Artifact::new(kind, None)));
            }
            (None, Mark::Content(_)) => {
                self.surface
                    .start_tagged(ContentTag::Artifact(Artifact::new(
                        ArtifactType::Other,
                        None,
                    )));
            }
        }
        self.open = true;
    }

    fn end(&mut self) {
        self.close();
    }

    fn link(&mut self, rect: Bounds, target: &LinkTarget, node: StructId) {
        let (x0, y0) = self.slot.map(rect.x, rect.y);
        let (x1, y1) = self.slot.map(rect.right(), rect.bottom());
        if let Some(r) = Rect::from_ltrb(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
            && !self.copy
        {
            self.links.push((r, target.clone(), node));
        }
    }
}

/// Sorted cluster starts of a run (each glyph's text ends where the next cluster starts).
fn cluster_ends(run: &kiln_text::GlyphRun, len: usize) -> Vec<usize> {
    let mut v: Vec<usize> = run
        .glyphs
        .iter()
        .map(|g| (g.cluster as usize).min(len))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn luma(r: u8, g: u8, b: u8) -> u8 {
    let l = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let l = l.round().clamp(0.0, 255.0) as u8;
    l
}

/// The krilla image of a print image: JPEG passed through, PNG re-compressed from source, the
/// rest (and every grayscale image) from the decoded pixels.
fn krilla_image(img: &PrintImage, gray: bool) -> Option<Image> {
    if !gray && let Some(src) = &img.source {
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = src.data.clone();
        let made = match src.format {
            ImageFormat::Jpeg => Image::from_jpeg(krilla::Data::from(data), true),
            ImageFormat::Png => Image::from_png(krilla::Data::from(data), true),
        };
        if let Ok(image) = made {
            return Some(image);
        }
    }
    let expected = (img.width as usize)
        .checked_mul(img.height as usize)?
        .checked_mul(4)?;
    if img.width == 0 || img.height == 0 || img.rgba.len() != expected {
        return None;
    }
    let mut rgba = img.rgba.as_ref().clone();
    if gray {
        for px in rgba.as_chunks_mut::<4>().0 {
            let l = luma(px[0], px[1], px[2]);
            px[0] = l;
            px[1] = l;
            px[2] = l;
        }
    }
    Some(Image::from_rgba8(rgba, img.width, img.height))
}

fn rect_path(b: Bounds) -> Path {
    let mut pb = PathBuilder::new();
    if let Some(r) = Rect::from_xywh(b.x, b.y, b.w.max(0.0), b.h.max(0.0)) {
        pb.push_rect(r);
    } else {
        pb.move_to(b.x, b.y);
        pb.line_to(b.x, b.y);
        pb.close();
    }
    pb.finish().unwrap_or_else(|| {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(0.0, 0.0);
        pb.line_to(0.0, 0.0);
        pb.close();
        // A degenerate triangle always builds.
        pb.finish()
            .unwrap_or_else(|| unreachable!("a closed three-point path builds"))
    })
}

/// A (rounded) rectangle; `None` for empty or non-finite ones.
fn rounded_rect(b: Bounds, radius: f32) -> Option<Path> {
    if !(b.is_finite() && b.w > 0.0 && b.h > 0.0) {
        return None;
    }
    let r = radius.max(0.0).min(b.w / 2.0).min(b.h / 2.0);
    if r <= 0.0 {
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(b.x, b.y, b.w, b.h)?);
        return pb.finish();
    }
    // Circular corners with cubic arcs (κ = 0.5523).
    let k = r * 0.552_284_8;
    let (x0, y0, x1, y1) = (b.x, b.y, b.right(), b.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

fn to_path(p: &PathData) -> Option<Path> {
    if p.is_empty() || !p.is_valid() {
        return None;
    }
    let mut pb = PathBuilder::new();
    for s in p.segments() {
        match s {
            Segment::Move([x, y]) => pb.move_to(x, y),
            Segment::Line([x, y]) => pb.line_to(x, y),
            Segment::Quad([cx, cy, x, y]) => pb.quad_to(cx, cy, x, y),
            Segment::Cubic([a, b, c, d, x, y]) => pb.cubic_to(a, b, c, d, x, y),
            Segment::Close => pb.close(),
        }
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_parse_partially() {
        assert!(date_time("2026-10-17T09:30:00Z").is_some());
        assert!(date_time("2026").is_some());
        assert!(date_time("x").is_none());
    }

    #[test]
    fn rectangles_reject_empty_and_round_corners() {
        assert!(rounded_rect(Bounds::new(0.0, 0.0, 0.0, 5.0), 0.0).is_none());
        assert!(rounded_rect(Bounds::new(0.0, 0.0, f32::NAN, 5.0), 0.0).is_none());
        assert!(rounded_rect(Bounds::new(0.0, 0.0, 10.0, 5.0), 3.0).is_some());
        assert!(to_path(&PathData::new()).is_none());
    }

    #[test]
    fn page_labels_map_styles() {
        let l = crate::printable::PageLabel {
            style: LabelStyle::None,
            prefix: String::new(),
            number: 1,
        };
        assert!(page_label(&l).is_none());
        let l = crate::printable::PageLabel {
            style: LabelStyle::LowerRoman,
            prefix: String::new(),
            number: 4,
        };
        assert!(page_label(&l).is_some());
    }
}
