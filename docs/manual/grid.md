# Kiln Grid

Kiln Grid is the spreadsheet of Kiln. It opens and saves Excel workbooks (`.xlsx`, `.xlsm`, `.xltx`,
`.xltm`) and keeps what it does not edit exactly as it was, so a workbook goes back to Microsoft
Excel unchanged except for your edits. The ribbon, the shortcuts, the formula bar and the dialogs are
where Excel users expect them; the sheet is drawn by Kiln itself, the same on every computer.

> **Status (WP-71).** The sheet, the formula bar, typing and editing cells, formulas with
> autocomplete and argument hints, the fill handle, rows and columns, sheet tabs, freeze panes,
> zoom, the clipboard with Paste Special, number formats, fonts, fills, borders, alignment, wrap and
> merge, Format Cells, undo and redo, Find, Go To and the status bar statistics are in. Sorting and
> filtering, tables, conditional formatting rules, data validation rules, the Name Manager,
> comments and charts editing arrive with WP-72 and WP-73; files that already have them show them. Commands that
> are not in a build yet are hidden, never greyed out.

## The window

- **Ribbon**: Home, Insert, Page Layout, Formulas, Data, Review and View, with Excel's groups. The
  dialog launcher ↘ in the corner of Font, Alignment and Number opens Format Cells on that tab.
- **Formula bar** (under the ribbon): the **name box** shows the active cell or range; type a
  reference (`B12`, `Sheet2!A1:C3`) or a defined name and press Enter to go there, or pick a name
  from its list; type a new name (`Totals`) and press Enter to name the selected cells. **✕** and **✓** cancel and enter the cell being edited; **fx** opens Insert
  Function; the box shows the active cell's formula with each reference in the colour of its outline
  on the sheet. The arrow at the right expands the bar to several lines (View ▸ Formula Bar hides it).
- **Sheet**: column letters and row numbers, with the selected ones highlighted. Gridlines and
  headings are View options (View ▸ Gridlines, Page Layout ▸ Headings).
- **Sheet tabs** (below the sheet): **+** adds a sheet (Shift+F11); click a tab to show it, double-
  click to rename it, drag it to reorder. Right-click a tab to insert, delete, rename, duplicate,
  move, colour, hide or unhide sheets.
- **Status bar**: the mode (Ready, Enter, Edit, Point), and for a selection of several cells the
  Average, Count and Sum, formatted like the active cell. Right-click it to add Numerical Count,
  Minimum and Maximum. At the right: the views and the zoom (10 %–400 %, Zoom to Selection).

## Moving and selecting

| To | Press |
|---|---|
| Move one cell | the arrow keys |
| Next / previous cell across | Tab / Shift+Tab |
| Next / previous cell down | Enter / Shift+Enter |
| Jump to the edge of the data | Ctrl+arrow (add Shift to extend the selection) |
| Go to A1 / the last used cell | Ctrl+Home / Ctrl+End |
| Start of the row | Home |
| A screen down / up, right / left | Page Down / Page Up, Alt+Page Down / Alt+Page Up |
| Select the current region, then the whole sheet | Ctrl+A (press again) |
| Select the row / the column | Shift+Space / Ctrl+Space |
| Previous / next sheet | Ctrl+Page Up / Ctrl+Page Down |
| Go To | F5 or Ctrl+G |

Click a cell to select it; drag, or Shift+click, to select a range; Ctrl+click adds another range.
Click a column letter or a row number to select it whole. Inside a selection of several cells,
Enter and Tab move through it without leaving it. After a run of entries made with Tab, Enter goes
to the next row under the cell where the run started, as in Excel.

## Entering and editing

Type to replace the active cell's content (**Enter** mode); press **F2**, double-click the cell or
click in the formula bar to edit it where it is (**Edit** mode). Enter, Tab or a click elsewhere
keeps the entry; Esc cancels it. Alt+Enter starts a new line in the cell; Ctrl+Enter enters the same
value in every selected cell; Delete clears the selection's contents.

Kiln reads what you type as Excel does: numbers, percentages, currency, dates and times become
values with a matching number format; text that would read as a number stays text when it starts
with an apostrophe (`'0042`). **AutoComplete** finishes a word from the entries above in the same
column when only one entry matches; Alt+Down shows the list of the column's entries.
Ctrl+; enters today's date, Ctrl+Shift+; the time.

### Formulas

Start with `=`. While you type a function name, the **autocomplete** list shows the matching
functions with a description of the highlighted one; Tab inserts it with its opening parenthesis.
Inside the parentheses a tooltip shows the function's arguments with the current one in bold.

While a formula expects a reference, the arrow keys and clicks **point**: the reference goes into
the formula, and Shift extends it to a range (**Point** mode). Each reference in a formula is
outlined on the sheet in its own colour. F4 cycles the reference at the caret through `$A$1`, `A$1`,
`$A1` and `A1`. Enter closes the parentheses you left open. Ctrl+Shift+Enter enters a legacy array
formula. **Alt+=** (AutoSum) sums the numbers above or to the left. Formulas ▸ Calculate Now (F9)
recalculates the workbook, Show Formulas (Ctrl+`) shows formulas instead of their results.

### The fill handle

Drag the small square at the corner of the selection to fill: numbers and dates continue their
series (1, 2 → 3, 4, 5), text with a number counts on (Item 1 → Item 2), lists such as weekdays and
months follow, and formulas adjust their relative references. Ctrl+D fills down and Ctrl+R fills
right from the first row or column of the selection; Home ▸ Fill also fills up and left.

## Rows, columns and sheets

Drag the border between two column letters (or row numbers) to resize; double-click it to fit the
content. Home ▸ Format sets an exact Row Height or Column Width, AutoFits, hides and unhides; Home ▸
Insert and Delete insert or delete cells (shifting the others), whole rows, whole columns or sheets.
Ctrl+Shift+= and Ctrl+- insert and delete; Ctrl+9 / Ctrl+0 hide rows / columns, Ctrl+Shift+9 /
Ctrl+Shift+0 unhide them.

**Freeze panes** (View ▸ Freeze Panes) keeps the rows above and the columns left of the active cell
in view while the rest scrolls; Freeze Top Row and Freeze First Column are the usual cases. Choose
Freeze Panes again to unfreeze. **Split** divides the view at the active cell; in this version the
parts above and left of the split stay put while the rest scrolls, like frozen panes. The zoom (status bar,
Ctrl+wheel, View ▸ Zoom to Selection) is kept with the sheet.

## The clipboard

Copy (Ctrl+C) and Cut (Ctrl+X) mark the copied range with a moving border; Esc removes it. Paste
(Ctrl+V) pastes everything: values, formulas with their references adjusted, formats, merges and row
heights. Content from other applications pastes as text, split on tabs and lines (HTML tables keep
their cells). **Home ▸ Paste ▾** has Paste Values (Ctrl+Shift+V), Formulas, Formatting and Transpose;
**Paste Special…** (Ctrl+Alt+V) chooses what to paste (all, formulas, values, formats, all except
borders, values and number formats), an operation (add, subtract, multiply, divide), Skip blanks
and Transpose.

## Formatting cells

The Home tab formats the selection: font and size, bold (Ctrl+B), italic (Ctrl+I), underline
(Ctrl+U), strikethrough, font colour, fill colour, borders (with the line style and colour of the
Borders menu), alignment (top, middle, bottom, left, centre, right), indent, orientation, **Wrap
Text** and **Merge & Center** (with Merge Across, Merge Cells and Unmerge). The Number group sets the
number format (General, Number, Currency, Accounting, Short and Long Date, Time, Percentage,
Fraction, Scientific, Text) and adds or removes decimals; Ctrl+Shift+1 / 4 / 5 apply Number,
Currency and Percent; Ctrl+Shift+& draws an outline border and Ctrl+Shift+_ removes the borders.

**Format Cells** (Ctrl+1, or the ↘ launchers) has Excel's six tabs:

- **Number**: the categories with their formats, a **Type** box for any custom format code, and a
  **Sample** of the active cell in the chosen format.
- **Alignment**: horizontal (including Fill, Justify, Center Across Selection and Distributed) with
  the indent, vertical, Wrap text, Shrink to fit, Merge cells, and the text orientation in degrees or
  vertical text.
- **Font**: font, style, size, underline (single, double, accounting), strikethrough, superscript,
  subscript, colour, with a preview.
- **Border**: a line style and colour, then the presets (None, Outline, Inside) or each edge,
  inside line and diagonal on the preview.
- **Fill**: the background colour, the pattern style and pattern colour.
- **Protection**: Locked and Hidden (they apply once the sheet is protected).

## Find and Go To

Ctrl+F opens the find bar over the sheet: Enter finds the next cell containing the text, Shift+Enter
the previous one; the options match case, the entire cell, and look in formulas instead of values.
F5 or Ctrl+G opens Go To: pick a defined name or type a reference.

## Undo

Every change can be undone (Ctrl+Z) and redone (Ctrl+Y or Ctrl+Shift+Z), in Excel's steps: one entry,
one paste, one formatting command, one fill. Undo while typing an entry discards the entry.

## Accessibility

The sheet is a grid for screen readers: moving announces the cell's address, its value and its
formula. Every command has a keyboard path (the shortcuts above, Key Tips with Alt, the command
search with Alt+Q). The sheet follows the light and dark themes; cell colours set in the file are
kept, automatic colours follow the theme.
