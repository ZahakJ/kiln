// SPDX-License-Identifier: AGPL-3.0-or-later
//! The data sheet of a chart that has no workbook of its own to read (Insert ▸ Chart in Write and
//! Deck): a [`Table`] of cells the chart's references read, laid out as Office lays out a new
//! chart's embedded workbook — series names along row 1, categories (or x values, or the levels
//! of a hierarchy) down the first columns, values beside them. The host saves the table as the
//! chart's embedded workbook and builds the chart over it with
//! [`crate::edit::insert_from_range`], so the chart's series are references with caches, as
//! Office writes them.

use kiln_protocol::commands::ChartKind;
use kiln_sheet::Range;

use crate::data::{ChartCell, ChartData};
use crate::edit::{kind_of, sample_chart};
use crate::model::{ChartSpace, DataSource, Series, SeriesText};

/// The sheet a chart's embedded workbook holds its data on.
pub const DATA_SHEET: &str = "Sheet1";

/// Cells by row from A1 on one sheet (rows may differ in length; missing cells are blank).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// The sheet's name.
    pub sheet: String,
    /// The cells, row by row.
    pub rows: Vec<Vec<ChartCell>>,
}

impl Table {
    /// An empty table on `sheet`.
    #[must_use]
    pub fn new(sheet: &str) -> Self {
        Self {
            sheet: sheet.to_owned(),
            rows: Vec::new(),
        }
    }

    /// Sets the cell at `row`, `col` (0-based), growing the table as needed.
    pub fn set(&mut self, row: usize, col: usize, cell: ChartCell) {
        if self.rows.len() <= row {
            self.rows.resize(row + 1, Vec::new());
        }
        let r = &mut self.rows[row];
        if r.len() <= col {
            r.resize(col + 1, ChartCell::Blank);
        }
        r[col] = cell;
    }

    /// Number of columns (the longest row).
    #[must_use]
    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// The range from A1 to the last cell, or `None` for an empty table.
    #[must_use]
    pub fn range(&self) -> Option<Range> {
        let (h, w) = (self.rows.len(), self.width());
        if h == 0 || w == 0 {
            return None;
        }
        Some(Range::new(
            0,
            0,
            u32::try_from(h - 1).ok()?,
            u32::try_from(w - 1).ok()?,
        ))
    }
}

impl ChartData for Table {
    fn cell(&self, sheet: &str, row: u32, col: u32) -> Option<ChartCell> {
        if !sheet.eq_ignore_ascii_case(&self.sheet) {
            return None;
        }
        Some(
            self.rows
                .get(row as usize)
                .and_then(|r| r.get(col as usize))
                .cloned()
                .unwrap_or(ChartCell::Blank),
        )
    }

    fn last_row(&self, sheet: &str) -> Option<u32> {
        if !sheet.eq_ignore_ascii_case(&self.sheet) {
            return None;
        }
        u32::try_from(self.rows.len().saturating_sub(1)).ok()
    }

    fn last_col(&self, sheet: &str) -> Option<u32> {
        if !sheet.eq_ignore_ascii_case(&self.sheet) {
            return None;
        }
        u32::try_from(self.width().saturating_sub(1)).ok()
    }
}

fn number(v: f64) -> ChartCell {
    ChartCell::Number(v, "General".into())
}

fn text(s: &str) -> ChartCell {
    ChartCell::Text(s.to_owned())
}

/// The data sheet Office gives a new chart of `kind`: the values of
/// [`crate::edit::sample_series`] laid out on [`DATA_SHEET`].
#[must_use]
pub fn sample_table(kind: ChartKind) -> Table {
    table_of(&sample_chart(Some(kind)))
}

/// The cached (or literal) data of a chart laid out on [`DATA_SHEET`] as a new chart's data sheet
/// is, series in plotting order.
#[must_use]
pub fn table_of(cs: &ChartSpace) -> Table {
    let mut series: Vec<&Series> = cs
        .chart
        .plot_area
        .groups
        .iter()
        .flat_map(|g| g.series.iter())
        .collect();
    series.sort_by_key(|s| s.order);
    let kind = kind_of(cs).unwrap_or(ChartKind::ColumnClustered);
    let mut t = Table::new(DATA_SHEET);
    let xy = matches!(
        kind,
        ChartKind::ScatterMarkers
            | ChartKind::ScatterLines
            | ChartKind::ScatterSmooth
            | ChartKind::Bubble
    );
    // The label columns: one per level of a hierarchy (outermost first), else one.
    let levels: Vec<Vec<Option<String>>> = match series.first().and_then(|s| s.categories.as_ref())
    {
        Some(DataSource::MultiLevel(m)) => m.levels.iter().rev().cloned().collect(),
        Some(DataSource::Str(s)) => vec![s.points.clone()],
        Some(DataSource::Num(n)) => vec![
            n.points
                .iter()
                .map(|p| p.map(|v| crate::format::format_number(v, "General")))
                .collect(),
        ],
        None => Vec::new(),
    };
    let label_cols = levels.len().max(1);
    if xy {
        t.set(0, 0, text("X-Values"));
        if let Some(DataSource::Num(n)) = series.first().and_then(|s| s.categories.as_ref()) {
            for (i, v) in n.points.iter().enumerate() {
                t.set(i + 1, 0, v.map_or(ChartCell::Blank, number));
            }
        }
    } else {
        if label_cols > 1 {
            for (c, name) in ["Branch", "Stem", "Leaf"]
                .iter()
                .skip(3usize.saturating_sub(label_cols))
                .enumerate()
            {
                t.set(0, c, text(name));
            }
        }
        for (c, level) in levels.iter().enumerate() {
            for (i, v) in level.iter().enumerate() {
                t.set(i + 1, c, v.as_deref().map_or(ChartCell::Blank, text));
            }
        }
    }
    let mut col = label_cols;
    for s in series {
        let name = match &s.name {
            Some(SeriesText::Literal(n)) => n.clone(),
            Some(SeriesText::Ref { cache, .. }) => cache.clone().unwrap_or_default(),
            None => String::new(),
        };
        t.set(0, col, text(&name));
        if let Some(DataSource::Num(n)) = &s.values {
            for (i, v) in n.points.iter().enumerate() {
                t.set(i + 1, col, v.map_or(ChartCell::Blank, number));
            }
        }
        col += 1;
        if let Some(DataSource::Num(n)) = &s.bubble_sizes {
            t.set(0, col, text("Size"));
            for (i, v) in n.points.iter().enumerate() {
                t.set(i + 1, col, v.map_or(ChartCell::Blank, number));
            }
            col += 1;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::insert_from_range;

    fn values(cs: &crate::model::ChartSpace) -> Vec<Vec<Option<f64>>> {
        cs.chart
            .plot_area
            .groups
            .iter()
            .flat_map(|g| g.series.iter())
            .map(|s| match &s.values {
                Some(DataSource::Num(n)) => n.points.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn every_kind_is_built_over_its_sample_table_with_the_sample_values() {
        for kind in ChartKind::ALL {
            let table = sample_table(kind);
            let range = table.range().expect("cells");
            let cs = insert_from_range(&table, DATA_SHEET, range, Some(kind))
                .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert_eq!(kind_of(&cs), Some(kind), "{kind:?}");
            let sample = crate::edit::sample_chart(Some(kind));
            assert_eq!(values(&cs), values(&sample), "{kind:?}");
            // Every series reads the sheet.
            for s in cs
                .chart
                .plot_area
                .groups
                .iter()
                .flat_map(|g| g.series.iter())
            {
                let f = s.values.as_ref().and_then(DataSource::formula);
                assert!(
                    f.is_some_and(|f| f.starts_with("Sheet1!")),
                    "{kind:?}: {f:?}"
                );
            }
        }
    }

    #[test]
    fn a_column_chart_has_names_along_the_top_and_categories_down_the_side() {
        let t = sample_table(ChartKind::ColumnClustered);
        assert_eq!(t.rows.len(), 5);
        assert_eq!(t.width(), 4);
        assert_eq!(t.rows[0][1], text("Series 1"));
        assert_eq!(t.rows[1][0], text("Category 1"));
        assert_eq!(t.cell("sheet1", 1, 1), Some(number(4.3)));
        assert_eq!(t.cell("Other", 1, 1), None);
    }
}
