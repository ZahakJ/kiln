# @kiln/editor-deck

Kiln Deck's editor views: the slide stage, the thumbnail rail with sections
and drag-to-reorder, the speaker notes, the slide sorter, the context menus, the status bar and the dialogs
(Slide Size, Format Background, Insert Table, Header and Footer, Size and Position, Alt Text, Link, Find,
Replace, Replace Fonts, Crop, Paste Special, the Selection pane and the Shapes gallery). The web app mounts it in
the app frame (`apps/web/src/routes/deck`).

Presentation only. The slide is Rust tiles from `kiln-deck-layout`, painted on a canvas by
`@kiln/surface`'s painter with the core's overlay (selection, handles, smart guides, caret). Pointer gestures go
to the core in slide coordinates (`deck.stage.pointer_down/move/up`), which decides whether they select, move,
resize, rotate, draw or place the caret; the cursor over a point is the core's answer to `Query::Deck(HitTest)`.
A hidden textarea at the caret is the input proxy for typing and IME. The stage's own keys (`keys.ts`: caret
motion, Backspace and Delete, Enter, Tab, Esc, nudging with the arrows) go straight to the core in order with the
typing; every other shortcut is the window's keymap, with the `object`, `text-edit` and `rail` scopes set from the
core's state. Thumbnails are the core's (`Query::Deck(Thumbnails)`), cached per slide until its version changes.

Public API:

- `useDeckEditorModel({ client, doc, registry })` — the editor model of a presentation, with the editor's chrome
  commands (`registerDeckUi`) registered while mounted.
- `DeckEditor` — rail, stage, notes, menus and dialogs: `<DeckEditor model title theme />`.
- `DeckStatusBar`, `deckRibbonWithLayouts` (New Slide ▾ and Layout ▾ with the presentation's layouts).
- `DeckStageController`, `ThumbnailRail`, `SlideSorter`, `NotesPane` and the pure helpers (`deckKeyAction`,
  `railKeyAction`, `fitView`, `clampScroll`, `railRows`, `dropIndex`) for tests and reuse.

The stage element mirrors its frame count and last frame time as `data-kiln-frames` and `data-kiln-frame-ms` and
records `kiln:deck-frame` measures. In development builds the web app exposes the model as `window.__kilnDeck`
and the stage as `window.__kilnDeckStage` for `e2e/specs/deck.spec.ts` and `e2e/specs/deck-shortcuts.spec.ts`.
