// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slides and design: New Slide with a layout, Duplicate, Delete, reorder, select in the rail,
//! Previous/Next, Layout, Reset, Hide, sections, speaker notes, Header & Footer, themes and
//! variants, Slide Size and Format Background.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::edit::ScaleMode;
use kiln_deck::ops::{EditKind, Op, Transaction};
use kiln_deck::text::TextBody;
use kiln_deck::themes::{ThemeFile, bundled};
use kiln_deck::{
    AutoShape, Background, LayoutKind, Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps,
    SlideSize,
};
use kiln_doc::{Field, Fragment, InlineObject, Inlines, RunProps};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::{Fill, GradientFill, GradientStop, LinearShade, PatternFill};
use kiln_protocol::commands::{DeckCommand, DeckFill, DeckScaleMode, DeckSlideSize};

use super::DeckDoc;
use crate::model::Change;

/// A DrawingML colour from the chrome's `RRGGBB` or theme slot, with brightness and
/// transparency in percent.
pub(crate) fn drawing_color(
    c: &str,
    brightness: Option<f32>,
    transparency: Option<f32>,
) -> DrawingColor {
    let hex = c.trim().trim_start_matches('#');
    let mut d = if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        DrawingColor::srgb(&hex.to_ascii_uppercase())
    } else {
        DrawingColor::scheme(hex)
    };
    #[allow(clippy::cast_possible_truncation)]
    if let Some(b) = brightness.filter(|b| b.abs() > 0.01) {
        let k = (f64::from(b.clamp(-100.0, 100.0)) * 1000.0).round() as i64;
        if k > 0 {
            d = d.with("lumMod", 100_000 - k).with("lumOff", k);
        } else {
            d = d.with("lumMod", 100_000 + k);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    if let Some(t) = transparency.filter(|t| *t > 0.01) {
        let a = ((100.0 - f64::from(t.clamp(0.0, 100.0))) * 1000.0).round() as i64;
        d = d.with("alpha", a);
    }
    d
}

/// The fill of a chrome fill choice (pictures are stored as parts first: `part` is its name).
pub(crate) fn fill_of(f: &DeckFill, part: Option<&str>) -> Fill {
    match f {
        DeckFill::None => Fill::None,
        DeckFill::Solid {
            color,
            brightness,
            transparency,
        } => Fill::Solid(drawing_color(color, *brightness, *transparency)),
        DeckFill::Gradient { from, to, angle } => Fill::Gradient(GradientFill {
            stops: vec![
                GradientStop {
                    pos: 0.0,
                    color: drawing_color(from, None, None),
                },
                GradientStop {
                    pos: 1.0,
                    color: drawing_color(to, None, None),
                },
            ],
            linear: Some(LinearShade {
                angle: f64::from(*angle),
                scaled: false,
            }),
            rot_with_shape: Some(true),
            ..GradientFill::default()
        }),
        DeckFill::Pattern { preset, fg, bg } => Fill::Pattern(PatternFill {
            preset: kiln_ooxml::st::PresetPatternVal::parse(preset)
                .unwrap_or(kiln_ooxml::st::PresetPatternVal::Pct20),
            fg: Some(drawing_color(fg, None, None)),
            bg: Some(drawing_color(bg, None, None)),
        }),
        DeckFill::Picture { .. } => Fill::Blip(kiln_ooxml::drawingml::fill::BlipFill {
            embed: part.map(str::to_owned),
            mode: kiln_ooxml::drawingml::fill::BlipFillMode::Stretch,
            ..kiln_ooxml::drawingml::fill::BlipFill::default()
        }),
    }
}

impl DeckDoc {
    /// Stores a picture fill's image as a part; returns its name and the op.
    pub(crate) fn picture_part(&self, f: &DeckFill) -> Option<(String, Op)> {
        let DeckFill::Picture { name, bytes } = f else {
            return None;
        };
        let ext = name
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .filter(|e| ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&e.as_str()))
            .unwrap_or_else(|| "png".into());
        let key = (1u32..)
            .map(|n| format!("/ppt/media/image{n}.{ext}"))
            .find(|p| !self.pres.parts.contains_key(p))?;
        let ct = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            _ => "image/png",
        };
        Some((
            key.clone(),
            Op::SetPart {
                key,
                part: Some(kiln_deck::Part {
                    content_type: ct.into(),
                    bytes: bytes.clone(),
                }),
            },
        ))
    }

    /// The layout a new slide after the current one gets: the current slide's, except after a
    /// title slide (Title and Content), as PowerPoint does.
    fn next_layout(&self) -> Option<NodeId> {
        let slide = self.current_slide().and_then(|s| self.pres.slide(s))?;
        let (master, layout) = self.pres.layout(slide.layout)?;
        if matches!(layout.kind, LayoutKind::Title | LayoutKind::SectionHeader) {
            return master
                .layout_of_kind(&LayoutKind::TitleContent)
                .map(|l| l.id)
                .or(Some(layout.id));
        }
        Some(layout.id)
    }

    /// Selects a slide, as a click in the rail.
    pub(crate) fn select_slide(&mut self, index: usize, extend: bool, toggle: bool) {
        let n = self.pres.slides.len();
        if n == 0 {
            return;
        }
        let i = index.min(n - 1);
        self.end_text();
        self.view.shapes.clear();
        self.view.rail = true;
        if extend {
            let (a, b) = (self.view.anchor.min(i), self.view.anchor.max(i));
            self.view.slides = (a..=b).collect();
        } else if toggle {
            if let Some(k) = self.view.slides.iter().position(|s| *s == i) {
                if self.view.slides.len() > 1 {
                    self.view.slides.remove(k);
                }
            } else {
                self.view.slides.push(i);
                self.view.slides.sort_unstable();
            }
            self.view.anchor = i;
        } else {
            self.view.slides = vec![i];
            self.view.anchor = i;
        }
        self.view.current = i;
        if !self.view.slides.contains(&i) {
            self.view.current = self.view.slides.first().copied().unwrap_or(0);
        }
    }

    /// Shows slide `i` alone (after an insert, a move, Previous/Next).
    pub(crate) fn go_to(&mut self, i: usize) {
        let n = self.pres.slides.len();
        if n == 0 {
            return;
        }
        let i = i.min(n - 1);
        self.end_text();
        self.view.current = i;
        self.view.slides = vec![i];
        self.view.anchor = i;
        self.view.shapes.clear();
    }

    /// The bundled theme file of a master's theme, when it is one of Kiln's.
    pub(crate) fn bundled_of(&self) -> Option<ThemeFile> {
        let name = self.master()?.theme.name.clone();
        bundled()
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(&name) || t.id.eq_ignore_ascii_case(&name))
    }

    /// The master of the current slide.
    pub(crate) fn master(&self) -> Option<&kiln_deck::Master> {
        self.owner().and_then(|o| self.pres.master_of(o))
    }

    /// Slide and design commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn slide_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        let err = |e: kiln_deck::DeckError| KilnError::InvalidInput(e.to_string());
        match cmd {
            D::NewSlide { layout } => {
                let layout = match layout {
                    Some(l) => *l,
                    None => self
                        .next_layout()
                        .or_else(|| {
                            self.pres.masters.first().and_then(|m| {
                                m.layout_of_kind(&LayoutKind::TitleContent)
                                    .or_else(|| m.layouts.first())
                                    .map(|l| l.id)
                            })
                        })
                        .ok_or_else(|| KilnError::InvalidInput("no layout to use".into()))?,
                };
                let at = if self.pres.slides.is_empty() {
                    0
                } else {
                    self.view
                        .slides
                        .iter()
                        .max()
                        .copied()
                        .unwrap_or(self.view.current)
                        + 1
                };
                self.apply_with("deck.slides.new_slide", |p| {
                    kiln_deck::edit::insert_slide(p, layout, at)
                })?;
                self.go_to(at);
                self.view.rail = false;
                Ok(Change::Content)
            }
            D::Duplicate => {
                if !self.view.shapes.is_empty() && !self.view.rail {
                    // Ctrl+D on objects: copies a little down and to the right.
                    return self.duplicate_shapes_by(
                        kiln_deck_layout::geom::emu(12.0),
                        kiln_deck_layout::geom::emu(12.0),
                    );
                }
                let ids = self.selected_slide_ids();
                let last = self
                    .view
                    .slides
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or(self.view.current);
                self.apply_with("deck.slides.duplicate", |p| {
                    kiln_deck::edit::duplicate_slides(p, &ids)
                })?;
                let n = ids.len();
                self.view.slides = (last + 1..=last + n).collect();
                self.view.current = last + 1;
                self.view.anchor = last + 1;
                self.view.shapes.clear();
                self.view.rail = true;
                Ok(Change::Content)
            }
            D::DeleteSlides => {
                let ids = self.selected_slide_ids();
                if ids.is_empty() {
                    return Ok(Change::None);
                }
                let first = self
                    .view
                    .slides
                    .first()
                    .copied()
                    .unwrap_or(self.view.current);
                self.end_text();
                self.apply_with("deck.slides.delete", |p| {
                    kiln_deck::edit::delete_slides(p, &ids)
                })?;
                let n = self.pres.slides.len();
                let i = first.min(n.saturating_sub(1));
                self.go_to(i);
                self.view.rail = true;
                Ok(Change::Content)
            }
            D::MoveSlides { to } => {
                let ids = self.selected_slide_ids();
                let to = usize::try_from(*to)
                    .unwrap_or(0)
                    .min(self.pres.slides.len());
                let current = self.current_slide();
                self.apply_with("deck.slides.move", |p| {
                    kiln_deck::edit::move_slides(p, &ids, to)
                })?;
                // The moved slides stay selected where they landed.
                let idx: Vec<usize> = ids
                    .iter()
                    .filter_map(|id| self.pres.slide_index(*id))
                    .collect();
                self.view.slides = idx;
                self.view.slides.sort_unstable();
                if let Some(c) = current.and_then(|c| self.pres.slide_index(c)) {
                    self.view.current = c;
                    self.view.anchor = c;
                }
                self.view.rail = true;
                Ok(Change::Content)
            }
            D::SelectSlide {
                index,
                extend,
                toggle,
            } => {
                self.select_slide(usize::try_from(*index).unwrap_or(0), *extend, *toggle);
                Ok(Change::Selection)
            }
            D::PreviousSlide => {
                if self.view.current > 0 {
                    let i = self.view.current - 1;
                    let rail = self.view.rail;
                    self.go_to(i);
                    self.view.rail = rail;
                }
                Ok(Change::Selection)
            }
            D::NextSlide => {
                if self.view.current + 1 < self.pres.slides.len() {
                    let i = self.view.current + 1;
                    let rail = self.view.rail;
                    self.go_to(i);
                    self.view.rail = rail;
                }
                Ok(Change::Selection)
            }
            D::SetLayout { layout } => {
                let ids = self.selected_slide_ids();
                self.end_text();
                self.apply_with("deck.slides.layout", |p| {
                    kiln_deck::edit::change_layout(p, &ids, *layout)
                })?;
                self.view.shapes.clear();
                Ok(Change::Content)
            }
            D::ResetSlides => {
                let ids = self.selected_slide_ids();
                self.end_text();
                self.apply_with("deck.slides.reset_slide", |p| {
                    kiln_deck::edit::reset_slides(p, &ids)
                })?;
                Ok(Change::Content)
            }
            D::HideSlides { hidden } => {
                let ids = self.selected_slide_ids();
                let hide = hidden.unwrap_or_else(|| {
                    !ids.iter()
                        .all(|id| self.pres.slide(*id).is_some_and(|s| s.hidden))
                });
                self.apply_with("deck.show.hide_slide", |p| {
                    kiln_deck::edit::set_hidden(p, &ids, hide)
                })?;
                Ok(Change::Content)
            }
            D::AddSection { name } => {
                let at = self
                    .view
                    .slides
                    .first()
                    .copied()
                    .unwrap_or(self.view.current);
                let name = name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| "Untitled Section".into());
                self.apply_with("deck.slides.section", |p| {
                    kiln_deck::edit::add_section(p, at, &name)
                })?;
                Ok(Change::Content)
            }
            D::RenameSection { index, name } => {
                let id = self.section_id(*index)?;
                self.apply_with("deck.slides.rename_section", |p| {
                    kiln_deck::edit::rename_section(p, id, name)
                })?;
                Ok(Change::Content)
            }
            D::RemoveSection { index, with_slides } => {
                let id = self.section_id(*index)?;
                self.apply_with("deck.slides.remove_section", |p| {
                    kiln_deck::edit::remove_section(p, id, *with_slides)
                })?;
                self.validate_view();
                Ok(Change::Content)
            }
            D::MoveSection { index, delta } => {
                let id = self.section_id(*index)?;
                let d = isize::try_from(*delta).unwrap_or(0);
                self.apply_with("deck.slides.move_section", |p| {
                    kiln_deck::edit::move_section(p, id, d)
                })?;
                Ok(Change::Content)
            }
            D::SetNotes { text } => {
                let slide = self.slide_id()?;
                let now = self
                    .pres
                    .slide(slide)
                    .and_then(|s| s.notes.as_ref())
                    .map(|n| n.text.text())
                    .unwrap_or_default();
                if now == *text {
                    return Ok(Change::None);
                }
                self.apply_with("deck.notes.set_text", |p| {
                    kiln_deck::edit::set_notes_text(p, slide, text)
                })?;
                Ok(Change::Content)
            }
            D::ApplyTheme { name } => {
                let theme = bundled()
                    .into_iter()
                    .find(|t| t.id.eq_ignore_ascii_case(name) || t.name.eq_ignore_ascii_case(name))
                    .ok_or_else(|| {
                        KilnError::InvalidInput(format!("there is no theme called {name}"))
                    })?;
                self.end_text();
                self.apply_with("deck.design.themes", |p| {
                    kiln_deck::edit::apply_theme(p, &theme, None)
                })?;
                Ok(Change::Content)
            }
            D::ApplyVariant { index } => {
                let theme = self.bundled_of().ok_or_else(|| {
                    KilnError::InvalidInput(
                        "this theme has no Kiln variants; apply a Kiln theme first".into(),
                    )
                })?;
                let master = self
                    .master()
                    .map(|m| m.id)
                    .ok_or_else(|| KilnError::InvalidInput("no master".into()))?;
                let v = usize::try_from(*index).unwrap_or(0);
                self.apply_with("deck.design.variants", |p| {
                    kiln_deck::edit::apply_variant(p, master, &theme, v)
                })?;
                Ok(Change::Content)
            }
            D::SlideSize { size, mode } => {
                #[allow(clippy::cast_possible_truncation)]
                let size = match size {
                    DeckSlideSize::Widescreen => SlideSize::WIDESCREEN,
                    DeckSlideSize::Standard => SlideSize::STANDARD,
                    DeckSlideSize::Custom {
                        width_in,
                        height_in,
                    } => SlideSize::custom(
                        (f64::from(width_in.clamp(1.0, 56.0)) * 914_400.0).round() as i64,
                        (f64::from(height_in.clamp(1.0, 56.0)) * 914_400.0).round() as i64,
                    ),
                };
                let mode = match mode {
                    DeckScaleMode::Maximize => ScaleMode::Maximize,
                    DeckScaleMode::EnsureFit => ScaleMode::EnsureFit,
                };
                self.end_text();
                self.apply_with("deck.design.slide_size", |p| {
                    kiln_deck::edit::set_slide_size(p, size, mode)
                })?;
                Ok(Change::Content)
            }
            D::SetBackground { fill, all } => {
                let ids: Vec<NodeId> = if *all {
                    self.pres.slides.iter().map(|s| s.id).collect()
                } else {
                    self.selected_slide_ids()
                };
                let mut ops = Vec::new();
                let bg = match fill {
                    None => None,
                    Some(f) => {
                        let part = self.picture_part(f);
                        let name = part.as_ref().map(|(n, _)| n.clone());
                        if let Some((_, op)) = part {
                            ops.push(op);
                        }
                        Some(Background::Fill {
                            fill: fill_of(f, name.as_deref()),
                            shade_to_title: false,
                        })
                    }
                };
                let set = kiln_deck::edit::set_background(&mut self.pres, &ids, bg).map_err(err)?;
                ops.extend(set.ops);
                let tx = Transaction::new(ops).with_kind(EditKind::Format);
                self.apply("deck.design.format_background", tx)?;
                Ok(Change::Content)
            }
            D::HideBackgroundGraphics { hide } => {
                let ids = self.selected_slide_ids();
                let hide = hide.unwrap_or_else(|| {
                    ids.iter()
                        .all(|id| self.pres.slide(*id).is_some_and(|s| s.show_master_shapes))
                });
                let ops: Vec<Op> = ids
                    .iter()
                    .map(|id| Op::SetShowMasterShapes {
                        owner: *id,
                        show: !hide,
                    })
                    .collect();
                self.apply(
                    "deck.design.hide_background_graphics",
                    Transaction::new(ops),
                )?;
                Ok(Change::Content)
            }
            D::SetHeaderFooter {
                date,
                fixed_date,
                slide_number,
                footer,
                not_on_title,
                all,
            } => self.header_footer(
                *date,
                fixed_date.as_deref(),
                *slide_number,
                footer.as_deref(),
                *not_on_title,
                *all,
            ),
            _ => Ok(Change::None),
        }
    }

    fn section_id(&self, index: u32) -> Result<NodeId> {
        self.pres
            .sections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|s| s.id)
            .ok_or_else(|| KilnError::InvalidInput(format!("there is no section {index}")))
    }

    /// Insert ▸ Slide Number without text editing: turns the slide numbers on (all slides but
    /// the title slide).
    pub(crate) fn set_header_footer_slide_number(&mut self) -> Result<Change> {
        let (date, footer) = self.footer_state();
        self.header_footer(date, None, true, footer.as_deref(), true, true)
    }

    /// Whether the current slide shows a date and which footer.
    pub(crate) fn footer_state(&self) -> (bool, Option<String>) {
        let Some(s) = self.current_slide().and_then(|s| self.pres.slide(s)) else {
            return (false, None);
        };
        let has = |k: PlaceholderType| {
            s.shapes
                .iter()
                .find(|x| x.placeholder.is_some_and(|p| p.kind == k))
        };
        (
            has(PlaceholderType::Date).is_some(),
            has(PlaceholderType::Footer)
                .and_then(Shape::text)
                .map(TextBody::text),
        )
    }

    /// Header & Footer: adds or removes the date, footer and slide number placeholders.
    fn header_footer(
        &mut self,
        date: bool,
        fixed: Option<&str>,
        number: bool,
        footer: Option<&str>,
        not_on_title: bool,
        all: bool,
    ) -> Result<Change> {
        let ids: Vec<NodeId> = if all {
            self.pres.slides.iter().map(|s| s.id).collect()
        } else {
            self.selected_slide_ids()
        };
        self.end_text();
        self.begin_group();
        let mut result = Ok(());
        for id in ids {
            let Some(slide) = self.pres.slide(id).cloned() else {
                continue;
            };
            let is_title = self
                .pres
                .layout(slide.layout)
                .is_some_and(|(_, l)| l.kind == LayoutKind::Title);
            let skip = not_on_title && is_title;
            for (kind, want, text) in [
                (
                    PlaceholderType::Date,
                    date && !skip,
                    Some(fixed.map_or_else(|| "datetime1".to_owned(), |f| format!("text:{f}"))),
                ),
                (
                    PlaceholderType::Footer,
                    footer.is_some() && !skip,
                    footer.map(|f| format!("text:{f}")),
                ),
                (
                    PlaceholderType::SlideNumber,
                    number && !skip,
                    Some("slidenum".to_owned()),
                ),
            ] {
                let existing = slide
                    .shapes
                    .iter()
                    .find(|s| s.placeholder.is_some_and(|p| p.kind == kind))
                    .map(|s| s.id);
                let r = match (existing, want) {
                    (Some(e), false) => kiln_deck::edit::delete_shapes(&mut self.pres, &[e])
                        .map_err(|e| KilnError::InvalidInput(e.to_string()))
                        .and_then(|tx| self.apply("deck.insert.header_footer", tx).map(|_| ())),
                    (None, true) => {
                        let shape =
                            self.footer_shape(slide.layout, kind, text.as_deref().unwrap_or(""));
                        match shape {
                            Some(s) => {
                                kiln_deck::edit::insert_shapes(&mut self.pres, id, vec![s], None)
                                    .map_err(|e| KilnError::InvalidInput(e.to_string()))
                                    .and_then(|tx| {
                                        self.apply("deck.insert.header_footer", tx).map(|_| ())
                                    })
                            }
                            None => Ok(()),
                        }
                    }
                    (Some(e), true) if kind == PlaceholderType::Footer => {
                        let t = footer.unwrap_or("");
                        kiln_deck::edit::set_text(
                            &self.pres,
                            kiln_deck::ops::TextTarget::Shape(e),
                            t,
                        )
                        .map_err(|e| KilnError::InvalidInput(e.to_string()))
                        .and_then(|tx| self.apply("deck.insert.header_footer", tx).map(|_| ()))
                    }
                    _ => Ok(()),
                };
                if r.is_err() {
                    result = r;
                }
            }
        }
        self.end_group();
        result?;
        Ok(Change::Content)
    }

    /// A slide's date, footer or slide number placeholder, matching its layout's.
    fn footer_shape(
        &mut self,
        layout: NodeId,
        kind: PlaceholderType,
        content: &str,
    ) -> Option<Shape> {
        let ph = self
            .pres
            .layout(layout)
            .and_then(|(_, l)| {
                l.shapes
                    .iter()
                    .find(|s| s.placeholder.is_some_and(|p| p.kind == kind))
                    .and_then(|s| s.placeholder)
            })
            .unwrap_or_else(|| Placeholder::new(kind, 10 + kind as u32));
        let id = self.pres.alloc_id();
        let paragraph = |inl: Inlines| -> Option<TextBody> {
            let mut p = kiln_doc::Paragraph::new(NodeId(1));
            p.content = inl;
            TextBody::from_paragraphs(vec![p], std::collections::BTreeMap::new()).ok()
        };
        let body = if let Some(t) = content.strip_prefix("text:") {
            TextBody::from_text(t)
        } else {
            let result = match content {
                "slidenum" => "‹#›".to_owned(),
                _ => String::new(),
            };
            let inl = Inlines::from_fragment(Fragment::object(
                InlineObject::Field(Field::new(content, &result)),
                RunProps::default(),
            ))
            .ok()?;
            paragraph(inl)?
        };
        let name = match kind {
            PlaceholderType::Date => "Date Placeholder",
            PlaceholderType::Footer => "Footer Placeholder",
            _ => "Slide Number Placeholder",
        };
        let mut s = Shape::new(
            id,
            format!("{name} {}", id.0),
            None,
            ShapeKind::Geometry(AutoShape {
                text_box: false,
                props: ShapeProps::default(),
                text: Some(body),
            }),
        );
        s.placeholder = Some(ph);
        Some(s)
    }
}
