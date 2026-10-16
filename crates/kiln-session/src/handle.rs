// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session thread (native targets): one thread owns the [`Session`]; callers send it jobs over
//! a `flume` channel and await one-shot replies (PLAN.md §3.8). The Tauri IPC handlers use the
//! async methods, tests and the CLI the blocking ones.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    A11yTree, Command, DocKind, DocSummary, Event, ExecResult, HostReply, OpenHint, Query,
    QueryResult, RequestId, SceneDelta, Viewport,
};

use crate::host::Host;
use crate::session::Session;

type Job = Box<dyn FnOnce(&mut Session, &mut Vec<flume::Sender<Event>>) + Send>;

/// A cloneable handle to a session running on its own thread. The thread ends when the last
/// handle is dropped (or [`SessionHandle::shutdown`] is called).
#[derive(Debug, Clone)]
pub struct SessionHandle {
    jobs: flume::Sender<Job>,
}

/// Joins the session thread.
#[derive(Debug)]
pub struct SessionThread {
    thread: JoinHandle<()>,
}

impl SessionThread {
    /// Waits for the session thread to end (after every handle was dropped).
    ///
    /// # Errors
    ///
    /// `Internal` if the thread panicked outside a job.
    pub fn join(self) -> Result<()> {
        self.thread
            .join()
            .map_err(|_| KilnError::Internal("the session thread panicked".into()))
    }
}

/// Turns a panic payload into an error message.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

impl SessionHandle {
    /// Starts a session with the built-in apps on a new thread named `kiln-session`.
    ///
    /// # Errors
    ///
    /// `Internal` when the operating system cannot create the thread.
    pub fn spawn(host: Box<dyn Host>) -> Result<(Self, SessionThread)> {
        Self::spawn_with(move || Session::new(host))
    }

    /// Starts a session built by `make` on a new thread.
    ///
    /// # Errors
    ///
    /// `Internal` when the operating system cannot create the thread.
    pub fn spawn_with(
        make: impl FnOnce() -> Session + Send + 'static,
    ) -> Result<(Self, SessionThread)> {
        let (jobs, rx) = flume::unbounded::<Job>();
        let thread = std::thread::Builder::new()
            .name("kiln-session".into())
            .spawn(move || {
                let mut session = make();
                let mut subscribers: Vec<flume::Sender<Event>> = Vec::new();
                while let Ok(job) = rx.recv() {
                    // A job that panics fails alone; its reply channel is dropped, so the caller
                    // sees an error, and the session keeps serving.
                    if let Err(payload) =
                        catch_unwind(AssertUnwindSafe(|| job(&mut session, &mut subscribers)))
                    {
                        let message = panic_message(payload.as_ref());
                        session_error_event(&mut subscribers, &message);
                    }
                    if session.has_events() {
                        let events = session.poll();
                        subscribers.retain(|s| events.iter().all(|e| s.send(e.clone()).is_ok()));
                    }
                }
            })
            .map_err(|e| KilnError::Internal(format!("cannot start the session thread: {e}")))?;
        Ok((Self { jobs }, SessionThread { thread }))
    }

    /// Runs `f` on the session thread and returns its reply receiver.
    fn submit<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Session) -> R + Send + 'static,
    ) -> Result<flume::Receiver<R>> {
        let (tx, rx) = flume::bounded(1);
        let job: Job = Box::new(move |session, _| {
            // The caller may have given up waiting; that is not an error.
            let _ = tx.send(f(session));
        });
        self.jobs.send(job).map_err(|_| KilnError::Closed)?;
        Ok(rx)
    }

    /// Runs `f` on the session thread and waits for its result.
    ///
    /// # Errors
    ///
    /// `Closed` when the session thread has ended, `Internal` when `f` panicked.
    pub fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Session) -> R + Send + 'static,
    ) -> Result<R> {
        self.submit(f)?.recv().map_err(|_| lost())
    }

    /// Runs `f` on the session thread and awaits its result without blocking the caller's thread.
    ///
    /// # Errors
    ///
    /// As [`SessionHandle::call`].
    pub async fn call_async<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Session) -> R + Send + 'static,
    ) -> Result<R> {
        self.submit(f)?.recv_async().await.map_err(|_| lost())
    }

    /// Subscribes to every event the session emits from now on. Events are delivered in order;
    /// a subscriber whose receiver is dropped is removed.
    ///
    /// # Errors
    ///
    /// `Closed` when the session thread has ended.
    pub fn subscribe(&self) -> Result<flume::Receiver<Event>> {
        let (tx, rx) = flume::unbounded();
        let (done_tx, done_rx) = flume::bounded::<()>(1);
        let job: Job = Box::new(move |_, subscribers| {
            subscribers.push(tx);
            let _ = done_tx.send(());
        });
        self.jobs.send(job).map_err(|_| KilnError::Closed)?;
        done_rx.recv().map_err(|_| lost())?;
        Ok(rx)
    }

    /// Ends the session thread after the jobs already queued.
    pub fn shutdown(self) {
        drop(self);
    }

    // ----- typed async API (used by the Tauri IPC handlers) ---------------------------------------

    /// [`Session::execute`].
    ///
    /// # Errors
    ///
    /// As [`Session::execute`], plus `Closed`.
    pub async fn execute(&self, doc: Option<DocId>, cmd: Command) -> Result<ExecResult> {
        self.call_async(move |s| s.execute(doc, cmd)).await?
    }

    /// [`Session::query`].
    ///
    /// # Errors
    ///
    /// As [`Session::query`], plus `Closed`.
    pub async fn query(&self, doc: Option<DocId>, query: Query) -> Result<QueryResult> {
        self.call_async(move |s| s.query(doc, &query)).await?
    }

    /// [`Session::open`].
    ///
    /// # Errors
    ///
    /// As [`Session::open`], plus `Closed`.
    pub async fn open(&self, bytes: Vec<u8>, hint: OpenHint) -> Result<DocSummary> {
        self.call_async(move |s| s.open(&bytes, hint)).await?
    }

    /// [`Session::new_document`].
    ///
    /// # Errors
    ///
    /// As [`Session::new_document`], plus `Closed`.
    pub async fn new_document(&self, kind: DocKind) -> Result<DocSummary> {
        self.call_async(move |s| s.new_document(kind)).await?
    }

    /// [`Session::set_viewport`].
    ///
    /// # Errors
    ///
    /// As [`Session::set_viewport`], plus `Closed`.
    pub async fn set_viewport(&self, doc: DocId, viewport: Viewport) -> Result<()> {
        self.call_async(move |s| s.set_viewport(doc, viewport))
            .await?
    }

    /// [`Session::frame`].
    ///
    /// # Errors
    ///
    /// As [`Session::frame`], plus `Closed`.
    pub async fn frame(&self, doc: DocId) -> Result<SceneDelta> {
        self.call_async(move |s| s.frame(doc)).await?
    }

    /// [`Session::a11y`].
    ///
    /// # Errors
    ///
    /// As [`Session::a11y`], plus `Closed`.
    pub async fn a11y(&self, doc: DocId) -> Result<A11yTree> {
        self.call_async(move |s| s.a11y(doc)).await?
    }

    /// [`Session::host_reply`].
    ///
    /// # Errors
    ///
    /// As [`Session::host_reply`], plus `Closed`.
    pub async fn host_reply(&self, id: RequestId, reply: HostReply) -> Result<()> {
        self.call_async(move |s| s.host_reply(id, reply)).await?
    }
}

fn lost() -> KilnError {
    KilnError::Internal("the session dropped the request (a bug in the core; see the log)".into())
}

fn session_error_event(subscribers: &mut Vec<flume::Sender<Event>>, message: &str) {
    let event = Event::Error {
        doc: None,
        error: KilnError::Internal(message.to_owned()).into(),
    };
    subscribers.retain(|s| s.send(event.clone()).is_ok());
}
