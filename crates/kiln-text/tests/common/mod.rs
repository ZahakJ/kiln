// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers for the integration tests: a deterministic engine (bundled fonts plus the test
//! fonts in `tests/fonts/`, no system fonts) and a textual description of a layout for snapshots.
#![allow(dead_code)]

use std::fmt::Write;

use kiln_text::{
    FontStore, FontStoreOptions, LineEnd, ParagraphLayout, ParagraphSpec, SpanStyle, TextEngine,
    TextRun,
};

/// Bundled fonts + test fonts (Noto Naskh Arabic, a Noto Sans CJK SC subset); system fonts off so
/// results are identical on every machine.
pub fn engine() -> TextEngine {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts");
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: true,
        system: false,
        directories: vec![dir.into()],
    });
    assert!(
        fonts.has_family("Noto Naskh Arabic"),
        "test Arabic font missing"
    );
    assert!(
        fonts.has_family("Noto Sans CJK SC"),
        "test CJK font missing"
    );
    fonts.set_script_fallbacks(*b"Arab", &["Noto Naskh Arabic"]);
    for script in [*b"Hani", *b"Hira", *b"Kana"] {
        fonts.set_script_fallbacks(script, &["Noto Sans CJK SC"]);
    }
    TextEngine::new(fonts)
}

/// A style in px from a size in points.
pub fn style(family: &str, pt: f32) -> SpanStyle {
    SpanStyle::new(family, pt * 4.0 / 3.0)
}

/// A one-run paragraph.
pub fn para(text: &str, family: &str, pt: f32, width: f32) -> ParagraphSpec {
    ParagraphSpec::plain(text, style(family, pt), width)
}

/// A paragraph from several runs.
pub fn runs(parts: Vec<TextRun>, width: f32) -> ParagraphSpec {
    ParagraphSpec {
        runs: parts,
        width,
        ..ParagraphSpec::default()
    }
}

fn r2(v: f32) -> f32 {
    // Round to 1/100 px and normalise -0.
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

/// Text description of a layout: geometry and content of every line.
pub fn describe(layout: &ParagraphLayout) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "lines {} | glyphs {} | height {} | rtl {}",
        layout.lines.len(),
        layout.glyph_count(),
        r2(layout.height),
        layout.rtl
    );
    for (i, line) in layout.lines.iter().enumerate() {
        let mut text: String = layout.text()[line.text_range.clone()]
            .chars()
            .filter(|c| *c != '\u{ad}')
            .map(|c| match c {
                '\t' => '→',
                '\n' => '↵',
                c => c,
            })
            .collect();
        if line.end == LineEnd::Hyphen {
            text.push('-');
        }
        let end = match line.end {
            LineEnd::Wrap => "wrap",
            LineEnd::Hyphen => "hyphen",
            LineEnd::Emergency => "emergency",
            LineEnd::LineBreak => "break",
            LineEnd::Paragraph => "end",
        };
        let _ = writeln!(
            out,
            "{i:>2} {end:<9} g={:<3} x={:<7} w={:<7} top={:<7} h={:<6} base={:<7} |{}|",
            line.glyph_count(),
            r2(line.x),
            r2(line.width),
            r2(line.top),
            r2(line.height),
            r2(line.baseline),
            text.trim_end_matches(' ')
        );
    }
    if !layout.substitutions.is_empty() {
        let subs: Vec<String> = layout
            .substitutions
            .iter()
            .map(|s| format!("{} -> {} ({:?})", s.requested, s.family, s.kind))
            .collect();
        let _ = writeln!(out, "substitutions: {}", subs.join(", "));
    }
    out
}

/// Describes decorations and runs too (for style tests).
pub fn describe_detailed(layout: &ParagraphLayout) -> String {
    let mut out = describe(layout);
    for (i, line) in layout.lines.iter().enumerate() {
        for run in &line.runs {
            let first = run.glyphs.first().map_or(0.0, |g| g.x);
            let _ = writeln!(
                out,
                "   line {i} run span={} font={} size={} level={} glyphs={} x0={} text={:?}",
                run.span,
                run.font.0,
                r2(run.size),
                run.level,
                run.glyphs.len(),
                r2(first),
                &layout.text()[run.text_range.clone()]
            );
        }
        for d in &line.decorations {
            let _ = writeln!(
                out,
                "   line {i} {:?} x0={} x1={} y={} t={}",
                d.kind,
                r2(d.x0),
                r2(d.x1),
                r2(d.y),
                r2(d.thickness)
            );
        }
    }
    out
}
