// SPDX-License-Identifier: AGPL-3.0-or-later
//! The style sheet (`styles.xml`): paragraph, character, table and numbering styles with
//! `basedOn` inheritance, `next` (style of the following paragraph) and `link` (paragraph ↔
//! character linked styles), plus `w:docDefaults`.
//!
//! Latent style information (`w:latentStyles`) only drives Word's style gallery and is not
//! modelled; the DOCX layer passes it through.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::DocError;
use crate::props::{
    NumberingRef, ParagraphProps, RunProps, TableCellProps, TableProps, TableRowProps,
};
use crate::st::{StyleType, TableCondition};

/// Formatting of one conditional region of a table style (`w:tblStylePr`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableStyleCondition {
    /// Paragraph properties.
    pub ppr: Option<ParagraphProps>,
    /// Run properties.
    pub rpr: Option<RunProps>,
    /// Table properties.
    pub tbl_pr: Option<TableProps>,
    /// Row properties.
    pub tr_pr: Option<TableRowProps>,
    /// Cell properties.
    pub tc_pr: Option<TableCellProps>,
}

/// A style (`w:style`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Style {
    /// `w:styleId`: the key used by `w:pStyle`, `w:rStyle` and `w:tblStyle`.
    pub id: String,
    /// `w:type`.
    pub kind: StyleType,
    /// `w:name` as stored (built-in names are lower case in files: `heading 1`).
    pub name: String,
    /// `w:basedOn`.
    pub based_on: Option<String>,
    /// `w:next`: style of the paragraph created by Enter at the end of this one.
    pub next: Option<String>,
    /// `w:link`: the linked style of the other type.
    pub link: Option<String>,
    /// `w:default`: the default style of its type.
    pub is_default: bool,
    /// `w:customStyle`: user-defined rather than built in.
    pub custom: bool,
    /// `w:qFormat`: shown in the Styles gallery.
    pub quick_style: bool,
    /// `w:uiPriority`.
    pub ui_priority: Option<u32>,
    /// `w:semiHidden`.
    pub semi_hidden: bool,
    /// `w:unhideWhenUsed`.
    pub unhide_when_used: bool,
    /// `w:hidden`.
    pub hidden: bool,
    /// `w:locked`.
    pub locked: bool,
    /// `w:aliases`, split on commas.
    pub aliases: Vec<String>,
    /// `w:pPr`.
    pub ppr: Option<ParagraphProps>,
    /// `w:rPr`.
    pub rpr: Option<RunProps>,
    /// `w:tblPr` (table styles).
    pub tbl_pr: Option<TableProps>,
    /// `w:trPr` (table styles).
    pub tr_pr: Option<TableRowProps>,
    /// `w:tcPr` (table styles).
    pub tc_pr: Option<TableCellProps>,
    /// `w:tblStylePr` by region (table styles).
    pub conditions: BTreeMap<TableCondition, TableStyleCondition>,
}

impl Style {
    /// A style with an id, type and name and nothing else.
    #[must_use]
    pub fn new(id: &str, kind: StyleType, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            kind,
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// Word's display name for this style.
    #[must_use]
    pub fn display_name(&self) -> String {
        style_display_name(&self.name)
    }
}

/// `w:docDefaults`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocDefaults {
    /// `w:pPrDefault`.
    pub ppr: ParagraphProps,
    /// `w:rPrDefault`.
    pub rpr: RunProps,
}

/// Word's UI name for a stored style name (`heading 1` → `Heading 1`, `toc 2` → `TOC 2`).
#[must_use]
pub fn style_display_name(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "toc heading" => return "TOC Heading".to_owned(),
        "table of figures" => return "Table of Figures".to_owned(),
        "html preformatted" => return "HTML Preformatted".to_owned(),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix("toc ")
        && rest.len() == 1
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return format!("TOC {rest}");
    }
    if name != lower {
        return name.to_owned();
    }
    let mut out = String::with_capacity(name.len());
    let mut start = true;
    for c in name.chars() {
        if start && c.is_lowercase() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}

/// Builds a style id from a display name the way Word does (`Heading 1` → `Heading1`).
#[must_use]
pub fn style_id_from_name(name: &str) -> String {
    let id: String = style_display_name(name)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    if id.is_empty() {
        "Style".to_owned()
    } else {
        id
    }
}

/// A partial update of a style ([`StyleSheet::update`]); `None` leaves a field unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StylePatch {
    /// New name.
    pub name: Option<String>,
    /// New `basedOn` (`Some(None)` clears it).
    pub based_on: Option<Option<String>>,
    /// New `next` (`Some(None)` clears it).
    pub next: Option<Option<String>>,
    /// New aliases.
    pub aliases: Option<Vec<String>>,
    /// New paragraph properties (`Some(None)` clears them).
    pub ppr: Option<Option<ParagraphProps>>,
    /// New run properties (`Some(None)` clears them).
    pub rpr: Option<Option<RunProps>>,
}

/// The style registry, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StyleSheet {
    /// `w:docDefaults`.
    pub doc_defaults: DocDefaults,
    styles: IndexMap<String, Style>,
}

impl StyleSheet {
    /// A sheet from defaults and styles. Later styles with a repeated id replace earlier ones and
    /// a later default style of a type clears the previous default.
    #[must_use]
    pub fn new(doc_defaults: DocDefaults, styles: impl IntoIterator<Item = Style>) -> Self {
        let mut sheet = Self {
            doc_defaults,
            styles: IndexMap::new(),
        };
        for s in styles {
            // An empty id is the only rejection; skipping it keeps construction infallible.
            let _ = sheet.add(s);
        }
        sheet
    }

    /// Number of styles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// True when there are no styles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// All styles in file order.
    pub fn iter(&self) -> impl Iterator<Item = &Style> {
        self.styles.values()
    }

    /// Styles of one type, in file order.
    pub fn by_type(&self, kind: StyleType) -> impl Iterator<Item = &Style> {
        self.styles.values().filter(move |s| s.kind == kind)
    }

    /// True when a style with this id exists.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.styles.contains_key(id)
    }

    /// The style with this id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Style> {
        self.styles.get(id)
    }

    /// The position of a style in file order.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.styles.get_index_of(id)
    }

    /// Finds a style by stored name, display name or alias (case-insensitive).
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Option<&Style> {
        let key = name.trim().to_lowercase();
        self.styles.values().find(|s| {
            s.name.to_lowercase() == key
                || style_display_name(&s.name).to_lowercase() == key
                || s.aliases.iter().any(|a| a.to_lowercase() == key)
        })
    }

    /// Adds or replaces a style (a replaced style keeps its position). A new default style
    /// clears the previous default of its type.
    ///
    /// # Errors
    ///
    /// [`DocError::InvalidStyle`] when the id is empty.
    pub fn add(&mut self, style: Style) -> Result<(), DocError> {
        if style.id.is_empty() {
            return Err(DocError::InvalidStyle("style id must not be empty".into()));
        }
        if style.is_default {
            for s in self.styles.values_mut() {
                if s.kind == style.kind && s.id != style.id {
                    s.is_default = false;
                }
            }
        }
        self.styles.insert(style.id.clone(), style);
        Ok(())
    }

    /// Inserts or replaces a style at a position (used by undo to restore order exactly).
    pub(crate) fn put_at(&mut self, style: Style, index: usize) -> Option<Style> {
        if let Some(slot) = self.styles.get_mut(&style.id) {
            return Some(std::mem::replace(slot, style));
        }
        let index = index.min(self.styles.len());
        self.styles.shift_insert(index, style.id.clone(), style);
        None
    }

    /// Removes a style without touching references to it (the raw operation behind
    /// [`Self::remove`]).
    pub(crate) fn take(&mut self, id: &str) -> Option<(usize, Style)> {
        let (index, _, style) = self.styles.shift_remove_full(id)?;
        Some((index, style))
    }

    /// Merges a patch into a style.
    ///
    /// # Errors
    ///
    /// [`DocError::UnknownStyle`] for an unknown id, [`DocError::InvalidStyle`] when the new
    /// `basedOn` would create a cycle.
    pub fn update(&mut self, id: &str, patch: StylePatch) -> Result<&Style, DocError> {
        if !self.contains(id) {
            return Err(DocError::UnknownStyle(id.to_owned()));
        }
        if let Some(Some(parent)) = &patch.based_on
            && self.would_cycle(id, parent)
        {
            return Err(DocError::InvalidStyle(format!(
                "basedOn cycle: {id} → {parent}"
            )));
        }
        let s = self
            .styles
            .get_mut(id)
            .ok_or_else(|| DocError::UnknownStyle(id.to_owned()))?;
        if let Some(v) = patch.name {
            s.name = v;
        }
        if let Some(v) = patch.based_on {
            s.based_on = v;
        }
        if let Some(v) = patch.next {
            s.next = v;
        }
        if let Some(v) = patch.aliases {
            s.aliases = v;
        }
        if let Some(v) = patch.ppr {
            s.ppr = v;
        }
        if let Some(v) = patch.rpr {
            s.rpr = v;
        }
        Ok(s)
    }

    /// Removes a style. Styles based on it are re-parented to its own `basedOn` (as Word does)
    /// and `next`/`link` references to it are dropped. Returns false for an unknown id.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some((_, removed)) = self.take(id) else {
            return false;
        };
        for other in self.styles.values_mut() {
            if other.based_on.as_deref() == Some(id) {
                other.based_on.clone_from(&removed.based_on);
            }
            if other.next.as_deref() == Some(id) {
                other.next = None;
            }
            if other.link.as_deref() == Some(id) {
                other.link = None;
            }
        }
        true
    }

    /// The default style of a type (`Normal`, `Default Paragraph Font`, `Normal Table`,
    /// `No List`).
    #[must_use]
    pub fn default_style(&self, kind: StyleType) -> Option<&Style> {
        self.styles
            .values()
            .find(|s| s.kind == kind && s.is_default)
    }

    /// The `basedOn` chain of a style from the root ancestor to the style itself. Cycles, missing
    /// parents and parents of another type end the chain (Word ignores such links).
    #[must_use]
    pub fn chain(&self, id: Option<&str>) -> Vec<&Style> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = id.and_then(|i| self.get(i));
        let kind = cur.map(|s| s.kind);
        while let Some(s) = cur {
            if Some(s.kind) != kind || !seen.insert(s.id.as_str()) {
                break;
            }
            out.push(s);
            cur = s.based_on.as_deref().and_then(|b| self.get(b));
        }
        out.reverse();
        out
    }

    /// The style of the paragraph created after one in `id` (Word's "Style for following
    /// paragraph").
    #[must_use]
    pub fn next_style_id(&self, id: &str) -> String {
        if let Some(s) = self.get(id) {
            if let Some(n) = &s.next
                && self.contains(n)
            {
                return n.clone();
            }
            return s.id.clone();
        }
        self.default_style(StyleType::Paragraph)
            .map_or_else(|| "Normal".to_owned(), |s| s.id.clone())
    }

    /// The linked style of the other type (paragraph ↔ character).
    #[must_use]
    pub fn linked(&self, id: &str) -> Option<&Style> {
        let s = self.get(id)?;
        let other = self.get(s.link.as_deref()?)?;
        (other.kind != s.kind).then_some(other)
    }

    /// Quick styles for the Styles gallery, ordered as Word orders them (`uiPriority`, then
    /// display name).
    #[must_use]
    pub fn quick_styles(&self) -> Vec<&Style> {
        let mut out: Vec<&Style> = self
            .styles
            .values()
            .filter(|s| {
                s.quick_style
                    && !s.hidden
                    && matches!(s.kind, StyleType::Paragraph | StyleType::Character)
            })
            .collect();
        out.sort_by(|a, b| {
            a.ui_priority
                .unwrap_or(99)
                .cmp(&b.ui_priority.unwrap_or(99))
                .then_with(|| a.display_name().cmp(&b.display_name()))
        });
        out
    }

    /// The list a paragraph style chain contributes (nearest level wins; `numId` 0 cancels).
    #[must_use]
    pub fn style_numbering(&self, id: Option<&str>) -> Option<(u32, u8)> {
        let mut found = NumberingRef::default();
        for s in self.chain(id) {
            if let Some(n) = s.ppr.as_ref().and_then(|p| p.numbering) {
                if n.num_id.is_some() {
                    found.num_id = n.num_id;
                }
                if n.level.is_some() {
                    found.level = n.level;
                }
            }
        }
        match found.num_id {
            None | Some(0) => None,
            Some(num) => Some((num, found.level.unwrap_or(0))),
        }
    }

    /// True when `ancestor` is `id` or one of its `basedOn` ancestors.
    #[must_use]
    pub fn inherits_from(&self, id: &str, ancestor: &str) -> bool {
        self.chain(Some(id)).iter().any(|s| s.id == ancestor)
    }

    /// An unused style id for a new style named `name`.
    #[must_use]
    pub fn unique_id(&self, name: &str) -> String {
        let base = style_id_from_name(name);
        if !self.contains(&base) {
            return base;
        }
        (1u32..)
            .map(|i| format!("{base}{i}"))
            .find(|id| !self.contains(id))
            .unwrap_or(base)
    }

    fn would_cycle(&self, id: &str, based_on: &str) -> bool {
        let mut cur = Some(based_on);
        let mut seen = HashSet::new();
        while let Some(c) = cur {
            if c == id {
                return true;
            }
            if !seen.insert(c) {
                return false;
            }
            cur = self.get(c).and_then(|s| s.based_on.as_deref());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::default_style_sheet;

    #[test]
    fn display_names_and_ids() {
        assert_eq!(style_display_name("heading 1"), "Heading 1");
        assert_eq!(style_display_name("toc 2"), "TOC 2");
        assert_eq!(style_display_name("footnote text"), "Footnote Text");
        assert_eq!(style_display_name("My Style"), "My Style");
        assert_eq!(style_display_name("toc heading"), "TOC Heading");
        assert_eq!(style_id_from_name("heading 1"), "Heading1");
        assert_eq!(style_id_from_name("List Paragraph"), "ListParagraph");
        assert_eq!(style_id_from_name("!!"), "Style");
    }

    #[test]
    fn finds_styles_by_stored_name_display_name_and_alias() -> Result<(), DocError> {
        let mut s = default_style_sheet();
        assert_eq!(
            s.get_by_name("Heading 1").map(|x| x.id.as_str()),
            Some("Heading1")
        );
        assert_eq!(
            s.get_by_name("heading 1").map(|x| x.id.as_str()),
            Some("Heading1")
        );
        s.update(
            "Quote",
            StylePatch {
                aliases: Some(vec!["Citation".into()]),
                ..StylePatch::default()
            },
        )?;
        assert_eq!(
            s.get_by_name("citation").map(|x| x.id.as_str()),
            Some("Quote")
        );
        Ok(())
    }

    #[test]
    fn rejects_based_on_cycles_and_reparents_on_remove() -> Result<(), DocError> {
        let mut s = default_style_sheet();
        let cycle = s.update(
            "Normal",
            StylePatch {
                based_on: Some(Some("Heading1".into())),
                ..StylePatch::default()
            },
        );
        assert!(matches!(cycle, Err(DocError::InvalidStyle(_))));
        assert!(matches!(
            s.update("Nope", StylePatch::default()),
            Err(DocError::UnknownStyle(_))
        ));
        let mut child = Style::new("Child", StyleType::Paragraph, "Child");
        child.based_on = Some("Heading1".into());
        child.next = Some("Heading1".into());
        s.add(child)?;
        assert!(s.remove("Heading1"));
        assert!(!s.remove("Heading1"));
        assert_eq!(
            s.get("Child").and_then(|c| c.based_on.as_deref()),
            Some("Normal")
        );
        assert_eq!(s.get("Child").and_then(|c| c.next.as_deref()), None);
        assert!(s.add(Style::default()).is_err());
        Ok(())
    }

    #[test]
    fn only_one_default_per_type() -> Result<(), DocError> {
        let mut s = default_style_sheet();
        let mut body = Style::new("Body", StyleType::Paragraph, "Body");
        body.is_default = true;
        s.add(body)?;
        assert_eq!(
            s.default_style(StyleType::Paragraph).map(|x| x.id.as_str()),
            Some("Body")
        );
        assert_eq!(s.get("Normal").map(|x| x.is_default), Some(false));
        Ok(())
    }

    #[test]
    fn chains_stop_at_a_parent_of_another_type() {
        let mut p = Style::new("P", StyleType::Paragraph, "P");
        p.based_on = Some("C".into());
        let s = StyleSheet::new(
            DocDefaults::default(),
            [Style::new("C", StyleType::Character, "C"), p],
        );
        let ids: Vec<_> = s.chain(Some("P")).iter().map(|x| x.id.clone()).collect();
        assert_eq!(ids, vec!["P"]);
        assert!(s.chain(None).is_empty());
        assert!(s.inherits_from("P", "P") && !s.inherits_from("P", "C"));
    }

    #[test]
    fn linked_styles_unique_ids_and_next() {
        let s = default_style_sheet();
        assert_eq!(
            s.linked("Heading1").map(|x| x.id.as_str()),
            Some("Heading1Char")
        );
        assert_eq!(
            s.linked("Heading1Char").map(|x| x.id.as_str()),
            Some("Heading1")
        );
        assert_eq!(s.linked("Normal"), None);
        assert_eq!(s.unique_id("Heading 1"), "Heading11");
        assert_eq!(s.unique_id("Fresh Style"), "FreshStyle");
        assert_eq!(s.next_style_id("Heading1"), "Normal");
        assert_eq!(s.next_style_id("ListParagraph"), "ListParagraph");
        assert_eq!(s.next_style_id("Missing"), "Normal");
        assert!(s.by_type(StyleType::Table).count() >= 2);
        assert!(!s.is_empty() && s.len() > 40);
        assert_eq!(s.index_of("Normal"), Some(0));
    }

    #[test]
    fn clone_is_deep() -> Result<(), DocError> {
        let s = default_style_sheet();
        let mut c = s.clone();
        c.update(
            "Normal",
            StylePatch {
                rpr: Some(Some(RunProps {
                    size: Some(30),
                    ..RunProps::default()
                })),
                ..StylePatch::default()
            },
        )?;
        assert_eq!(s.get("Normal").and_then(|n| n.rpr.as_ref()), None);
        Ok(())
    }

    #[test]
    fn put_at_and_take_restore_order() {
        let mut s = default_style_sheet();
        let before = s.clone();
        let Some((index, style)) = s.take("Heading2") else {
            panic!("Heading2 exists");
        };
        assert!(!s.contains("Heading2"));
        assert_eq!(s.put_at(style, index), None);
        assert_eq!(s, before);
    }
}
