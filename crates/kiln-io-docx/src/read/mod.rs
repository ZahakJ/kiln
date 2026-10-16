// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`read_docx`]: a WordprocessingML package (`.docx`, `.docm`, `.dotx`, `.dotm`) → the Write
//! model ([`kiln_doc::Document`]) plus everything a faithful writer needs (PLAN.md §3.10):
//! passthrough parts, raw XML of unmodelled properties, relationship targets and diagnostics.
//! Pure Rust without I/O, so it runs unchanged in the web build.

mod context;
mod drawing;
mod parts;
mod props;
mod sdt;
mod sections;
mod story;
mod styles;
mod summary;
mod tables;

use std::collections::{BTreeMap, HashMap};

use kiln_doc::model::HeaderFooterPart;
use kiln_doc::settings::{CustomProperty, DocProperties, PropertyValue};
use kiln_doc::st::NoteKind;
use kiln_doc::{Block, DocTheme, Document, Paragraph, Section};
use kiln_ooxml::drawingml::theme::parse_theme;
use kiln_ooxml::opc::{OpcError, OpcLimits, OpcPackage, rel_type};
use kiln_ooxml::passthrough::{CollectOptions, PassthroughSet};
use kiln_ooxml::props::{CustomPropertyValue, DocumentProperties, read_document_properties};
use kiln_ooxml::xml::format_number;

pub use context::{ChartRef, RelTarget};
pub use summary::{OutlineEntry, Summary, snapshot_facts};

use crate::common::extras::DocxExtras;
use crate::common::warnings::{DocxWarning, Severity, WarningCode};
use crate::common::xml::{XmlPath, attr};
use context::ReadContext;
use parts::{
    prepare_comments, prepare_notes, read_comments, read_header_footer, read_notes, read_people,
};
use props::Raw;
use sections::parse_section;
use story::{BlockSink, StoryOptions, StoryReader};
use styles::{parse_numbering, parse_settings, parse_styles};

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    /// The bytes are not an OPC package (not a zip, encrypted, legacy `.doc`, too large).
    #[error(transparent)]
    Package(#[from] OpcError),
    /// The package is not a Word document, or its main part is unusable.
    #[error("not a Word document: {0}")]
    NotWord(String),
}

/// Which kind of WordprocessingML package the main part's content type names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocxKind {
    /// `.docx`.
    Document,
    /// `.docm` (macros preserved, never run).
    MacroEnabledDocument,
    /// `.dotx`.
    Template,
    /// `.dotm`.
    MacroEnabledTemplate,
}

impl DocxKind {
    /// The kind for a main part content type, if it is a Word one.
    #[must_use]
    pub fn from_content_type(ct: &str) -> Option<Self> {
        let ct = ct.trim();
        Some(match ct {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" => {
                Self::Document
            }
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml" => {
                Self::Template
            }
            "application/vnd.ms-word.document.macroEnabled.main+xml" => Self::MacroEnabledDocument,
            "application/vnd.ms-word.template.macroEnabledTemplate.main+xml" => {
                Self::MacroEnabledTemplate
            }
            _ => return None,
        })
    }

    /// The usual file extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Document => "docx",
            Self::MacroEnabledDocument => "docm",
            Self::Template => "dotx",
            Self::MacroEnabledTemplate => "dotm",
        }
    }
}

/// Options for [`read_docx_with`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOptions {
    /// Zip limits (zip-bomb protection) the package is opened with.
    pub limits: Option<OpcLimits>,
}

/// Everything [`read_docx`] produces.
#[derive(Debug)]
pub struct DocxRead {
    /// The document (valid: [`Document::validate`] passes).
    pub document: Document,
    /// Parts and relationships the reader did not model, for the writer to re-emit.
    pub passthrough: PassthroughSet,
    /// Raw XML and ids of what the model does not hold, for the writer.
    pub extras: DocxExtras,
    /// The package's document properties as read (core, app and custom, including values the
    /// model's [`DocProperties`] does not hold).
    pub package_properties: DocumentProperties,
    /// Relationship id → target, per source part, for ids referenced from kept XML.
    pub relationships: BTreeMap<String, BTreeMap<String, RelTarget>>,
    /// Charts in the document (their parts stay in the passthrough).
    pub charts: Vec<ChartRef>,
    /// The main part (`/word/document.xml` normally).
    pub main_part: String,
    /// Document, template, macro-enabled or not.
    pub kind: DocxKind,
    /// Diagnostics for the Compatibility Checker.
    pub warnings: Vec<DocxWarning>,
}

/// Reads a `.docx`/`.docm`/`.dotx`/`.dotm` file.
///
/// # Errors
///
/// [`DocxError::Package`] for bytes that are not a readable package, [`DocxError::NotWord`]
/// for a package that is not a Word document or whose main part is not well-formed.
pub fn read_docx(bytes: &[u8]) -> Result<DocxRead, DocxError> {
    read_docx_with(bytes, ReadOptions::default())
}

/// [`read_docx`] with options.
///
/// # Errors
///
/// As [`read_docx`].
pub fn read_docx_with(bytes: &[u8], options: ReadOptions) -> Result<DocxRead, DocxError> {
    let pkg = match options.limits {
        Some(l) => OpcPackage::open_with_limits(bytes, l)?,
        None => OpcPackage::open(bytes)?,
    };
    read_package(&pkg)
}

fn doc_properties(p: &DocumentProperties) -> DocProperties {
    let c = &p.core;
    DocProperties {
        title: c.title.clone(),
        subject: c.subject.clone(),
        creator: c.creator.clone(),
        keywords: c.keywords.clone(),
        description: c.description.clone(),
        last_modified_by: c.last_modified_by.clone(),
        revision: c.revision.as_deref().and_then(|r| r.trim().parse().ok()),
        created: c.created.clone(),
        modified: c.modified.clone(),
        category: c.category.clone(),
        content_status: c.content_status.clone(),
        language: c.language.clone(),
        company: p.app.company.clone(),
        manager: p.app.manager.clone(),
        template: p.app.template.clone(),
        total_time: p.app.total_time.and_then(|t| u32::try_from(t).ok()),
        custom: p
            .custom
            .iter()
            .filter_map(|cp| {
                let value = match &cp.value {
                    CustomPropertyValue::Text(t) => PropertyValue::Text(t.clone()),
                    CustomPropertyValue::Int(i) => PropertyValue::Integer(*i),
                    CustomPropertyValue::Number(n) => PropertyValue::Real(format_number(*n)),
                    CustomPropertyValue::Bool(b) => PropertyValue::Bool(*b),
                    CustomPropertyValue::FileTime(d) => PropertyValue::Date(d.clone()),
                    CustomPropertyValue::Other(_) => return None,
                };
                Some(CustomProperty {
                    name: cp.name.clone(),
                    value,
                })
            })
            .collect(),
    }
}

fn not_word(msg: impl Into<String>) -> DocxError {
    DocxError::NotWord(msg.into())
}

/// Reads an opened package.
///
/// # Errors
///
/// As [`read_docx`].
pub fn read_package(pkg: &OpcPackage) -> Result<DocxRead, DocxError> {
    let main = pkg
        .main_document_part()?
        .filter(|m| pkg.has(m))
        .ok_or_else(|| not_word("the package has no main document part"))?;
    let content_type = pkg.content_type(&main).unwrap_or_default().to_owned();
    let kind = DocxKind::from_content_type(&content_type).ok_or_else(|| {
        not_word(format!(
            "the main part is {}",
            if content_type.is_empty() {
                "without a content type".to_owned()
            } else {
                content_type.clone()
            }
        ))
    })?;
    let main_doc = match pkg.part_xml_with(&main, context::part_parse_options()) {
        Ok(Some(d)) => d,
        Ok(None) => return Err(not_word("the main part is missing")),
        Err(e) => return Err(not_word(e.to_string())),
    };
    if main_doc.root.name != "w:document" {
        return Err(not_word(format!(
            "the main part is a {}, not a w:document",
            main_doc.root.name
        )));
    }

    let mut ctx = ReadContext::new(pkg);
    ctx.consume(&main);
    let root = &main_doc.root;
    let raw = Raw { scope: &root.attrs };
    ctx.extras.document_root_attrs = root
        .attrs
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

    read_shared_parts(&mut ctx, &main);

    // ---- background
    if let Some(bg) = root.child("w:background") {
        ctx.extras.background = Some(raw.xml(bg));
        if let Some(color) = attr(bg, "w:color")
            && color.len() == 6
            && color.bytes().all(|b| b.is_ascii_hexdigit())
        {
            ctx.extras.settings.background_color = Some(color.to_ascii_uppercase());
        }
    }

    // ---- notes and comments: ids first, so every story can refer to them
    let footnotes = prepare_notes(&mut ctx, &main, NoteKind::Footnote);
    let endnotes = prepare_notes(&mut ctx, &main, NoteKind::Endnote);
    let comments = prepare_comments(&mut ctx, &main);
    if let Some(p) = &footnotes {
        read_notes(&mut ctx, p, NoteKind::Footnote);
    }
    if let Some(p) = &endnotes {
        read_notes(&mut ctx, p, NoteKind::Endnote);
    }
    if let Some(p) = &comments {
        read_comments(&mut ctx, &main, p);
    }
    ctx.extras.people = read_people(&mut ctx, &main);

    // ---- body
    let body = root.child("w:body");
    let mut groups = {
        let mut path = XmlPath::new("w:document");
        path.push("w:body[1]".to_owned());
        let mut story = StoryReader::new(
            &mut ctx,
            StoryOptions {
                part: main.clone(),
                scope: root.attrs.clone(),
                path,
                note: None,
            },
        );
        let mut sink = BlockSink::default();
        if let Some(body) = body {
            story.read_blocks(body, &mut sink);
        }
        sink.close_toc(story.ctx);
        let mut groups = std::mem::take(&mut sink.groups);
        if let Some(last) = groups.last_mut() {
            last.sect_pr = body.and_then(|b| b.child("w:sectPr")).cloned();
        }
        // A trailing empty group after a final paragraph-level section break holds nothing.
        if groups.len() > 1
            && groups
                .last()
                .is_some_and(|g| g.blocks.is_empty() && g.sect_pr.is_none())
        {
            groups.pop();
        }
        if let Some(last) = groups.last_mut() {
            story.finish_story(&mut last.blocks);
        }
        groups
    };

    // ---- sections with their headers and footers
    let mut hf_cache = HashMap::new();
    for g in &mut groups {
        let parsed = parse_section(g.sect_pr.as_ref(), raw);
        let id = ctx.doc.alloc_id();
        let mut section = Section {
            id,
            props: parsed.props,
            headers: Default::default(),
            footers: Default::default(),
            blocks: std::mem::take(&mut g.blocks),
        };
        for (refs, part_kind, list) in [
            (
                &mut section.headers,
                HeaderFooterPart::Header,
                &parsed.headers,
            ),
            (
                &mut section.footers,
                HeaderFooterPart::Footer,
                &parsed.footers,
            ),
        ] {
            for (kind, rel_id) in list {
                let target = ctx.relationship(&main, rel_id).filter(|t| !t.external);
                let Some(target) = target.filter(|t| pkg.has(&t.target)) else {
                    ctx.warn(
                        WarningCode::PartMissing,
                        Severity::Warning,
                        format!(
                            "A {} reference ({rel_id}) points to no part.",
                            if part_kind == HeaderFooterPart::Header {
                                "header"
                            } else {
                                "footer"
                            }
                        ),
                        &main,
                        "/w:document/w:body[1]/w:sectPr",
                    );
                    continue;
                };
                if let Some(node) =
                    read_header_footer(&mut ctx, &target.target, part_kind, &mut hf_cache)
                {
                    refs.set(*kind, Some(node));
                }
            }
        }
        if section.blocks.is_empty() {
            section
                .blocks
                .push(Block::Paragraph(Paragraph::new(ctx.doc.alloc_id())));
        }
        if parsed.extras != Default::default() {
            ctx.extras.sections.insert(id, parsed.extras);
        }
        ctx.doc.sections.push(section);
    }
    if ctx.doc.sections.is_empty() {
        let parsed = parse_section(None, raw);
        let id = ctx.doc.alloc_id();
        let p = Paragraph::new(ctx.doc.alloc_id());
        ctx.doc.sections.push(Section {
            id,
            props: parsed.props,
            headers: Default::default(),
            footers: Default::default(),
            blocks: vec![Block::Paragraph(p)],
        });
    }

    check_references(&mut ctx, &main);

    let consumed_rels: Vec<(String, Vec<String>)> = ctx
        .consumed_rels
        .iter()
        .map(|(s, ids)| (s.clone(), ids.iter().cloned().collect()))
        .collect();
    let passthrough = PassthroughSet::collect(
        pkg,
        &CollectOptions {
            consumed_parts: ctx.consumed_parts.iter().cloned().collect(),
            consumed_relationships: consumed_rels,
        },
    )?;
    for source in ctx.consumed_parts.clone() {
        ctx.record_all_relationships(&source);
    }
    ctx.record_all_relationships(&main);

    let package_properties = std::mem::take(&mut ctx.package_properties);
    let mut document = std::mem::replace(&mut ctx.doc, Document::empty());
    document.refresh_id_counter();
    Ok(DocxRead {
        document,
        passthrough,
        extras: ctx.extras,
        package_properties,
        relationships: ctx.relationships,
        charts: ctx.charts,
        main_part: main,
        kind,
        warnings: ctx.warnings.into_vec(),
    })
}

/// Theme, styles, numbering, settings and document properties.
fn read_shared_parts(ctx: &mut ReadContext<'_>, main: &str) {
    if let Some(part) = ctx.related_part(main, rel_type::THEME)
        && let Some(doc) = ctx.load_part(&part)
    {
        match parse_theme(&doc.root) {
            Ok(theme) => {
                ctx.doc.theme = DocTheme::from_ooxml(&theme);
                ctx.consume(&part);
            }
            Err(e) => ctx.warn(
                WarningCode::PartInvalidXml,
                Severity::Warning,
                format!("The theme could not be read ({e}); the Office theme is used instead."),
                &part,
                "/",
            ),
        }
    }

    if let Some(part) = ctx.related_part(main, rel_type::STYLES)
        && let Some(doc) = ctx.load_part(&part)
    {
        let parsed = parse_styles(
            &doc.root,
            Raw {
                scope: &doc.root.attrs,
            },
        );
        ctx.doc.styles = parsed.sheet;
        ctx.extras.latent_styles = parsed.latent_styles;
        ctx.extras.styles_xml = parsed.styles_xml;
        ctx.consume(&part);
    }

    if let Some(part) = ctx.related_part(main, rel_type::NUMBERING)
        && let Some(doc) = ctx.load_part(&part)
    {
        let parsed = parse_numbering(
            &doc.root,
            Raw {
                scope: &doc.root.attrs,
            },
        );
        ctx.doc.numbering = parsed.numbering;
        ctx.extras.num_pic_bullets = parsed.pic_bullets;
        ctx.extras.abstract_nums_xml = parsed.abstracts_xml;
        ctx.consume(&part);
    }

    let settings_doc = ctx
        .related_part(main, rel_type::SETTINGS)
        .and_then(|part| ctx.load_part(&part).map(|d| (part, d)));
    let (settings, extras) = parse_settings(settings_doc.as_ref().map(|(_, d)| &d.root));
    ctx.doc.settings = settings;
    ctx.extras.settings = extras;
    if let Some((part, doc)) = &settings_doc {
        ctx.consume(part);
        ctx.extras.settings_xml = Some(
            Raw {
                scope: &doc.root.attrs,
            }
            .xml(&doc.root),
        );
    }

    match read_document_properties(ctx.pkg) {
        Ok(p) => {
            ctx.doc.properties = doc_properties(&p);
            ctx.package_properties = p;
            for t in [
                rel_type::CORE_PROPERTIES,
                rel_type::EXTENDED_PROPERTIES,
                rel_type::CUSTOM_PROPERTIES,
            ] {
                if let Some(part) = ctx.related_part("/", t) {
                    ctx.consume(&part);
                }
            }
        }
        Err(e) => ctx.warn(
            WarningCode::PartInvalidXml,
            Severity::Warning,
            format!("The document properties could not be read ({e}); they are kept as they are."),
            "/docProps/core.xml",
            "/",
        ),
    }
}

fn check_references(ctx: &mut ReadContext<'_>, main: &str) {
    let missing: Vec<String> = ctx
        .referenced_comments
        .iter()
        .filter(|id| !ctx.comment_ids.contains_key(*id))
        .cloned()
        .collect();
    for id in missing {
        ctx.warn(
            WarningCode::CommentMissing,
            Severity::Warning,
            format!("A comment reference points to comment {id}, which the file does not contain."),
            main,
            "/",
        );
    }
    let unanchored: Vec<String> = ctx
        .doc
        .comments
        .iter()
        .filter(|c| !ctx.anchored_comments.contains(&c.id))
        .map(|c| {
            ctx.extras
                .comment_ids
                .get(&c.id)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    for id in unanchored {
        ctx.warn(
            WarningCode::CommentUnanchored,
            Severity::Info,
            format!(
                "Comment {id} does not cover any text; it is listed in the Comments pane without a position."
            ),
            main,
            &format!("/w:comments/w:comment[@w:id='{id}']"),
        );
    }
}
