// SPDX-License-Identifier: AGPL-3.0-or-later
//! Animation effects as poses: [`pose`] gives what one effect does to its target at a stage of
//! its run, and [`fold`] combines every effect on a target, in sequence order, into the pose
//! the show draws it with.
//!
//! The geometry follows PowerPoint's presets (the behaviours PowerPoint writes for each
//! `presetID`, which LibreOffice's `effects.xml` mirrors): Fly In starts with the shape just
//! outside the slide edge, Float In rises a tenth of the slide while it fades in, Zoom and
//! Grow & Turn scale from the shape's centre while fading, Split opens a barn door, Wipe
//! reveals from a side, Pulse scales to 105 % and back, Teeter rocks ±2°, Spin and Grow/Shrink
//! keep their end state, the exits mirror the entrances.

use kiln_deck::animation::{AnimationEffect, EffectClass};
use kiln_deck::transition::{Direction8, Side};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_scene::{Bounds, PathData};
use kurbo::{Affine, Vec2};

use super::{Ramp, ease};

/// Where an effect is in its run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    /// Not started.
    Before,
    /// Running: linear progress of the current iteration, 0–1.
    Running(f32),
    /// Finished.
    After,
}

/// The geometry an effect works with, in slide px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// The target's box (the shape's frame, or a paragraph range's band).
    pub rect: Bounds,
    /// What its drawing can touch (shadows and outlines included): clips cover this.
    pub visual: Bounds,
    /// The slide's size.
    pub slide: (f32, f32),
}

impl Target {
    fn centre(&self) -> (f64, f64) {
        (
            f64::from(self.rect.x + self.rect.w / 2.0),
            f64::from(self.rect.y + self.rect.h / 2.0),
        )
    }
}

/// How a target is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    /// Shown at all.
    pub visible: bool,
    /// Opacity, 0–1.
    pub opacity: f32,
    /// Matrix in slide px.
    pub matrix: Affine,
    /// Clip in the target's own coordinates (before the matrix).
    pub clip: Option<PathData>,
    /// A colour drawn over the target's own colours, with its strength (Colour Pulse).
    pub tint: Option<(Option<DrawingColor>, f32)>,
    /// An opacity ramp in the target's own coordinates (the soft edge of Wipe and Split).
    pub ramp: Option<Ramp>,
}

impl Default for Pose {
    fn default() -> Self {
        Self::REST
    }
}

impl Pose {
    /// Shown as drawn on the slide.
    pub const REST: Pose = Pose {
        visible: true,
        opacity: 1.0,
        matrix: Affine::IDENTITY,
        clip: None,
        tint: None,
        ramp: None,
    };

    /// Not shown.
    pub const HIDDEN: Pose = Pose {
        visible: false,
        opacity: 1.0,
        matrix: Affine::IDENTITY,
        clip: None,
        tint: None,
        ramp: None,
    };

    /// Whether anything of it shows.
    #[must_use]
    pub fn shows(&self) -> bool {
        self.visible
            && self.opacity > 1.0 / 512.0
            && self.clip.as_ref().is_none_or(|c| !c.is_empty())
    }
}

/// What one effect contributes at a stage.
#[derive(Debug, Clone, PartialEq, Default)]
struct Delta {
    /// Sets visibility.
    visible: Option<bool>,
    opacity: f32,
    matrix: Option<Affine>,
    clip: Option<PathData>,
    tint: Option<(Option<DrawingColor>, f32)>,
    ramp: Option<Ramp>,
}

impl Delta {
    fn shown() -> Self {
        Self {
            visible: Some(true),
            opacity: 1.0,
            ..Self::default()
        }
    }

    fn hidden() -> Self {
        Self {
            visible: Some(false),
            opacity: 1.0,
            ..Self::default()
        }
    }

    fn none() -> Self {
        Self {
            opacity: 1.0,
            ..Self::default()
        }
    }
}

/// Whether a target starts hidden: its first effect is an entrance (PowerPoint hides shapes
/// until they enter).
#[must_use]
pub fn starts_hidden<'a>(effects: impl IntoIterator<Item = &'a AnimationEffect>) -> bool {
    effects
        .into_iter()
        .next()
        .is_some_and(|e| play_class(e) == EffectClass::Entrance)
}

/// The class an effect plays as (kept effects Kiln does not play enter as Appear and exit as
/// Disappear).
#[must_use]
pub fn play_class(e: &AnimationEffect) -> EffectClass {
    e.class()
}

/// Combines the effects on a target, in sequence order, each at its stage.
#[must_use]
pub fn fold<'a>(
    effects: impl IntoIterator<Item = (&'a AnimationEffect, Stage)>,
    target: &Target,
) -> Pose {
    let effects: Vec<(&AnimationEffect, Stage)> = effects.into_iter().collect();
    let mut out = if starts_hidden(effects.iter().map(|(e, _)| *e)) {
        Pose::HIDDEN
    } else {
        Pose::REST
    };
    for (effect, stage) in effects {
        let d = delta(effect, stage, target);
        if let Some(v) = d.visible {
            out.visible = v;
        }
        out.opacity *= d.opacity;
        if let Some(m) = d.matrix {
            out.matrix = m * out.matrix;
        }
        if let Some(c) = d.clip {
            out.clip = Some(c);
        }
        if let Some(r) = d.ramp {
            out.ramp = Some(r);
        }
        if d.tint.is_some() {
            out.tint = d.tint;
        }
    }
    out.opacity = out.opacity.clamp(0.0, 1.0);
    out
}

/// The pose of a target with only `effect` on it, at `stage` (as [`fold`] of one effect).
#[must_use]
pub fn pose(effect: &AnimationEffect, stage: Stage, target: &Target) -> Pose {
    fold([(effect, stage)], target)
}

/// Scaling by `(sx, sy)` and turning by `degrees` about a point.
fn about(c: (f64, f64), sx: f64, sy: f64, degrees: f64) -> Affine {
    Affine::translate(c)
        * Affine::rotate(degrees.to_radians())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-c.0, -c.1))
}

/// Where Fly In starts (or Fly Out ends): the offset that puts the shape just outside the slide
/// edge on the side `d` names.
fn off_slide(d: Direction8, t: &Target) -> Vec2 {
    let (cx, cy) = t.centre();
    let (sw, sh) = (f64::from(t.slide.0), f64::from(t.slide.1));
    let (hw, hh) = (f64::from(t.rect.w) / 2.0, f64::from(t.rect.h) / 2.0);
    let left = -hw - cx;
    let right = sw + hw - cx;
    let up = -hh - cy;
    let down = sh + hh - cy;
    match d {
        Direction8::Left => Vec2::new(left, 0.0),
        Direction8::Right => Vec2::new(right, 0.0),
        Direction8::Up => Vec2::new(0.0, up),
        Direction8::Down => Vec2::new(0.0, down),
        Direction8::LeftUp => Vec2::new(left, up),
        Direction8::RightUp => Vec2::new(right, up),
        Direction8::LeftDown => Vec2::new(left, down),
        Direction8::RightDown => Vec2::new(right, down),
    }
}

fn rect_path(x0: f32, y0: f32, x1: f32, y1: f32) -> PathData {
    if x1 > x0 && y1 > y0 {
        PathData::rect(Bounds::from_corners(x0, y0, x1, y1))
    } else {
        PathData::new()
    }
}

/// Split (barn door) at an opening `p`: vertical opens along x, from the centre or the edges.
fn barn(vertical: bool, inward: bool, p: f32, v: Bounds) -> PathData {
    let (lo, hi) = if vertical {
        (v.x, v.right())
    } else {
        (v.y, v.bottom())
    };
    let half = (hi - lo) / 2.0;
    let mid = lo + half;
    let open = half * p;
    let mut path = PathData::new();
    let mut add = |a: f32, b: f32| {
        let r = if vertical {
            rect_path(a, v.y, b, v.bottom())
        } else {
            rect_path(v.x, a, v.right(), b)
        };
        path.extend(&r);
    };
    if inward {
        add(lo, lo + open);
        add(hi - open, hi);
    } else {
        add(mid - open, mid + open);
    }
    path
}

/// Wipe from a side at `p`.
fn wipe_clip(from: Side, p: f32, v: Bounds) -> PathData {
    match from {
        Side::Left => rect_path(v.x, v.y, v.x + v.w * p, v.bottom()),
        Side::Right => rect_path(v.right() - v.w * p, v.y, v.right(), v.bottom()),
        Side::Up => rect_path(v.x, v.y, v.right(), v.y + v.h * p),
        Side::Down => rect_path(v.x, v.bottom() - v.h * p, v.right(), v.bottom()),
    }
}

/// The soft edge of an effect's wipe, as a share of the target's size.
const EDGE: f32 = 0.18;

/// Wipe's soft edge: the target is revealed from a side with a band at the edge in which it
/// fades out. The clip of [`wipe_clip`] is the band's far end.
fn wipe_ramp(from: Side, p: f32, v: Bounds) -> Ramp {
    let (len, start, sign, horizontal) = match from {
        Side::Left => (v.w, v.x, 1.0, true),
        Side::Right => (v.w, v.right(), -1.0, true),
        Side::Up => (v.h, v.y, 1.0, false),
        Side::Down => (v.h, v.bottom(), -1.0, false),
    };
    let feather = len * EDGE;
    let edge = start + sign * len * p;
    let solid = edge - sign * feather;
    let pt = |d: f32| {
        if horizontal {
            (d, v.y + v.h / 2.0)
        } else {
            (v.x + v.w / 2.0, d)
        }
    };
    Ramp {
        from: pt(solid),
        to: pt(edge),
        stops: vec![(0.0, 1.0), (1.0, 0.0)],
    }
}

/// Split's soft edges (one or two bands at the edges of the opening).
fn barn_ramp(vertical: bool, inward: bool, p: f32, v: Bounds) -> Ramp {
    let (lo, len) = if vertical { (v.x, v.w) } else { (v.y, v.h) };
    let half = len / 2.0;
    let open = half * p;
    let feather = (half * EDGE).min(open.max(0.0));
    let o = |d: f32| if len > 0.0 { d / len } else { 0.0 };
    let stops = if inward {
        vec![
            (o(open - feather), 1.0),
            (o(open), 0.0),
            (o(len - open), 0.0),
            (o(len - open + feather), 1.0),
        ]
    } else {
        vec![
            (o(half - open), 0.0),
            (o(half - open + feather), 1.0),
            (o(half + open - feather), 1.0),
            (o(half + open), 0.0),
        ]
    };
    let c = if vertical {
        v.y + v.h / 2.0
    } else {
        v.x + v.w / 2.0
    };
    let (from, to) = if vertical {
        ((lo, c), (lo + len, c))
    } else {
        ((c, lo), (c, lo + len))
    };
    Ramp { from, to, stops }
}

/// Teeter's angle at `p`: ±2° rocking, back to rest at the end.
fn teeter(p: f32) -> f64 {
    f64::from(ease::keys(
        p,
        &[
            (0.0, 0.0),
            (0.1, 2.0),
            (0.2, 2.0),
            (0.4, -2.0),
            (0.6, 2.0),
            (0.8, -2.0),
            (1.0, 0.0),
        ],
    ))
}

/// What one effect does at a stage.
#[allow(clippy::too_many_lines)]
fn delta(effect: &AnimationEffect, stage: Stage, t: &Target) -> Delta {
    use AnimationEffect as A;
    let c = t.centre();
    let class = play_class(effect);
    match stage {
        Stage::Before => return Delta::none(),
        Stage::After => {
            return match effect {
                // Emphasis effects that keep their end state.
                A::Spin { degrees } => Delta {
                    matrix: Some(about(c, 1.0, 1.0, f64::from(*degrees))),
                    ..Delta::none()
                },
                A::GrowShrink { scale_pct } => {
                    let s = f64::from(*scale_pct) / 100.0;
                    Delta {
                        matrix: Some(about(c, s, s, 0.0)),
                        ..Delta::none()
                    }
                }
                _ => match class {
                    EffectClass::Entrance => Delta::shown(),
                    EffectClass::Exit => Delta::hidden(),
                    _ => Delta::none(),
                },
            };
        }
        Stage::Running(_) => {}
    }
    let Stage::Running(p) = stage else {
        return Delta::none();
    };
    let p = ease::clamp01(p);
    let pf = f64::from(p);
    let v = t.visual;
    match effect {
        A::Appear => Delta::shown(),
        A::Disappear => Delta::hidden(),
        A::Fade => Delta {
            opacity: p,
            ..Delta::shown()
        },
        A::FadeOut => Delta {
            opacity: 1.0 - p,
            ..Delta::shown()
        },
        A::FlyIn { from } => {
            let o = off_slide(*from, t) * (1.0 - pf);
            Delta {
                matrix: Some(Affine::translate(o)),
                ..Delta::shown()
            }
        }
        A::FlyOut { to } => {
            let o = off_slide(*to, t) * pf;
            Delta {
                matrix: Some(Affine::translate(o)),
                ..Delta::shown()
            }
        }
        A::FloatIn { down } => {
            let e = f64::from(ease::out(p));
            let rise = f64::from(t.slide.1) * 0.1 * (1.0 - e);
            Delta {
                opacity: p,
                matrix: Some(Affine::translate((0.0, if *down { -rise } else { rise }))),
                ..Delta::shown()
            }
        }
        A::Split { vertical, inward } => Delta {
            clip: Some(barn(*vertical, *inward, p, v)),
            ramp: Some(barn_ramp(*vertical, *inward, p, v)),
            ..Delta::shown()
        },
        A::Wipe { from } => Delta {
            clip: Some(wipe_clip(*from, p, v)),
            ramp: Some(wipe_ramp(*from, p, v)),
            ..Delta::shown()
        },
        A::Zoom => Delta {
            opacity: p,
            matrix: Some(about(c, pf, pf, 0.0)),
            ..Delta::shown()
        },
        A::ZoomOut => Delta {
            opacity: 1.0 - p,
            matrix: Some(about(c, 1.0 - pf, 1.0 - pf, 0.0)),
            ..Delta::shown()
        },
        A::GrowTurn => Delta {
            opacity: p,
            matrix: Some(about(c, pf, pf, (pf - 1.0) * 90.0)),
            ..Delta::shown()
        },
        A::Pulse => {
            let s = 1.0 + 0.05 * (std::f64::consts::PI * pf).sin();
            Delta {
                matrix: Some(about(c, s, s, 0.0)),
                ..Delta::none()
            }
        }
        A::ColorPulse { color } => Delta {
            #[allow(clippy::cast_possible_truncation)]
            tint: Some((color.clone(), (std::f64::consts::PI * pf).sin() as f32)),
            ..Delta::none()
        },
        A::Teeter => Delta {
            matrix: Some(about(c, 1.0, 1.0, teeter(p))),
            ..Delta::none()
        },
        A::Spin { degrees } => Delta {
            matrix: Some(about(
                c,
                1.0,
                1.0,
                f64::from(*degrees) * f64::from(ease::smooth(p)),
            )),
            ..Delta::none()
        },
        A::GrowShrink { scale_pct } => {
            let s = 1.0 + (f64::from(*scale_pct) / 100.0 - 1.0) * f64::from(ease::smooth(p));
            Delta {
                matrix: Some(about(c, s, s, 0.0)),
                ..Delta::none()
            }
        }
        A::Other { class, .. } => match class {
            EffectClass::Entrance => Delta::shown(),
            EffectClass::Exit => Delta::none(),
            _ => Delta::none(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::path_contains;
    use kiln_deck::animation::gallery;

    fn target() -> Target {
        Target {
            rect: Bounds::new(400.0, 200.0, 200.0, 100.0),
            visual: Bounds::new(398.0, 198.0, 204.0, 104.0),
            slide: (1280.0, 720.0),
        }
    }

    fn point(m: Affine, x: f64, y: f64) -> (f64, f64) {
        let p = m * kurbo::Point::new(x, y);
        (p.x, p.y)
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn every_preset_rests_before_and_after_except_its_lasting_change() {
        let t = target();
        for e in gallery() {
            let before = pose(&e, Stage::Before, &t);
            let after = pose(&e, Stage::After, &t);
            match e.class() {
                EffectClass::Entrance => {
                    assert!(!before.shows(), "{e:?} shows before it enters");
                    assert_eq!(after, Pose::REST, "{e:?}");
                }
                EffectClass::Exit => {
                    assert_eq!(before, Pose::REST, "{e:?}");
                    assert!(!after.shows(), "{e:?} shows after it exits");
                }
                _ => {
                    assert_eq!(before, Pose::REST, "{e:?}");
                    match e {
                        AnimationEffect::GrowShrink { .. } => {
                            assert!(after.matrix != Affine::IDENTITY);
                        }
                        AnimationEffect::Spin { degrees } => {
                            // A whole turn ends where it started.
                            assert_eq!(degrees, 360);
                            assert!(close(point(after.matrix, 400.0, 200.0), (400.0, 200.0)));
                        }
                        _ => assert_eq!(after, Pose::REST, "{e:?}"),
                    }
                }
            }
            // Running, every effect shows its target at least somewhere in the run (Appear and
            // Disappear switch at the start).
            let mid = pose(&e, Stage::Running(0.5), &t);
            match e {
                AnimationEffect::Disappear => assert!(!mid.shows()),
                _ => assert!(mid.shows(), "{e:?} at 50 %"),
            }
        }
    }

    #[test]
    fn fade_and_fade_out_are_linear_in_opacity() {
        let t = target();
        assert!(
            (pose(&AnimationEffect::Fade, Stage::Running(0.25), &t).opacity - 0.25).abs() < 1e-6
        );
        assert!(
            (pose(&AnimationEffect::FadeOut, Stage::Running(0.25), &t).opacity - 0.75).abs() < 1e-6
        );
    }

    #[test]
    fn fly_in_starts_just_outside_the_slide_and_fly_out_ends_there() {
        let t = target();
        let from = |d| pose(&AnimationEffect::FlyIn { from: d }, Stage::Running(0.0), &t).matrix;
        // From the bottom: the shape's top is at the slide's bottom edge.
        assert!(close(
            point(from(Direction8::Down), 400.0, 200.0),
            (400.0, 720.0)
        ));
        // From the left: its right edge at the slide's left edge.
        assert!(close(
            point(from(Direction8::Left), 600.0, 250.0),
            (0.0, 250.0)
        ));
        assert!(close(
            point(from(Direction8::RightUp), 400.0, 300.0),
            (1280.0, 0.0)
        ));
        let half = pose(
            &AnimationEffect::FlyIn {
                from: Direction8::Down,
            },
            Stage::Running(0.5),
            &t,
        );
        assert!(close(point(half.matrix, 400.0, 200.0), (400.0, 460.0)));
        let out = pose(
            &AnimationEffect::FlyOut {
                to: Direction8::Right,
            },
            Stage::Running(1.0),
            &t,
        );
        assert!(close(point(out.matrix, 400.0, 200.0), (1280.0, 200.0)));
    }

    #[test]
    fn float_in_rises_a_tenth_of_the_slide_while_fading_in() {
        let t = target();
        let start = pose(
            &AnimationEffect::FloatIn { down: false },
            Stage::Running(0.0),
            &t,
        );
        assert!(close(point(start.matrix, 0.0, 0.0), (0.0, 72.0)));
        assert!(start.opacity.abs() < 1e-6);
        let down = pose(
            &AnimationEffect::FloatIn { down: true },
            Stage::Running(0.0),
            &t,
        );
        assert!(close(point(down.matrix, 0.0, 0.0), (0.0, -72.0)));
        let late = pose(
            &AnimationEffect::FloatIn { down: false },
            Stage::Running(0.9),
            &t,
        );
        assert!(point(late.matrix, 0.0, 0.0).1 < 2.0);
    }

    #[test]
    fn split_and_wipe_clip_the_target() {
        let t = target();
        let inside =
            |p: &Pose, x: f32, y: f32| p.clip.as_ref().is_some_and(|c| path_contains(c, x, y));
        let out = pose(
            &AnimationEffect::Split {
                vertical: true,
                inward: false,
            },
            Stage::Running(0.5),
            &t,
        );
        assert!(inside(&out, 500.0, 250.0) && !inside(&out, 405.0, 250.0));
        let inward = pose(
            &AnimationEffect::Split {
                vertical: false,
                inward: true,
            },
            Stage::Running(0.5),
            &t,
        );
        assert!(inside(&inward, 500.0, 200.0) && !inside(&inward, 500.0, 250.0));
        let wipe = pose(
            &AnimationEffect::Wipe { from: Side::Down },
            Stage::Running(0.3),
            &t,
        );
        assert!(inside(&wipe, 500.0, 295.0) && !inside(&wipe, 500.0, 205.0));
        let left = pose(
            &AnimationEffect::Wipe { from: Side::Left },
            Stage::Running(0.3),
            &t,
        );
        assert!(inside(&left, 410.0, 250.0) && !inside(&left, 590.0, 250.0));
    }

    #[test]
    fn zoom_and_grow_turn_scale_from_the_centre() {
        let t = target();
        let z = pose(&AnimationEffect::Zoom, Stage::Running(0.5), &t);
        assert!(close(point(z.matrix, 500.0, 250.0), (500.0, 250.0)));
        assert!(close(point(z.matrix, 400.0, 200.0), (450.0, 225.0)));
        assert!((z.opacity - 0.5).abs() < 1e-6);
        let zo = pose(&AnimationEffect::ZoomOut, Stage::Running(0.75), &t);
        assert!(close(point(zo.matrix, 400.0, 200.0), (475.0, 237.5)));
        let g = pose(&AnimationEffect::GrowTurn, Stage::Running(0.0), &t);
        assert!(!g.shows());
        let g2 = pose(&AnimationEffect::GrowTurn, Stage::Running(0.5), &t);
        // Half size, turned by -45°.
        let p = point(g2.matrix, 600.0, 250.0);
        assert!(close(
            p,
            (
                500.0 + 50.0 * (45f64).to_radians().cos(),
                250.0 - 50.0 * (45f64).to_radians().sin()
            )
        ));
    }

    #[test]
    fn emphasis_effects_move_and_return() {
        let t = target();
        let pulse = pose(&AnimationEffect::Pulse, Stage::Running(0.5), &t);
        assert!(close(point(pulse.matrix, 400.0, 200.0), (395.0, 197.5)));
        let teeter = pose(&AnimationEffect::Teeter, Stage::Running(0.4), &t);
        assert!(teeter.matrix != Affine::IDENTITY);
        assert_eq!(
            pose(&AnimationEffect::Teeter, Stage::Running(1.0), &t).matrix,
            Affine::IDENTITY
        );
        let spin = pose(
            &AnimationEffect::Spin { degrees: 180 },
            Stage::Running(0.5),
            &t,
        );
        // A quarter turn at half time (eased symmetrically).
        assert!(close(point(spin.matrix, 600.0, 250.0), (500.0, 350.0)));
        let grow = pose(
            &AnimationEffect::GrowShrink { scale_pct: 150 },
            Stage::After,
            &t,
        );
        assert!(close(point(grow.matrix, 400.0, 200.0), (350.0, 175.0)));
        let colour = pose(
            &AnimationEffect::ColorPulse { color: None },
            Stage::Running(0.5),
            &t,
        );
        assert!(
            colour
                .tint
                .as_ref()
                .is_some_and(|(_, a)| (*a - 1.0).abs() < 1e-6)
        );
        assert!(
            pose(
                &AnimationEffect::ColorPulse { color: None },
                Stage::After,
                &t
            )
            .tint
            .is_none()
        );
    }

    #[test]
    fn effects_fold_in_order() {
        let t = target();
        let fade = AnimationEffect::Fade;
        let spin = AnimationEffect::Spin { degrees: 90 };
        let out = AnimationEffect::FadeOut;
        // Entered and spun: visible and turned.
        let p = fold(
            [
                (&fade, Stage::After),
                (&spin, Stage::After),
                (&out, Stage::Before),
            ],
            &t,
        );
        assert!(p.shows() && p.matrix != Affine::IDENTITY);
        // Then exited.
        let gone = fold(
            [
                (&fade, Stage::After),
                (&spin, Stage::After),
                (&out, Stage::After),
            ],
            &t,
        );
        assert!(!gone.shows());
        // An exit before any entrance: visible until it runs.
        let first_exit = fold([(&out, Stage::Before), (&fade, Stage::Before)], &t);
        assert!(first_exit.shows());
        // Kept effects Kiln does not play: entrances appear, others do nothing.
        let other = AnimationEffect::Other {
            class: EffectClass::Entrance,
            preset_id: 3,
            subtype: 0,
            xml: None,
        };
        assert!(!pose(&other, Stage::Before, &t).shows());
        assert!(pose(&other, Stage::Running(0.1), &t).shows());
        assert!(starts_hidden([&other]));
        assert!(!starts_hidden(std::iter::empty::<&AnimationEffect>()));
    }
}
