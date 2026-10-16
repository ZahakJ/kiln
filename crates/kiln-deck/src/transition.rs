// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slide transitions (`p:transition`): the twelve effects of the Transitions tab (PLAN.md §1.4)
//! plus Morph (P2), their effect options, duration and advance settings, and the mapping of
//! every PowerPoint transition element onto them (unknown ones fall back to Fade, with the
//! original XML kept so a save writes it back).

use kiln_ooxml::xml::XmlElement;
use serde::{Deserialize, Serialize};

/// A side, as the `dir` of `p:push`, `p:wipe` and `p14:reveal` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Side {
    /// `l`.
    #[default]
    Left,
    /// `r`.
    Right,
    /// `u`.
    Up,
    /// `d`.
    Down,
}

impl Side {
    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Right => "r",
            Self::Up => "u",
            Self::Down => "d",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "l" => Self::Left,
            "r" => Self::Right,
            "u" => Self::Up,
            "d" => Self::Down,
            _ => return None,
        })
    }
}

/// One of eight directions (`ST_TransitionEightDirectionType`, Cover and Uncover).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Direction8 {
    /// `l`.
    #[default]
    Left,
    /// `r`.
    Right,
    /// `u`.
    Up,
    /// `d`.
    Down,
    /// `lu`.
    LeftUp,
    /// `ru`.
    RightUp,
    /// `ld`.
    LeftDown,
    /// `rd`.
    RightDown,
}

impl Direction8 {
    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Right => "r",
            Self::Up => "u",
            Self::Down => "d",
            Self::LeftUp => "lu",
            Self::RightUp => "ru",
            Self::LeftDown => "ld",
            Self::RightDown => "rd",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "l" => Self::Left,
            "r" => Self::Right,
            "u" => Self::Up,
            "d" => Self::Down,
            "lu" => Self::LeftUp,
            "ru" => Self::RightUp,
            "ld" => Self::LeftDown,
            "rd" => Self::RightDown,
            _ => return None,
        })
    }
}

/// The form of the Shape transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ShapeForm {
    /// `p:circle`.
    #[default]
    Circle,
    /// `p:diamond`.
    Diamond,
    /// `p:plus`.
    Plus,
}

/// What Morph matches by (P2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MorphBy {
    /// `byObject`.
    #[default]
    Objects,
    /// `byWord`.
    Words,
    /// `byChar`.
    Characters,
}

/// A transition effect with its options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TransitionEffect {
    /// No transition.
    #[default]
    None,
    /// Fade (smoothly, or through black).
    Fade {
        /// `thruBlk`.
        through_black: bool,
    },
    /// Push the old slide off.
    Push {
        /// The direction.
        dir: Side,
    },
    /// Wipe.
    Wipe {
        /// The direction.
        dir: Side,
    },
    /// Split.
    Split {
        /// `orient="vert"`.
        vertical: bool,
        /// `dir="in"`.
        inward: bool,
    },
    /// Reveal (`p14:reveal`).
    Reveal {
        /// The direction (left or right).
        dir: Side,
        /// `thruBlk`.
        through_black: bool,
    },
    /// Cut.
    Cut {
        /// `thruBlk`.
        through_black: bool,
    },
    /// Random Bars (P1).
    RandomBars {
        /// `dir="vert"`.
        vertical: bool,
    },
    /// Shape (circle, diamond, plus).
    Shape {
        /// The form.
        form: ShapeForm,
        /// Shrinks inwards instead of growing out.
        inward: bool,
    },
    /// Uncover (`p:pull`).
    Uncover {
        /// The direction.
        dir: Direction8,
    },
    /// Cover.
    Cover {
        /// The direction.
        dir: Direction8,
    },
    /// Flash (`p14:flash`).
    Flash,
    /// Morph (`p159:morph`, P2: shown as Fade until it is implemented).
    Morph {
        /// What matches.
        by: MorphBy,
    },
}

/// The gallery order of the Transitions tab.
pub const GALLERY: [TransitionEffect; 13] = [
    TransitionEffect::None,
    TransitionEffect::Morph {
        by: MorphBy::Objects,
    },
    TransitionEffect::Fade {
        through_black: false,
    },
    TransitionEffect::Push { dir: Side::Up },
    TransitionEffect::Wipe { dir: Side::Left },
    TransitionEffect::Split {
        vertical: true,
        inward: false,
    },
    TransitionEffect::Reveal {
        dir: Side::Left,
        through_black: false,
    },
    TransitionEffect::Cut {
        through_black: false,
    },
    TransitionEffect::RandomBars { vertical: false },
    TransitionEffect::Shape {
        form: ShapeForm::Circle,
        inward: false,
    },
    TransitionEffect::Uncover {
        dir: Direction8::Left,
    },
    TransitionEffect::Cover {
        dir: Direction8::Left,
    },
    TransitionEffect::Flash,
];

impl TransitionEffect {
    /// The English name in the gallery.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Fade { .. } => "Fade",
            Self::Push { .. } => "Push",
            Self::Wipe { .. } => "Wipe",
            Self::Split { .. } => "Split",
            Self::Reveal { .. } => "Reveal",
            Self::Cut { .. } => "Cut",
            Self::RandomBars { .. } => "Random Bars",
            Self::Shape { .. } => "Shape",
            Self::Uncover { .. } => "Uncover",
            Self::Cover { .. } => "Cover",
            Self::Flash => "Flash",
            Self::Morph { .. } => "Morph",
        }
    }

    /// What a show plays for an effect it cannot play.
    pub const FALLBACK: TransitionEffect = TransitionEffect::Fade {
        through_black: false,
    };

    /// PowerPoint's default duration for the effect, milliseconds.
    #[must_use]
    pub const fn default_duration_ms(&self) -> u32 {
        match self {
            Self::None | Self::Cut { .. } => 0,
            Self::Fade { .. } | Self::Flash => 700,
            Self::Push { .. } | Self::Wipe { .. } | Self::Uncover { .. } | Self::Cover { .. } => {
                1000
            }
            Self::Split { .. } => 1500,
            Self::Reveal { .. } | Self::Shape { .. } | Self::Morph { .. } => 2000,
            Self::RandomBars { .. } => 1000,
        }
    }

    /// Reads a `p:transition` child (`p:fade`, `p14:reveal`, `p15:prstTrans`, `p159:morph`, …).
    /// Returns the effect and whether it is exact; anything else maps to the nearest of the
    /// twelve (and the caller keeps the element for the writer).
    #[must_use]
    pub fn from_element(e: &XmlElement) -> (TransitionEffect, bool) {
        let local = kiln_ooxml::xml::namespaces::local_name(&e.name);
        let thru = e.attr_bool("thruBlk").unwrap_or(false);
        let side = |default: Side| e.attr("dir").and_then(Side::parse).unwrap_or(default);
        let dir8 = || {
            e.attr("dir")
                .and_then(Direction8::parse)
                .unwrap_or_default()
        };
        let vertical_orient = e.attr("orient") == Some("vert");
        let exact = |t| (t, true);
        let near = |t| (t, false);
        match local {
            "fade" => exact(Self::Fade {
                through_black: thru,
            }),
            "cut" => exact(Self::Cut {
                through_black: thru,
            }),
            "push" => exact(Self::Push {
                dir: side(Side::Left),
            }),
            "wipe" => exact(Self::Wipe {
                dir: side(Side::Left),
            }),
            "split" => exact(Self::Split {
                vertical: vertical_orient,
                inward: e.attr("dir") == Some("in"),
            }),
            "reveal" => exact(Self::Reveal {
                dir: side(Side::Left),
                through_black: thru,
            }),
            "randomBar" => exact(Self::RandomBars {
                vertical: e.attr("dir") == Some("vert"),
            }),
            "circle" => exact(Self::Shape {
                form: ShapeForm::Circle,
                inward: false,
            }),
            "diamond" => exact(Self::Shape {
                form: ShapeForm::Diamond,
                inward: false,
            }),
            "plus" => exact(Self::Shape {
                form: ShapeForm::Plus,
                inward: false,
            }),
            "pull" => exact(Self::Uncover { dir: dir8() }),
            "cover" => exact(Self::Cover { dir: dir8() }),
            "flash" => exact(Self::Flash),
            "morph" => exact(Self::Morph {
                by: match e.attr("option") {
                    Some("byWord") => MorphBy::Words,
                    Some("byChar") => MorphBy::Characters,
                    _ => MorphBy::Objects,
                },
            }),
            // Nearest relatives.
            "zoom" => near(Self::Shape {
                form: ShapeForm::Circle,
                inward: e.attr("dir") == Some("in"),
            }),
            "blinds" | "checker" | "comb" | "strips" | "shred" | "honeycomb" | "glitter" => {
                near(Self::RandomBars {
                    vertical: e.attr("dir") == Some("vert"),
                })
            }
            "wheel" | "wedge" | "wheelReverse" | "ripple" | "doors" | "window" => {
                near(Self::Wipe {
                    dir: side(Side::Left),
                })
            }
            "pan" | "conveyor" | "ferris" | "gallery" | "switch" | "flip" | "cube" | "rotate"
            | "box" | "orbit" | "flythrough" => near(Self::Push {
                dir: side(Side::Left),
            }),
            _ => near(Self::FALLBACK),
        }
    }

    /// The `p:transition` child for the effect (`None` for no transition). Reveal and Flash are
    /// PowerPoint 2010 effects (`p14:`) and Morph a 2019 one (`p159:`); the pptx writer wraps
    /// those in `mc:AlternateContent` with a Fade fallback.
    #[must_use]
    pub fn to_element(&self) -> Option<XmlElement> {
        let bool_attr = |el: XmlElement, name: &str, on: bool| {
            if on { el.with_attr(name, "1") } else { el }
        };
        Some(match *self {
            Self::None => return None,
            Self::Fade { through_black } => {
                bool_attr(XmlElement::new("p:fade"), "thruBlk", through_black)
            }
            Self::Cut { through_black } => {
                bool_attr(XmlElement::new("p:cut"), "thruBlk", through_black)
            }
            Self::Push { dir } => XmlElement::new("p:push").with_attr("dir", dir.as_str()),
            Self::Wipe { dir } => XmlElement::new("p:wipe").with_attr("dir", dir.as_str()),
            Self::Split { vertical, inward } => XmlElement::new("p:split")
                .with_attr("orient", if vertical { "vert" } else { "horz" })
                .with_attr("dir", if inward { "in" } else { "out" }),
            Self::Reveal { dir, through_black } => bool_attr(
                XmlElement::new("p14:reveal").with_attr("dir", dir.as_str()),
                "thruBlk",
                through_black,
            ),
            Self::RandomBars { vertical } => XmlElement::new("p:randomBar")
                .with_attr("dir", if vertical { "vert" } else { "horz" }),
            Self::Shape { form, inward } => {
                if inward {
                    XmlElement::new("p:zoom").with_attr("dir", "in")
                } else {
                    XmlElement::new(match form {
                        ShapeForm::Circle => "p:circle",
                        ShapeForm::Diamond => "p:diamond",
                        ShapeForm::Plus => "p:plus",
                    })
                }
            }
            Self::Uncover { dir } => XmlElement::new("p:pull").with_attr("dir", dir.as_str()),
            Self::Cover { dir } => XmlElement::new("p:cover").with_attr("dir", dir.as_str()),
            Self::Flash => XmlElement::new("p14:flash"),
            Self::Morph { by } => XmlElement::new("p159:morph").with_attr(
                "option",
                match by {
                    MorphBy::Objects => "byObject",
                    MorphBy::Words => "byWord",
                    MorphBy::Characters => "byChar",
                },
            ),
        })
    }
}

/// A slide's transition and advance settings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transition {
    /// The effect.
    pub effect: TransitionEffect,
    /// Duration, milliseconds (`p14:dur`).
    pub duration_ms: u32,
    /// Advance on mouse click (`advClick`, default on).
    pub advance_on_click: bool,
    /// Advance automatically after this many milliseconds (`advTm`).
    pub advance_after_ms: Option<u32>,
    /// The original `p:transition` child when the effect was not exact, written back on save.
    pub original: Option<String>,
}

impl Default for Transition {
    fn default() -> Self {
        Self::new(TransitionEffect::None)
    }
}

impl Transition {
    /// An effect with its default duration, advancing on click.
    #[must_use]
    pub const fn new(effect: TransitionEffect) -> Self {
        Self {
            effect,
            duration_ms: effect.default_duration_ms(),
            advance_on_click: true,
            advance_after_ms: None,
            original: None,
        }
    }

    /// The effect a show plays: Morph falls back to Fade until it is implemented (P2).
    #[must_use]
    pub const fn playable(&self) -> TransitionEffect {
        match self.effect {
            TransitionEffect::Morph { .. } => TransitionEffect::FALLBACK,
            e => e,
        }
    }

    /// Duration from `p:transition/@spd` (fast/med/slow) when a file has no `p14:dur`.
    #[must_use]
    pub fn duration_from_speed(spd: &str) -> u32 {
        match spd {
            "fast" => 500,
            "med" => 750,
            _ => 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gallery_effect_round_trips_through_its_element() {
        for e in GALLERY {
            match e.to_element() {
                None => assert_eq!(e, TransitionEffect::None),
                Some(el) => assert_eq!(TransitionEffect::from_element(&el), (e, true), "{e:?}"),
            }
        }
        // Every option too.
        let more = [
            TransitionEffect::Fade {
                through_black: true,
            },
            TransitionEffect::Cut {
                through_black: true,
            },
            TransitionEffect::Push { dir: Side::Down },
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
            TransitionEffect::Uncover {
                dir: Direction8::RightDown,
            },
            TransitionEffect::Cover {
                dir: Direction8::LeftUp,
            },
            TransitionEffect::Morph { by: MorphBy::Words },
            TransitionEffect::Morph {
                by: MorphBy::Characters,
            },
        ];
        for e in more {
            let el = e.to_element().unwrap();
            assert_eq!(TransitionEffect::from_element(&el), (e, true), "{e:?}");
        }
    }

    #[test]
    fn unknown_effects_fall_back_to_their_nearest_relative() {
        let el = |n: &str| XmlElement::new(n);
        assert_eq!(
            TransitionEffect::from_element(&el("p:dissolve")),
            (TransitionEffect::FALLBACK, false)
        );
        assert_eq!(
            TransitionEffect::from_element(&el("p15:prstTrans")),
            (TransitionEffect::FALLBACK, false)
        );
        assert!(matches!(
            TransitionEffect::from_element(&el("p:blinds")).0,
            TransitionEffect::RandomBars { .. }
        ));
        assert!(matches!(
            TransitionEffect::from_element(&el("p:wheel")).0,
            TransitionEffect::Wipe { .. }
        ));
        assert!(matches!(
            TransitionEffect::from_element(&el("p14:conveyor")).0,
            TransitionEffect::Push { .. }
        ));
        let zoom_in = XmlElement::new("p:zoom").with_attr("dir", "in");
        assert_eq!(
            TransitionEffect::from_element(&zoom_in),
            (
                TransitionEffect::Shape {
                    form: ShapeForm::Circle,
                    inward: true
                },
                false
            )
        );
    }

    #[test]
    fn defaults_and_playback() {
        let t = Transition::new(TransitionEffect::Fade {
            through_black: false,
        });
        assert_eq!(t.duration_ms, 700);
        assert!(t.advance_on_click);
        assert_eq!(Transition::default().effect, TransitionEffect::None);
        let m = Transition::new(TransitionEffect::Morph {
            by: MorphBy::Objects,
        });
        assert_eq!(m.playable(), TransitionEffect::FALLBACK);
        assert_eq!(Transition::duration_from_speed("fast"), 500);
        assert_eq!(Transition::duration_from_speed("med"), 750);
        assert_eq!(Transition::duration_from_speed("slow"), 1000);
        let names: Vec<_> = GALLERY.iter().map(TransitionEffect::name).collect();
        assert_eq!(names.len(), 13);
        assert!(names.contains(&"Random Bars"));
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
            assert_eq!(Direction8::parse(d.as_str()), Some(d));
        }
        assert_eq!(Side::parse("x"), None);
        assert_eq!(Direction8::parse("x"), None);
    }
}
