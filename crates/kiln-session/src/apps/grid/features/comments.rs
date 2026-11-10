// SPDX-License-Identifier: AGPL-3.0-or-later
//! Notes (Shift+F2: new, edit, delete, show/hide, Show All Notes, Convert to Comments),
//! threaded comments (Ctrl+Alt+M: new, reply, edit, delete, resolve, previous and next) and
//! hyperlinks (Ctrl+K: web, file, e-mail or a place in the workbook; Remove Link). The XLSX
//! writer saves notes as `comments` parts and threads as `threadedComments` with their people.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{GridCommentInfo, GridLink, GridNoteCard};
use kiln_sheet::address::cell_a1;
use kiln_sheet::commands::{CellInput, Content, MetaField};
use kiln_sheet::meta::{Hyperlink, Note, Person, ThreadedComment};
use kiln_sheet::{CellPos, CellValue, Command, Range, Sheet, SheetId};

use super::super::GridDoc;
use super::cf::subtract;
use crate::model::Change;

/// The link of a cell for the chrome.
pub(crate) fn link_at(sheet: &Sheet, at: CellPos) -> Option<GridLink> {
    let h = sheet.hyperlink_at(at.row, at.col)?;
    Some(GridLink {
        target: h.target.clone().unwrap_or_default(),
        location: h.location.clone().unwrap_or_default(),
        display: h.display.clone().unwrap_or_default(),
        tooltip: h.tooltip.clone().unwrap_or_default(),
    })
}

/// A comment id in Excel's GUID form, from the clock and a counter.
pub(crate) fn guid(now_ms: u64, n: usize) -> String {
    let a = now_ms ^ 0x9E37_79B9_7F4A_7C15;
    let b = (n as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ now_ms.rotate_left(17);
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        a as u16,
        (b >> 48) as u16,
        b & 0xFFFF_FFFF_FFFF
    )
}

fn iso(now_ms: u64) -> String {
    super::super::w3cdtf(now_ms)
}

impl GridDoc {
    fn meta_field(&mut self, label: &'static str, sid: SheetId, field: MetaField) -> Result<()> {
        self.exec(
            label,
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(field),
            },
        )?;
        Ok(())
    }

    fn person_for(&mut self, author: &str) -> String {
        if let Some(p) = self
            .workbook
            .persons()
            .iter()
            .find(|p| p.display_name == author)
        {
            return p.id.clone();
        }
        let mut people = self.workbook.persons().to_vec();
        let id = guid(self.now_ms.wrapping_add(7), people.len() + 1_000);
        people.push(Person {
            id: id.clone(),
            display_name: author.to_owned(),
            user_id: Some(author.to_owned()),
            provider_id: Some("None".into()),
        });
        self.log.set_persons(&mut self.workbook, people);
        id
    }

    pub(crate) fn comment_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        let a = self.view().sel.active;
        let (notes, threads, links) = self
            .workbook
            .sheet(sid)
            .map(|s| {
                (
                    s.meta().notes.clone(),
                    s.meta().threads.clone(),
                    s.meta().hyperlinks.clone(),
                )
            })
            .unwrap_or_default();
        match cmd {
            GridCommand::SetNote { text, author, cell } => {
                let mut notes = notes;
                let a = match cell {
                    Some(c) => {
                        let r = Range::parse_a1(&c.replace('$', ""))
                            .filter(|r| r.r0 == r.r1 && r.c0 == r.c1)
                            .ok_or_else(|| KilnError::InvalidInput(format!("{c} is not a cell")))?;
                        CellPos::new(r.r0, r.c0)
                    }
                    None => a,
                };
                match text {
                    Some(t) => {
                        if let Some(n) = notes.iter_mut().find(|n| n.at == a) {
                            n.text.clone_from(t);
                        } else {
                            notes.push(Note {
                                at: a,
                                author: author.clone(),
                                text: t.clone(),
                                visible: false,
                                size: None,
                            });
                        }
                    }
                    None if cell.is_some() => notes.retain(|n| n.at != a),
                    None => {
                        let sel = self.view().sel.ranges;
                        notes.retain(|n| !sel.iter().any(|r| r.contains(n.at.row, n.at.col)));
                    }
                }
                self.meta_field("grid.review.set_note", sid, MetaField::Notes(notes))?;
            }
            GridCommand::ShowNote { show } => {
                let mut notes = notes;
                let n = notes
                    .iter_mut()
                    .find(|n| n.at == a)
                    .ok_or_else(|| KilnError::InvalidInput("the cell has no note".into()))?;
                n.visible = show.unwrap_or(!n.visible);
                self.meta_field("grid.review.show_note", sid, MetaField::Notes(notes))?;
            }
            GridCommand::ShowAllNotes { show } => {
                self.features.all_notes = *show;
                return Ok(Change::Selection);
            }
            GridCommand::AddComment {
                text,
                author,
                reply_to,
                now_ms,
            } => {
                if text.trim().is_empty() {
                    return Err(KilnError::InvalidInput("write the comment first".into()));
                }
                let author = if author.trim().is_empty() {
                    "Author".to_owned()
                } else {
                    author.trim().to_owned()
                };
                let person = self.person_for(&author);
                let mut threads = threads;
                let at = match reply_to {
                    Some(id) => {
                        threads
                            .iter()
                            .find(|t| &t.id == id)
                            .ok_or_else(|| KilnError::InvalidInput("the comment is gone".into()))?
                            .at
                    }
                    None => a,
                };
                if reply_to.is_none() && threads.iter().any(|t| t.at == at && t.parent_id.is_none())
                {
                    return Err(KilnError::InvalidInput(
                        "the cell has a comment already: reply to it".into(),
                    ));
                }
                let id = guid(*now_ms, threads.len());
                threads.push(ThreadedComment {
                    id,
                    at,
                    author,
                    author_id: Some(person),
                    date: iso(*now_ms),
                    text: text.clone(),
                    parent_id: reply_to.clone(),
                    done: false,
                    mentions: Vec::new(),
                });
                self.meta_field("grid.review.add_comment", sid, MetaField::Threads(threads))?;
            }
            GridCommand::EditComment { id, text } => {
                let mut threads = threads;
                let c = threads
                    .iter_mut()
                    .find(|t| &t.id == id)
                    .ok_or_else(|| KilnError::InvalidInput("the comment is gone".into()))?;
                c.text.clone_from(text);
                self.meta_field("grid.review.edit_comment", sid, MetaField::Threads(threads))?;
            }
            GridCommand::DeleteComment { id } => match id {
                Some(id) => {
                    let mut threads = threads;
                    let root = threads
                        .iter()
                        .find(|t| &t.id == id)
                        .map(|t| t.parent_id.is_none());
                    if root == Some(true) {
                        threads.retain(|t| &t.id != id && t.parent_id.as_ref() != Some(id));
                    } else {
                        threads.retain(|t| &t.id != id);
                    }
                    self.meta_field("grid.review.delete", sid, MetaField::Threads(threads))?;
                }
                None => {
                    let sel = self.view().sel.ranges;
                    let inside = |p: CellPos| sel.iter().any(|r| r.contains(p.row, p.col));
                    let threads: Vec<ThreadedComment> =
                        threads.into_iter().filter(|t| !inside(t.at)).collect();
                    let notes: Vec<Note> = notes.into_iter().filter(|n| !inside(n.at)).collect();
                    self.exec_all(
                        "grid.review.delete",
                        vec![
                            Command::SetSheetMeta {
                                sheet: sid,
                                field: Box::new(MetaField::Threads(threads)),
                            },
                            Command::SetSheetMeta {
                                sheet: sid,
                                field: Box::new(MetaField::Notes(notes)),
                            },
                        ],
                    )?;
                }
            },
            GridCommand::ResolveComment { id, done } => {
                let mut threads = threads;
                let mut found = false;
                for t in &mut threads {
                    if &t.id == id || t.parent_id.as_ref() == Some(id) {
                        t.done = *done;
                        found = true;
                    }
                }
                if !found {
                    return Err(KilnError::InvalidInput("the comment is gone".into()));
                }
                self.meta_field(
                    "grid.review.resolve_comment",
                    sid,
                    MetaField::Threads(threads),
                )?;
            }
            GridCommand::NextComment { backward } => {
                return self.next_comment(*backward);
            }
            GridCommand::ConvertNotes { author, now_ms } => {
                let mut threads = threads;
                let mut k = threads.len();
                for n in &notes {
                    if threads.iter().any(|t| t.at == n.at) {
                        continue;
                    }
                    let who = if n.author.is_empty() {
                        author.clone()
                    } else {
                        n.author.clone()
                    };
                    let person = self.person_for(&who);
                    threads.push(ThreadedComment {
                        id: guid(*now_ms, k),
                        at: n.at,
                        author: who,
                        author_id: Some(person),
                        date: iso(*now_ms),
                        text: n.text.clone(),
                        parent_id: None,
                        done: false,
                        mentions: Vec::new(),
                    });
                    k += 1;
                }
                self.exec_all(
                    "grid.review.convert_notes",
                    vec![
                        Command::SetSheetMeta {
                            sheet: sid,
                            field: Box::new(MetaField::Threads(threads)),
                        },
                        Command::SetSheetMeta {
                            sheet: sid,
                            field: Box::new(MetaField::Notes(Vec::new())),
                        },
                    ],
                )?;
            }
            GridCommand::SetLink { link } => {
                let sel = self.view().sel.ranges;
                let mut links: Vec<Hyperlink> = links
                    .into_iter()
                    .flat_map(|h| {
                        let mut parts = vec![h.range];
                        for s in &sel {
                            parts = parts.iter().flat_map(|r| subtract(*r, *s)).collect();
                        }
                        parts
                            .into_iter()
                            .map(|range| Hyperlink { range, ..h.clone() })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let mut cmds = Vec::new();
                if let Some(l) = link {
                    if l.target.trim().is_empty() && l.location.trim().is_empty() {
                        return Err(KilnError::InvalidInput(
                            "enter an address or choose a place in this document".into(),
                        ));
                    }
                    let target = l.target.trim();
                    let target = if !target.is_empty()
                        && !target.contains(':')
                        && !target.starts_with('/')
                        && !target.starts_with('.')
                        && target.contains('.')
                        && !target.contains(' ')
                    {
                        // `kiln.example` as Excel completes it.
                        if target.contains('@') {
                            format!("mailto:{target}")
                        } else {
                            format!("https://{target}")
                        }
                    } else {
                        target.to_owned()
                    };
                    for r in &sel {
                        links.push(Hyperlink {
                            range: *r,
                            target: (!target.is_empty()).then(|| target.clone()),
                            location: (!l.location.trim().is_empty())
                                .then(|| l.location.trim().to_owned()),
                            display: (!l.display.is_empty()).then(|| l.display.clone()),
                            tooltip: (!l.tooltip.is_empty()).then(|| l.tooltip.clone()),
                        });
                    }
                    // The active cell shows the text to display (the address when empty).
                    let shown = if l.display.is_empty() {
                        if target.is_empty() {
                            l.location.clone()
                        } else {
                            target.clone()
                        }
                    } else {
                        l.display.clone()
                    };
                    let current = self.workbook.display_text(sid, a.row, a.col);
                    if current.is_empty() || (!l.display.is_empty() && current != l.display) {
                        cmds.push(Command::SetCells {
                            sheet: sid,
                            cells: vec![CellInput {
                                row: a.row,
                                col: a.col,
                                content: Content::Value(CellValue::text(shown.as_str())),
                                style: None,
                            }],
                        });
                    }
                }
                cmds.push(Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::Hyperlinks(links)),
                });
                self.exec_all("grid.insert.set_link", cmds)?;
            }
            _ => return Ok(Change::None),
        }
        Ok(Change::Content)
    }

    /// Previous/Next for notes and comments, across sheets.
    fn next_comment(&mut self, backward: bool) -> Result<Change> {
        let n = self.workbook.sheets().len();
        let a = self.view().sel.active;
        let cells_of = |i: usize| -> Vec<(u32, u32)> {
            let Some(s) = self.workbook.sheets().get(i) else {
                return Vec::new();
            };
            let mut v: Vec<(u32, u32)> = s
                .meta()
                .notes
                .iter()
                .map(|x| (x.at.row, x.at.col))
                .chain(s.meta().threads.iter().map(|t| (t.at.row, t.at.col)))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        for k in 0..=n {
            let i = if backward {
                (self.active + n * 2 - k) % n
            } else {
                (self.active + k) % n
            };
            let cells = cells_of(i);
            let pick = if k == 0 {
                if backward {
                    cells.iter().rev().find(|&&p| p < (a.row, a.col)).copied()
                } else {
                    cells.iter().find(|&&p| p > (a.row, a.col)).copied()
                }
            } else if backward {
                cells.last().copied()
            } else {
                cells.first().copied()
            };
            if let Some((r, c)) = pick {
                if i != self.active {
                    self.activate(i);
                }
                self.select_cell(r, c, false, false);
                return Ok(Change::Selection);
            }
        }
        Err(KilnError::InvalidInput(
            "The workbook has no comments or notes.".into(),
        ))
    }

    /// Every note and comment, for the Comments pane.
    pub(crate) fn comments(&self) -> Vec<GridCommentInfo> {
        let mut out = Vec::new();
        for (i, s) in self.workbook.sheets().iter().enumerate() {
            let sheet = u32::try_from(i).unwrap_or(0);
            for t in &s.meta().threads {
                out.push(GridCommentInfo {
                    id: t.id.clone(),
                    sheet,
                    row: t.at.row,
                    col: t.at.col,
                    cell: cell_a1(t.at.row, t.at.col, false),
                    author: t.author.clone(),
                    date: t.date.clone(),
                    text: t.text.clone(),
                    parent: t.parent_id.clone(),
                    done: t.done,
                    note: false,
                    visible: false,
                });
            }
            for n in &s.meta().notes {
                out.push(GridCommentInfo {
                    id: String::new(),
                    sheet,
                    row: n.at.row,
                    col: n.at.col,
                    cell: cell_a1(n.at.row, n.at.col, false),
                    author: n.author.clone(),
                    date: String::new(),
                    text: n.text.clone(),
                    parent: None,
                    done: false,
                    note: true,
                    visible: n.visible,
                });
            }
        }
        out.sort_by_key(|c| (c.sheet, c.row, c.col, c.note, c.parent.is_some()));
        out
    }

    /// Notes kept visible (and every note with Show All Notes) in the view.
    pub(crate) fn note_cards(&self) -> Vec<GridNoteCard> {
        let Some((_, sheet)) = self.active_sheet() else {
            return Vec::new();
        };
        let all = self.features.all_notes;
        if !all && !sheet.meta().notes.iter().any(|n| n.visible) {
            return Vec::new();
        }
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let view = self.last_view;
        let mut out = Vec::new();
        let visible = |r: u32, c: u32| {
            let rect = geo.display_rect(r, c);
            let (x, y) = geo.to_surface(&view, rect.right(), rect.y, hw);
            // (Measured in the left-to-right layout: a right-to-left sheet is its mirror image.)
            let lx = geo.surface_flip_x(&view, x);
            (lx >= hw && y >= geo.header_h() && lx <= view.width + 200.0 && y <= view.height)
                .then_some((x, y))
        };
        for n in &sheet.meta().notes {
            if !(n.visible || all) {
                continue;
            }
            if let Some((x, y)) = visible(n.at.row, n.at.col) {
                out.push(GridNoteCard {
                    row: n.at.row,
                    col: n.at.col,
                    author: n.author.clone(),
                    text: n.text.clone(),
                    x,
                    y,
                    thread: false,
                    replies: 0,
                });
            }
            if out.len() >= 200 {
                break;
            }
        }
        if all {
            for t in sheet
                .meta()
                .threads
                .iter()
                .filter(|t| t.parent_id.is_none())
            {
                if let Some((x, y)) = visible(t.at.row, t.at.col) {
                    let replies = sheet
                        .meta()
                        .threads
                        .iter()
                        .filter(|r| r.parent_id.as_ref() == Some(&t.id))
                        .count();
                    out.push(GridNoteCard {
                        row: t.at.row,
                        col: t.at.col,
                        author: t.author.clone(),
                        text: t.text.clone(),
                        x,
                        y,
                        thread: true,
                        replies: u32::try_from(replies).unwrap_or(0),
                    });
                }
            }
        }
        let _ = Range::cell(0, 0);
        out
    }
}
