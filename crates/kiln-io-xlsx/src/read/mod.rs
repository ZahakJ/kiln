// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`read_xlsx`]: a SpreadsheetML package → [`kiln_sheet::Workbook`] plus the [`XlsxSource`]
//! the writer needs for a faithful save.
//!
//! The workbook is assembled as a [`WorkbookSnapshot`] and built in one step
//! ([`Workbook::from_snapshot`]), with calculation held back so reading and calculating can be
//! measured apart; [`XlsxRead::calculate`] (done by [`read_xlsx`]) restores the file's calculation
//! settings, which recalculates every formula once.

mod cells;
mod scan;
mod stream;

use std::collections::BTreeSet;
use std::sync::Arc;

use kiln_formula::engine::Clock;
use kiln_ooxml::opc::{OpcPackage, rel_type};
use kiln_ooxml::xml::{ParseOptions, XmlElement, decode_xml_bytes, parse_str_with};
use kiln_sheet::meta::{
    CalcMode, ConditionalFormat, DefinedName, Hyperlink, Note, Person, SheetVisibility,
    WorkbookProps, WorkbookProtection,
};
use kiln_sheet::serial::{SheetSnapshot, WorkbookSnapshot};
use kiln_sheet::styles::StyleTable;
use kiln_sheet::workbook::{CellRecord, check_sheet_name, is_defined_name_valid};
use kiln_sheet::{
    CellValue, Command, SheetId, SheetLayout, SheetMeta, Theme, Workbook, WorkbookOptions,
};
use quick_xml::events::Event;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::parts::{
    REL_PERSON, REL_THREADED, dynamic_cell_metadata, parse_comments, parse_drawing, parse_persons,
    parse_table, parse_threaded, parse_vml_notes, read_theme,
};
use crate::sheet_xml::{
    apply_x14_rule, parse_auto_filter, parse_cf_rule, parse_cols, parse_data_validation,
    parse_password, parse_print, parse_ranges, parse_ref, parse_sheet_protection, parse_sheet_view,
    parse_x14_only_rule,
};
use crate::source::{DrawingSource, NotesSource, SheetSource, XlsxKind, XlsxSource};
use crate::styles::{read_styles, settle};
use crate::xmlx::{abool_or, anum, au32, child_local, children_local, parse_color};
use crate::{XlsxError, strings::parse_string_item};

/// How to read.
#[derive(Clone, Default)]
pub struct ReadOptions {
    /// Leave the formulas uncalculated (the workbook stays in manual calculation until
    /// [`XlsxRead::calculate`]); for measuring reading and calculation separately.
    pub defer_calculation: bool,
    /// The clock for `NOW()`/`TODAY()` (`None` = the system clock).
    pub clock: Option<Clock>,
    /// Calculate on the rayon pool (`None` = the crate default).
    pub parallel: Option<bool>,
}

impl std::fmt::Debug for ReadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadOptions")
            .field("defer_calculation", &self.defer_calculation)
            .field("parallel", &self.parallel)
            .finish_non_exhaustive()
    }
}

/// A workbook as read.
#[derive(Debug)]
pub struct XlsxRead {
    /// The workbook.
    pub workbook: Workbook,
    /// What the writer needs for a faithful save.
    pub source: XlsxSource,
    /// What could not be read exactly (damaged references, unknown values).
    pub warnings: Vec<String>,
    /// Formulas to bind when calculating: (sheet, row, column, text, legacy).
    pending: Vec<(SheetId, u32, u32, String, bool)>,
    calculated: bool,
}

impl XlsxRead {
    /// Binds the formulas to the engine and restores the file's calculation settings, which
    /// recalculates every formula (once). Done by [`read_xlsx`]; needed only after
    /// [`ReadOptions::defer_calculation`], until which formula cells hold their stored results.
    ///
    /// # Errors
    ///
    /// When the model refuses the file's calculation settings (they are then left as they are).
    pub fn calculate(&mut self) -> Result<(), XlsxError> {
        if self.calculated {
            return Ok(());
        }
        self.calculated = true;
        for (s, r, c, text, legacy) in std::mem::take(&mut self.pending) {
            let cached = self.workbook.value(s, r, c);
            self.workbook.load_formula(s, r, c, &text, cached, legacy)?;
        }
        let props = self.source.props.clone();
        if props.calc.mode == CalcMode::Manual {
            // Excel shows the stored results of a manual workbook; recalculating once gives the
            // engine's results the same place (values the engine cannot compute keep theirs).
            self.workbook.recalc_all();
        }
        self.workbook.apply(&Command::SetProps {
            props: Box::new(props),
        })?;
        self.workbook.take_changes();
        Ok(())
    }

    /// Whether [`XlsxRead::calculate`] has run.
    #[must_use]
    pub fn is_calculated(&self) -> bool {
        self.calculated
    }
}

/// Reads a workbook and calculates it.
///
/// # Errors
///
/// [`XlsxError`] when the package is not a readable workbook.
pub fn read_xlsx(bytes: &[u8]) -> Result<XlsxRead, XlsxError> {
    read_xlsx_with(bytes, &ReadOptions::default())
}

/// Reads a workbook with options.
///
/// # Errors
///
/// [`XlsxError`] when the package is not a readable workbook.
pub fn read_xlsx_with(bytes: &[u8], options: &ReadOptions) -> Result<XlsxRead, XlsxError> {
    let package = OpcPackage::open(bytes)?;
    let mut r = Reader {
        pkg: &package,
        warnings: Vec::new(),
        future: BTreeSet::new(),
    };
    let (snapshot, source) = r.read()?;
    let warnings = std::mem::take(&mut r.warnings);
    let mut future = std::mem::take(&mut r.future);
    let mut source = source;
    drop(package);
    // A fresh package for the writer: the one read holds every inflated part in its cache.
    source.package = OpcPackage::open(bytes)?;
    future.retain(|f| !crate::formula_text::is_future_function(f));
    source.future_functions = future;
    let mut snapshot = snapshot;
    snapshot.props.calc.mode = CalcMode::Manual;
    // Formulas are bound when calculating: until then the cells show their stored results, so
    // a workbook can be shown before its formulas are parsed.
    let mut pending = Vec::new();
    for s in &mut snapshot.sheets {
        for c in &mut s.cells {
            if let Some(f) = c.formula.take() {
                pending.push((SheetId(s.id), c.row, c.col, f, c.legacy));
            }
        }
    }
    let workbook = Workbook::from_snapshot(
        snapshot,
        WorkbookOptions {
            clock: options.clock.clone(),
            parallel: options.parallel,
            ..WorkbookOptions::default()
        },
    )?;
    let mut read = XlsxRead {
        workbook,
        source,
        warnings,
        pending,
        calculated: false,
    };
    if let Some(table) = read.source.styles.as_mut() {
        settle(table, read.workbook.styles());
    }
    if !options.defer_calculation {
        read.calculate()?;
    }
    Ok(read)
}

struct Reader<'a> {
    pkg: &'a OpcPackage,
    warnings: Vec<String>,
    future: BTreeSet<String>,
}

const PERSON_REL: &str = REL_PERSON;

fn decode(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    decode_xml_bytes(bytes)
}

fn parse_part_text(part: &str, text: &str) -> Result<XmlElement, XlsxError> {
    parse_str_with(
        text,
        ParseOptions {
            keep_too_deep_as_raw: true,
            ..ParseOptions::default()
        },
    )
    .map(|d| d.root)
    .map_err(|e| XlsxError::Xml {
        part: part.to_owned(),
        message: e.0,
    })
}

impl Reader<'_> {
    fn warn(&mut self, msg: impl Into<String>) {
        if self.warnings.len() < 1000 {
            self.warnings.push(msg.into());
        }
    }

    fn xml(&self, part: &str) -> Result<Option<XmlElement>, XlsxError> {
        match self.pkg.part(part)? {
            Some(b) => parse_part_text(part, &decode(&b)).map(Some),
            None => Ok(None),
        }
    }

    /// The target part of a relationship of `source` by type.
    fn related(&self, source: &str, ty: &str) -> Result<Option<String>, XlsxError> {
        let rels = self.pkg.relationships(source)?;
        Ok(rels
            .first_by_type(ty)
            .and_then(|r| rels.resolve(r))
            .filter(|p| self.pkg.has(p)))
    }

    #[allow(clippy::too_many_lines)]
    fn read(&mut self) -> Result<(WorkbookSnapshot, XlsxSource), XlsxError> {
        let pkg_rels = self.pkg.relationships("/")?;
        let main_rel = pkg_rels
            .first_by_type(rel_type::OFFICE_DOCUMENT)
            .cloned()
            .ok_or_else(|| XlsxError::NotWorkbook("the package has no main part".into()))?;
        let strict = main_rel.rel_type.starts_with("http://purl.oclc.org/");
        let workbook_part = pkg_rels
            .resolve(&main_rel)
            .filter(|p| self.pkg.has(p))
            .ok_or_else(|| XlsxError::NotWorkbook("the main part is missing".into()))?;
        drop(pkg_rels);
        let ct = self.pkg.content_type(&workbook_part).unwrap_or("");
        let kind = XlsxKind::from_content_type(ct).ok_or_else(|| {
            XlsxError::NotWorkbook(format!("the main part has the content type \"{ct}\""))
        })?;
        let wb_root = self
            .xml(&workbook_part)?
            .ok_or_else(|| XlsxError::NotWorkbook("the main part is missing".into()))?;
        if wb_root.name != "workbook" {
            return Err(XlsxError::NotWorkbook(format!(
                "the main part is <{}>",
                wb_root.name
            )));
        }

        // Theme, styles, shared strings, metadata, people.
        let theme_part = self.related(&workbook_part, rel_type::THEME)?;
        let theme: Option<Theme> = match &theme_part {
            Some(p) => {
                let b = self.pkg.part_bytes(p)?;
                let t = read_theme(&b).map(|t| t.0);
                if t.is_none() {
                    self.warn(format!("{p}: the theme could not be read"));
                }
                t
            }
            None => None,
        };
        let styles_part = self.related(&workbook_part, rel_type::STYLES)?;
        let (style_parts, styles_src) = match &styles_part {
            Some(p) => match self.xml(p)? {
                Some(root) => {
                    let mut w = Vec::new();
                    let (parts, src) = read_styles(&root, &mut w);
                    for m in w {
                        self.warn(format!("{p}: {m}"));
                    }
                    (Some(parts), Some(src))
                }
                None => (None, None),
            },
            None => (None, None),
        };
        let styles = match style_parts.map(StyleTable::from_parts) {
            Some(Ok(t)) => t,
            Some(Err(e)) => {
                self.warn(format!("styles: {e}; the default styles are used"));
                StyleTable::default()
            }
            None => StyleTable::default(),
        };
        let xf_count = styles.len() as u32;
        let sst_part = self.related(&workbook_part, rel_type::SHARED_STRINGS)?;
        let sst = match &sst_part {
            Some(p) => self.read_sst(p)?,
            None => Vec::new(),
        };
        let metadata_part = self.related(&workbook_part, rel_type::SHEET_METADATA)?;
        let dynamic_cm = match &metadata_part {
            Some(p) => self
                .xml(p)?
                .map(|r| dynamic_cell_metadata(&r))
                .unwrap_or_default(),
            None => FxHashSet::default(),
        };
        let persons_part = self.related(&workbook_part, PERSON_REL)?;
        let persons: Vec<Person> = match &persons_part {
            Some(p) => self.xml(p)?.map(|r| parse_persons(&r)).unwrap_or_default(),
            None => Vec::new(),
        };

        // Workbook properties.
        let mut props = WorkbookProps::default();
        if let Some(p) = wb_root.child("workbookPr") {
            props.date1904 = abool_or(p, "date1904", false);
            props.code_name = p.attr("codeName").map(str::to_owned);
            props.filter_privacy = abool_or(p, "filterPrivacy", false);
        }
        let calc = wb_root.child("calcPr");
        if let Some(c) = calc {
            props.calc.mode = match c.attr("calcMode") {
                Some("manual") => CalcMode::Manual,
                Some("autoNoTable") => CalcMode::AutoNoTable,
                _ => CalcMode::Auto,
            };
            props.calc.full_calc_on_load = abool_or(c, "fullCalcOnLoad", false);
            props.calc.calc_on_save = abool_or(c, "calcOnSave", true);
            props.calc.iterate = abool_or(c, "iterate", false);
            props.calc.iterate_count = au32(c, "iterateCount").unwrap_or(100).clamp(1, 32_767);
            props.calc.iterate_delta = anum(c, "iterateDelta")
                .filter(|d| *d >= 0.0)
                .unwrap_or(0.001);
            props.calc.full_precision = abool_or(c, "fullPrecision", true);
        }
        let protection = wb_root
            .child("workbookProtection")
            .map(|p| WorkbookProtection {
                lock_structure: abool_or(p, "lockStructure", false),
                lock_windows: abool_or(p, "lockWindows", false),
                password: parse_password(p, "workbookPassword", "workbook"),
            });
        let view = wb_root
            .child("bookViews")
            .and_then(|b| b.child("workbookView"));
        let active_all = view.and_then(|v| au32(v, "activeTab")).unwrap_or(0) as usize;
        let first_all = view.and_then(|v| au32(v, "firstSheet")).unwrap_or(0) as usize;
        props.tab_ratio = view.and_then(|v| au32(v, "tabRatio"));

        // Sheets.
        let wb_rels = self.pkg.relationships(&workbook_part)?.into_owned();
        let mut sheets: Vec<SheetSnapshot> = Vec::new();
        let mut sources: Vec<SheetSource> = Vec::new();
        let mut other_sheets: Vec<(usize, XmlElement)> = Vec::new();
        let mut index_of_all: Vec<Option<SheetId>> = Vec::new();
        let mut used_ids: FxHashSet<u32> = FxHashSet::default();
        let mut used_names: FxHashSet<String> = FxHashSet::default();
        let mut next_rule = 1u32;
        let mut table_ids: FxHashSet<u32> = FxHashSet::default();
        let mut max_table = 0u32;
        let sheet_els: Vec<XmlElement> = wb_root
            .child("sheets")
            .map(|s| s.children_named("sheet").cloned().collect())
            .unwrap_or_default();
        for (pos, sel) in sheet_els.into_iter().enumerate() {
            let part = sel
                .attr("r:id")
                .and_then(|id| wb_rels.get(id))
                .and_then(|r| wb_rels.resolve(r))
                .filter(|p| self.pkg.has(p));
            let is_worksheet = part.as_deref().is_some_and(|p| {
                self.pkg
                    .content_type(p)
                    .is_some_and(|ct| ct.ends_with("worksheet+xml"))
            });
            let Some(part) = part.filter(|_| is_worksheet) else {
                other_sheets.push((pos, sel));
                index_of_all.push(None);
                continue;
            };
            let mut name = sel.attr("name").unwrap_or("").to_owned();
            if check_sheet_name(&name).is_err() || used_names.contains(&name.to_uppercase()) {
                let fixed = (1..)
                    .map(|n| format!("Sheet{n}"))
                    .find(|n| !used_names.contains(&n.to_uppercase()))
                    .unwrap_or_default();
                self.warn(format!(
                    "the sheet name \"{name}\" is not valid in Excel; the sheet is called \"{fixed}\""
                ));
                name = fixed;
            }
            used_names.insert(name.to_uppercase());
            let file_id = au32(&sel, "sheetId").filter(|i| *i > 0 && !used_ids.contains(i));
            let id = file_id
                .unwrap_or_else(|| (1..).find(|i| !used_ids.contains(i)).unwrap_or(u32::MAX));
            used_ids.insert(id);
            let sid = SheetId(id);
            index_of_all.push(Some(sid));
            let visibility = match sel.attr("state") {
                Some("hidden") => SheetVisibility::Hidden,
                Some("veryHidden") => SheetVisibility::VeryHidden,
                _ => SheetVisibility::Visible,
            };
            let ctx = SheetCtx {
                part: &part,
                sst: &sst,
                xf_count,
                dynamic_cm: &dynamic_cm,
                has_metadata: metadata_part.is_some(),
                date1904: props.date1904,
                persons: &persons,
            };
            let (snap, src) = self.read_sheet(
                &ctx,
                sid,
                name,
                visibility,
                sel,
                &mut next_rule,
                &mut table_ids,
                &mut max_table,
            )?;
            sheets.push(snap);
            sources.push(src);
        }
        if sheets.is_empty() {
            return Err(XlsxError::NotWorkbook(
                "the workbook has no worksheets".into(),
            ));
        }
        let model_index = |all: usize| -> usize {
            let id = index_of_all
                .iter()
                .skip(all)
                .chain(index_of_all.iter().take(all).rev())
                .flatten()
                .next()
                .copied();
            id.and_then(|id| sheets.iter().position(|s| s.id == id.0))
                .unwrap_or(0)
        };
        props.active_sheet = model_index(active_all);
        props.first_sheet = model_index(first_all);

        // Defined names.
        let mut names: Vec<DefinedName> = Vec::new();
        let mut kept_names: Vec<(DefinedName, XmlElement)> = Vec::new();
        let mut raw_names: Vec<XmlElement> = Vec::new();
        for dn in wb_root
            .child("definedNames")
            .into_iter()
            .flat_map(|d| d.children_named("definedName"))
        {
            let Some(name) = dn.attr("name") else {
                continue;
            };
            let scope = match au32(dn, "localSheetId") {
                Some(i) => match index_of_all.get(i as usize).copied().flatten() {
                    Some(id) => Some(id),
                    None => {
                        raw_names.push(dn.clone());
                        continue;
                    }
                },
                None => None,
            };
            let text = dn.text();
            let formula = crate::formula_text::from_file_formula(
                text.trim().trim_start_matches('='),
                Some(&mut self.future),
            );
            let def = DefinedName {
                name: name.to_owned(),
                formula,
                scope,
                hidden: abool_or(dn, "hidden", false),
                comment: dn.attr("comment").map(str::to_owned),
            };
            let dup = names
                .iter()
                .any(|n| n.scope == def.scope && n.name.eq_ignore_ascii_case(&def.name));
            if !is_defined_name_valid(name) || dup {
                raw_names.push(dn.clone());
                continue;
            }
            names.push(def.clone());
            kept_names.push((def, dn.clone()));
        }

        let snapshot = WorkbookSnapshot {
            sheets,
            names,
            styles,
            theme: theme.clone().unwrap_or_default(),
            props: props.clone(),
            protection: protection.clone(),
            persons: persons.clone(),
            custom_lists: Vec::new(),
            passthrough: Default::default(),
            counters: (
                used_ids
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1),
                max_table.saturating_add(1),
                next_rule,
            ),
        };
        let source = XlsxSource {
            package: OpcPackage::new(),
            kind,
            strict,
            workbook_part,
            workbook_root: wb_root,
            other_sheets,
            styles_part,
            styles: styles_src,
            sst_part,
            theme_part,
            theme,
            metadata_part,
            dynamic_cm: dynamic_cm.iter().min().copied(),
            persons_part,
            persons,
            sheets: sources,
            names: kept_names,
            raw_names,
            props,
            protection,
            future_functions: BTreeSet::new(),
        };
        Ok((snapshot, source))
    }

    fn read_sst(&mut self, part: &str) -> Result<Vec<CellValue>, XlsxError> {
        let bytes = self.pkg.part_bytes(part)?;
        let text = decode(&bytes);
        let mut r = stream::reader(&text);
        let mut out = Vec::new();
        let mut interned: FxHashMap<Arc<str>, ()> = FxHashMap::default();
        loop {
            let ev = r.read_event().map_err(|e| stream::xml_error(part, &r, e))?;
            match ev {
                Event::Start(s) if stream::local(s.name().0) == "si" => {
                    let e = stream::subtree(part, &mut r, &s)?;
                    let v = parse_string_item(&e);
                    let v = match v {
                        CellValue::Text(t) => {
                            // Duplicate shared strings share one allocation.
                            match interned.get_key_value(&t) {
                                Some((k, ())) => CellValue::Text(k.clone()),
                                None => {
                                    interned.insert(t.clone(), ());
                                    CellValue::Text(t)
                                }
                            }
                        }
                        other => other,
                    };
                    out.push(v);
                }
                Event::Empty(s) if stream::local(s.name().0) == "si" => {
                    out.push(CellValue::text(""));
                }
                Event::DocType(_) => {
                    return Err(stream::xml_error(part, &r, "DTDs are not allowed"));
                }
                Event::GeneralRef(g) => {
                    stream::resolve_ref(part, &r, &g)?;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn read_sheet(
        &mut self,
        ctx: &SheetCtx<'_>,
        id: SheetId,
        name: String,
        visibility: SheetVisibility,
        sheet_el: XmlElement,
        next_rule: &mut u32,
        table_ids: &mut FxHashSet<u32>,
        max_table: &mut u32,
    ) -> Result<(SheetSnapshot, SheetSource), XlsxError> {
        let part = ctx.part;
        let bytes = self.pkg.part_bytes(part)?;
        let text = decode(&bytes);
        let streamed = cells::read_sheet_cells(
            &text,
            &cells::CellContext {
                part,
                sst: ctx.sst,
                xf_count: ctx.xf_count,
                dynamic_cm: ctx.dynamic_cm,
                has_metadata: ctx.has_metadata,
                date1904: ctx.date1904,
            },
        )?;
        for w in &streamed.warnings {
            self.warn(w.clone());
        }
        self.future
            .extend(streamed.future_functions.iter().cloned());
        let rest = match streamed.span {
            Some((a, b)) if a <= b && b <= text.len() => {
                let mut s = String::with_capacity(text.len() - (b - a) + 16);
                s.push_str(&text[..a]);
                s.push_str("<sheetData/>");
                s.push_str(&text[b..]);
                s
            }
            _ => text.into_owned(),
        };
        let mut root = parse_part_text(part, &rest)?;
        drop(rest);
        if root.child("sheetData").is_none() {
            crate::xmlx::replace_children(
                &mut root,
                "sheetData",
                vec![XmlElement::new("sheetData")],
                crate::sheet_xml::WORKSHEET_ORDER,
            );
        }
        let rels = self.pkg.relationships(part)?.into_owned();

        let mut meta = SheetMeta {
            visibility,
            ..SheetMeta::default()
        };
        if let Some(pr) = root.child("sheetPr") {
            meta.tab_color = pr.child("tabColor").and_then(parse_color);
            if let Some(o) = pr.child("outlinePr") {
                meta.summary_below = abool_or(o, "summaryBelow", true);
                meta.summary_right = abool_or(o, "summaryRight", true);
            }
        }
        if let Some(f) = root.child("sheetFormatPr") {
            if let Some(h) = anum(f, "defaultRowHeight").filter(|h| *h > 0.0) {
                meta.default_row_height = h;
            }
            meta.default_col_width = anum(f, "defaultColWidth").filter(|w| *w > 0.0);
        }
        if let Some(v) = root.child("sheetViews").and_then(|v| v.child("sheetView")) {
            meta.view = parse_sheet_view(v);
        }
        meta.protection = root
            .child("sheetProtection")
            .and_then(parse_sheet_protection);
        meta.auto_filter = root.child("autoFilter").and_then(parse_auto_filter);
        meta.merges = root
            .child("mergeCells")
            .map(|m| {
                m.children_named("mergeCell")
                    .filter_map(|c| c.attr("ref").and_then(parse_ref))
                    .filter(|r| !r.is_single_cell())
                    .collect()
            })
            .unwrap_or_default();
        meta.print = parse_print(&root);

        // Conditional formatting (2007 elements and the x14 extension).
        let cf_xml: Vec<XmlElement> = root
            .children_named("conditionalFormatting")
            .cloned()
            .collect();
        let mut by_ext: FxHashMap<String, (usize, usize)> = FxHashMap::default();
        for block in &cf_xml {
            let ranges = block.attr("sqref").map(parse_ranges).unwrap_or_default();
            if ranges.is_empty() {
                continue;
            }
            let mut rules = Vec::new();
            for r in block.children_named("cfRule") {
                match parse_cf_rule(r, *next_rule) {
                    Some((rule, ext)) => {
                        *next_rule += 1;
                        if let Some(ext) = ext {
                            by_ext.insert(ext.to_ascii_uppercase(), (meta.cf.len(), rules.len()));
                        }
                        rules.push(rule);
                    }
                    None => self.warn(format!(
                        "{part}: a conditional format of type \"{}\" is kept but not shown",
                        r.attr("type").unwrap_or("?")
                    )),
                }
            }
            if !rules.is_empty() {
                meta.cf.push(ConditionalFormat { ranges, rules });
            }
        }
        let ext_lst = root.child("extLst").cloned();
        let ext_by_uri = |uri: &str| -> Vec<XmlElement> {
            ext_lst
                .iter()
                .flat_map(|l| l.children_named("ext"))
                .filter(|e| e.attr("uri").is_some_and(|u| u.eq_ignore_ascii_case(uri)))
                .cloned()
                .collect()
        };
        let cf_ext = ext_by_uri("{78C0D931-6437-407d-A8EE-F0AAD7539E65}");
        for ext in &cf_ext {
            for block in ext.descendants("x14:conditionalFormatting") {
                let sqref = child_local(block, "sqref")
                    .map(XmlElement::text)
                    .unwrap_or_default();
                let mut extra_rules = Vec::new();
                for x in children_local(block, "cfRule") {
                    let xid = x.attr("id").unwrap_or("").to_ascii_uppercase();
                    if let Some(&(b, i)) = by_ext.get(&xid) {
                        if let Some(rule) = meta.cf.get_mut(b).and_then(|c| c.rules.get_mut(i)) {
                            apply_x14_rule(rule, x);
                        }
                    } else if let Some(rule) = parse_x14_only_rule(x, *next_rule) {
                        *next_rule += 1;
                        extra_rules.push(rule);
                    }
                }
                let ranges = parse_ranges(&sqref);
                if !extra_rules.is_empty() && !ranges.is_empty() {
                    meta.cf.push(ConditionalFormat {
                        ranges,
                        rules: extra_rules,
                    });
                }
            }
        }

        // Data validation.
        let dv_xml = root.child("dataValidations").cloned();
        for d in dv_xml
            .iter()
            .flat_map(|d| d.children_named("dataValidation"))
        {
            if let Some(v) = parse_data_validation(d, *next_rule) {
                *next_rule += 1;
                meta.validations.push(v);
            }
        }
        let dv_ext = ext_by_uri("{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}");
        for ext in &dv_ext {
            for d in ext.descendants("x14:dataValidation") {
                if let Some(v) = parse_data_validation(d, *next_rule) {
                    *next_rule += 1;
                    meta.validations.push(v);
                }
            }
        }

        // Hyperlinks.
        let mut hyperlink_rels = std::collections::BTreeMap::new();
        for h in root
            .child("hyperlinks")
            .into_iter()
            .flat_map(|h| h.children_named("hyperlink"))
        {
            let Some(range) = h.attr("ref").and_then(parse_ref) else {
                continue;
            };
            let target = match h.attr("r:id") {
                Some(rid) => match rels.get(rid) {
                    Some(rel) => {
                        hyperlink_rels.insert(rel.target.clone(), rid.to_owned());
                        Some(rel.target.clone())
                    }
                    None => None,
                },
                None => None,
            };
            meta.hyperlinks.push(Hyperlink {
                range,
                target,
                location: h.attr("location").map(str::to_owned),
                display: h.attr("display").map(str::to_owned),
                tooltip: h.attr("tooltip").map(str::to_owned),
            });
        }

        // Notes.
        let mut notes_src = None;
        if let Some(cp) = rels
            .first_by_type(rel_type::COMMENTS)
            .and_then(|r| rels.resolve(r))
            .filter(|p| self.pkg.has(p))
        {
            let mut notes: Vec<Note> = self
                .xml(&cp)?
                .map(|r| parse_comments(&r))
                .unwrap_or_default();
            let vml_part = root
                .child("legacyDrawing")
                .and_then(|l| l.attr("r:id"))
                .and_then(|id| rels.get(id))
                .and_then(|r| rels.resolve(r))
                .filter(|p| self.pkg.has(p));
            if let Some(vp) = &vml_part {
                let shapes = match self.xml(vp) {
                    Ok(Some(r)) => parse_vml_notes(&r),
                    _ => {
                        self.warn(format!("{vp}: the note shapes could not be read"));
                        FxHashMap::default()
                    }
                };
                for n in &mut notes {
                    if let Some((v, s)) = shapes.get(&(n.at.row, n.at.col)) {
                        n.visible = *v;
                        n.size = *s;
                    }
                }
            }
            meta.notes = notes.clone();
            notes_src = Some(NotesSource {
                comments_part: cp,
                vml_part,
            });
        }

        // Threaded comments.
        let mut threads_part = None;
        if let Some(tp) = rels
            .first_by_type(REL_THREADED)
            .and_then(|r| rels.resolve(r))
            .filter(|p| self.pkg.has(p))
        {
            if let Some(r) = self.xml(&tp)? {
                meta.threads = parse_threaded(&r, ctx.persons);
            }
            threads_part = Some(tp);
        }

        // Tables.
        let mut tables = std::collections::BTreeMap::new();
        for tp in root
            .child("tableParts")
            .into_iter()
            .flat_map(|t| t.children_named("tablePart"))
        {
            let Some(p) = tp
                .attr("r:id")
                .and_then(|id| rels.get(id))
                .and_then(|r| rels.resolve(r))
                .filter(|p| self.pkg.has(p))
            else {
                continue;
            };
            let Some(troot) = self.xml(&p)? else { continue };
            let Some(mut t) = parse_table(&troot) else {
                self.warn(format!("{p}: the table could not be read"));
                continue;
            };
            if !table_ids.insert(t.id) {
                let fresh = (*max_table).max(t.id).saturating_add(1);
                self.warn(format!(
                    "{p}: table id {} is used twice; it becomes {fresh}",
                    t.id
                ));
                t.id = fresh;
                table_ids.insert(fresh);
            }
            *max_table = (*max_table).max(t.id);
            tables.insert(t.id, (p, t.clone(), troot));
            meta.tables.push(t);
        }

        // Drawings.
        let mut drawing = None;
        if let Some(dp) = root
            .child("drawing")
            .and_then(|d| d.attr("r:id"))
            .and_then(|id| rels.get(id))
            .and_then(|r| rels.resolve(r))
            .filter(|p| self.pkg.has(p))
            && let Some(droot) = self.xml(&dp)?
        {
            let drels = self.pkg.relationships(&dp)?.into_owned();
            let resolve = |id: &str| drels.get(id).and_then(|r| drels.resolve(r));
            let anchors = parse_drawing(&droot, &resolve);
            meta.drawings = anchors.clone();
            drawing = Some(DrawingSource {
                part: dp,
                anchors,
                root: droot,
            });
        }

        let layout = SheetLayout {
            rows: streamed.rows,
            cols: root.child("cols").map(parse_cols).unwrap_or_default(),
        };
        let cells: Vec<CellRecord> = streamed.cells;
        let snap = SheetSnapshot {
            id: id.0,
            name,
            cells,
            layout: layout.clone(),
            meta: meta.clone(),
        };
        let src = SheetSource {
            id,
            part: part.to_owned(),
            sheet_el,
            root,
            meta,
            layout,
            array_formulas: streamed.array_formulas,
            dynamic: streamed.dynamic,
            cells: streamed.extras,
            row_attrs: streamed.row_attrs,
            hyperlink_rels,
            dv_xml,
            notes: notes_src,
            threads_part,
            tables,
            drawing,
        };
        Ok((snap, src))
    }
}

struct SheetCtx<'a> {
    part: &'a str,
    sst: &'a [CellValue],
    xf_count: u32,
    dynamic_cm: &'a FxHashSet<u32>,
    has_metadata: bool,
    date1904: bool,
    persons: &'a [Person],
}
