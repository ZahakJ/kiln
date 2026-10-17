// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write's advanced features (WP-66), on top of the editor of WP-64:
//!
//! - [`comments`] — new, reply, edit, resolve, delete and navigate; the Comments pane's data and
//!   the balloons of All Markup (placed by `kiln-doc-layout::annotations`);
//! - [`track`] — Track Changes: every edit made while it is on is recorded as insertions,
//!   deletions and formatting changes by the reviewer; markup modes (Simple, All, None,
//!   Original) that change the layout; Accept and Reject (one, all shown, all); Previous and
//!   Next; the Reviewing pane's data with one colour per reviewer;
//! - [`toc`] — tables of contents from heading styles (levels, leaders, clickable entries),
//!   Update Table, Add Text, tables of figures;
//! - [`notes`] — footnotes and endnotes: insert, navigate, Show Notes, numbering, convert;
//! - [`captions`] — captions with SEQ fields (and chapter numbers) and cross-references (REF,
//!   PAGEREF, NOTEREF to bookmarks the command adds);
//! - [`objects`] — floating objects: selection, position presets, wrapping, dragging and
//!   resizing, the Layout dialog, z-order, rotation, alignment, the Selection pane, text boxes
//!   and shapes with text;
//! - [`painter`] — Format Painter (single and sticky) and Copy/Paste Formatting;
//! - [`autoformat`] — Markdown shortcuts and smart typography as you type; [`markdown`] —
//!   Paste Markdown;
//! - [`blocks`] — code blocks, callouts, cover pages, document properties, word-count goals,
//!   hyphenation and focus mode;
//! - [`spell`] — spelling as you type (`kiln-spell`), squiggles, suggestions, Ignore, Ignore All,
//!   Add to Dictionary and the F7 pane's data.
//!
//! The hooks into the editor are few: [`WriteDoc::run_feature`] for the commands, the tracked
//! rewrite of every transaction while Track Changes is on ([`track`]), the typing hook of
//! AutoFormat, the overlay (change bars, balloon connectors, squiggles) and the query answers.

pub(super) mod autoformat;
pub(super) mod blocks;
pub(super) mod captions;
pub(super) mod comments;
pub(super) mod markdown;
pub(super) mod notes;
pub(super) mod objects;
pub(super) mod painter;
pub(super) mod spell;
#[cfg(test)]
mod tests;
pub(super) mod toc;
pub(super) mod track;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Block, Document, Op, Paragraph, Pos, Range, RunProps};
use kiln_protocol::WriteCommand;
use kiln_protocol::commands::write_features::{AutoFormatOptions, FeatureState, MarkupMode};
use kiln_protocol::commands::{WriteQuery, WriteQueryResult};
use kiln_scene::DrawOp;

use super::WriteDoc;
use crate::model::Change;

/// The reviewer's name when the user has not set one (Word's anonymous author).
pub(super) const DEFAULT_AUTHOR: &str = "Author";

/// Review ▸ Display for Review and Show Markup.
#[derive(Debug, Clone)]
pub(super) struct ReviewView {
    pub author: String,
    pub markup: MarkupMode,
    pub show_comments: bool,
    pub show_insertions: bool,
    pub show_formatting: bool,
}

impl Default for ReviewView {
    fn default() -> Self {
        Self {
            author: DEFAULT_AUTHOR.to_owned(),
            // Word opens documents in Simple Markup.
            markup: MarkupMode::Simple,
            show_comments: true,
            show_insertions: true,
            show_formatting: true,
        }
    }
}

/// The feature state of one Write document.
#[derive(Debug, Default)]
pub(in crate::apps::write) struct Features {
    pub review: ReviewView,
    /// Bypass Track Changes for the transactions applied now (accepting and rejecting).
    pub untracked: bool,
    /// The next `w:id` for a tracked change (0 until the document has been scanned).
    pub next_revision: u32,
    pub painter: Option<painter::Painter>,
    pub copied: Option<painter::Captured>,
    pub autoformat: AutoFormatOptionsState,
    pub focus: bool,
    pub spell: spell::SpellState,
    /// History steps that undo and redo as one (an AutoFormat that deletes the typed marker
    /// and then applies a style or a list): keyed by the state id of the group's top step.
    pub groups: Vec<UndoGroup>,
    /// Bumped by every change of the document (`note_edit`): the key of [`DocCache`].
    pub generation: u64,
    /// Whole-document answers kept until the document changes (typing in a 300-page document
    /// must not walk it for every frame).
    pub cache: std::cell::RefCell<DocCache>,
}

/// Answers that need a walk of the whole document, for one [`Features::generation`].
#[derive(Debug, Default)]
pub(in crate::apps::write) struct DocCache {
    generation: Option<u64>,
    revisions: Option<std::sync::Arc<Vec<kiln_doc::query::RevisionEntry>>>,
    has_toc: Option<bool>,
}

/// Several consecutive undo steps that Ctrl+Z and Ctrl+Y treat as one.
#[derive(Debug, Clone, Copy)]
pub(in crate::apps::write) struct UndoGroup {
    /// The state id of the group's first step.
    pub first: crate::undo::StateId,
    /// The state id of its last step.
    pub last: crate::undo::StateId,
    pub steps: usize,
}

/// AutoFormat options with Word's defaults (all on).
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::apps::write) struct AutoFormatOptionsState(pub AutoFormatOptions);

/// Whether a Write command is one of this module's.
#[must_use]
pub(super) fn handles(cmd: &WriteCommand) -> bool {
    use WriteCommand as W;
    matches!(
        cmd,
        W::NewComment { .. }
            | W::ReplyComment { .. }
            | W::EditComment { .. }
            | W::ResolveComment { .. }
            | W::DeleteComment { .. }
            | W::DeleteAllComments
            | W::SelectComment { .. }
            | W::TrackChanges
            | W::SetReviewer { .. }
            | W::MarkupMode { .. }
            | W::ShowMarkup { .. }
            | W::AcceptChange { .. }
            | W::RejectChange { .. }
            | W::ResolveRevision { .. }
            | W::PreviousChange
            | W::NextChange
            | W::InsertToc { .. }
            | W::InsertQuickToc
            | W::RemoveToc
            | W::UpdateToc { .. }
            | W::AddText { .. }
            | W::InsertTableOfFigures { .. }
            | W::InsertFootnote
            | W::InsertEndnote
            | W::NextNote { .. }
            | W::ShowNotes
            | W::NoteOptions { .. }
            | W::ConvertNotes { .. }
            | W::InsertCaption { .. }
            | W::InsertCrossReference { .. }
            | W::FollowLink
            | W::SelectObject { .. }
            | W::ObjectPosition { .. }
            | W::ObjectWrap { .. }
            | W::ObjectTransform { .. }
            | W::ObjectLayout { .. }
            | W::ObjectOrder { .. }
            | W::ObjectRotate { .. }
            | W::ObjectAlign { .. }
            | W::InsertTextBox { .. }
            | W::InsertShape { .. }
            | W::FormatPainter { .. }
            | W::CopyFormat
            | W::PasteFormat
            | W::CancelPainter
            | W::SetAutoFormat { .. }
            | W::PasteMarkdown { .. }
            | W::FocusMode
            | W::WordGoal { .. }
            | W::Hyphenation
            | W::InsertCodeBlock { .. }
            | W::InsertCallout { .. }
            | W::InsertCoverPage { .. }
            | W::SetProperties { .. }
            | W::SpellCheck { .. }
            | W::SetUserDictionary { .. }
            | W::SpellAddWord { .. }
            | W::SpellIgnoreAll { .. }
            | W::SpellIgnore { .. }
            | W::SpellReplace { .. }
            | W::SpellNext
            | W::SpellDictionary { .. }
    )
}

/// Commands of this module that F4 does not repeat (views, navigation, one-off actions).
#[must_use]
pub(super) fn repeatable(cmd: &WriteCommand) -> bool {
    use WriteCommand as W;
    matches!(
        cmd,
        W::AddText { .. }
            | W::InsertFootnote
            | W::InsertEndnote
            | W::ObjectWrap { .. }
            | W::ObjectPosition { .. }
            | W::ObjectOrder { .. }
            | W::ObjectRotate { .. }
            | W::ObjectAlign { .. }
            | W::InsertCallout { .. }
            | W::InsertCodeBlock { .. }
    )
}

/// `RRGGBB` → an ISO 8601 timestamp of the host's clock (`2026-10-17T09:30:00Z`).
#[must_use]
pub(super) fn iso(now: &kiln_doc::fields::FieldDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    )
}

/// Initials of a name ("Zoë Ångström" → "ZÅ"), at most three letters.
#[must_use]
pub(super) fn initials(name: &str) -> String {
    let out: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .flat_map(char::to_uppercase)
        .take(3)
        .collect();
    if out.is_empty() { "?".to_owned() } else { out }
}

/// `PutStyle` ops for Word's built-in styles the document lacks (with their bases and links).
#[must_use]
pub(super) fn missing_styles(doc: &Document, ids: &[&str]) -> Vec<Op> {
    let all = kiln_doc::template::default_styles();
    let mut wanted: Vec<String> = Vec::new();
    let mut queue: Vec<String> = ids.iter().map(|s| (*s).to_owned()).collect();
    while let Some(id) = queue.pop() {
        if doc.styles.contains(&id) || wanted.contains(&id) {
            continue;
        }
        let Some(s) = all.iter().find(|s| s.id == id) else {
            continue;
        };
        wanted.push(id.clone());
        if let Some(b) = &s.based_on {
            queue.push(b.clone());
        }
        if let Some(l) = &s.link {
            queue.push(l.clone());
        }
    }
    // Bases first so `basedOn` always names an existing style.
    let mut ordered: Vec<&kiln_doc::Style> = Vec::new();
    let mut pending: Vec<&kiln_doc::Style> =
        all.iter().filter(|s| wanted.contains(&s.id)).collect();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|s| {
            let ready = s
                .based_on
                .as_ref()
                .is_none_or(|b| doc.styles.contains(b) || ordered.iter().any(|o| &o.id == b));
            if ready {
                ordered.push(s);
            }
            !ready
        });
        if pending.len() == before {
            ordered.append(&mut pending);
        }
    }
    let mut index = doc.styles.len();
    ordered
        .into_iter()
        .map(|s| {
            let op = Op::PutStyle {
                style: Box::new(s.clone()),
                index,
            };
            index += 1;
            op
        })
        .collect()
}

/// Every paragraph of a block list, in order (tables, content controls and field results
/// included).
pub(super) fn paragraphs_of<'a>(blocks: &'a [Block], out: &mut Vec<&'a Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        paragraphs_of(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => paragraphs_of(&s.blocks, out),
            Block::Field(f) => paragraphs_of(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

impl WriteDoc {
    /// Runs one of this module's commands.
    pub(super) fn run_feature(&mut self, cmd: &WriteCommand) -> Result<Change> {
        self.end_composition();
        let display = (
            self.features.review.markup,
            self.features.review.show_comments,
            self.features.review.show_insertions,
            self.features.review.show_formatting,
        );
        let settings = self.doc.settings.clone();
        let flags = (
            self.features.painter.is_some(),
            self.features.copied.is_some(),
            self.features.focus,
            self.features.spell.on,
            self.features.autoformat.0,
            self.features.review.author.clone(),
            self.features.spell.revision,
        );
        let mut change = self.measure(|d| d.dispatch_feature(cmd))?;
        if (
            self.features.review.markup,
            self.features.review.show_comments,
            self.features.review.show_insertions,
            self.features.review.show_formatting,
        ) != display
        {
            self.sync_review_display();
        }
        if change == Change::None {
            let now = (
                self.features.painter.is_some(),
                self.features.copied.is_some(),
                self.features.focus,
                self.features.spell.on,
                self.features.autoformat.0,
                self.features.review.author.clone(),
                self.features.spell.revision,
            );
            if now != flags
                || (
                    self.features.review.markup,
                    self.features.review.show_comments,
                    self.features.review.show_insertions,
                    self.features.review.show_formatting,
                ) != display
            {
                change = Change::Selection;
            }
        }
        // Settings changed outside the history (Track Changes on/off) count as content: the
        // document is saved with them.
        if self.doc.settings != settings && change != Change::Content {
            change = Change::Content;
        }
        Ok(change)
    }

    fn dispatch_feature(&mut self, cmd: &WriteCommand) -> Result<()> {
        use WriteCommand as W;
        match cmd {
            W::NewComment { text } => self.new_comment(text),
            W::ReplyComment { parent, text } => self.reply_comment(*parent, text),
            W::EditComment { id, text } => self.edit_comment(*id, text),
            W::ResolveComment { id, resolved } => self.resolve_comment(*id, *resolved),
            W::DeleteComment { id } => self.delete_comment(*id),
            W::DeleteAllComments => self.delete_all_comments(),
            W::SelectComment { id } => self.select_comment(*id),
            W::TrackChanges => {
                self.toggle_tracking();
                Ok(())
            }
            W::SetReviewer { name } => self.set_reviewer(name),
            W::MarkupMode { mode } => {
                self.features.review.markup = *mode;
                Ok(())
            }
            W::ShowMarkup { kind } => {
                self.toggle_show_markup(*kind);
                Ok(())
            }
            W::AcceptChange { scope } => self.resolve_changes(*scope, true),
            W::RejectChange { scope } => self.resolve_changes(*scope, false),
            W::ResolveRevision { target, accept } => self.resolve_one(target, *accept),
            W::PreviousChange => self.next_change(true),
            W::NextChange => self.next_change(false),
            W::InsertToc { spec } => self.insert_toc(spec),
            W::InsertQuickToc => {
                self.insert_toc(&kiln_protocol::commands::write_features::TocSpec::default())
            }
            W::RemoveToc => self.remove_toc(),
            W::UpdateToc { entire } => self.update_toc(*entire),
            W::AddText { level } => self.add_text(*level),
            W::InsertTableOfFigures { label } => self.insert_table_of_figures(label),
            W::InsertFootnote => self.insert_note(kiln_doc::st::NoteKind::Footnote),
            W::InsertEndnote => self.insert_note(kiln_doc::st::NoteKind::Endnote),
            W::NextNote { backward } => self.next_note(*backward),
            W::ShowNotes => self.show_notes(),
            W::NoteOptions { spec } => self.note_options(spec),
            W::ConvertNotes { to_endnotes } => self.convert_notes(*to_endnotes),
            W::InsertCaption { spec } => self.insert_caption(spec),
            W::InsertCrossReference { spec } => self.insert_cross_reference(spec),
            W::FollowLink => self.follow_link(),
            W::SelectObject { id } => self.select_object(*id),
            W::ObjectPosition { preset } => self.object_position(*preset),
            W::ObjectWrap { wrap } => self.object_wrap(*wrap),
            W::ObjectTransform { id, rect } => self.object_transform(*id, *rect),
            W::ObjectLayout { id, spec } => self.object_layout(*id, spec),
            W::ObjectOrder { order } => self.object_order(*order),
            W::ObjectRotate { op } => self.object_rotate(*op),
            W::ObjectAlign { align } => self.object_align(*align),
            W::InsertTextBox { text } => self.insert_text_box(text.as_deref()),
            W::InsertShape { preset, text } => self.insert_shape(preset, text.as_deref()),
            W::FormatPainter { sticky } => self.format_painter(*sticky),
            W::CopyFormat => self.copy_format(),
            W::PasteFormat => self.paste_format(),
            W::CancelPainter => {
                self.features.painter = None;
                Ok(())
            }
            W::SetAutoFormat { options } => {
                self.features.autoformat = AutoFormatOptionsState(*options);
                Ok(())
            }
            W::PasteMarkdown { text } => self.paste_markdown(text),
            W::FocusMode => {
                self.features.focus = !self.features.focus;
                Ok(())
            }
            W::WordGoal { words } => self.set_word_goal(*words),
            W::Hyphenation => self.toggle_hyphenation(),
            W::InsertCodeBlock { language } => self.insert_code_block(language.as_deref()),
            W::InsertCallout { kind } => self.insert_callout(*kind),
            W::InsertCoverPage { design } => self.insert_cover_page(*design),
            W::SetProperties { props } => self.set_properties(props),
            W::SpellCheck { on } => {
                self.features.spell.on = *on;
                self.features.spell.revision += 1;
                Ok(())
            }
            W::SetUserDictionary { words } => {
                self.features.spell.speller.set_user_words(words);
                self.features.spell.invalidate();
                Ok(())
            }
            W::SpellAddWord { word } => self.spell_add_word(word),
            W::SpellIgnoreAll { word } => {
                self.features.spell.speller.ignore_all(word);
                self.features.spell.invalidate();
                Ok(())
            }
            W::SpellIgnore { pos } => self.spell_ignore_once(*pos),
            W::SpellReplace { pos, with, all } => self.spell_replace(*pos, with, *all),
            W::SpellNext => self.spell_next(),
            W::SpellDictionary { aff, dic } => {
                kiln_spell::load_dictionary(aff, dic).map_err(|e| {
                    KilnError::InvalidInput(format!(
                        "the spelling dictionary could not be read: {e}"
                    ))
                })?;
                self.features.spell.invalidate();
                Ok(())
            }
            other => Err(KilnError::Internal(format!(
                "{} reached the feature dispatcher",
                other.meta().id
            ))),
        }
    }

    /// Answers this module's queries (`None` for the editor's own).
    pub(super) fn feature_query(&self, q: &WriteQuery) -> Option<Result<WriteQueryResult>> {
        Some(match q {
            WriteQuery::Comments => Ok(WriteQueryResult::Comments(self.comment_infos())),
            WriteQuery::Revisions => {
                Ok(WriteQueryResult::Revisions(Box::new(self.revisions_info())))
            }
            WriteQuery::CrossRefTargets => {
                Ok(WriteQueryResult::CrossRefTargets(self.cross_ref_targets()))
            }
            WriteQuery::Objects => Ok(WriteQueryResult::Objects(self.object_infos())),
            WriteQuery::SpellingAt { pos } => self
                .spelling_at(*pos)
                .map(|h| WriteQueryResult::Spelling(h.map(Box::new))),
            WriteQuery::NextMisspelling { from } => self
                .next_misspelling(*from)
                .map(|h| WriteQueryResult::Spelling(h.map(Box::new))),
            _ => return None,
        })
    }

    /// The feature state reported with the selection's formatting.
    pub(super) fn feature_state(&self) -> FeatureState {
        let r = &self.features.review;
        let (s, _) = self.span();
        FeatureState {
            tracking: self.doc.settings.track_revisions,
            author: r.author.clone(),
            markup: r.markup,
            show_comments: r.show_comments,
            show_insertions: r.show_insertions,
            show_formatting: r.show_formatting,
            revisions: u32::try_from(self.revisions_cached().len()).unwrap_or(u32::MAX),
            comments: u32::try_from(
                self.doc
                    .comments
                    .iter()
                    .filter(|c| c.meta.parent.is_none())
                    .count(),
            )
            .unwrap_or(u32::MAX),
            open_comments: u32::try_from(
                self.doc
                    .comments
                    .iter()
                    .filter(|c| c.meta.parent.is_none() && !c.meta.resolved)
                    .count(),
            )
            .unwrap_or(u32::MAX),
            comment: self.comment_at(s),
            painter: self.features.painter.as_ref().map(|p| p.sticky),
            copied_format: self.features.copied.is_some(),
            focus: self.features.focus,
            word_goal: self.word_goal(),
            autoformat: self.features.autoformat.0,
            spelling: self.features.spell.on,
            dictionary: kiln_spell::has_dictionary(),
            misspellings: self.features.spell.count(),
            hyphenation: self.doc.settings.auto_hyphenation,
            object: self.selected_object_info(),
            in_note: matches!(self.doc.story_of(s.para), Some(kiln_doc::Story::Note(_))),
            has_toc: self.has_toc_cached(),
        }
    }

    /// The cache of whole-document answers, emptied when the document changed.
    fn doc_cache(&self) -> std::cell::RefMut<'_, DocCache> {
        let mut c = self.features.cache.borrow_mut();
        if c.generation != Some(self.features.generation) {
            *c = DocCache {
                generation: Some(self.features.generation),
                ..DocCache::default()
            };
        }
        c
    }

    /// Every tracked change of the document (cached until it changes).
    pub(super) fn revisions_cached(&self) -> std::sync::Arc<Vec<kiln_doc::query::RevisionEntry>> {
        let mut c = self.doc_cache();
        c.revisions
            .get_or_insert_with(|| std::sync::Arc::new(self.doc.revisions()))
            .clone()
    }

    /// Whether the document has a table of contents (cached until it changes).
    fn has_toc_cached(&self) -> bool {
        let mut c = self.doc_cache();
        *c.has_toc
            .get_or_insert_with(|| toc::find_toc(&self.doc, false).is_some())
    }

    /// The overlay this module adds: change bars, balloon connectors, squiggles.
    pub(super) fn feature_overlay(&self) -> Vec<DrawOp> {
        let Some(v) = &self.view else {
            return Vec::new();
        };
        let mut ops = Vec::new();
        ops.extend(self.review_overlay(v));
        ops.extend(self.spell_overlay(v));
        ops
    }

    /// The direct run formatting at a position (the char before it, else the char after it).
    pub(super) fn props_at(&self, pos: Pos) -> RunProps {
        self.doc
            .paragraph(pos.para)
            .map(|p| kiln_doc::edit::typing_props(p, pos.offset))
            .unwrap_or_default()
    }

    /// The paragraph, or an error.
    pub(super) fn paragraph(&self, id: NodeId) -> Result<&Paragraph> {
        self.doc
            .paragraph(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no paragraph {id}")))
    }

    /// The selection as a range.
    pub(super) fn selection_range(&self) -> Range {
        Range::new(self.anchor, self.head)
    }
}

impl WriteDoc {
    /// The number of history entries since `len` (from `self.states.len()`) become one undo step.
    pub(super) fn group_undo_since(&mut self, len: usize) {
        let steps = self.states.len().saturating_sub(len);
        if steps < 2 {
            return;
        }
        let group = UndoGroup {
            first: self.states[len],
            last: self.states[self.states.len() - 1],
            steps,
        };
        let groups = &mut self.features.groups;
        groups.push(group);
        if groups.len() > 64 {
            groups.remove(0);
        }
    }

    /// How many history steps the next Ctrl+Z undoes.
    pub(in crate::apps::write) fn grouped_undo_steps(&self) -> usize {
        self.features
            .groups
            .iter()
            .rev()
            .find(|g| {
                g.last == self.state
                    && self.states.len() >= g.steps
                    && self.states[self.states.len() - g.steps] == g.first
            })
            .map_or(1, |g| g.steps)
    }

    /// How many history steps the next Ctrl+Y redoes.
    pub(in crate::apps::write) fn grouped_redo_steps(&self) -> usize {
        let Some(&next) = self.undone.last() else {
            return 1;
        };
        self.features
            .groups
            .iter()
            .rev()
            .find(|g| {
                g.first == next
                    && self.undone.len() >= g.steps
                    && self.undone[self.undone.len() - g.steps] == g.last
            })
            .map_or(1, |g| g.steps)
    }
}
