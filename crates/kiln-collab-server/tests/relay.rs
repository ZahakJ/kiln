// SPDX-License-Identifier: AGPL-3.0-or-later
//! The relay end to end: real WebSocket clients through a real server on a free port.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use kiln_collab::{Crdt, Frame, Incoming, LoroCrdt, Refusal, RoomKind, SyncClient, WriteBinding};
use kiln_collab_server::store::{Storage, Store, StoreError, Stored};
use kiln_collab_server::{Relay, SharedToken};
use kiln_doc::{Document, IdLanes, Pos, edit};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Error = Box<dyn std::error::Error + Send + Sync>;

const TOKEN: &str = "test-token";

fn data_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

struct Server {
    addr: SocketAddr,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<(), kiln_collab_server::ServeError>>,
}

async fn open(dir: &Path) -> Result<Store, Error> {
    // The previous run's database closes once its last connection task has ended.
    let mut store = Store::open(dir);
    for _ in 0..50 {
        if store.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        store = Store::open(dir);
    }
    Ok(store?)
}

async fn start(dir: &Path) -> Result<Server, Error> {
    let relay = Relay::with_storage(open(dir).await?, SharedToken::new(TOKEN));
    serve(relay).await
}

async fn serve(relay: Relay) -> Result<Server, Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (stop, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(relay.serve(listener, async {
        let _ = rx.await;
    }));
    Ok(Server { addr, stop, task })
}

impl Server {
    async fn stop(self) -> Result<(), Error> {
        let _ = self.stop.send(());
        tokio::time::timeout(Duration::from_secs(10), self.task).await???;
        Ok(())
    }
}

/// One editor: its document, binding, sync client, socket and outgoing updates.
struct Peer {
    doc: Document,
    binding: WriteBinding,
    client: SyncClient,
    ws: Ws,
    outbox: Arc<Mutex<Vec<Vec<u8>>>>,
    _sub: kiln_collab::Subscription,
}

async fn socket(addr: SocketAddr, room: &str) -> Result<Ws, Error> {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/rooms/{room}")).await?;
    Ok(ws)
}

async fn next_frame(ws: &mut Ws) -> Result<Option<Vec<u8>>, Error> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Err(_) | Ok(None) => return Ok(None),
            Ok(Some(m)) => match m? {
                Message::Binary(b) => return Ok(Some(b.to_vec())),
                Message::Close(_) => return Ok(None),
                _ => {}
            },
        }
    }
}

fn subscribe(binding: &WriteBinding) -> (Arc<Mutex<Vec<Vec<u8>>>>, kiln_collab::Subscription) {
    let outbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let sink = Arc::clone(&outbox);
    let sub = binding.crdt().subscribe(Box::new(move |u| {
        if let Ok(mut o) = sink.lock() {
            o.push(u.to_vec());
        }
    }));
    (outbox, sub)
}

impl Peer {
    /// Creates the room with a new document.
    async fn create(addr: SocketAddr, room: &str, peer: u64) -> Result<Self, Error> {
        let mut doc = Document::new();
        let binding = WriteBinding::share(&mut doc, peer, IdLanes::default())?;
        let mut client = SyncClient::new(room, TOKEN, peer, RoomKind::Write);
        let mut ws = socket(addr, room).await?;
        ws.send(Message::Binary(client.hello(binding.crdt())?.into()))
            .await?;
        let welcome = next_frame(&mut ws).await?.ok_or("no welcome")?;
        let (Incoming::Joined { lanes, .. }, replies) = client.receive(&welcome, binding.crdt())?
        else {
            return Err("not admitted".into());
        };
        doc.set_id_lanes(lanes);
        for r in replies {
            ws.send(Message::Binary(r.into())).await?;
        }
        // The relay answers in order: once this comes back, the document is stored.
        let version = binding.crdt().version();
        ws.send(Message::Binary(client.snapshot_request(&version)?.into()))
            .await?;
        let answer = next_frame(&mut ws).await?.ok_or("no snapshot")?;
        assert!(matches!(Frame::decode(&answer)?, Frame::Snapshot { .. }));
        let (outbox, sub) = subscribe(&binding);
        Ok(Self {
            doc,
            binding,
            client,
            ws,
            outbox,
            _sub: sub,
        })
    }

    /// Joins an existing room from nothing.
    async fn join(addr: SocketAddr, room: &str, peer: u64) -> Result<Self, Error> {
        let mut client = SyncClient::new(room, TOKEN, peer, RoomKind::Write);
        let empty = LoroCrdt::new(peer)?;
        let mut ws = socket(addr, room).await?;
        ws.send(Message::Binary(client.hello(&empty)?.into()))
            .await?;
        let welcome = next_frame(&mut ws).await?.ok_or("no welcome")?;
        let (Incoming::Joined { state, lanes, .. }, _) = client.receive(&welcome, &empty)? else {
            return Err("not admitted".into());
        };
        let (binding, doc) = WriteBinding::join(&state, peer, lanes)?;
        let (outbox, sub) = subscribe(&binding);
        Ok(Self {
            doc,
            binding,
            client,
            ws,
            outbox,
            _sub: sub,
        })
    }

    async fn type_text(&mut self, text: &str) -> Result<(), Error> {
        let p = self.doc.body_paragraphs()[0].id;
        let end = self.doc.body_paragraphs()[0].content.len();
        let tx = edit::insert_text(&mut self.doc, Pos::new(p, end), text)?;
        self.doc.apply(tx.clone())?;
        self.binding.commit(&self.doc, &tx)?;
        let updates: Vec<Vec<u8>> =
            std::mem::take(&mut *self.outbox.lock().map_err(|_| "poisoned")?);
        for u in updates {
            for frame in self.client.local_update(&u)? {
                self.ws.send(Message::Binary(frame.into())).await?;
            }
        }
        Ok(())
    }

    /// Merges what arrives until `want` updates came or nothing more arrives.
    async fn receive(&mut self, want: usize) -> Result<(), Error> {
        let mut got = 0;
        while got < want {
            let Some(bytes) = next_frame(&mut self.ws).await? else {
                break;
            };
            if let (Incoming::Update(u), _) = self.client.receive(&bytes, self.binding.crdt())? {
                self.binding.apply_remote(&mut self.doc, &u)?;
                got += 1;
            }
        }
        Ok(())
    }

    /// Drops the connection (edits queue), later reconnects and merges both ways.
    async fn go_offline(&mut self) -> Result<(), Error> {
        self.ws.close(None).await?;
        while let Ok(Some(_)) = tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await {
        }
        self.client.disconnected();
        Ok(())
    }

    async fn reconnect(&mut self, addr: SocketAddr, room: &str) -> Result<(), Error> {
        self.ws = socket(addr, room).await?;
        let hello = self.client.hello(self.binding.crdt())?;
        self.ws.send(Message::Binary(hello.into())).await?;
        let welcome = next_frame(&mut self.ws).await?.ok_or("no welcome")?;
        let (Incoming::Joined { state, .. }, replies) =
            self.client.receive(&welcome, self.binding.crdt())?
        else {
            return Err("not admitted".into());
        };
        if !state.is_empty() {
            self.binding.apply_remote(&mut self.doc, &state)?;
        }
        for r in replies {
            self.ws.send(Message::Binary(r.into())).await?;
        }
        Ok(())
    }

    fn text(&self) -> String {
        self.doc.body_paragraphs()[0].text()
    }

    async fn close(mut self) -> Result<(), Error> {
        self.ws.close(None).await?;
        while let Ok(Some(_)) = tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await {
        }
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_editors_converge_a_late_joiner_catches_up_and_a_restart_keeps_the_room()
-> Result<(), Error> {
    let dir = data_dir("relay-converge");
    let server = start(&dir).await?;
    let mut a = Peer::create(server.addr, "report", 11).await?;
    let mut b = Peer::join(server.addr, "report", 12).await?;
    a.type_text("Alpha ").await?;
    b.type_text("Beta ").await?;
    a.type_text("again").await?;
    b.receive(2).await?;
    a.receive(1).await?;
    assert_eq!(a.doc, b.doc);
    assert!(a.text().contains("Alpha") && a.text().contains("Beta") && a.text().contains("again"));

    // B edits offline while A goes on; B's queued edits merge when it reconnects.
    b.go_offline().await?;
    b.type_text(" offline").await?;
    assert_eq!(b.client.queue().len(), 1);
    a.type_text(" online").await?;
    b.reconnect(server.addr, "report").await?;
    assert!(b.client.queue().is_empty());
    a.receive(1).await?;
    assert_eq!(a.doc, b.doc);
    assert!(a.text().contains("offline") && a.text().contains("online"));

    let late = Peer::join(server.addr, "report", 13).await?;
    assert_eq!(late.doc, a.doc, "the late joiner gets the whole document");
    let expected = a.doc.clone();
    a.close().await?;
    b.close().await?;
    late.close().await?;
    server.stop().await?;

    let server = start(&dir).await?;
    let after = Peer::join(server.addr, "report", 14).await?;
    assert_eq!(after.doc, expected, "the room survives a restart");
    after.close().await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_token_is_refused_and_health_answers() -> Result<(), Error> {
    let dir = data_dir("relay-refuse");
    let server = start(&dir).await?;
    let mut ws = socket(server.addr, "secret-room").await?;
    let mut client = SyncClient::new("secret-room", "wrong", 1, RoomKind::Write);
    ws.send(Message::Binary(client.hello(&LoroCrdt::new(1)?)?.into()))
        .await?;
    let reply = next_frame(&mut ws).await?.ok_or("no reply")?;
    assert_eq!(
        Frame::decode(&reply)?,
        Frame::Refused {
            reason: Refusal::BadToken
        }
    );
    assert!(
        next_frame(&mut ws).await?.is_none(),
        "the relay closes the connection"
    );

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut http = TcpStream::connect(server.addr).await?;
    http.write_all(b"GET /health HTTP/1.1\r\nHost: relay\r\nConnection: close\r\n\r\n")
        .await?;
    let mut answer = String::new();
    http.read_to_string(&mut answer).await?;
    assert!(
        answer.starts_with("HTTP/1.1 200") && answer.ends_with("ok"),
        "{answer}"
    );
    server.stop().await?;
    Ok(())
}

/// A store whose writes can be made to fail.
struct Flaky {
    store: Store,
    fail: Arc<std::sync::atomic::AtomicBool>,
}

impl Flaky {
    fn check(&self) -> Result<(), StoreError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(redb::Error::Io(std::io::Error::other("disk full")));
        }
        Ok(())
    }
}

impl Storage for Flaky {
    fn load(&self, room: &str) -> Result<Stored, StoreError> {
        self.store.load(room)
    }
    fn append(&self, room: &str, seq: u64, update: &[u8]) -> Result<(), StoreError> {
        self.check()?;
        self.store.append(room, seq, update)
    }
    fn remove(&self, room: &str, seq: u64) -> Result<(), StoreError> {
        self.store.remove(room, seq)
    }
    fn put_meta(&self, room: &str, meta: &[u8]) -> Result<(), StoreError> {
        self.check()?;
        self.store.put_meta(room, meta)
    }
    fn compact(&self, room: &str, snapshot: &[u8], upto: u64) -> Result<(), StoreError> {
        self.check()?;
        self.store.compact(room, snapshot, upto)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_update_that_cannot_be_stored_is_sent_again() -> Result<(), Error> {
    let dir = data_dir("relay-flaky");
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let store = Flaky {
        store: open(&dir).await?,
        fail: Arc::clone(&fail),
    };
    let server = serve(Relay::with_storage(store, SharedToken::new(TOKEN))).await?;
    let mut a = Peer::create(server.addr, "flaky", 21).await?;
    let mut b = Peer::join(server.addr, "flaky", 22).await?;
    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    a.type_text("kept").await?;
    // The relay could not store it: A hears so and the connection closes; B gets nothing.
    let reply = next_frame(&mut a.ws).await?.ok_or("no reply")?;
    assert!(matches!(
        Frame::decode(&reply)?,
        Frame::Refused {
            reason: Refusal::Unavailable(_)
        }
    ));
    a.client.disconnected();
    assert!(
        next_frame(&mut b.ws)
            .await?
            .is_none_or(|f| { !matches!(Frame::decode(&f), Ok(Frame::Update { .. })) })
    );
    fail.store(false, std::sync::atomic::Ordering::SeqCst);
    // On reconnecting A sends it again; everyone gets it, a late joiner too.
    a.reconnect(server.addr, "flaky").await?;
    b.receive(1).await?;
    assert!(b.text().contains("kept"), "{}", b.text());
    let late = Peer::join(server.addr, "flaky", 23).await?;
    assert!(late.text().contains("kept"));
    a.close().await?;
    b.close().await?;
    late.close().await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_connection_closing_does_not_say_its_peer_left() -> Result<(), Error> {
    let dir = data_dir("relay-left");
    let server = start(&dir).await?;
    let mut a = Peer::create(server.addr, "left", 31).await?;
    let mut b = Peer::join(server.addr, "left", 32).await?;
    // B reconnects on a second socket before the first one goes.
    let old = std::mem::replace(&mut b.ws, socket(server.addr, "left").await?);
    let hello = b.client.hello(b.binding.crdt())?;
    b.ws.send(Message::Binary(hello.into())).await?;
    let welcome = next_frame(&mut b.ws).await?.ok_or("no welcome")?;
    let (Incoming::Joined { lanes, .. }, _) = b.client.receive(&welcome, b.binding.crdt())? else {
        return Err("not admitted".into());
    };
    assert_eq!(lanes, b.doc.id_lanes(), "a reconnection keeps its lane");
    let mut old = old;
    old.close(None).await?;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_secs(5), old.next()).await {}
    // A hears no Left for B, which is still there.
    while let Ok(Some(f)) = tokio::time::timeout(Duration::from_millis(500), a.ws.next()).await {
        if let Message::Binary(f) = f? {
            assert!(!matches!(Frame::decode(&f)?, Frame::Left { peer: 32 }));
        }
    }
    b.close().await?;
    let mut left = false;
    while let Some(f) = next_frame(&mut a.ws).await? {
        if matches!(Frame::decode(&f)?, Frame::Left { peer: 32 }) {
            left = true;
            break;
        }
    }
    assert!(left, "once B's last connection goes, A hears it left");
    a.close().await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_stays_bounded() -> Result<(), Error> {
    let dir = data_dir("relay-bounded");
    let relay =
        Relay::with_storage(open(&dir).await?, SharedToken::new(TOKEN)).with_client_queue(64);
    let watch = relay.clone();
    let server = serve(relay).await?;
    // A first message larger than any hello is not read.
    let mut ws = socket(server.addr, "bounded").await?;
    let _ = ws
        .send(Message::Binary(
            vec![0u8; kiln_collab_server::MAX_MESSAGE + 1].into(),
        ))
        .await;
    assert!(
        next_frame(&mut ws).await.ok().flatten().is_none(),
        "no answer"
    );
    // A client that stops reading is closed once too much waits for it.
    let mut a = Peer::create(server.addr, "bounded", 41).await?;
    let tcp = tokio::net::TcpSocket::new_v4()?;
    tcp.set_recv_buffer_size(4096)?;
    let tcp = tcp.connect(server.addr).await?;
    let (mut slow, _) =
        tokio_tungstenite::client_async(format!("ws://{}/rooms/bounded", server.addr), tcp).await?;
    let mut slow_client = SyncClient::new("bounded", TOKEN, 42, RoomKind::Write);
    slow.send(Message::Binary(
        slow_client.hello(&LoroCrdt::new(42)?)?.into(),
    ))
    .await?;
    let mut welcome = None;
    while welcome.is_none() {
        match slow.next().await {
            Some(Ok(Message::Binary(b))) => welcome = Some(b),
            Some(Ok(_)) => {}
            _ => return Err("no welcome".into()),
        }
    }
    for i in 0..5_000 {
        let mut p = kiln_collab::Presence::new(41, "A".repeat(2_000));
        p.clock = i;
        if let Some(f) = a.client.presence(p)? {
            a.ws.send(Message::Binary(f.into())).await?;
        }
    }
    // It reads now: what was queued for it, then the end of the connection.
    let mut got = 0;
    let closed = loop {
        match tokio::time::timeout(Duration::from_secs(5), slow.next()).await {
            Ok(Some(Ok(Message::Binary(_)))) => got += 1,
            Ok(Some(Ok(Message::Close(_))) | None | Some(Err(_))) => break true,
            Ok(Some(Ok(_))) => {}
            Err(_) => break false,
        }
    };
    assert!(
        closed && got < 5_000,
        "the slow client was closed after {got} messages"
    );
    // The relay answers in order: once this comes back, the flood is through.
    let version = a.binding.crdt().version();
    a.ws.send(Message::Binary(a.client.snapshot_request(&version)?.into()))
        .await?;
    while let Some(f) = next_frame(&mut a.ws).await? {
        if matches!(Frame::decode(&f)?, Frame::Snapshot { .. }) {
            break;
        }
    }
    // A large update goes in parts and arrives whole.
    let mut b = Peer::join(server.addr, "bounded", 43).await?;
    a.type_text(&"y".repeat(kiln_collab::client::PART * 3))
        .await?;
    b.receive(1).await?;
    assert_eq!(b.text().len(), a.text().len(), "the large update arrived");
    // A room nobody is in leaves memory.
    a.close().await?;
    b.close().await?;
    drop(slow);
    drop(slow_client);
    drop(welcome);
    let mut open_rooms = usize::MAX;
    for _ in 0..100 {
        open_rooms = watch.open_rooms().await;
        if open_rooms == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(open_rooms, 0);
    server.stop().await?;
    Ok(())
}

#[test]
fn the_store_keeps_updates_until_compacted() -> Result<(), Error> {
    let dir = data_dir("relay-store");
    let store = Store::open(&dir)?;
    store.append("a", 1, b"one")?;
    store.append("a", 2, b"two")?;
    store.append("b", 1, b"other")?;
    store.put_meta("a", b"meta")?;
    let a = store.load("a")?;
    assert_eq!(a.updates, vec![(1, b"one".to_vec()), (2, b"two".to_vec())]);
    assert_eq!(a.meta.as_deref(), Some(&b"meta"[..]));
    store.compact("a", b"snap", 1)?;
    drop(store);
    let store = Store::open(&dir)?;
    let a = store.load("a")?;
    assert_eq!(a.snapshot.as_deref(), Some(&b"snap"[..]));
    assert_eq!(a.updates, vec![(2, b"two".to_vec())]);
    assert_eq!(store.load("b")?.updates.len(), 1);
    assert!(store.load("c")?.snapshot.is_none());
    Ok(())
}
