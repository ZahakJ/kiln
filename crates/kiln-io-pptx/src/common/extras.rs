// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a faithful writer needs that the presentation model does not hold: where each slide,
//! layout and master came from, the ids the file used, and the original XML of everything the
//! reader approximated or did not model.
//!
//! Every XML string here is a self-contained fragment (see [`crate::common::xml::fragment`]).
//! Relationship ids inside fragments are the source part's original ids; the passthrough set
//! keeps those relationships (their targets are passthrough parts), and its `emit` reports how
//! they were renumbered.

use std::collections::BTreeMap;

use kiln_common::NodeId;

/// Why a shape's original XML is kept: the model shows an approximation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeSource {
    /// SmartArt (`a:graphicData` with `dgm:relIds`): the model has the group of shapes of its
    /// drawing part, or an empty picture when the file has none.
    SmartArt {
        /// The original `p:graphicFrame`.
        xml: String,
    },
    /// An OLE object (`p:oleObj`): the model has its preview picture.
    Ole {
        /// The original `p:graphicFrame` (or the `mc:AlternateContent` around it).
        xml: String,
    },
    /// A shape read from the chosen branch of an `mc:AlternateContent` (newer content with a
    /// fallback for older readers).
    AlternateContent {
        /// The original `mc:AlternateContent`.
        xml: String,
    },
}

impl ShapeSource {
    /// The kept XML.
    #[must_use]
    pub fn xml(&self) -> &str {
        match self {
            Self::SmartArt { xml } | Self::Ole { xml } | Self::AlternateContent { xml } => xml,
        }
    }
}

/// `a:rPr` children of a run that the text model has no field for (text outline and shadow,
/// underline line and fill, the symbol font).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptRun {
    /// First char of the run in its paragraph.
    pub start: usize,
    /// One past the last char.
    pub end: usize,
    /// The elements' XML, in file order.
    pub xml: Vec<String>,
}

/// The [`KeptRun`]s of a paragraph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptRuns {
    /// The paragraph's text when it was read, for placing the runs after an edit.
    pub text: Vec<char>,
    /// The runs.
    pub runs: Vec<KeptRun>,
    /// The same elements of the paragraph's `a:endParaRPr`.
    pub mark: Vec<String>,
}

impl KeptRuns {
    /// The runs in a paragraph that now reads `now`: an edit is the part between the text the two
    /// share at the start and at the end, runs after it move with their text, and a run the edit
    /// falls in keeps the text typed there. A paragraph replaced whole has none left.
    #[must_use]
    pub fn placed(&self, now: &[char]) -> Vec<KeptRun> {
        if self.text == now {
            return self.runs.clone();
        }
        let (old, new) = (self.text.len(), now.len());
        let same = |a: &char, b: &char| a == b;
        let head = self
            .text
            .iter()
            .zip(now)
            .take_while(|(a, b)| same(a, b))
            .count();
        let tail = self
            .text
            .iter()
            .rev()
            .zip(now.iter().rev())
            .take(old.min(new) - head)
            .take_while(|(a, b)| same(a, b))
            .count();
        if head == 0 && tail == 0 {
            return Vec::new();
        }
        let place = |at: usize, end: bool| {
            if at + tail >= old && (end || at + tail > old || at == 0 || at > head) {
                at + new - old
            } else if at <= head {
                at
            } else {
                new - tail
            }
        };
        self.runs
            .iter()
            .map(|r| KeptRun {
                start: place(r.start, false),
                end: place(r.end, true),
                xml: r.xml.clone(),
            })
            .filter(|r| r.start < r.end)
            .collect()
    }
}

/// A `p:spTree` child the model has no shape for (ink, an unknown graphic, groups nested too
/// deep), kept with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueShape {
    /// The shape list it was in: a slide, layout, master, notes master or group id.
    pub parent: NodeId,
    /// How many modelled shapes come before it in that list.
    pub index: usize,
    /// The original XML.
    pub xml: String,
}

/// The monikers a modern comment names the object it is on with (`ac:spMk` for a shape,
/// `ac:picMk`, `ac:graphicFrameMk`, `ac:grpSpMk`, `ac:cxnSpMk`).
pub const MONIKERS: &[&str] = &["spMk", "picMk", "graphicFrameMk", "grpSpMk", "cxnSpMk"];

/// What a modern comment on an object or a range of its text was anchored with
/// (`ac:deMkLst` or `ac:txMkLst`), kept so a save writes it back while the comment is on the
/// same object and the object's text is as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentAnchor {
    /// The original moniker list.
    pub xml: String,
    /// The object it names.
    pub shape: NodeId,
    /// The object's text as read ([`anchor_text`]).
    pub text: String,
}

/// The text a comment's anchor is checked against: every text body of a shape (table cells too).
#[must_use]
pub fn anchor_text(shape: &kiln_deck::Shape) -> String {
    let mut out = String::new();
    shape.walk_text(&mut |t| {
        out.push_str(&t.text());
        out.push('\n');
    });
    out
}

/// The shape `id` in a shape list or a group in it.
#[must_use]
pub fn find_shape(shapes: &[kiln_deck::Shape], id: NodeId) -> Option<&kiln_deck::Shape> {
    shapes.iter().find_map(|s| {
        if s.id == id {
            Some(s)
        } else {
            s.children().and_then(|c| find_shape(c, id))
        }
    })
}

/// Where a presentation came from, and what the model does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PptxExtras {
    /// The part each slide, layout, master and the notes master was read from.
    pub part_names: BTreeMap<NodeId, String>,
    /// The notes slide part of each slide that has one.
    pub notes_parts: BTreeMap<NodeId, String>,
    /// The id a slide's notes page is filed under in [`Self::owner_xml`] and
    /// [`Self::opaque_shapes`] (notes pages have no id in the model).
    pub notes_keys: BTreeMap<NodeId, NodeId>,
    /// The theme part of each master.
    pub theme_parts: BTreeMap<NodeId, String>,
    /// `p:sldId/@id`, `p:sldMasterId/@id` and `p:sldLayoutId/@id` by model id.
    pub list_ids: BTreeMap<NodeId, u32>,
    /// `p:cNvPr/@id` of every shape as read, by model id.
    pub shape_ids: BTreeMap<NodeId, u32>,
    /// `p14:section/@id` (a GUID) by section id.
    pub section_guids: BTreeMap<NodeId, String>,
    /// Shapes the model approximates, with the original XML.
    pub shape_sources: BTreeMap<NodeId, ShapeSource>,
    /// `p:spTree` children without a model shape.
    pub opaque_shapes: Vec<OpaqueShape>,
    /// Shape XML the model does not hold (3-D scene and bevel, extension lists, custom data),
    /// by shape id.
    pub shape_xml: BTreeMap<NodeId, Vec<String>>,
    /// The original `p:transition` (or the `mc:AlternateContent` around it) of each slide that
    /// has one.
    pub transitions: BTreeMap<NodeId, String>,
    /// The original `p:timing` of each slide that has one.
    pub timing: BTreeMap<NodeId, String>,
    /// Children of a slide, layout, master or notes master root (and of its `p:cSld`) the model
    /// does not hold (`p:extLst`, `p:custDataLst`, `p:controls`, `p:hf` of layouts), by owner.
    pub owner_xml: BTreeMap<NodeId, Vec<String>>,
    /// Children of `p:presentation` the model does not hold (`p:custShowLst`, `p:photoAlbum`,
    /// `p:kinsoku`, `p:modifyVerifier`, `p:embeddedFontLst`, `p:custDataLst`, `p:extLst` without
    /// the sections, which the model holds).
    pub presentation_xml: Vec<String>,
    /// Attributes of `p:presentation` other than `firstSlideNum` (`saveSubsetFonts`,
    /// `autoCompressPictures`, `rtl`, …) as written.
    pub presentation_attrs: Vec<(String, String)>,
    /// Whether the comments came from modern (threaded, `p188:`) comment parts.
    pub modern_comments: bool,
    /// The anchors of modern comments on objects, by comment id.
    pub comment_anchors: BTreeMap<NodeId, CommentAnchor>,
    /// Unmodelled run properties, by shape, table cell (counted from 1, 0 for a shape's own
    /// text) and paragraph.
    pub kept_runs: BTreeMap<(NodeId, u32, NodeId), KeptRuns>,
}

impl PptxExtras {
    /// The original XML of an approximated shape.
    #[must_use]
    pub fn source_of(&self, shape: NodeId) -> Option<&ShapeSource> {
        self.shape_sources.get(&shape)
    }

    /// True when the shape is SmartArt shown as shapes or a placeholder picture.
    #[must_use]
    pub fn is_smart_art(&self, shape: NodeId) -> bool {
        matches!(self.source_of(shape), Some(ShapeSource::SmartArt { .. }))
    }

    /// True when the shape is an OLE object shown as its picture.
    #[must_use]
    pub fn is_ole(&self, shape: NodeId) -> bool {
        matches!(self.source_of(shape), Some(ShapeSource::Ole { .. }))
    }

    /// Records unmodelled XML of a shape.
    pub fn push_shape_xml(&mut self, shape: NodeId, xml: String) {
        if !xml.is_empty() {
            self.shape_xml.entry(shape).or_default().push(xml);
        }
    }

    /// Records unmodelled XML of an owner.
    pub fn push_owner_xml(&mut self, owner: NodeId, xml: String) {
        if !xml.is_empty() {
            self.owner_xml.entry(owner).or_default().push(xml);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_classified() {
        let mut x = PptxExtras::default();
        x.shape_sources.insert(
            NodeId(3),
            ShapeSource::SmartArt {
                xml: "<p:graphicFrame/>".into(),
            },
        );
        x.shape_sources
            .insert(NodeId(4), ShapeSource::Ole { xml: "<o/>".into() });
        x.shape_sources.insert(
            NodeId(5),
            ShapeSource::AlternateContent { xml: "<m/>".into() },
        );
        assert!(x.is_smart_art(NodeId(3)) && !x.is_smart_art(NodeId(4)));
        assert!(x.is_ole(NodeId(4)) && !x.is_ole(NodeId(5)));
        assert_eq!(x.source_of(NodeId(5)).unwrap().xml(), "<m/>");
        x.push_shape_xml(NodeId(3), String::new());
        x.push_shape_xml(NodeId(3), "<a:sp3d/>".into());
        x.push_owner_xml(NodeId(1), "<p:extLst/>".into());
        x.push_owner_xml(NodeId(1), String::new());
        assert_eq!(x.shape_xml[&NodeId(3)].len(), 1);
        assert_eq!(x.owner_xml[&NodeId(1)].len(), 1);
    }
}

#[cfg(test)]
mod kept_tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn runs(text: &str, spans: &[(usize, usize)]) -> KeptRuns {
        KeptRuns {
            text: chars(text),
            runs: spans
                .iter()
                .map(|&(start, end)| KeptRun {
                    start,
                    end,
                    xml: vec![String::new()],
                })
                .collect(),
            mark: Vec::new(),
        }
    }

    fn spans(k: &KeptRuns, now: &str) -> Vec<(usize, usize)> {
        k.placed(&chars(now))
            .iter()
            .map(|r| (r.start, r.end))
            .collect()
    }

    #[test]
    fn runs_follow_their_text_through_an_edit() {
        let k = runs("Hello world", &[(0, 5), (6, 11)]);
        assert_eq!(spans(&k, "Hello world"), [(0, 5), (6, 11)]);
        // Typed at the end, in the last run, and before a run, which moves.
        assert_eq!(spans(&k, "Hello world!"), [(0, 5), (6, 12)]);
        assert_eq!(spans(&k, "Oh Hello world"), [(3, 8), (9, 14)]);
        // Typed inside the first run.
        assert_eq!(spans(&k, "Heyllo world"), [(0, 6), (7, 12)]);
        // Deleted from the second.
        assert_eq!(spans(&k, "Hello wod"), [(0, 5), (6, 9)]);
        // Nothing in common: nothing to go by.
        assert!(spans(&k, "xyz").is_empty());
    }
}
