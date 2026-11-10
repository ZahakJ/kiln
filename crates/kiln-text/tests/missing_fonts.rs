// SPDX-License-Identifier: AGPL-3.0-or-later
//! Characters no font covers are reported by kind (the web core fetches its fallbacks then).

use kiln_text::{
    FontStore, FontStoreOptions, MISSING_ARABIC, MISSING_MATH, ParagraphSpec, SpanStyle,
    TextEngine, take_missing,
};

#[test]
fn characters_without_a_font_are_reported_by_kind() {
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
        directories: Vec::new(),
    });
    fonts.add_font_data(
        include_bytes!("../../../assets/fonts/carlito/Carlito-Regular.ttf").to_vec(),
    );
    fonts.refresh_fallbacks();
    let mut engine = TextEngine::new(fonts);
    let _ = take_missing();
    engine.layout(&ParagraphSpec::plain(
        "Plain Latin text",
        SpanStyle::new("Calibri", 11.0),
        400.0,
    ));
    assert_eq!(take_missing() & (MISSING_ARABIC | MISSING_MATH), 0);
    engine.layout(&ParagraphSpec::plain(
        "مرحبا ⨁",
        SpanStyle::new("Calibri", 11.0),
        400.0,
    ));
    let kinds = take_missing();
    assert_ne!(kinds & MISSING_ARABIC, 0, "{kinds}");
    assert_ne!(kinds & MISSING_MATH, 0, "{kinds}");
    // Natively the bundled Noto fonts cover both.
    let mut bundled = TextEngine::new(FontStore::bundled());
    bundled.layout(&ParagraphSpec::plain(
        "مرحبا ⨁",
        SpanStyle::new("Calibri", 11.0),
        400.0,
    ));
    assert_eq!(take_missing() & (MISSING_ARABIC | MISSING_MATH), 0);
}

/// The web core's fallback fonts: Carlito, every slice of Noto Sans CJK SC
/// (`assets/fonts/noto/cjk/`) and Noto Emoji.
fn web_fonts() -> FontStore {
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
        directories: Vec::new(),
    });
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
    fonts.add_font_data(std::fs::read(assets.join("carlito/Carlito-Regular.ttf")).unwrap());
    let mut slices: Vec<_> = std::fs::read_dir(assets.join("noto/cjk"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "otf"))
        .collect();
    slices.sort();
    assert!(slices.len() > 50, "the slices of the CJK font");
    for s in slices {
        fonts.add_font_data(std::fs::read(s).unwrap());
    }
    fonts.add_font_data(std::fs::read(assets.join("noto/NotoEmoji-Regular.ttf")).unwrap());
    fonts.refresh_fallbacks();
    fonts
}

/// Lays `text` out in one long line and gives the glyphs and what was missed.
fn glyphs(engine: &mut TextEngine, text: &str, lang: Option<&str>) -> (Vec<u32>, u32) {
    let _ = take_missing();
    let mut style = SpanStyle::new("Calibri", 11.0);
    style.lang = lang.map(str::to_owned);
    let layout = engine.layout(&ParagraphSpec::plain(text, style, 1.0e9));
    let ids = layout
        .lines
        .iter()
        .flat_map(|l| l.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.id)))
        .collect();
    (ids, take_missing())
}

/// Chinese, Japanese, Korean and emoji are their own kinds, and the web core's fonts for them
/// cover them.
#[test]
fn cjk_and_emoji_are_reported_and_the_web_fallbacks_cover_them() {
    use kiln_text::{MISSING_CJK, MISSING_EMOJI};
    let text = "日本語 中文 한국어 かなカナ 😀🎉";
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
        directories: Vec::new(),
    });
    fonts.add_font_data(
        include_bytes!("../../../assets/fonts/carlito/Carlito-Regular.ttf").to_vec(),
    );
    fonts.refresh_fallbacks();
    let mut engine = TextEngine::new(fonts);
    let (_, kinds) = glyphs(&mut engine, text, None);
    assert_ne!(kinds & MISSING_CJK, 0, "{kinds}");
    assert_ne!(kinds & MISSING_EMOJI, 0, "{kinds}");
    let missed = kiln_text::take_missing_chars();
    assert!(
        missed.contains(&u32::from('語')) && missed.contains(&u32::from('한')),
        "{missed:?}"
    );
    let mut engine = TextEngine::new(web_fonts());
    let (ids, kinds) = glyphs(&mut engine, text, None);
    assert_eq!(kinds, 0, "{kinds}");
    assert!(!ids.contains(&0));
}

/// Every Hangul syllable draws as one glyph: none comes apart into loose jamo, none is a box.
#[test]
fn all_11172_hangul_syllables_shape_whole() {
    let text: String = ('\u{AC00}'..='\u{D7A3}').collect();
    let mut engine = TextEngine::new(web_fonts());
    let (ids, kinds) = glyphs(&mut engine, &text, Some("ko"));
    let boxes = ids.iter().filter(|&&g| g == 0).count();
    println!(
        "HANGUL: 11172 syllables, {} glyphs, {boxes} boxes, missing kinds {kinds}",
        ids.len()
    );
    assert_eq!(ids.len(), 11_172, "a syllable came apart into jamo");
    assert_eq!(boxes, 0);
    assert_eq!(kinds, 0);
}

/// The characters review 5 found drawn as boxes: common Chinese and name characters, a jōyō
/// kanji outside JIS X 0208, Cantonese, and the Hangul that came apart.
#[test]
fn the_characters_found_missing_draw() {
    let samples = [
        ("zh-Hans", "啰镕堃"),
        ("ja", "𠮟"),
        // Cantonese: 佢哋喺邊度？我哋冇嘢做，嘅確係噉。咁啱嘞，乜嘢都唔使驚。
        (
            "zh-HK",
            "佢哋喺邊度？我哋冇嘢做，嘅確係噉。咁啱嘞，乜嘢都唔使驚。嚟啦冚唪唥攰",
        ),
        ("ko", "똠뷁쌰햏"),
    ];
    let mut engine = TextEngine::new(web_fonts());
    for (lang, text) in samples {
        let (ids, kinds) = glyphs(&mut engine, text, Some(lang));
        println!(
            "SAMPLE {lang} {text}: {} glyphs, {} boxes",
            ids.len(),
            ids.iter().filter(|&&g| g == 0).count()
        );
        assert_eq!(ids.len(), text.chars().count(), "{text}");
        assert!(!ids.contains(&0), "{text}");
        assert_eq!(kinds, 0, "{text}");
    }
}

/// Emoji reach the emoji font by their properties and sequences: flags, ✅, ⭐ with and without
/// VS16, keycaps, ZWJ and tag sequences each draw as one emoji glyph, not boxes or a text star.
#[test]
fn emoji_flags_and_sequences_draw_from_the_emoji_font() {
    let mut engine = TextEngine::new(web_fonts());
    for e in [
        "🇯🇵",
        "🇬🇧",
        "🇺🇦",
        "✅",
        "⭐",
        "⭐\u{FE0F}",
        "1\u{FE0F}\u{20E3}",
        "#\u{FE0F}\u{20E3}",
        "❤\u{FE0F}",
        "👨\u{200D}👩\u{200D}👧",
        "🏳\u{FE0F}\u{200D}🌈",
        "👍🏽",
        "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
    ] {
        let (ids, kinds) = glyphs(&mut engine, e, None);
        let mut style = SpanStyle::new("Calibri", 11.0);
        style.lang = None;
        let layout = engine.layout(&ParagraphSpec::plain(e, style, 1.0e9));
        let families: Vec<String> = layout
            .lines
            .iter()
            .flat_map(|l| l.runs.iter())
            .map(|r| {
                engine
                    .fonts()
                    .font(r.font)
                    .map(|f| f.family.clone())
                    .unwrap_or_default()
            })
            .collect();
        println!("EMOJI {e:?}: glyphs {ids:?} from {families:?}, missing kinds {kinds}");
        assert!(!ids.contains(&0), "{e:?}");
        assert_eq!(kinds, 0, "{e:?}");
        assert!(
            families.iter().all(|f| f == "Noto Emoji"),
            "{e:?}: {families:?}"
        );
    }
}

/// The slices keep `locl`: one character (丑) takes its Japanese, Korean or Traditional Chinese
/// form by the text's language, or by the East Asian Office font the text asks for (MS Gothic is
/// Japanese, Malgun Gothic Korean, PMingLiU Traditional Chinese).
#[test]
fn a_character_takes_its_regional_form_by_language() {
    let mut engine = TextEngine::new(web_fonts());
    let mut form = |family: &str, lang: Option<&str>| {
        let mut style = SpanStyle::new(family, 11.0);
        style.lang = lang.map(str::to_owned);
        let layout = engine.layout(&ParagraphSpec::plain("丑", style, 1.0e9));
        layout.lines[0].runs[0].glyphs[0].id
    };
    let sc = form("Calibri", Some("zh-Hans"));
    let ja = form("Calibri", Some("ja"));
    let ko = form("Calibri", Some("ko"));
    let tc = form("Calibri", Some("zh-TW"));
    let (gothic, malgun, ming) = (
        form("MS Gothic", None),
        form("Malgun Gothic", None),
        form("PMingLiU", None),
    );
    println!(
        "LOCL 丑: zh-Hans {sc}, ja {ja}, ko {ko}, zh-TW {tc}; MS Gothic {gothic}, Malgun Gothic \
         {malgun}, PMingLiU {ming}"
    );
    for (form, what) in [(ja, "ja"), (ko, "ko"), (tc, "zh-TW")] {
        assert_ne!(form, sc, "{what}");
    }
    assert_eq!((gothic, malgun, ming), (ja, ko, tc));
}
