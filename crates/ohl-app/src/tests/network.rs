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
/// closes. With it (2026-11-23), the ordinary check windows on the
/// offline file read its export's neighbours: the Clearances window's
/// rules to other lines find what the network's own check finds there,
/// said with the export's date, the lines read and that they may be
/// stale; Structure loads name the tower shared with another line, the
/// fields the lines near (NOT ALL CHECKED). With `OHL_SHOTS` (made as
/// `USER=tester`): the user guide's pictures.
/// The test's synthetic country without pictures: the fewest pylons with
/// a tower shared by two lines.
const PYLONS: usize = 150;

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
    // The pictures on a country of several lines (other lines near, a
    // whole route's clearances: seconds); the test alone on a smaller one
    // with a tower shared by two lines (the check windows with the
    // export's neighbours) and the loads on shared towers.
    let pictures = ohl_core::env::var("OHL_SHOTS").is_ok();
    let country = ohl_synth::country(&ohl_synth::Settings {
        pylons: if pictures { 300 } else { PYLONS },
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
        // A line sharing a tower with another (its neighbours exported).
        codes
            .iter()
            .find(|c| {
                !ohl_store_pg::import::twins(&net, c, 0.5)
                    .unwrap()
                    .is_empty()
            })
            .expect("a shared tower in the synthetic country")
            .clone()
    };
    let twin = {
        let net = Network::connect(&url).unwrap();
        ohl_store_pg::import::twins(&net, &code, 0.5).unwrap()[0].clone()
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
    // The working set: the route of the shared tower, followed by
    // OHL-Design.
    let route = {
        let net = Network::connect(&url).unwrap();
        let mut s = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        s.object_routes(&[(ohl_session::Kind::Structure, twin.structure.clone())])
            .pop()
            .and_then(|r| r.into_iter().next())
            .expect("the shared tower's route")
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
            // Generous: the full check runs many tests at once.
            assert!(t0.elapsed() < std::time::Duration::from_secs(300), "{what}");
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
        .build_eframe(|_| {
            // The shipped wire library, as an installed OHL-Design finds it
            // beside itself (the synthetic lines hold no wire file; the
            // tests' build folder may lie outside the source tree).
            let mut m = load(ws.clone());
            m.library = ohl_cables::Library::load(&root().join("libraries/cables")).unwrap();
            App::with_model(m, None)
        });
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
        if pictures {
            w.kind = crate::network_offline::CheckKind::Clearances;
            w.case = "10 °C".into();
            w.rules_file = rules.display().to_string();
        } else {
            w.kind = crate::network_offline::CheckKind::Loads;
        }
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
    window(offline.state_mut()).step = Step::Check;
    offline.run_steps(2);
    offline_files_neighbours_in_the_check_windows(
        &mut offline,
        &ws,
        &rules,
        report.number,
        &twin,
        &|h, title, name| {
            if let Ok(shots) = ohl_core::env::var("OHL_SHOTS") {
                assert_eq!(crate::files::user(), "tester", "made with USER=tester");
                h.input_mut().events.push(egui::Event::PointerGone);
                h.run_steps(2);
                let image = h.render().unwrap();
                let ppp = image.width() as f32 / 1100.0;
                let image = match super::tool_shots::window_rect(h, title) {
                    Some(rect) => super::tool_shots::crop(&image, rect, ppp),
                    None => image,
                };
                image.save(format!("{shots}/{name}.png")).unwrap();
            }
        },
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

/// The check windows on offline file `ws` (open in `h`) with its export's
/// neighbours (2026-11-23): `rules` the window's rule file, `number` the
/// export's number as the window wrote it into the file, `twin` a tower
/// of the working set shared with another line; `shot` the pictures.
fn offline_files_neighbours_in_the_check_windows(
    h: &mut Harness<'_, App>,
    ws: &Path,
    rules: &Path,
    number: i64,
    twin: &ohl_store_pg::import::Twin,
    shot: &dyn Fn(&mut Harness<'_, App>, &str, &str),
) {
    use crate::offline_neighbours::{ExportRecord, Neighbours, RECORD_KEY};
    // What the file holds: the other line, and the server's word on the
    // export the window kept in it.
    {
        let r = h.state_mut().ready().unwrap();
        let n = r
            .model
            .offline_neighbours()
            .expect("an offline working set");
        let n = n.as_ref().as_ref().unwrap();
        assert!(
            n.lines.iter().any(|l| l.model == twin.other_line),
            "{}",
            n.read_words()
        );
        assert_eq!(
            n.record,
            Some(ExportRecord {
                number,
                stale: false,
                changed_meanwhile: 0
            })
        );
        assert!(n.stale_words().contains("They may be stale"));
        assert!(
            n.stale_words()
                .contains("Nothing changed on the network while")
        );
    }
    // A stale export said with what changed meanwhile.
    {
        let s = Session::open(ws).unwrap();
        let kept = s.setting(RECORD_KEY).unwrap();
        let stale = ExportRecord {
            number,
            stale: true,
            changed_meanwhile: 3,
        };
        s.set_setting(RECORD_KEY, Some(&serde_json::to_string(&stale).unwrap()))
            .unwrap();
        let n = Neighbours::read(&s).unwrap().unwrap();
        assert!(
            n.stale_words()
                .contains("STALE already when exported: 3 object(s)"),
            "{}",
            n.stale_words()
        );
        s.set_setting(RECORD_KEY, kept.as_deref()).unwrap();
    }
    // The rules to other lines with the neighbours: what the network's own
    // check finds on the same file (the same pairs and distances).
    let text = "[[rule]]\nname = \"other lines\"\ntarget = \"other_lines\"\nradial = 60.0\n";
    let set = ohl_clearance::RuleSet::parse(text).unwrap().rules;
    let wanted: Vec<String> = Session::open(ws)
        .unwrap()
        .part()
        .unwrap()
        .iter()
        .filter(|(k, _)| *k == ohl_session::Kind::Section)
        .map(|(_, id)| id.clone())
        .collect();
    let (with, without) = {
        let r = h.state_mut().ready().unwrap();
        let m = &r.model;
        // The case the network's check takes (the line has no weather
        // file of its own: the shipped default cases).
        let case = ohl_scope::default_cases()
            .unwrap()
            .weather
            .into_iter()
            .find(|c| c.name == "10 °C")
            .unwrap();
        let states = ohl_clearance::zone::wind_states(&[case], |_| true, &Default::default());
        let input = |neighbours| crate::clearance::Input {
            states: states.clone(),
            solved: None,
            sections: wanted.clone(),
            neighbours,
            ..super::clearances::clearance_input(m, set.clone())
        };
        // Without first: the first solve of a process is the slowest (its
        // times then compare).
        let without = crate::clearance::run(&input(None)).unwrap();
        (
            crate::clearance::run(&input(m.offline_neighbours())).unwrap(),
            without,
        )
    };
    assert!(
        without
            .notes
            .iter()
            .any(|n| n.contains("other lines are checked by")),
        "{:?}",
        without.notes
    );
    assert!(without.findings.is_empty() && without.neighbours.is_empty());
    eprintln!(
        "clearances of the working set: {:.2?} with the export's neighbours, {:.2?} without",
        with.took, without.took
    );
    assert!(
        with.neighbours[0].contains("Neighbours from the export of")
            && with
                .neighbours
                .iter()
                .any(|t| t.contains(&format!("line {}", twin.other_line))),
        "{:?}",
        with.neighbours
    );
    assert!(
        with.not_all_checked.is_empty(),
        "{:?}",
        with.not_all_checked
    );
    let server = ohl_server::offline_checks::check_offline(
        ws,
        &ohl_server::ScopedCheck::Clearances {
            case: "10 °C".into(),
            rules: text.into(),
        },
    )
    .unwrap();
    eprintln!("the network's check on the file: {:.2} s", server.seconds);
    // The server's words for a wire as the window's.
    let wire = |s: &str| match s.strip_prefix("jumper:") {
        Some(j) => format!("jumper {j}"),
        None => s.to_owned(),
    };
    let other = |s: &str| {
        let (line, w) = s.strip_prefix("line ").unwrap().split_once(": ").unwrap();
        match w.strip_prefix("jumper:") {
            Some(j) => format!("jumper {line}:{j}"),
            None => format!("{line}:{w}"),
        }
    };
    let mut on_server: Vec<(String, String, f64)> = server
        .findings
        .iter()
        .filter(|f| f.violates && f.what == "other lines")
        .map(|f| (wire(&f.on), other(f.against.as_deref().unwrap()), f.value))
        .collect();
    let mut here: Vec<(String, String, f64)> = with
        .findings
        .iter()
        .map(|f| {
            (
                f.wire.describe(),
                f.other.as_ref().unwrap().describe(),
                f.distance,
            )
        })
        .collect();
    on_server.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    here.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    assert!(!on_server.is_empty(), "{server:?}");
    assert!(
        !here.is_empty(),
        "{:?} {:?} {:?}",
        with.neighbours,
        with.notes,
        with.rules
    );
    assert_eq!(
        on_server.iter().map(|x| (&x.0, &x.1)).collect::<Vec<_>>(),
        here.iter().map(|x| (&x.0, &x.1)).collect::<Vec<_>>()
    );
    for (a, b) in on_server.iter().zip(&here) {
        assert!((a.2 - b.2).abs() < 1e-6, "{a:?} {b:?}");
    }
    // Structure loads: the shared tower named, computed with the other
    // line's sections in the Shared towers view (2026-11-24).
    {
        let r = h.state_mut().ready().unwrap();
        let m = &r.model;
        let ids: Vec<String> = m.line.structures.iter().map(|s| s.id.clone()).collect();
        let notes = crate::loads::offline_notes(m, &m.offline_neighbours().unwrap(), &ids);
        let named = notes[0].contains(&format!(
            "{} (line {} {})",
            twin.structure, twin.other_line, twin.other
        ));
        assert!(
            (named || notes[0].contains(" more —"))
                && notes[0].contains("shared with another line of the export")
                && notes[0].contains("the Shared towers view"),
            "{notes:?}"
        );
        // The fields: the other line within 500 m, its field not added.
        let field = crate::offline_neighbours::field_notes(m, true);
        assert!(
            field[0].starts_with("NOT ALL CHECKED")
                && field[0].contains(&format!("line {}", twin.other_line)),
            "{field:?}"
        );
    }
    // The Clearances window: the export's neighbours said before Run,
    // then the report with them.
    {
        let r = h.state_mut().ready().unwrap();
        r.network_offline = None;
        r.clearances = crate::clearance::Clearances::new(&r.model, Some(rules));
        r.clearances.open = true;
    }
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("Offline working set: neighbours from its export")
            .is_some()
    );
    assert!(
        h.query_by_label_contains("Neighbours from the export of")
            .is_some()
    );
    shot(h, "Clearances", "offline-neighbours-setup");
    let run = |h: &mut Harness<'_, App>| {
        h.get_by_label("Run").click();
        let t0 = std::time::Instant::now();
        while h.state_mut().ready().unwrap().clearances.report.is_none() {
            assert!(
                t0.elapsed() < std::time::Duration::from_secs(300),
                "checked"
            );
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        h.run_steps(3);
    };
    // The pictures' before: the same window as before 2026-11-23 (the
    // neighbours not read; the rules to other lines skipped).
    if ohl_core::env::var("OHL_SHOTS").is_ok() {
        crate::offline_neighbours::IGNORED.with(|c| c.set(true));
        h.run_steps(3);
        run(h);
        shot(h, "Clearances", "offline-neighbours-clearances-before");
        crate::offline_neighbours::IGNORED.with(|c| c.set(false));
        let r = h.state_mut().ready().unwrap();
        r.clearances = crate::clearance::Clearances::new(&r.model, Some(rules));
        r.clearances.open = true;
        h.run_steps(3);
    }
    run(h);
    let rep = h.state_mut().ready().unwrap().clearances.report.clone();
    let rep = rep.unwrap().unwrap();
    assert!(
        rep.neighbours
            .iter()
            .any(|t| t.contains("Within the rules' reach")),
        "{:?}",
        rep.neighbours
    );
    assert!(
        h.query_by_label_contains("Within the rules' reach")
            .is_some()
    );
    shot(h, "Clearances", "offline-neighbours-clearances");

    // The Structure loads window (2026-11-24): the shared tower with the
    // other line's sections on it, under each line's cases, the worst
    // kept — the same loads as the network's check on the same file.
    {
        let r = h.state_mut().ready().unwrap();
        r.clearances.open = false;
        // The shipped default weather and load cases: those the network's
        // check takes for a line without files of its own (the window's
        // own loads in the same cases).
        let mut m = model::load(
            Sources {
                file: ws.to_path_buf(),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                weather: Some(root().join("libraries/weather/default-weather-cases.ohlwc")),
                load_cases: Some(root().join("libraries/loadcases/default-load-cases.ohllc")),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap();
        m.library = ohl_cables::Library::load(&root().join("libraries/cables")).unwrap();
        r.model = m;
        r.loads = crate::loads::LoadsPage::default();
        r.loads.open = true;
    }
    h.run_steps(3);
    let run_loads = |h: &mut Harness<'_, App>| {
        h.get_by_label("Run").click();
        let t0 = std::time::Instant::now();
        loop {
            let l = &h.state_mut().ready().unwrap().loads;
            if l.results.is_some() && !l.running() && !l.shared.running() {
                break;
            }
            assert!(
                t0.elapsed() < std::time::Duration::from_secs(300),
                "computed"
            );
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        h.run_steps(3);
        t0.elapsed()
    };
    let window_with = run_loads(h);
    let (own_model, c, took, cases) = {
        let r = h.state_mut().ready().unwrap();
        let n = r.model.offline_neighbours().unwrap();
        let own = n.as_ref().as_ref().unwrap().meta.model.clone();
        let cases = match &r.loads.results {
            Some(Ok(v)) => v.len(),
            other => panic!("{other:?}"),
        };
        (
            own,
            r.loads.shared.last.clone().expect("the shared towers"),
            r.loads.took,
            cases,
        )
    };
    // The network's check on the same file: every tower of the working
    // set shared with another line.
    let server = ohl_server::offline_checks::check_offline(
        ws,
        &ohl_server::ScopedCheck::StructureLoads {
            structures: Vec::new(),
        },
    )
    .unwrap();
    eprintln!(
        "structure loads: the window {:.2?} (its own loads, {cases} case(s)), the {} shared \
         tower(s) with the other line {:.2?} beside (the network's code {:.2} s), the whole run \
         {window_with:.2?}; the network's check on the file {:.2} s",
        took.unwrap(),
        c.computed().len(),
        c.took,
        c.result.as_ref().unwrap().seconds,
        server.seconds
    );
    assert!(c.not_all_checked().is_empty(), "{:?}", c.not_all_checked());
    assert!(
        c.computed().contains(&twin.structure.as_str()),
        "{:?}",
        c.computed()
    );
    let measures = [
        "factored transverse (N)",
        "factored longitudinal (N)",
        "factored vertical (N)",
    ];
    let on_server: Vec<(String, String, String, f64)> = server
        .findings
        .iter()
        .filter(|f| f.what == "structure loads (worst)")
        .map(|f| {
            (
                f.on.clone(),
                f.against.clone().unwrap(),
                f.measure.clone(),
                f.value,
            )
        })
        .collect();
    let mut here = Vec::new();
    let towers = &c.result.as_ref().unwrap().loads.towers;
    assert!(
        towers
            .iter()
            .any(|t| t.twins.iter().any(|n| n.model == twin.other_line))
    );
    for t in towers {
        assert!(!t.twins.is_empty());
        for w in &t.worst {
            for (k, measure) in measures.iter().enumerate() {
                let (line, case) = &w.governing[k];
                here.push((
                    format!(
                        "structure {} point {}",
                        t.structure,
                        if w.point.is_empty() {
                            "(body)"
                        } else {
                            &w.point
                        }
                    ),
                    if *line == own_model {
                        format!("case {case}")
                    } else {
                        format!("line {line}'s case {case}")
                    },
                    (*measure).to_owned(),
                    w.factored[k],
                ));
            }
        }
    }
    assert!(!on_server.is_empty(), "{server:?}");
    // The same points, governing cases and loads, to the bit.
    assert_eq!(on_server, here);
    // Some point carries the other line's sections.
    assert!(
        c.result.as_ref().unwrap().loads.towers.iter().any(|t| t
            .cases
            .iter()
            .any(|x| x.points.iter().any(|p| p.lines.contains(&twin.other_line)))),
        "{c:?}"
    );
    // Said in the window: the Shared towers view, its verdict.
    h.get_by_label("Shared towers").click();
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("Shared towers with the export's other lines")
            .is_some()
    );
    assert!(
        h.query_by_label_contains("shared tower(s) computed with every line's sections")
            .is_some()
    );
    shot(h, "Structure loads", "offline-neighbours-loads");
    // A shared tower outside the working set: NOT ALL CHECKED, in the
    // view and the Usage view.
    let mut outside = c.clone();
    outside.outside = vec!["X1".into()];
    assert!(
        outside.not_all_checked()[0].starts_with("NOT ALL CHECKED: structure(s) X1"),
        "{:?}",
        outside.not_all_checked()
    );
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .loads
            .shared
            .usage_words()
            .unwrap()
            .starts_with("NOT ALL CHECKED: the usage of the shared tower(s)")
    );

    // The Magnetic field window (2026-11-24): the export's lines within
    // 500 m of the spans computed, each solved in the case with its own
    // settings and wires, their field added by size — by the network's
    // code; the same field as the network's check on the same file.
    let amps = 600.0;
    let server = ohl_server::offline_checks::check_offline(
        ws,
        &ohl_server::ScopedCheck::Fields {
            case: "10 °C".into(),
            distance: None,
            amps: Some(amps),
            limit_ut: None,
        },
    )
    .unwrap();
    let (own, with, words, took_own, took_with) = {
        let r = h.state_mut().ready().unwrap();
        r.loads.open = false;
        let m = &r.model;
        let case = m
            .cases
            .iter()
            .find(|c| c.case.name == "10 °C")
            .expect("the default weather cases")
            .case
            .clone();
        // This line's field as the network's check computes it: profiles
        // across the working set's spans 1 m above the ground, ±50 m every
        // 5 m, every circuit without a current of its own at `amps`.
        let t0 = std::time::Instant::now();
        let solved = crate::model::Solved::as_shown(&m.line, &m.library, case.clone());
        let ws_sections: std::collections::BTreeSet<String> = wanted.iter().cloned().collect();
        let settings = ohl_electrical::magnetic::Settings {
            height: 1.0,
            width: 50.0,
            step: 5.0,
            profiles: true,
            stations: ohl_scope::fields::stations_of(&m.line, &ws_sections),
            currents: ohl_electrical::magnetic::circuits(&m.line)
                .into_iter()
                .filter(|c| m.line.system_current(&c.system).is_none())
                .map(|c| ohl_electrical::magnetic::CircuitCurrent {
                    circuit: c.system.clone(),
                    amps,
                    reverse: false,
                })
                .collect(),
            ..ohl_electrical::magnetic::Settings::default()
        };
        let src = ohl_electrical::Sources {
            line: &m.line,
            library: &m.library,
            shapes: &solved.shapes,
            temperature: case.temperature,
            ground: None,
            jumpers: &solved.jumpers,
        };
        let (own, _) = ohl_electrical::magnetic::run_keeping(&src, &settings, None).unwrap();
        let took_own = t0.elapsed();
        let nb = crate::offline_neighbours::MagneticNeighbours::of(m, Some(amps)).unwrap();
        let mut with = own.clone();
        let t1 = std::time::Instant::now();
        let words = nb.add(&m.line, &case, &mut with);
        (own, with, words, took_own, t1.elapsed())
    };
    eprintln!(
        "magnetic field of the working set's spans: {took_own:.2?} alone, + {took_with:.2?} with \
         the export's lines near (the network's code); the network's check on the file {:.2} s",
        server.seconds
    );
    assert!(
        words[0].starts_with("Neighbours from the export of")
            && words[0].contains("included")
            && words[0].contains(&format!("line {}", twin.other_line)),
        "{words:?}"
    );
    assert!(
        !words.iter().any(|w| w.starts_with("NOT ALL CHECKED")),
        "{words:?}"
    );
    // The other lines added: more field everywhere, never less.
    for (a, b) in own.profiles.iter().zip(&with.profiles) {
        for (p, q) in a.points.iter().zip(&b.points) {
            assert!(q.b_ut >= p.b_ut, "{p:?} {q:?}");
        }
    }
    assert!(
        own.profiles
            .iter()
            .zip(&with.profiles)
            .any(|(a, b)| b.max_ut > a.max_ut + 1e-9)
    );
    // The same sums as the network's (`add_by_size` on the same field,
    // the same sources): to the bit.
    {
        let r = h.state_mut().ready().unwrap();
        let case = r
            .model
            .cases
            .iter()
            .find(|c| c.case.name == "10 °C")
            .unwrap()
            .case
            .clone();
        let f = ohl_server::offline_checks::field_neighbours(
            ws,
            &wanted,
            &case,
            &ohl_scope::fields::FieldSpec {
                amps: Some(amps),
                ..Default::default()
            },
        )
        .unwrap();
        let refs: Vec<&ohl_electrical::magnetic::Wires3d> =
            f.others.lines.iter().map(|l| &l.wires).collect();
        let (sums, _, _) = ohl_scope::fields::add_by_size(&own, &refs, None);
        for (a, b) in sums.iter().zip(&with.profiles) {
            for (p, q) in a.points.iter().zip(&b.points) {
                assert_eq!(p.b_ut, q.b_ut);
            }
        }
    }
    // Per span the network's highest value.
    let on_server: std::collections::BTreeMap<String, f64> = server
        .findings
        .iter()
        .filter(|f| f.what == "magnetic field")
        .map(|f| (f.on.clone(), f.value))
        .collect();
    let here: std::collections::BTreeMap<String, f64> = with
        .profiles
        .iter()
        .map(|p| (format!("span {}–{}", p.from, p.to), p.max_ut))
        .collect();
    // Per span the same highest value as the network's check, to the bit
    // (this line's field there the same too: the file's line holds no
    // more of it near than the network's sections within 500 m).
    assert!(!on_server.is_empty(), "{server:?}");
    assert_eq!(on_server, here);

    // The window itself: this line's circuits and the other lines' at
    // `amps`, the working set's spans ticked; before (the neighbours not
    // read: the window alone) and with them.
    {
        let r = h.state_mut().ready().unwrap();
        for c in ohl_electrical::magnetic::circuits(&r.model.line) {
            r.magnetic_field.set_current(&c.label(), amps);
        }
        r.magnetic_field.others_amps = format!("{amps}");
        r.magnetic_field.tick_sections(&r.model, &wanted);
        let case = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "10 °C")
            .unwrap();
        r.magnetic_field.show(case);
    }
    h.run_steps(3);
    let run_field = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .magnetic_field
            .common
            .job
            .result = None;
        h.get_by_label("Run").click();
        let t0 = std::time::Instant::now();
        loop {
            let f = &h.state_mut().ready().unwrap().magnetic_field;
            if f.common.job.result.is_some() && !f.common.job.running() {
                break;
            }
            assert!(
                t0.elapsed() < std::time::Duration::from_secs(300),
                "computed"
            );
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        h.run_steps(3);
        t0.elapsed()
    };
    crate::offline_neighbours::IGNORED.with(|c| c.set(true));
    h.run_steps(3);
    let alone = run_field(h);
    shot(h, "Magnetic field", "offline-neighbours-magnetic-before");
    crate::offline_neighbours::IGNORED.with(|c| c.set(false));
    h.run_steps(3);
    let together = run_field(h);
    eprintln!(
        "the Magnetic field window: {alone:.2?} alone, {together:.2?} with the export's lines near"
    );
    let notes = match &h
        .state_mut()
        .ready()
        .unwrap()
        .magnetic_field
        .common
        .job
        .result
    {
        Some(Ok(a)) => a.field.notes.clone(),
        other => panic!("{:?}", other.as_ref().map(|r| r.as_ref().err())),
    };
    assert!(
        notes[0].starts_with("Neighbours from the export of") && notes[0].contains("included"),
        "{notes:?}"
    );
    assert!(
        !notes.iter().any(|n| n.starts_with("NOT ALL CHECKED")),
        "{notes:?}"
    );
    shot(h, "Magnetic field", "offline-neighbours-magnetic");
}
