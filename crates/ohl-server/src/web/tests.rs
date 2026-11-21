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
    let general = server.keys().code(None);
    let link = server.web_link(None).unwrap();
    assert!(link.starts_with("https://127.0.0.1:"), "{link}");
    assert!(link.ends_with(&format!("/#code={general}")), "{link}");
    let personal = server.web_link(Some("bob b")).unwrap();
    assert!(personal.contains("#user=bob%20b&code="), "{personal}");
    // The sign-in window's question (2026-11-20): a project's server takes
    // invitations only.
    assert_eq!(
        crate::sign_in::options(&server.invitation).unwrap(),
        crate::sign_in::SignInOptions::default()
    );
    let rt = tokio::runtime::Runtime::new().unwrap();

    // The files, and nothing else.
    rt.block_on(async {
        // The same question over the web client's socket: answered.
        let mut w = ws(addr, fp).await;
        send(
            &mut w,
            &ClientMsg::SignInOptions {
                protocol: crate::PROTOCOL,
            },
        )
        .await;
        assert!(
            matches!(next(&mut w).await, Some(ServerMsg::SignIn(o)) if o.oidc.is_none()),
            "the sign-in options"
        );
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
        let code = server.keys().code(Some("bob"));
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

/// On a network's server a browser gets what the network holds, asks for
/// the tiles around its view and draws their packages; a change touching
/// them comes, and the tile read again shows it; edits refused (a
/// viewer); protocol 2 refused with the reason.
#[cfg(feature = "network")]
#[test]
fn web_viewers_read_tiles_of_a_network() {
    use crate::network_tests as n;
    let Some(t) = n::test_network("web", 300) else {
        return;
    };
    let server = Server::start_network(
        Options {
            web: Some("127.0.0.1:0".parse().unwrap()),
            ..n::local("abcd2345")
        },
        n::net_options(&t.url),
    )
    .unwrap();
    let addr = server.web_addr.unwrap();
    let fp = server.invitation.fingerprint;
    let general = server.keys().code(None);
    let personal = server.keys().code(Some("wendy"));
    let code = t.codes[0].clone();
    let (id, p) = n::structure_of(&t, &code, 2);
    let tiles = n::tiles_around(p, 1);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut w = rt.block_on(async {
        let mut old = ws(addr, fp).await;
        let hello = ClientMsg::Hello {
            protocol: 2,
            user: "wendy".into(),
            code: general.clone(),
        };
        send(&mut old, &hello).await;
        let m = next(&mut old).await.unwrap();
        assert!(
            matches!(&m, ServerMsg::Refused { reason } if reason.contains("protocol 2")),
            "{m:?}"
        );
        let (mut w, m) = join(addr, fp, "wendy", &personal).await;
        assert!(
            matches!(
                m,
                ServerMsg::Welcome {
                    role: Role::Viewer,
                    size: 0,
                    ..
                }
            ),
            "{m:?}"
        );
        let info = until(&mut w, |m| matches!(m, ServerMsg::Network(_))).await;
        let ServerMsg::Network(info) = info else {
            unreachable!()
        };
        assert_eq!(info.models.len(), t.codes.len());
        let held: Vec<crate::HeldTile> = tiles
            .iter()
            .map(|k| crate::HeldTile {
                tile: *k,
                rev: None,
            })
            .collect();
        send(
            &mut w,
            &ClientMsg::Interest {
                routes: Vec::new(),
                tiles: held,
            },
        )
        .await;
        let mut got = std::collections::BTreeMap::new();
        while got.len() < tiles.len() {
            if let ServerMsg::Tile(pkg) = until(&mut w, |m| matches!(m, ServerMsg::Tile(_))).await {
                got.insert(pkg.tile, pkg);
            }
        }
        assert!(
            got.values()
                .flat_map(|p| p.structures.iter())
                .any(|s| s.id == id && s.position == p),
            "the structure in its tile"
        );
        // A viewer's edit refused.
        send(
            &mut w,
            &ClientMsg::RequestOn {
                id: 9,
                model: code.clone(),
                op: n::moved(&id, p),
                line: "main".into(),
            },
        )
        .await;
        let m = until(&mut w, |m| matches!(m, ServerMsg::Response { id: 9, .. })).await;
        assert!(
            matches!(&m, ServerMsg::Response { result: Err(e), .. } if e == super::VIEW_ONLY),
            "{m:?}"
        );
        w
    });
    // Bob moves the structure (QUIC): the browser is told, reads it again.
    let bob = crate::client::join_network(
        &server.invitation_for("bob"),
        "bob",
        &crate::client::NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    bob.link
        .send(ClientMsg::RequestOn {
            id: 1,
            model: code.clone(),
            op: n::moved(&id, [p[0], p[1] + 2.0, p[2]]),
            line: "main".into(),
        })
        .unwrap();
    rt.block_on(async {
        let m = until(&mut w, |m| matches!(m, ServerMsg::Change(_))).await;
        let ServerMsg::Change(c) = m else {
            unreachable!()
        };
        let again: Vec<crate::HeldTile> = c
            .tiles
            .iter()
            .filter(|k| tiles.contains(k))
            .map(|k| crate::HeldTile {
                tile: *k,
                rev: None,
            })
            .collect();
        assert!(!again.is_empty());
        let n = again.len();
        send(
            &mut w,
            &ClientMsg::Interest {
                routes: Vec::new(),
                tiles: again,
            },
        )
        .await;
        let mut moved = false;
        for _ in 0..n {
            if let ServerMsg::Tile(pkg) = until(&mut w, |m| matches!(m, ServerMsg::Tile(_))).await {
                assert_eq!(pkg.rev, c.at);
                moved |= pkg
                    .structures
                    .iter()
                    .any(|s| s.id == id && s.position[1] == p[1] + 2.0);
            }
        }
        assert!(moved, "the tile read again shows the move");
    });
    drop(bob);
    server.stop();
}

/// The web page's sign-in with the organisation (2026-11-21), the browser
/// played by Node.js running the page's own `signin.js` (when installed;
/// else the same steps here) against the fake provider on the loopback:
/// the page's security policy names the provider's origin — the configured
/// issuer's, whatever the request says; the page's question answered; the
/// provider allows the page's origin (CORS) and its address as the
/// redirect; the code flow with PKCE; a forged answer, a code used twice
/// and a sign-in too old refused; the join with the ID token a viewer of
/// the network; the same sign-in under another user's name refused.
#[cfg(feature = "network")]
#[test]
fn the_web_page_signs_in_with_the_organisation() {
    use crate::fake_provider::FakeProvider;
    use crate::network_tests as n;
    use crate::sign_in::OidcOffer;
    use ohl_store_pg::rights::RightsCommand as R;
    let Some(t) = n::test_network("web-sign-in", 40) else {
        return;
    };
    let idp = FakeProvider::start("00u-bob");
    ohl_store_pg::Network::connect(&t.url)
        .unwrap()
        .rights_command(
            "admin",
            &R::MapIdentity {
                issuer: idp.issuer.clone(),
                subject: "00u-bob".into(),
                user: "bob".into(),
            },
        )
        .unwrap();
    let oidc = Arc::new(
        crate::oidc::Oidc::new(crate::oidc::OidcConfig::new(&idp.issuer, "ohl-web")).unwrap(),
    );
    let server = Server::start_network(
        Options {
            web: Some("127.0.0.1:0".parse().unwrap()),
            ..n::local("abcd2345")
        },
        crate::network::NetOptions {
            oidc: Some(oidc),
            ..n::net_options(&t.url)
        },
    )
    .unwrap();
    let addr = server.web_addr.unwrap();
    let fp = server.invitation.fingerprint;
    let page = format!("https://{addr}");
    let redirect = format!("{page}/");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // The policy names the configured issuer's origin, not the
        // request's words.
        let r = http(
            addr,
            fp,
            &format!("GET / HTTP/1.1\r\nHost: {addr}\r\nOrigin: https://evil.example\r\n\r\n"),
        )
        .await;
        let csp = r
            .lines()
            .find(|l| l.starts_with("Content-Security-Policy:"))
            .unwrap();
        assert!(
            csp.contains(&format!("connect-src 'self' wss://{addr} {};", idp.issuer)),
            "{csp}"
        );
        assert!(!csp.contains("evil"), "{csp}");
        // The provider's answer comes back to the page's address: the page,
        // no referrer, its sign-in script.
        let back = http(
            addr,
            fp,
            &format!("GET /?code=x&state=y HTTP/1.1\r\nHost: {addr}\r\n\r\n"),
        )
        .await;
        assert!(back.starts_with("HTTP/1.1 200 OK"), "{back}");
        assert!(back.contains("Referrer-Policy: no-referrer") && back.contains("signin.js"));
        let js = http(addr, fp, "GET /signin.js HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(js.starts_with("HTTP/1.1 200 OK") && js.contains("code_challenge_method"));
        // The page's question.
        let mut w = ws(addr, fp).await;
        send(&mut w, &ClientMsg::SignInOptions { protocol: PROTOCOL }).await;
        match next(&mut w).await {
            Some(ServerMsg::SignIn(o)) => assert_eq!(
                o.oidc,
                Some(OidcOffer {
                    issuer: idp.issuer.clone(),
                    client_id: "ohl-web".into()
                })
            ),
            other => panic!("{other:?}"),
        }
    });
    // The provider allows the page's origin, and only it.
    idp.allow_origin(&page);
    let cors = |origin: &str| {
        ureq::get(&format!("{}/.well-known/openid-configuration", idp.issuer))
            .header("Origin", origin)
            .call()
            .unwrap()
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap().to_owned())
    };
    assert_eq!(cors(&page).as_deref(), Some(page.as_str()));
    assert_eq!(cors("https://evil.example"), None);
    let token = node_sign_in(&idp.issuer, &redirect).unwrap_or_else(|| {
        // No Node.js: the same steps as the page.
        rust_sign_in(&idp.issuer, &redirect, &page)
    });
    rt.block_on(async {
        let join = |user: &str| ClientMsg::Join {
            protocol: PROTOCOL,
            user: user.into(),
            code: String::new(),
            model: None,
            since: None,
            routes: Vec::new(),
            tiles: Vec::new(),
            held: false,
            working_set: None,
            see_as: None,
            id_token: Some(token.clone()),
        };
        let mut w = ws(addr, fp).await;
        send(&mut w, &join("bob")).await;
        // Bob, signed in (a designer of the lines; the web client views
        // only).
        let m = next(&mut w).await.unwrap();
        assert!(
            matches!(&m, ServerMsg::Welcome { size: 0, peers, .. }
                if peers.iter().any(|p| p.user == "bob")),
            "{m:?}"
        );
        let info = until(&mut w, |m| matches!(m, ServerMsg::Network(_))).await;
        assert!(matches!(info, ServerMsg::Network(i) if i.models.len() == t.codes.len()));
        drop(w);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut w = ws(addr, fp).await;
        send(&mut w, &join("carol")).await;
        let m = next(&mut w).await.unwrap();
        assert!(
            matches!(&m, ServerMsg::Refused { reason } if reason.contains("another user's")),
            "{m:?}"
        );
    });
    server.stop();
}

/// The page's sign-in in Node.js, with the page's `signin.js`: the ID
/// token (none: Node.js not installed).
#[cfg(feature = "network")]
fn node_sign_in(issuer: &str, redirect: &str) -> Option<String> {
    const DRIVER: &str = r#"
require(process.argv[1]);
const [, , issuer, client, redirect] = process.argv;
(async () => {
  const S = globalThis.OhlSignIn;
  const b = await S.begin({ issuer, client_id: client }, redirect, { user: "bob" });
  const r = await fetch(b.url, { redirect: "manual" });
  const loc = r.headers.get("location") || "";
  if (!loc.startsWith(redirect + "?")) throw new Error("redirected to " + loc);
  const ans = S.answer(new URL(loc).search);
  const refused = async (f) => {
    try { await f(); return null; } catch (e) { return e.message; }
  };
  const forged = await refused(() => S.finish(b.pending, { ...ans, state: "another" }));
  const late = await refused(() =>
    S.finish(b.pending, ans, b.pending.started + S.PENDING_MAX + 1));
  const s = await S.finish(b.pending, ans);
  const twice = await refused(() => S.finish(b.pending, ans));
  const again = await S.renew(s);
  console.log(JSON.stringify({
    url: b.url, id_token: s.id_token, renewed: again.id_token, subject: s.subject,
    user: b.pending.extra.user, forged, late, twice,
    stale: S.stale(s), stale_later: S.stale(s, s.got + 51 * 60 * 1000),
    kept: Object.keys(b.pending).sort(),
  }));
})().catch((e) => { console.error(e.message); process.exit(1); });
"#;
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/signin.js");
    let out = std::process::Command::new("node")
        .arg("-e")
        .arg(DRIVER)
        .arg(&script)
        .arg(issuer)
        .arg("ohl-web")
        .arg(redirect)
        .output()
        .ok()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let url = v["url"].as_str().unwrap();
    assert!(url.starts_with(&format!("{issuer}/authorize?")), "{url}");
    for part in [
        "response_type=code",
        "client_id=ohl-web",
        "code_challenge_method=S256",
        "scope=openid",
    ] {
        assert!(url.contains(part), "{url}");
    }
    assert!(url.contains(&format!(
        "redirect_uri={}",
        redirect.replace(':', "%3A").replace('/', "%2F")
    )));
    assert_eq!(v["subject"], "00u-bob");
    assert_eq!(v["user"], "bob");
    assert!(v["forged"].as_str().unwrap().contains("not this sign-in's"));
    assert!(v["late"].as_str().unwrap().contains("10 minutes"));
    assert!(v["twice"].as_str().unwrap().contains("refused"));
    assert_eq!(v["stale"], false);
    assert_eq!(v["stale_later"], true);
    // Kept across the provider's page: one-time values, no token.
    assert_eq!(
        v["kept"],
        serde_json::json!([
            "client_id",
            "extra",
            "issuer",
            "nonce",
            "redirect",
            "started",
            "state",
            "token_endpoint",
            "verifier"
        ])
    );
    assert_ne!(v["renewed"], v["id_token"]);
    Some(v["id_token"].as_str().unwrap().to_owned())
}

/// The page's steps in Rust (Node.js not installed): discovery, the code
/// flow with PKCE and the page's address as the redirect, the code
/// exchanged from the page's origin.
#[cfg(feature = "network")]
fn rust_sign_in(issuer: &str, redirect: &str, origin: &str) -> String {
    use sha2::{Digest, Sha256};
    let b64url = |d: &[u8]| {
        crate::base64::encode(d)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let d: serde_json::Value = agent
        .get(&format!("{issuer}/.well-known/openid-configuration"))
        .header("Origin", origin)
        .call()
        .unwrap()
        .body_mut()
        .read_json()
        .unwrap();
    let verifier = crate::random_code(64);
    let url = format!(
        "{}?response_type=code&client_id=ohl-web&redirect_uri={}&scope=openid&state=s1&nonce=n1\
         &code_challenge={}&code_challenge_method=S256",
        d["authorization_endpoint"].as_str().unwrap(),
        super::fragment_escape(redirect),
        b64url(&Sha256::digest(verifier.as_bytes()))
    );
    let r = agent.get(&url).call().unwrap();
    let location = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(
        location.starts_with(&format!("{redirect}?code=")),
        "{location}"
    );
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned();
    let mut r = agent
        .post(d["token_endpoint"].as_str().unwrap())
        .header("Origin", origin)
        .send_form([
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect),
            ("client_id", "ohl-web"),
            ("code_verifier", verifier.as_str()),
        ])
        .unwrap();
    assert_eq!(
        r.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some(origin)
    );
    let body: serde_json::Value = r.body_mut().read_json().unwrap();
    body["id_token"].as_str().unwrap().to_owned()
}

/// The provider's origin for the page's policy: the issuer's scheme and
/// host only; HTTPS, or plain HTTP on this computer; none for anything
/// else.
#[test]
fn provider_origins_for_the_page() {
    let o = |issuer: &str| {
        super::provider_origin(&crate::sign_in::OidcOffer {
            issuer: issuer.into(),
            client_id: "c".into(),
        })
    };
    assert_eq!(
        o("https://id.example.org/realms/grid").as_deref(),
        Some("https://id.example.org")
    );
    assert_eq!(
        o("https://id.example.org:8443").as_deref(),
        Some("https://id.example.org:8443")
    );
    assert_eq!(
        o("http://127.0.0.1:5000").as_deref(),
        Some("http://127.0.0.1:5000")
    );
    for bad in [
        "http://id.example.org",
        "ftp://id.example.org",
        "https://id.example.org; script-src *",
        "https://",
        "id.example.org",
        "https://a b",
        "https://user@id.example.org",
    ] {
        assert_eq!(o(bad), None, "{bad}");
    }
}
