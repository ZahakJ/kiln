// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands: the only way the UI changes anything in the core.
//!
//! [`Command`] is an enum of per-app sub-enums, each in its own file together with that app's
//! [`CommandMeta`] table, so the work packages that own an app never edit the same file:
//!
//! | File | Sub-enum | Table |
//! |---|---|---|
//! | `suite.rs` | [`SuiteCommand`] | [`SUITE_COMMANDS`] |
//! | `write.rs` | [`WriteCommand`] | [`WRITE_COMMANDS`] |
//! | `grid.rs` | [`GridCommand`] | [`GRID_COMMANDS`] |
//! | `deck.rs` | [`DeckCommand`] | [`DECK_COMMANDS`] |
//! | `notes.rs` | [`NotesCommand`] | [`NOTES_COMMANDS`] |
//!
//! Tables are written with the [`command_table!`](crate::command_table) macro, which generates the
//! static table and a `meta()` method whose `match` is exhaustive, so a variant without metadata is
//! a compile error. [`catalog`] flattens every table into the `catalog.json` that the TypeScript
//! command registry, ribbon, palette and Key Tips read.

mod deck;
mod grid;
mod notes;
mod suite;
mod write;

pub use deck::{DECK_COMMANDS, DeckCommand};
pub use grid::{GRID_COMMANDS, GridCommand};
pub use notes::{NOTES_COMMANDS, NotesCommand};
pub use suite::{SUITE_COMMANDS, SuiteCommand};
pub use write::{CaretMotion, SelectUnit, WRITE_COMMANDS, WriteCommand};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::{App, AppMask};

/// Delivery tier of a command. A build shows commands up to its tier and hides the rest (never
/// greys them out, PLAN.md §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Tier {
    /// Ships in v0.1.
    P0,
    /// Follows in the first parity rounds.
    P1,
    /// Later or niche.
    P2,
}

/// When a command can run. The session checks this before dispatching; the UI greys out or hides
/// controls from the `can_*` flags in `Event::Changed`, never from its own logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Availability {
    /// Always, with or without a document (New, Open, Ping).
    Always,
    /// Needs an open document of one of the command's apps.
    Document,
    /// Needs a document that is not read-only.
    Editable,
    /// Needs something to undo.
    CanUndo,
    /// Needs something to redo.
    CanRedo,
    /// Needs a non-empty selection.
    Selection,
}

impl Availability {
    /// Whether the command needs a target document.
    #[must_use]
    pub const fn needs_document(self) -> bool {
        !matches!(self, Availability::Always)
    }
}

/// Static metadata of one command, declared next to the command in its app's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    /// Stable id, `<app>.<group>.<name>` (e.g. `suite.file.save`). Ribbon items, keymaps, Key
    /// Tips, palette synonyms and parity rows refer to commands by this id.
    pub id: &'static str,
    /// i18n key of the label: `cmd.` + id.
    pub label_key: &'static str,
    /// The Office Key Tip letters within the command's ribbon tab, if it has one.
    pub keytip: Option<&'static str>,
    /// Ribbon/menu group (`file`, `edit`, `view`, …).
    pub group: &'static str,
    /// Delivery tier.
    pub tier: Tier,
    /// Apps that offer the command.
    pub apps: AppMask,
    /// When it can run.
    pub availability: Availability,
}

/// One row of `catalog.json`: [`CommandMeta`] in owned, serialisable form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommandInfo {
    /// Stable id.
    pub id: String,
    /// i18n key of the label.
    pub label_key: String,
    /// Key Tip letters.
    pub keytip: Option<String>,
    /// Group.
    pub group: String,
    /// Delivery tier.
    pub tier: Tier,
    /// Apps that offer the command.
    pub apps: Vec<App>,
    /// When it can run.
    pub availability: Availability,
}

impl CommandMeta {
    /// The catalog row.
    #[must_use]
    pub fn info(&self) -> CommandInfo {
        CommandInfo {
            id: self.id.to_owned(),
            label_key: self.label_key.to_owned(),
            keytip: self.keytip.map(str::to_owned),
            group: self.group.to_owned(),
            tier: self.tier,
            apps: self.apps.apps(),
            availability: self.availability,
        }
    }
}

/// A command for the core, addressed to the app that implements it.
///
/// JSON form (serde's external tagging): `{"Suite":"Undo"}`, `{"Write":{"InsertText":{"text":"a"}}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum Command {
    /// Suite-wide commands (files, undo, view, diagnostics).
    Suite(SuiteCommand),
    /// Kiln Write commands.
    Write(WriteCommand),
    /// Kiln Grid commands.
    Grid(GridCommand),
    /// Kiln Deck commands.
    Deck(DeckCommand),
    /// Kiln Notes commands.
    Notes(NotesCommand),
}

impl Command {
    /// The command's metadata.
    #[must_use]
    pub fn meta(&self) -> &'static CommandMeta {
        match self {
            Command::Suite(c) => c.meta(),
            Command::Write(c) => c.meta(),
            Command::Grid(c) => c.meta(),
            Command::Deck(c) => c.meta(),
            Command::Notes(c) => c.meta(),
        }
    }

    /// The command's stable id.
    #[must_use]
    pub fn id(&self) -> &'static str {
        self.meta().id
    }
}

impl From<SuiteCommand> for Command {
    fn from(c: SuiteCommand) -> Self {
        Command::Suite(c)
    }
}

impl From<WriteCommand> for Command {
    fn from(c: WriteCommand) -> Self {
        Command::Write(c)
    }
}

/// Every app's command table, in catalog order.
pub const TABLES: [&[CommandMeta]; 5] = [
    SUITE_COMMANDS,
    WRITE_COMMANDS,
    GRID_COMMANDS,
    DECK_COMMANDS,
    NOTES_COMMANDS,
];

/// The metadata of a command id, if it exists.
#[must_use]
pub fn meta_by_id(id: &str) -> Option<&'static CommandMeta> {
    TABLES.iter().flat_map(|t| t.iter()).find(|m| m.id == id)
}

/// Every command's catalog row (`catalog.json`).
#[must_use]
pub fn catalog() -> Vec<CommandInfo> {
    TABLES
        .iter()
        .flat_map(|t| t.iter())
        .map(CommandMeta::info)
        .collect()
}

/// Declares a command table: the `static` slice of [`CommandMeta`] and a `meta()` method on the
/// command enum that maps every variant to its row.
///
/// ```ignore
/// command_table! {
///     SuiteCommand => SUITE_COMMANDS;
///     Self::Save => { id: "suite.file.save", keytip: Some("S"), group: "file",
///                     tier: P0, apps: AppMask::DOCUMENTS, availability: Document };
/// }
/// ```
///
/// The generated `match` lists the patterns exactly as written, so it is exhaustive: adding a
/// variant without a row does not compile, and the row index of every arm is computed by the
/// macro, so rows and arms cannot drift apart.
#[macro_export]
macro_rules! command_table {
    (
        $ty:ident => $table:ident;
        $(
            $pat:pat => {
                id: $id:literal,
                keytip: $keytip:expr,
                group: $group:literal,
                tier: $tier:ident,
                apps: $apps:expr,
                availability: $avail:ident $(,)?
            };
        )*
    ) => {
        #[doc = concat!("Metadata of every [`", stringify!($ty), "`], in declaration order.")]
        pub static $table: &[$crate::commands::CommandMeta] = &[
            $(
                $crate::commands::CommandMeta {
                    id: $id,
                    label_key: concat!("cmd.", $id),
                    keytip: $keytip,
                    group: $group,
                    tier: $crate::commands::Tier::$tier,
                    apps: $apps,
                    availability: $crate::commands::Availability::$avail,
                },
            )*
        ];

        impl $ty {
            /// The command's metadata row.
            #[must_use]
            pub fn meta(&self) -> &'static $crate::commands::CommandMeta {
                $crate::command_table!(@arms self, $table, (0usize), [] $($pat =>)*)
            }
        }
    };
    (@arms $self:ident, $table:ident, ($($idx:tt)*), [$($arms:tt)*]) => {
        match *$self {
            $($arms)*
        }
    };
    (@arms $self:ident, $table:ident, ($($idx:tt)*), [$($arms:tt)*] $pat:pat => $($tail:tt)*) => {
        $crate::command_table!(
            @arms $self, $table, ($($idx)* + 1usize),
            [$($arms)* $pat => &$table[$($idx)*],]
            $($tail)*
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_well_formed_and_prefixed_by_their_table() {
        let prefixes = ["suite.", "write.", "grid.", "deck.", "notes."];
        let mut seen = HashSet::new();
        for (table, prefix) in TABLES.iter().zip(prefixes) {
            for m in *table {
                assert!(seen.insert(m.id), "duplicate command id {}", m.id);
                assert!(
                    m.id.starts_with(prefix),
                    "{} must start with {prefix}",
                    m.id
                );
                let parts: Vec<&str> = m.id.split('.').collect();
                assert_eq!(parts.len(), 3, "{} must be <app>.<group>.<name>", m.id);
                assert!(
                    m.id.bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'_'),
                    "{} must be lowercase snake case",
                    m.id
                );
                assert_eq!(parts[1], m.group, "{}: group must match the id", m.id);
                assert_eq!(m.label_key, format!("cmd.{}", m.id));
                assert_ne!(m.apps, AppMask::NONE, "{} belongs to no app", m.id);
                if let Some(k) = m.keytip {
                    assert!(
                        !k.is_empty()
                            && k.len() <= 3
                            && k.bytes().all(|b| b.is_ascii_alphanumeric())
                    );
                }
            }
        }
    }

    #[test]
    fn meta_resolves_every_command_to_its_own_row() {
        let commands: Vec<Command> = crate::samples::commands();
        let mut covered = HashSet::new();
        for c in &commands {
            let m = c.meta();
            assert_eq!(meta_by_id(m.id), Some(m));
            assert_eq!(c.id(), m.id);
            covered.insert(m.id);
        }
        let all: HashSet<&str> = TABLES.iter().flat_map(|t| t.iter().map(|m| m.id)).collect();
        assert_eq!(covered, all, "the samples must contain every command");
    }

    #[test]
    fn catalog_lists_every_row_in_order() {
        let cat = catalog();
        assert_eq!(cat.len(), TABLES.iter().map(|t| t.len()).sum::<usize>());
        assert_eq!(cat[0].id, SUITE_COMMANDS[0].id);
        let save = cat
            .iter()
            .find(|c| c.id == "suite.file.save")
            .expect("save");
        assert_eq!(save.label_key, "cmd.suite.file.save");
        assert_eq!(
            save.apps,
            vec![App::Write, App::Grid, App::Deck, App::Notes]
        );
        assert_eq!(save.availability, Availability::Document);
        assert!(meta_by_id("suite.file.nope").is_none());
    }

    #[test]
    fn commands_use_external_tagging_in_json() {
        let undo = Command::from(SuiteCommand::Undo);
        assert_eq!(serde_json::to_string(&undo).unwrap(), r#"{"Suite":"Undo"}"#);
        let insert = Command::from(WriteCommand::InsertText { text: "a".into() });
        assert_eq!(
            serde_json::to_string(&insert).unwrap(),
            r#"{"Write":{"InsertText":{"text":"a"}}}"#
        );
        let back: Command = serde_json::from_str(r#"{"Suite":{"Echo":{"text":"hi"}}}"#).unwrap();
        assert_eq!(
            back,
            Command::Suite(SuiteCommand::Echo { text: "hi".into() })
        );
        assert!(serde_json::from_str::<Command>(r#"{"Grid":"Anything"}"#).is_err());
    }

    #[test]
    fn availability_knows_which_commands_need_a_document() {
        assert!(!Availability::Always.needs_document());
        for a in [
            Availability::Document,
            Availability::Editable,
            Availability::CanUndo,
            Availability::CanRedo,
            Availability::Selection,
        ] {
            assert!(a.needs_document());
        }
    }
}
