// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`read_pptx`]: a PresentationML package (`.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`,
//! `.ppsm`) → the Deck model ([`kiln_deck::Presentation`]) plus everything a faithful writer
//! needs (PLAN.md §3.10): passthrough parts, the original XML of what the model approximates,
//! part names and file ids, and diagnostics. Pure Rust without I/O, so it runs unchanged in the
//! web build.
//!
//! Conventions of the model built here:
//! - **Media references are part keys.** Pictures, picture fills, picture bullets, media and
//!   charts refer to [`kiln_deck::Presentation::parts`] by the part name they were read from
//!   (`/ppt/media/image1.png`), never by relationship id. A chart's relationships part
//!   (`/ppt/charts/_rels/chart1.xml.rels`) and its targets (embedded workbook, style, colours)
//!   are stored next to it.
//! - **Links** on shapes and text are URLs, `#slide:<id>` for a jump to a slide, or the
//!   `ppaction://` verb for other actions (next slide, end show, …).
//! - **Fields** (`a:fld`) are kiln-doc simple fields whose instruction is the DrawingML field
//!   type (`slidenum`, `datetime1`, …) and whose result is the text the file shows.

mod context;
mod masters;
mod motion;
mod presentation;
mod shapes;
mod slides;
mod smartart;
mod summary;
mod table;
mod text;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::Presentation;
use kiln_ooxml::opc::{OpcError, OpcLimits, OpcPackage};
use kiln_ooxml::passthrough::{CollectOptions, PassthroughSet};
use kiln_ooxml::props::{DocumentProperties, read_document_properties};
use kiln_ooxml::xml::{ParseOptions, XmlDocument};

pub use summary::{SlideSummary, Summary, body_text, plain_text, shape_kind, snapshot_facts};

use crate::common::extras::PptxExtras;
use crate::common::kind::PptxKind;
use crate::common::warnings::{PptxWarning, Severity, WarningCode, WarningCollector};

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PptxError {
    /// The bytes are not an OPC package (not a zip, encrypted, legacy `.ppt`, too large).
    #[error(transparent)]
    Package(#[from] OpcError),
    /// The package is not a presentation, or its main part is unusable.
    #[error("not a presentation: {0}")]
    NotPresentation(String),
    /// The reader built a model the Deck invariants reject (a reader bug; reported, never a
    /// panic).
    #[error("the presentation could not be built: {0}")]
    Invalid(String),
}

/// Options for [`read_pptx_with`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOptions {
    /// Zip limits (zip-bomb protection) the package is opened with.
    pub limits: Option<OpcLimits>,
}

/// Everything [`read_pptx`] produces.
#[derive(Debug)]
pub struct PptxRead {
    /// The presentation ([`Presentation::validate`] passes).
    pub presentation: Presentation,
    /// Parts and relationships the reader did not model, for the writer to re-emit.
    pub passthrough: PassthroughSet,
    /// Part names, file ids and the original XML of what the model does not hold.
    pub extras: PptxExtras,
    /// The package's document properties (core, app and custom).
    pub properties: DocumentProperties,
    /// The main part (`/ppt/presentation.xml` normally).
    pub main_part: String,
    /// Presentation, template, show, macro-enabled or not.
    pub kind: PptxKind,
    /// Diagnostics for the Compatibility Checker.
    pub warnings: Vec<PptxWarning>,
}

/// Reads a `.pptx`/`.pptm`/`.potx`/`.potm`/`.ppsx`/`.ppsm` file.
///
/// # Errors
///
/// [`PptxError::Package`] for bytes that are not a readable package,
/// [`PptxError::NotPresentation`] for a package that is not a presentation or whose main part
/// is not well-formed.
pub fn read_pptx(bytes: &[u8]) -> Result<PptxRead, PptxError> {
    read_pptx_with(bytes, ReadOptions::default())
}

/// [`read_pptx`] with options.
///
/// # Errors
///
/// As [`read_pptx`].
pub fn read_pptx_with(bytes: &[u8], options: ReadOptions) -> Result<PptxRead, PptxError> {
    let pkg = match options.limits {
        Some(l) => OpcPackage::open_with_limits(bytes, l)?,
        None => OpcPackage::open(bytes)?,
    };
    read_package(&pkg)
}

/// How the reader parses parts: content nested beyond the XML parser's depth limit is kept
/// verbatim instead of refusing the part, so such a file still opens.
pub(crate) fn parse_options() -> ParseOptions {
    ParseOptions {
        keep_too_deep_as_raw: true,
        ..ParseOptions::default()
    }
}

/// The deepest group nesting the reader follows; deeper groups are kept as XML only.
pub(crate) const MAX_GROUP_DEPTH: usize = 32;

/// The reader's state while it walks the package.
pub(crate) struct Reader<'p> {
    pub pkg: &'p OpcPackage,
    pub pres: Presentation,
    pub extras: PptxExtras,
    pub warnings: WarningCollector,
    consumed: BTreeSet<String>,
    consumed_rels: BTreeMap<String, BTreeSet<String>>,
    /// Slide part → its (pre-allocated) id, so links can jump forwards.
    pub slide_ids: HashMap<String, NodeId>,
    /// Layout part → id.
    pub layout_ids: HashMap<String, NodeId>,
}

impl<'p> Reader<'p> {
    fn new(pkg: &'p OpcPackage) -> Self {
        Self {
            pkg,
            pres: Presentation::empty(),
            extras: PptxExtras::default(),
            warnings: WarningCollector::default(),
            consumed: BTreeSet::new(),
            consumed_rels: BTreeMap::new(),
            slide_ids: HashMap::new(),
            layout_ids: HashMap::new(),
        }
    }

    /// Marks a part as modelled (the writer regenerates or re-stores it).
    pub fn consume(&mut self, part: &str) {
        self.consumed.insert(part.to_owned());
    }

    /// Marks a relationship as modelled although its target is not a consumed part (an
    /// external hyperlink).
    pub fn consume_rel(&mut self, source: &str, id: &str) {
        self.consumed_rels
            .entry(source.to_owned())
            .or_default()
            .insert(id.to_owned());
    }

    /// Records a diagnostic.
    pub fn warn(
        &mut self,
        code: WarningCode,
        severity: Severity,
        message: impl Into<String>,
        part: &str,
        location: impl Into<String>,
    ) {
        self.warnings.push(code, severity, message, part, location);
    }

    /// Parses a part. Missing and malformed parts are reported and give `None`.
    pub fn part_xml(&mut self, part: &str, from: &str) -> Option<XmlDocument> {
        match self.pkg.part_xml_with(part, parse_options()) {
            Ok(Some(d)) => Some(d),
            Ok(None) => {
                self.warn(
                    WarningCode::PartMissing,
                    Severity::Warning,
                    format!("{from} refers to a part that does not exist"),
                    part,
                    "",
                );
                None
            }
            Err(e) => {
                self.warn(
                    WarningCode::PartInvalidXml,
                    Severity::Warning,
                    format!("the part is not readable ({e}); it is kept but not shown"),
                    part,
                    "",
                );
                None
            }
        }
    }
}

fn not_presentation(msg: impl Into<String>) -> PptxError {
    PptxError::NotPresentation(msg.into())
}

/// Reads an opened package.
///
/// # Errors
///
/// As [`read_pptx`].
pub fn read_package(pkg: &OpcPackage) -> Result<PptxRead, PptxError> {
    let main = pkg
        .main_document_part()?
        .filter(|m| pkg.has(m))
        .ok_or_else(|| not_presentation("the package has no main part"))?;
    let content_type = pkg.content_type(&main).unwrap_or_default().to_owned();
    let kind = PptxKind::from_content_type(&content_type).ok_or_else(|| {
        not_presentation(format!(
            "the main part is {}",
            if content_type.is_empty() {
                "without a content type".to_owned()
            } else {
                content_type.clone()
            }
        ))
    })?;
    let doc = match pkg.part_xml_with(&main, parse_options()) {
        Ok(Some(d)) => d,
        Ok(None) => return Err(not_presentation("the main part is missing")),
        Err(e) => return Err(not_presentation(e.to_string())),
    };
    if doc.root.name != "p:presentation" {
        return Err(not_presentation(format!(
            "the main part is a {}, not a p:presentation",
            doc.root.name
        )));
    }

    let mut r = Reader::new(pkg);
    r.consume(&main);
    let properties = read_properties(&mut r);
    presentation::read(&mut r, &main, &doc.root);

    r.pres.refresh_id_counter();
    r.pres
        .validate()
        .map_err(|e| PptxError::Invalid(e.to_string()))?;

    let options = CollectOptions {
        consumed_parts: r.consumed.iter().cloned().collect(),
        consumed_relationships: r
            .consumed_rels
            .iter()
            .map(|(s, ids)| (s.clone(), ids.iter().cloned().collect()))
            .collect(),
    };
    let passthrough = PassthroughSet::collect(pkg, &options)?;
    Ok(PptxRead {
        presentation: r.pres,
        passthrough,
        extras: r.extras,
        properties,
        main_part: main,
        kind,
        warnings: r.warnings.into_vec(),
    })
}

/// Reads the document properties and, when they are readable, marks their parts consumed (the
/// writer regenerates them from [`PptxRead::properties`]); unreadable ones stay in the
/// passthrough as they are.
fn read_properties(r: &mut Reader<'_>) -> DocumentProperties {
    use kiln_ooxml::opc::rel_type;
    match read_document_properties(r.pkg) {
        Ok(p) => {
            for t in [
                rel_type::CORE_PROPERTIES,
                rel_type::EXTENDED_PROPERTIES,
                rel_type::CUSTOM_PROPERTIES,
            ] {
                if let Ok(Some(part)) = r.pkg.related_part("/", t) {
                    r.consume(&part);
                }
            }
            p
        }
        Err(e) => {
            r.warn(
                WarningCode::PartInvalidXml,
                Severity::Warning,
                format!(
                    "the document properties are not readable ({e}); they are kept as they are"
                ),
                "/docProps/core.xml",
                "",
            );
            DocumentProperties::default()
        }
    }
}
