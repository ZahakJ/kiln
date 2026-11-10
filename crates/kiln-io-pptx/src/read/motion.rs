// SPDX-License-Identifier: AGPL-3.0-or-later
//! `p:transition` → [`Transition`] (mapped to Kiln's twelve, the original kept when the mapping
//! is not exact) and `p:timing` → the animation [`Timeline`] (main sequence and trigger
//! sequences; effects Kiln does not play keep their `p:par` XML).
//!
//! PowerPoint's timing tree is `tmRoot` → `p:seq` (`mainSeq` or `interactiveSeq`) → one `p:par`
//! per click → one `p:par` per with/after group → one `p:par` per effect, whose `p:cTn` carries
//! `presetClass`/`presetID`/`presetSubtype`/`nodeType` and whose behaviours (`p:set`,
//! `p:animEffect`, `p:anim`, `p:animRot`, `p:animScale`, `p:animClr`, …) carry the target and
//! the durations. The reader walks it tolerantly: an effect is any `p:cTn` with a preset class
//! or an effect node type, wherever it sits.

use std::collections::HashMap;

use kiln_common::NodeId;
use kiln_deck::animation::{
    AnimationEffect, Effect, EffectClass, EffectStart, EffectTarget, Timeline, TriggerSequence,
};
use kiln_deck::transition::{Transition, TransitionEffect};
use kiln_ooxml::drawingml::color::parse_color_child;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use crate::common::kind::SUPPORTED_MC;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{attr_u32, fragment, local};

/// Milliseconds from a time attribute (`dur`, `delay`); `None` for `indefinite` or garbage.
fn ms(e: &XmlElement, name: &str) -> Option<u32> {
    let v = e.attr(name)?;
    let n: i64 = v.trim().parse().ok()?;
    u32::try_from(n.max(0)).ok()
}

/// The `p:transition` of a slide root, looking inside `mc:AlternateContent`: the element and
/// the XML the extras keep (the whole block when it was wrapped).
pub(crate) fn find_transition(root: &XmlElement) -> Option<(&XmlElement, &XmlElement)> {
    for c in root.elements() {
        if c.name == "p:transition" {
            return Some((c, c));
        }
        if c.name == "mc:AlternateContent" {
            let chosen = c.select_alternate_content(&SUPPORTED_MC);
            if let Some(t) = chosen.into_iter().find(|e| e.name == "p:transition") {
                return Some((t, c));
            }
        }
    }
    None
}

impl Reader<'_> {
    /// Reads a slide's transition.
    pub fn read_transition(
        &mut self,
        part: &str,
        t: &XmlElement,
        kept: &XmlElement,
        slide: NodeId,
        scope: &kiln_ooxml::xml::Attrs,
    ) -> Transition {
        self.extras.transitions.insert(slide, fragment(kept, scope));
        let child = t
            .elements()
            .find(|c| !matches!(local(c), "sndAc" | "extLst"));
        let (effect, exact) = child.map_or((TransitionEffect::None, true), |c| {
            TransitionEffect::from_element(c)
        });
        let duration_ms = ms(t, "p14:dur")
            .or_else(|| t.attr("spd").map(Transition::duration_from_speed))
            .unwrap_or_else(|| effect.default_duration_ms());
        let original = match child {
            Some(c) if !exact => {
                self.warn(
                    WarningCode::TransitionMapped,
                    Severity::Info,
                    format!(
                        "the {} transition is shown as {} and kept for saving",
                        c.name,
                        effect.name()
                    ),
                    part,
                    "",
                );
                Some(fragment(c, scope))
            }
            _ => None,
        };
        Transition {
            effect,
            duration_ms,
            advance_on_click: t.attr_bool("advClick").unwrap_or(true),
            advance_after_ms: ms(t, "advTm"),
            original,
        }
    }

    /// Reads a slide's animations. `ids` maps the slide's `p:cNvPr/@id`s to shape ids.
    pub fn read_timing(
        &mut self,
        part: &str,
        timing: &XmlElement,
        slide: NodeId,
        ids: &HashMap<u32, NodeId>,
        scope: &kiln_ooxml::xml::Attrs,
    ) -> Timeline {
        self.extras.timing.insert(slide, fragment(timing, scope));
        let mut timeline = Timeline::default();
        let mut seqs = Vec::new();
        collect_named(timing, "p:seq", &mut seqs);
        for seq in seqs {
            let Some(ctn) = seq.child("p:cTn") else {
                continue;
            };
            let mut effects = Vec::new();
            let mut pars = Vec::new();
            collect_effects(ctn, &mut pars);
            for par in pars {
                if let Some(e) = self.read_effect(part, par, ids, scope) {
                    effects.push(e);
                }
            }
            match ctn.attr("nodeType") {
                Some("interactiveSeq") => {
                    let trigger = ctn
                        .child("p:stCondLst")
                        .into_iter()
                        .flat_map(|l| l.children_named("p:cond"))
                        .find_map(|c| {
                            c.child_path(&["p:tgtEl", "p:spTgt"])
                                .and_then(|t| attr_u32(t, "spid"))
                        })
                        .and_then(|spid| ids.get(&spid).copied());
                    match trigger {
                        Some(trigger) => {
                            timeline.triggers.push(TriggerSequence { trigger, effects })
                        }
                        None if !effects.is_empty() => self.warn(
                            WarningCode::TimingUnreadable,
                            Severity::Warning,
                            "a trigger animation without a trigger shape is kept for saving only",
                            part,
                            "",
                        ),
                        None => {}
                    }
                }
                _ => timeline.sequence.extend(effects),
            }
        }
        timeline
    }

    fn read_effect(
        &mut self,
        part: &str,
        par: &XmlElement,
        ids: &HashMap<u32, NodeId>,
        scope: &kiln_ooxml::xml::Attrs,
    ) -> Option<Effect> {
        let ctn = par.child("p:cTn")?;
        let target_el = descendants_local(par, "spTgt").into_iter().next();
        let target = target_el.and_then(|t| {
            let shape = ids.get(&attr_u32(t, "spid")?)?;
            let paragraphs = t.child_path(&["p:txEl", "p:pRg"]).map(|r| {
                (
                    attr_u32(r, "st").unwrap_or(0),
                    attr_u32(r, "end").unwrap_or(0),
                )
            });
            Some(EffectTarget {
                shape: *shape,
                paragraphs,
            })
        });
        let Some(target) = target else {
            self.warn(
                WarningCode::TimingUnreadable,
                Severity::Warning,
                "an animation of a shape that is not on the slide is kept for saving only",
                part,
                "",
            );
            return None;
        };
        let class = EffectClass::parse(ctn.attr("presetClass").unwrap_or("entr"));
        let preset = attr_u32(ctn, "presetID").unwrap_or(0);
        let subtype = attr_u32(ctn, "presetSubtype").unwrap_or(0);
        let mut effect = AnimationEffect::from_ooxml_preset(class, preset, subtype);
        refine(&mut effect, par);
        if let AnimationEffect::Other { xml, .. } = &mut effect {
            *xml = Some(fragment(par, scope));
            self.warn(
                WarningCode::AnimationUnsupported,
                Severity::Info,
                format!(
                    "an animation effect ({} {preset}) is kept for saving but not played",
                    class.as_str()
                ),
                part,
                "",
            );
        }
        let start = match ctn.attr("nodeType") {
            Some("clickEffect") => EffectStart::OnClick,
            Some("afterEffect") => EffectStart::AfterPrevious,
            _ => EffectStart::WithPrevious,
        };
        let delay_ms = ctn
            .child("p:stCondLst")
            .and_then(|l| l.child("p:cond"))
            .and_then(|c| ms(c, "delay"))
            .unwrap_or(0);
        let duration_ms = descendants_local(par, "cBhvr")
            .into_iter()
            .filter_map(|b| b.child("p:cTn"))
            .filter_map(|c| {
                let d = ms(c, "dur")?;
                let delay = c
                    .child("p:stCondLst")
                    .and_then(|l| l.child("p:cond"))
                    .and_then(|c| ms(c, "delay"))
                    .unwrap_or(0);
                Some(d.saturating_add(delay))
            })
            .max()
            .unwrap_or_else(|| effect.default_duration_ms());
        let repeat = ms(ctn, "repeatCount").map_or(1, |r| (r / 1000).max(1));
        Some(Effect {
            id: self.pres.alloc_id(),
            target,
            effect,
            start,
            duration_ms,
            delay_ms,
            repeat,
        })
    }
}

/// Reads the options PowerPoint keeps in the behaviours.
fn refine(effect: &mut AnimationEffect, par: &XmlElement) {
    match effect {
        AnimationEffect::Spin { degrees } => {
            if let Some(by) = descendants_local(par, "animRot")
                .into_iter()
                .find_map(|r| r.attr_int("by"))
            {
                *degrees = i32::try_from(by / 60_000).unwrap_or(360);
            }
        }
        AnimationEffect::GrowShrink { scale_pct } => {
            if let Some(x) = descendants_local(par, "animScale")
                .into_iter()
                .find_map(|s| s.child("p:by"))
                .and_then(|b| b.attr_int("x"))
            {
                *scale_pct = u32::try_from(x / 1000).unwrap_or(150);
            }
        }
        AnimationEffect::ColorPulse { color } => {
            *color = descendants_local(par, "animClr")
                .into_iter()
                .find_map(|c| parse_color_child(c.child("p:to")));
        }
        _ => {}
    }
}

/// Every element named `name` (not looking inside matches), in document order, without
/// recursion.
fn collect_named<'a>(e: &'a XmlElement, name: &str, out: &mut Vec<&'a XmlElement>) {
    let mut stack: Vec<&'a XmlElement> = e.elements().collect();
    stack.reverse();
    while let Some(c) = stack.pop() {
        if c.name == name {
            out.push(c);
        } else {
            let at = stack.len();
            stack.extend(c.elements());
            stack[at..].reverse();
        }
    }
}

/// The `p:par` of every effect under a sequence's `p:cTn`, in order (nested sequences are
/// read on their own), without recursion.
fn collect_effects<'a>(ctn: &'a XmlElement, out: &mut Vec<&'a XmlElement>) {
    let children = |c: &'a XmlElement| -> Vec<&'a XmlElement> {
        c.child("p:childTnLst")
            .map(|l| l.children_named("p:par").collect())
            .unwrap_or_default()
    };
    let mut stack = children(ctn);
    stack.reverse();
    while let Some(par) = stack.pop() {
        let Some(inner) = par.child("p:cTn") else {
            continue;
        };
        let is_effect = inner.attr("presetClass").is_some()
            || matches!(
                inner.attr("nodeType"),
                Some("clickEffect" | "withEffect" | "afterEffect")
            );
        if is_effect {
            out.push(par);
        } else {
            let mut next = children(inner);
            next.reverse();
            stack.extend(next);
        }
    }
}

/// Every descendant with this local name, in document order, without recursion.
pub(crate) fn descendants_local<'a>(e: &'a XmlElement, name: &str) -> Vec<&'a XmlElement> {
    let mut out = Vec::new();
    let mut stack: Vec<&'a XmlElement> = e.elements().collect();
    stack.reverse();
    while let Some(c) = stack.pop() {
        if local(c) == name {
            out.push(c);
        }
        let at = stack.len();
        stack.extend(c.elements());
        stack[at..].reverse();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn walks_keep_document_order() {
        let e = parse_xml_str(r#"<r><a n="1"><a n="2"/></a><b><a n="3"/></b><a n="4"/></r>"#)
            .unwrap()
            .root;
        let order: Vec<_> = descendants_local(&e, "a")
            .iter()
            .map(|a| a.attr("n").unwrap())
            .collect();
        assert_eq!(order, ["1", "2", "3", "4"]);
        let mut top = Vec::new();
        collect_named(&e, "a", &mut top);
        let order: Vec<_> = top.iter().map(|a| a.attr("n").unwrap()).collect();
        assert_eq!(order, ["1", "3", "4"]);
    }

    #[test]
    fn times_parse() {
        let e = parse_xml_str(r#"<x dur="500" delay="indefinite" neg="-5"/>"#)
            .unwrap()
            .root;
        assert_eq!(ms(&e, "dur"), Some(500));
        assert_eq!(ms(&e, "delay"), None);
        assert_eq!(ms(&e, "neg"), Some(0));
        assert_eq!(ms(&e, "nope"), None);
    }
}
