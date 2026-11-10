// SPDX-License-Identifier: AGPL-3.0-or-later
//! Format as Table and Insert ▸ Table (Ctrl+T), the Table Design tab (styles gallery, Table
//! Style Options, total row functions, name, Resize Table, Convert to Range), Home ▸ Cell
//! Styles and Format Painter.
//!
//! A table gets a header row (the region's first row, or `Column1`… in a row inserted above),
//! filter buttons and banded rows; turning the total row on writes `SUBTOTAL(109,Table1[Col])`
//! formulas that skip filtered rows. Converting to a range keeps the table's look as cell
//! formats and turns structured references into A1 references, as Excel does.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_features::{
    GridCellStyleInfo, GridTableColumn, GridTableInfo, GridTableOptions, GridTableStyle, GridTotals,
};
use kiln_protocol::commands::{GridCommand, GridRange};
use kiln_sheet::commands::{CellInput, Content, MetaField, PasteOp, PasteWhat, TablePatch};
use kiln_sheet::meta::{Table, TableColumn, TableFilter, TableStyleInfo, TotalsFunction};
use kiln_sheet::styles::{
    BorderPatch, BorderSide, BorderStyle, Dxf, Fill, FontPatch, Patch, StylePatch,
};
use kiln_sheet::table_style::{TableStyleElement, builtin_element};
use kiln_sheet::{CellPos, CellValue, Color, Command, Range, SheetId};

use super::super::GridDoc;
use super::super::format::hex;
use crate::model::Change;

fn totals(t: GridTotals) -> TotalsFunction {
    match t {
        GridTotals::None => TotalsFunction::None,
        GridTotals::Sum => TotalsFunction::Sum,
        GridTotals::Average => TotalsFunction::Average,
        GridTotals::Count => TotalsFunction::Count,
        GridTotals::CountNumbers => TotalsFunction::CountNums,
        GridTotals::Max => TotalsFunction::Max,
        GridTotals::Min => TotalsFunction::Min,
        GridTotals::StdDev => TotalsFunction::StdDev,
        GridTotals::Var => TotalsFunction::Var,
    }
}

fn grid_totals(t: TotalsFunction) -> GridTotals {
    match t {
        TotalsFunction::Sum => GridTotals::Sum,
        TotalsFunction::Average => GridTotals::Average,
        TotalsFunction::Count => GridTotals::Count,
        TotalsFunction::CountNums => GridTotals::CountNumbers,
        TotalsFunction::Max => GridTotals::Max,
        TotalsFunction::Min => GridTotals::Min,
        TotalsFunction::StdDev => GridTotals::StdDev,
        TotalsFunction::Var => GridTotals::Var,
        TotalsFunction::None | TotalsFunction::Custom => GridTotals::None,
    }
}

/// A column name inside a structured reference (`'` escapes `[`, `]`, `#` and `'`).
pub(crate) fn escape_column(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// The total row formula of a column.
fn totals_formula(table: &str, column: &str, f: TotalsFunction) -> Option<String> {
    f.subtotal_number()
        .map(|n| format!("SUBTOTAL({n},{table}[{}])", escape_column(column)))
}

/// Whether a table name is valid (Excel's rules for names).
fn valid_table_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_' || first == '\\')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && name.len() <= 255
        && kiln_sheet::address::parse_cell_a1(name).is_none()
        && !matches!(name.to_ascii_uppercase().as_str(), "R" | "C")
}

/// Unique, non-empty column names from header texts.
fn column_names(headers: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, h) in headers.into_iter().enumerate() {
        let base = if h.trim().is_empty() {
            format!("Column{}", i + 1)
        } else {
            h.replace(['\n', '\r'], " ")
        };
        let mut name = base.clone();
        let mut n = 2;
        while out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            name = format!("{base}{n}");
            n += 1;
        }
        out.push(name);
    }
    out
}

/// A differential format as a style patch (Convert to Range keeps the table's look).
fn dxf_patch(d: &Dxf) -> StylePatch {
    let side = |s: &Option<BorderSide>| match s {
        Some(b) => Patch::Set(*b),
        None => Patch::Keep,
    };
    StylePatch {
        num_fmt: d.num_fmt.as_ref().map(ToString::to_string),
        font: d.font.clone(),
        fill: d.fill.clone().map_or(Patch::Keep, Patch::Set),
        border: d.border.as_ref().map(|b| BorderPatch {
            left: side(&b.left),
            right: side(&b.right),
            top: side(&b.top),
            bottom: side(&b.bottom),
            ..BorderPatch::default()
        }),
        ..StylePatch::default()
    }
}

/// One built-in cell style: name, gallery group and its format.
struct CellStyleDef {
    name: &'static str,
    group: &'static str,
    fill: Option<Color>,
    font: Option<Color>,
    bold: bool,
    italic: bool,
    size: Option<f64>,
    bottom: Option<(BorderStyle, Color)>,
    top: Option<(BorderStyle, Color)>,
    outline: Option<(BorderStyle, Color)>,
    num_fmt: Option<&'static str>,
}

const fn style(name: &'static str, group: &'static str) -> CellStyleDef {
    CellStyleDef {
        name,
        group,
        fill: None,
        font: None,
        bold: false,
        italic: false,
        size: None,
        bottom: None,
        top: None,
        outline: None,
        num_fmt: None,
    }
}

const GBN: &str = "Good, Bad and Neutral";
const DATA: &str = "Data and Model";
const TITLES: &str = "Titles and Headings";
const THEMED: &str = "Themed Cell Styles";
const NUMBER: &str = "Number Format";

/// Excel's built-in cell styles (Office theme colours through the workbook's theme).
fn builtin_styles() -> Vec<CellStyleDef> {
    let rgb = Color::rgb;
    let accent = |i: u8, tint: f64| Color::theme(3 + i, tint);
    let mut v = vec![
        style("Normal", GBN),
        CellStyleDef {
            fill: Some(rgb(0xFF_C7CE)),
            font: Some(rgb(0x9C_0006)),
            ..style("Bad", GBN)
        },
        CellStyleDef {
            fill: Some(rgb(0xC6_EFCE)),
            font: Some(rgb(0x00_6100)),
            ..style("Good", GBN)
        },
        CellStyleDef {
            fill: Some(rgb(0xFF_EB9C)),
            font: Some(rgb(0x9C_5700)),
            ..style("Neutral", GBN)
        },
        CellStyleDef {
            fill: Some(rgb(0xF2_F2F2)),
            font: Some(rgb(0xFA_7D00)),
            bold: true,
            outline: Some((BorderStyle::Thin, rgb(0x7F_7F7F))),
            ..style("Calculation", DATA)
        },
        CellStyleDef {
            fill: Some(rgb(0xA5_A5A5)),
            font: Some(rgb(0xFF_FFFF)),
            bold: true,
            outline: Some((BorderStyle::Double, rgb(0x3F_3F3F))),
            ..style("Check Cell", DATA)
        },
        CellStyleDef {
            font: Some(rgb(0x7F_7F7F)),
            italic: true,
            ..style("Explanatory Text", DATA)
        },
        CellStyleDef {
            fill: Some(rgb(0xFF_CC99)),
            font: Some(rgb(0x3F_3F76)),
            outline: Some((BorderStyle::Thin, rgb(0x7F_7F7F))),
            ..style("Input", DATA)
        },
        CellStyleDef {
            font: Some(rgb(0xFA_7D00)),
            bottom: Some((BorderStyle::Double, rgb(0xFF_8001))),
            ..style("Linked Cell", DATA)
        },
        CellStyleDef {
            fill: Some(rgb(0xFF_FFCC)),
            outline: Some((BorderStyle::Thin, rgb(0xB2_B2B2))),
            ..style("Note", DATA)
        },
        CellStyleDef {
            fill: Some(rgb(0xF2_F2F2)),
            font: Some(rgb(0x3F_3F3F)),
            bold: true,
            outline: Some((BorderStyle::Thin, rgb(0x3F_3F3F))),
            ..style("Output", DATA)
        },
        CellStyleDef {
            font: Some(rgb(0xFF_0000)),
            ..style("Warning Text", DATA)
        },
        CellStyleDef {
            font: Some(Color::theme(3, 0.0)),
            bold: true,
            size: Some(15.0),
            bottom: Some((BorderStyle::Thick, accent(1, 0.0))),
            ..style("Heading 1", TITLES)
        },
        CellStyleDef {
            font: Some(Color::theme(3, 0.0)),
            bold: true,
            size: Some(13.0),
            bottom: Some((BorderStyle::Thick, accent(1, 0.5))),
            ..style("Heading 2", TITLES)
        },
        CellStyleDef {
            font: Some(Color::theme(3, 0.0)),
            bold: true,
            bottom: Some((BorderStyle::Medium, accent(1, 0.4))),
            ..style("Heading 3", TITLES)
        },
        CellStyleDef {
            font: Some(Color::theme(3, 0.0)),
            bold: true,
            ..style("Heading 4", TITLES)
        },
        CellStyleDef {
            font: Some(Color::theme(3, 0.0)),
            size: Some(18.0),
            ..style("Title", TITLES)
        },
        CellStyleDef {
            bold: true,
            top: Some((BorderStyle::Thin, accent(1, 0.0))),
            bottom: Some((BorderStyle::Double, accent(1, 0.0))),
            ..style("Total", TITLES)
        },
    ];
    const SHADES: [(&str, f64); 3] = [("20%", 0.8), ("40%", 0.6), ("60%", 0.4)];
    const NAMES: [[&str; 4]; 6] = [
        ["20% - Accent1", "40% - Accent1", "60% - Accent1", "Accent1"],
        ["20% - Accent2", "40% - Accent2", "60% - Accent2", "Accent2"],
        ["20% - Accent3", "40% - Accent3", "60% - Accent3", "Accent3"],
        ["20% - Accent4", "40% - Accent4", "60% - Accent4", "Accent4"],
        ["20% - Accent5", "40% - Accent5", "60% - Accent5", "Accent5"],
        ["20% - Accent6", "40% - Accent6", "60% - Accent6", "Accent6"],
    ];
    for (k, names) in NAMES.iter().enumerate() {
        let i = u8::try_from(k + 1).unwrap_or(1);
        for (j, (_, tint)) in SHADES.iter().enumerate() {
            v.push(CellStyleDef {
                fill: Some(accent(i, *tint)),
                font: (j == 2).then(|| rgb(0xFF_FFFF)),
                ..style(names[j], THEMED)
            });
        }
        v.push(CellStyleDef {
            fill: Some(accent(i, 0.0)),
            font: Some(rgb(0xFF_FFFF)),
            ..style(names[3], THEMED)
        });
    }
    v.extend([
        CellStyleDef {
            num_fmt: Some("_-* #,##0.00_-;-* #,##0.00_-;_-* \"-\"??_-;_-@_-"),
            ..style("Comma", NUMBER)
        },
        CellStyleDef {
            num_fmt: Some("_-* #,##0_-;-* #,##0_-;_-* \"-\"_-;_-@_-"),
            ..style("Comma [0]", NUMBER)
        },
        CellStyleDef {
            num_fmt: Some("_-\"$\"* #,##0.00_-;-\"$\"* #,##0.00_-;_-\"$\"* \"-\"??_-;_-@_-"),
            ..style("Currency", NUMBER)
        },
        CellStyleDef {
            num_fmt: Some("_-\"$\"* #,##0_-;-\"$\"* #,##0_-;_-\"$\"* \"-\"_-;_-@_-"),
            ..style("Currency [0]", NUMBER)
        },
        CellStyleDef {
            num_fmt: Some("0%"),
            ..style("Percent", NUMBER)
        },
    ]);
    v
}

/// The cell styles of the gallery with their swatch colours (resolved with the Office theme).
pub(crate) fn cell_styles() -> Vec<GridCellStyleInfo> {
    let theme = kiln_sheet::Theme::default();
    builtin_styles()
        .into_iter()
        .map(|d| GridCellStyleInfo {
            name: d.name.into(),
            group: d.group.into(),
            fill: hex(d.fill, &theme, 0xFF_FFFF),
            color: hex(d.font, &theme, 0),
            bold: d.bold,
            border: hex(d.bottom.or(d.outline).map(|b| b.1), &theme, 0),
        })
        .collect()
}

impl GridDoc {
    /// The table at a cell for the Table Design tab.
    pub(crate) fn table_info(&self, sid: SheetId, at: CellPos) -> Option<GridTableInfo> {
        let t = self.workbook.sheet(sid)?.table_at(at.row, at.col)?;
        let style = t.style.clone();
        Some(GridTableInfo {
            name: t.name.clone(),
            range: GridRange {
                r0: t.range.r0,
                c0: t.range.c0,
                r1: t.range.r1,
                c1: t.range.c1,
            },
            reference: t.range.to_a1(true),
            style: style.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
            options: GridTableOptions {
                header_row: t.header_row,
                total_row: t.totals_row,
                banded_rows: style.as_ref().is_some_and(|s| s.show_row_stripes),
                banded_columns: style.as_ref().is_some_and(|s| s.show_column_stripes),
                first_column: style.as_ref().is_some_and(|s| s.show_first_column),
                last_column: style.as_ref().is_some_and(|s| s.show_last_column),
                filter_button: t.auto_filter.is_some(),
            },
            columns: t
                .columns
                .iter()
                .map(|c| GridTableColumn {
                    name: c.name.clone(),
                    totals: grid_totals(c.totals_function),
                    formula: c.calculated_formula.as_ref().map(|f| format!("={f}")),
                })
                .collect(),
            column: at.col - t.range.c0,
        })
    }

    /// The built-in table styles, Light, Medium and Dark, with their swatch colours.
    pub(crate) fn table_styles(&self) -> Vec<GridTableStyle> {
        let theme = self.workbook.theme();
        let mut out = Vec::new();
        for (family, max) in [("Light", 21), ("Medium", 28), ("Dark", 11)] {
            for n in 1..=max {
                let name = format!("TableStyle{family}{n}");
                let header = builtin_element(&name, TableStyleElement::HeaderRow);
                let band = builtin_element(&name, TableStyleElement::FirstRowStripe);
                let whole = builtin_element(&name, TableStyleElement::WholeTable);
                let fill = |d: &Option<Dxf>| {
                    d.as_ref()
                        .and_then(|d| d.fill.as_ref())
                        .and_then(|f| hex(f.main_color(), theme, 0xFF_FFFF))
                };
                out.push(GridTableStyle {
                    family: family.into(),
                    header: fill(&header),
                    header_text: header
                        .as_ref()
                        .and_then(|d| d.font.as_ref())
                        .and_then(|f| hex(f.color, theme, 0)),
                    band: fill(&band).or_else(|| fill(&whole)),
                    line: whole
                        .as_ref()
                        .or(header.as_ref())
                        .and_then(|d| d.border.as_ref())
                        .and_then(|b| b.bottom.as_ref().or(b.top.as_ref()))
                        .and_then(|s| hex(s.color, theme, 0)),
                    name,
                });
            }
        }
        out
    }

    fn active_table(&self) -> Result<Table> {
        let sid = self.sid()?;
        let a = self.view().sel.active;
        self.workbook
            .sheet(sid)
            .and_then(|s| s.table_at(a.row, a.col))
            .cloned()
            .ok_or_else(|| KilnError::InvalidInput("select a cell in a table first".into()))
    }

    /// Commits a cell entry: a formula typed in a table column whose other data cells are empty
    /// fills the whole column and becomes the column's calculated formula (Excel's AutoCorrect
    /// option "Fill formulas in tables to create calculated columns"), in one undo step.
    pub(crate) fn commit_entry(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        cmd: Command,
        text: &str,
    ) -> Result<()> {
        let calculated = text.starts_with('=') && text.len() > 1;
        let table = calculated
            .then(|| self.workbook.sheet(sheet)?.table_at(row, col).cloned())
            .flatten();
        let fill = table.as_ref().and_then(|t| {
            let dr = t.data_range()?;
            if dr.r1 == dr.r0 || !(dr.r0..=dr.r1).contains(&row) {
                return None;
            }
            let s = self.workbook.sheet(sheet)?;
            let empty = (dr.r0..=dr.r1).filter(|&r| r != row).all(|r| {
                s.cells().get(r, col).is_none_or(|c| {
                    matches!(c.value, CellValue::Blank)
                        && c.kind == kiln_sheet::store::CellKind::Value
                })
            });
            empty.then_some(dr)
        });
        let (Some(t), Some(dr)) = (table, fill) else {
            self.exec("grid.editing.commit", cmd)?;
            return Ok(());
        };
        let mut columns = t.columns.clone();
        if let Some(c) = columns.get_mut((col - t.range.c0) as usize) {
            c.calculated_formula = Some(text[1..].to_owned());
        }
        self.exec_all(
            "grid.editing.commit",
            vec![
                cmd,
                Command::CopyRange {
                    sheet,
                    range: Range::cell(row, col),
                    to_sheet: None,
                    to: Range::new(dr.r0, col, dr.r1, col),
                    what: PasteWhat::Formulas,
                    op: PasteOp::None,
                    transpose: false,
                    skip_blanks: false,
                },
                Command::UpdateTable {
                    name: t.name.clone(),
                    patch: Box::new(TablePatch {
                        columns: Some(columns),
                        ..TablePatch::default()
                    }),
                },
            ],
        )?;
        self.features.message = Some(format!(
            "The formula fills the {} column (a calculated column).",
            t.columns
                .get((col - t.range.c0) as usize)
                .map_or("", |c| c.name.as_str())
        ));
        Ok(())
    }

    fn update(&mut self, label: &'static str, name: &str, patch: TablePatch) -> Result<()> {
        self.exec(
            label,
            Command::UpdateTable {
                name: name.to_owned(),
                patch: Box::new(patch),
            },
        )?;
        Ok(())
    }

    fn create_table(&mut self, range: Option<GridRange>, header: bool, style: &str) -> Result<()> {
        let sid = self.sid()?;
        let mut r = match range {
            Some(g) => Range::new(g.r0, g.c0, g.r1, g.c1),
            None => self.data_range()?,
        };
        if r.is_whole_rows() || r.is_whole_cols() {
            r = self.clip_to_data(r);
        }
        if r.width() > 16_384 || r.height() < 1 {
            return Err(KilnError::InvalidInput(
                "the table's range is not valid".into(),
            ));
        }
        if let Some(sheet) = self.workbook.sheet(sid) {
            if sheet.tables().iter().any(|t| t.range.intersects(&r)) {
                return Err(KilnError::InvalidInput(
                    "A table can't overlap a range that contains another table.".into(),
                ));
            }
            if !sheet.merges_in(&r).is_empty() {
                return Err(KilnError::InvalidInput(
                    "A table can't contain merged cells: unmerge them first.".into(),
                ));
            }
        }
        let mut cmds = Vec::new();
        // Excel moves a sheet AutoFilter into the table.
        if self
            .workbook
            .sheet(sid)
            .and_then(|s| s.meta().auto_filter.as_ref())
            .is_some_and(|f| f.range.intersects(&r))
        {
            cmds.push(Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::AutoFilter(None)),
            });
        }
        let names = if header {
            column_names(
                (r.c0..=r.c1)
                    .map(|c| self.workbook.display_text(sid, r.r0, c))
                    .collect(),
            )
        } else {
            // A header row goes in above the data.
            cmds.push(Command::InsertRows {
                sheet: sid,
                at: r.r0,
                count: 1,
                format_from: Default::default(),
            });
            r = Range::new(r.r0, r.c0, r.r1 + 1, r.c1);
            column_names(vec![String::new(); r.width() as usize])
        };
        if r.height() < 2 {
            r.r1 += 1;
        }
        let name = self.workbook.next_table_name();
        cmds.push(Command::AddTable {
            sheet: sid,
            table: Box::new(Table {
                id: 0,
                name: name.clone(),
                display_name: Some(name),
                range: r,
                header_row: true,
                totals_row: false,
                columns: names.into_iter().map(TableColumn::named).collect(),
                style: Some(TableStyleInfo {
                    name: if style.is_empty() {
                        "TableStyleMedium2".into()
                    } else {
                        style.to_owned()
                    },
                    ..TableStyleInfo::default()
                }),
                auto_filter: Some(TableFilter::default()),
                comment: None,
            }),
        });
        self.exec_all("grid.table.create", cmds)?;
        Ok(())
    }

    /// Turns the total row on or off: the row below becomes the total row (a row is inserted
    /// when it holds something), with Total in the first column and a SUBTOTAL in the last.
    fn total_row(&mut self, t: &Table, on: bool) -> Result<Vec<Command>> {
        let sid = self.sid()?;
        let mut cmds = Vec::new();
        if on == t.totals_row {
            return Ok(cmds);
        }
        let mut columns = t.columns.clone();
        if on {
            let below = t.range.r1 + 1;
            let occupied = (t.range.c0..=t.range.c1).any(|c| self.has_content(sid, below, c));
            if occupied {
                cmds.push(Command::InsertRows {
                    sheet: sid,
                    at: below,
                    count: 1,
                    format_from: Default::default(),
                });
            }
            let last = columns.len().saturating_sub(1);
            let numeric = t.data_range().is_some_and(|d| {
                (d.r0..=d.r1).any(|r| {
                    matches!(
                        self.workbook.value(sid, r, t.range.c1),
                        CellValue::Number(_)
                    )
                })
            });
            if let Some(c) = columns.get_mut(last)
                && c.totals_function == TotalsFunction::None
            {
                c.totals_function = if numeric {
                    TotalsFunction::Sum
                } else {
                    TotalsFunction::Count
                };
            }
            if last > 0
                && let Some(c) = columns.first_mut()
                && c.totals_label.is_none()
                && c.totals_function == TotalsFunction::None
            {
                c.totals_label = Some("Total".into());
            }
            let range = Range::new(t.range.r0, t.range.c0, below, t.range.c1);
            cmds.push(Command::UpdateTable {
                name: t.name.clone(),
                patch: Box::new(TablePatch {
                    range: Some(range),
                    totals_row: Some(true),
                    columns: Some(columns.clone()),
                    ..TablePatch::default()
                }),
            });
            let cells = columns
                .iter()
                .enumerate()
                .map(|(i, c)| CellInput {
                    row: below,
                    col: t.range.c0 + i as u32,
                    content: match (
                        totals_formula(&t.name, &c.name, c.totals_function),
                        &c.totals_label,
                    ) {
                        (Some(f), _) => Content::Formula(f),
                        (None, Some(l)) => Content::Value(CellValue::text(l.as_str())),
                        (None, None) => Content::Clear,
                    },
                    style: None,
                })
                .collect();
            cmds.push(Command::SetCells { sheet: sid, cells });
        } else {
            let last = t.range.r1;
            cmds.push(Command::ClearRange {
                sheet: sid,
                ranges: vec![Range::new(last, t.range.c0, last, t.range.c1)],
                what: kiln_sheet::commands::ClearWhat::Contents,
            });
            cmds.push(Command::UpdateTable {
                name: t.name.clone(),
                patch: Box::new(TablePatch {
                    range: Some(Range::new(
                        t.range.r0,
                        t.range.c0,
                        last.saturating_sub(1).max(t.range.r0 + 1),
                        t.range.c1,
                    )),
                    totals_row: Some(false),
                    ..TablePatch::default()
                }),
            });
        }
        Ok(cmds)
    }

    pub(crate) fn table_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        match cmd {
            GridCommand::CreateTable {
                range,
                header,
                style,
            } => self.create_table(*range, *header, style)?,
            GridCommand::TableStyle { style } => {
                let t = self.active_table()?;
                let info = if style.is_empty() {
                    None
                } else {
                    Some(TableStyleInfo {
                        name: style.clone(),
                        ..t.style.clone().unwrap_or_default()
                    })
                };
                self.update(
                    "grid.table.style",
                    &t.name,
                    TablePatch {
                        style: info.map_or(Patch::Clear, Patch::Set),
                        ..TablePatch::default()
                    },
                )?;
            }
            GridCommand::TableOptions { options } => {
                let t = self.active_table()?;
                if options.header_row != t.header_row {
                    return Err(KilnError::Unsupported(
                        "turning a table's header row off".into(),
                    ));
                }
                let mut cmds = self.total_row(&t, options.total_row)?;
                let style = t.style.clone().unwrap_or_default();
                cmds.push(Command::UpdateTable {
                    name: t.name.clone(),
                    patch: Box::new(TablePatch {
                        style: Patch::Set(TableStyleInfo {
                            show_row_stripes: options.banded_rows,
                            show_column_stripes: options.banded_columns,
                            show_first_column: options.first_column,
                            show_last_column: options.last_column,
                            ..style
                        }),
                        auto_filter: match (options.filter_button, &t.auto_filter) {
                            (true, None) => Patch::Set(TableFilter::default()),
                            (false, Some(_)) => Patch::Clear,
                            _ => Patch::Keep,
                        },
                        ..TablePatch::default()
                    }),
                });
                if !options.filter_button && t.auto_filter.is_some() {
                    // Hidden rows come back first.
                    cmds.insert(
                        0,
                        Command::UpdateTable {
                            name: t.name.clone(),
                            patch: Box::new(TablePatch {
                                auto_filter: Patch::Set(TableFilter::default()),
                                ..TablePatch::default()
                            }),
                        },
                    );
                    cmds.insert(1, Command::ApplyFilter { sheet: sid });
                }
                self.exec_all("grid.table.options", cmds)?;
            }
            GridCommand::TableTotals { col, totals: f } => {
                let t = self.active_table()?;
                if !t.totals_row {
                    return Err(KilnError::InvalidInput(
                        "turn the total row on first".into(),
                    ));
                }
                let i = col
                    .checked_sub(t.range.c0)
                    .filter(|i| (*i as usize) < t.columns.len())
                    .ok_or_else(|| {
                        KilnError::InvalidInput("the column is not in the table".into())
                    })? as usize;
                let mut columns = t.columns.clone();
                columns[i].totals_function = totals(*f);
                columns[i].totals_label = None;
                let content =
                    match totals_formula(&t.name, &columns[i].name, columns[i].totals_function) {
                        Some(x) => Content::Formula(x),
                        None => Content::Clear,
                    };
                self.exec_all(
                    "grid.table.totals",
                    vec![
                        Command::UpdateTable {
                            name: t.name.clone(),
                            patch: Box::new(TablePatch {
                                columns: Some(columns),
                                ..TablePatch::default()
                            }),
                        },
                        Command::SetCells {
                            sheet: sid,
                            cells: vec![CellInput {
                                row: t.range.r1,
                                col: *col,
                                content,
                                style: None,
                            }],
                        },
                    ],
                )?;
            }
            GridCommand::RenameTable { name } => {
                let t = self.active_table()?;
                let name = name.trim();
                if !valid_table_name(name) {
                    return Err(KilnError::InvalidInput(format!(
                        "\"{name}\" is not a valid table name: start with a letter or underscore, no spaces"
                    )));
                }
                if !name.eq_ignore_ascii_case(&t.name)
                    && (self.workbook.find_table(name).is_some()
                        || self
                            .workbook
                            .names()
                            .iter()
                            .any(|n| n.name.eq_ignore_ascii_case(name)))
                {
                    return Err(KilnError::InvalidInput(format!(
                        "the name \"{name}\" is already used"
                    )));
                }
                self.update(
                    "grid.table.rename",
                    &t.name,
                    TablePatch {
                        name: Some(name.to_owned()),
                        display_name: Patch::Set(name.to_owned()),
                        ..TablePatch::default()
                    },
                )?;
            }
            GridCommand::ResizeTable { range } => {
                let t = self.active_table()?;
                let r = Range::new(range.r0, range.c0, range.r1, range.c1);
                if r.r0 != t.range.r0 || !r.intersects(&t.range) {
                    return Err(KilnError::InvalidInput(
                        "The headers must remain in the same row, and the new range must overlap the table."
                            .into(),
                    ));
                }
                if r.c0 != t.range.c0 {
                    return Err(KilnError::InvalidInput(
                        "keep the table's first column; resize from its right and bottom edges"
                            .into(),
                    ));
                }
                self.update(
                    "grid.table.resize",
                    &t.name,
                    TablePatch {
                        range: Some(r),
                        ..TablePatch::default()
                    },
                )?;
            }
            GridCommand::ConvertToRange => {
                let t = self.active_table()?;
                let mut cmds = Vec::new();
                // The table's look stays as cell formats.
                for row in t.range.r0..=t.range.r1 {
                    for col in t.range.c0..=t.range.c1 {
                        let Some(d) = self.workbook.table_format(sid, row, col) else {
                            continue;
                        };
                        let cur = self.workbook.sheet(sid).map_or(0, |s| s.style_at(row, col));
                        let style = self
                            .log
                            .patch_style(&mut self.workbook, cur, &dxf_patch(&d));
                        if style != cur {
                            cmds.push(Command::SetStyleIndex {
                                sheet: sid,
                                ranges: vec![Range::cell(row, col)],
                                style,
                            });
                        }
                    }
                }
                cmds.push(Command::RemoveTable { name: t.name });
                self.exec_all("grid.table.convert_to_range", cmds)?;
            }
            GridCommand::ApplyCellStyle { name } => {
                let def = builtin_styles()
                    .into_iter()
                    .find(|d| d.name.eq_ignore_ascii_case(name))
                    .ok_or_else(|| KilnError::InvalidInput(format!("no cell style \"{name}\"")))?;
                let side = |b: Option<(BorderStyle, Color)>| match b {
                    Some((style, c)) => Patch::Set(BorderSide {
                        style,
                        color: Some(c),
                    }),
                    None => Patch::Clear,
                };
                let outline = def.outline;
                let patch = if def.name == "Normal" {
                    StylePatch {
                        num_fmt: Some("General".into()),
                        font: Some(FontPatch {
                            bold: Some(false),
                            italic: Some(false),
                            color: Some(Color::theme(1, 0.0)),
                            size: Some(kiln_sheet::num::F64(11.0)),
                            ..FontPatch::default()
                        }),
                        fill: Patch::Set(Fill::NONE),
                        border: Some(BorderPatch {
                            left: Patch::Clear,
                            right: Patch::Clear,
                            top: Patch::Clear,
                            bottom: Patch::Clear,
                            ..BorderPatch::default()
                        }),
                        ..StylePatch::default()
                    }
                } else {
                    StylePatch {
                        num_fmt: def.num_fmt.map(str::to_owned),
                        font: (def.font.is_some() || def.bold || def.italic || def.size.is_some())
                            .then(|| FontPatch {
                                color: def.font,
                                bold: Some(def.bold),
                                italic: Some(def.italic),
                                size: def.size.map(kiln_sheet::num::F64),
                                ..FontPatch::default()
                            }),
                        fill: def.fill.map_or(Patch::Keep, |c| Patch::Set(Fill::solid(c))),
                        border: (def.bottom.is_some() || def.top.is_some() || outline.is_some())
                            .then(|| BorderPatch {
                                left: if outline.is_some() {
                                    side(outline)
                                } else {
                                    Patch::Keep
                                },
                                right: if outline.is_some() {
                                    side(outline)
                                } else {
                                    Patch::Keep
                                },
                                top: side(def.top.or(outline)),
                                bottom: side(def.bottom.or(outline)),
                                ..BorderPatch::default()
                            }),
                        ..StylePatch::default()
                    }
                };
                self.patch_selection("grid.styles.apply_cell_style", patch)?;
            }
            GridCommand::PaintFormat { from } => {
                let src = Range::new(from.r0, from.c0, from.r1, from.c1);
                let v = self.view();
                let dest = v.sel.ranges.last().copied().unwrap_or(Range::cell(0, 0));
                // A click paints the source's size from the cell clicked.
                let to = if dest.height() % src.height() == 0 && dest.width() % src.width() == 0 {
                    dest
                } else {
                    Range::new(
                        dest.r0,
                        dest.c0,
                        (dest.r0 + src.height() - 1).min(kiln_sheet::MAX_ROWS - 1),
                        (dest.c0 + src.width() - 1).min(kiln_sheet::MAX_COLS - 1),
                    )
                };
                self.exec(
                    "grid.clipboard.paint_format",
                    Command::CopyRange {
                        sheet: sid,
                        range: src,
                        to_sheet: None,
                        to,
                        what: PasteWhat::Formats,
                        op: PasteOp::None,
                        transpose: false,
                        skip_blanks: false,
                    },
                )?;
                let mut v = self.view();
                v.sel.ranges = vec![to];
                self.set_view(v);
            }
            _ => {}
        }
        Ok(Change::Content)
    }
}
