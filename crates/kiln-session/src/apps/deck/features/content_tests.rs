// SPDX-License-Identifier: AGPL-3.0-or-later
//! Print and export, Design Ideas, Outline View and Reuse Slides, one behaviour per test.

use kiln_deck::{Presentation, ShapeKind};
use kiln_protocol::commands::DeckCommand as D;
use kiln_protocol::commands::deck_features::{DeckOutlineEdit as E, DeckPictureFormat};
use kiln_protocol::print::{PrintSettings, SlideLayout, SlidePrint};
use kiln_protocol::{Command, FileFormat};

use crate::apps::deck::DeckDoc;
use crate::model::{Change, DocModel};
use crate::print::{PrintJob, PrintOutput};

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pptx/features")
            .join(name),
    )
    .expect("fixture")
}

fn open(name: &str) -> DeckDoc {
    let bytes = fixture_bytes(name);
    let read = kiln_io_pptx::read_pptx(&bytes).expect("reads");
    let (pres, original) = crate::apps::deck::save::Original::from_read(read, &bytes);
    DeckDoc::new(pres, Some(original), Some(name.into()))
}

fn run(d: &mut DeckDoc, c: D) -> Change {
    d.execute(&Command::Deck(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

fn outline(d: &mut DeckDoc, e: E) {
    run(d, D::EditOutline { edit: e });
}

/// The pages a print makes (the preview's sheets; one page per sheet).
fn pages(d: &DeckDoc, slides: Option<SlidePrint>, pages: &str) -> usize {
    let settings = PrintSettings {
        pages: pages.into(),
        slides,
        ..PrintSettings::default()
    };
    match d
        .print(&PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 0.5,
        })
        .expect("prints")
    {
        PrintOutput::Preview(p) => p.sheets as usize,
        PrintOutput::Pdf(_) => panic!("a preview"),
    }
}

fn pdf(d: &DeckDoc, slides: Option<SlidePrint>, pages: &str) -> Vec<u8> {
    let settings = PrintSettings {
        pages: pages.into(),
        slides,
        tagged: false,
        ..PrintSettings::default()
    };
    match d
        .print(&PrintJob::Pdf {
            settings: &settings,
            title: "Plans",
        })
        .expect("prints")
    {
        PrintOutput::Pdf(f) => f.bytes,
        PrintOutput::Preview(_) => panic!("a PDF"),
    }
}

fn layout(layout: SlideLayout, per_page: u32) -> Option<SlidePrint> {
    Some(SlidePrint {
        layout,
        per_page,
        ..SlidePrint::default()
    })
}

#[test]
fn pdf_layouts_have_their_page_counts() {
    let d = open("placeholders.pptx");
    let n = d.pres.slides.iter().filter(|s| !s.hidden).count();
    assert_eq!(pages(&d, None, ""), n);
    assert_eq!(pages(&d, layout(SlideLayout::NotesPages, 1), ""), n);
    for per in [1, 2, 3, 4, 6, 9] {
        let l = layout(SlideLayout::Handouts, per);
        assert_eq!(pages(&d, l, ""), n.div_ceil(per as usize), "{per} per page");
        assert!(pdf(&d, l, "").starts_with(b"%PDF-"));
    }
    assert!(pages(&d, layout(SlideLayout::Outline, 1), "") >= 1);
    // The page box picks slides.
    assert_eq!(pages(&d, None, "2-3"), 2);
    assert_eq!(pages(&d, layout(SlideLayout::Handouts, 2), "1-3"), 2);
}

#[test]
fn the_pdf_has_the_slides_text() {
    let d = open("placeholders.pptx");
    let title = kiln_deck_layout::a11y::slide_title(&d.pres, d.pres.slides[0].id);
    assert!(!title.is_empty());
    let settings = PrintSettings::default();
    let PrintOutput::Pdf(f) = d
        .print(&PrintJob::Pdf {
            settings: &settings,
            title: "Plans",
        })
        .expect("prints")
    else {
        panic!("a PDF")
    };
    // Tagged with a structure tree and bookmarks per slide.
    let text = String::from_utf8_lossy(&f.bytes);
    assert!(text.contains("/StructTreeRoot"), "tagged");
    assert!(text.contains("/Outlines"), "bookmarks");
}

#[test]
fn the_print_preview_shows_handout_sheets() {
    let d = open("placeholders.pptx");
    let n = d.pres.slides.len();
    let settings = PrintSettings {
        slides: layout(SlideLayout::Handouts, 3),
        ..PrintSettings::default()
    };
    let PrintOutput::Preview(p) = d
        .print(&PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 1.0,
        })
        .expect("preview")
    else {
        panic!("a preview")
    };
    assert_eq!(p.sheets as usize, n.div_ceil(3));
    // Portrait: the notes page size (7.5 × 10 in).
    assert!(p.height_pt > p.width_pt);
    let bad = PrintSettings {
        pages: "40-50".into(),
        ..PrintSettings::default()
    };
    assert!(
        d.print(&PrintJob::Preview {
            settings: &bad,
            sheet: 0,
            scale: 1.0
        })
        .is_err()
    );
}

#[test]
fn hidden_slides_print_when_asked() {
    let mut d = open("placeholders.pptx");
    let n = d.pres.slides.len();
    run(&mut d, D::HideSlides { hidden: Some(true) });
    assert_eq!(pages(&d, None, ""), n - 1);
    let all = SlidePrint {
        hidden: true,
        ..SlidePrint::default()
    };
    assert_eq!(pages(&d, Some(all), ""), n);
}

#[test]
fn slides_export_as_pictures() {
    let d = open("placeholders.pptx");
    let one = d
        .export_pictures(Some(&[0]), DeckPictureFormat::Png, 1280)
        .expect("png");
    assert!(one.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(one.name, "placeholders.png");
    let img = image::load_from_memory(&one.bytes).expect("decodes");
    let ratio = f64::from(img.width()) / f64::from(img.height());
    let want = d.pres.slide_size.cx as f64 / d.pres.slide_size.cy as f64;
    assert_eq!(img.width(), 1280);
    assert!((ratio - want).abs() < 0.01);
    let jpeg = d
        .export_pictures(Some(&[1]), DeckPictureFormat::Jpeg, 640)
        .expect("jpeg");
    assert!(jpeg.bytes.starts_with(&[0xFF, 0xD8, 0xFF]));
    let all = d
        .export_pictures(None, DeckPictureFormat::Png, 320)
        .expect("zip");
    assert_eq!(all.name, "placeholders.zip");
    let zip = zip::ZipArchive::new(std::io::Cursor::new(all.bytes)).expect("a zip");
    assert_eq!(zip.len(), d.pres.slides.len());
    assert!(
        zip.file_names()
            .any(|n| n.is_ok_and(|n| n == "placeholders/Slide1.png"))
    );
    assert!(
        d.export_pictures(Some(&[99]), DeckPictureFormat::Png, 100)
            .is_err()
    );
}

/// A new deck whose second slide has a title and `n` bullets.
fn with_bullets(n: usize) -> DeckDoc {
    let mut d = DeckDoc::new(Presentation::new(), None, None);
    run(&mut d, D::NewSlide { layout: None });
    outline(
        &mut d,
        E::Text {
            slide: 1,
            line: 0,
            text: "Firing".into(),
        },
    );
    let body = d.pres.slides[1]
        .shapes
        .iter()
        .find(|s| {
            s.placeholder
                .is_some_and(|p| !p.kind.is_title() && !p.kind.is_footer_area())
        })
        .map(|s| s.id)
        .expect("a body placeholder");
    let text: Vec<String> = (1..=n).map(|i| format!("Point {i}")).collect();
    d.apply_with("test", |p| {
        kiln_deck::edit::text::set_text(p, kiln_deck::TextTarget::Shape(body), &text.join("\n"))
    })
    .expect("text");
    d
}

#[test]
fn design_ideas_suggest_and_apply_in_one_step() {
    let mut d = with_bullets(3);
    let before = d.pres.clone();
    let ideas = d.design_ideas(160.0, 1.0).expect("ideas");
    assert!((4..=6).contains(&ideas.len()), "{}", ideas.len());
    for i in &ideas {
        assert!(i.png.starts_with(b"\x89PNG"), "{}", i.id);
        let img = image::load_from_memory(&i.png).expect("decodes");
        assert_eq!(img.width(), 160);
    }
    // Different ideas look different.
    assert_ne!(ideas[0].png, ideas[1].png);
    run(&mut d, D::ApplyDesignIdea { id: "band".into() });
    let decor = |d: &DeckDoc| {
        d.pres.slides[1]
            .shapes
            .iter()
            .filter(|s| s.name.starts_with("Design Element"))
            .count()
    };
    assert_eq!(decor(&d), 1);
    // Another idea replaces it.
    run(&mut d, D::ApplyDesignIdea { id: "dark".into() });
    assert_eq!(decor(&d), 1);
    assert!(d.pres.slides[1].background.is_some());
    run(&mut d, D::ApplyDesignIdea { id: "panel".into() });
    assert!(d.pres.slides[1].background.is_none());
    // One undo step each.
    assert!(d.undo() && d.undo() && d.undo());
    assert_eq!(d.pres, before);
    let err = d
        .execute(&Command::Deck(D::ApplyDesignIdea {
            id: "pictureRight".into(),
        }))
        .expect_err("no picture");
    assert!(err.to_string().contains("does not fit"), "{err}");
}

#[test]
fn design_ideas_fill_the_area_with_the_picture() {
    let mut d = open("pictures.pptx");
    let k = d
        .pres
        .slides
        .iter()
        .position(|s| {
            s.shapes
                .iter()
                .any(|sh| matches!(sh.kind, ShapeKind::Picture(_)))
        })
        .expect("a picture slide");
    run(
        &mut d,
        D::SelectSlide {
            index: k as u32,
            extend: false,
            toggle: false,
        },
    );
    let ideas = d.design_ideas(120.0, 1.0).expect("ideas");
    assert_eq!(ideas[0].id, "pictureRight");
    run(
        &mut d,
        D::ApplyDesignIdea {
            id: "pictureRight".into(),
        },
    );
    // The right half of the slide is pictures in a grid.
    let slide = &d.pres.slides[k];
    let (w, h) = (d.pres.slide_size.cx, d.pres.slide_size.cy);
    let (mut left, mut top, mut right, mut bottom) = (i64::MAX, i64::MAX, 0, 0);
    for sh in &slide.shapes {
        if let ShapeKind::Picture(p) = &sh.kind {
            let x = sh.xfrm.expect("placed");
            assert!(p.crop.l >= 0 && p.crop.t >= 0);
            left = left.min(x.x);
            top = top.min(x.y);
            right = right.max(x.x + x.cx);
            bottom = bottom.max(x.y + x.cy);
        }
    }
    assert_eq!((left, top), (w / 2, 0));
    assert!(
        (w - right).abs() < 8 && (h - bottom).abs() < 8,
        "{right} {bottom}"
    );
}

#[test]
fn slide_text_has_every_shape_with_text_tables_and_notes() {
    let d = with_bullets(2);
    let t = d.slide_text();
    assert_eq!(t.len(), 2);
    let lines: Vec<&[String]> = t[1].texts.iter().map(|x| x.lines.as_slice()).collect();
    assert_eq!(
        lines,
        [
            &["Firing".to_owned()][..],
            &["Point 1".to_owned(), "Point 2".to_owned()][..]
        ]
    );
    assert_eq!(t[1].shapes, 2, "the title and the body");
    let tables = open("tables.pptx");
    let cells = tables
        .slide_text()
        .into_iter()
        .flat_map(|s| s.texts)
        .find(|x| x.kind == "table")
        .expect("a table's text");
    assert!(cells.lines.len() > 3, "{cells:?}");
    let notes = open("notes.pptx");
    assert!(notes.slide_text().iter().any(|s| !s.notes.is_empty()));
}

#[test]
fn outline_view_lists_titles_and_lines() {
    let d = with_bullets(3);
    let o = d.outline_view();
    assert_eq!(o.len(), 2);
    assert_eq!(o[1].title, "Firing");
    let lines: Vec<&str> = o[1].lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(lines, ["Point 1", "Point 2", "Point 3"]);
    assert!(o[1].has_title);
}

#[test]
fn outline_typing_keeps_formatting_and_merges() {
    let mut d = with_bullets(2);
    let steps = d.done.len();
    for t in ["Point 1 a", "Point 1 ab", "Point 1 abc"] {
        outline(
            &mut d,
            E::Text {
                slide: 1,
                line: 1,
                text: t.into(),
            },
        );
    }
    assert_eq!(d.outline_view()[1].lines[0].text, "Point 1 abc");
    // Typing is one undo step.
    assert_eq!(d.done.len(), steps + 1);
}

#[test]
fn outline_enter_tab_and_backspace_restructure_slides() {
    let mut d = with_bullets(3);
    // Enter in a body line splits it.
    outline(
        &mut d,
        E::Split {
            slide: 1,
            line: 2,
            offset: 5,
        },
    );
    let o = d.outline_view();
    assert_eq!(o[1].lines[1].text, "Point");
    assert_eq!(o[1].lines[2].text, " 2");
    // Shift+Tab on a first-level line makes a new slide of it and the lines after it.
    outline(
        &mut d,
        E::Indent {
            slide: 1,
            line: 3,
            by: -1,
        },
    );
    let o = d.outline_view();
    assert_eq!(o.len(), 3);
    assert_eq!(o[2].title, " 2");
    assert_eq!(o[2].lines.len(), 1);
    assert_eq!(o[2].lines[0].text, "Point 3");
    // Tab on that title makes it part of slide 2 again, its lines a level down.
    outline(
        &mut d,
        E::Indent {
            slide: 2,
            line: 0,
            by: 1,
        },
    );
    let o = d.outline_view();
    assert_eq!(o.len(), 2);
    let lines: Vec<(u8, &str)> = o[1]
        .lines
        .iter()
        .map(|l| (l.level, l.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [(0, "Point 1"), (0, "Point"), (0, " 2"), (1, "Point 3")]
    );
    // Backspace at the start of a line joins it to the one before.
    outline(&mut d, E::Join { slide: 1, line: 3 });
    assert_eq!(d.outline_view()[1].lines[1].text, "Point 2");
    // Enter in a title starts a new slide with the rest of it and the lines.
    outline(
        &mut d,
        E::Split {
            slide: 1,
            line: 0,
            offset: 4,
        },
    );
    let o = d.outline_view();
    assert_eq!(o.len(), 3);
    assert_eq!((o[1].title.as_str(), o[2].title.as_str()), ("Firi", "ng"));
    assert!(o[1].lines.is_empty() && o[2].lines.len() == 3);
    // Backspace at the start of that title joins the slides again.
    outline(&mut d, E::Join { slide: 2, line: 0 });
    let o = d.outline_view();
    assert_eq!(o.len(), 2);
    assert_eq!(o[1].title, "Firing");
    // Every step undoes on its own, and the outline round-trips through .pptx.
    let bytes = d.write(FileFormat::Pptx, None).expect("saves");
    let back = kiln_io_pptx::read_pptx(&bytes).expect("reads");
    let (pres, original) = crate::apps::deck::save::Original::from_read(back, &bytes);
    let back = DeckDoc::new(pres, Some(original), None);
    let text = |d: &DeckDoc| {
        d.outline_view()
            .into_iter()
            .map(|e| (e.title, e.lines))
            .collect::<Vec<_>>()
    };
    assert_eq!(text(&back), text(&d));
    while d.done.len() > 3 {
        assert!(d.undo());
    }
    assert_eq!(d.outline_view()[1].lines.len(), 3);
}

/// The runs of a slide's title: (text, bold).
fn title_runs(d: &DeckDoc, slide: usize) -> Vec<(String, bool)> {
    let t = d.pres.slides[slide]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| p.kind.is_title()))
        .and_then(kiln_deck::Shape::text)
        .expect("a title");
    t.paragraphs()
        .flat_map(|p| p.content.run_texts())
        .map(|(_, props, text)| (text, props.bold == Some(true)))
        .collect()
}

#[test]
fn outline_title_splits_and_joins_keep_their_runs() {
    let mut d = with_bullets(2);
    // "Firing" with "Fir" bold.
    let title = d.pres.slides[1]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| p.kind.is_title()))
        .expect("a title");
    let (id, old) = (title.id, title.text().expect("text").clone());
    let bold = kiln_doc::RunProps {
        bold: Some(true),
        ..kiln_doc::RunProps::default()
    };
    let mut frag = kiln_doc::Fragment::text("Fir", bold);
    let plain = kiln_doc::Fragment::text("ing", kiln_doc::RunProps::default());
    frag.text.push_str(&plain.text);
    frag.runs.extend(plain.runs);
    let mut para = old.paragraphs().next().expect("a paragraph").clone();
    para.content = kiln_doc::Inlines::from_fragment(frag).expect("runs");
    let mut body =
        kiln_deck::TextBody::from_paragraphs(vec![para], Default::default()).expect("a body");
    body.body = old.body.clone();
    body.list_style = old.list_style.clone();
    d.apply(
        "test",
        kiln_deck::Transaction::new(vec![kiln_deck::Op::SetTextBody {
            shape: id,
            text: Some(Box::new(body)),
        }]),
    )
    .expect("title");
    let firing = vec![("Fir".to_owned(), true), ("ing".to_owned(), false)];
    assert_eq!(title_runs(&d, 1), firing);
    // Enter in the title: each half keeps its runs.
    outline(
        &mut d,
        E::Split {
            slide: 1,
            line: 0,
            offset: 2,
        },
    );
    assert_eq!(title_runs(&d, 1), [("Fi".to_owned(), true)]);
    assert_eq!(
        title_runs(&d, 2),
        [("r".to_owned(), true), ("ing".to_owned(), false)]
    );
    // Backspace at the start of the second joins them again.
    outline(&mut d, E::Join { slide: 2, line: 0 });
    assert_eq!(title_runs(&d, 1), firing);
    // Shift+Tab then Tab: the line and the title it becomes keep their runs too.
    outline(
        &mut d,
        E::Split {
            slide: 1,
            line: 0,
            offset: 3,
        },
    );
    outline(
        &mut d,
        E::Indent {
            slide: 2,
            line: 0,
            by: 1,
        },
    );
    let line = &d.outline_view()[1].lines[0];
    assert_eq!((line.level, line.text.as_str()), (0, "ing"));
    assert_eq!(title_runs(&d, 1), [("Fir".to_owned(), true)]);
}

#[test]
fn the_outline_says_which_slides_hold_more_than_their_text() {
    let mut d = with_bullets(2);
    assert!(!d.outline_view()[1].more);
    run(
        &mut d,
        D::SetNotes {
            text: "Say hello".into(),
        },
    );
    let o = d.outline_view();
    assert!(o[1].more);
    assert!(!o[0].more);
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
        D::InsertShape {
            preset: "rect".into(),
            rect: None,
        },
    );
    assert!(d.outline_view()[0].more);
}

#[test]
fn reuse_slides_come_on_this_presentations_layouts() {
    let mut d = with_bullets(1);
    let bytes = fixture_bytes("pictures.pptx");
    let list = d.reuse_list(&bytes, 120.0, 1.0).expect("list");
    assert!(!list.is_empty());
    assert!(list.iter().all(|s| s.png.starts_with(b"\x89PNG")));
    let other = kiln_io_pptx::read_pptx(&bytes).expect("reads").presentation;
    run(
        &mut d,
        D::ReuseSlides {
            bytes: bytes.clone(),
            slides: vec![0],
        },
    );
    assert_eq!(d.pres.slides.len(), 3);
    assert_eq!(d.view.current, 2);
    let masters: Vec<_> = d.pres.masters[0].layouts.iter().map(|l| l.id).collect();
    assert!(masters.contains(&d.pres.slides[2].layout));
    // Its pictures came along.
    let mut refs = Vec::new();
    for s in &d.pres.slides[2].shapes {
        if let ShapeKind::Picture(p) = &s.kind {
            refs.push(p.media.clone());
        }
    }
    for r in &refs {
        assert!(d.pres.parts.contains_key(r), "{r}");
    }
    // Reusing the same slide again keeps one copy of identical pictures.
    let parts = d.pres.parts.len();
    run(
        &mut d,
        D::ReuseSlides {
            bytes,
            slides: vec![0],
        },
    );
    assert_eq!(d.pres.parts.len(), parts);
    assert!(other.slides.len() >= list.len());
    let saved = d.write(FileFormat::Pptx, None).expect("saves");
    assert_eq!(
        kiln_io_pptx::read_pptx(&saved)
            .expect("reads")
            .presentation
            .slides
            .len(),
        4
    );
}

/// A 2 × 2 PNG of one colour.
fn png(rgb: [u8; 3]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

/// The picture part of a slide's background.
fn background_part(s: &kiln_deck::Slide) -> Option<String> {
    match &s.background {
        Some(kiln_deck::Background::Fill {
            fill: kiln_ooxml::drawingml::fill::Fill::Blip(b),
            ..
        }) => b.embed.clone(),
        _ => None,
    }
}

#[test]
fn reused_slides_keep_their_animations_and_picture_background() {
    // The other presentation: a picture background and a shape that fades in.
    let red = png([200, 30, 30]);
    let mut src = DeckDoc::new(Presentation::new(), None, None);
    run(
        &mut src,
        D::SetBackground {
            fill: Some(kiln_protocol::commands::DeckFill::Picture {
                name: "red.png".into(),
                bytes: red.clone(),
            }),
            all: false,
        },
    );
    run(
        &mut src,
        D::InsertShape {
            preset: "rect".into(),
            rect: None,
        },
    );
    run(
        &mut src,
        D::AddAnimation {
            effect: "fade".into(),
        },
    );
    let bytes = src.write(FileFormat::Pptx, None).expect("saves");
    let theirs = kiln_io_pptx::read_pptx(&bytes).expect("reads").presentation;
    let their_bg = background_part(&theirs.slides[0]).expect("a picture background");
    // This one already has a different picture under the same name.
    let blue = png([30, 30, 200]);
    let mut d = DeckDoc::new(Presentation::new(), None, None);
    run(
        &mut d,
        D::InsertPicture {
            name: "blue.png".into(),
            bytes: blue.clone(),
            rect: None,
        },
    );
    let mine = d.pres.slides[0]
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ShapeKind::Picture(p) => Some(p.media.clone()),
            _ => None,
        })
        .expect("a picture");
    assert_eq!(mine, their_bg, "the names clash");
    run(
        &mut d,
        D::ReuseSlides {
            bytes,
            slides: vec![0],
        },
    );
    let check = |d: &DeckDoc| {
        let slide = &d.pres.slides[1];
        let bg = background_part(slide).expect("the background came along");
        assert_eq!(d.pres.parts[&bg].bytes, red);
        let pic = d.pres.slides[0]
            .shapes
            .iter()
            .find_map(|s| match &s.kind {
                ShapeKind::Picture(p) => Some(p.media.clone()),
                _ => None,
            })
            .expect("a picture");
        assert_eq!(d.pres.parts[&pic].bytes, blue);
        assert_eq!(slide.timeline.sequence.len(), 1);
        let target = slide.timeline.sequence[0].target.shape;
        assert!(
            slide.shapes.iter().any(|s| s.id == target),
            "the effect follows its shape"
        );
    };
    check(&d);
    let saved = d.write(FileFormat::Pptx, None).expect("saves");
    let read = kiln_io_pptx::read_pptx(&saved).expect("reads");
    let (pres, original) = crate::apps::deck::save::Original::from_read(read, &saved);
    check(&DeckDoc::new(pres, Some(original), None));
}

/// A 2 × 1 PNG (a poster frame).
fn poster() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 1, image::Rgba([200, 80, 40, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

#[test]
fn video_and_audio_are_embedded_with_their_poster_and_saved() {
    use kiln_protocol::commands::deck_features::DeckMediaKind;
    let mut d = with_bullets(1);
    let clip = b"\x00\x00\x00\x18ftypmp42 not a real clip".to_vec();
    run(
        &mut d,
        D::InsertMedia {
            kind: DeckMediaKind::Video,
            name: "kiln.mp4".into(),
            mime: "video/mp4".into(),
            bytes: clip.clone(),
            poster: poster(),
            width: 1920,
            height: 1080,
        },
    );
    run(
        &mut d,
        D::InsertMedia {
            kind: DeckMediaKind::Audio,
            name: "bell.mp3".into(),
            mime: "audio/mpeg".into(),
            bytes: b"ID3 bell".to_vec(),
            poster: poster(),
            width: 96,
            height: 96,
        },
    );
    let m = d.slide_media().expect("media");
    assert_eq!(m.items.len(), 2);
    let video = &m.items[0];
    assert_eq!(video.mime, "video/mp4");
    // Fitted to two thirds of the slide, centred, 16:9.
    assert!((video.rect.width / video.rect.height - 16.0 / 9.0).abs() < 0.01);
    assert!((video.rect.x * 2.0 + video.rect.width - m.width).abs() < 1.0);
    assert_eq!(m.items[1].rect.width, 48.0);
    let part = video.part.clone().expect("embedded");
    assert_eq!(d.media_bytes(&part).expect("bytes"), clip);
    assert!(d.media_bytes("/ppt/media/none.mp4").is_err());
    // One undo step each; saved and read back as media.
    let bytes = d.write(FileFormat::Pptx, None).expect("saves");
    let back = kiln_io_pptx::read_pptx(&bytes).expect("reads").presentation;
    let kinds: Vec<_> = back.slides[1]
        .shapes
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Media(m) => Some((m.kind, m.poster.is_some())),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (kiln_deck::shape::MediaKind::Video, true),
            (kiln_deck::shape::MediaKind::Audio, true)
        ]
    );
    assert!(d.undo() && d.undo());
    assert!(d.slide_media().expect("media").items.is_empty());
    let err = d
        .execute(&Command::Deck(D::InsertMedia {
            kind: DeckMediaKind::Video,
            name: "x.mp4".into(),
            mime: "video/mp4".into(),
            bytes: clip,
            poster: b"not a picture".to_vec(),
            width: 1,
            height: 1,
        }))
        .expect_err("a poster is a picture");
    assert!(err.to_string().contains("poster"), "{err}");
}

#[test]
fn equations_are_office_math_drawn_as_their_linear_text() {
    let mut d = with_bullets(1);
    let shapes = d.pres.slides[1].shapes.len();
    run(
        &mut d,
        D::InsertEquation {
            linear: "a^2+b^2=c^2".into(),
        },
    );
    // A new text box with the equation, and the caret after it.
    let slide = &d.pres.slides[1];
    assert_eq!(slide.shapes.len(), shapes + 1);
    let body = slide
        .shapes
        .last()
        .and_then(kiln_deck::Shape::text)
        .expect("text");
    let para = body.paragraphs().next().expect("a paragraph");
    let eq = para
        .content
        .objects_with_offsets()
        .find_map(|(_, o)| match o {
            kiln_doc::InlineObject::Equation(e) => Some(e.clone()),
            _ => None,
        })
        .expect("an equation");
    assert!(eq.omml.starts_with("<m:oMath>"));
    assert_eq!(eq.linear_text(), "a²+b²=c²");
    // Typed after it, in the same text.
    run(
        &mut d,
        D::InsertText {
            text: " (Pythagoras)".into(),
        },
    );
    let o = d.pres.slides[1]
        .shapes
        .last()
        .and_then(kiln_deck::Shape::text)
        .map(kiln_deck::TextBody::text)
        .unwrap_or_default();
    assert!(o.ends_with(" (Pythagoras)"), "{o:?}");
    // Saved as Office Math and read back.
    let bytes = d.write(FileFormat::Pptx, None).expect("saves");
    let back = kiln_io_pptx::read_pptx(&bytes).expect("reads").presentation;
    let saved = back.slides[1]
        .shapes
        .last()
        .and_then(kiln_deck::Shape::text)
        .and_then(|t| t.paragraphs().next().cloned())
        .expect("saved paragraph");
    assert!(
        saved
            .content
            .objects_with_offsets()
            .any(|(_, o)| matches!(o, kiln_doc::InlineObject::Equation(_)))
    );
    assert!(
        d.execute(&Command::Deck(D::InsertEquation {
            linear: "  ".into()
        }))
        .is_err()
    );
}

/// Writes previews of the print layouts, Design Ideas and exported pictures as PNG for visual
/// review (`KILN_DECK_PRINT_REVIEW=dir`).
#[test]
fn print_review_renders() {
    let Some(dir) = std::env::var_os("KILN_DECK_PRINT_REVIEW") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("dir");
    for (file, name, slides) in [
        ("placeholders.pptx", "slides", None),
        (
            "placeholders.pptx",
            "handouts-3",
            layout(SlideLayout::Handouts, 3),
        ),
        (
            "pictures.pptx",
            "handouts-6",
            layout(SlideLayout::Handouts, 6),
        ),
        ("notes.pptx", "notes", layout(SlideLayout::NotesPages, 1)),
        (
            "many-slides.pptx",
            "outline",
            layout(SlideLayout::Outline, 1),
        ),
    ] {
        let d = open(file);
        let settings = PrintSettings {
            slides: slides.map(|s| SlidePrint { frame: true, ..s }),
            ..PrintSettings::default()
        };
        let PrintOutput::Preview(p) = d
            .print(&PrintJob::Preview {
                settings: &settings,
                sheet: 0,
                scale: 1.5,
            })
            .expect("preview")
        else {
            panic!("a preview")
        };
        let e = p.scene.extent;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let view = kiln_raster::View {
            width: (e.w * 1.5) as u32,
            height: (e.h * 1.5) as u32,
            scroll_x: e.x,
            scroll_y: e.y,
            options: kiln_raster::PaintOptions::default(),
        };
        let png = kiln_raster::render_delta(&p.scene, &view, &kiln_scene::Palette::light())
            .expect("render")
            .encode_png()
            .expect("png");
        std::fs::write(dir.join(format!("{name}.png")), png).expect("write");
    }
    for (file, slide) in [
        ("placeholders.pptx", 1),
        ("pictures.pptx", 0),
        ("tables.pptx", 0),
    ] {
        let mut d = open(file);
        run(
            &mut d,
            D::SelectSlide {
                index: slide,
                extend: false,
                toggle: false,
            },
        );
        for i in d.design_ideas(320.0, 1.0).expect("ideas") {
            let stem = file.trim_end_matches(".pptx");
            std::fs::write(dir.join(format!("idea-{stem}-{}.png", i.id)), i.png).expect("write");
        }
    }
    let d = with_bullets(7);
    let mut d = d;
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    for i in d.design_ideas(320.0, 1.0).expect("ideas") {
        std::fs::write(dir.join(format!("idea-bullets-{}.png", i.id)), i.png).expect("write");
    }
    run(
        &mut d,
        D::InsertEquation {
            linear: "x=(-b\\pm\\sqrt(b^2-4ac))/(2a)".into(),
        },
    );
    let f = d
        .export_pictures(Some(&[1]), DeckPictureFormat::Png, 960)
        .expect("png");
    std::fs::write(dir.join("equation.png"), f.bytes).expect("write");
}
