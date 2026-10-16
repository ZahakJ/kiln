# @kiln/tokens

Design tokens for the Kiln UI (PLAN.md §1.2): CSS custom properties (`--kiln-*`) for the light, dark and high-contrast themes and the five app accents, the same values as a typed TS object, a Tailwind 4 `@theme` block and the bundled, metric-compatible document fonts (Liberation Sans/Serif/Mono, Carlito, Caladea). The values are the provisional v1.0 palette; WP-51 regenerates this package from the brand kit's `tokens.json`, including the UI fonts.

**Public API.**

- CSS: `@kiln/tokens/index.css` (fonts + tokens, import once per app), `tokens.css`, `fonts.css`, and `theme.css` (Tailwind 4: import after `tailwindcss`). Themes: `<html data-theme="light|dark|hc">` (absent = follow `prefers-color-scheme`); accent: `<html data-app="write|grid|deck|notes|hub">` sets `--kiln-accent` / `--kiln-accent-strong`. Both attributes also work on nested elements.
- TS: `colors`, `scale`, `derived`, `cssVar()`, `contrastRatio()`, `DOCUMENT_FONTS`; theme runtime `applyTheme()`, `resolveTheme()`, `createThemeStore()`, `loadThemePreference()` / `saveThemePreference()` and `THEME_BOOT_SCRIPT` (inline in `<head>` to avoid a wrong-theme flash). React bindings (`ThemeProvider`, `useTheme`) live in `@kiln/ui-kit` so this package stays framework-free.

**Workflow.** `src/tokens.ts` is the single source of truth; `pnpm --filter @kiln/tokens generate` rewrites the three CSS files and a test fails if they drift. Contrast of text tokens (≥ 4.5:1 in every theme) is unit-tested. Fonts are fetched from their upstream repositories by `node scripts/fetch-fonts.mjs` (plain Node, SHA-256 pinned in `fonts/pins.json`); `--check` verifies the committed files and runs before the tests. Their OFL licences are listed in `fonts/NOTICES.json`, which `pnpm licenses:notices` folds into THIRD-PARTY-NOTICES.

Licence: AGPL-3.0-or-later (code); fonts SIL OFL 1.1.
