// SPDX-License-Identifier: AGPL-3.0-or-later
//! The stable namespace prefix table.
//!
//! Office writes these prefixes, so using the same ones keeps Kiln's output diffable against Office
//! files and lets readers query elements by qualified name (`w:pPr`). The table order is also the
//! order in which the writer adds namespace declarations it has to supply itself.
//!
//! Namespaces that Office serialises as the *default* namespace (SpreadsheetML, content types,
//! relationships, extended and custom properties) come out of the parser unprefixed
//! (`sheetData`, `Relationship`, `Override`, `Application`, `property`).

/// `(prefix, namespace URI)` in the stable order. The first entry for a URI wins when mapping a URI
/// to its prefix.
pub const NAMESPACES: &[(&str, &str)] = &[
    // WordprocessingML
    (
        "w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    ("w10", "urn:schemas-microsoft-com:office:word"),
    (
        "w14",
        "http://schemas.microsoft.com/office/word/2010/wordml",
    ),
    (
        "w15",
        "http://schemas.microsoft.com/office/word/2012/wordml",
    ),
    (
        "w16",
        "http://schemas.microsoft.com/office/word/2018/wordml",
    ),
    (
        "w16cex",
        "http://schemas.microsoft.com/office/word/2018/wordml/cex",
    ),
    (
        "w16cid",
        "http://schemas.microsoft.com/office/word/2016/wordml/cid",
    ),
    (
        "w16se",
        "http://schemas.microsoft.com/office/word/2015/wordml/symex",
    ),
    (
        "w16sdtdh",
        "http://schemas.microsoft.com/office/word/2020/wordml/sdtdatahash",
    ),
    (
        "wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ),
    (
        "wp14",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
    ),
    (
        "wps",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    ),
    (
        "wpg",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    ),
    (
        "wpc",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
    ),
    (
        "wpi",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
    ),
    (
        "wne",
        "http://schemas.microsoft.com/office/word/2006/wordml",
    ),
    // DrawingML
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    (
        "a14",
        "http://schemas.microsoft.com/office/drawing/2010/main",
    ),
    (
        "a16",
        "http://schemas.microsoft.com/office/drawing/2014/main",
    ),
    (
        "pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ),
    (
        "c",
        "http://schemas.openxmlformats.org/drawingml/2006/chart",
    ),
    (
        "c14",
        "http://schemas.microsoft.com/office/drawing/2007/8/2/chart",
    ),
    (
        "c16",
        "http://schemas.microsoft.com/office/drawing/2014/chart",
    ),
    (
        "c16r2",
        "http://schemas.microsoft.com/office/drawing/2015/06/chart",
    ),
    (
        "cx",
        "http://schemas.microsoft.com/office/drawing/2014/chartex",
    ),
    (
        "dgm",
        "http://schemas.openxmlformats.org/drawingml/2006/diagram",
    ),
    (
        "xdr",
        "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing",
    ),
    (
        "cdr",
        "http://schemas.openxmlformats.org/drawingml/2006/chartDrawing",
    ),
    (
        "lc",
        "http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas",
    ),
    // PresentationML
    (
        "p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ),
    (
        "p14",
        "http://schemas.microsoft.com/office/powerpoint/2010/main",
    ),
    (
        "p15",
        "http://schemas.microsoft.com/office/powerpoint/2012/main",
    ),
    (
        "p188",
        "http://schemas.microsoft.com/office/powerpoint/2018/8/main",
    ),
    // SpreadsheetML (Office writes the main namespace as the default namespace)
    (
        "x",
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    ),
    (
        "x14",
        "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main",
    ),
    (
        "x14ac",
        "http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac",
    ),
    (
        "x15",
        "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main",
    ),
    (
        "x15ac",
        "http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac",
    ),
    (
        "x16r2",
        "http://schemas.microsoft.com/office/spreadsheetml/2015/02/main",
    ),
    (
        "xr",
        "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
    ),
    (
        "xr2",
        "http://schemas.microsoft.com/office/spreadsheetml/2015/revision2",
    ),
    (
        "xr3",
        "http://schemas.microsoft.com/office/spreadsheetml/2016/revision3",
    ),
    (
        "xr6",
        "http://schemas.microsoft.com/office/spreadsheetml/2016/revision6",
    ),
    (
        "xr10",
        "http://schemas.microsoft.com/office/spreadsheetml/2016/revision10",
    ),
    ("xm", "http://schemas.microsoft.com/office/excel/2006/main"),
    // Shared
    (
        "r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
    (
        "m",
        "http://schemas.openxmlformats.org/officeDocument/2006/math",
    ),
    (
        "mc",
        "http://schemas.openxmlformats.org/markup-compatibility/2006",
    ),
    (
        "vt",
        "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes",
    ),
    (
        "sl",
        "http://schemas.openxmlformats.org/schemaLibrary/2006/main",
    ),
    // Document properties
    (
        "cp",
        "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
    ),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dcmitype", "http://purl.org/dc/dcmitype/"),
    ("xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    (
        "ep",
        "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
    ),
    (
        "op",
        "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties",
    ),
    // Packaging
    (
        "ct",
        "http://schemas.openxmlformats.org/package/2006/content-types",
    ),
    (
        "rel",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ),
    // VML (legacy)
    ("v", "urn:schemas-microsoft-com:vml"),
    ("o", "urn:schemas-microsoft-com:office:office"),
];

/// Namespace URIs by their stable prefix, for code that builds trees.
pub mod ns {
    #![allow(missing_docs)]
    pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    pub const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
    pub const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
    pub const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
    pub const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
    pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
    pub const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
    pub const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
    pub const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
    pub const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    pub const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
    pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    pub const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
    pub const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
    pub const VT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
    pub const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
    pub const DC: &str = "http://purl.org/dc/elements/1.1/";
    pub const DCTERMS: &str = "http://purl.org/dc/terms/";
    pub const DCMITYPE: &str = "http://purl.org/dc/dcmitype/";
    pub const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
    pub const EP: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
    pub const OP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
    pub const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
    pub const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
    pub const XML: &str = "http://www.w3.org/XML/1998/namespace";
}

/// Namespaces that Office serialises as the default namespace; their elements are unprefixed in
/// Kiln's trees.
pub const DEFAULT_NAMESPACE_URIS: &[&str] = &[
    ns::X,
    ns::CT,
    ns::REL,
    ns::EP,
    ns::OP,
    "http://purl.oclc.org/ooxml/spreadsheetml/main",
];

/// ISO/IEC 29500 Strict namespaces, mapped to the transitional prefix readers query by.
const STRICT_URIS: &[(&str, &str)] = &[
    ("http://purl.oclc.org/ooxml/wordprocessingml/main", "w"),
    ("http://purl.oclc.org/ooxml/drawingml/main", "a"),
    ("http://purl.oclc.org/ooxml/drawingml/picture", "pic"),
    ("http://purl.oclc.org/ooxml/drawingml/chart", "c"),
    ("http://purl.oclc.org/ooxml/drawingml/diagram", "dgm"),
    (
        "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing",
        "xdr",
    ),
    (
        "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
        "wp",
    ),
    ("http://purl.oclc.org/ooxml/presentationml/main", "p"),
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships",
        "r",
    ),
    ("http://purl.oclc.org/ooxml/officeDocument/math", "m"),
    (
        "http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes",
        "vt",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/extendedProperties",
        "",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/customProperties",
        "",
    ),
];

/// The canonical prefix Office uses for a namespace URI (`""` = the default namespace), if known.
#[must_use]
pub fn canonical_prefix(uri: &str) -> Option<&'static str> {
    if DEFAULT_NAMESPACE_URIS.contains(&uri) {
        return Some("");
    }
    if let Some((_, p)) = STRICT_URIS.iter().find(|(u, _)| *u == uri) {
        return Some(p);
    }
    // VML drawings in workbooks bind `x` to the legacy Excel namespace.
    if uri == "urn:schemas-microsoft-com:office:excel" {
        return Some("x");
    }
    NAMESPACES.iter().find(|(_, u)| *u == uri).map(|(p, _)| *p)
}

/// The namespace URI of a prefix in the stable table, used when a builder emits an element with a
/// prefix that no ancestor declares. `x` resolves to SpreadsheetML main.
#[must_use]
pub fn namespace_for_prefix(prefix: &str) -> Option<&'static str> {
    NAMESPACES
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, u)| *u)
}

/// Rank of a prefix in the stable table, so automatically added declarations come out in a fixed
/// order. Unknown prefixes rank last.
#[must_use]
pub fn prefix_rank(prefix: &str) -> usize {
    NAMESPACES
        .iter()
        .position(|(p, _)| *p == prefix)
        .unwrap_or(NAMESPACES.len())
}

/// Splits `w:pPr` into `("w", "pPr")` and `sheetData` into `("", "sheetData")`.
#[must_use]
pub fn split_qname(name: &str) -> (&str, &str) {
    match name.find(':') {
        Some(i) => (&name[..i], &name[i + 1..]),
        None => ("", name),
    }
}

/// The local part of a qualified name.
#[must_use]
pub fn local_name(name: &str) -> &str {
    split_qname(name).1
}

/// Attributes whose *value* is a whitespace-separated list of namespace prefixes (Markup
/// Compatibility).
pub const PREFIX_LIST_ATTRIBUTES: &[&str] = &[
    "mc:Ignorable",
    "mc:MustUnderstand",
    "mc:ProcessContent",
    "mc:PreserveElements",
    "mc:PreserveAttributes",
    "Requires",
];

/// True for an attribute whose value lists namespace prefixes.
#[must_use]
pub fn is_prefix_list_attribute(name: &str) -> bool {
    PREFIX_LIST_ATTRIBUTES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_unique_prefixes() {
        for (i, (p, _)) in NAMESPACES.iter().enumerate() {
            assert!(
                NAMESPACES[i + 1..].iter().all(|(q, _)| q != p),
                "duplicate prefix {p}"
            );
        }
        assert_eq!(namespace_for_prefix("w"), Some(ns::W));
        assert_eq!(canonical_prefix(ns::X), Some(""));
        assert_eq!(canonical_prefix(ns::A), Some("a"));
        assert_eq!(prefix_rank("w"), 0);
        assert_eq!(prefix_rank("zz"), NAMESPACES.len());
    }
}
