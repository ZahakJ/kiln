# Charts

Kiln Grid, Kiln Write and Kiln Deck share one chart engine. A chart you open from an Excel workbook, a
Word document or a PowerPoint deck is drawn the way Office draws it, every setting Kiln does not edit
stays in the file, and a chart you make in Kiln opens in Office as an ordinary Office chart.

> **Status (WP-73).** The chart engine (reading, writing, drawing and every command below) and the chart
> controls are done; each app shows them once its editing surface is in: Grid with WP-71, Write with
> WP-64 and Deck with WP-78.

## Chart types

| Family | Types |
|---|---|
| Column | Clustered, Stacked, 100% Stacked |
| Bar | Clustered, Stacked, 100% Stacked |
| Line | Line, Line with Markers, Stacked, 100% Stacked, Smooth |
| Area | Area, Stacked, 100% Stacked |
| Pie | Pie, Pie with a pulled-out slice, Doughnut |
| X Y (Scatter) | Markers only, Straight Lines and Markers, Smooth Lines, Bubble |
| Radar | Radar, Radar with Markers, Filled Radar |
| Hierarchy | Treemap, Sunburst |
| Combo | Clustered Column – Line, with the line on a secondary axis or not |

3-D charts from Office files are drawn in 2-D. Stock, surface and bar-of-pie charts are kept in the
file unchanged and shown as an empty plot area.

## Making a chart

1. Select the cells, headings included (for example the quarters in column A and the amounts beside them).
2. **Insert ▸ Recommended Charts** shows the types that suit the selection, best first; or pick a type from
   the **Column**, **Line**, **Pie**, **Scatter**, **Hierarchy** or **Combo** galleries.

Kiln reads the selection the way Excel does: a top row of text names the series, a left column of
text or dates labels the categories, and series run down the columns when there are at least as many
rows as columns. Recommended Charts suggests a scatter chart when the first column holds numbers, a
line chart for dates or long runs, a pie chart for a single series of a few positive values, a combo
chart with a secondary axis when the series differ in size tenfold or more, and a treemap or sunburst
when two text columns form a hierarchy.

In Kiln Write and Kiln Deck, **Insert ▸ Chart** adds a chart with Office's sample data (three series
over four categories for most types; quarterly sales for a pie; a few points for a scatter or bubble
chart; branches and leaves for a treemap or sunburst), which the chart keeps inside itself. A chart
copied from Excel keeps its data in its own small workbook inside the document, as in Office.

## Changing a chart

Select the chart: the **Chart Design** and **Format** tabs appear.

- **Add Chart Element** (or the + next to the chart): titles, axes, axis titles, gridlines, data labels
  and their position, the legend and its position, and linear, exponential or moving-average trendlines.
- **Quick Layout**: Office's eleven layouts.
- **Change Colors** and **Chart Styles**: the colourful set, shades of one colour, or greys.
- **Switch Row/Column** swaps what is a series and what is a category.
- **Select Data** changes the range the chart reads and lists its series and category labels.
- **Change Chart Type** keeps the data, titles and formatting and changes the type, combos and
  hierarchy charts included. A combo chart needs at least two series; Kiln says so instead of
  making a chart of another type. Choosing **Pie** pulls exploded slices back in.
- **Move Chart** (Grid) puts the chart on its own chart sheet or on another sheet.
- **Shape Fill** and **Shape Outline** (Format tab) colour one series; the current colour is checked.
- **Format Selection** opens the Format Chart pane: the title, a series' fill and line (the colour in
  use is marked; **Automatic** shows the colour the chart picks), line width and secondary axis, and
  the options of each axis the chart has: the minimum, maximum and major unit the chart is drawn
  with, marked **Auto** while Kiln chooses them and with **Reset** once you set them, logarithmic
  scale, values (or categories) in reverse order, and the number format.

Every change can be undone with Ctrl+Z.

## How charts look

Axes get Excel's automatic bounds and round tick steps (1, 2 or 5 × a power of ten). When category labels
do not fit side by side they are slanted, and when they still do not fit only every second (third…) label
is shown. Legends wrap into rows when they sit above or below the chart. Data labels that would cover
each other move aside.

On screen, charts in documents that use Office's default theme are drawn in Kiln's own colours — jade,
ember, cobalt, saffron, clay and plum — tuned separately for the light and the dark theme, so a chart
never glares white in dark mode. The colours in the file do not change, and printing and PDF export use
the document's colours. Charts in documents with their own theme always use that theme's colours.

## What is kept in the file

Kiln reads and writes everything in the lists above, and keeps everything else it finds in a chart —
error bars, up/down bars, drop lines, 3-D settings, data tables, extensions written by newer versions of
Office — exactly as it was. Formatting you did not change is written back byte for byte. See
[File formats](../FORMATS.md#excel-workbooks-xlsx-xltx-xlsm--kiln-io-xlsx) for the details.
