// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ohl_session::{Command, Role, Session};

use crate::client::{Incoming, Joined, join, replay};
use crate::server::{Options, Server};
use crate::{ClientMsg, Invitation, Op, ServerMsg};

pub(crate) fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ohl-server-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project of the synthetic line: alice its admin, bob a designer.
pub(crate) fn project(dir: &Path) -> PathBuf {
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

pub(crate) fn local(code: &str) -> Options {
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
pub(crate) struct Client {
    pub(crate) joined: Joined,
    pub(crate) replica: Session,
}

impl Client {
    pub(crate) fn join(inv: &Invitation, user: &str, dir: &Path) -> Result<Self, String> {
        let file = dir.join(format!("{user}.ohl"));
        let mut seen = 0;
        let joined = join(inv, user, &file, &mut |p| seen = p.received)?;
        assert_eq!(seen, std::fs::metadata(&file).unwrap().len());
        let replica = Session::open(&file).unwrap();
        assert_eq!(replica.last_seq().unwrap(), joined.seq);
        Ok(Self { joined, replica })
    }

    /// Takes messages until `until` says stop (10 s at most), applying
    /// the edits to the replica; returns the messages.
    pub(crate) fn pump(&mut self, mut until: impl FnMut(&ServerMsg) -> bool) -> Vec<ServerMsg> {
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

    /// Requests an edit and waits for its answer.
    pub(crate) fn request(&mut self, op: Op) -> Result<String, String> {
        let id = self.joined.link.request(op).unwrap();
        let got = self.pump(|m| matches!(m, ServerMsg::Response { id: i, .. } if *i == id));
        match got.last() {
            Some(ServerMsg::Response { result, .. }) => result.clone(),
            _ => unreachable!(),
        }
    }
}

pub(crate) fn position(s: &Session, id: &str) -> [f64; 3] {
    s.line()
        .structures
        .iter()
        .find(|x| x.id == id)
        .unwrap()
        .position
}

pub(crate) fn moved(id: &str, p: [f64; 3]) -> Op {
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
    let target = crate::lock_target("main", ohl_session::Kind::Structure, &id);
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
    assert!(text.starts_with("ohl://line.example.org:4433/k7f39qzt/abab"));
    assert_eq!(text.parse::<Invitation>().unwrap(), inv);
    // A link of before 2026-10-18.
    assert_eq!(
        text.replacen("ohl://", "ols://", 1)
            .parse::<Invitation>()
            .unwrap(),
        inv
    );
    assert!("ohl://x/y".parse::<Invitation>().is_err());
    assert!("http://a:1/b/00".parse::<Invitation>().is_err());
}

#[test]
fn lock_targets_of_commands() {
    use ohl_session::Kind;
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
        "main",
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
    assert!(crate::execute(&mut s, "carol", "main", &set).is_err());
    assert!(crate::execute(&mut s, "bob", "main", &set).is_ok());
    assert!(crate::check_name("bob").is_ok());
    for bad in ["", " bob", "bob\n", &"x".repeat(crate::MAX_NAME + 1)] {
        assert!(crate::check_name(bad).is_err(), "{bad:?}");
    }
    assert!(crate::same_code("abc123", "abc123"));
    assert!(!crate::same_code("abc123", "abc124") && !crate::same_code("abc", "abc1"));
    // Multi-byte characters in the fingerprint: refused, not a panic.
    let fp = "é".repeat(32);
    assert!(format!("ohl://h:1/c/{fp}").parse::<Invitation>().is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replicas_follow_version_control() {
    let dir = temp("versions");
    let path = project(&dir);
    let mut server = Session::open(&path).unwrap();
    let copy = dir.join("replica.ohl");
    server.copy_to(&copy).unwrap();
    let mut replica = Session::open(&copy).unwrap();
    let id = server.line().structures[5].id.clone();
    let p = position(&server, &id);
    // Bob works on a phase line while alice stays on main.
    let ops = [
        (
            "alice",
            "main",
            Op::Commit {
                message: "first".into(),
            },
        ),
        (
            "bob",
            "main",
            Op::CreateLine {
                name: "alt".into(),
                from: "main".into(),
            },
        ),
        ("bob", "alt", moved(&id, [p[0] + 3.0, p[1], p[2]])),
        (
            "bob",
            "alt",
            Op::Commit {
                message: "moved".into(),
            },
        ),
        (
            "bob",
            "alt",
            Op::SubmitReview {
                branch: "alt".into(),
                target: "main".into(),
                reviewers: vec![],
                note: String::new(),
            },
        ),
        (
            "alice",
            "main",
            Op::DecideReview {
                id: 1,
                approve: true,
                comment: "ok".into(),
            },
        ),
        (
            "alice",
            "alt",
            Op::MergeReview {
                id: 1,
                message: None,
                choices: Default::default(),
            },
        ),
        (
            "alice",
            "main",
            Op::Tag {
                name: "rev A".into(),
                at: None,
                note: String::new(),
            },
        ),
        (
            "bob",
            "main",
            Op::RenameLine {
                from: "alt".into(),
                to: "phase 1".into(),
            },
        ),
    ];
    for (user, line, op) in &ops {
        let seq = crate::execute(&mut server, user, line, op)
            .unwrap_or_else(|e| panic!("{}: {e}", op.describe()))
            .map(|a| a.seq);
        replay(&mut replica, user, line, op, seq).unwrap();
        // Both keep the line they have open.
        assert_eq!(server.current_line(), "main");
        assert_eq!(replica.current_line(), "main");
    }
    // The same revisions (ids are content hashes), branches and line.
    let ids = |s: &Session| {
        s.revisions()
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&server), ids(&replica));
    assert_eq!(ids(&server).len(), 2);
    assert_eq!(
        server.head_revision().unwrap(),
        replica.head_revision().unwrap()
    );
    assert_eq!(server.line(), replica.line());
    assert_eq!(position(&replica, &id)[0], p[0] + 3.0);
    let lines = |s: &Session| {
        s.lines()
            .unwrap()
            .into_iter()
            .map(|l| (l.name, l.user, l.revision, l.objects))
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&server), lines(&replica));
    assert!(replica.is_line("phase 1"));
    // The phase kept as its differences from main's revision: read back
    // alike from the replica's file and from a new copy of the server's.
    drop(replica);
    let replica = Session::open(&copy).unwrap();
    let fresh = dir.join("fresh.ohl");
    server.copy_to(&fresh).unwrap();
    let fresh = Session::open(&fresh).unwrap();
    for s in [&replica, &fresh] {
        assert_eq!(lines(s), lines(&server));
        assert_eq!(
            s.line_named("phase 1").unwrap(),
            server.line_named("phase 1").unwrap()
        );
    }
    assert!(
        Op::BringIn {
            from: "main".into(),
            choices: Default::default()
        }
        .targets()
        .is_none()
    );
}
