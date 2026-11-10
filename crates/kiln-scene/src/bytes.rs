// SPDX-License-Identifier: AGPL-3.0-or-later
//! `#[serde(with = "crate::bytes")]` for pixel buffers: serialised with `serialize_bytes`, so
//! postcard writes a length and the raw bytes and the schema records `BYTES` (one block copy in the
//! TypeScript decoder instead of one varint per byte). JSON still reads and writes number arrays.

use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserializer, Serializer};

/// Serialises a byte buffer.
///
/// # Errors
///
/// Whatever the serializer reports.
#[allow(clippy::ptr_arg)] // the signature `serde(with)` expects
pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(bytes)
}

/// Deserialises a byte buffer from bytes or a sequence of numbers.
///
/// # Errors
///
/// When the input is neither.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a byte buffer")
        }
        fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }
        fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(v)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 20));
            while let Some(b) = seq.next_element::<u8>()? {
                out.push(b);
            }
            Ok(out)
        }
    }
    d.deserialize_byte_buf(V)
}
