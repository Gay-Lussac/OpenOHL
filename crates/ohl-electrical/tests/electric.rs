// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The electric field of the synthetic double circuit
//! (`examples/electrical`): symmetry, the voltage given, 2D against 3D,
//! a cut and a volume.

use std::path::Path;

use ohl_core::units::Temperature;
use ohl_electrical::cross_section::Sources;
use ohl_electrical::electric::{self, CircuitVoltage, Settings};
use ohl_electrical::magnetic::{self, Model, StationSpec};
use ohl_line::Line;

fn example() -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let line =
        Line::from_json(&std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap())
            .unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    (line, library)
}

fn field(settings: &Settings) -> electric::ElectricField {
    let (line, library) = example();
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
    electric::run(&src, settings, None).unwrap()
}

fn mid(from: &str, to: &str) -> StationSpec {
    StationSpec {
        from: from.into(),
        to: to.into(),
        fraction: Some(0.5),
    }
}

/// Symmetric line, symmetric profile; some kV/m under a line of a few
/// hundred kV at 20-odd m; the voltage not given is said.
#[test]
fn a_symmetric_profile() {
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let m = field(&s);
    let p = &m.profiles[0];
    for k in 0..p.points.len() / 2 {
        let (l, r) = (p.points[k].e_kv_m, p.points[p.points.len() - 1 - k].e_kv_m);
        assert!((l - r).abs() < 1e-6 * l.max(1e-6), "{l} {r}");
    }
    assert!(p.max_kv_m > 0.05 && p.max_kv_m < 20.0, "{}", p.max_kv_m);
    assert!(
        m.notes.iter().any(|n| n.contains("nominal")),
        "{:?}",
        m.notes
    );
    // The earth wire at zero voltage carries a charge too (induced).
    assert!(m.charges.iter().any(|c| c.kv == 0.0 && c.nc_per_m > 0.0));
}

/// The field goes with the voltage: twice the voltage, twice the field.
#[test]
fn the_field_goes_with_the_voltage() {
    let mut s = Settings {
        stations: vec![mid("T1", "T2")],
        voltages: vec![CircuitVoltage {
            circuit: "*".into(),
            kv: 200.0,
        }],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let a = field(&s).profiles[0].max_kv_m;
    s.voltages[0].kv = 400.0;
    let b = field(&s).profiles[0].max_kv_m;
    assert!((b - 2.0 * a).abs() < 1e-9 * b, "{a} {b}");
}

/// At mid-span of a straight line the 3D field is near the 2D one (the
/// wires sag towards the station from both sides: within a few per cent).
#[test]
fn two_d_against_three_d() {
    let mut s = Settings {
        stations: vec![mid("T1", "T2")],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let d3 = field(&s).profiles[0].max_kv_m;
    s.model = Model::TwoD;
    let d2 = field(&s).profiles[0].max_kv_m;
    assert!((d3 - d2).abs() < 0.1 * d2, "3D {d3} 2D {d2}");
}

/// A cut and a volume (the views the magnetic field has).
#[test]
fn a_cut_and_a_volume() {
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        cuts: vec![magnetic::CutSpec {
            from: "T1".into(),
            to: "T2".into(),
            fraction: Some(0.5),
            width: 30.0,
            above: 5.0,
            step: 1.0,
        }],
        volumes: vec![magnetic::VolumeSpec {
            from: "T1".into(),
            to: "T2".into(),
            width: 30.0,
            above: 5.0,
            step: 2.0,
        }],
        levels: vec![1.0],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let m = field(&s);
    assert!(!m.cuts[0].values.isolines(1.0).is_empty());
    let v = &m.volumes[0].values;
    assert!(!v.surfaces[0].1.triangles.is_empty());
    let d = v.deviation[0].unwrap();
    // (A 2 m lattice near 110 kV conductors: some per cent, said.)
    assert!(d.median < 0.02 && d.p90 < 0.15, "{d:?}");
}

/// The tiled conducting floor (the reference) against the plane under
/// each point on the example's evenly sloping ground, where the plane is
/// exact: they agree within the floor's own error (its tiles and its end).
#[test]
fn the_tiled_floor_on_even_ground() {
    let mut s = Settings {
        stations: vec![mid("T1", "T2")],
        width: 30.0,
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let t0 = std::time::Instant::now();
    let plane = field(&s);
    // (A floor smaller than the default: the test stays quick.)
    s.ground_model = electric::GroundModel::Tiled(electric::TiledFloor {
        fine: 2.0,
        coarse: 8.0,
        fine_along: 16.0,
        fine_margin: 4.0,
        reach: 96.0,
    });
    let tiled = field(&s);
    let (a, b) = (&plane.profiles[0], &tiled.profiles[0]);
    let worst = a
        .points
        .iter()
        .zip(&b.points)
        .map(|(x, y)| (x.e_kv_m - y.e_kv_m).abs() / a.max_kv_m)
        .fold(0.0, f64::max);
    eprintln!(
        "plane max {} tiled max {} worst {worst} tiles {:?} departure {} in {:?}",
        a.max_kv_m,
        b.max_kv_m,
        b.tiles,
        a.ground_departure,
        t0.elapsed()
    );
    assert!(a.ground_departure < 0.05, "{}", a.ground_departure);
    assert!(worst < 0.03, "{worst}");
}

/// The tiled floor is the default ground (owner, 2026-10-29: "keep the
/// tiled implementation"); given points near one another share a floor
/// and agree with the profile through them; a cut on it.
#[test]
fn the_tiled_floor_is_the_default() {
    assert!(matches!(
        Settings::default().ground_model,
        electric::GroundModel::Tiled(_)
    ));
    let small = electric::GroundModel::Tiled(electric::TiledFloor {
        fine: 2.0,
        coarse: 8.0,
        fine_along: 16.0,
        fine_margin: 4.0,
        reach: 96.0,
    });
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        width: 20.0,
        ground_model: small,
        ..Settings::default()
    };
    let m = field(&s);
    let p = &m.profiles[0];
    assert!(p.tiles.is_some());
    let q = &p.points[25];
    let s2 = Settings {
        points: vec![magnetic::GivenPoint {
            name: "q".into(),
            x: q.x,
            y: q.y,
            z: Some(q.z),
            height: None,
        }],
        cuts: vec![magnetic::CutSpec {
            from: "T1".into(),
            to: "T2".into(),
            fraction: Some(0.5),
            width: 20.0,
            above: 5.0,
            step: 2.0,
        }],
        ..s.clone()
    };
    let m2 = field(&s2);
    // The point's own floor (around it, not the profile's station) gives
    // about the profile's value.
    let (a, b) = (m2.points[0].e_kv_m, q.e_kv_m);
    assert!((a - b).abs() < 0.03 * p.max_kv_m, "{a} {b}");
    assert!(m2.cuts[0].values.values.iter().any(|v| *v > 0.0));
}

/// A volume and a cut on the tiled floor (owner, 2026-10-29: coarser
/// tiles by default, a parameter): on the example's evenly sloping ground,
/// where the plane under each point is exact, the volume's surface lies on
/// its level by the exact field within the coarse tiles' error.
#[test]
fn a_volume_on_the_tiled_floor() {
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        volumes: vec![magnetic::VolumeSpec {
            from: "T1".into(),
            to: "T2".into(),
            width: 20.0,
            above: 5.0,
            step: 2.0,
        }],
        levels: vec![1.0],
        // (16 m tiles and a small floor for the profile: the test stays
        // quick.)
        volume_tile: 16.0,
        ground_model: electric::GroundModel::Tiled(electric::TiledFloor {
            fine: 4.0,
            coarse: 16.0,
            fine_along: 8.0,
            fine_margin: 4.0,
            reach: 64.0,
        }),
        ..Settings::default()
    };
    let (line, library) = example();
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
    let plane = Settings {
        ground_model: electric::GroundModel::Plane,
        stations: s.stations.clone(),
        ..Settings::default()
    };
    let (m, charges) = electric::run_keeping(&src, &s, None).unwrap();
    assert!(
        m.notes.iter().any(|n| n.contains("tiled floor")),
        "{:?}",
        m.notes
    );
    let charges = charges.unwrap();
    assert!(!m.volumes[0].values.surfaces[0].1.triangles.is_empty());
    // The volume's floor against the plane (exact here), at points from
    // 1 m above the ground up, across the span's middle.
    let ground = electric::feet_ground(&line);
    let at = |id: &str| {
        let p = line.structure(id).unwrap().position;
        [p[0], p[1], p[2]]
    };
    let path = ohl_electrical::field_views::Path::new(vec![at("T1"), at("T2")]).unwrap();
    let floor = electric::Floor::under(&charges, &path, 20.0, 16.0, &ground).unwrap();
    let mid = path.place(0.5 * path.length(), 0.0);
    let mut worst: f64 = 0.0;
    let mut top: f64 = 0.0;
    for k in -10..=10 {
        for h in [1.0, 3.0, 8.0] {
            let (x, y) = (mid[0], mid[1] + 2.0 * k as f64);
            let p = [x, y, ground(x, y) + h];
            let exact = charges.field(
                p,
                &electric::GroundPlane::fitted(&ground, x, y, plane.ground_radius),
            );
            top = top.max(exact);
            worst = worst.max((floor.field(p, &charges) - exact).abs());
        }
    }
    // (Measured 2026-10-29: 16 m tiles 8.3 % of the largest value, 8 m
    // 2.8 %, 4 m 0.16 %.)
    assert!(worst < 0.10 * top, "{} of {}", worst, top);
}

/// The 2D profile from its own cross-section's charges (owner,
/// 2026-10-31): the example double circuit at the middle of T1–T2 against
/// hvlbuzz 2.0 on the same cross-section
/// (benchmarks/electrical/hvlbuzz/D-example-double-circuit-T1-T2.json, its
/// profile beside it): within 0.2 % at every point (from the span's mean
/// cross-section it was 3.6 % low at the axis). 3D differs by the 2D/3D
/// methods (the wires curve up away from the middle, the next span):
/// its largest value within 2 % of hvlbuzz's.
#[test]
fn the_2d_profile_matches_hvlbuzz_on_its_cross_section() {
    let csv = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/electrical/hvlbuzz/D-example-double-circuit-T1-T2_profile.csv"),
    )
    .unwrap();
    let hvl: Vec<(f64, f64)> = csv
        .lines()
        .skip(1)
        .map(|l| {
            let mut f = l.split(';').map(|v| v.trim().parse::<f64>().unwrap());
            (f.next().unwrap(), f.next().unwrap() / 1e3)
        })
        .collect();
    let at = |model| Settings {
        model,
        stations: vec![mid("T1", "T2")],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let two = field(&at(Model::TwoD));
    let p = &two.profiles[0];
    assert_eq!(p.points.len(), hvl.len());
    for (q, (s, e)) in p.points.iter().zip(&hvl) {
        assert!((q.offset - s).abs() < 1e-9);
        assert!(
            (q.e_kv_m - e).abs() <= 2e-3 * e,
            "at {s} m: {} against {e}",
            q.e_kv_m
        );
    }
    let largest = hvl.iter().map(|h| h.1).fold(0.0, f64::max);
    let three = field(&at(Model::ThreeD));
    let m = three.profiles[0].max_kv_m;
    assert!(
        (m - largest).abs() <= 0.02 * largest,
        "3D {m} against {largest}"
    );
}
