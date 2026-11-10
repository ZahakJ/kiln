// SPDX-License-Identifier: AGPL-3.0-or-later
//! The document-wide parts: styles (a movable list, so their order survives), document
//! defaults, lists, settings, properties, theme and media.

use std::collections::{BTreeMap, HashSet};

use kiln_doc::numbering::{AbstractNum, NumInstance};
use kiln_doc::settings::{DocProperties, Media, Settings};
use kiln_doc::style::{DocDefaults, Style, StyleSheet};
use kiln_doc::theme::DocTheme;
use kiln_doc::{Document, Numbering, Op};
use loro::{LoroDoc, LoroMap};

use crate::error::{Result, corrupt};
use crate::value::{
    child_map, find_map, from_json, get_bytes, get_fields, get_json, get_str, map_at, maps_of,
    put_bytes, put_fields, put_json, put_str, to_json,
};

/// Root containers.
pub(crate) const STYLES: &str = "styles";
pub(crate) const DOC: &str = "doc";
pub(crate) const ABSTRACTS: &str = "abstracts";
pub(crate) const INSTANCES: &str = "instances";
pub(crate) const MEDIA: &str = "media";

/// The document-wide parts as the CRDT holds them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Parts {
    pub styles: Vec<Style>,
    pub defaults: DocDefaults,
    pub numbering: Numbering,
    pub settings: Settings,
    pub properties: DocProperties,
    pub theme: DocTheme,
    pub media: BTreeMap<String, Media>,
}

fn write_keyed(map: &LoroMap, entries: &[(String, Vec<u8>)], json: bool) -> Result<()> {
    let keep: HashSet<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
    let stale: Vec<String> = map
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !keep.contains(k.as_str()))
        .collect();
    for k in stale {
        map.delete(&k)?;
    }
    for (k, v) in entries {
        if json {
            put_str(map, k, &String::from_utf8_lossy(v))?;
        } else {
            put_bytes(map, k, v)?;
        }
    }
    Ok(())
}

fn json_entries<K: ToString, T: serde::Serialize>(
    items: impl IntoIterator<Item = (K, T)>,
) -> Result<Vec<(String, Vec<u8>)>> {
    items
        .into_iter()
        .map(|(k, v)| Ok((k.to_string(), to_json(&v)?.into_bytes())))
        .collect()
}

/// Writes every document-wide part (unchanged values write nothing).
pub(crate) fn write_parts(ldoc: &LoroDoc, doc: &Document) -> Result<()> {
    let styles = ldoc.get_movable_list(STYLES);
    let want: Vec<(&str, String)> = doc
        .styles
        .iter()
        .map(|s| Ok((s.id.as_str(), to_json(s)?)))
        .collect::<Result<_>>()?;
    let keep: HashSet<&str> = want.iter().map(|(id, _)| *id).collect();
    let mut have: Vec<Option<String>> = (0..styles.len())
        .map(|i| map_at(&styles, i).and_then(|m| get_str(&m, "id")))
        .collect();
    // Entries no longer wanted go, and so do later duplicates of an id (from a merge).
    let mut seen = HashSet::new();
    let doomed: Vec<usize> = have
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            !id.as_deref()
                .is_some_and(|id| keep.contains(id) && seen.insert(id.to_owned()))
        })
        .map(|(i, _)| i)
        .collect();
    for i in doomed.into_iter().rev() {
        styles.delete(i, 1)?;
        have.remove(i);
    }
    for (i, (id, json)) in want.iter().enumerate() {
        if have.get(i).and_then(Option::as_deref) != Some(*id) {
            match (i + 1..have.len()).find(|&j| have[j].as_deref() == Some(*id)) {
                Some(j) => {
                    styles.mov(j, i)?;
                    let v = have.remove(j);
                    have.insert(i, v);
                }
                None => {
                    styles.insert_container(i, LoroMap::new())?;
                    have.insert(i, Some((*id).to_owned()));
                }
            }
        }
        let map = map_at(&styles, i).ok_or_else(|| corrupt("a style entry that is not a map"))?;
        put_str(&map, "id", id)?;
        put_str(&map, "v", json)?;
    }
    while styles.len() > want.len() {
        styles.delete(styles.len() - 1, 1)?;
    }
    let root = ldoc.get_map(DOC);
    put_json(&root, "defaults", &doc.styles.doc_defaults)?;
    put_fields(&child_map(&root, "settings")?, &doc.settings)?;
    put_fields(&child_map(&root, "properties")?, &doc.properties)?;
    put_json(&root, "theme", &doc.theme)?;
    write_keyed(
        &ldoc.get_map(ABSTRACTS),
        &json_entries(doc.numbering.abstracts.iter())?,
        true,
    )?;
    write_keyed(
        &ldoc.get_map(INSTANCES),
        &json_entries(doc.numbering.instances.iter())?,
        true,
    )?;
    let media: Vec<(String, Vec<u8>)> = doc
        .media
        .iter()
        .map(|(k, m)| Ok((k.clone(), postcard::to_allocvec(m)?)))
        .collect::<Result<_>>()?;
    write_keyed(&ldoc.get_map(MEDIA), &media, false)
}

fn read_keyed<T: serde::de::DeserializeOwned>(map: &LoroMap) -> Result<BTreeMap<u32, T>> {
    let mut out = BTreeMap::new();
    for k in map.keys() {
        if let (Ok(id), Some(json)) = (k.parse::<u32>(), get_str(map, &k)) {
            out.insert(id, from_json(&json)?);
        }
    }
    Ok(out)
}

/// The document-wide parts read back.
pub(crate) fn read_parts(ldoc: &LoroDoc) -> Result<Parts> {
    let mut seen = HashSet::new();
    let mut styles = Vec::new();
    for m in maps_of(&ldoc.get_movable_list(STYLES)) {
        if let (Some(id), Some(json)) = (get_str(&m, "id"), get_str(&m, "v"))
            && seen.insert(id)
        {
            styles.push(from_json(&json)?);
        }
    }
    let root = ldoc.get_map(DOC);
    let media_map = ldoc.get_map(MEDIA);
    let mut media = BTreeMap::new();
    for k in media_map.keys() {
        if let Some(bytes) = get_bytes(&media_map, &k) {
            media.insert(k.to_string(), postcard::from_bytes(&bytes)?);
        }
    }
    Ok(Parts {
        styles,
        defaults: get_json(&root, "defaults")?,
        numbering: Numbering {
            abstracts: read_keyed(&ldoc.get_map(ABSTRACTS))?,
            instances: read_keyed(&ldoc.get_map(INSTANCES))?,
        },
        settings: get_fields(&find_map(&root, "settings").ok_or_else(|| corrupt("no settings"))?)?,
        properties: match find_map(&root, "properties") {
            Some(m) => get_fields(&m)?,
            None => DocProperties::default(),
        },
        theme: get_json(&root, "theme")?,
        media,
    })
}

/// Sets a document's parts (a document being built from the CRDT).
pub(crate) fn install_parts(doc: &mut Document, parts: Parts) {
    doc.styles = StyleSheet::new(parts.defaults, parts.styles);
    doc.numbering = parts.numbering;
    doc.settings = parts.settings;
    doc.properties = parts.properties;
    doc.theme = parts.theme;
    doc.media = parts.media;
}

fn keyed_ops<T: Clone + PartialEq>(
    have: &BTreeMap<u32, T>,
    want: &BTreeMap<u32, T>,
    op: impl Fn(u32, Option<T>) -> Op,
) -> Vec<Op> {
    let mut ops = Vec::new();
    for (id, v) in want {
        if have.get(id) != Some(v) {
            ops.push(op(*id, Some(v.clone())));
        }
    }
    for id in have.keys() {
        if !want.contains_key(id) {
            ops.push(op(*id, None));
        }
    }
    ops
}

/// The ops that give a document the parts the CRDT holds.
pub(crate) fn part_ops(doc: &Document, parts: &Parts) -> Vec<Op> {
    let mut ops = Vec::new();
    for (i, s) in parts.styles.iter().enumerate() {
        if doc.styles.get(&s.id) != Some(s) {
            ops.push(Op::PutStyle {
                style: Box::new(s.clone()),
                index: i,
            });
        }
    }
    let ids: HashSet<&str> = parts.styles.iter().map(|s| s.id.as_str()).collect();
    for s in doc.styles.iter() {
        if !ids.contains(s.id.as_str()) {
            ops.push(Op::DeleteStyle { id: s.id.clone() });
        }
    }
    if doc.styles.doc_defaults != parts.defaults {
        ops.push(Op::SetDocDefaults {
            defaults: Box::new(parts.defaults.clone()),
        });
    }
    ops.extend(keyed_ops::<AbstractNum>(
        &doc.numbering.abstracts,
        &parts.numbering.abstracts,
        |id, def| Op::SetAbstractNum {
            id,
            def: def.map(Box::new),
        },
    ));
    ops.extend(keyed_ops::<NumInstance>(
        &doc.numbering.instances,
        &parts.numbering.instances,
        |id, instance| Op::SetNumInstance { id, instance },
    ));
    if doc.settings != parts.settings {
        ops.push(Op::SetSettings {
            settings: Box::new(parts.settings.clone()),
        });
    }
    if doc.properties != parts.properties {
        ops.push(Op::SetProperties {
            properties: Box::new(parts.properties.clone()),
        });
    }
    if doc.theme != parts.theme {
        ops.push(Op::SetTheme {
            theme: Box::new(parts.theme.clone()),
        });
    }
    for (name, m) in &parts.media {
        if doc.media.get(name) != Some(m) {
            ops.push(Op::PutMedia {
                name: name.clone(),
                media: Some(m.clone()),
            });
        }
    }
    for name in doc.media.keys() {
        if !parts.media.contains_key(name) {
            ops.push(Op::PutMedia {
                name: name.clone(),
                media: None,
            });
        }
    }
    ops
}
