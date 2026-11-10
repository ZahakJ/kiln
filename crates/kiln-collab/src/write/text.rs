// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paragraph content ↔ `LoroText`.
//!
//! Characters are the model's characters (an inline object is one `U+FFFC`). Formatting is
//! per character, as in the model, spread over text marks:
//!
//! - `r.<field>`: one key per set [`RunProps`] field (JSON), so concurrent bold and italic on
//!   the same words both survive;
//! - `m.<hash>`: one key per distinct [`Mark`] (hyperlink, comment range, tracked change,
//!   content control), the value its JSON;
//! - `o`: the inline object an object character stands for (JSON).
//!
//! Marks never expand over text typed at their edges (see [`crate::LoroCrdt`]); text typed
//! inside a range gets the range's marks from Loro and then exactly its fragment's formatting
//! from [`sync_text`], so both sides always hold the same per-character formatting.

use std::collections::{BTreeMap, HashMap};

use kiln_common::NodeId;
use kiln_doc::{
    Document, Fragment, InlineObject, Inlines, Mark, MarkSpan, OBJECT_CHAR, Op, RunProps, RunSpan,
};
use loro::{LoroText, LoroValue, TextDelta};
use serde_json::{Map, Value};

use crate::error::{Result, corrupt};
use crate::value::{fields_of, fnv64, from_json, to_json};

/// The formatting of a stretch of characters: text-mark key → JSON value.
pub(crate) type Attrs = BTreeMap<String, String>;

const RUN: &str = "r.";
const MARK: &str = "m.";
const OBJECT: &str = "o";

/// A stretch of characters with the same formatting.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seg {
    pub len: usize,
    pub attrs: Attrs,
}

/// The text-mark key of a mark.
fn mark_key(json: &str) -> String {
    format!("{MARK}{:016x}", fnv64(json.as_bytes()))
}

/// A paragraph's characters and formatting stretches.
pub(crate) fn model_segments(content: &Inlines) -> Result<(Vec<char>, Vec<Seg>)> {
    let chars: Vec<char> = content.rope().chars().collect();
    let len = chars.len();
    let mut cuts = vec![0, len];
    let mut runs = Vec::with_capacity(content.runs().len());
    let mut at = 0;
    for r in content.runs() {
        cuts.push(at);
        runs.push((at, at + r.len, fields_of(&r.props)?));
        at += r.len;
    }
    let mut marks = Vec::with_capacity(content.marks().len());
    for m in content.marks() {
        cuts.push(m.start);
        cuts.push(m.end);
        let json = to_json(&m.mark)?;
        marks.push((m.start, m.end, mark_key(&json), json));
    }
    let mut objects = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if *c == OBJECT_CHAR {
            cuts.push(i);
            cuts.push(i + 1);
            objects.push(i);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut segs = Vec::with_capacity(cuts.len());
    let mut run = 0;
    let mut object = 0;
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b || b > len {
            continue;
        }
        let mut attrs = Attrs::new();
        while run < runs.len() && runs[run].1 <= a {
            run += 1;
        }
        if let Some((_, _, fields)) = runs.get(run) {
            for (k, v) in fields {
                attrs.insert(format!("{RUN}{k}"), v.clone());
            }
        }
        for (s, e, key, json) in &marks {
            if *s <= a && *e >= b {
                attrs.insert(key.clone(), json.clone());
            }
        }
        if chars[a] == OBJECT_CHAR {
            while object < objects.len() && objects[object] < a {
                object += 1;
            }
            if let Some(o) = content.objects().get(object) {
                attrs.insert(OBJECT.to_owned(), to_json(o)?);
            }
        }
        push_seg(&mut segs, b - a, attrs);
    }
    Ok((chars, segs))
}

fn push_seg(segs: &mut Vec<Seg>, len: usize, attrs: Attrs) {
    if len == 0 {
        return;
    }
    match segs.last_mut() {
        Some(last) if last.attrs == attrs && !attrs.contains_key(OBJECT) => last.len += len,
        _ => segs.push(Seg { len, attrs }),
    }
}

fn attr_string(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        LoroValue::Null => None,
        other => Some(format!("{other:?}")),
    }
}

/// A `LoroText`'s characters and formatting stretches.
pub(crate) fn loro_segments(text: &LoroText) -> (Vec<char>, Vec<Seg>) {
    let mut chars = Vec::new();
    let mut segs = Vec::new();
    for d in text.to_delta() {
        if let TextDelta::Insert { insert, attributes } = d {
            let before = chars.len();
            chars.extend(insert.chars());
            let attrs: Attrs = attributes
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| attr_string(&v).map(|v| (k, v)))
                .collect();
            // Object characters always stand alone, even when Loro joins equal neighbours.
            if attrs.contains_key(OBJECT) {
                for _ in before..chars.len() {
                    segs.push(Seg {
                        len: 1,
                        attrs: attrs.clone(),
                    });
                }
            } else {
                push_seg(&mut segs, chars.len() - before, attrs);
            }
        }
    }
    (chars, segs)
}

/// Paragraph content from characters and formatting stretches. An object character whose
/// object was lost in a merge is dropped (the next local sync removes it from the CRDT too).
pub(crate) fn content_from(chars: &[char], segs: &[Seg]) -> Result<Inlines> {
    let mut frag = Fragment::default();
    let mut props_cache: HashMap<Vec<(String, String)>, RunProps> = HashMap::new();
    let mut open: BTreeMap<String, (usize, Mark)> = BTreeMap::new();
    let mut at = 0;
    let mut out = 0;
    for seg in segs {
        let run_fields: Vec<(String, String)> = seg
            .attrs
            .iter()
            .filter_map(|(k, v)| k.strip_prefix(RUN).map(|f| (f.to_owned(), v.clone())))
            .collect();
        let props = match props_cache.get(&run_fields) {
            Some(p) => p.clone(),
            None => {
                let mut obj = Map::new();
                for (f, v) in &run_fields {
                    obj.insert(f.clone(), serde_json::from_str(v)?);
                }
                let p: RunProps = serde_json::from_value(Value::Object(obj))?;
                props_cache.insert(run_fields, p.clone());
                p
            }
        };
        let object: Option<InlineObject> = match seg.attrs.get(OBJECT) {
            Some(json) => Some(from_json(json)?),
            None => None,
        };
        let start = out;
        for c in chars.iter().skip(at).take(seg.len) {
            if *c == OBJECT_CHAR {
                let Some(o) = &object else { continue };
                frag.objects.push(o.clone());
            }
            frag.text.push(*c);
            out += 1;
        }
        at += seg.len;
        if out > start {
            frag.runs.push(RunSpan::new(out - start, props));
        }
        // Close marks this stretch does not continue, open the ones it starts.
        let keys: Vec<String> = open.keys().cloned().collect();
        for k in keys {
            if !seg.attrs.contains_key(&k)
                && let Some((s, m)) = open.remove(&k)
                && start > s
            {
                frag.marks.push(MarkSpan::new(s, start, m));
            }
        }
        for (k, v) in &seg.attrs {
            if k.starts_with(MARK) && !open.contains_key(k) {
                open.insert(k.clone(), (start, from_json(v)?));
            }
        }
    }
    for (s, m) in open.into_values() {
        if out > s {
            frag.marks.push(MarkSpan::new(s, out, m));
        }
    }
    Ok(Inlines::from_fragment(frag)?)
}

/// The content a `LoroText` holds.
pub(crate) fn decode_text(text: &LoroText) -> Result<Inlines> {
    let (chars, segs) = loro_segments(text);
    content_from(&chars, &segs)
}

fn common_prefix(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

fn common_suffix(a: &[char], b: &[char], limit: usize) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take(limit)
        .take_while(|(x, y)| x == y)
        .count()
}

/// Makes a `LoroText` hold a paragraph's content: one delete and one insert around the
/// common prefix and suffix, then marks where the formatting differs.
pub(crate) fn sync_text(text: &LoroText, content: &Inlines) -> Result<()> {
    let (mchars, msegs) = model_segments(content)?;
    let lchars: Vec<char> = text.to_string().chars().collect();
    if lchars != mchars {
        let p = common_prefix(&lchars, &mchars);
        let s = common_suffix(&lchars, &mchars, lchars.len().min(mchars.len()) - p);
        let removed = lchars.len() - p - s;
        if removed > 0 {
            text.delete(p, removed)?;
        }
        if mchars.len() > p + s {
            let inserted: String = mchars[p..mchars.len() - s].iter().collect();
            text.insert(p, &inserted)?;
        }
    }
    let (_, lsegs) = loro_segments(text);
    if lsegs != msegs {
        format_diff(text, &lsegs, &msegs)?;
    }
    Ok(())
}

/// What one key should become over a range: a value or unset.
type Want = Option<String>;

/// Marks and unmarks a `LoroText` (formatting `have`) until it has formatting `want`; both
/// cover the same characters.
pub(crate) fn format_diff(text: &LoroText, have: &[Seg], want: &[Seg]) -> Result<()> {
    let mut open: BTreeMap<String, (Want, usize, usize)> = BTreeMap::new();
    let flush = |key: &str, (value, s, e): (Want, usize, usize)| -> Result<()> {
        match value {
            Some(v) => text.mark(s..e, key, v)?,
            None => text.unmark(s..e, key)?,
        }
        Ok(())
    };
    let (mut i, mut j) = (0, 0);
    let (mut left_h, mut left_w) = (
        have.first().map_or(0, |s| s.len),
        want.first().map_or(0, |s| s.len),
    );
    let mut pos = 0;
    while i < have.len() && j < want.len() {
        let n = left_h.min(left_w);
        let (h, w) = (&have[i].attrs, &want[j].attrs);
        let mut here: BTreeMap<&str, Want> = BTreeMap::new();
        if h != w {
            for (k, v) in w {
                if h.get(k) != Some(v) {
                    here.insert(k, Some(v.clone()));
                }
            }
            for k in h.keys() {
                if !w.contains_key(k) {
                    here.insert(k, None);
                }
            }
        }
        // Ranges that stop here are written; the ones that continue grow.
        let stale: Vec<String> = open
            .iter()
            .filter(|(k, (v, _, e))| *e != pos || here.get(k.as_str()) != Some(v))
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            if let Some(r) = open.remove(&k) {
                flush(&k, r)?;
            }
        }
        for (k, v) in here {
            open.entry(k.to_owned())
                .and_modify(|r| r.2 = pos + n)
                .or_insert((v, pos, pos + n));
        }
        pos += n;
        left_h -= n;
        left_w -= n;
        if left_h == 0 {
            i += 1;
            left_h = have.get(i).map_or(0, |s| s.len);
        }
        if left_w == 0 {
            j += 1;
            left_w = want.get(j).map_or(0, |s| s.len);
        }
    }
    for (k, r) in std::mem::take(&mut open) {
        flush(&k, r)?;
    }
    Ok(())
}

/// A paragraph's characters with, for each, its run and object, to compare formatting
/// character by character without converting it.
struct CharIndex<'a> {
    content: &'a Inlines,
    chars: Vec<char>,
    run: Vec<usize>,
    object: Vec<Option<usize>>,
}

impl<'a> CharIndex<'a> {
    fn new(content: &'a Inlines) -> Self {
        let chars: Vec<char> = content.rope().chars().collect();
        let mut run = Vec::with_capacity(chars.len());
        for (k, r) in content.runs().iter().enumerate() {
            run.extend(std::iter::repeat_n(k, r.len));
        }
        run.resize(chars.len(), content.runs().len().saturating_sub(1));
        let mut next = 0;
        let object = chars
            .iter()
            .map(|c| {
                (*c == OBJECT_CHAR).then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        Self {
            content,
            chars,
            run,
            object,
        }
    }

    fn marks_at(&self, i: usize) -> impl Iterator<Item = &Mark> {
        self.content
            .marks()
            .iter()
            .filter(move |m| m.start <= i && i < m.end)
            .map(|m| &m.mark)
    }

    /// Whether character `i` here and character `j` of `other` are the same character with the
    /// same formatting.
    fn same(&self, i: usize, other: &CharIndex<'_>, j: usize) -> bool {
        self.chars[i] == other.chars[j]
            && self.content.runs().get(self.run[i]).map(|r| &r.props)
                == other.content.runs().get(other.run[j]).map(|r| &r.props)
            && self.object[i].and_then(|k| self.content.objects().get(k))
                == other.object[j].and_then(|k| other.content.objects().get(k))
            && self.marks_at(i).eq(other.marks_at(j))
    }
}

/// The value a text-mark change gives a key: a value, or unset (`null`).
fn change_value(v: &LoroValue) -> Option<String> {
    attr_string(v)
}

/// Applies a Loro text change (the delta of an event) to a model paragraph, one op at a time
/// through `apply`. Fails when the change does not fit the paragraph; the caller then rebuilds
/// the paragraph's content from the CRDT.
pub(crate) fn apply_delta(
    doc: &mut Document,
    para: NodeId,
    delta: &[TextDelta],
    apply: &mut impl FnMut(&mut Document, Op) -> Result<()>,
) -> Result<()> {
    let content = |doc: &Document| -> Result<Inlines> {
        doc.paragraph(para)
            .map(|p| p.content.clone())
            .ok_or_else(|| corrupt("a text change for a paragraph the model lacks"))
    };
    let mut pos = 0;
    for d in delta {
        match d {
            TextDelta::Retain {
                retain,
                attributes: None,
            } => pos += retain,
            TextDelta::Retain {
                retain,
                attributes: Some(attrs),
            } => {
                let changes: Vec<(String, Option<String>)> = attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), change_value(v)))
                    .collect();
                for op in format_ops(para, &content(doc)?, pos, *retain, &changes)? {
                    apply(doc, op)?;
                }
                pos += retain;
            }
            TextDelta::Insert { insert, attributes } => {
                let chars: Vec<char> = insert.chars().collect();
                let attrs: Attrs = attributes
                    .iter()
                    .flatten()
                    .filter_map(|(k, v)| attr_string(v).map(|v| (k.clone(), v)))
                    .collect();
                let len = chars.len();
                let inserted = content_from(&chars, &[Seg { len, attrs }])?;
                if inserted.len() != len {
                    return Err(corrupt("an object character without its object"));
                }
                apply(
                    doc,
                    Op::InsertInline {
                        para,
                        offset: pos,
                        fragment: inserted.slice(0, len)?,
                    },
                )?;
                pos += len;
            }
            TextDelta::Delete { delete } => {
                apply(
                    doc,
                    Op::DeleteInline {
                        para,
                        offset: pos,
                        len: *delete,
                    },
                )?;
            }
        }
    }
    Ok(())
}

/// The ops of a formatting change over `pos..pos + len`: run property fields set or unset,
/// marks added or removed, objects replaced.
fn format_ops(
    para: NodeId,
    content: &Inlines,
    pos: usize,
    len: usize,
    changes: &[(String, Option<String>)],
) -> Result<Vec<Op>> {
    let end = pos + len;
    let mut ops = Vec::new();
    let fields: Vec<(&str, &Option<String>)> = changes
        .iter()
        .filter_map(|(k, v)| k.strip_prefix(RUN).map(|f| (f, v)))
        .collect();
    if !fields.is_empty() {
        let mut runs = Vec::new();
        let mut at = 0;
        for r in content.runs() {
            let (s, e) = (at.max(pos), (at + r.len).min(end));
            if s < e {
                let mut obj = match serde_json::to_value(&r.props)? {
                    Value::Object(o) => o,
                    _ => Map::new(),
                };
                for (f, v) in &fields {
                    match v {
                        Some(json) => {
                            obj.insert((*f).to_owned(), serde_json::from_str(json)?);
                        }
                        None => {
                            obj.remove(*f);
                        }
                    }
                }
                runs.push(RunSpan::new(
                    e - s,
                    serde_json::from_value(Value::Object(obj))?,
                ));
            }
            at += r.len;
        }
        ops.push(Op::SetRuns {
            para,
            offset: pos,
            runs,
        });
    }
    for (k, v) in changes {
        if !k.starts_with(MARK) {
            continue;
        }
        match v {
            Some(json) => ops.push(Op::AddMark {
                para,
                start: pos,
                end,
                mark: from_json(json)?,
            }),
            None => {
                for m in content.marks() {
                    if m.start < end && m.end > pos && mark_key(&to_json(&m.mark)?) == *k {
                        ops.push(Op::RemoveMark {
                            para,
                            start: pos,
                            end,
                            mark: m.mark.clone(),
                        });
                        break;
                    }
                }
            }
        }
    }
    if let Some((_, v)) = changes.iter().find(|(k, _)| k == OBJECT) {
        let Some(json) = v else {
            return Err(corrupt("an object removed from its character"));
        };
        let object: InlineObject = from_json(json)?;
        for (offset, _) in content.objects_with_offsets() {
            if offset >= pos && offset < end {
                ops.push(Op::SetObject {
                    para,
                    offset,
                    object: object.clone(),
                });
            }
        }
    }
    Ok(ops)
}

/// The ops that turn a paragraph's content `old` into `new`: formatting ops when only the
/// formatting changed, otherwise one delete and one insert around the unchanged ends.
pub(crate) fn content_ops(para: NodeId, old: &Inlines, new: &Inlines) -> Result<Vec<Op>> {
    if old == new {
        return Ok(Vec::new());
    }
    let mut ops = Vec::new();
    if old.rope() == new.rope() {
        if old.runs() != new.runs() {
            ops.push(Op::SetRuns {
                para,
                offset: 0,
                runs: new.runs().to_vec(),
            });
        }
        if old.marks() != new.marks() {
            ops.push(Op::SetMarks {
                para,
                marks: new.marks().to_vec(),
            });
        }
        for ((offset, a), b) in old.objects_with_offsets().zip(new.objects()) {
            if a != b {
                ops.push(Op::SetObject {
                    para,
                    offset,
                    object: b.clone(),
                });
            }
        }
        return Ok(ops);
    }
    let (a, b) = (CharIndex::new(old), CharIndex::new(new));
    let (oc, nc) = (&a.chars, &b.chars);
    let mut p = 0;
    while p < oc.len() && p < nc.len() && a.same(p, &b, p) {
        p += 1;
    }
    let mut s = 0;
    while s < oc.len() - p && s < nc.len() - p && a.same(oc.len() - 1 - s, &b, nc.len() - 1 - s) {
        s += 1;
    }
    if oc.len() > p + s {
        ops.push(Op::DeleteInline {
            para,
            offset: p,
            len: oc.len() - p - s,
        });
    }
    if nc.len() > p + s {
        ops.push(Op::InsertInline {
            para,
            offset: p,
            fragment: new.slice(p, nc.len() - s)?,
        });
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::{Hyperlink, LinkTarget};

    fn bold() -> RunProps {
        RunProps {
            bold: Some(true),
            ..RunProps::default()
        }
    }

    fn link() -> Mark {
        Mark::Link(Hyperlink {
            target: LinkTarget::Url("https://kiln.example".into()),
            tooltip: None,
            frame: None,
            history: true,
        })
    }

    fn sample() -> Result<Inlines> {
        let mut c =
            Inlines::from_fragment(Fragment::text("Hello bold world", RunProps::default()))?;
        c.set_runs(6, vec![RunSpan::new(4, bold())])?;
        c.add_mark(0, 10, link())?;
        Ok(c)
    }

    #[test]
    fn content_round_trips_through_loro_text() -> Result<()> {
        let doc = loro::LoroDoc::new();
        doc.config_default_text_style(Some(loro::StyleConfig {
            expand: loro::ExpandType::None,
        }));
        let text = doc.get_text("t");
        let content = sample()?;
        sync_text(&text, &content)?;
        assert_eq!(decode_text(&text)?, content);
        // Typing inside the bold link without formatting: the new text must not inherit.
        let mut typed = content.clone();
        typed.insert(8, Fragment::text("XY", RunProps::default()))?;
        sync_text(&text, &typed)?;
        assert_eq!(decode_text(&text)?, typed);
        Ok(())
    }

    #[test]
    fn content_ops_rebuild_the_new_content() -> Result<()> {
        let old = sample()?;
        let mut new = old.clone();
        new.insert(3, Fragment::text("--", bold()))?;
        new.remove(12, 14)?;
        let ops = content_ops(NodeId(1), &old, &new)?;
        assert_eq!(ops.len(), 2);
        let mut fmt = old.clone();
        fmt.set_runs(0, vec![RunSpan::new(3, bold())])?;
        let ops = content_ops(NodeId(1), &old, &fmt)?;
        assert!(matches!(ops.as_slice(), [Op::SetRuns { .. }]));
        Ok(())
    }
}
