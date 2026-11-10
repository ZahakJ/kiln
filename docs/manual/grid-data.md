# Kiln Grid: data, tables, pages and review

This page covers the Data, Page Layout and Review tabs of [Kiln Grid](grid.md), Format as Table,
conditional formatting, pictures and shapes. Formulas, names and auditing are in
[Formulas in Kiln Grid](grid-formulas.md). Everything here saves to `.xlsx` and opens the same in
Microsoft Excel and LibreOffice.

## Sort

- **Data ▸ Sort A to Z / Z to A** (also Home ▸ Sort & Filter) sorts the region around the active
  cell by its column. A first row that looks like headings stays on top.
- **Data ▸ Sort…** opens the Sort dialog: add levels with **Add Level**, choose the column, what to
  sort on (cell values, cell colour, font colour, conditional formatting icon) and the order
  (A to Z, Z to A, or a custom list such as months). **Options…** sorts case-sensitively or left to
  right. **My data has headers** decides whether the first row is sorted.

## Filter

- **Data ▸ Filter** (Ctrl+Shift+L) puts a dropdown button in each heading of the region; again
  removes them. Inside a table the buttons are the table's (Table Design ▸ Filter Button).
- The dropdown sorts by the column, filters by colour, and lists the column's values with a
  search box: untick **(Select All)**, tick the values to keep, and choose **OK**.
- **Text Filters**, **Number Filters** and **Date Filters** (whichever fits the column) offer
  equals, begins with, contains, greater than, between, Top 10, above average, and periods such
  as This Month or Last Year. **Custom Filter…** joins two conditions with And or Or; `?` and `*`
  are wildcards.
- **Clear Filter From "…"** clears one column. **Data ▸ Clear** clears them all; **Data ▸ Reapply**
  filters again after the data changed. Filtered-out rows are hidden; their row numbers turn blue.
- `SUBTOTAL` and the status bar count the visible rows only.
- **Alt+Down** on a heading opens its dropdown with the keys in it: type to search, the arrows move,
  Space ticks a value, Enter applies, Esc closes. A column's list shows only the values the other
  columns' filters leave; dates are grouped by year and month.
- In a filtered list, Delete, Clear, Fill, formats, Ctrl+Enter, Copy and Cut act on the shown rows
  only. Pasting one cell over a filtered selection fills its shown cells; a block is pasted once at
  the top left.

## Conditional formatting

Home ▸ Conditional Formatting:

- **Highlight Cells Rules**: greater than, less than, between, equal to, text that contains, a date
  occurring, duplicate or unique values, each with a choice of format.
- **Top/Bottom Rules**: top or bottom N items or percent, above or below average.
- **Data Bars**, **Color Scales** and **Icon Sets**: gradient or solid bars, two- and three-colour
  scales, and arrows, traffic lights, flags, ratings and the other Excel sets.
- **New Rule…** shows every rule type, including **Use a formula to determine which cells to
  format** (a formula such as `=$B2="Firing"` written for the first cell of the range).
- **Manage Rules…** lists the rules of the selection or the sheet: edit, delete, duplicate,
  reorder, change **Applies to**, and tick **Stop If True**. **Clear Rules** clears the selection's
  or the sheet's.

In the dark theme, gradient data bars fade towards the page so the numbers on them stay readable.

## Data validation

**Data ▸ Data Validation…**:

- **Settings**: allow any value, whole numbers, decimals, a list, dates, times, a text length or a
  custom formula, compared with between, equal to, greater than and the rest. A **List** takes
  values separated by commas (`Low,Medium,High`) or a range (`=$F$2:$F$9`); with **In-cell
  dropdown** the cell shows a button that opens the list.
- **Input Message**: a title and message shown beside the cell when it is selected.
- **Error Alert**: the alert shown when an entry breaks the rule. **Stop** refuses it: Retry goes
  back to the entry, Cancel leaves the cell as it was. **Warning** asks whether to continue (Yes
  keeps the entry, No goes back to it) and **Information** keeps it after OK.
- **Circle Invalid Data** circles cells whose values break their rule; **Clear Validation Circles**
  removes the circles.

## Tables

- **Insert ▸ Table** or **Home ▸ Format as Table** (Ctrl+T) makes the region a table: tick **My
  table has headers**, or Kiln names the columns Column1, Column2…. The gallery picks one of Excel's
  60 styles.
- The **Table Design** tab appears while the active cell is in a table: rename it, **Resize
  Table…**, **Remove Duplicates**, **Convert to Range**, the **Total Function** of a total-row cell,
  the **Table Style Options** (Header Row, Total Row, Banded Rows and Columns, First and Last
  Column, Filter Button) and the styles.
- **Total Row** adds a row of `SUBTOTAL` formulas that count only the visible rows.
- Formulas can use **structured references**: `=SUM(Costs[Amount])`, `=[@Amount]*2`. After
  `Costs[` the suggestions are the columns, `#All`, `#Data`, `#Headers`, `#Totals`, `#This Row`, and
  `@` (this row) inside the table. Files store them as Excel does (`Costs[[#This Row],[Amount]]`).
- A total-row cell has a dropdown button (or Alt+Down) with the functions: Average, Count, Sum…
- A formula typed into an empty column of a table fills the whole column: a **calculated column**.
  Undo takes the column back in one step.
- Typing in the row below or the column beside a table makes the table bigger.

## Cell Styles and Format Painter

- **Home ▸ Cell Styles** applies Good, Bad and Neutral, Data and Model, Titles and Headings, the
  themed styles and number styles.
- **Home ▸ Format Painter** copies the format of the selection to the next cells you select; a
  double-click keeps the brush until you press Esc.

## Find, Replace and Go To Special

- **Ctrl+F** and **Ctrl+H** open Find and Replace. **Options >>** searches the sheet or workbook,
  by rows or columns, in formulas, values, notes or comments, with **Match case** and **Match
  entire cell contents**; `?`, `*` and `~` work as in Excel. **Find All** lists every match;
  **Replace All** reports how many cells changed.
- **Home ▸ Find & Select ▸ Go To Special…** selects notes, constants, formulas, blanks, the current
  region or array, the last cell, visible cells only, errors, conditional formats or data
  validation.

## Notes, comments and links

- **Review ▸ Notes ▸ New Note** (Shift+F2) writes a note in a card beside the cell; Esc or a
  click elsewhere keeps it. A red triangle marks the cell; pointing at it shows the note. **Show
  All Notes** keeps every note on the sheet; **Convert to Comments** turns notes into threads.
- **Review ▸ New Comment** (or Insert ▸ Comment) starts a thread in the Comments pane; others
  reply, resolve and reopen it. **Previous** and **Next** go from thread to thread, across sheets.
- Notes and comments are written under the user name of File ▸ Options.
- **Insert ▸ Link** (Ctrl+K) links a cell to a web page, an e-mail address, or a place in the
  workbook (a cell or a defined name), with a ScreenTip. **Insert ▸ Link ▸ Open Hyperlink** follows
  the active cell's link; **Remove Hyperlink** takes it off.

## Pictures, shapes and text boxes

- **Insert ▸ Pictures** puts a picture (PNG, JPEG, GIF, BMP or WebP) at the active cell, at its own
  size or smaller to fit the window. **Insert ▸ Shapes** places a rectangle, oval, arrow, star,
  callout, line and others; **Insert ▸ Text Box** places a box and starts its text.
- Click an object to select it; drag it to move it and its handles to size it. Pictures keep
  their proportions unless **Size and Properties…** says otherwise. Delete removes it.
- **Picture Format**: **Alt Text**, **Reset Picture** (and Size), **Crop** (how much of each side
  to hide), Bring Forward and Send Backward, Height and Width.
- **Shape Format**: change the shape, **Shape Fill**, **Shape Outline** and its weight, **Text
  Fill**, **Edit Text…** (or just type with the shape selected), Alt Text, arrange and size.
- **Size and Properties…** sets the name, size, proportions and whether the object moves and sizes
  with the cells under it, moves only, or stays put.
- Objects print where they are, and save with the workbook.

## Getting data in and out

- **Data ▸ From Text/CSV** opens a file: the preview shows its first rows with the detected
  encoding and delimiter, which you can change; choose whether to detect data types and whether
  numbers use a decimal comma (`1.234,56`). **Load to** a new sheet or the active cell.
- **Data ▸ From JSON** reads an array of records (`[{"name": "Clay", "kg": 25}]`) or of arrays as a
  table.
- **Data ▸ Export** downloads the selection or the sheet as CSV (UTF-8), tab-separated text or JSON
  records.
- **Data ▸ Refresh All** recalculates and reapplies the filters.

## Data tools and outlines

- **Data ▸ Text to Columns…** splits a column by delimiters (tab, semicolon, comma, space or
  another character, with a text qualifier and consecutive delimiters as one) or at fixed widths,
  and sets each column's type.
- **Data ▸ Remove Duplicates…** keeps the first row of each combination of the ticked columns and
  reports how many rows went.
- **Data ▸ Group** (Alt+Shift+Right) and **Ungroup** (Alt+Shift+Left) outline rows or columns;
  **Hide Detail** and **Show Detail** fold and unfold them, and **Show Outline Symbols** shows or
  hides the outline bar.

## Spelling

**Review ▸ Spelling** (F7) goes through the text cells: **Change** or **Change All** to a
suggestion, **Ignore Once**, **Ignore All**, or **Add to Dictionary**.

## Page setup and printing

- **Page Layout ▸ Margins**, **Orientation** and **Size** set the page; **Custom Margins…** opens
  Page Setup.
- **Print Area ▸ Set Print Area** prints only the selection; **Add to Print Area** adds a range
  (each prints on its own page); **Clear Print Area** prints the used range again. Pictures, shapes
  and charts are part of the used range.
- **Breaks** inserts a page break above and to the left of the active cell, removes it, or resets
  them all.
- **Print Titles** opens Page Setup's Sheet tab: rows to repeat at top (`$1:$1`) and columns to
  repeat at left, gridlines and headings, page order.
- **Scale to Fit**: width and height in pages (Fit Sheet on One Page), or a percentage.
- **Sheet Options**: view and print gridlines and headings.
- **Insert ▸ Header & Footer** opens Page Setup's Header/Footer tab: presets, or codes such as
  `&CPage &P of &N` (`&L`, `&C`, `&R` for the left, centre and right parts; `&P` page, `&N`
  pages, `&D` date, `&T` time, `&A` sheet, `&F` file).
- **File ▸ Print** (Ctrl+P) shows the pages as they print; the PDF has real, searchable text.
  `kiln convert book.xlsx book.pdf` prints a workbook from the command line with its page setup.

## Page Break Preview and Page Layout

- **View ▸ Page Break Preview** greys out what does not print and draws the page breaks: dashed
  lines are automatic, solid lines manual. Drag a line to move a break.
- **View ▸ Page Layout** shows the printed pages one under another, each with rulers along its
  edges (View ▸ Ruler shows or hides them) and the sheet's faint gridlines, which do not print. Click
  a cell on a page to select it, drag to select a range, type to fill it, double-click to edit it;
  the keys work as on the sheet. Click the top or bottom of a page to set the header or footer.
  The empty page after the last one says **Click to add data**: a click there selects the cell
  under the printed ones, where what you type starts the next page.
