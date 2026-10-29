// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The magnetic field of the synthetic double-circuit example
//! (`examples/electrical`): symmetry, the phase arrangement, the load-flow
//! directions, the earth wire, 2D against 3D, the limit line.

use std::path::Path;

use ohl_core::units::Temperature;
use ohl_electrical::cross_section::Sources;
use ohl_electrical::magnetic::{self, CircuitCurrent, Model, Settings, StationSpec};
use ohl_line::Line;

fn example() -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let line =
        Line::from_json(&std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap())
            .unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    (line, library)
}

fn field(
    line: &Line,
    library: &ohl_cables::Library,
    settings: &Settings,
) -> magnetic::MagneticField {
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(line, library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let src = Sources {
        line,
        library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
    };
    magnetic::run(&src, settings, None).unwrap()
}

fn both(amps: f64, south_reverse: bool) -> Vec<CircuitCurrent> {
    vec![
        CircuitCurrent {
            circuit: "North".into(),
            amps,
            reverse: false,
        },
        CircuitCurrent {
            circuit: "South".into(),
            amps,
            reverse: south_reverse,
        },
    ]
}

fn mid(from: &str, to: &str) -> StationSpec {
    StationSpec {
        from: from.into(),
        to: to.into(),
        fraction: Some(0.5),
    }
}

fn at(p: &magnetic::Profile, offset: f64) -> f64 {
    p.points
        .iter()
        .find(|x| (x.offset - offset).abs() < 1e-9)
        .unwrap()
        .b_ut
}

/// The example is symmetric about its axis (both circuits alike, ±5 m,
/// the ground level across): the profile is too. Every span gets a
/// profile at its lowest point by default.
#[test]
fn symmetric_profile_and_default_stations() {
    let (line, library) = example();
    let s = Settings {
        currents: both(1000.0, false),
        ..Settings::default()
    };
    let m = field(&line, &library, &s);
    assert_eq!(m.profiles.len(), 2);
    let p = &m.profiles[0];
    assert_eq!(p.points.len(), 101);
    for x in [3.0, 10.0, 25.0, 50.0] {
        let (l, r) = (at(p, -x), at(p, x));
        assert!((l - r).abs() < 1e-6 * l.max(1e-3), "{x}: {l} {r}");
    }
    // Under the line some µT for 1000 A at 20-odd m.
    assert!(p.max_ut > 1.0 && p.max_ut < 30.0, "{}", p.max_ut);
    // The lowest point: near the middle of the span over a ground
    // sloping 2 m.
    assert!((p.at - 150.0).abs() < 20.0, "{}", p.at);
}

/// Reversing one circuit's load flow (or its phase order) changes the
/// field: with the same phasing on both sides and the same direction the
/// two circuits add far away; one reversed, they cancel far away.
#[test]
fn directions_and_phase_arrangement() {
    let (line, library) = example();
    let mut s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        ..Settings::default()
    };
    let same = field(&line, &library, &s);
    s.currents = both(1000.0, true);
    let reversed = field(&line, &library, &s);
    let (a, b) = (at(&same.profiles[0], 40.0), at(&reversed.profiles[0], 40.0));
    assert!(b < 0.5 * a, "{a} {b}");
    // The South circuit's phases in the other order (L1 at the top):
    // a different field.
    let mut swapped = line.clone();
    let sys = swapped.systems.iter_mut().find(|x| x.id == "sysB").unwrap();
    sys.phases[0].angle_deg = Some(120.0);
    sys.phases[2].angle_deg = Some(0.0);
    s.currents = both(1000.0, false);
    let other = field(&swapped, &library, &s);
    let c = at(&other.profiles[0], 40.0);
    assert!((c - a).abs() > 0.05 * a, "{a} {c}");
    assert!(c < a, "{a} {c}");
}

/// The earth wire's induced current (−Z_ee⁻¹ Z_ep I_p): some per cent of
/// the phases' current; it changes the field by some per cent. In this
/// arrangement (L1 at the bottom, L3 at the top, next to the earth wire)
/// the induced current is nearly in phase with L1 and *raises* the field
/// near the ground a little — an earth wire does not always lower it. Two
/// circuits alike with opposite load flows induce nothing (their
/// couplings cancel by symmetry).
#[test]
fn earth_wire_currents() {
    let (line, library) = example();
    let mut s = Settings {
        currents: vec![CircuitCurrent {
            circuit: "North".into(),
            amps: 1000.0,
            reverse: false,
        }],
        stations: vec![mid("T1", "T2")],
        ..Settings::default()
    };
    let without = field(&line, &library, &s);
    s.earth_wires = true;
    let with = field(&line, &library, &s);
    assert_eq!(with.earth_wires.len(), 2);
    let ie = with.earth_wires[0].amps;
    assert!(ie > 10.0 && ie < 100.0, "{ie}");
    let (a, b) = (without.profiles[0].max_ut, with.profiles[0].max_ut);
    assert!((b / a - 1.0).abs() < 0.1, "{a} {b}");
    s.currents = both(1000.0, true);
    let opposite = field(&line, &library, &s);
    assert!(opposite.earth_wires[0].amps < 1e-6 * 1000.0);
    // Both alike: twice the one circuit's.
    s.currents = both(1000.0, false);
    let two = field(&line, &library, &s);
    assert!((two.earth_wires[0].amps / ie - 2.0).abs() < 1e-9);
}

/// At a span's lowest point the 2D model (the wires at their heights
/// there, infinitely long) gives somewhat more than the 3D one (the wires
/// rising towards the structures), and not much more.
#[test]
fn two_d_against_three_d() {
    let (line, library) = example();
    let mut s = Settings {
        currents: both(1000.0, false),
        stations: vec![StationSpec {
            from: "T1".into(),
            to: "T2".into(),
            fraction: None,
        }],
        limit_ut: Some(1.0),
        ..Settings::default()
    };
    let d3 = field(&line, &library, &s);
    s.model = Model::TwoD;
    let d2 = field(&line, &library, &s);
    let (a, b) = (d3.profiles[0].max_ut, d2.profiles[0].max_ut);
    assert!(b > a && b < 1.3 * a, "3D {a} 2D {b}");
    // The limit line: symmetric, wider in 2D.
    let (l3, l2) = (d3.profiles[0].limit.unwrap(), d2.profiles[0].limit.unwrap());
    assert!((l3.left + l3.right).abs() < 1e-9, "{l3:?}");
    assert!(l2.right >= l3.right, "{l2:?} {l3:?}");
    // Margins: the limit less B.
    let p = &d3.profiles[0].points[50];
    assert!((p.margin_ut.unwrap() - (1.0 - p.b_ut)).abs() < 1e-12);
    // The line is short (600 m): the modelled length is said.
    assert!(
        d3.profiles[0]
            .warnings
            .iter()
            .any(|w| w.contains("modelled")),
        "{:?}",
        d3.profiles[0].warnings
    );
}

/// A line angle near the station: the 2D model warns, the 3D does not.
#[test]
fn two_d_warns_at_a_line_angle() {
    let (mut line, library) = example();
    // Turn the line at T2 by moving T3 15° off the straight.
    let a = 15f64.to_radians();
    let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    let (x, y) = (300.0 + 300.0 * a.cos(), -300.0 * a.sin());
    let (dx, dy) = (x - t3.position[0], y - t3.position[1]);
    t3.position[0] = x;
    t3.position[1] = y;
    for w in &mut t3.wires {
        w.position[0] += dx;
        w.position[1] += dy;
    }
    let s = Settings {
        currents: both(500.0, false),
        stations: vec![StationSpec {
            from: "T2".into(),
            to: "T3".into(),
            fraction: Some(0.1),
        }],
        model: Model::TwoD,
        ..Settings::default()
    };
    let m = field(&line, &library, &s);
    assert!(
        m.profiles[0]
            .warnings
            .iter()
            .any(|w| w.contains("line angle")),
        "{:?}",
        m.profiles[0].warnings
    );
}

/// Given points: at the profile's place, the profile's value.
#[test]
fn given_points() {
    let (line, library) = example();
    let mut s = Settings {
        currents: both(800.0, false),
        stations: vec![mid("T1", "T2")],
        ..Settings::default()
    };
    let m = field(&line, &library, &s);
    let q = &m.profiles[0].points[70];
    s.points = vec![magnetic::GivenPoint {
        name: "house".into(),
        x: q.x,
        y: q.y,
        z: None,
        height: Some(1.0),
    }];
    let m2 = field(&line, &library, &s);
    assert!((m2.points[0].b_ut - q.b_ut).abs() < 1e-9 * q.b_ut);
    assert_eq!(m2.points[0].name, "house");
    // A grid over a span: its row at the station matches the profile.
    s.grids = vec![("T1".into(), "T2".into())];
    s.step = 5.0;
    let m3 = field(&line, &library, &s);
    let g = &m3.grids[0];
    assert_eq!(g.columns, 21);
    assert_eq!(g.rows, 61);
    let csv = magnetic::csv(&m3);
    assert!(csv.lines().count() > g.points.len());
}

fn field_keeping(
    line: &Line,
    library: &ohl_cables::Library,
    settings: &Settings,
) -> (magnetic::MagneticField, magnetic::Wires3d) {
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(line, library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let src = Sources {
        line,
        library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
    };
    let (m, w) = magnetic::run_keeping(&src, settings, None).unwrap();
    (m, w.unwrap())
}

/// A vertical cut across the line: its values are the field at its
/// points (a given point there agrees), its isolines lie on the level
/// within what the drawing's interpolation allows, none under the ground.
#[test]
fn a_vertical_cut_across_the_line() {
    let (line, library) = example();
    let mut s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        cuts: vec![magnetic::CutSpec {
            from: "T1".into(),
            to: "T2".into(),
            fraction: Some(0.5),
            width: 40.0,
            above: 10.0,
            step: 1.0,
        }],
        ..Settings::default()
    };
    let (m, wires) = field_keeping(&line, &library, &s);
    let c = &m.cuts[0].values;
    let nc = c.offsets.len();
    assert_eq!(nc, 81);
    // A point of the grid as a given point.
    let (col, row) = (60, 12);
    let p = c.cut.at(c.offsets[col]);
    s.points = vec![magnetic::GivenPoint {
        name: "p".into(),
        x: p[0],
        y: p[1],
        z: Some(c.elevations[row]),
        height: None,
    }];
    let m2 = field(&line, &library, &s);
    let v = c.values[row * nc + col];
    assert!(
        (m2.points[0].b_ut - v).abs() < 1e-9 * v,
        "{} {v}",
        m2.points[0].b_ut
    );
    // The isolines of 5 µT: on the level within a few per cent, above the
    // ground.
    let lines = c.isolines(5.0);
    assert!(!lines.is_empty());
    let mut worst: f64 = 0.0;
    for l in &lines {
        for q in l {
            let g = c.ground_at(q[0]).unwrap();
            assert!(q[1] >= g - 1e-9);
            let xy = c.cut.at(q[0]);
            let b = wires.field([xy[0], xy[1], q[1]]).0 * 1e6;
            worst = worst.max((b - 5.0).abs() / 5.0);
        }
    }
    assert!(worst < 0.05, "isoline off its level by {worst}");
}

/// A volume over a span: a surface per level, every vertex on its level
/// within what the lattice's interpolation allows, and fewer points
/// computed than the full lattice.
#[test]
fn a_volume_over_a_span() {
    let (line, library) = example();
    let s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        volumes: vec![magnetic::VolumeSpec {
            from: "T1".into(),
            to: "T2".into(),
            width: 40.0,
            above: 10.0,
            step: 1.0,
        }],
        levels: vec![2.0, 10.0],
        ..Settings::default()
    };
    let (m, wires) = field_keeping(&line, &library, &s);
    let v = &m.volumes[0].values;
    // (A small box full of field: some 60 % computed; a wider one less.)
    assert!(v.computed * 10 < v.full * 7, "{} of {}", v.computed, v.full);
    let b = &v.bounds;
    for (level, mesh) in &v.surfaces {
        assert!(mesh.triangles.len() > 100, "level {level}");
        // No holes: an edge of one triangle only lies on the box's sides.
        let mut edges = std::collections::HashMap::new();
        for t in &mesh.triangles {
            for e in 0..3 {
                let (a, c) = (t[e], t[(e + 1) % 3]);
                *edges.entry((a.min(c), a.max(c))).or_insert(0) += 1;
            }
        }
        let on_side = |p: [f64; 3]| {
            let (dx, dy) = (p[0] - b.start[0], p[1] - b.start[1]);
            let t = dx * b.ahead[0] + dy * b.ahead[1];
            let s = dx * b.ahead[1] - dy * b.ahead[0];
            let d = b.dims();
            let len = (d[0] - 1) as f64 * b.step;
            let top = b.bottom + (d[2] - 1) as f64 * b.step;
            let w = -b.width + (d[1] - 1) as f64 * b.step;
            [t, len - t, s + b.width, w - s, p[2] - b.bottom, top - p[2]]
                .iter()
                .any(|x| x.abs() < 1e-6)
        };
        for ((a, c), n) in &edges {
            assert!(*n <= 2);
            if *n == 1 {
                let (p, q) = (mesh.vertices[*a as usize], mesh.vertices[*c as usize]);
                assert!(
                    on_side(p) && on_side(q),
                    "a hole at {p:?} {q:?}, level {level}"
                );
            }
        }
    }
    // How far the surfaces lie off their levels, as reported: mostly
    // close; worst where the field turns sharply between lattice points
    // (near the conductors).
    for (k, (level, _)) in v.surfaces.iter().enumerate() {
        let d = v.deviation[k].unwrap();
        assert!(d.points > 100);
        assert!(d.median < 0.005 && d.p90 < 0.02, "level {level}: {d:?}");
        // The report agrees with the field at the vertices.
        let mesh = &v.surfaces[k].1;
        let every = mesh.vertices.len().div_ceil(200);
        let worst = mesh
            .vertices
            .iter()
            .step_by(every)
            .map(|p| (wires.field(*p).0 * 1e6 - level).abs() / level)
            .fold(0.0, f64::max);
        assert!((worst - d.max).abs() < 1e-9, "{worst} {}", d.max);
    }
}

/// Shorter pieces than the solver's: the field barely changes (the
/// solver's pieces lie a few mm off the wire), the pieces and their bound
/// are said.
#[test]
fn shorter_pieces() {
    let (line, library) = example();
    let mut s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        ..Settings::default()
    };
    let a = field(&line, &library, &s);
    s.piece_length = Some(0.5);
    let b = field(&line, &library, &s);
    let (pa, pb) = (a.pieces.unwrap(), b.pieces.unwrap());
    assert!(pb.count > 4 * pa.count, "{} {}", pa.count, pb.count);
    assert!(pb.longest <= 0.5 + 1e-9);
    assert!(pb.off_wire < pa.off_wire / 10.0, "{pa:?} {pb:?}");
    for (x, y) in a.profiles[0].points.iter().zip(&b.profiles[0].points) {
        assert!(
            (x.b_ut - y.b_ut).abs() < 1e-3 * x.b_ut,
            "{} {}",
            x.b_ut,
            y.b_ut
        );
    }
}
