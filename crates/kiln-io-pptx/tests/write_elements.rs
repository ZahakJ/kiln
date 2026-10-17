// SPDX-License-Identifier: AGPL-3.0-or-later
//! Construct by construct: a presentation built in Kiln (no file behind it) with every kind of
//! shape, text, motion and annotation the model holds is written, read back and compared field
//! by field; LibreOffice must open it.

mod read_support;

use kiln_common::NodeId;
use kiln_deck::animation::{AnimationEffect, Effect, EffectStart, TriggerSequence};
use kiln_deck::geom::{ChildSpace, Rect, Transform};
use kiln_deck::shape::{
    AutoShape, CellProps, ChartRef, Connection, Connector, Crop, Geometry, Group, Media, MediaKind,
    MediaSource, Picture, Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps, Table,
};
use kiln_deck::slide::{Background, Comment, Section, Slide};
use kiln_deck::text::TextBody;
use kiln_deck::transition::{Direction8, Side, Transition, TransitionEffect};
use kiln_deck::{Bullet, DeckParaProps, LayoutKind, Presentation};
use kiln_doc::RunProps;
use kiln_doc::fields::{Field, FieldForm};
use kiln_doc::props::DocColor;
use kiln_doc::st::BreakKind;
use kiln_doc::{Break, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark, Paragraph};
use kiln_io_pptx::{PptxRead, WriteOptions, body_text, read_pptx, write_pptx};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::{BlipFill, Fill};
use kiln_ooxml::drawingml::line::Line;
use kiln_ooxml::opc::OpcPackage;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D,
    0x1D, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn concat(mut a: Fragment, b: Fragment) -> Fragment {
    let base = a.text.chars().count();
    a.text.push_str(&b.text);
    a.runs.extend(b.runs);
    a.marks.extend(b.marks.into_iter().map(|mut m| {
        m.start += base;
        m.end += base;
        m
    }));
    a.objects.extend(b.objects);
    a
}

/// Rich text: bold, a coloured run, a link to a web page, a jump to a slide, a field, a line
/// break; a second paragraph with a picture bullet and spacing.
fn rich_text(jump_to: NodeId) -> TextBody {
    let bold = RunProps {
        bold: Some(true),
        size: Some(48),
        ..RunProps::default()
    };
    let red = RunProps {
        color: Some(DocColor::rgb(0xC0_30_20)),
        italic: Some(true),
        ..RunProps::default()
    };
    let link = |url: &str| {
        Mark::Link(Hyperlink {
            target: LinkTarget::Url(url.to_owned()),
            tooltip: Some("Open <it> & see".to_owned()),
            frame: None,
            history: true,
        })
    };
    let mut f = Fragment::text("Bold ", bold);
    f = concat(f, Fragment::text("red ", red));
    f = concat(
        f,
        Fragment::text("web", RunProps::default()).with_mark(link("https://example.com/a?b=1&c=2")),
    );
    f = concat(f, Fragment::text(" ", RunProps::default()));
    f = concat(
        f,
        Fragment::text("slide", RunProps::default())
            .with_mark(link(&format!("#slide:{}", jump_to.0))),
    );
    f = concat(
        f,
        Fragment::object(
            InlineObject::Break(Break {
                kind: BreakKind::TextWrapping,
                clear: None,
            }),
            RunProps::default(),
        ),
    );
    f = concat(
        f,
        Fragment::object(
            InlineObject::Field(Field {
                instr: "slidenum".into(),
                result: "‹#›".into(),
                form: FieldForm::Simple,
                dirty: false,
                locked: false,
            }),
            RunProps::default(),
        ),
    );
    f = concat(
        f,
        Fragment::text(" _x0041_ snake_case\ttab", RunProps::default()),
    );
    let mut p1 = Paragraph::new(NodeId(1));
    p1.content = Inlines::from_fragment(f).expect("inlines");
    let mut p2 = Paragraph::new(NodeId(2));
    p2.content = Inlines::from_fragment(Fragment::text("Second paragraph", RunProps::default()))
        .expect("inlines");
    p2.props.outline_level = Some(1);
    let deck = DeckParaProps {
        bullet: Some(Bullet::Picture("/ppt/media/image1.png".into())),
        space_before: Some(kiln_deck::TextSpacing::Points(600)),
        ..DeckParaProps::default()
    };
    TextBody::from_paragraphs(vec![p1, p2], [(NodeId(2), deck)].into_iter().collect())
        .expect("body")
}

fn shape(p: &mut Presentation, name: &str, rect: Rect, kind: ShapeKind) -> Shape {
    Shape::new(p.alloc_id(), name, Some(Transform::from_rect(rect)), kind)
}

/// The test presentation and the ids the checks need.
struct Built {
    pres: Presentation,
    box_id: NodeId,
    other_id: NodeId,
}

fn build() -> Built {
    let mut p = Presentation::new();
    p.parts.insert(
        "/ppt/media/image1.png".into(),
        kiln_deck::Part {
            content_type: "image/png".into(),
            bytes: PNG.to_vec(),
        },
    );
    p.parts.insert(
        "/ppt/media/media1.mp4".into(),
        kiln_deck::Part {
            content_type: "video/mp4".into(),
            bytes: b"not really a video".to_vec(),
        },
    );
    // A chart with its workbook, borrowed from the chart fixture.
    let charts = read_support::read_fixture("pptx/features/charts.pptx");
    let chart_key = charts
        .presentation
        .parts
        .keys()
        .find(|k| k.starts_with("/ppt/charts/chart") && k.ends_with(".xml"))
        .cloned()
        .expect("a chart part");
    for (k, v) in &charts.presentation.parts {
        if k.starts_with("/ppt/charts/") || k.starts_with("/ppt/embeddings/") {
            p.parts.insert(k.clone(), v.clone());
        }
    }
    let layout = p.masters[0]
        .layout_of_kind(&LayoutKind::TitleContent)
        .map(|l| l.id)
        .expect("title and content");
    let second = Slide::new(p.alloc_id(), layout);
    let second_id = second.id;
    let mut first = Slide::new(p.alloc_id(), layout);

    // Text box with rich text, a placeholder title, a preset shape with adjustments.
    let mut tb = shape(
        &mut p,
        "Text & <box>",
        Rect::new(457_200, 457_200, 4_572_000, 1_371_600),
        ShapeKind::Geometry(AutoShape {
            text_box: true,
            props: ShapeProps {
                geometry: Some(Geometry::preset("rect")),
                fill: Some(Fill::None),
                ..ShapeProps::default()
            },
            text: Some(rich_text(second_id)),
        }),
    );
    tb.alt_text = Some("A box with text".into());
    tb.title = Some("Box title".into());
    tb.link = Some("https://kiln.example/".into());
    let box_id = tb.id;
    let mut title = shape(
        &mut p,
        "Title 1",
        Rect::new(0, 0, 1, 1),
        ShapeKind::Geometry(AutoShape {
            text_box: false,
            props: ShapeProps::default(),
            text: Some(TextBody::from_text("A title\u{b}on two lines")),
        }),
    );
    title.xfrm = None;
    title.placeholder = Some(Placeholder::new(PlaceholderType::Title, 0));
    let mut star = Shape::geometry(
        p.alloc_id(),
        "roundRect",
        Rect::new(5_000_000, 500_000, 2_000_000, 1_000_000),
    );
    if let ShapeKind::Geometry(a) = &mut star.kind {
        a.props.geometry = Some(Geometry::Preset {
            name: "roundRect".into(),
            adjust: vec![("adj".into(), 30_000)],
        });
        a.props.line = Some(Line {
            fill: Some(Fill::Solid(DrawingColor::scheme("accent2"))),
            ..Line::default()
        });
    }
    let other_id = star.id;
    // Picture with crop and transparency.
    let pic = shape(
        &mut p,
        "Picture 7",
        Rect::new(100_000, 3_000_000, 1_000_000, 1_000_000),
        ShapeKind::Picture(Picture {
            media: "/ppt/media/image1.png".into(),
            crop: Crop {
                l: 10_000,
                t: 0,
                r: 5_000,
                b: -2_000,
            },
            props: ShapeProps {
                geometry: Some(Geometry::preset("ellipse")),
                ..ShapeProps::default()
            },
            alpha: Some(60_000),
        }),
    );
    // Video (embedded, with poster and trim) and audio (linked).
    let video = shape(
        &mut p,
        "Video 8",
        Rect::new(1_200_000, 3_000_000, 1_000_000, 800_000),
        ShapeKind::Media(Media {
            kind: MediaKind::Video,
            source: MediaSource::Embedded("/ppt/media/media1.mp4".into()),
            poster: Some("/ppt/media/image1.png".into()),
            trim_start_ms: 1500,
            trim_end_ms: 250,
            props: ShapeProps::default(),
        }),
    );
    let audio = shape(
        &mut p,
        "Audio 9",
        Rect::new(2_400_000, 3_000_000, 300_000, 300_000),
        ShapeKind::Media(Media {
            kind: MediaKind::Audio,
            source: MediaSource::Linked("https://example.com/sound.mp3".into()),
            poster: None,
            trim_start_ms: 0,
            trim_end_ms: 0,
            props: ShapeProps::default(),
        }),
    );
    // A table with a merged header and a filled cell.
    let mut table = Table::with_size(2, 3, 3_000_000, 800_000);
    table.rows[0].cells[0].grid_span = 2;
    table.rows[0].cells[1].h_merge = true;
    table.rows[0].cells[0].text = TextBody::from_text("Merged");
    table.rows[1].cells[2].text = TextBody::from_text("Cell");
    table.rows[1].cells[2].props = CellProps {
        fill: Some(Fill::Solid(DrawingColor::srgb("FFEEAA"))),
        margins: [Some(0), None, Some(45_720), None],
        ..CellProps::default()
    };
    let table = shape(
        &mut p,
        "Table 10",
        Rect::new(3_000_000, 3_000_000, 3_000_000, 800_000),
        ShapeKind::Table(table),
    );
    let chart = shape(
        &mut p,
        "Chart 11",
        Rect::new(6_500_000, 3_000_000, 4_000_000, 3_000_000),
        ShapeKind::Chart(ChartRef { part: chart_key }),
    );
    // A group with its own coordinate space.
    let a = Shape::geometry(p.alloc_id(), "ellipse", Rect::new(0, 0, 100, 100));
    let b = Shape::geometry(p.alloc_id(), "triangle", Rect::new(100, 0, 100, 100));
    let group = shape(
        &mut p,
        "Group 12",
        Rect::new(8_000_000, 500_000, 2_000_000, 1_000_000),
        ShapeKind::Group(Group {
            child_space: ChildSpace {
                x: 0,
                y: 0,
                cx: 200,
                cy: 100,
            },
            children: vec![a, b],
            props: ShapeProps::default(),
        }),
    );
    // A connector glued to the box and the star.
    let connector = shape(
        &mut p,
        "Connector 13",
        Rect::new(5_029_200, 1_000_000, 100_000, 100_000),
        ShapeKind::Connector(Connector {
            props: ShapeProps {
                geometry: Some(Geometry::preset("straightConnector1")),
                line: Some(Line::default()),
                ..ShapeProps::default()
            },
            start: Some(Connection {
                shape: box_id,
                site: 3,
            }),
            end: Some(Connection {
                shape: other_id,
                site: 1,
            }),
        }),
    );
    first.shapes = vec![
        title, tb, star, pic, video, audio, table, chart, group, connector,
    ];
    first.name = Some("Everything".into());
    first.transition = Some(Transition {
        effect: TransitionEffect::Push { dir: Side::Up },
        duration_ms: 750,
        advance_on_click: false,
        advance_after_ms: Some(4000),
        original: None,
    });
    // Every gallery effect, alternating start modes, and a trigger sequence.
    let effects = [
        AnimationEffect::Appear,
        AnimationEffect::Fade,
        AnimationEffect::FlyIn {
            from: Direction8::LeftDown,
        },
        AnimationEffect::FloatIn { down: true },
        AnimationEffect::Split {
            vertical: true,
            inward: false,
        },
        AnimationEffect::Wipe { from: Side::Right },
        AnimationEffect::Zoom,
        AnimationEffect::GrowTurn,
        AnimationEffect::Pulse,
        AnimationEffect::ColorPulse {
            color: Some(DrawingColor::scheme("accent3")),
        },
        AnimationEffect::Teeter,
        AnimationEffect::Spin { degrees: -90 },
        AnimationEffect::GrowShrink { scale_pct: 80 },
        AnimationEffect::Disappear,
        AnimationEffect::FadeOut,
        AnimationEffect::FlyOut { to: Direction8::Up },
        AnimationEffect::ZoomOut,
    ];
    for (i, e) in effects.into_iter().enumerate() {
        let mut fx = Effect::new(p.alloc_id(), if i % 2 == 0 { box_id } else { other_id }, e);
        fx.start = [
            EffectStart::OnClick,
            EffectStart::WithPrevious,
            EffectStart::AfterPrevious,
        ][i % 3];
        fx.delay_ms = (i as u32) * 100;
        fx.duration_ms = fx.duration_ms.max(2) + 10 * i as u32;
        if i == 4 {
            fx.target.paragraphs = Some((0, 0));
        }
        if i == 5 {
            fx.repeat = 3;
        }
        first.timeline.sequence.push(fx);
    }
    first.timeline.triggers.push(TriggerSequence {
        trigger: other_id,
        effects: vec![Effect::new(p.alloc_id(), box_id, AnimationEffect::Pulse)],
    });
    first.notes = Some(kiln_deck::Notes {
        text: TextBody::from_text("Speaker notes\nsecond line"),
        shapes: Vec::new(),
    });
    first.comments.push(Comment {
        id: p.alloc_id(),
        author: "Ana Lima".into(),
        initials: "AL".into(),
        created: "2026-10-16T09:00:00.000".into(),
        text: "Legacy comment".into(),
        shape: None,
        pos: (914_400, 457_200),
        replies: Vec::new(),
        resolved: false,
    });

    let mut second = second;
    second.hidden = true;
    second.background = Some(Background::Fill {
        fill: Fill::Blip(BlipFill {
            embed: Some("/ppt/media/image1.png".into()),
            ..BlipFill::default()
        }),
        shade_to_title: false,
    });
    second.transition = Some(Transition::new(TransitionEffect::Morph {
        by: kiln_deck::transition::MorphBy::Characters,
    }));
    // The default title slide of a new presentation stays first.
    p.slides.push(first);
    p.slides.push(second);
    let ids: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    p.sections = vec![
        Section {
            id: p.alloc_id(),
            name: "Intro".into(),
            len: 1,
        },
        Section {
            id: p.alloc_id(),
            name: "Body & more".into(),
            len: ids.len() - 1,
        },
    ];
    p.refresh_id_counter();
    p.validate().expect("valid");
    Built {
        pres: p,
        box_id,
        other_id,
    }
}

/// A list style without the rendering the reader caches for theme colours (a presentation built
/// in Kiln has none until it is read).
fn uncached(s: &kiln_deck::ListStyle) -> kiln_deck::ListStyle {
    let mut out = s.clone();
    let fix = |l: &mut kiln_deck::LevelStyle| {
        if let Some(c) = l.run.color.as_mut()
            && c.theme_color.is_some()
        {
            c.val = kiln_doc::props::ColorValue::Auto;
        }
    };
    if let Some(mut d) = out.default_level().cloned() {
        fix(&mut d);
        out.set_default_level(Some(d));
    }
    for i in 0..kiln_deck::text::LEVELS {
        if let Some(mut l) = out.get_level(i).cloned() {
            fix(&mut l);
            out.set_level(i, l);
        }
    }
    out
}

/// Marks with slide jumps naming the slide's position (ids differ after a read).
fn jumps_by_index(p: &Presentation, marks: &[kiln_doc::MarkSpan]) -> Vec<kiln_doc::MarkSpan> {
    marks
        .iter()
        .cloned()
        .map(|mut m| {
            if let Mark::Link(h) = &mut m.mark
                && let LinkTarget::Url(u) = &mut h.target
                && let Some(id) = u
                    .strip_prefix("#slide:")
                    .and_then(|n| n.parse::<u64>().ok())
            {
                *u = format!("#slide-at:{:?}", p.slide_index(NodeId(id)));
            }
            m
        })
        .collect()
}

fn write_and_read(p: &Presentation) -> (Vec<u8>, PptxRead) {
    let out = write_pptx(p, None, &WriteOptions::default()).expect("writes");
    assert!(out.issues.is_empty(), "unexpected issues: {:?}", out.issues);
    let back = read_pptx(&out.bytes).expect("reads back");
    (out.bytes, back)
}

#[test]
fn a_new_presentation_with_every_construct_round_trips() {
    let built = build();
    let p = &built.pres;
    let (bytes, back) = write_and_read(p);
    let q = &back.presentation;
    // Only the linked clip is worth a note.
    assert!(
        back.warnings
            .iter()
            .all(|w| w.code == kiln_io_pptx::WarningCode::MediaLinked),
        "the reader complained: {:?}",
        back.warnings
    );
    assert_eq!(q.slides.len(), 3);
    assert_eq!(q.masters.len(), 1);
    assert_eq!(q.masters[0].layouts.len(), p.masters[0].layouts.len());
    assert_eq!(q.slide_size, p.slide_size);
    assert_eq!(q.masters[0].theme, p.masters[0].theme);
    assert_eq!(
        uncached(&q.masters[0].title_style),
        uncached(&p.masters[0].title_style)
    );
    assert_eq!(
        uncached(&q.masters[0].body_style),
        uncached(&p.masters[0].body_style)
    );
    assert_eq!(
        uncached(&q.default_text_style),
        uncached(&p.default_text_style)
    );
    let (a, b) = (&p.slides[1], &q.slides[1]);
    assert_eq!(b.name.as_deref(), Some("Everything"));
    assert_eq!(a.shapes.len(), b.shapes.len());
    for (x, y) in a.shapes.iter().zip(&b.shapes) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.kind.label(), y.kind.label(), "{}", x.name);
        assert_eq!(x.xfrm, y.xfrm, "{}", x.name);
        assert_eq!(x.placeholder, y.placeholder, "{}", x.name);
        assert_eq!(x.alt_text, y.alt_text, "{}", x.name);
        assert_eq!(x.title, y.title, "{}", x.name);
        assert_eq!(x.link, y.link, "{}", x.name);
        assert_eq!(x.props(), y.props(), "{}", x.name);
        match (&x.kind, &y.kind) {
            (ShapeKind::Geometry(m), ShapeKind::Geometry(n)) => {
                assert_eq!(m.text_box, n.text_box);
                let (t, u) = (m.text.as_ref().unwrap(), n.text.as_ref().unwrap());
                assert_eq!(body_text(t), body_text(u), "{}", x.name);
                let pa: Vec<_> = t.paragraphs().collect();
                let pb: Vec<_> = u.paragraphs().collect();
                for (i, (pa, pb)) in pa.iter().zip(&pb).enumerate() {
                    assert_eq!(pa.props, pb.props, "{} para {i}", x.name);
                    assert_eq!(pa.content.runs(), pb.content.runs(), "{} para {i}", x.name);
                    assert_eq!(
                        jumps_by_index(p, pa.content.marks()),
                        jumps_by_index(q, pb.content.marks()),
                        "{} para {i}",
                        x.name
                    );
                    assert_eq!(
                        pa.content.objects(),
                        pb.content.objects(),
                        "{} para {i}",
                        x.name
                    );
                    assert_eq!(
                        t.deck_props(pa.id),
                        u.deck_props(pb.id),
                        "{} para {i}",
                        x.name
                    );
                }
            }
            (ShapeKind::Picture(m), ShapeKind::Picture(n)) => {
                assert_eq!(m.media, n.media);
                assert_eq!(m.crop, n.crop);
                assert_eq!(m.alpha, n.alpha);
            }
            (ShapeKind::Media(m), ShapeKind::Media(n)) => assert_eq!(m, n),
            (ShapeKind::Table(m), ShapeKind::Table(n)) => assert_eq!(m, n),
            (ShapeKind::Chart(m), ShapeKind::Chart(n)) => assert_eq!(m, n),
            (ShapeKind::Group(m), ShapeKind::Group(n)) => {
                assert_eq!(m.child_space, n.child_space);
                assert_eq!(m.children.len(), n.children.len());
            }
            (ShapeKind::Connector(m), ShapeKind::Connector(n)) => {
                // Ends are glued to the same shapes (ids differ after a read).
                let name_of =
                    |pres: &Presentation, id: NodeId| pres.shape(id).map(|s| s.name.clone());
                assert_eq!(
                    m.start.map(|c| (name_of(p, c.shape), c.site)),
                    n.start.map(|c| (name_of(q, c.shape), c.site))
                );
                assert_eq!(
                    m.end.map(|c| (name_of(p, c.shape), c.site)),
                    n.end.map(|c| (name_of(q, c.shape), c.site))
                );
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(a.transition, b.transition);
    assert_eq!(a.timeline.sequence.len(), b.timeline.sequence.len());
    for (e, f) in a.timeline.sequence.iter().zip(&b.timeline.sequence) {
        assert_eq!(e.effect, f.effect);
        assert_eq!(e.start, f.start, "{:?}", e.effect);
        assert_eq!(e.delay_ms, f.delay_ms, "{:?}", e.effect);
        assert_eq!(e.duration_ms, f.duration_ms, "{:?}", e.effect);
        assert_eq!(e.repeat, f.repeat, "{:?}", e.effect);
        assert_eq!(e.target.paragraphs, f.target.paragraphs, "{:?}", e.effect);
        let target = |pres: &Presentation, id: NodeId| pres.shape(id).map(|s| s.name.clone());
        assert_eq!(target(p, e.target.shape), target(q, f.target.shape));
    }
    assert_eq!(b.timeline.triggers.len(), 1);
    assert_eq!(
        q.shape(b.timeline.triggers[0].trigger).map(|s| s.id),
        q.shape(b.timeline.triggers[0].trigger).map(|s| s.id)
    );
    assert_eq!(b.notes_text(), "Speaker notes\nsecond line");
    assert_eq!(b.comments.len(), 1);
    assert_eq!(b.comments[0].author, "Ana Lima");
    assert_eq!(b.comments[0].pos, (914_400, 457_200));
    assert!(!back.extras.modern_comments);
    let (c, d) = (&p.slides[2], &q.slides[2]);
    assert!(d.hidden);
    assert_eq!(c.background, d.background);
    assert_eq!(c.transition, d.transition);
    assert_eq!(
        q.sections
            .iter()
            .map(|s| (&s.name, s.len))
            .collect::<Vec<_>>(),
        p.sections
            .iter()
            .map(|s| (&s.name, s.len))
            .collect::<Vec<_>>()
    );
    let _ = (built.box_id, built.other_id);
    // KILN_KEEP_OUTPUT=<dir> keeps the file for a look.
    if let Some(dir) = std::env::var_os("KILN_KEEP_OUTPUT") {
        let _ = std::fs::write(std::path::Path::new(&dir).join("everything.pptx"), &bytes);
    }

    // LibreOffice opens it.
    if let Ok(oracle) = kiln_testing::oracle::Oracle::locate() {
        let dir = std::env::temp_dir().join(format!("kiln-pptx-elements-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        let file = dir.join("everything.pptx");
        std::fs::write(&file, &bytes).expect("written");
        assert!(
            oracle.opens(&file, &dir),
            "LibreOffice cannot open the file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn the_package_is_laid_out_like_powerpoint_s() {
    let built = build();
    let out = write_pptx(&built.pres, None, &WriteOptions::default()).expect("writes");
    let pkg = OpcPackage::open(&out.bytes).expect("opens");
    let main = pkg.main_document_part().unwrap().unwrap();
    assert_eq!(main, "/ppt/presentation.xml");
    for (part, ct) in [
        ("/ppt/slides/slide1.xml", "presentationml.slide+xml"),
        (
            "/ppt/slideLayouts/slideLayout1.xml",
            "presentationml.slideLayout+xml",
        ),
        (
            "/ppt/slideMasters/slideMaster1.xml",
            "presentationml.slideMaster+xml",
        ),
        (
            "/ppt/notesMasters/notesMaster1.xml",
            "presentationml.notesMaster+xml",
        ),
        (
            "/ppt/notesSlides/notesSlide1.xml",
            "presentationml.notesSlide+xml",
        ),
        ("/ppt/theme/theme1.xml", "theme+xml"),
        ("/ppt/comments/comment1.xml", "presentationml.comments+xml"),
        (
            "/ppt/commentAuthors.xml",
            "presentationml.commentAuthors+xml",
        ),
        ("/docProps/core.xml", "core-properties+xml"),
        ("/docProps/app.xml", "extended-properties+xml"),
        ("/ppt/media/image1.png", "image/png"),
    ] {
        let got = pkg
            .content_type(part)
            .unwrap_or_else(|| panic!("{part} is missing"));
        assert!(got.ends_with(ct), "{part}: {got}");
    }
    // The presentation relates its theme, masters, notes master, slides and authors.
    let rels = pkg.relationships(&main).unwrap();
    for t in [
        "theme",
        "slideMaster",
        "notesMaster",
        "slide",
        "commentAuthors",
    ] {
        assert!(
            rels.all()
                .iter()
                .any(|r| r.rel_type.ends_with(&format!("/{t}"))),
            "no {t} relationship"
        );
    }
    // Document properties count the slides.
    let app =
        String::from_utf8(pkg.part("/docProps/app.xml").unwrap().unwrap().into_owned()).unwrap();
    assert!(app.contains("<Slides>3</Slides>"), "{app}");
    assert!(app.contains("<HiddenSlides>1</HiddenSlides>"), "{app}");
    // A stamped save sets the time and the application.
    let stamped = write_pptx(
        &built.pres,
        None,
        &WriteOptions {
            now: Some("2026-10-16T12:00:00Z".into()),
            author: Some("Kiln Test".into()),
            ..WriteOptions::default()
        },
    )
    .expect("writes");
    let back = read_pptx(&stamped.bytes).expect("reads");
    assert_eq!(
        back.properties.core.modified.as_deref(),
        Some("2026-10-16T12:00:00Z")
    );
    assert_eq!(
        back.properties.core.last_modified_by.as_deref(),
        Some("Kiln Test")
    );
    assert_eq!(back.properties.app.application.as_deref(), Some("Kiln"));
    // Writing twice gives the same bytes.
    let again = write_pptx(&built.pres, None, &WriteOptions::default()).expect("writes");
    assert_eq!(again.bytes, out.bytes);
}

#[test]
fn threaded_comments_and_templates_have_their_own_parts() {
    let mut built = build();
    let slide = &mut built.pres.slides[1];
    slide.comments[0].resolved = true;
    slide.comments[0]
        .replies
        .push(kiln_deck::slide::CommentReply {
            id: NodeId(9_999_999),
            author: "Bea Ruiz".into(),
            created: "2026-10-16T10:00:00.000".into(),
            text: "Fixed.".into(),
        });
    built.pres.refresh_id_counter();
    let out = write_pptx(
        &built.pres,
        None,
        &WriteOptions {
            kind: Some(kiln_io_pptx::PptxKind::Template),
            ..WriteOptions::default()
        },
    )
    .expect("writes");
    let pkg = OpcPackage::open(&out.bytes).expect("opens");
    assert_eq!(
        pkg.content_type("/ppt/presentation.xml"),
        Some(kiln_io_pptx::PptxKind::Template.content_type())
    );
    assert!(pkg.has("/ppt/authors.xml"));
    let back = read_pptx(&out.bytes).expect("reads");
    assert_eq!(back.kind, kiln_io_pptx::PptxKind::Template);
    assert!(back.extras.modern_comments);
    let c = &back.presentation.slides[1].comments[0];
    assert!(c.resolved);
    assert_eq!(c.replies.len(), 1);
    assert_eq!(c.replies[0].text, "Fixed.");
    assert_eq!(c.replies[0].author, "Bea Ruiz");
    assert_eq!(c.pos, (914_400, 457_200));
}
