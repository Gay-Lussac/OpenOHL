// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The default load cases (owner, 2026-11-15: "these values are arbitrary
//! but needed to check the codes"): exactly the owner's values, and
//! computed on the synthetic example line with the default weather cases.

use std::path::Path;

use super::*;

fn ex() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

/// The default load cases read, check, name only default weather cases,
/// and carry exactly the owner's factors.
#[test]
fn the_default_load_cases_are_the_owners_values() {
    let set = default_load_cases();
    let weather = ohl_weather::default_cases();
    // (name, weather, V, T, L, wind from, body pressure Pa, body factor,
    // the case's wind direction)
    type Want = (
        &'static str,
        &'static str,
        [f64; 3],
        Option<ohl_weather::WindSide>,
        Option<f64>,
        Option<f64>,
        Option<WindDirection>,
    );
    use ohl_weather::WindSide::{Left, Right};
    let want: [Want; 8] = [
        ("-20 °C", "-20 °C", [1.0; 3], None, None, None, None),
        ("10 °C", "10 °C", [1.0; 3], None, None, None, None),
        ("40 °C", "40 °C", [1.0; 3], None, None, None, None),
        (
            "10 °C + 300 Pa wind left",
            "10 °C + 300 Pa wind",
            [1.0, 1.5, 1.0],
            Some(Left),
            None,
            Some(1.5),
            None,
        ),
        (
            "10 °C + 300 Pa wind right",
            "10 °C + 300 Pa wind",
            [1.0, 1.5, 1.0],
            Some(Right),
            None,
            Some(1.5),
            None,
        ),
        (
            "0 °C + 20 N/m ice",
            "0 °C + 20 N/m ice",
            [1.3, 1.0, 1.0],
            None,
            None,
            None,
            None,
        ),
        (
            "10 °C + 300 Pa wind ahead",
            "10 °C + 300 Pa wind",
            [1.0, 1.5, 1.5],
            None,
            None,
            Some(1.5),
            Some(WindDirection::Angle(0.0)),
        ),
        (
            "10 °C + 300 Pa wind back",
            "10 °C + 300 Pa wind",
            [1.0, 1.5, 1.5],
            None,
            None,
            Some(1.5),
            Some(WindDirection::Angle(180.0)),
        ),
    ];
    assert_eq!(set.cases.len(), want.len());
    for (c, (name, w, [v, t, l], side, pressure, factor, direction)) in set.cases.iter().zip(want) {
        assert_eq!(c.name, name);
        assert_eq!(c.weather, w, "{name}");
        assert!(
            weather.cases.iter().any(|x| x.name == c.weather),
            "{name}: its weather case is a default one"
        );
        assert_eq!(c.condition, WireCondition::Initial, "{name}");
        assert_eq!(
            (c.vertical, c.transverse, c.longitudinal),
            (v, t, l),
            "{name}"
        );
        assert_eq!(c.strength, 1.0, "{name}");
        assert_eq!(c.wind_from, side, "{name}");
        let s = c.structure.unwrap_or_default();
        assert_eq!(s.wind_pressure, pressure, "{name}");
        assert_eq!(s.wind_factor, factor, "{name}");
        assert_eq!(c.wind_direction, direction, "{name}");
        assert_eq!(c.on_structure.direction, None, "{name}");
        assert_eq!(c.on_structure.ice, None, "{name}");
        assert!(
            !c.use_local && !c.has_broken() && c.removed.is_empty(),
            "{name}"
        );
        assert!(
            c.description.starts_with(
                "default case, arbitrary values for checking (not from a design code)"
            ),
            "{name}"
        );
        assert_eq!(c.tags, ["default"], "{name}");
    }
    // Written back by the program's writer, the same cases.
    let again = LoadCases::parse(&set.to_toml()).unwrap();
    assert_eq!(again.cases, set.cases);
    assert_eq!(set.written_version(), 7);
}

/// Every default load case computes on the synthetic example line with
/// the default weather cases: loads at the structures in each; the wind
/// across factored 1.5 (wires and bodies), the ice's vertical 1.3; the
/// wind along the line (2026-11-15: a real wind direction) a longitudinal
/// load on the bodies and on the wires' strings, the wires' across loads
/// far below the wind across, said as one direction in the notes.
#[test]
fn the_default_load_cases_compute_on_the_example_line() {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::default_cases().cases;
    let cases = default_load_cases().cases;
    let mut r = structure_loads(&line, &library, &weather, &cases, &default_budget()).unwrap();
    add_structure_wind(&mut r, &line, &types, &weather);
    let by = |name: &str| r.iter().find(|c| c.case.name == name).unwrap();
    for c in &r {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
        assert!(
            c.loads.iter().any(|l| !l.point.is_empty()),
            "{}: loads at the structures",
            c.case.name
        );
    }
    let bands = |c: &CaseLoads| -> Vec<AttachmentLoad> {
        c.loads
            .iter()
            .filter(|l| l.point.is_empty() && l.label.starts_with("wind on the structure"))
            .cloned()
            .collect()
    };
    let wires = |c: &CaseLoads| -> Vec<AttachmentLoad> {
        c.loads
            .iter()
            .filter(|l| !l.point.is_empty())
            .cloned()
            .collect()
    };
    // The still cases: no wind on the bodies.
    for name in ["-20 °C", "10 °C", "40 °C", "0 °C + 20 N/m ice"] {
        assert!(bands(by(name)).is_empty(), "{name}");
    }
    // The wind across: the bodies' bands across the line, × 1.5; the
    // wires' transverse × 1.5; left and right opposite.
    for (name, sign) in [
        ("10 °C + 300 Pa wind left", 1.0),
        ("10 °C + 300 Pa wind right", -1.0),
    ] {
        let c = by(name);
        let b = bands(c);
        assert!(!b.is_empty(), "{name}: the types' wind bands");
        for l in &b {
            assert!(
                l.local[0] * sign > 0.0 && l.local[1].abs() < 1e-9,
                "{name}: {l:?}"
            );
            assert!((l.factored[0] - 1.5 * l.local[0]).abs() < 1e-6, "{name}");
        }
        for l in wires(c) {
            assert!((l.factored[0] - 1.5 * l.local[0]).abs() < 1e-6, "{name}");
            assert!((l.factored[1] - l.local[1]).abs() < 1e-6, "{name}");
            assert!((l.factored[2] - l.local[2]).abs() < 1e-6, "{name}");
        }
    }
    // The ice: vertical × 1.3, the rest × 1.
    for l in wires(by("0 °C + 20 N/m ice")) {
        assert!((l.factored[2] - 1.3 * l.local[2]).abs() < 1e-6);
        assert!((l.factored[0] - l.local[0]).abs() < 1e-6);
    }
    // The wind along the line: the bands' force all along it (from ahead
    // towards −y, from behind towards +y), 300 Pa × 1.5; the wires' loads
    // along the line × 1.5, their across loads far below the cross wind's
    // (the spans nearly along the wind), the strings blown along the line.
    let still = wires(by("10 °C"));
    // The wind's part across: the change from still air (the pull at a
    // line angle is in both).
    let across: f64 = wires(by("10 °C + 300 Pa wind right"))
        .iter()
        .zip(&still)
        .map(|(a, s)| (a.local[0] - s.local[0]).abs())
        .sum();
    for (name, sign) in [
        ("10 °C + 300 Pa wind ahead", -1.0),
        ("10 °C + 300 Pa wind back", 1.0),
    ] {
        let c = by(name);
        assert!(c.structure_wind.has_longitudinal(), "{name}");
        assert_eq!(c.structure_wind.pressure, 300.0, "{name}");
        assert_eq!(c.structure_wind.factor, 1.5, "{name}");
        let b = bands(c);
        assert!(!b.is_empty(), "{name}: the types' wind bands");
        for l in &b {
            assert!(
                l.local[1] * sign > 0.0 && l.local[0].abs() < 1e-6 * l.local[1].abs(),
                "{name}: {l:?}"
            );
            assert!((l.factored[1] - 1.5 * l.local[1]).abs() < 1e-6, "{name}");
            assert!(l.label.contains("NOT ON THE SAFE SIDE"), "{name}");
        }
        let w = wires(c);
        assert_eq!(w.len(), still.len(), "{name}");
        for l in &w {
            assert!((l.factored[1] - 1.5 * l.local[1]).abs() < 1e-6, "{name}");
        }
        let along: f64 = w
            .iter()
            .zip(&still)
            .map(|(a, s)| a.local[1] - s.local[1])
            .sum();
        assert!(along * sign > 0.0, "{name}: the wires blown along: {along}");
        let here: f64 = w
            .iter()
            .zip(&still)
            .map(|(a, s)| (a.local[0] - s.local[0]).abs())
            .sum();
        assert!(here < 0.5 * across, "{name}: {here} vs {across} across");
        assert!(
            c.notes
                .iter()
                .any(|n| n.contains("not the across envelope")),
            "{name}: {:?}",
            c.notes
        );
    }
}
