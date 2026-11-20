// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network export-working-set`, `offline-bundle`, `import-bundle`,
//! `check-offline`, `bench-offline`: the offline working set (ADR-0013,
//! phase 12). Exporting and bringing back host the network in this
//! process as its one writer (refused while its server runs — a client
//! of the running server asks it with the protocol's messages instead);
//! making the bundle and checking offline need no database.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ohl_server::network::NetOptions;
use ohl_server::server::{OfflineJob, Options, Server};
use ohl_server::{BroughtBack, OfflineExported, ScopedCheck};
use ohl_session::versions::Bundle;
use ohl_session::working_set::{Pick, WorkingSetSpec};
use ohl_session::{Command, Session};
use serde_json::{Value, json};

use crate::network_bench::summary;

/// The working set picked on the command line.
#[derive(clap::Args, Debug, Clone)]
pub struct PickArgs {
    /// The line (model).
    #[arg(long)]
    pub line: String,
    /// A route picked whole (repeatable); with --from and --to, or
    /// --stations, the one route of the part.
    #[arg(long = "route")]
    pub routes: Vec<String>,
    /// A part of the route from this structure …
    #[arg(long, requires = "to")]
    pub from: Option<String>,
    /// … to this one.
    #[arg(long, requires = "from")]
    pub to: Option<String>,
    /// A part of the route by station range, m: FROM:TO.
    #[arg(long, conflicts_with_all = ["from", "to"])]
    pub stations: Option<String>,
}

impl PickArgs {
    /// The working set.
    pub fn spec(&self) -> Result<WorkingSetSpec, String> {
        let part = self.from.is_some() || self.stations.is_some();
        if self.routes.is_empty() {
            return Err("pick a route (--route)".into());
        }
        if part && self.routes.len() != 1 {
            return Err("a part (--from and --to, or --stations) is of one route".into());
        }
        let picks = if let (Some(from), Some(to)) = (&self.from, &self.to) {
            vec![Pick::Structures {
                route: self.routes[0].clone(),
                from: from.clone(),
                to: to.clone(),
            }]
        } else if let Some(st) = &self.stations {
            let (a, b) = st
                .split_once(':')
                .ok_or("--stations FROM:TO (metres along the route)")?;
            let num = |x: &str| {
                x.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("not a station: {x}"))
            };
            vec![Pick::Stations {
                route: self.routes[0].clone(),
                alignment: None,
                from: num(a)?,
                to: num(b)?,
            }]
        } else {
            self.routes
                .iter()
                .map(|r| Pick::Route { route: r.clone() })
                .collect()
        };
        Ok(WorkingSetSpec {
            name: String::new(),
            model: self.line.clone(),
            picks,
        })
    }
}

/// The network hosted in this process (its one writer: refused while
/// its server runs).
pub fn host(database: &str) -> Result<Server, String> {
    Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
            code: Some(format!("offline-{}", std::process::id())),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: database.to_owned(),
            idle_solve: false,
            packagers: 1,
            readers: 1,
            ..NetOptions::default()
        },
    )
    .map_err(|e| {
        format!(
            "{e} (the network's server may be running: a client of it exports and brings back \
             with the protocol's messages)"
        )
    })
}

/// Exports `spec` for `user` into `out`.
pub fn export(
    server: &Server,
    user: &str,
    spec: WorkingSetSpec,
    reach: Option<f64>,
    out: &Path,
) -> Result<OfflineExported, String> {
    let v = server.offline(
        user,
        OfflineJob::Export {
            working_set: spec,
            reach,
            out: out.to_owned(),
        },
    )?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Brings `bundle` back for `user`.
pub fn bring_back(
    server: &Server,
    user: &str,
    bundle: Bundle,
    branch: Option<String>,
) -> Result<BroughtBack, String> {
    let v = server.offline(
        user,
        OfflineJob::Import {
            bundle: Box::new(bundle),
            branch,
        },
    )?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// The bundle of offline working set `file` (only its changes) written to
/// `out` (must not exist).
pub fn make_bundle(file: &Path, out: &Path) -> Result<Bundle, String> {
    if out.exists() {
        return Err(format!("{}: exists already", out.display()));
    }
    let s = Session::open(file).map_err(|e| e.to_string())?;
    let b = s.offline_bundle().map_err(|e| e.to_string())?;
    std::fs::write(out, serde_json::to_string(&b).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(b)
}

/// A bundle read from a file.
pub fn read_bundle(path: &Path) -> Result<Bundle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: not a bundle: {e}", path.display()))
}

/// The check asked on the command line.
pub fn scoped_check(
    check: &str,
    case: Option<String>,
    rules: Option<PathBuf>,
    structures: Vec<String>,
    distance: Option<f64>,
    amps: Option<f64>,
    limit_ut: Option<f64>,
) -> Result<ScopedCheck, String> {
    let case = || case.clone().ok_or("--case NAME: the weather case");
    Ok(match check {
        "clearances" => {
            let rules = rules.ok_or("--rules FILE: the clearance rules")?;
            ScopedCheck::Clearances {
                case: case()?,
                rules: std::fs::read_to_string(&rules)
                    .map_err(|e| format!("{}: {e}", rules.display()))?,
            }
        }
        "fields" => ScopedCheck::Fields {
            case: case()?,
            distance,
            amps,
            limit_ut,
        },
        "loads" => ScopedCheck::StructureLoads { structures },
        other => {
            return Err(format!(
                "check {other}: clearances, fields or loads (structure loads)"
            ));
        }
    })
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// `bench-offline`: an offline working set of a whole route exported (its
/// size and time), worked on (bundles of 20 and of 500 changes, their
/// size and time) and brought back (its time on the server); with
/// `during_reach`, phase 14: a move's latency while the route is exported
/// with that reach, on the authority and on the exports' thread
/// ([`moves_during_export`]).
pub fn bench(
    database: &str,
    user: &str,
    line: Option<String>,
    rounds: usize,
    work: &Path,
    during_reach: Option<f64>,
) -> Result<Value, String> {
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = ohl_store_pg::Network::connect(database).map_err(e)?;
    let code = match line {
        Some(l) => l,
        None => {
            let mut found = None;
            for l in net.lines().map_err(e)? {
                if !ohl_store_pg::import::twins(&net, &l.id, 0.5)
                    .map_err(e)?
                    .is_empty()
                {
                    found = Some(l.id);
                    break;
                }
            }
            found.ok_or("no line with a tower shared with another line")?
        }
    };
    let route = net
        .route_list()
        .map_err(e)?
        .into_iter()
        .find(|(l, _)| *l == code)
        .map(|(_, r)| r)
        .ok_or("no route")?;
    drop(net);
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let t = Instant::now();
    let server = host(database)?;
    let start_s = t.elapsed().as_secs_f64();
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![Pick::Route {
            route: route.clone(),
        }],
    };
    eprintln!("bench-offline: line {code}, route {route}");
    let mut export_ms = Vec::new();
    let mut server_ms = Vec::new();
    let mut last = None;
    for k in 0..rounds.max(1) {
        let path = work.join(format!("export-{k}.ohl"));
        let _ = std::fs::remove_file(&path);
        let t = Instant::now();
        let out = export(&server, user, spec.clone(), None, &path)?;
        export_ms.push(ms(t));
        server_ms.push(out.seconds * 1e3);
        last = Some((out, path));
    }
    let (out, path) = last.expect("a round");
    // Offline work: N structures of the working set moved, committed.
    let mut results = Vec::new();
    for (n, commits) in [(20usize, 1usize), (500, 5)] {
        let file = work.join(format!("work-{n}.ohl"));
        std::fs::copy(&path, &file).map_err(|e| e.to_string())?;
        let mut s = Session::open(&file).map_err(|e| e.to_string())?;
        let ids: Vec<String> = s
            .part()
            .ok_or("not a part")?
            .iter()
            .filter(|(k, _)| *k == ohl_session::Kind::Structure)
            .map(|(_, id)| id.clone())
            .collect();
        let t = Instant::now();
        let per = n.div_ceil(commits);
        for k in 0..n {
            let id = &ids[k % ids.len()];
            let p = s.line().structure(id).ok_or("no structure")?.position;
            s.apply(
                user,
                Command::MoveStructure {
                    id: id.clone(),
                    position: [p[0] + 0.01, p[1], p[2]],
                },
            )
            .map_err(|e| e.to_string())?;
            if (k + 1) % per == 0 || k + 1 == n {
                s.commit(user, &format!("bench: {} changes", k + 1))
                    .map_err(|e| e.to_string())?;
            }
        }
        let edit_ms = ms(t);
        drop(s);
        let bundle_path = work.join(format!("bundle-{n}.json"));
        let _ = std::fs::remove_file(&bundle_path);
        let t = Instant::now();
        let bundle = make_bundle(&file, &bundle_path)?;
        let bundle_ms = ms(t);
        let bytes = std::fs::metadata(&bundle_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let t = Instant::now();
        let back = bring_back(&server, user, bundle.clone(), None)?;
        let back_ms = ms(t);
        results.push(json!({
            "changes": n,
            "commits": commits,
            "objects_changed": back.puts + back.deletes,
            "edits_offline_ms": edit_ms,
            "bundle_ms": bundle_ms,
            "bundle_bytes": bytes,
            "bundle_revisions": bundle.revisions.len(),
            "bundle_blobs": bundle.blobs.len(),
            "brought_back_ms": back_ms,
            "brought_back_server_ms": back.seconds * 1e3,
            "branch": back.branch,
            "conflicts": back.conflicts.len(),
        }));
    }
    server.stop();
    std::thread::sleep(Duration::from_millis(100));
    let mut during = serde_json::Map::new();
    if let Some(reach) = during_reach {
        for (name, worker) in [("on_the_authority", false), ("on_the_thread", true)] {
            eprintln!("bench-offline: moves while the route is exported ({name}, {reach} m)");
            during.insert(
                name.to_owned(),
                moves_during_export(database, user, worker, &spec, reach, work)?,
            );
        }
    }
    Ok(json!({
        "line": code,
        "route": route,
        "start_s": start_s,
        "moves_during_an_export": during,
        "export": {
            "ms": summary(&export_ms),
            "server_ms": summary(&server_ms),
            "bytes": out.bytes,
            "wanted": out.wanted,
            "context": out.context,
            "neighbour_lines": out.neighbour_lines,
            "neighbour_sections": out.neighbour_sections,
            "recorded_first": out.recorded_first,
        },
        "bundles": results,
    }))
}

/// Phase 14: a move's latency (sent, applied, answered) by another user,
/// again and again while the working set `spec` is exported for `user`
/// with `reach` (the export written on the authority, as before, or on
/// the exports' thread). The export through the command line's way (the
/// network hosted here: no message's limit on the file), in a thread of
/// its own; the mover is `bench-mover`, a designer of the network (made
/// once). Every wait is bounded.
pub fn moves_during_export(
    database: &str,
    user: &str,
    worker: bool,
    spec: &WorkingSetSpec,
    reach: f64,
    work: &Path,
) -> Result<Value, String> {
    use ohl_server::client::{Joined, NetJoin, join_network};
    use ohl_server::{ClientMsg, ServerMsg};
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = ohl_store_pg::Network::connect(database).map_err(e)?;
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
                note: "bench-offline".into(),
            },
        )
        .map_err(e)?;
    }
    // A structure of another line than the exported one, moved and back
    // (a network of one model: its last structure).
    let others: Vec<String> = net.lines().map_err(e)?.into_iter().map(|l| l.id).collect();
    let (other, own) = match others.iter().rfind(|c| **c != spec.model) {
        Some(c) => (c.clone(), false),
        None => (spec.model.clone(), true),
    };
    let s = Session::open_on(Box::new(net.store(&other).map_err(e)?), &other)
        .map_err(|x| x.to_string())?;
    let st = if own {
        s.line().structures.last()
    } else {
        s.line().structures.first()
    }
    .ok_or("no structure")?;
    let (sid, p) = (st.id.clone(), st.position);
    drop(s);
    drop(net);
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
            code: Some(format!("offline-{}", std::process::id())),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: database.to_owned(),
            idle_solve: false,
            export_worker: worker,
            ..NetOptions::default()
        },
    )?;
    let mut moving = join_network(
        &server.invitation_for(mover),
        mover,
        &NetJoin::default(),
        &mut |_| {},
    )?;
    crate::network_bench::pump(&mut moving, Duration::from_secs(120), |m| {
        matches!(m, ServerMsg::Network(_))
    })?;
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
                    command: Command::MoveStructure {
                        id: sid.clone(),
                        position: [p[0] + dx, p[1], p[2]],
                    },
                },
                line: "main".into(),
            })
            .map_err(|e| e.to_string())?;
        let want = id;
        let got = crate::network_bench::pump(
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
    // A first export (the main line recorded, what an export reads made
    // once), then the moves alone.
    let first = work.join(format!("during-first-{worker}.ohl"));
    let _ = std::fs::remove_file(&first);
    export(&server, user, spec.clone(), Some(reach), &first)?;
    let idle: Vec<f64> = (0..10)
        .map(|k| move_once(&mut moving, k))
        .collect::<Result<_, _>>()?;
    let out = work.join(format!("during-{worker}.ohl"));
    let _ = std::fs::remove_file(&out);
    let mut during = Vec::new();
    let mut k = 0;
    let started = Instant::now();
    let done = std::thread::scope(|s| -> Result<OfflineExported, String> {
        let h = s.spawn(|| export(&server, user, spec.clone(), Some(reach), &out));
        while !h.is_finished() {
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("the export did not end in 30 minutes".into());
            }
            during.push(move_once(&mut moving, k)?);
            k += 1;
        }
        h.join()
            .map_err(|_| "the export's thread panicked".to_owned())?
    })?;
    let export_s = started.elapsed().as_secs_f64();
    // The structure back where it was.
    if k % 2 == 1 {
        move_once(&mut moving, 1)?;
    }
    // The export worked on (20 structures moved, committed) and brought
    // back as a bundle while the other user moves (2026-11-20: its
    // branch's line made ready on the exports' thread).
    let worked = work.join(format!("during-work-{worker}.ohl"));
    let bundle_path = work.join(format!("during-bundle-{worker}.json"));
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::copy(&out, &worked).map_err(|e| e.to_string())?;
    {
        let mut s = Session::open(&worked).map_err(|e| e.to_string())?;
        let ids: Vec<String> = s
            .part()
            .ok_or("not a part")?
            .iter()
            .filter(|(k, _)| *k == ohl_session::Kind::Structure)
            .map(|(_, id)| id.clone())
            .take(20)
            .collect();
        for id in &ids {
            let p = s.line().structure(id).ok_or("no structure")?.position;
            s.apply(
                user,
                Command::MoveStructure {
                    id: id.clone(),
                    position: [p[0] + 0.01, p[1], p[2]],
                },
            )
            .map_err(|e| e.to_string())?;
        }
        s.commit(user, "bench: 20 changes")
            .map_err(|e| e.to_string())?;
    }
    let bundle = make_bundle(&worked, &bundle_path)?;
    let mut bundle_during = Vec::new();
    let mut k = 0;
    let started = Instant::now();
    let back = std::thread::scope(|s| -> Result<BroughtBack, String> {
        let h = s.spawn(|| bring_back(&server, user, bundle.clone(), None));
        while !h.is_finished() {
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("the bundle did not come back in 30 minutes".into());
            }
            bundle_during.push(move_once(&mut moving, k)?);
            k += 1;
        }
        h.join()
            .map_err(|_| "the bundle's thread panicked".to_owned())?
    })?;
    let bundle_s = started.elapsed().as_secs_f64();
    if k % 2 == 1 {
        move_once(&mut moving, 1)?;
    }
    let stats: Value = serde_json::from_str(&server.probe(ohl_server::network::Probe::Stats)?)
        .map_err(|e| e.to_string())?;
    drop(moving);
    server.stop();
    let _ = std::fs::remove_file(&bundle_path);
    for f in [&first, &out, &worked] {
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", f.display()));
        }
    }
    Ok(json!({
        "reach_m": reach,
        "moved": format!("{other} {sid}"),
        "export_s": export_s,
        "export_server_s": done.seconds,
        "bytes": done.bytes,
        "wanted": done.wanted,
        "context": done.context,
        "neighbour_lines": done.neighbour_lines,
        "neighbour_sections": done.neighbour_sections,
        "stale": done.stale,
        "changed_meanwhile": done.changed_meanwhile,
        "export_plan_ms": stats["export_plan_ms"],
        "moves_during": during.len(),
        "move_ms_idle": summary(&idle),
        "move_ms_during": summary(&during),
        "move_ms_during_max": during.iter().cloned().fold(0.0, f64::max),
        "bundle": {
            "changes": 20,
            "s": bundle_s,
            "server_s": back.seconds,
            "puts": back.puts,
            "conflicts": back.conflicts.len(),
            "bundle_plan_ms": stats["bundle_plan_ms"],
            "moves_during": bundle_during.len(),
            "move_ms_during": summary(&bundle_during),
            "move_ms_during_max": bundle_during.iter().cloned().fold(0.0, f64::max),
        },
    }))
}
