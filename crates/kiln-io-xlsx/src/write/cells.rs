// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `sheetData` writer: rows and cells straight into text (no tree), values from the store,
//! spilled values of dynamic arrays and legacy array formulas from the engine, strings into the
//! shared string table.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use kiln_ooxml::xml::{escape_attr, escape_text};
use kiln_sheet::axis::RowProps;
use kiln_sheet::store::CellKind;
use kiln_sheet::{CellValue, Range, Sheet, Workbook};
use rustc_hash::FxHashMap;

use crate::formula_text::to_file_formula;
use crate::source::SheetSource;
use crate::strings::SstBuilder;
use crate::text::{encode_x_escapes, fmt_num, push_cell_ref};

/// What the cell writer reports.
#[derive(Debug, Default)]
pub(crate) struct CellsOut {
    pub(crate) xml: String,
    /// The used range (cells and rows written).
    pub(crate) used: Option<Range>,
    /// A dynamic-array formula was written (the metadata part is needed).
    pub(crate) dynamic: bool,
}

/// Whether a formula evaluates differently as a pre-dynamic-array formula (ranges, arrays and
/// names can be implicitly intersected), so it must be saved as a dynamic array to keep its
/// meaning.
pub(crate) fn needs_array_semantics(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            in_str = !in_str;
        } else if !in_str {
            if c == b':' || c == b'{' || c == b'[' {
                return true;
            }
            if c.is_ascii_alphabetic() || c == b'_' || c == b'\\' {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'.' | b'\\' | b'$'))
                {
                    i += 1;
                }
                let word = &text[start..i];
                let mut j = i;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                let call = b.get(j) == Some(&b'(');
                let sheet = b.get(j) == Some(&b'!');
                let is_ref = crate::text::parse_cell_ref(word.as_bytes()).is_some();
                let logical =
                    word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE");
                if !(call || sheet || is_ref || logical) {
                    // A defined name (or a LET/LAMBDA parameter): may stand for a range.
                    return true;
                }
                continue;
            }
            if c == b'\'' {
                // A quoted sheet name: skip it.
                i += 1;
                while i < b.len() {
                    if b[i] == b'\'' {
                        if b.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
        }
        i += 1;
    }
    false
}

struct Spill {
    values: FxHashMap<(u32, u32), CellValue>,
}

/// Writes `sheetData`.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn write_cells(
    wb: &Workbook,
    sheet: &Sheet,
    src: Option<&SheetSource>,
    sst: &mut SstBuilder,
    dynamic_cm: Option<u32>,
    future: &BTreeSet<String>,
    xf_count: u32,
) -> CellsOut {
    let id = sheet.id();
    let store = sheet.cells();
    // Values that formulas spill (or fill, for legacy arrays) into cells the store does not hold.
    let mut spill = Spill {
        values: FxHashMap::default(),
    };
    let mut anchors: FxHashMap<(u32, u32), Range> = FxHashMap::default();
    for (r, c, d) in store.iter() {
        if d.kind.is_engine_formula()
            && let Some(sr) = wb.spill_range(id, r, c)
            && !sr.blocked
        {
            let b = sr.bounds;
            let rg = Range::new(b.r0, b.c0, b.r1, b.c1);
            anchors.insert((r, c), rg);
            if rg.cell_count() > 1 && rg.cell_count() <= 4_000_000 {
                for row in rg.r0..=rg.r1 {
                    for col in rg.c0..=rg.c1 {
                        if (row, col) != (r, c) {
                            spill.values.insert((row, col), wb.value(id, row, col));
                        }
                    }
                }
            }
        }
    }
    // Rows to write: stored cells, spilled cells, rows with properties or kept attributes.
    let mut rows: Vec<u32> = store.rows().to_vec();
    let layout_rows: Vec<(u32, RowProps)> = sheet.rows().to_vec();
    let mut row_props: FxHashMap<u32, RowProps> = FxHashMap::default();
    for (r, p) in &layout_rows {
        row_props.insert(*r, *p);
        rows.push(*r);
    }
    let empty_attrs = FxHashMap::default();
    let row_attrs = src.map_or(&empty_attrs, |s| &s.row_attrs);
    rows.extend(row_attrs.keys().copied());
    let mut spill_by_row: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for &(r, c) in spill.values.keys() {
        spill_by_row.entry(r).or_default().push(c);
        rows.push(r);
    }
    rows.sort_unstable();
    rows.dedup();

    let mut out = CellsOut {
        xml: String::with_capacity(64 + store.len() * 40),
        ..CellsOut::default()
    };
    let mut used: Option<Range> = None;
    let x = &mut out.xml;
    x.push_str("<sheetData>");
    let mut cols: Vec<u32> = Vec::new();
    for &r in &rows {
        cols.clear();
        if let Some(ch) = store.chunk(r) {
            cols.extend(ch.cols().iter().map(|&c| u32::from(c)));
        }
        if let Some(extra) = spill_by_row.get(&r) {
            cols.extend(extra.iter().copied());
            cols.sort_unstable();
            cols.dedup();
        }
        let props = row_props.get(&r);
        let attrs = row_attrs.get(&r);
        // A row with only kept attributes and no cells is dropped (Excel recreates it).
        if cols.is_empty() && props.is_none() {
            continue;
        }
        x.push_str("<row r=\"");
        let _ = write!(x, "{}", u64::from(r) + 1);
        x.push('"');
        if let (Some(&a), Some(&b)) = (cols.first(), cols.last()) {
            let _ = write!(x, " spans=\"{}:{}\"", a + 1, b + 1);
            let row_range = Range::new(r, a, r, b);
            used = Some(used.map_or(row_range, |u| u.union(&row_range)));
        }
        if let Some(p) = props {
            if let Some(s) = p.style.filter(|s| *s < xf_count) {
                let _ = write!(x, " s=\"{s}\" customFormat=\"1\"");
            }
            if let Some(h) = p.height {
                let _ = write!(x, " ht=\"{}\"", fmt_num(h.0));
            }
            if p.hidden || p.filtered {
                x.push_str(" hidden=\"1\"");
            }
            if p.custom_height {
                x.push_str(" customHeight=\"1\"");
            }
            if p.outline_level > 0 {
                let _ = write!(x, " outlineLevel=\"{}\"", p.outline_level);
            }
            if p.collapsed {
                x.push_str(" collapsed=\"1\"");
            }
            if p.thick_top {
                x.push_str(" thickTop=\"1\"");
            }
            if p.thick_bot {
                x.push_str(" thickBot=\"1\"");
            }
        }
        if let Some(attrs) = attrs {
            for (k, v) in attrs {
                let _ = write!(x, " {k}=\"{}\"", escape_attr(v));
            }
        }
        if cols.is_empty() {
            x.push_str("/>");
            continue;
        }
        x.push('>');
        for &c in &cols {
            let data = store.get(r, c);
            let style = data.map_or(0, |d| d.style).min(xf_count.saturating_sub(1));
            let extra = src.and_then(|s| s.cells.get(&(r, c)));
            let kind = data.map_or(CellKind::Value, |d| d.kind);
            let (value, formula) = match data {
                Some(d) if d.kind.is_formula() => {
                    let v = if d.kind.is_engine_formula() {
                        wb.value(id, r, c)
                    } else {
                        d.value.clone()
                    };
                    (v, wb.formula(id, r, c))
                }
                Some(d) if !d.value.is_blank() => (d.value.clone(), None),
                _ => (
                    spill
                        .values
                        .get(&(r, c))
                        .cloned()
                        .unwrap_or(CellValue::Blank),
                    None,
                ),
            };
            x.push_str("<c r=\"");
            push_cell_ref(x, r, c);
            x.push('"');
            if style != 0 {
                let _ = write!(x, " s=\"{style}\"");
            }
            let t = match &value {
                CellValue::Blank | CellValue::Number(_) => None,
                CellValue::Bool(_) => Some("b"),
                CellValue::Error(_) => Some("e"),
                CellValue::Text(_) if formula.is_some() => Some("str"),
                CellValue::Text(_) | CellValue::RichText(_) => {
                    if formula.is_some() {
                        Some("str")
                    } else {
                        Some("s")
                    }
                }
            };
            if let Some(t) = t {
                let _ = write!(x, " t=\"{t}\"");
            }
            // Formula form: legacy, legacy array (CSE) or dynamic array.
            let mut f_open = String::new();
            if let Some(text) = &formula {
                let legacy = kind == CellKind::LegacyFormula;
                let cse = src.and_then(|s| s.array_formulas.get(&(r, c))).copied();
                let was_dynamic = src.is_some_and(|s| s.dynamic.contains(&(r, c)));
                let spill_rg = anchors.get(&(r, c)).copied();
                let file_text = match extra.and_then(|e| e.file_formula.as_ref()) {
                    Some((file, model)) if model == text => file.clone(),
                    _ => to_file_formula(text, Some(future)),
                };
                f_open.push_str("<f");
                if kind == CellKind::RawFormula || legacy {
                    // As read (a raw formula keeps its own text).
                } else if let Some(rg) = cse {
                    let _ = write!(f_open, " t=\"array\" ref=\"{}\"", range_text(&rg));
                } else if spill_rg.is_some_and(|g| g.cell_count() > 1)
                    || was_dynamic
                    || needs_array_semantics(text)
                {
                    let rg = spill_rg.unwrap_or(Range::cell(r, c));
                    let _ = write!(f_open, " t=\"array\" ref=\"{}\"", range_text(&rg));
                    if let Some(cm) = dynamic_cm {
                        let _ = write!(x, " cm=\"{cm}\"");
                        out.dynamic = true;
                    }
                }
                if let Some(e) = extra {
                    for (k, v) in &e.f_attrs {
                        let _ = write!(f_open, " {k}=\"{}\"", escape_attr(v));
                    }
                }
                f_open.push('>');
                f_open.push_str(&escape_text(&file_text));
                f_open.push_str("</f>");
            }
            if let Some(e) = extra {
                if let Some(vm) = &e.vm {
                    let _ = write!(x, " vm=\"{}\"", escape_attr(vm));
                }
                if e.ph {
                    x.push_str(" ph=\"1\"");
                }
            }
            let data_table = extra
                .and_then(|e| e.data_table.as_ref())
                .filter(|_| formula.is_none());
            let has_v = !value.is_blank();
            if !has_v && formula.is_none() && data_table.is_none() {
                x.push_str("/>");
                continue;
            }
            x.push('>');
            x.push_str(&f_open);
            if let Some(dt) = data_table {
                let opts = kiln_ooxml::xml::WriteOptions {
                    declaration: kiln_ooxml::xml::Declaration::Omit,
                    declare_namespaces: false,
                };
                if let Ok(s) = kiln_ooxml::xml::write_element(dt, &opts) {
                    x.push_str(&s);
                }
            }
            match &value {
                CellValue::Blank => {}
                CellValue::Number(n) => {
                    x.push_str("<v>");
                    x.push_str(&fmt_num(*n));
                    x.push_str("</v>");
                }
                CellValue::Bool(b) => x.push_str(if *b { "<v>1</v>" } else { "<v>0</v>" }),
                CellValue::Error(e) => {
                    x.push_str("<v>");
                    x.push_str(&escape_text(e.code()));
                    x.push_str("</v>");
                }
                CellValue::Text(s) if formula.is_some() => {
                    x.push_str("<v>");
                    x.push_str(&escape_text(&encode_x_escapes(s)));
                    x.push_str("</v>");
                }
                CellValue::RichText(rt) if formula.is_some() => {
                    x.push_str("<v>");
                    x.push_str(&escape_text(&encode_x_escapes(rt.plain())));
                    x.push_str("</v>");
                }
                v @ (CellValue::Text(_) | CellValue::RichText(_)) => {
                    let i = sst.index(v);
                    let _ = write!(x, "<v>{i}</v>");
                }
            }
            x.push_str("</c>");
        }
        x.push_str("</row>");
    }
    x.push_str("</sheetData>");
    out.used = used;
    out
}

pub(crate) fn range_text(r: &Range) -> String {
    let mut s = String::with_capacity(12);
    push_cell_ref(&mut s, r.r0, r.c0);
    if !r.is_single_cell() {
        s.push(':');
        push_cell_ref(&mut s, r.r1, r.c1);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_semantics_are_detected() {
        for (f, want) in [
            ("A1+B2*2", false),
            ("SUM(A1,B2)", false),
            ("IF(TRUE,1,2)", false),
            ("Data!A1*2", false),
            ("'My Sheet'!A1*2", false),
            ("\"a:b\"&A1", false),
            ("SUM(A1:A3*B1:B3)", true),
            ("SUM(Total)", true),
            ("{1,2}*2", true),
            ("Table1[Col]", true),
        ] {
            assert_eq!(needs_array_semantics(f), want, "{f}");
        }
    }
}
