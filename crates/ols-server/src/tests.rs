// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ols_session::{Command, Role, Session};

use crate::client::{Incoming, Joined, join, replay};
use crate::server::{Options, Server};
use crate::{ClientMsg, Invitation, Op, ServerMsg};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ols-server-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project of the synthetic line: alice its admin, bob a designer.
fn project(dir: &Path) -> PathBuf {
    let line = ols_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json"),
    )
    .unwrap();
    let path = dir.join("p.ols");
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
        newcomers: Some(Role::Viewer),
    }
}

/// A client with its replica.
struct Client {
    joined: Joined,
    replica: Session,
}

impl Client {
    fn join(inv: &Invitation, user: &str, dir: &Path) -> Result<Self, String> {
        let file = dir.join(format!("{user}.ols"));
        let mut seen = 0;
        let joined = join(inv, user, &file, &mut |p| seen = p.received)?;
        assert_eq!(seen, std::fs::metadata(&file).unwrap().len());
        let replica = Session::open(&file).unwrap();
        assert_eq!(replica.last_seq().unwrap(), joined.seq);
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
                    if let ServerMsg::Applied { user, op, seq } = &m {
                        replay(&mut self.replica, user, op, *seq).unwrap();
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

    /// Requests an edit and waits for its answer.
    fn request(&mut self, op: Op) -> Result<String, String> {
        let id = self.joined.link.request(op).unwrap();
        let got = self.pump(|m| matches!(m, ServerMsg::Response { id: i, .. } if *i == id));
        match got.last() {
            Some(ServerMsg::Response { result, .. }) => result.clone(),
            _ => unreachable!(),
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

/// Two users edit one project: each sees the other's edits in order, with
/// the server's log numbers; undo is per user; a soft lock keeps others
/// off an object; viewers cannot edit; the admin rolls back; a wrong code
/// or another server's certificate is refused.
#[test]
fn users_share_a_project() {
    let dir = temp("share");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let inv = server.invitation.clone();
    assert_eq!(inv.to_string().parse::<Invitation>().unwrap(), inv);
    // A user of the project joins with their personal invitation only.
    let general = Client::join(&inv, "alice", &dir).err().unwrap();
    assert!(general.contains("personal invitation"), "{general}");
    let other = Client::join(&server.invitation_for("bob"), "alice", &dir)
        .err()
        .unwrap();
    assert!(other.contains("access code"), "{other}");
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let mut bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    assert_eq!(bob.joined.role, Role::Designer);
    let id = alice.replica.line().structures[1].id.clone();
    let p0 = position(&alice.replica, &id);

    // Alice moves a structure: both replicas follow.
    let p1 = [p0[0] + 5.0, p0[1], p0[2]];
    assert_eq!(alice.request(moved(&id, p1)).unwrap(), "move structure");
    bob.pump(|m| matches!(m, ServerMsg::Applied { .. }));
    assert_eq!(position(&alice.replica, &id), p1);
    assert_eq!(position(&bob.replica, &id), p1);
    assert_eq!(
        alice.replica.last_seq().unwrap(),
        bob.replica.last_seq().unwrap()
    );

    // Bob has nothing to undo; alice's undo is hers.
    assert!(
        bob.request(Op::Undo)
            .unwrap_err()
            .contains("nothing to undo")
    );
    alice.request(Op::Undo).unwrap();
    bob.pump(|m| matches!(m, ServerMsg::Applied { .. }));
    assert_eq!(position(&bob.replica, &id), p0);

    // Alice holds the structure: bob's move is refused.
    let target = crate::lock_target(ols_session::Kind::Structure, &id);
    alice
        .joined
        .link
        .send(ClientMsg::Lock {
            target: target.clone(),
        })
        .unwrap();
    bob.pump(|m| matches!(m, ServerMsg::Locks { locks: l } if l.contains_key(&target)));
    let refused = bob.request(moved(&id, p1)).unwrap_err();
    assert!(refused.contains("being edited by alice"), "{refused}");
    // Her own edit goes through.
    alice.request(moved(&id, p1)).unwrap();
    alice
        .joined
        .link
        .send(ClientMsg::Unlock {
            target: target.clone(),
        })
        .unwrap();
    bob.pump(|m| matches!(m, ServerMsg::Locks { locks: l } if l.is_empty()));
    let p2 = [p1[0], p1[1] + 2.0, p1[2]];
    bob.request(moved(&id, p2)).unwrap();
    alice.pump(|m| matches!(m, ServerMsg::Applied { user, .. } if user == "bob"));
    assert_eq!(position(&alice.replica, &id), p2);

    // A newcomer is a viewer: her edits are refused. She sees the others.
    let mut carol = Client::join(&inv, "carol", &dir).unwrap();
    assert_eq!(carol.joined.role, Role::Viewer);
    assert_eq!(position(&carol.replica, &id), p2);
    assert_eq!(
        carol
            .joined
            .peers
            .iter()
            .map(|p| p.user.as_str())
            .collect::<Vec<_>>(),
        ["alice", "bob", "carol"]
    );
    assert!(
        carol
            .request(moved(&id, p0))
            .unwrap_err()
            .contains("viewer")
    );
    // Alice made carol a user (the server's SetUser) and sees her join.
    alice.pump(|m| matches!(m, ServerMsg::Peers { peers: p } if p.len() == 3));
    assert_eq!(alice.replica.role("carol").unwrap(), Some(Role::Viewer));
    // The admin makes her a designer.
    alice
        .request(Op::SetUser {
            name: "carol".into(),
            role: Role::Designer,
        })
        .unwrap();
    carol.pump(|m| {
        matches!(
            m,
            ServerMsg::Applied {
                op: Op::SetUser { .. },
                ..
            }
        )
    });
    carol.request(moved(&id, p0)).unwrap();

    // Admin rollback: back to just after alice's first move.
    let first = alice
        .replica
        .log()
        .unwrap()
        .iter()
        .find(|e| e.user == "alice" && e.kind == "do")
        .unwrap()
        .seq;
    assert!(
        bob.request(Op::RollbackAfter { seq: first }).is_err(),
        "admins only"
    );
    alice.request(Op::RollbackAfter { seq: first }).unwrap();
    for c in [&mut bob, &mut carol] {
        c.pump(|m| {
            matches!(
                m,
                ServerMsg::Applied {
                    op: Op::RollbackAfter { .. },
                    ..
                }
            )
        });
    }
    for c in [&alice, &bob, &carol] {
        assert_eq!(position(&c.replica, &id), p1);
    }
    let server_session_seq = alice.replica.last_seq().unwrap();
    assert_eq!(carol.replica.last_seq().unwrap(), server_session_seq);

    // Presence reaches the others.
    bob.joined
        .link
        .send(ClientMsg::Presence(crate::Presence {
            selection: "structure T1".into(),
            camera: Some([1.0, 2.0, 3.0]),
        }))
        .unwrap();
    alice.pump(|m| {
        matches!(m, ServerMsg::Peers { peers: p } if p.iter().any(|x| x.presence.selection == "structure T1"))
    });

    // Leaving releases: the server lists who is left.
    drop(carol);
    let t = Instant::now();
    while server.peers().len() != 2 {
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    // Same name twice, wrong code, another certificate: refused.
    let again = Client::join(&server.invitation_for("bob"), "bob", &dir)
        .err()
        .unwrap();
    assert!(again.contains("connected already"), "{again}");
    let mut wrong = inv.clone();
    wrong.code = "nope".into();
    assert!(
        Client::join(&wrong, "dave", &dir)
            .err()
            .unwrap()
            .contains("access code")
    );
    let mut other = inv.clone();
    other.fingerprint[0] ^= 1;
    assert!(Client::join(&other, "dave", &dir).is_err());

    // The server wrote everything to the project.
    drop(alice);
    drop(bob);
    server.stop();
    let s = Session::open(&path).unwrap();
    assert_eq!(position(&s, &id), p1);
    assert_eq!(s.last_seq().unwrap(), server_session_seq);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invitations_read_back() {
    let inv = Invitation {
        address: "line.example.org:4433".into(),
        code: "k7f39qzt".into(),
        fingerprint: [0xab; 32],
    };
    let text = inv.to_string();
    assert!(text.starts_with("ols://line.example.org:4433/k7f39qzt/abab"));
    assert_eq!(text.parse::<Invitation>().unwrap(), inv);
    assert!("ols://x/y".parse::<Invitation>().is_err());
    assert!("http://a:1/b/00".parse::<Invitation>().is_err());
}

#[test]
fn lock_targets_of_commands() {
    use ols_session::Kind;
    let op = Op::Do {
        command: Command::Batch {
            commands: vec![
                Command::MoveStructure {
                    id: "T1".into(),
                    position: [0.0; 3],
                },
                Command::Delete {
                    kind: Kind::Section,
                    id: "S1".into(),
                    confirm: false,
                },
            ],
        },
    };
    assert_eq!(
        op.targets(),
        Some(vec![
            (Kind::Structure, "T1".into()),
            (Kind::Section, "S1".into())
        ])
    );
    assert_eq!(Op::Undo.targets(), Some(vec![]));
}

#[test]
fn viewers_may_not_change_settings_and_names_are_checked() {
    let dir = temp("roles");
    let path = project(&dir);
    let mut s = Session::open(&path).unwrap();
    crate::execute(
        &mut s,
        "alice",
        &Op::SetUser {
            name: "carol".into(),
            role: Role::Viewer,
        },
    )
    .unwrap();
    let set = Op::SetSetting {
        key: "app.check_surface".into(),
        value: Some("x".into()),
    };
    assert!(crate::execute(&mut s, "carol", &set).is_err());
    assert!(crate::execute(&mut s, "bob", &set).is_ok());
    assert!(crate::check_name("bob").is_ok());
    for bad in ["", " bob", "bob\n", &"x".repeat(crate::MAX_NAME + 1)] {
        assert!(crate::check_name(bad).is_err(), "{bad:?}");
    }
    assert!(crate::same_code("abc123", "abc123"));
    assert!(!crate::same_code("abc123", "abc124") && !crate::same_code("abc", "abc1"));
    // Multi-byte characters in the fingerprint: refused, not a panic.
    let fp = "é".repeat(32);
    assert!(format!("ols://h:1/c/{fp}").parse::<Invitation>().is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
