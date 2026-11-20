// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The PostgreSQL store on a throwaway cluster (`common`): the store
//! suite every store passes, the same session on both stores, the
//! network's schema and writer's lock, import (renames, twin towers,
//! refusals) and the per-model export's round trip; the rights' changes
//! through the one function, their audit, and schema 3 brought to 4.

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::Cluster;
use ohl_session::store::Store;
use ohl_session::store::sqlite::SqliteStore;
use ohl_session::{Command, Kind, Role, Session, SessionError};
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::{NetError, Network, export::export};

fn example() -> ohl_line::Line {
    ohl_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// Every store of the suite in a database of its own (one network per
/// project name: the suite imports one line into two projects, which one
/// network refuses — its ids are unique), copied from a migrated
/// template.
#[test]
fn store_suite_on_postgresql() {
    let Some(cluster) = Cluster::start("suite") else {
        return;
    };
    cluster.create_db("template_ohl");
    drop(Network::connect(&cluster.url("template_ohl")).unwrap());
    let nets: RefCell<BTreeMap<String, Network>> = RefCell::new(BTreeMap::new());
    let open = |name: &str| -> Box<dyn Store> {
        let mut nets = nets.borrow_mut();
        let net = nets.entry(name.to_owned()).or_insert_with(|| {
            let db = name.replace('-', "_");
            cluster.create_db_from(&db, Some("template_ohl"));
            let net = Network::connect_writer(&cluster.url(&db)).unwrap();
            net.create_line(name, name, "").unwrap();
            net
        });
        Box::new(net.store(name).unwrap())
    };
    let failed = ohl_session::store::suite::run_all(&open);
    assert!(failed.is_empty(), "{failed:#?}");

    // The same session on both stores: the same transcript.
    let pg = ohl_session::store::suite::session_transcript(&open, "transcript");
    let dir = cluster.scratch();
    let file = |name: &str| -> Box<dyn Store> {
        Box::new(SqliteStore::open(&dir.join(format!("{name}.ohl"))).unwrap())
    };
    let sqlite = ohl_session::store::suite::session_transcript(&file, "transcript");
    assert_eq!(pg, sqlite);
    drop(nets);
}

#[test]
fn schema_and_the_writers_lock() {
    let Some(cluster) = Cluster::start("lock") else {
        return;
    };
    cluster.create_db("net");
    let url = cluster.url("net");
    let first = Network::connect_writer(&url).unwrap();
    assert_eq!(first.schema().unwrap(), ohl_store_pg::schema::SCHEMA);
    assert_eq!(
        first.meta("schema").unwrap(),
        Some(ohl_store_pg::schema::SCHEMA.to_string())
    );
    // One writer.
    let second = Network::connect_writer(&url).unwrap_err();
    assert!(
        matches!(&second, NetError::Refused(m) if m.contains("writer's lock")),
        "{second}"
    );
    // Readers may connect; they may not write.
    let reader = Network::connect(&url).unwrap();
    assert!(!reader.is_writer());
    assert!(reader.create_line("A", "a", "").is_err());
    // The lock stays while a model's store still uses the writer's
    // connection, and is released explicitly when its last user goes: a
    // writer connecting right after (a server restarted) is not refused,
    // without waiting for PostgreSQL's process of the old connection to
    // end. Tried several times in a row.
    first.create_line("W", "w", "").unwrap();
    let store = first.store("W").unwrap();
    drop(first);
    assert!(Network::connect_writer(&url).is_err());
    drop(store);
    for _ in 0..5 {
        let w = Network::connect_writer(&url).unwrap();
        assert!(w.is_writer());
        drop(w);
    }
    // A writer dropped in the middle of a step (a transaction open) lets
    // its lock go as well.
    let mid = Network::connect_writer(&url).unwrap();
    {
        let s = mid.store("W").unwrap();
        s.savepoint("half").unwrap();
        drop(mid);
        drop(s);
    }
    let again = Network::connect_writer(&url).unwrap();
    // Migrations run once.
    assert_eq!(again.schema().unwrap(), ohl_store_pg::schema::SCHEMA);
    // A newer database is refused before anything changes.
    let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    c.execute(
        "INSERT INTO net_schema (version, note) VALUES (99, 'from the future')",
        &[],
    )
    .unwrap();
    let newer = Network::connect(&url).unwrap_err();
    assert!(
        matches!(&newer, NetError::Refused(m) if m.contains("newer")),
        "{newer}"
    );
    // The audit is append only.
    again.audit("ana", "test", "net", "").unwrap();
    assert!(c.execute("DELETE FROM audit", &[]).is_err());
    assert!(c.execute("UPDATE audit SET detail = 'x'", &[]).is_err());
    // Model codes: short and plain.
    assert!(again.create_line("A B", "x", "").is_err());
    assert!(again.create_line("", "x", "").is_err());
    again.create_line("L-1.a", "x", "").unwrap();
    assert!(again.create_line("L-1.a", "x", "").is_err());
    // Working sets saved by name (schema 3): replaced by name, per owner,
    // refused without a name, a known model or picks.
    use ohl_session::working_set::{Pick, WorkingSetSpec};
    let spec = WorkingSetSpec {
        name: "north part".into(),
        model: "L-1.a".into(),
        picks: vec![
            Pick::Route { route: "r1".into() },
            Pick::Stations {
                route: "r2".into(),
                alignment: None,
                from: 120.25,
                to: 980.5,
            },
        ],
    };
    // The audit's test above left the network refused for newer schemas:
    // a fresh database for the rest.
    cluster.create_db("ws");
    let ws = Network::connect_writer(&cluster.url("ws")).unwrap();
    ws.create_line("L-1.a", "x", "").unwrap();
    ws.save_working_set("ana", &spec, &["r1".into(), "r2".into()])
        .unwrap();
    let mut other = spec.clone();
    other.picks.pop();
    ws.save_working_set("ana", &other, &["r1".into()]).unwrap();
    ws.save_working_set("ben", &spec, &[]).unwrap();
    let saved = ws.working_sets("ana").unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].spec, other, "replaced by name");
    assert_eq!(saved[0].routes, ["r1"]);
    assert_eq!(
        ws.working_sets("ben").unwrap()[0].spec,
        spec,
        "stations kept exactly"
    );
    for bad in [
        WorkingSetSpec {
            name: " ".into(),
            ..spec.clone()
        },
        WorkingSetSpec {
            model: "nowhere".into(),
            ..spec.clone()
        },
        WorkingSetSpec {
            picks: Vec::new(),
            ..spec.clone()
        },
    ] {
        assert!(ws.save_working_set("ana", &bad, &[]).is_err(), "{bad:?}");
    }
    assert!(ws.delete_working_set("ana", "north part").unwrap());
    assert!(!ws.delete_working_set("ana", "north part").unwrap());
    assert!(ws.working_sets("ana").unwrap().is_empty());
    // A database of schema 2 with a working set of routes only is brought
    // forward: it reads as those routes picked whole.
    cluster.create_db("old");
    let mut c = postgres::Client::connect(&cluster.url("old"), postgres::NoTls).unwrap();
    c.batch_execute(
        "CREATE TABLE net_schema (version integer PRIMARY KEY,
             time timestamptz NOT NULL DEFAULT clock_timestamp(), note text NOT NULL)",
    )
    .unwrap();
    for (version, note, sql) in &ohl_store_pg::schema::MIGRATIONS[..2] {
        c.batch_execute(sql).unwrap();
        c.execute(
            "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
            &[version, note],
        )
        .unwrap();
    }
    c.batch_execute(
        "INSERT INTO working_set (owner, name, routes) VALUES ('ana', 'old', '[\"r1\"]')",
    )
    .unwrap();
    let old = Network::connect(&cluster.url("old")).unwrap();
    assert_eq!(old.schema().unwrap(), ohl_store_pg::schema::SCHEMA);
    let saved = old.working_sets("ana").unwrap();
    assert_eq!(saved[0].spec.picks, [Pick::Route { route: "r1".into() }]);
    assert_eq!(saved[0].spec.model, "");
}

/// The project file used by the round trip: the example line with a
/// second user, edits, a commit, a branch with an edit of its own, a tag,
/// a library file, a setting and an edit grant; left on its main line.
fn project(dir: &Path) -> PathBuf {
    let path = dir.join("example.ohl");
    let mut s = Session::open(&path).unwrap();
    s.import(&example(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    let p = s.line().structure("T6").unwrap().position;
    s.apply(
        "ben",
        Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 2.0, p[1], p[2]],
        },
    )
    .unwrap();
    s.commit("ana", "first").unwrap();
    s.create_branch("ana", "phase 1", None).unwrap();
    s.select_line("phase 1").unwrap();
    s.apply(
        "ben",
        Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 9.0, p[1], p[2]],
        },
    )
    .unwrap();
    s.select_line("main").unwrap();
    s.tag("ana", "issued", None, "").unwrap();
    s.put_library_file("ana", "weather", "cases", "[cases]\n")
        .unwrap();
    s.set_setting("units", Some("metric")).unwrap();
    s.grant(
        "ana",
        &ohl_session::collab::Grant {
            subject: "ben".into(),
            kinds: vec!["structure".into()],
            objects: Vec::new(),
            area: None,
            note: "structures".into(),
        },
    )
    .unwrap();
    s.checkpoint().unwrap();
    path
}

/// The rows `sql` selects from a project file, as text, sorted (the file
/// read as immutable: nothing made beside it).
fn dump(path: &Path, sql: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(
        format!("file:{}?mode=ro&immutable=1", path.display()),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    let mut out: Vec<String> = stmt
        .query_map([], |r| {
            let mut row = Vec::new();
            for k in 0..n {
                let v: rusqlite::types::Value = r.get(k)?;
                row.push(format!("{v:?}"));
            }
            Ok(row.join("|"))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    out.sort();
    out
}

const MAIN_OBJECTS: &str =
    "SELECT kind, id, ord, version, json, gone FROM model_object WHERE line = 'main'";
const MAIN_INDEX: &str =
    "SELECT kind, id, level, tx, ty, xmin, ymin, xmax, ymax FROM spatial_index WHERE line = 'main'";
const META: &str = "SELECT key, value FROM ols_meta WHERE key NOT LIKE 'network.%'";

#[test]
fn import_and_export_round_trip() {
    let Some(cluster) = Cluster::start("roundtrip") else {
        return;
    };
    cluster.create_db("net");
    let dir = cluster.scratch();
    let file = project(&dir);
    let net = Network::connect_writer(&cluster.url("net")).unwrap();

    // Imported as A: nothing renamed, no twin; the history stays in the
    // file.
    let a = import(&net, &file, &ImportOptions::new("A", "ana")).unwrap();
    assert!(a.renamed.is_empty(), "{:?}", a.renamed);
    assert!(a.twins.is_empty());
    assert_eq!(a.objects["structure"], example().structures.len());
    assert!(a.revision.is_some());
    assert!(
        a.left_in_file
            .iter()
            .any(|l| l.contains("other project lines")),
        "{:?}",
        a.left_in_file
    );
    assert!(a.left_in_file.iter().any(|l| l.contains("tags")));
    assert!(a.html().contains("Twin towers (0)"));
    assert_eq!(
        net.meta("crs").unwrap().as_deref(),
        Some(example().crs.as_str())
    );
    // One log entry for the import.
    let s = Session::open_on(Box::new(net.store("A").unwrap()), "A").unwrap();
    let log = s.log().unwrap();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].kind, "import");
    assert!(log[0].note.as_deref().unwrap().contains("example.ohl"));
    let file_line = Session::open_read_only(&file).unwrap().export();
    assert_eq!(s.export(), file_line);
    drop(s);

    // Exported back: the same objects, index and meta rows (the round
    // trip, rows compared).
    let back = dir.join("A.ohl");
    let e = export(&net, "A", &back, "ana").unwrap();
    assert_eq!(e.structures, example().structures.len());
    assert_eq!(dump(&file, MAIN_OBJECTS), dump(&back, MAIN_OBJECTS));
    assert_eq!(dump(&file, MAIN_INDEX), dump(&back, MAIN_INDEX));
    assert_eq!(dump(&file, META), dump(&back, META));
    assert_eq!(
        dump(&file, "SELECT name, role FROM session_user"),
        dump(&back, "SELECT name, role FROM session_user")
    );
    assert_eq!(
        dump(
            &file,
            "SELECT subject, kinds, objects, area FROM edit_grant"
        ),
        dump(
            &back,
            "SELECT subject, kinds, objects, area FROM edit_grant"
        )
    );
    let network_keys = dump(&back, "SELECT key FROM ols_meta WHERE key LIKE 'network.%'");
    assert_eq!(network_keys.len(), 5, "{network_keys:?}");
    assert_eq!(Session::open_read_only(&back).unwrap().export(), file_line);
    // An export does not overwrite.
    assert!(export(&net, "A", &back, "ana").is_err());

    // Edited through the network: the export follows.
    {
        let mut s = Session::open_on(Box::new(net.store("A").unwrap()), "A").unwrap();
        let p = s.line().structure("T2").unwrap().position;
        s.apply(
            "ben",
            Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 1.0, p[1], p[2]],
            },
        )
        .unwrap();
        s.undo("ben").unwrap();
        s.redo("ben").unwrap();
        let after = dir.join("A-edited.ohl");
        export(&net, "A", &after, "ana").unwrap();
        let exported = Session::open_read_only(&after).unwrap();
        assert_eq!(exported.export(), s.export());
        assert_eq!(exported.log().unwrap().len(), 4, "import, do, undo, redo");
    }

    // The same file again as B: every element's id is A's — renamed with
    // B's code; every structure a twin of A's (listed, not merged) but T2,
    // which A moved by 1 m (beyond the 0.5 m tolerance).
    let b = import(&net, &file, &ImportOptions::new("B", "ana")).unwrap();
    let elements: usize = [
        "route",
        "alignment",
        "alignment_point",
        "structure",
        "section",
        "jumper",
        "spacer",
    ]
    .iter()
    .map(|k| a.objects.get(*k).copied().unwrap_or(0))
    .sum();
    assert_eq!(b.renamed.len(), elements);
    assert!(b.renamed.iter().all(|r| r.to == format!("B-{}", r.from)));
    let structures = example().structures.len();
    assert_eq!(b.twins.len(), structures - 1, "{:?}", b.twins);
    assert!(b.twins.iter().all(|t| t.id_in_file != "T2"));
    assert!(b.twins.iter().all(|t| t.other_line == "A"
        && t.structure == format!("B-{}", t.id_in_file)
        && t.distance_m < 1e-9));
    let html = b.html();
    assert!(html.contains("B-T6") && html.contains("Twin towers"));
    // Both lines keep their structures.
    assert_eq!(
        Session::open_on(Box::new(net.store("A").unwrap()), "A")
            .unwrap()
            .line()
            .structures
            .len(),
        structures
    );
    // B's ids mapped back: the file's line.
    let b_file = dir.join("B.ohl");
    export(&net, "B", &b_file, "ana").unwrap();
    let mut mapped =
        Session::open_on(Box::new(SqliteStore::copy_in_memory(&b_file).unwrap()), "B").unwrap();
    for r in &b.renamed {
        mapped
            .apply(
                "ana",
                Command::Rename {
                    kind: Kind::parse(&r.kind).unwrap(),
                    id: r.to.clone(),
                    new_id: r.from.clone(),
                },
            )
            .unwrap();
    }
    assert_eq!(mapped.export(), file_line);
    // Log numbers run on across the network.
    let a_last = Session::open_on(Box::new(net.store("A").unwrap()), "A")
        .unwrap()
        .last_seq()
        .unwrap();
    let b_log = Session::open_on(Box::new(net.store("B").unwrap()), "B")
        .unwrap()
        .log()
        .unwrap();
    assert!(b_log[0].seq > a_last);

    // Ids unique in the network: B cannot make one of A's.
    let mut s = Session::open_on(Box::new(net.store("B").unwrap()), "B").unwrap();
    let before = s.export();
    let mut t = s.line().structure("B-T2").unwrap().clone();
    t.id = "T2".into();
    let err = s
        .apply(
            "ana",
            Command::Put {
                kind: Kind::Structure,
                id: "T2".into(),
                value: serde_json::to_value(&t).unwrap(),
                ord: None,
            },
        )
        .unwrap_err();
    assert!(
        matches!(&err, SessionError::Refused(m) if m.contains("unique in the network")),
        "{err}"
    );
    assert_eq!(s.export(), before);
    // The session goes on after the refusal.
    let p = s.line().structure("B-T2").unwrap().position;
    s.apply(
        "ana",
        Command::MoveStructure {
            id: "B-T2".into(),
            position: [p[0] + 1.0, p[1], p[2]],
        },
    )
    .unwrap();
    drop(s);

    // Refused: a code taken, another coordinate system, a part.
    assert!(import(&net, &file, &ImportOptions::new("A", "ana")).is_err());
    let mut other = example();
    other.crs = "another grid".into();
    let other_file = dir.join("other.ohl");
    Session::open(&other_file)
        .unwrap()
        .import(&other, "ana")
        .unwrap();
    let err = import(&net, &other_file, &ImportOptions::new("C", "ana")).unwrap_err();
    assert!(
        matches!(&err, NetError::Refused(m) if m.contains("reproject")),
        "{err}"
    );
    let part_file = dir.join("part.ohl");
    Session::open_read_only(&file)
        .unwrap()
        .write_part(
            &[(Kind::Structure, "T2".to_owned())].into_iter().collect(),
            &part_file,
            "ana",
        )
        .unwrap();
    let err = import(&net, &part_file, &ImportOptions::new("D", "ana")).unwrap_err();
    assert!(
        matches!(&err, NetError::Refused(m) if m.contains("part")),
        "{err}"
    );
    // Nothing of the refused imports stayed.
    let lines: Vec<String> = net.lines().unwrap().into_iter().map(|l| l.id).collect();
    assert_eq!(lines, vec!["A".to_owned(), "B".to_owned()]);
    // In the audit: two imports, four exports.
    let audit = net.audit_rows().unwrap();
    let actions: Vec<&str> = audit.iter().map(|r| r[2].as_str()).collect();
    assert_eq!(
        actions,
        vec!["import", "export", "export", "import", "export"]
    );
    // The file is the archive: unchanged by the imports, nothing made
    // beside it.
    assert_eq!(Session::open_read_only(&file).unwrap().export(), file_line);
    for end in ["-wal", "-shm"] {
        let mut beside = file.as_os_str().to_owned();
        beside.push(end);
        assert!(!Path::new(&beside).exists(), "{end} beside the file");
    }
}

/// Phase 7: rights changes go through the rights' one function — the
/// first network admin by bootstrap only; users, groups and roles by
/// network admins; an assignment by who manages its scope and holds its
/// role's permissions there; a deny by network admins only; the last
/// network admin kept; route and object scopes get their line; a region
/// read back exactly; every change and refusal in the audit; the
/// revision moved on by each change.
#[test]
fn rights_changes_through_the_one_function() {
    use ohl_session::rights::{NoModels, Permission, Perms, SHIPPED_ROLES, Scope, Subject};
    use ohl_store_pg::rights::RightsCommand as R;
    let Some(cluster) = Cluster::start("rights") else {
        return;
    };
    cluster.create_db("net");
    let dir = cluster.scratch();
    let file = project(&dir);
    let net = Network::connect_writer(&cluster.url("net")).unwrap();
    import(&net, &file, &ImportOptions::new("A", "ana")).unwrap();
    let d = net.rights().unwrap();
    // The shipped roles as the session knows them; the import's users.
    for (name, perms) in SHIPPED_ROLES {
        assert_eq!(d.roles[name], (Perms::of(perms), true), "{name}");
    }
    assert!(d.users.contains_key("ana") && d.users.contains_key("ben"));
    assert!(d.rev > 0, "the import moved the rights on");
    let route = Session::open_on(Box::new(net.store("A").unwrap()), "A")
        .unwrap()
        .line()
        .routes[0]
        .id
        .clone();
    let no = NoModels;
    let ok = |actor: &str, c: R| net.rights_command(actor, &c).map_err(|e| e.to_string());
    let assign = |subject: &str, role: Option<&str>, deny: bool, scope: Scope| R::Assign {
        subject: Subject::parse(subject).unwrap(),
        role: role.map(str::to_owned),
        deny,
        scope,
        note: String::new(),
    };
    // Users only by network admins: none yet, so bootstrap first, once.
    let e = ok(
        "ana",
        R::AddUser {
            name: "cid".into(),
            display: None,
        },
    )
    .unwrap_err();
    assert!(e.contains("network admins only"), "{e}");
    ok("zoe", R::Bootstrap { user: "zoe".into() }).unwrap();
    let e = ok("ana", R::Bootstrap { user: "ana".into() }).unwrap_err();
    assert!(e.contains("has a network admin"), "{e}");
    let rev = net.rights_rev().unwrap();
    ok(
        "zoe",
        R::AddUser {
            name: "cid".into(),
            display: Some("Cid".into()),
        },
    )
    .unwrap();
    assert_eq!(net.rights_rev().unwrap(), rev + 1);
    ok(
        "zoe",
        R::AddGroup {
            name: "crew".into(),
            note: String::new(),
        },
    )
    .unwrap();
    ok(
        "zoe",
        R::AddMember {
            group: "crew".into(),
            user: "cid".into(),
        },
    )
    .unwrap();
    assert!(
        ok(
            "zoe",
            R::AddMember {
                group: "crew".into(),
                user: "nobody".into()
            }
        )
        .is_err()
    );
    assert!(
        ok(
            "zoe",
            R::AddUser {
                name: "a:b".into(),
                display: None
            }
        )
        .is_err()
    );
    // Roles: not a shipped one; not removed while given.
    let e = ok(
        "zoe",
        R::PutRole {
            name: "viewer".into(),
            permissions: vec![Permission::Edit],
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("shipped"), "{e}");
    ok(
        "zoe",
        R::PutRole {
            name: "keeper".into(),
            permissions: vec![Permission::See, Permission::ManageRights],
            note: String::new(),
        },
    )
    .unwrap();
    // A route scope gets its line; one not in the network is refused.
    let done = ok(
        "zoe",
        assign(
            "group:crew",
            Some("viewer"),
            false,
            Scope {
                route: Some(route.clone()),
                ..Scope::default()
            },
        ),
    )
    .unwrap();
    assert!(done.contains("line A"), "{done}");
    let e = ok(
        "zoe",
        assign(
            "group:crew",
            Some("viewer"),
            false,
            Scope {
                route: Some("nowhere".into()),
                ..Scope::default()
            },
        ),
    )
    .unwrap_err();
    assert!(e.contains("no route"), "{e}");
    // The line admin (ana, from the import) gives on her line, not
    // beyond; no deny.
    ok(
        "ana",
        assign("user:ben", Some("checker"), false, Scope::line("A")),
    )
    .unwrap();
    let e = ok(
        "ana",
        assign("user:ben", Some("viewer"), false, Scope::network()),
    )
    .unwrap_err();
    assert!(e.contains("does not manage"), "{e}");
    let e = ok("ana", assign("user:ben", None, true, Scope::line("A"))).unwrap_err();
    assert!(e.contains("network admins only"), "{e}");
    // Who holds see and manage_rights only gives no more than that.
    ok(
        "zoe",
        assign("user:cid", Some("keeper"), false, Scope::line("A")),
    )
    .unwrap();
    let e = ok(
        "cid",
        assign("user:ben", Some("designer"), false, Scope::line("A")),
    )
    .unwrap_err();
    assert!(e.contains("may give only"), "{e}");
    ok(
        "cid",
        assign("user:ben", Some("viewer"), false, Scope::line("A")),
    )
    .unwrap();
    let e = ok(
        "zoe",
        R::DeleteRole {
            name: "keeper".into(),
        },
    )
    .unwrap_err();
    assert!(e.contains("given by"), "{e}");
    // A region read back exactly; a deny by the network admin.
    let region = vec![[0.1, 0.2], [1000.3, 0.4], [1000.5, 900.6]];
    ok(
        "zoe",
        assign(
            "user:ben",
            None,
            true,
            Scope {
                region: Some(region.clone()),
                ..Scope::default()
            },
        ),
    )
    .unwrap();
    let d = net.rights().unwrap();
    let back = d.assignments.iter().find(|a| a.deny).unwrap();
    assert_eq!(back.scope.region.as_ref(), Some(&region));
    // The last network admin stays.
    let zoe_admin = d
        .assignments
        .iter()
        .find(|a| a.role.as_deref() == Some("network_admin"))
        .unwrap()
        .id;
    let e = ok("zoe", R::Unassign { id: zoe_admin }).unwrap_err();
    assert!(e.contains("last network admin"), "{e}");
    let e = ok(
        "zoe",
        R::SetActive {
            name: "zoe".into(),
            active: false,
        },
    )
    .unwrap_err();
    assert!(e.contains("last network admin"), "{e}");
    // The audit: each change with what it was and is, each refusal.
    let rows = net.audit_rows().unwrap();
    for action in [
        "rights.bootstrap",
        "rights.add_user",
        "rights.assign",
        "rights.put_role",
    ] {
        assert!(rows.iter().any(|r| r[2] == action), "{action}");
    }
    assert!(rows.iter().filter(|r| r[2] == "refused").count() >= 8);
    let after: Option<String> = {
        let mut c = postgres::Client::connect(&cluster.url("net"), postgres::NoTls).unwrap();
        c.query_one(
            "SELECT after::text FROM audit WHERE action = 'rights.assign' ORDER BY id DESC LIMIT 1",
            &[],
        )
        .unwrap()
        .get(0)
    };
    assert!(after.unwrap().contains("region"));
    // A disabled user has no rights; a removed group takes its
    // assignments.
    ok(
        "zoe",
        R::SetActive {
            name: "ben".into(),
            active: false,
        },
    )
    .unwrap();
    let d = net.rights().unwrap();
    assert!(!d.compile("ben", &no).active());
    ok(
        "zoe",
        R::DeleteGroup {
            name: "crew".into(),
        },
    )
    .unwrap();
    let d = net.rights().unwrap();
    assert!(
        d.assignments
            .iter()
            .all(|a| a.subject != Subject::Group("crew".into()))
    );
}

/// A network of schema 3 (phase 6) brought to 4 when a program connects:
/// its assignments kept and read, the rights' revision made.
#[test]
fn schema_3_brought_to_4() {
    let Some(cluster) = Cluster::start("schema3") else {
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
        for (version, note, sql) in &ohl_store_pg::schema::MIGRATIONS[..3] {
            c.batch_execute(sql).unwrap();
            c.execute(
                "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
                &[version, note],
            )
            .unwrap();
        }
        c.batch_execute(
            "INSERT INTO net_user (name, display) VALUES ('old', 'old');
             INSERT INTO line (id, name) VALUES ('A', 'a');
             INSERT INTO assignment (subject_kind, subject, role, scope_kind, line, by_user)
                 VALUES ('user', 'old', 'designer', 'line', 'A', 'import')",
        )
        .unwrap();
    }
    let net = Network::connect(&url).unwrap();
    assert_eq!(net.schema().unwrap(), ohl_store_pg::schema::SCHEMA);
    assert_eq!(net.rights_rev().unwrap(), 0);
    let d = net.rights().unwrap();
    let rights = d.compile("old", &ohl_session::rights::NoModels);
    assert_eq!(rights.model_role("A"), Some(Role::Designer));
}
