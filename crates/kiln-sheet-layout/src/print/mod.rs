// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sheet printing: a sheet's page setup resolved into pages, and the pages painted for
//! `kiln-io-pdf` with the screen's cell painter, so print, PDF and the screen agree.
//!
//! - [`setup`]: paper, margins, scale, print area and print titles from the sheet's
//!   `PrintSettings` and the `_xlnm.Print_Area` / `_xlnm.Print_Titles` names.
//! - [`paginate`]: which rows and columns go on each page — fit to N pages wide and tall, manual
//!   breaks (kept along an axis whose fit is automatic, as Excel and LibreOffice do), hidden
//!   rows and columns, print titles, down-then-over or over-then-down order. Page Break Preview
//!   shows the same pages.
//! - [`hf`]: header and footer codes (`&L`, `&C`, `&R`, `&P`, `&N`, `&D`, `&T`, `&F`, `&A`,
//!   `&Z`, `&&`; font codes are read and dropped).
//! - [`pages`]: [`pages::WorkbookPrint`], the `PrintablePages` of one or more sheets.

pub mod hf;
pub mod pages;

use kiln_sheet::meta::{Orientation, PrintSettings};
use kiln_sheet::{Range, Sheet, SheetId, Workbook};

use crate::geom::SheetGeometry;

/// CSS (and page) pixels per inch.
pub const PX_PER_IN: f64 = 96.0;

/// Paper size in inches for a SpreadsheetML paper code (Letter for unknown codes).
#[must_use]
pub fn paper_inches(code: u32) -> (f64, f64) {
    const MM: f64 = 1.0 / 25.4;
    match code {
        3 => (11.0, 17.0),
        4 => (17.0, 11.0),
        5 => (8.5, 14.0),
        7 => (7.25, 10.5),
        8 => (297.0 * MM, 420.0 * MM),
        9 | 10 => (210.0 * MM, 297.0 * MM),
        11 => (148.0 * MM, 210.0 * MM),
        12 => (250.0 * MM, 354.0 * MM),
        13 => (182.0 * MM, 257.0 * MM),
        _ => (8.5, 11.0),
    }
}

/// The name of a paper code, for the ribbon and the dialog.
#[must_use]
pub fn paper_name(code: u32) -> &'static str {
    match code {
        3 => "Tabloid",
        5 => "Legal",
        7 => "Executive",
        8 => "A3",
        9 => "A4",
        11 => "A5",
        13 => "B5",
        _ => "Letter",
    }
}

/// A sheet's page setup with its names resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    /// The settings stored on the sheet.
    pub print: PrintSettings,
    /// Print areas (each prints on its own pages); empty = the used range.
    pub areas: Vec<Range>,
    /// Rows repeated at the top of each page.
    pub title_rows: Option<(u32, u32)>,
    /// Columns repeated at the left of each page.
    pub title_cols: Option<(u32, u32)>,
}

/// The ranges of a name's formula on `sheet` (`'Print'!$A$1:$L$120,Print!$N$1:$O$5`).
#[must_use]
pub fn name_ranges(formula: &str) -> Vec<Range> {
    split_top(formula.trim_start_matches('='))
        .into_iter()
        .filter_map(|part| {
            let part = part.trim();
            let addr = match part.rfind('!') {
                Some(i) => &part[i + 1..],
                None => part,
            };
            Range::parse_a1(addr)
        })
        .collect()
}

/// Splits on commas outside quotes.
fn split_top(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ',' if !quoted => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// The page setup of a sheet.
#[must_use]
pub fn setup(wb: &Workbook, sheet: SheetId) -> PageSetup {
    let print = wb
        .sheet(sheet)
        .map(|s| s.meta().print.clone())
        .unwrap_or_default();
    let name = |n: &str| {
        wb.names()
            .iter()
            .find(|d| d.scope == Some(sheet) && d.name.eq_ignore_ascii_case(n))
            .map(|d| d.formula.clone())
    };
    let areas = name("_xlnm.Print_Area")
        .map(|f| name_ranges(&f))
        .unwrap_or_default();
    let mut title_rows = None;
    let mut title_cols = None;
    if let Some(f) = name("_xlnm.Print_Titles") {
        for r in name_ranges(&f) {
            if r.is_whole_rows() {
                title_rows = Some((r.r0, r.r1));
            } else if r.is_whole_cols() {
                title_cols = Some((r.c0, r.c1));
            }
        }
    }
    PageSetup {
        print,
        areas,
        title_rows,
        title_cols,
    }
}

/// One printed page of a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageArea {
    /// Rows (inclusive) of the print area on this page.
    pub rows: (u32, u32),
    /// Columns.
    pub cols: (u32, u32),
    /// Title rows printed above them.
    pub title_rows: Option<(u32, u32)>,
    /// Title columns printed left of them.
    pub title_cols: Option<(u32, u32)>,
}

/// A sheet split into pages.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetPages {
    /// Paper width, page px.
    pub width: f32,
    /// Paper height, page px.
    pub height: f32,
    /// Left, right, top, bottom, header and footer margins, page px.
    pub margins: [f32; 6],
    /// Sheet px → page px.
    pub scale: f32,
    /// Pages in print order.
    pub pages: Vec<PageArea>,
    /// Every row starting a page (automatic and manual), in each area, ascending.
    pub row_breaks: Vec<u32>,
    /// Every column starting a page.
    pub col_breaks: Vec<u32>,
    /// The areas printed.
    pub areas: Vec<Range>,
    /// The resolved setup.
    pub setup: PageSetup,
}

impl SheetPages {
    /// The width the cells may use on a page, page px.
    #[must_use]
    pub fn body_width(&self) -> f32 {
        (self.width - self.margins[0] - self.margins[1]).max(1.0)
    }

    /// The height the cells may use on a page, page px.
    #[must_use]
    pub fn body_height(&self) -> f32 {
        (self.height - self.margins[2] - self.margins[3]).max(1.0)
    }
}

/// Where a printed page puts its cells (page px): the origin of the headings and of the cells,
/// and the size of the print titles (sheet px). [`pages::WorkbookPrint`] paints with it and
/// Page Layout view finds cells on the page with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    /// Left of the printed block (row headings when printed), page px.
    pub x0: f32,
    /// Top of the printed block (column headings when printed), page px.
    pub y0: f32,
    /// Left of the first column of cells, page px.
    pub cx0: f32,
    /// Top of the first row of cells, page px.
    pub cy0: f32,
    /// Width of the title columns, sheet px.
    pub tw: f32,
    /// Height of the title rows, sheet px.
    pub th: f32,
}

/// The [`PageFrame`] of one page.
#[must_use]
pub fn page_frame(p: &SheetPages, geo: &SheetGeometry, page: &PageArea) -> PageFrame {
    let s = p.scale;
    let span_rows =
        |a: u32, b: u32| -> f32 { (geo.rows.offset(b + 1) - geo.rows.offset(a)) as f32 };
    let span_cols =
        |a: u32, b: u32| -> f32 { (geo.cols.offset(b + 1) - geo.cols.offset(a)) as f32 };
    let headings = p.setup.print.print_headings;
    let th = page.title_rows.map_or(0.0, |(a, b)| span_rows(a, b));
    let tw = page.title_cols.map_or(0.0, |(a, b)| span_cols(a, b));
    let bh = span_rows(page.rows.0, page.rows.1);
    let bw = span_cols(page.cols.0, page.cols.1);
    let hh = if headings { HEADING_H } else { 0.0 };
    let hw = if headings { HEADING_W } else { 0.0 };
    let total_w = (hw + tw + bw) * s;
    let total_h = (hh + th + bh) * s;
    let mut x0 = p.margins[0];
    let mut y0 = p.margins[2];
    if p.setup.print.horizontal_centered {
        x0 += ((p.body_width() - total_w) / 2.0).max(0.0);
    }
    if p.setup.print.vertical_centered {
        y0 += ((p.body_height() - total_h) / 2.0).max(0.0);
    }
    PageFrame {
        x0,
        y0,
        cx0: x0 + hw * s,
        cy0: y0 + hh * s,
        tw,
        th,
    }
}

/// A block of cells printed on a page: its origin (page px) and its rows and columns as
/// `(index, offset, size)` in page px from that origin. Hidden rows and columns are left out.
#[derive(Debug, Clone, PartialEq)]
pub struct PageBand {
    /// Left, page px.
    pub x: f32,
    /// Top, page px.
    pub y: f32,
    /// Rows.
    pub rows: Vec<(u32, f32, f32)>,
    /// Columns.
    pub cols: Vec<(u32, f32, f32)>,
}

/// The cells of page `n` (0-based) as printed: the title corner, the title rows, the title
/// columns and, last, the page's own cells. With `fill`, a page that ends its print area goes on
/// with the empty rows and columns that would fit on it (Page Layout view, like Excel's, lets
/// cells past the data be typed into).
#[must_use]
pub fn page_bands(
    p: &SheetPages,
    geo: &SheetGeometry,
    n: usize,
    fill: bool,
) -> Option<Vec<PageBand>> {
    let page = p.pages.get(n)?;
    let f = page_frame(p, geo, page);
    let s = p.scale;
    let lines_to = |rows: bool, a: u32, b: u32, room: f32| -> Vec<(u32, f32, f32)> {
        let axis = if rows { &geo.rows } else { &geo.cols };
        let max = if rows {
            kiln_sheet::MAX_ROWS
        } else {
            kiln_sheet::MAX_COLS
        };
        let mut at = 0.0;
        let mut out = Vec::new();
        let mut i = a;
        while i < max && (i <= b || (at + axis.span(i).1 as f32) * s <= room) {
            let size = if axis.is_hidden(i) {
                0.0
            } else {
                axis.span(i).1 as f32
            };
            if size > 0.0 {
                out.push((i, at * s, size * s));
            }
            at += size;
            i += 1;
        }
        out
    };
    let lines = |rows: bool, a: u32, b: u32| lines_to(rows, a, b, 0.0);
    // The print area this page belongs to: does the page end it down and across?
    let area = p
        .areas
        .iter()
        .find(|r| r.contains(page.rows.0, page.cols.0))
        .copied();
    let (room_h, room_w) = match area {
        Some(a) if fill => (
            if page.rows.1 >= a.r1 {
                p.height - p.margins[3] - (f.cy0 + f.th * s)
            } else {
                0.0
            },
            if page.cols.1 >= a.c1 {
                p.width - p.margins[1] - (f.cx0 + f.tw * s)
            } else {
                0.0
            },
        ),
        _ => (0.0, 0.0),
    };
    let mut out = Vec::new();
    if let (Some((ra, rb)), Some((ca, cb))) = (page.title_rows, page.title_cols) {
        out.push(PageBand {
            x: f.cx0,
            y: f.cy0,
            rows: lines(true, ra, rb),
            cols: lines(false, ca, cb),
        });
    }
    if let Some((ra, rb)) = page.title_rows {
        out.push(PageBand {
            x: f.cx0 + f.tw * s,
            y: f.cy0,
            rows: lines(true, ra, rb),
            cols: lines(false, page.cols.0, page.cols.1),
        });
    }
    if let Some((ca, cb)) = page.title_cols {
        out.push(PageBand {
            x: f.cx0,
            y: f.cy0 + f.th * s,
            rows: lines(true, page.rows.0, page.rows.1),
            cols: lines(false, ca, cb),
        });
    }
    out.push(PageBand {
        x: f.cx0 + f.tw * s,
        y: f.cy0 + f.th * s,
        rows: lines_to(true, page.rows.0, page.rows.1, room_h),
        cols: lines_to(false, page.cols.0, page.cols.1, room_w),
    });
    Some(out)
}

/// Header band size of printed headings, sheet px.
pub const HEADING_H: f32 = 20.0;
/// Row heading width, sheet px.
pub const HEADING_W: f32 = 36.0;

/// What a sheet prints when it has no print area: its used range (values and formats), merges
/// and drawings included.
#[must_use]
pub fn default_area(sheet: &Sheet) -> Option<Range> {
    let mut r = sheet.cells().used_range(false);
    if let Some(mut u) = r {
        for m in sheet.merges() {
            if m.intersects(&u) || m.r0 <= u.r1 + 1 {
                u = u.union(m);
            }
        }
        r = Some(u);
    }
    // Charts, pictures and shapes print too: the range reaches the cell under each one's
    // bottom-right corner.
    let shown: Vec<_> = sheet.meta().drawings.iter().filter(|d| !d.hidden).collect();
    if !shown.is_empty() {
        let geo = crate::render::geometry_of(sheet, false);
        for d in shown {
            let rect = crate::drawings::anchor_rect(&geo, &d.anchor);
            let r0 = geo.rows.index_at(f64::from(rect.y));
            let c0 = geo.cols.index_at(f64::from(rect.x));
            let r1 = geo.rows.index_at(f64::from(rect.bottom() - 0.5).max(0.0));
            let c1 = geo.cols.index_at(f64::from(rect.right() - 0.5).max(0.0));
            let dr = Range::new(r0.min(r1), c0.min(c1), r1.max(r0), c1.max(c0));
            r = Some(match r {
                Some(u) => u.union(&dr),
                None => dr,
            });
        }
    }
    r
}

/// Splits one axis of `len` items into pages of `avail` px (sizes scaled by `scale`), starting a
/// page at every `manual` break, `first`..=`last`. A page after the print titles (`titles`: their
/// last index and scaled size) repeats them, so it holds that much less (print-setup.xlsx
/// paginates as in LibreOffice: 1–31, 32–60 at the manual break, 61–90 and 91–120 under the
/// title row).
fn split_axis(
    size: &dyn Fn(u32) -> f64,
    first: u32,
    last: u32,
    avail: f64,
    scale: f64,
    manual: &[u32],
    titles: Option<(u32, f64)>,
) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut start = first;
    let mut used = 0.0;
    let mut i = first;
    let room = |start: u32| match titles {
        Some((t, h)) if start > t => (avail - h).max(1.0),
        _ => avail,
    };
    while i <= last {
        let s = size(i) * scale;
        let forced = i > start && manual.binary_search(&i).is_ok();
        if i > start && (forced || used + s > room(start) + 0.01) {
            out.push((start, i - 1));
            start = i;
            used = 0.0;
            continue;
        }
        used += s;
        if i == u32::MAX {
            break;
        }
        i += 1;
    }
    out.push((start, last));
    out
}

/// Sizes in sheet px along one axis of a geometry.
fn axis_size(geo: &SheetGeometry, rows: bool) -> impl Fn(u32) -> f64 + '_ {
    move |i| {
        let a = if rows { &geo.rows } else { &geo.cols };
        if a.is_hidden(i) { 0.0 } else { a.span(i).1 }
    }
}

/// The pages of a sheet with its page setup.
#[must_use]
pub fn paginate(wb: &Workbook, sheet_index: usize) -> Option<SheetPages> {
    let sheet = wb.sheets().get(sheet_index)?;
    let setup = setup(wb, sheet.id());
    let geo = crate::render::geometry_of(sheet, false);
    Some(paginate_with(sheet, &geo, setup))
}

/// [`paginate`] with a geometry already built and a setup.
#[must_use]
pub fn paginate_with(sheet: &Sheet, geo: &SheetGeometry, setup: PageSetup) -> SheetPages {
    let p = &setup.print;
    let (mut pw, mut ph) = paper_inches(p.paper_size);
    if p.orientation == Orientation::Landscape {
        std::mem::swap(&mut pw, &mut ph);
    }
    let m = &p.margins;
    let px = |inches: f64| (inches.max(0.0) * PX_PER_IN) as f32;
    let margins = [
        px(m.left),
        px(m.right),
        px(m.top),
        px(m.bottom),
        px(m.header),
        px(m.footer),
    ];
    let width = px(pw);
    let height = px(ph);
    let body_w = f64::from((width - margins[0] - margins[1]).max(1.0));
    // A header or footer keeps a gap of 0.1 in from the rows (LibreOffice's spacing: with them,
    // print-setup.xlsx holds 31 rows a page, without them 32).
    let hf = &p.header_footer;
    let some = |t: &Option<String>| t.as_deref().is_some_and(|t| !t.trim().is_empty());
    let gap = |on: bool| if on { 0.1 * PX_PER_IN } else { 0.0 };
    let header = some(&hf.odd_header) || some(&hf.even_header) || some(&hf.first_header);
    let footer = some(&hf.odd_footer) || some(&hf.even_footer) || some(&hf.first_footer);
    let body_h = (f64::from(height - margins[2] - margins[3]) - gap(header) - gap(footer)).max(1.0);
    let areas: Vec<Range> = if setup.areas.is_empty() {
        default_area(sheet).into_iter().collect()
    } else {
        setup.areas.clone()
    };
    let rsize = axis_size(geo, true);
    let csize = axis_size(geo, false);
    let span = |f: &dyn Fn(u32) -> f64, a: u32, b: u32| -> f64 { (a..=b).map(f).sum() };
    let heading = |b: bool, v: f32| if b { f64::from(v) } else { 0.0 };
    let heading_h = heading(p.print_headings, HEADING_H);
    let heading_w = heading(p.print_headings, HEADING_W);
    // The print titles: the first page holds them as its own rows, the others repeat them.
    let titles_h = setup.title_rows.map_or(0.0, |(a, b)| span(&rsize, a, b));
    let titles_w = setup.title_cols.map_or(0.0, |(a, b)| span(&csize, a, b));
    let row_titles = |scale: f64| setup.title_rows.map(|(_, b)| (b, titles_h * scale));
    let col_titles = |scale: f64| setup.title_cols.map(|(_, b)| (b, titles_w * scale));
    // Manual breaks count along an axis whose fit is automatic (or without fit to page).
    let row_manual: Vec<u32> = if p.fit_to_page && p.fit_to_height > 0 {
        Vec::new()
    } else {
        let mut v = p.row_breaks.clone();
        v.sort_unstable();
        v
    };
    let col_manual: Vec<u32> = if p.fit_to_page && p.fit_to_width > 0 {
        Vec::new()
    } else {
        let mut v = p.col_breaks.clone();
        v.sort_unstable();
        v
    };
    let count = |rows: bool, scale: f64| -> usize {
        areas
            .iter()
            .map(|a| {
                if rows {
                    split_axis(
                        &rsize,
                        a.r0,
                        a.r1,
                        (body_h - heading_h * scale).max(1.0),
                        scale,
                        &row_manual,
                        row_titles(scale),
                    )
                    .len()
                } else {
                    split_axis(
                        &csize,
                        a.c0,
                        a.c1,
                        (body_w - heading_w * scale).max(1.0),
                        scale,
                        &col_manual,
                        col_titles(scale),
                    )
                    .len()
                }
            })
            .max()
            .unwrap_or(1)
    };
    let scale = if p.fit_to_page {
        // The largest scale (at most 100 %, at least 10 %) whose page counts fit.
        let fits = |s: f64| {
            (p.fit_to_width == 0 || count(false, s) <= p.fit_to_width as usize)
                && (p.fit_to_height == 0 || count(true, s) <= p.fit_to_height as usize)
        };
        if fits(1.0) {
            1.0
        } else {
            let (mut lo, mut hi) = (0.1_f64, 1.0_f64);
            for _ in 0..24 {
                let mid = (lo + hi) / 2.0;
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            // Excel's scale is a whole percentage.
            ((lo * 100.0).floor() / 100.0).max(0.1)
        }
    } else {
        f64::from(p.scale.clamp(10, 400)) / 100.0
    };
    let mut pages = Vec::new();
    let mut row_breaks = Vec::new();
    let mut col_breaks = Vec::new();
    for a in &areas {
        let rows = split_axis(
            &rsize,
            a.r0,
            a.r1,
            (body_h - heading_h * scale).max(1.0),
            scale,
            &row_manual,
            row_titles(scale),
        );
        let cols = split_axis(
            &csize,
            a.c0,
            a.c1,
            (body_w - heading_w * scale).max(1.0),
            scale,
            &col_manual,
            col_titles(scale),
        );
        row_breaks.extend(rows.iter().skip(1).map(|r| r.0));
        col_breaks.extend(cols.iter().skip(1).map(|c| c.0));
        let titled_rows = |r: (u32, u32)| setup.title_rows.filter(|t| r.0 > t.1);
        let titled_cols = |c: (u32, u32)| setup.title_cols.filter(|t| c.0 > t.1);
        if p.down_then_over {
            for c in &cols {
                for r in &rows {
                    pages.push(PageArea {
                        rows: *r,
                        cols: *c,
                        title_rows: titled_rows(*r),
                        title_cols: titled_cols(*c),
                    });
                }
            }
        } else {
            for r in &rows {
                for c in &cols {
                    pages.push(PageArea {
                        rows: *r,
                        cols: *c,
                        title_rows: titled_rows(*r),
                        title_cols: titled_cols(*c),
                    });
                }
            }
        }
    }
    row_breaks.sort_unstable();
    row_breaks.dedup();
    col_breaks.sort_unstable();
    col_breaks.dedup();
    SheetPages {
        width,
        height,
        margins,
        scale: scale as f32,
        pages,
        row_breaks,
        col_breaks,
        areas,
        setup,
    }
}

#[cfg(test)]
mod tests;
