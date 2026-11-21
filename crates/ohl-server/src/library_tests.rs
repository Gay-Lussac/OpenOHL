// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Country libraries on a network's server (phase 9), on a throwaway
//! PostgreSQL cluster: Update library pins planned and carried out by the
//! server (its carried-out form never a client's), a locked kind refused
//! in a line, a pending draft used only in its requester's branch and
//! blocking its review and merge until granted, a withdrawn entry kept by
//! who uses it but never newly used, the use index kept with each
//! command, a built line moved only by whoever reviews it — and the
//! libraries read again while the server runs.

use std::time::{Duration, Instant};

use ohl_session::rights::Scope;
use ohl_session::{Command, Kind, Session};
use ohl_store_pg::Network;
use ohl_store_pg::library::LibraryCommand as L;

use crate::client::{Incoming, Joined, NetJoin, join_network};
use crate::network_tests::{add_user, local, moved, net_options, test_network};
use crate::server::Server;
use crate::{ClientMsg, Op, ServerMsg};

const DRAKE: &str = "acsr-drake-26-7";

/// The shipped wire file with a second wire (a copy of the first).
fn cables() -> String {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/cables/acsr-examples.ohlw"),
    )
    .unwrap();
    let at = text.find("[[cable]]").unwrap();
    format!("{text}\n{}", text[at..].replace(DRAKE, "spare-wire"))
}

fn ask(j: &Joined, id: u64, model: &str, line: &str, op: Op) -> Result<String, String> {
    j.link
        .send(ClientMsg::RequestOn {
            id,
            model: model.into(),
            op,
            line: line.into(),
        })
        .unwrap();
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < crate::tests::WAIT, "no answer");
        match j.link.recv_timeout(Duration::from_millis(20)) {
            Some(Incoming::Msg(ServerMsg::Response { id: i, result })) if i == id => {
                return result;
            }
            Some(Incoming::Closed(why)) => return Err(format!("closed: {why}")),
            _ => {}
        }
    }
}

fn lib(net: &Network, who: &str, c: L) -> Result<String, String> {
    net.library_command(who, &c)
        .map(|d| d.text)
        .map_err(|e| e.to_string())
}

#[test]
fn country_libraries_on_the_server() {
    let Some(t) = test_network("library", 120) else {
        return;
    };
    let a = t.codes[0].clone();
    let net = Network::connect(&t.url).unwrap();
    // A maintainer of the wires; dora designs line A only.
    add_user(
        &net,
        "mia",
        "library_maintainer",
        Scope {
            library_kind: Some("cables".into()),
            ..Scope::default()
        },
    );
    add_user(&net, "dora", "designer", Scope::line(&a));
    lib(
        &net,
        "admin",
        L::PutFile {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            text: cables(),
            emergency: false,
        },
    )
    .unwrap();
    lib(
        &net,
        "mia",
        L::Release {
            kind: "cables".into(),
            tag: "R1".into(),
            notes: String::new(),
        },
    )
    .unwrap();
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let join = |who: &str| {
        join_network(
            &server.invitation_for(who),
            who,
            &NetJoin::default(),
            &mut |_| {},
        )
        .unwrap()
    };
    let bob = join("bob");
    let carol = join("carol");
    let dora = join("dora");

    // Update library pins, planned by the server; the carried-out form
    // is never a client's.
    let e = ask(
        &bob,
        1,
        &a,
        "main",
        Op::LibraryPins {
            pins: [("cables".to_owned(), "R1".to_owned())].into(),
            changes: vec![("cables".into(), "evil.ohlw".into(), Some("x = 1".into()))],
            stale: Default::default(),
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("Update library pins"), "{e}");
    ask(
        &bob,
        2,
        &a,
        "main",
        Op::UpdateLibraryPins {
            kinds: Vec::new(),
            release: None,
        },
    )
    .unwrap();
    let status = net
        .library_status(
            &net.rights()
                .unwrap()
                .compile("admin", &ohl_session::rights::NoModels),
            Some(&a),
        )
        .unwrap();
    assert_eq!(status[0].kinds[0].pinned.as_deref(), Some("R1"));
    let e = ask(
        &bob,
        3,
        &a,
        "main",
        Op::UpdateLibraryPins {
            kinds: Vec::new(),
            release: None,
        },
    )
    .unwrap_err();
    assert!(e.contains("nothing to update"), "{e}");

    // Locked while the server runs: a line's file of the kind refused.
    lib(
        &net,
        "mia",
        L::Lock {
            kind: "cables".into(),
            file: None,
            locked: true,
            note: String::new(),
        },
    )
    .unwrap();
    let e = ask(
        &bob,
        4,
        &a,
        "main",
        Op::PutLibrary {
            kind: "cables".into(),
            name: "mine.ohlw".into(),
            text: cables(),
        },
    )
    .unwrap_err();
    assert!(e.contains("locked"), "{e}");
    let e = ask(
        &bob,
        5,
        &a,
        "main",
        Op::Do {
            command: Command::Put {
                kind: Kind::Library,
                id: "cables/mine.ohlw".into(),
                value: serde_json::json!({"id": "cables/mine.ohlw", "kind": "cables",
                    "name": "mine.ohlw", "text": "x = 1"}),
                ord: None,
            },
        },
    )
    .unwrap_err();
    assert!(e.contains("locked"), "{e}");

    // A pending draft in dora's own branch: hers only; it blocks the
    // branch's review and merge until granted.
    ask(
        &dora,
        6,
        &a,
        "main",
        Op::CreateBranch {
            name: "d1".into(),
            at: None,
        },
    )
    .unwrap();
    let new_wire = "format = \"ohl-wire\"\n[[cable]]\nid = \"new-wire\"\nname = \"N\"\n";
    let id = net
        .library_command(
            "dora",
            &L::Request {
                kind: "cables".into(),
                file: "acsr.ohlw".into(),
                text: new_wire.into(),
                entry: None,
                reason: "a new wire".into(),
                line: Some(a.clone()),
                branch: Some("d1".into()),
                routes: Vec::new(),
                draft: false,
            },
        )
        .unwrap()
        .request
        .unwrap();
    let draft = format!("request-{id}.ohlw");
    let text = net.library_state().unwrap().requests[&id].text.clone();
    let put = |j: &Joined, n: u64, line: &str| {
        ask(
            j,
            n,
            &a,
            line,
            Op::PutLibrary {
                kind: "cables".into(),
                name: draft.clone(),
                text: text.clone(),
            },
        )
    };
    assert!(
        put(&dora, 7, "main")
            .unwrap_err()
            .contains("blocks the merge")
    );
    put(&dora, 8, "d1").unwrap();
    ask(
        &dora,
        80,
        &a,
        "d1",
        Op::Commit {
            message: "a new wire".into(),
        },
    )
    .unwrap();
    let e = put(&carol, 9, "d1").unwrap_err();
    assert!(e.contains("requester"), "{e}");
    let e = ask(
        &dora,
        10,
        &a,
        "main",
        Op::SubmitReview {
            branch: "d1".into(),
            target: "main".into(),
            reviewers: Vec::new(),
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("pending library drafts"), "{e}");
    let merge = |n: u64| {
        ask(
            &bob,
            n,
            &a,
            "main",
            Op::Merge {
                other: "d1".into(),
                message: None,
                choices: Default::default(),
            },
        )
    };
    assert!(merge(11).unwrap_err().contains("pending library drafts"));
    lib(
        &net,
        "mia",
        L::Grant {
            id,
            note: String::new(),
        },
    )
    .unwrap();
    // Granted: the merge passes (the server read the libraries again).
    merge(12).unwrap();
    let s = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
    assert!(
        s.library_files("cables")
            .unwrap()
            .iter()
            .any(|(n, _)| *n == draft),
        "the granted draft in main"
    );
    drop(s);

    // Withdrawn: who uses it keeps it (a move passes), nothing starts
    // using it; the use index follows the commands.
    lib(
        &net,
        "mia",
        L::WithdrawEntry {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            entry: "spare-wire".into(),
            note: String::new(),
        },
    )
    .unwrap();
    let s = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
    let st = s.line().structures[0].clone();
    let section = s.line().sections[0].id.clone();
    let mut value = (*s.object_value(Kind::Section, &section).unwrap()).clone();
    drop(s);
    ask(
        &bob,
        13,
        &a,
        "main",
        moved(
            &st.id,
            [st.position[0] + 0.3, st.position[1], st.position[2]],
        ),
    )
    .unwrap();
    value["cable"] = serde_json::Value::String("spare-wire".into());
    let put_section = |n: u64, v: serde_json::Value| {
        ask(
            &bob,
            n,
            &a,
            "main",
            Op::Do {
                command: Command::Put {
                    kind: Kind::Section,
                    id: section.clone(),
                    value: v,
                    ord: None,
                },
            },
        )
    };
    let e = put_section(14, value.clone()).unwrap_err();
    assert!(e.contains("withdrawn"), "{e}");
    let admin = net
        .rights()
        .unwrap()
        .compile("admin", &ohl_session::rights::NoModels);
    let before = net.uses(&admin, "cables", DRAKE).unwrap().len();
    lib(
        &net,
        "mia",
        L::RestoreEntry {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            entry: "spare-wire".into(),
        },
    )
    .unwrap();
    put_section(15, value).unwrap();
    let spare = net.uses(&admin, "cables", "spare-wire").unwrap();
    assert!(spare.iter().any(|u| u.object == section), "{spare:?}");
    assert_eq!(net.uses(&admin, "cables", DRAKE).unwrap().len(), before - 1);

    // Built: only whoever reviews the whole line moves its pins.
    lib(
        &net,
        "admin",
        L::PutFile {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            text: cables().replace("1.108 in", "1.109 in"),
            emergency: true,
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    lib(
        &net,
        "mia",
        L::Release {
            kind: "cables".into(),
            tag: "R2".into(),
            notes: String::new(),
        },
    )
    .unwrap();
    lib(
        &net,
        "admin",
        L::SetBuilt {
            line: a.clone(),
            built: true,
        },
    )
    .unwrap();
    let pins = Op::UpdateLibraryPins {
        kinds: vec!["cables".into()],
        release: None,
    };
    let e = ask(&dora, 16, &a, "main", pins.clone()).unwrap_err();
    assert!(e.contains("built"), "{e}");
    drop(server);
}
