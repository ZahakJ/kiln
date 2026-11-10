// SPDX-License-Identifier: AGPL-3.0-or-later
//! The report a PivotTable writes into its sheet, laid out as Excel lays it out:
//!
//! - **Filters** above the table: one row per filter field (`Region` | `(All)`), then a gap row.
//! - **Headers**: with column fields, a caption row (`Sum of Amount` | `Column Labels`), one row
//!   per column level with the item labels, and the row-labels caption row; without them one
//!   row (`Row Labels` | `Sum of Amount` | …).
//! - **Body**: one line per row item, group, subtotal and the grand total, by layout — Compact
//!   (one indented label column, subtotals on the group's line), Outline (one column per row
//!   field) or Tabular (items beside their group, subtotals below).
//!
//! [`PivotOutput`] keeps what the lines are, for the PivotTable style, the field buttons and
//! drill-through.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::engine::{Summary, Tree};
use super::{AxisItem, PivotFunction, PivotLayout, PivotSubtotal, PivotTable, SubtotalPlace};
use crate::address::{CellPos, MAX_COLS, MAX_ROWS, Range};
use crate::value::CellValue;

/// What a line of the report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotLineKind {
    /// An item at the axis's last level (or the only line of an axis without fields).
    #[default]
    Item,
    /// A group above its items (its line shows the subtotal when subtotals are at the top).
    Group,
    /// A subtotal below a group.
    Subtotal,
    /// The grand total.
    Grand,
    /// An empty line after a group (Insert Blank Line after Each Item).
    Blank,
}

/// One row or column line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PivotLine {
    /// What it is.
    pub kind: PivotLineKind,
    /// The items from the first field level down to the line's group or item (cache item
    /// indices); empty for the grand total.
    pub path: Vec<u32>,
    /// The axis position its label belongs to (0 = outermost).
    pub level: u8,
    /// The value field it shows, when Σ Values is on this axis.
    #[serde(default)]
    pub value: Option<u16>,
    /// A custom subtotal's function.
    #[serde(default)]
    pub function: Option<PivotFunction>,
    /// Whether the line shows values (a group line only does with subtotals at the top).
    #[serde(default = "yes")]
    pub values: bool,
}

fn yes() -> bool {
    true
}

impl Default for PivotLine {
    /// An item line that shows values.
    fn default() -> PivotLine {
        PivotLine {
            kind: PivotLineKind::Item,
            path: Vec::new(),
            level: 0,
            value: None,
            function: None,
            values: true,
        }
    }
}

/// The report's shape as last written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PivotOutput {
    /// The table (headers and body); the Filters area is apart.
    pub range: Option<Range>,
    /// The Filters area.
    #[serde(default)]
    pub pages: Option<Range>,
    /// Header rows above the first body row.
    pub header_rows: u32,
    /// Label columns before the first value column.
    pub label_cols: u32,
    /// Body rows.
    pub rows: Vec<PivotLine>,
    /// Value columns.
    pub cols: Vec<PivotLine>,
    /// The report has no fields yet: the range holds the placeholder Excel shows.
    #[serde(default)]
    pub empty: bool,
    /// Rows of the header that hold column labels (the column caption row is above them).
    #[serde(default)]
    pub col_label_rows: u32,
}

impl PivotOutput {
    /// The first body row.
    pub fn first_data_row(&self) -> Option<u32> {
        Some(self.range?.r0 + self.header_rows)
    }

    /// The first value column.
    pub fn first_data_col(&self) -> Option<u32> {
        Some(self.range?.c0 + self.label_cols)
    }

    /// The body line and value column of a cell (`None` in headers or outside).
    pub fn lines_at(&self, row: u32, col: u32) -> Option<(Option<&PivotLine>, Option<&PivotLine>)> {
        let r = self.range?;
        if !r.contains(row, col) || row < r.r0 + self.header_rows {
            return None;
        }
        let line = self.rows.get((row - r.r0 - self.header_rows) as usize);
        let column = (col >= r.c0 + self.label_cols)
            .then(|| self.cols.get((col - r.c0 - self.label_cols) as usize))
            .flatten();
        Some((line, column))
    }
}

/// One cell of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportCell {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Value.
    pub value: CellValue,
    /// Number format (`None`: General).
    pub num_fmt: Option<Arc<str>>,
    /// Indent in characters (compact labels).
    pub indent: u8,
    /// A label cell (left-aligned, whatever its value).
    pub label: bool,
}

/// A rendered report.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotReport {
    /// The cells, row by row.
    pub cells: Vec<ReportCell>,
    /// The shape.
    pub output: PivotOutput,
}

/// The default captions.
const ROW_LABELS: &str = "Row Labels";
const COL_LABELS: &str = "Column Labels";
const GRAND: &str = "Grand Total";
const BLANK: &str = "(blank)";
const VALUES: &str = "Values";

/// Rows and columns the placeholder of an empty report covers (Excel's: 3 × 18).
const PLACEHOLDER: (u32, u32) = (18, 3);

fn axis_fields(axis: &[AxisItem]) -> usize {
    axis.iter()
        .filter(|a| matches!(a, AxisItem::Field(_)))
        .count()
}

struct AxisLines<'s> {
    table: &'s PivotTable,
    tree: &'s Tree,
    items: &'s [AxisItem],
    rows: bool,
    out: Vec<(PivotLine, u32)>,
}

impl AxisLines<'_> {
    fn subtotals(&self, field: usize) -> Vec<Option<PivotFunction>> {
        match &self.table.fields[field].subtotal {
            PivotSubtotal::None => Vec::new(),
            PivotSubtotal::Automatic => vec![None],
            PivotSubtotal::Custom(fs) => fs.iter().map(|f| Some(*f)).collect(),
        }
    }

    /// Lines for `node` (at axis position `k`, items above it fixed), Σ Values at `value`.
    fn emit(&mut self, node: u32, k: usize, value: Option<u16>) {
        let Some(item) = self.items.get(k) else {
            return;
        };
        let last = k + 1 == self.items.len();
        let level = u8::try_from(k).unwrap_or(u8::MAX);
        let opts = &self.table.options;
        match *item {
            AxisItem::Values => {
                for v in 0..self.table.values.len() {
                    let v = u16::try_from(v).ok();
                    let path = self.tree.path(node);
                    if last {
                        self.out.push((
                            PivotLine {
                                kind: PivotLineKind::Item,
                                path,
                                level,
                                value: v,
                                function: None,
                                values: true,
                            },
                            node,
                        ));
                    } else {
                        if self.rows {
                            self.out.push((
                                PivotLine {
                                    kind: PivotLineKind::Group,
                                    path,
                                    level,
                                    value: v,
                                    function: None,
                                    values: false,
                                },
                                node,
                            ));
                        }
                        self.emit(node, k + 1, v);
                    }
                }
            }
            AxisItem::Field(f) => {
                let kids = self.tree.nodes[node as usize].children.clone();
                // A field has subtotals only with another field below it (Σ Values does not
                // count).
                let inner = self.items[k + 1..]
                    .iter()
                    .any(|a| matches!(a, AxisItem::Field(_)));
                let subs = if inner { self.subtotals(f) } else { Vec::new() };
                for child in kids {
                    let path = self.tree.path(child);
                    if last {
                        self.out.push((
                            PivotLine {
                                kind: PivotLineKind::Item,
                                path,
                                level,
                                value,
                                function: None,
                                values: true,
                            },
                            child,
                        ));
                        continue;
                    }
                    // Σ Values below the field: one subtotal per value, at the bottom.
                    let values_below =
                        value.is_none() && self.items[k + 1..].contains(&AxisItem::Values);
                    let top = self.rows
                        && opts.layout != PivotLayout::Tabular
                        && opts.subtotals == SubtotalPlace::Top
                        && subs == [None]
                        && !values_below;
                    if self.rows && opts.layout != PivotLayout::Tabular {
                        self.out.push((
                            PivotLine {
                                kind: PivotLineKind::Group,
                                path: path.clone(),
                                level,
                                value,
                                function: None,
                                values: top,
                            },
                            child,
                        ));
                    }
                    self.emit(child, k + 1, value);
                    if !top && opts.subtotals != SubtotalPlace::Off {
                        let per_value: Vec<Option<u16>> = if values_below {
                            (0..self.table.values.len())
                                .map(|v| u16::try_from(v).ok())
                                .collect()
                        } else {
                            vec![value]
                        };
                        for s in &subs {
                            for v in &per_value {
                                self.out.push((
                                    PivotLine {
                                        kind: PivotLineKind::Subtotal,
                                        path: path.clone(),
                                        level,
                                        value: *v,
                                        function: *s,
                                        values: true,
                                    },
                                    child,
                                ));
                            }
                        }
                    }
                    if self.rows && opts.blank_rows {
                        self.out.push((
                            PivotLine {
                                kind: PivotLineKind::Blank,
                                path: path.clone(),
                                level,
                                value,
                                function: None,
                                values: false,
                            },
                            child,
                        ));
                    }
                }
            }
        }
    }
}

/// The lines of one axis with their tree nodes.
fn lines(table: &PivotTable, tree: &Tree, items: &[AxisItem], rows: bool) -> Vec<(PivotLine, u32)> {
    let opts = &table.options;
    let mut a = AxisLines {
        table,
        tree,
        items,
        rows,
        out: Vec::new(),
    };
    if items.is_empty() {
        // One line for the whole data (the column of values, or the row of totals).
        return vec![(PivotLine::default(), 0)];
    }
    a.emit(0, 0, None);
    let mut out = a.out;
    // A blank line after the last group is not drawn.
    if out.last().is_some_and(|l| l.0.kind == PivotLineKind::Blank) {
        out.pop();
    }
    let grand = if rows {
        opts.col_totals
    } else {
        opts.row_totals
    };
    if grand && axis_fields(items) > 0 {
        if items.contains(&AxisItem::Values) {
            for v in 0..table.values.len() {
                out.push((
                    PivotLine {
                        kind: PivotLineKind::Grand,
                        value: u16::try_from(v).ok(),
                        ..PivotLine::default()
                    },
                    0,
                ));
            }
        } else {
            out.push((
                PivotLine {
                    kind: PivotLineKind::Grand,
                    ..PivotLine::default()
                },
                0,
            ));
        }
    }
    out
}

/// The label of an item.
fn item_label(table: &PivotTable, field: usize, item: u32) -> CellValue {
    match table.cache.fields[field].items.get(item as usize) {
        Some(v) if !v.is_empty_text_or_blank() => v.clone(),
        _ => CellValue::text(BLANK),
    }
}

fn item_text(table: &PivotTable, field: usize, item: u32) -> String {
    let v = item_label(table, field, item);
    match (&v, &table.cache.fields[field].num_fmt) {
        (CellValue::Number(_), Some(code)) => crate::format::format_value(&v, code, false).text,
        _ => v.display_general(),
    }
}

/// The field of the axis item at a position.
fn field_at(items: &[AxisItem], k: usize) -> Option<usize> {
    match items.get(k) {
        Some(AxisItem::Field(f)) => Some(*f),
        _ => None,
    }
}

/// The field level (Σ Values left out) of the axis item at a position.
fn field_level(items: &[AxisItem], k: usize) -> usize {
    items[..k]
        .iter()
        .filter(|a| matches!(a, AxisItem::Field(_)))
        .count()
}

/// What tells a line's group at axis position `k` apart from its neighbours': the items down to
/// that position, and the value when Σ Values is at or above it.
fn group_key(items: &[AxisItem], line: &PivotLine, k: usize) -> Vec<u32> {
    let n = field_level(items, k + 1).min(line.path.len());
    let mut key = line.path[..n].to_vec();
    if let Some(v) = line.value
        && items[..=k].contains(&AxisItem::Values)
    {
        key.push(u32::from(v) | 0x8000_0000);
    }
    key
}

/// The label of a line at an axis position `k` (its own, or an ancestor's for repeated and
/// tabular labels).
fn label_of(
    table: &PivotTable,
    items: &[AxisItem],
    line: &PivotLine,
    k: usize,
) -> Option<CellValue> {
    match items.get(k)? {
        AxisItem::Values => {
            let v = line.value?;
            Some(CellValue::text(
                table.values.get(usize::from(v))?.name.clone(),
            ))
        }
        AxisItem::Field(f) => {
            let lvl = field_level(items, k);
            let item = *line.path.get(lvl)?;
            Some(item_label(table, *f, item))
        }
    }
}

fn subtotal_label(table: &PivotTable, items: &[AxisItem], line: &PivotLine) -> String {
    let k = usize::from(line.level);
    let Some(f) = field_at(items, k) else {
        return String::new();
    };
    let lvl = field_level(items, k);
    let item = line.path.get(lvl).copied().unwrap_or(0);
    let name = item_text(table, f, item);
    let what = match line.function {
        Some(func) => func.caption().to_owned(),
        None => "Total".to_owned(),
    };
    match line.value.and_then(|v| table.values.get(usize::from(v))) {
        Some(v) if items.contains(&AxisItem::Values) && line.function.is_none() => {
            format!("{name} {}", v.name)
        }
        _ => format!("{name} {what}"),
    }
}

fn grand_label(table: &PivotTable, line: &PivotLine) -> String {
    match line.value.and_then(|v| table.values.get(usize::from(v))) {
        Some(v) => format!("Total {}", v.name),
        None => GRAND.to_owned(),
    }
}

/// Renders a PivotTable at its position. `None` when the report would leave the sheet.
pub fn render(table: &PivotTable) -> Option<PivotReport> {
    let opts = &table.options;
    let at = table.at;
    let mut cells: Vec<ReportCell> = Vec::new();
    let mut put = |row: u32,
                   col: u32,
                   value: CellValue,
                   num_fmt: Option<Arc<str>>,
                   indent: u8,
                   label: bool| {
        if value.is_blank() {
            return;
        }
        cells.push(ReportCell {
            row,
            col,
            value,
            num_fmt,
            indent,
            label,
        });
    };
    // Filters.
    let pages = if table.pages.is_empty() {
        None
    } else {
        let n = u32::try_from(table.pages.len()).ok()?;
        let top = at.row.checked_sub(n + 1)?;
        for (i, p) in table.pages.iter().enumerate() {
            let row = top + u32::try_from(i).ok()?;
            let field = table.fields.get(p.field)?;
            put(
                row,
                at.col,
                CellValue::text(field.shown_name()),
                None,
                0,
                true,
            );
            let cf = table.cache.fields.get(p.field)?;
            let shown = match &p.item {
                Some(v) if v.is_empty_text_or_blank() => CellValue::text(BLANK),
                Some(v) => cf
                    .find(v)
                    .and_then(|i| cf.items.get(i as usize).cloned())
                    .unwrap_or_else(|| v.clone()),
                None if !field.hidden.is_empty() => CellValue::text("(Multiple Items)"),
                None => CellValue::text("(All)"),
            };
            put(
                row,
                at.col + 1,
                shown,
                table
                    .cache
                    .fields
                    .get(p.field)?
                    .num_fmt
                    .as_deref()
                    .map(Arc::from),
                0,
                true,
            );
        }
        Some(Range::new(top, at.col, top + n - 1, at.col + 1))
    };
    if table.rows.is_empty() && table.cols.is_empty() && table.values.is_empty() {
        let (h, w) = PLACEHOLDER;
        if at.row + h > MAX_ROWS || at.col + w > MAX_COLS {
            return None;
        }
        return Some(PivotReport {
            cells,
            output: PivotOutput {
                range: Some(Range::new(at.row, at.col, at.row + h - 1, at.col + w - 1)),
                pages,
                empty: true,
                ..PivotOutput::default()
            },
        });
    }
    let summary = Summary::build(table);
    let row_lines = lines(table, &summary.rows, &table.rows, true);
    // Without values the column items still head their (empty) columns, as in Excel.
    let col_lines = if table.values.is_empty() && axis_fields(&table.cols) == 0 {
        Vec::new()
    } else {
        lines(table, &summary.cols, &table.cols, false)
    };
    let row_fields = !table.rows.is_empty();
    let col_fields = axis_fields(&table.cols) > 0;
    let compact = opts.layout == PivotLayout::Compact;
    let label_cols: u32 = if !row_fields {
        u32::from(col_fields)
    } else if compact {
        1
    } else {
        u32::try_from(table.rows.len()).ok()?
    };
    let col_label_rows: u32 = if col_fields {
        u32::try_from(table.cols.len()).ok()?
    } else {
        0
    };
    let header_rows: u32 = if col_fields { 1 + col_label_rows } else { 1 };
    let body_rows = u32::try_from(row_lines.len()).ok()?;
    let data_cols = u32::try_from(col_lines.len()).ok()?;
    let width = (label_cols + data_cols).max(1);
    let height = header_rows
        + if row_fields || !table.values.is_empty() {
            body_rows
        } else {
            0
        };
    if at.row + height > MAX_ROWS || at.col + width > MAX_COLS {
        return None;
    }
    let range = Range::new(at.row, at.col, at.row + height - 1, at.col + width - 1);
    let data_r0 = at.row + header_rows;
    let data_c0 = at.col + label_cols;
    let single_value = table.values.len() == 1;
    let row_caption = opts
        .row_caption
        .clone()
        .unwrap_or_else(|| ROW_LABELS.to_owned());
    let col_caption = opts
        .col_caption
        .clone()
        .unwrap_or_else(|| COL_LABELS.to_owned());

    // Header: the caption row and the column labels.
    if col_fields {
        if single_value && !table.cols.contains(&AxisItem::Values) {
            put(
                at.row,
                at.col,
                CellValue::text(table.values[0].name.clone()),
                None,
                0,
                true,
            );
        }
        let caption = if compact {
            col_caption.clone()
        } else {
            field_at(&table.cols, 0)
                .map(|f| table.fields[f].shown_name().to_owned())
                .unwrap_or_else(|| VALUES.to_owned())
        };
        put(at.row, data_c0, CellValue::text(caption), None, 0, true);
        let mut prev: Vec<Option<Vec<u32>>> = vec![None; table.cols.len()];
        for (j, (line, _)) in col_lines.iter().enumerate() {
            let col = data_c0 + u32::try_from(j).ok()?;
            match line.kind {
                PivotLineKind::Grand => {
                    put(
                        at.row + 1,
                        col,
                        CellValue::text(grand_label(table, line)),
                        None,
                        0,
                        true,
                    );
                }
                PivotLineKind::Subtotal => {
                    let k = usize::from(line.level);
                    put(
                        at.row + 1 + u32::try_from(k).ok()?,
                        col,
                        CellValue::text(subtotal_label(table, &table.cols, line)),
                        None,
                        0,
                        true,
                    );
                    for p in prev.iter_mut().skip(k) {
                        *p = None;
                    }
                }
                _ => {
                    for k in 0..table.cols.len() {
                        if field_at(&table.cols, k).is_some()
                            && field_level(&table.cols, k) >= line.path.len()
                        {
                            continue;
                        }
                        // A label shows where its group starts.
                        let key = Some(group_key(&table.cols, line, k));
                        if key != prev[k] {
                            if let Some(v) = label_of(table, &table.cols, line, k) {
                                let fmt = field_at(&table.cols, k)
                                    .and_then(|f| table.cache.fields[f].num_fmt.as_deref())
                                    .map(Arc::from);
                                put(at.row + 1 + u32::try_from(k).ok()?, col, v, fmt, 0, true);
                            }
                            for p in prev.iter_mut().skip(k + 1) {
                                *p = None;
                            }
                            prev[k] = key;
                        }
                    }
                }
            }
        }
    } else {
        // One header row: value names over the value columns.
        for (j, (line, _)) in col_lines.iter().enumerate() {
            let col = data_c0 + u32::try_from(j).ok()?;
            let v = usize::from(line.value.unwrap_or(0));
            if let Some(spec) = table.values.get(v)
                && !table.rows.contains(&AxisItem::Values)
            {
                put(
                    at.row,
                    col,
                    CellValue::text(spec.name.clone()),
                    None,
                    0,
                    true,
                );
            } else if table.rows.contains(&AxisItem::Values) && j == 0 {
                put(
                    at.row,
                    col,
                    CellValue::text(if single_value {
                        table.values[0].name.clone()
                    } else {
                        "Data".to_owned()
                    }),
                    None,
                    0,
                    true,
                );
            }
        }
    }
    // The row-labels captions on the last header row.
    let caption_row = at.row + header_rows - 1;
    if row_fields {
        if compact {
            put(
                caption_row,
                at.col,
                CellValue::text(row_caption),
                None,
                0,
                true,
            );
        } else {
            for (k, item) in table.rows.iter().enumerate() {
                let name = match item {
                    AxisItem::Field(f) => table.fields[*f].shown_name().to_owned(),
                    AxisItem::Values => VALUES.to_owned(),
                };
                put(
                    caption_row,
                    at.col + u32::try_from(k).ok()?,
                    CellValue::text(name),
                    None,
                    0,
                    true,
                );
            }
        }
    }

    // Body.
    let tabular = opts.layout == PivotLayout::Tabular;
    let mut shown_above: Vec<Option<Vec<u32>>> = vec![None; table.rows.len()];
    for (i, (line, rnode)) in row_lines.iter().enumerate() {
        let row = data_r0 + u32::try_from(i).ok()?;
        let k = usize::from(line.level);
        // Labels.
        if row_fields {
            match line.kind {
                PivotLineKind::Grand => {
                    put(
                        row,
                        at.col,
                        CellValue::text(grand_label(table, line)),
                        None,
                        0,
                        true,
                    );
                }
                PivotLineKind::Subtotal => {
                    let col = if compact {
                        at.col
                    } else {
                        at.col + u32::try_from(k).ok()?
                    };
                    let indent = if compact {
                        opts.indent.saturating_mul(u8::try_from(k).unwrap_or(0))
                    } else {
                        0
                    };
                    put(
                        row,
                        col,
                        CellValue::text(subtotal_label(table, &table.rows, line)),
                        None,
                        indent,
                        true,
                    );
                }
                PivotLineKind::Blank => {}
                PivotLineKind::Item | PivotLineKind::Group => {
                    if compact {
                        if let Some(v) = label_of(table, &table.rows, line, k) {
                            let fmt = field_at(&table.rows, k)
                                .and_then(|f| table.cache.fields[f].num_fmt.as_deref())
                                .map(Arc::from);
                            let indent = opts.indent.saturating_mul(u8::try_from(k).unwrap_or(0));
                            put(row, at.col, v, fmt, indent, true);
                        }
                    } else {
                        // Outline: the line's own label (groups have their own lines);
                        // tabular: its ancestors' too where a group starts; repeated labels:
                        // on every line.
                        for (j, above) in shown_above.iter_mut().enumerate().take(k + 1) {
                            let key = group_key(&table.rows, line, j);
                            let show = j == k
                                || opts.repeat_labels
                                || (tabular && above.as_ref() != Some(&key));
                            if show && let Some(v) = label_of(table, &table.rows, line, j) {
                                let fmt = field_at(&table.rows, j)
                                    .and_then(|f| table.cache.fields[f].num_fmt.as_deref())
                                    .map(Arc::from);
                                put(row, at.col + u32::try_from(j).ok()?, v, fmt, 0, true);
                            }
                            *above = Some(key);
                        }
                        for s in shown_above.iter_mut().skip(k + 1) {
                            *s = None;
                        }
                    }
                }
            }
        }
        if !line.values || table.values.is_empty() {
            continue;
        }
        for (j, (cline, cnode)) in col_lines.iter().enumerate() {
            if !cline.values {
                continue;
            }
            let col = data_c0 + u32::try_from(j).ok()?;
            let v = usize::from(line.value.or(cline.value).unwrap_or(0));
            let Some(spec) = table.values.get(v) else {
                continue;
            };
            let function = line.function.or(cline.function).unwrap_or(spec.function);
            let mut value = summary.cell(*rnode, *cnode, v, function);
            if value.is_blank()
                && let Some(t) = &opts.empty_text
            {
                value = CellValue::text(t.clone());
            }
            let fmt = spec
                .num_fmt
                .clone()
                .or_else(|| spec.show_as.is_percent().then(|| "0.00%".to_owned()))
                .map(Arc::from);
            put(row, col, value, fmt, 0, false);
        }
    }
    Some(PivotReport {
        cells,
        output: PivotOutput {
            range: Some(range),
            pages,
            header_rows,
            label_cols,
            rows: row_lines.into_iter().map(|l| l.0).collect(),
            cols: col_lines.into_iter().map(|l| l.0).collect(),
            empty: false,
            col_label_rows,
        },
    })
}

/// The source records behind a value cell of the report (drill-through, Show Details), in
/// source order; `None` when the cell is not a value of the report.
pub fn drill(table: &PivotTable, row: u32, col: u32) -> Option<Vec<usize>> {
    let (line, column) = table.output.lines_at(row, col)?;
    let line = line?;
    if matches!(line.kind, PivotLineKind::Blank)
        || (!line.values && line.kind == PivotLineKind::Group)
    {
        return None;
    }
    let column = column.cloned().unwrap_or_default();
    let summary = Summary::build(table);
    let r = summary.rows.find(&line.path)?;
    let c = summary.cols.find(&column.path)?;
    Some(summary.records(r, c))
}

/// The position of a field button (`Row Labels`, `Column Labels` or a filter's value) at a
/// cell, with the axis it filters.
pub fn button_at(table: &PivotTable, row: u32, col: u32) -> Option<super::PivotArea> {
    let out = &table.output;
    let r = out.range?;
    if out.empty {
        return None;
    }
    if let Some(p) = out.pages
        && p.contains(row, col)
        && col == p.c0 + 1
    {
        return Some(super::PivotArea::Filters);
    }
    let caption_row = r.r0 + out.header_rows - 1;
    if !table.rows.is_empty()
        && row == caption_row
        && col >= r.c0
        && col < r.c0 + out.label_cols.max(1)
    {
        return Some(super::PivotArea::Rows);
    }
    if out.col_label_rows > 0 && row == r.r0 && col == r.c0 + out.label_cols {
        return Some(super::PivotArea::Columns);
    }
    None
}

/// The top-left of the report's table for a report whose Filters area starts at `top`.
pub fn table_origin(top: CellPos, pages: usize) -> CellPos {
    let n = u32::try_from(pages).unwrap_or(0);
    CellPos {
        row: top.row + if n > 0 { n + 1 } else { 0 },
        col: top.col,
    }
}
