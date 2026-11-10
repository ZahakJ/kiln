// SPDX-License-Identifier: AGPL-3.0-or-later
//! Model values inside Loro containers.
//!
//! A value the CRDT merges as a whole (a theme, a transition, a table grid) is one JSON string
//! under one key: concurrent writes to it are last-writer-wins. A property set (paragraph, run,
//! cell, section properties) is spread over a map with one JSON string per field, so two people
//! changing different properties of the same thing both keep their change. Writers only touch a
//! key when its value changed, so an unchanged value produces no CRDT operation.

use kiln_common::NodeId;
use loro::{
    Container, ContainerTrait, LoroMap, LoroMovableList, LoroText, LoroValue, ValueOrContainer,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::error::{Result, corrupt};

/// A value as stored.
pub(crate) fn to_json<T: Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_string(v)?)
}

/// A stored value.
pub(crate) fn from_json<T: DeserializeOwned>(s: &str) -> Result<T> {
    Ok(serde_json::from_str(s)?)
}

/// The string under `key`, if there is one.
pub(crate) fn get_str(map: &LoroMap, key: &str) -> Option<String> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

/// The integer under `key`, if there is one.
pub(crate) fn get_i64(map: &LoroMap, key: &str) -> Option<i64> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::I64(i)) => Some(i),
        _ => None,
    }
}

/// The bytes under `key`, if there are some.
pub(crate) fn get_bytes(map: &LoroMap, key: &str) -> Option<Vec<u8>> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::Binary(b)) => Some(b.to_vec()),
        _ => None,
    }
}

/// A node id as stored (ids stay below 2^53, so they fit an `i64`).
pub(crate) fn id_value(id: NodeId) -> i64 {
    i64::try_from(id.0).unwrap_or(i64::MAX)
}

/// The node id under `key`.
pub(crate) fn get_id(map: &LoroMap, key: &str) -> Result<NodeId> {
    get_i64(map, key)
        .and_then(|i| u64::try_from(i).ok())
        .map(NodeId)
        .ok_or_else(|| corrupt(format!("no {key} in {}", map.id())))
}

/// Stores a string unless it is already there.
pub(crate) fn put_str(map: &LoroMap, key: &str, s: &str) -> Result<()> {
    if get_str(map, key).as_deref() != Some(s) {
        map.insert(key, s)?;
    }
    Ok(())
}

/// Stores an integer unless it is already there.
pub(crate) fn put_i64(map: &LoroMap, key: &str, i: i64) -> Result<()> {
    if get_i64(map, key) != Some(i) {
        map.insert(key, i)?;
    }
    Ok(())
}

/// Stores bytes unless they are already there.
pub(crate) fn put_bytes(map: &LoroMap, key: &str, b: &[u8]) -> Result<()> {
    if get_bytes(map, key).as_deref() != Some(b) {
        map.insert(key, b.to_vec())?;
    }
    Ok(())
}

/// Stores a value as JSON unless the same value is already there.
pub(crate) fn put_json<T: Serialize>(map: &LoroMap, key: &str, v: &T) -> Result<()> {
    put_str(map, key, &to_json(v)?)
}

/// The value stored as JSON under `key`.
pub(crate) fn get_json<T: DeserializeOwned>(map: &LoroMap, key: &str) -> Result<T> {
    let s = get_str(map, key).ok_or_else(|| corrupt(format!("no {key} in {}", map.id())))?;
    from_json(&s)
}

/// The value stored as JSON under `key`, or `T::default()` when there is none.
pub(crate) fn get_json_or_default<T: DeserializeOwned + Default>(
    map: &LoroMap,
    key: &str,
) -> Result<T> {
    match get_str(map, key) {
        Some(s) => from_json(&s),
        None => Ok(T::default()),
    }
}

/// The child map under `key`, created when missing (as a mergeable container, so two replicas
/// creating it at the same time end up with one).
pub(crate) fn child_map(map: &LoroMap, key: &str) -> Result<LoroMap> {
    Ok(map.ensure_mergeable_map(key)?)
}

/// The child movable list under `key`, created when missing.
pub(crate) fn child_list(map: &LoroMap, key: &str) -> Result<LoroMovableList> {
    Ok(map.ensure_mergeable_movable_list(key)?)
}

/// The child text under `key`, created when missing.
pub(crate) fn child_text(map: &LoroMap, key: &str) -> Result<LoroText> {
    Ok(map.ensure_mergeable_text(key)?)
}

/// The child map under `key`, if there is one (readers never create containers).
pub(crate) fn find_map(map: &LoroMap, key: &str) -> Option<LoroMap> {
    match map.get(key)? {
        ValueOrContainer::Container(Container::Map(m)) => Some(m),
        _ => None,
    }
}

/// The child movable list under `key`, if there is one.
pub(crate) fn find_list(map: &LoroMap, key: &str) -> Option<LoroMovableList> {
    match map.get(key)? {
        ValueOrContainer::Container(Container::MovableList(l)) => Some(l),
        _ => None,
    }
}

/// The child text under `key`, if there is one.
pub(crate) fn find_text(map: &LoroMap, key: &str) -> Option<LoroText> {
    match map.get(key)? {
        ValueOrContainer::Container(Container::Text(t)) => Some(t),
        _ => None,
    }
}

/// The map at `index` of a list.
pub(crate) fn map_at(list: &LoroMovableList, index: usize) -> Option<LoroMap> {
    match list.get(index)? {
        ValueOrContainer::Container(Container::Map(m)) => Some(m),
        _ => None,
    }
}

/// Every map of a list, in order (other entries are skipped).
pub(crate) fn maps_of(list: &LoroMovableList) -> Vec<LoroMap> {
    (0..list.len()).filter_map(|i| map_at(list, i)).collect()
}

/// A struct's fields as JSON strings by field name; fields that are `null` (unset options) are
/// left out.
pub(crate) fn fields_of<T: Serialize>(v: &T) -> Result<Vec<(String, String)>> {
    match serde_json::to_value(v)? {
        Value::Object(o) => o
            .into_iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| Ok((k, serde_json::to_string(&v)?)))
            .collect(),
        other => Err(corrupt(format!("not a property set: {other}"))),
    }
}

/// Spreads a struct over a map, one key per field, writing only fields that changed and
/// removing fields that became unset.
pub(crate) fn put_fields<T: Serialize>(map: &LoroMap, v: &T) -> Result<()> {
    let fields = fields_of(v)?;
    let mut stale: Vec<String> = map.keys().map(|k| k.to_string()).collect();
    for (k, json) in &fields {
        stale.retain(|s| s != k);
        put_str(map, k, json)?;
    }
    for k in stale {
        map.delete(&k)?;
    }
    Ok(())
}

/// A struct read back from a map written by [`put_fields`] (missing fields are unset options).
pub(crate) fn get_fields<T: DeserializeOwned>(map: &LoroMap) -> Result<T> {
    let mut obj = Map::new();
    for k in map.keys() {
        if let Some(s) = get_str(map, &k) {
            obj.insert(k.to_string(), serde_json::from_str(&s)?);
        }
    }
    Ok(serde_json::from_value(Value::Object(obj))?)
}

/// [`get_fields`] of the child map under `key`; an absent map is the default value.
pub(crate) fn get_child_fields<T: DeserializeOwned + Default>(
    map: &LoroMap,
    key: &str,
) -> Result<T> {
    match find_map(map, key) {
        Some(m) => get_fields(&m),
        None => Ok(T::default()),
    }
}

/// A 64-bit FNV-1a hash (stable across platforms and releases, unlike `std`'s hasher).
pub(crate) fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::RunProps;

    #[test]
    fn property_sets_round_trip_field_by_field() -> Result<()> {
        let doc = loro::LoroDoc::new();
        let map = doc.get_map("m");
        let mut props = RunProps {
            bold: Some(true),
            size: Some(24),
            ..RunProps::default()
        };
        put_fields(&map, &props)?;
        assert_eq!(map.len(), 2);
        assert_eq!(get_fields::<RunProps>(&map)?, props);
        props.bold = None;
        props.italic = Some(false);
        put_fields(&map, &props)?;
        assert_eq!(get_fields::<RunProps>(&map)?, props);
        assert!(map.get("bold").is_none());
        Ok(())
    }

    #[test]
    fn unchanged_values_write_nothing() -> Result<()> {
        let doc = loro::LoroDoc::new();
        let map = doc.get_map("m");
        put_json(&map, "k", &vec![1, 2])?;
        doc.commit();
        let before = doc.oplog_vv();
        put_json(&map, "k", &vec![1, 2])?;
        put_i64(&map, "n", 3)?;
        put_i64(&map, "n", 3)?;
        doc.commit();
        assert_eq!(doc.len_ops(), 2);
        assert_ne!(doc.oplog_vv(), before);
        Ok(())
    }
}
