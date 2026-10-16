// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fields that need the layout (Update Field, F9). Fields show their cached results until they
//! are updated on command; these functions compute what an update writes: page-dependent and
//! cross-reference results (PAGE, NUMPAGES, SECTIONPAGES, PAGEREF, REF, DATE, TIME) and the
//! entries of a table of contents with their page numbers. Headers and footers are not listed:
//! their PAGE fields are live on every page.

use kiln_common::NodeId;
use kiln_doc::fields::{FieldContext, FieldKind, LevelRange, evaluate_field};
use kiln_doc::{Document, InlineObject};

use crate::engine::DocLayout;

/// The new result of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldUpdate {
    /// The paragraph holding the field.
    pub para: NodeId,
    /// Char offset of the field object.
    pub offset: usize,
    /// The new result.
    pub result: String,
}

/// An entry of a table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// The heading paragraph.
    pub para: NodeId,
    /// Level 1–9.
    pub level: u8,
    /// The heading's text.
    pub text: String,
    /// The page number as displayed.
    pub page: String,
}

/// The displayed page number of the page holding a paragraph's start.
#[must_use]
pub fn page_of(layout: &DocLayout, para: NodeId) -> Option<(i64, String)> {
    let r = layout.lines_of(para).first()?;
    let p = layout.pages().get(r.page as usize)?;
    Some((p.number, p.number_text.clone()))
}

/// New results of the body's fields that depend on the layout or the clock.
#[must_use]
pub fn field_updates(
    layout: &DocLayout,
    doc: &Document,
    now: Option<kiln_doc::fields::FieldDateTime>,
) -> Vec<FieldUpdate> {
    let total = i64::try_from(layout.pages().len()).ok();
    let bookmarks = doc.bookmarks();
    let bookmark_page = |name: &str| -> Option<i64> {
        let b = bookmarks.iter().find(|b| b.name == name)?;
        page_of(layout, b.para).map(|p| p.0)
    };
    let bookmark_text = |name: &str| doc.bookmark_text(name);
    let mut out = Vec::new();
    for p in doc.body_paragraphs() {
        let page = page_of(layout, p.id);
        let section_pages = layout.lines_of(p.id).first().and_then(|r| {
            let s = layout.pages().get(r.page as usize)?.section;
            i64::try_from(layout.pages().iter().filter(|x| x.section == s).count()).ok()
        });
        for (offset, o) in p.content.objects_with_offsets() {
            let InlineObject::Field(f) = o else { continue };
            if f.locked {
                continue;
            }
            let parsed = f.parsed();
            let result = match &parsed.kind {
                FieldKind::Toc(_) | FieldKind::Seq(_) | FieldKind::Unknown => None,
                _ => evaluate_field(
                    &parsed,
                    &FieldContext {
                        page: page.as_ref().map(|p| p.0),
                        num_pages: total,
                        section_pages,
                        section: None,
                        now,
                        bookmark_text: Some(&bookmark_text),
                        bookmark_page: Some(&bookmark_page),
                    },
                ),
            };
            if let Some(result) = result
                && result != f.result
            {
                out.push(FieldUpdate {
                    para: p.id,
                    offset,
                    result,
                });
            }
        }
    }
    out
}

/// The entries of a table of contents over `levels` (TOC `\o "1-3"`; all heading levels when
/// `None`), with page numbers from the layout.
#[must_use]
pub fn toc_entries(
    layout: &DocLayout,
    doc: &Document,
    levels: Option<LevelRange>,
) -> Vec<TocEntry> {
    let (lo, hi) = levels.map_or((1, 9), |l| (l.from, l.to));
    doc.outline()
        .into_iter()
        .filter(|e| {
            let lvl = u32::from(e.level) + 1;
            lvl >= lo && lvl <= hi
        })
        .map(|e| TocEntry {
            para: e.para,
            level: e.level + 1,
            text: e.text,
            page: page_of(layout, e.para).map(|p| p.1).unwrap_or_default(),
        })
        .collect()
}
