// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deck commands, one behaviour per test: slides, sections, notes, inserting and arranging
//! objects, pointer gestures with snapping and modifiers, text editing, formatting, design,
//! the clipboard, tables, find and replace, undo.

use kiln_common::NodeId;
use kiln_deck::text::{Autofit, Bullet, TextDirection};
use kiln_deck::{Presentation, ShapeKind, TextAnchor};
use kiln_deck_layout::geom::{emu, px, rect_px};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_protocol::commands::{
    CaretMotion, ClipboardData, DeckAlign, DeckAnchor, DeckAutofit, DeckCommand as D,
    DeckDirection, DeckEffect, DeckFill, DeckOrder, DeckOutline, DeckRect, DeckScaleMode,
    DeckSlideSize, DeckTextDirection, EditCommand as E, TableDeleteTarget, TextCase,
};
use kiln_protocol::{Command, SuiteCommand};

use super::DeckDoc;
use crate::model::{Change, DocModel};

fn doc() -> DeckDoc {
    DeckDoc::new(Presentation::new(), None, None)
}

fn run(d: &mut DeckDoc, c: D) -> Change {
    d.execute(&Command::Deck(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

fn fails(d: &mut DeckDoc, c: D) -> String {
    d.execute(&Command::Deck(c))
        .expect_err("should fail")
        .to_string()
}

fn ed(d: &mut DeckDoc, e: E) -> Change {
    d.execute(&Command::Suite(SuiteCommand::Edit(e.clone())))
        .unwrap_or_else(|err| panic!("{e:?}: {err}"))
}

fn down(d: &mut DeckDoc, x: f32, y: f32) {
    run(
        d,
        D::PointerDown {
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
}

fn drag(d: &mut DeckDoc, from: (f32, f32), to: (f32, f32), shift: bool, ctrl: bool, alt: bool) {
    run(
        d,
        D::PointerDown {
            x: from.0,
            y: from.1,
            shift,
            ctrl,
            alt,
            clicks: 1,
        },
    );
    let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    run(
        d,
        D::PointerMove {
            x: mid.0,
            y: mid.1,
            shift,
            ctrl,
            alt,
        },
    );
    run(
        d,
        D::PointerMove {
            x: to.0,
            y: to.1,
            shift,
            ctrl,
            alt,
        },
    );
    run(
        d,
        D::PointerUp {
            x: to.0,
            y: to.1,
            shift,
            ctrl,
            alt,
        },
    );
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<DeckRect> {
    Some(DeckRect {
        x,
        y,
        width: w,
        height: h,
    })
}

/// Inserts a rectangle at a place and returns its id (selected).
fn shape_at(d: &mut DeckDoc, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    run(
        d,
        D::InsertShape {
            preset: "rect".into(),
            rect: rect(x, y, w, h),
        },
    );
    *d.view.shapes.last().expect("inserted")
}

fn bx(d: &DeckDoc, id: NodeId) -> kiln_scene::Bounds {
    rect_px(&d.xfrm_of(id).expect("xfrm"))
}

fn slide_shapes(d: &DeckDoc) -> Vec<NodeId> {
    d.pres.slides[d.view.current]
        .shapes
        .iter()
        .map(|s| s.id)
        .collect()
}

fn title(d: &DeckDoc) -> NodeId {
    d.pres.slides[d.view.current]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| p.kind.is_title()))
        .map(|s| s.id)
        .expect("title")
}

fn text_of(d: &DeckDoc, id: NodeId) -> String {
    d.pres
        .shape(id)
        .and_then(|s| s.text())
        .map(kiln_deck::TextBody::text)
        .unwrap_or_default()
}

fn centre(d: &DeckDoc, id: NodeId) -> (f32, f32) {
    let b = bx(d, id);
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

/// Clicks into the title placeholder and types.
fn type_title(d: &mut DeckDoc, text: &str) -> NodeId {
    let t = title(d);
    let (x, y) = centre(d, t);
    down(d, x, y);
    run(
        d,
        D::PointerUp {
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    run(d, D::InsertText { text: text.into() });
    t
}

fn first_run(d: &DeckDoc, id: NodeId) -> kiln_doc::RunProps {
    let body = d.pres.shape(id).and_then(|s| s.text()).expect("text");
    let p = body.paragraphs().next().expect("para");
    p.content.props_at(0).cloned().unwrap_or_default()
}

// ── slides ──────────────────────────────────────────────────────────────────────────────

#[test]
fn new_slide_after_a_title_slide_is_title_and_content() {
    let mut d = doc();
    assert_eq!(run(&mut d, D::NewSlide { layout: None }), Change::Content);
    assert_eq!(d.pres.slides.len(), 2);
    assert_eq!(d.view.current, 1);
    let (_, l) = d.pres.layout(d.pres.slides[1].layout).expect("layout");
    assert_eq!(l.kind, kiln_deck::LayoutKind::TitleContent);
}

#[test]
fn new_slide_takes_a_layout_from_the_gallery() {
    let mut d = doc();
    let blank = d.pres.masters[0]
        .layout_of_kind(&kiln_deck::LayoutKind::Blank)
        .expect("blank")
        .id;
    run(
        &mut d,
        D::NewSlide {
            layout: Some(blank),
        },
    );
    assert_eq!(d.pres.slides[1].layout, blank);
    assert!(d.pres.slides[1].shapes.is_empty());
}

#[test]
fn duplicate_copies_the_selected_slides_after_them() {
    let mut d = doc();
    type_title(&mut d, "One");
    run(&mut d, D::Escape);
    run(&mut d, D::Escape);
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::Duplicate);
    assert_eq!(d.pres.slides.len(), 2);
    assert_eq!(d.view.current, 1);
    assert_eq!(
        kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[1].id),
        "One"
    );
}

#[test]
fn delete_slides_removes_the_selection_and_shows_the_next() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(&mut d, D::NewSlide { layout: None });
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::DeleteSlides);
    assert_eq!(d.pres.slides.len(), 2);
    assert_eq!(d.view.current, 1);
}

#[test]
fn move_slides_reorders_like_a_drag_in_the_rail() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(&mut d, D::NewSlide { layout: None });
    let ids: Vec<NodeId> = d.pres.slides.iter().map(|s| s.id).collect();
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::MoveSlides { to: 3 });
    let now: Vec<NodeId> = d.pres.slides.iter().map(|s| s.id).collect();
    assert_eq!(now, vec![ids[1], ids[2], ids[0]]);
    assert_eq!(d.view.slides, vec![2]);
}

#[test]
fn shift_and_ctrl_select_slides_in_the_rail() {
    let mut d = doc();
    for _ in 0..3 {
        run(&mut d, D::NewSlide { layout: None });
    }
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    run(
        &mut d,
        D::SelectSlide {
            index: 2,
            extend: true,
            toggle: false,
        },
    );
    assert_eq!(d.view.slides, vec![0, 1, 2]);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: true,
        },
    );
    assert_eq!(d.view.slides, vec![0, 2]);
}

#[test]
fn previous_and_next_slide_move_the_stage() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(&mut d, D::PreviousSlide);
    assert_eq!(d.view.current, 0);
    run(&mut d, D::PreviousSlide);
    assert_eq!(d.view.current, 0);
    run(&mut d, D::NextSlide);
    assert_eq!(d.view.current, 1);
}

#[test]
fn layout_change_keeps_the_text() {
    let mut d = doc();
    type_title(&mut d, "Kept");
    run(&mut d, D::Escape);
    let tc = d.pres.masters[0]
        .layout_of_kind(&kiln_deck::LayoutKind::TitleContent)
        .expect("layout")
        .id;
    run(&mut d, D::SetLayout { layout: tc });
    assert_eq!(d.pres.slides[0].layout, tc);
    assert_eq!(
        kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[0].id),
        "Kept"
    );
}

#[test]
fn reset_puts_a_moved_placeholder_back() {
    let mut d = doc();
    let t = title(&d);
    let before = bx(&d, t);
    run(&mut d, D::SelectShapes { ids: vec![t] });
    run(
        &mut d,
        D::Nudge {
            direction: DeckDirection::Right,
            fine: false,
        },
    );
    assert!(bx(&d, t).x > before.x);
    run(&mut d, D::ResetSlides);
    assert!((bx(&d, t).x - before.x).abs() < 0.01);
}

#[test]
fn hide_slide_toggles() {
    let mut d = doc();
    run(&mut d, D::HideSlides { hidden: None });
    assert!(d.pres.slides[0].hidden);
    run(&mut d, D::HideSlides { hidden: None });
    assert!(!d.pres.slides[0].hidden);
}

#[test]
fn sections_are_added_renamed_moved_and_removed() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(&mut d, D::AddSection { name: None });
    assert_eq!(d.pres.sections.len(), 2, "{:?}", d.pres.sections);
    run(
        &mut d,
        D::RenameSection {
            index: 1,
            name: "Plans".into(),
        },
    );
    assert_eq!(d.pres.sections[1].name, "Plans");
    run(
        &mut d,
        D::MoveSection {
            index: 1,
            delta: -1,
        },
    );
    assert_eq!(d.pres.sections[0].name, "Plans");
    run(
        &mut d,
        D::RemoveSection {
            index: 0,
            with_slides: false,
        },
    );
    assert_eq!(d.pres.slides.len(), 2);
}

#[test]
fn notes_are_set_and_undone() {
    let mut d = doc();
    run(
        &mut d,
        D::SetNotes {
            text: "Say why.\nThen how.".into(),
        },
    );
    let notes = d.pres.slides[0].notes.as_ref().map(|n| n.text.text());
    assert_eq!(notes.as_deref(), Some("Say why.\nThen how."));
    assert!(d.undo());
    assert!(
        d.pres.slides[0]
            .notes
            .as_ref()
            .is_none_or(|n| n.text.text().is_empty())
    );
    assert_eq!(
        run(
            &mut d,
            D::SetNotes {
                text: String::new()
            }
        ),
        Change::None
    );
}

#[test]
fn undo_and_redo_label_the_step() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    assert_eq!(d.undo_label(), Some("deck.slides.new_slide"));
    assert!(d.undo());
    assert_eq!(d.pres.slides.len(), 1);
    assert_eq!(d.redo_label(), Some("deck.slides.new_slide"));
    assert!(d.redo());
    assert_eq!(d.pres.slides.len(), 2);
}

// ── inserting objects ───────────────────────────────────────────────────────────────────

#[test]
fn insert_shape_lands_in_the_middle_selected() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertShape {
            preset: "ellipse".into(),
            rect: None,
        },
    );
    let id = d.view.shapes[0];
    let b = bx(&d, id);
    assert!((b.x + b.w / 2.0 - 640.0).abs() < 1.0 && (b.y + b.h / 2.0 - 360.0).abs() < 1.0);
    assert_eq!(slide_shapes(&d).last(), Some(&id));
    // Its text is centred both ways, as PowerPoint's.
    let body = d.pres.shape(id).and_then(|s| s.text()).expect("text");
    assert_eq!(body.body.anchor, Some(kiln_deck::text::TextAnchor::Middle));
    assert!(
        body.paragraphs()
            .all(|p| p.props.alignment == Some(kiln_doc::st::Alignment::Center))
    );
}

#[test]
fn unknown_presets_are_refused() {
    let mut d = doc();
    assert!(
        fails(
            &mut d,
            D::InsertShape {
                preset: "teapot".into(),
                rect: None
            }
        )
        .contains("teapot")
    );
}

#[test]
fn lines_are_inserted_as_connectors() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertShape {
            preset: "straightConnector1".into(),
            rect: rect(10.0, 10.0, 200.0, 0.0),
        },
    );
    let id = d.view.shapes[0];
    assert!(matches!(
        d.pres.shape(id).map(|s| &s.kind),
        Some(ShapeKind::Connector(_))
    ));
}

#[test]
fn a_new_text_box_is_edited_at_once() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertTextBox {
            rect: rect(100.0, 100.0, 300.0, 40.0),
        },
    );
    assert!(d.view.text.is_some());
    run(
        &mut d,
        D::InsertText {
            text: "Hello".into(),
        },
    );
    assert_eq!(text_of(&d, d.view.shapes[0]), "Hello");
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 80, 40, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

#[test]
fn pictures_are_fitted_and_stored_as_parts() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertPicture {
            name: "kiln.png".into(),
            bytes: png(4000, 1000),
            rect: None,
        },
    );
    let id = d.view.shapes[0];
    let ShapeKind::Picture(p) = &d.pres.shape(id).expect("pic").kind else {
        panic!("not a picture")
    };
    assert!(d.pres.parts.contains_key(&p.media));
    let b = bx(&d, id);
    assert!(
        b.w <= 1280.0 * 0.8 + 0.5 && (b.w / b.h - 4.0).abs() < 0.01,
        "{b:?}"
    );
    assert_eq!(
        d.pres.shape(id).and_then(|s| s.alt_text.clone()).as_deref(),
        Some("kiln")
    );
}

#[test]
fn bytes_that_are_not_a_picture_are_refused() {
    let mut d = doc();
    assert!(
        fails(
            &mut d,
            D::InsertPicture {
                name: "x.png".into(),
                bytes: b"nope".to_vec(),
                rect: None
            }
        )
        .contains("not a picture")
    );
}

#[test]
fn insert_table_puts_the_caret_in_the_first_cell() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 3, cols: 4 });
    let id = d.view.shapes[0];
    let ShapeKind::Table(t) = &d.pres.shape(id).expect("table").kind else {
        panic!()
    };
    assert_eq!((t.rows.len(), t.grid.len()), (3, 4));
    run(&mut d, D::InsertText { text: "Q1".into() });
    let ShapeKind::Table(t) = &d.pres.shape(id).expect("table").kind else {
        panic!()
    };
    assert_eq!(t.cell(0, 0).map(|c| c.text.text()).as_deref(), Some("Q1"));
}

#[test]
fn insert_chart_adds_a_chart_with_sample_data() {
    let mut d = doc();
    d.execute(&Command::Suite(SuiteCommand::ChartInsert { kind: None }))
        .expect("chart");
    let id = d.view.shapes[0];
    let ShapeKind::Chart(c) = &d.pres.shape(id).expect("chart").kind else {
        panic!()
    };
    assert!(
        d.pres.parts[&c.part].bytes.starts_with(b"<?xml")
            || !d.pres.parts[&c.part].bytes.is_empty()
    );
}

#[test]
fn delete_removes_the_selected_objects() {
    let mut d = doc();
    let id = shape_at(&mut d, 10.0, 10.0, 50.0, 50.0);
    run(&mut d, D::Delete);
    assert!(d.pres.shape(id).is_none());
    assert!(d.view.shapes.is_empty());
}

#[test]
fn ctrl_d_duplicates_objects_offset() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(&mut d, D::Duplicate);
    let copy = d.view.shapes[0];
    assert_ne!(copy, id);
    assert!((bx(&d, copy).x - 112.0).abs() < 0.01);
    assert_eq!(d.pres.slides.len(), 1);
}

// ── arranging ───────────────────────────────────────────────────────────────────────────

#[test]
fn arrows_nudge_by_the_grid_and_ctrl_by_a_pixel() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::Nudge {
            direction: DeckDirection::Right,
            fine: false,
        },
    );
    assert!((bx(&d, id).x - 108.0).abs() < 0.01);
    run(
        &mut d,
        D::Nudge {
            direction: DeckDirection::Up,
            fine: true,
        },
    );
    assert!((bx(&d, id).y - 99.0).abs() < 0.01);
}

#[test]
fn shift_arrows_resize() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::Resize {
            direction: DeckDirection::Right,
            fine: false,
        },
    );
    run(
        &mut d,
        D::Resize {
            direction: DeckDirection::Up,
            fine: true,
        },
    );
    let b = bx(&d, id);
    assert!(
        (b.w - 58.0).abs() < 0.01 && (b.h - 49.0).abs() < 0.01,
        "{b:?}"
    );
}

#[test]
fn size_and_position_take_points_and_lock_the_aspect() {
    let mut d = doc();
    let id = shape_at(&mut d, 0.0, 0.0, 100.0, 50.0);
    run(
        &mut d,
        D::SetTransform {
            x: Some(72.0),
            y: Some(36.0),
            width: Some(150.0),
            height: None,
            rotation: None,
            lock_aspect: true,
        },
    );
    let b = bx(&d, id);
    assert!((b.x - 96.0).abs() < 0.01 && (b.y - 48.0).abs() < 0.01);
    assert!(
        (b.w - 200.0).abs() < 0.01 && (b.h - 100.0).abs() < 0.01,
        "{b:?}"
    );
    run(
        &mut d,
        D::SetTransform {
            x: None,
            y: None,
            width: None,
            height: None,
            rotation: Some(30.0),
            lock_aspect: false,
        },
    );
    assert_eq!(d.xfrm_of(id).expect("x").rot, 30 * 60_000);
}

#[test]
fn rotate_and_flip() {
    let mut d = doc();
    let id = shape_at(&mut d, 0.0, 0.0, 100.0, 50.0);
    run(&mut d, D::Rotate { degrees: 90.0 });
    assert_eq!(d.xfrm_of(id).expect("x").rot, 90 * 60_000);
    run(&mut d, D::Flip { horizontal: true });
    assert!(d.xfrm_of(id).expect("x").flip_h);
}

#[test]
fn one_object_aligns_to_the_slide() {
    let mut d = doc();
    let id = shape_at(&mut d, 300.0, 300.0, 100.0, 50.0);
    run(
        &mut d,
        D::Align {
            how: DeckAlign::Right,
        },
    );
    assert!((bx(&d, id).right() - 1280.0).abs() < 0.01);
    run(
        &mut d,
        D::Align {
            how: DeckAlign::Middle,
        },
    );
    assert!((bx(&d, id).y + 25.0 - 360.0).abs() < 0.01);
}

#[test]
fn several_objects_align_to_each_other_and_distribute() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    let b = shape_at(&mut d, 400.0, 160.0, 50.0, 50.0);
    let c = shape_at(&mut d, 500.0, 220.0, 50.0, 50.0);
    run(&mut d, D::SelectShapes { ids: vec![a, b, c] });
    run(
        &mut d,
        D::Align {
            how: DeckAlign::Top,
        },
    );
    assert!((bx(&d, b).y - 100.0).abs() < 0.01 && (bx(&d, c).y - 100.0).abs() < 0.01);
    run(&mut d, D::Distribute { horizontal: true });
    let gap1 = bx(&d, b).x - bx(&d, a).right();
    let gap2 = bx(&d, c).x - bx(&d, b).right();
    assert!((gap1 - gap2).abs() < 0.1, "{gap1} {gap2}");
}

#[test]
fn group_and_ungroup() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    let b = shape_at(&mut d, 300.0, 100.0, 50.0, 50.0);
    run(&mut d, D::SelectShapes { ids: vec![a, b] });
    run(&mut d, D::Group);
    let g = d.view.shapes[0];
    assert!(matches!(
        d.pres.shape(g).map(|s| &s.kind),
        Some(ShapeKind::Group(_))
    ));
    assert!(fails(&mut d, D::Group).contains("two or more"));
    run(&mut d, D::Ungroup);
    assert_eq!(d.view.shapes.len(), 2);
    assert!(d.pres.shape(g).is_none());
}

#[test]
fn order_brings_to_front_and_back() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    let b = shape_at(&mut d, 120.0, 120.0, 50.0, 50.0);
    run(&mut d, D::SelectShapes { ids: vec![a] });
    run(
        &mut d,
        D::Order {
            order: DeckOrder::Front,
        },
    );
    assert_eq!(slide_shapes(&d).last(), Some(&a));
    run(
        &mut d,
        D::Order {
            order: DeckOrder::Back,
        },
    );
    assert_eq!(slide_shapes(&d)[0], a);
    let _ = b;
}

#[test]
fn alt_text_and_links() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::SetAltText {
            text: "A kiln".into(),
        },
    );
    assert_eq!(
        d.pres.shape(id).and_then(|s| s.alt_text.clone()).as_deref(),
        Some("A kiln")
    );
    run(
        &mut d,
        D::Link {
            url: Some("https://kiln.example/".into()),
        },
    );
    assert_eq!(
        d.pres.shape(id).and_then(|s| s.link.clone()).as_deref(),
        Some("https://kiln.example/")
    );
}

#[test]
fn crop_and_reset_a_picture() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertPicture {
            name: "k.png".into(),
            bytes: png(100, 100),
            rect: None,
        },
    );
    let id = d.view.shapes[0];
    run(
        &mut d,
        D::Crop {
            left: 10.0,
            top: 0.0,
            right: 5.0,
            bottom: 0.0,
        },
    );
    let crop = |d: &DeckDoc| match &d.pres.shape(id).expect("p").kind {
        ShapeKind::Picture(p) => p.crop,
        _ => panic!(),
    };
    assert_eq!(crop(&d).l, 10_000);
    run(&mut d, D::ResetPicture);
    assert!(crop(&d).is_none());
}

#[test]
fn change_shape_keeps_position() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::ChangeShape {
            preset: "ellipse".into(),
        },
    );
    let p = d.pres.resolved_props(id).expect("props");
    assert_eq!(
        p.geometry
            .as_ref()
            .and_then(kiln_deck::Geometry::preset_name),
        Some("ellipse")
    );
    assert!((bx(&d, id).x - 100.0).abs() < 0.01);
}

// ── the stage ───────────────────────────────────────────────────────────────────────────

#[test]
fn clicking_selects_shift_adds_and_empty_clears() {
    let mut d = doc();
    let a = shape_at(&mut d, 900.0, 500.0, 60.0, 60.0);
    let b = shape_at(&mut d, 1100.0, 500.0, 60.0, 60.0);
    run(&mut d, D::Escape);
    down(&mut d, 930.0, 530.0);
    assert_eq!(d.view.shapes, vec![a]);
    run(
        &mut d,
        D::PointerUp {
            x: 930.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    run(
        &mut d,
        D::PointerDown {
            x: 1130.0,
            y: 530.0,
            shift: true,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
    assert_eq!(d.view.shapes, vec![a, b]);
    run(
        &mut d,
        D::PointerUp {
            x: 1130.0,
            y: 530.0,
            shift: true,
            ctrl: false,
            alt: false,
        },
    );
    down(&mut d, 1250.0, 20.0);
    run(
        &mut d,
        D::PointerUp {
            x: 1250.0,
            y: 20.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    assert!(d.view.shapes.is_empty());
}

#[test]
fn a_marquee_selects_the_objects_inside() {
    let mut d = doc();
    let a = shape_at(&mut d, 900.0, 600.0, 40.0, 40.0);
    let b = shape_at(&mut d, 1000.0, 600.0, 40.0, 40.0);
    run(&mut d, D::Escape);
    drag(&mut d, (880.0, 590.0), (1060.0, 660.0), false, false, false);
    assert_eq!(d.view.shapes.len(), 2);
    assert!(d.view.shapes.contains(&a) && d.view.shapes.contains(&b));
}

#[test]
fn a_drag_moves_and_undoes_in_one_step() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 60.0, 60.0);
    let steps = d.done.len();
    drag(&mut d, (930.0, 530.0), (1000.0, 600.0), false, false, true);
    assert!((bx(&d, id).x - 970.0).abs() < 0.01 && (bx(&d, id).y - 570.0).abs() < 0.01);
    assert_eq!(d.done.len(), steps + 1);
    assert!(d.undo());
    assert!((bx(&d, id).x - 900.0).abs() < 0.01);
}

#[test]
fn moves_snap_to_the_slide_centre_with_a_guide() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 100.0, 60.0);
    // Centre ends 3 px from the slide's vertical centre line (640).
    run(
        &mut d,
        D::PointerDown {
            x: 950.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
    run(
        &mut d,
        D::PointerMove {
            x: 643.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    assert!(d.drag.as_ref().is_some_and(|g| !g.guides.is_empty()));
    run(
        &mut d,
        D::PointerUp {
            x: 643.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    let b = bx(&d, id);
    assert!((b.x + b.w / 2.0 - 640.0).abs() < 0.01, "{b:?}");
}

#[test]
fn alt_turns_snapping_off() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 100.0, 60.0);
    drag(&mut d, (950.0, 530.0), (643.0, 530.0), false, false, true);
    let b = bx(&d, id);
    assert!((b.x + b.w / 2.0 - 643.0).abs() < 0.01, "{b:?}");
}

#[test]
fn shift_constrains_a_move_to_one_axis() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 60.0, 60.0);
    drag(&mut d, (930.0, 530.0), (1030.0, 545.0), true, false, true);
    let b = bx(&d, id);
    assert!(
        (b.y - 500.0).abs() < 0.01 && (b.x - 1000.0).abs() < 0.01,
        "{b:?}"
    );
}

#[test]
fn ctrl_drag_duplicates() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 60.0, 60.0);
    let n = slide_shapes(&d).len();
    drag(&mut d, (930.0, 530.0), (1030.0, 600.0), false, true, true);
    assert_eq!(slide_shapes(&d).len(), n + 1);
    assert!((bx(&d, id).x - 900.0).abs() < 0.01, "the original stays");
    let copy = d.view.shapes[0];
    assert!((bx(&d, copy).x - 1000.0).abs() < 0.01);
}

#[test]
fn the_corner_handle_resizes_and_shift_keeps_proportions() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 100.0, 50.0);
    drag(&mut d, (1000.0, 550.0), (1050.0, 600.0), false, false, true);
    let b = bx(&d, id);
    assert!(
        (b.w - 150.0).abs() < 0.01 && (b.h - 100.0).abs() < 0.01,
        "{b:?}"
    );
    drag(&mut d, (1050.0, 600.0), (1110.0, 610.0), true, false, true);
    let b = bx(&d, id);
    assert!((b.w / b.h - 1.5).abs() < 0.01, "{b:?}");
}

#[test]
fn the_rotation_handle_turns_in_15_degree_steps_with_shift() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 400.0, 100.0, 100.0);
    let top = (950.0, 400.0 - kiln_deck_layout::select::ROTATE_OFFSET);
    drag(&mut d, top, (1090.0, 380.0), true, false, false);
    let rot = d.xfrm_of(id).expect("x").rot;
    assert_eq!(rot % (15 * 60_000), 0);
    assert!(rot > 0);
}

#[test]
fn an_armed_shape_is_drawn_by_dragging() {
    let mut d = doc();
    run(
        &mut d,
        D::ArmInsert {
            preset: Some("ellipse".into()),
        },
    );
    drag(&mut d, (100.0, 600.0), (300.0, 700.0), false, false, true);
    let id = d.view.shapes[0];
    let b = bx(&d, id);
    assert!(
        (b.w - 200.0).abs() < 0.5 && (b.h - 100.0).abs() < 0.5,
        "{b:?}"
    );
    assert!(d.view.armed.is_none());
}

#[test]
fn escape_cancels_a_drag_and_then_clears() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 60.0, 60.0);
    run(
        &mut d,
        D::PointerDown {
            x: 930.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: true,
            clicks: 1,
        },
    );
    run(
        &mut d,
        D::PointerMove {
            x: 1100.0,
            y: 600.0,
            shift: false,
            ctrl: false,
            alt: true,
        },
    );
    run(&mut d, D::Escape);
    assert!((bx(&d, id).x - 900.0).abs() < 0.01);
    run(&mut d, D::Escape);
    assert!(d.view.shapes.is_empty());
}

#[test]
fn clicking_a_placeholder_edits_its_text_and_hides_the_prompt() {
    let mut d = doc();
    let t = type_title(&mut d, "Open studio");
    assert_eq!(text_of(&d, t), "Open studio");
    assert!(d.view.text.as_ref().is_some_and(|e| e.shape == t));
}

#[test]
fn double_click_edits_a_shapes_text() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 120.0, 60.0);
    run(&mut d, D::Escape);
    run(
        &mut d,
        D::PointerDown {
            x: 960.0,
            y: 530.0,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 2,
        },
    );
    assert!(d.view.text.as_ref().is_some_and(|e| e.shape == id));
}

#[test]
fn tab_cycles_objects_and_ctrl_a_selects_all() {
    let mut d = doc();
    let n = slide_shapes(&d).len();
    run(&mut d, D::Escape);
    run(&mut d, D::CycleSelection { backward: false });
    assert_eq!(d.view.shapes.len(), 1);
    run(&mut d, D::SelectAll);
    assert_eq!(d.view.shapes.len(), n);
}

#[test]
fn moving_a_shape_keeps_its_connector_attached() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 100.0, 100.0);
    let b = shape_at(&mut d, 500.0, 100.0, 100.0, 100.0);
    let mut c = kiln_deck::edit::new_connector(
        &mut d.pres,
        "straightConnector1",
        (emu(200.0), emu(150.0)),
        (emu(500.0), emu(150.0)),
        (
            Some(kiln_deck::Connection { shape: a, site: 3 }),
            Some(kiln_deck::Connection { shape: b, site: 1 }),
        ),
    );
    c.name = "Connector".into();
    let cid = c.id;
    d.insert_on_slide("deck.insert.shape", vec![c])
        .expect("insert");
    run(&mut d, D::SelectShapes { ids: vec![b] });
    run(
        &mut d,
        D::Nudge {
            direction: DeckDirection::Down,
            fine: false,
        },
    );
    let x = d.xfrm_of(cid).expect("connector");
    assert!((px(x.y + x.cy) - 158.0).abs() < 0.5, "{x:?}");
}

// ── text ────────────────────────────────────────────────────────────────────────────────

#[test]
fn backspace_and_delete() {
    let mut d = doc();
    let t = type_title(&mut d, "Kilns");
    run(
        &mut d,
        D::DeleteText {
            forward: false,
            word: false,
        },
    );
    assert_eq!(text_of(&d, t), "Kiln");
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::DocumentStart,
            extend: false,
        },
    );
    run(
        &mut d,
        D::DeleteText {
            forward: true,
            word: false,
        },
    );
    assert_eq!(text_of(&d, t), "iln");
}

#[test]
fn ctrl_backspace_deletes_a_word() {
    let mut d = doc();
    let t = type_title(&mut d, "Open studio weekend");
    run(
        &mut d,
        D::DeleteText {
            forward: false,
            word: true,
        },
    );
    assert_eq!(text_of(&d, t), "Open studio ");
}

#[test]
fn enter_splits_and_shift_enter_breaks_the_line() {
    let mut d = doc();
    let t = type_title(&mut d, "One");
    run(&mut d, D::NewParagraph);
    run(&mut d, D::InsertText { text: "Two".into() });
    assert_eq!(text_of(&d, t), "One\nTwo");
    run(&mut d, D::LineBreak);
    run(
        &mut d,
        D::InsertText {
            text: "Three".into(),
        },
    );
    let body = d.pres.shape(t).and_then(|s| s.text()).expect("text");
    assert_eq!(body.paragraph_count(), 2);
}

#[test]
fn tab_at_a_paragraph_start_demotes_and_shift_tab_promotes() {
    let mut d = doc();
    let tc = d.pres.masters[0]
        .layout_of_kind(&kiln_deck::LayoutKind::TitleContent)
        .expect("l")
        .id;
    run(&mut d, D::NewSlide { layout: Some(tc) });
    let body_ph = d.pres.slides[1]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| !p.kind.is_title()))
        .map(|s| s.id)
        .expect("body");
    let (x, y) = centre(&d, body_ph);
    down(&mut d, x, y);
    run(
        &mut d,
        D::InsertText {
            text: "Point".into(),
        },
    );
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::LineStart,
            extend: false,
        },
    );
    run(&mut d, D::Tab { shift: false });
    let level = |d: &DeckDoc| {
        let b = d.pres.shape(body_ph).and_then(|s| s.text()).expect("t");
        kiln_deck::TextBody::level_of(b.paragraphs().next().expect("p"))
    };
    assert_eq!(level(&d), 1);
    run(&mut d, D::Tab { shift: true });
    assert_eq!(level(&d), 0);
    run(&mut d, D::Demote);
    assert_eq!(level(&d), 1);
    run(&mut d, D::Promote);
    assert_eq!(level(&d), 0);
}

#[test]
fn caret_motion_and_shift_selection() {
    let mut d = doc();
    type_title(&mut d, "Open studio");
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::WordBackward,
            extend: true,
        },
    );
    assert_eq!(d.selected_text_string().as_deref(), Some("studio"));
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::LineStart,
            extend: false,
        },
    );
    assert_eq!(d.view.text.as_ref().map(|t| t.head.offset), Some(0));
}

#[test]
fn arrows_in_a_right_to_left_paragraph_move_in_logical_order() {
    let mut d = doc();
    let t = type_title(&mut d, "مرحبا Kiln");
    // The title's paragraph right to left (`a:pPr rtl="1"`).
    let shape = d.pres.shape_mut(t).expect("title");
    let old = shape.text_mut().expect("text").clone();
    let paras: Vec<_> = old
        .paragraphs()
        .map(|p| {
            let mut p = p.clone();
            p.props.bidi = Some(true);
            p
        })
        .collect();
    let mut body =
        kiln_deck::TextBody::from_paragraphs(paras, old.all_deck_props().clone()).expect("body");
    body.body = old.body.clone();
    body.list_style = old.list_style.clone();
    *shape.text_mut().expect("text") = body;
    d.renderer.get_mut().invalidate();
    let para = d.view.text.as_ref().expect("editing").head.para;
    let at = |d: &DeckDoc| d.view.text.as_ref().map(|t| (t.head.para, t.head.offset));
    let mv = |d: &mut DeckDoc, motion| {
        run(
            d,
            D::MoveCaret {
                motion,
                extend: false,
            },
        );
    };
    mv(&mut d, CaretMotion::LineStart);
    assert_eq!(at(&d), Some((para, 0)));
    // Left goes on in the text (towards the left of the screen in Arabic), Right goes back.
    mv(&mut d, CaretMotion::Left);
    assert_eq!(at(&d), Some((para, 1)));
    mv(&mut d, CaretMotion::Right);
    assert_eq!(at(&d), Some((para, 0)));
    // Ctrl+Left is the next word.
    mv(&mut d, CaretMotion::WordLeft);
    assert_eq!(at(&d), Some((para, 6)));
    // The layout runs the paragraph right to left: its start is at the right.
    let (layout, _) = d
        .text_geometry(crate::apps::deck::TextTarget::Shape(t))
        .expect("layout");
    assert!(layout.is_rtl(para));
}

#[test]
fn select_all_while_editing_selects_the_text() {
    let mut d = doc();
    type_title(&mut d, "Open studio");
    run(&mut d, D::SelectAll);
    assert_eq!(d.selected_text_string().as_deref(), Some("Open studio"));
    run(
        &mut d,
        D::InsertText {
            text: "Closed".into(),
        },
    );
    assert_eq!(text_of(&d, title(&d)), "Closed");
}

#[test]
fn the_ime_composition_shows_until_committed() {
    let mut d = doc();
    let t = type_title(&mut d, "Kiln ");
    run(
        &mut d,
        D::SetComposition {
            text: "にほ".into(),
        },
    );
    assert_eq!(
        d.view
            .text
            .as_ref()
            .and_then(|e| e.composition.clone())
            .as_deref(),
        Some("にほ")
    );
    assert_eq!(text_of(&d, t), "Kiln ");
    run(
        &mut d,
        D::InsertText {
            text: "日本".into(),
        },
    );
    assert_eq!(text_of(&d, t), "Kiln 日本");
    assert!(
        d.view
            .text
            .as_ref()
            .is_some_and(|e| e.composition.is_none())
    );
}

#[test]
fn typing_with_a_shape_selected_replaces_its_text() {
    let mut d = doc();
    let id = shape_at(&mut d, 900.0, 500.0, 120.0, 60.0);
    run(&mut d, D::InsertText { text: "Hi".into() });
    run(&mut d, D::Escape);
    run(&mut d, D::InsertText { text: "Yo".into() });
    assert_eq!(text_of(&d, id), "Yo");
}

#[test]
fn typing_is_undone_word_by_word() {
    let mut d = doc();
    let t = type_title(&mut d, "Every ");
    run(&mut d, D::InsertText { text: "t".into() });
    run(&mut d, D::InsertText { text: "i".into() });
    assert!(d.undo());
    assert_eq!(text_of(&d, t), "Every ");
}

#[test]
fn a_text_box_grows_with_its_text() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertTextBox {
            rect: rect(100.0, 100.0, 200.0, 30.0),
        },
    );
    let id = d.view.shapes[0];
    for _ in 0..4 {
        run(&mut d, D::NewParagraph);
        run(
            &mut d,
            D::InsertText {
                text: "Line".into(),
            },
        );
    }
    assert!(bx(&d, id).h > 60.0, "{:?}", bx(&d, id));
}

#[test]
fn slide_number_field_goes_into_the_text() {
    let mut d = doc();
    let t = type_title(&mut d, "Slide ");
    run(&mut d, D::InsertSlideNumber);
    let body = d.pres.shape(t).and_then(|s| s.text()).expect("t");
    let p = body.paragraphs().next().expect("p");
    assert!(
        p.content
            .objects()
            .iter()
            .any(|o| matches!(o, kiln_doc::InlineObject::Field(f) if f.instr == "slidenum"))
    );
}

#[test]
fn find_selects_matches_across_slides_and_replace_all_replaces() {
    let mut d = doc();
    type_title(&mut d, "kiln one");
    run(&mut d, D::Escape);
    run(&mut d, D::NewSlide { layout: None });
    type_title(&mut d, "two kiln");
    run(&mut d, D::Escape);
    run(&mut d, D::Escape);
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    run(
        &mut d,
        D::Find {
            text: "Kiln".into(),
            match_case: false,
            whole_word: true,
            backward: false,
        },
    );
    assert_eq!(d.view.current, 0);
    assert_eq!(d.selected_text_string().as_deref(), Some("kiln"));
    run(
        &mut d,
        D::Find {
            text: "Kiln".into(),
            match_case: false,
            whole_word: true,
            backward: false,
        },
    );
    assert_eq!(d.view.current, 1);
    run(
        &mut d,
        D::Replace {
            text: "kiln".into(),
            with: "Kiln".into(),
            match_case: false,
            whole_word: false,
            all: true,
        },
    );
    assert_eq!(
        kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[0].id),
        "Kiln one"
    );
    assert_eq!(
        kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[1].id),
        "two Kiln"
    );
    run(
        &mut d,
        D::Find {
            text: "zzz".into(),
            match_case: false,
            whole_word: false,
            backward: false,
        },
    );
    assert!(d.find_done);
}

#[test]
fn replace_fonts_changes_every_run() {
    let mut d = doc();
    type_title(&mut d, "Kiln");
    ed(
        &mut d,
        E::FontFamily {
            family: "Georgia".into(),
        },
    );
    run(&mut d, D::SelectAll);
    ed(
        &mut d,
        E::FontFamily {
            family: "Georgia".into(),
        },
    );
    run(
        &mut d,
        D::ReplaceFont {
            from: "Georgia".into(),
            to: "Figtree".into(),
        },
    );
    let r = first_run(&d, title(&d));
    assert_eq!(r.fonts.and_then(|f| f.ascii).as_deref(), Some("Figtree"));
}

// ── formatting ──────────────────────────────────────────────────────────────────────────

#[test]
fn bold_applies_to_all_text_of_the_selected_shape() {
    let mut d = doc();
    let t = type_title(&mut d, "Kiln");
    run(&mut d, D::Escape);
    let on = d.effective_run().and_then(|r| r.bold) == Some(true);
    ed(&mut d, E::Bold);
    assert_eq!(first_run(&d, t).bold, Some(!on));
}

#[test]
fn italic_applies_to_the_selection_only() {
    let mut d = doc();
    let t = type_title(&mut d, "Open studio");
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::WordBackward,
            extend: true,
        },
    );
    ed(&mut d, E::Italic);
    let body = d.pres.shape(t).and_then(|s| s.text()).expect("t");
    let p = body.paragraphs().next().expect("p");
    assert_ne!(p.content.props_at(0).and_then(|r| r.italic), Some(true));
    assert_eq!(p.content.props_at(6).and_then(|r| r.italic), Some(true));
}

#[test]
fn font_size_grow_and_shrink() {
    let mut d = doc();
    let t = type_title(&mut d, "Kiln");
    run(&mut d, D::SelectAll);
    ed(&mut d, E::FontSize { size: 18.0 });
    assert_eq!(first_run(&d, t).size, Some(36));
    ed(&mut d, E::GrowFont { one_point: false });
    assert_eq!(first_run(&d, t).size, Some(40));
    ed(&mut d, E::ShrinkFont { one_point: true });
    assert_eq!(first_run(&d, t).size, Some(38));
    assert!(
        d.execute(&Command::Suite(SuiteCommand::Edit(E::FontSize {
            size: -3.0
        })))
        .is_err()
    );
}

#[test]
fn colour_highlight_and_clear_formatting() {
    let mut d = doc();
    let t = type_title(&mut d, "Kiln");
    run(&mut d, D::SelectAll);
    ed(
        &mut d,
        E::FontColor {
            color: Some("C8352B".into()),
        },
    );
    ed(
        &mut d,
        E::Highlight {
            color: Some("FFE066".into()),
        },
    );
    let r = first_run(&d, t);
    assert!(r.color.is_some() && r.highlight.is_some());
    ed(&mut d, E::ClearFormatting);
    let r = first_run(&d, t);
    assert!(r.color.is_none() && r.highlight.is_none());
}

#[test]
fn change_case_keeps_the_formatting() {
    let mut d = doc();
    let t = type_title(&mut d, "open studio");
    run(&mut d, D::SelectAll);
    ed(&mut d, E::Bold);
    ed(
        &mut d,
        E::ChangeCase {
            case: Some(TextCase::Title),
        },
    );
    assert_eq!(text_of(&d, t), "Open Studio");
    ed(
        &mut d,
        E::ChangeCase {
            case: Some(TextCase::Upper),
        },
    );
    assert_eq!(text_of(&d, t), "OPEN STUDIO");
    assert!(first_run(&d, t).bold.is_some());
}

#[test]
fn subscript_and_superscript_toggle() {
    let mut d = doc();
    let t = type_title(&mut d, "H2O");
    run(&mut d, D::SelectAll);
    ed(&mut d, E::Superscript);
    assert_eq!(
        first_run(&d, t).vert_align,
        Some(kiln_doc::st::VertAlign::Superscript)
    );
    ed(&mut d, E::Superscript);
    assert_eq!(
        first_run(&d, t).vert_align,
        Some(kiln_doc::st::VertAlign::Baseline)
    );
}

#[test]
fn bullets_numbering_alignment_and_spacing() {
    let mut d = doc();
    let t = type_title(&mut d, "List");
    ed(&mut d, E::Bullets { glyph: None });
    let deck = |d: &DeckDoc| d.effective_paragraph().expect("para").deck;
    assert!(matches!(deck(&d).bullet, Some(Bullet::Char(_))));
    ed(&mut d, E::Numbering { style: None });
    assert!(matches!(deck(&d).bullet, Some(Bullet::AutoNumber { .. })));
    ed(&mut d, E::AlignCenter);
    assert_eq!(
        d.effective_paragraph().and_then(|p| p.para.alignment),
        Some(kiln_doc::st::Alignment::Center)
    );
    ed(&mut d, E::LineSpacing { spacing: 1.5 });
    assert_eq!(
        deck(&d).line_spacing,
        Some(kiln_deck::TextSpacing::Percent(150_000))
    );
    let _ = t;
}

#[test]
fn shape_fill_outline_and_effects() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::SetFill {
            fill: DeckFill::Solid {
                color: "accent2".into(),
                brightness: Some(40.0),
                transparency: None,
            },
        },
    );
    assert!(matches!(
        d.pres.resolved_props(id).and_then(|p| p.fill),
        Some(Fill::Solid(_))
    ));
    run(
        &mut d,
        D::SetFill {
            fill: DeckFill::Gradient {
                from: "E2622A".into(),
                to: "F7B500".into(),
                angle: 90.0,
            },
        },
    );
    assert!(matches!(
        d.pres.resolved_props(id).and_then(|p| p.fill),
        Some(Fill::Gradient(_))
    ));
    run(
        &mut d,
        D::SetOutline {
            outline: DeckOutline::Weight { points: 3.0 },
        },
    );
    run(
        &mut d,
        D::SetOutline {
            outline: DeckOutline::Dash {
                dash: "dash".into(),
            },
        },
    );
    run(
        &mut d,
        D::SetOutline {
            outline: DeckOutline::Arrows {
                head: "none".into(),
                tail: "triangle".into(),
            },
        },
    );
    let line = d
        .pres
        .resolved_props(id)
        .and_then(|p| p.line)
        .expect("line");
    assert_eq!(line.width, Some(emu(4.0)));
    assert!(line.dash.is_some() && line.tail.is_some() && line.head.is_none());
    run(
        &mut d,
        D::SetEffect {
            effect: DeckEffect::Shadow {
                distance: 3.0,
                blur: 4.0,
                angle: 45.0,
            },
        },
    );
    assert!(
        d.pres
            .resolved_props(id)
            .and_then(|p| p.effects)
            .and_then(|e| e.outer_shadow)
            .is_some()
    );
    run(
        &mut d,
        D::SetEffect {
            effect: DeckEffect::None,
        },
    );
    assert!(
        d.pres
            .resolved_props(id)
            .and_then(|p| p.effects)
            .and_then(|e| e.outer_shadow)
            .is_none()
    );
}

#[test]
fn quick_styles_use_the_theme() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::SetFill {
            fill: DeckFill::None,
        },
    );
    run(&mut d, D::QuickStyle { index: 7 });
    let p = d.pres.resolved_props(id).expect("p");
    assert!(p.fill.is_none());
    assert!(p.style.is_some_and(|s| s.fill.color.is_some()));
}

#[test]
fn text_box_properties() {
    let mut d = doc();
    let id = shape_at(&mut d, 100.0, 100.0, 200.0, 100.0);
    run(
        &mut d,
        D::AlignText {
            anchor: DeckAnchor::Bottom,
        },
    );
    run(
        &mut d,
        D::TextDirection {
            direction: DeckTextDirection::Rotate90,
        },
    );
    run(
        &mut d,
        D::Columns {
            count: 2,
            spacing: 12.0,
        },
    );
    run(
        &mut d,
        D::Autofit {
            mode: DeckAutofit::Shrink,
        },
    );
    let b = d.pres.resolved_body(id).expect("body");
    assert_eq!(b.anchor, TextAnchor::Bottom);
    assert_eq!(b.direction, TextDirection::Rotate90);
    assert_eq!(b.columns, 2);
    assert!(matches!(b.autofit, Autofit::Shrink { .. }));
}

#[test]
fn character_spacing_and_text_shadow() {
    let mut d = doc();
    let t = type_title(&mut d, "Kiln");
    run(&mut d, D::SelectAll);
    run(&mut d, D::CharacterSpacing { points: 3.0 });
    assert_eq!(first_run(&d, t).spacing, Some(60));
    run(&mut d, D::TextShadow { on: None });
    assert_eq!(first_run(&d, t).shadow, Some(true));
}

#[test]
fn the_format_painter_copies_a_shapes_look() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    run(
        &mut d,
        D::SetFill {
            fill: DeckFill::Solid {
                color: "1D8A60".into(),
                brightness: None,
                transparency: None,
            },
        },
    );
    let b = shape_at(&mut d, 300.0, 100.0, 50.0, 50.0);
    run(&mut d, D::SelectShapes { ids: vec![a] });
    run(&mut d, D::CopyFormat);
    run(&mut d, D::SelectShapes { ids: vec![b] });
    run(&mut d, D::PasteFormat);
    assert_eq!(
        d.pres.resolved_props(b).and_then(|p| p.fill),
        d.pres.resolved_props(a).and_then(|p| p.fill)
    );
}

// ── design ──────────────────────────────────────────────────────────────────────────────

#[test]
fn themes_and_variants_apply() {
    let mut d = doc();
    run(
        &mut d,
        D::ApplyTheme {
            name: "Cobalt".into(),
        },
    );
    assert_eq!(d.pres.masters[0].theme.name, "Cobalt");
    run(&mut d, D::ApplyVariant { index: 1 });
    assert!(
        fails(
            &mut d,
            D::ApplyTheme {
                name: "Nope".into()
            }
        )
        .contains("Nope")
    );
}

#[test]
fn slide_size_changes_to_standard() {
    let mut d = doc();
    run(
        &mut d,
        D::SlideSize {
            size: DeckSlideSize::Standard,
            mode: DeckScaleMode::EnsureFit,
        },
    );
    assert_eq!(d.pres.slide_size.cx, 9_144_000);
    assert_eq!(d.deck_state().size_kind, "standard");
}

#[test]
fn format_background_for_every_slide_and_hide_graphics() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(
        &mut d,
        D::SetBackground {
            fill: Some(DeckFill::Solid {
                color: "FFF7EE".into(),
                brightness: None,
                transparency: None,
            }),
            all: true,
        },
    );
    assert!(d.pres.slides.iter().all(|s| s.background.is_some()));
    assert_eq!(d.deck_state().background.as_deref(), Some("FFF7EE"));
    run(&mut d, D::HideBackgroundGraphics { hide: None });
    assert!(!d.pres.slides[d.view.current].show_master_shapes);
    run(
        &mut d,
        D::SetBackground {
            fill: None,
            all: false,
        },
    );
    assert!(d.pres.slides[d.view.current].background.is_none());
}

#[test]
fn header_and_footer_add_slide_numbers_but_not_on_the_title_slide() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(
        &mut d,
        D::SetHeaderFooter {
            date: false,
            fixed_date: None,
            slide_number: true,
            footer: Some("Kiln Co-op".into()),
            not_on_title: true,
            all: true,
        },
    );
    let has = |s: &kiln_deck::Slide, k: kiln_deck::PlaceholderType| {
        s.shapes
            .iter()
            .any(|x| x.placeholder.is_some_and(|p| p.kind == k))
    };
    assert!(!has(
        &d.pres.slides[0],
        kiln_deck::PlaceholderType::SlideNumber
    ));
    assert!(has(
        &d.pres.slides[1],
        kiln_deck::PlaceholderType::SlideNumber
    ));
    assert!(has(&d.pres.slides[1], kiln_deck::PlaceholderType::Footer));
    assert_eq!(
        d.deck_state().header_footer.footer.as_deref(),
        Some("Kiln Co-op")
    );
}

// ── clipboard ───────────────────────────────────────────────────────────────────────────

#[test]
fn copy_and_paste_objects() {
    let mut d = doc();
    shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    ed(&mut d, E::Copy);
    let data = d.take_clipboard().expect("copied");
    assert!(data.kiln.is_some());
    let n = slide_shapes(&d).len();
    ed(&mut d, E::Paste { data: Some(data) });
    assert_eq!(slide_shapes(&d).len(), n + 1);
    assert!(
        (bx(&d, d.view.shapes[0]).x - 112.0).abs() < 0.01,
        "pasted over itself, offset"
    );
}

#[test]
fn cut_and_paste_a_slide() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    type_title(&mut d, "Second");
    run(&mut d, D::Escape);
    run(&mut d, D::Escape);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    ed(&mut d, E::Cut);
    assert_eq!(d.pres.slides.len(), 1);
    let data = d.take_clipboard().expect("copied");
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    ed(&mut d, E::Paste { data: Some(data) });
    assert_eq!(d.pres.slides.len(), 2);
    assert_eq!(
        kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[1].id),
        "Second"
    );
}

#[test]
fn plain_text_pastes_into_the_text_or_a_new_text_box() {
    let mut d = doc();
    let t = type_title(&mut d, "A ");
    ed(
        &mut d,
        E::Paste {
            data: Some(ClipboardData {
                text: "pasted".into(),
                html: None,
                kiln: None,
            }),
        },
    );
    assert_eq!(text_of(&d, t), "A pasted");
    run(&mut d, D::Escape);
    run(&mut d, D::Escape);
    let n = slide_shapes(&d).len();
    ed(
        &mut d,
        E::PasteTextOnly {
            data: Some(ClipboardData {
                text: "box".into(),
                html: None,
                kiln: None,
            }),
        },
    );
    assert_eq!(slide_shapes(&d).len(), n + 1);
    assert_eq!(text_of(&d, d.view.shapes[0]), "box");
}

#[test]
fn copied_text_keeps_its_formatting() {
    let mut d = doc();
    let t = type_title(&mut d, "Bold");
    run(&mut d, D::SelectAll);
    ed(&mut d, E::FontSize { size: 60.0 });
    ed(&mut d, E::Copy);
    let data = d.take_clipboard().expect("copied");
    run(
        &mut d,
        D::MoveCaret {
            motion: CaretMotion::DocumentEnd,
            extend: false,
        },
    );
    ed(&mut d, E::Paste { data: Some(data) });
    assert_eq!(text_of(&d, t), "BoldBold");
    let body = d.pres.shape(t).and_then(|s| s.text()).expect("t");
    let p = body.paragraphs().next().expect("p");
    assert_eq!(p.content.props_at(5).and_then(|r| r.size), Some(120));
}

// ── tables ──────────────────────────────────────────────────────────────────────────────

#[test]
fn table_rows_columns_and_styles() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 2, cols: 2 });
    let id = d.view.shapes[0];
    let table = |d: &DeckDoc| match &d.pres.shape(id).expect("t").kind {
        ShapeKind::Table(t) => t.clone(),
        _ => panic!(),
    };
    ed(&mut d, E::TableInsertRowBelow);
    ed(&mut d, E::TableInsertColumnRight);
    assert_eq!((table(&d).rows.len(), table(&d).grid.len()), (3, 3));
    ed(
        &mut d,
        E::TableDelete {
            what: TableDeleteTarget::Columns,
        },
    );
    assert_eq!(table(&d).grid.len(), 2);
    ed(&mut d, E::TableHeaderRow);
    assert!(!table(&d).look.first_row);
    ed(
        &mut d,
        E::TableStyle {
            style: "Grid".into(),
        },
    );
    assert_eq!(
        table(&d).style_id.as_deref(),
        Some(kiln_deck_layout::table::GRID_STYLE)
    );
    ed(
        &mut d,
        E::TableDelete {
            what: TableDeleteTarget::Table,
        },
    );
    assert!(d.pres.shape(id).is_none());
}

// ── the chrome's state ──────────────────────────────────────────────────────────────────

#[test]
fn state_reports_selection_contexts_and_format() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertPicture {
            name: "k.png".into(),
            bytes: png(10, 10),
            rect: None,
        },
    );
    let st = d.deck_state();
    assert_eq!(st.contexts, vec!["picture".to_owned()]);
    assert!(st.selection_box.is_some());
    type_title(&mut d, "Kiln");
    let st = d.deck_state();
    assert!(st.editing && st.caret.is_some());
    let f = st.format.expect("format");
    assert!(f.font_size > 30.0, "{f:?}");
    assert_eq!(st.shapes.len(), slide_shapes(&d).len());
}

#[test]
fn thumbnail_versions_change_only_for_edited_slides() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    let v0: Vec<u64> = d.deck_state().slides.iter().map(|s| s.version).collect();
    type_title(&mut d, "Edited");
    let v1: Vec<u64> = d.deck_state().slides.iter().map(|s| s.version).collect();
    assert_eq!(v0[0], v1[0]);
    assert_ne!(v0[1], v1[1]);
    run(
        &mut d,
        D::ApplyTheme {
            name: "Jade".into(),
        },
    );
    let v2: Vec<u64> = d.deck_state().slides.iter().map(|s| s.version).collect();
    assert_ne!(v1[0], v2[0]);
}

#[test]
fn view_toggles() {
    let mut d = doc();
    assert!(d.deck_state().notes_visible, "the notes pane starts shown");
    run(&mut d, D::ToggleGuides);
    run(&mut d, D::ToggleGridlines);
    run(&mut d, D::ToggleNotes);
    let st = d.deck_state();
    assert!(st.guides && st.gridlines && !st.notes_visible);
}

#[test]
fn compatibility_lists_nothing_for_a_new_deck() {
    let d = doc();
    assert!(
        d.compat_report(kiln_protocol::FileFormat::Pptx)
            .iter()
            .all(|i| i.severity != kiln_protocol::CompatSeverity::Loss)
    );
}

#[test]
fn f4_repeats_the_last_formatting_on_the_new_selection() {
    let mut d = doc();
    let a = shape_at(&mut d, 100.0, 100.0, 50.0, 50.0);
    let b = shape_at(&mut d, 400.0, 100.0, 50.0, 50.0);
    run(&mut d, D::SelectShapes { ids: vec![a] });
    run(
        &mut d,
        D::SetFill {
            fill: DeckFill::Solid {
                color: "E2622A".into(),
                brightness: None,
                transparency: None,
            },
        },
    );
    // Selecting another shape is not repeated; F4 fills it too.
    run(&mut d, D::SelectShapes { ids: vec![b] });
    assert_eq!(ed(&mut d, E::Repeat), Change::Content);
    let solid = |d: &DeckDoc, id| {
        matches!(
            d.pres.resolved_props(id).and_then(|p| p.fill),
            Some(Fill::Solid(_))
        )
    };
    assert!(solid(&d, a) && solid(&d, b));
    // Nothing done yet: F4 does nothing.
    let mut fresh = doc();
    assert_eq!(ed(&mut fresh, E::Repeat), Change::None);
}

#[test]
fn insert_date_and_time_type_at_the_caret() {
    let mut d = doc();
    // 2026-10-14 17:46:40 UTC.
    d.now_ms = 1_792_000_000_000;
    let t = type_title(&mut d, "Open ");
    ed(&mut d, E::InsertDate);
    run(
        &mut d,
        D::InsertText {
            text: " at ".into(),
        },
    );
    ed(&mut d, E::InsertTime);
    assert_eq!(text_of(&d, t), "Open 14/10/2026 at 17:46");
}

#[test]
fn tables_and_charts_take_the_empty_content_placeholder() {
    let mut d = doc();
    // A Title and Content slide: its body placeholder is where a table goes.
    run(&mut d, D::NewSlide { layout: None });
    let body = d.pres.slides[d.view.current]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| !p.kind.is_title()))
        .map(|s| s.id)
        .expect("body");
    let slot = bx(&d, body);
    run(&mut d, D::InsertTable { rows: 2, cols: 3 });
    let table = d.view.shapes[0];
    let r = bx(&d, table);
    assert!(
        (r.x - slot.x).abs() < 1.0 && (r.y - slot.y).abs() < 1.0 && (r.w - slot.w).abs() < 1.0,
        "{r:?} {slot:?}"
    );
    assert!(d.pres.shape(body).is_none(), "the placeholder makes way");
    // Undo brings the placeholder back in one step.
    assert!(d.undo());
    assert!(d.pres.shape(body).is_some());
    // With no placeholder free, a chart avoids what is on the slide.
    let mut c = doc();
    let blank = blank_layout(&c);
    run(&mut c, D::SetLayout { layout: blank });
    let s = shape_at(&mut c, 320.0, 180.0, 640.0, 360.0);
    c.execute(&Command::Suite(SuiteCommand::ChartInsert { kind: None }))
        .expect("chart");
    let chart = c.view.shapes[0];
    let (a, b) = (bx(&c, s), bx(&c, chart));
    let ix = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
    let iy = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
    assert!(ix * iy < a.w * a.h * 0.75, "{a:?} {b:?}");
}

fn blank_layout(d: &DeckDoc) -> NodeId {
    d.pres.masters[0]
        .layouts
        .iter()
        .find(|l| l.name == "Blank")
        .map(|l| l.id)
        .expect("blank layout")
}

#[test]
fn tab_goes_through_the_cells_and_adds_a_row_after_the_last() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 1, cols: 2 });
    let id = d.view.shapes[0];
    run(&mut d, D::InsertText { text: "Q1".into() });
    run(&mut d, D::Tab { shift: false });
    run(&mut d, D::InsertText { text: "Q2".into() });
    run(&mut d, D::Tab { shift: false });
    run(&mut d, D::InsertText { text: "Q3".into() });
    run(&mut d, D::Tab { shift: true });
    let ShapeKind::Table(t) = &d.pres.shape(id).expect("table").kind else {
        panic!()
    };
    assert_eq!(t.rows.len(), 2, "Tab in the last cell adds a row");
    let cell = |r, c| t.cell(r, c).map(|x| x.text.text()).unwrap_or_default();
    assert_eq!(
        (cell(0, 0), cell(0, 1), cell(1, 0)),
        ("Q1".into(), "Q2".into(), "Q3".into())
    );
    assert!(matches!(
        d.view.text.as_ref().map(|t| t.target),
        Some(kiln_deck::ops::TextTarget::Cell { row: 0, col: 1, .. })
    ));
}

#[test]
fn double_click_on_a_placeholder_selects_the_word_and_triple_click_the_paragraph() {
    let mut d = doc();
    let t = type_title(&mut d, "Every ticket fires");
    run(&mut d, D::Escape);
    run(&mut d, D::Escape);
    let r = bx(&d, t);
    let (x, y) = (r.x + 30.0, r.y + r.h / 2.0);
    let click = |d: &mut DeckDoc, clicks: u8| {
        run(
            d,
            D::PointerDown {
                x,
                y,
                shift: false,
                ctrl: false,
                alt: false,
                clicks,
            },
        );
        run(
            d,
            D::PointerUp {
                x,
                y,
                shift: false,
                ctrl: false,
                alt: false,
            },
        );
    };
    click(&mut d, 1);
    click(&mut d, 2);
    run(
        &mut d,
        D::InsertText {
            text: "Each ".into(),
        },
    );
    assert_eq!(text_of(&d, t), "Each ticket fires");
    click(&mut d, 1);
    click(&mut d, 2);
    click(&mut d, 3);
    run(
        &mut d,
        D::InsertText {
            text: "Clay".into(),
        },
    );
    assert_eq!(text_of(&d, t), "Clay");
}
