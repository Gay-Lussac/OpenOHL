// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-scoped`: phase 10 measured — checks on a working
//! set with their neighbours pulled in, computed on the server.
//!
//! The server in this process (the idle solve off); the first line with a
//! tower shared with another line (a twin) is the working set's: its
//! whole route checked for clearances (the other lines' wires and its
//! own beyond as targets), its field with the lines within 500 m, the
//! loads on the shared tower under each line's cases; each `runs` times
//! (the first reads the libraries), timed on the server and at the
//! client. Phase 13: then a move's latency while a whole route's
//! clearances run — the check on the authority (as before) and on the
//! checks' worker (adds the user `bench-mover`, a designer of the
//! network, made once). Every wait is bounded.

use std::time::{Duration, Instant};

use ohl_server::client::{Joined, NetJoin, join_network};
use ohl_server::network::NetOptions;
use ohl_server::server::{Options, Server};
use ohl_server::{ClientMsg, ScopedCheck, ScopedReport, ServerMsg};
use ohl_session::working_set::{Pick, WorkingSetSpec};
use ohl_store_pg::Network;
use serde_json::{Value, json};

use crate::network_bench::{pump, summary};

/// The rules of the clearance check measured (arbitrary distances for
/// measuring, not from a standard).
const RULES: &str = r#"
[[rule]]
name = "other lines"
target = "other_lines"
vertical = { by_voltage = [[150, 5.0], [420, 7.0]] }
radial = { by_voltage = [[150, 4.0], [420, 6.5]] }
[[rule]]
name = "phase to phase"
target = "same_line"
radial = "3 m"
"#;

fn ask(
    j: &mut Joined,
    id: u64,
    spec: &WorkingSetSpec,
    check: ScopedCheck,
) -> Result<(ScopedReport, f64), String> {
    let started = Instant::now();
    j.link
        .send(ClientMsg::Check {
            id,
            working_set: Some(spec.clone()),
            check,
        })
        .map_err(|e| e.to_string())?;
    let got = pump(j, Duration::from_secs(1800), |m| {
        matches!(m, ServerMsg::CheckResult { id: i, .. } if *i == id)
            || matches!(m, ServerMsg::Notice { .. })
    })?;
    let at_client = started.elapsed().as_secs_f64();
    match got.last() {
        Some(ServerMsg::CheckResult { result, .. }) => result.clone().map(|r| (*r, at_client)),
        other => Err(format!("no result: {other:?}")),
    }
}

fn measured(runs: Vec<(ScopedReport, f64)>) -> Value {
    let server: Vec<f64> = runs.iter().map(|(r, _)| r.seconds * 1e3).collect();
    let client: Vec<f64> = runs.iter().map(|(_, t)| t * 1e3).collect();
    let last = &runs.last().expect("runs").0;
    json!({
        "server_ms": summary(&server),
        "client_ms": summary(&client),
        "first_ms": server.first(),
        "status": last.status,
        "reach": last.reach,
        "results": last.results,
        "violations": last.violations,
        "pulled_lines": last.pulled.lines.len(),
        "pulled_objects": last.pulled.objects.len(),
        "read": last.pulled.read,
        "not_all_checked": last.not_all_checked.len(),
        "notes": last.notes.len(),
    })
}

/// Measures the scoped checks (see the module's notes); JSON.
pub fn bench(database: &str, user: &str, runs: usize) -> Result<Value, String> {
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = Network::connect(database).map_err(e)?;
    // The first line with a twin tower.
    let mut found = None;
    for l in net.lines().map_err(e)? {
        let twins = ohl_store_pg::import::twins(&net, &l.id, 0.5).map_err(e)?;
        if let Some(t) = twins.first() {
            found = Some((l.id.clone(), t.clone()));
            break;
        }
    }
    let (code, twin) = found.ok_or("no line with a tower shared with another line")?;
    let routes: Vec<String> = net
        .route_list()
        .map_err(e)?
        .into_iter()
        .filter(|(l, _)| *l == code)
        .map(|(_, r)| r)
        .collect();
    let route = routes.first().cloned().ok_or("no route")?;
    let structures = net.structures_on_route(&code, &route).map_err(e)?.len();
    drop(net);
    eprintln!("bench-scoped: line {code}, route {route} ({structures} structures)");
    let t = Instant::now();
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
            code: Some("bench-scoped-code".into()),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: database.to_owned(),
            idle_solve: false,
            ..NetOptions::default()
        },
    )?;
    let start_s = t.elapsed().as_secs_f64();
    let mut j = join_network(
        &server.invitation_for(user),
        user,
        &NetJoin::default(),
        &mut |_| {},
    )?;
    pump(&mut j, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![Pick::Route {
            route: route.clone(),
        }],
    };
    let mut id = 0;
    let mut run = |j: &mut Joined, check: ScopedCheck, n: usize| -> Result<Value, String> {
        let mut out = Vec::new();
        for _ in 0..n.max(1) {
            id += 1;
            out.push(ask(j, id, &spec, check.clone())?);
        }
        Ok(measured(out))
    };
    eprintln!("bench-scoped: clearances");
    let clearances = run(
        &mut j,
        ScopedCheck::Clearances {
            case: "10 °C + 300 Pa wind".into(),
            rules: RULES.into(),
        },
        runs,
    )?;
    eprintln!("bench-scoped: field");
    let field = run(
        &mut j,
        ScopedCheck::Fields {
            case: "40 °C".into(),
            distance: None,
            amps: Some(500.0),
            limit_ut: Some(1.0),
        },
        runs,
    )?;
    eprintln!("bench-scoped: structure loads on the shared tower");
    let loads = run(
        &mut j,
        ScopedCheck::StructureLoads {
            structures: vec![twin.structure.clone()],
        },
        runs,
    )?;
    drop(j);
    server.stop();
    eprintln!("bench-scoped: moves while a check runs (on the authority, then the worker)");
    let clearances_check = ScopedCheck::Clearances {
        case: "10 °C + 300 Pa wind".into(),
        rules: RULES.into(),
    };
    let mut during = serde_json::Map::new();
    for (name, worker) in [("on_the_authority", false), ("on_the_worker", true)] {
        during.insert(
            name.to_owned(),
            moves_during_check(database, user, worker, &spec, clearances_check.clone())?,
        );
    }
    Ok(json!({
        "line": code,
        "route": route,
        "structures": structures,
        "shared_tower": twin.structure,
        "twin": format!("{} {}", twin.other_line, twin.other),
        "server_start_s": start_s,
        "clearances_route": clearances,
        "field_route": field,
        "loads_shared_tower": loads,
        "moves_during_a_check": during,
    }))
}

/// A move's latency (sent, applied, answered) by another user, again and
/// again while `check` runs for `user` (the check on the checks' worker or
/// on the authority).
fn moves_during_check(
    database: &str,
    user: &str,
    worker: bool,
    spec: &WorkingSetSpec,
    check: ScopedCheck,
) -> Result<Value, String> {
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = Network::connect(database).map_err(e)?;
    let mover = "bench-mover";
    if !net.rights().map_err(e)?.users.contains_key(mover) {
        use ohl_store_pg::rights::RightsCommand as R;
        net.rights_command(
            user,
            &R::AddUser {
                name: mover.into(),
                display: None,
            },
        )
        .map_err(e)?;
        net.rights_command(
            user,
            &R::Assign {
                subject: ohl_session::rights::Subject::User(mover.into()),
                role: Some("designer".into()),
                deny: false,
                scope: ohl_session::rights::Scope::network(),
                note: "bench-scoped".into(),
            },
        )
        .map_err(e)?;
    }
    // A structure of another line than the checked one, moved and back.
    let other = net
        .lines()
        .map_err(e)?
        .into_iter()
        .map(|l| l.id)
        .rfind(|c| *c != spec.model)
        .ok_or("one line only")?;
    let s = ohl_session::Session::open_on(Box::new(net.store(&other).map_err(e)?), &other)
        .map_err(|x| x.to_string())?;
    let st = s.line().structures.first().ok_or("no structure")?;
    let (sid, p) = (st.id.clone(), st.position);
    drop(s);
    drop(net);
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
            code: Some("bench-scoped-code".into()),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: database.to_owned(),
            idle_solve: false,
            check_worker: worker,
            ..NetOptions::default()
        },
    )?;
    let mut checker = join_network(
        &server.invitation_for(user),
        user,
        &NetJoin::default(),
        &mut |_| {},
    )?;
    let mut moving = join_network(
        &server.invitation_for(mover),
        mover,
        &NetJoin::default(),
        &mut |_| {},
    )?;
    for j in [&mut checker, &mut moving] {
        pump(j, Duration::from_secs(120), |m| {
            matches!(m, ServerMsg::Network(_))
        })?;
    }
    // A first check: the libraries read (as the runs above).
    ask(&mut checker, 1, spec, check.clone())?;
    let mut id = 100;
    let mut move_once = |j: &mut Joined, k: usize| -> Result<f64, String> {
        id += 1;
        let dx = if k.is_multiple_of(2) { 0.1 } else { 0.0 };
        let started = Instant::now();
        j.link
            .send(ClientMsg::RequestOn {
                id,
                model: other.clone(),
                op: ohl_server::Op::Do {
                    command: ohl_session::Command::MoveStructure {
                        id: sid.clone(),
                        position: [p[0] + dx, p[1], p[2]],
                    },
                },
                line: "main".into(),
            })
            .map_err(|e| e.to_string())?;
        let want = id;
        let got = pump(
            j,
            Duration::from_secs(600),
            |m| matches!(m, ServerMsg::Response { id: i, .. } if *i == want),
        )?;
        match got.last() {
            Some(ServerMsg::Response { result: Ok(_), .. }) => {
                Ok(started.elapsed().as_secs_f64() * 1e3)
            }
            other => Err(format!("move refused: {other:?}")),
        }
    };
    // Idle: the moves alone.
    let idle: Vec<f64> = (0..10)
        .map(|k| move_once(&mut moving, k))
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    checker
        .link
        .send(ClientMsg::Check {
            id: 2,
            working_set: Some(spec.clone()),
            check,
        })
        .map_err(|e| e.to_string())?;
    let mut during = Vec::new();
    let mut done = None;
    let mut k = 0;
    while done.is_none() {
        if started.elapsed() > Duration::from_secs(1800) {
            return Err("the check did not end in 30 minutes".into());
        }
        during.push(move_once(&mut moving, k)?);
        k += 1;
        while let Some(ohl_server::client::Incoming::Msg(m)) =
            checker.link.recv_timeout(Duration::from_millis(0))
        {
            if let ServerMsg::CheckResult { id: 2, result } = &m {
                done = Some((started.elapsed().as_secs_f64(), result.is_ok()));
            }
        }
    }
    let stale = pump(&mut checker, Duration::from_secs(2), |m| {
        matches!(m, ServerMsg::CheckStale { id: 2, .. })
    })
    .is_ok();
    // The structure back where it was.
    if k % 2 == 1 {
        move_once(&mut moving, 1)?;
    }
    drop((checker, moving));
    server.stop();
    let (check_s, ok) = done.expect("ended");
    Ok(json!({
        "check_s": check_s,
        "check_ok": ok,
        "stale_after": stale,
        "moves_during": during.len(),
        "move_ms_idle": summary(&idle),
        "move_ms_during": summary(&during),
        "move_ms_during_max": during.iter().cloned().fold(0.0, f64::max),
    }))
}
