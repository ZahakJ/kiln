// SPDX-License-Identifier: AGPL-3.0-or-later
//! Suite-wide commands: files, undo/redo, view and diagnostics. Owned by WP-52; later suite work
//! packages append variants and rows here.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::{AppMask, DocKind, FileFormat};
use crate::viewport::ViewMode;

/// Suite-wide commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum SuiteCommand {
    /// Open a file. With `path: None` the host shows its Open dialog first.
    Open {
        /// The file to open; `None` asks the host to pick one.
        path: Option<String>,
    },
    /// Create a new, empty document.
    New {
        /// The kind of document.
        kind: DocKind,
    },
    /// Save the document to its path, or ask for one (Save As) if it has none.
    Save,
    /// Save the document under a new path and/or format.
    SaveAs {
        /// The target path; `None` asks the host to pick one.
        path: Option<String>,
        /// The format; `None` keeps the current format (or uses the path's extension).
        format: Option<FileFormat>,
    },
    /// Close the document (the UI asks about unsaved changes before sending this).
    Close,
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Set the zoom factor (1.0 = 100 %), between 0.1 and 5.0 like Office.
    SetZoom {
        /// The zoom factor.
        zoom: f32,
    },
    /// Switch the document view (Print Layout, Draft, Normal, Slide Sorter…).
    SetViewMode {
        /// The view.
        mode: ViewMode,
    },
    /// The surface lost its painted tiles (remounted, canvas context lost): the next frame
    /// resets the painter and resends everything visible (WP-54).
    Repaint,
    /// Round trip with no effect; the reply carries the core and protocol versions.
    Ping,
    /// Round trip that returns its text (bridge diagnostics).
    Echo {
        /// Any text.
        text: String,
    },
}

crate::command_table! {
    SuiteCommand => SUITE_COMMANDS;
    Self::Open { .. } => {
        id: "suite.file.open", keytip: Some("O"), group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::New { .. } => {
        id: "suite.file.new", keytip: Some("N"), group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::Save => {
        id: "suite.file.save", keytip: Some("S"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::SaveAs { .. } => {
        id: "suite.file.save_as", keytip: Some("A"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Close => {
        id: "suite.file.close", keytip: Some("C"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Undo => {
        id: "suite.edit.undo", keytip: None, group: "edit",
        tier: P0, apps: AppMask::DOCUMENTS, availability: CanUndo,
    };
    Self::Redo => {
        id: "suite.edit.redo", keytip: None, group: "edit",
        tier: P0, apps: AppMask::DOCUMENTS, availability: CanRedo,
    };
    Self::SetZoom { .. } => {
        id: "suite.view.zoom", keytip: Some("Q"), group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::SetViewMode { .. } => {
        id: "suite.view.mode", keytip: None, group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Repaint => {
        id: "suite.view.repaint", keytip: None, group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Ping => {
        id: "suite.dev.ping", keytip: None, group: "dev",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::Echo { .. } => {
        id: "suite.dev.echo", keytip: None, group: "dev",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
}

impl SuiteCommand {
    /// The smallest and largest zoom factors (10 %–500 %, as in Office).
    pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.1..=5.0;
}
