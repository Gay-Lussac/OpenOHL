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
