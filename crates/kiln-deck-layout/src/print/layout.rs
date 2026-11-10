// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where things go on handouts, notes pages and outline pages, and their text.

use kiln_common::NodeId;
use kiln_deck::{Owner, PlaceholderType, Presentation, TextBody};
use kiln_scene::Bounds;
use kiln_text::{
    Alignment, ParagraphLayout, ParagraphProps, ParagraphSpec, SpanStyle, StrikeKind, TabAlignment,
    TabLeader, TabStop, TextEngine, TextRun, UnderlineKind,
};

use super::{INK, Placed, Printed, StructId, StructKind, StructTree};
use crate::geom::px;
use crate::text::{DEFAULT_SIZE_PT, TextEnv, span_style};

/// Side margins of handouts, notes pages and outline pages: 0.5 in.
pub const MARGIN: f32 = 48.0;
/// The band above and below handout slides for the header and footer: 0.75 in.
const BAND: f32 = 72.0;
/// The gap between handout slides: 0.25 in.
pub const GAP: f32 = 24.0;
/// The ruled lines beside three-per-page handouts are this far apart: 0.3 in.
const LINE_STEP: f32 = 28.8;
/// Notes text when nothing sets a size: 12 pt.
const NOTES_PT: f32 = 12.0;

/// The slides of one handout page: `per_page` rectangles in reading order (rows first, or
/// columns first when `vertical`), each the slide's shape fitted into its cell. Three per page
/// use the left half of the page (the lines for notes take the right).
#[must_use]
pub fn handout_cells(
    per_page: u8,
    vertical: bool,
    page: (f32, f32),
    slide: (f32, f32),
) -> Vec<Bounds> {
    let (cols, rows): (usize, usize) = match per_page {
        0 | 1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        5 | 6 => (2, 3),
        _ => (3, 3),
    };
    let area = Bounds::new(MARGIN, BAND, page.0 - 2.0 * MARGIN, page.1 - 2.0 * BAND);
    let area_w = if per_page == 3 {
        area.w / 2.0 - GAP
    } else {
        area.w
    };
    #[allow(clippy::cast_precision_loss)]
    let (cw, ch) = (
        (area_w - GAP * (cols - 1) as f32) / cols as f32,
        (area.h - GAP * (rows - 1) as f32) / rows as f32,
    );
    let aspect = slide.0 / slide.1.max(1.0);
    let (w, h) = if cw / ch.max(1.0) > aspect {
        (ch * aspect, ch)
    } else {
        (cw, cw / aspect)
    };
    let mut out = Vec::with_capacity(cols * rows);
    for k in 0..cols * rows {
        let (c, r) = if vertical {
            (k / rows, k % rows)
        } else {
            (k % cols, k / cols)
        };
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (
            area.x + c as f32 * (cw + GAP) + (cw - w) / 2.0,
            area.y + r as f32 * (ch + GAP) + (ch - h) / 2.0,
        );
        out.push(Bounds::new(x, y, w, h));
    }
    out
}

/// The y of each ruled line beside a slide of a three-per-page handout.
pub(super) fn note_lines(slide: Bounds) -> impl Iterator<Item = f32> {
    let mut y = slide.y + LINE_STEP;
    std::iter::from_fn(move || {
        if y > slide.bottom() + 0.5 {
            return None;
        }
        let out = y;
        y += LINE_STEP;
        Some(out)
    })
}

/// The slide image's and the notes' boxes on a notes page: the notes master's slide image and
/// body placeholders (the slide fitted and centred in its box), or PowerPoint's defaults.
#[must_use]
pub fn notes_boxes(pres: &Presentation, page: (f32, f32), slide: (f32, f32)) -> (Bounds, Bounds) {
    let placeholder = |kind: PlaceholderType| {
        let nm = pres.notes_master.as_ref()?;
        let s = nm
            .shapes
            .iter()
            .find(|s| s.placeholder.is_some_and(|p| p.kind == kind))?;
        let x = s.xfrm.as_ref()?;
        let b = Bounds::new(px(x.x), px(x.y), px(x.cx), px(x.cy));
        (b.w > 8.0 && b.h > 8.0 && b.right() <= page.0 + 1.0 && b.bottom() <= page.1 + 1.0)
            .then_some(b)
    };
    let image_box = placeholder(PlaceholderType::SlideImage).unwrap_or_else(|| {
        Bounds::new(
            2.0 * MARGIN,
            0.075 * page.1,
            page.0 - 4.0 * MARGIN,
            0.375 * page.1,
        )
    });
    let body = placeholder(PlaceholderType::Body).unwrap_or_else(|| {
        Bounds::new(
            1.5 * MARGIN,
            0.475 * page.1,
            page.0 - 3.0 * MARGIN,
            0.42 * page.1,
        )
    });
    let aspect = slide.0 / slide.1.max(1.0);
    let (w, h) = if image_box.w / image_box.h.max(1.0) > aspect {
        (image_box.h * aspect, image_box.h)
    } else {
        (image_box.w, image_box.w / aspect)
    };
    let image = Bounds::new(
        image_box.x + (image_box.w - w) / 2.0,
        image_box.y + (image_box.h - h) / 2.0,
        w,
        h,
    );
    (image, body)
}

/// A plain style for page numbers and the outline.
fn plain_style(size: f32, bold: bool) -> SpanStyle {
    SpanStyle {
        family: "Calibri".into(),
        size,
        weight: if bold { 700 } else { 400 },
        italic: false,
        color: INK,
        underline: UnderlineKind::None,
        strike: StrikeKind::None,
        letter_spacing: 0.0,
        baseline_shift: 0.0,
        lang: None,
        rtl: false,
        kerning: true,
        ligatures: true,
    }
}

/// One line of plain text.
pub(super) fn plain(
    engine: &mut TextEngine,
    text: &str,
    size: f32,
    bold: bool,
    width: f32,
) -> ParagraphLayout {
    engine.layout(&ParagraphSpec {
        runs: vec![TextRun::new(text, plain_style(size, bold))],
        props: ParagraphProps::default(),
        width,
    })
}

/// The speaker notes of a slide laid out in the notes page's body box, with the formatting
/// of their runs over the notes master's text style. Text that does not fit the box is cut, as
/// PowerPoint prints it.
pub(super) fn notes_text(
    engine: &mut TextEngine,
    pres: &Presentation,
    slide: NodeId,
    body: Bounds,
    tree: &mut StructTree,
) -> Vec<Placed> {
    let Some(notes) = pres.slide(slide).and_then(|s| s.notes.as_ref()) else {
        return Vec::new();
    };
    let owner = pres
        .notes_master
        .as_ref()
        .map_or(Owner::Slide(slide), |m| Owner::NotesMaster(m.id));
    let Some(theme) = pres.theme_of(owner).or_else(|| pres.theme()) else {
        return Vec::new();
    };
    let Some(ctx) = pres
        .color_context(owner)
        .or_else(|| pres.color_context(Owner::Slide(slide)))
    else {
        return Vec::new();
    };
    let env = TextEnv {
        theme,
        ctx,
        slide_number: pres.slide_index(slide).map_or(1, |i| i + 1),
        today: None,
        editing: false,
        fallback_color: Some(INK),
        fallback_bold: false,
    };
    let style = pres
        .notes_master
        .as_ref()
        .map(|m| m.notes_style.clone())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut y = body.y + 4.8;
    for para in notes.text.paragraphs() {
        let level = TextBody::level_of(para);
        let base = style.level(level).run;
        let mut runs = Vec::new();
        for (_, rprops, text) in para.content.run_texts() {
            let mut merged = kiln_doc::merge::merge_run_props(&base, Some(rprops));
            if merged.size.is_none() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    merged.size = Some((NOTES_PT * 2.0) as u32);
                }
            }
            let text: String = text
                .chars()
                .map(|c| if c == kiln_doc::OBJECT_CHAR { ' ' } else { c })
                .collect();
            runs.push(TextRun::new(text, span_style(&merged, &env, 1.0)));
        }
        #[allow(clippy::cast_precision_loss)]
        let indent = 36.0 * level as f32;
        let empty = runs.iter().all(|r| r.text.is_empty());
        let mark = plain_style(crate::geom::pt(NOTES_PT), false);
        let layout = engine.layout(&ParagraphSpec {
            runs,
            props: ParagraphProps {
                indent_start: indent,
                mark_style: Some(mark),
                ..ParagraphProps::default()
            },
            width: (body.w - 9.6).max(16.0),
        });
        if y + layout.height > body.bottom() {
            break;
        }
        let h = layout.height;
        if !empty {
            out.push(Placed {
                layout,
                x: body.x + 4.8,
                y,
                node: tree.add(StructKind::P, None),
            });
        }
        y += h;
    }
    out
}

/// The body text of a slide as outline lines: (level, text) of every paragraph of its body
/// placeholders, empty ones left out.
fn body_lines(pres: &Presentation, slide: NodeId) -> Vec<(usize, String)> {
    let Some(s) = pres.slide(slide) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for sh in &s.shapes {
        if !sh
            .placeholder
            .is_some_and(|p| !p.kind.is_title() && !p.kind.is_footer_area())
        {
            continue;
        }
        if let Some(t) = sh.text() {
            for p in t.paragraphs() {
                let text = p.content.plain_text(true);
                if !text.trim().is_empty() {
                    out.push((TextBody::level_of(p), text.replace('\n', " ")));
                }
            }
        }
    }
    out
}

/// The outline: every printed slide's number and title (bold) and its body text, flowing over
/// pages with 1 in at the top and bottom.
pub(super) fn outline_pages(
    engine: &mut TextEngine,
    pres: &Presentation,
    printed: &[Printed],
    page: (f32, f32),
    tree: &mut StructTree,
) -> Vec<Vec<Placed>> {
    let top = 2.0 * MARGIN;
    let bottom = page.1 - 2.0 * MARGIN;
    let width = page.0 - 3.0 * MARGIN;
    let x = 1.5 * MARGIN;
    let mut pages: Vec<Vec<Placed>> = vec![Vec::new()];
    let mut y = top;
    let mut place = |layout: ParagraphLayout, node: StructId, gap: f32, y: &mut f32| {
        if *y + layout.height > bottom && pages.last().is_some_and(|p| !p.is_empty()) {
            pages.push(Vec::new());
            *y = top;
        } else if pages.last().is_some_and(|p| !p.is_empty()) {
            *y += gap;
        }
        let h = layout.height;
        if let Some(p) = pages.last_mut() {
            p.push(Placed {
                layout,
                x,
                y: *y,
                node,
            });
        }
        *y += h;
    };
    let title_size = crate::geom::pt(DEFAULT_SIZE_PT);
    let body_size = crate::geom::pt(14.0);
    for p in printed {
        let section = tree.add(StructKind::Section, None);
        let title = if p.title.is_empty() {
            String::new()
        } else {
            p.title.clone()
        };
        let heading = tree.add(
            StructKind::Heading {
                level: 1,
                title: if title.is_empty() {
                    format!("Slide {}", p.number)
                } else {
                    title.clone()
                },
            },
            Some(section),
        );
        let hang = MARGIN;
        let layout = engine.layout(&ParagraphSpec {
            runs: vec![TextRun::new(
                format!("{}\t{title}", p.number),
                plain_style(title_size, true),
            )],
            props: ParagraphProps {
                indent_start: hang,
                first_line: -hang,
                tab_stops: vec![TabStop::new(hang, TabAlignment::Start, TabLeader::None)],
                ..ParagraphProps::default()
            },
            width,
        });
        place(layout, heading, 14.0, &mut y);
        for (level, text) in body_lines(pres, p.id) {
            #[allow(clippy::cast_precision_loss)]
            let indent = hang + 24.0 + 30.0 * level as f32;
            let layout = engine.layout(&ParagraphSpec {
                runs: vec![TextRun::new(
                    format!("\u{2022}\t{text}"),
                    plain_style(body_size, false),
                )],
                props: ParagraphProps {
                    alignment: Alignment::Start,
                    indent_start: indent,
                    first_line: -18.0,
                    tab_stops: vec![TabStop::new(indent, TabAlignment::Start, TabLeader::None)],
                    ..ParagraphProps::default()
                },
                width,
            });
            let node = tree.add(StructKind::P, Some(section));
            place(layout, node, 3.0, &mut y);
        }
    }
    pages
}
