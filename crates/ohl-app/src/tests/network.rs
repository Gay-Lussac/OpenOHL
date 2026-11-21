// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: OHL-Design on a network (phase 6) without a server. A working
//! set's replica written from a whole model as the server writes it,
//! opened like any project, then fed the server's rows after edits of the
//! whole model: taken in place (the line kept beside the session checked
//! against the line made again whole after every edit, as every test's),
//! and the replica's line the one a replica written again holds. The Join
//! a network window's picture (`OHL_SHOTS`).

use super::*;
use ohl_server::{Position, RowsMsg};
use ohl_session::working_set::{Pick, WORKING_SET_KEY, WorkingSetSpec};
use ohl_session::{Command, Session};

#[test]
fn a_replica_takes_the_servers_rows_in_place() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-network-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    let mut whole = Session::open(&dir.join("whole.ohl")).unwrap();
    whole.import(&line, "tester").unwrap();
    let detail = whole.route_detail("main").unwrap();
    let st = |k: usize| detail.structures[k].station.unwrap();
    let picks = vec![Pick::Stations {
        route: "main".into(),
        alignment: None,
        from: st(1) + 1.0,
        to: st(2) - 1.0,
    }];
    let spec = WorkingSetSpec {
        name: String::new(),
        model: "A".into(),
        picks: picks.clone(),
    };
    let mut extent = whole.pick_extent(&picks).unwrap();
    let path = dir.join("replica.ohl");
    whole
        .write_working_set(
            &extent,
            &path,
            "tester",
            &[(
                WORKING_SET_KEY.to_owned(),
                serde_json::to_string(&spec).unwrap(),
            )],
        )
        .unwrap();
    let mut m = model::load(
        Sources {
            file: path,
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert_eq!(
        m.session().unwrap().working_set_spec().unwrap(),
        Some(spec.clone())
    );
    let mut users = whole.extent_users(&extent);
    let ids: Vec<(String, [f64; 3])> = whole
        .line()
        .structures
        .iter()
        .map(|s| (s.id.clone(), s.position))
        .collect();
    let mut taken = 0;
    for k in 0..24 {
        // Bob moves a structure of the line (inside the working set or
        // not), one way then back.
        let (id, p) = &ids[(k * 3) % ids.len()];
        let dx = if k % 2 == 0 { 0.7 } else { 0.0 };
        let a = whole
            .apply(
                "tester",
                Command::MoveStructure {
                    id: id.clone(),
                    position: [p[0] + dx, p[1], p[2]],
                },
            )
            .unwrap();
        let changed = a.objects.iter().cloned().collect();
        let new = whole.extent_of(&picks);
        let rows = whole.rows_for(&extent, &users, &new, &changed);
        users = whole.extent_users(&new);
        extent = new;
        if rows.is_empty() {
            continue;
        }
        let msg = RowsMsg {
            model: "A".into(),
            at: Position {
                seq: k as i64 + 2,
                step: 0,
            },
            user: Some("bob".into()),
            what: Some(format!("move {id}")),
            rows,
            spec: None,
        };
        m.take_rows_now("tester", &msg).unwrap();
        taken += 1;
    }
    assert!(taken > 4, "{taken} rows taken");
    // The replica holds what a replica written again holds.
    let again = dir.join("again.ohl");
    whole
        .write_working_set(&extent, &again, "tester", &[])
        .unwrap();
    let fresh = Session::open(&again).unwrap();
    assert!(*fresh.line() == *m.session().unwrap().line());
    assert!(
        m.session()
            .unwrap()
            .file_meta("network.position")
            .unwrap()
            .is_some(),
        "the position held"
    );
}

/// The Join a network window as a user sees it after Show (`OHL_SHOTS`:
/// the user guide's picture).
#[test]
fn join_a_network_window_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("network-shot");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.state_mut().join_network_window =
        Some(crate::network_window::JoinNetworkWindow::sample());
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/join-a-network.png"))
        .unwrap();
}

/// The Sign in to a network window against a local network's server
/// (a throwaway PostgreSQL cluster, invented users) with OIDC sign-in from
/// a fake identity provider on the loopback: the server asked how it takes
/// sign-ins, the invitation's end said, the organisation's sign-in done in
/// the "browser" (the provider's page played by the test), joined with it
/// to the Join a network window; Escape closes the window. With
/// `OHL_SHOTS` (made as `USER=tester`): the user guide's picture.
#[test]
fn signing_in_to_a_local_network() {
    use ohl_server::fake_provider::FakeProvider;
    use ohl_server::network::NetOptions;
    use ohl_server::oidc::{Oidc, OidcConfig};
    use ohl_server::server::{Options, Server};
    use ohl_store_pg::Network;
    use ohl_store_pg::rights::RightsCommand as R;
    let Some(cluster) = ohl_store_pg::testing::Cluster::start("app-sign-in") else {
        return;
    };
    cluster.create_db("net");
    let url = cluster.url("net");
    let idp = FakeProvider::start("00u-tester");
    {
        let net = Network::connect_writer(&url).unwrap();
        for c in [
            R::Bootstrap {
                user: "admin".into(),
            },
            R::AddUser {
                name: "tester".into(),
                display: None,
            },
            R::MapIdentity {
                issuer: idp.issuer.clone(),
                subject: "00u-tester".into(),
                user: "tester".into(),
            },
        ] {
            net.rights_command("admin", &c).unwrap();
        }
    }
    cluster.wait_for_locks_gone("net");
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            code: Some("abcd2345".into()),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: url.clone(),
            idle_solve: false,
            invite_days: Some(90),
            oidc: Some(std::sync::Arc::new(
                Oidc::new(OidcConfig::new(&idp.issuer, "ohl-design")).unwrap(),
            )),
            ..NetOptions::default()
        },
    )
    .unwrap();
    let invitation = server.invitation_for("tester").to_string();
    let model = synthetic_project("sign-in-shot");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|_| App::with_model(model, None));
    let mut w = crate::network_window::JoinNetworkWindow::new("tester".into());
    w.sign_in.invitation = invitation.clone();
    harness.state_mut().join_network_window = Some(w);
    let wait = |h: &mut Harness<'_, App>, what: &str, done: &dyn Fn(&App) -> bool| {
        let t0 = std::time::Instant::now();
        while !done(h.state()) {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "{what}");
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        h.run_steps(2);
    };
    let sign_in = |a: &App| -> bool {
        a.join_network_window
            .as_ref()
            .is_some_and(|w| w.sign_in.options_known())
    };
    wait(&mut harness, "the server asked", &sign_in);
    assert!(harness.query_by_label_contains("valid until").is_some());
    assert!(
        harness
            .query_by_label_contains("also takes your organisation's sign-in")
            .is_some()
    );
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        assert_eq!(crate::files::user(), "tester", "made with USER=tester");
        let image = harness.render().unwrap();
        let ppp = image.width() as f32 / 1000.0;
        let image = match super::tool_shots::window_rect(&harness, "Sign in to a network") {
            Some(rect) => super::tool_shots::crop(&image, rect, ppp),
            None => image,
        };
        image
            .save(format!("{dir}/sign-in-to-a-network.png"))
            .unwrap();
    }
    // Signed in in the "browser".
    harness
        .get_by_label("Sign in with your organisation ↗")
        .click();
    harness.run_steps(2);
    let t0 = std::time::Instant::now();
    let page = loop {
        let opened = crate::help::OPENED.lock().unwrap().clone();
        if let Some(p) = opened
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .find(|p| p.starts_with(&idp.issuer))
        {
            break p;
        }
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(30),
            "no sign-in page opened"
        );
        harness.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    assert!(
        FakeProvider::browse(&page)
            .unwrap()
            .contains("close this tab")
    );
    let signed = |a: &App| -> bool {
        a.join_network_window
            .as_ref()
            .is_some_and(|w| w.sign_in.signed().is_some())
    };
    wait(&mut harness, "signed in", &signed);
    assert!(
        harness
            .query_by_label_contains("Signed in as 00u-tester")
            .is_some()
    );
    // Joined with the sign-in: the Join a network window.
    harness.get_by_label("Join with your sign-in").click();
    wait(&mut harness, "joined", &|a: &App| {
        a.join_network_window
            .as_ref()
            .is_some_and(|w| w.joined_network())
    });
    assert!(harness.query_by_label_contains("Join a network").is_some());
    // Escape closes the sign-in window, nothing changes.
    let mut w = crate::network_window::JoinNetworkWindow::new("tester".into());
    w.sign_in.invitation = invitation;
    harness.state_mut().join_network_window = Some(w);
    harness.run_steps(2);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    assert!(harness.state().join_network_window.is_none());
    server.stop();
}

/// Versions › Offline copy's Offline working set window (2026-11-21)
/// against a local network's server (a throwaway PostgreSQL cluster, a
/// small synthetic country, the invented user `tester`): 1 the working set
/// exported through this connection, the file written, an existing file
/// never replaced; a stale export said in red with what changed meanwhile;
/// 2 the offline file checked with the network's code; 3 its bundle made
/// after a commit; 4 brought back onto a branch, never main; Escape
/// closes. With `OHL_SHOTS` (made as `USER=tester`): the user guide's
/// pictures.
#[test]
fn offline_working_set_window_on_a_local_network() {
    use crate::network_offline::{Exported, OfflineWindow, Step};
    use ohl_server::client::{NetJoin, join_network};
    use ohl_server::network::NetOptions;
    use ohl_server::server::{Options, Server};
    use ohl_store_pg::Network;
    use ohl_store_pg::import::{ImportOptions, import};
    use ohl_store_pg::rights::RightsCommand as R;
    let Some(cluster) = ohl_store_pg::testing::Cluster::start("app-offline") else {
        return;
    };
    cluster.create_db("net");
    let url = cluster.url("net");
    let dir = cluster.scratch();
    let country = ohl_synth::country(&ohl_synth::Settings {
        pylons: 300,
        ..Default::default()
    })
    .unwrap();
    let files =
        ohl_synth::project::write_line_projects(&country, &dir.join("lines"), "tester").unwrap();
    let code = {
        let net = Network::connect_writer(&url).unwrap();
        let mut codes = Vec::new();
        for (code, path, _) in files {
            let mut o = ImportOptions::new(&code, "tester");
            o.revision = false;
            import(&net, &path, &o).unwrap();
            codes.push(code);
        }
        net.rights_command(
            "tester",
            &R::Bootstrap {
                user: "tester".into(),
            },
        )
        .unwrap();
        codes[0].clone()
    };
    cluster.wait_for_locks_gone("net");
    let server = Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            code: Some("abcd2345".into()),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: url.clone(),
            idle_solve: false,
            ..NetOptions::default()
        },
    )
    .unwrap();
    let invitation = server.invitation_for("tester");
    // The working set: the line's first route, followed by OHL-Design.
    let route = {
        let net = Network::connect(&url).unwrap();
        net.route_list()
            .unwrap()
            .into_iter()
            .find(|(l, _)| *l == code)
            .unwrap()
            .1
    };
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![Pick::Route { route }],
    };
    let replica = dir.join("replica.ohl");
    let joined = join_network(
        &invitation,
        "tester",
        &NetJoin {
            working_set: Some(spec.clone()),
            file: Some(replica.clone()),
            ..NetJoin::default()
        },
        &mut |_| {},
    )
    .unwrap();
    let load = |file: PathBuf| {
        model::load(
            Sources {
                file,
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap()
    };
    let mut m = load(replica);
    let mut remote = crate::remote::Remote::new(joined, invitation, false);
    remote.network = Some(Box::new(crate::network_window::NetState::new(spec)));
    m.attach(remote);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run_steps(2);
    fn window(a: &mut App) -> &mut OfflineWindow {
        a.ready()
            .unwrap()
            .network_offline
            .as_mut()
            .expect("the window")
    }
    let wait = |h: &mut Harness<'_, App>, what: &str, done: &dyn Fn(&mut App) -> bool| {
        let t0 = std::time::Instant::now();
        while !done(h.state_mut()) {
            assert!(t0.elapsed() < std::time::Duration::from_secs(60), "{what}");
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        h.run_steps(2);
    };
    // The pictures show short file names (the test's files are in a
    // temporary folder), without the mouse pointer.
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        if let Ok(shots) = ohl_core::env::var("OHL_SHOTS") {
            assert_eq!(crate::files::user(), "tester", "made with USER=tester");
            let w = window(h.state_mut());
            let short = |p: &str| {
                Path::new(p)
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
            };
            let kept = (
                w.export_file.clone(),
                w.bundle_in.clone(),
                w.rules_file.clone(),
                w.exported
                    .iter()
                    .map(|e| e.file.clone())
                    .collect::<Vec<_>>(),
            );
            w.export_file = short(&w.export_file);
            w.bundle_in = short(&w.bundle_in);
            w.rules_file = short(&w.rules_file);
            for e in &mut w.exported {
                e.file = PathBuf::from(short(&e.file.display().to_string()));
            }
            h.input_mut().events.push(egui::Event::PointerGone);
            h.run_steps(2);
            let image = h.render().unwrap();
            let ppp = image.width() as f32 / 1100.0;
            let image = match super::tool_shots::window_rect(h, "Offline working set") {
                Some(rect) => super::tool_shots::crop(&image, rect, ppp),
                None => image,
            };
            image.save(format!("{shots}/{name}.png")).unwrap();
            let w = window(h.state_mut());
            w.export_file = kept.0;
            w.bundle_in = kept.1;
            w.rules_file = kept.2;
            for (e, f) in w.exported.iter_mut().zip(kept.3) {
                e.file = f;
            }
        }
    };
    // 1. Exported through this connection.
    crate::network_offline::open(harness.state_mut().ready().unwrap(), Step::Export);
    let ws = dir.join(format!("{code}-offline.ohl"));
    window(harness.state_mut()).export_file = ws.display().to_string();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("Your working set: line")
            .is_some()
    );
    harness.get_by_label("Export").click();
    wait(&mut harness, "exported", &|a| {
        !window(a).exported.is_empty() || window(a).error.is_some()
    });
    assert_eq!(window(harness.state_mut()).error, None);
    let report = window(harness.state_mut()).exported[0].report.clone();
    assert!(ws.is_file() && report.wanted > 0, "{report:?}");
    assert!(harness.query_by_label_contains("Wrote ").is_some());
    shot(&mut harness, "offline-working-set");
    // The same file again: refused before anything is asked.
    harness.run_steps(2);
    assert!(harness.query_by_label_contains("exists already").is_none());
    // A stale export said in red, with what changed meanwhile.
    let mut stale = report.clone();
    stale.stale = true;
    stale.changed_meanwhile = 3;
    window(harness.state_mut()).exported.push(Exported {
        file: dir.join(format!("{code}-offline-2.ohl")),
        report: stale,
    });
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("STALE: 3 object(s)")
            .is_some()
    );
    shot(&mut harness, "offline-working-set-stale");
    window(harness.state_mut()).exported.pop();
    // Escape closes the window.
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .network_offline
            .is_none()
    );

    // 2 and 3, offline: the file worked on and committed, checked, its
    // bundle made.
    {
        let mut s = Session::open(&ws).unwrap();
        let id = s
            .part()
            .unwrap()
            .iter()
            .find(|(k, _)| *k == ohl_session::Kind::Structure)
            .unwrap()
            .1
            .clone();
        let p = s.line().structure(&id).unwrap().position;
        s.apply(
            "tester",
            Command::MoveStructure {
                id,
                position: [p[0] + 0.5, p[1], p[2]],
            },
        )
        .unwrap();
        s.commit("tester", "offline work").unwrap();
    }
    let mut offline = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_eframe(|_| App::with_model(load(ws.clone()), None));
    offline.run_steps(2);
    crate::network_offline::open(offline.state_mut().ready().unwrap(), Step::Check);
    let rules = dir.join("rules.ohlr");
    std::fs::write(
        &rules,
        "[[rule]]\nname = \"other lines\"\ntarget = \"other_lines\"\n\
         vertical = { by_voltage = [[150, 5.0], [420, 7.0]] }\n\
         radial = { by_voltage = [[150, 4.0], [420, 6.5]] }\n\
         [[rule]]\nname = \"phase to phase\"\ntarget = \"same_line\"\nradial = \"3 m\"\n",
    )
    .unwrap();
    {
        let w = window(offline.state_mut());
        w.kind = crate::network_offline::CheckKind::Clearances;
        w.case = "10 °C".into();
        w.rules_file = rules.display().to_string();
    }
    offline.run_steps(2);
    assert!(
        offline
            .query_by_label_contains("Offline working set of line")
            .is_some()
    );
    offline.get_by_label("Check").click();
    wait(&mut offline, "checked", &|a| window(a).checked.is_some());
    let checked = window(offline.state_mut()).checked.clone().unwrap();
    assert!(checked.is_ok(), "{checked:?}");
    shot(&mut offline, "offline-working-set-check");
    let bundle = dir.join("work.bundle");
    crate::network_offline::open(offline.state_mut().ready().unwrap(), Step::Bundle);
    window(offline.state_mut()).bundle_out = bundle.display().to_string();
    offline.run_steps(2);
    offline.get_by_label("Make the bundle").click();
    offline.run_steps(2);
    let made = window(offline.state_mut()).bundled.clone().unwrap();
    assert!(made.is_ok() && bundle.is_file(), "{made:?}");
    // Not on a network: steps 1 and 4 greyed, said why.
    window(offline.state_mut()).step = Step::BringBack;
    offline.run_steps(2);
    assert!(
        offline
            .query_by_label_contains("Join the network first")
            .is_some()
    );

    // 4. Brought back onto a branch, never main.
    crate::network_offline::open(harness.state_mut().ready().unwrap(), Step::BringBack);
    window(harness.state_mut()).bundle_in = bundle.display().to_string();
    harness.run_steps(2);
    harness.get_by_label("Bring back").click();
    wait(&mut harness, "brought back", &|a| {
        window(a).brought.is_some()
    });
    let back = window(harness.state_mut())
        .brought
        .clone()
        .unwrap()
        .unwrap();
    assert!(back.branch.starts_with("offline-tester-"), "{back:?}");
    assert!(back.revisions == 1 && back.puts >= 1, "{back:?}");
    assert!(
        harness
            .query_by_label_contains("Brought back onto branch")
            .is_some()
    );
    shot(&mut harness, "offline-working-set-back");
    // The same bundle twice: refused, said.
    harness.get_by_label("Bring back").click();
    wait(&mut harness, "refused", &|a| {
        window(a).brought.as_ref().is_some_and(|b| b.is_err())
    });
    drop(harness);
    server.stop();
}
