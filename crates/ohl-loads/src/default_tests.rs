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
    let want: [Want; 9] = [
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
            "0 °C + 60 N/m ice",
            "0 °C + 60 N/m ice",
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

/// "For the 60 it should only be applied above 1000m" (owner,
/// 2026-11-15): with the default groups ([`default_case_groups`]: the own
/// list of "0 °C + 60 N/m ice" with the rule ground elevation ≥ 1000 m)
/// the example line — every structure near 500 m, one raised to 1100 m
/// and one put exactly on 1000 m for the test — gives that case to those
/// two only; every other default case (the 20 N/m ice too) to every
/// structure, exactly as without the groups. Without the groups the case
/// goes to every structure (more load: the safe side).
#[test]
fn the_60_n_m_ice_goes_only_to_structures_above_1000_m() {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut stored = Line::load(&ex().join("synthetic-line-typed.ohl.json")).unwrap();
    let ids: Vec<String> = stored.structures.iter().map(|s| s.id.clone()).collect();
    assert!(stored.structures.iter().all(|s| s.position[2] < 1000.0));
    let (high, edge) = (ids[1].clone(), ids[3].clone());
    let lift = |line: &mut Line, id: &str, z: f64| {
        let s = line.structures.iter_mut().find(|s| s.id == id).unwrap();
        let dz = z - s.position[2];
        s.position[2] = z;
        for p in &mut s.points {
            p.position[2] += dz;
        }
        for w in &mut s.wires {
            w.position[2] += dz;
        }
    };
    lift(&mut stored, &high, 1100.0);
    lift(&mut stored, &edge, 1000.0);
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::default_cases().cases;
    let cases = default_load_cases().cases;
    let loads = |line: &Line| {
        let line = line
            .resolve(&types, &ohl_structures::InsulatorLibrary::default())
            .unwrap();
        structure_loads(&line, &library, &weather, &cases, &default_budget()).unwrap()
    };
    let at = |r: &[CaseLoads], case: &str| -> std::collections::BTreeSet<String> {
        r.iter()
            .find(|c| c.case.name == case)
            .unwrap()
            .loads
            .iter()
            .filter(|l| !l.point.is_empty())
            .map(|l| l.structure.clone())
            .collect()
    };
    let without = loads(&stored);
    let every = at(&without, "0 °C + 20 N/m ice");
    assert!(every.len() > 2, "{every:?}");
    // Without the groups: the 60 N/m case at every structure.
    assert_eq!(at(&without, DEFAULT_HIGH_ICE_CASE), every);
    let mut grouped = stored.clone();
    let (added, kept) = add_default_case_groups(&mut grouped);
    assert_eq!(
        (added, kept),
        (vec![DEFAULT_HIGH_ICE_CASE.to_owned()], Vec::<String>::new())
    );
    assert_eq!(grouped.groups_problem(), None);
    let with = loads(&grouped);
    let high_ones: std::collections::BTreeSet<String> = [high.clone(), edge.clone()]
        .into_iter()
        .filter(|id| every.contains(id))
        .collect();
    assert!(!high_ones.is_empty());
    assert_eq!(at(&with, DEFAULT_HIGH_ICE_CASE), high_ones);
    // Every other case: unchanged, at every structure.
    for c in cases.iter().filter(|c| c.name != DEFAULT_HIGH_ICE_CASE) {
        assert_eq!(at(&with, &c.name), at(&without, &c.name), "{}", c.name);
    }
    // The 60 N/m loads at a high structure: those of the run without the
    // groups.
    let pick = |r: &[CaseLoads]| -> Vec<[f64; 3]> {
        r.iter()
            .find(|c| c.case.name == DEFAULT_HIGH_ICE_CASE)
            .unwrap()
            .loads
            .iter()
            .filter(|l| l.structure == high)
            .map(|l| l.factored)
            .collect()
    };
    assert_eq!(pick(&with), pick(&without));
    // Added again: kept, nothing doubled.
    let (added, kept) = add_default_case_groups(&mut grouped);
    assert!(added.is_empty() && kept == [DEFAULT_HIGH_ICE_CASE]);
    assert_eq!(grouped.structure_groups.len(), 1);
}
