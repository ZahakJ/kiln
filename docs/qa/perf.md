# Performance measurements

Results for the budgets of PLAN.md §3.9. `cargo xtask budgets` (WP-60) will write the automated rows; until
then this file records manual measurements, each with its date, machine and command.

Machine: the development machine of docs/qa/env.md (Ubuntu 24.04.5, 4 CPUs, 7.5 GB RAM), Rust 1.99.1,
`.cargo/config.toml` as committed (3 jobs, sccache, dependencies at `opt-level = 2`).

## B15 — clean build of the workspace (budget < 6 min on this machine)

| Date | Work package | Scope | Command | Time | Notes |
|---|---|---|---|---|---|
| 2026-10-15 | WP-50 | first build after creating the workspace: 30 core crates + xtask, all targets (tests included), dependencies downloaded but never compiled, sccache empty | `cargo build --workspace --exclude kiln-desktop --all-targets --timings` | 55.2 s | timing report: `target/cargo-timings/` |
| 2026-10-15 | WP-50 | clean build into a fresh target directory, same scope | `CARGO_TARGET_DIR=<empty dir> cargo build --workspace --exclude kiln-desktop --all-targets` | 41.5 s | 17 of 94 compile requests were cacheable (incremental dev builds bypass sccache); 0 hits because the paths differ from the first build |

`kiln-desktop` (Tauri and WebKitGTK, several hundred crates) is not part of these numbers: its system
libraries are not installed on this machine (docs/qa/env.md). CI builds it in the `desktop-smoke` job;
the CI pipeline duration (budget < 25 min) is read from the `ci.yml` run of each push.

The crates are scaffolds at this point, so these times are the floor; every engine work package that adds
dependencies re-measures and records the new clean-build time here.
