// SPDX-License-Identifier: AGPL-3.0-or-later
//! The app storage behind the core's `HostOp::Store*` requests.
//!
//! A [`StorageHost`] keeps byte values by area and key; a value can be replaced
//! ([`StorageHost::put`], durably and at once) or appended to ([`StorageHost::append`]; reading
//! it gives everything appended, in order). Native hosts answer the requests with
//! [`answer`] over [`crate::NativeStorage`]; the web answers them in the page from the Origin
//! Private File System (`apps/web/src/host`); tests use [`MemoryStorage`].

use std::collections::BTreeMap;

use kiln_common::{KilnError, Result};
use kiln_protocol::{HostOp, HostReply, StoreArea};

/// Byte values by area and key.
pub trait StorageHost {
    /// The value of a key (`None` when there is none).
    ///
    /// # Errors
    ///
    /// `Io` when the storage cannot be read.
    fn get(&self, area: StoreArea, key: &str) -> Result<Option<Vec<u8>>>;
    /// Replaces a value.
    ///
    /// # Errors
    ///
    /// `Io` when the storage cannot be written.
    fn put(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()>;
    /// Appends to a value (creating it).
    ///
    /// # Errors
    ///
    /// `Io` when the storage cannot be written.
    fn append(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()>;
    /// Removes a value (nothing happens when there is none).
    ///
    /// # Errors
    ///
    /// `Io` when the storage cannot be written.
    fn delete(&mut self, area: StoreArea, key: &str) -> Result<()>;
    /// The keys of an area starting with `prefix`, sorted.
    ///
    /// # Errors
    ///
    /// `Io` when the storage cannot be read.
    fn list(&self, area: StoreArea, prefix: &str) -> Result<Vec<String>>;
}

/// Answers a `HostOp::Store*` request from a storage (`None` for other requests).
pub fn answer(store: &mut dyn StorageHost, op: &HostOp) -> Option<HostReply> {
    let reply = |r: Result<HostReply>| r.unwrap_or_else(|e| HostReply::Failed((&e).into()));
    Some(match op {
        HostOp::StoreGet { area, key } => reply(store.get(*area, key).map(|v| match v {
            Some(bytes) => HostReply::Bytes(bytes),
            None => HostReply::Empty,
        })),
        HostOp::StorePut { area, key, bytes } => {
            reply(store.put(*area, key, bytes).map(|()| HostReply::Done))
        }
        HostOp::StoreAppend { area, key, bytes } => {
            reply(store.append(*area, key, bytes).map(|()| HostReply::Done))
        }
        HostOp::StoreDelete { area, key } => {
            reply(store.delete(*area, key).map(|()| HostReply::Done))
        }
        HostOp::StoreList { area, prefix } => reply(store.list(*area, prefix).map(HostReply::Keys)),
        _ => return None,
    })
}

/// Checks a key: 1–512 bytes, no control characters, no `..` segment.
///
/// # Errors
///
/// `InvalidInput` naming the key.
pub fn check_key(key: &str) -> Result<()> {
    let bad = key.is_empty()
        || key.len() > 512
        || key.chars().any(char::is_control)
        || key.split('/').any(|s| s == "..");
    if bad {
        Err(KilnError::InvalidInput(format!(
            "{key:?} is not a storage key"
        )))
    } else {
        Ok(())
    }
}

/// An app storage in memory (tests, and hosts that keep nothing between runs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryStorage {
    values: BTreeMap<(StoreArea, String), Vec<u8>>,
}

impl StorageHost for MemoryStorage {
    fn get(&self, area: StoreArea, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.values.get(&(area, key.to_owned())).cloned())
    }

    fn put(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()> {
        check_key(key)?;
        self.values.insert((area, key.to_owned()), bytes.to_vec());
        Ok(())
    }

    fn append(&mut self, area: StoreArea, key: &str, bytes: &[u8]) -> Result<()> {
        check_key(key)?;
        self.values
            .entry((area, key.to_owned()))
            .or_default()
            .extend_from_slice(bytes);
        Ok(())
    }

    fn delete(&mut self, area: StoreArea, key: &str) -> Result<()> {
        self.values.remove(&(area, key.to_owned()));
        Ok(())
    }

    fn list(&self, area: StoreArea, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .values
            .keys()
            .filter(|(a, k)| *a == area && k.starts_with(prefix))
            .map(|(_, k)| k.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_answered_from_a_store() {
        let mut s = MemoryStorage::default();
        let a = StoreArea::Recents;
        let put = HostOp::StorePut {
            area: a,
            key: "r/1".into(),
            bytes: vec![1],
        };
        assert_eq!(answer(&mut s, &put), Some(HostReply::Done));
        let app = HostOp::StoreAppend {
            area: a,
            key: "r/1".into(),
            bytes: vec![2, 3],
        };
        assert_eq!(answer(&mut s, &app), Some(HostReply::Done));
        let get = HostOp::StoreGet {
            area: a,
            key: "r/1".into(),
        };
        assert_eq!(answer(&mut s, &get), Some(HostReply::Bytes(vec![1, 2, 3])));
        let other = HostOp::StoreGet {
            area: StoreArea::Prefs,
            key: "r/1".into(),
        };
        assert_eq!(
            answer(&mut s, &other),
            Some(HostReply::Empty),
            "areas are apart"
        );
        let list = HostOp::StoreList {
            area: a,
            prefix: "r/".into(),
        };
        assert_eq!(
            answer(&mut s, &list),
            Some(HostReply::Keys(vec!["r/1".into()]))
        );
        let del = HostOp::StoreDelete {
            area: a,
            key: "r/1".into(),
        };
        assert_eq!(answer(&mut s, &del), Some(HostReply::Done));
        assert_eq!(answer(&mut s, &get), Some(HostReply::Empty));
        assert_eq!(answer(&mut s, &HostOp::Recents), None);
        let bad = HostOp::StorePut {
            area: a,
            key: "../x".into(),
            bytes: vec![],
        };
        assert!(matches!(answer(&mut s, &bad), Some(HostReply::Failed(_))));
    }
}
