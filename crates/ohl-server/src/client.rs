// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The client: joins a server with an invitation, receives the project
//! file, then exchanges messages through a [`Link`] (a thread runs the
//! connection; the application polls).
//!
//! A network's server ([`crate::network`]) is joined with [`join_network`]:
//! with a model, its copy comes as a project file does and its changes are
//! replayed with [`replay_change`] (the replica takes the network's log
//! numbers); without one, nothing comes but what the client asks for
//! (tiles, routes).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use ohl_session::{Applied, Role, Session};
use tokio::sync::mpsc;

use crate::{
    ALPN, Change, ClientMsg, HeldTile, Invitation, Op, PROTOCOL, Peer, Position, RowsMsg,
    ServerMsg, execute, fingerprint, read_msg, write_msg,
};
use ohl_session::working_set::WorkingSetSpec;

/// What the connection gives the application.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A message of the server.
    Msg(ServerMsg),
    /// The connection ended, with why.
    Closed(String),
}

/// Joined: the replica's file, the server's welcome, the link.
pub struct Joined {
    /// Project (line) name.
    pub project: String,
    /// The user's role.
    pub role: Role,
    /// Log entry the copy is at.
    pub seq: i64,
    /// Users connected.
    pub peers: Vec<Peer>,
    /// Soft locks: target → user.
    pub locks: BTreeMap<String, String>,
    /// The messages since.
    pub link: Link,
}

/// The connection, from the application's side.
pub struct Link {
    out: mpsc::UnboundedSender<ClientMsg>,
    inbox: std_mpsc::Receiver<Incoming>,
    next: u64,
    /// The user.
    pub user: String,
    /// The server's address.
    pub address: String,
    /// The project line the user works on: requests act on it.
    pub line: String,
}

impl Link {
    /// Sends a request; returns its id (the [`ServerMsg::Response`] has it).
    pub fn request(&mut self, op: Op) -> Result<u64, String> {
        self.next += 1;
        self.send(ClientMsg::Request {
            id: self.next,
            op,
            line: self.line.clone(),
        })?;
        Ok(self.next)
    }

    /// Sends a request on model `model` of a network (phase 6: a partial
    /// replica's edits); returns its id.
    pub fn request_on(&mut self, model: &str, op: Op) -> Result<u64, String> {
        self.next += 1;
        self.send(ClientMsg::RequestOn {
            id: self.next,
            model: model.to_owned(),
            op,
            line: self.line.clone(),
        })?;
        Ok(self.next)
    }

    /// A new id for a question to the server (a pick shown, a route's
    /// structures).
    pub fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Sends a message.
    pub fn send(&self, msg: ClientMsg) -> Result<(), String> {
        self.out
            .send(msg)
            .map_err(|_| "not connected to the server".to_owned())
    }

    /// The next message, if one came.
    pub fn try_recv(&self) -> Option<Incoming> {
        match self.inbox.try_recv() {
            Ok(m) => Some(m),
            Err(std_mpsc::TryRecvError::Empty) => None,
            Err(std_mpsc::TryRecvError::Disconnected) => {
                Some(Incoming::Closed("connection closed".into()))
            }
        }
    }

    /// The next message, waiting at most `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Incoming> {
        match self.inbox.recv_timeout(timeout) {
            Ok(m) => Some(m),
            Err(std_mpsc::RecvTimeoutError::Timeout) => None,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                Some(Incoming::Closed("connection closed".into()))
            }
        }
    }
}

/// Applies a broadcast edit to the replica; its log entry must be the
/// server's (else the replica is out of sync: join again).
pub fn replay(
    session: &mut Session,
    user: &str,
    line: &str,
    op: &Op,
    seq: Option<i64>,
) -> Result<Option<Applied>, String> {
    let applied = execute(session, user, line, op).map_err(|e| format!("replica: {e}"))?;
    let mine = applied.as_ref().map(|a| a.seq);
    if mine != seq {
        return Err(format!(
            "replica out of sync (log entry {mine:?}, server {seq:?}): join again"
        ));
    }
    Ok(applied)
}

/// Applies a network's change to the replica of its model: its log
/// entries take the server's numbers ([`Session::set_next_seq`]), and the
/// replica's last entry must then be the server's (else it is out of
/// sync: join again).
pub fn replay_change(session: &mut Session, change: &Change) -> Result<Option<Applied>, String> {
    session.set_next_seq(change.first);
    let applied = execute(session, &change.user, &change.line, &change.op);
    session.set_next_seq(None);
    let applied = applied.map_err(|e| format!("replica: {e}"))?;
    let mine = applied.as_ref().map(|a| a.seq);
    if mine != change.seq {
        return Err(format!(
            "replica out of sync (log entry {mine:?}, server {:?}): join again",
            change.seq
        ));
    }
    if change.first.is_some() {
        let last = session.last_seq().map_err(|e| e.to_string())?;
        if last != change.at.seq {
            return Err(format!(
                "replica out of sync (last log entry {last}, server {}): join again",
                change.at.seq
            ));
        }
    }
    Ok(applied)
}

/// Takes rows of the server into a partial replica (phase 6): its objects
/// as the server holds them ([`Session::take_rows`], as `user`), its
/// working set when it changed, the stream's position it holds now
/// (`network.position`). Out of step (a row the replica cannot take):
/// join again.
pub fn take_rows(
    session: &mut Session,
    user: &str,
    msg: &RowsMsg,
) -> Result<Option<Applied>, String> {
    let applied = session
        .take_rows(user, msg.rows.clone())
        .map_err(|e| format!("replica out of step ({e}): join again"))?;
    if let Some(spec) = &msg.spec {
        session
            .set_working_set_spec(spec)
            .map_err(|e| e.to_string())?;
    }
    session
        .set_file_meta(
            "network.position",
            &serde_json::to_string(&msg.at).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    Ok(applied)
}

/// A check's result gone stale on the network (phase 10,
/// [`crate::ServerMsg::CheckStale`]): the working set's routes in the
/// replica marked out of date with the reason, as phase 8 marks a shared
/// tower's move (the replica's users mark them up to date when they have
/// checked again).
pub fn take_check_stale(
    session: &Session,
    user: &str,
    check: &str,
    reason: &str,
) -> Result<(), String> {
    let routes: Vec<String> = match session.working_set_spec().map_err(|e| e.to_string())? {
        Some(spec) => {
            let mut r: Vec<String> = spec
                .picks
                .iter()
                .filter_map(|p| match p {
                    ohl_session::working_set::Pick::Route { route }
                    | ohl_session::working_set::Pick::Stations { route, .. }
                    | ohl_session::working_set::Pick::Structures { route, .. } => {
                        Some(route.clone())
                    }
                    ohl_session::working_set::Pick::Area { .. } => None,
                })
                .collect();
            if r.is_empty() {
                r = session.line().routes.iter().map(|x| x.id.clone()).collect();
            }
            r
        }
        None => session.line().routes.iter().map(|x| x.id.clone()).collect(),
    };
    session
        .mark_stale(user, &routes, &format!("{check}: {reason}"))
        .map_err(|e| e.to_string())
}

/// What a partial replica joining again says it holds (phase 6).
pub fn hold(session: &Session) -> ClientMsg {
    ClientMsg::Hold {
        objects: session
            .held()
            .into_iter()
            .map(|((k, id), h)| (k, id, h))
            .collect(),
    }
}

/// The stream's position a partial replica holds (`network.position`).
pub fn replica_position(session: &Session) -> Option<Position> {
    session
        .file_meta("network.position")
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// How to join a network's server.
#[derive(Debug, Clone, Default)]
pub struct NetJoin {
    /// The model to follow whole: its copy is written to `file`.
    pub model: Option<String>,
    /// Where the model's copy goes (needed with a model).
    pub file: Option<std::path::PathBuf>,
    /// The changes after this position first (after a disconnect).
    pub since: Option<Position>,
    /// The routes of the working set.
    pub routes: Vec<String>,
    /// The tiles viewed.
    pub tiles: Vec<HeldTile>,
    /// The model's copy is held already (a replica catching up from
    /// `since`): none is sent.
    pub held: bool,
    /// Phase 6: the working set to follow as a partial replica (in place
    /// of `model`): its replica is written to `file` (unless `held`: then
    /// send [`hold`] of the replica and take the rows that come).
    pub working_set: Option<WorkingSetSpec>,
    /// Phase 7: an admin sees as this user (what both may see, read only).
    pub see_as: Option<String>,
    /// Phase 13: an ID token of the network's identity provider in place
    /// of the invitation's access code (OIDC sign-in).
    pub id_token: Option<String>,
}

/// Joins the network's server of `invitation` as `user` (protocol 3's
/// join); blocks until joined (as [`join`]). With a model, its copy is
/// written to `how.file` first.
pub fn join_network(
    invitation: &Invitation,
    user: &str,
    how: &NetJoin,
    progress: &mut dyn FnMut(Progress),
) -> Result<Joined, String> {
    if (how.model.is_some() || how.working_set.is_some()) && !how.held && how.file.is_none() {
        return Err("a file for the model's copy".into());
    }
    let hello = ClientMsg::Join {
        protocol: PROTOCOL,
        user: user.to_owned(),
        // Signed in with a token: no access code beside it.
        code: if how.id_token.is_some() {
            String::new()
        } else {
            invitation.code.clone()
        },
        model: how.model.clone(),
        since: how.since,
        routes: how.routes.clone(),
        tiles: how.tiles.clone(),
        held: how.held,
        working_set: how.working_set.clone(),
        see_as: how.see_as.clone(),
        id_token: how.id_token.clone(),
    };
    join_with(invitation, user, hello, how.file.clone(), progress)
}

/// Trusts the server whose certificate has the invitation's fingerprint.
#[derive(Debug)]
struct Pinned {
    fingerprint: [u8; 32],
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity) == self.fingerprint {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "the server is not the one of the invitation (certificate fingerprint)".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// TLS 1.3 trusting only the certificate with fingerprint `fp`.
pub(crate) fn pinned_tls(fp: [u8; 32], alpn: &[u8]) -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Pinned {
        fingerprint: fp,
        algorithms: provider.signature_verification_algorithms,
    };
    let mut tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    tls.alpn_protocols = vec![alpn.to_vec()];
    Ok(tls)
}

fn tls_client(fp: [u8; 32]) -> Result<quinn::ClientConfig, String> {
    let tls = pinned_tls(fp, ALPN)?;
    let crypto =
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(|e| e.to_string())?;
    let mut config = quinn::ClientConfig::new(Arc::new(crypto));
    config.transport_config(Arc::new(crate::server::transport()));
    Ok(config)
}

/// Progress of a join: bytes of the project file received, of `total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Received.
    pub received: u64,
    /// Total.
    pub total: u64,
}

enum Handshake {
    Progress(Progress),
    Done(Result<Box<Joined>, String>),
}

/// Joins the server of `invitation` as `user`; the project file is written
/// to `file` (replaced). Blocks until joined (or refused, or no answer in
/// 20 s, or the transfer stops for 60 s), calling `progress` meanwhile.
pub fn join(
    invitation: &Invitation,
    user: &str,
    file: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<Joined, String> {
    let hello = ClientMsg::Hello {
        protocol: PROTOCOL,
        user: user.to_owned(),
        code: invitation.code.clone(),
    };
    join_with(invitation, user, hello, Some(file.to_owned()), progress)
}

pub(crate) fn join_with(
    invitation: &Invitation,
    user: &str,
    hello: ClientMsg,
    file: Option<std::path::PathBuf>,
    progress: &mut dyn FnMut(Progress),
) -> Result<Joined, String> {
    let (hs_tx, hs_rx) = std_mpsc::channel::<Handshake>();
    let (out, out_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (in_tx, inbox) = std_mpsc::channel::<Incoming>();
    let invitation = invitation.clone();
    let address = invitation.address.clone();
    std::thread::Builder::new()
        .name("ohl-client".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = hs_tx.send(Handshake::Done(Err(e.to_string())));
                    return;
                }
            };
            rt.block_on(run(invitation, hello, file, hs_tx, out_rx, in_tx));
        })
        .map_err(|e| e.to_string())?;
    loop {
        match hs_rx.recv() {
            Ok(Handshake::Progress(p)) => progress(p),
            Ok(Handshake::Done(Ok(mut j))) => {
                j.link = Link {
                    out,
                    inbox,
                    next: 0,
                    user: user.to_owned(),
                    address,
                    line: ohl_session::versions::MAIN.to_owned(),
                };
                return Ok(*j);
            }
            Ok(Handshake::Done(Err(e))) => return Err(e),
            Err(_) => return Err("the connection thread stopped".into()),
        }
    }
}

async fn run(
    invitation: Invitation,
    hello: ClientMsg,
    file: Option<std::path::PathBuf>,
    hs: std_mpsc::Sender<Handshake>,
    mut out_rx: mpsc::UnboundedReceiver<ClientMsg>,
    inbox: std_mpsc::Sender<Incoming>,
) {
    let connected = connect(&invitation, &hello, file.as_deref(), &hs).await;
    let (endpoint, conn, mut send, mut recv, joined) = match connected {
        Ok(c) => c,
        Err(e) => {
            let _ = hs.send(Handshake::Done(Err(e)));
            return;
        }
    };
    let _ = hs.send(Handshake::Done(Ok(Box::new(joined))));
    let reading = async {
        loop {
            match read_msg::<ServerMsg>(&mut recv).await {
                Ok(Some(m)) => {
                    if inbox.send(Incoming::Msg(m)).is_err() {
                        return "closed".to_owned();
                    }
                }
                Ok(None) => return "the server ended the connection".to_owned(),
                Err(e) => return e,
            }
        }
    };
    let writing = async {
        while let Some(m) = out_rx.recv().await {
            if let Err(e) = write_msg(&mut send, &m).await {
                return e;
            }
        }
        // The application let go of the link.
        "left".to_owned()
    };
    let why = tokio::select! {
        w = reading => w,
        w = writing => w,
        e = conn.closed() => e.to_string(),
    };
    conn.close(0u32.into(), b"bye");
    // Let the close reach the server (at most 2 s).
    let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
    let _ = inbox.send(Incoming::Closed(why));
}

type Connected = (
    quinn::Endpoint,
    quinn::Connection,
    quinn::SendStream,
    quinn::RecvStream,
    Joined,
);

async fn connect(
    invitation: &Invitation,
    hello: &ClientMsg,
    file: Option<&Path>,
    hs: &std_mpsc::Sender<Handshake>,
) -> Result<Connected, String> {
    let addr = tokio::net::lookup_host(&invitation.address)
        .await
        .map_err(|e| format!("{}: {e}", invitation.address))?
        .next()
        .ok_or_else(|| format!("{}: no address", invitation.address))?;
    let bind: std::net::SocketAddr = if addr.is_ipv6() {
        "[::]:0".parse().expect("address")
    } else {
        "0.0.0.0:0".parse().expect("address")
    };
    let mut endpoint = quinn::Endpoint::client(bind).map_err(|e| e.to_string())?;
    endpoint.set_default_client_config(tls_client(invitation.fingerprint)?);
    let conn = tokio::time::timeout(
        Duration::from_secs(20),
        endpoint
            .connect(addr, "ohl-server")
            .map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|_| format!("{}: no answer in 20 s", invitation.address))?
    .map_err(|e| format!("{}: {e}", invitation.address))?;
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    write_msg(&mut send, hello).await?;
    let welcome = tokio::time::timeout(Duration::from_secs(60), read_msg::<ServerMsg>(&mut recv))
        .await
        .map_err(|_| "no welcome in 60 s".to_owned())??;
    let (project, role, seq, size, peers, locks) = match welcome {
        Some(ServerMsg::Welcome {
            project,
            role,
            seq,
            size,
            peers,
            locks,
        }) => (project, role, seq, size, peers, locks),
        Some(ServerMsg::Refused { reason }) => return Err(format!("refused: {reason}")),
        Some(other) => return Err(format!("unexpected {other:?}")),
        None => return Err("the server closed the connection".into()),
    };
    // A network's client following no model gets no file.
    match file {
        Some(file) if size > 0 || matches!(hello, ClientMsg::Hello { .. }) => {
            receive_file(&conn, file, size, hs).await?;
        }
        _ => {}
    }
    // The link is filled in by `join`.
    let (dummy_out, _) = mpsc::unbounded_channel();
    let (_, dummy_in) = std_mpsc::channel();
    Ok((
        endpoint,
        conn,
        send,
        recv,
        Joined {
            project,
            role,
            seq,
            peers,
            locks,
            link: Link {
                out: dummy_out,
                inbox: dummy_in,
                next: 0,
                user: String::new(),
                address: String::new(),
                line: String::new(),
            },
        },
    ))
}

async fn receive_file(
    conn: &quinn::Connection,
    file: &Path,
    size: u64,
    hs: &std_mpsc::Sender<Handshake>,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut stream = tokio::time::timeout(Duration::from_secs(60), conn.accept_uni())
        .await
        .map_err(|_| "no project file in 60 s".to_owned())?
        .map_err(|e| e.to_string())?;
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    // The replica's own files (an older copy) go first.
    let ext = file
        .extension()
        .map_or(String::new(), |e| e.to_string_lossy().into_owned());
    for log in ["wal", "shm"] {
        let _ = tokio::fs::remove_file(file.with_extension(format!("{ext}-{log}"))).await;
    }
    let mut out = tokio::fs::File::create(file)
        .await
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let mut received = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = tokio::time::timeout(Duration::from_secs(60), stream.read(&mut buf))
            .await
            .map_err(|_| "the transfer stopped for 60 s".to_owned())?
            .map_err(|e| e.to_string())?;
        let Some(n) = n else { break };
        out.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
        received += n as u64;
        let _ = hs.send(Handshake::Progress(Progress {
            received,
            total: size,
        }));
    }
    out.flush().await.map_err(|e| e.to_string())?;
    if received != size {
        return Err(format!("project file: {received} of {size} bytes"));
    }
    Ok(())
}
