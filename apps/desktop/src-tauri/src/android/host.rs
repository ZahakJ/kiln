// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Android host: what the core asks of the outside world, answered in the Rust process as on
//! the desktop, so no request reaches the page.
//!
//! - Documents behind content URIs (the storage access framework, files other apps hand to Kiln)
//!   are read and written through `tauri-plugin-fs`, which opens them through Android's content
//!   resolver; Kiln names them with handles ([`logic::handle`]) that end in the document's name,
//!   so the core finds the name and the format as it does for files. Their size and date come
//!   from the provider. Files in the app's own folders go through [`NativeHost`] (inside
//!   `StoredHost`, which this host wraps: the stamps and watches of content URIs never reach it).
//! - Open and Save As are the storage access framework's dialogs, with lasting access to the
//!   document picked (Recent files open it again after a restart).
//! - The clipboard with its plain text and HTML.
//! - Print: the PDF the core writes goes to Android's print dialog (PrintManager), named after the
//!   document.
//! - Links open in the system's handler for web pages and e-mail addresses only.

use std::path::Path;

use kiln_common::ErrorKind;
use kiln_protocol::{ErrorInfo, FileStamp, HostOp, HostReply, RequestId};
use kiln_session::{Deliver, Host};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_fs::FsExt;

use super::logic;
use super::plugin::{Android, Refused};
use crate::shell::Shell;

fn failed(kind: ErrorKind, message: impl Into<String>) -> HostReply {
    HostReply::Failed(ErrorInfo {
        kind,
        message: message.into(),
    })
}

/// The Android host around the app storage's host.
pub struct AndroidHost<R: Runtime> {
    app: AppHandle<R>,
    inner: Box<dyn Host>,
    deliver: Option<Deliver>,
}

impl<R: Runtime> std::fmt::Debug for AndroidHost<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndroidHost").finish_non_exhaustive()
    }
}

impl<R: Runtime> AndroidHost<R> {
    /// The host of the app, around `inner` (`StoredHost` on the app's data folder).
    #[must_use]
    pub fn new(app: AppHandle<R>, inner: Box<dyn Host>) -> Self {
        Self {
            app,
            inner,
            deliver: None,
        }
    }

    fn text(&self, key: &str) -> String {
        self.app
            .try_state::<std::sync::Arc<Shell>>()
            .map_or_else(|| key.to_owned(), |s| s.string(key))
    }

    /// A content URI's document, read whole.
    fn read(&self, handle: &str) -> HostReply {
        let Ok(url) = tauri::Url::parse(logic::uri_of(handle)) else {
            return failed(ErrorKind::NotFound, self.text("openNotFound"));
        };
        self.app.fs().read(url).map_or_else(
            |e| self.io_failure(&e, "openNoPermission"),
            HostReply::Bytes,
        )
    }

    /// A content URI's document, replaced (the provider has no atomic replace; the bytes are
    /// written in one go and flushed).
    fn write(&self, handle: &str, bytes: &[u8]) -> HostReply {
        use std::io::Write;
        let Ok(url) = tauri::Url::parse(logic::uri_of(handle)) else {
            return failed(ErrorKind::NotFound, self.text("openNotFound"));
        };
        let mut options = tauri_plugin_fs::OpenOptions::new();
        options.read(false).write(true).truncate(true);
        let result = self
            .app
            .fs()
            .open(url, options)
            .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()));
        result.map_or_else(
            |e| self.io_failure(&e, "androidNoWrite"),
            |()| HostReply::Done,
        )
    }

    fn io_failure(&self, e: &std::io::Error, denied: &str) -> HostReply {
        match e.kind() {
            std::io::ErrorKind::NotFound => failed(ErrorKind::NotFound, self.text("openNotFound")),
            std::io::ErrorKind::PermissionDenied => failed(ErrorKind::Io, self.text(denied)),
            // The fs plugin reports Android's refusals (SecurityException) as plain errors.
            _ if e.to_string().contains("ermission") => failed(ErrorKind::Io, self.text(denied)),
            _ => failed(ErrorKind::Io, self.text("openUnreadable")),
        }
    }

    fn plugin(&self) -> Option<tauri::State<'_, Android<R>>> {
        self.app.try_state::<Android<R>>()
    }

    /// The size and date of a content URI's document: `Empty` when it is gone, `Unsupported`
    /// when the provider does not say (the core then writes without the check).
    fn stat(&self, handle: &str) -> HostReply {
        let Some(android) = self.plugin() else {
            return failed(ErrorKind::Unsupported, "no stamp");
        };
        match android.stat(logic::uri_of(handle)) {
            Ok(s) if s.exists == Some(false) => HostReply::Empty,
            Ok(s) => match (s.size, s.modified) {
                (Some(size), Some(modified_ms)) => {
                    HostReply::Stamp(FileStamp { size, modified_ms })
                }
                _ => failed(ErrorKind::Unsupported, "no stamp"),
            },
            Err(_) => failed(ErrorKind::Io, self.text("openNoPermission")),
        }
    }

    /// Runs `work` on a thread of its own (Android's dialogs and services answer later) and
    /// delivers its reply.
    fn later(
        &self,
        id: RequestId,
        work: impl FnOnce(&AppHandle<R>) -> HostReply + Send + 'static,
    ) -> bool {
        let Some(deliver) = self.deliver.clone() else {
            return false;
        };
        let app = self.app.clone();
        std::thread::spawn(move || deliver(id, work(&app)));
        true
    }

    fn print(app: &AppHandle<R>, path: &str) -> HostReply {
        let text = |k: &str| app.state::<std::sync::Arc<Shell>>().string(k);
        let Some(android) = app.try_state::<Android<R>>() else {
            return failed(ErrorKind::Unavailable, text("androidPrintFailed"));
        };
        android
            .print(path, &logic::print_job_name(path))
            .map_or_else(
                |_| failed(ErrorKind::Unavailable, text("androidPrintFailed")),
                |()| HostReply::Done,
            )
    }
}

fn shell_text<R: Runtime>(app: &AppHandle<R>, key: &str) -> String {
    app.try_state::<std::sync::Arc<Shell>>()
        .map_or_else(|| key.to_owned(), |s| s.string(key))
}

/// The reply to a picker: the document's handle, or `Cancelled`.
fn chosen<R: Runtime>(
    app: &AppHandle<R>,
    result: Result<super::plugin::Chosen, Refused>,
) -> HostReply {
    match result {
        Ok(c) => c.uri.map_or(HostReply::Cancelled, |uri| {
            HostReply::Path(logic::handle(&uri, &c.name))
        }),
        Err(_) => failed(ErrorKind::Unavailable, shell_text(app, "openUnreadable")),
    }
}

impl<R: Runtime> Host for AndroidHost<R> {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        match op {
            HostOp::ReadFile { path } if logic::is_content(path) => Some(self.read(path)),
            HostOp::WriteFile { path, bytes } if logic::is_content(path) => {
                Some(self.write(path, bytes))
            }
            HostOp::StatFile { path } if logic::is_content(path) => Some(self.stat(path)),
            // Providers do not report changes; the stamp before each save stands in.
            HostOp::WatchFile { path } if logic::is_content(path) => None,
            HostOp::UnwatchFile { path } if logic::is_content(path) => Some(HostReply::Done),
            HostOp::OpenExternal { url } if !logic::may_open_url(url) => {
                Some(failed(ErrorKind::InvalidInput, self.text("androidLinks")))
            }
            HostOp::OpenWindow { .. } => Some(failed(
                ErrorKind::Unsupported,
                self.text("androidOneScreen"),
            )),
            _ => self.inner.request(op),
        }
    }

    fn request_later(&mut self, id: RequestId, op: &HostOp) -> bool {
        match op {
            // Never answered: the core asks for the stamp before each save instead.
            HostOp::WatchFile { path } if logic::is_content(path) => true,
            HostOp::PickOpen { filters } => {
                let mimes = logic::picker_mimes(filters);
                self.later(id, move |app| {
                    let result = app
                        .try_state::<Android<R>>()
                        .ok_or_else(|| Refused("unavailable".into()))
                        .and_then(|a| a.open_document(&mimes));
                    chosen(app, result)
                })
            }
            HostOp::PickSave {
                suggested_name,
                filters,
            } => {
                let mime = logic::save_mime(suggested_name, filters).to_owned();
                let name = logic::safe_name(suggested_name);
                self.later(id, move |app| {
                    let result = app
                        .try_state::<Android<R>>()
                        .ok_or_else(|| Refused("unavailable".into()))
                        .and_then(|a| a.create_document(&name, &mime));
                    chosen(app, result)
                })
            }
            HostOp::ClipboardRead => self.later(id, |app| {
                match app.try_state::<Android<R>>().map(|a| a.clipboard_read()) {
                    Some(Ok(c)) => HostReply::Clipboard {
                        text: c.text,
                        html: c.html.filter(|h| !h.is_empty()),
                    },
                    _ => failed(ErrorKind::Unavailable, "the clipboard is not available"),
                }
            }),
            HostOp::ClipboardWrite { text, html } => {
                let (text, html) = (text.clone(), html.clone());
                self.later(id, move |app| {
                    match app
                        .try_state::<Android<R>>()
                        .map(|a| a.clipboard_write(&text, html.as_deref()))
                    {
                        Some(Ok(())) => HostReply::Done,
                        _ => failed(ErrorKind::Unavailable, "the clipboard is not available"),
                    }
                })
            }
            HostOp::OpenExternal { url } => {
                let url = url.clone();
                self.later(id, move |app| {
                    use tauri_plugin_opener::OpenerExt;
                    app.opener().open_url(url, None::<&str>).map_or_else(
                        |_| failed(ErrorKind::Unavailable, shell_text(app, "androidNoApp")),
                        |()| HostReply::Done,
                    )
                })
            }
            // The core prints by writing a PDF and asking for it to be opened (`print::write_temp`).
            HostOp::OpenFile { path } if logic::is_print_file(path) => {
                let path = path.clone();
                self.later(id, move |app| Self::print(app, &path))
            }
            HostOp::OpenFile { path } => {
                let path = path.clone();
                self.later(id, move |app| {
                    // Only what Kiln wrote to its own temporary folder can be handed to an app.
                    let inside = std::env::temp_dir().canonicalize().is_ok_and(|tmp| {
                        Path::new(&path)
                            .canonicalize()
                            .is_ok_and(|p| p.starts_with(tmp) && p.is_file())
                    });
                    if !inside {
                        return failed(ErrorKind::InvalidInput, shell_text(app, "androidLinks"));
                    }
                    match app
                        .try_state::<Android<R>>()
                        .map(|a| a.view(&path, logic::mime_of_name(&path)))
                    {
                        Some(Ok(())) => HostReply::Done,
                        _ => failed(ErrorKind::Unavailable, shell_text(app, "androidNoApp")),
                    }
                })
            }
            HostOp::PrintPdf { name, bytes } => {
                let (name, bytes) = (name.clone(), bytes.clone());
                self.later(id, move |app| {
                    let pdf = kiln_protocol::PdfFile {
                        name,
                        bytes,
                        pdf_ua: false,
                        ua_issues: Vec::new(),
                    };
                    match kiln_session::print::write_temp(&pdf) {
                        Ok(path) => Self::print(app, &path),
                        Err(_) => failed(ErrorKind::Io, shell_text(app, "androidPrintFailed")),
                    }
                })
            }
            _ => self.inner.request_later(id, op),
        }
    }

    fn attach(&mut self, deliver: Deliver) {
        self.deliver = Some(deliver.clone());
        self.inner.attach(deliver);
    }

    fn has_storage(&self) -> bool {
        self.inner.has_storage()
    }

    fn now_ms(&self) -> u64 {
        self.inner.now_ms()
    }
}
