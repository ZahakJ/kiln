// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hostile paragraph geometry and pathological text: layout must terminate quickly with bounded
//! output. These values reach kiln-text from untrusted documents (`w:ind`, `w:pgSz`, `w:tabs`,
//! `w:spacing`, pasted tokens under `w:autoHyphenation`).

mod common;

use std::time::{Duration, Instant};

use common::{engine, style};
use kiln_text::{ParagraphLayout, ParagraphSpec, SpanStyle, TabAlignment, TabLeader, TabStop};

fn glyph_count(layout: &ParagraphLayout) -> usize {
    layout
        .lines
        .iter()
        .flat_map(|l| &l.runs)
        .map(|r| r.glyphs.len())
        .sum()
}

fn assert_finite(layout: &ParagraphLayout) {
    for line in &layout.lines {
        assert!(line.x.is_finite() && line.width.is_finite() && line.baseline.is_finite());
        for run in &line.runs {
            for g in &run.glyphs {
                assert!(g.x.is_finite() && g.y.is_finite(), "{g:?}");
            }
        }
    }
}

#[test]
fn leader_over_a_huge_width_terminates_with_bounded_glyphs() {
    let mut engine = engine();
    let mut spec = ParagraphSpec::plain("a\tb", SpanStyle::new("Calibri", 14.0), 1e9);
    spec.props.tab_stops = vec![TabStop::new(1e8, TabAlignment::End, TabLeader::Dot)];
    let layout = engine.layout(&spec);
    assert_eq!(layout.lines.len(), 1);
    let n = glyph_count(&layout);
    assert!(n > 2 && n <= 16_384 + 2, "{n} glyphs");
    assert_finite(&layout);
}

#[test]
fn leader_with_a_huge_hanging_indent_terminates() {
    let mut engine = engine();
    let mut spec = ParagraphSpec::plain("\t", SpanStyle::new("Calibri", 14.0), 0.0);
    spec.props.first_line = -1e9;
    spec.props.tab_stops = vec![TabStop::new(
        -50.0,
        TabAlignment::Start,
        TabLeader::Underscore,
    )];
    let layout = engine.layout(&spec);
    assert!(glyph_count(&layout) <= 16_384 + 1);
    assert_finite(&layout);

    // Every leader kind, extreme indents in both directions and far tab stops.
    for leader in [
        TabLeader::Dot,
        TabLeader::Hyphen,
        TabLeader::Underscore,
        TabLeader::MiddleDot,
        TabLeader::Heavy,
    ] {
        for (first, start, end, width, stop) in [
            (-1e9, 0.0, 0.0, 0.0, -50.0),
            (1e9, 1e9, -1e9, 1e9, 1e9),
            (-3e38, -3e38, 3e38, 3e38, 3e38),
            (0.0, -1e7, 0.0, 1e7, 5e6),
        ] {
            let mut spec = ParagraphSpec::plain("x\ty\tz", SpanStyle::new("Calibri", 0.5), width);
            spec.props.first_line = first;
            spec.props.indent_start = start;
            spec.props.indent_end = end;
            spec.props.default_tab = 3e38;
            spec.props.tab_stops = vec![
                TabStop::new(stop, TabAlignment::End, leader),
                TabStop::new(-stop, TabAlignment::Decimal, leader),
            ];
            let layout = engine.layout(&spec);
            assert!(glyph_count(&layout) <= 2 * 16_384 + 3);
            assert_finite(&layout);
        }
    }
}

/// Lays out `spec` and returns the elapsed time of the fastest of three runs.
fn time(engine: &mut kiln_text::TextEngine, spec: &ParagraphSpec) -> (Duration, usize) {
    let mut best = Duration::MAX;
    let mut lines = 0;
    for _ in 0..3 {
        let t = Instant::now();
        let layout = engine.layout(spec);
        best = best.min(t.elapsed());
        lines = layout.lines.len();
    }
    (best, lines)
}

#[test]
fn hyphenation_cost_on_long_tokens_is_bounded() {
    let mut engine = engine();
    let token: String = "abcdefghijklmnopqrstuvwxyz"
        .chars()
        .cycle()
        .take(10_000)
        .collect();
    for width in [600.0, 40.0, 0.0] {
        let mut spec = ParagraphSpec::plain(&token, style("Calibri", 11.0), width);
        let (plain, plain_lines) = time(&mut engine, &spec);
        spec.props.hyphenation = true;
        let (hyph, hyph_lines) = time(&mut engine, &spec);
        assert_eq!(plain_lines, hyph_lines, "a long token is never hyphenated");
        // Before the bound this took 0.8 s / 15 s / 63 s in release builds against 4-6 ms without
        // hyphenation. Allow generous slack for debug builds and a loaded CI machine.
        assert!(
            hyph <= plain * 3 + Duration::from_millis(250),
            "width {width}: {hyph:?} with hyphenation vs {plain:?} without"
        );
    }
}

#[cfg(feature = "hyphenation")]
#[test]
fn ordinary_words_are_still_hyphenated_next_to_long_tokens() {
    let mut engine = engine();
    let long: String = "x".repeat(500);
    let text = format!("{long} hyphenation hyphenation hyphenation");
    let mut spec = ParagraphSpec::plain(&text, style("Calibri", 11.0), 90.0);
    spec.props.hyphenation = true;
    let layout = engine.layout(&spec);
    assert!(
        layout
            .lines
            .iter()
            .any(|l| l.end == kiln_text::LineEnd::Hyphen),
        "expected at least one automatic hyphen"
    );
}

#[test]
fn extreme_letter_spacing_is_clamped() {
    let mut engine = engine();
    let mut s = style("Calibri", 11.0);
    s.letter_spacing = -1e6;
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(20);
    let spec = ParagraphSpec::plain(&text, s.clone(), 300.0);
    let layout = engine.layout(&spec);
    assert!(layout.lines.len() > 1, "condensed text must still wrap");
    assert_finite(&layout);

    s.letter_spacing = 1e9;
    let layout = engine.layout(&ParagraphSpec::plain("abc", s, 1e9));
    assert_finite(&layout);
}
