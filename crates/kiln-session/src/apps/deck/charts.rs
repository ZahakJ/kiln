// SPDX-License-Identifier: AGPL-3.0-or-later
//! Charts on slides (the model, editor and renderer, hosted by Deck): Insert ▸ Chart builds
//! the chart over an embedded data sheet as PowerPoint does, the Chart Design and Format commands
//! edit the selected chart (Switch Row/Column and Select Data read that sheet), and Edit Data
//! changes the sheet and the chart with it.
//!
//! A chart is three parts of the presentation's store: the chart part, its relationships part and
//! the workbook that relationship points at (`/ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx`).
//! Every change writes them in one transaction, so it is one undo step and a save writes what the
//! chart shows.

use kiln_charts::data::{ChartCell, ChartData, data_range};
use kiln_charts::edit::HostContext;
use kiln_charts::embed::{DATA_SHEET, Table, sample_table};
use kiln_charts::model::{ChartSpace, ExternalData};
use kiln_charts::package::PACKAGE_RELATIONSHIP;
use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, Transaction};
use kiln_deck::{Part, ShapeKind};
use kiln_ooxml::opc::{Relationships, relative_target, rels_part_name, resolve_target};
use kiln_ooxml::st::TargetMode;
use kiln_protocol::SuiteCommand;
use kiln_protocol::commands::ChartState;
use kiln_protocol::commands::deck_content::DeckChartData;
use kiln_sheet::Range;

use super::DeckDoc;
use super::chart_book::Book;
use crate::model::Change;

/// The media type of a chart's embedded workbook.
const XLSX_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
/// The media type of a relationships part.
const RELS_TYPE: &str = "application/vnd.openxmlformats-package.relationships+xml";
/// The most cells Edit Data shows and takes.
const MAX_DATA_CELLS: usize = 10_000;

/// A chart's embedded workbook.
struct DataSheet {
    /// Its part in the store.
    part: String,
    book: Book,
}

impl DataSheet {
    fn workbook(&self) -> &Book {
        &self.book
    }
}

/// What Edit Data says when a chart's workbook cannot be read (the chart and its workbook stay as
/// they are, as PowerPoint leaves a chart whose data it cannot open).
fn unreadable(why: &str) -> String {
    format!("The chart's data can't be edited: {why}. The chart and its data are kept as they are.")
}

/// The text Edit Data shows for a cell.
fn cell_text(c: &ChartCell) -> String {
    match c {
        ChartCell::Blank => String::new(),
        ChartCell::Number(v, code) => kiln_charts::format::format_number(*v, code),
        ChartCell::Text(t) => t.clone(),
    }
}

/// What a typed cell holds: a number when the text reads as one (`1,250.5`, `12%`), else text.
fn typed_value(text: &str) -> ChartCell {
    let t = text.trim();
    if t.is_empty() {
        return ChartCell::Blank;
    }
    let plain: String = t.chars().filter(|c| *c != ',').collect();
    if let Some(p) = plain.strip_suffix('%')
        && let Ok(v) = p.trim().parse::<f64>()
        && v.is_finite()
    {
        return ChartCell::Number(v / 100.0, "0%".into());
    }
    match plain.parse::<f64>() {
        Ok(v) if v.is_finite() => ChartCell::Number(v, "General".into()),
        _ => ChartCell::Text(t.into()),
    }
}

impl DeckDoc {
    /// The selected chart: its shape and chart part.
    fn selected_chart(&self) -> Option<(NodeId, String)> {
        self.view
            .shapes
            .iter()
            .find_map(|id| match &self.pres.shape(*id)?.kind {
                ShapeKind::Chart(c) => Some((*id, c.part.clone())),
                _ => None,
            })
    }

    fn read_chart(&self, part: &str) -> Result<ChartSpace> {
        let bytes = self
            .pres
            .parts
            .get(part)
            .map(|p| p.bytes.clone())
            .ok_or_else(|| KilnError::InvalidInput("the chart's data is missing".into()))?;
        kiln_charts::read::read_chart(&bytes).map_err(|e| KilnError::InvalidInput(e.to_string()))
    }

    /// The chart's embedded workbook, when it has one in the store; the error says why the
    /// workbook it has cannot be read.
    fn data_sheet(
        &self,
        part: &str,
        chart: &ChartSpace,
    ) -> std::result::Result<Option<DataSheet>, String> {
        let Some(ext) = chart.external_data.as_ref() else {
            return Ok(None);
        };
        let Some(rels) = self
            .pres
            .parts
            .get(&rels_part_name(part))
            .and_then(|r| Relationships::parse(part, &r.bytes).ok())
        else {
            return Ok(None);
        };
        let Some(rel) = rels.get(&ext.rel_id) else {
            return Ok(None);
        };
        if rel.is_external() {
            return Err("its data is in a linked workbook outside the presentation".into());
        }
        let target = resolve_target(part, &rel.target);
        let Some(stored) = self.pres.parts.get(&target) else {
            return Ok(None);
        };
        let sheet = data_range(chart).map_or_else(|| DATA_SHEET.to_owned(), |a| a.sheet);
        let book = Book::read(&stored.bytes, &sheet)?;
        Ok(Some(DataSheet { part: target, book }))
    }

    /// A free part name of the store from a pattern with `{n}`.
    fn free_name(&self, pattern: &str) -> String {
        (1u32..)
            .map(|n| pattern.replace("{n}", &n.to_string()))
            .find(|p| !self.pres.parts.contains_key(p))
            .unwrap_or_else(|| pattern.replace("{n}", "0"))
    }

    /// The ops that store a chart, its workbook and the relationship between them (new parts
    /// for a chart that had none).
    fn chart_ops(
        &self,
        part: &str,
        chart: &mut ChartSpace,
        workbook: Option<(&str, Vec<u8>)>,
    ) -> Vec<Op> {
        let mut ops = Vec::new();
        if let Some((sheet_part, bytes)) = workbook {
            let rels_key = rels_part_name(part);
            let mut rels = self
                .pres
                .parts
                .get(&rels_key)
                .and_then(|p| Relationships::parse(part, &p.bytes).ok())
                .unwrap_or_else(|| Relationships::new(part));
            let target = relative_target(part, sheet_part);
            let known = chart
                .external_data
                .as_ref()
                .and_then(|e| rels.get(&e.rel_id))
                .is_some_and(|r| resolve_target(part, &r.target) == sheet_part);
            if !known {
                if let Ok(rel) = rels.add(PACKAGE_RELATIONSHIP, &target, None, TargetMode::Internal)
                {
                    chart.external_data = Some(ExternalData {
                        rel_id: rel.id,
                        auto_update: Some(false),
                    });
                }
                ops.push(Op::SetPart {
                    key: rels_key,
                    part: Some(Part {
                        content_type: RELS_TYPE.into(),
                        bytes: rels.to_bytes(),
                    }),
                });
            }
            ops.push(Op::SetPart {
                key: sheet_part.to_owned(),
                part: Some(Part {
                    content_type: XLSX_TYPE.into(),
                    bytes,
                }),
            });
        }
        // A classic chart changed into a treemap is a chartex part (the slide's relationship to
        // it follows the content type on save).
        ops.push(Op::SetPart {
            key: part.to_owned(),
            part: Some(Part {
                content_type: chart.format.content_type().to_owned(),
                bytes: kiln_charts::write::write_chart(chart),
            }),
        });
        ops
    }

    /// Insert ▸ Chart (Office's sample data on an embedded data sheet) or a chart command on the
    /// selected chart.
    pub(crate) fn chart_command(&mut self, cmd: &SuiteCommand) -> Result<Change> {
        if let SuiteCommand::ChartInsert { kind } = cmd {
            return self
                .insert_chart(kind.unwrap_or(kiln_protocol::commands::ChartKind::ColumnClustered));
        }
        if matches!(cmd, SuiteCommand::ChartMove { .. }) {
            return Err(KilnError::unavailable(
                cmd.meta().id,
                "a chart on a slide is moved by dragging it",
            ));
        }
        let Some((_, part)) = self.selected_chart() else {
            return Err(KilnError::unavailable(
                cmd.meta().id,
                "select a chart first",
            ));
        };
        let mut chart = self.read_chart(&part)?;
        let sheet = self.data_sheet(&part, &chart).ok().flatten();
        let changed = kiln_charts::edit::apply(
            &mut chart,
            cmd,
            HostContext {
                data: sheet.as_ref().map(|s| s.workbook() as &dyn ChartData),
                selection: None,
            },
        )
        .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        if !changed {
            return Ok(Change::None);
        }
        let ops = self.chart_ops(&part, &mut chart, None);
        self.apply(cmd.meta().id, Transaction::new(ops))?;
        self.chart_changed();
        Ok(Change::Content)
    }

    /// A chart of `kind` over a new data sheet, in the slide's empty content placeholder or in
    /// the middle of the slide.
    fn insert_chart(&mut self, kind: kiln_protocol::commands::ChartKind) -> Result<Change> {
        let table = sample_table(kind);
        let range = table
            .range()
            .ok_or_else(|| KilnError::InvalidInput("no sample data".into()))?;
        let mut chart = kiln_charts::edit::insert_from_range(&table, DATA_SHEET, range, Some(kind))
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        // PowerPoint's new charts have no chart-area fill or border: the slide shows through.
        chart
            .shape
            .set_fill(Some(kiln_ooxml::drawingml::fill::Fill::None));
        chart
            .shape
            .set_line(Some(kiln_ooxml::drawingml::line::Line {
                fill: Some(kiln_ooxml::drawingml::fill::Fill::None),
                ..kiln_ooxml::drawingml::line::Line::default()
            }));
        let stem = match chart.format {
            kiln_charts::model::PartFormat::Classic => "/ppt/charts/chart{n}.xml",
            kiln_charts::model::PartFormat::ChartEx => "/ppt/charts/chartEx{n}.xml",
        };
        let part = self.free_name(stem);
        let sheet_part = self.free_name("/ppt/embeddings/Microsoft_Excel_Worksheet{n}.xlsx");
        let bytes = Book::new(table.clone())?.write()?;
        let ops = self.chart_ops(&part, &mut chart, Some((&sheet_part, bytes)));
        self.end_text();
        self.begin_group();
        let r = match self.place_object(640.0, 360.0, false, false) {
            Ok(r) => r,
            Err(e) => {
                self.end_group();
                return Err(e);
            }
        };
        let shape = kiln_deck::edit::new_chart(&mut self.pres, &part, super::shapes::rect_emu(&r));
        let id = shape.id;
        let slide = self.slide_id()?;
        let mut tx = Transaction::new(ops);
        let ins = kiln_deck::edit::insert_shapes(&mut self.pres, slide, vec![shape], None)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        tx.ops.extend(ins.ops);
        tx.selection_after = vec![id];
        let applied = self.apply("suite.chart.insert", tx);
        self.end_group();
        applied?;
        self.view.shapes = vec![id];
        Ok(Change::Content)
    }

    /// Redraws the slide that shows the chart (its thumbnail too).
    fn chart_changed(&mut self) {
        if let Some(slide) = self.current_slide() {
            *self.slide_versions.entry(slide).or_insert(0) += 1;
        }
    }

    /// Edit Data: the cells of the selected chart's data range, as the data sheet shows them
    /// (a chart without a sheet shows the data it holds, laid out as a new one).
    pub(crate) fn chart_data(&self) -> Option<DeckChartData> {
        let (_, part) = self.selected_chart()?;
        let chart = self.read_chart(&part).ok()?;
        let sheet = self.data_sheet(&part, &chart);
        let (cells, area) = match (&sheet, data_range(&chart)) {
            (Ok(Some(s)), Some(area)) => (read_area(s.workbook(), &area.sheet, area.range)?, area),
            _ => {
                let table = kiln_charts::embed::table_of(&chart);
                let range = table.range()?;
                let cells = read_area(&table, DATA_SHEET, range)?;
                (
                    cells,
                    kiln_charts::data::Area {
                        sheet: DATA_SHEET.into(),
                        range,
                    },
                )
            }
        };
        Some(DeckChartData {
            sheet: area.sheet,
            row: area.range.r0,
            col: area.range.c0,
            cells,
            unreadable: sheet.err().map(|why| unreadable(&why)),
        })
    }

    /// Edit Data: puts `cells` on the chart's data sheet where its data range starts and points
    /// the chart at them (a range of another size is the chart's new data range, as when its
    /// outline is dragged in PowerPoint's data sheet).
    pub(crate) fn set_chart_data(&mut self, cells: &[Vec<String>]) -> Result<Change> {
        const ID: &str = "deck.chart.set_data";
        let Some((_, part)) = self.selected_chart() else {
            return Err(KilnError::unavailable(ID, "select a chart first"));
        };
        let rows = cells.len();
        let cols = cells.iter().map(Vec::len).max().unwrap_or(0);
        if rows < 2 || cols < 2 {
            return Err(KilnError::InvalidInput(
                "a chart's data has a row of names and a column of labels".into(),
            ));
        }
        if rows * cols > MAX_DATA_CELLS {
            return Err(KilnError::InvalidInput(format!(
                "Edit Data takes at most {MAX_DATA_CELLS} cells"
            )));
        }
        let mut chart = self.read_chart(&part)?;
        // A workbook that cannot be read is never replaced: the chart keeps it.
        let sheet = self
            .data_sheet(&part, &chart)
            .map_err(|why| KilnError::InvalidInput(unreadable(&why)))?;
        let before = self.chart_data();
        let (sheet_name, r0, c0) = before.as_ref().map_or((DATA_SHEET.to_owned(), 0, 0), |d| {
            (d.sheet.clone(), d.row, d.col)
        });
        let (sheet_part, mut book) = match sheet {
            Some(s) => (s.part, s.book),
            None => (
                self.free_name("/ppt/embeddings/Microsoft_Excel_Worksheet{n}.xlsx"),
                Book::new(Table::new(&sheet_name))?,
            ),
        };
        let at = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
        let shown = |r: usize, c: usize| {
            before
                .as_ref()
                .and_then(|b| b.cells.get(r))
                .and_then(|row| row.get(c))
        };
        // The cells of the old range the new one leaves out are cleared.
        if let Some(old) = &before {
            for (r, row) in old.cells.iter().enumerate() {
                for c in 0..row.len() {
                    let inside = cells.get(r).is_some_and(|line| c < line.len());
                    if !inside
                        && book
                            .table
                            .rows
                            .get(at(r0) + r)
                            .is_some_and(|l| at(c0) + c < l.len())
                    {
                        book.table.set(at(r0) + r, at(c0) + c, ChartCell::Blank);
                    }
                }
            }
        }
        // Only the cells typed into change: the others keep their values, formulas and formats.
        for (r, row) in cells.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                if shown(r, c) == Some(text) {
                    continue;
                }
                let (row, col) = (at(r0) + r, at(c0) + c);
                let value = match (typed_value(text), book.format_at(row, col)) {
                    // A number typed into a cell keeps the cell's number format, as in Excel,
                    // unless it is typed as a percentage into a cell that is not one.
                    (ChartCell::Number(v, code), Some(was))
                        if code == "General" || was.contains('%') =>
                    {
                        ChartCell::Number(v, was.to_owned())
                    }
                    (value, _) => value,
                };
                book.table.set(row, col, value);
            }
        }
        let same_size = before.as_ref().is_some_and(|b| {
            b.cells.len() == rows && b.cells.iter().map(Vec::len).max().unwrap_or(0) == cols
        });
        let area = Range::new(r0, c0, r0 + to_u32(rows - 1), c0 + to_u32(cols - 1));
        if let Some(b) = &before {
            let old_cols = to_u32(b.cells.iter().map(Vec::len).max().unwrap_or(1).max(1));
            let old_rows = to_u32(b.cells.len().max(1));
            book.follow(
                Range::new(r0, c0, r0 + old_rows - 1, c0 + old_cols - 1),
                area,
            )?;
        }
        if same_size && chart.external_data.is_some() {
            kiln_charts::data::refresh(&mut chart, &book);
        } else {
            let area = kiln_charts::data::Area {
                sheet: book.table.sheet.clone(),
                range: area,
            };
            kiln_charts::edit::select_data(&mut chart, &book, &area.to_formula())
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        }
        let bytes = book.write()?;
        let ops = self.chart_ops(&part, &mut chart, Some((&sheet_part, bytes)));
        self.apply(ID, Transaction::new(ops))?;
        self.chart_changed();
        Ok(Change::Content)
    }

    /// What Chart Design and Format show for the selected chart.
    pub(crate) fn chart_ui_state(&self) -> Option<ChartState> {
        if self.view.shapes.len() != 1 {
            return None;
        }
        let (id, part) = self.selected_chart()?;
        let chart = self.read_chart(&part).ok()?;
        let r = kiln_deck_layout::geom::rect_px(&self.xfrm_of(id)?);
        let owner = self.owner()?;
        let mut state = self
            .renderer
            .borrow_mut()
            .chart_state(&self.pres, owner, &chart, r.w, r.h)?;
        state.has_cells = state.has_cells && matches!(self.data_sheet(&part, &chart), Ok(Some(_)));
        Some(state)
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The cells of `range` on `sheet` as Edit Data shows them.
fn read_area(data: &dyn ChartData, sheet: &str, range: Range) -> Option<Vec<Vec<String>>> {
    if range.cell_count() > MAX_DATA_CELLS as u64 {
        return None;
    }
    (range.r0..=range.r1)
        .map(|r| {
            (range.c0..=range.c1)
                .map(|c| data.cell(sheet, r, c).map(|x| cell_text(&x)))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use kiln_charts::model::DataSource;
    use kiln_protocol::commands::{ChartElementKind, ChartKind, DeckCommand};
    use kiln_protocol::{Command, FileFormat, SuiteCommand};

    use super::super::DeckDoc;
    use super::*;
    use crate::model::DocModel;

    fn suite(d: &mut DeckDoc, c: SuiteCommand) {
        d.execute(&Command::Suite(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }

    fn deck(d: &mut DeckDoc, c: DeckCommand) {
        d.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }

    /// A Title and Content slide with a chart of `kind` in its content placeholder.
    fn with_chart(kind: ChartKind) -> DeckDoc {
        let mut d = DeckDoc::new(kiln_deck::Presentation::new(), None, None);
        deck(&mut d, DeckCommand::NewSlide { layout: None });
        suite(&mut d, SuiteCommand::ChartInsert { kind: Some(kind) });
        d
    }

    /// A presentation opened from `.pptx` bytes, as the app opens it.
    fn open(bytes: &[u8]) -> DeckDoc {
        let read = kiln_io_pptx::read_pptx(bytes).expect("open");
        let (pres, original) = super::super::save::Original::from_read(read, bytes);
        DeckDoc::new(pres, Some(original), None)
    }

    fn reopen(d: &DeckDoc) -> DeckDoc {
        let mut again = open(&d.save(FileFormat::Pptx).expect("save"));
        again.view.current = 1;
        let chart = again.pres.slides[1]
            .shapes
            .iter()
            .find(|s| matches!(s.kind, ShapeKind::Chart(_)))
            .map(|s| s.id)
            .expect("the chart is on the slide");
        again.view.shapes = vec![chart];
        again
    }

    fn values(d: &DeckDoc) -> Vec<Vec<Option<f64>>> {
        let (_, part) = d.selected_chart().expect("a chart");
        let cs = d.read_chart(&part).expect("chart");
        cs.chart
            .plot_area
            .groups
            .iter()
            .flat_map(|g| g.series.iter())
            .map(|s| match &s.values {
                Some(DataSource::Num(n)) => n.points.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn every_kind_is_inserted_over_a_data_sheet_and_reopens() {
        for kind in [
            ChartKind::ColumnClustered,
            ChartKind::BarClustered,
            ChartKind::Line,
            ChartKind::Area,
            ChartKind::Pie,
            ChartKind::Doughnut,
            ChartKind::ScatterMarkers,
            ChartKind::Bubble,
            ChartKind::ComboColumnLine,
            ChartKind::Treemap,
            ChartKind::Sunburst,
            ChartKind::Radar,
        ] {
            let d = with_chart(kind);
            let st = d.deck_state();
            assert_eq!(st.contexts, vec!["chart".to_owned()], "{kind:?}");
            let chart = st.chart.expect("Chart Design shows");
            assert_eq!(chart.kind, Some(kind), "{kind:?}");
            assert!(chart.has_cells, "{kind:?}: the chart reads its data sheet");
            let data = d.chart_data().expect("Edit Data");
            assert_eq!(data.sheet, "Sheet1");
            assert_eq!(
                data.cells,
                read_area(
                    &sample_table(kind),
                    DATA_SHEET,
                    sample_table(kind).range().unwrap()
                )
                .unwrap(),
                "{kind:?}"
            );
            let again = reopen(&d);
            assert_eq!(values(&again), values(&d), "{kind:?}");
            assert_eq!(again.chart_data(), Some(data), "{kind:?}");
            assert_eq!(again.chart_ui_state().and_then(|c| c.kind), Some(kind));
        }
    }

    #[test]
    fn edit_data_changes_values_and_adds_a_series() {
        let mut d = with_chart(ChartKind::ColumnClustered);
        let mut cells = d.chart_data().expect("data").cells;
        cells[1][1] = "10".into();
        deck(
            &mut d,
            DeckCommand::SetChartData {
                cells: cells.clone(),
            },
        );
        assert_eq!(values(&d)[0][0], Some(10.0));
        // A fourth series: the range grows by a column.
        for (i, row) in cells.iter_mut().enumerate() {
            row.push(if i == 0 {
                "Kilns".into()
            } else {
                format!("{i}")
            });
        }
        deck(
            &mut d,
            DeckCommand::SetChartData {
                cells: cells.clone(),
            },
        );
        assert_eq!(values(&d).len(), 4);
        assert_eq!(
            values(&d)[3],
            vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0)]
        );
        let st = d.chart_ui_state().expect("state");
        assert_eq!(st.series.last().map(|s| s.name.as_str()), Some("Kilns"));
        // One undo step per edit.
        assert!(d.undo());
        assert_eq!(values(&d).len(), 3);
        let again = reopen(&d);
        assert_eq!(values(&again)[0][0], Some(10.0));
        assert_eq!(
            again.chart_data().map(|c| c.cells[1][1].clone()),
            Some("10".into())
        );
    }

    #[test]
    fn chart_design_edits_the_selected_chart_with_its_sheet() {
        let mut d = with_chart(ChartKind::ColumnClustered);
        suite(&mut d, SuiteCommand::ChartSwitchRowColumn);
        // Four series of three values: the categories are the old series.
        assert_eq!(values(&d).len(), 4);
        suite(
            &mut d,
            SuiteCommand::ChartChangeType {
                kind: ChartKind::Line,
            },
        );
        suite(&mut d, SuiteCommand::ChartSetStyle { style: 4 });
        suite(
            &mut d,
            SuiteCommand::ChartSetElement {
                element: ChartElementKind::PrimaryVerticalAxisTitle,
                on: true,
            },
        );
        let again = reopen(&d);
        let st = again.chart_ui_state().expect("state");
        assert_eq!(st.kind, Some(ChartKind::Line));
        assert_eq!(st.style, Some(4));
        assert_eq!(values(&again).len(), 4);
        // Without a chart selected the commands are refused.
        d.view.shapes.clear();
        assert!(
            d.execute(&Command::Suite(SuiteCommand::ChartSwitchRowColumn))
                .is_err()
        );
    }

    /// The workbook of `charts.pptx`'s first chart (made by PowerPoint: categories down A2:A5,
    /// series North, South and East across B1:D1, every data cell in style 1).
    const BOOK: &str = "/ppt/embeddings/Microsoft_Excel_Sheet1.xlsx";
    const SHEET: &str = "/xl/worksheets/sheet1.xml";

    fn part_text(pkg: &kiln_ooxml::opc::OpcPackage, name: &str) -> String {
        String::from_utf8(pkg.part(name).expect("read").expect(name).into_owned()).expect("utf-8")
    }

    fn replaced(text: &str, edits: &[(&str, &str)]) -> String {
        edits.iter().fold(text.to_owned(), |t, (a, b)| {
            assert!(t.contains(a), "{a} is in the part");
            t.replacen(a, b, 1)
        })
    }

    /// `charts.pptx` with its first chart's workbook changed by `edit`, opened with that chart
    /// selected.
    fn with_book(edit: impl FnOnce(&mut kiln_ooxml::opc::OpcPackage)) -> DeckDoc {
        let mut deck_pkg =
            kiln_ooxml::opc::OpcPackage::open(&super::super::tests::fixture("charts.pptx"))
                .expect("pptx");
        let mut book =
            kiln_ooxml::opc::OpcPackage::open(&deck_pkg.part(BOOK).expect("read").expect("book"))
                .expect("xlsx");
        edit(&mut book);
        deck_pkg
            .set_part(BOOK, book.save().expect("xlsx"))
            .expect("set");
        let mut d = open(&deck_pkg.save().expect("pptx"));
        select_chart(&mut d, "/ppt/charts/chart1.xml");
        d
    }

    fn select_chart(d: &mut DeckDoc, part: &str) {
        let (slide, shape) = d
            .pres
            .slides
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                s.shapes
                    .iter()
                    .find(|x| matches!(&x.kind, ShapeKind::Chart(c) if c.part == part))
                    .map(|x| (i, x.id))
            })
            .expect("the chart");
        d.view.current = slide;
        d.view.shapes = vec![shape];
    }

    /// The workbook PowerPoint made, by hand given what Edit Data must keep: C2:C5 in a percent
    /// style, row 3 hidden with a height of its own, a formula in D5 that the calculation chain
    /// names.
    fn styled(book: &mut kiln_ooxml::opc::OpcPackage) {
        let sheet = replaced(
            &part_text(book, SHEET),
            &[
                (
                    r#"<row r="3" spans="1:4">"#,
                    r#"<row r="3" spans="1:4" ht="18" customHeight="1" hidden="1">"#,
                ),
                (
                    r#"<c r="C2" s="1"><v>22.3</v>"#,
                    r#"<c r="C2" s="2"><v>0.223</v>"#,
                ),
                (
                    r#"<c r="C3" s="1"><v>28.6</v>"#,
                    r#"<c r="C3" s="2"><v>0.286</v>"#,
                ),
                (
                    r#"<c r="C4" s="1"><v>15.2</v>"#,
                    r#"<c r="C4" s="2"><v>0.152</v>"#,
                ),
                (
                    r#"<c r="C5" s="1"><v>18.4</v>"#,
                    r#"<c r="C5" s="2"><v>0.184</v>"#,
                ),
                (
                    r#"<c r="D5" s="1"><v>21.3</v>"#,
                    r#"<c r="D5" s="1"><f>D2+D4</f><v>31.8</v>"#,
                ),
            ],
        );
        book.set_part(SHEET, sheet.into_bytes()).expect("sheet");
        // Style 2: a percentage with two decimals (Excel's built-in format 10).
        let styles = replaced(
            &part_text(book, "/xl/styles.xml"),
            &[
                (r#"<cellXfs count="2">"#, r#"<cellXfs count="3">"#),
                (
                    "</cellXfs>",
                    r#"<xf numFmtId="10" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>"#,
                ),
            ],
        );
        book.set_part("/xl/styles.xml", styles.into_bytes())
            .expect("styles");
        let chain = kiln_ooxml::xml::XmlElement::new("calcChain")
            .with_attr(
                "xmlns",
                "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
            )
            .with_child(
                kiln_ooxml::xml::XmlElement::new("c")
                    .with_attr("r", "D5")
                    .with_attr("i", 1),
            );
        book.add_xml_part(
            "/xl/calcChain.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml",
            &chain,
        )
        .expect("chain");
        book.add_relationship(
            "/xl/workbook.xml",
            kiln_ooxml::opc::rel_type::CALC_CHAIN,
            "/xl/calcChain.xml",
            kiln_ooxml::opc::RelationshipOptions::default(),
        )
        .expect("rel");
    }

    /// The chart's workbook as saved.
    fn saved_book(d: &DeckDoc) -> kiln_ooxml::opc::OpcPackage {
        let bytes = d.save(FileFormat::Pptx).expect("save");
        let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes).expect("pptx");
        kiln_ooxml::opc::OpcPackage::open(&pkg.part(BOOK).expect("read").expect("book"))
            .expect("xlsx")
    }

    /// The `c` element of `cell` in a sheet's XML.
    fn cell_xml<'a>(sheet: &'a str, cell: &str) -> &'a str {
        let at = sheet
            .find(&format!(r#"<c r="{cell}""#))
            .unwrap_or_else(|| panic!("{cell} is in {sheet}"));
        let rest = &sheet[at..];
        let end = rest
            .find("</c>")
            .map_or_else(|| rest.find("/>").unwrap_or(0) + 2, |e| e + 4);
        &rest[..end]
    }

    fn format_codes(d: &DeckDoc) -> Vec<Option<String>> {
        let (_, part) = d.selected_chart().expect("a chart");
        let cs = d.read_chart(&part).expect("chart");
        cs.chart
            .plot_area
            .groups
            .iter()
            .flat_map(|g| g.series.iter())
            .map(|s| match &s.values {
                Some(DataSource::Num(n)) => n.format_code.clone(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn edit_data_keeps_a_powerpoint_workbooks_styles_formulas_and_hidden_rows() {
        let mut d = with_book(styled);
        let data = d.chart_data().expect("Edit Data");
        assert_eq!(data.cells[1][2], "22.30%", "the percent style is read");
        assert_eq!(data.cells[4][3], "31.8", "the formula's value is read");
        let mut cells = data.cells.clone();
        cells[1][1] = "20".into();
        deck(
            &mut d,
            DeckCommand::SetChartData {
                cells: cells.clone(),
            },
        );
        // The hidden row is not plotted, as in PowerPoint (`c:plotVisOnly`).
        let v = values(&d);
        assert_eq!(v[0], vec![Some(20.0), Some(16.7), Some(25.1)]);
        assert_eq!(v[2], vec![Some(12.0), Some(19.8), Some(31.8)]);
        // The caches take the workbook's formats.
        assert_eq!(
            format_codes(&d),
            vec![
                Some("General".to_owned()),
                Some("0.00%".to_owned()),
                Some("General".to_owned())
            ]
        );
        let book = saved_book(&d);
        let sheet = part_text(&book, SHEET);
        assert_eq!(cell_xml(&sheet, "B2"), r#"<c r="B2" s="1"><v>20</v></c>"#);
        assert_eq!(
            cell_xml(&sheet, "C2"),
            r#"<c r="C2" s="2"><v>0.223</v></c>"#
        );
        assert_eq!(
            cell_xml(&sheet, "A2"),
            r#"<c r="A2" s="1" t="s"><v>0</v></c>"#
        );
        assert_eq!(
            cell_xml(&sheet, "D5"),
            r#"<c r="D5" s="1"><f>D2+D4</f><v>31.8</v></c>"#
        );
        assert!(
            sheet.contains(r#"<row r="3" spans="1:4" ht="18" customHeight="1" hidden="1">"#),
            "{sheet}"
        );
        assert!(book.has("/xl/calcChain.xml"), "no formula was typed over");
        // Typing over the formula leaves a value, as in Excel; a percentage typed into a cell
        // without one gets a style with Excel's 0% format.
        cells[4][3] = "30".into();
        cells[2][1] = "12%".into();
        deck(
            &mut d,
            DeckCommand::SetChartData {
                cells: cells.clone(),
            },
        );
        let book = saved_book(&d);
        let sheet = part_text(&book, SHEET);
        assert_eq!(cell_xml(&sheet, "D5"), r#"<c r="D5" s="1"><v>30</v></c>"#);
        assert!(
            !book.has("/xl/calcChain.xml"),
            "the chain named the formula"
        );
        let b3 = cell_xml(&sheet, "B3");
        assert!(b3.contains("<v>0.12</v>"), "{b3}");
        let styles = part_text(&book, "/xl/styles.xml");
        assert!(b3.contains(r#"s="3""#), "{b3}");
        assert!(
            styles.contains(r#"<cellXfs count="4">"#) && styles.contains(r#"numFmtId="9""#),
            "{styles}"
        );
        let again = reopen_chart(&d);
        let data = again.chart_data().expect("data");
        assert_eq!(data.cells[2][1], "12%");
        assert_eq!(data.cells[4][3], "30");
        assert_eq!(data.cells[1][2], "22.30%");
    }

    fn reopen_chart(d: &DeckDoc) -> DeckDoc {
        let mut again = open(&d.save(FileFormat::Pptx).expect("save"));
        select_chart(&mut again, "/ppt/charts/chart1.xml");
        again
    }

    #[test]
    fn a_percentage_typed_into_a_new_chart_is_plotted_and_saved_as_one() {
        let mut d = with_chart(ChartKind::ColumnClustered);
        let mut cells = d.chart_data().expect("data").cells;
        cells[1][1] = "12%".into();
        deck(&mut d, DeckCommand::SetChartData { cells });
        assert_eq!(values(&d)[0][0], Some(0.12));
        assert_eq!(format_codes(&d)[0].as_deref(), Some("0%"));
        let again = reopen(&d);
        assert_eq!(
            again.chart_data().map(|c| c.cells[1][1].clone()),
            Some("12%".into())
        );
    }

    #[test]
    fn edit_data_keeps_the_sheets_tables_and_names_their_columns_as_excel_does() {
        let mut d = with_book(|book| {
            let sheet = replaced(
                &part_text(book, SHEET),
                &[(
                    "</worksheet>",
                    r#"<tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#,
                )],
            );
            book.set_part(SHEET, sheet.into_bytes()).expect("sheet");
            let table = concat!(
                r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" "#,
                r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
                r#"xmlns:xr3="http://schemas.microsoft.com/office/spreadsheetml/2016/revision3" "#,
                r#"mc:Ignorable="xr3" id="1" name="Table1" displayName="Table1" ref="A1:D5" totalsRowShown="0">"#,
                r#"<autoFilter ref="A1:D5"/><tableColumns count="4">"#,
                r#"<tableColumn id="1" xr3:uid="{00000001-0000-0000-0000-000000000000}" name="Column1"/>"#,
                r#"<tableColumn id="2" xr3:uid="{00000002-0000-0000-0000-000000000000}" name="North" dataDxfId="3"/>"#,
                r#"<tableColumn id="3" xr3:uid="{00000003-0000-0000-0000-000000000000}" name="South">"#,
                r#"<calculatedColumnFormula>Table1[[#This Row],[North]]*2</calculatedColumnFormula></tableColumn>"#,
                r#"<tableColumn id="4" xr3:uid="{00000004-0000-0000-0000-000000000000}" name="East"/>"#,
                r#"</tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#,
            );
            book.add_part(
                "/xl/tables/table1.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
                table.as_bytes().to_vec(),
            )
            .expect("table");
            book.add_relationship(
                SHEET,
                kiln_ooxml::opc::rel_type::TABLE,
                "/xl/tables/table1.xml",
                kiln_ooxml::opc::RelationshipOptions {
                    id: Some("rId1"),
                    ..kiln_ooxml::opc::RelationshipOptions::default()
                },
            )
            .expect("rel");
        });
        // A fifth series named like the first.
        let mut cells = d.chart_data().expect("data").cells;
        for (i, row) in cells.iter_mut().enumerate() {
            row.push(if i == 0 {
                "North".into()
            } else {
                format!("{i}")
            });
        }
        deck(&mut d, DeckCommand::SetChartData { cells });
        let st = d.chart_ui_state().expect("state");
        assert_eq!(st.series.last().map(|s| s.name.as_str()), Some("North2"));
        let book = saved_book(&d);
        let sheet = part_text(&book, SHEET);
        assert!(cell_xml(&sheet, "E1").contains("<t>North2</t>"), "{sheet}");
        let table = part_text(&book, "/xl/tables/table1.xml");
        for kept in [
            r#"ref="A1:E5""#,
            r#"<autoFilter ref="A1:E5"/>"#,
            r#"<tableColumns count="5">"#,
            r#"<tableColumn id="1" xr3:uid="{00000001-0000-0000-0000-000000000000}" name="Column1"/>"#,
            r#"<tableColumn id="2" xr3:uid="{00000002-0000-0000-0000-000000000000}" name="North" dataDxfId="3"/>"#,
            r#"<calculatedColumnFormula>Table1[[#This Row],[North]]*2</calculatedColumnFormula>"#,
            r#"<tableColumn id="5" name="North2"/>"#,
        ] {
            assert!(table.contains(kept), "{kept} in {table}");
        }
    }

    #[test]
    fn new_cells_take_the_prefix_the_sheet_is_written_with() {
        let mut d = with_book(|book| {
            // The same sheet with its namespace bound to `x:`, as some writers do.
            let sheet = part_text(book, SHEET);
            let mut prefixed = String::new();
            let mut chars = sheet.chars().peekable();
            while let Some(c) = chars.next() {
                prefixed.push(c);
                if c == '<' {
                    match chars.peek() {
                        Some('/') => {
                            prefixed.push('/');
                            chars.next();
                            prefixed.push_str("x:");
                        }
                        Some(n) if n.is_ascii_alphabetic() => prefixed.push_str("x:"),
                        _ => {}
                    }
                }
            }
            let prefixed = prefixed.replacen("xmlns=", "xmlns:x=", 1);
            book.set_part(SHEET, prefixed.into_bytes()).expect("sheet");
        });
        let mut cells = d.chart_data().expect("data").cells;
        cells.push(vec!["Q5".into(), "1".into(), "2".into(), "3".into()]);
        deck(&mut d, DeckCommand::SetChartData { cells });
        assert_eq!(values(&d)[0].len(), 5);
        let sheet = part_text(&saved_book(&d), SHEET);
        assert!(
            sheet.contains(r#"<x:row r="6"><x:c r="A6" t="inlineStr"><x:is><x:t>Q5</x:t></x:is></x:c><x:c r="B6"><x:v>1</x:v></x:c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<x:dimension ref="A1:D6"/>"#), "{sheet}");
        let again = reopen_chart(&d);
        assert_eq!(again.chart_data().map(|c| c.cells.len()), Some(6));
    }

    #[test]
    fn edit_data_refuses_a_workbook_it_cannot_read_and_keeps_it() {
        // An Excel 97 workbook (a compound file) in the chart's embedding.
        let xls = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1 not an Office Open XML package".to_vec();
        let mut pkg =
            kiln_ooxml::opc::OpcPackage::open(&super::super::tests::fixture("charts.pptx"))
                .expect("pptx");
        pkg.set_part(BOOK, xls.clone()).expect("set");
        let mut d = open(&pkg.save().expect("pptx"));
        select_chart(&mut d, "/ppt/charts/chart1.xml");
        let parts: Vec<String> = d.pres.parts.keys().cloned().collect();
        let data = d.chart_data().expect("the chart's own values are shown");
        assert!(
            data.unreadable
                .as_deref()
                .is_some_and(|m| m.contains("can't be edited")),
            "{data:?}"
        );
        let mut cells = data.cells;
        cells[1][1] = "99".into();
        let r = d.execute(&Command::Deck(DeckCommand::SetChartData { cells }));
        assert!(r.is_err(), "Edit Data is refused");
        assert_eq!(d.pres.parts.keys().cloned().collect::<Vec<_>>(), parts);
        assert_eq!(d.pres.parts.get(BOOK).map(|p| p.bytes.clone()), Some(xls));
        assert!(!values(&d).iter().flatten().any(|v| *v == Some(99.0)));
    }
}
