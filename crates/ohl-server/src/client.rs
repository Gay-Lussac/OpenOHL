// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The client: joins a server with an invitation, receives the project
//! file, then exchanges messages through a [`Link`] (a thread runs the
//! connection; the application polls).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use ohl_session::{Applied, Role, Session};
use tokio::sync::mpsc;

use crate::{
    ALPN, ClientMsg, Invitation, Op, PROTOCOL, Peer, ServerMsg, execute, fingerprint, read_msg,
    write_msg,
};

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
    let (hs_tx, hs_rx) = std_mpsc::channel::<Handshake>();
    let (out, out_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (in_tx, inbox) = std_mpsc::channel::<Incoming>();
    let invitation = invitation.clone();
    let user_owned = user.to_owned();
    let file = file.to_owned();
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
            rt.block_on(run(invitation, user_owned, file, hs_tx, out_rx, in_tx));
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
    user: String,
    file: std::path::PathBuf,
    hs: std_mpsc::Sender<Handshake>,
    mut out_rx: mpsc::UnboundedReceiver<ClientMsg>,
    inbox: std_mpsc::Sender<Incoming>,
) {
    let connected = connect(&invitation, &user, &file, &hs).await;
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
    user: &str,
    file: &Path,
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
    write_msg(
        &mut send,
        &ClientMsg::Hello {
            protocol: PROTOCOL,
            user: user.to_owned(),
            code: invitation.code.clone(),
        },
    )
    .await?;
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
    receive_file(&conn, file, size, hs).await?;
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
