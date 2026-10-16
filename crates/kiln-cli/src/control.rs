// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli control`: the JSON control channel over stdio (PLAN.md §3.1 principle 2).
//!
//! Newline-delimited JSON: each input line is one request, an object with one verb key and an
//! optional `"id"` that the response echoes; each request gets exactly one response line, followed
//! by the events of subscribed documents. Commands, queries, viewports, summaries and events use
//! the `kiln-protocol` types and their JSON form (the same the TypeScript client sees).
//!
//! | Request | Result |
//! |---|---|
//! | `{"new":{"kind":"scratch"}}` (kinds: scratch, write, grid, deck, notes) | `DocSummary` |
//! | `{"open":{"path":"a.txt","read_only":false}}` | `DocSummary` |
//! | `{"execute":{"doc":1,"cmd":{"Write":{"InsertText":{"text":"hi"}}}}}` (`doc` omitted for session commands) | `ExecResult` |
//! | `{"query":{"doc":1,"q":"Paragraphs"}}`; `"q":"Text"` gives the plain text | `QueryResult` |
//! | `{"viewport":{"doc":1,"viewport":{"width_px":1000,"height_px":800}}}` (missing fields take their defaults) | `null` |
//! | `{"frame":{"doc":1,"full":false}}` | a summary of the `SceneDelta` (the whole delta with `"full":true`) |
//! | `{"render":{"doc":1,"png":"out/p-%03d.png","page":1,"dpi":96,"dark":false}}` | `{"written":[paths]}` |
//! | `{"save":{"doc":1,"path":"out.txt","format":"Txt"}}` (`path` omitted: save in place) | `ExecResult` |
//! | `{"subscribe":{"doc":1}}`, `{"subscribe":{}}` for every document; `unsubscribe` likewise | `null` |
//! | `{"close":{"doc":1}}` | `ExecResult` |
//! | `{"ping":{}}` | `CoreInfo` |
//!
//! Responses are `{"id":…,"ok":true,"result":…}` or `{"id":…,"ok":false,"error":{"kind":…,
//! "message":…}}`; events are `{"event":…}`. A bad line produces an error response and the channel
//! keeps going; the process exits 0 at the end of input.

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, Write};

use kiln_common::{DocId, KilnError};
use kiln_protocol::{
    Command, DocKind, ErrorInfo, Event, ExecResult, OpenHint, Query, QueryResult, SceneDelta,
    SuiteCommand, Viewport,
};
use kiln_session::{NativeHost, Session};
use serde_json::{Value, json};

use crate::CliError;
use crate::pages::PageOptions;

/// Usage text.
pub const USAGE: &str =
    "kiln-cli control    (newline-delimited JSON requests on stdin; see docs/manual/cli.md)";

/// Longest accepted request line (a request may carry text to insert).
pub const MAX_LINE: usize = 64 << 20;

/// Which documents' events are forwarded.
#[derive(Debug, Default)]
struct Subscriptions {
    all: bool,
    docs: BTreeSet<DocId>,
}

impl Subscriptions {
    fn wants(&self, doc: Option<DocId>) -> bool {
        self.all || doc.is_some_and(|d| self.docs.contains(&d))
    }
}

/// The state of one control session.
pub struct Control {
    session: Session,
    subscriptions: Subscriptions,
    /// The last viewport a client set per document, restored after `render`.
    viewports: HashMap<DocId, Viewport>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control")
            .field("session", &self.session)
            .field("subscriptions", &self.subscriptions)
            .finish_non_exhaustive()
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

fn invalid(msg: impl Into<String>) -> KilnError {
    KilnError::InvalidInput(msg.into())
}

fn field<'a>(args: &'a Value, name: &str) -> Option<&'a Value> {
    args.get(name).filter(|v| !v.is_null())
}

fn doc_arg(args: &Value, required: bool) -> Result<Option<DocId>, KilnError> {
    match field(args, "doc") {
        None if required => Err(invalid("\"doc\" (a document id) is required")),
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(|n| Some(DocId(n)))
            .ok_or_else(|| invalid(format!("\"doc\" must be a document id, not {v}"))),
    }
}

fn required_doc(args: &Value) -> Result<DocId, KilnError> {
    doc_arg(args, true)?.ok_or_else(|| invalid("\"doc\" is required"))
}

fn parse_kind(v: Option<&Value>) -> Result<DocKind, KilnError> {
    let name = v
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("\"kind\" is required (scratch, write, grid, deck or notes)"))?;
    let kind = match name.to_ascii_lowercase().as_str() {
        "scratch" => DocKind::Scratch,
        "write" | "document" => DocKind::Write,
        "grid" | "workbook" => DocKind::Grid,
        "deck" | "presentation" => DocKind::Deck,
        "notes" | "page" => DocKind::Notes,
        _ => {
            return Err(invalid(format!(
                "unknown document kind \"{name}\" (scratch, write, grid, deck or notes)"
            )));
        }
    };
    Ok(kind)
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, KilnError> {
    serde_json::to_value(v).map_err(|e| KilnError::Internal(e.to_string()))
}

fn from_json<T: serde::de::DeserializeOwned>(what: &str, v: Value) -> Result<T, KilnError> {
    serde_json::from_value(v).map_err(|e| invalid(format!("{what}: {e}")))
}

/// A short description of a frame, for clients that do not paint.
fn frame_summary(d: &SceneDelta) -> Value {
    json!({
        "revision": d.revision,
        "reset": d.reset,
        "scale": d.scale,
        "extent": d.extent,
        "tiles": d.tiles.len(),
        "ops": d.tiles.iter().map(|t| t.ops.len()).sum::<usize>(),
        "tile_ids": d.tiles.iter().map(|t| t.id).collect::<Vec<_>>(),
        "removed": d.removed.len(),
        "atlas_updates": d.atlas.len(),
        "images": d.images.len(),
        "overlay": d.overlay.as_ref().map(|o| o.ops.len()),
    })
}

impl Control {
    /// A control session with the built-in apps and native file access.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: Session::new(Box::new(NativeHost)),
            subscriptions: Subscriptions::default(),
            viewports: HashMap::new(),
        }
    }

    /// Handles one request line and returns the output lines (the response, then events).
    pub fn handle_line(&mut self, line: &str) -> Vec<String> {
        let (id, result) = match serde_json::from_str::<Value>(line) {
            Err(e) => (Value::Null, Err(invalid(format!("not JSON: {e}")))),
            Ok(Value::Object(mut obj)) => {
                let id = obj.remove("id").unwrap_or(Value::Null);
                let result = if obj.len() == 1 {
                    let (verb, args) = obj.into_iter().next().unwrap_or_default();
                    self.dispatch(&verb, &args)
                } else {
                    Err(invalid(
                        "a request is an object with exactly one verb (new, open, execute, query, viewport, frame, render, save, subscribe, unsubscribe, close, ping) and an optional \"id\"",
                    ))
                };
                (id, result)
            }
            Ok(_) => (Value::Null, Err(invalid("a request must be a JSON object"))),
        };
        let response = match result {
            Ok(v) => json!({"id": id, "ok": true, "result": v}),
            Err(e) => json!({"id": id, "ok": false, "error": ErrorInfo::from(&e)}),
        };
        let mut out = vec![response.to_string()];
        for event in self.session.poll() {
            if self.subscriptions.wants(event.doc())
                && let Ok(v) = serde_json::to_value(&event)
            {
                out.push(json!({ "event": v }).to_string());
            }
            if let Event::Closed { doc } = event {
                self.subscriptions.docs.remove(&doc);
            }
        }
        out
    }

    fn dispatch(&mut self, verb: &str, args: &Value) -> Result<Value, KilnError> {
        if !args.is_object() && !args.is_null() {
            return Err(invalid(format!(
                "the arguments of \"{verb}\" must be an object"
            )));
        }
        match verb {
            "ping" => to_json(&Session::core_info()),
            "new" => {
                let kind = parse_kind(field(args, "kind"))?;
                to_json(&self.session.new_document(kind)?)
            }
            "open" => {
                let path = field(args, "path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("\"path\" is required"))?;
                let bytes = std::fs::read(path).map_err(KilnError::from)?;
                let hint = OpenHint {
                    path: Some(path.to_owned()),
                    read_only: field(args, "read_only")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    ..OpenHint::default()
                };
                to_json(&self.session.open(&bytes, hint)?)
            }
            "execute" => {
                let doc = doc_arg(args, false)?;
                let cmd: Command = from_json(
                    "cmd",
                    field(args, "cmd")
                        .cloned()
                        .ok_or_else(|| invalid("\"cmd\" is required"))?,
                )?;
                to_json(&self.session.execute(doc, cmd)?)
            }
            "query" => self.query(args),
            "viewport" => {
                let doc = required_doc(args)?;
                let mut base = to_json(&Viewport::default())?;
                let given =
                    field(args, "viewport").ok_or_else(|| invalid("\"viewport\" is required"))?;
                let Value::Object(given) = given else {
                    return Err(invalid("\"viewport\" must be an object"));
                };
                if let Value::Object(b) = &mut base {
                    for (k, v) in given {
                        b.insert(k.clone(), v.clone());
                    }
                }
                let vp: Viewport = from_json("viewport", base)?;
                self.session.set_viewport(doc, vp)?;
                self.viewports.insert(doc, vp);
                Ok(Value::Null)
            }
            "frame" => {
                let doc = required_doc(args)?;
                let delta = self.session.frame(doc)?;
                if field(args, "full")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    to_json(&delta)
                } else {
                    Ok(frame_summary(&delta))
                }
            }
            "render" => self.render(args),
            "save" => {
                let doc = required_doc(args)?;
                let format = field(args, "format")
                    .cloned()
                    .map(|f| from_json("format", f))
                    .transpose()?;
                let cmd = match field(args, "path").and_then(Value::as_str) {
                    Some(path) => SuiteCommand::SaveAs {
                        path: Some(path.to_owned()),
                        format,
                    },
                    None if format.is_none() => SuiteCommand::Save,
                    None => SuiteCommand::SaveAs { path: None, format },
                };
                let r: ExecResult = self.session.execute(Some(doc), Command::Suite(cmd))?;
                to_json(&r)
            }
            "close" => {
                let doc = required_doc(args)?;
                let r = self
                    .session
                    .execute(Some(doc), Command::Suite(SuiteCommand::Close))?;
                // The subscription ends once the `Closed` event has been forwarded.
                self.viewports.remove(&doc);
                to_json(&r)
            }
            "subscribe" | "unsubscribe" => {
                let on = verb == "subscribe";
                match doc_arg(args, false)? {
                    Some(d) if on => {
                        self.subscriptions.docs.insert(d);
                    }
                    Some(d) => {
                        self.subscriptions.docs.remove(&d);
                    }
                    None => {
                        self.subscriptions.all = on;
                        if !on {
                            self.subscriptions.docs.clear();
                        }
                    }
                }
                Ok(Value::Null)
            }
            other => Err(invalid(format!("unknown request \"{other}\""))),
        }
    }

    fn query(&mut self, args: &Value) -> Result<Value, KilnError> {
        let doc = doc_arg(args, false)?;
        let q = field(args, "q").ok_or_else(|| invalid("\"q\" is required"))?;
        if q.as_str() == Some("Text") {
            // The document's plain text: its paragraphs joined by line breaks.
            let QueryResult::Paragraphs(paras) = self.session.query(doc, &Query::Paragraphs)?
            else {
                return Err(KilnError::Internal(
                    "Paragraphs answered differently".into(),
                ));
            };
            let text: Vec<&str> = paras.iter().map(|p| p.text.as_str()).collect();
            return to_json(&QueryResult::Text(text.join("\n")));
        }
        let query: Query = from_json("q", q.clone())?;
        to_json(&self.session.query(doc, &query)?)
    }

    fn render(&mut self, args: &Value) -> Result<Value, KilnError> {
        let doc = required_doc(args)?;
        let target = field(args, "png")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("\"png\" (an output file, pattern or directory) is required"))?
            .to_owned();
        let pages = match field(args, "page") {
            None => None,
            Some(Value::Array(list)) => Some(
                list.iter()
                    .map(|v| v.as_u64().and_then(|n| usize::try_from(n).ok()))
                    .collect::<Option<Vec<usize>>>()
                    .ok_or_else(|| invalid("\"page\" must be page numbers"))?,
            ),
            Some(v) => Some(vec![
                v.as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| invalid("\"page\" must be a page number"))?,
            ]),
        };
        let dpi = match field(args, "dpi") {
            None => 96,
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| invalid("\"dpi\" must be a whole number"))?,
        };
        let options = PageOptions {
            dpi,
            dark: field(args, "dark")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pages,
        };
        let rendered = crate::pages::render(&mut self.session, doc, &options).map_err(invalid);
        // Rendering moves the viewport; put the client's back.
        if let Some(vp) = self.viewports.get(&doc).copied() {
            self.session.set_viewport(doc, vp)?;
            self.session
                .execute(Some(doc), Command::Suite(SuiteCommand::Repaint))?;
        }
        let written = crate::pages::write_pages(&rendered?, &target).map_err(invalid)?;
        Ok(json!({ "written": written }))
    }
}

/// Runs the channel until the end of `input`.
///
/// # Errors
///
/// When reading the input or writing the output fails.
pub fn serve(input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    let mut control = Control::new();
    let mut reader = input;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = std::io::Read::take(&mut reader, MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("reading stdin: {e}"))?;
        if n == 0 {
            return Ok(());
        }
        let lines = if buf.len() > MAX_LINE && buf.last() != Some(&b'\n') {
            // Skip the rest of the oversized line.
            let mut rest = Vec::new();
            loop {
                rest.clear();
                let m = std::io::Read::take(&mut reader, 1 << 20)
                    .read_until(b'\n', &mut rest)
                    .map_err(|e| format!("reading stdin: {e}"))?;
                if m == 0 || rest.last() == Some(&b'\n') {
                    break;
                }
            }
            let e = invalid(format!("request longer than {MAX_LINE} bytes"));
            vec![json!({"id": null, "ok": false, "error": ErrorInfo::from(&e)}).to_string()]
        } else {
            let text = String::from_utf8_lossy(&buf);
            let line = text.trim();
            if line.is_empty() {
                continue;
            }
            control.handle_line(line)
        };
        for l in lines {
            writeln!(output, "{l}").map_err(|e| format!("writing stdout: {e}"))?;
        }
        output.flush().map_err(|e| format!("writing stdout: {e}"))?;
    }
}

/// Runs the subcommand on stdin/stdout.
///
/// # Errors
///
/// Unexpected arguments, and I/O failures on the standard streams.
pub fn run(args: &[String]) -> Result<String, CliError> {
    if let Some(a) = args.first() {
        return Err(CliError::usage(format!("unexpected argument '{a}'"), USAGE));
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(stdin.lock(), stdout.lock()).map_err(CliError::Failed)?;
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_lines(input: &str) -> Vec<Value> {
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn creates_edits_and_queries_a_scratch_document() {
        let out = run_lines(concat!(
            "{\"new\":{\"kind\":\"scratch\"}}\n",
            "{\"execute\":{\"doc\":1,\"cmd\":{\"Write\":{\"InsertText\":{\"text\":\"hi\"}}}}}\n",
            "{\"id\":7,\"query\":{\"doc\":1,\"q\":\"Text\"}}\n",
            "{\"query\":{\"doc\":1,\"q\":\"Paragraphs\"}}\n",
            "{\"query\":{\"q\":\"Documents\"}}\n",
        ));
        assert_eq!(out.len(), 5);
        assert_eq!(out[0]["ok"], true);
        assert_eq!(out[0]["result"]["doc"], 1);
        assert_eq!(out[0]["result"]["kind"], "Scratch");
        assert_eq!(out[1]["result"]["outcome"], "Applied");
        assert_eq!(
            out[2],
            json!({"id": 7, "ok": true, "result": {"Text": "hi"}})
        );
        assert_eq!(out[3]["result"]["Paragraphs"][0]["text"], "hi");
        assert_eq!(out[4]["result"]["Documents"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn reports_errors_and_keeps_going() {
        let out = run_lines(concat!(
            "not json\n",
            "[1]\n",
            "{\"frobnicate\":{}}\n",
            "{\"new\":{},\"open\":{}}\n",
            "{\"new\":{\"kind\":\"spreadsheet\"}}\n",
            "{\"query\":{\"doc\":9,\"q\":\"Text\"}}\n",
            "{\"execute\":{\"doc\":1,\"cmd\":{\"Nope\":{}}}}\n",
            "\n",
            "{\"ping\":{}}\n",
        ));
        assert_eq!(out.len(), 8, "{out:?}");
        for r in &out[..7] {
            assert_eq!(r["ok"], false, "{r}");
            assert!(r["error"]["message"].as_str().is_some());
        }
        assert_eq!(out[0]["error"]["kind"], "InvalidInput");
        assert_eq!(out[5]["error"]["kind"], "NotFound");
        assert_eq!(out[7]["ok"], true);
        assert_eq!(out[7]["result"]["version"], kiln_common::VERSION);
    }

    #[test]
    fn subscriptions_forward_events() {
        let out = run_lines(concat!(
            "{\"subscribe\":{}}\n",
            "{\"new\":{\"kind\":\"scratch\"}}\n",
            "{\"unsubscribe\":{}}\n",
            "{\"subscribe\":{\"doc\":1}}\n",
            "{\"execute\":{\"doc\":1,\"cmd\":{\"Write\":{\"InsertText\":{\"text\":\"x\"}}}}}\n",
            "{\"close\":{\"doc\":1}}\n",
        ));
        let events: Vec<&Value> = out.iter().filter(|v| v.get("event").is_some()).collect();
        assert!(
            events.iter().any(|e| e["event"].get("Opened").is_some()),
            "{out:?}"
        );
        assert!(events.iter().any(|e| e["event"].get("Changed").is_some()));
        assert!(events.iter().any(|e| e["event"].get("Closed").is_some()));
    }

    #[test]
    fn viewport_frame_render_and_save() {
        let dir = std::env::temp_dir().join(format!("kiln-control-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.txt");
        std::fs::write(&src, "First paragraph.\nSecond paragraph.").unwrap();
        let saved = dir.join("out.txt");
        let png = dir.join("p-%d.png");
        let input = format!(
            concat!(
                "{{\"open\":{{\"path\":{src}}}}}\n",
                "{{\"viewport\":{{\"doc\":1,\"viewport\":{{\"width_px\":900,\"height_px\":700}}}}}}\n",
                "{{\"frame\":{{\"doc\":1}}}}\n",
                "{{\"render\":{{\"doc\":1,\"png\":{png},\"page\":1,\"dpi\":48}}}}\n",
                "{{\"frame\":{{\"doc\":1}}}}\n",
                "{{\"execute\":{{\"doc\":1,\"cmd\":{{\"Write\":{{\"InsertText\":{{\"text\":\"New \"}}}}}}}}}}\n",
                "{{\"save\":{{\"doc\":1,\"path\":{saved}}}}}\n",
            ),
            src = json!(src),
            png = json!(png),
            saved = json!(saved),
        );
        let out = run_lines(&input);
        assert!(out.iter().all(|r| r["ok"] == true), "{out:?}");
        assert_eq!(out[0]["result"]["title"], "in");
        assert!(
            out[2]["result"]["tiles"].as_u64().unwrap() >= 1,
            "{}",
            out[2]
        );
        let written = out[3]["result"]["written"][0].as_str().unwrap();
        assert!(written.ends_with("p-1.png"));
        assert!(std::path::Path::new(written).is_file());
        // After rendering, the client's viewport is back: the next frame repaints it at scale 1.
        assert_eq!(out[4]["result"]["scale"], 1.0);
        assert_eq!(out[6]["result"]["outcome"], "Applied");
        let text = std::fs::read_to_string(&saved).unwrap();
        assert!(text.starts_with("New First paragraph."), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_arguments() {
        assert!(matches!(run(&["x".into()]), Err(CliError::Usage(_))));
    }
}
