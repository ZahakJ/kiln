# @kiln/i18n

Kiln's interface languages: English (the source), German and Arabic. i18next with ICU messages
(i18next-icu, intl-messageformat), the language preference, the text direction of the chrome, and
locale-aware formatting of numbers, dates and lists through `Intl`.

Only interface text lives here. Document text comes from the core, and Excel function names stay as
Excel spells them.

**Messages.** `locales/en.json` maps each key to its English message. A key is either a command's
`cmd.<id>` (its title) and `cmd.<id>.description` (its ScreenTip), a structured key for messages
with plurals (`status.words`), or the English text itself. `locales/de.json` and `locales/ar.json`
carry every key; their header entry (`//`) marks them as machine-drafted and needing review.

**API.** `t(key, values)` formats a message in the interface language; `tx(text)` translates text
kept as data when it is a message and returns anything else unchanged; `msg('…')` marks a
module-level string as a message without translating it; `setLanguage(pref)` switches the language
(`system`, `en`, `de`, `ar`) and sets `lang` and `dir` on the root element; `formatNumber`,
`formatCount`, `formatDate`, `formatList`, `languageName` and `localizeNumberText` format for the
interface language. `@kiln/i18n/react` has `useLanguage`, `useT`, `useDirection` and `Message` (a
message with markup in its placeholders). `@kiln/i18n/fonts.css` adds the Arabic faces of the
interface fonts (Noto Sans Arabic, Noto Naskh Arabic).

**Tooling.**

- `pnpm --filter @kiln/i18n extract` rewrites `locales/en.json` from the `t()`/`msg()` calls of the
  interface and the command system's messages (`@kiln/commands` `commandMessages`).
- `tsx scripts/check-locale.ts de` lists translated messages whose ICU syntax, placeholders or
  plural forms do not match English.
- `pnpm --filter @kiln/i18n test` checks that English matches the sources, that German and Arabic
  have every English key with the same placeholders and the plural categories of their language
  (six in Arabic), and that no TSX file of the interface shows hard-coded English (the guard: JSX
  text, user-facing attributes and properties, announcements). Mark a string that is not interface
  text (a font sample, a format code) with an `i18n-ignore` comment on its line or the line before.
