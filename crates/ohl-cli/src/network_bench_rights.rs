// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-rights`: phase 7 measured — what the rights cost on
//! a network with many users.
//!
//! 1. The users, groups and assignments made (once; named `rb-…`): users
//!    in groups of 20, assignments of every kind of scope (lines, routes,
//!    parts of routes, regions, kinds on a line, a deny now and then),
//!    each through the rights' one function.
//! 2. The server in this process (the idle solve off): its start (every
//!    object's place kept), then a command's rights decided — its rule and
//!    every object it changed — for the network admin and for a designer
//!    of one route (with the most assignments), while 20 viewers of other
//!    rights view the tiles around the structure moved (the clients that
//!    may not see each change worked out);
//! 3. a viewer of a region joining a view of 4 × 3 tiles: each tile's
//!    rights class and its package filtered, made from scratch and from
//!    the cache, against a network admin's view of the same tiles.
//!
//! Every wait is bounded.

use std::time::{Duration, Instant};

use ohl_server::client::{Joined, NetJoin, join_network};
use ohl_server::network::{NetOptions, Probe};
use ohl_server::server::{Options, Server};
use ohl_server::{ClientMsg, Op, ServerMsg};
use ohl_session::rights::{Scope, Subject};
use ohl_store_pg::Network;
use ohl_store_pg::rights::RightsCommand as R;
use serde_json::{Value, json};

use crate::network_bench::{held, pump, summary, tiles, view};

/// What to measure.
pub struct RightsBench {
    pub database: String,
    pub user: String,
    pub users: usize,
    pub assignments: usize,
    pub moves: usize,
}

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

fn numbers(v: &Value, k: &str) -> Vec<f64> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

fn stats(server: &Server) -> Result<Value, String> {
    serde_json::from_str(&server.probe(Probe::Stats)?).map_err(|e| e.to_string())
}

/// The entries of `k` after the first `from`.
fn since(v: &Value, k: &str, from: usize) -> Vec<f64> {
    numbers(v, k).into_iter().skip(from).collect()
}

fn moved(id: &str, p: [f64; 3]) -> Op {
    Op::Do {
        command: ohl_session::Command::MoveStructure {
            id: id.into(),
            position: p,
        },
    }
}

fn edit(j: &mut Joined, id: &mut u64, model: &str, op: Op) -> Result<f64, String> {
    *id += 1;
    let want = *id;
    let t = Instant::now();
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
        _ => Ok(t.elapsed().as_secs_f64() * 1e3),
    }
}

pub fn bench(r: &RightsBench) -> Result<Value, String> {
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let net = Network::connect(&r.database).map_err(e)?;
    let run = |c: R| net.rights_command(&r.user, &c).map_err(e);
    if net.rights().map_err(e)?.network_admins().is_empty() {
        run(R::Bootstrap {
            user: r.user.clone(),
        })?;
    }
    let lines: Vec<String> = net.lines().map_err(e)?.into_iter().map(|l| l.id).collect();
    let routes = net.route_list().map_err(e)?;
    let centres = net.sample_structures(4000).map_err(e)?;
    if routes.is_empty() || centres.len() < 100 {
        return Err("the network needs routes and 100 structures".into());
    }
    let mut rng = Rng(0x0b5e_55ed);
    // 1. Users, groups, assignments (made once).
    let made = Instant::now();
    let mut assign_ms = Vec::new();
    let have = net.rights().map_err(e)?;
    let groups = r.users.div_ceil(20).max(1);
    let (first_line, first_route) = routes[0].clone();
    let on_first = net
        .structures_on_route(&first_line, &first_route)
        .map_err(e)?;
    if on_first.len() < 4 {
        return Err(format!("route {first_route}: fewer than 4 structures"));
    }
    if !have.users.contains_key("rb-u000") {
        for k in 0..r.users {
            run(R::AddUser {
                name: format!("rb-u{k:03}"),
                display: None,
            })?;
        }
        for g in 0..groups {
            run(R::AddGroup {
                name: format!("rb-g{g:02}"),
                note: String::new(),
            })?;
        }
        for k in 0..r.users {
            run(R::AddMember {
                group: format!("rb-g{:02}", k / 20),
                user: format!("rb-u{k:03}"),
            })?;
        }
        let roles = ["viewer", "designer", "checker", "library_maintainer"];
        for k in 0..r.assignments {
            // A tenth to the groups, the rest to the users.
            let subject = if k % 10 == 0 {
                Subject::Group(format!("rb-g{:02}", rng.below(groups)))
            } else {
                Subject::User(format!("rb-u{:03}", rng.below(r.users)))
            };
            let (line, route) = routes[rng.below(routes.len())].clone();
            let c = centres[rng.below(centres.len())];
            let deny = rng.below(20) == 0;
            let scope = match rng.below(20) {
                0..=6 => Scope::line(&lines[rng.below(lines.len())]),
                7..=11 => Scope {
                    route: Some(route.clone()),
                    ..Scope::default()
                },
                12 => {
                    let st = net.structures_on_route(&line, &route).map_err(e)?;
                    if st.len() < 4 {
                        Scope::line(&line)
                    } else {
                        Scope {
                            route: Some(route.clone()),
                            from_structure: Some(st[1].clone()),
                            to_structure: Some(st[st.len() - 2].clone()),
                            ..Scope::default()
                        }
                    }
                }
                13..=16 => Scope {
                    region: Some(vec![
                        [c[0] - 1500.0, c[1] - 1500.0],
                        [c[0] + 1500.0, c[1] - 1500.0],
                        [c[0] + 1500.0, c[1] + 1500.0],
                        [c[0] - 1500.0, c[1] + 1500.0],
                    ]),
                    ..Scope::default()
                },
                17 | 18 => Scope {
                    line: Some(line.clone()),
                    kind: Some(["structure", "section", "meta"][rng.below(3)].into()),
                    ..Scope::default()
                },
                _ => Scope {
                    line: Some(line.clone()),
                    library_kind: Some(["costs", "cables"][rng.below(2)].into()),
                    ..Scope::default()
                },
            };
            let t = Instant::now();
            run(R::Assign {
                subject,
                role: (!deny || rng.below(2) == 0)
                    .then(|| roles[rng.below(roles.len())].to_owned()),
                deny,
                scope,
                note: "the bench".into(),
            })?;
            assign_ms.push(t.elapsed().as_secs_f64() * 1e3);
        }
        // The two measured: a designer of the first route, a viewer of a
        // region (each in two groups besides their own assignments).
        for (name, role, scope) in [
            (
                "rb-designer",
                "designer",
                Scope {
                    route: Some(first_route.clone()),
                    ..Scope::default()
                },
            ),
            (
                "rb-region",
                "viewer",
                Scope {
                    region: Some({
                        let c = centres[7];
                        vec![
                            [c[0] - 1800.0, c[1] - 1200.0],
                            [c[0] + 1800.0, c[1] - 1200.0],
                            [c[0] + 1800.0, c[1] + 1200.0],
                            [c[0] - 1800.0, c[1] + 1200.0],
                        ]
                    }),
                    ..Scope::default()
                },
            ),
        ] {
            run(R::AddUser {
                name: name.into(),
                display: None,
            })?;
            for g in [0, groups / 2] {
                run(R::AddMember {
                    group: format!("rb-g{g:02}"),
                    user: name.into(),
                })?;
            }
            run(R::Assign {
                subject: Subject::User(name.into()),
                role: Some(role.into()),
                deny: false,
                scope,
                note: "the bench".into(),
            })?;
        }
    }
    let made_s = made.elapsed().as_secs_f64();
    let data = net.rights().map_err(e)?;
    let of = |u: &str| data.assignments_of(u).len();
    let (n_users, n_assignments) = (data.users.len(), data.assignments.len());
    let designer_assignments = of("rb-designer");
    drop(net);

    // 2. The server in this process.
    eprintln!("bench-rights: the server");
    let t = Instant::now();
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
            code: Some("bench-rights-code".into()),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: r.database.clone(),
            idle_solve: false,
            ..NetOptions::default()
        },
    )?;
    let start_s = t.elapsed().as_secs_f64();
    let s0 = stats(&server)?;
    let s = &on_first[on_first.len() / 2];
    let p = {
        let n = Network::connect(&r.database).map_err(e)?;
        let (b, _) = n.placement_of(&first_line, "structure", s).map_err(e)?;
        let b = b.ok_or("no place")?;
        [(b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0]
    };
    let info = {
        let mut j = join_network(
            &server.invitation_for(&r.user),
            &r.user,
            &NetJoin::default(),
            &mut |_| {},
        )?;
        let got = pump(&mut j, Duration::from_secs(60), |m| {
            matches!(m, ServerMsg::Network(_))
        })?;
        match got.last().cloned() {
            Some(ServerMsg::Network(i)) => i,
            _ => return Err("no network info".into()),
        }
    };
    // 20 viewers of other rights around the structure moved.
    let near = view(p, 2, 2, info.grid_size, info.tile_level);
    let mut viewers = Vec::new();
    for k in 0..20.min(r.users) {
        let name = format!("rb-u{k:03}");
        let mut j = join_network(
            &server.invitation_for(&name),
            &name,
            &NetJoin {
                tiles: held(&near),
                ..NetJoin::default()
            },
            &mut |_| {},
        )?;
        tiles(&mut j, near.len())?;
        viewers.push(j);
    }
    // The moves: the network admin's, then the route's designer's.
    let z = ohl_store_pg::Network::connect(&r.database)
        .and_then(|n| n.placement_of(&first_line, "structure", s))
        .map_err(e)?;
    let z = z.0.map_or(0.0, |_| 0.0);
    let base = {
        let n = Network::connect(&r.database).map_err(e)?;
        let st =
            ohl_session::Session::open_on(Box::new(n.store(&first_line).map_err(e)?), &first_line)
                .map_err(|x| x.to_string())?;
        st.line()
            .structures
            .iter()
            .find(|x| &x.id == s)
            .map(|x| x.position)
            .ok_or("no structure")?
    };
    let _ = z;
    let mut out = serde_json::Map::new();
    let mut id = 0u64;
    for who in [r.user.as_str(), "rb-designer"] {
        let before = stats(&server)?;
        let (n_check, n_applied, n_change) = (
            numbers(&before, "rights_check_us").len(),
            numbers(&before, "applied_ms").len(),
            numbers(&before, "rights_change_us").len(),
        );
        let mut j = join_network(
            &server.invitation_for(who),
            who,
            &NetJoin::default(),
            &mut |_| {},
        )?;
        let mut answered = Vec::new();
        for k in 0..r.moves {
            let dx = if k % 2 == 0 { 0.5 } else { 0.0 };
            answered.push(edit(
                &mut j,
                &mut id,
                &first_line,
                moved(s, [base[0] + dx, base[1], base[2]]),
            )?);
        }
        let after = stats(&server)?;
        out.insert(
            format!(
                "moves_{}",
                if who == r.user {
                    "network_admin"
                } else {
                    "route_designer"
                }
            ),
            json!({
                "rights_check_us": summary(&since(&after, "rights_check_us", n_check)),
                "applied_ms": summary(&since(&after, "applied_ms", n_applied)),
                "who_may_not_see_us": summary(&since(&after, "rights_change_us", n_change)),
                "answered_ms": summary(&answered),
            }),
        );
        drop(j);
        std::thread::sleep(Duration::from_millis(300));
    }
    drop(viewers);

    // 3. A region's viewer on a view of 4 × 3 tiles, made and cached,
    // against the network admin on the same view.
    let c = centres[7];
    let v = view(c, 4, 3, info.grid_size, info.tile_level);
    let mut views = serde_json::Map::new();
    for (label, who, clear) in [
        ("network_admin_made", r.user.as_str(), true),
        ("region_viewer_made", "rb-region", true),
        ("region_viewer_cached", "rb-region", false),
        ("network_admin_cached", r.user.as_str(), false),
    ] {
        std::thread::sleep(Duration::from_millis(400));
        if clear {
            server.probe(Probe::ClearCaches)?;
        }
        let before = stats(&server)?;
        let (n_class, n_filter, n_built) = (
            numbers(&before, "rights_class_us").len(),
            numbers(&before, "rights_filter_us").len(),
            numbers(&before, "built_ms").len(),
        );
        let t = Instant::now();
        let mut j = join_network(
            &server.invitation_for(who),
            who,
            &NetJoin {
                tiles: held(&v),
                ..NetJoin::default()
            },
            &mut |_| {},
        )?;
        tiles(&mut j, v.len())?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let after = stats(&server)?;
        views.insert(
            label.into(),
            json!({
                "view_ms": ms,
                "tiles": v.len(),
                "class_us": summary(&since(&after, "rights_class_us", n_class)),
                "filter_us": summary(&since(&after, "rights_filter_us", n_filter)),
                "built_ms": summary(&since(&after, "built_ms", n_built)),
            }),
        );
        drop(j);
    }
    let last = stats(&server)?;
    server.stop();
    Ok(json!({
        "users": n_users,
        "assignments": n_assignments,
        "route_designer_assignments": designer_assignments,
        "made_s": made_s,
        "assign_ms": summary(&assign_ms),
        "server_start_s": start_s,
        "server_start_inside_s": s0.get("start_s"),
        "places": s0.get("places"),
        "compile_us": summary(&numbers(&last, "rights_compile_us")),
        "moves": Value::Object(out),
        "views": Value::Object(views),
    }))
}
