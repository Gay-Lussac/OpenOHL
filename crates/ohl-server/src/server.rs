// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The server: one authority thread owns the session and applies every
//! edit in one order; a task per connection reads requests and writes
//! what the authority broadcasts.
//!
//! Limits against users who would stop the others (security review
//! 2026-10-26): a job that fails inside the authority (a panic) is
//! refused to its client and the session read again from the file, the
//! server keeps running; joins are limited per address
//! ([`JOINS_PER_MINUTE`]), wrong access codes make that address wait
//! (doubling up to [`MAX_BACKOFF`]); connections not joined yet are
//! limited per address ([`MAX_PENDING`]); the copies of the project for
//! joining clients are shared while nothing changed and at most
//! [`MAX_COPIES`] exist at once; presence and lock messages are paced per
//! connection, the users' list broadcast at most every [`PEERS_EVERY`].
//! The limits of connections are the host's parameters
//! ([`Options::max_connections`], [`Options::max_per_address`]) and hold
//! for every address, the computer's own (loopback, e.g. behind a reverse
//! proxy) included (owner, 2026-10-26); only the join rate and the waits
//! after wrong codes leave loopback out.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ohl_session::{Role, Session};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::stream::{Deliver, Feed, FeedEnd, Item, Own, To, View};
use crate::{
    ALPN, Bucket, ClientMsg, Invitation, Keys, MAX_HELLO, Op, PROTOCOL, Peer, Presence, ServerMsg,
    execute, fingerprint, lock_target, random_code, read_msg, read_msg_max, write_encoded,
    write_msg,
};

/// Messages waiting for a client before its forwarder stops taking
/// broadcasts.
const OUT_QUEUE: usize = 256;

/// Locks one user may hold.
const MAX_LOCKS: usize = 256;

/// Copies of the project for joining clients at once (each as large as
/// the project): more joins wait.
pub const MAX_COPIES: usize = 8;

/// Users the general invitation adds in one run of the server.
pub const MAX_NEWCOMERS: usize = 200;

/// Joins (attempts) per address and minute (a burst of as many).
pub const JOINS_PER_MINUTE: f64 = 12.0;

/// Longest wait after wrong access codes, seconds (1 s after the first,
/// doubled after each next).
pub const MAX_BACKOFF: u64 = 300;

/// Connections of one address not joined yet (QUIC and web each), by
/// default ([`Options::max_per_address`]).
pub const MAX_PENDING: usize = 8;

/// QUIC connections at once, by default ([`Options::max_connections`]).
pub const MAX_QUIC: usize = 256;

/// The users' list is broadcast at most this often for presence changes
/// (the latest one wins).
pub const PEERS_EVERY: Duration = Duration::from_millis(100);

/// How a server is started.
#[derive(Debug, Clone)]
pub struct Options {
    /// Address and port to listen on (port 0: any free one).
    pub bind: SocketAddr,
    /// Host name or address given in the invitation (default: this
    /// computer's address on the local network).
    pub advertise: Option<String>,
    /// The secret the general access code is made from (default: a
    /// random one; give one to keep the general invitation across
    /// restarts). Personal invitations never derive from it.
    pub code: Option<String>,
    /// The file keeping the key of the personal invitations (made with a
    /// random key when missing, readable by its owner only; `None`: a
    /// random key for this run — the personal invitations change at the
    /// next start).
    pub keys: Option<PathBuf>,
    /// Role given to users the project does not know (`None`: they are
    /// refused).
    pub newcomers: Option<Role>,
    /// Also serve the web client (HTTPS and WebSocket over TCP) on this
    /// address and port ([`crate::web`]).
    pub web: Option<SocketAddr>,
    /// Connections at once (QUIC; more wait for a free one).
    pub max_connections: usize,
    /// Connections of one address not joined yet, every address (the
    /// computer's own too).
    pub max_per_address: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:4433".parse().expect("address"),
            advertise: None,
            code: None,
            keys: None,
            newcomers: Some(Role::Viewer),
            web: None,
            max_connections: MAX_QUIC,
            max_per_address: MAX_PENDING,
        }
    }
}

/// A server running (stopped when dropped).
pub struct Server {
    /// How to join it.
    pub invitation: Invitation,
    /// Where it listens.
    pub local_addr: SocketAddr,
    /// Where the web client is served, if it is.
    pub web_addr: Option<SocketAddr>,
    /// Host name or address given in the invitations.
    host: String,
    keys: Keys,
    pub(crate) jobs: std_mpsc::Sender<Job>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// One broadcast: a message for everyone (or, on a network, for the
/// clients whose interest it touches: [`To`]), with the request it answers
/// (client, request id) when it is an edit. Encoded once
/// ([`Event::new`]): every client is sent the same bytes (QUIC and web
/// clients alike: the message's JSON), and the message itself is shared,
/// not copied per client. `index`: its place among the broadcasts
/// (given when sent).
#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub(crate) index: u64,
    pub(crate) answers: Option<(u64, u64)>,
    pub(crate) msg: Arc<ServerMsg>,
    /// The message's JSON (or why it could not be written).
    pub(crate) text: Result<Utf8Bytes, String>,
    pub(crate) to: To,
    /// Network, phase 7: what a client that does not see everything gets
    /// instead (the users without their presence — a selection names
    /// objects, a camera a place —, no soft locks).
    pub(crate) redacted: Option<Result<Utf8Bytes, String>>,
}

impl Event {
    /// The broadcast of `msg` to everyone, encoded.
    pub(crate) fn new(answers: Option<(u64, u64)>, msg: ServerMsg) -> Self {
        Self::to(answers, msg, To::All)
    }

    /// The broadcast of `msg` to `to`, encoded.
    pub(crate) fn to(answers: Option<(u64, u64)>, msg: ServerMsg, to: To) -> Self {
        let encode = |m: &ServerMsg| {
            serde_json::to_string(m)
                .map(Utf8Bytes::from)
                .map_err(|e| e.to_string())
        };
        let text = encode(&msg);
        let redacted = redact(&msg).map(|m| encode(&m));
        Self {
            index: 0,
            answers,
            msg: Arc::new(msg),
            text,
            to,
            redacted,
        }
    }
}

/// What a client of a network that does not see everything gets of a
/// message for everyone (phase 7): the users without their presence, no
/// soft locks (their targets name objects). `None`: the message as it is.
pub(crate) fn redact(msg: &ServerMsg) -> Option<ServerMsg> {
    match msg {
        ServerMsg::Peers { peers } => Some(ServerMsg::Peers {
            peers: peers
                .iter()
                .map(|p| Peer {
                    presence: Presence::default(),
                    ..p.clone()
                })
                .collect(),
        }),
        ServerMsg::Locks { .. } => Some(ServerMsg::Locks {
            locks: BTreeMap::new(),
        }),
        _ => None,
    }
}

impl Event {
    /// The text client `restricted` (or not) gets.
    fn text_for(&self, restricted: bool) -> &Result<Utf8Bytes, String> {
        match (&self.redacted, restricted) {
            (Some(r), true) => r,
            _ => &self.text,
        }
    }
}

/// What one client's writer sends: a broadcast as encoded once, or a
/// message of its own (answers, notices).
enum Out {
    Shared(Result<Utf8Bytes, String>),
    Own(ServerMsg),
}

/// A copy of the project file for joining clients, removed when the last
/// one holding it is done.
pub(crate) struct Snapshot {
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) live: Arc<AtomicUsize>,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A joining client's copy of the project (or of a network's model): made
/// already, or being made (on a network, by a reader at a snapshot).
#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub(crate) enum CopyWait {
    Ready(Arc<Snapshot>),
    Making(oneshot::Receiver<Result<Arc<Snapshot>, String>>),
}

pub(crate) struct Joined {
    pub(crate) events: broadcast::Receiver<Event>,
    /// Network mode: what is for this client alone.
    pub(crate) own: Option<mpsc::Receiver<Own>>,
    /// The broadcasts sent before it joined.
    pub(crate) index: u64,
    /// The project file's copy (none for web clients and network clients
    /// that hold no model).
    pub(crate) copy: Option<CopyWait>,
    pub(crate) welcome: ServerMsg,
    /// What it receives of the broadcasts.
    pub(crate) view: View,
}

/// What a web client's scene is made from: the line and the libraries
/// as they are after `revision` edits.
pub(crate) struct SceneSource {
    pub(crate) revision: u64,
    pub(crate) seq: i64,
    pub(crate) line: ohl_line::Line,
    /// Kind → (file name, text), for the kinds a scene needs.
    pub(crate) libraries: BTreeMap<String, Vec<(String, String)>>,
}

/// How a client joins beyond its name and code (protocol 3's join; the
/// hello of protocol 2 gives the defaults).
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub(crate) struct JoinOptions {
    /// Send a copy of the project file (QUIC clients; on a network, of
    /// `model`).
    pub(crate) copy: bool,
    /// A web client (a viewer).
    pub(crate) web: bool,
    /// Network: the model it follows whole.
    pub(crate) model: Option<String>,
    /// Network: the changes after this position first (a catch-up).
    pub(crate) since: Option<crate::Position>,
    /// Network: its working set's routes.
    pub(crate) routes: Vec<String>,
    /// Network: the tiles it views.
    pub(crate) tiles: Vec<crate::HeldTile>,
    /// Network, phase 6: the working set it follows as a partial replica.
    pub(crate) working_set: Option<ohl_session::working_set::WorkingSetSpec>,
    /// Network, phase 7: an admin seeing as this user.
    pub(crate) see_as: Option<String>,
}

/// Network, phase 6: what a client asks about working sets.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub(crate) enum NetAsk {
    Pick {
        id: u64,
        model: String,
        picks: Vec<ohl_session::working_set::Pick>,
    },
    Set {
        spec: ohl_session::working_set::WorkingSetSpec,
    },
    Hold {
        objects: Vec<(ohl_session::Kind, String, u64)>,
    },
    Save {
        spec: ohl_session::working_set::WorkingSetSpec,
    },
    Delete {
        name: String,
    },
    List,
    RouteDetail {
        model: String,
        route: String,
    },
    /// Phase 10: a check on a working set, its neighbours pulled in.
    Check {
        id: u64,
        working_set: Option<ohl_session::working_set::WorkingSetSpec>,
        check: crate::ScopedCheck,
    },
}

#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub(crate) enum Job {
    Join {
        client: u64,
        user: String,
        code: String,
        options: JoinOptions,
        reply: oneshot::Sender<Result<Joined, String>>,
    },
    Request {
        client: u64,
        user: String,
        id: u64,
        op: Op,
        line: String,
        /// Network: the model (default: the one the client follows).
        model: Option<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Presence {
        user: String,
        presence: Presence,
    },
    Lock {
        user: String,
        target: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Unlock {
        user: String,
        target: String,
    },
    Leave {
        client: u64,
        user: String,
    },
    Peers {
        reply: std_mpsc::Sender<Vec<Peer>>,
    },
    /// The scene's source, unless the revision is `have` still.
    Scene {
        have: Option<u64>,
        reply: oneshot::Sender<Result<Option<SceneSource>, String>>,
    },
    /// Network: a client's interest declared again.
    Interest {
        client: u64,
        routes: Vec<String>,
        tiles: Vec<crate::HeldTile>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Network: a client catches up from a position.
    Catch {
        client: u64,
        since: crate::Position,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Network, phase 6: a question about working sets (answered on the
    /// client's own queue).
    Net {
        client: u64,
        user: String,
        ask: NetAsk,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Network: a tile package built by a packager.
    #[cfg(feature = "network")]
    Built(Box<crate::packager::Built>),
    /// Network: a measurement of the authority's own work (the bench).
    #[cfg(feature = "network")]
    Probe {
        what: crate::network::Probe,
        reply: std_mpsc::Sender<Result<String, String>>,
    },
    /// A job that fails inside the authority (tests).
    #[cfg(test)]
    Panic {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Stop,
}

/// What both authorities keep: who is connected, the soft locks, the
/// broadcasts, the keys.
pub(crate) struct Common {
    pub(crate) keys: Keys,
    pub(crate) newcomers: Option<Role>,
    /// Users the general invitation added in this run.
    pub(crate) added: usize,
    pub(crate) events: broadcast::Sender<Event>,
    /// Broadcasts sent.
    pub(crate) sent: u64,
    pub(crate) peers: BTreeMap<String, Peer>,
    pub(crate) locks: BTreeMap<String, String>,
    /// The users' list last broadcast, and when it is due again (a
    /// presence changed since).
    pub(crate) peers_sent: Instant,
    pub(crate) peers_due: Option<Instant>,
}

impl Common {
    pub(crate) fn new(
        keys: Keys,
        newcomers: Option<Role>,
        events: broadcast::Sender<Event>,
    ) -> Self {
        Self {
            keys,
            newcomers,
            added: 0,
            events,
            sent: 0,
            peers: BTreeMap::new(),
            locks: BTreeMap::new(),
            peers_sent: Instant::now(),
            peers_due: None,
        }
    }

    /// Sends a broadcast (its place given here).
    pub(crate) fn send(&mut self, mut event: Event) {
        self.sent += 1;
        event.index = self.sent;
        let _ = self.events.send(event);
    }

    pub(crate) fn broadcast(&mut self, msg: ServerMsg) {
        self.send(Event::new(None, msg));
    }

    pub(crate) fn peers_msg(&self) -> ServerMsg {
        ServerMsg::Peers {
            peers: self.peers.values().cloned().collect(),
        }
    }

    /// The users' list, to everyone at once.
    pub(crate) fn broadcast_peers(&mut self) {
        self.broadcast(self.peers_msg());
        self.peers_sent = Instant::now();
        self.peers_due = None;
    }

    /// The users' list after a presence changed: at most every
    /// [`PEERS_EVERY`] (the latest wins).
    fn presence_changed(&mut self) {
        let next = self.peers_sent + PEERS_EVERY;
        if Instant::now() >= next {
            self.broadcast_peers();
        } else {
            self.peers_due.get_or_insert(next);
        }
    }

    /// The access code checked: `Ok(personal)` (else the general
    /// invitation's); refused when it is neither, or the user is here.
    pub(crate) fn check_code(&self, user: &str, code: &str) -> Result<bool, String> {
        crate::check_name(user)?;
        let personal = crate::same_code(code, &self.keys.code(Some(user)));
        let general = crate::same_code(code, &self.keys.code(None));
        if !personal && !general {
            return Err(WRONG_CODE.into());
        }
        if self.peers.contains_key(user) {
            return Err(format!("{user} is connected already"));
        }
        Ok(personal)
    }

    /// A user joined: listed, the list broadcast.
    pub(crate) fn joined(&mut self, user: &str, role: Role) {
        self.peers.insert(
            user.to_owned(),
            Peer {
                user: user.to_owned(),
                role,
                presence: Presence::default(),
            },
        );
        self.broadcast_peers();
    }

    /// The soft locks others hold on `targets` (`None`: the whole line)
    /// of project line `line`: the first refused.
    pub(crate) fn locked(
        &self,
        user: &str,
        line: &str,
        targets: Option<Vec<(ohl_session::Kind, String)>>,
    ) -> Result<(), String> {
        let held: Vec<(&String, &String)> = self
            .locks
            .iter()
            .filter(|(_, u)| u.as_str() != user)
            .collect();
        if held.is_empty() {
            return Ok(());
        }
        match targets {
            None => {
                let (t, u) = held[0];
                Err(format!("{t} is being edited by {u}"))
            }
            Some(targets) => {
                for (kind, oid) in targets {
                    let t = lock_target(line, kind, &oid);
                    if let Some(u) = self.locks.get(&t).filter(|u| u.as_str() != user) {
                        return Err(format!("{t} is being edited by {u}"));
                    }
                }
                Ok(())
            }
        }
    }

    pub(crate) fn lock(&mut self, user: &str, target: String) -> Result<(), String> {
        if self.peers.get(user).is_none_or(|p| p.role < Role::Designer) {
            return Err(format!("{user} may not edit, so holds no lock"));
        }
        if target.len() > crate::MAX_LABEL {
            return Err("lock target too long".into());
        }
        match self.locks.get(&target) {
            Some(u) if u != user => Err(format!("{target} is being edited by {u}")),
            Some(_) => Ok(()),
            None if self.locks.values().filter(|u| *u == user).count() >= MAX_LOCKS => {
                Err(format!("{user} holds {MAX_LOCKS} locks already"))
            }
            None => {
                self.locks.insert(target, user.to_owned());
                self.broadcast(ServerMsg::Locks {
                    locks: self.locks.clone(),
                });
                Ok(())
            }
        }
    }

    pub(crate) fn unlock(&mut self, user: &str, target: &str) {
        if self.locks.get(target).is_some_and(|u| u == user) {
            self.locks.remove(target);
            self.broadcast(ServerMsg::Locks {
                locks: self.locks.clone(),
            });
        }
    }

    pub(crate) fn leave(&mut self, user: &str) {
        let before = self.locks.len();
        self.locks.retain(|_, u| u != user);
        if self.locks.len() != before {
            self.broadcast(ServerMsg::Locks {
                locks: self.locks.clone(),
            });
        }
        if self.peers.remove(user).is_some() {
            self.broadcast_peers();
        }
    }

    /// A presence message (cut to [`crate::MAX_LABEL`]).
    pub(crate) fn presence(&mut self, user: &str, mut presence: Presence) {
        if presence.selection.len() > crate::MAX_LABEL {
            let mut end = crate::MAX_LABEL;
            while !presence.selection.is_char_boundary(end) {
                end -= 1;
            }
            presence.selection.truncate(end);
        }
        if let Some(p) = self.peers.get_mut(user)
            && p.presence != presence
        {
            p.presence = presence;
            self.presence_changed();
        }
    }

    /// The jobs both authorities answer alike; the others are given back.
    pub(crate) fn handle(&mut self, job: Job) -> Option<Job> {
        match job {
            Job::Presence { user, presence } => self.presence(&user, presence),
            Job::Lock {
                user,
                target,
                reply,
            } => {
                let _ = reply.send(self.lock(&user, target));
            }
            Job::Unlock { user, target } => self.unlock(&user, &target),
            Job::Peers { reply } => {
                let _ = reply.send(self.peers.values().cloned().collect());
            }
            job => return Some(job),
        }
        None
    }
}

/// An authority: the one thread applying every request in one order.
pub(crate) trait Host {
    fn common(&mut self) -> &mut Common;
    /// A job (the common ones are answered before).
    fn handle(&mut self, job: Job);
    /// When it wants [`Host::tick`] next.
    fn due(&self) -> Option<Instant> {
        None
    }
    /// Its own timed work (heartbeats).
    fn tick(&mut self) {}
    /// It stops.
    fn stop(&mut self);
}

/// Runs an authority until stopped.
pub(crate) fn run<H: Host>(mut host: H, jobs: std_mpsc::Receiver<Job>) {
    loop {
        let due = [host.common().peers_due, host.due()]
            .into_iter()
            .flatten()
            .min();
        let job = match due {
            Some(due) => match jobs.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(job) => Some(job),
                Err(std_mpsc::RecvTimeoutError::Timeout) => None,
                Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match jobs.recv() {
                Ok(job) => Some(job),
                Err(_) => break,
            },
        };
        match job {
            Some(Job::Stop) => break,
            Some(job) => {
                if let Some(job) = host.common().handle(job) {
                    host.handle(job);
                }
            }
            None => {}
        }
        if host
            .common()
            .peers_due
            .is_some_and(|due| Instant::now() >= due)
        {
            host.common().broadcast_peers();
        }
        if host.due().is_some_and(|due| Instant::now() >= due) {
            host.tick();
        }
    }
    host.stop();
}

/// The authority of a project file: the session, who is connected, the
/// soft locks.
struct Authority {
    common: Common,
    session: Session,
    /// The project file (read again after a job failed inside).
    path: PathBuf,
    copies: PathBuf,
    /// Changes of the project file in this run (requests, users added):
    /// the copy for joining clients is made again after one.
    state: u64,
    /// The last copy made, and the state it is of.
    snapshot: Option<(u64, Arc<Snapshot>)>,
    /// Copies existing.
    live: Arc<AtomicUsize>,
    /// Edits of the line or its libraries since the server started.
    revision: u64,
}

impl Authority {
    /// The copy of the project for a joining client: the last one, when
    /// nothing changed since.
    fn snapshot(&mut self) -> Result<Arc<Snapshot>, String> {
        if let Some((state, s)) = &self.snapshot
            && *state == self.state
        {
            return Ok(s.clone());
        }
        // The old one goes when its last client has it.
        self.snapshot = None;
        if self.live.load(Ordering::SeqCst) >= MAX_COPIES {
            return Err(format!(
                "the server is sending the project to {MAX_COPIES} others: join again in a minute"
            ));
        }
        let path = self
            .copies
            .join(format!("copy-{}-{}.ohl", self.state, random_code(8)));
        let size = self
            .session
            .copy_for_others(&path)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                std::fs::metadata(&path)
                    .map(|m| m.len())
                    .map_err(|e| e.to_string())
            });
        let size = match size {
            Ok(size) => size,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
        };
        self.live.fetch_add(1, Ordering::SeqCst);
        let snapshot = Arc::new(Snapshot {
            path,
            size,
            live: self.live.clone(),
        });
        self.snapshot = Some((self.state, snapshot.clone()));
        Ok(snapshot)
    }

    fn join(&mut self, user: &str, code: &str, copy: bool) -> Result<Joined, String> {
        let personal = self.common.check_code(user, code)?;
        let role = match self.session.role(user).map_err(|e| e.to_string())? {
            Some(_) if !personal => {
                return Err(format!(
                    "{user} is a user of this project: join with your personal invitation \
                     (ask the host)"
                ));
            }
            Some(r) => r,
            None => {
                let role = self
                    .common
                    .newcomers
                    .ok_or_else(|| format!("{user} is not a user of this project"))?;
                if self.common.added >= MAX_NEWCOMERS {
                    return Err(format!(
                        "{MAX_NEWCOMERS} newcomers joined since the server started: ask the \
                         host for a personal invitation"
                    ));
                }
                let users = self.session.users().map_err(|e| e.to_string())?;
                crate::check_look_alike(user, users.iter().map(|(n, _)| n.as_str()))?;
                // Added by the project's admin, like every replica will.
                let admin = users
                    .into_iter()
                    .find(|(_, r)| *r == Role::Admin)
                    .map(|(n, _)| n)
                    .ok_or("the project has no admin")?;
                let op = Op::SetUser {
                    name: user.to_owned(),
                    role,
                };
                let line = self.session.current_line().to_owned();
                self.state += 1;
                execute(&mut self.session, &admin, &line, &op).map_err(|e| e.to_string())?;
                self.common.added += 1;
                self.common.broadcast(ServerMsg::Applied {
                    user: admin,
                    op,
                    line,
                    seq: None,
                });
                role
            }
        };
        // Subscribed before the copy: nothing applied after it is missed.
        let events = self.common.events.subscribe();
        let index = self.common.sent;
        let (copy, size) = if copy {
            let s = self.snapshot()?;
            let size = s.size;
            (Some(CopyWait::Ready(s)), size)
        } else {
            (None, 0)
        };
        let seq = self.session.last_seq().map_err(|e| e.to_string())?;
        self.common.joined(user, role);
        Ok(Joined {
            events,
            own: None,
            index,
            copy,
            welcome: ServerMsg::Welcome {
                project: self.session.line().name.clone(),
                role,
                seq,
                size,
                peers: self.common.peers.values().cloned().collect(),
                locks: self.common.locks.clone(),
            },
            view: View::everything(),
        })
    }

    /// The objects `op` changes (for the soft locks); an undo or redo
    /// those of the log entry it reverses (`None`: the whole line).
    fn targets(
        &self,
        user: &str,
        op: &Op,
        line: &str,
    ) -> Result<Option<Vec<(ohl_session::Kind, String)>>, String> {
        targets(&self.session, user, op, line)
    }

    fn request(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        op: Op,
        line: String,
    ) -> Result<(), String> {
        if matches!(op, Op::UpdateLibraryPins { .. } | Op::LibraryPins { .. }) {
            return Err(format!(
                "library pins are a network's (phase 9); {}",
                FILE_MODE.split(':').next().unwrap_or(FILE_MODE)
            ));
        }
        // Soft locks: others' objects are refused (admins pass).
        let admin = self.session.role(user).map_err(|e| e.to_string())? == Some(Role::Admin);
        if !admin && self.common.locks.values().any(|u| u != user) {
            let targets = self.targets(user, &op, &line)?;
            self.common.locked(user, &line, targets)?;
        }
        // Whatever happens, the copy for joining clients is made again.
        self.state += 1;
        let applied = execute(&mut self.session, user, &line, &op).map_err(|e| e.to_string())?;
        if let Op::SetSetting { key, value } = &op {
            log(&format!(
                "{user} {} setting {key} on {line}",
                if value.is_some() {
                    "changed"
                } else {
                    "removed"
                }
            ));
        }
        if crate::web::changes_scene(&op, &line) {
            self.revision += 1;
        }
        let role_changed = matches!(&op, Op::SetUser { .. });
        self.common.send(Event::new(
            Some((client, id)),
            ServerMsg::Applied {
                user: user.to_owned(),
                op: op.clone(),
                line,
                seq: applied.map(|a| a.seq),
            },
        ));
        if let (true, Op::SetUser { name, role }) = (role_changed, &op)
            && let Some(p) = self.common.peers.get_mut(name)
        {
            p.role = *role;
            self.common.broadcast_peers();
        }
        Ok(())
    }

    fn scene(&self, have: Option<u64>) -> Result<Option<SceneSource>, String> {
        if have == Some(self.revision) {
            return Ok(None);
        }
        let mut libraries = BTreeMap::new();
        for kind in crate::web::SCENE_LIBRARIES {
            let files = self
                .session
                .library_files(kind)
                .map_err(|e| e.to_string())?;
            libraries.insert(kind.to_string(), files);
        }
        Ok(Some(SceneSource {
            revision: self.revision,
            seq: self.session.last_seq().map_err(|e| e.to_string())?,
            // The main line (else the line the server has open).
            line: self
                .session
                .line_named(ohl_session::versions::MAIN)
                .unwrap_or_else(|_| self.session.line())
                .clone(),
            libraries,
        }))
    }

    /// Runs `f`; a panic inside it (a bug) is refused instead of stopping
    /// the server, and the session read again from the project file (what
    /// the job had not stored is gone with it).
    fn guarded<T>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *self))) {
            Ok(r) => r,
            Err(_) => {
                log(&format!(
                    "{what} failed inside the server (a bug): refused; the project is read \
                     again from its file"
                ));
                self.recover();
                Err(format!(
                    "{what}: refused, an internal error of the server (it keeps running)"
                ))
            }
        }
    }

    /// After a job failed inside: the session as the project file holds
    /// it, the copy for joining clients made again.
    fn recover(&mut self) {
        self.state += 1;
        self.snapshot = None;
        match Session::open(&self.path) {
            Ok(s) => self.session = s,
            Err(e) => log(&format!("reading the project again: {e}")),
        }
    }
}

impl Host for Authority {
    fn common(&mut self) -> &mut Common {
        &mut self.common
    }

    fn handle(&mut self, job: Job) {
        match job {
            Job::Join {
                user,
                code,
                options,
                reply,
                ..
            } => {
                let _ = reply.send(self.guarded("joining", |a| a.join(&user, &code, options.copy)));
            }
            Job::Request {
                client,
                user,
                id,
                op,
                line,
                reply,
                ..
            } => {
                let what = op.describe();
                let _ = reply.send(self.guarded(&what, |a| a.request(client, &user, id, op, line)));
            }
            Job::Leave { user, .. } => self.common.leave(&user),
            Job::Scene { have, reply } => {
                let _ = reply.send(self.guarded("the scene", |a| a.scene(have)));
            }
            Job::Interest { reply, .. } | Job::Catch { reply, .. } | Job::Net { reply, .. } => {
                let _ = reply.send(Err(FILE_MODE.into()));
            }
            #[cfg(feature = "network")]
            Job::Built(_) => {}
            #[cfg(feature = "network")]
            Job::Probe { reply, .. } => {
                let _ = reply.send(Err(FILE_MODE.into()));
            }
            #[cfg(test)]
            Job::Panic { reply } => {
                let _ = reply.send(self.guarded("a test", |_| -> Result<(), String> {
                    panic!("a job failing inside the authority")
                }));
            }
            Job::Presence { .. }
            | Job::Lock { .. }
            | Job::Unlock { .. }
            | Job::Peers { .. }
            | Job::Stop => {}
        }
    }

    fn stop(&mut self) {
        self.snapshot = None;
        let _ = self.session.checkpoint();
        let _ = std::fs::remove_dir_all(&self.copies);
    }
}

/// Why a network's message is refused by a server of a project file.
pub(crate) const FILE_MODE: &str =
    "this server hosts a project file: interest, tiles and catch-up are for a network's server";

/// The objects `op` of `user` on project line `line` changes (for the
/// soft locks); an undo or redo those of the log entry it reverses
/// (`None`: the whole line).
pub(crate) fn targets(
    session: &Session,
    user: &str,
    op: &Op,
    line: &str,
) -> Result<Option<Vec<(ohl_session::Kind, String)>>, String> {
    let entry = match op {
        Op::Undo => session.undo_entry(user, line),
        Op::Redo => session.redo_entry(user, line),
        _ => return Ok(op.targets()),
    }
    .map_err(|e| e.to_string())?;
    Ok(Some(match entry {
        Some(seq) => session.entry_objects(seq).map_err(|e| e.to_string())?,
        None => Vec::new(),
    }))
}

/// The refusal of a wrong access code (the address then waits).
pub(crate) const WRONG_CODE: &str = "wrong access code";

/// What one address did lately.
struct Seen {
    /// Connections not joined yet.
    pending: usize,
    joins: Bucket,
    /// Wrong access codes in a row.
    wrong: u32,
    /// No join before.
    wait_until: Option<Instant>,
    last: Instant,
}

/// Limits per address (QUIC and web): joins, wrong access codes,
/// connections not joined yet. Loopback addresses (this computer) pass.
pub(crate) struct Gate {
    seen: Mutex<HashMap<IpAddr, Seen>>,
    /// Connections of one address not joined yet.
    per_address: usize,
}

impl Default for Gate {
    fn default() -> Self {
        Self::new(MAX_PENDING)
    }
}

impl Gate {
    /// A gate letting `per_address` connections of one address wait for
    /// their join (at least 1).
    pub(crate) fn new(per_address: usize) -> Self {
        Self {
            seen: Mutex::default(),
            per_address: per_address.max(1),
        }
    }
}

/// A connection not joined yet (released when dropped).
pub(crate) struct Pending {
    gate: Arc<Gate>,
    ip: IpAddr,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut seen) = self.gate.seen.lock()
            && let Some(s) = seen.get_mut(&self.ip)
        {
            s.pending = s.pending.saturating_sub(1);
        }
    }
}

impl Gate {
    fn with<T>(&self, ip: IpAddr, f: impl FnOnce(&mut Seen) -> T) -> T {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        // Forgotten after ten quiet minutes.
        if seen.len() > 4096 {
            seen.retain(|_, s| {
                s.pending > 0 || now.duration_since(s.last) < Duration::from_secs(600)
            });
        }
        let s = seen.entry(ip).or_insert_with(|| Seen {
            pending: 0,
            joins: Bucket::new(JOINS_PER_MINUTE / 60.0, JOINS_PER_MINUTE),
            wrong: 0,
            wait_until: None,
            last: now,
        });
        s.last = now;
        f(s)
    }

    /// A new connection of `ip`, if it may have one more not joined yet.
    pub(crate) fn enter(self: &Arc<Self>, ip: IpAddr) -> Option<Pending> {
        let ok = self.with(ip, |s| {
            if s.pending >= self.per_address {
                false
            } else {
                s.pending += 1;
                true
            }
        });
        ok.then(|| Pending {
            gate: self.clone(),
            ip,
        })
    }

    /// Whether `ip` may try to join now.
    pub(crate) fn may_join(&self, ip: IpAddr) -> Result<(), String> {
        if ip.is_loopback() {
            return Ok(());
        }
        self.with(ip, |s| {
            if let Some(until) = s.wait_until {
                let now = Instant::now();
                if now < until {
                    return Err(format!(
                        "wrong access codes from this address: try again in {} s",
                        (until - now).as_secs() + 1
                    ));
                }
            }
            if !s.joins.take() {
                return Err(format!(
                    "more than {JOINS_PER_MINUTE} joins a minute from this address: try again \
                     in a minute"
                ));
            }
            Ok(())
        })
    }

    /// What a join of `ip` came to: a wrong access code makes it wait
    /// (1 s, doubled each time up to [`MAX_BACKOFF`]); a join forgets them.
    pub(crate) fn joined(&self, ip: IpAddr, result: &Result<(), String>) {
        if ip.is_loopback() {
            return;
        }
        self.with(ip, |s| match result {
            Err(e) if e == WRONG_CODE => {
                s.wrong = s.wrong.saturating_add(1);
                let wait = (1u64 << (s.wrong - 1).min(16)).min(MAX_BACKOFF);
                s.wait_until = Some(Instant::now() + Duration::from_secs(wait));
            }
            Err(_) => {}
            Ok(()) => {
                s.wrong = 0;
                s.wait_until = None;
            }
        });
    }
}

/// Per connection: presence and lock messages paced.
pub(crate) struct Pace {
    presence: Bucket,
    locks: Bucket,
    /// A network's interest and catch-up requests (each may make the
    /// server build tiles or read its stream).
    interest: Bucket,
}

impl Pace {
    pub(crate) fn new() -> Self {
        Self {
            // The applications send their presence when it changes, a few
            // times a second at most.
            presence: Bucket::new(10.0, 20.0),
            locks: Bucket::new(20.0, 64.0),
            // The web client asks for its tiles at most every 250 ms.
            interest: Bucket::new(4.0, 8.0),
        }
    }
}

/// Joins at the authority, within the address's limits.
pub(crate) async fn join(
    gate: &Gate,
    ip: IpAddr,
    jobs: &std_mpsc::Sender<Job>,
    client: u64,
    user: String,
    code: String,
    options: JoinOptions,
) -> Result<Result<Joined, String>, String> {
    if let Err(e) = gate.may_join(ip) {
        return Ok(Err(e));
    }
    let (tx, rx) = oneshot::channel();
    jobs.send(Job::Join {
        client,
        user,
        code,
        options,
        reply: tx,
    })
    .map_err(|_| "server stopped")?;
    let joined = rx.await.map_err(|_| "server stopped")?;
    gate.joined(ip, &joined.as_ref().map(|_| ()).map_err(Clone::clone));
    Ok(joined)
}

/// This computer's address on the local network (the one used to reach
/// the internet; no packet is sent), else localhost.
pub fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn tls_server(cert: &rcgen::CertifiedKey<rcgen::KeyPair>) -> Result<quinn::ServerConfig, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()));
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert.cert.der().to_vec())], key)
        .map_err(|e| e.to_string())?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).map_err(|e| e.to_string())?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    config.transport_config(Arc::new(transport()));
    Ok(config)
}

/// Keep-alive every 5 s; a peer silent for 30 s is gone.
pub(crate) fn transport() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(Duration::from_secs(5)));
    t.max_idle_timeout(Duration::from_secs(30).try_into().ok());
    t
}

/// What a server is known by: its certificate, its keys, its address.
pub(crate) struct Identity {
    pub(crate) config: quinn::ServerConfig,
    pub(crate) acceptor: Option<tokio_rustls::TlsAcceptor>,
    pub(crate) keys: Keys,
    pub(crate) code: String,
    pub(crate) host: String,
    pub(crate) fingerprint: [u8; 32],
}

/// An authority running, about to listen.
pub(crate) struct Listening {
    pub(crate) config: quinn::ServerConfig,
    pub(crate) acceptor: Option<tokio_rustls::TlsAcceptor>,
    pub(crate) keys: Keys,
    pub(crate) code: String,
    pub(crate) host: String,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) jobs: std_mpsc::Sender<Job>,
    pub(crate) authority_thread: std::thread::JoinHandle<()>,
}

impl Server {
    /// The certificate, keys and address of a server started with
    /// `options`.
    pub(crate) fn identity(options: &Options) -> Result<Identity, String> {
        let host = options.advertise.clone().unwrap_or_else(|| {
            if options.bind.ip().is_unspecified() {
                local_ip()
            } else {
                options.bind.ip().to_string()
            }
        });
        // Names a browser may check (it still warns: self-signed).
        let mut names = vec!["ohl-server".to_owned(), host.clone()];
        for n in ["localhost", "127.0.0.1"] {
            if !names.iter().any(|x| x == n) {
                names.push(n.to_owned());
            }
        }
        let cert = rcgen::generate_simple_self_signed(names).map_err(|e| e.to_string())?;
        let fp = fingerprint(cert.cert.der());
        let config = tls_server(&cert)?;
        let acceptor = match options.web {
            Some(_) => Some(crate::web::acceptor(&cert)?),
            None => None,
        };
        let secret = options.code.clone().unwrap_or_else(|| random_code(16));
        let personal = match &options.keys {
            Some(file) => Keys::personal_key_file(file)?,
            None => {
                let mut key = [0u8; 32];
                getrandom::fill(&mut key).map_err(|e| e.to_string())?;
                key
            }
        };
        let keys = Keys::new(&secret, personal);
        let code = keys.code(None);
        Ok(Identity {
            config,
            acceptor,
            keys,
            code,
            host,
            fingerprint: fp,
        })
    }

    /// Hosts the project `path` (opened here: the server is then its only
    /// writer). Returns once it listens.
    pub fn start(path: &Path, options: Options) -> Result<Server, String> {
        let session = Session::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !session.has_line() {
            return Err(format!("{}: no line in the project", path.display()));
        }
        let Identity {
            config,
            acceptor,
            keys,
            code,
            host,
            fingerprint: fp,
        } = Self::identity(&options)?;
        let copies = crate::private_temp_dir("ohl-server")?;
        let (events, _) = broadcast::channel(4096);
        let (jobs, job_rx) = std_mpsc::channel();
        let authority = Authority {
            common: Common::new(keys.clone(), options.newcomers, events),
            session,
            path: path.to_path_buf(),
            copies,
            state: 0,
            snapshot: None,
            live: Arc::new(AtomicUsize::new(0)),
            revision: 0,
        };
        let authority_thread = std::thread::Builder::new()
            .name("ohl-authority".into())
            .spawn(move || run(authority, job_rx))
            .map_err(|e| e.to_string())?;
        Self::listen(
            options,
            Listening {
                config,
                acceptor,
                keys,
                code,
                host,
                fingerprint: fp,
                jobs,
                authority_thread,
            },
        )
    }

    /// Listens for clients of the authority already running (both modes).
    pub(crate) fn listen(options: Options, l: Listening) -> Result<Server, String> {
        let Listening {
            config,
            acceptor,
            keys,
            code,
            host,
            fingerprint: fp,
            jobs,
            authority_thread,
        } = l;
        let (stop, stop_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = std_mpsc::channel();
        let net_jobs = jobs.clone();
        let stop_jobs = jobs.clone();
        let bind = options.bind;
        let web_bind = options.web;
        let thread = std::thread::Builder::new()
            .name("ohl-server".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                rt.block_on(async move {
                    let endpoint = match quinn::Endpoint::server(config, bind) {
                        Ok(e) => e,
                        Err(e) => {
                            let _ = ready_tx.send(Err(format!("{bind}: {e}")));
                            return;
                        }
                    };
                    let quic_addr = match endpoint.local_addr() {
                        Ok(a) => a,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                    let web = match (web_bind, acceptor) {
                        (Some(addr), Some(acceptor)) => {
                            match tokio::net::TcpListener::bind(addr).await {
                                Ok(l) => Some((l, acceptor)),
                                Err(e) => {
                                    let _ = ready_tx.send(Err(format!("{addr} (web): {e}")));
                                    return;
                                }
                            }
                        }
                        _ => None,
                    };
                    let web_addr = web.as_ref().and_then(|(l, _)| l.local_addr().ok());
                    let _ = ready_tx.send(Ok((quic_addr, web_addr)));
                    let gate = Arc::new(Gate::new(options.max_per_address));
                    let web_task = web.map(|(listener, acceptor)| {
                        tokio::spawn(crate::web::listen(
                            listener,
                            acceptor,
                            net_jobs.clone(),
                            gate.clone(),
                        ))
                    });
                    let slots = Arc::new(tokio::sync::Semaphore::new(options.max_connections.max(1)));
                    let mut next = 0u64;
                    let mut stop_rx = stop_rx;
                    loop {
                        tokio::select! {
                            _ = &mut stop_rx => break,
                            incoming = endpoint.accept() => {
                                let Some(incoming) = incoming else { break };
                                // At most so many connections, and so many
                                // of one address not joined yet.
                                let (Ok(slot), Some(pending)) = (
                                    slots.clone().try_acquire_owned(),
                                    gate.enter(incoming.remote_address().ip()),
                                ) else {
                                    incoming.refuse();
                                    continue;
                                };
                                next += 1;
                                let (jobs, gate) = (net_jobs.clone(), gate.clone());
                                tokio::spawn(async move {
                                    if let Err(e) = connection(incoming, next, jobs, gate, pending).await {
                                        log(&format!("connection {next}: {e}"));
                                    }
                                    drop(slot);
                                });
                            }
                        }
                    }
                    if let Some(t) = web_task {
                        t.abort();
                    }
                    endpoint.close(0u32.into(), b"server stopped");
                    endpoint.wait_idle().await;
                });
                let _ = stop_jobs.send(Job::Stop);
                let _ = authority_thread.join();
            })
            .map_err(|e| e.to_string())?;
        let (local_addr, web_addr) = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "the server did not start".to_owned())??;
        Ok(Server {
            invitation: Invitation {
                address: format!("{host}:{}", local_addr.port()),
                code,
                fingerprint: fp,
            },
            local_addr,
            web_addr,
            host,
            keys,
            jobs,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The keys the access codes are made from (never sent).
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// The personal invitation of `user` (a user of the project joins
    /// only with theirs).
    pub fn invitation_for(&self, user: &str) -> Invitation {
        Invitation {
            code: self.keys.code(Some(user)),
            ..self.invitation.clone()
        }
    }

    /// The web client's link (when it is served), with the general access
    /// code or `user`'s personal one in the fragment, which browsers do
    /// not send to the server: `https://HOST:PORT/#user=NAME&code=CODE`.
    pub fn web_link(&self, user: Option<&str>) -> Option<String> {
        let addr = self.web_addr?;
        let code = self.keys.code(user);
        let who = user
            .map(|u| format!("user={}&", crate::web::fragment_escape(u)))
            .unwrap_or_default();
        Some(format!(
            "https://{}:{}/#{who}code={code}",
            self.host,
            addr.port()
        ))
    }

    /// The users connected.
    pub fn peers(&self) -> Vec<Peer> {
        let (tx, rx) = std_mpsc::channel();
        if self.jobs.send(Job::Peers { reply: tx }).is_err() {
            return Vec::new();
        }
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    /// Stops: the connections are closed, the project written through.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn log(text: &str) {
    eprintln!("ohl-server: {text}");
}

/// One client: hello, the copy, then requests in and broadcasts out.
async fn connection(
    incoming: quinn::Incoming,
    client: u64,
    jobs: std_mpsc::Sender<Job>,
    gate: Arc<Gate>,
    pending: Pending,
) -> Result<(), String> {
    let ip = incoming.remote_address().ip();
    let conn = incoming.await.map_err(|e| e.to_string())?;
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(10), conn.accept_bi())
        .await
        .map_err(|_| "no stream in 10 s".to_owned())?
        .map_err(|e| e.to_string())?;
    let hello: Option<ClientMsg> =
        tokio::time::timeout(Duration::from_secs(10), read_msg_max(&mut recv, MAX_HELLO))
            .await
            .map_err(|_| "no hello in 10 s".to_owned())??;
    let (protocol, user, code, options) = match hello {
        Some(ClientMsg::Hello {
            protocol,
            user,
            code,
        }) => (
            protocol,
            user,
            code,
            JoinOptions {
                copy: true,
                ..JoinOptions::default()
            },
        ),
        Some(ClientMsg::Join {
            protocol,
            user,
            code,
            model,
            since,
            routes,
            tiles,
            held,
            working_set,
            see_as,
        }) => (
            protocol,
            user,
            code,
            JoinOptions {
                copy: (model.is_some() || working_set.is_some()) && !held,
                web: false,
                model,
                since,
                routes,
                tiles,
                working_set,
                see_as,
            },
        ),
        _ => return Err("no hello".into()),
    };
    if protocol != PROTOCOL {
        let reason = crate::protocol_refusal(protocol);
        write_msg(&mut send, &ServerMsg::Refused { reason }).await?;
        let _ = send.finish();
        // Let the refusal arrive before the connection goes.
        tokio::time::sleep(Duration::from_millis(200)).await;
        return Ok(());
    }
    let joined = join(&gate, ip, &jobs, client, user.clone(), code, options).await?;
    drop(pending);
    let joined = match joined {
        Ok(j) => j,
        Err(reason) => {
            write_msg(&mut send, &ServerMsg::Refused { reason }).await?;
            let _ = send.finish();
            // Let the refusal arrive before the connection goes.
            tokio::time::sleep(Duration::from_millis(200)).await;
            return Ok(());
        }
    };
    log(&format!("{user} joined from {}", conn.remote_address()));
    let result = serve(&conn, &mut send, &mut recv, client, &user, joined, &jobs).await;
    let _ = jobs.send(Job::Leave {
        client,
        user: user.clone(),
    });
    log(&format!("{user} left"));
    result
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    conn: &quinn::Connection,
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    client: u64,
    user: &str,
    joined: Joined,
    jobs: &std_mpsc::Sender<Job>,
) -> Result<(), String> {
    let Joined {
        events,
        own,
        index,
        copy,
        mut welcome,
        view,
    } = joined;
    // The copy, on its own stream; removed whatever happens.
    let copy = match copy {
        Some(CopyWait::Ready(c)) => Some(c),
        Some(CopyWait::Making(rx)) => Some(
            rx.await
                .map_err(|_| "server stopped".to_owned())
                .and_then(|r| r)
                .map_err(|e| format!("the copy of the model: {e}"))?,
        ),
        None => None,
    };
    if let (Some(c), ServerMsg::Welcome { size, .. }) = (&copy, &mut welcome) {
        *size = c.size;
    }
    match copy {
        Some(copy) => {
            // A client that does not take it in good time is dropped (it
            // holds a copy of the project meanwhile).
            let deadline = Duration::from_secs(60 + copy.size / (256 << 10));
            let sent = match write_msg(send, &welcome).await {
                Ok(()) => tokio::time::timeout(deadline, send_file(conn, &copy.path))
                    .await
                    .unwrap_or_else(|_| {
                        Err(format!("project not taken in {} s", deadline.as_secs()))
                    }),
                Err(e) => Err(e),
            };
            drop(copy);
            sent?;
        }
        None => write_msg(send, &welcome).await?,
    }
    // One writer: broadcasts (with the answers to this client's requests
    // right after their edits) and this client's notices.
    // Bounded: a client that stops reading holds the forwarder, which then
    // lags behind the broadcasts and drops the client.
    let (out, mut out_rx) = mpsc::channel::<Out>(OUT_QUEUE);
    let mut feed = Feed::new(events, own, index);
    let mut view = view;
    let forward = {
        let out = out.clone();
        let conn = conn.clone();
        tokio::spawn(async move {
            loop {
                match feed.next().await {
                    Ok(item) => {
                        let (deliver, answer) = deliver(&mut view, client, item);
                        let deliver = deliver.map(|d| match d {
                            Deliver::Shared(text) => Out::Shared(Ok(text)),
                            Deliver::Own(m) => Out::Own(*m),
                        });
                        if let Some(d) = deliver
                            && out.send(d).await.is_err()
                        {
                            break;
                        }
                        if let Some(a) = answer
                            && out.send(Out::Own(a)).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(FeedEnd::Lagged) => {
                        let _ = out.try_send(Out::Own(ServerMsg::Notice {
                            text: "too far behind the server: join again".into(),
                        }));
                        // A client that reads gets the notice first.
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        conn.close(0u32.into(), b"too far behind");
                        break;
                    }
                    Err(FeedEnd::LetGo) => {
                        let _ = out.try_send(Out::Own(ServerMsg::Notice {
                            text: "the server let this connection go: join again".into(),
                        }));
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        conn.close(0u32.into(), b"let go");
                        break;
                    }
                    Err(FeedEnd::Closed) => break,
                }
            }
        })
    };
    let mut pace = Pace::new();
    let reading = async {
        loop {
            let Some(msg) = read_msg::<ClientMsg>(recv).await? else {
                return Ok::<(), String>(());
            };
            if let Some(answer) = dispatch(msg, client, user, jobs, &mut pace).await? {
                let _ = out.send(Out::Own(answer)).await;
            }
        }
    };
    let writing = async {
        while let Some(out) = out_rx.recv().await {
            let msg = match out {
                Out::Shared(text) => {
                    write_encoded(send, text?.as_bytes()).await?;
                    continue;
                }
                Out::Own(msg) => msg,
            };
            let last =
                matches!(&msg, ServerMsg::Notice { text } if text.starts_with("too far behind"));
            write_msg(send, &msg).await?;
            if last {
                return Err::<(), String>("client too far behind".into());
            }
        }
        Ok(())
    };
    let result = tokio::select! {
        r = reading => r,
        r = writing => r,
        _ = conn.closed() => Ok(()),
    };
    forward.abort();
    result
}

/// Passes a joined client's message to the authority; returns what to
/// tell that client at once (a refusal), if anything. QUIC and web
/// clients alike.
pub(crate) async fn dispatch(
    msg: ClientMsg,
    client: u64,
    user: &str,
    jobs: &std_mpsc::Sender<Job>,
    pace: &mut Pace,
) -> Result<Option<ServerMsg>, String> {
    Ok(match msg {
        ClientMsg::Hello { .. } | ClientMsg::Join { .. } => return Err("hello twice".into()),
        ClientMsg::Interest { .. } | ClientMsg::Catch { .. } if !pace.interest.take() => {
            Some(ServerMsg::Notice {
                text: "too many interest or catch-up requests: slow down".into(),
            })
        }
        ClientMsg::Interest { routes, tiles } => {
            let (tx, rx) = oneshot::channel();
            jobs.send(Job::Interest {
                client,
                routes,
                tiles,
                reply: tx,
            })
            .map_err(|_| "server stopped")?;
            rx.await
                .map_err(|_| "server stopped")?
                .err()
                .map(|text| ServerMsg::Notice { text })
        }
        ClientMsg::Catch { since } => {
            let (tx, rx) = oneshot::channel();
            jobs.send(Job::Catch {
                client,
                since,
                reply: tx,
            })
            .map_err(|_| "server stopped")?;
            rx.await
                .map_err(|_| "server stopped")?
                .err()
                .map(|text| ServerMsg::Notice { text })
        }
        ClientMsg::RequestOn {
            id,
            model,
            op,
            line,
        } => request(jobs, client, user, id, op, line, Some(model)).await?,
        ClientMsg::Pick { .. }
        | ClientMsg::SetWorkingSet { .. }
        | ClientMsg::Hold { .. }
        | ClientMsg::SaveWorkingSet { .. }
        | ClientMsg::DeleteWorkingSet { .. }
        | ClientMsg::WorkingSets
        | ClientMsg::RouteDetail { .. }
        | ClientMsg::Check { .. }
            if !pace.interest.take() =>
        {
            Some(ServerMsg::Notice {
                text: "too many working set requests: slow down".into(),
            })
        }
        ClientMsg::Pick { id, model, picks } => {
            net_ask(jobs, client, user, NetAsk::Pick { id, model, picks }).await?
        }
        ClientMsg::SetWorkingSet { spec } => {
            net_ask(jobs, client, user, NetAsk::Set { spec }).await?
        }
        ClientMsg::Hold { objects } => {
            net_ask(jobs, client, user, NetAsk::Hold { objects }).await?
        }
        ClientMsg::SaveWorkingSet { spec } => {
            net_ask(jobs, client, user, NetAsk::Save { spec }).await?
        }
        ClientMsg::DeleteWorkingSet { name } => {
            net_ask(jobs, client, user, NetAsk::Delete { name }).await?
        }
        ClientMsg::WorkingSets => net_ask(jobs, client, user, NetAsk::List).await?,
        ClientMsg::RouteDetail { model, route } => {
            net_ask(jobs, client, user, NetAsk::RouteDetail { model, route }).await?
        }
        ClientMsg::Check {
            id,
            working_set,
            check,
        } => {
            net_ask(
                jobs,
                client,
                user,
                NetAsk::Check {
                    id,
                    working_set,
                    check,
                },
            )
            .await?
        }
        ClientMsg::Request { id, op, line } => {
            request(jobs, client, user, id, op, line, None).await?
        }
        // More than the applications send: dropped.
        ClientMsg::Presence(_) if !pace.presence.take() => None,
        ClientMsg::Lock { .. } if !pace.locks.take() => Some(ServerMsg::Notice {
            text: "too many lock requests: slow down".into(),
        }),
        ClientMsg::Presence(presence) => {
            let _ = jobs.send(Job::Presence {
                user: user.to_owned(),
                presence,
            });
            None
        }
        ClientMsg::Lock { target } => {
            let (tx, rx) = oneshot::channel();
            jobs.send(Job::Lock {
                user: user.to_owned(),
                target,
                reply: tx,
            })
            .map_err(|_| "server stopped")?;
            rx.await
                .map_err(|_| "server stopped")?
                .err()
                .map(|text| ServerMsg::Notice { text })
        }
        ClientMsg::Unlock { target } => {
            let _ = jobs.send(Job::Unlock {
                user: user.to_owned(),
                target,
            });
            None
        }
    })
}

/// A question about working sets to the authority (its answer comes on
/// the client's own queue); the refusal to tell the client, if refused.
async fn net_ask(
    jobs: &std_mpsc::Sender<Job>,
    client: u64,
    user: &str,
    ask: NetAsk,
) -> Result<Option<ServerMsg>, String> {
    let (tx, rx) = oneshot::channel();
    jobs.send(Job::Net {
        client,
        user: user.to_owned(),
        ask,
        reply: tx,
    })
    .map_err(|_| "server stopped")?;
    Ok(rx
        .await
        .map_err(|_| "server stopped")?
        .err()
        .map(|text| ServerMsg::Notice { text }))
}

/// An edit to the authority; the refusal to tell the client, if refused.
async fn request(
    jobs: &std_mpsc::Sender<Job>,
    client: u64,
    user: &str,
    id: u64,
    op: Op,
    line: String,
    model: Option<String>,
) -> Result<Option<ServerMsg>, String> {
    let (tx, rx) = oneshot::channel();
    jobs.send(Job::Request {
        client,
        user: user.to_owned(),
        id,
        op,
        line,
        model,
        reply: tx,
    })
    .map_err(|_| "server stopped")?;
    Ok(rx
        .await
        .map_err(|_| "server stopped")?
        .err()
        .map(|e| ServerMsg::Response { id, result: Err(e) }))
}

/// What of a feed's item goes to client `client` (its view `view`), and
/// the answer to its own edit right after it (QUIC and web alike).
pub(crate) fn deliver(
    view: &mut View,
    client: u64,
    item: Item,
) -> (Option<Deliver>, Option<ServerMsg>) {
    match item {
        Item::Own(own) => (view.own(own), None),
        Item::Event(ev) => {
            let text = ev.text_for(view.restricted).clone();
            let Event {
                answers, msg, to, ..
            } = ev;
            let answer = match (&*msg, answers) {
                (ServerMsg::Applied { op, .. }, Some((c, id))) if c == client => {
                    Some(ServerMsg::Response {
                        id,
                        result: Ok(op.describe()),
                    })
                }
                (ServerMsg::Change(change), Some((c, id))) if c == client => {
                    Some(ServerMsg::Response {
                        id,
                        result: Ok(change.op.describe()),
                    })
                }
                _ => None,
            };
            let deliver = match text {
                Ok(text) => view.broadcast(&to, &text, client),
                Err(e) => Some(Deliver::Own(Box::new(ServerMsg::Notice {
                    text: format!("a message the server could not write: {e}"),
                }))),
            };
            (deliver, answer)
        }
    }
}

/// Sends a file on a new one-way stream (1 MiB pieces).
async fn send_file(conn: &quinn::Connection, path: &Path) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let mut stream = conn.open_uni().await.map_err(|e| e.to_string())?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        stream
            .write_all(&buf[..n])
            .await
            .map_err(|e| e.to_string())?;
    }
    stream.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    /// L3/L4/M2: wrong access codes make an address wait, longer each
    /// time; joins a minute and connections not joined yet are limited per
    /// address; this computer is not limited.
    #[test]
    fn addresses_are_limited() {
        let gate = Arc::new(Gate::default());
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let other: IpAddr = "192.0.2.8".parse().unwrap();
        assert!(gate.may_join(ip).is_ok());
        gate.joined(ip, &Err(WRONG_CODE.into()));
        let e = gate.may_join(ip).unwrap_err();
        assert!(e.contains("wrong access codes"), "{e}");
        assert!(gate.may_join(other).is_ok(), "another address");
        gate.with(ip, |s| {
            assert_eq!(s.wrong, 1);
            s.wait_until = None;
        });
        gate.joined(ip, &Err(WRONG_CODE.into()));
        let wait = gate.with(ip, |s| s.wait_until.unwrap() - Instant::now());
        assert!(wait > Duration::from_millis(1500), "doubled: {wait:?}");
        gate.with(ip, |s| s.wait_until = None);
        gate.joined(ip, &Ok(()));
        gate.with(ip, |s| assert_eq!(s.wrong, 0));
        // Joins a minute.
        let busy: IpAddr = "192.0.2.9".parse().unwrap();
        for _ in 0..JOINS_PER_MINUTE as usize {
            assert!(gate.may_join(busy).is_ok());
        }
        assert!(gate.may_join(busy).unwrap_err().contains("a minute"));
        // Connections not joined yet.
        let held: Vec<Pending> = (0..MAX_PENDING).map(|_| gate.enter(ip).unwrap()).collect();
        assert!(gate.enter(ip).is_none());
        drop(held);
        assert!(gate.enter(ip).is_some());
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        // The computer's own connections are limited too (a reverse proxy),
        // by the host's parameter (owner, 2026-10-26).
        let few = Arc::new(Gate::new(2));
        let held: Vec<Pending> = (0..2).map(|_| few.enter(local).unwrap()).collect();
        assert!(few.enter(local).is_none());
        drop(held);
        for _ in 0..100 {
            gate.joined(local, &Err(WRONG_CODE.into()));
            assert!(gate.may_join(local).is_ok());
        }
        // Presence and locks paced per connection.
        let mut pace = Pace::new();
        let sent = (0..100).filter(|_| pace.presence.take()).count();
        assert!((20..25).contains(&sent), "{sent}");
        let locks = (0..200).filter(|_| pace.locks.take()).count();
        assert!((64..70).contains(&locks), "{locks}");
    }
}
