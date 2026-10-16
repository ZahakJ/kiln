// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layout benchmark (WP-53 acceptance): 1,000 mixed paragraphs at 600 px.
//!
//! ```sh
//! cargo run --release -p kiln-text --example layout_bench
//! ```
//!
//! The paragraphs mix English (left, justified, indented, with tabs and leaders), German with
//! automatic hyphenation, Arabic right-to-left, Chinese, mixed-direction text and multi-run styled
//! text, about 60 words each, all distinct. Fonts: the bundled OFL fonts plus the test fonts for
//! Arabic and CJK, so the result does not depend on the machine's fonts.
//!
//! Output: `cold_ms` is the first pass (font loading, shaping caches), `total_ms` the time to lay
//! out the 1,000 paragraphs once the engine is warm, which is how an open document re-lays out text.

use std::time::Instant;

use kiln_text::{
    Alignment, Direction, FontStore, FontStoreOptions, ParagraphSpec, SpanStyle, TabAlignment,
    TabLeader, TabStop, TextEngine, TextRun,
};

const EN: &[&str] = &[
    "Kiln",
    "opens",
    "edits",
    "and",
    "saves",
    "real",
    "Office",
    "files",
    "while",
    "looking",
    "like",
    "its",
    "own",
    "product",
    "warmer",
    "calmer",
    "faster",
    "with",
    "features",
    "Office",
    "lacks",
    "such",
    "as",
    "version",
    "history",
    "live",
    "collaboration",
    "and",
    "presenter",
    "view",
    "pagination",
    "is",
    "deterministic",
    "so",
    "documents",
    "print",
    "exactly",
    "as",
    "shown",
];
const DE: &[&str] = &[
    "Die",
    "Silbentrennung",
    "verbessert",
    "das",
    "Schriftbild",
    "von",
    "Blocksatz",
    "erheblich",
    "besonders",
    "bei",
    "schmalen",
    "Spalten",
    "mit",
    "langen",
    "zusammengesetzten",
    "Wörtern",
    "Versicherungsgesellschaft",
    "Rechtsschutz",
    "Dokumentenverwaltung",
    "und",
    "Zeilenumbruch",
];
const AR: &[&str] = &[
    "تُعدّ",
    "اللغة",
    "العربية",
    "من",
    "أكثر",
    "اللغات",
    "انتشاراً",
    "في",
    "العالم",
    "ويتحدث",
    "بها",
    "مليون",
    "شخص",
    "الوطن",
    "العربي",
    "وخارجه",
    "المستند",
    "الصفحة",
];
const ZH: &[&str] = &[
    "中文", "排版", "需要", "在", "汉字", "之间", "允许", "换行", "标点", "符号", "遵循", "禁则",
    "规则", "这是", "一段", "用于", "测试", "的", "简体", "文本", "，", "。",
];

/// A small deterministic generator (no rand dependency in an example).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_possible_truncation)]
        let v = (self.0 >> 33) as usize;
        v % n
    }

    fn words(&mut self, list: &[&str], n: usize, sep: &str) -> String {
        (0..n)
            .map(|_| list[self.next(list.len())])
            .collect::<Vec<_>>()
            .join(sep)
    }
}

fn paragraphs() -> Vec<ParagraphSpec> {
    let mut rng = Lcg(0x5eed);
    let px = |pt: f32| pt * 4.0 / 3.0;
    (0..1000)
        .map(|i| {
            let mut spec = match i % 8 {
                0 | 1 => ParagraphSpec::plain(
                    rng.words(EN, 60, " ") + ".",
                    SpanStyle::new("Calibri", px(11.0)),
                    600.0,
                ),
                2 => {
                    let mut s = ParagraphSpec::plain(
                        rng.words(EN, 60, " "),
                        SpanStyle::new("Cambria", px(12.0)),
                        600.0,
                    );
                    s.props.alignment = Alignment::Justify;
                    s.props.first_line = 36.0;
                    s
                }
                3 => {
                    let mut style = SpanStyle::new("Times New Roman", px(12.0));
                    style.lang = Some("de".into());
                    let mut s = ParagraphSpec::plain(rng.words(DE, 50, " "), style, 600.0);
                    s.props.hyphenation = true;
                    s.props.alignment = Alignment::Justify;
                    s
                }
                4 => {
                    let mut s = ParagraphSpec::plain(
                        rng.words(AR, 55, " "),
                        SpanStyle::new("Arial", px(12.0)),
                        600.0,
                    );
                    s.props.direction = Direction::Rtl;
                    s
                }
                5 => ParagraphSpec::plain(
                    rng.words(ZH, 90, ""),
                    SpanStyle::new("Calibri", px(11.0)),
                    600.0,
                ),
                6 => {
                    let mut s = ParagraphSpec::plain(
                        format!(
                            "{}\t{}\t{}",
                            rng.words(EN, 4, " "),
                            rng.next(1000),
                            rng.words(EN, 30, " ")
                        ),
                        SpanStyle::new("Calibri", px(11.0)),
                        600.0,
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
                                rng.words(EN, 15, " ") + " ",
                                SpanStyle::new("Calibri", px(11.0)),
                            ),
                            TextRun::new(rng.words(EN, 10, " ") + " ", bold),
                            TextRun::new(
                                rng.words(AR, 6, " ") + " 2026 ",
                                SpanStyle::new("Arial", px(11.0)),
                            ),
                            TextRun::new(rng.words(EN, 20, " "), italic),
                        ],
                        width: 600.0,
                        ..ParagraphSpec::default()
                    }
                }
            };
            spec.width = 600.0;
            spec
        })
        .collect()
}

fn main() {
    let test_fonts = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts");
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: true,
        system: false,
        directories: vec![test_fonts.into()],
    });
    fonts.set_script_fallbacks(*b"Arab", &["Noto Naskh Arabic"]);
    fonts.set_script_fallbacks(*b"Hani", &["Noto Sans CJK SC"]);
    let mut engine = TextEngine::new(fonts);
    let specs = paragraphs();

    let cold = Instant::now();
    let mut lines = 0;
    for spec in &specs {
        lines += engine.layout(spec).lines.len();
    }
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;

    // Best of five warm passes, to keep scheduler noise on a shared machine out of the number.
    let mut best = f64::INFINITY;
    let mut glyphs = 0;
    for _ in 0..5 {
        let start = Instant::now();
        glyphs = 0;
        for spec in &specs {
            glyphs += engine.layout(spec).glyph_count();
        }
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!("paragraphs 1000 | lines {lines} | glyphs {glyphs}");
    println!("cold_ms {cold_ms:.1}");
    println!("total_ms {best:.1}");
    if best >= 60.0 {
        eprintln!("budget exceeded: total_ms must be < 60");
        std::process::exit(1);
    }
}
