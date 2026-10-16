// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`write_xlsx`]: [`kiln_sheet::Workbook`] → a SpreadsheetML package.
//!
//! With the [`XlsxSource`] of the file read, the original package is the starting point: the
//! parts the model covers (workbook, worksheets, styles, shared strings, notes, threaded
//! comments, tables, cell metadata, and drawings and the theme when they changed) are written
//! from the model, reusing the XML read wherever the model still holds what it said; every other
//! part keeps its bytes, and its relationships their ids. `calcChain.xml` is dropped (Excel
//! rebuilds it). Parts that only the removed content referred to (the table of a deleted sheet)
//! go with it. Without a source a new package is written.

mod cells;
mod sheet;

use std::collections::{BTreeSet, VecDeque};

use kiln_ooxml::opc::{
    OpcPackage, RelationshipOptions, part_key, rel_type, to_transitional_rel_type,
};
use kiln_ooxml::props::{
    AppProperties, CoreProperties, PropertiesUpdate, read_document_properties,
    write_document_properties,
};
use kiln_ooxml::xml::{WriteOptions as XmlWriteOptions, XmlElement, XmlNode, write_element};
use kiln_sheet::meta::{CalcMode, Note, SheetVisibility};
use kiln_sheet::{SheetId, Workbook};

use crate::XlsxError;
use crate::parts::{
    CT_COMMENTS, CT_METADATA, CT_PERSON, CT_TABLE, CT_THREADED, CT_VML, DYNAMIC_METADATA,
    REL_PERSON, REL_THREADED, comments_el, notes_vml, patch_drawing, persons_el, table_el,
    threaded_el, write_theme,
};
use crate::sheet_xml::{WORKSHEET_ORDER, file_formula, set_password};
use crate::source::{SheetSource, XlsxKind, XlsxSource};
use crate::strings::SstBuilder;
use crate::styles::write_styles;
use crate::xmlx::{replace_children, set_attr};

/// How to write.
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    /// The package kind (default: the kind read, else `.xlsx`). Saving a macro-enabled workbook
    /// as `.xlsx` drops its macros.
    pub kind: Option<XlsxKind>,
    /// The save time (W3CDTF): sets `dcterms:modified` (and `dcterms:created` when missing) and
    /// the application name. `None` leaves the document properties as read, so a rewrite is
    /// deterministic.
    pub now: Option<String>,
    /// `cp:lastModifiedBy` (with `now`).
    pub author: Option<String>,
}

/// The result of a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxWritten {
    /// The package.
    pub bytes: Vec<u8>,
    /// Parts written from the model, relationships parts that changed and
    /// `/[Content_Types].xml` when it changed; every other part is a copy of the original.
    pub regenerated: Vec<String>,
    /// Parts of the original package that the output no longer has.
    pub dropped: Vec<String>,
    /// What could not be written exactly.
    pub issues: Vec<String>,
}

const WORKBOOK_ORDER: &[&str] = &[
    "fileVersion",
    "fileSharing",
    "workbookPr",
    "workbookProtection",
    "bookViews",
    "sheets",
    "functionGroups",
    "externalReferences",
    "definedNames",
    "calcPr",
    "oleSize",
    "customWorkbookViews",
    "pivotCaches",
    "smartTagPr",
    "smartTagTypes",
    "webPublishing",
    "fileRecoveryPr",
    "webPublishObjects",
    "extLst",
];

const CT_WORKSHEET: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const CT_SST: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

struct Out {
    pkg: OpcPackage,
    regenerated: BTreeSet<String>,
    issues: Vec<String>,
}

/// A part's XML as bytes. A tree that cannot be serialised (say, a reused subtree with a prefix
/// no ancestor declares) is an error naming the part, never an empty part in a corrupt package.
fn xml_bytes(name: &str, root: &XmlElement) -> Result<Vec<u8>, XlsxError> {
    write_element(root, &XmlWriteOptions::default())
        .map(String::into_bytes)
        .map_err(|e| XlsxError::Xml {
            part: name.to_owned(),
            message: e.to_string(),
        })
}

impl Out {
    fn put(&mut self, name: &str, ct: &str, bytes: Vec<u8>) -> Result<(), XlsxError> {
        if self.pkg.has(name) {
            self.pkg.set_part(name, bytes)?;
            if self.pkg.content_type(name) != Some(ct) {
                self.pkg.set_content_type(name, ct)?;
            }
        } else {
            self.pkg.add_part(name, ct, bytes)?;
        }
        self.regenerated.insert(name.to_owned());
        Ok(())
    }

    fn put_xml(&mut self, name: &str, ct: &str, root: &XmlElement) -> Result<(), XlsxError> {
        let bytes = xml_bytes(name, root)?;
        self.put(name, ct, bytes)
    }

    /// The id of a relationship of `source` to `target` of type `ty`, added when missing.
    fn rel(&mut self, source: &str, ty: &str, target: &str) -> Result<String, XlsxError> {
        let rels = self.pkg.relationships(source)?;
        if let Some(r) = rels.by_type(ty).find(|r| {
            rels.resolve(r)
                .is_some_and(|t| part_key(&t) == part_key(target))
        }) {
            return Ok(r.id.clone());
        }
        drop(rels);
        let r = self
            .pkg
            .add_relationship(source, ty, target, RelationshipOptions::default())?;
        self.regenerated
            .insert(kiln_ooxml::opc::rels_part_name(source));
        Ok(r.id)
    }

    fn external_rel(
        &mut self,
        source: &str,
        ty: &str,
        target: &str,
        prefer: Option<&str>,
    ) -> Result<String, XlsxError> {
        let rels = self.pkg.relationships(source)?;
        if let Some(id) = prefer
            && rels
                .get(id)
                .is_some_and(|r| r.is_external() && r.target == target && r.rel_type == ty)
        {
            return Ok(id.to_owned());
        }
        if let Some(r) = rels
            .by_type(ty)
            .find(|r| r.is_external() && r.target == target)
        {
            return Ok(r.id.clone());
        }
        drop(rels);
        let r = self.pkg.add_relationship(
            source,
            ty,
            target,
            RelationshipOptions {
                id: None,
                external: true,
            },
        )?;
        self.regenerated
            .insert(kiln_ooxml::opc::rels_part_name(source));
        Ok(r.id)
    }

    fn remove_rel(&mut self, source: &str, id: &str) -> Result<(), XlsxError> {
        if self.pkg.remove_relationship(source, id)? {
            self.regenerated
                .insert(kiln_ooxml::opc::rels_part_name(source));
        }
        Ok(())
    }

    /// Removes every relationship of `source` of type `ty` not in `keep`.
    fn prune_rels(
        &mut self,
        source: &str,
        ty: &str,
        keep: &BTreeSet<String>,
    ) -> Result<(), XlsxError> {
        let ids: Vec<String> = self
            .pkg
            .relationships(source)?
            .by_type(ty)
            .filter(|r| !keep.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        for id in ids {
            self.remove_rel(source, &id)?;
        }
        Ok(())
    }

    fn next_name(&self, pattern: &str, taken: &BTreeSet<String>) -> Result<String, XlsxError> {
        let mut n = 1u64;
        loop {
            let c = pattern.replacen("{n}", &n.to_string(), 1);
            if !self.pkg.has(&c) && !taken.contains(&part_key(&c)) {
                return Ok(c);
            }
            n += 1;
            if n > 1_000_000 {
                return Err(XlsxError::NotWorkbook("too many parts".into()));
            }
        }
    }
}

/// Parts reachable from the package relationships.
fn reachable(pkg: &OpcPackage) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from(["/".to_owned()]);
    while let Some(src) = queue.pop_front() {
        let Ok(rels) = pkg.relationships(&src) else {
            continue;
        };
        for r in rels.all() {
            if let Some(t) = rels.resolve(r)
                && pkg.has(&t)
                && seen.insert(part_key(&t))
            {
                queue.push_back(t);
            }
        }
    }
    seen
}

fn new_package() -> Result<OpcPackage, XlsxError> {
    let mut pkg = OpcPackage::new();
    pkg.add_part(
        "/xl/workbook.xml",
        XlsxKind::Workbook.content_type(),
        Vec::new(),
    )?;
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/xl/workbook.xml",
        RelationshipOptions::default(),
    )?;
    Ok(pkg)
}

fn workbook_template() -> XmlElement {
    XmlElement::new("workbook")
        .with_attr(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        )
        .with_attr(
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        )
        .with_child(XmlElement::new("workbookPr"))
        .with_child(XmlElement::new("bookViews").with_child(XmlElement::new("workbookView")))
        .with_child(XmlElement::new("sheets"))
        .with_child(XmlElement::new("calcPr").with_attr("calcId", "191029"))
}

fn is_macro_ct(ct: &str) -> bool {
    let l = ct.to_ascii_lowercase();
    l.contains("vbaproject") || l.contains("ms-office.activex") || l.contains("vbadata")
}

/// Reads and writes a workbook without editing it (the fidelity round trip).
///
/// # Errors
///
/// As [`crate::read_xlsx`] and [`write_xlsx`].
pub fn rewrite_xlsx(bytes: &[u8], options: &WriteOptions) -> Result<XlsxWritten, XlsxError> {
    let read = crate::read_xlsx(bytes)?;
    write_xlsx(&read.workbook, Some(&read.source), options)
}

/// Writes a workbook; with the source of the file read, the save keeps everything the model
/// does not cover.
///
/// # Errors
///
/// [`XlsxError::Package`] when the package cannot be assembled.
#[allow(clippy::too_many_lines)]
pub fn write_xlsx(
    wb: &Workbook,
    source: Option<&XlsxSource>,
    options: &WriteOptions,
) -> Result<XlsxWritten, XlsxError> {
    let kind = options
        .kind
        .or(source.map(XlsxSource::kind))
        .unwrap_or_default();
    let pkg = match source {
        Some(s) => s.package.clone(),
        None => new_package()?,
    };
    let before = reachable(&pkg);
    let original: Vec<String> = pkg.all_part_names().map(str::to_owned).collect();
    let mut out = Out {
        pkg,
        regenerated: BTreeSet::new(),
        issues: Vec::new(),
    };
    let wb_part = source.map_or_else(
        || "/xl/workbook.xml".to_owned(),
        |s| s.workbook_part.clone(),
    );
    let mut taken: BTreeSet<String> = BTreeSet::new();

    // Strict files are saved as transitional: every relationship type is mapped.
    if source.is_some_and(XlsxSource::is_strict) {
        transitional_rels(&mut out)?;
    }

    // Macros only into macro-enabled kinds.
    if !kind.has_macros() {
        let macro_parts: Vec<String> = out
            .pkg
            .part_names()
            .filter(|p| out.pkg.content_type(p).is_some_and(is_macro_ct))
            .map(str::to_owned)
            .collect();
        for p in macro_parts {
            out.pkg.remove_part(&p, true)?;
            out.regenerated
                .insert(kiln_ooxml::opc::rels_part_name(&wb_part));
            out.issues
                .push(format!("{p}: macros are not kept in this file type"));
        }
    }

    // Shared strings are written after the sheets; reserve the part name.
    let sst_part = source
        .and_then(|s| s.sst_part.clone())
        .unwrap_or_else(|| "/xl/sharedStrings.xml".to_owned());
    taken.insert(part_key(&sst_part));
    let styles_part = source
        .and_then(|s| s.styles_part.clone())
        .unwrap_or_else(|| "/xl/styles.xml".to_owned());
    taken.insert(part_key(&styles_part));

    // Dynamic-array metadata index.
    let dynamic_cm = source.and_then(|s| s.dynamic_cm).unwrap_or(1);
    let has_metadata = source.is_some_and(|s| s.metadata_part.is_some());
    let metadata_usable = !has_metadata || source.is_some_and(|s| s.dynamic_cm.is_some());

    let future = source
        .map(|s| s.future_functions.clone())
        .unwrap_or_default();
    let mut sst = SstBuilder::default();
    let xf_count = wb.styles().len() as u32;
    let active = wb.props().active_sheet;

    // Sheet parts: kept for sheets read, new ones numbered after them.
    let model_ids: BTreeSet<u32> = wb.sheets().iter().map(|s| s.id().0).collect();
    let mut sheet_parts: Vec<(SheetId, String)> = Vec::new();
    for s in wb.sheets() {
        if let Some(src) = source.and_then(|x| x.sheet(s.id())) {
            taken.insert(part_key(&src.part));
            sheet_parts.push((s.id(), src.part.clone()));
        }
    }
    for s in wb.sheets() {
        if sheet_parts.iter().any(|(id, _)| *id == s.id()) {
            continue;
        }
        let name = out.next_name("/xl/worksheets/sheet{n}.xml", &taken)?;
        taken.insert(part_key(&name));
        sheet_parts.push((s.id(), name));
    }
    // Sheets removed since reading.
    if let Some(src) = source {
        for s in &src.sheets {
            if !model_ids.contains(&s.id.0) {
                out.pkg.remove_part(&s.part, true)?;
                out.regenerated
                    .insert(kiln_ooxml::opc::rels_part_name(&wb_part));
            }
        }
    }

    let mut any_dynamic = false;
    let mut sheet_rels: Vec<(SheetId, String)> = Vec::new();
    for (pos, s) in wb.sheets().iter().enumerate() {
        let part = sheet_parts
            .iter()
            .find(|(id, _)| *id == s.id())
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        let src = source.and_then(|x| x.sheet(s.id()));
        let cells = cells::write_cells(
            wb,
            s,
            src,
            &mut sst,
            metadata_usable.then_some(dynamic_cm),
            &future,
            xf_count,
        );
        any_dynamic |= cells.dynamic;
        let mut root = src.map_or_else(sheet::template, |x| x.root.clone());
        let mut issues = Vec::new();
        sheet::patch_worksheet(&mut root, s, src, pos == active, cells.used, &mut issues);
        out.issues.extend(issues);
        if !out.pkg.has(&part) {
            out.pkg.add_part(&part, CT_WORKSHEET, Vec::new())?;
        }
        write_sheet_relations(
            &mut out,
            wb,
            s,
            src,
            &part,
            &mut root,
            &mut taken,
            pos as u32 + 1,
        )?;
        // Serialise with the streamed cells in place of the placeholder.
        let text = write_element(&root, &XmlWriteOptions::default()).unwrap_or_default();
        let bytes = match text.find("<sheetData/>") {
            Some(i) => {
                let mut b = String::with_capacity(text.len() + cells.xml.len());
                b.push_str(&text[..i]);
                b.push_str(&cells.xml);
                b.push_str(&text[i + "<sheetData/>".len()..]);
                b.into_bytes()
            }
            None => text.into_bytes(),
        };
        out.put(&part, CT_WORKSHEET, bytes)?;
        let id = out.rel(&wb_part, rel_type::WORKSHEET, &part)?;
        sheet_rels.push((s.id(), id));
    }

    // Styles.
    let styles_root = write_styles(wb.styles(), source.and_then(|s| s.styles.as_ref()));
    out.put_xml(&styles_part, CT_STYLES, &styles_root)?;
    out.rel(&wb_part, rel_type::STYLES, &styles_part)?;

    // Shared strings.
    if sst.items.is_empty() {
        if out.pkg.has(&sst_part) {
            out.pkg.remove_part(&sst_part, true)?;
            out.regenerated
                .insert(kiln_ooxml::opc::rels_part_name(&wb_part));
        }
    } else {
        out.put(&sst_part, CT_SST, sst.to_xml().into_bytes())?;
        out.rel(&wb_part, rel_type::SHARED_STRINGS, &sst_part)?;
    }

    // Theme: kept unless the model's colours or fonts changed (or there is none).
    let theme_changed = source.is_none_or(|s| s.theme.as_ref() != Some(wb.theme()));
    if theme_changed {
        let part = source
            .and_then(|s| s.theme_part.clone())
            .unwrap_or_else(|| "/xl/theme/theme1.xml".to_owned());
        let base = out
            .pkg
            .part(&part)?
            .and_then(|b| kiln_ooxml::drawingml::theme::parse_theme_bytes(&b).ok());
        out.put(
            &part,
            kiln_ooxml::opc::content_type::THEME,
            write_theme(wb.theme(), base),
        )?;
        out.rel(&wb_part, rel_type::THEME, &part)?;
    }

    // Dynamic-array metadata.
    if any_dynamic && !has_metadata {
        let part = "/xl/metadata.xml";
        out.put(part, CT_METADATA, DYNAMIC_METADATA.as_bytes().to_vec())?;
        out.rel(&wb_part, rel_type::SHEET_METADATA, part)?;
    }

    // People of threaded comments.
    let persons_changed = source.is_none_or(|s| s.persons != wb.persons());
    if persons_changed && !wb.persons().is_empty() {
        let part = source
            .and_then(|s| s.persons_part.clone())
            .unwrap_or_else(|| "/xl/persons/person.xml".to_owned());
        out.put_xml(&part, CT_PERSON, &persons_el(wb.persons()))?;
        out.rel(&wb_part, REL_PERSON, &part)?;
    }

    // calcChain is rebuilt by Excel; a stale one makes it repair the file.
    let chains: Vec<String> = out
        .pkg
        .relationships(&wb_part)?
        .by_type(rel_type::CALC_CHAIN)
        .map(|r| r.id.clone())
        .collect();
    for id in chains {
        out.remove_rel(&wb_part, &id)?;
    }

    // The workbook part.
    let root = workbook_xml(wb, source, &sheet_rels);
    out.put_xml(&wb_part, kind.content_type(), &root)?;

    // Document properties.
    if options.now.is_some() || source.is_none() {
        let props = read_document_properties(&out.pkg)?;
        let mut core: CoreProperties = props.core.clone();
        let mut app: AppProperties = props.app.clone();
        if let Some(now) = &options.now {
            core.modified = Some(now.clone());
            if core.created.is_none() {
                core.created = Some(now.clone());
            }
        }
        if let Some(a) = &options.author {
            core.last_modified_by = Some(a.clone());
            if core.creator.is_none() {
                core.creator = Some(a.clone());
            }
        }
        app.application = Some("Kiln".to_owned());
        write_document_properties(
            &mut out.pkg,
            &PropertiesUpdate {
                core: Some(core),
                app: Some(app),
                custom: None,
            },
        )?;
        for p in ["/docProps/core.xml", "/docProps/app.xml", "/_rels/.rels"] {
            out.regenerated.insert(p.to_owned());
        }
        if let Ok(Some(p)) = out.pkg.related_part("/", rel_type::CORE_PROPERTIES) {
            out.regenerated.insert(p);
        }
        if let Ok(Some(p)) = out.pkg.related_part("/", rel_type::EXTENDED_PROPERTIES) {
            out.regenerated.insert(p);
        }
    }

    // Strict parts kept verbatim are re-serialised in transitional namespaces.
    if source.is_some_and(XlsxSource::is_strict) {
        transitional_parts(&mut out)?;
    }

    // Parts only removed content referred to.
    let after = reachable(&out.pkg);
    let orphans: Vec<String> = out
        .pkg
        .part_names()
        .filter(|p| before.contains(&part_key(p)) && !after.contains(&part_key(p)))
        .map(str::to_owned)
        .collect();
    for p in orphans {
        out.pkg.remove_part(&p, true)?;
    }

    if out.pkg.content_types().is_dirty() {
        out.regenerated.insert("/[Content_Types].xml".to_owned());
    }
    let present: BTreeSet<String> = out.pkg.all_part_names().map(part_key).collect();
    let dropped: Vec<String> = original
        .into_iter()
        .filter(|p| !present.contains(&part_key(p)))
        .collect();
    let bytes = out.pkg.save()?;
    Ok(XlsxWritten {
        bytes,
        regenerated: out
            .regenerated
            .into_iter()
            .filter(|p| present.contains(&part_key(p)) || p.starts_with("/["))
            .collect(),
        dropped,
        issues: out.issues,
    })
}

/// Relationship-bearing worksheet children and the parts behind them: hyperlinks, notes,
/// threaded comments, tables and the drawing.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn write_sheet_relations(
    out: &mut Out,
    wb: &Workbook,
    s: &kiln_sheet::Sheet,
    src: Option<&SheetSource>,
    part: &str,
    root: &mut XmlElement,
    taken: &mut BTreeSet<String>,
    number: u32,
) -> Result<(), XlsxError> {
    let meta = s.meta();
    let read = src.map(|x| &x.meta);

    // Hyperlinks.
    if read.is_none_or(|r| r.hyperlinks != meta.hyperlinks) {
        let mut keep = BTreeSet::new();
        let mut list = XmlElement::new("hyperlinks");
        for h in &meta.hyperlinks {
            let mut e =
                XmlElement::new("hyperlink").with_attr("ref", crate::sheet_xml::ref_text(&h.range));
            if let Some(t) = &h.target {
                let prefer = src
                    .and_then(|x| x.hyperlink_rels.get(t))
                    .map(String::as_str);
                let id = out.external_rel(part, rel_type::HYPERLINK, t, prefer)?;
                keep.insert(id.clone());
                e.attrs.set("r:id", id);
            }
            for (k, v) in [
                ("location", &h.location),
                ("tooltip", &h.tooltip),
                ("display", &h.display),
            ] {
                if let Some(v) = v {
                    e.attrs.set(k, v.clone());
                }
            }
            list.children.push(XmlNode::Element(e));
        }
        out.prune_rels(part, rel_type::HYPERLINK, &keep)?;
        replace_children(
            root,
            "hyperlinks",
            (!meta.hyperlinks.is_empty())
                .then_some(list)
                .into_iter()
                .collect(),
            WORKSHEET_ORDER,
        );
    }

    // Notes (and legacy placeholders for threaded comments, which Excel expects).
    let notes_same = read.is_some_and(|r| r.notes == meta.notes);
    let threads_same = read.is_some_and(|r| r.threads == meta.threads);
    if !notes_same || !threads_same {
        let mut legacy: Vec<Note> = meta.notes.clone();
        for t in meta.threads.iter().filter(|t| t.parent_id.is_none()) {
            if !legacy.iter().any(|n| n.at == t.at) {
                legacy.push(Note {
                    at: t.at,
                    author: format!("tc={}", t.id),
                    text: format!(
                        "[Threaded comment]\n\nYour version of Excel allows you to read this threaded comment; however, any edits to it will get removed if the file is opened in a newer version of Excel.\n\nComment:\n    {}",
                        t.text
                    ),
                    visible: false,
                    size: None,
                });
            }
        }
        legacy.sort_by_key(|n| (n.at.row, n.at.col));
        let old = src.and_then(|x| x.notes.as_ref());
        if legacy.is_empty() {
            if let Some(o) = old {
                out.pkg.remove_part(&o.comments_part, true)?;
                out.regenerated
                    .insert(kiln_ooxml::opc::rels_part_name(part));
                if let Some(v) = &o.vml_part {
                    // A VML part with other shapes (form controls) stays.
                    if !vml_has_other_shapes(&out.pkg, v) {
                        out.pkg.remove_part(v, true)?;
                        replace_children(root, "legacyDrawing", Vec::new(), WORKSHEET_ORDER);
                    }
                }
            }
        } else {
            let cp = match old {
                Some(o) => o.comments_part.clone(),
                None => {
                    let n = out.next_name("/xl/comments{n}.xml", taken)?;
                    taken.insert(part_key(&n));
                    n
                }
            };
            out.put_xml(&cp, CT_COMMENTS, &comments_el(&legacy))?;
            out.rel(part, rel_type::COMMENTS, &cp)?;
            let vp = match old.and_then(|o| o.vml_part.clone()) {
                Some(v) => v,
                None => {
                    let n = out.next_name("/xl/drawings/vmlDrawing{n}.vml", taken)?;
                    taken.insert(part_key(&n));
                    n
                }
            };
            let vml = merge_vml(&out.pkg, &vp, &notes_vml(&legacy, number));
            out.put(&vp, CT_VML, vml.into_bytes())?;
            let id = out.rel(part, rel_type::VML_DRAWING, &vp)?;
            replace_children(
                root,
                "legacyDrawing",
                vec![XmlElement::new("legacyDrawing").with_attr("r:id", id)],
                WORKSHEET_ORDER,
            );
        }
    }

    // Threaded comments.
    if !threads_same {
        let old = src.and_then(|x| x.threads_part.clone());
        if meta.threads.is_empty() {
            if let Some(o) = old {
                out.pkg.remove_part(&o, true)?;
                out.regenerated
                    .insert(kiln_ooxml::opc::rels_part_name(part));
            }
        } else {
            let tp = match old {
                Some(o) => o,
                None => {
                    let n = out.next_name("/xl/threadedComments/threadedComment{n}.xml", taken)?;
                    taken.insert(part_key(&n));
                    n
                }
            };
            out.put_xml(&tp, CT_THREADED, &threaded_el(&meta.threads))?;
            out.rel(part, REL_THREADED, &tp)?;
            if wb.persons().is_empty() {
                out.issues.push(format!(
                    "{}: threaded comments without a people list",
                    s.name()
                ));
            }
        }
    }

    // Tables.
    let tables_same = read.is_some_and(|r| r.tables == meta.tables);
    if !tables_same {
        let mut keep = BTreeSet::new();
        let mut parts_el = XmlElement::new("tableParts").with_attr("count", meta.tables.len());
        for t in &meta.tables {
            let old = src.and_then(|x| x.tables.get(&t.id));
            let tp = match old {
                Some((p, read_t, troot)) => {
                    if read_t != t {
                        out.put_xml(p, CT_TABLE, &table_el(t, Some(troot)))?;
                    }
                    p.clone()
                }
                None => {
                    let n = out.next_name("/xl/tables/table{n}.xml", taken)?;
                    taken.insert(part_key(&n));
                    out.put_xml(&n, CT_TABLE, &table_el(t, None))?;
                    n
                }
            };
            let id = out.rel(part, rel_type::TABLE, &tp)?;
            keep.insert(id.clone());
            parts_el.children.push(XmlNode::Element(
                XmlElement::new("tablePart").with_attr("r:id", id),
            ));
        }
        out.prune_rels(part, rel_type::TABLE, &keep)?;
        replace_children(
            root,
            "tableParts",
            (!meta.tables.is_empty())
                .then_some(parts_el)
                .into_iter()
                .collect(),
            WORKSHEET_ORDER,
        );
    }

    // Drawing anchors.
    if let Some(d) = src.and_then(|x| x.drawing.as_ref())
        && d.anchors != meta.drawings
    {
        if meta.drawings.is_empty() {
            let ids: Vec<String> = out
                .pkg
                .relationships(part)?
                .by_type(rel_type::DRAWING)
                .map(|r| r.id.clone())
                .collect();
            for id in ids {
                out.remove_rel(part, &id)?;
            }
            replace_children(root, "drawing", Vec::new(), WORKSHEET_ORDER);
        } else {
            let mut issues = Vec::new();
            let patched = patch_drawing(&d.root, &d.anchors, &meta.drawings, &mut issues);
            out.issues.extend(issues);
            out.put_xml(
                &d.part,
                "application/vnd.openxmlformats-officedocument.drawing+xml",
                &patched,
            )?;
        }
    }
    Ok(())
}

fn vml_has_other_shapes(pkg: &OpcPackage, vml: &str) -> bool {
    pkg.part_xml(vml).ok().flatten().is_some_and(|d| {
        d.root.descendants("v:shape").iter().any(|s| {
            s.elements()
                .find(|e| kiln_ooxml::xml::namespaces::local_name(&e.name) == "ClientData")
                .is_some_and(|c| c.attr("ObjectType") != Some("Note"))
        })
    })
}

/// The note shapes of `generated` with the other shapes of the VML part read kept.
fn merge_vml(pkg: &OpcPackage, part: &str, generated: &str) -> String {
    let Some(doc) = pkg.part_xml(part).ok().flatten() else {
        return generated.to_owned();
    };
    let others: Vec<&XmlElement> = doc
        .root
        .elements()
        .filter(|e| e.name == "v:shape")
        .filter(|s| {
            s.elements()
                .find(|e| kiln_ooxml::xml::namespaces::local_name(&e.name) == "ClientData")
                .is_some_and(|c| c.attr("ObjectType") != Some("Note"))
        })
        .collect();
    if others.is_empty() {
        return generated.to_owned();
    }
    let opts = kiln_ooxml::xml::WriteOptions {
        declaration: kiln_ooxml::xml::Declaration::Omit,
        declare_namespaces: false,
    };
    let extra: String = others
        .into_iter()
        .filter_map(|e| write_element(e, &opts).ok())
        .collect();
    generated.replacen("</xml>", &format!("{extra}</xml>"), 1)
}

/// `workbook.xml`: sheets, defined names, properties, protection and views from the model; all
/// else as read.
#[allow(clippy::too_many_lines)]
fn workbook_xml(
    wb: &Workbook,
    source: Option<&XlsxSource>,
    rels: &[(SheetId, String)],
) -> XmlElement {
    let mut root = source.map_or_else(workbook_template, |s| s.workbook_root.clone());
    root.attrs.remove("conformance");
    let props = wb.props();
    let read = source.map(|s| &s.props);

    // Sheets, with chartsheets and dialog sheets where they were.
    let mut entries: Vec<XmlElement> = Vec::new();
    for s in wb.sheets() {
        let base = source
            .and_then(|x| x.sheet(s.id()))
            .map(|x| x.sheet_el.clone());
        let mut e = base.unwrap_or_else(|| XmlElement::new("sheet"));
        e.attrs.set("name", s.name().to_owned());
        e.attrs.set("sheetId", s.id().0.to_string());
        let state = match s.meta().visibility {
            SheetVisibility::Visible => e
                .attr("state")
                .filter(|v| *v == "visible")
                .map(str::to_owned),
            SheetVisibility::Hidden => Some("hidden".to_owned()),
            SheetVisibility::VeryHidden => Some("veryHidden".to_owned()),
        };
        set_attr(&mut e, "state", state);
        if let Some((_, id)) = rels.iter().find(|(i, _)| *i == s.id()) {
            e.attrs.set("r:id", id.clone());
        }
        entries.push(e);
    }
    let mut all: Vec<Option<SheetId>> = wb.sheets().iter().map(|s| Some(s.id())).collect();
    if let Some(src) = source {
        for (pos, el) in &src.other_sheets {
            let at = (*pos).min(entries.len());
            entries.insert(at, el.clone());
            all.insert(at, None);
        }
    }
    let mut sheets_el = root
        .child("sheets")
        .cloned()
        .unwrap_or_else(|| XmlElement::new("sheets"));
    sheets_el.children = entries.into_iter().map(XmlNode::Element).collect();
    replace_children(&mut root, "sheets", vec![sheets_el], WORKBOOK_ORDER);
    let all_index = |model: usize| -> usize {
        let id = wb.sheets().get(model).map(|s| s.id());
        all.iter().position(|x| *x == id).unwrap_or(model)
    };

    // Defined names.
    let mut names: Vec<XmlElement> = Vec::new();
    for n in wb.names() {
        let local = n.scope.and_then(|id| wb.sheet_index(id)).map(all_index);
        let orig =
            source.and_then(|s| s.names.iter().find(|(d, _)| d == n).map(|(_, e)| e.clone()));
        let mut e = match orig {
            Some(e) => e,
            None => {
                let mut e = XmlElement::new("definedName").with_attr("name", n.name.clone());
                if let Some(c) = &n.comment {
                    e.attrs.set("comment", c.clone());
                }
                if n.hidden {
                    e.attrs.set("hidden", "1");
                }
                e.children.push(XmlNode::text(file_formula(&n.formula)));
                e
            }
        };
        set_attr(&mut e, "localSheetId", local.map(|l| l.to_string()));
        names.push(e);
    }
    if let Some(src) = source {
        names.extend(src.raw_names.iter().cloned());
    }
    let names_el = (!names.is_empty()).then(|| {
        let mut d = root
            .child("definedNames")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("definedNames"));
        d.children = names.into_iter().map(XmlNode::Element).collect();
        d
    });
    replace_children(
        &mut root,
        "definedNames",
        names_el.into_iter().collect(),
        WORKBOOK_ORDER,
    );

    // workbookPr
    if read.is_none_or(|r| {
        r.date1904 != props.date1904
            || r.code_name != props.code_name
            || r.filter_privacy != props.filter_privacy
    }) {
        let mut p = root
            .child("workbookPr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("workbookPr"));
        set_attr(&mut p, "date1904", props.date1904.then(|| "1".to_owned()));
        set_attr(&mut p, "codeName", props.code_name.clone());
        set_attr(
            &mut p,
            "filterPrivacy",
            props.filter_privacy.then(|| "1".to_owned()),
        );
        replace_children(&mut root, "workbookPr", vec![p], WORKBOOK_ORDER);
    }

    // workbookProtection
    if source.is_none_or(|s| s.protection.as_ref() != wb.protection()) {
        let el = wb.protection().map(|p| {
            let mut e = XmlElement::new("workbookProtection");
            set_password(&mut e, p.password.as_ref(), "workbookPassword", "workbook");
            set_attr(
                &mut e,
                "lockStructure",
                p.lock_structure.then(|| "1".to_owned()),
            );
            set_attr(
                &mut e,
                "lockWindows",
                p.lock_windows.then(|| "1".to_owned()),
            );
            e
        });
        replace_children(
            &mut root,
            "workbookProtection",
            el.into_iter().collect(),
            WORKBOOK_ORDER,
        );
    }

    // bookViews
    let active = all_index(props.active_sheet);
    let first = all_index(props.first_sheet);
    let read_active = read.map(|r| all_index(r.active_sheet));
    if read.is_none_or(|r| {
        Some(active) != read_active
            || r.first_sheet != props.first_sheet
            || r.tab_ratio != props.tab_ratio
    }) || root.child("bookViews").is_none()
    {
        let mut views = root
            .child("bookViews")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("bookViews"));
        if views.child("workbookView").is_none() {
            views
                .children
                .push(XmlNode::Element(XmlElement::new("workbookView")));
        }
        if let Some(v) = views.child_mut("workbookView") {
            set_attr(v, "firstSheet", (first > 0).then(|| first.to_string()));
            set_attr(v, "activeTab", (active > 0).then(|| active.to_string()));
            set_attr(v, "tabRatio", props.tab_ratio.map(|t| t.to_string()));
        }
        replace_children(&mut root, "bookViews", vec![views], WORKBOOK_ORDER);
    }

    // calcPr
    if read.is_none_or(|r| r.calc != props.calc) {
        let c = &props.calc;
        let mut e = root
            .child("calcPr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("calcPr").with_attr("calcId", "191029"));
        set_attr(
            &mut e,
            "calcMode",
            match c.mode {
                CalcMode::Auto => None,
                CalcMode::Manual => Some("manual".to_owned()),
                CalcMode::AutoNoTable => Some("autoNoTable".to_owned()),
            },
        );
        set_attr(
            &mut e,
            "fullCalcOnLoad",
            c.full_calc_on_load.then(|| "1".to_owned()),
        );
        set_attr(
            &mut e,
            "calcOnSave",
            (!c.calc_on_save).then(|| "0".to_owned()),
        );
        set_attr(&mut e, "iterate", c.iterate.then(|| "1".to_owned()));
        set_attr(
            &mut e,
            "iterateCount",
            (c.iterate_count != 100).then(|| c.iterate_count.to_string()),
        );
        set_attr(
            &mut e,
            "iterateDelta",
            (c.iterate_delta != 0.001).then(|| crate::text::fmt_num(c.iterate_delta)),
        );
        set_attr(
            &mut e,
            "fullPrecision",
            (!c.full_precision).then(|| "0".to_owned()),
        );
        replace_children(&mut root, "calcPr", vec![e], WORKBOOK_ORDER);
    }
    root
}

/// Maps Strict relationship types to transitional in every `.rels` part.
fn transitional_rels(out: &mut Out) -> Result<(), XlsxError> {
    for src in out.pkg.relationship_sources() {
        let rels = out.pkg.relationships(&src)?.into_owned();
        let strict: Vec<_> = rels
            .all()
            .iter()
            .filter(|r| to_transitional_rel_type(&r.rel_type) != r.rel_type)
            .cloned()
            .collect();
        for r in strict {
            out.pkg.remove_relationship(&src, &r.id)?;
            out.pkg.add_relationship_raw(
                &src,
                &to_transitional_rel_type(&r.rel_type),
                &r.target,
                Some(&r.id),
                r.target_mode,
            )?;
            out.regenerated
                .insert(kiln_ooxml::opc::rels_part_name(&src));
        }
    }
    Ok(())
}

/// Re-serialises the XML parts kept from a Strict file (namespaces become transitional).
fn transitional_parts(out: &mut Out) -> Result<(), XlsxError> {
    let names: Vec<String> = out
        .pkg
        .part_names()
        .filter(|p| !out.regenerated.contains(*p))
        .map(str::to_owned)
        .collect();
    for p in names {
        let Some(ct) = out.pkg.content_type(&p).map(str::to_owned) else {
            continue;
        };
        if !kiln_ooxml::opc::is_xml_content_type(&ct) {
            continue;
        }
        let Ok(Some(bytes)) = out.pkg.part(&p).map(|b| b.map(|b| b.into_owned())) else {
            continue;
        };
        if !String::from_utf8_lossy(&bytes).contains("purl.oclc.org/ooxml") {
            continue;
        }
        if let Ok(doc) = kiln_ooxml::xml::parse_xml(&bytes) {
            let b = kiln_ooxml::xml::document_to_bytes(&doc).unwrap_or(bytes);
            out.put(&p, &ct, b)?;
        }
    }
    Ok(())
}

/// The relationships a written workbook's part graph keeps, for tests.
#[cfg(test)]
pub(crate) fn reachable_parts(bytes: &[u8]) -> BTreeSet<String> {
    OpcPackage::open(bytes)
        .map(|p| reachable(&p))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unserialisable_part_is_an_error_not_an_empty_part() {
        let mut root = XmlElement::new("worksheet");
        root.children
            .push(kiln_ooxml::xml::XmlNode::Element(XmlElement::new(
                "zz9:mystery",
            )));
        match xml_bytes("/xl/worksheets/sheet1.xml", &root) {
            Err(XlsxError::Xml { part, message }) => {
                assert_eq!(part, "/xl/worksheets/sheet1.xml");
                assert!(message.contains("zz9"), "{message}");
            }
            other => panic!("expected an XML error, got {other:?}"),
        }
        assert!(xml_bytes("/x.xml", &XmlElement::new("worksheet")).is_ok());
    }

    #[test]
    fn a_new_workbook_is_a_complete_package() {
        let mut wb = Workbook::new();
        let id = wb.first_sheet().expect("sheet");
        wb.load_value(id, 0, 0, kiln_sheet::CellValue::text("hello"))
            .expect("value");
        wb.load_formula(id, 1, 0, "LEN(A1)", kiln_sheet::CellValue::Blank, false)
            .expect("formula");
        wb.recalc_all();
        let w = write_xlsx(&wb, None, &WriteOptions::default()).expect("write");
        let parts = reachable_parts(&w.bytes);
        for p in [
            "/xl/workbook.xml",
            "/xl/worksheets/sheet1.xml",
            "/xl/styles.xml",
            "/xl/sharedstrings.xml",
            "/xl/theme/theme1.xml",
        ] {
            assert!(parts.contains(p), "{p} in {parts:?}");
        }
        let back = crate::read_xlsx(&w.bytes).expect("read back");
        assert_eq!(
            back.workbook.value(id, 1, 0),
            kiln_sheet::CellValue::Number(5.0)
        );
        // Deterministic.
        let again = write_xlsx(&wb, None, &WriteOptions::default()).expect("write");
        assert_eq!(again.bytes, w.bytes);
    }
}
