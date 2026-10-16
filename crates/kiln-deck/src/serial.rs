// SPDX-License-Identifier: AGPL-3.0-or-later
//! Binary storage of the model: postcard behind a small header (`model.bin` in `.kdeck`
//! bundles, autosave snapshots, the control channel).

use crate::error::{DeckError, Result};
use crate::ops::Transaction;
use crate::presentation::Presentation;

/// The first bytes of a serialised presentation.
const MAGIC: &[u8; 4] = b"KDCK";
/// The first bytes of a serialised transaction.
const TX_MAGIC: &[u8; 4] = b"KDTX";
/// The format version; bump on incompatible model changes.
const FORMAT_VERSION: u16 = 1;

fn header(magic: &[u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 1024);
    out.extend_from_slice(magic);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out
}

fn body<'a>(bytes: &'a [u8], magic: &[u8; 4], what: &str) -> Result<&'a [u8]> {
    let rest = bytes
        .strip_prefix(magic.as_slice())
        .ok_or_else(|| DeckError::Corrupt(format!("not a Kiln Deck {what}")))?;
    let (version, rest) = rest
        .split_first_chunk::<2>()
        .ok_or_else(|| DeckError::Corrupt("truncated header".into()))?;
    let version = u16::from_le_bytes(*version);
    if version != FORMAT_VERSION {
        return Err(DeckError::Corrupt(format!(
            "unsupported {what} version {version}"
        )));
    }
    Ok(rest)
}

impl Presentation {
    /// Serialises the presentation: `KDCK`, a little-endian `u16` version, then postcard.
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] if a value cannot be encoded (a DrawingML value kept from a file
    /// with an undeclarable namespace prefix).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_extend(self, header(MAGIC)).map_err(|e| DeckError::Corrupt(e.to_string()))
    }

    /// Reads a presentation written by [`Presentation::to_bytes`] and validates it, so damaged
    /// or hostile bytes are refused instead of breaking invariants later.
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] for a wrong header, an unknown version, undecodable or trailing
    /// bytes, or a model that fails [`Presentation::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let rest = body(bytes, MAGIC, "presentation")?;
        let (mut pres, rest): (Presentation, &[u8]) =
            postcard::take_from_bytes(rest).map_err(|e| DeckError::Corrupt(e.to_string()))?;
        if !rest.is_empty() {
            return Err(DeckError::Corrupt(format!("{} trailing bytes", rest.len())));
        }
        // The allocator is stored, but never trust it to be above every id.
        pres.refresh_id_counter();
        pres.validate()?;
        Ok(pres)
    }
}

impl Transaction {
    /// Serialises a transaction (autosave journal, control channel).
    ///
    /// # Errors
    /// As [`Presentation::to_bytes`].
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_extend(self, header(TX_MAGIC)).map_err(|e| DeckError::Corrupt(e.to_string()))
    }

    /// Reads a transaction written by [`Transaction::to_bytes`].
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] for a wrong header or undecodable bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let rest = body(bytes, TX_MAGIC, "transaction")?;
        let (tx, rest): (Transaction, &[u8]) =
            postcard::take_from_bytes(rest).map_err(|e| DeckError::Corrupt(e.to_string()))?;
        if !rest.is_empty() {
            return Err(DeckError::Corrupt(format!("{} trailing bytes", rest.len())));
        }
        Ok(tx)
    }
}
