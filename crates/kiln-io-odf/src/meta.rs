// SPDX-License-Identifier: AGPL-3.0-or-later
//! `meta.xml`: document properties.
//!
//! | ODF | Model (`kiln_doc::DocProperties`, the core/app/custom properties of OOXML) |
//! |---|---|
//! | `dc:title`, `dc:subject`, `dc:description`, `dc:language` | title, subject, description, language |
//! | `meta:keyword` (one per keyword) | keywords, joined with `; ` as Word writes them |
//! | `meta:initial-creator`, `dc:creator` | creator, last modified by |
//! | `meta:creation-date`, `dc:date` | created, modified |
//! | `meta:editing-cycles`, `meta:editing-duration` | revision, total editing time (minutes) |
//! | `meta:template/@xlink:title` | template |
//! | `meta:user-defined` | custom properties (text, number, date, boolean); LibreOffice's `OOXMLCorePropertyCategory`/`…ContentStatus` and `Company`/`Manager` map to their own fields |

use kiln_doc::settings::{CustomProperty, PropertyValue};
use kiln_doc::DocProperties;

use crate::OdfError;
use crate::units::{duration_days, duration_str};
use crate::xml::{XmlOut, all_text, kid, parse_part};

const CATEGORY: &str = "OOXMLCorePropertyCategory";
const CONTENT_STATUS: &str = "OOXMLCorePropertyContentStatus";

/// Reads `meta.xml` (absent or empty input gives empty properties).
///
/// # Errors
///
/// [`OdfError::Xml`] when the part is not well-formed.
pub fn read_meta(bytes: Option<&[u8]>) -> Result<DocProperties, OdfError> {
    let mut p = DocProperties::default();
    let Some(bytes) = bytes else {
        return Ok(p);
    };
    let root = parse_part("meta.xml", bytes)?;
    let Some(meta) = kid(&root, "office:meta") else {
        return Ok(p);
    };
    let mut keywords: Vec<String> = Vec::new();
    for e in meta.elements() {
        let text = || Some(all_text(e).trim().to_owned()).filter(|t| !t.is_empty());
        match e.name.as_str() {
            "dc:title" => p.title = text(),
            "dc:subject" => p.subject = text(),
            "dc:description" => p.description = text(),
            "dc:language" => p.language = text(),
            "dc:creator" => p.last_modified_by = text(),
            "meta:initial-creator" => p.creator = text(),
            "meta:creation-date" => p.created = text(),
            "dc:date" => p.modified = text(),
            "meta:editing-cycles" => p.revision = text().and_then(|t| t.parse().ok()),
            "meta:editing-duration" => {
                p.total_time = text()
                    .and_then(|t| duration_days(&t))
                    .map(|d| d * 1440.0)
                    .filter(|m| m.is_finite() && *m >= 0.0 && *m < f64::from(u32::MAX))
                    .map(|m| {
                        // In range by the filter above.
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let v = m.round() as u32;
                        v
                    });
            }
            "meta:keyword" => keywords.extend(text()),
            "meta:template" => {
                p.template = e
                    .attr("xlink:title")
                    .map(str::to_owned)
                    .filter(|t| !t.is_empty());
            }
            "meta:user-defined" => {
                let Some(name) = e.attr("meta:name") else {
                    continue;
                };
                let raw = all_text(e);
                match name {
                    CATEGORY => p.category = Some(raw).filter(|t| !t.is_empty()),
                    CONTENT_STATUS => p.content_status = Some(raw).filter(|t| !t.is_empty()),
                    "Company" if p.company.is_none() => p.company = Some(raw),
                    "Manager" if p.manager.is_none() => p.manager = Some(raw),
                    _ => {
                        let value = match e.attr("meta:value-type").unwrap_or("string") {
                            "float" | "percentage" | "currency" => match raw.trim().parse::<i64>() {
                                Ok(i) if !raw.contains(['.', 'e', 'E']) => {
                                    PropertyValue::Integer(i)
                                }
                                _ => PropertyValue::Real(raw.trim().to_owned()),
                            },
                            "boolean" => PropertyValue::Bool(raw.trim() == "true"),
                            "date" => PropertyValue::Date(raw.trim().to_owned()),
                            _ => PropertyValue::Text(raw),
                        };
                        p.custom.push(CustomProperty {
                            name: name.to_owned(),
                            value,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    if !keywords.is_empty() {
        p.keywords = Some(keywords.join("; "));
    }
    Ok(p)
}

/// Writes `meta.xml`. `generator` names the producing application.
#[must_use]
pub fn write_meta(p: &DocProperties, generator: &str) -> Vec<u8> {
    let mut w = XmlOut::new();
    w.root("office:document-meta", &["office", "meta", "dc", "xlink"])
        .attr("office:version", "1.3");
    w.start("office:meta");
    w.text_element("meta:generator", generator);
    let mut el = |name: &str, v: &Option<String>| {
        if let Some(v) = v {
            w.text_element(name, v);
        }
    };
    el("dc:title", &p.title);
    el("dc:subject", &p.subject);
    el("dc:description", &p.description);
    el("meta:initial-creator", &p.creator);
    el("dc:creator", &p.last_modified_by);
    el("meta:creation-date", &p.created);
    el("dc:date", &p.modified);
    el("dc:language", &p.language);
    if let Some(k) = &p.keywords {
        for kw in k.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            w.text_element("meta:keyword", kw);
        }
    }
    if let Some(r) = p.revision {
        w.text_element("meta:editing-cycles", &r.to_string());
    }
    if let Some(t) = p.total_time {
        w.text_element(
            "meta:editing-duration",
            &duration_str(f64::from(t) / 1440.0),
        );
    }
    if let Some(t) = &p.template {
        w.start("meta:template")
            .attr("xlink:type", "simple")
            .attr("xlink:actuate", "onRequest")
            .attr("xlink:title", t)
            .attr("xlink:href", "")
            .end();
    }
    let mut user = |name: &str, kind: &str, value: &str| {
        w.start("meta:user-defined")
            .attr("meta:name", name)
            .attr("meta:value-type", kind)
            .text(value)
            .end();
    };
    if let Some(c) = &p.category {
        user(CATEGORY, "string", c);
    }
    if let Some(c) = &p.content_status {
        user(CONTENT_STATUS, "string", c);
    }
    if let Some(c) = &p.company {
        user("Company", "string", c);
    }
    if let Some(c) = &p.manager {
        user("Manager", "string", c);
    }
    for c in &p.custom {
        match &c.value {
            PropertyValue::Text(t) => user(&c.name, "string", t),
            PropertyValue::Integer(i) => user(&c.name, "float", &i.to_string()),
            PropertyValue::Real(r) => user(&c.name, "float", r),
            PropertyValue::Bool(b) => user(&c.name, "boolean", if *b { "true" } else { "false" }),
            PropertyValue::Date(d) => user(&c.name, "date", d.trim_end_matches('Z')),
        }
    }
    w.end();
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_round_trip() {
        let p = DocProperties {
            title: Some("T".into()),
            subject: Some("S".into()),
            creator: Some("A".into()),
            last_modified_by: Some("B".into()),
            keywords: Some("x; y".into()),
            description: Some("D".into()),
            revision: Some(3),
            created: Some("2026-01-15T09:30:00".into()),
            modified: Some("2026-01-16T09:30:00".into()),
            category: Some("Cat".into()),
            content_status: Some("Draft".into()),
            language: Some("en-GB".into()),
            company: Some("Co".into()),
            manager: Some("M".into()),
            template: Some("Normal.dotm".into()),
            total_time: Some(90),
            custom: vec![
                CustomProperty {
                    name: "n".into(),
                    value: PropertyValue::Integer(42),
                },
                CustomProperty {
                    name: "r".into(),
                    value: PropertyValue::Real("0.75".into()),
                },
                CustomProperty {
                    name: "b".into(),
                    value: PropertyValue::Bool(true),
                },
                CustomProperty {
                    name: "d".into(),
                    value: PropertyValue::Date("2026-02-01T00:00:00".into()),
                },
                CustomProperty {
                    name: "t".into(),
                    value: PropertyValue::Text("text & <more>".into()),
                },
            ],
        };
        let back = read_meta(Some(&write_meta(&p, "Kiln"))).unwrap();
        assert_eq!(back, p);
        assert_eq!(read_meta(None).unwrap(), DocProperties::default());
    }
}
