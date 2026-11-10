// SPDX-License-Identifier: AGPL-3.0-or-later
//! The bundled themes: every one parses, builds a complete master, applies to a five-slide
//! sample (and every variant too), keeps text readable, and survives storage. The format's
//! refusals.

use kiln_ooxml::drawingml::color::{Rgba, resolve_color};
use kiln_ooxml::drawingml::fill::Fill;

use super::*;
use crate::edit::tests::{roundtrip, sample};
use crate::presentation::Owner;
use crate::shape::{PlaceholderType, Shape};

/// The colour a background shows on average (solid fills exactly, gradients by their stops).
fn background_color(p: &Presentation, owner: Owner) -> Rgba {
    let ctx = p.color_context(owner).unwrap();
    match p.resolved_background(owner).unwrap() {
        Background::Fill {
            fill: Fill::Solid(c),
            ..
        } => resolve_color(c, &ctx),
        Background::Fill {
            fill: Fill::Gradient(g),
            ..
        } => {
            let n = g.stops.len().max(1) as f64;
            let sum = g.stops.iter().fold((0.0, 0.0, 0.0), |acc, s| {
                let c = resolve_color(&s.color, &ctx);
                (acc.0 + c.r, acc.1 + c.g, acc.2 + c.b)
            });
            Rgba::rgb(sum.0 / n, sum.1 / n, sum.2 / n)
        }
        Background::Theme { color: Some(c), .. } => resolve_color(c, &ctx),
        other => panic!("unexpected background {other:?}"),
    }
}

#[test]
fn contrast_follows_wcag() {
    let black = Rgba::rgb(0.0, 0.0, 0.0);
    let white = Rgba::rgb(255.0, 255.0, 255.0);
    assert!((contrast(black, white) - 21.0).abs() < 1e-9);
    assert!((contrast(white, white) - 1.0).abs() < 1e-9);
    // #767676 on white is the classic 4.54:1.
    let grey = Rgba::rgb(118.0, 118.0, 118.0);
    assert!((contrast(grey, white) - 4.54).abs() < 0.01);
}

#[test]
fn twelve_themes_are_bundled_and_parse() {
    assert_eq!(BUNDLED.len(), 12);
    let all = bundled();
    assert_eq!(all.len(), 12, "every bundled theme parses");
    let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 12, "ids are unique");
    for (t, b) in all.iter().zip(BUNDLED) {
        assert_eq!(t.id, b.id, "the id matches the file name");
        assert!(!t.name.is_empty() && !t.description.is_empty(), "{}", t.id);
        assert_eq!(t.variants.len(), 4, "{} has four variants", t.id);
        assert!(
            !t.theme.format_scheme.fill_styles.is_empty(),
            "{} has a format scheme",
            t.id
        );
        assert_eq!(t.theme.format_scheme.bg_fill_styles.len(), 3);
        assert!(!t.theme.font_scheme.major.latin.typeface.is_empty());
        assert!(!t.theme.font_scheme.minor.latin.typeface.is_empty());
    }
    assert_eq!(bundled_theme(DEFAULT_THEME).unwrap().name, "Kiln");
    assert!(bundled_theme("nope").is_none());
}

#[test]
fn every_theme_builds_a_complete_master() {
    for t in bundled() {
        for variant in std::iter::once(None).chain((0..t.variants.len()).map(Some)) {
            for size in [SlideSize::WIDESCREEN, SlideSize::STANDARD] {
                let p = Presentation::with_theme(&t, size.clone(), variant);
                assert!(p.slides.is_empty());
                let m = &p.masters[0];
                assert_eq!(m.layouts.len(), 11, "{}", t.id);
                for (l, kind) in m.layouts.iter().zip(LayoutKind::STANDARD) {
                    assert_eq!(l.kind, kind);
                    // Every layout placeholder resolves a position on the slide and inherits
                    // from a master placeholder.
                    for s in l.shapes.iter().filter(|s| s.placeholder.is_some()) {
                        let x = p.resolved_xfrm(s.id).unwrap().rect();
                        let slide = Rect::new(0, 0, size.cx, size.cy);
                        assert_eq!(
                            x.union(&slide),
                            slide,
                            "{} {kind:?} {} leaves the slide",
                            t.id,
                            s.name
                        );
                        let chain = p.chain_of(s.id).unwrap();
                        assert!(
                            chain.master.is_some(),
                            "{} {kind:?} {} has no master",
                            t.id,
                            s.name
                        );
                    }
                }
                // Master placeholders: title, body, date, footer, number.
                let kinds: Vec<PlaceholderType> = m
                    .shapes
                    .iter()
                    .filter_map(|s| s.placeholder.map(|p| p.kind))
                    .collect();
                assert_eq!(
                    kinds,
                    vec![
                        PlaceholderType::Title,
                        PlaceholderType::Body,
                        PlaceholderType::Date,
                        PlaceholderType::Footer,
                        PlaceholderType::SlideNumber
                    ]
                );
                // Text styles are complete: nine body levels, sizes and fonts everywhere.
                for i in 0..LEVELS {
                    let b = m.body_style.level(i);
                    assert!(
                        b.run.size.is_some() && b.run.fonts.is_some(),
                        "{} body level {i}",
                        t.id
                    );
                    assert!(b.deck.bullet.is_some());
                }
                assert!(m.title_style.level(0).run.size.is_some());
                p.validate().unwrap();
            }
        }
    }
}

#[test]
fn every_theme_keeps_text_readable() {
    for t in bundled() {
        for variant in std::iter::once(None).chain((0..t.variants.len()).map(Some)) {
            let mut p = Presentation::with_theme(&t, SlideSize::WIDESCREEN, variant);
            let lay = p.masters[0]
                .layout_of_kind(&LayoutKind::TitleContent)
                .unwrap()
                .id;
            let slide = crate::edit::new_slide(&mut p, lay);
            let sid = slide.id;
            p.slides.push(slide);
            let owner = Owner::Slide(sid);
            let bg = background_color(&p, owner);
            let ctx = p.color_context(owner).unwrap();
            let body = p.slides[0]
                .shapes
                .iter()
                .find(|s| {
                    s.placeholder
                        .is_some_and(|ph| ph.kind == PlaceholderType::Object)
                })
                .unwrap()
                .id;
            let title = p.slides[0].shapes[0].id;
            for (shape, min) in [(title, 4.5), (body, 4.5)] {
                let style = p.level_style(shape, 0).unwrap();
                let color =
                    crate::dml::color::doc_to_drawing_color(style.run.color.as_ref().unwrap());
                let ratio = contrast(resolve_color(&color, &ctx), bg);
                assert!(
                    ratio >= min,
                    "{} variant {variant:?}: contrast {ratio:.2} < {min}",
                    t.id
                );
            }
        }
    }
}

/// The acceptance check: every theme (and every variant) applies to a five-slide sample; the
/// slides keep their content and land on the layout of the same kind; undo restores the deck.
#[test]
fn every_theme_applies_to_a_five_slide_sample() {
    let base = sample();
    assert_eq!(base.slides.len(), 5);
    let kinds: Vec<LayoutKind> = base
        .slides
        .iter()
        .map(|s| base.layout(s.layout).unwrap().1.kind.clone())
        .collect();
    let texts: Vec<Vec<String>> = base
        .slides
        .iter()
        .map(|s| {
            s.shapes
                .iter()
                .filter_map(|x| x.text().map(TextBody::text))
                .collect()
        })
        .collect();
    for t in bundled() {
        for variant in std::iter::once(None).chain((0..t.variants.len()).map(Some)) {
            let mut p = base.clone();
            let tx = crate::edit::apply_theme(&mut p, &t, variant).unwrap();
            roundtrip(&mut p, tx);
            assert_eq!(p.masters.len(), 1);
            assert_eq!(p.masters[0].name, t.name);
            assert_eq!(p.theme().unwrap().name, t.theme.name);
            if let Some(v) = variant {
                assert_eq!(
                    p.theme().unwrap().color_scheme,
                    t.variants[v]
                        .colors
                        .clone()
                        .unwrap_or_else(|| t.theme.color_scheme.clone())
                );
            }
            for (i, s) in p.slides.iter().enumerate() {
                assert_eq!(
                    p.layout(s.layout).unwrap().1.kind,
                    kinds[i],
                    "{}: slide {i}",
                    t.id
                );
                let now: Vec<String> = s
                    .shapes
                    .iter()
                    .filter_map(|x| x.text().map(TextBody::text))
                    .collect();
                assert_eq!(now, texts[i], "{}: slide {i} text", t.id);
                // Placeholders inherit positions and theme text styles from the new master.
                for sh in s.shapes.iter().filter(|x| x.placeholder.is_some()) {
                    assert!(
                        p.resolved_xfrm(sh.id).is_some(),
                        "{} slide {i} {}",
                        t.id,
                        sh.name
                    );
                    let lvl = p.level_style(sh.id, 0).unwrap();
                    assert!(lvl.run.size.is_some());
                }
            }
            // The title resolves to the theme's title size (or the layout's override).
            let title = p.slides[1]
                .shapes
                .iter()
                .find(|x| x.placeholder.is_some_and(|ph| ph.kind.is_title()))
                .unwrap()
                .id;
            let size = p.level_style(title, 0).unwrap().run.size.unwrap();
            assert_eq!(
                Some(size),
                p.masters[0].title_style.level(0).run.size,
                "{}",
                t.id
            );
            p.validate().unwrap();
            let back = Presentation::from_bytes(&p.to_bytes().unwrap()).unwrap();
            assert_eq!(back, p);
        }
    }
}

#[test]
fn variants_apply_to_a_master() {
    let mut p = Presentation::new();
    let t = bundled_theme(DEFAULT_THEME).unwrap();
    let master = p.masters[0].id;
    for v in 0..t.variants.len() {
        let tx = crate::edit::apply_variant(&mut p, master, &t, v).unwrap();
        roundtrip(&mut p, tx);
        assert_eq!(
            p.masters[0].theme.color_scheme,
            *t.variants[v].colors.as_ref().unwrap()
        );
    }
    // The "Night" variant is dark.
    let night = t.variants.iter().position(|v| v.name == "Night").unwrap();
    let tx = crate::edit::apply_variant(&mut p, master, &t, night).unwrap();
    p.apply(tx).unwrap();
    assert_eq!(p.masters[0].color_map, ColorMapping::dark());
    assert!(crate::edit::apply_variant(&mut p, master, &t, 99).is_err());
    assert!(crate::edit::apply_variant(&mut p, NodeId(999_999), &t, 0).is_err());
    // Colours and fonts on their own.
    let other = bundled_theme("raku").unwrap();
    let tx =
        crate::edit::set_theme_colors(&mut p, master, other.theme.color_scheme.clone()).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.masters[0].theme.color_scheme, other.theme.color_scheme);
    let tx = crate::edit::set_theme_fonts(&mut p, master, other.theme.font_scheme.clone()).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.masters[0].theme.font_scheme.major.latin.typeface, "Arial");
}

#[test]
fn a_new_presentation_uses_the_kiln_theme() {
    let p = Presentation::new();
    assert_eq!(p.masters[0].name, "Kiln");
    assert_eq!(p.slides.len(), 1);
    let s = &p.slides[0];
    assert_eq!(p.layout(s.layout).unwrap().1.kind, LayoutKind::Title);
    assert_eq!(s.shapes.len(), 2);
    assert!(p.notes_master.is_some());
    assert_eq!(Presentation::default(), p);
    // The fallback theme builds too.
    let f = ThemeFile::fallback();
    let q = Presentation::with_theme(&f, SlideSize::STANDARD, None);
    q.validate().unwrap();
    assert_eq!(q.masters[0].layouts.len(), 11);
}

const MINIMAL: &str = r#"<?xml version="1.0"?>
<kd:deckTheme xmlns:kd="urn:kiln:deck-theme:1" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" version="1" id="t" name="T">
<a:theme name="T"><a:themeElements/></a:theme>
<kd:master/>
</kd:deckTheme>"#;

#[test]
fn a_minimal_theme_file_gets_defaults() {
    let t = ThemeFile::parse(MINIMAL).unwrap();
    assert_eq!(t.id, "t");
    assert!(t.layouts.is_empty() && t.variants.is_empty());
    assert!(!t.theme.format_scheme.fill_styles.is_empty());
    let p = Presentation::with_theme(&t, SlideSize::WIDESCREEN, None);
    p.validate().unwrap();
    assert_eq!(p.masters[0].body_style.level(0).run.size, Some(56));
}

#[test]
fn theme_files_report_their_problems() {
    let bad = [
        ("not xml", "<"),
        (
            "wrong root",
            r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"/>"#,
        ),
        ("no id", &MINIMAL.replace(r#"id="t""#, "")),
        (
            "future version",
            &MINIMAL.replace(r#"version="1""#, r#"version="2""#),
        ),
        (
            "no theme",
            &MINIMAL.replace(r#"<a:theme name="T"><a:themeElements/></a:theme>"#, ""),
        ),
        ("no master", &MINIMAL.replace("<kd:master/>", "")),
        (
            "bad grid",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:grid marginX="lots"/></kd:master>"#,
            ),
        ),
        (
            "grid out of range",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:grid gap="7"/></kd:master>"#,
            ),
        ),
        (
            "half a rect",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:ph type="title" x="0.1"/></kd:master>"#,
            ),
        ),
        (
            "unknown ph",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:ph type="banner"/></kd:master>"#,
            ),
        ),
        (
            "unknown preset",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:shape prst="blob" x="0" y="0" w="1" h="1"/></kd:master>"#,
            ),
        ),
        (
            "shape without rect",
            &MINIMAL.replace(
                "<kd:master/>",
                r#"<kd:master><kd:shape prst="rect"/></kd:master>"#,
            ),
        ),
        (
            "background without fill",
            &MINIMAL.replace("<kd:master/>", "<kd:master><kd:background/></kd:master>"),
        ),
        (
            "custom layout",
            &MINIMAL.replace("<kd:master/>", r#"<kd:master/><kd:layout type="cust"/>"#),
        ),
    ];
    for (what, xml) in bad {
        let e = ThemeFile::parse(xml);
        assert!(matches!(e, Err(DeckError::Theme(_))), "{what}: {e:?}");
    }
    // Background references and custom geometry are accepted.
    let ok = MINIMAL.replace(
        "<kd:master/>",
        r#"<kd:master><kd:background><kd:bgRef idx="1002"><a:schemeClr val="bg2"/></kd:bgRef></kd:background><kd:shape name="c" x="0" y="0" w="1" h="1"><a:custGeom><a:pathLst><a:path w="1" h="1"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="1" y="1"/></a:lnTo></a:path></a:pathLst></a:custGeom><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></kd:shape></kd:master>"#,
    );
    let t = ThemeFile::parse(&ok).unwrap();
    assert!(matches!(
        t.master.background,
        Background::Theme { idx: 1002, .. }
    ));
    assert!(matches!(t.master.shapes[0].geometry, Geometry::Custom(_)));
}

/// The decorations drawn behind a slide's text: master shapes (when the slide shows them),
/// then layout shapes, bottom to top. Placeholders are text, not decoration.
fn decorations(p: &Presentation, slide: NodeId) -> Vec<&Shape> {
    let s = p.slide(slide).unwrap();
    let (master, layout) = p.layout(s.layout).unwrap();
    let mut out: Vec<&Shape> = Vec::new();
    if p.shows_master_shapes(slide) {
        out.extend(master.shapes.iter().filter(|x| x.placeholder.is_none()));
    }
    out.extend(layout.shapes.iter().filter(|x| x.placeholder.is_none()));
    out
}

/// Where a placeholder's text can go: its box inside the body insets.
fn text_area(p: &Presentation, shape: NodeId) -> Rect {
    let r = p.resolved_xfrm(shape).unwrap().rect();
    let [l, t, rt, b] = p.resolved_body(shape).unwrap().insets;
    Rect::new(r.x + l, r.y + t, r.cx - l - rt, r.cy - t - b)
}

/// One slide on every layout of a theme (variant).
fn every_layout(t: &ThemeFile, variant: Option<usize>, size: SlideSize) -> Presentation {
    let mut p = Presentation::with_theme(t, size, variant);
    let layouts: Vec<NodeId> = p.masters[0].layouts.iter().map(|l| l.id).collect();
    for l in layouts {
        let slide = crate::edit::new_slide(&mut p, l);
        p.slides.push(slide);
    }
    p
}

/// Hairlines, underlines and markers are drawn for one layout's title band; on any layout
/// they show on, they must stay clear of every placeholder's text area (the Celadon section
/// header once drew its rule through the subtitle).
#[test]
fn decorative_rules_never_cross_placeholder_text() {
    for t in bundled() {
        for size in [SlideSize::WIDESCREEN, SlideSize::STANDARD] {
            let thin = size.cy / 50;
            let p = every_layout(&t, None, size);
            for s in &p.slides {
                let kind = &p.layout(s.layout).unwrap().1.kind;
                for d in decorations(&p, s.id) {
                    let Some(b) = d.xfrm.map(|x| x.bounds()) else {
                        continue;
                    };
                    if b.cx.min(b.cy) > thin {
                        continue;
                    }
                    for ph in s.shapes.iter().filter(|x| x.placeholder.is_some()) {
                        assert!(
                            !b.intersects(&text_area(&p, ph.id)),
                            "{} {kind:?}: {} crosses {}",
                            t.id,
                            d.name,
                            ph.name
                        );
                    }
                }
            }
        }
    }
}

/// The colour behind a point of a slide: the topmost filled decoration under it, else the
/// background (translucent fills blended over what is below).
fn color_behind(p: &Presentation, slide: NodeId, at: (i64, i64)) -> Rgba {
    let owner = Owner::Slide(slide);
    let ctx = p.color_context(owner).unwrap();
    let mut c = background_color(p, owner);
    for d in decorations(p, slide) {
        let Some(b) = d.xfrm.map(|x| x.bounds()) else {
            continue;
        };
        if !(b.x <= at.0 && at.0 < b.right() && b.y <= at.1 && at.1 < b.bottom()) {
            continue;
        }
        let fill = match d.props().and_then(|x| x.fill.as_ref()) {
            Some(Fill::Solid(f)) => resolve_color(f, &ctx),
            Some(Fill::Gradient(g)) if !g.stops.is_empty() => {
                // Gradient decorations are judged by their strongest stop.
                g.stops.iter().map(|s| resolve_color(&s.color, &ctx)).fold(
                    Rgba {
                        a: 0.0,
                        ..Rgba::rgb(0.0, 0.0, 0.0)
                    },
                    |a, x| if x.a > a.a { x } else { a },
                )
            }
            _ => continue,
        };
        let a = fill.a;
        c = Rgba::rgb(
            fill.r * a + c.r * (1.0 - a),
            fill.g * a + c.g * (1.0 - a),
            fill.b * a + c.b * (1.0 - a),
        );
    }
    c
}

/// Every placeholder on every layout, in every variant, reads against what is behind it:
/// 4.5:1 for text under 24 pt, 3:1 for larger (WCAG AA). This covers the per-layout
/// colours (subtitles on accent-filled section headers, titles on panels) that the master
/// check does not.
#[test]
fn every_layout_keeps_its_text_readable() {
    let mut failures = Vec::new();
    for t in bundled() {
        for variant in std::iter::once(None).chain((0..t.variants.len()).map(Some)) {
            let p = every_layout(&t, variant, SlideSize::WIDESCREEN);
            for s in &p.slides {
                let kind = &p.layout(s.layout).unwrap().1.kind;
                let ctx = p.color_context(Owner::Slide(s.id)).unwrap();
                for ph in s.shapes.iter().filter(|x| x.placeholder.is_some()) {
                    let style = p.level_style(ph.id, 0).unwrap();
                    let Some(color) = style.run.color.as_ref() else {
                        continue;
                    };
                    let fg = resolve_color(&crate::dml::color::doc_to_drawing_color(color), &ctx);
                    let area = text_area(&p, ph.id);
                    let bg = color_behind(&p, s.id, area.center());
                    let ratio = contrast(fg, bg);
                    let half_points = style.run.size.unwrap_or(0);
                    let min = if half_points >= 48 { 3.0 } else { 4.5 };
                    if ratio < min {
                        failures.push(format!(
                            "{} {variant:?} {kind:?} {}: {ratio:.2} < {min}",
                            t.id, ph.name
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
