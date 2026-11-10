// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content controls (`w:sdt`).

use serde::{Deserialize, Serialize};

use crate::st::SdtKind;

/// One entry of a drop-down list or combo box.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SdtListItem {
    /// `w:displayText`.
    pub display_text: String,
    /// `w:value`.
    pub value: String,
}

/// A content control's properties. The editor shows the title, tag and kind; `pr_xml` and
/// `end_pr_xml` keep the original `w:sdtPr`/`w:sdtEndPr` so the writer re-wraps the content
/// exactly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SdtInfo {
    /// The kind derived from `w:sdtPr`.
    pub kind: SdtKind,
    /// `w:id`.
    pub id: Option<i64>,
    /// `w:tag`.
    pub tag: Option<String>,
    /// `w:alias` (the title).
    pub alias: Option<String>,
    /// Building-block gallery (`Table of Contents`, `Cover Pages`, …).
    pub doc_part_gallery: Option<String>,
    /// Checkbox state.
    pub checked: Option<bool>,
    /// `w:showingPlcHdr`: the content is placeholder text.
    pub showing_placeholder: bool,
    /// Items of drop-down lists and combo boxes.
    pub list_items: Vec<SdtListItem>,
    /// Date picker format (`w:date/w:dateFormat`).
    pub date_format: Option<String>,
    /// `w:lock` (`sdtLocked`, `contentLocked`, `sdtContentLocked`).
    pub lock: Option<String>,
    /// The original `w:sdtPr` XML.
    pub pr_xml: String,
    /// The original `w:sdtEndPr` XML.
    pub end_pr_xml: Option<String>,
}

impl SdtInfo {
    /// True for a table-of-contents building block.
    #[must_use]
    pub fn is_toc(&self) -> bool {
        self.doc_part_gallery.as_deref() == Some("Table of Contents")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toc_building_blocks_are_recognised() {
        let mut s = SdtInfo {
            kind: SdtKind::DocPartObj,
            ..SdtInfo::default()
        };
        assert!(!s.is_toc());
        s.doc_part_gallery = Some("Table of Contents".into());
        assert!(s.is_toc());
    }
}
