// SPDX-License-Identifier: AGPL-3.0-or-later
//! Resolution of structured references (`Table1[[#Headers],[Qty]]`, `[@Qty]`) to areas.

use super::host::TableDef;
use super::types::{Area, SheetId};
use crate::parser::StructuredSpecial;
use crate::refs::Bounds;
use crate::values::ErrorValue;

/// The area a structured reference covers, seen from the cell `(sheet, row)`.
pub(crate) fn resolve_structured(
    t: &TableDef,
    specials: &[StructuredSpecial],
    this_row: bool,
    columns: Option<&(String, Option<String>)>,
    at: (SheetId, u32),
) -> Result<Area, ErrorValue> {
    let tb = t.bounds;
    let (mut c0, mut c1) = (tb.c0, tb.c1);
    if let Some((a, b)) = columns {
        let find = |name: &str| {
            t.columns
                .iter()
                .position(|c| c.to_uppercase() == name.to_uppercase())
                .ok_or(ErrorValue::Ref)
        };
        let ia = find(a)? as u32;
        let ib = match b {
            Some(b) => find(b)? as u32,
            None => ia,
        };
        c0 = tb.c0 + ia.min(ib);
        c1 = tb.c0 + ia.max(ib);
        if c1 > tb.c1 {
            return Err(ErrorValue::Ref);
        }
    }
    let data_r0 = tb.r0 + u32::from(t.header_row);
    let data_r1 = tb.r1.saturating_sub(u32::from(t.totals_row));
    if this_row {
        let (sheet, row) = at;
        if sheet != t.sheet || row < data_r0 || row > data_r1 {
            return Err(ErrorValue::Value);
        }
        return Ok(Area {
            sheet: t.sheet,
            bounds: Bounds {
                r0: row,
                c0,
                r1: row,
                c1,
            },
        });
    }
    let mut r0 = u32::MAX;
    let mut r1 = 0u32;
    let mut any = false;
    let mut add = |a: u32, b: u32| {
        if a <= b {
            r0 = r0.min(a);
            r1 = r1.max(b);
            any = true;
        }
    };
    let items: &[StructuredSpecial] = if specials.is_empty() {
        &[StructuredSpecial::Data]
    } else {
        specials
    };
    for s in items {
        match s {
            StructuredSpecial::All => add(tb.r0, tb.r1),
            StructuredSpecial::Data => add(data_r0, data_r1),
            StructuredSpecial::Headers => {
                if !t.header_row {
                    return Err(ErrorValue::Ref);
                }
                add(tb.r0, tb.r0);
            }
            StructuredSpecial::Totals => {
                if !t.totals_row {
                    return Err(ErrorValue::Ref);
                }
                add(tb.r1, tb.r1);
            }
        }
    }
    if !any {
        return Err(ErrorValue::Ref);
    }
    Ok(Area {
        sheet: t.sheet,
        bounds: Bounds { r0, c0, r1, c1 },
    })
}
