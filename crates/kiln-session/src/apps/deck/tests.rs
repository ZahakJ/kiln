// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deck through the session: create, open every feature fixture, save and reopen, frames of the
//! stage, thumbnails, the accessibility tree and the queries.

use kiln_protocol::commands::{DeckCommand, DeckQuery, DeckQueryResult, DeckState};
use kiln_protocol::geom::Point;
use kiln_protocol::{
    Command, DocKind, FileFormat, OpenHint, Query, QueryResult, ViewMode, Viewport,
};

use super::DeckApp;
use crate::model::AppFactory;
use crate::{DeferredHost, Session};

pub(crate) fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pptx/features")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

pub(crate) fn viewport(zoom: f32) -> Viewport {
    Viewport {
        width_px: 1400.0,
        height_px: 820.0,
        scroll: Point::new(-60.0, -50.0),
        zoom,
        dpr: 1.0,
        mode: ViewMode::SlideNormal,
    }
}

fn state(s: &Session, doc: kiln_common::DocId) -> DeckState {
    match s
        .query(Some(doc), &Query::Deck(DeckQuery::State))
        .expect("state")
    {
        QueryResult::Deck(DeckQueryResult::State(st)) => *st,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_new_presentation_has_one_title_slide_and_paints() {
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let st = state(&s, doc);
    assert_eq!(st.slides.len(), 1);
    assert_eq!(st.slides[0].layout, "Title Slide");
    assert_eq!(st.theme, "Kiln");
    assert!((st.slide_width - 1280.0).abs() < 0.5);
    s.set_viewport(doc, viewport(0.8)).expect("viewport");
    let d = s.frame(doc).expect("frame");
    assert!(!d.tiles.is_empty(), "the stage has tiles");
    assert!(!d.atlas.is_empty(), "the prompts' glyphs");
    let status = match s.query(Some(doc), &Query::Status).expect("status") {
        QueryResult::Status(st) => st,
        other => panic!("{other:?}"),
    };
    assert_eq!(status.slide.map(|p| (p.current, p.total)), Some((1, 1)));
}

#[test]
fn every_feature_fixture_opens_paints_and_saves() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pptx/features");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("fixtures")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".pptx"))
        .collect();
    names.sort();
    assert!(names.len() >= 15);
    for name in names {
        let mut s = Session::new(Box::new(DeferredHost::new(|| 1_792_000_000_000)));
        let doc = s
            .open(
                &fixture(&name),
                OpenHint {
                    name: Some(name.clone()),
                    ..OpenHint::default()
                },
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .doc;
        s.set_viewport(doc, viewport(0.7)).expect("viewport");
        let d = s.frame(doc).expect("frame");
        assert!(!d.tiles.is_empty(), "{name}: no tiles");
        let st = state(&s, doc);
        assert!(!st.slides.is_empty(), "{name}");
        // Thumbnails of the first slides.
        let th = s
            .query(
                Some(doc),
                &Query::Deck(DeckQuery::Thumbnails {
                    from: 0,
                    count: 4,
                    width: 160.0,
                    dpr: 1.0,
                }),
            )
            .expect("thumbnails");
        let QueryResult::Deck(DeckQueryResult::Scene(scene)) = th else {
            panic!("{name}: {th:?}")
        };
        assert_eq!(scene.tiles.len(), st.slides.len().min(4), "{name}");
        // A save writes a package the reader opens again with the same slides.
        let model = DeckApp
            .open(&fixture(&name), FileFormat::Pptx, &OpenHint::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let bytes = model
            .save(FileFormat::Pptx)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let again = kiln_io_pptx::read_pptx(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(again.presentation.slides.len(), st.slides.len(), "{name}");
    }
}

#[test]
fn edits_save_and_reopen() {
    let mut m = DeckApp.create();
    let mut run = |c: DeckCommand| {
        m.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    };
    run(DeckCommand::NewSlide { layout: None });
    run(DeckCommand::InsertShape {
        preset: "rightArrow".into(),
        rect: None,
    });
    run(DeckCommand::InsertText {
        text: "Onwards".into(),
    });
    run(DeckCommand::Escape);
    run(DeckCommand::SetNotes {
        text: "Say why.".into(),
    });
    let bytes = m.save(FileFormat::Pptx).expect("save");
    let read = kiln_io_pptx::read_pptx(&bytes).expect("reopen");
    let p = read.presentation;
    assert_eq!(p.slides.len(), 2);
    let texts: Vec<String> = p.slides[1]
        .shapes
        .iter()
        .filter_map(|sh| sh.text().map(kiln_deck::TextBody::text))
        .collect();
    assert!(texts.iter().any(|t| t == "Onwards"), "{texts:?}");
    assert_eq!(
        p.slides[1].notes.as_ref().map(|n| n.text.text()).as_deref(),
        Some("Say why.")
    );
}

#[test]
fn the_accessibility_tree_names_the_slide_and_its_shapes() {
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s
        .open(
            &fixture("placeholders.pptx"),
            OpenHint {
                name: Some("placeholders.pptx".into()),
                ..OpenHint::default()
            },
        )
        .expect("open")
        .doc;
    let tree = s.a11y(doc).expect("a11y");
    let root = &tree.nodes[0];
    assert_eq!(root.role, kiln_protocol::A11yRole::Group);
    assert!(
        root.name
            .as_deref()
            .is_some_and(|n| n.contains("slide 1 of")),
        "{:?}",
        root.name
    );
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.role == kiln_protocol::A11yRole::TextBox)
    );
}

#[test]
fn layouts_and_themes_have_thumbnails() {
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let st = state(&s, doc);
    for q in [
        DeckQuery::LayoutThumbnails {
            width: 120.0,
            dpr: 1.0,
        },
        DeckQuery::ThemeThumbnails {
            width: 96.0,
            dpr: 1.0,
        },
    ] {
        let QueryResult::Deck(DeckQueryResult::Scene(scene)) =
            s.query(Some(doc), &Query::Deck(q.clone())).expect("query")
        else {
            panic!("{q:?}")
        };
        let want = if matches!(q, DeckQuery::LayoutThumbnails { .. }) {
            st.layouts.len()
        } else {
            st.themes.len()
        };
        assert_eq!(scene.tiles.len(), want, "{q:?}");
        assert!(want >= 11);
    }
}

/// Writes PNGs of the stage for visual review (`KILN_DECK_REVIEW=dir`).
#[test]
fn stage_review_renders() {
    let Some(dir) = std::env::var_os("KILN_DECK_REVIEW") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let run = |s: &mut Session, c: DeckCommand| {
        s.execute(Some(doc), Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"))
    };
    s.set_viewport(doc, viewport(0.9)).expect("viewport");
    let shot = |s: &mut Session, name: &str| {
        s.execute(
            Some(doc),
            Command::Suite(kiln_protocol::SuiteCommand::Repaint),
        )
        .expect("repaint");
        let d = s.frame(doc).expect("frame");
        let mut c = kiln_raster::Compositor::new();
        c.apply(&d).expect("apply");
        let px = c
            .render(
                &kiln_raster::View {
                    width: 1400,
                    height: 820,
                    scroll_x: -60.0,
                    scroll_y: -50.0,
                    options: kiln_raster::PaintOptions::default(),
                },
                &kiln_scene::Palette::light().for_app("deck"),
            )
            .expect("render");
        let png = kiln_raster::Rgba::from_pixmap(&px)
            .encode_png()
            .expect("png");
        std::fs::write(dir.join(format!("{name}.png")), png).expect("write");
    };
    shot(&mut s, "new");
    run(
        &mut s,
        DeckCommand::PointerDown {
            x: 400.0,
            y: 280.0,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
    run(
        &mut s,
        DeckCommand::InsertText {
            text: "Every ticket fires the next kiln.".into(),
        },
    );
    shot(&mut s, "typed");
    run(&mut s, DeckCommand::Escape);
    shot(&mut s, "selected");
    run(&mut s, DeckCommand::Escape);
    run(
        &mut s,
        DeckCommand::InsertShape {
            preset: "roundRect".into(),
            rect: None,
        },
    );
    run(
        &mut s,
        DeckCommand::PointerDown {
            x: 600.0,
            y: 400.0,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
    run(
        &mut s,
        DeckCommand::PointerMove {
            x: 650.0,
            y: 330.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    shot(&mut s, "dragging");
    run(
        &mut s,
        DeckCommand::PointerUp {
            x: 650.0,
            y: 330.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    shot(&mut s, "dropped");
}

#[test]
fn the_shapes_gallery_has_every_category() {
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let QueryResult::Deck(DeckQueryResult::Icons(icons)) = s
        .query(
            Some(doc),
            &Query::Deck(DeckQuery::ShapeIcons { size: 20.0 }),
        )
        .expect("icons")
    else {
        panic!("not icons")
    };
    assert!(icons.len() > 150, "{}", icons.len());
    for cat in [
        "lines",
        "basic",
        "arrows",
        "math",
        "flowchart",
        "stars",
        "callouts",
        "actions",
    ] {
        assert!(icons.iter().any(|i| i.category == cat), "{cat}");
    }
    let arrow = icons
        .iter()
        .find(|i| i.preset == "rightArrow")
        .expect("arrow");
    assert!(arrow.path.starts_with('M'), "{}", arrow.path);
}

/// the stage's first frame of a new presentation with the interface in `ui`.
fn first_frame(
    ui: &str,
    language: Option<&str>,
) -> (Session, kiln_common::DocId, kiln_scene::SceneDelta) {
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let options = kiln_protocol::commands::CoreOptions {
        ui_language: ui.to_owned(),
        ..Default::default()
    };
    s.execute(
        None,
        kiln_protocol::SuiteCommand::SetOptions { options }.into(),
    )
    .expect("options");
    let doc = s
        .execute(
            None,
            kiln_protocol::SuiteCommand::New {
                kind: DocKind::Deck,
                language: language.map(str::to_owned),
            }
            .into(),
        )
        .expect("new")
        .summary
        .expect("summary")
        .doc;
    s.set_viewport(doc, viewport(0.8)).expect("viewport");
    let d = s.frame(doc).expect("frame");
    (s, doc, d)
}

#[test]
fn placeholder_prompts_are_drawn_in_the_interface_language() {
    let (_, _, en) = first_frame("en", None);
    let (_, _, de) = first_frame("de", None);
    let (mut s, doc, ar) = first_frame("ar", Some("ar-SA"));
    assert!(!en.atlas.is_empty() && !de.atlas.is_empty() && !ar.atlas.is_empty());
    assert_ne!(en.atlas, ar.atlas, "Arabic prompts");
    assert_ne!(en.atlas, de.atlas, "German prompts");
    // The interface changes while the presentation is open: the stage redraws, and the
    // presentation itself does not change.
    let options = kiln_protocol::commands::CoreOptions {
        ui_language: "en".into(),
        ..Default::default()
    };
    s.execute(
        None,
        kiln_protocol::SuiteCommand::SetOptions { options }.into(),
    )
    .expect("options");
    let again = s.frame(doc).expect("frame");
    assert!(!again.tiles.is_empty(), "the stage redraws");
    let summary = s.summary(doc).expect("summary");
    assert!(!summary.dirty && !summary.can_undo);
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut text = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(name).expect(name), &mut text).expect("utf-8");
    text
}

#[test]
fn a_new_presentation_in_arabic_is_right_to_left_through_pptx() {
    use kiln_doc::st::Alignment;
    use kiln_ooxml::drawingml::fill::Fill;
    let (s, doc, _) = first_frame("ar", Some("ar-SA"));
    let bytes = s.docs[&doc].model.save(FileFormat::Pptx).expect("save");
    // Prompts are never content: the slide's placeholders are empty in every language.
    let slide = part(&bytes, "ppt/slides/slide1.xml");
    for prompt in ["انقر", "Click to add", "durch Klicken"] {
        assert!(!slide.contains(prompt), "{prompt} in {slide}");
    }
    // PowerPoint's right-to-left master: rtl="1", right-aligned title and body text.
    let master = part(&bytes, "ppt/slideMasters/slideMaster1.xml");
    let title = &master[master.find("<p:titleStyle>").expect("titleStyle")..];
    assert!(title.starts_with("<p:titleStyle><a:lvl1pPr") && title[..200].contains("rtl=\"1\""));
    assert!(title[..200].contains("algn=\"r\""), "{}", &title[..200]);

    let check = |p: &kiln_deck::presentation::Presentation| {
        assert!(p.is_right_to_left());
        let m = &p.masters[0];
        for style in [&m.title_style, &m.body_style] {
            let l = style.level(0);
            assert_eq!(l.para.bidi, Some(true));
            assert_eq!(l.para.alignment, Some(Alignment::Right));
        }
        // The title slide's title sits on the right; its glow on the left.
        let layout = m
            .layout_of_kind(&kiln_deck::slide::LayoutKind::Title)
            .expect("title layout");
        let title = layout
            .shapes
            .iter()
            .find(|s| s.placeholder.is_some_and(|ph| ph.kind.is_title()))
            .expect("title");
        let x = title.xfrm.expect("xfrm");
        assert_eq!(x.x + x.cx, p.slide_size.cx - p.slide_size.cx * 625 / 10_000);
        assert_eq!(
            title
                .text()
                .expect("text")
                .list_style
                .level(0)
                .para
                .alignment,
            Some(Alignment::Right)
        );
        match &layout.background {
            Some(kiln_deck::slide::Background::Fill {
                fill: Fill::Gradient(g),
                ..
            }) => {
                let r = g.path.and_then(|p| p.fill_to_rect).expect("fillToRect");
                assert!(r.l < r.r, "{r:?}");
            }
            other => panic!("{other:?}"),
        }
    };
    let deck = |b: &[u8]| {
        DeckApp
            .open(b, FileFormat::Pptx, &OpenHint::default())
            .expect("open")
    };
    let read = kiln_io_pptx::read_pptx(&bytes).expect("read");
    check(&read.presentation);
    // Saved again, it stays right to left.
    let again = deck(&bytes).save(FileFormat::Pptx).expect("save");
    check(&kiln_io_pptx::read_pptx(&again).expect("read").presentation);

    // Other languages keep left-to-right presentations.
    let (s, doc, _) = first_frame("de", Some("de-DE"));
    let bytes = s.docs[&doc].model.save(FileFormat::Pptx).expect("save");
    let p = kiln_io_pptx::read_pptx(&bytes).expect("read").presentation;
    assert!(!p.is_right_to_left());
    assert_eq!(
        p.masters[0].title_style.level(0).para.alignment,
        Some(Alignment::Left)
    );
}

#[test]
fn a_theme_applied_to_a_right_to_left_presentation_stays_right_to_left() {
    let mut p = kiln_deck::presentation::Presentation::new();
    p.make_right_to_left();
    let theme = kiln_deck::themes::bundled_theme("cobalt").expect("cobalt");
    let tx = kiln_deck::edit::theme::apply_theme(&mut p, &theme, None).expect("apply");
    p.apply(tx).expect("applied");
    assert!(p.is_right_to_left());
}
