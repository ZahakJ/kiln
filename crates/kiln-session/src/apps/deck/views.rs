// SPDX-License-Identifier: AGPL-3.0-or-later
//! View ▸ Guides, Notes Page and the ruler: the drawing guides — PowerPoint's two in the
//! middle until the file names its own — added, dragged and dragged off, snapped to while
//! dragging, and kept where PowerPoint keeps them (`p:cSldViewPr/p:guideLst` of
//! `viewProps.xml`, patched in place on save); the notes page's geometry; and the indents the
//! ruler's markers set.

use kiln_common::{KilnError, Result};
use kiln_deck_layout::geom::px;
use kiln_ooxml::opc::Relationship;
use kiln_ooxml::passthrough::{PassthroughPart, PassthroughSet};
use kiln_ooxml::xml::{XmlDocument, XmlElement, parse_xml};
use kiln_protocol::commands::DeckRect;
use kiln_protocol::commands::deck_content::{DeckGuide, DeckNotesPage};

use super::DeckDoc;
use crate::model::Change;

/// The content type of `viewProps.xml`.
const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";
/// The relationship from the presentation to it.
const REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps";
/// Where a new presentation's part goes.
const PART: &str = "/ppt/viewProps.xml";
/// Guide positions are in eighths of a point: 6 per slide px.
const UNITS_PER_PX: f32 = 6.0;
/// The most guides a slide keeps.
const MAX_GUIDES: usize = 64;

const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The passthrough part that is `viewProps.xml`.
fn part(set: &PassthroughSet) -> Option<&PassthroughPart> {
    set.parts()
        .find(|p| p.content_type.eq_ignore_ascii_case(CONTENT_TYPE))
}

/// The guides a `viewProps.xml` holds (`None` when it names none).
fn read(bytes: &[u8]) -> Option<Vec<DeckGuide>> {
    let doc = parse_xml(bytes).ok()?;
    let list = doc
        .root
        .child("p:slideViewPr")?
        .child("p:cSldViewPr")?
        .child("p:guideLst")?;
    #[allow(clippy::cast_precision_loss)]
    let guides: Vec<DeckGuide> = list
        .children_named("p:guide")
        .map(|g| DeckGuide {
            horizontal: g.attr("orient") == Some("horz"),
            pos: g.attr_int("pos").unwrap_or(0) as f32 / UNITS_PER_PX,
        })
        .take(MAX_GUIDES)
        .collect();
    Some(guides)
}

/// PowerPoint's default guides: one across the middle each way.
fn middle(w: f32, h: f32) -> Vec<DeckGuide> {
    vec![
        DeckGuide {
            horizontal: true,
            pos: h / 2.0,
        },
        DeckGuide {
            horizontal: false,
            pos: w / 2.0,
        },
    ]
}

fn guide_list(guides: &[DeckGuide]) -> XmlElement {
    XmlElement::new("p:guideLst").with_children(guides.iter().map(|g| {
        #[allow(clippy::cast_possible_truncation)]
        let pos = (g.pos * UNITS_PER_PX).round() as i64;
        XmlElement::new("p:guide")
            .with_opt_attr("orient", g.horizontal.then_some("horz"))
            .with_attr("pos", pos.max(0))
    }))
}

/// `p:cViewPr`, which `p:cSldViewPr` must start with: the view's zoom and scroll position, as
/// PowerPoint writes them for a new presentation (fit to the window, at 100 %).
fn common_view() -> XmlElement {
    let ratio = |name: &str| {
        XmlElement::new(name)
            .with_attr("n", 100)
            .with_attr("d", 100)
    };
    XmlElement::new("p:cViewPr")
        .with_attr("varScale", "1")
        .with_child(
            XmlElement::new("p:scale")
                .with_child(ratio("a:sx"))
                .with_child(ratio("a:sy")),
        )
        .with_child(
            XmlElement::new("p:origin")
                .with_attr("x", 0)
                .with_attr("y", 0),
        )
}

/// `viewProps.xml` with the guides written in, everything else as it was.
fn patch(bytes: &[u8], guides: &[DeckGuide]) -> Option<Vec<u8>> {
    let mut doc: XmlDocument = parse_xml(bytes).ok()?;
    if doc.root.child("p:slideViewPr").is_none() {
        doc.root.upsert_child(
            XmlElement::new("p:slideViewPr").with_child(XmlElement::new("p:cSldViewPr")),
            &[
                "p:outlineViewPr",
                "p:notesTextViewPr",
                "p:sorterViewPr",
                "p:notesViewPr",
                "p:gridSpacing",
                "p:extLst",
            ],
        );
    }
    let view = doc.root.child_mut("p:slideViewPr")?;
    if view.child("p:cSldViewPr").is_none() {
        view.upsert_child(XmlElement::new("p:cSldViewPr"), &["p:extLst"]);
    }
    let c = view.child_mut("p:cSldViewPr")?;
    if c.child("p:cViewPr").is_none() {
        c.children.insert(0, common_view().into());
    }
    c.upsert_child(guide_list(guides), &["p:extLst"]);
    kiln_ooxml::xml::document_to_bytes(&doc).ok()
}

/// A new `viewProps.xml` with the guides.
fn fresh(guides: &[DeckGuide]) -> Option<Vec<u8>> {
    let root = XmlElement::new("p:viewPr")
        .with_attr("xmlns:a", NS_A)
        .with_attr("xmlns:r", NS_R)
        .with_attr("xmlns:p", NS_P)
        .with_child(
            XmlElement::new("p:slideViewPr").with_child(
                XmlElement::new("p:cSldViewPr")
                    .with_child(common_view())
                    .with_child(guide_list(guides)),
            ),
        );
    kiln_ooxml::xml::document_to_bytes(&XmlDocument::new(root)).ok()
}

/// A passthrough set with the guides written into its `viewProps.xml` (added, with its
/// relationship from the presentation, when there is none).
pub(crate) fn apply(set: &PassthroughSet, guides: &[DeckGuide], main_part: &str) -> PassthroughSet {
    let mut out = set.clone();
    if let Some(p) = part(set) {
        if let Some(bytes) = patch(&p.bytes, guides) {
            out.record_part(PassthroughPart { bytes, ..p.clone() });
        }
        return out;
    }
    let Some(bytes) = fresh(guides) else {
        return out;
    };
    out.record_part(PassthroughPart {
        name: PART.to_owned(),
        content_type: CONTENT_TYPE.to_owned(),
        bytes,
        relationships: Vec::new(),
    });
    out.record_relationship(
        main_part,
        Relationship {
            id: "rIdKilnViewProps".to_owned(),
            rel_type: REL_TYPE.to_owned(),
            target: "viewProps.xml".to_owned(),
            target_mode: kiln_ooxml::st::TargetMode::Internal,
        },
    );
    out
}

/// The guides of a file (PowerPoint's middle ones when it names none), for a slide `w` × `h` px.
pub(crate) fn guides_of(set: Option<&PassthroughSet>, w: f32, h: f32) -> Vec<DeckGuide> {
    set.and_then(part)
        .and_then(|p| read(&p.bytes))
        .unwrap_or_else(|| middle(w, h))
}

impl DeckDoc {
    /// Add, move and remove a guide; the ruler's indents.
    pub(crate) fn view_command(
        &mut self,
        cmd: &kiln_protocol::commands::DeckCommand,
    ) -> Result<Change> {
        use kiln_protocol::commands::DeckCommand as D;
        let (w, h) = (px(self.pres.slide_size.cx), px(self.pres.slide_size.cy));
        // On the slide, and on the eighth of a point the file keeps.
        let clamp = |g: &DeckGuide, pos: f32| {
            (pos.clamp(0.0, if g.horizontal { h } else { w }) * UNITS_PER_PX).round() / UNITS_PER_PX
        };
        match cmd {
            D::AddGuide { horizontal, pos } => {
                if self.guides.len() >= MAX_GUIDES {
                    return Err(KilnError::InvalidInput(format!(
                        "a slide has at most {MAX_GUIDES} guides"
                    )));
                }
                let g = DeckGuide {
                    horizontal: *horizontal,
                    pos: 0.0,
                };
                let pos = pos.unwrap_or(if *horizontal { h / 2.0 } else { w / 2.0 });
                self.guides.push(DeckGuide {
                    pos: clamp(&g, pos),
                    ..g
                });
                // Adding a guide shows the guides, as in PowerPoint.
                self.view.guides = true;
            }
            D::MoveGuide { index, pos } => {
                let g = self
                    .guides
                    .get_mut(*index as usize)
                    .ok_or_else(|| KilnError::InvalidInput(format!("there is no guide {index}")))?;
                g.pos = clamp(g, *pos);
            }
            D::RemoveGuide { index } => {
                if (*index as usize) >= self.guides.len() {
                    return Err(KilnError::InvalidInput(format!(
                        "there is no guide {index}"
                    )));
                }
                self.guides.remove(*index as usize);
            }
            D::ParagraphIndent { left, first_line } => {
                return self.paragraph_indent(*left, *first_line);
            }
            _ => return Ok(Change::None),
        }
        self.epoch += 1;
        Ok(Change::Selection)
    }

    /// The ruler's markers: the paragraphs' left and first-line indents (points).
    fn paragraph_indent(&mut self, left: f32, first_line: f32) -> Result<Change> {
        const ID: &str = "deck.paragraph.indent";
        if !(0.0..=1584.0).contains(&left)
            || !(-1584.0..=1584.0).contains(&first_line)
            || left + first_line < 0.0
        {
            return Err(KilnError::InvalidInput(
                "an indent is 0 to 22 inches from the edge of the text".into(),
            ));
        }
        #[allow(clippy::cast_possible_truncation)]
        let twips = |pt: f32| (pt * 20.0).round() as i32;
        let indent = kiln_doc::Indent {
            left: Some(twips(left)),
            first_line: Some(twips(first_line)),
            ..kiln_doc::Indent::default()
        };
        self.format_paragraphs(ID, |p| p.indent = Some(indent))
    }

    /// The notes page's geometry (View ▸ Notes Page): page, slide picture and notes box, px.
    pub(crate) fn notes_page(&self) -> DeckNotesPage {
        let page = (px(self.pres.notes_size.cx), px(self.pres.notes_size.cy));
        let slide = (px(self.pres.slide_size.cx), px(self.pres.slide_size.cy));
        let (image, body) = kiln_deck_layout::print::notes_boxes(&self.pres, page, slide);
        let rect = |b: kiln_scene::Bounds| DeckRect {
            x: b.x,
            y: b.y,
            width: b.w,
            height: b.h,
        };
        DeckNotesPage {
            width: page.0,
            height: page.1,
            slide: rect(image),
            body: rect(body),
        }
    }

    /// The drawing guides as snapping lines (zero-width boxes across the slide), when shown.
    pub(crate) fn guide_snaps(&self) -> Vec<kiln_scene::Bounds> {
        if !self.view.guides {
            return Vec::new();
        }
        self.guides
            .iter()
            .map(|g| {
                if g.horizontal {
                    kiln_scene::Bounds::new(-10_000.0, g.pos, 20_000.0, 0.0)
                } else {
                    kiln_scene::Bounds::new(g.pos, -10_000.0, 0.0, 20_000.0)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use kiln_protocol::commands::DeckCommand as D;
    use kiln_protocol::{Command, FileFormat};

    use super::super::DeckDoc;
    use crate::model::DocModel;

    fn run(d: &mut DeckDoc, c: D) {
        d.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }

    fn open(bytes: &[u8]) -> DeckDoc {
        let read = kiln_io_pptx::read_pptx(bytes).expect("open");
        let (pres, original) = super::super::save::Original::from_read(read, bytes);
        DeckDoc::new(pres, Some(original), None)
    }

    #[test]
    fn guides_are_read_dragged_added_removed_and_saved_in_view_props() {
        let mut d = open(&super::super::tests::fixture("transitions.pptx"));
        // The file's: the middle of a 4:3 slide (2160 and 2880 eighths of a point).
        let st = d.deck_state();
        assert_eq!(st.guide_list.len(), 2);
        assert!((st.guide_list[0].pos - 360.0).abs() < 0.01 && st.guide_list[0].horizontal);
        assert!((st.guide_list[1].pos - 480.0).abs() < 0.01 && !st.guide_list[1].horizontal);
        run(
            &mut d,
            D::MoveGuide {
                index: 1,
                pos: 300.0,
            },
        );
        run(
            &mut d,
            D::AddGuide {
                horizontal: true,
                pos: Some(100.0),
            },
        );
        run(&mut d, D::RemoveGuide { index: 0 });
        assert!(d.view.guides, "adding a guide shows them");
        let again = open(&d.save(FileFormat::Pptx).expect("save"));
        let g = again.deck_state().guide_list;
        assert_eq!(g.len(), 2);
        assert!(!g[0].horizontal && (g[0].pos - 300.0).abs() < 0.2, "{g:?}");
        assert!(g[1].horizontal && (g[1].pos - 100.0).abs() < 0.2, "{g:?}");
        // The rest of viewProps.xml is kept.
        let bytes = d.save(FileFormat::Pptx).expect("save");
        let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes).expect("zip");
        let xml = String::from_utf8(
            pkg.part("/ppt/viewProps.xml")
                .ok()
                .flatten()
                .expect("part")
                .into_owned(),
        )
        .expect("utf-8");
        assert!(
            xml.contains("p:notesTextViewPr") && xml.contains("pos=\"1800\""),
            "{xml}"
        );
    }

    #[test]
    fn a_new_presentation_gets_its_guides_saved_and_shapes_snap_to_them() {
        let mut d = DeckDoc::new(kiln_deck::Presentation::new(), None, None);
        run(&mut d, D::ToggleGuides);
        run(
            &mut d,
            D::MoveGuide {
                index: 1,
                pos: 200.0,
            },
        );
        assert!(d.guide_snaps().iter().any(|b| (b.x - 200.0).abs() < 0.01));
        let again = open(&d.save(FileFormat::Pptx).expect("save"));
        assert!(
            again
                .deck_state()
                .guide_list
                .iter()
                .any(|g| !g.horizontal && (g.pos - 200.0).abs() < 0.2)
        );
        // An untouched file keeps its viewProps.xml as it was.
        let plain = DeckDoc::new(kiln_deck::Presentation::new(), None, None);
        let pkg = kiln_ooxml::opc::OpcPackage::open(&plain.save(FileFormat::Pptx).expect("save"))
            .expect("zip");
        assert!(!pkg.has("/ppt/viewProps.xml") || plain.guides == plain.guides_read);
    }

    /// The children of `p:slideViewPr/p:cSldViewPr`, in order, with `p:cViewPr`'s.
    fn common_view_children(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
        let doc = kiln_ooxml::xml::parse_xml(bytes).expect("xml");
        let c = doc
            .root
            .child_path(&["p:slideViewPr", "p:cSldViewPr"])
            .expect("p:cSldViewPr");
        let names = |e: &kiln_ooxml::xml::XmlElement| {
            e.elements().map(|x| x.name.clone()).collect::<Vec<_>>()
        };
        let view = c.child("p:cViewPr").map(names).unwrap_or_default();
        (names(c), view)
    }

    #[test]
    fn view_props_written_for_guides_have_the_view_ppt_requires() {
        let guides = super::middle(960.0, 540.0);
        let expected = (
            vec!["p:cViewPr".to_owned(), "p:guideLst".to_owned()],
            vec!["p:scale".to_owned(), "p:origin".to_owned()],
        );
        // A new viewProps.xml.
        let fresh = super::fresh(&guides).expect("fresh");
        assert_eq!(common_view_children(&fresh), expected);
        let doc = kiln_ooxml::xml::parse_xml(&fresh).expect("xml");
        let scale = doc
            .root
            .child_path(&["p:slideViewPr", "p:cSldViewPr", "p:cViewPr", "p:scale"])
            .expect("scale");
        for r in ["a:sx", "a:sy"] {
            let r = scale.child(r).expect("ratio");
            assert_eq!((r.attr("n"), r.attr("d")), (Some("100"), Some("100")));
        }
        // A file's viewProps.xml without a slide view, and with one that has no common view.
        for file in [
            concat!(
                r#"<p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
                r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">"#,
                r#"<p:normalViewPr/><p:notesTextViewPr><p:cViewPr><p:scale><a:sx n="1" d="1"/>"#,
                r#"<a:sy n="1" d="1"/></p:scale><p:origin x="0" y="0"/></p:cViewPr></p:notesTextViewPr>"#,
                r#"<p:gridSpacing cx="76200" cy="76200"/></p:viewPr>"#,
            ),
            concat!(
                r#"<p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
                r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">"#,
                r#"<p:slideViewPr><p:cSldViewPr snapToGrid="0"/></p:slideViewPr></p:viewPr>"#,
            ),
        ] {
            let patched = super::patch(file.as_bytes(), &guides).expect("patch");
            assert_eq!(common_view_children(&patched), expected, "{file}");
            let doc = kiln_ooxml::xml::parse_xml(&patched).expect("xml");
            let order: Vec<&str> = doc.root.elements().map(|e| e.name.as_str()).collect();
            let at = |n: &str| order.iter().position(|x| *x == n);
            // In the schema's place: after p:normalViewPr, before p:notesTextViewPr.
            assert!(
                at("p:normalViewPr") < at("p:slideViewPr")
                    && at("p:slideViewPr") < at("p:notesTextViewPr").or(Some(usize::MAX)),
                "{order:?}"
            );
        }
    }

    #[test]
    fn the_notes_page_and_the_rulers_indents() {
        let mut d = DeckDoc::new(kiln_deck::Presentation::new(), None, None);
        let page = d.notes_page();
        assert!(page.height > page.width, "a portrait page");
        assert!(page.slide.y < page.body.y, "the slide above the notes");
        run(&mut d, D::NewSlide { layout: None });
        let body = d.pres.slides[1].shapes[1].id;
        run(&mut d, D::SelectShapes { ids: vec![body] });
        run(&mut d, D::EditText);
        run(
            &mut d,
            D::InsertText {
                text: "Clay".into(),
            },
        );
        run(
            &mut d,
            D::ParagraphIndent {
                left: 36.0,
                first_line: -18.0,
            },
        );
        let f = d.deck_state().format.expect("format");
        assert!(
            (f.indent_left - 36.0).abs() < 0.01 && (f.indent_first + 18.0).abs() < 0.01,
            "{f:?}"
        );
        assert!(d.deck_state().text_area.is_some());
        assert!(
            d.execute(&Command::Deck(D::ParagraphIndent {
                left: 10.0,
                first_line: -20.0
            }))
            .is_err()
        );
    }
}
