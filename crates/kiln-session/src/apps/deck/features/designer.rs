// SPDX-License-Identifier: AGPL-3.0-or-later
//! Design Ideas: layouts and a theme variant suggested for the current slide from what
//! it holds — its title, its text (and how many paragraphs), its pictures, tables and charts —
//! computed here, without a service. Each idea is a plan: where the title, the text and the
//! pictures or other visuals go, the decoration behind them in the theme's colours, light text
//! on dark decoration, and the dark variant (the theme's second dark colour behind light
//! text). The pane shows each idea drawn by `kiln-raster`; applying one is one undo
//! step, and applying another replaces it (the decoration ideas add is named "Design Element").
//!
//! Pictures share the area they get in a grid whose cells are closest to 4:3, each cropped to
//! fill its cell; charts are sized to a cell, tables and groups keep their size and move to it.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::dml::color::drawing_to_doc_color;
use kiln_deck::ops::{EditKind, Op, TextTarget, Transaction};
use kiln_deck::shape::{AutoShape, Crop, Geometry, ShapeProps};
use kiln_deck::text::{LEVELS, ListStyle};
use kiln_deck::{Background, LayoutKind, Presentation, Shape, ShapeKind, Transform};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_protocol::commands::deck_features::DeckDesignIdea;

use super::super::DeckDoc;
use crate::model::Change;

/// The name of the shapes ideas add (removed when another idea is applied).
const DECOR: &str = "Design Element";

/// What the current slide holds.
#[derive(Debug, Clone)]
struct Content {
    slide: NodeId,
    w: i64,
    h: i64,
    title: Option<NodeId>,
    /// Text shapes other than the title (body placeholders, subtitles, text boxes).
    texts: Vec<NodeId>,
    /// Paragraphs with text in `texts`.
    paragraphs: usize,
    pictures: Vec<NodeId>,
    /// Tables, charts and groups.
    visuals: Vec<NodeId>,
    /// Shapes an idea added before.
    decor: Vec<NodeId>,
    /// The layout is a title slide or section header.
    title_slide: bool,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            slide: NodeId(0),
            w: 0,
            h: 0,
            title: None,
            texts: Vec::new(),
            paragraphs: 0,
            pictures: Vec::new(),
            visuals: Vec::new(),
            decor: Vec::new(),
            title_slide: false,
        }
    }
}

/// A rectangle as fractions of the slide.
#[derive(Debug, Clone, Copy, PartialEq)]
struct R(f64, f64, f64, f64);

/// A piece of decoration: where, in which theme colour, how opaque (1000ths of a percent).
#[derive(Debug, Clone)]
struct Decor {
    at: R,
    color: &'static str,
    alpha: Option<i64>,
}

/// Where an idea puts things.
#[derive(Debug, Clone)]
struct Plan {
    title: R,
    text: R,
    /// Pictures and other visuals (when the slide has any).
    visual: Option<R>,
    decor: Vec<Decor>,
    /// The title is on dark decoration.
    light_title: bool,
    /// The text is on dark decoration.
    light_text: bool,
    /// The dark variant.
    dark: bool,
    /// Title and text centred.
    centred: bool,
}

/// The ideas, in the order the pane lists them, and the names it reads out.
const IDEAS: [(&str, &str); 10] = [
    ("pictureRight", "Picture on the right"),
    ("pictureLeft", "Picture on the left"),
    ("pictureTop", "Picture across the top"),
    ("pictureFull", "Full-bleed picture"),
    ("band", "Title band"),
    ("panel", "Side panel"),
    ("bar", "Accent bar"),
    ("dark", "Dark"),
    ("sideTitle", "Title on the left"),
    ("statement", "Statement"),
];

/// A text area left of a picture and the picture area (fractions).
const M: f64 = 0.055;

fn plan(id: &str, c: &Content) -> Option<Plan> {
    let has_visual = !c.pictures.is_empty() || !c.visuals.is_empty();
    let pictures = !c.pictures.is_empty();
    let base = Plan {
        title: R(M, 0.06, 1.0 - 2.0 * M, 0.17),
        text: R(M, 0.28, 1.0 - 2.0 * M, 0.64),
        visual: None,
        decor: Vec::new(),
        light_title: false,
        light_text: false,
        dark: false,
        centred: false,
    };
    // With text, visuals take the right of the text area; alone, all of it.
    let split = |p: Plan| -> Plan {
        if has_visual && c.texts.is_empty() {
            Plan {
                visual: Some(p.text),
                ..p
            }
        } else if has_visual {
            Plan {
                text: R(M, p.text.1, 0.40, p.text.3),
                visual: Some(R(0.50, p.text.1, 0.50 - M, p.text.3)),
                ..p
            }
        } else {
            p
        }
    };
    Some(match id {
        "pictureRight" | "pictureLeft" if pictures => {
            let left = id == "pictureLeft";
            let (tx, vx) = if left { (0.53, 0.0) } else { (M, 0.5) };
            Plan {
                title: R(tx, 0.08, 0.42, 0.2),
                text: R(tx, 0.32, 0.42, 0.6),
                visual: Some(R(vx, 0.0, 0.5, 1.0)),
                ..base
            }
        }
        "pictureTop" if pictures => Plan {
            title: R(M, 0.6, 1.0 - 2.0 * M, 0.13),
            text: R(M, 0.74, 1.0 - 2.0 * M, 0.22),
            visual: Some(R(0.0, 0.0, 1.0, 0.55)),
            ..base
        },
        "pictureFull" if pictures && c.paragraphs <= 3 => Plan {
            title: R(M, 0.7, 1.0 - 2.0 * M, 0.14),
            text: R(M, 0.84, 1.0 - 2.0 * M, 0.12),
            visual: Some(R(0.0, 0.0, 1.0, 1.0)),
            decor: vec![Decor {
                at: R(0.0, 0.66, 1.0, 0.34),
                color: "tx1",
                alpha: Some(62_000),
            }],
            light_title: true,
            light_text: true,
            ..base
        },
        "band" => {
            if c.title_slide || (!has_visual && c.paragraphs <= 2) {
                Plan {
                    title: R(M, 0.33, 1.0 - 2.0 * M, 0.22),
                    text: R(M, 0.64, 1.0 - 2.0 * M, 0.2),
                    decor: vec![Decor {
                        at: R(0.0, 0.3, 1.0, 0.28),
                        color: "accent1",
                        alpha: None,
                    }],
                    light_title: true,
                    ..base
                }
            } else {
                split(Plan {
                    title: R(M, 0.04, 1.0 - 2.0 * M, 0.17),
                    text: R(M, 0.3, 1.0 - 2.0 * M, 0.62),
                    decor: vec![Decor {
                        at: R(0.0, 0.0, 1.0, 0.24),
                        color: "accent1",
                        alpha: None,
                    }],
                    light_title: true,
                    ..base
                })
            }
        }
        "panel" => {
            let text = if has_visual {
                R(M, 0.42, 0.36 - 2.0 * M, 0.5)
            } else {
                R(0.41, 0.1, 0.59 - M, 0.8)
            };
            Plan {
                title: if has_visual {
                    R(M, 0.1, 0.36 - 2.0 * M, 0.3)
                } else {
                    R(M, 0.1, 0.36 - 2.0 * M, 0.8)
                },
                text,
                visual: has_visual.then_some(R(0.41, 0.1, 0.59 - M, 0.8)),
                decor: vec![Decor {
                    at: R(0.0, 0.0, 0.36, 1.0),
                    color: "tx2",
                    alpha: None,
                }],
                light_title: true,
                light_text: has_visual,
                ..base
            }
        }
        "sideTitle" if !has_visual && c.title.is_some() && !c.texts.is_empty() => Plan {
            title: R(M, 0.12, 0.3, 0.5),
            text: R(0.4, 0.12, 0.6 - M, 0.76),
            decor: vec![Decor {
                at: R(M, 0.64, 0.08, 0.01),
                color: "accent1",
                alpha: None,
            }],
            ..base
        },
        "bar" | "dark" => {
            let dark = id == "dark";
            split(Plan {
                title: R(M + 0.03, 0.07, 1.0 - 2.0 * M - 0.03, 0.16),
                text: R(M + 0.03, 0.28, 1.0 - 2.0 * M - 0.03, 0.64),
                decor: vec![Decor {
                    at: R(M, 0.08, 0.012, 0.14),
                    color: "accent1",
                    alpha: None,
                }],
                dark,
                light_title: dark,
                light_text: dark,
                ..base
            })
        }
        "statement" if !has_visual && c.paragraphs <= 2 && c.title.is_some() => Plan {
            title: R(0.1, 0.22, 0.8, 0.3),
            text: R(0.15, 0.6, 0.7, 0.24),
            decor: vec![Decor {
                at: R(0.44, 0.555, 0.12, 0.008),
                color: "accent1",
                alpha: None,
            }],
            centred: true,
            ..base
        },
        _ => return None,
    })
}

/// The ideas for a slide: four to six.
fn ideas_for(c: &Content) -> Vec<(&'static str, &'static str, Plan)> {
    let mut out: Vec<_> = IDEAS
        .iter()
        .filter_map(|(id, label)| plan(id, c).map(|p| (*id, *label, p)))
        .collect();
    out.truncate(6);
    out
}

fn content(pres: &Presentation, slide: NodeId) -> Option<Content> {
    let s = pres.slide(slide)?;
    let mut c = Content {
        slide,
        w: pres.slide_size.cx,
        h: pres.slide_size.cy,
        title_slide: pres
            .layout(s.layout)
            .is_some_and(|(_, l)| matches!(l.kind, LayoutKind::Title | LayoutKind::SectionHeader)),
        ..Content::default()
    };
    for sh in &s.shapes {
        if sh.hidden {
            continue;
        }
        if sh.name.starts_with(DECOR) {
            c.decor.push(sh.id);
            continue;
        }
        let ph = sh.placeholder.map(|p| p.kind);
        if ph.is_some_and(kiln_deck::PlaceholderType::is_footer_area) {
            continue;
        }
        match &sh.kind {
            ShapeKind::Picture(_) => c.pictures.push(sh.id),
            ShapeKind::Table(_) | ShapeKind::Chart(_) | ShapeKind::Group(_) => {
                c.visuals.push(sh.id);
            }
            ShapeKind::Geometry(_) => {
                let Some(t) = sh.text() else { continue };
                if ph.is_some_and(kiln_deck::PlaceholderType::is_title) {
                    c.title = Some(sh.id);
                } else if !t.is_empty() {
                    c.texts.push(sh.id);
                    c.paragraphs += t
                        .paragraphs()
                        .filter(|p| !p.content.plain_text(true).trim().is_empty())
                        .count();
                }
            }
            ShapeKind::Connector(_) | ShapeKind::Media(_) => {}
        }
    }
    Some(c)
}

/// EMU of a fraction of the slide.
#[allow(clippy::cast_possible_truncation)]
fn emu(r: R, c: &Content) -> Transform {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (c.w as f64, c.h as f64);
    Transform::new(
        (r.0 * w).round() as i64,
        (r.1 * h).round() as i64,
        (r.2 * w).round().max(1.0) as i64,
        (r.3 * h).round().max(1.0) as i64,
    )
}

/// The size of a picture's image in pixels.
pub(crate) fn image_size(pres: &Presentation, media: &str) -> Option<(u32, u32)> {
    let part = pres.parts.get(media)?;
    image::ImageReader::new(std::io::Cursor::new(&part.bytes[..]))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// A crop that makes an image of `img` pixels fill a box of `cx` × `cy` EMU, centred.
pub(crate) fn fill_crop((iw, ih): (u32, u32), cx: i64, cy: i64) -> Crop {
    let img = f64::from(iw.max(1)) / f64::from(ih.max(1));
    #[allow(clippy::cast_precision_loss)]
    let area = cx.max(1) as f64 / cy.max(1) as f64;
    #[allow(clippy::cast_possible_truncation)]
    let cut = |keep: f64| ((1.0 - keep) / 2.0 * 100_000.0).round() as i32;
    if img > area {
        let c = cut(area / img);
        Crop {
            l: c,
            t: 0,
            r: c,
            b: 0,
        }
    } else {
        let c = cut(img / area);
        Crop {
            l: 0,
            t: c,
            r: 0,
            b: c,
        }
    }
}

/// `n` cells of a grid over `t` with gaps, rows and columns chosen so the cells are closest to
/// 4:3, in reading order.
fn grid(t: Transform, n: usize, gap: i64) -> Vec<Transform> {
    let n = n.max(1);
    let cells = |cols: usize| {
        let rows = n.div_ceil(cols);
        let (c, r) = (
            i64::try_from(cols).unwrap_or(1),
            i64::try_from(rows).unwrap_or(1),
        );
        ((t.cx - gap * (c - 1)) / c, (t.cy - gap * (r - 1)) / r, rows)
    };
    let cols = (1..=n)
        .min_by(|a, b| {
            let off = |cols: usize| {
                let (w, h, _) = cells(cols);
                (f64_of(w) / f64_of(h.max(1)) / (4.0 / 3.0)).ln().abs()
            };
            off(*a).total_cmp(&off(*b))
        })
        .unwrap_or(1);
    let (w, h, _) = cells(cols);
    (0..n)
        .map(|k| {
            let (c, r) = (
                i64::try_from(k % cols).unwrap_or(0),
                i64::try_from(k / cols).unwrap_or(0),
            );
            Transform::new(t.x + c * (w + gap), t.y + r * (h + gap), w.max(1), h.max(1))
        })
        .collect()
}

/// A shape's own list style with every level's text in `color` (or that colour taken away).
fn recoloured(own: &ListStyle, color: Option<&kiln_doc::DocColor>) -> ListStyle {
    let mut out = own.clone();
    for level in 0..LEVELS {
        let mut l = own.get_level(level).cloned().unwrap_or_default();
        l.run.color = color.cloned();
        if l == kiln_deck::text::LevelStyle::default() {
            out.clear_level(level);
        } else {
            out.set_level(level, l);
        }
    }
    out
}

/// The light text colour ideas give text on dark decoration.
fn light() -> kiln_doc::DocColor {
    drawing_to_doc_color(&DrawingColor::scheme("bg1"), None)
}

/// Builds the transaction of an idea on `pres` (ids for the decoration come from it).
fn build(pres: &mut Presentation, c: &Content, p: &Plan) -> Result<Transaction> {
    let mut ops = Vec::new();
    // The last idea's decoration goes.
    if let Some(s) = pres.slide(c.slide) {
        let mut idx: Vec<usize> = s
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, sh)| c.decor.contains(&sh.id))
            .map(|(i, _)| i)
            .collect();
        idx.sort_unstable_by(|a, b| b.cmp(a));
        for index in idx {
            ops.push(Op::RemoveShapes {
                parent: c.slide,
                index,
                count: 1,
            });
        }
    }
    // The dark variant: the theme's second dark colour behind the slide (light text below).
    let slide = pres
        .slide(c.slide)
        .ok_or_else(|| KilnError::InvalidInput("the slide is gone".into()))?;
    let dark_bg = Background::Fill {
        fill: Fill::Solid(DrawingColor::scheme("tx2")),
        shade_to_title: false,
    };
    let is_dark = slide.background.as_ref() == Some(&dark_bg);
    if p.dark != is_dark {
        ops.push(Op::SetBackground {
            owner: c.slide,
            background: p.dark.then_some(dark_bg),
        });
    }
    // Text: where it goes, light on dark decoration.
    let place_text = |ops: &mut Vec<Op>, id: NodeId, light_text: bool| {
        let Some(sh) = pres.shape(id) else { return };
        let Some(t) = sh.text() else { return };
        let want = light_text.then(light);
        let style = recoloured(&t.list_style, want.as_ref());
        if style != t.list_style {
            ops.push(Op::SetListStyle {
                target: TextTarget::Shape(id),
                style: Box::new(style),
            });
        }
    };
    if let Some(t) = c.title {
        ops.push(Op::SetTransform {
            shape: t,
            xfrm: Some(emu(p.title, c)),
        });
        place_text(&mut ops, t, p.light_title);
    }
    let area = emu(p.text, c);
    let n = i64::try_from(c.texts.len().max(1)).unwrap_or(1);
    let gap = c.h / 40;
    for (k, id) in c.texts.iter().enumerate() {
        let k = i64::try_from(k).unwrap_or(0);
        let hh = (area.cy - gap * (n - 1)) / n;
        ops.push(Op::SetTransform {
            shape: *id,
            xfrm: Some(Transform::new(
                area.x,
                area.y + k * (hh + gap),
                area.cx,
                hh.max(1),
            )),
        });
        place_text(&mut ops, *id, p.light_text);
    }
    if p.centred {
        for id in c.title.iter().chain(&c.texts) {
            let Some(b) = pres.shape(*id).and_then(Shape::text) else {
                continue;
            };
            let paras: Vec<NodeId> = b.paragraphs().map(|x| x.id).collect();
            let tx = kiln_deck::edit::text::edit_text(pres, TextTarget::Shape(*id), |doc| {
                let mut ops = Vec::new();
                for para in &paras {
                    if let Some(p) = doc.paragraph(*para) {
                        let mut props = p.props.clone();
                        props.alignment = Some(kiln_doc::st::Alignment::Center);
                        ops.push(kiln_doc::Op::SetParagraphProps {
                            para: *para,
                            props: Box::new(props),
                        });
                    }
                }
                Ok(kiln_doc::Transaction::new(ops))
            })
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            ops.extend(tx.ops);
        }
    }
    // Pictures fill their area (a row of them); charts take its size; tables move to it.
    if let Some(v) = p.visual {
        let area = emu(v, c);
        let gap = if v.2 >= 0.99 && v.3 >= 0.99 {
            0
        } else {
            c.w / 80
        };
        let all: Vec<NodeId> = c.pictures.iter().chain(&c.visuals).copied().collect();
        let cells = grid(area, all.len(), gap);
        for (id, cell) in all.iter().zip(cells) {
            let Some(sh) = pres.shape(*id) else { continue };
            match &sh.kind {
                ShapeKind::Picture(pic) => {
                    let mut pic = pic.clone();
                    if let Some(size) = image_size(pres, &pic.media) {
                        pic.crop = fill_crop(size, cell.cx, cell.cy);
                    }
                    ops.push(Op::SetShapeKind {
                        shape: *id,
                        kind: Box::new(ShapeKind::Picture(pic)),
                    });
                    ops.push(Op::SetTransform {
                        shape: *id,
                        xfrm: Some(cell),
                    });
                }
                ShapeKind::Table(_) | ShapeKind::Group(_) => {
                    // Tables and groups keep their size, scaled down only to fit; a table's
                    // size is its grid's.
                    let now = kiln_deck_layout::render::absolute_xfrm(pres, *id).unwrap_or(cell);
                    let (tw, th) = match &sh.kind {
                        ShapeKind::Table(t) => (
                            t.grid.iter().sum::<i64>().max(now.cx),
                            t.rows.iter().map(|r| r.height).sum::<i64>().max(now.cy),
                        ),
                        _ => (now.cx, now.cy),
                    };
                    let s = (f64_of(cell.cx) / f64_of(tw.max(1)))
                        .min(f64_of(cell.cy) / f64_of(th.max(1)))
                        .min(1.0);
                    #[allow(clippy::cast_possible_truncation)]
                    let scaled = |v: i64| (f64_of(v) * s).round() as i64;
                    let (w, h) = (scaled(tw), scaled(th));
                    if let ShapeKind::Table(t) = &sh.kind
                        && s < 1.0
                    {
                        let mut t = t.clone();
                        for g in &mut t.grid {
                            *g = scaled(*g).max(1);
                        }
                        for r in &mut t.rows {
                            r.height = scaled(r.height).max(1);
                        }
                        ops.push(Op::SetShapeKind {
                            shape: *id,
                            kind: Box::new(ShapeKind::Table(t)),
                        });
                    }
                    ops.push(Op::SetTransform {
                        shape: *id,
                        xfrm: Some(Transform::new(
                            cell.x + (cell.cx - w) / 2,
                            cell.y + (cell.cy - h) / 2,
                            w.max(1),
                            h.max(1),
                        )),
                    });
                }
                _ => ops.push(Op::SetTransform {
                    shape: *id,
                    xfrm: Some(cell),
                }),
            }
        }
    }
    // The decoration, behind everything; pictures that fill the slide go behind it.
    let full_bleed = p.visual.is_some_and(|v| v.2 >= 0.99 && v.3 >= 0.99);
    let mut decor = Vec::new();
    for (k, d) in p.decor.iter().enumerate() {
        let id = pres.alloc_id();
        let mut color = DrawingColor::scheme(d.color);
        if let Some(a) = d.alpha {
            color = color.with("alpha", a);
        }
        let mut s = Shape::new(
            id,
            format!("{DECOR} {}", k + 1),
            Some(emu(d.at, c)),
            ShapeKind::Geometry(AutoShape {
                text_box: false,
                props: ShapeProps {
                    geometry: Some(Geometry::Preset {
                        name: "rect".into(),
                        adjust: Vec::new(),
                    }),
                    fill: Some(Fill::Solid(color)),
                    line: Some(Line {
                        fill: Some(Fill::None),
                        ..Line::default()
                    }),
                    effects: None,
                    style: None,
                },
                text: None,
            }),
        );
        s.alt_text = None;
        decor.push(s);
    }
    // The z-order once the last idea's decoration is gone: a full-bleed picture goes to the
    // back, and the new decoration over it, under the text.
    let mut order: Vec<NodeId> = pres
        .slide(c.slide)
        .map(|s| {
            s.shapes
                .iter()
                .map(|sh| sh.id)
                .filter(|id| !c.decor.contains(id))
                .collect()
        })
        .unwrap_or_default();
    let mut index = 0;
    if full_bleed {
        for (k, pic) in c.pictures.iter().enumerate() {
            if let Some(from) = order.iter().position(|id| id == pic)
                && from != k
            {
                ops.push(Op::MoveShape {
                    parent: c.slide,
                    from,
                    to: k,
                });
                let id = order.remove(from);
                order.insert(k, id);
            }
        }
        index = order
            .iter()
            .position(|id| Some(*id) == c.title || c.texts.contains(id))
            .unwrap_or(order.len());
    }
    if !decor.is_empty() {
        ops.push(Op::InsertShapes {
            parent: c.slide,
            index,
            shapes: decor,
        });
    }
    Ok(Transaction::new(ops).with_kind(EditKind::Other))
}

#[allow(clippy::cast_precision_loss)]
fn f64_of(v: i64) -> f64 {
    v as f64
}

impl DeckDoc {
    /// The Design Ideas pane's suggestions for the current slide, each drawn `width` CSS px
    /// wide at `dpr`.
    pub(crate) fn design_ideas(&self, width: f32, dpr: f32) -> Result<Vec<DeckDesignIdea>> {
        let Some(slide) = self.current_slide() else {
            return Ok(Vec::new());
        };
        let Some(c) = content(&self.pres, slide) else {
            return Ok(Vec::new());
        };
        let mut renderer = self
            .renderer
            .try_borrow_mut()
            .map_err(|_| KilnError::Internal("the renderer is in use".into()))?;
        let slide_w = kiln_deck_layout::geom::px(self.pres.slide_size.cx).max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let px = (width.clamp(48.0, 640.0) * dpr.clamp(0.5, 4.0)).round() as u32;
        let mut out = Vec::new();
        for (id, label, p) in ideas_for(&c) {
            let mut pres = self.pres.clone();
            let tx = build(&mut pres, &c, &p)?;
            pres.apply(tx)
                .map_err(|e| KilnError::Internal(e.to_string()))?;
            #[allow(clippy::cast_precision_loss)]
            let scale = px as f32 / slide_w;
            // Pictures are decoded once for every idea; the copy's own parts are the same.
            let file = super::export::render_png(&mut renderer, &pres, slide, scale)?;
            out.push(DeckDesignIdea {
                id: id.into(),
                label: label.into(),
                png: file,
            });
        }
        Ok(out)
    }

    /// Applies an idea to the current slide.
    pub(crate) fn apply_design_idea(&mut self, id: &str) -> Result<Change> {
        let slide = self.slide_id()?;
        let c = content(&self.pres, slide)
            .ok_or_else(|| KilnError::InvalidInput("the slide is gone".into()))?;
        let Some(p) = plan(id, &c) else {
            return Err(KilnError::InvalidInput(format!(
                "the design idea {id} does not fit this slide"
            )));
        };
        self.end_text();
        self.apply_with("deck.designer.apply", |pres| {
            build(pres, &c, &p).map_err(|e| kiln_deck::DeckError::Corrupt(e.to_string()))
        })?;
        self.view.shapes.clear();
        Ok(Change::Content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_fill_the_box() {
        let c = fill_crop((1600, 900), 100, 100);
        assert!(c.l > 20_000 && c.l == c.r && c.t == 0);
        let c = fill_crop((900, 1600), 160, 90);
        assert!(c.t > 30_000 && c.t == c.b && c.l == 0);
    }

    #[test]
    fn every_slide_gets_four_to_six_ideas() {
        let text = Content {
            w: 12_192_000,
            h: 6_858_000,
            title: Some(NodeId(1)),
            texts: vec![NodeId(2)],
            paragraphs: 7,
            ..Content::default()
        };
        let pictured = Content {
            pictures: vec![NodeId(3)],
            paragraphs: 2,
            ..text.clone()
        };
        let tabled = Content {
            visuals: vec![NodeId(4)],
            paragraphs: 3,
            ..text.clone()
        };
        let title_only = Content {
            texts: Vec::new(),
            paragraphs: 0,
            title_slide: true,
            ..text.clone()
        };
        for c in [&text, &pictured, &tabled, &title_only] {
            let ideas = ideas_for(c);
            assert!((4..=6).contains(&ideas.len()), "{c:?}: {}", ideas.len());
        }
        let ids = |c: &Content| ideas_for(c).iter().map(|i| i.0).collect::<Vec<_>>();
        assert!(ids(&pictured).starts_with(&["pictureRight", "pictureLeft"]));
        assert!(ids(&text).contains(&"sideTitle"));
        assert!(!ids(&tabled).iter().any(|i| i.starts_with("picture")));
    }
}
