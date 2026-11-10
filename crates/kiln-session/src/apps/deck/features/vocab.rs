// SPDX-License-Identifier: AGPL-3.0-or-later
//! The words of the Transitions and Animations tabs: gallery ids and Effect Options for every
//! transition and effect, as PowerPoint lists them, mapped to and from the `kiln-deck` model.

use kiln_deck::animation::{AnimationEffect, EffectClass};
use kiln_deck::transition::{Direction8, MorphBy, ShapeForm, Side, TransitionEffect};
use kiln_protocol::commands::deck_show::DeckChoice;

fn choice(id: &str, label: &str, group: Option<&str>) -> DeckChoice {
    DeckChoice {
        id: id.to_owned(),
        label: label.to_owned(),
        group: group.map(str::to_owned),
    }
}

// ── transitions ──────────────────────────────────────────────────────────────────────────────

/// The gallery id of a transition.
#[must_use]
pub fn transition_id(e: &TransitionEffect) -> &'static str {
    use TransitionEffect as T;
    match e {
        T::None => "none",
        T::Fade { .. } => "fade",
        T::Push { .. } => "push",
        T::Wipe { .. } => "wipe",
        T::Split { .. } => "split",
        T::Reveal { .. } => "reveal",
        T::Cut { .. } => "cut",
        T::RandomBars { .. } => "randomBars",
        T::Shape { .. } => "shape",
        T::Uncover { .. } => "uncover",
        T::Cover { .. } => "cover",
        T::Flash => "flash",
        T::Morph { .. } => "morph",
    }
}

/// The transition of a gallery id, with PowerPoint's default option.
#[must_use]
pub fn transition_of(id: &str) -> Option<TransitionEffect> {
    kiln_deck::transition::GALLERY
        .iter()
        .copied()
        .find(|e| transition_id(e).eq_ignore_ascii_case(id))
}

/// Push and Wipe: `dir` is the way things move, so "From Bottom" moves up.
const SIDES: [(Side, &str, &str); 4] = [
    (Side::Up, "fromBottom", "From Bottom"),
    (Side::Right, "fromLeft", "From Left"),
    (Side::Left, "fromRight", "From Right"),
    (Side::Down, "fromTop", "From Top"),
];

/// Cover and Uncover, in the order of PowerPoint's menu.
const DIRS8: [(Direction8, &str, &str); 8] = [
    (Direction8::Left, "fromRight", "From Right"),
    (Direction8::Down, "fromTop", "From Top"),
    (Direction8::Right, "fromLeft", "From Left"),
    (Direction8::Up, "fromBottom", "From Bottom"),
    (Direction8::LeftDown, "fromTopRight", "From Top-Right"),
    (Direction8::LeftUp, "fromBottomRight", "From Bottom-Right"),
    (Direction8::RightDown, "fromTopLeft", "From Top-Left"),
    (Direction8::RightUp, "fromBottomLeft", "From Bottom-Left"),
];

/// The Effect Options of a transition.
#[must_use]
pub fn transition_options(e: &TransitionEffect) -> Vec<DeckChoice> {
    use TransitionEffect as T;
    let dir = Some("Direction");
    match e {
        T::None | T::Flash => Vec::new(),
        T::Fade { .. } | T::Cut { .. } => vec![
            choice("smoothly", "Smoothly", None),
            choice("throughBlack", "Through Black", None),
        ],
        T::Push { .. } => SIDES.iter().map(|(_, id, l)| choice(id, l, dir)).collect(),
        T::Wipe { .. } => [SIDES[2], SIDES[3], SIDES[1], SIDES[0]]
            .iter()
            .map(|(_, id, l)| choice(id, l, dir))
            .collect(),
        T::Split { .. } => vec![
            choice("verticalOut", "Vertical Out", None),
            choice("verticalIn", "Vertical In", None),
            choice("horizontalOut", "Horizontal Out", None),
            choice("horizontalIn", "Horizontal In", None),
        ],
        T::Reveal { .. } => vec![
            choice("smoothlyFromRight", "Smoothly From Right", None),
            choice("smoothlyFromLeft", "Smoothly From Left", None),
            choice("throughBlackFromRight", "Through Black From Right", None),
            choice("throughBlackFromLeft", "Through Black From Left", None),
        ],
        T::RandomBars { .. } => vec![
            choice("vertical", "Vertical", None),
            choice("horizontal", "Horizontal", None),
        ],
        T::Shape { .. } => vec![
            choice("circle", "Circle", Some("Shape")),
            choice("diamond", "Diamond", Some("Shape")),
            choice("plus", "Plus", Some("Shape")),
            choice("in", "In", dir),
            choice("out", "Out", dir),
        ],
        T::Uncover { .. } | T::Cover { .. } => {
            DIRS8.iter().map(|(_, id, l)| choice(id, l, dir)).collect()
        }
        T::Morph { .. } => vec![
            choice("objects", "Objects", None),
            choice("words", "Words", None),
            choice("characters", "Characters", None),
        ],
    }
}

/// The option a transition shows as chosen (the shape's and the direction's for Shape: the
/// direction is returned).
#[must_use]
pub fn transition_option(e: &TransitionEffect) -> Option<String> {
    use TransitionEffect as T;
    let s = |x: &str| Some(x.to_owned());
    match *e {
        T::None | T::Flash => None,
        T::Fade { through_black } | T::Cut { through_black } => s(if through_black {
            "throughBlack"
        } else {
            "smoothly"
        }),
        T::Push { dir } | T::Wipe { dir } => SIDES
            .iter()
            .find(|(d, ..)| *d == dir)
            .map(|(_, id, _)| (*id).to_owned()),
        T::Split { vertical, inward } => s(match (vertical, inward) {
            (true, false) => "verticalOut",
            (true, true) => "verticalIn",
            (false, false) => "horizontalOut",
            (false, true) => "horizontalIn",
        }),
        T::Reveal { dir, through_black } => s(match (through_black, dir) {
            (false, Side::Right) => "smoothlyFromLeft",
            (false, _) => "smoothlyFromRight",
            (true, Side::Right) => "throughBlackFromLeft",
            (true, _) => "throughBlackFromRight",
        }),
        T::RandomBars { vertical } => s(if vertical { "vertical" } else { "horizontal" }),
        T::Shape { form, inward } => s(if inward {
            "in"
        } else {
            match form {
                ShapeForm::Circle => "circle",
                ShapeForm::Diamond => "diamond",
                ShapeForm::Plus => "plus",
            }
        }),
        T::Uncover { dir } | T::Cover { dir } => DIRS8
            .iter()
            .find(|(d, ..)| *d == dir)
            .map(|(_, id, _)| (*id).to_owned()),
        T::Morph { by } => s(match by {
            MorphBy::Objects => "objects",
            MorphBy::Words => "words",
            MorphBy::Characters => "characters",
        }),
    }
}

/// A transition with an option applied; `None` when the effect has no such option.
#[must_use]
pub fn with_transition_option(e: TransitionEffect, option: &str) -> Option<TransitionEffect> {
    use TransitionEffect as T;
    let side = || {
        SIDES
            .iter()
            .find(|(_, id, _)| *id == option)
            .map(|(d, ..)| *d)
    };
    let dir8 = || {
        DIRS8
            .iter()
            .find(|(_, id, _)| *id == option)
            .map(|(d, ..)| *d)
    };
    Some(match e {
        T::None | T::Flash => return None,
        T::Fade { .. } => T::Fade {
            through_black: match option {
                "throughBlack" => true,
                "smoothly" => false,
                _ => return None,
            },
        },
        T::Cut { .. } => T::Cut {
            through_black: match option {
                "throughBlack" => true,
                "smoothly" => false,
                _ => return None,
            },
        },
        T::Push { .. } => T::Push { dir: side()? },
        T::Wipe { .. } => T::Wipe { dir: side()? },
        T::Split { .. } => {
            let (vertical, inward) = match option {
                "verticalOut" => (true, false),
                "verticalIn" => (true, true),
                "horizontalOut" => (false, false),
                "horizontalIn" => (false, true),
                _ => return None,
            };
            T::Split { vertical, inward }
        }
        T::Reveal { .. } => {
            let (through_black, dir) = match option {
                "smoothlyFromRight" => (false, Side::Left),
                "smoothlyFromLeft" => (false, Side::Right),
                "throughBlackFromRight" => (true, Side::Left),
                "throughBlackFromLeft" => (true, Side::Right),
                _ => return None,
            };
            T::Reveal { dir, through_black }
        }
        T::RandomBars { .. } => T::RandomBars {
            vertical: match option {
                "vertical" => true,
                "horizontal" => false,
                _ => return None,
            },
        },
        T::Shape { form, inward } => match option {
            "circle" => T::Shape {
                form: ShapeForm::Circle,
                inward,
            },
            "diamond" => T::Shape {
                form: ShapeForm::Diamond,
                inward,
            },
            "plus" => T::Shape {
                form: ShapeForm::Plus,
                inward,
            },
            "in" => T::Shape { form, inward: true },
            "out" => T::Shape {
                form,
                inward: false,
            },
            _ => return None,
        },
        T::Uncover { .. } => T::Uncover { dir: dir8()? },
        T::Cover { .. } => T::Cover { dir: dir8()? },
        T::Morph { .. } => T::Morph {
            by: match option {
                "objects" => MorphBy::Objects,
                "words" => MorphBy::Words,
                "characters" => MorphBy::Characters,
                _ => return None,
            },
        },
    })
}

// ── animations ───────────────────────────────────────────────────────────────────────────────

/// The gallery id of an effect (`custom` for one kept from a file).
#[must_use]
pub fn effect_id(e: &AnimationEffect) -> &'static str {
    use AnimationEffect as A;
    match e {
        A::Appear => "appear",
        A::Fade => "fade",
        A::FlyIn { .. } => "flyIn",
        A::FloatIn { .. } => "floatIn",
        A::Split { .. } => "split",
        A::Wipe { .. } => "wipe",
        A::Zoom => "zoom",
        A::GrowTurn => "growTurn",
        A::Pulse => "pulse",
        A::ColorPulse { .. } => "colorPulse",
        A::Teeter => "teeter",
        A::Spin { .. } => "spin",
        A::GrowShrink { .. } => "growShrink",
        A::Disappear => "disappear",
        A::FadeOut => "fadeOut",
        A::FlyOut { .. } => "flyOut",
        A::ZoomOut => "zoomOut",
        A::Other { .. } => "custom",
    }
}

/// The effect of a gallery id with PowerPoint's default options.
#[must_use]
pub fn effect_of(id: &str) -> Option<AnimationEffect> {
    kiln_deck::animation::gallery()
        .into_iter()
        .find(|e| effect_id(e).eq_ignore_ascii_case(id))
}

/// The class's word for the pane.
#[must_use]
pub fn class_word(c: EffectClass) -> &'static str {
    match c {
        EffectClass::Entrance => "entrance",
        EffectClass::Emphasis => "emphasis",
        EffectClass::Exit => "exit",
        EffectClass::Path | EffectClass::Other => "path",
    }
}

/// Fly In and Fly Out, in PowerPoint's menu order.
const FLY: [(Direction8, &str, &str); 8] = [
    (Direction8::Down, "Bottom", "Bottom"),
    (Direction8::LeftDown, "BottomLeft", "Bottom-Left"),
    (Direction8::Left, "Left", "Left"),
    (Direction8::LeftUp, "TopLeft", "Top-Left"),
    (Direction8::Up, "Top", "Top"),
    (Direction8::RightUp, "TopRight", "Top-Right"),
    (Direction8::Right, "Right", "Right"),
    (Direction8::RightDown, "BottomRight", "Bottom-Right"),
];

const WIPE: [(Side, &str, &str); 4] = [
    (Side::Down, "fromBottom", "From Bottom"),
    (Side::Left, "fromLeft", "From Left"),
    (Side::Right, "fromRight", "From Right"),
    (Side::Up, "fromTop", "From Top"),
];

const SPIN: [(i32, &str, &str); 4] = [
    (90, "quarter", "Quarter Spin"),
    (180, "half", "Half Spin"),
    (360, "full", "Full Spin"),
    (720, "two", "Two Spins"),
];

const GROW: [(u32, &str, &str); 4] = [
    (25, "tiny", "Tiny"),
    (50, "smaller", "Smaller"),
    (150, "larger", "Larger"),
    (400, "huge", "Huge"),
];

const COLOURS: [(&str, &str); 6] = [
    ("accent1", "Accent 1"),
    ("accent2", "Accent 2"),
    ("accent3", "Accent 3"),
    ("accent4", "Accent 4"),
    ("accent5", "Accent 5"),
    ("accent6", "Accent 6"),
];

/// How a text shape's effect is built (Effect Options ▸ Sequence).
pub const SEQUENCE: [(&str, &str); 3] = [
    ("asOneObject", "As One Object"),
    ("allAtOnce", "All at Once"),
    ("byParagraph", "By Paragraph"),
];

/// The Effect Options of an effect (Sequence is added by the caller for text shapes).
#[must_use]
pub fn effect_options(e: &AnimationEffect) -> Vec<DeckChoice> {
    use AnimationEffect as A;
    let dir = Some("Direction");
    match e {
        A::FlyIn { .. } => FLY
            .iter()
            .map(|(_, id, l)| choice(&format!("from{id}"), &format!("From {l}"), dir))
            .collect(),
        A::FlyOut { .. } => FLY
            .iter()
            .map(|(_, id, l)| choice(&format!("to{id}"), &format!("To {l}"), dir))
            .collect(),
        A::FloatIn { .. } => vec![
            choice("up", "Float Up", dir),
            choice("down", "Float Down", dir),
        ],
        A::Split { .. } => vec![
            choice("horizontalIn", "Horizontal In", dir),
            choice("horizontalOut", "Horizontal Out", dir),
            choice("verticalIn", "Vertical In", dir),
            choice("verticalOut", "Vertical Out", dir),
        ],
        A::Wipe { .. } => WIPE.iter().map(|(_, id, l)| choice(id, l, dir)).collect(),
        A::Spin { .. } => {
            let mut v = vec![
                choice("clockwise", "Clockwise", dir),
                choice("counterclockwise", "Counterclockwise", dir),
            ];
            v.extend(SPIN.iter().map(|(_, id, l)| choice(id, l, Some("Amount"))));
            v
        }
        A::GrowShrink { .. } => GROW
            .iter()
            .map(|(_, id, l)| choice(id, l, Some("Amount")))
            .collect(),
        A::ColorPulse { .. } => COLOURS
            .iter()
            .map(|(id, l)| choice(id, l, Some("Colour")))
            .collect(),
        _ => Vec::new(),
    }
}

/// The option an effect shows as chosen.
#[must_use]
pub fn effect_option(e: &AnimationEffect) -> Option<String> {
    use AnimationEffect as A;
    match e {
        A::FlyIn { from } => FLY
            .iter()
            .find(|(d, ..)| d == from)
            .map(|(_, id, _)| format!("from{id}")),
        A::FlyOut { to } => FLY
            .iter()
            .find(|(d, ..)| d == to)
            .map(|(_, id, _)| format!("to{id}")),
        A::FloatIn { down } => Some(if *down { "down" } else { "up" }.into()),
        A::Split { vertical, inward } => Some(
            match (vertical, inward) {
                (false, true) => "horizontalIn",
                (false, false) => "horizontalOut",
                (true, true) => "verticalIn",
                (true, false) => "verticalOut",
            }
            .into(),
        ),
        A::Wipe { from } => WIPE
            .iter()
            .find(|(d, ..)| d == from)
            .map(|(_, id, _)| (*id).to_owned()),
        A::Spin { degrees } => SPIN
            .iter()
            .find(|(d, ..)| *d == degrees.abs())
            .map(|(_, id, _)| (*id).to_owned()),
        A::GrowShrink { scale_pct } => GROW
            .iter()
            .find(|(s, ..)| s == scale_pct)
            .map(|(_, id, _)| (*id).to_owned()),
        A::ColorPulse { color } => color.as_ref().and_then(|c| match &c.model {
            kiln_ooxml::drawingml::color::ColorModel::Scheme(name) => Some(name.clone()),
            _ => None,
        }),
        _ => None,
    }
}

/// An effect with an option applied; `None` when it has no such option.
#[must_use]
pub fn with_effect_option(e: &AnimationEffect, option: &str) -> Option<AnimationEffect> {
    use AnimationEffect as A;
    Some(match e {
        A::FlyIn { .. } => A::FlyIn {
            from: FLY
                .iter()
                .find(|(_, id, _)| format!("from{id}") == option)?
                .0,
        },
        A::FlyOut { .. } => A::FlyOut {
            to: FLY.iter().find(|(_, id, _)| format!("to{id}") == option)?.0,
        },
        A::FloatIn { .. } => A::FloatIn {
            down: match option {
                "down" => true,
                "up" => false,
                _ => return None,
            },
        },
        A::Split { .. } => {
            let (vertical, inward) = match option {
                "horizontalIn" => (false, true),
                "horizontalOut" => (false, false),
                "verticalIn" => (true, true),
                "verticalOut" => (true, false),
                _ => return None,
            };
            A::Split { vertical, inward }
        }
        A::Wipe { .. } => A::Wipe {
            from: WIPE.iter().find(|(_, id, _)| *id == option)?.0,
        },
        A::Spin { degrees } => {
            let amount = degrees.abs().max(1);
            let sign = degrees.signum().max(-1);
            let sign = if sign == 0 { 1 } else { sign };
            A::Spin {
                degrees: match option {
                    "clockwise" => amount,
                    "counterclockwise" => -amount,
                    _ => sign * SPIN.iter().find(|(_, id, _)| *id == option)?.0,
                },
            }
        }
        A::GrowShrink { .. } => A::GrowShrink {
            scale_pct: GROW.iter().find(|(_, id, _)| *id == option)?.0,
        },
        A::ColorPulse { .. } => {
            COLOURS.iter().find(|(id, _)| *id == option)?;
            A::ColorPulse {
                color: Some(kiln_ooxml::drawingml::color::DrawingColor::scheme(option)),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transition_has_an_id_and_its_options_round_trip() {
        for e in kiln_deck::transition::GALLERY {
            let id = transition_id(&e);
            assert_eq!(transition_of(id), Some(e), "{id}");
            for o in transition_options(&e) {
                let t = with_transition_option(e, &o.id).unwrap_or_else(|| panic!("{id} {}", o.id));
                assert_eq!(transition_id(&t), id);
                let shown = transition_option(&t);
                // Shape's two lists: the chosen shape, or In / Out.
                if !matches!(e, TransitionEffect::Shape { .. }) || o.id == "in" {
                    assert_eq!(shown.as_deref(), Some(o.id.as_str()), "{id}");
                }
            }
        }
        assert_eq!(transition_of("nope"), None);
        assert!(with_transition_option(TransitionEffect::Flash, "x").is_none());
        // Directions read as PowerPoint names them.
        assert_eq!(
            transition_option(&TransitionEffect::Push { dir: Side::Up }).as_deref(),
            Some("fromBottom")
        );
        assert_eq!(
            with_transition_option(TransitionEffect::Wipe { dir: Side::Up }, "fromRight"),
            Some(TransitionEffect::Wipe { dir: Side::Left })
        );
    }

    #[test]
    fn every_effect_has_an_id_and_its_options_round_trip() {
        for e in kiln_deck::animation::gallery() {
            let id = effect_id(&e);
            assert_eq!(effect_of(id).as_ref(), Some(&e), "{id}");
            for o in effect_options(&e) {
                let t = with_effect_option(&e, &o.id).unwrap_or_else(|| panic!("{id} {}", o.id));
                assert_eq!(effect_id(&t), id);
                if !matches!(e, AnimationEffect::Spin { .. })
                    || o.group.as_deref() == Some("Amount")
                {
                    assert_eq!(
                        effect_option(&t).as_deref(),
                        Some(o.id.as_str()),
                        "{id} {}",
                        o.id
                    );
                }
            }
        }
        let ccw = with_effect_option(&AnimationEffect::Spin { degrees: 180 }, "counterclockwise");
        assert_eq!(ccw, Some(AnimationEffect::Spin { degrees: -180 }));
        let quarter = with_effect_option(&AnimationEffect::Spin { degrees: -180 }, "quarter");
        assert_eq!(quarter, Some(AnimationEffect::Spin { degrees: -90 }));
        assert_eq!(class_word(EffectClass::Other), "path");
        assert!(with_effect_option(&AnimationEffect::Appear, "x").is_none());
    }
}
