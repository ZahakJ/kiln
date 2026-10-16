// SPDX-License-Identifier: AGPL-3.0-or-later
//! Renders every bundled theme as an HTML contact sheet for design review:
//!
//! ```sh
//! cargo run -p kiln-deck --example theme_gallery -- out/themes.html [fonts-dir]
//! ```
//!
//! Each theme gets six sample slides (title, title and content, section header, two content,
//! comparison and a colour swatch slide) built through the real model: placeholders resolve
//! position, body properties and text styles through the layout and master exactly as the
//! editor will. Slides are drawn with HTML/CSS (shapes clipped to their preset geometry), so a
//! browser screenshot shows each design. Theme fonts map to the families Kiln bundles (the
//! substitution table of `kiln-text`); `fonts-dir` may add the brand's Fraunces and Figtree.
//! This is a design tool, not the slide renderer (`kiln-deck-layout`).

use std::fmt::Write as _;

use kiln_deck::TextTarget;
use kiln_deck::edit;
use kiln_deck::geom::Transform;
use kiln_deck::inherit::ResolvedParagraph;
use kiln_deck::shape::{Geometry, PlaceholderType, Shape, ShapeKind};
use kiln_deck::text::{Bullet, TextAnchor, TextSpacing};
use kiln_deck::{Background, LayoutKind, NodeId, Owner, Presentation, Rect, SlideSize, themes};
use kiln_doc::st::{Alignment, ThemeFontRef};
use kiln_ooxml::drawingml::color::{ColorContext, resolve_color, rgba_to_css};
use kiln_ooxml::drawingml::fill::{Fill, fill_to_css};
use kiln_ooxml::drawingml::presets::preset_to_svg_path;
use kiln_ooxml::drawingml::theme::Theme;

const W: f64 = 480.0;

/// The family Kiln draws for a theme font name (kiln-text's substitution table).
fn family(name: &str) -> &str {
    match name {
        "Calibri" | "Calibri Light" | "Corbel" | "Candara" => "Carlito",
        "Cambria" | "Constantia" => "Caladea",
        "Arial" | "Trebuchet MS" | "Gill Sans MT" | "Verdana" | "Helvetica" => "Liberation Sans",
        "Georgia" | "Garamond" | "Book Antiqua" | "Palatino Linotype" | "Times New Roman" => {
            "Liberation Serif"
        }
        "Aptos" | "Aptos Display" => "Figtree",
        other => other,
    }
}

fn theme_font(theme: &Theme, r: Option<ThemeFontRef>, direct: Option<&str>) -> String {
    if let Some(d) = direct {
        return family(d).to_owned();
    }
    let major = r.is_some_and(ThemeFontRef::is_major);
    let fs = &theme.font_scheme;
    let f = if major { &fs.major } else { &fs.minor };
    family(&f.latin.typeface).to_owned()
}

fn px(emu: i64, scale: f64) -> f64 {
    emu as f64 * scale
}

fn fill_css(fill: &Fill, ctx: &ColorContext) -> String {
    fill_to_css(fill, ctx).unwrap_or_else(|| "transparent".to_owned())
}

fn background_css(p: &Presentation, owner: Owner, ctx: &ColorContext) -> String {
    match p.resolved_background(owner) {
        Some(Background::Fill { fill, .. }) => fill_css(fill, ctx),
        Some(Background::Theme { idx, color }) => {
            let theme = p.theme_of(owner).expect("theme");
            let styles = &theme.format_scheme.bg_fill_styles;
            let i = usize::try_from(idx.saturating_sub(1001)).unwrap_or(0);
            let mut c = *ctx;
            c.ph_clr = color.as_ref().map(|x| resolve_color(x, ctx));
            styles
                .get(i)
                .map_or_else(|| "white".to_owned(), |f| fill_css(f, &c))
        }
        None => "white".to_owned(),
    }
}

fn geometry_path(g: &Geometry, w: f64, h: f64) -> Option<String> {
    match g {
        Geometry::Preset { name, adjust } => {
            let adj: Vec<(&str, f64)> = adjust
                .iter()
                .map(|(k, v)| (k.as_str(), *v as f64))
                .collect();
            preset_to_svg_path(name, w, h, &adj).ok()
        }
        Geometry::Custom(def) => def.compile().ok().map(|c| c.resolve(w, h, &[]).to_svg()),
    }
}

fn shape_box(x: &Transform, scale: f64) -> String {
    let mut t = String::new();
    if x.rot != 0 {
        let _ = write!(t, "rotate({}deg)", f64::from(x.rot) / 60_000.0);
    }
    if x.flip_h {
        t.push_str(" scaleX(-1)");
    }
    if x.flip_v {
        t.push_str(" scaleY(-1)");
    }
    format!(
        "left:{:.2}px;top:{:.2}px;width:{:.2}px;height:{:.2}px;transform:{}",
        px(x.x, scale),
        px(x.y, scale),
        px(x.cx, scale),
        px(x.cy, scale),
        if t.is_empty() { "none".to_owned() } else { t }
    )
}

fn draw_geometry(out: &mut String, p: &Presentation, s: &Shape, ctx: &ColorContext, scale: f64) {
    let Some(x) = p.resolved_xfrm(s.id) else {
        return;
    };
    let Some(props) = p.resolved_props(s.id) else {
        return;
    };
    let (w, h) = (px(x.cx, scale), px(x.cy, scale));
    let fill = match (&props.fill, &props.style) {
        (Some(f), _) => fill_css(f, ctx),
        (None, Some(st)) if st.fill.idx > 0 => st.fill.color.as_ref().map_or_else(
            || "transparent".to_owned(),
            |c| rgba_to_css(resolve_color(c, ctx)),
        ),
        _ => "transparent".to_owned(),
    };
    let clip = props
        .geometry
        .as_ref()
        .and_then(|g| geometry_path(g, w, h))
        .map(|d| format!("clip-path:path('{d}');"))
        .unwrap_or_default();
    let _ = write!(
        out,
        "<div class=s style=\"{};background:{fill};{clip}\"></div>",
        shape_box(&x, scale)
    );
}

fn align_css(a: Option<Alignment>) -> &'static str {
    match a {
        Some(Alignment::Center) => "center",
        Some(Alignment::Right) => "right",
        Some(Alignment::Both | Alignment::Distribute) => "justify",
        _ => "left",
    }
}

fn draw_text(
    out: &mut String,
    p: &Presentation,
    s: &Shape,
    owner: Owner,
    ctx: &ColorContext,
    scale: f64,
) {
    let Some(body) = s.text() else { return };
    if body.is_empty() {
        return;
    }
    let Some(x) = p.resolved_xfrm(s.id) else {
        return;
    };
    let Some(b) = p.resolved_body(s.id) else {
        return;
    };
    let theme = p.theme_of(owner).expect("theme");
    let [l, t, r, bt] = b.insets;
    let justify = match b.anchor {
        TextAnchor::Middle => "center",
        TextAnchor::Bottom => "flex-end",
        _ => "flex-start",
    };
    let vertical = matches!(
        b.direction,
        kiln_deck::text::TextDirection::EastAsianVertical
            | kiln_deck::text::TextDirection::Rotate90
    );
    let _ = write!(
        out,
        "<div class=t style=\"{};padding:{:.2}px {:.2}px {:.2}px {:.2}px;justify-content:{justify};{}\">",
        shape_box(&x, scale),
        px(t, scale),
        px(r, scale),
        px(bt, scale),
        px(l, scale),
        if vertical {
            "writing-mode:vertical-rl;"
        } else {
            ""
        }
    );
    for para in body.paragraphs() {
        let Some(ResolvedParagraph {
            para: pp,
            deck,
            run,
        }) = p.resolved_paragraph(s.id, body, para)
        else {
            continue;
        };
        let size_pt = f64::from(run.size.unwrap_or(36)) / 2.0;
        let font_px = size_pt * 12_700.0 * scale;
        let line = match deck.line_spacing {
            Some(TextSpacing::Percent(v)) => f64::from(v) / 100_000.0 * 1.2,
            _ => 1.2,
        };
        let before = match deck.space_before {
            Some(TextSpacing::Points(v)) => f64::from(v) / 100.0 * 12_700.0 * scale,
            Some(TextSpacing::Percent(v)) => f64::from(v) / 100_000.0 * font_px * 1.2,
            None => 0.0,
        };
        let ind = pp.indent.unwrap_or_default();
        let left = f64::from(ind.left.unwrap_or(0)) * 635.0 * scale;
        let hanging = f64::from(ind.hanging.unwrap_or(0)) * 635.0 * scale;
        let _ = write!(
            out,
            "<div class=p style=\"text-align:{};margin-top:{before:.2}px;padding-left:{left:.2}px;text-indent:{:.2}px;line-height:{line:.3}\">",
            align_css(pp.alignment),
            -hanging
        );
        if let Some(Bullet::Char(c)) = &deck.bullet {
            let color = match &deck.bullet_color {
                Some(kiln_deck::text::BulletColor::Color(c)) => rgba_to_css(resolve_color(c, ctx)),
                _ => run
                    .color
                    .as_ref()
                    .map(|c| {
                        rgba_to_css(resolve_color(
                            &kiln_deck::dml::color::doc_to_drawing_color(c),
                            ctx,
                        ))
                    })
                    .unwrap_or_default(),
            };
            let _ = write!(
                out,
                "<span style=\"display:inline-block;width:{hanging:.2}px;text-indent:0;color:{color};font-size:{font_px:.2}px\">{c}</span>"
            );
        }
        for (range, direct, text) in para.content.run_texts() {
            let _ = range;
            let eff = kiln_doc::merge::merge_run_props(&run, Some(direct));
            let fonts = eff.fonts.clone().unwrap_or_default();
            let fam = theme_font(theme, fonts.h_ansi_theme, fonts.h_ansi.as_deref());
            let size_pt = f64::from(eff.size.unwrap_or(36)) / 2.0;
            let color = eff
                .color
                .as_ref()
                .map(|c| {
                    rgba_to_css(resolve_color(
                        &kiln_deck::dml::color::doc_to_drawing_color(c),
                        ctx,
                    ))
                })
                .unwrap_or_else(|| "black".to_owned());
            let spacing = f64::from(eff.spacing.unwrap_or(0)) / 20.0 * 12_700.0 * scale;
            let _ = write!(
                out,
                "<span style=\"font-family:'{fam}';font-size:{:.2}px;font-weight:{};font-style:{};color:{color};letter-spacing:{spacing:.2}px;text-transform:{}\">{}</span>",
                size_pt * 12_700.0 * scale,
                if eff.bold == Some(true) { 700 } else { 400 },
                if eff.italic == Some(true) {
                    "italic"
                } else {
                    "normal"
                },
                if eff.caps == Some(true) {
                    "uppercase"
                } else {
                    "none"
                },
                html_escape(&text)
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn draw_slide(out: &mut String, p: &Presentation, slide: NodeId) {
    let scale = W / p.slide_size.cx as f64;
    let h = p.slide_size.cy as f64 * scale;
    let owner = Owner::Slide(slide);
    let ctx = p.color_context(owner).expect("context");
    let _ = write!(
        out,
        "<div class=slide style=\"height:{h:.1}px;background:{}\">",
        background_css(p, owner, &ctx)
    );
    let s = p.slide(slide).expect("slide");
    let (master, layout) = p.layout(s.layout).expect("layout");
    let mut decor: Vec<&Shape> = Vec::new();
    if p.shows_master_shapes(slide) {
        decor.extend(master.shapes.iter().filter(|x| x.placeholder.is_none()));
    }
    decor.extend(layout.shapes.iter().filter(|x| x.placeholder.is_none()));
    for d in decor {
        draw_geometry(out, p, d, &ctx, scale);
    }
    for sh in &s.shapes {
        if matches!(sh.kind, ShapeKind::Geometry(_)) {
            draw_geometry(out, p, sh, &ctx, scale);
            draw_text(out, p, sh, owner, &ctx, scale);
        }
    }
    out.push_str("</div>");
}

fn set(p: &mut Presentation, slide: NodeId, kind: PlaceholderType, n: usize, text: &str) {
    let id = p
        .slide(slide)
        .expect("slide")
        .shapes
        .iter()
        .filter(|s| s.placeholder.is_some_and(|ph| ph.kind == kind))
        .nth(n)
        .map(|s| s.id);
    let Some(id) = id else { return };
    let tx = edit::set_text(p, TextTarget::Shape(id), text).expect("set text");
    p.apply(tx).expect("apply");
}

fn levels(p: &mut Presentation, slide: NodeId, kind: PlaceholderType, n: usize, lv: &[i32]) {
    let id = p
        .slide(slide)
        .expect("slide")
        .shapes
        .iter()
        .filter(|s| s.placeholder.is_some_and(|ph| ph.kind == kind))
        .nth(n)
        .map(|s| s.id)
        .expect("placeholder");
    let paras: Vec<NodeId> = p
        .shape(id)
        .and_then(Shape::text)
        .expect("text")
        .paragraphs()
        .map(|x| x.id)
        .collect();
    for (para, l) in paras.iter().zip(lv) {
        if *l > 0 {
            let tx =
                edit::set_paragraph_level(p, TextTarget::Shape(id), &[*para], *l).expect("level");
            p.apply(tx).expect("apply");
        }
    }
}

fn add(p: &mut Presentation, kind: LayoutKind) -> NodeId {
    let l = p.masters[0].layout_of_kind(&kind).expect("layout").id;
    let i = p.slides.len();
    let tx = edit::insert_slide(p, l, i).expect("insert");
    p.apply(tx).expect("apply");
    p.slides[i].id
}

fn sample_deck(t: &themes::ThemeFile, variant: Option<usize>) -> Presentation {
    use PlaceholderType as T;
    let mut p = Presentation::with_theme(t, SlideSize::WIDESCREEN, variant);
    let s = add(&mut p, LayoutKind::Title);
    set(&mut p, s, T::CenteredTitle, 0, "Open Studio Weekend");
    set(&mut p, s, T::Subtitle, 0, "15–16 November · The Clay Barn");
    let s = add(&mut p, LayoutKind::TitleContent);
    set(&mut p, s, T::Title, 0, "Every ticket fires the next kiln");
    set(
        &mut p,
        s,
        T::Object,
        0,
        "Ticket money goes straight back into the studio\nGas and kiln time\nClay and glaze for the open sessions\nBursaries for new members\nShelves and repairs",
    );
    levels(&mut p, s, T::Object, 0, &[0, 1, 1, 0, 0]);
    let s = add(&mut p, LayoutKind::SectionHeader);
    set(&mut p, s, T::Title, 0, "Part two: the firing");
    set(
        &mut p,
        s,
        T::Body,
        0,
        "What happens between Friday night and Sunday noon",
    );
    let s = add(&mut p, LayoutKind::TwoContent);
    set(&mut p, s, T::Title, 0, "Raku or stoneware?");
    set(
        &mut p,
        s,
        T::Object,
        0,
        "Raku\nFast, dramatic, smoky\nOut of the kiln in an hour",
    );
    levels(&mut p, s, T::Object, 0, &[0, 1, 1]);
    set(
        &mut p,
        s,
        T::Object,
        1,
        "Stoneware\nSlow and strong\nReady on Monday",
    );
    levels(&mut p, s, T::Object, 1, &[0, 1, 1]);
    let s = add(&mut p, LayoutKind::Comparison);
    set(&mut p, s, T::Title, 0, "This year and last");
    set(&mut p, s, T::Body, 0, "2025");
    set(&mut p, s, T::Object, 0, "212 visitors\n£4,900 raised");
    set(&mut p, s, T::Body, 1, "2026");
    set(&mut p, s, T::Object, 1, "340 visitors\n£6,400 raised");
    // A swatch slide: the six accents as Quick Style shapes.
    let s = add(&mut p, LayoutKind::TitleOnly);
    set(&mut p, s, T::Title, 0, "Six accents");
    let cx = 1_500_000;
    let shapes: Vec<Shape> = (0..6)
        .map(|i| {
            let mut sh = Shape::geometry(
                p.alloc_id(),
                if i % 2 == 0 { "roundRect" } else { "ellipse" },
                Rect::new(900_000 + i64::from(i) * (cx + 230_000), 2_700_000, cx, cx),
            );
            if let Some(props) = sh.props_mut()
                && let Some(st) = props.style.as_mut()
            {
                st.fill.color = Some(kiln_ooxml::drawingml::color::DrawingColor::scheme(
                    &format!("accent{}", i + 1),
                ));
            }
            sh
        })
        .collect();
    let tx = edit::insert_shapes(&mut p, s, shapes, None).expect("shapes");
    p.apply(tx).expect("apply");
    p
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out_path = args.next().unwrap_or_else(|| "themes.html".to_owned());
    let fonts = args.next();
    let mut html =
        String::from("<!doctype html><meta charset=utf-8><title>Kiln Deck themes</title><style>");
    if let Some(dir) = &fonts {
        for (fam, file) in [
            ("Fraunces", "Fraunces-VF.ttf"),
            ("Figtree", "Figtree-VF.ttf"),
            ("Carlito", "carlito/Carlito-Regular.ttf"),
            ("Caladea", "caladea/Caladea-Regular.ttf"),
            ("Liberation Sans", "liberation/LiberationSans-Regular.ttf"),
            ("Liberation Serif", "liberation/LiberationSerif-Regular.ttf"),
        ] {
            let _ = write!(
                html,
                "@font-face{{font-family:'{fam}';src:url('{dir}/{file}');font-weight:100 900}}"
            );
        }
        for (fam, file) in [
            ("Carlito", "carlito/Carlito-Bold.ttf"),
            ("Caladea", "caladea/Caladea-Bold.ttf"),
            ("Liberation Sans", "liberation/LiberationSans-Bold.ttf"),
            ("Liberation Serif", "liberation/LiberationSerif-Bold.ttf"),
        ] {
            let _ = write!(
                html,
                "@font-face{{font-family:'{fam}';src:url('{dir}/{file}');font-weight:700}}"
            );
        }
    }
    html.push_str(
        "body{margin:0;padding:24px;background:#d8d2ca;font:13px system-ui}h2{margin:28px 0 8px;font:600 18px system-ui}\
         .row{display:flex;flex-wrap:wrap;gap:14px}.slide{position:relative;width:480px;overflow:hidden;box-shadow:0 1px 4px rgba(0,0,0,.25)}\
         .s,.t{position:absolute;box-sizing:border-box}.t{display:flex;flex-direction:column;overflow:visible}.p{white-space:pre-wrap}\
         </style>",
    );
    for t in themes::bundled() {
        let p = sample_deck(&t, None);
        let _ = write!(
            html,
            "<section id={}><h2>{} — {}</h2><div class=row>",
            t.id,
            t.name,
            html_escape(&t.description)
        );
        for s in &p.slides {
            draw_slide(&mut html, &p, s.id);
        }
        html.push_str("</div><div class=row style=\"margin-top:10px\">");
        for v in 0..t.variants.len() {
            let pv = sample_deck(&t, Some(v));
            let _ = write!(html, "<div><div>{}</div>", t.variants[v].name);
            draw_slide(&mut html, &pv, pv.slides[1].id);
            html.push_str("</div>");
        }
        html.push_str("</div></section>");
    }
    std::fs::write(&out_path, html).expect("write the gallery");
    println!("wrote {out_path}");
}
