// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slide transitions as plates: [`frame`] says, for a transition at a progress, which of the
//! outgoing slide, the incoming slide and a solid colour are drawn, bottom to top, each with
//! its [`Xform`].
//!
//! Directions follow `p:transition` as PowerPoint writes it: `dir` is the way things move, so
//! Push with `dir="u"` ("From Bottom") moves both slides up, and Wipe with `dir="l"` ("From
//! Right") moves the wipe's edge to the left. Wipe and Split have PowerPoint's soft edge
//! (a band where the incoming slide fades in); the moving transitions ease in and out.

use kiln_deck::transition::{Direction8, ShapeForm, Side, TransitionEffect};
use kiln_scene::{Bounds, PathData};

use super::{Ramp, Xform, ease};

/// What a plate shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plate {
    /// The slide being left.
    Outgoing,
    /// The slide coming in.
    Incoming,
    /// A solid colour over the whole slide (`0xRRGGBBAA`): through black, Flash's white.
    Fill(u32),
}

/// One plate of a transition frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// What it shows.
    pub plate: Plate,
    /// Where.
    pub xform: Xform,
}

impl Layer {
    fn new(plate: Plate, xform: Xform) -> Self {
        Self { plate, xform }
    }
}

/// Black.
pub const BLACK: u32 = 0x0000_00ff;
/// White.
pub const WHITE: u32 = 0xffff_ffff;

/// Soft edge of Wipe and Split, as a fraction of the distance the edge travels.
const FEATHER: f32 = 0.12;
/// Bars of Random Bars.
const BARS: usize = 96;

/// The plates of `effect` at `progress` (0–1) on a `w` × `h` slide, bottom to top. At 0 only
/// the outgoing slide shows (unless the effect starts from a colour), at 1 only the incoming.
#[must_use]
pub fn frame(effect: TransitionEffect, progress: f32, w: f32, h: f32) -> Vec<Layer> {
    use TransitionEffect as T;
    let p = ease::clamp01(progress);
    let incoming = || vec![Layer::new(Plate::Incoming, Xform::IDENTITY)];
    if p >= 1.0 {
        return incoming();
    }
    let (wf, hf) = (f64::from(w), f64::from(h));
    match effect {
        T::None => incoming(),
        T::Cut { through_black } => {
            if through_black {
                vec![Layer::new(Plate::Fill(BLACK), Xform::IDENTITY)]
            } else {
                incoming()
            }
        }
        T::Fade {
            through_black: true,
        } => through_black_frame(p, None),
        // Morph is played as Fade until it exists (Transition::playable).
        T::Fade { .. } | T::Morph { .. } => vec![
            Layer::new(Plate::Outgoing, Xform::IDENTITY),
            Layer::new(Plate::Incoming, Xform::faded(p)),
        ],
        T::Push { dir } => {
            let (dx, dy) = side_vector(dir);
            let e = f64::from(ease::smooth(p));
            vec![
                Layer::new(Plate::Outgoing, Xform::translate(dx * e * wf, dy * e * hf)),
                Layer::new(
                    Plate::Incoming,
                    Xform::translate(dx * (e - 1.0) * wf, dy * (e - 1.0) * hf),
                ),
            ]
        }
        T::Cover { dir } => {
            let (dx, dy) = dir8_vector(dir);
            let e = f64::from(ease::smooth(p));
            vec![
                Layer::new(Plate::Outgoing, Xform::IDENTITY),
                Layer::new(
                    Plate::Incoming,
                    Xform::translate(dx * (e - 1.0) * wf, dy * (e - 1.0) * hf),
                ),
            ]
        }
        T::Uncover { dir } => {
            let (dx, dy) = dir8_vector(dir);
            let e = f64::from(ease::smooth(p));
            vec![
                Layer::new(Plate::Incoming, Xform::IDENTITY),
                Layer::new(Plate::Outgoing, Xform::translate(dx * e * wf, dy * e * hf)),
            ]
        }
        T::Wipe { dir } => {
            let mut out = vec![Layer::new(Plate::Outgoing, Xform::IDENTITY)];
            out.extend(wipe(dir, p, w, h));
            out
        }
        T::Split { vertical, inward } => {
            let mut out = vec![Layer::new(Plate::Outgoing, Xform::IDENTITY)];
            out.extend(split(vertical, inward, p, w, h));
            out
        }
        T::Reveal { dir, through_black } => reveal(dir, through_black, p, w),
        T::RandomBars { vertical } => vec![
            Layer::new(Plate::Outgoing, Xform::IDENTITY),
            Layer::new(Plate::Incoming, Xform::clipped(bars(vertical, p, w, h))),
        ],
        T::Shape { form, inward } => {
            if inward {
                // The outgoing slide shrinks into the shape over the incoming one.
                vec![
                    Layer::new(Plate::Incoming, Xform::IDENTITY),
                    Layer::new(Plate::Outgoing, Xform::clipped(shape(form, 1.0 - p, w, h))),
                ]
            } else {
                vec![
                    Layer::new(Plate::Outgoing, Xform::IDENTITY),
                    Layer::new(Plate::Incoming, Xform::clipped(shape(form, p, w, h))),
                ]
            }
        }
        T::Flash => {
            // A white flash: up quickly, held, and down to show the new slide.
            let white = ease::keys(p, &[(0.0, 0.0), (0.3, 1.0), (0.55, 1.0), (1.0, 0.0)]);
            let base = if p < 0.45 {
                Plate::Outgoing
            } else {
                Plate::Incoming
            };
            vec![
                Layer::new(base, Xform::IDENTITY),
                Layer::new(Plate::Fill(WHITE), Xform::faded(white)),
            ]
        }
    }
}

/// Through black: the outgoing slide fades to black in the first half (moving by `shift` if
/// given), the incoming one comes up from black in the second.
fn through_black_frame(p: f32, shift: Option<(f64, f64)>) -> Vec<Layer> {
    let mut out = vec![Layer::new(Plate::Fill(BLACK), Xform::IDENTITY)];
    if p < 0.5 {
        let mut x = Xform::faded(1.0 - p * 2.0);
        if let Some((dx, dy)) = shift {
            x.matrix = kurbo::Affine::translate((dx, dy));
        }
        out.push(Layer::new(Plate::Outgoing, x));
    } else {
        out.push(Layer::new(Plate::Incoming, Xform::faded(p * 2.0 - 1.0)));
    }
    out
}

/// The unit vector a side moves along.
fn side_vector(s: Side) -> (f64, f64) {
    match s {
        Side::Left => (-1.0, 0.0),
        Side::Right => (1.0, 0.0),
        Side::Up => (0.0, -1.0),
        Side::Down => (0.0, 1.0),
    }
}

/// The vector one of eight directions moves along (diagonals move a whole slide on each axis).
fn dir8_vector(d: Direction8) -> (f64, f64) {
    match d {
        Direction8::Left => (-1.0, 0.0),
        Direction8::Right => (1.0, 0.0),
        Direction8::Up => (0.0, -1.0),
        Direction8::Down => (0.0, 1.0),
        Direction8::LeftUp => (-1.0, -1.0),
        Direction8::RightUp => (1.0, -1.0),
        Direction8::LeftDown => (-1.0, 1.0),
        Direction8::RightDown => (1.0, 1.0),
    }
}

/// A rectangle as a path, `None` when it is empty.
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Option<PathData> {
    (x1 > x0 && y1 > y0).then(|| PathData::rect(Bounds::from_corners(x0, y0, x1, y1)))
}

/// An interval `[a, b]` along an axis, as a rectangle across the other axis.
fn span(horizontal_axis: bool, a: f32, b: f32, w: f32, h: f32) -> Option<PathData> {
    if horizontal_axis {
        rect(a.max(0.0), 0.0, b.min(w), h)
    } else {
        rect(0.0, a.max(0.0), w, b.min(h))
    }
}

/// Wipe: the edge moves along `dir`; `dir="l"` starts at the right. The edge is PowerPoint's soft
/// band, `FEATHER` of the slide wide, in which the incoming slide goes from opaque to clear.
fn wipe(dir: Side, p: f32, w: f32, h: f32) -> Vec<Layer> {
    let horizontal = matches!(dir, Side::Left | Side::Right);
    let len = if horizontal { w } else { h };
    let feather = len * FEATHER;
    // The edge travels the slide and its own width, so that the band has left at the end.
    let solid = p * (len + feather) - feather;
    let edge = solid + feather;
    if edge <= 0.0 {
        return Vec::new();
    }
    // Distances along the reveal from where it starts, then as points on the slide.
    let from_end = matches!(dir, Side::Left | Side::Up);
    let at = |d: f32| {
        let v = if from_end { len - d } else { d };
        if horizontal {
            (v, h / 2.0)
        } else {
            (w / 2.0, v)
        }
    };
    vec![Layer::new(
        Plate::Incoming,
        Xform::ramped(Ramp {
            from: at(solid),
            to: at(edge),
            stops: vec![(0.0, 1.0), (1.0, 0.0)],
        }),
    )]
}

/// Split: a vertical or horizontal opening from the centre outwards, or from both edges
/// inwards, with soft edges.
fn split(vertical: bool, inward: bool, p: f32, w: f32, h: f32) -> Vec<Layer> {
    // A vertical split opens along the x axis.
    let horizontal_axis = vertical;
    let len = if horizontal_axis { w } else { h };
    let half = len / 2.0;
    let feather = half * FEATHER;
    let solid = p * (half + feather) - feather;
    if solid + feather <= 0.0 {
        return Vec::new();
    }
    // Opacity along the axis, as offsets of the slide's length.
    let o = |v: f32| v / len;
    let stops = if inward {
        vec![
            (o(solid), 1.0),
            (o(solid + feather), 0.0),
            (o(len - solid - feather), 0.0),
            (o(len - solid), 1.0),
        ]
    } else {
        vec![
            (o(half - solid - feather), 0.0),
            (o(half - solid), 1.0),
            (o(half + solid), 1.0),
            (o(half + solid + feather), 0.0),
        ]
    };
    // Stops past the ends of the slide still order the ramp; keep them sorted and distinct.
    let mut stops: Vec<(f32, f32)> = stops;
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (from, to) = if horizontal_axis {
        ((0.0, h / 2.0), (w, h / 2.0))
    } else {
        ((w / 2.0, 0.0), (w / 2.0, h))
    };
    vec![Layer::new(
        Plate::Incoming,
        Xform::ramped(Ramp { from, to, stops }),
    )]
}

/// Reveal: the outgoing slide slides away along `dir` and fades, uncovering the incoming one
/// (smoothly), or fades through black (through black).
fn reveal(dir: Side, through_black: bool, p: f32, w: f32) -> Vec<Layer> {
    let (dx, _) = side_vector(match dir {
        Side::Right => Side::Right,
        _ => Side::Left,
    });
    let travel = f64::from(w) * 0.3;
    if through_black {
        let e = f64::from(ease::smooth((p * 2.0).min(1.0)));
        return through_black_frame(p, Some((dx * e * travel, 0.0)));
    }
    let e = ease::smooth(p);
    vec![
        Layer::new(Plate::Incoming, Xform::faded(ease::out(p * 1.4))),
        Layer::new(
            Plate::Outgoing,
            Xform {
                matrix: kurbo::Affine::translate((dx * f64::from(e) * travel, 0.0)),
                opacity: 1.0 - e,
                clip: None,
                ramp: None,
            },
        ),
    ]
}

/// A deterministic permutation of `0..n` (the order Random Bars appear in).
fn permutation(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut s = seed | 1;
    for i in (1..n).rev() {
        // xorshift64*
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        let r = s.wrapping_mul(0x2545_f491_4f6c_dd1d);
        let j = usize::try_from(r % (i as u64 + 1)).unwrap_or(0);
        order.swap(i, j);
    }
    order
}

/// Random Bars: thin bars of the incoming slide appear in a random order (horizontal bars, or
/// vertical with `vertical`).
fn bars(vertical: bool, p: f32, w: f32, h: f32) -> PathData {
    let order = permutation(BARS, 0x5eed_ba25);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let shown = ((p * BARS as f32).floor() as usize).min(BARS);
    let len = if vertical { w } else { h };
    #[allow(clippy::cast_precision_loss)]
    let bar = len / BARS as f32;
    let mut picked: Vec<usize> = order[..shown].to_vec();
    picked.sort_unstable();
    let mut path = PathData::new();
    // Neighbouring bars merge into one rectangle (no seams between them).
    let mut i = 0;
    while i < picked.len() {
        let start = picked[i];
        let mut end = start;
        while i + 1 < picked.len() && picked[i + 1] == end + 1 {
            i += 1;
            end = picked[i];
        }
        #[allow(clippy::cast_precision_loss)]
        let (a, b) = (start as f32 * bar, (end + 1) as f32 * bar);
        if let Some(r) = span(vertical, a, b, w, h) {
            path.extend(&r);
        }
        i += 1;
    }
    path
}

/// The Shape transition's opening at `p`: a circle, diamond or plus centred on the slide that
/// covers it at 1.
fn shape(form: ShapeForm, p: f32, w: f32, h: f32) -> PathData {
    let (cx, cy) = (w / 2.0, h / 2.0);
    let mut path = PathData::new();
    if p <= 0.0 {
        return path;
    }
    match form {
        ShapeForm::Circle => {
            let r = p * (cx * cx + cy * cy).sqrt();
            // Four cubic arcs.
            let k = 0.552_284_8 * r;
            path.move_to(cx + r, cy)
                .cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r)
                .cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy)
                .cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r)
                .cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy)
                .close();
        }
        ShapeForm::Diamond => {
            // |x|/a + |y|/b = 1 through the corners at 1: a = w, b = h.
            let (a, b) = (p * w, p * h);
            path.move_to(cx, cy - b)
                .line_to(cx + a, cy)
                .line_to(cx, cy + b)
                .line_to(cx - a, cy)
                .close();
        }
        ShapeForm::Plus => {
            let (arm_w, arm_h) = (p * w, p * h);
            let (bar_w, bar_h) = (p * w / 2.0, p * h / 2.0);
            // One outline of the cross (so the overlap is not a hole under non-zero fill).
            path.move_to(cx - bar_w, cy - arm_h)
                .line_to(cx + bar_w, cy - arm_h)
                .line_to(cx + bar_w, cy - bar_h)
                .line_to(cx + arm_w, cy - bar_h)
                .line_to(cx + arm_w, cy + bar_h)
                .line_to(cx + bar_w, cy + bar_h)
                .line_to(cx + bar_w, cy + arm_h)
                .line_to(cx - bar_w, cy + arm_h)
                .line_to(cx - bar_w, cy + bar_h)
                .line_to(cx - arm_w, cy + bar_h)
                .line_to(cx - arm_w, cy - bar_h)
                .line_to(cx - bar_w, cy - bar_h)
                .close();
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::path_contains;
    use kiln_deck::transition::{GALLERY, MorphBy};

    const W: f32 = 1280.0;
    const H: f32 = 720.0;

    /// The plate that a point shows on top (with its opacity), for a frame.
    fn top_at(layers: &[Layer], x: f32, y: f32) -> Option<(Plate, f32)> {
        layers.iter().rev().find_map(|l| {
            let inv = l.xform.matrix.inverse();
            let local = inv * kurbo::Point::new(f64::from(x), f64::from(y));
            #[allow(clippy::cast_possible_truncation)]
            let (lx, ly) = (local.x as f32, local.y as f32);
            let in_slide = (0.0..=W).contains(&lx) && (0.0..=H).contains(&ly);
            let in_clip = l
                .xform
                .clip
                .as_ref()
                .is_none_or(|c| path_contains(c, lx, ly));
            let alpha = l.xform.opacity * l.xform.ramp.as_ref().map_or(1.0, |r| r.at(lx, ly));
            (in_slide && in_clip && alpha > 0.0).then_some((l.plate, alpha))
        })
    }

    fn all_effects() -> Vec<TransitionEffect> {
        let mut v: Vec<TransitionEffect> = GALLERY.to_vec();
        v.extend([
            TransitionEffect::Fade {
                through_black: true,
            },
            TransitionEffect::Cut {
                through_black: true,
            },
            TransitionEffect::Push { dir: Side::Left },
            TransitionEffect::Push { dir: Side::Down },
            TransitionEffect::Wipe { dir: Side::Up },
            TransitionEffect::Wipe { dir: Side::Right },
            TransitionEffect::Split {
                vertical: false,
                inward: true,
            },
            TransitionEffect::Reveal {
                dir: Side::Right,
                through_black: true,
            },
            TransitionEffect::RandomBars { vertical: true },
            TransitionEffect::Shape {
                form: ShapeForm::Diamond,
                inward: false,
            },
            TransitionEffect::Shape {
                form: ShapeForm::Plus,
                inward: false,
            },
            TransitionEffect::Shape {
                form: ShapeForm::Circle,
                inward: true,
            },
            TransitionEffect::Uncover {
                dir: Direction8::RightDown,
            },
            TransitionEffect::Cover {
                dir: Direction8::LeftUp,
            },
        ]);
        v
    }

    #[test]
    fn every_transition_ends_on_the_incoming_slide() {
        for e in all_effects() {
            let end = frame(e, 1.0, W, H);
            assert_eq!(end.len(), 1, "{e:?}");
            assert_eq!(end[0].plate, Plate::Incoming);
            assert_eq!(end[0].xform, Xform::IDENTITY);
            // Beyond the end too.
            assert_eq!(frame(e, 7.0, W, H), end);
        }
    }

    #[test]
    fn every_transition_starts_on_the_outgoing_slide_or_a_colour() {
        for e in all_effects() {
            let start = frame(e, 0.0, W, H);
            let centre = top_at(&start, W / 2.0, H / 2.0);
            let ok = match e {
                // Instant effects show the new slide (or black) straight away.
                TransitionEffect::None
                | TransitionEffect::Cut {
                    through_black: false,
                } => centre == Some((Plate::Incoming, 1.0)),
                TransitionEffect::Cut {
                    through_black: true,
                } => centre == Some((Plate::Fill(BLACK), 1.0)),
                // Reveal uncovers the incoming slide beneath the outgoing one.
                _ => centre == Some((Plate::Outgoing, 1.0)),
            };
            assert!(ok, "{e:?}: {centre:?}");
        }
    }

    #[test]
    fn fade_cross_fades_and_through_black_passes_black() {
        let f = frame(
            TransitionEffect::Fade {
                through_black: false,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(f[0].plate, Plate::Outgoing);
        assert_eq!((f[1].plate, f[1].xform.opacity), (Plate::Incoming, 0.5));
        let b = TransitionEffect::Fade {
            through_black: true,
        };
        let quarter = frame(b, 0.25, W, H);
        assert_eq!(quarter[0].plate, Plate::Fill(BLACK));
        assert_eq!(
            (quarter[1].plate, quarter[1].xform.opacity),
            (Plate::Outgoing, 0.5)
        );
        let mid = frame(b, 0.5, W, H);
        assert_eq!(top_at(&mid, 10.0, 10.0), Some((Plate::Fill(BLACK), 1.0)));
        let late = frame(b, 0.75, W, H);
        assert_eq!(
            (late[1].plate, late[1].xform.opacity),
            (Plate::Incoming, 0.5)
        );
        // Morph plays as a smooth fade.
        assert_eq!(
            frame(
                TransitionEffect::Morph {
                    by: MorphBy::Objects
                },
                0.5,
                W,
                H
            ),
            f
        );
    }

    #[test]
    fn push_moves_both_slides_the_way_dir_says() {
        let f = frame(TransitionEffect::Push { dir: Side::Up }, 0.5, W, H);
        // Half way (smooth easing is symmetric): each slide is half a slide up.
        let c = |l: &Layer| l.xform.coeffs();
        assert_eq!(c(&f[0])[5], -360.0);
        assert_eq!(c(&f[1])[5], 360.0);
        assert_eq!(
            top_at(&f, W / 2.0, 100.0).map(|t| t.0),
            Some(Plate::Outgoing)
        );
        assert_eq!(
            top_at(&f, W / 2.0, 600.0).map(|t| t.0),
            Some(Plate::Incoming)
        );
        let l = frame(TransitionEffect::Push { dir: Side::Left }, 0.25, W, H);
        assert!(c(&l[0])[4] < 0.0 && c(&l[1])[4] > 0.0);
        // Eased: slower at the start than linear.
        assert!(c(&l[0])[4] > -0.25 * W);
    }

    #[test]
    fn cover_moves_the_incoming_slide_over_the_outgoing_one() {
        let f = frame(
            TransitionEffect::Cover {
                dir: Direction8::Left,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(f[0].xform, Xform::IDENTITY);
        assert_eq!(f[1].plate, Plate::Incoming);
        assert_eq!(f[1].xform.coeffs()[4], W / 2.0);
        let d = frame(
            TransitionEffect::Cover {
                dir: Direction8::RightDown,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(d[1].xform.coeffs()[4], -W / 2.0);
        assert_eq!(d[1].xform.coeffs()[5], -H / 2.0);
    }

    #[test]
    fn uncover_moves_the_outgoing_slide_off_the_incoming_one() {
        let f = frame(
            TransitionEffect::Uncover {
                dir: Direction8::Down,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(
            (f[0].plate, &f[0].xform),
            (Plate::Incoming, &Xform::IDENTITY)
        );
        assert_eq!(f[1].plate, Plate::Outgoing);
        assert_eq!(f[1].xform.coeffs()[5], H / 2.0);
        assert_eq!(top_at(&f, 10.0, 10.0).map(|t| t.0), Some(Plate::Incoming));
        assert_eq!(
            top_at(&f, 10.0, H - 10.0).map(|t| t.0),
            Some(Plate::Outgoing)
        );
    }

    #[test]
    fn wipe_reveals_from_the_side_it_starts_at_with_a_soft_edge() {
        // dir="l" ("From Right"): the right part is revealed first.
        let f = frame(TransitionEffect::Wipe { dir: Side::Left }, 0.5, W, H);
        assert_eq!(top_at(&f, W - 10.0, H / 2.0), Some((Plate::Incoming, 1.0)));
        assert_eq!(top_at(&f, 10.0, H / 2.0), Some((Plate::Outgoing, 1.0)));
        // Somewhere between, the edge is partly transparent.
        let soft = (0..1280)
            .step_by(4)
            .filter_map(|x| {
                #[allow(clippy::cast_precision_loss)]
                top_at(&f, x as f32, H / 2.0)
            })
            .any(|(plate, a)| plate == Plate::Incoming && a > 0.0 && a < 1.0);
        assert!(soft);
        let down = frame(TransitionEffect::Wipe { dir: Side::Down }, 0.3, W, H);
        assert_eq!(
            top_at(&down, W / 2.0, 5.0).map(|t| t.0),
            Some(Plate::Incoming)
        );
        assert_eq!(
            top_at(&down, W / 2.0, H - 5.0).map(|t| t.0),
            Some(Plate::Outgoing)
        );
    }

    #[test]
    fn split_opens_from_the_centre_or_the_edges() {
        let out = frame(
            TransitionEffect::Split {
                vertical: true,
                inward: false,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(top_at(&out, W / 2.0, 10.0), Some((Plate::Incoming, 1.0)));
        assert_eq!(top_at(&out, 5.0, H / 2.0), Some((Plate::Outgoing, 1.0)));
        assert_eq!(top_at(&out, W - 5.0, H / 2.0), Some((Plate::Outgoing, 1.0)));
        let inward = frame(
            TransitionEffect::Split {
                vertical: false,
                inward: true,
            },
            0.5,
            W,
            H,
        );
        assert_eq!(top_at(&inward, W / 2.0, 5.0), Some((Plate::Incoming, 1.0)));
        assert_eq!(
            top_at(&inward, W / 2.0, H - 5.0),
            Some((Plate::Incoming, 1.0))
        );
        assert_eq!(
            top_at(&inward, W / 2.0, H / 2.0),
            Some((Plate::Outgoing, 1.0))
        );
    }

    #[test]
    fn reveal_slides_the_outgoing_slide_away_and_fades_it() {
        let f = frame(
            TransitionEffect::Reveal {
                dir: Side::Left,
                through_black: false,
            },
            0.5,
            W,
            H,
        );
        let out = f.iter().find(|l| l.plate == Plate::Outgoing).unwrap();
        assert!(out.xform.coeffs()[4] < 0.0);
        assert!((out.xform.opacity - 0.5).abs() < 1e-3);
        let b = frame(
            TransitionEffect::Reveal {
                dir: Side::Right,
                through_black: true,
            },
            0.25,
            W,
            H,
        );
        assert_eq!(b[0].plate, Plate::Fill(BLACK));
        assert!(b[1].xform.coeffs()[4] > 0.0);
    }

    #[test]
    fn random_bars_reveal_about_the_progress_in_bars() {
        let covered = |p: f32, vertical: bool| {
            let f = frame(TransitionEffect::RandomBars { vertical }, p, W, H);
            let mut n = 0;
            for i in 0..720 {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 + 0.5;
                let (x, y) = if vertical {
                    (t * W / 720.0, H / 2.0)
                } else {
                    (W / 2.0, t)
                };
                if top_at(&f, x, y).map(|t| t.0) == Some(Plate::Incoming) {
                    n += 1;
                }
            }
            f64::from(n) / 720.0
        };
        for vertical in [false, true] {
            assert!(covered(0.0, vertical) < 0.01);
            let half = covered(0.5, vertical);
            assert!((half - 0.5).abs() < 0.03, "{half}");
            assert!(covered(0.9, vertical) > 0.85);
        }
        // The same frame every time (a deterministic order).
        assert_eq!(
            frame(TransitionEffect::RandomBars { vertical: false }, 0.37, W, H),
            frame(TransitionEffect::RandomBars { vertical: false }, 0.37, W, H)
        );
        let mut p = permutation(50, 9);
        p.sort_unstable();
        assert_eq!(p, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn shape_grows_a_circle_diamond_or_plus_from_the_centre() {
        for form in [ShapeForm::Circle, ShapeForm::Diamond, ShapeForm::Plus] {
            let e = TransitionEffect::Shape {
                form,
                inward: false,
            };
            let f = frame(e, 0.3, W, H);
            assert_eq!(
                top_at(&f, W / 2.0, H / 2.0).map(|t| t.0),
                Some(Plate::Incoming),
                "{form:?}"
            );
            assert_eq!(
                top_at(&f, 2.0, 2.0).map(|t| t.0),
                Some(Plate::Outgoing),
                "{form:?}"
            );
            let late = frame(e, 0.999, W, H);
            assert_eq!(
                top_at(&late, 2.0, 2.0).map(|t| t.0),
                Some(Plate::Incoming),
                "{form:?}"
            );
        }
        // The plus has arms: along the axes, not at the diagonal.
        let plus = frame(
            TransitionEffect::Shape {
                form: ShapeForm::Plus,
                inward: false,
            },
            0.4,
            W,
            H,
        );
        assert_eq!(
            top_at(&plus, W / 2.0 + 0.35 * W, H / 2.0).map(|t| t.0),
            Some(Plate::Incoming)
        );
        assert_eq!(
            top_at(&plus, W / 2.0 + 0.3 * W, H / 2.0 + 0.3 * H).map(|t| t.0),
            Some(Plate::Outgoing)
        );
        // Inward shrinks the old slide into the shape.
        let inward = frame(
            TransitionEffect::Shape {
                form: ShapeForm::Circle,
                inward: true,
            },
            0.3,
            W,
            H,
        );
        assert_eq!(
            top_at(&inward, W / 2.0, H / 2.0).map(|t| t.0),
            Some(Plate::Outgoing)
        );
        assert_eq!(
            top_at(&inward, 2.0, 2.0).map(|t| t.0),
            Some(Plate::Incoming)
        );
    }

    #[test]
    fn flash_goes_white_between_the_slides() {
        let at = |p| top_at(&frame(TransitionEffect::Flash, p, W, H), 50.0, 50.0);
        assert_eq!(at(0.1).map(|t| t.0), Some(Plate::Fill(WHITE)));
        assert_eq!(at(0.4), Some((Plate::Fill(WHITE), 1.0)));
        let late = frame(TransitionEffect::Flash, 0.9, W, H);
        assert_eq!(late[0].plate, Plate::Incoming);
        assert!(late[1].xform.opacity < 0.3);
    }

    #[test]
    fn instant_transitions_need_no_frames() {
        assert_eq!(frame(TransitionEffect::None, 0.0, W, H).len(), 1);
        assert_eq!(
            frame(
                TransitionEffect::Cut {
                    through_black: true
                },
                0.5,
                W,
                H
            )[0]
            .plate,
            Plate::Fill(BLACK)
        );
    }
}
