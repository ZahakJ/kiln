// SPDX-License-Identifier: AGPL-3.0-or-later
//! document properties (File ▸ Info ▸ Properties) for Kiln Grid and Kiln Deck, and the
//! personal information Inspect Document removes. Kiln Write keeps its properties in its model;
//! a workbook or presentation keeps the ones the user set here, written over the properties of
//! the file it was read from when it is saved (`.xlsx`, `.pptx`, and the native bundle as
//! `passthrough/properties.json`).

use kiln_common::{KilnError, Result};
use kiln_ooxml::opc::OpcPackage;
use kiln_ooxml::props::{
    DocumentProperties, PropertiesUpdate, read_document_properties, write_document_properties,
};
use kiln_protocol::commands::DocPropertiesInfo;
use kiln_protocol::commands::write_features::PropertiesEdit;
use serde::{Deserialize, Serialize};

/// The bundle passthrough entry of the properties set in Kiln.
pub(crate) const BUNDLE_ENTRY: &str = "properties.json";

/// The longest property value.
const MAX_CHARS: usize = 1_024;

/// The properties a user edits, as set (each `None` is empty).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UserProps {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub keywords: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub manager: Option<String>,
    #[serde(default)]
    pub last_modified_by: Option<String>,
}

impl UserProps {
    /// The editable properties of a package's document properties.
    pub(crate) fn from_ooxml(p: &DocumentProperties) -> Self {
        Self {
            title: p.core.title.clone(),
            subject: p.core.subject.clone(),
            author: p.core.creator.clone(),
            keywords: p.core.keywords.clone(),
            description: p.core.description.clone(),
            category: p.core.category.clone(),
            status: p.core.content_status.clone(),
            company: p.app.company.clone(),
            manager: p.app.manager.clone(),
            last_modified_by: p.core.last_modified_by.clone(),
        }
    }

    /// Writes these over a package's document properties.
    pub(crate) fn apply_ooxml(&self, p: &mut DocumentProperties) {
        p.core.title.clone_from(&self.title);
        p.core.subject.clone_from(&self.subject);
        p.core.creator.clone_from(&self.author);
        p.core.keywords.clone_from(&self.keywords);
        p.core.description.clone_from(&self.description);
        p.core.category.clone_from(&self.category);
        p.core.content_status.clone_from(&self.status);
        p.core.last_modified_by.clone_from(&self.last_modified_by);
        p.app.company.clone_from(&self.company);
        p.app.manager.clone_from(&self.manager);
    }

    /// Applies an edit: every `Some` field is set (an empty or blank value clears it). Returns
    /// whether anything changed.
    pub(crate) fn edit(&mut self, e: &PropertiesEdit) -> Result<bool> {
        let mut next = self.clone();
        let set = |field: &mut Option<String>, v: &Option<String>| -> Result<()> {
            if let Some(v) = v {
                if v.chars().count() > MAX_CHARS {
                    return Err(KilnError::InvalidInput(
                        "a property holds at most 1,024 characters".into(),
                    ));
                }
                *field = (!v.trim().is_empty()).then(|| v.trim().to_owned());
            }
            Ok(())
        };
        set(&mut next.title, &e.title)?;
        set(&mut next.subject, &e.subject)?;
        set(&mut next.author, &e.author)?;
        set(&mut next.keywords, &e.keywords)?;
        set(&mut next.description, &e.description)?;
        set(&mut next.category, &e.category)?;
        set(&mut next.status, &e.status)?;
        set(&mut next.company, &e.company)?;
        set(&mut next.manager, &e.manager)?;
        set(&mut next.last_modified_by, &e.last_modified_by)?;
        if next == *self {
            return Ok(false);
        }
        *self = next;
        Ok(true)
    }

    /// The properties as the Properties dialog shows them (counts left at zero).
    pub(crate) fn info(&self) -> DocPropertiesInfo {
        DocPropertiesInfo {
            title: self.title.clone(),
            subject: self.subject.clone(),
            author: self.author.clone(),
            keywords: self.keywords.clone(),
            description: self.description.clone(),
            last_modified_by: self.last_modified_by.clone(),
            category: self.category.clone(),
            status: self.status.clone(),
            company: self.company.clone(),
            manager: self.manager.clone(),
            ..DocPropertiesInfo::default()
        }
    }

    /// The properties of a saved package with these written over them.
    pub(crate) fn patch_package(&self, bytes: &[u8]) -> std::result::Result<Vec<u8>, String> {
        let mut pkg = OpcPackage::open(bytes).map_err(|e| e.to_string())?;
        let mut props = read_document_properties(&pkg).map_err(|e| e.to_string())?;
        self.apply_ooxml(&mut props);
        write_document_properties(
            &mut pkg,
            &PropertiesUpdate {
                core: Some(props.core),
                app: Some(props.app),
                custom: None,
            },
        )
        .map_err(|e| e.to_string())?;
        pkg.save().map_err(|e| e.to_string())
    }

    /// The properties kept in a bundle, if any.
    pub(crate) fn from_bundle(b: &kiln_storage::Bundle) -> Option<Self> {
        b.passthrough
            .get(BUNDLE_ENTRY)
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
    }

    /// Keeps the properties in a bundle.
    pub(crate) fn to_bundle(&self, b: &mut kiln_storage::Bundle) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            b.passthrough.insert(BUNDLE_ENTRY.into(), bytes);
        }
    }
}

/// How many pieces of personal information the properties hold (author, last modified by,
/// company, manager), for Inspect Document.
pub(crate) fn personal_count(p: &DocPropertiesInfo) -> u32 {
    [&p.author, &p.last_modified_by, &p.company, &p.manager]
        .into_iter()
        .filter(|v| v.as_deref().is_some_and(|s| !s.trim().is_empty()))
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

/// The edit Inspect Document ▸ Document Properties and Personal Information ▸ Remove All makes.
pub(crate) fn clear_personal() -> PropertiesEdit {
    PropertiesEdit {
        author: Some(String::new()),
        last_modified_by: Some(String::new()),
        company: Some(String::new()),
        manager: Some(String::new()),
        ..PropertiesEdit::default()
    }
}

/// the untitled names of new documents in the language they are created in (Office's
/// own: Document1 / Dokument1 / مستند1), with Western digits. Unknown languages keep English.
pub(crate) fn untitled_stem(kind: kiln_protocol::DocKind, language: Option<&str>) -> &'static str {
    use kiln_protocol::DocKind;
    let primary = language
        .and_then(|l| l.split(['-', '_']).next())
        .map(str::to_ascii_lowercase);
    match (primary.as_deref(), kind) {
        (Some("de"), DocKind::Write) => "Dokument",
        (Some("de"), DocKind::Grid) => "Mappe",
        (Some("de"), DocKind::Deck) => "Präsentation",
        (Some("ar"), DocKind::Write) => "مستند",
        (Some("ar"), DocKind::Grid) => "مصنف",
        (Some("ar"), DocKind::Deck) => "عرض تقديمي",
        _ => kind.untitled_stem(),
    }
}

/// The name of a new workbook's first sheet in a language (Sheet1 / Tabelle1 / ورقة1).
pub(crate) fn first_sheet_name(language: &str) -> String {
    let primary = language
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    format!("{}1", kiln_sheet::sheet_stem_for(&primary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::DocKind;

    #[test]
    fn untitled_names_follow_the_language() {
        assert_eq!(untitled_stem(DocKind::Write, None), "Document");
        assert_eq!(untitled_stem(DocKind::Grid, Some("de-DE")), "Mappe");
        assert_eq!(untitled_stem(DocKind::Deck, Some("de")), "Präsentation");
        assert_eq!(untitled_stem(DocKind::Write, Some("ar-SA")), "مستند");
        assert_eq!(untitled_stem(DocKind::Grid, Some("ar_EG")), "مصنف");
        assert_eq!(untitled_stem(DocKind::Deck, Some("ar")), "عرض تقديمي");
        assert_eq!(untitled_stem(DocKind::Write, Some("he-IL")), "Document");
        assert_eq!(first_sheet_name("de-AT"), "Tabelle1");
        assert_eq!(first_sheet_name("ar-SA"), "ورقة1");
        assert_eq!(first_sheet_name("fr-FR"), "Sheet1");
    }

    #[test]
    fn an_edit_sets_trims_and_clears() {
        let mut p = UserProps::default();
        assert!(
            p.edit(&PropertiesEdit {
                title: Some("  Budget  ".into()),
                company: Some("Kiln".into()),
                ..PropertiesEdit::default()
            })
            .unwrap()
        );
        assert_eq!(p.title.as_deref(), Some("Budget"));
        assert_eq!(personal_count(&p.info()), 1);
        assert!(p.edit(&clear_personal()).unwrap());
        assert_eq!(p.company, None);
        assert_eq!(personal_count(&p.info()), 0);
        assert!(!p.edit(&PropertiesEdit::default()).unwrap());
        assert!(
            p.edit(&PropertiesEdit {
                title: Some("x".repeat(1_025)),
                ..PropertiesEdit::default()
            })
            .is_err()
        );
    }
}
