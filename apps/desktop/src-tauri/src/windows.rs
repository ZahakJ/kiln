// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows: Kiln Home, a window per document (Office's model), the audience window of Presenter
//! View (`HostOp::OpenWindow`, on the one native core: both windows follow the show through
//! `Event::Show`), opening files given on the command line, by the desktop environment or a
//! second launch (single instance), dropped on a window, and closing with unsaved changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kiln_protocol::{DocKind, DocSummary, OpenHint};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WindowEvent};

use crate::bridge::Kiln;
use crate::files;
use crate::shell::{Shell, WindowContext, route_app};

/// The first window's label (its size and place are remembered as `main`).
pub const MAIN: &str = "main";

/// The event a document window's page hears when the window is asked to close: the page closes
/// the document (asking about unsaved changes) and then the window (`kiln_close_window`).
pub const CLOSE_EVENT: &str = "kiln://close-requested";

/// A number that makes a `#/write?new=…` route new each time.
#[must_use]
pub fn nonce() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

/// The route of an open document (`#/write?doc=3`).
#[must_use]
pub fn doc_route(summary: &DocSummary) -> String {
    let app = match summary.kind {
        DocKind::Grid => "grid",
        DocKind::Deck => "deck",
        DocKind::Scratch | DocKind::Write | DocKind::Notes => "write",
    };
    format!("#/{app}?doc={}", summary.doc.0)
}

/// The label prefix and window-state key of a route: windows of one kind remember one size and
/// place (`main` for the first, `document` for the others, `show` for the audience).
#[must_use]
pub fn state_key(label: &str) -> String {
    match label.split_once('-') {
        Some(("show", _)) => "show".into(),
        Some(_) => "document".into(),
        None => label.to_owned(),
    }
}

/// Opens a window on `route` with `title` (the page sets its own title once it shows).
///
/// # Errors
///
/// When the window cannot be created.
pub fn open<R: Runtime>(
    app: &AppHandle<R>,
    route: &str,
    title: &str,
) -> tauri::Result<WebviewWindow<R>> {
    let shell = Arc::clone(&app.state::<Arc<Shell>>());
    let kind = route_app(route);
    let label = if app.get_webview_window(MAIN).is_none() && kind != "present" {
        MAIN.to_owned()
    } else if kind == "present" {
        shell.next_label("show")
    } else {
        shell.next_label("doc")
    };
    let mut builder = tauri::WebviewWindowBuilder::new(app, &label, WebviewUrl::App(route.into()))
        .title(title)
        .min_inner_size(400.0, 500.0)
        .on_document_title_changed(|w, title| {
            if !title.is_empty() {
                let _ = w.set_title(&title);
            }
        });
    // Windows needs the HTML drag and drop of the chrome (sheet tabs, panes): files dropped on a
    // window come through the page there.
    #[cfg(target_os = "windows")]
    {
        builder = builder.disable_drag_drop_handler();
    }
    let mut fullscreen_on = None;
    if kind == "present" {
        builder = builder
            .inner_size(1280.0, 720.0)
            .min_inner_size(320.0, 180.0);
        // The audience sees the show on the other screen, full screen, as in PowerPoint.
        if let Some(presenter) = app.get_webview_window(MAIN).or_else(|| first_window(app))
            && let Ok(monitors) = presenter.available_monitors()
            && monitors.len() > 1
        {
            let here = presenter.current_monitor().ok().flatten();
            if let Some(other) = monitors
                .iter()
                .find(|m| here.as_ref().is_none_or(|h| h.position() != m.position()))
            {
                let p = other.position().to_logical::<f64>(other.scale_factor());
                builder = builder.position(p.x + 40.0, p.y + 40.0);
                fullscreen_on = Some(());
            }
        }
    } else {
        builder = builder.inner_size(1280.0, 800.0).center();
    }
    let window = builder.build()?;
    if fullscreen_on.is_some() {
        let _ = window.set_fullscreen(true);
    }
    shell.set_context(
        &label,
        WindowContext {
            app: kind.to_owned(),
            doc: None,
            dirty: false,
        },
    );
    crate::menu::apply(&window, &shell, kind);
    Ok(window)
}

fn first_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.webview_windows()
        .into_values()
        .find(|w| !w.label().starts_with("show-"))
}

/// Shows `route` in `window` (a Kiln Home window opening a document).
pub fn navigate<R: Runtime>(window: &WebviewWindow<R>, route: &str) {
    if let Ok(js) = serde_json::to_string(route) {
        let _ = window.eval(format!("window.location.hash = {js};"));
    }
}

/// Whether `window` shows Kiln Home (and so can show a document instead of opening another
/// window for it).
fn shows_home<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    window
        .url()
        .ok()
        .is_none_or(|u| route_app(u.fragment().unwrap_or("")) == "home")
}

/// Opens documents: each in a window of its own, the first in `into` when that shows Kiln Home.
/// A file Kiln cannot open is reported in a message, with what went wrong.
pub async fn open_paths<R: Runtime>(
    app: AppHandle<R>,
    paths: Vec<PathBuf>,
    into: Option<WebviewWindow<R>>,
) {
    let mut target = into.filter(shows_home);
    for path in paths {
        match open_document(&app, &path).await {
            Ok(summary) => {
                let route = doc_route(&summary);
                let app2 = app.clone();
                let reuse = target.take();
                let title = summary.title.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(w) = reuse {
                        navigate(&w, &route);
                        let _ = w.set_focus();
                    } else if let Ok(w) = open(&app2, &route, &title) {
                        let _ = w.set_focus();
                    }
                });
            }
            Err(why) => {
                // At startup the message waits (a few seconds at most) for the page to say which
                // language the interface is in.
                let shell = Arc::clone(&app.state::<Arc<Shell>>());
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    for _ in 0..50 {
                        if shell.language_known() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                })
                .await;
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || cannot_open(&app2, &path, why));
            }
        }
    }
}

/// Why a file given to Kiln did not open, as the person is told (`menu.json` strings, translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFailure {
    /// Not one of the document types Kiln opens.
    NotADocument,
    /// The file is not there (moved, renamed, deleted).
    NotFound,
    /// The file may not be read.
    NoPermission,
    /// The file is damaged, or not what its name says.
    Damaged,
    /// A format or a feature of the file Kiln cannot open.
    Unsupported,
    /// Anything else that stopped reading it.
    Unreadable,
}

impl OpenFailure {
    /// From reading the file.
    #[must_use]
    pub fn from_io(e: &std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::NoPermission,
            std::io::ErrorKind::IsADirectory => Self::NotADocument,
            _ => Self::Unreadable,
        }
    }

    /// From the core's answer to opening it.
    #[must_use]
    pub fn from_core(e: &kiln_common::KilnError) -> Self {
        use kiln_common::ErrorKind as K;
        match e.kind() {
            K::Format | K::InvalidInput => Self::Damaged,
            K::Unsupported => Self::Unsupported,
            K::NotFound => Self::NotFound,
            _ => Self::Unreadable,
        }
    }

    /// The message's key in `menu.json`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::NotADocument => "notADocument",
            Self::NotFound => "openNotFound",
            Self::NoPermission => "openNoPermission",
            Self::Damaged => "openDamaged",
            Self::Unsupported => "openUnsupported",
            Self::Unreadable => "openUnreadable",
        }
    }
}

/// Reads and opens one document in the core.
async fn open_document<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<DocSummary, OpenFailure> {
    if !files::opens(path) {
        return Err(OpenFailure::NotADocument);
    }
    let bytes = std::fs::read(path).map_err(|e| OpenFailure::from_io(&e))?;
    let hint = OpenHint {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
        path: Some(path.to_string_lossy().into_owned()),
        format: None,
        read_only: false,
        kind: None,
        template: false,
    };
    let session = app.state::<Kiln>().session.clone();
    session
        .open(bytes, hint)
        .await
        .map_err(|e| OpenFailure::from_core(&e))
}

/// The text of the message about a file that did not open: what and why, in the interface
/// language (no system or parser error text).
#[must_use]
pub fn cannot_open_text(shell: &Shell, name: &str, why: OpenFailure) -> String {
    format!(
        "{}\n\n{}",
        shell.string("couldNotOpen").replace("{name}", name),
        shell.string(why.key())
    )
}

fn cannot_open<R: Runtime>(app: &AppHandle<R>, path: &Path, why: OpenFailure) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    let shell = app.state::<Arc<Shell>>();
    let name = path
        .file_name()
        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned();
    let mut dialog = app
        .dialog()
        .message(cannot_open_text(&shell, &name, why))
        .title(shell.string("couldNotOpenTitle"))
        .kind(MessageDialogKind::Error);
    if let Some(w) = first_window(app) {
        dialog = dialog.parent(&w);
    }
    dialog.show(|_| ());
}

/// Window events: files dropped on a window open (documents only), closing a document window
/// asks its page first, a closed window is forgotten, and on macOS the menu bar follows the
/// window in front.
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    let app = window.app_handle().clone();
    let shell = Arc::clone(&app.state::<Arc<Shell>>());
    let label = window.label().to_owned();
    match event {
        WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
            let docs: Vec<PathBuf> = paths.iter().filter(|p| files::opens(p)).cloned().collect();
            if docs.is_empty() {
                return;
            }
            let into = app.get_webview_window(&label);
            tauri::async_runtime::spawn(open_paths(app, docs, into));
        }
        WindowEvent::CloseRequested { api, .. } => {
            let ctx = shell.context(&label);
            let has_doc = ctx
                .as_ref()
                .is_some_and(|c| c.doc.is_some() && c.app != "present");
            // The page closes the document first (asking about unsaved changes), then the window.
            if has_doc && !shell.take_agreed(&label) {
                api.prevent_close();
                let _ = app.emit_to(label.as_str(), CLOSE_EVENT, ());
            }
        }
        WindowEvent::Destroyed => {
            shell.forget(&label);
        }
        #[cfg(target_os = "macos")]
        WindowEvent::Focused(true) => {
            if let Some(w) = app.get_webview_window(&label) {
                let ctx = shell.context(&label).map(|c| c.app).unwrap_or_default();
                crate::menu::apply(&w, &shell, &ctx);
            }
        }
        _ => {}
    }
}

/// Quit: every window closes as its close button would, so unsaved changes are asked about;
/// the app ends with its last window, once no document with unsaved changes is left without one
/// ([`exit_guard`]).
pub fn quit<R: Runtime>(app: &AppHandle<R>) {
    for w in app.webview_windows().into_values() {
        let _ = w.close();
    }
}

/// The document of a route (`#/write?doc=3` → 3).
#[must_use]
pub fn doc_of_route(route: &str) -> Option<u64> {
    let (_, query) = route.split_once('?')?;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("doc="))
        .and_then(|n| n.parse().ok())
}

/// Documents with unsaved changes that no window shows: they must get a window (and the
/// question about their changes) before Kiln may end.
#[must_use]
pub fn orphans(open: &[DocSummary], shown: &[u64]) -> Vec<DocSummary> {
    open.iter()
        .filter(|d| d.dirty && !shown.contains(&u64::from(d.doc.0)))
        .cloned()
        .collect()
}

/// Shows a route the page asks for in a window of its own (a document opened, made new or made
/// from a template in a document window): the window that already shows that document comes to
/// the front instead, if there is one.
///
/// # Errors
///
/// When the window cannot be created, or the route is not a document or Kiln Home.
pub fn show_route<R: Runtime>(app: &AppHandle<R>, route: &str) -> tauri::Result<()> {
    let kind = route_app(route);
    if kind == "present" {
        return Err(tauri::Error::InvalidWebviewUrl(
            "a slide show opens through the core",
        ));
    }
    if let Some(doc) = doc_of_route(route) {
        let shell = app.state::<Arc<Shell>>();
        let showing = app.webview_windows().into_values().find(|w| {
            shell
                .context(w.label())
                .is_some_and(|c| c.doc == Some(doc) && c.app != "present")
        });
        if let Some(w) = showing {
            let _ = w.unminimize();
            return w.set_focus();
        }
    }
    let w = open(app, route, "Kiln")?;
    w.set_focus()
}

/// Whether Kiln may end now: every document with unsaved changes the core holds is shown by a
/// window. The others get a window each, which asks about the changes as soon as it shows the
/// document (as Quit does), and Kiln stays.
pub async fn exit_guard<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(kiln) = app.try_state::<Kiln>() else {
        return true;
    };
    let session = kiln.session.clone();
    let open = match session.query(None, kiln_protocol::Query::Documents).await {
        Ok(kiln_protocol::QueryResult::Documents(d)) => d,
        // The core has ended: nothing is left to save.
        _ => return true,
    };
    let shell = Arc::clone(&app.state::<Arc<Shell>>());
    let shown: Vec<u64> = app
        .webview_windows()
        .keys()
        .filter_map(|l| shell.context(l).and_then(|c| c.doc))
        .collect();
    let left = orphans(&open, &shown);
    if left.is_empty() {
        return true;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        for d in left {
            if let Ok(w) = open_window_for(&app2, &d) {
                shell.ask_close_when_ready(w.label());
                let _ = w.set_focus();
            }
        }
    });
    false
}

fn open_window_for<R: Runtime>(
    app: &AppHandle<R>,
    d: &DocSummary,
) -> tauri::Result<WebviewWindow<R>> {
    open(app, &doc_route(d), &d.title)
}

/// A second launch of Kiln (single instance): its files open here, or a new Kiln Home window
/// comes up when it names none.
pub fn second_instance<R: Runtime>(app: &AppHandle<R>, argv: Vec<String>, cwd: String) {
    let options = crate::cli::Options::parse(argv.into_iter().skip(1), Path::new(&cwd));
    if options.files.is_empty() {
        if let Ok(w) = open(app, "#/home", "Kiln") {
            let _ = w.set_focus();
        }
        return;
    }
    tauri::async_runtime::spawn(open_paths(app.clone(), options.files, None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_common::DocId;
    use kiln_protocol::App;

    fn summary(kind: DocKind, doc: u32) -> DocSummary {
        DocSummary {
            doc: DocId(doc),
            kind,
            app: App::Write,
            title: "Report".into(),
            path: None,
            format: None,
            revision: 0,
            dirty: false,
            read_only: false,
            can_undo: false,
            can_redo: false,
            undo_label: None,
            redo_label: None,
            save_formats: Vec::new(),
            autosave: kiln_protocol::AutoSave::Off,
        }
    }

    #[test]
    fn documents_open_on_their_apps_route() {
        assert_eq!(doc_route(&summary(DocKind::Write, 3)), "#/write?doc=3");
        assert_eq!(doc_route(&summary(DocKind::Grid, 4)), "#/grid?doc=4");
        assert_eq!(doc_route(&summary(DocKind::Deck, 5)), "#/deck?doc=5");
    }

    #[test]
    fn quitting_finds_the_unsaved_documents_no_window_shows() {
        let mut a = summary(DocKind::Write, 1);
        a.dirty = true;
        let mut b = summary(DocKind::Grid, 2);
        b.dirty = true;
        let c = summary(DocKind::Deck, 3);
        // Document 1 is in a window; 2 was left behind by New in its window; 3 has no changes.
        let left = orphans(&[a, b.clone(), c], &[1]);
        assert_eq!(left, vec![b]);
        assert!(orphans(&[], &[]).is_empty());
        assert_eq!(doc_of_route("#/grid?doc=12"), Some(12));
        assert_eq!(doc_of_route("#/present?doc=3&view=audience"), Some(3));
        assert_eq!(doc_of_route("#/write?new=17"), None);
        assert_eq!(doc_of_route("#/home"), None);
    }

    #[test]
    fn a_file_that_does_not_open_is_explained_without_system_errors() {
        use kiln_common::KilnError;
        use std::io::{Error, ErrorKind as Io};
        assert_eq!(
            OpenFailure::from_io(&Error::from(Io::NotFound)),
            OpenFailure::NotFound
        );
        assert_eq!(
            OpenFailure::from_io(&Error::from(Io::PermissionDenied)),
            OpenFailure::NoPermission
        );
        assert_eq!(
            OpenFailure::from_io(&Error::other("disk")),
            OpenFailure::Unreadable
        );
        let damaged = KilnError::Format {
            format: "docx".into(),
            message: "not a zip package".into(),
        };
        assert_eq!(OpenFailure::from_core(&damaged), OpenFailure::Damaged);
        assert_eq!(
            OpenFailure::from_core(&KilnError::Unsupported("encrypted".into())),
            OpenFailure::Unsupported
        );
        let shell = Shell::new();
        for why in [
            OpenFailure::NotADocument,
            OpenFailure::NotFound,
            OpenFailure::NoPermission,
            OpenFailure::Damaged,
            OpenFailure::Unsupported,
            OpenFailure::Unreadable,
        ] {
            for lang in ["en", "de", "ar"] {
                shell.set_language(lang);
                let text = cannot_open_text(&shell, "Report.docx", why);
                assert!(text.contains("Report.docx"), "{lang} {why:?}");
                assert!(
                    !text.contains("{name}") && !text.contains("os error"),
                    "{text}"
                );
                assert_ne!(
                    shell.string(why.key()),
                    why.key(),
                    "{lang} {why:?}: no message"
                );
            }
        }
        shell.set_language("en");
        assert!(
            cannot_open_text(&shell, "a.docx", OpenFailure::NotFound)
                .starts_with("Kiln could not open a.docx.")
        );
    }

    #[test]
    fn windows_of_a_kind_share_a_remembered_place() {
        assert_eq!(state_key("main"), "main");
        assert_eq!(state_key("doc-4"), "document");
        assert_eq!(state_key("show-2"), "show");
        assert!(nonce() > 1_700_000_000_000);
    }
}
