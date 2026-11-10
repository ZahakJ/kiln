// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Home tab's completions: Paste Special's comments, validation and column widths,
//! Paste Link, Home ▸ Fill ▸ Series, the Auto Fill Options button, custom fill lists, the
//! border pen (Draw Border, Draw Border Grid, Erase Border), deleting a custom number format,
//! and rich text runs typed inside the cell editor.

use kiln_common::{KilnError, Result};
use kiln_formula::refs::copy_formula;
use kiln_protocol::commands::grid_home::{
    GridAutoFillChoice, GridAutoFillOptions, GridDateUnit, GridHomeState, GridPen, GridPenMode,
    GridSeries, GridSeriesType,
};
use kiln_protocol::commands::{
    ClipboardData, EditCommand, GridBorders, GridCommand, GridFill, GridLineStyle, GridRange,
};
use kiln_sheet::clipboard::ClipSource;
use kiln_sheet::color::Color;
use kiln_sheet::commands::{CellInput, ColPatch, Content, MetaField};
use kiln_sheet::fill::{DateUnit, SeriesSpec, SeriesType};
use kiln_sheet::num::F64;
use kiln_sheet::styles::{
    BorderPatch, BorderStyle, Font, FontPatch, Patch, StylePatch, Underline, VertAlign,
};
use kiln_sheet::value::{RichText, TextRun};
use kiln_sheet::{CellPos, CellValue, MAX_COLS, MAX_ROWS, Range, SheetId};

use super::GridDoc;
use super::clipboard::Extras;
use super::view::Sel;
use crate::model::{Change, DocModel};

/// The last fill-handle fill, which the Auto Fill Options button redoes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LastFill {
    sheet: SheetId,
    source: Range,
    target: Range,
    /// The document state right after the fill: the button lasts while nothing else changed.
    state: u64,
    choice: GridAutoFillChoice,
    dates: bool,
}

/// View state of the Home completions.
#[derive(Debug, Clone)]
pub(crate) struct HomeState {
    pub(crate) pen: Option<GridPen>,
    /// The pen's line style and colour, kept between uses.
    pub(crate) pen_style: GridLineStyle,
    pub(crate) pen_color: Option<String>,
    pub(crate) pen_preview: Option<Range>,
    pub(crate) last_fill: Option<LastFill>,
    /// Number formats deleted from the Custom list this session.
    pub(crate) deleted_codes: Vec<String>,
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState {
            pen: None,
            pen_style: GridLineStyle::Thin,
            pen_color: None,
            pen_preview: None,
            last_fill: None,
            deleted_codes: Vec::new(),
        }
    }
}

/// The rich text of an entry being typed: one format id per byte of the text (0 = the cell's
/// font, `n` = `fonts[n - 1]`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichEdit {
    ids: Vec<u16>,
    fonts: Vec<FontPatch>,
}

impl RichEdit {
    /// From a rich-text value.
    #[must_use]
    pub fn from_value(r: &RichText) -> RichEdit {
        let mut e = RichEdit::default();
        for run in r.runs() {
            let id = run.font.as_ref().map_or(0, |f| e.intern(f.clone()));
            e.ids.extend(std::iter::repeat_n(id, run.text.len()));
        }
        e
    }

    fn intern(&mut self, f: FontPatch) -> u16 {
        if f == FontPatch::default() {
            return 0;
        }
        if let Some(i) = self.fonts.iter().position(|x| *x == f) {
            return (i + 1) as u16;
        }
        self.fonts.push(f);
        u16::try_from(self.fonts.len()).unwrap_or(u16::MAX)
    }

    /// Whether any byte has its own format.
    #[must_use]
    pub fn is_rich(&self) -> bool {
        self.ids.iter().any(|&i| i != 0)
    }

    /// Whether the runs describe `text` (an edit the runs did not follow drops them).
    #[must_use]
    pub fn fits(&self, text: &str) -> bool {
        self.ids.len() == text.len()
    }

    fn patch_at(&self, byte: usize) -> FontPatch {
        match self.ids.get(byte).copied().unwrap_or(0) {
            0 => FontPatch::default(),
            i => self.fonts[usize::from(i) - 1].clone(),
        }
    }

    /// Follows an edit of the text from `old` to `new`: the changed middle is replaced, and
    /// typed characters take the format of the character before them (Excel).
    pub fn follow(&mut self, old: &str, new: &str) {
        if self.ids.len() != old.len() {
            self.ids.resize(old.len(), 0);
        }
        let prefix = old
            .char_indices()
            .zip(new.char_indices())
            .take_while(|((_, a), (_, b))| a == b)
            .last()
            .map_or(0, |((i, c), _)| i + c.len_utf8());
        let max_suffix = old.len().min(new.len()) - prefix;
        let mut suffix = 0;
        for (a, b) in old.chars().rev().zip(new.chars().rev()) {
            if a != b || suffix + a.len_utf8() > max_suffix {
                break;
            }
            suffix += a.len_utf8();
        }
        let inherit = if prefix > 0 {
            self.ids[prefix - 1]
        } else {
            self.ids.first().copied().unwrap_or(0)
        };
        let inserted = new.len() - prefix - suffix;
        self.ids.splice(
            prefix..old.len() - suffix,
            std::iter::repeat_n(inherit, inserted),
        );
    }

    /// Applies `patch` to the bytes `a..b`.
    fn apply(&mut self, a: usize, b: usize, patch: &FontPatch) {
        let b = b.min(self.ids.len());
        for i in a..b {
            let merged = merge(&self.patch_at(i), patch);
            self.ids[i] = self.intern(merged);
        }
    }

    /// The runs of `text` (same length as the ids).
    #[must_use]
    pub fn runs(&self, text: &str) -> Vec<TextRun> {
        let mut out: Vec<TextRun> = Vec::new();
        let mut start = 0;
        let mut cur = self.ids.first().copied().unwrap_or(0);
        for (i, _) in text
            .char_indices()
            .skip(1)
            .chain(std::iter::once((text.len(), ' ')))
        {
            let id = self.ids.get(i).copied().unwrap_or(cur);
            if id != cur || i == text.len() {
                out.push(TextRun {
                    text: text[start..i].to_owned(),
                    font: (cur != 0).then(|| self.fonts[usize::from(cur) - 1].clone()),
                });
                start = i;
                cur = id;
            }
        }
        out
    }

    /// Spans of `text` with their own font: (bytes, effective font).
    #[must_use]
    pub fn spans(&self, text: &str, cell: &Font) -> Vec<(std::ops::Range<usize>, Font, bool)> {
        let mut at = 0;
        let mut out = Vec::new();
        for r in self.runs(text) {
            let len = r.text.len();
            if let Some(p) = &r.font {
                out.push((at..at + len, p.apply(cell), p.color.is_some()));
            }
            at += len;
        }
        out
    }
}

/// `over` laid on `base`.
fn merge(base: &FontPatch, over: &FontPatch) -> FontPatch {
    FontPatch {
        name: over.name.clone().or_else(|| base.name.clone()),
        size: over.size.or(base.size),
        bold: over.bold.or(base.bold),
        italic: over.italic.or(base.italic),
        underline: over.underline.or(base.underline),
        strike: over.strike.or(base.strike),
        color: over.color.or(base.color),
        vert_align: over.vert_align.or(base.vert_align),
        scheme: over.scheme.or(base.scheme),
    }
}

fn series_type(t: GridSeriesType) -> SeriesType {
    match t {
        GridSeriesType::Linear => SeriesType::Linear,
        GridSeriesType::Growth => SeriesType::Growth,
        GridSeriesType::Date => SeriesType::Date,
        GridSeriesType::AutoFill => SeriesType::AutoFill,
    }
}

fn date_unit(u: GridDateUnit) -> DateUnit {
    match u {
        GridDateUnit::Day => DateUnit::Day,
        GridDateUnit::Weekday => DateUnit::Weekday,
        GridDateUnit::Month => DateUnit::Month,
        GridDateUnit::Year => DateUnit::Year,
    }
}

/// The ranges `src` covers inside the pasted `area`, tile by tile: for each part of `src`'s
/// sub-range `sub`, the destination range and its offset from `sub`.
fn tiles(
    src: Range,
    sub: Range,
    area: Range,
    block: (u32, u32),
    transpose: bool,
) -> Vec<(Range, i64, i64)> {
    let (h, w) = block;
    let tv = (area.height() / h).max(1);
    let th = (area.width() / w).max(1);
    let (oi, oj) = (sub.r0 - src.r0, sub.c0 - src.c0);
    let (sh, sw) = (sub.height(), sub.width());
    let (di, dj, dh, dw) = if transpose {
        (oj, oi, sw, sh)
    } else {
        (oi, oj, sh, sw)
    };
    let mut out = Vec::new();
    for ti in 0..tv {
        for tj in 0..th {
            let r0 = area.r0 + ti * h + di;
            let c0 = area.c0 + tj * w + dj;
            if r0 >= MAX_ROWS || c0 >= MAX_COLS {
                continue;
            }
            let r = Range::new(
                r0,
                c0,
                (r0 + dh - 1).min(MAX_ROWS - 1),
                (c0 + dw - 1).min(MAX_COLS - 1),
            );
            out.push((
                r,
                i64::from(r0) - i64::from(sub.r0),
                i64::from(c0) - i64::from(sub.c0),
            ));
        }
    }
    out
}

fn shift_formula(f: &str, dr: i64, dc: i64) -> String {
    copy_formula(f, dr, dc).map_or_else(|_| f.to_owned(), |t| t.trim_start_matches('=').to_owned())
}

/// The nearest cell edge to a point within `tol` pixels: 0 top, 1 bottom, 2 left, 3 right.
fn nearest_edge(rect: kiln_scene::Bounds, x: f32, y: f32, tol: f32) -> Option<u8> {
    let d = [
        (y - rect.y).abs(),
        (rect.bottom() - y).abs(),
        (x - rect.x).abs(),
        (rect.right() - x).abs(),
    ];
    let (i, best) = d
        .iter()
        .copied()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    (best <= tol).then_some(i as u8)
}

impl GridDoc {
    /// The Home completions' commands (`None`: not one of them).
    pub(crate) fn home(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        use GridCommand as G;
        Some(match cmd {
            G::PasteLink { data } => self.paste_link(data.as_ref()),
            G::FillSeries { series } => self.fill_series(series),
            G::AutoFillOption { choice } => self.auto_fill_option(*choice),
            G::SetCustomLists { lists } => self.set_custom_lists(lists),
            G::ImportCustomList => self.import_custom_list(),
            G::BorderPen { mode, style, color } => {
                let was = self.home.pen.as_ref().map(|p| p.mode);
                if let Some(s) = style {
                    self.home.pen_style = *s;
                }
                if let Some(c) = color {
                    self.home.pen_color = Some(c.clone()).filter(|c| !c.is_empty());
                    if let Some(c) = &self.home.pen_color
                        && super::format::parse_color(c).is_none()
                    {
                        return Some(Err(KilnError::InvalidInput(format!(
                            "“{c}” is not a colour"
                        ))));
                    }
                }
                let mode = mode.or(was).unwrap_or(GridPenMode::Draw);
                self.home.pen = Some(GridPen {
                    mode,
                    style: self.home.pen_style,
                    color: self.home.pen_color.clone(),
                });
                self.home.pen_preview = None;
                Ok(Change::Selection)
            }
            G::BorderPenOff => {
                self.home.pen = None;
                self.home.pen_preview = None;
                Ok(Change::Selection)
            }
            G::DrawBorder {
                x0,
                y0,
                x1,
                y1,
                done,
            } => self.draw_border((*x0, *y0), (*x1, *y1), *done),
            G::DeleteNumberFormat { code } => self.delete_number_format(code),
            _ => return None,
        })
    }

    /// The Home tab's state for the chrome.
    pub(crate) fn home_state(&self) -> GridHomeState {
        let auto_fill = self
            .home
            .last_fill
            .filter(|f| self.fill_still_valid(f))
            .map(|f| {
                let geo = self.geometry_ref();
                let rect = geo.range_rect(&f.target);
                let hw = self.header_w(&geo);
                let (x, y) = geo.to_surface(&self.last_view, rect.right(), rect.bottom(), hw);
                let mut choices = vec![
                    GridAutoFillChoice::Fill(GridFill::Copy),
                    GridAutoFillChoice::Fill(GridFill::Series),
                    GridAutoFillChoice::Fill(GridFill::FormatsOnly),
                    GridAutoFillChoice::Fill(GridFill::WithoutFormatting),
                ];
                if f.dates {
                    choices.extend([
                        GridAutoFillChoice::Fill(GridFill::Days),
                        GridAutoFillChoice::Fill(GridFill::Weekdays),
                        GridAutoFillChoice::Fill(GridFill::Months),
                        GridAutoFillChoice::Fill(GridFill::Years),
                    ]);
                }
                choices.push(GridAutoFillChoice::FlashFill);
                GridAutoFillOptions {
                    x,
                    y,
                    choices,
                    current: f.choice,
                }
            });
        GridHomeState {
            series_columns: self.last_range().is_ok_and(|r| r.width() <= r.height()),
            pen: self.home.pen.clone(),
            auto_fill,
            pen_preview: self.home.pen_preview.map(|r| GridRange {
                r0: r.r0,
                c0: r.c0,
                r1: r.r1,
                c1: r.c1,
            }),
        }
    }

    fn fill_still_valid(&self, f: &LastFill) -> bool {
        self.state == f.state && self.sid().is_ok_and(|s| s == f.sheet) && self.edit.is_none()
    }

    /// After a fill-handle fill: what the Auto Fill Options button redoes.
    pub(crate) fn note_fill(&mut self, source: Range, target: Range, mode: GridFill) {
        let Ok(sid) = self.sid() else {
            return;
        };
        let styles = self.workbook.styles();
        let sheet = self.workbook.sheet(sid);
        let dates = (source.r0..=source.r1).all(|r| {
            (source.c0..=source.c1).all(|c| {
                let rec = self.workbook.record(sid, r, c);
                let st = sheet.map_or(rec.style, |s| {
                    if s.cells().contains(r, c) {
                        rec.style
                    } else {
                        s.style_at(r, c)
                    }
                });
                rec.value.as_number().is_some()
                    && kiln_formula::numfmt::NumberFormat::parse(styles.num_fmt(st)).is_date()
            })
        });
        let choice = if mode == GridFill::Auto {
            // What the drag did: a copy when the first new cell repeats the source's first.
            let first_new = if target.r1 > source.r1 {
                CellPos::new(source.r1 + 1, source.c0)
            } else if target.r0 < source.r0 {
                CellPos::new(source.r0 - 1, source.c0)
            } else if target.c1 > source.c1 {
                CellPos::new(source.r0, source.c1 + 1)
            } else {
                CellPos::new(source.r0, source.c0.saturating_sub(1))
            };
            let a = self.workbook.value(sid, first_new.row, first_new.col);
            let single = source.cell_count() == 1;
            let b = self.workbook.value(sid, source.r0, source.c0);
            if single && a == b {
                GridAutoFillChoice::Fill(GridFill::Copy)
            } else if dates {
                GridAutoFillChoice::Fill(GridFill::Days)
            } else {
                GridAutoFillChoice::Fill(GridFill::Series)
            }
        } else {
            GridAutoFillChoice::Fill(mode)
        };
        self.home.last_fill = Some(LastFill {
            sheet: sid,
            source,
            target,
            state: self.state,
            choice,
            dates,
        });
    }

    fn auto_fill_option(&mut self, choice: GridAutoFillChoice) -> Result<Change> {
        let f = self
            .home
            .last_fill
            .filter(|f| self.fill_still_valid(f))
            .ok_or_else(|| KilnError::InvalidInput("there is no fill to change".into()))?;
        if f.choice == choice {
            return Ok(Change::None);
        }
        // The fill as it was before, then the fill again with the new option.
        if !DocModel::undo(self) {
            return Err(KilnError::InvalidInput(
                "the fill can no longer be changed".into(),
            ));
        }
        match choice {
            GridAutoFillChoice::Fill(mode) => {
                let mut v = self.view();
                v.sel = Sel {
                    ranges: vec![f.source],
                    active: CellPos::new(f.source.r0, f.source.c0),
                };
                self.set_view(v);
                let (row, col) = if f.target.r1 > f.source.r1 || f.target.c1 > f.source.c1 {
                    (f.target.r1, f.target.c1)
                } else {
                    (f.target.r0, f.target.c0)
                };
                self.grid(&GridCommand::AutoFill { row, col, mode })?;
            }
            GridAutoFillChoice::FlashFill => {
                let at = if f.target.r1 > f.source.r1 {
                    CellPos::new(f.source.r1 + 1, f.source.c0)
                } else {
                    CellPos::new(f.target.r0, f.source.c0)
                };
                let mut v = self.view();
                v.sel = Sel {
                    ranges: vec![Range::cell(at.row, at.col)],
                    active: at,
                };
                self.set_view(v);
                self.flash_fill()?;
            }
        }
        if let Some(last) = &mut self.home.last_fill {
            last.choice = choice;
            last.state = self.state;
        }
        Ok(Change::Content)
    }

    // ── paste ─────────────────────────────────────────────────────────────────────────────

    /// The validation rules, threaded comments and column widths a paste carries from `src`
    /// into `area` of `sid`.
    pub(crate) fn paste_extras(
        &mut self,
        src: ClipSource,
        sid: SheetId,
        area: Range,
        block: (u32, u32),
        transpose: bool,
        extras: Extras,
    ) -> Result<Vec<kiln_sheet::Command>> {
        let mut cmds = Vec::new();
        let Some(source) = self.workbook.sheet(src.sheet) else {
            return Ok(cmds);
        };
        let src_meta = source.meta().clone();
        let src_cols: Vec<Option<F64>> = (src.range.c0..=src.range.c1)
            .map(|c| source.cols().get(c).and_then(|p| p.width))
            .collect();
        let dst_meta = self
            .workbook
            .sheet(sid)
            .map(|s| s.meta().clone())
            .unwrap_or_default();
        if extras.validation {
            let src_rules: Vec<_> = src_meta
                .validations
                .iter()
                .filter(|v| v.ranges.iter().any(|r| r.intersects(&src.range)))
                .cloned()
                .collect();
            let touches = dst_meta
                .validations
                .iter()
                .any(|v| v.ranges.iter().any(|r| r.intersects(&area)));
            if !src_rules.is_empty() || touches {
                let mut rules: Vec<_> = dst_meta
                    .validations
                    .iter()
                    .filter_map(|v| {
                        let mut v = v.clone();
                        v.ranges = v
                            .ranges
                            .iter()
                            .flat_map(|r| super::features::cf::subtract(*r, area))
                            .collect();
                        (!v.ranges.is_empty()).then_some(v)
                    })
                    .collect();
                for rule in &src_rules {
                    for sub in rule
                        .ranges
                        .iter()
                        .filter_map(|r| r.intersection(&src.range))
                    {
                        for (dest, dr, dc) in tiles(src.range, sub, area, block, transpose) {
                            let mut v = rule.clone();
                            v.id = self.new_rule_id();
                            v.ranges = vec![dest];
                            v.formula1 = v.formula1.as_deref().map(|f| shift_formula(f, dr, dc));
                            v.formula2 = v.formula2.as_deref().map(|f| shift_formula(f, dr, dc));
                            rules.push(v);
                        }
                    }
                }
                cmds.push(kiln_sheet::Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::Validations(rules)),
                });
            }
        }
        if extras.threads {
            let roots: Vec<_> = src_meta
                .threads
                .iter()
                .filter(|t| t.parent_id.is_none() && src.range.contains(t.at.row, t.at.col))
                .cloned()
                .collect();
            let gone: Vec<String> = dst_meta
                .threads
                .iter()
                .filter(|t| area.contains(t.at.row, t.at.col))
                .map(|t| t.id.clone())
                .collect();
            if !roots.is_empty() || !gone.is_empty() {
                let mut threads: Vec<_> = dst_meta
                    .threads
                    .iter()
                    .filter(|t| !gone.contains(&t.id))
                    .cloned()
                    .collect();
                let mut n = threads.len() + src_meta.threads.len();
                for root in &roots {
                    let replies: Vec<_> = src_meta
                        .threads
                        .iter()
                        .filter(|t| t.parent_id.as_deref() == Some(root.id.as_str()))
                        .cloned()
                        .collect();
                    let sub = Range::cell(root.at.row, root.at.col);
                    for (dest, _, _) in tiles(src.range, sub, area, block, transpose) {
                        let at = CellPos::new(dest.r0, dest.c0);
                        n += 1;
                        let id = super::features::comments::guid(self.now_ms, n);
                        threads.push(kiln_sheet::meta::ThreadedComment {
                            id: id.clone(),
                            at,
                            ..root.clone()
                        });
                        for r in &replies {
                            n += 1;
                            threads.push(kiln_sheet::meta::ThreadedComment {
                                id: super::features::comments::guid(self.now_ms, n),
                                at,
                                parent_id: Some(id.clone()),
                                ..r.clone()
                            });
                        }
                    }
                }
                cmds.push(kiln_sheet::Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::Threads(threads)),
                });
            }
        }
        if extras.widths {
            let w = block.1;
            let th = (area.width() / w).max(1);
            for tj in 0..th {
                for j in 0..w {
                    let col = area.c0 + tj * w + j;
                    if col >= MAX_COLS {
                        continue;
                    }
                    let from = src_cols
                        .get(j as usize % src_cols.len().max(1))
                        .copied()
                        .flatten();
                    cmds.push(kiln_sheet::Command::SetColProps {
                        sheet: sid,
                        at: col,
                        count: 1,
                        patch: ColPatch {
                            width: from.map_or(Patch::Clear, Patch::Set),
                            custom_width: Some(from.is_some()),
                            ..ColPatch::default()
                        },
                    });
                }
            }
        }
        Ok(cmds)
    }

    /// Paste Link: each cell refers to its copied cell.
    fn paste_link(&mut self, data: Option<&ClipboardData>) -> Result<Change> {
        let clip = self
            .clip_of(data, false)
            .ok_or_else(|| KilnError::InvalidInput("the clipboard is empty".into()))?;
        let src = clip
            .source
            .filter(|s| self.workbook.sheet(s.sheet).is_some())
            .ok_or_else(|| {
                KilnError::InvalidInput("Paste Link needs cells copied from this workbook.".into())
            })?;
        let sid = self.sid()?;
        let at = self.view().sel.active;
        let prefix = if src.sheet == sid {
            String::new()
        } else {
            let name = self
                .workbook
                .sheet(src.sheet)
                .map(|s| s.name().to_owned())
                .unwrap_or_default();
            format!("{}!", kiln_formula::refs::quote_sheet(&name))
        };
        let single = src.range.is_single_cell();
        let mut cells = Vec::new();
        for i in 0..src.range.height() {
            for j in 0..src.range.width() {
                let (row, col) = (at.row + i, at.col + j);
                if row >= MAX_ROWS || col >= MAX_COLS {
                    continue;
                }
                let a1 = kiln_sheet::address::cell_a1(src.range.r0 + i, src.range.c0 + j, single);
                cells.push(CellInput {
                    row,
                    col,
                    content: Content::Typed(format!("={prefix}{a1}")),
                    style: None,
                });
            }
        }
        self.exec(
            "grid.clipboard.paste_link",
            kiln_sheet::Command::SetCells { sheet: sid, cells },
        )?;
        let area = Range::new(
            at.row,
            at.col,
            (at.row + src.range.height() - 1).min(MAX_ROWS - 1),
            (at.col + src.range.width() - 1).min(MAX_COLS - 1),
        );
        let mut v = self.view();
        v.sel = Sel {
            ranges: vec![area],
            active: at,
        };
        self.set_view(v);
        Ok(Change::Content)
    }

    // ── fill ──────────────────────────────────────────────────────────────────────────────

    fn fill_series(&mut self, s: &GridSeries) -> Result<Change> {
        let sid = self.sid()?;
        // Whole rows and columns (the whole sheet) fill as far as the data reaches, as Excel
        // does; never to the sheet's last row or column.
        let selected = self.last_range()?;
        let range = self.clip_to_data(selected);
        // Down or across as the selection's shape says (before it is clipped).
        let columns = s
            .columns
            .unwrap_or(selected.height() > selected.width() || selected.is_single_cell());
        let spec = SeriesSpec {
            columns,
            kind: series_type(s.kind),
            unit: date_unit(s.unit),
            step: s.step,
            stop: s.stop,
            trend: s.trend,
        };
        if s.kind == GridSeriesType::Growth && !s.trend && s.step == 0.0 {
            return Err(KilnError::InvalidInput(
                "a growth step cannot be zero".into(),
            ));
        }
        // With a stop value, each lane of whole rows or columns fills on to its stop value, as
        // far as the selection reaches (Excel's rule), not only as far as the data.
        let range = if spec.stop.is_some()
            && (selected.is_whole_rows() || selected.is_whole_cols())
            && !range.is_single_cell()
        {
            let mut out = range;
            let lanes: Vec<CellPos> = if columns {
                (range.c0..=range.c1)
                    .map(|c| CellPos::new(range.r0, c))
                    .collect()
            } else {
                (range.r0..=range.r1)
                    .map(|r| CellPos::new(r, range.c0))
                    .collect()
            };
            for at in lanes {
                let reach = kiln_sheet::fill::series_target(
                    &self.workbook,
                    sid,
                    Range::cell(at.row, at.col),
                    &spec,
                );
                if columns {
                    out.r1 = out.r1.max(reach.r1.min(selected.r1));
                } else {
                    out.c1 = out.c1.max(reach.c1.min(selected.c1));
                }
            }
            out
        } else {
            range
        };
        let target = kiln_sheet::fill::series_target(&self.workbook, sid, range, &spec);
        self.exec(
            "grid.editing.fill_series",
            kiln_sheet::Command::FillSeries {
                sheet: sid,
                range: target,
                spec,
            },
        )?;
        if target != range {
            let mut v = self.view();
            v.sel = Sel {
                ranges: vec![target],
                active: v.sel.active,
            };
            v.head = CellPos::new(target.r1, target.c1);
            self.set_view(v);
        }
        Ok(Change::Content)
    }

    fn set_custom_lists(&mut self, lists: &[Vec<String>]) -> Result<Change> {
        let clean: Vec<Vec<String>> = lists
            .iter()
            .map(|l| {
                l.iter()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|l| l.len() >= 2)
            .collect();
        if clean.len() != lists.len() {
            return Err(KilnError::InvalidInput(
                "a custom list needs at least two entries".into(),
            ));
        }
        if clean
            .iter()
            .any(|l| l.iter().any(|s| s.chars().count() > 80))
        {
            return Err(KilnError::InvalidInput(
                "a list entry has at most 80 characters".into(),
            ));
        }
        self.log.set_custom_lists(&mut self.workbook, clean);
        Ok(Change::Selection)
    }

    /// Import: the selection's text values (by column, or the row when it is one row).
    fn import_custom_list(&mut self) -> Result<Change> {
        let sid = self.sid()?;
        let r = self.last_range()?;
        let r = self.clip_to_data(r);
        let mut list: Vec<String> = Vec::new();
        for pos in r.cells() {
            let v = self.workbook.value(sid, pos.row, pos.col);
            let t = match &v {
                CellValue::Blank => continue,
                CellValue::Number(_) => {
                    return Err(KilnError::InvalidInput(
                        "Cells without simple text were ignored: a custom list holds text.".into(),
                    ));
                }
                other => other.display_general(),
            };
            if !t.trim().is_empty() && !list.contains(&t) {
                list.push(t);
            }
        }
        if list.len() < 2 {
            return Err(KilnError::InvalidInput(
                "select at least two cells of text to import a list".into(),
            ));
        }
        let mut lists = self.workbook.custom_lists().to_vec();
        if !lists.contains(&list) {
            lists.push(list);
        }
        self.log.set_custom_lists(&mut self.workbook, lists);
        Ok(Change::Selection)
    }

    // ── border pen ────────────────────────────────────────────────────────────────────────

    fn draw_border(&mut self, a: (f32, f32), b: (f32, f32), done: bool) -> Result<Change> {
        let pen = self
            .home
            .pen
            .clone()
            .ok_or_else(|| KilnError::InvalidInput("no border pen is active".into()))?;
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let view = self.last_view;
        let at = |p: (f32, f32)| geo.to_sheet(&view, p.0, p.1, hw).map(|(_, x, y)| (x, y));
        let (Some(pa), Some(pb)) = (at(a), at(b)) else {
            self.home.pen_preview = None;
            return Ok(Change::Selection);
        };
        let cell = |p: (f32, f32)| {
            (
                geo.rows.index_at(f64::from(p.1.max(0.0))),
                geo.cols.index_at(f64::from(p.0.max(0.0))),
            )
        };
        let (ra, ca) = cell(pa);
        let (rb, cb) = cell(pb);
        let range = geo.expand_to_merges(Range::new(ra, ca, rb, cb));
        let moved = (a.0 - b.0).abs().max((a.1 - b.1).abs()) > 4.0;
        if !done {
            self.home.pen_preview = moved.then_some(range);
            return Ok(Change::Selection);
        }
        self.home.pen_preview = None;
        let style = super::format::line_style(pen.style);
        let color = pen.color.as_deref().and_then(super::format::parse_color);
        let label = "grid.font.draw_border";
        let z = view.zoom.max(0.01);
        // A click near a gridline draws (or erases) that one edge.
        let edge = if moved {
            None
        } else {
            nearest_edge(geo.display_rect(rb, cb), pb.0, pb.1, 5.0 / z).map(|e| (rb, cb, e))
        };
        match (pen.mode, edge) {
            (GridPenMode::Erase, Some((r, c, e))) => self.erase_edge(label, r, c, e)?,
            (GridPenMode::Draw | GridPenMode::Grid, Some((r, c, e))) => {
                let preset = [
                    GridBorders::Top,
                    GridBorders::Bottom,
                    GridBorders::Left,
                    GridBorders::Right,
                ][usize::from(e)];
                self.borders_over(label, vec![Range::cell(r, c)], preset, style, color)?;
            }
            (GridPenMode::Draw, None) => {
                self.borders_over(label, vec![range], GridBorders::Outside, style, color)?;
            }
            (GridPenMode::Grid, None) => {
                self.borders_over(label, vec![range], GridBorders::All, style, color)?;
            }
            (GridPenMode::Erase, None) => {
                self.borders_over(
                    label,
                    vec![range],
                    GridBorders::None,
                    BorderStyle::None,
                    None,
                )?;
            }
        }
        Ok(Change::Content)
    }

    /// Erases one edge of a cell and the neighbour's side that draws on the same line.
    fn erase_edge(&mut self, label: &'static str, r: u32, c: u32, edge: u8) -> Result<()> {
        let sid = self.sid()?;
        let clear = |side: u8| {
            let mut p = BorderPatch::default();
            match side {
                0 => p.top = Patch::Clear,
                1 => p.bottom = Patch::Clear,
                2 => p.left = Patch::Clear,
                _ => p.right = Patch::Clear,
            }
            p
        };
        let mut cmds = vec![(Range::cell(r, c), clear(edge))];
        let neighbour = match edge {
            0 if r > 0 => Some((Range::cell(r - 1, c), 1)),
            1 if r + 1 < MAX_ROWS => Some((Range::cell(r + 1, c), 0)),
            2 if c > 0 => Some((Range::cell(r, c - 1), 3)),
            3 if c + 1 < MAX_COLS => Some((Range::cell(r, c + 1), 2)),
            _ => None,
        };
        if let Some((n, side)) = neighbour {
            cmds.push((n, clear(side)));
        }
        self.exec_all(
            label,
            cmds.into_iter()
                .map(|(range, b)| kiln_sheet::Command::SetStyle {
                    sheet: sid,
                    ranges: vec![range],
                    patch: Box::new(StylePatch {
                        border: Some(b),
                        ..StylePatch::default()
                    }),
                })
                .collect(),
        )?;
        Ok(())
    }

    // ── number formats ────────────────────────────────────────────────────────────────────

    /// The workbook's number format codes beyond the built-in ones.
    pub(crate) fn custom_codes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .workbook
            .styles()
            .num_fmt_codes()
            .filter(|c| {
                kiln_sheet::styles::builtin_num_fmt_id(c).is_none()
                    && !c.eq_ignore_ascii_case("general")
                    && !self.home.deleted_codes.iter().any(|d| d == c)
            })
            .map(str::to_owned)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn delete_number_format(&mut self, code: &str) -> Result<Change> {
        if kiln_sheet::styles::builtin_num_fmt_id(code).is_some()
            || code.eq_ignore_ascii_case("general")
        {
            return Err(KilnError::InvalidInput(
                "A built-in number format cannot be deleted.".into(),
            ));
        }
        if !self.custom_codes().iter().any(|c| c == code) {
            return Err(KilnError::InvalidInput(format!(
                "the workbook has no number format “{code}”"
            )));
        }
        let general = StylePatch {
            num_fmt: Some("General".to_owned()),
            ..StylePatch::default()
        };
        let mut cmds = Vec::new();
        for s in self.workbook.sheets() {
            let ranges: Vec<Range> = s
                .cells()
                .iter()
                .filter(|(_, _, d)| self.workbook.styles().num_fmt(d.style) == code)
                .map(|(r, c, _)| Range::cell(r, c))
                .collect();
            if !ranges.is_empty() {
                cmds.push(kiln_sheet::Command::SetStyle {
                    sheet: s.id(),
                    ranges,
                    patch: Box::new(general.clone()),
                });
            }
        }
        self.exec_all("grid.number.delete_format", cmds)?;
        self.home.deleted_codes.push(code.to_owned());
        Ok(Change::Content)
    }

    // ── rich text in the editor ───────────────────────────────────────────────────────────

    /// A font command while a text entry has selected characters formats those characters
    /// (Excel's partial formatting); `None` when it applies to the cells as usual.
    pub(crate) fn rich_edit(&mut self, cmd: &EditCommand) -> Option<Result<Change>> {
        use EditCommand as E;
        let e = self.edit.as_ref()?;
        let (a, b) = (e.start.min(e.end), e.start.max(e.end));
        if a == b || e.text.starts_with('=') || e.inline.is_some() || e.text.starts_with('\'') {
            return None;
        }
        let sid = self.sid().ok()?;
        let style = self
            .workbook
            .sheet(sid)
            .map_or(0, |s| s.style_at(e.row, e.col));
        let cell_font = self.workbook.styles().get(style).font;
        let rich = e
            .rich
            .clone()
            .filter(|r| r.fits(&e.text))
            .unwrap_or_else(|| RichEdit {
                ids: vec![0; e.text.len()],
                fonts: Vec::new(),
            });
        let first = rich.patch_at(a).apply(&cell_font);
        let patch = match cmd {
            E::Bold => FontPatch {
                bold: Some(!first.bold),
                ..FontPatch::default()
            },
            E::Italic => FontPatch {
                italic: Some(!first.italic),
                ..FontPatch::default()
            },
            E::Underline => FontPatch {
                underline: Some(if first.underline == Underline::Single {
                    Underline::None
                } else {
                    Underline::Single
                }),
                ..FontPatch::default()
            },
            E::DoubleUnderline => FontPatch {
                underline: Some(if first.underline == Underline::Double {
                    Underline::None
                } else {
                    Underline::Double
                }),
                ..FontPatch::default()
            },
            E::Strikethrough => FontPatch {
                strike: Some(!first.strike),
                ..FontPatch::default()
            },
            E::Superscript | E::Subscript => {
                let want = if matches!(cmd, E::Superscript) {
                    VertAlign::Superscript
                } else {
                    VertAlign::Subscript
                };
                FontPatch {
                    vert_align: Some(if first.vert_align == want {
                        VertAlign::Baseline
                    } else {
                        want
                    }),
                    ..FontPatch::default()
                }
            }
            E::FontFamily { family } => {
                let f = family.trim();
                if f.is_empty() || f.len() > 31 {
                    return Some(Err(KilnError::InvalidInput(
                        "a font name has 1–31 characters".into(),
                    )));
                }
                FontPatch {
                    name: Some(f.into()),
                    ..FontPatch::default()
                }
            }
            E::FontSize { size } => {
                let s = f64::from(*size);
                if !(1.0..=409.0).contains(&s) {
                    return Some(Err(KilnError::InvalidInput(
                        "a font size is between 1 and 409 points".into(),
                    )));
                }
                FontPatch {
                    size: Some(F64((s * 2.0).round() / 2.0)),
                    ..FontPatch::default()
                }
            }
            E::GrowFont { .. } | E::ShrinkFont { .. } => {
                let d = if matches!(cmd, E::GrowFont { .. }) {
                    1.0
                } else {
                    -1.0
                };
                FontPatch {
                    size: Some(F64((first.size.0 + d).clamp(1.0, 409.0))),
                    ..FontPatch::default()
                }
            }
            E::FontColor { color } => {
                let c = match color.as_deref() {
                    Some(c) => match super::format::parse_color(c) {
                        Some(c) => c,
                        None => {
                            return Some(Err(KilnError::InvalidInput(format!(
                                "“{c}” is not a colour"
                            ))));
                        }
                    },
                    None => Color::Auto,
                };
                FontPatch {
                    color: Some(c),
                    ..FontPatch::default()
                }
            }
            _ => return None,
        };
        let mut rich = rich;
        rich.apply(a, b, &patch);
        if let Some(e) = &mut self.edit {
            e.rich = Some(rich);
        }
        Some(Ok(Change::Selection))
    }

    /// The content a committed text entry with rich runs gets (`None`: plain).
    pub(crate) fn rich_content(&self, e: &super::editor::EditState, text: &str) -> Option<Content> {
        let rich = e.rich.as_ref().filter(|r| r.is_rich() && r.fits(&e.text))?;
        if text.starts_with('=') || text.starts_with('\'') || text != e.text {
            return None;
        }
        let parsed = kiln_sheet::data::parse_input(text, self.workbook.props().date1904);
        if !matches!(parsed.value, CellValue::Text(_)) {
            return None;
        }
        Some(Content::Value(CellValue::RichText(std::sync::Arc::new(
            RichText::new(rich.runs(text)),
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_edits_follow_typing_and_split_runs() {
        let mut r = RichEdit {
            ids: vec![0; 5],
            fonts: Vec::new(),
        };
        r.apply(
            0,
            2,
            &FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            },
        );
        let runs = r.runs("Hello");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "He");
        assert_eq!(runs[1].font, None);
        // Typing after the bold part continues it; typing at the end does not.
        r.follow("Hello", "HeXllo");
        r.follow("HeXllo", "HeXllo!");
        let runs = r.runs("HeXllo!");
        assert_eq!(runs[0].text, "HeX");
        assert_eq!(runs[1].text, "llo!");
        r.follow("HeXllo!", "Hlo!");
        assert_eq!(r.runs("Hlo!")[0].text, "H");
    }
}
