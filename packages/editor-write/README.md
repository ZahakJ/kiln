# @kiln/editor-write

Kiln Write's editor views (WP-64, PLAN.md §1.3–§1.4): the document surface with Word's rulers, the
Navigation and Styles panes, the dialogs (Font, Paragraph, Tabs, Page Setup, Find and Replace, Insert
Table, Link, Symbol, Date and Time, Word Count, Paste Special, Bookmark, New and Modify Style), Insert ▸
Table's grid picker, the status bar, the mini toolbar and the context menu. The web app mounts it in the
app frame (`apps/web/src/routes/write`).

Presentation only (PLAN.md §3.7). Every edit is a core command run through the window's command
registry (`model.run`) or, for the document's clipboard events, sent straight to the core in order with
the typing (`model.runCore`). Everything the views show — the selection's formatting, styles, headings,
pages, rulers, Find results, counts — is an answer of `Query::Write`, asked after each change once the
document has painted, and an answer that did not change keeps its object so nothing re-renders for it.

Public API:

- `useWriteEditorModel({ client, doc, registry })` — the editor model of a document, with the editor's
  chrome commands (`registerWriteUi`: the dialog commands, Find, the ruler and panes, the zoom fits, Esc)
  registered while mounted.
- `WriteEditor` — the editing area: `<WriteEditor model title theme ribbon />`.
- `WriteStatusBar` — page, words, language, views and zoom: `<WriteStatusBar model readOnly onFit />`.
- `withTablePicker(ribbon)` — Write's ribbon with the grid picker in Insert ▸ Tables.
- `WriteEditorModel`, `contextsOf`, `contextMenuEntries`, `miniToolbarItems`, `fitZoom` and the panes,
  rulers and dialogs for reuse (Kiln Notes pages are Write documents).

In development builds the web app exposes the model as `window.__kilnWrite` for the end-to-end tests
(`e2e/specs/write.spec.ts`, `e2e/specs/write-shortcuts.spec.ts`).
