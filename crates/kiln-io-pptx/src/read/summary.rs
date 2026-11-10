// SPDX-License-Identifier: AGPL-3.0-or-later
//! What `kiln-cli inspect --pptx` prints ([`Summary`]), the shape vocabulary shared with the
//! python-pptx oracle ([`shape_kind`]), plain text the way PowerPoint copies it
//! ([`body_text`]) and the canonical facts the fidelity scorer compares ([`snapshot_facts`]).

use std::fmt::Write as _;

use kiln_deck::shape::{Geometry, Shape, ShapeKind};
use kiln_deck::slide::Background;
use kiln_deck::text::TextBody;
use kiln_deck::{Presentation, Slide};
use kiln_doc::InlineObject;
use kiln_doc::inline::OBJECT_CHAR;

use super::PptxRead;
use crate::common::extras::PptxExtras;

/// The kind of a shape in python-pptx's vocabulary (`placeholder`, `textbox`, `autoshape`,
/// `freeform`, `picture`, `group`, `table`, `chart`, `connector`, `media`, `ole`, `diagram`,
/// `other`), which the read tests compare with the oracle and the CLI counts.
#[must_use]
pub fn shape_kind(s: &Shape, extras: &PptxExtras) -> &'static str {
    if extras.is_smart_art(s.id) {
        return "diagram";
    }
    if s.placeholder.is_some() {
        return "placeholder";
    }
    if extras.is_ole(s.id) {
        return "ole";
    }
    match &s.kind {
        ShapeKind::Geometry(a) => match &a.props.geometry {
            Some(Geometry::Custom(_)) => "freeform",
            Some(Geometry::Preset { .. }) if !a.text_box => "autoshape",
            _ if a.text_box => "textbox",
            _ => "other",
        },
        ShapeKind::Picture(_) => "picture",
        ShapeKind::Group(_) => "group",
        ShapeKind::Table(_) => "table",
        ShapeKind::Chart(_) => "chart",
        ShapeKind::Connector(_) => "connector",
        ShapeKind::Media(_) => "media",
    }
}

/// The text of a body as PowerPoint copies it (and python-pptx reports it): paragraphs joined
/// by `\n`, a line break as `\v`, fields as their shown text.
#[must_use]
pub fn body_text(body: &TextBody) -> String {
    let mut out = String::new();
    for (i, p) in body.paragraphs().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let objects = p.content.objects();
        let mut k = 0;
        for c in p.content.raw_text().chars() {
            if c != OBJECT_CHAR {
                out.push(c);
                continue;
            }
            match objects.get(k) {
                Some(InlineObject::Break(_)) => out.push('\u{b}'),
                Some(InlineObject::Field(f)) => out.push_str(&f.result),
                _ => {}
            }
            k += 1;
        }
    }
    out
}

/// One slide in the summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideSummary {
    /// 1-based position.
    pub number: usize,
    /// The layout's name.
    pub layout: String,
    /// Top-level shapes.
    pub shapes: usize,
    /// The title placeholder's text (first line), if any.
    pub title: Option<String>,
    /// Hidden in the slide show.
    pub hidden: bool,
    /// The transition's name, if any.
    pub transition: Option<String>,
    /// Animation effects (main sequence and triggers).
    pub animations: usize,
    /// Characters of speaker notes.
    pub notes_chars: usize,
    /// Comments.
    pub comments: usize,
}

/// Counts and outline of a read presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// `pptx`, `pptm`, `potx`, …
    pub kind: &'static str,
    /// Slide width and height, EMU.
    pub size: (i64, i64),
    /// Each master's name and its layouts' names.
    pub masters: Vec<(String, Vec<String>)>,
    /// The slides.
    pub slides: Vec<SlideSummary>,
    /// Sections with their slide counts.
    pub sections: Vec<(String, usize)>,
    /// Shape counts by kind (python-pptx's vocabulary), over all slides and inside groups.
    pub shape_kinds: Vec<(&'static str, usize)>,
    /// Parts kept without being modelled.
    pub passthrough_parts: usize,
    /// Diagnostics.
    pub warnings: usize,
}

fn count_kinds(shapes: &[Shape], extras: &PptxExtras, out: &mut Vec<(&'static str, usize)>) {
    for s in shapes {
        let k = shape_kind(s, extras);
        match out.iter_mut().find(|(n, _)| *n == k) {
            Some((_, c)) => *c += 1,
            None => out.push((k, 1)),
        }
        if k != "diagram"
            && let Some(children) = s.children()
        {
            count_kinds(children, extras, out);
        }
    }
}

fn title_of(slide: &Slide) -> Option<String> {
    slide
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| p.kind.is_title()))
        .and_then(Shape::text)
        .map(|t| {
            body_text(t)
                .lines()
                .next()
                .unwrap_or_default()
                .replace('\u{b}', " ")
        })
        .filter(|t| !t.is_empty())
}

impl Summary {
    /// Summarises a read presentation.
    #[must_use]
    pub fn of(read: &PptxRead) -> Self {
        let p = &read.presentation;
        let mut shape_kinds = Vec::new();
        for s in &p.slides {
            count_kinds(&s.shapes, &read.extras, &mut shape_kinds);
        }
        shape_kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        Self {
            kind: read.kind.extension(),
            size: (p.slide_size.cx, p.slide_size.cy),
            masters: p
                .masters
                .iter()
                .map(|m| {
                    (
                        m.name.clone(),
                        m.layouts.iter().map(|l| l.name.clone()).collect(),
                    )
                })
                .collect(),
            slides: p
                .slides
                .iter()
                .enumerate()
                .map(|(i, s)| SlideSummary {
                    number: i + 1,
                    layout: p
                        .slide_layout(s)
                        .map(|(_, l)| l.name.clone())
                        .unwrap_or_default(),
                    shapes: s.shapes.len(),
                    title: title_of(s),
                    hidden: s.hidden,
                    transition: s
                        .transition
                        .as_ref()
                        .filter(|t| t.effect != kiln_deck::TransitionEffect::None)
                        .map(|t| t.effect.name().to_owned()),
                    animations: s.timeline.sequence.len()
                        + s.timeline
                            .triggers
                            .iter()
                            .map(|t| t.effects.len())
                            .sum::<usize>(),
                    notes_chars: s.notes_text().chars().count(),
                    comments: s.comments.len(),
                })
                .collect(),
            sections: p.sections.iter().map(|s| (s.name.clone(), s.len)).collect(),
            shape_kinds,
            passthrough_parts: read.passthrough.len(),
            warnings: read.warnings.len(),
        }
    }

    /// The summary as text: size, masters with their layouts, one line per slide, counts.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let inches = |emu: i64| emu as f64 / 914_400.0;
        let _ = writeln!(
            out,
            "Presentation ({}): {} slide{}, {:.2}\" × {:.2}\" ({} × {} EMU)",
            self.kind,
            self.slides.len(),
            if self.slides.len() == 1 { "" } else { "s" },
            inches(self.size.0),
            inches(self.size.1),
            self.size.0,
            self.size.1
        );
        out.push_str("\nMasters\n");
        for (i, (name, layouts)) in self.masters.iter().enumerate() {
            let _ = writeln!(out, "  {}. {name}: {} layouts", i + 1, layouts.len());
            for l in layouts {
                let _ = writeln!(out, "       - {l}");
            }
        }
        out.push_str("\nSlides\n");
        let w = self
            .slides
            .iter()
            .map(|s| s.layout.chars().count())
            .max()
            .unwrap_or(6)
            .max(6);
        let _ = writeln!(
            out,
            "  {:>3}  {:<w$}  {:>6}  TITLE",
            "#", "LAYOUT", "SHAPES"
        );
        for s in &self.slides {
            let mut extra = Vec::new();
            if s.hidden {
                extra.push("hidden".to_owned());
            }
            if let Some(t) = &s.transition {
                extra.push(format!("transition {t}"));
            }
            if s.animations > 0 {
                extra.push(format!("{} animations", s.animations));
            }
            if s.notes_chars > 0 {
                extra.push("notes".to_owned());
            }
            if s.comments > 0 {
                extra.push(format!("{} comments", s.comments));
            }
            let _ = writeln!(
                out,
                "  {:>3}  {:<w$}  {:>6}  {}{}",
                s.number,
                s.layout,
                s.shapes,
                s.title.as_deref().unwrap_or("—"),
                if extra.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", extra.join(", "))
                }
            );
        }
        if !self.sections.is_empty() {
            out.push_str("\nSections\n");
            for (name, len) in &self.sections {
                let _ = writeln!(out, "  {name}: {len} slides");
            }
        }
        out.push_str("\nCounts\n");
        for (k, n) in &self.shape_kinds {
            let _ = writeln!(out, "  {k} {n}");
        }
        let _ = writeln!(out, "  passthrough_parts {}", self.passthrough_parts);
        let _ = writeln!(out, "  diagnostics {}", self.warnings);
        out
    }
}

fn background_fact(b: &Background) -> String {
    match b {
        Background::Fill { fill, .. } => format!("fill:{}", fill_kind(fill)),
        Background::Theme { idx, .. } => format!("theme:{idx}"),
    }
}

fn fill_kind(f: &kiln_ooxml::drawingml::fill::Fill) -> &'static str {
    use kiln_ooxml::drawingml::fill::Fill;
    match f {
        Fill::None => "none",
        Fill::Solid(_) => "solid",
        Fill::Gradient(_) => "gradient",
        Fill::Pattern(_) => "pattern",
        Fill::Blip(_) => "picture",
        Fill::Group => "group",
    }
}

fn shape_facts(prefix: &str, shapes: &[Shape], extras: &PptxExtras, out: &mut Vec<String>) {
    for (i, s) in shapes.iter().enumerate() {
        let p = format!("{prefix}/{i}");
        out.push(format!("{p}={}:{}", shape_kind(s, extras), s.name));
        if let Some(x) = s.xfrm {
            out.push(format!(
                "{p}/xfrm={},{},{},{},{},{}{}",
                x.x,
                x.y,
                x.cx,
                x.cy,
                x.rot,
                u8::from(x.flip_h),
                u8::from(x.flip_v)
            ));
        }
        if let Some(ph) = s.placeholder {
            out.push(format!("{p}/ph={}:{}", ph.kind.as_str(), ph.idx));
        }
        if s.hidden {
            out.push(format!("{p}/hidden"));
        }
        if let Some(l) = &s.link {
            out.push(format!("{p}/link={l}"));
        }
        if let Some(props) = s.props() {
            if let Some(g) = &props.geometry {
                out.push(format!("{p}/geom={}", g.preset_name().unwrap_or("custom")));
            }
            if let Some(f) = &props.fill {
                out.push(format!("{p}/fill={}", fill_kind(f)));
            }
            if props.line.is_some() {
                out.push(format!("{p}/line"));
            }
            if props.style.is_some() {
                out.push(format!("{p}/style"));
            }
        }
        match &s.kind {
            ShapeKind::Geometry(a) => {
                if let Some(t) = &a.text {
                    out.push(format!("{p}/paragraphs={}", t.paragraph_count()));
                }
            }
            ShapeKind::Picture(pic) => {
                out.push(format!("{p}/image={}", pic.media));
                if !pic.crop.is_none() {
                    let c = pic.crop;
                    out.push(format!("{p}/crop={},{},{},{}", c.l, c.t, c.r, c.b));
                }
            }
            ShapeKind::Table(t) => {
                out.push(format!("{p}/table={}x{}", t.rows.len(), t.grid.len()));
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        if cell.grid_span > 1 || cell.row_span > 1 || cell.h_merge || cell.v_merge {
                            out.push(format!(
                                "{p}/cell{r},{c}={}x{}{}{}",
                                cell.grid_span,
                                cell.row_span,
                                if cell.h_merge { "h" } else { "" },
                                if cell.v_merge { "v" } else { "" }
                            ));
                        }
                    }
                }
            }
            ShapeKind::Chart(c) => out.push(format!("{p}/chart={}", c.part)),
            ShapeKind::Connector(c) => {
                out.push(format!(
                    "{p}/ends={},{}",
                    u8::from(c.start.is_some()),
                    u8::from(c.end.is_some())
                ));
            }
            ShapeKind::Media(m) => out.push(format!("{p}/media={:?}", m.kind)),
            ShapeKind::Group(g) => {
                if !extras.is_smart_art(s.id) {
                    shape_facts(&p, &g.children, extras, out);
                }
            }
        }
    }
}

/// The canonical facts of a read presentation, as `path=value` strings: slide size, masters
/// and layouts, and per slide its layout, flags, background, shapes (kind, name, transform,
/// placeholder, geometry, fill, table shape, chart, media), transition, animations, notes,
/// comments; sections. Ids are left out, so two reads of equivalent files give the same facts.
#[must_use]
pub fn snapshot_facts(read: &PptxRead) -> Vec<String> {
    let p: &Presentation = &read.presentation;
    let x = &read.extras;
    let mut out = vec![format!(
        "size={}x{}:{}",
        p.slide_size.cx,
        p.slide_size.cy,
        p.slide_size.kind.as_ooxml().unwrap_or("16x9")
    )];
    out.push(format!("firstSlideNum={}", p.first_slide_number));
    for (mi, m) in p.masters.iter().enumerate() {
        out.push(format!("master{mi}={}", m.name));
        out.push(format!("master{mi}/theme={}", m.theme.name));
        out.push(format!("master{mi}/bg={}", background_fact(&m.background)));
        shape_facts(&format!("master{mi}"), &m.shapes, x, &mut out);
        for (li, l) in m.layouts.iter().enumerate() {
            out.push(format!(
                "master{mi}/layout{li}={}:{}",
                l.kind.as_ooxml(),
                l.name
            ));
            shape_facts(&format!("master{mi}/layout{li}"), &l.shapes, x, &mut out);
        }
    }
    for (si, s) in p.slides.iter().enumerate() {
        let sp = format!("slide{si}");
        if let Some((_, l)) = p.slide_layout(s) {
            out.push(format!("{sp}/layout={}", l.name));
        }
        if s.hidden {
            out.push(format!("{sp}/hidden"));
        }
        if let Some(b) = &s.background {
            out.push(format!("{sp}/bg={}", background_fact(b)));
        }
        shape_facts(&sp, &s.shapes, x, &mut out);
        if let Some(t) = &s.transition {
            out.push(format!(
                "{sp}/transition={}:{}ms:{}:{:?}",
                t.effect.name(),
                t.duration_ms,
                u8::from(t.advance_on_click),
                t.advance_after_ms
            ));
        }
        for (ei, e) in s.timeline.sequence.iter().enumerate() {
            let (class, id, sub) = e.effect.ooxml_preset();
            out.push(format!(
                "{sp}/effect{ei}={}:{id}:{sub}:{:?}:{}ms+{}ms",
                class.as_str(),
                e.start,
                e.duration_ms,
                e.delay_ms
            ));
        }
        if !s.timeline.triggers.is_empty() {
            out.push(format!("{sp}/triggers={}", s.timeline.triggers.len()));
        }
        if s.notes.is_some() {
            out.push(format!("{sp}/notes"));
        }
        for (ci, c) in s.comments.iter().enumerate() {
            out.push(format!(
                "{sp}/comment{ci}={}:{}:{}",
                c.author,
                c.created,
                c.replies.len()
            ));
        }
    }
    for (i, sec) in p.sections.iter().enumerate() {
        out.push(format!("section{i}={}:{}", sec.name, sec.len));
    }
    out
}

/// Every slide's text and notes in order, for the fidelity text score.
#[must_use]
pub fn plain_text(read: &PptxRead) -> String {
    fn shapes_text(shapes: &[Shape], out: &mut String) {
        for s in shapes {
            match &s.kind {
                ShapeKind::Geometry(a) => {
                    if let Some(t) = &a.text {
                        out.push_str(&body_text(t));
                        out.push('\n');
                    }
                }
                ShapeKind::Table(t) => {
                    for row in &t.rows {
                        for c in &row.cells {
                            out.push_str(&body_text(&c.text));
                            out.push('\t');
                        }
                        out.push('\n');
                    }
                }
                ShapeKind::Group(g) => shapes_text(&g.children, out),
                _ => {}
            }
        }
    }
    let mut out = String::new();
    for s in &read.presentation.slides {
        shapes_text(&s.shapes, &mut out);
        if let Some(n) = &s.notes {
            out.push_str(&body_text(&n.text));
            out.push('\n');
        }
        for c in &s.comments {
            out.push_str(&c.text);
            out.push('\n');
        }
    }
    out
}
