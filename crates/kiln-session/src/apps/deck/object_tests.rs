// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paste as Picture, icons, code blocks, symbols, crop handles and Crop to, table cell
//! selection with merge, split and margins, and connectors drawn onto connection sites — each
//! checked through a `.pptx` save and reopen.

use kiln_common::NodeId;
use kiln_deck::{Geometry, Presentation, ShapeKind};
use kiln_deck_layout::geom::rect_px;
use kiln_protocol::commands::deck_content::{DeckCellRange, DeckCropFit, DeckListDiagram};
use kiln_protocol::commands::{DeckCommand as D, DeckRect, EditCommand as E};
use kiln_protocol::{Command, FileFormat, SuiteCommand};

use super::DeckDoc;
use crate::model::DocModel;

fn doc() -> DeckDoc {
    DeckDoc::new(Presentation::new(), None, None)
}

fn run(d: &mut DeckDoc, c: D) {
    d.execute(&Command::Deck(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"));
}

fn ed(d: &mut DeckDoc, e: E) {
    d.execute(&Command::Suite(SuiteCommand::Edit(e.clone())))
        .unwrap_or_else(|err| panic!("{e:?}: {err}"));
}

fn reopen(d: &DeckDoc) -> DeckDoc {
    let bytes = d.save(FileFormat::Pptx).expect("save");
    let read = kiln_io_pptx::read_pptx(&bytes).expect("open");
    let (pres, original) = super::save::Original::from_read(read, &bytes);
    DeckDoc::new(pres, Some(original), None)
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([40, 90, 200, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

fn press(d: &mut DeckDoc, from: (f32, f32), to: (f32, f32), shift: bool) {
    run(
        d,
        D::PointerDown {
            x: from.0,
            y: from.1,
            shift,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    );
    for t in [0.5, 1.0] {
        run(
            d,
            D::PointerMove {
                x: from.0 + (to.0 - from.0) * t,
                y: from.1 + (to.1 - from.1) * t,
                shift,
                ctrl: false,
                alt: false,
            },
        );
    }
    run(
        d,
        D::PointerUp {
            x: to.0,
            y: to.1,
            shift,
            ctrl: false,
            alt: false,
        },
    );
}

fn shapes(d: &DeckDoc) -> &[kiln_deck::Shape] {
    &d.pres.slides[0].shapes
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Option<DeckRect> {
    Some(DeckRect {
        x,
        y,
        width,
        height,
    })
}

#[test]
fn objects_paste_as_a_picture_where_they_were() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertShape {
            preset: "ellipse".into(),
            rect: rect(100.0, 120.0, 200.0, 100.0),
        },
    );
    ed(&mut d, E::Copy);
    run(&mut d, D::PastePicture { data: None });
    let pic = d.view.shapes[0];
    let s = d.pres.shape(pic).expect("pasted");
    let ShapeKind::Picture(p) = &s.kind else {
        panic!("not a picture: {:?}", s.kind)
    };
    let bytes = &d.pres.parts[&p.media].bytes;
    assert!(bytes.starts_with(b"\x89PNG"));
    let b = rect_px(&s.xfrm.expect("xfrm"));
    // The ellipse and its outline.
    assert!(
        (b.x - 100.0).abs() < 3.0 && (b.w - 200.0).abs() < 6.0,
        "{b:?}"
    );
    let again = reopen(&d);
    assert!(
        again.pres.slides[0]
            .shapes
            .iter()
            .any(|s| matches!(s.kind, ShapeKind::Picture(_)))
    );
}

#[test]
fn text_pastes_as_a_picture_in_the_middle() {
    let mut d = doc();
    run(
        &mut d,
        D::PastePicture {
            data: Some(kiln_protocol::commands::ClipboardData {
                text: "Quarterly results".into(),
                html: None,
                kiln: None,
            }),
        },
    );
    let s = d.pres.shape(d.view.shapes[0]).expect("pasted");
    assert!(matches!(s.kind, ShapeKind::Picture(_)));
}

#[test]
fn an_icon_is_a_shape_with_its_strokes_as_outline() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertIcon {
            name: "Heart".into(),
            paths: vec![
                "M19 14c1.5-1.5 3-3.2 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.8 0-3 .5-4.5 2-1.5-1.5-2.7-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4 3 5.5l7 7Z".into(),
            ],
        },
    );
    let id = d.view.shapes[0];
    let check = |d: &DeckDoc, id: NodeId| {
        let s = d.pres.shape(id).expect("icon");
        let ShapeKind::Geometry(a) = &s.kind else {
            panic!("not a shape")
        };
        let Some(Geometry::Custom(g)) = &a.props.geometry else {
            panic!("no custom geometry")
        };
        assert!(g.path[0].cmds.len() > 5);
        assert!(a.props.line.as_ref().and_then(|l| l.width).is_some());
        assert_eq!(s.alt_text.as_deref(), Some("Heart"));
    };
    check(&d, id);
    let again = reopen(&d);
    let id = again.pres.slides[0]
        .shapes
        .iter()
        .find(|s| s.name.contains("Heart"))
        .map(|s| s.id)
        .expect("the icon is kept");
    check(&again, id);
    assert!(
        d.execute(&Command::Deck(D::InsertIcon {
            name: "x".into(),
            paths: vec!["not a path".into()],
        }))
        .is_err()
    );
}

#[test]
fn a_code_block_is_highlighted_monospaced_text_that_round_trips() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertCodeBlock {
            code: "fn main() {\n    let n = 42; // answer\n}".into(),
            language: Some("rust".into()),
        },
    );
    let check = |d: &DeckDoc| {
        let s = shapes(d)
            .iter()
            .find(|s| s.name.starts_with("Code Block"))
            .expect("code block");
        assert!(s.name.ends_with("(rust)"));
        let body = s.text().expect("text");
        assert_eq!(body.text(), "fn main() {\n    let n = 42; // answer\n}");
        let p = body.paragraphs().next().expect("line");
        let first = p.content.props_at(0).cloned().unwrap_or_default();
        let latin = first.fonts.as_ref().and_then(|f| f.ascii.clone());
        assert_eq!(latin.as_deref(), Some("Consolas"));
        // The keyword and the name differ in colour.
        let name = p.content.props_at(4).cloned().unwrap_or_default();
        assert_ne!(first.color, name.color);
    };
    check(&d);
    check(&reopen(&d));
}

#[test]
fn a_symbol_goes_in_at_the_caret_in_its_font() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertSymbol {
            text: "→".into(),
            font: Some("Segoe UI Symbol".into()),
        },
    );
    let id = d.view.shapes[0];
    let s = d.pres.shape(id).expect("text box");
    let body = s.text().expect("text");
    assert_eq!(body.text(), "→");
    let p = body.paragraphs().next().expect("para");
    let font = |at: usize| {
        p.content
            .props_at(at)
            .and_then(|r| r.fonts.as_ref())
            .and_then(|f| f.ascii.clone())
    };
    assert_eq!(font(0).as_deref(), Some("Segoe UI Symbol"));
    // One undo step.
    assert!(d.undo());
    assert!(d.pres.shape(id).is_none());
}

fn picture(d: &mut DeckDoc) -> NodeId {
    run(
        d,
        D::InsertPicture {
            name: "wide.png".into(),
            bytes: png(400, 200),
            rect: rect(100.0, 100.0, 400.0, 200.0),
        },
    );
    d.view.shapes[0]
}

fn crop(d: &DeckDoc, id: NodeId) -> kiln_deck::Crop {
    match &d.pres.shape(id).expect("pic").kind {
        ShapeKind::Picture(p) => p.crop,
        _ => panic!("not a picture"),
    }
}

#[test]
fn crop_handles_crop_the_frame_and_a_drag_inside_moves_the_picture() {
    let mut d = doc();
    let id = picture(&mut d);
    run(&mut d, D::CropMode { on: true });
    assert!(d.deck_state().cropping);
    // The left handle 100 px in: a quarter of the picture is cropped away.
    press(&mut d, (100.0, 200.0), (200.0, 200.0), false);
    let c = crop(&d, id);
    assert_eq!((c.l, c.r), (25_000, 0));
    let b = rect_px(&d.xfrm_of(id).expect("xfrm"));
    assert!((b.x - 200.0).abs() < 0.5 && (b.w - 300.0).abs() < 0.5);
    // Inside the frame: the picture moves 40 px left behind it.
    press(&mut d, (300.0, 200.0), (260.0, 200.0), false);
    let c = crop(&d, id);
    assert_eq!((c.l, c.r), (35_000, -10_000));
    let b2 = rect_px(&d.xfrm_of(id).expect("xfrm"));
    assert!((b2.x - b.x).abs() < 0.5);
    assert!(d.undo());
    assert_eq!(crop(&d, id).l, 25_000);
    // Esc ends cropping; the crop is kept through a save.
    run(&mut d, D::Escape);
    assert!(!d.deck_state().cropping);
    let again = reopen(&d);
    let pic = again.pres.slides[0]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Picture(_)))
        .map(|s| s.id)
        .expect("picture");
    assert_eq!(crop(&again, pic).l, 25_000);
}

#[test]
fn crop_to_an_aspect_ratio_fill_and_fit() {
    let mut d = doc();
    let id = picture(&mut d);
    run(
        &mut d,
        D::CropTo {
            fit: DeckCropFit::Aspect {
                width: 1,
                height: 1,
            },
        },
    );
    let b = rect_px(&d.xfrm_of(id).expect("xfrm"));
    assert!((b.w - 200.0).abs() < 0.5 && (b.h - 200.0).abs() < 0.5 && (b.x - 200.0).abs() < 0.5);
    let c = crop(&d, id);
    assert_eq!((c.l, c.r, c.t), (25_000, 25_000, 0));
    // Fit shows the whole 2:1 picture in the square frame; Fill crops it to the frame.
    run(
        &mut d,
        D::CropTo {
            fit: DeckCropFit::Fit,
        },
    );
    let c = crop(&d, id);
    assert_eq!((c.l, c.t, c.b), (0, -50_000, -50_000));
    run(
        &mut d,
        D::CropTo {
            fit: DeckCropFit::Fill,
        },
    );
    assert_eq!(crop(&d, id).l, 25_000);
    // Crop to Shape is Change Shape on the picture.
    run(
        &mut d,
        D::ChangeShape {
            preset: "ellipse".into(),
        },
    );
    let again = reopen(&d);
    let s = again.pres.slides[0]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Picture(_)))
        .expect("picture");
    let ShapeKind::Picture(p) = &s.kind else {
        panic!()
    };
    assert_eq!(
        p.props.geometry.as_ref().and_then(Geometry::preset_name),
        Some("ellipse")
    );
}

fn table(d: &DeckDoc, id: NodeId) -> kiln_deck::Table {
    match &d.pres.shape(id).expect("table").kind {
        ShapeKind::Table(t) => t.clone(),
        _ => panic!("not a table"),
    }
}

#[test]
fn select_table_selects_every_cell() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 3, cols: 2 });
    run(&mut d, D::InsertText { text: "A".into() });
    run(
        &mut d,
        D::SelectCells {
            range: DeckCellRange {
                row: 0,
                col: 0,
                rows: 0,
                cols: 0,
            },
        },
    );
    assert_eq!(
        d.deck_state().cells,
        Some(DeckCellRange {
            row: 0,
            col: 0,
            rows: 3,
            cols: 2
        })
    );
}

#[test]
fn selected_cells_merge_split_and_take_margins() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 3, cols: 3 });
    let id = d.view.shapes[0];
    run(&mut d, D::InsertText { text: "A".into() });
    run(
        &mut d,
        D::SelectCells {
            range: DeckCellRange {
                row: 0,
                col: 0,
                rows: 2,
                cols: 2,
            },
        },
    );
    assert_eq!(
        d.deck_state().cells,
        Some(DeckCellRange {
            row: 0,
            col: 0,
            rows: 2,
            cols: 2
        })
    );
    run(
        &mut d,
        D::CellMargins {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
    let t = table(&d, id);
    assert_eq!(t.cell(1, 1).expect("c").props.margins, [Some(0); 4]);
    assert_eq!(t.cell(2, 2).expect("c").props.margins, [None; 4]);
    ed(&mut d, E::TableMergeCells);
    let t = table(&d, id);
    let o = t.cell(0, 0).expect("c");
    assert_eq!(
        (o.grid_span, o.row_span, o.text.text().as_str()),
        (2, 2, "A")
    );
    assert!(d.deck_state().cells.is_none());
    let again = reopen(&d);
    let tid = again.pres.slides[0]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Table(_)))
        .map(|s| s.id)
        .expect("table");
    let t = table(&again, tid);
    assert_eq!(
        t.cell(0, 0).map(|c| (c.grid_span, c.row_span)),
        Some((2, 2))
    );
    assert!(t.cell(1, 1).is_some_and(|c| c.h_merge && c.v_merge));
    ed(&mut d, E::TableSplitCells { rows: 1, cols: 1 });
    assert_eq!(table(&d, id).cell(0, 0).map(|c| c.grid_span), Some(1));
}

#[test]
fn dragging_across_cells_selects_them_and_shading_fills_them() {
    let mut d = doc();
    run(&mut d, D::InsertTable { rows: 2, cols: 3 });
    let id = d.view.shapes[0];
    let b = rect_px(&d.xfrm_of(id).expect("xfrm"));
    let cell = |r: f32, c: f32| (b.x + b.w * (c + 0.5) / 3.0, b.y + b.h * (r + 0.5) / 2.0);
    // Click into the first cell, then drag from it to the second row's second cell.
    press(&mut d, cell(0.0, 0.0), cell(0.0, 0.0), false);
    press(&mut d, cell(0.0, 0.0), cell(1.0, 1.0), false);
    assert_eq!(
        d.deck_state().cells,
        Some(DeckCellRange {
            row: 0,
            col: 0,
            rows: 2,
            cols: 2
        })
    );
    ed(
        &mut d,
        E::TableShading {
            color: Some("FF0000".into()),
        },
    );
    let t = table(&d, id);
    assert!(t.cell(1, 1).is_some_and(|c| c.props.fill.is_some()));
    assert!(t.cell(1, 2).is_some_and(|c| c.props.fill.is_none()));
}

#[test]
fn a_drawn_connector_attaches_to_the_sites_it_lands_on() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertShape {
            preset: "rect".into(),
            rect: rect(100.0, 100.0, 100.0, 100.0),
        },
    );
    let a = d.view.shapes[0];
    run(
        &mut d,
        D::InsertShape {
            preset: "rect".into(),
            rect: rect(500.0, 300.0, 100.0, 100.0),
        },
    );
    let b = d.view.shapes[0];
    run(
        &mut d,
        D::ArmInsert {
            preset: Some("straightConnector1".into()),
        },
    );
    // From near A's right side to near B's left side.
    press(&mut d, (203.0, 148.0), (497.0, 352.0), false);
    let c = d.view.shapes[0];
    let conn = |d: &DeckDoc, id: NodeId| match &d.pres.shape(id).expect("c").kind {
        ShapeKind::Connector(k) => (k.start, k.end),
        _ => panic!("not a connector"),
    };
    let (s, e) = conn(&d, c);
    assert_eq!(s.map(|s| s.shape), Some(a));
    assert_eq!(e.map(|s| s.shape), Some(b));
    let x = rect_px(&d.xfrm_of(c).expect("xfrm"));
    assert!(
        (x.x - 200.0).abs() < 0.5 && (x.y - 150.0).abs() < 0.5,
        "{x:?}"
    );
    // Moving B drags the end along.
    run(&mut d, D::SelectShapes { ids: vec![b] });
    press(&mut d, (550.0, 350.0), (550.0, 450.0), true);
    let x = rect_px(&d.xfrm_of(c).expect("xfrm"));
    assert!((x.y + x.h - 450.0).abs() < 0.5, "{x:?}");
    let again = reopen(&d);
    let id = again.pres.slides[0]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Connector(_)))
        .map(|s| s.id)
        .expect("connector");
    let (s, e) = conn(&again, id);
    assert!(s.is_some() && e.is_some());
}

#[test]
fn a_line_drawn_up_and_left_keeps_its_direction() {
    let mut d = doc();
    run(
        &mut d,
        D::ArmInsert {
            preset: Some("line".into()),
        },
    );
    press(&mut d, (400.0, 400.0), (300.0, 300.0), false);
    let x = d.xfrm_of(d.view.shapes[0]).expect("xfrm");
    assert!(x.flip_h && x.flip_v);
}

#[test]
fn a_list_becomes_grouped_shapes_one_per_item() {
    let mut d = doc();
    run(
        &mut d,
        D::InsertTextBox {
            rect: rect(100.0, 100.0, 800.0, 300.0),
        },
    );
    for (i, line) in ["Plan", "Build", "Ship"].iter().enumerate() {
        if i > 0 {
            run(&mut d, D::NewParagraph);
        }
        run(
            &mut d,
            D::InsertText {
                text: (*line).into(),
            },
        );
    }
    run(&mut d, D::NewParagraph);
    run(&mut d, D::Demote);
    run(
        &mut d,
        D::InsertText {
            text: "on time".into(),
        },
    );
    let source = d.view.text.as_ref().map(|t| t.shape).expect("editing");
    run(
        &mut d,
        D::ConvertToDiagram {
            layout: DeckListDiagram::Process,
        },
    );
    assert!(d.pres.shape(source).is_none());
    let check = |d: &DeckDoc| {
        let group = shapes(d)
            .iter()
            .find(|s| s.name.starts_with("List Diagram"))
            .expect("the diagram");
        let ShapeKind::Group(g) = &group.kind else {
            panic!("not a group")
        };
        let texts: Vec<String> = g
            .children
            .iter()
            .filter_map(|c| c.text().map(kiln_deck::TextBody::text))
            .filter(|t| !t.is_empty())
            .collect();
        assert_eq!(texts, ["Plan", "Build", "Ship\non time"]);
        let arrows = g
            .children
            .iter()
            .filter(|c| c.name.starts_with("Arrow"))
            .count();
        assert_eq!(arrows, 2);
    };
    check(&d);
    check(&reopen(&d));
    assert!(d.undo());
    assert!(d.pres.shape(source).is_some());
}
