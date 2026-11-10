// SPDX-License-Identifier: AGPL-3.0-or-later
//! The WebSocket and timer requests of shared documents, answered natively (tokio-tungstenite):
//! [`NetHost`] wraps any host and answers `SocketOpen`, `SocketSend`, `SocketReceive`,
//! `SocketClose` and `Timer` itself, from threads of its own that deliver the replies through the
//! session thread ([`crate::Host::attach`]). Everything else goes to the wrapped host.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::{SinkExt, StreamExt};
use kiln_common::KilnError;
use kiln_protocol::{ErrorInfo, HostOp, HostReply, RequestId};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::host::{Deliver, Host};

/// How long a connection may take to open.
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// What the session sends a socket's thread.
enum Out {
    Send(Vec<u8>),
    Close,
}

/// What a socket received and who waits for it.
#[derive(Default)]
struct Inbox {
    frames: VecDeque<Vec<u8>>,
    closed: bool,
    waiting: Option<RequestId>,
}

struct Socket {
    out: mpsc::UnboundedSender<Out>,
    inbox: Arc<Mutex<Inbox>>,
}

/// A host that answers the socket and timer requests natively and passes the rest on.
pub struct NetHost {
    inner: Box<dyn Host>,
    deliver: Option<Deliver>,
    sockets: BTreeMap<u32, Socket>,
}

impl std::fmt::Debug for NetHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetHost")
            .field("sockets", &self.sockets.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn failed(message: impl Into<String>) -> HostReply {
    HostReply::Failed(ErrorInfo::from(&KilnError::Io(message.into())))
}

fn lock(inbox: &Mutex<Inbox>) -> std::sync::MutexGuard<'_, Inbox> {
    inbox.lock().unwrap_or_else(PoisonError::into_inner)
}

impl NetHost {
    /// Wraps a host.
    #[must_use]
    pub fn new(inner: Box<dyn Host>) -> Self {
        Self {
            inner,
            deliver: None,
            sockets: BTreeMap::new(),
        }
    }

    fn open(&mut self, id: RequestId, socket: u32, url: &str, deliver: Deliver) {
        let (out, mut rx) = mpsc::unbounded_channel::<Out>();
        let inbox: Arc<Mutex<Inbox>> = Arc::default();
        self.sockets.insert(
            socket,
            Socket {
                out,
                inbox: Arc::clone(&inbox),
            },
        );
        let url = url.to_owned();
        let spawned = std::thread::Builder::new()
            .name(format!("kiln-socket-{socket}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        deliver(id, failed(e.to_string()));
                        return;
                    }
                };
                runtime.block_on(async move {
                    if url.starts_with("wss://") {
                        deliver(
                            id,
                            failed("secure connections (wss://) are not available in this build"),
                        );
                        return;
                    }
                    let connecting = tokio_tungstenite::connect_async(url.as_str());
                    let ws = match tokio::time::timeout(OPEN_TIMEOUT, connecting).await {
                        Ok(Ok((ws, _))) => ws,
                        Ok(Err(e)) => {
                            deliver(id, failed(e.to_string()));
                            return;
                        }
                        Err(_) => {
                            deliver(id, failed(format!("could not connect to {url} in time")));
                            return;
                        }
                    };
                    deliver(id, HostReply::Done);
                    let (mut sink, mut stream) = ws.split();
                    loop {
                        tokio::select! {
                            msg = stream.next() => match msg {
                                Some(Ok(Message::Binary(b))) => {
                                    let mut i = lock(&inbox);
                                    match i.waiting.take() {
                                        Some(w) => {
                                            drop(i);
                                            deliver(w, HostReply::Bytes(b.to_vec()));
                                        }
                                        None => i.frames.push_back(b.to_vec()),
                                    }
                                }
                                Some(Ok(_)) => {}
                                Some(Err(_)) | None => break,
                            },
                            out = rx.recv() => match out {
                                Some(Out::Send(b)) => {
                                    if sink.send(Message::Binary(b.into())).await.is_err() {
                                        break;
                                    }
                                }
                                Some(Out::Close) | None => {
                                    let _ = sink.close().await;
                                    break;
                                }
                            },
                        }
                    }
                    let mut i = lock(&inbox);
                    i.closed = true;
                    if let Some(w) = i.waiting.take() {
                        drop(i);
                        deliver(w, HostReply::Empty);
                    }
                });
            });
        if let Err(e) = spawned {
            if let Some(d) = &self.deliver {
                d(id, failed(e.to_string()));
            }
            self.sockets.remove(&socket);
        }
    }
}

impl Host for NetHost {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        match op {
            HostOp::SocketSend { socket, bytes } => Some(match self.sockets.get(socket) {
                Some(s) if s.out.send(Out::Send(bytes.clone())).is_ok() => HostReply::Done,
                _ => failed("the connection is closed"),
            }),
            HostOp::SocketClose { socket } => {
                if let Some(s) = self.sockets.remove(socket) {
                    let _ = s.out.send(Out::Close);
                }
                Some(HostReply::Done)
            }
            HostOp::SocketReceive { socket } => {
                let Some(s) = self.sockets.get(socket) else {
                    return Some(HostReply::Empty);
                };
                let mut i = lock(&s.inbox);
                match i.frames.pop_front() {
                    Some(f) => Some(HostReply::Bytes(f)),
                    None if i.closed => Some(HostReply::Empty),
                    None => None,
                }
            }
            HostOp::SocketOpen { .. } | HostOp::Timer { .. } => None,
            other => self.inner.request(other),
        }
    }

    fn request_later(&mut self, id: RequestId, op: &HostOp) -> bool {
        let Some(deliver) = self.deliver.clone() else {
            return false;
        };
        match op {
            HostOp::SocketOpen { socket, url } => {
                self.open(id, *socket, url, deliver);
                true
            }
            HostOp::SocketReceive { socket } => {
                let Some(s) = self.sockets.get(socket) else {
                    deliver(id, HostReply::Empty);
                    return true;
                };
                let mut i = lock(&s.inbox);
                // A frame (or the close) that came in since `request` looked.
                if let Some(f) = i.frames.pop_front() {
                    drop(i);
                    deliver(id, HostReply::Bytes(f));
                } else if i.closed {
                    drop(i);
                    deliver(id, HostReply::Empty);
                } else {
                    i.waiting = Some(id);
                }
                true
            }
            HostOp::Timer { ms } => {
                let ms = u64::from(*ms);
                std::thread::Builder::new()
                    .name("kiln-timer".into())
                    .spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(ms));
                        deliver(id, HostReply::Done);
                    })
                    .is_ok()
            }
            _ => self.inner.request_later(id, op),
        }
    }

    fn has_storage(&self) -> bool {
        self.inner.has_storage()
    }

    fn attach(&mut self, deliver: Deliver) {
        self.inner.attach(Arc::clone(&deliver));
        self.deliver = Some(deliver);
    }

    fn now_ms(&self) -> u64 {
        self.inner.now_ms()
    }
}

impl Drop for NetHost {
    fn drop(&mut self) {
        for s in self.sockets.values() {
            let _ = s.out.send(Out::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use kiln_common::DocId;
    use kiln_protocol::collab::CollabStatus;
    use kiln_protocol::{Command, DocKind, Query, QueryResult, SuiteCommand, WriteCommand};

    use crate::{NativeHost, SessionHandle};

    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn until(what: &str, mut ok: impl FnMut() -> bool) -> Result<(), Error> {
        let start = Instant::now();
        while !ok() {
            if start.elapsed() > Duration::from_secs(20) {
                return Err(format!("timed out waiting for {what}").into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    fn texts(h: &SessionHandle, doc: DocId) -> Vec<String> {
        h.call(move |s| match s.query(Some(doc), &Query::Paragraphs) {
            Ok(QueryResult::Paragraphs(p)) => p.into_iter().map(|p| p.text).collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default()
    }

    fn status(h: &SessionHandle, doc: DocId) -> Option<CollabStatus> {
        h.call(move |s| match s.query_mut(Some(doc), &Query::Collab) {
            Ok(QueryResult::Collab(c)) if c.shared => Some(c.status),
            _ => None,
        })
        .ok()
        .flatten()
    }

    #[test]
    fn two_sessions_share_a_document_through_the_relay() -> Result<(), Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp")
            .join(format!("kiln-session-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let relay =
            kiln_collab_server::Relay::new(&dir, kiln_collab_server::SharedToken::new("t"))?;
        let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
        let server = format!("ws://{}", listener.local_addr()?);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let task = runtime.spawn(relay.serve(listener, async {
            let _ = stopped.await;
        }));

        let (a, _) = SessionHandle::spawn(Box::new(NativeHost))?;
        let (b, _) = SessionHandle::spawn(Box::new(NativeHost))?;
        let doc = a.call(|s| s.new_document(DocKind::Write))??.doc;
        let url = server.clone();
        a.call(move |s| {
            s.execute(
                Some(doc),
                Command::Write(WriteCommand::InsertText {
                    text: "Hello world".into(),
                }),
            )?;
            s.execute(
                Some(doc),
                SuiteCommand::Share {
                    server: url,
                    token: "t".into(),
                    name: "Ana".into(),
                }
                .into(),
            )
        })??;
        until("the sharer to be online", || {
            status(&a, doc) == Some(CollabStatus::Online)
        })?;
        let link = a
            .call(move |s| match s.query_mut(Some(doc), &Query::Collab) {
                Ok(QueryResult::Collab(c)) => c.link,
                _ => None,
            })?
            .ok_or("no link")?;
        let joined = b
            .call(move |s| {
                s.execute(
                    None,
                    SuiteCommand::JoinShared {
                        kind: DocKind::Write,
                        server: link.server,
                        room: link.room,
                        token: link.token,
                        name: "Rana".into(),
                    }
                    .into(),
                )
            })??
            .summary
            .ok_or("no summary")?
            .doc;
        until("the joiner to get the document", || {
            texts(&b, joined) == ["Hello world"]
        })?;
        a.call(move |s| {
            s.execute(
                Some(doc),
                Command::Write(WriteCommand::InsertText { text: "!".into() }),
            )
        })??;
        until("the edit to reach the joiner", || {
            texts(&b, joined) == ["Hello world!"]
        })?;
        drop((a, b));
        let _ = stop.send(());
        runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(10), task).await })???;
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
