// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML fills, outlines and effects resolved against the slide's theme and drawn as Scene
//! ops: solid, gradient (linear and path), pattern and picture fills; outlines with width, dash
//! presets, caps, joins and arrowheads; outer shadows and glows.
//!
//! A shape's own `spPr` value wins; otherwise its Quick Style (`p:style`) refers into the theme's
//! format scheme (fill, line and effect style lists) with the reference's colour standing in for
//! `phClr`, as Office does.

use kiln_deck::{ShapeProps, ShapeStyle};
use kiln_ooxml::drawingml::color::{ColorContext, DrawingColor, Rgba, resolve_color};
use kiln_ooxml::drawingml::effects::EffectList;
use kiln_ooxml::drawingml::fill::{
    BlipFill, BlipFillMode, Fill, GradientFill, PatternFill, TileInfo,
};
use kiln_ooxml::drawingml::line::{Line, LineDash, LineEnd, LineJoin as DmlJoin};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_ooxml::st::{
    LineCap as DmlCap, LineEndSize, LineEndType, PathShadeType, PresetPatternVal,
};
use kiln_scene::{
    Bounds, DrawOp, FillRule, Gradient, GradientKind, GradientStop, LineCap, LineJoin, Paint,
    PathData, Stroke,
};

use crate::geom::px;

/// `0xRRGGBBAA` of a resolved DrawingML colour.
#[must_use]
pub fn rgba(c: Rgba) -> u32 {
    let c = c.to_color();
    u32::from_be_bytes([c.r, c.g, c.b, c.a])
}

/// Resolves a colour to `0xRRGGBBAA`.
#[must_use]
pub fn color(c: &DrawingColor, ctx: &ColorContext) -> u32 {
    rgba(resolve_color(c, ctx))
}

/// Multiplies a colour's alpha.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn with_alpha(c: u32, alpha: f32) -> u32 {
    let a = (c & 0xff) as f32 * alpha.clamp(0.0, 1.0);
    (c & 0xffff_ff00) | (a.round() as u32).min(255)
}

/// A solid or gradient fill made `alpha` times as opaque (soft edges, reflections); `None` for
/// fills that cannot fade.
#[must_use]
pub fn faded(fill: &Paintable, alpha: f32) -> Option<Paintable> {
    match fill {
        Paintable::Solid(c) => Some(Paintable::Solid(with_alpha(*c, alpha))),
        Paintable::Gradient {
            stops,
            angle,
            focus,
        } => Some(Paintable::Gradient {
            stops: stops
                .iter()
                .map(|(o, c)| (*o, with_alpha(*c, alpha)))
                .collect(),
            angle: *angle,
            focus: *focus,
        }),
        _ => None,
    }
}

/// Mixes `amount` of `b` into `a` (alpha kept from `a`).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let t = amount.clamp(0.0, 1.0);
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let m = |s: u32| ((ch(a, s) * (1.0 - t) + ch(b, s) * t).round() as u32).min(255) << s;
    m(24) | m(16) | m(8) | (a & 0xff)
}

/// What a shape resolves against: the theme and the colour context of its slide.
#[derive(Debug, Clone, Copy)]
pub struct Look<'a> {
    /// The theme of the slide's master.
    pub theme: &'a Theme,
    /// Theme colours and the colour map in force.
    pub ctx: ColorContext,
}

impl Look<'_> {
    /// The context with a placeholder colour (`phClr`) for theme style lists.
    #[must_use]
    pub fn with_ph(&self, ph: Option<&DrawingColor>) -> ColorContext {
        let mut ctx = self.ctx;
        ctx.ph_clr = ph.map(|c| resolve_color(c, &self.ctx));
        ctx
    }
}

/// A fill ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Paintable {
    /// Nothing.
    None,
    /// One colour.
    Solid(u32),
    /// A gradient: stops (offset 0–1, colour), linear angle in degrees (clockwise from the x
    /// axis) or a path gradient's focus rectangle (fractions of the box).
    Gradient {
        /// Stops.
        stops: Vec<(f32, u32)>,
        /// Linear angle, `None` for a path (radial) gradient.
        angle: Option<f32>,
        /// Focus of a path gradient, as `(l, t, r, b)` insets.
        focus: (f32, f32, f32, f32),
    },
    /// A preset pattern of two colours.
    Pattern {
        /// The preset.
        preset: PresetPatternVal,
        /// Foreground (the lines).
        fg: u32,
        /// Background.
        bg: u32,
    },
    /// A picture (part name), stretched or tiled.
    Picture {
        /// The image part.
        part: String,
        /// Source rectangle insets (fractions, `l, t, r, b`).
        crop: (f32, f32, f32, f32),
        /// Tiled instead of stretched.
        tile: Option<Tiling>,
        /// Opacity.
        alpha: f32,
    },
}

/// How a tiled picture fill repeats (`a:tile`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tiling {
    /// Offset of the tiles, px.
    pub offset: (f32, f32),
    /// Size of a tile as a multiple of the picture's own size.
    pub scale: (f32, f32),
    /// Where the first tile sits in the filled box (0 left or top, 0.5 centre, 1 right or bottom).
    pub align: (f32, f32),
}

impl Tiling {
    /// The tiling of `a:tile` (PowerPoint's defaults when it is absent).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn of(tile: Option<&TileInfo>) -> Self {
        use kiln_ooxml::st::RectAlignment as A;
        let Some(t) = tile else {
            return Self {
                offset: (0.0, 0.0),
                scale: (1.0, 1.0),
                align: (0.0, 0.0),
            };
        };
        let align = match t.algn {
            A::Tl => (0.0, 0.0),
            A::T => (0.5, 0.0),
            A::Tr => (1.0, 0.0),
            A::L => (0.0, 0.5),
            A::Ctr => (0.5, 0.5),
            A::R => (1.0, 0.5),
            A::Bl => (0.0, 1.0),
            A::B => (0.5, 1.0),
            A::Br => (1.0, 1.0),
        };
        Self {
            offset: (px(t.tx), px(t.ty)),
            scale: (t.sx as f32, t.sy as f32),
            align,
        }
    }
}

/// An outline ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// Colour.
    pub paint: u32,
    /// Width in px.
    pub width: f32,
    /// Dash lengths in px (empty: solid).
    pub dash: Vec<f32>,
    /// Cap.
    pub cap: LineCap,
    /// Join.
    pub join: LineJoin,
    /// Arrowhead at the start.
    pub head: Option<LineEnd>,
    /// Arrowhead at the end.
    pub tail: Option<LineEnd>,
}

/// Effects ready to draw.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Effects {
    /// Outer shadow: offset, blur radius and colour.
    pub shadow: Option<(f32, f32, f32, u32)>,
    /// Glow: radius and colour.
    pub glow: Option<(f32, u32)>,
    /// Soft edges radius.
    pub soft_edge: Option<f32>,
    /// Reflection: start and end opacity, how much of the shape it shows (0–1), gap below.
    pub reflection: Option<(f32, f32, f32, f32)>,
}

/// A theme style list entry (`idx` is 1-based; 0 means none; 1001+ are background styles).
fn style_entry<T: Clone>(list: &[T], idx: u32) -> Option<T> {
    if idx == 0 {
        return None;
    }
    let i = usize::try_from(idx.saturating_sub(1)).ok()?;
    list.get(i).or_else(|| list.last()).cloned()
}

/// Resolves a fill: the explicit one, else the Quick Style's theme fill.
#[must_use]
pub fn resolve_fill(fill: Option<&Fill>, style: Option<&ShapeStyle>, look: &Look<'_>) -> Paintable {
    if let Some(f) = fill {
        return paintable(f, &look.ctx);
    }
    let Some(st) = style else {
        return Paintable::None;
    };
    let fs = &look.theme.format_scheme;
    let entry = if st.fill.idx >= 1000 {
        style_entry(&fs.bg_fill_styles, st.fill.idx - 1000)
    } else {
        style_entry(&fs.fill_styles, st.fill.idx)
    };
    entry.map_or(Paintable::None, |f| {
        paintable(&f, &look.with_ph(st.fill.color.as_ref()))
    })
}

/// A background style (`p:bgRef idx`, 1001+), with its colour as `phClr`.
#[must_use]
pub fn background_style(idx: u32, color: Option<&DrawingColor>, look: &Look<'_>) -> Paintable {
    let fs = &look.theme.format_scheme;
    let entry = if idx >= 1000 {
        style_entry(&fs.bg_fill_styles, idx - 1000)
    } else {
        style_entry(&fs.fill_styles, idx)
    };
    entry.map_or_else(
        || {
            color.map_or(Paintable::None, |c| {
                Paintable::Solid(self::color(c, &look.ctx))
            })
        },
        |f| paintable(&f, &look.with_ph(color)),
    )
}

/// A DrawingML fill as a paintable value.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn paintable(f: &Fill, ctx: &ColorContext) -> Paintable {
    match f {
        Fill::None | Fill::Group => Paintable::None,
        Fill::Solid(c) => Paintable::Solid(color(c, ctx)),
        Fill::Gradient(g) => gradient(g, ctx),
        Fill::Pattern(PatternFill { preset, fg, bg }) => Paintable::Pattern {
            preset: *preset,
            fg: fg.as_ref().map_or(0x0000_00ff, |c| color(c, ctx)),
            bg: bg.as_ref().map_or(0xffff_ffff, |c| color(c, ctx)),
        },
        Fill::Blip(b) => picture(b),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn gradient(g: &GradientFill, ctx: &ColorContext) -> Paintable {
    let mut stops: Vec<(f32, u32)> = g
        .stops
        .iter()
        .map(|s| ((s.pos as f32).clamp(0.0, 1.0), color(&s.color, ctx)))
        .collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    if stops.is_empty() {
        return Paintable::None;
    }
    if stops.len() == 1 {
        return Paintable::Solid(stops[0].1);
    }
    let (angle, focus) = match (&g.linear, &g.path) {
        (_, Some(p)) if p.shade != PathShadeType::Rect || p.fill_to_rect.is_some() => {
            let r = p.fill_to_rect.unwrap_or_default();
            (None, (r.l as f32, r.t as f32, r.r as f32, r.b as f32))
        }
        (Some(l), _) => (Some(l.angle as f32), (0.0, 0.0, 0.0, 0.0)),
        _ => (Some(90.0), (0.0, 0.0, 0.0, 0.0)),
    };
    Paintable::Gradient {
        stops,
        angle,
        focus,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn picture(b: &BlipFill) -> Paintable {
    let Some(part) = b.embed.clone().or_else(|| b.link.clone()) else {
        return Paintable::None;
    };
    let crop = b.src_rect.map_or((0.0, 0.0, 0.0, 0.0), |r| {
        (r.l as f32, r.t as f32, r.r as f32, r.b as f32)
    });
    Paintable::Picture {
        part,
        crop,
        tile: (b.mode == BlipFillMode::Tile).then(|| Tiling::of(b.tile.as_ref())),
        alpha: b.alpha.map_or(1.0, |a| a as f32).clamp(0.0, 1.0),
    }
}

/// Resolves an outline: the explicit `a:ln` over the Quick Style's theme line.
#[must_use]
pub fn resolve_line(
    line: Option<&Line>,
    style: Option<&ShapeStyle>,
    look: &Look<'_>,
) -> Option<Outline> {
    let base = style.and_then(|st| {
        style_entry(&look.theme.format_scheme.line_styles, st.line.idx)
            .map(|l| (l, look.with_ph(st.line.color.as_ref())))
    });
    let own = line;
    // Properties the shape sets win; the rest come from the style.
    let fill = own
        .and_then(|l| l.fill.as_ref().map(|f| (f.clone(), look.ctx)))
        .or_else(|| {
            base.as_ref()
                .and_then(|(l, ctx)| l.fill.as_ref().map(|f| (f.clone(), *ctx)))
        });
    let (fill, ctx) = fill?;
    let paint = match &fill {
        Fill::None => return None,
        Fill::Solid(c) => color(c, &ctx),
        Fill::Gradient(g) => g
            .stops
            .first()
            .map_or(0x0000_00ff, |s| color(&s.color, &ctx)),
        Fill::Pattern(p) => p.fg.as_ref().map_or(0x0000_00ff, |c| color(c, &ctx)),
        Fill::Blip(_) | Fill::Group => 0x0000_00ff,
    };
    let pick = |f: &dyn Fn(&Line) -> bool| own.filter(|l| f(l)).or(base.as_ref().map(|(l, _)| l));
    let width_emu = own
        .and_then(|l| l.width)
        .or_else(|| base.as_ref().and_then(|(l, _)| l.width))
        .unwrap_or(9525);
    let width = px(width_emu).max(0.5);
    let dash_src = pick(&|l| l.dash.is_some()).and_then(|l| l.dash.clone());
    let cap = pick(&|l| l.cap.is_some())
        .and_then(|l| l.cap)
        .map_or(LineCap::Butt, |c| match c {
            DmlCap::Rnd => LineCap::Round,
            DmlCap::Sq => LineCap::Square,
            DmlCap::Flat => LineCap::Butt,
        });
    let join =
        pick(&|l| l.join.is_some())
            .and_then(|l| l.join)
            .map_or(LineJoin::Round, |j| match j {
                DmlJoin::Round => LineJoin::Round,
                DmlJoin::Bevel => LineJoin::Bevel,
                DmlJoin::Miter(_) => LineJoin::Miter,
            });
    let dash = dash_lengths(dash_src.as_ref(), width, cap);
    let head = own
        .and_then(|l| l.head)
        .or_else(|| base.as_ref().and_then(|(l, _)| l.head))
        .filter(|e| e.end_type != LineEndType::None);
    let tail = own
        .and_then(|l| l.tail)
        .or_else(|| base.as_ref().and_then(|(l, _)| l.tail))
        .filter(|e| e.end_type != LineEndType::None);
    Some(Outline {
        paint,
        width,
        dash,
        cap,
        join,
        head,
        tail,
    })
}

/// Dash lengths in px for a dash preset (Office's patterns, in multiples of the width).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn dash_lengths(d: Option<&LineDash>, width: f32, cap: LineCap) -> Vec<f32> {
    use kiln_ooxml::st::PresetLineDashVal as P;
    let pattern: Vec<f32> = match d {
        None | Some(LineDash::Preset(P::Solid)) => return Vec::new(),
        Some(LineDash::Preset(p)) => match p {
            P::Dot => vec![1.0, 3.0],
            P::Dash => vec![4.0, 3.0],
            P::LgDash => vec![8.0, 3.0],
            P::DashDot => vec![4.0, 3.0, 1.0, 3.0],
            P::LgDashDot => vec![8.0, 3.0, 1.0, 3.0],
            P::LgDashDotDot => vec![8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            P::SysDash => vec![3.0, 1.0],
            P::SysDot => vec![1.0, 1.0],
            P::SysDashDot => vec![3.0, 1.0, 1.0, 1.0],
            P::SysDashDotDot => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            P::Solid => return Vec::new(),
        },
        Some(LineDash::Custom(ds)) => ds
            .iter()
            .flat_map(|(d, s)| [(*d / 100.0) as f32, (*s / 100.0) as f32])
            .collect(),
    };
    // Round and square caps grow every dash by the width: shorten the dashes to keep the look.
    let grow = if cap == LineCap::Butt { 0.0 } else { width };
    pattern
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let l = m * width;
            if i % 2 == 0 {
                (l - grow).max(0.01)
            } else {
                l + grow
            }
        })
        .collect()
}

/// Resolves effects: the explicit list over the Quick Style's theme effect style.
#[must_use]
pub fn resolve_effects(
    effects: Option<&EffectList>,
    style: Option<&ShapeStyle>,
    look: &Look<'_>,
) -> Effects {
    let (list, ctx) = match effects {
        Some(e) => (Some(e.clone()), look.ctx),
        None => match style {
            Some(st) => (
                style_entry(&look.theme.format_scheme.effect_styles, st.effect.idx)
                    .and_then(|e| e.effects),
                look.with_ph(st.effect.color.as_ref()),
            ),
            None => (None, look.ctx),
        },
    };
    let Some(list) = list else {
        return Effects::default();
    };
    #[allow(clippy::cast_possible_truncation)]
    let shadow = list.outer_shadow.as_ref().map(|s| {
        let d = px(s.dist);
        let dir = s.dir.to_radians();
        (
            d * dir.cos() as f32,
            d * dir.sin() as f32,
            px(s.blur_rad),
            color(&s.color, &ctx),
        )
    });
    let glow = list
        .glow
        .as_ref()
        .map(|g| (px(g.rad), color(&g.color, &ctx)));
    #[allow(clippy::cast_possible_truncation)]
    let reflection = list.reflection.as_ref().map(|r| {
        (
            r.st_a.clamp(0.0, 1.0) as f32,
            r.end_a.clamp(0.0, 1.0) as f32,
            r.end_pos.clamp(0.05, 1.0) as f32,
            px(r.dist),
        )
    });
    Effects {
        shadow,
        glow,
        soft_edge: list.soft_edge.map(px).filter(|r| *r > 0.5),
        reflection,
    }
}

/// Everything a shape's geometry needs to be drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeLook {
    /// Fill.
    pub fill: Paintable,
    /// Outline.
    pub line: Option<Outline>,
    /// Effects.
    pub effects: Effects,
}

/// Resolves the look of shape properties.
#[must_use]
pub fn shape_look(props: &ShapeProps, look: &Look<'_>) -> ShapeLook {
    ShapeLook {
        fill: resolve_fill(props.fill.as_ref(), props.style.as_ref(), look),
        line: resolve_line(props.line.as_ref(), props.style.as_ref(), look),
        effects: resolve_effects(props.effects.as_ref(), props.style.as_ref(), look),
    }
}

/// Where pictures come from while drawing.
pub trait PictureSource {
    /// The image id and pixel size of a part, decoding it on first use.
    fn picture(&mut self, part: &str) -> Option<(kiln_scene::ImageId, u32, u32)>;
    /// A part's own size in px at 96 dpi (from the resolution its file records), when known.
    fn natural_size(&mut self, _part: &str) -> Option<(f32, f32)> {
        None
    }
    /// The image id and pixel size of a pattern fill's tile, when the source makes them.
    fn pattern(
        &mut self,
        _preset: PresetPatternVal,
        _fg: u32,
        _bg: u32,
    ) -> Option<(kiln_scene::ImageId, u32, u32)> {
        None
    }
}

/// A source with no pictures (thumbnails of layouts in tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPictures;

impl PictureSource for NoPictures {
    fn picture(&mut self, _part: &str) -> Option<(kiln_scene::ImageId, u32, u32)> {
        None
    }
}

/// Fills `path` (whose box is `bounds`) with a paintable.
pub fn fill_path(
    out: &mut Vec<DrawOp>,
    path: &PathData,
    bounds: Bounds,
    fill: &Paintable,
    pictures: &mut dyn PictureSource,
) {
    match fill {
        Paintable::None => {}
        Paintable::Solid(c) => {
            if c & 0xff != 0 {
                out.push(DrawOp::Path {
                    path: path.clone(),
                    fill: Some(Paint::Rgba(*c)),
                    stroke: None,
                    rule: FillRule::NonZero,
                });
            }
        }
        Paintable::Gradient {
            stops,
            angle,
            focus,
        } => {
            let kind = match angle {
                Some(a) => {
                    let t = f64::from(*a).to_radians();
                    #[allow(clippy::cast_possible_truncation)]
                    let (s, c) = (t.sin() as f32, t.cos() as f32);
                    let (cx, cy) = (bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0);
                    let half = (bounds.w * c.abs() + bounds.h * s.abs()) / 2.0;
                    GradientKind::Linear {
                        x0: cx - c * half,
                        y0: cy - s * half,
                        x1: cx + c * half,
                        y1: cy + s * half,
                    }
                }
                None => {
                    let (l, t, r, b) = *focus;
                    let fx = bounds.x + bounds.w * (l + (1.0 - r)) / 2.0;
                    let fy = bounds.y + bounds.h * (t + (1.0 - b)) / 2.0;
                    let far = [
                        (bounds.x, bounds.y),
                        (bounds.right(), bounds.y),
                        (bounds.x, bounds.bottom()),
                        (bounds.right(), bounds.bottom()),
                    ]
                    .iter()
                    .map(|(x, y)| ((x - fx).powi(2) + (y - fy).powi(2)).sqrt())
                    .fold(1.0f32, f32::max);
                    GradientKind::Radial {
                        cx: fx,
                        cy: fy,
                        r: far,
                    }
                }
            };
            // DrawingML path gradients run from the focus (first stop) outwards; linear ones
            // from the start edge.
            let stops = stops
                .iter()
                .map(|(o, c)| GradientStop {
                    offset: *o,
                    paint: Paint::Rgba(*c),
                })
                .collect();
            out.push(DrawOp::PushClip {
                rect: path.bounds(),
                path: Some(path.clone()),
            });
            out.push(DrawOp::Gradient {
                rect: bounds.inflate(1.0, 1.0),
                radius: 0.0,
                gradient: Gradient { kind, stops },
            });
            out.push(DrawOp::PopClip);
        }
        Paintable::Pattern { preset, fg, bg } => {
            out.push(DrawOp::Path {
                path: path.clone(),
                fill: Some(Paint::Rgba(*bg)),
                stroke: None,
                rule: FillRule::NonZero,
            });
            out.push(DrawOp::PushClip {
                rect: path.bounds(),
                path: Some(path.clone()),
            });
            if let Some((image, w, h)) = pictures.pattern(*preset, *fg, *bg) {
                // The 8 × 8 bitmap, one bit per slide pixel, tiled from the shape's corner.
                #[allow(clippy::cast_precision_loss)]
                let (tw, th) = (w as f32, h as f32);
                let mut y = bounds.y;
                while y < bounds.bottom() {
                    let mut x = bounds.x;
                    while x < bounds.right() {
                        out.push(DrawOp::Image {
                            image,
                            src: Bounds::new(0.0, 0.0, tw, th),
                            dst: Bounds::new(x, y, tw, th),
                            opacity: 1.0,
                        });
                        x += tw;
                    }
                    y += th;
                }
            } else {
                pattern(out, bounds, *preset, *fg, *bg);
            }
            out.push(DrawOp::PopClip);
        }
        Paintable::Picture {
            part,
            crop,
            tile,
            alpha,
        } => {
            let Some((image, w, h)) = pictures.picture(part) else {
                out.push(DrawOp::Path {
                    path: path.clone(),
                    fill: Some(Paint::Rgba(0xd9d9_d9ff)),
                    stroke: None,
                    rule: FillRule::NonZero,
                });
                return;
            };
            #[allow(clippy::cast_precision_loss)]
            let (iw, ih) = (w as f32, h as f32);
            out.push(DrawOp::PushClip {
                rect: path.bounds(),
                path: Some(path.clone()),
            });
            if let Some(t) = tile {
                // Tiles of the picture's own size (its pixels at the resolution the file
                // records) times the scale, from the aligned tile outwards, as PowerPoint draws.
                let (nw, nh) = pictures.natural_size(part).unwrap_or((iw, ih));
                let tw = (nw * t.scale.0).abs().max(4.0);
                let th = (nh * t.scale.1).abs().max(4.0);
                let ox = bounds.x + (bounds.w - tw) * t.align.0 + t.offset.0;
                let oy = bounds.y + (bounds.h - th) * t.align.1 + t.offset.1;
                let x0 = ox - ((ox - bounds.x) / tw).ceil() * tw;
                let mut y = oy - ((oy - bounds.y) / th).ceil() * th;
                let mut n = 0;
                while y < bounds.bottom() && n < 4096 {
                    let mut x = x0;
                    while x < bounds.right() && n < 4096 {
                        out.push(DrawOp::Image {
                            image,
                            src: Bounds::new(0.0, 0.0, iw, ih),
                            dst: Bounds::new(x, y, tw, th),
                            opacity: *alpha,
                        });
                        x += tw;
                        n += 1;
                    }
                    y += th;
                }
            } else {
                let (l, t, r, b) = *crop;
                let src = Bounds::new(
                    iw * l,
                    ih * t,
                    (iw * (1.0 - l - r)).max(1.0),
                    (ih * (1.0 - t - b)).max(1.0),
                );
                out.push(DrawOp::Image {
                    image,
                    src,
                    dst: bounds,
                    opacity: *alpha,
                });
            }
            out.push(DrawOp::PopClip);
        }
    }
}

/// Draws a pattern's foreground inside `b` (clip already pushed).
#[allow(clippy::cast_precision_loss)]
fn pattern(out: &mut Vec<DrawOp>, b: Bounds, preset: PresetPatternVal, fg: u32, bg: u32) {
    use PresetPatternVal as P;
    let mut lines = PathData::new();
    let step = 8.0f32;
    let mut width = 1.0f32;
    let horiz = |lines: &mut PathData, step: f32| {
        let mut y = b.y + step / 2.0;
        while y < b.bottom() {
            lines.move_to(b.x, y).line_to(b.right(), y);
            y += step;
        }
    };
    let vert = |lines: &mut PathData, step: f32| {
        let mut x = b.x + step / 2.0;
        while x < b.right() {
            lines.move_to(x, b.y).line_to(x, b.bottom());
            x += step;
        }
    };
    let diag = |lines: &mut PathData, step: f32, up: bool| {
        let span = b.w + b.h;
        let mut d = 0.0;
        while d < span {
            if up {
                lines
                    .move_to(b.x + d - b.h, b.bottom())
                    .line_to(b.x + d, b.y);
            } else {
                lines
                    .move_to(b.x + d - b.h, b.y)
                    .line_to(b.x + d, b.bottom());
            }
            d += step;
        }
    };
    let density = match preset {
        P::Horz | P::LtHorz | P::NarHorz | P::DashHorz => {
            horiz(&mut lines, if preset == P::NarHorz { 4.0 } else { step });
            None
        }
        P::DkHorz => {
            width = 3.0;
            horiz(&mut lines, step);
            None
        }
        P::Vert | P::LtVert | P::NarVert | P::DashVert => {
            vert(&mut lines, if preset == P::NarVert { 4.0 } else { step });
            None
        }
        P::DkVert => {
            width = 3.0;
            vert(&mut lines, step);
            None
        }
        P::Cross | P::SmGrid | P::LgGrid | P::DotGrid => {
            let s = match preset {
                P::SmGrid => 4.0,
                P::LgGrid => 12.0,
                _ => step,
            };
            horiz(&mut lines, s);
            vert(&mut lines, s);
            None
        }
        P::DnDiag | P::LtDnDiag | P::DashDnDiag => {
            diag(&mut lines, step, false);
            None
        }
        P::UpDiag | P::LtUpDiag | P::DashUpDiag => {
            diag(&mut lines, step, true);
            None
        }
        P::DkDnDiag | P::WdDnDiag => {
            width = 3.0;
            diag(&mut lines, step, false);
            None
        }
        P::DkUpDiag | P::WdUpDiag => {
            width = 3.0;
            diag(&mut lines, step, true);
            None
        }
        P::DiagCross | P::Trellis | P::OpenDmnd | P::DiagBrick => {
            diag(&mut lines, step, false);
            diag(&mut lines, step, true);
            None
        }
        P::Pct5 => Some(0.05),
        P::Pct10 => Some(0.10),
        P::Pct20 => Some(0.20),
        P::Pct25 => Some(0.25),
        P::Pct30 => Some(0.30),
        P::Pct40 => Some(0.40),
        P::Pct50 => Some(0.50),
        P::Pct60 => Some(0.60),
        P::Pct70 => Some(0.70),
        P::Pct75 => Some(0.75),
        P::Pct80 => Some(0.80),
        P::Pct90 => Some(0.90),
        _ => Some(0.35),
    };
    if let Some(d) = density {
        out.push(DrawOp::FillRect {
            rect: b,
            paint: Paint::Rgba(mix(bg, fg, d)),
            radius: 0.0,
        });
        return;
    }
    if !lines.is_empty() {
        out.push(DrawOp::Path {
            path: lines,
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Rgba(fg),
                width,
                join: LineJoin::Miter,
                cap: LineCap::Butt,
                dash: Vec::new(),
            }),
            rule: FillRule::NonZero,
        });
    }
}

/// Strokes `path` with an outline and draws its arrowheads (open paths only).
pub fn stroke_path(out: &mut Vec<DrawOp>, path: &PathData, line: &Outline, closed: bool) {
    out.push(DrawOp::Path {
        path: path.clone(),
        fill: None,
        stroke: Some(Stroke {
            paint: Paint::Rgba(line.paint),
            width: line.width,
            join: line.join,
            cap: line.cap,
            dash: line.dash.clone(),
        }),
        rule: FillRule::NonZero,
    });
    if closed || (line.head.is_none() && line.tail.is_none()) {
        return;
    }
    let Some((start, out_start, end, out_end)) = crate::geom::path_ends(path) else {
        return;
    };
    if let Some(h) = &line.head {
        arrowhead(out, start, out_start, h, line);
    }
    if let Some(t) = &line.tail {
        arrowhead(out, end, out_end, t, line);
    }
}

fn end_scale(s: Option<LineEndSize>) -> f32 {
    match s {
        Some(LineEndSize::Sm) => 2.0,
        Some(LineEndSize::Lg) => 5.0,
        _ => 3.0,
    }
}

/// The line width arrowheads of thinner lines are sized by: 0.7 mm (about 2 pt). PowerPoint
/// draws the heads of a hairline as large as a 2 pt line's, so they read at any width.
const ARROW_BASE_EMU: i64 = 25_200;

/// An arrowhead at `tip`, pointing along `dir`.
fn arrowhead(
    out: &mut Vec<DrawOp>,
    tip: (f32, f32),
    dir: (f32, f32),
    end: &LineEnd,
    line: &Outline,
) {
    let w = line.width.max(px(ARROW_BASE_EMU));
    let half = end_scale(end.w) * w / 2.0;
    let len = end_scale(end.len) * w;
    let (dx, dy) = dir;
    let (nx, ny) = (-dy, dx);
    let back = (tip.0 - dx * len, tip.1 - dy * len);
    let left = (back.0 + nx * half, back.1 + ny * half);
    let right = (back.0 - nx * half, back.1 - ny * half);
    let paint = Paint::Rgba(line.paint);
    let mut p = PathData::new();
    match end.end_type {
        LineEndType::None => return,
        LineEndType::Triangle => {
            p.move_to(tip.0, tip.1)
                .line_to(left.0, left.1)
                .line_to(right.0, right.1)
                .close();
        }
        LineEndType::Stealth => {
            let notch = (tip.0 - dx * len * 0.6, tip.1 - dy * len * 0.6);
            p.move_to(tip.0, tip.1)
                .line_to(left.0, left.1)
                .line_to(notch.0, notch.1)
                .line_to(right.0, right.1)
                .close();
        }
        LineEndType::Diamond => {
            let mid = (tip.0 - dx * len / 2.0, tip.1 - dy * len / 2.0);
            p.move_to(tip.0 + dx * len / 2.0, tip.1 + dy * len / 2.0)
                .line_to(mid.0 + nx * half, mid.1 + ny * half)
                .line_to(tip.0 - dx * len / 2.0, tip.1 - dy * len / 2.0)
                .line_to(mid.0 - nx * half, mid.1 - ny * half)
                .close();
            let _ = mid;
        }
        LineEndType::Oval => {
            let (rx, ry) = (len / 2.0, half);
            let c = (tip.0, tip.1);
            // An ellipse along the line's direction, as four cubic arcs.
            let k = kiln_scene::shapes::KAPPA;
            let pt = |u: f32, v: f32| (c.0 + dx * u + nx * v, c.1 + dy * u + ny * v);
            let a = pt(rx, 0.0);
            p.move_to(a.0, a.1);
            let quads = [
                ((rx, ry * k), (rx * k, ry), (0.0, ry)),
                ((-rx * k, ry), (-rx, ry * k), (-rx, 0.0)),
                ((-rx, -ry * k), (-rx * k, -ry), (0.0, -ry)),
                ((rx * k, -ry), (rx, -ry * k), (rx, 0.0)),
            ];
            for (c1, c2, e) in quads {
                let (c1, c2, e) = (pt(c1.0, c1.1), pt(c2.0, c2.1), pt(e.0, e.1));
                p.cubic_to(c1.0, c1.1, c2.0, c2.1, e.0, e.1);
            }
            p.close();
        }
        LineEndType::Arrow => {
            p.move_to(left.0, left.1)
                .line_to(tip.0, tip.1)
                .line_to(right.0, right.1);
            out.push(DrawOp::Path {
                path: p,
                fill: None,
                stroke: Some(Stroke {
                    paint,
                    width: line.width,
                    join: LineJoin::Miter,
                    cap: LineCap::Round,
                    dash: Vec::new(),
                }),
                rule: FillRule::NonZero,
            });
            return;
        }
    }
    out.push(DrawOp::Path {
        path: p,
        fill: Some(paint),
        stroke: None,
        rule: FillRule::NonZero,
    });
}

/// Draws an outer shadow and a glow behind `path` (the shape's outline in scene px). Rectangles
/// use the Scene's blurred box shadow; other outlines get a few translucent layers.
pub fn effects_behind(
    out: &mut Vec<DrawOp>,
    path: &PathData,
    rect: Option<(Bounds, f32)>,
    effects: &Effects,
    filled: bool,
) {
    if let Some((r, c)) = effects.glow {
        // Five nested strokes: their overlap is about 70 % of the glow colour at the edge,
        // fading to nothing at the radius, as Office draws it.
        for (k, a) in [
            (1.0f32, 0.3f32),
            (0.8, 0.38),
            (0.6, 0.46),
            (0.4, 0.54),
            (0.2, 0.62),
        ] {
            out.push(DrawOp::Path {
                path: path.clone(),
                fill: None,
                stroke: Some(Stroke {
                    paint: Paint::Rgba(with_alpha(c, a)),
                    width: r * 2.0 * k,
                    join: LineJoin::Round,
                    cap: LineCap::Round,
                    dash: Vec::new(),
                }),
                rule: FillRule::NonZero,
            });
        }
    }
    let Some((dx, dy, blur, c)) = effects.shadow else {
        return;
    };
    let _ = rect;
    let mut moved = path.clone();
    moved.translate(dx, dy);
    let alpha = f32::from(u8::try_from(c & 0xff).unwrap_or(0)) / 255.0;
    if blur > 0.5 {
        // A blur approximated by nested translucent strokes: their overlap builds the falloff
        // from the edge outwards (identical in every painter).
        let steps = 6u8;
        for k in (1..=steps).rev() {
            let w = blur * 2.0 * f32::from(k) / f32::from(steps);
            out.push(DrawOp::Path {
                path: moved.clone(),
                fill: None,
                stroke: Some(Stroke {
                    paint: Paint::Rgba(with_alpha(c | 0xff, alpha / f32::from(steps) * 1.1)),
                    width: w,
                    join: LineJoin::Round,
                    cap: LineCap::Round,
                    dash: Vec::new(),
                }),
                rule: FillRule::NonZero,
            });
        }
    }
    if filled {
        out.push(DrawOp::Path {
            path: moved,
            fill: Some(Paint::Rgba(with_alpha(
                c | 0xff,
                if blur > 0.5 { alpha * 0.55 } else { alpha },
            ))),
            stroke: None,
            rule: FillRule::NonZero,
        });
    } else {
        out.push(DrawOp::Path {
            path: moved,
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Rgba(with_alpha(c | 0xff, alpha * 0.8)),
                width: 1.0,
                join: LineJoin::Round,
                cap: LineCap::Round,
                dash: Vec::new(),
            }),
            rule: FillRule::NonZero,
        });
    }
}

/// The lighter or darker shade of a fill for a preset's `lighten`/`darken` subpaths.
#[must_use]
pub fn shade(fill: &Paintable, mode: kiln_ooxml::st::PathFillMode) -> Paintable {
    use kiln_ooxml::st::PathFillMode as M;
    let f = |c: u32| match mode {
        M::Lighten => mix(c, 0xffff_ffff, 0.4),
        M::LightenLess => mix(c, 0xffff_ffff, 0.2),
        M::Darken => mix(c, 0x0000_00ff, 0.4),
        M::DarkenLess => mix(c, 0x0000_00ff, 0.2),
        M::Norm | M::None => c,
    };
    match fill {
        Paintable::Solid(c) => Paintable::Solid(f(*c)),
        Paintable::Gradient {
            stops,
            angle,
            focus,
        } => Paintable::Gradient {
            stops: stops.iter().map(|(o, c)| (*o, f(*c))).collect(),
            angle: *angle,
            focus: *focus,
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::drawingml::theme::office_theme;

    fn look(theme: &Theme) -> Look<'_> {
        Look {
            theme,
            ctx: kiln_ooxml::drawingml::theme::theme_color_context(
                theme,
                kiln_ooxml::drawingml::color::ColorMap::default(),
                None,
            ),
        }
    }

    #[test]
    fn quick_style_fill_uses_the_reference_colour() {
        let theme = office_theme();
        let l = look(&theme);
        let st = ShapeStyle::default_shape();
        let fill = resolve_fill(None, Some(&st), &l);
        // Office's accent 1 is 4472C4 (2013–2022 theme) or 156082 (2023); either way not black.
        assert!(
            matches!(fill, Paintable::Solid(c) if c != 0x0000_00ff),
            "{fill:?}"
        );
        let line = resolve_line(None, Some(&st), &l).expect("a style outline");
        assert!(line.width > 0.5);
        // An explicit no-fill wins over the style.
        assert_eq!(
            resolve_fill(Some(&Fill::None), Some(&st), &l),
            Paintable::None
        );
    }

    #[test]
    fn dash_presets_scale_with_the_width() {
        use kiln_ooxml::st::PresetLineDashVal as P;
        let d = dash_lengths(Some(&LineDash::Preset(P::Dash)), 2.0, LineCap::Butt);
        assert_eq!(d, vec![8.0, 6.0]);
        assert!(dash_lengths(Some(&LineDash::Preset(P::Solid)), 2.0, LineCap::Butt).is_empty());
    }

    #[test]
    fn arrowheads_of_hairlines_are_sized_as_a_two_point_line() {
        let head = |width: f32| {
            let line = Outline {
                paint: 0x0000_00ff,
                width,
                dash: Vec::new(),
                cap: LineCap::Butt,
                join: LineJoin::Round,
                head: None,
                tail: Some(LineEnd {
                    end_type: LineEndType::Triangle,
                    w: None,
                    len: None,
                }),
            };
            let mut out = Vec::new();
            let mut path = PathData::new();
            path.move_to(0.0, 0.0).line_to(100.0, 0.0);
            stroke_path(&mut out, &path, &line, false);
            match out.last() {
                Some(DrawOp::Path { path, .. }) => path.bounds(),
                other => panic!("no arrowhead: {other:?}"),
            }
        };
        // A 0.75 pt line: the medium head is 3 × 0.7 mm long.
        let thin = head(1.0);
        assert!((thin.w - 3.0 * px(ARROW_BASE_EMU)).abs() < 0.01, "{thin:?}");
        // A 6 pt line's head follows its own width.
        let thick = head(8.0);
        assert!((thick.w - 24.0).abs() < 0.01, "{thick:?}");
    }

    #[test]
    fn tiles_repeat_at_the_pictures_own_size_from_the_aligned_tile() {
        struct Texture;
        impl PictureSource for Texture {
            fn picture(&mut self, _part: &str) -> Option<(kiln_scene::ImageId, u32, u32)> {
                Some((kiln_scene::ImageId(1), 128, 128))
            }
            fn natural_size(&mut self, _part: &str) -> Option<(f32, f32)> {
                // 128 px at 75 dpi.
                Some((163.84, 163.84))
            }
        }
        let tiles = |tile: Option<TileInfo>| {
            let fill = Paintable::Picture {
                part: "ppt/media/image1.jpeg".into(),
                crop: (0.0, 0.0, 0.0, 0.0),
                tile: Some(Tiling::of(tile.as_ref())),
                alpha: 1.0,
            };
            let b = Bounds::new(0.0, 0.0, 400.0, 200.0);
            let mut out = Vec::new();
            fill_path(&mut out, &PathData::rect(b), b, &fill, &mut Texture);
            out.into_iter()
                .filter_map(|op| match op {
                    DrawOp::Image { dst, .. } => Some(dst),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let plain = tiles(None);
        assert_eq!(plain.len(), 3 * 2);
        assert_eq!((plain[0].x, plain[0].y, plain[0].w), (0.0, 0.0, 163.84));
        // Centred at half size: a tile on the box's centre, the grid reaching both edges.
        let centred = tiles(Some(TileInfo {
            tx: 0,
            ty: 0,
            sx: 0.5,
            sy: 0.5,
            flip: kiln_ooxml::st::TileFlipMode::None,
            algn: kiln_ooxml::st::RectAlignment::Ctr,
        }));
        assert!(
            centred
                .iter()
                .any(|d| (d.x + d.w / 2.0 - 200.0).abs() < 0.01
                    && (d.y + d.h / 2.0 - 100.0).abs() < 0.01)
        );
        assert!(centred.iter().all(|d| (d.w - 81.92).abs() < 0.01));
        assert!(centred.iter().any(|d| d.x <= 0.0) && centred.iter().any(|d| d.y <= 0.0));
    }

    #[test]
    fn mixing_and_alpha() {
        assert_eq!(mix(0x0000_00ff, 0xffff_ffff, 0.5), 0x8080_80ff);
        assert_eq!(with_alpha(0xff00_00ff, 0.5), 0xff00_0080);
    }
}
