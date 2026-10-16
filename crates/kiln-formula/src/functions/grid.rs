// SPDX-License-Identifier: AGPL-3.0-or-later
//! A uniform, lazily read 2-D view over a range argument (reference, array or scalar).
//! References are read cell by cell and only within the sheet's used extent, so whole-column
//! criteria ranges (`A:A`) stay cheap, while the declared shape is kept for Excel's size checks.

use std::sync::Arc;

use super::support::R;
use crate::engine::{Area, Arg, CallContext, EvalValue};
use crate::refs::{Bounds, MAX_COLS, MAX_ROWS};
use crate::values::{Array2D, ErrorValue, Value};

/// A range argument.
#[derive(Debug, Clone)]
pub(crate) enum Grid {
    /// Cells of a sheet; `used_*` is the part inside the used extent.
    Ref {
        area: Area,
        used_rows: usize,
        used_cols: usize,
    },
    /// An array, possibly viewed with a larger declared shape (blank outside).
    Arr {
        arr: Arc<Array2D>,
        rows: usize,
        cols: usize,
    },
}

impl Grid {
    /// Declared rows.
    pub(crate) fn rows(&self) -> usize {
        match self {
            Grid::Ref { area, .. } => area.dims().0,
            Grid::Arr { rows, .. } => *rows,
        }
    }

    /// Declared columns.
    pub(crate) fn cols(&self) -> usize {
        match self {
            Grid::Ref { area, .. } => area.dims().1,
            Grid::Arr { cols, .. } => *cols,
        }
    }

    /// Rows that may hold data.
    pub(crate) fn used_rows(&self) -> usize {
        match self {
            Grid::Ref { used_rows, .. } => *used_rows,
            Grid::Arr { arr, rows, .. } => arr.rows().min(*rows),
        }
    }

    /// Columns that may hold data.
    pub(crate) fn used_cols(&self) -> usize {
        match self {
            Grid::Ref { used_cols, .. } => *used_cols,
            Grid::Arr { arr, cols, .. } => arr.cols().min(*cols),
        }
    }

    /// The value at (r, c) of the declared shape.
    pub(crate) fn get(&self, r: usize, c: usize, ctx: &mut dyn CallContext) -> Value {
        match self {
            Grid::Ref { area, .. } => {
                let (row, col) = (area.bounds.r0 as usize + r, area.bounds.c0 as usize + c);
                if row >= MAX_ROWS as usize || col >= MAX_COLS as usize {
                    return Value::Blank;
                }
                ctx.read_cell(area.sheet, row as u32, col as u32)
            }
            Grid::Arr { arr, .. } => {
                if r < arr.rows() && c < arr.cols() {
                    arr.get(r, c).clone()
                } else {
                    Value::Blank
                }
            }
        }
    }
}

/// A grid over one area.
pub(crate) fn ref_grid(area: Area, ctx: &dyn CallContext) -> Grid {
    let c = ctx.clamp_area(area);
    let (used_rows, used_cols) = c.dims();
    Grid::Ref {
        area,
        used_rows,
        used_cols,
    }
}

fn array_grid(arr: Arc<Array2D>) -> Grid {
    let (rows, cols) = (arr.rows(), arr.cols());
    Grid::Arr { arr, rows, cols }
}

/// The grid of an evaluated argument (multi-area references and lambdas are `#VALUE!`).
pub(crate) fn grid_of_value(v: EvalValue, ctx: &mut dyn CallContext) -> R<Grid> {
    match v {
        EvalValue::Ref(r) => match r.single() {
            Some(a) => Ok(ref_grid(*a, ctx)),
            None => Err(ErrorValue::Value),
        },
        EvalValue::Value(Value::Lambda(_)) => Err(ErrorValue::Value),
        EvalValue::Value(Value::Array(a)) => Ok(array_grid(a)),
        EvalValue::Missing => Ok(array_grid(Arc::new(Array2D::filled(1, 1, Value::Blank)))),
        EvalValue::Value(v) => Ok(array_grid(Arc::new(Array2D::filled(1, 1, v)))),
    }
}

/// The grid of an argument position.
pub(crate) fn grid_of(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Grid> {
    let v = match args.get_mut(i) {
        Some(a) => ctx.force(a),
        None => EvalValue::Missing,
    };
    grid_of_value(v, ctx)
}

/// A grid with the shape of `like`, anchored at the top-left of the argument (`SUMIF` and
/// `AVERAGEIF` resize their sum range this way).
pub(crate) fn resized_grid(
    args: &mut [Arg<'_>],
    i: usize,
    like: &Grid,
    ctx: &mut dyn CallContext,
) -> R<Grid> {
    let v = match args.get_mut(i) {
        Some(a) => ctx.force(a),
        None => EvalValue::Missing,
    };
    if let EvalValue::Ref(r) = &v {
        let Some(s) = r.single() else {
            return Err(ErrorValue::Value);
        };
        // The area may run past the sheet edge; cells there read as blank.
        let r1 = s.bounds.r0 + (like.rows() - 1) as u32;
        let c1 = s.bounds.c0 + (like.cols() - 1) as u32;
        let area = Area {
            sheet: s.sheet,
            bounds: Bounds {
                r0: s.bounds.r0,
                c0: s.bounds.c0,
                r1,
                c1,
            },
        };
        return Ok(ref_grid(area, ctx));
    }
    let g = grid_of_value(v, ctx)?;
    match g {
        Grid::Arr { arr, .. } => Ok(Grid::Arr {
            arr,
            rows: like.rows(),
            cols: like.cols(),
        }),
        other => Ok(other),
    }
}
