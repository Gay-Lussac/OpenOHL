// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Safety volumes checked by hand.

use super::*;
use crate::checks::Wire;
use crate::positions::{Axis, PositionKind, WireShape};
use ohl_line::electrical::{Potential, SystemKind};
use std::f64::consts::PI;

/// A straight level wire along x from 0 to `length` at height `z`,
/// offset `y`.
fn straight(length: f64, y: f64, z: f64, kv: Option<f64>) -> Wire {
    Wire {
        section: "S".into(),
        span: 0,
        circuit: "c".into(),
        phase: "L1".into(),
        potential: kv.map(|kv| Potential {
            nominal_kv: kv,
            kind: SystemKind::Ac,
            frequency_hz: None,
            to_earth: [kv / 3f64.sqrt(), 0.0],
        }),
        bundle: vec![],
        points: (0..=10).map(|k| [length * k as f64 / 10.0, y, z]).collect(),
        ends: vec![],
        surface_radius: 0.0,
        polyline_error: 0.0,
    }
}

/// Where the mesh's edges cross the plane x = `x`: the outline of its
/// section there.
fn section(mesh: &Mesh, x: f64) -> Vec<P3> {
    let mut out = Vec::new();
    for t in &mesh.triangles {
        let c = mesh.corners(t);
        for k in 0..3 {
            let (a, b) = (c[k], c[(k + 1) % 3]);
            if (a[0] - x) * (b[0] - x) < 0.0 {
                let s = (x - a[0]) / (b[0] - a[0]);
                out.push([x, a[1] + s * (b[1] - a[1]), a[2] + s * (b[2] - a[2])]);
            }
        }
    }
    out
}

fn params(d: f64) -> SafetyParams {
    SafetyParams {
        distance: Requirement::Fixed(d),
        ..SafetyParams::default()
    }
}

/// Points on the exact surface of a capsule of radius `r` around the
/// segment from (0, y, z) to (L, y, z).
fn capsule_surface(length: f64, y: f64, z: f64, r: f64) -> Vec<P3> {
    let mut out = Vec::new();
    for i in 0..=40 {
        let x = length * f64::from(i) / 40.0;
        for j in 0..24 {
            let a = 2.0 * PI * f64::from(j) / 24.0 + 0.013 * f64::from(i);
            out.push([x, y + r * a.cos(), z + r * a.sin()]);
        }
    }
    // The end caps.
    for j in 0..12 {
        for k in 1..6 {
            let (a, b) = (
                2.0 * PI * f64::from(j) / 12.0,
                PI / 2.0 * f64::from(k) / 6.0,
            );
            let (s, c) = b.sin_cos();
            out.push([-r * s, y + r * c * a.cos(), z + r * c * a.sin()]);
            out.push([length + r * s, y + r * c * a.cos(), z + r * c * a.sin()]);
        }
    }
    out
}

/// Hand check: a straight level wire of radius r = 0.015 m (L = 100 m),
/// still air, distance d = 3 m: the envelope is a capsule of radius
/// D = d + r = 3.015 m — volume π D² L + 4/3 π D³ = 2970.6 m³. The mesh
/// is closed, every point of the exact surface is inside it, every vertex
/// is outside it, and its volume lies between the capsule's and that of
/// the capsule grown by the allowance (D + a).
#[test]
fn a_straight_wire_gives_a_capsule() {
    let mut w = straight(100.0, 0.0, 20.0, None);
    w.surface_radius = 0.015;
    let positions = Positions::of_wires("still", vec![w]);
    let t = std::time::Instant::now();
    let v = build(&positions, &params(3.0), &Budget::new()).unwrap();
    let took = t.elapsed();
    assert_eq!(v.volumes.len(), 1);
    let s = &v.volumes[0];
    let d = 3.015;
    let (l, mesh) = (100.0, &s.mesh);
    let exact = PI * d * d * l + 4.0 / 3.0 * PI * d * d * d;
    let a = s.allowance;
    let grown = PI * (d + a) * (d + a) * l + 4.0 / 3.0 * PI * (d + a).powi(3);
    let vol = mesh.volume();
    println!(
        "capsule: D = {d} m, cell {:.3} m, level {} m, allowance {:.4} m, {} triangles ({} before simplifying), {} vertices, \
         volume {vol:.1} m³ (exact {exact:.1}, grown {grown:.1}; +{:.2} %), {took:.2?}",
        s.stats.cell,
        s.stats.level,
        a,
        mesh.triangles.len(),
        s.simplified.before,
        mesh.vertices.len(),
        100.0 * (vol / exact - 1.0)
    );
    assert_eq!(mesh.open_edges(), 0);
    assert!(vol >= exact, "{vol} < {exact}");
    assert!(vol <= grown, "{vol} > {grown}");
    assert!(a <= 0.05 + ROUNDING + 1e-9, "{a}");
    for p in capsule_surface(l, 0.0, 20.0, d) {
        assert!(mesh.contains(p), "{p:?} outside the mesh");
    }
    // Every vertex outside the envelope, by no more than the allowance.
    for q in &mesh.vertices {
        let r = q[1].hypot(q[2] - 20.0);
        let dist = if q[0] < 0.0 {
            (q[0] * q[0] + r * r).sqrt()
        } else if q[0] > l {
            ((q[0] - l).powi(2) + r * r).sqrt()
        } else {
            r
        };
        assert!(dist >= d && dist <= d + a + 1e-9, "{dist}");
    }
}

/// A swing arc about a level axis 30 m up: straight wires blown to
/// φ = ±40° at radius R = 10 m and hanging in still air at r = 8 m (a
/// band from r to R, turned rigidly), distance d = 2 m, the band's margin
/// m = [`crate::positions::BAND_MARGIN`] = 0.15 m: D = d + m = 2.15 m.
/// In the middle of the span the volume reaches R sin 40° + D = 8.578 m
/// to the right and to the left, R + D = 12.15 m below the axis, and up
/// to r cos 40° − D = 3.978 m below it (the band's inner end corners,
/// its highest points; the arc alone at R: R cos 40° − d = 5.660 m) — the
/// closed form, within the allowance (the mesh is never inside).
#[test]
fn the_swing_arc_s_extremes_match_the_closed_form() {
    let (r_out, r_in, d, z0, l) = (10.0, 8.0, 2.0, 30.0, 100.0);
    let m = crate::positions::BAND_MARGIN;
    let big = d + m;
    let axis = Axis::new([0.0, 0.0, z0], [l, 0.0, z0]).unwrap();
    let turned = |phi: f64, r: f64| {
        // Right of the axis looking along +x is −y.
        straight(l, -r * phi.sin(), z0 - r * phi.cos(), None)
    };
    let phi = 40f64.to_radians();
    let (left, still, right) = (turned(-phi, r_out), turned(0.0, r_in), turned(phi, r_out));
    let shape = |w: &Wire| WireShape::new(w.clone(), &axis);
    let arc = SwingArc::new(axis, &shape(&left), &shape(&still), &shape(&right)).unwrap();
    let mut positions = Positions::default();
    positions.cases.push("wind".into());
    let s = positions.add_span(still.clone(), axis);
    positions.add(s, 0, PositionKind::Arc, Shape::Arc(arc));
    let v = build(&positions, &params(d), &Budget::new()).unwrap();
    let s = &v.volumes[0];
    assert_eq!(s.mesh.open_edges(), 0);
    let a = s.allowance;
    println!(
        "band: {} triangles, cell {:.3} m, arc error {:.4} m, allowance {a:.4} m",
        s.mesh.triangles.len(),
        s.stats.cell,
        s.arc_error
    );
    let mid = section(&s.mesh, l / 2.0);
    assert!(!mid.is_empty());
    let max = |f: &dyn Fn(&P3) -> f64| mid.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    let lateral = max(&|p| p[1].abs());
    let below = max(&|p| z0 - p[2]);
    let above = max(&|p| p[2] - z0);
    let (e_lat, e_below, e_top) = (
        r_out * phi.sin() + big,
        r_out + big,
        -(r_in * phi.cos() - big),
    );
    assert!((e_lat - 8.578).abs() < 1e-3 && (e_top + 3.978).abs() < 1e-3);
    println!(
        "  lateral {lateral:.4} (closed form {e_lat:.4}), below {below:.4} ({e_below:.4}), top {above:.4} ({e_top:.4})"
    );
    for (got, exact) in [(lateral, e_lat), (below, e_below), (above, e_top)] {
        // Never inside; outside by at most the allowance and the arc's
        // own error (its cells grown).
        assert!(
            got >= exact && got <= exact + a + s.arc_error,
            "{got} vs {exact}"
        );
    }
    // The band's exact outer and inner offset points (radius R + D and
    // r − D from the axis, between the angles) and its inside are inside.
    for k in 0..=20 {
        let p = -phi + 2.0 * phi * f64::from(k) / 20.0;
        for x in [5.0, 50.0, 95.0] {
            for rad in [r_out + big, r_in - big, 0.5 * (r_out + r_in)] {
                let q = [x, -rad * p.sin(), z0 - rad * p.cos()];
                assert!(s.mesh.contains(q), "{q:?}");
            }
        }
    }
}

fn example() -> (ohl_line::Line, ohl_cables::Library, Vec<WeatherCase>) {
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = ohl_line::Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json"))
        .unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    let cases =
        ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&["high wind".into(), "max temperature".into()], &[])
            .unwrap();
    (line, library, cases)
}

/// Steps against the band on the example line (high wind 600 Pa, steps
/// of 100 Pa, both sides): the band's prims at D = 2 m (and the band's
/// margin, as [`build`] takes them) hold every step's tube — every point
/// D away from a stepped position's centre line, in six directions, lies
/// within the envelope (f ≤ 0). The arc alone at
/// the largest radius did not: a step's centre line lay up to 1.53 m
/// inside it (measured 2026-10-20; the band since 2026-10-21, measured at
/// 10 Pa in `tests/arc_band.rs`).
#[test]
fn the_band_holds_the_steps_tubes() {
    let (line, library, cases) = example();
    let steps = WindSteps {
        arc: false,
        stepped: true,
        step: 100.0,
        sides: crate::rules::WindSides::Both,
        ..Default::default()
    };
    let positions = solve_positions(
        &line,
        &library,
        &cases
            .into_iter()
            .filter(|c| c.name == "high wind")
            .collect::<Vec<_>>(),
        WindMode::Both.options(steps),
        &Budget::new(),
    )
    .unwrap();
    let d = 2.0;
    let mut worst = (f64::NEG_INFINITY, String::new());
    let mut n = 0;
    for span in &positions.spans {
        let Some(arc) = span
            .positions
            .iter()
            .find_map(|&i| match &positions.positions[i].shape {
                Shape::Arc(a) => Some(a),
                _ => None,
            })
        else {
            continue;
        };
        let (prims, _, _) = arc_prims(arc, d + crate::positions::BAND_MARGIN, 0.05);
        let field = Field::new(prims);
        for &i in &span.positions {
            let p = &positions.positions[i];
            let Shape::Wire(w) = &p.shape else { continue };
            if !matches!(
                p.kind,
                PositionKind::StepLeft | PositionKind::StepRight | PositionKind::Still
            ) {
                continue;
            }
            n += 1;
            for q in &w.wire.points {
                for o in [
                    [d, 0.0, 0.0],
                    [-d, 0.0, 0.0],
                    [0.0, d, 0.0],
                    [0.0, -d, 0.0],
                    [0.0, 0.0, d],
                    [0.0, 0.0, -d],
                ] {
                    let x = [q[0] + o[0], q[1] + o[1], q[2] + o[2]];
                    let f = field.value(x, f64::INFINITY);
                    if f > worst.0 {
                        worst = (
                            f,
                            positions.describe(i)
                                + &format!(" {} span {}", span.wire.section, span.wire.span + 1),
                        );
                    }
                }
            }
        }
    }
    println!(
        "steps against the band: {n} step positions, their tubes at most {:+.4} m outside the band's envelope ({})",
        worst.0, worst.1
    );
    assert!(n > 0);
    assert!(worst.0 <= 0.0, "{worst:?}");
}

/// By voltage: two straight wires of 220 and 380 kV systems 50 m apart,
/// the table 7 m up to 220 kV and 9 m up to 380 kV (invented values):
/// two volumes, reaching 7 and 9 m from their wires.
#[test]
fn each_wire_gets_the_distance_of_its_voltage() {
    let mut a = straight(60.0, 0.0, 30.0, Some(220.0));
    a.circuit = "A".into();
    let mut b = straight(60.0, 50.0, 30.0, Some(380.0));
    b.circuit = "B".into();
    b.section = "T".into();
    let positions = Positions::of_wires("still", vec![a, b]);
    let p = SafetyParams {
        distance: Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 9.0)]),
        tolerance: 0.1,
        ..SafetyParams::default()
    };
    let v = build(&positions, &p, &Budget::new()).unwrap();
    assert_eq!(v.volumes.len(), 2);
    for (s, y, d) in [(&v.volumes[0], 0.0, 7.0), (&v.volumes[1], 50.0, 9.0)] {
        assert_eq!(s.distance, d);
        let cut = section(&s.mesh, 30.0);
        let reach = cut
            .iter()
            .map(|q| (q[1] - y).hypot(q[2] - 30.0))
            .fold(0.0, f64::max);
        let least = cut
            .iter()
            .map(|q| (q[1] - y).hypot(q[2] - 30.0))
            .fold(f64::INFINITY, f64::min);
        println!(
            "{} kV: d = {d} m, its section at mid-span {least:.4}–{reach:.4} m from the wire",
            s.kv.unwrap()
        );
        // The section's corners lie on triangles' sides: not inside the
        // envelope; as far out as the allowance measured.
        assert!(
            least >= d - 0.05 && reach <= d + s.allowance + 1e-9,
            "{least} {reach}"
        );
        assert!(!cut.is_empty());
    }
    // Above the table's last row: an error naming the wire.
    let p = SafetyParams {
        distance: Requirement::ByVoltage(vec![(220.0, 7.0)]),
        ..SafetyParams::default()
    };
    let e = build(&positions, &p, &Budget::new()).unwrap_err();
    assert!(e.contains("380"), "{e}");
    // No distance: refused (none is built in).
    let e = build(&positions, &SafetyParams::default(), &Budget::new()).unwrap_err();
    assert!(e.contains("distance"), "{e}");
}

/// The example line in high wind and at the highest temperature, swing
/// arc and steps, 4 m: one closed volume per wire, and per circuit; sizes
/// and times printed (the report's numbers). Long without optimisation:
/// run with `cargo test --release -p ohl-clearance -- --ignored`.
#[test]
#[ignore]
fn the_example_line_s_volumes() {
    let (line, library, cases) = example();
    let steps = WindSteps {
        arc: false,
        stepped: true,
        step: 100.0,
        sides: crate::rules::WindSides::Both,
        ..Default::default()
    };
    for grouping in [Grouping::Wire, Grouping::Circuit] {
        let t = std::time::Instant::now();
        let v = run(
            &line,
            &library,
            &cases,
            WindMode::Both,
            steps,
            &SafetyParams {
                distance: Requirement::Fixed(4.0),
                grouping,
                ..SafetyParams::default()
            },
            &Budget::new(),
        )
        .unwrap();
        println!(
            "per {}: {} positions, {:.2?}",
            grouping.as_str(),
            v.positions,
            t.elapsed()
        );
        for s in &v.volumes {
            println!(
                "  {}: {} triangles (fine {}, checked {}), {} vertices, {:.0} m³, cell {:.3} m, allowance {:.4} m, arc error {:.4} m (median {:.4}), {:.2} s (mesh {:.2}, simplify {:.2})",
                s.name,
                s.mesh.triangles.len(),
                s.simplified.before,
                s.simplified.checked,
                s.mesh.vertices.len(),
                s.mesh.volume(),
                s.stats.cell,
                s.allowance,
                s.arc_error,
                s.arc_error_median,
                s.seconds,
                s.timings[0],
                s.timings[1]
            );
            assert_eq!(s.mesh.open_edges(), 0, "{}", s.name);
            assert!(s.mesh.volume() > 0.0);
            assert!(s.stats.vertex_min >= 0.0);
        }
        let expected = match grouping {
            Grouping::Wire => 3,
            Grouping::Circuit => 2,
            Grouping::System => 1,
        };
        assert_eq!(v.volumes.len(), expected);
    }
}

/// The number of pieces of a mesh: triangles joined through shared
/// vertices.
fn pieces(mesh: &Mesh) -> usize {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..mesh.vertices.len()).collect();
    for t in &mesh.triangles {
        let a = root(&mut parent, t[0] as usize);
        for &k in &t[1..] {
            let b = root(&mut parent, k as usize);
            parent[b] = a;
        }
    }
    let mut roots: Vec<usize> = mesh
        .triangles
        .iter()
        .map(|t| root(&mut parent, t[0] as usize))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    roots.len()
}

/// Continuity at a dead-end structure: section A ends at its clamp
/// (100, 0, 20), section B starts at the clamp on the structure's other
/// side (106, 0, 20); a jumper hangs between them, sagging 1.5 m
/// (parabola). Distance d = 1 m: the two sections' tubes alone end 6 m
/// apart, a gap of 6 − 2·1 = 4 m (two pieces); with the jumper, its tube
/// joins them (one closed piece) and every point within d of the jumper
/// is inside the volume. Structures are not part of the volume.
#[test]
fn the_jumper_closes_the_gap_at_a_dead_end() {
    let a = straight(100.0, 0.0, 20.0, None);
    let mut b = straight(100.0, 0.0, 20.0, None);
    b.section = "B".into();
    for p in &mut b.points {
        p[0] += 106.0;
    }
    let jumper_points: Vec<P3> = (0..=12)
        .map(|k| {
            let t = f64::from(k) / 12.0;
            [100.0 + 6.0 * t, 0.0, 20.0 - 4.0 * 1.5 * t * (1.0 - t)]
        })
        .collect();
    let jumper = Wire {
        section: "jumper:J".into(),
        points: jumper_points.clone(),
        ..a.clone()
    };
    let p = SafetyParams {
        distance: Requirement::Fixed(1.0),
        tolerance: 0.05,
        ..SafetyParams::default()
    };
    let without = build(
        &Positions::of_wires("still", vec![a.clone(), b.clone()]),
        &p,
        &Budget::new(),
    )
    .unwrap();
    assert_eq!(without.volumes.len(), 1);
    assert_eq!(pieces(&without.volumes[0].mesh), 2, "the gap without it");
    let with = build(
        &Positions::of_wires("still", vec![a, b, jumper]),
        &p,
        &Budget::new(),
    )
    .unwrap();
    assert_eq!(with.volumes.len(), 1);
    let v = &with.volumes[0];
    println!(
        "dead end: {} triangles, {} piece(s), sections {:?}",
        v.mesh.triangles.len(),
        pieces(&v.mesh),
        v.sections
    );
    assert_eq!(v.mesh.open_edges(), 0);
    assert_eq!(pieces(&v.mesh), 1, "no gap with the jumper");
    assert!(v.sections.contains(&"jumper:J".to_owned()));
    // Points within d of the jumper (on it, and 0.95 m off it across and
    // below) are inside.
    for q in &jumper_points {
        for off in [
            [0.0, 0.0, 0.0],
            [0.0, 0.95, 0.0],
            [0.0, -0.95, 0.0],
            [0.0, 0.0, -0.95],
        ] {
            let x = [q[0] + off[0], q[1] + off[1], q[2] + off[2]];
            assert!(v.mesh.contains(x), "{x:?}");
        }
    }
}

/// The example line in high wind, swing arc: the jumper's positions are
/// taken (blown left and right as solved, and on a swing band of its own
/// between them through still air, 2026-10-21), and the spans' arcs as
/// before; every band widened by the margin asked for; the danger trees'
/// options leave the jumpers out.
#[test]
fn the_volume_s_positions_include_the_jumpers() {
    let (line, library, cases) = example();
    let windy: Vec<WeatherCase> = cases
        .into_iter()
        .filter(|c| c.name == "high wind")
        .collect();
    let positions = solve_positions(
        &line,
        &library,
        &windy,
        WindMode::Arc.options(WindSteps {
            band_margin: 0.4,
            ..WindSteps::default()
        }),
        &Budget::new(),
    )
    .unwrap();
    assert!(positions.notes.is_empty(), "{:?}", positions.notes);
    let span = positions
        .spans
        .iter()
        .find(|s| s.wire.section == "jumper:J-T3-branch")
        .expect("the jumper's positions");
    let mut kinds: Vec<&str> = span
        .positions
        .iter()
        .map(|&i| positions.positions[i].kind.name())
        .collect();
    kinds.sort_unstable();
    let mut expected = vec![
        PositionKind::Left.name(),
        PositionKind::Right.name(),
        PositionKind::Arc.name(),
    ];
    expected.sort_unstable();
    assert_eq!(kinds, expected);
    let arcs: Vec<&SwingArc> = positions
        .positions
        .iter()
        .filter_map(|p| match &p.shape {
            Shape::Arc(a) => Some(a),
            _ => None,
        })
        .collect();
    assert!(arcs.len() > 1);
    assert!(arcs.iter().all(|a| a.margin() == 0.4));
    let plain = solve_positions(
        &line,
        &library,
        &windy,
        PositionOptions::default(),
        &Budget::new(),
    )
    .unwrap();
    assert!(
        plain
            .spans
            .iter()
            .all(|s| !s.wire.section.starts_with("jumper:"))
    );
}
