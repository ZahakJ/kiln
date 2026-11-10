// SPDX-License-Identifier: AGPL-3.0-or-later
//! A shared document of either kind behind one byte-level API: models and transactions go in
//! and come out in their postcard form (`Document::to_bytes`, `Transaction::to_bytes`).
//!
//! The web runs collaboration in its own module, loaded only when a document is shared, next to
//! the app core that holds the document: the core hands each applied transaction to the
//! [`Replica`] and applies what [`Replica::apply_remote`] returns. The replica keeps a copy of
//! the model to work the ops out against.

use kiln_deck::Presentation;
use kiln_doc::{Document, IdLanes};
use serde::{Deserialize, Serialize};

use crate::crdt::LoroCrdt;
use crate::deck::{DeckBinding, DeckChange};
use crate::error::{CollabError, Result};
use crate::history::VersionHistory;
use crate::protocol::RoomKind;
use crate::write::{WriteBinding, WriteChange};

/// What the core must do after a merge, undo or redo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreChange {
    /// Nothing changed.
    None,
    /// Apply this transaction (postcard) and invalidate what its ops touch.
    Apply(Vec<u8>),
    /// Replace the model with this one (postcard) and lay everything out again.
    Replace(Vec<u8>),
}

impl CoreChange {
    /// The stored form.
    ///
    /// # Errors
    ///
    /// postcard failed to encode.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(postcard::to_allocvec(self)?)
    }

    /// Reads the stored form.
    ///
    /// # Errors
    ///
    /// Damaged bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(postcard::from_bytes(bytes)?)
    }
}

/// A shared Write, Notes or Deck document with its model.
#[derive(Debug)]
pub enum Replica {
    /// Kiln Write or Kiln Notes.
    Write {
        /// The binding.
        binding: Box<WriteBinding>,
        /// The model.
        doc: Box<Document>,
    },
    /// Kiln Deck.
    Deck {
        /// The binding.
        binding: Box<DeckBinding>,
        /// The model.
        pres: Box<Presentation>,
    },
}

fn doc_err(e: kiln_doc::DocError) -> CollabError {
    CollabError::Doc(e)
}

impl Replica {
    /// Starts sharing a model (postcard bytes of a document or presentation).
    ///
    /// # Errors
    ///
    /// Bytes that are not a model of that kind.
    pub fn share(kind: RoomKind, model: &[u8], peer: u64, lanes: IdLanes) -> Result<Self> {
        Ok(match kind {
            RoomKind::Write | RoomKind::Notes => {
                let mut doc = Document::from_bytes(model).map_err(doc_err)?;
                let binding = WriteBinding::share(&mut doc, peer, lanes)?;
                Self::Write {
                    binding: Box::new(binding),
                    doc: Box::new(doc),
                }
            }
            RoomKind::Deck => {
                let mut pres = Presentation::from_bytes(model)?;
                let binding = DeckBinding::share(&mut pres, peer, lanes)?;
                Self::Deck {
                    binding: Box::new(binding),
                    pres: Box::new(pres),
                }
            }
        })
    }

    /// Joins from the state a server sent.
    ///
    /// # Errors
    ///
    /// A state that is not a document of that kind.
    pub fn join(kind: RoomKind, state: &[u8], peer: u64, lanes: IdLanes) -> Result<Self> {
        Ok(match kind {
            RoomKind::Write | RoomKind::Notes => {
                let (binding, doc) = WriteBinding::join(state, peer, lanes)?;
                Self::Write {
                    binding: Box::new(binding),
                    doc: Box::new(doc),
                }
            }
            RoomKind::Deck => {
                let (binding, pres) = DeckBinding::join(state, peer, lanes)?;
                Self::Deck {
                    binding: Box::new(binding),
                    pres: Box::new(pres),
                }
            }
        })
    }

    /// Gives the model the id lane of a server's welcome.
    pub fn set_id_lanes(&mut self, lanes: IdLanes) {
        match self {
            Self::Write { doc, .. } => doc.set_id_lanes(lanes),
            Self::Deck { pres, .. } => pres.set_id_lanes(lanes),
        }
    }

    /// The CRDT.
    #[must_use]
    pub fn crdt(&self) -> &LoroCrdt {
        match self {
            Self::Write { binding, .. } => binding.crdt(),
            Self::Deck { binding, .. } => binding.crdt(),
        }
    }

    /// The CRDT, mutably (to merge a welcome's state with [`Replica::apply_remote`] instead).
    pub fn crdt_mut(&mut self) -> &mut LoroCrdt {
        match self {
            Self::Write { binding, .. } => binding.crdt_mut(),
            Self::Deck { binding, .. } => binding.crdt_mut(),
        }
    }

    /// The model (postcard), for the core to load after joining.
    ///
    /// # Errors
    ///
    /// postcard failed to encode.
    pub fn model(&self) -> Result<Vec<u8>> {
        match self {
            Self::Write { doc, .. } => doc.to_bytes().map_err(doc_err),
            Self::Deck { pres, .. } => Ok(pres.to_bytes()?),
        }
    }

    /// Records a transaction (postcard) the core has just applied.
    ///
    /// # Errors
    ///
    /// Bytes that are not a transaction of this kind, or a transaction the model copy refuses
    /// (the core and the replica have drifted apart: reload the core from [`Replica::model`]).
    pub fn commit(&mut self, tx: &[u8]) -> Result<()> {
        match self {
            Self::Write { binding, doc } => {
                let tx = kiln_doc::Transaction::from_bytes(tx).map_err(doc_err)?;
                doc.apply(tx.clone())?;
                binding.commit(doc, &tx)
            }
            Self::Deck { binding, pres } => {
                let tx = kiln_deck::Transaction::from_bytes(tx)?;
                pres.apply(tx.clone())?;
                binding.commit(pres, &tx)
            }
        }
    }

    /// Merges an update from another replica.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import, or a merged state that is not a valid model.
    pub fn apply_remote(&mut self, update: &[u8]) -> Result<CoreChange> {
        match self {
            Self::Write { binding, doc } => {
                let c = binding.apply_remote(doc, update)?;
                write_change(doc, c)
            }
            Self::Deck { binding, pres } => {
                let c = binding.apply_remote(pres, update)?;
                deck_change(pres, c)
            }
        }
    }

    /// Undoes the local peer's latest change.
    ///
    /// # Errors
    ///
    /// The CRDT failed to undo.
    pub fn undo(&mut self) -> Result<CoreChange> {
        match self {
            Self::Write { binding, doc } => match binding.undo(doc)? {
                Some(c) => write_change(doc, c),
                None => Ok(CoreChange::None),
            },
            Self::Deck { binding, pres } => match binding.undo(pres)? {
                Some(c) => deck_change(pres, c),
                None => Ok(CoreChange::None),
            },
        }
    }

    /// Redoes the latest undone local change.
    ///
    /// # Errors
    ///
    /// The CRDT failed to redo.
    pub fn redo(&mut self) -> Result<CoreChange> {
        match self {
            Self::Write { binding, doc } => match binding.redo(doc)? {
                Some(c) => write_change(doc, c),
                None => Ok(CoreChange::None),
            },
            Self::Deck { binding, pres } => match binding.redo(pres)? {
                Some(c) => deck_change(pres, c),
                None => Ok(CoreChange::None),
            },
        }
    }

    /// Ends the open typing group (the caret moved).
    pub fn break_undo_group(&mut self) {
        match self {
            Self::Write { binding, .. } => binding.break_undo_group(),
            Self::Deck { binding, .. } => binding.break_undo_group(),
        }
    }

    /// A version's model (postcard), for a read-only preview.
    ///
    /// # Errors
    ///
    /// An unknown version.
    pub fn checkout(&self, id: &str) -> Result<Vec<u8>> {
        match self {
            Self::Write { binding, .. } => VersionHistory::checkout_write(binding.crdt(), id)?
                .to_bytes()
                .map_err(doc_err),
            Self::Deck { binding, .. } => {
                Ok(VersionHistory::checkout_deck(binding.crdt(), id)?.to_bytes()?)
            }
        }
    }

    /// Restores a version as a new change; the core replaces its model with the result.
    ///
    /// # Errors
    ///
    /// An unknown version, or a failure to write the CRDT.
    pub fn restore(
        &mut self,
        history: &mut VersionHistory,
        id: &str,
        author: &str,
        now_ms: u64,
    ) -> Result<CoreChange> {
        match self {
            Self::Write { binding, doc } => {
                **doc = history.restore_write(id, binding, doc, author, now_ms)?;
                Ok(CoreChange::Replace(doc.to_bytes().map_err(doc_err)?))
            }
            Self::Deck { binding, pres } => {
                **pres = history.restore_deck(id, binding, pres, author, now_ms)?;
                Ok(CoreChange::Replace(pres.to_bytes()?))
            }
        }
    }
}

fn write_change(doc: &Document, c: WriteChange) -> Result<CoreChange> {
    Ok(if c.reloaded {
        CoreChange::Replace(doc.to_bytes().map_err(doc_err)?)
    } else if c.tx.is_empty() {
        CoreChange::None
    } else {
        CoreChange::Apply(c.tx.to_bytes().map_err(doc_err)?)
    })
}

fn deck_change(pres: &Presentation, c: DeckChange) -> Result<CoreChange> {
    Ok(if c.reloaded {
        CoreChange::Replace(pres.to_bytes()?)
    } else if c.tx.is_empty() {
        CoreChange::None
    } else {
        CoreChange::Apply(c.tx.to_bytes()?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::Crdt;
    use kiln_doc::{Pos, edit};

    #[test]
    fn a_core_and_its_replica_stay_in_step_through_bytes() -> Result<()> {
        // The core's document.
        let mut core = Document::new();
        core.set_id_lanes(IdLanes::new(1, 4));
        let mut a = Replica::share(RoomKind::Write, &core.to_bytes()?, 1, IdLanes::new(1, 4))?;
        let mut b = Replica::join(
            RoomKind::Write,
            &a.crdt().snapshot()?,
            2,
            IdLanes::new(2, 4),
        )?;
        let mut b_core = Document::from_bytes(&b.model()?)?;
        let p = core.body_paragraphs()[0].id;
        let tx = edit::insert_text(&mut core, Pos::new(p, 0), "shared")?;
        core.apply(tx.clone())?;
        a.commit(&tx.to_bytes()?)?;
        let update = a.crdt().export_from(&b.crdt().version())?;
        match b.apply_remote(&update)? {
            CoreChange::Apply(bytes) => {
                b_core.apply(kiln_doc::Transaction::from_bytes(&bytes)?)?;
            }
            other => panic!("expected ops, got {other:?}"),
        }
        assert_eq!(b_core, core);
        assert!(matches!(a.undo()?, CoreChange::Apply(_)));
        Ok(())
    }
}
