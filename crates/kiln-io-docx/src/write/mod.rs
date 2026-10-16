// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`write_docx`]: the Write model ([`kiln_doc::Document`]) → a WordprocessingML package
//! (`.docx`, `.docm`, `.dotx`, `.dotm`), the inverse of [`crate::read`] (PLAN.md §3.10).
//!
//! - Every construct the reader models is written back in Office's element order; raw XML the
//!   reader kept for what the model does not cover (unmodelled properties, drawings, content
//!   control properties, styles and list definitions as read) is merged back in place.
//! - Parts and relationships the reader did not model (the passthrough: custom XML, macros,
//!   glossary, fonts, charts, thumbnails, unknown parts) are copied with their relationships
//!   repaired, before anything else is allocated, so relationship ids that kept raw XML refers to
//!   stay valid.
//! - With the original package at hand, parts nobody edited ([`TouchedParts`]) are copied byte
//!   for byte, and so is every relationships part whose relationships did not change.
//! - Content types and relationships are regenerated deterministically: the same model and
//!   options give identical bytes. The only time-dependent values (`dcterms:modified`, the
//!   revision count, the application name) are set from [`WriteOptions::now`] when given.
//!
//! [`compat_report`] lists what saving will lose or change (the Compatibility Checker).

mod compat;
mod context;
mod drawing;
mod parts;
mod props;
mod sdt;
mod story;
mod touched;
mod xml;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_doc::st::{HeaderFooterKind, NoteKind, NoteType};
use kiln_doc::{Block, DocError, Document, InlineObject, NodeId};
use kiln_ooxml::opc::{
    OpcError, OpcPackage, content_type, directory_of, is_rels_part, part_key, rel_type,
    rel_type_matches, source_part_of_rels, validate_part_name,
};
use kiln_ooxml::passthrough::PassthroughSet;
use kiln_ooxml::props::{
    AppProperties, CoreProperties, CustomProperty, CustomPropertyValue, DocumentProperties,
    PropertiesUpdate, write_document_properties,
};
use kiln_ooxml::xml::XmlElement;

pub use compat::{CompatIssue, CompatOptions, CompatOrigin, CompatSeverity, compat_report};
pub use context::WriteIssue;
pub use touched::TouchedParts;

use crate::common::extras::{DocxExtras, Person};
use crate::read::{DocxKind, DocxRead, RelTarget};
use context::WriteContext;
use parts::{CommentEx, SettingsInput, make_hex_id};
use props::{HeaderFooterRef, section_props_xml};
use story::{BlockOpts, StoryIds, StoryWriter, separator_run};

/// Why a document could not be written.
#[derive(Debug, thiserror::Error)]
pub enum DocxWriteError {
    /// The model breaks an invariant ([`Document::validate`]); nothing was written.
    #[error("the document is not consistent: {0}")]
    InvalidDocument(#[from] DocError),
    /// The package could not be assembled.
    #[error(transparent)]
    Package(#[from] OpcError),
}

/// What the reader returned beside the document, for a faithful save.
#[derive(Debug, Clone, Copy)]
pub struct WriteSource<'a> {
    /// Parts and relationships the reader did not model.
    pub passthrough: &'a PassthroughSet,
    /// Raw XML and ids the model does not hold.
    pub extras: &'a DocxExtras,
    /// Document properties as read.
    pub package_properties: &'a DocumentProperties,
    /// Relationship id → target per source part, as read.
    pub relationships: &'a BTreeMap<String, BTreeMap<String, RelTarget>>,
    /// The main part name as read.
    pub main_part: &'a str,
    /// The format read.
    pub kind: DocxKind,
    /// The original package: lets the writer copy untouched parts and relationships byte for
    /// byte, and keep the theme and `w:docDefaults` exactly.
    pub package: Option<&'a OpcPackage>,
}

impl<'a> WriteSource<'a> {
    /// The side data of a read.
    #[must_use]
    pub fn from_read(read: &'a DocxRead) -> Self {
        Self {
            passthrough: &read.passthrough,
            extras: &read.extras,
            package_properties: &read.package_properties,
            relationships: &read.relationships,
            main_part: &read.main_part,
            kind: read.kind,
            package: None,
        }
    }

    /// With the original package.
    #[must_use]
    pub fn with_package(mut self, package: &'a OpcPackage) -> Self {
        self.package = Some(package);
        self
    }
}

/// How to write.
#[derive(Debug, Clone, Default)]
pub struct WriteOptions<'a> {
    /// The target format (default: the format read, else `.docx`). Saving a macro-enabled
    /// document as `.docx`/`.dotx` removes its macros.
    pub kind: Option<DocxKind>,
    /// The save time (W3CDTF, `2026-10-16T09:30:00Z`): sets `dcterms:modified` (and
    /// `dcterms:created` when missing), increments `cp:revision` and stamps the application.
    /// `None` leaves the properties as read, which makes a rewrite deterministic.
    pub now: Option<String>,
    /// `cp:lastModifiedBy`; kept from the model when absent.
    pub author: Option<String>,
    /// The page count from the layout engine for `docProps/app.xml` (dropped when unknown).
    pub pages: Option<u32>,
    /// Write no passthrough parts.
    pub skip_passthrough: bool,
    /// Copy every part these edits did not touch byte for byte from
    /// [`WriteSource::package`] (`None`: regenerate every part from the model).
    pub preserve: Option<&'a TouchedParts>,
}

/// The result of a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxWritten {
    /// The package.
    pub bytes: Vec<u8>,
    /// Parts the writer produced from the model, relationships parts it changed and
    /// `/[Content_Types].xml`. Every other part of the output is a copy of the original.
    pub regenerated: Vec<String>,
    /// Parts of the original package that the output no longer has (when the original package
    /// was given).
    pub dropped: Vec<String>,
    /// Model parts copied byte for byte because no edit touched them.
    pub preserved: Vec<String>,
    /// What could not be written exactly.
    pub issues: Vec<WriteIssue>,
}

const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

fn wml(kind: &str) -> String {
    format!("{WML}.{kind}+xml")
}

fn main_content_type(kind: DocxKind) -> &'static str {
    match kind {
        DocxKind::Document => content_type::DOCX_DOCUMENT,
        DocxKind::MacroEnabledDocument => "application/vnd.ms-word.document.macroEnabled.main+xml",
        DocxKind::Template => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml"
        }
        DocxKind::MacroEnabledTemplate => {
            "application/vnd.ms-word.template.macroEnabledTemplate.main+xml"
        }
    }
}

fn is_macro_type(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [
        "vbaproject",
        "vbadata",
        "ms-office.activex",
        "keymapcustomizations",
        "wordvbadata",
    ]
    .iter()
    .any(|m| l.contains(m))
}

const COMMENTS_IDS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsIds+xml";
const COMMENTS_EXTENSIBLE_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtensible+xml";

/// The passthrough to emit: macros only into macro-enabled formats, comment companions only
/// while there are comments.
fn filtered_passthrough(set: &PassthroughSet, macros: bool, comments: bool) -> PassthroughSet {
    let mut out = PassthroughSet::new();
    let drop_part = |ct: &str| {
        (!macros && is_macro_type(ct))
            || (!comments && (ct == COMMENTS_IDS_TYPE || ct == COMMENTS_EXTENSIBLE_TYPE))
    };
    let mut dropped: HashSet<String> = HashSet::new();
    for p in set.parts() {
        if drop_part(&p.content_type) {
            dropped.insert(part_key(&p.name));
        } else {
            out.record_part(p.clone());
        }
    }
    for r in set.relationships() {
        let rt = r.rel.rel_type.to_ascii_lowercase();
        if !macros && is_macro_type(&rt) {
            continue;
        }
        if !comments && (rt.ends_with("/commentsids") || rt.ends_with("/commentsextensible")) {
            continue;
        }
        if !r.rel.is_external() {
            let target = kiln_ooxml::opc::resolve_target(&r.source, &r.rel.target);
            if dropped.contains(&part_key(&target)) {
                continue;
            }
        }
        out.record_relationship(&r.source, r.rel.clone());
    }
    out
}

/// The writer's part plan: names of the parts it produces.
#[derive(Debug, Default)]
struct PartNames {
    main: String,
    styles: Option<String>,
    settings: Option<String>,
    numbering: Option<String>,
    footnotes: Option<String>,
    endnotes: Option<String>,
    comments: Option<String>,
    comments_extended: Option<String>,
    people: Option<String>,
    theme: Option<String>,
    font_table: Option<String>,
    /// Header/footer story → part name.
    headers_footers: BTreeMap<NodeId, String>,
}

/// Every id the model's revisions use (new revisions are numbered above).
fn max_revision_id(doc: &Document) -> u32 {
    let mut max = 0u32;
    let mut visit = |blocks: &[Block]| {
        let mut stack: Vec<&[Block]> = vec![blocks];
        while let Some(bs) = stack.pop() {
            for b in bs {
                match b {
                    Block::Paragraph(p) => {
                        for m in p.content.marks() {
                            if let Some(r) = m.mark.revision() {
                                max = max.max(r.id);
                            }
                        }
                        let rv = &p.revision;
                        for id in [
                            rv.mark.as_ref().map(|m| m.info.id),
                            rv.props_change.as_ref().map(|c| c.info.id),
                            rv.mark_props_change.as_ref().map(|c| c.info.id),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            max = max.max(id);
                        }
                        for o in p.content.objects() {
                            if let InlineObject::Drawing(d) = o
                                && let Some(tb) = &d.text_box
                            {
                                stack.push(tb);
                            }
                        }
                    }
                    Block::Table(t) => {
                        for r in &t.rows {
                            if let Some(rv) = &r.revision {
                                max = max.max(rv.info.id);
                            }
                            for c in &r.cells {
                                stack.push(&c.blocks);
                            }
                        }
                    }
                    Block::Sdt(s) => stack.push(&s.blocks),
                    Block::Field(f) => stack.push(&f.blocks),
                    Block::Opaque(_) => {}
                }
            }
        }
    };
    for s in &doc.sections {
        visit(&s.blocks);
    }
    for h in &doc.headers_footers {
        visit(&h.blocks);
    }
    for n in &doc.notes {
        visit(&n.blocks);
    }
    for c in &doc.comments {
        visit(&c.blocks);
    }
    max
}

/// All kept drawing and object XML of the model, for the largest `wp:docPr` id.
fn max_doc_pr(doc: &Document) -> u64 {
    let mut max = 0u64;
    let mut stack: Vec<&[Block]> = Vec::new();
    for s in &doc.sections {
        stack.push(&s.blocks);
    }
    for h in &doc.headers_footers {
        stack.push(&h.blocks);
    }
    for n in &doc.notes {
        stack.push(&n.blocks);
    }
    for c in &doc.comments {
        stack.push(&c.blocks);
    }
    while let Some(bs) = stack.pop() {
        for b in bs {
            match b {
                Block::Paragraph(p) => {
                    for o in p.content.objects() {
                        match o {
                            InlineObject::Drawing(d) => {
                                if let Some(x) = &d.xml {
                                    max = max.max(drawing::max_doc_pr_id(x));
                                }
                                if let Some(tb) = &d.text_box {
                                    stack.push(tb);
                                }
                            }
                            InlineObject::Opaque { xml, .. } => {
                                max = max.max(drawing::max_doc_pr_id(xml));
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    for r in &t.rows {
                        for c in &r.cells {
                            stack.push(&c.blocks);
                        }
                    }
                }
                Block::Sdt(s) => stack.push(&s.blocks),
                Block::Field(f) => stack.push(&f.blocks),
                Block::Opaque(o) => max = max.max(drawing::max_doc_pr_id(&o.xml)),
            }
        }
    }
    max
}

/// The largest `w:bookmarkStart` id in some XML.
fn max_bookmark_id(xml: &str) -> u64 {
    let mut max = 0u64;
    let mut rest = xml;
    while let Some(i) = rest.find("<w:bookmarkStart") {
        rest = &rest[i + 16..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let Some(j) = tag.find("w:id=\"") {
            let v = &tag[j + 6..];
            if let Some(k) = v.find('"')
                && let Ok(n) = v[..k].trim().parse::<i64>()
            {
                max = max.max(u64::try_from(n).unwrap_or(0));
            }
        }
        rest = &rest[end..];
    }
    max
}

/// Numeric ids for notes: the ids read, fresh ones for new notes (above the largest for normal
/// notes, below the smallest for separators), per kind.
fn note_ids(doc: &Document, extras: &DocxExtras) -> HashMap<NodeId, String> {
    let mut out = HashMap::new();
    for kind in [NoteKind::Footnote, NoteKind::Endnote] {
        let notes: Vec<_> = doc.notes.iter().filter(|n| n.kind == kind).collect();
        let mut used: BTreeSet<String> = BTreeSet::new();
        let (mut max, mut min) = (0i64, 0i64);
        for n in &notes {
            if let Some(id) = extras.note_ids.get(&n.id) {
                used.insert(id.clone());
                if let Ok(v) = id.trim().parse::<i64>() {
                    max = max.max(v);
                    min = min.min(v);
                }
            }
        }
        for n in &notes {
            let id = match extras.note_ids.get(&n.id) {
                Some(id) => id.clone(),
                None if n.note_type == NoteType::Normal => {
                    max += 1;
                    max.to_string()
                }
                None => {
                    min -= 1;
                    min.to_string()
                }
            };
            used.insert(id.clone());
            out.insert(n.id, id);
        }
    }
    out
}

fn comment_ids(doc: &Document, extras: &DocxExtras) -> HashMap<NodeId, String> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut max = -1i64;
    for c in &doc.comments {
        if let Some(id) = extras.comment_ids.get(&c.id) {
            used.insert(id.clone());
            if let Ok(v) = id.trim().parse::<i64>() {
                max = max.max(v);
            }
        }
    }
    let mut out = HashMap::new();
    for c in &doc.comments {
        let id = match extras.comment_ids.get(&c.id) {
            Some(id) => id.clone(),
            None => {
                max += 1;
                while used.contains(&max.to_string()) {
                    max += 1;
                }
                used.insert(max.to_string());
                max.to_string()
            }
        };
        out.insert(c.id, id);
    }
    out
}

/// Relationship facts for comparing two relationship sets.
fn rel_facts(pkg: &OpcPackage, source: &str) -> Option<BTreeSet<(String, String, String, bool)>> {
    let rels = pkg.relationships(source).ok()?;
    Some(
        rels.all()
            .iter()
            .map(|r| {
                let target = if r.is_external() {
                    r.target.clone()
                } else {
                    part_key(&rels.resolve(r).unwrap_or_else(|| r.target.clone()))
                };
                (r.id.clone(), r.rel_type.clone(), target, r.is_external())
            })
            .collect(),
    )
}

/// Writes a document. `source` is what the reader returned beside it (when the document was
/// read from a file); without it a new package is built.
///
/// # Errors
///
/// [`DocxWriteError::InvalidDocument`] when the model is not consistent,
/// [`DocxWriteError::Package`] when the package cannot be assembled (an invalid part name in the
/// passthrough, for example).
#[allow(clippy::too_many_lines)]
pub fn write_docx(
    doc: &Document,
    source: Option<&WriteSource<'_>>,
    options: &WriteOptions<'_>,
) -> Result<DocxWritten, DocxWriteError> {
    doc.validate()?;
    let default_extras = DocxExtras::default();
    let empty_passthrough = PassthroughSet::new();
    let empty_rels: BTreeMap<String, BTreeMap<String, RelTarget>> = BTreeMap::new();
    let empty_props = DocumentProperties::default();
    let extras = source.map_or(&default_extras, |s| s.extras);
    let original_pkg = source.and_then(|s| s.package);
    let kind = options
        .kind
        .or(source.map(|s| s.kind))
        .unwrap_or(DocxKind::Document);
    let macros = matches!(
        kind,
        DocxKind::MacroEnabledDocument | DocxKind::MacroEnabledTemplate
    );
    let passthrough = if options.skip_passthrough {
        PassthroughSet::new()
    } else {
        filtered_passthrough(
            source.map_or(&empty_passthrough, |s| s.passthrough),
            macros,
            !doc.comments.is_empty(),
        )
    };

    // ---- original relationships (the package's own from the original package, when at hand)
    let mut original_rels = source.map_or(&empty_rels, |s| s.relationships).clone();
    if let Some(pkg) = original_pkg
        && let Ok(rels) = pkg.relationships("/")
    {
        let map = original_rels.entry("/".to_owned()).or_default();
        for r in rels.all() {
            map.insert(
                r.id.clone(),
                RelTarget {
                    rel_type: r.rel_type.clone(),
                    target: if r.is_external() {
                        r.target.clone()
                    } else {
                        rels.resolve(r).unwrap_or_else(|| r.target.clone())
                    },
                    external: r.is_external(),
                },
            );
        }
    }

    // ---- part names
    let main_original = source.map(|s| s.main_part.to_owned());
    let main = main_original
        .clone()
        .filter(|m| m.ends_with(".xml") && validate_part_name(m).is_ok())
        .unwrap_or_else(|| "/word/document.xml".to_owned());
    let dir = directory_of(&main).to_owned();
    let main_rels = main_original.as_ref().and_then(|m| {
        original_rels
            .iter()
            .find(|(k, _)| part_key(k) == part_key(m))
            .map(|(_, v)| v)
    });
    let original_target = |rt: &str| -> Option<String> {
        main_rels.and_then(|rels| {
            rels.values()
                .find(|t| !t.external && rel_type_matches(&t.rel_type, rt))
                .map(|t| t.target.clone())
        })
    };
    let mut taken: BTreeSet<String> = passthrough.parts().map(|p| part_key(&p.name)).collect();
    taken.insert(part_key(&main));
    let mut pick = |preferred: Option<String>, fallback: String| -> String {
        let candidates = preferred
            .into_iter()
            .chain(std::iter::once(fallback.clone()));
        for c in candidates {
            if validate_part_name(&c).is_ok() && taken.insert(part_key(&c)) {
                return c;
            }
        }
        let dot = fallback.rfind('.').unwrap_or(fallback.len());
        let stem = fallback[..dot]
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .to_owned();
        let ext = fallback[dot..].to_owned();
        let mut n = 1u32;
        loop {
            let c = format!("{stem}{n}{ext}");
            if taken.insert(part_key(&c)) {
                return c;
            }
            n += 1;
        }
    };
    let has_notes = |k: NoteKind| doc.notes.iter().any(|n| n.kind == k);
    let mut names = PartNames {
        main: main.clone(),
        ..PartNames::default()
    };
    // Styles, settings and the theme are written for new documents, for documents that had them
    // and whenever the model holds more than the defaults a missing part stands for.
    let o_styles = original_target(rel_type::STYLES);
    if source.is_none()
        || o_styles.is_some()
        || !doc.styles.is_empty()
        || doc.styles.doc_defaults != kiln_doc::DocDefaults::default()
    {
        names.styles = Some(pick(o_styles, format!("{dir}styles.xml")));
    }
    let o_settings = original_target(rel_type::SETTINGS);
    if source.is_none()
        || o_settings.is_some()
        || doc.settings != kiln_doc::Settings::default()
        || extras.settings != crate::common::extras::SettingsExtras::default()
        || doc.notes.iter().any(|n| n.note_type == NoteType::Normal)
    {
        names.settings = Some(pick(o_settings, format!("{dir}settings.xml")));
    }
    let o_numbering = original_target(rel_type::NUMBERING);
    if !doc.numbering.abstracts.is_empty()
        || !doc.numbering.instances.is_empty()
        || !extras.num_pic_bullets.is_empty()
        || o_numbering.is_some()
    {
        names.numbering = Some(pick(o_numbering, format!("{dir}numbering.xml")));
    }
    let o_footnotes = original_target(rel_type::FOOTNOTES);
    if has_notes(NoteKind::Footnote) || o_footnotes.is_some() {
        names.footnotes = Some(pick(o_footnotes, format!("{dir}footnotes.xml")));
    }
    let o_endnotes = original_target(rel_type::ENDNOTES);
    if has_notes(NoteKind::Endnote) || o_endnotes.is_some() {
        names.endnotes = Some(pick(o_endnotes, format!("{dir}endnotes.xml")));
    }
    let o_comments = original_target(rel_type::COMMENTS);
    if !doc.comments.is_empty() || o_comments.is_some() {
        names.comments = Some(pick(o_comments, format!("{dir}comments.xml")));
    }
    let o_ext = original_target(rel_type::COMMENTS_EXTENDED);
    let threads = doc
        .comments
        .iter()
        .any(|c| c.meta.parent.is_some() || c.meta.resolved);
    if names.comments.is_some()
        && !doc.comments.is_empty()
        && (o_ext.is_some() || source.is_none() || threads)
    {
        names.comments_extended = Some(pick(o_ext, format!("{dir}commentsExtended.xml")));
    }
    let o_people = original_target(rel_type::PEOPLE);
    if o_people.is_some()
        || !extras.people.is_empty()
        || (source.is_none() && !doc.comments.is_empty())
    {
        names.people = Some(pick(o_people, format!("{dir}people.xml")));
    }
    let o_theme = original_target(rel_type::THEME);
    if source.is_none() || o_theme.is_some() || doc.theme != kiln_doc::DocTheme::office() {
        names.theme = Some(pick(o_theme, format!("{dir}theme/theme1.xml")));
    }
    let has_font_table = passthrough
        .parts()
        .any(|p| p.content_type == wml("fontTable"));
    if source.is_none() && !has_font_table {
        names.font_table = Some(pick(None, format!("{dir}fontTable.xml")));
    }
    let referenced: Vec<NodeId> = doc
        .sections
        .iter()
        .flat_map(|s| {
            [
                HeaderFooterKind::Default,
                HeaderFooterKind::Even,
                HeaderFooterKind::First,
            ]
            .into_iter()
            .flat_map(move |k| [s.headers.get(k), s.footers.get(k)])
            .flatten()
        })
        .collect();
    for id in referenced {
        if names.headers_footers.contains_key(&id) {
            continue;
        }
        let Some(story) = doc.headers_footers.iter().find(|h| h.id == id) else {
            continue;
        };
        let footer = story.part == kiln_doc::HeaderFooterPart::Footer;
        let fallback = format!("{dir}{}1.xml", if footer { "footer" } else { "header" });
        let name = pick(extras.header_footer_parts.get(&id).cloned(), fallback);
        names.headers_footers.insert(id, name);
    }

    // ---- placeholders, so relationships to and from them survive the passthrough
    let mut pkg = OpcPackage::new();
    let mut orig_to_new: HashMap<String, String> = HashMap::new();
    let mut produced: Vec<(String, String)> =
        vec![(names.main.clone(), main_content_type(kind).to_owned())];
    if let Some(m) = &main_original {
        orig_to_new.insert(part_key(m), names.main.clone());
    }
    for (orig, name) in [
        (original_target(rel_type::STYLES), names.styles.as_ref()),
        (original_target(rel_type::SETTINGS), names.settings.as_ref()),
        (
            original_target(rel_type::NUMBERING),
            names.numbering.as_ref(),
        ),
        (
            original_target(rel_type::FOOTNOTES),
            names.footnotes.as_ref(),
        ),
        (original_target(rel_type::ENDNOTES), names.endnotes.as_ref()),
        (original_target(rel_type::COMMENTS), names.comments.as_ref()),
        (
            original_target(rel_type::COMMENTS_EXTENDED),
            names.comments_extended.as_ref(),
        ),
        (original_target(rel_type::PEOPLE), names.people.as_ref()),
        (original_target(rel_type::THEME), names.theme.as_ref()),
    ] {
        if let (Some(o), Some(n)) = (orig, name) {
            orig_to_new.insert(part_key(&o), n.clone());
        }
    }
    for (id, name) in &names.headers_footers {
        if let Some(o) = extras.header_footer_parts.get(id) {
            orig_to_new.insert(part_key(o), name.clone());
        }
    }
    let optional = [
        (&names.styles, content_type::DOCX_STYLES.to_owned()),
        (&names.settings, content_type::DOCX_SETTINGS.to_owned()),
        (&names.numbering, content_type::DOCX_NUMBERING.to_owned()),
        (&names.footnotes, wml("footnotes")),
        (&names.endnotes, wml("endnotes")),
        (&names.comments, wml("comments")),
        (&names.comments_extended, wml("commentsExtended")),
        (&names.people, wml("people")),
    ];
    for (n, ct) in optional {
        if let Some(n) = n {
            produced.push((n.clone(), ct));
        }
    }
    for (id, name) in &names.headers_footers {
        let footer = doc
            .headers_footers
            .iter()
            .any(|h| h.id == *id && h.part == kiln_doc::HeaderFooterPart::Footer);
        produced.push((name.clone(), wml(if footer { "footer" } else { "header" })));
    }
    if let Some(t) = &names.theme {
        produced.push((t.clone(), content_type::THEME.to_owned()));
    }
    if let Some(f) = &names.font_table {
        produced.push((f.clone(), wml("fontTable")));
    }
    for (name, ct) in &produced {
        pkg.add_part(name, ct, Vec::new())?;
    }

    // ---- passthrough first: its relationship ids are kept as written
    let emitted = passthrough.emit(&mut pkg, |n| orig_to_new.get(&part_key(n)).cloned())?;

    // ---- what is preserved byte for byte: untouched parts whose relationships all still lead
    // somewhere
    let mut preserved: BTreeSet<String> = BTreeSet::new();
    let mut first_bookmark = 0u64;
    if let (Some(touched), Some(orig)) = (options.preserve, original_pkg)
        && !touched.everything
    {
        let can = |name: &str| -> bool {
            if !orig.has(name) || orig.content_type(name) != pkg.content_type(name) {
                return false;
            }
            let Ok(rels) = orig.relationships(name) else {
                return false;
            };
            rels.all().iter().all(|r| {
                r.is_external()
                    || rels
                        .resolve(r)
                        .is_some_and(|t| pkg.has(&t) || doc.media.contains_key(&t))
            })
        };
        let mut candidates: Vec<(bool, Option<&String>)> = vec![
            (touched.body, Some(&names.main)),
            (touched.styles, names.styles.as_ref()),
            (touched.numbering, names.numbering.as_ref()),
            (touched.settings, names.settings.as_ref()),
            (touched.footnotes, names.footnotes.as_ref()),
            (touched.endnotes, names.endnotes.as_ref()),
            (touched.comments, names.comments.as_ref()),
            (touched.comments, names.comments_extended.as_ref()),
            (touched.comments, names.people.as_ref()),
        ];
        for (id, name) in &names.headers_footers {
            candidates.push((touched.header_footer(*id), Some(name)));
        }
        for (touched_flag, name) in candidates {
            if let Some(n) = name
                && !touched_flag
                && can(n)
            {
                preserved.insert(n.clone());
            }
        }
        // Bookmarks written into regenerated parts are numbered above those of copied parts.
        for p in &preserved {
            if let Ok(b) = orig.part_bytes(p) {
                let max = max_bookmark_id(&String::from_utf8_lossy(&b));
                first_bookmark = first_bookmark.max(max + 1);
            }
        }
    }

    let mut ctx = WriteContext::new(
        pkg,
        doc,
        &original_rels,
        max_doc_pr(doc),
        max_revision_id(doc),
        first_bookmark,
    );
    for d in &emitted.dropped {
        let what = d
            .rel
            .rel_type
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        ctx.issue(
            "passthrough.dropped",
            format!(
                "A relationship from {} ({what}) could not be kept.",
                d.source
            ),
            &d.source,
        );
    }
    // Media the passthrough parts refer to (an image shared with a kept chart, for example).
    for p in passthrough.parts() {
        for r in &p.relationships {
            if r.is_external() {
                continue;
            }
            let target = kiln_ooxml::opc::resolve_target(&p.name, &r.target);
            if !ctx.pkg.has(&target) && doc.media.contains_key(&target) {
                let _ = ctx.media_part(&target);
            }
        }
    }
    for (name, _) in &produced {
        ctx.produced.insert(part_key(name));
        let original = orig_to_new
            .iter()
            .find(|(_, v)| *v == name)
            .map(|(k, _)| k.clone());
        if let Some(o) = original {
            // The original spelling of the name, for relationship lookups.
            let spelled = original_rels
                .keys()
                .find(|k| part_key(k) == o)
                .cloned()
                .unwrap_or(o);
            ctx.set_original(name, &spelled);
        }
    }
    let is_preserved = |n: &str| preserved.contains(n);

    // ---- package and main relationships
    ctx.rel("/", rel_type::OFFICE_DOCUMENT, &names.main, false);
    for (n, t) in [
        (&names.styles, rel_type::STYLES),
        (&names.settings, rel_type::SETTINGS),
        (&names.numbering, rel_type::NUMBERING),
        (&names.footnotes, rel_type::FOOTNOTES),
        (&names.endnotes, rel_type::ENDNOTES),
        (&names.comments, rel_type::COMMENTS),
        (&names.comments_extended, rel_type::COMMENTS_EXTENDED),
        (&names.people, rel_type::PEOPLE),
        (&names.font_table, rel_type::FONT_TABLE),
    ] {
        if let Some(n) = n {
            ctx.rel(&names.main, t, n, false);
        }
    }
    if let Some(t) = &names.theme {
        ctx.rel(&names.main, rel_type::THEME, t, false);
    }

    // ---- story ids
    let ids = StoryIds {
        notes: note_ids(doc, extras),
        comments: comment_ids(doc, extras),
        comment_reference_style: doc.styles.contains("CommentReference"),
        extras,
    };
    let mut written: Vec<(String, String)> = Vec::new();

    // ---- headers and footers
    for (id, name) in &names.headers_footers {
        let Some(story) = doc.headers_footers.iter().find(|h| h.id == *id) else {
            continue;
        };
        if is_preserved(name) {
            continue;
        }
        let mut w = StoryWriter::new(&mut ctx, &ids, name, None);
        let mut body = w.story(&story.blocks, BlockOpts::default());
        if !matches!(story.blocks.last(), Some(Block::Paragraph(_))) {
            body.push_str("<w:p/>");
        }
        let root = if story.part == kiln_doc::HeaderFooterPart::Footer {
            "w:ftr"
        } else {
            "w:hdr"
        };
        written.push((name.clone(), xml::part(root, &body, &["w", "r"])));
    }

    // ---- body
    if !is_preserved(&names.main) {
        // References in the order of the stories (the reader numbers stories as it meets them).
        let story_index = |id: NodeId| doc.headers_footers.iter().position(|h| h.id == id);
        let mut refs_of: Vec<Vec<HeaderFooterRef>> = Vec::new();
        for s in &doc.sections {
            let mut refs = Vec::new();
            for (footer, list) in [(false, &s.headers), (true, &s.footers)] {
                let mut kinds: Vec<(usize, HeaderFooterKind, NodeId)> = [
                    HeaderFooterKind::Default,
                    HeaderFooterKind::Even,
                    HeaderFooterKind::First,
                ]
                .into_iter()
                .filter_map(|k| {
                    let id = list.get(k)?;
                    Some((story_index(id).unwrap_or(usize::MAX), k, id))
                })
                .collect();
                kinds.sort_by_key(|(i, _, _)| *i);
                for (_, k, id) in kinds {
                    let Some(part) = names.headers_footers.get(&id) else {
                        continue;
                    };
                    let rt = if footer {
                        rel_type::FOOTER
                    } else {
                        rel_type::HEADER
                    };
                    let rel_id = ctx.rel(&names.main, rt, part, false);
                    refs.push(HeaderFooterRef {
                        footer,
                        kind: k,
                        rel_id,
                    });
                }
            }
            refs_of.push(refs);
        }
        let mut w = StoryWriter::new(&mut ctx, &ids, &names.main, None);
        let parts: Vec<&[Block]> = doc.sections.iter().map(|s| s.blocks.as_slice()).collect();
        w.prepare_comments(&parts);
        let mut body = String::new();
        let n = doc.sections.len();
        for (i, s) in doc.sections.iter().enumerate() {
            let sect = section_props_xml(&s.props, extras.sections.get(&s.id), &refs_of[i]);
            if i + 1 == n {
                body.push_str(&w.blocks(&s.blocks, BlockOpts::default()));
                body.push_str(&sect);
            } else {
                body.push_str(&w.blocks(
                    &s.blocks,
                    BlockOpts {
                        sect_pr: Some(sect),
                        ..BlockOpts::default()
                    },
                ));
            }
        }
        let inner = format!(
            "{}<w:body>{body}</w:body>",
            extras.background.as_deref().unwrap_or_default()
        );
        let strict = extras.document_root_attrs.iter().any(|(k, v)| {
            k == "w:conformance" || (k.starts_with("xmlns") && v.contains("purl.oclc.org"))
        });
        let xml_text = if extras.document_root_attrs.is_empty() || strict {
            xml::part("w:document", &inner, &["w", "r"])
        } else {
            xml::part_with_root_attrs("w:document", &extras.document_root_attrs, &inner)
        };
        written.push((names.main.clone(), xml_text));
    }

    // ---- notes
    let mut separators: HashMap<NoteKind, Vec<String>> = HashMap::new();
    for (kind, name) in [
        (NoteKind::Footnote, &names.footnotes),
        (NoteKind::Endnote, &names.endnotes),
    ] {
        let notes: Vec<_> = doc.notes.iter().filter(|n| n.kind == kind).collect();
        let mut seps: Vec<String> = notes
            .iter()
            .filter(|n| n.note_type != NoteType::Normal)
            .filter_map(|n| ids.notes.get(&n.id).cloned())
            .collect();
        let Some(name) = name else {
            continue;
        };
        let el = match kind {
            NoteKind::Footnote => "w:footnote",
            NoteKind::Endnote => "w:endnote",
        };
        let mut body = String::new();
        let has_separators = notes.iter().any(|n| n.note_type == NoteType::Separator);
        if !has_separators && notes.iter().any(|n| n.note_type == NoteType::Normal) {
            // Notes created in the editor: Word's separator notes.
            let used: HashSet<&String> = ids.notes.values().collect();
            let mut free = (-1i64..).step_by(1);
            let mut pick_id = || loop {
                let v = free.next().unwrap_or(-1);
                let s = v.to_string();
                if !used.contains(&s) {
                    return s;
                }
            };
            let (sep, cont) = (pick_id(), pick_id());
            for (t, id) in [("separator", &sep), ("continuationSeparator", &cont)] {
                body.push_str(&format!(
                    "<{el} w:type=\"{t}\" w:id=\"{id}\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:{t}/></w:r></w:p></{el}>"
                ));
            }
            seps = vec![sep, cont];
        }
        separators.insert(kind, seps);
        if is_preserved(name) {
            continue;
        }
        for n in notes {
            let id = ids.notes.get(&n.id).cloned().unwrap_or_default();
            let mut w = StoryWriter::new(&mut ctx, &ids, name, Some(n.id));
            let content = w.story(
                &n.blocks,
                BlockOpts {
                    first_prefix: separator_run(n.note_type).map(str::to_owned),
                    ..BlockOpts::default()
                },
            );
            let ty = (n.note_type != NoteType::Normal).then(|| n.note_type.as_str());
            body.push_str(
                &xml::El::new(el)
                    .opt("w:type", ty)
                    .attr("w:id", id)
                    .wrap(&content),
            );
        }
        let root = match kind {
            NoteKind::Footnote => "w:footnotes",
            NoteKind::Endnote => "w:endnotes",
        };
        written.push((name.clone(), xml::part(root, &body, &["w", "r"])));
    }

    // ---- comments
    let mut regenerated_extra: Vec<String> = Vec::new();
    if let Some(cname) = &names.comments {
        let mut used_para: BTreeSet<String> = extras
            .paragraphs
            .values()
            .filter_map(|p| p.para_id.clone())
            .collect();
        let mut entries: Vec<CommentEx> = Vec::new();
        let mut para_of: HashMap<NodeId, String> = HashMap::new();
        // Threads (commentsExtended) and durable ids (commentsIds) key comments by the paraId of
        // their last paragraph.
        let needs_para_ids = names.comments_extended.is_some()
            || !ctx.pkg.parts_by_content_type(COMMENTS_IDS_TYPE).is_empty()
            || source.is_none();
        for (i, c) in doc.comments.iter().enumerate() {
            let own = match c.blocks.last() {
                Some(Block::Paragraph(p)) => {
                    extras.paragraphs.get(&p.id).and_then(|x| x.para_id.clone())
                }
                _ => None,
            };
            let wid = ids.comments.get(&c.id).cloned().unwrap_or_default();
            let para = match own {
                Some(p) => Some(p),
                None if needs_para_ids => {
                    Some(make_hex_id(&format!("comment:{wid}:{i}"), &mut used_para))
                }
                None => None,
            };
            if let Some(p) = para {
                para_of.insert(c.id, p);
            }
        }
        let mut body = String::new();
        if !is_preserved(cname) {
            for c in &doc.comments {
                let wid = ids.comments.get(&c.id).cloned().unwrap_or_default();
                let style = if ids.comment_reference_style {
                    "<w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr>"
                } else {
                    ""
                };
                let mut w = StoryWriter::new(&mut ctx, &ids, cname, None);
                let content = w.story(
                    &c.blocks,
                    BlockOpts {
                        first_prefix: Some(format!("<w:r>{style}<w:annotationRef/></w:r>")),
                        last_para_id: para_of.get(&c.id).cloned(),
                        ..BlockOpts::default()
                    },
                );
                body.push_str(
                    &xml::El::new("w:comment")
                        .attr("w:id", &wid)
                        .attr("w:author", &c.meta.author)
                        .opt("w:date", c.meta.date.as_deref())
                        .opt("w:initials", c.meta.initials.as_deref())
                        .wrap(&content),
                );
            }
            written.push((cname.clone(), xml::part("w:comments", &body, &["w", "r"])));
        }
        for c in &doc.comments {
            if let Some(p) = para_of.get(&c.id) {
                entries.push(CommentEx {
                    para_id: p.clone(),
                    parent: c.meta.parent.and_then(|pid| para_of.get(&pid).cloned()),
                    done: c.meta.resolved,
                });
            }
        }
        if let Some(ext) = &names.comments_extended
            && !is_preserved(ext)
        {
            written.push((ext.clone(), parts::comments_extended_part(&entries)));
        }
        // commentsIds / commentsExtensible (Word 2016+), kept in step with the comments.
        let current: Vec<String> = doc
            .comments
            .iter()
            .filter_map(|c| para_of.get(&c.id).cloned())
            .collect();
        let ids_part = ctx
            .pkg
            .parts_by_content_type(COMMENTS_IDS_TYPE)
            .first()
            .map(|s| (*s).to_owned());
        match ids_part {
            Some(p) if !is_preserved(cname) => {
                let parsed = ctx.pkg.part_xml(&p).ok().flatten().map(|d| d.root);
                let map: Vec<(String, String)> = parsed
                    .as_ref()
                    .map(|r| {
                        r.elements()
                            .filter_map(|e| {
                                Some((
                                    e.attr("w16cid:paraId")?.to_owned(),
                                    e.attr("w16cid:durableId")?.to_owned(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let old: BTreeSet<&String> = map.iter().map(|(p, _)| p).collect();
                let new: BTreeSet<&String> = current.iter().collect();
                if old != new {
                    let mut used: BTreeSet<String> = map.iter().map(|(_, d)| d.clone()).collect();
                    let entries: Vec<(String, String)> = current
                        .iter()
                        .map(|para| {
                            let durable = map
                                .iter()
                                .find(|(p, _)| p == para)
                                .map(|(_, d)| d.clone())
                                .unwrap_or_else(|| {
                                    make_hex_id(&format!("durable:{para}"), &mut used)
                                });
                            (para.clone(), durable)
                        })
                        .collect();
                    let durable: BTreeSet<String> =
                        entries.iter().map(|(_, d)| d.clone()).collect();
                    ctx.pkg
                        .set_part(&p, parts::comments_ids_part(&entries).into_bytes())?;
                    regenerated_extra.push(p.clone());
                    // Drop extensible entries of removed comments.
                    let ext_part = ctx
                        .pkg
                        .parts_by_content_type(COMMENTS_EXTENSIBLE_TYPE)
                        .first()
                        .map(|s| (*s).to_owned());
                    if let Some(ep) = ext_part
                        && let Ok(Some(mut d)) = ctx.pkg.part_xml(&ep)
                    {
                        d.root.children.retain(|c| {
                            c.as_element().is_none_or(|e| {
                                e.attr("w16cex:durableId")
                                    .is_none_or(|v| durable.contains(v))
                            })
                        });
                        let bytes = kiln_ooxml::xml::document_to_bytes(&d).unwrap_or_default();
                        ctx.pkg.set_part(&ep, bytes)?;
                        regenerated_extra.push(ep);
                    }
                }
            }
            None if source.is_none() && !doc.comments.is_empty() => {
                let p = format!("{dir}commentsIds.xml");
                if !ctx.pkg.has(&p) {
                    let mut used: BTreeSet<String> = BTreeSet::new();
                    let entries: Vec<(String, String)> = current
                        .iter()
                        .map(|para| {
                            (
                                para.clone(),
                                make_hex_id(&format!("durable:{para}"), &mut used),
                            )
                        })
                        .collect();
                    ctx.pkg.add_part(
                        &p,
                        COMMENTS_IDS_TYPE,
                        parts::comments_ids_part(&entries).into_bytes(),
                    )?;
                    ctx.produced.insert(part_key(&p));
                    ctx.rel(&names.main, rel_type::COMMENTS_IDS, &p, false);
                    regenerated_extra.push(p);
                }
            }
            _ => {}
        }
    }
    if let Some(pname) = &names.people
        && !is_preserved(pname)
    {
        let mut people: Vec<Person> = extras.people.clone();
        if !extras.people.is_empty() || source.is_none() {
            // Authors of comments written in Kiln join the list, as Word adds them.
            for c in doc
                .comments
                .iter()
                .filter(|c| !extras.comment_ids.contains_key(&c.id))
            {
                if !people.iter().any(|p| p.author == c.meta.author) {
                    people.push(Person {
                        author: c.meta.author.clone(),
                        provider_id: Some("None".to_owned()),
                        user_id: Some(c.meta.author.clone()),
                    });
                }
            }
        }
        written.push((pname.clone(), parts::people_part(&people)));
    }

    // ---- styles, numbering, settings, theme, fonts
    let original_root = |name: Option<&str>| -> Option<XmlElement> {
        let pkg = original_pkg?;
        pkg.part_xml(name?).ok().flatten().map(|d| d.root)
    };
    if let Some(n) = &names.styles
        && !is_preserved(n)
    {
        let orig = original_root(original_target(rel_type::STYLES).as_deref());
        written.push((
            n.clone(),
            parts::styles_part(&doc.styles, extras, orig.as_ref()),
        ));
    }
    if let Some(n) = &names.numbering
        && !is_preserved(n)
    {
        let orig = original_root(original_target(rel_type::NUMBERING).as_deref());
        written.push((n.clone(), parts::numbering_part(doc, extras, orig.as_ref())));
    }
    if let Some(n) = &names.settings
        && !is_preserved(n)
    {
        let empty: Vec<String> = Vec::new();
        written.push((
            n.clone(),
            parts::settings_part(&SettingsInput {
                new_document: source.is_none(),
                settings: &doc.settings,
                extras: &extras.settings,
                original: extras.settings_xml.as_deref(),
                footnote_separators: separators.get(&NoteKind::Footnote).unwrap_or(&empty),
                endnote_separators: separators.get(&NoteKind::Endnote).unwrap_or(&empty),
            }),
        ));
    }
    let theme_original: Option<Vec<u8>> = original_pkg.and_then(|p| {
        original_target(rel_type::THEME).and_then(|t| p.part_bytes(&t).ok().map(|b| b.into_owned()))
    });
    let mut theme_copied = true;
    if let Some(t) = &names.theme {
        let theme_bytes = parts::theme_part(&doc.theme, theme_original.as_deref());
        theme_copied = theme_original.as_deref() == Some(theme_bytes.as_slice());
        ctx.pkg.set_part(t, theme_bytes)?;
    }
    if let Some(f) = &names.font_table {
        written.push((f.clone(), parts::font_table_part(doc)));
    }

    // ---- the written parts, with the relationships their kept XML refers to
    for (name, text) in &written {
        repair_raw_relationships(&mut ctx, name, text);
        ctx.pkg.set_part(name, text.clone().into_bytes())?;
    }

    // ---- preserved parts: original bytes and relationships
    if let Some(orig) = original_pkg {
        for name in &preserved {
            let bytes = orig.part_bytes(name)?.into_owned();
            let structural: Vec<(String, String, String)> = ctx
                .pkg
                .relationships(name)
                .map(|r| {
                    r.all()
                        .iter()
                        .filter(|x| !x.is_external())
                        .map(|x| {
                            (
                                x.id.clone(),
                                x.rel_type.clone(),
                                r.resolve(x).unwrap_or_else(|| x.target.clone()),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let current_ids: Vec<String> = ctx
                .pkg
                .relationships(name)
                .map(|r| r.all().iter().map(|x| x.id.clone()).collect())
                .unwrap_or_default();
            for id in current_ids {
                ctx.pkg.remove_relationship(name, &id)?;
            }
            let originals = orig.relationships(name)?.into_owned();
            for r in originals.all() {
                if !r.is_external()
                    && let Some(t) = originals.resolve(r)
                    && !ctx.pkg.has(&t)
                {
                    let _ = ctx.media_part(&t);
                }
                ctx.pkg.add_relationship_raw(
                    name,
                    &r.rel_type,
                    &r.target,
                    Some(&r.id),
                    r.target_mode,
                )?;
            }
            ctx.forget_rels(name);
            // Structural relationships the writer needs that the original part lacks (a
            // numbering part added since, for example).
            for (_, rt, target) in structural {
                let present = originals.all().iter().any(|r| {
                    rel_type_matches(&r.rel_type, &rt)
                        && originals
                            .resolve(r)
                            .is_some_and(|t| part_key(&t) == part_key(&target))
                });
                let structural_type = [
                    rel_type::STYLES,
                    rel_type::SETTINGS,
                    rel_type::NUMBERING,
                    rel_type::FOOTNOTES,
                    rel_type::ENDNOTES,
                    rel_type::COMMENTS,
                    rel_type::COMMENTS_EXTENDED,
                    rel_type::COMMENTS_IDS,
                    rel_type::PEOPLE,
                    rel_type::THEME,
                    rel_type::FONT_TABLE,
                ]
                .iter()
                .any(|s| rel_type_matches(&rt, s));
                if !present && structural_type {
                    ctx.rel(name, &rt, &target, false);
                }
            }
            ctx.pkg.set_part(name, bytes)?;
        }
    }

    // ---- document properties
    let props = write_properties(&mut ctx, doc, source, options, &empty_props);
    write_document_properties(&mut ctx.pkg, &props)?;

    // ---- relationships parts that did not change: the original bytes
    let mut restored: BTreeSet<String> = BTreeSet::new();
    if let Some(orig) = original_pkg {
        let rels_parts: Vec<String> = ctx
            .pkg
            .all_part_names()
            .filter(|n| is_rels_part(n))
            .map(str::to_owned)
            .collect();
        for r in rels_parts {
            let Some(source_part) = source_part_of_rels(&r) else {
                continue;
            };
            if !orig.has(&r) {
                continue;
            }
            if rel_facts(&ctx.pkg, &source_part) == rel_facts(orig, &source_part)
                && let Ok(bytes) = orig.part_bytes(&r)
            {
                let bytes = bytes.into_owned();
                ctx.pkg.set_part(&r, bytes)?;
                restored.insert(part_key(&r));
            }
        }
    }

    // ---- what was regenerated
    let mut regenerated: Vec<String> = vec![kiln_ooxml::opc::CONTENT_TYPES_PART.to_owned()];
    for (name, _) in &written {
        regenerated.push(name.clone());
    }
    regenerated.extend(regenerated_extra);
    if !theme_copied && let Some(t) = &names.theme {
        regenerated.push(t.clone());
    }
    for rt in [
        rel_type::CORE_PROPERTIES,
        rel_type::EXTENDED_PROPERTIES,
        rel_type::CUSTOM_PROPERTIES,
    ] {
        if let Ok(Some(p)) = ctx.pkg.related_part("/", rt) {
            regenerated.push(p);
        }
    }
    for n in ctx.pkg.all_part_names() {
        if is_rels_part(n) && !restored.contains(&part_key(n)) {
            regenerated.push(n.to_owned());
        }
    }
    regenerated.sort();
    regenerated.dedup();
    let dropped: Vec<String> = original_pkg
        .map(|o| {
            o.all_part_names()
                .filter(|n| !ctx.pkg.has(n))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let bytes = ctx.pkg.save()?;
    Ok(DocxWritten {
        bytes,
        regenerated,
        dropped,
        preserved: preserved.into_iter().collect(),
        issues: ctx.issues,
    })
}

/// A Unix time (seconds) as the W3CDTF timestamp [`WriteOptions::now`] takes
/// (`2026-10-16T09:30:00Z`), proleptic Gregorian, for hosts that only have a clock.
#[must_use]
pub fn w3cdtf(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let rem = unix_seconds.rem_euclid(86_400);
    // Civil from days (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Reads and writes a file without edits, regenerating every part from the model: the round
/// trip the fidelity scorer and `kiln-cli convert` measure.
///
/// # Errors
///
/// As [`crate::read_docx`] and [`write_docx`].
pub fn rewrite_docx(bytes: &[u8], options: &WriteOptions<'_>) -> Result<DocxWritten, RewriteError> {
    let pkg =
        OpcPackage::open(bytes).map_err(|e| RewriteError::Read(crate::DocxError::Package(e)))?;
    let read = crate::read::read_package(&pkg).map_err(RewriteError::Read)?;
    let source = WriteSource::from_read(&read).with_package(&pkg);
    write_docx(&read.document, Some(&source), options).map_err(RewriteError::Write)
}

/// Why [`rewrite_docx`] failed.
#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    /// The input could not be read.
    #[error("read: {0}")]
    Read(crate::DocxError),
    /// The document could not be written.
    #[error("write: {0}")]
    Write(DocxWriteError),
}

/// Raw XML kept from the original file may refer to relationships the writer did not recreate
/// (an image used by a kept shape, a chart, an OLE object): those ids are re-added with their
/// original targets.
fn repair_raw_relationships(ctx: &mut WriteContext<'_>, part: &str, text: &str) {
    for id in xml::relationship_refs(text) {
        let exists = ctx
            .pkg
            .relationships(part)
            .is_ok_and(|r| r.get(&id).is_some());
        if exists {
            continue;
        }
        let Some(target) = ctx.original_rels_of(part).and_then(|o| o.get(&id)).cloned() else {
            ctx.issue(
                "relationship.missing",
                format!("Content refers to a relationship ({id}) that no longer exists."),
                part,
            );
            continue;
        };
        if !target.external
            && !ctx.pkg.has(&target.target)
            && ctx.media_part(&target.target).is_none()
        {
            ctx.issue(
                "relationship.missing",
                format!(
                    "Content refers to a part ({}) that is not in the file.",
                    target.target
                ),
                part,
            );
            continue;
        }
        ctx.rel_with_id(part, &id, &target.rel_type, &target.target, target.external);
    }
}

/// The document properties to write: the package's as read, with the model's values.
fn write_properties(
    ctx: &mut WriteContext<'_>,
    doc: &Document,
    source: Option<&WriteSource<'_>>,
    options: &WriteOptions<'_>,
    empty: &DocumentProperties,
) -> PropertiesUpdate {
    let read = source.map_or(empty, |s| s.package_properties);
    let m = &doc.properties;
    let mut core: CoreProperties = read.core.clone();
    core.title.clone_from(&m.title);
    core.subject.clone_from(&m.subject);
    core.creator.clone_from(&m.creator);
    core.keywords.clone_from(&m.keywords);
    core.description.clone_from(&m.description);
    core.last_modified_by.clone_from(&m.last_modified_by);
    // A revision the model could not read as a number stays as written.
    core.revision = match m.revision {
        Some(r) => Some(r.to_string()),
        None => read
            .core
            .revision
            .clone()
            .filter(|r| r.trim().parse::<u32>().is_err()),
    };
    core.created.clone_from(&m.created);
    core.modified.clone_from(&m.modified);
    core.category.clone_from(&m.category);
    core.content_status.clone_from(&m.content_status);
    core.language.clone_from(&m.language);
    if let Some(now) = &options.now {
        core.modified = Some(now.clone());
        if core.created.is_none() {
            core.created = Some(now.clone());
        }
        if let Some(r) = core
            .revision
            .as_deref()
            .and_then(|r| r.trim().parse::<u64>().ok())
        {
            core.revision = Some((r + 1).to_string());
        } else if core.revision.is_none() {
            core.revision = Some("1".to_owned());
        }
    }
    if let Some(a) = &options.author {
        core.last_modified_by = Some(a.clone());
    }
    let mut app: AppProperties = read.app.clone();
    app.template.clone_from(&m.template);
    app.total_time = m.total_time.map(i64::from);
    app.company.clone_from(&m.company);
    app.manager.clone_from(&m.manager);
    let stats = doc.statistics(Default::default());
    let as_i64 = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    app.words = Some(as_i64(stats.words));
    app.characters = Some(as_i64(stats.characters));
    app.characters_with_spaces = Some(as_i64(stats.characters_with_spaces));
    app.paragraphs = Some(as_i64(stats.paragraphs));
    if options.now.is_some() {
        app.application = Some("Kiln".to_owned());
        app.app_version = None;
        app.lines = None;
        app.pages = options.pages.map(i64::from);
    } else if let Some(p) = options.pages {
        app.pages = Some(i64::from(p));
    }
    // Custom properties: the model's, typed as read when unchanged; values the model cannot hold
    // stay.
    let mut custom: Vec<CustomProperty> = Vec::new();
    for c in &m.custom {
        use kiln_doc::settings::PropertyValue as V;
        let value = match &c.value {
            V::Text(t) => CustomPropertyValue::Text(t.clone()),
            V::Integer(i) => CustomPropertyValue::Int(*i),
            V::Real(r) => CustomPropertyValue::Number(r.trim().parse().unwrap_or(0.0)),
            V::Bool(b) => CustomPropertyValue::Bool(*b),
            V::Date(d) => CustomPropertyValue::FileTime(d.clone()),
        };
        let kept = read.custom.iter().find(|r| r.name == c.name);
        let same = kept.is_some_and(|k| match (&k.value, &c.value) {
            (CustomPropertyValue::Number(n), V::Real(r)) => {
                kiln_ooxml::xml::format_number(*n) == *r
            }
            (a, _) => *a == value,
        });
        custom.push(match kept {
            Some(k) if same => k.clone(),
            _ => CustomProperty {
                name: c.name.clone(),
                value,
                link_target: kept.and_then(|k| k.link_target.clone()),
            },
        });
    }
    for r in &read.custom {
        if matches!(r.value, CustomPropertyValue::Other(_))
            && !custom.iter().any(|c| c.name == r.name)
        {
            custom.push(r.clone());
        }
    }
    // Keep the original part names and relationship ids of the property parts.
    for (rt, default) in [
        (rel_type::CORE_PROPERTIES, "/docProps/core.xml"),
        (rel_type::EXTENDED_PROPERTIES, "/docProps/app.xml"),
        (rel_type::CUSTOM_PROPERTIES, "/docProps/custom.xml"),
    ] {
        if rt == rel_type::CUSTOM_PROPERTIES && custom.is_empty() {
            continue;
        }
        let original = source
            .and_then(|s| s.package)
            .and_then(|p| p.related_part("/", rt).ok().flatten());
        let name = original.unwrap_or_else(|| default.to_owned());
        let ct = match rt {
            r if r == rel_type::CORE_PROPERTIES => content_type::CORE_PROPERTIES,
            r if r == rel_type::EXTENDED_PROPERTIES => content_type::EXTENDED_PROPERTIES,
            _ => content_type::CUSTOM_PROPERTIES,
        };
        if !ctx.pkg.has(&name) && ctx.pkg.add_part(&name, ct, Vec::new()).is_err() {
            continue;
        }
        ctx.rel("/", rt, &name, false);
    }
    PropertiesUpdate {
        core: Some(core),
        app: Some(app),
        custom: Some(custom),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_times_are_w3cdtf() {
        assert_eq!(super::w3cdtf(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::w3cdtf(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(super::w3cdtf(1_760_000_000), "2025-10-09T08:53:20Z");
        assert_eq!(super::w3cdtf(-1), "1969-12-31T23:59:59Z");
    }
}
