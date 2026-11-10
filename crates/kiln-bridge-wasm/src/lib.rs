// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-bridge-wasm
//!
//! The WebAssembly bridge: `wasm-bindgen` exports of `kiln-session` for the web
//! build. `apps/web/src/worker.ts` instantiates [`KilnSession`] inside a dedicated Web Worker;
//! `WasmWorkerClient` (packages/protocol) talks to the Worker with `postMessage`.
//!
//! Encodings: small payloads (commands, results, queries, events, viewports) are JSON strings of
//! the `kiln-protocol` types; frames are postcard bytes (`Uint8Array`), decoded by the
//! schema-driven decoder in `packages/protocol/src/postcard.ts`. Errors are thrown as JavaScript
//! `Error`s whose message is the JSON of an `ErrorInfo` (`{"kind": "...", "message": "..."}`).
//!
//! The session is single-threaded here; the host defers every file, dialog, clipboard and
//! preference request to the main thread (`Event::HostRequest` → `hostReply`).
//!
//! `#![forbid(unsafe_code)]` is deliberately absent: `#[wasm_bindgen]` expands to `unsafe` glue
//! (ABI conversions) in this crate. Hand-written `unsafe` is not allowed here.
#![warn(missing_docs)]

use kiln_common::{DocId, KilnError};
use kiln_protocol::{Command, DocKind, ErrorInfo, HostReply, OpenHint, Query, RequestId, Viewport};
use kiln_session::{AppFactory, DeferredHost, Session};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

#[cfg(feature = "threads")]
pub use wasm_bindgen_rayon::init_thread_pool;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The apps this module serves, as the web names them: `write`, `grid` or `deck` for a per-app
/// module, or several joined by `+` (the full core when every app feature is on).
pub const APPS: &str = {
    match (
        cfg!(feature = "write"),
        cfg!(feature = "grid"),
        cfg!(feature = "deck"),
    ) {
        (true, false, false) => "write",
        (false, true, false) => "grid",
        (false, false, true) => "deck",
        (true, true, false) => "write+grid",
        (true, false, true) => "write+deck",
        (false, true, true) => "grid+deck",
        (true, true, true) => "write+grid+deck",
        (false, false, false) => "",
    }
};

/// How this module numbers its documents: `(first, step)`. A per-app module takes the ids
/// `tag, tag + 4, tag + 8, …` with the tag 1 for Write, 2 for Grid and 3 for Deck (4 is kept
/// for Notes), so documents keep distinct ids while each app runs in its own Worker and the
/// page can tell which module holds a document from its id alone
/// (`apps/web/src/client/cores.ts`). A module with several apps numbers 1, 2, 3, ….
pub const DOC_IDS: (u32, u32) = {
    match APPS.as_bytes() {
        b"write" => (1, 4),
        b"grid" => (2, 4),
        b"deck" => (3, 4),
        _ => (1, 1),
    }
};

/// The app registrations of this module (in the order of `kiln_session::apps::builtin`).
/// `#[cfg]` rather than `cfg!`: an app left out must not be referenced at all, or its code stays
/// linked through the vtable.
#[allow(unused_mut, clippy::vec_init_then_push)]
fn apps() -> Vec<Box<dyn AppFactory>> {
    let mut apps: Vec<Box<dyn AppFactory>> = Vec::new();
    #[cfg(feature = "write")]
    {
        apps.push(Box::new(kiln_session::apps::scratch::ScratchApp));
        apps.push(Box::new(kiln_session::apps::write::WriteApp));
    }
    #[cfg(feature = "grid")]
    apps.push(Box::new(kiln_session::apps::grid::GridApp));
    #[cfg(feature = "deck")]
    apps.push(Box::new(kiln_session::apps::deck::DeckApp));
    apps
}

/// The first panic's message, for the page ([`take_panic`]).
static PANIC: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Installs the panic hook: the message goes to the console and is kept for [`take_panic`].
#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        if let Ok(mut p) = PANIC.lock() {
            p.get_or_insert_with(|| info.to_string());
        }
        console_error_panic_hook::hook(info);
    }));
}

/// The message of a panic in the core since the last call, if one happened. A WebAssembly core
/// cannot go on after a panic (its objects stay borrowed): the worker then answers every call
/// itself, and the page says so in written words while the autosave journal keeps the document.
#[wasm_bindgen(js_name = "takePanic")]
#[must_use]
pub fn take_panic() -> Option<String> {
    PANIC.lock().ok()?.take()
}

/// Returns the suite version, so the web host can check that it loaded the expected core.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    VERSION.to_owned()
}

/// The apps this module serves ([`APPS`]): the worker checks that it loaded the module it asked for.
#[wasm_bindgen(js_name = "coreApps")]
#[must_use]
pub fn core_apps() -> String {
    APPS.to_owned()
}

/// Gives the core a document font (the page fetches Carlito as a static asset and registers it
/// before creating the session: the core embeds no font, budget B12). Returns `false` when it
/// was already registered.
#[wasm_bindgen(js_name = "registerFont")]
#[must_use]
pub fn register_font(bytes: &[u8]) -> bool {
    kiln_session::register_font(bytes.to_vec())
}

/// Gives the core the formula function documentation (inline docs, Insert Function), which it
/// does not embed (budget B12): one JSON object mapping each file stem of
/// `crates/kiln-formula/docs` to that file's content. Returns `false` when it was already given
/// or the text is not such an object. Only the module with Kiln Grid has it.
#[cfg(feature = "grid")]
#[wasm_bindgen(js_name = "loadFunctionDocs")]
#[must_use]
pub fn load_function_docs(json: &str) -> bool {
    kiln_session::load_function_docs(json)
}

/// The kinds of characters the core laid out without a font since the last call (bit 1 Arabic,
/// 2 symbols, 4 mathematics, 8 other): the worker then fetches and registers the font that has
/// them.
#[wasm_bindgen(js_name = "takeMissingFonts")]
#[must_use]
pub fn take_missing_fonts() -> u32 {
    kiln_session::take_missing_fonts()
}

/// The Chinese, Japanese and Korean characters the core laid out without a font since the last
/// call (code points): the worker fetches the slices of the CJK font that have them.
#[wasm_bindgen(js_name = "takeMissingChars")]
#[must_use]
pub fn take_missing_chars() -> Vec<u32> {
    kiln_session::take_missing_chars()
}

/// Milliseconds since the Unix epoch.
#[cfg(target_arch = "wasm32")]
fn clock() -> u64 {
    // `Date.now()` is a non-negative integer number of milliseconds.
    js_sys::Date::now() as u64
}

#[cfg(not(target_arch = "wasm32"))]
fn clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn parse<T: DeserializeOwned>(what: &str, json: &str) -> Result<T, ErrorInfo> {
    serde_json::from_str(json)
        .map_err(|e| ErrorInfo::from(KilnError::InvalidInput(format!("{what}: {e}"))))
}

fn to_json<T: Serialize>(value: &T) -> Result<String, ErrorInfo> {
    serde_json::to_string(value)
        .map_err(|e| ErrorInfo::from(KilnError::Internal(format!("serialising a reply: {e}"))))
}

fn js_error(e: ErrorInfo) -> JsError {
    // `ErrorInfo` always serialises; the fallback keeps the error even if it did not.
    let json = serde_json::to_string(&e).unwrap_or_else(|_| e.message.clone());
    JsError::new(&json)
}

/// The session API in plain Rust (JSON in, JSON or bytes out), wrapped by [`KilnSession`]. Kept
/// separate so it is unit-tested natively.
#[derive(Debug)]
pub struct Bridge {
    session: Session,
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    /// A session with this module's apps ([`APPS`]) and a host that defers to the main thread.
    #[must_use]
    pub fn new() -> Self {
        let mut session = Session::with_apps(Box::new(DeferredHost::new(clock)), apps());
        let (first, step) = DOC_IDS;
        session.number_documents(first, step);
        Self { session }
    }

    /// Opens a file: `hint` is an `OpenHint`; returns a `DocSummary`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn open(&mut self, bytes: &[u8], hint: &str) -> Result<String, ErrorInfo> {
        let hint: OpenHint = parse("OpenHint", hint)?;
        to_json(&self.session.open(bytes, hint)?)
    }

    /// Creates a document: `kind` is a `DocKind`; returns a `DocSummary`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn new_document(&mut self, kind: &str) -> Result<String, ErrorInfo> {
        let kind: DocKind = parse("DocKind", kind)?;
        to_json(&self.session.new_document(kind)?)
    }

    /// Executes a `Command`; returns an `ExecResult`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn execute(&mut self, doc: Option<u32>, cmd: &str) -> Result<String, ErrorInfo> {
        let cmd: Command = parse("Command", cmd)?;
        to_json(&self.session.execute(doc.map(DocId), cmd)?)
    }

    /// Answers a `Query`; returns a `QueryResult`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn query(&mut self, doc: Option<u32>, query: &str) -> Result<String, ErrorInfo> {
        let query: Query = parse("Query", query)?;
        to_json(&self.session.query_mut(doc.map(DocId), &query)?)
    }

    /// Sets a document's `Viewport`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn set_viewport(&mut self, doc: u32, viewport: &str) -> Result<(), ErrorInfo> {
        let viewport: Viewport = parse("Viewport", viewport)?;
        Ok(self.session.set_viewport(DocId(doc), viewport)?)
    }

    /// The next frame: a postcard-encoded `SceneDelta`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn frame(&mut self, doc: u32) -> Result<Vec<u8>, ErrorInfo> {
        let delta = self.session.frame(DocId(doc))?;
        postcard::to_allocvec(&delta)
            .map_err(|e| ErrorInfo::from(KilnError::Internal(format!("encoding a frame: {e}"))))
    }

    /// The `A11yTree` of a document.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn a11y(&self, doc: u32) -> Result<String, ErrorInfo> {
        to_json(&self.session.a11y(DocId(doc))?)
    }

    /// Every queued `Event`, as a JSON array.
    ///
    /// # Errors
    ///
    /// Only if serialisation fails (a bug).
    pub fn poll(&mut self) -> Result<String, ErrorInfo> {
        to_json(&self.session.poll())
    }

    /// Delivers a `HostReply` for request `id`.
    ///
    /// # Errors
    ///
    /// The session's error, as `ErrorInfo`.
    pub fn host_reply(&mut self, id: u32, reply: &str) -> Result<(), ErrorInfo> {
        let reply: HostReply = parse("HostReply", reply)?;
        Ok(self.session.host_reply(RequestId(id), reply)?)
    }

    /// gives the session the collaboration engines (the collaboration module).
    pub fn set_collab(&mut self, backend: Box<dyn kiln_session::CollabBackend>) {
        self.session.set_collab(backend);
    }

    /// Whether the session has the collaboration engines.
    #[must_use]
    pub fn has_collab(&self) -> bool {
        self.session.has_collab()
    }

    /// the page keeps the app storage (autosave journals, versions, recent files).
    pub fn enable_storage(&mut self) {
        self.session.enable_storage();
    }
}

/// the collaboration module as the session's engines: a JavaScript function that takes
/// a postcard `EngineRequest` and returns a postcard `Result<EngineReply, String>`
/// (`CollabEngines.call` of `kiln-collab-wasm`, which the Worker loads when a document is shared
/// or joined).
#[cfg(target_arch = "wasm32")]
struct JsCollab(js_sys::Function);

#[cfg(target_arch = "wasm32")]
impl kiln_session::CollabBackend for JsCollab {
    fn request(
        &mut self,
        request: kiln_protocol::collab::engine::EngineRequest,
    ) -> kiln_common::Result<kiln_protocol::collab::engine::EngineReply> {
        let bytes = postcard::to_allocvec(&request)
            .map_err(|e| KilnError::Internal(format!("encoding a collaboration call: {e}")))?;
        let out = self
            .0
            .call1(
                &wasm_bindgen::JsValue::NULL,
                &js_sys::Uint8Array::from(bytes.as_slice()),
            )
            .map_err(|e| KilnError::Internal(format!("the collaboration module failed: {e:?}")))?;
        let out = js_sys::Uint8Array::new(&out).to_vec();
        let result: Result<kiln_protocol::collab::engine::EngineReply, String> =
            postcard::from_bytes(&out)
                .map_err(|e| KilnError::Internal(format!("decoding a collaboration reply: {e}")))?;
        result.map_err(KilnError::InvalidInput)
    }
}

/// The core, as JavaScript sees it. Every method throws an `Error` whose message is an
/// `ErrorInfo` JSON object on failure.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct KilnSession {
    inner: Bridge,
}

#[wasm_bindgen]
impl KilnSession {
    /// Panics inside a session call, as a bug in the core would (the e2e test of what the page
    /// shows then, `e2e/specs/pwa-stopped.spec.ts`). Nothing in the page calls it.
    #[wasm_bindgen(js_name = "panicForTest")]
    #[allow(clippy::unused_self)]
    pub fn panic_for_test(&mut self) {
        panic!("a panic forced by a test");
    }

    /// A new session with no documents.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> KilnSession {
        Self::default()
    }

    /// The core version.
    #[must_use]
    pub fn version() -> String {
        VERSION.to_owned()
    }

    /// Opens a file from its bytes. `hint`: `OpenHint` JSON. Returns `DocSummary` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn open(&mut self, bytes: &[u8], hint: &str) -> Result<String, JsError> {
        self.inner.open(bytes, hint).map_err(js_error)
    }

    /// Creates a document. `kind`: `DocKind` JSON (e.g. `"Scratch"`). Returns `DocSummary` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    #[wasm_bindgen(js_name = "newDocument")]
    pub fn new_document(&mut self, kind: &str) -> Result<String, JsError> {
        self.inner.new_document(kind).map_err(js_error)
    }

    /// Executes a command on `doc` (or the session when `doc` is undefined). `cmd`: `Command` JSON.
    /// Returns `ExecResult` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn execute(&mut self, doc: Option<u32>, cmd: &str) -> Result<String, JsError> {
        self.inner.execute(doc, cmd).map_err(js_error)
    }

    /// Answers a query. `query`: `Query` JSON. Returns `QueryResult` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn query(&mut self, doc: Option<u32>, query: &str) -> Result<String, JsError> {
        self.inner.query(doc, query).map_err(js_error)
    }

    /// Sets the viewport of a document. `viewport`: `Viewport` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    #[wasm_bindgen(js_name = "setViewport")]
    pub fn set_viewport(&mut self, doc: u32, viewport: &str) -> Result<(), JsError> {
        self.inner.set_viewport(doc, viewport).map_err(js_error)
    }

    /// The next frame of a document: postcard bytes of a `SceneDelta`.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn frame(&mut self, doc: u32) -> Result<Vec<u8>, JsError> {
        self.inner.frame(doc).map_err(js_error)
    }

    /// The accessibility tree of a document as `A11yTree` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn a11y(&self, doc: u32) -> Result<String, JsError> {
        self.inner.a11y(doc).map_err(js_error)
    }

    /// Takes the queued events as a JSON array of `Event`.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    pub fn poll(&mut self) -> Result<String, JsError> {
        self.inner.poll().map_err(js_error)
    }

    /// Delivers the reply to host request `id`. `reply`: `HostReply` JSON.
    ///
    /// # Errors
    ///
    /// Throws an `ErrorInfo` JSON message.
    #[wasm_bindgen(js_name = "hostReply")]
    pub fn host_reply(&mut self, id: u32, reply: &str) -> Result<(), JsError> {
        self.inner.host_reply(id, reply).map_err(js_error)
    }

    /// gives the core the collaboration module: `call` is `CollabEngines.call` of
    /// `kiln-collab-wasm` (postcard in, postcard out).
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = "setCollab")]
    pub fn set_collab(&mut self, call: js_sys::Function) {
        self.inner.set_collab(Box::new(JsCollab(call)));
    }

    /// Whether the core has the collaboration module.
    #[wasm_bindgen(js_name = "hasCollab")]
    #[must_use]
    pub fn has_collab(&self) -> bool {
        self.inner.has_collab()
    }

    /// the page keeps the app storage in the Origin Private File System and answers its
    /// requests: the core keeps autosave journals, versions of local files and recent files.
    #[wasm_bindgen(js_name = "enableStorage")]
    pub fn enable_storage(&mut self) {
        self.inner.enable_storage();
    }
}

#[cfg(test)]
mod module_tests {
    use super::*;

    #[test]
    fn a_module_numbers_documents_by_its_app() {
        let mut b = Bridge::new();
        let kind = match APPS {
            "grid" => "\"Grid\"",
            "deck" => "\"Deck\"",
            _ => "\"Write\"",
        };
        let first: kiln_protocol::DocSummary =
            serde_json::from_str(&b.new_document(kind).unwrap()).unwrap();
        let second: kiln_protocol::DocSummary =
            serde_json::from_str(&b.new_document(kind).unwrap()).unwrap();
        assert_eq!(first.doc.0, DOC_IDS.0);
        assert_eq!(second.doc.0, DOC_IDS.0 + DOC_IDS.1);
        assert_eq!(core_apps(), APPS);
    }

    #[test]
    fn per_app_modules_use_distinct_residues() {
        // `apps/web/src/client/cores.ts` maps `doc % 4` back to the module.
        let tags = [("write", 1), ("grid", 2), ("deck", 3)];
        if APPS.contains('+') {
            assert_eq!(DOC_IDS, (1, 1));
        } else {
            assert_eq!(DOC_IDS.1, 4);
            assert!(tags.contains(&(APPS, DOC_IDS.0)));
        }
    }

    #[test]
    fn a_module_refuses_another_apps_documents() {
        let mut b = Bridge::new();
        for (app, kind) in [
            ("write", "\"Write\""),
            ("grid", "\"Grid\""),
            ("deck", "\"Deck\""),
        ] {
            let made = b.new_document(kind);
            if APPS.split('+').any(|a| a == app) {
                assert!(made.is_ok(), "{app}");
            } else {
                assert_eq!(
                    made.unwrap_err().kind,
                    kiln_protocol::ErrorKind::Unsupported
                );
            }
        }
    }
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::*;
    use kiln_protocol::{
        DocSummary, ErrorKind, Event, ExecResult, ExecValue, QueryResult, SceneDelta,
    };

    #[test]
    fn exported_version_is_the_suite_version() {
        assert_eq!(super::version(), kiln_common::VERSION);
        assert_eq!(KilnSession::version(), kiln_common::VERSION);
    }

    #[test]
    fn json_round_trip_through_the_bridge() {
        let mut b = Bridge::new();
        let pong: ExecResult =
            serde_json::from_str(&b.execute(None, r#"{"Suite":"Ping"}"#).unwrap()).unwrap();
        assert!(matches!(pong.value, ExecValue::Pong(info) if info.version == VERSION));

        let doc: DocSummary =
            serde_json::from_str(&b.new_document("\"Scratch\"").unwrap()).unwrap();
        let r: ExecResult = serde_json::from_str(
            &b.execute(Some(doc.doc.0), r#"{"Write":{"InsertText":{"text":"Hi"}}}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(r.summary.unwrap().revision, 1);
        let q: QueryResult =
            serde_json::from_str(&b.query(Some(doc.doc.0), "\"Paragraphs\"").unwrap()).unwrap();
        assert!(matches!(q, QueryResult::Paragraphs(p) if p[0].text == "Hi"));

        let events: Vec<Event> = serde_json::from_str(&b.poll().unwrap()).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(b.poll().unwrap(), "[]");

        let frame: SceneDelta = postcard::from_bytes(&b.frame(doc.doc.0).unwrap()).unwrap();
        assert_eq!(frame.revision, 1);
        b.set_viewport(
            doc.doc.0,
            r#"{"width_px":800,"height_px":600,"scroll":{"x":0,"y":0},"zoom":1,"dpr":2,"mode":"PrintLayout"}"#,
        )
        .unwrap();
        assert!(b.a11y(doc.doc.0).unwrap().contains("\"Hi\""));
        let opened: DocSummary = serde_json::from_str(
            &b.open(
                b"a\nb",
                r#"{"name":"x.txt","path":null,"format":null,"read_only":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(opened.title, "x");
    }

    #[test]
    fn errors_carry_their_kind() {
        let mut b = Bridge::new();
        let e = b.execute(None, "{not json").unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidInput);
        assert!(e.message.starts_with("invalid input: Command:"));
        let e = b.execute(Some(4), r#"{"Suite":"Undo"}"#).unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotFound);
        assert_eq!(b.frame(4).unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(
            b.host_reply(1, "\"Done\"").unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            b.host_reply(1, "nope").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(
            b.set_viewport(1, "{}").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(b.a11y(1).unwrap_err().kind, ErrorKind::NotFound);
        // No app creates notebooks yet (Kiln Deck creates presentations).
        assert_eq!(
            b.new_document("\"Notes\"").unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            b.query(None, "\"Summary\"").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn host_requests_are_deferred_to_the_main_thread() {
        let mut b = Bridge::new();
        let r: ExecResult = serde_json::from_str(
            &b.execute(None, r#"{"Suite":{"Open":{"path":"/a.txt"}}}"#)
                .unwrap(),
        )
        .unwrap();
        let kiln_protocol::Outcome::Pending(id) = r.outcome else {
            panic!("{r:?}")
        };
        let events: Vec<Event> = serde_json::from_str(&b.poll().unwrap()).unwrap();
        assert!(matches!(&events[..], [Event::HostRequest(req)] if req.id == id));
        b.host_reply(id.0, r#"{"Bytes":[104,105]}"#).unwrap();
        let events: Vec<Event> = serde_json::from_str(&b.poll().unwrap()).unwrap();
        assert!(matches!(&events[..], [Event::Opened { summary }] if summary.title == "a"));
    }
}
