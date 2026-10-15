# @officina/ui-kit

Office-familiar UI building blocks shared by every app, built on Radix, Lucide and the Officina Glyphs set, styled entirely with `@officina/tokens` so every theme and app accent works.

**Public API.**

- Primitives (`src/primitives`): `Button` (default, primary, ghost, toolbar, split, danger; stacked ribbon layout), `IconButton`, `ToggleButton`, `Menu`/`MenuItem`/`MenuCheckboxItem`/`MenuRadioItem`/`Submenu`, `Popover`, `Tooltip` (title + shortcut + description), `Dialog`, `Tabs`, `Select`, `Combobox` (editable font family/size box with inline completion), `NumberInput` (pt/px/cm/mm/in with typed-unit conversion), `ColorPicker` (10×6 theme tints, standard and recent colours, custom), `Slider` and `ZoomSlider`, `Toolbar` (roving tabindex), `SplitButton`, `Gallery` (in-ribbon gallery with expand and live preview), `Separator`, `Kbd`, `Toast`/`useToast`, `Spinner`, `VisuallyHidden`, `ScrollArea`; helpers `formatShortcut`, `ariaKeyShortcuts`, `parseLength`, `convertLength`, `applyTint`, `themeVariants`.
- Icons (`src/icons`): `Icon` renders a Lucide icon or a glyph by name; 275 glyphs exported by name from `src/icons/glyphs/index.ts` (20×20, 1.5 px stroke, `currentColor`, optional colourful style via `data-icon-style="colorful"`); `REQUIRED_GLYPHS` lists the glyphs the §1.4 ribbon specs use.
- Theme (`src/theme`): `ThemeProvider` (applies `data-theme`/`data-app`, persists the choice) and `useTheme()`.

Keyboard and ARIA behaviour follows PLAN.md §3.10 and is covered by Vitest + jsdom tests in `test/`.

**Gallery.** `pnpm --filter @officina/ui-kit gallery` serves `stories/` (every primitive; `?theme=light|dark|hc`, `?app=…`, `?view=matrix`, `?icons=colorful`). `pnpm --filter @officina/ui-kit gallery:shots [dir]` captures review screenshots (themes, phone width, open menus/panels/dialogs) with Playwright. `GalleryPage` and `ThemeMatrix` are exported from `@officina/ui-kit/stories`.

**Note for apps.** Packages install their own `node_modules`; bundlers must dedupe React (`resolve.dedupe: ['react', 'react-dom']`), as `vitest.config.ts` and `stories/vite.config.ts` do.

Licence: AGPL-3.0-or-later.
