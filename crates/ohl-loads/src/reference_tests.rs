// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The reference state of the structures (plan of 2026-11-15, phase 5b)
//! on the example line: none set is the line as before bit for bit; a
//! reference case solved stores its points' moves and draws its bodies;
//! a level-2 solve with it is the coupled solve's with those moves; any
//! edit of what it was solved from makes it stale and unused; a line
//! read again is unchecked (unused) until checked.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ohl_line::reference::{ReferenceStatus, StructureReference};
use ohl_line::solve::SectionShape;
use ohl_line::solve::flexible::CalculationLevel;

use super::reference::{check_reference, reference_bodies, solve_reference};
use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn example() -> (
    Line,
    ohl_cables::Library,
    ohl_structures::StructureLibrary,
    ohl_sections::SectionLibrary,
    Vec<WeatherCase>,
) {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    (
        line,
        library,
        types,
        ohl_sections::SectionLibrary::default(),
        weather,
    )
}

fn case<'a>(weather: &'a [WeatherCase], name: &str) -> &'a WeatherCase {
    weather.iter().find(|w| w.name == name).unwrap()
}

/// Every span's tension and every clamp of `a` and `b` within `tol`
/// (relative on the tension, m on the clamps).
fn close(
    a: &[Result<SectionShape, ohl_line::LineError>],
    b: &[Result<SectionShape, ohl_line::LineError>],
    tol: f64,
) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Ok(x), Ok(y)) => {
                x.spans.len() == y.spans.len()
                    && x.spans.iter().zip(&y.spans).all(|(p, q)| {
                        (p.horizontal_tension - q.horizontal_tension).abs()
                            <= tol * p.horizontal_tension.abs().max(1.0)
                            && (0..3).all(|c| (p.low_point[c] - q.low_point[c]).abs() <= tol)
                    })
            }
            (Err(_), Err(_)) => true,
            _ => false,
        })
}

/// The largest difference of a span's lowest point between `a` and `b`,
/// m.
fn largest_change(
    a: &[Result<SectionShape, ohl_line::LineError>],
    b: &[Result<SectionShape, ohl_line::LineError>],
) -> f64 {
    let mut most = 0.0_f64;
    for (x, y) in a.iter().zip(b) {
        let (Ok(x), Ok(y)) = (x, y) else { continue };
        for (p, q) in x.spans.iter().zip(&y.spans) {
            for c in 0..3 {
                most = most.max((p.low_point[c] - q.low_point[c]).abs());
            }
        }
    }
    most
}

/// The whole cycle on the example line in its "high wind" case.
#[test]
fn a_reference_case_solved_kept_used_and_stale() {
    let (line, library, types, sections, weather) = example();
    let budget = ohl_wirefe::default_fe_budget();
    let windy = case(&weather, "high wind");
    let everyday = case(&weather, "everyday");
    let plain_windy = ohl_line::solve::solve_line_with(&line, &library, windy, &budget);
    let plain_everyday = ohl_line::solve::solve_line_with(&line, &library, everyday, &budget);

    // A reference case set, not solved yet: nothing changes, bit for bit,
    // and the results say why.
    let mut set = line.clone();
    set.reference = Some(StructureReference::new("high wind"));
    assert_eq!(
        check_reference(&mut set, &library, &types, &sections, &weather),
        Some(ReferenceStatus::Unchecked)
    );
    assert!(set.reference_in_use().is_none());
    assert_eq!(
        format!("{plain_windy:?}"),
        format!(
            "{:?}",
            ohl_line::solve::solve_line_with(&set, &library, windy, &budget)
        )
    );
    assert!(set.reference_note().unwrap().contains("not solved yet"));

    // Solved: the flexible structures' points moved and kept, the others
    // listed rigid, their bodies deflected.
    let started = Instant::now();
    let solved = solve_reference(
        &set,
        &library,
        &types,
        &sections,
        &weather,
        "high wind",
        &budget,
        None,
        None,
    )
    .unwrap();
    eprintln!(
        "reference solve, example line, \"high wind\": {:.0?} (setup {:.0?}, coupled solve \
         {:.0?} in {} passes, bodies {:.0?})",
        started.elapsed(),
        solved.setup,
        solved.solve,
        solved.state.passes,
        solved.drawing
    );
    let state = &solved.state;
    assert_eq!(state.level, 3);
    let mut flexible: Vec<&str> = state.points.iter().map(|p| p.structure.as_str()).collect();
    flexible.dedup();
    assert_eq!(flexible, ["T2", "T4", "T6"], "{:?}", state.rigid);
    let rigid: Vec<&str> = state.rigid.iter().map(|r| r.structure.as_str()).collect();
    assert_eq!(rigid, ["T1", "T3", "T5", "T7"]);
    // Leeward (to the left of the line): the pole's top by decimetres.
    let local = super::reference::moves_in_structure_axes(&set, state);
    for (s, l, u) in &local {
        let least = if s == "T6" { 0.1 } else { 0.005 };
        assert!(u[0] < -least, "{s} {l}: {u:?}");
    }
    assert!(
        solved.bodies_failed.is_empty(),
        "{:?}",
        solved.bodies_failed
    );
    assert_eq!(
        solved.bodies.keys().map(String::as_str).collect::<Vec<_>>(),
        ["T2", "T4", "T6"]
    );
    for (id, body) in &solved.bodies {
        let point_most = local
            .iter()
            .filter(|(s, _, _)| s == id)
            .map(|(_, _, u)| (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt())
            .fold(0.0, f64::max);
        // The body moves about as its points (they are on it).
        let most = body.largest();
        assert!(
            most > 0.5 * point_most && most < 2.0 * point_most,
            "{id}: body {most} against points {point_most}"
        );
    }
    let t6 = &solved.bodies["T6"];
    assert!(!t6.pole.is_empty() && t6.joints.is_empty());
    assert!(!solved.bodies["T2"].joints.is_empty());

    // Kept with the line and checked: current, used.
    let mut kept = set.clone();
    kept.reference.as_mut().unwrap().state = Some(state.clone());
    assert_eq!(
        check_reference(&mut kept, &library, &types, &sections, &weather),
        Some(ReferenceStatus::Current)
    );
    assert!(kept.reference_in_use().is_some());
    assert!(
        kept.reference_note()
            .unwrap()
            .starts_with("structures as deflected in case \"high wind\" (calculation level 3)")
    );

    // A level-2 solve in the reference case with the state in use: the
    // coupled solve's with those moves (its converged pass).
    let with = ohl_line::solve::solve_line_with(&kept, &library, windy, &budget);
    let coupled = crate::prepare_check_level(
        &line,
        &library,
        &types,
        &sections,
        std::slice::from_ref(windy),
        CalculationLevel::Linear,
        &budget,
        None,
        None,
    )
    .solve(&line, &library, windy, &budget);
    let coupled = coupled.flexible().unwrap();
    assert!(
        close(&with, coupled, 1e-6),
        "level 2 with the reference against level 3"
    );
    assert!(
        !close(&with, &plain_windy, 1e-6),
        "the moves change the solve"
    );
    // Another case keeps the reference lean (not its own).
    let other = ohl_line::solve::solve_line_with(&kept, &library, everyday, &budget);
    let lean = largest_change(&other, &plain_everyday);
    assert!(lean > 1e-3, "everyday with the high wind's lean: {lean}");
    // Its time: a level-2 solve (the same solve, points moved).
    // The levels 3 and 4 do not take it: the coupled solve on the line
    // with the state in use is the one without.
    let level3 = crate::prepare_check_level(
        &kept,
        &library,
        &types,
        &sections,
        std::slice::from_ref(windy),
        CalculationLevel::Linear,
        &budget,
        None,
        None,
    );
    let at3 = level3.solve(&kept, &library, windy, &budget);
    assert_eq!(format!("{:?}", at3.rigid), format!("{plain_windy:?}"));
    assert!(close(at3.flexible().unwrap(), coupled, 1e-9));
    assert!(
        level3
            .notes(&LevelTally::default(), None)
            .iter()
            .any(|n| n.contains("not used at calculation level 3"))
    );
    // The structure loads keep the structures as drawn, said.
    let loads_cases = vec![LoadCase::from_weather("high wind")];
    let as_kept = structure_loads(&kept, &library, &weather, &loads_cases, &budget).unwrap();
    let as_drawn = structure_loads(&line, &library, &weather, &loads_cases, &budget).unwrap();
    assert_eq!(
        format!("{:?}", as_kept[0].loads),
        format!("{:?}", as_drawn[0].loads)
    );
    assert!(
        as_kept[0]
            .notes
            .iter()
            .any(|n| n.contains("the structure loads keep the structures as drawn")),
        "{:?}",
        as_kept[0].notes
    );

    // Stale on any edit of what it was solved from, and not used then:
    // the structures as drawn, bit for bit, said.
    let stale = |mut l: Line,
                 library: &ohl_cables::Library,
                 sections: &ohl_sections::SectionLibrary,
                 weather: &[WeatherCase],
                 what: &str| {
        let status = check_reference(&mut l, library, &types, sections, weather);
        match &status {
            Some(ReferenceStatus::Stale(why)) => assert!(why.contains(what), "{why}"),
            other => panic!("{what}: {other:?}"),
        }
        assert!(l.reference_in_use().is_none());
        assert!(l.reference_note().unwrap().contains("out of date"));
        let mut drawn = l.clone();
        drawn.reference = None;
        let w = weather.iter().find(|w| w.name == "everyday").unwrap();
        assert_eq!(
            format!(
                "{:?}",
                ohl_line::solve::solve_line_with(&l, library, w, &budget)
            ),
            format!(
                "{:?}",
                ohl_line::solve::solve_line_with(&drawn, library, w, &budget)
            )
        );
    };
    // A structure turned.
    let mut edited = kept.clone();
    edited.structures[1].orientation_deg += 1.0;
    stale(edited, &library, &sections, &weather, "the line");
    // A wire's library entry (here: gone).
    stale(
        kept.clone(),
        &ohl_cables::Library::default(),
        &sections,
        &weather,
        "the wires",
    );
    // The structures' cross-sections.
    let other_sections = ohl_sections::SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    stale(
        kept.clone(),
        &library,
        &other_sections,
        &weather,
        "the structures",
    );
    // The case's weather (its loads).
    let mut stronger = weather.clone();
    for w in &mut stronger {
        if w.name == "high wind" {
            w.wind_pressure = ohl_core::units::Stress::from_pascals(w.wind_pressure.si() + 50.0);
        }
    }
    stale(
        kept.clone(),
        &library,
        &sections,
        &stronger,
        "the reference case's weather",
    );
    // Another reference case.
    let mut moved_case = kept.clone();
    moved_case.reference.as_mut().unwrap().case = "everyday".into();
    stale(
        moved_case,
        &library,
        &sections,
        &weather,
        "solved in case \"high wind\"",
    );
    // A structure dragged (in memory only).
    let mut dragged = kept.clone();
    dragged.reference_stale("a structure being moved");
    assert!(dragged.reference_in_use().is_none());

    // The file: version 22 with a reference, read back unchecked (not
    // used) until checked again; the bodies drawn again from it.
    let text = kept.to_json();
    assert!(text.contains("\"version\": 22"), "{}", &text[..80]);
    let mut back = Line::from_json(&text).unwrap();
    assert_eq!(back.reference, kept.reference);
    assert_eq!(
        back.reference.as_ref().unwrap().status,
        ReferenceStatus::Unchecked
    );
    assert!(back.reference_in_use().is_none());
    assert!(
        back.reference_note()
            .unwrap()
            .contains("not checked against the structure and wire libraries")
    );
    check_reference(&mut back, &library, &types, &sections, &weather);
    assert!(back.reference_in_use().is_some());
    let (bodies, failed) = reference_bodies(&back, &types, &sections, state, &weather);
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(bodies, solved.bodies);
    // A line without one is written as before.
    assert!(!line.to_json().contains("\"version\": 22"));
}
