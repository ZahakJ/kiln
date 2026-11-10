// SPDX-License-Identifier: AGPL-3.0-or-later
//! A chart's embedded workbook, read and written with no spreadsheet engine.
//!
//! Reading takes the values of the chart's sheet with each cell's number format (from the
//! workbook's `cellXfs`) and the rows and columns that are hidden: what Edit Data shows and the
//! chart reads. Writing patches the workbook PowerPoint or Kiln made in place: only the cells
//! whose value changed are rewritten, each keeping its style (a number typed in another format
//! gets a style with that format), and a formula typed over becomes a value, as in Excel. Every
//! other cell keeps its value, formula and style, rows and columns keep their height, width and
//! hidden state, the sheet's tables follow the chart's data range, and the rest of the workbook
//! is kept as it was. Formulas are not recalculated here: a workbook that keeps formulas is
//! marked to be recalculated when Excel opens it. A new chart gets a minimal workbook that Excel
//! and PowerPoint open.

use std::collections::{BTreeSet, HashMap, HashSet};

use kiln_charts::data::{ChartCell, ChartData};
use kiln_charts::embed::Table;
use kiln_common::{KilnError, Result};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, rel_type};
use kiln_ooxml::xml::{
    ParseOptions, WriteOptions, XmlDocument, XmlElement, XmlNode, format_number, parse_xml_with,
    write_document,
};
use kiln_sheet::Range;
use kiln_sheet::styles::{builtin_num_fmt, builtin_num_fmt_id};

/// The most cells read from a workbook (Edit Data takes at most 10,000).
const MAX_CELLS: usize = 100_000;
/// The spreadsheetml namespace.
const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// The relationships namespace.
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORKBOOK_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const SHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const STYLES_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
/// Where a new workbook's sheet goes.
const NEW_SHEET: &str = "/xl/worksheets/sheet1.xml";
/// The first number format id a workbook may define (lower ones are Excel's built-in formats).
const FIRST_CUSTOM_FORMAT: u32 = 164;

/// A chart's data sheet: the values of one sheet from A1, and the workbook they came from.
pub(super) struct Book {
    /// The values, as Edit Data shows them and the chart reads them.
    pub table: Table,
    /// The chart's data range before and after an edit: the sheet's tables over it follow it.
    moved: Option<(Range, Range)>,
    /// The values as read: a save rewrites only the cells that differ.
    read: Table,
    package: OpcPackage,
    workbook_part: String,
    sheet_part: String,
    /// The number format of each cell style, by `cellXfs` index.
    formats: Vec<String>,
    hidden_rows: BTreeSet<u32>,
    /// Hidden columns, as inclusive ranges.
    hidden_cols: Vec<(u32, u32)>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The namespace prefix of an element's name with its colon (`x:`), or nothing.
fn prefix_of(name: &str) -> &str {
    name.rfind(':').map_or("", |i| &name[..=i])
}

fn kids<'a>(e: &'a XmlElement, name: &'a str) -> impl Iterator<Item = &'a XmlElement> + 'a {
    e.elements().filter(move |c| local(&c.name) == name)
}

fn on(v: Option<&str>) -> bool {
    matches!(v, Some("1" | "true"))
}

fn parse_u32(v: Option<&str>) -> Option<u32> {
    v.and_then(|v| v.trim().parse().ok())
}

/// A part, its prefixes as the file wrote them (new elements take the prefix of their parent).
fn raw_xml(pkg: &OpcPackage, part: &str) -> Option<XmlDocument> {
    let options = ParseOptions {
        normalize_prefixes: false,
        ..ParseOptions::default()
    };
    parse_xml_with(&pkg.part(part).ok()??, options).ok()
}

fn put_xml(pkg: &mut OpcPackage, part: &str, doc: &XmlDocument) -> Result<()> {
    let xml = write_document(doc, &WriteOptions::default()).map_err(|e| err(&e))?;
    pkg.set_part(part, xml.into_bytes()).map_err(|e| err(&e))
}

fn err(e: &dyn std::fmt::Display) -> KilnError {
    KilnError::InvalidInput(format!("chart data sheet: {e}"))
}

/// The text of a shared or inline string item (its runs joined, phonetic runs left out).
fn string_item(si: &XmlElement) -> String {
    let mut out = String::new();
    for c in si.elements() {
        match local(&c.name) {
            "t" => out.push_str(&c.text()),
            "r" => kids(c, "t").for_each(|t| out.push_str(&t.text())),
            _ => {}
        }
    }
    out
}

/// `A1` → `(row, col)`, zero-based.
fn cell_ref(r: &str) -> Option<(usize, usize)> {
    let split = r.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = r.split_at(split);
    let mut col = 0usize;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col
            .checked_mul(26)?
            .checked_add(usize::from(ch.to_ascii_uppercase() as u8 - b'A' + 1))?;
    }
    let row: usize = digits.parse().ok()?;
    (col > 0 && row > 0).then(|| (row - 1, col - 1))
}

/// `(row, col)` → `A1`.
fn ref_of(row: usize, col: usize) -> String {
    let mut letters = Vec::new();
    let mut c = col + 1;
    while c > 0 {
        let rem = (c - 1) % 26;
        letters.push(char::from(b'A' + u8::try_from(rem).unwrap_or(0)));
        c = (c - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", letters.into_iter().collect::<String>(), row + 1)
}

fn at(table: &Table, r: usize, c: usize) -> &ChartCell {
    table
        .rows
        .get(r)
        .and_then(|l| l.get(c))
        .unwrap_or(&ChartCell::Blank)
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The text a cell shows (a table column takes its header cell's text as its name).
fn shown(cell: &ChartCell) -> String {
    match cell {
        ChartCell::Blank => String::new(),
        ChartCell::Number(v, code) => kiln_charts::format::format_number(*v, code),
        ChartCell::Text(t) => t.clone(),
    }
}

/// The styles part of a workbook.
fn styles_part(pkg: &OpcPackage, workbook: &str) -> Option<String> {
    pkg.related_part(workbook, rel_type::STYLES).ok().flatten()
}

/// The format code of every `cellXfs` style.
fn number_formats(doc: &XmlDocument) -> Vec<String> {
    let custom: HashMap<u32, String> = kids(&doc.root, "numFmts")
        .flat_map(|n| kids(n, "numFmt"))
        .filter_map(|f| {
            Some((
                parse_u32(f.attr("numFmtId"))?,
                f.attr("formatCode")?.to_owned(),
            ))
        })
        .collect();
    kids(&doc.root, "cellXfs")
        .flat_map(|x| kids(x, "xf"))
        .map(|xf| {
            let id = parse_u32(xf.attr("numFmtId")).unwrap_or(0);
            custom
                .get(&id)
                .cloned()
                .or_else(|| builtin_num_fmt(id).map(str::to_owned))
                .unwrap_or_else(|| "General".into())
        })
        .collect()
}

impl Book {
    /// A new workbook holding `table` on its one sheet (written on [`Book::write`]).
    pub(super) fn new(table: Table) -> Result<Self> {
        let package = new_book(&table.sheet).map_err(|e| err(&e))?;
        Ok(Self {
            read: Table::new(&table.sheet),
            table,
            moved: None,
            package,
            workbook_part: "/xl/workbook.xml".into(),
            sheet_part: NEW_SHEET.into(),
            formats: vec!["General".into()],
            hidden_rows: BTreeSet::new(),
            hidden_cols: Vec::new(),
        })
    }

    /// Reads the sheet the chart names (the first sheet when there is none of that name). The
    /// error says why the workbook cannot be read.
    pub(super) fn read(bytes: &[u8], sheet_name: &str) -> std::result::Result<Self, String> {
        let not_xlsx = || "it is not an Excel workbook (.xlsx) that Kiln can open".to_owned();
        let pkg = OpcPackage::open(bytes).map_err(|_| not_xlsx())?;
        let wb_part = pkg
            .related_part("/", rel_type::OFFICE_DOCUMENT)
            .ok()
            .flatten()
            .ok_or_else(not_xlsx)?;
        let wb = raw_xml(&pkg, &wb_part).ok_or_else(not_xlsx)?;
        if local(&wb.root.name) != "workbook" {
            return Err(not_xlsx());
        }
        let rels = pkg.relationships(&wb_part).map_err(|_| not_xlsx())?;
        let sheets: Vec<&XmlElement> = kids(&wb.root, "sheets")
            .flat_map(|s| kids(s, "sheet"))
            .collect();
        let sheet = sheets
            .iter()
            .find(|s| s.attr("name") == Some(sheet_name))
            .or_else(|| sheets.first())
            .ok_or_else(|| "the workbook has no sheet".to_owned())?;
        let sheet_part = sheet
            .attrs
            .iter()
            .find(|(k, _)| local(k) == "id")
            .and_then(|(_, rid)| rels.get(rid))
            .and_then(|rel| rels.resolve(rel))
            .ok_or_else(not_xlsx)?;
        let name = sheet.attr("name").unwrap_or(sheet_name).to_owned();
        let shared: Vec<String> = pkg
            .related_part(&wb_part, rel_type::SHARED_STRINGS)
            .ok()
            .flatten()
            .and_then(|p| raw_xml(&pkg, &p))
            .map(|d| kids(&d.root, "si").map(string_item).collect())
            .unwrap_or_default();
        let formats = styles_part(&pkg, &wb_part)
            .and_then(|p| raw_xml(&pkg, &p))
            .map(|d| number_formats(&d))
            .unwrap_or_default();
        let doc = raw_xml(&pkg, &sheet_part)
            .filter(|d| local(&d.root.name) == "worksheet")
            .ok_or_else(|| format!("its sheet {name} is not a worksheet Kiln can read"))?;
        let too_big = || format!("its sheet {name} is larger than Edit Data reads");
        let mut rows: Vec<Vec<ChartCell>> = Vec::new();
        let mut hidden_rows = BTreeSet::new();
        let mut count = 0usize;
        let mut next_row = 0usize;
        for row in kids(&doc.root, "sheetData").flat_map(|d| kids(d, "row")) {
            let r = parse_u32(row.attr("r"))
                .filter(|r| *r > 0)
                .map_or(next_row, |r| r as usize - 1);
            next_row = r + 1;
            if on(row.attr("hidden")) {
                hidden_rows.insert(to_u32(r));
            }
            let mut next_col = 0usize;
            for c in kids(row, "c") {
                count += 1;
                if count > MAX_CELLS {
                    return Err(too_big());
                }
                let (r, col) = c.attr("r").and_then(cell_ref).unwrap_or((r, next_col));
                next_col = col + 1;
                let format = || {
                    parse_u32(c.attr("s"))
                        .and_then(|s| formats.get(s as usize).cloned())
                        .unwrap_or_else(|| "General".into())
                };
                let v = kids(c, "v").next().map(XmlElement::text);
                let cell = match c.attr("t") {
                    Some("s") => v
                        .and_then(|i| i.trim().parse::<usize>().ok())
                        .and_then(|i| shared.get(i).cloned())
                        .map_or(ChartCell::Blank, ChartCell::Text),
                    Some("inlineStr") => kids(c, "is")
                        .next()
                        .map_or(ChartCell::Blank, |is| ChartCell::Text(string_item(is))),
                    Some("str" | "e" | "d") => v.map_or(ChartCell::Blank, ChartCell::Text),
                    Some("b") => ChartCell::Number(
                        if v.as_deref().map(str::trim) == Some("1") {
                            1.0
                        } else {
                            0.0
                        },
                        format(),
                    ),
                    _ => v
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .filter(|n| n.is_finite())
                        .map_or(ChartCell::Blank, |n| ChartCell::Number(n, format())),
                };
                if matches!(cell, ChartCell::Blank) {
                    continue;
                }
                if r >= MAX_CELLS || col >= 16_384 {
                    return Err(too_big());
                }
                if rows.len() <= r {
                    rows.resize_with(r + 1, Vec::new);
                }
                let line = &mut rows[r];
                if line.len() <= col {
                    line.resize(col + 1, ChartCell::Blank);
                }
                line[col] = cell;
            }
        }
        let hidden_cols = kids(&doc.root, "cols")
            .flat_map(|c| kids(c, "col"))
            .filter(|c| on(c.attr("hidden")))
            .filter_map(|c| {
                let min = parse_u32(c.attr("min"))?.max(1);
                let max = parse_u32(c.attr("max")).unwrap_or(min).max(min);
                Some((min - 1, max - 1))
            })
            .collect();
        let table = Table { sheet: name, rows };
        Ok(Self {
            read: table.clone(),
            table,
            moved: None,
            package: pkg,
            workbook_part: wb_part,
            sheet_part,
            formats,
            hidden_rows,
            hidden_cols,
        })
    }

    /// The number format a cell has (`General` when it has none).
    pub(super) fn format_at(&self, row: usize, col: usize) -> Option<&str> {
        match at(&self.table, row, col) {
            ChartCell::Number(_, code) => Some(code),
            _ => None,
        }
    }

    /// The chart's data range moves from `old` to `new`: the sheet's tables over it follow it,
    /// and a header cell that repeats another's name takes a number (as Excel names a table's
    /// columns), so the chart reads the names the workbook keeps.
    pub(super) fn follow(&mut self, old: Range, new: Range) -> Result<()> {
        self.moved = Some((old, new));
        let mut scratch = self.package.clone();
        let mut table = self.table.clone();
        self.fit_tables(&mut scratch, &mut table)?;
        self.table = table;
        Ok(())
    }

    /// The workbook with the changed cells written in, as `.xlsx` bytes.
    pub(super) fn write(&self) -> Result<Vec<u8>> {
        let mut pkg = self.package.clone();
        let mut table = self.table.clone();
        self.fit_tables(&mut pkg, &mut table)?;
        let mut styles = Styles::open(&pkg, &self.workbook_part, &self.formats);
        let mut doc =
            raw_xml(&pkg, &self.sheet_part).ok_or_else(|| err(&"the sheet is missing"))?;
        let patched = patch_sheet(&mut doc.root, &self.read, &table, &mut styles)?;
        put_xml(&mut pkg, &self.sheet_part, &doc)?;
        styles.save(&mut pkg)?;
        // A formula typed over leaves the calculation chain naming a cell without one.
        if patched.formula_removed
            && let Some(chain) = pkg
                .related_part(&self.workbook_part, rel_type::CALC_CHAIN)
                .ok()
                .flatten()
        {
            pkg.remove_part(&chain, true).map_err(|e| err(&e))?;
        }
        // The formulas kept may read the cells that changed: Excel recalculates them on open.
        if patched.changed
            && patched.has_formulas
            && let Some(mut wb) = raw_xml(&pkg, &self.workbook_part)
        {
            let p = prefix_of(&wb.root.name).to_owned();
            if let Some(calc) = wb.root.elements_mut().find(|e| local(&e.name) == "calcPr") {
                calc.set_attr("fullCalcOnLoad", Some("1"));
            } else {
                {
                    let after: Vec<String> = [
                        "oleSize",
                        "customWorkbookViews",
                        "pivotCaches",
                        "smartTagPr",
                        "smartTagTypes",
                        "webPublishing",
                        "fileRecoveryPr",
                        "webPublishObjects",
                        "extLst",
                    ]
                    .iter()
                    .map(|n| format!("{p}{n}"))
                    .collect();
                    let after: Vec<&str> = after.iter().map(String::as_str).collect();
                    wb.root.upsert_child(
                        XmlElement::new(format!("{p}calcPr")).with_attr("fullCalcOnLoad", "1"),
                        &after,
                    );
                }
            }
            put_xml(&mut pkg, &self.workbook_part, &wb)?;
        }
        pkg.save().map_err(|e| err(&e))
    }

    /// The sheet's tables over the chart's old data range span its new one, with a column for
    /// each of its columns named after its header cell (unique, as Excel requires: a repeated
    /// name takes a number, in the cell too). Columns that stay keep everything else they had.
    fn fit_tables(&self, pkg: &mut OpcPackage, table: &mut Table) -> Result<()> {
        let Some((old, new)) = self.moved else {
            return Ok(());
        };
        let parts = pkg
            .related_parts(&self.sheet_part, rel_type::TABLE)
            .map_err(|e| err(&e))?;
        for part in parts {
            let Some(mut t) = raw_xml(pkg, &part) else {
                continue;
            };
            let Some(was) = t.root.attr("ref").and_then(Range::parse_a1) else {
                continue;
            };
            if !was.intersects(&old) {
                continue;
            }
            let header = t.root.attr("headerRowCount") != Some("0");
            let totals = parse_u32(t.root.attr("totalsRowCount")).unwrap_or(0);
            let span = Range::new(new.r0, new.c0, new.r1.saturating_add(totals), new.c1);
            t.root.set_attr("ref", Some(span.to_a1(false)));
            let mut names: Vec<String> = Vec::new();
            let mut used = HashSet::new();
            for (i, col) in (new.c0..=new.c1).enumerate() {
                let text = if header {
                    shown(at(table, new.r0 as usize, col as usize))
                } else {
                    String::new()
                };
                let base = if text.trim().is_empty() {
                    format!("Column{}", i + 1)
                } else {
                    text.clone()
                };
                let mut name = base.clone();
                let mut n = 2;
                while !used.insert(name.to_lowercase()) {
                    name = format!("{base}{n}");
                    n += 1;
                }
                if !text.trim().is_empty() && name != text {
                    table.set(new.r0 as usize, col as usize, ChartCell::Text(name.clone()));
                }
                names.push(name);
            }
            for el in t.root.elements_mut() {
                match local(&el.name) {
                    "autoFilter" => el.set_attr("ref", Some(new.to_a1(false))),
                    "tableColumns" => {
                        let p = prefix_of(&el.name).to_owned();
                        let kept: Vec<XmlElement> = kids(el, "tableColumn").cloned().collect();
                        let mut next_id = kept
                            .iter()
                            .filter_map(|c| parse_u32(c.attr("id")))
                            .max()
                            .unwrap_or(0);
                        el.children = names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let col = match kept.get(i) {
                                    Some(c) => {
                                        let mut c = c.clone();
                                        c.set_attr("name", Some(name.as_str()));
                                        c
                                    }
                                    None => {
                                        next_id += 1;
                                        XmlElement::new(format!("{p}tableColumn"))
                                            .with_attr("id", next_id)
                                            .with_attr("name", name.as_str())
                                    }
                                };
                                XmlNode::Element(col)
                            })
                            .collect();
                        el.set_attr("count", Some(names.len()));
                    }
                    _ => {}
                }
            }
            put_xml(pkg, &part, &t)?;
        }
        Ok(())
    }
}

impl ChartData for Book {
    fn cell(&self, sheet: &str, row: u32, col: u32) -> Option<ChartCell> {
        self.table.cell(sheet, row, col)
    }

    fn last_row(&self, sheet: &str) -> Option<u32> {
        self.table.last_row(sheet)
    }

    fn last_col(&self, sheet: &str) -> Option<u32> {
        self.table.last_col(sheet)
    }

    fn hidden(&self, sheet: &str, row: u32, col: u32) -> bool {
        sheet.eq_ignore_ascii_case(&self.table.sheet)
            && (self.hidden_rows.contains(&row)
                || self
                    .hidden_cols
                    .iter()
                    .any(|(a, b)| (*a..=*b).contains(&col)))
    }
}

/// The workbook's styles: a typed number in a new format gets a cell style with that format.
struct Styles {
    part: Option<String>,
    doc: Option<XmlDocument>,
    formats: Vec<String>,
    changed: bool,
}

impl Styles {
    fn open(pkg: &OpcPackage, workbook: &str, formats: &[String]) -> Self {
        let part = styles_part(pkg, workbook);
        let doc = part.as_deref().and_then(|p| raw_xml(pkg, p));
        Self {
            part,
            doc,
            formats: formats.to_vec(),
            changed: false,
        }
    }

    fn format(&self, style: u32) -> &str {
        self.formats
            .get(style as usize)
            .map_or("General", String::as_str)
    }

    /// A style like `base` with the number format `code`: `base` itself when it has it.
    fn with_format(&mut self, base: u32, code: &str) -> u32 {
        if self.format(base) == code {
            return base;
        }
        let Some(doc) = self.doc.as_mut() else {
            return base;
        };
        let p = prefix_of(&doc.root.name).to_owned();
        let id = match builtin_num_fmt_id(code) {
            Some(id) => id,
            None => {
                if !doc.root.elements().any(|e| local(&e.name) == "numFmts") {
                    doc.root.children.insert(
                        0,
                        XmlNode::Element(
                            XmlElement::new(format!("{p}numFmts")).with_attr("count", 0),
                        ),
                    );
                }
                let Some(list) = doc
                    .root
                    .elements_mut()
                    .find(|e| local(&e.name) == "numFmts")
                else {
                    return base;
                };
                let known = kids(list, "numFmt")
                    .find(|f| f.attr("formatCode") == Some(code))
                    .and_then(|f| parse_u32(f.attr("numFmtId")));
                match known {
                    Some(id) => id,
                    None => {
                        let id = kids(list, "numFmt")
                            .filter_map(|f| parse_u32(f.attr("numFmtId")))
                            .map(|i| i + 1)
                            .max()
                            .unwrap_or(FIRST_CUSTOM_FORMAT)
                            .max(FIRST_CUSTOM_FORMAT);
                        list.children.push(XmlNode::Element(
                            XmlElement::new(format!("{p}numFmt"))
                                .with_attr("numFmtId", id)
                                .with_attr("formatCode", code),
                        ));
                        let n = kids(list, "numFmt").count();
                        list.set_attr("count", Some(n));
                        id
                    }
                }
            }
        };
        let Some(xfs) = doc
            .root
            .elements_mut()
            .find(|e| local(&e.name) == "cellXfs")
        else {
            return base;
        };
        let mut xf = kids(xfs, "xf")
            .nth(base as usize)
            .cloned()
            .unwrap_or_else(|| {
                XmlElement::new(format!("{p}xf"))
                    .with_attr("numFmtId", 0)
                    .with_attr("fontId", 0)
                    .with_attr("fillId", 0)
                    .with_attr("borderId", 0)
                    .with_attr("xfId", 0)
            });
        xf.set_attr("numFmtId", Some(id));
        xf.set_attr("applyNumberFormat", Some("1"));
        if let Some(i) = kids(xfs, "xf").position(|x| *x == xf) {
            return to_u32(i);
        }
        xfs.children.push(XmlNode::Element(xf));
        let n = kids(xfs, "xf").count();
        xfs.set_attr("count", Some(n));
        self.formats.resize(n - 1, "General".into());
        self.formats.push(code.to_owned());
        self.changed = true;
        to_u32(n - 1)
    }

    fn save(&self, pkg: &mut OpcPackage) -> Result<()> {
        match (&self.part, &self.doc) {
            (Some(part), Some(doc)) if self.changed => put_xml(pkg, part, doc),
            _ => Ok(()),
        }
    }
}

/// What patching a sheet did.
#[derive(Default)]
struct Patched {
    /// A cell changed.
    changed: bool,
    /// A formula was typed over.
    formula_removed: bool,
    /// The sheet still has formulas.
    has_formulas: bool,
}

/// The row number (1-based) of a `row`, and the position of a `c` as `(row, col)`, zero-based.
fn row_number(row: &XmlElement) -> usize {
    parse_u32(row.attr("r")).map_or(0, |r| r as usize)
}

fn cell_col(c: &XmlElement) -> usize {
    c.attr("r").and_then(cell_ref).map_or(0, |(_, col)| col)
}

/// Writes the cells of `table` that differ from `read` into a worksheet, in place.
fn patch_sheet(
    root: &mut XmlElement,
    read: &Table,
    table: &Table,
    styles: &mut Styles,
) -> Result<Patched> {
    let mut out = Patched::default();
    let Some(data) = root.elements_mut().find(|e| local(&e.name) == "sheetData") else {
        return Err(err(&"the sheet has no cells element"));
    };
    let p = prefix_of(&data.name).to_owned();
    // Rows and cells without a position (allowed, rare) get one, so cells can be found.
    let mut next_row = 0usize;
    for row in data.elements_mut().filter(|e| local(&e.name) == "row") {
        let r = match row_number(row) {
            0 => next_row + 1,
            r => r,
        };
        next_row = r;
        row.set_attr("r", Some(r));
        let mut next_col = 0usize;
        for c in row.elements_mut().filter(|e| local(&e.name) == "c") {
            let col = c
                .attr("r")
                .and_then(cell_ref)
                .map_or(next_col, |(_, col)| col);
            next_col = col + 1;
            c.set_attr("r", Some(ref_of(r - 1, col)));
        }
    }
    let height = read.rows.len().max(table.rows.len());
    let mut shared_gone: HashSet<String> = HashSet::new();
    let mut touched_rows: BTreeSet<usize> = BTreeSet::new();
    for r in 0..height {
        let width = read
            .rows
            .get(r)
            .map_or(0, Vec::len)
            .max(table.rows.get(r).map_or(0, Vec::len));
        for c in 0..width {
            let new = at(table, r, c);
            if at(read, r, c) == new {
                continue;
            }
            out.changed = true;
            touched_rows.insert(r + 1);
            let Some(row) = row_mut(data, r + 1, &p) else {
                continue;
            };
            let Some(cell) = cell_mut(row, r, c, &p, !matches!(new, ChartCell::Blank)) else {
                continue;
            };
            if set_value(cell, new, styles, &p, &mut shared_gone) {
                out.formula_removed = true;
            }
        }
    }
    // The other cells of a shared formula whose first cell was typed over keep their values.
    if !shared_gone.is_empty() {
        for row in data.elements_mut() {
            for c in row.elements_mut() {
                let before = c.children.len();
                c.children.retain(|n| {
                    !matches!(n, XmlNode::Element(f) if local(&f.name) == "f"
                        && f.attr("t") == Some("shared")
                        && f.attr("si").is_some_and(|si| shared_gone.contains(si)))
                });
                if c.children.len() != before {
                    out.formula_removed = true;
                }
            }
        }
    }
    // Empty unstyled cells go; rows left with nothing and no formatting of their own go.
    for row in data
        .elements_mut()
        .filter(|e| touched_rows.contains(&row_number(e)))
    {
        row.children.retain(|n| match n {
            XmlNode::Element(c) if local(&c.name) == "c" => {
                c.elements().next().is_some() || parse_u32(c.attr("s")).unwrap_or(0) != 0
            }
            _ => true,
        });
        if row.attr("spans").is_some() {
            let cols: Vec<usize> = kids(row, "c").map(cell_col).collect();
            let spans = match (cols.iter().min(), cols.iter().max()) {
                (Some(a), Some(b)) => Some(format!("{}:{}", a + 1, b + 1)),
                _ => None,
            };
            row.set_attr("spans", spans);
        }
    }
    data.children.retain(|n| match n {
        XmlNode::Element(row) if local(&row.name) == "row" => {
            row.elements().next().is_some()
                || row
                    .attrs
                    .iter()
                    .any(|(k, _)| !matches!(local(k), "r" | "spans"))
        }
        _ => true,
    });
    out.has_formulas = data
        .elements()
        .flat_map(|row| row.elements())
        .any(|c| kids(c, "f").next().is_some());
    // The sheet's used range.
    let mut used: Option<(usize, usize, usize, usize)> = None;
    for row in kids(data, "row") {
        for c in kids(row, "c") {
            if let Some((r, col)) = c.attr("r").and_then(cell_ref) {
                used = Some(used.map_or((r, col, r, col), |(r0, c0, r1, c1)| {
                    (r0.min(r), c0.min(col), r1.max(r), c1.max(col))
                }));
            }
        }
    }
    let dimension = match used {
        Some((r0, c0, r1, c1)) if (r0, c0) == (r1, c1) => ref_of(r0, c0),
        Some((r0, c0, r1, c1)) => format!("{}:{}", ref_of(r0, c0), ref_of(r1, c1)),
        None => "A1".into(),
    };
    if let Some(d) = root.elements_mut().find(|e| local(&e.name) == "dimension") {
        d.set_attr("ref", Some(dimension));
    }
    Ok(out)
}

/// The `row` numbered `r` (1-based), added in its place when the sheet has none.
fn row_mut<'a>(data: &'a mut XmlElement, r: usize, p: &str) -> Option<&'a mut XmlElement> {
    let found = data.children.iter().position(
        |n| matches!(n, XmlNode::Element(e) if local(&e.name) == "row" && row_number(e) >= r),
    );
    let i = match found {
        Some(i) if matches!(&data.children[i], XmlNode::Element(e) if row_number(e) == r) => i,
        Some(i) => {
            data.children.insert(
                i,
                XmlNode::Element(XmlElement::new(format!("{p}row")).with_attr("r", r)),
            );
            i
        }
        None => {
            // After the last row (anything after the rows, which is never there, stays last).
            let i = data
                .children
                .iter()
                .rposition(|n| matches!(n, XmlNode::Element(e) if local(&e.name) == "row"))
                .map_or(0, |i| i + 1);
            data.children.insert(
                i,
                XmlNode::Element(XmlElement::new(format!("{p}row")).with_attr("r", r)),
            );
            i
        }
    };
    data.children[i].as_element_mut()
}

/// The cell at `col` of a row, added in its place when `create` and the row has none.
fn cell_mut<'a>(
    row: &'a mut XmlElement,
    r: usize,
    col: usize,
    p: &str,
    create: bool,
) -> Option<&'a mut XmlElement> {
    let found = row.children.iter().position(
        |n| matches!(n, XmlNode::Element(e) if local(&e.name) == "c" && cell_col(e) >= col),
    );
    let i = match found {
        Some(i) if matches!(&row.children[i], XmlNode::Element(e) if cell_col(e) == col) => i,
        _ if !create => return None,
        Some(i) => {
            row.children
                .insert(i, XmlNode::Element(new_cell(r, col, p)));
            i
        }
        None => {
            let i = row
                .children
                .iter()
                .rposition(|n| matches!(n, XmlNode::Element(e) if local(&e.name) == "c"))
                .map_or(0, |i| i + 1);
            row.children
                .insert(i, XmlNode::Element(new_cell(r, col, p)));
            i
        }
    };
    row.children[i].as_element_mut()
}

fn new_cell(r: usize, col: usize, p: &str) -> XmlElement {
    XmlElement::new(format!("{p}c")).with_attr("r", ref_of(r, col))
}

/// Gives a cell a new value, keeping its style (or one like it with the value's number format)
/// and anything else it holds. Returns whether a formula was removed; the shared formulas whose
/// first cell this was are added to `shared_gone`.
fn set_value(
    cell: &mut XmlElement,
    value: &ChartCell,
    styles: &mut Styles,
    p: &str,
    shared_gone: &mut HashSet<String>,
) -> bool {
    let mut formula = false;
    cell.children.retain(|n| match n {
        XmlNode::Element(e) => match local(&e.name) {
            "f" => {
                formula = true;
                if e.attr("t") == Some("shared")
                    && e.attr("ref").is_some()
                    && let Some(si) = e.attr("si")
                {
                    shared_gone.insert(si.to_owned());
                }
                false
            }
            "v" | "is" => false,
            _ => true,
        },
        XmlNode::Text(_) => false,
        _ => true,
    });
    cell.set_attr::<&str>("cm", None);
    cell.set_attr::<&str>("vm", None);
    match value {
        ChartCell::Blank => cell.set_attr::<&str>("t", None),
        ChartCell::Number(n, code) => {
            cell.set_attr::<&str>("t", None);
            let style = parse_u32(cell.attr("s")).unwrap_or(0);
            let style = styles.with_format(style, code);
            cell.set_attr("s", (style != 0).then_some(style));
            cell.children.insert(
                0,
                XmlNode::Element(XmlElement::new(format!("{p}v")).with_text(format_number(*n))),
            );
        }
        ChartCell::Text(t) => {
            cell.set_attr("t", Some("inlineStr"));
            let spaced = t.starts_with(char::is_whitespace) || t.ends_with(char::is_whitespace);
            let text = XmlElement::new(format!("{p}t"))
                .with_opt_attr("xml:space", spaced.then_some("preserve"))
                .with_text(t.as_str());
            cell.children.insert(
                0,
                XmlNode::Element(XmlElement::new(format!("{p}is")).with_child(text)),
            );
        }
    }
    formula
}

/// A workbook of one empty sheet named `sheet` with Excel's default style.
fn new_book(sheet: &str) -> std::result::Result<OpcPackage, kiln_ooxml::opc::OpcError> {
    let mut pkg = OpcPackage::new();
    let wb = "/xl/workbook.xml";
    let st = "/xl/styles.xml";
    let worksheet = XmlElement::new("worksheet")
        .with_attr("xmlns", NS)
        .with_attr("xmlns:r", NS_R)
        .with_child(XmlElement::new("dimension").with_attr("ref", "A1"))
        .with_child(XmlElement::new("sheetData"));
    let workbook = XmlElement::new("workbook")
        .with_attr("xmlns", NS)
        .with_attr("xmlns:r", NS_R)
        .with_child(
            XmlElement::new("sheets").with_child(
                XmlElement::new("sheet")
                    .with_attr("name", sheet)
                    .with_attr("sheetId", 1)
                    .with_attr("r:id", "rId1"),
            ),
        );
    let styles =
        XmlElement::new("styleSheet")
            .with_attr("xmlns", NS)
            .with_child(
                XmlElement::new("fonts").with_attr("count", 1).with_child(
                    XmlElement::new("font")
                        .with_child(XmlElement::new("sz").with_attr("val", 11))
                        .with_child(XmlElement::new("name").with_attr("val", "Calibri")),
                ),
            )
            .with_child(
                XmlElement::new("fills")
                    .with_attr("count", 2)
                    .with_child(XmlElement::new("fill").with_child(
                        XmlElement::new("patternFill").with_attr("patternType", "none"),
                    ))
                    .with_child(XmlElement::new("fill").with_child(
                        XmlElement::new("patternFill").with_attr("patternType", "gray125"),
                    )),
            )
            .with_child(
                XmlElement::new("borders")
                    .with_attr("count", 1)
                    .with_child(XmlElement::new("border")),
            )
            .with_child(
                XmlElement::new("cellStyleXfs")
                    .with_attr("count", 1)
                    .with_child(XmlElement::new("xf").with_attr("numFmtId", 0)),
            )
            .with_child(
                XmlElement::new("cellXfs").with_attr("count", 1).with_child(
                    XmlElement::new("xf")
                        .with_attr("numFmtId", 0)
                        .with_attr("xfId", 0),
                ),
            )
            .with_child(
                XmlElement::new("cellStyles")
                    .with_attr("count", 1)
                    .with_child(
                        XmlElement::new("cellStyle")
                            .with_attr("name", "Normal")
                            .with_attr("xfId", 0)
                            .with_attr("builtinId", 0),
                    ),
            );
    pkg.add_xml_part(wb, WORKBOOK_TYPE, &workbook)?;
    pkg.add_xml_part(NEW_SHEET, SHEET_TYPE, &worksheet)?;
    pkg.add_xml_part(st, STYLES_TYPE, &styles)?;
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        wb,
        RelationshipOptions::default(),
    )?;
    pkg.add_relationship(
        wb,
        rel_type::WORKSHEET,
        NEW_SHEET,
        RelationshipOptions {
            id: Some("rId1"),
            ..RelationshipOptions::default()
        },
    )?;
    pkg.add_relationship(wb, rel_type::STYLES, st, RelationshipOptions::default())?;
    Ok(pkg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_book_reads_back_and_a_rewrite_changes_only_its_cells() {
        let table =
            kiln_charts::embed::sample_table(kiln_protocol::commands::ChartKind::ColumnClustered);
        let bytes = Book::new(table.clone())
            .expect("new")
            .write()
            .expect("write");
        let mut back = Book::read(&bytes, "Sheet1").expect("read");
        assert_eq!(back.table.rows, table.rows);
        back.table.rows[1][1] = ChartCell::Number(9.5, "General".into());
        back.table.rows.push(vec![
            ChartCell::Text("Category 5".into()),
            ChartCell::Number(1.0, "General".into()),
        ]);
        let again = Book::read(&back.write().expect("rewrite"), "Sheet1").expect("read");
        assert_eq!(
            again.table.rows[1][1],
            ChartCell::Number(9.5, "General".into())
        );
        assert_eq!(again.table.rows.len(), table.rows.len() + 1);
        assert_eq!(again.table.rows[2], table.rows[2], "untouched rows stay");
        // A cell cleared goes, and so does a row left empty.
        let mut cleared = again;
        cleared.table.rows.pop();
        let sheet = OpcPackage::open(&cleared.write().expect("write")).expect("open");
        let xml = String::from_utf8(
            sheet
                .part(NEW_SHEET)
                .expect("read")
                .expect("sheet")
                .into_owned(),
        )
        .expect("utf-8");
        assert!(!xml.contains(r#"<row r="6""#), "{xml}");
        assert!(xml.contains(r#"<dimension ref="A1:D5"/>"#), "{xml}");
    }

    #[test]
    fn references_go_both_ways() {
        for (r, c) in [(0, 0), (9, 25), (0, 26), (99, 701), (3, 702)] {
            assert_eq!(cell_ref(&ref_of(r, c)), Some((r, c)));
        }
        assert_eq!(ref_of(0, 27), "AB1");
    }
}
