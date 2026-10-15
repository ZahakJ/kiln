# @officina/commands

The command system every Officina app registers into (PLAN.md §1.3, §1.7, §2.1). It is UI-free;
`@officina/ui-kit` renders it as the ribbon, menus, palette, Key Tips and Options ▸ Keyboard.

**Public API.**

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
- `tiers.ts`: P0/P1/P2 gating from `OFFICINA_TIERS` / `VITE_OFFICINA_TIERS` and release mode; release
  builds hide P1/P2 and pending commands.
- `RecentCommands` (local only, no telemetry), `createOfficeSynonyms()`, `searchCommands()` / `fuzzyScore()`
  (titles, Office names and synonyms).
- `AssistantProvider` (summarise, rewrite, formula from text, slides from outline) with `NullProvider`
  (the default: "no assistant configured") and the local `EchoProvider` sample; no network calls.
- `parseShortcutDoc()` / `expandShortcutExpression()` read docs/SHORTCUTS.md; `keymaps.test.ts` checks
  every row of the document against the keymaps.

Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`. Consumed from source through the `@officina/*` paths.

Licence: AGPL-3.0-or-later.
