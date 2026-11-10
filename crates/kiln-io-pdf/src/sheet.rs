// SPDX-License-Identifier: AGPL-3.0-or-later
//! Imposition: which pages go on which sheet of paper, where and how large. Shared by the PDF
//! writer and the preview so both place pages identically.

use kiln_common::{KilnError, Result};

use crate::options::{PdfOptions, Scaling};
use crate::printable::PageInfo;

/// Points per CSS px (72 / 96).
pub const PT_PER_PX: f32 = 0.75;
/// The paper margin around a grid of several pages per sheet, pt.
const GRID_MARGIN: f32 = 18.0;
/// The gap between pages of a grid, pt.
const GRID_GAP: f32 = 9.0;

/// One page placed on a sheet: page px → sheet pt is `x' = x × scale + dx`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// The page (0-based index into the document's pages).
    pub page: usize,
    /// Scale from page px to sheet pt.
    pub scale: f32,
    /// Horizontal offset on the sheet, pt.
    pub dx: f32,
    /// Vertical offset on the sheet, pt.
    pub dy: f32,
    /// The cell the page may paint into (clip), on the sheet in pt: x, y, w, h.
    pub clip: [f32; 4],
}

impl Slot {
    /// Maps a page point (px) to the sheet (pt).
    #[must_use]
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.dx, y * self.scale + self.dy)
    }
}

/// A sheet of paper.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Width, pt.
    pub width: f32,
    /// Height, pt.
    pub height: f32,
    /// Pages on it, in reading order.
    pub slots: Vec<Slot>,
    /// Which copy this sheet belongs to (0 = the first; later copies carry no structure).
    pub copy: u32,
}

/// Lays the selected pages out on sheets.
///
/// # Errors
///
/// `InvalidInput` for invalid options or when the selection contains no page of the document.
pub fn impose(pages: &[PageInfo], options: &PdfOptions) -> Result<Vec<Sheet>> {
    options.validate()?;
    let selected = options.pages_of(pages.len());
    if selected.is_empty() {
        return Err(KilnError::InvalidInput(format!(
            "the page selection has no pages in this {}-page document",
            pages.len()
        )));
    }
    let size = |p: usize| {
        let info = &pages[p];
        (
            (info.width * PT_PER_PX).max(1.0),
            (info.height * PT_PER_PX).max(1.0),
        )
    };
    let n = options.per_sheet.max(1) as usize;
    let mut one_copy = Vec::new();
    for group in selected.chunks(n) {
        let (pw, ph) = size(group[0]);
        let sheet = if n == 1 {
            match options.scaling {
                Scaling::Actual => {
                    let s = options.scale;
                    let (w, h) = (pw * s, ph * s);
                    Sheet {
                        width: w,
                        height: h,
                        slots: vec![Slot {
                            page: group[0],
                            scale: PT_PER_PX * s,
                            dx: 0.0,
                            dy: 0.0,
                            clip: [0.0, 0.0, w, h],
                        }],
                        copy: 0,
                    }
                }
                Scaling::FitTo(paper) => {
                    // Turn the paper with the page.
                    let (lo, hi) = (paper.width.min(paper.height), paper.width.max(paper.height));
                    let (w, h) = if pw > ph { (hi, lo) } else { (lo, hi) };
                    let mut sheet = Sheet {
                        width: w,
                        height: h,
                        slots: Vec::new(),
                        copy: 0,
                    };
                    sheet
                        .slots
                        .push(fit(group[0], (pw, ph), [0.0, 0.0, w, h], options.scale));
                    sheet
                }
            }
        } else {
            let (mut cols, mut rows) = options.grid();
            let landscape_pages = pw > ph;
            if landscape_pages {
                std::mem::swap(&mut cols, &mut rows);
            }
            // The paper: the chosen one, else the first page's; turned so the grid's cells
            // come out closest to the pages' shape.
            let (bw, bh) = match options.scaling {
                Scaling::FitTo(p) => (p.width.min(p.height), p.width.max(p.height)),
                Scaling::Actual => (pw.min(ph), pw.max(ph)),
            };
            #[allow(clippy::cast_precision_loss)]
            let (c, r) = (cols as f32, rows as f32);
            let portrait_fit = cell_fit((bw, bh), (c, r), (pw, ph));
            let landscape_fit = cell_fit((bh, bw), (c, r), (pw, ph));
            let (w, h) = if landscape_fit > portrait_fit {
                (bh, bw)
            } else {
                (bw, bh)
            };
            let cw = (w - 2.0 * GRID_MARGIN - (c - 1.0) * GRID_GAP) / c;
            let ch = (h - 2.0 * GRID_MARGIN - (r - 1.0) * GRID_GAP) / r;
            let slots = group
                .iter()
                .enumerate()
                .map(|(i, &page)| {
                    #[allow(clippy::cast_precision_loss)]
                    let (col, row) = ((i % cols as usize) as f32, (i / cols as usize) as f32);
                    let cell = [
                        GRID_MARGIN + col * (cw + GRID_GAP),
                        GRID_MARGIN + row * (ch + GRID_GAP),
                        cw,
                        ch,
                    ];
                    fit(page, size(page), cell, options.scale)
                })
                .collect();
            Sheet {
                width: w,
                height: h,
                slots,
                copy: 0,
            }
        };
        one_copy.push(sheet);
    }
    let mut out = Vec::with_capacity(one_copy.len() * options.copies as usize);
    for copy in 0..options.copies.max(1) {
        out.extend(one_copy.iter().cloned().map(|mut s| {
            s.copy = copy;
            s
        }));
    }
    Ok(out)
}

/// How large a page of `page` size comes out in a grid cell of a sheet.
fn cell_fit(sheet: (f32, f32), grid: (f32, f32), page: (f32, f32)) -> f32 {
    let cw = (sheet.0 - 2.0 * GRID_MARGIN - (grid.0 - 1.0) * GRID_GAP) / grid.0;
    let ch = (sheet.1 - 2.0 * GRID_MARGIN - (grid.1 - 1.0) * GRID_GAP) / grid.1;
    (cw / page.0).min(ch / page.1)
}

/// A page of `size` (pt) fitted into `cell`, centred, then zoomed by `zoom`.
fn fit(page: usize, size: (f32, f32), cell: [f32; 4], zoom: f32) -> Slot {
    let s = (cell[2] / size.0).min(cell[3] / size.1) * zoom;
    let (w, h) = (size.0 * s, size.1 * s);
    Slot {
        page,
        scale: PT_PER_PX * s,
        dx: cell[0] + (cell[2] - w) / 2.0,
        dy: cell[1] + (cell[3] - h) / 2.0,
        clip: cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{PageSelection, Paper};

    fn letter(n: usize) -> Vec<PageInfo> {
        (0..n)
            .map(|_| PageInfo {
                width: 816.0,
                height: 1056.0,
                ..PageInfo::default()
            })
            .collect()
    }

    #[test]
    fn one_page_per_sheet_keeps_the_page_size() {
        let sheets = impose(&letter(3), &PdfOptions::default()).unwrap();
        assert_eq!(sheets.len(), 3);
        assert!((sheets[0].width - 612.0).abs() < 0.01);
        assert!((sheets[0].height - 792.0).abs() < 0.01);
        assert_eq!(sheets[2].slots[0].page, 2);
        assert_eq!(sheets[0].slots[0].map(816.0, 1056.0), (612.0, 792.0));
    }

    #[test]
    fn fitting_centres_on_the_paper_and_copies_repeat() {
        let options = PdfOptions {
            scaling: Scaling::FitTo(Paper::A4),
            copies: 2,
            pages: PageSelection::parse("2").unwrap(),
            ..PdfOptions::default()
        };
        let sheets = impose(&letter(3), &options).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[1].copy, 1);
        let s = &sheets[0];
        assert!((s.width - Paper::A4.width).abs() < 0.01);
        let slot = s.slots[0];
        assert_eq!(slot.page, 1);
        // Letter is wider relative to its height than A4: it fills the width and is centred
        // vertically.
        let (x0, y0) = slot.map(0.0, 0.0);
        let (x1, y1) = slot.map(816.0, 1056.0);
        assert!(x0.abs() < 0.01 && (x1 - s.width).abs() < 0.01);
        assert!((y0 - (s.height - y1)).abs() < 0.01 && y0 > 0.0);
    }

    #[test]
    fn grids_turn_the_sheet_when_it_fits_better() {
        let options = PdfOptions {
            per_sheet: 2,
            ..PdfOptions::default()
        };
        let sheets = impose(&letter(3), &options).unwrap();
        assert_eq!(sheets.len(), 2);
        assert!(sheets[0].width > sheets[0].height, "two up is landscape");
        assert_eq!(sheets[0].slots.len(), 2);
        assert_eq!(sheets[1].slots.len(), 1);
        let a = sheets[0].slots[0];
        let b = sheets[0].slots[1];
        assert!(b.dx > a.dx && (a.dy - b.dy).abs() < 0.01);
        let options = PdfOptions {
            per_sheet: 4,
            ..PdfOptions::default()
        };
        let sheets = impose(&letter(4), &options).unwrap();
        assert!(sheets[0].width < sheets[0].height, "four up stays portrait");
        assert_eq!(sheets[0].slots[3].page, 3);
    }

    #[test]
    fn empty_selections_are_refused() {
        let options = PdfOptions {
            pages: PageSelection::parse("9-12").unwrap(),
            ..PdfOptions::default()
        };
        assert!(impose(&letter(3), &options).is_err());
    }
}
