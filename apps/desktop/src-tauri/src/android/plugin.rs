// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Kotlin side of the shell (`KilnPlugin.kt` in the generated project), registered as the
//! `kiln-android` plugin and called from Rust. Every call blocks until Android answers, so the
//! host makes them from threads of its own, never from the session thread.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

/// The package of the activity and of `KilnPlugin`.
const PACKAGE: &str = "io.github.zahakj.kiln";

/// The plugin's handle, managed by the app.
pub struct Android<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> std::fmt::Debug for Android<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Android")
    }
}

/// Registers the plugin.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("kiln-android")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PACKAGE, "KilnPlugin")?;
            app.manage(Android(handle));
            Ok(())
        })
        .build()
}

/// Why a call failed: Android's answer (`missing`, `noApp`, `denied`) or the bridge's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A document picked or created through the storage access framework.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chosen {
    /// Its content URI; `None` when the person cancelled.
    pub uri: Option<String>,
    /// The name the provider gives it.
    #[serde(default)]
    pub name: String,
}

/// The size and modification time of a document behind a content URI.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Stat {
    /// `Some(false)` when the document is gone; `None` when the provider cannot tell.
    pub exists: Option<bool>,
    /// Bytes.
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
}

/// The clipboard.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Clip {
    /// Plain text.
    #[serde(default)]
    pub text: String,
    /// The HTML flavour.
    pub html: Option<String>,
}

/// An answer with nothing in it (`null` or `{}`).
type Nothing = serde_json::Value;

impl<R: Runtime> Android<R> {
    fn call<T: DeserializeOwned>(
        &self,
        command: &str,
        payload: impl Serialize,
    ) -> Result<T, Refused> {
        self.0
            .run_mobile_plugin(command, payload)
            .map_err(|e| match e {
                tauri::plugin::mobile::PluginInvokeError::InvokeRejected(r) => {
                    Refused(r.message.unwrap_or_default())
                }
                other => Refused(other.to_string()),
            })
    }

    /// The Open dialog of the storage access framework.
    ///
    /// # Errors
    ///
    /// When Android cannot show it.
    pub fn open_document(&self, mime_types: &[String]) -> Result<Chosen, Refused> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            mime_types: &'a [String],
        }
        self.call("openDocument", Args { mime_types })
    }

    /// The Save As dialog: a new document named `name`.
    ///
    /// # Errors
    ///
    /// When Android cannot show it.
    pub fn create_document(&self, name: &str, mime_type: &str) -> Result<Chosen, Refused> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            name: &'a str,
            mime_type: &'a str,
        }
        self.call("createDocument", Args { name, mime_type })
    }

    /// The size and date of a document.
    ///
    /// # Errors
    ///
    /// `denied` when Kiln may no longer read it.
    pub fn stat(&self, uri: &str) -> Result<Stat, Refused> {
        #[derive(Serialize)]
        struct Args<'a> {
            uri: &'a str,
        }
        self.call("stat", Args { uri })
    }

    /// The clipboard's text and HTML.
    ///
    /// # Errors
    ///
    /// When the bridge fails.
    pub fn clipboard_read(&self) -> Result<Clip, Refused> {
        self.call("clipboardRead", ())
    }

    /// Puts text, and HTML when given, on the clipboard.
    ///
    /// # Errors
    ///
    /// When the bridge fails.
    pub fn clipboard_write(&self, text: &str, html: Option<&str>) -> Result<(), Refused> {
        #[derive(Serialize)]
        struct Args<'a> {
            text: &'a str,
            html: Option<&'a str>,
        }
        self.call::<Nothing>("clipboardWrite", Args { text, html })
            .map(|_| ())
    }

    /// Prints a PDF with the system's print dialog as `job_name`.
    ///
    /// # Errors
    ///
    /// `missing` when the file is not there.
    pub fn print(&self, path: &str, job_name: &str) -> Result<(), Refused> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            path: &'a str,
            job_name: &'a str,
        }
        self.call::<Nothing>("print", Args { path, job_name })
            .map(|_| ())
    }

    /// Sends `target` (a content URI or a file in the app's cache) through the share sheet,
    /// titled `title`.
    ///
    /// # Errors
    ///
    /// `missing`, or `noApp` when no app takes it.
    pub fn share(
        &self,
        target: &str,
        mime_type: &str,
        name: &str,
        title: &str,
    ) -> Result<(), Refused> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            target: &'a str,
            mime_type: &'a str,
            name: &'a str,
            title: &'a str,
        }
        self.call::<Nothing>(
            "share",
            Args {
                target,
                mime_type,
                name,
                title,
            },
        )
        .map(|_| ())
    }

    /// Opens a file of the app's cache in the app that shows its type.
    ///
    /// # Errors
    ///
    /// `missing`, or `noApp` when no app shows it.
    pub fn view(&self, path: &str, mime_type: &str) -> Result<(), Refused> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            path: &'a str,
            mime_type: &'a str,
        }
        self.call::<Nothing>("view", Args { path, mime_type })
            .map(|_| ())
    }

    /// The colour behind the system bars, with dark icons when `light`.
    ///
    /// # Errors
    ///
    /// When the colour is not `#RRGGBB`.
    pub fn set_bars(&self, color: &str, light: bool) -> Result<(), Refused> {
        #[derive(Serialize)]
        struct Args<'a> {
            color: &'a str,
            light: bool,
        }
        self.call::<Nothing>("setBars", Args { color, light })
            .map(|_| ())
    }

    /// Puts Kiln in the background (Back on Kiln Home).
    ///
    /// # Errors
    ///
    /// When the bridge fails.
    pub fn move_task_to_back(&self) -> Result<(), Refused> {
        self.call::<Nothing>("moveTaskToBack", ()).map(|_| ())
    }

    /// Sends every document handed to Kiln (at launch and later) to `channel`.
    ///
    /// # Errors
    ///
    /// When the bridge fails.
    pub fn watch_intents(
        &self,
        channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> Result<(), Refused> {
        #[derive(Serialize)]
        struct Args {
            channel: tauri::ipc::Channel<serde_json::Value>,
        }
        self.call::<Nothing>("watchIntents", Args { channel })
            .map(|_| ())
    }
}

/// The plugin of `app`.
#[must_use]
pub fn android<R: Runtime, M: Manager<R>>(app: &M) -> Option<tauri::State<'_, Android<R>>> {
    app.try_state::<Android<R>>()
}
