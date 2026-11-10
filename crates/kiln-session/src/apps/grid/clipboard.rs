// SPDX-License-Identifier: AGPL-3.0-or-later
//! The clipboard: Copy and Cut put the selection on the system clipboard as tab-separated text,
//! HTML (with formats, for other apps) and Kiln's own format (formulas, styles, merges and the
//! source, so a paste in Kiln adjusts references and Cut moves); Paste and Paste Special read
//! any of them back.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::{ClipboardData, GridCommand, GridPaste, GridPasteOp, PasteMode};
use kiln_sheet::clipboard::ClipData;
use kiln_sheet::commands::{PasteOp, PasteWhat};
use kiln_sheet::{CellPos, Range};
use serde::{Deserialize, Serialize};

use super::view::Sel;
use super::{CopyMark, GridDoc};
use crate::model::Change;

/// The prefix of Kiln Grid's clipboard flavour (postcard in base64).
const TAG: &str = "kiln-grid/1:";

/// Kiln Grid's clipboard flavour (`application/x-kiln-fragment`).
#[derive(Debug, Serialize, Deserialize)]
struct Fragment {
    /// Format marker and version.
    #[serde(rename = "kilnGrid")]
    version: u32,
    /// The cells.
    data: ClipData,
}

fn what_of(p: GridPaste) -> PasteWhat {
    match p {
        GridPaste::All => PasteWhat::All,
        GridPaste::Values => PasteWhat::Values,
        GridPaste::Formulas => PasteWhat::Formulas,
        GridPaste::Formats => PasteWhat::Formats,
        GridPaste::ValuesAndNumberFormats => PasteWhat::ValuesAndNumberFormats,
        GridPaste::AllExceptBorders => PasteWhat::AllExceptBorders,
        GridPaste::FormulasAndNumberFormats => PasteWhat::FormulasAndNumberFormats,
        GridPaste::Comments => PasteWhat::Comments,
        // Validation and column widths change no cell: the block is pasted for its notes
        // alone (none), and the extras do the work.
        GridPaste::Validation | GridPaste::ColumnWidths => PasteWhat::Comments,
    }
}

/// What a paste carries besides the cells (from a copy of this workbook only).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Extras {
    /// The cells themselves (contents, formats, notes as `what` says).
    pub content: bool,
    /// Data validation rules.
    pub validation: bool,
    /// Threaded comments.
    pub threads: bool,
    /// Column widths.
    pub widths: bool,
}

impl Extras {
    const ALL: Extras = Extras {
        content: true,
        validation: true,
        threads: true,
        widths: false,
    };
    const CELLS: Extras = Extras {
        content: true,
        validation: false,
        threads: false,
        widths: false,
    };

    fn of(p: GridPaste) -> Extras {
        match p {
            GridPaste::All | GridPaste::AllExceptBorders => Extras::ALL,
            GridPaste::Comments => Extras {
                threads: true,
                ..Extras::CELLS
            },
            GridPaste::Validation => Extras {
                content: false,
                validation: true,
                ..Extras::default()
            },
            GridPaste::ColumnWidths => Extras {
                content: false,
                widths: true,
                ..Extras::default()
            },
            _ => Extras::CELLS,
        }
    }
}

fn op_of(o: GridPasteOp) -> PasteOp {
    match o {
        GridPasteOp::None => PasteOp::None,
        GridPasteOp::Add => PasteOp::Add,
        GridPasteOp::Subtract => PasteOp::Subtract,
        GridPasteOp::Multiply => PasteOp::Multiply,
        GridPasteOp::Divide => PasteOp::Divide,
    }
}

/// The most cells a paste fills with copies of the clipboard's block (one whole column).
const MAX_TILED_PASTE: u64 = 1 << 20;

impl GridDoc {
    /// Copy (Ctrl+C) or Cut (Ctrl+X): the cells go to the clipboard and the range is marked;
    /// a Cut moves them on the next paste, as Excel's does.
    pub(crate) fn copy_selection(&mut self, cut: bool) -> Result<Change> {
        let sid = self.sid()?;
        let sel = self.view().sel.ranges;
        if sel.len() > 1 {
            return Err(KilnError::InvalidInput(
                "That command cannot be used on multiple selections.".into(),
            ));
        }
        let r = *sel
            .last()
            .ok_or_else(|| KilnError::InvalidInput("nothing is selected".into()))?;
        // Whole rows and columns copy their used part.
        let used = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.cells().used_range(false))
            .unwrap_or_else(|| Range::cell(0, 0));
        let r = Range::new(
            r.r0,
            r.c0,
            r.r1.min(used.r1.max(r.r0)),
            r.c1.min(used.c1.max(r.c0)),
        );
        if r.cell_count() > 4_000_000 {
            return Err(KilnError::InvalidInput(
                "the selection is too large to copy".into(),
            ));
        }
        // In a filtered list only the shown rows are copied (Excel), one under another.
        let runs = self.visible_ranges(&[r]);
        let filtered = runs.len() != 1 || runs.first() != Some(&r);
        if filtered && runs.is_empty() {
            return Err(KilnError::InvalidInput(
                "no shown cells are selected".into(),
            ));
        }
        let data = if filtered {
            let mut data = ClipData {
                rows: 0,
                cols: r.width(),
                cells: Vec::new(),
                merges: Vec::new(),
                source: Some(kiln_sheet::clipboard::ClipSource {
                    sheet: sid,
                    range: r,
                    cut,
                }),
            };
            for run in &runs {
                let part = self
                    .workbook
                    .copy(sid, *run, cut)
                    .map_err(super::model_err)?;
                let off = data.rows;
                data.cells.extend(part.cells.into_iter().map(|mut e| {
                    e.row += off;
                    e
                }));
                data.merges.extend(
                    part.merges
                        .iter()
                        .filter_map(|m| m.offset(i64::from(off), 0)),
                );
                data.rows += run.height();
            }
            data
        } else {
            self.workbook.copy(sid, r, cut).map_err(super::model_err)?
        };
        let html = data.to_html(self.workbook.theme());
        // Postcard in base64: the web core then needs no JSON decoder for the cell model.
        let kiln = postcard::to_allocvec(&Fragment {
            version: 1,
            data: data.clone(),
        })
        .ok()
        .map(|b| format!("{TAG}{}", crate::b64::encode(&b)));
        self.copied = Some(ClipboardData {
            text: data.to_tsv(),
            html: Some(html),
            kiln,
        });
        self.clip = Some(data);
        self.copy = Some(CopyMark {
            sheet: sid,
            range: r,
            cut,
            runs: filtered.then_some(runs),
        });
        Ok(Change::Selection)
    }

    /// The cells a clipboard holds.
    pub(crate) fn clip_of(&self, data: Option<&ClipboardData>, html: bool) -> Option<ClipData> {
        let date1904 = self.workbook.props().date1904;
        let Some(d) = data else {
            return self.clip.clone();
        };
        // Nothing on the system clipboard (the browser refused Kiln's copy, or holds only what
        // Kiln cannot paste): Kiln's own last copy, so Ctrl+C then Ctrl+V works within Grid.
        if d.text.is_empty() && d.html.as_deref().is_none_or(str::is_empty) && d.kiln.is_none() {
            return self.clip.clone();
        }
        if !html
            && let Some(f) = d
                .kiln
                .as_deref()
                .and_then(|k| k.strip_prefix(TAG))
                .and_then(crate::b64::decode)
                .and_then(|b| postcard::from_bytes::<Fragment>(&b).ok())
        {
            return Some(f.data);
        }
        // Text that is exactly Kiln's last copy pastes as that copy (formulas adjust).
        if !html
            && let Some(own) = &self.clip
            && own.to_tsv().replace("\r\n", "\n") == d.text.replace("\r\n", "\n")
        {
            return Some(own.clone());
        }
        if let Some(h) = d
            .html
            .as_deref()
            .filter(|h| h.contains("<table") || h.contains("<TABLE"))
        {
            return Some(ClipData::from_html(h, date1904));
        }
        if d.text.is_empty() {
            return None;
        }
        Some(ClipData::from_tsv(&d.text, date1904))
    }

    /// Pastes clipboard data at the selection.
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paste_data(
        &mut self,
        label: &'static str,
        clip: &ClipData,
        what: PasteWhat,
        op: PasteOp,
        transpose: bool,
        skip_blanks: bool,
        extras: Extras,
    ) -> Result<Change> {
        let sid = self.sid()?;
        let target = *self
            .view()
            .sel
            .ranges
            .last()
            .ok_or_else(|| KilnError::InvalidInput("nothing is selected".into()))?;
        if clip.rows == 0 || clip.cols == 0 {
            return Ok(Change::None);
        }
        let (h, w) = if transpose {
            (clip.cols, clip.rows)
        } else {
            (clip.rows, clip.cols)
        };
        // The shown rows of a filtered list, copied here: each block of rows pastes through
        // Copy (formulas adjust), one under another; a cut then clears what it took.
        let runs = match (clip.source, &self.copy) {
            (Some(src), Some(mark))
                if mark.sheet == src.sheet && mark.range == src.range && !transpose =>
            {
                mark.runs.clone()
            }
            _ => None,
        };
        let was_cut = clip.source.is_some_and(|s| s.cut);
        let mut cmds = Vec::new();
        let area = if let (Some(runs), Some(src)) = (runs, clip.source) {
            let mut off = 0u32;
            let mut clear = runs.clone();
            for run in &runs {
                let to = Range::new(
                    target.r0 + off,
                    target.c0,
                    (target.r0 + off + run.height() - 1).min(kiln_sheet::MAX_ROWS - 1),
                    (target.c0 + run.width() - 1).min(kiln_sheet::MAX_COLS - 1),
                );
                if src.sheet == sid {
                    clear = clear
                        .into_iter()
                        .flat_map(|r| super::features::cf::subtract(r, to))
                        .collect();
                }
                cmds.push(kiln_sheet::Command::CopyRange {
                    sheet: src.sheet,
                    range: *run,
                    to_sheet: Some(sid),
                    to,
                    what,
                    op,
                    transpose: false,
                    skip_blanks,
                });
                off += run.height();
            }
            if src.cut && !clear.is_empty() {
                cmds.push(kiln_sheet::Command::ClearRange {
                    sheet: src.sheet,
                    ranges: clear,
                    what: kiln_sheet::commands::ClearWhat::All,
                });
            }
            Range::new(
                target.r0,
                target.c0,
                target.r0 + off.max(1) - 1,
                target.c0 + w - 1,
            )
        } else {
            // Into a filtered list: one cell fills the shown cells of the selection; a block
            // goes once at its top left (Excel does not spread a block over hidden rows). A
            // selection that is a whole multiple of the copied block is filled with copies of it
            // (Excel); any other selection, or one whose copies would exceed a million cells
            // (Ctrl+A, Ctrl+V), takes the block once at its top left.
            let shown = self.visible_ranges(&[target]);
            let filtered = shown.len() != 1 || shown.first() != Some(&target);
            let tiles = target.height() % h == 0 && target.width() % w == 0;
            let targets = if filtered && h == 1 && w == 1 {
                shown
            } else if target.is_single_cell()
                || (!filtered && tiles && target.cell_count() <= MAX_TILED_PASTE)
            {
                vec![target]
            } else {
                vec![Range::cell(target.r0, target.c0)]
            };
            for t in &targets {
                cmds.push(
                    self.workbook
                        .paste_command(sid, *t, clip, what, op, transpose, skip_blanks)
                        .map_err(super::model_err)?,
                );
            }
            match targets.as_slice() {
                [t] if t.is_single_cell() => Range::new(
                    t.r0,
                    t.c0,
                    (t.r0 + h - 1).min(kiln_sheet::MAX_ROWS - 1),
                    (t.c0 + w - 1).min(kiln_sheet::MAX_COLS - 1),
                ),
                _ => target,
            }
        };
        if !extras.content {
            cmds.clear();
        }
        if let Some(src) = clip
            .source
            .filter(|s| self.workbook.sheet(s.sheet).is_some())
        {
            cmds.extend(self.paste_extras(src, sid, area, (h, w), transpose, extras)?);
        }
        self.exec_all(label, cmds)?;
        // The pasted area becomes the selection.
        let mut v = self.view();
        v.sel = Sel {
            ranges: vec![area],
            active: CellPos::new(area.r0, area.c0),
        };
        v.head = CellPos::new(area.r1, area.c1);
        self.set_view(v);
        if was_cut {
            // A cut pastes once.
            self.copy = None;
            self.clip = None;
        }
        Ok(Change::Content)
    }

    /// Paste (Ctrl+V) and its options.
    pub(crate) fn paste(
        &mut self,
        data: Option<&ClipboardData>,
        mode: PasteMode,
    ) -> Result<Change> {
        let clip = self
            .clip_of(data, mode == PasteMode::Html)
            .ok_or_else(|| KilnError::InvalidInput("the clipboard is empty".into()))?;
        let what = match mode {
            PasteMode::KeepSource | PasteMode::Html => PasteWhat::All,
            PasteMode::Merge => PasteWhat::Formulas,
            PasteMode::TextOnly => PasteWhat::Values,
        };
        let extras = if what == PasteWhat::All {
            Extras::ALL
        } else {
            Extras::CELLS
        };
        self.paste_data(
            "suite.edit.paste",
            &clip,
            what,
            PasteOp::None,
            false,
            false,
            extras,
        )
    }

    /// Paste Special and its presets.
    pub(crate) fn paste_special(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        let extras = match cmd {
            G::PasteSpecial { what, .. } => Extras::of(*what),
            G::PasteTranspose { .. } => Extras::ALL,
            _ => Extras::CELLS,
        };
        let (what, op, skip, transpose, data) = match cmd {
            G::PasteSpecial {
                what,
                op,
                skip_blanks,
                transpose,
                data,
            } => (
                what_of(*what),
                op_of(*op),
                *skip_blanks,
                *transpose,
                data.as_ref(),
            ),
            G::PasteValues { data } => (
                PasteWhat::Values,
                PasteOp::None,
                false,
                false,
                data.as_ref(),
            ),
            G::PasteFormulas { data } => (
                PasteWhat::Formulas,
                PasteOp::None,
                false,
                false,
                data.as_ref(),
            ),
            G::PasteFormatting { data } => (
                PasteWhat::Formats,
                PasteOp::None,
                false,
                false,
                data.as_ref(),
            ),
            G::PasteTranspose { data } => {
                (PasteWhat::All, PasteOp::None, false, true, data.as_ref())
            }
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} as a paste",
                    cmd.meta().id
                )));
            }
        };
        let mut clip = self
            .clip_of(data, false)
            .ok_or_else(|| KilnError::InvalidInput("the clipboard is empty".into()))?;
        // Paste Special never moves: a cut pastes as a copy of its values and formats.
        if let Some(src) = &mut clip.source
            && src.cut
            && what != PasteWhat::All
        {
            src.cut = false;
        }
        self.paste_data(cmd.meta().id, &clip, what, op, transpose, skip, extras)
    }
}
