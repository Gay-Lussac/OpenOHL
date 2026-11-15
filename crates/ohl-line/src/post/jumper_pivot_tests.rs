// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A pivoting post holding only a jumper's end, solved **together with
//! its jumper** (owner, 2026-11-15: "Should the jumper be solved together
//! with the post? — yes"), and its hinge friction (owner, 2026-11-15:
//! "keep it frictionless for now with an option to add it if needed").

use super::*;
use crate::jumper::{MovedEnd, TapContext, solve_jumpers, solve_one_jumper_in};
use ohl_core::units::Temperature;

/// The textbook cable (EA 30 MN, 1.6 kg/m at 20 °C).
fn library() -> ohl_cables::Library {
    let mut l = ohl_cables::Library::default();
    l.add_toml(
        std::path::Path::new("tb"),
        "format = \"ohl-wire\"\nversion = 1\n\n[[cable]]\nid = \"tb\"\nname = \"Textbook cable\"\n\
         kind = \"aaac\"\nsource = \"Invented textbook data\"\ndiameter = \"28 mm\"\n\
         mass_per_length = \"1.6 kg/m\"\nrated_tensile_strength = \"120 kN\"\n\
         outer = { area = \"400 mm²\", modulus = \"75 GPa\", thermal_expansion = 20e-6 }\n",
    )
    .unwrap();
    l
}

/// The benchmark tbins-007: a hinged post P1 (450 N) standing level 1.5 m
/// out (−y) from A1 (0, 0, 10), braced by B1 (200 N) from A2 straight
/// above: it turns about the vertical through A1 and A2. A jumper J, 4.0 m
/// of unstressed wire at 10 °C, no bending stiffness, from its top W1 to a
/// fixed point W2 (3, −2.5, 9). `extra`: more of P1's fields.
fn held(extra: &str, rigid: bool) -> Line {
    // A rigid jumper has no length of its own.
    let kind = if rigid {
        r#""rigid": true"#
    } else {
        r#""length": 4.0, "temperature": 10.0, "bending_stiffness": 0.0"#
    };
    let line = Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 17, "name": "pivot", "crs": "local",
 "structures": [
  {{"id": "T", "position": [0, 0, 0], "orientation_deg": -90,
   "points": [{{"label": "A1", "position": [0, 0, 10]}}, {{"label": "A2", "position": [0, 0, 11.5]}}],
   "strings": [{{"label": "P1", "top": "A1", "wire": "W1", "length": 1.5, "angle_deg": 90.0,
                 "direction_deg": 90.0, "weight": 450.0, "post": true, "hinged": true,
                 "stiffness": 1e11{extra}}},
               {{"label": "B1", "top": "A2", "wire": "W1", "weight": 200.0, "stiffness": 1e11}}],
   "wires": [{{"label": "W1", "position": [0, -1.5, 10], "set": 1}},
             {{"label": "W2", "position": [3, -2.5, 9], "set": 2}}]}}
 ],
 "jumpers": [{{"id": "J", "cable": "tb", "from": {{"structure": "T", "set": 1}},
               "to": {{"structure": "T", "set": 2}}, {kind}}}],
 "sections": []
}}"#
    ))
    .unwrap();
    line.validate().unwrap();
    line
}

fn still() -> WeatherCase {
    WeatherCase::still("still", Temperature::from_celsius(10.0))
}

/// The jumper solved with its end on the post's clamp turned by `turn`
/// (radians): its pull on the clamp.
fn pull_at(line: &Line, turn: f64) -> Point {
    let (pivots, _) = jumper_pivots(line);
    let p = &pivots[0];
    let context = TapContext {
        moved: vec![MovedEnd {
            point: p.point.clone(),
            at: p.clamp_at(turn),
            axis: p.axis(),
            turn,
        }],
        ..Default::default()
    };
    solve_one_jumper_in(line, &library(), &still(), &[], &line.jumpers[0], &context)
        .unwrap()
        .end_forces[0]
}

/// The moment about the vertical through A1 of `f` on the top turned by
/// `turn`.
fn moment(turn: f64, f: Point) -> f64 {
    let r = [1.5 * turn.sin(), -1.5 * turn.cos()];
    r[0] * f[1] - r[1] * f[0]
}

/// Hand check: in still air the jumper hangs in the vertical plane through
/// its ends, so its pull on the top points in plan straight at W2; the
/// post settles where that pull has no part along its circle — the top,
/// the axis and W2 in one vertical plane: turned atan(3 / 2.5) = 50.194°.
/// It is a stable balance (turned a degree either way, the pull turns it
/// back). The decoupled answer before (the jumper solved to the top as
/// given and its pull held fixed while the post turns, [`hinged_alone`])
/// is 71.57°: 21° too far.
#[test]
fn a_jumper_held_post_settles_with_its_jumper() {
    let line = held("", false);
    let shapes = solve_jumpers(&line, &library(), &still(), &[]);
    let shape = shapes[0].as_ref().unwrap();
    let p = &shape.pivots[0];
    let expected = 3f64.atan2(2.5).to_degrees();
    assert!((p.turn_deg - expected).abs() < 1e-6, "{p:?} {expected}");
    assert!(!p.stuck && p.hinge_moment == 0.0, "{p:?}");
    assert_eq!((p.post.as_str(), p.wire.as_str(), p.set), ("P1", "W1", 1));
    // Its end on the turned clamp.
    let t = p.turn_deg.to_radians();
    let top = [1.5 * t.sin(), -1.5 * t.cos(), 10.0];
    assert!(
        (0..3)
            .all(|k| (p.clamp[k] - top[k]).abs() < 1e-9
                && (shape.points[0][k] - top[k]).abs() < 1e-9),
        "{p:?}"
    );
    // Balanced and stable.
    let f = shape.end_forces[0];
    assert!(moment(t, f).abs() < 1e-6, "{}", moment(t, f));
    let d = 1f64.to_radians();
    assert!(moment(t + d, pull_at(&line, t + d)) < -1e-3);
    assert!(moment(t - d, pull_at(&line, t - d)) > 1e-3);
    // The decoupled answer: the pull as given, held fixed — the post
    // points along its part across the axis.
    let given = pull_at(&line, 0.0);
    let turned = given[0].atan2(-given[1]).to_degrees();
    assert!((turned - 71.565).abs() < 0.01, "{turned}");
}

/// The members' forces of the settled post: the statics at its top
/// ([`hinged_post_loads`]): their forces on A1 and A2 add up to the
/// jumper's pull and the members' weights; nothing across the post at its
/// base without friction.
#[test]
fn the_settled_post_passes_the_pull_to_its_bases() {
    let line = held("", false);
    let shape = solve_jumpers(&line, &library(), &still(), &[])
        .remove(0)
        .unwrap();
    let p = &shape.pivots[0];
    let f = shape.end_forces[0];
    let (members, (base, _, force, couple)) = hinged_post_loads(
        &line.structures[0],
        "W1",
        f,
        shape.end_moments[0],
        Some(p),
        [0.0; 3],
        1e11,
    )
    .unwrap();
    assert_eq!(base, "A1");
    assert!(
        norm(force) < 1e-9 && norm(couple) < 1e-9,
        "{force:?} {couple:?}"
    );
    let sum = [0, 1, 2].map(|k| members[0].on_top[k] + members[1].on_top[k]);
    let expected = [f[0], f[1], f[2] - 650.0];
    assert!(
        (0..3).all(|k| (sum[k] - expected[k]).abs() < 1e-4),
        "{sum:?} {expected:?}"
    );
    // The post pushes, the brace pulls (the reference's 354.04 N and
    // 515.77 N, benchmark tbins-007).
    assert!((members[0].force + 354.0407).abs() < 1e-3, "{members:?}");
    assert!((members[1].force - 515.7685).abs() < 1e-3, "{members:?}");
}

/// With a hinge friction (owner, 2026-11-15): above the moment the jumper
/// puts on the post as given, the hinge holds it there (stick: its moment
/// the jumper's, reversed; the structure takes it at the post's base);
/// below, the post turns until the jumper's moment has fallen to the
/// friction (slip), less far than without friction.
#[test]
fn a_hinge_friction_sticks_then_slips() {
    let free = held("", false);
    let m0 = moment(0.0, pull_at(&free, 0.0));
    assert!(m0 > 1.0, "{m0}");
    // Stick.
    let line = held(&format!(", \"hinge_friction_nm\": {}", 2.0 * m0), false);
    let shape = solve_jumpers(&line, &library(), &still(), &[])
        .remove(0)
        .unwrap();
    let p = &shape.pivots[0];
    assert!(p.stuck && p.turn_deg == 0.0, "{p:?}");
    assert!((p.hinge_moment + m0).abs() < 1e-9 * m0, "{p:?} {m0}");
    let (_, (_, _, force, couple)) = hinged_post_loads(
        &line.structures[0],
        "W1",
        shape.end_forces[0],
        shape.end_moments[0],
        Some(p),
        [0.0; 3],
        1e11,
    )
    .unwrap();
    // On the structure: the hinge's moment reversed about the axis (z),
    // and at its base the jumper's pull across the post (+x), which the
    // hinge holds, carried down the post.
    assert!((couple[2] - m0).abs() < 1e-9 * m0, "{couple:?}");
    assert!((force[0] - m0 / 1.5).abs() < 1e-9 * m0, "{force:?}");
    // Slip.
    let friction = 0.5 * m0;
    let line = held(&format!(", \"hinge_friction_nm\": {friction}"), false);
    let shape = solve_jumpers(&line, &library(), &still(), &[])
        .remove(0)
        .unwrap();
    let p = &shape.pivots[0];
    let t = p.turn_deg.to_radians();
    assert!(!p.stuck && p.turn_deg > 0.0 && p.turn_deg < 50.0, "{p:?}");
    assert!((p.hinge_moment + friction).abs() < 1e-12, "{p:?}");
    let left = moment(t, shape.end_forces[0]);
    assert!((left - friction).abs() < 1e-6 * m0, "{left} {friction}");
}

/// A rigid jumper (a tube) holds a pivoting post as a brace would; its
/// force along it is not modelled: said, not solved.
#[test]
fn a_rigid_jumper_on_a_pivoting_post_is_said() {
    let line = held("", true);
    let e = solve_jumpers(&line, &library(), &still(), &[])
        .remove(0)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("rigid") && e.contains("loads not computed"),
        "{e}"
    );
}

/// A hinge friction is a hinged post's, ≥ 0: refused on its brace and
/// below 0; written at line file version 19 only when given.
#[test]
fn a_hinge_friction_is_a_hinged_posts() {
    let mut line = held(", \"hinge_friction_nm\": 100.0", false);
    assert!(line.to_json().contains("\"version\": 19"));
    assert!(!held("", false).to_json().contains("\"version\": 19"));
    line.structures[0].strings[0].hinge_friction_nm = Some(-1.0);
    assert!(line.validate().is_err());
    line.structures[0].strings[0].hinge_friction_nm = None;
    line.structures[0].strings[1].hinge_friction_nm = Some(10.0);
    let e = line.validate().unwrap_err().to_string();
    assert!(e.contains("hinge friction"), "{e}");
}
