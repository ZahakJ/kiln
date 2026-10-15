# @officina/ui-kit

Office-familiar UI building blocks shared by every app, built on Radix, Lucide and the Officina Glyphs set, styled entirely with `@officina/tokens` so every theme and app accent works.

**Public API.**

- Primitives (`src/primitives`): `Button` (default, primary, ghost, toolbar, split, danger; stacked ribbon layout), `IconButton`, `ToggleButton`, `Menu`/`MenuItem`/`MenuCheckboxItem`/`MenuRadioItem`/`Submenu`, `Popover`, `Tooltip` (title + shortcut + description), `Dialog`, `Tabs`, `Select`, `Combobox` (editable font family/size box with inline completion), `NumberInput` (pt/px/cm/mm/in with typed-unit conversion), `ColorPicker` (10×6 theme tints, standard and recent colours, custom), `Slider` and `ZoomSlider`, `Toolbar` (roving tabindex), `SplitButton`, `Gallery` (in-ribbon gallery with expand and live preview), `Separator`, `Kbd`, `Toast`/`useToast`, `Spinner`, `VisuallyHidden`, `ScrollArea`; helpers `formatShortcut`, `ariaKeyShortcuts`, `parseLength`, `convertLength`, `applyTint`, `themeVariants`.
- Icons (`src/icons`): `Icon` renders a Lucide icon or a glyph by name; 275 glyphs exported by name from `src/icons/glyphs/index.ts` (20×20, 1.5 px stroke, `currentColor`, optional colourful style via `data-icon-style="colorful"`); `REQUIRED_GLYPHS` lists the glyphs the §1.4 ribbon specs use.
- Theme (`src/theme`): `ThemeProvider` (applies `data-theme`/`data-app`, persists the choice) and `useTheme()`.
- Office chrome (PLAN.md §1.3–1.5), bound to the `@officina/commands` registry through `CommandsProvider`
  (hooks `useCommandState`, `useCommandShortcut`, `useExecute`…):
  - `AppFrame`: title bar (app mark, AutoSave switch, Save/Undo/Redo, inline rename, Search/Tell me box,
    presence slot, Share, window-controls slot), ribbon, side panes, status bar; wires the global
    shortcuts, Key Tips (Alt / F10 / Alt+letter), the command palette, the Ctrl+/ cheat sheet, the
    Backstage, File ▸ Options and the theme/ribbon commands.
  - `Ribbon` from a `RibbonSpec` (`ribbon`, `tab`, `group`, `button`, `toggle`, `split`, `dropdown`,
    `gallery`, `combobox`, `color`, `numberInput`, `spacer`, `custom`, `row` builders): Classic 3-row and
    Simplified 1-row layouts (automatic at 1100 px), collapse to tabs (Ctrl+F1) with a floating panel,
    contextual tab sets, groups that collapse (Classic) or overflow into "More" (Simplified), dialog
    launchers, ScreenTips with shortcut and Office name, Key Tips attributes, tablist + toolbar keyboard
    model. `validateRibbonSpec()` checks command ids and icons for tests.
  - `Backstage` (Office-order navigation, pages are slots), `StatusBar` (customisable items, view
    switcher, zoom slider with a presets popover), `CommandPalette` (cmdk; fuzzy over titles, Office
    names and synonyms; Recent), `TaskPaneHost` (left/right docking, resizable, tabbed), `MiniToolbar` and
    `useSelectionMiniToolbar`, `ContextMenu` (pointer, Shift+F10, optional mini toolbar),
    `KeyTipsLayer`, `DialogLauncher`, `OptionsDialog` (General, Display, Proofing, Save, Language,
    Keyboard with live remapping, Privacy) with the persisted `useOfficeOptions()` store, and
    `ShortcutSheet`.
  - Icons by name: `resolveCommandIcon()` / `CommandIcon` (glyphs + a curated Lucide set; extend with
    `registerCommandIcons`).

Keyboard and ARIA behaviour follows PLAN.md §3.10 and is covered by Vitest + jsdom tests in `test/` and
next to each chrome component in `src/`.

**Gallery.** `pnpm --filter @officina/ui-kit gallery` serves `stories/` (every primitive; `?theme=light|dark|hc`, `?app=…`, `?view=matrix`, `?icons=colorful`). `pnpm --filter @officina/ui-kit gallery:shots [dir]` captures review screenshots (themes, phone width, open menus/panels/dialogs) with Playwright. `GalleryPage` and `ThemeMatrix` are exported from `@officina/ui-kit/stories`.

**Chrome demo.** `?view=chrome` in the gallery shows a Scriba-like window wired to the real registry,
keymap and a full Home/Insert/Layout/Review/View ribbon (`stories/Chrome.tsx`); `pnpm --filter
@officina/ui-kit chrome:shots [dir]` captures light/dark, desktop/phone screenshots of every chrome part.

**Note for apps.** Packages install their own `node_modules`; bundlers must dedupe React (`resolve.dedupe: ['react', 'react-dom']`), as `vitest.config.ts` and `stories/vite.config.ts` do.

Licence: AGPL-3.0-or-later.
