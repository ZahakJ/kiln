// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-collab-server
//!
//! The self-hosted collaboration relay: an `axum` and `tokio` WebSocket server.
//! Each shared document is a room at `/rooms/{room}`; clients speak the sync protocol of
//! `kiln_collab::protocol`. The relay stores every update before it merges it into the room's
//! state (so a late joiner gets one snapshot) and passes it on; an update it cannot store is
//! neither merged nor passed on, and the client is told so ([`Refusal::Unavailable`]) and sends
//! it again when it reconnects. Presence is relayed as is. A room's updates are folded into a
//! snapshot every [`COMPACT_EVERY`] updates and when its last client leaves, and the room is
//! then dropped from memory. `GET /health` answers `ok`.
//!
//! Memory per connection is bounded: a client sends messages of at most [`MAX_MESSAGE`] bytes
//! (a larger update comes in parts, at most [`MAX_UPDATE`] in all, and only after the hello is
//! admitted), and a client that does not read what the relay sends it falls behind by at most
//! [`QUEUE`] messages before the relay closes its connection.
//!
//! **Authentication is a shared token** ([`SharedToken`]): anyone holding the link can join.
//! That is enough for a team behind its own network, not for the open internet; put the relay
//! behind TLS and replace [`Authenticate`] with a real check (accounts, per-room tokens) for
//! anything more.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod store;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use kiln_collab::{Frame, Presence, Refusal, Room, RoomMeta};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};

use crate::store::{Storage, Store};

/// The version of this crate (the suite version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Updates stored between two snapshots of a room.
pub const COMPACT_EVERY: u64 = 256;

/// The largest message the relay reads from a client: an update part
/// (`kiln_collab::client::PART`) with room to spare, or a hello.
pub const MAX_MESSAGE: usize = 2 * kiln_collab::client::PART;

/// The largest update the relay accepts, in parts (a whole document's state when a room is
/// created).
pub const MAX_UPDATE: usize = 64 * 1024 * 1024;

/// How many messages may wait for a client before the relay gives up on it.
pub const QUEUE: usize = 1024;

/// How long a new connection may take to say hello.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// Decides who may join a room.
pub trait Authenticate: Send + Sync + 'static {
    /// Whether `token` opens `room`.
    fn authenticate(&self, room: &str, token: &str) -> bool;
}

/// One token for every room. Not production-grade: see the crate documentation.
#[derive(Debug, Clone)]
pub struct SharedToken(String);

impl SharedToken {
    /// The token clients must present.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }
}

impl Authenticate for SharedToken {
    fn authenticate(&self, _room: &str, token: &str) -> bool {
        // Compared without an early exit, so the time taken does not reveal a matching prefix.
        let (a, b) = (self.0.as_bytes(), token.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

/// Where the relay listens and keeps its data.
#[derive(Debug, Clone)]
pub struct Config {
    /// The address to listen on.
    pub bind: SocketAddr,
    /// The directory of the room database.
    pub data_dir: PathBuf,
}

struct Client {
    peer: u64,
    tx: mpsc::Sender<Message>,
    /// Closes the connection (it fell too far behind).
    kick: Arc<tokio::sync::Notify>,
    presence: Option<Presence>,
}

impl Client {
    /// Queues a message; false when the client is too far behind (it is then closed).
    fn send(&self, m: Message) -> bool {
        if self.tx.try_send(m).is_ok() {
            return true;
        }
        self.kick.notify_one();
        false
    }
}

struct Hub {
    room: Room,
    /// The sequence number of the latest stored update.
    seq: u64,
    /// Stored updates not yet folded into a snapshot.
    pending: u64,
    clients: HashMap<u64, Client>,
}

struct Shared {
    store: Box<dyn Storage>,
    auth: Box<dyn Authenticate>,
    rooms: Mutex<HashMap<String, Arc<Mutex<Hub>>>>,
    next_conn: std::sync::atomic::AtomicU64,
    /// How many messages may wait for one client.
    queue: usize,
}

/// A relay ready to serve.
#[derive(Clone)]
pub struct Relay {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay").finish_non_exhaustive()
    }
}

/// Why the relay failed to start or stopped.
#[derive(Debug)]
pub enum ServeError {
    /// The database could not be opened.
    Store(store::StoreError),
    /// The listener failed.
    Io(std::io::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "room database: {e}"),
            Self::Io(e) => write!(f, "listener: {e}"),
        }
    }
}

impl std::error::Error for ServeError {}

impl Relay {
    /// A relay storing rooms in `data_dir`, admitting whoever `auth` admits.
    ///
    /// # Errors
    ///
    /// The database cannot be opened.
    pub fn new(data_dir: &std::path::Path, auth: impl Authenticate) -> Result<Self, ServeError> {
        let store = Store::open(data_dir).map_err(ServeError::Store)?;
        Ok(Self::with_storage(store, auth))
    }

    /// A relay keeping rooms in `storage`.
    pub fn with_storage(storage: impl Storage, auth: impl Authenticate) -> Self {
        Self {
            shared: Arc::new(Shared {
                store: Box::new(storage),
                auth: Box::new(auth),
                rooms: Mutex::new(HashMap::new()),
                next_conn: std::sync::atomic::AtomicU64::new(1),
                queue: QUEUE,
            }),
        }
    }

    /// The same relay letting at most `messages` wait for a client (default [`QUEUE`]); call it
    /// before serving.
    #[must_use]
    pub fn with_client_queue(mut self, messages: usize) -> Self {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.queue = messages.max(1);
        }
        self
    }

    /// How many rooms the relay holds in memory.
    pub async fn open_rooms(&self) -> usize {
        self.shared.rooms.lock().await.len()
    }

    /// The HTTP routes: `/health` and `/rooms/{room}`.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/rooms/{room}", get(upgrade))
            .with_state(self.clone())
    }

    /// Serves on `listener` until `shutdown` completes, then folds every open room's updates
    /// into a snapshot.
    ///
    /// # Errors
    ///
    /// The listener failed.
    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), ServeError> {
        axum::serve(listener, self.router())
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(ServeError::Io)?;
        let rooms: Vec<(String, Arc<Mutex<Hub>>)> =
            self.shared.rooms.lock().await.drain().collect();
        for (name, hub) in rooms {
            compact(&*self.shared.store, &name, &mut *hub.lock().await);
        }
        Ok(())
    }

    /// Drops a room nobody is in from memory once its updates are folded into a snapshot (it
    /// is read again from storage when someone comes back).
    async fn release(&self, name: &str) {
        let mut rooms = self.shared.rooms.lock().await;
        let Some(hub) = rooms.get(name).cloned() else {
            return;
        };
        // Only the map and this function hold the room: nobody is on the way in.
        if Arc::strong_count(&hub) > 2 {
            return;
        }
        let mut h = hub.lock().await;
        if h.clients.is_empty() {
            compact(&*self.shared.store, name, &mut h);
            if h.pending == 0 {
                rooms.remove(name);
            }
        }
    }

    async fn hub(&self, name: &str) -> Result<Arc<Mutex<Hub>>, String> {
        let mut rooms = self.shared.rooms.lock().await;
        if let Some(h) = rooms.get(name) {
            return Ok(Arc::clone(h));
        }
        let stored = self.shared.store.load(name).map_err(|e| e.to_string())?;
        let meta = match &stored.meta {
            Some(m) => RoomMeta::from_bytes(m).map_err(|e| e.to_string())?,
            None => RoomMeta::default(),
        };
        let updates: Vec<Vec<u8>> = stored.updates.iter().map(|(_, u)| u.clone()).collect();
        let room =
            Room::restore(stored.snapshot.as_deref(), &updates, meta).map_err(|e| e.to_string())?;
        let hub = Arc::new(Mutex::new(Hub {
            room,
            seq: stored.updates.last().map_or(0, |(s, _)| *s),
            pending: stored.updates.len() as u64,
            clients: HashMap::new(),
        }));
        rooms.insert(name.to_owned(), Arc::clone(&hub));
        Ok(hub)
    }
}

fn compact(store: &dyn Storage, name: &str, hub: &mut Hub) {
    if hub.pending == 0 {
        return;
    }
    // A failed compaction keeps the updates; the next one retries.
    if let Ok(snapshot) = hub.room.snapshot()
        && store.compact(name, &snapshot, hub.seq).is_ok()
    {
        hub.pending = 0;
    }
}

async fn upgrade(
    ws: WebSocketUpgrade,
    Path(room): Path<String>,
    State(relay): State<Relay>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE)
        .on_upgrade(move |socket| connection(socket, room, relay))
}

fn binary(frame: &Frame) -> Option<Message> {
    frame.encode().ok().map(|b| Message::Binary(b.into()))
}

async fn connection(socket: WebSocket, name: String, relay: Relay) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Message>(relay.shared.queue);
    let kick = Arc::new(tokio::sync::Notify::new());
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            let close = matches!(m, Message::Close(_));
            if sink.send(m).await.is_err() || close {
                break;
            }
        }
    });
    let end = |tx: mpsc::Sender<Message>, reason: Option<Refusal>| async move {
        if let Some(reason) = reason
            && let Some(m) = binary(&Frame::Refused { reason })
        {
            let _ = tx.send(m).await;
        }
        let _ = tx.send(Message::Close(None)).await;
    };
    // The first frame must be a hello for this room with a token that opens it.
    let first = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Binary(b)))) => b,
        _ => {
            end(tx, None).await;
            let _ = writer.await;
            return;
        }
    };
    let (peer, kind, version, held) = match Frame::decode(&first) {
        Ok(Frame::Hello {
            room,
            token,
            peer,
            kind,
            version,
            lane,
        }) if room == name && relay.shared.auth.authenticate(&name, &token) => {
            (peer, kind, version, lane)
        }
        Ok(Frame::Hello { .. }) => {
            end(tx, Some(Refusal::BadToken)).await;
            let _ = writer.await;
            return;
        }
        Err(_) => {
            let v = kiln_collab::protocol::protocol_of(&first).unwrap_or(0);
            end(tx, Some(Refusal::Protocol(v))).await;
            let _ = writer.await;
            return;
        }
        Ok(_) => {
            end(tx, None).await;
            let _ = writer.await;
            return;
        }
    };
    let hub = match relay.hub(&name).await {
        Ok(h) => h,
        Err(e) => {
            end(tx, Some(Refusal::Unavailable(e))).await;
            let _ = writer.await;
            return;
        }
    };
    let conn = relay
        .shared
        .next_conn
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let admitted = admit(
        &relay, &name, &hub, conn, peer, kind, &version, held, &tx, &kick,
    )
    .await;
    if let Err(reason) = admitted {
        drop(hub);
        relay.release(&name).await;
        end(tx, reason).await;
        let _ = writer.await;
        return;
    }
    // An update that came in parts.
    let mut parts: Vec<u8> = Vec::new();
    let mut refusal: Option<Refusal> = None;
    loop {
        let msg = tokio::select! {
            m = stream.next() => m,
            () = kick.notified() => break,
        };
        let Some(Ok(msg)) = msg else { break };
        let bytes = match msg {
            Message::Binary(b) => b,
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(frame) = Frame::decode(&bytes) else {
            continue;
        };
        let update = match frame {
            Frame::Update { bytes: update } => update,
            Frame::Part { bytes: part, last } => {
                if parts.len() + part.len() > MAX_UPDATE {
                    break;
                }
                parts.extend_from_slice(&part);
                if !last {
                    continue;
                }
                std::mem::take(&mut parts)
            }
            Frame::Awareness { mut presence } => {
                let mut h = hub.lock().await;
                presence.peer = peer;
                if let Some(c) = h.clients.get_mut(&conn) {
                    c.presence = Some(presence.clone());
                }
                if let Some(m) = binary(&Frame::Awareness { presence }) {
                    for (id, c) in &h.clients {
                        if *id != conn {
                            c.send(m.clone());
                        }
                    }
                }
                continue;
            }
            Frame::SnapshotRequest { version } => {
                let h = hub.lock().await;
                if let Ok(f) = h.room.snapshot_for(&version)
                    && let Some(m) = binary(&f)
                    && h.clients.get(&conn).is_some_and(|c| !c.send(m))
                {
                    break;
                }
                continue;
            }
            _ => continue,
        };
        let mut h = hub.lock().await;
        let seq = h.seq + 1;
        // Stored first: an update the room has is one that survives a restart.
        if relay.shared.store.append(&name, seq, &update).is_err() {
            // Neither merged nor passed on: the client sends it again after reconnecting
            // (the welcome's version lacks it).
            refusal = Some(Refusal::Unavailable(
                "the change could not be stored".into(),
            ));
            break;
        }
        if h.room.update(&update).is_err() {
            // Not an update: dropped again (a stored update the room cannot read would break
            // its restoring).
            let _ = relay.shared.store.remove(&name, seq);
            continue;
        }
        h.seq = seq;
        h.pending += 1;
        let out = binary(&Frame::Update { bytes: update });
        if let Some(out) = out {
            for (id, c) in &h.clients {
                if *id != conn {
                    c.send(out.clone());
                }
            }
        }
        if h.pending >= COMPACT_EVERY {
            compact(&*relay.shared.store, &name, &mut h);
        }
    }
    {
        let mut h = hub.lock().await;
        h.clients.remove(&conn);
        // The same peer may be back on a newer connection already.
        if !h.clients.values().any(|c| c.peer == peer)
            && let Some(m) = binary(&Frame::Left { peer })
        {
            for c in h.clients.values() {
                c.send(m.clone());
            }
        }
    }
    drop(hub);
    relay.release(&name).await;
    end(tx, refusal).await;
    let _ = writer.await;
}

/// Admits a connection to its room: the welcome, or why not (with `None`, closes without a
/// reason).
#[allow(clippy::too_many_arguments)]
async fn admit(
    relay: &Relay,
    name: &str,
    hub: &Arc<Mutex<Hub>>,
    conn: u64,
    peer: u64,
    kind: kiln_collab::RoomKind,
    version: &[u8],
    held: Option<u32>,
    tx: &mpsc::Sender<Message>,
    kick: &Arc<tokio::sync::Notify>,
) -> Result<(), Option<Refusal>> {
    let mut h = hub.lock().await;
    let (frame, changed) = h
        .room
        .hello(kind, version, held)
        .map_err(|e| Some(Refusal::Unavailable(e.to_string())))?;
    let Frame::Welcome {
        kind,
        state,
        version,
        lane,
        lanes,
        ..
    } = frame
    else {
        return Err(match frame {
            Frame::Refused { reason } => Some(reason),
            _ => None,
        });
    };
    if changed {
        let stored = h
            .room
            .meta()
            .to_bytes()
            .ok()
            .is_some_and(|meta| relay.shared.store.put_meta(name, &meta).is_ok());
        if !stored {
            return Err(Some(Refusal::Unavailable(
                "the room could not be stored".into(),
            )));
        }
    }
    let peers: Vec<Presence> = h
        .clients
        .values()
        .filter_map(|c| c.presence.clone())
        .collect();
    let welcome = Frame::Welcome {
        kind,
        state,
        version,
        lane,
        lanes,
        peers,
    };
    let client = Client {
        peer,
        tx: tx.clone(),
        kick: Arc::clone(kick),
        presence: None,
    };
    if let Some(m) = binary(&welcome)
        && !client.send(m)
    {
        return Err(None);
    }
    h.clients.insert(conn, client);
    Ok(())
}
