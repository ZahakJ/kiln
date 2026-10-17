# @kiln/commands

The command system every Kiln app registers into (PLAN.md §1.3, §1.7, §2.1, §3.7). It is UI-free;
`@kiln/ui-kit` renders it as the ribbon, menus, palette, Key Tips and Options ▸ Keyboard. It is a thin
layer over the core's generated catalog (`packages/protocol/src/generated/catalog.json`): every id it
knows is a core command, a `ui.*` chrome action, or a reservation for a command the core does not
implement yet (hidden in release builds, never greyed).

**Public API.**

- Core: `CORE_CATALOG` / `isCoreCommand` / `idKind` (the generated catalog), `buildCommand(id, args)`
  (the typed protocol `Command`, argument shapes checked, `CommandArgsError` otherwise) and
  `connectCore(registry, { client, document, log })`, which registers every core command of the window's
  app so `registry.execute(id, args)` calls `KilnClient.execute`, takes enabled state and the view/zoom
  values from `Event::Changed` (`isAvailable` reads the core's flags; nothing is computed in TS) and
  records each dispatch in a `CommandLog`.
- Ribbons as data: `RIBBONS` (`writeRibbon`, `gridRibbon`, `deckRibbon`, `notesRibbon` in
  `src/catalog/<app>.ts`, shared groups and contextual tabs in `catalog/ribbon-shared.ts`) built with
  `rtab`/`rgroup`/`rbutton`/`rtoggle`/`rsplit`/`rdropdown`/`rgallery`/`rcombo`/`rcolor`/`rnumber`/`rrow`
  and menu entries `mitem`/`msep`/`mhead`/`msub`; `ribbonCommandRefs()` lists every id a ribbon uses.
- `CommandRegistry`: `declare()` commands by id and title (pending no-ops until implemented),
  `register({ id, title, officeName?, synonyms?, icon?, run, isEnabled?, isChecked?, value?, when? })`
  (returns a disposable), `execute(id, args)` (never throws; returns `ran`, `pending`, `disabled`,
  `hidden`, `unknown` or `failed`), `getState(id)` (visible, enabled, checked, value, pending),
  `setContext()` / `invalidate()` and change subscriptions (`subscribe`, `subscribeTo(id)`, `batch`).
  `createCommandRegistry({ app })` returns one pre-loaded with the catalog, Office synonyms, the build's
  tier gate and a per-app recent-commands list.
- `COMMAND_CATALOG`: every command of §1.4 and §1.7 (over 400) with Office names, icons, tiers, Key Tips and
  ScreenTip descriptions, so menus and the palette are complete from day one.
- Keymaps: `parseShortcut('Mod+Shift+L', platform)` (Mod = Ctrl, or ⌘ on macOS; shifted symbols
  normalised the way Office binds them), `eventToChord`, `formatChord`/`formatShortcut`; `Keymap`
  resolves key events per app and scope (`slideshow`, `rail`, `object`, `cell-edit`), lists shortcuts per
  command, detects conflicts and stores the user's remapping (`assign`, `unassign`, `resetCommand`,
  `resetAll`). `keymaps/{common,write,grid,deck}.ts` are the single source of truth for docs/SHORTCUTS.md;
  `createDefaultKeymap()` builds the Office defaults.
- Key Tips: `TAB_KEYTIPS` (Office letters), `assignKeyTips()` (unique, prefix-free letters, Office's
  preferred ones first) and the `KeyTipSession` state machine.
- `tiers.ts`: P0/P1/P2 gating from `KILN_TIERS` / `VITE_KILN_TIERS` (default P0) and release mode;
  release builds hide P1/P2 and pending commands; development builds preview pending P0 commands in
  their slots (`VITE_KILN_PREVIEW_PLANNED=0` turns that off).
- `RecentCommands` (local only, no telemetry), `createOfficeSynonyms()`, `searchCommands()` / `fuzzyScore()`
  (titles, Office names and synonyms).
- `AssistantProvider` (summarise, rewrite, formula from text, slides from outline) with `NullProvider`
  (the default: "no assistant configured") and the local `EchoProvider` sample; no network calls.
- `SHORTCUT_TABLE` / `SLIDE_SHOW_TABLE` (`keymaps/table.ts`): PLAN.md §1.7 as data (57 + 7 rows);
  `keymaps.test.ts` fails unless every row is bound in every app that uses it. docs/SHORTCUTS.md is
  generated from the table and the keymaps (`shortcutsDocText()`, `renderShortcutsDoc()`), with each
  binding's catalog id; `parseShortcutDoc()` reads it back.
- `catalog.test.ts` ("every id exists in catalog.json"): every ribbon item, keymap binding, Key Tip and
  synonym names a declared id that is a core, chrome or reserved command; core commands have a
  builder and a presentation; reserved commands are hidden in release builds.

Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm shortcuts:doc` (regenerates docs/SHORTCUTS.md),
`pnpm shortcuts:check`. Consumed from source through the `@kiln/*` paths.

Licence: AGPL-3.0-or-later.
