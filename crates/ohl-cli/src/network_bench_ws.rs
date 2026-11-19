// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-working-set`: phase 6 measured on a network.
//!
//! 1. `ohl network serve` as a process of its own, the idle solve off (so
//!    the timings are the clients' own): a route's structures; working
//!    sets picked and shown (a whole route, a third of it by stations, an
//!    area); a partial replica loaded (the whole route, then the third),
//!    opened; the rows of an editor's moves arriving in it (inside the
//!    working set, and elsewhere: none); the replica gone while the editor
//!    moves on, joining again with what it holds; the whole route added
//!    to the third; the context tiles around the working set (made, then
//!    from the cache); the server's memory with it.
//! 2. The server started again with the idle solve on: the time until
//!    every tile is made once, the packagers' time, the memory.
//!
//! Every wait is bounded; the server process is stopped at the end, also
//! on an error.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ohl_server::client::{Joined, NetJoin, hold, join_network, take_rows};
use ohl_server::{ClientMsg, Op, ServerMsg};
use ohl_session::Session;
use ohl_session::working_set::{Pick, WorkingSetSpec};
use serde_json::{Value, json};

use crate::network_bench::{Bench, FOLLOWER, held, pump, rss_mb, serve, summary, tiles, view};

/// What to measure.
pub struct WsBench {
    pub database: String,
    pub user: String,
    pub line: Option<String>,
    pub moves: usize,
    pub idle_wait: Duration,
    pub work: PathBuf,
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1e5).round() / 100.0
}

/// Takes messages of `j` until rows came (`limit` at most), taking them
/// into `replica`; the rows' puts and deletes.
fn rows_into(
    j: &mut Joined,
    replica: &mut Session,
    limit: Duration,
) -> Result<(usize, usize), String> {
    let got = pump(j, limit, |m| {
        matches!(m, ServerMsg::Rows(_) | ServerMsg::Notice { .. })
    })?;
    let r = match got.last() {
        Some(ServerMsg::Rows(r)) => r,
        Some(ServerMsg::Notice { text }) => return Err(format!("refused: {text}")),
        _ => return Err("no rows".into()),
    };
    take_rows(replica, FOLLOWER, r)?;
    Ok((r.rows.puts.len(), r.rows.deletes.len()))
}

/// An edit of `model` by `j`, waited for.
fn edit(j: &mut Joined, id: &mut u64, model: &str, op: Op) -> Result<(), String> {
    *id += 1;
    let want = *id;
    j.link.send(ClientMsg::RequestOn {
        id: want,
        model: model.into(),
        op,
        line: "main".into(),
    })?;
    let got = pump(
        j,
        Duration::from_secs(30),
        |m| matches!(m, ServerMsg::Response { id, .. } if *id == want),
    )?;
    match got.last() {
        Some(ServerMsg::Response { result: Err(e), .. }) => Err(format!("edit refused: {e}")),
        _ => Ok(()),
    }
}

fn moved(id: &str, p: [f64; 3]) -> Op {
    Op::Do {
        command: ohl_session::Command::MoveStructure {
            id: id.into(),
            position: p,
        },
    }
}

/// The server's counters (its `--stats` file).
fn stats(path: &std::path::Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

fn numbers(v: &Value, k: &str) -> Vec<f64> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

fn file_mb(p: &std::path::Path) -> f64 {
    std::fs::metadata(p).map_or(0.0, |m| (m.len() as f64 / 1e6 * 100.0).round() / 100.0)
}

pub fn bench(w: &WsBench) -> Result<Value, String> {
    std::fs::create_dir_all(&w.work).map_err(|e| format!("{}: {e}", w.work.display()))?;
    let b = Bench {
        database: w.database.clone(),
        user: w.user.clone(),
        line: w.line.clone(),
        viewers: 0,
        view: (5, 5),
        moves: w.moves,
        areas: 0,
        work: w.work.clone(),
    };
    let mut out = BTreeMap::new();
    let stats_file = w.work.join("server-ws-stats.json");
    let (served, _general, personal, follows, start_s) =
        serve(&b, &stats_file, &["--no-idle-solve"])?;
    let pid = served.child.id();
    std::thread::sleep(Duration::from_secs(2));
    let idle_mb = rss_mb(pid);
    out.insert("server_start_s", json!((start_s * 100.0).round() / 100.0));
    out.insert("server_idle_mb", json!(idle_mb));

    eprintln!("bench: a route's structures, working sets shown");
    let mut desk = join_network(&follows, FOLLOWER, &NetJoin::default(), &mut |_| {})?;
    let got = pump(&mut desk, Duration::from_secs(60), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
    let Some(ServerMsg::Network(info)) = got.last() else {
        return Err("no network info".into());
    };
    let (grid, level) = (info.grid_size, info.tile_level);
    let model = match &w.line {
        Some(l) => l.clone(),
        None => info.models.first().ok_or("no line")?.code.clone(),
    };
    let route = info
        .routes
        .iter()
        .find(|r| r.model == model)
        .ok_or("the line has no route")?
        .id
        .clone();
    let t = Instant::now();
    desk.link.send(ClientMsg::RouteDetail {
        model: model.clone(),
        route: route.clone(),
    })?;
    let got = pump(&mut desk, Duration::from_secs(60), |m| {
        matches!(m, ServerMsg::RouteDetail { .. })
    })?;
    let detail_ms = ms(t);
    let Some(ServerMsg::RouteDetail {
        result: Ok(detail), ..
    }) = got.last()
    else {
        return Err(format!("route detail: {:?}", got.last()));
    };
    let n = detail.structures.len();
    let st = |k: usize| detail.structures[k].station.unwrap_or(0.0);
    let third = [Pick::Stations {
        route: route.clone(),
        alignment: None,
        from: st(n / 3),
        to: st(2 * n / 3),
    }];
    let whole = [Pick::Route {
        route: route.clone(),
    }];
    let mid = detail.structures[n / 2].position;
    let area = [Pick::Area {
        polygon: vec![
            [mid[0] - 500.0, mid[1] - 500.0],
            [mid[0] + 500.0, mid[1] - 500.0],
            [mid[0] + 500.0, mid[1] + 500.0],
            [mid[0] - 500.0, mid[1] + 500.0],
        ],
    }];
    let mut shown = BTreeMap::new();
    let mut id = 0u64;
    for (name, picks) in [
        ("whole_route", &whole[..]),
        ("third", &third[..]),
        ("area_1km", &area[..]),
    ] {
        let mut times = Vec::new();
        let mut last = Value::Null;
        for _ in 0..10 {
            // The server paces working-set requests (4 a second).
            std::thread::sleep(Duration::from_millis(300));
            id += 1;
            let want = id;
            let t = Instant::now();
            desk.link.send(ClientMsg::Pick {
                id: want,
                model: model.clone(),
                picks: picks.to_vec(),
            })?;
            let got = pump(&mut desk, Duration::from_secs(60), |m| {
                matches!(m, ServerMsg::Extent { id, .. } if *id == want)
                    || matches!(m, ServerMsg::Notice { .. })
            })?;
            if let Some(ServerMsg::Notice { text }) = got.last() {
                return Err(format!("pick refused: {text}"));
            }
            times.push(ms(t));
            if let Some(ServerMsg::Extent { result, .. }) = got.last() {
                last = match result {
                    Ok(e) => json!({
                        "structures": e.structures, "sections": e.sections,
                        "jumpers": e.jumpers, "context": e.context,
                    }),
                    Err(e) => json!({ "error": e }),
                };
            }
        }
        shown.insert(name, json!({ "ms": summary(&times), "extent": last }));
    }
    drop(desk);
    out.insert("route", json!({ "structures": n, "detail_ms": detail_ms }));
    out.insert("shown", json!(shown));

    eprintln!("bench: partial replicas loaded");
    let mut loads = BTreeMap::new();
    for (name, picks) in [("whole_route", &whole[..]), ("third", &third[..])] {
        let file = w.work.join(format!("ws-{name}.ohl"));
        let _ = std::fs::remove_file(&file);
        let t = Instant::now();
        let j = join_network(
            &follows,
            FOLLOWER,
            &NetJoin {
                working_set: Some(WorkingSetSpec {
                    name: String::new(),
                    model: model.clone(),
                    picks: picks.to_vec(),
                }),
                file: Some(file.clone()),
                ..NetJoin::default()
            },
            &mut |_| {},
        )?;
        let joined_s = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let s = Session::open(&file).map_err(|e| e.to_string())?;
        let open_s = t.elapsed().as_secs_f64();
        loads.insert(
            name,
            json!({
                "join_s": (joined_s * 1000.0).round() / 1000.0,
                "open_s": (open_s * 1000.0).round() / 1000.0,
                "file_mb": file_mb(&file),
                "structures": s.line().structures.len(),
                "sections": s.line().sections.len(),
                "working_set": s.part().map_or(0, |p| p.len()),
            }),
        );
        drop((j, s));
    }
    // The whole line followed, for comparison (phase 5's desktop).
    let file = w.work.join("ws-model.ohl");
    let _ = std::fs::remove_file(&file);
    let t = Instant::now();
    let j = join_network(
        &follows,
        FOLLOWER,
        &NetJoin {
            model: Some(model.clone()),
            file: Some(file.clone()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )?;
    loads.insert(
        "whole_line_followed",
        json!({ "join_s": (t.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
                "file_mb": file_mb(&file) }),
    );
    drop(j);
    out.insert("replica_loaded", json!(loads));

    eprintln!("bench: rows of edits, joining again, a route added");
    let file = w.work.join("ws-live.ohl");
    let _ = std::fs::remove_file(&file);
    let spec = WorkingSetSpec {
        name: String::new(),
        model: model.clone(),
        picks: third.to_vec(),
    };
    let mut d = join_network(
        &follows,
        FOLLOWER,
        &NetJoin {
            working_set: Some(spec.clone()),
            file: Some(file.clone()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )?;
    let mut r = Session::open(&file).map_err(|e| e.to_string())?;
    let inside: Vec<(String, [f64; 3])> = r
        .part()
        .map(|p| {
            p.iter()
                .filter(|(k, _)| *k == ohl_session::Kind::Structure)
                .filter_map(|(_, id)| r.line().structure(id).map(|s| (id.clone(), s.position)))
                .collect()
        })
        .unwrap_or_default();
    if inside.is_empty() {
        return Err("the working set holds no structure".into());
    }
    let mut editor = join_network(&personal, &w.user, &NetJoin::default(), &mut |_| {})?;
    let mut arrived = Vec::new();
    let mut puts = Vec::new();
    for k in 0..w.moves {
        let (sid, p) = &inside[(k / 2 * 7) % inside.len()];
        let dx = if k % 2 == 0 { 0.5 } else { 0.0 };
        let t = Instant::now();
        edit(
            &mut editor,
            &mut id,
            &model,
            moved(sid, [p[0] + dx, p[1], p[2]]),
        )?;
        let (n_puts, _) = rows_into(&mut d, &mut r, Duration::from_secs(30))?;
        arrived.push(ms(t));
        puts.push(n_puts as f64);
    }
    // The replica gone; the editor moves on; joining again.
    drop(d);
    let gone_moves = 20.min(inside.len() * 2);
    for k in 0..gone_moves {
        let (sid, p) = &inside[k % inside.len()];
        let dy = if k < inside.len() { 0.25 } else { 0.0 };
        edit(
            &mut editor,
            &mut id,
            &model,
            moved(sid, [p[0], p[1] + dy, p[2]]),
        )?;
    }
    let t = Instant::now();
    let mut d = join_network(
        &follows,
        FOLLOWER,
        &NetJoin {
            working_set: Some(spec.clone()),
            held: true,
            ..NetJoin::default()
        },
        &mut |_| {},
    )?;
    std::thread::sleep(Duration::from_millis(300));
    d.link.send(hold(&r))?;
    let (again_puts, again_deletes) = rows_into(&mut d, &mut r, Duration::from_secs(60))?;
    let again_ms = ms(t);
    // The whole route added.
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let mut bigger = spec.clone();
    bigger.picks.extend(whole.iter().cloned());
    d.link.send(ClientMsg::SetWorkingSet { spec: bigger })?;
    let (added_puts, _) = rows_into(&mut d, &mut r, Duration::from_secs(60))?;
    let added_ms = ms(t);
    out.insert(
        "rows",
        json!({
            "move_arrived_in_replica_ms": summary(&arrived),
            "puts_per_move": summary(&puts),
            "joined_again": { "after_moves": gone_moves, "ms": again_ms,
                              "puts": again_puts, "deletes": again_deletes },
            "route_added": { "ms": added_ms, "puts": added_puts },
        }),
    );

    eprintln!("bench: context tiles around the working set");
    let c = inside[inside.len() / 2].1;
    let around = view([c[0], c[1]], 5, 5, grid, level);
    let t = Instant::now();
    d.link.send(ClientMsg::Interest {
        routes: Vec::new(),
        tiles: held(&around),
    })?;
    tiles(&mut d, around.len())?;
    let made_ms = ms(t);
    d.link.send(ClientMsg::Interest {
        routes: Vec::new(),
        tiles: Vec::new(),
    })?;
    std::thread::sleep(Duration::from_millis(200));
    let t = Instant::now();
    d.link.send(ClientMsg::Interest {
        routes: Vec::new(),
        tiles: held(&around),
    })?;
    tiles(&mut d, around.len())?;
    out.insert(
        "context_tiles_5x5",
        json!({ "made_ms": made_ms, "cached_ms": ms(t), "tiles": around.len() }),
    );
    let with_mb = rss_mb(pid);
    std::thread::sleep(Duration::from_millis(1200));
    let s = stats(&stats_file);
    out.insert(
        "server",
        json!({
            "with_working_set_mb": with_mb,
            "replica_taken_ms": summary(&numbers(&s, "replica_ms")),
            "rows_per_change_ms": summary(&numbers(&s, "rows_ms")),
            "move_applied_ms": summary(&numbers(&s, "applied_ms")),
        }),
    );
    drop((d, editor, r));
    drop(served);

    if !w.idle_wait.is_zero() {
        eprintln!("bench: the idle solve");
        let stats_file = w.work.join("server-idle-stats.json");
        let _ = std::fs::remove_file(&stats_file);
        let (served, _, _, _, _) = serve(&b, &stats_file, &[])?;
        let pid = served.child.id();
        let started = Instant::now();
        let mut peak = 0.0f64;
        let s = loop {
            std::thread::sleep(Duration::from_secs(2));
            peak = peak.max(rss_mb(pid));
            let s = stats(&stats_file);
            if !s["idle_done_s"].is_null() || s["idle_full"] == json!(true) {
                break s;
            }
            if started.elapsed() > w.idle_wait {
                break s;
            }
        };
        let (shapes, shapes_bytes) = (s["shapes"].clone(), s["shapes_bytes"].clone());
        out.insert(
            "idle_solve",
            json!({
                "tiles": s["tiles"], "made": s["idle_built"],
                "left": s["idle_left"], "stopped_full": s["idle_full"],
                "packagers_s": s["idle_s"], "from_s": s["idle_from_s"],
                "done_s": s["idle_done_s"],
                "wall_s": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                "shapes": shapes, "shapes_mb": shapes_bytes.as_f64().map(|b| (b / 1e6 * 10.0).round() / 10.0),
                "packages": s["packages"], "packages_mb": s["packages_bytes"].as_f64().map(|b| (b / 1e6 * 10.0).round() / 10.0),
                "peak_mb": peak, "after_mb": rss_mb(pid),
            }),
        );
        drop(served);
    }
    Ok(json!(out))
}
