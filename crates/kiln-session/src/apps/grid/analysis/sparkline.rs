// SPDX-License-Identifier: AGPL-3.0-or-later
//! Insert ▸ Sparklines and the Sparkline tab: groups created from a data range and a location
//! range (one sparkline per row or column of the data), their settings, Edit Data, Group,
//! Ungroup and Clear. Every change is one step of the sheet's sparkline list.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::{
    GridSparkAxis, GridSparkEdit, GridSparkEmpty, GridSparkInfo, GridSparkKind, GridSparkOptions,
};
use kiln_sheet::Command;
use kiln_sheet::color::Color;
use kiln_sheet::commands::MetaField;
use kiln_sheet::num::F64;
use kiln_sheet::pivot::cache::split_reference;
use kiln_sheet::sparkline::{EmptyCells, SparkAxis, Sparkline, SparklineGroup, SparklineKind};
use kiln_sheet::{CellPos, Range, SheetId};

use super::super::GridDoc;
use crate::model::Change;

fn bad(s: &str) -> KilnError {
    KilnError::InvalidInput(s.to_owned())
}

fn kind(k: GridSparkKind) -> SparklineKind {
    match k {
        GridSparkKind::Line => SparklineKind::Line,
        GridSparkKind::Column => SparklineKind::Column,
        GridSparkKind::WinLoss => SparklineKind::WinLoss,
    }
}

fn hex(c: &Color, theme: &kiln_sheet::Theme) -> String {
    format!("{:06X}", c.resolve(theme, 0))
}

/// The Sparkline tab's view of a group.
pub(crate) fn options_out(g: &SparklineGroup, theme: &kiln_sheet::Theme) -> GridSparkOptions {
    let c = &g.colors;
    let axis = |a: SparkAxis| match a {
        SparkAxis::Individual => GridSparkAxis::Individual,
        SparkAxis::Group => GridSparkAxis::Group,
        SparkAxis::Custom(v) => GridSparkAxis::Custom(v.0),
    };
    GridSparkOptions {
        kind: match g.kind {
            SparklineKind::Line => GridSparkKind::Line,
            SparklineKind::Column => GridSparkKind::Column,
            SparklineKind::WinLoss => GridSparkKind::WinLoss,
        },
        high: g.high,
        low: g.low,
        first: g.first,
        last: g.last,
        negative: g.negative,
        markers: g.markers,
        show_axis: g.show_axis,
        #[allow(clippy::cast_possible_truncation)]
        line_weight: g.line_weight.0 as f32,
        empty: match g.empty {
            EmptyCells::Gap => GridSparkEmpty::Gap,
            EmptyCells::Zero => GridSparkEmpty::Zero,
            EmptyCells::Span => GridSparkEmpty::Span,
        },
        show_hidden: g.show_hidden,
        min: axis(g.min),
        max: axis(g.max),
        right_to_left: g.right_to_left,
        colors: [
            &c.series,
            &c.negative,
            &c.markers,
            &c.high,
            &c.low,
            &c.first,
            &c.last,
            &c.axis,
        ]
        .into_iter()
        .map(|x| hex(x, theme))
        .collect(),
    }
}

fn apply_options(g: &mut SparklineGroup, o: &GridSparkOptions) -> Result<()> {
    g.kind = kind(o.kind);
    g.high = o.high;
    g.low = o.low;
    g.first = o.first;
    g.last = o.last;
    g.negative = o.negative;
    g.markers = o.markers;
    g.show_axis = o.show_axis;
    if !(0.25..=6.0).contains(&o.line_weight) {
        return Err(bad("The line weight is between 0.25 and 6 points."));
    }
    g.line_weight = F64(f64::from(o.line_weight));
    g.empty = match o.empty {
        GridSparkEmpty::Gap => EmptyCells::Gap,
        GridSparkEmpty::Zero => EmptyCells::Zero,
        GridSparkEmpty::Span => EmptyCells::Span,
    };
    g.show_hidden = o.show_hidden;
    let axis = |a: GridSparkAxis| match a {
        GridSparkAxis::Individual => SparkAxis::Individual,
        GridSparkAxis::Group => SparkAxis::Group,
        GridSparkAxis::Custom(v) => SparkAxis::Custom(F64(v)),
    };
    g.min = axis(o.min);
    g.max = axis(o.max);
    g.right_to_left = o.right_to_left;
    let c = &mut g.colors;
    let slots: [&mut Color; 8] = [
        &mut c.series,
        &mut c.negative,
        &mut c.markers,
        &mut c.high,
        &mut c.low,
        &mut c.first,
        &mut c.last,
        &mut c.axis,
    ];
    for (slot, text) in slots.into_iter().zip(&o.colors) {
        if text.is_empty() {
            continue;
        }
        *slot = Color::hex(text).map_err(|_| bad("A colour is not valid."))?;
    }
    Ok(())
}

/// A Sparkline Style preset: the accent of its column, the shade of its row (darker 50 %,
/// darker 25 %, the accent, lighter 40 %); markers and points in a contrasting shade.
fn apply_style(g: &mut SparklineGroup, index: u32) {
    let accent = 4 + u8::try_from(index % 6).unwrap_or(0);
    let tint = match index / 6 {
        0 => -0.499_984_740_745_262,
        1 => -0.249_977_111_117_893,
        2 => 0.0,
        _ => 0.399_975_585_192_419_2,
    };
    let series = Color::theme(accent, tint);
    let point = Color::theme(
        accent,
        if index / 6 >= 2 {
            -0.499_984_740_745_262
        } else {
            0.399_975_585_192_419_2
        },
    );
    let c = &mut g.colors;
    c.series = series;
    c.markers = series;
    c.negative = Color::theme(5, 0.0);
    c.first = point;
    c.last = point;
    c.high = point;
    c.low = point;
}

/// One sparkline per row (or column) of the data, placed along the location.
fn layout(data: Range, data_sheet: Option<String>, location: Range) -> Result<Vec<Sparkline>> {
    let n = if location.height() == 1 {
        location.width()
    } else if location.width() == 1 {
        location.height()
    } else {
        return Err(bad(
            "Location reference is not valid because the cells are not all in the same column or \
             same row. Select cells that are all in a single row or column.",
        ));
    };
    let cell = |i: u32| {
        if location.height() == 1 {
            CellPos::new(location.r0, location.c0 + i)
        } else {
            CellPos::new(location.r0 + i, location.c0)
        }
    };
    let (by_rows, count) = if n == 1 {
        // One sparkline over the whole range (a row or a column of data).
        if data.height() != 1 && data.width() != 1 {
            return Err(bad("Data Range dimensions are not valid."));
        }
        (true, 1)
    } else if data.height() == n {
        (true, n)
    } else if data.width() == n {
        (false, n)
    } else {
        return Err(bad("Data Range dimensions are not valid."));
    };
    Ok((0..count)
        .map(|i| Sparkline {
            at: cell(i),
            sheet: data_sheet.clone(),
            data: if n == 1 {
                data
            } else if by_rows {
                Range::new(data.r0 + i, data.c0, data.r0 + i, data.c1)
            } else {
                Range::new(data.r0, data.c0 + i, data.r1, data.c0 + i)
            },
        })
        .collect())
}

impl GridDoc {
    fn parse_area(&self, text: &str) -> Result<(Option<String>, Range)> {
        let (sheet, r) = split_reference(text).ok_or_else(|| bad("Reference isn't valid."))?;
        let me = self.active_sheet().map(|(_, s)| s.name().to_owned());
        let sheet = match sheet {
            Some(n) => {
                let s = self
                    .workbook
                    .sheet_by_name(&n)
                    .ok_or_else(|| bad("Reference isn't valid."))?;
                (Some(s.name()) != me.as_deref()).then(|| s.name().to_owned())
            }
            None => None,
        };
        Ok((sheet, r))
    }

    fn set_sparklines(&mut self, sid: SheetId, list: Vec<SparklineGroup>) -> Result<Change> {
        self.exec(
            "grid.sparkline.edit",
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Sparklines(list)),
            },
        )?;
        Ok(Change::Content)
    }

    fn sparkline_list(&self, sid: SheetId) -> Vec<SparklineGroup> {
        self.workbook
            .sheet(sid)
            .map(|s| s.meta().sparklines.clone())
            .unwrap_or_default()
    }

    /// Insert ▸ Sparklines.
    pub(crate) fn insert_sparklines(
        &mut self,
        k: GridSparkKind,
        data: &str,
        location: &str,
    ) -> Result<Change> {
        let sid = self.sid()?;
        let (dsheet, d) = self.parse_area(data)?;
        let (lsheet, l) = self.parse_area(location)?;
        if lsheet.is_some() {
            return Err(bad("The location must be on this sheet."));
        }
        let lines = layout(d, dsheet, l)?;
        let mut list = self.sparkline_list(sid);
        // A cell holds one sparkline: the new ones replace those already there.
        for g in &mut list {
            g.sparklines.retain(|s| !l.contains(s.at.row, s.at.col));
        }
        list.retain(|g| !g.sparklines.is_empty());
        list.push(SparklineGroup::new(kind(k), lines));
        self.exec(
            "grid.sparkline.insert",
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Sparklines(list)),
            },
        )?;
        Ok(Change::Content)
    }

    /// The Sparkline tab.
    pub(crate) fn sparkline_edit(&mut self, edit: &GridSparkEdit) -> Result<Change> {
        let sid = self.sid()?;
        let a = self.view().sel.active;
        let sel: Vec<Range> = self.view().sel.ranges.clone();
        let mut list = self.sparkline_list(sid);
        let gi = list
            .iter()
            .position(|g| g.at(a.row, a.col).is_some())
            .ok_or_else(|| bad("Select a cell with a sparkline first."))?;
        let in_sel = |p: CellPos| sel.iter().any(|r| r.contains(p.row, p.col));
        match edit {
            GridSparkEdit::Options { options } => apply_options(&mut list[gi], options)?,
            GridSparkEdit::Style { index } => {
                if *index >= 24 {
                    return Err(bad("There is no such sparkline style."));
                }
                apply_style(&mut list[gi], *index);
            }
            GridSparkEdit::Data { data, location } => {
                let (dsheet, d) = self.parse_area(data)?;
                let (_, l) = self.parse_area(location)?;
                let lines = layout(d, dsheet, l)?;
                let g = &mut list[gi];
                let old: Vec<CellPos> = g.sparklines.iter().map(|s| s.at).collect();
                g.sparklines = lines;
                let kept: Vec<CellPos> = list[gi].sparklines.iter().map(|s| s.at).collect();
                for (j, other) in list.iter_mut().enumerate() {
                    if j != gi {
                        other.sparklines.retain(|s| !kept.contains(&s.at));
                    }
                }
                let _ = old;
                list.retain(|g| !g.sparklines.is_empty());
            }
            GridSparkEdit::Clear { groups } => {
                if *groups {
                    let hit: Vec<usize> = list
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| g.sparklines.iter().any(|s| in_sel(s.at)))
                        .map(|(i, _)| i)
                        .collect();
                    let mut i = 0;
                    list.retain(|_| {
                        let keep = !hit.contains(&i);
                        i += 1;
                        keep
                    });
                } else {
                    for g in &mut list {
                        g.sparklines.retain(|s| !in_sel(s.at));
                    }
                    list.retain(|g| !g.sparklines.is_empty());
                }
            }
            GridSparkEdit::Group => {
                // The selected sparklines join one group with the active cell's settings.
                let template = list[gi].clone();
                let mut members: Vec<Sparkline> = Vec::new();
                for g in &mut list {
                    let (take, keep): (Vec<Sparkline>, Vec<Sparkline>) =
                        g.sparklines.drain(..).partition(|s| in_sel(s.at));
                    members.extend(take);
                    g.sparklines = keep;
                }
                list.retain(|g| !g.sparklines.is_empty());
                list.push(SparklineGroup {
                    sparklines: members,
                    ..template
                });
            }
            GridSparkEdit::Ungroup => {
                let mut out = Vec::new();
                for g in list {
                    if g.sparklines.iter().any(|s| in_sel(s.at)) && g.sparklines.len() > 1 {
                        for s in &g.sparklines {
                            out.push(SparklineGroup {
                                sparklines: vec![s.clone()],
                                ..g.clone()
                            });
                        }
                    } else {
                        out.push(g);
                    }
                }
                list = out;
            }
        }
        self.set_sparklines(sid, list)
    }

    /// The sparkline group at the active cell.
    pub(crate) fn sparkline_info(&self) -> Option<GridSparkInfo> {
        let sid = self.sid().ok()?;
        let a = self.view().sel.active;
        let sheet = self.workbook.sheet(sid)?;
        let (g, _) = sheet.sparkline_at(a.row, a.col)?;
        let span = |f: &dyn Fn(&Sparkline) -> Range| {
            g.sparklines
                .iter()
                .map(f)
                .reduce(|x, y| x.union(&y))
                .map(|r| r.to_a1(false))
                .unwrap_or_default()
        };
        let data_sheet = g.sparklines.first().and_then(|s| s.sheet.clone());
        let data = span(&|s: &Sparkline| s.data);
        Some(GridSparkInfo {
            options: options_out(g, self.workbook.theme()),
            data: match data_sheet {
                Some(n) => format!("{}!{data}", super::quote_sheet(&n)),
                None => data,
            },
            location: span(&|s: &Sparkline| Range::cell(s.at.row, s.at.col)),
            count: u32::try_from(g.sparklines.len()).unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sparkline_per_row_or_column() {
        let rows = layout(Range::new(1, 0, 4, 5), None, Range::new(1, 6, 4, 6)).expect("rows");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[2].data, Range::new(3, 0, 3, 5));
        assert_eq!(rows[2].at, CellPos::new(3, 6));
        let cols = layout(Range::new(0, 1, 9, 3), None, Range::new(10, 1, 10, 3)).expect("cols");
        assert_eq!(cols[1].data, Range::new(0, 2, 9, 2));
        assert!(layout(Range::new(0, 0, 3, 3), None, Range::new(0, 5, 1, 6)).is_err());
        assert!(layout(Range::new(0, 0, 3, 3), None, Range::new(0, 5, 2, 5)).is_err());
        let one = layout(Range::new(0, 0, 0, 11), None, Range::cell(0, 12)).expect("one");
        assert_eq!(one.len(), 1);
    }
}
