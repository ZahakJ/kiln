// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck's motion and slide show: the arguments and answers of the Transitions,
//! Animations and Slide Show tabs, and the slide show itself. The commands are variants of
//! [`super::DeckCommand`]; the questions are variants of [`super::DeckQuery`].
//!
//! A show is played by the core: the chrome sends what the presenter does (next, previous,
//! a slide number, a pen stroke) and asks for frames with its clock's time
//! ([`super::DeckQuery::ShowFrame`]). A frame is a scene delta (the tiles of the slides being
//! shown: runs of shapes that do not move and every animated shape on its own) and a
//! composition: groups of layers, each a tile (or a colour) with a matrix, an opacity and a
//! clip, painted bottom to top. Lengths are slide px.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::scene::{DrawOp, Paint, PathData, SceneDelta, TileId};

/// A choice of a menu the core lists (Effect Options), with the group it is shown under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckChoice {
    /// What the command takes (`fromBottom`, `throughBlack`, …).
    pub id: String,
    /// What the menu shows ("From Bottom").
    pub label: String,
    /// The heading it is listed under ("Direction", "Sequence"), if any.
    pub group: Option<String>,
}

/// The current slide's transition, as the Transitions tab shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckTransitionInfo {
    /// The gallery entry (`none`, `fade`, `push`, `wipe`, `split`, `reveal`, `cut`,
    /// `randomBars`, `shape`, `uncover`, `cover`, `flash`, `morph`).
    pub effect: String,
    /// The effect option in force.
    pub option: Option<String>,
    /// The effect options of the effect.
    pub options: Vec<DeckChoice>,
    /// Duration, milliseconds.
    pub duration_ms: u32,
    /// Advance on mouse click.
    pub on_click: bool,
    /// Advance automatically after this many milliseconds.
    pub after_ms: Option<u32>,
    /// The file's transition is not one of the twelve: it plays as its nearest and is saved as
    /// it was.
    pub kept: bool,
}

/// When an effect starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckEffectStart {
    /// On Click.
    OnClick,
    /// With Previous.
    WithPrevious,
    /// After Previous.
    AfterPrevious,
}

/// One effect of the current slide's main sequence (a row of the Animation Pane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckAnimationInfo {
    /// The effect.
    pub id: NodeId,
    /// The shape it animates.
    pub shape: NodeId,
    /// The shape's name.
    pub shape_name: String,
    /// The paragraphs it animates (first, last), when it builds text by paragraph.
    pub paragraphs: Option<(u32, u32)>,
    /// The gallery entry (`appear`, `flyIn`, …; `custom` for an effect kept from a file).
    pub effect: String,
    /// Its name ("Fly In").
    pub name: String,
    /// `entrance`, `emphasis`, `exit` or `path` (motion paths and other kept effects).
    pub class: String,
    /// The effect option in force.
    pub option: Option<String>,
    /// When it starts.
    pub start: DeckEffectStart,
    /// Duration of one iteration, milliseconds.
    pub duration_ms: u32,
    /// Delay, milliseconds.
    pub delay_ms: u32,
    /// Iterations.
    pub repeat: u32,
    /// The click it runs on (the stage's badge; 0 runs by itself after the transition).
    pub click: u32,
    /// Start within its click, milliseconds (the pane's timeline).
    #[ts(type = "number")]
    pub begin_ms: u64,
    /// End within its click, milliseconds.
    #[ts(type = "number")]
    pub end_ms: u64,
    /// Kiln plays it as PowerPoint does (otherwise it is kept from the file and plays as its
    /// nearest).
    pub playable: bool,
}

/// How a show is presented (Set Up Slide Show ▸ Show type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum DeckShowType {
    /// Presented by a speaker (full screen).
    #[default]
    Speaker,
    /// Browsed by an individual (window).
    Window,
    /// Browsed at a kiosk (full screen, loops, timings only).
    Kiosk,
}

/// Set Up Slide Show (`p:showPr` of `presProps.xml`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct DeckShowSettings {
    /// Show type.
    pub kind: DeckShowType,
    /// Loop continuously until Esc.
    pub loop_show: bool,
    /// Show without narration.
    pub no_narration: bool,
    /// Show without animation.
    pub no_animation: bool,
    /// First slide shown (1-based), when not all.
    pub from: Option<u32>,
    /// Last slide shown (1-based), when not all.
    pub to: Option<u32>,
    /// Advance slides manually even where slides have timings.
    pub manual: bool,
    /// Use Presenter View.
    pub presenter_view: bool,
}

/// What a show is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum DeckShowMode {
    /// The slide show.
    #[default]
    Show,
    /// Rehearse Timings: the show with a timer that records each slide's time.
    Rehearse,
    /// Transitions ▸ Preview: the slide's transition on the stage.
    PreviewTransition,
    /// Animations ▸ Preview: the slide's effects on the stage, one click after another.
    PreviewAnimations,
}

/// A pointer tool of the show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum DeckShowTool {
    /// The arrow: clicks advance.
    #[default]
    Arrow,
    /// The laser pointer.
    Laser,
    /// The pen (ink on the slide).
    Pen,
    /// The highlighter.
    Highlighter,
    /// The eraser (removes strokes it touches).
    Eraser,
}

/// What a pointer did on the show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckPointerPhase {
    /// Pressed.
    Down,
    /// Moved (pressed or not).
    Move,
    /// Released.
    Up,
    /// Left the show.
    Leave,
}

/// What the show's screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum DeckShowScreen {
    /// The slide.
    #[default]
    Slide,
    /// Black (B).
    Black,
    /// White (W).
    White,
    /// "End of slide show": the next advance ends it.
    End,
}

/// The state of a show.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct DeckShowStatus {
    /// A show is running.
    pub running: bool,
    /// What for.
    pub mode: DeckShowMode,
    /// The slide shown (index in the presentation).
    pub slide: u32,
    /// Its position in the show (1-based).
    pub position: u32,
    /// Slides in the show.
    pub count: u32,
    /// Clicks done on the slide.
    pub click: u32,
    /// Clicks the slide has.
    pub clicks: u32,
    /// What the screen shows.
    pub screen: DeckShowScreen,
    /// The pointer tool.
    pub tool: DeckShowTool,
    /// A slide number being typed (number, then Enter).
    pub typed: String,
    /// The next slide of the show (index), if any.
    pub next: Option<u32>,
    /// Mouse clicks advance the current slide.
    pub on_click: bool,
    /// How the show is presented.
    pub kind: DeckShowType,
    /// Bumped whenever any of the above changes.
    #[ts(type = "number")]
    pub version: u64,
}

/// A linear opacity ramp (the soft edge of Wipe and Split): along the line from `from` to `to`,
/// in slide px, the opacity at each stop's offset (0–1 along the line), held before the first
/// stop and after the last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckShowRamp {
    /// Where offset 0 is.
    pub from: [f32; 2],
    /// Where offset 1 is.
    pub to: [f32; 2],
    /// `(offset, opacity)`, in increasing offset.
    pub stops: Vec<(f32, f32)>,
}

/// One layer of a show frame: a tile of the frame's scene (or a colour over the slide) drawn
/// with a matrix, an opacity and a clip in its own coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckShowLayer {
    /// The tile, or `None` for [`DeckShowLayer::fill`] over the whole slide.
    pub tile: Option<TileId>,
    /// The colour drawn when there is no tile.
    pub fill: Option<Paint>,
    /// Matrix, slide px to slide px (canvas `transform()` order).
    pub matrix: [f32; 6],
    /// Opacity, 0–1.
    pub opacity: f32,
    /// Clip in the layer's coordinates (before the matrix).
    pub clip: Option<PathData>,
    /// An opacity ramp in the layer's coordinates.
    pub ramp: Option<DeckShowRamp>,
}

/// One slide of a show frame (or a colour plate): its layers, moved, faded and clipped together
/// by the transition. A group with an opacity below 1 and several layers is composed on its
/// own before it is faded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckShowGroup {
    /// Matrix applied to every layer.
    pub matrix: [f32; 6],
    /// Opacity of the whole group.
    pub opacity: f32,
    /// Clip in slide px (not moved by the matrix).
    pub clip: Option<PathData>,
    /// An opacity ramp over the whole group, in slide px (not moved by the matrix).
    pub ramp: Option<DeckShowRamp>,
    /// Layers, bottom to top.
    pub layers: Vec<DeckShowLayer>,
}

/// A frame of the show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckShowFrame {
    /// Tiles, glyphs and pictures the painter lacks (a reset when the size changed).
    pub delta: SceneDelta,
    /// The composition, bottom to top.
    pub groups: Vec<DeckShowGroup>,
    /// Ink, the laser pointer and the like over the slide, in slide px.
    pub ink: Vec<DrawOp>,
    /// Slide width, px.
    pub slide_width: f32,
    /// Slide height, px.
    pub slide_height: f32,
    /// Something is still moving: ask for the next frame.
    pub animating: bool,
    /// Milliseconds until the show advances by itself (slide timings), if it will.
    pub advance_in_ms: Option<u32>,
    /// The show's state.
    pub status: DeckShowStatus,
}

/// What Presenter View shows beside the slides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckShowInfo {
    /// The show's state.
    pub status: DeckShowStatus,
    /// The current slide's speaker notes.
    pub notes: String,
    /// The current slide's title.
    pub title: String,
    /// The slides of the show (indices in the presentation), in order.
    pub slides: Vec<u32>,
    /// Rehearse Timings: the time of each slide so far, milliseconds (index = position).
    pub rehearsed: Vec<u32>,
}

/// The Transitions, Animations and Slide Show tabs' state (part of the Deck state).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct DeckMotionState {
    /// The current slide's transition.
    pub transition: Option<DeckTransitionInfo>,
    /// The current slide's effects, in order.
    pub animations: Vec<DeckAnimationInfo>,
    /// The selected effects (the pane's selection, or the selected shapes' effects).
    pub selected: Vec<NodeId>,
    /// The Effect Options of the selected effect.
    pub options: Vec<DeckChoice>,
    /// The effect option in force on the selected effect.
    pub option: Option<String>,
    /// The Animation Painter holds effects to paste.
    pub painter: bool,
    /// Effect numbers are shown on the stage.
    pub badges: bool,
    /// Set Up Slide Show.
    pub settings: DeckShowSettings,
    /// The show's state.
    pub show: DeckShowStatus,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the show types (see `crate::samples`).

    use super::*;
    use crate::samples::Samples;

    pub(crate) fn status() -> DeckShowStatus {
        DeckShowStatus {
            running: true,
            mode: DeckShowMode::Show,
            slide: 2,
            position: 3,
            count: 12,
            click: 1,
            clicks: 4,
            screen: DeckShowScreen::Slide,
            tool: DeckShowTool::Pen,
            typed: "1".into(),
            next: Some(3),
            on_click: true,
            kind: DeckShowType::Speaker,
            version: 9_007_199_254_740_991,
        }
    }

    pub(crate) fn transition() -> DeckTransitionInfo {
        DeckTransitionInfo {
            effect: "push".into(),
            option: Some("fromBottom".into()),
            options: vec![choice()],
            duration_ms: 1000,
            on_click: true,
            after_ms: Some(5000),
            kept: false,
        }
    }

    fn choice() -> DeckChoice {
        DeckChoice {
            id: "fromBottom".into(),
            label: "From Bottom".into(),
            group: Some("Direction".into()),
        }
    }

    pub(crate) fn animation() -> DeckAnimationInfo {
        DeckAnimationInfo {
            id: NodeId(31),
            shape: NodeId(9),
            shape_name: "Title 1".into(),
            paragraphs: Some((0, 2)),
            effect: "flyIn".into(),
            name: "Fly In".into(),
            class: "entrance".into(),
            option: Some("fromBottom".into()),
            start: DeckEffectStart::AfterPrevious,
            duration_ms: 500,
            delay_ms: 250,
            repeat: 1,
            click: 2,
            begin_ms: 250,
            end_ms: 750,
            playable: true,
        }
    }

    pub(crate) fn motion() -> DeckMotionState {
        DeckMotionState {
            transition: Some(transition()),
            animations: vec![animation()],
            selected: vec![NodeId(31)],
            options: vec![choice()],
            option: Some("fromBottom".into()),
            painter: false,
            badges: true,
            settings: settings(),
            show: status(),
        }
    }

    pub(crate) fn settings() -> DeckShowSettings {
        DeckShowSettings {
            kind: DeckShowType::Kiosk,
            loop_show: true,
            no_narration: false,
            no_animation: true,
            from: Some(2),
            to: Some(5),
            manual: false,
            presenter_view: true,
        }
    }

    fn layer() -> DeckShowLayer {
        DeckShowLayer {
            tile: Some(TileId::content(2049)),
            fill: None,
            matrix: [1.0, 0.0, 0.0, 1.0, -640.0, 0.0],
            opacity: 0.5,
            clip: Some(PathData::rect(crate::scene::Bounds::new(
                0.0, 0.0, 320.0, 720.0,
            ))),
            ramp: Some(ramp()),
        }
    }

    fn ramp() -> DeckShowRamp {
        DeckShowRamp {
            from: [640.0, 360.0],
            to: [793.6, 360.0],
            stops: vec![(0.0, 1.0), (1.0, 0.0)],
        }
    }

    pub(crate) fn frame() -> DeckShowFrame {
        DeckShowFrame {
            delta: crate::scene::samples::small_delta(),
            groups: vec![DeckShowGroup {
                matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                opacity: 1.0,
                clip: None,
                ramp: None,
                layers: vec![
                    layer(),
                    DeckShowLayer {
                        tile: None,
                        fill: Some(Paint::Rgba(0x0000_00ff)),
                        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        opacity: 1.0,
                        clip: None,
                        ramp: None,
                    },
                ],
            }],
            ink: Vec::new(),
            slide_width: 1280.0,
            slide_height: 720.0,
            animating: true,
            advance_in_ms: Some(1500),
            status: status(),
        }
    }

    pub(crate) fn info() -> DeckShowInfo {
        DeckShowInfo {
            status: status(),
            notes: "Pause on the chart.".into(),
            title: "Results".into(),
            slides: vec![0, 1, 3],
            rehearsed: vec![12_000, 4_500],
        }
    }

    macro_rules! samples_of {
        ($($ty:ty => [$($v:expr),* $(,)?];)*) => {
            $(impl Samples for $ty {
                fn samples() -> Vec<Self> {
                    vec![$($v),*]
                }
            })*
        };
    }

    samples_of! {
        DeckChoice => [choice()];
        DeckTransitionInfo => [transition()];
        DeckEffectStart => [DeckEffectStart::OnClick, DeckEffectStart::AfterPrevious];
        DeckAnimationInfo => [animation()];
        DeckShowType => [DeckShowType::Speaker, DeckShowType::Window];
        DeckShowSettings => [DeckShowSettings::default(), settings()];
        DeckShowMode => [DeckShowMode::Show, DeckShowMode::PreviewAnimations];
        DeckShowTool => [DeckShowTool::Arrow, DeckShowTool::Highlighter];
        DeckPointerPhase => [DeckPointerPhase::Down, DeckPointerPhase::Leave];
        DeckShowScreen => [DeckShowScreen::Black, DeckShowScreen::End];
        DeckShowStatus => [DeckShowStatus::default(), status()];
        DeckShowRamp => [ramp()];
        DeckShowLayer => [layer()];
        DeckShowGroup => [frame().groups[0].clone()];
        DeckShowFrame => [frame()];
        DeckShowInfo => [info()];
        DeckMotionState => [DeckMotionState::default(), motion()];
    }

    /// Registers the show types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            DeckChoice, DeckTransitionInfo, DeckEffectStart, DeckAnimationInfo, DeckShowType,
            DeckShowSettings, DeckShowMode, DeckShowTool, DeckPointerPhase, DeckShowScreen,
            DeckShowStatus, DeckShowRamp, DeckShowLayer, DeckShowGroup, DeckShowFrame, DeckShowInfo,
            DeckMotionState,
        );
    }
}
