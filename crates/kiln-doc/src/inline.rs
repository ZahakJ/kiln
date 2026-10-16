// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paragraph content: text in a rope, formatting runs, annotation marks and inline objects.
//!
//! - **Text** is a [`ropey::Rope`]; offsets everywhere are in `char`s (Unicode scalar values). A
//!   tab is `'\t'`, a non-breaking hyphen `U+2011`, an optional hyphen `U+00AD`.
//! - **Objects** (breaks, drawings, fields, note references, symbols, equations, bookmarks,
//!   opaque passthrough) occupy one [`OBJECT_CHAR`] (`U+FFFC`) each; the *k*-th `U+FFFC` of the
//!   text is `objects[k]`. User text never contains `U+FFFC`.
//! - **Runs** partition the text into spans of identical direct formatting ([`RunProps`], what
//!   `w:r/w:rPr` holds). They are kept normalised: no empty runs and no two neighbours with equal
//!   properties, so equal formatting has exactly one representation.
//! - **Marks** are attributed spans that may overlap: hyperlinks, comment ranges, tracked
//!   insertions/deletions/moves/format changes and inline content controls. They are normalised
//!   too (identical marks that touch or overlap are merged; order is canonical).
//!
//! Marks behave as per-character attributes, like run formatting: inserted text carries exactly
//! the marks of its [`Fragment`], and a mark that spans the insertion point is split around the
//! new text rather than stretched over it. Whether typed text joins a hyperlink, a comment or a
//! tracked insertion is the editing layer's decision (it puts the mark in the fragment; see
//! [`crate::edit::insert_text`]). Because the content of every char is preserved exactly, "delete,
//! then insert what was deleted" restores a paragraph exactly even when the deletion joined two
//! spans of the same mark, which undo relies on.

use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::Range;

use crate::DocError;
use crate::fields::Field;
use crate::model::{Block, Drawing};
use crate::props::RunProps;
use crate::revision::RevisionInfo;
use crate::sdt::SdtInfo;
use crate::st::{BreakClear, BreakKind, NoteKind};
use kiln_common::NodeId;

/// The character that stands for an inline object in paragraph text.
pub const OBJECT_CHAR: char = '\u{FFFC}';

/// A span of identical direct run formatting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpan {
    /// Length in chars (never 0 in a normalised paragraph).
    pub len: usize,
    /// Direct formatting.
    pub props: RunProps,
}

impl RunSpan {
    /// A span.
    #[must_use]
    pub fn new(len: usize, props: RunProps) -> Self {
        Self { len, props }
    }
}

/// A hyperlink target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkTarget {
    /// An external URL (relationship target in DOCX).
    Url(String),
    /// A bookmark in this document (`w:anchor`).
    Bookmark(String),
}

/// A hyperlink (`w:hyperlink`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hyperlink {
    /// Where it goes.
    pub target: LinkTarget,
    /// `w:tooltip`.
    pub tooltip: Option<String>,
    /// `w:tgtFrame`.
    pub frame: Option<String>,
    /// `w:history`.
    pub history: bool,
}

/// An annotation over a range of a paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mark {
    /// A hyperlink.
    Link(Hyperlink),
    /// The range a comment is anchored to (`w:commentRangeStart`/`End`), by comment id.
    Comment(NodeId),
    /// A tracked insertion (`w:ins`).
    Insertion(RevisionInfo),
    /// A tracked deletion (`w:del`).
    Deletion(RevisionInfo),
    /// Text moved away from here (`w:moveFrom`).
    MoveFrom(RevisionInfo),
    /// Text moved here (`w:moveTo`).
    MoveTo(RevisionInfo),
    /// A tracked formatting change (`w:rPrChange`) with the formatting before it.
    FormatChange {
        /// Who and when.
        info: RevisionInfo,
        /// The run properties before the change.
        before: Box<RunProps>,
    },
    /// An inline content control (`w:sdt` around runs).
    Sdt(Box<SdtInfo>),
}

impl Mark {
    /// The revision this mark records, if it is a tracked change.
    #[must_use]
    pub fn revision(&self) -> Option<&RevisionInfo> {
        match self {
            Self::Insertion(i) | Self::Deletion(i) | Self::MoveFrom(i) | Self::MoveTo(i) => Some(i),
            Self::FormatChange { info, .. } => Some(info),
            _ => None,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Sdt(_) => 0,
            Self::Link(_) => 1,
            Self::Comment(_) => 2,
            Self::Insertion(_) => 3,
            Self::Deletion(_) => 4,
            Self::MoveFrom(_) => 5,
            Self::MoveTo(_) => 6,
            Self::FormatChange { .. } => 7,
        }
    }
}

/// A mark over `start..end` (chars).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkSpan {
    /// First char.
    pub start: usize,
    /// One past the last char.
    pub end: usize,
    /// The mark.
    pub mark: Mark,
}

impl MarkSpan {
    /// A span.
    #[must_use]
    pub fn new(start: usize, end: usize, mark: Mark) -> Self {
        Self { start, end, mark }
    }
}

/// A break (`w:br`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Break {
    /// Line, page or column.
    pub kind: BreakKind,
    /// `w:clear` for line breaks around floats.
    pub clear: Option<BreakClear>,
}

/// A symbol character from a symbol font (`w:sym`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    /// `w:font`.
    pub font: String,
    /// `w:char` (often a Private Use Area code, `F0E0`).
    pub code: u32,
}

/// An equation, kept as Office Math Markup (`m:oMath`) so it round-trips untouched.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Equation {
    /// The `m:oMath` (or `m:oMathPara`) XML.
    pub omml: String,
    /// Display equation on its own line (`m:oMathPara`) rather than inline.
    pub display: bool,
}

/// A reference to a footnote or endnote.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteRef {
    /// The note.
    pub note: NodeId,
    /// Footnote or endnote.
    pub kind: NoteKind,
    /// A custom reference mark (`w:customMarkFollows` with the following text).
    pub custom_mark: Option<String>,
}

/// An inline object; each occupies one [`OBJECT_CHAR`] in the paragraph text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineObject {
    /// A line, page or column break.
    Break(Break),
    /// A picture, shape, chart or text box.
    Drawing(Box<Drawing>),
    /// A field (simple or complex) with its cached result.
    Field(Field),
    /// A footnote or endnote reference.
    NoteRef(NoteRef),
    /// A symbol-font character.
    Symbol(Symbol),
    /// An equation.
    Equation(Equation),
    /// Start of a bookmark (`w:bookmarkStart`); bookmarks may span paragraphs.
    BookmarkStart {
        /// The bookmark name (`_Toc…`, `_Ref…` or the user's).
        name: String,
    },
    /// End of a bookmark (`w:bookmarkEnd`).
    BookmarkEnd {
        /// The bookmark name.
        name: String,
    },
    /// Content the model does not understand, kept as XML for the writer.
    Opaque {
        /// The original XML.
        xml: String,
        /// What the editor shows (may be empty).
        label: String,
    },
}

impl InlineObject {
    /// The text this object contributes to plain text (`\n` for a line break, the cached result
    /// for a field, `U+FFFC` for drawings and equations, nothing for markers).
    #[must_use]
    pub fn plain_text(&self) -> String {
        match self {
            Self::Break(b) => match b.kind {
                BreakKind::TextWrapping => "\n".to_owned(),
                BreakKind::Page => "\u{000C}".to_owned(),
                BreakKind::Column => "\u{000B}".to_owned(),
            },
            Self::Field(f) => f.result.clone(),
            Self::Drawing(_) | Self::Equation(_) => OBJECT_CHAR.to_string(),
            Self::Symbol(s) => char::from_u32(s.code)
                .map(|c| {
                    crate::numbering::formats::bullet_display_text(&c.to_string(), Some(&s.font))
                })
                .unwrap_or_default(),
            Self::NoteRef(n) => n.custom_mark.clone().unwrap_or_default(),
            Self::BookmarkStart { .. } | Self::BookmarkEnd { .. } => String::new(),
            Self::Opaque { label, .. } => label.clone(),
        }
    }

    /// Block containers nested in this object (a text box's paragraphs).
    #[must_use]
    pub fn blocks(&self) -> Option<&Vec<Block>> {
        match self {
            Self::Drawing(d) => d.text_box.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn blocks_mut(&mut self) -> Option<&mut Vec<Block>> {
        match self {
            Self::Drawing(d) => d.text_box.as_mut(),
            _ => None,
        }
    }
}

/// A self-contained piece of paragraph content: what [`crate::Op::InsertInline`] inserts and
/// what deleting a range captures.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Fragment {
    /// The text, with one [`OBJECT_CHAR`] per object.
    pub text: String,
    /// Formatting runs covering the whole text.
    pub runs: Vec<RunSpan>,
    /// Marks, relative to the fragment.
    pub marks: Vec<MarkSpan>,
    /// The objects, in text order.
    pub objects: Vec<InlineObject>,
}

impl Fragment {
    /// Plain text with one formatting. [`OBJECT_CHAR`]s in `text` are dropped (objects are
    /// inserted with [`Fragment::object`]).
    #[must_use]
    pub fn text(text: &str, props: RunProps) -> Self {
        let text: String = text.chars().filter(|c| *c != OBJECT_CHAR).collect();
        let len = text.chars().count();
        Self {
            text,
            runs: if len == 0 {
                Vec::new()
            } else {
                vec![RunSpan::new(len, props)]
            },
            marks: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// One object with a formatting.
    #[must_use]
    pub fn object(object: InlineObject, props: RunProps) -> Self {
        Self {
            text: OBJECT_CHAR.to_string(),
            runs: vec![RunSpan::new(1, props)],
            marks: Vec::new(),
            objects: vec![object],
        }
    }

    /// The same fragment with a mark over all of it.
    #[must_use]
    pub fn with_mark(mut self, mark: Mark) -> Self {
        let len = self.len();
        if len > 0 {
            self.marks.push(MarkSpan::new(0, len, mark));
        }
        self
    }

    /// Length in chars.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Checks the fragment's internal consistency.
    ///
    /// # Errors
    ///
    /// [`DocError::InvalidFragment`] when runs do not cover the text exactly, a mark is out of
    /// range or empty, or the object count differs from the number of [`OBJECT_CHAR`]s.
    pub fn validate(&self) -> Result<(), DocError> {
        let len = self.len();
        check_parts(
            len,
            count_objects(&self.text),
            &self.runs,
            &self.marks,
            self.objects.len(),
        )
        .map_err(DocError::InvalidFragment)
    }
}

fn count_objects(text: &str) -> usize {
    text.chars().filter(|c| *c == OBJECT_CHAR).count()
}

fn check_parts(
    len: usize,
    object_chars: usize,
    runs: &[RunSpan],
    marks: &[MarkSpan],
    objects: usize,
) -> Result<(), String> {
    let run_total: usize = runs.iter().map(|r| r.len).sum();
    if run_total != len {
        return Err(format!("runs cover {run_total} chars of {len}"));
    }
    if runs.iter().any(|r| r.len == 0) {
        return Err("empty run".to_owned());
    }
    if let Some(m) = marks.iter().find(|m| m.start >= m.end || m.end > len) {
        return Err(format!("mark {}..{} outside 0..{len}", m.start, m.end));
    }
    if object_chars != objects {
        return Err(format!("{object_chars} object chars but {objects} objects"));
    }
    Ok(())
}

/// The content of a paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Inlines {
    #[serde(with = "rope_serde")]
    text: Rope,
    runs: Vec<RunSpan>,
    marks: Vec<MarkSpan>,
    objects: Vec<InlineObject>,
}

mod rope_serde {
    use ropey::Rope;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(rope: &Rope, s: S) -> Result<S::Ok, S::Error> {
        // A rope is a sequence of chunks; collect once (paragraphs are small).
        let text: String = rope.chunks().collect();
        s.serialize_str(&text)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Rope, D::Error> {
        let text = String::deserialize(d)?;
        Ok(Rope::from_str(&text))
    }
}

impl Inlines {
    /// Empty content.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Content from a fragment.
    ///
    /// # Errors
    ///
    /// [`DocError::InvalidFragment`] for an inconsistent fragment.
    pub fn from_fragment(fragment: Fragment) -> Result<Self, DocError> {
        let mut out = Self::new();
        out.insert(0, fragment)?;
        Ok(out)
    }

    /// Length in chars.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len_chars()
    }

    /// True when there is no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.len_chars() == 0
    }

    /// The text rope (objects appear as [`OBJECT_CHAR`]).
    #[must_use]
    pub fn rope(&self) -> &Rope {
        &self.text
    }

    /// The raw text (objects appear as [`OBJECT_CHAR`]).
    #[must_use]
    pub fn raw_text(&self) -> String {
        self.text.chunks().collect()
    }

    /// The formatting runs.
    #[must_use]
    pub fn runs(&self) -> &[RunSpan] {
        &self.runs
    }

    /// The marks, in canonical order.
    #[must_use]
    pub fn marks(&self) -> &[MarkSpan] {
        &self.marks
    }

    /// The objects, in text order.
    #[must_use]
    pub fn objects(&self) -> &[InlineObject] {
        &self.objects
    }

    pub(crate) fn objects_mut(&mut self) -> &mut [InlineObject] {
        &mut self.objects
    }

    /// The char at an offset.
    #[must_use]
    pub fn char_at(&self, offset: usize) -> Option<char> {
        (offset < self.len()).then(|| self.text.char(offset))
    }

    /// Number of objects before `offset`.
    #[must_use]
    pub fn objects_before(&self, offset: usize) -> usize {
        let end = offset.min(self.len());
        self.text
            .slice(..end)
            .chars()
            .filter(|c| *c == OBJECT_CHAR)
            .count()
    }

    /// The object at `offset`, when the char there is [`OBJECT_CHAR`].
    #[must_use]
    pub fn object_at(&self, offset: usize) -> Option<&InlineObject> {
        if self.char_at(offset)? != OBJECT_CHAR {
            return None;
        }
        self.objects.get(self.objects_before(offset))
    }

    pub(crate) fn object_at_mut(&mut self, offset: usize) -> Option<&mut InlineObject> {
        if self.char_at(offset)? != OBJECT_CHAR {
            return None;
        }
        let k = self.objects_before(offset);
        self.objects.get_mut(k)
    }

    /// Iterates over `(offset, object)` pairs.
    pub fn objects_with_offsets(&self) -> impl Iterator<Item = (usize, &InlineObject)> {
        self.text
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == OBJECT_CHAR)
            .map(|(i, _)| i)
            .zip(self.objects.iter())
    }

    /// The direct formatting at `offset` (of the char at `offset`, or of the last char for the
    /// end of the paragraph). `None` for empty content.
    #[must_use]
    pub fn props_at(&self, offset: usize) -> Option<&RunProps> {
        let mut pos = 0;
        let mut last = None;
        for r in &self.runs {
            if offset < pos + r.len {
                return Some(&r.props);
            }
            pos += r.len;
            last = Some(&r.props);
        }
        last
    }

    /// Iterates over the runs with their text: `(range, props, text)`.
    pub fn run_texts(&self) -> impl Iterator<Item = (Range<usize>, &RunProps, String)> + '_ {
        let mut pos = 0;
        self.runs.iter().map(move |r| {
            let range = pos..pos + r.len;
            pos += r.len;
            let text: String = self.text.slice(range.clone()).chars().collect();
            (range, &r.props, text)
        })
    }

    /// Plain text: objects contribute [`InlineObject::plain_text`]. With `skip_deleted`, text
    /// under tracked deletions and moves away is left out.
    #[must_use]
    pub fn plain_text(&self, skip_deleted: bool) -> String {
        let deleted: Vec<Range<usize>> = if skip_deleted {
            self.marks
                .iter()
                .filter(|m| matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_)))
                .map(|m| m.start..m.end)
                .collect()
        } else {
            Vec::new()
        };
        let mut out = String::with_capacity(self.text.len_bytes());
        let mut object = 0;
        for (i, c) in self.text.chars().enumerate() {
            let is_object = c == OBJECT_CHAR;
            let skip = deleted.iter().any(|r| r.contains(&i));
            if is_object {
                if !skip && let Some(o) = self.objects.get(object) {
                    out.push_str(&o.plain_text());
                }
                object += 1;
            } else if !skip {
                out.push(c);
            }
        }
        out
    }

    /// Checks the invariants: runs cover the text, are non-empty and normalised; marks are in
    /// range, non-empty and normalised; one object per [`OBJECT_CHAR`].
    ///
    /// # Errors
    ///
    /// [`DocError::Corrupt`] describing the first violation.
    pub fn validate(&self) -> Result<(), DocError> {
        let len = self.len();
        let objects = self.text.chars().filter(|c| *c == OBJECT_CHAR).count();
        check_parts(len, objects, &self.runs, &self.marks, self.objects.len())
            .map_err(DocError::Corrupt)?;
        if self.runs.windows(2).any(|w| w[0].props == w[1].props) {
            return Err(DocError::Corrupt("neighbouring runs are equal".into()));
        }
        let mut normal = self.marks.clone();
        normalize_marks(&mut normal);
        if normal != self.marks {
            return Err(DocError::Corrupt("marks are not normalised".into()));
        }
        Ok(())
    }

    fn check_offset(&self, offset: usize) -> Result<(), DocError> {
        if offset > self.len() {
            return Err(DocError::OutOfRange {
                offset,
                len: self.len(),
            });
        }
        Ok(())
    }

    fn check_range(&self, start: usize, end: usize) -> Result<(), DocError> {
        if start > end || end > self.len() {
            return Err(DocError::OutOfRange {
                offset: end.max(start),
                len: self.len(),
            });
        }
        Ok(())
    }

    /// Copies `start..end` as a fragment.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] for a range outside the content.
    pub fn slice(&self, start: usize, end: usize) -> Result<Fragment, DocError> {
        self.check_range(start, end)?;
        let text: String = self.text.slice(start..end).chars().collect();
        let first_obj = self.objects_before(start);
        let n_obj = text.chars().filter(|c| *c == OBJECT_CHAR).count();
        let objects = self.objects[first_obj..first_obj + n_obj].to_vec();
        let mut runs = Vec::new();
        let mut pos = 0;
        for r in &self.runs {
            let (a, b) = (pos.max(start), (pos + r.len).min(end));
            if a < b {
                runs.push(RunSpan::new(b - a, r.props.clone()));
            }
            pos += r.len;
        }
        let marks = self
            .marks
            .iter()
            .filter_map(|m| {
                let (a, b) = (m.start.max(start), m.end.min(end));
                (a < b).then(|| MarkSpan::new(a - start, b - start, m.mark.clone()))
            })
            .collect();
        Ok(Fragment {
            text,
            runs,
            marks,
            objects,
        })
    }

    /// Inserts a fragment at `offset`. Marks starting at or after it move; a mark spanning it is
    /// split around the inserted text, which carries only the fragment's own marks.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] or [`DocError::InvalidFragment`]; the content is unchanged then.
    pub fn insert(&mut self, offset: usize, fragment: Fragment) -> Result<(), DocError> {
        self.check_offset(offset)?;
        fragment.validate()?;
        let n = fragment.len();
        if n == 0 {
            return Ok(());
        }
        let first_obj = self.objects_before(offset);
        self.text.insert(offset, &fragment.text);
        self.objects.splice(first_obj..first_obj, fragment.objects);
        // Runs: split at offset and splice the fragment's runs in.
        let idx = split_runs_at(&mut self.runs, offset);
        self.runs.splice(idx..idx, fragment.runs);
        normalize_runs(&mut self.runs);
        // Marks: per-character, so a spanning mark splits around the new text.
        let mut tails = Vec::new();
        for m in &mut self.marks {
            if m.start >= offset {
                m.start += n;
                m.end += n;
            } else if m.end > offset {
                tails.push(MarkSpan::new(offset + n, m.end + n, m.mark.clone()));
                m.end = offset;
            }
        }
        self.marks.extend(tails);
        self.marks
            .extend(fragment.marks.into_iter().map(|m| MarkSpan {
                start: m.start + offset,
                end: m.end + offset,
                mark: m.mark,
            }));
        normalize_marks(&mut self.marks);
        Ok(())
    }

    /// Removes `start..end` and returns it as a fragment.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`]; the content is unchanged then.
    pub fn remove(&mut self, start: usize, end: usize) -> Result<Fragment, DocError> {
        let fragment = self.slice(start, end)?;
        let n = end - start;
        if n == 0 {
            return Ok(fragment);
        }
        let first_obj = self.objects_before(start);
        self.objects
            .drain(first_obj..first_obj + fragment.objects.len());
        self.text.remove(start..end);
        let a = split_runs_at(&mut self.runs, start);
        let b = split_runs_at(&mut self.runs, end);
        self.runs.drain(a..b);
        normalize_runs(&mut self.runs);
        for m in &mut self.marks {
            m.start = shrink(m.start, start, end);
            m.end = shrink(m.end, start, end);
        }
        normalize_marks(&mut self.marks);
        Ok(fragment)
    }

    /// Replaces the formatting of `offset..offset + Σlen` with `runs` and returns the runs that
    /// were there.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] when the runs reach past the end, [`DocError::InvalidFragment`]
    /// for an empty run.
    pub fn set_runs(
        &mut self,
        offset: usize,
        runs: Vec<RunSpan>,
    ) -> Result<Vec<RunSpan>, DocError> {
        let total: usize = runs.iter().map(|r| r.len).sum();
        let end = offset.checked_add(total).ok_or(DocError::OutOfRange {
            offset,
            len: self.len(),
        })?;
        self.check_range(offset, end)?;
        if runs.iter().any(|r| r.len == 0) {
            return Err(DocError::InvalidFragment("empty run".into()));
        }
        let a = split_runs_at(&mut self.runs, offset);
        let b = split_runs_at(&mut self.runs, end);
        let old: Vec<RunSpan> = self.runs.splice(a..b, runs).collect();
        normalize_runs(&mut self.runs);
        Ok(old)
    }

    /// Replaces all marks (normalised on the way in) and returns the previous ones.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] for a mark outside the content or an empty mark.
    pub fn set_marks(&mut self, mut marks: Vec<MarkSpan>) -> Result<Vec<MarkSpan>, DocError> {
        if let Some(m) = marks
            .iter()
            .find(|m| m.start >= m.end || m.end > self.len())
        {
            return Err(DocError::OutOfRange {
                offset: m.end.max(m.start),
                len: self.len(),
            });
        }
        normalize_marks(&mut marks);
        Ok(std::mem::replace(&mut self.marks, marks))
    }

    /// Adds a mark over `start..end`.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] for an empty or out-of-range span.
    pub fn add_mark(&mut self, start: usize, end: usize, mark: Mark) -> Result<(), DocError> {
        if start >= end {
            return Err(DocError::OutOfRange {
                offset: start,
                len: self.len(),
            });
        }
        self.check_range(start, end)?;
        self.marks.push(MarkSpan::new(start, end, mark));
        normalize_marks(&mut self.marks);
        Ok(())
    }

    /// Removes `mark` (exact equality) from `start..end`, splitting spans that reach outside.
    ///
    /// # Errors
    ///
    /// [`DocError::OutOfRange`] for an out-of-range span.
    pub fn remove_mark(&mut self, start: usize, end: usize, mark: &Mark) -> Result<(), DocError> {
        self.check_range(start, end)?;
        let mut out = Vec::with_capacity(self.marks.len() + 1);
        for m in self.marks.drain(..) {
            if m.mark != *mark || m.end <= start || m.start >= end {
                out.push(m);
                continue;
            }
            if m.start < start {
                out.push(MarkSpan::new(m.start, start, m.mark.clone()));
            }
            if m.end > end {
                out.push(MarkSpan::new(end, m.end, m.mark));
            }
        }
        normalize_marks(&mut out);
        self.marks = out;
        Ok(())
    }

    /// Splits at `offset`: this keeps `..offset` and the tail is returned.
    pub(crate) fn split_off(&mut self, offset: usize) -> Result<Inlines, DocError> {
        let tail = self.remove(offset, self.len())?;
        Inlines::from_fragment(tail)
    }

    /// Appends another paragraph's content.
    pub(crate) fn append(&mut self, other: Inlines) -> Result<(), DocError> {
        let len = other.len();
        let fragment = other.slice(0, len)?;
        let at = self.len();
        self.insert(at, fragment)
    }
}

/// Maps a position through the removal of `start..end`.
fn shrink(p: usize, start: usize, end: usize) -> usize {
    if p <= start {
        p
    } else if p >= end {
        p - (end - start)
    } else {
        start
    }
}

/// Ensures a run boundary at `offset` and returns the index of the run starting there (or the
/// run count when `offset` is the end).
fn split_runs_at(runs: &mut Vec<RunSpan>, offset: usize) -> usize {
    let mut pos = 0;
    for i in 0..runs.len() {
        if pos == offset {
            return i;
        }
        let len = runs[i].len;
        if offset < pos + len {
            let head = offset - pos;
            let props = runs[i].props.clone();
            runs[i].len = head;
            runs.insert(i + 1, RunSpan::new(len - head, props));
            return i + 1;
        }
        pos += len;
    }
    runs.len()
}

fn normalize_runs(runs: &mut Vec<RunSpan>) {
    runs.retain(|r| r.len > 0);
    let mut out: Vec<RunSpan> = Vec::with_capacity(runs.len());
    for r in runs.drain(..) {
        match out.last_mut() {
            Some(last) if last.props == r.props => last.len += r.len,
            _ => out.push(r),
        }
    }
    *runs = out;
}

/// Canonical order of marks: by start, end, kind, then by encoded value (deterministic).
fn mark_order(a: &MarkSpan, b: &MarkSpan) -> Ordering {
    a.start
        .cmp(&b.start)
        .then(a.end.cmp(&b.end))
        .then(a.mark.rank().cmp(&b.mark.rank()))
        .then_with(|| {
            if a.mark == b.mark {
                Ordering::Equal
            } else {
                let ea = postcard::to_allocvec(&a.mark).unwrap_or_default();
                let eb = postcard::to_allocvec(&b.mark).unwrap_or_default();
                ea.cmp(&eb)
            }
        })
}

/// Drops empty marks, merges identical marks that touch or overlap, and sorts canonically.
pub(crate) fn normalize_marks(marks: &mut Vec<MarkSpan>) {
    marks.retain(|m| m.start < m.end);
    // Group identical marks (marks per paragraph are few, so quadratic grouping is fine).
    let mut groups: Vec<(Mark, Vec<(usize, usize)>)> = Vec::new();
    for m in marks.drain(..) {
        match groups.iter_mut().find(|(k, _)| *k == m.mark) {
            Some((_, spans)) => spans.push((m.start, m.end)),
            None => groups.push((m.mark, vec![(m.start, m.end)])),
        }
    }
    for (mark, mut spans) in groups {
        spans.sort_unstable();
        let mut cur: Option<(usize, usize)> = None;
        for (s, e) in spans {
            cur = match cur {
                Some((cs, ce)) if s <= ce => Some((cs, ce.max(e))),
                Some((cs, ce)) => {
                    marks.push(MarkSpan::new(cs, ce, mark.clone()));
                    Some((s, e))
                }
                None => Some((s, e)),
            };
        }
        if let Some((cs, ce)) = cur {
            marks.push(MarkSpan::new(cs, ce, mark));
        }
    }
    marks.sort_by(mark_order);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::st::BreakKind;

    fn bold() -> RunProps {
        RunProps {
            bold: Some(true),
            ..RunProps::default()
        }
    }

    fn link(url: &str) -> Mark {
        Mark::Link(Hyperlink {
            target: LinkTarget::Url(url.into()),
            tooltip: None,
            frame: None,
            history: true,
        })
    }

    fn content(text: &str) -> Result<Inlines, DocError> {
        Inlines::from_fragment(Fragment::text(text, RunProps::default()))
    }

    #[test]
    fn insert_splits_runs_and_merges_equal_neighbours() -> Result<(), DocError> {
        let mut c = content("Hello world")?;
        c.insert(5, Fragment::text(" big", bold()))?;
        assert_eq!(c.raw_text(), "Hello big world");
        assert_eq!(c.runs().len(), 3);
        assert_eq!(c.runs()[1], RunSpan::new(4, bold()));
        c.insert(0, Fragment::text("", bold()))?;
        c.set_runs(5, vec![RunSpan::new(4, RunProps::default())])?;
        assert_eq!(c.runs().len(), 1);
        c.validate()?;
        assert_eq!(c.props_at(3), Some(&RunProps::default()));
        assert_eq!(c.props_at(100), Some(&RunProps::default()));
        assert_eq!(Inlines::new().props_at(0), None);
        Ok(())
    }

    #[test]
    fn remove_returns_what_insert_restores() -> Result<(), DocError> {
        let mut c = content("abcdefghij")?;
        c.set_runs(2, vec![RunSpan::new(4, bold())])?;
        c.add_mark(1, 9, link("https://a"))?;
        c.add_mark(4, 6, Mark::Comment(NodeId(7)))?;
        c.insert(
            3,
            Fragment::object(
                InlineObject::Break(Break {
                    kind: BreakKind::TextWrapping,
                    clear: None,
                }),
                RunProps::default(),
            ),
        )?;
        let before = c.clone();
        for (a, b) in [(0, 3), (2, 5), (4, 6), (0, 11), (6, 11), (3, 4), (5, 5)] {
            let frag = c.remove(a, b)?;
            c.validate()?;
            c.insert(a, frag)?;
            assert_eq!(c, before, "remove {a}..{b}");
        }
        Ok(())
    }

    #[test]
    fn marks_are_per_character() -> Result<(), DocError> {
        let mut c = content("abcdef")?;
        c.add_mark(2, 4, link("u"))?;
        c.insert(4, Fragment::text("X", RunProps::default()))?;
        c.insert(2, Fragment::text("Y", RunProps::default()))?;
        assert_eq!((c.marks()[0].start, c.marks()[0].end), (3, 5));
        // Plain text inserted inside a mark splits it.
        c.insert(4, Fragment::text("Z", RunProps::default()))?;
        let spans: Vec<_> = c.marks().iter().map(|m| (m.start, m.end)).collect();
        assert_eq!(spans, vec![(3, 4), (5, 6)]);
        // Text that carries the mark joins it again.
        let mut d = content("abcdef")?;
        d.add_mark(2, 4, link("u"))?;
        d.insert(
            3,
            Fragment::text("Z", RunProps::default()).with_mark(link("u")),
        )?;
        let spans: Vec<_> = d.marks().iter().map(|m| (m.start, m.end)).collect();
        assert_eq!(spans, vec![(2, 5)]);
        Ok(())
    }

    #[test]
    fn deleting_between_two_spans_of_a_mark_is_undoable() -> Result<(), DocError> {
        let mut c = content("abcdefghijklmn")?;
        c.add_mark(0, 5, Mark::Comment(NodeId(1)))?;
        c.add_mark(10, 14, Mark::Comment(NodeId(1)))?;
        let before = c.clone();
        let frag = c.remove(1, 10)?;
        assert_eq!(c.marks().len(), 1, "the two spans touch and merge");
        c.insert(1, frag)?;
        assert_eq!(c, before);
        Ok(())
    }

    #[test]
    fn identical_marks_merge_and_remove_mark_splits() -> Result<(), DocError> {
        let mut c = content("abcdefghij")?;
        c.add_mark(0, 4, link("u"))?;
        c.add_mark(4, 8, link("u"))?;
        c.add_mark(2, 3, link("v"))?;
        assert_eq!(c.marks().len(), 2);
        assert_eq!((c.marks()[0].start, c.marks()[0].end), (0, 8));
        c.remove_mark(3, 5, &link("u"))?;
        let spans: Vec<_> = c
            .marks()
            .iter()
            .filter(|m| m.mark == link("u"))
            .map(|m| (m.start, m.end))
            .collect();
        assert_eq!(spans, vec![(0, 3), (5, 8)]);
        assert!(c.add_mark(3, 3, link("u")).is_err());
        assert!(c.add_mark(3, 30, link("u")).is_err());
        let old = c.set_marks(Vec::new())?;
        assert_eq!(old.len(), 3);
        assert!(c.set_marks(vec![MarkSpan::new(0, 99, link("u"))]).is_err());
        Ok(())
    }

    #[test]
    fn objects_follow_their_placeholder() -> Result<(), DocError> {
        let mut c = content("ab")?;
        let field = InlineObject::Opaque {
            xml: "<x/>".into(),
            label: "[x]".into(),
        };
        c.insert(1, Fragment::object(field.clone(), RunProps::default()))?;
        c.insert(
            0,
            Fragment::object(
                InlineObject::BookmarkStart { name: "b".into() },
                RunProps::default(),
            ),
        )?;
        assert_eq!(c.objects().len(), 2);
        assert_eq!(c.object_at(2), Some(&field));
        assert_eq!(c.object_at(1), None);
        assert_eq!(c.objects_before(3), 2);
        assert_eq!(c.plain_text(false), "a[x]b");
        let pairs: Vec<usize> = c.objects_with_offsets().map(|(o, _)| o).collect();
        assert_eq!(pairs, vec![0, 2]);
        let tail = c.split_off(2)?;
        assert_eq!(tail.objects(), std::slice::from_ref(&field));
        assert_eq!(c.objects().len(), 1);
        c.append(tail)?;
        assert_eq!(c.len(), 4);
        c.validate()?;
        Ok(())
    }

    #[test]
    fn plain_text_can_skip_deletions() -> Result<(), DocError> {
        let mut c = content("keep gone")?;
        c.add_mark(4, 9, Mark::Deletion(RevisionInfo::new(1, "A", None)))?;
        assert_eq!(c.plain_text(true), "keep");
        assert_eq!(c.plain_text(false), "keep gone");
        Ok(())
    }

    #[test]
    fn rejects_bad_ranges_and_fragments() -> Result<(), DocError> {
        let mut c = content("abc")?;
        assert!(
            c.insert(4, Fragment::text("x", RunProps::default()))
                .is_err()
        );
        assert!(c.remove(2, 1).is_err());
        assert!(c.remove(0, 4).is_err());
        assert!(c.slice(0, 9).is_err());
        assert!(c.set_runs(2, vec![RunSpan::new(5, bold())]).is_err());
        assert!(c.set_runs(0, vec![RunSpan::new(0, bold())]).is_err());
        assert!(
            c.set_runs(usize::MAX, vec![RunSpan::new(5, bold())])
                .is_err()
        );
        let bad = Fragment {
            text: "ab".into(),
            runs: vec![RunSpan::new(1, RunProps::default())],
            ..Fragment::default()
        };
        assert!(c.insert(0, bad).is_err());
        let bad_obj = Fragment {
            text: OBJECT_CHAR.to_string(),
            runs: vec![RunSpan::new(1, RunProps::default())],
            ..Fragment::default()
        };
        assert!(c.insert(0, bad_obj).is_err());
        assert_eq!(c.raw_text(), "abc");
        assert_eq!(Fragment::text("a\u{FFFC}b", RunProps::default()).text, "ab");
        assert!(Fragment::text("", RunProps::default()).is_empty());
        assert_eq!(
            Fragment::text("ab", bold())
                .with_mark(link("u"))
                .marks
                .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn run_texts_and_serde_round_trip() -> Result<(), DocError> {
        let mut c = content("plain bold")?;
        c.set_runs(6, vec![RunSpan::new(4, bold())])?;
        let texts: Vec<String> = c.run_texts().map(|(_, _, t)| t).collect();
        assert_eq!(texts, vec!["plain ", "bold"]);
        let bytes = postcard::to_allocvec(&c).map_err(|e| DocError::Corrupt(e.to_string()))?;
        let back: Inlines =
            postcard::from_bytes(&bytes).map_err(|e| DocError::Corrupt(e.to_string()))?;
        assert_eq!(back, c);
        assert_eq!(c.char_at(0), Some('p'));
        assert_eq!(c.char_at(99), None);
        assert_eq!(c.rope().len_chars(), 10);
        Ok(())
    }
}
