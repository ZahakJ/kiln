//! Prints the layout of a paragraph (development aid): `cargo run -p kiln-text --example dump -- "text" [width] [family]`.
use kiln_text::{FontStore, ParagraphSpec, SpanStyle, TextEngine};

fn main() {
    let mut args = std::env::args().skip(1);
    let text = args
        .next()
        .unwrap_or_else(|| "The quick brown fox jumps over the lazy dog.".into());
    let width: f32 = args.next().and_then(|w| w.parse().ok()).unwrap_or(200.0);
    let family = args.next().unwrap_or_else(|| "Calibri".into());
    let mut engine = TextEngine::new(FontStore::bundled());
    let layout = engine.layout(&ParagraphSpec::plain(
        text,
        SpanStyle::new(family, 14.666_667),
        width,
    ));
    print!("{}", layout.debug_lines());
    for line in &layout.lines {
        println!(
            "top {:.2} h {:.2} base {:.2} x {:.2} w {:.2}",
            line.top, line.height, line.baseline, line.x, line.width
        );
    }
    println!("subs {:?}", layout.substitutions);
}
