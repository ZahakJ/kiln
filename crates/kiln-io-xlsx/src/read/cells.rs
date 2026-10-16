// SPDX-License-Identifier: AGPL-3.0-or-later
//! The streaming `sheetData` reader: rows and cells straight from `quick-xml` events into cell
//! records, without building a tree. Shared formulas are expanded per cell, legacy array formulas
//! and dynamic arrays are recognised (the cells they cover keep only their formats, since the
//! engine fills them), and every attribute the model has no field for is kept for the writer.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::Arc;

use kiln_formula::ErrorValue;
use kiln_sheet::axis::RowProps;
use kiln_sheet::num::F64;
use kiln_sheet::{CellValue, Range};
use quick_xml::events::Event;
use rustc_hash::{FxHashMap, FxHashSet};

use super::scan::{Attrs, Scanner, Tok, local, unescape, unescape_attr};
use super::stream::{resolve_ref, xml_error};
use crate::XlsxError;
use crate::formula_text::from_file_formula;
use crate::source::CellExtra;
use crate::strings::parse_string_item;
use crate::text::{decode_x_escapes, parse_bool, parse_cell_ref};

/// What the reader needs to know while streaming a sheet.
pub(crate) struct CellContext<'a> {
    pub(crate) part: &'a str,
    pub(crate) sst: &'a [CellValue],
    pub(crate) xf_count: u32,
    pub(crate) dynamic_cm: &'a FxHashSet<u32>,
    pub(crate) has_metadata: bool,
    pub(crate) date1904: bool,
}

/// One cell as read.
pub(crate) type ReadCell = kiln_sheet::workbook::CellRecord;

/// The result of streaming a worksheet.
#[derive(Debug, Default)]
pub(crate) struct SheetCells {
    pub(crate) cells: Vec<ReadCell>,
    pub(crate) rows: Vec<(u32, RowProps)>,
    pub(crate) row_attrs: FxHashMap<u32, Vec<(String, String)>>,
    pub(crate) extras: FxHashMap<(u32, u32), CellExtra>,
    pub(crate) array_formulas: FxHashMap<(u32, u32), Range>,
    pub(crate) dynamic: FxHashSet<(u32, u32)>,
    /// Byte span of the `sheetData` element in the part text.
    pub(crate) span: Option<(usize, usize)>,
    pub(crate) warnings: Vec<String>,
    pub(crate) future_functions: BTreeSet<String>,
}

/// The `t` attribute of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CellType {
    #[default]
    Number,
    Shared,
    Str,
    Inline,
    Bool,
    Error,
    Date,
}

impl CellType {
    fn parse(v: &str) -> CellType {
        match v {
            "s" => CellType::Shared,
            "str" => CellType::Str,
            "inlineStr" => CellType::Inline,
            "b" => CellType::Bool,
            "e" => CellType::Error,
            "d" => CellType::Date,
            _ => CellType::Number,
        }
    }
}

#[derive(Default)]
struct RawCell<'a> {
    row: Option<u32>,
    col: Option<u32>,
    t: CellType,
    s: u32,
    cm: Option<u32>,
    vm: Option<String>,
    ph: bool,
    v: Option<Cow<'a, str>>,
    f: Option<RawFormula>,
    is: Option<Inline>,
}

/// An inline string as read: plain text (interned when the cell is finished) or rich text.
enum Inline {
    Plain(String),
    Rich(CellValue),
}

#[derive(Default)]
struct RawFormula {
    t: Option<String>,
    r#ref: Option<String>,
    si: Option<String>,
    text: String,
    other: Vec<(String, String)>,
    whole: Option<kiln_ooxml::xml::XmlElement>,
}

/// Days from 1899-12-30 (1900 system) or 1904-01-01 to an ISO date, plus the time fraction.
pub(crate) fn iso_to_serial(text: &str, date1904: bool) -> Option<f64> {
    let t = text.trim();
    let (date, time) = t.split_once('T').unwrap_or((t, ""));
    let mut p = date.split('-');
    let y: i64 = p.next()?.parse().ok()?;
    let m: i64 = p.next()?.parse().ok()?;
    let d: i64 = p.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days since 1970-01-01 (civil from days, Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let base = if date1904 { -24_107 } else { -25_569 };
    let mut serial = (days - base) as f64;
    let time = time.trim_end_matches('Z');
    if !time.is_empty() {
        let mut tp = time.split(':');
        let h: f64 = tp.next()?.parse().ok()?;
        let mi: f64 = tp.next().unwrap_or("0").parse().ok()?;
        let s: f64 = tp.next().unwrap_or("0").parse().ok()?;
        serial += (h * 3600.0 + mi * 60.0 + s) / 86_400.0;
    }
    // Before 1900-03-01, Excel's serials are one lower than the calendar (its 1900-02-29).
    if !date1904 && serial < 61.0 {
        serial -= 1.0;
    }
    Some(serial)
}

fn error_value(text: &str) -> ErrorValue {
    ErrorValue::from_code(text.trim()).unwrap_or(ErrorValue::Value)
}

fn parse_range(text: &str) -> Option<Range> {
    let t = text.trim();
    match t.split_once(':') {
        Some((a, b)) => {
            let (r0, c0) = parse_cell_ref(a.as_bytes())?;
            let (r1, c1) = parse_cell_ref(b.as_bytes())?;
            Some(Range::new(r0, c0, r1, c1))
        }
        None => parse_cell_ref(t.as_bytes()).map(|(r, c)| Range::cell(r, c)),
    }
}

type Attr<'a> = (&'a str, Cow<'a, str>);

/// Attributes of a start tag, values resolved.
fn attrs<'a>(raw: &'a str, part: &str) -> Result<Vec<Attr<'a>>, XlsxError> {
    let mut out = Vec::with_capacity(4);
    for kv in Attrs::new(raw) {
        let (k, v) = kv.map_err(|m| err(part, m))?;
        out.push((k, unescape_attr(v).map_err(|m| err(part, m))?));
    }
    Ok(out)
}

fn err(part: &str, message: impl Into<String>) -> XlsxError {
    XlsxError::Xml {
        part: part.to_owned(),
        message: message.into(),
    }
}

fn read_row_attrs(
    raw: &str,
    part: &str,
    next_row: u32,
    out: &mut SheetCells,
) -> Result<u32, XlsxError> {
    let mut row = next_row;
    let mut props = RowProps::default();
    let mut any = false;
    let mut custom_format = false;
    let mut style = None;
    let mut extra = Vec::new();
    for (k, v) in attrs(raw, part)? {
        match k {
            "r" => {
                if let Some(r) = v
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|r| (1..=1_048_576).contains(r))
                {
                    row = r - 1;
                }
            }
            "spans" => {}
            "ht" => {
                if let Some(h) = v
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|h| h.is_finite() && *h >= 0.0)
                {
                    props.height = Some(F64(h));
                    any = true;
                }
            }
            "customHeight" => {
                props.custom_height = parse_bool(&v).unwrap_or(false);
                any |= props.custom_height;
            }
            "hidden" => {
                props.hidden = parse_bool(&v).unwrap_or(false);
                any |= props.hidden;
            }
            "outlineLevel" => {
                props.outline_level = v.trim().parse::<u8>().unwrap_or(0).min(7);
                any |= props.outline_level > 0;
            }
            "collapsed" => {
                props.collapsed = parse_bool(&v).unwrap_or(false);
                any |= props.collapsed;
            }
            "thickTop" => {
                props.thick_top = parse_bool(&v).unwrap_or(false);
                any |= props.thick_top;
            }
            "thickBot" => {
                props.thick_bot = parse_bool(&v).unwrap_or(false);
                any |= props.thick_bot;
            }
            "customFormat" => custom_format = parse_bool(&v).unwrap_or(false),
            "s" => style = v.trim().parse::<u32>().ok(),
            k => extra.push((k.to_owned(), v.into_owned())),
        }
    }
    if custom_format && let Some(s) = style {
        props.style = Some(s);
        any = true;
    }
    if any {
        out.rows.push((row, props));
    }
    if !extra.is_empty() {
        out.row_attrs.insert(row, extra);
    }
    Ok(row)
}

/// Where `sheetData` starts: (offset of its start tag, offset after it, self-closing). The part
/// before it is read with the general reader (it is small); `None` when there is none.
fn find_sheet_data(text: &str, part: &str) -> Result<Option<(usize, usize, bool)>, XlsxError> {
    let mut r = super::stream::reader(text);
    loop {
        let pos = usize::try_from(r.buffer_position()).unwrap_or(usize::MAX);
        let ev = r.read_event().map_err(|e| xml_error(part, &r, e))?;
        match ev {
            Event::Start(s) if local(s.name().0) == "sheetData" => {
                let after = usize::try_from(r.buffer_position()).unwrap_or(usize::MAX);
                return Ok(Some((pos, after, false)));
            }
            Event::Empty(s) if local(s.name().0) == "sheetData" => {
                let after = usize::try_from(r.buffer_position()).unwrap_or(usize::MAX);
                return Ok(Some((pos, after, true)));
            }
            Event::DocType(_) => return Err(xml_error(part, &r, "DTDs are not allowed")),
            Event::GeneralRef(g) => {
                resolve_ref(part, &r, &g)?;
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

/// Streams a worksheet's text: cells, rows and the `sheetData` span.
pub(crate) fn read_sheet_cells(text: &str, ctx: &CellContext<'_>) -> Result<SheetCells, XlsxError> {
    let part = ctx.part;
    let mut out = SheetCells::default();
    let Some((tag, after, empty)) = find_sheet_data(text, part)? else {
        return Ok(out);
    };
    if empty {
        out.span = Some((tag, after));
        return Ok(out);
    }
    let mut sc = Scanner::new(text, after);
    // About 40 bytes of markup per cell in typical files.
    out.cells.reserve((text.len() - after) / 48);
    let mut next_row = 0u32;
    let mut cur_row = 0u32;
    let mut next_col = 0u32;
    let mut in_row = false;
    // si → (anchor row, anchor col, model text)
    let mut shared: FxHashMap<String, (u32, u32, String)> = FxHashMap::default();
    let mut covered: Vec<(Range, (u32, u32))> = Vec::new();
    let mut interned: FxHashSet<Arc<str>> = FxHashSet::default();
    loop {
        match sc.next().map_err(|m| err(part, m))? {
            Tok::Start {
                name,
                attrs: raw,
                empty,
            } => match local(name) {
                "row" if !in_row => {
                    cur_row = read_row_attrs(raw, part, next_row, &mut out)?;
                    next_row = cur_row.saturating_add(1);
                    next_col = 0;
                    in_row = !empty;
                    covered.retain(|(rg, _)| rg.r1 >= cur_row);
                }
                "c" if in_row => {
                    let mut c = start_cell(raw, cur_row, next_col, ctx, &mut out)?;
                    if !empty {
                        read_cell_body(part, &mut sc, name, &mut c)?;
                    }
                    next_col =
                        finish_cell(c, ctx, &mut out, &mut shared, &mut covered, &mut interned)?;
                }
                _ => {
                    if !empty {
                        sc.skip(name).map_err(|m| err(part, m))?;
                    }
                }
            },
            Tok::End { name } => match local(name) {
                "row" if in_row => in_row = false,
                "sheetData" if !in_row => {
                    out.span = Some((tag, sc.pos()));
                    return Ok(out);
                }
                other => return Err(err(part, format!("unexpected </{other}> in sheetData"))),
            },
            Tok::Text(t) => {
                if !t.bytes().all(|b| b.is_ascii_whitespace()) {
                    unescape(t).map_err(|m| err(part, m))?;
                }
            }
            Tok::CData(_) => {}
            Tok::Eof => return Err(err(part, "sheetData is not closed")),
        }
    }
}

fn start_cell<'a>(
    raw: &'a str,
    row: u32,
    next_col: u32,
    ctx: &CellContext<'_>,
    out: &mut SheetCells,
) -> Result<RawCell<'a>, XlsxError> {
    let mut c = RawCell {
        row: Some(row),
        col: Some(next_col),
        ..RawCell::default()
    };
    for kv in Attrs::new(raw) {
        let (k, raw_v) = kv.map_err(|m| err(ctx.part, m))?;
        let v = unescape_attr(raw_v).map_err(|m| err(ctx.part, m))?;
        match k {
            "r" => match parse_cell_ref(v.as_bytes()) {
                Some((rr, cc)) => {
                    c.row = Some(rr);
                    c.col = Some(cc);
                }
                None => {
                    out.warnings.push(format!(
                        "{}: a cell has the invalid reference \"{v}\"",
                        ctx.part
                    ));
                    c.row = None;
                }
            },
            "t" => c.t = CellType::parse(&v),
            "s" => {
                let s = v.trim().parse::<u32>().unwrap_or(0);
                c.s = if s < ctx.xf_count {
                    s
                } else {
                    out.warnings.push(format!(
                        "{}: a cell uses format {s}, which does not exist",
                        ctx.part
                    ));
                    0
                };
            }
            "cm" => c.cm = v.trim().parse::<u32>().ok(),
            "vm" => c.vm = Some(v.into_owned()),
            "ph" => c.ph = parse_bool(&v).unwrap_or(false),
            _ => {}
        }
    }
    Ok(c)
}

/// An inline string: plain `<t>` text without building a tree, rich runs through the tree.
fn inline_string(part: &str, sc: &mut Scanner<'_>, is_name: &str) -> Result<Inline, XlsxError> {
    let mut plain = String::new();
    let mut el: Option<kiln_ooxml::xml::XmlElement> = None;
    loop {
        match sc.next().map_err(|m| err(part, m))? {
            Tok::Start {
                name,
                attrs: a,
                empty,
            } => {
                if local(name) == "t" && el.is_none() {
                    if !empty {
                        plain.push_str(&sc.text_of(name).map_err(|m| err(part, m))?);
                    }
                    continue;
                }
                let child = if empty {
                    let mut e = kiln_ooxml::xml::XmlElement::new(local(name));
                    for (k, v) in attrs(a, part)? {
                        e.attrs.set(k, v.into_owned());
                    }
                    e
                } else {
                    sc.subtree(name, a).map_err(|m| err(part, m))?
                };
                let e = el.get_or_insert_with(|| {
                    let mut e = kiln_ooxml::xml::XmlElement::new("is");
                    if !plain.is_empty() {
                        e.children.push(kiln_ooxml::xml::XmlNode::Element(
                            kiln_ooxml::xml::XmlElement::new("t").with_text(plain.clone()),
                        ));
                    }
                    e
                });
                e.children.push(kiln_ooxml::xml::XmlNode::Element(child));
            }
            Tok::End { name } if name == is_name => {
                return Ok(match el {
                    Some(e) => Inline::Rich(parse_string_item(&e)),
                    None => Inline::Plain(plain),
                });
            }
            Tok::End { name } => return Err(err(part, format!("</{name}> closes <{is_name}>"))),
            Tok::Text(t) => {
                unescape(t).map_err(|m| err(part, m))?;
            }
            Tok::CData(_) => {}
            Tok::Eof => return Err(err(part, "an inline string is not closed")),
        }
    }
}

fn read_cell_body<'a>(
    part: &str,
    sc: &mut Scanner<'a>,
    c_name: &str,
    c: &mut RawCell<'a>,
) -> Result<(), XlsxError> {
    loop {
        match sc.next().map_err(|m| err(part, m))? {
            Tok::Start {
                name,
                attrs: raw,
                empty,
            } => match local(name) {
                "v" => {
                    c.v = Some(if empty {
                        Cow::Borrowed("")
                    } else {
                        sc.text_of(name).map_err(|m| err(part, m))?
                    });
                }
                "f" => {
                    let mut f = formula_attrs(raw, part)?;
                    if f.t.as_deref() == Some("dataTable") {
                        let mut whole = if empty {
                            let mut e = kiln_ooxml::xml::XmlElement::new("f");
                            for (k, v) in attrs(raw, part)? {
                                e.attrs.set(k, v.into_owned());
                            }
                            e
                        } else {
                            sc.subtree(name, raw).map_err(|m| err(part, m))?
                        };
                        whole.name = "f".to_owned();
                        f.text = whole.text();
                        f.whole = Some(whole);
                    } else if !empty {
                        f.text = sc.text_of(name).map_err(|m| err(part, m))?.into_owned();
                    }
                    c.f = Some(f);
                }
                "is" => {
                    c.is = Some(if empty {
                        Inline::Plain(String::new())
                    } else {
                        inline_string(part, sc, name)?
                    });
                }
                _ => {
                    if !empty {
                        sc.skip(name).map_err(|m| err(part, m))?;
                    }
                }
            },
            Tok::End { name } if name == c_name => return Ok(()),
            Tok::End { name } => return Err(err(part, format!("</{name}> closes <{c_name}>"))),
            Tok::Text(t) => {
                unescape(t).map_err(|m| err(part, m))?;
            }
            Tok::CData(_) => {}
            Tok::Eof => return Err(err(part, "a cell is not closed")),
        }
    }
}

fn formula_attrs(raw: &str, part: &str) -> Result<RawFormula, XlsxError> {
    let mut f = RawFormula::default();
    for (k, v) in attrs(raw, part)? {
        let v = v.into_owned();
        match k {
            "t" => f.t = Some(v),
            "ref" => f.r#ref = Some(v),
            "si" => f.si = Some(v),
            k => f.other.push((k.to_owned(), v)),
        }
    }
    Ok(f)
}

/// The model text of a formula read (prefixes stripped); the file text is kept in `extra` when
/// it differs, so an unchanged formula is written back exactly.
fn model_text(file: String, extra: &mut CellExtra, future: &mut BTreeSet<String>) -> String {
    if !file.contains("_x") && !file.contains("_X") {
        return file;
    }
    let model = from_file_formula(&file, Some(future));
    extra.file_formula = Some((file, model.clone()));
    model
}

/// Short repeated texts share one allocation.
fn intern(set: &mut FxHashSet<Arc<str>>, s: &str) -> Arc<str> {
    if s.len() > 64 {
        return Arc::from(s);
    }
    if let Some(a) = set.get(s) {
        return a.clone();
    }
    let a: Arc<str> = Arc::from(s);
    if set.len() < 1 << 16 {
        set.insert(a.clone());
    }
    a
}

/// Turns a raw cell into a record; returns the next column.
fn finish_cell(
    c: RawCell<'_>,
    ctx: &CellContext<'_>,
    out: &mut SheetCells,
    shared: &mut FxHashMap<String, (u32, u32, String)>,
    covered: &mut Vec<(Range, (u32, u32))>,
    interned: &mut FxHashSet<Arc<str>>,
) -> Result<u32, XlsxError> {
    let (Some(row), Some(col)) = (c.row, c.col) else {
        return Ok(c.col.map_or(0, |x| x.saturating_add(1)));
    };
    if col >= kiln_sheet::MAX_COLS {
        return Ok(col);
    }
    let value = match c.t {
        CellType::Shared => {
            let idx = c.v.as_deref().and_then(|v| v.trim().parse::<usize>().ok());
            match idx.and_then(|i| ctx.sst.get(i)) {
                Some(v) => v.clone(),
                None => {
                    if c.v.as_deref().is_some_and(|v| !v.trim().is_empty()) {
                        out.warnings.push(format!(
                            "{}: a cell refers to a shared string that does not exist",
                            ctx.part
                        ));
                    }
                    CellValue::Blank
                }
            }
        }
        CellType::Str => c.v.as_deref().map_or(CellValue::Blank, |v| {
            CellValue::Text(intern(interned, &decode_x_escapes(v)))
        }),
        CellType::Inline => match c.is {
            Some(Inline::Plain(t)) => CellValue::Text(intern(interned, &decode_x_escapes(&t))),
            Some(Inline::Rich(v)) => v,
            None => c.v.as_deref().map_or(CellValue::Blank, |v| {
                CellValue::Text(intern(interned, &decode_x_escapes(v)))
            }),
        },
        CellType::Bool => {
            c.v.as_deref()
                .and_then(parse_bool)
                .map_or(CellValue::Blank, CellValue::Bool)
        }
        CellType::Error => {
            c.v.as_deref()
                .map_or(CellValue::Blank, |v| CellValue::Error(error_value(v)))
        }
        CellType::Date => {
            c.v.as_deref()
                .and_then(|v| iso_to_serial(v, ctx.date1904))
                .map_or(CellValue::Blank, CellValue::number)
        }
        CellType::Number => match c.v.as_deref().map(str::trim) {
            None | Some("") => CellValue::Blank,
            Some(v) => match v.parse::<f64>() {
                Ok(n) if n.is_finite() => CellValue::number(n),
                _ => {
                    out.warnings
                        .push(format!("{}: \"{v}\" is not a number", ctx.part));
                    CellValue::Blank
                }
            },
        },
    };
    let mut extra = CellExtra {
        vm: c.vm,
        ph: c.ph,
        ..CellExtra::default()
    };
    let mut record = ReadCell {
        row,
        col,
        value,
        formula: None,
        legacy: false,
        style: c.s,
    };
    // A cell inside a legacy array or dynamic array range other than its anchor holds a value
    // the engine recomputes: keep its format only.
    let inside = covered
        .iter()
        .any(|(rg, anchor)| rg.contains(row, col) && *anchor != (row, col));
    if let Some(f) = c.f {
        let RawFormula {
            t: f_type,
            r#ref: f_ref,
            si: f_si,
            text: f_text,
            other,
            whole,
        } = f;
        extra.f_attrs = other;
        match f_type.as_deref() {
            Some("dataTable") => extra.data_table = whole,
            Some("shared") => {
                let si = f_si.unwrap_or_default();
                if !f_text.trim().is_empty() {
                    let model = model_text(f_text, &mut extra, &mut out.future_functions);
                    shared.insert(si, (row, col, model.clone()));
                    record.formula = Some(model);
                    record.legacy = true;
                } else if let Some((ar, ac, text)) = shared.get(&si) {
                    let dr = i64::from(row) - i64::from(*ar);
                    let dc = i64::from(col) - i64::from(*ac);
                    let copied = kiln_formula::refs::copy_formula(text, dr, dc)
                        .unwrap_or_else(|_| text.clone());
                    record.formula = Some(copied);
                    record.legacy = true;
                } else {
                    out.warnings.push(format!(
                        "{}: a shared formula refers to group {si}, which has no formula",
                        ctx.part
                    ));
                }
            }
            Some("array") => {
                let model = model_text(f_text, &mut extra, &mut out.future_functions);
                let rg = f_ref
                    .as_deref()
                    .and_then(parse_range)
                    .unwrap_or(Range::cell(row, col));
                let dynamic = c.cm.is_some_and(|cm| ctx.dynamic_cm.contains(&cm))
                    || (c.cm.is_some() && !ctx.has_metadata);
                if dynamic {
                    out.dynamic.insert((row, col));
                } else {
                    out.array_formulas.insert((row, col), rg);
                }
                covered.push((rg, (row, col)));
                record.formula = Some(model);
                record.legacy = false;
            }
            _ => {
                if !f_text.trim().is_empty() {
                    record.formula =
                        Some(model_text(f_text, &mut extra, &mut out.future_functions));
                    record.legacy = true;
                }
            }
        }
    }
    if inside && record.formula.is_none() {
        record.value = CellValue::Blank;
    }
    if extra.vm.is_some()
        || extra.ph
        || !extra.f_attrs.is_empty()
        || extra.data_table.is_some()
        || extra.file_formula.is_some()
    {
        out.extras.insert((row, col), extra);
    }
    let keep = record.formula.is_some() || !record.value.is_blank() || record.style != 0;
    if keep {
        out.cells.push(record);
    }
    Ok(col.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(sst: &'a [CellValue], dynamic: &'a FxHashSet<u32>) -> CellContext<'a> {
        CellContext {
            part: "/xl/worksheets/sheet1.xml",
            sst,
            xf_count: 3,
            dynamic_cm: dynamic,
            has_metadata: true,
            date1904: false,
        }
    }

    #[test]
    fn iso_dates_become_serials() {
        assert_eq!(iso_to_serial("2026-01-15", false), Some(46_037.0));
        assert_eq!(iso_to_serial("1900-03-01", false), Some(61.0));
        assert_eq!(iso_to_serial("1900-01-01", false), Some(1.0));
        assert_eq!(iso_to_serial("1904-01-02", true), Some(1.0));
        assert_eq!(iso_to_serial("2026-01-15T12:00:00", false), Some(46_037.5));
        assert_eq!(iso_to_serial("2026-13-01", false), None);
    }

    #[test]
    fn cells_rows_and_formulas_stream() {
        let xml = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:C3"/><sheetData><row r="1" spans="1:3" ht="20" customHeight="1" x14ac:dyDescent="0.25"><c r="A1" t="s"><v>0</v></c><c r="B1" s="2"><v>1.5</v></c><c t="b"><v>1</v></c></row><row r="2"><c r="A2"><f t="shared" ref="A2:A3" si="0">B1*2</f><v>3</v></c><c r="B2" t="e"><v>#DIV/0!</v></c><c r="C2" t="str"><f>"a"&amp;"b"</f><v>ab</v></c></row><row r="3"><c r="A3"><f t="shared" si="0"/><v>0</v></c><c r="B3" t="inlineStr"><is><t>x &amp; y</t></is></c><c r="C3" s="9"/></row></sheetData><pageMargins/></worksheet>"#;
        let sst = [CellValue::text("hello")];
        let dynamic = FxHashSet::default();
        let out = read_sheet_cells(xml, &ctx(&sst, &dynamic)).expect("cells");
        let (a, b) = out.span.expect("span");
        assert!(xml[a..b].starts_with("<sheetData>") && xml[a..b].ends_with("</sheetData>"));
        let find = |r: u32, c: u32| out.cells.iter().find(|x| x.row == r && x.col == c).cloned();
        assert_eq!(find(0, 0).map(|c| c.value), Some(CellValue::text("hello")));
        assert_eq!(
            find(0, 1).map(|c| (c.value, c.style)),
            Some((CellValue::Number(1.5), 2))
        );
        assert_eq!(find(0, 2).map(|c| c.value), Some(CellValue::Bool(true)));
        assert_eq!(find(1, 0).and_then(|c| c.formula), Some("B1*2".to_owned()));
        assert_eq!(find(2, 0).and_then(|c| c.formula), Some("B2*2".to_owned()));
        assert_eq!(
            find(1, 1).map(|c| c.value),
            Some(CellValue::Error(ErrorValue::Div0))
        );
        assert_eq!(
            find(1, 2).and_then(|c| c.formula),
            Some("\"a\"&\"b\"".to_owned())
        );
        assert_eq!(find(2, 1).map(|c| c.value), Some(CellValue::text("x & y")));
        // An unknown format index falls back to 0 with a warning; the cell has nothing else.
        assert!(find(2, 2).is_none());
        assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0].1.height, Some(F64(20.0)));
        assert_eq!(
            out.row_attrs.get(&0),
            Some(&vec![("x14ac:dyDescent".to_owned(), "0.25".to_owned())])
        );
    }

    #[test]
    fn array_formulas_cover_their_range() {
        let xml = r#"<worksheet><sheetData><row r="1"><c r="A1"><f t="array" ref="A1:A2">B1:B2*2</f><v>2</v></c><c r="B1" cm="1"><f t="array" ref="B1:B3">_xlfn.SEQUENCE(3)</f><v>1</v></c></row><row r="2"><c r="A2"><v>4</v></c><c r="B2" s="1"><v>2</v></c></row></sheetData></worksheet>"#;
        let mut dynamic = FxHashSet::default();
        dynamic.insert(1);
        let out = read_sheet_cells(xml, &ctx(&[], &dynamic)).expect("cells");
        assert_eq!(
            out.array_formulas.get(&(0, 0)),
            Some(&Range::new(0, 0, 1, 0))
        );
        assert!(out.dynamic.contains(&(0, 1)));
        assert!(!out.cells.iter().any(|c| (c.row, c.col) == (1, 0)));
        let b2 = out
            .cells
            .iter()
            .find(|c| (c.row, c.col) == (1, 1))
            .expect("b2");
        assert_eq!((b2.value.clone(), b2.style), (CellValue::Blank, 1));
        let b1 = out
            .cells
            .iter()
            .find(|c| (c.row, c.col) == (0, 1))
            .expect("b1");
        assert_eq!(b1.formula.as_deref(), Some("SEQUENCE(3)"));
        assert!(!b1.legacy);
    }

    #[test]
    fn hostile_markup_is_refused_not_panicking() {
        let dynamic = FxHashSet::default();
        for xml in [
            "<worksheet><sheetData><row><c r=\"A1\"><v>1</v></c>",
            "<!DOCTYPE x [<!ENTITY a \"b\">]><worksheet/>",
            "<worksheet><sheetData><row><c r=\"A1\" t=\"s\"><v>&bad;</v></c></row></sheetData></worksheet>",
        ] {
            assert!(read_sheet_cells(xml, &ctx(&[], &dynamic)).is_err(), "{xml}");
        }
        let xml = "<worksheet><sheetData><row r=\"9999999\"><c r=\"ZZZZ1\"><v>x</v></c><c t=\"s\"><v>7</v></c></row></sheetData></worksheet>";
        let out = read_sheet_cells(xml, &ctx(&[], &dynamic)).expect("tolerated");
        assert!(out.cells.is_empty());
        assert!(!out.warnings.is_empty());
    }
}
