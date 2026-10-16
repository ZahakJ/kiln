// SPDX-License-Identifier: AGPL-3.0-or-later
//! `text-layout-1000`: 1,000 mixed paragraphs laid out at 600 px by `kiln-text` with warm caches
//! (how an open document re-lays out text after a width change). The paragraphs mix plain,
//! justified and indented English, hyphenated German, tab stops with leaders and multi-run styled
//! text, about 60 words each, in the bundled fonts only, so the result does not depend on the
//! machine. `cold_ms` (first pass: font loading, shaping caches) is reported as an extra figure.

use std::time::Instant;

use kiln_text::{
    Alignment, FontStore, ParagraphSpec, SpanStyle, TabAlignment, TabLeader, TabStop, TextEngine,
    TextRun,
};

use super::RunOptions;
use crate::harness::{Config, Sample, measure};
use crate::synth::Lcg;

const EN: &str = concat!(
    "Kiln opens edits and saves real Office files while looking like its own product ",
    "warmer calmer faster with features lacks such as version history live ",
    "collaboration presenter view pagination is deterministic so documents print ",
    "exactly shown stoneware porcelain glaze firing reduction atmosphere cone",
);
const DE: &str = concat!(
    "Die Silbentrennung verbessert das Schriftbild von Blocksatz erheblich besonders ",
    "bei schmalen Spalten mit langen zusammengesetzten Wörtern ",
    "Versicherungsgesellschaft Dokumentenverwaltung und Zeilenumbruch",
);

const WIDTH: f32 = 600.0;

fn px(pt: f32) -> f32 {
    pt * 4.0 / 3.0
}

/// The paragraphs (deterministic).
pub fn paragraphs(n: usize) -> Vec<ParagraphSpec> {
    let mut rng = Lcg::new(0x5eed);
    (0..n)
        .map(|i| {
            let mut spec = match i % 6 {
                0 | 1 => ParagraphSpec::plain(
                    rng.words(EN, 60, " ") + ".",
                    SpanStyle::new("Calibri", px(11.0)),
                    WIDTH,
                ),
                2 => {
                    let mut s = ParagraphSpec::plain(
                        rng.words(EN, 60, " "),
                        SpanStyle::new("Cambria", px(12.0)),
                        WIDTH,
                    );
                    s.props.alignment = Alignment::Justify;
                    s.props.first_line = 36.0;
                    s
                }
                3 => {
                    let mut style = SpanStyle::new("Times New Roman", px(12.0));
                    style.lang = Some("de".into());
                    let mut s = ParagraphSpec::plain(rng.words(DE, 50, " "), style, WIDTH);
                    s.props.hyphenation = true;
                    s.props.alignment = Alignment::Justify;
                    s
                }
                4 => {
                    let mut s = ParagraphSpec::plain(
                        format!(
                            "{}\t{}\t{}",
                            rng.words(EN, 4, " "),
                            rng.below(1000),
                            rng.words(EN, 30, " ")
                        ),
                        SpanStyle::new("Calibri", px(11.0)),
                        WIDTH,
                    );
                    s.props.tab_stops =
                        vec![TabStop::new(300.0, TabAlignment::End, TabLeader::Dot)];
                    s.props.indent_start = 24.0;
                    s.props.first_line = -24.0;
                    s
                }
                _ => {
                    let mut bold = SpanStyle::new("Calibri", px(11.0));
                    bold.weight = 700;
                    let mut italic = SpanStyle::new("Cambria", px(12.0));
                    italic.italic = true;
                    ParagraphSpec {
                        runs: vec![
                            TextRun::new(
                                rng.words(EN, 20, " ") + " ",
                                SpanStyle::new("Calibri", px(11.0)),
                            ),
                            TextRun::new(rng.words(EN, 15, " ") + " ", bold),
                            TextRun::new(rng.words(EN, 25, " "), italic),
                        ],
                        width: WIDTH,
                        ..ParagraphSpec::default()
                    }
                }
            };
            spec.width = WIDTH;
            spec
        })
        .collect()
}

pub fn run(options: &RunOptions) -> Result<Sample, String> {
    let (n, config) = if options.quick {
        (50, Config::new(0, 2))
    } else {
        (1000, Config::new(2, 15))
    };
    let mut engine = TextEngine::new(FontStore::bundled());
    let specs = paragraphs(n);
    let cold = Instant::now();
    let mut lines = 0usize;
    for s in &specs {
        lines += engine.layout(s).lines.len();
    }
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;
    if lines < n {
        return Err(format!("{n} paragraphs gave only {lines} lines"));
    }
    let mut sample = measure("text-layout-1000", config, || {
        let mut glyphs = 0usize;
        for s in &specs {
            glyphs += engine.layout(s).glyph_count();
        }
        if glyphs == 0 {
            return Err("no glyphs were shaped".into());
        }
        Ok(())
    })?;
    sample.extra.insert("cold_ms".into(), cold_ms);
    #[allow(clippy::cast_precision_loss)]
    sample.extra.insert("lines".into(), lines as f64);
    Ok(sample)
}
