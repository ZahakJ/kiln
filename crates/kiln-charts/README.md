# kiln-charts

Charts for Kiln Grid, Write and Deck: the chart model, its layout and rendering into a Scene,
reading and writing Office chart parts, and chart editing (PLAN.md §3.2, WP-73).

| Module | What it does |
|---|---|
| `model` | `ChartSpace` → `Chart` → `PlotArea` with chart groups (`GroupKind`: bar/column, line, area, pie, doughnut, scatter, bubble, radar, hierarchy for treemap and sunburst; anything else kept as `Other`), axes, series with references and cached values (`DataSource`: numbers, text, multi-level categories, or literal data), titles, legend, data labels, trendlines and `ShapeProps`/`TextProps` formatting. Optional fields are `None` when the file did not say. |
| `read`, `write`, `chartex` | `read_chart(bytes)` / `write_chart(&ChartSpace)` for `c:chartSpace` and `cx:chartSpace` parts. Children come out in schema order; unknown elements (`Extras`) go back where they were; unedited `c:spPr`, `c:txPr`, `c:rich` and `c:layout` are written as read. Hostile parts never panic (control characters are stripped on write, undeclared prefixes pruned). |
| `package` | The chart part handler for the format crates: `chart_parts`, `read_part`, `write_part` (fixes the content type when a chart changes vocabulary), `embedded_workbook` (Write/Deck charts' `embeddings/*.xlsx`), `package_chart_facts` for the fidelity scorer. |
| `data` | Chart references (`'Q1 sales'!$B$2:$B$9`, unions), `ChartData` (implemented for `kiln_sheet::Workbook`), `refresh` (fills every cache from the cells), `analyse_range` and `series_from_shape` (Excel's rules for turning a selection into series). |
| `scale` | Value axes: Excel's automatic bounds and nice major units, fixed bounds and units, log and reversed scales. |
| `palette` | Theme colours, Office's automatic series colours and chart styles 1–48, and Kiln's on-screen palette (jade, ember, cobalt, saffron, clay, plum; tuned for light and dark) used when the document has a default Office theme. |
| `text` | Labels through `kiln-text`: `TextMeasurer`, and `GlyphSink`s — `AtlasSink` (crisp bitmaps for upright text at the device scale) and `OutlineSink` (vectors, any rotation). |
| `layout` | `layout_chart` → `ChartLayout`: title, wrapping legend, axes with measured tick labels (rotated or thinned when they crowd), the plot area (or the file's manual layout) and every chart type's marks with resolved paints; data labels step aside when they would overlap; `hit_test` and `element_bounds` for selection. |
| `render` | `draw` (ops in chart px, data clipped to the plot), `render` (layout + draw), `scene_delta` (a one-tile Scene for thumbnails, goldens and previews) and `selection_ops` (frame, handles, element outline). |
| `edit` | `apply(&mut ChartSpace, &SuiteCommand, HostContext)` for every `SuiteCommand::Chart*`; `insert_from_range` with `recommend` (Recommended Charts), `change_type`, `switch_row_column`, `select_data`, `set_element`, `quick_layout` (1–11), `set_style`, `set_colors`, `format_series`, `format_axis`. Hosts keep the previous `ChartSpace` for undo. |
| `facts` | Structural facts of a chart for the fidelity scorer. |

## Using it

```rust
use kiln_charts::{edit, palette::ColorMode, render, text};

// Grid: insert a chart from the selection, then draw it at 480 × 300 px.
let mut chart = edit::insert_from_range(&workbook, "Budget", range, None)?; // Recommended Charts' pick
let mut measurer = text::TextMeasurer::new(text::default_fonts());
let mut sink = text::AtlasSink::new(device_scale, chart_origin_in_tile_device_px);
let (layout, ops) = render::render(&chart, 480.0, 300.0, Some(&theme), ColorMode::Screen { dark }, &mut measurer, &mut sink);
// … wrap `ops` in a PushTransform to the chart's position, upload `sink.updates(false)` with the tile.
edit::apply(&mut chart, &command, edit::HostContext { data: Some(&workbook), selection: None })?;
```

## Tests

- `tests/roundtrip.rs`: every chart part of `fixtures/{xlsx,pptx,docx}/features` and `fixtures/xlsx/charts`
  reads, writes and reads back with identical model facts and the same XML structure (1.000).
- `tests/golden.rs`: 18 sample charts (every type) in the light and dark screen themes and the nine
  `charts.pptx` charts in document colours, against `fixtures/golden/charts/*.png`
  (`KILN_UPDATE_GOLDEN=1` rewrites them; `KILN_CHART_PREVIEW=<dir>` also writes 2× renders).
- `tests/layout.rs`: legend wrapping, tick label collision avoidance, pie label placement, hit testing,
  every fixture at extreme sizes, and a property test feeding corrupted parts through read, layout, draw
  and write.
- Unit tests per module (scales, palette, references, Recommended Charts, every command).

`fixtures/xlsx/charts/generate.py` (openpyxl 3.1.5) regenerates the per-type fixture workbooks; the
treemap and sunburst parts are written as Excel 2016 lays them out.
