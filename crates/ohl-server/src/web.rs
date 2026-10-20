// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The web listener: browsers join a session without installing anything.
//!
//! One TCP port serves, over HTTPS with the server's self-signed
//! certificate (the browser warns once; the fingerprint is the
//! invitation's), the web client's files embedded in this program
//! ([`FILES`], nothing else) and a WebSocket at `/ws`. The WebSocket speaks
//! the QUIC protocol's messages, one JSON text message each: the same
//! hello, access codes, roles, authority and limits ([`crate::MAX_HELLO`]
//! before the code is checked, [`crate::MAX_MESSAGE`] after). A web client
//! keeps no replica: after the welcome (`size` 0, no file) it receives a
//! [`Scene`] — the line with its wires solved on the server — and a new one
//! whenever an edit changed the line or its libraries, besides the usual
//! broadcasts.
//!
//! Cost: a scene is one solve of the whole line in [`SCENE_CASE`] with the
//! default exit clauses of the finite-element solve (per section), made
//! once per change for every web client (cached by revision; one at a
//! time), at most every [`SCENE_DELAY`] while edits keep coming.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, broadcast, oneshot};
use tokio::time::Instant;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::{Role as WsRole, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

use crate::server::{Event, Job, SceneSource, dispatch, log};
use crate::{
    ClientMsg, MAX_HELLO, MAX_MESSAGE, Op, PROTOCOL, Scene, SceneSpan, SceneWire, ServerMsg,
};

/// The web client's files: path, media type, content.
pub const FILES: &[(&str, &str, &str)] = &[
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_str!("../web/index.html"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_str!("../web/style.css"),
    ),
    (
        "/app.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/app.js"),
    ),
    (
        "/gpu.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/gpu.js"),
    ),
];

/// Library kinds a scene is made with.
pub const SCENE_LIBRARIES: [&str; 3] = ["cables", "structures", "insulators"];

/// Weather case of the web client's wires.
pub const SCENE_CASE: &str = "15 °C, no wind";

/// After an edit, how long a web client waits for more before it asks
/// for a new scene.
pub const SCENE_DELAY: Duration = Duration::from_millis(200);

/// Web connections (pages and WebSockets) at once.
const MAX_CONNECTIONS: usize = 64;

/// Largest HTTP request head, bytes.
const MAX_HEAD: usize = 8 << 10;

/// Pings every so often; a client silent for [`SILENT`] is gone.
const PING: Duration = Duration::from_secs(10);
const SILENT: Duration = Duration::from_secs(30);

/// Web client ids start here (QUIC ones count from 1).
const WEB_CLIENTS: u64 = 1 << 62;

/// Whether an edit on project line `line` can change what a scene shows
/// (the main line, the libraries it is solved with).
pub fn changes_scene(op: &Op, line: &str) -> bool {
    let main = line == ohl_session::versions::MAIN;
    match op {
        Op::Do { .. } | Op::Undo | Op::Redo | Op::Rollback { .. } | Op::RollbackAfter { .. } => {
            main
        }
        Op::PutLibrary { kind, .. } | Op::DeleteLibrary { kind, .. } => {
            SCENE_LIBRARIES.contains(&kind.as_str())
        }
        // Version control: these replace the working state (line and
        // libraries).
        Op::BringIn { .. } | Op::Merge { .. } | Op::Revert { .. } => main,
        Op::RollBackLine { line, .. } => line == ohl_session::versions::MAIN,
        // Into its target, whichever line it was asked from; lines
        // deleted or renamed (the main one among them).
        Op::MergeReview { .. } | Op::DeleteLine { .. } | Op::RenameLine { .. } => true,
        Op::SetUser { .. }
        | Op::CreateLine { .. }
        | Op::SetLineDescription { .. }
        | Op::SetSetting { .. }
        | Op::Commit { .. }
        | Op::CreateBranch { .. }
        | Op::Tag { .. }
        | Op::ProtectBranch { .. }
        | Op::DeleteRef { .. }
        | Op::SubmitReview { .. }
        | Op::DecideReview { .. }
        | Op::Grant { .. }
        | Op::Revoke { .. } => false,
    }
}

/// Percent-escapes a value for a URL fragment (letters, digits and
/// `-._~` kept).
pub fn fragment_escape(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// TLS for the web listener, with the server's certificate.
pub(crate) fn acceptor(
    cert: &rcgen::CertifiedKey<rcgen::KeyPair>,
) -> Result<tokio_rustls::TlsAcceptor, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()));
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert.cert.der().to_vec())], key)
        .map_err(|e| e.to_string())?;
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(tls)))
}

/// Accepts web connections until the task is aborted.
pub(crate) async fn listen(
    listener: TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    jobs: std_mpsc::Sender<Job>,
) {
    let scenes = Arc::new(Scenes::new());
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut next = 0u64;
    loop {
        let tcp = match listener.accept().await {
            Ok((tcp, _)) => tcp,
            Err(_) => {
                // Out of file descriptors, or the like: try again later.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            continue;
        };
        next += 1;
        let client = WEB_CLIENTS + next;
        let (acceptor, jobs, scenes) = (acceptor.clone(), jobs.clone(), scenes.clone());
        tokio::spawn(async move {
            if let Err(e) = connection(tcp, acceptor, client, jobs, scenes).await {
                log(&format!("web connection {next}: {e}"));
            }
            drop(slot);
        });
    }
}

/// An HTTP request's head.
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
}

impl Request {
    fn parse(head: &[u8]) -> Option<Request> {
        let text = std::str::from_utf8(head).ok()?;
        let mut lines = text.split("\r\n");
        let mut first = lines.next()?.split(' ');
        let (method, target, version) = (first.next()?, first.next()?, first.next()?);
        if !version.starts_with("HTTP/1.") || first.next().is_some() {
            return None;
        }
        let path = target.split(['?', '#']).next()?.to_owned();
        let mut headers = BTreeMap::new();
        for line in lines.filter(|l| !l.is_empty()) {
            let (name, value) = line.split_once(':')?;
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
        Some(Request {
            method: method.to_owned(),
            path,
            headers,
        })
    }

    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map(String::as_str).unwrap_or("")
    }

    fn is_websocket(&self) -> bool {
        self.header("upgrade").eq_ignore_ascii_case("websocket")
            && self
                .header("connection")
                .split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
            && self.header("sec-websocket-version") == "13"
            && !self.header("sec-websocket-key").is_empty()
    }

    /// A page of another site may not open the WebSocket (it would still
    /// need an access code; this is defence in depth).
    fn same_origin(&self) -> bool {
        match self.headers.get("origin") {
            None => true,
            Some(o) => o.strip_prefix("https://") == Some(self.header("host")),
        }
    }

    /// The host, if it is a plain host name or address with a port.
    fn plain_host(&self) -> Option<&str> {
        let h = self.header("host");
        (!h.is_empty()
            && h.len() <= 255
            && h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b)))
        .then_some(h)
    }
}

/// Reads up to the end of the request head; returns it and what came
/// after it.
async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut buf = Vec::with_capacity(1024);
    let mut piece = [0u8; 1024];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            buf.truncate(end);
            return Ok((buf, rest));
        }
        if buf.len() > MAX_HEAD {
            return Err("request head too large".into());
        }
        let n = stream.read(&mut piece).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed in the request".into());
        }
        buf.extend_from_slice(&piece[..n]);
    }
}

async fn respond<S: AsyncWrite + Unpin>(
    stream: &mut S,
    status: &str,
    headers: &[(&str, String)],
    body: &[u8],
    head_only: bool,
) -> Result<(), String> {
    let mut text = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\n",
        body.len()
    );
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    stream
        .write_all(text.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    if !head_only {
        stream.write_all(body).await.map_err(|e| e.to_string())?;
    }
    stream.flush().await.map_err(|e| e.to_string())?;
    let _ = stream.shutdown().await;
    Ok(())
}

/// One web connection: a file, or the WebSocket.
async fn connection(
    tcp: TcpStream,
    acceptor: tokio_rustls::TlsAcceptor,
    client: u64,
    jobs: std_mpsc::Sender<Job>,
    scenes: Arc<Scenes>,
) -> Result<(), String> {
    let _ = tcp.set_nodelay(true);
    // A browser that does not trust the certificate yet hangs up here:
    // not worth a line in the log.
    let Ok(Ok(mut tls)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await
    else {
        return Ok(());
    };
    let (head, rest) = tokio::time::timeout(Duration::from_secs(10), read_head(&mut tls))
        .await
        .map_err(|_| "no request in 10 s".to_owned())??;
    let Some(req) = Request::parse(&head) else {
        return respond(&mut tls, "400 Bad Request", &[], b"bad request", false).await;
    };
    let head_only = req.method == "HEAD";
    if req.method != "GET" && !head_only {
        let allow = [("Allow", "GET, HEAD".to_owned())];
        return respond(&mut tls, "405 Method Not Allowed", &allow, b"", head_only).await;
    }
    if req.path == "/ws" {
        if !req.is_websocket() {
            return respond(&mut tls, "400 Bad Request", &[], b"a WebSocket", false).await;
        }
        if !req.same_origin() {
            return respond(&mut tls, "403 Forbidden", &[], b"another origin", false).await;
        }
        let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(
            req.header("sec-websocket-key").as_bytes(),
        );
        let answer = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        tls.write_all(answer.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        return websocket(tls, rest, client, jobs, scenes).await;
    }
    let path = if req.path == "/" {
        "/index.html"
    } else {
        req.path.as_str()
    };
    match FILES.iter().find(|(p, _, _)| *p == path) {
        Some((_, media, body)) => {
            let connect = match req.plain_host() {
                Some(h) => format!("'self' wss://{h}"),
                None => "'self'".into(),
            };
            let headers = [
                ("Content-Type", (*media).to_owned()),
                (
                    "Content-Security-Policy",
                    format!(
                        "default-src 'self'; connect-src {connect}; img-src 'self' data:; \
                         frame-ancestors 'none'; base-uri 'none'; form-action 'none'"
                    ),
                ),
            ];
            respond(&mut tls, "200 OK", &headers, body.as_bytes(), head_only).await
        }
        None => respond(&mut tls, "404 Not Found", &[], b"not found", head_only).await,
    }
}

fn ws_config(max: usize) -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(max))
        .max_frame_size(Some(max))
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
    msg: &ServerMsg,
) -> Result<(), String> {
    let text = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    if text.len() > MAX_MESSAGE {
        return Err(format!("message of {} bytes: too large", text.len()));
    }
    ws.send(Message::text(text))
        .await
        .map_err(|e| e.to_string())
}

async fn refuse<S: AsyncRead + AsyncWrite + Unpin>(ws: &mut WebSocketStream<S>, reason: String) {
    let _ = send(ws, &ServerMsg::Refused { reason }).await;
    let _ = ws.close(None).await;
}

/// A WebSocket: the hello, then messages both ways.
async fn websocket<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    rest: Vec<u8>,
    client: u64,
    jobs: std_mpsc::Sender<Job>,
    scenes: Arc<Scenes>,
) -> Result<(), String> {
    // Before the access code is checked, only a small message.
    let mut ws = WebSocketStream::from_partially_read(
        stream,
        rest,
        WsRole::Server,
        Some(ws_config(MAX_HELLO)),
    )
    .await;
    let first = tokio::time::timeout(Duration::from_secs(10), ws.next())
        .await
        .map_err(|_| "no hello in 10 s".to_owned())?;
    let hello: ClientMsg = match first {
        Some(Ok(Message::Text(t))) => {
            serde_json::from_str(t.as_str()).map_err(|e| format!("bad message: {e}"))?
        }
        Some(Ok(_)) => return Err("no hello".into()),
        Some(Err(e)) => return Err(e.to_string()),
        None => return Ok(()),
    };
    let ClientMsg::Hello {
        protocol,
        user,
        code,
    } = hello
    else {
        return Err("no hello".into());
    };
    if protocol != PROTOCOL {
        let reason = format!("protocol {protocol}; this server speaks {PROTOCOL}");
        refuse(&mut ws, reason).await;
        return Ok(());
    }
    let (tx, rx) = oneshot::channel();
    jobs.send(Job::Join {
        user: user.clone(),
        code,
        copy: false,
        reply: tx,
    })
    .map_err(|_| "server stopped")?;
    let joined = match rx.await.map_err(|_| "server stopped")? {
        Ok(j) => j,
        Err(reason) => {
            refuse(&mut ws, reason).await;
            return Ok(());
        }
    };
    log(&format!("{user} joined (web)"));
    // The client sends nothing more before the welcome: no frame is lost.
    let ws = WebSocketStream::from_raw_socket(
        ws.into_inner(),
        WsRole::Server,
        Some(ws_config(MAX_MESSAGE)),
    )
    .await;
    let result = joined_session(ws, client, &user, joined, &jobs, &scenes).await;
    let _ = jobs.send(Job::Leave { user: user.clone() });
    log(&format!("{user} left (web)"));
    result
}

async fn joined_session<S: AsyncRead + AsyncWrite + Unpin>(
    mut ws: WebSocketStream<S>,
    client: u64,
    user: &str,
    joined: crate::server::Joined,
    jobs: &std_mpsc::Sender<Job>,
    scenes: &Scenes,
) -> Result<(), String> {
    send(&mut ws, &joined.welcome).await?;
    let mut events = joined.events;
    let mut sent: Option<u64> = None;
    // When to send a new scene (now: the first one).
    let mut refresh = Some(Instant::now());
    let mut ping = tokio::time::interval(PING);
    let mut heard = Instant::now();
    loop {
        let due = refresh.unwrap_or_else(|| Instant::now() + PING);
        tokio::select! {
            _ = tokio::time::sleep_until(due), if refresh.is_some() => {
                refresh = None;
                let (revision, text) = scenes.current(jobs).await?;
                if sent != Some(revision) {
                    ws.send(Message::Text(text)).await.map_err(|e| e.to_string())?;
                    sent = Some(revision);
                }
            }
            event = events.recv() => match event {
                Ok(Event { answers, msg }) => {
                    let answer = match (&msg, answers) {
                        (ServerMsg::Applied { op, .. }, Some((c, id))) if c == client => {
                            Some(ServerMsg::Response { id, result: Ok(op.describe()) })
                        }
                        _ => None,
                    };
                    if let ServerMsg::Applied { op, line, .. } = &msg
                        && changes_scene(op, line)
                        && refresh.is_none()
                    {
                        refresh = Some(Instant::now() + SCENE_DELAY);
                    }
                    send(&mut ws, &msg).await?;
                    if let Some(a) = answer {
                        send(&mut ws, &a).await?;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let text = "too far behind the server: join again".to_owned();
                    let _ = send(&mut ws, &ServerMsg::Notice { text }).await;
                    let _ = ws.close(None).await;
                    return Err("client too far behind".into());
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
            incoming = ws.next() => {
                heard = Instant::now();
                match incoming {
                    None | Some(Ok(Message::Close(_))) => return Ok(()),
                    Some(Err(e)) => return Err(e.to_string()),
                    Some(Ok(Message::Text(t))) => {
                        let msg: ClientMsg = serde_json::from_str(t.as_str())
                            .map_err(|e| format!("bad message: {e}"))?;
                        // The web client only views: no edits, no locks.
                        let answer = match msg {
                            ClientMsg::Request { id, .. } => Some(ServerMsg::Response {
                                id,
                                result: Err(VIEW_ONLY.into()),
                            }),
                            ClientMsg::Lock { .. } => Some(ServerMsg::Notice {
                                text: VIEW_ONLY.into(),
                            }),
                            msg => dispatch(msg, client, user, jobs).await?,
                        };
                        if let Some(answer) = answer {
                            send(&mut ws, &answer).await?;
                        }
                    }
                    Some(Ok(Message::Binary(_))) => return Err("binary message".into()),
                    Some(Ok(_)) => {}
                }
            }
            _ = ping.tick() => {
                if heard.elapsed() > SILENT {
                    return Err(format!("silent for {} s", SILENT.as_secs()));
                }
                ws.send(Message::Ping(Default::default()))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
}

/// The last scene made (shared by the web clients), as its message's
/// text.
struct Scenes {
    last: tokio::sync::Mutex<Option<(u64, Utf8Bytes)>>,
    /// Where project library files are written to be read.
    work: PathBuf,
}

impl Scenes {
    fn new() -> Self {
        Self {
            last: tokio::sync::Mutex::new(None),
            work: std::env::temp_dir().join(format!("ohl-web-{}", crate::random_code(8))),
        }
    }

    /// The current scene: the one kept, or a new one when the line or its
    /// libraries changed since (one is made at a time).
    async fn current(&self, jobs: &std_mpsc::Sender<Job>) -> Result<(u64, Utf8Bytes), String> {
        let mut last = self.last.lock().await;
        let (tx, rx) = oneshot::channel();
        jobs.send(Job::Scene {
            have: last.as_ref().map(|(r, _)| *r),
            reply: tx,
        })
        .map_err(|_| "server stopped")?;
        let Some(source) = rx.await.map_err(|_| "server stopped")?? else {
            return last.clone().ok_or_else(|| "no scene".to_owned());
        };
        let revision = source.revision;
        let work = self.work.clone();
        let text = tokio::task::spawn_blocking(move || {
            serde_json::to_string(&ServerMsg::Scene(Box::new(scene(source, &work))))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        if text.len() > MAX_MESSAGE {
            return Err(format!("scene of {} bytes: too large", text.len()));
        }
        let text = Utf8Bytes::from(text);
        *last = Some((revision, text.clone()));
        Ok((revision, text))
    }
}

impl Drop for Scenes {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

/// Why a web client's edit is refused.
pub const VIEW_ONLY: &str = "the web client only views the project (a viewer): edit in OHL-Design";

/// Points of a span sent to web clients at most (its ends kept): a sag
/// curve drawn smooth, a light message for long lines.
pub const SCENE_SPAN_POINTS: usize = 24;

/// About `n` points of `points`, evenly, its ends kept.
fn thin(points: &[[f64; 3]], n: usize) -> Vec<[f64; 3]> {
    if points.len() <= n || n < 2 {
        return points.to_vec();
    }
    let step = (points.len() - 1) as f64 / (n - 1) as f64;
    (0..n)
        .map(|k| points[((k as f64 * step).round() as usize).min(points.len() - 1)])
        .collect()
}

fn mm(p: &[f64; 3]) -> [f64; 3] {
    p.map(|x| (x * 1000.0).round() / 1000.0)
}

/// The project's files of a library kind, written to a folder of `work`
/// to be loaded (`None`: the project has none).
fn library_dir(
    files: Option<&Vec<(String, String)>>,
    work: &Path,
    kind: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(files) = files.filter(|f| !f.is_empty()) else {
        return Ok(None);
    };
    let dir = work.join(format!("{kind}-{}", crate::random_code(8)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, text) in files {
        if !ohl_session::safe_file_name(name) {
            return Err(format!("library file \"{name}\": not a plain file name"));
        }
        std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(Some(dir))
}

/// Makes a scene: the line resolved with the project's libraries (else
/// the shipped ones) and solved in [`SCENE_CASE`]. What fails is noted.
pub(crate) fn scene(source: SceneSource, work: &Path) -> Scene {
    use ohl_core::files::shipped_library;
    let mut notes = Vec::new();
    let project = |kind: &str| source.libraries.get(kind);
    // Wires: the project's library files, else the shipped library.
    let mut cables = ohl_cables::Library::default();
    match project("cables").filter(|f| !f.is_empty()) {
        Some(files) => {
            for (name, text) in files {
                if let Err(e) = cables.add_toml(Path::new(name), text) {
                    notes.push(format!("wire library {name}: {e}"));
                }
            }
        }
        None => match ohl_cables::Library::load(&shipped_library("cables")) {
            Ok(l) => cables = l,
            Err(e) => notes.push(format!("wire library: {e}")),
        },
    }
    // Typed structures get their attachments from the structure types.
    let mut line = source.line;
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let resolved = (|| -> Result<ohl_line::Line, String> {
            let dir =
                |kind: &str| -> Result<PathBuf, String> {
                    Ok(library_dir(project(kind), work, kind)?
                        .unwrap_or_else(|| shipped_library(kind)))
                };
            let structures = ohl_structures::StructureLibrary::load_dir(&dir("structures")?)
                .map_err(|e| e.to_string())?;
            let insulators = ohl_structures::InsulatorLibrary::load_dir(&dir("insulators")?)
                .map_err(|e| e.to_string())?;
            line.resolve(&structures, &insulators)
                .map_err(|e| e.to_string())
        })();
        match resolved {
            Ok(l) => line = l,
            Err(e) => notes.push(format!("structure types: {e}")),
        }
        let _ = std::fs::remove_dir_all(work);
    }
    let case = ohl_weather::WeatherCase::still(
        SCENE_CASE,
        ohl_core::units::Temperature::from_celsius(15.0),
    );
    let wires = ohl_line::solve::solve_line(&line, &cables, &case)
        .into_iter()
        .zip(&line.sections)
        .map(|(result, section)| match result {
            Ok(shape) => SceneWire {
                section: section.id.clone(),
                spans: shape
                    .spans
                    .iter()
                    .map(|s| SceneSpan {
                        points: thin(&s.points, SCENE_SPAN_POINTS).iter().map(mm).collect(),
                        sag: s.sag,
                        max_tension: s.max_tension,
                    })
                    .collect(),
                error: None,
            },
            Err(e) => SceneWire {
                section: section.id.clone(),
                spans: Vec::new(),
                error: Some(e.to_string()),
            },
        })
        .collect();
    Scene {
        seq: source.seq,
        revision: source.revision,
        line,
        case: SCENE_CASE.into(),
        wires,
        notes,
    }
}

#[cfg(test)]
mod tests;
