// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Tauri IPC commands. Each forwards to the session thread through
//! [`SessionHandle`] and never blocks the IPC thread; errors reach the webview as `ErrorInfo`. The
//! shell's own commands (`kiln_window_context`, `kiln_close_window`, `kiln_home_painted`,
//! `kiln_open_paths`, `kiln_open_route`, `kiln_clipboard_text`, and on Android `kiln_share` and
//! `kiln_leave`) follow the IPC commands.

use std::time::Instant;

use kiln_common::{DocId, KilnError};
use kiln_protocol::{
    A11yTree, Command, DocKind, DocSummary, ErrorInfo, Event, ExecResult, HostReply, OpenHint,
    Query, QueryResult, RequestId, Viewport,
};
use std::sync::Arc;

use kiln_session::SessionHandle;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, Runtime, State, Webview, WebviewWindow};

use crate::shell::{Shell, WindowContext};

/// State shared by the commands.
#[derive(Debug)]
pub struct Kiln {
    /// The session thread.
    pub session: SessionHandle,
    /// When the process started (for the startup probe).
    pub started: Instant,
    /// `--startup-probe`: report the first paint of Kiln Home and exit.
    pub probe: bool,
    /// Whether the first paint of Kiln Home was reported.
    pub painted: std::sync::atomic::AtomicBool,
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

/// Streams the events of one document (`doc`), or of every document (`doc: null`), plus the
/// session-wide events, to the webview over an ordered channel. A webview has one stream per
/// `doc`: subscribing again (after a reload) replaces its previous stream, so streams never pile
/// up. Events nobody has received yet, such as a host request raised while the webview was
/// loading, are delivered to the new stream.
#[tauri::command]
pub async fn kiln_subscribe(
    state: State<'_, Kiln>,
    webview: Webview,
    doc: Option<DocId>,
    on_event: Channel<Event>,
) -> Reply<()> {
    let key = format!("webview:{}", webview.label());
    let events = state.session.subscribe_as(key, doc)?;
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
        elapsed_ms: crate::probe::elapsed_ms(state.started),
        rss_kb: crate::probe::own_rss_kib(),
    }
}

/// Kiln Home has painted its first frame. Under `--startup-probe` the time is taken now; the
/// memory once Home has been idle for a moment ([`crate::probe::IDLE`]); then the report is printed
/// and the app exits.
#[tauri::command]
pub fn kiln_home_painted<R: Runtime>(app: AppHandle<R>, state: State<'_, Kiln>) {
    use std::sync::atomic::Ordering;
    if !state.probe || state.painted.swap(true, Ordering::SeqCst) {
        return;
    }
    let ms = crate::probe::elapsed_ms(state.started);
    std::thread::spawn(move || {
        std::thread::sleep(crate::probe::IDLE);
        let tree = crate::probe::process_tree();
        let (rss, pss) = crate::probe::memory_kib(&tree);
        println!("{}", crate::probe::report(ms, tree.len(), rss, pss));
        app.exit(0);
    });
}

/// What a window's page shows, reported on every change: the app (`home`, `write`, `grid`,
/// `deck`, `present`), the document and whether it has unsaved changes, the interface language,
/// the theme (`light`, `dark`) and the title bar's colour (`bar`, `#RRGGBB`). The window's menu
/// bar, its language and its colours follow; on Android the system bars take the title bar's
/// colour.
#[tauri::command]
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn kiln_window_context<R: Runtime>(
    window: WebviewWindow<R>,
    shell: State<'_, Arc<Shell>>,
    app: String,
    doc: Option<u64>,
    dirty: bool,
    language: String,
    theme: Option<String>,
    bar: Option<String>,
) {
    let changed_language = shell.set_language(&language);
    let previous = shell.set_context(
        window.label(),
        WindowContext {
            app: app.clone(),
            doc,
            dirty,
        },
    );
    #[cfg(desktop)]
    if changed_language {
        // Every window's menus speak the new language.
        for w in window.app_handle().webview_windows().into_values() {
            let a = shell.context(w.label()).map(|c| c.app).unwrap_or_default();
            crate::menu::apply(&w, &shell, &a);
        }
    } else if previous.as_deref() != Some(app.as_str()) {
        crate::menu::apply(&window, &shell, &app);
    }
    #[cfg(mobile)]
    let _ = (changed_language, previous);
    #[cfg(target_os = "android")]
    crate::android::page_reported(window.app_handle(), theme.as_deref(), bar.as_deref());
    #[cfg(not(target_os = "android"))]
    let _ = bar;
    // A window opened at exit on an unsaved document asks about it now that it shows it.
    #[cfg(desktop)]
    if doc.is_some() && shell.take_ask_close(window.label()) {
        use tauri::Emitter;
        let _ = window.emit_to(window.label(), crate::windows::CLOSE_EVENT, ());
    }
    let theme = match theme.as_deref() {
        Some("dark") => Some(tauri::Theme::Dark),
        Some("light") => Some(tauri::Theme::Light),
        _ => None,
    };
    let _ = window.set_theme(theme);
}

/// The page has closed its document: closes the window. With `unless_last`, the last window
/// (not counting slide shows) stays open and `false` comes back (the page then shows Kiln Home).
#[tauri::command]
pub fn kiln_close_window<R: Runtime>(
    window: WebviewWindow<R>,
    shell: State<'_, Arc<Shell>>,
    unless_last: bool,
) -> bool {
    if unless_last {
        let others = window
            .app_handle()
            .webview_windows()
            .keys()
            .filter(|l| l.as_str() != window.label() && !l.starts_with("show-"))
            .count();
        if others == 0 {
            return false;
        }
    }
    shell.agree_close(window.label());
    let _ = window.close();
    true
}

/// Shows a route in a window of its own: a document a document window opened, made new or made
/// from a template, which must not take that window from the document it shows. The window that
/// already shows the document comes to the front instead.
#[tauri::command]
pub fn kiln_open_route<R: Runtime>(app: AppHandle<R>, route: String) -> Reply<()> {
    #[cfg(desktop)]
    {
        crate::windows::show_route(&app, &route)
            .map_err(|e| ErrorInfo::from(KilnError::InvalidInput(e.to_string())))
    }
    #[cfg(target_os = "android")]
    {
        crate::android::show_route(&app, &route);
        Ok(())
    }
}

/// Opens documents by path in windows of their own (the page's fallback for files dropped on a
/// window where the shell does not see drops itself).
#[tauri::command]
pub fn kiln_open_paths<R: Runtime>(window: WebviewWindow<R>, paths: Vec<String>) {
    #[cfg(desktop)]
    {
        let paths = paths.into_iter().map(std::path::PathBuf::from).collect();
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn(crate::windows::open_paths(app, paths, Some(window)));
    }
    #[cfg(mobile)]
    let _ = (window, paths);
}

/// The clipboard's plain text for a text field of the chrome (the menu's Copy, Cut and Paste
/// there): with `text`, puts it on the clipboard and returns it; without, returns what is there.
#[cfg(desktop)]
#[tauri::command]
pub async fn kiln_clipboard_text(
    clipboard: State<'_, Arc<crate::host::Clipboard>>,
    text: Option<String>,
) -> Reply<String> {
    let clipboard = Arc::clone(&clipboard);
    tauri::async_runtime::spawn_blocking(move || match text {
        Some(t) => match clipboard.write(&t, None) {
            HostReply::Failed(e) => Err(e),
            _ => Ok(t),
        },
        None => match clipboard.read() {
            HostReply::Clipboard { text, .. } => Ok(text),
            HostReply::Failed(e) => Err(e),
            _ => Ok(String::new()),
        },
    })
    .await
    .map_err(|e| ErrorInfo::from(KilnError::Internal(e.to_string())))?
}

/// Android: the clipboard through Android's clipboard service.
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn kiln_clipboard_text<R: Runtime>(
    app: AppHandle<R>,
    text: Option<String>,
) -> Reply<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let android = app
            .try_state::<crate::android::plugin::Android<R>>()
            .ok_or_else(|| ErrorInfo::from(KilnError::Internal("no Android plugin".into())))?;
        let refused = |e: crate::android::plugin::Refused| {
            ErrorInfo::from(KilnError::Internal(format!("the clipboard: {e}")))
        };
        match text {
            Some(t) => android
                .clipboard_write(&t, None)
                .map(|()| t)
                .map_err(refused),
            None => android.clipboard_read().map(|c| c.text).map_err(refused),
        }
    })
    .await
    .map_err(|e| ErrorInfo::from(KilnError::Internal(e.to_string())))?
}

/// Sends a copy of a document through Android's share sheet: its file as last saved, or with
/// `pdf` a PDF of it. Other platforms share through File ▸ Share.
#[tauri::command]
pub async fn kiln_share<R: Runtime>(app: AppHandle<R>, doc: DocId, pdf: bool) -> Reply<()> {
    #[cfg(target_os = "android")]
    {
        crate::android::share(app, doc, pdf).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, doc, pdf);
        Err(KilnError::Unsupported("sending a copy from the system's share sheet".into()).into())
    }
}

/// Back on Kiln Home (Android): Kiln goes to the background. Nothing elsewhere.
#[tauri::command]
pub fn kiln_leave<R: Runtime>(app: AppHandle<R>) {
    #[cfg(target_os = "android")]
    crate::android::leave(&app);
    #[cfg(not(target_os = "android"))]
    let _ = app;
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
