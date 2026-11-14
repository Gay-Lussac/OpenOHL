// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Post insulators (owner, 2026-11-14): the wire point a post holds is
//! fixed at its top; its load goes to the post's base point with its
//! lever, and the post's own weight and wind at its middle.

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Three spans, T2 holding the conductor on an upright post 1.6 m tall
/// (its base A1 at 28.4 m, its top W1 at 30 m).
fn on_a_post() -> Line {
    Line::from_json(
        r#"{
  "format": "ohl-line", "version": 17, "name": "post", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 30], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 28.4]}],
     "strings": [{"label": "P1", "top": "A1", "wire": "W1", "length": 1.6,
                  "angle_deg": 180.0, "weight": 450.0, "wind_area": 0.3,
                  "post": true, "cantilever_load": 8000.0}],
     "wires": [{"label": "W1", "position": [300, 0, 30], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 30], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}]
}"#,
    )
    .unwrap()
}

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n",
    )
    .unwrap()
    .cases
}

/// The conductor's load at T2 acts at the post's top: a load of A1 with a
/// lever of 1.6 m up, its moment about A1 = lever × force; the post's own
/// weight and wind at its middle (0.8 m); the clamp does not move.
#[test]
fn a_post_carries_the_wire_to_its_base_with_its_moment() {
    let line = on_a_post();
    line.validate().unwrap();
    let w = weather();
    let cases = vec![LoadCase::from_weather("wind")];
    let r = structure_loads(&line, &library(), &w, &cases, &default_budget()).unwrap();
    let at_t2: Vec<&AttachmentLoad> = r[0]
        .loads
        .iter()
        .filter(|l| l.structure == "T2" && !l.point.is_empty())
        .collect();
    assert!(at_t2.iter().all(|l| l.point == "A1"), "{at_t2:?}");
    let top = at_t2
        .iter()
        .find(|l| (l.lever[2] - 1.6).abs() < 1e-6)
        .expect("the wire's load at the post's top");
    assert!(top.lever[0].abs() < 1e-6 && top.lever[1].abs() < 1e-6);
    assert!(top.label.contains("post"), "{}", top.label);
    // The wind across the line on the wire: a transverse load.
    assert!(top.local[0].abs() > 500.0, "{top:?}");
    let own = at_t2
        .iter()
        .find(|l| (l.lever[2] - 0.8).abs() < 1e-6)
        .expect("the post's own load at its middle");
    assert!((own.local[2] - 450.0).abs() < 1e-6, "{own:?}");
    assert!((own.local[0].abs() - 500.0 * 0.3).abs() < 1e-6, "{own:?}");
    // The moment about the base: the lever (up) × the transverse force.
    let m = crate::capacity::lever_moment(top);
    assert!((m[1] - 1.6 * top.factored[0]).abs() < 1e-6, "{m:?}");
    // The section's clamp at T2 stays at the post's top.
    let solved = ohl_line::solve::solve_line(&line, &library(), &w[0]);
    let s = solved[0].as_ref().unwrap();
    assert!((s.clamps[1][2] - 30.0).abs() < 1e-9, "{:?}", s.clamps[1]);
}

/// A post holds its wire point alone and is no strain string; its ratings
/// and exit angles are a post's only.
#[test]
fn a_post_is_checked() {
    let mut line = on_a_post();
    line.structures[1].strings[0].strain = true;
    assert!(line.validate().is_err());
    let mut line = on_a_post();
    line.structures[1].strings[0].post = false;
    assert!(line.validate().is_err(), "a cantilever load on a string");
    let mut line = on_a_post();
    line.structures[1].strings[0].exit_angle_deg = Some(120.0);
    assert!(line.validate().is_err());
    // Written as version 17 (a line without a post: older).
    assert!(on_a_post().to_json().contains("\"version\": 17"));
}
