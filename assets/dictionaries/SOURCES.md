# Bundled spelling dictionaries

`kiln-spell` checks spelling with [spellbook](https://github.com/helix-editor/spellbook) 0.4.2 (a
Hunspell-compatible engine, MPL-2.0) and embeds the dictionaries below in the core, so spell checking
works offline on every platform and in the web build. Text in other languages is not checked (PLAN.md
§2.2, WP-66).

| Directory | Language | Dictionary | Licence | Upstream |
|---|---|---|---|---|
| `en-US/` | English (United States) | Hunspell `en_US` from SCOWL, version 2020.12.07 with the LibreOffice project's 2024-01-29 affix update | SCOWL licence (permissive, BSD-like; see `en-US/README_en_US.txt`, section "COPYRIGHT, SOURCES, and CREDITS") | LibreOffice dictionaries repository, `en/` |

## Download

The files were taken unchanged from the LibreOffice dictionaries repository at commit
`67032ee957631908098a72edf48dc81cac27e369`:

- `https://raw.githubusercontent.com/LibreOffice/dictionaries/67032ee957631908098a72edf48dc81cac27e369/en/en_US.aff`
- `https://raw.githubusercontent.com/LibreOffice/dictionaries/67032ee957631908098a72edf48dc81cac27e369/en/en_US.dic`
- `https://raw.githubusercontent.com/LibreOffice/dictionaries/67032ee957631908098a72edf48dc81cac27e369/en/README_en_US.txt`

The `en/` directory's `license.txt` (GPL-2.0) covers other components of that directory (the thesaurus
and hyphenation patterns), which Kiln does not ship; the `en_US` word list and affix file are under the
SCOWL licence quoted in `README_en_US.txt`.

## Checksums

Verify with `cd assets/dictionaries && sha256sum -c SOURCES.md` (the lines below are in `sha256sum`
format; the other lines of this file are ignored with a warning).

```
e746c882dd6f303c2c46e7452804b9201115a6942cfeb15f18f8edf774d2e24e  en-US/en_US.aff
f0b1a234bd178bdd01875b2a392a9647f888b8fe879f79c52aae62c2759b3647  en-US/en_US.dic
168b4c01cb841f766a72f310b065c04d09cff46376c37a81d799593ce751c371  en-US/README_en_US.txt
```
