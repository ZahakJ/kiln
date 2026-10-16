// SPDX-License-Identifier: AGPL-3.0-or-later
//! The inheritance resolution matrix: for each inherited property, every combination of where
//! it is set (master placeholder, layout placeholder, slide placeholder) resolves to the
//! nearest; placeholder matching by type and index as PowerPoint does; text styles, backgrounds
//! and colour maps.

use kiln_doc::RunProps;
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;

use super::*;
use crate::edit::tests::{layout, ph_of, sample};
use crate::geom::{Rect, Transform};
use crate::presentation::Presentation;
use crate::shape::{Geometry, PlaceholderType, ShapeStyle};
use crate::slide::{Background, LayoutKind};
use crate::text::{Autofit, Bullet, DeckParaProps, LevelStyle, TextAnchor};

/// A presentation with one Title and Content slide; returns it with the ids of the slide's
/// body placeholder, the layout's and the master's.
fn chain_fixture() -> (Presentation, NodeId, NodeId, NodeId) {
    let mut p = Presentation::new();
    let l = layout(&p, LayoutKind::TitleContent);
    let tx = crate::edit::insert_slide(&mut p, l, 1).unwrap();
    p.apply(tx).unwrap();
    let s = p.slides[1].id;
    let slide_body = ph_of(&p, s, PlaceholderType::Object);
    let layout_body = p
        .layout(l)
        .unwrap()
        .1
        .shapes
        .iter()
        .find(|x| x.placeholder.is_some_and(|ph| ph.idx == 1))
        .unwrap()
        .id;
    let master_body = p.masters[0]
        .shapes
        .iter()
        .find(|x| {
            x.placeholder
                .is_some_and(|ph| ph.kind == PlaceholderType::Body)
        })
        .unwrap()
        .id;
    (p, slide_body, layout_body, master_body)
}

/// Runs `set(p, shape, value)` on each level named in `mask` (bit 0 master, 1 layout, 2 slide)
/// with a value per level, and checks the resolved value is the nearest one set (or `none`).
fn matrix<T: PartialEq + std::fmt::Debug + Clone>(
    name: &str,
    values: [T; 3],
    none: Option<T>,
    clear: impl Fn(&mut Presentation, NodeId),
    set: impl Fn(&mut Presentation, NodeId, T),
    get: impl Fn(&Presentation, NodeId) -> Option<T>,
) {
    for mask in 0u8..8 {
        let (mut p, slide, lay, master) = chain_fixture();
        let levels = [master, lay, slide];
        for id in levels {
            clear(&mut p, id);
        }
        for (bit, id) in levels.iter().enumerate() {
            if mask & (1 << bit) != 0 {
                set(&mut p, *id, values[bit].clone());
            }
        }
        let expected = (0..3)
            .rev()
            .find(|bit| mask & (1 << bit) != 0)
            .map(|bit| values[bit].clone())
            .or_else(|| none.clone());
        assert_eq!(get(&p, slide), expected, "{name}: mask {mask:03b}");
        // The layout placeholder resolves through the master alone.
        let expected_layout = (0..2)
            .rev()
            .find(|bit| mask & (1 << bit) != 0)
            .map(|bit| values[bit].clone())
            .or_else(|| none.clone());
        assert_eq!(
            get(&p, lay),
            expected_layout,
            "{name} on the layout: mask {mask:03b}"
        );
    }
}

#[test]
fn position_and_size_inherit() {
    let xs = [
        Transform::new(1, 1, 10, 10),
        Transform::new(2, 2, 20, 20),
        Transform::new(3, 3, 30, 30),
    ];
    matrix(
        "xfrm",
        xs,
        None,
        |p, id| p.shape_mut(id).unwrap().xfrm = None,
        |p, id, v| p.shape_mut(id).unwrap().xfrm = Some(v),
        |p, id| p.resolved_xfrm(id),
    );
}

#[test]
fn shape_properties_inherit_property_by_property() {
    let fills = [
        Fill::Solid(DrawingColor::scheme("accent1")),
        Fill::Solid(DrawingColor::scheme("accent2")),
        Fill::Solid(DrawingColor::scheme("accent3")),
    ];
    matrix(
        "fill",
        fills,
        None,
        |p, id| p.shape_mut(id).unwrap().props_mut().unwrap().fill = None,
        |p, id, v| p.shape_mut(id).unwrap().props_mut().unwrap().fill = Some(v),
        |p, id| p.resolved_props(id).unwrap().fill,
    );
    // Geometry and fill come from different levels.
    let (mut p, slide, lay, master) = chain_fixture();
    p.shape_mut(master).unwrap().props_mut().unwrap().geometry =
        Some(Geometry::preset("roundRect"));
    p.shape_mut(lay).unwrap().props_mut().unwrap().fill = Some(Fill::None);
    let r = p.resolved_props(slide).unwrap();
    assert_eq!(r.geometry, Some(Geometry::preset("roundRect")));
    assert_eq!(r.fill, Some(Fill::None));
}

#[test]
fn body_properties_inherit_attribute_by_attribute() {
    matrix(
        "anchor",
        [TextAnchor::Top, TextAnchor::Middle, TextAnchor::Bottom],
        Some(TextAnchor::Top),
        |p, id| p.shape_mut(id).unwrap().text_mut().unwrap().body = Default::default(),
        |p, id, v| p.shape_mut(id).unwrap().text_mut().unwrap().body.anchor = Some(v),
        |p, id| p.resolved_body(id).map(|b| b.anchor),
    );
    matrix(
        "autofit",
        [Autofit::ResizeShape, Autofit::SHRINK, Autofit::Off],
        Some(Autofit::Off),
        |p, id| p.shape_mut(id).unwrap().text_mut().unwrap().body = Default::default(),
        |p, id, v| p.shape_mut(id).unwrap().text_mut().unwrap().body.autofit = Some(v),
        |p, id| p.resolved_body(id).map(|b| b.autofit),
    );
    // Insets inherit one side at a time.
    let (mut p, slide, lay, master) = chain_fixture();
    p.shape_mut(master).unwrap().text_mut().unwrap().body.insets =
        [Some(1), Some(1), Some(1), Some(1)];
    p.shape_mut(lay).unwrap().text_mut().unwrap().body.insets = [None, Some(2), None, None];
    p.shape_mut(slide).unwrap().text_mut().unwrap().body.insets = [None, None, Some(3), None];
    assert_eq!(p.resolved_body(slide).unwrap().insets, [1, 2, 3, 1]);
}

#[test]
fn list_styles_inherit_level_by_level() {
    let sizes = [20u32, 30, 40];
    matrix(
        "level 2 size",
        sizes,
        Some(40), // the master body style's second level: 20 pt (Kiln) = 40 half-points
        |p, id| p.shape_mut(id).unwrap().text_mut().unwrap().list_style = Default::default(),
        |p, id, v| {
            p.shape_mut(id)
                .unwrap()
                .text_mut()
                .unwrap()
                .list_style
                .set_level(
                    1,
                    LevelStyle {
                        run: RunProps {
                            size: Some(v),
                            ..RunProps::default()
                        },
                        ..LevelStyle::default()
                    },
                );
        },
        |p, id| p.level_style(id, 1).and_then(|l| l.run.size),
    );
    matrix(
        "bullet",
        [
            Bullet::Char("a".into()),
            Bullet::Char("b".into()),
            Bullet::None,
        ],
        Some(Bullet::Char("–".into())), // Kiln's second-level bullet
        |p, id| p.shape_mut(id).unwrap().text_mut().unwrap().list_style = Default::default(),
        |p, id, v| {
            p.shape_mut(id)
                .unwrap()
                .text_mut()
                .unwrap()
                .list_style
                .set_level(
                    1,
                    LevelStyle {
                        deck: DeckParaProps {
                            bullet: Some(v),
                            ..DeckParaProps::default()
                        },
                        ..LevelStyle::default()
                    },
                );
        },
        |p, id| p.level_style(id, 1).and_then(|l| l.deck.bullet),
    );
}

#[test]
fn paragraphs_and_runs_resolve_over_the_chain() {
    let (mut p, slide, _, master) = chain_fixture();
    // The master body style says 24 pt at level 1; a slide paragraph's own extras and a run's
    // direct size win.
    let tx = crate::edit::set_text(&p, crate::TextTarget::Shape(slide), "Hello").unwrap();
    p.apply(tx).unwrap();
    let body = p.shape(slide).unwrap().text().unwrap().clone();
    let para = body.paragraphs().next().unwrap().clone();
    let r = p.resolved_paragraph(slide, &body, &para).unwrap();
    assert_eq!(r.run.size, Some(48));
    assert_eq!(r.deck.bullet, Some(Bullet::Char("•".into())));
    let run = p.resolved_run(slide, &body, &para, 0).unwrap();
    assert_eq!(run.size, Some(48));
    let tx = crate::edit::set_para_extras(&p, crate::TextTarget::Shape(slide), &[para.id], |d| {
        d.bullet = Some(Bullet::None);
    })
    .unwrap();
    p.apply(tx).unwrap();
    let tx = crate::edit::edit_text(&p, crate::TextTarget::Shape(slide), |d| {
        kiln_doc::edit::format_runs(
            d,
            kiln_doc::Pos::new(para.id, 0),
            kiln_doc::Pos::new(para.id, 2),
            |r| r.size = Some(60),
        )
    })
    .unwrap();
    p.apply(tx).unwrap();
    let body = p.shape(slide).unwrap().text().unwrap().clone();
    let para = body.paragraph(para.id).unwrap().clone();
    let r = p.resolved_paragraph(slide, &body, &para).unwrap();
    assert_eq!(r.deck.bullet, Some(Bullet::None));
    assert_eq!(
        p.resolved_run(slide, &body, &para, 0).unwrap().size,
        Some(60)
    );
    assert_eq!(
        p.resolved_run(slide, &body, &para, 3).unwrap().size,
        Some(48)
    );
    // Past the end: the paragraph mark.
    assert_eq!(
        p.resolved_run(slide, &body, &para, 99).unwrap().size,
        Some(48)
    );
    let _ = master;
}

#[test]
fn placeholders_match_by_index_then_type() {
    let (p, slide, lay, master) = chain_fixture();
    let c = p.chain_of(slide).unwrap();
    assert_eq!(c.layout.map(|s| s.id), Some(lay));
    assert_eq!(c.master.map(|s| s.id), Some(master));
    // Titles match the title whatever their index.
    let lid = layout(&p, LayoutKind::TitleContent);
    let shapes = &p.layout(lid).unwrap().1.shapes;
    let t = match_layout_placeholder(shapes, &Placeholder::new(PlaceholderType::Title, 9)).unwrap();
    assert!(t.placeholder.unwrap().kind.is_title());
    // idx wins over type; unknown idx falls back to the type.
    let by_idx =
        match_layout_placeholder(shapes, &Placeholder::new(PlaceholderType::Body, 1)).unwrap();
    assert_eq!(by_idx.placeholder.unwrap().idx, 1);
    let by_type =
        match_layout_placeholder(shapes, &Placeholder::new(PlaceholderType::Date, 77)).unwrap();
    assert_eq!(by_type.placeholder.unwrap().kind, PlaceholderType::Date);
    assert!(
        match_layout_placeholder(shapes, &Placeholder::new(PlaceholderType::Picture, 42)).is_none()
    );
    // Layout → master by master type.
    let m = &p.masters[0].shapes;
    let get = |k| {
        match_master_placeholder(m, &Placeholder::new(k, 3)).map(|s| s.placeholder.unwrap().kind)
    };
    assert_eq!(
        get(PlaceholderType::CenteredTitle),
        Some(PlaceholderType::Title)
    );
    assert_eq!(get(PlaceholderType::Subtitle), Some(PlaceholderType::Body));
    assert_eq!(get(PlaceholderType::Picture), Some(PlaceholderType::Body));
    assert_eq!(
        get(PlaceholderType::SlideNumber),
        Some(PlaceholderType::SlideNumber)
    );
    assert_eq!(get(PlaceholderType::Header), None);
    // Shapes inside groups and non-placeholders inherit nothing.
    let q = sample();
    let group = q.slides[4]
        .shapes
        .iter()
        .find(|s| s.children().is_some())
        .unwrap();
    let child = q.chain_of(group.children().unwrap()[0].id).unwrap();
    assert!(child.layout.is_none() && child.master.is_none());
    let free = q.slides[2]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_none())
        .unwrap();
    let c = q.chain_of(free.id).unwrap();
    assert_eq!(c.ancestors().count(), 0);
    assert!(p.chain_of(NodeId(999_999)).is_none());
}

#[test]
fn text_style_kinds_follow_placeholder_types() {
    use crate::shape::Shape;
    let mk = |k: Option<PlaceholderType>| {
        let mut s = Shape::text_box(NodeId(1), Rect::new(0, 0, 1, 1), "");
        s.placeholder = k.map(|k| Placeholder::new(k, 0));
        TextStyleKind::of(&s)
    };
    assert_eq!(mk(Some(PlaceholderType::Title)), TextStyleKind::Title);
    assert_eq!(
        mk(Some(PlaceholderType::CenteredTitle)),
        TextStyleKind::Title
    );
    assert_eq!(mk(Some(PlaceholderType::Subtitle)), TextStyleKind::Body);
    assert_eq!(mk(Some(PlaceholderType::Object)), TextStyleKind::Body);
    assert_eq!(mk(Some(PlaceholderType::Footer)), TextStyleKind::Other);
    assert_eq!(mk(None), TextStyleKind::Other);
    // A text box uses the master's other style; a Quick Style's font reference colours it.
    let p = sample();
    let tb = p.slides[2]
        .shapes
        .iter()
        .find(|s| s.is_text_box())
        .unwrap()
        .id;
    let l = p.level_style(tb, 0).unwrap();
    assert_eq!(l.run.size, p.masters[0].other_style.level(0).run.size);
    let shape = p.slides[2]
        .shapes
        .iter()
        .find(|s| {
            s.props()
                .is_some_and(|x| x.style == Some(ShapeStyle::default_shape()))
        })
        .unwrap()
        .id;
    let l = p.level_style(shape, 0).unwrap();
    let color = l.run.color.unwrap();
    assert_eq!(
        color.theme_color,
        Some(kiln_doc::st::ThemeColorName::Light1)
    );
}

#[test]
fn backgrounds_and_colour_maps_inherit() {
    let (mut p, _, _, _) = chain_fixture();
    let s = p.slides[1].id;
    let lid = p.slides[1].layout;
    let owner = crate::Owner::Slide(s);
    let master_bg = p.masters[0].background.clone();
    assert_eq!(p.resolved_background(owner), Some(&master_bg));
    let lay_bg = Background::solid(DrawingColor::scheme("accent1"));
    p.layout_mut(lid).unwrap().background = Some(lay_bg.clone());
    assert_eq!(p.resolved_background(owner), Some(&lay_bg));
    let slide_bg = Background::solid(DrawingColor::scheme("accent2"));
    p.slide_mut(s).unwrap().background = Some(slide_bg.clone());
    assert_eq!(p.resolved_background(owner), Some(&slide_bg));
    assert_eq!(
        p.resolved_background(crate::Owner::Layout(lid)),
        Some(&lay_bg)
    );

    assert_eq!(p.resolved_color_map(owner), Some(ColorMapping::default()));
    p.layout_mut(lid).unwrap().color_map = Some(ColorMapping::dark());
    assert_eq!(p.resolved_color_map(owner), Some(ColorMapping::dark()));
    p.slide_mut(s).unwrap().color_map = Some(ColorMapping::default());
    assert_eq!(p.resolved_color_map(owner), Some(ColorMapping::default()));
    // The context maps tx1 through the map in force.
    p.slide_mut(s).unwrap().color_map = None;
    let ctx = p.color_context(owner).unwrap();
    let tx1 = kiln_ooxml::drawingml::color::resolve_color(&DrawingColor::scheme("tx1"), &ctx);
    let lt1 = kiln_ooxml::drawingml::color::resolve_color(&DrawingColor::scheme("lt1"), &ctx);
    assert_eq!(tx1, lt1, "dark map: text is light");
    // Master graphics.
    assert!(p.shows_master_shapes(s));
    p.layout_mut(lid).unwrap().show_master_shapes = false;
    assert!(!p.shows_master_shapes(s));
    assert!(!p.shows_master_shapes(NodeId(999_999)));
    assert_eq!(p.theme_of(owner).unwrap().name, "Kiln");
    assert!(p.master_of(crate::Owner::NotesMaster(NodeId(1))).is_some());
    assert!(p.layout_of(crate::Owner::Master(p.masters[0].id)).is_none());
}
