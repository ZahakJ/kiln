# kiln-common

Shared foundations for every Kiln crate (PLAN.md §3.2):

- **Identifiers:** `DocId` (an open document in a session) and `NodeId` (a stable node inside a document model, referenced by layout, selection, comments and the CRDT), with `NodeIdGen` for allocation.
- **Units:** `Emu`, `Twip`, `Pt` and `Px` with exact conversions (914,400 EMU = 1,440 twips = 72 pt = 96 CSS px per inch). EMU and twips are integers as in OOXML; points and pixels are `f64`.
- **Colour:** `Color` (sRGB with alpha) with `#RRGGBB` / `#RRGGBBAA` parsing and formatting; it serialises as a hex string.
- **Geometry:** `Point`, `Rect`, `Size`, `Vec2`, `Affine`, `Insets` re-exported from `kurbo` so every crate shares one geometry vocabulary.
- **Errors:** `KilnError` with a stable machine-readable `ErrorKind`, and the `Result` alias.

Features: `serde` (serialisation of ids, units and colours) and `ts` (`ts_rs::TS` exports); `kiln-protocol` enables both. Without features the crate depends only on `kurbo` and `thiserror`.
