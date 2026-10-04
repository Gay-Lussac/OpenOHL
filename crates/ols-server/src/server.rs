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
    ALPN, ClientMsg, Invitation, Op, PROTOCOL, Peer, Presence, ServerMsg, execute, fingerprint,
    lock_target, random_code, read_msg, write_msg,
};

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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:4433".parse().expect("address"),
            advertise: None,
            code: None,
            newcomers: Some(Role::Viewer),
        }
    }
}

/// A server running (stopped when dropped).
pub struct Server {
    /// How to join it.
    pub invitation: Invitation,
    /// Where it listens.
    pub local_addr: SocketAddr,
    secret: String,
    jobs: std_mpsc::Sender<Job>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// One broadcast: a message for everyone, with the request it answers
/// (client, request id) when it is an edit.
#[derive(Debug, Clone)]
struct Event {
    answers: Option<(u64, u64)>,
    msg: ServerMsg,
}

struct Joined {
    events: broadcast::Receiver<Event>,
    copy: PathBuf,
    welcome: ServerMsg,
}

enum Job {
    Join {
        user: String,
        code: String,
        reply: oneshot::Sender<Result<Joined, String>>,
    },
    Request {
        client: u64,
        user: String,
        id: u64,
        op: Op,
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

    fn join(&mut self, user: &str, code: &str) -> Result<Joined, String> {
        if user.trim().is_empty() {
            return Err("no user name".into());
        }
        let personal = code == crate::access_code(&self.secret, Some(user));
        let general = code == crate::access_code(&self.secret, None);
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
                execute(&mut self.session, &admin, &op).map_err(|e| e.to_string())?;
                self.broadcast(ServerMsg::Applied {
                    user: admin,
                    op,
                    seq: None,
                });
                role
            }
        };
        // Subscribed before the copy: nothing applied after it is missed.
        let events = self.events.subscribe();
        let copy = self.copies.join(format!(
            "{}-{}.ols",
            user.replace(['/', '\\'], "_"),
            random_code(6)
        ));
        self.session.copy_to(&copy).map_err(|e| e.to_string())?;
        let size = std::fs::metadata(&copy).map_err(|e| e.to_string())?.len();
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

    fn request(&mut self, client: u64, user: &str, id: u64, op: Op) -> Result<(), String> {
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
                        let t = lock_target(kind, &oid);
                        if let Some(u) = self.locks.get(&t).filter(|u| u.as_str() != user) {
                            return Err(format!("{t} is being edited by {u}"));
                        }
                    }
                }
            }
        }
        let applied = execute(&mut self.session, user, &op).map_err(|e| e.to_string())?;
        let role_changed = matches!(&op, Op::SetUser { .. });
        let _ = self.events.send(Event {
            answers: Some((client, id)),
            msg: ServerMsg::Applied {
                user: user.to_owned(),
                op: op.clone(),
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
        match self.locks.get(&target) {
            Some(u) if u != user => Err(format!("{target} is being edited by {u}")),
            Some(_) => Ok(()),
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

    fn run(mut self, jobs: std_mpsc::Receiver<Job>) {
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Join { user, code, reply } => {
                    let _ = reply.send(self.join(&user, &code));
                }
                Job::Request {
                    client,
                    user,
                    id,
                    op,
                    reply,
                } => {
                    let _ = reply.send(self.request(client, &user, id, op));
                }
                Job::Presence { user, presence } => {
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
        let cert = rcgen::generate_simple_self_signed(vec!["ols-server".into()])
            .map_err(|e| e.to_string())?;
        let fp = fingerprint(cert.cert.der());
        let config = tls_server(&cert)?;
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
                    let _ = ready_tx.send(endpoint.local_addr().map_err(|e| e.to_string()));
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
                    endpoint.close(0u32.into(), b"server stopped");
                    endpoint.wait_idle().await;
                });
                let _ = stop_jobs.send(Job::Stop);
                let _ = authority_thread.join();
            })
            .map_err(|e| e.to_string())?;
        let local_addr = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "the server did not start".to_owned())??;
        let host = options.advertise.clone().unwrap_or_else(|| {
            if local_addr.ip().is_unspecified() {
                local_ip()
            } else {
                local_addr.ip().to_string()
            }
        });
        Ok(Server {
            invitation: Invitation {
                address: format!("{host}:{}", local_addr.port()),
                code,
                fingerprint: fp,
            },
            local_addr,
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

fn log(text: &str) {
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
        tokio::time::timeout(Duration::from_secs(10), read_msg(&mut recv))
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
    write_msg(send, &joined.welcome).await?;
    // The copy, on its own stream.
    let sent = send_file(conn, &joined.copy).await;
    let _ = tokio::fs::remove_file(&joined.copy).await;
    sent?;
    // One writer: broadcasts (with the answers to this client's requests
    // right after their edits) and this client's notices.
    let (out, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let mut events = joined.events;
    let forward = {
        let out = out.clone();
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
                        if out.send(msg).is_err() {
                            break;
                        }
                        if let Some(a) = answer
                            && out.send(a).is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let _ = out.send(ServerMsg::Notice {
                            text: "too far behind the server: join again".into(),
                        });
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
            match msg {
                ClientMsg::Hello { .. } => return Err("hello twice".into()),
                ClientMsg::Request { id, op } => {
                    let (tx, rx) = oneshot::channel();
                    jobs.send(Job::Request {
                        client,
                        user: user.to_owned(),
                        id,
                        op,
                        reply: tx,
                    })
                    .map_err(|_| "server stopped")?;
                    if let Err(e) = rx.await.map_err(|_| "server stopped")? {
                        let _ = out.send(ServerMsg::Response { id, result: Err(e) });
                    }
                }
                ClientMsg::Presence(presence) => {
                    let _ = jobs.send(Job::Presence {
                        user: user.to_owned(),
                        presence,
                    });
                }
                ClientMsg::Lock { target } => {
                    let (tx, rx) = oneshot::channel();
                    jobs.send(Job::Lock {
                        user: user.to_owned(),
                        target,
                        reply: tx,
                    })
                    .map_err(|_| "server stopped")?;
                    if let Err(e) = rx.await.map_err(|_| "server stopped")? {
                        let _ = out.send(ServerMsg::Notice { text: e });
                    }
                }
                ClientMsg::Unlock { target } => {
                    let _ = jobs.send(Job::Unlock {
                        user: user.to_owned(),
                        target,
                    });
                }
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
