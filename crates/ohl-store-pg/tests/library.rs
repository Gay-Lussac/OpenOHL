// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Country libraries on a throwaway cluster (`common`, phase 9): the
//! import's mapping and use index, locks and who may change a locked
//! kind, requests and their drafts (who sees them, where a draft may be
//! used, what it blocks), grants, releases, Update library pins (built
//! lines, stale routes), withdrawn entries, the use index through the
//! rights (no hidden entry or line), the export standing alone, and
//! schema 4 brought to 5.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::Cluster;
use ohl_session::rights::{NoModels, Scope, Subject, UserRights};
use ohl_session::{Role, Session};
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::library::{
    LibraryCommand as L, RightsRoles, apply_pins, check_pin_rights, plan_pins,
};
use ohl_store_pg::rights::RightsCommand as R;

const DRAKE: &str = "acsr-drake-26-7";

fn cables_file() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables/acsr-examples.ohlw"),
    )
    .unwrap()
}

/// The example line with its wires' library file and a weather file.
fn project(dir: &Path) -> PathBuf {
    let path = dir.join("lib.ohl");
    let mut s = Session::open(&path).unwrap();
    let line = ohl_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    s.import(&line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s.put_library_file("ana", "cables", "acsr-examples.ohlw", &cables_file())
        .unwrap();
    s.put_library_file(
        "ana",
        "weather",
        "cases.ohlwc",
        "[[case]]\nname = \"cold\"\ntemperature = \"-20 °C\"\n",
    )
    .unwrap();
    s.checkpoint().unwrap();
    path
}

fn rights(net: &Network, user: &str) -> UserRights {
    net.rights().unwrap().compile(user, &NoModels)
}

fn session(net: &Network, user: &str) -> Session {
    let mut s = Session::open_on(Box::new(net.store("A").unwrap()), "A").unwrap();
    s.set_role_source(Some(Arc::new(RightsRoles {
        line: "A".into(),
        rights: rights(net, user),
    })));
    s
}

fn assign(net: &Network, subject: &str, role: Option<&str>, deny: bool, scope: Scope) {
    net.rights_command(
        "zoe",
        &R::Assign {
            subject: Subject::parse(subject).unwrap(),
            role: role.map(str::to_owned),
            deny,
            scope,
            note: String::new(),
        },
    )
    .unwrap();
}

fn of_kind(kind: &str) -> Scope {
    Scope {
        library_kind: Some(kind.into()),
        ..Scope::default()
    }
}

#[test]
fn country_libraries_through_the_rights() {
    let Some(cluster) = Cluster::start("library") else {
        return;
    };
    cluster.create_db("net");
    let dir = cluster.scratch();
    let file = project(&dir);
    let net = Network::connect_writer(&cluster.url("net")).unwrap();
    assert_eq!(net.schema().unwrap(), ohl_store_pg::schema::SCHEMA);

    // The import: the file's entries added to the network's library (an
    // unlocked kind, the network empty), the use index written.
    let report = import(&net, &file, &ImportOptions::new("A", "ana")).unwrap();
    let added: Vec<&str> = report
        .libraries
        .iter()
        .filter(|m| m.outcome == "added")
        .map(|m| m.entry.as_str())
        .collect();
    assert!(added.contains(&DRAKE), "{:?}", report.libraries);
    assert!(added.contains(&"cold"), "{:?}", report.libraries);
    assert!(report.uses >= 4, "{}", report.uses);
    assert!(report.text().contains("library entries mapped"));

    // People: zoe network admin; mia and max maintain the wires; dan
    // designs on the whole network; eve sees line A; ben designs line A
    // (from the import).
    net.rights_command("zoe", &R::Bootstrap { user: "zoe".into() })
        .unwrap();
    for u in ["mia", "max", "dan", "eve", "liz", "rob", "ria", "kim"] {
        net.rights_command(
            "zoe",
            &R::AddUser {
                name: u.into(),
                display: None,
            },
        )
        .unwrap();
    }
    assign(
        &net,
        "user:mia",
        Some("library_maintainer"),
        false,
        of_kind("cables"),
    );
    assign(
        &net,
        "user:max",
        Some("library_maintainer"),
        false,
        of_kind("cables"),
    );
    assign(&net, "user:dan", Some("designer"), false, Scope::network());
    assign(&net, "user:eve", Some("viewer"), false, Scope::line("A"));
    let cmd = |actor: &str, c: L| net.library_command(actor, &c).map_err(|e| e.to_string());

    // Locked: only a network admin's emergency changes the file; a
    // maintainer may lock, not unlock.
    let e = cmd(
        "dan",
        L::Lock {
            kind: "cables".into(),
            file: None,
            locked: true,
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("may not lock"), "{e}");
    cmd(
        "mia",
        L::Lock {
            kind: "cables".into(),
            file: None,
            locked: true,
            note: "the network's wires".into(),
        },
    )
    .unwrap();
    let e = cmd(
        "mia",
        L::Lock {
            kind: "cables".into(),
            file: None,
            locked: false,
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("network admins only"), "{e}");
    let changed = cables_file().replace("1.108 in", "1.109 in");
    for who in ["dan", "ben", "mia", "max"] {
        let e = cmd(
            who,
            L::PutFile {
                kind: "cables".into(),
                file: "acsr-examples.ohlw".into(),
                text: changed.clone(),
                emergency: true,
            },
        )
        .unwrap_err();
        assert!(e.contains("locked"), "{who}: {e}");
    }
    // An unlocked kind: a designer of the network may; a line's designer
    // is not the network's.
    cmd(
        "dan",
        L::PutFile {
            kind: "weather".into(),
            file: "north.ohlwc".into(),
            text: "[[case]]\nname = \"wind\"\n".into(),
            emergency: false,
        },
    )
    .unwrap();
    let e = cmd(
        "ben",
        L::PutFile {
            kind: "weather".into(),
            file: "south.ohlwc".into(),
            text: "[[case]]\nname = \"wind\"\n".into(),
            emergency: false,
        },
    )
    .unwrap_err();
    assert!(e.contains("may not"), "{e}");
    // An entry named once per kind (wires).
    let e = cmd(
        "zoe",
        L::PutFile {
            kind: "cables".into(),
            file: "copy.ohlw".into(),
            text: cables_file(),
            emergency: true,
        },
    )
    .unwrap_err();
    assert!(e.contains("named once"), "{e}");

    // Releases by maintainers only.
    let release = |who: &str, tag: &str| {
        cmd(
            who,
            L::Release {
                kind: "cables".into(),
                tag: tag.into(),
                notes: String::new(),
            },
        )
    };
    assert!(
        release("dan", "R1")
            .unwrap_err()
            .contains("not a maintainer")
    );
    assert!(
        release("mia", "R1")
            .unwrap()
            .text
            .contains("no line changed")
    );
    assert!(release("mia", "R1b").unwrap_err().contains("not changed"));

    // A request for an addition, its draft used in ben's own branch.
    {
        // The model's own roles (from the import): ben designs, ana admin.
        let mut s = Session::open_on(Box::new(net.store("A").unwrap()), "A").unwrap();
        s.create_branch("ben", "b1", None).unwrap();
        s.create_branch("ana", "a1", None).unwrap();
    }
    let new_cable = "format = \"ohl-wire\"\nversion = 1\n[[cable]]\nid = \"new-cable\"\n\
                     name = \"N\"\nkind = \"aaac\"\ndiameter = \"20 mm\"\n";
    let ask = |who: &str, branch: Option<&str>, draft: bool| {
        cmd(
            who,
            L::Request {
                kind: "cables".into(),
                file: "acsr-examples.ohlw".into(),
                text: new_cable.into(),
                entry: None,
                reason: "a new wire for line A".into(),
                line: Some("A".into()),
                branch: branch.map(str::to_owned),
                routes: vec!["main".into()],
                draft,
            },
        )
    };
    assert!(
        ask("ben", Some("main"), false)
            .unwrap_err()
            .contains("never in main")
    );
    assert!(ask("ben", Some("a1"), false).unwrap_err().contains("own"));
    assert!(
        ask("eve", None, false)
            .unwrap_err()
            .contains("may not request")
    );
    let done = net
        .library_command(
            "ben",
            &L::Request {
                kind: "cables".into(),
                file: "acsr-examples.ohlw".into(),
                text: new_cable.into(),
                entry: None,
                reason: "a new wire for line A".into(),
                line: Some("A".into()),
                branch: Some("b1".into()),
                routes: vec!["main".into()],
                draft: false,
            },
        )
        .unwrap();
    let id = done.request.unwrap();
    let hidden = net
        .library_command(
            "ben",
            &L::Request {
                kind: "cables".into(),
                file: "acsr-examples.ohlw".into(),
                text: new_cable.replace("new-cable", "later-cable"),
                entry: None,
                reason: "not asked yet".into(),
                line: Some("A".into()),
                branch: None,
                routes: Vec::new(),
                draft: true,
            },
        )
        .unwrap()
        .request
        .unwrap();
    // Who sees which requests: the requester all of theirs; a maintainer
    // those asked (not the draft), without the line it cannot see;
    // nobody else.
    let seen = |who: &str| -> Vec<i64> {
        net.library_requests(&rights(&net, who))
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect()
    };
    assert_eq!(seen("ben"), vec![id, hidden]);
    assert_eq!(seen("mia"), vec![id]);
    assert!(seen("eve").is_empty() && seen("dan").is_empty());
    let as_mia = net.library_requests(&rights(&net, "mia")).unwrap();
    assert_eq!(as_mia[0].line, None, "mia sees nothing of line A");
    assert!(as_mia[0].reason.contains("no access"));
    assert_eq!(
        net.library_requests(&rights(&net, "zoe")).unwrap()[0]
            .line
            .as_deref(),
        Some("A")
    );

    // The draft: in ben's branch only, with the request's text; it
    // blocks main until granted.
    let state = net.library_state().unwrap();
    let draft = state.requests[&id].draft_name();
    let text = state.requests[&id].text.clone();
    assert!(
        text.contains("new-cable") && !text.contains(DRAKE),
        "{text}"
    );
    let check = |s: &ohl_store_pg::library::LibraryState, who: &str, branch: &str, t: &str| {
        s.check_file(who, "A", branch, "cables", &draft, Some(t))
    };
    check(&state, "ben", "b1", &text).unwrap();
    assert!(
        check(&state, "ben", "main", &text)
            .unwrap_err()
            .contains("blocks the merge")
    );
    assert!(check(&state, "ana", "b1", &text).is_err());
    assert!(check(&state, "ben", "b1", "x = 1").is_err());
    assert!(
        state
            .check_file("ben", "A", "b1", "cables", "mine.ohlw", Some("x = 1"))
            .unwrap_err()
            .contains("locked")
    );
    state
        .check_file("ben", "A", "b1", "weather", "mine.ohlwc", Some("x = 1"))
        .unwrap();
    let files = vec![("cables".to_owned(), draft.clone(), text.clone())];
    assert_eq!(state.pending_drafts(&files).len(), 1);

    // Granted by another maintainer (never by its requester).
    let grant = |who: &str| {
        cmd(
            who,
            L::Grant {
                id,
                note: String::new(),
            },
        )
    };
    assert!(grant("ben").unwrap_err().contains("not a maintainer"));
    grant("mia").unwrap();
    assert!(grant("max").unwrap_err().contains("granted"));
    let state = net.library_state().unwrap();
    check(&state, "zoe", "main", &text).unwrap();
    assert!(state.pending_drafts(&files).is_empty());
    let head = net.library_files(&rights(&net, "mia"), "cables").unwrap();
    assert!(head[0].entries.iter().any(|e| e.0 == "new-cable"));
    assert!(
        release("max", "R2")
            .unwrap()
            .text
            .contains("1 line(s) in design")
    );

    // Update library pins: the line's own file becomes the release's.
    let mut s = session(&net, "ben");
    let plan = plan_pins(&net, &mut s, "A", Some(&["cables".to_owned()]), None).unwrap();
    assert_eq!(plan.kinds[0].to, "R2");
    assert_eq!(plan.kinds[0].changes.len(), 1, "{}", plan.describe());
    assert!(plan.kinds[0].used_changed.is_empty(), "{}", plan.describe());
    let a = apply_pins(&net, &mut s, &rights(&net, "ben"), &plan)
        .unwrap()
        .expect("changed");
    assert_eq!(a.objects.len(), 1);
    assert_eq!(s.log().unwrap().pop().unwrap().kind, "library_pins");
    let status = net.library_status(&rights(&net, "ben"), Some("A")).unwrap();
    let cables = status[0].kinds.iter().find(|k| k.kind == "cables").unwrap();
    assert_eq!((cables.pinned.as_deref(), cables.behind), (Some("R2"), 0));
    assert_eq!(
        net.library_state().unwrap().requests[&id]
            .released
            .as_deref(),
        Some("R2")
    );

    // A wire the line uses changed (an emergency), released: its routes
    // go out of date; the line built, only who reviews it moves it.
    cmd(
        "zoe",
        L::PutFile {
            kind: "cables".into(),
            file: "acsr-examples.ohlw".into(),
            text: s.library_files("cables").unwrap()[0]
                .1
                .replace("1.108 in", "1.109 in"),
            emergency: true,
        },
    )
    .unwrap();
    release("mia", "R3").unwrap();
    assert!(
        cmd(
            "ben",
            L::SetBuilt {
                line: "A".into(),
                built: true
            }
        )
        .unwrap_err()
        .contains("may not mark")
    );
    cmd(
        "ana",
        L::SetBuilt {
            line: "A".into(),
            built: true,
        },
    )
    .unwrap();
    let plan = plan_pins(&net, &mut s, "A", None, None).unwrap();
    let k = plan.kinds.iter().find(|k| k.kind == "cables").unwrap();
    assert_eq!(
        k.used_changed,
        vec![DRAKE.to_owned()],
        "{}",
        plan.describe()
    );
    assert!(!k.stale_routes.is_empty());
    assert!(plan.built);
    let e = check_pin_rights(&rights(&net, "ben"), &plan).unwrap_err();
    assert!(e.contains("built"), "{e}");
    assert!(apply_pins(&net, &mut s, &rights(&net, "ben"), &plan).is_err());
    drop(s);
    let mut s = session(&net, "ana");
    let plan = plan_pins(&net, &mut s, "A", Some(&["cables".to_owned()]), None).unwrap();
    apply_pins(&net, &mut s, &rights(&net, "ana"), &plan)
        .unwrap()
        .expect("changed");
    let marks = s.stale_routes().unwrap();
    assert!(
        marks
            .values()
            .any(|m| m.notices.iter().any(|n| n.what.contains("cables → R3"))),
        "{marks:?}"
    );
    // A release changes no line: R4 made, the line stays on R3.
    cmd(
        "zoe",
        L::PutFile {
            kind: "cables".into(),
            file: "acsr-examples.ohlw".into(),
            text: s.library_files("cables").unwrap()[0]
                .1
                .replace("1.109 in", "1.110 in"),
            emergency: true,
        },
    )
    .unwrap();
    release("mia", "R4").unwrap();
    let before = s.library_files("cables").unwrap();
    drop(s);
    let s = session(&net, "ana");
    assert_eq!(s.library_files("cables").unwrap(), before);
    let status = net.library_status(&rights(&net, "ana"), Some("A")).unwrap();
    let cables = status[0].kinds.iter().find(|k| k.kind == "cables").unwrap();
    assert_eq!((cables.pinned.as_deref(), cables.behind), (Some("R3"), 1));
    drop(s);

    // Withdrawn: still the line's, shown; a maintainer's only.
    let withdraw = |who: &str| {
        cmd(
            who,
            L::WithdrawEntry {
                kind: "cables".into(),
                file: "acsr-examples.ohlw".into(),
                entry: DRAKE.into(),
                note: "out of production".into(),
            },
        )
    };
    assert!(withdraw("dan").is_err());
    assert!(withdraw("mia").unwrap().text.contains("keep it"));
    let status = net.library_status(&rights(&net, "ana"), Some("A")).unwrap();
    let cables = status[0].kinds.iter().find(|k| k.kind == "cables").unwrap();
    assert_eq!(cables.withdrawn_used, vec![DRAKE.to_owned()]);
    let state = net.library_state().unwrap();
    assert_eq!(state.withdrawn_in(&[("cables", DRAKE.to_owned())]).len(), 1);

    // The use index through the rights: liz sees the wires but no line —
    // no use; rob sees line A and the wires — every use; ria a route —
    // its uses only, and only that route named; kim the structures of
    // line A — no section; a hidden entry is not found, in the same words
    // as an entry that is not there.
    assign(&net, "user:liz", Some("viewer"), false, of_kind("cables"));
    assign(&net, "user:rob", Some("viewer"), false, of_kind("cables"));
    assign(&net, "user:rob", Some("viewer"), false, Scope::line("A"));
    assign(&net, "user:ria", Some("viewer"), false, of_kind("cables"));
    assign(
        &net,
        "user:ria",
        Some("viewer"),
        false,
        Scope {
            route: Some("branch".into()),
            ..Scope::default()
        },
    );
    assign(&net, "user:kim", Some("viewer"), false, of_kind("cables"));
    let mut structures = Scope::line("A");
    structures.kind = Some("structure".into());
    assign(&net, "user:kim", Some("viewer"), false, structures);
    let all = net.uses(&rights(&net, "zoe"), "cables", DRAKE).unwrap();
    assert!(all.len() >= 4, "{all:?}");
    assert!(
        net.uses(&rights(&net, "liz"), "cables", DRAKE)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        net.uses(&rights(&net, "rob"), "cables", DRAKE)
            .unwrap()
            .len(),
        all.len()
    );
    let ria = net.uses(&rights(&net, "ria"), "cables", DRAKE).unwrap();
    assert!(!ria.is_empty() && ria.len() < all.len(), "{ria:?}");
    assert!(
        ria.iter().all(|u| u.routes == vec!["branch".to_owned()]),
        "{ria:?}"
    );
    assert!(
        net.uses(&rights(&net, "kim"), "cables", DRAKE)
            .unwrap()
            .is_empty()
    );
    let none = net
        .uses(&rights(&net, "rob"), "cables", "no-such-wire")
        .unwrap_err()
        .to_string();
    let mut entry = of_kind("cables");
    entry.library_file = Some("acsr-examples.ohlw".into());
    entry.library_entry = Some(DRAKE.into());
    assign(&net, "user:rob", None, true, entry);
    let hidden_entry = net
        .uses(&rights(&net, "rob"), "cables", DRAKE)
        .unwrap_err()
        .to_string();
    assert_eq!(
        hidden_entry.replace(DRAKE, "?"),
        none.replace("no-such-wire", "?")
    );
    // Nor listed in the files, nor among the kinds' withdrawn entries.
    let rob_files = net.library_files(&rights(&net, "rob"), "cables").unwrap();
    assert!(
        rob_files
            .iter()
            .all(|f| f.entries.iter().all(|e| e.0 != DRAKE))
    );
    let rob_status = net.library_status(&rights(&net, "rob"), Some("A")).unwrap();
    assert!(
        rob_status[0]
            .kinds
            .iter()
            .all(|k| k.withdrawn_used.is_empty())
    );
    // Eve sees line A but not the network's library: no entry found.
    assert!(net.uses(&rights(&net, "eve"), "cables", DRAKE).is_err());
    assert!(net.library_kinds(&rights(&net, "eve")).unwrap().is_empty());
    let kinds = net.library_kinds(&rights(&net, "mia")).unwrap();
    assert_eq!(kinds.len(), 1, "mia maintains the wires only: {kinds:?}");
    assert!(kinds[0].locked && kinds[0].maintainers.contains(&"user:mia".to_owned()));

    // The export stands alone: the pinned files in it, the pins and the
    // withdrawn entries in its meta.
    let out = dir.join("export.ohl");
    ohl_store_pg::export::export(&net, "A", &out, "zoe").unwrap();
    let f = Session::open_read_only(&out).unwrap();
    assert!(f.library_files("cables").unwrap()[0].1.contains("1.109 in"));
    let pins = f.file_meta("network.library_pins").unwrap().unwrap();
    assert!(
        pins.contains("\"cables\": \"R3\"") || pins.contains("\"cables\":\"R3\""),
        "{pins}"
    );
    let w = f.file_meta("network.library_withdrawn").unwrap().unwrap();
    assert!(w.contains(DRAKE), "{w}");

    // Every change and refusal in the audit.
    let audit = net.audit_rows().unwrap();
    for action in [
        "library.lock",
        "library.release",
        "library.grant",
        "library.pins",
    ] {
        assert!(audit.iter().any(|r| r[2] == action), "{action}");
    }
    assert!(
        audit
            .iter()
            .any(|r| r[2] == "refused" && r[3] == "library.put_file")
    );
    assert!(net.library_rev().unwrap() > 10);
}

/// A network of schema 4 (phase 7) brought to 5 when a program connects:
/// its models kept, the libraries' tables made, the revision started.
#[test]
fn schema_4_brought_to_5() {
    let Some(cluster) = Cluster::start("schema4") else {
        return;
    };
    cluster.create_db("old");
    let url = cluster.url("old");
    {
        let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        c.batch_execute(
            "CREATE TABLE net_schema (version integer PRIMARY KEY,
                 time timestamptz NOT NULL DEFAULT clock_timestamp(), note text NOT NULL)",
        )
        .unwrap();
        for (version, note, sql) in &ohl_store_pg::schema::MIGRATIONS[..4] {
            c.batch_execute(sql).unwrap();
            c.execute(
                "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
                &[version, note],
            )
            .unwrap();
        }
        c.batch_execute("INSERT INTO line (id, name) VALUES ('A', 'a')")
            .unwrap();
    }
    let net = Network::connect(&url).unwrap();
    assert_eq!(net.schema().unwrap(), 5);
    assert_eq!(net.library_rev().unwrap(), 0);
    assert_eq!(net.lines().unwrap().len(), 1);
    let s = net.library_state().unwrap();
    assert!(s.requests.is_empty() && s.locked_kinds.is_empty());
}
