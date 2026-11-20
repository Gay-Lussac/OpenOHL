// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Rights on a network's server (phase 7), on a throwaway PostgreSQL
//! cluster: only the network's users join, with their personal
//! invitations; commands decided by the rights (their rule, then every
//! object they changed, put back when refused) and refusals in the audit;
//! tiles, changes, catch-ups, working sets, route details and error
//! messages filtered; a grant and a deny in any order; a group's member
//! taken out, or a user disabled, while connected: let go, and less seen
//! after; "see as user"; and the property: over random assignments and
//! random edits, no message to a user holds the id or the place of an
//! object that user may not see.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::{Duration, Instant};

use ohl_session::rights::{Permission, Placement, Scope, Subject, Target};
use ohl_session::{Kind, Session};
use ohl_store_pg::Network;
use ohl_store_pg::rights::{DbResolve, RightsCommand as R};

use crate::client::{Incoming, Joined, NetJoin, join_network};
use crate::network_tests::{
    TestNet, add_user, local, moved, net_options, structure_of, test_network, tiles_around,
};
use crate::server::Server;
use crate::{ClientMsg, HeldTile, Op, ServerMsg, TileKey};

/// Takes messages until `until` (10 s at most); every message kept in
/// `log`.
fn pump(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    mut until: impl FnMut(&ServerMsg) -> bool,
) -> Vec<ServerMsg> {
    let started = Instant::now();
    let mut got = Vec::new();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            got.iter().rev().take(4).collect::<Vec<_>>()
        );
        match j.link.recv_timeout(Duration::from_millis(20)) {
            Some(Incoming::Msg(m)) => {
                log.push(m.clone());
                let stop = until(&m);
                got.push(m);
                if stop {
                    return got;
                }
            }
            Some(Incoming::Closed(why)) => {
                log.push(ServerMsg::Notice {
                    text: format!("closed: {why}"),
                });
                return got;
            }
            None => {}
        }
    }
}

/// What comes for `ms` milliseconds.
fn idle(j: &Joined, log: &mut Vec<ServerMsg>, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    pump(j, log, |_| Instant::now() >= until);
}

fn edit(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    model: &str,
    op: Op,
) -> Result<String, String> {
    edit_on(j, log, id, model, "main", op)
}

/// [`edit`] on project line (branch) `line`.
fn edit_on(
    j: &Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    model: &str,
    line: &str,
    op: Op,
) -> Result<String, String> {
    j.link
        .send(ClientMsg::RequestOn {
            id,
            model: model.into(),
            op,
            line: line.into(),
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

fn admin(t: &TestNet, c: R) -> Result<String, String> {
    let net = Network::connect(&t.url).unwrap();
    net.rights_command("admin", &c).map_err(|e| e.to_string())
}

fn assign(t: &TestNet, who: &str, role: Option<&str>, deny: bool, scope: Scope) -> i64 {
    let done = admin(
        t,
        R::Assign {
            subject: Subject::parse(who).unwrap(),
            role: role.map(str::to_owned),
            deny,
            scope,
            note: String::new(),
        },
    )
    .unwrap();
    // "assignment #N …"
    done.split('#')
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// What `user` may not see right now: ids and plan positions (mm) of the
/// structures, sections, routes and alignments of every model.
struct Hidden {
    ids: BTreeSet<String>,
    places: Vec<(i64, i64)>,
}

fn hidden_for(t: &TestNet, user: &str) -> Hidden {
    let net = Network::connect(&t.url).unwrap();
    let rights = net.rights().unwrap().compile(user, &DbResolve(&net));
    let mut ids = BTreeSet::new();
    let mut places = Vec::new();
    // Twin towers: a hidden structure standing where a visible one stands
    // (another line's) shows nothing the visible one does not.
    let mut seen_places = BTreeSet::new();
    for code in &t.codes {
        let mut s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
        let line = s.line().clone();
        let mut objects: Vec<(Kind, String, Option<[f64; 3]>)> = Vec::new();
        objects.extend(
            line.routes
                .iter()
                .map(|x| (Kind::Route, x.id.clone(), None)),
        );
        objects.extend(
            line.alignments
                .iter()
                .map(|x| (Kind::Alignment, x.id.clone(), None)),
        );
        objects.extend(
            line.sections
                .iter()
                .map(|x| (Kind::Section, x.id.clone(), None)),
        );
        objects.extend(
            line.structures
                .iter()
                .map(|x| (Kind::Structure, x.id.clone(), Some(x.position))),
        );
        let keys: Vec<(Kind, String)> = objects.iter().map(|(k, id, _)| (*k, id.clone())).collect();
        let routes = s.object_routes(&keys);
        for ((k, id, pos), r) in objects.into_iter().zip(routes) {
            let r: Vec<String> = r.into_iter().collect();
            let t = Target::object(
                code,
                k,
                &id,
                Placement {
                    bounds: s.spatial_entry(k, &id).map(|e| e.bounds),
                    routes: &r,
                },
            );
            let mm = pos.map(|p| {
                (
                    (p[0] * 1000.0).round() as i64,
                    (p[1] * 1000.0).round() as i64,
                )
            });
            if !rights.allows(Permission::See, &t) {
                if let Some(p) = mm {
                    places.push(p);
                }
                ids.insert(id);
            } else if let Some(p) = mm {
                seen_places.insert(p);
            }
        }
    }
    places.retain(|p| !seen_places.contains(p));
    Hidden { ids, places }
}

/// Every string (and each word of it) and every number of a message.
fn tokens(v: &serde_json::Value, words: &mut HashSet<String>, numbers: &mut HashSet<i64>) {
    match v {
        serde_json::Value::String(s) => {
            words.insert(s.clone());
            for w in ohl_session::rights::words(s) {
                words.insert(w.to_owned());
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                numbers.insert((f * 1000.0).round() as i64);
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| tokens(x, words, numbers)),
        serde_json::Value::Object(o) => o.values().for_each(|x| tokens(x, words, numbers)),
        _ => {}
    }
}

/// No message of `log` holds an id of `hidden` (hidden before and after:
/// both given) or a hidden structure's place (x and y together).
fn no_leak(log: &[ServerMsg], a: &Hidden, b: &Hidden, what: &str) {
    let ids: BTreeSet<&String> = a.ids.intersection(&b.ids).collect();
    let places: Vec<&(i64, i64)> = a.places.iter().filter(|p| b.places.contains(p)).collect();
    for m in log {
        // The line a client asked about, echoed in the answer: its own
        // words.
        let m = &match m.clone() {
            ServerMsg::RouteDetail { result, .. } => ServerMsg::RouteDetail {
                model: String::new(),
                result,
            },
            ServerMsg::Extent { id, result, .. } => ServerMsg::Extent {
                id,
                model: String::new(),
                result,
            },
            other => other,
        };
        let v = serde_json::to_value(m).unwrap();
        let (mut words, mut numbers) = (HashSet::new(), HashSet::new());
        tokens(&v, &mut words, &mut numbers);
        for id in &ids {
            assert!(!words.contains(*id), "{what}: hidden {id} in {m:?}");
        }
        for (x, y) in &places {
            assert!(
                !(numbers.contains(x) && numbers.contains(y)),
                "{what}: a hidden structure's place ({x}, {y}) mm in {m:?}"
            );
        }
    }
}

fn held(tiles: &[TileKey]) -> Vec<HeldTile> {
    tiles
        .iter()
        .map(|t| HeldTile {
            tile: *t,
            rev: None,
        })
        .collect()
}

/// A route of model `code` and two of its structures (the 2nd and 5th).
fn route_of(t: &TestNet, code: &str) -> (String, String, String) {
    let net = Network::connect(&t.url).unwrap();
    let mut s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
    let route = s.line().routes[0].id.clone();
    let d = s.route_detail(&route).unwrap();
    (
        route,
        d.structures[1].id.clone(),
        d.structures[4].id.clone(),
    )
}

fn audit_has(t: &TestNet, action: &str, part: &str) -> bool {
    Network::connect(&t.url)
        .unwrap()
        .audit_rows()
        .unwrap()
        .iter()
        .any(|r| r[2] == action && (r[3].contains(part) || r[4].contains(part)))
}

#[test]
fn rights_decide_commands_and_what_is_sent() {
    let Some(t) = test_network("rights", 240) else {
        return;
    };
    let (a, b) = (t.codes[0].clone(), t.codes[1].clone());
    let (route, _, _) = route_of(&t, &a);
    {
        let net = Network::connect(&t.url).unwrap();
        // Dora: a designer of one route of A; Olly: disabled.
        add_user(
            &net,
            "dora",
            "designer",
            Scope {
                route: Some(route.clone()),
                ..Scope::default()
            },
        );
        add_user(&net, "olly", "viewer", Scope::network());
    }
    admin(
        &t,
        R::SetActive {
            name: "olly".into(),
            active: false,
        },
    )
    .unwrap();
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    // Not a user, disabled, or with the general invitation: refused, in
    // the audit.
    for (who, inv) in [
        ("zed", server.invitation_for("zed")),
        ("olly", server.invitation_for("olly")),
        ("dora", server.invitation.clone()),
    ] {
        let e = join_network(&inv, who, &NetJoin::default(), &mut |_| {})
            .err()
            .unwrap();
        assert!(e.contains("network"), "{who}: {e}");
    }
    assert!(audit_has(&t, "refused", "zed"));
    let before = hidden_for(&t, "dora");
    // Dora views the tiles around a structure of her route and one of B.
    let net = Network::connect(&t.url).unwrap();
    let mut s = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
    let d = s.route_detail(&route).unwrap();
    let mine = d.structures[0].id.clone();
    let pm = s
        .line()
        .structures
        .iter()
        .find(|x| x.id == mine)
        .unwrap()
        .position;
    let (theirs, pt) = structure_of(&t, &b, 3);
    let mut tiles = tiles_around(pm, 1);
    tiles.extend(tiles_around(pt, 1));
    tiles.sort();
    tiles.dedup();
    let mut log = Vec::new();
    let dora = join_network(
        &server.invitation_for("dora"),
        "dora",
        &NetJoin {
            tiles: held(&tiles),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let mut packages = BTreeMap::new();
    pump(&dora, &mut log, |m| {
        if let ServerMsg::Tile(p) = m {
            packages.insert(p.tile, p.clone());
        }
        packages.len() == tiles.len()
    });
    let structures: BTreeSet<String> = packages
        .values()
        .flat_map(|p| p.structures.iter().map(|s| s.id.clone()))
        .collect();
    assert!(structures.contains(&mine), "her route's structure drawn");
    assert!(!structures.contains(&theirs), "B's structure not drawn");
    // The network: model A only, her route only.
    let info = log.iter().find_map(|m| match m {
        ServerMsg::Network(i) => Some(i.clone()),
        _ => None,
    });
    let info = info.unwrap();
    assert_eq!(
        info.models
            .iter()
            .map(|m| m.code.as_str())
            .collect::<Vec<_>>(),
        vec![a.as_str()]
    );
    assert!(
        info.routes.iter().all(|r| r.id == route),
        "{:?}",
        info.routes
    );
    // Her edit on her route passes; one of B is refused without its id.
    edit(
        &dora,
        &mut log,
        1,
        &a,
        moved(&mine, [pm[0] + 0.5, pm[1], pm[2]]),
    )
    .unwrap();
    let e = edit(
        &dora,
        &mut log,
        2,
        &b,
        moved(&theirs, [pt[0] + 0.5, pt[1], pt[2]]),
    )
    .unwrap_err();
    assert!(!e.contains(&theirs), "{e}");
    assert!(audit_has(&t, "refused", &b));
    // A structure of A off her route: refused (the unit put back).
    let off = s
        .line()
        .structures
        .iter()
        .find(|x| !d.structures.iter().any(|y| y.id == x.id))
        .map(|x| (x.id.clone(), x.position));
    if let Some((off, po)) = off {
        let e = edit(
            &dora,
            &mut log,
            3,
            &a,
            moved(&off, [po[0] + 0.5, po[1], po[2]]),
        )
        .unwrap_err();
        assert!(!e.contains(&off), "{e}");
        let again = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
        let now = again
            .line()
            .structures
            .iter()
            .find(|x| x.id == off)
            .unwrap()
            .position;
        assert_eq!(now, po, "refused: nothing kept");
    }
    // Following A whole: refused; a working set beyond her route: refused
    // without naming anything.
    let e = join_network(
        &server.invitation_for("dora"),
        "dora",
        &NetJoin {
            model: Some(a.clone()),
            file: Some(t.dir.join("dora-a.ohl")),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .err()
    .unwrap();
    assert!(
        e.contains("connected already") || e.contains("may not see all"),
        "{e}"
    );
    // Bob moves B's structure in her tile: she reads her tile again, no
    // change; the tile shows nothing of B.
    let bob = join_network(
        &server.invitation_for("bob"),
        "bob",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let mut bob_log = Vec::new();
    edit(
        &bob,
        &mut bob_log,
        1,
        &b,
        moved(&theirs, [pt[0] + 1.0, pt[1], pt[2]]),
    )
    .unwrap();
    let got = pump(&dora, &mut log, |m| {
        matches!(m, ServerMsg::TilesChanged { .. } | ServerMsg::Change(_))
    });
    assert!(
        matches!(got.last(), Some(ServerMsg::TilesChanged { .. })),
        "{:?}",
        got.last()
    );
    // A catch-up from the start: no change of B as it is.
    dora.link
        .send(ClientMsg::Catch {
            since: crate::Position::default(),
        })
        .unwrap();
    let got = pump(&dora, &mut log, |m| matches!(m, ServerMsg::CatchUp { .. }));
    if let Some(ServerMsg::CatchUp { changes, .. }) = got.last() {
        assert!(changes.iter().all(|c| c.model != b), "{changes:?}");
    }
    // A route detail and a pick of B's route: refused, nothing named.
    let (rb, _, _) = route_of(&t, &b);
    dora.link
        .send(ClientMsg::RouteDetail {
            model: b.clone(),
            route: rb.clone(),
        })
        .unwrap();
    pump(&dora, &mut log, |m| {
        matches!(m, ServerMsg::RouteDetail { .. })
    });
    dora.link
        .send(ClientMsg::Pick {
            id: 7,
            model: b.clone(),
            picks: vec![ohl_session::working_set::Pick::Route { route: rb.clone() }],
        })
        .unwrap();
    let got = pump(&dora, &mut log, |m| matches!(m, ServerMsg::Extent { .. }));
    assert!(
        matches!(got.last(), Some(ServerMsg::Extent { result: Err(_), .. })),
        "{:?}",
        got.last()
    );
    idle(&dora, &mut log, 300);
    let after = hidden_for(&t, "dora");
    no_leak(&log, &before, &after, "dora");
    drop((dora, bob));
    server.stop();
}

#[test]
fn grants_denies_and_groups_while_connected() {
    let Some(t) = test_network("groups", 160) else {
        return;
    };
    let a = t.codes[0].clone();
    admin(
        &t,
        R::AddUser {
            name: "dora".into(),
            display: None,
        },
    )
    .unwrap();
    admin(
        &t,
        R::AddGroup {
            name: "crew".into(),
            note: String::new(),
        },
    )
    .unwrap();
    admin(
        &t,
        R::AddMember {
            group: "crew".into(),
            user: "dora".into(),
        },
    )
    .unwrap();
    let (s1, p1) = structure_of(&t, &a, 1);
    let (s2, p2) = structure_of(&t, &a, 6);
    // A deny first, then the grant: the order changes nothing.
    let d = assign(
        &t,
        "user:dora",
        None,
        true,
        Scope {
            object: Some(s2.clone()),
            ..Scope::default()
        },
    );
    assign(&t, "group:crew", Some("designer"), false, Scope::line(&a));
    // Only a network admin sets a deny: a line admin may not.
    add_user(
        &Network::connect(&t.url).unwrap(),
        "lena",
        "line_admin",
        Scope::line(&a),
    );
    let net = Network::connect(&t.url).unwrap();
    let e = net
        .rights_command(
            "lena",
            &R::Assign {
                subject: Subject::User("bob".into()),
                role: None,
                deny: true,
                scope: Scope::line(&a),
                note: String::new(),
            },
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("network admins only"), "{e}");
    // Nor give rights on another line.
    let e = net
        .rights_command(
            "lena",
            &R::Assign {
                subject: Subject::User("bob".into()),
                role: Some("viewer".into()),
                deny: false,
                scope: Scope::line(&t.codes[1]),
                note: String::new(),
            },
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("does not manage"), "{e}");
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let mut log = Vec::new();
    let mut tiles = tiles_around(p1, 1);
    tiles.extend(tiles_around(p2, 1));
    tiles.sort();
    tiles.dedup();
    let dora = join_network(
        &server.invitation_for("dora"),
        "dora",
        &NetJoin {
            tiles: held(&tiles),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let mut n = 0;
    pump(&dora, &mut log, |m| {
        n += usize::from(matches!(m, ServerMsg::Tile(_)));
        n == tiles.len()
    });
    edit(
        &dora,
        &mut log,
        1,
        &a,
        moved(&s1, [p1[0] + 0.5, p1[1], p1[2]]),
    )
    .unwrap();
    let e = edit(
        &dora,
        &mut log,
        2,
        &a,
        moved(&s2, [p2[0] + 0.5, p2[1], p2[2]]),
    )
    .unwrap_err();
    assert!(!e.contains(&s2), "the denied structure not named: {e}");
    // The deny taken away (while she is connected): she is let go, and
    // may edit it after joining again.
    admin(&t, R::Unassign { id: d }).unwrap();
    let got = pump(
        &dora,
        &mut log,
        |m| matches!(m, ServerMsg::Notice { text } if text.contains("rights") || text.contains("closed")),
    );
    assert!(!got.is_empty());
    let e = edit(
        &dora,
        &mut log,
        3,
        &a,
        moved(&s1, [p1[0] + 1.0, p1[1], p1[2]]),
    );
    assert!(e.is_err(), "a client let go asks no more: {e:?}");
    drop(dora);
    let peers = Instant::now();
    while server.peers().iter().any(|p| p.user == "dora") {
        assert!(peers.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(50));
    }
    let dora = join_network(
        &server.invitation_for("dora"),
        "dora",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    edit(
        &dora,
        &mut log,
        4,
        &a,
        moved(&s2, [p2[0] + 0.5, p2[1], p2[2]]),
    )
    .unwrap();
    // Taken out of the group while connected: let go; nothing after.
    admin(
        &t,
        R::RemoveMember {
            group: "crew".into(),
            user: "dora".into(),
        },
    )
    .unwrap();
    // The next job of anyone reads the rights again: her edit refused.
    let e = edit(
        &dora,
        &mut log,
        5,
        &a,
        moved(&s1, [p1[0], p1[1] + 0.5, p1[2]]),
    );
    assert!(e.is_err(), "{e:?}");
    drop(dora);
    let peers = Instant::now();
    while server.peers().iter().any(|p| p.user == "dora") {
        assert!(peers.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(50));
    }
    // She joins again (a user still) and sees nothing of the network.
    let mut log2 = Vec::new();
    let dora = join_network(
        &server.invitation_for("dora"),
        "dora",
        &NetJoin {
            tiles: held(&tiles),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let mut n = 0;
    pump(&dora, &mut log2, |m| {
        n += usize::from(matches!(m, ServerMsg::Tile(_)));
        n == tiles.len()
    });
    for m in &log2 {
        match m {
            ServerMsg::Network(i) => assert!(i.models.is_empty() && i.routes.is_empty(), "{i:?}"),
            ServerMsg::Tile(p) => assert!(
                p.structures.is_empty() && p.wires.is_empty() && p.routes.is_empty(),
                "{p:?}"
            ),
            _ => {}
        }
    }
    // Disabled while connected: let go; joining again refused.
    admin(
        &t,
        R::SetActive {
            name: "dora".into(),
            active: false,
        },
    )
    .unwrap();
    let got = pump(
        &dora,
        &mut log2,
        |m| matches!(m, ServerMsg::Notice { text } if text.contains("rights") || text.contains("closed")),
    );
    assert!(!got.is_empty());
    drop(dora);
    let peers = Instant::now();
    while server.peers().iter().any(|p| p.user == "dora") {
        assert!(peers.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        join_network(
            &server.invitation_for("dora"),
            "dora",
            &NetJoin::default(),
            &mut |_| {}
        )
        .is_err()
    );
    // The audit: the rights changes, before and after.
    let rows = Network::connect(&t.url).unwrap().audit_rows().unwrap();
    for action in [
        "rights.assign",
        "rights.unassign",
        "rights.remove_member",
        "rights.set_active",
    ] {
        assert!(rows.iter().any(|r| r[2] == action), "{action}");
    }
    server.stop();
}

#[test]
fn see_as_a_user() {
    let Some(t) = test_network("seeas", 160) else {
        return;
    };
    let (a, b) = (t.codes[0].clone(), t.codes[1].clone());
    add_user(
        &Network::connect(&t.url).unwrap(),
        "eve",
        "viewer",
        Scope::line(&b),
    );
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let mut log = Vec::new();
    let (sa, pa) = structure_of(&t, &a, 2);
    let tiles = tiles_around(pa, 1);
    let admin_as_eve = join_network(
        &server.invitation_for("admin"),
        "admin",
        &NetJoin {
            tiles: held(&tiles),
            see_as: Some("eve".into()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let mut n = 0;
    pump(&admin_as_eve, &mut log, |m| {
        n += usize::from(matches!(m, ServerMsg::Tile(_)));
        n == tiles.len()
    });
    let info = log
        .iter()
        .find_map(|m| match m {
            ServerMsg::Network(i) => Some(i.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        info.models
            .iter()
            .map(|m| m.code.as_str())
            .collect::<Vec<_>>(),
        vec![b.as_str()]
    );
    assert!(
        log.iter().all(|m| match m {
            ServerMsg::Tile(p) => p.structures.iter().all(|s| s.model == b && s.id != sa),
            _ => true,
        }),
        "A's structures not drawn"
    );
    let e = edit(&admin_as_eve, &mut log, 1, &a, moved(&sa, pa)).unwrap_err();
    assert!(e.contains("seeing as"), "{e}");
    assert!(audit_has(&t, "see_as", "eve"));
    drop(admin_as_eve);
    // Who manages no rights may not see as another.
    let e = join_network(
        &server.invitation_for("vic"),
        "vic",
        &NetJoin {
            see_as: Some("eve".into()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .err()
    .unwrap();
    assert!(e.contains("manages no rights"), "{e}");
    server.stop();
}

/// A tiny deterministic random source (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The property (ADR-0013 phase 7): over random assignments (a line, a
/// route, a part of one, a region, a kind on a line, with a deny now and
/// then), random edits by others and the user's own, tiles, catch-ups,
/// route details and picks — no message to the user holds the id or the
/// place of an object the user may not see.
#[test]
fn property_no_message_holds_a_hidden_id_or_place() {
    let Some(t) = test_network("leak", 200) else {
        return;
    };
    add_user(
        &Network::connect(&t.url).unwrap(),
        "pat",
        "viewer",
        Scope::line(&t.codes[0]),
    );
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let bob = join_network(
        &server.invitation_for("bob"),
        "bob",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let mut bob_log = Vec::new();
    let mut r = Rng(0x7a7e_1eaf);
    let mut ids: Vec<i64> = Vec::new();
    let mut next_id = 100;
    for round in 0..6 {
        // Pat's rights this round.
        for id in ids.drain(..) {
            admin(&t, R::Unassign { id }).unwrap();
        }
        let code = t.codes[r.below(t.codes.len())].clone();
        let (route, from, to) = route_of(&t, &code);
        let (sx, px) = structure_of(&t, &code, r.below(40));
        let roles = ["viewer", "designer", "checker"];
        let role = roles[r.below(roles.len())];
        let scope = match r.below(5) {
            0 => Scope::line(&code),
            1 => Scope {
                route: Some(route.clone()),
                ..Scope::default()
            },
            2 => Scope {
                route: Some(route.clone()),
                from_structure: Some(from.clone()),
                to_structure: Some(to.clone()),
                ..Scope::default()
            },
            3 => Scope {
                region: Some(vec![
                    [px[0] - 600.0, px[1] - 600.0],
                    [px[0] + 600.0, px[1] - 600.0],
                    [px[0] + 600.0, px[1] + 600.0],
                    [px[0] - 600.0, px[1] + 600.0],
                ]),
                ..Scope::default()
            },
            _ => Scope {
                line: Some(code.clone()),
                kind: Some("structure".into()),
                ..Scope::default()
            },
        };
        ids.push(assign(&t, "user:pat", Some(role), false, scope));
        if r.below(3) == 0 {
            ids.push(assign(
                &t,
                "user:pat",
                None,
                true,
                Scope {
                    object: Some(sx.clone()),
                    ..Scope::default()
                },
            ));
        }
        let before = hidden_for(&t, "pat");
        let start = {
            let (seq, step) = Network::connect(&t.url).unwrap().position().unwrap();
            crate::Position {
                seq,
                step: step as u32,
            }
        };
        // Pat views tiles around two structures (one of the round's line,
        // one anywhere).
        let other = t.codes[r.below(t.codes.len())].clone();
        let (so, po) = structure_of(&t, &other, r.below(40));
        let mut tiles = tiles_around(px, 1);
        tiles.extend(tiles_around(po, 1));
        tiles.sort();
        tiles.dedup();
        let mut log = Vec::new();
        let pat = join_network(
            &server.invitation_for("pat"),
            "pat",
            &NetJoin {
                tiles: held(&tiles),
                ..NetJoin::default()
            },
            &mut |_| {},
        )
        .unwrap();
        let mut n = 0;
        pump(&pat, &mut log, |m| {
            n += usize::from(matches!(m, ServerMsg::Tile(_)));
            n == tiles.len()
        });
        // Bob moves structures in her tiles; she edits one herself.
        for (s, p, c) in [(&sx, px, &code), (&so, po, &other)] {
            next_id += 1;
            let _ = edit(
                &bob,
                &mut bob_log,
                next_id,
                c,
                moved(s, [p[0] + 0.3, p[1], p[2]]),
            );
            next_id += 1;
            let _ = edit(
                &pat,
                &mut log,
                next_id,
                c,
                moved(s, [p[0] + 0.6, p[1], p[2]]),
            );
        }
        // Phase 11: branches over a working set — Bob's (Pat in its team),
        // worked on, committed and merged into main (the other routes'
        // owners told), and Pat's own try at one.
        let ws = format!("ws-{round}");
        let on = |line: &str, op: Op| (line.to_owned(), op);
        for (line, op) in [
            on(
                "main",
                Op::Commit {
                    message: format!("round {round}"),
                },
            ),
            on(
                "main",
                Op::CreateWorkingSetBranch {
                    name: ws.clone(),
                    from: "main".into(),
                    routes: vec![route.clone()],
                    team: vec!["pat".into()],
                },
            ),
            on(&ws, moved(&sx, [px[0] + 0.9, px[1], px[2]])),
            on(
                &ws,
                Op::Commit {
                    message: "on the branch".into(),
                },
            ),
        ] {
            next_id += 1;
            let _ = edit_on(&bob, &mut bob_log, next_id, &code, &line, op);
        }
        for (line, op) in [
            on(&ws, moved(&sx, [px[0] + 1.2, px[1], px[2]])),
            on(
                &ws,
                Op::Commit {
                    message: "pat's".into(),
                },
            ),
            on(
                "main",
                Op::CreateWorkingSetBranch {
                    name: format!("pat-{round}"),
                    from: "main".into(),
                    routes: vec![route.clone()],
                    team: Vec::new(),
                },
            ),
        ] {
            next_id += 1;
            let _ = edit_on(&pat, &mut log, next_id, &code, &line, op);
        }
        next_id += 1;
        let _ = edit_on(
            &bob,
            &mut bob_log,
            next_id,
            &code,
            "main",
            Op::Merge {
                other: ws.clone(),
                message: None,
                choices: Default::default(),
            },
        );
        // A catch-up, a route detail, a pick, a working set of another line.
        pat.link.send(ClientMsg::Catch { since: start }).unwrap();
        pump(&pat, &mut log, |m| matches!(m, ServerMsg::CatchUp { .. }));
        let (ro, _, _) = route_of(&t, &other);
        pat.link
            .send(ClientMsg::RouteDetail {
                model: other.clone(),
                route: ro.clone(),
            })
            .unwrap();
        pump(&pat, &mut log, |m| {
            matches!(m, ServerMsg::RouteDetail { .. })
        });
        next_id += 1;
        pat.link
            .send(ClientMsg::Pick {
                id: next_id,
                model: other.clone(),
                picks: vec![ohl_session::working_set::Pick::Route { route: ro.clone() }],
            })
            .unwrap();
        pump(&pat, &mut log, |m| matches!(m, ServerMsg::Extent { .. }));
        // The tiles read again (at what she holds).
        idle(&pat, &mut log, 300);
        let after = hidden_for(&t, "pat");
        no_leak(&log, &before, &after, &format!("round {round}"));
        drop(pat);
        let peers = Instant::now();
        while server.peers().iter().any(|p| p.user == "pat") {
            assert!(peers.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    drop(bob);
    server.stop();
}

/// Phase 11: branches over a working set on a network — made by a
/// designer of a route (not a branch of the whole line), worked on by its
/// maker and team only, committed and merged into main by what they
/// touch, deleted by its maker; a commit records only what its user may
/// edit; a tag still needs the line whole; a designer of a part of a
/// route makes new objects inside it (the owner's question 1 of phase 7).
#[test]
fn branches_over_working_sets_on_a_network() {
    let Some(t) = test_network("branches", 240) else {
        return;
    };
    let a = t.codes[0].clone();
    let (route, from, to) = route_of(&t, &a);
    {
        let net = Network::connect(&t.url).unwrap();
        let on_route = Scope {
            route: Some(route.clone()),
            ..Scope::default()
        };
        add_user(&net, "dora", "designer", on_route.clone());
        add_user(&net, "tom", "designer", on_route);
        add_user(
            &net,
            "pia",
            "designer",
            Scope {
                route: Some(route.clone()),
                from_structure: Some(from.clone()),
                to_structure: Some(to.clone()),
                ..Scope::default()
            },
        );
    }
    let net = Network::connect(&t.url).unwrap();
    let mut s = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
    let d = s.route_detail(&route).unwrap();
    let ids: Vec<String> = d.structures.iter().map(|x| x.id.clone()).collect();
    let at = |id: &str| s.line().structure(id).unwrap().position;
    let (mine, pm) = (ids[0].clone(), at(&ids[0]));
    // A structure of the part, not one of its ends; one of the route
    // outside the part.
    let (inner, pi) = (ids[2].clone(), at(&ids[2]));
    let (outer, po) = (ids[8].clone(), at(&ids[8]));
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let join = |u: &str| {
        join_network(
            &server.invitation_for(u),
            u,
            &NetJoin::default(),
            &mut |_| {},
        )
        .unwrap()
    };
    let (bob, dora, tom, pia) = (join("bob"), join("dora"), join("tom"), join("pia"));
    let mut log = Vec::new();
    let mut n = 0u64;
    let mut go = |j: &Joined, line: &str, op: Op| {
        n += 1;
        edit_on(j, &mut log, n, &a, line, op)
    };
    // Main has no revision yet: a branch from it would record every
    // object — only who may edit them all (Bob) commits it.
    let ws = |name: &str, team: Vec<String>| Op::CreateWorkingSetBranch {
        name: name.into(),
        from: "main".into(),
        routes: vec![route.clone()],
        team,
    };
    let e = go(&dora, "main", ws("early", vec![])).unwrap_err();
    assert!(e.contains("may not commit"), "{e}");
    go(
        &bob,
        "main",
        Op::Commit {
            message: "first".into(),
        },
    )
    .unwrap();
    // A branch of the whole line: refused (the line whole); over her
    // route: made.
    let e = go(
        &dora,
        "main",
        Op::CreateLine {
            name: "whole".into(),
            from: "main".into(),
        },
    )
    .unwrap_err();
    assert!(e.contains("whole"), "{e}");
    go(&dora, "main", ws("dora-ws", vec!["pia".into()])).unwrap();
    assert!(
        go(
            &dora,
            "main",
            Op::CreateWorkingSetBranch {
                name: "nowhere".into(),
                from: "main".into(),
                routes: vec!["no-such-route".into()],
                team: vec![],
            },
        )
        .is_err()
    );
    // Her move on it and her commit; Pia (its team) too; Tom not.
    go(&dora, "dora-ws", moved(&mine, [pm[0] + 0.4, pm[1], pm[2]])).unwrap();
    go(
        &dora,
        "dora-ws",
        Op::Commit {
            message: "mine".into(),
        },
    )
    .unwrap();
    go(&pia, "dora-ws", moved(&inner, [pi[0] + 0.2, pi[1], pi[2]])).unwrap();
    let e = go(&tom, "dora-ws", moved(&mine, [pm[0] + 0.8, pm[1], pm[2]])).unwrap_err();
    assert!(e.contains("dora's"), "{e}");
    // Deleting or renaming it: its maker only.
    assert!(
        go(
            &pia,
            "main",
            Op::DeleteLine {
                name: "dora-ws".into(),
            },
        )
        .is_err()
    );
    go(
        &pia,
        "dora-ws",
        Op::Commit {
            message: "pia's".into(),
        },
    )
    .unwrap();
    // Not merged into main by Tom (not its maker nor of its team).
    let e = go(
        &tom,
        "main",
        Op::Merge {
            other: "dora-ws".into(),
            message: None,
            choices: Default::default(),
        },
    )
    .unwrap_err();
    assert!(e.contains("dora's"), "{e}");
    // Merged into main by Dora: what the merge changes there is hers to
    // change.
    go(
        &dora,
        "main",
        Op::Merge {
            other: "dora-ws".into(),
            message: None,
            choices: Default::default(),
        },
    )
    .unwrap();
    let main_now = Session::open_on(Box::new(net.store(&a).unwrap()), &a).unwrap();
    assert_eq!(
        main_now.line().structure(&mine).unwrap().position[0],
        pm[0] + 0.4
    );
    assert_eq!(
        main_now.line().structure(&inner).unwrap().position[0],
        pi[0] + 0.2
    );
    // A tag still needs the line whole.
    let e = go(
        &dora,
        "main",
        Op::Tag {
            name: "issued".into(),
            at: None,
            note: String::new(),
        },
    )
    .unwrap_err();
    assert!(e.contains("whole"), "{e}");
    // A commit records only what its user may edit: Dora moved a
    // structure outside Pia's part, Pia may not commit it; Dora may.
    go(&dora, "main", moved(&outer, [po[0] + 0.3, po[1], po[2]])).unwrap();
    let e = go(
        &pia,
        "main",
        Op::Commit {
            message: "not hers".into(),
        },
    )
    .unwrap_err();
    // (The structure lies outside what she sees: the refusal names nothing.)
    assert!(e.contains("no access") && !e.contains(&outer), "{e}");
    go(
        &dora,
        "main",
        Op::Commit {
            message: "hers".into(),
        },
    )
    .unwrap();
    // The owner's question 1 of phase 7: Pia makes a new structure inside
    // her part (one of its structures copied a metre along); outside it,
    // refused.
    let copy = |id: &str, new: &str, dx: f64| {
        let mut v = serde_json::to_value(s.line().structure(id).unwrap()).unwrap();
        v["id"] = serde_json::Value::String(new.into());
        let p = v["position"][0].as_f64().unwrap();
        v["position"][0] = serde_json::json!(p + dx);
        Op::Do {
            command: ohl_session::Command::Put {
                kind: Kind::Structure,
                id: new.into(),
                value: v,
                ord: None,
            },
        }
    };
    go(&pia, "main", copy(&inner, "P11-NEW", 1.0)).unwrap();
    assert!(go(&pia, "main", copy(&outer, "P11-OUT", 1.0)).is_err());
    // Deleted by its maker.
    go(
        &dora,
        "main",
        Op::DeleteLine {
            name: "dora-ws".into(),
        },
    )
    .unwrap();
    drop((bob, dora, tom, pia));
    server.stop();
}
