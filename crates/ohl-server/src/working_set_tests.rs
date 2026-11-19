// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Working sets on a network's server (phase 6), on a throwaway
//! PostgreSQL cluster: a part of a route picked and shown before it is
//! confirmed (snapped out to whole tension sections), saved by name; a
//! desktop joining with it gets its partial replica as a project file,
//! then the rows of every change touching it — before the answer to its
//! own edits, none for changes elsewhere; random edits by another user
//! leave the replica equal to the server's model at every settled point;
//! a route added comes whole; a replica joining again sends what it holds
//! and gets only what differs. Without PostgreSQL's binaries the test
//! says so and passes.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use ohl_session::working_set::{Pick, WorkingSetSpec};
use ohl_session::{Kind, Session};
use ohl_store_pg::Network;

use crate::client::{Incoming, Joined, NetJoin, hold, join_network, take_rows};
use crate::network_tests::{local, moved, net_options, test_network};
use crate::server::Server;
use crate::{ClientMsg, Op, ServerMsg};

/// Takes messages until `until` says stop (10 s at most), rows into
/// `replica` when given.
fn pump(
    j: &Joined,
    mut replica: Option<&mut Session>,
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
                if let (ServerMsg::Rows(r), Some(s)) = (&m, replica.as_deref_mut()) {
                    take_rows(s, "carol", r).unwrap();
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

/// Objects held, each with its hash.
type Held = BTreeMap<(Kind, String), u64>;

/// What the server's model holds of working set `picks`: each object of
/// its extent with its hash, and the working set.
fn truth(url: &str, code: &str, picks: &[Pick]) -> (Held, BTreeSet<(Kind, String)>) {
    let net = Network::connect(url).unwrap();
    let mut s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
    let e = s.extent_of(picks);
    let held = s
        .held()
        .into_iter()
        .filter(|(k, _)| e.keep.contains(k))
        .collect();
    (held, e.wanted)
}

/// An edit of model `model` by `j`; waits for its answer.
fn edit(j: &mut Joined, id: &mut u64, model: &str, op: Op) -> Result<String, String> {
    *id += 1;
    let k = *id;
    j.link
        .send(ClientMsg::RequestOn {
            id: k,
            model: model.into(),
            op,
            line: "main".into(),
        })
        .unwrap();
    let got = pump(
        j,
        None,
        |m| matches!(m, ServerMsg::Response { id: i, .. } if *i == k),
    );
    match got.last() {
        Some(ServerMsg::Response { result, .. }) => result.clone(),
        _ => unreachable!(),
    }
}

/// Where the structures of model `code` stand.
fn positions(url: &str, code: &str) -> BTreeMap<String, [f64; 3]> {
    let net = Network::connect(url).unwrap();
    let s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
    s.line()
        .structures
        .iter()
        .map(|x| (x.id.clone(), x.position))
        .collect()
}

/// Takes everything up to a heartbeat at the network's position now or
/// later (every change made before has arrived then).
fn settle(j: &Joined, r: &mut Session, url: &str) -> Vec<ServerMsg> {
    let (seq, step) = Network::connect(url).unwrap().position().unwrap();
    let at = crate::Position {
        seq,
        step: step as u32,
    };
    pump(
        j,
        Some(r),
        |m| matches!(m, ServerMsg::Heartbeat { at: h, .. } if *h >= at),
    )
}

fn same(r: &Session, url: &str, code: &str, picks: &[Pick], at: &str) {
    let (held, wanted) = truth(url, code, picks);
    let mine = r.held();
    assert_eq!(
        mine.keys().collect::<Vec<_>>(),
        held.keys().collect::<Vec<_>>(),
        "{at}: the same objects"
    );
    assert!(mine == held, "{at}: as the server holds them");
    assert_eq!(r.part(), Some(&wanted), "{at}: the working set");
    r.check_kept().unwrap();
}

#[test]
fn a_working_set_is_a_partial_replica() {
    let Some(t) = test_network("ws", 240) else {
        return;
    };
    let server = Server::start_network(local("wxyz2345"), net_options(&t.url)).unwrap();
    let a = t.codes[0].clone();
    // Carol looks around first: the network, a route's structures.
    let browse = join_network(
        &server.invitation_for("carol"),
        "carol",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let got = pump(&browse, None, |m| matches!(m, ServerMsg::Network(_)));
    let ServerMsg::Network(info) = got.last().unwrap() else {
        unreachable!()
    };
    let route = info
        .routes
        .iter()
        .find(|r| r.model == a)
        .expect("a route of A")
        .id
        .clone();
    browse
        .link
        .send(ClientMsg::RouteDetail {
            model: a.clone(),
            route: route.clone(),
        })
        .unwrap();
    let got = pump(&browse, None, |m| {
        matches!(m, ServerMsg::RouteDetail { .. })
    });
    let ServerMsg::RouteDetail {
        result: Ok(detail), ..
    } = got.last().unwrap()
    else {
        panic!("{:?}", got.last())
    };
    let n = detail.structures.len();
    assert!(n >= 8, "{n} structures");
    let st = |k: usize| detail.structures[k].station.unwrap();
    // A part of the route: between its 3rd and 5th structures.
    let part = vec![Pick::Stations {
        route: route.clone(),
        alignment: None,
        from: st(2) + 1.0,
        to: st(4) - 1.0,
    }];
    let ask = |id: u64, picks: Vec<Pick>| {
        browse
            .link
            .send(ClientMsg::Pick {
                id,
                model: a.clone(),
                picks,
            })
            .unwrap();
        let got = pump(
            &browse,
            None,
            |m| matches!(m, ServerMsg::Extent { id: i, .. } if *i == id),
        );
        match got.last() {
            Some(ServerMsg::Extent { result, .. }) => result.clone(),
            _ => unreachable!(),
        }
    };
    let shown = ask(1, part.clone()).unwrap();
    assert!(shown.structures >= 3 && shown.structures < n, "{shown:?}");
    let r = &shown.routes[0];
    assert!(
        r.from.unwrap() <= st(2) + 1.0 && r.to.unwrap() >= st(4) - 1.0,
        "snapped out"
    );
    let whole = ask(
        2,
        vec![Pick::Route {
            route: route.clone(),
        }],
    )
    .unwrap();
    assert_eq!(whole.structures, n);
    let e = ask(
        3,
        vec![Pick::Route {
            route: "nowhere".into(),
        }],
    )
    .unwrap_err();
    assert!(e.contains("no route nowhere"), "{e}");
    // Saved by name, listed, deleted.
    let spec = WorkingSetSpec {
        name: "my part".into(),
        model: a.clone(),
        picks: part.clone(),
    };
    browse
        .link
        .send(ClientMsg::SaveWorkingSet { spec: spec.clone() })
        .unwrap();
    let got = pump(&browse, None, |m| {
        matches!(m, ServerMsg::WorkingSets { .. })
    });
    let ServerMsg::WorkingSets { sets } = got.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].spec, spec);
    assert_eq!(sets[0].routes, std::slice::from_ref(&route));
    browse
        .link
        .send(ClientMsg::DeleteWorkingSet {
            name: "my part".into(),
        })
        .unwrap();
    let got = pump(&browse, None, |m| {
        matches!(m, ServerMsg::WorkingSets { .. })
    });
    assert!(matches!(got.last(), Some(ServerMsg::WorkingSets { sets }) if sets.is_empty()));
    drop(browse);

    // Carol opens the part: its replica comes as a project file.
    let file = t.dir.join("carol.ohl");
    let started = Instant::now();
    let d = join_network(
        &server.invitation_for("carol"),
        "carol",
        &NetJoin {
            working_set: Some(spec.clone()),
            file: Some(file.clone()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let loaded = started.elapsed();
    let mut r = Session::open(&file).unwrap();
    assert_eq!(r.working_set_spec().unwrap(), Some(spec.clone()));
    assert_eq!(r.file_meta("network.line").unwrap().as_deref(), Some(&*a));
    same(&r, &t.url, &a, &part, "joined");
    assert!(loaded < Duration::from_secs(10));
    let ws_structures: Vec<(String, [f64; 3])> = r
        .part()
        .unwrap()
        .iter()
        .filter(|(k, _)| *k == Kind::Structure)
        .map(|(_, id)| (id.clone(), r.line().structure(id).unwrap().position))
        .collect();
    // Bob edits: a structure of the working set — its rows come; one of
    // the route outside it, held by nobody's replica — none; another
    // model — none.
    let mut bob = join_network(
        &server.invitation_for("bob"),
        "bob",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let mut id = 0;
    let at = positions(&t.url, &a);
    let (s0, p0) = ws_structures[0].clone();
    edit(
        &mut bob,
        &mut id,
        &a,
        moved(&s0, [p0[0] + 0.5, p0[1], p0[2]]),
    )
    .unwrap();
    let got = pump(&d, Some(&mut r), |m| matches!(m, ServerMsg::Rows(_)));
    let ServerMsg::Rows(rows) = got.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(rows.user.as_deref(), Some("bob"));
    assert!(rows.rows.puts.iter().any(|p| p.id == s0));
    same(&r, &t.url, &a, &part, "bob's move");
    let far = detail.structures[n - 1].clone();
    let held_far = r.held().contains_key(&(Kind::Structure, far.id.clone()));
    let pf = at[&far.id];
    edit(
        &mut bob,
        &mut id,
        &a,
        moved(&far.id, [pf[0] + 0.5, pf[1], pf[2]]),
    )
    .unwrap();
    let b = t.codes[t.codes.len() - 1].clone();
    let (sb, pb) = crate::network_tests::structure_of(&t, &b, 2);
    edit(
        &mut bob,
        &mut id,
        &b,
        moved(&sb, [pb[0] + 0.5, pb[1], pb[2]]),
    )
    .unwrap();
    // A heartbeat after both: nothing came unless the far one is held
    // (an alignment point tied to it makes it context).
    let got = settle(&d, &mut r, &t.url);
    let rows_n = got
        .iter()
        .filter(|m| matches!(m, ServerMsg::Rows(_)))
        .count();
    assert_eq!(rows_n, usize::from(held_far), "rows only for what it holds");
    same(&r, &t.url, &a, &part, "elsewhere");
    // Carol's own edit: its rows come before its answer.
    let (s1, p1) = ws_structures[ws_structures.len() - 1].clone();
    id += 1;
    d.link
        .send(ClientMsg::RequestOn {
            id,
            model: a.clone(),
            op: moved(&s1, [p1[0], p1[1] + 0.25, p1[2]]),
            line: "main".into(),
        })
        .unwrap();
    let k = id;
    let got = pump(
        &d,
        Some(&mut r),
        |m| matches!(m, ServerMsg::Response { id: i, .. } if *i == k),
    );
    let at_rows = got.iter().position(|m| matches!(m, ServerMsg::Rows(_)));
    assert!(at_rows.is_some(), "rows before the answer: {got:?}");
    same(&r, &t.url, &a, &part, "carol's move");
    // Random edits by Bob, inside and outside, undone now and then: the
    // replica equal to the model once settled.
    let mut rng = 11u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for step in 0..16 {
        let op = if next() % 5 == 0 {
            Op::Undo
        } else {
            let k = (next() % n as u64) as usize;
            let s = &detail.structures[k].id;
            let p = at[s];
            let dx = (next() % 7) as f64 * 0.1 - 0.3;
            moved(s, [p[0] + dx, p[1], p[2]])
        };
        let _ = edit(&mut bob, &mut id, &a, op);
        let _ = step;
    }
    settle(&d, &mut r, &t.url);
    same(&r, &t.url, &a, &part, "random edits");
    // A library file saved and removed (changes that do not say which
    // objects they changed): the replica follows.
    edit(
        &mut bob,
        &mut id,
        &a,
        Op::PutLibrary {
            kind: "weather".into(),
            name: "extra.ohlwc".into(),
            text: "# a test file\n".into(),
        },
    )
    .unwrap();
    settle(&d, &mut r, &t.url);
    assert!(
        r.held()
            .contains_key(&(Kind::Library, "weather/extra.ohlwc".into()))
    );
    same(&r, &t.url, &a, &part, "a library file saved");
    edit(
        &mut bob,
        &mut id,
        &a,
        Op::DeleteLibrary {
            kind: "weather".into(),
            name: "extra.ohlwc".into(),
        },
    )
    .unwrap();
    settle(&d, &mut r, &t.url);
    same(&r, &t.url, &a, &part, "a library file removed");
    // A working set on another line is refused; the whole route added
    // comes, nothing taken away.
    d.link
        .send(ClientMsg::SetWorkingSet {
            spec: WorkingSetSpec {
                model: b.clone(),
                ..spec.clone()
            },
        })
        .unwrap();
    let got = pump(&d, Some(&mut r), |m| matches!(m, ServerMsg::Notice { .. }));
    assert!(
        matches!(got.last(), Some(ServerMsg::Notice { text }) if text.contains("one line")),
        "{got:?}"
    );
    let mut bigger = part.clone();
    bigger.push(Pick::Route {
        route: route.clone(),
    });
    let before = r.held().len();
    d.link
        .send(ClientMsg::SetWorkingSet {
            spec: WorkingSetSpec {
                picks: bigger.clone(),
                ..spec.clone()
            },
        })
        .unwrap();
    let got = pump(&d, Some(&mut r), |m| matches!(m, ServerMsg::Rows(_)));
    let ServerMsg::Rows(rows) = got.last().unwrap() else {
        unreachable!()
    };
    assert!(rows.rows.deletes.is_empty());
    assert!(r.held().len() > before);
    assert_eq!(r.working_set_spec().unwrap().unwrap().picks, bigger);
    same(&r, &t.url, &a, &bigger, "the route added");
    // Carol leaves; Bob moves on; she joins again with what she holds and
    // gets only what differs.
    drop(d);
    for k in 0..3 {
        let s = &detail.structures[k * 2].id;
        let p = at[s];
        edit(&mut bob, &mut id, &a, moved(s, [p[0], p[1] + 0.75, p[2]])).unwrap();
    }
    let spec = r.working_set_spec().unwrap().unwrap();
    let d = join_network(
        &server.invitation_for("carol"),
        "carol",
        &NetJoin {
            working_set: Some(spec),
            held: true,
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    d.link.send(hold(&r)).unwrap();
    let got = pump(&d, Some(&mut r), |m| matches!(m, ServerMsg::Rows(_)));
    let ServerMsg::Rows(rows) = got.last().unwrap() else {
        unreachable!()
    };
    assert!(
        rows.rows.puts.len() <= 3 * 4 && !rows.rows.puts.is_empty(),
        "only what differs: {}",
        rows.rows.puts.len()
    );
    same(&r, &t.url, &a, &bigger, "joined again");
}

/// The context case solved ahead while the server is idle: every tile
/// holding objects made once and kept, then served from the cache; a
/// change makes the tiles it touched again; nothing reaches a client that
/// did not ask.
#[test]
fn the_idle_solve_makes_every_tile_once() {
    let Some(t) = test_network("idle", 120) else {
        return;
    };
    let server = Server::start_network(
        local("idle2345"),
        crate::network::NetOptions {
            idle_solve: true,
            idle_after: Duration::from_millis(30),
            ..net_options(&t.url)
        },
    )
    .unwrap();
    let stats = |server: &Server| -> serde_json::Value {
        serde_json::from_str(&server.probe(crate::network::Probe::Stats).unwrap()).unwrap()
    };
    let started = Instant::now();
    let first = loop {
        let s = stats(&server);
        if !s["idle_done_s"].is_null() {
            break s;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "{s}");
        std::thread::sleep(Duration::from_millis(50));
    };
    let tiles = first["tiles"].as_u64().unwrap();
    let built = first["idle_built"].as_u64().unwrap();
    assert!(built > 0 && built <= tiles, "{built} of {tiles}");
    assert_eq!(first["packages"].as_u64().unwrap(), built, "kept");
    assert_eq!(first["idle_left"].as_u64().unwrap(), 0);
    // A viewer gets them from the cache, and nothing it did not ask for.
    let mut bob = join_network(
        &server.invitation_for("bob"),
        "bob",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let (sid, p) = crate::network_tests::structure_of(&t, &t.codes[0], 1);
    let around = crate::network_tests::tiles_around(p, 1);
    bob.link
        .send(ClientMsg::Interest {
            routes: Vec::new(),
            tiles: around
                .iter()
                .map(|k| crate::HeldTile {
                    tile: *k,
                    rev: None,
                })
                .collect(),
        })
        .unwrap();
    let mut n = 0;
    let got = pump(&bob, None, |m| {
        if matches!(m, ServerMsg::Tile(_)) {
            n += 1;
        }
        n == around.len()
    });
    assert!(
        got.iter()
            .filter_map(|m| match m {
                ServerMsg::Tile(p) => Some(p.tile),
                _ => None,
            })
            .all(|k| around.contains(&k))
    );
    let s = stats(&server);
    let solved_before = s["solved"].as_u64().unwrap();
    assert_eq!(s["idle_built"].as_u64().unwrap(), built, "{s}");
    assert!(
        !s["served_from_cache_us"].as_array().unwrap().is_empty(),
        "{s}"
    );
    // A change: the tiles it touched made again while idle.
    let mut id = 0;
    edit(
        &mut bob,
        &mut id,
        &t.codes[0],
        moved(&sid, [p[0] + 0.5, p[1], p[2]]),
    )
    .unwrap();
    let started = Instant::now();
    loop {
        let s = stats(&server);
        if s["idle_left"].as_u64() == Some(0) && s["idle_built"].as_u64().unwrap() > built {
            assert!(s["solved"].as_u64().unwrap() > solved_before, "{s}");
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "{s}");
        std::thread::sleep(Duration::from_millis(50));
    }
}
