# kiln-protocol

The single contract between the Rust core and the TypeScript presentation layer: `Command`, `CommandMeta`, `Event`, Scene types, `Viewport`, `Selection`, `A11yTree`, `DocSummary` and `HostRequest`, exported to TypeScript with `ts-rs`. This crate is a scaffold created by WP-50: it builds, is linted and tested in CI, and carries no engine logic yet; the work package that owns it in PLAN.md §11 fills it in.
