// SPDX-License-Identifier: AGPL-3.0-or-later
//! Set Up Slide Show as PowerPoint stores it: `p:showPr` in `presProps.xml` (show type, loop,
//! narration, animation, the slides shown, timings). Kiln reads it when a file opens and
//! writes it back into the part on save, leaving everything else of the part as it was.

use kiln_ooxml::opc::Relationship;
use kiln_ooxml::passthrough::{PassthroughPart, PassthroughSet};
use kiln_ooxml::xml::{XmlDocument, XmlElement, XmlNode, parse_xml};
use kiln_protocol::commands::deck_show::{DeckShowSettings, DeckShowType};

/// The content type of `presProps.xml`.
pub(crate) const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";
/// The relationship from the presentation to it.
pub(crate) const REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps";
/// Where a new presentation's part goes.
const PART: &str = "/ppt/presProps.xml";

const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The passthrough part that is `presProps.xml`.
pub(crate) fn part(set: &PassthroughSet) -> Option<&PassthroughPart> {
    set.parts()
        .find(|p| p.content_type.eq_ignore_ascii_case(CONTENT_TYPE))
}

fn flag(e: &XmlElement, name: &str) -> Option<bool> {
    e.attr(name).map(|v| matches!(v, "1" | "true" | "on"))
}

/// The settings a `presProps.xml` holds (the defaults when it has no `p:showPr`).
#[must_use]
pub(crate) fn read(bytes: &[u8]) -> DeckShowSettings {
    parse_xml(bytes)
        .ok()
        .and_then(|doc| doc.root.child("p:showPr").map(settings_of))
        .unwrap_or_default()
}

/// The settings a `p:showPr` holds. A custom show (`p:custShow`) is read as all slides, which
/// is what Kiln plays; the element itself is kept on save while the range is not changed.
fn settings_of(show: &XmlElement) -> DeckShowSettings {
    let mut s = DeckShowSettings {
        loop_show: flag(show, "loop").unwrap_or(false),
        no_narration: flag(show, "showNarration") == Some(false),
        no_animation: flag(show, "showAnimation") == Some(false),
        manual: flag(show, "useTimings") == Some(false),
        ..DeckShowSettings::default()
    };
    s.kind = if show.child("p:kiosk").is_some() {
        DeckShowType::Kiosk
    } else if show.child("p:browse").is_some() {
        DeckShowType::Window
    } else {
        DeckShowType::Speaker
    };
    if let Some(r) = show.child("p:sldRg") {
        s.from = r.attr("st").and_then(|v| v.parse().ok());
        s.to = r.attr("end").and_then(|v| v.parse().ok());
    }
    s
}

/// The part of the settings the file keeps: all but Use Presenter View, which PowerPoint keeps
/// outside the presentation ([MS-PPTX] has no element for it).
#[must_use]
pub(crate) fn saved(s: &DeckShowSettings) -> DeckShowSettings {
    DeckShowSettings {
        presenter_view: false,
        ..s.clone()
    }
}

/// The show type children of `p:showPr` (`EG_ShowType`).
const SHOW_TYPE: [&str; 3] = ["p:present", "p:browse", "p:kiosk"];
/// The slide list children of `p:showPr` (`EG_SlideListChoice`).
const SLIDE_LIST: [&str; 3] = ["p:sldAll", "p:sldRg", "p:custShow"];
/// The children after them.
const TAIL: [&str; 2] = ["p:penClr", "p:extLst"];

fn show_type(kind: DeckShowType) -> XmlElement {
    match kind {
        DeckShowType::Speaker => XmlElement::new("p:present"),
        DeckShowType::Window => XmlElement::new("p:browse").with_attr("showScrollbar", "0"),
        DeckShowType::Kiosk => XmlElement::new("p:kiosk"),
    }
}

fn slide_list(s: &DeckShowSettings) -> XmlElement {
    match (s.from, s.to) {
        (None, None) => XmlElement::new("p:sldAll"),
        (from, to) => XmlElement::new("p:sldRg")
            .with_attr("st", from.unwrap_or(1).max(1).to_string())
            .with_attr(
                "end",
                to.unwrap_or(u32::MAX).max(from.unwrap_or(1)).to_string(),
            ),
    }
}

/// Puts `new` in the place of the first child named in `group` (dropping the others of the
/// group), or before the first child named in `before`.
fn replace_choice(e: &mut XmlElement, group: &[&str], new: XmlElement, before: &[&str]) {
    let named = |c: &XmlNode, names: &[&str]| matches!(c, XmlNode::Element(x) if names.contains(&x.name.as_str()));
    let at = e.children.iter().position(|c| named(c, group));
    e.children.retain(|c| !named(c, group));
    let at = at
        .or_else(|| e.children.iter().position(|c| named(c, before)))
        .unwrap_or(e.children.len());
    e.children
        .insert(at.min(e.children.len()), XmlNode::Element(new));
}

/// Writes the settings into a `p:showPr`, changing only what differs from what it says: a
/// kiosk's restart time, a window's scroll bar, a custom show, the pen colour and extensions
/// stay as they were unless their setting changed.
fn write_into(show: &mut XmlElement, s: &DeckShowSettings) {
    let was = settings_of(show);
    if s.loop_show != was.loop_show {
        show.set_attr("loop", s.loop_show.then_some("1"));
    }
    if s.no_narration != was.no_narration {
        show.set_attr(
            "showNarration",
            Some(if s.no_narration { "0" } else { "1" }),
        );
    }
    if s.no_animation != was.no_animation {
        show.set_attr("showAnimation", s.no_animation.then_some("0"));
    }
    if s.manual != was.manual {
        show.set_attr("useTimings", s.manual.then_some("0"));
    }
    if s.kind != was.kind {
        let before = [&SLIDE_LIST[..], &TAIL[..]].concat();
        replace_choice(show, &SHOW_TYPE, show_type(s.kind), &before);
    }
    if (s.from, s.to) != (was.from, was.to) {
        replace_choice(show, &SLIDE_LIST, slide_list(s), &TAIL);
    }
}

/// The `p:showPr` of settings, for a part that has none.
fn show_pr(s: &DeckShowSettings) -> XmlElement {
    XmlElement::new("p:showPr")
        .with_opt_attr("loop", s.loop_show.then_some("1"))
        .with_attr("showNarration", if s.no_narration { "0" } else { "1" })
        .with_opt_attr("showAnimation", s.no_animation.then_some("0"))
        .with_opt_attr("useTimings", s.manual.then_some("0"))
        .with_child(show_type(s.kind))
        .with_child(slide_list(s))
}

/// `presProps.xml` with the settings written in (`None` when the part cannot be read).
#[must_use]
pub(crate) fn patch(bytes: &[u8], s: &DeckShowSettings) -> Option<Vec<u8>> {
    let mut doc: XmlDocument = parse_xml(bytes).ok()?;
    match doc.root.child_mut("p:showPr") {
        Some(show) => write_into(show, s),
        None => doc.root.upsert_child(show_pr(s), &["p:clrMru", "p:extLst"]),
    }
    kiln_ooxml::xml::document_to_bytes(&doc).ok()
}

/// A new `presProps.xml` with the settings.
#[must_use]
pub(crate) fn fresh(s: &DeckShowSettings) -> Option<Vec<u8>> {
    let root = XmlElement::new("p:presentationPr")
        .with_attr("xmlns:a", NS_A)
        .with_attr("xmlns:r", NS_R)
        .with_attr("xmlns:p", NS_P)
        .with_child(show_pr(s));
    let doc = XmlDocument::new(root);
    kiln_ooxml::xml::document_to_bytes(&doc).ok()
}

/// A passthrough set with the settings written into its `presProps.xml` (added, with its
/// relationship from the presentation, when there is none).
#[must_use]
pub(crate) fn apply(set: &PassthroughSet, s: &DeckShowSettings, main_part: &str) -> PassthroughSet {
    let mut out = set.clone();
    if let Some(p) = part(set) {
        if let Some(bytes) = patch(&p.bytes, s) {
            out.record_part(PassthroughPart { bytes, ..p.clone() });
        }
        return out;
    }
    let Some(bytes) = fresh(s) else {
        return out;
    };
    out.record_part(PassthroughPart {
        name: PART.to_owned(),
        content_type: CONTENT_TYPE.to_owned(),
        bytes,
        relationships: Vec::new(),
    });
    out.record_relationship(
        main_part,
        Relationship {
            id: "rIdKilnPresProps".to_owned(),
            rel_type: REL_TYPE.to_owned(),
            target: "presProps.xml".to_owned(),
            target_mode: kiln_ooxml::st::TargetMode::Internal,
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_show_pr() {
        let s = DeckShowSettings {
            kind: DeckShowType::Kiosk,
            loop_show: true,
            no_narration: true,
            no_animation: true,
            from: Some(2),
            to: Some(4),
            manual: true,
            presenter_view: false,
        };
        let bytes = fresh(&s).unwrap();
        assert_eq!(read(&bytes), s);
        let w = DeckShowSettings {
            kind: DeckShowType::Window,
            ..DeckShowSettings::default()
        };
        let patched = patch(&bytes, &w).unwrap();
        assert_eq!(read(&patched), w);
        let text = String::from_utf8(patched).unwrap();
        assert!(
            text.contains("p:browse") && text.contains("p:sldAll"),
            "{text}"
        );
        assert!(!text.contains("p:kiosk"));
    }

    const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

    fn part_with(show_pr: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr {NS}>{show_pr}<p:extLst><p:ext uri="{{X}}"/></p:extLst></p:presentationPr>"#
        )
        .into_bytes()
    }

    fn show_pr_of(bytes: &[u8]) -> XmlElement {
        parse_xml(bytes)
            .unwrap()
            .root
            .child("p:showPr")
            .unwrap()
            .clone()
    }

    #[test]
    fn a_change_keeps_what_its_setting_does_not_touch() {
        let bytes = part_with(
            r#"<p:showPr loop="1" showNarration="1"><p:kiosk restart="120000"/><p:custShow id="2"/><p:penClr><a:srgbClr val="00FF00"/></p:penClr></p:showPr>"#,
        );
        let was = read(&bytes);
        assert_eq!(
            (was.kind, was.from, was.to),
            (DeckShowType::Kiosk, None, None)
        );
        let s = DeckShowSettings {
            no_animation: true,
            ..was.clone()
        };
        let out = show_pr_of(&patch(&bytes, &s).unwrap());
        assert_eq!(out.attr("showAnimation"), Some("0"));
        assert_eq!(
            out.child("p:kiosk").and_then(|k| k.attr("restart")),
            Some("120000")
        );
        assert_eq!(
            out.child("p:custShow").and_then(|c| c.attr("id")),
            Some("2")
        );
        assert!(out.child("p:sldAll").is_none());
        assert!(out.child("p:penClr").is_some());
        assert_eq!(read(&patch(&bytes, &s).unwrap()), s);
        // Settings as they were leave the element as it was.
        assert_eq!(
            show_pr_of(&patch(&bytes, &was).unwrap()),
            show_pr_of(&bytes)
        );

        // A range replaces the custom show, in its place.
        let r = DeckShowSettings {
            from: Some(2),
            to: Some(3),
            ..was.clone()
        };
        let out = show_pr_of(&patch(&bytes, &r).unwrap());
        let names: Vec<&str> = out.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["p:kiosk", "p:sldRg", "p:penClr"]);
        assert_eq!(
            out.child("p:kiosk").and_then(|k| k.attr("restart")),
            Some("120000")
        );
    }

    #[test]
    fn a_window_keeps_its_scroll_bar() {
        let bytes = part_with(
            r#"<p:showPr showNarration="1"><p:browse showScrollbar="1"/><p:sldAll/></p:showPr>"#,
        );
        let was = read(&bytes);
        assert_eq!(was.kind, DeckShowType::Window);
        let s = DeckShowSettings {
            loop_show: true,
            manual: true,
            ..was
        };
        let out = show_pr_of(&patch(&bytes, &s).unwrap());
        assert_eq!(
            out.child("p:browse").and_then(|b| b.attr("showScrollbar")),
            Some("1")
        );
        assert_eq!(
            (out.attr("loop"), out.attr("useTimings")),
            (Some("1"), Some("0"))
        );
        // A new show type goes where the old one was.
        let k = DeckShowSettings {
            kind: DeckShowType::Speaker,
            ..s
        };
        let out = show_pr_of(&patch(&bytes, &k).unwrap());
        let names: Vec<&str> = out.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["p:present", "p:sldAll"]);
    }

    #[test]
    fn a_part_without_show_pr_gets_one_before_its_extensions() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:extLst><p:ext uri="{X}"/></p:extLst></p:presentationPr>"#;
        assert_eq!(read(xml), DeckShowSettings::default());
        let s = DeckShowSettings {
            loop_show: true,
            ..DeckShowSettings::default()
        };
        let out = String::from_utf8(patch(xml, &s).unwrap()).unwrap();
        let a = out.find("p:showPr").unwrap();
        let b = out.find("p:extLst").unwrap();
        assert!(a < b, "{out}");
        assert!(read(out.as_bytes()).loop_show);
        assert_eq!(read(b"not xml"), DeckShowSettings::default());
    }
}
