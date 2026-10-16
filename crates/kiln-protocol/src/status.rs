// SPDX-License-Identifier: AGPL-3.0-or-later
//! Status bar fields, computed by the core (PLAN.md §1.3).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::viewport::ViewMode;

/// "Page 3 of 12", "Slide 2 of 9".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Position {
    /// One-based current item.
    pub current: u32,
    /// Total items.
    pub total: u32,
}

/// Grid statistics of the selected cells (Average, Count, Numerical Count, Min, Max, Sum).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct CellStats {
    /// Non-empty cells.
    pub count: u32,
    /// Numeric cells.
    pub numeric_count: u32,
    /// Sum of the numeric cells.
    pub sum: f64,
    /// Average of the numeric cells (`None` without numbers).
    pub average: Option<f64>,
    /// Smallest number.
    pub min: Option<f64>,
    /// Largest number.
    pub max: Option<f64>,
}

/// Everything the status bar shows for a document. Fields an app does not use are `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatusBar {
    /// Write: page position.
    pub page: Option<Position>,
    /// Write/Notes: word count of the document (or of the selection when one exists).
    pub words: Option<u32>,
    /// Write/Notes: character count, as Word counts it (including spaces).
    pub chars: Option<u32>,
    /// Proofing language tag at the caret, e.g. `en-US`.
    pub language: Option<String>,
    /// Grid: statistics of the selection.
    pub cells: Option<CellStats>,
    /// Deck: slide position.
    pub slide: Option<Position>,
    /// Zoom factor.
    pub zoom: f32,
    /// The view.
    pub view: ViewMode,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            page: None,
            words: None,
            chars: None,
            language: None,
            cells: None,
            slide: None,
            zoom: 1.0,
            view: ViewMode::PrintLayout,
        }
    }
}
