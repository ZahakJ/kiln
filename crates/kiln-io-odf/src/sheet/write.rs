// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`kiln_sheet::Workbook`] → `.ods`.
//!
//! The inverse of [`super::read`]. Cell formats become automatic cell styles over the
//! `Default` style (which carries the workbook's default font), number formats become data
//! styles ([`crate::numfmt`]), formulas are written as OpenFormula ([`crate::formula`]) with
//! their current results as cached values and display text, dynamic arrays that spill as
//! matrix formulas, merges as spanned cells, identical consecutive rows and cells as repeats,
//! and a trailing filler row and column carry the default sizes. Conditional formats, data
//! validation, named ranges and expressions, the AutoFilter, notes, hyperlinks, page setup
//! (one page style per sheet) and the views (active sheet, frozen panes, zoom, gridlines,
//! cursor) are written as their ODF counterparts; tables keep their range and filter buttons
//! as database ranges.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kiln_formula::numfmt::NumberFormat;
use kiln_sheet::axis::{BASE_COL_WIDTH, ColProps, RowProps, default_col_width_for};
use kiln_sheet::meta::{Orientation, SheetVisibility};
use kiln_sheet::styles::{CellStyle, FontPatch, Underline, VertAlign};
use kiln_sheet::workbook::CellRecord;
use kiln_sheet::{CellValue, MAX_COLS, MAX_ROWS, Range, Sheet, SheetId, Theme, Workbook};

use super::rules::{dxf_style_el, write_cf, write_validation};
use super::style::{cell_props_els, color_hex, font_family_value, text_props_el};
use crate::el::El;
use crate::formula::{cell_address, excel_ref_to_range_address, excel_to_odf_with, range_address};
use crate::numfmt::data_style_els;
use crate::package::{OdfKind, PackageWriter};
use crate::units::{duration_str, serial_date_str};
use crate::xml::XmlOut;
use crate::{GENERATOR, OdfError, WriteOptions, meta};

const PREFIXES: &[&str] = &[
    "office",
    "style",
    "text",
    "table",
    "draw",
    "fo",
    "xlink",
    "dc",
    "meta",
    "number",
    "svg",
    "of",
    "loext",
    "calcext",
    "tableooo",
    "presentation",
    "chart",
    "config",
    "ooo",
];

/// Lengths with six decimals (row heights and column widths round-trip exactly).
fn pt6(v: f64) -> String {
    let mut s = format!("{v:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s}pt")
}

fn in6(v: f64) -> String {
    let mut s = format!("{v:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s}in")
}

/// Automatic styles shared by equal definitions.
#[derive(Debug, Default)]
struct Autos {
    list: Vec<El>,
    index: HashMap<El, String>,
    counters: HashMap<&'static str, usize>,
}

impl Autos {
    fn add(&mut self, prefix: &'static str, el: El) -> String {
        if let Some(n) = self.index.get(&el) {
            return n.clone();
        }
        let c = self.counters.entry(prefix).or_insert(0);
        *c += 1;
        let name = format!("{prefix}{c}");
        let mut named = el.clone();
        named
            .attrs
            .insert(0, ("style:name".to_owned(), name.clone()));
        self.list.push(named);
        self.index.insert(el, name.clone());
        name
    }
}

struct Writer<'w> {
    wb: &'w Workbook,
    theme: &'w Theme,
    autos: Autos,
    data_styles: Vec<El>,
    codes: HashMap<String, String>,
    cell_styles: HashMap<u32, String>,
    fonts: BTreeSet<String>,
    common: Vec<El>,
    common_codes: HashMap<String, String>,
    dxf_names: HashMap<u32, String>,
    layouts: Vec<El>,
    masters: Vec<El>,
    validations: Vec<(String, kiln_sheet::meta::DataValidation, String)>,
    warnings: Vec<String>,
    default_font: kiln_sheet::styles::Font,
    /// The charts the drawings reference, by part name.
    charts: &'w [(String, kiln_charts::model::ChartSpace)],
    /// Files besides the main parts: chart objects (a path ending in `/` is a manifest entry)
    /// and pictures.
    files: Vec<(String, String, Vec<u8>)>,
    /// Drawing frames numbered so far.
    frames_written: usize,
    /// Pictures by part name (see [`WriteOptions::media`]).
    media: &'w [(String, Vec<u8>)],
}

/// Writes a workbook as `.ods`.
///
/// # Errors
///
/// [`OdfError::Write`] when the archive cannot be written.
pub fn write_ods(wb: &Workbook, options: &WriteOptions) -> Result<Vec<u8>, OdfError> {
    let mut w = Writer {
        wb,
        theme: wb.theme(),
        autos: Autos::default(),
        data_styles: Vec::new(),
        codes: HashMap::new(),
        cell_styles: HashMap::new(),
        fonts: BTreeSet::new(),
        common: Vec::new(),
        common_codes: HashMap::new(),
        dxf_names: HashMap::new(),
        layouts: Vec::new(),
        masters: Vec::new(),
        validations: Vec::new(),
        warnings: Vec::new(),
        default_font: wb.styles().default_font().clone(),
        charts: &options.charts,
        files: Vec::new(),
        frames_written: 0,
        media: &options.media,
    };
    let content = w.content();
    let styles = w.styles_xml();
    let mut pkg = PackageWriter::new(OdfKind::Spreadsheet);
    pkg.set_template(options.template);
    pkg.add("content.xml", "text/xml", content);
    pkg.add("styles.xml", "text/xml", styles);
    let props = options.properties.clone().unwrap_or_default();
    pkg.add("meta.xml", "text/xml", meta::write_meta(&props, GENERATOR));
    pkg.add("settings.xml", "text/xml", settings_xml(wb));
    for (path, media, bytes) in std::mem::take(&mut w.files) {
        if path.ends_with('/') {
            pkg.add_entry(&path, &media);
        } else {
            pkg.add(&path, &media, bytes);
        }
    }
    pkg.finish()
}

/// What saving a workbook as `.ods` cannot keep (the Compatibility Checker's list).
#[must_use]
pub fn ods_issues(wb: &Workbook, options: &WriteOptions) -> Vec<String> {
    let mut w = Writer {
        wb,
        theme: wb.theme(),
        autos: Autos::default(),
        data_styles: Vec::new(),
        codes: HashMap::new(),
        cell_styles: HashMap::new(),
        fonts: BTreeSet::new(),
        common: Vec::new(),
        common_codes: HashMap::new(),
        dxf_names: HashMap::new(),
        layouts: Vec::new(),
        masters: Vec::new(),
        validations: Vec::new(),
        warnings: Vec::new(),
        default_font: wb.styles().default_font().clone(),
        charts: &options.charts,
        files: Vec::new(),
        frames_written: 0,
        media: &options.media,
    };
    let _ = w.content();
    let _ = w.styles_xml();
    w.warnings
}

impl Writer<'_> {
    fn data_style(&mut self, code: &str, common: bool) -> Option<String> {
        let map = if common {
            &self.common_codes
        } else {
            &self.codes
        };
        if let Some(n) = map.get(code) {
            return Some(n.clone());
        }
        let n = self.codes.len() + self.common_codes.len() + 1;
        let name = if common {
            format!("NC{n}")
        } else {
            format!("N{n}")
        };
        let els = data_style_els(&name, code);
        if els.is_empty() {
            return None;
        }
        if common {
            self.common.extend(els);
            self.common_codes.insert(code.to_owned(), name.clone());
        } else {
            self.data_styles.extend(els);
            self.codes.insert(code.to_owned(), name.clone());
        }
        Some(name)
    }

    fn cell_style(&mut self, xf: u32) -> Option<String> {
        // The default and its twin (a blank cell under a formatted row or column that shows the
        // default) are the Default style, never an automatic style of their own.
        if xf == 0 || self.wb.styles().get(xf) == self.wb.styles().get(0) {
            return None;
        }
        if let Some(n) = self.cell_styles.get(&xf) {
            return Some(n.clone());
        }
        let cs: CellStyle = self.wb.styles().get(xf);
        let mut st = El::new("style:style")
            .with("style:family", "table-cell")
            .with("style:parent-style-name", "Default");
        if let Some(ds) = self.data_style(&cs.num_fmt, false) {
            st.set("style:data-style-name", ds);
        }
        let (tc, pp) = cell_props_els(&cs, self.theme);
        st.push_nonempty(tc);
        st.push_nonempty(pp);
        let tp = text_props_el(&cs.font, Some(&self.default_font), self.theme);
        if tp.get("style:font-name").is_some() {
            self.fonts.insert(cs.font.name.to_string());
        }
        st.push_nonempty(tp);
        let name = self.autos.add("ce", st);
        self.cell_styles.insert(xf, name.clone());
        Some(name)
    }

    fn text_style(&mut self, f: &FontPatch) -> Option<String> {
        let mut tp = El::new("style:text-properties");
        if let Some(n) = &f.name {
            tp.set("style:font-name", n.to_string());
            tp.set("fo:font-family", font_family_value(n));
            self.fonts.insert(n.to_string());
        }
        if let Some(s) = f.size {
            tp.set("fo:font-size", pt6(s.0));
        }
        if let Some(b) = f.bold {
            tp.set("fo:font-weight", if b { "bold" } else { "normal" });
        }
        if let Some(i) = f.italic {
            tp.set("fo:font-style", if i { "italic" } else { "normal" });
        }
        if let Some(u) = f.underline {
            match u {
                Underline::None => tp.set("style:text-underline-style", "none"),
                Underline::Double | Underline::DoubleAccounting => {
                    tp.set("style:text-underline-style", "solid");
                    tp.set("style:text-underline-type", "double");
                }
                _ => tp.set("style:text-underline-style", "solid"),
            }
        }
        if let Some(s) = f.strike {
            tp.set(
                "style:text-line-through-style",
                if s { "solid" } else { "none" },
            );
        }
        if let Some(c) = &f.color {
            tp.set("fo:color", color_hex(c, self.theme, 0));
        }
        if let Some(v) = f.vert_align {
            tp.set(
                "style:text-position",
                match v {
                    VertAlign::Superscript => "super 58%",
                    VertAlign::Subscript => "sub 58%",
                    VertAlign::Baseline => "0% 100%",
                },
            );
        }
        if tp.is_empty() {
            return None;
        }
        let st = El::new("style:style")
            .with("style:family", "text")
            .with_child(tp);
        Some(self.autos.add("T", st))
    }

    fn dxf_style(&mut self, dxf: u32) -> String {
        if let Some(n) = self.dxf_names.get(&dxf) {
            return n.clone();
        }
        let name = format!("ConditionalStyle_{}", self.dxf_names.len() + 1);
        let d = self.wb.styles().dxf(dxf).cloned().unwrap_or_default();
        let ds = d.num_fmt.as_deref().and_then(|c| self.data_style(c, true));
        let el = dxf_style_el(&name, &d, ds.as_deref(), self.theme);
        self.common.push(el);
        self.dxf_names.insert(dxf, name.clone());
        name
    }

    #[allow(clippy::too_many_lines)]
    fn content(&mut self) -> Vec<u8> {
        let mut body = XmlOut::fragment();
        body.start("office:body").start("office:spreadsheet");
        let props = self.wb.props();
        body.start("table:calculation-settings")
            .attr("table:case-sensitive", "false")
            .attr("table:automatic-find-labels", "false")
            .attr("table:use-regular-expressions", "false")
            .attr("table:use-wildcards", "true");
        if props.date1904 {
            body.start("table:null-date")
                .attr("table:date-value", "1904-01-01")
                .end();
        }
        body.start("table:iteration")
            .attr(
                "table:status",
                if props.calc.iterate {
                    "enable"
                } else {
                    "disable"
                },
            )
            .attr("table:steps", props.calc.iterate_count.to_string())
            .attr(
                "table:maximum-difference",
                crate::units::number_str(props.calc.iterate_delta),
            )
            .end();
        body.end();
        // Validations are written before the tables but numbered while writing them.
        let mut tables = XmlOut::fragment();
        let sheets: Vec<&Sheet> = self.wb.sheets().iter().collect();
        for (i, s) in sheets.iter().enumerate() {
            self.table(&mut tables, s, i);
        }
        if !self.validations.is_empty() {
            body.start("table:content-validations");
            for (name, dv, base) in &self.validations {
                write_validation(&mut body, name, dv, base);
            }
            body.end();
        }
        body.raw(&tables.into_string());
        // Workbook-scope names.
        let names: Vec<&kiln_sheet::meta::DefinedName> = self
            .wb
            .names()
            .iter()
            .filter(|n| n.scope.is_none())
            .collect();
        let first = sheets.first().map_or("Sheet1", |s| s.name());
        self.named_expressions(&mut body, &names, first);
        // AutoFilters and tables as database ranges.
        let mut dbs = Vec::new();
        for (i, s) in sheets.iter().enumerate() {
            let m = s.meta();
            if let Some(af) = &m.auto_filter {
                dbs.push((
                    format!("__Anonymous_Sheet_DB__{i}"),
                    range_address(s.name(), &af.range),
                    true,
                ));
            }
            for t in &m.tables {
                dbs.push((
                    t.name.clone(),
                    range_address(s.name(), &t.range),
                    t.auto_filter.is_some() && m.auto_filter.is_none(),
                ));
            }
        }
        if !dbs.is_empty() {
            body.start("table:database-ranges");
            for (name, target, buttons) in dbs {
                body.start("table:database-range")
                    .attr("table:name", name)
                    .attr("table:target-range-address", target);
                if buttons {
                    body.attr("table:display-filter-buttons", "true");
                }
                body.end();
            }
            body.end();
        }
        body.end().end();
        let mut w = XmlOut::new();
        w.root("office:document-content", PREFIXES)
            .attr("office:version", "1.3");
        w.start("office:scripts").end();
        font_decls(&mut w, &self.fonts, &self.default_font.name);
        w.start("office:automatic-styles");
        for e in &self.autos.list {
            e.write(&mut w);
        }
        for e in &self.data_styles {
            e.write(&mut w);
        }
        w.end();
        w.raw(&body.into_string());
        w.finish()
    }

    fn named_expressions(
        &mut self,
        w: &mut XmlOut,
        names: &[&kiln_sheet::meta::DefinedName],
        base_sheet: &str,
    ) {
        let mut items = Vec::new();
        for n in names {
            if n.name.starts_with("_xlnm.") {
                continue;
            }
            items.push(*n);
        }
        if items.is_empty() {
            return;
        }
        w.start("table:named-expressions");
        for n in items {
            match excel_ref_to_range_address(&n.formula) {
                Some(addr) => {
                    w.start("table:named-range")
                        .attr("table:name", &n.name)
                        .attr("table:base-cell-address", cell_address(base_sheet, 0, 0))
                        .attr("table:cell-range-address", addr)
                        .end();
                }
                None => {
                    w.start("table:named-expression")
                        .attr("table:name", &n.name)
                        .attr("table:base-cell-address", cell_address(base_sheet, 0, 0))
                        .attr("table:expression", excel_to_odf_with(&n.formula, &|_| None))
                        .end();
                }
            }
        }
        w.end();
    }

    fn page_style(&mut self, s: &Sheet, i: usize) -> String {
        let p = &s.meta().print;
        let (pw, ph) = super::read::paper_size(p.paper_size);
        let (w, h) = if p.orientation == Orientation::Landscape {
            (ph, pw)
        } else {
            (pw, ph)
        };
        let mut props = El::new("style:page-layout-properties")
            .with("fo:page-width", in6(w))
            .with("fo:page-height", in6(h))
            .with(
                "style:print-orientation",
                if p.orientation == Orientation::Landscape {
                    "landscape"
                } else {
                    "portrait"
                },
            )
            .with("fo:margin-left", in6(p.margins.left))
            .with("fo:margin-right", in6(p.margins.right))
            .with("fo:margin-top", in6(p.margins.top))
            .with("fo:margin-bottom", in6(p.margins.bottom));
        if p.scale != 100 {
            props.set("style:scale-to", format!("{}%", p.scale));
        }
        let mut print = vec!["charts", "drawings", "objects", "zero-values"];
        if p.print_gridlines {
            print.push("grid");
        }
        if p.print_headings {
            print.push("headers");
        }
        props.set("style:print", print.join(" "));
        let centering = match (p.horizontal_centered, p.vertical_centered) {
            (true, true) => "both",
            (true, false) => "horizontal",
            (false, true) => "vertical",
            (false, false) => "none",
        };
        props.set("style:table-centering", centering);
        let layout = format!("pm{}", i + 1);
        self.layouts.push(
            El::new("style:page-layout")
                .with("style:name", layout.clone())
                .with_child(props),
        );
        let master = format!("PageStyle_{}", i + 1);
        let mut mp = El::new("style:master-page")
            .with("style:name", master.clone())
            .with("style:display-name", format!("Page style {}", i + 1))
            .with("style:page-layout-name", layout);
        mp.children.extend(super::headers::write(&p.header_footer));
        self.masters.push(mp);
        master
    }

    #[allow(clippy::too_many_lines)]
    fn table(&mut self, out: &mut XmlOut, s: &Sheet, index: usize) {
        let wb = self.wb;
        let id = s.id();
        let name = s.name();
        let m = s.meta();
        let master = self.page_style(s, index);
        let mut tp = El::new("style:table-properties")
            .with(
                "table:display",
                (m.visibility == SheetVisibility::Visible).to_string(),
            )
            .with(
                "style:writing-mode",
                if m.view.right_to_left {
                    "rl-tb"
                } else {
                    "lr-tb"
                },
            );
        if let Some(c) = &m.tab_color {
            tp.set("tableooo:tab-color", color_hex(c, self.theme, 0));
        }
        let ta = self.autos.add(
            "ta",
            El::new("style:style")
                .with("style:family", "table")
                .with("style:master-page-name", master)
                .with_child(tp),
        );
        out.start("table:table")
            .attr("table:name", name)
            .attr("table:style-name", ta);
        if m.protection.is_some() {
            out.attr("table:protected", "true");
        }
        if let Some(pa) = wb
            .names()
            .iter()
            .find(|n| n.scope == Some(id) && n.name == kiln_sheet::meta::builtin_names::PRINT_AREA)
        {
            let list: Vec<String> = pa
                .formula
                .split(',')
                .filter_map(excel_ref_to_range_address)
                .collect();
            if !list.is_empty() {
                out.attr("table:print-ranges", list.join(" "));
            }
        }
        let records: Vec<CellRecord> = wb
            .snapshot_sheet(id)
            .map(|snap| snap.cells)
            .unwrap_or_default();
        let layout = s.layout();
        let row_props: BTreeMap<u32, RowProps> = layout.rows.iter().copied().collect();
        let col_props: BTreeMap<u32, ColProps> = layout.cols.iter().copied().collect();
        let mdw = wb.max_digit_width();
        let default_width = m
            .default_col_width
            .unwrap_or_else(|| default_col_width_for(BASE_COL_WIDTH, mdw));
        let col_style = |autos: &mut Autos, width: f64, best_fit: bool| -> String {
            let mut p = El::new("style:table-column-properties")
                .with("fo:break-before", "auto")
                .with("style:column-width", in6(width * mdw / 96.0));
            if best_fit {
                p.set("style:use-optimal-column-width", "true");
            }
            autos.add(
                "co",
                El::new("style:style")
                    .with("style:family", "table-column")
                    .with_child(p),
            )
        };
        let default_height = m.default_row_height;
        let row_style = |autos: &mut Autos, height: f64, custom: bool| -> String {
            autos.add(
                "ro",
                El::new("style:style")
                    .with("style:family", "table-row")
                    .with_child(
                        El::new("style:table-row-properties")
                            .with("style:row-height", pt6(height))
                            .with("fo:break-before", "auto")
                            .with("style:use-optimal-row-height", (!custom).to_string()),
                    ),
            )
        };
        // Spilled arrays: anchors become matrix formulas, the cells they fill get values.
        let mut matrix: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        let mut spilled: BTreeMap<(u32, u32), CellValue> = BTreeMap::new();
        for (r, c, d) in s.cells().iter() {
            if d.kind != kiln_sheet::CellKind::Formula {
                continue;
            }
            // A single-cell result keeps array semantics only as a matrix formula.
            if wb.formula(id, r, c).is_some_and(|f| array_semantics(&f)) {
                matrix.insert((r, c), (1, 1));
            }
            if let Some(sr) = wb.spill_range(id, r, c)
                && !sr.blocked
            {
                let b = sr.bounds;
                let (rows, cols) = (b.r1 - b.r0 + 1, b.c1 - b.c0 + 1);
                if u64::from(rows) * u64::from(cols) > 1
                    && u64::from(rows) * u64::from(cols) <= 1_000_000
                {
                    matrix.insert((r, c), (rows, cols));
                    for rr in b.r0..=b.r1 {
                        for cc in b.c0..=b.c1 {
                            if (rr, cc) != (r, c) {
                                spilled.insert((rr, cc), wb.value(id, rr, cc));
                            }
                        }
                    }
                }
            }
        }
        // Print titles repeat as header rows and columns.
        let (mut title_rows, mut title_cols) = (None, None);
        if let Some(pt) = wb.names().iter().find(|n| {
            n.scope == Some(id) && n.name == kiln_sheet::meta::builtin_names::PRINT_TITLES
        }) {
            for part in pt.formula.split(',') {
                let toks = kiln_formula::parser::tokenize(
                    part.trim(),
                    &kiln_formula::parser::ParseOptions::default(),
                    0,
                );
                for t in toks {
                    if let kiln_formula::parser::TokenKind::Ref { address, .. } = t.kind {
                        match address {
                            kiln_formula::parser::RefAddress::Rows { r0, r1, .. } => {
                                title_rows = Some((r0, r1))
                            }
                            kiln_formula::parser::RefAddress::Cols { c0, c1, .. } => {
                                title_cols = Some((c0, c1))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        // Columns.
        let max_col_used = records
            .iter()
            .map(|r| r.col)
            .chain(col_props.keys().copied())
            .chain(m.merges.iter().map(|g| g.c1))
            .chain(title_cols.map(|t| t.1))
            .max()
            .unwrap_or(0)
            .min(MAX_COLS - 1);
        let mut cols: Vec<(String, u32)> = Vec::new();
        for c in 0..=max_col_used {
            let p = col_props.get(&c).copied().unwrap_or_default();
            let w = p.width.map_or(default_width, |w| w.0);
            let st = col_style(&mut self.autos, w, p.best_fit);
            let mut x = XmlOut::fragment();
            x.start("table:table-column").attr("table:style-name", st);
            if p.hidden {
                x.attr("table:visibility", "collapse");
            }
            if let Some(cs) = p.style.and_then(|xf| self.cell_style(xf)) {
                x.attr("table:default-cell-style-name", cs);
            }
            let h = if title_cols.is_some_and(|(a, b)| a <= c && c <= b) {
                'h'
            } else {
                '-'
            };
            let key = format!("{}|{h}|{}", p.outline_level, x.into_string());
            match cols.last_mut() {
                Some((k, n)) if *k == key => *n += 1,
                _ => cols.push((key, 1)),
            }
        }
        let filler = col_style(&mut self.autos, default_width, false);
        cols.push((
            format!("0|-|<table:table-column table:style-name=\"{filler}\"/>"),
            MAX_COLS - 1 - max_col_used,
        ));
        write_grouped(
            out,
            &cols,
            "table:table-column-group",
            "table:table-header-columns",
            "table:number-columns-repeated",
        );
        // Rows: cells by row.
        let mut by_row: BTreeMap<u32, Vec<&CellRecord>> = BTreeMap::new();
        for r in &records {
            by_row.entry(r.row).or_default().push(r);
        }
        let mut merges: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        let mut covered: BTreeSet<(u32, u32)> = BTreeSet::new();
        for g in &m.merges {
            merges.insert((g.r0, g.c0), (g.r1 - g.r0 + 1, g.c1 - g.c0 + 1));
            if g.cell_count() <= 4_000_000 {
                for rr in g.r0..=g.r1 {
                    for cc in g.c0..=g.c1 {
                        if (rr, cc) != (g.r0, g.c0) {
                            covered.insert((rr, cc));
                        }
                    }
                }
            }
        }
        let mut links: HashMap<(u32, u32), &kiln_sheet::meta::Hyperlink> = HashMap::new();
        for h in &m.hyperlinks {
            if h.range.cell_count() <= 100_000 {
                for p in h.range.cells() {
                    links.entry((p.row, p.col)).or_insert(h);
                }
            }
        }
        let mut notes: HashMap<(u32, u32), &kiln_sheet::meta::Note> = HashMap::new();
        for n in &m.notes {
            notes.insert((n.at.row, n.at.col), n);
        }
        // Validation names per cell (ranges clipped to the grid).
        let mut dv_at: Vec<(Range, String)> = Vec::new();
        for dv in &m.validations {
            let vname = format!("val{}", self.validations.len() + 1);
            let base = dv.ranges.first().map_or_else(
                || cell_address(name, 0, 0),
                |r| cell_address(name, r.r0, r.c0),
            );
            self.validations.push((vname.clone(), dv.clone(), base));
            for r in &dv.ranges {
                dv_at.push((*r, vname.clone()));
            }
        }
        let frames = self.frames(
            s,
            &col_props,
            &row_props,
            default_width,
            default_height,
            mdw,
        );
        let mut interesting: BTreeSet<u32> = by_row.keys().copied().collect();
        interesting.extend(frames.keys().map(|k| k.0));
        interesting.extend(row_props.keys().copied());
        interesting.extend(merges.keys().map(|k| k.0));
        interesting.extend(covered.iter().map(|k| k.0));
        interesting.extend(spilled.keys().map(|k| k.0));
        interesting.extend(links.keys().map(|k| k.0));
        interesting.extend(notes.keys().map(|k| k.0));
        if let Some((a, b)) = title_rows {
            interesting.extend(a..=b.min(a + 1000));
        }
        let dv_max = dv_at.iter().map(|(r, _)| r.r1).max();
        let max_row = interesting
            .iter()
            .next_back()
            .copied()
            .into_iter()
            .chain(dv_max)
            .max();
        let mut rows: Vec<(String, u32)> = Vec::new();
        let default_row = row_style(&mut self.autos, default_height, false);
        if let Some(max_row) = max_row {
            let max_row = max_row.min(MAX_ROWS - 1);
            let mut r = 0u32;
            while r <= max_row {
                // A run of rows with nothing at all collapses into one element.
                if !interesting.contains(&r) && !dv_at.iter().any(|(g, _)| g.r0 <= r && r <= g.r1) {
                    let next = interesting
                        .range(r..)
                        .next()
                        .copied()
                        .unwrap_or(max_row + 1)
                        .min(
                            dv_at
                                .iter()
                                .filter(|(g, _)| g.r0 > r)
                                .map(|(g, _)| g.r0)
                                .min()
                                .unwrap_or(max_row + 1),
                        )
                        .min(max_row + 1);
                    let n = next - r;
                    let key = format!(
                        "0|-|<table:table-row table:style-name=\"{default_row}\"><table:table-cell/></table:table-row>"
                    );
                    push_run(&mut rows, key, n);
                    r = next;
                    continue;
                }
                let p = row_props.get(&r).copied().unwrap_or_default();
                let h = p.height.map_or(default_height, |h| h.0);
                let rs = row_style(&mut self.autos, h, p.custom_height);
                let mut x = XmlOut::fragment();
                x.start("table:table-row").attr("table:style-name", rs);
                if p.hidden {
                    x.attr(
                        "table:visibility",
                        if p.filtered { "filter" } else { "collapse" },
                    );
                }
                if let Some(cs) = p.style.and_then(|xf| self.cell_style(xf)) {
                    x.attr("table:default-cell-style-name", cs);
                }
                // Cells of the row.
                let mut cols_here: BTreeSet<u32> = BTreeSet::new();
                if let Some(cells) = by_row.get(&r) {
                    cols_here.extend(cells.iter().map(|c| c.col));
                }
                for k in merges
                    .keys()
                    .chain(covered.iter())
                    .chain(spilled.keys())
                    .chain(links.keys())
                    .chain(notes.keys())
                    .chain(frames.keys())
                {
                    if k.0 == r {
                        cols_here.insert(k.1);
                    }
                }
                for (g, _) in &dv_at {
                    if g.r0 <= r && r <= g.r1 {
                        for c in g.c0..=g.c1.min(g.c0 + 16_383) {
                            cols_here.insert(c);
                        }
                    }
                }
                let records_here: HashMap<u32, &CellRecord> = by_row
                    .get(&r)
                    .map(|v| v.iter().map(|c| (c.col, *c)).collect())
                    .unwrap_or_default();
                let mut cells: Vec<(String, u32)> = Vec::new();
                let mut col = 0u32;
                for c in cols_here {
                    if c > col {
                        push_run(&mut cells, "<table:table-cell/>".to_owned(), c - col);
                    }
                    let rec = records_here.get(&c).copied();
                    let dv = dv_at
                        .iter()
                        .rev()
                        .find(|(g, _)| g.contains(r, c))
                        .map(|(_, n)| n.as_str());
                    let cell = self.cell(
                        id,
                        name,
                        r,
                        c,
                        rec,
                        spilled.get(&(r, c)),
                        merges.get(&(r, c)).copied(),
                        covered.contains(&(r, c)),
                        matrix.get(&(r, c)).copied(),
                        links.get(&(r, c)).copied(),
                        notes.get(&(r, c)).copied(),
                        dv,
                        p.style.is_some() || col_props.get(&c).is_some_and(|q| q.style.is_some()),
                    );
                    let cell = match frames.get(&(r, c)) {
                        Some(f) => with_frames(&cell, f),
                        None => cell,
                    };
                    push_run(&mut cells, cell, 1);
                    col = c + 1;
                }
                let mut inner = String::new();
                for (cx, n) in &cells {
                    inner.push_str(&with_repeat(cx, "table:number-columns-repeated", *n));
                }
                if inner.is_empty() {
                    inner.push_str("<table:table-cell/>");
                }
                x.raw(&inner);
                let h = if title_rows.is_some_and(|(a, b)| a <= r && r <= b) {
                    'h'
                } else {
                    '-'
                };
                let key = format!("{}|{h}|{}", p.outline_level, x.into_string());
                push_run(&mut rows, key, 1);
                r += 1;
            }
            let rest = MAX_ROWS - 1 - max_row;
            if rest > 0 {
                push_run(
                    &mut rows,
                    format!(
                        "0|-|<table:table-row table:style-name=\"{default_row}\"><table:table-cell/></table:table-row>"
                    ),
                    rest,
                );
            }
        } else {
            rows.push((
                format!("0|-|<table:table-row table:style-name=\"{default_row}\"><table:table-cell/></table:table-row>"),
                MAX_ROWS,
            ));
        }
        write_grouped(
            out,
            &rows,
            "table:table-row-group",
            "table:table-header-rows",
            "table:number-rows-repeated",
        );
        // Sheet-local names.
        let local: Vec<&kiln_sheet::meta::DefinedName> =
            wb.names().iter().filter(|n| n.scope == Some(id)).collect();
        self.named_expressions(out, &local, name);
        // Conditional formats.
        if !m.cf.is_empty() {
            out.start("calcext:conditional-formats");
            for cf in &m.cf {
                let Some(first) = cf.ranges.first() else {
                    continue;
                };
                let target: Vec<String> = cf.ranges.iter().map(|r| plain_range(name, r)).collect();
                let base = plain_range(name, &Range::cell(first.r0, first.c0));
                let dxfs: Vec<u32> = cf.rules.iter().filter_map(|r| r.kind.dxf()).collect();
                for d in &dxfs {
                    self.dxf_style(*d);
                }
                let names = self.dxf_names.clone();
                write_cf(out, cf, &target.join(" "), &base, &|d| {
                    names
                        .get(&d)
                        .cloned()
                        .unwrap_or_else(|| "Default".to_owned())
                });
            }
            out.end();
        }
        out.end();
    }

    /// The drawing frames of a sheet, by the cell they are anchored in: charts as chart objects,
    /// pictures from the workbook's passthrough store.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn frames(
        &mut self,
        s: &Sheet,
        col_props: &BTreeMap<u32, ColProps>,
        row_props: &BTreeMap<u32, RowProps>,
        default_width: f64,
        default_height: f64,
        mdw: f64,
    ) -> HashMap<(u32, u32), String> {
        use kiln_sheet::meta::{Anchor, AnchorPoint, DrawingKind};
        let mut out: HashMap<(u32, u32), String> = HashMap::new();
        if s.meta().drawings.is_empty() {
            return out;
        }
        // Column widths and row heights in EMU.
        let col_emu = |c: u32| -> i64 {
            let p = col_props.get(&c);
            if p.is_some_and(|p| p.hidden) {
                return 0;
            }
            let w = p.and_then(|p| p.width).map_or(default_width, |w| w.0);
            (w * mdw / 96.0 * 914_400.0).round() as i64
        };
        let row_emu = |r: u32| -> i64 {
            let p = row_props.get(&r);
            if p.is_some_and(|p| p.hidden) {
                return 0;
            }
            let h = p.and_then(|p| p.height).map_or(default_height, |h| h.0);
            (h * 12_700.0).round() as i64
        };
        let span = |a: &AnchorPoint, b: &AnchorPoint| -> (i64, i64) {
            let w: i64 = (a.col..b.col.min(a.col + 16_384)).map(col_emu).sum::<i64>() - a.col_off
                + b.col_off;
            let h: i64 = (a.row..b.row.min(a.row + 100_000))
                .map(row_emu)
                .sum::<i64>()
                - a.row_off
                + b.row_off;
            (w.max(1), h.max(1))
        };
        // The cell under an absolute position.
        let locate = |x: i64, y: i64| -> AnchorPoint {
            let (mut col, mut left) = (0u32, x.max(0));
            while col < 16_383 && left >= col_emu(col) && col_emu(col) > 0 {
                left -= col_emu(col);
                col += 1;
            }
            let (mut row, mut top) = (0u32, y.max(0));
            while row < 1_048_575 && top >= row_emu(row) && row_emu(row) > 0 && row < 100_000 {
                top -= row_emu(row);
                row += 1;
            }
            AnchorPoint {
                row,
                col,
                row_off: top,
                col_off: left,
            }
        };
        let sheet = s.name().to_owned();
        for d in &s.meta().drawings {
            let (from, end, size) = match d.anchor {
                Anchor::TwoCell { from, to, .. } => (from, Some(to), span(&from, &to)),
                Anchor::OneCell { from, ext } => (from, None, (ext.0.max(1), ext.1.max(1))),
                Anchor::Absolute { pos, ext } => {
                    (locate(pos.0, pos.1), None, (ext.0.max(1), ext.1.max(1)))
                }
            };
            self.frames_written += 1;
            let name = d
                .name
                .clone()
                .unwrap_or_else(|| format!("Object {}", self.frames_written));
            let inner = match d.kind {
                DrawingKind::Chart => {
                    let Some(mut cs) = self
                        .charts
                        .iter()
                        .find(|(n, _)| *n == d.reference)
                        .map(|(_, c)| c.clone())
                    else {
                        self.warn("a chart whose data is not in the workbook was not saved");
                        continue;
                    };
                    // The caches (and the object's local table) from the cells.
                    kiln_charts::data::refresh(&mut cs, self.wb);
                    if !crate::chart::supported(&cs) {
                        self.warn("a chart of a type ODF has no class for (stock, surface, treemap, …) was not saved");
                        continue;
                    }
                    let theme = drawingml_theme(self.theme);
                    let colors = kiln_charts::palette::ChartColors::new(
                        Some(&theme),
                        kiln_charts::palette::ColorMode::Document,
                    );
                    let n = (1u32..)
                        .find(|n| {
                            let p = format!("Object {n}/");
                            !self.files.iter().any(|(f, _, _)| f.starts_with(&p))
                        })
                        .unwrap_or(1);
                    let dir = format!("Object {n}");
                    self.files.push((
                        format!("{dir}/content.xml"),
                        "text/xml".to_owned(),
                        crate::chart::write_object(
                            &cs,
                            &colors,
                            size,
                            crate::chart::DataMode::Sheet,
                        ),
                    ));
                    self.files.push((
                        format!("{dir}/styles.xml"),
                        "text/xml".to_owned(),
                        crate::chart::object_styles(),
                    ));
                    self.files.push((
                        format!("{dir}/"),
                        crate::chart::CHART_MEDIA_TYPE.to_owned(),
                        Vec::new(),
                    ));
                    format!(
                        "<draw:object xlink:href=\"./{dir}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>"
                    )
                }
                DrawingKind::Picture => {
                    let Some(bytes) = self.wb.passthrough().get(&d.reference).or_else(|| {
                        self.media
                            .iter()
                            .find(|(n, _)| *n == d.reference)
                            .map(|(_, b)| b)
                    }) else {
                        self.warn("a picture whose data is not in the workbook was not saved");
                        continue;
                    };
                    let base = d
                        .reference
                        .rsplit('/')
                        .next()
                        .filter(|b| !b.is_empty())
                        .unwrap_or("image.png")
                        .to_owned();
                    let mut path = format!("Pictures/{base}");
                    let mut k = 1;
                    while self.files.iter().any(|(p, _, _)| *p == path) {
                        k += 1;
                        path = format!("Pictures/{k}-{base}");
                    }
                    let media = crate::package::media_type_for(&path).to_owned();
                    self.files.push((path.clone(), media, bytes.clone()));
                    let mut x = XmlOut::fragment();
                    x.start("draw:image")
                        .attr("xlink:href", &path)
                        .attr("xlink:type", "simple")
                        .attr("xlink:show", "embed")
                        .attr("xlink:actuate", "onLoad");
                    x.start("text:p").end();
                    x.end();
                    x.into_string()
                }
                _ => {
                    self.warn("shapes and groups on sheets are not saved in ODF");
                    continue;
                }
            };
            // Frames show no border or fill of their own.
            let gr = self.autos.add(
                "gr",
                El::new("style:style")
                    .with("style:family", "graphic")
                    .with_child(
                        El::new("style:graphic-properties")
                            .with("draw:stroke", "none")
                            .with("draw:fill", "none"),
                    ),
            );
            let mut x = XmlOut::fragment();
            x.start("draw:frame")
                .attr("draw:z-index", (self.frames_written - 1).to_string())
                .attr("draw:style-name", gr)
                .attr("draw:name", &name)
                .attr("svg:width", crate::units::emu_str(size.0))
                .attr("svg:height", crate::units::emu_str(size.1))
                .attr("svg:x", crate::units::emu_str(from.col_off.max(0)))
                .attr("svg:y", crate::units::emu_str(from.row_off.max(0)));
            if let Some(to) = end {
                x.attr(
                    "table:end-cell-address",
                    crate::formula::cell_address(&sheet, to.row, to.col),
                )
                .attr("table:end-x", crate::units::emu_str(to.col_off.max(0)))
                .attr("table:end-y", crate::units::emu_str(to.row_off.max(0)));
            }
            x.raw(&inner);
            if let Some(t) = &d.alt_text {
                x.text_element("svg:desc", t);
            }
            x.end();
            out.entry((from.row, from.col))
                .or_default()
                .push_str(&x.into_string());
        }
        out
    }

    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_owned());
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn cell(
        &mut self,
        sheet: SheetId,
        sheet_name: &str,
        r: u32,
        c: u32,
        rec: Option<&CellRecord>,
        spilled: Option<&CellValue>,
        merge: Option<(u32, u32)>,
        covered: bool,
        matrix: Option<(u32, u32)>,
        link: Option<&kiln_sheet::meta::Hyperlink>,
        note: Option<&kiln_sheet::meta::Note>,
        validation: Option<&str>,
        inherits: bool,
    ) -> String {
        let wb = self.wb;
        let tag = if covered {
            "table:covered-table-cell"
        } else {
            "table:table-cell"
        };
        let mut x = XmlOut::fragment();
        x.start(tag);
        let xf = rec.map_or_else(|| wb.cell(sheet, r, c).map_or(0, |d| d.style), |c| c.style);
        if let Some(st) = self.cell_style(xf) {
            x.attr("table:style-name", st);
        } else if inherits {
            // The row's or column's default style would apply otherwise.
            x.attr("table:style-name", "Default");
        }
        if let Some(v) = validation {
            x.attr("table:content-validation-name", v);
        }
        if let Some((rows, cols)) = merge
            && !covered
        {
            x.attr("table:number-columns-spanned", cols.to_string())
                .attr("table:number-rows-spanned", rows.to_string());
        }
        let formula = rec.and_then(|c| c.formula.as_deref());
        let value: CellValue = match (rec, spilled) {
            (Some(rec), _) if rec.formula.is_some() => wb.value(sheet, r, c),
            (Some(rec), _) => rec.value.clone(),
            (None, Some(v)) => v.clone(),
            (None, None) => CellValue::Blank,
        };
        if let Some(f) = formula {
            // ODF has no structured references: they become the ranges they denote.
            let mut text = f.to_owned();
            if text.contains('[') {
                for s in wb.sheets() {
                    for t in &s.meta().tables {
                        if let Some(x) = kiln_sheet::structured::structured_to_a1(
                            &text,
                            t,
                            s.name(),
                            sheet_name,
                            Some(kiln_sheet::CellPos::new(r, c)),
                        ) {
                            text = x;
                        }
                    }
                }
            }
            x.attr("table:formula", excel_to_odf_with(&text, &|_| None));
            if let Some((rows, cols)) = matrix {
                x.attr("table:number-matrix-columns-spanned", cols.to_string())
                    .attr("table:number-matrix-rows-spanned", rows.to_string());
            }
        }
        let code = wb.styles().get(xf).num_fmt.to_string();
        let display = if matches!(value, CellValue::Number(_)) {
            wb.display_text(sheet, r, c)
        } else {
            String::new()
        };
        match &value {
            CellValue::Number(n) => {
                let nf = NumberFormat::parse(&code);
                let lower = code.to_ascii_lowercase();
                let has_date = nf.is_date()
                    && (lower.contains('d')
                        || lower.contains('y')
                        || (lower.contains('m') && !lower.contains('h') && !lower.contains('s')));
                if nf.is_date() && has_date {
                    let serial = if wb.props().date1904 { n + 1462.0 } else { *n };
                    x.attr("office:value-type", "date")
                        .attr("office:date-value", serial_date_str(serial))
                        .attr("calcext:value-type", "date");
                } else if nf.is_date() {
                    x.attr("office:value-type", "time")
                        .attr("office:time-value", duration_str(*n))
                        .attr("calcext:value-type", "time");
                } else if nf.is_percent() {
                    x.attr("office:value-type", "percentage")
                        .attr("office:value", crate::units::number_str(*n))
                        .attr("calcext:value-type", "percentage");
                } else {
                    x.attr("office:value-type", "float")
                        .attr("office:value", crate::units::number_str(*n))
                        .attr("calcext:value-type", "float");
                }
            }
            CellValue::Bool(b) => {
                x.attr("office:value-type", "boolean")
                    .attr("office:boolean-value", b.to_string())
                    .attr("calcext:value-type", "boolean");
            }
            CellValue::Error(_) => {
                x.attr("office:value-type", "string")
                    .attr("office:string-value", "")
                    .attr("calcext:value-type", "error");
            }
            CellValue::Text(_) | CellValue::RichText(_) => {
                x.attr("office:value-type", "string")
                    .attr("calcext:value-type", "string");
            }
            CellValue::Blank => {
                if formula.is_some() {
                    x.attr("office:value-type", "string")
                        .attr("office:string-value", "")
                        .attr("calcext:value-type", "string");
                }
            }
        }
        if let Some(n) = note {
            x.start("office:annotation");
            if n.visible {
                x.attr("office:display", "true");
            }
            if !n.author.is_empty() {
                x.text_element("dc:creator", &n.author);
            }
            for line in n.text.split('\n') {
                x.start("text:p");
                write_text(&mut x, line);
                x.end();
            }
            x.end();
        }
        let link_start = |x: &mut XmlOut| {
            if let Some(h) = link {
                let href = match (&h.target, &h.location) {
                    (Some(t), Some(l)) => format!("{t}#{l}"),
                    (Some(t), None) => t.clone(),
                    (None, Some(l)) => format!("#{}", location_to_odf(l)),
                    (None, None) => String::new(),
                };
                x.start("text:a")
                    .attr("xlink:href", href)
                    .attr("xlink:type", "simple");
            }
        };
        // Whole-cell superscript and subscript: LibreOffice applies them to text spans only.
        let raised = match wb.styles().get(xf).font.vert_align {
            VertAlign::Baseline => None,
            v => self.text_style(&FontPatch {
                vert_align: Some(v),
                ..FontPatch::default()
            }),
        };
        match &value {
            CellValue::Text(t) => {
                for line in t.split('\n') {
                    x.start("text:p");
                    link_start(&mut x);
                    if let Some(st) = &raised {
                        x.start("text:span").attr("text:style-name", st);
                    }
                    write_text(&mut x, line);
                    if raised.is_some() {
                        x.end();
                    }
                    if link.is_some() {
                        x.end();
                    }
                    x.end();
                }
            }
            CellValue::RichText(rt) => {
                // Runs split at line breaks into paragraphs.
                let mut lines: Vec<Vec<(String, Option<FontPatch>)>> = vec![Vec::new()];
                for run in rt.runs() {
                    for (i, part) in run.text.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(Vec::new());
                        }
                        if !part.is_empty()
                            && let Some(l) = lines.last_mut()
                        {
                            l.push((part.to_owned(), run.font.clone()));
                        }
                    }
                }
                for line in lines {
                    x.start("text:p");
                    link_start(&mut x);
                    for (text, font) in line {
                        match font.as_ref().and_then(|f| self.text_style(f)) {
                            Some(st) => {
                                x.start("text:span").attr("text:style-name", st);
                                write_text(&mut x, &text);
                                x.end();
                            }
                            None => write_text(&mut x, &text),
                        }
                    }
                    if link.is_some() {
                        x.end();
                    }
                    x.end();
                }
            }
            CellValue::Number(_) => {
                x.start("text:p");
                link_start(&mut x);
                write_text(&mut x, &display);
                if link.is_some() {
                    x.end();
                }
                x.end();
            }
            CellValue::Bool(b) => {
                x.start("text:p")
                    .text(if *b { "TRUE" } else { "FALSE" })
                    .end();
            }
            CellValue::Error(e) => {
                x.start("text:p").text(e.code()).end();
            }
            CellValue::Blank => {
                if let Some(h) = link {
                    x.start("text:p");
                    link_start(&mut x);
                    x.text(h.display.as_deref().unwrap_or_default());
                    x.end();
                    x.end();
                }
            }
        }
        x.end();
        x.into_string()
    }

    fn styles_xml(&mut self) -> Vec<u8> {
        let mut w = XmlOut::new();
        w.root("office:document-styles", PREFIXES)
            .attr("office:version", "1.3");
        let mut fonts = self.fonts.clone();
        fonts.insert(self.default_font.name.to_string());
        font_decls(&mut w, &fonts, &self.default_font.name);
        w.start("office:styles");
        let mut tp = text_props_el(&self.default_font, None, self.theme);
        tp.set("fo:language", "en");
        tp.set("fo:country", "US");
        El::new("style:default-style")
            .with("style:family", "table-cell")
            .with_child(
                El::new("style:paragraph-properties").with("style:tab-stop-distance", "0.5in"),
            )
            .with_child(tp.clone())
            .write(&mut w);
        El::new("style:style")
            .with("style:name", "Default")
            .with("style:family", "table-cell")
            .with_child(tp)
            .write(&mut w);
        for e in &self.common {
            e.write(&mut w);
        }
        w.end();
        w.start("office:automatic-styles");
        for e in &self.layouts {
            e.write(&mut w);
        }
        w.end();
        w.start("office:master-styles");
        for e in &self.masters {
            e.write(&mut w);
        }
        w.end();
        w.finish()
    }
}

/// Whether a formula computes differently with implicit intersection: ranges combined by
/// operators (`SUM(A1:A10*B1:B10)`) or array constants.
fn array_semantics(f: &str) -> bool {
    let mut quoted = false;
    let (mut range, mut op) = (false, false);
    for c in f.chars() {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            ':' => range = true,
            '{' => return true,
            '*' | '/' | '+' | '-' | '^' | '&' | '=' | '<' | '>' => op = true,
            _ => {}
        }
    }
    range && op
}

fn push_run(v: &mut Vec<(String, u32)>, key: String, n: u32) {
    if n == 0 {
        return;
    }
    // Formula cells (and rows holding one) are never folded into a repeat: LibreOffice keeps a
    // single copy of a repeated formula cell, so the cells after it would shift.
    let formula = key.contains("table:formula=");
    match v.last_mut() {
        Some((k, c)) if *k == key && !formula => *c += n,
        _ if formula => v.extend(std::iter::repeat_n((key, 1), n as usize)),
        _ => v.push((key, n)),
    }
}

/// The workbook theme as a DrawingML theme (the colours of charts).
fn drawingml_theme(t: &Theme) -> kiln_ooxml::drawingml::theme::Theme {
    let mut theme = kiln_ooxml::drawingml::theme::office_theme();
    for (slot, c) in theme.color_scheme.colors.iter_mut().zip(t.colors) {
        *slot = kiln_ooxml::drawingml::color::DrawingColor::srgb(&format!("{:06X}", c & 0xFF_FFFF));
    }
    theme
}

/// A cell's XML with drawing frames inside it.
fn with_frames(cell: &str, frames: &str) -> String {
    let covered = cell.starts_with("<table:covered-table-cell");
    let close = if covered {
        "</table:covered-table-cell>"
    } else {
        "</table:table-cell>"
    };
    if let Some(open) = cell.strip_suffix("/>") {
        format!("{open}>{frames}{close}")
    } else if let Some(body) = cell.strip_suffix(close) {
        format!("{body}{frames}{close}")
    } else {
        cell.to_owned()
    }
}

/// Adds a repeat attribute to the first element of `xml` when `n` > 1.
fn with_repeat(xml: &str, attr: &str, n: u32) -> String {
    if n <= 1 {
        return xml.to_owned();
    }
    let end = xml.find([' ', '>', '/']).unwrap_or(xml.len());
    format!("{} {attr}=\"{n}\"{}", &xml[..end], &xml[end..])
}

/// Writes rows or columns keyed `level|header|xml` with their repeat counts, nesting outline
/// groups and wrapping print titles (`h`) in the header element.
fn write_grouped(out: &mut XmlOut, items: &[(String, u32)], group: &str, header: &str, attr: &str) {
    let mut depth = 0u8;
    let mut in_header = false;
    for (key, n) in items {
        let mut parts = key.splitn(3, '|');
        let level: u8 = parts.next().and_then(|l| l.parse().ok()).unwrap_or(0);
        let h = parts.next() == Some("h");
        let xml = parts.next().unwrap_or_default();
        // Header rows sit outside outline groups.
        let level = if h { 0 } else { level };
        if in_header && !h {
            out.raw(&format!("</{header}>"));
            in_header = false;
        }
        while depth < level {
            out.raw(&format!("<{group}>"));
            depth += 1;
        }
        while depth > level {
            out.raw(&format!("</{group}>"));
            depth -= 1;
        }
        if h && !in_header {
            out.raw(&format!("<{header}>"));
            in_header = true;
        }
        out.raw(&with_repeat(xml, attr, *n));
    }
    if in_header {
        out.raw(&format!("</{header}>"));
    }
    while depth > 0 {
        out.raw(&format!("</{group}>"));
        depth -= 1;
    }
}

/// Text with ODF white space: runs of spaces as `text:s`, tabs as `text:tab`.
fn write_text(x: &mut XmlOut, s: &str) {
    let mut buf = String::new();
    let mut spaces = 0usize;
    let mut at_start = true;
    let flush_spaces = |x: &mut XmlOut, buf: &mut String, spaces: &mut usize, at_start: bool| {
        if *spaces == 0 {
            return;
        }
        // One space is plain text unless it leads the paragraph.
        if *spaces == 1 && !at_start {
            buf.push(' ');
        } else {
            if !buf.is_empty() {
                x.text(buf);
                buf.clear();
            }
            x.start("text:s");
            if *spaces > 1 {
                x.attr("text:c", spaces.to_string());
            }
            x.end();
        }
        *spaces = 0;
    };
    for ch in s.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => {
                flush_spaces(x, &mut buf, &mut spaces, at_start);
                if !buf.is_empty() {
                    x.text(&buf);
                    buf.clear();
                }
                x.start("text:tab").end();
                at_start = false;
            }
            c => {
                if spaces > 1 || (spaces == 1 && at_start) {
                    flush_spaces(x, &mut buf, &mut spaces, at_start);
                } else if spaces == 1 {
                    buf.push(' ');
                    spaces = 0;
                }
                buf.push(c);
                at_start = false;
            }
        }
    }
    // Trailing spaces must be explicit or readers drop them.
    if spaces > 0 {
        if !buf.is_empty() {
            x.text(&buf);
            buf.clear();
        }
        x.start("text:s");
        if spaces > 1 {
            x.attr("text:c", spaces.to_string());
        }
        x.end();
    }
    if !buf.is_empty() {
        x.text(&buf);
    }
}

/// `Sheet.A1:Sheet.B2` (the form of `calcext` addresses).
fn plain_range(sheet: &str, r: &Range) -> String {
    let q = if sheet.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !sheet.starts_with(|c: char| c.is_ascii_digit())
    {
        sheet.to_owned()
    } else {
        format!("'{}'", sheet.replace('\'', "''"))
    };
    let a = kiln_sheet::address::cell_a1(r.r0, r.c0, false);
    if r.is_single_cell() {
        format!("{q}.{a}")
    } else {
        format!(
            "{q}.{a}:{q}.{}",
            kiln_sheet::address::cell_a1(r.r1, r.c1, false)
        )
    }
}

/// A hyperlink location (`Sheet2!A1`) as an ODF link target (`Sheet2.A1`).
fn location_to_odf(l: &str) -> String {
    let odf = excel_to_odf_with(l, &|_| None);
    let t = odf.trim_start_matches("of:=");
    match t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Some(inner) => inner
            .trim_start_matches('$')
            .replacen(".$", ".", 1)
            .replace('$', ""),
        None => l.to_owned(),
    }
}

fn font_decls(w: &mut XmlOut, fonts: &BTreeSet<String>, default: &str) {
    w.start("office:font-face-decls");
    let mut all = fonts.clone();
    all.insert(default.to_owned());
    for f in all {
        w.start("style:font-face")
            .attr("style:name", &f)
            .attr("svg:font-family", font_family_value(&f))
            .end();
    }
    w.end();
}

fn settings_xml(wb: &Workbook) -> Vec<u8> {
    let mut w = XmlOut::new();
    w.root("office:document-settings", &["office", "config", "ooo"])
        .attr("office:version", "1.3");
    w.start("office:settings");
    w.start("config:config-item-set")
        .attr("config:name", "ooo:view-settings");
    w.start("config:config-item-map-indexed")
        .attr("config:name", "Views");
    w.start("config:config-item-map-entry");
    let item = |w: &mut XmlOut, name: &str, ty: &str, v: &str| {
        w.start("config:config-item")
            .attr("config:name", name)
            .attr("config:type", ty)
            .text(v)
            .end();
    };
    item(&mut w, "ViewId", "string", "view1");
    w.start("config:config-item-map-named")
        .attr("config:name", "Tables");
    for s in wb.sheets() {
        let v = &s.meta().view;
        w.start("config:config-item-map-entry")
            .attr("config:name", s.name());
        let int = |w: &mut XmlOut, n: &str, x: u32| item(w, n, "int", &x.to_string());
        int(&mut w, "CursorPositionX", v.selection.active.col);
        int(&mut w, "CursorPositionY", v.selection.active.row);
        let (rows, cols) = v.freeze.map_or((0, 0), |f| (f.rows, f.cols));
        item(
            &mut w,
            "HorizontalSplitMode",
            "short",
            if cols > 0 { "2" } else { "0" },
        );
        item(
            &mut w,
            "VerticalSplitMode",
            "short",
            if rows > 0 { "2" } else { "0" },
        );
        int(&mut w, "HorizontalSplitPosition", cols);
        int(&mut w, "VerticalSplitPosition", rows);
        let active = match (rows > 0, cols > 0) {
            (true, true) => "3",
            (true, false) => "2",
            (false, true) => "3",
            (false, false) => "2",
        };
        item(&mut w, "ActiveSplitRange", "short", active);
        int(
            &mut w,
            "PositionLeft",
            if cols > 0 { 0 } else { v.top_left.col },
        );
        int(
            &mut w,
            "PositionRight",
            if cols > 0 {
                v.top_left.col.max(cols)
            } else {
                0
            },
        );
        int(
            &mut w,
            "PositionTop",
            if rows > 0 { 0 } else { v.top_left.row },
        );
        int(
            &mut w,
            "PositionBottom",
            if rows > 0 {
                v.top_left.row.max(rows)
            } else {
                0
            },
        );
        item(&mut w, "ZoomType", "short", "0");
        int(&mut w, "ZoomValue", u32::from(v.zoom));
        int(&mut w, "PageViewZoomValue", 60);
        item(&mut w, "ShowGrid", "boolean", &v.show_gridlines.to_string());
        w.end();
    }
    w.end();
    let active = wb
        .sheets()
        .get(wb.props().active_sheet)
        .or_else(|| wb.sheets().first())
        .map_or("Sheet1", |s| s.name());
    item(&mut w, "ActiveTable", "string", active);
    item(&mut w, "ShowGrid", "boolean", "true");
    w.end();
    w.end();
    w.end();
    w.start("config:config-item-set")
        .attr("config:name", "ooo:configuration-settings");
    item(&mut w, "HasColumnRowHeaders", "boolean", "true");
    w.end();
    w.finish()
}
