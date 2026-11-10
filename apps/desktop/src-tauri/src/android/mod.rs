// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Android app: the same core and interface as the desktop in one window, with the
//! Android parts the shell adds.
//!
//! - [`host`]: documents through the storage access framework (content URIs), the clipboard,
//!   print through Android's print dialog, links.
//! - Documents other apps hand to Kiln (VIEW, EDIT, SEND) open in the window, each on its app's
//!   route; one Kiln cannot open is explained in a message, in the interface language.
//! - The system bars take the colour of the page's title bar (its `theme-color`, from the design
//!   tokens) with icons that read on it; the page lies between the bars and above the keyboard.
//! - Back: the page closes what is open (a sheet, a dialog, Backstage, then the document) and on
//!   Kiln Home puts Kiln in the background ([`leave`]).
//! - Sending a copy of a document through the share sheet ([`share`]).
//!
//! The Kotlin half is `KilnPlugin.kt` and `MainActivity.kt` in `gen/android`. [`logic`] holds
//! what needs no Android and is tested on every platform.

pub mod logic;

#[cfg(target_os = "android")]
pub mod host;
#[cfg(target_os = "android")]
pub mod plugin;

#[cfg(target_os = "android")]
pub use app::*;

#[cfg(target_os = "android")]
mod app {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    use kiln_common::{DocId, ErrorKind, KilnError};
    use kiln_protocol::{ErrorInfo, OpenHint, PrintSettings, Query, QueryResult};
    use kiln_session::{Host, NativeHost, StoredHost};
    use serde::Deserialize;
    use tauri::ipc::{Channel, InvokeResponseBody};
    use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
    use tauri_plugin_fs::FsExt;

    use super::host::AndroidHost;
    use super::logic;
    use super::plugin::Android;
    use crate::bridge::Kiln;
    use crate::shell::Shell;

    /// The one window's label.
    pub const MAIN: &str = "main";

    /// The session's host: [`AndroidHost`] around the app storage in `data_dir` (or around the
    /// files alone when that folder cannot be used: documents still open and save).
    pub fn session_host<R: Runtime>(
        app: &AppHandle<R>,
        data_dir: Option<PathBuf>,
    ) -> Box<dyn Host> {
        let inner: Box<dyn Host> =
            match data_dir.map(|dir| StoredHost::open(Box::new(NativeHost), &dir)) {
                Some(Ok(stored)) => Box::new(stored),
                _ => Box::new(NativeHost),
            };
        Box::new(AndroidHost::new(app.clone(), inner))
    }

    /// A route waiting for the page: a document handed to Kiln before Kiln Home was ready.
    #[derive(Debug, Default)]
    pub struct Routes {
        ready: AtomicBool,
        waiting: Mutex<Option<String>>,
    }

    fn navigate<R: Runtime>(app: &AppHandle<R>, route: &str) {
        if let (Some(w), Ok(js)) = (app.get_webview_window(MAIN), serde_json::to_string(route)) {
            let _ = w.eval(format!("window.location.hash = {js};"));
        }
    }

    /// Shows `route` in the window now, or once the page has started.
    pub fn show_route<R: Runtime>(app: &AppHandle<R>, route: &str) {
        let routes = app.state::<Routes>();
        if routes.ready.load(Ordering::SeqCst) {
            navigate(app, route);
        } else {
            *routes
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(route.to_owned());
        }
    }

    /// The page reports what it shows: it is ready for the routes that waited, and the system
    /// bars take its title bar's colour (`bar`, else the theme's).
    pub fn page_reported<R: Runtime>(app: &AppHandle<R>, theme: Option<&str>, bar: Option<&str>) {
        let routes = app.state::<Routes>();
        if !routes.ready.swap(true, Ordering::SeqCst) {
            let waiting = routes
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(route) = waiting {
                navigate(app, &route);
            }
        }
        let color = bar.and_then(logic::bar_color).unwrap_or_else(|| {
            if theme == Some("dark") {
                "#17110E"
            } else {
                "#F6EFE6"
            }
            .to_owned()
        });
        let light = logic::is_light(&color);
        let app = app.clone();
        // Android answers on its main thread; never wait for it on this one.
        std::thread::spawn(move || {
            if let Some(android) = app.try_state::<Android<R>>() {
                let _ = android.set_bars(&color, light);
            }
        });
    }

    /// A document another app handed to Kiln.
    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Handed {
        uri: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        mime_type: String,
        #[serde(default)]
        writable: bool,
    }

    /// Opens Kiln Home and listens for the documents other apps hand to Kiln.
    ///
    /// # Errors
    ///
    /// When the window cannot be created.
    pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
        app.manage(Routes::default());
        WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("#/home".into())).build()?;
        let handle = app.clone();
        std::thread::spawn(move || {
            let opener = handle.clone();
            let channel = Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body
                    && let Ok(doc) = serde_json::from_str::<Handed>(&json)
                {
                    tauri::async_runtime::spawn(open_handed(opener.clone(), doc));
                }
                Ok(())
            });
            if let Some(android) = handle.try_state::<Android<R>>() {
                let _ = android.watch_intents(channel);
            }
        });
        Ok(())
    }

    /// Why a handed document did not open, as the message says it (`menu.json` keys).
    fn why(e: &KilnError) -> &'static str {
        match e.kind() {
            ErrorKind::Format | ErrorKind::InvalidInput => "openDamaged",
            ErrorKind::Unsupported => "openUnsupported",
            ErrorKind::NotFound => "openNotFound",
            _ => "openUnreadable",
        }
    }

    async fn open_handed<R: Runtime>(app: AppHandle<R>, doc: Handed) {
        let name = logic::document_name(&doc.name, &doc.mime_type);
        if kiln_protocol::FileFormat::from_path(&name).is_none() {
            cannot_open(&app, &name, "notADocument");
            return;
        }
        let bytes = match tauri::Url::parse(&doc.uri).map(|u| app.fs().read(u)) {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return cannot_open(&app, &name, "openNotFound");
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return cannot_open(&app, &name, "openNoPermission");
            }
            _ => return cannot_open(&app, &name, "openUnreadable"),
        };
        // A document Kiln may not write to opens without its place: Save asks where to save.
        let hint = OpenHint {
            name: Some(name.clone()),
            path: doc.writable.then(|| logic::handle(&doc.uri, &name)),
            format: None,
            read_only: false,
            kind: None,
            template: false,
        };
        let session = app.state::<Kiln>().session.clone();
        match session.open(bytes, hint).await {
            Ok(summary) => show_route(&app, &logic::doc_route(summary.kind, summary.doc.0)),
            Err(e) => cannot_open(&app, &name, why(&e)),
        }
    }

    /// The message about a document that did not open: what and why, in the interface language.
    fn cannot_open<R: Runtime>(app: &AppHandle<R>, name: &str, why: &str) {
        use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
        let shell = Arc::clone(&app.state::<Arc<Shell>>());
        let text = format!(
            "{}\n\n{}",
            shell.string("couldNotOpen").replace("{name}", name),
            shell.string(why)
        );
        app.dialog()
            .message(text)
            .title(shell.string("couldNotOpenTitle"))
            .kind(MessageDialogKind::Error)
            .show(|_| ());
    }

    fn refused(message: String) -> ErrorInfo {
        ErrorInfo {
            kind: ErrorKind::Unavailable,
            message,
        }
    }

    /// Sends a copy of document `doc` through the share sheet: its file as it was last saved,
    /// or with `pdf` a PDF of it as it is now.
    ///
    /// # Errors
    ///
    /// A message for the person: the document has unsaved changes or was never saved, or no app
    /// takes it.
    pub async fn share<R: Runtime>(
        app: AppHandle<R>,
        doc: DocId,
        pdf: bool,
    ) -> Result<(), ErrorInfo> {
        let shell = Arc::clone(&app.state::<Arc<Shell>>());
        let session = app.state::<Kiln>().session.clone();
        let summary = match session.query(Some(doc), Query::Summary).await? {
            QueryResult::Summary(s) => s,
            _ => return Err(KilnError::Internal("no summary".into()).into()),
        };
        let share_dir = std::env::temp_dir().join("kiln-share");
        let (target, mime, name) = if pdf {
            let QueryResult::Pdf(file) = session
                .query(
                    Some(doc),
                    Query::Pdf {
                        settings: PrintSettings::default(),
                    },
                )
                .await?
            else {
                return Err(KilnError::Internal("no PDF".into()).into());
            };
            let name = logic::safe_name(&format!("{}.pdf", summary.title));
            let path = share_dir.join(&name);
            std::fs::create_dir_all(&share_dir)
                .and_then(|()| std::fs::write(&path, &file.bytes))
                .map_err(|_| refused(shell.string("androidPrintFailed")))?;
            (
                path.to_string_lossy().into_owned(),
                "application/pdf".to_owned(),
                name,
            )
        } else {
            let Some(path) = summary.path.clone().filter(|_| !summary.dirty) else {
                return Err(refused(shell.string("androidSaveFirst")));
            };
            if logic::is_content(&path) {
                let name = logic::name_of(&path).unwrap_or(logic::UNNAMED).to_owned();
                (
                    logic::uri_of(&path).to_owned(),
                    logic::mime_of_name(&name).to_owned(),
                    name,
                )
            } else {
                // A file in the app's folders: a copy in the cache, which FileProvider shares.
                let name = logic::safe_name(&std::path::Path::new(&path).file_name().map_or_else(
                    || summary.title.clone(),
                    |n| n.to_string_lossy().into_owned(),
                ));
                let copy = share_dir.join(&name);
                std::fs::create_dir_all(&share_dir)
                    .and_then(|()| std::fs::copy(&path, &copy).map(|_| ()))
                    .map_err(|_| refused(shell.string("openUnreadable")))?;
                (
                    copy.to_string_lossy().into_owned(),
                    logic::mime_of_name(&name).to_owned(),
                    name,
                )
            }
        };
        let title = shell
            .string("androidSendCopy")
            .replace("{name}", &summary.title);
        let no_app = shell.string("androidNoApp");
        tauri::async_runtime::spawn_blocking(move || {
            app.try_state::<Android<R>>().map_or(Err(()), |a| {
                a.share(&target, &mime, &name, &title).map_err(|_| ())
            })
        })
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| refused(no_app))
    }

    /// Back on Kiln Home: Kiln goes to the background, as Android's apps do.
    pub fn leave<R: Runtime>(app: &AppHandle<R>) {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Some(android) = app.try_state::<Android<R>>() {
                let _ = android.move_task_to_back();
            }
        });
    }
}
