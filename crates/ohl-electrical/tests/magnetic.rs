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
        jumpers: &[],
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
    // ±100 m every 1 m (`magnetic::WIDTH`, 2026-11-02).
    assert_eq!(p.points.len(), 201);
    for x in [3.0, 10.0, 25.0, 50.0, 100.0] {
        let (l, r) = (at(p, -x), at(p, x));
        assert!((l - r).abs() < 1e-6 * l.max(1e-3), "{x}: {l} {r}");
    }
    // Under the line some µT for 1000 A at 20-odd m.
    assert!(p.max_ut > 1.0 && p.max_ut < 30.0, "{}", p.max_ut);
    // The lowest point: near the middle of the span over a ground
    // sloping 2 m.
    assert!((p.at - 150.0).abs() < 20.0, "{}", p.at);
}

/// A system's operating current in the line file (2026-11-04) is its
/// circuits' current when none is given; a current given overrides it.
#[test]
fn the_systems_current_is_the_default() {
    let (mut line, library) = example();
    let given = Settings {
        currents: both(800.0, false),
        ..Settings::default()
    };
    let by_hand = field(&line, &library, &given);
    // Nothing given, nothing in the line: no field.
    let none = field(&line, &library, &Settings::default());
    assert!(none.profiles[0].max_ut < 1e-9);
    for s in &mut line.systems {
        s.current_a = Some(800.0);
    }
    let from_line = field(&line, &library, &Settings::default());
    let (a, b) = (by_hand.profiles[0].max_ut, from_line.profiles[0].max_ut);
    assert!((a - b).abs() < 1e-9 * a, "{a} {b}");
    assert!(
        from_line
            .notes
            .iter()
            .any(|n| n.contains("operating current from the line")),
        "{:?}",
        from_line.notes
    );
    // A current given still overrides the line's.
    let half = Settings {
        currents: both(400.0, false),
        ..Settings::default()
    };
    let over = field(&line, &library, &half);
    assert!((over.profiles[0].max_ut - a / 2.0).abs() < 1e-6 * a);
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
    assert_eq!(g.columns, 41);
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
        jumpers: &[],
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
            at: magnetic::CutAt::Station {
                alignment: "main".into(),
                station: 150.0,
            },
            width: 40.0,
            above: 10.0,
            step: 1.0,
            label: Some("house A".into()),
        }],
        ..Settings::default()
    };
    let (m, wires) = field_keeping(&line, &library, &s);
    // The label carried to the result and the CSV's last column.
    assert_eq!(m.cuts[0].label.as_deref(), Some("house A"));
    let csv = magnetic::csv(&m);
    assert!(csv.lines().next().unwrap().ends_with(",label"));
    assert!(
        csv.lines()
            .filter(|l| l.starts_with("cut,"))
            .all(|l| l.ends_with(",house A"))
    );
    // The results as `ohl-field-results` JSON: the CSV's values, the
    // label, the isolines of the levels.
    let levels = Settings {
        levels: vec![5.0],
        ..s.clone()
    };
    let json: serde_json::Value = serde_json::from_str(
        &ohl_electrical::field_results::magnetic_json(&m, &levels, "still air", 40.0),
    )
    .unwrap();
    assert_eq!(json["format"], "ohl-field-results");
    assert_eq!(json["version"], 2);
    assert_eq!(json["case"]["name"], "still air");
    assert!(json["settings"]["cuts"].is_null() && json["settings"]["height"] == 1.0);
    let cut = &json["cuts"][0];
    assert_eq!(cut["label"], "house A");
    assert_eq!(cut["alignment"], "main");
    assert_eq!(cut["station"], 150.0);
    assert_eq!(
        (cut["from"].as_str(), cut["to"].as_str()),
        (Some("T1"), Some("T2"))
    );
    assert!((cut["at"].as_f64().unwrap() - 150.0).abs() < 1e-9);
    assert_eq!(cut["quantity"], "B");
    let values: Vec<f64> = cut["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let from_csv: Vec<f64> = csv
        .lines()
        .filter(|l| l.starts_with("cut,"))
        .map(|l| l.split(',').nth(10).unwrap().parse().unwrap())
        .collect();
    assert_eq!(values.len(), from_csv.len());
    assert!(
        values
            .iter()
            .zip(&from_csv)
            .all(|(a, b)| (a - b).abs() <= 0.6e-5)
    );
    assert_eq!(cut["isolines"][0]["level"], 5.0);
    assert!(!cut["isolines"][0]["lines"].as_array().unwrap().is_empty());
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

fn run_result(
    line: &Line,
    library: &ohl_cables::Library,
    settings: &Settings,
) -> Result<magnetic::MagneticField, String> {
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
        jumpers: &[],
    };
    magnetic::run(&src, settings, None)
}

/// The example turned at T2 by `deg` to the right: T3 (its wires and its
/// alignment point) moved.
fn turned(deg: f64) -> (Line, ohl_cables::Library) {
    let (mut line, library) = example();
    let a = deg.to_radians();
    let (x, y) = (300.0 + 300.0 * a.cos(), -300.0 * a.sin());
    let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    let (dx, dy) = (x - t3.position[0], y - t3.position[1]);
    t3.position[0] = x;
    t3.position[1] = y;
    for w in &mut t3.wires {
        w.position[0] += dx;
        w.position[1] += dy;
    }
    let p3 = line
        .alignment_points
        .iter_mut()
        .find(|p| p.id == "P3")
        .unwrap();
    p3.position[0] = x;
    p3.position[1] = y;
    (line, library)
}

fn cut_at(at: magnetic::CutAt) -> magnetic::CutSpec {
    magnetic::CutSpec {
        at,
        width: 20.0,
        above: 5.0,
        step: 2.0,
        label: None,
    }
}

/// A cut is placed on the alignment (owner, 2026-10-31: "cuts are made on
/// the alignment not the circuits or span"): at an angle point along the
/// bisector, on exactly the plane the cross-section panels take
/// (`ohl_line::route::plane_at`, which OHL-Design's panels call); the span
/// ahead of the structure is the one crossed.
#[test]
fn a_cut_at_an_angle_point_lies_along_the_bisector() {
    let (line, library) = turned(20.0);
    let s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        cuts: vec![cut_at(magnetic::CutAt::Station {
            alignment: "main".into(),
            station: 300.0,
        })],
        ..Settings::default()
    };
    let m = field(&line, &library, &s);
    let c = &m.cuts[0];
    assert!(c.angle_point);
    assert_eq!((c.alignment.as_str(), c.station), ("main", 300.0));
    assert_eq!((c.from.as_str(), c.to.as_str()), ("T2", "T3"));
    // The panel's plane, the same numbers.
    let view = line.alignment_views().remove(0);
    let panel = view.plane_at(300.0).unwrap();
    assert!(panel.angle_point);
    assert_eq!(c.values.cut.origin, panel.plane.origin);
    assert_eq!(c.values.cut.right, panel.plane.right());
    // Along the bisector: the plane's ahead halfway between the two
    // directions (0° and −20°), i.e. −10°.
    let ahead = panel.plane.ahead;
    let want = (-10f64).to_radians();
    assert!((ahead[0] - want.cos()).abs() < 1e-12 && (ahead[1] - want.sin()).abs() < 1e-12);
    assert!((c.values.cut.origin[0] - 300.0).abs() < 1e-9 && c.values.cut.origin[1].abs() < 1e-9);
    // Off an angle point: perpendicular to the alignment's segment.
    let s2 = Settings {
        cuts: vec![cut_at(magnetic::CutAt::Station {
            alignment: "main".into(),
            station: 450.0,
        })],
        ..s.clone()
    };
    let m2 = field(&line, &library, &s2);
    let c2 = &m2.cuts[0];
    assert!(!c2.angle_point);
    let p2 = view.plane_at(450.0).unwrap().plane;
    assert_eq!(c2.values.cut.origin, p2.origin);
    assert_eq!(c2.values.cut.right, p2.right());
    assert!((c2.at - 150.0).abs() < 1e-6, "{}", c2.at);
}

/// A place in a span (the old placement, still taken) is converted to the
/// alignment station of that place, said in the result; a station beyond
/// the alignment and a line without an alignment are errors saying so.
#[test]
fn a_span_place_becomes_an_alignment_station() {
    let (line, library) = example();
    let s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        cuts: vec![
            cut_at(magnetic::CutAt::InSpan {
                from: "T2".into(),
                to: "T3".into(),
                fraction: Some(0.25),
            }),
            cut_at(magnetic::CutAt::InSpan {
                from: "T1".into(),
                to: "T2".into(),
                fraction: None,
            }),
        ],
        ..Settings::default()
    };
    let m = field(&line, &library, &s);
    assert_eq!(m.cuts[0].alignment, "main");
    assert!((m.cuts[0].station - 375.0).abs() < 1e-9);
    assert!((m.cuts[0].at - 75.0).abs() < 1e-9);
    // The lowest point: where the profile at the lowest point is.
    let lowest = field(
        &line,
        &library,
        &Settings {
            stations: vec![StationSpec {
                from: "T1".into(),
                to: "T2".into(),
                fraction: None,
            }],
            cuts: Vec::new(),
            ..s.clone()
        },
    );
    assert!((m.cuts[1].station - lowest.profiles[0].at).abs() < 1e-6);
    // Beyond the alignment (600 m long).
    let beyond = Settings {
        cuts: vec![cut_at(magnetic::CutAt::Station {
            alignment: "main".into(),
            station: 650.0,
        })],
        ..s.clone()
    };
    // (A cut that fails is said, the rest of the run kept: 2026-11-14.)
    let not_computed = |m: &magnetic::MagneticField| {
        m.notes
            .iter()
            .find(|n| n.starts_with("NOT COMPUTED"))
            .cloned()
            .unwrap_or_default()
    };
    let m = run_result(&line, &library, &beyond).unwrap();
    assert!(m.cuts.is_empty());
    let e = not_computed(&m);
    assert!(e.contains("beyond"), "{e}");
    // No alignment: no cut, said.
    let mut bare = line.clone();
    bare.alignments.clear();
    bare.alignment_points.clear();
    bare.routes.clear();
    let m = run_result(&bare, &library, &s).unwrap();
    let e = not_computed(&m);
    assert!(e.contains("on no alignment"), "{e}");
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
        // (One span: the path is a straight from its first point.)
        let (p0, p1) = (b.path.points[0], b.path.points[1]);
        let len = b.path.length();
        let ahead = [(p1[0] - p0[0]) / len, (p1[1] - p0[1]) / len];
        let on_side = |p: [f64; 3]| {
            let (dx, dy) = (p[0] - p0[0], p[1] - p0[1]);
            let t = dx * ahead[0] + dy * ahead[1];
            let s = dx * ahead[1] - dy * ahead[0];
            let d = b.dims();
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

/// The example with circuit North's L1 cut at T2 into two sections joined
/// by a jumper (T2's wire point 1 to a new one 0.4 m ahead).
fn with_a_jumper() -> (Line, ohl_cables::Library) {
    with_a_jumper_drawn(false)
}

/// [`with_a_jumper`], the second section's points given from T3 back to
/// T2 when `backwards` (the jumper then joins two last points).
fn with_a_jumper_drawn(backwards: bool) -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap(),
    )
    .unwrap();
    let t2 = v["structures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["id"] == "T2")
        .unwrap();
    t2["wires"].as_array_mut().unwrap().push(serde_json::json!(
        {"label": "W11", "position": [300.4, 5.0, 424.0], "set": 11}
    ));
    let sections = v["sections"].as_array_mut().unwrap();
    let k = sections.iter().position(|s| s["id"] == "NL1").unwrap();
    let mut a = sections[k].clone();
    let mut b = sections[k].clone();
    a["id"] = "NL1a".into();
    a["points"] = serde_json::json!([{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1}]);
    b["id"] = "NL1b".into();
    b["points"] = if backwards {
        serde_json::json!([{"structure": "T3", "set": 1}, {"structure": "T2", "set": 11}])
    } else {
        serde_json::json!([{"structure": "T2", "set": 11}, {"structure": "T3", "set": 1}])
    };
    sections[k] = a;
    sections.push(b);
    v["jumpers"] = serde_json::json!([{
        "id": "J1", "cable": "acsr-drake-26-7",
        "from": {"structure": "T2", "set": 1}, "to": {"structure": "T2", "set": 11},
        "sag": 1.0, "temperature": 15.0
    }]);
    let line = Line::from_json(&v.to_string()).unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    (line, library)
}

/// A jumper carries its phase's current: next to it, the line cut and
/// jumpered gives about the field of the line uncut (the jumper hangs a
/// little lower); without the jumper the field there is clearly another.
#[test]
fn a_jumper_carries_its_phase_current() {
    let point = |y: f64, z: f64| magnetic::GivenPoint {
        name: "beside".into(),
        x: 300.2,
        y,
        z: Some(z),
        height: None,
    };
    let s = |jumpers: bool| Settings {
        currents: vec![CircuitCurrent {
            circuit: "North".into(),
            amps: 1000.0,
            reverse: false,
        }],
        stations: vec![mid("T1", "T2")],
        points: vec![point(5.0, 421.0), point(7.0, 423.0)],
        jumpers,
        ..Settings::default()
    };
    let (line, library) = example();
    let whole = field(&line, &library, &s(true));
    let (cut, library) = with_a_jumper();
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&cut, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&cut, &library, &case, &shapes)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let src = Sources {
        line: &cut,
        library: &library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
        jumpers: &jumpers,
    };
    let with = magnetic::run(&src, &s(true), None).unwrap();
    let without = magnetic::run(&src, &s(false), None).unwrap();
    for k in 0..2 {
        let (w, j, n) = (
            whole.points[k].b_ut,
            with.points[k].b_ut,
            without.points[k].b_ut,
        );
        assert!(
            (j - w).abs() < 0.05 * w,
            "point {k}: whole {w}, jumpered {j}"
        );
        assert!(
            (n - w).abs() > 2.0 * (j - w).abs(),
            "point {k}: whole {w}, jumpered {j}, none {n}"
        );
    }
    assert!(
        !with.notes.iter().any(|n| n.contains("without a current")),
        "{:?}",
        with.notes
    );
}

/// The earth wire network over the line: with earthings of no resistance
/// it is the infinitely long earth wire earthed everywhere; with very
/// high ones no current flows; the towers' currents into the ground add
/// to nothing (they close through the earth wires); a tower's current
/// changes the field beside it.
#[test]
fn the_earth_wire_network() {
    let (mut line, library) = example();
    let s = |towers: bool| Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        earth_wires: true,
        towers,
        points: vec![magnetic::GivenPoint {
            name: "at the foot".into(),
            x: 302.0,
            y: 0.0,
            z: None,
            height: Some(1.0),
        }],
        ..Settings::default()
    };
    let infinite = field(&line, &library, &s(false));
    line.electrical.earthing_resistance_ohm = Some(1e-7);
    let tight = field(&line, &library, &s(true));
    assert_eq!(infinite.earth_wires.len(), tight.earth_wires.len());
    for a in &infinite.earth_wires {
        let b = tight
            .earth_wires
            .iter()
            .find(|b| b.section == a.section && b.from == a.from)
            .unwrap();
        assert!(
            (a.amps - b.amps).abs() < 1e-3 * a.amps.max(1e-6),
            "{a:?} {b:?}"
        );
    }
    line.electrical.earthing_resistance_ohm = Some(1e7);
    let loose = field(&line, &library, &s(true));
    assert!(
        loose.earth_wires.iter().all(|e| e.amps < 1e-3),
        "{:?}",
        loose.earth_wires
    );
    line.electrical.earthing_resistance_ohm = Some(10.0);
    let ten = field(&line, &library, &s(true));
    assert_eq!(ten.towers.len(), 3);
    let sum = ten
        .towers
        .iter()
        .fold(magnetic::C::new(0.0, 0.0), |a, t| a + t.current);
    let largest = ten.towers.iter().map(|t| t.amps).fold(0.0, f64::max);
    assert!(largest > 0.1, "{:?}", ten.towers);
    assert!(sum.norm() < 1e-9 * largest.max(1.0), "{sum}");
    // The towers' paths change the field at a tower's foot.
    let at_foot = |m: &magnetic::MagneticField| m.points[0].b_ut;
    let without = {
        line.electrical.earthing_resistance_ohm = Some(1e7);
        field(&line, &library, &s(true))
    };
    assert!(
        (at_foot(&ten) - at_foot(&without)).abs() > 1e-4,
        "{} {}",
        at_foot(&ten),
        at_foot(&without)
    );
}

/// The current's direction follows the circuit through its sections and
/// jumpers (2026-10-29): a section given backwards (from T3 to T2, the
/// jumper joining two last points) carries the current the same way along
/// the line, so the field is the same as with it given forwards.
#[test]
fn a_section_given_backwards_carries_the_current_on() {
    let s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2"), mid("T2", "T3")],
        points: vec![magnetic::GivenPoint {
            name: "by the jumper".into(),
            x: 300.2,
            y: 7.0,
            z: Some(423.0),
            height: None,
        }],
        ..Settings::default()
    };
    let run = |backwards: bool| {
        let (line, library) = with_a_jumper_drawn(backwards);
        let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
        let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let src = Sources {
            line: &line,
            library: &library,
            shapes: &shapes,
            temperature: Temperature::from_celsius(40.0),
            ground: None,
            jumpers: &jumpers,
        };
        magnetic::run(&src, &s, None).unwrap()
    };
    let (fwd, back) = (run(false), run(true));
    assert!(
        !back.notes.iter().any(|n| n.contains("without a current")),
        "{:?}",
        back.notes
    );
    for (a, b) in fwd.profiles.iter().zip(&back.profiles) {
        assert!(
            (a.max_ut - b.max_ut).abs() < 1e-3 * a.max_ut,
            "{} {}",
            a.max_ut,
            b.max_ut
        );
    }
    let (a, b) = (fwd.points[0].b_ut, back.points[0].b_ut);
    assert!((a - b).abs() < 1e-2 * a, "{a} {b}");
}

/// Currents in and out at a circuit's end points (owner, 2026-10-29):
/// the jumpered example's circuit North has its ends at T1 and T3; 1000 A
/// in at T1 and out at T3 is the circuit's 1000 A along the line, in at T3
/// the same reversed; the ends are listed; a sum not 0, a structure not an
/// end and two "rests" are refused.
#[test]
fn currents_at_the_end_points() {
    let (line, library) = with_a_jumper();
    let ends = magnetic::circuit_ends(&line);
    let north = ends.iter().find(|(k, _)| k.circuit == "North").unwrap();
    assert_eq!(north.1, vec!["T1".to_string(), "T3".to_string()]);
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let src = Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
        jumpers: &jumpers,
    };
    let base = Settings {
        stations: vec![mid("T1", "T2"), mid("T2", "T3")],
        points: vec![magnetic::GivenPoint {
            name: "by the jumper".into(),
            x: 300.2,
            y: 7.0,
            z: Some(423.0),
            height: None,
        }],
        ..Settings::default()
    };
    let end = |s: &str, a: Option<f64>| magnetic::EndCurrent {
        circuit: "North".into(),
        structure: s.into(),
        amps: a,
    };
    let by = |reverse: bool| {
        let s = Settings {
            currents: vec![CircuitCurrent {
                circuit: "North".into(),
                amps: 1000.0,
                reverse,
            }],
            ..base.clone()
        };
        magnetic::run(&src, &s, None).unwrap()
    };
    let at = |ends: Vec<magnetic::EndCurrent>| {
        let s = Settings {
            ends,
            ..base.clone()
        };
        magnetic::run(&src, &s, None)
    };
    for (circuit, ends) in [
        (by(false), vec![end("T1", Some(1000.0)), end("T3", None)]),
        (
            by(true),
            vec![end("T3", Some(1000.0)), end("T1", Some(-1000.0))],
        ),
    ] {
        let e = at(ends).unwrap();
        for (a, b) in circuit.profiles.iter().zip(&e.profiles) {
            assert!(
                (a.max_ut - b.max_ut).abs() < 1e-9 * a.max_ut,
                "{} {}",
                a.max_ut,
                b.max_ut
            );
        }
        assert!((circuit.points[0].b_ut - e.points[0].b_ut).abs() < 1e-9 * circuit.points[0].b_ut);
        assert!(
            e.notes.iter().any(|n| n.contains("end points")),
            "{:?}",
            e.notes
        );
    }
    assert!(at(vec![end("T1", Some(1000.0)), end("T3", Some(-900.0))]).is_err());
    assert!(at(vec![end("T2", Some(1000.0))]).is_err());
    assert!(at(vec![end("T1", None), end("T3", None)]).is_err());
}

/// A circuit's phases as typed: `amps` at `angles` (degrees), L1 to L3.
fn per_phase(circuit: &str, amps: [f64; 3], angles: [f64; 3]) -> Vec<magnetic::PhaseCurrent> {
    ["L1", "L2", "L3"]
        .iter()
        .zip(amps.iter().zip(angles))
        .map(|(phase, (a, angle_deg))| magnetic::PhaseCurrent {
            circuit: circuit.into(),
            phase: (*phase).into(),
            amps: *a,
            angle_deg,
        })
        .collect()
}

/// A current per wire (owner, 2026-11-11). The phases typed at the
/// balanced values give the balanced field; three equal in-phase currents
/// (a pure net current of 3 × 100 A) give, far from the line, the field
/// of one conductor carrying their sum, μ0 I / (2π d) = 0.12 µT at 500 m
/// — falling as 1/d, where the balanced circuit's falls as 1/d². Balanced
/// input says that it holds no net current (not on the safe side far from
/// the line); input per phase names its net current instead.
#[test]
fn currents_per_phase_and_the_net_current() {
    let (line, library) = example();
    let far = Settings {
        model: Model::TwoD,
        stations: vec![mid("T1", "T2")],
        width: 500.0,
        step: 250.0,
        ..Settings::default()
    };
    let balanced = field(
        &line,
        &library,
        &Settings {
            currents: vec![CircuitCurrent {
                circuit: "North".into(),
                amps: 1000.0,
                reverse: false,
            }],
            ..far.clone()
        },
    );
    assert!(
        balanced.notes.iter().any(|n| n == magnetic::BALANCED_NOTE),
        "{:?}",
        balanced.notes
    );
    assert!(magnetic::BALANCED_NOTE.starts_with("NOT ON THE SAFE SIDE far from the line: "));
    // The same currents typed per phase: the same field, to rounding.
    let typed = field(
        &line,
        &library,
        &Settings {
            phase_currents: per_phase("North", [1000.0; 3], [0.0, -120.0, 120.0]),
            ..far.clone()
        },
    );
    for (a, b) in balanced.profiles[0]
        .points
        .iter()
        .zip(&typed.profiles[0].points)
    {
        assert!(
            (a.b_ut - b.b_ut).abs() <= 1e-9 * a.b_ut.max(1e-12),
            "{a:?} {b:?}"
        );
    }
    // Every current typed per phase: nothing balanced is assumed.
    assert!(!typed.notes.iter().any(|n| n == magnetic::BALANCED_NOTE));
    let said = typed
        .notes
        .iter()
        .find(|n| n.starts_with("circuit North: currents per phase as given"))
        .expect("the phases and their sum are said");
    assert!(said.contains("L2 1000.0 A at -120.0°"), "{said}");
    assert!(said.contains("the net current, 0.0 A"), "{said}");
    assert_eq!(typed.currents[0].source, magnetic::PER_PHASE_SOURCE);
    assert_eq!(typed.currents[0].amps, 1000.0);
    // The balanced circuit far away: as 1/d² (a quarter at twice the
    // distance, within the wires' spread over the distance).
    let p = &balanced.profiles[0];
    let ratio = at(p, 500.0) / at(p, 250.0);
    assert!((ratio - 0.25).abs() < 0.03, "{ratio}");
    // A pure net current, 3 × 100 A in phase: as 1/d, and the size of one
    // conductor's field at the sum — by hand μ0 · 300 A / (2π · 500 m)
    // = 2e-7 · 300 / 500 T = 0.12 µT (the mean of both sides: the
    // circuit hangs 5 m beside the axis).
    let net = field(
        &line,
        &library,
        &Settings {
            phase_currents: per_phase("North", [100.0; 3], [0.0; 3]),
            ..far.clone()
        },
    );
    let p = &net.profiles[0];
    let mean = 0.5 * (at(p, 500.0) + at(p, -500.0));
    assert!((mean / 0.12 - 1.0).abs() < 0.005, "{mean}");
    let ratio = at(p, 500.0) / at(p, 250.0);
    assert!((ratio - 0.5).abs() < 0.01, "{ratio}");
    assert!(
        net.notes
            .iter()
            .any(|n| n.contains("the net current, 300.0 A at 0.0°")),
        "{:?}",
        net.notes
    );
    // An unbalance of 5 % in one phase leaves 50 A: at 500 m more than the
    // whole balanced 1000 A gives there.
    let unbalanced = field(
        &line,
        &library,
        &Settings {
            phase_currents: per_phase("North", [1050.0, 1000.0, 1000.0], [0.0, -120.0, 120.0]),
            ..far.clone()
        },
    );
    assert!(
        unbalanced
            .notes
            .iter()
            .any(|n| n.contains("the net current, 50.0 A at 0.0°")),
        "{:?}",
        unbalanced.notes
    );
    let (b, u) = (&balanced.profiles[0], &unbalanced.profiles[0]);
    assert!(
        at(u, 500.0) > 2.0 * at(b, 500.0),
        "{} {}",
        at(u, 500.0),
        at(b, 500.0)
    );
    // One circuit per phase, the other balanced: the note stays for it.
    let mixed = field(
        &line,
        &library,
        &Settings {
            currents: vec![CircuitCurrent {
                circuit: "South".into(),
                amps: 500.0,
                reverse: false,
            }],
            phase_currents: per_phase("North", [1000.0; 3], [0.0, -120.0, 120.0]),
            ..far.clone()
        },
    );
    assert!(mixed.notes.iter().any(|n| n == magnetic::BALANCED_NOTE));
    // Refused: a phase missing, a phase the system has not, per phase
    // with end points.
    let refused = |s: Settings| {
        let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
        let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let src = Sources {
            line: &line,
            library: &library,
            shapes: &shapes,
            temperature: Temperature::from_celsius(40.0),
            ground: None,
            jumpers: &[],
        };
        magnetic::run(&src, &s, None).unwrap_err()
    };
    let mut two = per_phase("North", [1000.0; 3], [0.0, -120.0, 120.0]);
    two.pop();
    let e = refused(Settings {
        phase_currents: two,
        ..far.clone()
    });
    assert!(e.contains("need every phase") && e.contains("L3"), "{e}");
    let mut wrong = per_phase("North", [1000.0; 3], [0.0, -120.0, 120.0]);
    wrong[1].phase = "L9".into();
    let e = refused(Settings {
        phase_currents: wrong,
        ..far.clone()
    });
    assert!(e.contains("no phase L9"), "{e}");
    let e = refused(Settings {
        phase_currents: per_phase("North", [1000.0; 3], [0.0, -120.0, 120.0]),
        ends: vec![magnetic::EndCurrent {
            circuit: "North".into(),
            structure: "T1".into(),
            amps: Some(1000.0),
        }],
        ..far
    });
    assert!(e.contains("give one of the two"), "{e}");
}

/// An earth wire's current typed instead of computed (owner,
/// 2026-11-11): typed at the value the phases induce, the field is the
/// induced one's; it is carried with the earth wire currents off, in
/// every span; alone (no phase current) it is one conductor's field, as
/// 1/d; it is not taken together with the structures' network, nor for a
/// phase conductor.
#[test]
fn an_earth_wire_current_typed() {
    let (line, library) = example();
    let base = Settings {
        model: Model::TwoD,
        currents: vec![CircuitCurrent {
            circuit: "North".into(),
            amps: 1000.0,
            reverse: false,
        }],
        stations: vec![mid("T1", "T2")],
        width: 500.0,
        step: 250.0,
        ..Settings::default()
    };
    let induced = field(
        &line,
        &library,
        &Settings {
            earth_wires: true,
            ..base.clone()
        },
    );
    let row = induced
        .earth_wires
        .iter()
        .find(|e| e.from == "T1" && e.to == "T2")
        .unwrap();
    let typed = |amps: f64, angle_deg: f64| magnetic::EarthCurrent {
        section: "EW".into(),
        amps,
        angle_deg,
        frequency_hz: None,
    };
    // Typed at the induced value, the induced currents off: the same field.
    let same = field(
        &line,
        &library,
        &Settings {
            earth_currents: vec![typed(row.amps, row.angle_deg)],
            ..base.clone()
        },
    );
    for (a, b) in induced.profiles[0]
        .points
        .iter()
        .zip(&same.profiles[0].points)
    {
        assert!(
            (a.b_ut - b.b_ut).abs() <= 1e-9 * a.b_ut.max(1e-12),
            "{a:?} {b:?}"
        );
    }
    // In every span of its section, as typed.
    assert_eq!(same.earth_wires.len(), 2);
    for e in &same.earth_wires {
        assert!((e.amps - row.amps).abs() < 1e-9 && e.frequency_hz == 50.0);
    }
    assert!(
        same.notes
            .iter()
            .any(|n| n.starts_with("earth wire EW: its current as typed, ")),
        "{:?}",
        same.notes
    );
    // Another value: another field (it is the typed one that counts, also
    // with the induced currents on).
    let other = field(
        &line,
        &library,
        &Settings {
            earth_wires: true,
            earth_currents: vec![typed(3.0 * row.amps, row.angle_deg)],
            ..base.clone()
        },
    );
    assert!((other.earth_wires[0].amps - 3.0 * row.amps).abs() < 1e-9);
    assert!(at(&other.profiles[0], 500.0) != at(&induced.profiles[0], 500.0));
    // Alone, 60 A: one conductor's field far away, μ0 · 60 A / (2π · 500 m)
    // = 2e-7 · 60 / 500 T = 0.024 µT (the earth wire on the axis), as 1/d.
    let alone = field(
        &line,
        &library,
        &Settings {
            currents: Vec::new(),
            phase_currents: Vec::new(),
            earth_currents: vec![typed(60.0, 0.0)],
            ..base.clone()
        },
    );
    let p = &alone.profiles[0];
    assert!(
        (at(p, 500.0) / 0.024 - 1.0).abs() < 0.005,
        "{}",
        at(p, 500.0)
    );
    assert!((at(p, 500.0) / at(p, 250.0) - 0.5).abs() < 0.005);
    // Refused with the structures' network, and for a phase conductor.
    let refused = |s: Settings| {
        let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
        let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let src = Sources {
            line: &line,
            library: &library,
            shapes: &shapes,
            temperature: Temperature::from_celsius(40.0),
            ground: None,
            jumpers: &[],
        };
        magnetic::run(&src, &s, None).unwrap_err()
    };
    let e = refused(Settings {
        model: Model::ThreeD,
        earth_wires: true,
        towers: true,
        earth_currents: vec![typed(60.0, 0.0)],
        ..base.clone()
    });
    assert!(
        e.contains("the network computes every earth wire's current"),
        "{e}"
    );
    let e = refused(Settings {
        earth_currents: vec![magnetic::EarthCurrent {
            section: "NL1".into(),
            ..typed(60.0, 0.0)
        }],
        ..base
    });
    assert!(e.contains("is a phase conductor"), "{e}");
}

/// A cut that cannot be placed (a span that does not exist) does not stop
/// the others (owner, 2026-11-14): the good cut computed, the bad one said
/// in the notes as not computed, with why.
#[test]
fn a_failed_cut_leaves_the_others() {
    let (line, library) = example();
    let good = magnetic::CutSpec {
        at: magnetic::CutAt::Station {
            alignment: "main".into(),
            station: 150.0,
        },
        width: 20.0,
        above: 5.0,
        step: 2.0,
        label: Some("house A".into()),
    };
    let bad = magnetic::CutSpec {
        at: magnetic::CutAt::InSpan {
            from: "T1".into(),
            to: "NOWHERE".into(),
            fraction: Some(0.5),
        },
        label: Some("house B".into()),
        ..good.clone()
    };
    let s = Settings {
        currents: both(1000.0, false),
        stations: vec![mid("T1", "T2")],
        cuts: vec![bad, good],
        ..Settings::default()
    };
    let (m, _) = field_keeping(&line, &library, &s);
    assert_eq!(m.cuts.len(), 1);
    assert_eq!(m.cuts[0].label.as_deref(), Some("house A"));
    let note = m
        .notes
        .iter()
        .find(|n| n.starts_with("NOT COMPUTED"))
        .expect("the failed cut said");
    assert!(
        note.contains("1 of 2") && note.contains("house B"),
        "{note}"
    );
}
