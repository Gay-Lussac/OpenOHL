// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network bench-library`: phase 9 measured on a network (the
//! synthetic country's 147 lines).
//!
//! 1. As the network's writer (no server running): the shipped wire and
//!    weather files put into the network's library and released; the use
//!    index made again from every line; **Update library pins** of every
//!    line in design (each line its files put, one log entry); a wire the
//!    lines use changed and released, the pins updated again (the routes
//!    using it marked out of date); the use index asked by a network
//!    admin and by a viewer of one route.
//! 2. The server in this process: its start, then moves of a structure
//!    (each one's use index kept in its unit of work), against phase 7's.
//!
//! Every wait is bounded.

use std::time::{Duration, Instant};

use ohl_server::client::{Joined, NetJoin, join_network};
use ohl_server::network::{NetOptions, Probe};
use ohl_server::server::{Options, Server};
use ohl_server::{ClientMsg, Op, ServerMsg};
use ohl_session::Session;
use ohl_session::rights::{Scope, Subject};
use ohl_store_pg::Network;
use ohl_store_pg::library::{LibraryCommand as L, RightsRoles, apply_pins, plan_pins};
use ohl_store_pg::rights::{DbResolve, RightsCommand as R};
use serde_json::{Value, json};

use crate::network_bench::{pump, summary};

const CABLES: &str = include_str!("../../../libraries/cables/acsr-examples.ohlw");
const WEATHER: &str = include_str!("../../../libraries/weather/default-weather-cases.ohlwc");
const DRAKE: &str = "acsr-drake-26-7";

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

/// Every line in design moved to the latest releases of `kinds` (all
/// when empty): per line seconds, files changed, routes marked stale.
fn update_all(net: &Network, user: &str, kinds: &[String]) -> Result<Value, String> {
    let me = net
        .rights()
        .map_err(|e| e.to_string())?
        .compile(user, &DbResolve(net));
    let lines = net.following_lines().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let (mut per_line, mut plan_ms, mut files, mut stale, mut moved) =
        (Vec::new(), Vec::new(), 0usize, 0usize, 0usize);
    for code in &lines {
        let t = Instant::now();
        let mut s = Session::open_on(Box::new(net.store(code).map_err(|e| e.to_string())?), code)
            .map_err(|e| e.to_string())?;
        s.set_role_source(Some(std::sync::Arc::new(RightsRoles {
            line: code.clone(),
            rights: me.clone(),
        })));
        let p = Instant::now();
        let plan = plan_pins(
            net,
            &mut s,
            code,
            (!kinds.is_empty()).then_some(kinds),
            None,
        )
        .map_err(|e| format!("{code}: {e}"))?;
        plan_ms.push(p.elapsed().as_secs_f64() * 1e3);
        if plan.moves() {
            apply_pins(net, &mut s, &me, &plan).map_err(|e| format!("{code}: {e}"))?;
            moved += 1;
            files += plan.changes().len();
            stale += plan.stale().len();
        }
        per_line.push(t.elapsed().as_secs_f64());
    }
    Ok(json!({
        "lines": lines.len(),
        "moved": moved,
        "files_changed": files,
        "routes_marked_stale": stale,
        "seconds": started.elapsed().as_secs_f64(),
        "per_line_s": summary(&per_line),
        "plan_ms": summary(&plan_ms),
    }))
}

pub fn bench(database: &str, user: &str, queries: usize) -> Result<Value, String> {
    let e = |x: ohl_store_pg::NetError| x.to_string();
    let mut out = serde_json::Map::new();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let first_line;
    {
        let net = Network::connect_writer(database).map_err(e)?;
        if net.rights().map_err(e)?.network_admins().is_empty() {
            net.rights_command(
                user,
                &R::Bootstrap {
                    user: user.to_owned(),
                },
            )
            .map_err(e)?;
        }
        let lib = |c: L| net.library_command(user, &c).map_err(e);
        let lines = net.lines().map_err(e)?;
        first_line = lines.first().ok_or("no line in the network")?.id.clone();
        out.insert("lines".into(), json!(lines.len()));
        // 1. The files, released.
        let t = Instant::now();
        lib(L::PutFile {
            kind: "cables".into(),
            file: "acsr-examples.ohlw".into(),
            text: CABLES.into(),
            emergency: true,
        })?;
        lib(L::PutFile {
            kind: "weather".into(),
            file: "default-weather-cases.ohlwc".into(),
            text: WEATHER.into(),
            emergency: true,
        })?;
        let put_ms = t.elapsed().as_secs_f64() * 1e3;
        let mut release_ms = Vec::new();
        for kind in ["cables", "weather"] {
            let t = Instant::now();
            let r = lib(L::Release {
                kind: kind.into(),
                tag: format!("b{stamp}-1"),
                notes: "bench".into(),
            });
            match r {
                Ok(_) => release_ms.push(t.elapsed().as_secs_f64() * 1e3),
                Err(x) if x.contains("not changed") => {}
                Err(x) => return Err(x),
            }
        }
        out.insert("put_files_ms".into(), json!((put_ms * 10.0).round() / 10.0));
        out.insert("release_ms".into(), summary(&release_ms));
        // The use index made again from every line.
        let t = Instant::now();
        let mut rows = 0;
        for l in &lines {
            let s = Session::open_on(Box::new(net.store(&l.id).map_err(e)?), &l.id)
                .map_err(|x| x.to_string())?;
            rows += net.reindex_uses(&l.id, &s).map_err(e)?;
        }
        out.insert(
            "reindex".into(),
            json!({"rows": rows, "seconds": t.elapsed().as_secs_f64()}),
        );
        // Update library pins: the files put in every line.
        eprintln!("bench-library: Update library pins of every line in design");
        out.insert("update_pins_first".into(), update_all(&net, user, &[])?);
        // A wire every line uses changed, released, the pins moved again.
        let changed = CABLES.replace("1.108 in", &format!("1.108{} in", stamp % 7 + 1));
        lib(L::PutFile {
            kind: "cables".into(),
            file: "acsr-examples.ohlw".into(),
            text: changed,
            emergency: true,
        })?;
        let t = Instant::now();
        lib(L::Release {
            kind: "cables".into(),
            tag: format!("b{stamp}-2"),
            notes: "bench: a wire changed".into(),
        })?;
        out.insert(
            "release_changed_ms".into(),
            json!(t.elapsed().as_secs_f64() * 1e3),
        );
        eprintln!("bench-library: Update library pins after a wire changed");
        out.insert(
            "update_pins_changed".into(),
            update_all(&net, user, &["cables".to_owned()])?,
        );
        // The use index asked: a network admin; a viewer of one route.
        let routes = net.route_list().map_err(e)?;
        let (rline, route) = routes.first().cloned().ok_or("no route")?;
        let viewer = "lb-viewer";
        if !net.rights().map_err(e)?.users.contains_key(viewer) {
            net.rights_command(
                user,
                &R::AddUser {
                    name: viewer.into(),
                    display: None,
                },
            )
            .map_err(e)?;
            for scope in [
                Scope {
                    library_kind: Some("cables".into()),
                    ..Scope::default()
                },
                Scope {
                    line: Some(rline.clone()),
                    route: Some(route.clone()),
                    ..Scope::default()
                },
            ] {
                net.rights_command(
                    user,
                    &R::Assign {
                        subject: Subject::User(viewer.into()),
                        role: Some("viewer".into()),
                        deny: false,
                        scope,
                        note: "bench-library".into(),
                    },
                )
                .map_err(e)?;
            }
        }
        let data = net.rights().map_err(e)?;
        // The wire every section and jumper uses, and a weather case every
        // line holds (a file's entry).
        for (label, who, kind, entry) in [
            ("network_admin", user, "cables", DRAKE),
            ("route_viewer", viewer, "cables", DRAKE),
            ("network_admin_weather_case", user, "weather", "-20 °C"),
        ] {
            let me = data.compile(who, &DbResolve(&net));
            let mut ms = Vec::new();
            let mut n = 0;
            for _ in 0..queries.max(1) {
                let t = Instant::now();
                n = net.uses(&me, kind, entry).map_err(e)?.len();
                ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
            out.insert(
                format!("uses_{label}"),
                json!({"rows_seen": n, "ms": summary(&ms)}),
            );
        }
        let t = Instant::now();
        let status = net
            .library_status(&data.compile(user, &DbResolve(&net)), None)
            .map_err(e)?;
        out.insert(
            "status_all_lines_ms".into(),
            json!({"lines": status.len(), "ms": t.elapsed().as_secs_f64() * 1e3}),
        );
    }

    // 2. The server: its start, then moves with the use index kept.
    eprintln!("bench-library: the server");
    let t = Instant::now();
    let server = {
        let mut tries = 0;
        loop {
            match Server::start_network(
                Options {
                    bind: "127.0.0.1:0".parse().map_err(|_| "address")?,
                    code: Some("bench-library-code".into()),
                    newcomers: None,
                    ..Options::default()
                },
                NetOptions {
                    database: database.to_owned(),
                    idle_solve: false,
                    ..NetOptions::default()
                },
            ) {
                Ok(s) => break s,
                Err(x) if x.contains("writer") && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(x) => return Err(x),
            }
        }
    };
    out.insert("server_start_s".into(), json!(t.elapsed().as_secs_f64()));
    let (sid, base) = {
        let n = Network::connect(database).map_err(e)?;
        let s = Session::open_on(Box::new(n.store(&first_line).map_err(e)?), &first_line)
            .map_err(|x| x.to_string())?;
        let st = &s.line().structures[s.line().structures.len() / 2];
        (st.id.clone(), st.position)
    };
    let mut j = join_network(
        &server.invitation_for(user),
        user,
        &NetJoin::default(),
        &mut |_| {},
    )?;
    let before: Value =
        serde_json::from_str(&server.probe(Probe::Stats)?).map_err(|x| x.to_string())?;
    let n0 = before
        .get("applied_ms")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut id = 0;
    let mut answered = Vec::new();
    for k in 0..40 {
        let dx = if k % 2 == 0 { 0.5 } else { 0.0 };
        answered.push(edit(
            &mut j,
            &mut id,
            &first_line,
            Op::Do {
                command: ohl_session::Command::MoveStructure {
                    id: sid.clone(),
                    position: [base[0] + dx, base[1], base[2]],
                },
            },
        )?);
    }
    let after: Value =
        serde_json::from_str(&server.probe(Probe::Stats)?).map_err(|x| x.to_string())?;
    let applied: Vec<f64> = after
        .get("applied_ms")
        .and_then(Value::as_array)
        .map(|a| a.iter().skip(n0).filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    out.insert(
        "moves".into(),
        json!({"applied_ms": summary(&applied), "answered_ms": summary(&answered)}),
    );
    drop(j);
    drop(server);
    Ok(Value::Object(out))
}
