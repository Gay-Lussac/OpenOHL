// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The fixes of the security review of 2026-10-26: invitations, a job
//! failing inside the authority, locks, settings, names.

use ohl_session::collab::Grant;
use ohl_session::{Role, Session};

use crate::server::{Job, Options, Server};
use crate::tests::{Client, local, moved, position, project, temp};
use crate::{Keys, Op, ServerMsg};

/// The access codes of before 2026-10-26: SHA-256 of the secret and the
/// name, 6 bytes.
fn old_code(secret: &str, user: Option<&str>) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(secret.as_bytes());
    h.update(b"\n");
    match user {
        Some(u) => h.update(format!("user:{u}").as_bytes()),
        None => h.update(b"*"),
    }
    h.finalize()[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// H2: a short `--code` gives the general invitation only (as before);
/// the personal ones come from a random key kept in a private file, the
/// same across restarts with that file.
#[test]
fn personal_invitations_do_not_follow_from_the_code() {
    // RFC 4231, test case 2.
    let mac: String = crate::hmac_sha256(b"Jefe", b"what do ya want for nothing?")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        mac,
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    let a = Keys::new("tower42", [1; 32]);
    let b = Keys::new("tower42", [2; 32]);
    assert_eq!(a.code(None), b.code(None));
    assert_eq!(
        a.code(None),
        old_code("tower42", None),
        "general links kept"
    );
    assert_ne!(a.code(Some("alice")), b.code(Some("alice")));
    assert_eq!(a.code(Some("alice")).len(), 32);
    assert_eq!(format!("{a:?}"), "Keys(..)");

    let dir = temp("keys");
    let path = project(&dir);
    let file = dir.join("p.ohl.server-key");
    let options = || Options {
        keys: Some(file.clone()),
        ..local("tower42")
    };
    let server = Server::start(&path, options()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(server.invitation.code, old_code("tower42", None));
    // Who guessed the code from the general invitation cannot make the
    // admin's personal one.
    let mut forged = server.invitation.clone();
    forged.code = old_code("tower42", Some("alice"));
    let refused = Client::join(&forged, "alice", &dir).err().unwrap();
    assert!(refused.contains("access code"), "{refused}");
    let personal = server.invitation_for("alice").code;
    drop(Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap());
    server.stop();
    // Restarted with the key file: the same personal invitation.
    let again = Server::start(&path, options()).unwrap();
    assert_eq!(again.invitation_for("alice").code, personal);
    again.stop();
    // Another key: another one; a damaged key file is refused.
    let other = Server::start(&path, local("tower42")).unwrap();
    assert_ne!(other.invitation_for("alice").code, personal);
    other.stop();
    std::fs::write(&file, "not a key").unwrap();
    assert!(
        Server::start(&path, options())
            .err()
            .unwrap()
            .contains("not a server key file")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// M2: a job failing inside the authority is refused to its client; the
/// server keeps serving everyone.
#[test]
fn a_failing_job_does_not_stop_the_server() {
    let dir = temp("panic");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    server.jobs.send(Job::Panic { reply: tx }).unwrap();
    let answer = rx.blocking_recv().unwrap();
    assert!(answer.unwrap_err().contains("internal error"));
    let id = alice.replica.line().structures[1].id.clone();
    let p = position(&alice.replica, &id);
    let p1 = [p[0] + 1.0, p[1], p[2]];
    alice.request(moved(&id, p1)).unwrap();
    // Others still join.
    let bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    assert_eq!(position(&bob.replica, &id), p1);
    drop((alice, bob));
    server.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// L5: an undo reverses objects someone else holds: refused like an
/// edit; admins pass the locks.
#[test]
fn locks_hold_undo_and_let_admins_pass() {
    let dir = temp("locks");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let mut bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    let ids: Vec<String> = bob
        .replica
        .line()
        .structures
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let (x, y) = (&ids[1], &ids[2]);
    let px = position(&bob.replica, x);
    bob.request(moved(x, [px[0] + 1.0, px[1], px[2]])).unwrap();
    // Carol, a designer, holds x: bob's undo of his move is refused.
    alice
        .request(Op::SetUser {
            name: "carol".into(),
            role: Role::Designer,
        })
        .unwrap();
    let mut carol = Client::join(&server.invitation_for("carol"), "carol", &dir).unwrap();
    let lock = |c: &mut Client, id: &str| {
        let target = crate::lock_target("main", ohl_session::Kind::Structure, id);
        c.joined
            .link
            .send(crate::ClientMsg::Lock {
                target: target.clone(),
            })
            .unwrap();
        c.pump(|m| matches!(m, ServerMsg::Locks { locks } if locks.contains_key(&target)));
    };
    lock(&mut carol, x);
    let refused = bob.request(Op::Undo).unwrap_err();
    assert!(refused.contains("being edited by carol"), "{refused}");
    // The admin passes carol's lock.
    let py = position(&alice.replica, y);
    lock(&mut carol, y);
    alice
        .request(moved(y, [py[0], py[1] + 1.0, py[2]]))
        .unwrap();
    // A large operation of the admin too (a roll-back of everything).
    alice.request(Op::RollbackAfter { seq: 1 }).unwrap();
    drop((alice, bob, carol));
    server.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// M5: settings need a designer whom no grant limits, an admin on a
/// protected line, hold no path of one computer; copies for others keep
/// only the file name of such paths.
#[test]
fn settings_need_rights_and_share_no_paths() {
    let dir = temp("settings");
    let path = project(&dir);
    let mut s = Session::open(&path).unwrap();
    let set = |value: &str| Op::SetSetting {
        key: "app.check_surface".into(),
        value: Some(value.into()),
    };
    assert!(crate::execute(&mut s, "bob", "main", &set("ground")).is_ok());
    for local in ["/Users/x/w.ohlwc", "C:\\x\\w.ohlwc", "\\\\host\\w", "~/w"] {
        let e = crate::execute(&mut s, "bob", "main", &set(local)).unwrap_err();
        assert!(e.to_string().contains("not shared"), "{e}");
    }
    // A grant limits bob: no settings.
    s.grant(
        "alice",
        &Grant {
            subject: "bob".into(),
            objects: vec!["T1".into()],
            ..Grant::default()
        },
    )
    .unwrap();
    assert!(crate::execute(&mut s, "bob", "main", &set("x")).is_err());
    // A protected line: the admin only.
    s.set_user("alice", "dan", Role::Designer).unwrap();
    s.protect_branch("alice", "main", true).unwrap();
    let e = crate::execute(&mut s, "dan", "main", &set("x")).unwrap_err();
    assert!(e.to_string().contains("protected"), "{e}");
    assert!(crate::execute(&mut s, "alice", "main", &set("x")).is_ok());
    // The weather file used last: its name only in a copy.
    s.set_setting("app.weather", Some("/Users/someone/cases.ohlwc"))
        .unwrap();
    let copy = dir.join("copy.ohl");
    s.copy_for_others(&copy).unwrap();
    let c = Session::open(&copy).unwrap();
    assert_eq!(
        c.setting("app.weather").unwrap().as_deref(),
        Some("cases.ohlwc")
    );
    assert_eq!(
        c.setting("app.check_surface").unwrap().as_deref(),
        Some("x")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// L8: names with invisible characters, and names looking like a user's,
/// are refused to newcomers.
#[test]
fn look_alike_names_are_refused() {
    for bad in ["bob\u{200b}", "\u{202e}nimda", "a\u{feff}"] {
        assert!(crate::check_name(bad).is_err(), "{bad:?}");
    }
    assert!(crate::check_name("José Müller").is_ok());
    let users = ["admin", "bob", "Anna"];
    for bad in ["Аdmin", "ADMIN", "b0b", "аnna", "ａｄｍｉｎ", "a d m i n"] {
        assert!(crate::check_look_alike(bad, users).is_err(), "{bad} passes");
    }
    for good in ["admin", "bobby", "carol", "Анна"] {
        assert!(crate::check_look_alike(good, users).is_ok(), "{good}");
    }
    let dir = temp("names");
    let path = project(&dir);
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let refused = Client::join(&server.invitation, "аlice", &dir)
        .err()
        .unwrap();
    assert!(refused.contains("looks like"), "{refused}");
    server.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
