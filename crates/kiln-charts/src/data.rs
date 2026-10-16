// SPDX-License-Identifier: AGPL-3.0-or-later
//! Series data: chart references (`Sheet1!$B$2:$B$7`), refreshing the cached values from a
//! workbook, and turning a selected range into series the way Excel does.
//!
//! Grid charts refresh from the open workbook. Write and Deck charts refresh from their embedded
//! workbook (`embeddings/*.xlsx`), which the host reads with `kiln-io-xlsx` into the same
//! [`Workbook`] type; charts without one keep their literal data.

use kiln_formula::refs::column_to_letters;
use kiln_sheet::{CellValue, Range, Sheet, Workbook};

use crate::model::*;

/// One area of a reference: a sheet and a range on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    /// The sheet name (unquoted).
    pub sheet: String,
    /// The range.
    pub range: Range,
}

impl Area {
    /// The reference text Excel writes: `Sheet1!$A$1:$A$5`, quoting names that need it.
    #[must_use]
    pub fn to_formula(&self) -> String {
        format!("{}!{}", quote_sheet(&self.sheet), self.range.to_a1(true))
    }

    /// Number of cells.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.range.cell_count()
    }

    /// Whether the area has no cells (never: ranges are at least one cell).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `Sheet1`, or `'Q1 sales'` with embedded quotes doubled.
#[must_use]
pub fn quote_sheet(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && Range::parse_a1(name).is_none();
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// Parses a chart reference: one area or a parenthesised, comma-separated union of areas, each
/// `Sheet!A1:B2` with an optionally quoted sheet name. Returns `None` for anything else (defined
/// names, external references), in which case the cached values stay as they are.
#[must_use]
pub fn parse_reference(formula: &str) -> Option<Vec<Area>> {
    let mut f = formula.trim();
    if let Some(rest) = f.strip_prefix('=') {
        f = rest.trim();
    }
    if f.starts_with('(') && f.ends_with(')') && f.len() >= 2 {
        f = &f[1..f.len() - 1];
    }
    let mut areas = Vec::new();
    let mut rest = f;
    loop {
        let (area, after) = parse_area(rest)?;
        areas.push(area);
        let after = after.trim_start();
        if after.is_empty() {
            break;
        }
        rest = after.strip_prefix(',')?.trim_start();
    }
    Some(areas)
}

fn parse_area(s: &str) -> Option<(Area, &str)> {
    let (sheet, after_sheet) = if let Some(stripped) = s.strip_prefix('\'') {
        let mut name = String::new();
        let mut chars = stripped.char_indices().peekable();
        let mut end = None;
        while let Some((i, c)) = chars.next() {
            if c == '\'' {
                if chars.peek().map(|(_, c)| *c) == Some('\'') {
                    name.push('\'');
                    chars.next();
                } else {
                    end = Some(i + 1);
                    break;
                }
            } else {
                name.push(c);
            }
        }
        let end = end?;
        let after = stripped[end..].strip_prefix('!')?;
        (name, after)
    } else {
        let bang = s.find('!')?;
        let name = &s[..bang];
        if name.is_empty() || name.contains([',', '(', ')', '[', ']']) {
            return None;
        }
        (name.to_owned(), &s[bang + 1..])
    };
    let end = after_sheet.find(',').unwrap_or(after_sheet.len());
    let range = Range::parse_a1(&after_sheet[..end])?;
    Some((Area { sheet, range }, &after_sheet[end..]))
}

/// A cell as a chart reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartCell {
    /// Nothing.
    Blank,
    /// A number, with its cell's number format code.
    Number(f64, String),
    /// Text (including formatted logicals and error codes).
    Text(String),
}

/// What series references read from. Implemented for [`Workbook`]; hosts with another data store
/// (a Deck chart's literal table) implement it too.
pub trait ChartData {
    /// The cell at `row`, `col` (0-based) of the sheet named `sheet` (case-insensitive), or
    /// `None` when there is no such sheet.
    fn cell(&self, sheet: &str, row: u32, col: u32) -> Option<ChartCell>;
    /// The last row with content on the sheet (to clip whole-column references).
    fn last_row(&self, sheet: &str) -> Option<u32>;
    /// The last column with content on the sheet.
    fn last_col(&self, sheet: &str) -> Option<u32>;
}

fn sheet_named<'a>(wb: &'a Workbook, name: &str) -> Option<&'a Sheet> {
    wb.sheet_by_name(name).or_else(|| {
        wb.sheets()
            .iter()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    })
}

impl ChartData for Workbook {
    fn cell(&self, sheet: &str, row: u32, col: u32) -> Option<ChartCell> {
        let s = sheet_named(self, sheet)?;
        let value = s.cells().value(row, col);
        Some(match value {
            CellValue::Blank => ChartCell::Blank,
            CellValue::Number(n) => {
                ChartCell::Number(*n, self.styles().num_fmt(s.style_at(row, col)).to_owned())
            }
            other => ChartCell::Text(other.display_general()),
        })
    }

    fn last_row(&self, sheet: &str) -> Option<u32> {
        sheet_named(self, sheet)?.cells().rows().last().copied()
    }

    fn last_col(&self, sheet: &str) -> Option<u32> {
        let s = sheet_named(self, sheet)?;
        s.cells().iter().map(|(_, c, _)| c).max()
    }
}

/// Most cells one reference reads (a whole-column reference is clipped to the used rows first).
pub const MAX_REFERENCE_CELLS: u64 = 1 << 20;

/// The cells of a reference in reading order (row by row within each area, areas in order).
fn read_cells(data: &dyn ChartData, formula: &str) -> Option<Vec<ChartCell>> {
    let areas = parse_reference(formula)?;
    let mut out = Vec::new();
    for a in areas {
        let mut r = a.range;
        if let Some(last) = data.last_row(&a.sheet) {
            r.r1 = r.r1.min(last.max(r.r0));
        } else {
            data.cell(&a.sheet, 0, 0)?;
        }
        if let Some(last) = data.last_col(&a.sheet) {
            r.c1 = r.c1.min(last.max(r.c0));
        }
        if r.cell_count() > MAX_REFERENCE_CELLS {
            return None;
        }
        for row in r.r0..=r.r1 {
            for col in r.c0..=r.c1 {
                out.push(data.cell(&a.sheet, row, col)?);
            }
        }
    }
    Some(out)
}

fn refresh_source(data: &dyn ChartData, src: &mut DataSource) -> bool {
    let Some(formula) = src.formula().map(str::to_owned) else {
        return false;
    };
    match src {
        DataSource::Num(n) => {
            let Some(cells) = read_cells(data, &formula) else {
                return false;
            };
            let points: Vec<Option<f64>> = cells
                .iter()
                .map(|c| match c {
                    ChartCell::Number(v, _) => Some(*v),
                    _ => None,
                })
                .collect();
            let code = cells.iter().find_map(|c| match c {
                ChartCell::Number(_, f) => Some(f.clone()),
                _ => None,
            });
            let changed = n.points != points || (!n.has_cache);
            n.points = points;
            n.format_code = code
                .or_else(|| n.format_code.take())
                .or(Some("General".into()));
            n.has_cache = true;
            changed
        }
        DataSource::Str(s) => {
            let Some(cells) = read_cells(data, &formula) else {
                return false;
            };
            let points: Vec<Option<String>> = cells.iter().map(cell_text).collect();
            let changed = s.points != points || !s.has_cache;
            s.points = points;
            s.has_cache = true;
            changed
        }
        DataSource::MultiLevel(m) => {
            let Some(areas) = parse_reference(&formula) else {
                return false;
            };
            let Some(a) = areas.first() else {
                return false;
            };
            // Columns are levels (outermost first); Excel stores the innermost level first.
            let r = a.range;
            let (levels_n, points_n) = (r.width(), r.height());
            if u64::from(levels_n) * u64::from(points_n) > MAX_REFERENCE_CELLS {
                return false;
            }
            let mut levels: Vec<Vec<Option<String>>> = Vec::new();
            for col in (r.c0..=r.c1).rev() {
                let mut lvl = Vec::with_capacity(points_n as usize);
                for row in r.r0..=r.r1 {
                    let Some(c) = data.cell(&a.sheet, row, col) else {
                        return false;
                    };
                    lvl.push(cell_text(&c));
                }
                levels.push(lvl);
            }
            let changed = m.levels != levels || !m.has_cache;
            m.levels = levels;
            m.has_cache = true;
            changed
        }
    }
}

fn cell_text(c: &ChartCell) -> Option<String> {
    match c {
        ChartCell::Blank => None,
        ChartCell::Number(v, f) => Some(crate::format::format_number(*v, f)),
        ChartCell::Text(t) => Some(t.clone()),
    }
}

fn refresh_text(data: &dyn ChartData, formula: &str) -> Option<String> {
    let cells = read_cells(data, formula)?;
    let parts: Vec<String> = cells.iter().filter_map(cell_text).collect();
    Some(parts.join(" "))
}

/// Refreshes every cached value of the chart from `data`: series names, categories, values,
/// bubble sizes and titles linked to cells. References that cannot be resolved keep their cache.
/// Returns whether anything changed.
pub fn refresh(cs: &mut ChartSpace, data: &dyn ChartData) -> bool {
    let mut changed = false;
    for g in &mut cs.chart.plot_area.groups {
        for s in &mut g.series {
            if let Some(SeriesText::Ref { formula, cache }) = &mut s.name
                && let Some(t) = refresh_text(data, formula)
                && cache.as_deref() != Some(t.as_str())
            {
                *cache = Some(t);
                changed = true;
            }
            for src in [&mut s.categories, &mut s.values, &mut s.bubble_sizes]
                .into_iter()
                .flatten()
            {
                changed |= refresh_source(data, src);
            }
        }
    }
    let mut titles: Vec<&mut Title> = cs.chart.title.iter_mut().collect();
    titles.extend(
        cs.chart
            .plot_area
            .axes
            .iter_mut()
            .filter_map(|a| a.title.as_mut()),
    );
    for t in titles {
        if let Some(TitleText::Ref { formula, cache }) = &mut t.text
            && let Some(v) = refresh_text(data, formula)
            && cache.as_deref() != Some(v.as_str())
        {
            *cache = Some(v);
            changed = true;
        }
    }
    changed
}

/// Which way series run in a source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesIn {
    /// Each column is a series (categories down the first column).
    Columns,
    /// Each row is a series (categories along the first row).
    Rows,
}

/// How a source range splits into names, categories and series, as Excel decides it.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeShape {
    /// The sheet.
    pub sheet: String,
    /// The whole range.
    pub range: Range,
    /// Series direction.
    pub series_in: SeriesIn,
    /// The first row holds labels (series names, or categories when series are in rows).
    pub header_row: bool,
    /// The first column holds labels.
    pub header_col: bool,
    /// The label column holds dates.
    pub date_labels: bool,
    /// The category/x values are numbers that are not dates (an X–Y candidate).
    pub numeric_labels: bool,
}

fn is_text(c: &ChartCell) -> bool {
    matches!(c, ChartCell::Text(_))
}

/// Analyses a selected range the way Excel's chart insertion does: a top row of text over numbers
/// is a header row; a left column of text (or dates, or a top-left blank) is a label column;
/// series run down the columns when the data has at least as many rows as columns.
#[must_use]
pub fn analyse_range(data: &dyn ChartData, sheet: &str, range: Range) -> Option<RangeShape> {
    let mut r = range;
    if let Some(last) = data.last_row(sheet) {
        r.r1 = r.r1.min(last.max(r.r0));
    }
    if let Some(last) = data.last_col(sheet) {
        r.c1 = r.c1.min(last.max(r.c0));
    }
    if r.cell_count() > MAX_REFERENCE_CELLS {
        return None;
    }
    let cell = |row: u32, col: u32| data.cell(sheet, row, col).unwrap_or(ChartCell::Blank);
    let top_left = cell(r.r0, r.c0);
    let first_row_text = r.height() > 1
        && (r.c0 + u32::from(r.width() > 1)..=r.c1)
            .all(|c| !matches!(cell(r.r0, c), ChartCell::Number(..)))
        && (r.c0..=r.c1).any(|c| is_text(&cell(r.r0, c)));
    let label_rows = (r.r0 + u32::from(first_row_text))..=r.r1;
    let first_col_text = r.width() > 1 && label_rows.clone().any(|row| is_text(&cell(row, r.c0)));
    let first_col_dates = r.width() > 1
        && !first_col_text
        && label_rows.clone().all(|row| match cell(row, r.c0) {
            ChartCell::Number(_, f) => crate::format::is_date_format(&f),
            _ => false,
        });
    let header_col = first_col_text
        || first_col_dates
        || (first_row_text && matches!(top_left, ChartCell::Blank) && r.width() > 1);
    let rows = r.height() - u32::from(first_row_text);
    let cols = r.width() - u32::from(header_col);
    let series_in = if rows >= cols {
        SeriesIn::Columns
    } else {
        SeriesIn::Rows
    };
    // A numeric first column under a text header (Temperature, Shrinkage…) with numeric columns
    // beside it is an X–Y candidate.
    let numeric_labels = !header_col
        && first_row_text
        && r.width() > 1
        && label_rows
            .clone()
            .all(|row| matches!(cell(row, r.c0), ChartCell::Number(..)));
    Some(RangeShape {
        sheet: sheet.to_owned(),
        range: r,
        series_in,
        header_row: first_row_text,
        header_col,
        date_labels: first_col_dates,
        numeric_labels,
    })
}

fn area(sheet: &str, r0: u32, c0: u32, r1: u32, c1: u32) -> String {
    Area {
        sheet: sheet.to_owned(),
        range: Range::new(r0, c0, r1, c1),
    }
    .to_formula()
}

/// Builds series (references only; call [`refresh`] to fill the caches) from an analysed range.
/// With `xy`, the label column (or the first column) becomes the x values of every series.
#[must_use]
pub fn series_from_shape(shape: &RangeShape, xy: bool) -> Vec<Series> {
    let r = shape.range;
    let s = shape.sheet.as_str();
    let mut out = Vec::new();
    let xy_first = xy && !shape.header_col;
    match shape.series_in {
        SeriesIn::Columns => {
            let d0 = r.r0 + u32::from(shape.header_row);
            if d0 > r.r1 {
                return out;
            }
            let label_col = (shape.header_col || xy_first).then_some(r.c0);
            let first = r.c0 + u32::from(label_col.is_some());
            for (i, col) in (first..=r.c1).enumerate() {
                out.push(new_series(
                    i,
                    shape.header_row.then(|| area(s, r.r0, col, r.r0, col)),
                    label_col.map(|c| area(s, d0, c, r.r1, c)),
                    area(s, d0, col, r.r1, col),
                    xy || shape.date_labels,
                ));
            }
        }
        SeriesIn::Rows => {
            let d0 = r.c0 + u32::from(shape.header_col);
            if d0 > r.c1 {
                return out;
            }
            let label_row = (shape.header_row || xy_first).then_some(r.r0);
            let first = r.r0 + u32::from(label_row.is_some());
            for (i, row) in (first..=r.r1).enumerate() {
                out.push(new_series(
                    i,
                    shape.header_col.then(|| area(s, row, r.c0, row, r.c0)),
                    label_row.map(|lr| area(s, lr, d0, lr, r.c1)),
                    area(s, row, d0, row, r.c1),
                    xy,
                ));
            }
        }
    }
    out
}

fn new_series(
    i: usize,
    name: Option<String>,
    labels: Option<String>,
    values: String,
    numeric_labels: bool,
) -> Series {
    let i = u32::try_from(i).unwrap_or(u32::MAX);
    Series {
        idx: i,
        order: i,
        name: name.map(|formula| SeriesText::Ref {
            formula,
            cache: None,
        }),
        categories: labels.map(|formula| {
            if numeric_labels {
                DataSource::Num(NumData {
                    formula: Some(formula),
                    has_cache: true,
                    ..NumData::default()
                })
            } else {
                DataSource::Str(StrData {
                    formula: Some(formula),
                    has_cache: true,
                    ..StrData::default()
                })
            }
        }),
        values: Some(DataSource::Num(NumData {
            formula: Some(values),
            has_cache: true,
            ..NumData::default()
        })),
        ..Series::default()
    }
}

/// The source range covering every series' references, when they all sit in one rectangle on one
/// sheet (what Select Data shows as "Chart data range").
#[must_use]
pub fn data_range(cs: &ChartSpace) -> Option<Area> {
    let mut sheet: Option<String> = None;
    let mut acc: Option<Range> = None;
    let mut add = |f: Option<&str>| -> Option<()> {
        let Some(f) = f else { return Some(()) };
        for a in parse_reference(f)? {
            if sheet.get_or_insert_with(|| a.sheet.clone()).as_str() != a.sheet {
                return None;
            }
            acc = Some(acc.map_or(a.range, |r| r.union(&a.range)));
        }
        Some(())
    };
    for g in &cs.chart.plot_area.groups {
        for s in &g.series {
            if let Some(SeriesText::Ref { formula, .. }) = &s.name {
                add(Some(formula))?;
            }
            add(s.categories.as_ref().and_then(DataSource::formula))?;
            add(s.values.as_ref().and_then(DataSource::formula))?;
            add(s.bubble_sizes.as_ref().and_then(DataSource::formula))?;
        }
    }
    Some(Area {
        sheet: sheet?,
        range: acc?,
    })
}

/// The letters of a column (re-exported for the UI's Select Data dialog).
#[must_use]
pub fn column_letters(col: u32) -> String {
    column_to_letters(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_sheet::CellValue;

    pub(crate) fn sample() -> Workbook {
        let mut wb = Workbook::new();
        let id = wb.add_sheet(Some("Budget 2026"), None, None).unwrap();
        let rows: [(&str, [f64; 3]); 4] = [
            ("Q1", [730.0, 1180.0, 0.0]),
            ("Q2", [505.0, 640.0, 0.0]),
            ("Q3", [745.0, 1240.0, 120.0]),
            ("Q4", [650.0, 1320.0, 380.0]),
        ];
        for (c, h) in ["Quarter", "Materials", "Firing", "Events"]
            .iter()
            .enumerate()
        {
            wb.load_value(id, 0, c as u32, CellValue::text(*h)).unwrap();
        }
        for (r, (q, vals)) in rows.iter().enumerate() {
            let r = r as u32 + 1;
            wb.load_value(id, r, 0, CellValue::text(*q)).unwrap();
            for (c, v) in vals.iter().enumerate() {
                wb.load_value(id, r, c as u32 + 1, CellValue::Number(*v))
                    .unwrap();
            }
        }
        wb
    }

    #[test]
    fn parses_references() {
        let a = parse_reference("'Budget 2026'!$B$2:$B$5").unwrap();
        assert_eq!(a[0].sheet, "Budget 2026");
        assert_eq!(a[0].range, Range::new(1, 1, 4, 1));
        let u = parse_reference("(Sheet1!$A$1,Sheet1!$A$3:$A$4)").unwrap();
        assert_eq!(u.len(), 2);
        let q = parse_reference("'It''s'!A1").unwrap();
        assert_eq!(q[0].sheet, "It's");
        assert!(parse_reference("MyName").is_none());
        assert!(parse_reference("[1]Sheet1!A1").is_none());
        assert!(parse_reference("Sheet1!").is_none());
        assert_eq!(a[0].to_formula(), "'Budget 2026'!$B$2:$B$5");
        assert_eq!(quote_sheet("Sheet1"), "Sheet1");
        assert_eq!(quote_sheet("A1"), "'A1'");
    }

    #[test]
    fn analyses_a_table_and_builds_series() {
        let wb = sample();
        let shape = analyse_range(&wb, "Budget 2026", Range::new(0, 0, 4, 3)).unwrap();
        assert!(shape.header_row && shape.header_col);
        assert_eq!(shape.series_in, SeriesIn::Columns);
        let series = series_from_shape(&shape, false);
        assert_eq!(series.len(), 3);
        let mut cs = ChartSpace::default();
        cs.chart.plot_area.groups.push(ChartGroup {
            kind: GroupKind::Bar {
                dir: BarDir::Column,
                grouping: Some(Grouping::Clustered),
                gap_width: None,
                overlap: None,
                three_d: false,
            },
            vary_colors: None,
            series,
            data_labels: None,
            axis_ids: vec![1, 2],
            extras: Extras::default(),
        });
        assert!(refresh(&mut cs, &wb));
        let s = &cs.chart.plot_area.groups[0].series[1];
        assert_eq!(s.name.as_ref().unwrap().text(), Some("Firing"));
        assert_eq!(s.values.as_ref().unwrap().number(3), Some(1320.0));
        assert_eq!(
            s.categories.as_ref().unwrap().label(0).as_deref(),
            Some("Q1")
        );
        assert!(!refresh(&mut cs, &wb), "a second refresh changes nothing");
        let dr = data_range(&cs).unwrap();
        assert_eq!(dr.to_formula(), "'Budget 2026'!$A$1:$D$5");
    }

    #[test]
    fn wide_ranges_put_series_in_rows() {
        let wb = sample();
        // B1:D3 has 3 columns and 2 data rows under a header: series in rows.
        let shape = analyse_range(&wb, "Budget 2026", Range::new(0, 1, 2, 3)).unwrap();
        assert_eq!(shape.series_in, SeriesIn::Rows);
        assert!(shape.header_row && !shape.header_col);
        assert_eq!(series_from_shape(&shape, false).len(), 2);
    }
}
