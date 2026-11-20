// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-versions`: phase 11 measured on a network's models
//! (the synthetic country's), as the network's writer (no server running),
//! each measure with split trees and with flat ones on the same model:
//!
//! 1. **a commit of one changed route** (every structure of a route moved
//!    a centimetre, one command), its status before;
//! 2. **a branch over a working set** (the route): ten structures moved
//!    and committed on it, one on main; the diff of the two, ahead and
//!    behind, the merge's plan and the merge;
//! 3. **a merge at a shared tower** (a model whose routes share towers):
//!    a branch over one route moving a tower another route uses, merged
//!    into main (the other route marked out of date).
//!
//! Edits the network (structures moved, revisions, branches).

use std::time::Instant;

use ohl_session::shared::BranchScope;
use ohl_session::versions::Choices;
use ohl_session::{Command, Kind, Session};
use ohl_store_pg::Network;
use serde_json::{Value, json};

use crate::network_bench::summary;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Every structure of `ids` moved by `dx` (one command).
fn move_all(s: &mut Session, user: &str, ids: &[String], dx: f64) -> Result<(), String> {
    let commands: Vec<Command> = ids
        .iter()
        .filter_map(|id| {
            let p = s.line().structure(id)?.position;
            Some(Command::MoveStructure {
                id: id.clone(),
                position: [p[0] + dx, p[1], p[2]],
            })
        })
        .collect();
    s.apply(user, Command::Batch { commands }).map_err(err)?;
    Ok(())
}

/// The measures on model `code` with split trees (`split`) or flat ones.
fn measure(
    net: &Network,
    code: &str,
    user: &str,
    rounds: usize,
    split: bool,
    tag: &str,
) -> Result<Value, String> {
    let t = Instant::now();
    let mut s = Session::open_on(Box::new(net.store(code).map_err(err)?), code).map_err(err)?;
    let open_ms = ms(t);
    s.set_split_trees(split);
    let objects = s.object_keys().count();
    // The first revision of this kind (every object hashed).
    let t = Instant::now();
    let first = s.commit(user, &format!("bench {tag}: before")).is_ok();
    let first_ms = ms(t);
    let route = s
        .line()
        .routes
        .first()
        .map(|r| r.id.clone())
        .ok_or("a model without a route")?;
    let ids: Vec<String> = s
        .route_detail(&route)
        .map_err(err)?
        .structures
        .iter()
        .map(|x| x.id.clone())
        .collect();
    // 1. A commit of one changed route.
    let (mut status_ms, mut commit_ms) = (Vec::new(), Vec::new());
    for k in 0..rounds {
        let dx = if k % 2 == 0 { 0.01 } else { -0.01 };
        move_all(&mut s, user, &ids, dx)?;
        let t = Instant::now();
        let changes = s.status().map_err(err)?.changes.objects.len();
        status_ms.push(ms(t));
        let t = Instant::now();
        s.commit(
            user,
            &format!("bench {tag}: route {route} moved ({changes})"),
        )
        .map_err(err)?;
        commit_ms.push(ms(t));
    }
    // 2. A branch over the route.
    let branch = format!("bench-{tag}");
    s.create_line(user, &branch, "main").map_err(err)?;
    s.set_branch_scope(
        user,
        &branch,
        Some(BranchScope {
            by: user.to_owned(),
            routes: [route.clone()].into(),
            team: Vec::new(),
        }),
    )
    .map_err(err)?;
    let (on_branch, on_main) = ids.split_at(ids.len().min(10).min(ids.len().saturating_sub(1)));
    s.select_line(&branch).map_err(err)?;
    move_all(&mut s, user, on_branch, 0.02)?;
    s.commit(user, "bench: on the branch").map_err(err)?;
    s.select_line("main").map_err(err)?;
    move_all(&mut s, user, &on_main[..on_main.len().min(1)], 0.02)?;
    s.commit(user, "bench: on main").map_err(err)?;
    let t = Instant::now();
    let diff = s.diff_revisions("main", &branch).map_err(err)?;
    let diff_ms = ms(t);
    let t = Instant::now();
    let ab = s.ahead_behind("main", &branch).map_err(err)?;
    let ab_ms = ms(t);
    let t = Instant::now();
    let plan = s.merge_plan(&branch, &Choices::new()).map_err(err)?;
    let plan_ms = ms(t);
    let t = Instant::now();
    s.merge(user, &branch, None, &Choices::new()).map_err(err)?;
    let merge_ms = ms(t);
    // 3. A merge at a shared tower: a structure of another route too.
    let shared = {
        let keys: Vec<(Kind, String)> = ids.iter().map(|i| (Kind::Structure, i.clone())).collect();
        s.object_routes(&keys)
            .into_iter()
            .zip(ids.iter())
            .find(|(r, _)| r.len() > 1)
            .map(|(r, id)| (id.clone(), r))
    };
    let tower = match shared {
        Some((id, routes)) => {
            let b2 = format!("bench-{tag}-shared");
            s.create_line(user, &b2, "main").map_err(err)?;
            s.set_branch_scope(
                user,
                &b2,
                Some(BranchScope {
                    by: user.to_owned(),
                    routes: [route.clone()].into(),
                    team: Vec::new(),
                }),
            )
            .map_err(err)?;
            s.select_line(&b2).map_err(err)?;
            move_all(&mut s, user, std::slice::from_ref(&id), 0.05)?;
            s.commit(user, "bench: a shared tower moved").map_err(err)?;
            s.select_line("main").map_err(err)?;
            let t = Instant::now();
            let out = s.merge(user, &b2, None, &Choices::new()).map_err(err)?;
            let m = ms(t);
            let stale = s.stale_routes().map_err(err)?;
            json!({
                "tower": id,
                "routes": routes,
                "merge_ms": m,
                "notice": out.applied.and_then(|a| a.notice),
                "routes_stale": stale.len(),
            })
        }
        None => Value::Null,
    };
    Ok(json!({
        "trees": if split { "split" } else { "flat" },
        "model": code,
        "objects": objects,
        "route": route,
        "route_structures": ids.len(),
        "open_ms": open_ms,
        "first_commit_ms": first.then_some(first_ms),
        "status_ms": summary(&status_ms),
        "commit_route_ms": summary(&commit_ms),
        "branch": {
            "diff_ms": diff_ms,
            "diff_objects": diff.objects.len(),
            "ahead_behind_ms": ab_ms,
            "ahead_behind": [ab.0, ab.1],
            "merge_plan_ms": plan_ms,
            "merge_local": plan.local.as_ref().map(|k| k.len()),
            "merge_ms": merge_ms,
        },
        "shared_tower": tower,
    }))
}

/// The measures on each of `lines` (default: the network's first), split
/// and flat trees as `modes` says.
pub fn bench(
    database: &str,
    user: &str,
    lines: &[String],
    rounds: usize,
    modes: &[String],
) -> Result<Value, String> {
    let net = Network::connect_writer(database).map_err(err)?;
    let codes: Vec<String> = if lines.is_empty() {
        net.lines()
            .map_err(err)?
            .into_iter()
            .take(1)
            .map(|l| l.id)
            .collect()
    } else {
        lines.to_vec()
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut out = Vec::new();
    for code in &codes {
        for (k, mode) in modes.iter().enumerate() {
            let split = match mode.as_str() {
                "split" => true,
                "flat" => false,
                m => return Err(format!("trees {m}: split or flat")),
            };
            eprintln!("bench-versions: {code}, {mode} trees");
            out.push(measure(
                &net,
                code,
                user,
                rounds,
                split,
                // The same mode twice (opening with the first run's
                // branches): a tag of its own each.
                &format!("{mode}-{stamp}-{k}"),
            )?);
        }
    }
    Ok(json!({ "runs": out }))
}
