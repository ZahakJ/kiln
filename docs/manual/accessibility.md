# Accessibility

Kiln works with the keyboard alone, gives screen readers its interface through the browser's accessibility tree, and follows the system's contrast and motion settings.

## The keyboard

Every command can be reached without a mouse.

- **Alt** (or **F10**) shows Key Tips: letters on the File button, the ribbon tabs and the title bar. Type a tab's letter to open it, then a command's letters to run it: **Alt, H, 1** is Bold. A menu button's or split button's letters open its menu with letters on every item (**Alt, H, B, B** is Bottom Border); the arrow keys and **Enter** work in it too. A gallery's letters put the keys on its items. **Esc** goes back one level. The letters are Office's wherever Office has them; the full list is in [Keyboard shortcuts](shortcuts.md#key-tips).
- **F6** moves between the parts of the window: the document, the open panes (in Kiln Deck also the slide thumbnails, the speaker notes and the outline), the status bar, the title bar and the ribbon. **Shift+F6** goes the other way.
- In the ribbon, the arrow keys move between tabs and commands, **Down** or **Enter** goes from a tab into its commands, **Enter** opens a menu, **Esc** goes back to the tab. In a text box such as Font or Font Size, **Left** and **Right** move the caret, and past its start or end they go on to the next command.
- **Alt+Q** (or **Ctrl+Shift+P**) opens Search: type a command's name, or its Office name, and press **Enter**.
- **Shift+F10** or the Menu key opens the context menu where the caret or the selection is.
- In dialogs, the keys start in the first field; **Tab** moves between fields, **Enter** confirms and **Esc** cancels. The keys go back to the document when a dialog or menu closes after a command.

The keys always show where they are: the focused control has an outline in the app's colour (the high-contrast theme's amber, or the system's highlight colour in a Windows contrast theme).

## Screen readers

- The document is read as text: Kiln Write reads paragraphs, headings with their levels, lists, tables, pictures by their alt text, text boxes, and the headers, footers and notes of the pages shown. Kiln Grid reads the sheet as a grid with its row and column headers. Kiln Deck reads the slide, its title and the names of its shapes.
- Kiln reads out what changes: the cell you move to with its value and formula ("B12, 1,284.00, formula =SUM(B2:B11)", or "B13, blank"), the cell mode in Kiln Grid (Ready, Enter, Edit and Point mode), a range you select ("A1 to C4 selected"), the text you select in a document, the shapes you select on a slide, the slide shown, the slide show's slides and its end, and Read-only when a document opens that way.
- The ribbon, menus, dialogs, panes, the File menu and the status bar are standard controls with their names, roles and states (pressed, checked, expanded), so the screen reader's own commands for them work.

## The Text View

**View ▸ Accessibility ▸ Text View** (**Alt, W, T, V**; in Kiln Deck **Alt, W, V**) shows the document as plain text with its structure, over the page view, for screen reader users who prefer it, or anyone who wants to read without the page layout:

- **Kiln Write**: headings, paragraphs, lists, tables with their header rows, pictures by their alt text, text boxes, headers and footers. Type, press **Enter** and **Backspace**, and use any shortcut or ribbon command: it acts on the text you select here, as on the page. The page view follows every change.
- **Kiln Grid**: the active sheet's cells as a table with row numbers and column letters. The arrow keys move between cells (the sheet's active cell moves with them); **Enter** or **F2** edits a cell, typing starts a new entry, **Enter** saves it and **Esc** cancels.
- **Kiln Deck**: each slide as a section headed by its number and title, with its text shapes, pictures, tables and speaker notes. Type into a text shape to change it.

Select the command again, or **Close Text View**, to go back to the page.

## Without the pointer

Everything the pointer does has keys:

- **Shapes and text boxes**: after **Insert ▸ Shapes** or **Text Box**, **Enter** places it in its usual size (in Kiln Deck in the middle of the slide).
- **Format Painter and Animation Painter**: pick up the format, move to the target (the arrows in Kiln Grid, **Tab** between objects in Kiln Deck) and press **Enter**.
- **Kiln Write**: **Shift+F10** on a link offers **Open Link**; a comment is selected when the keys enter it in the Comments pane.
- **Kiln Grid**: **Home ▸ Find & Select ▸ Go To Special ▸ Objects** selects the first chart or picture, **Tab** and **Shift+Tab** go from one to the next and **Shift+F10** opens its menu; **Alt+Shift+F10** opens the error indicator's menu (Convert to Number, Ignore Error); **Alt+Down** on a table's total row picks its function; **Home ▸ Format** has Move or Copy Sheet, Hide Sheet and Unhide Sheet.
- **Kiln Deck**: in the slide thumbnails, the arrows select slides and **Ctrl+Up** and **Ctrl+Down** move them (with **Shift**, to the start or the end); **Shift+F10** opens a slide's menu, with its section's in **Section**. A guide takes the keys with **Tab**: the arrows move it (**Shift** for a larger step) and **Delete** removes it. **Table Layout ▸ Select ▸ Select Table** selects every cell. In a slide show, **Tab** reaches the toolbar's buttons and **Enter** presses them.

## Contrast and colour

- **View ▸ Theme ▸ High Contrast** (or **File ▸ Options ▸ General**) turns on the high-contrast theme: near-black backgrounds, light text and lines, and amber focus outlines; every text, line and outline colour has a contrast of at least 7:1 against the background.
- With the theme set to **Use System Setting**, Kiln switches to high contrast when the system asks for more contrast (Windows contrast themes, macOS Increase Contrast).
- In a Windows contrast theme, Kiln uses the theme's own colours for its text, controls and outlines; selected tabs, pressed buttons and the focused control keep an outline in the theme's highlight colour.

## Motion

When the system asks for reduced motion (Windows: **Show animations** off; macOS: **Reduce motion**; Linux desktops: their animation setting), Kiln's panes, menus, dialogs and the File menu appear without sliding or fading, and slide shows play every transition as a fade and every animation effect at once, on the same clicks.
