// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package-wide state of one read: the OPC package, the document being built (its id allocator
//! and media), what was consumed (everything else becomes passthrough), id maps for comments,
//! notes and bookmarks, and diagnostics.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kiln_doc::st::NoteKind;
use kiln_doc::{Document, NodeId};
use kiln_ooxml::opc::OpcPackage;
use kiln_ooxml::props::DocumentProperties;
use kiln_ooxml::xml::{ParseOptions, XmlDocument};

use crate::common::extras::DocxExtras;
use crate::common::warnings::{DocxWarning, Severity, WarningCode, WarningCollector};

/// A relationship target as the reader resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelTarget {
    /// The relationship type URI.
    pub rel_type: String,
    /// The part name for internal targets, the URL for external ones.
    pub target: String,
    /// Whether the target is outside the package.
    pub external: bool,
}

/// A chart in the document, kept for the charts work (the chart part stays passthrough).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartRef {
    /// The part holding the drawing (`/word/document.xml`, a header part, …).
    pub source_part: String,
    /// The relationship id of the chart from `source_part`.
    pub rel_id: String,
    /// The resolved chart part (`/word/charts/chart1.xml`).
    pub chart_part: Option<String>,
    /// The drawing's id in the model.
    pub drawing: NodeId,
    /// Extent in EMU.
    pub width: i64,
    /// Extent in EMU.
    pub height: i64,
    /// Floating (`wp:anchor`) rather than inline.
    pub anchored: bool,
}

pub(crate) struct ReadContext<'p> {
    pub pkg: &'p OpcPackage,
    pub doc: Document,
    pub extras: DocxExtras,
    pub warnings: WarningCollector,
    pub consumed_parts: BTreeSet<String>,
    pub consumed_rels: BTreeMap<String, BTreeSet<String>>,
    pub relationships: BTreeMap<String, BTreeMap<String, RelTarget>>,
    pub charts: Vec<ChartRef>,
    /// `w:comment/@w:id` → comment node.
    pub comment_ids: HashMap<String, NodeId>,
    /// Comments whose range marked at least one character.
    pub anchored_comments: HashSet<NodeId>,
    /// Comment ids referenced from a story (`w:commentReference`).
    pub referenced_comments: BTreeSet<String>,
    /// `(kind, w:id)` → note node.
    pub note_ids: HashMap<(NoteKind, String), NodeId>,
    /// `w:bookmarkStart/@w:id` → name, for the matching `w:bookmarkEnd`.
    pub bookmark_names: HashMap<String, String>,
    /// The package's document properties as read.
    pub package_properties: DocumentProperties,
    /// How deeply the reader is nested in containers (cells, content controls, wrappers, text
    /// boxes); bounded by [`MAX_DEPTH`] so hostile nesting cannot exhaust the stack.
    pub depth: usize,
    /// How many text boxes the reader is inside (Word cannot put a text box in a text box; files
    /// that do anyway are bounded by [`MAX_TEXT_BOX_DEPTH`]).
    pub text_box_depth: usize,
}

/// How the reader parses parts: content nested beyond the XML parser's depth limit is kept
/// verbatim ([`kiln_ooxml::xml::XmlNode::Raw`]) instead of refusing the part, so such a document
/// opens and its over-deep content (already beyond [`MAX_DEPTH`], hence opaque) saves unchanged.
pub(crate) fn part_parse_options() -> ParseOptions {
    ParseOptions {
        keep_too_deep_as_raw: true,
        ..ParseOptions::default()
    }
}

/// The deepest text box nesting whose text the reader follows; deeper text boxes keep their XML
/// only.
pub(crate) const MAX_TEXT_BOX_DEPTH: usize = 4;

/// The deepest container nesting the reader follows (Word documents rarely nest tables more
/// than a few levels; the XML parser itself stops at 2,000 elements). Deeper tables are kept as
/// opaque XML, deeper wrappers are dropped with a diagnostic.
pub(crate) const MAX_DEPTH: usize = 64;

impl<'p> ReadContext<'p> {
    pub fn new(pkg: &'p OpcPackage) -> Self {
        Self {
            pkg,
            doc: Document::empty(),
            extras: DocxExtras::default(),
            warnings: WarningCollector::default(),
            consumed_parts: BTreeSet::new(),
            consumed_rels: BTreeMap::new(),
            relationships: BTreeMap::new(),
            charts: Vec::new(),
            comment_ids: HashMap::new(),
            anchored_comments: HashSet::new(),
            referenced_comments: BTreeSet::new(),
            note_ids: HashMap::new(),
            bookmark_names: HashMap::new(),
            package_properties: DocumentProperties::default(),
            depth: 0,
            text_box_depth: 0,
        }
    }

    pub fn warn(
        &mut self,
        code: WarningCode,
        severity: Severity,
        message: impl Into<String>,
        part: &str,
        path: &str,
    ) {
        self.warnings.add(DocxWarning {
            code,
            severity,
            message: message.into(),
            part: part.to_owned(),
            path: path.to_owned(),
        });
    }

    pub fn consume(&mut self, part: &str) {
        self.consumed_parts.insert(part.to_owned());
    }

    pub fn consume_rel(&mut self, source: &str, id: &str) {
        self.consumed_rels
            .entry(source.to_owned())
            .or_default()
            .insert(id.to_owned());
    }

    /// Parses an XML part; `None` (with a diagnostic) when it is missing or malformed.
    pub fn load_part(&mut self, name: &str) -> Option<XmlDocument> {
        match self.pkg.part_xml_with(name, part_parse_options()) {
            Ok(Some(doc)) => Some(doc),
            Ok(None) => {
                self.warn(
                    WarningCode::PartMissing,
                    Severity::Warning,
                    format!("The part {name} is referenced but missing from the package."),
                    name,
                    "/",
                );
                None
            }
            Err(e) => {
                self.warn(
                    WarningCode::PartInvalidXml,
                    Severity::Warning,
                    format!("The part {name} could not be read and was skipped ({e})."),
                    name,
                    "/",
                );
                None
            }
        }
    }

    /// The first part `source` relates to with `rel_type` (unreadable relationships count as
    /// none, with a diagnostic).
    pub fn related_part(&mut self, source: &str, rel_type: &str) -> Option<String> {
        match self.pkg.related_part(source, rel_type) {
            Ok(p) => p.filter(|p| self.pkg.has(p)),
            Err(e) => {
                self.warn(
                    WarningCode::PartInvalidXml,
                    Severity::Warning,
                    format!("The relationships of {source} could not be read ({e})."),
                    source,
                    "/",
                );
                None
            }
        }
    }

    /// Looks up a relationship of `source` and records it in the id → target table. Returns the
    /// target (a part name for internal targets, the URL for external ones).
    pub fn relationship(&mut self, source: &str, id: &str) -> Option<RelTarget> {
        let rels = self.pkg.relationships(source).ok()?;
        let r = rels.get(id)?;
        let external = r.is_external();
        let target = if external {
            r.target.clone()
        } else {
            rels.resolve(r).unwrap_or_else(|| r.target.clone())
        };
        let t = RelTarget {
            rel_type: r.rel_type.clone(),
            target,
            external,
        };
        self.relationships
            .entry(source.to_owned())
            .or_default()
            .insert(id.to_owned(), t.clone());
        Some(t)
    }

    /// Records every relationship of `source` in the id → target table (for ids referenced from
    /// preserved raw XML).
    pub fn record_all_relationships(&mut self, source: &str) {
        let ids: Vec<String> = match self.pkg.relationships(source) {
            Ok(rels) => rels.all().iter().map(|r| r.id.clone()).collect(),
            Err(_) => return,
        };
        for id in ids {
            let _ = self.relationship(source, &id);
        }
    }
}
