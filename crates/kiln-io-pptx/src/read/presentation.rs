// SPDX-License-Identifier: AGPL-3.0-or-later
//! `p:presentation`: slide and notes sizes, the master, notes master and slide lists, the
//! default text style, `firstSlideNum`, sections (`p14:sectionLst`), comment authors; the order
//! in which everything else is read.

use std::collections::{HashMap, HashSet};

use kiln_common::NodeId;
use kiln_deck::slide::{Section, SlideSize, SlideSizeKind};
use kiln_ooxml::opc::rel_type_matches;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::PartCx;
use super::slides::{Authors, NotesTheme};
use crate::common::kind::{rel, uri};
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{attr_u32, fragment, local};

/// Children of `p:presentation` the model holds.
const MODELLED: [&str; 7] = [
    "p:sldMasterIdLst",
    "p:notesMasterIdLst",
    "p:sldIdLst",
    "p:sldSz",
    "p:notesSz",
    "p:defaultTextStyle",
    "p:extLst",
];

fn slide_size(e: Option<&XmlElement>, default: SlideSize) -> Option<SlideSize> {
    let Some(e) = e else {
        return Some(default);
    };
    let size = SlideSize {
        cx: e.attr_int("cx")?,
        cy: e.attr_int("cy")?,
        kind: SlideSizeKind::from_ooxml(e.attr("type")),
    };
    size.is_valid().then_some(size)
}

/// Reads the presentation part and everything it refers to.
pub(crate) fn read(r: &mut Reader<'_>, main: &str, root: &XmlElement) {
    let cx = PartCx::new(r, main, &root.attrs, None);
    let target = |rid: Option<&str>| -> Option<String> { cx.target(rid?) };

    // ---- sizes and attributes
    match slide_size(root.child("p:sldSz"), SlideSize::WIDESCREEN) {
        Some(s) => r.pres.slide_size = s,
        None => r.warn(
            WarningCode::PartUnusable,
            Severity::Warning,
            "the slide size is outside 1 EMU to 56 inches; 16:9 is used",
            main,
            "p:sldSz",
        ),
    }
    if let Some(n) = slide_size(root.child("p:notesSz"), SlideSize::NOTES) {
        r.pres.notes_size = n;
    }
    r.pres.first_slide_number = attr_u32(root, "firstSlideNum").unwrap_or(1);
    r.extras.presentation_attrs = root
        .attrs
        .iter()
        .filter(|(k, _)| *k != "firstSlideNum")
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    for c in root.elements() {
        if !MODELLED.contains(&c.name.as_str()) {
            r.extras.presentation_xml.push(fragment(c, &root.attrs));
        }
    }
    if let Some(d) = root.child("p:defaultTextStyle") {
        r.pres.default_text_style = r.read_list_style(&cx, d);
    }

    // ---- masters and their layouts
    if let Some(list) = root.child("p:sldMasterIdLst") {
        for m in list.children_named("p:sldMasterId") {
            let Some(part) = target(m.attr("r:id")) else {
                continue;
            };
            if r.extras.part_names.values().any(|p| *p == part) {
                continue;
            }
            r.read_master(&part, attr_u32(m, "id"));
        }
    }
    // Masters must have a layout; a file without a usable one gets a blank layout.
    let mut empty = Vec::new();
    for m in &r.pres.masters {
        if m.layouts.is_empty() {
            empty.push(m.id);
        }
    }
    for id in empty {
        let layout = kiln_deck::Layout::new(r.pres.alloc_id(), kiln_deck::LayoutKind::Blank);
        if let Some(m) = r.pres.master_mut(id) {
            m.layouts.push(layout);
        }
    }
    if r.pres.masters.is_empty() {
        r.warn(
            WarningCode::PartUnusable,
            Severity::Warning,
            "the presentation has no usable slide master; a blank one is used",
            main,
            "p:sldMasterIdLst",
        );
        r.fallback_master();
    }

    // ---- notes master
    // Some producers (python-pptx) relate the notes master without listing it.
    let notes_theme: NotesTheme = root
        .child("p:notesMasterIdLst")
        .and_then(|l| l.child("p:notesMasterId"))
        .and_then(|n| target(n.attr("r:id")))
        .or_else(|| {
            cx.rels
                .all()
                .iter()
                .find(|x| !x.is_external() && rel_type_matches(&x.rel_type, rel::NOTES_MASTER))
                .and_then(|x| cx.rels.resolve(x))
        })
        .and_then(|part| r.read_notes_master(&part));

    // ---- comment authors
    let mut legacy = Authors::new();
    let mut modern = Authors::new();
    for rl in cx.rels.all() {
        if rl.is_external() {
            continue;
        }
        let Some(t) = cx.rels.resolve(rl) else {
            continue;
        };
        if rel_type_matches(&rl.rel_type, rel::COMMENT_AUTHORS) {
            legacy = r.read_authors(&t);
        } else if rel_type_matches(&rl.rel_type, rel::AUTHORS) {
            modern = r.read_authors(&t);
        }
    }

    // ---- slides: ids first, so links can jump forwards
    let mut order: Vec<(String, NodeId, Option<u32>)> = Vec::new();
    if let Some(list) = root.child("p:sldIdLst") {
        let mut seen = HashSet::new();
        for s in list.children_named("p:sldId") {
            let part = target(s.attr("r:id"));
            match part {
                Some(p) if r.pkg.has(&p) && seen.insert(p.clone()) => {
                    let id = r.pres.alloc_id();
                    r.slide_ids.insert(p.clone(), id);
                    order.push((p, id, attr_u32(s, "id")));
                }
                _ => r.warn(
                    WarningCode::SlideSkipped,
                    Severity::Warning,
                    "the slide list names a slide twice or a slide that does not exist",
                    main,
                    format!("p:sldId {}", s.attr("id").unwrap_or_default()),
                ),
            }
        }
    }
    let mut file_ids: HashMap<u32, usize> = HashMap::new();
    for (part, id, list_id) in order {
        if let Some(slide) = r.read_slide(&part, id, &notes_theme, &legacy, &modern) {
            if let Some(l) = list_id {
                r.extras.list_ids.insert(id, l);
                file_ids.insert(l, r.pres.slides.len());
            }
            r.pres.slides.push(slide);
        } else {
            r.slide_ids.remove(&part);
        }
    }

    // ---- sections
    if let Some(ext) = root
        .child("p:extLst")
        .into_iter()
        .flat_map(|l| l.children_named("p:ext"))
        .find(|e| e.attr("uri") == Some(uri::SECTIONS_EXT))
    {
        read_sections(r, main, ext, &file_ids);
    }
    if let Some(list) = root.child("p:extLst") {
        // Other extensions are kept; the sections are the model's.
        let mut kept = list.clone();
        kept.children.retain(|n| {
            n.as_element()
                .is_none_or(|e| e.attr("uri") != Some(uri::SECTIONS_EXT))
        });
        if kept.elements().next().is_some() {
            r.extras.presentation_xml.push(fragment(&kept, &root.attrs));
        }
    }
}

fn read_sections(r: &mut Reader<'_>, main: &str, ext: &XmlElement, file_ids: &HashMap<u32, usize>) {
    let Some(list) = ext.elements().find(|e| local(e) == "sectionLst") else {
        return;
    };
    let n = r.pres.slides.len();
    // Each slide's section; sections must take the slides in order.
    let mut of_slide: Vec<Option<usize>> = vec![None; n];
    let mut sections = Vec::new();
    for (i, s) in list
        .elements()
        .filter(|e| local(e) == "section")
        .enumerate()
    {
        let ids = s.elements().find(|e| local(e) == "sldIdLst");
        for sid in ids.into_iter().flat_map(|l| l.elements()) {
            if let Some(&idx) = attr_u32(sid, "id").and_then(|v| file_ids.get(&v))
                && of_slide[idx].is_none()
            {
                of_slide[idx] = Some(i);
            }
        }
        sections.push((
            s.attr("name").unwrap_or_default().to_owned(),
            s.attr("id").map(str::to_owned),
        ));
    }
    if sections.is_empty() {
        return;
    }
    // Slides no section lists stay with the section before them (the first for leading ones).
    let mut current = 0usize;
    let mut lens = vec![0usize; sections.len()];
    let mut ordered = true;
    for s in &of_slide {
        let sec = s.unwrap_or(current);
        if sec < current {
            ordered = false;
        }
        current = current.max(sec);
        lens[current] += 1;
    }
    if !ordered {
        r.warn(
            WarningCode::SectionsInconsistent,
            Severity::Warning,
            "the sections do not take the slides in order; they were dropped",
            main,
            "p14:sectionLst",
        );
        return;
    }
    for ((name, guid), len) in sections.into_iter().zip(lens) {
        let id = r.pres.alloc_id();
        if let Some(g) = guid {
            r.extras.section_guids.insert(id, g);
        }
        r.pres.sections.push(Section { id, name, len });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    #[test]
    fn slide_sizes_read_and_refuse_nonsense() {
        let e = |x: &str| {
            let x = x.replacen(
                "<p:sldSz",
                "<p:sldSz xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"",
                1,
            );
            parse_xml_str(&x).unwrap().root
        };
        let s = slide_size(
            Some(&e(
                r#"<p:sldSz cx="9144000" cy="6858000" type="screen4x3"/>"#,
            )),
            SlideSize::WIDESCREEN,
        )
        .unwrap();
        assert_eq!(s, SlideSize::STANDARD);
        assert!(
            slide_size(
                Some(&e(r#"<p:sldSz cx="0" cy="5"/>"#)),
                SlideSize::WIDESCREEN
            )
            .is_none()
        );
        assert!(slide_size(Some(&e(r#"<p:sldSz cy="5"/>"#)), SlideSize::WIDESCREEN).is_none());
        assert_eq!(slide_size(None, SlideSize::NOTES), Some(SlideSize::NOTES));
    }
}
