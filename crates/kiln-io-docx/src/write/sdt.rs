// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content control properties (`w:sdtPr`). A control read from a file keeps its `w:sdtPr`
//! verbatim; when the model's view of it changed (a check box ticked, a tag renamed, a drop-down
//! list edited, the placeholder replaced) the kept element is patched, so everything the model
//! does not hold (data bindings, appearance, colours, placeholders) survives. Controls created in
//! the editor get a `w:sdtPr` built from the model.

use kiln_doc::SdtInfo;
use kiln_doc::sdt::SdtListItem;
use kiln_doc::st::SdtKind;
use kiln_ooxml::xml::{Attrs, XmlElement};

use super::xml::{El, val};
use crate::common::units::{parse_decimal, parse_on_off};
use crate::common::xml::{attr, attr_of, fragment_xml, parse_fragment};

const KIND_ELEMENTS: [(&str, SdtKind); 15] = [
    ("w:richText", SdtKind::RichText),
    ("w:text", SdtKind::Text),
    ("w:date", SdtKind::Date),
    ("w:dropDownList", SdtKind::DropDownList),
    ("w:comboBox", SdtKind::ComboBox),
    ("w14:checkbox", SdtKind::Checkbox),
    ("w:picture", SdtKind::Picture),
    ("w:docPartObj", SdtKind::DocPartObj),
    ("w:docPartList", SdtKind::DocPartList),
    ("w:group", SdtKind::Group),
    ("w:bibliography", SdtKind::Bibliography),
    ("w:citation", SdtKind::Citation),
    ("w:equation", SdtKind::Equation),
    ("w15:repeatingSection", SdtKind::RepeatingSection),
    ("w15:repeatingSectionItem", SdtKind::RepeatingSectionItem),
];

/// The values the reader derives from a `w:sdtPr` (the model fields besides the raw XML).
#[derive(Debug, PartialEq, Eq)]
struct Derived {
    id: Option<i64>,
    tag: Option<String>,
    alias: Option<String>,
    lock: Option<String>,
    showing_placeholder: bool,
    doc_part_gallery: Option<String>,
    checked: Option<bool>,
    list_items: Vec<SdtListItem>,
    date_format: Option<String>,
}

fn child_val(pr: &XmlElement, name: &str) -> Option<String> {
    attr_of(pr.child(name), "w:val").map(str::to_owned)
}

fn marker(pr: &XmlElement) -> Option<(&XmlElement, SdtKind)> {
    pr.elements().find_map(|c| {
        KIND_ELEMENTS
            .iter()
            .find(|(n, _)| *n == c.name)
            .map(|(_, k)| (c, *k))
    })
}

fn derive(pr: &XmlElement) -> Derived {
    let mut d = Derived {
        id: parse_decimal(attr_of(pr.child("w:id"), "w:val")),
        tag: child_val(pr, "w:tag"),
        alias: child_val(pr, "w:alias"),
        lock: child_val(pr, "w:lock"),
        showing_placeholder: pr
            .child("w:showingPlcHdr")
            .is_some_and(|e| parse_on_off(attr(e, "w:val"), true)),
        doc_part_gallery: None,
        checked: None,
        list_items: Vec::new(),
        date_format: None,
    };
    if let Some((m, kind)) = marker(pr) {
        match kind {
            SdtKind::DocPartObj | SdtKind::DocPartList => {
                d.doc_part_gallery =
                    attr_of(m.child("w:docPartGallery"), "w:val").map(str::to_owned);
            }
            SdtKind::Checkbox => {
                d.checked = Some(parse_on_off(
                    attr_of(m.child("w14:checked"), "w14:val"),
                    false,
                ));
            }
            SdtKind::DropDownList | SdtKind::ComboBox => {
                d.list_items = m
                    .children_named("w:listItem")
                    .map(|li| {
                        let value = attr(li, "w:value").unwrap_or_default().to_owned();
                        SdtListItem {
                            display_text: attr(li, "w:displayText")
                                .map_or_else(|| value.clone(), str::to_owned),
                            value,
                        }
                    })
                    .collect();
            }
            SdtKind::Date => {
                d.date_format = attr_of(m.child("w:dateFormat"), "w:val").map(str::to_owned);
            }
            _ => {}
        }
    }
    d
}

fn wanted(info: &SdtInfo) -> Derived {
    Derived {
        id: info.id,
        tag: info.tag.clone(),
        alias: info.alias.clone(),
        lock: info.lock.clone(),
        showing_placeholder: info.showing_placeholder,
        doc_part_gallery: info.doc_part_gallery.clone(),
        checked: info.checked,
        list_items: info.list_items.clone(),
        date_format: info.date_format.clone(),
    }
}

/// CT_SdtPr's leading children in schema order (the type marker follows).
const ORDER: [&str; 10] = [
    "w:rPr",
    "w:alias",
    "w:tag",
    "w:id",
    "w:lock",
    "w:placeholder",
    "w:temporary",
    "w:showingPlcHdr",
    "w:dataBinding",
    "w:label",
];

fn set_val_child(pr: &mut XmlElement, name: &str, value: Option<&str>) {
    match value {
        None => {
            pr.remove_children(name);
        }
        Some(v) => {
            if let Some(c) = pr.child_mut(name) {
                c.set_attr("w:val", Some(v));
            } else {
                let el = XmlElement::new(name).with_attr("w:val", v);
                let rank = ORDER.iter().position(|n| *n == name).unwrap_or(ORDER.len());
                let before: Vec<&str> = ORDER[rank + 1..].to_vec();
                let mut before_all = before;
                before_all.extend(KIND_ELEMENTS.iter().map(|(n, _)| *n));
                pr.upsert_child(el, &before_all);
            }
        }
    }
}

fn patch(pr: &mut XmlElement, info: &SdtInfo, d: &Derived) {
    if d.alias != info.alias {
        set_val_child(pr, "w:alias", info.alias.as_deref());
    }
    if d.tag != info.tag {
        set_val_child(pr, "w:tag", info.tag.as_deref());
    }
    if d.id != info.id {
        let id = info.id.map(|i| i.to_string());
        set_val_child(pr, "w:id", id.as_deref());
    }
    if d.lock != info.lock {
        set_val_child(pr, "w:lock", info.lock.as_deref());
    }
    if d.showing_placeholder != info.showing_placeholder {
        if info.showing_placeholder {
            let before: Vec<&str> = ORDER[8..]
                .iter()
                .copied()
                .chain(KIND_ELEMENTS.iter().map(|(n, _)| *n))
                .collect();
            pr.remove_children("w:showingPlcHdr");
            pr.upsert_child(XmlElement::new("w:showingPlcHdr"), &before);
        } else {
            pr.remove_children("w:showingPlcHdr");
        }
    }
    let kind = marker(pr).map(|(_, k)| k);
    let Some(m) = pr
        .children
        .iter_mut()
        .filter_map(|c| c.as_element_mut())
        .find(|c| KIND_ELEMENTS.iter().any(|(n, _)| *n == c.name))
    else {
        return;
    };
    match kind {
        Some(SdtKind::Checkbox) if d.checked != info.checked => {
            let v = if info.checked.unwrap_or(false) {
                "1"
            } else {
                "0"
            };
            if let Some(c) = m.child_mut("w14:checked") {
                c.set_attr("w14:val", Some(v));
            } else {
                m.upsert_child(
                    XmlElement::new("w14:checked").with_attr("w14:val", v),
                    &["w14:checkedState", "w14:uncheckedState"],
                );
            }
        }
        Some(SdtKind::DropDownList | SdtKind::ComboBox) if d.list_items != info.list_items => {
            m.remove_children("w:listItem");
            for li in &info.list_items {
                m.children.push(
                    XmlElement::new("w:listItem")
                        .with_attr("w:displayText", li.display_text.as_str())
                        .with_attr("w:value", li.value.as_str())
                        .into(),
                );
            }
        }
        Some(SdtKind::Date) if d.date_format != info.date_format => match &info.date_format {
            Some(f) => {
                if let Some(c) = m.child_mut("w:dateFormat") {
                    c.set_attr("w:val", Some(f.as_str()));
                } else {
                    m.upsert_child(
                        XmlElement::new("w:dateFormat").with_attr("w:val", f.as_str()),
                        &["w:lid", "w:storeMappedDataAs", "w:calendar"],
                    );
                }
            }
            None => {
                m.remove_children("w:dateFormat");
            }
        },
        Some(SdtKind::DocPartObj | SdtKind::DocPartList)
            if d.doc_part_gallery != info.doc_part_gallery =>
        {
            match &info.doc_part_gallery {
                Some(g) => {
                    if let Some(c) = m.child_mut("w:docPartGallery") {
                        c.set_attr("w:val", Some(g.as_str()));
                    } else {
                        m.upsert_child(
                            XmlElement::new("w:docPartGallery").with_attr("w:val", g.as_str()),
                            &["w:docPartCategory", "w:docPartUnique"],
                        );
                    }
                }
                None => {
                    m.remove_children("w:docPartGallery");
                }
            }
        }
        _ => {}
    }
}

/// A `w:sdtPr` built from the model.
fn build(info: &SdtInfo) -> String {
    let mut inner = String::new();
    if let Some(a) = &info.alias {
        inner.push_str(&val("w:alias", a));
    }
    if let Some(t) = &info.tag {
        inner.push_str(&val("w:tag", t));
    }
    if let Some(i) = info.id {
        inner.push_str(&val("w:id", i));
    }
    if let Some(l) = &info.lock {
        inner.push_str(&val("w:lock", l));
    }
    if info.showing_placeholder {
        inner.push_str("<w:showingPlcHdr/>");
    }
    let marker = match info.kind {
        SdtKind::Checkbox => {
            let on = info.checked.unwrap_or(false);
            format!(
                "<w14:checkbox><w14:checked w14:val=\"{}\"/><w14:checkedState w14:val=\"2612\" w14:font=\"MS Gothic\"/><w14:uncheckedState w14:val=\"2610\" w14:font=\"MS Gothic\"/></w14:checkbox>",
                if on { "1" } else { "0" }
            )
        }
        SdtKind::DropDownList | SdtKind::ComboBox => {
            let items: String = info
                .list_items
                .iter()
                .map(|li| {
                    El::new("w:listItem")
                        .attr("w:displayText", &li.display_text)
                        .attr("w:value", &li.value)
                        .empty()
                })
                .collect();
            let name = if info.kind == SdtKind::ComboBox {
                "w:comboBox"
            } else {
                "w:dropDownList"
            };
            El::new(name).wrap(&items)
        }
        SdtKind::Date => El::new("w:date").wrap(
            &info
                .date_format
                .as_deref()
                .map(|f| val("w:dateFormat", f))
                .unwrap_or_default(),
        ),
        SdtKind::DocPartObj | SdtKind::DocPartList => {
            let name = if info.kind == SdtKind::DocPartObj {
                "w:docPartObj"
            } else {
                "w:docPartList"
            };
            let gallery = info
                .doc_part_gallery
                .as_deref()
                .map(|g| val("w:docPartGallery", g))
                .unwrap_or_default();
            El::new(name).wrap(&format!("{gallery}<w:docPartUnique/>"))
        }
        SdtKind::Text => "<w:text/>".to_owned(),
        SdtKind::Picture => "<w:picture/>".to_owned(),
        SdtKind::Group => "<w:group/>".to_owned(),
        SdtKind::Bibliography => "<w:bibliography/>".to_owned(),
        SdtKind::Citation => "<w:citation/>".to_owned(),
        SdtKind::Equation => "<w:equation/>".to_owned(),
        SdtKind::RepeatingSection => "<w15:repeatingSection/>".to_owned(),
        SdtKind::RepeatingSectionItem => "<w15:repeatingSectionItem/>".to_owned(),
        SdtKind::RichText | SdtKind::Other => String::new(),
    };
    inner.push_str(&marker);
    El::new("w:sdtPr").wrap(&inner)
}

/// The `w:sdtPr` of a content control: the kept element (patched where the model changed), or
/// one built from the model.
pub(crate) fn sdt_pr_xml(info: &SdtInfo) -> String {
    if info.pr_xml.trim().is_empty() {
        return build(info);
    }
    let Ok(mut pr) = parse_fragment(&info.pr_xml) else {
        return build(info);
    };
    let d = derive(&pr);
    let want = wanted(info);
    if d == want {
        return info.pr_xml.clone();
    }
    patch(&mut pr, info, &d);
    let scope: Attrs = pr.attrs.clone();
    fragment_xml(&pr, &scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_of(pr: &str) -> SdtInfo {
        let el = parse_fragment(pr).unwrap();
        let d = derive(&el);
        SdtInfo {
            kind: marker(&el).map_or(SdtKind::RichText, |(_, k)| k),
            id: d.id,
            tag: d.tag,
            alias: d.alias,
            doc_part_gallery: d.doc_part_gallery,
            checked: d.checked,
            showing_placeholder: d.showing_placeholder,
            list_items: d.list_items,
            date_format: d.date_format,
            lock: d.lock,
            pr_xml: pr.to_owned(),
            end_pr_xml: None,
        }
    }

    #[test]
    fn unchanged_controls_keep_their_properties_verbatim() {
        let pr = r#"<w:sdtPr><w:alias w:val="Done"/><w:tag w:val="t"/><w:id w:val="-1"/><w14:checkbox><w14:checked w14:val="0"/><w14:checkedState w14:val="2612" w14:font="MS Gothic"/></w14:checkbox></w:sdtPr>"#;
        let info = info_of(pr);
        assert_eq!(sdt_pr_xml(&info), pr);
    }

    #[test]
    fn changed_controls_are_patched() {
        let pr = r#"<w:sdtPr><w:alias w:val="Done"/><w:id w:val="5"/><w14:checkbox><w14:checked w14:val="0"/><w14:checkedState w14:val="2612" w14:font="MS Gothic"/></w14:checkbox></w:sdtPr>"#;
        let mut info = info_of(pr);
        info.checked = Some(true);
        info.tag = Some("x".into());
        info.alias = None;
        let out = sdt_pr_xml(&info);
        assert!(out.contains(r#"<w14:checked w14:val="1"/>"#), "{out}");
        assert!(
            out.contains(r#"<w:tag w:val="x"/><w:id w:val="5"/>"#),
            "{out}"
        );
        assert!(!out.contains("w:alias"), "{out}");
        assert_eq!(info_of(&out).checked, Some(true));
    }

    #[test]
    fn lists_and_dates_are_patched_and_built() {
        let pr = r#"<w:sdtPr><w:dropDownList><w:listItem w:displayText="A" w:value="a"/></w:dropDownList></w:sdtPr>"#;
        let mut info = info_of(pr);
        info.list_items.push(SdtListItem {
            display_text: "B".into(),
            value: "b".into(),
        });
        let out = sdt_pr_xml(&info);
        assert_eq!(info_of(&out).list_items.len(), 2);
        let built = sdt_pr_xml(&SdtInfo {
            kind: SdtKind::Date,
            date_format: Some("d/M/yyyy".into()),
            tag: Some("when".into()),
            ..SdtInfo::default()
        });
        assert_eq!(
            built,
            r#"<w:sdtPr><w:tag w:val="when"/><w:date><w:dateFormat w:val="d/M/yyyy"/></w:date></w:sdtPr>"#
        );
        let parsed = info_of(&built);
        assert_eq!(parsed.date_format.as_deref(), Some("d/M/yyyy"));
    }
}
