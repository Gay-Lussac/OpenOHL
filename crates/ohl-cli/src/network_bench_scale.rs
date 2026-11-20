// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-scale`: phase 14, the network measured end to end
//! with many clients at once.
//!
//! `ohl network serve` as a process of its own (its memory its own; the
//! idle solve off, more connections of one address allowed: the clients
//! all come from this computer): the time until it listens and its memory
//! idle; then
//!
//! 1. **joins at once**: `editors` users, each a designer of one route
//!    (its line's), and `viewers` users of the whole network join
//!    together, each with a view of its own (`view` tiles around a
//!    structure: its packages made) — the time from asking to the whole
//!    first view;
//! 2. **moves at once**: every editor moves structures of its own route
//!    back and forth (at least `moves` each, one after the other, every
//!    answer awaited, and on until the long work is answered — at most
//!    `edit_seconds`), all editors together — each move's latency (sent →
//!    answered) and the moves per second of all;
//! 3. **long work queued meanwhile**: while they move, two users ask a
//!    whole route's clearances (the checks' worker) and two export a whole
//!    route as an offline working set (the exports' thread) — each one's
//!    time from asking to its answer, the queue's waits;
//! 4. the server's memory with everyone connected, and its own timings.
//!
//! Users are named `scale-…` (made once). Every wait is bounded; the
//! server process is stopped at the end, also on an error. The structures
//! moved end where they were.

use std::path::PathBuf;
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use ohl_server::client::{Joined, NetJoin, join_network};
use ohl_server::{ClientMsg, Invitation, Op, ScopedCheck, ServerMsg};
use ohl_session::working_set::{Pick, WorkingSetSpec};
use serde_json::{Value, json};

use crate::network_bench::{Bench, held, personal_of, pump, rss_mb, serve, summary, tiles, view};

/// The rules of the clearance check (arbitrary distances for measuring,
/// not from a standard; as `bench-scoped`).
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

/// What to measure.
pub struct Scale {
    pub database: String,
    pub user: String,
    pub editors: usize,
    pub viewers: usize,
    pub view: (i64, i64),
    pub moves: usize,
    /// The editors move on while the long work is not answered, at most
    /// this long.
    pub edit_seconds: u64,
    pub work: PathBuf,
}

/// The users made once (by the network admin `admin`): editors each a
/// designer of route `routes[k]`, viewers, two checkers and two exporters
/// of the whole network.
fn users(
    database: &str,
    admin: &str,
    routes: &[(String, String)],
    viewers: usize,
) -> Result<(), String> {
    use ohl_session::rights::{Scope, Subject};
    use ohl_store_pg::rights::RightsCommand as R;
    let net = ohl_store_pg::Network::connect(database).map_err(|e| e.to_string())?;
    let d = net.rights().map_err(|e| e.to_string())?;
    if d.network_admins().is_empty() {
        net.rights_command(
            admin,
            &R::Bootstrap {
                user: admin.to_owned(),
            },
        )
        .map_err(|e| e.to_string())?;
    }
    let mut wanted: Vec<(String, &str, Scope)> = Vec::new();
    for (k, (_, route)) in routes.iter().enumerate() {
        wanted.push((
            format!("scale-editor-{k}"),
            "designer",
            Scope {
                route: Some(route.clone()),
                ..Scope::default()
            },
        ));
    }
    for k in 0..viewers {
        wanted.push((format!("scale-viewer-{k}"), "viewer", Scope::network()));
    }
    for name in [
        "scale-checker-0",
        "scale-checker-1",
        "scale-exporter-0",
        "scale-exporter-1",
    ] {
        wanted.push((name.to_owned(), "designer", Scope::network()));
    }
    for (name, role, scope) in wanted {
        if d.users.contains_key(&name) {
            continue;
        }
        let run = |c: R| net.rights_command(admin, &c).map_err(|e| e.to_string());
        run(R::AddUser {
            name: name.clone(),
            display: None,
        })?;
        run(R::Assign {
            subject: Subject::User(name),
            role: Some(role.to_owned()),
            deny: false,
            scope,
            note: "bench-scale".into(),
        })?;
    }
    Ok(())
}

/// One client's moves of its structures, at least `moves` and on while
/// `more` says so: each latency, ms.
fn moves_of(
    j: &mut Joined,
    model: &str,
    structures: &[(String, [f64; 3])],
    moves: usize,
    more: &dyn Fn() -> bool,
) -> Result<Vec<f64>, String> {
    let mut out = Vec::with_capacity(moves);
    let mut k = 0;
    // In pairs: every structure back where it was.
    while k < moves || (k % 2 == 1) || more() {
        let (id, p) = &structures[(k / 2 * 7) % structures.len()];
        let dx = if k % 2 == 0 { 0.1 } else { 0.0 };
        let want = 1000 + k as u64;
        let t = Instant::now();
        j.link
            .send(ClientMsg::RequestOn {
                id: want,
                model: model.to_owned(),
                op: Op::Do {
                    command: ohl_session::Command::MoveStructure {
                        id: id.clone(),
                        position: [p[0] + dx, p[1], p[2]],
                    },
                },
                line: "main".into(),
            })
            .map_err(|e| e.to_string())?;
        let got = pump(
            j,
            Duration::from_secs(600),
            |m| matches!(m, ServerMsg::Response { id, .. } if *id == want),
        )?;
        if let Some(ServerMsg::Response { result: Err(e), .. }) = got.last() {
            return Err(format!("{model}: move refused: {e}"));
        }
        out.push(t.elapsed().as_secs_f64() * 1e3);
        k += 1;
    }
    Ok(out)
}

/// Long work asked by one client: its answer's time, s, and whether it
/// was a result (not a refusal).
fn long_work(j: &mut Joined, msg: ClientMsg) -> Result<(f64, bool, String), String> {
    let t = Instant::now();
    j.link.send(msg).map_err(|e| e.to_string())?;
    let got = pump(j, Duration::from_secs(1800), |m| {
        matches!(
            m,
            ServerMsg::CheckResult { id: 7, .. } | ServerMsg::WorkingSetExported { id: 7, .. }
        )
    })?;
    let s = t.elapsed().as_secs_f64();
    Ok(match got.last() {
        Some(ServerMsg::CheckResult { result: Ok(r), .. }) => (s, true, r.status.clone()),
        Some(ServerMsg::WorkingSetExported { result: Ok(f), .. }) => (
            s,
            true,
            format!(
                "{} bytes, stale {}, {} changed meanwhile",
                f.report.bytes, f.report.stale, f.report.changed_meanwhile
            ),
        ),
        Some(ServerMsg::CheckResult { result: Err(e), .. })
        | Some(ServerMsg::WorkingSetExported { result: Err(e), .. }) => (s, false, e.clone()),
        other => (s, false, format!("{other:?}")),
    })
}

pub fn bench(s: &Scale) -> Result<Value, String> {
    std::fs::create_dir_all(&s.work).map_err(|e| format!("{}: {e}", s.work.display()))?;
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = ohl_store_pg::Network::connect(&s.database).map_err(e)?;
    // One route per line, the first `editors` lines.
    let mut routes: Vec<(String, String)> = Vec::new();
    for (line, route) in net.route_list().map_err(e)? {
        if !routes.iter().any(|(l, _)| *l == line) {
            routes.push((line, route));
        }
    }
    routes.truncate(s.editors.max(1));
    let mut structures = Vec::new();
    for (line, route) in &routes {
        let on: std::collections::BTreeSet<String> = net
            .structures_on_route(line, route)
            .map_err(e)?
            .into_iter()
            .collect();
        let session = ohl_session::Session::open_on(Box::new(net.store(line).map_err(e)?), line)
            .map_err(|x| x.to_string())?;
        let mine: Vec<(String, [f64; 3])> = session
            .line()
            .structures
            .iter()
            .filter(|x| on.contains(&x.id))
            .map(|x| (x.id.clone(), x.position))
            .collect();
        if mine.is_empty() {
            return Err(format!("route {route} of line {line} has no structure"));
        }
        structures.push(mine);
    }
    let centres = net.sample_structures(4000).map_err(e)?;
    if centres.len() < s.viewers + 10 {
        return Err("too few structures for the views".into());
    }
    drop(net);
    users(&s.database, &s.user, &routes, s.viewers)?;
    let b = Bench {
        database: s.database.clone(),
        user: s.user.clone(),
        line: None,
        viewers: 0,
        view: s.view,
        moves: 0,
        areas: 0,
        work: s.work.clone(),
    };
    let stats_file = s.work.join("scale-stats.json");
    let (served, general, _personal, _follower, start_s) = serve(
        &b,
        &stats_file,
        &["--no-idle-solve", "--max-per-address", "256"],
    )?;
    let pid = served.child.id();
    std::thread::sleep(Duration::from_secs(2));
    let idle_mb = rss_mb(pid);
    let invitation = |name: &str| -> Result<Invitation, String> { personal_of(&general, &b, name) };

    // 1. Everyone joins at once, each with a view of its own.
    eprintln!(
        "bench-scale: {} editors and {} viewers join at once",
        routes.len(),
        s.viewers
    );
    let (w, h) = s.view;
    let n = (w * h) as usize;
    let (grid, level) = {
        let mut probe = join_network(
            &invitation("scale-viewer-0")?,
            "scale-viewer-0",
            &NetJoin::default(),
            &mut |_| {},
        )?;
        let got = pump(&mut probe, Duration::from_secs(60), |m| {
            matches!(m, ServerMsg::Network(_))
        })?;
        match got.last() {
            Some(ServerMsg::Network(info)) => (info.grid_size, info.tile_level),
            _ => return Err("no network info".into()),
        }
    };
    let mut names: Vec<(String, [f64; 2])> = Vec::new();
    for (k, mine) in structures.iter().enumerate() {
        let p = mine[0].1;
        names.push((format!("scale-editor-{k}"), [p[0], p[1]]));
    }
    for k in 0..s.viewers {
        names.push((
            format!("scale-viewer-{k}"),
            centres[(k * 37 + 11) % centres.len()],
        ));
    }
    let joined: Vec<Result<(Joined, f64), String>> = std::thread::scope(|sc| {
        let handles: Vec<_> = names
            .iter()
            .map(|(name, c)| {
                let inv = invitation(name);
                sc.spawn(move || -> Result<(Joined, f64), String> {
                    let t = Instant::now();
                    let mut j = join_network(
                        &inv?,
                        name,
                        &NetJoin {
                            tiles: held(&view(*c, w, h, grid, level)),
                            ..NetJoin::default()
                        },
                        &mut |_| {},
                    )?;
                    tiles(&mut j, n)?;
                    Ok((j, t.elapsed().as_secs_f64() * 1e3))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("a join panicked".into())))
            .collect()
    });
    let mut clients = Vec::new();
    let mut join_ms = Vec::new();
    for r in joined {
        let (j, ms) = r?;
        clients.push(j);
        join_ms.push(ms);
    }
    let joined_mb = rss_mb(pid);
    let mut viewers = clients.split_off(routes.len());
    let mut editors = clients;

    // 2. and 3. Every editor moves at once; long work asked meanwhile.
    eprintln!("bench-scale: moves at once, checks and exports queued meanwhile");
    let mut heavy = Vec::new();
    for name in [
        "scale-checker-0",
        "scale-checker-1",
        "scale-exporter-0",
        "scale-exporter-1",
    ] {
        let mut j = join_network(&invitation(name)?, name, &NetJoin::default(), &mut |_| {})?;
        pump(&mut j, Duration::from_secs(60), |m| {
            matches!(m, ServerMsg::Network(_))
        })?;
        heavy.push((name, j));
    }
    let spec = |k: usize| WorkingSetSpec {
        name: String::new(),
        model: routes[k % routes.len()].0.clone(),
        picks: vec![Pick::Route {
            route: routes[k % routes.len()].1.clone(),
        }],
    };
    let all = Arc::new(Mutex::new(Vec::new()));
    // The editors move until the long work is answered.
    let long_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let last_move = Arc::new(Mutex::new(Duration::ZERO));
    let start = Arc::new(Barrier::new(editors.len() + heavy.len() + 1));
    let started = Instant::now();
    let (moved, long): (Vec<Result<(), String>>, Vec<Value>) = std::thread::scope(|sc| {
        let mut handles = Vec::new();
        for (k, j) in editors.iter_mut().enumerate() {
            let (model, mine) = (routes[k].0.clone(), &structures[k]);
            let (all, start) = (all.clone(), start.clone());
            let (done, last) = (long_done.clone(), last_move.clone());
            handles.push(sc.spawn(move || -> Result<(), String> {
                start.wait();
                let cap = Duration::from_secs(s.edit_seconds);
                let more =
                    || !done.load(std::sync::atomic::Ordering::SeqCst) && started.elapsed() < cap;
                let ms = moves_of(j, &model, mine, s.moves, &more)?;
                all.lock().unwrap_or_else(|e| e.into_inner()).extend(ms);
                let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
                *l = (*l).max(started.elapsed());
                Ok(())
            }));
        }
        let mut asked = Vec::new();
        for (k, (name, j)) in heavy.iter_mut().enumerate() {
            let start = start.clone();
            let msg = if name.contains("checker") {
                ClientMsg::Check {
                    id: 7,
                    working_set: Some(spec(k)),
                    check: ScopedCheck::Clearances {
                        case: "10 °C + 300 Pa wind".into(),
                        rules: RULES.into(),
                    },
                }
            } else {
                ClientMsg::ExportWorkingSet {
                    id: 7,
                    working_set: spec(k),
                    reach: None,
                }
            };
            let name = name.to_string();
            asked.push(sc.spawn(move || -> Value {
                start.wait();
                // A moment after the moves began.
                std::thread::sleep(Duration::from_millis(500));
                match long_work(j, msg) {
                    Ok((s, ok, what)) => json!({"who": name, "s": s, "ok": ok, "what": what}),
                    Err(e) => json!({"who": name, "error": e}),
                }
            }));
        }
        start.wait();
        let long = asked
            .into_iter()
            .map(|h| h.join().unwrap_or(json!({"error": "panicked"})))
            .collect();
        long_done.store(true, std::sync::atomic::Ordering::SeqCst);
        let moved = handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("an editor panicked".into()))
            })
            .collect();
        (moved, long)
    });
    let edits_s = last_move
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_secs_f64();
    for r in moved {
        r?;
    }
    let all = all.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let with_all_mb = rss_mb(pid);
    // Every client still connected (the viewers received every change).
    let mut alive = 0;
    for j in viewers.iter_mut().chain(editors.iter_mut()) {
        alive += usize::from(
            pump(j, Duration::from_secs(5), |m| {
                matches!(m, ServerMsg::Heartbeat { .. })
            })
            .is_ok(),
        );
    }
    std::thread::sleep(Duration::from_millis(1500));
    let stats: Value = std::fs::read_to_string(&stats_file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let server = |k: &str| -> Vec<f64> {
        stats
            .get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default()
    };
    let connected = editors.len() + viewers.len() + heavy.len() + 1;
    drop((editors, viewers, heavy));
    drop(served);
    Ok(json!({
        "editors": routes.len(),
        "viewers": s.viewers,
        "view_tiles": [w, h],
        "server_start_s": (start_s * 100.0).round() / 100.0,
        "server_read_s": stats.get("start_s").cloned().unwrap_or(Value::Null),
        "idle_mb": idle_mb,
        "joined_mb": joined_mb,
        "with_all_mb": with_all_mb,
        "connected": connected,
        "still_connected_after": alive,
        "join_to_first_view_ms": summary(&join_ms),
        "moves": all.len(),
        "move_ms": summary(&all),
        "move_ms_max": all.iter().cloned().fold(0.0, f64::max),
        "moves_per_s": (all.len() as f64 / edits_s * 10.0).round() / 10.0,
        "edits_s": edits_s,
        "long_work": long,
        "move_applied_ms_server": summary(&server("applied_ms")),
        "heavy_wait_ms": summary(&server("heavy_wait_ms")),
        "check_snapshot_ms": summary(&server("check_snapshot_ms")),
        "export_plan_ms": summary(&server("export_plan_ms")),
        "tile_built_ms_server": summary(&server("built_ms")),
        "tiles_not_made": stats.get("tiles_not_made").cloned().unwrap_or(Value::Null),
        "heavy_refused": stats.get("heavy_refused").cloned().unwrap_or(Value::Null),
    }))
}
