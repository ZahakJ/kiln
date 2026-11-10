// SPDX-License-Identifier: AGPL-3.0-or-later
//! SmartArt: shown as the group of shapes PowerPoint pre-rendered into the diagram's drawing
//! part (`ppt/diagrams/drawingN.xml`, `dsp:` shapes), or as an empty picture when the file has
//! no drawing. The diagram parts stay in the passthrough and the original graphic frame in the
//! extras, so an unedited SmartArt saves as SmartArt.

use kiln_deck::geom::{ChildSpace, Transform};
use kiln_deck::shape::{Group, Picture, Shape, ShapeKind};
use kiln_ooxml::opc::rel_type_matches;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::PartCx;
use super::shapes::NonVisual;
use crate::common::extras::ShapeSource;
use crate::common::kind::rel;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{child_local, fragment, local};

impl Reader<'_> {
    /// The drawing part of a diagram: named by the data part's `dsp:dataModelExt/@relId` (a
    /// relationship of the slide), else the slide's only diagram drawing.
    fn diagram_drawing(&mut self, cx: &PartCx, rel_ids: &XmlElement) -> Option<String> {
        let data_part = rel_ids.attr("r:dm").and_then(|id| cx.target(id));
        if let Some(dp) = data_part
            && let Ok(Some(doc)) = self.pkg.part_xml_with(&dp, super::parse_options())
        {
            let ext = doc
                .root
                .descendants("dsp:dataModelExt")
                .into_iter()
                .next()
                .cloned()
                .or_else(|| {
                    // `dsp` is not in Office's prefix table: match the local name.
                    super::motion::descendants_local(&doc.root, "dataModelExt")
                        .first()
                        .copied()
                        .cloned()
                });
            if let Some(target) = ext
                .as_ref()
                .and_then(|e| e.attr("relId"))
                .and_then(|id| cx.target(id))
            {
                return Some(target);
            }
        }
        let drawings: Vec<String> = cx
            .rels
            .all()
            .iter()
            .filter(|r| !r.is_external() && rel_type_matches(&r.rel_type, rel::DIAGRAM_DRAWING))
            .filter_map(|r| cx.rels.resolve(r))
            .collect();
        let diagrams = cx
            .rels
            .all()
            .iter()
            .filter(|r| rel_type_matches(&r.rel_type, rel::DIAGRAM_DATA))
            .count();
        (drawings.len() == 1 && diagrams == 1).then(|| drawings[0].clone())
    }

    /// Reads a SmartArt graphic frame.
    pub(crate) fn read_smart_art(
        &mut self,
        cx: &mut PartCx,
        frame: &XmlElement,
        data: &XmlElement,
        nv: &NonVisual,
        xfrm: Option<Transform>,
    ) -> Shape {
        let drawing = data
            .elements()
            .find(|e| local(e) == "relIds")
            .and_then(|ids| self.diagram_drawing(cx, ids));
        let doc = drawing.as_ref().and_then(|d| {
            self.pkg
                .part_xml_with(d, super::parse_options())
                .ok()
                .flatten()
        });
        let tree = doc.as_ref().and_then(|d| child_local(&d.root, "spTree"));
        let mut shape = match (tree, &drawing) {
            (Some(tree), Some(part)) => {
                let mut group = self.new_shape(cx, nv, xfrm, ShapeKind::Group(Group::default()));
                let scope = doc
                    .as_ref()
                    .map(|d| d.root.attrs.clone())
                    .unwrap_or_default();
                let mut dcx = PartCx::new(self, part, &scope, cx.color);
                dcx.consume_media = false;
                let mut children = self.read_tree(&mut dcx, tree, group.id, 1);
                self.resolve_connections(&dcx, &mut children);
                let (cw, ch) = xfrm.map_or((0, 0), |x| (x.cx, x.cy));
                group.kind = ShapeKind::Group(Group {
                    // Drawing coordinates are relative to the frame's top-left corner.
                    child_space: ChildSpace {
                        x: 0,
                        y: 0,
                        cx: cw,
                        cy: ch,
                    },
                    children,
                    props: kiln_deck::ShapeProps::default(),
                });
                group
            }
            _ => self.new_shape(cx, nv, xfrm, ShapeKind::Picture(Picture::default())),
        };
        if shape.name.is_empty() {
            shape.name = "Diagram".to_owned();
        }
        self.extras.shape_sources.insert(
            shape.id,
            ShapeSource::SmartArt {
                xml: fragment(frame, &cx.scope),
            },
        );
        self.warn(
            WarningCode::SmartArt,
            Severity::Info,
            if matches!(shape.kind, ShapeKind::Group(_)) {
                "SmartArt is shown as grouped shapes and kept as SmartArt for saving"
            } else {
                "SmartArt without a drawing is shown as an empty frame and kept for saving"
            },
            &cx.part,
            format!("shape \"{}\"", nv.name),
        );
        shape
    }
}
