# @officina/tokens

Design tokens of the "Horizon" design language (PLAN.md §1.2): CSS custom properties (`--of-*`) for the light, dark and high-contrast themes and the five app accents, the same values as a typed TS object, a Tailwind 4 `@theme` block, the UI fonts (Inter Variable, JetBrains Mono) and the bundled, metric-compatible document fonts (Liberation Sans/Serif/Mono, Carlito, Caladea).

**Public API.**

- CSS: `@officina/tokens/index.css` (fonts + tokens, import once per app), `tokens.css`, `fonts.css`, and `theme.css` (Tailwind 4: import after `tailwindcss`). Themes: `<html data-theme="light|dark|hc">` (absent = follow `prefers-color-scheme`); accent: `<html data-app="write|grid|deck|notes|hub">` sets `--of-accent` / `--of-accent-strong`. Both attributes also work on nested elements.
- TS: `colors`, `scale`, `derived`, `cssVar()`, `contrastRatio()`, `DOCUMENT_FONTS`; theme runtime `applyTheme()`, `resolveTheme()`, `createThemeStore()`, `loadThemePreference()` / `saveThemePreference()` and `THEME_BOOT_SCRIPT` (inline in `<head>` to avoid a wrong-theme flash). React bindings (`ThemeProvider`, `useTheme`) live in `@officina/ui-kit` so this package stays framework-free.

**Workflow.** `src/tokens.ts` is the single source of truth; `pnpm --filter @officina/tokens generate` rewrites the three CSS files and a test fails if they drift. Contrast of text tokens (≥ 4.5:1 in every theme) is unit-tested. Fonts are fetched from their upstream repositories by `node scripts/fetch-fonts.mjs` (plain Node, SHA-256 pinned in `fonts/pins.json`); `--check` verifies the committed files and runs before the tests. Their OFL licences are listed in `fonts/NOTICES.json`, which `pnpm licenses:notices` folds into THIRD-PARTY-NOTICES.

Licence: AGPL-3.0-or-later (code); fonts SIL OFL 1.1.
