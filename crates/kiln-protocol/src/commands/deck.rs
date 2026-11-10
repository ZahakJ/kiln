// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck commands and queries: slides, shapes, transforms on the stage, text in
//! shapes, formatting, design, notes and the view. The stage sends pointer gestures as they
//! happen ([`DeckCommand::PointerDown`] …) and the core decides what they do — select, move,
//! resize, rotate, marquee, place the caret — with smart guides and snapping computed in Rust.
//! Lengths are slide px (EMU / 9525) unless a field says points.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::write::CaretMotion;
use crate::app::AppMask;

pub mod content;
pub mod features;
pub mod review;
pub mod show;

use features::{DeckMediaKind, DeckOutlineEdit, DeckPictureFormat};
use show::{DeckEffectStart, DeckPointerPhase, DeckShowMode, DeckShowSettings};

/// A rectangle on the slide, in slide px.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckRect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// A direction of the arrow keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckDirection {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
}

/// Align objects (Arrange ▸ Align).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckAlign {
    /// Align Left.
    Left,
    /// Align Center.
    Center,
    /// Align Right.
    Right,
    /// Align Top.
    Top,
    /// Align Middle.
    Middle,
    /// Align Bottom.
    Bottom,
}

/// Z-order (Arrange ▸ Order Objects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckOrder {
    /// Bring to Front.
    Front,
    /// Bring Forward.
    Forward,
    /// Send Backward.
    Backward,
    /// Send to Back.
    Back,
}

/// A fill chosen in Shape Fill or Format Background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckFill {
    /// No Fill.
    None,
    /// A colour: `RRGGBB` or a theme slot (`accent1`, `tx1`, `bg2`, …), with an optional tint
    /// (+) or shade (−) in percent and transparency in percent.
    Solid {
        /// The colour.
        color: String,
        /// Lighter (+) or darker (−), percent.
        brightness: Option<f32>,
        /// Transparency, percent.
        transparency: Option<f32>,
    },
    /// A linear gradient between two colours at an angle (degrees, clockwise from left to
    /// right).
    Gradient {
        /// Start colour.
        from: String,
        /// End colour.
        to: String,
        /// Angle.
        angle: f32,
    },
    /// A picture (bytes of an image file).
    Picture {
        /// File name (its extension picks the media type).
        name: String,
        /// The image file.
        bytes: Vec<u8>,
    },
    /// A preset pattern (`ltUpDiag`, `pct20`, …) of a foreground and background colour.
    Pattern {
        /// The DrawingML preset.
        preset: String,
        /// Foreground.
        fg: String,
        /// Background.
        bg: String,
    },
}

/// An outline chosen in Shape Outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckOutline {
    /// No Outline.
    None,
    /// A colour (as [`DeckFill::Solid`]).
    Color {
        /// The colour.
        color: String,
    },
    /// A weight in points.
    Weight {
        /// Points.
        points: f32,
    },
    /// A dash preset (`solid`, `sysDot`, `sysDash`, `dash`, `dashDot`, `lgDash`, …).
    Dash {
        /// The DrawingML preset.
        dash: String,
    },
    /// Arrowheads (`none`, `triangle`, `stealth`, `arrow`, `oval`, `diamond`).
    Arrows {
        /// At the start.
        head: String,
        /// At the end.
        tail: String,
    },
}

/// An effect chosen in Shape Effects.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckEffect {
    /// No effects.
    None,
    /// An outer shadow: offset distance and blur in points, direction in degrees.
    Shadow {
        /// Offset.
        distance: f32,
        /// Blur.
        blur: f32,
        /// Direction.
        angle: f32,
    },
    /// No shadow.
    NoShadow,
    /// A glow of a radius in points in the accent colour.
    Glow {
        /// Radius.
        radius: f32,
    },
    /// Soft edges of a radius in points.
    SoftEdges {
        /// Radius.
        radius: f32,
    },
}

/// Vertical text alignment (Align Text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckAnchor {
    /// Top.
    Top,
    /// Middle.
    Middle,
    /// Bottom.
    Bottom,
}

/// Text direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckTextDirection {
    /// Horizontal.
    Horizontal,
    /// Rotate all text 90°.
    Rotate90,
    /// Rotate all text 270°.
    Rotate270,
    /// Stacked.
    Stacked,
}

/// Text autofit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckAutofit {
    /// Do not Autofit.
    Off,
    /// Shrink text on overflow.
    Shrink,
    /// Resize shape to fit text.
    ResizeShape,
}

/// Slide sizes (Design ▸ Slide Size).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckSlideSize {
    /// Widescreen 16:9 (13.333 × 7.5 in).
    Widescreen,
    /// Standard 4:3 (10 × 7.5 in).
    Standard,
    /// A size in inches.
    Custom {
        /// Width.
        width_in: f32,
        /// Height.
        height_in: f32,
    },
}

/// How content follows a slide size change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckScaleMode {
    /// Maximize: content keeps its size.
    Maximize,
    /// Ensure Fit: content scales down to fit.
    EnsureFit,
}

/// Kiln Deck commands (the PowerPoint ribbon and stage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckCommand {
    // ── Slides ────────────────────────────────────────────────────────────────────────────
    /// New Slide after the current one (Ctrl+M), on a layout (default: the current slide's,
    /// or Title and Content after a title slide).
    NewSlide {
        /// The layout.
        layout: Option<NodeId>,
    },
    /// Duplicate (Ctrl+D): the selected objects, or the selected slides.
    Duplicate,
    /// Delete the selected slides.
    DeleteSlides,
    /// Move the selected slides to before the slide at `to` (in the order before the move).
    MoveSlides {
        /// Target index (0 = first; the slide count = after the last).
        to: u32,
    },
    /// Select a slide (the rail): `extend` selects the range from the current one (Shift),
    /// `toggle` adds or removes it (Ctrl).
    SelectSlide {
        /// Slide index.
        index: u32,
        /// Shift.
        extend: bool,
        /// Ctrl.
        toggle: bool,
    },
    /// Previous slide (Ctrl+Page Up).
    PreviousSlide,
    /// Next slide (Ctrl+Page Down).
    NextSlide,
    /// Change the layout of the selected slides.
    SetLayout {
        /// The layout.
        layout: NodeId,
    },
    /// Reset the selected slides' placeholders to their layout.
    ResetSlides,
    /// Hide Slide (toggle when `hidden` is `None`).
    HideSlides {
        /// Hidden or shown.
        hidden: Option<bool>,
    },
    /// Add a section starting at the current slide.
    AddSection {
        /// Its name (default "Untitled Section").
        name: Option<String>,
    },
    /// Rename a section.
    RenameSection {
        /// Section index.
        index: u32,
        /// New name.
        name: String,
    },
    /// Remove a section (its slides join the previous section, or are deleted).
    RemoveSection {
        /// Section index.
        index: u32,
        /// Delete its slides too.
        with_slides: bool,
    },
    /// Move a section up or down.
    MoveSection {
        /// Section index.
        index: u32,
        /// −1 up, +1 down.
        delta: i32,
    },
    /// The current slide's speaker notes (plain text, one paragraph per line).
    SetNotes {
        /// The text.
        text: String,
    },
    // ── Insert ───────────────────────────────────────────────────────────────────────────
    /// Insert a preset shape (`rect`, `ellipse`, `rightArrow`, `line`, …), at `rect` or at the
    /// default size in the middle of the slide.
    InsertShape {
        /// ST_ShapeType name.
        preset: String,
        /// Where.
        rect: Option<DeckRect>,
    },
    /// Insert a text box (and edit its text).
    InsertTextBox {
        /// Where.
        rect: Option<DeckRect>,
    },
    /// Insert a picture.
    InsertPicture {
        /// File name.
        name: String,
        /// The image file.
        bytes: Vec<u8>,
        /// Where (default: fitted in the middle, or into the selected picture placeholder).
        rect: Option<DeckRect>,
    },
    /// Insert a table of `rows` × `cols`.
    InsertTable {
        /// Rows.
        rows: u32,
        /// Columns.
        cols: u32,
    },
    /// Insert the slide number (into the text being edited, else as the slide's footer).
    InsertSlideNumber,
    /// Header & Footer (date, slide number, footer text) for the current or every slide.
    SetHeaderFooter {
        /// Show the date.
        date: bool,
        /// A fixed date text (`None`: update automatically).
        fixed_date: Option<String>,
        /// Show the slide number.
        slide_number: bool,
        /// Footer text (`None`: no footer).
        footer: Option<String>,
        /// Not on title slides.
        not_on_title: bool,
        /// Every slide, not just the current one.
        all: bool,
    },
    /// A hyperlink on the selected text or shapes (`None` removes it).
    Link {
        /// URL, `mailto:` address or `#slide:N`.
        url: Option<String>,
    },
    // ── The stage (pointer gestures, in slide px) ───────────────────────────────────────
    /// A button press on the stage. `clicks` is 2 for a double-click (edit text, select a
    /// word), 3 for a triple-click.
    PointerDown {
        /// x.
        x: f32,
        /// y.
        y: f32,
        /// Shift.
        shift: bool,
        /// Ctrl or Cmd.
        ctrl: bool,
        /// Alt.
        alt: bool,
        /// Click count.
        clicks: u8,
    },
    /// The pointer moved with the button down.
    PointerMove {
        /// x.
        x: f32,
        /// y.
        y: f32,
        /// Shift.
        shift: bool,
        /// Ctrl or Cmd.
        ctrl: bool,
        /// Alt.
        alt: bool,
    },
    /// The button was released.
    PointerUp {
        /// x.
        x: f32,
        /// y.
        y: f32,
        /// Shift.
        shift: bool,
        /// Ctrl or Cmd.
        ctrl: bool,
        /// Alt.
        alt: bool,
    },
    /// Draws a new shape by dragging: the next drag on the stage inserts `preset` (`textBox`
    /// for a text box); `None` cancels.
    ArmInsert {
        /// The shape.
        preset: Option<String>,
    },
    /// Select shapes by id (the Selection Pane).
    SelectShapes {
        /// The shapes.
        ids: Vec<NodeId>,
    },
    /// Select All (Ctrl+A): every object on the slide, or all text while editing.
    SelectAll,
    /// Esc: leave the text, then clear the selection.
    Escape,
    /// Tab / Shift+Tab without text editing: the next or previous object.
    CycleSelection {
        /// Shift+Tab.
        backward: bool,
    },
    // ── Arrange ──────────────────────────────────────────────────────────────────────────
    /// Arrow keys: move the selection (Ctrl: by one pixel).
    Nudge {
        /// Direction.
        direction: DeckDirection,
        /// One pixel instead of one grid step.
        fine: bool,
    },
    /// Shift+Arrows: resize the selection.
    Resize {
        /// Direction.
        direction: DeckDirection,
        /// One pixel instead of one grid step.
        fine: bool,
    },
    /// Size and position in points (Format ▸ Size), rotation in degrees.
    SetTransform {
        /// Left.
        x: Option<f32>,
        /// Top.
        y: Option<f32>,
        /// Width.
        width: Option<f32>,
        /// Height.
        height: Option<f32>,
        /// Rotation.
        rotation: Option<f32>,
        /// Keep the proportions when one side changes.
        lock_aspect: bool,
    },
    /// Rotate by degrees (Rotate Right 90° = 90).
    Rotate {
        /// Degrees, clockwise.
        degrees: f32,
    },
    /// Flip.
    Flip {
        /// Flip Horizontal, else Vertical.
        horizontal: bool,
    },
    /// Align (to the slide for one object, to each other for several).
    Align {
        /// How.
        how: DeckAlign,
    },
    /// Distribute three or more objects evenly.
    Distribute {
        /// Horizontally, else vertically.
        horizontal: bool,
    },
    /// Group (Ctrl+Shift+G).
    Group,
    /// Ungroup (Ctrl+Shift+H).
    Ungroup,
    /// Order objects.
    Order {
        /// How.
        order: DeckOrder,
    },
    /// Delete the selection (objects, or the slides when the rail has the focus).
    Delete,
    // ── Format ───────────────────────────────────────────────────────────────────────────
    /// Shape Fill.
    SetFill {
        /// The fill.
        fill: DeckFill,
    },
    /// Shape Outline.
    SetOutline {
        /// The outline change.
        outline: DeckOutline,
    },
    /// Shape Effects.
    SetEffect {
        /// The effect.
        effect: DeckEffect,
    },
    /// Quick Styles: theme style `index` (0–41: 6 accents × 7 intensities).
    QuickStyle {
        /// Gallery index.
        index: u32,
    },
    /// Change Shape.
    ChangeShape {
        /// ST_ShapeType name.
        preset: String,
    },
    /// Alt Text.
    SetAltText {
        /// Description.
        text: String,
    },
    /// Crop a picture: the part cut off each side, percent.
    Crop {
        /// Left.
        left: f32,
        /// Top.
        top: f32,
        /// Right.
        right: f32,
        /// Bottom.
        bottom: f32,
    },
    /// Reset Picture (crop and size).
    ResetPicture,
    /// Align Text (vertical).
    AlignText {
        /// Anchor.
        anchor: DeckAnchor,
    },
    /// Text Direction.
    TextDirection {
        /// Direction.
        direction: DeckTextDirection,
    },
    /// Columns.
    Columns {
        /// Count (1–16).
        count: u32,
        /// Spacing in points.
        spacing: f32,
    },
    /// Text autofit.
    Autofit {
        /// Mode.
        mode: DeckAutofit,
    },
    /// Character Spacing in points (0 = normal).
    CharacterSpacing {
        /// Points.
        points: f32,
    },
    /// Text Shadow (toggle when `None`).
    TextShadow {
        /// On or off.
        on: Option<bool>,
    },
    /// Copy formatting (Format Painter, Ctrl+Shift+C).
    CopyFormat,
    /// Paste formatting (Ctrl+Shift+V).
    PasteFormat,
    // ── Text ─────────────────────────────────────────────────────────────────────────────
    /// F2 / Edit Text: edit the selected shape's text (caret at the end).
    EditText,
    /// Type text at the caret (replacing the selection).
    InsertText {
        /// The text.
        text: String,
    },
    /// Backspace / Delete (Ctrl: a word).
    DeleteText {
        /// Delete, else Backspace.
        forward: bool,
        /// A word.
        word: bool,
    },
    /// Enter: a new paragraph.
    NewParagraph,
    /// Shift+Enter: a line break.
    LineBreak,
    /// Tab / Shift+Tab in text: demote or promote the paragraph at its start, else a tab.
    Tab {
        /// Shift+Tab.
        shift: bool,
    },
    /// Caret motion (arrows, Home, End, Ctrl+arrows) with Shift extending the selection.
    MoveCaret {
        /// The motion.
        motion: CaretMotion,
        /// Shift.
        extend: bool,
    },
    /// The IME's composition at the caret (empty ends it).
    SetComposition {
        /// The composition text.
        text: String,
    },
    /// Selects text in a shape (the Accessible text view): paragraph index and char offset of
    /// each end; the caret is `head`. Starts editing the shape's text when needed.
    SelectText {
        /// The shape.
        shape: NodeId,
        /// The anchor's paragraph (index in the shape's text).
        anchor_para: u32,
        /// The anchor's char offset in its paragraph.
        anchor_offset: u32,
        /// The caret's paragraph.
        head_para: u32,
        /// The caret's char offset.
        head_offset: u32,
    },
    /// Promote (Alt+Shift+Left): the paragraphs one level up.
    Promote,
    /// Demote (Alt+Shift+Right).
    Demote,
    // ── Design ───────────────────────────────────────────────────────────────────────────
    /// Apply a bundled theme by name.
    ApplyTheme {
        /// Theme name (`Kiln`, `Porcelain`, …).
        name: String,
    },
    /// Apply a variant (0–3) of the current theme.
    ApplyVariant {
        /// Variant index.
        index: u32,
    },
    /// Slide Size.
    SlideSize {
        /// The size.
        size: DeckSlideSize,
        /// How content follows.
        mode: DeckScaleMode,
    },
    /// Format Background for the current slide, or every slide.
    SetBackground {
        /// The fill (`None` resets to the layout's).
        fill: Option<DeckFill>,
        /// Apply to All.
        all: bool,
    },
    /// Hide Background Graphics on the current slide (toggle when `None`).
    HideBackgroundGraphics {
        /// Hidden or shown.
        hide: Option<bool>,
    },
    // ── View ─────────────────────────────────────────────────────────────────────────────
    /// Guides (toggle).
    ToggleGuides,
    /// Gridlines (toggle).
    ToggleGridlines,
    /// The notes pane (toggle).
    ToggleNotes,
    /// Find the next match after the selection, across slides and notes.
    Find {
        /// Text.
        text: String,
        /// Match case.
        match_case: bool,
        /// Whole words only.
        whole_word: bool,
        /// Search backwards.
        backward: bool,
    },
    /// Replace the current match (and find the next), or every match.
    Replace {
        /// Text.
        text: String,
        /// Replacement.
        with: String,
        /// Match case.
        match_case: bool,
        /// Whole words only.
        whole_word: bool,
        /// Replace All.
        all: bool,
    },
    /// Replace one font with another everywhere (Replace Fonts).
    ReplaceFont {
        /// The font to replace.
        from: String,
        /// Its replacement.
        to: String,
    },

    // ── Transitions ──────────────────────────────────────────────────────────────────
    /// Transition to This Slide: the gallery entry (see [`show::DeckTransitionInfo::effect`]) on
    /// the selected slides, with its default option and duration.
    SetTransition {
        /// The gallery entry.
        effect: String,
    },
    /// Effect Options of the selected slides' transition.
    TransitionOption {
        /// A [`show::DeckChoice::id`] of [`show::DeckTransitionInfo::options`].
        option: String,
    },
    /// Duration of the selected slides' transition.
    TransitionDuration {
        /// Milliseconds.
        ms: u32,
    },
    /// Advance Slide ▸ On Mouse Click (toggled with `None`).
    AdvanceOnClick {
        /// On or off.
        on: Option<bool>,
    },
    /// Advance Slide ▸ After (`None` turns it off).
    AdvanceAfter {
        /// Milliseconds.
        ms: Option<u32>,
    },
    /// Apply To All: every slide gets the current slide's transition and timing.
    ApplyTransitionToAll,

    // ── Animations ───────────────────────────────────────────────────────────────────
    /// The Animation gallery: the effect replaces the selected shapes' effects (or is added to
    /// shapes that have none).
    SetAnimation {
        /// The gallery entry (`appear`, `fade`, `flyIn`, …, `none` removes).
        effect: String,
    },
    /// Add Animation: the effect is added after the selected shapes' other effects.
    AddAnimation {
        /// The gallery entry.
        effect: String,
    },
    /// Effect Options of the selected effects.
    AnimationOption {
        /// A [`show::DeckChoice::id`] of [`show::DeckMotionState::options`].
        option: String,
    },
    /// Timing ▸ Start of the selected effects.
    AnimationStart {
        /// When they start.
        start: DeckEffectStart,
    },
    /// Timing ▸ Duration of the selected effects.
    AnimationDuration {
        /// Milliseconds.
        ms: u32,
    },
    /// Timing ▸ Delay of the selected effects.
    AnimationDelay {
        /// Milliseconds.
        ms: u32,
    },
    /// Repeat of the selected effects.
    AnimationRepeat {
        /// Iterations (≥ 1).
        count: u32,
    },
    /// Removes the selected effects.
    RemoveAnimations,
    /// Moves an effect to a position of the sequence (a drag in the Animation Pane).
    MoveAnimation {
        /// The effect.
        id: NodeId,
        /// Its new index.
        to: u32,
    },
    /// Move Earlier / Move Later for the selected effects.
    ReorderAnimation {
        /// Earlier instead of later.
        earlier: bool,
    },
    /// Selects effects (the Animation Pane), and their shapes on the stage.
    SelectAnimations {
        /// The effects.
        ids: Vec<NodeId>,
    },
    /// Animation Painter: picks up the selected shape's effects.
    CopyAnimation,
    /// Animation Painter: gives the selected shapes the effects picked up.
    PasteAnimation,
    /// Shows or hides the effect numbers on the stage (the Animations tab, the pane).
    MotionBadges {
        /// Shown.
        on: bool,
    },

    // ── Slide show ───────────────────────────────────────────────────────────────────
    /// Set Up Slide Show.
    SetShowSettings {
        /// The settings.
        settings: DeckShowSettings,
    },
    /// Starts a slide show (From Beginning, From Current Slide, Rehearse Timings, Preview).
    ShowStart {
        /// The slide to start at (index); `None` starts at the first slide of the show.
        from: Option<u32>,
        /// What for.
        mode: DeckShowMode,
        /// The system asks for reduced motion (`prefers-reduced-motion`): transitions play as
        /// fades and effects appear at once, in the same clicks.
        #[serde(default)]
        reduced_motion: bool,
    },
    /// Next: the next click's effects, or the next slide (N, Space, Right, Page Down, Enter).
    ShowNext,
    /// Previous: back one click or one slide (P, Backspace, Left, Page Up).
    ShowPrevious,
    /// The first slide (Home).
    ShowFirst,
    /// The last slide (End).
    ShowLast,
    /// A digit of a slide number (number, then Enter).
    ShowDigit {
        /// 0–9.
        digit: u8,
    },
    /// Goes to the slide number typed (Enter), or Next when none was.
    ShowGoToTyped,
    /// Goes to a slide (See All Slides, Presenter View's grid).
    ShowGoTo {
        /// The slide (index in the presentation).
        index: u32,
    },
    /// Black screen on or off (B, .).
    ShowBlack,
    /// White screen on or off (W, ,).
    ShowWhite,
    /// Laser pointer on or off (Ctrl+L).
    ShowLaser,
    /// Pen on or off (Ctrl+P).
    ShowPen,
    /// Highlighter on or off (Ctrl+I).
    ShowHighlighter,
    /// Eraser on or off (Ctrl+E).
    ShowEraser,
    /// The arrow (Ctrl+A).
    ShowArrow,
    /// Erases every stroke on the slide (E).
    ShowErase,
    /// The pointer on the show, in slide px: clicks advance with the arrow (on a touch screen
    /// the left third goes back), the pen draws, the laser points.
    ShowPointer {
        /// What it did.
        phase: DeckPointerPhase,
        /// x.
        x: f32,
        /// y.
        y: f32,
        /// A touch.
        touch: bool,
    },
    /// Ends the show (Esc).
    ShowEnd,
    /// Rehearse Timings: keeps the times recorded as the slides' timings.
    ShowKeepTimings,

    // ── Design Ideas, Outline View, Reuse Slides ─────────────────────────────────────
    /// Applies a suggestion of the Design Ideas pane to the current slide (one undo step).
    ApplyDesignIdea {
        /// A [`features::DeckDesignIdea::id`] of [`DeckQuery::DesignIdeas`].
        id: String,
    },
    /// An edit in Outline View.
    EditOutline {
        /// The edit.
        edit: DeckOutlineEdit,
    },
    /// Reuse Slides: inserts slides of another presentation after the current slide, on this
    /// presentation's layouts and theme (Use Destination Theme).
    ReuseSlides {
        /// The other presentation (`.pptx`, `.potx`, `.pptm`).
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
        /// Its slides to insert (indices), in order; empty for every slide.
        slides: Vec<u32>,
    },
    /// Insert ▸ Video or Audio from a file: the clip embedded with its poster frame, on the
    /// current slide (a video `width` × `height` fitted to the slide, centred; audio as its icon).
    InsertMedia {
        /// Video or audio.
        kind: DeckMediaKind,
        /// The file's name (`kiln.mp4`).
        name: String,
        /// Its media type (`video/mp4`).
        mime: String,
        /// The clip.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
        /// The poster frame, PNG (the first frame of a video, the audio icon).
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        poster: Vec<u8>,
        /// The poster's (the video's) width in pixels.
        width: u32,
        /// Its height in pixels.
        height: u32,
    },
    /// Insert ▸ Equation from its linear form (`a^2+b^2=c^2`, as Word's and PowerPoint's
    /// equation editor takes it), saved as Office Math: at the caret, or in a new text box.
    InsertEquation {
        /// The linear form.
        linear: String,
    },
    // ── charts ──────────────────────────────────────────────────────────────────────
    /// Edit Data: the selected chart's data range on its data sheet, row by row (the first row
    /// the series names, the first column the categories; text that reads as a number is one).
    /// A range of another size becomes the chart's data range.
    SetChartData {
        /// The cells.
        cells: Vec<Vec<String>>,
    },
    // ── comments ────────────────────────────────────────────────────────────────────
    /// New Comment (Ctrl+Alt+M) on the selected object, or on the slide.
    NewComment {
        /// The text.
        text: String,
        /// The author (File ▸ Options ▸ User name).
        author: String,
        /// The author's initials.
        initials: String,
    },
    /// Reply to a thread of the current slide.
    ReplyComment {
        /// The thread.
        id: NodeId,
        /// The reply.
        text: String,
        /// The author.
        author: String,
    },
    /// Edit a comment or a reply.
    EditComment {
        /// The comment or reply.
        id: NodeId,
        /// The new text.
        text: String,
    },
    /// Resolve or reopen a thread.
    ResolveComment {
        /// The thread.
        id: NodeId,
        /// Resolved.
        resolved: bool,
    },
    /// Delete a comment, a reply, the slide's comments or every comment.
    DeleteComment {
        /// The comment or reply ([`review::DeckCommentScope::One`]; the selected thread when
        /// `None`).
        id: Option<NodeId>,
        /// What goes.
        scope: review::DeckCommentScope,
    },
    /// Select a thread of the current slide (`None`: none).
    SelectComment {
        /// The thread.
        id: Option<NodeId>,
    },
    /// Previous or Next comment, across the slides.
    NextComment {
        /// Previous.
        backward: bool,
    },
    // ── spelling ────────────────────────────────────────────────────────────────────
    /// Spelling (F7): stops at the next misspelled word from the selection, on the slides and in
    /// their notes, and selects it ([`DeckState::spelling`]; `None` with
    /// [`DeckState::spell_done`] once every slide is checked).
    SpellCheck,
    /// Change (or Change All) the misspelled word, then go on.
    SpellChange {
        /// The replacement.
        with: String,
        /// Change All.
        all: bool,
    },
    /// Ignore Once, Ignore All or Add to Dictionary, then go on.
    SpellIgnore {
        /// Ignore every occurrence.
        all: bool,
        /// Add to the dictionary.
        add: bool,
    },
    /// The en-US dictionary's files for the web core.
    LoadDictionary {
        /// `.aff` text.
        aff: String,
        /// `.dic` text.
        dic: String,
    },
    /// The user's dictionary (from the user's preferences, sent when the editor opens).
    SetUserDictionary {
        /// The words.
        words: Vec<String>,
    },
    // ── design ──────────────────────────────────────────────────────────────────────
    /// Design ▸ Variants ▸ Colors: the colour scheme of a bundled theme (by name), `Office` or
    /// `Grayscale`, for the current master.
    SetThemeColors {
        /// The scheme.
        scheme: String,
    },
    /// Design ▸ Variants ▸ Fonts: the heading and body fonts of a bundled theme or `Office`.
    SetThemeFonts {
        /// The scheme.
        scheme: String,
    },
    /// Design ▸ Variants ▸ Effects: the fill, line and effect styles of a bundled theme or
    /// `Office`.
    SetThemeEffects {
        /// The scheme.
        scheme: String,
    },
    /// Design ▸ Variants ▸ Background Styles: style 1–12 (the theme's three background fills in
    /// the four background and text colours) on the current master.
    SetBackgroundStyle {
        /// The style.
        style: u8,
    },
    /// Design ▸ Themes ▸ Browse for Themes: the theme of an Office theme (`.thmx`), or the master,
    /// layouts and theme of a presentation or template.
    ApplyThemeFile {
        /// The file.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
    // ── Slide Master view ───────────────────────────────────────────────────────────
    /// View ▸ Slide Master (on, at the current slide's layout) and Close Master View (off).
    MasterView {
        /// On.
        on: bool,
    },
    /// Shows a master or layout on the stage (a click in the rail).
    SelectMasterItem {
        /// The master or layout.
        id: NodeId,
    },
    /// Insert Slide Master: a new master with the standard layouts, after the last.
    InsertSlideMaster,
    /// Insert Layout: a custom layout with a title and the footers, after the one shown.
    InsertLayout,
    /// Delete the master or layout shown (refused while slides use it).
    DeleteMasterItem,
    /// Rename the master or layout shown.
    RenameMasterItem {
        /// The name.
        name: String,
    },
    /// Preserve the master shown (kept when no slide uses it).
    PreserveMaster {
        /// On (`None` toggles).
        on: Option<bool>,
    },
    /// Insert Placeholder on the layout shown.
    InsertPlaceholder {
        /// What it holds.
        kind: content::DeckPlaceholderKind,
    },
    /// Slide Master ▸ Title: the layout shown has a title placeholder or not.
    LayoutTitle {
        /// On (`None` toggles).
        on: Option<bool>,
    },
    /// Slide Master ▸ Footers: the layout shown has the date, footer and slide number or not.
    LayoutFooters {
        /// On (`None` toggles).
        on: Option<bool>,
    },
    // ── guides and the ruler ────────────────────────────────────────────────────────
    /// Add a drawing guide (Ctrl+drag of a guide, or the slide's menu).
    AddGuide {
        /// Horizontal, else vertical.
        horizontal: bool,
        /// Where, slide px (the slide's middle when `None`).
        pos: Option<f32>,
    },
    /// A guide dragged to another place.
    MoveGuide {
        /// The guide (index of [`DeckState::guide_list`]).
        index: u32,
        /// Where, slide px.
        pos: f32,
    },
    /// A guide dragged off the slide.
    RemoveGuide {
        /// The guide.
        index: u32,
    },
    /// The ruler's indent markers: the left and first-line indent of the paragraphs being edited
    /// (or of all the text of the selected shapes).
    ParagraphIndent {
        /// Left indent, points.
        left: f32,
        /// First line, points from the left indent (negative: a hanging indent).
        first_line: f32,
    },
    // ── paste as a picture, icons, code, symbols ────────────────────────────────────
    /// Paste ▸ Picture: what was copied (objects, a slide or text) pasted as a PNG picture.
    PastePicture {
        /// The clipboard (Kiln's last copy in this presentation when `None`).
        data: Option<super::ClipboardData>,
    },
    /// Insert ▸ Icons: an icon drawn as an editable shape (its outline is the icon's strokes).
    InsertIcon {
        /// Its name (the shape's name).
        name: String,
        /// SVG path data in a 24 × 24 box, one per stroke.
        paths: Vec<String>,
    },
    /// Insert ▸ Code Block: the code in a monospaced box, coloured by its syntax.
    InsertCodeBlock {
        /// The code.
        code: String,
        /// The language (`rust`, `python`, …), for the shape's name.
        language: Option<String>,
    },
    /// Insert ▸ Symbol: characters at the caret (or in a new text box), in a font.
    InsertSymbol {
        /// The characters.
        text: String,
        /// The symbol font (Wingdings, Symbol, …), or the text's own.
        font: Option<String>,
    },
    /// Home ▸ Convert to List Diagram: the selected text's list becomes grouped shapes, one per
    /// first-level item (its sub-items in the same shape).
    ConvertToDiagram {
        /// The layout.
        layout: content::DeckListDiagram,
    },
    // ── Picture Format ▸ Crop ──────────────────────────────────────────────────────
    /// Crop: the selected picture shows its crop handles on the stage (Esc or Enter ends it).
    CropMode {
        /// On.
        on: bool,
    },
    /// Crop ▸ Aspect Ratio, Fill or Fit.
    CropTo {
        /// How.
        fit: content::DeckCropFit,
    },
    // ── Table Layout ───────────────────────────────────────────────────────────────
    /// Selects a rectangle of cells of the table being edited.
    SelectCells {
        /// The cells (no rows and no columns: the whole table).
        range: content::DeckCellRange,
    },
    /// Cell Margins of the selected cells, points (Normal 7.2/3.6, None 0, Narrow 3.6, Wide 10.8).
    CellMargins {
        /// Left.
        left: f32,
        /// Top.
        top: f32,
        /// Right.
        right: f32,
        /// Bottom.
        bottom: f32,
    },
}

const DECK: AppMask = AppMask::DECK;

crate::command_table! {
    DeckCommand => DECK_COMMANDS;
    Self::NewSlide { .. } => {
        id: "deck.slides.new_slide", keytip: Some("I"), group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Duplicate => {
        id: "deck.slides.duplicate", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::DeleteSlides => {
        id: "deck.slides.delete", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MoveSlides { .. } => {
        id: "deck.slides.move", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectSlide { .. } => {
        id: "deck.slides.select", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::PreviousSlide => {
        id: "deck.slides.previous_slide", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::NextSlide => {
        id: "deck.slides.next_slide", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SetLayout { .. } => {
        id: "deck.slides.layout", keytip: Some("L"), group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ResetSlides => {
        id: "deck.slides.reset_slide", keytip: Some("A"), group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::HideSlides { .. } => {
        id: "deck.show.hide_slide", keytip: Some("H"), group: "show",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AddSection { .. } => {
        id: "deck.slides.section", keytip: Some("T1"), group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::RenameSection { .. } => {
        id: "deck.slides.rename_section", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::RemoveSection { .. } => {
        id: "deck.slides.remove_section", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MoveSection { .. } => {
        id: "deck.slides.move_section", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetNotes { .. } => {
        id: "deck.notes.set_text", keytip: None, group: "notes",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertShape { .. } => {
        id: "deck.insert.shape", keytip: None, group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertTextBox { .. } => {
        id: "deck.insert.text_box", keytip: Some("X"), group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertPicture { .. } => {
        id: "deck.insert.picture", keytip: None, group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertTable { .. } => {
        id: "deck.insert.table", keytip: Some("T"), group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertSlideNumber => {
        id: "deck.insert.slide_number", keytip: Some("SN"), group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetHeaderFooter { .. } => {
        id: "deck.insert.header_footer", keytip: Some("H"), group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Link { .. } => {
        id: "deck.insert.link", keytip: Some("I2"), group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::PointerDown { .. } => {
        id: "deck.stage.pointer_down", keytip: None, group: "stage",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::PointerMove { .. } => {
        id: "deck.stage.pointer_move", keytip: None, group: "stage",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::PointerUp { .. } => {
        id: "deck.stage.pointer_up", keytip: None, group: "stage",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ArmInsert { .. } => {
        id: "deck.stage.arm_insert", keytip: None, group: "stage",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectShapes { .. } => {
        id: "deck.select.shapes", keytip: None, group: "select",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SelectAll => {
        id: "deck.select.all", keytip: None, group: "select",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::Escape => {
        id: "deck.select.escape", keytip: None, group: "select",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::CycleSelection { .. } => {
        id: "deck.select.next", keytip: None, group: "select",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::Nudge { .. } => {
        id: "deck.drawing.nudge", keytip: None, group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Resize { .. } => {
        id: "deck.drawing.resize", keytip: None, group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetTransform { .. } => {
        id: "deck.format.size_position", keytip: None, group: "format",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Rotate { .. } => {
        id: "deck.arrange.rotate", keytip: Some("AY"), group: "arrange",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Flip { .. } => {
        id: "deck.arrange.flip", keytip: None, group: "arrange",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Align { .. } => {
        id: "deck.arrange.align", keytip: Some("AA"), group: "arrange",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Distribute { .. } => {
        id: "deck.arrange.distribute", keytip: None, group: "arrange",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Group => {
        id: "deck.drawing.group", keytip: Some("G"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Ungroup => {
        id: "deck.drawing.ungroup", keytip: Some("U"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Order { .. } => {
        id: "deck.arrange.order", keytip: None, group: "arrange",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Delete => {
        id: "deck.edit.delete", keytip: None, group: "edit",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetFill { .. } => {
        id: "deck.drawing.shape_fill", keytip: Some("SF"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetOutline { .. } => {
        id: "deck.drawing.shape_outline", keytip: Some("SO"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetEffect { .. } => {
        id: "deck.drawing.shape_effects", keytip: Some("SE"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::QuickStyle { .. } => {
        id: "deck.drawing.quick_styles", keytip: Some("Q"), group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ChangeShape { .. } => {
        id: "deck.drawing.change_shape", keytip: None, group: "drawing",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetAltText { .. } => {
        id: "deck.format.alt_text", keytip: None, group: "format",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Crop { .. } => {
        id: "deck.picture.crop", keytip: None, group: "picture",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ResetPicture => {
        id: "deck.picture.reset", keytip: None, group: "picture",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AlignText { .. } => {
        id: "deck.paragraph.align_text", keytip: Some("AT"), group: "paragraph",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::TextDirection { .. } => {
        id: "deck.paragraph.text_direction", keytip: Some("AX"), group: "paragraph",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Columns { .. } => {
        id: "deck.paragraph.columns", keytip: None, group: "paragraph",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Autofit { .. } => {
        id: "deck.format.autofit", keytip: None, group: "format",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::CharacterSpacing { .. } => {
        id: "deck.font.character_spacing", keytip: None, group: "font",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::TextShadow { .. } => {
        id: "deck.font.text_shadow", keytip: None, group: "font",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::CopyFormat => {
        id: "deck.clipboard.copy_format", keytip: None, group: "clipboard",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::PasteFormat => {
        id: "deck.clipboard.paste_format", keytip: None, group: "clipboard",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::EditText => {
        id: "deck.slides.edit_text", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertText { .. } => {
        id: "deck.text.insert", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::DeleteText { .. } => {
        id: "deck.text.delete", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::NewParagraph => {
        id: "deck.text.new_paragraph", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::LineBreak => {
        id: "deck.text.line_break", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Tab { .. } => {
        id: "deck.text.tab", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MoveCaret { .. } => {
        id: "deck.text.move", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SetComposition { .. } => {
        id: "deck.text.composition", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectText { .. } => {
        id: "deck.text.select", keytip: None, group: "text",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Promote => {
        id: "deck.slides.promote", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::Demote => {
        id: "deck.slides.demote", keytip: None, group: "slides",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ApplyTheme { .. } => {
        id: "deck.design.themes", keytip: Some("H"), group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ApplyVariant { .. } => {
        id: "deck.design.variants", keytip: Some("V"), group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SlideSize { .. } => {
        id: "deck.design.slide_size", keytip: Some("S"), group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetBackground { .. } => {
        id: "deck.design.format_background", keytip: Some("B"), group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::HideBackgroundGraphics { .. } => {
        id: "deck.design.hide_background_graphics", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ToggleGuides => {
        id: "deck.view.guides", keytip: Some("U"), group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ToggleGridlines => {
        id: "deck.view.gridlines", keytip: Some("G"), group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ToggleNotes => {
        id: "deck.view.notes", keytip: Some("NO"), group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::Find { .. } => {
        id: "deck.edit.find", keytip: None, group: "edit",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::Replace { .. } => {
        id: "deck.edit.replace", keytip: None, group: "edit",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ReplaceFont { .. } => {
        id: "deck.edit.replace_font", keytip: None, group: "edit",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetTransition { .. } => {
        id: "deck.transitions.gallery", keytip: Some("T"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::TransitionOption { .. } => {
        id: "deck.transitions.effect_options", keytip: Some("EO"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::TransitionDuration { .. } => {
        id: "deck.transitions.duration", keytip: Some("E"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AdvanceOnClick { .. } => {
        id: "deck.transitions.on_click", keytip: Some("M"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AdvanceAfter { .. } => {
        id: "deck.transitions.after", keytip: Some("AF"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ApplyTransitionToAll => {
        id: "deck.transitions.apply_to_all", keytip: Some("L"), group: "transitions",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetAnimation { .. } => {
        id: "deck.animations.gallery", keytip: Some("S"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AddAnimation { .. } => {
        id: "deck.animations.add_animation", keytip: Some("AA"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AnimationOption { .. } => {
        id: "deck.animations.effect_options", keytip: Some("EM"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AnimationStart { .. } => {
        id: "deck.animations.start", keytip: Some("ST"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AnimationDuration { .. } => {
        id: "deck.animations.duration", keytip: Some("D"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AnimationDelay { .. } => {
        id: "deck.animations.delay", keytip: Some("DY"), group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AnimationRepeat { .. } => {
        id: "deck.animations.repeat", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::RemoveAnimations => {
        id: "deck.animations.remove", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MoveAnimation { .. } => {
        id: "deck.animations.move", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ReorderAnimation { .. } => {
        id: "deck.animations.reorder", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectAnimations { .. } => {
        id: "deck.animations.select", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::CopyAnimation => {
        id: "deck.animations.copy", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::PasteAnimation => {
        id: "deck.animations.paste", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MotionBadges { .. } => {
        id: "deck.animations.badges", keytip: None, group: "animations",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SetShowSettings { .. } => {
        id: "deck.show.settings", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ShowStart { .. } => {
        id: "deck.show.start", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowNext => {
        id: "deck.show.next", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowPrevious => {
        id: "deck.show.previous", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowFirst => {
        id: "deck.show.first", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowLast => {
        id: "deck.show.last", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowDigit { .. } => {
        id: "deck.show.digit", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowGoToTyped => {
        id: "deck.show.go_to_typed", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowGoTo { .. } => {
        id: "deck.show.go_to", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowBlack => {
        id: "deck.show.black", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowWhite => {
        id: "deck.show.white", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowLaser => {
        id: "deck.show.laser", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowPen => {
        id: "deck.show.pen", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowHighlighter => {
        id: "deck.show.highlighter", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowEraser => {
        id: "deck.show.eraser", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowArrow => {
        id: "deck.show.arrow", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowErase => {
        id: "deck.show.erase", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowPointer { .. } => {
        id: "deck.show.pointer", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ShowEnd => {
        id: "deck.show.end", keytip: None, group: "show",
        tier: P0, apps: DECK, availability: Document,
    };
    // Asked after Rehearse Timings, which is P1.
    Self::ShowKeepTimings => {
        id: "deck.show.keep_timings", keytip: None, group: "show",
        tier: P1, apps: DECK, availability: Editable,
    };
    Self::ApplyDesignIdea { .. } => {
        id: "deck.designer.apply", keytip: None, group: "designer",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::EditOutline { .. } => {
        id: "deck.outline.edit", keytip: None, group: "outline",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ReuseSlides { .. } => {
        id: "deck.insert.reuse_slides_from", keytip: None, group: "insert",
        tier: P1, apps: DECK, availability: Editable,
    };
    Self::InsertMedia { .. } => {
        id: "deck.insert.media", keytip: None, group: "insert",
        tier: P1, apps: DECK, availability: Editable,
    };
    Self::InsertEquation { .. } => {
        id: "deck.insert.equation", keytip: None, group: "insert",
        tier: P1, apps: DECK, availability: Editable,
    };
    Self::SetChartData { .. } => {
        id: "deck.chart.set_data", keytip: None, group: "chart",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::NewComment { .. } => {
        id: "deck.comment.new", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ReplyComment { .. } => {
        id: "deck.comment.reply", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::EditComment { .. } => {
        id: "deck.comment.edit", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ResolveComment { .. } => {
        id: "deck.comment.resolve", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::DeleteComment { .. } => {
        id: "deck.comment.delete", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectComment { .. } => {
        id: "deck.comment.select", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::NextComment { .. } => {
        id: "deck.comment.next", keytip: None, group: "comment",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SpellCheck => {
        id: "deck.spell.check", keytip: None, group: "spell",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SpellChange { .. } => {
        id: "deck.spell.change", keytip: None, group: "spell",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SpellIgnore { .. } => {
        id: "deck.spell.ignore", keytip: None, group: "spell",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::LoadDictionary { .. } => {
        id: "deck.spell.load_dictionary", keytip: None, group: "spell",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SetUserDictionary { .. } => {
        id: "deck.spell.user_dictionary", keytip: None, group: "spell",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SetThemeColors { .. } => {
        id: "deck.design.colors", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetThemeFonts { .. } => {
        id: "deck.design.fonts", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetThemeEffects { .. } => {
        id: "deck.design.effects", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SetBackgroundStyle { .. } => {
        id: "deck.design.background_style", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ApplyThemeFile { .. } => {
        id: "deck.design.browse_themes", keytip: None, group: "design",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::MasterView { .. } => {
        id: "deck.master.view", keytip: None, group: "master",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::SelectMasterItem { .. } => {
        id: "deck.master.select", keytip: None, group: "master",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::InsertSlideMaster => {
        id: "deck.master.insert_master", keytip: Some("IM"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertLayout => {
        id: "deck.master.insert_layout", keytip: Some("IL"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::DeleteMasterItem => {
        id: "deck.master.delete", keytip: Some("D"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::RenameMasterItem { .. } => {
        id: "deck.master.rename", keytip: Some("R"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::PreserveMaster { .. } => {
        id: "deck.master.preserve", keytip: Some("P"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertPlaceholder { .. } => {
        id: "deck.master.insert_placeholder", keytip: Some("IP"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::LayoutTitle { .. } => {
        id: "deck.master.title", keytip: Some("TI"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::LayoutFooters { .. } => {
        id: "deck.master.footers", keytip: Some("F"), group: "master",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::AddGuide { .. } => {
        id: "deck.view.add_guide", keytip: None, group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::MoveGuide { .. } => {
        id: "deck.view.move_guide", keytip: None, group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::RemoveGuide { .. } => {
        id: "deck.view.remove_guide", keytip: None, group: "view",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::ParagraphIndent { .. } => {
        id: "deck.paragraph.indent", keytip: None, group: "paragraph",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::PastePicture { .. } => {
        id: "deck.clipboard.paste_picture", keytip: Some("U"), group: "clipboard",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertIcon { .. } => {
        id: "deck.insert.icon", keytip: None, group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertCodeBlock { .. } => {
        id: "deck.insert.code_block", keytip: None, group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::InsertSymbol { .. } => {
        id: "deck.insert.symbol", keytip: None, group: "insert",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::ConvertToDiagram { .. } => {
        id: "deck.paragraph.convert_to_diagram", keytip: None, group: "paragraph",
        tier: P1, apps: DECK, availability: Editable,
    };
    Self::CropMode { .. } => {
        id: "deck.picture.crop_mode", keytip: Some("C"), group: "picture",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::CropTo { .. } => {
        id: "deck.picture.crop_to", keytip: None, group: "picture",
        tier: P0, apps: DECK, availability: Editable,
    };
    Self::SelectCells { .. } => {
        id: "deck.table.select_cells", keytip: None, group: "table",
        tier: P0, apps: DECK, availability: Document,
    };
    Self::CellMargins { .. } => {
        id: "deck.table.cell_margins", keytip: Some("M"), group: "table",
        tier: P0, apps: DECK, availability: Editable,
    };
}

/// What the chrome asks the Deck core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckQuery {
    /// Everything the chrome shows (slides, sections, selection, formatting, design).
    State,
    /// Thumbnails of slides `from .. from + count`, `width` CSS px wide at `dpr`, as one
    /// self-contained scene (tile index = slide index).
    Thumbnails {
        /// First slide.
        from: u32,
        /// How many.
        count: u32,
        /// Thumbnail width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// Thumbnails of the current master's layouts (the layout gallery), tile index = layout
    /// index in [`DeckState::layouts`].
    LayoutThumbnails {
        /// Width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// The current slide drawn in each bundled theme (the theme gallery), tile index = index
    /// in [`DeckState::themes`].
    ThemeThumbnails {
        /// Width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// The current theme's variants drawn on a title slide (tile index = variant), for the
    /// Variants gallery; no tiles for a theme without Kiln variants.
    VariantThumbnails {
        /// Width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// View ▸ Notes Page: the notes page's geometry.
    NotesPage,
    /// Slide Master view's rail: every master and layout (tile index = item of
    /// [`content::DeckMasterView::items`]).
    MasterThumbnails {
        /// Width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// What a point of the stage (slide px) is over, for the pointer's cursor.
    HitTest {
        /// x.
        x: f32,
        /// y.
        y: f32,
    },
    /// The text of every slide as an outline.
    Outline,
    /// The Shapes gallery: every preset's outline as SVG path data in a `size` × `size` box.
    ShapeIcons {
        /// Icon size in px.
        size: f32,
    },
    /// The running show's frame at `now_ms` of the show's clock, for a screen of `width` ×
    /// `height` CSS px at `dpr`.
    ShowFrame {
        /// The show's clock, milliseconds (any origin; it only has to increase).
        now_ms: f64,
        /// Screen width, CSS px.
        width: f32,
        /// Screen height, CSS px.
        height: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// What Presenter View shows beside the slides.
    ShowInfo,
    /// The presentation as `.pptx` bytes (a second window or tab opens it to show it).
    Snapshot,
    /// The Design Ideas pane: suggestions for the current slide, each drawn `width` CSS px wide
    /// at `dpr`.
    DesignIdeas {
        /// Thumbnail width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// File ▸ Export ▸ Change File Type: slides as pictures `width` px wide — one picture, or
    /// a `.zip` of a folder with one per slide.
    ExportPictures {
        /// The slides (indices); `None` for every slide.
        slides: Option<Vec<u32>>,
        /// PNG or JPEG.
        format: DeckPictureFormat,
        /// Width in pixels (PowerPoint's default is the slide's size at 96 dpi).
        width: u32,
    },
    /// Outline View: every slide's title and body lines.
    OutlineView,
    /// Reuse Slides: the slides of another presentation, with thumbnails `width` CSS px wide.
    ReuseList {
        /// The presentation.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
        /// Thumbnail width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// The clips on the slide the show shows (the stage's slide when no show runs).
    SlideMedia,
    /// A clip's bytes (to play it).
    MediaBytes {
        /// Its part ([`features::DeckMediaItem::part`]).
        part: String,
    },
    /// Edit Data: the selected chart's data range.
    ChartData,
    /// The text of every shape and the notes of every slide (a saved file is checked against
    /// it).
    SlideText,
}

impl Eq for DeckQuery {}

/// One slide in the rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSlideInfo {
    /// The slide.
    pub id: NodeId,
    /// Its title (the title placeholder's text).
    pub title: String,
    /// Hidden in the show.
    pub hidden: bool,
    /// The layout's name.
    pub layout: String,
    /// Has speaker notes.
    pub notes: bool,
    /// Has a transition or animations (the rail's star).
    pub motion: bool,
    /// Changes whenever what the slide shows changes (thumbnail cache key).
    #[ts(type = "number")]
    pub version: u64,
}

/// A section of the rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSectionInfo {
    /// Name.
    pub name: String,
    /// First slide index.
    pub start: u32,
    /// Number of slides.
    pub count: u32,
}

/// A shape of the current slide (the Selection Pane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckShapeInfo {
    /// The shape.
    pub id: NodeId,
    /// Its name.
    pub name: String,
    /// `shape`, `text box`, `picture`, `table`, `chart`, `group`, `connector`, `media`,
    /// `placeholder`.
    pub kind: String,
    /// Hidden.
    pub hidden: bool,
}

/// A layout of the current master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckLayoutInfo {
    /// The layout.
    pub id: NodeId,
    /// Its name.
    pub name: String,
}

/// Formatting of the selection, for the ribbon's pressed states and values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckFormat {
    /// Font family.
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Strikethrough.
    pub strike: bool,
    /// Superscript.
    pub superscript: bool,
    /// Subscript.
    pub subscript: bool,
    /// Text shadow.
    pub shadow: bool,
    /// Text colour `RRGGBB`.
    pub color: Option<String>,
    /// `left`, `center`, `right`, `justify`.
    pub align: String,
    /// Bullets.
    pub bullets: bool,
    /// Numbering.
    pub numbering: bool,
    /// Vertical anchor.
    pub anchor: DeckAnchor,
    /// Text direction.
    pub direction: DeckTextDirection,
    /// Autofit.
    pub autofit: DeckAutofit,
    /// Columns.
    pub columns: u32,
    /// Character spacing in points.
    pub spacing: f32,
    /// Shape fill `RRGGBB` when solid.
    pub fill: Option<String>,
    /// The kind of the shape's fill: `none`, `solid`, `gradient`, `picture` or `pattern`
    /// (`none` without a shape).
    pub fill_kind: String,
    /// Outline `RRGGBB` when solid.
    pub outline: Option<String>,
    /// Outline weight in points.
    pub outline_width: Option<f32>,
    /// The paragraph's left indent, points.
    pub indent_left: f32,
    /// Its first-line indent, points from the left indent.
    pub indent_first: f32,
}

/// The size and position of the selection, in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckBox {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Rotation in degrees.
    pub rotation: f32,
}

/// Header & Footer of the current slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckHeaderFooter {
    /// Date shown.
    pub date: bool,
    /// Slide number shown.
    pub slide_number: bool,
    /// Footer text, when shown.
    pub footer: Option<String>,
}

/// Everything the Deck chrome shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckState {
    /// The slides.
    pub slides: Vec<DeckSlideInfo>,
    /// The sections (empty without sections).
    pub sections: Vec<DeckSectionInfo>,
    /// The slide on the stage.
    pub current: u32,
    /// The selected slides (the rail).
    pub selected: Vec<u32>,
    /// Slide width in px.
    pub slide_width: f32,
    /// Slide height in px.
    pub slide_height: f32,
    /// `widescreen`, `standard` or `custom`.
    pub size_kind: String,
    /// Shapes of the current slide, top first.
    pub shapes: Vec<DeckShapeInfo>,
    /// Selected shapes.
    pub selection: Vec<NodeId>,
    /// Text is being edited.
    pub editing: bool,
    /// Contextual tabs to show: `shape`, `picture`, `table`, `chart`.
    pub contexts: Vec<String>,
    /// Formatting of the selection or the text being edited.
    pub format: Option<DeckFormat>,
    /// Size and position of the selection.
    pub selection_box: Option<DeckBox>,
    /// The selected shape's alt text.
    pub alt_text: Option<String>,
    /// The current theme's name.
    pub theme: String,
    /// The bundled themes (the gallery).
    pub themes: Vec<String>,
    /// The current master's layouts.
    pub layouts: Vec<DeckLayoutInfo>,
    /// The current slide's layout.
    pub layout: Option<NodeId>,
    /// The current slide's speaker notes.
    pub notes: String,
    /// Guides shown.
    pub guides: bool,
    /// Gridlines shown.
    pub gridlines: bool,
    /// Notes pane shown.
    pub notes_visible: bool,
    /// The current slide's header and footer.
    pub header_footer: DeckHeaderFooter,
    /// The current slide's background colour, when solid.
    pub background: Option<String>,
    /// Background graphics hidden on the current slide.
    pub hide_graphics: bool,
    /// A shape is armed for drawing (Insert ▸ Shapes).
    pub armed: Option<String>,
    /// Find found nothing more.
    pub find_done: bool,
    /// Where the caret is, in slide px, while editing (the input proxy goes there).
    pub caret: Option<DeckRect>,
    /// The Transitions, Animations and Slide Show tabs.
    pub motion: show::DeckMotionState,
    /// Chart Design and Format: the selected chart.
    pub chart: Option<super::suite::ChartState>,
    /// The current slide's comment threads.
    pub comments: Vec<review::DeckComment>,
    /// Comment threads in the presentation.
    pub comment_total: u32,
    /// The selected thread.
    pub comment: Option<NodeId>,
    /// The misspelled word Spelling stopped at.
    pub spelling: Option<review::DeckSpelling>,
    /// Spelling went through every slide.
    pub spell_done: bool,
    /// Slide Master view, while it is on.
    pub master_view: Option<content::DeckMasterView>,
    /// The drawing guides (shown with [`DeckState::guides`]).
    pub guide_list: Vec<content::DeckGuide>,
    /// The text area of the shape being edited, slide px (the ruler's indent markers).
    pub text_area: Option<DeckRect>,
    /// The selected picture shows its crop handles (Picture Format ▸ Crop).
    pub cropping: bool,
    /// The selected cells of the table being edited (more than one).
    pub cells: Option<content::DeckCellRange>,
}

/// What the pointer is over on the stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DeckCursor {
    /// Nothing (the slide or the pasteboard).
    Default,
    /// A shape (move).
    Move,
    /// Text (I-beam).
    Text,
    /// A sizing handle: the CSS cursor (`nwse-resize`, …) by angle, 0 = `ns`, 1 = `nesw`,
    /// 2 = `ew`, 3 = `nwse`.
    Resize(u8),
    /// The rotation handle.
    Rotate,
    /// Drawing a new shape.
    Crosshair,
}

/// One slide of the outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckOutlineSlide {
    /// Title.
    pub title: String,
    /// Body paragraphs with their levels (0-based).
    pub lines: Vec<(u8, String)>,
}

/// A preset of the Shapes gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckShapeIcon {
    /// ST_ShapeType name.
    pub preset: String,
    /// PowerPoint's name of the shape ("Rectangle: Rounded Corners").
    pub label: String,
    /// Gallery category: `lines`, `basic`, `arrows`, `math`, `flowchart`, `stars`, `callouts`,
    /// `actions`.
    pub category: String,
    /// SVG path data.
    pub path: String,
}

/// Answers to [`DeckQuery`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckQueryResult {
    /// [`DeckQuery::State`].
    State(Box<DeckState>),
    /// Thumbnail scenes.
    Scene(Box<crate::scene::SceneDelta>),
    /// [`DeckQuery::HitTest`].
    Cursor(DeckCursor),
    /// [`DeckQuery::Outline`].
    Outline(Vec<DeckOutlineSlide>),
    /// [`DeckQuery::ShapeIcons`].
    Icons(Vec<DeckShapeIcon>),
    /// [`DeckQuery::ShowFrame`].
    ShowFrame(Box<show::DeckShowFrame>),
    /// [`DeckQuery::ShowInfo`].
    ShowInfo(Box<show::DeckShowInfo>),
    /// [`DeckQuery::Snapshot`].
    Bytes(
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        Vec<u8>,
    ),
    /// [`DeckQuery::DesignIdeas`].
    DesignIdeas(Vec<features::DeckDesignIdea>),
    /// [`DeckQuery::ExportPictures`].
    File(features::DeckFile),
    /// [`DeckQuery::OutlineView`].
    OutlineView(Vec<features::DeckOutlineEntry>),
    /// [`DeckQuery::ReuseList`].
    ReuseList(Vec<features::DeckReuseSlide>),
    /// [`DeckQuery::SlideMedia`].
    SlideMedia(features::DeckSlideMedia),
    /// [`DeckQuery::ChartData`] (`None` without a chart selected).
    ChartData(Option<content::DeckChartData>),
    /// [`DeckQuery::NotesPage`].
    NotesPage(content::DeckNotesPage),
    /// [`DeckQuery::SlideText`].
    SlideText(Vec<content::DeckSlideText>),
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the Deck protocol types (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::samples::Samples;

    fn rect() -> DeckRect {
        DeckRect {
            x: 120.5,
            y: -8.0,
            width: 240.0,
            height: 135.0,
        }
    }

    /// One of every Deck command.
    #[must_use]
    pub fn commands() -> Vec<Command> {
        use DeckCommand as C;
        [
            C::NewSlide { layout: None },
            C::NewSlide {
                layout: Some(NodeId(42)),
            },
            C::Duplicate,
            C::DeleteSlides,
            C::MoveSlides { to: 3 },
            C::SelectSlide {
                index: 2,
                extend: true,
                toggle: false,
            },
            C::PreviousSlide,
            C::NextSlide,
            C::SetLayout { layout: NodeId(7) },
            C::ResetSlides,
            C::HideSlides { hidden: None },
            C::AddSection {
                name: Some("Rückblick «2026»".into()),
            },
            C::RenameSection {
                index: 1,
                name: "Plans".into(),
            },
            C::RemoveSection {
                index: 0,
                with_slides: false,
            },
            C::MoveSection {
                index: 1,
                delta: -1,
            },
            C::SetNotes {
                text: "Start with the number.\nThen the jade slice.".into(),
            },
            C::InsertShape {
                preset: "rightArrow".into(),
                rect: Some(rect()),
            },
            C::InsertTextBox { rect: None },
            C::InsertPicture {
                name: "kiln.png".into(),
                bytes: vec![137, 80, 78, 71],
                rect: None,
            },
            C::InsertTable { rows: 3, cols: 4 },
            C::InsertSlideNumber,
            C::SetHeaderFooter {
                date: true,
                fixed_date: None,
                slide_number: true,
                footer: Some("Kiln Co-op".into()),
                not_on_title: true,
                all: true,
            },
            C::Link {
                url: Some("https://kiln.example/".into()),
            },
            C::PointerDown {
                x: 480.0,
                y: 270.5,
                shift: false,
                ctrl: true,
                alt: false,
                clicks: 2,
            },
            C::PointerMove {
                x: 500.0,
                y: 280.0,
                shift: true,
                ctrl: false,
                alt: true,
            },
            C::PointerUp {
                x: 500.0,
                y: 280.0,
                shift: false,
                ctrl: false,
                alt: false,
            },
            C::ArmInsert {
                preset: Some("ellipse".into()),
            },
            C::SelectShapes {
                ids: vec![NodeId(9), NodeId(10)],
            },
            C::SelectAll,
            C::Escape,
            C::CycleSelection { backward: true },
            C::Nudge {
                direction: DeckDirection::Left,
                fine: true,
            },
            C::Resize {
                direction: DeckDirection::Down,
                fine: false,
            },
            C::SetTransform {
                x: Some(72.0),
                y: None,
                width: Some(144.5),
                height: None,
                rotation: Some(15.0),
                lock_aspect: true,
            },
            C::Rotate { degrees: 90.0 },
            C::Flip { horizontal: false },
            C::Align {
                how: DeckAlign::Middle,
            },
            C::Distribute { horizontal: true },
            C::Group,
            C::Ungroup,
            C::Order {
                order: DeckOrder::Back,
            },
            C::Delete,
            C::SetFill {
                fill: DeckFill::Solid {
                    color: "accent2".into(),
                    brightness: Some(40.0),
                    transparency: None,
                },
            },
            C::SetFill {
                fill: DeckFill::Gradient {
                    from: "E2622A".into(),
                    to: "F7B500".into(),
                    angle: 45.0,
                },
            },
            C::SetFill {
                fill: DeckFill::Pattern {
                    preset: "ltUpDiag".into(),
                    fg: "1B1411".into(),
                    bg: "FFF7EE".into(),
                },
            },
            C::SetFill {
                fill: DeckFill::Picture {
                    name: "clay.jpg".into(),
                    bytes: vec![255, 216],
                },
            },
            C::SetFill {
                fill: DeckFill::None,
            },
            C::SetOutline {
                outline: DeckOutline::Weight { points: 2.25 },
            },
            C::SetOutline {
                outline: DeckOutline::Arrows {
                    head: "none".into(),
                    tail: "triangle".into(),
                },
            },
            C::SetEffect {
                effect: DeckEffect::Shadow {
                    distance: 3.0,
                    blur: 4.0,
                    angle: 45.0,
                },
            },
            C::QuickStyle { index: 41 },
            C::ChangeShape {
                preset: "roundRect".into(),
            },
            C::SetAltText {
                text: "Kiln at night".into(),
            },
            C::Crop {
                left: 10.0,
                top: 0.0,
                right: 5.5,
                bottom: 0.0,
            },
            C::ResetPicture,
            C::AlignText {
                anchor: DeckAnchor::Bottom,
            },
            C::TextDirection {
                direction: DeckTextDirection::Rotate270,
            },
            C::Columns {
                count: 2,
                spacing: 12.0,
            },
            C::Autofit {
                mode: DeckAutofit::Shrink,
            },
            C::CharacterSpacing { points: -0.5 },
            C::TextShadow { on: Some(true) },
            C::CopyFormat,
            C::PasteFormat,
            C::EditText,
            C::InsertText {
                text: "Ünïcödé ✓".into(),
            },
            C::DeleteText {
                forward: true,
                word: true,
            },
            C::NewParagraph,
            C::LineBreak,
            C::Tab { shift: true },
            C::MoveCaret {
                motion: CaretMotion::WordForward,
                extend: true,
            },
            C::SetComposition {
                text: "にほん".into(),
            },
            C::SelectText {
                shape: NodeId(7),
                anchor_para: 0,
                anchor_offset: 2,
                head_para: 1,
                head_offset: 0,
            },
            C::Promote,
            C::Demote,
            C::ApplyTheme {
                name: "Tenmoku".into(),
            },
            C::ApplyVariant { index: 3 },
            C::SlideSize {
                size: DeckSlideSize::Custom {
                    width_in: 11.0,
                    height_in: 8.5,
                },
                mode: DeckScaleMode::EnsureFit,
            },
            C::SetBackground {
                fill: Some(DeckFill::Solid {
                    color: "FFF7EE".into(),
                    brightness: None,
                    transparency: Some(20.0),
                }),
                all: true,
            },
            C::HideBackgroundGraphics { hide: Some(true) },
            C::ToggleGuides,
            C::ToggleGridlines,
            C::ToggleNotes,
            C::Find {
                text: "kiln".into(),
                match_case: false,
                whole_word: true,
                backward: false,
            },
            C::Replace {
                text: "kiln".into(),
                with: "Kiln".into(),
                match_case: true,
                whole_word: false,
                all: true,
            },
            C::ReplaceFont {
                from: "Calibri".into(),
                to: "Figtree".into(),
            },
            C::SetTransition {
                effect: "push".into(),
            },
            C::TransitionOption {
                option: "fromBottom".into(),
            },
            C::TransitionDuration { ms: 1250 },
            C::AdvanceOnClick { on: None },
            C::AdvanceAfter { ms: Some(5000) },
            C::ApplyTransitionToAll,
            C::SetAnimation {
                effect: "flyIn".into(),
            },
            C::AddAnimation {
                effect: "pulse".into(),
            },
            C::AnimationOption {
                option: "byParagraph".into(),
            },
            C::AnimationStart {
                start: DeckEffectStart::WithPrevious,
            },
            C::AnimationDuration { ms: 750 },
            C::AnimationDelay { ms: 250 },
            C::AnimationRepeat { count: 3 },
            C::RemoveAnimations,
            C::MoveAnimation {
                id: NodeId(31),
                to: 0,
            },
            C::ReorderAnimation { earlier: true },
            C::SelectAnimations {
                ids: vec![NodeId(31)],
            },
            C::CopyAnimation,
            C::PasteAnimation,
            C::MotionBadges { on: true },
            C::SetShowSettings {
                settings: show::samples::settings(),
            },
            C::ShowStart {
                from: Some(2),
                mode: DeckShowMode::Rehearse,
                reduced_motion: true,
            },
            C::ShowNext,
            C::ShowPrevious,
            C::ShowFirst,
            C::ShowLast,
            C::ShowDigit { digit: 7 },
            C::ShowGoToTyped,
            C::ShowGoTo { index: 4 },
            C::ShowBlack,
            C::ShowWhite,
            C::ShowLaser,
            C::ShowPen,
            C::ShowHighlighter,
            C::ShowEraser,
            C::ShowArrow,
            C::ShowErase,
            C::ShowPointer {
                phase: DeckPointerPhase::Move,
                x: 640.5,
                y: 360.0,
                touch: true,
            },
            C::ShowEnd,
            C::ShowKeepTimings,
            C::ApplyDesignIdea {
                id: "panelLeft".into(),
            },
            C::EditOutline {
                edit: features::samples::edits()[1].clone(),
            },
            C::ReuseSlides {
                bytes: vec![80, 75, 3, 4],
                slides: vec![0, 2],
            },
            C::InsertMedia {
                kind: DeckMediaKind::Video,
                name: "kiln.mp4".into(),
                mime: "video/mp4".into(),
                bytes: vec![0, 0, 0, 24],
                poster: vec![137, 80, 78, 71],
                width: 1280,
                height: 720,
            },
            C::InsertEquation {
                linear: "x=(-b\\pm\\sqrt(b^2-4ac))/(2a)".into(),
            },
            C::SetChartData {
                cells: content::samples::chart_data().cells,
            },
            C::NewComment {
                text: "Can we show last year here?".into(),
                author: "Zoë Okafor".into(),
                initials: "ZO".into(),
            },
            C::ReplyComment {
                id: NodeId(40),
                text: "Added it.".into(),
                author: "Sam Price".into(),
            },
            C::EditComment {
                id: NodeId(41),
                text: "Added it on slide 3.".into(),
            },
            C::ResolveComment {
                id: NodeId(40),
                resolved: true,
            },
            C::DeleteComment {
                id: None,
                scope: review::DeckCommentScope::Slide,
            },
            C::SelectComment {
                id: Some(NodeId(40)),
            },
            C::NextComment { backward: true },
            C::SpellCheck,
            C::SpellChange {
                with: "glaze".into(),
                all: false,
            },
            C::SpellIgnore {
                all: true,
                add: false,
            },
            C::LoadDictionary {
                aff: "SET UTF-8".into(),
                dic: "1\nkiln".into(),
            },
            C::SetUserDictionary {
                words: vec!["Tenmoku".into()],
            },
            C::SetThemeColors {
                scheme: "Cobalt".into(),
            },
            C::SetThemeFonts {
                scheme: "Office".into(),
            },
            C::SetThemeEffects {
                scheme: "Raku".into(),
            },
            C::SetBackgroundStyle { style: 10 },
            C::ApplyThemeFile {
                bytes: vec![80, 75, 3, 4],
            },
            C::MasterView { on: true },
            C::SelectMasterItem { id: NodeId(3) },
            C::InsertSlideMaster,
            C::InsertLayout,
            C::DeleteMasterItem,
            C::RenameMasterItem {
                name: "Quote".into(),
            },
            C::PreserveMaster { on: Some(true) },
            C::InsertPlaceholder {
                kind: content::DeckPlaceholderKind::Picture,
            },
            C::LayoutTitle { on: None },
            C::LayoutFooters { on: Some(true) },
            C::AddGuide {
                horizontal: false,
                pos: Some(212.5),
            },
            C::MoveGuide {
                index: 1,
                pos: 640.0,
            },
            C::RemoveGuide { index: 0 },
            C::ParagraphIndent {
                left: 36.0,
                first_line: -18.0,
            },
            C::PastePicture { data: None },
            C::InsertIcon {
                name: "Flame".into(),
                paths: vec!["M12 2c1 4 6 6 6 12a6 6 0 0 1-12 0c0-4 3-6 3-9".into()],
            },
            C::InsertCodeBlock {
                code: "let kiln = 1240;".into(),
                language: Some("rust".into()),
            },
            C::InsertSymbol {
                text: "\u{2192}".into(),
                font: Some("Wingdings".into()),
            },
            C::ConvertToDiagram {
                layout: content::DeckListDiagram::Process,
            },
            C::CropMode { on: true },
            C::CropTo {
                fit: content::DeckCropFit::Aspect {
                    width: 4,
                    height: 3,
                },
            },
            C::SelectCells {
                range: content::DeckCellRange {
                    row: 0,
                    col: 0,
                    rows: 2,
                    cols: 1,
                },
            },
            C::CellMargins {
                left: 3.6,
                top: 3.6,
                right: 3.6,
                bottom: 3.6,
            },
        ]
        .into_iter()
        .map(Command::Deck)
        .collect()
    }

    impl Samples for DeckCommand {
        fn samples() -> Vec<Self> {
            commands()
                .into_iter()
                .filter_map(|c| match c {
                    Command::Deck(d) => Some(d),
                    _ => None,
                })
                .collect()
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

    fn format() -> DeckFormat {
        DeckFormat {
            font_family: "Figtree".into(),
            font_size: 18.0,
            bold: true,
            italic: false,
            underline: false,
            strike: false,
            superscript: false,
            subscript: false,
            shadow: false,
            color: Some("1B1411".into()),
            align: "left".into(),
            bullets: true,
            numbering: false,
            anchor: DeckAnchor::Top,
            direction: DeckTextDirection::Horizontal,
            autofit: DeckAutofit::Shrink,
            columns: 1,
            spacing: 0.0,
            fill: Some("E2622A".into()),
            fill_kind: "solid".into(),
            outline: None,
            outline_width: Some(0.75),
            indent_left: 27.0,
            indent_first: -27.0,
        }
    }

    pub(crate) fn state() -> DeckState {
        DeckState {
            slides: vec![DeckSlideInfo {
                id: NodeId(3),
                title: "Every ticket fires the next kiln.".into(),
                hidden: false,
                layout: "Title and Content".into(),
                notes: true,
                motion: true,
                version: 9_007_199_254_740_991,
            }],
            sections: vec![DeckSectionInfo {
                name: "Open studio".into(),
                start: 0,
                count: 1,
            }],
            current: 0,
            selected: vec![0],
            slide_width: 1280.0,
            slide_height: 720.0,
            size_kind: "widescreen".into(),
            shapes: vec![DeckShapeInfo {
                id: NodeId(9),
                name: "Title 1".into(),
                kind: "placeholder".into(),
                hidden: false,
            }],
            selection: vec![NodeId(9)],
            editing: true,
            contexts: vec!["shape".into()],
            format: Some(format()),
            selection_box: Some(DeckBox {
                x: 72.0,
                y: 40.5,
                width: 600.0,
                height: 96.0,
                rotation: 0.0,
            }),
            alt_text: None,
            theme: "Kiln".into(),
            themes: vec!["Kiln".into(), "Porcelain".into()],
            layouts: vec![DeckLayoutInfo {
                id: NodeId(5),
                name: "Title Slide".into(),
            }],
            layout: Some(NodeId(5)),
            notes: "Start with the number.".into(),
            guides: false,
            gridlines: true,
            notes_visible: true,
            header_footer: DeckHeaderFooter {
                date: false,
                slide_number: true,
                footer: Some("Kiln Co-op".into()),
            },
            background: Some("FFF7EE".into()),
            hide_graphics: false,
            armed: None,
            find_done: false,
            caret: Some(DeckRect {
                x: 300.0,
                y: 60.0,
                width: 1.0,
                height: 40.0,
            }),
            motion: show::samples::motion(),
            chart: None,
            comments: vec![review::samples::comment()],
            comment_total: 3,
            comment: Some(NodeId(40)),
            spelling: Some(review::samples::spelling()),
            spell_done: false,
            master_view: Some(content::samples::master_view()),
            guide_list: vec![content::DeckGuide {
                horizontal: true,
                pos: 360.0,
            }],
            text_area: Some(rect()),
            cropping: false,
            cells: Some(content::DeckCellRange {
                row: 0,
                col: 1,
                rows: 2,
                cols: 2,
            }),
        }
    }

    samples_of! {
        DeckRect => [rect()];
        DeckDirection => [DeckDirection::Up, DeckDirection::Right];
        DeckAlign => [DeckAlign::Left, DeckAlign::Bottom];
        DeckOrder => [DeckOrder::Front, DeckOrder::Back];
        DeckFill => [DeckFill::None, DeckFill::Solid { color: "accent1".into(), brightness: Some(-25.0), transparency: None }];
        DeckOutline => [DeckOutline::None, DeckOutline::Dash { dash: "sysDot".into() }];
        DeckEffect => [DeckEffect::None, DeckEffect::Glow { radius: 8.0 }, DeckEffect::SoftEdges { radius: 2.5 }, DeckEffect::NoShadow];
        DeckAnchor => [DeckAnchor::Top, DeckAnchor::Middle];
        DeckTextDirection => [DeckTextDirection::Horizontal, DeckTextDirection::Stacked];
        DeckAutofit => [DeckAutofit::Off, DeckAutofit::ResizeShape];
        DeckSlideSize => [DeckSlideSize::Widescreen, DeckSlideSize::Standard];
        DeckScaleMode => [DeckScaleMode::Maximize];
        DeckQuery => [
            DeckQuery::State,
            DeckQuery::Thumbnails { from: 0, count: 12, width: 160.0, dpr: 2.0 },
            DeckQuery::LayoutThumbnails { width: 120.0, dpr: 1.0 },
            DeckQuery::ThemeThumbnails { width: 96.0, dpr: 1.5 },
            DeckQuery::VariantThumbnails { width: 64.0, dpr: 2.0 },
            DeckQuery::MasterThumbnails { width: 160.0, dpr: 1.0 },
            DeckQuery::NotesPage,
            DeckQuery::HitTest { x: 12.5, y: -3.0 },
            DeckQuery::Outline,
            DeckQuery::ShapeIcons { size: 20.0 },
            DeckQuery::ShowFrame { now_ms: 1234.5, width: 1440.0, height: 900.0, dpr: 2.0 },
            DeckQuery::ShowInfo,
            DeckQuery::Snapshot,
            DeckQuery::DesignIdeas { width: 180.0, dpr: 2.0 },
            DeckQuery::ExportPictures { slides: Some(vec![1]), format: DeckPictureFormat::Png, width: 1280 },
            DeckQuery::ExportPictures { slides: None, format: DeckPictureFormat::Jpeg, width: 1920 },
            DeckQuery::OutlineView,
            DeckQuery::ReuseList { bytes: vec![80, 75, 3, 4], width: 160.0, dpr: 1.0 },
            DeckQuery::SlideMedia,
            DeckQuery::MediaBytes { part: "/ppt/media/media1.mp4".into() },
            DeckQuery::ChartData,
            DeckQuery::SlideText,
        ];
        DeckShapeIcon => [DeckShapeIcon { preset: "rightArrow".into(), label: "Arrow: Right".into(), category: "arrows".into(), path: "M0 5 L15 5 L15 0 L20 10 L15 20 L15 15 L0 15 Z".into() }];
        DeckSlideInfo => [state().slides[0].clone()];
        DeckSectionInfo => [state().sections[0].clone()];
        DeckShapeInfo => [state().shapes[0].clone()];
        DeckLayoutInfo => [state().layouts[0].clone()];
        DeckFormat => [format()];
        DeckBox => [DeckBox { x: 0.0, y: 0.0, width: 1.0, height: 1.0, rotation: 359.5 }];
        DeckHeaderFooter => [state().header_footer];
        DeckState => [state()];
        DeckCursor => [DeckCursor::Default, DeckCursor::Resize(3), DeckCursor::Rotate, DeckCursor::Text];
        DeckOutlineSlide => [DeckOutlineSlide { title: "Plans".into(), lines: vec![(0, "Clay".into()), (1, "Glaze".into())] }];
        DeckQueryResult => [
            DeckQueryResult::State(Box::new(state())),
            DeckQueryResult::Scene(Box::new(crate::scene::samples::small_delta())),
            DeckQueryResult::Cursor(DeckCursor::Move),
            DeckQueryResult::Outline(Vec::new()),
            DeckQueryResult::Icons(Vec::new()),
            DeckQueryResult::ShowFrame(Box::new(show::samples::frame())),
            DeckQueryResult::ShowInfo(Box::new(show::samples::info())),
            DeckQueryResult::Bytes(vec![80, 75, 3, 4]),
            DeckQueryResult::DesignIdeas(Vec::new()),
            DeckQueryResult::File(features::DeckFile { name: "Plans.zip".into(), mime: "application/zip".into(), bytes: vec![80, 75] }),
            DeckQueryResult::OutlineView(vec![features::samples::entry()]),
            DeckQueryResult::ReuseList(Vec::new()),
            DeckQueryResult::SlideMedia(features::DeckSlideMedia { slide: 0, width: 960.0, height: 540.0, items: vec![features::samples::media()] }),
            DeckQueryResult::ChartData(Some(content::samples::chart_data())),
            DeckQueryResult::NotesPage(content::DeckNotesPage {
                width: 720.0,
                height: 960.0,
                slide: rect(),
                body: rect(),
            }),
            DeckQueryResult::SlideText(vec![content::DeckSlideText {
                slide: NodeId(7),
                shapes: 2,
                texts: vec![content::samples::shape_text()],
                notes: Vec::new(),
                hidden: false,
            }]),
        ];
    }

    /// Registers the Deck types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            DeckCommand, DeckRect, DeckDirection, DeckAlign, DeckOrder, DeckFill, DeckOutline,
            DeckEffect, DeckAnchor, DeckTextDirection, DeckAutofit, DeckSlideSize, DeckScaleMode,
            DeckQuery, DeckSlideInfo, DeckSectionInfo, DeckShapeInfo, DeckLayoutInfo, DeckFormat,
            DeckBox, DeckHeaderFooter, DeckState, DeckCursor, DeckOutlineSlide, DeckQueryResult,
            DeckShapeIcon,
        );
        show::samples::visit(v);
        features::samples::visit(v);
        content::samples::visit(v);
        review::samples::visit(v);
    }
}
