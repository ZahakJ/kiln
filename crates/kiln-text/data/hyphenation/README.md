# Hyphenation dictionaries

`en-us.standard.bincode` and `de-1996.standard.bincode` are the prebuilt Knuth–Liang dictionaries shipped
in the `hyphenation` 0.8.4 crate (`dictionaries/`, MIT OR Apache-2.0), copied unchanged so `kiln-text`
embeds only the two languages it needs instead of the crate's `embed_all` set (≈ 3 MB).

The patterns come from the TeX `hyph-utf8` project:

- `hyph-en-us` (Knuth and Liang, 1977): "Copying and distribution of this file, with or without
  modification, are permitted in any medium without royalty provided the copyright notice and this notice
  are preserved."
- `hyph-de-1996` (German, reformed orthography, Trennmuster project): MIT licence.

Further languages can be added the same way: copy the crate's `<code>.standard.bincode` file here and
extend `HyphLang` in `src/hyphen.rs`.
