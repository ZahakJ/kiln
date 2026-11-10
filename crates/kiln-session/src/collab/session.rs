// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session's side of a shared document: commands, edits, undo, the socket and the timer,
//! and the queries of the Share dialog, the presence avatars and the Version History pane.

use kiln_common::{DocId, KilnError, Result};
use kiln_protocol::collab::engine::{
    CollabChange, EngineCall, EngineReply, EngineRequest, EngineValue,
};
use kiln_protocol::collab::{CollabLink, CollabPeer, CollabState, CollabStatus, VersionSlides};
use kiln_protocol::{
    DocKind, ExecResult, ExecValue, HostOp, HostReply, Outcome, Query, QueryResult, SuiteCommand,
};

use super::{CollabBackend, Shared, Wait, initials, random_peer, random_room, room_url};
use crate::session::{Pending, Session};

/// The longest wait between reconnection attempts.
const MAX_BACKOFF_MS: u32 = 8_000;
/// The first wait.
const FIRST_BACKOFF_MS: u32 = 500;
/// The timer of a connected document: presence expiry and the ten-minute versions.
const TICK_MS: u32 = 15_000;

fn unavailable(id: &str, why: &str) -> KilnError {
    KilnError::unavailable(id, why)
}

impl Session {
    /// Gives the session the collaboration engines (the web's collaboration module, loaded when
    /// a document is first shared or joined; native builds have them linked in).
    pub fn set_collab(&mut self, backend: Box<dyn CollabBackend>) {
        self.collab.backend = Some(backend);
    }

    /// Whether the session can share documents now.
    #[must_use]
    pub fn has_collab(&self) -> bool {
        self.collab.backend.is_some()
    }

    fn engine(&mut self, handle: u32, call: EngineCall) -> Result<EngineReply> {
        let backend = self
            .collab
            .backend
            .as_mut()
            .ok_or_else(|| unavailable("suite.collab.share", "collaboration is not loaded"))?;
        backend.request(EngineRequest { handle, call })
    }

    /// One call to the engine of a shared document, its reply absorbed.
    fn call(&mut self, doc: DocId, call: EngineCall, own: bool) -> Result<EngineReply> {
        let handle = self
            .collab
            .shared
            .get(&doc)
            .map(|s| s.handle)
            .ok_or_else(|| KilnError::InvalidInput("the document is not shared".into()))?;
        let reply = self.engine(handle, call)?;
        self.absorb(doc, &reply, own)?;
        Ok(reply)
    }

    /// Applies what an engine reply says: the change, the lane, the arrival of a joiner's
    /// document, a refusal, the undo state, the others; and sends its frames.
    fn absorb(&mut self, doc: DocId, reply: &EngineReply, own: bool) -> Result<()> {
        let Some(s) = self.collab.shared.get_mut(&doc) else {
            return Ok(());
        };
        // Admitted (again): the others hear where this person is (presence is not queued).
        let came_online = reply.online && !s.online;
        if came_online {
            s.sent = None;
            // Admitted: the next drop starts waiting afresh.
            s.backoff_ms = FIRST_BACKOFF_MS;
            s.problem = None;
        }
        s.online = reply.online;
        s.queued = reply.queued;
        s.colour = reply.colour;
        if let Some(r) = &reply.refused {
            s.refused = true;
            s.problem = Some(r.clone());
        }
        // A refusal for now: closed, and tried again after the usual wait.
        let unavailable = reply.unavailable.is_some();
        if let Some(r) = &reply.unavailable {
            s.problem = Some(r.clone());
        }
        if reply.joined {
            s.joining = false;
        }
        let peers = reply.peers.clone();
        if let Some(p) = &peers {
            s.peers.clone_from(p);
        }
        let refused = reply.refused.is_some();
        let d = self.doc_mut(doc)?;
        let Some(m) = d.model.collab() else {
            return Ok(());
        };
        if let Some((lane, lanes)) = reply.lanes {
            m.set_lanes(lane, lanes);
        }
        m.set_undo_state(reply.can_undo, reply.can_redo);
        let changed = reply.change != CollabChange::None;
        if changed {
            m.apply_change(&reply.change, own)?;
        }
        if let Some(p) = &peers {
            m.set_peers(p);
        }
        if changed || peers.is_some() || reply.joined {
            if matches!(reply.change, CollabChange::Replace(_)) {
                d.frames.reset();
            }
            // Others' carets are in the overlay: a new revision asks the surfaces for a frame.
            d.revision += 1;
            self.emit_changed(doc);
        }
        // others' edits reach the autosave journal as a checkpoint.
        if changed && !own {
            self.storage_remote(doc);
        }
        if peers.is_some() || reply.joined || refused || unavailable {
            self.emit_collab(doc);
        }
        self.send(doc, &reply.frames);
        if refused {
            self.disconnect(doc);
        }
        if unavailable {
            self.disconnect(doc);
            let _ = self.call(doc, EngineCall::Disconnected, false);
            self.arm_timer(doc);
        }
        if came_online || reply.joined {
            self.send_whereabouts(doc);
        }
        Ok(())
    }

    /// Sends where this person is when it changed since it was last sent.
    fn send_whereabouts(&mut self, id: DocId) {
        let Some(s) = self.collab.shared.get(&id) else {
            return;
        };
        if s.joining || s.preview.is_some() {
            return;
        }
        let sent = s.sent.clone();
        let Ok(d) = self.doc_mut(id) else { return };
        let Some(m) = d.model.collab() else { return };
        let here = m.whereabouts();
        if sent.as_ref() == Some(&here) {
            return;
        }
        let moved = sent
            .as_ref()
            .is_some_and(|s| s.caret != here.caret || s.anchor != here.anchor);
        if moved {
            let _ = self.call(id, EngineCall::BreakUndoGroup, false);
        }
        let call = EngineCall::Presence {
            caret: here.caret,
            anchor: here.anchor,
            slide: here.slide,
            shapes: here.shapes.clone(),
        };
        if self.call(id, call, false).is_ok()
            && let Some(s) = self.collab.shared.get_mut(&id)
        {
            s.sent = Some(here);
        }
    }

    fn emit_collab(&mut self, doc: DocId) {
        if let Some(s) = self.collab.shared.get(&doc) {
            let peers = u32::try_from(s.peers.len()).unwrap_or(u32::MAX);
            let connected = s.online;
            self.events.push_back(kiln_protocol::Event::Collab {
                doc,
                peers,
                connected,
            });
        }
    }

    /// Sends frames on the document's socket (dropped while it is down: the engine queued the
    /// updates among them, and presence is sent afresh).
    fn send(&mut self, doc: DocId, frames: &[Vec<u8>]) {
        let Some(s) = self.collab.shared.get(&doc) else {
            return;
        };
        if !s.connected {
            return;
        }
        let socket = s.socket;
        for f in frames {
            self.request(
                HostOp::SocketSend {
                    socket,
                    bytes: f.clone(),
                },
                Pending::Collab(Wait::Send { doc, socket }),
            );
        }
    }

    /// Opens a new connection for a shared document.
    fn connect(&mut self, doc: DocId) {
        let socket = self.collab.next_socket;
        self.collab.next_socket = self.collab.next_socket.wrapping_add(1).max(1);
        let Some(s) = self.collab.shared.get_mut(&doc) else {
            return;
        };
        s.socket = socket;
        s.connected = false;
        s.opening = true;
        let url = room_url(&s.link);
        self.request(
            HostOp::SocketOpen { socket, url },
            Pending::Collab(Wait::Open { doc, socket }),
        );
    }

    /// Closes the document's connection.
    fn disconnect(&mut self, doc: DocId) {
        let Some(s) = self.collab.shared.get_mut(&doc) else {
            return;
        };
        let was = s.connected;
        s.connected = false;
        s.online = false;
        let socket = s.socket;
        if was {
            self.request(HostOp::SocketClose { socket }, Pending::Ignore);
        }
    }

    fn arm_timer(&mut self, doc: DocId) {
        let Some(s) = self.collab.shared.get_mut(&doc) else {
            return;
        };
        if s.timer {
            return;
        }
        s.timer = true;
        let ms = if s.connected || s.refused {
            TICK_MS
        } else {
            // Each attempt until a welcome waits twice as long as the one before.
            let ms = s.backoff_ms;
            s.backoff_ms = (s.backoff_ms * 2).min(MAX_BACKOFF_MS);
            ms
        };
        self.request(HostOp::Timer { ms }, Pending::Collab(Wait::Timer { doc }));
    }

    /// A reply to a socket or timer request.
    pub(crate) fn collab_reply(&mut self, wait: Wait, reply: HostReply) {
        let doc = wait.doc();
        let current = |s: &Shared, socket: u32| s.socket == socket;
        match wait {
            Wait::Open { socket, .. } => {
                let wanted = self
                    .collab
                    .shared
                    .get(&doc)
                    .is_some_and(|s| current(s, socket));
                if !wanted {
                    // A connection that is no longer wanted (the document closed, a newer one).
                    if matches!(reply, HostReply::Done) {
                        self.request(HostOp::SocketClose { socket }, Pending::Ignore);
                    }
                    return;
                }
                let Some(s) = self.collab.shared.get_mut(&doc) else {
                    return;
                };
                s.opening = false;
                if matches!(reply, HostReply::Done) {
                    s.connected = true;
                    if let Err(e) = self.call(doc, EngineCall::Hello, false) {
                        self.collab_failed(doc, &e);
                    }
                    self.request(
                        HostOp::SocketReceive { socket },
                        Pending::Collab(Wait::Receive { doc, socket }),
                    );
                } else {
                    if let HostReply::Failed(e) = &reply {
                        s.problem = Some(e.message.clone());
                    }
                    self.emit_collab(doc);
                    self.arm_timer(doc);
                }
            }
            Wait::Receive { socket, .. } => {
                let Some(s) = self.collab.shared.get(&doc) else {
                    return;
                };
                if !current(s, socket) {
                    return;
                }
                if let HostReply::Bytes(frame) = reply {
                    let now = self.host.now_ms();
                    if let Err(e) =
                        self.call(doc, EngineCall::Receive { frame, now_ms: now }, false)
                    {
                        self.collab_failed(doc, &e);
                    }
                    let open = self
                        .collab
                        .shared
                        .get(&doc)
                        .is_some_and(|s| s.connected && s.socket == socket);
                    if open {
                        self.request(
                            HostOp::SocketReceive { socket },
                            Pending::Collab(Wait::Receive { doc, socket }),
                        );
                    }
                } else {
                    // Closed (or failed): edits wait for the next connection.
                    if let Some(s) = self.collab.shared.get_mut(&doc) {
                        s.connected = false;
                        s.online = false;
                    }
                    let _ = self.call(doc, EngineCall::Disconnected, false);
                    self.emit_collab(doc);
                    self.arm_timer(doc);
                }
            }
            Wait::Send { socket, .. } => {
                if matches!(reply, HostReply::Failed(_))
                    && self
                        .collab
                        .shared
                        .get(&doc)
                        .is_some_and(|s| current(s, socket))
                {
                    self.disconnect(doc);
                    let _ = self.call(doc, EngineCall::Disconnected, false);
                    self.emit_collab(doc);
                    self.arm_timer(doc);
                }
            }
            Wait::Timer { .. } => {
                let Some(s) = self.collab.shared.get_mut(&doc) else {
                    return;
                };
                s.timer = false;
                let reconnect = !s.connected && !s.opening && !s.refused;
                let now = self.host.now_ms();
                if let Err(e) = self.call(doc, EngineCall::Tick { now_ms: now }, false) {
                    self.collab_failed(doc, &e);
                }
                if reconnect {
                    self.connect(doc);
                }
                self.arm_timer(doc);
            }
        }
    }

    fn collab_failed(&mut self, doc: DocId, e: &KilnError) {
        if let Some(s) = self.collab.shared.get_mut(&doc) {
            s.problem = Some(e.to_string());
        }
        self.events.push_back(kiln_protocol::Event::Error {
            doc: Some(doc),
            error: e.into(),
        });
        self.emit_collab(doc);
    }

    // ----- commands ------------------------------------------------------------------------

    /// The collaboration commands.
    pub(crate) fn collab_command(
        &mut self,
        doc: Option<DocId>,
        cmd: SuiteCommand,
    ) -> Result<ExecResult> {
        match cmd {
            SuiteCommand::Share {
                server,
                token,
                name,
            } => {
                let id =
                    doc.ok_or_else(|| KilnError::InvalidInput("Share needs a document".into()))?;
                self.share(id, &server, &token, &name)?;
                self.finish_collab(id)
            }
            SuiteCommand::JoinShared {
                kind,
                server,
                room,
                token,
                name,
            } => self.join(kind, &server, &room, &token, &name),
            SuiteCommand::StopSharing => {
                let id = require(doc)?;
                self.collab_close(id);
                if let Some(m) = self.doc_mut(id)?.model.collab() {
                    m.set_peers(&[]);
                }
                self.doc_mut(id)?.revision += 1;
                self.emit_changed(id);
                self.finish_collab(id)
            }
            SuiteCommand::VersionPreview { id: version } => {
                let id = require(doc)?;
                self.preview(id, version)?;
                self.finish_collab(id)
            }
            SuiteCommand::VersionRestore { id: version } => {
                let id = require(doc)?;
                self.preview(id, None)?;
                let now = self.host.now_ms();
                self.call(
                    id,
                    EngineCall::Restore {
                        id: version,
                        now_ms: now,
                    },
                    false,
                )?;
                self.finish_collab(id)
            }
            SuiteCommand::VersionRename { id: version, name } => {
                let id = require(doc)?;
                self.call(id, EngineCall::RenameVersion { id: version, name }, false)?;
                self.finish_collab(id)
            }
            _ => Err(KilnError::Internal("not a collaboration command".into())),
        }
    }

    fn finish_collab(&mut self, id: DocId) -> Result<ExecResult> {
        Ok(ExecResult {
            outcome: Outcome::Applied,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    fn check_server(server: &str) -> Result<()> {
        let s = server.trim();
        if (s.starts_with("ws://") || s.starts_with("wss://")) && s.len() > "wss://".len() {
            Ok(())
        } else {
            Err(KilnError::InvalidInput(format!(
                "{server:?} is not a collaboration server address (ws://… or wss://…)"
            )))
        }
    }

    fn share(&mut self, id: DocId, server: &str, token: &str, name: &str) -> Result<()> {
        Self::check_server(server)?;
        if self.collab.shared.contains_key(&id) {
            return Err(unavailable(
                "suite.collab.share",
                "the document is shared already",
            ));
        }
        if self.collab.backend.is_none() {
            return Err(unavailable(
                "suite.collab.share",
                "collaboration is not loaded",
            ));
        }
        let now = self.host.now_ms();
        let d = self.doc_mut(id)?;
        let read_only = d.read_only;
        let m = d.model.collab().ok_or_else(|| {
            unavailable(
                "suite.collab.share",
                "this kind of document cannot be shared",
            )
        })?;
        let kind = m.collab_kind();
        let model = m.model_bytes()?;
        let name = display_name(name);
        let link = CollabLink {
            server: server.trim().trim_end_matches('/').to_owned(),
            room: random_room(),
            token: token.to_owned(),
        };
        let reply = self.engine(
            0,
            EngineCall::Share {
                kind,
                model,
                room: link.room.clone(),
                token: link.token.clone(),
                peer: random_peer(),
                name: name.clone(),
                now_ms: now,
            },
        )?;
        let EngineValue::Created(handle) = reply.value else {
            return Err(KilnError::Internal("the engine was not created".into()));
        };
        self.collab
            .shared
            .insert(id, new_shared(handle, link, name, false, read_only));
        if let Some(m) = self.doc_mut(id)?.model.collab() {
            m.set_shared(true);
        }
        self.absorb(id, &reply, false)?;
        self.connect(id);
        self.arm_timer(id);
        self.emit_collab(id);
        Ok(())
    }

    fn join(
        &mut self,
        kind: DocKind,
        server: &str,
        room: &str,
        token: &str,
        name: &str,
    ) -> Result<ExecResult> {
        Self::check_server(server)?;
        if room.trim().is_empty() {
            return Err(KilnError::InvalidInput(
                "a shared document's link needs its doc".into(),
            ));
        }
        if self.collab.backend.is_none() {
            return Err(unavailable(
                "suite.collab.join",
                "collaboration is not loaded",
            ));
        }
        let name = display_name(name);
        let reply = self.engine(
            0,
            EngineCall::Join {
                kind,
                room: room.trim().to_owned(),
                token: token.to_owned(),
                peer: random_peer(),
                name: name.clone(),
            },
        )?;
        let EngineValue::Created(handle) = reply.value else {
            return Err(KilnError::Internal("the engine was not created".into()));
        };
        let summary = match self.new_document(kind) {
            Ok(s) => s,
            Err(e) => {
                let _ = self.engine(handle, EngineCall::Close);
                return Err(e);
            }
        };
        let id = summary.doc;
        if let Ok(d) = self.doc_mut(id) {
            d.title = "Shared document".into();
            if let Some(m) = d.model.collab() {
                m.set_shared(true);
            }
        }
        let link = CollabLink {
            server: server.trim().trim_end_matches('/').to_owned(),
            room: room.trim().to_owned(),
            token: token.to_owned(),
        };
        self.collab
            .shared
            .insert(id, new_shared(handle, link, name, true, false));
        self.absorb(id, &reply, false)?;
        self.connect(id);
        self.arm_timer(id);
        self.emit_collab(id);
        Ok(ExecResult {
            outcome: Outcome::Applied,
            summary: Some(self.summary(id)?),
            value: ExecValue::None,
        })
    }

    fn preview(&mut self, id: DocId, version: Option<String>) -> Result<()> {
        let shared = self.collab.shared.get(&id).ok_or_else(|| {
            unavailable("suite.collab.version_preview", "the document is not shared")
        })?;
        if shared.preview == version {
            return Ok(());
        }
        let model = match &version {
            Some(v) => {
                let reply = self.call(id, EngineCall::Checkout { id: v.clone() }, false)?;
                match reply.value {
                    EngineValue::Model(m) => Some(m),
                    _ => return Err(KilnError::Internal("no model for the version".into())),
                }
            }
            None => None,
        };
        let d = self.doc_mut(id)?;
        if let Some(m) = d.model.collab() {
            m.preview(model.as_deref())?;
        }
        d.frames.reset();
        d.revision += 1;
        if let Some(s) = self.collab.shared.get_mut(&id) {
            s.preview = version;
        }
        self.emit_changed(id);
        self.emit_collab(id);
        Ok(())
    }

    /// Shows the document again in place of a version on show (before it is saved, printed or
    /// exported: those write the document, as Word does).
    pub(crate) fn collab_show_document(&mut self, id: DocId) {
        if self
            .collab
            .shared
            .get(&id)
            .is_some_and(|s| s.preview.is_some())
        {
            let _ = self.preview(id, None);
        }
    }

    /// Stops sharing a document (closed, or Stop Sharing).
    pub(crate) fn collab_close(&mut self, id: DocId) {
        if !self.collab.shared.contains_key(&id) {
            return;
        }
        let _ = self.preview(id, None);
        self.disconnect(id);
        if let Some(s) = self.collab.shared.remove(&id) {
            let _ = self.engine(s.handle, EngineCall::Close);
            if let Ok(d) = self.doc_mut(id) {
                d.read_only = s.read_only;
                if let Some(m) = d.model.collab() {
                    m.set_shared(false);
                }
            }
        }
    }

    /// After a command: the transactions it applied go to the engine, and where this person is
    /// now goes to the others.
    pub(crate) fn collab_after(&mut self, id: DocId) {
        let Some(s) = self.collab.shared.get(&id) else {
            return;
        };
        if s.joining || s.preview.is_some() {
            return;
        }
        let Ok(d) = self.doc_mut(id) else { return };
        let Some(m) = d.model.collab() else { return };
        let txs = m.take_applied();
        let now = self.host.now_ms();
        for tx in txs {
            if let Err(e) = self.call(id, EngineCall::Commit { tx, now_ms: now }, false) {
                self.collab_failed(id, &e);
                return;
            }
        }
        self.send_whereabouts(id);
    }

    /// Undo or Redo of a shared document: this person's changes only. Returns whether anything
    /// changed.
    pub(crate) fn collab_undo(&mut self, id: DocId, redo: bool) -> Result<bool> {
        if self.collab.locks(id) {
            return Err(unavailable("suite.edit.undo", "the document is read-only"));
        }
        let call = if redo {
            EngineCall::Redo
        } else {
            EngineCall::Undo
        };
        match self.call(id, call, true) {
            Ok(reply) => Ok(reply.change != CollabChange::None),
            Err(e) => {
                // A refused step clears the stack: the buttons hear of it.
                let _ = self.call(id, EngineCall::State, false);
                Err(e)
            }
        }
    }

    /// The document was saved: a version.
    pub(crate) fn collab_saved(&mut self, id: DocId) {
        if self.collab.is_shared(id) && !self.collab.locks(id) {
            let now = self.host.now_ms();
            let _ = self.call(id, EngineCall::Saved { now_ms: now }, false);
        }
    }

    // ----- queries -------------------------------------------------------------------------

    /// The collaboration queries (`None` for the others).
    pub(crate) fn collab_query(
        &mut self,
        doc: Option<DocId>,
        query: &Query,
    ) -> Option<Result<QueryResult>> {
        let id = doc?;
        let result = match query {
            Query::Collab => self.collab_state(id).map(QueryResult::Collab),
            Query::Versions => self.versions(id),
            Query::VersionCompare { id: v } => self
                .call(id, EngineCall::Compare { id: v.clone() }, false)
                .and_then(|r| match r.value {
                    EngineValue::Compare(c) => Ok(QueryResult::VersionCompare(c)),
                    _ => Err(KilnError::Internal("no comparison".into())),
                }),
            Query::VersionSlides { id: v, width, dpr } => self.version_slides(id, v, *width, *dpr),
            _ => return None,
        };
        Some(result)
    }

    fn versions(&mut self, id: DocId) -> Result<QueryResult> {
        if !self.collab.is_shared(id) {
            return Ok(QueryResult::Versions(Vec::new()));
        }
        match self.call(id, EngineCall::Versions, false)?.value {
            EngineValue::Versions(v) => Ok(QueryResult::Versions(v)),
            _ => Err(KilnError::Internal("no versions".into())),
        }
    }

    fn version_slides(&mut self, id: DocId, v: &str, width: f32, dpr: f32) -> Result<QueryResult> {
        let EngineValue::Slides(slides) = self
            .call(id, EngineCall::SlideChanges { id: v.to_owned() }, false)?
            .value
        else {
            return Err(KilnError::Internal("no slides".into()));
        };
        let EngineValue::Model(model) = self
            .call(id, EngineCall::Checkout { id: v.to_owned() }, false)?
            .value
        else {
            return Err(KilnError::Internal("no model for the version".into()));
        };
        let d = self.doc_mut(id)?;
        let m = d
            .model
            .collab()
            .ok_or_else(|| KilnError::Unsupported("versions of this document".into()))?;
        let scene = m.version_slides(&model, &slides, width, dpr)?;
        Ok(QueryResult::VersionSlides(Box::new(VersionSlides {
            slides,
            scene,
        })))
    }

    fn collab_state(&mut self, id: DocId) -> Result<CollabState> {
        let Some(s) = self.collab.shared.get(&id) else {
            self.doc(id)?;
            // a document that is not shared shows its local versions.
            let local = self
                .storage
                .docs
                .get(&id)
                .and_then(|s| s.preview.as_ref())
                .map(|p| p.id.clone());
            return Ok(CollabState {
                shared: false,
                status: CollabStatus::Offline,
                link: None,
                name: String::new(),
                initials: String::new(),
                colour: String::new(),
                peers: Vec::new(),
                queued: 0,
                preview: local,
                problem: None,
            });
        };
        let status = if s.refused {
            CollabStatus::Refused
        } else if s.online {
            CollabStatus::Online
        } else if s.connected || s.joining {
            CollabStatus::Connecting
        } else {
            CollabStatus::Offline
        };
        let mut state = CollabState {
            shared: true,
            status,
            link: Some(s.link.clone()),
            name: s.name.clone(),
            initials: initials(&s.name),
            colour: format!("{:06X}", s.colour & 0xFF_FFFF),
            peers: s.peers.clone(),
            queued: s.queued,
            preview: s.preview.clone(),
            problem: s.problem.clone(),
        };
        let d = self.doc_mut(id)?;
        if let Some(m) = d.model.collab() {
            for p in &mut state.peers {
                p.at = m.locate(p);
            }
        }
        Ok(state)
    }
}

fn require(doc: Option<DocId>) -> Result<DocId> {
    doc.ok_or_else(|| KilnError::InvalidInput("the command needs a document".into()))
}

/// The name shown to the others (a name is needed; "Guest" without one).
fn display_name(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        "Guest".to_owned()
    } else {
        n.chars().take(64).collect()
    }
}

fn new_shared(
    handle: u32,
    link: CollabLink,
    name: String,
    joining: bool,
    read_only: bool,
) -> Shared {
    Shared {
        handle,
        link,
        name,
        socket: 0,
        connected: false,
        opening: false,
        online: false,
        joining,
        refused: false,
        problem: None,
        peers: Vec::<CollabPeer>::new(),
        queued: 0,
        colour: 0,
        preview: None,
        read_only,
        sent: None,
        backoff_ms: FIRST_BACKOFF_MS,
        timer: false,
    }
}
