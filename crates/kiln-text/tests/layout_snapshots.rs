// SPDX-License-Identifier: AGPL-3.0-or-later
//! Snapshot tests of line breaking and glyph counts. Snapshots live in `fixtures/golden/text/`;
//! review changes with `cargo insta review` (or `INSTA_UPDATE=always` and `git diff`).

mod common;

use common::{describe, describe_detailed, engine, para, runs, style};
use kiln_text::{
    Alignment, Direction, LineSpacing, ParagraphLayout, ParagraphSpec, StrikeKind, TabAlignment,
    TabLeader, TabStop, TextRun, UnderlineKind,
};

const EN: &str = "Kiln is an office suite that opens, edits and saves real Microsoft Office files. \
It feels immediately familiar to Office users while looking and feeling like its own product: \
warmer, calmer and faster, with features Office lacks.";

const EN_LONG: &str = "Pagination and line breaking are deterministic Rust code using bundled font \
metrics, so a three-hundred-page document paginates identically on every platform and identically \
in the exported PDF. Rust owns the hot loop (layout, invalidation, culling, glyph caching) and runs \
multi-threaded natively; the painter only blits tiles and glyph quads.";

const DE: &str = "Die Silbentrennung verbessert das Schriftbild von Blocksatz erheblich, besonders \
bei schmalen Spalten mit langen zusammengesetzten Wörtern wie Donaudampfschifffahrtsgesellschaft \
oder Rechtsschutzversicherungsgesellschaften.";

const AR: &str = "تُعدّ اللغة العربية من أكثر اللغات انتشاراً في العالم، ويتحدث بها أكثر من \
أربعمائة مليون شخص في الوطن العربي وخارجه.";

const HE: &str = "שלום עולם, זהו טקסט בעברית לבדיקת שבירת השורות בכיוון מימין לשמאל.";

const ZH: &str = "中文排版需要在汉字之间允许换行，标点符号遵循禁则规则。这是一段用于测试的简体中文文本，\
包含逗号、句号以及引号「这样」的内容。";

const JA: &str = "日本語の文章では、句読点の前で改行してはいけません。ひらがなとカタカナ、漢字が混在する\
テキストを正しく折り返す必要があります。";

fn check(name: &str, spec: &ParagraphSpec) -> ParagraphLayout {
    check_with(name, spec, describe)
}

fn check_with(
    name: &str,
    spec: &ParagraphSpec,
    f: fn(&ParagraphLayout) -> String,
) -> ParagraphLayout {
    let mut e = engine();
    let layout = e.layout(spec);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../fixtures/golden/text");
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    let text = f(&layout);
    settings.bind(|| insta::assert_snapshot!(name, text));
    layout
}

fn with(mut spec: ParagraphSpec, f: impl FnOnce(&mut ParagraphSpec)) -> ParagraphSpec {
    f(&mut spec);
    spec
}

// ---------------------------------------------------------------- English alignment

#[test]
fn en_left() {
    check("en_left", &para(EN, "Calibri", 11.0, 400.0));
}

#[test]
fn en_left_times_long() {
    check(
        "en_left_times_long",
        &para(EN_LONG, "Times New Roman", 12.0, 600.0),
    );
}

#[test]
fn en_center() {
    let l = check(
        "en_center",
        &with(para(EN, "Arial", 10.0, 360.0), |s| {
            s.props.alignment = Alignment::Center;
        }),
    );
    let first = &l.lines[0];
    assert!(
        (first.x * 2.0 + first.width - 360.0).abs() < 0.01,
        "centred"
    );
}

#[test]
fn en_right() {
    let l = check(
        "en_right",
        &with(para(EN, "Arial", 10.0, 360.0), |s| {
            s.props.alignment = Alignment::Right;
        }),
    );
    for line in &l.lines {
        assert!((line.x + line.width - 360.0).abs() < 0.01);
    }
}

#[test]
fn en_justify() {
    let l = check(
        "en_justify",
        &with(para(EN_LONG, "Cambria", 11.0, 420.0), |s| {
            s.props.alignment = Alignment::Justify;
        }),
    );
    let (last, rest) = l.lines.split_last().expect("lines");
    for line in rest {
        assert!((line.x + line.width - 420.0).abs() < 0.01, "full measure");
    }
    assert!(last.width < 420.0 - 1.0, "last line not stretched");
}

#[test]
fn en_distribute() {
    let l = check(
        "en_distribute",
        &with(
            para(
                "Distributed alignment spreads every line.",
                "Calibri",
                11.0,
                300.0,
            ),
            |s| {
                s.props.alignment = Alignment::Distribute;
            },
        ),
    );
    let last = l.lines.last().expect("line");
    assert!(
        (last.x + last.width - 300.0).abs() < 0.01,
        "last line stretched too"
    );
}

// ---------------------------------------------------------------- Indents

#[test]
fn en_first_line_indent() {
    let l = check(
        "en_first_line_indent",
        &with(para(EN, "Calibri", 11.0, 400.0), |s| {
            s.props.first_line = 48.0;
        }),
    );
    assert!((l.lines[0].x - 48.0).abs() < 0.01 && l.lines[1].x.abs() < 0.01);
}

#[test]
fn en_hanging_indent_numbered() {
    let l = check(
        "en_hanging_indent_numbered",
        &with(para(&format!("1.\t{EN}"), "Calibri", 11.0, 400.0), |s| {
            s.props.indent_start = 24.0;
            s.props.first_line = -24.0;
        }),
    );
    // The tab jumps to the implicit stop at the hanging indent; later lines start there.
    let text_x = l.lines[0].clusters[3].x;
    assert!((text_x - 24.0).abs() < 0.01, "{text_x}");
    assert!((l.lines[1].x - 24.0).abs() < 0.01);
}

#[test]
fn en_indents_both_sides() {
    check(
        "en_indents_both_sides",
        &with(para(EN, "Liberation Serif", 12.0, 500.0), |s| {
            s.props.indent_start = 60.0;
            s.props.indent_end = 60.0;
        }),
    );
}

// ---------------------------------------------------------------- Tabs

#[test]
fn tabs_default_stops() {
    let l = check(
        "tabs_default_stops",
        &para("Name\tRole\tTeam\tLocation", "Calibri", 11.0, 400.0),
    );
    let xs: Vec<f32> = l.lines[0]
        .clusters
        .iter()
        .filter(|c| {
            l.text()[c.text_range.clone()] == *"R" || &l.text()[c.text_range.clone()] == "T"
        })
        .map(|c| c.x)
        .collect();
    assert_eq!(xs, vec![48.0, 96.0]);
}

#[test]
fn tabs_dot_leader_right_aligned() {
    let l = check_with(
        "tabs_dot_leader_right_aligned",
        &with(
            para("Chapter 1: Getting started\t12", "Calibri", 11.0, 400.0),
            |s| {
                s.props.tab_stops = vec![TabStop::new(400.0, TabAlignment::End, TabLeader::Dot)];
            },
        ),
        describe_detailed,
    );
    let line = &l.lines[0];
    assert!(
        (line.x + line.width - 400.0).abs() < 0.01,
        "page number ends at the stop"
    );
    assert!(line.glyph_count() > 40, "leader dots emitted");
}

#[test]
fn tabs_center_and_decimal() {
    let l = check(
        "tabs_center_and_decimal",
        &with(
            para("Item\tCentred\t1234.56", "Liberation Sans", 10.0, 500.0),
            |s| {
                s.props.tab_stops = vec![
                    TabStop::new(300.0, TabAlignment::Decimal, TabLeader::None),
                    TabStop::new(150.0, TabAlignment::Center, TabLeader::Hyphen),
                ];
            },
        ),
    );
    let text = l.text();
    let dot = l.lines[0]
        .clusters
        .iter()
        .find(|c| &text[c.text_range.clone()] == ".")
        .expect("decimal point");
    assert!(
        (dot.x - 300.0).abs() < 0.01,
        "decimal at the stop: {}",
        dot.x
    );
}

#[test]
fn tabs_underscore_and_heavy_leaders() {
    check_with(
        "tabs_underscore_and_heavy_leaders",
        &with(para("Signature\t\tDate\t", "Calibri", 11.0, 480.0), |s| {
            s.props.tab_stops = vec![
                TabStop::new(240.0, TabAlignment::Start, TabLeader::Underscore),
                TabStop::new(300.0, TabAlignment::Start, TabLeader::None),
                TabStop::new(470.0, TabAlignment::Start, TabLeader::Heavy),
            ];
        }),
        describe_detailed,
    );
}

// ---------------------------------------------------------------- Breaks and hyphenation

#[test]
fn manual_line_breaks() {
    let l = check(
        "manual_line_breaks",
        &para("First line\nSecond line\nThird", "Calibri", 11.0, 400.0),
    );
    assert_eq!(l.lines.len(), 3);
}

#[test]
fn manual_break_at_end_adds_empty_line() {
    let l = check(
        "manual_break_at_end_adds_empty_line",
        &para("Ends with a break\n", "Calibri", 11.0, 400.0),
    );
    assert_eq!(l.lines.len(), 2);
}

#[test]
fn soft_hyphens() {
    check(
        "soft_hyphens",
        &para(
            "Extra\u{ad}ordinary in\u{ad}com\u{ad}pre\u{ad}hen\u{ad}si\u{ad}bil\u{ad}i\u{ad}ties abound.",
            "Calibri",
            11.0,
            150.0,
        ),
    );
}

#[test]
fn emergency_break_long_url() {
    // UAX #14 allows breaks after the slashes of a URL, as Word does.
    let l = check(
        "emergency_break_long_url",
        &para(
            "See https://github.com/ZahakJ/kiln/blob/main/crates/kiln-text/src/layout.rs for details.",
            "Courier New",
            10.0,
            200.0,
        ),
    );
    assert!(
        l.lines
            .iter()
            .all(|x| x.end != kiln_text::LineEnd::Emergency)
    );
}

#[test]
fn emergency_break_inside_a_word() {
    let l = check(
        "emergency_break_inside_a_word",
        &para(
            "Sequence ACGTTGCAACGTTGCAACGTTGCAACGTTGCAACGTTGCAACGTTGCA ends here.",
            "Calibri",
            11.0,
            150.0,
        ),
    );
    assert!(
        l.lines
            .iter()
            .any(|x| x.end == kiln_text::LineEnd::Emergency)
    );
    for line in &l.lines {
        assert!(line.width <= 150.0 + 0.01, "{}", line.width);
    }
}

#[test]
fn english_hyphenation() {
    let l = check(
        "english_hyphenation",
        &with(para(EN_LONG, "Calibri", 11.0, 220.0), |s| {
            s.props.hyphenation = true;
            s.props.alignment = Alignment::Justify;
        }),
    );
    assert!(l.lines.iter().any(|x| x.end == kiln_text::LineEnd::Hyphen));
}

#[test]
fn german_hyphenation() {
    let mut spec = para(DE, "Cambria", 11.0, 200.0);
    spec.runs[0].style.lang = Some("de-DE".into());
    spec.props.hyphenation = true;
    let l = check("german_hyphenation", &spec);
    assert!(l.lines.iter().any(|x| x.end == kiln_text::LineEnd::Hyphen));
}

#[test]
fn german_without_hyphenation() {
    let mut spec = para(DE, "Cambria", 11.0, 200.0);
    spec.runs[0].style.lang = Some("de-DE".into());
    let l = check("german_without_hyphenation", &spec);
    assert!(l.lines.iter().all(|x| x.end != kiln_text::LineEnd::Hyphen));
}

// ---------------------------------------------------------------- Bidi

#[test]
fn arabic_rtl() {
    let l = check(
        "arabic_rtl",
        &with(para(AR, "Arial", 14.0, 300.0), |s| {
            s.props.direction = Direction::Rtl;
            s.runs[0].style.lang = Some("ar".into());
        }),
    );
    assert!(l.rtl);
    // Start alignment in an RTL paragraph is the right edge.
    let first = &l.lines[0];
    assert!((first.x + first.width - 300.0).abs() < 0.01);
}

#[test]
fn arabic_rtl_justified() {
    let l = check(
        "arabic_rtl_justified",
        &with(para(AR, "Arial", 14.0, 300.0), |s| {
            s.props.direction = Direction::Rtl;
            s.props.alignment = Alignment::Justify;
        }),
    );
    let first = &l.lines[0];
    assert!(first.x.abs() < 0.01 && (first.width - 300.0).abs() < 0.01);
}

#[test]
fn hebrew_rtl() {
    check(
        "hebrew_rtl",
        &with(para(HE, "Arial", 12.0, 260.0), |s| {
            s.props.direction = Direction::Rtl;
        }),
    );
}

#[test]
fn mixed_ltr_paragraph_with_rtl_and_numbers() {
    check_with(
        "mixed_ltr_paragraph_with_rtl_and_numbers",
        &para(
            "The headline مرحبا بالعالم 2026 appeared next to שלום עולם and the figure 1,234.50 in English text.",
            "Calibri",
            11.0,
            300.0,
        ),
        describe_detailed,
    );
}

#[test]
fn mixed_rtl_paragraph_with_ltr_and_numbers() {
    check_with(
        "mixed_rtl_paragraph_with_ltr_and_numbers",
        &with(
            para(
                "قال المدير إن مبيعات Kiln Write ارتفعت بنسبة 25% في عام 2026 مقارنة بالعام الماضي.",
                "Arial",
                12.0,
                260.0,
            ),
            |s| s.props.direction = Direction::Rtl,
        ),
        describe_detailed,
    );
}

#[test]
fn auto_direction_from_first_strong() {
    let l = check(
        "auto_direction_from_first_strong",
        &with(para("שלום world", "Arial", 12.0, 300.0), |s| {
            s.props.direction = Direction::Auto;
        }),
    );
    assert!(l.rtl);
}

// ---------------------------------------------------------------- CJK

#[test]
fn cjk_chinese() {
    check("cjk_chinese", &para(ZH, "Microsoft YaHei", 11.0, 240.0));
}

#[test]
fn cjk_japanese_justified() {
    check(
        "cjk_japanese_justified",
        &with(para(JA, "MS Mincho", 10.5, 250.0), |s| {
            s.props.alignment = Alignment::Justify;
            s.runs[0].style.lang = Some("ja".into());
        }),
    );
}

#[test]
fn cjk_mixed_with_latin() {
    check(
        "cjk_mixed_with_latin",
        &para(
            "Kiln 支持中文、English 和数字 12345 的混合排版。",
            "Calibri",
            11.0,
            180.0,
        ),
    );
}

// ---------------------------------------------------------------- Styles and spacing

#[test]
fn mixed_styles_and_superscript() {
    let base = style("Calibri", 11.0);
    let mut bold = base.clone();
    bold.weight = 700;
    let mut italic = style("Cambria", 14.0);
    italic.italic = true;
    let mut sup = base.clone();
    sup.size = base.size * 0.65;
    sup.baseline_shift = base.size * 0.33;
    check_with(
        "mixed_styles_and_superscript",
        &runs(
            vec![
                TextRun::new("Einstein's ", base.clone()),
                TextRun::new("famous", bold),
                TextRun::new(" equation E = mc", base.clone()),
                TextRun::new("2", sup),
                TextRun::new(" is set in ", base),
                TextRun::new("larger italic Cambria.", italic),
            ],
            260.0,
        ),
        describe_detailed,
    );
}

#[test]
fn letter_spacing_expanded() {
    let mut s = style("Calibri", 11.0);
    s.letter_spacing = 2.0;
    check(
        "letter_spacing_expanded",
        &runs(vec![TextRun::new(EN, s)], 400.0),
    );
}

#[test]
fn line_spacing_double() {
    let l = check(
        "line_spacing_double",
        &with(para(EN, "Calibri", 11.0, 300.0), |s| {
            s.props.line_spacing = LineSpacing::Multiple(2.0);
            s.props.space_before = 8.0;
            s.props.space_after = 10.0;
        }),
    );
    assert!((l.lines[0].top - 8.0).abs() < 0.01);
}

#[test]
fn line_spacing_exactly_and_at_least() {
    check(
        "line_spacing_exactly",
        &with(para(EN, "Calibri", 11.0, 300.0), |s| {
            s.props.line_spacing = LineSpacing::Exactly(12.0);
        }),
    );
    check(
        "line_spacing_at_least",
        &with(para(EN, "Calibri", 11.0, 300.0), |s| {
            s.props.line_spacing = LineSpacing::AtLeast(24.0);
        }),
    );
}

#[test]
fn empty_paragraph() {
    let l = check("empty_paragraph", &para("", "Cambria", 14.0, 300.0));
    assert_eq!(l.lines.len(), 1);
    assert!(l.height > 18.0);
}

#[test]
fn spaces_hang_at_line_end() {
    let l = check(
        "spaces_hang_at_line_end",
        &para(
            "Word        word        word        word        word",
            "Liberation Mono",
            10.0,
            160.0,
        ),
    );
    for line in &l.lines {
        assert!(line.width <= 160.0 + 0.01);
    }
}

#[test]
fn monospace_columns() {
    check(
        "monospace_columns",
        &para(
            "fn main() { println!(\"Hello, Kiln\"); let total: u64 = (1..=100).sum(); }",
            "Courier New",
            10.0,
            250.0,
        ),
    );
}

#[test]
fn decorations_underline_and_strike() {
    let mut u = style("Calibri", 11.0);
    u.underline = UnderlineKind::Single;
    let mut w = style("Calibri", 11.0);
    w.underline = UnderlineKind::Words;
    let mut d = style("Calibri", 11.0);
    d.underline = UnderlineKind::Double;
    let mut s = style("Calibri", 11.0);
    s.strike = StrikeKind::Single;
    check_with(
        "decorations_underline_and_strike",
        &runs(
            vec![
                TextRun::new("Underlined words ", u),
                TextRun::new("words only here ", w),
                TextRun::new("double ", d),
                TextRun::new("and struck through.", s),
            ],
            500.0,
        ),
        describe_detailed,
    );
}

#[test]
fn ligatures_and_kerning_switches() {
    let on = style("Carlito", 11.0);
    let mut off = on.clone();
    off.ligatures = false;
    off.kerning = false;
    let l = check_with(
        "ligatures_and_kerning_switches",
        &runs(
            vec![
                TextRun::new("office fluffy AVATAR | ", on),
                TextRun::new("office fluffy AVATAR", off),
            ],
            600.0,
        ),
        describe_detailed,
    );
    let runs = &l.lines[0].runs;
    let first: usize = runs
        .iter()
        .filter(|r| r.span == 0)
        .map(|r| r.glyphs.len())
        .sum();
    let second: usize = runs
        .iter()
        .filter(|r| r.span == 1)
        .map(|r| r.glyphs.len())
        .sum();
    assert!(
        first < second + 3,
        "ligatures reduce the glyph count ({first} vs {second})"
    );
}

#[test]
fn office_font_substitutions_are_reported() {
    let l = check(
        "office_font_substitutions_are_reported",
        &runs(
            vec![
                TextRun::new("Aptos body text, ", style("Aptos", 11.0)),
                TextRun::new("Wingdings symbols, ", style("Wingdings", 11.0)),
                TextRun::new("and an unknown serif face.", style("Fancy Serif Pro", 11.0)),
            ],
            500.0,
        ),
    );
    assert_eq!(l.substitutions.len(), 3);
}
