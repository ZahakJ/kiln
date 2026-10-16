// SPDX-License-Identifier: AGPL-3.0-or-later
//! The result of executing a command.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::host::RequestId;
use crate::summary::DocSummary;

/// What a command did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Outcome {
    /// The command ran and changed the document or session.
    Applied,
    /// The command ran but nothing changed (caret already at the end, zoom unchanged…).
    Unchanged,
    /// The command waits for the host (a dialog or file read); the result arrives as an event.
    Pending(RequestId),
}

/// Information returned by the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreInfo {
    /// The core's version (the suite version).
    pub version: String,
    /// The protocol version ([`crate::PROTOCOL_VERSION`]); the UI refuses a mismatch.
    pub protocol: u32,
    /// The target the core was built for, e.g. `wasm32` or `x86_64-linux`.
    pub target: String,
}

/// A value some commands return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ExecValue {
    /// No value.
    #[default]
    None,
    /// The reply to `Ping`.
    Pong(CoreInfo),
    /// The reply to `Echo`.
    Echo(String),
}

/// The result of `execute`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExecResult {
    /// What happened.
    pub outcome: Outcome,
    /// The target document after the command (also for `New`), or `None` for session commands and
    /// for a document the command closed.
    pub summary: Option<DocSummary>,
    /// A returned value.
    pub value: ExecValue,
}
