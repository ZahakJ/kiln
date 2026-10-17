// SPDX-License-Identifier: AGPL-3.0-or-later
//! The kinds of PresentationML package and the content types and URIs the reader dispatches on.

/// Which kind of presentation package the main part's content type names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PptxKind {
    /// `.pptx`.
    Presentation,
    /// `.pptm` (macros preserved, never run).
    MacroEnabledPresentation,
    /// `.potx`.
    Template,
    /// `.potm`.
    MacroEnabledTemplate,
    /// `.ppsx` (opens as a slide show in PowerPoint).
    Show,
    /// `.ppsm`.
    MacroEnabledShow,
}

impl PptxKind {
    /// Every kind.
    pub const ALL: [PptxKind; 6] = [
        Self::Presentation,
        Self::MacroEnabledPresentation,
        Self::Template,
        Self::MacroEnabledTemplate,
        Self::Show,
        Self::MacroEnabledShow,
    ];

    /// The main part's content type.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Presentation => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
            }
            Self::MacroEnabledPresentation => {
                "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"
            }
            Self::Template => {
                "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
            }
            Self::MacroEnabledTemplate => {
                "application/vnd.ms-powerpoint.template.macroEnabled.main+xml"
            }
            Self::Show => {
                "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"
            }
            Self::MacroEnabledShow => {
                "application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml"
            }
        }
    }

    /// The kind for a main part content type, if it is a presentation one.
    #[must_use]
    pub fn from_content_type(ct: &str) -> Option<Self> {
        let ct = ct.trim();
        Self::ALL.into_iter().find(|k| k.content_type() == ct)
    }

    /// The usual file extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Presentation => "pptx",
            Self::MacroEnabledPresentation => "pptm",
            Self::Template => "potx",
            Self::MacroEnabledTemplate => "potm",
            Self::Show => "ppsx",
            Self::MacroEnabledShow => "ppsm",
        }
    }
}

/// Relationship types of PresentationML parts (transitional; strict ones are mapped by
/// `kiln_ooxml::opc::rel_type_matches`).
pub mod rel {
    macro_rules! od {
        ($s:literal) => {
            concat!(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/",
                $s
            )
        };
    }
    /// Slide.
    pub const SLIDE: &str = od!("slide");
    /// Slide layout.
    pub const SLIDE_LAYOUT: &str = od!("slideLayout");
    /// Slide master.
    pub const SLIDE_MASTER: &str = od!("slideMaster");
    /// Notes slide.
    pub const NOTES_SLIDE: &str = od!("notesSlide");
    /// Notes master.
    pub const NOTES_MASTER: &str = od!("notesMaster");
    /// Handout master.
    pub const HANDOUT_MASTER: &str = od!("handoutMaster");
    /// Legacy comments of a slide.
    pub const COMMENTS: &str = od!("comments");
    /// Legacy comment authors of the presentation.
    pub const COMMENT_AUTHORS: &str = od!("commentAuthors");
    /// Modern (threaded) comments of a slide.
    pub const MODERN_COMMENTS: &str =
        "http://schemas.microsoft.com/office/2018/10/relationships/comments";
    /// Modern comment authors of the presentation.
    pub const AUTHORS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";
    /// A slide jump in a hyperlink.
    pub const HYPERLINK: &str = od!("hyperlink");
    /// Image.
    pub const IMAGE: &str = od!("image");
    /// Chart.
    pub const CHART: &str = od!("chart");
    /// ChartEx (`cx:chart`).
    pub const CHART_EX: &str = "http://schemas.microsoft.com/office/2014/relationships/chartEx";
    /// Video (linked or embedded, the poster's sibling).
    pub const VIDEO: &str = od!("video");
    /// Audio.
    pub const AUDIO: &str = od!("audio");
    /// The embedded media part of a video or audio (`p14:media`).
    pub const MEDIA: &str = "http://schemas.microsoft.com/office/2007/relationships/media";
    /// SmartArt drawing (the pre-rendered shapes).
    pub const DIAGRAM_DRAWING: &str =
        "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
    /// SmartArt data.
    pub const DIAGRAM_DATA: &str = od!("diagramData");
}

/// `a:graphicData/@uri` values.
pub mod uri {
    /// A table.
    pub const TABLE: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";
    /// A chart.
    pub const CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
    /// A ChartEx chart.
    pub const CHART_EX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
    /// SmartArt.
    pub const DIAGRAM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
    /// An OLE object.
    pub const OLE: &str = "http://schemas.openxmlformats.org/presentationml/2006/ole";
    /// The sections extension of `p:presentation/p:extLst`.
    pub const SECTIONS_EXT: &str = "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";
    /// The embedded media extension of `p:nvPr/p:extLst` (`p14:media`).
    pub const MEDIA_EXT: &str = "{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}";
    /// The modern comments extension of `p:sld/p:extLst` (`p188:commentRel`).
    pub const MODERN_COMMENTS_EXT: &str = "{6950BFC3-D8DA-4A85-94F7-54DA5524770B}";
}

/// `mc:Choice/@Requires` prefixes the reader understands. A choice that needs anything else
/// falls back (VML OLE objects, ink, 3-D models).
pub const SUPPORTED_MC: [&str; 20] = [
    "a14", "a16", "p14", "p15", "p159", "p188", "c14", "c16", "c16r2", "cx", "cx1", "cx2", "cx3",
    "cx4", "cx5", "cx6", "cx7", "cx8", "dsp", "m",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_their_content_types() {
        for k in PptxKind::ALL {
            assert_eq!(PptxKind::from_content_type(k.content_type()), Some(k));
            assert!(!k.extension().is_empty());
        }
        assert_eq!(
            PptxKind::from_content_type(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            ),
            None
        );
    }
}
