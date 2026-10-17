// SPDX-License-Identifier: AGPL-3.0-or-later
//! The animation timeline (`p:timing`): the main sequence of effects with their start mode,
//! duration and delay, the effects of the Animations tab (PLAN.md §1.4) with PowerPoint's
//! preset ids, and the schedule a show plays them on. Trigger sequences (P2) are kept.

use kiln_common::NodeId;
use kiln_ooxml::drawingml::color::DrawingColor;
use serde::{Deserialize, Serialize};

use crate::dml::serde_xml::xml_opt;
use crate::transition::{Direction8, Side};

/// The class of an effect (`presetClass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectClass {
    /// `entr`.
    Entrance,
    /// `emph`.
    Emphasis,
    /// `exit`.
    Exit,
    /// `path` (motion paths, P2; kept).
    Path,
    /// Anything else (`verb`, `mediacall`); kept.
    Other,
}

impl EffectClass {
    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entrance => "entr",
            Self::Emphasis => "emph",
            Self::Exit => "exit",
            Self::Path => "path",
            Self::Other => "verb",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "entr" => Self::Entrance,
            "emph" => Self::Emphasis,
            "exit" => Self::Exit,
            "path" => Self::Path,
            _ => Self::Other,
        }
    }
}

/// An animation effect with its options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationEffect {
    /// Entrance: Appear.
    Appear,
    /// Entrance: Fade.
    Fade,
    /// Entrance: Fly In (from a direction).
    FlyIn {
        /// Where it comes from.
        from: Direction8,
    },
    /// Entrance: Float In (up, or down).
    FloatIn {
        /// Floats down instead of up.
        down: bool,
    },
    /// Entrance: Split.
    Split {
        /// Vertical instead of horizontal.
        vertical: bool,
        /// In instead of out.
        inward: bool,
    },
    /// Entrance: Wipe (from a side).
    Wipe {
        /// Where it starts.
        from: Side,
    },
    /// Entrance: Zoom.
    Zoom,
    /// Entrance: Grow & Turn (P1).
    GrowTurn,
    /// Emphasis: Pulse.
    Pulse,
    /// Emphasis: Colour Pulse.
    ColorPulse {
        /// The pulse colour (accent 1 by default).
        #[serde(with = "xml_opt")]
        color: Option<DrawingColor>,
    },
    /// Emphasis: Teeter.
    Teeter,
    /// Emphasis: Spin.
    Spin {
        /// Degrees, positive clockwise.
        degrees: i32,
    },
    /// Emphasis: Grow/Shrink.
    GrowShrink {
        /// Size in percent (150 grows by half).
        scale_pct: u32,
    },
    /// Exit: Disappear.
    Disappear,
    /// Exit: Fade.
    FadeOut,
    /// Exit: Fly Out (towards a direction).
    FlyOut {
        /// Where it goes.
        to: Direction8,
    },
    /// Exit: Zoom.
    ZoomOut,
    /// An effect Kiln does not play yet, kept from a file (played as Appear/Disappear/nothing).
    Other {
        /// `presetClass`.
        class: EffectClass,
        /// `presetID`.
        preset_id: u32,
        /// `presetSubtype`.
        subtype: u32,
        /// The original `p:par` XML of the effect, written back on save.
        xml: Option<String>,
    },
}

impl AnimationEffect {
    /// The class.
    #[must_use]
    pub const fn class(&self) -> EffectClass {
        match self {
            Self::Appear
            | Self::Fade
            | Self::FlyIn { .. }
            | Self::FloatIn { .. }
            | Self::Split { .. }
            | Self::Wipe { .. }
            | Self::Zoom
            | Self::GrowTurn => EffectClass::Entrance,
            Self::Pulse
            | Self::ColorPulse { .. }
            | Self::Teeter
            | Self::Spin { .. }
            | Self::GrowShrink { .. } => EffectClass::Emphasis,
            Self::Disappear | Self::FadeOut | Self::FlyOut { .. } | Self::ZoomOut => {
                EffectClass::Exit
            }
            Self::Other { class, .. } => *class,
        }
    }

    /// The English name in the gallery.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Appear => "Appear",
            Self::Fade | Self::FadeOut => "Fade",
            Self::FlyIn { .. } => "Fly In",
            Self::FloatIn { .. } => "Float In",
            Self::Split { .. } => "Split",
            Self::Wipe { .. } => "Wipe",
            Self::Zoom | Self::ZoomOut => "Zoom",
            Self::GrowTurn => "Grow & Turn",
            Self::Pulse => "Pulse",
            Self::ColorPulse { .. } => "Colour Pulse",
            Self::Teeter => "Teeter",
            Self::Spin { .. } => "Spin",
            Self::GrowShrink { .. } => "Grow/Shrink",
            Self::Disappear => "Disappear",
            Self::FlyOut { .. } => "Fly Out",
            Self::Other { .. } => "Custom",
        }
    }

    /// PowerPoint's default duration, milliseconds.
    #[must_use]
    pub const fn default_duration_ms(&self) -> u32 {
        match self {
            Self::Appear | Self::Disappear => 1,
            Self::Fade | Self::FadeOut | Self::FlyIn { .. } | Self::FlyOut { .. } => 500,
            Self::FloatIn { .. }
            | Self::Split { .. }
            | Self::Wipe { .. }
            | Self::Zoom
            | Self::ZoomOut
            | Self::Pulse => 500,
            Self::GrowTurn | Self::ColorPulse { .. } | Self::Teeter | Self::GrowShrink { .. } => {
                1000
            }
            Self::Spin { .. } => 2000,
            Self::Other { .. } => 500,
        }
    }

    /// `(presetClass, presetID, presetSubtype)` as PowerPoint writes them in `p:cTn`.
    #[must_use]
    pub fn ooxml_preset(&self) -> (EffectClass, u32, u32) {
        use EffectClass::{Emphasis, Entrance, Exit};
        match self {
            Self::Appear => (Entrance, 1, 0),
            Self::FlyIn { from } => (Entrance, 2, dir8_subtype(*from)),
            Self::Fade => (Entrance, 10, 0),
            Self::Split { vertical, inward } => (Entrance, 16, split_subtype(*vertical, *inward)),
            Self::Wipe { from } => (Entrance, 22, side_subtype(*from)),
            Self::GrowTurn => (Entrance, 31, 0),
            Self::FloatIn { down } => (Entrance, if *down { 47 } else { 42 }, 0),
            Self::Zoom => (Entrance, 53, 16),
            Self::GrowShrink { .. } => (Emphasis, 6, 0),
            Self::Spin { .. } => (Emphasis, 8, 0),
            Self::Pulse => (Emphasis, 26, 0),
            Self::ColorPulse { .. } => (Emphasis, 27, 0),
            Self::Teeter => (Emphasis, 32, 0),
            Self::Disappear => (Exit, 1, 0),
            Self::FlyOut { to } => (Exit, 2, dir8_subtype(*to)),
            Self::FadeOut => (Exit, 10, 0),
            Self::ZoomOut => (Exit, 53, 16),
            Self::Other {
                class,
                preset_id,
                subtype,
                ..
            } => (*class, *preset_id, *subtype),
        }
    }

    /// The effect for a `(presetClass, presetID, presetSubtype)` triple read from a file.
    /// Options PowerPoint keeps in the behaviours (spin angle, grow scale, pulse colour) take
    /// their defaults; the reader overrides them when it parses the behaviours.
    #[must_use]
    pub fn from_ooxml_preset(class: EffectClass, id: u32, subtype: u32) -> AnimationEffect {
        use EffectClass::{Emphasis, Entrance, Exit};
        match (class, id) {
            (Entrance, 1) => Self::Appear,
            (Entrance, 2) => Self::FlyIn {
                from: subtype_dir8(subtype),
            },
            (Entrance, 10) => Self::Fade,
            (Entrance, 16) => {
                let (vertical, inward) = subtype_split(subtype);
                Self::Split { vertical, inward }
            }
            (Entrance, 22) => Self::Wipe {
                from: subtype_side(subtype),
            },
            (Entrance, 31) => Self::GrowTurn,
            (Entrance, 42) => Self::FloatIn { down: false },
            (Entrance, 47) => Self::FloatIn { down: true },
            (Entrance, 53) => Self::Zoom,
            (Emphasis, 6) => Self::GrowShrink { scale_pct: 150 },
            (Emphasis, 8) => Self::Spin { degrees: 360 },
            (Emphasis, 26) => Self::Pulse,
            (Emphasis, 27) => Self::ColorPulse { color: None },
            (Emphasis, 32) => Self::Teeter,
            (Exit, 1) => Self::Disappear,
            (Exit, 2) => Self::FlyOut {
                to: subtype_dir8(subtype),
            },
            (Exit, 10) => Self::FadeOut,
            (Exit, 53) => Self::ZoomOut,
            _ => Self::Other {
                class,
                preset_id: id,
                subtype,
                xml: None,
            },
        }
    }
}

/// The Animations gallery, in PowerPoint's order.
#[must_use]
pub fn gallery() -> Vec<AnimationEffect> {
    vec![
        AnimationEffect::Appear,
        AnimationEffect::Fade,
        AnimationEffect::FlyIn {
            from: Direction8::Down,
        },
        AnimationEffect::FloatIn { down: false },
        AnimationEffect::Split {
            vertical: false,
            inward: true,
        },
        AnimationEffect::Wipe { from: Side::Down },
        AnimationEffect::Zoom,
        AnimationEffect::GrowTurn,
        AnimationEffect::Pulse,
        AnimationEffect::ColorPulse { color: None },
        AnimationEffect::Teeter,
        AnimationEffect::Spin { degrees: 360 },
        AnimationEffect::GrowShrink { scale_pct: 150 },
        AnimationEffect::Disappear,
        AnimationEffect::FadeOut,
        AnimationEffect::FlyOut {
            to: Direction8::Down,
        },
        AnimationEffect::ZoomOut,
    ]
}

// PowerPoint's direction subtypes: bit flags top 1, right 2, bottom 4, left 8.
fn dir8_subtype(d: Direction8) -> u32 {
    match d {
        Direction8::Up => 1,
        Direction8::Right => 2,
        Direction8::Down => 4,
        Direction8::Left => 8,
        Direction8::RightUp => 3,
        Direction8::RightDown => 6,
        Direction8::LeftUp => 9,
        Direction8::LeftDown => 12,
    }
}

fn subtype_dir8(s: u32) -> Direction8 {
    match s {
        1 => Direction8::Up,
        2 => Direction8::Right,
        8 => Direction8::Left,
        3 => Direction8::RightUp,
        6 => Direction8::RightDown,
        9 => Direction8::LeftUp,
        12 => Direction8::LeftDown,
        _ => Direction8::Down,
    }
}

fn side_subtype(s: Side) -> u32 {
    match s {
        Side::Up => 1,
        Side::Right => 2,
        Side::Down => 4,
        Side::Left => 8,
    }
}

fn subtype_side(s: u32) -> Side {
    match s {
        1 => Side::Up,
        2 => Side::Right,
        8 => Side::Left,
        _ => Side::Down,
    }
}

// Split: horizontal in 42, horizontal out 26, vertical in 21, vertical out 37 (the values
// PowerPoint writes and LibreOffice's import tables use).
fn split_subtype(vertical: bool, inward: bool) -> u32 {
    match (vertical, inward) {
        (false, true) => 42,
        (false, false) => 26,
        (true, true) => 21,
        (true, false) => 37,
    }
}

fn subtype_split(s: u32) -> (bool, bool) {
    match s {
        26 => (false, false),
        21 => (true, true),
        37 => (true, false),
        _ => (false, true),
    }
}

/// When an effect starts (`nodeType` of its `p:cTn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EffectStart {
    /// `clickEffect`: on the next click.
    #[default]
    OnClick,
    /// `withEffect`: together with the previous effect.
    WithPrevious,
    /// `afterEffect`: when the previous effects finish.
    AfterPrevious,
}

/// What an effect animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectTarget {
    /// The shape (on the same slide).
    pub shape: NodeId,
    /// A paragraph range `[first, last]` when the text is built by paragraph (`p:txEl/p:pRg`).
    pub paragraphs: Option<(u32, u32)>,
}

impl EffectTarget {
    /// The whole shape.
    #[must_use]
    pub const fn shape(shape: NodeId) -> Self {
        Self {
            shape,
            paragraphs: None,
        }
    }
}

/// One effect in a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effect {
    /// Stable id (unique in the presentation).
    pub id: NodeId,
    /// What it animates.
    pub target: EffectTarget,
    /// The effect.
    pub effect: AnimationEffect,
    /// When it starts.
    pub start: EffectStart,
    /// Duration of one iteration, milliseconds.
    pub duration_ms: u32,
    /// Delay after its start point, milliseconds.
    pub delay_ms: u32,
    /// Iterations (≥ 1).
    pub repeat: u32,
}

impl Effect {
    /// An effect on a shape with PowerPoint's defaults (on click, default duration, no delay).
    #[must_use]
    pub fn new(id: NodeId, shape: NodeId, effect: AnimationEffect) -> Self {
        Self {
            id,
            target: EffectTarget::shape(shape),
            duration_ms: effect.default_duration_ms(),
            effect,
            start: EffectStart::OnClick,
            delay_ms: 0,
            repeat: 1,
        }
    }

    /// Total time from its start point to its end, milliseconds.
    #[must_use]
    pub fn total_ms(&self) -> u64 {
        u64::from(self.delay_ms).saturating_add(
            u64::from(self.duration_ms).saturating_mul(u64::from(self.repeat.max(1))),
        )
    }
}

/// An interactive sequence started by clicking a shape (`p:seq concurrent="1"` with a trigger;
/// P2: kept and listed, not played yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerSequence {
    /// The shape whose click starts the sequence.
    pub trigger: NodeId,
    /// The effects.
    pub effects: Vec<Effect>,
}

/// A slide's animations.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Timeline {
    /// The main sequence, in order (the Animation Pane's list).
    pub sequence: Vec<Effect>,
    /// Trigger sequences (P2).
    pub triggers: Vec<TriggerSequence>,
}

/// Where an effect sits in the show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scheduled {
    /// Index into [`Timeline::sequence`].
    pub index: usize,
    /// The click that starts its group: 0 runs automatically after the transition, `n` on the
    /// n-th click.
    pub click: usize,
    /// Start, milliseconds after its click.
    pub start_ms: u64,
    /// End, milliseconds after its click.
    pub end_ms: u64,
}

impl Timeline {
    /// True when there are no effects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty() && self.triggers.is_empty()
    }

    /// Number of clicks the main sequence needs.
    #[must_use]
    pub fn clicks(&self) -> usize {
        self.sequence
            .iter()
            .filter(|e| e.start == EffectStart::OnClick)
            .count()
    }

    /// The schedule: per effect, its click and its start and end within the click. "With
    /// previous" starts with the previous effect's group; "after previous" starts when everything
    /// before it in the click has finished (PowerPoint's timing tree).
    #[must_use]
    pub fn schedule(&self) -> Vec<Scheduled> {
        let mut out = Vec::with_capacity(self.sequence.len());
        let mut click = 0usize;
        let mut group_start = 0u64;
        let mut click_end = 0u64;
        for (index, e) in self.sequence.iter().enumerate() {
            match e.start {
                EffectStart::OnClick => {
                    click += 1;
                    group_start = 0;
                    click_end = 0;
                }
                EffectStart::WithPrevious => {}
                EffectStart::AfterPrevious => group_start = click_end,
            }
            let start = group_start.saturating_add(u64::from(e.delay_ms));
            let end = group_start.saturating_add(e.total_ms());
            click_end = click_end.max(end);
            out.push(Scheduled {
                index,
                click,
                start_ms: start,
                end_ms: end,
            });
        }
        out
    }

    /// The effect numbers the stage shows next to animated shapes (the click each effect runs on;
    /// 0 for effects that run automatically).
    #[must_use]
    pub fn badges(&self) -> Vec<(NodeId, usize)> {
        self.schedule()
            .into_iter()
            .map(|s| (self.sequence[s.index].target.shape, s.click))
            .collect()
    }

    /// Every effect id (main sequence and triggers).
    pub fn effect_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.sequence
            .iter()
            .chain(self.triggers.iter().flat_map(|t| t.effects.iter()))
            .map(|e| e.id)
    }

    /// Every shape the timeline refers to (targets and triggers).
    pub fn shape_refs(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.sequence
            .iter()
            .chain(self.triggers.iter().flat_map(|t| t.effects.iter()))
            .map(|e| e.target.shape)
            .chain(self.triggers.iter().map(|t| t.trigger))
    }

    /// The index of an effect in the main sequence.
    #[must_use]
    pub fn position(&self, id: NodeId) -> Option<usize> {
        self.sequence.iter().position(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_through_powerpoint_ids() {
        for e in gallery() {
            let (class, id, sub) = e.ooxml_preset();
            assert_eq!(class, e.class());
            assert_eq!(
                AnimationEffect::from_ooxml_preset(class, id, sub),
                e,
                "{e:?}"
            );
        }
        for d in [
            Direction8::Left,
            Direction8::Right,
            Direction8::Up,
            Direction8::Down,
            Direction8::LeftUp,
            Direction8::RightUp,
            Direction8::LeftDown,
            Direction8::RightDown,
        ] {
            let e = AnimationEffect::FlyIn { from: d };
            let (c, i, s) = e.ooxml_preset();
            assert_eq!(AnimationEffect::from_ooxml_preset(c, i, s), e);
        }
        for s in [Side::Left, Side::Right, Side::Up, Side::Down] {
            let e = AnimationEffect::Wipe { from: s };
            let (c, i, st) = e.ooxml_preset();
            assert_eq!(AnimationEffect::from_ooxml_preset(c, i, st), e);
        }
        for (v, n) in [(false, false), (false, true), (true, false), (true, true)] {
            let e = AnimationEffect::Split {
                vertical: v,
                inward: n,
            };
            let (c, i, st) = e.ooxml_preset();
            assert_eq!(AnimationEffect::from_ooxml_preset(c, i, st), e);
        }
        let other = AnimationEffect::from_ooxml_preset(EffectClass::Entrance, 3, 10);
        assert_eq!(other.class(), EffectClass::Entrance);
        assert_eq!(other.ooxml_preset(), (EffectClass::Entrance, 3, 10));
        assert_eq!(EffectClass::parse("path"), EffectClass::Path);
        assert_eq!(
            EffectClass::parse(EffectClass::Exit.as_str()),
            EffectClass::Exit
        );
        assert_eq!(EffectClass::parse("mediacall"), EffectClass::Other);
    }

    #[test]
    fn the_gallery_covers_the_animations_tab() {
        let g = gallery();
        let count = |c| g.iter().filter(|e| e.class() == c).count();
        assert_eq!(count(EffectClass::Entrance), 8);
        assert_eq!(count(EffectClass::Emphasis), 5);
        assert_eq!(count(EffectClass::Exit), 4);
        assert!(
            g.iter()
                .all(|e| !e.name().is_empty() && e.default_duration_ms() > 0)
        );
    }

    #[test]
    fn schedule_follows_start_modes() {
        let mk = |id, start, dur, delay| Effect {
            start,
            duration_ms: dur,
            delay_ms: delay,
            ..Effect::new(NodeId(id), NodeId(100 + id), AnimationEffect::Fade)
        };
        let t = Timeline {
            sequence: vec![
                mk(1, EffectStart::WithPrevious, 500, 0),   // automatic
                mk(2, EffectStart::OnClick, 500, 0),        // click 1 at 0
                mk(3, EffectStart::WithPrevious, 800, 100), // with it, 100..900
                mk(4, EffectStart::AfterPrevious, 500, 0),  // after both: 900..1400
                mk(5, EffectStart::OnClick, 1000, 250),     // click 2
            ],
            triggers: Vec::new(),
        };
        let s = t.schedule();
        assert_eq!(t.clicks(), 2);
        let tuple: Vec<_> = s.iter().map(|x| (x.click, x.start_ms, x.end_ms)).collect();
        assert_eq!(
            tuple,
            vec![
                (0, 0, 500),
                (1, 0, 500),
                (1, 100, 900),
                (1, 900, 1400),
                (2, 250, 1250)
            ]
        );
        assert_eq!(t.badges()[3], (NodeId(104), 1));
        assert_eq!(t.position(NodeId(3)), Some(2));
        assert_eq!(t.effect_ids().count(), 5);
        assert!(!t.is_empty());
        assert!(Timeline::default().is_empty());
    }

    #[test]
    fn repeats_extend_effects() {
        let mut e = Effect::new(NodeId(1), NodeId(2), AnimationEffect::Pulse);
        e.repeat = 3;
        e.delay_ms = 10;
        assert_eq!(e.total_ms(), 10 + 3 * 500);
    }
}
