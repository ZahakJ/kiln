# DOCX reader oracle texts

`<name>.mammoth.txt` is the raw text mammoth (an independent `.docx` reader, dev-only) extracts from
`fixtures/docx/features/<name>.docx`, with simple fields unwrapped into their results first (mammoth
ignores `w:fldSimple`). `crates/kiln-io-docx/tests/read_mammoth.rs` compares the Kiln reader's body
text with them paragraph by paragraph (threshold 0.99; every feature fixture scores 1.0).

Regenerate after changing the feature corpus, and check freshness in CI or by hand:

```sh
node packages/testing/scripts/mammoth-docx-text.mjs
node packages/testing/scripts/mammoth-docx-text.mjs --check
```
