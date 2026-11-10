# Formulas in Kiln Grid

Kiln Grid calculates Excel's worksheet functions in the same way Excel does (dynamic arrays, `LET`
and `LAMBDA` included). This page covers the Formulas tab: inserting functions, names, auditing,
Formula Lab and calculation. Typing formulas, autocomplete and argument hints are in
[Kiln Grid](grid.md#entering-and-editing).

## Inserting functions

- **Formulas ▸ Insert Function** (or **fx** on the formula bar) opens Insert Function: search by
  name or pick a category; the box under the list shows the function's syntax and what it does.
- **OK** opens **Function Arguments**: a box for each argument, with its value as Kiln computes it
  beside the box (`= {1;2;3}` for a range), the description of the argument you are typing, and the
  formula's result at the bottom. Boxes for repeating arguments (`number1`, `number2`, …) appear as
  you fill them. **OK** puts the function in the active cell, or into the formula you are editing.
- **Financial**, **Logical**, **Text**, **Date & Time**, **Lookup & Reference** and **Math & Trig**
  list their functions; **More Functions** has Statistical, Engineering, Information, Compatibility
  and Web. Each opens Function Arguments; **Insert Function…** at the end of a list opens the dialog
  on that category.
- **Recently Used** lists the ten functions you inserted last.

## Names

- **Define Name** names the selection (or anything you type in **Refers to**), for the workbook or
  one sheet, with a comment. Typing a new name in the name box does the same.
- **Name Manager** lists every name with its value, reference, scope and comment, and tables by
  their names; **New…**, **Edit…** and **Delete** change them, and the filter box finds them.
- **Paste Names…** (Use in Formula) puts a name into the formula being edited.
- **Create from Selection** names each row or column of the selection from its labels in the top
  row, left column, bottom row or right column.
- Names work in every formula: `=SUM(Amounts)`, `=Rate*Hours`.

## Auditing

- **Trace Precedents** draws blue arrows from the cells a formula uses to the formula; press it
  again to go one level further. Ranges are outlined; a reference to another sheet draws a dashed
  arrow to a sheet icon. **Trace Dependents** draws arrows to the formulas that use the cell.
  **Remove Arrows** removes all of them, or only the precedent or dependent ones.
- **Show Formulas** (Ctrl+`) shows formulas instead of their results.
- **Error Checking** goes through the cells with errors: what the error means, **Previous** and
  **Next**, **Edit in Formula Bar** and **Show Calculation Steps…**.
- **Evaluate Formula** shows the active cell's formula and evaluates it one step at a time
  (**Evaluate** underlines the part computed next and replaces it with its value).
- **Watch Window** keeps cells in view with their sheet, name, value and formula while you work
  elsewhere: **Add Watch** watches the selected cells; click a cell to go to it.

## Formula Lab

**Formulas ▸ Formula Lab** opens a pane that follows the active cell: the formula as a tree with
every part's value, each function's documentation, the branches `IF` did not evaluate marked
**not evaluated**, and the steps from the formula to its result. For `=IF(SUM(A1:A3)>5,"x","y")`
with 1, 2 and 3 in A1:A3 the steps are `=IF(SUM({1;2;3})>5,"x","y")`, `=IF(6>5,"x","y")`,
`=IF(TRUE,"x","y")` and `="x"`.

## Newer functions

Kiln Grid has 505 functions, Excel's dynamic-array ones among them. **GROUPBY** and **PIVOTBY**
summarise a table by the values of one or two fields (`=GROUPBY(A2:A9,C2:C9,SUM)`), **TRIMRANGE**
drops the empty rows and columns round a range, **GETPIVOTDATA** reads a value from a PivotTable,
and **FORECAST.ETS** with .CONFINT, .SEASONALITY and .STAT forecasts a time series with exponential
smoothing. A function's name works as a LAMBDA: `SUM` in GROUPBY above.

## Calculation

- **Calculation Options**: **Automatic** (the default), **Automatic Except for Data Tables** and
  **Manual**. In Manual, the status bar says **Calculate** when results are out of date; **Calculate
  Now** (F9) calculates the workbook and **Calculate Sheet** (Shift+F9) the active sheet.
- A formula that refers to its own cell, directly or through others, is a **circular reference**:
  Kiln warns when you make one and the status bar names the cell (`Circular References: A1`); such
  formulas show 0.
- **Calculation Options ▸ Iterative Calculation…** lets circular formulas iterate: tick **Enable
  iterative calculation** and set **Maximum Iterations** (100) and **Maximum Change** (0.001).
- The calculation mode and the iteration settings save with the workbook.
- **File ▸ Options ▸ Advanced ▸ Use 1904 date system** counts dates from 1 January 1904, as Excel
  for the Mac once did. As in Excel, the dates already entered keep their numbers, so they show four
  years and a day later; formulas such as DATE count from 1904.

## Shortcuts

Paste Name, Insert Function, Name Manager and Create from Selection are on the Formulas tab.
