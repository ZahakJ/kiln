// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data ▸ Data Validation: rules for the selection (whole number, decimal, list, date, time,
//! text length, custom) with input messages and error alerts, Clear All, the in-cell dropdown
//! button beside the active cell, and Circle Invalid Data. The entry is checked when it is
//! committed (`editor.rs`), with the rule's alert.

use kiln_common::{KilnError, Result};
use kiln_formula::values::{CoercionOptions, parse_date_time_text};
use kiln_protocol::commands::grid_features::{GridErrorStyle, GridValidation, GridValidationKind};
use kiln_protocol::commands::{GridCommand, GridHit};
use kiln_scene::{Bounds, DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{DataValidation, ErrorStyle, ValidationType};
use kiln_sheet::{CellPos, Command, Range};
use kiln_sheet_layout::SheetGeometry;

use super::super::GridDoc;
use super::cf::{compare, operator, subtract};
use crate::model::Change;

fn kind(k: GridValidationKind) -> ValidationType {
    match k {
        GridValidationKind::Any => ValidationType::Any,
        GridValidationKind::Whole => ValidationType::Whole,
        GridValidationKind::Decimal => ValidationType::Decimal,
        GridValidationKind::List => ValidationType::List,
        GridValidationKind::Date => ValidationType::Date,
        GridValidationKind::Time => ValidationType::Time,
        GridValidationKind::TextLength => ValidationType::TextLength,
        GridValidationKind::Custom => ValidationType::Custom,
    }
}

fn grid_kind(k: ValidationType) -> GridValidationKind {
    match k {
        ValidationType::Any => GridValidationKind::Any,
        ValidationType::Whole => GridValidationKind::Whole,
        ValidationType::Decimal => GridValidationKind::Decimal,
        ValidationType::List => GridValidationKind::List,
        ValidationType::Date => GridValidationKind::Date,
        ValidationType::Time => GridValidationKind::Time,
        ValidationType::TextLength => GridValidationKind::TextLength,
        ValidationType::Custom => GridValidationKind::Custom,
    }
}

/// A criterion as typed in the dialog, as the rule stores it.
fn criterion(k: ValidationType, text: &str, date1904: bool) -> Result<Option<String>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if let Some(f) = t.strip_prefix('=') {
        return Ok(Some(f.to_owned()));
    }
    Ok(Some(match k {
        ValidationType::List => format!("\"{}\"", t.replace('"', "\"\"")),
        ValidationType::Whole | ValidationType::Decimal | ValidationType::TextLength => {
            if t.parse::<f64>().is_err() {
                return Err(KilnError::InvalidInput(format!("\"{t}\" is not a number")));
            }
            t.to_owned()
        }
        ValidationType::Date | ValidationType::Time => {
            let opts = CoercionOptions {
                date1904,
                current_year: None,
            };
            match t.parse::<f64>() {
                Ok(n) => n.to_string(),
                Err(_) => parse_date_time_text(t, &opts)
                    .map(|n| n.to_string())
                    .ok_or_else(|| {
                        KilnError::InvalidInput(format!("\"{t}\" is not a date or time"))
                    })?,
            }
        }
        ValidationType::Custom | ValidationType::Any => t.to_owned(),
    }))
}

/// A stored criterion as the dialog shows it.
fn shown(k: ValidationType, f: Option<&str>) -> String {
    let Some(f) = f else {
        return String::new();
    };
    if k == ValidationType::List && f.len() >= 2 && f.starts_with('"') && f.ends_with('"') {
        return f[1..f.len() - 1].replace("\"\"", "\"");
    }
    if f.parse::<f64>().is_ok() && k != ValidationType::Custom {
        return f.to_owned();
    }
    format!("={f}")
}

impl GridDoc {
    pub(crate) fn validation_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        match cmd {
            GridCommand::SetValidation { rule } => {
                let sel: Vec<Range> = self
                    .view()
                    .sel
                    .ranges
                    .iter()
                    .map(|r| self.clip_to_used(*r))
                    .collect();
                let mut rules: Vec<DataValidation> = self
                    .workbook
                    .sheet(sid)
                    .map(|s| s.meta().validations.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|mut v| {
                        let mut ranges = v.ranges.clone();
                        for s in &sel {
                            ranges = ranges.iter().flat_map(|r| subtract(*r, *s)).collect();
                        }
                        v.ranges = ranges;
                        (!v.ranges.is_empty()).then_some(v)
                    })
                    .collect();
                if let Some(g) = rule {
                    let k = kind(g.kind);
                    let date1904 = self.workbook.props().date1904;
                    let formula1 = criterion(k, &g.formula1, date1904)?;
                    let formula2 = criterion(k, &g.formula2, date1904)?;
                    let range_op = matches!(
                        g.op,
                        kiln_protocol::commands::grid_features::GridCompare::Between
                            | kiln_protocol::commands::grid_features::GridCompare::NotBetween
                    );
                    if k != ValidationType::Any && formula1.is_none() {
                        return Err(KilnError::InvalidInput(
                            if k == ValidationType::List {
                                "enter the list's source (items separated by commas, or a range)"
                            } else {
                                "enter the value to compare with"
                            }
                            .into(),
                        ));
                    }
                    let needs_two = range_op
                        && !matches!(
                            k,
                            ValidationType::List | ValidationType::Custom | ValidationType::Any
                        );
                    if needs_two && formula2.is_none() {
                        return Err(KilnError::InvalidInput("enter the maximum too".into()));
                    }
                    let id = self.new_rule_id();
                    let mut v = DataValidation::new(id, sel.clone(), k);
                    v.operator = operator(g.op);
                    v.formula1 = formula1;
                    v.formula2 = if needs_two { formula2 } else { None };
                    v.allow_blank = g.allow_blank;
                    v.in_cell_dropdown = g.dropdown;
                    v.show_input_message = g.show_input;
                    v.prompt_title = Some(g.input_title.clone()).filter(|s| !s.is_empty());
                    v.prompt = Some(g.input.clone()).filter(|s| !s.is_empty());
                    v.show_error_message = g.show_error;
                    v.error_style = match g.style {
                        GridErrorStyle::Stop => ErrorStyle::Stop,
                        GridErrorStyle::Warning => ErrorStyle::Warning,
                        GridErrorStyle::Information => ErrorStyle::Information,
                    };
                    v.error_title = Some(g.error_title.clone()).filter(|s| !s.is_empty());
                    v.error = Some(g.error.clone()).filter(|s| !s.is_empty());
                    if k != ValidationType::Any || v.prompt.is_some() || v.prompt_title.is_some() {
                        rules.push(v);
                    }
                }
                self.exec(
                    "grid.data.set_validation",
                    Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::Validations(rules)),
                    },
                )?;
                Ok(Change::Content)
            }
            GridCommand::CircleInvalid { on } => {
                self.features.circles = *on;
                self.refresh_circles();
                if let Some(s) = &mut self.scene {
                    s.invalidate_all();
                }
                Ok(Change::Selection)
            }
            _ => Ok(Change::None),
        }
    }

    /// Recomputes the invalid cells to circle (at most 255, as Excel).
    pub(crate) fn refresh_circles(&mut self) {
        self.features.invalid.clear();
        if !self.features.circles {
            return;
        }
        let Ok(sid) = self.sid() else {
            return;
        };
        let Some(used) = self
            .active_sheet()
            .and_then(|(_, s)| s.cells().used_range(false))
        else {
            return;
        };
        let mut cells = self.workbook.invalid_cells(sid, used);
        cells.truncate(255);
        self.features.invalid = cells;
    }

    /// The Data Validation dialog's values for the active cell.
    pub(crate) fn validation_info(&self) -> Result<Option<GridValidation>> {
        let sid = self.sid()?;
        let a = self.view().sel.active;
        let Some(v) = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.validation_at(a.row, a.col))
        else {
            return Ok(None);
        };
        Ok(Some(GridValidation {
            kind: grid_kind(v.kind),
            op: compare(v.operator),
            formula1: shown(v.kind, v.formula1.as_deref()),
            formula2: shown(v.kind, v.formula2.as_deref()),
            allow_blank: v.allow_blank,
            dropdown: v.in_cell_dropdown,
            show_input: v.show_input_message,
            input_title: v.prompt_title.clone().unwrap_or_default(),
            input: v.prompt.clone().unwrap_or_default(),
            show_error: v.show_error_message,
            style: match v.error_style {
                ErrorStyle::Stop => GridErrorStyle::Stop,
                ErrorStyle::Warning => GridErrorStyle::Warning,
                ErrorStyle::Information => GridErrorStyle::Information,
            },
            error_title: v.error_title.clone().unwrap_or_default(),
            error: v.error.clone().unwrap_or_default(),
        }))
    }

    /// The dropdown button beside the active cell (sheet pixels), when it has a list: a
    /// validation list, or a table's total row (its functions).
    fn dropdown_rect(&self, geo: &SheetGeometry) -> Option<Bounds> {
        if self.edit.is_some() {
            return None;
        }
        let sid = self.sid().ok()?;
        let a = self.view().sel.active;
        let sheet = self.workbook.sheet(sid)?;
        let list = sheet
            .validation_at(a.row, a.col)
            .is_some_and(|v| v.kind == ValidationType::List && v.in_cell_dropdown);
        let total = sheet
            .table_at(a.row, a.col)
            .is_some_and(|t| t.totals_row && a.row == t.range.r1);
        if !list && !total {
            return None;
        }
        let cell = geo.display_rect(a.row, a.col);
        let h = cell.h.clamp(14.0, 20.0);
        Some(Bounds::new(cell.right() + 1.0, cell.bottom() - h, 16.0, h))
    }

    pub(crate) fn dropdown_button_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        let Some(b) = self.dropdown_rect(geo) else {
            return;
        };
        out.push(DrawOp::FillRect {
            rect: b,
            paint: Paint::Role(kiln_scene::ColorRole::Panel),
            radius: 2.0,
        });
        out.push(DrawOp::StrokeRect {
            rect: b.inflate(-0.5, -0.5),
            paint: Paint::Role(kiln_scene::ColorRole::Line),
            width: 1.0,
            radius: 2.0,
        });
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let mut p = PathData::new();
        p.move_to(cx - 3.5, cy - 1.5)
            .line_to(cx, cy + 2.0)
            .line_to(cx + 3.5, cy - 1.5);
        out.push(DrawOp::Path {
            path: p,
            fill: None,
            stroke: Some(Stroke::solid(Paint::Role(kiln_scene::ColorRole::Ink), 1.4)),
            rule: FillRule::NonZero,
        });
    }

    pub(crate) fn dropdown_hit(&self, x: f32, y: f32) -> Option<GridHit> {
        self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
            self.dropdown_rect(geo)?
                .contains(sx, sy)
                .then_some(GridHit::ListButton)
        })
    }

    pub(crate) fn circle_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        if !self.features.circles {
            return;
        }
        for &CellPos { row, col } in &self.features.invalid {
            let r = geo.display_rect(row, col).inflate(2.0, 1.0);
            let mut p = PathData::new();
            // An ellipse around the cell, from four curves.
            let (cx, cy, rx, ry) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
            let k = 0.552_284_8;
            p.move_to(cx + rx, cy)
                .cubic_to(cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry)
                .cubic_to(cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy)
                .cubic_to(cx - rx, cy - ry * k, cx - rx * k, cy - ry, cx, cy - ry)
                .cubic_to(cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy)
                .close();
            out.push(DrawOp::Path {
                path: p,
                fill: None,
                stroke: Some(Stroke::solid(Paint::Rgba(0xD9_2D_20_FF), 1.5)),
                rule: FillRule::NonZero,
            });
        }
    }
}

/// The chrome's name of an error style.
pub(crate) fn chrome_style(
    s: kiln_sheet::meta::ErrorStyle,
) -> kiln_protocol::commands::grid_features::GridErrorStyle {
    use kiln_protocol::commands::grid_features::GridErrorStyle as G;
    match s {
        kiln_sheet::meta::ErrorStyle::Stop => G::Stop,
        kiln_sheet::meta::ErrorStyle::Warning => G::Warning,
        kiln_sheet::meta::ErrorStyle::Information => G::Information,
    }
}
