// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Transitions and Animations tabs: Transition to This Slide with its Effect
//! Options, Duration, Advance Slide and Apply To All; the Animation gallery, Effect Options
//! (Sequence included: By Paragraph splits a text shape's effect per paragraph), Add Animation,
//! Start, Duration, Delay and Repeat, the Animation Pane (select, reorder, remove), the Animation
//! Painter and the effect numbers on the stage. Every change is a `kiln-deck` transaction (one
//! undo step), saved by the pptx writer as PowerPoint's `p:transition` and `p:timing`.

pub(crate) mod designer;
pub mod export;
mod media;
mod outline;
pub(crate) mod reuse;
pub(crate) mod vocab;

#[cfg(test)]
mod content_tests;
#[cfg(test)]
mod tests;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::animation::{AnimationEffect, Effect, EffectStart, EffectTarget};
use kiln_deck::edit;
use kiln_deck::ops::{EditKind, Op, Transaction};
use kiln_deck::transition::{Transition, TransitionEffect};
use kiln_protocol::commands::DeckCommand;
use kiln_protocol::commands::deck_show::{
    DeckAnimationInfo, DeckChoice, DeckEffectStart, DeckMotionState, DeckTransitionInfo,
};

use super::DeckDoc;
use crate::model::Change;

/// The protocol's start of an effect.
pub(crate) fn start_of(s: EffectStart) -> DeckEffectStart {
    match s {
        EffectStart::OnClick => DeckEffectStart::OnClick,
        EffectStart::WithPrevious => DeckEffectStart::WithPrevious,
        EffectStart::AfterPrevious => DeckEffectStart::AfterPrevious,
    }
}

/// The model's start of an effect.
pub(crate) fn start_from(s: DeckEffectStart) -> EffectStart {
    match s {
        DeckEffectStart::OnClick => EffectStart::OnClick,
        DeckEffectStart::WithPrevious => EffectStart::WithPrevious,
        DeckEffectStart::AfterPrevious => EffectStart::AfterPrevious,
    }
}

/// The longest duration and delay the ribbon takes (59.99 s, as PowerPoint).
const MAX_MS: u32 = 59_990;

fn invalid(msg: impl Into<String>) -> KilnError {
    KilnError::InvalidInput(msg.into())
}

/// One transaction of several ops.
fn tx(ops: Vec<Op>, selection: Vec<NodeId>) -> Transaction {
    Transaction::new(ops)
        .with_kind(EditKind::Other)
        .with_selection(Vec::new(), selection)
}

impl DeckDoc {
    /// The effects the Animations tab acts on: the pane's selection, else every effect of the
    /// selected shapes (in sequence order).
    pub(crate) fn selected_effects(&self) -> Vec<NodeId> {
        let Some(slide) = self.current_slide().and_then(|id| self.pres.slide(id)) else {
            return Vec::new();
        };
        let seq = &slide.timeline.sequence;
        if !self.view.effects.is_empty() {
            return seq
                .iter()
                .filter(|e| self.view.effects.contains(&e.id))
                .map(|e| e.id)
                .collect();
        }
        seq.iter()
            .filter(|e| self.view.shapes.contains(&self.top_of(e.target.shape)))
            .map(|e| e.id)
            .collect()
    }

    /// The top-level shape a shape belongs to on its slide.
    pub(crate) fn top_of(&self, shape: NodeId) -> NodeId {
        let Some(loc) = self.pres.locate(shape) else {
            return shape;
        };
        let Some(first) = loc.path.first() else {
            return shape;
        };
        self.pres
            .owner_shapes(loc.owner)
            .and_then(|list| list.get(*first))
            .map_or(shape, |s| s.id)
    }

    /// The current slide's effect with an id.
    fn effect(&self, id: NodeId) -> Option<&Effect> {
        self.current_slide()
            .and_then(|s| self.pres.slide(s))
            .and_then(|s| s.timeline.sequence.iter().find(|e| e.id == id))
    }

    /// Runs a Transitions or Animations command.
    pub(crate) fn motion_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        match cmd {
            D::SetTransition { effect } => self.set_transition(effect),
            D::TransitionOption { option } => self.transition_option(option),
            D::TransitionDuration { ms } => {
                let ms = (*ms).clamp(10, MAX_MS);
                self.edit_transitions("deck.transitions.duration", |t| t.duration_ms = ms)
            }
            D::AdvanceOnClick { on } => {
                let current = self
                    .current_slide()
                    .and_then(|s| self.pres.slide(s))
                    .and_then(|s| s.transition.as_ref())
                    .is_none_or(|t| t.advance_on_click);
                let on = on.unwrap_or(!current);
                self.edit_transitions("deck.transitions.on_click", |t| t.advance_on_click = on)
            }
            D::AdvanceAfter { ms } => {
                let ms = ms.map(|m| m.min(86_399_000));
                self.edit_transitions("deck.transitions.after", |t| t.advance_after_ms = ms)
            }
            D::ApplyTransitionToAll => {
                let id = self.slide_id()?;
                self.apply_with("deck.transitions.apply_to_all", |p| {
                    edit::apply_transition_to_all(p, id)
                })?;
                Ok(Change::Content)
            }
            D::SetAnimation { effect } => self.set_animation(effect),
            D::AddAnimation { effect } => self.add_animation(effect),
            D::AnimationOption { option } => self.animation_option(option),
            D::AnimationStart { start } => {
                let start = start_from(*start);
                self.edit_effects("deck.animations.start", |e| e.start = start)
            }
            D::AnimationDuration { ms } => {
                let ms = (*ms).clamp(1, MAX_MS);
                self.edit_effects("deck.animations.duration", |e| e.duration_ms = ms)
            }
            D::AnimationDelay { ms } => {
                let ms = (*ms).min(MAX_MS);
                self.edit_effects("deck.animations.delay", |e| e.delay_ms = ms)
            }
            D::AnimationRepeat { count } => {
                let n = (*count).clamp(1, 1000);
                self.edit_effects("deck.animations.repeat", |e| e.repeat = n)
            }
            D::RemoveAnimations => {
                let ids = self.selected_effects();
                if ids.is_empty() {
                    return Ok(Change::None);
                }
                self.apply_with("deck.animations.remove", |p| edit::remove_effects(p, &ids))?;
                self.view.effects.clear();
                Ok(Change::Content)
            }
            D::MoveAnimation { id, to } => {
                let to = usize::try_from(*to).unwrap_or(usize::MAX);
                let id = *id;
                self.apply_with("deck.animations.move", |p| edit::move_effect(p, id, to))?;
                self.view.effects = vec![id];
                Ok(Change::Content)
            }
            D::ReorderAnimation { earlier } => self.reorder(*earlier),
            D::SelectAnimations { ids } => {
                let slide = self.current_slide().and_then(|s| self.pres.slide(s));
                let known: Vec<NodeId> = slide
                    .map(|s| {
                        s.timeline
                            .sequence
                            .iter()
                            .filter(|e| ids.contains(&e.id))
                            .map(|e| e.id)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut shapes: Vec<NodeId> = Vec::new();
                for id in &known {
                    if let Some(e) = self.effect(*id) {
                        let top = self.top_of(e.target.shape);
                        if !shapes.contains(&top) {
                            shapes.push(top);
                        }
                    }
                }
                self.end_text();
                self.view.shapes = shapes;
                self.view.rail = false;
                self.view.effects = known;
                Ok(Change::Selection)
            }
            D::CopyAnimation => {
                let first = self
                    .view
                    .shapes
                    .first()
                    .copied()
                    .ok_or_else(|| invalid("select a shape with animations to copy"))?;
                let slide = self.slide_id()?;
                let effects: Vec<Effect> = self
                    .pres
                    .slide(slide)
                    .map(|s| {
                        s.timeline
                            .sequence
                            .iter()
                            .filter(|e| self.top_of(e.target.shape) == first)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                if effects.is_empty() {
                    return Err(invalid("the selected shape has no animations"));
                }
                self.anim_clip = Some(effects);
                Ok(Change::Selection)
            }
            D::PasteAnimation => self.paste_animation(),
            D::MotionBadges { on } => {
                self.view.badges = *on;
                Ok(Change::Selection)
            }
            _ => Err(KilnError::Unsupported(format!(
                "{} in the Animations tab",
                cmd.meta().id
            ))),
        }
    }

    /// The transitions of the selected slides, changed by `f` (slides without one get the
    /// default transition first, so timings can be set without an effect).
    fn edit_transitions(
        &mut self,
        label: &'static str,
        f: impl Fn(&mut Transition),
    ) -> Result<Change> {
        let ids = self.selected_slide_ids();
        let mut ops = Vec::new();
        for id in &ids {
            let Some(s) = self.pres.slide(*id) else {
                continue;
            };
            let mut t = s.transition.clone().unwrap_or_default();
            f(&mut t);
            let t = if t == Transition::default() {
                None
            } else {
                Some(t)
            };
            if t != s.transition {
                ops.push(Op::SetTransition {
                    slide: *id,
                    transition: t,
                });
            }
        }
        if ops.is_empty() {
            return Ok(Change::None);
        }
        self.apply(label, tx(ops, ids))?;
        Ok(Change::Content)
    }

    fn set_transition(&mut self, effect: &str) -> Result<Change> {
        let e = vocab::transition_of(effect)
            .ok_or_else(|| invalid(format!("no transition called {effect}")))?;
        self.edit_transitions("deck.transitions.gallery", move |t| {
            if t.effect != e || t.original.is_some() {
                t.effect = e;
                t.duration_ms = e.default_duration_ms();
                t.original = None;
            }
        })
    }

    fn transition_option(&mut self, option: &str) -> Result<Change> {
        let current = self
            .current_slide()
            .and_then(|s| self.pres.slide(s))
            .and_then(|s| s.transition.as_ref())
            .map_or(TransitionEffect::None, |t| t.effect);
        if vocab::with_transition_option(current, option).is_none() {
            return Err(invalid(format!(
                "{} has no effect option {option}",
                current.name()
            )));
        }
        let option = option.to_owned();
        self.edit_transitions("deck.transitions.effect_options", move |t| {
            if let Some(e) = vocab::with_transition_option(t.effect, &option)
                && e != t.effect
            {
                t.effect = e;
                t.original = None;
            }
        })
    }

    /// The selected effects changed by `f`.
    fn edit_effects(&mut self, label: &'static str, f: impl Fn(&mut Effect)) -> Result<Change> {
        let ids = self.selected_effects();
        if ids.is_empty() {
            return Err(invalid("select an animation first"));
        }
        self.begin_group();
        let mut result = Ok(Change::None);
        for id in ids {
            match self.apply_with(label, |p| edit::update_effect(p, id, &f)) {
                Ok(true) => result = Ok(Change::Content),
                Ok(false) => {}
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.end_group();
        result
    }

    /// The selected shapes on the current slide, or an error naming the command.
    fn animatable_shapes(&self) -> Result<Vec<NodeId>> {
        if self.view.shapes.is_empty() {
            return Err(invalid("select a shape to animate"));
        }
        Ok(self.view.shapes.clone())
    }

    fn set_animation(&mut self, effect: &str) -> Result<Change> {
        if effect == "none" {
            return self.motion_command(&DeckCommand::RemoveAnimations);
        }
        let e = vocab::effect_of(effect)
            .ok_or_else(|| invalid(format!("no animation called {effect}")))?;
        let shapes = self.animatable_shapes()?;
        let selected = self.selected_effects();
        self.begin_group();
        let mut changed = false;
        let mut made = Vec::new();
        let run = (|| -> Result<()> {
            for shape in shapes {
                let mine: Vec<NodeId> = selected
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.effect(*id)
                            .is_some_and(|x| self.top_of(x.target.shape) == shape)
                    })
                    .collect();
                if mine.is_empty() {
                    let before = self.selected_shape_effect_count(shape);
                    changed |= self.apply_with("deck.animations.gallery", |p| {
                        edit::add_effect(p, shape, e.clone(), None)
                    })?;
                    made.extend(self.new_effects_of(shape, before));
                } else {
                    for id in mine {
                        changed |= self.apply_with("deck.animations.gallery", |p| {
                            edit::update_effect(p, id, |x| {
                                if x.effect != e {
                                    x.duration_ms = e.default_duration_ms();
                                    x.effect = e.clone();
                                }
                            })
                        })?;
                        made.push(id);
                    }
                }
            }
            Ok(())
        })();
        self.end_group();
        run?;
        self.view.effects = made;
        Ok(if changed {
            Change::Content
        } else {
            Change::None
        })
    }

    fn selected_shape_effect_count(&self, shape: NodeId) -> usize {
        self.pres
            .slide_of_shape(shape)
            .map_or(0, |s| s.timeline.sequence.len())
    }

    /// Effects of a slide's sequence after index `from` that target `shape`.
    fn new_effects_of(&self, shape: NodeId, from: usize) -> Vec<NodeId> {
        self.pres
            .slide_of_shape(shape)
            .map(|s| {
                s.timeline
                    .sequence
                    .iter()
                    .skip(from)
                    .filter(|e| e.target.shape == shape)
                    .map(|e| e.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn add_animation(&mut self, effect: &str) -> Result<Change> {
        let e = vocab::effect_of(effect)
            .ok_or_else(|| invalid(format!("no animation called {effect}")))?;
        let shapes = self.animatable_shapes()?;
        self.begin_group();
        let mut made = Vec::new();
        let run = (|| -> Result<()> {
            for shape in shapes {
                let before = self.selected_shape_effect_count(shape);
                self.apply_with("deck.animations.add_animation", |p| {
                    edit::add_effect(p, shape, e.clone(), None)
                })?;
                made.extend(self.new_effects_of(shape, before));
            }
            Ok(())
        })();
        self.end_group();
        run?;
        self.view.effects = made;
        Ok(Change::Content)
    }

    fn animation_option(&mut self, option: &str) -> Result<Change> {
        if vocab::SEQUENCE.iter().any(|(id, _)| *id == option) {
            return self.sequence_option(option);
        }
        let ids = self.selected_effects();
        let any = ids
            .iter()
            .filter_map(|id| self.effect(*id))
            .any(|e| vocab::with_effect_option(&e.effect, option).is_some());
        if !any {
            return Err(invalid(format!(
                "the selected animations have no option {option}"
            )));
        }
        let option = option.to_owned();
        self.edit_effects("deck.animations.effect_options", move |e| {
            if let Some(x) = vocab::with_effect_option(&e.effect, &option) {
                e.effect = x;
            }
        })
    }

    /// The paragraphs of a shape's text (0 when it has none).
    fn paragraph_count(&self, shape: NodeId) -> u32 {
        self.pres
            .shape(shape)
            .and_then(|s| s.text())
            .map_or(0, |b| {
                u32::try_from(
                    b.paragraphs()
                        .filter(|p| !p.content.plain_text(true).trim().is_empty())
                        .count(),
                )
                .unwrap_or(0)
            })
    }

    /// Effect Options ▸ Sequence: one effect for the whole shape (As One Object), or one per
    /// paragraph starting together (All at Once) or one click after another (By Paragraph).
    fn sequence_option(&mut self, option: &str) -> Result<Change> {
        let slide = self.slide_id()?;
        let ids = self.selected_effects();
        let Some(first) = ids.first().and_then(|id| self.effect(*id)).cloned() else {
            return Err(invalid("select an animation first"));
        };
        let shape = first.target.shape;
        let paras = self.paragraph_count(shape);
        if paras == 0 {
            return Err(invalid("the shape has no text to build by paragraph"));
        }
        let seq = self
            .pres
            .slide(slide)
            .map(|s| s.timeline.sequence.clone())
            .unwrap_or_default();
        // The run of effects that build this shape with the first effect's kind.
        let start = seq.iter().position(|e| e.id == first.id).unwrap_or(0);
        let mut end = start + 1;
        while end < seq.len()
            && seq[end].target.shape == shape
            && seq[end].target.paragraphs.is_some()
            && seq[end].effect == first.effect
        {
            end += 1;
        }
        let mut ops: Vec<Op> = (start..end)
            .rev()
            .map(|index| Op::RemoveEffect { slide, index })
            .collect();
        let mut made: Vec<Effect> = Vec::new();
        match option {
            "asOneObject" => made.push(Effect {
                target: EffectTarget::shape(shape),
                ..first.clone()
            }),
            _ => {
                for k in 0..paras {
                    let id = if k == 0 {
                        first.id
                    } else {
                        self.pres.alloc_id()
                    };
                    made.push(Effect {
                        id,
                        target: EffectTarget {
                            shape,
                            paragraphs: Some((k, k)),
                        },
                        start: if k == 0 {
                            first.start
                        } else if option == "byParagraph" {
                            EffectStart::OnClick
                        } else {
                            EffectStart::WithPrevious
                        },
                        delay_ms: if k == 0 { first.delay_ms } else { 0 },
                        ..first.clone()
                    });
                }
            }
        }
        let sel: Vec<NodeId> = made.iter().map(|e| e.id).collect();
        for (k, e) in made.into_iter().enumerate() {
            ops.push(Op::InsertEffect {
                slide,
                index: start + k,
                effect: Box::new(e),
            });
        }
        self.apply("deck.animations.effect_options", tx(ops, Vec::new()))?;
        self.view.effects = sel;
        Ok(Change::Content)
    }

    fn reorder(&mut self, earlier: bool) -> Result<Change> {
        let slide = self.slide_id()?;
        let mut ids = self.selected_effects();
        let len = self
            .pres
            .slide(slide)
            .map_or(0, |s| s.timeline.sequence.len());
        if ids.is_empty() || len < 2 {
            return Ok(Change::None);
        }
        if !earlier {
            ids.reverse();
        }
        self.begin_group();
        let mut changed = false;
        let mut result = Ok(());
        for id in &ids {
            let Some(at) = self
                .pres
                .slide(slide)
                .and_then(|s| s.timeline.position(*id))
            else {
                continue;
            };
            let to = if earlier {
                at.checked_sub(1)
            } else {
                (at + 1 < len).then_some(at + 1)
            };
            let Some(to) = to else { break };
            match self.apply_with("deck.animations.reorder", |p| edit::move_effect(p, *id, to)) {
                Ok(c) => changed |= c,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.end_group();
        result?;
        Ok(if changed {
            Change::Content
        } else {
            Change::None
        })
    }

    /// Animation Painter: the picked-up effects replace the selected shapes' effects.
    fn paste_animation(&mut self) -> Result<Change> {
        let clip = self
            .anim_clip
            .clone()
            .ok_or_else(|| invalid("pick up animations with the Animation Painter first"))?;
        let shapes = self.animatable_shapes()?;
        let mut ops = Vec::new();
        let mut made = Vec::new();
        for shape in shapes {
            let Some(slide) = self.pres.slide_of_shape(shape) else {
                continue;
            };
            let slide_id = slide.id;
            let seq = slide.timeline.sequence.clone();
            let paras = self.paragraph_count(shape);
            let mut len = seq.len();
            for (i, e) in seq.iter().enumerate().rev() {
                if self.top_of(e.target.shape) == shape {
                    ops.push(Op::RemoveEffect {
                        slide: slide_id,
                        index: i,
                    });
                    len -= 1;
                }
            }
            for e in &clip {
                let id = self.pres.alloc_id();
                let paragraphs = e.target.paragraphs.filter(|(_, last)| *last < paras);
                ops.push(Op::InsertEffect {
                    slide: slide_id,
                    index: len,
                    effect: Box::new(Effect {
                        id,
                        target: EffectTarget { shape, paragraphs },
                        ..e.clone()
                    }),
                });
                made.push(id);
                len += 1;
            }
        }
        if ops.is_empty() {
            return Ok(Change::None);
        }
        let sel = self.view.shapes.clone();
        self.apply("deck.animations.paste", tx(ops, sel))?;
        self.view.effects = made;
        Ok(Change::Content)
    }

    /// The Transitions and Animations tabs' state for the chrome.
    pub(crate) fn motion_state(&self) -> DeckMotionState {
        let slide = self.current_slide().and_then(|id| self.pres.slide(id));
        let transition = slide.map(|s| {
            let t = s.transition.clone().unwrap_or_default();
            DeckTransitionInfo {
                effect: vocab::transition_id(&t.effect).to_owned(),
                option: vocab::transition_option(&t.effect),
                options: vocab::transition_options(&t.effect),
                duration_ms: t.duration_ms,
                on_click: t.advance_on_click,
                after_ms: t.advance_after_ms,
                kept: t.original.is_some(),
            }
        });
        let mut animations = Vec::new();
        if let Some(s) = slide {
            let schedule = s.timeline.schedule();
            for (e, at) in s.timeline.sequence.iter().zip(schedule) {
                let class = e.effect.class();
                animations.push(DeckAnimationInfo {
                    id: e.id,
                    shape: e.target.shape,
                    shape_name: self
                        .pres
                        .shape(e.target.shape)
                        .map(|x| x.name.clone())
                        .unwrap_or_default(),
                    paragraphs: e.target.paragraphs,
                    effect: vocab::effect_id(&e.effect).to_owned(),
                    name: e.effect.name().to_owned(),
                    class: vocab::class_word(class).to_owned(),
                    option: vocab::effect_option(&e.effect),
                    start: start_of(e.start),
                    duration_ms: e.duration_ms,
                    delay_ms: e.delay_ms,
                    repeat: e.repeat,
                    click: u32::try_from(at.click).unwrap_or(u32::MAX),
                    begin_ms: at.start_ms,
                    end_ms: at.end_ms,
                    playable: !matches!(e.effect, AnimationEffect::Other { .. }),
                });
            }
        }
        let selected = self.selected_effects();
        let first = selected.first().and_then(|id| self.effect(*id));
        let mut options: Vec<DeckChoice> = first
            .map(|e| vocab::effect_options(&e.effect))
            .unwrap_or_default();
        let mut option = first.and_then(|e| vocab::effect_option(&e.effect));
        if let Some(e) = first
            && self.paragraph_count(e.target.shape) > 1
            && e.effect.class() != kiln_deck::animation::EffectClass::Other
        {
            options.extend(vocab::SEQUENCE.iter().map(|(id, label)| DeckChoice {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                group: Some("Sequence".into()),
            }));
            if option.is_none() {
                option = Some(self.sequence_of(e).to_owned());
            }
        }
        DeckMotionState {
            transition,
            animations,
            selected,
            options,
            option,
            painter: self.anim_clip.is_some(),
            badges: self.view.badges,
            settings: self.show_settings.clone(),
            show: self.show_status(),
        }
    }

    /// How a text shape's effect is built, for Effect Options ▸ Sequence.
    fn sequence_of(&self, e: &Effect) -> &'static str {
        if e.target.paragraphs.is_none() {
            return "asOneObject";
        }
        let siblings: Vec<&Effect> = self
            .current_slide()
            .and_then(|s| self.pres.slide(s))
            .map(|s| {
                s.timeline
                    .sequence
                    .iter()
                    .filter(|x| x.target.shape == e.target.shape && x.target.paragraphs.is_some())
                    .collect()
            })
            .unwrap_or_default();
        if siblings
            .iter()
            .skip(1)
            .any(|x| x.start == EffectStart::OnClick)
        {
            "byParagraph"
        } else {
            "allAtOnce"
        }
    }

    /// Whether a slide has a transition or effects (the rail's star).
    pub(crate) fn has_motion(slide: &kiln_deck::Slide) -> bool {
        slide
            .transition
            .as_ref()
            .is_some_and(|t| t.effect != TransitionEffect::None)
            || !slide.timeline.sequence.is_empty()
    }
}
