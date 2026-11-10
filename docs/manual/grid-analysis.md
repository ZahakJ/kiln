# Kiln Grid: PivotTables, sparklines and analysis

This page covers PivotTables, sparklines, Goal Seek, Subtotal, Flash Fill, protection and themes
in [Kiln Grid](grid.md). Everything here saves to `.xlsx`, and the files open in Microsoft Excel
and LibreOffice with the same PivotTables, sparklines and passwords.

## PivotTables

### Making one

1. Click in a table or a range with headings, then choose **Insert ▸ PivotTable**.
2. **Create PivotTable** suggests the range around the active cell, or the table's name. Choose
   **New Worksheet** (the default) or **Existing Worksheet** and a cell, then **OK**.
3. An empty report appears with its prompt, and the **PivotTable Fields** pane opens beside the
   sheet.

### The PivotTable Fields pane

- Tick a field to add it. Text and dates go to **Rows** and numbers go to **Σ Values**, as in Excel.
- Drag fields from the list or between **Filters**, **Columns**, **Rows** and **Σ Values**. Drag a
  field out of the areas, or untick it, to remove it. **Σ Values** appears in Rows or Columns once
  there are two values, and you can move it like a field.
- Each field in an area has a menu: **Move Up**, **Move Down**, **Move to Beginning**, **Move to End**,
  the **Move to …** commands, **Remove Field**, and **Field Settings…** or **Value Field Settings…**.
- **Search** narrows the field list. The pane closes with **×** and opens again from
  **PivotTable Analyze ▸ Field List**. On a phone the pane opens over the bottom of the sheet.

### Values

**Value Field Settings…** (from the field's menu, or **PivotTable Analyze ▸ Field Settings** on a
value) has two tabs.

- **Summarize Values By**: Sum, Count, Average, Max, Min, Product, Count Numbers, StdDev, StdDevp,
  Var or Varp. The name follows the choice ("Average of Units") until you type your own.
- **Show Values As**: % of Grand Total, % of Column Total, % of Row Total, % Of, % of Parent Row
  Total, % of Parent Column Total, Difference From, % Difference From, Running Total In,
  % Running Total In, Rank Smallest to Largest, Rank Largest to Smallest, and Index. Some of these
  ask for a base field and a base item.
- **Number Format…** sets the values' format.

### Sorting and filtering

- Each row and column field has a button in the report. It sorts the field A to Z or Z to A,
  offers **Label Filters…** (equals, begins with, contains, between and more, with `?` and `*`
  wildcards) and **Value Filters…** (Top 10 items, percent or sum, and comparisons), and lists the
  field's items with a search box to tick the ones you want.
- A field in **Filters** shows above the report. Pick one item, or tick **Select Multiple Items**.
- **Field Settings…** gives the field a custom name and sets its subtotals (Automatic, None, or
  custom functions) and its sort. It also holds the item list and the label and value filters.

### Layout and style

- **Design ▸ Subtotals**: do not show them, or show them at the bottom or the top of each group.
  **Grand Totals**: off, on, or for rows or columns only.
- **Design ▸ Report Layout**: **Compact** (the default, with row fields indented in one column),
  **Outline** or **Tabular** form. You can repeat item labels or leave them out. **Blank Rows**
  inserts a blank line after each item.
- **Design ▸ PivotTable Styles**: Excel's 84 styles (Light, Medium and Dark), plus **None**.
  **Row Headers**, **Column Headers**, **Banded Rows** and **Banded Columns** change the style's
  look.
- **PivotTable Analyze ▸ Options** renames the report. It also sets the text for empty cells,
  repeated item labels, blank lines and the grand totals.

### Refreshing and details

- A PivotTable keeps a copy of its source. After the source changes, choose **PivotTable Analyze ▸
  Refresh**, **Refresh All**, or **Data ▸ Refresh All**. **Change Data Source** points the report at
  another range or table.
- Double-click a value, or choose **PivotTable Analyze ▸ Show Details**, to list the rows behind it
  on a new sheet as a table.
- The report's cells cannot be typed over. Kiln shows Excel's message: "Cannot change this part of
  a PivotTable report." Conditional formatting on the report's cells works and stays after a
  refresh.
- **Clear All** empties the report, **Move PivotTable** moves it, and **Delete** removes it.

### In files

Kiln saves PivotTables with their cache, so Excel and LibreOffice open them ready to refresh. A
PivotTable you did not edit is written back exactly as it was. PivotTables made in Excel or
LibreOffice open as working PivotTables. A report using something Kiln does not hold (calculated
fields, grouped dates, OLAP sources) stays as its values and is saved back unchanged.

## Sparklines

- Select the data, then choose **Insert ▸ Line**, **Column** or **Win/Loss** in the Sparklines
  group. Give the location range, for example a column beside the data, and choose **OK**. Each
  row (or column) of data becomes one sparkline, and they form a group.
- The **Sparkline** tab appears when the active cell has one. It has:
  - **Edit Data** to change the group's location and data, and **Hidden & Empty Cells…** to show
    empty cells as gaps, zero, or a line joining the points, and to choose whether hidden rows
    count;
  - **Line**, **Column** and **Win/Loss** to change the type;
  - **High Point**, **Low Point**, **Negative Points**, **First Point**, **Last Point** and
    **Markers** to mark points;
  - the **Style** gallery (24 styles), **Sparkline Color** and **Marker Color**;
  - **Axis** (show the axis, right to left, the same minimum or maximum for the group), **Group**,
    **Ungroup** and **Clear**.
- Sparklines redraw when their data changes. They save to `.xlsx` as Excel 2010 sparklines.

## Goal Seek

**Data ▸ What-If Analysis ▸ Goal Seek…** finds the input that makes a formula reach a value. In
**Set cell**, give the formula's cell. In **To value**, give the target, and in **By changing
cell**, give the input cell (a constant). **Goal Seek Status** shows whether a solution was found,
the target and the value reached. **OK** keeps the new input, and **Cancel** puts the old one back.

## Subtotal

**Data ▸ Subtotal…** inserts a total row at each change in a column of a sorted list. Choose the
column (**At each change in**), the function (Sum, Count, Average, Max, Min, Product, Count
Numbers, StdDev, StdDevp, Var or Varp) and the columns to total.

- **Replace current subtotals**, **Page break between groups** and **Summary below data** work as
  in Excel. A grand total row ends the list.
- The totals are `SUBTOTAL` formulas, and the list is outlined in three levels.
- The outline gutter left of the row numbers has the level buttons **1**, **2** and **3**. Button 1
  shows only the grand total, button 2 the subtotals, and button 3 every row. Each group has a bracket
  and a **−** at its total. Click **−** to hide the group's rows, and **+** to show them again. A group
  you open keeps its own hidden groups hidden. **Data ▸ Hide Detail / Show Detail** do the same for the
  group at the active cell.
- The same gutters appear for rows and columns you group with **Data ▸ Group**. **Data ▸ Show Outline
  Symbols** hides or shows them.
- **Remove All** takes the totals and the outline away.

## Flash Fill

Type an example of what you want beside your data, such as first names next to full names. Then:

- press **Ctrl+E**, or choose **Data ▸ Flash Fill** or **Home ▸ Fill ▸ Flash Fill**, to fill the
  rest of the column; or
- keep typing. After a second example, Kiln shows the rest of the column in grey. Press **Enter**
  to take the suggestions, or carry on typing to ignore them.

Flash Fill learns splits, joins, parts of text, changes of case and parts of numbers.
When it cannot find a pattern that fits every example, it says so.

## Protection

- Cells are **locked** by default. To unlock the cells people should fill in, choose
  **Home ▸ Format ▸ Lock Cell** (or **Format Cells ▸ Protection**).
- **Review ▸ Protect Sheet** protects the active sheet. You can give a password and choose what
  stays allowed (selecting cells, formatting, inserting and deleting rows and columns, sorting,
  AutoFilter, PivotTables, editing objects). The password is asked for twice.
- On a protected sheet, typing in a locked cell shows Excel's message: "The cell or chart you're
  trying to change is on a protected sheet. To make a change, unprotect the sheet. You might be
  requested to enter a password." Commands that would change locked cells are refused the same
  way.
- **Review ▸ Unprotect Sheet** asks for the password, if there is one.
- **Review ▸ Protect Workbook** locks the structure, so sheets cannot be added, deleted, renamed,
  moved or hidden. It also takes an optional password.
- Passwords are saved as Excel saves them: a salted SHA-512 hash with 100,000 rounds. Excel and
  LibreOffice accept the same password. Protection keeps people from changing a sheet by mistake.
  It is not encryption.

## Themes

**Page Layout ▸ Themes** applies a workbook theme: Excel's **Office** theme or one of Kiln's twelve
themes (the same as in Kiln Deck). Theme colours in the palette, theme fonts (`+Headings` and
`+Body`), table styles and PivotTable styles change with it. **Colors** and **Fonts** take only the
colours or only the fonts of another theme. The theme saves to `.xlsx` with the workbook.
