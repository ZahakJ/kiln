// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the reader builds from the feature corpus, beyond the oracle's slides, shapes and
//! text: placeholder inheritance, pictures and crops, tables, charts, transitions, animations,
//! notes, sections, links, connectors, groups, backgrounds, colours, SmartArt and the
//! passthrough.

mod read_support;

use kiln_deck::animation::{AnimationEffect, EffectStart};
use kiln_deck::shape::{Geometry, ShapeKind};
use kiln_deck::slide::{Background, LayoutKind, SlideSizeKind};
use kiln_deck::transition::TransitionEffect;
use kiln_deck::{Owner, ShapePath};
use kiln_doc::{InlineObject, Mark};
use kiln_io_pptx::{PptxKind, WarningCode, body_text, read_pptx, snapshot_facts};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::opc::OpcPackage;
use read_support::{decks, fixtures, read_fixture};

#[test]
fn every_deck_of_the_corpus_reads_into_a_valid_model() {
    // The committed corpus, plus the external corpus and the large deck where they were fetched
    // or generated (they are not in a fresh checkout).
    let mut all = decks("pptx");
    assert!(all.len() >= 19, "{} decks", all.len());
    all.extend(decks("external/pptx"));
    let large = fixtures().join("large/deck-200.pptx");
    if large.exists() {
        all.push(large);
    }
    for path in all {
        let bytes = std::fs::read(&path).unwrap();
        let read = read_pptx(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        read.presentation.validate().unwrap();
        assert_eq!(read.kind, PptxKind::Presentation);
        assert_eq!(read.main_part, "/ppt/presentation.xml");
        // Every part is either modelled or carried, never both and never neither.
        let pkg = OpcPackage::open(&bytes).unwrap();
        for name in pkg.part_names() {
            let modelled = name == read.main_part
                || read.presentation.parts.contains_key(name)
                || read.extras.part_names.values().any(|p| p == name)
                || read.extras.notes_parts.values().any(|p| p == name)
                || read.extras.theme_parts.values().any(|p| p == name);
            assert!(
                read.passthrough.has(name)
                    || modelled
                    || name.starts_with("/docProps/")
                    || name.contains("comment")
                    || name.contains("uthors"),
                "{}: {name} is neither modelled nor kept",
                path.display()
            );
            assert!(
                !(read.passthrough.has(name) && read.presentation.parts.contains_key(name)),
                "{}: {name} is both modelled and kept",
                path.display()
            );
        }
        // The stored form round-trips the model.
        let stored = read.presentation.to_bytes().unwrap();
        let back = kiln_deck::Presentation::from_bytes(&stored).unwrap();
        assert_eq!(back, read.presentation, "{}", path.display());
    }
}

#[test]
fn placeholders_inherit_from_their_layouts() {
    let read = read_fixture("pptx/features/placeholders.pptx");
    let p = &read.presentation;
    assert_eq!(p.masters.len(), 1);
    let m = &p.masters[0];
    assert_eq!(m.layouts.len(), 11);
    assert_eq!(m.layouts[0].kind, LayoutKind::Title);
    assert_eq!(m.layouts[4].kind, LayoutKind::Comparison);
    assert!(!m.title_style.is_empty() && !m.body_style.is_empty());
    assert_eq!(m.theme.name, "Office Theme");
    // Slide placeholders carry no position of their own: it comes from the layout.
    let s = &p.slides[1];
    let title = &s.shapes[0];
    assert!(title.placeholder.unwrap().kind.is_title());
    let resolved = p.resolved_xfrm(title.id).expect("inherited transform");
    let layout_title = p
        .slide_layout(s)
        .unwrap()
        .1
        .shapes
        .iter()
        .find(|x| x.placeholder.is_some_and(|ph| ph.kind.is_title()))
        .unwrap();
    assert_eq!(
        Some(resolved),
        layout_title
            .xfrm
            .or_else(|| p.resolved_xfrm(layout_title.id))
    );
    // Body levels are paragraph outline levels.
    let body = s.shapes[1].text().unwrap();
    let levels: Vec<_> = body
        .paragraphs()
        .map(kiln_deck::TextBody::level_of)
        .collect();
    assert_eq!(levels, vec![0, 1, 2]);
    assert_eq!(read.extras.list_ids.len(), 11 + 1 + 11);
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
}

#[test]
fn pictures_keep_their_media_and_crop() {
    let read = read_fixture("pptx/features/pictures.pptx");
    let p = &read.presentation;
    let pics: Vec<_> = p.slides[0]
        .shapes
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Picture(pic) => Some(pic),
            _ => None,
        })
        .collect();
    assert_eq!(pics.len(), 4);
    for pic in &pics {
        let part = p.parts.get(&pic.media).expect("image stored");
        assert!(
            part.content_type.starts_with("image/"),
            "{}",
            part.content_type
        );
        assert!(!read.passthrough.has(&pic.media));
    }
    let cropped = pics.iter().find(|p| !p.crop.is_none()).unwrap();
    assert_eq!(
        (
            cropped.crop.l,
            cropped.crop.t,
            cropped.crop.r,
            cropped.crop.b
        ),
        (10_000, 15_000, 20_000, 5_000)
    );
}

#[test]
fn tables_read_cells_spans_and_style() {
    let read = read_fixture("pptx/features/tables.pptx");
    let tables: Vec<_> = read
        .presentation
        .slides
        .iter()
        .flat_map(|s| &s.shapes)
        .filter_map(|s| match &s.kind {
            ShapeKind::Table(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tables.len(), 2);
    for t in &tables {
        assert!(t.rows.iter().all(|r| r.cells.len() == t.grid.len()));
        assert!(t.style_id.is_some());
        assert!(t.look.band_row);
    }
    assert!(tables[0].look.first_row && !tables[1].look.first_row);
    assert!(
        tables
            .iter()
            .flat_map(|t| &t.rows)
            .flat_map(|r| &r.cells)
            .any(|c| c.grid_span > 1 || c.row_span > 1 || c.h_merge || c.v_merge),
        "a merged cell"
    );
}

#[test]
fn charts_are_stored_with_their_workbooks() {
    let read = read_fixture("pptx/features/charts.pptx");
    let p = &read.presentation;
    let charts: Vec<_> = p
        .slides
        .iter()
        .flat_map(|s| &s.shapes)
        .filter_map(|s| match &s.kind {
            ShapeKind::Chart(c) => Some(c.part.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(charts.len(), 9);
    for c in &charts {
        let part = &p.parts[c];
        assert!(kiln_charts::package::is_chart_content_type(
            &part.content_type
        ));
        assert!(kiln_charts::read::read_chart(&part.bytes).is_ok(), "{c}");
        let rels = kiln_ooxml::opc::rels_part_name(c);
        assert!(p.parts.contains_key(&rels), "{rels}");
    }
    assert!(
        p.parts.keys().any(|k| k.ends_with(".xlsx")),
        "embedded workbooks"
    );
    assert!(
        !read
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::ChartUnreadable)
    );
}

#[test]
fn transitions_map_to_the_twelve_and_keep_the_others() {
    let read = read_fixture("pptx/features/transitions.pptx");
    let t: Vec<_> = read
        .presentation
        .slides
        .iter()
        .map(|s| s.transition.clone().unwrap())
        .collect();
    assert_eq!(t.len(), 12);
    assert!(matches!(
        t[0].effect,
        TransitionEffect::Fade {
            through_black: false
        }
    ));
    assert_eq!(t[0].duration_ms, 1000, "spd=slow");
    assert_eq!(t[0].advance_after_ms, Some(3000));
    assert!(matches!(t[1].effect, TransitionEffect::Push { .. }));
    assert_eq!(t[1].duration_ms, 750, "spd=med");
    assert!(matches!(t[2].effect, TransitionEffect::Wipe { .. }));
    assert!(matches!(
        t[3].effect,
        TransitionEffect::Split {
            vertical: true,
            inward: false
        }
    ));
    assert!(matches!(t[4].effect, TransitionEffect::Cover { .. }));
    assert!(matches!(t[5].effect, TransitionEffect::Uncover { .. }));
    assert!(matches!(t[6].effect, TransitionEffect::Cut { .. }));
    assert!(matches!(
        t[7].effect,
        TransitionEffect::RandomBars { vertical: true }
    ));
    assert!(matches!(t[8].effect, TransitionEffect::Shape { .. }));
    // Dissolve, zoom and blinds have no exact twin: the nearest is shown, the original kept.
    for (i, x) in t.iter().enumerate().take(9) {
        assert!(x.original.is_none(), "slide {}", i + 1);
    }
    for x in &t[9..12] {
        let o = x.original.as_deref().unwrap();
        assert!(
            o.contains("dissolve") || o.contains("zoom") || o.contains("blinds"),
            "{o}"
        );
    }
    assert!(
        matches!(t[9].effect, TransitionEffect::Fade { .. }),
        "unknown → Fade"
    );
    assert_eq!(
        read.warnings
            .iter()
            .filter(|w| w.code == WarningCode::TransitionMapped)
            .count(),
        3
    );
    assert_eq!(read.extras.transitions.len(), 12);
}

#[test]
fn animations_become_the_timeline() {
    let read = read_fixture("pptx/features/animations.pptx");
    let s = &read.presentation.slides[0];
    let seq = &s.timeline.sequence;
    assert_eq!(seq.len(), 3);
    assert_eq!(seq[0].effect, AnimationEffect::Appear);
    assert_eq!(seq[0].duration_ms, 1);
    assert_eq!(seq[1].effect, AnimationEffect::Fade);
    assert_eq!(seq[1].duration_ms, 500);
    assert!(matches!(seq[2].effect, AnimationEffect::Wipe { .. }));
    assert!(seq.iter().all(|e| e.start == EffectStart::OnClick));
    // Targets are the slide's shapes.
    for e in seq {
        assert!(s.shapes.iter().any(|x| x.id == e.target.shape));
    }
    assert!(read.extras.timing.contains_key(&s.id));
    let second = &read.presentation.slides[1].timeline.sequence;
    assert_eq!(second.len(), 3);
}

#[test]
fn notes_sections_hidden_slides_and_sizes() {
    let notes = read_fixture("pptx/features/notes.pptx");
    let texts: Vec<_> = notes
        .presentation
        .slides
        .iter()
        .map(|s| s.notes_text())
        .collect();
    assert!(texts.iter().all(|t| !t.is_empty()), "{texts:?}");
    assert!(notes.presentation.notes_master.is_some());
    assert_eq!(notes.extras.notes_parts.len(), 3);

    let sections = read_fixture("pptx/features/sections.pptx");
    let names: Vec<_> = sections
        .presentation
        .sections
        .iter()
        .map(|s| (s.name.as_str(), s.len))
        .collect();
    assert_eq!(
        names,
        vec![("Introduction", 2), ("Results", 3), ("Appendix", 1)]
    );
    assert_eq!(sections.extras.section_guids.len(), 3);

    let many = read_fixture("pptx/features/many-slides.pptx");
    let hidden: Vec<_> = many
        .presentation
        .slides
        .iter()
        .enumerate()
        .filter(|(_, s)| s.hidden)
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(hidden, vec![4, 18]);

    let four = read_fixture("pptx/features/aspect-4x3.pptx");
    assert_eq!(four.presentation.slide_size.kind, SlideSizeKind::Standard);
    assert_eq!(four.presentation.slide_size.cx, 9_144_000);
}

#[test]
fn hyperlinks_go_to_urls_and_slides() {
    let read = read_fixture("pptx/features/hyperlinks.pptx");
    let p = &read.presentation;
    let mut links = Vec::new();
    for sh in &p.slides[0].shapes {
        if let Some(l) = &sh.link {
            links.push(l.clone());
        }
        if let Some(t) = sh.text() {
            for para in t.paragraphs() {
                for m in para.content.marks() {
                    if let Mark::Link(h) = &m.mark {
                        let kiln_doc::LinkTarget::Url(u) = &h.target else {
                            panic!("not a url")
                        };
                        links.push(u.clone());
                    }
                }
            }
        }
    }
    assert!(links.iter().any(|l| l.starts_with("http")), "{links:?}");
    let jump = format!("#slide:{}", p.slides[1].id.0);
    assert!(links.contains(&jump), "{links:?} lacks {jump}");
}

#[test]
fn connectors_attach_to_their_shapes() {
    let read = read_fixture("pptx/features/connectors.pptx");
    let s = &read.presentation.slides[0];
    let connectors: Vec<_> = s
        .shapes
        .iter()
        .filter_map(|x| match &x.kind {
            ShapeKind::Connector(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(connectors.len(), 4);
    let attached: Vec<_> = connectors
        .iter()
        .filter_map(|c| c.start.zip(c.end))
        .collect();
    assert!(!attached.is_empty());
    for (a, b) in attached {
        assert!(s.shapes.iter().any(|x| x.id == a.shape));
        assert!(s.shapes.iter().any(|x| x.id == b.shape));
    }
}

#[test]
fn groups_nest_with_child_spaces() {
    let read = read_fixture("pptx/features/groups.pptx");
    let s = &read.presentation.slides[0];
    let groups: Vec<_> = s.shapes.iter().filter(|x| x.children().is_some()).collect();
    assert!(!groups.is_empty());
    let nested = groups
        .iter()
        .any(|g| g.children().unwrap().iter().any(|c| c.children().is_some()));
    assert!(nested, "a group in a group");
    for g in groups {
        let ShapeKind::Group(gr) = &g.kind else {
            unreachable!()
        };
        assert!(gr.child_space.is_valid() && gr.child_space.cx > 0);
        let path = read.presentation.locate(gr.children[0].id).unwrap();
        assert_eq!(path.owner, Owner::Slide(s.id));
        assert_eq!(path.path.len(), 2);
        let _ = ShapePath {
            owner: path.owner,
            path: vec![0],
        };
    }
}

#[test]
fn backgrounds_and_theme_colours() {
    let read = read_fixture("pptx/features/backgrounds.pptx");
    let bgs: Vec<_> = read
        .presentation
        .slides
        .iter()
        .map(|s| s.background.clone())
        .collect();
    assert!(matches!(
        &bgs[0],
        Some(Background::Fill {
            fill: Fill::Solid(_),
            ..
        })
    ));
    assert!(matches!(
        &bgs[1],
        Some(Background::Fill {
            fill: Fill::Gradient(_),
            ..
        })
    ));
    assert!(matches!(
        &bgs[2],
        Some(Background::Fill {
            fill: Fill::Pattern(_),
            ..
        })
    ));

    let colours = read_fixture("pptx/features/theme-colors.pptx");
    // Shapes filled with scheme colours keep the scheme reference.
    let schemes = colours.presentation.slides[0]
        .shapes
        .iter()
        .filter_map(|s| s.props()?.fill.clone())
        .filter(|f| matches!(f, Fill::Solid(c) if matches!(c.model, kiln_ooxml::drawingml::color::ColorModel::Scheme(_))))
        .count();
    assert!(schemes >= 30, "{schemes}");

    let styled = read_fixture("pptx/features/shape-styles.pptx");
    assert!(
        styled.presentation.slides[0]
            .shapes
            .iter()
            .filter_map(|s| s.props())
            .any(|p| p.style.is_some())
    );
    let presets = read_fixture("pptx/features/preset-shapes.pptx");
    let names: std::collections::BTreeSet<_> = presets
        .presentation
        .slides
        .iter()
        .flat_map(|s| &s.shapes)
        .filter_map(|s| match s.props()?.geometry.as_ref()? {
            Geometry::Preset { name, .. } => Some(name.clone()),
            Geometry::Custom(_) => None,
        })
        .collect();
    assert_eq!(names.len(), 60);
}

#[test]
fn text_formatting_reads_runs_and_fields() {
    let read = read_fixture("pptx/features/text-formatting.pptx");
    let p = &read.presentation;
    let mut bold = false;
    let mut sized = false;
    for sh in p.slides.iter().flat_map(|s| &s.shapes) {
        let Some(t) = sh.text() else { continue };
        for para in t.paragraphs() {
            for r in para.content.runs() {
                bold |= r.props.bold == Some(true);
                sized |= r.props.size.is_some();
            }
            for o in para.content.objects() {
                if let InlineObject::Field(f) = o {
                    assert!(!f.instr.is_empty());
                }
            }
        }
        let _ = body_text(t);
    }
    assert!(bold && sized);
}

#[test]
fn smart_art_is_a_group_of_its_drawing() {
    // PowerPoint's own SmartArt, from the external corpus (`tools/fetch-corpus.sh`); a synthetic
    // diagram is tested in `read_elements.rs` for checkouts without it.
    if !fixtures().join("external/pptx/poi-SmartArt.pptx").exists() {
        return;
    }
    let read = read_fixture("external/pptx/poi-SmartArt.pptx");
    let s = &read.presentation.slides[0];
    let d = &s.shapes[0];
    assert!(read.extras.is_smart_art(d.id));
    let children = d.children().expect("drawn as a group");
    assert!(children.len() >= 3, "{}", children.len());
    assert!(
        children.iter().all(|c| c.text().is_some()),
        "the drawing's shapes hold text"
    );
    // The diagram parts stay as they are.
    for part in [
        "/ppt/diagrams/data1.xml",
        "/ppt/diagrams/drawing1.xml",
        "/ppt/diagrams/layout1.xml",
    ] {
        assert!(read.passthrough.has(part), "{part}");
    }
    assert!(
        read.warnings
            .iter()
            .any(|w| w.code == WarningCode::SmartArt)
    );
}

#[test]
fn facts_describe_the_structure_without_ids() {
    let a = snapshot_facts(&read_fixture("pptx/features/placeholders.pptx"));
    let b = snapshot_facts(&read_fixture("pptx/features/placeholders.pptx"));
    assert_eq!(a, b);
    assert!(a.iter().any(|f| f == "slide4/layout=Comparison"), "{a:?}");
    assert!(
        a.iter()
            .any(|f| f.starts_with("slide0/0=placeholder:Title 1"))
    );
    assert!(
        a.iter()
            .any(|f| f.starts_with("master0/layout0=title:Title Slide"))
    );
    let t = snapshot_facts(&read_fixture("pptx/features/transitions.pptx"));
    assert!(
        t.iter()
            .any(|f| f.starts_with("slide0/transition=Fade:1000ms")),
        "{t:?}"
    );
    let txt = kiln_io_pptx::plain_text(&read_fixture("pptx/features/tables.pptx"));
    assert!(txt.contains('\t'));
}
