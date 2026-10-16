// SPDX-License-Identifier: AGPL-3.0-or-later
//! Glyph atlas, outlines, font store, substitution and metrics.

mod common;

use kiln_text::{
    ATLAS_PAGE_SIZE, AtlasFormat, FontStore, FontStoreOptions, GlyphAtlas, OUTLINE_THRESHOLD,
    ParagraphSpec, RunGlyphs, SpanStyle, SubstitutionKind, TextEngine, glyph_outline,
};

#[test]
fn atlas_rasterises_each_glyph_once_and_reports_dirty_pages() {
    let mut engine = common::engine();
    let layout = engine.layout(&ParagraphSpec::plain(
        "Kiln renders glyphs once; spaces have no bitmap.",
        SpanStyle::new("Calibri", 16.0),
        600.0,
    ));
    let fonts = engine.fonts();
    let mut atlas = GlyphAtlas::new();
    let run = &layout.lines[0].runs[0];
    let RunGlyphs::Bitmaps(first) = atlas
        .prepare_run(fonts, run, 10.0, 20.0, 1.0)
        .expect("atlas")
    else {
        panic!("small text must use bitmaps");
    };
    let spaces = layout.text().matches(' ').count();
    assert_eq!(
        first.len(),
        run.glyphs.len() - spaces,
        "one bitmap per visible glyph"
    );
    let dirty = atlas.take_dirty();
    assert_eq!(dirty.len(), 1);
    let cached = atlas.len();
    // Same positions again: no new rasterisation, no dirty page.
    let RunGlyphs::Bitmaps(second) = atlas
        .prepare_run(fonts, run, 10.0, 20.0, 1.0)
        .expect("atlas")
    else {
        panic!()
    };
    assert_eq!(first, second);
    assert_eq!(atlas.len(), cached);
    assert!(atlas.take_dirty().is_empty());

    let page = atlas.page(dirty[0]).expect("page");
    assert_eq!(page.format, AtlasFormat::A8);
    assert_eq!(
        (page.width, page.height),
        (ATLAS_PAGE_SIZE, ATLAS_PAGE_SIZE)
    );
    // Every placed bitmap has ink and lies inside the page.
    for g in &first {
        let a = g.glyph;
        assert!(u32::from(a.x) + u32::from(a.width) <= page.width);
        assert!(u32::from(a.y) + u32::from(a.height) <= page.height);
        let mut ink = 0_u32;
        for yy in 0..usize::from(a.height) {
            let row = (usize::from(a.y) + yy) * page.width as usize + usize::from(a.x);
            ink += page.data[row..row + usize::from(a.width)]
                .iter()
                .map(|&v| u32::from(v))
                .sum::<u32>();
        }
        assert!(ink > 0, "glyph bitmap is blank");
        // Bitmaps hang from the baseline by their top bearing.
        let baseline = (20.0 + layout.lines[0].baseline).round() as i32;
        assert_eq!(g.y + i32::from(a.top), baseline, "{g:?}");
        assert!(a.top > 0 && i32::from(a.top) <= 16, "{g:?}");
    }
}

#[test]
fn subpixel_positions_produce_distinct_bitmaps() {
    let mut engine = common::engine();
    let layout = engine.layout(&ParagraphSpec::plain(
        "l",
        SpanStyle::new("Calibri", 15.0),
        100.0,
    ));
    let run = &layout.lines[0].runs[0];
    let mut atlas = GlyphAtlas::new();
    for quarter in 0..4 {
        let x = 10.0 + quarter as f32 * 0.25;
        atlas
            .prepare_run(engine.fonts(), run, x, 10.0, 1.0)
            .expect("atlas");
    }
    assert_eq!(atlas.len(), 4, "one entry per quarter-pixel offset");
}

#[test]
fn large_glyphs_become_outlines() {
    let mut engine = common::engine();
    let layout = engine.layout(&ParagraphSpec::plain(
        "Ag",
        SpanStyle::new("Cambria", 72.0),
        400.0,
    ));
    let run = &layout.lines[0].runs[0];
    let mut atlas = GlyphAtlas::new();
    // 72 px at 2× zoom = 144 device px ≥ the threshold.
    const { assert!(72.0 * 2.0 >= OUTLINE_THRESHOLD) };
    match atlas
        .prepare_run(engine.fonts(), run, 0.0, 100.0, 2.0)
        .expect("atlas")
    {
        RunGlyphs::Outlines(paths) => {
            assert_eq!(paths.len(), 2);
            let bb = kurbo::Shape::bounding_box(&paths[0]);
            // "A" at 144 px: cap height ≈ 0.67 em, sits on the baseline at y = 100 + baseline*2.
            assert!(bb.height() > 80.0 && bb.height() < 120.0, "{bb:?}");
        }
        RunGlyphs::Bitmaps(_) => panic!("expected outlines"),
    }
    assert!(atlas.is_empty());
    // At 1× the same run is small enough for bitmaps.
    assert!(matches!(
        atlas
            .prepare_run(engine.fonts(), run, 0.0, 100.0, 1.0)
            .expect("atlas"),
        RunGlyphs::Bitmaps(_)
    ));
    let path = glyph_outline(engine.fonts(), run.font, run.glyphs[0].id, 100.0).expect("outline");
    assert!(!path.elements().is_empty());
}

#[test]
fn colour_glyphs_go_to_rgba_pages_when_an_emoji_font_exists() {
    let mut fonts = FontStore::new(&FontStoreOptions::default());
    if !fonts.has_family("Noto Color Emoji") {
        eprintln!("skipped: no colour emoji font on this machine");
        return;
    }
    fonts.set_script_fallbacks(*b"Zsye", &["Noto Color Emoji"]);
    let mut engine = TextEngine::new(fonts);
    let layout = engine.layout(&ParagraphSpec::plain(
        "😀",
        SpanStyle::new("Calibri", 20.0),
        100.0,
    ));
    let run = &layout.lines[0].runs[0];
    let mut atlas = GlyphAtlas::new();
    let RunGlyphs::Bitmaps(g) = atlas
        .prepare_run(engine.fonts(), run, 0.0, 20.0, 1.0)
        .expect("atlas")
    else {
        panic!()
    };
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].glyph.format, AtlasFormat::Rgba8);
}

#[test]
fn bundled_store_has_the_document_fonts() {
    let mut fonts = FontStore::bundled();
    for family in [
        "Liberation Sans",
        "Liberation Serif",
        "Liberation Mono",
        "Carlito",
        "Caladea",
    ] {
        assert!(fonts.has_family(family), "{family}");
    }
    assert!(!fonts.uses_system_fonts());
    assert_eq!(fonts.family_names().len(), 5);
}

#[test]
fn office_names_resolve_through_the_substitution_table() {
    let mut fonts = FontStore::bundled();
    let r = fonts.resolve("Calibri");
    assert_eq!(
        (r.family.as_str(), r.kind),
        ("Carlito", SubstitutionKind::MetricCompatible)
    );
    assert_eq!(fonts.resolve("Cambria").family, "Caladea");
    assert_eq!(fonts.resolve("Times New Roman").family, "Liberation Serif");
    assert_eq!(fonts.resolve("Courier New").family, "Liberation Mono");
    assert_eq!(fonts.resolve("Helvetica").family, "Liberation Sans");
    assert_eq!(
        fonts.resolve("Calibri Light").kind,
        SubstitutionKind::Approximate
    );
    assert_eq!(fonts.resolve("Carlito").kind, SubstitutionKind::Exact);
    let aptos = fonts.resolve("Aptos");
    assert_eq!(
        (aptos.family.as_str(), aptos.kind),
        ("Liberation Sans", SubstitutionKind::UiSans)
    );
    fonts.set_ui_sans("Carlito");
    assert_eq!(fonts.resolve("Aptos").family, "Carlito");
    assert_eq!(
        fonts.resolve("Symbol").kind,
        SubstitutionKind::SymbolFallback
    );
    assert_eq!(
        fonts.resolve("Garamondish Serif").family,
        "Liberation Serif"
    );
    assert_eq!(
        fonts.resolve("Garamondish Serif").kind,
        SubstitutionKind::Missing
    );
}

#[test]
fn line_heights_follow_words_metric_rule() {
    let mut fonts = FontStore::bundled();
    let carlito = fonts.primary_font("Carlito", 400, false).expect("Carlito");
    let m = fonts.metrics(carlito, 2048.0);
    // Carlito: usWinAscent 1950 + usWinDescent 550 (no USE_TYPO_METRICS) → Calibri's 1.2207 em.
    assert!(
        (m.ascent - 1950.0).abs() < 0.5 && (m.descent - 550.0).abs() < 0.5,
        "{m:?}"
    );
    assert!((m.line_height() / 2048.0 - 1.2207).abs() < 1e-3);
    let sans = fonts
        .primary_font("Liberation Sans", 400, false)
        .expect("Liberation Sans");
    let u = fonts.unit_metrics(sans).expect("metrics");
    assert!(!u.use_typo_metrics);
    assert!(
        (u.ascent - 1854.0).abs() < 0.5 && (u.descent - 434.0).abs() < 0.5,
        "{u:?}"
    );
    // The (font, size) cache serves repeated lookups.
    let before = fonts.metrics_cache_len();
    let _ = fonts.metrics(sans, 16.0);
    let _ = fonts.metrics(sans, 16.0);
    assert_eq!(fonts.metrics_cache_len(), before + 1);
}

#[test]
fn system_fonts_are_discovered_on_native_targets() {
    let mut fonts = FontStore::new(&FontStoreOptions::default());
    assert!(fonts.uses_system_fonts());
    // Bundled fonts are always there; system families come on top where the platform has any.
    let names = fonts.family_names();
    assert!(names.len() >= 5);
    eprintln!("{} families with system fonts", names.len());
    assert!(fonts.has_family("Carlito"));
}

#[test]
fn shaping_cache_reuses_words_and_keeps_results_identical() {
    let mut engine = common::engine();
    let spec = ParagraphSpec::plain(
        "the cache shapes the same words once and the layout stays the same",
        SpanStyle::new("Calibri", 15.0),
        200.0,
    );
    let first = engine.layout(&spec);
    let (entries, _, misses) = engine.shape_cache_stats();
    assert!(entries > 0 && misses > 0);
    let second = engine.layout(&spec);
    let (_, hits, misses_after) = engine.shape_cache_stats();
    assert_eq!(first, second);
    assert_eq!(misses, misses_after, "nothing reshaped");
    assert!(hits > 0);
    // A fresh engine (no cache) produces the same layout.
    let fresh = common::engine().layout(&spec);
    assert_eq!(first, fresh);
}
