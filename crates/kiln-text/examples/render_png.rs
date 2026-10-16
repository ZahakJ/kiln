// SPDX-License-Identifier: AGPL-3.0-or-later
//! Renders sample paragraphs through the glyph atlas into a PNG, for looking at the output of the
//! whole pipeline (layout → atlas → composition) without a browser:
//!
//! ```sh
//! cargo run -p kiln-text --example render_png -- out.png [scale] [dark]
//! ```
//!
//! This is a development aid, not the Scene painter (that is `kiln-raster`, WP-54).

use kiln_text::{
    Alignment, AtlasFormat, Direction, FontStore, FontStoreOptions, GlyphAtlas, ParagraphLayout,
    ParagraphSpec, RunGlyphs, SpanStyle, TabAlignment, TabLeader, TabStop, TextEngine, TextRun,
    UnderlineKind,
};

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

impl Canvas {
    fn blend(&mut self, x: i32, y: i32, rgb: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || a <= 0.0 {
            return;
        }
        let p = &mut self.px[y as usize * self.w + x as usize];
        for i in 0..3 {
            p[i] = p[i] * (1.0 - a) + rgb[i] * a;
        }
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, rgb: [f32; 3]) {
        for y in y0.floor() as i32..y1.ceil() as i32 {
            for x in x0.floor() as i32..x1.ceil() as i32 {
                let cov_x = ((x as f32 + 1.0).min(x1) - (x as f32).max(x0)).clamp(0.0, 1.0);
                let cov_y = ((y as f32 + 1.0).min(y1) - (y as f32).max(y0)).clamp(0.0, 1.0);
                self.blend(x, y, rgb, cov_x * cov_y);
            }
        }
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [
        ((c >> 24) & 0xff) as f32 / 255.0,
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
    ]
}

#[allow(clippy::too_many_arguments)]
fn paint(
    canvas: &mut Canvas,
    atlas: &mut GlyphAtlas,
    fonts: &FontStore,
    l: &ParagraphLayout,
    ox: f32,
    oy: f32,
    scale: f32,
    dark: bool,
) {
    let ink = |c: u32| {
        if dark && c == kiln_text::BLACK {
            [0.92, 0.9, 0.86]
        } else {
            rgb(c)
        }
    };
    for line in &l.lines {
        for run in &line.runs {
            let color = ink(run.color);
            match atlas
                .prepare_run(fonts, run, ox * scale, oy * scale, scale)
                .expect("atlas")
            {
                RunGlyphs::Bitmaps(glyphs) => {
                    for g in glyphs {
                        let page = atlas.page(g.glyph.page).expect("page");
                        for yy in 0..g.glyph.height as usize {
                            for xx in 0..g.glyph.width as usize {
                                let (px, py) = (g.glyph.x as usize + xx, g.glyph.y as usize + yy);
                                let a = match page.format {
                                    AtlasFormat::A8 => {
                                        page.data[py * page.width as usize + px] as f32 / 255.0
                                    }
                                    AtlasFormat::Rgba8 => {
                                        page.data[(py * page.width as usize + px) * 4 + 3] as f32
                                            / 255.0
                                    }
                                };
                                canvas.blend(g.x + xx as i32, g.y + yy as i32, color, a);
                            }
                        }
                    }
                }
                RunGlyphs::Outlines(paths) => {
                    // Fill outlines by sampling (nonzero winding) at pixel centres.
                    for path in paths {
                        let bb = kurbo::Shape::bounding_box(&path);
                        for y in bb.y0.floor() as i32..bb.y1.ceil() as i32 {
                            for x in bb.x0.floor() as i32..bb.x1.ceil() as i32 {
                                let mut cov = 0.0;
                                for (sx, sy) in
                                    [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                                {
                                    let p = kurbo::Point::new(f64::from(x) + sx, f64::from(y) + sy);
                                    if kurbo::Shape::winding(&path, p) != 0 {
                                        cov += 0.25;
                                    }
                                }
                                canvas.blend(x, y, color, cov);
                            }
                        }
                    }
                }
            }
        }
        for d in &line.decorations {
            canvas.rect(
                (ox + d.x0) * scale,
                (oy + d.y) * scale,
                (ox + d.x1) * scale,
                (oy + d.y + d.thickness) * scale,
                ink(d.color),
            );
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "kiln-text.png".into());
    let scale: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let dark = args.next().is_some_and(|s| s == "dark");
    let test_fonts = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts");
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: true,
        system: false,
        directories: vec![test_fonts.into()],
    });
    fonts.set_script_fallbacks(*b"Arab", &["Noto Naskh Arabic"]);
    fonts.set_script_fallbacks(*b"Hani", &["Noto Sans CJK SC"]);
    let mut engine = TextEngine::new(fonts);
    let px = |pt: f32| pt * 4.0 / 3.0;
    let width = 560.0;

    let mut title = SpanStyle::new("Cambria", px(26.0));
    title.weight = 700;
    title.color = 0x1f3a_93ff;
    let mut big = SpanStyle::new("Calibri", px(80.0));
    big.color = 0xc24e_1aff;
    let mut body = ParagraphSpec::plain(
        "Kiln lays out every paragraph in Rust with the same metrics the rasteriser uses, so the line \
         breaks on screen, in print and in the exported PDF are identical. This paragraph is justified \
         with hyphenation: incomprehensibilities and extraordinarily long words split cleanly.",
        SpanStyle::new("Calibri", px(11.0)),
        width,
    );
    body.props.alignment = Alignment::Justify;
    body.props.hyphenation = true;
    body.props.first_line = 24.0;
    let mut toc = ParagraphSpec::plain(
        "1. Getting started\t3\n2. Tabs, leaders and indents\t17\n3. Right-to-left text\t42",
        SpanStyle::new("Calibri", px(11.0)),
        width,
    );
    toc.props.tab_stops = vec![TabStop::new(width, TabAlignment::End, TabLeader::Dot)];
    let mut ar = ParagraphSpec::plain(
        "تُعدّ اللغة العربية من أكثر اللغات انتشاراً في العالم، ويتحدث بها أكثر من أربعمائة مليون شخص في الوطن العربي وخارجه.",
        SpanStyle::new("Arial", px(14.0)),
        width,
    );
    ar.props.direction = Direction::Rtl;
    let zh = ParagraphSpec::plain(
        "中文排版需要在汉字之间允许换行，标点符号遵循禁则规则。这是一段用于测试的简体中文文本，包含逗号、句号以及引号「这样」的内容。",
        SpanStyle::new("Calibri", px(12.0)),
        width,
    );
    let mut u = SpanStyle::new("Calibri", px(12.0));
    u.underline = UnderlineKind::Single;
    let mut it = SpanStyle::new("Times New Roman", px(12.0));
    it.italic = true;
    let mut b = SpanStyle::new("Arial", px(12.0));
    b.weight = 700;
    let mixed = ParagraphSpec {
        runs: vec![
            TextRun::new("Mixed runs: ", SpanStyle::new("Calibri", px(12.0))),
            TextRun::new("underlined", u),
            TextRun::new(", ", SpanStyle::new("Calibri", px(12.0))),
            TextRun::new("Times italic", it),
            TextRun::new(", ", SpanStyle::new("Calibri", px(12.0))),
            TextRun::new("Arial bold", b),
            TextRun::new(
                " and the headline مرحبا بالعالم 2026 inside English.",
                SpanStyle::new("Calibri", px(12.0)),
            ),
        ],
        width,
        ..ParagraphSpec::default()
    };
    let paras = [
        ParagraphSpec::plain("The Kiln text engine", title, width),
        body,
        toc,
        mixed,
        ar,
        zh,
        ParagraphSpec::plain("Kiln Ag", big, width),
    ];
    let layouts: Vec<ParagraphLayout> = paras.iter().map(|p| engine.layout(p)).collect();
    let margin = 24.0;
    let total_h: f32 = layouts.iter().map(|l| l.height + 14.0).sum::<f32>() + 2.0 * margin;
    let (w, h) = (
        ((width + 2.0 * margin) * scale) as usize,
        (total_h * scale) as usize,
    );
    let bg = if dark {
        [0.11, 0.105, 0.1]
    } else {
        [0.985, 0.975, 0.96]
    };
    let mut canvas = Canvas {
        w,
        h,
        px: vec![bg; w * h],
    };
    let mut atlas = GlyphAtlas::new();
    let mut y = margin;
    let fonts = engine.fonts();
    for l in &layouts {
        paint(&mut canvas, &mut atlas, fonts, l, margin, y, scale, dark);
        y += l.height + 14.0;
    }
    let bytes: Vec<u8> = canvas
        .px
        .iter()
        .flat_map(|p| p.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    let file = std::fs::File::create(&out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&bytes)
        .expect("png data");
    println!(
        "wrote {out} ({w}x{h}), atlas pages {}, glyphs {}",
        atlas.pages().len(),
        atlas.len()
    );
}
