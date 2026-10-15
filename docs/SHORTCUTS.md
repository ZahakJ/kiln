# Keyboard shortcuts

Officina keeps Microsoft Office's default shortcuts. On macOS, use ⌘ where the
table says Ctrl. Shortcuts marked ★ are Officina additions that Office does not
have. A dash means the shortcut does nothing in that application.

The table below is the specification. The keymaps in
`packages/commands/src/keymaps/` implement it, the same keymaps drive the
ribbon's Key Tips and the desktop menus, and `e2e/shortcuts.spec.ts` checks
every row. In the application, **Ctrl+/** ★ shows the shortcuts for the current
app, and **File ▸ Options ▸ Keyboard** lists and remaps them.

| Shortcut | Scriba (documents) | Tabula (spreadsheets) | Scena (presentations) |
|---|---|---|---|
| Ctrl+N / O / S / P / W | New / Open / Save / Print / Close | same | same |
| F12 or Ctrl+Shift+S | Save As | Save As | Save As |
| Ctrl+Z / Ctrl+Y (also Ctrl+Shift+Z) | Undo / Redo | same | same |
| Ctrl+X / C / V, Ctrl+Alt+V | Cut / Copy / Paste, Paste Special | same | same |
| Ctrl+Shift+C / Ctrl+Shift+V | Copy / Paste formatting (Format Painter) | — | Copy / Paste formatting |
| Ctrl+A | Select all | Select region, then sheet | Select all objects or text |
| Ctrl+F / Ctrl+H / Ctrl+G | Find / Replace / Go To | Find / Replace / Go To (also F5) | Find / Replace / — |
| Ctrl+B / I / U | Bold / Italic / Underline | same | same |
| Ctrl+Shift+D | Double underline | — | — |
| Ctrl+= / Ctrl+Shift+= | Subscript / Superscript | — | Subscript / Superscript |
| Ctrl+Shift+> / <, Ctrl+] / [ | Grow / shrink font | — | Grow / shrink font |
| Ctrl+Shift+A | All caps | — | — |
| Ctrl+Shift+L | Bullets | Toggle AutoFilter | Bullets |
| Ctrl+E / L / R / J | Centre / Left / Right / Justify | Flash Fill (Ctrl+E) | Centre / Left / Right / Justify |
| Ctrl+1 / 2 / 5 | Line spacing 1 / 2 / 1.5 | Format Cells (Ctrl+1) | — |
| Ctrl+M / Ctrl+Shift+M | Indent / Outdent | — | New slide (Ctrl+M) |
| Ctrl+Enter | Page break | Fill the selection with the entry | New slide (in the thumbnail rail) |
| Ctrl+Shift+Enter | Column break | Array formula (legacy) | — |
| Ctrl+K | Insert link | Insert link | Insert link |
| Ctrl+Shift+F | Font dialog | — | — |
| Ctrl+D | Font dialog | Fill down | Duplicate |
| Ctrl+Shift+N | Normal style | — | — |
| Ctrl+Alt+1 / 2 / 3 | Heading 1 / 2 / 3 | — | — |
| Ctrl+Space | Clear character formatting | Select column | — |
| Ctrl+Q | Clear paragraph formatting | — | — |
| Shift+F3 | Change case | — | Change case |
| F7 / Shift+F7 | Spelling / Thesaurus | Spelling | Spelling |
| Ctrl+Shift+E | Track changes | — | — |
| Ctrl+Alt+M | New comment | New comment (Shift+F2: note) | New comment |
| Ctrl+Shift+8 | Show ¶ | Outline symbols | — |
| Alt+Shift+Arrows | Move paragraph / promote and demote | Group / ungroup rows and columns | Promote / demote |
| Ctrl+F1 | Collapse or expand the ribbon | same | same |
| Alt or F10 | Key Tips | same | same |
| Alt+Q, Ctrl+Shift+P ★ | Search / command palette | same | same |
| Ctrl+/ ★ | Shortcut cheat sheet | same | same |
| Ctrl+Shift+V | — | Paste values only ★ | — |
| F2 | — | Edit the cell in place | Edit the text of the selected object |
| F4 | Repeat last action | Toggle `$` absolute and relative references; repeat | Repeat |
| F9 / Shift+F9 | Update fields | Recalculate workbook / sheet | — |
| F5 / Shift+F5 | Go To | Go To | Slide show from the start / from the current slide |
| Ctrl+` | — | Show formulas | — |
| Ctrl+; / Ctrl+Shift+; | Insert date / time | Insert date / time | — |
| Ctrl+T or Ctrl+L | — | Format as Table | — |
| Ctrl+Shift+1 / 4 / 5 | — | Number / Currency / Percent format | — |
| Ctrl+Shift+& / _ | — | Outline border / remove borders | — |
| Ctrl+9 / Ctrl+0, Ctrl+Shift+9 / 0 | — | Hide rows / columns; unhide | — |
| Ctrl+Page Up / Page Down | — | Previous / next sheet | Previous / next slide |
| Ctrl+Arrows, Ctrl+Shift+Arrows, Ctrl+Home / End | Move by word or paragraph | Jump to the edge of data, extend, A1, last cell | — |
| Shift+Space | — | Select row | — |
| Alt+= | — | AutoSum | — |
| Alt+Enter | — | New line in the cell | — |
| Alt+Down | — | Open the filter or autocomplete list | — |
| Ctrl+- / Ctrl+Shift+= | — | Delete / insert cells | — |
| Shift+F11 | — | New sheet | — |
| Ctrl+Shift+G / Ctrl+Shift+H | — | — | Group / ungroup |
| Shift+Arrows / Ctrl+Arrows (object selected) | — | — | Resize / nudge the shape |
| Esc | Cancel, close the pane | Cancel the edit | Deselect, end the show |

## During a slide show

| Key | Action |
|---|---|
| N, Space, Right, Page Down | Next |
| P, Backspace, Left, Page Up | Previous |
| Home / End | First / last slide |
| number, then Enter | Go to that slide |
| B / W | Black / white screen |
| Ctrl+L / Ctrl+P / Ctrl+I | Laser pointer / pen / highlighter |
| Esc | End the show |
