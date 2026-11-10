// SPDX-License-Identifier: AGPL-3.0-or-later
//! `.ods` → [`kiln_sheet::Workbook`].
//!
//! - **Sheets** (`table:table`): name, visibility and tab colour (the table style), protection,
//!   print ranges (`_xlnm.Print_Area`), page setup (orientation, paper, margins, scale from the
//!   master page's page layout).
//! - **Rows and columns**: heights (`style:row-height`, `use-optimal-row-height` false = a
//!   custom height), widths (converted to Excel's character widths with the default font's
//!   digit width), hidden and filtered, outline groups, default cell styles. The trailing
//!   filler row and column LibreOffice writes give the sheet's default sizes.
//! - **Cells**: every value type (float, percentage, currency, date, time, boolean, string,
//!   errors), rich text spans, hyperlinks (`text:a`), notes (`office:annotation`), formulas
//!   (`table:formula`, translated by [`crate::formula`]) with their cached results, matrix
//!   formulas as array formulas, merges (`number-columns-spanned`/`number-rows-spanned`),
//!   styles (cell, row and column default styles over `Default`) and validation names.
//!   Repeated rows and cells are expanded only where they hold something; formatting repeated
//!   over a whole row or column becomes the row's or column's style.
//! - **Workbook**: named ranges and expressions, database ranges with filter buttons
//!   (AutoFilter), conditional formats ([`super::rules`]), the null date (1904 system),
//!   iteration settings, and the view settings of `settings.xml` (active sheet, frozen panes,
//!   zoom, gridlines, cursor).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use kiln_formula::ErrorValue;
use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::axis::{
    BASE_COL_WIDTH, ColProps, DefaultFontMetrics, FontMetrics, RowProps, default_col_width_for,
};
use kiln_sheet::meta::{
    AutoFilter, CalcMode, DefinedName, Freeze, Hyperlink, Note, Orientation, SheetProtection,
    SheetVisibility, WorkbookProps,
};
use kiln_sheet::num::F64;
use kiln_sheet::serial::{SheetSnapshot, WorkbookSnapshot};
use kiln_sheet::styles::{CellStyle, FontPatch, StyleTable};
use kiln_sheet::value::{RichText, TextRun};
use kiln_sheet::workbook::CellRecord;
use kiln_sheet::{
    CellPos, CellValue, Command, MAX_COLS, MAX_ROWS, Range, SheetId, SheetLayout, SheetMeta, Theme,
    Workbook, WorkbookOptions,
};

use super::rules::{dxf_from_style, read_cf, read_validation};
use super::style::{background, points_exact, resolve_chain};
use crate::formula::{odf_to_excel, parse_range_list};
use crate::numfmt::read_data_style_in;
use crate::package::{OdfKind, Package};
use crate::props::FontFaces;
use crate::text::styles::{Scope, StyleIndex};
use crate::units::{date_serial, duration_days};
use crate::xml::{all_text, kid, kids, parse_part};
use crate::{OdfError, meta};

/// A workbook as read.
#[derive(Debug)]
pub struct OdsRead {
    /// The workbook, calculated.
    pub workbook: Workbook,
    /// Document properties (`meta.xml`).
    pub properties: kiln_doc::DocProperties,
    /// What could not be represented exactly.
    pub warnings: Vec<String>,
    /// The file was a template (`.ots`).
    pub template: bool,
    /// The charts of the sheets' chart drawings, by the part name their anchors reference.
    pub charts: Vec<(String, kiln_charts::model::ChartSpace)>,
}

/// Reads an `.ods` (or `.ots`) file and calculates it.
///
/// # Errors
///
/// [`OdfError`] when the file is not a readable ODF spreadsheet.
pub fn read_ods(bytes: &[u8]) -> Result<OdsRead, OdfError> {
    let pkg = Package::open(bytes)?;
    read_package(&pkg)
}

/// Most styled-but-empty cells a single repeated element is expanded into.
const MAX_STYLED_EXPANSION: u64 = 65_536;
/// Most cells read in total (a guard against hostile repeat counts).
const MAX_CELLS: usize = 20_000_000;
/// Most extra copies made of repeated cells holding a value.
const MAX_REPEATED_VALUES: usize = 1_000_000;
/// Most extra copies made of repeated formula cells (each is calculated on open).
const MAX_REPEATED_FORMULAS: usize = 100_000;

/// A display name as ODF encodes it into a style name: characters other than letters, digits,
/// `-` and `.` become `_xx_` (their hex code), so `_` is `_5f_` and a space `_20_`.
fn encode_style_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() || c == '-' || c == '.' {
            out.push(c);
        } else {
            out.push_str(&format!("_{:x}_", u32::from(c)));
        }
    }
    out
}

fn parse_u32(s: Option<&str>) -> Option<u32> {
    s.and_then(|v| v.trim().parse::<u32>().ok())
}

fn repeat(e: &XmlElement, attr: &str) -> u32 {
    parse_u32(e.attr(attr)).unwrap_or(1).max(1)
}

/// The paragraph text of a cell: ODF white space handling, `text:s`, `text:tab`,
/// `text:line-break`; spans with their text styles; links.
#[derive(Debug, Default)]
struct CellText {
    runs: Vec<(String, Option<String>)>,
    link: Option<String>,
}

impl CellText {
    fn push(&mut self, s: &str, style: Option<&str>) {
        if s.is_empty() {
            return;
        }
        match self.runs.last_mut() {
            Some((t, st)) if st.as_deref() == style => t.push_str(s),
            _ => self.runs.push((s.to_owned(), style.map(str::to_owned))),
        }
    }

    fn plain(&self) -> String {
        self.runs.iter().map(|(t, _)| t.as_str()).collect()
    }
}

fn collect_text(el: &XmlElement, style: Option<&str>, out: &mut CellText, space: &mut bool) {
    for c in &el.children {
        match c {
            XmlNode::Text(t) => {
                let mut s = String::new();
                for ch in t.text.chars() {
                    if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                        if !*space {
                            s.push(' ');
                            *space = true;
                        }
                    } else {
                        s.push(ch);
                        *space = false;
                    }
                }
                out.push(&s, style);
            }
            XmlNode::Element(e) => match e.name.as_str() {
                "text:s" => {
                    let n = repeat(e, "text:c").min(10_000) as usize;
                    out.push(&" ".repeat(n), style);
                    *space = false;
                }
                "text:tab" => {
                    out.push("\t", style);
                    *space = false;
                }
                "text:line-break" => {
                    out.push("\n", style);
                    *space = true;
                }
                "text:span" => {
                    let st = e.attr("text:style-name").or(style);
                    collect_text(e, st, out, space);
                }
                "text:a" => {
                    if out.link.is_none() {
                        out.link = e.attr("xlink:href").map(str::to_owned);
                    }
                    let st = e.attr("text:style-name").or(style);
                    collect_text(e, st, out, space);
                }
                "office:annotation"
                | "office:annotation-end"
                | "text:bookmark"
                | "text:bookmark-start"
                | "text:bookmark-end" => {}
                _ => collect_text(e, style, out, space),
            },
            _ => {}
        }
    }
}

fn paragraphs_text(cell: &XmlElement) -> CellText {
    let mut out = CellText::default();
    let mut first = true;
    for p in cell
        .elements()
        .filter(|e| e.name == "text:p" || e.name == "text:h")
    {
        if !first {
            out.push("\n", None);
        }
        first = false;
        let mut space = true;
        collect_text(p, None, &mut out, &mut space);
        // Trailing collapsible white space of a paragraph is dropped.
        if let Some((t, _)) = out.runs.last_mut()
            && space
            && t.ends_with(' ')
        {
            t.pop();
        }
    }
    out.runs.retain(|(t, _)| !t.is_empty());
    out
}

struct Reader<'a> {
    pkg: &'a Package,
    /// Charts of the drawings, by the part name their anchors reference.
    charts: Vec<(String, kiln_charts::model::ChartSpace)>,
    /// Pictures of the drawings, for the workbook's passthrough store.
    pictures: Vec<(String, Vec<u8>)>,
    ix: StyleIndex<'a>,
    fonts: FontFaces,
    styles: StyleTable,
    base: CellStyle,
    /// Cell style name → style index.
    xf_cache: HashMap<String, u32>,
    /// Data style name → Excel code.
    codes: HashMap<String, String>,
    dxf_cache: HashMap<String, u32>,
    warnings: Vec<String>,
    date1904: bool,
    mdw: f64,
    rule_id: u32,
    cells_read: usize,
    /// Cells created by repeating a value cell (beyond its first copy).
    repeated_values: usize,
    /// Cells created by repeating a formula cell (beyond its first copy).
    repeated_formulas: usize,
    content_validations: Option<&'a XmlElement>,
}

impl<'a> Reader<'a> {
    /// Whether one more copy of a repeated cell with content may be made: the copies are
    /// bounded ([`MAX_REPEATED_VALUES`], [`MAX_REPEATED_FORMULAS`]), so a few hundred bytes of
    /// repeat counts cannot ask for billions of cells.
    fn allow_repeat(&mut self, formula: bool) -> bool {
        let (used, max) = if formula {
            (&mut self.repeated_formulas, MAX_REPEATED_FORMULAS)
        } else {
            (&mut self.repeated_values, MAX_REPEATED_VALUES)
        };
        if *used >= max {
            self.warn(
                "the spreadsheet repeats cells more often than Kiln expands them; the extra copies were skipped",
            );
            return false;
        }
        *used += 1;
        true
    }

    /// Drawing frames in cells (cell anchors) and in `table:shapes` (sheet positions): charts
    /// and pictures.
    fn frames(
        &mut self,
        t: &'a XmlElement,
        rows: &[(&'a XmlElement, u8, bool)],
        meta: &mut SheetMeta,
    ) {
        use kiln_sheet::meta::{Anchor, AnchorPoint, DrawingAnchor, DrawingKind};
        let mut found: Vec<(&'a XmlElement, Option<(u32, u32)>)> = Vec::new();
        if let Some(shapes) = kid(t, "table:shapes") {
            for f in kids(shapes, "draw:frame") {
                found.push((f, None));
            }
        }
        let mut row = 0u32;
        for (r_el, _, _) in rows {
            if row >= MAX_ROWS || found.len() >= 10_000 {
                break;
            }
            let mut col = 0u32;
            for cell in r_el.elements() {
                if !cell.name.ends_with("table-cell") || col >= MAX_COLS {
                    continue;
                }
                for f in kids(cell, "draw:frame") {
                    found.push((f, Some((row, col))));
                }
                col = col.saturating_add(repeat(cell, "table:number-columns-repeated"));
            }
            row = row.saturating_add(repeat(r_el, "table:number-rows-repeated"));
        }
        for (f, at) in found {
            let len = |n: &str| f.attr(n).and_then(crate::units::emu).unwrap_or(0).max(0);
            let (w, h) = (len("svg:width").max(1), len("svg:height").max(1));
            let (x, y) = (len("svg:x"), len("svg:y"));
            let anchor = match at {
                Some((row, col)) => {
                    let from = AnchorPoint {
                        row,
                        col,
                        row_off: y,
                        col_off: x,
                    };
                    let end = f
                        .attr("table:end-cell-address")
                        .and_then(|a| crate::formula::parse_range_list(a).into_iter().next());
                    match end {
                        Some((_, r)) => Anchor::TwoCell {
                            from,
                            to: AnchorPoint {
                                row: r.r0,
                                col: r.c0,
                                row_off: len("table:end-y"),
                                col_off: len("table:end-x"),
                            },
                            edit_as: kiln_sheet::meta::EditAs::OneCell,
                        },
                        None => Anchor::OneCell { from, ext: (w, h) },
                    }
                }
                None => Anchor::Absolute {
                    pos: (x, y),
                    ext: (w, h),
                },
            };
            let chart = kid(f, "draw:object")
                .and_then(|o| o.attr("xlink:href"))
                .and_then(|h| crate::chart::read_linked(self.pkg, h));
            let (kind, reference) = if let Some(cs) = chart {
                let part = format!("/xl/charts/chart{}.xml", self.charts.len() + 1);
                self.charts.push((part.clone(), cs));
                (DrawingKind::Chart, part)
            } else if let Some(href) = kid(f, "draw:image")
                .and_then(|i| i.attr("xlink:href"))
                .filter(|_| kid(f, "draw:object").is_none() && kid(f, "draw:object-ole").is_none())
            {
                let path = href.trim_start_matches("./");
                let Some(bytes) = self.pkg.file(path) else {
                    self.warn("a picture whose file is missing was not imported");
                    continue;
                };
                let base = path.rsplit('/').next().unwrap_or("image.png");
                let mut part = format!("/xl/media/{base}");
                let mut k = 1;
                while self.pictures.iter().any(|(p, _)| *p == part) {
                    k += 1;
                    part = format!("/xl/media/{k}-{base}");
                }
                self.pictures.push((part.clone(), bytes.to_vec()));
                (DrawingKind::Picture, part)
            } else {
                self.warn("embedded objects other than charts on sheets are not imported");
                continue;
            };
            meta.drawings.push(DrawingAnchor {
                id: (meta.drawings.len() + 1).to_string(),
                kind,
                reference,
                anchor,
                name: f.attr("draw:name").map(str::to_owned),
                alt_text: kid(f, "svg:desc").map(all_text).filter(|t| !t.is_empty()),
                hidden: false,
                locked: true,
                look: Default::default(),
            });
        }
    }

    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_owned());
        }
    }

    /// A style and its parents, root first.
    fn chain(&self, family: &str, name: &str) -> Vec<&'a XmlElement> {
        let mut out = Vec::new();
        let mut cur = self.ix.get(Scope::Content, family, name);
        let mut guard = 0;
        while let Some(s) = cur {
            out.push(s);
            guard += 1;
            if guard > 32 {
                break;
            }
            cur = s
                .attr("style:parent-style-name")
                .and_then(|p| self.ix.get(Scope::Styles, family, p));
        }
        out.reverse();
        out
    }

    fn code_of(&mut self, data_style: &str) -> Option<String> {
        if let Some(c) = self.codes.get(data_style) {
            return Some(c.clone());
        }
        let el = self.ix.data_styles.get(data_style)?;
        let code = read_data_style_in(el, &self.ix.data_styles)?;
        self.codes.insert(data_style.to_owned(), code.clone());
        Some(code)
    }

    fn xf(&mut self, name: Option<&str>) -> u32 {
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            return 0;
        };
        if let Some(i) = self.xf_cache.get(name) {
            return *i;
        }
        let chain = self.chain("table-cell", name);
        let mut cs = resolve_chain(&self.base, &chain, &self.fonts);
        let data = chain
            .iter()
            .rev()
            .find_map(|s| s.attr("style:data-style-name"));
        if let Some(d) = data
            && let Some(code) = self.code_of(d)
        {
            cs.num_fmt = Arc::from(code.as_str());
        }
        let i = if cs == self.base {
            0
        } else {
            self.styles.intern(&cs)
        };
        self.xf_cache.insert(name.to_owned(), i);
        i
    }

    fn dxf(&mut self, name: &str) -> u32 {
        if let Some(i) = self.dxf_cache.get(name) {
            return *i;
        }
        // LibreOffice names the style by its display name (`ConditionalStyle_1` for the style
        // `ConditionalStyle_5f_1`).
        let style = self
            .ix
            .get(Scope::Styles, "table-cell", name)
            .or_else(|| {
                self.ix
                    .get(Scope::Styles, "table-cell", &encode_style_name(name))
            })
            .or_else(|| self.ix.get(Scope::Content, "table-cell", name));
        let d = match style {
            Some(s) => {
                let code = s
                    .attr("style:data-style-name")
                    .and_then(|d| self.code_of(d));
                dxf_from_style(s, &self.fonts, code)
            }
            None => kiln_sheet::styles::Dxf::default(),
        };
        let i = self.styles.intern_dxf(d);
        self.dxf_cache.insert(name.to_owned(), i);
        i
    }

    fn font_patch(&self, text_style: &str) -> Option<FontPatch> {
        let st = self.ix.get(Scope::Content, "text", text_style)?;
        let d = dxf_from_style(st, &self.fonts, None);
        d.font
    }

    fn value_of(&self, c: &XmlElement, text: &CellText) -> CellValue {
        let attr = |n: &str| c.attr(n);
        let calc_type = attr("calcext:value-type");
        if calc_type == Some("error") {
            let t = text.plain();
            return ErrorValue::from_code(t.trim())
                .map_or(CellValue::Error(ErrorValue::NA), CellValue::Error);
        }
        match attr("office:value-type") {
            Some("float" | "percentage" | "currency") => attr("office:value")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .map_or(CellValue::Blank, CellValue::Number),
            Some("date") => attr("office:date-value")
                .and_then(date_serial)
                .map_or(CellValue::Blank, |s| {
                    CellValue::Number(if self.date1904 { s - 1462.0 } else { s })
                }),
            Some("time") => attr("office:time-value")
                .and_then(duration_days)
                .map_or(CellValue::Blank, CellValue::Number),
            Some("boolean") => {
                CellValue::Bool(matches!(attr("office:boolean-value"), Some("true" | "1")))
            }
            Some("string") => {
                if let Some(s) = attr("office:string-value") {
                    return CellValue::Text(Arc::from(s));
                }
                self.text_value(text)
            }
            _ => CellValue::Blank,
        }
    }

    /// The formatting of a single span covering the whole text: LibreOffice writes whole-cell
    /// superscript and subscript that way, and it belongs to the cell's format.
    fn whole_span(&self, text: &CellText) -> Option<FontPatch> {
        match text.runs.as_slice() {
            [(_, Some(st))] => self.font_patch(st),
            _ => None,
        }
    }

    fn text_value(&self, text: &CellText) -> CellValue {
        if text.runs.iter().all(|(_, s)| s.is_none()) || self.whole_span(text).is_some() {
            return CellValue::Text(Arc::from(text.plain().as_str()));
        }
        let runs: Vec<TextRun> = text
            .runs
            .iter()
            .map(|(t, s)| TextRun {
                text: t.clone(),
                font: s.as_deref().and_then(|n| self.font_patch(n)),
            })
            .collect();
        if runs.iter().all(|r| r.font.is_none()) {
            return CellValue::Text(Arc::from(text.plain().as_str()));
        }
        CellValue::RichText(Arc::new(RichText::new(runs)))
    }

    fn col_width(&self, style: Option<&str>) -> Option<f64> {
        let st = self.ix.get(Scope::Content, "table-column", style?)?;
        let w = kid(st, "style:table-column-properties")?
            .attr("style:column-width")
            .and_then(crate::units::parse_inches)?;
        // Excel's stored width (digit widths, to a 256th) from the length at 96 dpi.
        Some((w * 96.0 / self.mdw * 256.0).round() / 256.0)
    }

    fn row_height(&self, style: Option<&str>) -> Option<(f64, bool)> {
        let st = self.ix.get(Scope::Content, "table-row", style?)?;
        let rp = kid(st, "style:table-row-properties")?;
        let h = rp.attr("style:row-height").and_then(points_exact)?;
        let custom = rp.attr("style:use-optimal-row-height") == Some("false");
        Some((h, custom))
    }

    #[allow(clippy::too_many_lines)]
    fn read(
        &mut self,
        content: &'a XmlElement,
        settings: Option<&XmlElement>,
    ) -> Result<WorkbookSnapshot, OdfError> {
        let body = kid(content, "office:body")
            .and_then(|b| kid(b, "office:spreadsheet"))
            .ok_or_else(|| OdfError::Corrupt("content.xml has no office:spreadsheet".into()))?;
        let mut props = WorkbookProps::default();
        if let Some(cs) = kid(body, "table:calculation-settings") {
            if let Some(nd) = kid(cs, "table:null-date")
                && nd
                    .attr("table:date-value")
                    .is_some_and(|d| d.starts_with("1904-01-01"))
            {
                props.date1904 = true;
            }
            if let Some(it) = kid(cs, "table:iteration") {
                props.calc.iterate = it.attr("table:status") == Some("enable");
                if let Some(n) = parse_u32(it.attr("table:steps")) {
                    props.calc.iterate_count = n;
                }
                if let Some(d) = it
                    .attr("table:maximum-difference")
                    .and_then(|v| v.parse::<f64>().ok())
                {
                    props.calc.iterate_delta = d;
                }
            }
        }
        self.date1904 = props.date1904;
        let tables: Vec<&XmlElement> = kids(body, "table:table").collect();
        if tables.is_empty() {
            return Err(OdfError::Corrupt("the spreadsheet has no tables".into()));
        }
        let mut sheets = Vec::new();
        let mut names: Vec<DefinedName> = Vec::new();
        let mut used: Vec<String> = Vec::new();
        for (i, t) in tables.iter().enumerate() {
            let mut name = t
                .attr("table:name")
                .map(str::to_owned)
                .filter(|n| kiln_sheet::workbook::check_sheet_name(n).is_ok())
                .unwrap_or_else(|| format!("Sheet{}", i + 1));
            let mut n = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
                name = format!("{} ({n})", name.chars().take(26).collect::<String>());
                n += 1;
            }
            used.push(name.clone());
            #[allow(clippy::cast_possible_truncation)]
            let id = i as u32 + 1;
            let s = self.sheet(t, id, &name, &mut names);
            sheets.push(s);
        }
        // Named ranges and expressions (workbook scope and sheet-local ones).
        if let Some(ne) = kid(body, "table:named-expressions") {
            read_names(ne, None, &mut names);
        }
        for (t, s) in tables.iter().zip(&sheets) {
            if let Some(ne) = kid(t, "table:named-expressions") {
                read_names(ne, Some(SheetId(s.id)), &mut names);
            }
        }
        // AutoFilter ranges.
        if let Some(dbs) = kid(body, "table:database-ranges") {
            for db in kids(dbs, "table:database-range") {
                if db.attr("table:display-filter-buttons") != Some("true") {
                    continue;
                }
                let Some((Some(sheet), range)) = db
                    .attr("table:target-range-address")
                    .map(parse_range_list)
                    .and_then(|v| v.into_iter().next())
                else {
                    continue;
                };
                if let Some(s) = sheets.iter_mut().find(|s| s.name == sheet)
                    && s.meta.auto_filter.is_none()
                {
                    s.meta.auto_filter = Some(AutoFilter {
                        range,
                        columns: Vec::new(),
                        sort: None,
                    });
                }
            }
        }
        names.retain(|n| kiln_sheet::workbook::is_defined_name_valid(&n.name));
        let mut seen = std::collections::HashSet::new();
        names.retain(|n| seen.insert((n.name.to_uppercase(), n.scope)));
        // Views.
        read_views(settings, &mut sheets, &mut props);
        Ok(WorkbookSnapshot {
            sheets,
            names,
            styles: self.styles.clone(),
            theme: Theme::default(),
            props,
            protection: None,
            persons: Vec::new(),
            custom_lists: Vec::new(),
            passthrough: BTreeMap::new(),
            counters: (0, 1, self.rule_id + 1),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn sheet(
        &mut self,
        t: &'a XmlElement,
        id: u32,
        name: &str,
        names: &mut Vec<DefinedName>,
    ) -> SheetSnapshot {
        let mut meta = SheetMeta::default();
        let mut layout = SheetLayout::default();
        let mut cells: Vec<CellRecord> = Vec::new();
        if let Some(ts) = t
            .attr("table:style-name")
            .and_then(|s| self.ix.get(Scope::Content, "table", s))
        {
            if let Some(tp) = kid(ts, "style:table-properties") {
                if tp.attr("table:display") == Some("false") {
                    meta.visibility = SheetVisibility::Hidden;
                }
                if let Some(c) = tp
                    .attr("tableooo:tab-color")
                    .or_else(|| tp.attr("table:tab-color"))
                    .and_then(background)
                {
                    meta.tab_color = Some(c);
                }
                if tp.attr("style:writing-mode") == Some("rl-tb") {
                    meta.view.right_to_left = true;
                }
            }
            if let Some(mp) = ts.attr("style:master-page-name") {
                self.page_setup(mp, &mut meta);
            }
        }
        if t.attr("table:protected") == Some("true") {
            meta.protection = Some(SheetProtection::default());
        }
        if let Some(pr) = t.attr("table:print-ranges") {
            let parts: Vec<String> = parse_range_list(pr)
                .into_iter()
                .map(|(_, r)| format!("{}!{}", sheet_prefix(name), r.to_a1(true)))
                .collect();
            if !parts.is_empty() {
                names.push(DefinedName {
                    name: kiln_sheet::meta::builtin_names::PRINT_AREA.to_owned(),
                    formula: parts.join(","),
                    scope: Some(SheetId(id)),
                    hidden: false,
                    comment: None,
                });
            }
        }
        // Columns.
        let mut col_elems: Vec<(&XmlElement, u8, bool)> = Vec::new();
        collect_columns(t, 0, false, &mut col_elems);
        let mut title_cols: Option<(u32, u32)> = None;
        let mut col = 0u32;
        let mut col_styles: Vec<(u32, u32, u32)> = Vec::new();
        let mut col_runs: Vec<(u32, u32, Option<f64>, bool, bool, u8)> = Vec::new();
        for (c, level, header) in &col_elems {
            let n = repeat(c, "table:number-columns-repeated").min(MAX_COLS.saturating_sub(col));
            if n == 0 {
                break;
            }
            if *header {
                title_cols = Some(title_cols.map_or((col, col + n - 1), |(a, _)| (a, col + n - 1)));
            }
            let width = self.col_width(c.attr("table:style-name"));
            let best_fit = c
                .attr("table:style-name")
                .and_then(|s| self.ix.get(Scope::Content, "table-column", s))
                .and_then(|s| kid(s, "style:table-column-properties"))
                .is_some_and(|p| p.attr("style:use-optimal-column-width") == Some("true"));
            let hidden = matches!(c.attr("table:visibility"), Some("collapse" | "filter"));
            let xf = self.xf(c.attr("table:default-cell-style-name"));
            if xf != 0 {
                col_styles.push((col, col + n - 1, xf));
            }
            col_runs.push((col, n, width, hidden, best_fit, *level));
            col += n;
        }
        // The trailing filler column sets the default width.
        let standard = default_col_width_for(BASE_COL_WIDTH, self.mdw);
        let default_width = col_runs
            .last()
            .filter(|r| r.1 >= 256)
            .and_then(|r| r.2)
            .unwrap_or(standard);
        if (default_width - standard).abs() > 1e-9 {
            meta.default_col_width = Some(default_width);
        }
        let mut col_props: BTreeMap<u32, ColProps> = BTreeMap::new();
        for (start, n, width, hidden, best_fit, level) in &col_runs {
            let w = width.filter(|w| (w - default_width).abs() > 1e-9);
            let styled: Vec<&(u32, u32, u32)> = col_styles
                .iter()
                .filter(|(a, b, _)| *a < start + n && *b >= *start)
                .collect();
            if w.is_none() && !hidden && *level == 0 && !best_fit && styled.is_empty() {
                continue;
            }
            if *n > 16_384 {
                continue;
            }
            for c in *start..start + n {
                let p = ColProps {
                    width: w.map(F64),
                    custom_width: w.is_some(),
                    best_fit: *best_fit,
                    hidden: *hidden,
                    outline_level: *level,
                    style: col_styles
                        .iter()
                        .find(|(a, b, _)| *a <= c && c <= *b)
                        .map(|s| s.2),
                    ..ColProps::default()
                };
                if p != ColProps::default() {
                    col_props.insert(c, p);
                }
            }
        }
        let col_style_of = |c: u32| -> u32 {
            col_styles
                .iter()
                .find(|(a, b, _)| *a <= c && c <= *b)
                .map_or(0, |s| s.2)
        };
        // Rows.
        let mut row_elems: Vec<(&XmlElement, u8, bool)> = Vec::new();
        collect_rows(t, 0, false, &mut row_elems);
        // The trailing filler row sets the default height.
        let default_height = row_elems
            .last()
            .filter(|(r, _, _)| repeat(r, "table:number-rows-repeated") >= 256)
            .and_then(|(r, _, _)| self.row_height(r.attr("table:style-name")))
            .map_or(kiln_sheet::axis::DEFAULT_ROW_HEIGHT_PT, |(h, _)| h);
        meta.default_row_height = default_height;
        let mut row_props: BTreeMap<u32, RowProps> = BTreeMap::new();
        let mut row = 0u32;
        let mut validations: HashMap<String, usize> = HashMap::new();
        let mut dv_cells: Vec<Vec<(u32, u32)>> = Vec::new();
        let mut dv_list = Vec::new();
        let mut matrices: Vec<Range> = Vec::new();
        let mut title_rows: Option<(u32, u32)> = None;
        for (r_el, level, header) in &row_elems {
            if row >= MAX_ROWS {
                break;
            }
            let reps = repeat(r_el, "table:number-rows-repeated").min(MAX_ROWS - row);
            if *header {
                title_rows =
                    Some(title_rows.map_or((row, row + reps - 1), |(a, _)| (a, row + reps - 1)));
            }
            let height = self.row_height(r_el.attr("table:style-name"));
            let hidden = matches!(r_el.attr("table:visibility"), Some("collapse" | "filter"));
            let filtered = r_el.attr("table:visibility") == Some("filter");
            let row_xf = self.xf(r_el.attr("table:default-cell-style-name"));
            let mut rp = RowProps {
                hidden,
                filtered,
                outline_level: *level,
                ..RowProps::default()
            };
            if let Some((h, custom)) = height
                && (custom || (h - default_height).abs() > 1e-9)
            {
                rp.height = Some(F64(h));
                rp.custom_height = custom;
            }
            // Cells of one instance of the row.
            let mut row_cells: Vec<(u32, CellRecord, Option<String>)> = Vec::new();
            let mut whole_row_style: Option<u32> = (row_xf != 0).then_some(row_xf);
            let mut c = 0u32;
            for cell in r_el.elements() {
                if c >= MAX_COLS {
                    break;
                }
                let covered = cell.name == "table:covered-table-cell";
                if !covered && cell.name != "table:table-cell" {
                    continue;
                }
                let n = repeat(cell, "table:number-columns-repeated").min(MAX_COLS - c);
                let style_name = cell.attr("table:style-name");
                let xf = if style_name.is_some() {
                    self.xf(style_name)
                } else if row_xf != 0 {
                    row_xf
                } else {
                    col_style_of(c)
                };
                let text = paragraphs_text(cell);
                let xf = match self.whole_span(&text) {
                    Some(patch)
                        if cell.attr("office:value-type") == Some("string")
                            || cell.attr("office:value-type").is_none() =>
                    {
                        self.styles.patch(
                            xf,
                            &kiln_sheet::styles::StylePatch {
                                font: Some(patch),
                                ..kiln_sheet::styles::StylePatch::default()
                            },
                        )
                    }
                    _ => xf,
                };
                let formula = cell.attr("table:formula");
                let note = kid(cell, "office:annotation");
                let validation = cell.attr("table:content-validation-name");
                let mut value = self.value_of(cell, &text);
                if value.is_blank()
                    && formula.is_none()
                    && !text.runs.is_empty()
                    && cell.attr("office:value-type").is_none()
                {
                    // A cell with text but no value type: LibreOffice reads it as text.
                    value = self.text_value(&text);
                }
                let has_content = !value.is_blank() || formula.is_some() || note.is_some();
                let spans = (
                    repeat(cell, "table:number-columns-spanned"),
                    repeat(cell, "table:number-rows-spanned"),
                );
                if !has_content && validation.is_none() && spans == (1, 1) {
                    // Formatting only.
                    // The default named where the row or column has a format of its own: the
                    // cell holds the default's twin (a blank cell with the default is no cell).
                    // Under a formatted row every cell of the run does; otherwise the cells in
                    // formatted columns.
                    if xf == 0 && style_name.is_some() {
                        let twin_cols: Vec<u32> = if row_xf != 0 {
                            (c..c + n).collect()
                        } else {
                            (c..c + n).filter(|&k| col_style_of(k) != 0).collect()
                        };
                        if !twin_cols.is_empty()
                            && u64::try_from(twin_cols.len())
                                .unwrap_or(u64::MAX)
                                .saturating_mul(u64::from(reps))
                                <= MAX_STYLED_EXPANSION
                        {
                            let t = self.styles.default_twin();
                            for k in twin_cols {
                                row_cells.push((
                                    k,
                                    CellRecord {
                                        row,
                                        col: k,
                                        style: t,
                                        ..CellRecord::default()
                                    },
                                    None,
                                ));
                            }
                        }
                        c += n;
                        continue;
                    }
                    let default_here = xf == col_style_of(c) || xf == 0;
                    if !default_here {
                        let product = u64::from(n) * u64::from(reps);
                        if product > MAX_STYLED_EXPANSION {
                            if c + n >= MAX_COLS - 1 && c == 0 {
                                whole_row_style = Some(xf);
                            } else if c + n >= MAX_COLS - 1 {
                                // The rest of the row: kept as the row's style only when the
                                // whole row has it.
                            }
                        } else {
                            for k in 0..n {
                                row_cells.push((
                                    c + k,
                                    CellRecord {
                                        row,
                                        col: c + k,
                                        style: xf,
                                        ..CellRecord::default()
                                    },
                                    None,
                                ));
                            }
                        }
                    }
                    c += n;
                    continue;
                }
                let mut copies = n;
                for k in 0..n {
                    if k > 0 && !self.allow_repeat(formula.is_some()) {
                        copies = k;
                        break;
                    }
                    let at = c + k;
                    let mut rec = CellRecord {
                        row,
                        col: at,
                        value: value.clone(),
                        formula: None,
                        legacy: true,
                        style: xf,
                    };
                    if let Some(f) = formula {
                        let text = odf_to_excel(f);
                        if !text.is_empty() {
                            rec.formula = Some(text);
                            let mc = parse_u32(cell.attr("table:number-matrix-columns-spanned"));
                            let mr = parse_u32(cell.attr("table:number-matrix-rows-spanned"));
                            if mc.is_some() || mr.is_some() {
                                rec.legacy = false;
                                let (mc, mr) = (mc.unwrap_or(1).max(1), mr.unwrap_or(1).max(1));
                                matrices.push(Range::new(
                                    row,
                                    at,
                                    (row + mr - 1).min(MAX_ROWS - 1),
                                    (at + mc - 1).min(MAX_COLS - 1),
                                ));
                            }
                        }
                    }
                    row_cells.push((at, rec, validation.map(str::to_owned)));
                }
                if let Some(href) = &text.link {
                    let (target, location) = match href.strip_prefix('#') {
                        Some(loc) => (None, Some(crate::formula::range_address_to_excel(loc))),
                        None => (Some(href.clone()), None),
                    };
                    for k in 0..copies {
                        meta.hyperlinks.push(Hyperlink {
                            range: Range::cell(row, c + k),
                            target: target.clone(),
                            location: location.clone(),
                            display: None,
                            tooltip: None,
                        });
                    }
                }
                if let Some(a) = note {
                    let author = kid(a, "dc:creator").map(all_text).unwrap_or_default();
                    let text: Vec<String> = kids(a, "text:p").map(paragraphs_text_of).collect();
                    for k in 0..copies {
                        meta.notes.push(Note {
                            at: CellPos::new(row, c + k),
                            author: author.clone(),
                            text: text.join("\n"),
                            visible: a.attr("office:display") == Some("true"),
                            size: None,
                        });
                    }
                }
                if spans.0 > 1 || spans.1 > 1 {
                    let r1 = (row + spans.1 - 1).min(MAX_ROWS - 1);
                    let c1 = (c + spans.0 - 1).min(MAX_COLS - 1);
                    meta.merges.push(Range::new(row, c, r1, c1));
                }
                c += n;
            }
            if let Some(xf) = whole_row_style {
                rp.style = Some(xf);
            }
            let has_props = rp != RowProps::default();
            let row_count = if row_cells.is_empty() && !has_props {
                reps
            } else {
                reps.min(MAX_ROWS - row)
            };
            if !row_cells.is_empty() || has_props {
                let mut out_of_budget = false;
                for k in 0..row_count {
                    let at = row + k;
                    if has_props {
                        row_props.insert(at, rp);
                    }
                    if out_of_budget {
                        if has_props {
                            continue;
                        }
                        break;
                    }
                    for (col, rec, dv) in &row_cells {
                        if self.cells_read >= MAX_CELLS {
                            break;
                        }
                        if k > 0 && rec.has_content() && !self.allow_repeat(rec.formula.is_some()) {
                            out_of_budget = true;
                            break;
                        }
                        let mut rec = rec.clone();
                        rec.row = at;
                        if let Some(name) = dv {
                            let slot = match validations.get(name) {
                                Some(s) => *s,
                                None => {
                                    let s = dv_cells.len();
                                    validations.insert(name.clone(), s);
                                    dv_cells.push(Vec::new());
                                    dv_list.push(name.clone());
                                    s
                                }
                            };
                            dv_cells[slot].push((at, *col));
                        }
                        if rec.has_content() || rec.style != 0 {
                            cells.push(rec);
                            self.cells_read += 1;
                        }
                    }
                }
                if self.cells_read >= MAX_CELLS {
                    self.warn(
                        "the spreadsheet has more cells than Kiln reads; the rest were skipped",
                    );
                }
            }
            row += reps;
        }
        self.frames(t, &row_elems, &mut meta);
        // Header rows and columns are the print titles.
        let mut titles = Vec::new();
        let prefix = sheet_prefix(name);
        if let Some((a, b)) = title_cols {
            titles.push(format!(
                "{prefix}!${}:${}",
                kiln_formula::refs::column_to_letters(a),
                kiln_formula::refs::column_to_letters(b)
            ));
        }
        if let Some((a, b)) = title_rows {
            titles.push(format!("{prefix}!${}:${}", a + 1, b + 1));
        }
        if !titles.is_empty() {
            names.push(DefinedName {
                name: kiln_sheet::meta::builtin_names::PRINT_TITLES.to_owned(),
                formula: titles.join(","),
                scope: Some(SheetId(id)),
                hidden: false,
                comment: None,
            });
        }
        // Cells inside a matrix formula's range other than its anchor hold results the engine
        // recomputes: they keep their format only.
        if !matrices.is_empty() {
            for c in &mut cells {
                if c.formula.is_none()
                    && matrices
                        .iter()
                        .any(|m| m.contains(c.row, c.col) && (m.r0, m.c0) != (c.row, c.col))
                {
                    c.value = CellValue::Blank;
                }
            }
            cells.retain(|c| c.has_content() || c.style != 0);
        }
        cells.sort_by_key(|c| (c.row, c.col));
        cells.dedup_by_key(|c| (c.row, c.col));
        // Validations.
        if let Some(cv) = self.content_validations {
            for (slot, vname) in dv_list.iter().enumerate() {
                let Some(el) = kids(cv, "table:content-validation")
                    .find(|e| e.attr("table:name") == Some(vname.as_str()))
                else {
                    continue;
                };
                self.rule_id += 1;
                let mut dv = read_validation(el, self.rule_id);
                dv.ranges = cells_to_ranges(&dv_cells[slot]);
                meta.validations.push(dv);
            }
        }
        // Conditional formats.
        if let Some(cfs) = kid(t, "calcext:conditional-formats") {
            let mut priority = 0;
            for cf in kids(cfs, "calcext:conditional-format") {
                let ranges: Vec<Range> = cf
                    .attr("calcext:target-range-address")
                    .map(parse_range_list)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(_, r)| r)
                    .collect();
                let mut rule_id = self.rule_id;
                let mut dxf = |n: &str| self.dxf(n);
                if let Some(f) = read_cf(cf, ranges, &mut dxf, &mut rule_id, &mut priority) {
                    meta.cf.push(f);
                }
                self.rule_id = rule_id;
            }
        }
        layout.rows = row_props.into_iter().collect();
        layout.cols = col_props.into_iter().collect();
        SheetSnapshot {
            id,
            name: name.to_owned(),
            cells,
            layout,
            meta,
        }
    }

    fn page_setup(&self, master: &str, meta: &mut SheetMeta) {
        let Some(mp) = self.ix.master_page(master) else {
            return;
        };
        meta.print.header_footer = super::headers::read(mp);
        let Some(pl) = mp
            .attr("style:page-layout-name")
            .and_then(|n| self.ix.page_layouts.get(n))
            .and_then(|l| kid(l, "style:page-layout-properties"))
        else {
            return;
        };
        let inches = |a: &str| pl.attr(a).and_then(crate::units::parse_inches);
        if pl.attr("style:print-orientation") == Some("landscape") {
            meta.print.orientation = Orientation::Landscape;
        }
        if let (Some(w), Some(h)) = (inches("fo:page-width"), inches("fo:page-height")) {
            let (w, h) = if w > h { (h, w) } else { (w, h) };
            meta.print.paper_size = paper_code(w, h);
        }
        let round = |v: f64| (v * 1e6).round() / 1e6;
        if let Some(v) = inches("fo:margin-left") {
            meta.print.margins.left = round(v);
        }
        if let Some(v) = inches("fo:margin-right") {
            meta.print.margins.right = round(v);
        }
        if let Some(v) = inches("fo:margin-top") {
            meta.print.margins.top = round(v);
        }
        if let Some(v) = inches("fo:margin-bottom") {
            meta.print.margins.bottom = round(v);
        }
        if let Some(s) = parse_u32(pl.attr("style:scale-to").map(|v| v.trim_end_matches('%'))) {
            meta.print.scale = u16::try_from(s.clamp(10, 400)).unwrap_or(100);
        }
        if let Some(p) = pl.attr("style:print") {
            meta.print.print_gridlines = p.contains("grid");
            meta.print.print_headings = p.contains("headers");
        }
        meta.print.horizontal_centered = pl
            .attr("style:table-centering")
            .is_some_and(|c| c == "horizontal" || c == "both");
        meta.print.vertical_centered = pl
            .attr("style:table-centering")
            .is_some_and(|c| c == "vertical" || c == "both");
    }
}

/// Paper sizes by their portrait size in inches (Excel's `paperSize` codes).
const PAPERS: &[(u32, f64, f64)] = &[
    (1, 8.5, 11.0),
    (5, 8.5, 14.0),
    (8, 11.693, 16.535),
    (9, 8.268, 11.693),
    (11, 5.827, 8.268),
    (3, 11.0, 17.0),
];

fn paper_code(w: f64, h: f64) -> u32 {
    PAPERS
        .iter()
        .find(|(_, pw, ph)| (pw - w).abs() < 0.05 && (ph - h).abs() < 0.05)
        .map_or(9, |p| p.0)
}

/// The inches of a paper size code (portrait).
#[must_use]
pub fn paper_size(code: u32) -> (f64, f64) {
    PAPERS
        .iter()
        .find(|p| p.0 == code)
        .map_or((8.268, 11.693), |p| (p.1, p.2))
}

fn paragraphs_text_of(p: &XmlElement) -> String {
    let mut out = CellText::default();
    let mut space = true;
    collect_text(p, None, &mut out, &mut space);
    out.plain().trim_end().to_owned()
}

pub(crate) fn sheet_prefix(name: &str) -> String {
    if kiln_formula::refs::sheet_needs_quotes(name) {
        kiln_formula::refs::quote_sheet(name)
    } else {
        name.to_owned()
    }
}

fn collect_columns<'a>(
    el: &'a XmlElement,
    level: u8,
    header: bool,
    out: &mut Vec<(&'a XmlElement, u8, bool)>,
) {
    for c in el.elements() {
        match c.name.as_str() {
            "table:table-column" => out.push((c, level, header)),
            "table:table-column-group" => {
                collect_columns(c, level.saturating_add(1).min(7), header, out);
            }
            "table:table-columns" => collect_columns(c, level, header, out),
            "table:table-header-columns" => collect_columns(c, level, true, out),
            _ => {}
        }
    }
}

fn collect_rows<'a>(
    el: &'a XmlElement,
    level: u8,
    header: bool,
    out: &mut Vec<(&'a XmlElement, u8, bool)>,
) {
    for c in el.elements() {
        match c.name.as_str() {
            "table:table-row" => out.push((c, level, header)),
            "table:table-row-group" => collect_rows(c, level.saturating_add(1).min(7), header, out),
            "table:table-rows" => collect_rows(c, level, header, out),
            "table:table-header-rows" => collect_rows(c, level, true, out),
            _ => {}
        }
    }
}

/// Cells to a small list of rectangles: runs per row, then runs of equal runs down columns.
fn cells_to_ranges(cells: &[(u32, u32)]) -> Vec<Range> {
    let mut sorted = cells.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for (r, c) in sorted {
        match runs.last_mut() {
            Some((rr, _, c1)) if *rr == r && *c1 + 1 == c => *c1 = c,
            _ => runs.push((r, c, c)),
        }
    }
    let mut out: Vec<Range> = Vec::new();
    for (r, c0, c1) in runs {
        if let Some(rg) = out
            .iter_mut()
            .find(|g| g.c0 == c0 && g.c1 == c1 && g.r1 + 1 == r)
        {
            rg.r1 = r;
        } else {
            out.push(Range::new(r, c0, r, c1));
        }
    }
    out
}

fn read_names(ne: &XmlElement, scope: Option<SheetId>, names: &mut Vec<DefinedName>) {
    for e in ne.elements() {
        let Some(name) = e.attr("table:name") else {
            continue;
        };
        let formula = match e.name.as_str() {
            "table:named-range" => match e.attr("table:cell-range-address") {
                Some(a) => crate::formula::range_address_to_excel(a),
                None => continue,
            },
            "table:named-expression" => match e.attr("table:expression") {
                Some(x) => odf_to_excel(x),
                None => continue,
            },
            _ => continue,
        };
        // LibreOffice's names of Excel's built-ins.
        let name = match name {
            "Excel_BuiltIn_Print_Area" => kiln_sheet::meta::builtin_names::PRINT_AREA.to_owned(),
            "Excel_BuiltIn__FilterDatabase" => {
                kiln_sheet::meta::builtin_names::FILTER_DATABASE.to_owned()
            }
            "Excel_BuiltIn_Print_Titles" => {
                kiln_sheet::meta::builtin_names::PRINT_TITLES.to_owned()
            }
            n => n.to_owned(),
        };
        names.push(DefinedName {
            name,
            formula,
            scope,
            hidden: false,
            comment: None,
        });
    }
}

fn config_items(set: &XmlElement) -> HashMap<&str, String> {
    set.elements()
        .filter(|e| e.name == "config:config-item")
        .filter_map(|e| Some((e.attr("config:name")?, all_text(e))))
        .collect()
}

fn read_views(
    settings: Option<&XmlElement>,
    sheets: &mut [SheetSnapshot],
    props: &mut WorkbookProps,
) {
    let Some(root) = settings else {
        return;
    };
    let Some(views) = root
        .descendants("config:config-item-map-indexed")
        .into_iter()
        .find(|e| e.attr("config:name") == Some("Views"))
    else {
        return;
    };
    let Some(view) = kid(views, "config:config-item-map-entry") else {
        return;
    };
    let top = config_items(view);
    if let Some(active) = top.get("ActiveTable")
        && let Some(i) = sheets.iter().position(|s| s.name == *active)
    {
        props.active_sheet = i;
    }
    let grid = top.get("ShowGrid").map(|v| v == "true");
    let Some(tables) = view.elements().find(|e| {
        e.name == "config:config-item-map-named" && e.attr("config:name") == Some("Tables")
    }) else {
        return;
    };
    for entry in kids(tables, "config:config-item-map-entry") {
        let Some(name) = entry.attr("config:name") else {
            continue;
        };
        let Some(s) = sheets.iter_mut().find(|s| s.name == name) else {
            continue;
        };
        let items = config_items(entry);
        let num = |k: &str| items.get(k).and_then(|v| v.trim().parse::<u32>().ok());
        let v = &mut s.meta.view;
        if let Some(z) = num("ZoomValue") {
            v.zoom = u16::try_from(z.clamp(10, 400)).unwrap_or(100);
        }
        if let Some(g) = items.get("ShowGrid").map(|v| v == "true").or(grid) {
            v.show_gridlines = g;
        }
        let (hm, vm) = (num("HorizontalSplitMode"), num("VerticalSplitMode"));
        let cols = if hm == Some(2) {
            num("HorizontalSplitPosition").unwrap_or(0)
        } else {
            0
        };
        let rows = if vm == Some(2) {
            num("VerticalSplitPosition").unwrap_or(0)
        } else {
            0
        };
        if rows > 0 || cols > 0 {
            v.freeze = Some(Freeze { rows, cols });
        }
        if let (Some(x), Some(y)) = (num("CursorPositionX"), num("CursorPositionY"))
            && x < MAX_COLS
            && y < MAX_ROWS
        {
            v.selection.active = CellPos::new(y, x);
            v.selection.ranges = vec![Range::cell(y, x)];
        }
        let left = if cols > 0 {
            num("PositionRight")
        } else {
            num("PositionLeft")
        };
        let top_row = if rows > 0 {
            num("PositionBottom")
        } else {
            num("PositionTop")
        };
        if let (Some(l), Some(t)) = (left, top_row)
            && (l, t) != (0, 0)
            && l < MAX_COLS
            && t < MAX_ROWS
        {
            v.top_left = CellPos::new(t, l);
        }
    }
}

/// Reads an opened package.
///
/// # Errors
///
/// [`OdfError`] when the package is not a readable ODF spreadsheet.
pub fn read_package(pkg: &Package) -> Result<OdsRead, OdfError> {
    if pkg.kind != Some(OdfKind::Spreadsheet) {
        return Err(OdfError::NotOdf(format!(
            "{} is not a spreadsheet",
            pkg.media_type
        )));
    }
    let content = parse_part(
        "content.xml",
        pkg.file("content.xml")
            .ok_or_else(|| OdfError::Corrupt("content.xml is missing".into()))?,
    )?;
    let styles = match pkg.file("styles.xml") {
        Some(b) => Some(parse_part("styles.xml", b)?),
        None => None,
    };
    let settings = pkg
        .file("settings.xml")
        .and_then(|b| parse_part("settings.xml", b).ok());
    let ix = StyleIndex::new(styles.as_ref(), Some(&content));
    let mut fonts = FontFaces::default();
    for root in [styles.as_ref(), Some(&content)].into_iter().flatten() {
        fonts.add_decls(root);
    }
    // The default cell style: the `table-cell` default style, then `Default`.
    let mut base = CellStyle::default();
    base.font.scheme = kiln_sheet::styles::FontScheme::None;
    base.font.family = None;
    base.font.color = None;
    let mut chain: Vec<&XmlElement> = Vec::new();
    if let Some(d) = ix.default_style("table-cell") {
        chain.push(d);
    }
    if let Some(d) = ix.get(Scope::Styles, "table-cell", "Default") {
        chain.push(d);
    }
    base = resolve_chain(&base, &chain, &fonts);
    let mdw = DefaultFontMetrics.max_digit_width(&base.font.name, base.font.size.0);
    let table = StyleTable::new(base.font.clone());
    let xf0 = table.get(0);
    base = CellStyle {
        num_fmt: xf0.num_fmt.clone(),
        style_id: xf0.style_id,
        ..base
    };
    let content_validations = kid(&content, "office:body")
        .and_then(|b| kid(b, "office:spreadsheet"))
        .and_then(|s| kid(s, "table:content-validations"));
    let mut r = Reader {
        pkg,
        ix,
        fonts,
        styles: table,
        base,
        xf_cache: HashMap::new(),
        codes: HashMap::new(),
        dxf_cache: HashMap::new(),
        warnings: Vec::new(),
        date1904: false,
        mdw,
        rule_id: 0,
        cells_read: 0,
        repeated_values: 0,
        repeated_formulas: 0,
        content_validations,
        charts: Vec::new(),
        pictures: Vec::new(),
    };
    let mut snapshot = r.read(&content, settings.as_ref())?;
    let mut charts = std::mem::take(&mut r.charts);
    let pictures = std::mem::take(&mut r.pictures);
    snapshot.styles = r.styles.clone();
    let warnings = std::mem::take(&mut r.warnings);
    let mut props = snapshot.props.clone();
    snapshot.props.calc.mode = CalcMode::Manual;
    let mut pending = Vec::new();
    for s in &mut snapshot.sheets {
        for c in &mut s.cells {
            if let Some(f) = c.formula.take() {
                pending.push((SheetId(s.id), c.row, c.col, f, c.legacy));
            }
        }
    }
    let mut wb = Workbook::from_snapshot(snapshot, WorkbookOptions::default())
        .map_err(|e| OdfError::Model(e.to_string()))?;
    for (s, row, col, text, legacy) in pending {
        let cached = wb.value(s, row, col);
        wb.load_formula(s, row, col, &text, cached, legacy)
            .map_err(|e| OdfError::Model(e.to_string()))?;
    }
    props.calc.mode = CalcMode::Auto;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .map_err(|e| OdfError::Model(e.to_string()))?;
    wb.take_changes();
    for (part, bytes) in pictures {
        wb.passthrough_mut().insert(part, bytes);
    }
    // Chart caches from the cells.
    for (_, cs) in &mut charts {
        kiln_charts::data::refresh(cs, &wb);
    }
    Ok(OdsRead {
        charts,
        workbook: wb,
        properties: meta::read_meta(pkg.file("meta.xml"))?,
        warnings,
        template: pkg.media_type.ends_with("-template"),
    })
}
