// SPDX-License-Identifier: AGPL-3.0-or-later
//! The PivotTable cache: the source read once into one column per field of indices into the
//! field's shared items (its distinct values, in the order they first appear). Text items are
//! told apart without regard to case, as Excel does; the first spelling met is the one shown.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::{PivotError, PivotSource};
use crate::address::Range;
use crate::value::CellValue;
use crate::{SheetId, Workbook};

/// How items are told apart: numbers by value, text without case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemKey {
    /// An empty cell.
    Blank,
    /// A number (bits of the value, `-0` as `0`).
    Number(u64),
    /// Text, lower-cased.
    Text(String),
    /// A logical.
    Bool(bool),
    /// An error code.
    Error(&'static str),
}

impl ItemKey {
    /// The key of a value.
    pub fn of(v: &CellValue) -> ItemKey {
        match v {
            CellValue::Blank => ItemKey::Blank,
            CellValue::Number(n) => ItemKey::Number(if *n == 0.0 { 0 } else { n.to_bits() }),
            CellValue::Text(_) | CellValue::RichText(_) => {
                let t = v.as_text().unwrap_or_default();
                if t.is_empty() {
                    ItemKey::Blank
                } else {
                    ItemKey::Text(t.to_lowercase())
                }
            }
            CellValue::Bool(b) => ItemKey::Bool(*b),
            CellValue::Error(e) => ItemKey::Error(e.code()),
        }
    }
}

/// One field of the cache.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CacheField {
    /// The field's name (its header).
    pub name: String,
    /// Distinct values in the order they first appear (rich text as plain text, empty text as
    /// blank).
    pub items: Vec<CellValue>,
    /// For each record, the index of its item.
    pub index: Vec<u32>,
    /// The number format of the field's first value (dates keep showing as dates).
    #[serde(default)]
    pub num_fmt: Option<String>,
}

impl CacheField {
    /// Whether every non-blank item is a number (a new value field sums it, otherwise counts).
    pub fn numeric_only(&self) -> bool {
        self.items
            .iter()
            .all(|v| matches!(v, CellValue::Number(_) | CellValue::Blank))
            && self.items.iter().any(|v| matches!(v, CellValue::Number(_)))
    }

    /// The value of a record.
    pub fn value(&self, record: usize) -> &CellValue {
        static BLANK: CellValue = CellValue::Blank;
        self.index
            .get(record)
            .and_then(|&i| self.items.get(i as usize))
            .unwrap_or(&BLANK)
    }

    /// The index of an item by value.
    pub fn find(&self, v: &CellValue) -> Option<u32> {
        let k = ItemKey::of(v);
        self.items
            .iter()
            .position(|x| ItemKey::of(x) == k)
            .and_then(|i| u32::try_from(i).ok())
    }

    /// Puts the items in the order of `order` (indices of the current items); the items it leaves
    /// out follow in their current order. The records keep their values.
    pub fn reorder(&mut self, order: &[u32]) {
        let n = self.items.len();
        let mut seen = vec![false; n];
        let mut perm: Vec<u32> = Vec::with_capacity(n);
        for &o in order {
            if (o as usize) < n && !seen[o as usize] {
                seen[o as usize] = true;
                perm.push(o);
            }
        }
        for (i, s) in seen.iter().enumerate() {
            if !s {
                perm.push(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        if perm.iter().enumerate().all(|(i, &o)| i == o as usize) {
            return;
        }
        let mut new_of = vec![0u32; n];
        for (new, &old) in perm.iter().enumerate() {
            new_of[old as usize] = u32::try_from(new).unwrap_or(u32::MAX);
        }
        self.items = perm
            .iter()
            .map(|&o| self.items[o as usize].clone())
            .collect();
        for x in &mut self.index {
            *x = new_of[*x as usize];
        }
    }

    /// Keeps a manual (or data source) order across a refresh, as Excel does: the items that were
    /// in `prior` come first, in its order, and items new to the field follow in the order they
    /// first appear in the source.
    pub fn keep_order(&mut self, prior: &[CellValue]) {
        let at: FxHashMap<ItemKey, u32> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, v)| (ItemKey::of(v), u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();
        let order: Vec<u32> = prior
            .iter()
            .filter_map(|v| at.get(&ItemKey::of(v)).copied())
            .collect();
        self.reorder(&order);
    }
}

/// The source of a PivotTable as of its last refresh.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PivotCache {
    /// Fields, in source column order.
    pub fields: Vec<CacheField>,
    /// The number of records (source data rows).
    pub records: u32,
}

/// A builder that interns one field's values.
#[derive(Default)]
struct FieldBuilder {
    items: Vec<CellValue>,
    keys: FxHashMap<ItemKey, u32>,
    index: Vec<u32>,
}

impl FieldBuilder {
    fn push(&mut self, v: CellValue) {
        let v = match v {
            CellValue::RichText(r) => CellValue::Text(r.plain().clone()),
            CellValue::Text(t) if t.is_empty() => CellValue::Blank,
            other => other,
        };
        let key = ItemKey::of(&v);
        let next = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        let i = *self.keys.entry(key).or_insert_with(|| {
            self.items.push(v);
            next
        });
        self.index.push(i);
    }
}

/// Field names from a header row: blanks are not allowed, duplicates get a number (`Amount2`).
pub fn field_names(headers: &[CellValue]) -> Result<Vec<String>, PivotError> {
    let mut out: Vec<String> = Vec::with_capacity(headers.len());
    for h in headers {
        let text = h.display_general();
        let text = text.trim();
        if text.is_empty() {
            return Err(PivotError::BadSource(
                "The PivotTable field name is not valid. To create a PivotTable report, you must \
                 use data that is organized as a list with labeled columns. If you are changing \
                 the name of a PivotTable field, you must type a new name for the field."
                    .into(),
            ));
        }
        let mut name = text.to_owned();
        let mut n = 2;
        while out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            name = format!("{text}{n}");
            n += 1;
        }
        out.push(name);
    }
    Ok(out)
}

impl PivotCache {
    /// Builds a cache from rows of values (the first row the headers).
    pub fn from_rows(
        headers: &[CellValue],
        rows: &[Vec<CellValue>],
    ) -> Result<PivotCache, PivotError> {
        let names = field_names(headers)?;
        let mut builders: Vec<FieldBuilder> =
            names.iter().map(|_| FieldBuilder::default()).collect();
        for r in rows {
            for (i, b) in builders.iter_mut().enumerate() {
                b.push(r.get(i).cloned().unwrap_or_default());
            }
        }
        Ok(PivotCache {
            records: u32::try_from(rows.len()).unwrap_or(u32::MAX),
            fields: names
                .into_iter()
                .zip(builders)
                .map(|(name, b)| CacheField {
                    name,
                    items: b.items,
                    index: b.index,
                    num_fmt: None,
                })
                .collect(),
        })
    }

    /// Reads a source from the workbook.
    pub fn read(wb: &Workbook, source: &PivotSource) -> Result<PivotCache, PivotError> {
        let (sid, range, header) = resolve(wb, source)?;
        let sheet = wb
            .sheet(sid)
            .ok_or_else(|| PivotError::BadSource("The source sheet no longer exists.".into()))?;
        let headers: Vec<CellValue> = (range.c0..=range.c1)
            .map(|c| wb.value(sid, header, c))
            .collect();
        let names = field_names(&headers)?;
        let mut builders: Vec<FieldBuilder> =
            names.iter().map(|_| FieldBuilder::default()).collect();
        let mut fmts: Vec<Option<String>> = vec![None; names.len()];
        let data_r0 = header + 1;
        if data_r0 <= range.r1 {
            for row in data_r0..=range.r1 {
                for (i, b) in builders.iter_mut().enumerate() {
                    let col = range.c0 + u32::try_from(i).unwrap_or(0);
                    let v = wb.value(sid, row, col);
                    if fmts[i].is_none() && !v.is_blank() {
                        let code = wb.styles().num_fmt(sheet.style_at(row, col));
                        fmts[i] = Some(code.to_owned());
                    }
                    b.push(v);
                }
            }
        }
        let records = range.r1.saturating_sub(header);
        Ok(PivotCache {
            records,
            fields: names
                .into_iter()
                .zip(builders)
                .zip(fmts)
                .map(|((name, b), fmt)| CacheField {
                    name,
                    items: b.items,
                    index: b.index,
                    num_fmt: fmt.filter(|f| !f.eq_ignore_ascii_case("General")),
                })
                .collect(),
        })
    }

    /// The number of records.
    pub fn len(&self) -> usize {
        self.records as usize
    }

    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.records == 0
    }
}

/// Splits `Sheet1!$A$1:$D$9` (or `'My sheet'!A:D`) into the sheet name and the range.
pub fn split_reference(text: &str) -> Option<(Option<String>, Range)> {
    let t = text.trim().trim_start_matches('=');
    let (sheet, cells) = match t.rfind('!') {
        Some(i) => {
            let s = &t[..i];
            let name = if let Some(q) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                q.replace("''", "'")
            } else {
                s.to_owned()
            };
            (Some(name), &t[i + 1..])
        }
        None => (None, t),
    };
    Range::parse_a1(cells).map(|r| (sheet, r))
}

/// The sheet, the range (headers included, whole columns clipped to the used rows) and the
/// header row of a source.
pub fn resolve(wb: &Workbook, source: &PivotSource) -> Result<(SheetId, Range, u32), PivotError> {
    let bad = |s: &str| PivotError::BadSource(s.to_owned());
    match source {
        PivotSource::Table { name } => {
            let (sid, t) = wb
                .find_table(name)
                .ok_or_else(|| bad("Reference isn't valid: the source table no longer exists."))?;
            if !t.header_row {
                return Err(bad(
                    "The PivotTable field name is not valid: the source table has no header row.",
                ));
            }
            let r1 = if t.totals_row && t.range.r1 > t.range.r0 {
                t.range.r1 - 1
            } else {
                t.range.r1
            };
            Ok((
                sid,
                Range::new(t.range.r0, t.range.c0, r1, t.range.c1),
                t.range.r0,
            ))
        }
        PivotSource::Range { sheet, range } => {
            let s = wb
                .sheet_by_name(sheet)
                .ok_or_else(|| bad("Reference isn't valid: the source sheet no longer exists."))?;
            let r = clip(s, *range)?;
            Ok((s.id(), r, r.r0))
        }
        PivotSource::Name { name } => {
            let n = wb
                .names()
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(name) && n.scope.is_none())
                .or_else(|| {
                    wb.names()
                        .iter()
                        .find(|n| n.name.eq_ignore_ascii_case(name))
                })
                .ok_or_else(|| bad("Reference isn't valid: the source name no longer exists."))?;
            let (sheet, range) = split_reference(&n.formula)
                .ok_or_else(|| bad("Reference isn't valid: the source name is not a range."))?;
            let s = match sheet {
                Some(sheet) => wb.sheet_by_name(&sheet),
                None => n.scope.and_then(|id| wb.sheet(id)),
            }
            .ok_or_else(|| bad("Reference isn't valid: the source sheet no longer exists."))?;
            let r = clip(s, range)?;
            Ok((s.id(), r, r.r0))
        }
    }
}

fn clip(s: &crate::Sheet, r: Range) -> Result<Range, PivotError> {
    let r = if r.is_whole_cols() || r.is_whole_rows() {
        let used = s.cells().used_range(false).ok_or_else(|| {
            PivotError::BadSource("Reference isn't valid: the source range is empty.".into())
        })?;
        r.intersection(&used).ok_or_else(|| {
            PivotError::BadSource("Reference isn't valid: the source range is empty.".into())
        })?
    } else {
        r
    };
    if r.width() > 16_384 {
        return Err(PivotError::BadSource(
            "The PivotTable source has more fields than a PivotTable can use.".into(),
        ));
    }
    Ok(r)
}
