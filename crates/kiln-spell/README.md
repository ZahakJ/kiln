# kiln-spell

Spell checking for Kiln on [`spellbook`](https://crates.io/crates/spellbook) 0.4 (a Rust
implementation of Hunspell's checking and suggestion) with the en-US Hunspell dictionary from the
LibreOffice dictionaries repository (`assets/dictionaries/en-US`; source, revision, checksums and
licence in `assets/dictionaries/SOURCES.md`).

## What it does

- `dictionary()` — the en-US dictionary, parsed once per process. Native builds embed it;
  the web core does not (budget B12): the page fetches the two files and hands them to
  `load_dictionary(aff, dic)` (`write.spell.load_dictionary`). `has_dictionary()` says whether
  checking can run yet.
- `checks_language(tag)` — whether a run's language (`w:lang`) is one Kiln has a dictionary for
  (English (United States); no language at all counts as that, Word's default). Text in other
  languages is left unchecked rather than flagged.
- `words(text)` — the words of a paragraph's text with their char offsets (Unicode word
  boundaries, with Word's exceptions: words with digits, words in UPPERCASE, single letters and
  Internet and file addresses are skipped).
- `Speller` — one document's checker: the user's dictionary (`set_user_words`, `add_user_word`,
  `remove_user_word`, matched case-insensitively for lower-case entries), Ignore All
  (`ignore_all`, per document), `is_correct`, `misspellings(text)` and `suggest(word)` (Hunspell's
  suggestions, with a transposition first when there is one, in the word's capitalisation).
- `valid_user_word` — what Add to Dictionary accepts (one word, at most 64 characters, no spaces).

Incremental checking lives with the editor (`kiln-session`'s Write features): results are cached
per paragraph by a fingerprint of its text and languages, so typing re-checks one paragraph.

## Tests

`cargo nextest run -p kiln-spell`: flags "teh" but not "the", suggestions in order and case, the
user dictionary and Ignore All, language tags, word boundaries and loading a dictionary at run
time.
