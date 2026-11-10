// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of Kiln Grid's Home tab completions: Home ▸ Fill ▸ Series,
//! the Auto Fill Options button after a fill-handle drag, custom fill lists, and the border pen
//! (Home ▸ Borders ▸ Draw Border, Draw Border Grid, Erase Border). The commands are variants of
//! [`super::GridCommand`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{GridFill, GridLineStyle};

/// Home ▸ Fill ▸ Series ▸ Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSeriesType {
    /// Add the step value to each cell.
    #[default]
    Linear,
    /// Multiply each cell by the step value.
    Growth,
    /// Dates by the date unit.
    Date,
    /// What the fill handle would do with the first cells.
    AutoFill,
}

/// Home ▸ Fill ▸ Series ▸ Date unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridDateUnit {
    /// Days.
    #[default]
    Day,
    /// Monday to Friday.
    Weekday,
    /// Months.
    Month,
    /// Years.
    Year,
}

/// The Series dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridSeries {
    /// Series in columns (down each column) rather than rows (`None`: as Excel picks from the
    /// selection's shape, columns when it is taller than wide).
    pub columns: Option<bool>,
    /// Type.
    pub kind: GridSeriesType,
    /// Date unit (Date only).
    #[serde(default)]
    pub unit: GridDateUnit,
    /// Step value.
    pub step: f64,
    /// Stop value: the series stops before passing it (and, from one cell, runs on to it).
    #[serde(default)]
    pub stop: Option<f64>,
    /// Trend: the best-fit line (Linear) or curve (Growth) of the selection's first values
    /// replaces the step.
    #[serde(default)]
    pub trend: bool,
}

/// One choice of the Auto Fill Options button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridAutoFillChoice {
    /// Fill again with this option.
    Fill(GridFill),
    /// Flash Fill the filled cells from the examples above them.
    FlashFill,
}

/// The Auto Fill Options button shown after a fill-handle drag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridAutoFillOptions {
    /// x of the filled range's bottom-right corner (surface CSS pixels).
    pub x: f32,
    /// y of that corner.
    pub y: f32,
    /// The choices, in Excel's order.
    pub choices: Vec<GridAutoFillChoice>,
    /// The one in effect.
    pub current: GridAutoFillChoice,
}

/// What the border pen does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridPenMode {
    /// Draw Border: a click draws the nearest cell edge, a drag the outline of the cells.
    Draw,
    /// Draw Border Grid: a drag draws every edge of the cells.
    Grid,
    /// Erase Border: a click erases the nearest edge, a drag every edge of the cells.
    Erase,
}

/// The border pen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridPen {
    /// Mode.
    pub mode: GridPenMode,
    /// Line style.
    pub style: GridLineStyle,
    /// Line colour `RRGGBB` (`None`: automatic).
    pub color: Option<String>,
}

/// The Home tab's state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridHomeState {
    /// The border pen, while one is active.
    #[serde(default)]
    pub pen: Option<GridPen>,
    /// The Auto Fill Options button, until the next command.
    #[serde(default)]
    pub auto_fill: Option<GridAutoFillOptions>,
    /// The range the border pen is dragging over (a preview).
    #[serde(default)]
    pub pen_preview: Option<super::GridRange>,
    /// Home ▸ Fill ▸ Series would fill down columns (Excel's choice for the selection's shape:
    /// rows when it is wider than tall).
    #[serde(default)]
    pub series_columns: bool,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::commands::grid::GridCommand as C;
    use crate::samples::Samples;

    pub(crate) fn series() -> GridSeries {
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::Date,
            unit: GridDateUnit::Month,
            step: 2.0,
            stop: Some(46_000.0),
            trend: false,
        }
    }

    pub(crate) fn pen() -> GridPen {
        GridPen {
            mode: GridPenMode::Grid,
            style: GridLineStyle::Dashed,
            color: Some("C8352B".into()),
        }
    }

    pub(crate) fn state() -> GridHomeState {
        GridHomeState {
            pen: Some(pen()),
            auto_fill: Some(GridAutoFillOptions {
                x: 320.0,
                y: 188.5,
                choices: vec![
                    GridAutoFillChoice::Fill(GridFill::Copy),
                    GridAutoFillChoice::Fill(GridFill::Series),
                    GridAutoFillChoice::FlashFill,
                ],
                current: GridAutoFillChoice::Fill(GridFill::Series),
            }),
            pen_preview: Some(super::super::GridRange {
                r0: 1,
                c0: 1,
                r1: 4,
                c1: 3,
            }),
            series_columns: true,
        }
    }

    /// The Home commands' samples.
    pub(crate) fn commands() -> Vec<Command> {
        vec![
            C::FillSeries { series: series() },
            C::AutoFillOption {
                choice: GridAutoFillChoice::Fill(GridFill::FormatsOnly),
            },
            C::SetCustomLists {
                lists: vec![vec!["North".into(), "South".into(), "East".into()]],
            },
            C::ImportCustomList,
            C::BorderPen {
                mode: Some(GridPenMode::Erase),
                style: Some(GridLineStyle::Thick),
                color: Some(String::new()),
            },
            C::BorderPen {
                mode: None,
                style: None,
                color: Some("1F4E79".into()),
            },
            C::BorderPenOff,
            C::DrawBorder {
                x0: 120.0,
                y0: 40.0,
                x1: 300.5,
                y1: 98.0,
                done: true,
            },
            C::DeleteNumberFormat {
                code: "0.000\" kg\"".into(),
            },
            C::PasteLink { data: None },
        ]
        .into_iter()
        .map(Command::Grid)
        .collect()
    }

    crate::samples::samples_for! {
        GridSeriesType => vec![GridSeriesType::Linear, GridSeriesType::AutoFill];
        GridDateUnit => vec![GridDateUnit::Day, GridDateUnit::Year];
        GridSeries => vec![series()];
        GridAutoFillChoice => vec![GridAutoFillChoice::Fill(GridFill::Days), GridAutoFillChoice::FlashFill];
        GridAutoFillOptions => vec![state().auto_fill.unwrap_or_else(|| unreachable!())];
        GridPenMode => vec![GridPenMode::Draw, GridPenMode::Erase];
        GridPen => vec![pen()];
        GridHomeState => vec![state(), GridHomeState::default()];
    }

    /// Registers the types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            GridSeriesType, GridDateUnit, GridSeries, GridAutoFillChoice, GridAutoFillOptions,
            GridPenMode, GridPen, GridHomeState,
        );
    }
}
