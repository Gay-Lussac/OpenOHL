// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ohl_session::Role;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use crate::server::{Options, Server};
use crate::tests::{Client, local, moved, position, project, temp};
use crate::{ClientMsg, MAX_HELLO, PROTOCOL, Scene, ServerMsg};

type Ws = WebSocketStream<TlsStream<TcpStream>>;

/// TLS to the server, trusting its certificate by fingerprint.
async fn tls(addr: SocketAddr, fp: [u8; 32]) -> TlsStream<TcpStream> {
    let config = crate::client::pinned_tls(fp, b"http/1.1").unwrap();
    let tcp = TcpStream::connect(addr).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("ohl-server").unwrap();
    tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .unwrap()
}

/// An HTTP request; the whole answer.
async fn http(addr: SocketAddr, fp: [u8; 32], request: &str) -> String {
    let mut s = tls(addr, fp).await;
    s.write_all(request.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out).await;
    String::from_utf8_lossy(&out).into_owned()
}

async fn ws(addr: SocketAddr, fp: [u8; 32]) -> Ws {
    let stream = tls(addr, fp).await;
    tokio_tungstenite::client_async("wss://ohl-server/ws", stream)
        .await
        .unwrap()
        .0
}

async fn send(ws: &mut Ws, msg: &ClientMsg) {
    ws.send(Message::text(serde_json::to_string(msg).unwrap()))
        .await
        .unwrap();
}

/// The next server message (`None`: closed), 20 s at most.
async fn next(ws: &mut Ws) -> Option<ServerMsg> {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(20), ws.next())
            .await
            .expect("a message in 20 s");
        match m {
            Some(Ok(Message::Text(t))) => return Some(serde_json::from_str(t.as_str()).unwrap()),
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => {}
        }
    }
}

async fn until(ws: &mut Ws, mut stop: impl FnMut(&ServerMsg) -> bool) -> ServerMsg {
    loop {
        let m = next(ws).await.expect("connection open");
        if stop(&m) {
            return m;
        }
    }
}

async fn join(addr: SocketAddr, fp: [u8; 32], user: &str, code: &str) -> (Ws, ServerMsg) {
    let mut w = ws(addr, fp).await;
    let hello = ClientMsg::Hello {
        protocol: PROTOCOL,
        user: user.into(),
        code: code.into(),
    };
    send(&mut w, &hello).await;
    let answer = next(&mut w).await.expect("an answer");
    (w, answer)
}

fn scene(m: ServerMsg) -> Scene {
    match m {
        ServerMsg::Scene(s) => *s,
        other => panic!("not a scene: {other:?}"),
    }
}

/// A browser joins: the page's files and nothing else; the same access
/// codes as QUIC; a solved scene, then a QUIC user's edit and a new scene;
/// a viewer's edit refused; presence reaches the QUIC user; a hello too
/// large is refused.
#[test]
fn web_clients_join_and_follow() {
    let dir = temp("web");
    let path = project(&dir);
    let server = Server::start(
        &path,
        Options {
            web: Some("127.0.0.1:0".parse().unwrap()),
            ..local("abcd2345")
        },
    )
    .unwrap();
    let addr = server.web_addr.unwrap();
    let fp = server.invitation.fingerprint;
    let general = crate::access_code(server.secret(), None);
    let link = server.web_link(None).unwrap();
    assert!(link.starts_with("https://127.0.0.1:"), "{link}");
    assert!(link.ends_with(&format!("/#code={general}")), "{link}");
    let personal = server.web_link(Some("bob b")).unwrap();
    assert!(personal.contains("#user=bob%20b&code="), "{personal}");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // The files, and nothing else.
    rt.block_on(async {
        let page = http(addr, fp, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
        assert!(page.contains("Content-Security-Policy"));
        assert!(page.contains("<html"));
        let js = http(addr, fp, "GET /app.js HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(js.starts_with("HTTP/1.1 200 OK") && js.contains("text/javascript"));
        for bad in ["/../Cargo.toml", "/src/web.rs", "//etc/passwd", "/app.js/"] {
            let r = http(addr, fp, &format!("GET {bad} HTTP/1.1\r\nHost: x\r\n\r\n")).await;
            assert!(r.starts_with("HTTP/1.1 404"), "{bad}: {r}");
        }
        let post = http(addr, fp, "POST / HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(post.starts_with("HTTP/1.1 405"), "{post}");
        let garbage = http(addr, fp, "hello\r\n\r\n").await;
        assert!(garbage.starts_with("HTTP/1.1 400"), "{garbage}");
    });

    // Refused: a wrong code; a user of the project without their own; a
    // hello larger than the QUIC path allows.
    rt.block_on(async {
        let (_, m) = join(addr, fp, "dave", "nope").await;
        assert!(
            matches!(&m, ServerMsg::Refused { reason } if reason.contains("access code")),
            "{m:?}"
        );
        let (_, m) = join(addr, fp, "bob", &general).await;
        assert!(
            matches!(&m, ServerMsg::Refused { reason } if reason.contains("personal invitation")),
            "{m:?}"
        );
        let mut w = ws(addr, fp).await;
        let big = format!(
            "{{\"type\":\"hello\",\"protocol\":1,\"user\":\"{}\",\"code\":\"x\"}}",
            "x".repeat(MAX_HELLO)
        );
        let _ = w.send(Message::text(big)).await;
        assert_eq!(next(&mut w).await, None);
    });

    // Carol joins in a browser: a viewer, then the scene.
    let (mut carol, first) = rt.block_on(async {
        let (mut w, m) = join(addr, fp, "carol", &general).await;
        match m {
            ServerMsg::Welcome { role, size, .. } => {
                assert_eq!(role, Role::Viewer);
                assert_eq!(size, 0);
            }
            other => panic!("{other:?}"),
        }
        let s = scene(until(&mut w, |m| matches!(m, ServerMsg::Scene(_))).await);
        (w, s)
    });
    assert!(!first.line.structures.is_empty());
    assert_eq!(first.wires.len(), first.line.sections.len());
    for w in &first.wires {
        assert!(w.error.is_none(), "{:?}", w.error);
        assert!(!w.spans.is_empty() && w.spans.iter().all(|s| s.sag > 0.0));
        // Thinned for the browser, ends kept.
        assert!(
            w.spans
                .iter()
                .all(|s| (2..=super::SCENE_SPAN_POINTS).contains(&s.points.len()))
        );
    }

    // Bob, a designer, in a browser: the web client only views — his edit
    // and lock are refused.
    rt.block_on(async {
        let code = crate::access_code(server.secret(), Some("bob"));
        let (mut w, m) = join(addr, fp, "bob", &code).await;
        assert!(
            matches!(
                m,
                ServerMsg::Welcome {
                    role: Role::Designer,
                    ..
                }
            ),
            "{m:?}"
        );
        let id = first.line.structures[0].id.clone();
        let p = first.line.structures[0].position;
        send(
            &mut w,
            &ClientMsg::Request {
                id: 3,
                op: moved(&id, [p[0] + 1.0, p[1], p[2]]),
                line: "main".into(),
            },
        )
        .await;
        let m = until(&mut w, |m| matches!(m, ServerMsg::Response { id: 3, .. })).await;
        assert!(
            matches!(&m, ServerMsg::Response { result: Err(e), .. } if e == super::VIEW_ONLY),
            "{m:?}"
        );
        send(
            &mut w,
            &ClientMsg::Lock {
                target: format!("main:structure:{id}"),
            },
        )
        .await;
        let m = until(&mut w, |m| matches!(m, ServerMsg::Notice { .. })).await;
        assert!(
            matches!(&m, ServerMsg::Notice { text } if text == super::VIEW_ONLY),
            "{m:?}"
        );
        let _ = w.close(None).await;
    });

    // Alice (QUIC) moves a structure: carol sees the edit, then the line
    // solved again.
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let id = alice.replica.line().structures[1].id.clone();
    let p0 = position(&alice.replica, &id);
    let p1 = [p0[0] + 4.0, p0[1], p0[2]];
    alice.request(moved(&id, p1)).unwrap();
    let next_scene = rt.block_on(async {
        until(
            &mut carol,
            |m| matches!(m, ServerMsg::Applied { user, .. } if user == "alice"),
        )
        .await;
        scene(until(&mut carol, |m| matches!(m, ServerMsg::Scene(_))).await)
    });
    assert!(next_scene.revision > first.revision);
    assert_eq!(next_scene.seq, alice.replica.last_seq().unwrap());
    let moved_one = next_scene.line.structures.iter().find(|s| s.id == id);
    assert_eq!(moved_one.unwrap().position, p1);

    // A viewer's edit is refused by the authority; presence goes out.
    rt.block_on(async {
        let request = ClientMsg::Request {
            id: 7,
            op: moved(&id, p0),
            line: "main".into(),
        };
        send(&mut carol, &request).await;
        let m = until(&mut carol, |m| {
            matches!(m, ServerMsg::Response { id: 7, .. })
        })
        .await;
        assert!(
            matches!(&m, ServerMsg::Response { result: Err(e), .. } if e.contains("viewer")),
            "{m:?}"
        );
        let presence = crate::Presence {
            selection: "structure T1".into(),
            camera: Some([1.0, 2.0, 3.0]),
        };
        send(&mut carol, &ClientMsg::Presence(presence)).await;
    });
    alice.pump(|m| {
        matches!(m, ServerMsg::Peers { peers } if peers.iter().any(|p| p.user == "carol" && p.presence.selection == "structure T1"))
    });

    // Carol leaves: the server lists only alice.
    rt.block_on(async {
        let _ = carol.close(None).await;
    });
    let t = std::time::Instant::now();
    while server.peers().len() != 1 {
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(alice);
    server.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scene_edits_and_escapes() {
    use crate::Op;
    // The scene shows the main line: edits of other lines do not touch it.
    assert!(super::changes_scene(&Op::Undo, "main"));
    assert!(!super::changes_scene(&Op::Undo, "phase 1"));
    assert!(!super::changes_scene(
        &Op::CreateLine {
            name: "a".into(),
            from: "main".into()
        },
        "main"
    ));
    assert!(super::changes_scene(
        &Op::DeleteLibrary {
            kind: "cables".into(),
            name: "a.ohlw".into()
        },
        "phase 1"
    ));
    assert!(!super::changes_scene(
        &Op::DeleteLibrary {
            kind: "weather".into(),
            name: "a.ohlwc".into()
        },
        "main"
    ));
    assert_eq!(super::fragment_escape("ann-é b&c"), "ann-%C3%A9%20b%26c");
}

/// The scripts parse (when Node.js is installed; skipped otherwise).
#[test]
fn scripts_parse_when_node_is_installed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    for (path, _, _) in super::FILES.iter().filter(|(p, _, _)| p.ends_with(".js")) {
        let file = dir.join(path.trim_start_matches('/'));
        match std::process::Command::new("node")
            .arg("--check")
            .arg(&file)
            .output()
        {
            Ok(out) => assert!(
                out.status.success(),
                "{}: {}",
                file.display(),
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(_) => return,
        }
    }
}
