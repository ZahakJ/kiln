// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reuse Slides: the slides of another presentation listed with thumbnails, and those
//! picked inserted after the current slide on this presentation's layouts and theme — the
//! layout of the same name, else the one of the same kind, else the first (PowerPoint's Use
//! Destination Theme). Their pictures, media and charts come along; a part whose name this
//! presentation already uses for something else is renamed (a chart's relationships follow).

use std::collections::BTreeMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, Transaction};
use kiln_deck::shape::MediaSource;
use kiln_deck::{Background, Part, Presentation, Shape, ShapeKind, Slide};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_protocol::commands::deck_features::DeckReuseSlide;

use super::super::DeckDoc;
use crate::model::Change;

fn read(bytes: &[u8]) -> Result<Presentation> {
    kiln_io_pptx::read_pptx(bytes)
        .map(|r| r.presentation)
        .map_err(|e| KilnError::Format {
            format: "pptx".into(),
            message: e.to_string(),
        })
}

/// The picture part of a fill.
pub(crate) fn blip(fill: Option<&Fill>) -> Option<&String> {
    match fill {
        Some(Fill::Blip(b)) => b.embed.as_ref(),
        _ => None,
    }
}

pub(crate) fn blip_mut(fill: Option<&mut Fill>) -> Option<&mut String> {
    match fill {
        Some(Fill::Blip(b)) => b.embed.as_mut(),
        _ => None,
    }
}

/// Every part key a shape (and its children) refers to.
pub(crate) fn shape_refs(s: &Shape, out: &mut Vec<String>) {
    s.walk(&mut |x| {
        match &x.kind {
            ShapeKind::Picture(p) => out.push(p.media.clone()),
            ShapeKind::Chart(c) => out.push(c.part.clone()),
            ShapeKind::Media(m) => {
                if let MediaSource::Embedded(k) = &m.source {
                    out.push(k.clone());
                }
                out.extend(m.poster.clone());
            }
            ShapeKind::Table(t) => {
                let cells = t.rows.iter().flat_map(|r| &r.cells);
                out.extend(cells.filter_map(|c| blip(c.props.fill.as_ref())).cloned());
            }
            _ => {}
        }
        out.extend(blip(x.props().and_then(|p| p.fill.as_ref())).cloned());
    });
}

/// Every part key a slide refers to: its shapes, its background and its notes page.
fn part_refs(s: &Slide, out: &mut Vec<String>) {
    for sh in &s.shapes {
        shape_refs(sh, out);
    }
    if let Some(Background::Fill { fill, .. }) = &s.background {
        out.extend(blip(Some(fill)).cloned());
    }
    for sh in s.notes.iter().flat_map(|n| &n.shapes) {
        shape_refs(sh, out);
    }
}

/// Renames part keys in a shape (and its children).
pub(crate) fn rename_shape_refs(s: &mut Shape, map: &BTreeMap<String, String>) {
    let fix = |k: &mut String| {
        if let Some(n) = map.get(k.as_str()) {
            k.clone_from(n);
        }
    };
    match &mut s.kind {
        ShapeKind::Picture(p) => fix(&mut p.media),
        ShapeKind::Chart(c) => fix(&mut c.part),
        ShapeKind::Media(m) => {
            if let MediaSource::Embedded(k) = &mut m.source {
                fix(k);
            }
            if let Some(p) = m.poster.as_mut() {
                fix(p);
            }
        }
        ShapeKind::Group(g) => {
            for c in &mut g.children {
                rename_shape_refs(c, map);
            }
        }
        ShapeKind::Table(t) => {
            for c in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                if let Some(k) = blip_mut(c.props.fill.as_mut()) {
                    fix(k);
                }
            }
        }
        ShapeKind::Geometry(_) | ShapeKind::Connector(_) => {}
    }
    if let Some(k) = blip_mut(s.props_mut().and_then(|p| p.fill.as_mut())) {
        fix(k);
    }
}

/// Renames part keys in a slide: its shapes, its background and its notes page.
fn rename_refs(s: &mut Slide, map: &BTreeMap<String, String>) {
    for sh in &mut s.shapes {
        rename_shape_refs(sh, map);
    }
    if let Some(Background::Fill { fill, .. }) = s.background.as_mut()
        && let Some(k) = blip_mut(Some(fill))
        && let Some(n) = map.get(k.as_str())
    {
        k.clone_from(n);
    }
    for sh in s.notes.iter_mut().flat_map(|n| &mut n.shapes) {
        rename_shape_refs(sh, map);
    }
}

/// The relationships part of a part (`/ppt/charts/_rels/chart1.xml.rels`).
fn rels_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// The internal targets of a relationships part, resolved against `part`'s folder.
fn targets(rels: &[u8], part: &str) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(rels);
    let dir = part.rsplit_once('/').map_or("", |(d, _)| d);
    let mut out = Vec::new();
    for chunk in text.split("<Relationship ").skip(1) {
        if chunk.contains("TargetMode=\"External\"") {
            continue;
        }
        let Some(t) = chunk
            .split("Target=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
        else {
            continue;
        };
        let mut parts: Vec<&str> = if t.starts_with('/') {
            Vec::new()
        } else {
            dir.split('/').filter(|p| !p.is_empty()).collect()
        };
        for seg in t.split('/').filter(|p| !p.is_empty()) {
            match seg {
                "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        out.push((t.to_owned(), format!("/{}", parts.join("/"))));
    }
    out
}

/// A part name not used in `taken`: `stem` + the next free number + `ext`.
pub(crate) fn fresh(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (base, ext) = name.rsplit_once('.').map_or((name, ""), |(b, e)| (b, e));
    let stem = base.trim_end_matches(|c: char| c.is_ascii_digit());
    (1..)
        .map(|n| {
            if ext.is_empty() {
                format!("{stem}{n}")
            } else {
                format!("{stem}{n}.{ext}")
            }
        })
        .find(|c| !taken(c))
        .unwrap_or_else(|| name.to_owned())
}

impl DeckDoc {
    /// The slides of another presentation, with thumbnails `width` CSS px wide at `dpr`.
    pub(crate) fn reuse_list(
        &self,
        bytes: &[u8],
        width: f32,
        dpr: f32,
    ) -> Result<Vec<DeckReuseSlide>> {
        let other = read(bytes)?;
        let mut renderer =
            kiln_deck_layout::Renderer::new(super::super::fonts(), super::super::fonts());
        let slide_w = kiln_deck_layout::geom::px(other.slide_size.cx).max(1.0);
        let scale = width.clamp(48.0, 480.0) * dpr.clamp(0.5, 4.0) / slide_w;
        other
            .slides
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Ok(DeckReuseSlide {
                    index: u32::try_from(i).unwrap_or(u32::MAX),
                    title: kiln_deck_layout::a11y::slide_title(&other, s.id),
                    png: super::export::render_png(&mut renderer, &other, s.id, scale)?,
                })
            })
            .collect()
    }

    /// Inserts slides of another presentation after the current one.
    pub(crate) fn reuse_slides(&mut self, bytes: &[u8], picked: &[u32]) -> Result<Change> {
        let other = read(bytes)?;
        let indices: Vec<usize> = if picked.is_empty() {
            (0..other.slides.len()).collect()
        } else {
            picked
                .iter()
                .map(|i| {
                    usize::try_from(*i)
                        .ok()
                        .filter(|i| *i < other.slides.len())
                        .ok_or_else(|| {
                            KilnError::InvalidInput(format!("there is no slide {}", i + 1))
                        })
                })
                .collect::<Result<_>>()?
        };
        if indices.is_empty() {
            return Ok(Change::None);
        }
        let master = self
            .pres
            .masters
            .first()
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no master".into()))?;
        // Parts: copied when new, kept when the same bytes are here already, renamed when the
        // name means something else here.
        let mut refs = Vec::new();
        for i in &indices {
            part_refs(&other.slides[*i], &mut refs);
        }
        refs.sort();
        refs.dedup();
        let mut copy: BTreeMap<String, Part> = BTreeMap::new();
        let mut rename: BTreeMap<String, String> = BTreeMap::new();
        let mut queue: Vec<String> = refs;
        while let Some(name) = queue.pop() {
            if copy.contains_key(&name) || rename.contains_key(&name) {
                continue;
            }
            let Some(part) = other.parts.get(&name) else {
                continue;
            };
            let here = self.pres.parts.get(&name);
            if here.is_some_and(|h| h.bytes == part.bytes) {
                continue;
            }
            let target = if here.is_some() {
                let taken = |c: &str| {
                    self.pres.parts.contains_key(c)
                        || copy.contains_key(c)
                        || other.parts.contains_key(c)
                };
                let n = fresh(&name, &taken);
                rename.insert(name.clone(), n.clone());
                n
            } else {
                name.clone()
            };
            copy.insert(target.clone(), part.clone());
            // A chart's relationships and what they point at come with it.
            let rels = rels_of(&name);
            if let Some(r) = other.parts.get(&rels) {
                let mut xml = String::from_utf8_lossy(&r.bytes).into_owned();
                for (raw, abs) in targets(&r.bytes, &name) {
                    let Some(tp) = other.parts.get(&abs) else {
                        continue;
                    };
                    let clash = self
                        .pres
                        .parts
                        .get(&abs)
                        .is_some_and(|h| h.bytes != tp.bytes);
                    if clash {
                        let taken = |c: &str| {
                            self.pres.parts.contains_key(c)
                                || copy.contains_key(c)
                                || other.parts.contains_key(c)
                        };
                        let n = fresh(&abs, &taken);
                        let new_raw = match (raw.rsplit_once('/'), n.rsplit_once('/')) {
                            (Some((dir, _)), Some((_, file))) => format!("{dir}/{file}"),
                            _ => n.clone(),
                        };
                        xml = xml.replace(
                            &format!("Target=\"{raw}\""),
                            &format!("Target=\"{new_raw}\""),
                        );
                        copy.insert(n, tp.clone());
                    } else if !self.pres.parts.contains_key(&abs) {
                        copy.insert(abs, tp.clone());
                    }
                }
                copy.insert(
                    rels_of(&target),
                    Part {
                        content_type: r.content_type.clone(),
                        bytes: xml.into_bytes(),
                    },
                );
            }
        }
        // The slides, on this presentation's layouts.
        let layout_for = |s: &Slide| -> Option<NodeId> {
            let (_, l) = other.layout(s.layout)?;
            master
                .layouts
                .iter()
                .find(|x| x.name == l.name)
                .or_else(|| master.layout_of_kind(&l.kind))
                .or_else(|| master.layouts.first())
                .map(|x| x.id)
        };
        let fallback = master.layouts.first().map(|l| l.id);
        let layouts: Vec<Option<NodeId>> = indices
            .iter()
            .map(|i| layout_for(&other.slides[*i]).or(fallback))
            .collect();
        let mut slides = Vec::new();
        for (i, layout) in indices.iter().zip(layouts) {
            let Some(layout) = layout else {
                continue;
            };
            // Fresh ids for the slide, its shapes and effects, the animations following their
            // shapes. Comments stay behind (their authors are the other presentation's).
            let mut slide = kiln_deck::edit::copy_slide(&mut self.pres, &other.slides[*i]);
            slide.layout = layout;
            slide.comments.clear();
            rename_refs(&mut slide, &rename);
            slides.push(slide);
        }
        let mut ops: Vec<Op> = copy
            .into_iter()
            .map(|(key, part)| Op::SetPart {
                key,
                part: Some(part),
            })
            .collect();
        let at = if self.pres.slides.is_empty() {
            0
        } else {
            self.view.current + 1
        };
        let n = slides.len();
        for (k, slide) in slides.into_iter().enumerate() {
            ops.push(Op::InsertSlide {
                index: at + k,
                slide: Box::new(slide),
            });
        }
        if !self.pres.sections.is_empty() && n > 0 {
            let mut sec = self.pres.sections.clone();
            let target = if at == 0 {
                0
            } else {
                self.pres.section_of_index(at - 1).unwrap_or(0)
            };
            if let Some(s) = sec.get_mut(target) {
                s.len += n;
            }
            ops.push(Op::SetSections { sections: sec });
        }
        self.end_text();
        self.apply("deck.insert.reuse_slides_from", Transaction::new(ops))?;
        self.view.slides = (at..at + n).collect();
        self.view.current = at;
        self.view.anchor = at;
        self.view.shapes.clear();
        Ok(Change::Content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationship_targets_resolve_and_names_stay_free() {
        let rels = br#"<Relationships><Relationship Id="rId1" Target="../embeddings/Book1.xlsx"/><Relationship Id="rId2" TargetMode="External" Target="http://x"/><Relationship Id="rId3" Target="style1.xml"/></Relationships>"#;
        let t = targets(rels, "/ppt/charts/chart1.xml");
        assert_eq!(
            t,
            vec![
                (
                    "../embeddings/Book1.xlsx".into(),
                    "/ppt/embeddings/Book1.xlsx".into()
                ),
                ("style1.xml".into(), "/ppt/charts/style1.xml".into()),
            ]
        );
        assert_eq!(
            rels_of("/ppt/charts/chart1.xml"),
            "/ppt/charts/_rels/chart1.xml.rels"
        );
        let taken = |c: &str| matches!(c, "/ppt/media/image1.png" | "/ppt/media/image2.png");
        assert_eq!(
            fresh("/ppt/media/image1.png", &taken),
            "/ppt/media/image3.png"
        );
    }
}
