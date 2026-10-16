// SPDX-License-Identifier: AGPL-3.0-or-later
//! A worksheet part: the element tree read (or a new one) with every modelled child replaced
//! when the model no longer holds what was read, and `sheetData` written by [`super::cells`].

use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::meta::CfKind;
use kiln_sheet::{Range, Sheet};

use crate::sheet_xml::{
    WORKSHEET_ORDER, auto_filter_el, breaks_el, cols_el, conditional_formatting_el,
    data_bar_needs_x14, data_validation_el, header_footer_el, icon_set_needs_x14, page_margins_el,
    page_setup_el, print_options_el, ref_text, sheet_protection_el, sheet_view_el, x14_cf_rule_el,
};
use crate::source::SheetSource;
use crate::xmlx::{color_el, replace_children, set_attr};

const CF_EXT_URI: &str = "{78C0D931-6437-407d-A8EE-F0AAD7539E65}";
const DV_EXT_URI: &str = "{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

/// A new worksheet root.
pub(crate) fn template() -> XmlElement {
    XmlElement::new("worksheet")
        .with_attr(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        )
        .with_attr(
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        )
        .with_child(XmlElement::new("sheetData"))
}

fn set_child(root: &mut XmlElement, name: &str, el: Option<XmlElement>) {
    replace_children(root, name, el.into_iter().collect(), WORKSHEET_ORDER);
}

/// A deterministic GUID for an x14 rule link.
fn x14_guid(sheet: u32, rule: u32) -> String {
    format!(
        "{{6B1C1A5E-{:04X}-4000-8000-{:012X}}}",
        sheet & 0xFFFF,
        rule
    )
}

fn ext_el(uri: &str, child: XmlElement) -> XmlElement {
    XmlElement::new("ext")
        .with_attr("uri", uri)
        .with_attr("xmlns:x14", NS_X14)
        .with_child(child)
}

/// Removes the extension with `uri` from `extLst` (and `extLst` itself when it empties).
fn drop_ext(root: &mut XmlElement, uri: &str) {
    if let Some(l) = root.child_mut("extLst") {
        l.children.retain(|c| match c {
            XmlNode::Element(e) => !e.attr("uri").is_some_and(|u| u.eq_ignore_ascii_case(uri)),
            _ => true,
        });
        if l.elements().next().is_none() {
            root.remove_children("extLst");
        }
    }
}

fn add_ext(root: &mut XmlElement, ext: XmlElement) {
    if root.child("extLst").is_none() {
        set_child(root, "extLst", Some(XmlElement::new("extLst")));
    }
    if let Some(l) = root.child_mut("extLst") {
        l.children.push(XmlNode::Element(ext));
    }
}

/// Everything about the sheet's own XML (relationship-bearing elements are set by the caller).
pub(crate) fn patch_worksheet(
    root: &mut XmlElement,
    sheet: &Sheet,
    src: Option<&SheetSource>,
    tab_selected: bool,
    used: Option<Range>,
    issues: &mut Vec<String>,
) {
    let meta = sheet.meta();
    let read = src.map(|s| &s.meta);
    let changed = |f: &dyn Fn(&kiln_sheet::SheetMeta) -> bool| read.is_none_or(|r| !f(r));

    // sheetPr: tab colour, outline summary positions, fit to page.
    if changed(&|r| {
        r.tab_color == meta.tab_color
            && r.summary_below == meta.summary_below
            && r.summary_right == meta.summary_right
            && r.print.fit_to_page == meta.print.fit_to_page
    }) {
        let mut pr = root
            .child("sheetPr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("sheetPr"));
        let order = ["tabColor", "outlinePr", "pageSetUpPr"];
        replace_children(
            &mut pr,
            "tabColor",
            meta.tab_color
                .iter()
                .map(|c| color_el("tabColor", c))
                .collect(),
            &order,
        );
        let mut outline = pr
            .child("outlinePr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("outlinePr"));
        set_attr(
            &mut outline,
            "summaryBelow",
            (!meta.summary_below).then(|| "0".to_owned()),
        );
        set_attr(
            &mut outline,
            "summaryRight",
            (!meta.summary_right).then(|| "0".to_owned()),
        );
        let keep_outline = !outline.attrs.is_empty() || pr.child("outlinePr").is_some();
        replace_children(
            &mut pr,
            "outlinePr",
            keep_outline.then_some(outline).into_iter().collect(),
            &order,
        );
        let mut setup = pr
            .child("pageSetUpPr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("pageSetUpPr"));
        set_attr(
            &mut setup,
            "fitToPage",
            meta.print.fit_to_page.then(|| "1".to_owned()),
        );
        let keep_setup = !setup.attrs.is_empty() || pr.child("pageSetUpPr").is_some();
        replace_children(
            &mut pr,
            "pageSetUpPr",
            keep_setup.then_some(setup).into_iter().collect(),
            &order,
        );
        let empty = pr.attrs.is_empty() && pr.children.is_empty();
        set_child(root, "sheetPr", (!empty).then_some(pr));
    }

    // dimension
    let dim = used.map_or_else(|| "A1".to_owned(), |u| ref_text(&u));
    set_child(
        root,
        "dimension",
        Some(XmlElement::new("dimension").with_attr("ref", dim)),
    );

    // sheetViews
    let view_same = read.is_some_and(|r| r.view == meta.view);
    let had_views = root.child("sheetViews").is_some();
    if !view_same || !had_views {
        let mut views = root
            .child("sheetViews")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("sheetViews"));
        let first = views
            .children
            .iter()
            .position(|c| matches!(c, XmlNode::Element(e) if e.name == "sheetView"));
        let el = sheet_view_el(&meta.view, tab_selected);
        match first {
            Some(i) => views.children[i] = XmlNode::Element(el),
            None => views.children.push(XmlNode::Element(el)),
        }
        set_child(root, "sheetViews", Some(views));
    } else if let Some(v) = root
        .child_mut("sheetViews")
        .and_then(|v| v.child_mut("sheetView"))
    {
        let selected = v
            .attr("tabSelected")
            .is_some_and(|s| s == "1" || s == "true");
        if selected != tab_selected {
            set_attr(v, "tabSelected", tab_selected.then(|| "1".to_owned()));
        }
    }

    // sheetFormatPr
    if changed(&|r| {
        r.default_row_height == meta.default_row_height
            && r.default_col_width == meta.default_col_width
    }) || root.child("sheetFormatPr").is_none()
    {
        let mut f = root
            .child("sheetFormatPr")
            .cloned()
            .unwrap_or_else(|| XmlElement::new("sheetFormatPr"));
        set_attr(
            &mut f,
            "defaultColWidth",
            meta.default_col_width.map(crate::text::fmt_num),
        );
        f.attrs.set(
            "defaultRowHeight",
            crate::text::fmt_num(meta.default_row_height),
        );
        set_child(root, "sheetFormatPr", Some(f));
    }

    // cols
    let layout = sheet.layout();
    if src.is_none_or(|s| s.layout.cols != layout.cols) {
        set_child(root, "cols", cols_el(&layout.cols));
    }

    // sheetProtection
    if changed(&|r| r.protection == meta.protection) {
        set_child(
            root,
            "sheetProtection",
            meta.protection.as_ref().map(sheet_protection_el),
        );
    }

    // autoFilter
    if changed(&|r| r.auto_filter == meta.auto_filter) {
        set_child(
            root,
            "autoFilter",
            meta.auto_filter
                .as_ref()
                .map(|a| auto_filter_el(&a.range, &a.columns, a.sort.as_ref())),
        );
    }

    // mergeCells
    if changed(&|r| r.merges == meta.merges) {
        let el = (!meta.merges.is_empty()).then(|| {
            let mut m = XmlElement::new("mergeCells").with_attr("count", meta.merges.len());
            for r in &meta.merges {
                m.children.push(XmlNode::Element(
                    XmlElement::new("mergeCell").with_attr("ref", ref_text(r)),
                ));
            }
            m
        });
        set_child(root, "mergeCells", el);
    }

    // Conditional formatting (2007 elements + x14 extension).
    if changed(&|r| r.cf == meta.cf) {
        drop_ext(root, CF_EXT_URI);
        let sheet_no = sheet.id().0;
        let mut blocks = Vec::new();
        let mut x14_blocks = Vec::new();
        for cf in &meta.cf {
            let mut ids = Vec::new();
            for rule in &cf.rules {
                let needs = match &rule.kind {
                    CfKind::DataBar(b) => data_bar_needs_x14(b),
                    CfKind::IconSet(s) => icon_set_needs_x14(s),
                    _ => false,
                };
                if needs {
                    let id = x14_guid(sheet_no, rule.id);
                    if let Some(x) = x14_cf_rule_el(rule, &id) {
                        let mut block = XmlElement::new("x14:conditionalFormatting")
                            .with_attr("xmlns:xm", NS_XM)
                            .with_child(x);
                        block.children.push(XmlNode::Element(
                            XmlElement::new("xm:sqref")
                                .with_text(crate::sheet_xml::sqref_text(&cf.ranges)),
                        ));
                        x14_blocks.push(block);
                        ids.push(Some(id));
                        continue;
                    }
                }
                ids.push(None);
            }
            blocks.push(conditional_formatting_el(cf, &ids));
        }
        replace_children(root, "conditionalFormatting", blocks, WORKSHEET_ORDER);
        if !x14_blocks.is_empty() {
            let mut list = XmlElement::new("x14:conditionalFormattings");
            list.children
                .extend(x14_blocks.into_iter().map(XmlNode::Element));
            add_ext(root, ext_el(CF_EXT_URI, list));
        }
    }

    // Data validation.
    if changed(&|r| r.validations == meta.validations) {
        drop_ext(root, DV_EXT_URI);
        let el = (!meta.validations.is_empty()).then(|| {
            let mut d =
                XmlElement::new("dataValidations").with_attr("count", meta.validations.len());
            if let Some(orig) = src.and_then(|s| s.dv_xml.as_ref()) {
                for (k, v) in orig.attrs.iter() {
                    if k != "count" {
                        d.attrs.set(k, v);
                    }
                }
            }
            for v in &meta.validations {
                d.children.push(XmlNode::Element(data_validation_el(v)));
            }
            d
        });
        set_child(root, "dataValidations", el);
    }

    // Page setup.
    if changed(&|r| r.print == meta.print) {
        let p = &meta.print;
        set_child(root, "printOptions", print_options_el(p));
        set_child(root, "pageMargins", Some(page_margins_el(&p.margins)));
        let base = root.child("pageSetup").cloned();
        set_child(root, "pageSetup", page_setup_el(p, base.as_ref()));
        set_child(root, "headerFooter", header_footer_el(&p.header_footer));
        set_child(
            root,
            "rowBreaks",
            breaks_el("rowBreaks", &p.row_breaks, true),
        );
        set_child(
            root,
            "colBreaks",
            breaks_el("colBreaks", &p.col_breaks, false),
        );
    } else if src.is_none() {
        set_child(
            root,
            "pageMargins",
            Some(page_margins_el(&meta.print.margins)),
        );
    }
    if meta.drawings.len() > src.map_or(0, |s| s.meta.drawings.len())
        && src.is_none_or(|s| s.drawing.is_none())
    {
        issues.push(format!(
            "{}: drawing objects added in Kiln are not written yet",
            sheet.name()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_added_and_dropped() {
        let mut root = template();
        add_ext(
            &mut root,
            ext_el(CF_EXT_URI, XmlElement::new("x14:conditionalFormattings")),
        );
        assert!(root.child("extLst").is_some());
        drop_ext(&mut root, CF_EXT_URI);
        assert!(root.child("extLst").is_none());
        assert!(x14_guid(1, 2).starts_with('{'));
    }
}
