// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Office features against the model: symbol fonts and Alt+X, AutoCorrect, Side to Side, hiding objects,
//! several selected objects aligned and distributed, Table Properties, Sort, the proofing
//! language and drag and drop of text — through the commands the chrome sends, with undo, and
//! saved to `.docx` and read back where the feature lives in the file.

use kiln_common::NodeId;
use kiln_doc::{Block, InlineObject};
use kiln_protocol::commands::write_features::{ObjectAlign, WrapMode};
use kiln_protocol::commands::write_office::{
    AutoCorrectEntry, PreferredWidth, RowHeightSpec, SortKind, SortSpec, TablePropertiesSpec,
};
use kiln_protocol::commands::{CellVerticalAlign, TextAlign, WriteQuery, WriteQueryResult};
use kiln_protocol::geom::Rect;
use kiln_protocol::{Command, FileFormat, TextPos, WriteCommand};

use crate::apps::write::WriteDoc;
use crate::model::DocModel;

fn doc() -> WriteDoc {
    WriteDoc::new(kiln_doc::Document::new(), Vec::new(), None)
}

fn w(d: &mut WriteDoc, c: WriteCommand) {
    let id = Command::Write(c.clone()).id();
    d.execute(&Command::Write(c))
        .unwrap_or_else(|e| panic!("{id} failed: {e}"));
}

fn typ(d: &mut WriteDoc, text: &str) {
    w(
        d,
        WriteCommand::InsertText {
            text: text.to_owned(),
        },
    );
}

fn keys(d: &mut WriteDoc, text: &str) {
    for c in text.chars() {
        typ(d, &c.to_string());
    }
}

fn ids(d: &WriteDoc) -> Vec<NodeId> {
    d.body().iter().map(|p| p.id).collect()
}

fn texts(d: &WriteDoc) -> Vec<String> {
    d.body()
        .iter()
        .map(|p| p.content.plain_text(true))
        .collect()
}

fn pos(d: &WriteDoc, para: usize, offset: u32) -> TextPos {
    TextPos {
        block: ids(d)[para],
        offset,
    }
}

fn select(d: &mut WriteDoc, a: (usize, u32), b: (usize, u32)) {
    let p = pos(d, a.0, a.1);
    w(
        d,
        WriteCommand::SelectTo {
            pos: p,
            extend: false,
        },
    );
    let p = pos(d, b.0, b.1);
    w(
        d,
        WriteCommand::SelectTo {
            pos: p,
            extend: true,
        },
    );
}

fn query(d: &WriteDoc, q: WriteQuery) -> WriteQueryResult {
    d.write_query(&q).expect("query")
}

fn round_trip(d: &WriteDoc) -> kiln_doc::Document {
    let bytes = d.save(FileFormat::Docx).expect("saves");
    kiln_io_docx::read_docx(&bytes).expect("reads").document
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(w, h, image::Rgba([40, 90, 200, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    png
}

/// A document with `n` floating pictures (in front of the text), the last one selected.
fn pictures(n: usize) -> (WriteDoc, Vec<NodeId>) {
    let mut d = doc();
    typ(&mut d, "One\nTwo\nThree\nFour");
    let mut out = Vec::new();
    for i in 0..n {
        let p = pos(&d, i, 0);
        w(
            &mut d,
            WriteCommand::SelectTo {
                pos: p,
                extend: false,
            },
        );
        w(
            &mut d,
            WriteCommand::InsertPicture {
                name: format!("p{i}.png"),
                bytes: png(48, 32),
                floating: false,
            },
        );
        w(
            &mut d,
            WriteCommand::ObjectWrap {
                wrap: WrapMode::InFront,
            },
        );
        let id = d.selected_object().expect("selected").2.id;
        out.push(id);
    }
    (d, out)
}

#[test]
fn symbol_fonts_and_alt_x() {
    let mut d = doc();
    let WriteQueryResult::SymbolTable(table) = query(
        &d,
        WriteQuery::SymbolTable {
            font: "Wingdings".into(),
        },
    ) else {
        panic!()
    };
    let smiley = table.iter().find(|c| c.text == "☺").expect("Wingdings J");
    assert_eq!(smiley.code, 0xF04A);
    w(
        &mut d,
        WriteCommand::InsertSymbol {
            font: "Wingdings".into(),
            code: smiley.code,
        },
    );
    let p = &d.body()[0];
    assert!(matches!(
        p.content.objects().first(),
        Some(InlineObject::Symbol(s)) if s.font == "Wingdings" && s.code == 0xF04A
    ));
    // Word writes it as w:sym and reads it back.
    let back = round_trip(&d);
    assert!(
        back.body_paragraphs()[0]
            .content
            .objects()
            .iter()
            .any(|o| matches!(o, InlineObject::Symbol(s) if s.code == 0xF04A))
    );
    // Alt+X: the code before the caret becomes its character, and back.
    keys(&mut d, " snow 2603");
    w(&mut d, WriteCommand::ToggleUnicode);
    assert!(texts(&d)[0].ends_with("snow ☃"), "{:?}", texts(&d));
    w(&mut d, WriteCommand::ToggleUnicode);
    assert!(texts(&d)[0].ends_with("snow 2603"), "{:?}", texts(&d));
    assert!(d.undo());
    assert!(texts(&d)[0].ends_with("snow ☃"));
    assert!(
        d.execute(&Command::Write(WriteCommand::InsertSymbol {
            font: "Arial".into(),
            code: 0x41,
        }))
        .is_err()
    );
}

#[test]
fn autocorrect_typos_symbols_and_capitals_with_undo() {
    let mut d = doc();
    w(
        &mut d,
        WriteCommand::SetAutoCorrect {
            entries: vec![
                AutoCorrectEntry {
                    from: "teh".into(),
                    to: "the".into(),
                },
                AutoCorrectEntry {
                    from: "(c)".into(),
                    to: "©".into(),
                },
            ],
        },
    );
    keys(&mut d, "teh kiln (c) on monday. it is HOt ");
    assert_eq!(texts(&d), vec!["The kiln © on Monday. It is Hot "]);
    // Ctrl+Z right after a correction brings back what was typed.
    keys(&mut d, "teh ");
    assert!(d.undo());
    assert_eq!(texts(&d), vec!["The kiln © on Monday. It is Hot teh "]);
    // Off: nothing changes.
    let o = kiln_protocol::commands::write_features::AutoFormatOptions {
        replace_text: Some(false),
        capitalize_sentences: Some(false),
        ..Default::default()
    };
    w(&mut d, WriteCommand::SetAutoFormat { options: o });
    typ(&mut d, "\n");
    keys(&mut d, "teh end ");
    assert_eq!(texts(&d)[1], "teh end ");
}

#[test]
fn autocorrect_leaves_addresses_alone_and_ends_sentences_after_numbers() {
    let mut d = doc();
    w(
        &mut d,
        WriteCommand::SetAutoCorrect {
            entries: vec![
                AutoCorrectEntry {
                    from: "teh".into(),
                    to: "the".into(),
                },
                AutoCorrectEntry {
                    from: "(c)".into(),
                    to: "©".into(),
                },
            ],
        },
    );
    keys(
        &mut d,
        "see www.teh.com, teh@teh.org and C:\\teh\\(c) then cone 6. it fired -- well ",
    );
    assert_eq!(
        texts(&d),
        vec!["See www.teh.com, teh@teh.org and C:\\teh\\(c) then cone 6. It fired – well "]
    );
    typ(&mut d, "\n");
    keys(&mut d, "-- dash ");
    assert_eq!(texts(&d)[1], "– dash ");
}

#[test]
fn side_to_side_puts_pages_in_a_row() {
    let mut d = doc();
    for _ in 0..3 {
        typ(&mut d, "Page");
        w(&mut d, WriteCommand::PageBreak);
    }
    let pages = |d: &mut WriteDoc| {
        let v = d.view_synced();
        v.layout
            .pages()
            .iter()
            .map(|p| p.origin)
            .collect::<Vec<_>>()
    };
    let vertical = pages(&mut d);
    assert!(vertical.len() >= 3);
    assert!(vertical[1].1 > vertical[0].1 && (vertical[1].0 - vertical[0].0).abs() < 1.0);
    w(&mut d, WriteCommand::PageMovement { side_to_side: true });
    let row = pages(&mut d);
    assert!(row[1].0 > row[0].0 && (row[1].1 - row[0].1).abs() < 1.0);
    assert!(d.view_synced().layout.extent().w > d.view_synced().layout.extent().h);
}

#[test]
fn hidden_objects_are_not_painted_and_round_trip() {
    let (mut d, ids) = pictures(2);
    w(
        &mut d,
        WriteCommand::ObjectVisible {
            id: Some(ids[0]),
            visible: false,
        },
    );
    let (_, _, first) = d.find_drawing(ids[0]).unwrap();
    assert!(first.hidden);
    let infos = d.object_infos();
    assert!(infos.iter().any(|o| o.id == ids[0] && o.hidden));
    let back = round_trip(&d);
    let hidden: Vec<bool> = back
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects())
        .filter_map(|o| match o {
            InlineObject::Drawing(x) => Some(x.hidden),
            _ => None,
        })
        .collect();
    assert_eq!(hidden.iter().filter(|h| **h).count(), 1);
    // Show All, then undo.
    w(
        &mut d,
        WriteCommand::ObjectVisible {
            id: None,
            visible: true,
        },
    );
    assert!(!d.find_drawing(ids[0]).unwrap().2.hidden);
    assert!(d.undo());
    assert!(d.find_drawing(ids[0]).unwrap().2.hidden);
}

#[test]
fn several_objects_align_to_each_other_and_distribute() {
    let (mut d, ids) = pictures(3);
    // Spread them out.
    for (i, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = 150.0 + 120.0 * i as f32 + if i == 1 { 40.0 } else { 0.0 };
        #[allow(clippy::cast_precision_loss)]
        let y = 140.0 + 50.0 * i as f32;
        w(
            &mut d,
            WriteCommand::ObjectTransform {
                id: *id,
                rect: Rect::new(x, y, 48.0, 32.0),
            },
        );
    }
    w(
        &mut d,
        WriteCommand::SelectObject {
            id: ids[0],
            add: None,
        },
    );
    for id in &ids[1..] {
        w(
            &mut d,
            WriteCommand::SelectObject {
                id: *id,
                add: Some(true),
            },
        );
    }
    assert_eq!(d.selected_objects(), ids);
    let rect = |d: &mut WriteDoc, id| {
        d.view_synced();
        d.drawing_rect(id).unwrap()
    };
    w(
        &mut d,
        WriteCommand::ObjectAlign {
            align: ObjectAlign::Top,
            relative_to: None,
        },
    );
    let tops: Vec<f32> = ids.iter().map(|id| rect(&mut d, *id).y).collect();
    assert!(tops.iter().all(|t| (t - tops[0]).abs() < 1.0), "{tops:?}");
    assert_eq!(d.selected_objects(), ids, "they stay selected");
    w(
        &mut d,
        WriteCommand::ObjectAlign {
            align: ObjectAlign::DistributeHorizontally,
            relative_to: None,
        },
    );
    let xs: Vec<f32> = ids.iter().map(|id| rect(&mut d, *id).x).collect();
    let (g1, g2) = (xs[1] - xs[0], xs[2] - xs[1]);
    assert!((g1 - g2).abs() < 1.5, "{xs:?}");
    // One undo step each.
    assert!(d.undo());
    let back: Vec<f32> = ids.iter().map(|id| rect(&mut d, *id).x).collect();
    assert!((back[1] - xs[1]).abs() > 1.0);
    // Align to the margin with several selected: each of them, one undo step.
    w(
        &mut d,
        WriteCommand::ObjectAlign {
            align: ObjectAlign::Left,
            relative_to: Some(kiln_protocol::commands::write_features::RelativeTo::Margin),
        },
    );
    let lefts: Vec<f32> = ids.iter().map(|id| rect(&mut d, *id).x).collect();
    assert!(
        lefts.iter().all(|x| (x - lefts[0]).abs() < 1.0),
        "{lefts:?}"
    );
    assert_eq!(d.selected_objects(), ids, "they stay selected");
    assert!(d.undo());
    let back: Vec<f32> = ids.iter().map(|id| rect(&mut d, *id).x).collect();
    assert!(
        (back[0] - back[2]).abs() > 1.0,
        "one step undoes them all: {back:?}"
    );
    assert!(d.redo());
    // Shift+click again removes one (after selecting them again: undo moved the selection).
    w(
        &mut d,
        WriteCommand::SelectObject {
            id: ids[0],
            add: None,
        },
    );
    for id in &ids[1..] {
        w(
            &mut d,
            WriteCommand::SelectObject {
                id: *id,
                add: Some(true),
            },
        );
    }
    w(
        &mut d,
        WriteCommand::SelectObject {
            id: ids[2],
            add: Some(true),
        },
    );
    assert_eq!(d.selected_objects(), ids[..2].to_vec());
}

fn table_doc() -> WriteDoc {
    let mut d = doc();
    w(
        &mut d,
        WriteCommand::InsertTable {
            rows: 4,
            cols: 2,
            autofit: kiln_protocol::commands::TableAutofit::Fixed,
        },
    );
    for (i, (a, b)) in [
        ("Name", "Date"),
        ("Pear", "3/1/2026"),
        ("apple", "12/24/2025"),
        ("Fig", "1/15/2026"),
    ]
    .iter()
    .enumerate()
    {
        typ(&mut d, a);
        w(&mut d, WriteCommand::Tab { back: false });
        typ(&mut d, b);
        if i < 3 {
            w(&mut d, WriteCommand::Tab { back: false });
        }
    }
    d
}

fn table(d: &WriteDoc) -> kiln_doc::Table {
    d.doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t.clone()),
            _ => None,
        })
        .expect("table")
}

fn first_column(d: &WriteDoc) -> Vec<String> {
    table(d)
        .rows
        .iter()
        .map(|r| {
            r.cells[0]
                .blocks
                .iter()
                .filter_map(Block::as_paragraph)
                .map(|p| p.content.plain_text(true))
                .collect::<String>()
        })
        .collect()
}

#[test]
fn table_properties_set_and_read_back() {
    let mut d = table_doc();
    let at = table(&d).rows[1].cells[0]
        .blocks
        .iter()
        .find_map(Block::as_paragraph)
        .unwrap()
        .id;
    w(
        &mut d,
        WriteCommand::SelectTo {
            pos: TextPos {
                block: at,
                offset: 0,
            },
            extend: false,
        },
    );
    w(
        &mut d,
        WriteCommand::TableProperties {
            spec: Box::new(TablePropertiesSpec {
                width: Some(PreferredWidth {
                    value: 80.0,
                    percent: true,
                }),
                alignment: Some(TextAlign::Center),
                wrap_around: Some(true),
                row_height: Some(RowHeightSpec {
                    points: 30.0,
                    exact: true,
                }),
                row_can_break: Some(false),
                cell_v_align: Some(CellVerticalAlign::Bottom),
                alt_title: Some("Fruit".into()),
                ..TablePropertiesSpec::default()
            }),
        },
    );
    let WriteQueryResult::TableProperties(Some(p)) = query(&d, WriteQuery::TableProperties) else {
        panic!()
    };
    assert_eq!(
        p.width,
        Some(PreferredWidth {
            value: 80.0,
            percent: true
        })
    );
    assert_eq!(p.alignment, Some(TextAlign::Center));
    assert_eq!(p.wrap_around, Some(true));
    assert_eq!(
        p.row_height,
        Some(RowHeightSpec {
            points: 30.0,
            exact: true
        })
    );
    assert_eq!(p.row_can_break, Some(false));
    assert_eq!(p.cell_v_align, Some(CellVerticalAlign::Bottom));
    assert_eq!(p.alt_title.as_deref(), Some("Fruit"));
    // In the file Word reads.
    let back = round_trip(&d);
    let t = back.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!(t.props.position.is_some());
    assert_eq!(t.props.caption.as_deref(), Some("Fruit"));
    assert_eq!(t.rows[1].props.cant_split, Some(true));
    assert!(d.undo());
    let WriteQueryResult::TableProperties(Some(p)) = query(&d, WriteQuery::TableProperties) else {
        panic!()
    };
    assert_eq!(p.wrap_around, Some(false));
}

#[test]
fn sort_table_rows_by_text_and_date_and_paragraphs_by_number() {
    let mut d = table_doc();
    w(
        &mut d,
        WriteCommand::Sort {
            spec: SortSpec {
                header: true,
                ..SortSpec::default()
            },
        },
    );
    assert_eq!(first_column(&d), ["Name", "apple", "Fig", "Pear"]);
    w(
        &mut d,
        WriteCommand::Sort {
            spec: SortSpec {
                kind: SortKind::Date,
                column: 1,
                header: true,
                descending: true,
                ..SortSpec::default()
            },
        },
    );
    assert_eq!(first_column(&d), ["Name", "Pear", "Fig", "apple"]);
    assert!(d.undo());
    assert_eq!(first_column(&d), ["Name", "apple", "Fig", "Pear"]);
    // Paragraphs by number.
    let mut d = doc();
    typ(&mut d, "10 pots\n9 jugs\n100 cups\nno number");
    select(&mut d, (0, 0), (3, 4));
    w(
        &mut d,
        WriteCommand::Sort {
            spec: SortSpec {
                kind: SortKind::Number,
                ..SortSpec::default()
            },
        },
    );
    assert_eq!(texts(&d), ["9 jugs", "10 pots", "100 cups", "no number"]);
}

#[test]
fn proofing_language_and_no_proof_round_trip() {
    let mut d = doc();
    typ(&mut d, "Bonjour le monde");
    select(&mut d, (0, 0), (0, 7));
    w(
        &mut d,
        WriteCommand::SetLanguage {
            lang: "fr-FR".into(),
            no_proof: true,
        },
    );
    let back = round_trip(&d);
    let p = back.body_paragraphs()[0];
    let r = &p.content.runs()[0].props;
    assert_eq!(
        r.lang.as_ref().and_then(|l| l.val.as_deref()),
        Some("fr-FR")
    );
    assert_eq!(r.no_proof, Some(true));
    assert!(
        d.execute(&Command::Write(WriteCommand::SetLanguage {
            lang: "not a tag!".into(),
            no_proof: false,
        }))
        .is_err()
    );
}

#[test]
fn drag_and_drop_moves_or_copies_the_selection_in_one_step() {
    let mut d = doc();
    typ(&mut d, "red green blue\nsecond");
    // Move "green " to the start of the second paragraph.
    select(&mut d, (0, 4), (0, 10));
    let to = pos(&d, 1, 0);
    w(&mut d, WriteCommand::MoveSelection { to, copy: false });
    assert_eq!(texts(&d), ["red blue", "green second"]);
    assert!(d.undo());
    assert_eq!(texts(&d), ["red green blue", "second"]);
    // Move forward within the paragraph: "red " after "blue".
    select(&mut d, (0, 0), (0, 4));
    let to = pos(&d, 0, 14);
    w(&mut d, WriteCommand::MoveSelection { to, copy: false });
    assert_eq!(texts(&d)[0], "green bluered ");
    // Copy with Ctrl.
    select(&mut d, (1, 0), (1, 6));
    let to = pos(&d, 0, 0);
    w(&mut d, WriteCommand::MoveSelection { to, copy: true });
    assert_eq!(texts(&d), ["secondgreen bluered ", "second"]);
    // A drop inside the selection does nothing.
    select(&mut d, (1, 0), (1, 6));
    let to = pos(&d, 1, 3);
    w(&mut d, WriteCommand::MoveSelection { to, copy: false });
    assert_eq!(texts(&d)[1], "second");
}

/// The ops of the first page.
fn page_ops(d: &mut WriteDoc) -> Vec<kiln_scene::DrawOp> {
    d.view_synced();
    let doc = d.doc.clone();
    let v = d.view.as_deref_mut().expect("a view");
    let rect = v.layout.page_rect(0).expect("a page");
    let crate::apps::write::WriteView { layout, scene, .. } = v;
    scene.page_ops(layout, &doc, 0, rect)
}

fn shape_doc() -> (WriteDoc, NodeId) {
    let mut d = doc();
    typ(&mut d, "Text beside the shape");
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "roundRect".into(),
            text: Some("Fire".into()),
            rect: None,
        },
    );
    let id = d.selected_object().expect("selected").2.id;
    (d, id)
}

#[test]
fn shape_styles_gradients_effects_and_text_direction() {
    use kiln_protocol::commands::write_office::{
        GradientSpec, ShapeEffect, ShapeStyleLevel, ShapeStylePreset, TextDirection,
    };
    let (mut d, id) = shape_doc();
    w(
        &mut d,
        WriteCommand::ShapeStyle {
            preset: ShapeStylePreset {
                color: 2,
                level: ShapeStyleLevel::Intense,
            },
        },
    );
    let xml = d.find_drawing(id).unwrap().2.xml.unwrap();
    assert!(
        xml.contains("a:gradFill") && xml.contains("accent2"),
        "{xml}"
    );
    assert!(xml.contains("a:outerShdw"));
    let ops = page_ops(&mut d);
    assert!(
        ops.iter()
            .any(|o| matches!(o, kiln_scene::DrawOp::Gradient { .. })),
        "the gradient is painted"
    );
    w(
        &mut d,
        WriteCommand::ShapeGradient {
            gradient: Some(GradientSpec {
                from: "FFFFFF".into(),
                to: "C2481C".into(),
                angle: 90.0,
            }),
        },
    );
    assert!(
        d.find_drawing(id)
            .unwrap()
            .2
            .xml
            .unwrap()
            .contains("C2481C")
    );
    w(
        &mut d,
        WriteCommand::ShapeEffect {
            effect: ShapeEffect::Glow,
        },
    );
    let xml = d.find_drawing(id).unwrap().2.xml.unwrap();
    assert!(xml.contains("a:glow") && !xml.contains("a:outerShdw"));
    w(
        &mut d,
        WriteCommand::ShapeTextDirection {
            direction: TextDirection::Rotate270,
        },
    );
    assert!(
        d.find_drawing(id)
            .unwrap()
            .2
            .xml
            .unwrap()
            .contains("vert=\"vert270\"")
    );
    // The text is laid out turned.
    let turned = d.view_synced().layout.pages()[0]
        .front
        .iter()
        .any(|it| matches!(it, kiln_doc_layout::page::Item::Line(l) if l.turn.is_some()));
    assert!(turned, "the shape's text turns");
    // Saved for Word.
    let back = round_trip(&d);
    let xml = back
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects())
        .find_map(|o| match o {
            InlineObject::Drawing(x) => x.xml.clone(),
            _ => None,
        })
        .unwrap();
    assert!(xml.contains("a:glow") && xml.contains("vert270"));
}

#[test]
fn crop_to_shape_and_picture_corrections() {
    use kiln_protocol::commands::write_office::PictureCorrection;
    let (mut d, ids) = pictures(1);
    w(
        &mut d,
        WriteCommand::CropToShape {
            preset: "ellipse".into(),
        },
    );
    let pic = d.find_drawing(ids[0]).unwrap().2;
    assert_eq!(
        kiln_doc_layout::drawing::picture_geometry(&pic).as_deref(),
        Some("ellipse")
    );
    let ops = page_ops(&mut d);
    assert!(
        ops.iter()
            .any(|o| matches!(o, kiln_scene::DrawOp::PushClip { path: Some(_), .. }))
    );
    w(
        &mut d,
        WriteCommand::PictureCorrections {
            correction: PictureCorrection {
                brightness: 20,
                contrast: -10,
            },
        },
    );
    let pic = d.find_drawing(ids[0]).unwrap().2;
    assert_eq!(
        kiln_doc_layout::drawing::picture_lum(&pic),
        Some((20_000, -10_000))
    );
    // Word reads both from the file.
    let back = round_trip(&d);
    let xml = back
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects())
        .find_map(|o| match o {
            InlineObject::Drawing(x) => x.xml.clone(),
            _ => None,
        })
        .unwrap();
    assert!(
        xml.contains("prst=\"ellipse\"") && xml.contains("bright=\"20000\""),
        "{xml}"
    );
    assert!(
        d.execute(&Command::Write(WriteCommand::CropToShape {
            preset: "notashape".into(),
        }))
        .is_err()
    );
}

#[test]
fn balloons_inline_and_revisions_in_balloons() {
    use kiln_protocol::commands::write_features::MarkupMode;
    use kiln_protocol::commands::write_office::BalloonMode;
    let mut d = doc();
    typ(&mut d, "The kiln glows bright");
    w(&mut d, WriteCommand::SetReviewer { name: "Ana".into() });
    w(&mut d, WriteCommand::TrackChanges);
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::All,
        },
    );
    select(&mut d, (0, 15), (0, 21));
    w(&mut d, WriteCommand::DeleteBackward);
    select(&mut d, (0, 4), (0, 8));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "Which kiln?".into(),
        },
    );
    let balloons = |d: &mut WriteDoc| {
        d.view_synced();
        let WriteQueryResult::RevisionBalloons(b) = query(d, WriteQuery::RevisionBalloons) else {
            panic!()
        };
        b
    };
    let comment_balloons = |d: &mut WriteDoc| {
        d.view_synced();
        d.comment_infos()
            .iter()
            .filter(|c| c.balloon.is_some())
            .count()
    };
    assert!(
        balloons(&mut d).is_empty(),
        "deletions stay inline by default"
    );
    assert_eq!(comment_balloons(&mut d), 1);
    w(
        &mut d,
        WriteCommand::Balloons {
            mode: BalloonMode::Revisions,
        },
    );
    let b = balloons(&mut d);
    assert_eq!(b.len(), 1);
    assert!(b[0].deleted && b[0].text.contains("bright"), "{b:?}");
    assert_eq!(comment_balloons(&mut d), 1);
    w(
        &mut d,
        WriteCommand::Balloons {
            mode: BalloonMode::Inline,
        },
    );
    assert!(balloons(&mut d).is_empty());
    assert_eq!(comment_balloons(&mut d), 0, "no balloons at all");
}

fn drawings_of(doc: &kiln_doc::Document) -> Vec<kiln_doc::Drawing> {
    doc.all_paragraphs()
        .iter()
        .flat_map(|(_, p)| p.content.objects())
        .filter_map(|o| match o {
            InlineObject::Drawing(d) => Some((**d).clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn group_and_ungroup_objects_paint_and_round_trip() {
    let (mut d, ids) = pictures(2);
    // A shape with text joins the pictures.
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "ellipse".into(),
            text: Some("Kiln".into()),
            rect: Some(Rect::new(300.0, 300.0, 120.0, 60.0)),
        },
    );
    let shape = d.selected_object().expect("selected").2.id;
    for (i, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = 150.0 + 100.0 * i as f32;
        w(
            &mut d,
            WriteCommand::ObjectTransform {
                id: *id,
                rect: Rect::new(x, 150.0, 48.0, 32.0),
            },
        );
    }
    let mut all = ids.clone();
    all.push(shape);
    let before: Vec<Rect> = {
        d.view_synced();
        all.iter().map(|id| d.drawing_rect(*id).unwrap()).collect()
    };
    w(
        &mut d,
        WriteCommand::SelectObject {
            id: all[0],
            add: None,
        },
    );
    for id in &all[1..] {
        w(
            &mut d,
            WriteCommand::SelectObject {
                id: *id,
                add: Some(true),
            },
        );
    }
    w(&mut d, WriteCommand::GroupObjects { ungroup: false });
    let objs = drawings_of(&d.doc);
    assert_eq!(objs.len(), 1, "one group");
    let g = &objs[0];
    assert_eq!(g.kind, kiln_doc::DrawingKind::Group);
    assert_eq!(g.group.as_ref().unwrap().members.len(), 3);
    // The group wraps as the first selected object (In Front): mixed wraps take the first one's.
    let wrap_of = |d: &kiln_doc::Drawing| match &d.placement {
        kiln_doc::Placement::Anchor(a) => Some(a.wrap),
        kiln_doc::Placement::Inline => None,
    };
    let in_front = |w: Option<kiln_doc::st::WrapType>| {
        matches!(
            w,
            Some(kiln_doc::st::WrapType::InFront | kiln_doc::st::WrapType::None)
        )
    };
    assert!(in_front(wrap_of(g)), "{:?}", wrap_of(g));
    assert_eq!(
        d.selected_object().unwrap().2.id,
        g.id,
        "the group is selected"
    );
    // The group is where its members were: their bounding box.
    d.view_synced();
    let gr = d.drawing_rect(g.id).unwrap();
    let left = before.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
    let top = before.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
    assert!(
        (gr.x - left).abs() < 1.5 && (gr.y - top).abs() < 1.5,
        "{gr:?} {before:?}"
    );
    // Its pictures, shape and text are painted (no placeholder frame).
    let ops = page_ops(&mut d);
    let images = ops
        .iter()
        .filter(|o| matches!(o, kiln_scene::DrawOp::Image { .. }))
        .count();
    assert_eq!(images, 2, "both pictures");
    // Saved as a wpg group with its members, read back as one.
    let bytes = d.save(FileFormat::Docx).expect("saves");
    let xml = {
        let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes).expect("zip");
        String::from_utf8(
            pkg.part("/word/document.xml")
                .unwrap()
                .unwrap()
                .into_owned(),
        )
        .unwrap()
    };
    assert!(
        xml.contains("<wpg:wgp>") && xml.contains("<a:chExt"),
        "{xml}"
    );
    assert_eq!(xml.matches("<pic:pic").count(), 2);
    assert!(xml.contains("prst=\"ellipse\"") && xml.contains(">Kiln<"));
    let back = kiln_io_docx::read_docx(&bytes).expect("reads").document;
    let objs2 = drawings_of(&back);
    assert_eq!(objs2.len(), 1);
    let g2 = objs2[0].group.as_ref().expect("read as a group");
    assert_eq!(g2.members.len(), 3);
    assert!(
        g2.members.iter().any(|m| m.drawing.text_box.is_some()),
        "the shape keeps its text"
    );
    // Undo brings the separate objects back; redo the group.
    assert!(d.undo());
    assert_eq!(drawings_of(&d.doc).len(), 3);
    assert!(d.redo());
    assert_eq!(drawings_of(&d.doc).len(), 1);
    // Ungroup: three objects where they were, all selected.
    w(&mut d, WriteCommand::GroupObjects { ungroup: true });
    let objs = drawings_of(&d.doc);
    assert_eq!(objs.len(), 3);
    // Each member gets its own wrapping back: the pictures In Front, the shape Square.
    for o in &objs {
        if o.id == shape {
            assert_eq!(
                wrap_of(o),
                Some(kiln_doc::st::WrapType::Square),
                "{}",
                o.name
            );
        } else {
            assert!(in_front(wrap_of(o)), "{}: {:?}", o.name, wrap_of(o));
        }
    }
    assert_eq!(d.selected_objects().len(), 3);
    d.view_synced();
    for (id, r) in all.iter().zip(&before) {
        let now = d.drawing_rect(*id).expect("member back");
        assert!(
            (now.x - r.x).abs() < 1.5 && (now.y - r.y).abs() < 1.5,
            "{now:?} {r:?}"
        );
    }
    // Ungrouped members save as drawings of their own.
    let back = round_trip(&d);
    let kinds: Vec<_> = drawings_of(&back).into_iter().map(|d| d.kind).collect();
    assert_eq!(kinds.len(), 3, "{kinds:?}");
    // A single object cannot be grouped.
    w(
        &mut d,
        WriteCommand::SelectObject {
            id: all[0],
            add: None,
        },
    );
    assert!(
        d.execute(&Command::Write(WriteCommand::GroupObjects {
            ungroup: false
        }))
        .is_err()
    );
}

#[test]
fn split_window_frames_carry_the_second_pane_s_tiles() {
    use crate::frame::{FrameRequest, FrameSource};
    let mut d = doc();
    for i in 0..400 {
        typ(
            &mut d,
            &format!("Paragraph {i} of a long document that runs over many pages.\n"),
        );
    }
    let vp = kiln_protocol::Viewport {
        width_px: 900.0,
        height_px: 400.0,
        ..kiln_protocol::Viewport::default()
    };
    let req = FrameRequest::for_viewport(&vp).expect("a request");
    let far = |d: &WriteDoc| {
        let s = d.scene();
        s.slots()
            .iter()
            .filter(|t| t.rect.y > 5000.0 && t.rect.y < 6000.0)
            .filter(|t| s.is_built(t.id))
            .count()
    };
    for _ in 0..20 {
        d.prepare(&req).unwrap();
    }
    assert_eq!(far(&d), 0, "far pages are not built for the viewport alone");
    w(
        &mut d,
        WriteCommand::SplitRegion {
            region: Some(Rect::new(0.0, 5200.0, 900.0, 300.0)),
        },
    );
    assert_eq!(d.extra_regions(&req).len(), 1);
    for _ in 0..20 {
        d.prepare(&req).unwrap();
    }
    assert!(far(&d) > 0, "the second pane's tiles are built");
    w(&mut d, WriteCommand::SplitRegion { region: None });
    assert!(d.extra_regions(&req).is_empty());
    d.prepare(&req).unwrap();
    assert_eq!(far(&d), 0, "and dropped when the split is removed");
}

#[test]
fn arrows_follow_the_paragraph_direction() {
    use kiln_protocol::CaretMotion;
    let mut d = doc();
    typ(&mut d, "abc\nשלום");
    // The second paragraph right to left.
    let id = ids(&d)[1];
    let mut props = d.doc.paragraph(id).unwrap().props.clone();
    props.bidi = Some(true);
    d.commit(kiln_doc::Transaction::new(vec![
        kiln_doc::Op::SetParagraphProps {
            para: id,
            props: Box::new(props),
        },
    ]))
    .unwrap();
    let mv = |d: &mut WriteDoc, motion| {
        w(
            d,
            WriteCommand::MoveCaret {
                motion,
                extend: false,
            },
        );
        (d.head.para, d.head.offset)
    };
    select(&mut d, (1, 0), (1, 0));
    assert_eq!(
        mv(&mut d, CaretMotion::Left),
        (id, 1),
        "Left goes on in RTL text"
    );
    assert_eq!(mv(&mut d, CaretMotion::Right), (id, 0), "Right goes back");
    select(&mut d, (0, 1), (0, 1));
    assert_eq!(mv(&mut d, CaretMotion::Left).1, 0, "LTR as before");
    assert_eq!(mv(&mut d, CaretMotion::WordRight).1, 3);
}
