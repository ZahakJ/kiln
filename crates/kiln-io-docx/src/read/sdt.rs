// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content controls (`w:sdt`, ECMA-376 Part 1 §17.5.2): `w:sdtPr` → [`SdtInfo`].

use kiln_doc::SdtInfo;
use kiln_doc::sdt::SdtListItem;
use kiln_doc::st::SdtKind;
use kiln_ooxml::xml::XmlElement;

use super::props::Raw;
use crate::common::units::{parse_decimal, parse_on_off};
use crate::common::xml::{attr, attr_of, val};

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

/// Children of `w:sdtPr` that are not a type marker.
const COMMON: [&str; 17] = [
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
    "w:tabIndex",
    "w15:color",
    "w15:appearance",
    "w15:webExtensionLinked",
    "w15:webExtensionCreated",
    "w15:dataBinding",
    "w16sdtdh:storeItemChecksum",
];

/// Reads the properties of a `w:sdt` (any level).
pub(crate) fn parse_sdt_info(sdt: &XmlElement, raw: Raw<'_>) -> SdtInfo {
    let pr = sdt.child("w:sdtPr");
    let end_pr = sdt.child("w:sdtEndPr");
    let mut kind = SdtKind::RichText;
    let mut marker = None;
    for c in pr.into_iter().flat_map(XmlElement::elements) {
        if let Some((_, k)) = KIND_ELEMENTS.iter().find(|(n, _)| *n == c.name) {
            kind = *k;
            marker = Some(c);
            break;
        }
        if !COMMON.contains(&c.name.as_str()) {
            kind = SdtKind::Other;
        }
    }
    let child_val = |name: &str| val(pr.and_then(|p| p.child(name))).map(str::to_owned);
    let mut info = SdtInfo {
        kind,
        id: parse_decimal(val(pr.and_then(|p| p.child("w:id")))),
        tag: child_val("w:tag"),
        alias: child_val("w:alias"),
        lock: child_val("w:lock"),
        showing_placeholder: pr
            .and_then(|p| p.child("w:showingPlcHdr"))
            .is_some_and(|e| parse_on_off(val(Some(e)), true)),
        pr_xml: pr.map_or_else(|| "<w:sdtPr/>".to_owned(), |p| raw.xml(p)),
        end_pr_xml: end_pr.map(|e| raw.xml(e)),
        ..SdtInfo::default()
    };
    if let Some(marker) = marker {
        match kind {
            SdtKind::DocPartObj | SdtKind::DocPartList => {
                info.doc_part_gallery = val(marker.child("w:docPartGallery")).map(str::to_owned);
            }
            SdtKind::Checkbox => {
                info.checked = Some(parse_on_off(
                    attr_of(marker.child("w14:checked"), "w14:val"),
                    false,
                ));
            }
            SdtKind::DropDownList | SdtKind::ComboBox => {
                info.list_items = marker
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
                info.date_format = val(marker.child("w:dateFormat")).map(str::to_owned);
            }
            _ => {}
        }
    }
    info
}
