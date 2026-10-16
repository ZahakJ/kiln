// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Tauri IPC commands of PLAN.md §3.5. Each forwards to the session thread through
//! [`SessionHandle`] and never blocks the IPC thread; errors reach the webview as `ErrorInfo`.

use std::time::Instant;

use kiln_common::{DocId, KilnError};
use kiln_protocol::{
    A11yTree, Command, DocKind, DocSummary, ErrorInfo, Event, ExecResult, HostReply, OpenHint,
    Query, QueryResult, RequestId, Viewport,
};
use kiln_session::SessionHandle;
use serde::Serialize;
use tauri::State;
use tauri::ipc::{Channel, InvokeBody, Request, Response};

/// State shared by the commands.
#[derive(Debug)]
pub struct Kiln {
    /// The session thread.
    pub session: SessionHandle,
    /// When the process started (for the startup probe).
    pub started: Instant,
}

type Reply<T> = Result<T, ErrorInfo>;

/// The header that carries the `OpenHint` of `kiln_open` (percent-encoded JSON, because header
/// values must be ASCII and file names need not be).
pub const HINT_HEADER: &str = "kiln-hint";

/// Decodes `%XX` escapes (as produced by `encodeURIComponent`).
///
/// # Errors
///
/// `InvalidInput` for a malformed escape or a result that is not UTF-8.
pub fn percent_decode(input: &str) -> Result<String, KilnError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| KilnError::InvalidInput("malformed percent escape".into()))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| KilnError::InvalidInput("hint is not UTF-8".into()))
}

/// Parses the `kiln-hint` header value.
///
/// # Errors
///
/// `InvalidInput` when the value is not percent-encoded `OpenHint` JSON.
pub fn parse_hint(value: Option<&str>) -> Result<OpenHint, KilnError> {
    match value {
        None => Ok(OpenHint::default()),
        Some(v) => serde_json::from_str(&percent_decode(v)?)
            .map_err(|e| KilnError::InvalidInput(format!("kiln-hint: {e}"))),
    }
}

/// Opens a file sent as the raw request body; the `kiln-hint` header carries the `OpenHint`.
#[tauri::command]
pub async fn kiln_open(state: State<'_, Kiln>, request: Request<'_>) -> Reply<DocSummary> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(KilnError::InvalidInput(
            "kiln_open expects the file bytes as the request body".into(),
        )
        .into());
    };
    let header = request
        .headers()
        .get(HINT_HEADER)
        .map(|v| {
            v.to_str()
                .map_err(|_| KilnError::InvalidInput("kiln-hint is not ASCII".into()))
        })
        .transpose()?;
    let hint = parse_hint(header)?;
    Ok(state.session.open(bytes.clone(), hint).await?)
}

/// Creates a document.
#[tauri::command]
pub async fn kiln_new(state: State<'_, Kiln>, kind: DocKind) -> Reply<DocSummary> {
    Ok(state.session.new_document(kind).await?)
}

/// Executes a command on a document (or the session with `doc: null`).
#[tauri::command]
pub async fn kiln_execute(
    state: State<'_, Kiln>,
    doc: Option<DocId>,
    cmd: Command,
) -> Reply<ExecResult> {
    Ok(state.session.execute(doc, cmd).await?)
}

/// Answers a query.
#[tauri::command]
pub async fn kiln_query(
    state: State<'_, Kiln>,
    doc: Option<DocId>,
    query: Query,
) -> Reply<QueryResult> {
    Ok(state.session.query(doc, query).await?)
}

/// Sets a document's viewport.
#[tauri::command]
pub async fn kiln_viewport(state: State<'_, Kiln>, doc: DocId, vp: Viewport) -> Reply<()> {
    Ok(state.session.set_viewport(doc, vp).await?)
}

/// The next frame, as raw postcard bytes (an `ArrayBuffer` in the webview).
#[tauri::command]
pub async fn kiln_frame(state: State<'_, Kiln>, doc: DocId) -> Reply<Response> {
    let delta = state.session.frame(doc).await?;
    let bytes = postcard::to_allocvec(&delta)
        .map_err(|e| KilnError::Internal(format!("encoding a frame: {e}")))?;
    Ok(Response::new(bytes))
}

/// The accessibility tree of a document.
#[tauri::command]
pub async fn kiln_a11y(state: State<'_, Kiln>, doc: DocId) -> Reply<A11yTree> {
    Ok(state.session.a11y(doc).await?)
}

/// Streams every session event to the webview over an ordered channel until the webview goes away.
#[tauri::command]
pub async fn kiln_subscribe(state: State<'_, Kiln>, on_event: Channel<Event>) -> Reply<()> {
    let events = state.session.subscribe()?;
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv_async().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// Delivers the webview's answer to a host request.
#[tauri::command]
pub async fn kiln_host_reply(state: State<'_, Kiln>, id: RequestId, reply: HostReply) -> Reply<()> {
    Ok(state.session.host_reply(id, reply).await?)
}

/// What `kiln_startup_probe` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StartupProbe {
    /// Milliseconds since the process started.
    pub elapsed_ms: u64,
    /// Resident set size in KiB, where the OS reports it.
    pub rss_kb: Option<u64>,
}

/// Time since process start and memory, for the B1/B2 budgets measured from the webview.
#[tauri::command]
pub fn kiln_startup_probe(state: State<'_, Kiln>) -> StartupProbe {
    StartupProbe {
        elapsed_ms: u64::try_from(state.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        rss_kb: crate::rss_kib(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::FileFormat;

    #[test]
    fn decodes_percent_encoded_hints() {
        assert_eq!(percent_decode("Zo%C3%AB%20a.txt").unwrap(), "Zoë a.txt");
        assert!(percent_decode("%G1").is_err());
        assert!(percent_decode("%4").is_err());
        assert!(percent_decode("%FF").is_err());
        let hint = parse_hint(Some(
            "%7B%22name%22%3A%22Zo%C3%AB.txt%22%2C%22path%22%3Anull%2C%22format%22%3A%22Txt%22%2C%22read_only%22%3Atrue%7D",
        ))
        .unwrap();
        assert_eq!(hint.name.as_deref(), Some("Zoë.txt"));
        assert_eq!(hint.format, Some(FileFormat::Txt));
        assert!(hint.read_only);
        assert_eq!(parse_hint(None).unwrap(), OpenHint::default());
        assert!(parse_hint(Some("nope")).is_err());
    }
}
