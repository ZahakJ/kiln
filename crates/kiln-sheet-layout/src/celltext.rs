// SPDX-License-Identifier: AGPL-3.0-or-later
//! The text of a cell as `kiln-text` paragraphs: fonts and rich-text runs, alignment, wrap,
//! shrink to fit, rotation and the text box inside the cell.

use kiln_scene::{Bounds, Paint};
use kiln_sheet::CellValue;
use kiln_sheet::styles::{Font, HAlign, Underline, VAlign, VertAlign};
use kiln_text::{
    Alignment, BLACK, ParagraphLayout, ParagraphProps, ParagraphSpec, SpanStyle, StrikeKind,
    TextEngine, TextRun, UnderlineKind,
};

use crate::look::{CellLook, Colors, pt_to_px};

/// Space between the cell's edges and its text.
pub const PAD: f32 = 3.0;
/// Width of one indent level.
pub const INDENT: f32 = 9.0;
/// Width that never wraps.
const NO_WRAP: f32 = 1.0e7;

/// A run style for a cell font at `scale` × its size.
#[must_use]
pub fn span_style(font: &Font, scale: f32, color: u32) -> SpanStyle {
    let mut size = pt_to_px(font.size.0).max(1.0) * scale;
    let mut shift = 0.0;
    match font.vert_align {
        VertAlign::Baseline => {}
        VertAlign::Superscript => {
            size *= 2.0 / 3.0;
            shift = size * 0.5;
        }
        VertAlign::Subscript => {
            size *= 2.0 / 3.0;
            shift = -size * 0.25;
        }
    }
    SpanStyle {
        family: font.name.to_string(),
        size,
        weight: if font.bold { 700 } else { 400 },
        italic: font.italic,
        color,
        underline: match font.underline {
            Underline::None => UnderlineKind::None,
            Underline::Single | Underline::SingleAccounting => UnderlineKind::Single,
            Underline::Double | Underline::DoubleAccounting => UnderlineKind::Double,
        },
        strike: if font.strike {
            StrikeKind::Single
        } else {
            StrikeKind::None
        },
        letter_spacing: 0.0,
        baseline_shift: shift,
        lang: None,
        rtl: false,
        kerning: false,
        ligatures: true,
    }
}

/// The runs of a cell's text with one paint per run (rich text keeps its own formatting).
#[derive(Debug, Clone, PartialEq)]
pub struct Runs {
    /// Text runs (their colour is a placeholder; see `paints`).
    pub runs: Vec<TextRun>,
    /// The paint of each run.
    pub paints: Vec<Paint>,
}

impl Runs {
    /// The plain text.
    #[must_use]
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// Literal paints travel through `kiln-text` as their colour; the placeholder stands for the
/// themed ink.
fn color_key(paint: Paint) -> u32 {
    match paint {
        Paint::Rgba(c) if c != BLACK => c,
        _ => BLACK,
    }
}

/// Runs for displayed `text` in the look's font, or for a rich-text value (whose runs keep their
/// formatting when the cell shows the value itself).
#[must_use]
pub fn runs_for(
    text: &str,
    value: &CellValue,
    look: &CellLook,
    ink: Paint,
    colors: Colors<'_>,
    scale: f32,
) -> Runs {
    if let CellValue::RichText(rich) = value
        && rich.plain().as_ref() == text
    {
        let mut runs = Vec::new();
        let mut paints = Vec::new();
        for r in rich.runs() {
            let font = r
                .font
                .as_ref()
                .map_or_else(|| look.font.clone(), |p| p.apply(&look.font));
            let paint = if r.font.as_ref().is_some_and(|p| p.color.is_some()) {
                colors.ink(&font, &look.fill)
            } else {
                ink
            };
            runs.push(TextRun::new(
                r.text.clone(),
                span_style(&font, scale, color_key(paint)),
            ));
            paints.push(paint);
        }
        return Runs { runs, paints };
    }
    Runs {
        runs: vec![TextRun::new(
            text,
            span_style(&look.font, scale, color_key(ink)),
        )],
        paints: vec![ink],
    }
}

/// Lays out runs on one line (no wrapping).
pub fn layout_line(engine: &mut TextEngine, runs: &Runs) -> ParagraphLayout {
    engine.layout(&ParagraphSpec {
        runs: runs.runs.clone(),
        props: ParagraphProps::default(),
        width: NO_WRAP,
    })
}

/// Lays out runs wrapped to `width` with a horizontal alignment.
pub fn layout_wrapped(
    engine: &mut TextEngine,
    runs: &Runs,
    width: f32,
    align: HAlign,
    numeric: bool,
) -> ParagraphLayout {
    let alignment = match align {
        HAlign::General if numeric => Alignment::Right,
        HAlign::General | HAlign::Left | HAlign::Fill => Alignment::Left,
        HAlign::Center | HAlign::CenterContinuous => Alignment::Center,
        HAlign::Right => Alignment::Right,
        HAlign::Justify => Alignment::Justify,
        HAlign::Distributed => Alignment::Distribute,
    };
    engine.layout(&ParagraphSpec {
        runs: runs.runs.clone(),
        props: ParagraphProps {
            alignment,
            ..ParagraphProps::default()
        },
        width: width.max(1.0),
    })
}

/// The width of a laid-out paragraph's widest line.
#[must_use]
pub fn text_width(layout: &ParagraphLayout) -> f32 {
    layout.lines.iter().map(|l| l.width).fold(0.0, f32::max)
}

/// Where a paragraph of `w × h` sits inside `cell`: the top-left of its box.
#[must_use]
pub fn place(
    cell: Bounds,
    w: f32,
    h: f32,
    align: HAlign,
    valign: VAlign,
    indent: u8,
    numeric: bool,
) -> (f32, f32) {
    let ind = f32::from(indent) * INDENT;
    let x = match align {
        HAlign::General if numeric => cell.right() - PAD - w,
        HAlign::Right => cell.right() - PAD - ind - w,
        HAlign::Center | HAlign::CenterContinuous | HAlign::Distributed if w < cell.w => {
            cell.x + (cell.w - w) / 2.0
        }
        HAlign::Center | HAlign::CenterContinuous => cell.x + (cell.w - w) / 2.0,
        _ => cell.x + PAD + ind,
    };
    let y = match valign {
        VAlign::Top | VAlign::Justify | VAlign::Distributed => cell.y + 1.0,
        VAlign::Center => cell.y + (cell.h - h) / 2.0,
        VAlign::Bottom => cell.bottom() - 1.0 - h,
    };
    (x, y)
}

/// The rotation of `text_rotation` in radians (counter-clockwise positive), or `None` for
/// horizontal and stacked text.
#[must_use]
pub fn rotation(text_rotation: u8) -> Option<f32> {
    match text_rotation {
        1..=90 => Some(f32::from(text_rotation).to_radians()),
        91..=180 => Some(-f32::from(text_rotation - 90).to_radians()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_sheet::num::F64;

    #[test]
    fn places_text_like_excel() {
        let cell = Bounds::new(0.0, 0.0, 64.0, 20.0);
        let (x, y) = place(cell, 20.0, 18.0, HAlign::General, VAlign::Bottom, 0, true);
        assert_eq!((x, y), (64.0 - PAD - 20.0, 1.0));
        let (x, _) = place(cell, 20.0, 18.0, HAlign::General, VAlign::Bottom, 0, false);
        assert_eq!(x, PAD);
        let (x, y) = place(cell, 20.0, 10.0, HAlign::Center, VAlign::Center, 0, false);
        assert_eq!((x, y), (22.0, 5.0));
        let (x, _) = place(cell, 20.0, 10.0, HAlign::Left, VAlign::Top, 2, false);
        assert_eq!(x, PAD + 2.0 * INDENT);
        assert!(rotation(45).is_some_and(|r| r > 0.0));
        assert!(rotation(135).is_some_and(|r| r < 0.0));
        assert_eq!(rotation(0), None);
        assert_eq!(rotation(255), None);
    }

    #[test]
    fn fonts_become_span_styles() {
        let f = Font {
            bold: true,
            size: F64(12.0),
            vert_align: VertAlign::Superscript,
            ..Font::default()
        };
        let s = span_style(&f, 1.0, BLACK);
        assert_eq!(s.weight, 700);
        assert!((s.size - 16.0 * 2.0 / 3.0).abs() < 1e-4);
        assert!(s.baseline_shift > 0.0);
    }
}
