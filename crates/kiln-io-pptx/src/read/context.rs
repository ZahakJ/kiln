// SPDX-License-Identifier: AGPL-3.0-or-later
//! Per-part reading context: the part's relationships, its colour context, the file's shape ids
//! and the helpers that turn relationship ids into part keys and links.

use std::collections::{BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::Part;
use kiln_ooxml::drawingml::color::ColorContext;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::opc::{Relationships, rel_type_matches, rels_part_name};
use kiln_ooxml::xml::{Attrs, XmlElement};

use super::Reader;
use crate::common::kind::rel;
use crate::common::warnings::{Severity, WarningCode};

/// A connector end as written: (file shape id, connection site).
pub(crate) type FileConnection = Option<(u32, u32)>;

/// A connector whose ends are resolved once the part is read: (connector, start, end).
pub(crate) type PendingConnection = (NodeId, FileConnection, FileConnection);

/// Everything shape reading needs about the part the shapes are in.
pub(crate) struct PartCx {
    /// The part name.
    pub part: String,
    /// Its relationships.
    pub rels: Relationships,
    /// The root element's attributes (namespace declarations for fragments).
    pub scope: Attrs,
    /// Theme colours and the colour map in force, so theme colours in text cache their RGB.
    pub color: Option<ColorContext>,
    /// `p:cNvPr/@id` → model id, for connectors and animation targets.
    pub ids: HashMap<u32, NodeId>,
    /// Connector ends to resolve once every shape of the part is read: (connector, start, end)
    /// as (file shape id, site).
    pub connections: Vec<PendingConnection>,
    /// Whether media found through this part is modelled (false inside SmartArt drawings, whose
    /// parts are kept as they are).
    pub consume_media: bool,
}

impl PartCx {
    /// A context for a part.
    pub fn new(r: &mut Reader<'_>, part: &str, scope: &Attrs, color: Option<ColorContext>) -> Self {
        let rels = match r.pkg.relationships(part) {
            Ok(rels) => rels.into_owned(),
            Err(e) => {
                r.warn(
                    WarningCode::PartInvalidXml,
                    Severity::Warning,
                    format!("the relationships are not readable ({e})"),
                    &rels_part_name(part),
                    "",
                );
                Relationships::new(part)
            }
        };
        Self {
            part: part.to_owned(),
            rels,
            scope: scope.clone(),
            color,
            ids: HashMap::new(),
            connections: Vec::new(),
            consume_media: true,
        }
    }

    /// The internal part a relationship id points at.
    pub fn target(&self, id: &str) -> Option<String> {
        let rel = self.rels.get(id)?;
        if rel.is_external() {
            return None;
        }
        self.rels.resolve(rel)
    }
}

/// What a relationship id gave.
pub(crate) enum MediaRef {
    /// A part now in [`kiln_deck::Presentation::parts`] under this key.
    Part(String),
    /// An external file or URL.
    External(String),
    /// Nothing usable (unknown id, missing part).
    Missing,
}

impl Reader<'_> {
    /// Loads the part a relationship points at into the presentation's parts store (once) and
    /// returns its key.
    pub fn media(&mut self, cx: &PartCx, id: &str) -> MediaRef {
        let Some(rel) = cx.rels.get(id) else {
            return MediaRef::Missing;
        };
        if rel.is_external() {
            return MediaRef::External(rel.target.clone());
        }
        let Some(target) = cx.rels.resolve(rel) else {
            return MediaRef::Missing;
        };
        if self.store_part(&target, cx.consume_media) {
            MediaRef::Part(target)
        } else {
            MediaRef::Missing
        }
    }

    /// Copies a package part into the parts store; false when it does not exist.
    pub fn store_part(&mut self, name: &str, consume: bool) -> bool {
        if self.pres.parts.contains_key(name) {
            return true;
        }
        let Ok(Some(bytes)) = self.pkg.part(name) else {
            return false;
        };
        let content_type = self
            .pkg
            .content_type(name)
            .unwrap_or(kiln_ooxml::opc::content_type::OCTET_STREAM)
            .to_owned();
        self.pres.parts.insert(
            name.to_owned(),
            Part {
                content_type,
                bytes: bytes.into_owned(),
            },
        );
        if consume {
            self.consume(name);
        }
        true
    }

    /// Stores a chart part with its relationships part and what those point at (embedded
    /// workbook, chart style and colours, user shapes and their images), all consumed.
    pub fn store_chart(&mut self, chart: &str) -> bool {
        if !self.store_part(chart, true) {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut queue = vec![(chart.to_owned(), 0usize)];
        while let Some((source, depth)) = queue.pop() {
            if !seen.insert(source.clone()) || depth > 2 {
                continue;
            }
            let Ok(rels) = self.pkg.relationships(&source) else {
                continue;
            };
            let rels = rels.into_owned();
            if rels.is_empty() {
                continue;
            }
            let rels_name = rels_part_name(&source);
            self.pres.parts.entry(rels_name).or_insert_with(|| Part {
                content_type: kiln_ooxml::opc::content_type::RELATIONSHIPS.to_owned(),
                bytes: rels.to_bytes(),
            });
            for r in rels.all() {
                if r.is_external() {
                    continue;
                }
                if let Some(t) = rels.resolve(r)
                    && self.store_part(&t, true)
                {
                    queue.push((t, depth + 1));
                }
            }
        }
        true
    }

    /// Rewrites the relationship id of a picture fill to the part key, loading the image.
    pub fn fix_fill(&mut self, cx: &PartCx, fill: &mut Fill, location: &str) {
        let Fill::Blip(b) = fill else { return };
        let Some(id) = b.embed.clone() else { return };
        match self.media(cx, &id) {
            MediaRef::Part(key) => b.embed = Some(key),
            MediaRef::External(url) => {
                b.embed = None;
                b.link = Some(url);
            }
            MediaRef::Missing => {
                b.embed = None;
                self.warn(
                    WarningCode::ImageMissing,
                    Severity::Warning,
                    "a picture fill's image is missing",
                    &cx.part,
                    location.to_owned(),
                );
            }
        }
    }

    /// The link of an `a:hlinkClick`/`a:hlinkHover`: a URL, `#slide:<id>`, or the action verb.
    pub fn link(&mut self, cx: &PartCx, h: &XmlElement) -> Option<String> {
        let id = h.attr("r:id").filter(|s| !s.is_empty());
        let action = h.attr("action").filter(|s| !s.is_empty());
        if let Some(id) = id {
            let Some(rel) = cx.rels.get(id) else {
                self.warn(
                    WarningCode::LinkBroken,
                    Severity::Warning,
                    format!("a hyperlink refers to the unknown relationship {id}"),
                    &cx.part,
                    "",
                );
                return action.map(str::to_owned);
            };
            if rel.is_external() {
                self.consume_rel(&cx.part, id);
                return Some(rel.target.clone());
            }
            let target = cx.rels.resolve(rel)?;
            if rel_type_matches(&rel.rel_type, rel::SLIDE) {
                if let Some(slide) = self.slide_ids.get(&target) {
                    return Some(format!("#slide:{}", slide.0));
                }
                self.warn(
                    WarningCode::LinkBroken,
                    Severity::Warning,
                    "a hyperlink jumps to a slide that is not in the presentation",
                    &cx.part,
                    "",
                );
                return action.map(str::to_owned);
            }
            // A link to another part of the package: the part name (the relationship stays in
            // the passthrough with the part).
            return Some(target);
        }
        action.map(str::to_owned)
    }
}
