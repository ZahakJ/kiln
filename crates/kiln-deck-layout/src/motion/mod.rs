// SPDX-License-Identifier: AGPL-3.0-or-later
//! Motion for the slide show: slide transitions and animation effects
//! evaluated at a progress, as plain geometry a compositor applies to Scene tiles.
//!
//! Nothing here draws. A show composes each frame from *plates* (the outgoing slide, the
//! incoming slide, a colour) and, within a slide, from *layers* (runs of shapes that do not
//! move, and every animated shape or paragraph on its own). Each gets an [`Xform`]: a matrix
//! in slide px, an opacity and an optional clip path.
//!
//! - [`transition`]: the twelve transitions of the Transitions tab with their effect options;
//!   [`transition::frame`] gives the plates of a transition at a progress.
//! - [`effects`]: the entrance, emphasis and exit effects of the Animations tab;
//!   [`effects::fold`] gives the pose of a shape (or paragraph range) after its effects.
//! - [`timeline`]: where every effect of a slide's main sequence stands at a click and a time.
//!
//! The progress is always supplied by the caller (the show's clock is injected), so any
//! transition or effect can be evaluated, and tested, at any moment.

pub mod effects;
pub mod timeline;
pub mod transition;

use kiln_scene::PathData;
use kurbo::Affine;

/// Where a plate or layer is drawn: a matrix (slide px to slide px), an opacity and a clip in
/// the layer's own coordinates (before the matrix).
#[derive(Debug, Clone, PartialEq)]
pub struct Xform {
    /// The matrix.
    pub matrix: Affine,
    /// Opacity, 0–1.
    pub opacity: f32,
    /// The clip, in the layer's coordinates.
    pub clip: Option<PathData>,
    /// An opacity ramp across the layer (the soft edge of Wipe and Split).
    pub ramp: Option<Ramp>,
}

/// A linear opacity ramp: along the line from `from` to `to` (slide px), the opacity at each
/// stop's offset (0–1 along the line), held before the first stop and after the last.
#[derive(Debug, Clone, PartialEq)]
pub struct Ramp {
    /// Where offset 0 is.
    pub from: (f32, f32),
    /// Where offset 1 is.
    pub to: (f32, f32),
    /// `(offset, opacity)` in increasing offset.
    pub stops: Vec<(f32, f32)>,
}

impl Ramp {
    /// The opacity at a point.
    #[must_use]
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (self.to.0 - self.from.0, self.to.1 - self.from.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= f32::EPSILON {
            0.0
        } else {
            ((x - self.from.0) * dx + (y - self.from.1) * dy) / len2
        };
        ease::keys(t, &self.stops)
    }
}

impl Default for Xform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Xform {
    /// Drawn as it is.
    pub const IDENTITY: Xform = Xform {
        matrix: Affine::IDENTITY,
        opacity: 1.0,
        clip: None,
        ramp: None,
    };

    /// Translated by `(dx, dy)` slide px.
    #[must_use]
    pub fn translate(dx: f64, dy: f64) -> Self {
        Self {
            matrix: Affine::translate((dx, dy)),
            ..Self::IDENTITY
        }
    }

    /// At an opacity.
    #[must_use]
    pub fn faded(opacity: f32) -> Self {
        Self {
            opacity: opacity.clamp(0.0, 1.0),
            ..Self::IDENTITY
        }
    }

    /// Faded along a ramp.
    #[must_use]
    pub fn ramped(ramp: Ramp) -> Self {
        Self {
            ramp: Some(ramp),
            ..Self::IDENTITY
        }
    }

    /// Clipped to a path.
    #[must_use]
    pub fn clipped(clip: PathData) -> Self {
        Self {
            clip: Some(clip),
            ..Self::IDENTITY
        }
    }

    /// With an opacity.
    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Whether anything of it shows.
    #[must_use]
    pub fn shows(&self) -> bool {
        self.opacity > 1.0 / 512.0 && self.clip.as_ref().is_none_or(|c| !c.is_empty())
    }

    /// The matrix in canvas `transform()` order.
    #[must_use]
    pub fn coeffs(&self) -> [f32; 6] {
        crate::geom::coeffs(self.matrix)
    }
}

/// Easing curves (progress 0–1 in, 0–1 out).
pub mod ease {
    /// `p` clamped to 0–1 (non-finite is 0).
    #[must_use]
    pub fn clamp01(p: f32) -> f32 {
        if p.is_finite() {
            p.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Smooth start and end (the curve PowerPoint's moving transitions follow).
    #[must_use]
    pub fn smooth(p: f32) -> f32 {
        let p = clamp01(p);
        p * p * (3.0 - 2.0 * p)
    }

    /// Decelerating to the end.
    #[must_use]
    pub fn out(p: f32) -> f32 {
        let p = clamp01(p);
        1.0 - (1.0 - p) * (1.0 - p)
    }

    /// Accelerating from the start.
    #[must_use]
    pub fn into(p: f32) -> f32 {
        let p = clamp01(p);
        p * p
    }

    /// Piecewise-linear through `(time, value)` keys sorted by time.
    #[must_use]
    pub fn keys(p: f32, keys: &[(f32, f32)]) -> f32 {
        let p = clamp01(p);
        let Some(first) = keys.first() else {
            return 0.0;
        };
        if p <= first.0 {
            return first.1;
        }
        for w in keys.windows(2) {
            let (a, b) = (w[0], w[1]);
            if p <= b.0 {
                let span = b.0 - a.0;
                if span <= f32::EPSILON {
                    return b.1;
                }
                return a.1 + (b.1 - a.1) * (p - a.0) / span;
            }
        }
        keys.last().map_or(0.0, |k| k.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_curves_start_at_zero_and_end_at_one() {
        for f in [ease::smooth, ease::out, ease::into, ease::clamp01] {
            assert!(f(0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert!((0.0..=1.0).contains(&f(0.5)));
            assert!(f(-3.0).abs() < 1e-6 && (f(9.0) - 1.0).abs() < 1e-6);
        }
        assert!((ease::smooth(0.5) - 0.5).abs() < 1e-6);
        assert!(ease::out(0.5) > 0.5 && ease::into(0.5) < 0.5);
        assert!(ease::clamp01(f32::NAN).abs() < 1e-6);
        let k = [(0.0, 0.0), (0.5, 2.0), (1.0, 0.0)];
        assert!((ease::keys(0.25, &k) - 1.0).abs() < 1e-6);
        assert!((ease::keys(0.75, &k) - 1.0).abs() < 1e-6);
        assert!(ease::keys(0.5, &[]).abs() < 1e-6);
    }

    #[test]
    fn xforms_show_unless_transparent_or_clipped_away() {
        assert!(Xform::IDENTITY.shows());
        assert!(!Xform::faded(0.0).shows());
        assert!(!Xform::clipped(PathData::new()).shows());
        assert_eq!(
            Xform::translate(3.0, 4.0).coeffs(),
            [1.0, 0.0, 0.0, 1.0, 3.0, 4.0]
        );
        assert!((Xform::faded(2.0).opacity - 1.0).abs() < 1e-6);
    }
}
