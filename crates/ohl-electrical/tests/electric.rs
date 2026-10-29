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
