// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A **pivoting post** (owner, 2026-11-15: "Pivoting should simply be tied
//! to the jumper or piece it's attached to … If braced then some degrees of
//! freedom are locked"): a hinged post with one brace whose base is
//! straight above the post's turns freely about the vertical through both
//! bases; the conductor clamped at its top holds it — it swings along the
//! line until the spans' pulls balance, like a suspension string.

use super::*;
use ohl_core::units::Temperature;

/// An all-aluminium test cable (15.97 N/m, EA 34.66 MN), its weight given
/// at 20 °C.
fn cable() -> Cable {
    toml::from_str(
        "id = \"hand\"\nname = \"hand\"\nkind = \"aaac\"\nsource = \"invented test data\"\n\
         diameter = \"28.1 mm\"\nmass_per_length = \"1.6285 kg/m\"\n\
         rated_tensile_strength = \"140 kN\"\n\
         outer = { area = \"400 mm²\", modulus = \"86.64 GPa\", thermal_expansion = 18.8514e-6 }\n",
    )
    .unwrap()
}

/// Two level 300 m spans along x. T2 holds the wire on a hinged post P1,
/// 1.5 m long, standing level to the right of the line (−y) on A1 at
/// (300, 0, 28.4), braced by B1 from A2 1.5 m straight above A1: the pair
/// turns freely about the vertical through A1 and A2. Both 10⁹ N stiff
/// (the hand check takes their stretch). The ends' wire points are in line
/// with the post's top. `brace` is A2's position; `stringing` the
/// section's.
fn pivoting(brace: [f64; 3], stringing: &str) -> Line {
    let brace_length = {
        let d = [300.0 - brace[0], -1.5 - brace[1], 28.4 - brace[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let line = Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 17, "name": "pivot", "crs": "local",
 "structures": [
  {{"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
   "wires": [{{"label": "W1", "position": [0, -1.5, 28.4], "set": 1}}]}},
  {{"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
   "points": [{{"label": "A1", "position": [300, 0, 28.4]}},
              {{"label": "A2", "position": [{}, {}, {}]}}],
   "strings": [{{"label": "P1", "top": "A1", "wire": "W1", "length": 1.5,
                 "angle_deg": 90.0, "direction_deg": 90.0, "weight": 450.0,
                 "post": true, "hinged": true, "stiffness": 1e9}},
               {{"label": "B1", "top": "A2", "wire": "W1", "length": {brace_length},
                 "weight": 200.0, "stiffness": 1e9}}],
   "wires": [{{"label": "W1", "position": [300, -1.5, 28.4], "set": 1}}]}},
  {{"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
   "wires": [{{"label": "W1", "position": [600, -1.5, 28.4], "set": 1}}]}}
 ],
 "sections": [{{"id": "S", "cable": "hand", "bundle": 1,
  "points": [{{"structure": "T1", "set": 1}}, {{"structure": "T2", "set": 1}},
             {{"structure": "T3", "set": 1}}],
  "stringing": {stringing}}}]
}}"#,
        brace[0], brace[1], brace[2]
    ))
    .unwrap();
    line.validate().unwrap();
    assert_eq!(
        line.structures[1].wire("W1").unwrap().position,
        [300.0, -1.5, 28.4]
    );
    line
}

const ABOVE: [f64; 3] = [300.0, 0.0, 29.9];

fn still() -> WeatherCase {
    WeatherCase::still("still", Temperature::from_celsius(20.0))
}

fn force(s: &SectionShape, label: &str) -> f64 {
    s.strings[1]
        .iter()
        .find(|x| x.label == label)
        .unwrap_or_else(|| panic!("{label}: {:?}", s.strings[1]))
        .force
}

/// The braced post's shape: a pivot with one brace straight above (or
/// anywhere: two bases always lie on one line); held in every direction
/// with two braces ahead and back above the post's base.
#[test]
fn one_brace_pivots_two_do_not() {
    let stringing = r#"{"kind": "tension", "horizontal_tension": 20000, "temperature": 20}"#;
    let line = pivoting(ABOVE, stringing);
    let shapes = |line: &Line| {
        holders_of(line, &line.sections[0])
            .unwrap()
            .iter()
            .map(|h| h.shape)
            .collect::<Vec<_>>()
    };
    assert_eq!(shapes(&line)[1], Shape::Pivot);
    // A brace offset along the line: still one line through the two bases.
    let line = pivoting([301.0, 0.0, 29.9], stringing);
    assert_eq!(shapes(&line)[1], Shape::Pivot);
    // Two braces, 1 m ahead and back: they hold it.
    let mut line = pivoting([301.0, 0.0, 29.9], stringing);
    let t2 = &mut line.structures[1];
    let mut a3 = t2.points[1].clone();
    a3.label = "A3".into();
    a3.position = [299.0, 0.0, 29.9];
    t2.points.push(a3);
    let mut b2 = t2.strings[1].clone();
    b2.label = "B2".into();
    b2.top = "A3".into();
    t2.strings.push(b2);
    line.validate().unwrap();
    assert_eq!(shapes(&line)[1], Shape::General);
}

/// Clipped in with more wire in the first span (300.70 m) than in the
/// second (300.40 m), in still air: the second pulls harder, and the post
/// swings towards it — about the vertical through its hinge, its top on the
/// circle of its length — until the pulls balance. Hand check: the angle φ
/// at which the two spans' pulls on the top (elastic catenaries, each of
/// its unstressed length between its end and the top at
/// (300 + 1.5 sin φ, −1.5 cos φ, 28.4)) have no part along the circle,
/// found by bisection; then the brace's force from the vertical balance,
/// the post's from the horizontal one in the plane of the two (rigid
/// members, half their weights on the top). The solve's top, forces and
/// loads on A1 and A2 agree.
#[test]
fn a_pivoting_post_swings_until_the_pulls_balance() {
    let line = pivoting(
        ABOVE,
        r#"{"kind": "unstressed_lengths", "lengths": [300.70, 300.40], "temperature": 20}"#,
    );
    let c = cable();
    let s = solve_section(&line, &line.sections[0], &c, &still(), &default_fe_budget()).unwrap();
    // The hand check.
    let strung = strung_cable(&line.sections[0], &c).unwrap();
    let (w, ea) = (strung.weight().si(), strung.axial_stiffness().si());
    let budget = ohl_wirefe::catenary_element::default_element_budget();
    // The pull of a span from `from` (unstressed length `l0`) on the top.
    let pull = |from: Vector3<f64>, top: Vector3<f64>, l0: f64| {
        CableProperties {
            unstressed_length: l0,
            axial_stiffness: ea,
            load: Vector3::new(0.0, 0.0, -w),
        }
        .solve(top - from, None, &budget)
        .unwrap()
        .force_j
    };
    let (t1, t3) = (
        Vector3::new(0.0, -1.5, 28.4),
        Vector3::new(600.0, -1.5, 28.4),
    );
    let base = Vector3::new(300.0, 0.0, 28.4);
    // The members' lengths under their forces (N = EA (l − l0) / l0) put
    // the top in the vertical plane at φ: ρ from the axis, z.
    let (lp0, lb0) = (1.5, (1.5f64 * 1.5 * 2.0).sqrt());
    let (mut post, mut brace, mut phi) = (0.0, 0.0, 0.0);
    let mut at = base;
    for _ in 0..6 {
        let (lp, lb) = (lp0 * (1.0 + post / 1e9), lb0 * (1.0 + brace / 1e9));
        let z = (lp * lp - lb * lb) / 3.0 + 29.15;
        let rho = (lp * lp - (z - 28.4) * (z - 28.4)).sqrt();
        let top = |phi: f64| Vector3::new(300.0 + rho * phi.sin(), -rho * phi.cos(), z);
        let wires = |phi: f64| pull(t1, top(phi), 300.70) + pull(t3, top(phi), 300.40);
        let along = |phi: f64| wires(phi).dot(&Vector3::new(phi.cos(), phi.sin(), 0.0));
        let (mut lo, mut hi) = (0.0, 1.0);
        assert!(along(lo) > 0.0 && along(hi) < 0.0);
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if along(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        phi = 0.5 * (lo + hi);
        at = top(phi);
        // On the top: the wires, half of each member's weight; balanced
        // by F + N_p û_p + N_b û_b = 0 (û towards the member's base,
        // tension positive) in the vertical plane of the two.
        let on_top = wires(phi) - Vector3::new(0.0, 0.0, (450.0 + 200.0) / 2.0);
        let a2 = Vector3::new(ABOVE[0], ABOVE[1], ABOVE[2]);
        let (up, ub) = ((base - at) / lp, (a2 - at) / lb);
        let radial = Vector3::new(phi.sin(), -phi.cos(), 0.0);
        let (f_r, f_z) = (on_top.dot(&radial), on_top.z);
        // f_r + N_p up_r + N_b ub_r = 0, f_z + N_p up_z + N_b ub_z = 0.
        let (a, b, c, d) = (up.dot(&radial), ub.dot(&radial), up.z, ub.z);
        let det = a * d - b * c;
        post = (-f_r * d + b * f_z) / det;
        brace = (-a * f_z + c * f_r) / det;
    }
    eprintln!(
        "swing {:.4}°, top {at:?}, post {post:.3} N, brace {brace:.3} N",
        phi.to_degrees()
    );
    // About 5.5°: some 14 cm along the line.
    assert!(phi.to_degrees() > 3.0 && phi.to_degrees() < 10.0, "{phi}");
    let got = v(s.clamps[1]);
    assert!((got - at).norm() < 1e-6, "{got:?} {at:?}");
    assert!(
        (force(&s, "P1") - post).abs() < 1e-6 * post.abs(),
        "{}",
        force(&s, "P1")
    );
    assert!(
        (force(&s, "B1") - brace).abs() < 1e-6 * brace,
        "{}",
        force(&s, "B1")
    );
    assert!(
        post < 0.0 && brace > 0.0,
        "the post pushes, the brace pulls"
    );
    // The swing relieves the spans' difference: free to turn, the post
    // carries along the line only what its swing turns of the spans'
    // small outward pull (the top moved 7 mm in towards the structure).
    let (h1, h2) = (s.spans[0].horizontal_tension, s.spans[1].horizontal_tension);
    eprintln!("H {h1} {h2}");
    assert!((h2 - h1).abs() < 1.0, "{h1} {h2}");
    // On the structure: the post's force at A1 and the brace's at A2,
    // each with half its weight.
    let support = |label: &str| {
        s.attachment_loads[1]
            .iter()
            .find(|x| x.label == label)
            .map(|x| v(x.force))
            .unwrap_or_else(|| panic!("{label}: {:?}", s.attachment_loads[1]))
    };
    let a2 = Vector3::new(ABOVE[0], ABOVE[1], ABOVE[2]);
    let on_a1 = (at - base).normalize() * post - Vector3::new(0.0, 0.0, 225.0);
    let on_a2 = (at - a2).normalize() * brace - Vector3::new(0.0, 0.0, 100.0);
    assert!(
        (support("A1") - on_a1).norm() < 1e-6 * on_a1.norm(),
        "{:?}",
        support("A1")
    );
    assert!(
        (support("A2") - on_a2).norm() < 1e-6 * on_a2.norm(),
        "{:?}",
        support("A2")
    );
}

/// In the blocks (strung by tension: the section rolls in every weather
/// case): the section settles and the post swings until the wires' pull on
/// its top has no part along its circle — with the far end 20 m higher
/// (in still air and in 300 Pa of wind; before 2026-11-15 the rolling over
/// the sheaves turned the post without end and the section was not solved:
/// "rolling did not settle in 20 passes"), and with a 2 kN weight at the
/// middle of the second span (point loads are left out of the sharing:
/// the post swings towards the loaded span). Over sheaves and with equal H.
#[test]
fn a_pivoting_post_in_the_blocks_settles() {
    for blocks in [crate::InTheBlocks::Sheaves, crate::InTheBlocks::EqualH] {
        for (rise, wind, weight) in [(20.0, 0.0, 0.0), (20.0, 300.0, 0.0), (0.0, 0.0, 2000.0)] {
            let mut line = pivoting(
                ABOVE,
                r#"{"kind": "tension", "horizontal_tension": 20000, "temperature": 20}"#,
            );
            line.solver.blocks = blocks;
            line.structures[2].wires[0].position[2] += rise;
            if weight > 0.0 {
                line.sections[0].span_points.push(crate::SpanPoint {
                    label: "M".into(),
                    span: 1,
                    at: crate::SpanPlace::at_fraction(0.5),
                    description: String::new(),
                    object: Some(crate::LineObject::clamp(weight)),
                });
            }
            line.validate().unwrap();
            let mut case = still();
            case.wind_pressure = ohl_core::units::Stress::from_si(wind);
            let what = format!("{blocks:?}, rise {rise} m, wind {wind} Pa, weight {weight} N");
            let s = solve_section(
                &line,
                &line.sections[0],
                &cable(),
                &case,
                &default_fe_budget(),
            )
            .unwrap_or_else(|e| panic!("{what}: {e}"));
            let at = v(s.clamps[1]);
            let base = Vector3::new(300.0, 0.0, 28.4);
            let r = at - base;
            // On its circle (but for its members' stretch, some 0.1 mm).
            assert!(
                (r.xy().norm() - 1.5).abs() < 1e-4 && r.z.abs() < 1e-4,
                "{what}: {r:?}"
            );
            if weight > 0.0 {
                // Towards the loaded span.
                assert!(r.x > 0.01, "{what}: {r:?}");
            }
            // Balanced along its circle: the wires' pull on the top (with
            // half the members' weights, vertical) has no part along it.
            let tangent = Vector3::new(-r.y, r.x, 0.0).normalize();
            let pull = v(s.pulls[1]);
            assert!(
                pull.dot(&tangent).abs() < 1e-6 * pull.norm(),
                "{what}: {pull:?} {tangent:?}"
            );
        }
    }
}
