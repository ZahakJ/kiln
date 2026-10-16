// SPDX-License-Identifier: AGPL-3.0-or-later
//! Relationship types (transitional URIs). [`rel_type_matches`] also accepts their ISO/IEC 29500
//! Strict equivalents.

macro_rules! od {
    ($tail:literal) => {
        concat!(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/",
            $tail
        )
    };
}
macro_rules! pkg {
    ($tail:literal) => {
        concat!(
            "http://schemas.openxmlformats.org/package/2006/relationships/",
            $tail
        )
    };
}
macro_rules! ms {
    ($tail:literal) => {
        concat!("http://schemas.microsoft.com/office/", $tail)
    };
}

/// Relationship type URIs by name.
pub mod rel_type {
    /// `officeDocument`
    pub const OFFICE_DOCUMENT: &str = od!("officeDocument");
    /// `metadata/core-properties`
    pub const CORE_PROPERTIES: &str = pkg!("metadata/core-properties");
    /// `extended-properties`
    pub const EXTENDED_PROPERTIES: &str = od!("extended-properties");
    /// `custom-properties`
    pub const CUSTOM_PROPERTIES: &str = od!("custom-properties");
    /// `metadata/thumbnail`
    pub const THUMBNAIL: &str = pkg!("metadata/thumbnail");
    /// `digital-signature/origin`
    pub const DIGITAL_SIGNATURE_ORIGIN: &str = pkg!("digital-signature/origin");
    /// `customXml`
    pub const CUSTOM_XML: &str = od!("customXml");
    /// `customXmlProps`
    pub const CUSTOM_XML_PROPS: &str = od!("customXmlProps");
    /// `theme`
    pub const THEME: &str = od!("theme");
    /// `styles`
    pub const STYLES: &str = od!("styles");
    /// `2007/relationships/stylesWithEffects`
    pub const STYLES_WITH_EFFECTS: &str = ms!("2007/relationships/stylesWithEffects");
    /// `numbering`
    pub const NUMBERING: &str = od!("numbering");
    /// `settings`
    pub const SETTINGS: &str = od!("settings");
    /// `webSettings`
    pub const WEB_SETTINGS: &str = od!("webSettings");
    /// `fontTable`
    pub const FONT_TABLE: &str = od!("fontTable");
    /// `font`
    pub const FONT: &str = od!("font");
    /// `header`
    pub const HEADER: &str = od!("header");
    /// `footer`
    pub const FOOTER: &str = od!("footer");
    /// `footnotes`
    pub const FOOTNOTES: &str = od!("footnotes");
    /// `endnotes`
    pub const ENDNOTES: &str = od!("endnotes");
    /// `comments`
    pub const COMMENTS: &str = od!("comments");
    /// `2011/relationships/commentsExtended`
    pub const COMMENTS_EXTENDED: &str = ms!("2011/relationships/commentsExtended");
    /// `2016/09/relationships/commentsIds`
    pub const COMMENTS_IDS: &str = ms!("2016/09/relationships/commentsIds");
    /// `2018/08/relationships/commentsExtensible`
    pub const COMMENTS_EXTENSIBLE: &str = ms!("2018/08/relationships/commentsExtensible");
    /// `2011/relationships/people`
    pub const PEOPLE: &str = ms!("2011/relationships/people");
    /// `glossaryDocument`
    pub const GLOSSARY_DOCUMENT: &str = od!("glossaryDocument");
    /// `image`
    pub const IMAGE: &str = od!("image");
    /// `hyperlink`
    pub const HYPERLINK: &str = od!("hyperlink");
    /// `oleObject`
    pub const OLE_OBJECT: &str = od!("oleObject");
    /// `package`
    pub const PACKAGE: &str = od!("package");
    /// `chart`
    pub const CHART: &str = od!("chart");
    /// `chartUserShapes`
    pub const CHART_USER_SHAPES: &str = od!("chartUserShapes");
    /// `2011/relationships/chartStyle`
    pub const CHART_STYLE: &str = ms!("2011/relationships/chartStyle");
    /// `2011/relationships/chartColorStyle`
    pub const CHART_COLOR_STYLE: &str = ms!("2011/relationships/chartColorStyle");
    /// `diagramData`
    pub const DIAGRAM_DATA: &str = od!("diagramData");
    /// `diagramLayout`
    pub const DIAGRAM_LAYOUT: &str = od!("diagramLayout");
    /// `diagramQuickStyle`
    pub const DIAGRAM_QUICK_STYLE: &str = od!("diagramQuickStyle");
    /// `diagramColors`
    pub const DIAGRAM_COLORS: &str = od!("diagramColors");
    /// `2007/relationships/diagramDrawing`
    pub const DIAGRAM_DRAWING: &str = ms!("2007/relationships/diagramDrawing");
    /// `2006/relationships/vbaProject`
    pub const VBA_PROJECT: &str = ms!("2006/relationships/vbaProject");
    /// `printerSettings`
    pub const PRINTER_SETTINGS: &str = od!("printerSettings");
    /// `video`
    pub const VIDEO: &str = od!("video");
    /// `audio`
    pub const AUDIO: &str = od!("audio");
    /// `2007/relationships/media`
    pub const MEDIA: &str = ms!("2007/relationships/media");
    /// `worksheet`
    pub const WORKSHEET: &str = od!("worksheet");
    /// `chartsheet`
    pub const CHARTSHEET: &str = od!("chartsheet");
    /// `dialogsheet`
    pub const DIALOGSHEET: &str = od!("dialogsheet");
    /// `sharedStrings`
    pub const SHARED_STRINGS: &str = od!("sharedStrings");
    /// `calcChain`
    pub const CALC_CHAIN: &str = od!("calcChain");
    /// `drawing`
    pub const DRAWING: &str = od!("drawing");
    /// `vmlDrawing`
    pub const VML_DRAWING: &str = od!("vmlDrawing");
    /// `table`
    pub const TABLE: &str = od!("table");
    /// `pivotTable`
    pub const PIVOT_TABLE: &str = od!("pivotTable");
    /// `pivotCacheDefinition`
    pub const PIVOT_CACHE_DEFINITION: &str = od!("pivotCacheDefinition");
    /// `pivotCacheRecords`
    pub const PIVOT_CACHE_RECORDS: &str = od!("pivotCacheRecords");
    /// `externalLink`
    pub const EXTERNAL_LINK: &str = od!("externalLink");
    /// `queryTable`
    pub const QUERY_TABLE: &str = od!("queryTable");
    /// `connections`
    pub const CONNECTIONS: &str = od!("connections");
    /// `sheetMetadata`
    pub const SHEET_METADATA: &str = od!("sheetMetadata");
    /// `2017/10/relationships/threadedComment`
    pub const THREADED_COMMENT: &str = ms!("2017/10/relationships/threadedComment");
    /// `2017/10/relationships/person`
    pub const PERSON: &str = ms!("2017/10/relationships/person");
    /// `slide`
    pub const SLIDE: &str = od!("slide");
    /// `slideLayout`
    pub const SLIDE_LAYOUT: &str = od!("slideLayout");
    /// `slideMaster`
    pub const SLIDE_MASTER: &str = od!("slideMaster");
    /// `notesSlide`
    pub const NOTES_SLIDE: &str = od!("notesSlide");
    /// `notesMaster`
    pub const NOTES_MASTER: &str = od!("notesMaster");
    /// `handoutMaster`
    pub const HANDOUT_MASTER: &str = od!("handoutMaster");
    /// `presProps`
    pub const PRES_PROPS: &str = od!("presProps");
    /// `viewProps`
    pub const VIEW_PROPS: &str = od!("viewProps");
    /// `tableStyles`
    pub const TABLE_STYLES: &str = od!("tableStyles");
    /// `commentAuthors`
    pub const COMMENT_AUTHORS: &str = od!("commentAuthors");
    /// `comments`
    pub const SLIDE_COMMENTS: &str = od!("comments");
    /// `2018/10/relationships/comments`
    pub const MODERN_COMMENT: &str = ms!("2018/10/relationships/comments");
    /// `2018/10/relationships/authors`
    pub const AUTHORS: &str = ms!("2018/10/relationships/authors");
    /// `tags`
    pub const TAGS: &str = od!("tags");
}

const OD: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_OD: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

/// Maps a Strict relationship type URI to its transitional equivalent; other URIs are returned as
/// they are.
#[must_use]
pub fn to_transitional_rel_type(rel_type: &str) -> std::borrow::Cow<'_, str> {
    let Some(tail) = rel_type
        .strip_prefix(STRICT_OD)
        .and_then(|t| t.strip_prefix('/'))
    else {
        return std::borrow::Cow::Borrowed(rel_type);
    };
    let mapped = match tail {
        "metadata/core-properties" => return std::borrow::Cow::Borrowed(rel_type::CORE_PROPERTIES),
        "extendedProperties" => "extended-properties",
        "customProperties" => "custom-properties",
        other => other,
    };
    std::borrow::Cow::Owned(format!("{OD}/{mapped}"))
}

/// True when `actual` is `expected` or its Strict equivalent.
#[must_use]
pub fn rel_type_matches(actual: &str, expected: &str) -> bool {
    actual == expected || to_transitional_rel_type(actual) == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_types_match_their_transitional_twins() {
        assert!(rel_type_matches(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument",
            rel_type::OFFICE_DOCUMENT
        ));
        assert!(rel_type_matches(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties",
            rel_type::EXTENDED_PROPERTIES
        ));
        assert!(rel_type_matches(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/metadata/core-properties",
            rel_type::CORE_PROPERTIES
        ));
        assert!(!rel_type_matches(rel_type::STYLES, rel_type::THEME));
        assert_eq!(
            rel_type::THREADED_COMMENT,
            "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment"
        );
    }
}
