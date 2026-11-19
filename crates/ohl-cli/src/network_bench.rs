// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench`: the plan's targets (§4) measured on a network.
//!
//! 1. `ohl network serve` as a process of its own (so its memory is its
//!    own): the time until it listens (every line read, the tile index
//!    made), its memory idle, then with viewers — half of them on one
//!    view (what one more connection costs: the packages are shared),
//!    half each on a view of its own — the time from joining to every
//!    tile of the first view received, tile packages asked again (from
//!    the cache) and new (made), a desktop joining a line (its copy), and
//!    structures moved by it: the answer and the arrival at another
//!    client following the line. The server's own timings come from its
//!    `--stats` file.
//! 2. The server in this process (the first stopped: one writer):
//!    area queries of the authority's tile index, and the same areas
//!    asked of the database's spatial index (PostGIS).
//!
//! Every wait is bounded; the server process is stopped at the end, also
//! on an error.

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ohl_server::client::{Incoming, Joined, NetJoin, join_network};
use ohl_server::{ClientMsg, HeldTile, Invitation, Op, ServerMsg, TileKey};
use serde_json::{Value, json};

/// What to measure.
pub struct Bench {
    pub database: String,
    pub user: String,
    pub line: Option<String>,
    pub viewers: usize,
    pub view: (i64, i64),
    pub moves: usize,
    pub areas: usize,
    pub work: PathBuf,
}

/// The server process, stopped when dropped.
struct Served {
    child: Child,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Resident memory of process `pid`, MB.
fn rss_mb(pid: u32) -> f64 {
    Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map_or(0.0, |kb| (kb / 1024.0 * 10.0).round() / 10.0)
}

fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let k = ((p / 100.0) * (s.len() - 1) as f64).round() as usize;
    (s[k.min(s.len() - 1)] * 1000.0).round() / 1000.0
}

fn summary(v: &[f64]) -> Value {
    json!({
        "n": v.len(),
        "p50": percentile(v, 50.0),
        "p95": percentile(v, 95.0),
        "max": percentile(v, 100.0),
    })
}

/// Takes messages of `j` until `until` says stop (`limit` at most).
fn pump(
    j: &mut Joined,
    limit: Duration,
    mut until: impl FnMut(&ServerMsg) -> bool,
) -> Result<Vec<ServerMsg>, String> {
    let started = Instant::now();
    let mut got = Vec::new();
    loop {
        if started.elapsed() > limit {
            return Err(format!("no answer in {} s", limit.as_secs()));
        }
        match j.link.recv_timeout(Duration::from_millis(5)) {
            Some(Incoming::Msg(m)) => {
                let stop = until(&m);
                got.push(m);
                if stop {
                    return Ok(got);
                }
            }
            Some(Incoming::Closed(why)) => return Err(format!("closed: {why}")),
            None => {}
        }
    }
}

/// Waits for `n` tile packages.
fn tiles(j: &mut Joined, n: usize) -> Result<(), String> {
    let mut seen = 0;
    let wait = std::env::var("OHL_BENCH_TILE_WAIT_S")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    pump(j, Duration::from_secs(wait), |m| {
        if matches!(m, ServerMsg::Tile(_)) {
            seen += 1;
        }
        seen >= n
    })
    .map(|_| ())
}

/// The tiles of a view of `w` × `h` tiles of level `level` around `c`.
fn view(c: [f64; 2], w: i64, h: i64, grid: f64, level: i8) -> Vec<TileKey> {
    let g = ohl_core::grid::Grid::new(grid).unwrap_or_default();
    let t = g.tile_at(level, c);
    let mut out = Vec::new();
    for dx in 0..w {
        for dy in 0..h {
            out.push(TileKey(level, t.x - w / 2 + dx, t.y - h / 2 + dy));
        }
    }
    out
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

/// Starts `ohl network serve`; returns it, the general and personal
/// invitations, and the seconds until it listened.
/// The client following the line moved (a viewer of it: a newcomer at
/// the first run, a user of it after; its personal invitation serves
/// both).
const FOLLOWER: &str = "bench-follower";

/// Starts `ohl network serve`; returns it, the general, the user's and
/// the follower's invitations, and the seconds until it listened.
#[allow(clippy::type_complexity)]
fn serve(
    b: &Bench,
    stats: &std::path::Path,
) -> Result<(Served, Invitation, Invitation, Invitation, f64), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut child = Command::new(exe)
        .args(["network", "serve", "--bind", "127.0.0.1:0", "--code"])
        .arg("bench-general-code")
        .arg("--keys")
        .arg(b.work.join("bench.server-key"))
        .args([
            "--invite",
            &b.user,
            "--invite",
            FOLLOWER,
            "--heartbeat-ms",
            "1000",
        ])
        .arg("--stats")
        .arg(stats)
        .env("OHL_DATABASE_URL", &b.database)
        .stdout(Stdio::piped())
        .stderr(
            std::fs::File::create(b.work.join("server.log"))
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null()),
        )
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = child.stdout.take().ok_or("no output")?;
    let served = Served { child };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Read to the end: a server writing to a closed pipe would stop.
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let (mut general, mut personal, mut follower) = (None, None, None);
    let mut seconds = 0.0;
    let prefix = format!("invitation for {}: ", b.user);
    let follows = format!("invitation for {FOLLOWER}: ");
    while general.is_none() || personal.is_none() || follower.is_none() {
        let line = rx
            .recv_timeout(Duration::from_secs(600))
            .map_err(|_| "the server did not start in 10 minutes".to_owned())?;
        if let Some(i) = line.strip_prefix("invitation (newcomers): ") {
            general = Some(i.parse::<Invitation>()?);
            seconds = started.elapsed().as_secs_f64();
        } else if let Some(i) = line.strip_prefix(&prefix) {
            personal = Some(i.parse::<Invitation>()?);
        } else if let Some(i) = line.strip_prefix(&follows) {
            follower = Some(i.parse::<Invitation>()?);
        }
    }
    Ok((
        served,
        general.expect("read"),
        personal.expect("read"),
        follower.expect("read"),
        seconds,
    ))
}

pub fn bench(b: &Bench) -> Result<Value, String> {
    std::fs::create_dir_all(&b.work).map_err(|e| format!("{}: {e}", b.work.display()))?;
    let stats_file = b.work.join("server-stats.json");
    let (served, general, personal, follows, start_s) = serve(b, &stats_file)?;
    let pid = served.child.id();
    std::thread::sleep(Duration::from_secs(2));
    let idle_mb = rss_mb(pid);

    eprintln!("bench: the network's info");
    // What the network holds.
    let t = Instant::now();
    let mut first = join_network(&general, "viewer-0", &NetJoin::default(), &mut |_| {})?;
    let got = pump(&mut first, Duration::from_secs(60), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
    let info_s = t.elapsed().as_secs_f64();
    let Some(ServerMsg::Network(info)) = got.last().cloned() else {
        return Err("no network info".into());
    };
    // Views centred on structures (a sample the same every time): the
    // first ones for the viewers, the next ones for tiles never asked.
    let centres = ohl_store_pg::Network::connect(&b.database)
        .and_then(|n| n.sample_structures(4000))
        .map_err(|e| e.to_string())?;
    if centres.len() < 100 {
        return Err("the network has fewer than 100 structures".into());
    }
    let (w, h) = b.view;
    let level = info.tile_level;
    let grid = info.grid_size;
    let n = (w * h) as usize;

    eprintln!("bench: viewers on views of their own");
    // Viewers each on a view of its own: joining to the first view whole
    // (its packages made: none asked before).
    let distinct = (b.viewers / 2).max(1);
    let mut first_view_built = Vec::new();
    let mut clients: Vec<Joined> = Vec::new();
    for k in 0..distinct {
        let c = centres[k % centres.len()];
        let tiles_k = view(c, w, h, grid, level);
        let t = Instant::now();
        let mut j = join_network(
            &general,
            &format!("viewer-d{k}"),
            &NetJoin {
                tiles: held(&tiles_k),
                ..NetJoin::default()
            },
            &mut |_| {},
        )?;
        tiles(&mut j, n)?;
        first_view_built.push(t.elapsed().as_secs_f64() * 1e3);
        clients.push(j);
    }
    let after_distinct_mb = rss_mb(pid);
    eprintln!("bench: viewers on one view");
    // Viewers on one view (made once, then from the cache): what one more
    // connection costs.
    let same = b.viewers.saturating_sub(distinct).max(1);
    let shared_view = view(centres[0], w, h, grid, level);
    let mut first_view_cached = Vec::new();
    let before_same_mb = rss_mb(pid);
    for k in 0..same {
        let t = Instant::now();
        let mut j = join_network(
            &general,
            &format!("viewer-s{k}"),
            &NetJoin {
                tiles: held(&shared_view),
                ..NetJoin::default()
            },
            &mut |_| {},
        )?;
        tiles(&mut j, n)?;
        if k > 0 {
            first_view_cached.push(t.elapsed().as_secs_f64() * 1e3);
        }
        clients.push(j);
    }
    std::thread::sleep(Duration::from_secs(1));
    let after_same_mb = rss_mb(pid);

    eprintln!("bench: tiles asked again and new");
    // Tiles asked again (cache) and new (made), one at a time.
    let mut cached_ms = Vec::new();
    let mut built_ms = Vec::new();
    let probe = &mut first;
    let base: Vec<TileKey> = Vec::new();
    for k in 0..30usize {
        // A structure's tile nobody asked for (far down the sample).
        let c = centres[(centres.len() / 2 + k * 37) % centres.len()];
        let fresh = view(c, 1, 1, grid, level)[0];
        for (tile, out) in [(fresh, &mut built_ms), (fresh, &mut cached_ms)] {
            // Within the server's pace of interest requests (4 a second).
            std::thread::sleep(Duration::from_millis(260));
            probe
                .link
                .send(ClientMsg::Interest {
                    routes: Vec::new(),
                    tiles: held(&base),
                })
                .map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(260));
            let t = Instant::now();
            probe
                .link
                .send(ClientMsg::Interest {
                    routes: Vec::new(),
                    tiles: held(&[tile]),
                })
                .map_err(|e| e.to_string())?;
            tiles(probe, 1)?;
            out.push(t.elapsed().as_secs_f64() * 1e3);
        }
    }

    eprintln!("bench: a desktop joins and moves");
    // A desktop joins a line (its copy), another client follows it; the
    // desktop moves structures back and forth.
    let line = match &b.line {
        Some(l) => l.clone(),
        None => info.models.first().ok_or("no line")?.code.clone(),
    };
    let copy = b.work.join(format!("bench-{line}.ohl"));
    let _ = std::fs::remove_file(&copy);
    let t = Instant::now();
    let mut editor = join_network(
        &personal,
        &b.user,
        &NetJoin {
            model: Some(line.clone()),
            file: Some(copy.clone()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )?;
    let desktop_join_s = t.elapsed().as_secs_f64();
    let replica = ohl_session::Session::open(&copy).map_err(|e| e.to_string())?;
    let structures: Vec<(String, [f64; 3])> = replica
        .line()
        .structures
        .iter()
        .map(|s| (s.id.clone(), s.position))
        .collect();
    drop(replica);
    let mut follower = join_network(
        &follows,
        FOLLOWER,
        &NetJoin {
            model: Some(line.clone()),
            file: Some(b.work.join(format!("bench-{line}-follower.ohl"))),
            ..NetJoin::default()
        },
        &mut |_| {},
    )?;
    pump(&mut follower, Duration::from_secs(30), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
    pump(&mut editor, Duration::from_secs(30), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
    let mut answered_ms = Vec::new();
    let mut arrived_ms = Vec::new();
    for k in 0..b.moves {
        let (id, p) = &structures[(k / 2 * 31) % structures.len()];
        let dx = if k % 2 == 0 { 1.0 } else { 0.0 };
        let op = Op::Do {
            command: ohl_session::Command::MoveStructure {
                id: id.clone(),
                position: [p[0] + dx, p[1], p[2]],
            },
        };
        let t = Instant::now();
        editor
            .link
            .send(ClientMsg::RequestOn {
                id: k as u64 + 1,
                model: line.clone(),
                op,
                line: "main".into(),
            })
            .map_err(|e| e.to_string())?;
        let want = k as u64 + 1;
        let got = pump(
            &mut editor,
            Duration::from_secs(30),
            |m| matches!(m, ServerMsg::Response { id, .. } if *id == want),
        )?;
        if let Some(ServerMsg::Response { result: Err(e), .. }) = got.last() {
            return Err(format!("move refused: {e}"));
        }
        answered_ms.push(t.elapsed().as_secs_f64() * 1e3);
        pump(&mut follower, Duration::from_secs(30), |m| {
            matches!(m, ServerMsg::Change(_))
        })?;
        arrived_ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let connected = clients.len() + 3;
    let with_all_mb = rss_mb(pid);
    std::thread::sleep(Duration::from_millis(1200));
    let stats: Value = std::fs::read_to_string(&stats_file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let server_ms = |k: &str| -> Vec<f64> {
        stats
            .get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default()
    };
    drop((clients, first, editor, follower));
    drop(served);

    eprintln!("bench: area queries");
    // The server in this process: area queries inside the authority, and
    // the same areas asked of PostgreSQL.
    let server = ohl_server::server::Server::start_network(
        ohl_server::server::Options {
            bind: "127.0.0.1:0".parse().expect("address"),
            ..Default::default()
        },
        ohl_server::network::NetOptions {
            database: b.database.clone(),
            ..Default::default()
        },
    )?;
    let size = grid * 2f64.powi(i32::from(level));
    let mut area_us = Vec::new();
    let mut area_objects = Vec::new();
    let mut db_ms = Vec::new();
    let reader = ohl_store_pg::Network::connect(&b.database).map_err(|e| e.to_string())?;
    for k in 0..b.areas {
        let c = centres[(k * 7 + 1) % centres.len()];
        let area = [
            c[0] - 0.5 * w as f64 * size,
            c[1] - 0.5 * h as f64 * size,
            c[0] + 0.5 * w as f64 * size,
            c[1] + 0.5 * h as f64 * size,
        ];
        let r: Value = serde_json::from_str(&server.probe(ohl_server::network::Probe::Area(area))?)
            .map_err(|e| e.to_string())?;
        area_us.push(r["us"].as_f64().unwrap_or(f64::NAN));
        area_objects.push(r["objects"].as_f64().unwrap_or(0.0));
        let t = Instant::now();
        reader.objects_in_area(area).map_err(|e| e.to_string())?;
        db_ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    server.stop();

    let per_client = |a: f64, b: f64, n: usize| ((a - b) / n.max(1) as f64 * 100.0).round() / 100.0;
    let mut out = BTreeMap::new();
    out.insert("line_moved", json!(line));
    out.insert("models", json!(info.models.len()));
    out.insert(
        "structures",
        json!(info.models.iter().map(|m| m.structures).sum::<usize>()),
    );
    out.insert("routes", json!(info.routes.len()));
    out.insert("tile_level", json!(level));
    out.insert("view_tiles", json!([w, h]));
    out.insert("server_start_s", json!((start_s * 100.0).round() / 100.0));
    out.insert(
        "server_read_s",
        stats.get("start_s").cloned().unwrap_or(Value::Null),
    );
    out.insert("idle_mb", json!(idle_mb));
    out.insert(
        "memory",
        json!({
            "after_distinct_views_mb": after_distinct_mb,
            "distinct_viewers": distinct,
            "per_distinct_viewer_mb": per_client(after_distinct_mb, idle_mb, distinct),
            "before_same_view_mb": before_same_mb,
            "after_same_view_mb": after_same_mb,
            "same_view_viewers": same,
            "per_connection_mb": per_client(after_same_mb, before_same_mb, same),
            "with_all_mb": with_all_mb,
            "connected": connected,
        }),
    );
    out.insert("network_info_s", json!((info_s * 1000.0).round() / 1000.0));
    out.insert("first_view_built_ms", summary(&first_view_built));
    out.insert("first_view_cached_ms", summary(&first_view_cached));
    out.insert("tile_built_ms_client", summary(&built_ms));
    out.insert("tile_cached_ms_client", summary(&cached_ms));
    out.insert("tile_built_ms_server", summary(&server_ms("built_ms")));
    out.insert(
        "tile_cached_us_server",
        summary(&server_ms("served_from_cache_us")),
    );
    out.insert(
        "desktop_join_s",
        json!((desktop_join_s * 1000.0).round() / 1000.0),
    );
    out.insert("move_answered_ms", summary(&answered_ms));
    out.insert("move_arrived_ms", summary(&arrived_ms));
    out.insert("move_applied_ms_server", summary(&server_ms("applied_ms")));
    out.insert("area_query_us_server", summary(&area_us));
    out.insert("area_objects", summary(&area_objects));
    out.insert("area_query_ms_database", summary(&db_ms));
    for k in [
        "packages",
        "packages_bytes",
        "shapes",
        "shapes_bytes",
        "shapes_hits",
        "shapes_asked",
        "solved",
        "tiles",
        "tiled_objects",
    ] {
        out.insert(k, stats.get(k).cloned().unwrap_or(Value::Null));
    }
    Ok(json!(out))
}
