// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentations (`.odp`, `.otp`) ⇄ [`kiln_deck::Presentation`], and the drawing shapes every
//! ODF document kind shares ([`shapes`]).

pub mod graphic;
pub mod read;
pub mod shapes;
pub mod text;
pub mod write;

use std::fmt::Write as _;

use kiln_deck::shape::{Shape, ShapeKind};
use kiln_deck::text::TextBody;
use kiln_deck::{Owner, Presentation};
use kiln_doc::inline::InlineObject;
use kiln_ooxml::drawingml::fill::Fill;

pub use read::{OdpRead, read_odp};
pub use write::{odp_issues, write_odp};

/// The text of a body: paragraphs on lines, line breaks as `\u{b}`, field results.
#[must_use]
pub fn body_text(body: &TextBody) -> String {
    let mut out = String::new();
    for (i, p) in body.paragraphs().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let objects = p.content.objects();
        let mut k = 0;
        for c in p.content.raw_text().chars() {
            if c != kiln_doc::OBJECT_CHAR {
                out.push(c);
                continue;
            }
            match objects.get(k) {
                Some(InlineObject::Break(_)) => out.push('\u{b}'),
                Some(InlineObject::Field(f)) => out.push_str(&f.result),
                _ => {}
            }
            k += 1;
        }
    }
    out
}

fn shapes_text(shapes: &[Shape], out: &mut String) {
    for s in shapes {
        match &s.kind {
            ShapeKind::Geometry(a) => {
                if let Some(t) = &a.text {
                    out.push_str(&body_text(t));
                    out.push('\n');
                }
            }
            ShapeKind::Table(t) => {
                for row in &t.rows {
                    for c in &row.cells {
                        out.push_str(&body_text(&c.text));
                        out.push('\t');
                    }
                    out.push('\n');
                }
            }
            ShapeKind::Group(g) => shapes_text(&g.children, out),
            _ => {}
        }
    }
}

/// Every slide's text and notes in order (the fidelity text measure).
#[must_use]
pub fn deck_text(p: &Presentation) -> String {
    let mut out = String::new();
    for s in &p.slides {
        shapes_text(&s.shapes, &mut out);
        if let Some(n) = &s.notes {
            out.push_str(&body_text(&n.text));
            out.push('\n');
        }
    }
    out
}

fn fill_fact(f: Option<&Fill>, p: &Presentation, owner: Owner) -> String {
    let ctx = p.color_context(owner).unwrap_or_default();
    match f {
        None => "inherit".to_owned(),
        Some(Fill::None) => "none".to_owned(),
        Some(Fill::Solid(c)) => format!("solid:{}", graphic::css(c, &ctx).0),
        Some(Fill::Gradient(g)) => format!("gradient:{}", g.stops.len()),
        Some(Fill::Pattern(_)) => "pattern".to_owned(),
        Some(Fill::Blip(_)) => "picture".to_owned(),
        Some(Fill::Group) => "group".to_owned(),
    }
}

fn text_facts(
    prefix: &str,
    shape: &Shape,
    body: &TextBody,
    p: &Presentation,
    owner: Owner,
    out: &mut Vec<String>,
) {
    let ctx = p.color_context(owner).unwrap_or_default();
    let theme = p
        .theme_of(owner)
        .cloned()
        .unwrap_or_else(kiln_ooxml::drawingml::theme::office_theme);
    for (i, para) in body.paragraphs().enumerate() {
        let lvl = TextBody::level_of(para);
        let Some(r) = p.resolved_paragraph(shape.id, body, para) else {
            continue;
        };
        let bullet = match &r.deck.bullet {
            Some(kiln_deck::text::Bullet::Char(c)) => format!("char:{c}"),
            Some(kiln_deck::text::Bullet::AutoNumber { scheme, start_at }) => {
                format!("num:{scheme}:{start_at}")
            }
            Some(kiln_deck::text::Bullet::Picture(_)) => "picture".to_owned(),
            Some(kiln_deck::text::Bullet::None) | None => "none".to_owned(),
        };
        out.push(format!(
            "{prefix}/p{i}=lvl{lvl}:{bullet}:{:?}",
            r.para.alignment
        ));
        let mut runs = String::new();
        let mut last = String::new();
        let len = para.content.len();
        for off in 0..len {
            let direct = para.content.props_at(off).cloned().unwrap_or_default();
            let rp = text::flatten_run(
                &kiln_doc::merge::merge_run_props(&r.run, Some(&direct)),
                &ctx,
                &theme,
            );
            let color = match rp.color.as_ref().map(|c| c.val) {
                Some(kiln_doc::props::ColorValue::Rgb(c)) => {
                    format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b)
                }
                _ => "auto".to_owned(),
            };
            let key = format!(
                "b{}i{}u{}s{}c{color}f{}",
                u8::from(rp.bold == Some(true)),
                u8::from(rp.italic == Some(true)),
                u8::from(
                    rp.underline
                        .is_some_and(|u| u.style != kiln_doc::st::UnderlineStyle::None)
                ),
                rp.size.unwrap_or(0),
                rp.fonts
                    .as_ref()
                    .and_then(|f| f.ascii.clone())
                    .unwrap_or_default()
            );
            if key != last {
                let _ = write!(runs, "[{key}]");
                last = key;
            }
        }
        out.push(format!("{prefix}/p{i}/runs={runs}"));
    }
}

fn shape_facts(
    prefix: &str,
    shapes: &[Shape],
    p: &Presentation,
    owner: Owner,
    out: &mut Vec<String>,
) {
    for (i, s) in shapes.iter().enumerate() {
        let pf = format!("{prefix}/{i}");
        out.push(format!("{pf}={}:{}", s.kind.label(), s.name));
        if let Some(x) = p.resolved_xfrm(s.id).or(s.xfrm) {
            // Positions to a hundredth of a millimetre: what every ODF producer keeps.
            let r = |v: i64| (v + 180) / 360;
            out.push(format!(
                "{pf}/xfrm={},{},{},{},{},{}{}",
                r(x.x),
                r(x.y),
                r(x.cx),
                r(x.cy),
                (x.rot + 30_000) / 60_000,
                u8::from(x.flip_h),
                u8::from(x.flip_v)
            ));
        }
        if let Some(ph) = s.placeholder {
            out.push(format!("{pf}/ph={}", ph.kind.as_str()));
        }
        if let Some(props) = s.props() {
            if let Some(g) = &props.geometry {
                out.push(format!("{pf}/geom={}", g.preset_name().unwrap_or("custom")));
            }
            let resolved = p.resolved_props(s.id).unwrap_or_else(|| props.clone());
            let theme = p
                .theme_of(owner)
                .cloned()
                .unwrap_or_else(kiln_ooxml::drawingml::theme::office_theme);
            let ctx = p.color_context(owner).unwrap_or_default();
            let (fill, line, _) = graphic::effective(&resolved, &theme, &ctx);
            out.push(format!("{pf}/fill={}", fill_fact(fill.as_ref(), p, owner)));
            out.push(format!(
                "{pf}/line={}",
                line.map_or_else(
                    || "inherit".to_owned(),
                    |l| fill_fact(l.fill.as_ref(), p, owner)
                )
            ));
        }
        match &s.kind {
            ShapeKind::Geometry(a) => {
                if let Some(t) = &a.text {
                    text_facts(&pf, s, t, p, owner, out);
                }
            }
            ShapeKind::Picture(pic) => {
                let size = p.parts.get(&pic.media).map_or(0, |m| m.bytes.len());
                out.push(format!("{pf}/image={size}"));
            }
            ShapeKind::Table(t) => {
                out.push(format!("{pf}/table={}x{}", t.rows.len(), t.grid.len()));
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        out.push(format!(
                            "{pf}/cell{r},{c}={}x{}:{}",
                            cell.grid_span,
                            cell.row_span,
                            body_text(&cell.text)
                        ));
                        if let Some(f) = &cell.props.fill {
                            out.push(format!("{pf}/cell{r},{c}/fill={f:?}"));
                        }
                        let borders: Vec<bool> =
                            cell.props.borders.iter().map(|b| b.0.is_some()).collect();
                        if borders.iter().any(|b| *b) {
                            out.push(format!("{pf}/cell{r},{c}/borders={borders:?}"));
                        }
                    }
                }
            }
            ShapeKind::Group(g) => shape_facts(&pf, &g.children, p, owner, out),
            ShapeKind::Media(m) => out.push(format!("{pf}/media={:?}", m.kind)),
            ShapeKind::Chart(c) => {
                match p
                    .parts
                    .get(&c.part)
                    .and_then(|b| crate::chart::from_drawingml(&b.bytes))
                {
                    Some(cs) => out.extend(crate::chart::facts(&format!("{pf}/chart"), &cs)),
                    None => out.push(format!("{pf}/chart=unreadable")),
                }
            }
            ShapeKind::Connector(_) => {}
        }
    }
}

/// The canonical facts of a presentation for the fidelity scorer: slide size, layouts and
/// their placeholders, and per slide its layout, flags, background, transition, notes and
/// shapes (kind, name, resolved position to 0.01 mm, placeholder, geometry, resolved fill and
/// outline colours, paragraphs with their level, bullet and alignment, runs with their
/// resolved formatting, tables, pictures).
#[must_use]
pub fn deck_facts(p: &Presentation) -> Vec<String> {
    let mut out = vec![format!(
        "size={},{}",
        (p.slide_size.cx + 180) / 360,
        (p.slide_size.cy + 180) / 360
    )];
    let mut layout_index = std::collections::HashMap::new();
    let mut n = 0;
    for m in &p.masters {
        for l in &m.layouts {
            layout_index.insert(l.id, n);
            out.push(format!("layout{n}={}", l.name));
            n += 1;
        }
    }
    for (si, s) in p.slides.iter().enumerate() {
        let sp = format!("slide{si}");
        if let Some((_, l)) = p.slide_layout(s) {
            out.push(format!("{sp}/layout={}", l.name));
        }
        if s.hidden {
            out.push(format!("{sp}/hidden"));
        }
        if let Some(kiln_deck::Background::Fill { fill, .. }) = &s.background {
            out.push(format!(
                "{sp}/bg={}",
                fill_fact(Some(fill), p, Owner::Slide(s.id))
            ));
        }
        if let Some(t) = &s.transition {
            out.push(format!(
                "{sp}/transition={}:{}:{:?}",
                t.effect.name(),
                t.duration_ms,
                t.advance_after_ms
            ));
        }
        for c in &s.comments {
            out.push(format!("{sp}/comment={}:{}", c.author, c.text));
        }
        if let Some(nt) = &s.notes {
            out.push(format!("{sp}/notes={}", body_text(&nt.text)));
        }
        shape_facts(&sp, &s.shapes, p, Owner::Slide(s.id), &mut out);
    }
    out
}
