// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTable styles: which part of a style formats a report cell, and the built-in
//! PivotStyleLight1–28, PivotStyleMedium1–28 and PivotStyleDark1–28.
//!
//! Like the table styles ([`crate::table_style`]), the built-in styles are generated from the
//! theme: each family cycles through Dark 1 and the six accents in groups of seven, and the
//! groups differ in how strong the header, subtotal and grand-total rows are. Excel's default for
//! new reports is PivotStyleLight16: a pale accent header and grand total with accent rules.

use serde::{Deserialize, Serialize};

use super::render::{PivotLineKind, PivotOutput};
use super::{PivotStyleInfo, PivotTable};
use crate::color::Color;
use crate::styles::{Border, BorderSide, BorderStyle, Dxf, Fill, FontPatch};

/// A part of a report a PivotTable style formats, lowest precedence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PivotStyleElement {
    WholeTable,
    PageFieldLabels,
    PageFieldValues,
    FirstColumnStripe,
    SecondColumnStripe,
    FirstRowStripe,
    SecondRowStripe,
    FirstColumn,
    HeaderRow,
    FirstHeaderCell,
    SubtotalColumn1,
    SubtotalColumn2,
    SubtotalRow1,
    SubtotalRow2,
    SubtotalRow3,
    RowSubheading1,
    RowSubheading2,
    RowSubheading3,
    ColumnSubheading1,
    ColumnSubheading2,
    GrandTotalColumn,
    GrandTotalRow,
}

/// The elements that apply to a cell of a report, lowest precedence first.
pub fn cell_elements(
    out: &PivotOutput,
    info: &PivotStyleInfo,
    row: u32,
    col: u32,
) -> Vec<PivotStyleElement> {
    use PivotStyleElement::*;
    let mut v = Vec::new();
    if let Some(p) = out.pages
        && p.contains(row, col)
    {
        v.push(if col == p.c0 {
            PageFieldLabels
        } else {
            PageFieldValues
        });
        return v;
    }
    let Some(r) = out.range else {
        return v;
    };
    if out.empty || !r.contains(row, col) {
        return v;
    }
    v.push(WholeTable);
    let body_r0 = r.r0 + out.header_rows;
    let label_end = r.c0 + out.label_cols;
    let line = (row >= body_r0)
        .then(|| out.rows.get((row - body_r0) as usize))
        .flatten();
    let column = (col >= label_end)
        .then(|| out.cols.get((col - label_end) as usize))
        .flatten();
    if row >= body_r0 {
        let i = row - body_r0;
        if info.row_stripes && !matches!(line.map(|l| l.kind), Some(PivotLineKind::Grand)) {
            v.push(if i.is_multiple_of(2) {
                FirstRowStripe
            } else {
                SecondRowStripe
            });
        }
    }
    if col >= label_end && info.col_stripes {
        let j = col - label_end;
        v.push(if j.is_multiple_of(2) {
            FirstColumnStripe
        } else {
            SecondColumnStripe
        });
    }
    if info.row_headers && col < label_end && row >= body_r0 {
        v.push(FirstColumn);
    }
    if let Some(c) = column {
        match c.kind {
            PivotLineKind::Subtotal => v.push(if c.level == 0 {
                SubtotalColumn1
            } else {
                SubtotalColumn2
            }),
            PivotLineKind::Grand => v.push(GrandTotalColumn),
            _ => {}
        }
    }
    if row < body_r0 {
        if info.col_headers {
            v.push(HeaderRow);
            if row == r.r0 + out.header_rows - 1 && col == r.c0 {
                v.push(FirstHeaderCell);
            }
            if row > r.r0 && row < body_r0 - 1
                || (out.col_label_rows > 0 && row > r.r0 && row < body_r0)
            {
                let depth = row - r.r0 - 1;
                v.push(if depth == 0 {
                    ColumnSubheading1
                } else {
                    ColumnSubheading2
                });
            }
        }
        return v;
    }
    if let Some(l) = line {
        match l.kind {
            PivotLineKind::Group if info.row_headers || col >= label_end => {
                v.push(match l.level {
                    0 => RowSubheading1,
                    1 => RowSubheading2,
                    _ => RowSubheading3,
                });
            }
            PivotLineKind::Subtotal => v.push(match l.level {
                0 => SubtotalRow1,
                1 => SubtotalRow2,
                _ => SubtotalRow3,
            }),
            PivotLineKind::Grand => v.push(GrandTotalRow),
            _ => {}
        }
    }
    v
}

/// A built-in style's family and number (`PivotStyleLight16` → (Light, 16)).
pub fn builtin_family(name: &str) -> Option<(&'static str, u32)> {
    for family in ["Light", "Medium", "Dark"] {
        if let Some(n) = name
            .strip_prefix("PivotStyle")
            .and_then(|s| s.strip_prefix(family))
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|n| (1..=28).contains(n))
        {
            return Some((family, n));
        }
    }
    None
}

/// Every built-in style name, in the gallery's order.
pub fn builtin_names() -> Vec<String> {
    let mut out = Vec::with_capacity(84);
    for family in ["Light", "Medium", "Dark"] {
        for n in 1..=28 {
            out.push(format!("PivotStyle{family}{n}"));
        }
    }
    out
}

/// The differential format of one element of a built-in style.
pub fn builtin_element(name: &str, element: PivotStyleElement) -> Option<Dxf> {
    use PivotStyleElement::*;
    let (family, n) = builtin_family(name)?;
    let slot = (n - 1) % 7;
    let group = (n - 1) / 7;
    let theme = if slot == 0 { 1 } else { 3 + slot as u8 };
    let accent = |tint: f64| Color::theme(theme, tint);
    let line = |c: Color, style: BorderStyle| {
        Some(BorderSide {
            style,
            color: Some(c),
        })
    };
    let bold = |color: Option<Color>| {
        Some(FontPatch {
            bold: Some(true),
            color,
            ..FontPatch::default()
        })
    };
    let white = Some(Color::theme(0, 0.0));
    let fill = |c: Color| Some(Fill::solid(c));
    let rule = |top: bool, bottom: bool, c: Color| {
        Some(Border {
            top: if top {
                line(c, BorderStyle::Thin)
            } else {
                None
            },
            bottom: if bottom {
                line(c, BorderStyle::Thin)
            } else {
                None
            },
            ..Border::default()
        })
    };
    let pale = 0.799_981_688_894_314_4;
    let light = 0.599_993_896_298_104_6;
    let d = match (family, group, element) {
        // Light 1–7: rules only.
        ("Light", 0, WholeTable) => Dxf {
            border: rule(true, true, accent(0.0)),
            ..Dxf::default()
        },
        ("Light", 0, HeaderRow) => Dxf {
            font: bold(None),
            border: rule(false, true, accent(0.0)),
            ..Dxf::default()
        },
        // Light 8–14: an accent header with white text.
        ("Light", 1, WholeTable) => Dxf {
            border: Some(Border {
                top: line(accent(0.0), BorderStyle::Thin),
                bottom: line(accent(0.0), BorderStyle::Thin),
                left: line(accent(0.0), BorderStyle::Thin),
                right: line(accent(0.0), BorderStyle::Thin),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        ("Light", 1, HeaderRow) => Dxf {
            font: bold(white),
            fill: fill(accent(0.0)),
            ..Dxf::default()
        },
        // Light 15–21 (the default, Light16): a pale header and grand total, accent rules.
        ("Light", 2, WholeTable) => Dxf {
            border: rule(true, true, accent(0.0)),
            ..Dxf::default()
        },
        ("Light", 2, HeaderRow) => Dxf {
            font: bold(None),
            fill: fill(accent(pale)),
            border: rule(false, true, accent(0.0)),
            ..Dxf::default()
        },
        ("Light", 2, GrandTotalRow) => Dxf {
            font: bold(None),
            fill: fill(accent(pale)),
            border: rule(true, false, accent(0.0)),
            ..Dxf::default()
        },
        // Light 22–28: tinted subheadings.
        ("Light", 3, HeaderRow) => Dxf {
            font: bold(None),
            border: rule(false, true, accent(0.0)),
            ..Dxf::default()
        },
        ("Light", 3, RowSubheading1) => Dxf {
            font: bold(None),
            fill: fill(accent(pale)),
            ..Dxf::default()
        },
        ("Light", _, GrandTotalRow) => Dxf {
            font: bold(None),
            border: Some(Border {
                top: line(accent(0.0), BorderStyle::Double),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        // Medium: an accent header, tinted subheadings and grand total.
        ("Medium", _, WholeTable) => Dxf {
            border: rule(false, true, accent(light)),
            ..Dxf::default()
        },
        ("Medium", g, HeaderRow) => Dxf {
            font: bold(if g % 2 == 0 { white } else { None }),
            fill: fill(if g % 2 == 0 {
                accent(0.0)
            } else {
                accent(light)
            }),
            ..Dxf::default()
        },
        ("Medium", _, RowSubheading1) => Dxf {
            font: bold(None),
            fill: fill(accent(pale)),
            ..Dxf::default()
        },
        ("Medium", _, GrandTotalRow) => Dxf {
            font: bold(None),
            fill: fill(accent(light)),
            border: Some(Border {
                top: line(accent(0.0), BorderStyle::Thin),
                ..Border::default()
            }),
            ..Dxf::default()
        },
        // Dark: a dark body with white text.
        ("Dark", _, WholeTable) => Dxf {
            font: Some(FontPatch {
                color: white,
                ..FontPatch::default()
            }),
            fill: fill(accent(-0.249_977_111_117_893)),
            ..Dxf::default()
        },
        ("Dark", _, HeaderRow) => Dxf {
            font: bold(white),
            fill: fill(Color::theme(1, 0.0)),
            ..Dxf::default()
        },
        ("Dark", _, RowSubheading1) => Dxf {
            font: bold(white),
            fill: fill(accent(-0.499_984_740_745_262_8)),
            ..Dxf::default()
        },
        ("Dark", _, GrandTotalRow) => Dxf {
            font: bold(white),
            fill: fill(Color::theme(1, 0.0)),
            ..Dxf::default()
        },
        // Every family: bold subtotals and subheadings, banded rows in a pale tint.
        (_, _, SubtotalRow1 | SubtotalRow2 | SubtotalRow3 | SubtotalColumn1 | SubtotalColumn2) => {
            Dxf {
                font: bold(None),
                ..Dxf::default()
            }
        }
        (_, _, RowSubheading1 | RowSubheading2 | GrandTotalColumn) => Dxf {
            font: bold(None),
            ..Dxf::default()
        },
        (_, _, PageFieldLabels) => Dxf {
            font: bold(None),
            ..Dxf::default()
        },
        ("Dark", _, FirstRowStripe | FirstColumnStripe) => Dxf {
            fill: fill(accent(-0.499_984_740_745_262_8)),
            ..Dxf::default()
        },
        (_, _, FirstRowStripe | FirstColumnStripe) => Dxf {
            fill: fill(accent(pale)),
            ..Dxf::default()
        },
        _ => return None,
    };
    Some(d)
}

impl crate::Workbook {
    /// The PivotTable-style format of a cell inside a PivotTable report, or `None`.
    pub fn pivot_format(&self, sheet: kiln_formula::SheetId, row: u32, col: u32) -> Option<Dxf> {
        let s = self.sheet(sheet)?;
        let p = s.pivot_at(row, col)?;
        pivot_cell_format(p, row, col)
    }
}

/// The PivotTable-style format of a report cell.
pub fn pivot_cell_format(p: &PivotTable, row: u32, col: u32) -> Option<Dxf> {
    let info = &p.options.style;
    if info.name.is_empty() {
        return None;
    }
    let mut out: Option<Dxf> = None;
    let range = p.output.range;
    let header_end = range.map(|r| r.r0 + p.output.header_rows);
    for e in cell_elements(&p.output, info, row, col) {
        if let Some(mut d) = builtin_element(&info.name, e) {
            // A style's rules frame its part: the whole table's on the table's edges, the header
            // row's under the last header row.
            if let (Some(b), Some(r)) = (d.border.as_mut(), range) {
                match e {
                    PivotStyleElement::WholeTable => {
                        if row != r.r0 {
                            b.top = None;
                        }
                        if row != r.r1 {
                            b.bottom = None;
                        }
                        if col != r.c0 {
                            b.left = None;
                        }
                        if col != r.c1 {
                            b.right = None;
                        }
                    }
                    PivotStyleElement::HeaderRow if header_end.is_some_and(|h| row + 1 != h) => {
                        b.bottom = None;
                    }
                    _ => {}
                }
            }
            let d = d;
            out = Some(match out {
                Some(lower) => lower.layered(&d),
                None => d,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_and_names() {
        assert_eq!(builtin_family("PivotStyleLight16"), Some(("Light", 16)));
        assert_eq!(builtin_family("PivotStyleDark28"), Some(("Dark", 28)));
        assert_eq!(builtin_family("PivotStyleMedium29"), None);
        assert_eq!(builtin_names().len(), 84);
        let h = builtin_element("PivotStyleLight16", PivotStyleElement::HeaderRow).expect("header");
        assert_eq!(
            h.fill,
            Some(Fill::solid(Color::theme(4, 0.799_981_688_894_314_4)))
        );
        assert!(builtin_element("PivotStyleLight16", PivotStyleElement::GrandTotalRow).is_some());
    }
}
