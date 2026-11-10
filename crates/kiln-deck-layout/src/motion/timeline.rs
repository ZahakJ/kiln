// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where a slide's effects stand: [`stages`] places every effect of the main sequence at a
//! [`Playhead`] (the clicks done and the time since the last one), from the schedule of
//! [`Timeline::schedule`] (PowerPoint's timing tree: on click, with previous, after previous,
//! delays and repeats).

use kiln_deck::animation::Timeline;

use super::effects::Stage;

/// A moment of a slide in the show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playhead {
    /// Clicks done on the slide (0 = only the effects that start by themselves).
    pub click: usize,
    /// Milliseconds since that click (for click 0: since the transition ended); `None` once
    /// everything of the click has settled.
    pub ms: Option<u64>,
}

impl Playhead {
    /// The slide as it is after `click` clicks, settled.
    #[must_use]
    pub const fn settled(click: usize) -> Self {
        Self { click, ms: None }
    }

    /// `ms` after `click`.
    #[must_use]
    pub const fn at(click: usize, ms: u64) -> Self {
        Self {
            click,
            ms: Some(ms),
        }
    }
}

/// The stage of every effect of the main sequence at `at` (index = sequence index).
#[must_use]
pub fn stages(timeline: &Timeline, at: Playhead) -> Vec<Stage> {
    let schedule = timeline.schedule();
    let mut out = vec![Stage::Before; timeline.sequence.len()];
    for s in schedule {
        let e = &timeline.sequence[s.index];
        out[s.index] = if s.click < at.click {
            Stage::After
        } else if s.click > at.click {
            Stage::Before
        } else {
            match at.ms {
                None => Stage::After,
                Some(t) if t < s.start_ms => Stage::Before,
                Some(t) if t >= s.end_ms => Stage::After,
                Some(t) => {
                    let dur = u64::from(e.duration_ms.max(1));
                    let into = t - s.start_ms;
                    let iteration = into / dur;
                    if iteration >= u64::from(e.repeat.max(1)) {
                        Stage::After
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        let p = (into % dur) as f32 / dur as f32;
                        Stage::Running(p)
                    }
                }
            }
        };
    }
    out
}

/// When the effects of `click` have all finished, milliseconds after the click (0 when the
/// click has none).
#[must_use]
pub fn click_end_ms(timeline: &Timeline, click: usize) -> u64 {
    timeline
        .schedule()
        .into_iter()
        .filter(|s| s.click == click)
        .map(|s| s.end_ms)
        .max()
        .unwrap_or(0)
}

/// Whether anything of the slide still moves at `at`.
#[must_use]
pub fn running(timeline: &Timeline, at: Playhead) -> bool {
    at.ms.is_some_and(|t| t < click_end_ms(timeline, at.click))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_common::NodeId;
    use kiln_deck::animation::{AnimationEffect, Effect, EffectStart};

    fn timeline() -> Timeline {
        let mk = |id: u64, start, dur, delay, repeat| Effect {
            start,
            duration_ms: dur,
            delay_ms: delay,
            repeat,
            ..Effect::new(NodeId(id), NodeId(100 + id), AnimationEffect::Fade)
        };
        Timeline {
            sequence: vec![
                mk(1, EffectStart::AfterPrevious, 400, 0, 1), // automatic, 0..400
                mk(2, EffectStart::OnClick, 500, 0, 1),       // click 1, 0..500
                mk(3, EffectStart::WithPrevious, 500, 250, 2), // click 1, 250..1250, twice
                mk(4, EffectStart::OnClick, 1000, 0, 1),      // click 2
            ],
            triggers: Vec::new(),
        }
    }

    #[test]
    fn effects_are_placed_by_click_and_time() {
        let t = timeline();
        assert_eq!(
            stages(&t, Playhead::at(0, 200)),
            vec![
                Stage::Running(0.5),
                Stage::Before,
                Stage::Before,
                Stage::Before
            ]
        );
        assert_eq!(
            stages(&t, Playhead::settled(0)),
            vec![Stage::After, Stage::Before, Stage::Before, Stage::Before]
        );
        assert_eq!(
            stages(&t, Playhead::at(1, 100)),
            vec![
                Stage::After,
                Stage::Running(0.2),
                Stage::Before,
                Stage::Before
            ]
        );
        // The second iteration of the repeating effect.
        assert_eq!(
            stages(&t, Playhead::at(1, 1000)),
            vec![
                Stage::After,
                Stage::After,
                Stage::Running(0.5),
                Stage::Before
            ]
        );
        assert_eq!(stages(&t, Playhead::at(1, 1250))[2], Stage::After);
        assert_eq!(
            stages(&t, Playhead::at(2, 0)),
            vec![
                Stage::After,
                Stage::After,
                Stage::After,
                Stage::Running(0.0)
            ]
        );
    }

    #[test]
    fn a_click_runs_until_its_last_effect_ends() {
        let t = timeline();
        assert_eq!(click_end_ms(&t, 0), 400);
        assert_eq!(click_end_ms(&t, 1), 1250);
        assert_eq!(click_end_ms(&t, 3), 0);
        assert!(running(&t, Playhead::at(1, 1249)));
        assert!(!running(&t, Playhead::at(1, 1250)));
        assert!(!running(&t, Playhead::settled(1)));
    }
}
