// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left presentations: a new presentation in Arabic, Hebrew, Persian or Urdu gets its
//! master and layouts mirrored, as PowerPoint's right-to-left templates are. Every text style
//! is right to left (`rtl="1"`) with left and right alignment swapped, placeholders and
//! decoration are mirrored across the slide, and gradients turn the other way. A theme applied
//! to such a presentation is mirrored the same way.

use kiln_doc::st::Alignment;
use kiln_ooxml::drawingml::fill::Fill;

use crate::geom::FULL_TURN;
use crate::presentation::Presentation;
use crate::shape::Shape;
use crate::slide::{Background, Master};
use crate::text::ListStyle;

/// Makes every level of a list style right to left, swapping left and right alignment.
pub fn mirror_list_style(style: &mut ListStyle) {
    for level in style.entries_mut() {
        level.para.bidi = Some(true);
        level.para.alignment = match level.para.alignment {
            Some(Alignment::Left) => Some(Alignment::Right),
            Some(Alignment::Right) => Some(Alignment::Left),
            other => other,
        };
    }
}

/// A fill seen in a mirror: gradients run the other way (pictures and patterns stay).
fn mirror_fill(fill: &mut Fill) {
    if let Fill::Gradient(g) = fill {
        if let Some(lin) = &mut g.linear {
            lin.angle = (180.0 - lin.angle).rem_euclid(360.0);
        }
        if let Some(rect) = g.path.as_mut().and_then(|p| p.fill_to_rect.as_mut()) {
            std::mem::swap(&mut rect.l, &mut rect.r);
        }
        if let Some(rect) = &mut g.tile_rect {
            std::mem::swap(&mut rect.l, &mut rect.r);
        }
    }
}

fn mirror_background(bg: &mut Background) {
    if let Background::Fill { fill, .. } = bg {
        mirror_fill(fill);
    }
}

/// Mirrors shapes across a vertical axis at `width / 2`: their boxes, rotation, the geometry
/// of decoration (text is never flipped) and the shapes' own list styles. A group's children
/// keep their places in its child space.
fn mirror_shapes(shapes: &mut [Shape], width: i64) {
    for s in shapes {
        let decoration = s.placeholder.is_none() && s.text().is_none();
        if let Some(x) = &mut s.xfrm {
            x.x = width - x.x - x.cx;
            x.rot = (FULL_TURN - x.rot).rem_euclid(FULL_TURN);
            if decoration {
                x.flip_h = !x.flip_h;
            }
        }
        if let Some(fill) = s.props_mut().and_then(|p| p.fill.as_mut()) {
            mirror_fill(fill);
        }
        if let Some(t) = s.text_mut() {
            mirror_list_style(&mut t.list_style);
        }
    }
}

/// Mirrors a master and its layouts for a right-to-left presentation `width` EMU wide.
pub fn mirror_master(master: &mut Master, width: i64) {
    mirror_list_style(&mut master.title_style);
    mirror_list_style(&mut master.body_style);
    mirror_list_style(&mut master.other_style);
    mirror_background(&mut master.background);
    mirror_shapes(&mut master.shapes, width);
    for layout in &mut master.layouts {
        if let Some(bg) = &mut layout.background {
            mirror_background(bg);
        }
        mirror_shapes(&mut layout.shapes, width);
    }
}

impl Presentation {
    /// Whether the presentation was made right to left: its first master's title style runs
    /// right to left.
    #[must_use]
    pub fn is_right_to_left(&self) -> bool {
        self.masters
            .first()
            .is_some_and(|m| m.title_style.level(0).para.bidi == Some(true))
    }

    /// Makes a new presentation right to left: masters, layouts, the notes master, the default
    /// text style and the slides' own shapes.
    pub fn make_right_to_left(&mut self) {
        let width = self.slide_size.cx;
        for m in &mut self.masters {
            mirror_master(m, width);
        }
        if let Some(n) = &mut self.notes_master {
            mirror_list_style(&mut n.notes_style);
        }
        mirror_list_style(&mut self.default_text_style);
        for s in &mut self.slides {
            if let Some(bg) = &mut s.background {
                mirror_background(bg);
            }
            mirror_shapes(&mut s.shapes, width);
        }
    }
}
