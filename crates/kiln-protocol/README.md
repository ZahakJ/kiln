# kiln-protocol

The single contract between the Rust core and the TypeScript presentation layer (PLAN.md §3.5, `docs/adr/0005-bridge.md`). Every type derives `serde` and `ts_rs::TS`:

- `Command` = `Suite(SuiteCommand) | Write(WriteCommand) | Grid(GridCommand) | Deck(DeckCommand) | Notes(NotesCommand)`, one file per app in `src/commands/`, each with its `CommandMeta` table written with `command_table!` (id `<app>.<group>.<name>`, label key, Key Tip, group, `Tier`, `AppMask`, `Availability`). `catalog()` flattens the tables into `catalog.json`.
- `Event`, `ExecResult`, `Query`/`QueryResult`, `Viewport`, `Selection` (`TextPos`, `Caret`, `Handle`), `StatusBar`, `DocSummary`, `A11yTree`, `HostRequest`/`HostOp`/`HostReply`, `ErrorInfo`, `OpenHint`, and `scene::SceneDelta` (a placeholder until WP-54 re-exports `kiln-scene`).
- `PROTOCOL_VERSION`, checked by the UI on `Ping`.

The `schema` feature adds `samples` (representative values of every type) and `schema` (the generators behind `cargo xtask protocol`: TypeScript files, `catalog.json`, `schema.json` and the postcard vectors in `tests/vectors/`). A unit test checks that the committed vectors are current and decode.

Adding a command: add the variant to `src/commands/<app>.rs`, add its row to the table in the same file, add a sample to `src/samples.rs::commands()` (a test fails otherwise), then run `cargo xtask protocol` and commit the generated files.
