// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Write editor (WP-64): every ribbon, dialog and shortcut command of Kiln Write, applied to
//! the `kiln-doc` model as transactions through the undo history, and the queries the editor's
//! panes, rulers and dialogs ask.
//!
//! - [`format`] — character formatting (Word's toggle rules, pending formatting at the caret,
//!   the whole word under a collapsed caret), fonts, sizes, colours, case;
//! - [`paragraph`] — alignment, indents, spacing, tabs, borders, shading, moving paragraphs;
//! - [`styles`] — applying, updating, creating and modifying styles;
//! - [`lists`] — bullets, numbering, multilevel lists, levels, restart and continue;
//! - [`tables`] — inserting and editing tables, cell navigation, table styles;
//! - [`page`] — breaks, sections, page setup, headers and footers;
//! - [`insert`] — links, bookmarks, fields, dates, pictures, drop caps;
//! - [`clipboard`] and [`html`] — copy as Kiln's rich format, HTML and text; paste with Word's
//!   options; sanitised HTML mapped to paragraphs, lists and tables;
//! - [`find`] — Find, Replace and Go To with match case, whole words, Word's wildcards and
//!   regular expressions;
//! - [`marks`] — the formatting marks of Show/Hide ¶;
//! - [`query`] — `Query::Write`.
//!
//! Every command is logged ([`EditorState::log`]) so the end-to-end tests can prove that a
//! shortcut or ribbon control reached its handler (`WriteQuery::CommandLog`).

mod clipboard;
mod find;
mod format;
mod html;
mod insert;
mod lists;
pub(super) mod marks;
mod page;
mod paragraph;
mod query;
mod stats;
mod styles;
mod tables;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Op, Pos, Range, RunProps, Transaction, edit};
use kiln_protocol::commands::{ClipboardData, CommandLogEntry, EditCommand};
use kiln_protocol::{Command, SuiteCommand, WriteCommand};

use super::WriteDoc;
use crate::model::Change;

/// How many commands the log keeps.
const LOG_LIMIT: usize = 400;

/// Header or footer editing (Insert ▸ Header, double-click in the header area).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeaderFooterEdit {
    /// The section whose header or footer is edited.
    pub section: usize,
    /// Header or footer.
    pub part: kiln_doc::HeaderFooterPart,
    /// First page, even pages or the default.
    pub kind: kiln_doc::st::HeaderFooterKind,
    /// Where the caret was in the body.
    pub body: Pos,
}

/// The editor's state besides the document and the selection.
#[derive(Debug, Default)]
pub(super) struct EditorState {
    /// Show/Hide ¶.
    pub show_marks: bool,
    /// Formatting for the next typed text at a collapsed caret (Ctrl+B with nothing selected).
    pub pending: Option<(Pos, RunProps)>,
    /// The commands executed, newest last.
    pub log: VecDeque<CommandLogEntry>,
    /// The last repeatable command (F4).
    pub repeat: Option<Command>,
    /// What Copy or Cut produced, until the session takes it.
    pub copied: Option<ClipboardData>,
    /// Header or footer editing.
    pub header_footer: Option<HeaderFooterEdit>,
    /// The host's clock (Unix milliseconds) for dates, times and fields.
    pub now_ms: Option<u64>,
    /// Word and character counts per paragraph (the status bar).
    pub stats: std::cell::RefCell<stats::StatsCache>,
}

impl EditorState {
    fn record(&mut self, id: &str, outcome: &str) {
        if self.log.len() >= LOG_LIMIT {
            self.log.pop_front();
        }
        self.log.push_back(CommandLogEntry {
            id: id.to_owned(),
            outcome: outcome.to_owned(),
        });
    }
}

/// Edits applied one after the other and recorded as one undo step (paste, replace all, moving
/// paragraphs): each part sees the document as the previous parts left it.
pub(super) struct Batch {
    forward: Vec<Op>,
    inverses: Vec<Transaction>,
    kind: kiln_doc::EditKind,
    before: Range,
}

/// Maps a model error to the session's error type.
pub(super) fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// Whether a command repeats with F4 (an edit that does not depend on the selection's content).
fn repeatable(cmd: &Command) -> bool {
    match cmd {
        Command::Suite(SuiteCommand::Edit(e)) => !matches!(
            e,
            EditCommand::Cut
                | EditCommand::Copy
                | EditCommand::Repeat
                | EditCommand::InsertPictures
                | EditCommand::Paste { .. }
                | EditCommand::PasteKeepSource { .. }
                | EditCommand::PasteMerge { .. }
                | EditCommand::PasteTextOnly { .. }
                | EditCommand::PasteAs { .. }
        ),
        Command::Write(w) => !matches!(
            w,
            WriteCommand::MoveCaret { .. }
                | WriteCommand::SelectTo { .. }
                | WriteCommand::SelectAll
                | WriteCommand::SelectAt { .. }
                | WriteCommand::SetComposition { .. }
                | WriteCommand::CancelComposition
                | WriteCommand::ShowMarks
                | WriteCommand::EditHeader
                | WriteCommand::EditFooter
                | WriteCommand::CloseHeaderFooter
                | WriteCommand::FindNext { .. }
                | WriteCommand::GoTo { .. }
                | WriteCommand::TableSelect { .. }
                | WriteCommand::DeleteBackward
                | WriteCommand::DeleteForward
                | WriteCommand::DeleteWordBackward
                | WriteCommand::DeleteWordForward
        ),
        _ => false,
    }
}

impl WriteDoc {
    /// Runs a command with logging, F4 bookkeeping and an up-to-date layout afterwards.
    pub(super) fn execute_logged(&mut self, command: &Command) -> Result<Change> {
        let result = match command {
            Command::Write(cmd) => self.run_write(cmd),
            Command::Suite(SuiteCommand::Edit(EditCommand::Repeat)) => self.repeat(),
            Command::Suite(SuiteCommand::Edit(cmd)) => self.run_edit(cmd),
            _ => Err(KilnError::Unsupported(format!(
                "{} in Kiln Write",
                command.id()
            ))),
        };
        let outcome = match &result {
            Ok(Change::Content) => "applied",
            Ok(Change::Selection) => "selection",
            Ok(Change::None) => "unchanged",
            Err(_) => "error",
        };
        self.editor.record(command.id(), outcome);
        if matches!(result, Ok(Change::Content)) {
            if repeatable(command) {
                self.editor.repeat = Some(command.clone());
            }
            // Lay out what the edit changed near the viewport now, so the selection and the
            // rulers the session reports with this change match the new text.
            if let Some(v) = &mut self.view {
                v.sync(&self.doc, false);
            }
        }
        if !matches!(
            command,
            Command::Write(
                WriteCommand::InsertText { .. }
                    | WriteCommand::CommitComposition { .. }
                    | WriteCommand::SetComposition { .. }
            )
        ) && self
            .editor
            .pending
            .as_ref()
            .is_some_and(|(at, _)| *at != self.head || self.anchor != self.head)
        {
            self.editor.pending = None;
        }
        result
    }

    /// F4: runs the last repeatable command again at the current selection.
    fn repeat(&mut self) -> Result<Change> {
        let Some(cmd) = self.editor.repeat.clone() else {
            return Ok(Change::None);
        };
        match &cmd {
            Command::Write(c) => self.run_write(c),
            Command::Suite(SuiteCommand::Edit(e)) => self.run_edit(e),
            _ => Ok(Change::None),
        }
    }

    /// A Write command: typing with pending formatting and Enter's Word rules here, the
    /// original text editing in `editing.rs`, the editor's commands in [`Self::run_editor`].
    fn run_write(&mut self, cmd: &WriteCommand) -> Result<Change> {
        match cmd {
            WriteCommand::InsertText { text } if text == "\n" || text == "\r\n" || text == "\r" => {
                self.end_composition();
                self.measure(|d| d.enter())
            }
            WriteCommand::InsertText { text } | WriteCommand::CommitComposition { text }
                if self.pending_applies() && !text.is_empty() =>
            {
                self.end_composition();
                self.measure(|d| d.insert_pending(text))
            }
            WriteCommand::SelectAt { .. } if self.editor.header_footer.is_some() => {
                self.measure(|d| d.select_in_header_footer(cmd))
            }
            WriteCommand::SelectAt {
                unit: kiln_protocol::SelectUnit::Word,
                point,
                extend: false,
            } if self.editor.header_footer.is_none() => {
                // Double-click in a header or footer area opens it for editing (Word).
                if self.measure(|d| d.open_header_footer_at(point.x, point.y))? == Change::None {
                    self.run(cmd)
                } else {
                    Ok(Change::Selection)
                }
            }
            _ => self.run(cmd),
        }
    }

    /// Runs `f` and reports what it changed.
    pub(super) fn measure(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) -> Result<Change> {
        let state = self.state;
        let sel = (self.anchor, self.head);
        let marks = self.editor.show_marks;
        let hf = self.editor.header_footer;
        let pending = self.editor.pending.clone();
        f(self)?;
        Ok(if self.state != state {
            Change::Content
        } else if (self.anchor, self.head) != sel
            || self.editor.show_marks != marks
            || self.editor.header_footer != hf
            || self.editor.pending != pending
        {
            Change::Selection
        } else {
            Change::None
        })
    }

    /// The editor's Write commands (everything `editing.rs` does not handle).
    pub(super) fn run_editor(&mut self, cmd: &WriteCommand) -> Result<Change> {
        self.end_composition();
        self.measure(|d| d.dispatch_write(cmd))
    }

    fn dispatch_write(&mut self, cmd: &WriteCommand) -> Result<()> {
        use WriteCommand as W;
        match cmd {
            W::Tab { back } => self.tab(*back),
            W::LineBreak => self.insert_break(kiln_doc::st::BreakKind::TextWrapping),
            W::ShowMarks => {
                self.editor.show_marks = !self.editor.show_marks;
                Ok(())
            }
            W::ClearParagraphFormatting => self.clear_paragraph_formatting(),
            W::ClearCharacterFormatting => self.clear_character_formatting(),
            W::MoveParagraphUp => self.move_paragraphs(false),
            W::MoveParagraphDown => self.move_paragraphs(true),
            W::Promote => self.promote(true),
            W::Demote => self.promote(false),
            W::ParagraphShading { color } => self.paragraph_shading(color.as_deref()),
            W::ParagraphBorders { preset } => self.paragraph_borders(*preset),
            W::SetParagraph { format } => self.set_paragraph(format),
            W::SetFont { format } => self.set_font(format),
            W::SetTabs { tabs } => self.set_tabs(tabs),
            W::SmallCaps => self.toggle_prop(kiln_doc::ToggleProp::SmallCaps),
            W::UnderlineStyle { style } => self.underline_style(*style),
            W::ApplyStyle { style_id } => self.apply_style(style_id),
            W::StyleNormal => self.apply_style("Normal"),
            W::StyleHeading1 => self.apply_style("Heading1"),
            W::StyleHeading2 => self.apply_style("Heading2"),
            W::StyleHeading3 => self.apply_style("Heading3"),
            W::StyleTitle => self.apply_style("Title"),
            W::UpdateStyleFromSelection { style_id } => {
                self.update_style_from_selection(style_id.as_deref())
            }
            W::NewStyleFromSelection { name } => self.new_style_from_selection(name),
            W::ModifyStyle { style_id, spec } => self.modify_style(style_id, spec),
            W::ListLevel { level } => self.set_list_level(*level),
            W::RestartNumbering => self.restart_numbering(None),
            W::ContinueNumbering => self.continue_numbering(),
            W::SetNumberingValue { value } => self.restart_numbering(Some(*value)),
            W::PageBreak => self.insert_break(kiln_doc::st::BreakKind::Page),
            W::ColumnBreak => self.insert_break(kiln_doc::st::BreakKind::Column),
            W::BlankPage => self.blank_page(),
            W::SectionBreak { kind } => self.section_break(*kind),
            W::Columns { count } => self.set_columns(*count),
            W::IndentLeft { cm } => self.indent_side(true, *cm),
            W::IndentRight { cm } => self.indent_side(false, *cm),
            W::SpacingBefore { pt } => self.spacing_side(true, *pt),
            W::SpacingAfter { pt } => self.spacing_side(false, *pt),
            W::PageSetup { setup } => self.page_setup(setup),
            W::EditHeader => self.edit_header_footer(kiln_doc::HeaderFooterPart::Header),
            W::EditFooter => self.edit_header_footer(kiln_doc::HeaderFooterPart::Footer),
            W::CloseHeaderFooter => {
                self.close_header_footer();
                Ok(())
            }
            W::DifferentFirstPage => self.different_first_page(),
            W::DifferentOddEven => self.different_odd_even(),
            W::LinkToPrevious => self.link_to_previous(),
            W::PageNumber { position } => self.page_number(*position),
            W::InsertLink { url, text, tooltip } => {
                self.insert_link(url, text.as_deref(), tooltip.as_deref())
            }
            W::RemoveLink => self.remove_link(),
            W::InsertBookmark { name } => self.insert_bookmark(name),
            W::InsertDateTime { format, as_field } => self.insert_date_time(format, *as_field),
            W::InsertField { code } => self.insert_field(code),
            W::UpdateFields => self.update_fields(),
            W::DropCap { placement } => self.drop_cap(*placement),
            W::InsertPicture {
                name,
                bytes,
                floating,
            } => self.insert_picture(name, bytes, *floating),
            W::InsertTable {
                rows,
                cols,
                autofit,
            } => self.insert_table(*rows, *cols, *autofit),
            W::ConvertTextToTable { separator } => self.convert_text_to_table(*separator),
            W::TableAutofit { mode } => self.table_autofit(*mode),
            W::TableAlign { align } => self.table_align(*align),
            W::TableSplit => self.split_table(),
            W::TableSelect { what } => self.table_select(*what),
            W::TableCellSize { width, height } => self.table_cell_size(*width, *height),
            W::FindNext { query, backward } => self.find_next(query, *backward),
            W::Replace { query, replacement } => self.replace_one(query, replacement),
            W::ReplaceAll { query, replacement } => self.replace_all(query, replacement),
            W::GoTo { target } => self.go_to(target),
            // Handled by `editing.rs`.
            W::InsertText { .. }
            | W::DeleteBackward
            | W::MoveCaret { .. }
            | W::SelectTo { .. }
            | W::DeleteForward
            | W::DeleteWordBackward
            | W::DeleteWordForward
            | W::SelectAll
            | W::SelectAt { .. }
            | W::SetComposition { .. }
            | W::CommitComposition { .. }
            | W::CancelComposition => Err(KilnError::Internal(format!(
                "{} reached the editor dispatcher",
                cmd.meta().id
            ))),
        }
    }

    /// A shared editing command (`SuiteCommand::Edit`).
    pub(super) fn run_edit(&mut self, cmd: &EditCommand) -> Result<Change> {
        self.end_composition();
        self.measure(|d| d.dispatch_edit(cmd))
    }

    fn dispatch_edit(&mut self, cmd: &EditCommand) -> Result<()> {
        use EditCommand as E;
        use kiln_doc::ToggleProp as T;
        match cmd {
            E::Cut => self.cut(),
            E::Copy => {
                self.copy();
                Ok(())
            }
            E::Paste { data } | E::PasteKeepSource { data } => {
                self.paste(data.as_ref(), kiln_protocol::commands::PasteMode::KeepSource)
            }
            E::PasteMerge { data } => {
                self.paste(data.as_ref(), kiln_protocol::commands::PasteMode::Merge)
            }
            E::PasteTextOnly { data } => {
                self.paste(data.as_ref(), kiln_protocol::commands::PasteMode::TextOnly)
            }
            E::PasteAs { mode, data } => self.paste(data.as_ref(), *mode),
            // Handled before dispatch (it re-runs another command).
            E::Repeat => Ok(()),
            E::Bold => self.toggle_prop(T::Bold),
            E::Italic => self.toggle_prop(T::Italic),
            E::Strikethrough => self.toggle_prop(T::Strike),
            E::AllCaps => self.toggle_prop(T::Caps),
            E::Underline => self.toggle_underline(kiln_doc::st::UnderlineStyle::Single),
            E::DoubleUnderline => self.toggle_underline(kiln_doc::st::UnderlineStyle::Double),
            E::Subscript => self.toggle_vert_align(kiln_doc::st::VertAlign::Subscript),
            E::Superscript => self.toggle_vert_align(kiln_doc::st::VertAlign::Superscript),
            E::FontFamily { family } => self.font_family(family),
            E::FontSize { size } => self.font_size(*size),
            E::GrowFont { one_point } => self.grow_font(true, *one_point),
            E::ShrinkFont { one_point } => self.grow_font(false, *one_point),
            E::ChangeCase { case } => self.change_case(*case),
            E::ClearFormatting => self.clear_all_formatting(),
            E::FontColor { color } => self.font_color(color.as_deref()),
            E::Highlight { color } => self.highlight(color.as_deref()),
            E::Bullets { glyph } => self.bullets(glyph.as_deref()),
            E::Numbering { style } => self.numbering(*style),
            E::MultilevelList { legal } => self.multilevel(*legal),
            E::Indent => self.indent(true),
            E::Outdent => self.indent(false),
            E::AlignLeft => self.align(kiln_doc::st::Alignment::Left),
            E::AlignCenter => self.align(kiln_doc::st::Alignment::Center),
            E::AlignRight => self.align(kiln_doc::st::Alignment::Right),
            E::Justify => self.align(kiln_doc::st::Alignment::Both),
            E::LineSpacing { spacing } => self.line_spacing(*spacing),
            E::InsertDate => self.insert_now(false),
            E::InsertTime => self.insert_now(true),
            E::InsertPictures => Err(KilnError::InvalidInput(
                "Insert ▸ Pictures asks the host for a file first; send write.insert.picture with its bytes".into(),
            )),
            E::Margins { preset } => self.margins(*preset),
            E::Orientation { orientation } => self.orientation(*orientation),
            E::PageSize { size } => self.paper(*size),
            E::TableInsertRowAbove => self.insert_rows(false),
            E::TableInsertRowBelow => self.insert_rows(true),
            E::TableInsertColumnLeft => self.insert_columns(false),
            E::TableInsertColumnRight => self.insert_columns(true),
            E::TableDelete { what } => self.table_delete(*what),
            E::TableMergeCells => self.merge_cells(),
            E::TableSplitCells { rows, cols } => self.split_cells(*rows, *cols),
            E::TableDistributeRows => self.distribute(true),
            E::TableDistributeColumns => self.distribute(false),
            E::TableCellAlignment {
                vertical,
                horizontal,
            } => self.cell_alignment(*vertical, *horizontal),
            E::TableRepeatHeaderRows => self.repeat_header_rows(),
            E::TableConvertToText { separator } => self.table_to_text(*separator),
            E::TableHeaderRow => self.table_look(|l| l.first_row = !l.first_row),
            E::TableTotalRow => self.table_look(|l| l.last_row = !l.last_row),
            E::TableBandedRows => self.table_look(|l| l.no_h_band = !l.no_h_band),
            E::TableFirstColumn => self.table_look(|l| l.first_column = !l.first_column),
            E::TableBandedColumns => self.table_look(|l| l.no_v_band = !l.no_v_band),
            E::TableStyle { style } => self.table_style(style),
            E::TableShading { color } => self.cell_shading(color.as_deref()),
            E::TableBorders { preset } => self.cell_borders(*preset),
        }
    }

    // ----- selection helpers -------------------------------------------------------------

    /// The selection in document order.
    pub(super) fn span(&self) -> (Pos, Pos) {
        Range::new(self.anchor, self.head).ordered(&self.doc)
    }

    /// Sets the selection (and ends the typing group).
    pub(super) fn set_selection(&mut self, anchor: Pos, head: Pos) {
        self.anchor = anchor;
        self.head = head;
        self.goal_x = None;
        self.history.break_group();
    }

    /// The paragraphs the selection touches, in order.
    pub(super) fn selected_paragraphs(&self) -> Result<Vec<NodeId>> {
        let (s, e) = self.span();
        Ok(edit::segments(&self.doc, s, e)
            .map_err(doc_err)?
            .into_iter()
            .map(|(id, _, _)| id)
            .collect())
    }

    // ----- applying edits ------------------------------------------------------------------

    /// Applies one transaction as an undo step (with its kind and selections).
    pub(super) fn commit(&mut self, tx: Transaction) -> Result<()> {
        self.apply(tx)
    }

    /// Starts a batch of edits that becomes one undo step.
    pub(super) fn batch(&self, kind: kiln_doc::EditKind) -> Batch {
        Batch {
            forward: Vec::new(),
            inverses: Vec::new(),
            kind,
            before: Range::new(self.anchor, self.head),
        }
    }

    /// Applies one part of a batch now.
    pub(super) fn batch_apply(&mut self, b: &mut Batch, tx: Transaction) -> Result<()> {
        if tx.is_empty() {
            return Ok(());
        }
        let inverse = self.doc.apply(tx.clone()).map_err(doc_err)?;
        self.touched.record(&self.doc, &tx);
        self.note_edit(&tx);
        b.forward.extend(tx.ops);
        b.inverses.push(inverse);
        Ok(())
    }

    /// Records a batch as one undo step and puts the selection at `after`.
    pub(super) fn batch_commit(&mut self, b: Batch, after: Range) {
        if b.forward.is_empty() {
            self.set_selection(after.anchor, after.head);
            return;
        }
        let mut inverse_ops = Vec::new();
        for inv in b.inverses.into_iter().rev() {
            inverse_ops.extend(inv.ops);
        }
        let forward = Transaction::new(b.forward)
            .with_kind(b.kind)
            .with_selection(Some(b.before), Some(after));
        let inverse = Transaction::new(inverse_ops)
            .with_kind(b.kind)
            .with_selection(Some(after), Some(b.before));
        self.history.break_group();
        self.history.record(forward, inverse);
        self.history.break_group();
        self.bump_state(true);
        self.anchor = after.anchor;
        self.head = after.head;
        self.goal_x = None;
    }

    /// Tells the layout and the word counts what an applied transaction changed.
    pub(in crate::apps::write) fn note_edit(&mut self, tx: &Transaction) {
        if let Some(v) = &mut self.view {
            v.note(tx);
        }
        self.editor.stats.get_mut().note(tx);
    }

    /// The body's word and character counts (cached per paragraph).
    pub(in crate::apps::write) fn body_statistics(&self) -> kiln_doc::query::Statistics {
        match self.editor.stats.try_borrow_mut() {
            Ok(mut cache) => cache.body(&self.doc),
            Err(_) => self
                .doc
                .statistics(kiln_doc::query::StatisticsOptions::default()),
        }
    }

    /// Takes what Copy or Cut produced (for the session's clipboard write).
    pub(super) fn take_copied(&mut self) -> Option<ClipboardData> {
        self.editor.copied.take()
    }

    /// The host's clock as a field date-time (local time is the host's business; the core
    /// formats what it is given).
    pub(super) fn now(&self) -> kiln_doc::fields::FieldDateTime {
        super::view::date_time(self.editor.now_ms.unwrap_or(0))
    }
}
