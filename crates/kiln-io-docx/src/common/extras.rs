// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the reader keeps beside the [`kiln_doc::Document`] so the writer can reproduce the file:
//! raw XML of the properties the model has no field for, original ids, and the document-level
//! XML (`w:latentStyles`, the settings root, `w:background`).
//!
//! Per-node extras are keyed by [`NodeId`]; run extras by character range within their
//! paragraph. They describe the file as read: after an edit, a writer applies them only to
//! nodes that still exist (ids are never reused) and to runs whose range is unchanged.
//!
//! Raw XML fragments use the stable Office prefixes without declaring them (see
//! [`crate::common::xml::fragment_xml`]).

use std::collections::{BTreeMap, BTreeSet};

use kiln_doc::props::{Border, Lang};
use kiln_doc::settings::NoteSettings;
use kiln_doc::{NodeId, SdtInfo};

/// Unmodelled run properties of one character range (`w:noProof`, `w:em`, `w14:*` effects, …).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunExtras {
    /// First char of the range in the paragraph.
    pub start: usize,
    /// One past the last char.
    pub end: usize,
    /// Unmodelled `w:rPr` children, in file order.
    pub xml: Vec<String>,
    /// `w:rFonts/@w:hint` (`eastAsia`, `cs`, `default`).
    pub font_hint: Option<String>,
}

/// Paragraph details the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphExtras {
    /// Unmodelled `w:pPr` children (`w:framePr`, `w:snapToGrid`, `w:cnfStyle`, …), in file order.
    pub ppr_xml: Vec<String>,
    /// Unmodelled children of the paragraph mark's `w:rPr`.
    pub mark_xml: Vec<String>,
    /// `w14:paraId`.
    pub para_id: Option<String>,
    /// `w14:textId`.
    pub text_id: Option<String>,
    /// Run-level extras.
    pub runs: Vec<RunExtras>,
}

impl ParagraphExtras {
    /// True when there is nothing to keep.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ppr_xml.is_empty()
            && self.mark_xml.is_empty()
            && self.para_id.is_none()
            && self.text_id.is_none()
            && self.runs.is_empty()
    }
}

/// Row details the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowExtras {
    /// Unmodelled `w:trPr` children (`w:wBefore`, `w:cnfStyle`, `w:trPrChange`, …).
    pub xml: Vec<String>,
    /// `w:tblPrEx` (table property exceptions of this row).
    pub tbl_pr_ex: Option<String>,
    /// Row-level content controls around the row, outermost first.
    pub sdt: Vec<SdtInfo>,
}

/// Cell details the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellExtras {
    /// Unmodelled `w:tcPr` children (`w:cnfStyle`, `w:cellIns`, `w:tcPrChange`, …).
    pub xml: Vec<String>,
    /// Cell-level content controls around the cell, outermost first.
    pub sdt: Vec<SdtInfo>,
}

/// Page borders (`w:pgBorders`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageBorders {
    /// `w:offsetFrom` (`page` or `text`).
    pub offset_from: Option<String>,
    /// `w:display` (`allPages`, `firstPage`, `notFirstPage`).
    pub display: Option<String>,
    /// `w:zOrder` (`front`, `back`).
    pub z_order: Option<String>,
    /// Top edge.
    pub top: Option<Border>,
    /// Left edge.
    pub left: Option<Border>,
    /// Bottom edge.
    pub bottom: Option<Border>,
    /// Right edge.
    pub right: Option<Border>,
}

/// Section details the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionExtras {
    /// Page borders.
    pub page_borders: Option<PageBorders>,
    /// Section-level footnote properties (`w:footnotePr`).
    pub footnote_pr: Option<NoteSettings>,
    /// Section-level endnote properties (`w:endnotePr`).
    pub endnote_pr: Option<NoteSettings>,
    /// `w:textDirection`.
    pub text_direction: Option<String>,
    /// `w:rtlGutter`.
    pub rtl_gutter: bool,
    /// `w:formProt`.
    pub form_prot: bool,
    /// `w:noEndnote`.
    pub no_endnote: bool,
    /// `w:sectPrChange` (tracked section property change), raw XML.
    pub sect_pr_change: Option<String>,
    /// Every other unmodelled `w:sectPr` child (`w:paperSrc`, `w:printerSettings`, a
    /// `w:pgNumType` with chapter numbering, …), raw XML in file order.
    pub xml: Vec<String>,
    /// `w:rsidR`/`w:rsidSect` and other attributes of the `w:sectPr` element.
    pub attrs: Vec<(String, String)>,
}

/// `w:documentProtection`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Protection {
    /// `w:edit` (`readOnly`, `comments`, `trackedChanges`, `forms`).
    pub edit: Option<String>,
    /// `w:enforcement`.
    pub enforced: bool,
}

/// `settings.xml` values the model's [`kiln_doc::Settings`] does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsExtras {
    /// `w:doNotTrackMoves`.
    pub do_not_track_moves: Option<bool>,
    /// `w:doNotTrackFormatting`.
    pub do_not_track_formatting: Option<bool>,
    /// `w:themeFontLang`.
    pub theme_font_lang: Option<Lang>,
    /// `w:documentProtection` (the password hash stays in [`DocxExtras::settings_xml`]).
    pub protection: Option<Protection>,
    /// The page colour (`w:background/@w:color`, `RRGGBB`), shown when `w:displayBackgroundShape`
    /// asks for it.
    pub background_color: Option<String>,
}

/// A comment author from `people.xml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Person {
    /// `w15:author`.
    pub author: String,
    /// `w15:presenceInfo/@w15:providerId`.
    pub provider_id: Option<String>,
    /// `w15:presenceInfo/@w15:userId`.
    pub user_id: Option<String>,
}

/// Everything the reader keeps for the writer beside the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocxExtras {
    /// Attributes of the `w:document` root (namespace declarations, `mc:Ignorable`).
    pub document_root_attrs: Vec<(String, String)>,
    /// `w:document/w:background`, raw XML.
    pub background: Option<String>,
    /// `w:latentStyles` of `styles.xml`, raw XML.
    pub latent_styles: Option<String>,
    /// Every `w:style` as read, raw XML by style id (for properties the model does not cover).
    pub styles_xml: BTreeMap<String, String>,
    /// `w:numPicBullet` definitions of `numbering.xml`, raw XML.
    pub num_pic_bullets: Vec<String>,
    /// Every `w:abstractNum` as read, raw XML by abstract id.
    pub abstract_nums_xml: BTreeMap<u32, String>,
    /// The `settings.xml` root, raw XML, so a writer can patch modelled values and keep the rest.
    pub settings_xml: Option<String>,
    /// Settings beyond the model.
    pub settings: SettingsExtras,
    /// Paragraph extras.
    pub paragraphs: BTreeMap<NodeId, ParagraphExtras>,
    /// Unmodelled `w:tblPr` children (`w:tblPrChange`, `w:tblInd` in percent, …) by table.
    pub tables: BTreeMap<NodeId, Vec<String>>,
    /// Row extras.
    pub rows: BTreeMap<NodeId, RowExtras>,
    /// Cell extras.
    pub cells: BTreeMap<NodeId, CellExtras>,
    /// End-of-cell paragraphs the reader added because the file's cell ended with a nested
    /// table and no paragraph. The model keeps Word's invariant (every cell ends with a
    /// paragraph); the writer leaves such a paragraph out again while it is still empty and
    /// unformatted and still follows a table, so an unedited cell is saved as it was (Word and
    /// LibreOffice lay the two forms out differently).
    pub cell_end_paragraphs: BTreeSet<NodeId>,
    /// Section extras.
    pub sections: BTreeMap<NodeId, SectionExtras>,
    /// The part each header or footer story was read from.
    pub header_footer_parts: BTreeMap<NodeId, String>,
    /// The original `w:id` of each footnote and endnote.
    pub note_ids: BTreeMap<NodeId, String>,
    /// The original `w:id` of each comment.
    pub comment_ids: BTreeMap<NodeId, String>,
    /// Comment authors (`people.xml`).
    pub people: Vec<Person>,
}
