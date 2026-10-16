// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the reader keeps beside the model so that a save is faithful: the original package, the
//! XML of every part it read (with the modelled pieces replaced on write), the model values those
//! pieces produced (to tell edited from untouched), and the cell attributes the model has no
//! field for.

use std::collections::{BTreeMap, BTreeSet};

use kiln_ooxml::opc::OpcPackage;
use kiln_ooxml::xml::XmlElement;
use kiln_sheet::meta::{
    DefinedName, DrawingAnchor, Person, Table, WorkbookProps, WorkbookProtection,
};
use kiln_sheet::{Range, SheetId, SheetLayout, SheetMeta, Theme};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::styles::StylesSource;

/// The kind of workbook package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum XlsxKind {
    /// `.xlsx`.
    #[default]
    Workbook,
    /// `.xlsm` (macros).
    MacroEnabled,
    /// `.xltx`.
    Template,
    /// `.xltm`.
    MacroEnabledTemplate,
}

impl XlsxKind {
    /// The main part's content type.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            XlsxKind::Workbook => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
            }
            XlsxKind::MacroEnabled => "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
            XlsxKind::Template => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"
            }
            XlsxKind::MacroEnabledTemplate => {
                "application/vnd.ms-excel.template.macroEnabled.main+xml"
            }
        }
    }

    /// The kind of a main part content type.
    #[must_use]
    pub fn from_content_type(ct: &str) -> Option<XlsxKind> {
        [
            XlsxKind::Workbook,
            XlsxKind::MacroEnabled,
            XlsxKind::Template,
            XlsxKind::MacroEnabledTemplate,
        ]
        .into_iter()
        .find(|k| k.content_type().eq_ignore_ascii_case(ct))
    }

    /// The kind for a file extension (`xlsx`, `xlsm`, `xltx`, `xltm`).
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<XlsxKind> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "xlsx" => XlsxKind::Workbook,
            "xlsm" => XlsxKind::MacroEnabled,
            "xltx" => XlsxKind::Template,
            "xltm" => XlsxKind::MacroEnabledTemplate,
            _ => return None,
        })
    }

    /// Whether the kind carries macros.
    #[must_use]
    pub const fn has_macros(self) -> bool {
        matches!(
            self,
            XlsxKind::MacroEnabled | XlsxKind::MacroEnabledTemplate
        )
    }
}

/// Cell attributes and formula details the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CellExtra {
    /// `vm` (value metadata: rich values such as pictures in cells and linked data types).
    pub(crate) vm: Option<String>,
    /// `ph` (show phonetic text).
    pub(crate) ph: bool,
    /// Attributes of `<f>` other than `t`, `ref` and `si` (`ca`, `aca`, `bx`, …), written back
    /// while the cell has a formula.
    pub(crate) f_attrs: Vec<(String, String)>,
    /// A data-table formula (`t="dataTable"`) kept whole; the cell holds its last value.
    pub(crate) data_table: Option<XmlElement>,
    /// The formula text as stored in the file (with its `_xlfn.` prefixes), with the model text
    /// it produced: written back verbatim while the model formula is unchanged.
    pub(crate) file_formula: Option<(String, String)>,
}

/// A worksheet as read.
#[derive(Debug, Clone)]
pub(crate) struct SheetSource {
    pub(crate) id: SheetId,
    /// The worksheet part.
    pub(crate) part: String,
    /// The `<sheet>` element of `workbook.xml` (its `r:id` and unknown attributes).
    pub(crate) sheet_el: XmlElement,
    /// The worksheet root with an empty `<sheetData/>` placeholder.
    pub(crate) root: XmlElement,
    /// Metadata and row/column properties as read.
    pub(crate) meta: SheetMeta,
    pub(crate) layout: SheetLayout,
    /// Legacy array formulas (Ctrl+Shift+Enter): anchor → the range they fill.
    pub(crate) array_formulas: FxHashMap<(u32, u32), Range>,
    /// Dynamic-array anchors as read.
    pub(crate) dynamic: FxHashSet<(u32, u32)>,
    pub(crate) cells: FxHashMap<(u32, u32), CellExtra>,
    /// Row attributes the model has no field for (`x14ac:dyDescent`, `ph`, …).
    pub(crate) row_attrs: FxHashMap<u32, Vec<(String, String)>>,
    /// Hyperlinks as read with their relationship ids (target → id).
    pub(crate) hyperlink_rels: BTreeMap<String, String>,
    /// The `dataValidations` element read (its attributes are kept when rules change).
    pub(crate) dv_xml: Option<XmlElement>,
    /// Notes with their comments and VML parts.
    pub(crate) notes: Option<NotesSource>,
    /// The threaded comments part.
    pub(crate) threads_part: Option<String>,
    /// Tables: model id → (part, table as read, root element).
    pub(crate) tables: BTreeMap<u32, (String, Table, XmlElement)>,
    /// The drawing part and its anchors as read.
    pub(crate) drawing: Option<DrawingSource>,
}

/// Notes as read.
#[derive(Debug, Clone)]
pub(crate) struct NotesSource {
    pub(crate) comments_part: String,
    pub(crate) vml_part: Option<String>,
}

/// A drawing part as read.
#[derive(Debug, Clone)]
pub(crate) struct DrawingSource {
    pub(crate) part: String,
    pub(crate) anchors: Vec<DrawingAnchor>,
    pub(crate) root: XmlElement,
}

/// Everything the reader keeps for a faithful save. Opaque: hand it back to
/// [`crate::write_xlsx`].
#[derive(Debug, Clone)]
pub struct XlsxSource {
    pub(crate) package: OpcPackage,
    pub(crate) kind: XlsxKind,
    pub(crate) strict: bool,
    pub(crate) workbook_part: String,
    pub(crate) workbook_root: XmlElement,
    /// Chartsheets and dialog sheets: their position among all `<sheet>` entries and the entry.
    pub(crate) other_sheets: Vec<(usize, XmlElement)>,
    pub(crate) styles_part: Option<String>,
    pub(crate) styles: Option<StylesSource>,
    pub(crate) sst_part: Option<String>,
    pub(crate) theme_part: Option<String>,
    pub(crate) theme: Option<Theme>,
    pub(crate) metadata_part: Option<String>,
    /// The 1-based `cellMetadata` index of the dynamic-array flag in the metadata part as read.
    pub(crate) dynamic_cm: Option<u32>,
    pub(crate) persons_part: Option<String>,
    pub(crate) persons: Vec<Person>,
    pub(crate) sheets: Vec<SheetSource>,
    /// Defined names as read (model form) with their elements.
    pub(crate) names: Vec<(DefinedName, XmlElement)>,
    /// `definedName` elements the model could not take (invalid names, unknown scopes).
    pub(crate) raw_names: Vec<XmlElement>,
    pub(crate) props: WorkbookProps,
    pub(crate) protection: Option<WorkbookProtection>,
    /// Functions the file wrote with `_xlfn.` that the writer's table does not know.
    pub(crate) future_functions: BTreeSet<String>,
}

impl XlsxSource {
    /// The kind of package read.
    #[must_use]
    pub fn kind(&self) -> XlsxKind {
        self.kind
    }

    /// The original package.
    #[must_use]
    pub fn package(&self) -> &OpcPackage {
        &self.package
    }

    /// Whether the file used ISO/IEC 29500 Strict namespaces (it is saved as transitional).
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    pub(crate) fn sheet(&self, id: SheetId) -> Option<&SheetSource> {
        self.sheets.iter().find(|s| s.id == id)
    }
}
