// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The server through its public interface (code review 2026-10-26): a
//! refused join leaves nothing behind, two clients editing the same
//! object at once end with the same project in the same order, and a
//! client gone in the middle of a request leaves the server serving the
//! others. Each test has its own server on a free local port; every wait
//! is bounded (10 s).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ohl_server::client::{Incoming, Joined, join, replay};
use ohl_server::server::{Options, Server};
use ohl_server::{ClientMsg, Invitation, Op, ServerMsg};
use ohl_session::{Command, Role, Session};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ohl-server-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project of the synthetic line: alice its admin, bob a designer.
fn project(dir: &Path) -> PathBuf {
    let line = ohl_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    let path = dir.join("p.ohl");
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "alice").unwrap();
    s.set_user("alice", "bob", Role::Designer).unwrap();
    s.checkpoint().unwrap();
    path
}

fn local(code: &str) -> Options {
    Options {
        bind: "127.0.0.1:0".parse().unwrap(),
        advertise: None,
        code: Some(code.into()),
        keys: None,
        newcomers: Some(Role::Viewer),
        web: None,
    }
}

/// A client with its replica.
struct Client {
    joined: Joined,
    replica: Session,
}

impl Client {
    fn join(inv: &Invitation, user: &str, dir: &Path) -> Result<Self, String> {
        let file = dir.join(format!("{user}.ohl"));
        let joined = join(inv, user, &file, &mut |_| {})?;
        let replica = Session::open(&file).unwrap();
        Ok(Self { joined, replica })
    }

    /// Takes messages until `until` says stop (10 s at most), applying
    /// the edits to the replica; returns the messages.
    fn pump(&mut self, mut until: impl FnMut(&ServerMsg) -> bool) -> Vec<ServerMsg> {
        let started = Instant::now();
        let mut got = Vec::new();
        loop {
            assert!(started.elapsed() < Duration::from_secs(10), "{got:?}");
            match self.joined.link.recv_timeout(Duration::from_millis(50)) {
                Some(Incoming::Msg(m)) => {
                    if let ServerMsg::Applied {
                        user,
                        op,
                        line,
                        seq,
                    } = &m
                    {
                        replay(&mut self.replica, user, line, op, *seq).unwrap();
                    }
                    let stop = until(&m);
                    got.push(m);
                    if stop {
                        return got;
                    }
                }
                Some(Incoming::Closed(why)) => panic!("closed: {why}"),
                None => {}
            }
        }
    }
}

fn position(s: &Session, id: &str) -> [f64; 3] {
    s.line()
        .structures
        .iter()
        .find(|x| x.id == id)
        .unwrap()
        .position
}

fn moved(id: &str, p: [f64; 3]) -> Op {
    Op::Do {
        command: Command::MoveStructure {
            id: id.into(),
            position: p,
        },
    }
}

/// Waits (10 s at most) until the server lists `n` users.
fn peers(server: &Server, n: usize) {
    let t = Instant::now();
    while server.peers().len() != n {
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "{:?}",
            server.peers()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Refused joins — a wrong access code, another server's certificate, a
/// name with control characters or surrounding spaces, a project user on
/// the general invitation — each say why, leave no user connected and no
/// user added to the project; the server goes on serving (a good join
/// after them works).
#[test]
fn a_refused_join_leaves_nothing_behind() {
    let dir = temp("refused");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let inv = server.invitation.clone();
    let mut wrong = inv.clone();
    wrong.code = "nope".into();
    let e = Client::join(&wrong, "dave", &dir).err().unwrap();
    assert!(e.contains("access code"), "{e}");
    let mut other = inv.clone();
    other.fingerprint[0] ^= 1;
    assert!(Client::join(&other, "dave", &dir).is_err());
    for bad in ["", " dave", "da\nve"] {
        assert!(Client::join(&inv, bad, &dir).is_err(), "{bad:?}");
    }
    let e = Client::join(&inv, "alice", &dir).err().unwrap();
    assert!(e.contains("personal invitation"), "{e}");
    peers(&server, 0);
    // A good join still works, and finds only itself.
    let bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    assert_eq!(bob.joined.role, Role::Designer);
    assert_eq!(
        bob.joined
            .peers
            .iter()
            .map(|p| p.user.as_str())
            .collect::<Vec<_>>(),
        ["bob"]
    );
    drop(bob);
    server.stop();
    // No refused name was made a user of the project.
    let s = Session::open(&path).unwrap();
    assert_eq!(s.role("dave").unwrap(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two clients move the same structure at once, several times each,
/// without waiting: every request is answered, both see every edit in the
/// server's one order (the same log numbers), and both replicas and the
/// project end with the structure where the last edit in that order put
/// it.
#[test]
fn two_clients_editing_the_same_object_agree() {
    let dir = temp("same-object");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let mut bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    let id = alice.replica.line().structures[1].id.clone();
    let p0 = position(&alice.replica, &id);
    let n = 5;
    let mut ids = (Vec::new(), Vec::new());
    for k in 1..=n {
        let a = [p0[0] + k as f64, p0[1], p0[2]];
        let b = [p0[0], p0[1] + k as f64, p0[2]];
        ids.0
            .push(alice.joined.link.request(moved(&id, a)).unwrap());
        ids.1.push(bob.joined.link.request(moved(&id, b)).unwrap());
    }
    // Each takes the 2n edits and its own n answers.
    let take = |c: &mut Client, mine: &[u64]| -> Vec<(String, Option<i64>)> {
        let (mut applied, mut answered) = (Vec::new(), 0);
        c.pump(|m| {
            match m {
                ServerMsg::Applied { user, seq, .. } => applied.push((user.clone(), *seq)),
                ServerMsg::Response { id, result } if mine.contains(id) => {
                    assert!(result.is_ok(), "{result:?}");
                    answered += 1;
                }
                _ => {}
            }
            applied.len() == 2 * n && answered == n
        });
        applied
    };
    let seen_a = take(&mut alice, &ids.0);
    let seen_b = take(&mut bob, &ids.1);
    assert_eq!(seen_a, seen_b, "one order for both");
    let seqs: Vec<i64> = seen_a.iter().filter_map(|x| x.1).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    let (pa, pb) = (position(&alice.replica, &id), position(&bob.replica, &id));
    assert_eq!(pa, pb);
    // The last edit in the server's order is the one kept: alice's last
    // (x moved) or bob's last (y moved).
    let last = &seen_a.last().unwrap().0;
    let expected = if last == "alice" {
        [p0[0] + n as f64, p0[1], p0[2]]
    } else {
        [p0[0], p0[1] + n as f64, p0[2]]
    };
    assert_eq!(pa, expected);
    assert_eq!(
        alice.replica.last_seq().unwrap(),
        bob.replica.last_seq().unwrap()
    );
    drop(alice);
    drop(bob);
    server.stop();
    let s = Session::open(&path).unwrap();
    assert_eq!(position(&s, &id), expected);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A client that sends a request and disconnects before its answer: the
/// server neither stops nor hangs — the edit is applied in order or not
/// at all, the other client's replica agrees with the project, the client
/// is no longer listed, its soft locks are released, and the other client
/// goes on editing (the same object too).
#[test]
fn a_client_gone_mid_request_leaves_the_server_serving() {
    let dir = temp("gone");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let mut bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    peers(&server, 2);
    let id = alice.replica.line().structures[1].id.clone();
    let p0 = position(&alice.replica, &id);
    // Bob holds the structure, asks for a move and is gone at once.
    let target = ohl_server::lock_target("main", ohl_session::Kind::Structure, &id);
    bob.joined
        .link
        .send(ClientMsg::Lock {
            target: target.clone(),
        })
        .unwrap();
    alice.pump(|m| matches!(m, ServerMsg::Locks { locks } if locks.contains_key(&target)));
    let gone = [p0[0] + 7.0, p0[1], p0[2]];
    bob.joined.link.request(moved(&id, gone)).unwrap();
    drop(bob);
    peers(&server, 1);
    // His lock is released (a Locks message without it reaches alice).
    alice.pump(|m| matches!(m, ServerMsg::Locks { locks } if !locks.contains_key(&target)));
    // Alice edits the same structure; her answer comes.
    let mine = [p0[0], p0[1] + 3.0, p0[2]];
    let rid = alice.joined.link.request(moved(&id, mine)).unwrap();
    let got = alice.pump(|m| matches!(m, ServerMsg::Response { id, .. } if *id == rid));
    match got.last() {
        Some(ServerMsg::Response { result, .. }) => assert!(result.is_ok(), "{result:?}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(position(&alice.replica, &id), mine);
    let seq = alice.replica.last_seq().unwrap();
    // A newcomer gets the same project.
    let carol = Client::join(&inv_of(&server), "carol", &dir).unwrap();
    assert_eq!(position(&carol.replica, &id), mine);
    drop(carol);
    drop(alice);
    server.stop();
    let s = Session::open(&path).unwrap();
    assert_eq!(position(&s, &id), mine);
    assert_eq!(s.last_seq().unwrap(), seq);
    // Bob's edit, if the server took it before he went, is in the log
    // before alice's (one order), never after.
    let log = s.log().unwrap();
    let bobs = log.iter().filter(|e| e.user == "bob").count();
    assert!(bobs <= 1, "{bobs}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn inv_of(server: &Server) -> Invitation {
    server.invitation.clone()
}
