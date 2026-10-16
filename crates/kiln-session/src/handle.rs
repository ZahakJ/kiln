// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session thread (native targets): one thread owns the [`Session`]; callers send it jobs over
//! a `flume` channel and await one-shot replies (PLAN.md §3.8). The Tauri IPC handlers use the
//! async methods, tests and the CLI the blocking ones.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::{
    A11yTree, Command, DocKind, DocSummary, Event, ExecResult, HostReply, OpenHint, Query,
    QueryResult, RequestId, SceneDelta, Viewport,
};

use crate::host::Host;
use crate::session::Session;

type Job = Box<dyn FnOnce(&mut Session, &mut Hub) + Send>;

/// How many undelivered events (other than host requests) the session keeps for a late
/// subscriber; older ones are dropped first.
const BACKLOG: usize = 256;

/// One event subscriber: everything, or one document plus the session-wide events.
#[derive(Debug)]
struct Subscriber {
    tx: flume::Sender<Event>,
    doc: Option<DocId>,
    /// Who subscribed, for subscribers that replace their previous subscription.
    owner: Option<String>,
}

impl Subscriber {
    fn wants(&self, event: &Event) -> bool {
        match (self.doc, event.doc()) {
            (Some(want), Some(about)) => want == about,
            _ => true,
        }
    }
}

/// Fans session events out to the subscribers. An event no subscriber wanted (typically one
/// emitted before the UI subscribed, such as a host request raised while the webview loads) is
/// kept and handed to the next matching subscriber, so a host request is never lost.
#[derive(Debug, Default)]
struct Hub {
    subscribers: Vec<Subscriber>,
    backlog: VecDeque<Event>,
}

impl Hub {
    fn add(&mut self, subscriber: Subscriber) {
        self.subscribers.retain(|s| {
            !s.tx.is_disconnected()
                && (subscriber.owner.is_none()
                    || s.owner != subscriber.owner
                    || s.doc != subscriber.doc)
        });
        self.subscribers.push(subscriber);
        // Nobody wanted these when they were emitted; the earlier subscribers still do not.
        for event in std::mem::take(&mut self.backlog) {
            self.publish(event);
        }
    }

    fn publish(&mut self, event: Event) {
        let mut delivered = false;
        self.subscribers.retain(|s| {
            if !s.wants(&event) {
                return true;
            }
            let alive = s.tx.send(event.clone()).is_ok();
            delivered |= alive;
            alive
        });
        if !delivered {
            self.keep(event);
        }
    }

    fn keep(&mut self, event: Event) {
        self.backlog.push_back(event);
        if self.backlog.len() > BACKLOG {
            // Host requests stay until delivered (their number is bounded by the pending
            // requests); anything else may be dropped, oldest first.
            if let Some(i) = self
                .backlog
                .iter()
                .position(|e| !matches!(e, Event::HostRequest(_)))
            {
                self.backlog.remove(i);
            }
        }
    }
}

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
                let mut hub = Hub::default();
                while let Ok(job) = rx.recv() {
                    // A job that panics fails alone: its reply channel is dropped, so the caller
                    // sees an error; a document it was changing is marked damaged (read-only, see
                    // `Session::recover_from_panic`); the session keeps serving the rest.
                    // `AssertUnwindSafe` is sound because of that recovery step.
                    if let Err(payload) =
                        catch_unwind(AssertUnwindSafe(|| job(&mut session, &mut hub)))
                    {
                        session.recover_from_panic(&panic_message(payload.as_ref()));
                    }
                    if session.has_events() {
                        for event in session.poll() {
                            hub.publish(event);
                        }
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

    /// Subscribes to the events of one document (`Some`) or of every document (`None`);
    /// session-wide events (host requests, recovery, errors without a document) reach every
    /// subscriber. The subscription covers every event of the jobs submitted after this call,
    /// plus any earlier event no subscriber has received yet. Events arrive in order; a
    /// subscriber whose receiver is dropped is removed. Never blocks.
    ///
    /// # Errors
    ///
    /// `Closed` when the session thread has ended.
    pub fn subscribe(&self, doc: Option<DocId>) -> Result<flume::Receiver<Event>> {
        self.add_subscriber(None, doc)
    }

    /// As [`SessionHandle::subscribe`], but replaces the subscription `owner` made earlier for the
    /// same `doc` (a webview that reloads subscribes again): the previous receiver is
    /// disconnected after the events already sent to it, and receives nothing more.
    ///
    /// # Errors
    ///
    /// `Closed` when the session thread has ended.
    pub fn subscribe_as(
        &self,
        owner: impl Into<String>,
        doc: Option<DocId>,
    ) -> Result<flume::Receiver<Event>> {
        self.add_subscriber(Some(owner.into()), doc)
    }

    fn add_subscriber(
        &self,
        owner: Option<String>,
        doc: Option<DocId>,
    ) -> Result<flume::Receiver<Event>> {
        let (tx, rx) = flume::unbounded();
        let job: Job = Box::new(move |_, hub| hub.add(Subscriber { tx, doc, owner }));
        self.jobs.send(job).map_err(|_| KilnError::Closed)?;
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
