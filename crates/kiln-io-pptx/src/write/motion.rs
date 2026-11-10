// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`Transition`] → `p:transition` and the animation [`Timeline`] → `p:timing`.
//!
//! What was read is written back verbatim whenever the model still says the same thing: the
//! original `p:transition` (sounds and PowerPoint 2010+ options included) when the transition
//! is unchanged, and the original `p:timing` — with the effects Kiln does not play — when the
//! slide's timeline and the shapes it animates are unchanged. Otherwise both are generated the
//! way PowerPoint writes them: transitions that need PowerPoint 2010 or later (Reveal, Flash,
//! Morph, exact durations) inside `mc:AlternateContent` with a plain fallback, and a timing tree
//! of click, group and effect nodes whose behaviours play each effect.

use std::collections::{BTreeSet, HashMap};

use kiln_common::NodeId;
use kiln_deck::animation::{
    AnimationEffect, Effect, EffectClass, EffectStart, EffectTarget, Timeline,
};
use kiln_deck::transition::{Direction8, Side, Transition, TransitionEffect};
use kiln_ooxml::drawingml::color::{DrawingColor, parse_color_child, serialize_color};
use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, XmlNode};

use super::xml::{NS_P159, descendants_local, parse_kept};
use crate::common::kind::SUPPORTED_MC;

// ------------------------------------------------------------------ transitions

/// Milliseconds from a time attribute; `None` for `indefinite` or garbage (as the reader).
fn ms(e: &XmlElement, name: &str) -> Option<u32> {
    let n: i64 = e.attr(name)?.trim().parse().ok()?;
    u32::try_from(n.max(0)).ok()
}

/// The `p:transition` in a kept fragment (itself, or the chosen branch of the
/// `mc:AlternateContent` around it).
fn find_transition(root: &XmlElement) -> Option<&XmlElement> {
    if root.name == "p:transition" {
        return Some(root);
    }
    if root.name == "mc:AlternateContent" {
        return root
            .select_alternate_content(&SUPPORTED_MC)
            .into_iter()
            .find(|e| e.name == "p:transition");
    }
    None
}

/// Whether the kept transition still describes the model's.
fn same_transition(kept: &XmlElement, t: &Transition) -> bool {
    let Some(el) = find_transition(kept) else {
        return false;
    };
    let child = el
        .elements()
        .find(|c| !matches!(local_name(&c.name), "sndAc" | "extLst"));
    let (effect, exact) = child.map_or(
        (TransitionEffect::None, true),
        TransitionEffect::from_element,
    );
    let duration = ms(el, "p14:dur")
        .or_else(|| el.attr("spd").map(Transition::duration_from_speed))
        .unwrap_or_else(|| effect.default_duration_ms());
    let original_ok = match (&t.original, child) {
        (None, _) => exact,
        (Some(o), Some(c)) => {
            !exact
                && parse_kept(o).is_some_and(|p| {
                    p.name == c.name
                        && p.attrs
                            .iter()
                            .filter(|(k, _)| !k.starts_with("xmlns"))
                            .eq(c.attrs.iter().filter(|(k, _)| !k.starts_with("xmlns")))
                })
        }
        (Some(_), None) => false,
    };
    effect == t.effect
        && duration == t.duration_ms
        && el.attr_bool("advClick").unwrap_or(true) == t.advance_on_click
        && ms(el, "advTm") == t.advance_after_ms
        && original_ok
}

fn speed(duration_ms: u32) -> &'static str {
    match duration_ms {
        0..=500 => "fast",
        501..=750 => "med",
        _ => "slow",
    }
}

fn transition_shell(t: &Transition, p14_duration: bool) -> XmlElement {
    let spd = speed(t.duration_ms);
    XmlElement::new("p:transition")
        .with_opt_attr("spd", (spd != "fast").then_some(spd))
        .with_opt_attr("p14:dur", p14_duration.then_some(t.duration_ms))
        .with_opt_attr("advClick", (!t.advance_on_click).then_some("0"))
        .with_opt_attr("advTm", t.advance_after_ms)
}

/// The effect element: the original one when the model still maps it to the same effect (an
/// exotic transition Kiln shows as its nearest relative), else the model's.
fn effect_element(t: &Transition) -> Option<XmlElement> {
    if let Some(o) = &t.original
        && let Some(e) = parse_kept(o)
        && TransitionEffect::from_element(&e).0 == t.effect
    {
        return Some(e);
    }
    t.effect.to_element()
}

/// A slide's `p:transition`: the kept original when it still says the same, else generated.
pub(crate) fn transition_node(t: &Transition, kept: Option<&str>) -> Option<XmlNode> {
    if let Some(k) = kept
        && let Some(parsed) = parse_kept(k)
        && same_transition(&parsed, t)
    {
        return Some(XmlNode::Element(parsed));
    }
    let effect = effect_element(t);
    let name = effect.as_ref().map(|e| e.name.clone()).unwrap_or_default();
    let modern = name.starts_with("p14:") || name.starts_with("p159:");
    let exact_speed = Transition::duration_from_speed(speed(t.duration_ms)) == t.duration_ms;
    if !modern && exact_speed {
        return Some(XmlNode::Element(
            transition_shell(t, false).with_opt_child(effect),
        ));
    }
    // PowerPoint 2010+: the exact duration and newer effects, with a fallback for older readers.
    let (requires, fallback_effect) = if name.starts_with("p159:") {
        ("p159 p14", TransitionEffect::FALLBACK.to_element())
    } else if modern {
        ("p14", TransitionEffect::FALLBACK.to_element())
    } else {
        ("p14", effect.clone())
    };
    let mut choice = XmlElement::new("mc:Choice").with_attr("Requires", requires);
    if requires.contains("p159") {
        choice = XmlElement::new("mc:Choice")
            .with_attr("xmlns:p159", NS_P159)
            .with_attr("Requires", requires);
    }
    Some(XmlNode::Element(
        XmlElement::new("mc:AlternateContent")
            .with_child(choice.with_child(transition_shell(t, true).with_opt_child(effect)))
            .with_child(
                XmlElement::new("mc:Fallback")
                    .with_child(transition_shell(t, false).with_opt_child(fallback_effect)),
            ),
    ))
}

// ------------------------------------------------------------------ reading kept timing back

/// One effect as the reader sees it in a timing tree, with the file's shape ids.
#[derive(Debug, PartialEq, Eq)]
struct SeenEffect {
    spid: u32,
    paragraphs: Option<(u32, u32)>,
    effect: AnimationEffect,
    start: EffectStart,
    delay_ms: u32,
    duration_ms: u32,
    repeat: u32,
}

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

fn attr_u32(e: &XmlElement, name: &str) -> Option<u32> {
    e.attr_int(name).and_then(|v| u32::try_from(v).ok())
}

/// The options PowerPoint keeps in the behaviours (as the reader refines them).
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

fn seen_effect(par: &XmlElement) -> Option<SeenEffect> {
    let ctn = par.child("p:cTn")?;
    let tgt = descendants_local(par, "spTgt").into_iter().next()?;
    let spid = attr_u32(tgt, "spid")?;
    let paragraphs = tgt.child_path(&["p:txEl", "p:pRg"]).map(|r| {
        (
            attr_u32(r, "st").unwrap_or(0),
            attr_u32(r, "end").unwrap_or(0),
        )
    });
    let class = EffectClass::parse(ctn.attr("presetClass").unwrap_or("entr"));
    let preset = attr_u32(ctn, "presetID").unwrap_or(0);
    let subtype = attr_u32(ctn, "presetSubtype").unwrap_or(0);
    let mut effect = AnimationEffect::from_ooxml_preset(class, preset, subtype);
    refine(&mut effect, par);
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
    Some(SeenEffect {
        spid,
        paragraphs,
        effect,
        start,
        delay_ms,
        duration_ms,
        repeat,
    })
}

/// The main sequence and the trigger sequences (trigger spid, effects) of a timing tree.
type SeenTimeline = (Vec<SeenEffect>, Vec<(u32, Vec<SeenEffect>)>);

fn seen_timeline(timing: &XmlElement) -> SeenTimeline {
    let mut main = Vec::new();
    let mut triggers = Vec::new();
    let mut seqs = Vec::new();
    collect_named(timing, "p:seq", &mut seqs);
    for seq in seqs {
        let Some(ctn) = seq.child("p:cTn") else {
            continue;
        };
        let mut pars = Vec::new();
        collect_effects(ctn, &mut pars);
        let effects: Vec<SeenEffect> = pars.into_iter().filter_map(seen_effect).collect();
        if ctn.attr("nodeType") == Some("interactiveSeq") {
            let trigger = ctn
                .child("p:stCondLst")
                .into_iter()
                .flat_map(|l| l.children_named("p:cond"))
                .find_map(|c| {
                    c.child_path(&["p:tgtEl", "p:spTgt"])
                        .and_then(|t| attr_u32(t, "spid"))
                });
            match trigger {
                Some(t) => triggers.push((t, effects)),
                None => main.push(SeenEffect {
                    // An unreadable trigger sequence: never equal to a model.
                    spid: 0,
                    paragraphs: None,
                    effect: AnimationEffect::Appear,
                    start: EffectStart::OnClick,
                    delay_ms: u32::MAX,
                    duration_ms: u32::MAX,
                    repeat: u32::MAX,
                }),
            }
        } else {
            main.extend(effects);
        }
    }
    (main, triggers)
}

/// The effect without the kept XML (which is not part of what the effect says).
fn bare(e: &AnimationEffect) -> AnimationEffect {
    match e {
        AnimationEffect::Other {
            class,
            preset_id,
            subtype,
            ..
        } => AnimationEffect::Other {
            class: *class,
            preset_id: *preset_id,
            subtype: *subtype,
            xml: None,
        },
        other => other.clone(),
    }
}

fn seen_of(e: &Effect, file_id: &dyn Fn(NodeId) -> Option<u32>) -> Option<SeenEffect> {
    Some(SeenEffect {
        spid: file_id(e.target.shape)?,
        paragraphs: e.target.paragraphs,
        effect: bare(&e.effect),
        start: e.start,
        delay_ms: e.delay_ms,
        duration_ms: e.duration_ms,
        repeat: e.repeat.max(1),
    })
}

/// Whether the kept `p:timing` still describes the slide: the same effects (as the reader reads
/// them) on the same shapes, every shape it names still on the slide under the id it had.
fn same_timing(
    kept: &XmlElement,
    timeline: &Timeline,
    ids: &HashMap<NodeId, u32>,
    original: &HashMap<NodeId, u32>,
) -> bool {
    // Every spid in the kept tree must be a shape still written under that id.
    let written: BTreeSet<u32> = ids.values().copied().collect();
    let mut stack = vec![kept];
    while let Some(e) = stack.pop() {
        if let Some(spid) = e.attr("spid") {
            match spid.trim().parse::<u32>() {
                Ok(v) if written.contains(&v) => {}
                _ => return false,
            }
        }
        stack.extend(e.elements());
    }
    // Shapes keep the id they were read with.
    let file_id = |n: NodeId| -> Option<u32> {
        let id = *ids.get(&n)?;
        (original.get(&n) == Some(&id)).then_some(id)
    };
    let (main, triggers) = seen_timeline(kept);
    let mut model_main = Vec::new();
    for e in &timeline.sequence {
        let Some(s) = seen_of(e, &file_id) else {
            return false;
        };
        model_main.push(s);
    }
    let mut seen_main = main;
    for s in &mut seen_main {
        s.effect = bare(&s.effect);
    }
    if seen_main != model_main || triggers.len() != timeline.triggers.len() {
        return false;
    }
    for ((spid, effects), t) in triggers.into_iter().zip(&timeline.triggers) {
        if file_id(t.trigger) != Some(spid) || effects.len() != t.effects.len() {
            return false;
        }
        for (mut s, e) in effects.into_iter().zip(&t.effects) {
            s.effect = bare(&s.effect);
            if Some(s) != seen_of(e, &file_id) {
                return false;
            }
        }
    }
    true
}

// ------------------------------------------------------------------ generating timing

/// Builds a timing tree with unique `p:cTn` ids.
struct TimingBuilder<'i> {
    next: u32,
    ids: &'i HashMap<NodeId, u32>,
    /// Original file id → written id, for kept effect XML.
    remap: &'i HashMap<u32, u32>,
}

fn cond(delay: &str) -> XmlElement {
    XmlElement::new("p:cond").with_attr("delay", delay)
}

fn st_cond(delay: &str) -> XmlElement {
    XmlElement::new("p:stCondLst").with_child(cond(delay))
}

fn tgt_el(spid: u32, paragraphs: Option<(u32, u32)>) -> XmlElement {
    let mut t = XmlElement::new("p:spTgt").with_attr("spid", spid);
    if let Some((st, end)) = paragraphs {
        t = t.with_child(
            XmlElement::new("p:txEl").with_child(
                XmlElement::new("p:pRg")
                    .with_attr("st", st)
                    .with_attr("end", end),
            ),
        );
    }
    XmlElement::new("p:tgtEl").with_child(t)
}

fn attr_names(names: &[&str]) -> Option<XmlElement> {
    (!names.is_empty()).then(|| {
        XmlElement::new("p:attrNameLst").with_children(
            names
                .iter()
                .map(|n| XmlElement::new("p:attrName").with_text(*n)),
        )
    })
}

/// Off-slide positions for Fly In/Out: (x from, y from) formulas for a direction.
fn offscreen(d: Direction8) -> (&'static str, &'static str) {
    let x = match d {
        Direction8::Left | Direction8::LeftUp | Direction8::LeftDown => "0-#ppt_w/2",
        Direction8::Right | Direction8::RightUp | Direction8::RightDown => "1+#ppt_w/2",
        _ => "#ppt_x",
    };
    let y = match d {
        Direction8::Up | Direction8::LeftUp | Direction8::RightUp => "0-#ppt_h/2",
        Direction8::Down | Direction8::LeftDown | Direction8::RightDown => "1+#ppt_h/2",
        _ => "#ppt_y",
    };
    (x, y)
}

fn wipe_filter(from: Side) -> &'static str {
    match from {
        Side::Up => "wipe(down)",
        Side::Down => "wipe(up)",
        Side::Left => "wipe(right)",
        Side::Right => "wipe(left)",
    }
}

impl TimingBuilder<'_> {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// `p:cBhvr` with its own `p:cTn`.
    fn bhvr(
        &mut self,
        dur: u32,
        delay: Option<u32>,
        fill_hold: bool,
        target: &EffectTarget,
        spid: u32,
        names: &[&str],
    ) -> XmlElement {
        let id = self.id();
        let ctn = XmlElement::new("p:cTn")
            .with_attr("id", id)
            .with_attr("dur", dur.max(1))
            .with_opt_attr("fill", fill_hold.then_some("hold"))
            .with_opt_child(delay.map(|d| st_cond(&d.to_string())));
        XmlElement::new("p:cBhvr")
            .with_child(ctn)
            .with_child(tgt_el(spid, target.paragraphs))
            .with_opt_child(attr_names(names))
    }

    fn set_visibility(
        &mut self,
        t: &EffectTarget,
        spid: u32,
        visible: bool,
        delay: u32,
    ) -> XmlElement {
        let b = self.bhvr(1, Some(delay), true, t, spid, &["style.visibility"]);
        XmlElement::new("p:set").with_child(b).with_child(
            XmlElement::new("p:to").with_child(
                XmlElement::new("p:strVal")
                    .with_attr("val", if visible { "visible" } else { "hidden" }),
            ),
        )
    }

    fn anim_effect(
        &mut self,
        t: &EffectTarget,
        spid: u32,
        out: bool,
        filter: &str,
        dur: u32,
    ) -> XmlElement {
        let b = self.bhvr(dur, None, false, t, spid, &[]);
        XmlElement::new("p:animEffect")
            .with_attr("transition", if out { "out" } else { "in" })
            .with_attr("filter", filter)
            .with_child(b)
    }

    /// `p:anim` of one property through key values (`tm` in 1000ths of a percent).
    fn anim(
        &mut self,
        t: &EffectTarget,
        spid: u32,
        attr: &str,
        values: &[(u32, &str)],
        dur: u32,
    ) -> XmlElement {
        let mut b = self.bhvr(dur, None, true, t, spid, &[attr]);
        b.attrs.insert(0, "additive", "base");
        XmlElement::new("p:anim")
            .with_attr("calcmode", "lin")
            .with_attr("valueType", "num")
            .with_child(b)
            .with_child(
                XmlElement::new("p:tavLst").with_children(values.iter().map(|(tm, v)| {
                    XmlElement::new("p:tav").with_attr("tm", *tm).with_child(
                        XmlElement::new("p:val")
                            .with_child(XmlElement::new("p:strVal").with_attr("val", *v)),
                    )
                })),
            )
    }

    /// The behaviours that play an effect.
    fn behaviours(&mut self, e: &Effect, spid: u32) -> Vec<XmlElement> {
        let t = &e.target;
        let d = e.duration_ms.max(1);
        let end = d.saturating_sub(1);
        let mut out = Vec::new();
        match &e.effect {
            AnimationEffect::Appear => {
                let b = self.bhvr(d, Some(0), true, t, spid, &["style.visibility"]);
                out.push(
                    XmlElement::new("p:set").with_child(b).with_child(
                        XmlElement::new("p:to")
                            .with_child(XmlElement::new("p:strVal").with_attr("val", "visible")),
                    ),
                );
            }
            AnimationEffect::Fade => {
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim_effect(t, spid, false, "fade", d));
            }
            AnimationEffect::FlyIn { from } => {
                let (x, y) = offscreen(*from);
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim(t, spid, "ppt_x", &[(0, x), (100_000, "#ppt_x")], d));
                out.push(self.anim(t, spid, "ppt_y", &[(0, y), (100_000, "#ppt_y")], d));
            }
            AnimationEffect::FloatIn { down } => {
                let from = if *down { "#ppt_y-.1" } else { "#ppt_y+.1" };
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim_effect(t, spid, false, "fade", d));
                out.push(self.anim(t, spid, "ppt_y", &[(0, from), (100_000, "#ppt_y")], d));
            }
            AnimationEffect::Split { vertical, inward } => {
                let f = match (vertical, inward) {
                    (true, true) => "barn(inVertical)",
                    (true, false) => "barn(outVertical)",
                    (false, true) => "barn(inHorizontal)",
                    (false, false) => "barn(outHorizontal)",
                };
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim_effect(t, spid, false, f, d));
            }
            AnimationEffect::Wipe { from } => {
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim_effect(t, spid, false, wipe_filter(*from), d));
            }
            AnimationEffect::Zoom => {
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim(t, spid, "ppt_w", &[(0, "0"), (100_000, "#ppt_w")], d));
                out.push(self.anim(t, spid, "ppt_h", &[(0, "0"), (100_000, "#ppt_h")], d));
                out.push(self.anim_effect(t, spid, false, "fade", d));
            }
            AnimationEffect::GrowTurn => {
                out.push(self.set_visibility(t, spid, true, 0));
                out.push(self.anim(t, spid, "ppt_w", &[(0, "0"), (100_000, "#ppt_w")], d));
                out.push(self.anim(t, spid, "ppt_h", &[(0, "0"), (100_000, "#ppt_h")], d));
                out.push(self.anim(t, spid, "style.rotation", &[(0, "90"), (100_000, "0")], d));
                out.push(self.anim_effect(t, spid, false, "fade", d));
            }
            AnimationEffect::Pulse => {
                let w = [(0, "#ppt_w"), (50_000, "#ppt_w*1.05"), (100_000, "#ppt_w")];
                let h = [(0, "#ppt_h"), (50_000, "#ppt_h*1.05"), (100_000, "#ppt_h")];
                out.push(self.anim(t, spid, "ppt_w", &w, d));
                out.push(self.anim(t, spid, "ppt_h", &h, d));
            }
            AnimationEffect::ColorPulse { color } => {
                let b = self.bhvr(d, None, true, t, spid, &["style.color"]);
                let c = color
                    .clone()
                    .unwrap_or_else(|| DrawingColor::scheme("accent1"));
                out.push(
                    XmlElement::new("p:animClr")
                        .with_attr("clrSpc", "hsl")
                        .with_attr("dir", "cw")
                        .with_child(b)
                        .with_child(XmlElement::new("p:to").with_child(serialize_color(&c))),
                );
            }
            AnimationEffect::Teeter => {
                let v = [
                    (0, "0"),
                    (25_000, "4"),
                    (50_000, "-4"),
                    (75_000, "4"),
                    (100_000, "0"),
                ];
                out.push(self.anim(t, spid, "r", &v, d));
            }
            AnimationEffect::Spin { degrees } => {
                let b = self.bhvr(d, None, true, t, spid, &["r"]);
                out.push(
                    XmlElement::new("p:animRot")
                        .with_attr("by", i64::from(*degrees) * 60_000)
                        .with_child(b),
                );
            }
            AnimationEffect::GrowShrink { scale_pct } => {
                let b = self.bhvr(d, None, true, t, spid, &[]);
                let v = i64::from(*scale_pct) * 1000;
                out.push(
                    XmlElement::new("p:animScale")
                        .with_child(b)
                        .with_child(XmlElement::new("p:by").with_attr("x", v).with_attr("y", v)),
                );
            }
            AnimationEffect::Disappear => {
                let b = self.bhvr(d, Some(0), true, t, spid, &["style.visibility"]);
                out.push(
                    XmlElement::new("p:set").with_child(b).with_child(
                        XmlElement::new("p:to")
                            .with_child(XmlElement::new("p:strVal").with_attr("val", "hidden")),
                    ),
                );
            }
            AnimationEffect::FadeOut => {
                out.push(self.anim_effect(t, spid, true, "fade", d));
                out.push(self.set_visibility(t, spid, false, end));
            }
            AnimationEffect::FlyOut { to } => {
                let (x, y) = offscreen(*to);
                out.push(self.anim(t, spid, "ppt_x", &[(0, "#ppt_x"), (100_000, x)], d));
                out.push(self.anim(t, spid, "ppt_y", &[(0, "#ppt_y"), (100_000, y)], d));
                out.push(self.set_visibility(t, spid, false, end));
            }
            AnimationEffect::ZoomOut => {
                out.push(self.anim(t, spid, "ppt_w", &[(0, "#ppt_w"), (100_000, "0")], d));
                out.push(self.anim(t, spid, "ppt_h", &[(0, "#ppt_h"), (100_000, "0")], d));
                out.push(self.anim_effect(t, spid, true, "fade", d));
                out.push(self.set_visibility(t, spid, false, end));
            }
            AnimationEffect::Other { .. } => {}
        }
        out
    }

    /// One effect's `p:par`.
    fn effect_par(&mut self, e: &Effect, node_type: &str) -> Option<XmlElement> {
        let spid = *self.ids.get(&e.target.shape)?;
        if let AnimationEffect::Other { xml, .. } = &e.effect {
            return self.kept_effect(xml.as_deref()?, e, node_type);
        }
        let (class, preset, subtype) = e.effect.ooxml_preset();
        let id = self.id();
        let behaviours = self.behaviours(e, spid);
        let ctn = XmlElement::new("p:cTn")
            .with_attr("id", id)
            .with_attr("presetID", preset)
            .with_attr("presetClass", class.as_str())
            .with_attr("presetSubtype", subtype)
            .with_opt_attr(
                "repeatCount",
                (e.repeat > 1).then(|| u64::from(e.repeat) * 1000),
            )
            .with_attr("fill", "hold")
            .with_attr("grpId", 0)
            .with_attr("nodeType", node_type)
            .with_child(st_cond(&e.delay_ms.to_string()))
            .with_child(XmlElement::new("p:childTnLst").with_children(behaviours));
        Some(XmlElement::new("p:par").with_child(ctn))
    }

    /// An effect Kiln does not play: its kept `p:par` with fresh node ids, the model's start,
    /// delay and repeat, and the shape ids it is written with.
    fn kept_effect(&mut self, xml: &str, e: &Effect, node_type: &str) -> Option<XmlElement> {
        let mut par = parse_kept(xml)?;
        let mut stack: Vec<&mut XmlElement> = vec![&mut par];
        let mut first = true;
        while let Some(el) = stack.pop() {
            if el.name == "p:cTn" {
                self.next += 1;
                el.set_attr("id", Some(self.next));
                if first {
                    first = false;
                    el.set_attr("nodeType", Some(node_type));
                    el.set_attr(
                        "repeatCount",
                        (e.repeat > 1).then(|| u64::from(e.repeat) * 1000),
                    );
                    match el
                        .child_mut("p:stCondLst")
                        .and_then(|l| l.child_mut("p:cond"))
                    {
                        Some(c) => c.set_attr("delay", Some(e.delay_ms)),
                        None => {
                            el.children
                                .insert(0, XmlNode::Element(st_cond(&e.delay_ms.to_string())));
                        }
                    }
                }
            }
            if let Some(v) = el.attr("spid").and_then(|v| v.trim().parse::<u32>().ok())
                && let Some(n) = self.remap.get(&v)
            {
                el.set_attr("spid", Some(*n));
            }
            // Visit in document order so the effect's own cTn gets the first id.
            let mut children: Vec<&mut XmlElement> = el.elements_mut().collect();
            children.reverse();
            stack.extend(children);
        }
        Some(par)
    }

    /// The click, group and effect nodes of a sequence.
    fn sequence(&mut self, effects: &[Effect], seq_id: u32, interactive: bool) -> Vec<XmlElement> {
        let mut clicks: Vec<XmlElement> = Vec::new();
        // The click node being filled and its groups: (group delay, effect pars).
        let mut groups: Vec<(u64, Vec<XmlElement>)> = Vec::new();
        let mut click_end = 0u64;
        let mut group_start = 0u64;
        let mut first = true;
        let mut auto_start = false;
        let flush = |b: &mut Self,
                     groups: &mut Vec<(u64, Vec<XmlElement>)>,
                     clicks: &mut Vec<XmlElement>,
                     auto: bool,
                     first_click: bool| {
            if groups.is_empty() {
                return;
            }
            let click_id = b.id();
            let mut stc =
                XmlElement::new("p:stCondLst").with_child(cond(if interactive && first_click {
                    "0"
                } else {
                    "indefinite"
                }));
            if auto {
                stc = stc.with_child(
                    XmlElement::new("p:cond")
                        .with_attr("evt", "onBegin")
                        .with_attr("delay", "0")
                        .with_child(XmlElement::new("p:tn").with_attr("val", seq_id)),
                );
            }
            let mut inner = Vec::new();
            for (delay, pars) in groups.drain(..) {
                let gid = b.id();
                inner.push(
                    XmlElement::new("p:par").with_child(
                        XmlElement::new("p:cTn")
                            .with_attr("id", gid)
                            .with_attr("fill", "hold")
                            .with_child(st_cond(&delay.to_string()))
                            .with_child(XmlElement::new("p:childTnLst").with_children(pars)),
                    ),
                );
            }
            clicks.push(
                XmlElement::new("p:par").with_child(
                    XmlElement::new("p:cTn")
                        .with_attr("id", click_id)
                        .with_attr("fill", "hold")
                        .with_child(stc)
                        .with_child(XmlElement::new("p:childTnLst").with_children(inner)),
                ),
            );
        };
        // Node ids are handed out depth first in document order as PowerPoint does: the click
        // and group nodes get theirs when the click is complete, so collect effects first.
        let mut pending: Vec<(EffectStart, &Effect)> = Vec::new();
        for e in effects {
            pending.push((e.start, e));
        }
        let mut first_click = true;
        for (start, e) in pending {
            let node_type = match start {
                EffectStart::OnClick => "clickEffect",
                EffectStart::WithPrevious => "withEffect",
                EffectStart::AfterPrevious => "afterEffect",
            };
            match start {
                EffectStart::OnClick => {
                    if !first {
                        flush(self, &mut groups, &mut clicks, auto_start, first_click);
                        first_click = false;
                        auto_start = false;
                    }
                    click_end = 0;
                    group_start = 0;
                    groups.push((0, Vec::new()));
                }
                EffectStart::WithPrevious => {
                    if groups.is_empty() {
                        auto_start = first && !interactive;
                        groups.push((0, Vec::new()));
                    }
                }
                EffectStart::AfterPrevious => {
                    if groups.is_empty() {
                        auto_start = first && !interactive;
                    }
                    group_start = click_end;
                    groups.push((group_start, Vec::new()));
                }
            }
            first = false;
            click_end = click_end.max(group_start.saturating_add(e.total_ms()));
            if let Some(par) = self.effect_par(e, node_type)
                && let Some(g) = groups.last_mut()
            {
                g.1.push(par);
            }
        }
        flush(self, &mut groups, &mut clicks, auto_start, first_click);
        clicks
    }
}

/// Text shapes on the slide, for the build list.
pub(crate) struct TimingInput<'t> {
    pub timeline: &'t Timeline,
    /// Written `p:cNvPr/@id` of each shape on the slide.
    pub ids: &'t HashMap<NodeId, u32>,
    /// The id each shape was read with.
    pub original_ids: &'t HashMap<NodeId, u32>,
    /// Shapes with text (they get a `p:bldP`).
    pub text_shapes: &'t BTreeSet<NodeId>,
}

/// A slide's `p:timing`: the kept original when it still describes the slide, else generated.
/// `None` when there is nothing to animate.
pub(crate) fn timing_node(input: &TimingInput<'_>, kept: Option<&str>) -> Option<XmlNode> {
    let tl = input.timeline;
    if let Some(k) = kept
        && let Some(parsed) = parse_kept(k)
        && same_timing(&parsed, tl, input.ids, input.original_ids)
    {
        return Some(XmlNode::Element(parsed));
    }
    if tl.is_empty() {
        return None;
    }
    let remap: HashMap<u32, u32> = input
        .original_ids
        .iter()
        .filter_map(|(n, orig)| input.ids.get(n).map(|w| (*orig, *w)))
        .collect();
    let mut b = TimingBuilder {
        next: 2,
        ids: input.ids,
        remap: &remap,
    };
    let main = b.sequence(&tl.sequence, 2, false);
    let mut root_children = Vec::new();
    if !main.is_empty() {
        root_children.push(
            XmlElement::new("p:seq")
                .with_attr("concurrent", "1")
                .with_attr("nextAc", "seek")
                .with_child(
                    XmlElement::new("p:cTn")
                        .with_attr("id", 2)
                        .with_attr("dur", "indefinite")
                        .with_attr("nodeType", "mainSeq")
                        .with_child(XmlElement::new("p:childTnLst").with_children(main)),
                )
                .with_child(
                    XmlElement::new("p:prevCondLst").with_child(
                        XmlElement::new("p:cond")
                            .with_attr("evt", "onPrev")
                            .with_attr("delay", "0")
                            .with_child(
                                XmlElement::new("p:tgtEl").with_child(XmlElement::new("p:sldTgt")),
                            ),
                    ),
                )
                .with_child(
                    XmlElement::new("p:nextCondLst").with_child(
                        XmlElement::new("p:cond")
                            .with_attr("evt", "onNext")
                            .with_attr("delay", "0")
                            .with_child(
                                XmlElement::new("p:tgtEl").with_child(XmlElement::new("p:sldTgt")),
                            ),
                    ),
                ),
        );
    }
    for t in &tl.triggers {
        let Some(&spid) = input.ids.get(&t.trigger) else {
            continue;
        };
        let seq_id = b.id();
        let pars = b.sequence(&t.effects, seq_id, true);
        let on_click = || {
            XmlElement::new("p:cond")
                .with_attr("evt", "onClick")
                .with_attr("delay", "0")
                .with_child(
                    XmlElement::new("p:tgtEl")
                        .with_child(XmlElement::new("p:spTgt").with_attr("spid", spid)),
                )
        };
        root_children.push(
            XmlElement::new("p:seq")
                .with_attr("concurrent", "1")
                .with_attr("nextAc", "seek")
                .with_child(
                    XmlElement::new("p:cTn")
                        .with_attr("id", seq_id)
                        .with_attr("restart", "whenNotActive")
                        .with_attr("fill", "hold")
                        .with_attr("evtFilter", "cancelBubble")
                        .with_attr("nodeType", "interactiveSeq")
                        .with_child(XmlElement::new("p:stCondLst").with_child(on_click()))
                        .with_child(
                            XmlElement::new("p:endSync")
                                .with_attr("evt", "end")
                                .with_attr("delay", "0")
                                .with_child(XmlElement::new("p:rtn").with_attr("val", "all")),
                        )
                        .with_child(XmlElement::new("p:childTnLst").with_children(pars)),
                )
                .with_child(XmlElement::new("p:nextCondLst").with_child(on_click())),
        );
    }
    if root_children.is_empty() {
        return None;
    }
    // The build list: text shapes animated as a whole or by paragraph.
    let mut builds: Vec<(u32, bool)> = Vec::new();
    for e in tl
        .sequence
        .iter()
        .chain(tl.triggers.iter().flat_map(|t| t.effects.iter()))
    {
        if matches!(e.effect, AnimationEffect::Other { .. }) {
            continue;
        }
        if input.text_shapes.contains(&e.target.shape)
            && let Some(&spid) = input.ids.get(&e.target.shape)
            && !builds.iter().any(|(s, _)| *s == spid)
        {
            builds.push((spid, e.target.paragraphs.is_some()));
        }
    }
    let bld = (!builds.is_empty()).then(|| {
        XmlElement::new("p:bldLst").with_children(builds.into_iter().map(|(spid, by_para)| {
            let p = XmlElement::new("p:bldP")
                .with_attr("spid", spid)
                .with_attr("grpId", 0);
            if by_para {
                p.with_attr("build", "p")
            } else {
                p.with_attr("animBg", "1")
            }
        }))
    });
    Some(XmlNode::Element(
        XmlElement::new("p:timing")
            .with_child(
                XmlElement::new("p:tnLst").with_child(
                    XmlElement::new("p:par").with_child(
                        XmlElement::new("p:cTn")
                            .with_attr("id", 1)
                            .with_attr("dur", "indefinite")
                            .with_attr("restart", "never")
                            .with_attr("nodeType", "tmRoot")
                            .with_child(
                                XmlElement::new("p:childTnLst").with_children(root_children),
                            ),
                    ),
                ),
            )
            .with_opt_child(bld),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_deck::transition::MorphBy;

    #[test]
    fn transitions_pick_their_form() {
        let mut fade = Transition::new(TransitionEffect::Fade {
            through_black: false,
        });
        fade.duration_ms = 500;
        let n = transition_node(&fade, None).unwrap();
        let e = n.as_element().unwrap();
        assert_eq!(e.name, "p:transition");
        assert!(e.child("p:fade").is_some());
        let mut slow = fade.clone();
        slow.duration_ms = 1600;
        let n = transition_node(&slow, None).unwrap();
        let ac = n.as_element().unwrap();
        assert_eq!(ac.name, "mc:AlternateContent");
        let chosen = ac.select_alternate_content(&SUPPORTED_MC);
        assert_eq!(chosen[0].attr("p14:dur"), Some("1600"));
        let morph = Transition::new(TransitionEffect::Morph { by: MorphBy::Words });
        let n = transition_node(&morph, None).unwrap();
        let ac = n.as_element().unwrap();
        let fallback = ac.child("mc:Fallback").unwrap();
        assert!(
            fallback
                .child("p:transition")
                .unwrap()
                .child("p:fade")
                .is_some()
        );
        // An unchanged kept transition comes back verbatim.
        let kept = r#"<p:transition xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" spd="slow"><p:fade/></p:transition>"#;
        let mut t = fade.clone();
        t.duration_ms = 1000;
        let n = transition_node(&t, Some(kept)).unwrap();
        assert_eq!(n.as_element().unwrap().attr("spd"), Some("slow"));
    }

    #[test]
    fn generated_timing_reads_back_as_the_model() {
        let mut ids = HashMap::new();
        ids.insert(NodeId(10), 3u32);
        ids.insert(NodeId(11), 4u32);
        let effects = [
            AnimationEffect::Fade,
            AnimationEffect::FlyIn {
                from: Direction8::Left,
            },
            AnimationEffect::Spin { degrees: 180 },
            AnimationEffect::GrowShrink { scale_pct: 120 },
            AnimationEffect::FadeOut,
            AnimationEffect::Appear,
        ];
        let mut tl = Timeline::default();
        for (i, eff) in effects.into_iter().enumerate() {
            let mut e = Effect::new(NodeId(100 + i as u64), NodeId(10 + (i as u64 % 2)), eff);
            e.start = match i % 3 {
                0 => EffectStart::OnClick,
                1 => EffectStart::WithPrevious,
                _ => EffectStart::AfterPrevious,
            };
            e.delay_ms = 250 * i as u32;
            tl.sequence.push(e);
        }
        let text = BTreeSet::from([NodeId(10)]);
        let input = TimingInput {
            timeline: &tl,
            ids: &ids,
            original_ids: &ids,
            text_shapes: &text,
        };
        let node = timing_node(&input, None).unwrap();
        let el = node.as_element().unwrap();
        // Every cTn id is unique.
        let ctns = descendants_local(el, "cTn");
        let unique: BTreeSet<_> = ctns.iter().filter_map(|c| c.attr("id")).collect();
        assert_eq!(unique.len(), ctns.len());
        // The tree reads back as the timeline, so it would be kept verbatim next time.
        assert!(same_timing(el, &tl, &ids, &ids));
        // A changed duration is noticed.
        let mut changed = tl.clone();
        changed.sequence[1].duration_ms += 1;
        assert!(!same_timing(el, &changed, &ids, &ids));
    }
}
