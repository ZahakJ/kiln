// SPDX-License-Identifier: AGPL-3.0-or-later
//! A presentation as a native bundle (`.kdeck`) and in the autosave journal.
//!
//! `model.bin` is the presentation (`KDCK`); the file it was opened from goes to
//! `passthrough/original.pptx` and the parts the edits since opening touched to
//! `passthrough/touched.json` (saving it as PowerPoint again writes exactly what a save without
//! the bundle would: the touched slides, layouts and masters from the model, every other part
//! byte for byte from the original), and Set Up Slide Show and the drawing guides, which live
//! beside the model, to `passthrough/kiln-deck.json`. The journal records every transaction the editor applies
//! (where collaboration records them, plus Undo and Redo) and replays them on the checkpoint.

use std::collections::BTreeSet;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::Presentation;
use kiln_deck::ops::Transaction;
use kiln_protocol::DocKind;
use kiln_protocol::commands::deck_content::DeckGuide;
use kiln_protocol::commands::deck_show::DeckShowSettings;
use kiln_storage::Bundle;
use serde::{Deserialize, Serialize};

use super::{DeckDoc, save};

/// The passthrough entry of the original PowerPoint file.
const ORIGINAL: &str = "original.pptx";
/// The passthrough entry of the settings beside the model.
const SETTINGS: &str = "kiln-deck.json";
/// The passthrough entry of the parts touched since the original was opened.
const TOUCHED: &str = "touched.json";

/// [`kiln_io_pptx::TouchedParts`] as kept in a bundle.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Touched {
    everything: bool,
    presentation: bool,
    owners: BTreeSet<u64>,
    notes: BTreeSet<u64>,
    comments: BTreeSet<u64>,
}

impl From<&kiln_io_pptx::TouchedParts> for Touched {
    fn from(t: &kiln_io_pptx::TouchedParts) -> Self {
        let ids = |s: &BTreeSet<NodeId>| s.iter().map(|n| n.0).collect();
        Self {
            everything: t.everything,
            presentation: t.presentation,
            owners: ids(&t.owners),
            notes: ids(&t.notes),
            comments: ids(&t.comments),
        }
    }
}

impl From<Touched> for kiln_io_pptx::TouchedParts {
    fn from(t: Touched) -> Self {
        let ids = |s: BTreeSet<u64>| s.into_iter().map(NodeId).collect();
        Self {
            everything: t.everything,
            presentation: t.presentation,
            owners: ids(t.owners),
            notes: ids(t.notes),
            comments: ids(t.comments),
        }
    }
}

/// What a presentation keeps beside its model.
#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    show: DeckShowSettings,
    guides: Vec<DeckGuide>,
}

fn err(e: impl std::fmt::Display) -> KilnError {
    KilnError::Format {
        format: "kdeck".into(),
        message: e.to_string(),
    }
}

impl DeckDoc {
    /// Records a transaction for the journal (while it records).
    pub(super) fn record_journal(&mut self, tx: &Transaction) {
        if let Some(j) = &mut self.journal
            && let Ok(bytes) = tx.to_bytes()
        {
            j.push(bytes);
        }
    }

    /// The presentation as a bundle (the presentation itself while a shared version is shown).
    pub(super) fn to_bundle(&self) -> Result<Bundle> {
        let pres = self.live_presentation();
        let mut b = Bundle::new(DocKind::Deck, pres.to_bytes().map_err(err)?);
        if let Some(o) = &self.original {
            b.passthrough.insert(ORIGINAL.into(), o.bytes().to_vec());
            let touched = serde_json::to_vec(&Touched::from(&self.touched)).map_err(err)?;
            b.passthrough.insert(TOUCHED.into(), touched);
        }
        let settings = Settings {
            show: self.show_settings.clone(),
            guides: self.guides.clone(),
        };
        b.passthrough
            .insert(SETTINGS.into(), serde_json::to_vec(&settings).map_err(err)?);
        if let Some(p) = &self.props {
            p.to_bundle(&mut b);
        }
        Ok(b)
    }

    /// The presentation of a bundle: saving it as PowerPoint writes the parts touched since
    /// the original was opened from the model and copies every other part of the original (a
    /// bundle without that record has every part written from the model). An original that
    /// cannot be read is a loss the Compatibility Checker reports.
    pub(super) fn from_bundle(bundle: &Bundle, name: Option<String>) -> Result<Self> {
        let pres = Presentation::from_bytes(&bundle.model).map_err(err)?;
        let read = bundle.passthrough.get(ORIGINAL).map(|bytes| {
            kiln_io_pptx::read_pptx(bytes).map(|r| save::Original::from_read(r, bytes).1)
        });
        let lost = matches!(read, Some(Err(_)));
        let original = read.and_then(std::result::Result::ok);
        let has_original = original.is_some();
        let mut d = Self::new(pres, original, name);
        d.original_lost = lost;
        d.props = crate::props::UserProps::from_bundle(bundle);
        if has_original {
            d.touched = bundle
                .passthrough
                .get(TOUCHED)
                .and_then(|t| serde_json::from_slice::<Touched>(t).ok())
                .map_or_else(kiln_io_pptx::TouchedParts::all, Into::into);
        }
        if let Some(s) = bundle
            .passthrough
            .get(SETTINGS)
            .and_then(|b| serde_json::from_slice::<Settings>(b).ok())
        {
            d.show_settings = s.show;
            d.guides = s.guides;
        }
        Ok(d)
    }

    /// Starts or stops recording for the journal.
    pub(super) fn set_journaling(&mut self, on: bool) {
        self.journal = on.then(Vec::new);
    }

    /// The transactions recorded since the last call.
    pub(super) fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        self.journal.as_mut().map(std::mem::take)
    }

    /// Applies one recorded transaction (recovery).
    pub(super) fn apply_journal(&mut self, op: &[u8]) -> Result<()> {
        let tx = Transaction::from_bytes(op).map_err(err)?;
        self.pres.apply(tx.clone()).map_err(err)?;
        self.pres.refresh_id_counter();
        self.after_history(&tx);
        Ok(())
    }
}

/// the slides of a local version marked against the version before it.
pub(super) fn marks(
    before: Option<&[u8]>,
    version: &[u8],
) -> Result<Vec<kiln_protocol::collab::VersionSlide>> {
    use kiln_protocol::collab::{VersionSlide, VersionSlideMark};
    let before = before
        .map(Presentation::from_bytes)
        .transpose()
        .map_err(err)?;
    let now = Presentation::from_bytes(version).map_err(err)?;
    Ok(kiln_collab::changed_slides(before.as_ref(), &now)
        .into_iter()
        .map(|c| VersionSlide {
            slide: c.slide,
            mark: match c.mark {
                kiln_collab::SlideMark::Same => VersionSlideMark::Same,
                kiln_collab::SlideMark::Added => VersionSlideMark::Added,
                kiln_collab::SlideMark::Changed => VersionSlideMark::Changed,
            },
        })
        .collect())
}
