# Project context for the orchestrating session

What the project owner has asked for, in their own priorities. Treat these as
standing instructions when continuing the build.

## Goal

A full, open-source replacement for the Microsoft Office suite (Word, Excel,
PowerPoint, OneNote) on Windows, Linux, macOS, Android (APK) and the web. Not
a 1:1 clone: it must cover the useful features people would actually miss
from Office, and measurably beat Microsoft Office **and** the other
open-source suites and AI-built clones (LibreOffice, ONLYOFFICE, Univer,
WordCraft/GridCraft/DeckCraft) on quality, fidelity and efficiency.

## Standing instructions

- **Keep going without waiting for approval.** After v0.1, run the parity
  rounds of PLAN.md §7 (gaps from `parity/parity.json`, the fidelity corpus and
  the competitor benchmark → new work packages → build, review, commit, push,
  tag) until every critical and commonly-missed row is done or better and the
  competitor bars are beaten. VBA and Access are out of scope. Report progress
  briefly.
- **Principled, efficient stack.** Rust for every engine (models, layout,
  formulas, file formats, CRDT), Tauri 2 for desktop and Android, WebAssembly
  for the web; the TypeScript/React layer stays thin chrome with no business
  logic. Efficiency budgets in PLAN.md §3.9 are enforced in CI.
- **Its own personality, not a cheap Office clone.** The brand is Kiln
  (Clay direction, `brand/BRAND.md`): Kiln Write (cobalt), Kiln Grid (jade),
  Kiln Deck (ember), Kiln Notes (saffron), Kiln Home. Tagline "Where work gets
  made." Familiar ribbon and shortcuts, but warm and premium. Visual work is
  reviewed by rendering and looking at it.
- **Releases.** v0.1.0 with Windows (exe/msi), macOS (dmg), Linux
  (AppImage/deb), Android APKs and a web build on GitHub Releases, then a
  release per parity round. Windows/macOS are unsigned unless certificates are
  provided; the Android release key goes in `~/.kiln-keys` and a GitHub secret
  (tell the owner to back it up).
- **GitHub presentation** must match the owner's other projects
  (zahakj.github.io/concord, zahakj.github.io/astrolabe, repos concord,
  astrolabe, abax): centred `<picture>` banner (dark + light), badge row,
  plain understated prose without emoji or hype, screenshots, bold-lead
  feature bullets, "Compared with" section, Building, docs table, Licence.
  The Pages site uses **real application screenshots only** (never mockups),
  a measured comparison table against Microsoft 365, Google Workspace,
  LibreOffice, ONLYOFFICE, Univer and the Craft apps, light and dark, phone
  widths, Lighthouse ≥ 95.
- **Honesty.** Never claim numbers that were not measured; record every loss
  against a competitor as a P0 parity row.
- **Outward-facing actions** (new repos, secrets, releases outside the plan)
  and anything a permission prompt refuses go back to the owner; do not work
  around refusals.
- **Look after the machine.** Keep a monitor on disk, thermal throttling and
  memory; clean up build output and stale worktrees; do not let the disk fill.

## Licence

AGPL-3.0-or-later for Kiln; dependencies must be permissive (checked by
`cargo deny` and `pnpm licenses:check`).
