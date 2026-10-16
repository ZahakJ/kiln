// SPDX-License-Identifier: AGPL-3.0-or-later
//! Table styles: which parts of a table style apply to a cell (header row, totals row, first and
//! last column, banded rows and columns with their stripe sizes, the corner cells), the custom
//! table styles of a workbook, and Excel's built-in Light, Medium and Dark families.
//!
//! Custom styles (`<tableStyles>` in a file) are reproduced exactly from their differential
//! formats. Built-in styles are generated from the theme following the families' layout —
//! Light: thin rules in the accent, banded with a pale tint; Medium: an accent header with white
//! bold text and pale accent bands; Dark: a dark accent body with white text — which matches
//! Excel's presets in structure and colour family but not in every border weight.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::meta::{Table, TableStyleInfo};
use crate::styles::{Border, BorderSide, BorderStyle, Dxf, Fill, FontPatch};

/// A part of a table that a table style formats, lowest precedence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum TableStyleElement {
    WholeTable,
    FirstColumnStripe,
    SecondColumnStripe,
    FirstRowStripe,
    SecondRowStripe,
    LastColumn,
    FirstColumn,
    HeaderRow,
    TotalRow,
    FirstHeaderCell,
    LastHeaderCell,
    FirstTotalCell,
    LastTotalCell,
}

/// One element of a table style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableStyleEntry {
    /// Which part.
    pub element: TableStyleElement,
    /// Its differential format (an index into the workbook's dxfs).
    pub dxf: u32,
    /// Stripe size in rows or columns (1 unless set).
    pub size: u32,
}

/// A custom table style.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableStyleDef {
    /// Name (`TableStyleLight1` style names are built in).
    pub name: String,
    /// Usable for tables.
    #[serde(default = "yes")]
    pub table: bool,
    /// Usable for PivotTables.
    #[serde(default)]
    pub pivot: bool,
    /// Elements.
    pub elements: Vec<TableStyleEntry>,
}

fn yes() -> bool {
    true
}

/// The elements of a table style that apply to (row, col), lowest precedence first, given the
/// style options and the stripe sizes (`row_stripes`, `col_stripes` = sizes of the first and
/// second stripes).
pub fn cell_elements(
    t: &Table,
    info: &TableStyleInfo,
    row: u32,
    col: u32,
    row_stripes: (u32, u32),
    col_stripes: (u32, u32),
) -> Vec<TableStyleElement> {
    use TableStyleElement::*;
    let r = t.range;
    if !r.contains(row, col) {
        return Vec::new();
    }
    let mut out = vec![WholeTable];
    let header = t.header_row && row == r.r0;
    let total = t.totals_row && row == r.r1;
    let data_r0 = r.r0 + u32::from(t.header_row);
    if info.show_column_stripes {
        let (a, b) = (col_stripes.0.max(1), col_stripes.1.max(1));
        let k = (col - r.c0) % (a + b);
        out.push(if k < a {
            FirstColumnStripe
        } else {
            SecondColumnStripe
        });
    }
    if info.show_row_stripes && !header && !total && row >= data_r0 {
        let (a, b) = (row_stripes.0.max(1), row_stripes.1.max(1));
        let k = (row - data_r0) % (a + b);
        out.push(if k < a {
            FirstRowStripe
        } else {
            SecondRowStripe
        });
    }
    if info.show_last_column && col == r.c1 {
        out.push(LastColumn);
    }
    if info.show_first_column && col == r.c0 {
        out.push(FirstColumn);
    }
    if header {
        out.push(HeaderRow);
        if info.show_first_column && col == r.c0 {
            out.push(FirstHeaderCell);
        }
        if info.show_last_column && col == r.c1 {
            out.push(LastHeaderCell);
        }
    }
    if total {
        out.push(TotalRow);
        if info.show_first_column && col == r.c0 {
            out.push(FirstTotalCell);
        }
        if info.show_last_column && col == r.c1 {
            out.push(LastTotalCell);
        }
    }
    out
}

/// A built-in style's family and number (`TableStyleMedium2` → (Medium, 2)).
pub fn builtin_family(name: &str) -> Option<(&'static str, u32)> {
    for (family, max) in [("Light", 21), ("Medium", 28), ("Dark", 11)] {
        if let Some(n) = name
            .strip_prefix("TableStyle")
            .and_then(|s| s.strip_prefix(family))
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|n| (1..=max).contains(n))
        {
            return Some((family, n));
        }
    }
    None
}

/// The differential formats of a built-in table style for one element.
pub fn builtin_element(name: &str, element: TableStyleElement) -> Option<Dxf> {
    use TableStyleElement::*;
    let (family, n) = builtin_family(name)?;
    // Each family cycles through dark 1 and the six accents in groups of seven.
    let slot = (n - 1) % 7;
    let theme = if slot == 0 { 1 } else { 3 + slot as u8 };
    let accent = |tint: f64| Color::theme(theme, tint);
    let solid = |c: Color| Some(Fill::solid(c));
    let line = |c: Color, style: BorderStyle| {
        Some(BorderSide {
            style,
            color: Some(c),
        })
    };
    let white_bold = Some(FontPatch {
        bold: Some(true),
        color: Some(Color::theme(0, 0.0)),
        ..FontPatch::default()
    });
    let bold = Some(FontPatch {
        bold: Some(true),
        ..FontPatch::default()
    });
    let dxf = match (family, element) {
        ("Light", WholeTable) => Dxf {
            border: Some(Border {
                top: line(accent(0.0), BorderStyle::Thin),
                bottom: line(accent(0.0), BorderStyle::Thin),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        ("Light", HeaderRow) => Dxf {
            font: bold,
            border: Some(Border {
                bottom: line(accent(0.0), BorderStyle::Thin),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        ("Light", FirstRowStripe) => Dxf {
            fill: solid(accent(0.799_981_688_894_314_4)),
            ..Dxf::default()
        },
        ("Medium", WholeTable) => Dxf {
            border: Some(Border {
                bottom: line(accent(0.399_975_585_192_419_2), BorderStyle::Thin),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        ("Medium", HeaderRow) => Dxf {
            font: white_bold,
            fill: solid(accent(0.0)),
            ..Dxf::default()
        },
        ("Medium", FirstRowStripe) => Dxf {
            fill: solid(accent(0.799_981_688_894_314_4)),
            ..Dxf::default()
        },
        ("Dark", WholeTable) => Dxf {
            font: Some(FontPatch {
                color: Some(Color::theme(0, 0.0)),
                ..FontPatch::default()
            }),
            fill: solid(accent(-0.249_977_111_117_893)),
            ..Dxf::default()
        },
        ("Dark", HeaderRow) => Dxf {
            font: white_bold,
            fill: solid(Color::theme(1, 0.0)),
            ..Dxf::default()
        },
        ("Dark", FirstRowStripe) => Dxf {
            fill: solid(accent(-0.499_984_740_745_262_8)),
            ..Dxf::default()
        },
        (_, TotalRow) => Dxf {
            font: bold,
            border: Some(Border {
                top: line(accent(0.0), BorderStyle::Double),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        (_, FirstColumn | LastColumn) => Dxf {
            font: bold,
            ..Dxf::default()
        },
        (_, FirstColumnStripe) => Dxf {
            fill: solid(accent(0.799_981_688_894_314_4)),
            ..Dxf::default()
        },
        _ => return None,
    };
    Some(dxf)
}

impl crate::Workbook {
    /// The table-style format of a cell inside a table (layered from the lowest-precedence
    /// element up), or `None` outside tables or for tables without a style.
    pub fn table_format(&self, sheet: kiln_formula::SheetId, row: u32, col: u32) -> Option<Dxf> {
        let s = self.sheet(sheet)?;
        let t = s.table_at(row, col)?;
        let info = t.style.clone()?;
        let custom = self
            .styles()
            .table_styles()
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(&info.name));
        let size = |e: TableStyleElement| {
            custom
                .and_then(|d| d.elements.iter().find(|x| x.element == e))
                .map_or(1, |x| x.size.max(1))
        };
        let rows = (
            size(TableStyleElement::FirstRowStripe),
            size(TableStyleElement::SecondRowStripe),
        );
        let cols = (
            size(TableStyleElement::FirstColumnStripe),
            size(TableStyleElement::SecondColumnStripe),
        );
        let mut out: Option<Dxf> = None;
        for e in cell_elements(t, &info, row, col, rows, cols) {
            let d = match custom {
                Some(def) => def
                    .elements
                    .iter()
                    .find(|x| x.element == e)
                    .and_then(|x| self.styles().dxf(x.dxf).cloned()),
                None => builtin_element(&info.name, e),
            };
            if let Some(d) = d {
                out = Some(match out {
                    Some(lower) => lower.layered(&d),
                    None => d,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Range;
    use crate::meta::TableColumn;
    use TableStyleElement::*;

    fn table(totals: bool) -> Table {
        Table {
            id: 1,
            name: "T".into(),
            display_name: None,
            range: Range::new(0, 0, 6, 2),
            header_row: true,
            totals_row: totals,
            columns: ["a", "b", "c"]
                .into_iter()
                .map(TableColumn::named)
                .collect(),
            style: None,
            auto_filter: None,
            comment: None,
        }
    }

    #[test]
    fn resolves_header_bands_and_totals() {
        let t = table(true);
        let info = TableStyleInfo {
            show_first_column: true,
            show_last_column: true,
            ..TableStyleInfo::default()
        };
        assert_eq!(
            cell_elements(&t, &info, 0, 0, (1, 1), (1, 1)),
            vec![WholeTable, FirstColumn, HeaderRow, FirstHeaderCell]
        );
        assert_eq!(
            cell_elements(&t, &info, 1, 1, (1, 1), (1, 1)),
            vec![WholeTable, FirstRowStripe]
        );
        assert_eq!(
            cell_elements(&t, &info, 2, 1, (1, 1), (1, 1)),
            vec![WholeTable, SecondRowStripe]
        );
        assert_eq!(
            cell_elements(&t, &info, 6, 2, (1, 1), (1, 1)),
            vec![WholeTable, LastColumn, TotalRow, LastTotalCell]
        );
        // Stripes of two rows.
        assert_eq!(
            cell_elements(&t, &info, 2, 1, (2, 1), (1, 1)),
            vec![WholeTable, FirstRowStripe]
        );
        assert_eq!(
            cell_elements(&t, &info, 3, 1, (2, 1), (1, 1)),
            vec![WholeTable, SecondRowStripe]
        );
        assert!(cell_elements(&t, &info, 9, 1, (1, 1), (1, 1)).is_empty());
        let cols = TableStyleInfo {
            show_row_stripes: false,
            show_column_stripes: true,
            ..TableStyleInfo::default()
        };
        assert_eq!(
            cell_elements(&t, &cols, 3, 1, (1, 1), (1, 1)),
            vec![WholeTable, SecondColumnStripe]
        );
    }

    #[test]
    fn built_in_families() {
        assert_eq!(builtin_family("TableStyleMedium2"), Some(("Medium", 2)));
        assert_eq!(builtin_family("TableStyleLight21"), Some(("Light", 21)));
        assert_eq!(builtin_family("TableStyleDark12"), None);
        assert_eq!(builtin_family("Custom"), None);
        let header = builtin_element("TableStyleMedium2", HeaderRow).expect("header");
        assert_eq!(header.fill, Some(Fill::solid(Color::theme(4, 0.0))));
        assert!(builtin_element("TableStyleMedium2", SecondRowStripe).is_none());
    }
}
