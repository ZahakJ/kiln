// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the Grid chrome asks the core (`Query::Grid`): the state of the sheet tabs, name box,
//! formula bar, mode, editor and ribbon; hit testing; the Format Cells dialog; number format
//! previews; formula hints; Zoom to Selection; the function list.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::{
    GridCellFormat, GridEditor, GridFormatCells, GridFunctionInfo, GridHit, GridLayout, GridMode,
    GridQuery, GridQueryResult, GridRefSpan, GridSheetTab, GridState,
};
use kiln_protocol::geom::Rect;
use kiln_sheet::address::cell_a1;
use kiln_sheet::meta::SheetVisibility;
use kiln_sheet::styles::{CellStyle, Fill, PatternType, VertAlign};
use kiln_sheet::{CellValue, Range};
use kiln_sheet_layout::HitTarget;
use kiln_sheet_layout::overlay::formula_refs;

use super::GridDoc;
use super::editor::{EditMode, byte_to_utf16, formula_hint, utf16_to_byte};
use super::format::{
    category, grid_h_align, grid_side, grid_v_align, hex, pattern_name, preset_name, underline_name,
};

fn fill_color(fill: &Fill, theme: &kiln_sheet::Theme) -> Option<String> {
    match fill {
        Fill::Pattern {
            pattern: PatternType::None,
            ..
        } => None,
        f => hex(f.main_color(), theme, 0xFF_FFFF),
    }
}

impl GridDoc {
    fn cell_format(&self, style: &CellStyle) -> GridCellFormat {
        let theme = self.workbook.theme();
        let a = self.view().sel.active;
        let merged = self.geometry_ref().merge_at(a.row, a.col).is_some();
        let font_color = match style.font.color {
            None | Some(kiln_sheet::color::Color::Auto) => None,
            c => hex(c, theme, 0),
        };
        GridCellFormat {
            font_family: style.font.name.to_string(),
            font_size: style.font.size.0,
            bold: style.font.bold,
            italic: style.font.italic,
            underline: style.font.underline != kiln_sheet::styles::Underline::None,
            strike: style.font.strike,
            font_color,
            fill_color: fill_color(&style.fill, theme),
            h_align: grid_h_align(style.alignment.horizontal),
            v_align: grid_v_align(style.alignment.vertical),
            wrap: style.alignment.wrap_text,
            merged,
            indent: style.alignment.indent,
            rotation: style.alignment.text_rotation,
            number_format: style.num_fmt.to_string(),
            number_format_name: preset_name(&style.num_fmt).to_owned(),
        }
    }

    /// The name box: the defined name of the selection, its size while it is several cells
    /// wide, or the active cell.
    fn name_box(&self) -> String {
        let v = self.view();
        let Some(r) = v.sel.ranges.last().copied() else {
            return String::new();
        };
        let sid = self.sid().ok();
        // A defined name that is exactly the selection.
        if let Some(sid) = sid
            && let Some(sheet) = self.workbook.sheet(sid)
        {
            let abs = format!("{}!{}", quote(sheet.name()), r.to_a1(true));
            for n in self.workbook.names() {
                if n.hidden || n.name.starts_with("_xlnm") {
                    continue;
                }
                if n.scope.is_none_or(|s| s == sid)
                    && n.formula.replace('\'', "") == abs.replace('\'', "")
                {
                    return n.name.clone();
                }
            }
        }
        cell_a1(v.sel.active.row, v.sel.active.col, false)
    }

    fn mode(&self) -> GridMode {
        match self.edit.as_ref().map(|e| e.mode) {
            None => GridMode::Ready,
            Some(EditMode::Enter) => GridMode::Enter,
            Some(EditMode::Edit) => GridMode::Edit,
            Some(EditMode::Point) => GridMode::Point,
        }
    }

    fn editor_state(&self) -> Option<GridEditor> {
        let e = self.edit.as_ref()?;
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let view = self.last_view;
        let sheet_name = self
            .active_sheet()
            .map(|(_, s)| s.name().to_owned())
            .unwrap_or_default();
        let text = e.display_text();
        let (s, en) = e.byte_selection();
        let refs = formula_refs(&e.text, &sheet_name)
            .into_iter()
            .map(|r| GridRefSpan {
                start: byte_to_utf16(&e.text, r.span.start),
                end: byte_to_utf16(&e.text, r.span.end),
                color: format!("{:06X}", r.color),
            })
            .collect();
        let cell = geo.display_rect(e.row, e.col);
        let z = view.zoom;
        let (x, y) = geo.to_surface(&view, cell.x, cell.y, hw);
        let hint = if e.text.starts_with('=') {
            formula_hint(&e.text, e.end)
        } else {
            None
        };
        Some(GridEditor {
            text: text.clone(),
            start: byte_to_utf16(&text, s),
            end: byte_to_utf16(&text, en),
            refs,
            completions: e.completions.clone(),
            completion_index: e.completion_index as u32,
            inline_completion: e.inline.clone(),
            hint,
            rect: Rect::new(x, y, cell.w * z, cell.h * z),
            caret: Rect::new(x, y, 1.5, cell.h * z),
            pick_list: e.pick_list.clone(),
            formula_bar: e.formula_bar,
        })
    }

    /// The state of the chrome.
    pub(crate) fn grid_state(&self) -> GridState {
        let theme = self.workbook.theme();
        let sheets = self
            .workbook
            .sheets()
            .iter()
            .map(|s| GridSheetTab {
                name: s.name().to_owned(),
                color: hex(s.meta().tab_color, theme, 0),
                hidden: s.meta().visibility != SheetVisibility::Visible,
            })
            .collect();
        let v = self.view();
        let a = v.sel.active;
        let sid = self.sid().ok();
        let formula = match (&self.edit, sid) {
            (Some(e), _) => e.display_text(),
            (None, Some(sid)) => self.edit_text(sid, a.row, a.col),
            _ => String::new(),
        };
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let (cw, ch) = self.content_extent(&geo, &self.last_view);
        let sheet_view = self
            .active_sheet()
            .map(|(_, s)| s.meta().view.clone())
            .unwrap_or_default();
        let ranges = &v.sel.ranges;
        GridState {
            sheets,
            active_sheet: self.active as u32,
            name_box: self.name_box(),
            formula,
            mode: self.mode(),
            editor: self.editor_state(),
            format: self.cell_format(&self.active_style()),
            layout: GridLayout {
                header_w: hw,
                header_h: geo.header_h(),
                freeze_rows: geo.freeze_rows,
                freeze_cols: geo.freeze_cols,
                frozen_h: geo.frozen_h(),
                frozen_w: geo.frozen_w(),
                content_w: cw,
                content_h: ch,
                sheet_zoom: f32::from(sheet_view.zoom) / 100.0,
                split: sheet_view.split.is_some(),
            },
            gridlines: sheet_view.show_gridlines,
            headings: sheet_view.show_headings,
            show_formulas: sheet_view.show_formulas,
            reveal: self.reveal,
            copy_marked: self.copy.is_some(),
            multi: ranges.len() > 1
                || ranges
                    .last()
                    .is_some_and(|r| !r.is_single_cell() && geo.merge_at(r.r0, r.c0) != Some(*r)),
            whole_rows: !ranges.is_empty() && ranges.iter().all(Range::is_whole_rows),
            whole_columns: !ranges.is_empty() && ranges.iter().all(Range::is_whole_cols),
            stats: self.stats_text(),
            drawing: self.drawing_state(),
            names: {
                let mut n: Vec<String> = self
                    .workbook
                    .names()
                    .iter()
                    .filter(|n| {
                        !n.hidden
                            && !n.name.starts_with("_xlnm")
                            && n.scope.is_none_or(|s| Some(s) == sid)
                    })
                    .map(|n| n.name.clone())
                    .collect();
                n.sort();
                n.dedup();
                n
            },
        }
    }

    /// The status bar's statistics, formatted like the active cell (Excel shows a sum of
    /// prices as a price).
    fn stats_text(&self) -> Option<kiln_protocol::commands::GridStats> {
        let s = crate::model::DocModel::status(self).cells?;
        if s.count < 2 {
            return None;
        }
        let code = self.active_style().num_fmt.to_string();
        let code = if code.trim() == "@" {
            "General".to_owned()
        } else {
            code
        };
        let date1904 = self.workbook.props().date1904;
        let f = |n: f64| {
            if code.trim().is_empty() || code.eq_ignore_ascii_case("general") {
                kiln_sheet::format::general(n, 15)
            } else {
                kiln_sheet::format::format_value(&CellValue::Number(n), &code, date1904)
                    .text
                    .trim()
                    .to_owned()
            }
        };
        Some(kiln_protocol::commands::GridStats {
            average: s.average.map(f),
            count: s.count,
            numerical_count: s.numeric_count,
            min: s.min.map(f),
            max: s.max.map(f),
            sum: (s.numeric_count > 0).then(|| f(s.sum)),
        })
    }

    fn format_cells(&self) -> GridFormatCells {
        let style = self.active_style();
        let theme = self.workbook.theme();
        let (pattern, pattern_color) = match &style.fill {
            Fill::Pattern { pattern, fg, .. } if *pattern != PatternType::Solid => {
                (pattern_name(*pattern).to_owned(), hex(*fg, theme, 0))
            }
            Fill::Pattern { pattern, .. } => (pattern_name(*pattern).to_owned(), None),
            Fill::Gradient(_) => ("solid".to_owned(), None),
        };
        let b = style.border;
        let diag = b.diagonal;
        GridFormatCells {
            number_format: style.num_fmt.to_string(),
            category: category(&style.num_fmt).to_owned(),
            sample: self.preview(&style.num_fmt),
            format: self.cell_format(&style),
            underline: underline_name(style.font.underline).to_owned(),
            superscript: style.font.vert_align == VertAlign::Superscript,
            subscript: style.font.vert_align == VertAlign::Subscript,
            pattern,
            pattern_color,
            shrink: style.alignment.shrink_to_fit,
            border_left: grid_side(b.left, theme),
            border_right: grid_side(b.right, theme),
            border_top: grid_side(b.top, theme),
            border_bottom: grid_side(b.bottom, theme),
            border_diagonal_up: if b.diagonal_up {
                grid_side(diag, theme)
            } else {
                None
            },
            border_diagonal_down: if b.diagonal_down {
                grid_side(diag, theme)
            } else {
                None
            },
            locked: style.protection.locked,
            hidden: style.protection.hidden,
            fonts: vec![
                "Aptos".into(),
                "Arial".into(),
                "Calibri".into(),
                "Calibri Light".into(),
                "Cambria".into(),
                "Consolas".into(),
                "Courier New".into(),
                "Figtree".into(),
                "Fraunces".into(),
                "Georgia".into(),
                "Liberation Mono".into(),
                "Liberation Sans".into(),
                "Liberation Serif".into(),
                "Segoe UI".into(),
                "Times New Roman".into(),
                "Verdana".into(),
            ],
        }
    }

    /// The active cell's value in a format.
    fn preview(&self, code: &str) -> String {
        let Ok(sid) = self.sid() else {
            return String::new();
        };
        let a = self.view().sel.active;
        let v = self.workbook.value(sid, a.row, a.col);
        let v = if v.is_blank() {
            CellValue::Number(1234.5)
        } else {
            v
        };
        kiln_sheet::format::format_value(&v, code, self.workbook.props().date1904).text
    }

    /// Answers a Grid query.
    pub(crate) fn grid_query(&self, q: &GridQuery) -> Result<GridQueryResult> {
        Ok(match q {
            GridQuery::State => GridQueryResult::State(Box::new(self.grid_state())),
            GridQuery::HitTest { x, y } => {
                let geo = self.geometry_ref();
                let hw = self.header_w(&geo);
                let sel = self.view().sel.ranges.last().copied();
                let fill_sel = if self.edit.is_none() && self.view().sel.ranges.len() == 1 {
                    sel
                } else {
                    None
                };
                if let Some(hit) = self.hit_chart(*x, *y) {
                    return Ok(GridQueryResult::Hit(hit));
                }
                let hit = match geo.hit_test(&self.last_view, *x, *y, hw, fill_sel.as_ref()) {
                    None => GridHit::None,
                    Some(HitTarget::Cell { row, col }) => GridHit::Cell { row, col },
                    Some(HitTarget::FillHandle) => GridHit::FillHandle,
                    Some(HitTarget::ColumnHeader { col }) => GridHit::ColumnHeader { col },
                    Some(HitTarget::RowHeader { row }) => GridHit::RowHeader { row },
                    Some(HitTarget::ColumnBorder { col }) => {
                        let (x, w) = geo.cols.span(col);
                        let (sx, _) = geo.to_surface(&self.last_view, x as f32, geo.frozen_h(), hw);
                        GridHit::ColumnBorder {
                            col,
                            start: sx,
                            size: w as f32,
                        }
                    }
                    Some(HitTarget::RowBorder { row }) => {
                        let (y, h) = geo.rows.span(row);
                        let (_, sy) = geo.to_surface(&self.last_view, geo.frozen_w(), y as f32, hw);
                        GridHit::RowBorder {
                            row,
                            start: sy,
                            size: h as f32,
                        }
                    }
                    Some(HitTarget::Corner) => GridHit::Corner,
                };
                GridQueryResult::Hit(hit)
            }
            GridQuery::FormatCells => GridQueryResult::FormatCells(Box::new(self.format_cells())),
            GridQuery::FormatPreview { code } => {
                if code.len() > 255 {
                    return Err(KilnError::InvalidInput(
                        "a number format has at most 255 characters".into(),
                    ));
                }
                GridQueryResult::Preview(self.preview(code))
            }
            GridQuery::FormulaHint { text, caret } => {
                GridQueryResult::Hint(formula_hint(text, utf16_to_byte(text, *caret)))
            }
            GridQuery::ZoomToSelection { width, height } => {
                let geo = self.geometry_ref();
                let r = self
                    .view()
                    .sel
                    .ranges
                    .last()
                    .copied()
                    .unwrap_or_else(|| Range::cell(0, 0));
                let rect = geo.range_rect(&r);
                let hw = self.header_w(&geo);
                let w = (width - hw).max(1.0);
                let h = (height - geo.header_h()).max(1.0);
                let z = (w / rect.w.max(1.0)).min(h / rect.h.max(1.0));
                GridQueryResult::Zoom(z.clamp(0.1, 4.0))
            }
            GridQuery::RecommendedCharts => GridQueryResult::Charts(self.recommended_charts()),
            GridQuery::Functions { prefix, category } => {
                let p = prefix.to_uppercase();
                let mut list: Vec<GridFunctionInfo> = kiln_formula::functions::docs()
                    .iter()
                    .filter(|d| d.name.starts_with(&p))
                    .filter(|d| category.as_deref().is_none_or(|c| d.category_name == c))
                    .map(|d| GridFunctionInfo {
                        name: d.name.clone(),
                        category: d.category_name.clone(),
                        syntax: d.syntax.clone(),
                        summary: d.summary.clone(),
                    })
                    .collect();
                list.sort_by(|a, b| a.name.cmp(&b.name));
                GridQueryResult::Functions(list)
            }
        })
    }
}

fn quote(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}
