# Test fonts

Fonts used only by the `kiln-text` integration tests so that Arabic and CJK snapshots are identical on every
machine (the tests disable system fonts). Both are licensed under the SIL Open Font License 1.1 (`OFL.txt`).
They are not shipped.

| File | Source | Notes |
|---|---|---|
| `NotoNaskhArabic-Regular.ttf` | Debian/Ubuntu `fonts-noto-core` (Noto Naskh Arabic, version 2.013) | unchanged |
| `NotoSansCJKsc-Subset.otf` | Debian/Ubuntu `fonts-noto-cjk` 20230817 (`NotoSansCJK-Regular.ttc`, face 2: Noto Sans CJK SC 2.004) | subset with fontTools to printable ASCII plus the characters in `cjk-subset-chars.txt` (every CJK character used by the tests); all layout features kept |

To regenerate the subset after adding CJK test text, collect the characters from `tests/*.rs` into
`cjk-subset-chars.txt` and run `pyftsubset`-equivalent code (fontTools `Subsetter` with `layout_features=['*']`,
`notdef_outline=True`) on face 2 of the collection.
