// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `[Content_Types].xml` stream: extension defaults and per-part overrides.

use indexmap::IndexMap;

use super::part_name::{extension_of, part_key};
use crate::xml::{XmlElement, XmlParseError, element_to_bytes, namespaces::ns, parse_xml};

/// Common content types.
pub mod content_type {
    #![allow(missing_docs)]
    pub const RELATIONSHIPS: &str = "application/vnd.openxmlformats-package.relationships+xml";
    pub const CORE_PROPERTIES: &str = "application/vnd.openxmlformats-package.core-properties+xml";
    pub const EXTENDED_PROPERTIES: &str =
        "application/vnd.openxmlformats-officedocument.extended-properties+xml";
    pub const CUSTOM_PROPERTIES: &str =
        "application/vnd.openxmlformats-officedocument.custom-properties+xml";
    pub const THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
    pub const XML: &str = "application/xml";
    pub const PNG: &str = "image/png";
    pub const JPEG: &str = "image/jpeg";
    pub const GIF: &str = "image/gif";
    pub const BMP: &str = "image/bmp";
    pub const TIFF: &str = "image/tiff";
    pub const SVG: &str = "image/svg+xml";
    pub const EMF: &str = "image/x-emf";
    pub const WMF: &str = "image/x-wmf";
    pub const OCTET_STREAM: &str = "application/octet-stream";
    // WordprocessingML
    pub const DOCX_DOCUMENT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
    pub const DOCX_STYLES: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
    pub const DOCX_NUMBERING: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
    pub const DOCX_SETTINGS: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
    // SpreadsheetML
    pub const XLSX_WORKBOOK: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
    pub const XLSX_WORKSHEET: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
    pub const XLSX_SHARED_STRINGS: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
    pub const XLSX_STYLES: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
    // PresentationML
    pub const PPTX_PRESENTATION: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
    pub const PPTX_SLIDE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
}

/// Default content types for extensions Office registers as `<Default>` entries.
pub const DEFAULT_EXTENSION_TYPES: &[(&str, &str)] = &[
    ("rels", content_type::RELATIONSHIPS),
    ("xml", content_type::XML),
    ("png", content_type::PNG),
    ("jpeg", content_type::JPEG),
    ("jpg", content_type::JPEG),
    ("gif", content_type::GIF),
    ("bmp", content_type::BMP),
    ("tif", content_type::TIFF),
    ("tiff", content_type::TIFF),
    ("svg", content_type::SVG),
    ("emf", content_type::EMF),
    ("wmf", content_type::WMF),
    (
        "bin",
        "application/vnd.openxmlformats-officedocument.oleObject",
    ),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("mp4", "video/mp4"),
    ("mp3", "audio/mpeg"),
    ("m4a", "audio/mp4"),
    ("wav", "audio/wav"),
    (
        "odttf",
        "application/vnd.openxmlformats-officedocument.obfuscatedFont",
    ),
    ("fntdata", "application/x-fontdata"),
];

/// The usual content type for an extension, if Office registers one.
#[must_use]
pub fn default_content_type_for_extension(ext: &str) -> Option<&'static str> {
    DEFAULT_EXTENSION_TYPES
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
        .map(|(_, t)| *t)
}

/// True for XML part content types, which Office always registers as `<Override>`.
#[must_use]
pub fn is_xml_content_type(ct: &str) -> bool {
    ct.ends_with("+xml") || ct == "application/xml" || ct == "text/xml"
}

/// The content-types table, in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentTypes {
    /// Extension (lower case) → content type.
    defaults: IndexMap<String, String>,
    /// Part key (case-folded) → (part name as written, content type).
    overrides: IndexMap<String, (String, String)>,
    dirty: bool,
}

impl ContentTypes {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty table with Office's `rels` and `xml` defaults.
    #[must_use]
    pub fn office_defaults() -> Self {
        let mut ct = Self::new();
        ct.defaults
            .insert("rels".into(), content_type::RELATIONSHIPS.into());
        ct.defaults.insert("xml".into(), content_type::XML.into());
        ct.dirty = true;
        ct
    }

    /// Parses `[Content_Types].xml`.
    ///
    /// # Errors
    /// [`XmlParseError`] when the part is not XML.
    pub fn parse(bytes: &[u8]) -> Result<Self, XmlParseError> {
        let doc = parse_xml(bytes)?;
        let mut ct = Self::new();
        for d in doc.root.children_named("Default") {
            if let (Some(ext), Some(t)) = (d.attr("Extension"), d.attr("ContentType")) {
                ct.defaults.insert(ext.to_ascii_lowercase(), t.to_owned());
            }
        }
        for o in doc.root.children_named("Override") {
            if let (Some(name), Some(t)) = (o.attr("PartName"), o.attr("ContentType")) {
                ct.overrides
                    .insert(part_key(name), (name.to_owned(), t.to_owned()));
            }
        }
        Ok(ct)
    }

    /// True once the table differs from what was parsed.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The `<Default>` entries: (extension, content type).
    pub fn defaults(&self) -> impl Iterator<Item = (&str, &str)> {
        self.defaults.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The `<Override>` entries: (part name as written, content type).
    pub fn overrides(&self) -> impl Iterator<Item = (&str, &str)> {
        self.overrides
            .values()
            .map(|(n, t)| (n.as_str(), t.as_str()))
    }

    /// The default content type of an extension.
    #[must_use]
    pub fn default_for(&self, ext: &str) -> Option<&str> {
        self.defaults
            .get(&ext.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// The override of a part, if it has one.
    #[must_use]
    pub fn override_for(&self, part_name: &str) -> Option<&str> {
        self.overrides
            .get(&part_key(part_name))
            .map(|(_, t)| t.as_str())
    }

    /// The content type of a part: its override, else the default for its extension.
    #[must_use]
    pub fn get(&self, part_name: &str) -> Option<&str> {
        self.override_for(part_name)
            .or_else(|| self.default_for(&extension_of(part_name)))
    }

    /// Adds or replaces a `<Default>`.
    pub fn set_default(&mut self, ext: &str, content_type: &str) {
        let ext = ext.to_ascii_lowercase();
        if self.defaults.get(&ext).map(String::as_str) != Some(content_type) {
            self.defaults.insert(ext, content_type.to_owned());
            self.dirty = true;
        }
    }

    /// Registers a part's content type the way Office does: a `<Default>` for non-XML media
    /// extensions (adding one if the extension has none), otherwise an `<Override>`.
    pub fn set(&mut self, part_name: &str, content_type: &str) {
        let ext = extension_of(part_name);
        let key = part_key(part_name);
        let def = self.defaults.get(&ext).cloned();
        if def.as_deref() == Some(content_type) {
            if self.overrides.shift_remove(&key).is_some() {
                self.dirty = true;
            }
            return;
        }
        if def.is_none() && !ext.is_empty() && !is_xml_content_type(content_type) {
            self.defaults.insert(ext, content_type.to_owned());
            self.overrides.shift_remove(&key);
            self.dirty = true;
            return;
        }
        let same = self
            .overrides
            .get(&key)
            .is_some_and(|(_, t)| t == content_type);
        if !same {
            self.overrides
                .insert(key, (part_name.to_owned(), content_type.to_owned()));
            self.dirty = true;
        }
    }

    /// Removes a part's override.
    pub fn remove(&mut self, part_name: &str) {
        if self.overrides.shift_remove(&part_key(part_name)).is_some() {
            self.dirty = true;
        }
    }

    /// Drops `<Default>` entries for extensions no remaining part uses (except `rels` and `xml`).
    pub fn prune_defaults<'a>(&mut self, part_names: impl IntoIterator<Item = &'a str>) {
        let mut used: Vec<String> = vec!["rels".into(), "xml".into()];
        used.extend(part_names.into_iter().map(extension_of));
        let before = self.defaults.len();
        self.defaults.retain(|ext, _| used.contains(ext));
        if self.defaults.len() != before {
            self.dirty = true;
        }
    }

    /// The table as an XML tree.
    #[must_use]
    pub fn to_element(&self) -> XmlElement {
        XmlElement::new("Types")
            .with_attr("xmlns", ns::CT)
            .with_children(self.defaults.iter().map(|(ext, t)| {
                XmlElement::new("Default")
                    .with_attr("Extension", ext.as_str())
                    .with_attr("ContentType", t.as_str())
            }))
            .with_children(self.overrides.values().map(|(name, t)| {
                XmlElement::new("Override")
                    .with_attr("PartName", name.as_str())
                    .with_attr("ContentType", t.as_str())
            }))
    }

    /// `[Content_Types].xml` bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        // The tree declares its only namespace and uses no prefixes, so writing cannot fail.
        element_to_bytes(&self.to_element()).unwrap_or_default()
    }
}
