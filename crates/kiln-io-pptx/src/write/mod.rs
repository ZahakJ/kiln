// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`write_pptx`]: the Deck model ([`kiln_deck::Presentation`]) → a PresentationML package
//! (`.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm`), the inverse of [`crate::read`]
//! (PLAN.md §3.10).
//!
//! - **Model parts** (the presentation, masters with their themes, layouts, slides, the notes
//!   master, notes pages, comments and their authors, document properties) are written from the
//!   model in Office's element order. Parts keep the names they were read from and new ones are
//!   numbered after them, so relationship ids, slide ids and the XML the reader kept stay valid.
//! - **Kept XML** (unmodelled shape and slide children, SmartArt and OLE frames, alternate
//!   content, ink, extension lists, transitions and timing Kiln approximates) is merged back in
//!   place; its relationships are re-created with their original ids.
//! - **Passthrough parts** (everything the reader did not model: presentation and view
//!   properties, table styles, the handout master, SmartArt and OLE parts, fonts, custom XML,
//!   thumbnails, VBA, printer settings, unknown parts) are copied byte for byte with their
//!   relationships repaired, before anything else is allocated. A relationships part whose
//!   relationships did not change keeps its original bytes too, and so does a theme the model
//!   did not change.
//! - **Determinism:** the same model, source and options give identical bytes. The only
//!   time-dependent values (`dcterms:modified`, the revision, the application name) are set
//!   from [`WriteOptions::now`] when it is given.
//!
//! Media and charts are written from [`kiln_deck::Presentation::parts`] under their part names
//! (a chart with its relationships, embedded workbook, style and colours). [`compat_report`]
//! lists what saving loses or changes (the Compatibility Checker).

mod comments;
mod compat;
mod context;
mod motion;
mod parts;
mod shapes;
mod text;
mod touched;
mod xml;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_common::NodeId;
use kiln_deck::{DeckError, Presentation};
use kiln_ooxml::opc::{
    OpcError, OpcPackage, is_rels_part, part_key, rel_type, source_part_of_rels, validate_part_name,
};
use kiln_ooxml::passthrough::PassthroughSet;
use kiln_ooxml::props::{DocumentProperties, PropertiesUpdate, write_document_properties};

pub use compat::{CompatIssue, CompatOptions, CompatOrigin, CompatSeverity, compat_report};
pub use context::WriteIssue;
pub use touched::TouchedParts;

use crate::common::extras::PptxExtras;
use crate::common::kind::{PptxKind, rel};
use crate::read::PptxRead;
use context::WriteContext;

/// Why a presentation could not be written.
#[derive(Debug, thiserror::Error)]
pub enum PptxWriteError {
    /// The model breaks an invariant ([`Presentation::validate`]); nothing was written.
    #[error("the presentation is not consistent: {0}")]
    InvalidPresentation(#[from] DeckError),
    /// The package could not be assembled.
    #[error(transparent)]
    Package(#[from] OpcError),
}

/// What the reader returned beside the presentation, for a faithful save.
#[derive(Debug, Clone, Copy)]
pub struct WriteSource<'a> {
    /// Parts and relationships the reader did not model.
    pub passthrough: &'a PassthroughSet,
    /// Part names, file ids and the XML the model does not hold.
    pub extras: &'a PptxExtras,
    /// Document properties as read.
    pub properties: &'a DocumentProperties,
    /// The main part name as read.
    pub main_part: &'a str,
    /// The format read.
    pub kind: PptxKind,
    /// The original package: lets the writer reuse relationship ids and part names, re-create
    /// the relationships kept XML refers to, and copy relationships parts and themes that did
    /// not change byte for byte.
    pub package: Option<&'a OpcPackage>,
}

impl<'a> WriteSource<'a> {
    /// The side data of a read.
    #[must_use]
    pub fn from_read(read: &'a PptxRead) -> Self {
        Self {
            passthrough: &read.passthrough,
            extras: &read.extras,
            properties: &read.properties,
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
    /// The target format (default: the format read, else `.pptx`). Saving a macro-enabled
    /// presentation as a format without macros removes its macros.
    pub kind: Option<PptxKind>,
    /// The save time (W3CDTF, `2026-10-16T09:30:00Z`): sets `dcterms:modified` (and
    /// `dcterms:created` when missing), increments `cp:revision` and stamps the application.
    /// `None` leaves the properties as read, which makes a rewrite deterministic.
    pub now: Option<String>,
    /// `cp:lastModifiedBy`; kept as read when absent.
    pub author: Option<String>,
    /// Write no passthrough parts.
    pub skip_passthrough: bool,
    /// Copy every slide, layout, master, notes and comment part these edits did not touch byte
    /// for byte from [`WriteSource::package`] (`None`: write every part from the model).
    pub preserve: Option<&'a TouchedParts>,
}

/// The result of a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxWritten {
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

const PML: &str = "application/vnd.openxmlformats-officedocument.presentationml";

fn pml(kind: &str) -> String {
    format!("{PML}.{kind}+xml")
}

fn is_macro_type(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [
        "vbaproject",
        "vbadata",
        "ms-office.activex",
        "ms-office.vbaproject",
    ]
    .iter()
    .any(|m| l.contains(m))
}

/// The passthrough to emit: macros only into macro-enabled formats.
fn filtered_passthrough(set: &PassthroughSet, macros: bool) -> PassthroughSet {
    if macros {
        return set.clone();
    }
    let mut out = PassthroughSet::new();
    let mut dropped: HashSet<String> = HashSet::new();
    for p in set.parts() {
        if is_macro_type(&p.content_type) {
            dropped.insert(part_key(&p.name));
        } else {
            out.record_part(p.clone());
        }
    }
    for r in set.relationships() {
        if is_macro_type(&r.rel.rel_type) {
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

/// The writer's part plan: the name of every part it produces and the list ids.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub main: String,
    pub masters: BTreeMap<NodeId, String>,
    /// Master → its theme part.
    pub themes: BTreeMap<NodeId, String>,
    pub layouts: BTreeMap<NodeId, String>,
    pub slides: BTreeMap<NodeId, String>,
    pub notes_master: Option<String>,
    /// A theme for a notes master the writer had to create.
    pub notes_master_theme: Option<String>,
    /// Slide → its notes page.
    pub notes: BTreeMap<NodeId, String>,
    /// Slide → its comments part.
    pub comments: BTreeMap<NodeId, String>,
    pub authors: Option<String>,
    pub modern_comments: bool,
    /// `p:sldId/@id`, `p:sldMasterId/@id`, `p:sldLayoutId/@id`.
    pub list_ids: BTreeMap<NodeId, u32>,
}

/// Hands out part names: the original one when it is free, else the first free number.
struct Namer {
    taken: HashSet<String>,
}

impl Namer {
    /// Claims a name read from the file, if it is a valid part name nobody took yet.
    fn keep(&mut self, original: &str) -> Option<String> {
        (original.starts_with('/')
            && validate_part_name(original).is_ok()
            && self.taken.insert(part_key(original)))
        .then(|| original.to_owned())
    }

    /// The first free name of a pattern with `{n}`.
    fn fresh(&mut self, pattern: &str) -> String {
        let name = (1u64..)
            .map(|n| pattern.replacen("{n}", &n.to_string(), 1))
            .find(|c| !self.taken.contains(&part_key(c)))
            .unwrap_or_else(|| pattern.replacen("{n}", "0", 1));
        self.taken.insert(part_key(&name));
        name
    }
}

/// List ids: the ones read when still unique and in range, else the next free ones.
fn plan_list_ids(pres: &Presentation, extras: &PptxExtras) -> BTreeMap<NodeId, u32> {
    const MASTER_MIN: u32 = 2_147_483_648;
    let mut out = BTreeMap::new();
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut pending_slides = Vec::new();
    for s in &pres.slides {
        match extras.list_ids.get(&s.id) {
            Some(&id) if (256..MASTER_MIN).contains(&id) && used.insert(id) => {
                out.insert(s.id, id);
            }
            _ => pending_slides.push(s.id),
        }
    }
    let mut next = used.iter().next_back().copied().unwrap_or(255);
    for id in pending_slides {
        next = next.saturating_add(1).max(256);
        out.insert(id, next);
    }
    // Masters and layouts share one id space above 2^31.
    let mut owners = Vec::new();
    for m in &pres.masters {
        owners.push(m.id);
        owners.extend(m.layouts.iter().map(|l| l.id));
    }
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut pending = Vec::new();
    for id in owners {
        match extras.list_ids.get(&id) {
            Some(&v) if v >= MASTER_MIN && used.insert(v) => {
                out.insert(id, v);
            }
            _ => pending.push(id),
        }
    }
    let mut next = used.iter().next_back().copied().unwrap_or(MASTER_MIN - 1);
    for id in pending {
        next = next.saturating_add(1).max(MASTER_MIN);
        out.insert(id, next);
    }
    out
}

/// What a planned part name is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Main,
    Master(NodeId),
    Layout(NodeId),
    Theme(NodeId),
    NotesMaster,
    NotesMasterTheme,
    Slide(NodeId),
    Notes(NodeId),
    Comments(NodeId),
    Authors,
}

fn plan(
    pres: &Presentation,
    extras: &PptxExtras,
    main_original: Option<&str>,
    passthrough: &PassthroughSet,
    ctx: &mut WriteContext<'_>,
) -> Plan {
    let original = |id: NodeId| extras.part_names.get(&id).cloned();
    // Every part the writer produces: its slot, the name it was read from, the pattern for a
    // new name.
    let mut wanted: Vec<(Slot, Option<String>, String)> = vec![(
        Slot::Main,
        Some(main_original.unwrap_or("/ppt/presentation.xml").to_owned()),
        "/ppt/presentation{n}.xml".to_owned(),
    )];
    for m in &pres.masters {
        wanted.push((
            Slot::Master(m.id),
            original(m.id),
            "/ppt/slideMasters/slideMaster{n}.xml".to_owned(),
        ));
        for l in &m.layouts {
            wanted.push((
                Slot::Layout(l.id),
                original(l.id),
                "/ppt/slideLayouts/slideLayout{n}.xml".to_owned(),
            ));
        }
    }
    for m in &pres.masters {
        wanted.push((
            Slot::Theme(m.id),
            extras.theme_parts.get(&m.id).cloned(),
            "/ppt/theme/theme{n}.xml".to_owned(),
        ));
    }
    let needs_notes = pres.notes_master.is_some() || pres.slides.iter().any(|s| s.notes.is_some());
    if needs_notes {
        wanted.push((
            Slot::NotesMaster,
            pres.notes_master.as_ref().and_then(|n| original(n.id)),
            "/ppt/notesMasters/notesMaster{n}.xml".to_owned(),
        ));
        if pres.notes_master.is_none() {
            wanted.push((
                Slot::NotesMasterTheme,
                None,
                "/ppt/theme/theme{n}.xml".to_owned(),
            ));
        }
    }
    for s in &pres.slides {
        wanted.push((
            Slot::Slide(s.id),
            original(s.id),
            "/ppt/slides/slide{n}.xml".to_owned(),
        ));
    }
    for s in &pres.slides {
        if s.notes.is_some() {
            wanted.push((
                Slot::Notes(s.id),
                extras.notes_parts.get(&s.id).cloned(),
                "/ppt/notesSlides/notesSlide{n}.xml".to_owned(),
            ));
        }
    }
    let modern_comments = comments::use_modern(pres, extras.modern_comments);
    let (comment_rel, authors_rel) = if modern_comments {
        (rel::MODERN_COMMENTS, rel::AUTHORS)
    } else {
        (rel::COMMENTS, rel::COMMENT_AUTHORS)
    };
    let mut any_comments = false;
    for s in &pres.slides {
        if s.comments.is_empty() {
            continue;
        }
        any_comments = true;
        let orig = extras
            .part_names
            .get(&s.id)
            .and_then(|sp| ctx.original_target(sp, comment_rel));
        let pattern = if modern_comments {
            format!(
                "/ppt/comments/modernComment_{}{{n}}.xml",
                stable_suffix(s.id)
            )
        } else {
            "/ppt/comments/comment{n}.xml".to_owned()
        };
        wanted.push((Slot::Comments(s.id), orig, pattern));
    }
    if any_comments {
        let orig = main_original
            .and_then(|m| ctx.original_target(m, authors_rel))
            .unwrap_or_else(|| {
                if modern_comments {
                    "/ppt/authors.xml".to_owned()
                } else {
                    "/ppt/commentAuthors.xml".to_owned()
                }
            });
        let pattern = if modern_comments {
            "/ppt/authors{n}.xml"
        } else {
            "/ppt/commentAuthors{n}.xml"
        };
        wanted.push((Slot::Authors, Some(orig), pattern.to_owned()));
    }

    // Names read keep their names first (so a new part never takes the name of a part that
    // comes later in the list), then new parts take the first free numbers.
    let mut namer = Namer {
        taken: passthrough.parts().map(|p| part_key(&p.name)).collect(),
    };
    let mut names: Vec<Option<String>> = wanted
        .iter()
        .map(|(_, orig, _)| orig.as_deref().and_then(|o| namer.keep(o)))
        .collect();
    for ((_, _, pattern), name) in wanted.iter().zip(names.iter_mut()) {
        if name.is_none() {
            *name = Some(namer.fresh(pattern));
        }
    }
    let mut p = Plan {
        modern_comments,
        ..Plan::default()
    };
    for ((slot, _, _), name) in wanted.into_iter().zip(names) {
        let name = name.unwrap_or_default();
        match slot {
            Slot::Main => p.main = name,
            Slot::Master(id) => {
                p.masters.insert(id, name);
            }
            Slot::Layout(id) => {
                p.layouts.insert(id, name);
            }
            Slot::Theme(id) => {
                p.themes.insert(id, name);
            }
            Slot::NotesMaster => p.notes_master = Some(name),
            Slot::NotesMasterTheme => p.notes_master_theme = Some(name),
            Slot::Slide(id) => {
                p.slides.insert(id, name);
            }
            Slot::Notes(id) => {
                p.notes.insert(id, name);
            }
            Slot::Comments(id) => {
                p.comments.insert(id, name);
            }
            Slot::Authors => p.authors = Some(name),
        }
    }
    p.list_ids = plan_list_ids(pres, extras);
    p
}

/// A short stable suffix for new modern comment parts.
fn stable_suffix(id: NodeId) -> String {
    let g = xml::stable_guid(&format!("comments:{}", id.0));
    format!("{}_", &g[1..9])
}

/// The relationships of a part as comparable facts.
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

/// Puts back the original bytes of every relationships part whose relationships did not change;
/// returns their keys.
fn restore_unchanged_rels(
    ctx: &mut WriteContext<'_>,
    orig: &OpcPackage,
) -> Result<BTreeSet<String>, OpcError> {
    let mut restored = BTreeSet::new();
    let rels_parts: Vec<String> = ctx
        .pkg
        .all_part_names()
        .filter(|n| is_rels_part(n))
        .map(str::to_owned)
        .collect();
    for r in rels_parts {
        let Some(source) = source_part_of_rels(&r) else {
            continue;
        };
        if !orig.has(&r) {
            continue;
        }
        if rel_facts(&ctx.pkg, &source) == rel_facts(orig, &source)
            && let Ok(bytes) = orig.part_bytes(&r)
        {
            let bytes = bytes.into_owned();
            ctx.pkg.set_part(&r, bytes)?;
            restored.insert(part_key(&r));
        }
    }
    Ok(restored)
}

/// The document properties to write: as read, with the counts of the presentation and the save
/// stamp.
fn properties_update(
    pres: &Presentation,
    read: &DocumentProperties,
    options: &WriteOptions<'_>,
) -> PropertiesUpdate {
    let mut core = read.core.clone();
    if let Some(now) = &options.now {
        core.modified = Some(now.clone());
        if core.created.is_none() {
            core.created = Some(now.clone());
        }
        core.revision = match core
            .revision
            .as_deref()
            .and_then(|r| r.trim().parse::<u64>().ok())
        {
            Some(r) => Some((r + 1).to_string()),
            None if core.revision.is_none() => Some("1".to_owned()),
            None => core.revision.clone(),
        };
    }
    if let Some(a) = &options.author {
        core.last_modified_by = Some(a.clone());
    }
    let mut app = read.app.clone();
    let count = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    app.slides = Some(count(pres.slides.len()));
    app.notes = Some(count(
        pres.slides.iter().filter(|s| s.notes.is_some()).count(),
    ));
    app.hidden_slides = Some(count(pres.slides.iter().filter(|s| s.hidden).count()));
    if options.now.is_some() {
        app.application = Some("Kiln".to_owned());
        app.app_version = None;
    }
    PropertiesUpdate {
        core: Some(core),
        app: Some(app),
        custom: Some(read.custom.clone()),
    }
}

/// Writes a presentation. `source` is what the reader returned beside it (when the
/// presentation was read from a file); without it a new package is built.
///
/// # Errors
///
/// [`PptxWriteError::InvalidPresentation`] when the model is not consistent,
/// [`PptxWriteError::Package`] when the package cannot be assembled (an invalid part name in
/// the passthrough, for example).
pub fn write_pptx(
    pres: &Presentation,
    source: Option<&WriteSource<'_>>,
    options: &WriteOptions<'_>,
) -> Result<PptxWritten, PptxWriteError> {
    pres.validate()?;
    let default_extras = PptxExtras::default();
    let empty_passthrough = PassthroughSet::new();
    let empty_props = DocumentProperties::default();
    let extras = source.map_or(&default_extras, |s| s.extras);
    let original_pkg = source.and_then(|s| s.package);
    let kind = options
        .kind
        .or(source.map(|s| s.kind))
        .unwrap_or(PptxKind::Presentation);
    let macros = matches!(
        kind,
        PptxKind::MacroEnabledPresentation
            | PptxKind::MacroEnabledTemplate
            | PptxKind::MacroEnabledShow
    );
    let passthrough = if options.skip_passthrough {
        PassthroughSet::new()
    } else {
        filtered_passthrough(source.map_or(&empty_passthrough, |s| s.passthrough), macros)
    };
    let main_original = source.map(|s| s.main_part.to_owned());

    let mut ctx = WriteContext::new(OpcPackage::new(), pres, original_pkg);
    let plan = plan(
        pres,
        extras,
        main_original.as_deref(),
        &passthrough,
        &mut ctx,
    );

    // ---- placeholders, so relationships to and from them survive the passthrough
    let mut produced: Vec<(String, String)> =
        vec![(plan.main.clone(), kind.content_type().to_owned())];
    for m in &pres.masters {
        produced.push((plan.masters[&m.id].clone(), pml("slideMaster")));
        for l in &m.layouts {
            produced.push((plan.layouts[&l.id].clone(), pml("slideLayout")));
        }
    }
    for t in plan.themes.values() {
        produced.push((t.clone(), kiln_ooxml::opc::content_type::THEME.to_owned()));
    }
    if let Some(n) = &plan.notes_master {
        produced.push((n.clone(), pml("notesMaster")));
    }
    if let Some(t) = &plan.notes_master_theme {
        produced.push((t.clone(), kiln_ooxml::opc::content_type::THEME.to_owned()));
    }
    for s in &pres.slides {
        produced.push((plan.slides[&s.id].clone(), pml("slide")));
    }
    for s in &pres.slides {
        if let Some(n) = plan.notes.get(&s.id) {
            produced.push((n.clone(), pml("notesSlide")));
        }
    }
    let (comment_ct, authors_ct) = if plan.modern_comments {
        (comments::MODERN_CT, comments::MODERN_AUTHORS_CT)
    } else {
        (comments::LEGACY_CT, comments::LEGACY_AUTHORS_CT)
    };
    for s in &pres.slides {
        if let Some(c) = plan.comments.get(&s.id) {
            produced.push((c.clone(), comment_ct.to_owned()));
        }
    }
    if let Some(a) = &plan.authors {
        produced.push((a.clone(), authors_ct.to_owned()));
    }
    for (name, ct) in &produced {
        ctx.pkg.add_part(name, ct, Vec::new())?;
    }

    // ---- original → new names, for the passthrough's relationships
    let mut orig_to_new: HashMap<String, String> = HashMap::new();
    if let Some(m) = &main_original {
        orig_to_new.insert(part_key(m), plan.main.clone());
    }
    let mut map_owner = |id: NodeId, new: &String| {
        if let Some(o) = extras.part_names.get(&id) {
            orig_to_new.insert(part_key(o), new.clone());
        }
    };
    for (id, n) in plan.masters.iter().chain(&plan.layouts).chain(&plan.slides) {
        map_owner(*id, n);
    }
    if let (Some(nm), Some(n)) = (&pres.notes_master, &plan.notes_master) {
        map_owner(nm.id, n);
    }
    for (master, theme) in &plan.themes {
        if let Some(o) = extras.theme_parts.get(master) {
            orig_to_new.insert(part_key(o), theme.clone());
        }
    }
    for (slide, n) in &plan.notes {
        if let Some(o) = extras.notes_parts.get(slide) {
            orig_to_new.insert(part_key(o), n.clone());
        }
    }

    // ---- passthrough first: its relationship ids are kept as written
    let emitted = passthrough.emit(&mut ctx.pkg, |n| orig_to_new.get(&part_key(n)).cloned())?;
    for d in &emitted.dropped {
        // Relationships of slides, layouts or masters that are no longer in the presentation go
        // with them.
        let gone = extras
            .part_names
            .values()
            .any(|p| part_key(p) == part_key(&d.source))
            && !orig_to_new.contains_key(&part_key(&d.source));
        if !gone {
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
                &d.source.clone(),
            );
        }
    }
    for (name, _) in &produced {
        ctx.produced(name);
    }

    // ---- package relationships
    ctx.rel("/", rel_type::OFFICE_DOCUMENT, &plan.main, false);

    // ---- the parts
    let preserve = options.preserve.filter(|_| original_pkg.is_some());
    parts::write_all(&mut ctx, pres, extras, &plan, preserve)?;

    // ---- document properties
    let read_props = source.map_or(&empty_props, |s| s.properties);
    for (rt, name, ct) in [
        (
            rel_type::CORE_PROPERTIES,
            "/docProps/core.xml",
            kiln_ooxml::opc::content_type::CORE_PROPERTIES,
        ),
        (
            rel_type::EXTENDED_PROPERTIES,
            "/docProps/app.xml",
            kiln_ooxml::opc::content_type::EXTENDED_PROPERTIES,
        ),
    ] {
        // The names and package relationship ids the file had.
        let target = original_pkg
            .and_then(|o| o.related_part("/", rt).ok().flatten())
            .filter(|t| !ctx.pkg.has(t))
            .unwrap_or_else(|| name.to_owned());
        if !ctx.pkg.has(&target) {
            ctx.pkg.add_part(&target, ct, Vec::new())?;
        }
        ctx.rel("/", rt, &target, false);
    }
    if !read_props.custom.is_empty() {
        let target = original_pkg
            .and_then(|o| {
                o.related_part("/", rel_type::CUSTOM_PROPERTIES)
                    .ok()
                    .flatten()
            })
            .filter(|t| !ctx.pkg.has(t))
            .unwrap_or_else(|| "/docProps/custom.xml".to_owned());
        if !ctx.pkg.has(&target) {
            ctx.pkg.add_part(
                &target,
                kiln_ooxml::opc::content_type::CUSTOM_PROPERTIES,
                Vec::new(),
            )?;
        }
        ctx.rel("/", rel_type::CUSTOM_PROPERTIES, &target, false);
    }
    write_document_properties(&mut ctx.pkg, &properties_update(pres, read_props, options))?;
    let mut regenerated_extra: Vec<String> = Vec::new();
    for rt in [
        rel_type::CORE_PROPERTIES,
        rel_type::EXTENDED_PROPERTIES,
        rel_type::CUSTOM_PROPERTIES,
    ] {
        if let Ok(Some(p)) = ctx.pkg.related_part("/", rt) {
            regenerated_extra.push(p);
        }
    }

    // ---- themes the model did not change: the original bytes
    let mut copied: BTreeSet<String> = BTreeSet::new();
    if let Some(orig) = original_pkg {
        copied = parts::restore_unchanged_themes(&mut ctx, pres, extras, &plan, orig)?;
    }

    // ---- relationships parts that did not change: the original bytes
    let restored = match original_pkg {
        Some(orig) => restore_unchanged_rels(&mut ctx, orig)?,
        None => BTreeSet::new(),
    };

    // ---- what was regenerated
    let mut regenerated: Vec<String> = vec![kiln_ooxml::opc::CONTENT_TYPES_PART.to_owned()];
    for (name, _) in &produced {
        let key = part_key(name);
        if !copied.contains(&key) && !ctx.preserved.contains(&key) {
            regenerated.push(name.clone());
        }
    }
    regenerated.extend(regenerated_extra);
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
    let mut preserved: Vec<String> = ctx
        .pkg
        .all_part_names()
        .filter(|n| ctx.preserved.contains(&part_key(n)))
        .map(str::to_owned)
        .collect();
    preserved.sort();
    Ok(PptxWritten {
        bytes,
        regenerated,
        dropped,
        preserved,
        issues: ctx.issues,
    })
}

/// Why [`rewrite_pptx`] failed.
#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    /// The input could not be read.
    #[error("read: {0}")]
    Read(crate::PptxError),
    /// The presentation could not be written.
    #[error("write: {0}")]
    Write(PptxWriteError),
}

/// Reads and writes a file without edits, every model part written from the model: the round
/// trip the fidelity scorer and `kiln-cli convert` measure.
///
/// # Errors
///
/// As [`crate::read_pptx`] and [`write_pptx`].
pub fn rewrite_pptx(bytes: &[u8], options: &WriteOptions<'_>) -> Result<PptxWritten, RewriteError> {
    let pkg =
        OpcPackage::open(bytes).map_err(|e| RewriteError::Read(crate::PptxError::Package(e)))?;
    let read = crate::read::read_package(&pkg).map_err(RewriteError::Read)?;
    let source = WriteSource::from_read(&read).with_package(&pkg);
    write_pptx(&read.presentation, Some(&source), options).map_err(RewriteError::Write)
}
