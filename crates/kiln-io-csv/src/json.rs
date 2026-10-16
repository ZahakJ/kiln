// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON ↔ tables. Importing an array of objects gives a sheet whose header row holds the keys (in
//! the order they first appear) and one row per object, formatted as a table; arrays of arrays
//! become plain rows. Exporting writes a sheet's first table (or its used range with the first
//! row as keys) as an array of objects.

use std::fmt;

use kiln_sheet::meta::{Table, TableColumn, TableStyleInfo};
use kiln_sheet::{CellValue, Range, SheetId, Workbook};
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use crate::{CsvError, ImportOptions, import::ImportReport};

/// A JSON value whose objects keep their key order.
#[derive(Debug, Clone, PartialEq)]
enum Ordered {
    Scalar(Value),
    Array(Vec<Ordered>),
    Object(Vec<(String, Ordered)>),
}

impl<'de> de::Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::Bool(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::from(v)))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::from(v)))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::from(v)))
            }
            fn visit_str<E>(self, v: &str) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::String(v.to_owned())))
            }
            fn visit_string<E>(self, v: String) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::String(v)))
            }
            fn visit_unit<E>(self) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::Null))
            }
            fn visit_none<E>(self) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Ordered, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = s.next_element()? {
                    v.push(x);
                }
                Ok(Ordered::Array(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Ordered, A::Error> {
                let mut v: Vec<(String, Ordered)> = Vec::new();
                while let Some((k, x)) = m.next_entry::<String, Ordered>()? {
                    match v.iter_mut().find(|(e, _)| *e == k) {
                        Some(slot) => slot.1 = x,
                        None => v.push((k, x)),
                    }
                }
                Ok(Ordered::Object(v))
            }
        }
        d.deserialize_any(V)
    }
}

fn ordered_text(o: &Ordered) -> String {
    fn to_value(o: &Ordered) -> Value {
        match o {
            Ordered::Scalar(v) => v.clone(),
            Ordered::Array(a) => Value::Array(a.iter().map(to_value).collect()),
            Ordered::Object(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), to_value(v))).collect())
            }
        }
    }
    to_value(o).to_string()
}

fn field(o: &Ordered) -> String {
    match o {
        Ordered::Scalar(Value::Null) => String::new(),
        Ordered::Scalar(Value::String(s)) => s.clone(),
        Ordered::Scalar(v) => v.to_string(),
        other => ordered_text(other),
    }
}

fn cell(o: &Ordered) -> CellValue {
    match o {
        Ordered::Scalar(Value::Null) => CellValue::Blank,
        Ordered::Scalar(Value::Bool(b)) => CellValue::Bool(*b),
        Ordered::Scalar(Value::Number(n)) => n.as_f64().map_or(CellValue::Blank, CellValue::number),
        Ordered::Scalar(Value::String(s)) => CellValue::text(s.as_str()),
        other => CellValue::text(ordered_text(other)),
    }
}

/// Reads JSON (an array of objects, or of arrays) into a one-sheet workbook.
///
/// # Errors
///
/// [`CsvError::Json`] for text that is not JSON or not an array.
pub fn read_json(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<(Workbook, ImportReport), CsvError> {
    let text = crate::encoding::decode(bytes, crate::encoding::detect(bytes));
    let root: Ordered = serde_json::from_str(&text).map_err(|e| CsvError::Json(e.to_string()))?;
    let Ordered::Array(items) = root else {
        return Err(CsvError::Json("the file is not a JSON array".into()));
    };
    let objects = !items.is_empty() && items.iter().all(|i| matches!(i, Ordered::Object(_)));
    let mut keys: Vec<String> = Vec::new();
    if objects {
        for i in &items {
            if let Ordered::Object(m) = i {
                for (k, _) in m {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
    }
    // Build the records as text for the shape, then set typed values directly.
    let mut records: Vec<Vec<String>> = Vec::new();
    if objects {
        records.push(keys.clone());
    }
    for i in &items {
        match i {
            Ordered::Object(m) => records.push(
                keys.iter()
                    .map(|k| {
                        m.iter()
                            .find(|(x, _)| x == k)
                            .map(|(_, v)| field(v))
                            .unwrap_or_default()
                    })
                    .collect(),
            ),
            Ordered::Array(a) => records.push(a.iter().map(field).collect()),
            scalar => records.push(vec![field(scalar)]),
        }
    }
    let text_opts = ImportOptions {
        as_text: true,
        ..options.clone()
    };
    let (mut wb, report) = crate::import::records_to_workbook(&records, &text_opts)?;
    let sheet = wb
        .first_sheet()
        .ok_or_else(|| CsvError::Json("no sheet".into()))?;
    // JSON values carry their types: numbers and logicals are set as such.
    let start = u32::from(objects);
    for (r, i) in items.iter().enumerate() {
        let row = start + r as u32;
        let values: Vec<CellValue> = match i {
            Ordered::Object(m) => keys
                .iter()
                .map(|k| {
                    m.iter()
                        .find(|(x, _)| x == k)
                        .map_or(CellValue::Blank, |(_, v)| cell(v))
                })
                .collect(),
            Ordered::Array(a) => a.iter().map(cell).collect(),
            scalar => vec![cell(scalar)],
        };
        for (c, v) in values.into_iter().enumerate() {
            if matches!(v, CellValue::Number(_) | CellValue::Bool(_)) {
                wb.load_value(sheet, row, c as u32, v)?;
            }
        }
    }
    if objects && !keys.is_empty() {
        let name = wb.next_table_name();
        let t = Table {
            id: 1,
            name,
            display_name: None,
            range: Range::new(0, 0, items.len() as u32, keys.len() as u32 - 1),
            header_row: true,
            totals_row: false,
            columns: keys.iter().map(TableColumn::named).collect(),
            style: Some(TableStyleInfo::default()),
            auto_filter: Some(Default::default()),
            comment: None,
        };
        let mut meta = wb
            .sheet(sheet)
            .map(|s| s.meta().clone())
            .unwrap_or_default();
        meta.tables.push(t);
        wb.load_sheet_meta(sheet, meta)?;
    }
    wb.recalc_all();
    wb.take_changes();
    Ok((wb, report))
}

fn json_value(v: &CellValue) -> Value {
    match v {
        CellValue::Blank => Value::Null,
        CellValue::Number(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Error(e) => Value::String(e.code().to_owned()),
        CellValue::Text(t) => Value::String(t.to_string()),
        CellValue::RichText(r) => Value::String(r.plain().to_string()),
    }
}

/// Writes a sheet's first table (or its used range, first row as keys) as an array of objects.
#[must_use]
pub fn write_json(wb: &Workbook, sheet: SheetId) -> Vec<u8> {
    let Some(s) = wb.sheet(sheet) else {
        return b"[]".to_vec();
    };
    let range = s
        .tables()
        .first()
        .map(|t| t.range)
        .or_else(|| crate::export::export_range(wb, sheet));
    let Some(range) = range else {
        return b"[]".to_vec();
    };
    let keys: Vec<String> = (range.c0..=range.c1)
        .map(|c| crate::export::cell_text(wb, sheet, range.r0, c, true))
        .collect();
    let mut out = String::from("[");
    for r in range.r0 + 1..=range.r1 {
        if r > range.r0 + 1 {
            out.push(',');
        }
        out.push_str("\n  {");
        for (i, c) in (range.c0..=range.c1).enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let key = keys.get(i).cloned().unwrap_or_default();
            out.push_str(&Value::String(key).to_string());
            out.push_str(": ");
            out.push_str(&json_value(&wb.value(sheet, r, c)).to_string());
        }
        out.push('}');
    }
    out.push_str("\n]\n");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrays_of_objects_become_tables_and_back() {
        let json = br#"[{"name":"Ann","age":41,"vip":true},{"age":7,"name":"Bo","tags":["x"]},{"name":null}]"#;
        let (wb, report) = read_json(json, &ImportOptions::default()).expect("json");
        assert_eq!(report.rows, 4);
        let s = wb.first_sheet().expect("sheet");
        assert_eq!(wb.value(s, 0, 0), CellValue::text("name"));
        assert_eq!(wb.value(s, 0, 3), CellValue::text("tags"));
        assert_eq!(wb.value(s, 1, 1), CellValue::Number(41.0));
        assert_eq!(wb.value(s, 1, 2), CellValue::Bool(true));
        assert_eq!(wb.value(s, 2, 3), CellValue::text("[\"x\"]"));
        assert_eq!(wb.value(s, 3, 0), CellValue::Blank);
        let t = &wb.sheet(s).expect("sheet").tables()[0];
        assert_eq!(t.range, Range::new(0, 0, 3, 3));
        let out = String::from_utf8(write_json(&wb, s)).expect("utf8");
        let back: Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(back[0]["name"], Value::String("Ann".into()));
        assert_eq!(back[0]["age"], serde_json::json!(41.0));
        assert_eq!(back[2]["name"], Value::Null);
    }

    #[test]
    fn refuses_what_is_not_an_array() {
        assert!(read_json(b"{\"a\":1}", &ImportOptions::default()).is_err());
        assert!(read_json(b"[1,", &ImportOptions::default()).is_err());
        let (wb, _) = read_json(b"[[1,\"a\"],[2]]", &ImportOptions::default()).expect("rows");
        let s = wb.first_sheet().expect("sheet");
        assert_eq!(wb.value(s, 1, 0), CellValue::Number(2.0));
        assert!(wb.sheet(s).expect("sheet").tables().is_empty());
    }
}
