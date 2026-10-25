// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Safety volumes chosen by electrical system and grouped per system
//! (2026-10-21).

use super::*;
use crate::checks::Wire;
use ohl_line::electrical::{Potential, SystemKind};

/// A straight level wire 10 m long at height 30 m, offset `y`.
fn wire(section: &str, circuit: &str, y: f64, kv: Option<f64>) -> Wire {
    Wire {
        section: section.into(),
        span: 0,
        circuit: circuit.into(),
        phase: "L1".into(),
        potential: kv.map(|kv| Potential {
            nominal_kv: kv,
            kind: SystemKind::Ac,
            frequency_hz: None,
            to_earth: [kv / 3f64.sqrt(), 0.0],
        }),
        bundle: vec![],
        points: (0..=10).map(|k| [k as f64, y, 30.0]).collect(),
        ends: vec![],
        surface_radius: 0.0,
        polyline_error: 0.0,
    }
}

/// Five short wires 30 m apart — circuits A and B of system X (220 kV),
/// circuit C of system Y (380 kV), two earth wires E and F (no system).
/// The systems taken pick exactly their wires (X: the sections of A and
/// B, the earth wires left out; "(no system)": the earth wires only); per
/// system one volume per system (X holding A and B, named after it), the
/// earth wires one volume "(no system)" together (2026-10-21; before: one
/// per circuit); each wire keeps the distance of its own voltage (table
/// 3 m up to 220 kV, 5 m up to 380 kV, invented values; the earth wires
/// at 0 kV: 3 m).
#[test]
fn chosen_and_grouped_by_system() {
    let positions = Positions::of_wires(
        "still",
        vec![
            wire("SA", "A", 0.0, Some(220.0)),
            wire("SB", "B", 30.0, Some(220.0)),
            wire("SC", "C", 60.0, Some(380.0)),
            wire("SE", "E", 90.0, None),
            wire("SF", "F", 120.0, None),
        ],
    );
    let of = |id: &str, name: &str| SystemOf {
        id: id.into(),
        name: name.into(),
    };
    let system_of: std::collections::BTreeMap<String, SystemOf> = [
        ("SA".to_owned(), of("X", "System X")),
        ("SB".to_owned(), of("X", "System X")),
        ("SC".to_owned(), of("Y", "System Y")),
    ]
    .into_iter()
    .collect();
    let base = SafetyParams {
        distance: Requirement::ByVoltage(vec![(220.0, 3.0), (380.0, 5.0)]),
        tolerance: 0.2,
        system_of,
        ..SafetyParams::default()
    };
    // System X, per wire: exactly the wires of A and B.
    let v = build(
        &positions,
        &SafetyParams {
            systems: vec!["X".into()],
            ..base.clone()
        },
        &Budget::new(),
    )
    .unwrap();
    let mut sections: Vec<&str> = v
        .volumes
        .iter()
        .flat_map(|s| s.sections.iter().map(String::as_str))
        .collect();
    sections.sort_unstable();
    assert_eq!(sections, ["SA", "SB"]);
    assert_eq!(v.volumes.len(), 2);
    // Per system, every wire: X, Y and the earth wires together.
    let v = build(
        &positions,
        &SafetyParams {
            grouping: Grouping::System,
            ..base.clone()
        },
        &Budget::new(),
    )
    .unwrap();
    let names: Vec<&str> = v.volumes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["System X", "System Y", NO_SYSTEM]);
    assert_eq!(v.volumes[0].sections, ["SA", "SB"]);
    assert_eq!(v.volumes[0].circuit, "A, B");
    assert_eq!(v.volumes[2].sections, ["SE", "SF"]);
    assert_eq!(v.volumes[2].circuit, "E, F");
    assert_eq!(v.volumes[2].kv, None);
    let d: Vec<f64> = v.volumes.iter().map(|s| s.distance).collect();
    assert_eq!(d, [3.0, 5.0, 3.0]);
    for s in &v.volumes {
        assert_eq!(s.mesh.open_edges(), 0, "{}", s.name);
    }
    // Per system and system X: one volume.
    let v = build(
        &positions,
        &SafetyParams {
            grouping: Grouping::System,
            systems: vec!["X".into()],
            ..base.clone()
        },
        &Budget::new(),
    )
    .unwrap();
    assert_eq!(v.volumes.len(), 1);
    // The wires without a system, ticked like a system: the earth wires
    // only; with X too, the five but C.
    let picked = |systems: &[&str]| {
        let v = build(
            &positions,
            &SafetyParams {
                systems: systems.iter().map(|s| (*s).to_owned()).collect(),
                ..base.clone()
            },
            &Budget::new(),
        )
        .unwrap();
        let mut s: Vec<String> = v.volumes.iter().flat_map(|x| x.sections.clone()).collect();
        s.sort();
        s
    };
    assert_eq!(picked(&[NO_SYSTEM]), ["SE", "SF"]);
    assert_eq!(picked(&[NO_SYSTEM, "X"]), ["SA", "SB", "SE", "SF"]);
    // A system with no wire: said so.
    let e = build(
        &positions,
        &SafetyParams {
            systems: vec!["Z".into()],
            ..base
        },
        &Budget::new(),
    )
    .unwrap_err();
    assert!(e.contains("systems Z"), "{e}");
    assert_eq!(Grouping::parse("system"), Ok(Grouping::System));
}

/// The example line's systems: every section of a system mapped to it,
/// the jumper to its `from` section's; an unknown system refused by
/// `run` before anything is solved.
#[test]
fn the_example_line_s_system_map() {
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = ohl_line::Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json"))
        .unwrap();
    let map = system_map(&line);
    let phases = line.section_phases().unwrap();
    assert!(phases.values().any(Option::is_some));
    for (id, a) in &phases {
        assert_eq!(map.contains_key(id), a.is_some(), "{id}");
    }
    for j in &line.jumpers {
        let from = line.jumper_section(j).unwrap();
        assert_eq!(
            map.get(&format!("jumper:{}", j.id)),
            map.get(&from.id),
            "{}",
            j.id
        );
    }
    let e = run(
        &line,
        &ohl_cables::Library::load(&root.join("libraries/cables")).unwrap(),
        &ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&["high wind".into()], &[])
            .unwrap(),
        WindMode::None,
        WindSteps::default(),
        &SafetyParams {
            distance: Requirement::Fixed(1.0),
            systems: vec!["no such system".into()],
            ..SafetyParams::default()
        },
        &Budget::new(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(e.contains("no system"), "{e}");
    // `none` and "(no system)" name the wires without a system (the line
    // has no system called none).
    assert!(no_system_key(&line, "none") && no_system_key(&line, "None"));
    assert!(no_system_key(&line, NO_SYSTEM));
    assert!(!no_system_key(&line, &line.systems[0].id));
    let mut named = line.clone();
    named.systems[0].name = "none".into();
    assert!(!no_system_key(&named, "none"), "a system called none wins");
    // A span the section does not have: refused.
    let section = &line.sections[0];
    let e = run(
        &line,
        &ohl_cables::Library::load(&root.join("libraries/cables")).unwrap(),
        &ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&["high wind".into()], &[])
            .unwrap(),
        WindMode::None,
        WindSteps::default(),
        &SafetyParams {
            distance: Requirement::Fixed(1.0),
            spans: vec![(section.id.clone(), section.points.len())],
            ..SafetyParams::default()
        },
        &Budget::new(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(e.contains("no span"), "{e}");
}

/// Chosen by span (2026-10-21): a section of three 10 m spans along x
/// (ends T1…T4) and a jumper hanging between the ends of spans 1 and 2
/// at T2. Span 1 alone: one position, the mesh no farther along than
/// its end plus the distance (2 m) and the allowance; spans 1 and 2: the
/// jumper comes along (both its ends taken); spans 1 and 3: it does not.
#[test]
fn chosen_by_span() {
    let span = |k: usize, ends: [&str; 2]| {
        let mut w = wire("S", "A", 0.0, Some(110.0));
        w.span = k;
        w.points = (0..=10)
            .map(|i| [10.0 * k as f64 + i as f64, 0.0, 30.0])
            .collect();
        w.ends = ends.iter().map(|e| (*e).to_owned()).collect();
        w
    };
    let mut jumper = wire("jumper:J", "A", 0.0, Some(110.0));
    jumper.points = vec![[10.0, 0.0, 30.0], [10.0, 0.5, 29.0], [10.0, 0.0, 30.0]];
    jumper.ends = vec!["T2:1:L1".into(), "T2:2:L1".into()];
    let positions = Positions::of_wires(
        "still",
        vec![
            span(0, ["T1:1:L1", "T2:1:L1"]),
            span(1, ["T2:2:L1", "T3:1:L1"]),
            span(2, ["T3:1:L1", "T4:1:L1"]),
            jumper,
        ],
    );
    let with = |spans: &[usize]| {
        build(
            &positions,
            &SafetyParams {
                distance: Requirement::Fixed(2.0),
                tolerance: 0.2,
                spans: spans.iter().map(|k| ("S".to_owned(), *k)).collect(),
                ..SafetyParams::default()
            },
            &Budget::new(),
        )
        .unwrap()
    };
    let v = with(&[0]);
    assert_eq!(v.volumes.len(), 1);
    assert_eq!(v.volumes[0].positions, 1);
    let reach = v.volumes[0]
        .mesh
        .vertices
        .iter()
        .map(|p| p[0])
        .fold(f64::MIN, f64::max);
    assert!(
        reach <= 10.0 + 2.0 + v.volumes[0].allowance + 1e-9,
        "{reach}"
    );
    assert!(reach > 11.5, "{reach}");
    assert_eq!(
        with(&[0, 1]).volumes[0].positions,
        3,
        "the jumper comes along"
    );
    assert_eq!(with(&[0, 2]).volumes[0].positions, 2, "no jumper");
    // As typed: N from 1.
    assert_eq!(parse_span("S:1"), Ok(("S".into(), 0)));
    assert_eq!(parse_span("a:b:3"), Ok(("a:b".into(), 2)));
    assert!(parse_span("S:0").is_err() && parse_span("S").is_err());
}
