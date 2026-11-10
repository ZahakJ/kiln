// SPDX-License-Identifier: AGPL-3.0-or-later
//! Page Layout: Margins, Orientation, Size, Print Area, Breaks, Print Titles, Scale to Fit,
//! Sheet Options (print gridlines and headings), the Page Setup dialog with headers and footers;
//! Page Break Preview (the printed pages over the sheet, breaks to drag); and printing the
//! sheet through `kiln-sheet-layout`'s print pages (File ▸ Print, Export ▸ PDF).

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_features::{
    GridBreakAction, GridMargins, GridPageInfo, GridPageSetup, GridPrintAreaAction,
};
use kiln_protocol::commands::{GridCommand, GridHit};
use kiln_scene::{Bounds, DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{Orientation, PrintSettings};
use kiln_sheet::{Command, Range, SheetId};
use kiln_sheet_layout::SheetGeometry;
use kiln_sheet_layout::print::hf::{Fields, sections};
use kiln_sheet_layout::print::pages::{PrintOptions, WorkbookPrint};
use kiln_sheet_layout::print::{SheetPages, name_ranges, paginate_with, setup};

use super::super::GridDoc;
use super::super::editor::quote_sheet_name;
use super::PagesCache;
use crate::model::Change;

/// Page Break Preview's line colour.
const BREAK_BLUE: u32 = 0x2F_5F_D8_FF;

fn ranges_text(sheet: &str, ranges: &[Range]) -> String {
    ranges
        .iter()
        .map(|r| format!("{}!{}", quote_sheet_name(sheet), a1_abs(r)))
        .collect::<Vec<_>>()
        .join(",")
}

/// `$A$1:$F$40`, `$1:$2` for whole rows, `$A:$B` for whole columns.
fn a1_abs(r: &Range) -> String {
    if r.is_whole_rows() && !r.is_whole_cols() {
        format!("${}:${}", r.r0 + 1, r.r1 + 1)
    } else if r.is_whole_cols() && !r.is_whole_rows() {
        let l = kiln_sheet_layout::col_letters;
        format!("${}:${}", l(r.c0), l(r.c1))
    } else {
        r.to_a1(true)
    }
}

/// Ranges typed in Page Setup (`$A$1:$F$40`, `A1:B2,D1:E5`, `$1:$2`, `'Sheet 1'!$A:$A`).
fn typed_ranges(text: &str) -> Result<Vec<Range>> {
    let t = text.trim().trim_start_matches('=');
    if t.is_empty() {
        return Ok(Vec::new());
    }
    let r = name_ranges(t);
    if r.is_empty() || t.split(',').count() != r.len() {
        return Err(KilnError::InvalidInput(format!(
            "\"{text}\" is not a valid reference"
        )));
    }
    Ok(r)
}

impl GridDoc {
    /// The pages of the active sheet (cached until the next edit).
    pub(crate) fn pages(&self) -> Option<SheetPages> {
        let (_, sheet) = self.active_sheet()?;
        let sid = sheet.id();
        if let Some(c) = self.features.pages.borrow().as_ref()
            && c.state == self.state
            && c.sheet == sid
        {
            return Some(c.pages.clone());
        }
        let geo = self.geometry_ref();
        let pages = paginate_with(sheet, &geo, setup(&self.workbook, sid));
        *self.features.pages.borrow_mut() = Some(PagesCache {
            state: self.state,
            sheet: sid,
            pages: pages.clone(),
        });
        Some(pages)
    }

    fn set_print(&mut self, label: &'static str, f: impl FnOnce(&mut PrintSettings)) -> Result<()> {
        let sid = self.sid()?;
        let mut p = self
            .workbook
            .sheet(sid)
            .map(|s| s.meta().print.clone())
            .unwrap_or_default();
        f(&mut p);
        self.exec(
            label,
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Print(p)),
            },
        )?;
        Ok(())
    }

    /// The command that sets (or, for no ranges, deletes) a reserved name of the sheet.
    fn reserved_name(&self, sid: SheetId, name: &str, ranges: &[Range]) -> Option<Command> {
        let sheet = self.workbook.sheet(sid)?.name().to_owned();
        let exists = self
            .workbook
            .names()
            .iter()
            .any(|n| n.scope == Some(sid) && n.name.eq_ignore_ascii_case(name));
        if ranges.is_empty() {
            return exists.then(|| Command::DeleteName {
                name: name.to_owned(),
                scope: Some(sid),
            });
        }
        Some(Command::DefineName {
            name: name.to_owned(),
            formula: ranges_text(&sheet, ranges),
            scope: Some(sid),
            hidden: false,
            comment: None,
        })
    }

    pub(crate) fn page_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        match cmd {
            GridCommand::PageSetup { setup: s } => self.page_setup(sid, s)?,
            GridCommand::PrintArea { action } => {
                let sel: Vec<Range> = self
                    .view()
                    .sel
                    .ranges
                    .iter()
                    .map(|r| self.clip_to_used(*r))
                    .collect();
                let current = setup(&self.workbook, sid).areas;
                let ranges = match action {
                    GridPrintAreaAction::Set => sel,
                    GridPrintAreaAction::Add => current.into_iter().chain(sel).collect(),
                    GridPrintAreaAction::Clear => Vec::new(),
                };
                if let Some(c) = self.reserved_name(sid, "_xlnm.Print_Area", &ranges) {
                    self.exec("grid.page_layout.set_print_area", c)?;
                }
            }
            GridCommand::PageBreak { action } => {
                let v = self.view();
                let a = v.sel.active;
                let whole_rows = v.sel.ranges.iter().all(Range::is_whole_rows);
                let whole_cols = v.sel.ranges.iter().all(Range::is_whole_cols);
                let action = *action;
                self.set_print("grid.page_layout.page_break", |p| match action {
                    GridBreakAction::Insert => {
                        if a.row > 0 && !whole_cols && !p.row_breaks.contains(&a.row) {
                            p.row_breaks.push(a.row);
                            p.row_breaks.sort_unstable();
                        }
                        if a.col > 0 && !whole_rows && !p.col_breaks.contains(&a.col) {
                            p.col_breaks.push(a.col);
                            p.col_breaks.sort_unstable();
                        }
                    }
                    GridBreakAction::Remove => {
                        p.row_breaks.retain(|&r| r != a.row);
                        p.col_breaks.retain(|&c| c != a.col);
                    }
                    GridBreakAction::Reset => {
                        p.row_breaks.clear();
                        p.col_breaks.clear();
                    }
                })?;
            }
            GridCommand::MovePageBreak { columns, from, to } => {
                let area = self
                    .pages()
                    .and_then(|p| p.areas.first().copied())
                    .unwrap_or(Range::cell(0, 0));
                let (lo, hi) = if *columns {
                    (area.c0, area.c1)
                } else {
                    (area.r0, area.r1)
                };
                let (from, to, columns) = (*from, *to, *columns);
                self.set_print("grid.page_layout.move_page_break", |p| {
                    let list = if columns {
                        &mut p.col_breaks
                    } else {
                        &mut p.row_breaks
                    };
                    list.retain(|&b| b != from);
                    if to > lo && to <= hi && !list.contains(&to) {
                        list.push(to);
                        list.sort_unstable();
                    }
                    // A break dragged past the fit makes the sheet print at 100 %.
                    p.fit_to_page = false;
                })?;
            }
            GridCommand::PrintGridlines => {
                self.set_print("grid.page_layout.print_gridlines", |p| {
                    p.print_gridlines = !p.print_gridlines;
                })?;
            }
            GridCommand::PrintHeadings => {
                self.set_print("grid.page_layout.print_headings", |p| {
                    p.print_headings = !p.print_headings;
                })?;
            }
            _ => return Ok(Change::None),
        }
        Ok(Change::Content)
    }

    fn page_setup(&mut self, sid: SheetId, s: &GridPageSetup) -> Result<()> {
        let mut p = self
            .workbook
            .sheet(sid)
            .map(|sh| sh.meta().print.clone())
            .unwrap_or_default();
        if let Some(l) = s.landscape {
            p.orientation = if l {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            };
        }
        if let Some(code) = s.paper {
            p.paper_size = code;
        }
        if let Some(m) = s.margins {
            for v in [m.left, m.right, m.top, m.bottom, m.header, m.footer] {
                if !(0.0..=20.0).contains(&v) {
                    return Err(KilnError::InvalidInput("margins are 0 to 20 inches".into()));
                }
            }
            p.margins.left = m.left;
            p.margins.right = m.right;
            p.margins.top = m.top;
            p.margins.bottom = m.bottom;
            p.margins.header = m.header;
            p.margins.footer = m.footer;
        }
        if let Some(sc) = s.scale {
            if !(10..=400).contains(&sc) {
                return Err(KilnError::InvalidInput("the scale is 10 % to 400 %".into()));
            }
            p.scale = sc;
            if s.fit.is_none() {
                p.fit_to_page = false;
            }
        }
        if let Some(w) = s.fit_width {
            p.fit_to_width = w;
            p.fit_to_page = true;
        }
        if let Some(h) = s.fit_height {
            p.fit_to_height = h;
            p.fit_to_page = true;
        }
        if p.fit_to_page && p.fit_to_width == 0 && p.fit_to_height == 0 {
            // Automatic both ways is no fitting at all.
            p.fit_to_page = false;
        }
        if let Some(f) = s.fit {
            p.fit_to_page = f;
        }
        if let Some(g) = s.gridlines {
            p.print_gridlines = g;
        }
        if let Some(h) = s.headings {
            p.print_headings = h;
        }
        if let Some(c) = s.center_h {
            p.horizontal_centered = c;
        }
        if let Some(c) = s.center_v {
            p.vertical_centered = c;
        }
        if let Some(h) = &s.header {
            p.header_footer.odd_header = (!h.is_empty()).then(|| h.clone());
        }
        if let Some(f) = &s.footer {
            p.header_footer.odd_footer = (!f.is_empty()).then(|| f.clone());
        }
        if let Some(fp) = s.first_page {
            p.first_page_number = (fp > 0).then_some(fp);
        }
        if let Some(o) = s.over_then_down {
            p.down_then_over = !o;
        }
        if let Some(b) = s.black_and_white {
            p.black_and_white = b;
        }
        let mut cmds = vec![Command::SetSheetMeta {
            sheet: sid,
            field: Box::new(MetaField::Print(p)),
        }];
        if let Some(a) = &s.print_area {
            let ranges = typed_ranges(a)?;
            cmds.extend(self.reserved_name(sid, "_xlnm.Print_Area", &ranges));
        }
        if s.title_rows.is_some() || s.title_cols.is_some() {
            let cur = setup(&self.workbook, sid);
            let rows = match &s.title_rows {
                Some(t) => {
                    let r = typed_ranges(t)?;
                    r.first().map(|r| (r.r0, r.r1))
                }
                None => cur.title_rows,
            };
            let cols = match &s.title_cols {
                Some(t) => {
                    let r = typed_ranges(t)?;
                    r.first().map(|r| (r.c0, r.c1))
                }
                None => cur.title_cols,
            };
            let mut ranges = Vec::new();
            if let Some((a, b)) = cols {
                ranges.push(Range::cols(a, b));
            }
            if let Some((a, b)) = rows {
                ranges.push(Range::rows(a, b));
            }
            cmds.extend(self.reserved_name(sid, "_xlnm.Print_Titles", &ranges));
        }
        self.exec_all("grid.page_layout.page_setup", cmds)?;
        Ok(())
    }

    /// The Page Setup dialog's values.
    pub(crate) fn page_info(&self) -> Result<GridPageInfo> {
        let sid = self.sid()?;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let st = setup(&self.workbook, sid);
        let p = &st.print;
        let pages = self.pages();
        let n = pages.as_ref().map_or(0, |p| p.pages.len());
        let date = super::super::w3cdtf(self.now_ms);
        let date = format!(
            "{}/{}/{}",
            date[5..7].trim_start_matches('0'),
            date[8..10].trim_start_matches('0'),
            &date[0..4]
        );
        let fields = Fields {
            page: p.first_page_number.unwrap_or(1),
            pages: u32::try_from(n).unwrap_or(0),
            date: &date,
            time: "",
            file: self
                .file_name
                .as_deref()
                .map_or("Book1", |f| f.rsplit(['/', '\\']).next().unwrap_or(f)),
            path: "",
            sheet: sheet.name(),
        };
        Ok(GridPageInfo {
            landscape: p.orientation == Orientation::Landscape,
            paper: p.paper_size,
            margins: GridMargins {
                left: p.margins.left,
                right: p.margins.right,
                top: p.margins.top,
                bottom: p.margins.bottom,
                header: p.margins.header,
                footer: p.margins.footer,
            },
            scale: p.scale,
            fit: p.fit_to_page,
            fit_width: p.fit_to_width,
            fit_height: p.fit_to_height,
            gridlines: p.print_gridlines,
            headings: p.print_headings,
            center_h: p.horizontal_centered,
            center_v: p.vertical_centered,
            header: p.header_footer.odd_header.clone().unwrap_or_default(),
            footer: p.header_footer.odd_footer.clone().unwrap_or_default(),
            first_page: p.first_page_number,
            over_then_down: !p.down_then_over,
            black_and_white: p.black_and_white,
            print_area: st.areas.iter().map(a1_abs).collect::<Vec<_>>().join(","),
            title_rows: st
                .title_rows
                .map(|(a, b)| a1_abs(&Range::rows(a, b)))
                .unwrap_or_default(),
            title_cols: st
                .title_cols
                .map(|(a, b)| a1_abs(&Range::cols(a, b)))
                .unwrap_or_default(),
            pages: u32::try_from(n).unwrap_or(0),
            row_breaks: p.row_breaks.clone(),
            col_breaks: p.col_breaks.clone(),
            header_preview: sections(p.header_footer.odd_header.as_deref().unwrap_or(""), &fields),
            footer_preview: sections(p.header_footer.odd_footer.as_deref().unwrap_or(""), &fields),
        })
    }

    /// Page Break Preview: blue page borders (dashed for automatic breaks) and the cells outside
    /// the print area greyed.
    pub(crate) fn page_break_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Some(p) = self.pages() else {
            return;
        };
        let manual_rows = &p.setup.print.row_breaks;
        let manual_cols = &p.setup.print.col_breaks;
        let view = self.last_view;
        let z = view.zoom.max(0.01);
        let visible = Bounds::new(
            view.scroll_x,
            view.scroll_y,
            view.width / z + 400.0,
            view.height / z + 400.0,
        );
        // Grey outside the areas.
        let mut outside = vec![visible];
        for a in &p.areas {
            let rect = geo.range_rect(a);
            outside = outside.into_iter().flat_map(|o| minus(o, rect)).collect();
        }
        for o in outside {
            out.push(DrawOp::FillRect {
                rect: o,
                paint: Paint::Rgba(0x80_80_80_40),
                radius: 0.0,
            });
        }
        for a in &p.areas {
            let rect = geo.range_rect(a);
            let line = |out: &mut Vec<DrawOp>, x0: f32, y0: f32, x1: f32, y1: f32, dashed: bool| {
                let mut path = PathData::new();
                path.move_to(x0, y0).line_to(x1, y1);
                out.push(DrawOp::Path {
                    path,
                    fill: None,
                    stroke: Some(Stroke {
                        dash: if dashed { vec![6.0, 4.0] } else { Vec::new() },
                        ..Stroke::solid(Paint::Rgba(BREAK_BLUE), 2.5)
                    }),
                    rule: FillRule::NonZero,
                });
            };
            out.push(DrawOp::StrokeRect {
                rect: rect.inflate(-1.0, -1.0),
                paint: Paint::Rgba(BREAK_BLUE),
                width: 3.0,
                radius: 0.0,
            });
            for &r in p.row_breaks.iter().filter(|&&r| r > a.r0 && r <= a.r1) {
                let y = geo.rows.offset(r) as f32;
                line(out, rect.x, y, rect.right(), y, !manual_rows.contains(&r));
            }
            for &c in p.col_breaks.iter().filter(|&&c| c > a.c0 && c <= a.c1) {
                let x = geo.cols.offset(c) as f32;
                line(out, x, rect.y, x, rect.bottom(), !manual_cols.contains(&c));
            }
        }
    }

    pub(crate) fn page_break_hit(&self, x: f32, y: f32) -> Option<GridHit> {
        if self.features.view_mode != Some(kiln_protocol::ViewMode::PageBreakPreview) {
            return None;
        }
        let p = self.pages()?;
        let geo = self.geo.as_ref()?;
        let hw = self.header_w(geo);
        let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
        let tol = 4.0 / self.last_view.zoom.max(0.1);
        for &r in &p.row_breaks {
            if (geo.rows.offset(r) as f32 - sy).abs() <= tol {
                return Some(GridHit::PageBreak {
                    columns: false,
                    index: r,
                });
            }
        }
        for &c in &p.col_breaks {
            if (geo.cols.offset(c) as f32 - sx).abs() <= tol {
                return Some(GridHit::PageBreak {
                    columns: true,
                    index: c,
                });
            }
        }
        None
    }

    /// File ▸ Print and Export ▸ PDF of the active sheet.
    pub(crate) fn print_job(
        &self,
        job: &crate::print::PrintJob<'_>,
    ) -> Result<crate::print::PrintOutput> {
        let settings = job.settings();
        let file = self.file_name.as_deref().map_or_else(
            || "Book1".to_owned(),
            |f| f.rsplit(['/', '\\']).next().unwrap_or(f).to_owned(),
        );
        let title = match job {
            crate::print::PrintJob::Pdf { title, .. } if !title.is_empty() => (*title).to_owned(),
            _ => file
                .rsplit_once('.')
                .map_or(file.clone(), |(stem, _)| stem.to_owned()),
        };
        let now = super::super::w3cdtf(self.now_ms);
        let opts = PrintOptions {
            sheets: vec![self.active],
            selection: None,
            title,
            file,
            path: String::new(),
            date: format!(
                "{}/{}/{}",
                now[5..7].trim_start_matches('0'),
                now[8..10].trim_start_matches('0'),
                &now[0..4]
            ),
            time: now[11..16].to_owned(),
            landscape: settings
                .orientation
                .map(|o| o == kiln_protocol::print::PrintOrientation::Landscape),
            margins: settings.margins.map(|m| match m {
                kiln_protocol::print::PrintMargins::Normal => [0.75, 0.7, 0.75, 0.7],
                kiln_protocol::print::PrintMargins::Narrow => [0.75, 0.25, 0.75, 0.25],
                kiln_protocol::print::PrintMargins::Moderate => [1.0, 0.75, 1.0, 0.75],
                kiln_protocol::print::PrintMargins::Wide => [1.0, 1.0, 1.0, 1.0],
            }),
        };
        let fonts = crate::apps::scratch::fonts::store();
        let cf = self
            .features
            .print_cf
            .borrow()
            .as_ref()
            .filter(|(state, sheet, _)| *state == self.state && *sheet == self.active)
            .map(|(_, _, cf)| cf.clone())
            .unwrap_or_default();
        let chart = |p: &str| self.charts.part(p);
        let drawings = kiln_sheet_layout::print::pages::PrintDrawings {
            chart: Some(&chart),
            images: self.print_images(),
            theme: self.charts.theme(),
        };
        let print = WorkbookPrint::with_cf(&self.workbook, fonts, opts, cf).with_drawings(drawings);
        crate::print::run(job, &print)
    }

    /// Evaluates the conditional formats of the active sheet's printed cells (before a preview
    /// or PDF; the print itself reads the workbook only).
    pub(crate) fn prepare_print(&mut self) {
        let fresh = self
            .features
            .print_cf
            .borrow()
            .as_ref()
            .is_some_and(|(state, sheet, _)| *state == self.state && *sheet == self.active);
        if fresh {
            return;
        }
        let opts = PrintOptions {
            sheets: vec![self.active],
            ..PrintOptions::default()
        };
        let cf = self.workbook.print_conditional_formats(&opts);
        *self.features.print_cf.borrow_mut() = Some((self.state, self.active, cf));
    }
}

/// `r` without `hole` (up to four rectangles).
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
