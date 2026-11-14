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
    field_of(&line, &library, settings)
}

fn field_of(
    line: &Line,
    library: &ohl_cables::Library,
    settings: &Settings,
) -> electric::ElectricField {
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

/// The voltage of the field (owner, 2026-11-11): a system's highest
/// operating voltage where the line gives one — the field in proportion,
/// here 1.1 times the nominal's — else the nominal, said as not on the
/// safe side for those circuits only; a voltage given for the run before
/// both. The result always says which voltage each circuit was computed
/// at.
#[test]
fn the_highest_operating_voltage_is_used_where_given() {
    let (mut line, library) = example();
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let unsafe_note = |m: &electric::ElectricField| -> Option<String> {
        m.notes
            .iter()
            .find(|n| n.starts_with("NOT ON THE SAFE SIDE: nominal voltage used for "))
            .cloned()
    };
    let used = |m: &electric::ElectricField| -> String {
        m.notes
            .iter()
            .find(|n| n.starts_with("voltages used, between phases: "))
            .cloned()
            .expect("the voltages used are always said")
    };
    // None given: the nominal, not on the safe side, for every circuit.
    let nominal = field_of(&line, &library, &s);
    let a = nominal.profiles[0].max_kv_m;
    let note = unsafe_note(&nominal).expect("said");
    assert!(
        note.ends_with("the highest operating voltage gives a higher field (give it with the system, or a voltage for this run)"),
        "{note}"
    );
    assert!(used(&nominal).contains("(its system's nominal voltage)"));
    assert!(!used(&nominal).contains("highest"));
    // One system with its highest operating voltage: the others still said.
    let systems = line.systems.len();
    assert!(systems >= 2, "the example is a double circuit");
    line.systems[0].highest_voltage_kv = Some(1.1 * line.systems[0].voltage_kv);
    let mixed = field_of(&line, &library, &s);
    let said = used(&mixed);
    assert!(
        said.contains("(its system's highest operating voltage)")
            && said.contains("(its system's nominal voltage)"),
        "{said}"
    );
    let partly = unsafe_note(&mixed).expect("the other systems at their nominal");
    assert!(partly.len() < note.len(), "{partly} / {note}");
    // Every system: 1.1 times the field, nothing unsafe to say.
    for sys in &mut line.systems {
        sys.highest_voltage_kv = Some(1.1 * sys.voltage_kv);
    }
    line.validate().unwrap();
    let highest = field_of(&line, &library, &s);
    let b = highest.profiles[0].max_kv_m;
    assert!((b - 1.1 * a).abs() < 1e-9 * b, "{a} {b}");
    assert_eq!(unsafe_note(&highest), None);
    assert!(!used(&highest).contains("nominal"));
    // The charges' voltages to earth follow.
    let kv = |m: &electric::ElectricField| m.charges.iter().map(|c| c.kv).fold(0.0, f64::max);
    assert!((kv(&highest) - 1.1 * kv(&nominal)).abs() < 1e-9 * kv(&highest));
    // A voltage given for the run comes first.
    let given = field_of(
        &line,
        &library,
        &Settings {
            voltages: vec![CircuitVoltage {
                circuit: "*".into(),
                kv: 200.0,
            }],
            ..s.clone()
        },
    );
    assert_eq!(unsafe_note(&given), None);
    assert!(used(&given).contains("200 kV (given for this run)"));
    assert!(!used(&given).contains("system's"));
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
            at: magnetic::CutAt::Station {
                alignment: "main".into(),
                station: 150.0,
            },
            width: 30.0,
            above: 5.0,
            step: 1.0,
            ..Default::default()
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
            at: magnetic::CutAt::Station {
                alignment: "main".into(),
                station: 150.0,
            },
            width: 20.0,
            above: 5.0,
            step: 2.0,
            ..Default::default()
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
    let floor = electric::Floor::under(
        &charges,
        &path,
        20.0,
        16.0,
        electric::VOLUME_MARGIN,
        &ground,
    )
    .unwrap();
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

/// A volume's floor in parts (owner, 2026-11-01: "split only when it
/// doesn't fit, margin by distance"): along the example's two spans
/// T1–T3, a floor per part of 100 m over the ground within the width and
/// 100 m of it against one floor under the whole run and 100 m beyond,
/// at points from 1 m above the ground up, across the run every 10 m,
/// each point taking its part's floor — as the volumes do: as close to
/// the exact field (level ground) as the one floor.
#[test]
fn a_floor_in_parts_matches_one_floor() {
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
    let s = Settings {
        stations: vec![mid("T1", "T2")],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let (_, charges) = electric::run_keeping(&src, &s, None).unwrap();
    let charges = charges.unwrap();
    let ground = electric::feet_ground(&line);
    let at = |id: &str| {
        let p = line.structure(id).unwrap().position;
        [p[0], p[1], p[2]]
    };
    let path = ohl_electrical::field_views::Path::new(vec![at("T1"), at("T2"), at("T3")]).unwrap();
    // (16 m tiles: the test stays quick.)
    let (width, tile) = (20.0, 16.0);
    let len = path.length();
    let one = electric::Floor::under(&charges, &path, width, tile, 100.0, &ground).unwrap();
    let parts_of = |margin: f64| {
        let parts = (len / margin).ceil() as usize;
        let floors: Vec<electric::Floor> = (0..parts)
            .map(|k| {
                let (t0, t1) = (
                    len * k as f64 / parts as f64,
                    len * (k + 1) as f64 / parts as f64,
                );
                electric::Floor::around(
                    &charges,
                    &path.plan_between(t0, t1),
                    width + margin,
                    tile,
                    &ground,
                )
                .unwrap()
            })
            .collect();
        (parts, floors)
    };
    // Against the exact field (level ground here: the plane under each
    // point is exact), each floor's largest difference over the largest
    // value.
    let exact = |p: [f64; 3]| {
        charges.field(
            p,
            &electric::GroundPlane::fitted(&ground, p[0], p[1], s.ground_radius),
        )
    };
    let error = |field: &dyn Fn([f64; 2], [f64; 3]) -> f64| {
        let (mut worst, mut top): (f64, f64) = (0.0, 0.0);
        let mut t = 5.0;
        while t < len {
            for k in -2..=2 {
                for h in [1.0, 3.0, 8.0] {
                    let xy = path.place(t, 10.0 * k as f64);
                    let p = [xy[0], xy[1], ground(xy[0], xy[1]) + h];
                    let e = exact(p);
                    top = top.max(e);
                    worst = worst.max((field(xy, p) - e).abs());
                }
            }
            t += 10.0;
        }
        worst / top
    };
    let compare = |margin: f64| {
        let (parts, floors) = parts_of(margin);
        let d = error(&|xy, p| {
            let part = ((path.station(xy) / len * parts as f64) as usize).min(parts - 1);
            floors[part].field(p, &charges)
        });
        (parts, d)
    };
    let d_one = error(&|_, p| one.field(p, &charges));
    let (n100, d100) = compare(100.0);
    let (n200, d200) = compare(200.0);
    // Measured 2026-11-01 against the exact field, the largest difference
    // over the largest value: 16 m tiles one floor 9.9 %, 6 parts with
    // 100 m 9.5 %, 3 parts with 200 m 8.9 %; 8 m tiles 3.1 %, 2.8 %,
    // 3.4 % — the tiles' size, not the parts, makes the difference.
    assert_eq!((n100, n200), (6, 3));
    assert!(
        d100 <= d_one + 0.005,
        "parts {d100} against one floor {d_one}"
    );
    assert!(
        d200 <= d_one + 0.005,
        "parts {d200} against one floor {d_one}"
    );
}

/// One floor while it fits in 2 GB (as before), parts as long as the
/// margin beyond: the owner's refused case (17 936 tiles, 5.1 GB) in
/// parts of a few thousand tiles.
#[test]
fn a_volume_is_floored_in_parts_only_when_one_floor_does_not_fit() {
    let one = electric::volume_parts(600.0, 50.0, 8.0, 100.0);
    assert_eq!(
        one,
        (1, electric::Floor::tiles_under(600.0, 50.0, 8.0, 100.0))
    );
    let long = 3600.0;
    let tiles = electric::Floor::tiles_under(long, 50.0, 8.0, 100.0);
    assert!(
        electric::floor_bytes(tiles) > electric::FLOOR_MEMORY,
        "{tiles}"
    );
    let (parts, each) = electric::volume_parts(long, 50.0, 8.0, 100.0);
    assert_eq!(parts, 36);
    assert!(each < 2500, "{each}");
    assert!(electric::floor_bytes(each) < 0.1e9);
}

/// A cut that cannot be placed does not stop the others (owner,
/// 2026-11-14): the good cut computed, the other said as not computed.
#[test]
fn a_failed_cut_leaves_the_others() {
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
        stations: vec![mid("T1", "T2")],
        cuts: vec![good, bad],
        ground_model: electric::GroundModel::Plane,
        ..Settings::default()
    };
    let m = field(&s);
    assert_eq!(m.cuts.len(), 1);
    assert!(
        m.notes
            .iter()
            .any(|n| n.starts_with("NOT COMPUTED") && n.contains("house B")),
        "{:?}",
        m.notes
    );
}
