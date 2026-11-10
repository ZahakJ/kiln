// SPDX-License-Identifier: AGPL-3.0-or-later
//! The autosave journal of an open document.
//!
//! A journal is a few values in the app storage's `Journal` area, under the document's journal
//! key `k`:
//!
//! - `k.meta`: [`JournalMeta`] as JSON: the journal format, kind, title, file and format, the
//!   *dirty marker* (`dirty: true` while the journal holds work that is not in the file) and
//!   the generation of the checkpoint the journal stands on.
//! - `k.base.<g>`: checkpoint `g`, the document as a native bundle behind a small header with
//!   the generation (`KJCP`, format, generation).
//! - `k.log.<g>`: the operations applied since checkpoint `g`, appended one record per command:
//!   `[length u32][crc32 u32][postcard Record]`, little-endian. Each record names its
//!   generation too.
//!
//! The first edit after opening (or a clean checkpoint taken soon after a file opens) starts
//! the journal; every command that changes the document appends a record; after
//! [`CHECKPOINT_OPS`] operations a new checkpoint is written, on top of the record that reached
//! the count.
//!
//! A checkpoint never touches what the marker names. It starts the next log, `k.log.<g+1>`, at
//! once, so records never wait for it and go on being written while it is under way; then it
//! writes `k.base.<g+1>`; only when that is on the disk does the marker move to `g + 1` and the
//! values of generation `g` go. The marker is the switch, and the logs chain: a crash before the
//! switch leaves `k.base.<g>`, `k.log.<g>` and `k.log.<g+1>`, which [`recover`] reads in that
//! order; a crash after it leaves `k.base.<g+1>` and `k.log.<g+1>`. Either way each record is
//! applied once, on top of the document it followed. A checkpoint small enough for a host's
//! write-ahead copy ([`INLINE_CHECKPOINT_BYTES`]) goes as one run of writes instead, which that
//! copy replays whole, so a change without operations is as safe as a record. Values of other
//! generations are leftovers, deleted by the next recovery scan. The hosts run the writes of one
//! journal in the order given. A crash in the middle of an append leaves a short or damaged last
//! record, which [`recover`] drops (it keeps every record before it); the session writes a
//! checkpoint as soon as it adopts a recovered journal, so later records never follow a damaged
//! one. Saving clears the marker (when nothing changed while the file was written); closing a
//! document cleanly removes its journal (the marker first, so a journal is never listed
//! half-removed).
//!
//! - `k.draft`: text the user is typing that is not part of the document yet (a cell of Kiln Grid
//!   left open in its editor), in the model's own encoding. It is written with each change of the
//!   text and goes when the entry ends. It belongs to the journal, not to a generation.
//! - `k.unkept`: present while a change that has no operations waits for a large checkpoint, or
//!   any change waits for the first one to land. A crash before it lands loses that change, and
//!   the recovered document says so.
//!
//! Journals of the first format (`k.base` and `k.log` without a generation, the generation in
//! the checkpoint's header) are read as before.
//!
//! What the hosts do to make this durable is their fsync policy: natively
//! [`crate::NativeStorage`] flushes every write to the disk before the next one starts; on the
//! web each write goes to its own file in the Origin Private File System, with a copy in the
//! page's write-ahead tail until it has finished (`apps/web/src/host/store.ts`).

use kiln_common::{KilnError, Result};
use kiln_protocol::{DocKind, FileFormat, HostOp, StoreArea};
use serde::{Deserialize, Serialize};

/// The journal format this version writes and the newest it reads. 4: records are numbered and can
/// name a change they do not hold ([`Record`]); 3: Kiln Grid's records are `GridOp`s (format 2:
/// bare sheet commands), and logs chain across a checkpoint under way. A change to the encoding
/// of any model's operations must raise it: a journal of a newer format is listed and refused,
/// never read.
pub const JOURNAL_FORMAT: u32 = 4;
/// The first format whose records are numbered.
const NUMBERED: u32 = 4;
/// The largest checkpoint written as one run of writes (see the module documentation). A host's
/// write-ahead copy holds at least this much.
pub const INLINE_CHECKPOINT_BYTES: usize = 256 * 1024;
/// Operations between checkpoints.
pub const CHECKPOINT_OPS: u32 = 200;

const CHECKPOINT_MAGIC: &[u8; 4] = b"KJCP";
/// Largest record the reader accepts (a paste of a large picture is one operation).
const MAX_RECORD: usize = 256 << 20;

/// `k.meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalMeta {
    /// The journal format ([`JOURNAL_FORMAT`] when written by this Kiln).
    pub format: u32,
    /// The journal key.
    pub key: String,
    /// The kind of document.
    pub kind: DocKind,
    /// Its title.
    pub title: String,
    /// The file it was opened from or saved to.
    #[serde(default)]
    pub path: Option<String>,
    /// That file's format.
    #[serde(default)]
    pub file_format: Option<FileFormat>,
    /// When the journal was started.
    pub created_ms: u64,
    /// When the marker or checkpoint was last written.
    pub saved_at_ms: u64,
    /// The journal holds work that is not in the file.
    pub dirty: bool,
    /// The checkpoint the journal stands on (`k.base.<generation>`, `k.log.<generation>`);
    /// 0 for a journal of the first format (`k.base`, `k.log`).
    #[serde(default)]
    pub generation: u64,
    /// The fingerprint of the file's contents as Kiln last opened or saved them (a recovered
    /// document finds its local version history with it).
    #[serde(default)]
    pub print: Option<String>,
    /// The number of the first record after the checkpoint the marker names.
    #[serde(default)]
    pub seq: u64,
}

/// One record of a log: the operations one command applied, or a change it could not hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The checkpoint the operations follow.
    pub generation: u64,
    /// When they were applied.
    pub time_ms: u64,
    /// The record's number: one more than the record before it, across logs. A number missing
    /// is a record that never reached the disk.
    pub seq: u64,
    /// The operations, each in the model's own encoding.
    pub ops: Vec<Vec<u8>>,
    /// A change made here that no operation can replay (others' edits, a setting a model keeps
    /// outside its operations), by name: what follows it cannot be replayed either.
    pub missing: Option<String>,
}

/// A record of the formats before [`NUMBERED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordBefore4 {
    /// The checkpoint the operations follow.
    pub generation: u64,
    /// When they were applied.
    pub time_ms: u64,
    /// The operations.
    pub ops: Vec<Vec<u8>>,
}

/// The marker's key.
#[must_use]
pub fn meta_key(key: &str) -> String {
    format!("{key}.meta")
}

/// The key of checkpoint `generation` (0: the first format's `k.base`).
#[must_use]
pub fn base_key(key: &str, generation: u64) -> String {
    if generation == 0 {
        format!("{key}.base")
    } else {
        format!("{key}.base.{generation}")
    }
}

/// The key of the log after checkpoint `generation` (0: the first format's `k.log`).
#[must_use]
pub fn log_key(key: &str, generation: u64) -> String {
    if generation == 0 {
        format!("{key}.log")
    } else {
        format!("{key}.log.{generation}")
    }
}

/// The key of the text being typed.
#[must_use]
pub fn draft_key(key: &str) -> String {
    format!("{key}.draft")
}

/// The key that says a change is waiting for its checkpoint.
#[must_use]
pub fn unkept_key(key: &str) -> String {
    format!("{key}.unkept")
}

/// The generation of a log's key, for a key of journal `key`.
fn log_generation(key: &str, of: &str) -> Option<u64> {
    let rest = of.strip_prefix(key)?.strip_prefix(".log")?;
    if rest.is_empty() {
        return Some(0);
    }
    rest.strip_prefix('.')?.parse().ok()
}

/// What `k.unkept` says of a journal that has no marker: its title and kind.
#[must_use]
pub fn unkept_about(bytes: &[u8]) -> Option<(String, DocKind)> {
    #[derive(Deserialize)]
    struct About {
        title: String,
        kind: DocKind,
    }
    let a: About = serde_json::from_slice(bytes).ok()?;
    Some((a.title, a.kind))
}

/// The journal a key of the `Journal` area belongs to (`k.base.3` → `k`).
#[must_use]
pub fn journal_of(key: &str) -> &str {
    key.split_once('.').map_or(key, |(j, _)| j)
}

/// Whether `key` is one of a journal's logs.
#[must_use]
pub fn is_log(key: &str) -> bool {
    key.split_once('.')
        .is_some_and(|(_, rest)| rest == "log" || rest.starts_with("log."))
}

/// The journals among the keys of the `Journal` area (those with a marker), and the keys of
/// leftovers without one (a removal a crash interrupted, or a journal whose first checkpoint
/// was cut short), which can be deleted.
#[must_use]
pub fn journals(keys: &[String]) -> (Vec<String>, Vec<String>) {
    let marked: Vec<String> = keys
        .iter()
        .filter_map(|k| k.strip_suffix(".meta").map(str::to_owned))
        .collect();
    let leftovers = keys
        .iter()
        .filter(|k| !k.ends_with(".meta") && !marked.iter().any(|m| m == journal_of(k)))
        .cloned()
        .collect();
    (marked, leftovers)
}

/// The keys of journal `key` among `keys`, the marker first (the order to remove them in).
#[must_use]
pub fn keys_of(key: &str, keys: &[String]) -> Vec<String> {
    let mut out = vec![meta_key(key)];
    out.extend(
        keys.iter()
            .filter(|k| journal_of(k) == key && !k.ends_with(".meta"))
            .cloned(),
    );
    out
}

/// The keys of journal `key` among `keys` that [`recover`] does not read with the marker at
/// `generation`: leftovers of a checkpoint a crash interrupted.
#[must_use]
pub fn stale_keys(key: &str, generation: u64, keys: &[String]) -> Vec<String> {
    let live = [
        meta_key(key),
        base_key(key, generation),
        draft_key(key),
        unkept_key(key),
    ];
    keys.iter()
        .filter(|k| {
            journal_of(k) == key
                && !live.contains(k)
                && !log_generation(key, k).is_some_and(|g| g >= generation)
        })
        .cloned()
        .collect()
}

/// Encodes a checkpoint: the header, then the bundle.
#[must_use]
pub fn encode_checkpoint(generation: u64, bundle: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bundle.len() + 16);
    out.extend_from_slice(CHECKPOINT_MAGIC);
    out.extend_from_slice(&JOURNAL_FORMAT.to_le_bytes());
    out.extend_from_slice(&generation.to_le_bytes());
    out.extend_from_slice(bundle);
    out
}

/// Decodes a checkpoint into its generation and bundle.
///
/// # Errors
///
/// `Format` for bytes that are not a checkpoint or one of a newer journal format.
pub fn decode_checkpoint(bytes: &[u8]) -> Result<(u64, &[u8])> {
    let rest = bytes
        .strip_prefix(CHECKPOINT_MAGIC.as_slice())
        .ok_or_else(|| damaged("the checkpoint is not a Kiln journal checkpoint"))?;
    let (format, rest) = rest
        .split_first_chunk::<4>()
        .ok_or_else(|| damaged("the checkpoint is cut short"))?;
    let format = u32::from_le_bytes(*format);
    if format > JOURNAL_FORMAT {
        return Err(newer(format));
    }
    let (generation, rest) = rest
        .split_first_chunk::<8>()
        .ok_or_else(|| damaged("the checkpoint is cut short"))?;
    Ok((u64::from_le_bytes(*generation), rest))
}

/// Encodes one record for the log.
///
/// # Errors
///
/// `Internal` if postcard cannot encode it.
pub fn encode_record(record: &Record) -> Result<Vec<u8>> {
    frame(postcard::to_allocvec(record).map_err(|e| KilnError::Internal(e.to_string()))?)
}

/// Encodes a record as the formats before [`NUMBERED`] wrote it (tests of old journals).
///
/// # Errors
///
/// `Internal` if postcard cannot encode it.
pub fn encode_record_before4(record: &RecordBefore4) -> Result<Vec<u8>> {
    frame(postcard::to_allocvec(record).map_err(|e| KilnError::Internal(e.to_string()))?)
}

fn frame(payload: Vec<u8>) -> Result<Vec<u8>> {
    let len = u32::try_from(payload.len())
        .map_err(|_| KilnError::InvalidInput("a journal record over 4 GiB".into()))?;
    let mut out = Vec::with_capacity(payload.len() + 8);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// What a log holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogRead {
    /// The whole records, in order.
    pub records: Vec<Record>,
    /// Bytes after the last whole record that were dropped (a record a crash cut short or
    /// damaged).
    pub dropped: usize,
}

/// Reads a log of journal format `format`, keeping every whole record up to the first short or
/// damaged one.
#[must_use]
pub fn decode_log(mut bytes: &[u8], format: u32) -> LogRead {
    let mut out = LogRead::default();
    while !bytes.is_empty() {
        let Some(record) = next_record(bytes, format) else {
            out.dropped = bytes.len();
            break;
        };
        let (record, used) = record;
        out.records.push(record);
        bytes = &bytes[used..];
    }
    out
}

fn next_record(bytes: &[u8], format: u32) -> Option<(Record, usize)> {
    let (len, rest) = bytes.split_first_chunk::<4>()?;
    let (crc, rest) = rest.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    if len > MAX_RECORD || rest.len() < len {
        return None;
    }
    let payload = &rest[..len];
    if crc32fast::hash(payload) != u32::from_le_bytes(*crc) {
        return None;
    }
    let record = if format >= NUMBERED {
        postcard::from_bytes(payload).ok()?
    } else {
        let r: RecordBefore4 = postcard::from_bytes(payload).ok()?;
        Record {
            generation: r.generation,
            time_ms: r.time_ms,
            seq: 0,
            ops: r.ops,
            missing: None,
        }
    };
    Some((record, 8 + len))
}

/// A journal read back after a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    /// The marker.
    pub meta: JournalMeta,
    /// The checkpoint's generation.
    pub generation: u64,
    /// The checkpoint: the document as a native bundle.
    pub bundle: Vec<u8>,
    /// The operations applied since, in order.
    pub ops: Vec<Vec<u8>>,
    /// When the journal was last written.
    pub saved_at_ms: u64,
    /// Bytes of a last record that was cut short and dropped.
    pub dropped: usize,
    /// Records of another checkpoint that were skipped.
    pub stale: usize,
    /// The text that was being typed.
    pub draft: Option<Vec<u8>>,
    /// The last generation of the logs read.
    pub last_generation: u64,
    /// The journal format the marker names, which is the encoding of the operations.
    pub format: u32,
    /// A change without operations was waiting for its checkpoint, and is not in `bundle`.
    pub unkept: bool,
    /// Where the replay stops short of the end, and why.
    pub stopped: Option<Stopped>,
    /// The number the next record gets.
    pub next_seq: u64,
}

/// Why a journal's replay stops before its last record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    /// The change that was not kept, by name; `None` for a record that never reached the disk.
    pub missing: Option<String>,
    /// When it was made (0: not known).
    pub time_ms: u64,
    /// The records after it, which were not replayed either.
    pub after: usize,
}

/// Reads a marker.
///
/// # Errors
///
/// `Format` for a marker that does not parse or one of a newer journal format.
pub fn decode_meta(bytes: &[u8]) -> Result<JournalMeta> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| damaged(&format!("the marker: {e}")))?;
    let format = value
        .get("format")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| damaged("the marker has no format"))?;
    if format > u64::from(JOURNAL_FORMAT) {
        return Err(newer(u32::try_from(format).unwrap_or(u32::MAX)));
    }
    serde_json::from_value(value).map_err(|e| damaged(&format!("the marker: {e}")))
}

/// Reads a journal back: the checkpoint the marker names, then the records of its generation
/// (records of another generation are skipped; a record a crash cut short is dropped with
/// everything after it). `get` gives the value of a key of the journal.
///
/// # Errors
///
/// `Format` for a missing or damaged marker or checkpoint, or a journal of a newer format.
pub fn recover<'a>(meta: &[u8], get: impl Fn(&str) -> Option<&'a [u8]>) -> Result<Recovered> {
    let meta = decode_meta(meta)?;
    let base = get(&base_key(&meta.key, meta.generation))
        .ok_or_else(|| damaged("the journal has no checkpoint"))?;
    let (generation, bundle) = decode_checkpoint(base)?;
    if meta.generation != 0 && generation != meta.generation {
        return Err(damaged("the checkpoint is not the one the marker names"));
    }
    let format = meta.format;
    let read = decode_log(
        get(&log_key(&meta.key, meta.generation)).unwrap_or_default(),
        format,
    );
    let draft = get(&draft_key(&meta.key)).map(<[u8]>::to_vec);
    let unkept = get(&unkept_key(&meta.key)).is_some();
    let mut dropped = read.dropped;
    let mut records: Vec<Record> = Vec::new();
    let mut stale = 0;
    let mut keep = |list: Vec<Record>, generation: u64| {
        for r in list {
            if r.generation == generation {
                records.push(r);
            } else {
                stale += 1;
            }
        }
    };
    keep(read.records, generation);
    // The logs of checkpoints that were under way go on from this one.
    let mut last_generation = generation;
    while let Some(log) = get(&log_key(&meta.key, last_generation + 1)) {
        last_generation += 1;
        let read = decode_log(log, format);
        dropped += read.dropped;
        keep(read.records, last_generation);
    }
    let numbered = format >= NUMBERED;
    let mut saved_at_ms = meta.saved_at_ms;
    let mut ops = Vec::new();
    let mut stopped = None;
    let mut expected = meta.seq;
    let total = records.len();
    for (i, r) in records.into_iter().enumerate() {
        let gap = numbered && r.seq != expected;
        if gap || r.missing.is_some() {
            stopped = Some(Stopped {
                missing: if gap { None } else { r.missing },
                time_ms: r.time_ms,
                after: total - i - usize::from(!gap),
            });
            break;
        }
        expected += 1;
        saved_at_ms = saved_at_ms.max(r.time_ms);
        ops.extend(r.ops);
    }
    // The next record follows the last one on the disk, whatever was replayed.
    let next_seq = expected + stopped.as_ref().map_or(0, |s| s.after as u64 + 1);
    Ok(Recovered {
        meta,
        generation,
        bundle: bundle.to_vec(),
        ops,
        saved_at_ms,
        dropped,
        stale,
        draft,
        last_generation,
        format,
        unkept,
        stopped,
        next_seq,
    })
}

/// The journal of one open document, as the session writes it: each call returns the app
/// storage operations to perform, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journal {
    meta: JournalMeta,
    /// The generation of the log being written (for a journal of the first format, until its
    /// next checkpoint, the one in its checkpoint's header).
    generation: u64,
    /// Every log on the storage, oldest first.
    logs: Vec<u64>,
    /// The checkpoint under way, and the number of the first record after it.
    writing: Option<(u64, u64)>,
    /// The number the next record gets.
    seq: u64,
    /// A change is not in the journal: the log it happened in (a checkpoint after that log
    /// started covers it).
    unkept_in: Option<u64>,
    /// Checkpoints whose write failed.
    orphans: Vec<u64>,
    /// `k.meta` exists: the first checkpoint has landed.
    marked: bool,
    /// `k.unkept` exists.
    unkept: bool,
    /// A checkpoint up to this size is written as one run of writes.
    inline_bytes: usize,
    ops: u32,
    started: bool,
    /// `k.draft` exists.
    has_draft: bool,
    /// The marker is dirty only because of the draft: the document itself has not changed.
    draft_marked: bool,
}

impl Journal {
    /// A journal not written yet.
    #[must_use]
    pub fn new(meta: JournalMeta) -> Self {
        Self {
            meta,
            generation: 0,
            logs: Vec::new(),
            writing: None,
            seq: 0,
            unkept_in: None,
            orphans: Vec::new(),
            marked: false,
            unkept: false,
            inline_bytes: INLINE_CHECKPOINT_BYTES,
            ops: 0,
            started: false,
            has_draft: false,
            draft_marked: false,
        }
    }

    /// A journal adopted from a recovered one (it goes on under the same key; write a
    /// checkpoint next, so new records never follow a damaged or unreplayed one).
    #[must_use]
    pub fn adopt(recovered: &Recovered) -> Self {
        Self {
            meta: recovered.meta.clone(),
            generation: recovered.last_generation,
            logs: std::iter::once(recovered.meta.generation)
                .chain(recovered.generation + 1..=recovered.last_generation)
                .collect(),
            writing: None,
            seq: recovered.next_seq,
            unkept_in: (recovered.unkept || recovered.stopped.is_some())
                .then_some(recovered.last_generation),
            orphans: Vec::new(),
            marked: true,
            unkept: recovered.unkept,
            inline_bytes: INLINE_CHECKPOINT_BYTES,
            ops: u32::try_from(recovered.ops.len()).unwrap_or(u32::MAX),
            started: true,
            has_draft: recovered.draft.is_some(),
            draft_marked: false,
        }
    }

    /// The key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.meta.key
    }

    /// The marker as it stands.
    #[must_use]
    pub fn meta(&self) -> &JournalMeta {
        &self.meta
    }

    /// Whether the checkpoint was written.
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    /// Whether the marker says the journal holds unsaved work.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.meta.dirty
    }

    /// Operations since the checkpoint.
    #[must_use]
    pub fn ops_since_checkpoint(&self) -> u32 {
        self.ops
    }

    /// Sets the largest checkpoint written as one run of writes.
    pub fn set_inline_bytes(&mut self, bytes: usize) {
        self.inline_bytes = bytes;
    }

    /// The key of the checkpoint under way: its write's completion is
    /// [`Journal::checkpoint_written`].
    #[must_use]
    pub fn writing_key(&self) -> Option<String> {
        self.writing.map(|(g, _)| base_key(&self.meta.key, g))
    }

    /// Whether the next write should be a checkpoint.
    #[must_use]
    pub fn checkpoint_due(&self) -> bool {
        self.ops >= CHECKPOINT_OPS
    }

    /// Starts a checkpoint (`bundle`, the document now) with the marker dirty; the next records
    /// go to a new log. The first one starts the journal. With `unkept`, a change that has no
    /// operations rests on this checkpoint.
    ///
    /// # Errors
    ///
    /// `Internal` if the marker cannot be encoded; `InvalidInput` while another is under way.
    pub fn checkpoint(&mut self, bundle: &[u8], now_ms: u64, unkept: bool) -> Result<Vec<HostOp>> {
        self.write_checkpoint(bundle, now_ms, true, unkept)
    }

    /// Writes the first checkpoint ahead of any change (the document as it was opened, while
    /// nothing else is going on), with the marker clean: the first change then only appends.
    ///
    /// # Errors
    ///
    /// `Internal` if the marker cannot be encoded.
    pub fn prepare(&mut self, bundle: &[u8], now_ms: u64) -> Result<Vec<HostOp>> {
        self.write_checkpoint(bundle, now_ms, false, false)
    }

    /// The next log, then the checkpoint; the marker follows it, at once when the checkpoint is
    /// small and otherwise in [`Journal::checkpoint_written`].
    fn write_checkpoint(
        &mut self,
        bundle: &[u8],
        now_ms: u64,
        dirty: bool,
        unkept: bool,
    ) -> Result<Vec<HostOp>> {
        if self.writing.is_some() {
            return Err(KilnError::InvalidInput("a checkpoint is under way".into()));
        }
        let key = self.meta.key.clone();
        self.generation += 1;
        let generation = self.generation;
        self.logs.push(generation);
        self.ops = 0;
        self.started = true;
        self.meta.dirty = dirty;
        self.draft_marked = self.draft_marked && !dirty;
        let mut out = vec![put(log_key(&key, generation), Vec::new())];
        let inline = bundle.len() <= self.inline_bytes;
        if unkept && !inline {
            out.extend(self.mark_unkept(generation - 1));
        }
        out.push(put(
            base_key(&key, generation),
            encode_checkpoint(generation, bundle),
        ));
        self.writing = Some((generation, self.seq));
        if inline {
            out.extend(self.checkpoint_written(now_ms)?);
        }
        Ok(out)
    }

    /// The checkpoint under way is on the disk: the marker moves to it, then the values before
    /// it go.
    ///
    /// # Errors
    ///
    /// `Internal` if the marker cannot be encoded.
    pub fn checkpoint_written(&mut self, now_ms: u64) -> Result<Vec<HostOp>> {
        let Some((generation, seq)) = self.writing.take() else {
            return Ok(Vec::new());
        };
        let key = self.meta.key.clone();
        let before = self.marked.then_some(self.meta.generation);
        self.meta.format = JOURNAL_FORMAT;
        self.meta.generation = generation;
        self.meta.seq = seq;
        self.meta.saved_at_ms = now_ms;
        self.marked = true;
        let mut out = vec![put(meta_key(&key), self.meta_bytes()?)];
        let delete = |key: String| HostOp::StoreDelete {
            area: StoreArea::Journal,
            key,
        };
        out.extend(before.map(|g| delete(base_key(&key, g))));
        out.extend(
            self.orphans
                .drain(..)
                .filter(|g| *g != generation)
                .map(|g| delete(base_key(&key, g))),
        );
        let (old, kept): (Vec<u64>, Vec<u64>) = self.logs.iter().partition(|g| **g < generation);
        self.logs = kept;
        out.extend(old.into_iter().map(|g| delete(log_key(&key, g))));
        if self.unkept_in.is_none_or(|g| g < generation) {
            self.unkept_in = None;
            if std::mem::take(&mut self.unkept) {
                out.push(delete(unkept_key(&key)));
            }
        }
        Ok(out)
    }

    /// Says, on the disk, that a change made in log `generation` is not in the journal until a
    /// checkpoint after it lands. Its value names the document (`{"title", "kind"}`), for a
    /// recovery that has nothing else to go on.
    fn mark_unkept(&mut self, generation: u64) -> Vec<HostOp> {
        self.unkept_in = Some(self.unkept_in.map_or(generation, |g| g.max(generation)));
        if std::mem::replace(&mut self.unkept, true) {
            return Vec::new();
        }
        let about = serde_json::json!({ "title": self.meta.title, "kind": self.meta.kind });
        vec![put(
            unkept_key(&self.meta.key),
            about.to_string().into_bytes(),
        )]
    }

    /// Whether a change is not in the journal until a checkpoint lands.
    #[must_use]
    pub fn unkept(&self) -> bool {
        self.unkept_in.is_some()
    }

    /// A change that no record can hold (others' edits, a setting outside a model's
    /// operations, a record that could not be written): it goes in the log by name, and a
    /// replay stops before it until a checkpoint after it lands. Ask for that checkpoint.
    ///
    /// # Errors
    ///
    /// `Internal` if the record cannot be encoded; `InvalidInput` before the first checkpoint.
    pub fn missing(&mut self, what: &str, now_ms: u64) -> Result<Vec<HostOp>> {
        if !self.started {
            return Err(KilnError::InvalidInput(
                "the journal has no checkpoint yet".into(),
            ));
        }
        let mut out = self.mark_unkept(self.generation);
        if !self.meta.dirty {
            self.meta.dirty = true;
            self.meta.saved_at_ms = now_ms;
            out.extend(self.marker()?);
        }
        out.push(self.record(Vec::new(), Some(what.to_owned()), now_ms)?);
        Ok(out)
    }

    /// A record of this journal could not be written: what follows it cannot be replayed until
    /// a checkpoint lands. Ask for one.
    pub fn record_lost(&mut self) -> Vec<HostOp> {
        self.mark_unkept(self.generation)
    }

    fn record(
        &mut self,
        ops: Vec<Vec<u8>>,
        missing: Option<String>,
        now_ms: u64,
    ) -> Result<HostOp> {
        let record = Record {
            generation: self.generation,
            time_ms: now_ms,
            seq: self.seq,
            ops,
            missing,
        };
        self.seq += 1;
        Ok(HostOp::StoreAppend {
            area: StoreArea::Journal,
            key: log_key(&self.meta.key, self.generation),
            bytes: encode_record(&record)?,
        })
    }

    /// The checkpoint under way could not be written: what the marker names stays, and the logs
    /// go on from it.
    pub fn checkpoint_failed(&mut self) {
        self.orphans.extend(self.writing.take().map(|(g, _)| g));
    }

    /// Appends the operations one command applied.
    ///
    /// # Errors
    ///
    /// `Internal` if the record cannot be encoded; `InvalidInput` before the first checkpoint.
    pub fn append(&mut self, ops: Vec<Vec<u8>>, now_ms: u64) -> Result<Vec<HostOp>> {
        if !self.started {
            return Err(KilnError::InvalidInput(
                "the journal has no checkpoint yet".into(),
            ));
        }
        let mut out = Vec::new();
        self.draft_marked = false;
        if !self.meta.dirty {
            self.meta.dirty = true;
            self.meta.saved_at_ms = now_ms;
            out.extend(self.marker()?);
        }
        // Until the first checkpoint lands nothing can be recovered: the host says so if asked.
        if !self.marked {
            out.extend(self.mark_unkept(self.generation));
        }
        self.ops = self
            .ops
            .saturating_add(u32::try_from(ops.len()).unwrap_or(u32::MAX));
        out.push(self.record(ops, None, now_ms)?);
        Ok(out)
    }

    /// The text being typed changed (`Some`), or the entry ended (`None`). A document with text
    /// being typed is listed by Document Recovery, so the marker is dirty until it ends.
    ///
    /// # Errors
    ///
    /// `Internal` if the marker cannot be encoded; `InvalidInput` before the first checkpoint.
    pub fn draft(&mut self, text: Option<&[u8]>, now_ms: u64) -> Result<Vec<HostOp>> {
        if !self.started {
            return Err(KilnError::InvalidInput(
                "the journal has no checkpoint yet".into(),
            ));
        }
        let key = draft_key(&self.meta.key);
        let mut out = Vec::new();
        match text {
            Some(text) => {
                if !self.meta.dirty {
                    self.meta.dirty = true;
                    self.draft_marked = true;
                    self.meta.saved_at_ms = now_ms;
                    out.extend(self.marker()?);
                }
                self.has_draft = true;
                out.push(put(key, text.to_vec()));
            }
            None => {
                self.has_draft = false;
                out.push(HostOp::StoreDelete {
                    area: StoreArea::Journal,
                    key,
                });
                if std::mem::take(&mut self.draft_marked) {
                    self.meta.dirty = false;
                    out.extend(self.marker()?);
                }
            }
        }
        Ok(out)
    }

    /// The document was saved; `title`, `path`, `format` and `print` (the fingerprint of what
    /// was written) are the file's now. With `clean`
    /// (the document is still what was written) the marker says the journal holds nothing the
    /// file does not; otherwise edits arrived while the file was written, and the marker stays
    /// dirty with every record kept.
    ///
    /// # Errors
    ///
    /// `Internal` if the marker cannot be encoded.
    pub fn saved(
        &mut self,
        title: &str,
        path: Option<&str>,
        format: Option<FileFormat>,
        print: Option<&str>,
        clean: bool,
        now_ms: u64,
    ) -> Result<Vec<HostOp>> {
        if clean && !self.has_draft {
            self.meta.dirty = false;
        }
        title.clone_into(&mut self.meta.title);
        self.meta.path = path.map(str::to_owned);
        self.meta.file_format = format;
        self.meta.print = print.map(str::to_owned);
        self.meta.saved_at_ms = now_ms;
        self.marker()
    }

    /// The marker as it stands, once there is one: before the first checkpoint lands, that
    /// checkpoint writes it.
    fn marker(&self) -> Result<Vec<HostOp>> {
        if self.marked {
            Ok(vec![put(meta_key(&self.meta.key), self.meta_bytes()?)])
        } else {
            Ok(Vec::new())
        }
    }

    /// Removes the journal (a clean close): the marker first.
    #[must_use]
    pub fn remove(&self) -> Vec<HostOp> {
        let key = &self.meta.key;
        let bases = std::iter::once(self.meta.generation)
            .chain(self.writing.map(|(g, _)| g))
            .chain(self.orphans.iter().copied())
            .map(|g| base_key(key, g));
        let logs = self.logs.iter().map(|g| log_key(key, *g));
        let keys = std::iter::once(meta_key(key))
            .chain(bases)
            .chain(logs)
            .chain([draft_key(key), unkept_key(key)]);
        delete_all(keys.collect::<Vec<_>>())
    }

    fn meta_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&self.meta).map_err(|e| KilnError::Internal(e.to_string()))
    }
}

/// The operations that delete `keys` of the `Journal` area, in order.
#[must_use]
pub fn delete_all(keys: impl IntoIterator<Item = String>) -> Vec<HostOp> {
    keys.into_iter()
        .map(|key| HostOp::StoreDelete {
            area: StoreArea::Journal,
            key,
        })
        .collect()
}

fn put(key: String, bytes: Vec<u8>) -> HostOp {
    HostOp::StorePut {
        area: StoreArea::Journal,
        key,
        bytes,
    }
}

fn damaged(what: &str) -> KilnError {
    KilnError::Format {
        format: "autosave journal".into(),
        message: what.to_owned(),
    }
}

fn newer(format: u32) -> KilnError {
    KilnError::Format {
        format: "autosave journal".into(),
        message: format!(
            "written by a newer version of Kiln (journal format {format}; this version reads up \
             to {JOURNAL_FORMAT})"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MemoryStorage, StorageHost, answer};

    fn meta(key: &str) -> JournalMeta {
        JournalMeta {
            format: JOURNAL_FORMAT,
            key: key.into(),
            kind: DocKind::Write,
            title: "Document1".into(),
            path: None,
            file_format: None,
            created_ms: 1,
            saved_at_ms: 1,
            dirty: false,
            generation: 0,
            print: None,
            seq: 0,
        }
    }

    fn run(store: &mut MemoryStorage, ops: Vec<HostOp>) {
        for op in ops {
            assert!(answer(store, &op).is_some(), "{op:?}");
        }
    }

    fn read(store: &MemoryStorage, key: &str) -> Result<Recovered> {
        let values: std::collections::BTreeMap<String, Vec<u8>> = store
            .list(StoreArea::Journal, "")?
            .into_iter()
            .filter_map(|k| {
                let v = store.get(StoreArea::Journal, &k).ok().flatten()?;
                Some((k, v))
            })
            .collect();
        let meta = values
            .get(&meta_key(key))
            .ok_or_else(|| damaged("no marker"))?;
        recover(meta, |k| values.get(k).map(Vec::as_slice))
    }

    fn keys(store: &MemoryStorage) -> Vec<String> {
        store.list(StoreArea::Journal, "").unwrap_or_default()
    }

    #[test]
    fn a_journal_replays_its_checkpoint_and_records() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k1"));
        assert!(
            j.append(vec![b"a".to_vec()], 2).is_err(),
            "no checkpoint yet"
        );
        run(&mut store, j.checkpoint(b"BUNDLE-1", 2, false)?);
        run(&mut store, j.append(vec![b"a".to_vec()], 3)?);
        run(&mut store, j.append(vec![b"b".to_vec(), b"c".to_vec()], 4)?);
        let r = read(&store, "k1")?;
        assert_eq!(r.bundle, b"BUNDLE-1");
        assert_eq!(r.ops, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        assert!(r.meta.dirty);
        assert_eq!(r.saved_at_ms, 4);
        assert_eq!((r.dropped, r.stale), (0, 0));
        assert_eq!(keys(&store), ["k1.base.1", "k1.log.1", "k1.meta"]);
        Ok(())
    }

    #[test]
    fn a_prepared_journal_is_clean_until_the_first_change() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k7"));
        run(&mut store, j.prepare(b"B", 1)?);
        assert!(!read(&store, "k7")?.meta.dirty);
        let ops = j.append(vec![b"x".to_vec()], 2)?;
        assert_eq!(ops.len(), 2, "the marker turns dirty with the first record");
        run(&mut store, ops);
        let r = read(&store, "k7")?;
        assert!(r.meta.dirty);
        assert_eq!(r.ops, vec![b"x".to_vec()]);
        Ok(())
    }

    #[test]
    fn every_two_hundred_operations_a_new_checkpoint_replaces_the_old() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k2"));
        run(&mut store, j.checkpoint(b"B1", 0, false)?);
        for i in 0..CHECKPOINT_OPS {
            assert!(!j.checkpoint_due());
            run(&mut store, j.append(vec![vec![i as u8]], u64::from(i))?);
        }
        assert!(j.checkpoint_due());
        run(&mut store, j.checkpoint(b"B2", 999, false)?);
        assert_eq!(j.ops_since_checkpoint(), 0);
        let r = read(&store, "k2")?;
        assert_eq!((r.generation, r.bundle.as_slice()), (2, b"B2".as_slice()));
        assert!(r.ops.is_empty());
        assert_eq!(
            keys(&store),
            ["k2.base.2", "k2.log.2", "k2.meta"],
            "the first generation is gone"
        );
        Ok(())
    }

    /// The document a journal stands for: its checkpoint followed by its operations.
    fn content(r: &Recovered) -> Vec<u8> {
        let mut out = r.bundle.clone();
        for op in &r.ops {
            out.extend_from_slice(op);
        }
        out
    }

    #[test]
    fn a_crash_at_any_point_of_a_checkpoint_recovers_the_whole_document() -> Result<()> {
        // "AB" + "c" + "d" is the document before the checkpoint, "ABcd" after it.
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k8"));
        run(&mut store, j.checkpoint(b"AB", 0, false)?);
        run(&mut store, j.append(vec![b"c".to_vec()], 1)?);
        run(&mut store, j.append(vec![b"d".to_vec()], 2)?);
        let writes = j.checkpoint(b"ABcd", 3, false)?;
        assert_eq!(writes.len(), 5, "log, base, marker, then the old two go");
        for cut in 0..=writes.len() {
            let mut crashed = store.clone();
            run(&mut crashed, writes[..cut].to_vec());
            let r = read(&crashed, "k8")?;
            assert_eq!(content(&r), b"ABcd", "a crash after {cut} writes");
            assert_eq!(r.generation, if cut >= 3 { 2 } else { 1 }, "cut {cut}");
            // The scan's leftovers are what recovery does not read.
            let left = stale_keys("k8", r.meta.generation, &keys(&crashed));
            let expected: &[&str] = match cut {
                2 => &["k8.base.2"],
                3 => &["k8.base.1", "k8.log.1"],
                4 => &["k8.log.1"],
                _ => &[],
            };
            assert_eq!(left, expected, "cut {cut}");
        }
        Ok(())
    }

    #[test]
    fn records_go_on_while_a_large_checkpoint_is_written_and_count_once() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k12"));
        j.set_inline_bytes(0);
        // The first one: nothing is listed until it lands.
        let first = j.checkpoint(b"AB", 0, false)?;
        assert_eq!(first.len(), 2, "log, base; the marker waits");
        run(&mut store, first);
        run(&mut store, j.checkpoint_written(0)?);
        run(&mut store, j.append(vec![b"c".to_vec()], 1)?);

        // The checkpoint starts; "ABc" is what it holds.
        let start = j.checkpoint(b"ABc", 2, false)?;
        assert!(j.writing_key().is_some());
        assert!(j.checkpoint(b"x", 2, false).is_err(), "one at a time");
        let (log, base) = (start[..1].to_vec(), start[1..].to_vec());
        run(&mut store, log);
        run(&mut store, j.append(vec![b"d".to_vec()], 3)?);

        // A crash with the checkpoint nowhere: the old one and both logs.
        let r = read(&store, "k12")?;
        assert_eq!((r.generation, r.last_generation), (1, 2));
        assert_eq!(content(&r), b"ABcd");

        // A crash with the checkpoint written and the marker not moved: the same.
        run(&mut store, base);
        assert_eq!(content(&read(&store, "k12")?), b"ABcd");
        run(&mut store, j.append(vec![b"e".to_vec()], 4)?);

        // The marker moves: the new one, and only what followed it.
        let flip = j.checkpoint_written(5)?;
        assert_eq!(flip.len(), 3, "marker, old checkpoint, old log");
        for cut in 1..=flip.len() {
            let mut crashed = store.clone();
            run(&mut crashed, flip[..cut].to_vec());
            let r = read(&crashed, "k12")?;
            assert_eq!(r.generation, 2);
            assert_eq!(content(&r), b"ABcde", "cut {cut}");
        }
        run(&mut store, flip);
        assert_eq!(keys(&store), ["k12.base.2", "k12.log.2", "k12.meta"]);
        Ok(())
    }

    #[test]
    fn a_recovered_journal_goes_on_past_every_log_it_read() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k13"));
        j.set_inline_bytes(0);
        run(&mut store, j.checkpoint(b"A", 0, false)?);
        run(&mut store, j.checkpoint_written(0)?);
        run(&mut store, j.checkpoint(b"A", 1, false)?);
        j.checkpoint_failed();
        run(&mut store, j.append(vec![b"b".to_vec()], 2)?);
        // A second try fails too; the logs chain.
        run(&mut store, j.checkpoint(b"Ab", 3, false)?);
        j.checkpoint_failed();
        run(&mut store, j.append(vec![b"c".to_vec()], 4)?);
        let r = read(&store, "k13")?;
        assert_eq!((r.generation, r.last_generation), (1, 3));
        assert_eq!(content(&r), b"Abc");

        let mut adopted = Journal::adopt(&r);
        run(&mut store, adopted.checkpoint(b"Abc", 5, false)?);
        assert_eq!(adopted.writing_key(), None, "small: one run of writes");
        let left = keys(&store);
        assert!(left.contains(&"k13.base.4".to_owned()), "{left:?}");
        assert_eq!(content(&read(&store, "k13")?), b"Abc");
        Ok(())
    }

    #[test]
    fn a_change_resting_on_a_large_checkpoint_is_named_until_it_lands() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k14"));
        j.set_inline_bytes(0);
        run(&mut store, j.checkpoint(b"A", 0, false)?);
        run(&mut store, j.checkpoint_written(0)?);
        run(&mut store, j.checkpoint(b"A!", 1, true)?);
        assert!(read(&store, "k14")?.unkept);
        run(&mut store, j.checkpoint_written(2)?);
        assert!(!read(&store, "k14")?.unkept);
        Ok(())
    }

    #[test]
    fn a_torn_last_record_is_dropped_and_the_rest_kept() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k3"));
        run(&mut store, j.checkpoint(b"B", 0, false)?);
        run(&mut store, j.append(vec![b"one".to_vec()], 1)?);
        run(&mut store, j.append(vec![b"two".to_vec()], 2)?);
        let full = store
            .get(StoreArea::Journal, &log_key("k3", 1))?
            .unwrap_or_default();
        let marker = store
            .get(StoreArea::Journal, &meta_key("k3"))?
            .unwrap_or_default();
        let base = encode_checkpoint(1, b"B");
        let with_log = |log: &[u8]| -> Result<Recovered> {
            recover(&marker, |k| match k {
                "k3.base.1" => Some(base.as_slice()),
                "k3.log.1" => Some(log),
                _ => None,
            })
        };
        // The crash came in the middle of the second record.
        for cut in 1..(full.len() / 2) {
            let r = with_log(&full[..full.len() - cut])?;
            assert_eq!(r.ops, vec![b"one".to_vec()], "cut {cut}");
            assert!(r.dropped > 0);
        }
        // A damaged byte in the last record: its checksum fails.
        let mut flipped = full.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0x40;
        assert_eq!(with_log(&flipped)?.ops, vec![b"one".to_vec()]);
        Ok(())
    }

    #[test]
    fn records_of_another_checkpoint_are_skipped() -> Result<()> {
        let mut j = Journal::new(meta("k4"));
        let _ = j.checkpoint(b"B1", 0, false)?;
        let rec = |generation, op: &[u8]| {
            encode_record(&Record {
                generation,
                time_ms: 1,
                seq: 0,
                ops: vec![op.to_vec()],
                missing: None,
            })
        };
        let log: Vec<u8> = [rec(0, b"old")?, rec(1, b"new")?].concat();
        let meta = serde_json::to_vec(j.meta()).map_err(|e| KilnError::Internal(e.to_string()))?;
        let base = encode_checkpoint(1, b"B1");
        let r = recover(&meta, |k| match k {
            "k4.base.1" => Some(base.as_slice()),
            "k4.log.1" => Some(log.as_slice()),
            _ => None,
        })?;
        assert_eq!(r.ops, vec![b"new".to_vec()]);
        assert_eq!(r.stale, 1);
        // A checkpoint that is not the one the marker names is refused.
        let wrong = encode_checkpoint(5, b"B5");
        assert!(recover(&meta, |k| (k == "k4.base.1").then_some(wrong.as_slice())).is_err());
        Ok(())
    }

    #[test]
    fn a_journal_of_the_first_format_is_read() -> Result<()> {
        // `k.base` with the generation in its header, `k.log`, a marker without a generation.
        let old = br#"{"format":1,"key":"k9","kind":"Deck","title":"Pitch","created_ms":5,
            "saved_at_ms":6,"dirty":true}"#;
        let mut base = encode_checkpoint(3, b"DECK");
        base[4..8].copy_from_slice(&1u32.to_le_bytes());
        let log = [
            encode_record_before4(&RecordBefore4 {
                generation: 2,
                time_ms: 7,
                ops: vec![b"stale".to_vec()],
            })?,
            encode_record_before4(&RecordBefore4 {
                generation: 3,
                time_ms: 8,
                ops: vec![b"x".to_vec()],
            })?,
        ]
        .concat();
        let r = recover(old, |k| match k {
            "k9.base" => Some(base.as_slice()),
            "k9.log" => Some(log.as_slice()),
            _ => None,
        })?;
        assert_eq!(
            (r.generation, r.ops.clone(), r.stale),
            (3, vec![b"x".to_vec()], 1)
        );
        assert_eq!(
            (r.meta.kind, r.meta.path.clone(), r.meta.generation),
            (DocKind::Deck, None, 0)
        );
        // Adopted, its next checkpoint moves it to the generation-keyed values.
        let mut store = MemoryStorage::default();
        run(
            &mut store,
            vec![
                put("k9.base".into(), base.clone()),
                put("k9.log".into(), log.clone()),
                put("k9.meta".into(), old.to_vec()),
            ],
        );
        let mut j = Journal::adopt(&r);
        run(&mut store, j.checkpoint(b"DECKx", 9, false)?);
        assert_eq!(keys(&store), ["k9.base.4", "k9.log.4", "k9.meta"]);
        let again = read(&store, "k9")?;
        assert_eq!(
            (again.meta.format, again.bundle.as_slice()),
            (JOURNAL_FORMAT, b"DECKx".as_slice())
        );
        Ok(())
    }

    #[test]
    fn a_journal_of_a_newer_format_is_refused() {
        let mut m = serde_json::to_value(meta("k5")).unwrap_or_default();
        m["format"] = serde_json::json!(JOURNAL_FORMAT + 1);
        m["something"] = serde_json::json!("from the future");
        let e = decode_meta(m.to_string().as_bytes())
            .unwrap_err()
            .to_string();
        assert!(e.contains("newer version of Kiln"), "{e}");
        let mut cp = encode_checkpoint(1, b"B");
        cp[4] = 9;
        assert!(
            decode_checkpoint(&cp)
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
        assert!(decode_meta(b"{}").is_err() && decode_meta(b"nope").is_err());
        assert!(decode_checkpoint(b"KJCP").is_err() && decode_checkpoint(b"PK").is_err());
    }

    #[test]
    fn a_save_clears_the_marker_only_when_nothing_changed_meanwhile() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k6"));
        run(&mut store, j.checkpoint(b"B", 0, false)?);
        run(
            &mut store,
            j.append(vec![b"typed while saving".to_vec()], 1)?,
        );
        run(
            &mut store,
            j.saved(
                "Plan",
                Some("/d/Plan.docx"),
                Some(FileFormat::Docx),
                None,
                false,
                2,
            )?,
        );
        let r = read(&store, "k6")?;
        assert!(r.meta.dirty, "the edit is not in the file");
        assert_eq!(r.ops, vec![b"typed while saving".to_vec()]);
        assert_eq!(r.meta.path.as_deref(), Some("/d/Plan.docx"));
        run(
            &mut store,
            j.saved(
                "Plan",
                Some("/d/Plan.docx"),
                Some(FileFormat::Docx),
                Some("9-abc"),
                true,
                5,
            )?,
        );
        assert!(!read(&store, "k6")?.meta.dirty);
        // The next edit marks it dirty again.
        let ops = j.append(vec![b"x".to_vec()], 6)?;
        assert_eq!(ops.len(), 2, "the marker, then the record");
        run(&mut store, ops);
        assert!(read(&store, "k6")?.meta.dirty);
        run(&mut store, j.remove());
        let keys = store.list(StoreArea::Journal, "")?;
        assert!(keys.is_empty(), "{keys:?}");
        Ok(())
    }

    #[test]
    fn the_text_being_typed_is_kept_beside_the_generations_and_ends_with_the_entry() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k10"));
        assert!(j.draft(Some(b"t"), 1).is_err(), "no checkpoint yet");
        run(&mut store, j.prepare(b"B", 1)?);
        // Nothing changed but the text: Document Recovery lists it all the same.
        run(&mut store, j.draft(Some(b"te"), 2)?);
        assert!(read(&store, "k10")?.meta.dirty);
        assert_eq!(
            read(&store, "k10")?.draft.as_deref(),
            Some(b"te".as_slice())
        );
        // A checkpoint leaves it.
        run(&mut store, j.checkpoint(b"B2", 3, false)?);
        assert_eq!(
            stale_keys("k10", 2, &keys(&store)),
            Vec::<String>::new(),
            "the draft is not a leftover"
        );
        run(&mut store, j.draft(None, 4)?);
        let r = read(&store, "k10")?;
        assert_eq!(r.draft, None);
        assert!(r.meta.dirty, "the checkpoint held a change");

        // Escape on a document nothing else changed: the marker is clean again.
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k11"));
        run(&mut store, j.prepare(b"B", 1)?);
        run(&mut store, j.draft(Some(b"x"), 2)?);
        run(&mut store, j.draft(None, 3)?);
        assert!(!read(&store, "k11")?.meta.dirty);

        // An entry that commits: its record, then the draft goes, and the marker stays dirty.
        run(&mut store, j.draft(Some(b"x"), 4)?);
        run(&mut store, j.append(vec![b"x".to_vec()], 5)?);
        run(&mut store, j.draft(None, 6)?);
        assert!(read(&store, "k11")?.meta.dirty);

        // A save with an entry open leaves the journal listed; a clean close removes the draft.
        run(&mut store, j.draft(Some(b"y"), 7)?);
        run(&mut store, j.saved("T", None, None, None, true, 8)?);
        assert!(read(&store, "k11")?.meta.dirty);
        run(&mut store, j.remove());
        assert!(keys(&store).is_empty());
        Ok(())
    }

    #[test]
    fn a_replay_stops_at_a_record_that_never_reached_the_disk_and_says_how_much_after() -> Result<()>
    {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k15"));
        run(&mut store, j.checkpoint(b"A", 0, false)?);
        run(&mut store, j.append(vec![b"b".to_vec()], 1)?);
        // The next record is lost; two after it reach the disk.
        let _ = j.append(vec![b"c".to_vec()], 2)?;
        run(&mut store, j.record_lost());
        run(&mut store, j.append(vec![b"d".to_vec()], 3)?);
        run(&mut store, j.append(vec![b"e".to_vec()], 4)?);
        let r = read(&store, "k15")?;
        assert_eq!(
            content(&r),
            b"Ab",
            "nothing replays onto a document missing a change"
        );
        let stop = r.stopped.clone().expect("stopped");
        assert_eq!((stop.missing, stop.after), (None, 2));
        assert!(r.unkept, "the page asks, and the recovery says so");
        // A checkpoint after it covers everything again.
        run(&mut store, j.checkpoint(b"Abcde", 5, false)?);
        let r = read(&store, "k15")?;
        assert_eq!(
            (content(&r), r.stopped, r.unkept),
            (b"Abcde".to_vec(), None, false)
        );
        Ok(())
    }

    #[test]
    fn a_change_no_record_holds_is_named_and_nothing_after_it_replays() -> Result<()> {
        let mut store = MemoryStorage::default();
        let mut j = Journal::new(meta("k16"));
        j.set_inline_bytes(0);
        run(&mut store, j.checkpoint(b"A", 0, false)?);
        run(&mut store, j.checkpoint_written(0)?);
        run(&mut store, j.append(vec![b"b".to_vec()], 1)?);
        // A checkpoint is under way when the change comes: it does not cover it.
        let start = j.checkpoint(b"Ab", 2, false)?;
        run(&mut store, start);
        run(&mut store, j.missing("Edits by others", 3)?);
        run(&mut store, j.append(vec![b"c".to_vec()], 4)?);
        run(&mut store, j.checkpoint_written(5)?);
        let r = read(&store, "k16")?;
        assert_eq!(content(&r), b"Ab");
        let stop = r.stopped.clone().expect("stopped");
        assert_eq!(stop.missing.as_deref(), Some("Edits by others"));
        assert_eq!(stop.after, 1);
        assert!(r.unkept && j.unkept());
        // The checkpoint after it.
        run(&mut store, j.checkpoint(b"Ab!c", 6, false)?);
        run(&mut store, j.checkpoint_written(7)?);
        let r = read(&store, "k16")?;
        assert_eq!(
            (content(&r), r.stopped.clone(), r.unkept),
            (b"Ab!c".to_vec(), None, false)
        );
        // A recovered journal goes on numbering after what it read.
        let mut adopted = Journal::adopt(&r);
        run(&mut store, adopted.append(vec![b"d".to_vec()], 8)?);
        assert_eq!(content(&read(&store, "k16")?), b"Ab!cd");
        Ok(())
    }

    #[test]
    fn leftovers_and_the_keys_of_a_journal_are_found() {
        let keys: Vec<String> = [
            "a.meta", "a.base.2", "a.log.2", "a.base.3", "b.base.1", "b.log.1", "c.log",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let (marked, leftovers) = journals(&keys);
        assert_eq!(marked, vec!["a"]);
        assert_eq!(leftovers, vec!["b.base.1", "b.log.1", "c.log"]);
        assert_eq!(
            keys_of("a", &keys),
            ["a.meta", "a.base.2", "a.log.2", "a.base.3"]
        );
        assert_eq!(stale_keys("a", 2, &keys), ["a.base.3"]);
        assert_eq!(journal_of("a.base.3"), "a");
        assert!(is_log("a.log.2") && is_log("c.log") && !is_log("a.base.2"));
    }
}
