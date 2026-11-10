// SPDX-License-Identifier: AGPL-3.0-or-later
//! The slide show: a [`Show`] is the presenter's place in a running show — the slide,
//! the clicks done on it, black or white screens, the pointer tool, ink, a slide number being
//! typed — and the show's clock. The chrome sends what the presenter does as commands and asks
//! for frames with its clock's time; [`frame`] builds each frame from the slides' tiles and the
//! motion of `kiln-deck-layout::motion`.
//!
//! Timing follows PowerPoint: entering a slide plays its transition, then the effects that
//! start by themselves; each click starts the next group of effects (a click while effects
//! still run starts the next group at once); going back shows the previous step as it ended,
//! without motion; a slide with "After" advances by itself once its effects have finished and
//! the time has passed; after the last slide comes the black "End of slide show" screen.
//! Hidden slides are skipped, but a slide number typed goes to them too.

pub(crate) mod frame;
pub(crate) mod ink;
pub(crate) mod settings;
#[cfg(test)]
mod tests;

use kiln_common::{KilnError, Result};
use kiln_deck::Presentation;
use kiln_deck::transition::TransitionEffect;
use kiln_protocol::commands::DeckCommand;
use kiln_protocol::commands::deck_show::{
    DeckPointerPhase, DeckShowInfo, DeckShowMode, DeckShowScreen, DeckShowSettings, DeckShowStatus,
    DeckShowTool, DeckShowType,
};
use rustc_hash::FxHashMap;

use super::DeckDoc;
use crate::model::{Change, ModelNotice};
use ink::InkStroke;

/// The transition into the slide being shown, while it plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Entering {
    /// The slide left (index) and the clicks done on it, or `None` from black (a show's first
    /// slide, a transition preview of the first slide).
    pub from: Option<(usize, usize)>,
    /// The transition.
    pub effect: TransitionEffect,
    /// Its duration, milliseconds.
    pub duration_ms: f64,
}

/// A running show.
#[derive(Debug)]
pub(crate) struct Show {
    pub mode: DeckShowMode,
    pub kind: DeckShowType,
    /// The slides of the show (indices), hidden ones included (they are skipped by Next and
    /// Previous but can be gone to by number).
    pub order: Vec<usize>,
    /// Index into `order`.
    pub pos: usize,
    /// Clicks done on the slide.
    pub click: usize,
    /// The step shows as it ended (after Previous), without motion.
    pub settled: bool,
    /// When the current step started on the show's clock; `None` until the next frame.
    pub since: Option<f64>,
    /// When the slide was entered on the show's clock (slide timings count from it).
    pub slide_since: Option<f64>,
    /// The transition into the slide (until it has played).
    pub entering: Option<Entering>,
    pub screen: DeckShowScreen,
    pub tool: DeckShowTool,
    pub typed: String,
    pub ink: FxHashMap<usize, Vec<InkStroke>>,
    /// A pen or highlighter stroke (or the eraser) is under way.
    pub drawing: bool,
    pub laser: Option<(f32, f32)>,
    /// Pointer pressed with the arrow (a click advances on release).
    pub pressed: bool,
    pub loop_show: bool,
    pub no_animation: bool,
    /// Reduced motion (`prefers-reduced-motion`): fades for transitions, effects at once.
    pub reduced_motion: bool,
    /// Slide timings are ignored.
    pub manual: bool,
    pub version: u64,
    /// The show clock's last time seen.
    pub now: f64,
    /// Rehearse Timings: each position's time so far, milliseconds.
    pub rehearsed: Vec<f64>,
    /// The show has ended (the chrome leaves it).
    pub ended: bool,
    /// The tiles of the slides being shown.
    pub scene: frame::ShowScene,
}

impl Show {
    fn new(
        pres: &Presentation,
        settings: &DeckShowSettings,
        mode: DeckShowMode,
        from: Option<usize>,
    ) -> Result<Self> {
        let n = pres.slides.len();
        if n == 0 {
            return Err(KilnError::InvalidInput(
                "the presentation has no slides to show".into(),
            ));
        }
        let preview = matches!(
            mode,
            DeckShowMode::PreviewTransition | DeckShowMode::PreviewAnimations
        );
        let order: Vec<usize> = if preview {
            vec![from.unwrap_or(0).min(n - 1)]
        } else {
            let first = settings
                .from
                .map_or(0, |f| usize::try_from(f.max(1) - 1).unwrap_or(0))
                .min(n - 1);
            let last = settings
                .to
                .map_or(n - 1, |t| usize::try_from(t.max(1) - 1).unwrap_or(n - 1))
                .clamp(first, n - 1);
            (first..=last).collect()
        };
        let mut show = Self {
            mode,
            kind: if mode == DeckShowMode::Show {
                settings.kind
            } else {
                DeckShowType::Speaker
            },
            order,
            pos: 0,
            click: 0,
            settled: false,
            since: None,
            slide_since: None,
            entering: None,
            screen: DeckShowScreen::Slide,
            tool: DeckShowTool::Arrow,
            typed: String::new(),
            ink: FxHashMap::default(),
            drawing: false,
            laser: None,
            pressed: false,
            loop_show: !preview && (settings.loop_show || settings.kind == DeckShowType::Kiosk),
            no_animation: !preview && settings.no_animation,
            reduced_motion: false,
            manual: preview || settings.manual || mode == DeckShowMode::Rehearse,
            version: 1,
            now: 0.0,
            rehearsed: Vec::new(),
            ended: false,
            scene: frame::ShowScene::default(),
        };
        // From a slide: the slide itself (hidden or not); otherwise the first one shown.
        let start = match from {
            Some(i) if !preview => show.order.iter().position(|s| *s == i.min(n - 1)),
            _ => None,
        };
        show.pos = start
            .or_else(|| show.shown_from(pres, 0, true))
            .unwrap_or(0);
        let slide = show.slide();
        let effect = match mode {
            DeckShowMode::PreviewAnimations => TransitionEffect::None,
            _ => pres.slides[slide]
                .transition
                .as_ref()
                .map_or(TransitionEffect::None, kiln_deck::Transition::playable),
        };
        let duration = pres.slides[slide]
            .transition
            .as_ref()
            .map_or(0, |t| t.duration_ms);
        // The show opens on its first slide's transition (from black), as PowerPoint does.
        let from_slide = (mode == DeckShowMode::PreviewTransition && slide > 0)
            .then(|| (slide - 1, pres.slides[slide - 1].timeline.clicks()));
        if effect != TransitionEffect::None && duration > 0 {
            show.entering = Some(Entering {
                from: from_slide,
                effect,
                duration_ms: f64::from(duration),
            });
        }
        Ok(show)
    }

    /// Reduced motion: the transition under way plays as a fade, and so do the later ones;
    /// effects appear at once.
    pub(crate) fn reduce_motion(&mut self) {
        self.reduced_motion = true;
        if let Some(e) = &mut self.entering {
            e.effect = calm(e.effect, true);
        }
    }

    /// The slide shown (index in the presentation).
    pub(crate) fn slide(&self) -> usize {
        self.order.get(self.pos).copied().unwrap_or(0)
    }

    /// The first position at or after (or before, with `forward` false) `from` whose slide is
    /// not hidden.
    fn shown_from(&self, pres: &Presentation, from: usize, forward: bool) -> Option<usize> {
        let ok = |p: usize| {
            self.order
                .get(p)
                .and_then(|s| pres.slides.get(*s))
                .is_some_and(|s| !s.hidden)
        };
        if forward {
            (from..self.order.len()).find(|p| ok(*p))
        } else {
            (0..=from.min(self.order.len().saturating_sub(1)))
                .rev()
                .find(|p| ok(*p))
        }
    }

    /// Clicks the slide shown needs.
    fn clicks(&self, pres: &Presentation) -> usize {
        if self.no_animation {
            return 0;
        }
        pres.slides
            .get(self.slide())
            .map_or(0, |s| s.timeline.clicks())
    }

    fn bump(&mut self) {
        self.version += 1;
    }

    /// Records the time spent on the slide being left (Rehearse Timings).
    fn note_time(&mut self) {
        if let Some(start) = self.slide_since {
            let spent = (self.now - start).max(0.0);
            if self.rehearsed.len() <= self.pos {
                self.rehearsed.resize(self.pos + 1, 0.0);
            }
            self.rehearsed[self.pos] += spent;
        }
    }

    /// Goes to a position: with its transition (`transition`), or straight to the step given.
    fn enter(
        &mut self,
        pres: &Presentation,
        pos: usize,
        transition: bool,
        click: usize,
        settled: bool,
    ) {
        self.note_time();
        let from = (self.slide(), self.click);
        self.pos = pos.min(self.order.len().saturating_sub(1));
        self.click = click;
        self.settled = settled;
        self.since = None;
        self.slide_since = None;
        self.screen = DeckShowScreen::Slide;
        self.typed.clear();
        self.laser = None;
        self.drawing = false;
        self.entering = None;
        if transition
            && let Some(t) = pres
                .slides
                .get(self.slide())
                .and_then(|s| s.transition.as_ref())
        {
            let effect = calm(t.playable(), self.reduced_motion);
            if effect != TransitionEffect::None && t.duration_ms > 0 {
                self.entering = Some(Entering {
                    from: Some(from),
                    effect,
                    duration_ms: f64::from(t.duration_ms),
                });
            }
        }
        self.bump();
    }

    /// Next: a click, the next slide, the end screen, or the end.
    pub(crate) fn next(&mut self, pres: &Presentation) {
        self.typed.clear();
        match self.screen {
            DeckShowScreen::Black | DeckShowScreen::White => {
                self.screen = DeckShowScreen::Slide;
                self.bump();
                return;
            }
            DeckShowScreen::End => {
                self.end();
                return;
            }
            DeckShowScreen::Slide => {}
        }
        if self.click < self.clicks(pres) {
            // A running transition is cut short by the click.
            self.entering = None;
            self.click += 1;
            self.settled = false;
            self.since = None;
            self.bump();
            return;
        }
        let after = self.pos + 1;
        if let Some(p) = self.shown_from(pres, after, true) {
            self.enter(pres, p, true, 0, false);
        } else if self.loop_show
            && let Some(p) = self.shown_from(pres, 0, true)
        {
            self.enter(pres, p, true, 0, false);
        } else if matches!(
            self.mode,
            DeckShowMode::PreviewTransition | DeckShowMode::PreviewAnimations
        ) {
            self.end();
        } else {
            self.note_time();
            self.slide_since = None;
            self.screen = DeckShowScreen::End;
            self.entering = None;
            self.laser = None;
            self.bump();
        }
    }

    /// Previous: back one click (as it ended) or to the previous slide's last step.
    pub(crate) fn previous(&mut self, pres: &Presentation) {
        self.typed.clear();
        match self.screen {
            DeckShowScreen::Black | DeckShowScreen::White => {
                self.screen = DeckShowScreen::Slide;
                self.bump();
                return;
            }
            DeckShowScreen::End => {
                self.screen = DeckShowScreen::Slide;
                self.click = self.clicks(pres);
                self.settled = true;
                self.since = None;
                self.slide_since = None;
                self.bump();
                return;
            }
            DeckShowScreen::Slide => {}
        }
        if self.click > 0 {
            // Back one click: the step before, as it ended.
            self.click -= 1;
            self.settled = true;
            self.since = None;
            self.entering = None;
            self.bump();
            return;
        }
        if let Some(p) = self
            .pos
            .checked_sub(1)
            .and_then(|p| self.shown_from(pres, p, false))
        {
            let clicks = if self.no_animation {
                0
            } else {
                pres.slides
                    .get(self.order[p])
                    .map_or(0, |s| s.timeline.clicks())
            };
            self.enter(pres, p, false, clicks, true);
        } else if self.loop_show
            && let Some(p) = self.shown_from(pres, self.order.len().saturating_sub(1), false)
            && p != self.pos
        {
            self.enter(pres, p, false, 0, false);
        }
    }

    /// Goes to a slide (index in the presentation), hidden or not; slides outside the show's
    /// range are not shown.
    pub(crate) fn go_to(&mut self, pres: &Presentation, index: usize) -> bool {
        match self.order.iter().position(|s| *s == index) {
            Some(p) => {
                self.enter(pres, p, false, 0, false);
                true
            }
            None => false,
        }
    }

    fn end(&mut self) {
        self.note_time();
        self.ended = true;
        self.bump();
    }

    fn toggle_tool(&mut self, tool: DeckShowTool) {
        self.tool = if self.tool == tool {
            DeckShowTool::Arrow
        } else {
            tool
        };
        self.drawing = false;
        if self.tool != DeckShowTool::Laser {
            self.laser = None;
        }
        self.bump();
    }

    /// Whether a mouse click advances the slide shown.
    fn on_click(&self, pres: &Presentation) -> bool {
        self.kind != DeckShowType::Kiosk
            && pres
                .slides
                .get(self.slide())
                .and_then(|s| s.transition.as_ref())
                .is_none_or(|t| t.advance_on_click)
    }

    fn pointer(
        &mut self,
        pres: &Presentation,
        phase: DeckPointerPhase,
        x: f32,
        y: f32,
        touch: bool,
    ) {
        let slide = self.slide();
        match (self.tool, phase) {
            (_, DeckPointerPhase::Leave) => {
                if self.laser.take().is_some() || self.drawing {
                    self.drawing = false;
                    self.bump();
                }
                self.pressed = false;
            }
            (DeckShowTool::Arrow, DeckPointerPhase::Down) => self.pressed = true,
            (DeckShowTool::Arrow, DeckPointerPhase::Up) => {
                if !std::mem::replace(&mut self.pressed, false) {
                    return;
                }
                let w = kiln_deck_layout::geom::px(pres.slide_size.cx);
                // A kiosk moves on by its timings alone, for a tap as for a click.
                let tap = touch && self.kind != DeckShowType::Kiosk;
                if tap && x < w / 3.0 {
                    self.previous(pres);
                } else if tap || self.on_click(pres) || self.screen != DeckShowScreen::Slide {
                    self.next(pres);
                }
            }
            (DeckShowTool::Arrow, DeckPointerPhase::Move) => {}
            (DeckShowTool::Laser, DeckPointerPhase::Up) => {}
            (DeckShowTool::Laser, _) => {
                self.laser = Some((x, y));
                self.bump();
            }
            (DeckShowTool::Pen | DeckShowTool::Highlighter, DeckPointerPhase::Down) => {
                let h = self.tool == DeckShowTool::Highlighter;
                self.ink
                    .entry(slide)
                    .or_default()
                    .push(InkStroke::new(h, (x, y)));
                self.drawing = true;
                self.bump();
            }
            (DeckShowTool::Pen | DeckShowTool::Highlighter, DeckPointerPhase::Move) => {
                if self.drawing
                    && let Some(s) = self.ink.get_mut(&slide).and_then(|v| v.last_mut())
                {
                    s.push((x, y));
                    self.bump();
                }
            }
            (DeckShowTool::Pen | DeckShowTool::Highlighter, DeckPointerPhase::Up) => {
                self.drawing = false;
                self.bump();
            }
            (DeckShowTool::Eraser, DeckPointerPhase::Down) => {
                self.drawing = true;
                if let Some(v) = self.ink.get_mut(&slide) {
                    ink::erase_at(v, x, y);
                }
                self.bump();
            }
            (DeckShowTool::Eraser, DeckPointerPhase::Move) => {
                if self.drawing
                    && let Some(v) = self.ink.get_mut(&slide)
                    && ink::erase_at(v, x, y)
                {
                    self.bump();
                }
            }
            (DeckShowTool::Eraser, DeckPointerPhase::Up) => self.drawing = false,
        }
    }

    /// The show's state for the chrome.
    pub(crate) fn status(&self, pres: &Presentation) -> DeckShowStatus {
        let slide = self.slide();
        let shown = |p: usize| {
            self.order
                .get(p)
                .and_then(|s| pres.slides.get(*s))
                .is_some_and(|s| !s.hidden)
        };
        let count = (0..self.order.len()).filter(|p| shown(*p)).count();
        let position = (0..=self.pos).filter(|p| shown(*p)).count().max(1);
        let next = self
            .shown_from(pres, self.pos + 1, true)
            .or_else(|| {
                self.loop_show
                    .then(|| self.shown_from(pres, 0, true))
                    .flatten()
            })
            .map(|p| self.order[p]);
        let u = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        DeckShowStatus {
            running: !self.ended,
            mode: self.mode,
            slide: u(slide),
            position: u(position),
            count: u(count),
            click: u(self.click),
            clicks: u(self.clicks(pres)),
            screen: self.screen,
            tool: self.tool,
            typed: self.typed.clone(),
            next: next.map(u),
            on_click: self.on_click(pres),
            kind: self.kind,
            version: self.version,
        }
    }
}

impl DeckDoc {
    /// The show's state (not running when there is no show).
    pub(crate) fn show_status(&self) -> DeckShowStatus {
        self.show
            .as_ref()
            .map(|s| s.status(&self.pres))
            .unwrap_or_default()
    }

    /// Runs a Slide Show command.
    pub(crate) fn show_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        match cmd {
            D::SetShowSettings { settings } => {
                let mut s = settings.clone();
                if let (Some(a), Some(b)) = (s.from, s.to)
                    && b < a
                {
                    s.to = Some(a);
                }
                if s == self.show_settings {
                    return Ok(Change::None);
                }
                // Use Presenter View is not kept in the file (PowerPoint has no place for it in
                // `p:showPr`), so it alone leaves the document unchanged.
                let saved = settings::saved(&s) != settings::saved(&self.show_settings);
                self.show_settings = s;
                if !saved {
                    return Ok(Change::Selection);
                }
                self.settings_changed();
                return Ok(Change::Content);
            }
            D::ShowStart {
                from,
                mode,
                reduced_motion,
            } => {
                self.end_text();
                let from = from.and_then(|f| usize::try_from(f).ok());
                let mut show = Show::new(&self.pres, &self.show_settings, *mode, from)?;
                if *reduced_motion {
                    show.reduce_motion();
                }
                self.show = Some(show);
                self.notify_show();
                return Ok(Change::Selection);
            }
            D::ShowKeepTimings => return self.keep_timings(),
            _ => {}
        }
        let pres = &self.pres;
        let Some(show) = self.show.as_mut() else {
            return Err(KilnError::InvalidInput("no slide show is running".into()));
        };
        let before = show.version;
        match cmd {
            D::ShowNext => show.next(pres),
            D::ShowPrevious => show.previous(pres),
            D::ShowFirst => {
                if let Some(p) = show.shown_from(pres, 0, true) {
                    show.enter(pres, p, false, 0, false);
                }
            }
            D::ShowLast => {
                if let Some(p) = show.shown_from(pres, show.order.len().saturating_sub(1), false) {
                    show.enter(pres, p, false, 0, false);
                }
            }
            D::ShowDigit { digit } => {
                if *digit <= 9 && show.typed.len() < 5 {
                    show.typed.push(char::from(b'0' + digit));
                    show.bump();
                }
            }
            D::ShowGoToTyped => {
                let typed = std::mem::take(&mut show.typed);
                match typed.parse::<usize>() {
                    Ok(n) if n >= 1 => {
                        if !show.go_to(pres, n - 1) {
                            show.bump();
                        }
                    }
                    _ if typed.is_empty() => show.next(pres),
                    _ => show.bump(),
                }
            }
            D::ShowGoTo { index } => {
                let i = usize::try_from(*index).unwrap_or(usize::MAX);
                show.go_to(pres, i);
            }
            D::ShowBlack | D::ShowWhite => {
                let screen = if matches!(cmd, D::ShowBlack) {
                    DeckShowScreen::Black
                } else {
                    DeckShowScreen::White
                };
                if show.screen != DeckShowScreen::End {
                    show.screen = if show.screen == screen {
                        DeckShowScreen::Slide
                    } else {
                        screen
                    };
                    show.bump();
                }
            }
            D::ShowLaser => show.toggle_tool(DeckShowTool::Laser),
            D::ShowPen => show.toggle_tool(DeckShowTool::Pen),
            D::ShowHighlighter => show.toggle_tool(DeckShowTool::Highlighter),
            D::ShowEraser => show.toggle_tool(DeckShowTool::Eraser),
            D::ShowArrow => {
                if show.tool != DeckShowTool::Arrow {
                    show.toggle_tool(show.tool);
                }
            }
            D::ShowErase => {
                let slide = show.slide();
                if show.ink.remove(&slide).is_some_and(|v| !v.is_empty()) {
                    show.bump();
                }
            }
            D::ShowPointer { phase, x, y, touch } => {
                if x.is_finite() && y.is_finite() {
                    show.pointer(pres, *phase, *x, *y, *touch);
                }
            }
            D::ShowEnd => show.end(),
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} in a slide show",
                    cmd.meta().id
                )));
            }
        }
        let changed = show.version != before;
        self.after_show_step();
        Ok(if changed {
            Change::Selection
        } else {
            Change::None
        })
    }

    /// After a show command or frame: a show that ended is put away (the stage goes to the
    /// slide it ended on), and every window following the show hears of it.
    pub(crate) fn after_show_step(&mut self) {
        let Some(show) = &self.show else { return };
        if show.ended {
            let slide = show.slide();
            let mode = show.mode;
            if mode == DeckShowMode::Rehearse {
                self.rehearsal = Some(
                    show.order
                        .iter()
                        .zip(show.rehearsed.iter().chain(std::iter::repeat(&0.0)))
                        .filter_map(|(s, ms)| {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let ms = ms.round().max(0.0) as u32;
                            self.pres.slides.get(*s).map(|x| (x.id, ms))
                        })
                        .filter(|(_, ms)| *ms > 0)
                        .collect(),
                );
            }
            if matches!(mode, DeckShowMode::Show | DeckShowMode::Rehearse)
                && slide < self.pres.slides.len()
            {
                self.view.current = slide;
                self.view.slides = vec![slide];
                self.view.shapes.clear();
            }
        }
        self.notify_show();
        if self.show.as_ref().is_some_and(|s| s.ended) {
            self.show = None;
        }
    }

    /// Queues the show's state for the windows that follow it.
    pub(crate) fn notify_show(&mut self) {
        let status = self.show_status();
        if self.notified.as_ref() != Some(&status) {
            self.notified = Some(status.clone());
            self.notices.push(ModelNotice::Show(status));
        }
    }

    /// Set Up Slide Show changed: saved with the file, so the document is modified (it is not
    /// an undo step).
    pub(crate) fn settings_changed(&mut self) {
        self.state = self.next_state;
        self.next_state += 1;
    }

    /// Rehearse Timings ▸ Yes: the times recorded become the slides' timings.
    fn keep_timings(&mut self) -> Result<Change> {
        let Some(times) = self.rehearsal.take() else {
            return Err(KilnError::InvalidInput("no rehearsal to keep".into()));
        };
        let mut ops = Vec::new();
        for (id, ms) in &times {
            let Some(s) = self.pres.slide(*id) else {
                continue;
            };
            let mut t = s.transition.clone().unwrap_or_default();
            t.advance_after_ms = Some(*ms);
            ops.push(kiln_deck::Op::SetTransition {
                slide: *id,
                transition: Some(t),
            });
        }
        if ops.is_empty() {
            return Ok(Change::None);
        }
        self.apply(
            "deck.show.keep_timings",
            kiln_deck::ops::Transaction::new(ops),
        )?;
        if self.show_settings.manual {
            self.show_settings.manual = false;
            self.settings_changed();
        }
        Ok(Change::Content)
    }

    /// What Presenter View shows beside the slides.
    pub(crate) fn show_info(&self) -> DeckShowInfo {
        let status = self.show_status();
        let (notes, title) = self
            .pres
            .slides
            .get(usize::try_from(status.slide).unwrap_or(0))
            .map(|s| {
                (
                    s.notes_text(),
                    kiln_deck_layout::a11y::slide_title(&self.pres, s.id),
                )
            })
            .unwrap_or_default();
        let (slides, rehearsed) = self.show.as_ref().map_or_else(
            || (Vec::new(), Vec::new()),
            |s| {
                (
                    s.order
                        .iter()
                        .filter(|i| self.pres.slides.get(**i).is_some_and(|x| !x.hidden))
                        .map(|i| u32::try_from(*i).unwrap_or(0))
                        .collect(),
                    s.rehearsed
                        .iter()
                        .map(|ms| {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let v = ms.max(0.0) as u32;
                            v
                        })
                        .collect(),
                )
            },
        );
        DeckShowInfo {
            status,
            notes,
            title,
            slides,
            rehearsed,
        }
    }
}

/// A transition without movement: a fade for anything but no transition or a fade already.
fn calm(effect: TransitionEffect, reduced: bool) -> TransitionEffect {
    match effect {
        TransitionEffect::None | TransitionEffect::Fade { .. } => effect,
        _ if reduced => TransitionEffect::Fade {
            through_black: false,
        },
        _ => effect,
    }
}
