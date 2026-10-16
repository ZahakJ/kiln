// SPDX-License-Identifier: AGPL-3.0-or-later
//! Binary storage of the model: postcard behind a small header (`model.bin` in `.kwrite`
//! bundles, autosave snapshots).

use crate::DocError;
use crate::model::Document;
use crate::ops::Transaction;

/// The first bytes of a serialised document.
const MAGIC: &[u8; 4] = b"KDOC";
/// The format version; bump on incompatible model changes.
const FORMAT_VERSION: u16 = 1;

impl Document {
    /// Serialises the document: `KDOC`, a little-endian `u16` version, then postcard.
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] if postcard cannot encode a value (not expected for valid models).
    pub fn to_bytes(&self) -> Result<Vec<u8>, DocError> {
        let mut out = Vec::with_capacity(4096);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        postcard::to_extend(self, out).map_err(|e| DocError::Corrupt(e.to_string()))
    }

    /// Reads a document written by [`Document::to_bytes`] and validates it, so a damaged or
    /// hostile file is refused instead of breaking invariants later.
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] for a wrong header, an unknown version, undecodable bytes, trailing
    /// bytes or a model that fails [`Document::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocError> {
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or_else(|| DocError::Corrupt("not a Kiln Write model".into()))?;
        let (version, body) = body
            .split_first_chunk::<2>()
            .ok_or_else(|| DocError::Corrupt("truncated header".into()))?;
        let version = u16::from_le_bytes(*version);
        if version != FORMAT_VERSION {
            return Err(DocError::Corrupt(format!(
                "unsupported model version {version}"
            )));
        }
        let (doc, rest): (Document, &[u8]) =
            postcard::take_from_bytes(body).map_err(|e| DocError::Corrupt(e.to_string()))?;
        if !rest.is_empty() {
            return Err(DocError::Corrupt(format!("{} trailing bytes", rest.len())));
        }
        doc.validate()?;
        Ok(doc)
    }
}

impl Transaction {
    /// Serialises a transaction with postcard (for the autosave journal and the control channel).
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] if postcard cannot encode a value.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DocError> {
        postcard::to_allocvec(self).map_err(|e| DocError::Corrupt(e.to_string()))
    }

    /// Reads a transaction written by [`Transaction::to_bytes`]. Its ops are validated when they
    /// are applied.
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] for undecodable bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocError> {
        postcard::from_bytes(bytes).map_err(|e| DocError::Corrupt(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_foreign_and_damaged_bytes() -> Result<(), DocError> {
        let doc = Document::new();
        let bytes = doc.to_bytes()?;
        assert_eq!(&bytes[..4], b"KDOC");
        assert_eq!(Document::from_bytes(&bytes)?, doc);
        assert!(Document::from_bytes(b"PK\x03\x04").is_err());
        assert!(Document::from_bytes(b"KDOC").is_err());
        let mut v9 = bytes.clone();
        v9[4] = 9;
        assert!(
            matches!(Document::from_bytes(&v9), Err(DocError::Corrupt(m)) if m.contains("version"))
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Document::from_bytes(&trailing).is_err());
        assert!(Document::from_bytes(&bytes[..bytes.len() / 2]).is_err());
        assert!(Transaction::from_bytes(&[0xFF, 0xFF, 0xFF]).is_err());
        Ok(())
    }

    #[test]
    fn keeps_the_id_allocator() -> Result<(), DocError> {
        let mut doc = Document::new();
        for _ in 0..10 {
            doc.alloc_id();
        }
        let back = Document::from_bytes(&doc.to_bytes()?)?;
        assert_eq!(back.peek_next_id(), doc.peek_next_id());
        Ok(())
    }
}
