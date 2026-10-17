// SPDX-License-Identifier: AGPL-3.0-or-later
//! The overlay above the cells (sheet pixels): the selection with its active cell and fill
//! handle (the brand kit's 2.5 px jade frame and square handle), the copy marquee, the fill
//! preview, the spill range of the active cell, the ranges a formula being edited refers to (each
//! in its own colour, matching the colours of the references in the editor and the formula bar),
//! and the in-cell editor itself: its box, its text with range colouring, the selection and the
//! caret, all laid out by `kiln-text` and drawn by the painter like any other text.

use kiln_formula::parser::{ParseOptions, RefAddress, TokenKind, tokenize};
use kiln_scene::{Bounds, ColorRole, DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::{CellPos, MAX_COLS, MAX_ROWS, Range};
use kiln_text::{ParagraphLayout, ParagraphProps, ParagraphSpec, TextEngine, TextRun};

use crate::celltext::{PAD, span_style};
use crate::geom::{FILL_HANDLE, SheetGeometry, Snap};
use crate::glyphs::GlyphCache;
use crate::look::{CellLook, rgb};

/// The colours of formula references, in order of first appearance (the brand's app colours,
/// then plum, teal and rose), `0xRRGGBB`.
pub const REF_COLORS: [u32; 7] = [
    0x1D_8A60, 0x31_59C6, 0xE0_5A27, 0x8E_4FB8, 0xB7_7F0E, 0x1F_8FA8, 0xC2_185B,
];

/// A reference in formula text: its byte span, the range (on the edited sheet when it names no
/// other sheet) and its colour.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaRef {
    /// Byte span in the text.
    pub span: std::ops::Range<usize>,
    /// The range, when it is on the sheet being edited.
    pub range: Option<Range>,
    /// `0xRRGGBB`.
    pub color: u32,
}

/// The references of a formula (text starting with `=`), coloured as Excel does: the same
/// reference text gets the same colour.
#[must_use]
pub fn formula_refs(text: &str, sheet_name: &str) -> Vec<FormulaRef> {
    let Some(body) = text
        .strip_prefix('=')
        .or_else(|| text.strip_prefix('+').filter(|_| text.len() > 1))
    else {
        return Vec::new();
    };
    let offset = text.len() - body.len();
    let opts = ParseOptions::default();
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for t in tokenize(body, &opts, offset as u32) {
        let TokenKind::Ref { prefix, address } = t.kind else {
            continue;
        };
        let (s, e) = (t.start as usize, t.end as usize);
        let Some(token_text) = text.get(s..e) else {
            continue;
        };
        let key = token_text.replace('$', "").to_ascii_uppercase();
        let k = match seen.iter().position(|x| *x == key) {
            Some(k) => k,
            None => {
                seen.push(key);
                seen.len() - 1
            }
        };
        let same_sheet = prefix.as_ref().is_none_or(|p| {
            p.workbook.is_none() && p.sheet2.is_none() && p.sheet.eq_ignore_ascii_case(sheet_name)
        });
        let range = same_sheet.then(|| address_range(&address)).flatten();
        out.push(FormulaRef {
            span: s..e,
            range,
            color: REF_COLORS[k % REF_COLORS.len()],
        });
    }
    out
}

fn address_range(a: &RefAddress) -> Option<Range> {
    Some(match *a {
        RefAddress::Cell { row, col, .. } => Range::new(row, col, row, col),
        RefAddress::Area { r0, c0, r1, c1, .. } => Range::new(r0, c0, r1, c1),
        RefAddress::Rows { r0, r1, .. } => Range::new(r0, 0, r1, MAX_COLS - 1),
        RefAddress::Cols { c0, c1, .. } => Range::new(0, c0, MAX_ROWS - 1, c1),
    })
}

/// The in-cell editor, laid out.
#[derive(Debug, Clone)]
pub struct Editor {
    /// The editor box (sheet pixels).
    pub rect: Bounds,
    /// The text.
    pub layout: ParagraphLayout,
    /// Paints of the runs.
    pub paints: Vec<Paint>,
    /// Top-left of the paragraph (sheet pixels).
    pub origin: (f32, f32),
    /// The caret (sheet pixels).
    pub caret: Bounds,
    /// Selected text (sheet pixels).
    pub selection: Vec<Bounds>,
}

/// Lays out the editor for `text` with the selection `sel` (byte offsets) over `cell` (sheet
/// pixels). The box grows to the right until `max_right`, then the text wraps; a wrapping cell
/// keeps its width.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn layout_editor(
    engine: &mut TextEngine,
    text: &str,
    sel: (usize, usize),
    cell: Bounds,
    look: &CellLook,
    wrap: bool,
    max_right: f32,
    refs: &[FormulaRef],
) -> Editor {
    let ink = look.ink;
    let key = |p: Paint| match p {
        Paint::Rgba(c) => c,
        Paint::Role(_) => kiln_text::BLACK,
    };
    // Runs split at the references, which take their colours.
    let mut runs = Vec::new();
    let mut paints = Vec::new();
    let mut at = 0usize;
    let push = |runs: &mut Vec<TextRun>, paints: &mut Vec<Paint>, t: &str, paint: Paint| {
        if !t.is_empty() {
            runs.push(TextRun::new(t, span_style(&look.font, 1.0, key(paint))));
            paints.push(paint);
        }
    };
    for r in refs {
        if r.span.start < at || r.span.end > text.len() {
            continue;
        }
        push(&mut runs, &mut paints, &text[at..r.span.start], ink);
        push(&mut runs, &mut paints, &text[r.span.clone()], rgb(r.color));
        at = r.span.end;
    }
    push(&mut runs, &mut paints, &text[at.min(text.len())..], ink);
    if runs.is_empty() {
        runs.push(TextRun::new("", span_style(&look.font, 1.0, key(ink))));
        paints.push(ink);
    }
    let avail_right = max_right.max(cell.right());
    let one_line = engine.layout(&ParagraphSpec {
        runs: runs.clone(),
        props: ParagraphProps::default(),
        width: 1.0e7,
    });
    let natural = one_line.lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let (width, layout) = if wrap || cell.x + natural + 2.0 * PAD + 2.0 > avail_right {
        let w = if wrap {
            cell.w
        } else {
            (avail_right - cell.x).max(cell.w)
        };
        let l = engine.layout(&ParagraphSpec {
            runs,
            props: ParagraphProps::default(),
            width: (w - 2.0 * PAD).max(1.0),
        });
        (w, l)
    } else {
        ((natural + 2.0 * PAD + 2.0).max(cell.w), one_line)
    };
    let height = (layout.height + 2.0).max(cell.h);
    let rect = Bounds::new(cell.x, cell.y, width, height);
    // Bottom-aligned like the cell when the text is one line shorter than the cell.
    let origin = (
        cell.x + PAD,
        (rect.bottom() - 1.0 - layout.height).max(rect.y + 1.0),
    );
    let (s, e) = (sel.0.min(text.len()), sel.1.min(text.len()));
    let c = layout.caret_rect(e);
    let caret = Bounds::new(origin.0 + c.x, origin.1 + c.top, 1.5, c.height);
    let selection = if s == e {
        Vec::new()
    } else {
        layout
            .selection_rects(s.min(e)..s.max(e))
            .into_iter()
            .map(|r| Bounds::new(origin.0 + r.x, origin.1 + r.y, r.width, r.height))
            .collect()
    };
    Editor {
        rect,
        layout,
        paints,
        origin,
        caret,
        selection,
    }
}

/// What the overlay shows.
#[derive(Debug, Clone, Default)]
pub struct OverlayInput {
    /// Selected ranges (the last one holds the active cell).
    pub ranges: Vec<Range>,
    /// The active cell.
    pub active: CellPos,
    /// Show the fill handle.
    pub fill_handle: bool,
    /// The range Copy or Cut marked.
    pub copy: Option<Range>,
    /// The target of a fill-handle drag in progress.
    pub fill_preview: Option<Range>,
    /// The spill range of the active cell.
    pub spill: Option<Range>,
    /// References of the formula being edited (ranges on this sheet).
    pub refs: Vec<(Range, u32)>,
    /// Hide the selection frame (a formula is being edited in point mode).
    pub editing: bool,
}

fn dashed_rect(rect: Bounds, paint: Paint, width: f32, dash: &[f32]) -> DrawOp {
    let mut p = PathData::new();
    p.move_to(rect.x, rect.y)
        .line_to(rect.right(), rect.y)
        .line_to(rect.right(), rect.bottom())
        .line_to(rect.x, rect.bottom())
        .close();
    DrawOp::Path {
        path: p,
        fill: None,
        stroke: Some(Stroke {
            dash: dash.to_vec(),
            ..Stroke::solid(paint, width)
        }),
        rule: FillRule::NonZero,
    }
}

/// Subtracts `hole` from `r` (up to four rectangles).
fn minus(r: Bounds, hole: Bounds) -> Vec<Bounds> {
    let Some(h) = r.intersection(&hole) else {
        return vec![r];
    };
    let mut out = Vec::new();
    if h.y > r.y {
        out.push(Bounds::from_corners(r.x, r.y, r.right(), h.y));
    }
    if h.bottom() < r.bottom() {
        out.push(Bounds::from_corners(r.x, h.bottom(), r.right(), r.bottom()));
    }
    if h.x > r.x {
        out.push(Bounds::from_corners(r.x, h.y, h.x, h.bottom()));
    }
    if h.right() < r.right() {
        out.push(Bounds::from_corners(h.right(), h.y, r.right(), h.bottom()));
    }
    out.retain(|b| b.w > 0.0 && b.h > 0.0);
    out
}

/// The overlay ops (sheet pixels).
#[must_use]
pub fn overlay_ops(geo: &SheetGeometry, input: &OverlayInput, snap: Snap) -> Vec<DrawOp> {
    let mut out = Vec::new();
    let clamp = |r: &Range| {
        // Whole rows and columns are drawn to the end of the sheet; the painter clips.
        geo.range_rect(&geo.expand_to_merges(*r))
    };
    // Formula references.
    for (r, color) in &input.refs {
        let rect = clamp(r);
        out.push(DrawOp::FillRect {
            rect,
            paint: Paint::Rgba((color << 8) | 0x17),
            radius: 0.0,
        });
        out.push(dashed_rect(
            rect.inflate(-0.75, -0.75),
            rgb(*color),
            1.5,
            &[4.0, 2.0],
        ));
    }
    if !input.ranges.is_empty() {
        let active = geo.display_rect(input.active.row, input.active.col);
        let multi = input.ranges.len() > 1
            || !input.ranges[0].is_single_cell()
                && geo.merge_at(input.ranges[0].r0, input.ranges[0].c0) != Some(input.ranges[0]);
        if multi {
            let mut rects = Vec::new();
            for r in &input.ranges {
                rects.extend(minus(clamp(r), active));
            }
            if !rects.is_empty() {
                out.push(DrawOp::SelectionRects {
                    rects,
                    paint: Paint::Role(ColorRole::AppTint),
                });
            }
        }
        if let Some(primary) = input.ranges.last() {
            let rect = clamp(primary);
            let r = Bounds::from_corners(
                snap.at(f64::from(rect.x)),
                snap.at(f64::from(rect.y)),
                snap.at(f64::from(rect.right())),
                snap.at(f64::from(rect.bottom())),
            );
            if !input.editing {
                out.push(DrawOp::StrokeRect {
                    rect: r.inflate(-0.25, -0.25).translate(-0.5, -0.5),
                    paint: Paint::Role(ColorRole::AppFill),
                    width: 2.5,
                    radius: 2.0,
                });
                if multi {
                    // The active cell keeps a thin frame inside a range.
                    out.push(DrawOp::StrokeRect {
                        rect: active.inflate(-1.5, -1.5),
                        paint: Paint::Role(ColorRole::AppFill),
                        width: 1.0,
                        radius: 1.0,
                    });
                }
                if input.fill_handle && input.ranges.len() == 1 {
                    let (hx, hy) = (r.right() - 0.5, r.bottom() - 0.5);
                    let h = Bounds::new(
                        hx - FILL_HANDLE / 2.0,
                        hy - FILL_HANDLE / 2.0,
                        FILL_HANDLE,
                        FILL_HANDLE,
                    );
                    out.push(DrawOp::FillRect {
                        rect: h.inflate(2.0, 2.0),
                        paint: Paint::Role(ColorRole::Page),
                        radius: 3.0,
                    });
                    out.push(DrawOp::FillRect {
                        rect: h,
                        paint: Paint::Role(ColorRole::AppFill),
                        radius: 2.0,
                    });
                }
            }
        }
    }
    if let Some(s) = input.spill {
        out.push(dashed_rect(
            clamp(&s).inflate(-1.0, -1.0),
            Paint::Role(ColorRole::AppText),
            1.0,
            &[3.0, 2.0],
        ));
    }
    if let Some(c) = input.copy {
        out.push(dashed_rect(
            clamp(&c).inflate(-1.0, -1.0),
            Paint::Role(ColorRole::AppFill),
            2.0,
            &[5.0, 3.0],
        ));
    }
    if let Some(f) = input.fill_preview {
        out.push(dashed_rect(
            clamp(&f).inflate(-1.0, -1.0),
            Paint::Role(ColorRole::Muted),
            1.5,
            &[4.0, 3.0],
        ));
    }
    out
}

/// The editor's ops (sheet pixels), drawn above everything else. Glyph positions are relative to
/// the document's device origin, as overlay glyphs are.
pub fn editor_ops(
    editor: &Editor,
    glyphs: &mut GlyphCache,
    engine: &TextEngine,
    out: &mut Vec<DrawOp>,
) {
    out.push(DrawOp::FillRect {
        rect: editor.rect,
        paint: Paint::Role(ColorRole::Page),
        radius: 0.0,
    });
    out.push(DrawOp::StrokeRect {
        rect: editor.rect.inflate(-0.25, -0.25).translate(-0.5, -0.5),
        paint: Paint::Role(ColorRole::AppFill),
        width: 2.5,
        radius: 2.0,
    });
    if !editor.selection.is_empty() {
        out.push(DrawOp::SelectionRects {
            rects: editor.selection.clone(),
            paint: Paint::Role(ColorRole::Selection),
        });
    }
    let s = glyphs.scale();
    let fallback = editor
        .paints
        .first()
        .copied()
        .unwrap_or(Paint::Role(ColorRole::Ink));
    for line in &editor.layout.lines {
        for run in &line.runs {
            let paint = editor.paints.get(run.span).copied().unwrap_or(fallback);
            glyphs.push_run(
                engine.fonts(),
                run,
                (editor.origin.0 * s, editor.origin.1 * s),
                paint,
                out,
            );
        }
        for d in &line.decorations {
            out.push(DrawOp::FillRect {
                rect: Bounds::new(
                    editor.origin.0 + d.x0,
                    editor.origin.1 + d.y,
                    d.x1 - d.x0,
                    d.thickness.max(0.5),
                ),
                paint: fallback,
                radius: 0.0,
            });
        }
    }
    out.push(DrawOp::Caret {
        rect: editor.caret,
        paint: Paint::Role(ColorRole::Caret),
        spark: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::AxisIndex;

    #[test]
    fn colours_references_like_excel() {
        let refs = formula_refs("=SUM(A1:B2)+A1*$a$1+Other!C3+A1:B2", "Sheet1");
        assert_eq!(refs.len(), 5);
        assert_eq!(refs[0].range, Some(Range::new(0, 0, 1, 1)));
        assert_eq!(refs[0].color, REF_COLORS[0]);
        assert_eq!(refs[1].color, REF_COLORS[1]);
        assert_eq!(refs[2].color, REF_COLORS[1], "$A$1 is the same cell as A1");
        assert_eq!(refs[3].range, None, "another sheet");
        assert_eq!(refs[4].color, REF_COLORS[0]);
        assert!(formula_refs("A1+B1", "Sheet1").is_empty(), "not a formula");
    }

    #[test]
    fn selection_frame_handle_and_wash() {
        let geo = SheetGeometry {
            rows: AxisIndex::new(100, 20.0),
            cols: AxisIndex::new(100, 64.0),
            freeze_rows: 0,
            freeze_cols: 0,
            headings: true,
            merges: Vec::new(),
        };
        let one = OverlayInput {
            ranges: vec![Range::cell(1, 1)],
            active: CellPos::new(1, 1),
            fill_handle: true,
            ..OverlayInput::default()
        };
        let ops = overlay_ops(&geo, &one, Snap(1.0));
        assert!(matches!(ops[0], DrawOp::StrokeRect { width, .. } if width == 2.5));
        assert_eq!(ops.len(), 3, "frame, handle halo, handle");
        let range = OverlayInput {
            ranges: vec![Range::new(1, 1, 3, 2)],
            ..one
        };
        let ops = overlay_ops(&geo, &range, Snap(1.0));
        let DrawOp::SelectionRects { rects, .. } = &ops[0] else {
            panic!("wash first");
        };
        let area: f32 = rects.iter().map(|r| r.w * r.h).sum();
        assert_eq!(
            area,
            128.0 * 60.0 - 64.0 * 20.0,
            "the active cell is not washed"
        );
        assert_eq!(
            minus(
                Bounds::new(0.0, 0.0, 10.0, 10.0),
                Bounds::new(20.0, 20.0, 1.0, 1.0)
            )
            .len(),
            1
        );
    }
}
