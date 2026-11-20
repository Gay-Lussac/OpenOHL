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
/// size and time) and brought back (its time on the server).
pub fn bench(
    database: &str,
    user: &str,
    line: Option<String>,
    rounds: usize,
    work: &Path,
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
    Ok(json!({
        "line": code,
        "route": route,
        "start_s": start_s,
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
