// SPDX-License-Identifier: AGPL-3.0-or-later
//! A paragraph of the model as text-engine input: styled runs, the mapping between the model's
//! char offsets and the laid-out text's byte offsets, and the inline content the text engine
//! does not know about (pictures in the line, floating objects, note references, page and column
//! breaks, highlights).
//!
//! The model counts chars with one `U+FFFC` per inline object; the laid-out text has list labels
//! in front, field results instead of fields, note numbers instead of references, upper-cased
//! text for caps, and a zero-width placeholder sized like the picture for each inline drawing.
//! [`TextMap`] converts between the two.

use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::fields::{FieldContext, FieldDateTime, FieldKind, evaluate_field};
use kiln_doc::inline::OBJECT_CHAR;
use kiln_doc::numbering::ListLabel;
use kiln_doc::resolve::{
    ResolvedParagraph, ResolvedRun, RunInput, RunTarget, TableCellContext, resolve_run,
};
use kiln_doc::st::{BreakKind, LevelSuffix};
use kiln_doc::{Document, Drawing, InlineObject, Mark, Paragraph, Placement, RunProps};
use kiln_text::{ParagraphSpec, Rgba, SpanStyle, TextRun, UnderlineKind};
use rustc_hash::FxHashMap;

use crate::style::{self, FontSlot, ParaFormat};
use crate::units::emu;

/// The zero-width space that carries an inline object (a break opportunity after it).
const ZWSP: char = '\u{200B}';

/// One stretch of the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seg {
    doc: usize,
    doc_len: usize,
    byte: usize,
    byte_len: usize,
    /// One laid-out char per model char (offsets inside map one to one).
    plain: bool,
}

/// Model char offsets ↔ laid-out byte offsets of one paragraph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextMap {
    segs: Vec<Seg>,
    doc_len: usize,
    text: Arc<str>,
}

impl TextMap {
    /// The laid-out text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Chars in the model paragraph.
    #[must_use]
    pub fn doc_len(&self) -> usize {
        self.doc_len
    }

    /// The byte offset where the model char `doc` starts (the end for `doc_len`).
    #[must_use]
    pub fn byte_of(&self, doc: usize) -> usize {
        let doc = doc.min(self.doc_len);
        let i = self.segs.partition_point(|s| s.doc <= doc);
        // Prefer the segment that starts exactly here (zero-length segments come first).
        let Some(seg) = i.checked_sub(1).map(|i| self.segs[i]) else {
            return self.label_end();
        };
        if doc >= seg.doc + seg.doc_len {
            return seg.byte + seg.byte_len;
        }
        if doc == seg.doc || !seg.plain {
            // Inside an atomic segment: its start.
            return seg.byte;
        }
        let k = doc - seg.doc;
        if seg.byte_len == seg.doc_len {
            return seg.byte + k;
        }
        let slice = &self.text[seg.byte..seg.byte + seg.byte_len];
        seg.byte + slice.char_indices().nth(k).map_or(seg.byte_len, |(b, _)| b)
    }

    fn label_end(&self) -> usize {
        self.segs.first().map_or(self.text.len(), |s| s.byte)
    }

    /// The model offset of a laid-out byte offset: label bytes map to 0, bytes inside an atomic
    /// segment (a field result, a picture) to its nearer edge.
    #[must_use]
    pub fn doc_of(&self, byte: usize) -> usize {
        let i = self.segs.partition_point(|s| s.byte <= byte);
        let Some(seg) = i.checked_sub(1).map(|i| self.segs[i]) else {
            return 0;
        };
        // Skip back over zero-length segments to the one containing the byte.
        if byte >= seg.byte + seg.byte_len {
            return (seg.doc + seg.doc_len).min(self.doc_len);
        }
        if seg.plain {
            let slice = &self.text[seg.byte..byte];
            return seg.doc + slice.chars().count();
        }
        if byte - seg.byte <= seg.byte_len / 2 {
            seg.doc
        } else {
            seg.doc + seg.doc_len
        }
    }

    /// The laid-out byte range of the model char range `[start, end)`.
    #[must_use]
    pub fn byte_range(&self, start: usize, end: usize) -> std::ops::Range<usize> {
        let a = self.byte_of(start);
        let b = self.byte_of(end).max(a);
        a..b
    }
}

/// A picture or shape in the line.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBox {
    /// Model offset of the object.
    pub doc: usize,
    /// The placeholder's span in the spec.
    pub span: usize,
    /// Byte offset of the placeholder.
    pub byte: usize,
    /// Width, px.
    pub width: f32,
    /// Height, px.
    pub height: f32,
    /// The drawing.
    pub drawing: Arc<Drawing>,
}

/// A floating object anchored in the paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorRef {
    /// Model offset of the anchor.
    pub doc: usize,
    /// Laid-out byte offset (for character- and line-relative positions).
    pub byte: usize,
    /// The drawing.
    pub drawing: Arc<Drawing>,
}

/// A note reference in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteMark {
    /// Laid-out byte offset.
    pub byte: usize,
    /// The note.
    pub note: NodeId,
}

/// A forced break inside the paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForcedBreak {
    /// Byte offset just after the break's `\n`.
    pub byte: usize,
    /// Page or column.
    pub column: bool,
}

/// A background behind a byte range (highlight, run shading, commented text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    /// Start byte.
    pub start: usize,
    /// End byte.
    pub end: usize,
    /// Colour.
    pub color: Rgba,
}

/// What a field resolves against while building a paragraph.
#[derive(Debug, Clone, Default)]
pub struct FieldEnv {
    /// The page number as displayed (already formatted with the section's number format).
    pub page: Option<String>,
    /// The page number.
    pub page_number: Option<i64>,
    /// Pages in the document.
    pub num_pages: Option<i64>,
    /// Pages in the section.
    pub section_pages: Option<i64>,
    /// Section number (1-based).
    pub section: Option<i64>,
    /// Now (DATE/TIME).
    pub now: Option<FieldDateTime>,
    /// FILENAME.
    pub file_name: Option<String>,
}

/// A paragraph ready for the text engine.
#[derive(Debug, Clone)]
pub struct ParaSource {
    /// The paragraph.
    pub id: NodeId,
    /// Runs and props for `kiln-text`.
    pub spec: ParagraphSpec,
    /// Offsets.
    pub map: TextMap,
    /// Flow and box properties.
    pub format: ParaFormat,
    /// Pictures in the line.
    pub inline: Vec<InlineBox>,
    /// Floating objects.
    pub anchors: Vec<AnchorRef>,
    /// Note references.
    pub notes: Vec<NoteMark>,
    /// Page and column breaks.
    pub breaks: Vec<ForcedBreak>,
    /// Backgrounds behind text.
    pub highlights: Vec<Highlight>,
    /// Spans painted in the theme's ink (auto colour).
    pub ink_spans: Vec<bool>,
    /// Spans not painted (placeholders).
    pub hidden_spans: Vec<bool>,
    /// Bytes of the list label at the start.
    pub label_len: usize,
    /// Whether the paragraph contains fields that depend on the page (PAGE, NUMPAGES…).
    pub page_fields: bool,
}

/// Resolution caches shared by every paragraph of a layout.
#[derive(Debug, Default)]
pub struct StyleCache {
    runs: FxHashMap<u64, Arc<ResolvedRun>>,
    paragraphs: FxHashMap<u64, Arc<ResolvedParagraph>>,
}

impl StyleCache {
    /// Forgets everything (styles or the theme changed).
    pub fn clear(&mut self) {
        self.runs.clear();
        self.paragraphs.clear();
    }

    /// The effective properties of a paragraph in its table context.
    pub fn paragraph(
        &mut self,
        doc: &Document,
        para: &Paragraph,
        table: Option<&TableCellContext>,
    ) -> Arc<ResolvedParagraph> {
        let key = hash_of(&(
            &para.props,
            table.map(|t| (&t.table_style_id, &t.conditions)),
        ));
        self.paragraphs
            .entry(key)
            .or_insert_with(|| {
                Arc::new(kiln_doc::resolve::resolve_paragraph(
                    &doc.context(),
                    &para.props,
                    table,
                ))
            })
            .clone()
    }

    /// The effective properties of a run (or the list label with `target`).
    pub fn run(
        &mut self,
        doc: &Document,
        para: &Paragraph,
        direct: &RunProps,
        table: Option<&TableCellContext>,
        target: RunTarget,
    ) -> Arc<ResolvedRun> {
        let label = target == RunTarget::ListLabel;
        let key = hash_of(&(
            &para.props.style_id,
            &para.props.numbering,
            if label { Some(&para.mark_props) } else { None },
            direct,
            table.map(|t| (&t.table_style_id, &t.conditions)),
            label,
        ));
        self.runs
            .entry(key)
            .or_insert_with(|| {
                Arc::new(resolve_run(
                    &doc.context(),
                    &RunInput {
                        paragraph: &para.props,
                        mark_props: &para.mark_props,
                        direct,
                        table,
                        target,
                    },
                ))
            })
            .clone()
    }
}

fn hash_of<T: std::hash::Hash>(v: &T) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault};
    BuildHasherDefault::<rustc_hash::FxHasher>::default().hash_one(v)
}

/// Inputs of [`build`].
#[derive(Debug, Clone, Copy)]
pub struct BuildCx<'a> {
    /// The document.
    pub doc: &'a Document,
    /// The table-style context of the paragraph, if it is in a table.
    pub table: Option<&'a TableCellContext>,
    /// The list label.
    pub label: Option<&'a ListLabel>,
    /// Display numbers of notes.
    pub note_numbers: &'a FxHashMap<NodeId, String>,
    /// Field values.
    pub fields: &'a FieldEnv,
    /// Type scale (Read Mode).
    pub scale: f32,
    /// Width of the text area, px.
    pub width: f32,
    /// Show tracked changes (insertions underlined, deletions struck through).
    pub markup: bool,
}

/// The display of a run segment.
#[derive(Clone, PartialEq)]
struct SegStyle {
    run: Arc<ResolvedRun>,
    slot: FontSlot,
    small: bool,
    revision: Revision,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Revision {
    None,
    Inserted(Rgba),
    Deleted(Rgba),
}

/// Author colours of tracked changes (Word's first author colour is a dark red).
const AUTHOR_COLORS: [Rgba; 6] = [
    0xB0_1E_2D_FF,
    0x1F_5F_A8_FF,
    0x2E_7D_32_FF,
    0x8E_24_AA_FF,
    0xC4_65_00_FF,
    0x00_83_8F_FF,
];

fn author_color(author: &str) -> Rgba {
    let h = hash_of(&author);
    #[allow(clippy::cast_possible_truncation)]
    let i = (h % AUTHOR_COLORS.len() as u64) as usize;
    AUTHOR_COLORS[i]
}

/// The comment highlight (Word's pale comment shading).
pub const COMMENT_FILL: Rgba = 0xF7_E3_A3_80;

struct Builder<'a, 'c> {
    cx: BuildCx<'a>,
    cache: &'c mut StyleCache,
    text: String,
    runs: Vec<TextRun>,
    ink: Vec<bool>,
    hidden: Vec<bool>,
    cur: Option<SegStyle>,
    segs: Vec<Seg>,
    out: ParaSourceParts,
}

#[derive(Default)]
struct ParaSourceParts {
    inline: Vec<InlineBox>,
    anchors: Vec<AnchorRef>,
    notes: Vec<NoteMark>,
    breaks: Vec<ForcedBreak>,
    highlights: Vec<Highlight>,
    page_fields: bool,
}

impl Builder<'_, '_> {
    /// Starts a new span with `style` unless the current one has it.
    fn style(&mut self, style: &SegStyle) {
        if self.cur.as_ref() == Some(style) {
            return;
        }
        let mut s = style::span_style(&style.run, style.slot, self.cx.scale);
        if style.small {
            s.size *= 0.8;
        }
        let mut ink = style::is_auto_color(&style.run);
        match style.revision {
            Revision::None => {}
            Revision::Inserted(c) => {
                s.color = c;
                ink = false;
                if s.underline == UnderlineKind::None {
                    s.underline = UnderlineKind::Single;
                }
            }
            Revision::Deleted(c) => {
                s.color = c;
                ink = false;
                s.strike = kiln_text::StrikeKind::Single;
            }
        }
        self.runs.push(TextRun::new(String::new(), s));
        self.ink.push(ink);
        self.hidden.push(false);
        self.cur = Some(style.clone());
    }

    fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
        if let Some(r) = self.runs.last_mut() {
            r.text.push_str(s);
        }
    }

    /// Records a mapping segment.
    fn seg(&mut self, doc: usize, doc_len: usize, byte: usize, plain: bool) {
        let byte_len = self.text.len() - byte;
        if let Some(last) = self.segs.last_mut()
            && plain
            && last.plain
            && last.doc + last.doc_len == doc
            && last.byte + last.byte_len == byte
        {
            last.doc_len += doc_len;
            last.byte_len += byte_len;
            return;
        }
        self.segs.push(Seg {
            doc,
            doc_len,
            byte,
            byte_len,
            plain,
        });
    }
}

/// Field result for display: evaluated when the field depends on the page or the clock, the
/// cached result otherwise (fields update on command).
fn field_text(field: &kiln_doc::Field, cx: &BuildCx<'_>, page_fields: &mut bool) -> String {
    let parsed = field.parsed();
    match &parsed.kind {
        FieldKind::Page(kind) => {
            *page_fields = true;
            if matches!(kind, kiln_doc::fields::PageFieldKind::Page)
                && parsed.general.is_empty()
                && let Some(p) = &cx.fields.page
            {
                return p.clone();
            }
            let ctx = FieldContext {
                page: cx.fields.page_number,
                num_pages: cx.fields.num_pages,
                section_pages: cx.fields.section_pages,
                section: cx.fields.section,
                ..FieldContext::default()
            };
            evaluate_field(&parsed, &ctx).unwrap_or_else(|| field.result.clone())
        }
        FieldKind::Date { .. } if !field.locked => {
            let ctx = FieldContext {
                now: cx.fields.now,
                ..FieldContext::default()
            };
            evaluate_field(&parsed, &ctx).unwrap_or_else(|| field.result.clone())
        }
        _ => match parsed.keyword.to_ascii_uppercase().as_str() {
            "FILENAME" => cx.fields.file_name.clone().map_or_else(
                || field.result.clone(),
                |name| {
                    let with_path = parsed.switches.iter().any(|s| s.name == "\\p");
                    if with_path {
                        name
                    } else {
                        name.rsplit(['/', '\\']).next().unwrap_or(&name).to_owned()
                    }
                },
            ),
            "AUTHOR" => cx
                .doc
                .properties
                .creator
                .clone()
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| field.result.clone()),
            "TITLE" => cx
                .doc
                .properties
                .title
                .clone()
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| field.result.clone()),
            _ => field.result.clone(),
        },
    }
}

/// Text of an equation: the `m:t` runs of its OMML in order.
fn equation_text(omml: &str) -> String {
    let mut out = String::new();
    let mut rest = omml;
    while let Some(i) = rest.find("<m:t") {
        rest = &rest[i + 4..];
        let Some(gt) = rest.find('>') else { break };
        if rest[..gt].ends_with('/') {
            rest = &rest[gt + 1..];
            continue;
        }
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</m:t>") else {
            break;
        };
        out.push_str(&unescape(&rest[..end]));
        rest = &rest[end..];
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Builds the text-engine input of a paragraph.
#[must_use]
pub fn build(cx: BuildCx<'_>, cache: &mut StyleCache, para: &Paragraph) -> ParaSource {
    let resolved = cache.paragraph(cx.doc, para, cx.table);
    let (mut props, format) =
        style::paragraph(&resolved, &cx.doc.settings, &cx.doc.theme, cx.scale);
    let mut b = Builder {
        cx,
        cache,
        text: String::new(),
        runs: Vec::new(),
        ink: Vec::new(),
        hidden: Vec::new(),
        cur: None,
        segs: Vec::new(),
        out: ParaSourceParts::default(),
    };

    // List label.
    if let Some(label) = cx.label {
        let run = b.cache.run(
            cx.doc,
            para,
            &RunProps::default(),
            cx.table,
            RunTarget::ListLabel,
        );
        let mut text = label.text.clone();
        match label.suffix {
            LevelSuffix::Tab => text.push('\t'),
            LevelSuffix::Space => text.push(' '),
            LevelSuffix::Nothing => {}
        }
        let slot = text
            .chars()
            .next()
            .map_or(FontSlot::Ascii, |c| style::font_slot(c, run.rtl));
        b.style(&SegStyle {
            run,
            slot,
            small: false,
            revision: Revision::None,
        });
        b.push_str(&text);
    }
    let label_len = b.text.len();

    let content = &para.content;
    let marks = content.marks();
    let mut obj_index = 0usize;
    let mut doc_off = 0usize;
    let objects = content.objects();
    for (range, direct, run_text) in content.run_texts() {
        let run = b.cache.run(cx.doc, para, direct, cx.table, RunTarget::Text);
        let mut highlight_start: Option<(usize, Option<Rgba>)> = None;
        for (k, ch) in run_text.chars().enumerate() {
            let off = range.start + k;
            doc_off = off + 1;
            // Revisions and comments covering this char.
            let mut revision = Revision::None;
            let mut commented = false;
            for m in marks.iter().filter(|m| m.start <= off && off < m.end) {
                match &m.mark {
                    Mark::Insertion(i) | Mark::MoveTo(i) if cx.markup => {
                        revision = Revision::Inserted(author_color(&i.author));
                    }
                    Mark::Deletion(i) | Mark::MoveFrom(i) => {
                        revision = if cx.markup {
                            Revision::Deleted(author_color(&i.author))
                        } else {
                            Revision::Deleted(0)
                        };
                    }
                    Mark::Comment(_) => commented = true,
                    _ => {}
                }
            }
            let byte = b.text.len();
            if ch == OBJECT_CHAR && obj_index < objects.len() {
                let object = &objects[obj_index];
                obj_index += 1;
                if !cx.markup && matches!(revision, Revision::Deleted(_)) {
                    b.seg(off, 1, byte, false);
                    continue;
                }
                object_text(&mut b, &run, revision, object, off);
                b.seg(off, 1, byte, false);
                continue;
            }
            if run.hidden || (!cx.markup && matches!(revision, Revision::Deleted(_))) {
                b.seg(off, 1, byte, false);
                continue;
            }
            let slot = style::font_slot(ch, run.rtl);
            let mut small = false;
            let mut plain = true;
            let shown: String = if run.caps || run.small_caps {
                small = run.small_caps && !run.caps && ch.is_lowercase();
                let up: String = ch.to_uppercase().collect();
                plain = up.chars().count() == 1;
                up
            } else {
                // Word draws a non-breaking hyphen (U+001E in files) and the optional hyphen.
                match ch {
                    '\u{1E}' => "\u{2011}".to_owned(),
                    '\u{1F}' => "\u{AD}".to_owned(),
                    '\r' => "\n".to_owned(),
                    c => c.to_string(),
                }
            };
            b.style(&SegStyle {
                run: run.clone(),
                slot,
                small,
                revision,
            });
            b.push_str(&shown);
            b.seg(off, 1, byte, plain);
            let bg = run
                .highlight
                .and_then(style::highlight)
                .or_else(|| {
                    style::shading_fill(
                        run.shading.as_ref(),
                        &cx.doc.theme,
                        &cx.doc.settings.color_scheme_mapping,
                    )
                })
                .or(if commented && cx.markup {
                    Some(COMMENT_FILL)
                } else {
                    None
                });
            match highlight_start {
                Some((_, c)) if c == bg => {}
                Some((s, c)) => {
                    b.highlight_range(s, byte, c);
                    highlight_start = Some((byte, bg));
                }
                None => highlight_start = Some((byte, bg)),
            }
        }
        if let Some((s, c)) = highlight_start {
            let end = b.text.len();
            b.highlight_range(s, end, c);
        }
    }
    let doc_len = content.len().max(doc_off);

    // The paragraph mark sizes empty paragraphs and the last line.
    // Right and centre tabs beyond the end of the line align at the end (Word's header and
    // footer styles carry stops at 6.5 in, past narrower text areas).
    let end = (cx.width - props.indent_end).max(0.0);
    for t in &mut props.tab_stops {
        if t.position > end && t.alignment != kiln_text::TabAlignment::Start {
            t.position = end;
        }
    }
    let mark_run = b
        .cache
        .run(cx.doc, para, &para.mark_props, cx.table, RunTarget::Text);
    let mut mark = style::span_style(&mark_run, FontSlot::Ascii, cx.scale);
    mark.baseline_shift = 0.0;
    props.mark_style = Some(mark);

    let text: Arc<str> = Arc::from(b.text.as_str());
    let ParaSourceParts {
        inline,
        anchors,
        notes,
        breaks,
        highlights,
        page_fields,
    } = b.out;
    ParaSource {
        id: para.id,
        spec: ParagraphSpec {
            runs: b.runs,
            props,
            width: cx.width.max(1.0),
        },
        map: TextMap {
            segs: b.segs,
            doc_len,
            text,
        },
        format,
        inline,
        anchors,
        notes,
        breaks,
        highlights,
        ink_spans: b.ink,
        hidden_spans: b.hidden,
        label_len,
        page_fields,
    }
}

impl Builder<'_, '_> {
    fn highlight_range(&mut self, start: usize, end: usize, color: Option<Rgba>) {
        if let Some(color) = color
            && end > start
        {
            if let Some(h) = self.out.highlights.last_mut()
                && h.end == start
                && h.color == color
            {
                h.end = end;
                return;
            }
            self.out.highlights.push(Highlight { start, end, color });
        }
    }
}

/// Appends the laid-out text of an inline object.
fn object_text(
    b: &mut Builder<'_, '_>,
    run: &Arc<ResolvedRun>,
    revision: Revision,
    object: &InlineObject,
    off: usize,
) {
    let text_style = |c: char| SegStyle {
        run: run.clone(),
        slot: style::font_slot(c, run.rtl),
        small: false,
        revision,
    };
    match object {
        InlineObject::Break(br) => {
            b.style(&text_style('\n'));
            b.push_str("\n");
            match br.kind {
                BreakKind::Page => b.out.breaks.push(ForcedBreak {
                    byte: b.text.len(),
                    column: false,
                }),
                BreakKind::Column => b.out.breaks.push(ForcedBreak {
                    byte: b.text.len(),
                    column: true,
                }),
                BreakKind::TextWrapping => {}
            }
        }
        InlineObject::Drawing(d) => match &d.placement {
            Placement::Inline => {
                let width = emu(d.width.clamp(0, 20_116_800)).max(1.0) * b.cx.scale;
                let height = emu(d.height.clamp(0, 20_116_800)).max(1.0) * b.cx.scale;
                // A plain zero-width space first (a break opportunity before the picture), then
                // the placeholder: a 1 px zero-width space raised to the picture's height and
                // widened to its width by letter spacing.
                b.style(&text_style(ZWSP));
                b.push_str("\u{200B}");
                let mut s: SpanStyle = style::span_style(run, FontSlot::Ascii, b.cx.scale);
                s.size = 1.0;
                s.baseline_shift = height;
                s.letter_spacing = width;
                s.underline = UnderlineKind::None;
                s.strike = kiln_text::StrikeKind::None;
                b.runs.push(TextRun::new(String::new(), s));
                b.ink.push(false);
                b.hidden.push(true);
                b.cur = None;
                let byte = b.text.len();
                b.push_str("\u{200B}");
                b.out.inline.push(InlineBox {
                    doc: off,
                    span: b.runs.len() - 1,
                    byte,
                    width,
                    height,
                    drawing: Arc::new((**d).clone()),
                });
            }
            Placement::Anchor(_) => b.out.anchors.push(AnchorRef {
                doc: off,
                byte: b.text.len(),
                drawing: Arc::new((**d).clone()),
            }),
        },
        InlineObject::Field(f) => {
            let mut page_fields = b.out.page_fields;
            let t = field_text(f, &b.cx, &mut page_fields);
            b.out.page_fields = page_fields;
            if let Some(c) = t.chars().next() {
                b.style(&text_style(c));
                b.push_str(&t);
            }
        }
        InlineObject::NoteRef(n) => {
            let t = n
                .custom_mark
                .clone()
                .or_else(|| b.cx.note_numbers.get(&n.note).cloned())
                .unwrap_or_default();
            if !t.is_empty() {
                let mut st = text_style('1');
                if st.run.vert_align == kiln_doc::st::VertAlign::Baseline {
                    let mut r = (*st.run).clone();
                    r.vert_align = kiln_doc::st::VertAlign::Superscript;
                    st.run = Arc::new(r);
                }
                b.out.notes.push(NoteMark {
                    byte: b.text.len(),
                    note: n.note,
                });
                b.style(&st);
                b.push_str(&t);
            }
        }
        InlineObject::Symbol(sym) => {
            let t = kiln_io_docx::symbol_code_to_unicode(&sym.font, sym.code)
                .map_or_else(|| object.plain_text(), String::from);
            if let Some(c) = t.chars().next() {
                b.style(&text_style(c));
                b.push_str(&t);
            }
        }
        InlineObject::Equation(e) => {
            let t = equation_text(&e.omml);
            if !t.is_empty() {
                let mut r = (**run).clone();
                r.fonts.ascii = "Cambria Math".into();
                r.fonts.h_ansi = "Cambria Math".into();
                r.italic = true;
                b.style(&SegStyle {
                    run: Arc::new(r),
                    slot: FontSlot::Ascii,
                    small: false,
                    revision,
                });
                b.push_str(&t);
            }
        }
        InlineObject::BookmarkStart { .. }
        | InlineObject::BookmarkEnd { .. }
        | InlineObject::Opaque { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::{Fragment, InlineObject, Inlines};

    fn para_with(fragments: Vec<Fragment>) -> (Document, Paragraph) {
        let doc = Document::new();
        let mut p = Paragraph::new(NodeId(900));
        let mut content = Inlines::new();
        for f in fragments {
            let at = content.len();
            content.insert(at, f).unwrap();
        }
        p.content = content;
        (doc, p)
    }

    fn src(doc: &Document, p: &Paragraph, label: Option<&ListLabel>) -> ParaSource {
        let notes = FxHashMap::default();
        let env = FieldEnv {
            page: Some("7".into()),
            page_number: Some(7),
            num_pages: Some(12),
            ..FieldEnv::default()
        };
        let mut cache = StyleCache::default();
        build(
            BuildCx {
                doc,
                table: None,
                label,
                note_numbers: &notes,
                fields: &env,
                scale: 1.0,
                width: 600.0,
                markup: true,
            },
            &mut cache,
            p,
        )
    }

    #[test]
    fn maps_plain_text_one_to_one() {
        let (doc, p) = para_with(vec![Fragment::text("héllo wörld", RunProps::default())]);
        let s = src(&doc, &p, None);
        assert_eq!(s.map.text(), "héllo wörld");
        for i in 0..=11 {
            let b = s.map.byte_of(i);
            assert_eq!(s.map.doc_of(b), i, "offset {i}");
        }
        assert_eq!(s.map.byte_of(2), 3);
    }

    #[test]
    fn fields_breaks_and_labels_map_atomically() {
        let (doc, p) = para_with(vec![
            Fragment::text("Page ", RunProps::default()),
            Fragment::object(
                InlineObject::Field(kiln_doc::Field::new("PAGE", "1")),
                RunProps::default(),
            ),
            Fragment::text(" of ", RunProps::default()),
            Fragment::object(
                InlineObject::Field(kiln_doc::Field::new("NUMPAGES", "1")),
                RunProps::default(),
            ),
            Fragment::object(
                InlineObject::Break(kiln_doc::Break {
                    kind: BreakKind::Page,
                    clear: None,
                }),
                RunProps::default(),
            ),
            Fragment::text("x", RunProps::default()),
        ]);
        let s = src(&doc, &p, None);
        assert_eq!(s.map.text(), "Page 7 of 12\nx");
        assert!(s.page_fields);
        assert_eq!(s.breaks.len(), 1);
        assert_eq!(s.map.byte_of(10), 10, "the second field starts at byte 10");
        assert_eq!(
            s.map.doc_of(11),
            10,
            "inside the field maps to the nearer edge"
        );
        assert_eq!(s.map.byte_of(11), 12);
        assert_eq!(s.map.doc_of(13), 12);
        assert_eq!(s.map.byte_of(13), 14);
    }

    #[test]
    fn inline_pictures_become_sized_placeholders() {
        let d = Drawing::picture(NodeId(77), "word/media/a.png", 952_500, 476_250);
        let (doc, p) = para_with(vec![
            Fragment::text("a", RunProps::default()),
            Fragment::object(InlineObject::Drawing(Box::new(d)), RunProps::default()),
            Fragment::text("b", RunProps::default()),
        ]);
        let s = src(&doc, &p, None);
        assert_eq!(s.inline.len(), 1);
        let ib = &s.inline[0];
        assert!((ib.width - 100.0).abs() < 1e-3 && (ib.height - 50.0).abs() < 1e-3);
        assert!(s.hidden_spans[ib.span]);
        assert_eq!(s.map.doc_of(s.map.text().len()), 3);
        assert_eq!(s.map.byte_of(2), s.map.text().len() - 1);
    }

    #[test]
    fn equations_show_their_text() {
        assert_eq!(
            equation_text(
                "<m:oMath><m:r><m:t>x</m:t></m:r><m:r><m:t xml:space=\"preserve\">&lt;2</m:t></m:r></m:oMath>"
            ),
            "x<2"
        );
    }
}
