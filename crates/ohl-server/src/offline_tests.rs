// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Phase 12 on a network's server, on a throwaway PostgreSQL cluster:
//!
//! - the rules of shared towers (phase 8) on commands made directly on a
//!   network's main line — a broken connection refused naming the other
//!   route, a move marking it stale and its owners told, a protected main
//!   waiting for a review;
//! - an offline working set exported (through the protocol and the
//!   command line's way), checked offline with the same report as the
//!   server's (or NOT ALL CHECKED where neighbours were left out), worked
//!   on, brought back as a bundle onto a branch over the working set —
//!   conflicts with main listed, never settled;
//! - bundles refused: changed by hand, of an unknown base, another user's,
//!   changing a read-only neighbour or a hidden object, after the user's
//!   rights changed — nothing applied, the refusal audited and naming
//!   nothing hidden.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ohl_session::rights::{Scope, Subject};
use ohl_session::versions::Bundle;
use ohl_session::working_set::{Pick, WorkingSetSpec};
use ohl_session::{Command, Kind, Session};
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::rights::RightsCommand as R;

use crate::client::{Joined, NetJoin, join_network};
use crate::network_tests::{add_user, local, moved, net_options, test_network};
use crate::rights_tests::{pump, pump_for};
use crate::scoped_tests::{RULES, area, shared};
use crate::server::{OfflineJob, Server};
use crate::{BroughtBack, ClientMsg, OfflineExported, Op, ScopedCheck, ScopedReport, ServerMsg};

fn assign(url: &str, who: &str, role: Option<&str>, deny: bool, scope: Scope) -> i64 {
    let net = Network::connect(url).unwrap();
    let done = net
        .rights_command(
            "admin",
            &R::Assign {
                subject: Subject::parse(who).unwrap(),
                role: role.map(str::to_owned),
                deny,
                scope,
                note: String::new(),
            },
        )
        .unwrap();
    done.split('#')
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn join(server: &Server, u: &str) -> Joined {
    join_network(
        &server.invitation_for(u),
        u,
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap()
}

fn request(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    model: &str,
    op: Op,
) -> Result<String, String> {
    j.link
        .send(ClientMsg::RequestOn {
            id,
            model: model.into(),
            op,
            line: "main".into(),
        })
        .unwrap();
    let got = pump(
        j,
        log,
        |m| matches!(m, ServerMsg::Response { id: i, .. } if *i == id),
    );
    match got.last() {
        Some(ServerMsg::Response { result, .. }) => result.clone(),
        other => Err(format!("no answer: {other:?}")),
    }
}

/// Working set `spec` exported through the protocol into `dir/NAME.ohl`.
fn export_via(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    spec: &WorkingSetSpec,
    path: &Path,
) -> Result<OfflineExported, String> {
    j.link
        .send(ClientMsg::ExportWorkingSet {
            id,
            working_set: spec.clone(),
            reach: None,
        })
        .unwrap();
    let got = pump_for(j, log, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::WorkingSetExported { id: i, .. } if *i == id)
            || matches!(m, ServerMsg::Notice { .. })
    });
    match got.last() {
        Some(ServerMsg::WorkingSetExported { result, .. }) => {
            let f = result.clone()?;
            std::fs::write(path, crate::base64::decode(&f.file)?).unwrap();
            Ok(f.report)
        }
        other => Err(format!("no export: {other:?}")),
    }
}

/// `bundle` brought back through the protocol.
fn import_via(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    bundle: &Bundle,
) -> Result<BroughtBack, String> {
    j.link
        .send(ClientMsg::ImportBundle {
            id,
            bundle: Box::new(bundle.clone()),
            branch: None,
        })
        .unwrap();
    let got = pump_for(j, log, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::BundleImported { id: i, .. } if *i == id)
            || matches!(m, ServerMsg::Notice { .. })
    });
    match got.last() {
        Some(ServerMsg::BundleImported { result, .. }) => result.clone().map(|b| *b),
        other => Err(format!("no answer: {other:?}")),
    }
}

fn check(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    spec: &WorkingSetSpec,
    c: &ScopedCheck,
) -> ScopedReport {
    j.link
        .send(ClientMsg::Check {
            id,
            working_set: Some(spec.clone()),
            check: c.clone(),
        })
        .unwrap();
    let got = pump_for(
        j,
        log,
        Duration::from_secs(120),
        |m| matches!(m, ServerMsg::CheckResult { id: i, .. } if *i == id),
    );
    match got.last() {
        Some(ServerMsg::CheckResult { result, .. }) => *result.clone().unwrap(),
        other => panic!("no result: {other:?}"),
    }
}

/// What two reports must share: their counts and every violation.
fn same_report(a: &ScopedReport, b: &ScopedReport) {
    assert_eq!(
        a.status, b.status,
        "{:?} / {:?}",
        a.not_all_checked, b.not_all_checked
    );
    assert_eq!((a.results, a.violations), (b.results, b.violations));
    let key = |r: &ScopedReport| -> Vec<String> {
        let mut v: Vec<String> = r
            .findings
            .iter()
            .map(|f| {
                format!(
                    "{} | {} | {:?} | {} | {:?} | {:?} | {}",
                    f.what, f.on, f.against, f.measure, f.value, f.required, f.violates
                )
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(key(a), key(b));
}

/// A structure of the file's working set (the `k`-th).
fn wanted_structure(s: &Session, k: usize) -> String {
    let ids: Vec<String> = s
        .part()
        .unwrap()
        .iter()
        .filter(|(kind, _)| *kind == Kind::Structure)
        .map(|(_, id)| id.clone())
        .collect();
    ids[k % ids.len()].clone()
}

fn shift(s: &mut Session, user: &str, id: &str, dx: f64) -> Result<(), String> {
    let p = s.line().structure(id).ok_or("no structure")?.position;
    s.apply(
        user,
        Command::MoveStructure {
            id: id.into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn lines_of(url: &str, code: &str) -> usize {
    let net = Network::connect(url).unwrap();
    let s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
    s.lines().unwrap().len()
}

fn audit_has(url: &str, action: &str, part: &str) -> bool {
    Network::connect(url)
        .unwrap()
        .audit_rows()
        .unwrap()
        .iter()
        .any(|r| r[2] == action && (r[3].contains(part) || r[4].contains(part)))
}

#[test]
fn offline_working_sets_go_out_and_come_back() {
    let Some(t) = test_network("offline", 300) else {
        return;
    };
    let (code, route, other, _twin, at) = shared(&t.url, &t.codes);
    {
        let net = Network::connect(&t.url).unwrap();
        let on_route = Scope {
            route: Some(route.clone()),
            ..Scope::default()
        };
        add_user(&net, "dora", "designer", on_route.clone());
        add_user(&net, "pat", "designer", on_route);
    }
    let dora_sees = assign(&t.url, "user:dora", Some("viewer"), false, Scope::network());
    let _ = dora_sees;
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let dir: PathBuf = t.dir.join("offline");
    std::fs::create_dir_all(&dir).unwrap();
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![area(at, 400.0)],
    };
    let (dora, bob) = (join(&server, "dora"), join(&server, "bob"));
    let mut log = Vec::new();
    // 1. Dora's export through the protocol: her working set editable,
    // the other line's neighbours with it, nothing left out.
    let path = dir.join("dora.ohl");
    // Main has no revision yet: recording it would record every object —
    // not hers to record (phase 11's rule for a branch over a working
    // set): the network's server records it itself (2026-11-23), audited
    // under her name, and her export goes out.
    let first = export_via(&dora, &mut log, 1, &spec, &dir.join("dora-first.ohl")).unwrap();
    assert!(first.recorded_first, "{first:?}");
    {
        let net = Network::connect(&t.url).unwrap();
        let s = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        let head = s
            .branch_revision(ohl_session::versions::MAIN)
            .unwrap()
            .unwrap();
        assert_eq!(head, first.base);
        let rev = s.revision(&head).unwrap();
        assert_eq!(rev.author, crate::network_rights::SERVER_AUTHOR, "{rev:?}");
        assert!(
            rev.message.contains("offline working set of dora"),
            "{rev:?}"
        );
    }
    assert!(audit_has(&t.url, "server revision", "dora"));
    // Main recorded: the next export records nothing first.
    let out = export_via(&dora, &mut log, 3, &spec, &path).unwrap();
    assert!(out.wanted > 0 && out.context > 0, "{out:?}");
    assert!(
        out.neighbour_lines >= 1 && out.left_out.is_empty(),
        "{out:?}"
    );
    assert!(!out.recorded_first, "main was recorded");
    assert_eq!(out.base, first.base);
    let net = Network::connect(&t.url).unwrap();
    let record = net.offline_export(&out.export).unwrap().unwrap();
    assert_eq!(
        (record.user.as_str(), record.base.as_str()),
        ("dora", out.base.as_str())
    );
    assert!(audit_has(&t.url, "export working set", &code));
    {
        let s = Session::open(&path).unwrap();
        let meta = s.offline_meta().unwrap().unwrap();
        assert_eq!(meta.export, out.export);
        assert!(
            s.offline_neighbours()
                .unwrap()
                .iter()
                .any(|n| n.model == other)
        );
    }
    // 2. Checks offline: the server's report, to the bit.
    for (k, c) in [
        ScopedCheck::Clearances {
            case: "10 °C".into(),
            rules: RULES.into(),
        },
        ScopedCheck::StructureLoads {
            structures: Vec::new(),
        },
    ]
    .iter()
    .enumerate()
    {
        let on_server = check(&dora, &mut log, 10 + k as u64, &spec, c);
        let offline = crate::offline_checks::check_offline(&path, c).unwrap();
        assert_eq!(on_server.status, "every part checked", "{on_server:?}");
        same_report(&on_server, &offline);
    }
    // 3. Pat may see only her route: the other line's neighbours left out
    // and counted (never written); her checks offline are not all checked.
    let pat_path = dir.join("pat.ohl");
    let pat_out: OfflineExported = serde_json::from_value(
        server
            .offline(
                "pat",
                OfflineJob::Export {
                    working_set: spec.clone(),
                    reach: None,
                    out: pat_path.clone(),
                },
            )
            .unwrap(),
    )
    .unwrap();
    assert!(pat_out.left_out.other_lines > 0, "{pat_out:?}");
    {
        let s = Session::open(&pat_path).unwrap();
        assert!(
            s.offline_neighbours()
                .unwrap()
                .iter()
                .all(|n| n.model != other)
        );
    }
    let offline = crate::offline_checks::check_offline(
        &pat_path,
        &ScopedCheck::Clearances {
            case: "10 °C".into(),
            rules: RULES.into(),
        },
    )
    .unwrap();
    assert_eq!(offline.status, "NOT ALL CHECKED");
    // 4. Dora works offline; Bob moves another structure on main meanwhile.
    let (s1, s2) = {
        let s = Session::open(&path).unwrap();
        (wanted_structure(&s, 1), wanted_structure(&s, 2))
    };
    let bundle = {
        let mut s = Session::open(&path).unwrap();
        shift(&mut s, "dora", &s1, 0.5).unwrap();
        s.commit("dora", "s1 east").unwrap();
        s.offline_bundle().unwrap()
    };
    let p2 = Session::open_on(Box::new(net.store(&code).unwrap()), &code)
        .unwrap()
        .line()
        .structure(&s2)
        .unwrap()
        .position;
    request(
        &bob,
        &mut log,
        20,
        &code,
        moved(&s2, [p2[0] + 0.2, p2[1], p2[2]]),
    )
    .unwrap();
    // 5. Brought back: a branch over the working set from the export's
    // base, its one change; main as it was.
    let back = import_via(&dora, &mut log, 21, &bundle).unwrap();
    assert_eq!(
        (back.puts, back.deletes, back.revisions),
        (1, 0, 1),
        "{back:?}"
    );
    assert!(back.conflicts.is_empty(), "{back:?}");
    assert!(audit_has(&t.url, "import bundle", &code));
    let model = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
    let base_x = {
        let s = Session::open(&path).unwrap();
        s.line_named("main")
            .unwrap()
            .structure(&s1)
            .unwrap()
            .position[0]
    };
    assert_eq!(
        model
            .line_named(&back.branch)
            .unwrap()
            .structure(&s1)
            .unwrap()
            .position[0],
        base_x
    );
    assert!((model.line().structure(&s1).unwrap().position[0] - (base_x - 0.5)).abs() < 1e-9);
    assert_eq!(
        model.branch_scope(&back.branch).unwrap().by,
        "dora",
        "a branch over the working set, hers"
    );
    drop(model);
    // 6. A conflict: Bob moves s1 on main, Dora again offline — listed,
    // never settled.
    let p1 = Session::open_on(Box::new(net.store(&code).unwrap()), &code)
        .unwrap()
        .line()
        .structure(&s1)
        .unwrap()
        .position;
    request(
        &bob,
        &mut log,
        22,
        &code,
        moved(&s1, [p1[0] - 0.3, p1[1], p1[2]]),
    )
    .unwrap();
    let bundle2 = {
        let mut s = Session::open(&path).unwrap();
        shift(&mut s, "dora", &s1, 0.25).unwrap();
        s.commit("dora", "s1 further east").unwrap();
        s.offline_bundle().unwrap()
    };
    let back2 = import_via(&dora, &mut log, 23, &bundle2).unwrap();
    assert_eq!(
        back2.conflicts,
        vec![format!("structure {s1}")],
        "{back2:?}"
    );
    assert_ne!(back2.branch, back.branch);
    // 7. Refused, nothing applied, audited.
    let before = lines_of(&t.url, &code);
    let refused = |r: Result<BroughtBack, String>, words: &str| {
        let e = r.unwrap_err();
        assert!(e.contains(words), "{words:?} not in: {e}");
    };
    // A value changed by hand.
    let mut b = bundle2.clone();
    let (h, text) = b
        .blobs
        .iter()
        .find(|(_, t)| t.contains(&format!("\"{s1}\"")) && t.contains("\"position\""))
        .map(|(h, t)| (h.clone(), t.clone()))
        .unwrap();
    b.blobs
        .insert(h, text.replace("\"position\"", "\"position\" "));
    refused(
        import_via(&dora, &mut log, 30, &b),
        "does not match its hash",
    );
    // The same bundle twice.
    refused(
        import_via(&dora, &mut log, 34, &bundle2),
        "brought back already",
    );
    // A base this network does not know.
    let alien = {
        let p = dir.join("alien.ohl");
        let mut s = Session::open(&p).unwrap();
        s.import(
            &ohl_line::Line::load(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
            )
            .unwrap(),
            "dora",
        )
        .unwrap();
        s.commit("dora", "mine").unwrap();
        shift(&mut s, "dora", "T2", 1.0).unwrap();
        s.commit("dora", "moved").unwrap();
        s.export_bundle("main").unwrap()
    };
    refused(import_via(&dora, &mut log, 31, &alien), "does not know");
    // Another user's export.
    let pat = join(&server, "pat");
    refused(import_via(&pat, &mut log, 32, &bundle2), "another user");
    // A read-only neighbour changed: the file's part opened up by hand.
    let read_only = {
        let s = Session::open(&path).unwrap();
        let wanted = s.part().unwrap().clone();
        let context: String = s
            .line()
            .structures
            .iter()
            .map(|x| x.id.clone())
            .find(|id| !wanted.contains(&(Kind::Structure, id.clone())))
            .expect("a structure of the context");
        // Everything of the file made editable.
        let all: std::collections::BTreeSet<(Kind, String)> = s
            .object_keys()
            .filter(|(k, _)| !matches!(k, Kind::Meta | Kind::Library))
            .cloned()
            .collect();
        drop(s);
        {
            use ohl_session::store::Store;
            let file = ohl_session::store::sqlite::SqliteStore::open(&path).unwrap();
            file.set_meta(
                ohl_session::part::PART_KEY,
                &serde_json::to_string(&all.into_iter().collect::<Vec<_>>()).unwrap(),
            )
            .unwrap();
        }
        let mut s = Session::open(&path).unwrap();
        shift(&mut s, "dora", &context, 0.1).unwrap();
        s.commit("dora", "a neighbour moved").unwrap();
        s.offline_bundle().unwrap()
    };
    refused(import_via(&dora, &mut log, 33, &read_only), "read-only");
    assert_eq!(lines_of(&t.url, &code), before, "nothing applied");
    assert!(audit_has(&t.url, "refused", "bundle"));
    drop((dora, bob, pat));
    server.stop();
}

/// Phase 12's security cases that change rights: a bundle with an edit to
/// a hidden object, and one brought back after the user's rights changed —
/// refused, naming nothing hidden, nothing applied.
#[test]
fn bundles_after_rights_changes_are_refused() {
    let Some(t) = test_network("offline-rights", 240) else {
        return;
    };
    let code = t.codes[0].clone();
    let (route, ids) = {
        let net = Network::connect(&t.url).unwrap();
        let mut s = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        let route = s.line().routes[0].id.clone();
        let d = s.route_detail(&route).unwrap();
        let ids: Vec<String> = d.structures.iter().map(|x| x.id.clone()).collect();
        (route, ids)
    };
    Network::connect(&t.url)
        .unwrap()
        .rights_command(
            "admin",
            &R::AddUser {
                name: "dora".into(),
                display: None,
            },
        )
        .unwrap();
    let designer = assign(
        &t.url,
        "user:dora",
        Some("designer"),
        false,
        Scope {
            route: Some(route.clone()),
            ..Scope::default()
        },
    );
    // A structure far along the route hidden from her (a deny on it).
    let hidden = ids[ids.len() - 2].clone();
    assign(
        &t.url,
        "user:dora",
        None,
        true,
        Scope {
            object: Some(hidden.clone()),
            ..Scope::default()
        },
    );
    {
        let net = Network::connect_writer(&t.url).unwrap();
        let mut s = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        s.commit("admin", "first").unwrap();
    }
    t._cluster.wait_for_locks_gone("net");
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let dir = t.dir.join("offline-rights");
    std::fs::create_dir_all(&dir).unwrap();
    // Her working set: the route's first structures (away from the hidden
    // one).
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![Pick::Structures {
            route: route.clone(),
            from: ids[0].clone(),
            to: ids[3].clone(),
        }],
    };
    let export = |name: &str| -> Result<PathBuf, String> {
        let p = dir.join(name);
        server.offline(
            "dora",
            OfflineJob::Export {
                working_set: spec.clone(),
                reach: Some(100.0),
                out: p.clone(),
            },
        )?;
        Ok(p)
    };
    let bring = |b: &Bundle| -> Result<serde_json::Value, String> {
        server.offline(
            "dora",
            OfflineJob::Import {
                bundle: Box::new(b.clone()),
                branch: None,
            },
        )
    };
    // The whole route reaches the hidden structure: refused as a join is.
    let whole = server.offline(
        "dora",
        OfflineJob::Export {
            working_set: WorkingSetSpec {
                name: String::new(),
                model: code.clone(),
                picks: vec![Pick::Route {
                    route: route.clone(),
                }],
            },
            reach: None,
            out: dir.join("whole.ohl"),
        },
    );
    let e = whole.unwrap_err();
    assert!(e.contains("no access") && !e.contains(&hidden), "{e}");
    // An edit to the hidden structure: a new object of its id, made
    // offline — refused without naming it.
    let path = export("a.ohl").unwrap();
    let hidden_edit = {
        let mut s = Session::open(&path).unwrap();
        let mine = wanted_structure(&s, 0);
        let mut v = serde_json::to_value(s.line().structure(&mine).unwrap()).unwrap();
        v["id"] = serde_json::Value::String(hidden.clone());
        let x = v["position"][0].as_f64().unwrap();
        v["position"][0] = serde_json::json!(x + 3.0);
        s.apply(
            "dora",
            Command::Put {
                kind: Kind::Structure,
                id: hidden.clone(),
                value: v,
                ord: None,
            },
        )
        .unwrap();
        s.commit("dora", "a structure of that id").unwrap();
        s.offline_bundle().unwrap()
    };
    let before = lines_of(&t.url, &code);
    let e = bring(&hidden_edit).unwrap_err();
    assert!(!e.contains(&hidden), "a hidden id named: {e}");
    assert_eq!(lines_of(&t.url, &code), before, "nothing applied");
    // After her rights changed (her route's grant taken away): refused.
    let path = export("b.ohl").unwrap();
    let valid = {
        let mut s = Session::open(&path).unwrap();
        let mine = wanted_structure(&s, 1);
        shift(&mut s, "dora", &mine, 0.4).unwrap();
        s.commit("dora", "mine east").unwrap();
        s.offline_bundle().unwrap()
    };
    Network::connect(&t.url)
        .unwrap()
        .rights_command("admin", &R::Unassign { id: designer })
        .unwrap();
    // The running server takes the rights before its next job.
    let e = bring(&valid).unwrap_err();
    assert!(!e.is_empty());
    assert_eq!(lines_of(&t.url, &code), before, "nothing applied");
    server.stop();
}

/// Phase 12's first item: a command made directly on a network's main
/// line is checked by the rules of shared towers against the routes that
/// are its user's.
#[test]
fn shared_towers_on_direct_edits_of_main() {
    let Some(t) = test_network("shared-main", 60) else {
        return;
    };
    // The example line (two routes sharing a tower) as a model of its own.
    let ex = t.dir.join("example.ohl");
    {
        let crs = Network::connect(&t.url)
            .unwrap()
            .meta("crs")
            .unwrap()
            .unwrap_or_default();
        let mut line = ohl_line::Line::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
        )
        .unwrap();
        line.crs = crs;
        let mut s = Session::open(&ex).unwrap();
        s.import(&line, "admin").unwrap();
    }
    {
        let net = Network::connect_writer(&t.url).unwrap();
        let mut o = ImportOptions::new("EX", "admin");
        o.revision = false;
        import(&net, &ex, &o).unwrap();
    }
    t._cluster.wait_for_locks_gone("net");
    // Its routes and the tower they share, as the network named them.
    let (main_route, branch_route, tower, used_by) = {
        let net = Network::connect(&t.url).unwrap();
        let mut s = Session::open_on(Box::new(net.store("EX").unwrap()), "EX").unwrap();
        let routes: Vec<String> = s.line().routes.iter().map(|r| r.id.clone()).collect();
        let keys: Vec<(Kind, String)> = s
            .line()
            .structures
            .iter()
            .map(|x| (Kind::Structure, x.id.clone()))
            .collect();
        let on = s.object_routes(&keys);
        let tower = keys
            .iter()
            .zip(on)
            .find(|(_, r)| r.len() == 2)
            .map(|((_, id), _)| id.clone())
            .expect("a shared tower");
        let r0 = routes[0].clone();
        let r1 = routes[1].clone();
        (r0, r1, tower, "EX")
    };
    let _ = used_by;
    {
        let net = Network::connect(&t.url).unwrap();
        add_user(
            &net,
            "dora",
            "designer",
            Scope {
                route: Some(main_route.clone()),
                ..Scope::default()
            },
        );
        add_user(
            &net,
            "ben",
            "designer",
            Scope {
                route: Some(branch_route.clone()),
                ..Scope::default()
            },
        );
    }
    assign(
        &t.url,
        "user:dora",
        Some("viewer"),
        false,
        Scope::line("EX"),
    );
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let (dora, ben, bob, admin) = (
        join(&server, "dora"),
        join(&server, "ben"),
        join(&server, "bob"),
        join(&server, "admin"),
    );
    let mut log = Vec::new();
    let p = {
        let net = Network::connect(&t.url).unwrap();
        Session::open_on(Box::new(net.store("EX").unwrap()), "EX")
            .unwrap()
            .line()
            .structure(&tower)
            .unwrap()
            .position
    };
    // Deleting the tower the other route uses: refused, naming it.
    let e = request(
        &dora,
        &mut log,
        1,
        "EX",
        Op::Do {
            command: Command::DeleteStructure {
                id: tower.clone(),
                confirm: true,
            },
        },
    )
    .unwrap_err();
    assert!(e.contains(&branch_route) && e.contains("rule 1"), "{e}");
    // A move: allowed, the other route out of date, its owner told.
    request(
        &dora,
        &mut log,
        2,
        "EX",
        moved(&tower, [p[0] + 0.3, p[1], p[2]]),
    )
    .unwrap();
    let mut ben_log = Vec::new();
    let got = pump(
        &ben,
        &mut ben_log,
        |m| matches!(m, ServerMsg::Notice { text } if text.contains("out of date")),
    );
    assert!(matches!(got.last(), Some(ServerMsg::Notice { text }) if text.contains(&branch_route)));
    let stale = {
        let net = Network::connect(&t.url).unwrap();
        Session::open_on(Box::new(net.store("EX").unwrap()), "EX")
            .unwrap()
            .stale_routes()
            .unwrap()
    };
    assert!(stale.contains_key(&branch_route) && !stale.contains_key(&main_route));
    // Bob may edit every route: as before (no rule).
    request(
        &bob,
        &mut log,
        3,
        "EX",
        moved(&tower, [p[0] + 0.6, p[1], p[2]]),
    )
    .unwrap();
    // A protected main: the network admin's move of the tower, made with
    // a working set of the first route followed, waits for a review.
    request(
        &admin,
        &mut log,
        4,
        "EX",
        Op::Commit {
            message: "first".into(),
        },
    )
    .unwrap();
    request(
        &admin,
        &mut log,
        5,
        "EX",
        Op::ProtectBranch {
            name: "main".into(),
            protected: true,
        },
    )
    .unwrap();
    // One connection a user: the admin joins again, following a working set.
    let gone = |server: &Server| {
        let t0 = std::time::Instant::now();
        while server.peers().iter().any(|p| p.user == "admin") {
            assert!(t0.elapsed() < crate::tests::WAIT);
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    drop(admin);
    gone(&server);
    let file = t.dir.join("admin-ws.ohl");
    let admin_ws = join_network(
        &server.invitation_for("admin"),
        "admin",
        &NetJoin {
            working_set: Some(WorkingSetSpec {
                name: String::new(),
                model: "EX".into(),
                picks: vec![Pick::Route {
                    route: main_route.clone(),
                }],
            }),
            file: Some(file),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let e = request(
        &admin_ws,
        &mut log,
        6,
        "EX",
        moved(&tower, [p[0] + 0.9, p[1], p[2]]),
    )
    .unwrap_err();
    assert!(e.contains("needs a review by its owners"), "{e}");
    drop(admin_ws);
    gone(&server);
    // Without a working set (every route hers): as before.
    let admin = join(&server, "admin");
    request(
        &admin,
        &mut log,
        7,
        "EX",
        moved(&tower, [p[0] + 0.9, p[1], p[2]]),
    )
    .unwrap();
    drop((dora, ben, bob, admin));
    server.stop();
}

/// Phase 14: the exports' thread (owner, 2026-11-20: phase 12's question
/// 6). An export written on its thread is the same as on the authority —
/// its report (its number, revision, size and seconds aside) and what the
/// file holds (its time and revision aside) — for a user seeing everything
/// and one seeing a route, through the protocol and the command line's
/// way. A move applied before an export's content is taken is in it; a
/// move landing while it is written is not, the file staying the base
/// revision's, and the export is marked stale. A user whose rights change
/// while it is written gets nothing.
#[test]
fn exports_on_their_thread_as_on_the_authority() {
    use crate::network::{NetOptions, Probe};
    let Some(t) = test_network("exports", 200) else {
        return;
    };
    let (code, route, _other, _twin, at) = shared(&t.url, &t.codes);
    {
        let net = Network::connect(&t.url).unwrap();
        let on_route = Scope {
            route: Some(route.clone()),
            ..Scope::default()
        };
        add_user(&net, "pat", "designer", on_route);
    }
    let dir: PathBuf = t.dir.join("exports");
    std::fs::create_dir_all(&dir).unwrap();
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![area(at, 400.0)],
    };
    // The main line recorded once (Bob may edit it all).
    {
        let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
        let bob = join(&server, "bob");
        let mut log = Vec::new();
        request(
            &bob,
            &mut log,
            1,
            &code,
            Op::Commit {
                message: "first".into(),
            },
        )
        .unwrap();
        drop(bob);
        server.stop();
    }
    let null = serde_json::Value::Null;
    // What a file holds, its export's own revision and time aside.
    let held = |path: &Path| -> serde_json::Value {
        let s = Session::open(path).unwrap();
        let mut meta = serde_json::to_value(s.offline_meta().unwrap().unwrap()).unwrap();
        meta["time"] = null.clone();
        meta["export"] = null.clone();
        serde_json::json!({
            "line": serde_json::to_value(s.line()).unwrap(),
            "part": s.part(),
            "neighbours": serde_json::to_value(s.offline_neighbours().unwrap()).unwrap(),
            "meta": meta,
        })
    };
    let report = |r: &OfflineExported| -> serde_json::Value {
        assert!(!r.stale && r.changed_meanwhile == 0, "{r:?}");
        let mut v = serde_json::to_value(r).unwrap();
        for k in ["number", "export", "seconds", "bytes"] {
            v[k] = null.clone();
        }
        v
    };
    for user in ["bob", "pat"] {
        let mut got = Vec::new();
        for worker in [false, true] {
            let server = Server::start_network(
                local("abcd2345"),
                NetOptions {
                    export_worker: worker,
                    ..net_options(&t.url)
                },
            )
            .unwrap();
            let j = join(&server, user);
            let mut log = Vec::new();
            let via = dir.join(format!("{user}-{worker}-protocol.ohl"));
            let a = export_via(&j, &mut log, 10, &spec, &via).unwrap();
            let line = dir.join(format!("{user}-{worker}-command.ohl"));
            let b: OfflineExported = serde_json::from_value(
                server
                    .offline(
                        user,
                        OfflineJob::Export {
                            working_set: spec.clone(),
                            reach: Some(800.0),
                            out: line.clone(),
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
            got.push((report(&a), held(&via), report(&b), held(&line)));
            drop(j);
            server.stop();
        }
        assert!(got[0] == got[1], "{user}: the thread's export differs");
        if user == "pat" {
            assert!(got[1].0["left_out"]["other_lines"].as_u64() > Some(0));
        }
    }
    // A move before an export, one while it is written (its writing held
    // at the exports' thread's gate until the move is answered).
    let gate = std::sync::Arc::new(crate::network::ExportGate::default());
    let server = Server::start_network(
        local("abcd2345"),
        NetOptions {
            export_gate: Some(gate.clone()),
            ..net_options(&t.url)
        },
    )
    .unwrap();
    let (bob, carol) = (join(&server, "bob"), join(&server, "carol"));
    let mut log = Vec::new();
    let position = |path: &Path, id: &str| -> [f64; 3] {
        Session::open(path)
            .unwrap()
            .line()
            .structure(id)
            .unwrap()
            .position
    };
    let first = dir.join("bob-true-protocol.ohl");
    let sid = wanted_structure(&Session::open(&first).unwrap(), 0);
    let p0 = position(&first, &sid);
    request(
        &carol,
        &mut log,
        20,
        &code,
        moved(&sid, [p0[0] + 0.3, p0[1], p0[2]]),
    )
    .unwrap();
    let before = dir.join("before.ohl");
    let out = export_via(&bob, &mut log, 21, &spec, &before).unwrap();
    assert!(out.recorded_first && !out.stale, "{out:?}");
    assert_eq!(position(&before, &sid)[0], p0[0] + 0.3);
    let plans = |server: &Server| -> usize {
        let v: serde_json::Value =
            serde_json::from_str(&server.probe(Probe::Stats).unwrap()).unwrap();
        v["export_plan_ms"].as_array().map_or(0, Vec::len)
    };
    let planned = plans(&server);
    let held = gate.hold();
    bob.link
        .send(ClientMsg::ExportWorkingSet {
            id: 22,
            working_set: spec.clone(),
            reach: None,
        })
        .unwrap();
    // Its content taken on the authority, then the move, then its writing.
    let t0 = std::time::Instant::now();
    while plans(&server) == planned {
        assert!(t0.elapsed() < Duration::from_secs(120), "no export started");
        std::thread::sleep(Duration::from_millis(20));
    }
    request(
        &carol,
        &mut log,
        23,
        &code,
        moved(&sid, [p0[0] + 0.6, p0[1], p0[2]]),
    )
    .unwrap();
    drop(held);
    let got = pump_for(&bob, &mut log, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::WorkingSetExported { id: 22, .. })
    });
    let Some(ServerMsg::WorkingSetExported { result, .. }) = got.last() else {
        panic!("no export: {got:?}")
    };
    let during = result.clone().unwrap();
    assert!(
        during.report.stale && during.report.changed_meanwhile >= 1,
        "{:?}",
        during.report
    );
    assert!(!during.report.recorded_first, "{:?}", during.report);
    let path = dir.join("during.ohl");
    std::fs::write(&path, crate::base64::decode(&during.file).unwrap()).unwrap();
    // The base revision's working set: the move before in it, not the one
    // during.
    assert_eq!(position(&path, &sid)[0], p0[0] + 0.3);
    assert_eq!(during.report.base, out.base);
    let net = Network::connect(&t.url).unwrap();
    let record = net.offline_export(&during.report.export).unwrap().unwrap();
    assert_eq!(record.base, out.base);
    assert!(audit_has(
        &t.url,
        "export working set",
        "changed on the server while it was written"
    ));
    drop((bob, carol));
    // Pat's rights changed while her export is written: nothing.
    let planned = plans(&server);
    let out_path = dir.join("pat-changed.ohl");
    let held = gate.hold();
    let asked = std::thread::scope(|s| {
        let h = s.spawn(|| {
            server.offline(
                "pat",
                OfflineJob::Export {
                    working_set: spec.clone(),
                    reach: None,
                    out: out_path.clone(),
                },
            )
        });
        let t0 = std::time::Instant::now();
        while plans(&server) == planned {
            assert!(t0.elapsed() < Duration::from_secs(120), "no export started");
            std::thread::sleep(Duration::from_millis(20));
        }
        assign(&t.url, "user:pat", Some("viewer"), false, Scope::network());
        drop(held);
        h.join().unwrap()
    });
    let e = asked.unwrap_err();
    assert!(e.contains("rights changed"), "{e}");
    assert!(!out_path.exists());
    server.stop();
}

/// The rest of the exports' thread (2026-11-20): a bundle decided on the
/// authority, its branch's line made ready on the exports' thread,
/// applied on the authority — the same report and the same branch as all
/// on the authority (through the protocol and the command line's way); a
/// move of main landing while it is made ready is a conflict when it is
/// applied (main as it is then), never replayed; a user whose rights
/// change meanwhile gets nothing applied.
#[test]
fn bundles_on_their_thread_as_on_the_authority() {
    use crate::network::{NetOptions, Probe};
    let Some(t) = test_network("bundles", 200) else {
        return;
    };
    let (code, route, _other, _twin, at) = shared(&t.url, &t.codes);
    {
        let net = Network::connect(&t.url).unwrap();
        let on_route = Scope {
            route: Some(route.clone()),
            ..Scope::default()
        };
        add_user(&net, "pat", "designer", on_route);
    }
    let dir: PathBuf = t.dir.join("bundles");
    std::fs::create_dir_all(&dir).unwrap();
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![area(at, 400.0)],
    };
    let null = serde_json::Value::Null;
    let report = |r: &BroughtBack| -> serde_json::Value {
        let mut v = serde_json::to_value(r).unwrap();
        for k in ["branch", "seconds"] {
            v[k] = null.clone();
        }
        v
    };
    // Bob's working set exported once; worked on twice alike (two bundles
    // of the same export, their messages apart).
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let bob = join(&server, "bob");
    let mut log = Vec::new();
    let path = dir.join("bob.ohl");
    export_via(&bob, &mut log, 1, &spec, &path).unwrap();
    drop(bob);
    server.stop();
    let (s1, s2) = {
        let s = Session::open(&path).unwrap();
        (wanted_structure(&s, 0), wanted_structure(&s, 3))
    };
    let worked = |name: &str| -> Bundle {
        let copy = dir.join(format!("{name}.ohl"));
        std::fs::copy(&path, &copy).unwrap();
        let mut s = Session::open(&copy).unwrap();
        shift(&mut s, "bob", &s1, 0.4).unwrap();
        shift(&mut s, "bob", &s2, -0.2).unwrap();
        s.commit("bob", &format!("two moves ({name})")).unwrap();
        s.offline_bundle().unwrap()
    };
    let mut got = Vec::new();
    let mut lines = Vec::new();
    for (k, worker) in [false, true].into_iter().enumerate() {
        let server = Server::start_network(
            local("abcd2345"),
            NetOptions {
                export_worker: worker,
                ..net_options(&t.url)
            },
        )
        .unwrap();
        let bob = join(&server, "bob");
        let mut log = Vec::new();
        let a = import_via(&bob, &mut log, 10, &worked(&format!("p{k}"))).unwrap();
        let b: BroughtBack = serde_json::from_value(
            server
                .offline(
                    "bob",
                    OfflineJob::Import {
                        bundle: Box::new(worked(&format!("c{k}"))),
                        branch: None,
                    },
                )
                .unwrap(),
        )
        .unwrap();
        got.push((report(&a), report(&b)));
        drop(bob);
        server.stop();
        let net = Network::connect(&t.url).unwrap();
        let model = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        for branch in [&a.branch, &b.branch] {
            lines.push(serde_json::to_value(model.line_named(branch).unwrap()).unwrap());
        }
    }
    assert!(got[0] == got[1], "the thread's bundles differ: {got:?}");
    assert!(got[0].0["puts"].as_u64() >= Some(2), "{got:?}");
    assert!(lines.iter().all(|l| *l == lines[0]), "the branches differ");
    // A move of s1 on main while a bundle's line is made ready (its
    // making held at the exports' thread's gate until the move is
    // answered): a conflict when it is applied.
    let gate = std::sync::Arc::new(crate::network::ExportGate::default());
    let server = Server::start_network(
        local("abcd2345"),
        NetOptions {
            export_gate: Some(gate.clone()),
            ..net_options(&t.url)
        },
    )
    .unwrap();
    let (bob, carol) = (join(&server, "bob"), join(&server, "carol"));
    let mut log = Vec::new();
    let plans = |server: &Server| -> usize {
        let v: serde_json::Value =
            serde_json::from_str(&server.probe(Probe::Stats).unwrap()).unwrap();
        v["bundle_plan_ms"].as_array().map_or(0, Vec::len)
    };
    let planned = plans(&server);
    let during_bundle = worked("during");
    let held = gate.hold();
    bob.link
        .send(ClientMsg::ImportBundle {
            id: 30,
            bundle: Box::new(during_bundle),
            branch: None,
        })
        .unwrap();
    let t0 = std::time::Instant::now();
    while plans(&server) == planned {
        assert!(t0.elapsed() < Duration::from_secs(120), "no bundle started");
        std::thread::sleep(Duration::from_millis(20));
    }
    let p1 = Network::connect(&t.url)
        .map(|net| {
            Session::open_on(Box::new(net.store(&code).unwrap()), &code)
                .unwrap()
                .line()
                .structure(&s1)
                .unwrap()
                .position
        })
        .unwrap();
    request(
        &carol,
        &mut log,
        31,
        &code,
        moved(&s1, [p1[0] - 0.3, p1[1], p1[2]]),
    )
    .unwrap();
    drop(held);
    let got = pump_for(&bob, &mut log, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::BundleImported { id: 30, .. })
    });
    let Some(ServerMsg::BundleImported { result, .. }) = got.last() else {
        panic!("no bundle: {got:?}")
    };
    let during = result.clone().unwrap();
    assert!(
        during.conflicts.contains(&format!("structure {s1}")),
        "{during:?}"
    );
    drop((bob, carol));
    // Pat's work brought back while her rights change: nothing applied.
    let pat = join(&server, "pat");
    let mut log = Vec::new();
    let pat_path = dir.join("pat.ohl");
    export_via(&pat, &mut log, 40, &spec, &pat_path).unwrap();
    drop(pat);
    let bundle = {
        let mut s = Session::open(&pat_path).unwrap();
        let id = wanted_structure(&s, 1);
        shift(&mut s, "pat", &id, 0.1).unwrap();
        s.commit("pat", "one move").unwrap();
        s.offline_bundle().unwrap()
    };
    let before = lines_of(&t.url, &code);
    let planned = plans(&server);
    let held = gate.hold();
    let asked = std::thread::scope(|s| {
        let h = s.spawn(|| {
            server.offline(
                "pat",
                OfflineJob::Import {
                    bundle: Box::new(bundle.clone()),
                    branch: None,
                },
            )
        });
        let t0 = std::time::Instant::now();
        while plans(&server) == planned {
            assert!(t0.elapsed() < Duration::from_secs(120), "no bundle started");
            std::thread::sleep(Duration::from_millis(20));
        }
        assign(&t.url, "user:pat", Some("viewer"), false, Scope::network());
        drop(held);
        h.join().unwrap()
    });
    let e = asked.unwrap_err();
    assert!(e.contains("rights changed"), "{e}");
    assert_eq!(lines_of(&t.url, &code), before, "no branch made");
    assert!(audit_has(
        &t.url,
        "refused",
        "rights changed while the bundle"
    ));
    server.stop();
}
