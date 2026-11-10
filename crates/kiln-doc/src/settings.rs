// SPDX-License-Identifier: AGPL-3.0-or-later
//! Document settings (`settings.xml`) and document properties (`docProps/core.xml`,
//! `docProps/app.xml`, `docProps/custom.xml`).

use serde::{Deserialize, Serialize};

use crate::st::NumberFormat;
use crate::theme::ColorSchemeMapping;

/// Footnote or endnote numbering settings (`w:footnotePr`, `w:endnotePr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct NoteSettings {
    /// `w:numFmt`.
    pub format: Option<NumberFormat>,
    /// `w:numStart`.
    pub start: Option<i64>,
    /// `w:numRestart` (`continuous`, `eachSect`, `eachPage`).
    pub restart: Option<String>,
    /// `w:pos` (`pageBottom`, `beneathText`, `sectEnd`, `docEnd`).
    pub position: Option<String>,
}

/// Settings that affect editing and layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Settings {
    /// `w:defaultTabStop`, twips.
    pub default_tab_stop: i32,
    /// `w:evenAndOddHeaders`.
    pub even_and_odd_headers: bool,
    /// `w:mirrorMargins`.
    pub mirror_margins: bool,
    /// `w:trackRevisions`: Track Changes is on.
    pub track_revisions: bool,
    /// `w:clrSchemeMapping`.
    pub color_scheme_mapping: ColorSchemeMapping,
    /// `w:compat/w:compatSetting[@name=compatibilityMode]`: 15 = Word 2013 and later.
    pub compatibility_mode: u32,
    /// `w:updateFields`: ask to update fields on open.
    pub update_fields: bool,
    /// `w:autoHyphenation`.
    pub auto_hyphenation: bool,
    /// `w:footnotePr`.
    pub footnotes: NoteSettings,
    /// `w:endnotePr`.
    pub endnotes: NoteSettings,
    /// `w:decimalSymbol`.
    pub decimal_symbol: Option<String>,
    /// `w:listSeparator`.
    pub list_separator: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_tab_stop: 720,
            even_and_odd_headers: false,
            mirror_margins: false,
            track_revisions: false,
            color_scheme_mapping: ColorSchemeMapping::default(),
            compatibility_mode: 15,
            update_fields: false,
            auto_hyphenation: false,
            footnotes: NoteSettings::default(),
            endnotes: NoteSettings::default(),
            decimal_symbol: None,
            list_separator: None,
        }
    }
}

/// The value of a custom document property, kept as written (numbers as their text, so a round
/// trip is exact).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertyValue {
    /// `vt:lpwstr`.
    Text(String),
    /// `vt:i4`.
    Integer(i64),
    /// `vt:r8`, as written.
    Real(String),
    /// `vt:bool`.
    Bool(bool),
    /// `vt:filetime`, ISO 8601.
    Date(String),
}

/// A custom property.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomProperty {
    /// The name.
    pub name: String,
    /// The value.
    pub value: PropertyValue,
}

/// Document properties (File ▸ Info).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DocProperties {
    /// `dc:title`.
    pub title: Option<String>,
    /// `dc:subject`.
    pub subject: Option<String>,
    /// `dc:creator`.
    pub creator: Option<String>,
    /// `cp:keywords`.
    pub keywords: Option<String>,
    /// `dc:description`.
    pub description: Option<String>,
    /// `cp:lastModifiedBy`.
    pub last_modified_by: Option<String>,
    /// `cp:revision`.
    pub revision: Option<u32>,
    /// `dcterms:created`, ISO 8601.
    pub created: Option<String>,
    /// `dcterms:modified`, ISO 8601.
    pub modified: Option<String>,
    /// `cp:category`.
    pub category: Option<String>,
    /// `cp:contentStatus`.
    pub content_status: Option<String>,
    /// `dc:language`.
    pub language: Option<String>,
    /// `Company` (app.xml).
    pub company: Option<String>,
    /// `Manager` (app.xml).
    pub manager: Option<String>,
    /// `Template` (app.xml).
    pub template: Option<String>,
    /// `TotalTime` in minutes (app.xml).
    pub total_time: Option<u32>,
    /// Custom properties in file order.
    pub custom: Vec<CustomProperty>,
}

/// An embedded media file (picture, font, OLE preview), stored by its part name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Media {
    /// The MIME type.
    pub content_type: String,
    /// The bytes.
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_word_2013_plus() {
        let s = Settings::default();
        assert_eq!(s.default_tab_stop, 720);
        assert_eq!(s.compatibility_mode, 15);
        assert!(!s.track_revisions && !s.even_and_odd_headers);
        let p = DocProperties::default();
        assert!(p.custom.is_empty() && p.title.is_none());
        let c = CustomProperty {
            name: "Project".into(),
            value: PropertyValue::Real("1.50".into()),
        };
        assert_eq!(c.value, PropertyValue::Real("1.50".into()));
    }
}
