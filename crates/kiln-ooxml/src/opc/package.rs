// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`OpcPackage`]: an Open Packaging Conventions container held as parts.
//!
//! Parts are inflated lazily, on first access, and cached; the archive is validated against
//! [`OpcLimits`] before anything is inflated. A part nobody changes is written back by copying its
//! compressed zip entry verbatim, so untouched parts are byte-identical (even compressed) after a
//! save, and a save that changes nothing costs little more than a memory copy. Relationships are
//! parsed on demand and re-serialised only when modified.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::{Arc, OnceLock};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter};

use super::OpcError;
use super::content_types::{ContentTypes, content_type};
use super::part_name::{
    extension_of, part_key, relative_target, rels_part_name, source_part_of_rels,
    validate_part_name, zip_name_to_part_name,
};
use super::rel_types::rel_type;
use super::relationships::{Relationship, Relationships};
use crate::st::TargetMode;
use crate::xml::{
    ParseOptions, WriteOptions, XmlDocument, XmlElement, parse_xml_with, write_document,
    write_element,
};

/// The name of the content-types stream (not a part; managed by the package).
pub const CONTENT_TYPES_PART: &str = "/[Content_Types].xml";
const CONTENT_TYPES_KEY: &str = "/[content_types].xml";
const PACKAGE_RELS: &str = "/_rels/.rels";

/// Limits applied before anything is inflated, so a hostile archive cannot exhaust memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpcLimits {
    /// Maximum number of parts (zip entries other than directories). Default 65 536.
    pub max_entries: usize,
    /// Maximum declared uncompressed size of one part, in bytes. Default 2 GiB.
    pub max_entry_size: u64,
    /// Maximum declared uncompressed size of all parts together, in bytes. Default 2 GiB.
    pub max_total_size: u64,
    /// Maximum compression ratio of a part larger than `ratio_threshold`. Real Office XML stays
    /// far below 1000:1; zip bombs exceed it by orders of magnitude. Default 1000.
    pub max_compression_ratio: u64,
    /// Parts smaller than this are exempt from the ratio check. Default 1 MiB.
    pub ratio_threshold: u64,
}

impl Default for OpcLimits {
    fn default() -> Self {
        Self {
            max_entries: 65_536,
            max_entry_size: 2 << 30,
            max_total_size: 2 << 30,
            max_compression_ratio: 1000,
            ratio_threshold: 1 << 20,
        }
    }
}

/// Options for [`OpcPackage::save_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOptions {
    /// Deflate level (0–9) for new and changed parts. Default 6.
    pub level: u8,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { level: 6 }
    }
}

/// Options for [`OpcPackage::add_relationship`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationshipOptions<'a> {
    /// The id to use; defaults to the next free `rIdN`.
    pub id: Option<&'a str>,
    /// An external target (a URL), stored verbatim.
    pub external: bool,
}

// Already-compressed media gain nothing from deflate.
const STORED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "mp4", "mp3", "m4a", "zip", "docx", "xlsx", "pptx", "wdp",
];

type Archive = ZipArchive<Cursor<Arc<[u8]>>>;

#[derive(Debug, Clone)]
struct PartEntry {
    /// The part name (`/word/document.xml`).
    name: String,
    /// The zip entry name, kept as found so untouched entries are copied exactly.
    zip_name: String,
    /// The entry in the source archive while the part is unchanged.
    source_index: Option<usize>,
    declared_size: u64,
    /// The inflated bytes (or why inflating failed), filled on first access.
    data: OnceLock<Result<Vec<u8>, String>>,
}

#[derive(Debug, Clone)]
struct RelsSlot {
    source: String,
    parsed: OnceLock<Result<Relationships, String>>,
    /// Modified since parsing: the `.rels` part is re-serialised on access and on save.
    dirty: bool,
}

impl RelsSlot {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            parsed: OnceLock::new(),
            dirty: false,
        }
    }
}

/// An Open Packaging Conventions container (`.docx`, `.xlsx`, `.pptx`).
#[derive(Debug, Clone)]
pub struct OpcPackage {
    parts: Vec<PartEntry>,
    index: HashMap<String, usize>,
    content_types: ContentTypes,
    /// `[Content_Types].xml` in the source archive: (entry index, zip name).
    content_types_source: Option<(usize, String)>,
    /// Relationships by case-folded source part name.
    rels: HashMap<String, RelsSlot>,
    archive: Option<Archive>,
}

impl Default for OpcPackage {
    fn default() -> Self {
        Self::new()
    }
}

fn decode_entry(archive: &Archive, index: usize, declared: u64) -> Result<Vec<u8>, String> {
    let mut archive = archive.clone();
    let file = archive.by_index(index).map_err(|e| e.to_string())?;
    let capacity = usize::try_from(declared.min(64 << 20)).unwrap_or(0);
    let mut buf = Vec::with_capacity(capacity);
    file.take(declared.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > declared {
        return Err("the entry inflates beyond its declared size".to_owned());
    }
    Ok(buf)
}

impl OpcPackage {
    /// An empty package with Office's `rels` and `xml` content-type defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parts: Vec::new(),
            index: HashMap::new(),
            content_types: ContentTypes::office_defaults(),
            content_types_source: None,
            rels: HashMap::new(),
            archive: None,
        }
    }

    /// Opens a package from zip bytes with the default [`OpcLimits`].
    ///
    /// # Errors
    /// See [`OpcPackage::open_with_limits`].
    pub fn open(bytes: &[u8]) -> Result<Self, OpcError> {
        Self::open_with_limits(bytes, OpcLimits::default())
    }

    /// Opens a package from zip bytes, enforcing `limits` before inflating anything. Only
    /// `[Content_Types].xml` is inflated here; parts are inflated on first access.
    ///
    /// # Errors
    /// [`OpcError::EncryptedOrLegacy`] for an OLE compound file (a password-protected or legacy
    /// binary Office file), [`OpcError::NotZip`], [`OpcError::CorruptZip`],
    /// [`OpcError::ZipBomb`] when a limit is exceeded, [`OpcError::DuplicatePart`],
    /// [`OpcError::MissingContentTypes`] and [`OpcError::ContentTypes`].
    pub fn open_with_limits(bytes: &[u8], limits: OpcLimits) -> Result<Self, OpcError> {
        if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
            return Err(OpcError::EncryptedOrLegacy);
        }
        if bytes.len() < 4 || !bytes.starts_with(b"PK") {
            return Err(OpcError::NotZip);
        }
        let data: Arc<[u8]> = Arc::from(bytes);
        let archive =
            ZipArchive::new(Cursor::new(data)).map_err(|e| OpcError::CorruptZip(e.to_string()))?;

        let mut pkg = Self {
            content_types: ContentTypes::new(),
            ..Self::new()
        };
        let mut count = 0usize;
        let mut total = 0u64;
        for i in 0..archive.len() {
            let entry = archive
                .by_index_data(i)
                .map_err(|e| OpcError::CorruptZip(e.to_string()))?;
            let zip_name = entry
                .name()
                .map_err(|e| OpcError::CorruptZip(e.to_string()))?
                .into_owned();
            if entry.is_dir() || zip_name.ends_with('/') {
                continue;
            }
            count += 1;
            if count > limits.max_entries {
                return Err(OpcError::ZipBomb(format!(
                    "more than {} zip entries",
                    limits.max_entries
                )));
            }
            let size = entry.size();
            if size > limits.max_entry_size {
                return Err(OpcError::ZipBomb(format!(
                    "entry {zip_name} declares {size} bytes (limit {})",
                    limits.max_entry_size
                )));
            }
            total = total.saturating_add(size);
            if total > limits.max_total_size {
                return Err(OpcError::ZipBomb(format!(
                    "the package expands beyond {} bytes",
                    limits.max_total_size
                )));
            }
            let compressed = entry.compressed_size().max(1);
            if size > limits.ratio_threshold
                && size > limits.max_compression_ratio.saturating_mul(compressed)
            {
                return Err(OpcError::ZipBomb(format!(
                    "entry {zip_name} has a suspicious compression ratio"
                )));
            }
            if entry.encrypted() {
                return Err(OpcError::CorruptZip(format!(
                    "entry {zip_name} uses zip encryption, which Office packages never do"
                )));
            }
            let name = zip_name_to_part_name(&zip_name);
            let key = part_key(&name);
            if key == CONTENT_TYPES_KEY {
                if pkg.content_types_source.is_some() {
                    return Err(OpcError::DuplicatePart(name));
                }
                pkg.content_types_source = Some((i, zip_name));
                continue;
            }
            if pkg.index.contains_key(&key) {
                return Err(OpcError::DuplicatePart(name));
            }
            pkg.index.insert(key, pkg.parts.len());
            pkg.parts.push(PartEntry {
                name,
                zip_name,
                source_index: Some(i),
                declared_size: size,
                data: OnceLock::new(),
            });
        }
        let Some((ct_index, _)) = pkg.content_types_source else {
            return Err(OpcError::MissingContentTypes);
        };
        let ct_size = archive
            .by_index_data(ct_index)
            .map_err(|e| OpcError::CorruptZip(e.to_string()))?
            .size();
        let ct_bytes = decode_entry(&archive, ct_index, ct_size).map_err(OpcError::CorruptZip)?;
        pkg.content_types =
            ContentTypes::parse(&ct_bytes).map_err(|e| OpcError::ContentTypes(e.to_string()))?;
        let sources: Vec<String> = pkg
            .parts
            .iter()
            .filter_map(|p| source_part_of_rels(&p.name))
            .collect();
        for source in sources {
            let canonical = pkg.canonical_source(&source);
            pkg.rels
                .insert(part_key(&source), RelsSlot::new(&canonical));
        }
        pkg.archive = Some(archive);
        Ok(pkg)
    }

    // ------------------------------------------------------------------ parts

    fn entry(&self, name: &str) -> Option<&PartEntry> {
        self.index.get(&part_key(name)).map(|&i| &self.parts[i])
    }

    fn entry_bytes<'a>(&'a self, entry: &'a PartEntry) -> Result<&'a [u8], OpcError> {
        let data = entry
            .data
            .get_or_init(|| match (&self.archive, entry.source_index) {
                (Some(a), Some(i)) => decode_entry(a, i, entry.declared_size),
                _ => Ok(Vec::new()),
            });
        data.as_deref()
            .map_err(|e| OpcError::CorruptZip(format!("{}: {e}", entry.name)))
    }

    fn rebuild_index(&mut self) {
        self.index = self
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| (part_key(&p.name), i))
            .collect();
    }

    /// Every part name except `[Content_Types].xml`, in package order, including `.rels` parts.
    pub fn all_part_names(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().map(|p| p.name.as_str())
    }

    /// Part names excluding relationship parts, in package order.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.all_part_names()
            .filter(|n| source_part_of_rels(n).is_none())
    }

    /// Number of parts (including `.rels` parts, excluding `[Content_Types].xml`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// True when the package holds no parts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// True when a part with this name exists (case-insensitively).
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.index.contains_key(&part_key(name))
    }

    /// True when the part's bytes are still those of the opened archive.
    #[must_use]
    pub fn is_untouched(&self, name: &str) -> bool {
        self.entry(name).is_some_and(|e| {
            e.source_index.is_some()
                && !source_part_of_rels(&e.name)
                    .and_then(|s| self.rels.get(&part_key(&s)))
                    .is_some_and(|slot| slot.dirty)
        })
    }

    /// The bytes of a part. Relationship parts reflect pending relationship edits.
    ///
    /// # Errors
    /// [`OpcError::CorruptZip`] when the part's zip entry cannot be inflated.
    pub fn part(&self, name: &str) -> Result<Option<Cow<'_, [u8]>>, OpcError> {
        let Some(entry) = self.entry(name) else {
            return Ok(None);
        };
        if let Some(source) = source_part_of_rels(&entry.name)
            && let Some(slot) = self.rels.get(&part_key(&source))
            && slot.dirty
            && let Some(Ok(rels)) = slot.parsed.get()
        {
            return Ok(Some(Cow::Owned(rels.to_bytes())));
        }
        self.entry_bytes(entry).map(|b| Some(Cow::Borrowed(b)))
    }

    /// The bytes of a part that must exist.
    ///
    /// # Errors
    /// [`OpcError::NoSuchPart`], or as [`OpcPackage::part`].
    pub fn part_bytes(&self, name: &str) -> Result<Cow<'_, [u8]>, OpcError> {
        self.part(name)?
            .ok_or_else(|| OpcError::NoSuchPart(name.to_owned()))
    }

    /// Parses an XML part; `None` if the part does not exist.
    ///
    /// # Errors
    /// [`OpcError::Xml`] when the part is not well-formed, or as [`OpcPackage::part`].
    pub fn part_xml(&self, name: &str) -> Result<Option<XmlDocument>, OpcError> {
        self.part_xml_with(name, ParseOptions::default())
    }

    /// [`OpcPackage::part_xml`] with parser options.
    ///
    /// # Errors
    /// As [`OpcPackage::part_xml`].
    pub fn part_xml_with(
        &self,
        name: &str,
        options: ParseOptions,
    ) -> Result<Option<XmlDocument>, OpcError> {
        let Some(bytes) = self.part(name)? else {
            return Ok(None);
        };
        parse_xml_with(&bytes, options)
            .map(Some)
            .map_err(|error| OpcError::Xml {
                part: name.to_owned(),
                error,
            })
    }

    fn reset_rels_slot_for(&mut self, part_name: &str) {
        if let Some(source) = source_part_of_rels(part_name) {
            let canonical = self.canonical_source(&source);
            self.rels
                .insert(part_key(&source), RelsSlot::new(&canonical));
        }
    }

    /// The source part's name as stored in the package (relationship targets resolve against
    /// it), or `source` itself for the package and for parts that do not exist.
    fn canonical_source(&self, source: &str) -> String {
        if source == "/" {
            return source.to_owned();
        }
        self.entry(source)
            .map_or_else(|| source.to_owned(), |e| e.name.clone())
    }

    /// Replaces the content of an existing part.
    ///
    /// # Errors
    /// [`OpcError::NoSuchPart`].
    pub fn set_part(&mut self, name: &str, bytes: impl Into<Vec<u8>>) -> Result<(), OpcError> {
        let i = *self
            .index
            .get(&part_key(name))
            .ok_or_else(|| OpcError::NoSuchPart(name.to_owned()))?;
        let entry = &mut self.parts[i];
        entry.data = OnceLock::from(Ok(bytes.into()));
        entry.source_index = None;
        let stored = entry.name.clone();
        self.reset_rels_slot_for(&stored);
        Ok(())
    }

    /// Replaces an existing part with a serialised XML document.
    ///
    /// # Errors
    /// [`OpcError::NoSuchPart`] or [`OpcError::XmlWrite`].
    pub fn set_part_xml(&mut self, name: &str, doc: &XmlDocument) -> Result<(), OpcError> {
        let bytes = write_document(doc, &WriteOptions::default())?.into_bytes();
        self.set_part(name, bytes)
    }

    /// Adds a new part and registers its content type.
    ///
    /// # Errors
    /// [`OpcError::PartName`] for an invalid name, [`OpcError::ReservedPartName`] for
    /// `[Content_Types].xml`, [`OpcError::PartExists`].
    pub fn add_part(
        &mut self,
        name: &str,
        content_type: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), OpcError> {
        validate_part_name(name)?;
        let key = part_key(name);
        if key == CONTENT_TYPES_KEY {
            return Err(OpcError::ReservedPartName);
        }
        if self.index.contains_key(&key) {
            return Err(OpcError::PartExists(name.to_owned()));
        }
        self.index.insert(key, self.parts.len());
        self.parts.push(PartEntry {
            name: name.to_owned(),
            zip_name: name.trim_start_matches('/').to_owned(),
            source_index: None,
            declared_size: 0,
            data: OnceLock::from(Ok(bytes.into())),
        });
        self.content_types.set(name, content_type);
        self.reset_rels_slot_for(name);
        Ok(())
    }

    /// Adds a new part holding an XML element (with Office's declaration).
    ///
    /// # Errors
    /// As [`OpcPackage::add_part`], or [`OpcError::XmlWrite`].
    pub fn add_xml_part(
        &mut self,
        name: &str,
        content_type: &str,
        root: &XmlElement,
    ) -> Result<(), OpcError> {
        let bytes = write_element(root, &WriteOptions::default())?.into_bytes();
        self.add_part(name, content_type, bytes)
    }

    /// Replaces a part (setting its content type) or adds it.
    ///
    /// # Errors
    /// As [`OpcPackage::add_part`].
    pub fn put_part(
        &mut self,
        name: &str,
        content_type: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), OpcError> {
        if self.has(name) {
            self.set_part(name, bytes)?;
            self.set_content_type(name, content_type)
        } else {
            self.add_part(name, content_type, bytes)
        }
    }

    /// Removes a part, its relationships part and its content-type override. With
    /// `remove_relationships`, every relationship in the package that targets it goes too.
    /// Returns whether the part existed.
    ///
    /// # Errors
    /// [`OpcError::Relationships`] when a `.rels` part that must be edited is unreadable.
    pub fn remove_part(
        &mut self,
        name: &str,
        remove_relationships: bool,
    ) -> Result<bool, OpcError> {
        let key = part_key(name);
        let Some(&i) = self.index.get(&key) else {
            return Ok(false);
        };
        let removed = self.parts.remove(i);
        self.content_types.remove(&removed.name);
        let rels_key = part_key(&rels_part_name(&removed.name));
        self.parts.retain(|p| part_key(&p.name) != rels_key);
        self.rels.remove(&key);
        self.reset_rels_slot_for_removed(&removed.name);
        self.rebuild_index();
        if remove_relationships {
            self.remove_relationships_targeting(&removed.name)?;
        }
        Ok(true)
    }

    fn reset_rels_slot_for_removed(&mut self, removed: &str) {
        // A removed `.rels` part leaves its source without relationships.
        if let Some(source) = source_part_of_rels(removed) {
            self.rels.remove(&part_key(&source));
        }
    }

    /// The first free part name for a pattern containing `{n}`, numbered from 1:
    /// `next_part_name("/word/media/image{n}.png")` → `/word/media/image3.png`.
    ///
    /// # Errors
    /// [`OpcError::BadPattern`] without `{n}`.
    pub fn next_part_name(&self, pattern: &str) -> Result<String, OpcError> {
        if !pattern.contains("{n}") {
            return Err(OpcError::BadPattern(pattern.to_owned()));
        }
        Ok((1u64..)
            .map(|n| pattern.replacen("{n}", &n.to_string(), 1))
            .find(|c| !self.has(c))
            .unwrap_or_else(|| pattern.replacen("{n}", "0", 1)))
    }

    /// The content type of a part (its override, else its extension's default).
    #[must_use]
    pub fn content_type(&self, name: &str) -> Option<&str> {
        if part_key(name) == CONTENT_TYPES_KEY {
            return None;
        }
        self.content_types.get(name)
    }

    /// Sets the content type of an existing part.
    ///
    /// # Errors
    /// [`OpcError::NoSuchPart`].
    pub fn set_content_type(&mut self, name: &str, content_type: &str) -> Result<(), OpcError> {
        let stored = self
            .entry(name)
            .map(|e| e.name.clone())
            .ok_or_else(|| OpcError::NoSuchPart(name.to_owned()))?;
        self.content_types.set(&stored, content_type);
        Ok(())
    }

    /// The content-types table.
    #[must_use]
    pub fn content_types(&self) -> &ContentTypes {
        &self.content_types
    }

    /// Parts whose content type equals `content_type`.
    #[must_use]
    pub fn parts_by_content_type(&self, content_type: &str) -> Vec<&str> {
        self.part_names()
            .filter(|n| self.content_type(n) == Some(content_type))
            .collect()
    }

    // ---------------------------------------------------------- relationships

    fn parse_rels(&self, source: &str) -> Result<Relationships, String> {
        match self.entry(&rels_part_name(source)) {
            None => Ok(Relationships::new(source)),
            Some(e) => {
                let bytes = self.entry_bytes(e).map_err(|e| e.to_string())?;
                Relationships::parse(source, bytes).map_err(|e| e.to_string())
            }
        }
    }

    /// The relationships of a source part (`/` for the package's own). Empty when the part has
    /// no `.rels` part.
    ///
    /// # Errors
    /// [`OpcError::Relationships`] when the `.rels` part is unreadable.
    pub fn relationships(&self, source: &str) -> Result<Cow<'_, Relationships>, OpcError> {
        let Some(slot) = self.rels.get(&part_key(source)) else {
            return Ok(Cow::Owned(Relationships::new(source)));
        };
        slot.parsed
            .get_or_init(|| self.parse_rels(&slot.source))
            .as_ref()
            .map(Cow::Borrowed)
            .map_err(|m| OpcError::Relationships {
                source_part: source.to_owned(),
                message: m.clone(),
            })
    }

    /// Source parts that have relationships (including `/`).
    #[must_use]
    pub fn relationship_sources(&self) -> Vec<String> {
        let mut out: Vec<String> = self.rels.values().map(|s| s.source.clone()).collect();
        out.sort_by_key(|s| {
            self.index
                .get(&part_key(&rels_part_name(s)))
                .copied()
                .unwrap_or(usize::MAX)
        });
        out
    }

    /// Edits the relationships of `source`, then keeps the `.rels` part in step.
    fn edit_rels<T>(
        &mut self,
        source: &str,
        edit: impl FnOnce(&mut Relationships) -> Result<T, OpcError>,
    ) -> Result<T, OpcError> {
        let key = part_key(source);
        let parsed_already = self
            .rels
            .get(&key)
            .is_some_and(|s| s.parsed.get().is_some());
        let fresh = if parsed_already {
            None
        } else {
            Some(self.parse_rels(source))
        };
        let canonical = self.canonical_source(source);
        let slot = self
            .rels
            .entry(key.clone())
            .or_insert_with(|| RelsSlot::new(&canonical));
        if let Some(parsed) = fresh {
            slot.parsed = OnceLock::from(parsed);
        }
        let rels = match slot.parsed.get_mut() {
            Some(Ok(r)) => r,
            Some(Err(m)) => {
                return Err(OpcError::Relationships {
                    source_part: source.to_owned(),
                    message: m.clone(),
                });
            }
            None => {
                return Err(OpcError::Relationships {
                    source_part: source.to_owned(),
                    message: "relationships were not loaded".to_owned(),
                });
            }
        };
        let out = edit(rels)?;
        let empty = rels.is_empty();
        slot.dirty = true;
        let rels_name = rels_part_name(&slot.source);
        let rels_key = part_key(&rels_name);
        if empty {
            if self.index.contains_key(&rels_key) {
                self.parts.retain(|p| part_key(&p.name) != rels_key);
                self.rebuild_index();
            }
        } else if !self.index.contains_key(&rels_key) {
            self.index.insert(rels_key, self.parts.len());
            self.parts.push(PartEntry {
                zip_name: rels_name.trim_start_matches('/').to_owned(),
                name: rels_name,
                source_index: None,
                declared_size: 0,
                data: OnceLock::new(),
            });
        }
        Ok(out)
    }

    /// Adds a relationship from `source`. For an internal target pass the absolute part name;
    /// it is stored relative, as Office writes it. External targets (URLs) are stored verbatim.
    ///
    /// # Errors
    /// [`OpcError::DuplicateRelationshipId`] or [`OpcError::Relationships`].
    pub fn add_relationship(
        &mut self,
        source: &str,
        rel_type: &str,
        target: &str,
        options: RelationshipOptions<'_>,
    ) -> Result<Relationship, OpcError> {
        let (stored, mode) = if options.external {
            (target.to_owned(), TargetMode::External)
        } else {
            (relative_target(source, target), TargetMode::Internal)
        };
        self.add_relationship_raw(source, rel_type, &stored, options.id, mode)
    }

    /// Adds a relationship with the target stored exactly as given.
    ///
    /// # Errors
    /// As [`OpcPackage::add_relationship`].
    pub fn add_relationship_raw(
        &mut self,
        source: &str,
        rel_type: &str,
        stored_target: &str,
        id: Option<&str>,
        mode: TargetMode,
    ) -> Result<Relationship, OpcError> {
        self.edit_rels(source, |r| r.add(rel_type, stored_target, id, mode))
    }

    /// Removes a relationship; returns whether it existed.
    ///
    /// # Errors
    /// [`OpcError::Relationships`].
    pub fn remove_relationship(&mut self, source: &str, id: &str) -> Result<bool, OpcError> {
        if self.relationships(source)?.get(id).is_none() {
            return Ok(false);
        }
        self.edit_rels(source, |r| Ok(r.remove(id).is_some()))
    }

    /// Removes every internal relationship in the package whose target is `part_name`; returns
    /// how many were removed. `.rels` parts with no such relationship are left untouched.
    ///
    /// # Errors
    /// [`OpcError::Relationships`].
    pub fn remove_relationships_targeting(&mut self, part_name: &str) -> Result<usize, OpcError> {
        let key = part_key(part_name);
        let mut removed = 0;
        for source in self.relationship_sources() {
            let hit = {
                let rels = self.relationships(&source)?;
                rels.all()
                    .iter()
                    .any(|r| rels.resolve(r).is_some_and(|t| part_key(&t) == key))
            };
            if hit {
                removed += self.edit_rels(&source, |r| Ok(r.remove_targeting(part_name).len()))?;
            }
        }
        Ok(removed)
    }

    /// Absolute names of the internal parts `source` relates to with `rel_type`, in
    /// relationship order.
    ///
    /// # Errors
    /// [`OpcError::Relationships`].
    pub fn related_parts(&self, source: &str, rel_type: &str) -> Result<Vec<String>, OpcError> {
        let rels = self.relationships(source)?;
        Ok(rels
            .by_type(rel_type)
            .filter_map(|r| rels.resolve(r))
            .collect())
    }

    /// The first internal part `source` relates to with `rel_type`.
    ///
    /// # Errors
    /// [`OpcError::Relationships`].
    pub fn related_part(&self, source: &str, rel_type: &str) -> Result<Option<String>, OpcError> {
        let rels = self.relationships(source)?;
        Ok(rels.first_by_type(rel_type).and_then(|r| rels.resolve(r)))
    }

    /// Resolves a relationship id on `source` to an absolute part name (internal targets only).
    ///
    /// # Errors
    /// [`OpcError::Relationships`].
    pub fn resolve_relationship(&self, source: &str, id: &str) -> Result<Option<String>, OpcError> {
        let rels = self.relationships(source)?;
        Ok(rels.get(id).and_then(|r| rels.resolve(r)))
    }

    /// The main document part (`/word/document.xml`, `/xl/workbook.xml`,
    /// `/ppt/presentation.xml`).
    ///
    /// # Errors
    /// [`OpcError::Relationships`] when the package relationships are unreadable.
    pub fn main_document_part(&self) -> Result<Option<String>, OpcError> {
        self.related_part("/", rel_type::OFFICE_DOCUMENT)
    }

    // ------------------------------------------------------------------ save

    /// Serialises the package with default [`SaveOptions`].
    ///
    /// # Errors
    /// See [`OpcPackage::save_with`].
    pub fn save(&self) -> Result<Vec<u8>, OpcError> {
        self.save_with(SaveOptions::default())
    }

    /// Serialises the package. `[Content_Types].xml` comes first and `/_rels/.rels` second, as
    /// Office writes them; other parts keep their order with new parts at the end. Untouched parts
    /// are copied as compressed; new and changed parts are deflated (media stored) with the
    /// 1980-01-01 timestamp Office uses, so output is reproducible.
    ///
    /// # Errors
    /// [`OpcError::CorruptZip`] when a changed part cannot be read, [`OpcError::Write`] when the
    /// archive cannot be written.
    pub fn save_with(&self, options: SaveOptions) -> Result<Vec<u8>, OpcError> {
        let werr = |e: zip::result::ZipError| OpcError::Write(e.to_string());
        let mut content_types = Cow::Borrowed(&self.content_types);
        if self
            .parts
            .iter()
            .any(|p| source_part_of_rels(&p.name).is_some())
            && content_types.default_for("rels").is_none()
        {
            content_types
                .to_mut()
                .set_default("rels", content_type::RELATIONSHIPS);
        }
        let estimate = self.archive.as_ref().map_or(64 << 10, |a| {
            usize::try_from(a.central_directory_start()).unwrap_or(0) + (16 << 10)
        });
        let mut writer = ZipWriter::new(Cursor::new(Vec::with_capacity(estimate)));
        let mut source = self.archive.clone();

        match (&mut source, &self.content_types_source) {
            (Some(archive), Some((i, zip_name))) if !content_types.is_dirty() => {
                let file = archive
                    .by_index_raw(*i)
                    .map_err(|e| OpcError::CorruptZip(e.to_string()))?;
                writer.raw_copy_file_rename(file, zip_name).map_err(werr)?;
            }
            _ => {
                write_new(
                    &mut writer,
                    "[Content_Types].xml",
                    &content_types.to_bytes(),
                    options.level,
                )?;
            }
        }

        let package_rels = self.index.get(PACKAGE_RELS).copied();
        let order = package_rels
            .into_iter()
            .chain((0..self.parts.len()).filter(|i| Some(*i) != package_rels));
        for i in order {
            let entry = &self.parts[i];
            let dirty_rels = source_part_of_rels(&entry.name)
                .and_then(|s| self.rels.get(&part_key(&s)))
                .filter(|slot| slot.dirty)
                .and_then(|slot| slot.parsed.get())
                .and_then(|r| r.as_ref().ok());
            if let Some(rels) = dirty_rels {
                write_new(
                    &mut writer,
                    &entry.zip_name,
                    &rels.to_bytes(),
                    options.level,
                )?;
                continue;
            }
            if let (Some(archive), Some(index)) = (&mut source, entry.source_index)
                && let Ok(file) = archive.by_index_raw(index)
            {
                writer
                    .raw_copy_file_rename(file, &entry.zip_name)
                    .map_err(werr)?;
                continue;
            }
            let bytes = self.entry_bytes(entry)?;
            let level = if STORED_EXTENSIONS.contains(&extension_of(&entry.name).as_str()) {
                0
            } else {
                options.level
            };
            write_new(&mut writer, &entry.zip_name, bytes, level)?;
        }
        Ok(writer.finish().map_err(werr)?.into_inner())
    }
}

fn write_new(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    zip_name: &str,
    bytes: &[u8],
    level: u8,
) -> Result<(), OpcError> {
    let base = SimpleFileOptions::default()
        .last_modified_time(DateTime::default())
        .system(System::Dos)
        .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
    let opts = if level == 0 {
        base.compression_method(CompressionMethod::Stored)
    } else {
        base.compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(level.min(9))))
    };
    writer
        .start_file(zip_name, opts)
        .map_err(|e| OpcError::Write(e.to_string()))?;
    writer
        .write_all(bytes)
        .map_err(|e| OpcError::Write(e.to_string()))
}
