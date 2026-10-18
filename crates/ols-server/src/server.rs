// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The server: one authority thread owns the session and applies every
//! edit in one order; a task per connection reads requests and writes
//! what the authority broadcasts.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use ols_session::{Role, Session};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    ALPN, ClientMsg, Invitation, MAX_HELLO, Op, PROTOCOL, Peer, Presence, ServerMsg, execute,
    fingerprint, lock_target, random_code, read_msg, read_msg_max, write_msg,
};

/// Messages waiting for a client before its forwarder stops taking
/// broadcasts.
const OUT_QUEUE: usize = 256;

/// Locks one user may hold.
const MAX_LOCKS: usize = 256;

/// How a server is started.
#[derive(Debug, Clone)]
pub struct Options {
    /// Address and port to listen on (port 0: any free one).
    pub bind: SocketAddr,
    /// Host name or address given in the invitation (default: this
    /// computer's address on the local network).
    pub advertise: Option<String>,
    /// The secret the access codes are made from (default: a random
    /// one; give one to keep the invitations across restarts).
    pub code: Option<String>,
    /// Role given to users the project does not know (`None`: they are
    /// refused).
    pub newcomers: Option<Role>,
    /// Also serve the web client (HTTPS and WebSocket over TCP) on this
    /// address and port ([`crate::web`]).
    pub web: Option<SocketAddr>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:4433".parse().expect("address"),
            advertise: None,
            code: None,
            newcomers: Some(Role::Viewer),
            web: None,
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
    secret: String,
    jobs: std_mpsc::Sender<Job>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// One broadcast: a message for everyone, with the request it answers
/// (client, request id) when it is an edit.
#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub(crate) answers: Option<(u64, u64)>,
    pub(crate) msg: ServerMsg,
}

pub(crate) struct Joined {
    pub(crate) events: broadcast::Receiver<Event>,
    /// The project file's copy (none for web clients).
    copy: Option<PathBuf>,
    pub(crate) welcome: ServerMsg,
}

/// What a web client's scene is made from: the line and the libraries
/// as they are after `revision` edits.
pub(crate) struct SceneSource {
    pub(crate) revision: u64,
    pub(crate) seq: i64,
    pub(crate) line: ols_line::Line,
    /// Kind → (file name, text), for the kinds a scene needs.
    pub(crate) libraries: BTreeMap<String, Vec<(String, String)>>,
}

pub(crate) enum Job {
    Join {
        user: String,
        code: String,
        /// Send a copy of the project file (QUIC clients).
        copy: bool,
        reply: oneshot::Sender<Result<Joined, String>>,
    },
    Request {
        client: u64,
        user: String,
        id: u64,
        op: Op,
        line: String,
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
    Stop,
}

/// The authority: the session, who is connected, the soft locks.
struct Authority {
    session: Session,
    secret: String,
    newcomers: Option<Role>,
    events: broadcast::Sender<Event>,
    peers: BTreeMap<String, Peer>,
    locks: BTreeMap<String, String>,
    copies: PathBuf,
    /// Edits of the line or its libraries since the server started.
    revision: u64,
}

impl Authority {
    fn broadcast(&self, msg: ServerMsg) {
        let _ = self.events.send(Event { answers: None, msg });
    }

    fn peers_msg(&self) -> ServerMsg {
        ServerMsg::Peers {
            peers: self.peers.values().cloned().collect(),
        }
    }

    fn join(&mut self, user: &str, code: &str, copy: bool) -> Result<Joined, String> {
        crate::check_name(user)?;
        let personal = crate::same_code(code, &crate::access_code(&self.secret, Some(user)));
        let general = crate::same_code(code, &crate::access_code(&self.secret, None));
        if !personal && !general {
            return Err("wrong access code".into());
        }
        if self.peers.contains_key(user) {
            return Err(format!("{user} is connected already"));
        }
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
                    .newcomers
                    .ok_or_else(|| format!("{user} is not a user of this project"))?;
                // Added by the project's admin, like every replica will.
                let admin = self
                    .session
                    .users()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|(_, r)| *r == Role::Admin)
                    .map(|(n, _)| n)
                    .ok_or("the project has no admin")?;
                let op = Op::SetUser {
                    name: user.to_owned(),
                    role,
                };
                let line = self.session.current_line().to_owned();
                execute(&mut self.session, &admin, &line, &op).map_err(|e| e.to_string())?;
                self.broadcast(ServerMsg::Applied {
                    user: admin,
                    op,
                    line,
                    seq: None,
                });
                role
            }
        };
        // Subscribed before the copy: nothing applied after it is missed.
        let events = self.events.subscribe();
        let (copy, size) = if copy {
            let copy = self.copies.join(format!(
                "{}-{}.ohl",
                user.replace(['/', '\\'], "_"),
                random_code(6)
            ));
            self.session.copy_to(&copy).map_err(|e| e.to_string())?;
            let size = std::fs::metadata(&copy).map_err(|e| e.to_string())?.len();
            (Some(copy), size)
        } else {
            (None, 0)
        };
        let seq = self.session.last_seq().map_err(|e| e.to_string())?;
        self.peers.insert(
            user.to_owned(),
            Peer {
                user: user.to_owned(),
                role,
                presence: Presence::default(),
            },
        );
        self.broadcast(self.peers_msg());
        Ok(Joined {
            events,
            copy,
            welcome: ServerMsg::Welcome {
                project: self.session.line().name.clone(),
                role,
                seq,
                size,
                peers: self.peers.values().cloned().collect(),
                locks: self.locks.clone(),
            },
        })
    }

    fn request(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        op: Op,
        line: String,
    ) -> Result<(), String> {
        // Soft locks: others' objects are refused.
        let held: Vec<(&String, &String)> = self
            .locks
            .iter()
            .filter(|(_, u)| u.as_str() != user)
            .collect();
        if !held.is_empty() {
            match op.targets() {
                None => {
                    let (t, u) = held[0];
                    return Err(format!("{t} is being edited by {u}"));
                }
                Some(targets) => {
                    for (kind, oid) in targets {
                        let t = lock_target(&line, kind, &oid);
                        if let Some(u) = self.locks.get(&t).filter(|u| u.as_str() != user) {
                            return Err(format!("{t} is being edited by {u}"));
                        }
                    }
                }
            }
        }
        let applied = execute(&mut self.session, user, &line, &op).map_err(|e| e.to_string())?;
        if crate::web::changes_scene(&op, &line) {
            self.revision += 1;
        }
        let role_changed = matches!(&op, Op::SetUser { .. });
        let _ = self.events.send(Event {
            answers: Some((client, id)),
            msg: ServerMsg::Applied {
                user: user.to_owned(),
                op: op.clone(),
                line,
                seq: applied.map(|a| a.seq),
            },
        });
        if let (true, Op::SetUser { name, role }) = (role_changed, &op)
            && let Some(p) = self.peers.get_mut(name)
        {
            p.role = *role;
            self.broadcast(self.peers_msg());
        }
        Ok(())
    }

    fn lock(&mut self, user: &str, target: String) -> Result<(), String> {
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

    fn unlock(&mut self, user: &str, target: &str) {
        if self.locks.get(target).is_some_and(|u| u == user) {
            self.locks.remove(target);
            self.broadcast(ServerMsg::Locks {
                locks: self.locks.clone(),
            });
        }
    }

    fn leave(&mut self, user: &str) {
        let before = self.locks.len();
        self.locks.retain(|_, u| u != user);
        if self.locks.len() != before {
            self.broadcast(ServerMsg::Locks {
                locks: self.locks.clone(),
            });
        }
        if self.peers.remove(user).is_some() {
            self.broadcast(self.peers_msg());
        }
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
                .line_named(ols_session::versions::MAIN)
                .unwrap_or_else(|_| self.session.line())
                .clone(),
            libraries,
        }))
    }

    fn run(mut self, jobs: std_mpsc::Receiver<Job>) {
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Join {
                    user,
                    code,
                    copy,
                    reply,
                } => {
                    let _ = reply.send(self.join(&user, &code, copy));
                }
                Job::Request {
                    client,
                    user,
                    id,
                    op,
                    line,
                    reply,
                } => {
                    let _ = reply.send(self.request(client, &user, id, op, line));
                }
                Job::Presence { user, mut presence } => {
                    if presence.selection.len() > crate::MAX_LABEL {
                        let mut end = crate::MAX_LABEL;
                        while !presence.selection.is_char_boundary(end) {
                            end -= 1;
                        }
                        presence.selection.truncate(end);
                    }
                    if let Some(p) = self.peers.get_mut(&user)
                        && p.presence != presence
                    {
                        p.presence = presence;
                        self.broadcast(self.peers_msg());
                    }
                }
                Job::Lock {
                    user,
                    target,
                    reply,
                } => {
                    let _ = reply.send(self.lock(&user, target));
                }
                Job::Unlock { user, target } => self.unlock(&user, &target),
                Job::Leave { user } => self.leave(&user),
                Job::Peers { reply } => {
                    let _ = reply.send(self.peers.values().cloned().collect());
                }
                Job::Scene { have, reply } => {
                    let _ = reply.send(self.scene(have));
                }
                Job::Stop => break,
            }
        }
        let _ = self.session.checkpoint();
        let _ = std::fs::remove_dir_all(&self.copies);
    }
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

impl Server {
    /// Hosts the project `path` (opened here: the server is then its only
    /// writer). Returns once it listens.
    pub fn start(path: &Path, options: Options) -> Result<Server, String> {
        let session = Session::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !session.has_line() {
            return Err(format!("{}: no line in the project", path.display()));
        }
        let host = options.advertise.clone().unwrap_or_else(|| {
            if options.bind.ip().is_unspecified() {
                local_ip()
            } else {
                options.bind.ip().to_string()
            }
        });
        // Names a browser may check (it still warns: self-signed).
        let mut names = vec!["ols-server".to_owned(), host.clone()];
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
        let code = crate::access_code(&secret, None);
        let copies = std::env::temp_dir().join(format!("ols-server-{}", random_code(8)));
        std::fs::create_dir_all(&copies).map_err(|e| e.to_string())?;
        let (events, _) = broadcast::channel(4096);
        let (jobs, job_rx) = std_mpsc::channel();
        let authority = Authority {
            session,
            secret: secret.clone(),
            newcomers: options.newcomers,
            events: events.clone(),
            peers: BTreeMap::new(),
            locks: BTreeMap::new(),
            copies,
            revision: 0,
        };
        let authority_thread = std::thread::Builder::new()
            .name("ols-authority".into())
            .spawn(move || authority.run(job_rx))
            .map_err(|e| e.to_string())?;
        let (stop, stop_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = std_mpsc::channel();
        let net_jobs = jobs.clone();
        let stop_jobs = jobs.clone();
        let bind = options.bind;
        let web_bind = options.web;
        let thread = std::thread::Builder::new()
            .name("ols-server".into())
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
                    let web_task = web.map(|(listener, acceptor)| {
                        tokio::spawn(crate::web::listen(listener, acceptor, net_jobs.clone()))
                    });
                    let mut next = 0u64;
                    let mut stop_rx = stop_rx;
                    loop {
                        tokio::select! {
                            _ = &mut stop_rx => break,
                            incoming = endpoint.accept() => {
                                let Some(incoming) = incoming else { break };
                                next += 1;
                                let jobs = net_jobs.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = connection(incoming, next, jobs).await {
                                        log(&format!("connection {next}: {e}"));
                                    }
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
            secret,
            jobs,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The secret the access codes are made from (never sent).
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// The personal invitation of `user` (a user of the project joins
    /// only with theirs).
    pub fn invitation_for(&self, user: &str) -> Invitation {
        Invitation {
            code: crate::access_code(&self.secret, Some(user)),
            ..self.invitation.clone()
        }
    }

    /// The web client's link (when it is served), with the general access
    /// code or `user`'s personal one in the fragment, which browsers do
    /// not send to the server: `https://HOST:PORT/#user=NAME&code=CODE`.
    pub fn web_link(&self, user: Option<&str>) -> Option<String> {
        let addr = self.web_addr?;
        let code = crate::access_code(&self.secret, user);
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
    eprintln!("ols-server: {text}");
}

/// One client: hello, the copy, then requests in and broadcasts out.
async fn connection(
    incoming: quinn::Incoming,
    client: u64,
    jobs: std_mpsc::Sender<Job>,
) -> Result<(), String> {
    let conn = incoming.await.map_err(|e| e.to_string())?;
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(10), conn.accept_bi())
        .await
        .map_err(|_| "no stream in 10 s".to_owned())?
        .map_err(|e| e.to_string())?;
    let hello: Option<ClientMsg> =
        tokio::time::timeout(Duration::from_secs(10), read_msg_max(&mut recv, MAX_HELLO))
            .await
            .map_err(|_| "no hello in 10 s".to_owned())??;
    let (user, code) = match hello {
        Some(ClientMsg::Hello {
            protocol,
            user,
            code,
        }) => {
            if protocol != PROTOCOL {
                let reason = format!("protocol {protocol}; this server speaks {PROTOCOL}");
                write_msg(&mut send, &ServerMsg::Refused { reason }).await?;
                let _ = send.finish();
                return Ok(());
            }
            (user, code)
        }
        _ => return Err("no hello".into()),
    };
    let (tx, rx) = oneshot::channel();
    jobs.send(Job::Join {
        user: user.clone(),
        code,
        copy: true,
        reply: tx,
    })
    .map_err(|_| "server stopped")?;
    let joined = match rx.await.map_err(|_| "server stopped")? {
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
    let _ = jobs.send(Job::Leave { user: user.clone() });
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
    // The copy, on its own stream; removed whatever happens.
    let copy = joined.copy.ok_or("no copy of the project")?;
    let sent = match write_msg(send, &joined.welcome).await {
        Ok(()) => send_file(conn, &copy).await,
        Err(e) => Err(e),
    };
    let _ = tokio::fs::remove_file(&copy).await;
    sent?;
    // One writer: broadcasts (with the answers to this client's requests
    // right after their edits) and this client's notices.
    // Bounded: a client that stops reading holds the forwarder, which then
    // lags behind the broadcasts and drops the client.
    let (out, mut out_rx) = mpsc::channel::<ServerMsg>(OUT_QUEUE);
    let mut events = joined.events;
    let forward = {
        let out = out.clone();
        let conn = conn.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event { answers, msg }) => {
                        let answer = match (&msg, answers) {
                            (ServerMsg::Applied { op, .. }, Some((c, id))) if c == client => {
                                Some(ServerMsg::Response {
                                    id,
                                    result: Ok(op.describe()),
                                })
                            }
                            _ => None,
                        };
                        if out.send(msg).await.is_err() {
                            break;
                        }
                        if let Some(a) = answer
                            && out.send(a).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let _ = out.try_send(ServerMsg::Notice {
                            text: "too far behind the server: join again".into(),
                        });
                        // A client that reads gets the notice first.
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        conn.close(0u32.into(), b"too far behind");
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };
    let reading = async {
        loop {
            let Some(msg) = read_msg::<ClientMsg>(recv).await? else {
                return Ok::<(), String>(());
            };
            if let Some(answer) = dispatch(msg, client, user, jobs).await? {
                let _ = out.send(answer).await;
            }
        }
    };
    let writing = async {
        while let Some(msg) = out_rx.recv().await {
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
) -> Result<Option<ServerMsg>, String> {
    Ok(match msg {
        ClientMsg::Hello { .. } => return Err("hello twice".into()),
        ClientMsg::Request { id, op, line } => {
            let (tx, rx) = oneshot::channel();
            jobs.send(Job::Request {
                client,
                user: user.to_owned(),
                id,
                op,
                line,
                reply: tx,
            })
            .map_err(|_| "server stopped")?;
            rx.await
                .map_err(|_| "server stopped")?
                .err()
                .map(|e| ServerMsg::Response { id, result: Err(e) })
        }
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
