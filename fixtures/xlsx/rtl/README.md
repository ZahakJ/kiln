# Right-to-left Excel fixtures

Written by `make.py` in this folder with openpyxl (`~/.venvs/kiln/bin/python -I
fixtures/xlsx/rtl/make.py`); the zip entries get fixed dates, so the file only changes when the
script does.

`sales.xlsx`, sheet `المبيعات` with `<sheetView rightToLeft="1">`:

- a title merged across A1:F1 and a total label merged across A10:D10;
- a header row with a fill and borders, Arabic product and category names, one Latin name
  (`Kiln Glaze 2026`), quantities, prices and `=C*D` totals with `#,##0.00`, a `SUM` total;
- frozen panes at B3 (two rows and column A);
- conditional formats: data bars on the quantities, a three-colour scale on the totals, a
  3-arrows icon set on the ratings;
- a medium red left (start) border on the total, which a right-to-left sheet draws on the cell's
  right side;
- `horizontal="left"` and `horizontal="right"` cells (they name the sides of the cell, so they do
  not swap), a wrapped cell, text that overflows into empty neighbours, a percentage and a date;
- notes on A4 and H4 (their indicators sit at the cell's top-left corner in a right-to-left
  sheet);
- a clustered bar chart of the totals anchored at A13 (charts are not mirrored);
- a second sheet, `Notes`, that runs left to right.

## Oracle

The file opens in LibreOffice 26.2 (`tools/oracle-soffice.sh sales.xlsx pdf <dir>`), which shows
the sheet right to left: column A at the right, the merged title, the data bars growing from the
right, the colour scale, the icons, the border on the right side of the total and the chart
unmirrored all agree with Kiln's render (`fixtures/golden/rtl/sales-0-*.png`). Where LibreOffice
differs from Excel, Kiln follows Excel: General numbers align to the left of the cell (the mirror
of their usual right alignment; LibreOffice keeps them at the right).
