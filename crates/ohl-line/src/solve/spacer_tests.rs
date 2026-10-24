// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of spacers between the sections of a bundle.

use super::*;
use ohl_core::units::Temperature;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap()
}

/// The project's default values in the solve: a string without its
/// own EA takes the project's (a soft one lets the clamps down); a
/// jumper without its own exit angle the project's.
#[test]
fn the_project_defaults_are_solved_with() {
    let lib = library();
    let case = ohl_weather::WeatherCase::still("15", Temperature::from_celsius(15.0));
    let mut line = crate::tests::example();
    for s in &mut line.structures {
        for x in &mut s.strings {
            x.stiffness = None;
        }
    }
    let low = |line: &Line| -> f64 {
        solve_line(line, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .flat_map(|s| s.clamps.clone())
            .map(|c| c[2])
            .sum()
    };
    let stiff = low(&line);
    line.defaults.insulator_stiffness = 5e5;
    let soft = low(&line);
    assert!(soft < stiff - 0.01, "{soft} {stiff}");
    // A jumper's exit angle.
    let mut line = crate::tests::example();
    let j = line
        .jumpers
        .iter_mut()
        .find(|j| j.angle_from_deg.is_none() && !j.rigid)
        .expect("a jumper without its own angle");
    let id = j.id.clone();
    let shape = |line: &Line| {
        let shapes: Vec<SectionShape> = solve_line(line, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        crate::jumper::solve_jumpers(line, &lib, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .find(|x| x.jumper == id)
            .unwrap()
            .points
    };
    let at30 = shape(&line);
    line.defaults.jumper_exit_angle_deg = 60.0;
    let at60 = shape(&line);
    let moved = at30
        .iter()
        .zip(&at60)
        .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt())
        .fold(0.0, f64::max);
    assert!(moved > 0.01, "{moved}");
}

/// S1 and S2 (both T1–T2) with span points at mid-span and a spacer
/// between them of `length` (none: from stringing).
fn spaced(length: Option<f64>, compression: Option<f64>) -> Line {
    let mut line = crate::tests::example();
    for s in &mut line.sections[..2] {
        s.span_points.push(crate::SpanPoint {
            label: "M".into(),
            span: 0,
            at: crate::SpanPlace::at_fraction(0.5),
            description: String::new(),
            // A spacer's end: a clamp (of no weight here).
            object: Some(crate::LineObject::clamp(0.0)),
        });
    }
    line.spacers.push(crate::Spacer {
        id: "SP".into(),
        a: crate::SpanPointRef {
            section: line.sections[0].id.clone(),
            point: "M".into(),
        },
        b: crate::SpanPointRef {
            section: line.sections[1].id.clone(),
            point: "M".into(),
        },
        length,
        weight: 200.0,
        compression,
        ..Default::default()
    });
    line.validate().unwrap();
    line
}

/// Sections joined by a spacer, one of them changed edit after edit
/// and brought up to date incrementally: the same shapes as a full
/// solve (the group coupled again from each solved alone, so the
/// result does not depend on the edits before).
#[test]
fn a_spacer_group_brought_up_to_date_is_a_full_solve() {
    let lib = library();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let mut line = spaced(Some(0.3), None);
    let mut live = crate::incremental::SolvedLine::solve(&line, &lib, &case);
    let id = line.sections[0].id.clone();
    for h in [22_000.0, 26_000.0, 24_000.0] {
        line.sections[0].stringing = crate::Stringing::Tension {
            horizontal_tension: h,
            temperature: 15.0,
        };
        live.update(&line, &lib, std::slice::from_ref(&id));
        let full: Vec<SectionShape> = solve_line(&line, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(live.shapes(&line), full, "after H = {h}");
    }
}

fn mid(shape: &SectionShape) -> Vector3<f64> {
    v(shape.span_points[0].1)
}

/// A spacer between two conductors of a span: shorter than their gap,
/// it pulls them together (its ends at its length, in tension, equal
/// and opposite on both); of the stringing distance it takes almost
/// nothing in still air at that temperature; longer, it pushes them
/// apart in compression — flagged beyond its limit, slack if it takes
/// none.
#[test]
fn a_spacer_joins_two_conductors() {
    let lib = library();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let solve = |line: &Line| -> Vec<SectionShape> {
        solve_line(line, &lib, &case)
            .into_iter()
            .map(|r| r.unwrap())
            .collect()
    };
    let free = solve(&crate::tests::example());
    let gap = (mid_of(&free[0]) - mid_of(&free[1])).norm();
    fn mid_of(s: &SectionShape) -> Vector3<f64> {
        let p = &s.spans[0].points;
        v(p[p.len() / 2])
    }
    assert!(gap > 1.0, "{gap}");
    // Shorter: pulled together to its length.
    let short = solve(&spaced(Some(0.7 * gap), Some(10_000.0)));
    let (a, b) = (mid(&short[0]), mid(&short[1]));
    assert!(
        ((a - b).norm() - 0.7 * gap).abs() < 0.01,
        "{} {}",
        (a - b).norm(),
        0.7 * gap
    );
    let state = &short[0].spacers[0];
    assert!(state.force > 0.0, "tension: {}", state.force);
    assert!(!state.slack && !state.over);
    assert!(short[1].spacers.is_empty(), "held by its end a");
    // Of the stringing distance (stringing at 15 °C here or not, it is
    // its length at stringing): a small force in still air.
    let set = solve(&spaced(None, Some(10_000.0)));
    let s = &set[0].spacers[0];
    assert!(
        s.force.abs() < 0.05 * state.force.abs().max(100.0),
        "{}",
        s.force
    );
    // Longer: pushed apart, in compression; flagged beyond its limit.
    let long = solve(&spaced(Some(1.3 * gap), Some(50.0)));
    let s = &long[0].spacers[0];
    assert!(s.force < 0.0, "compression: {}", s.force);
    assert!(s.over && long[0].flags.iter().any(|f| f.contains("spacer SP")));
    assert!((mid(&long[0]) - mid(&long[1])).norm() > gap);
    // Tension only: slack, nothing pushes.
    let slack = solve(&spaced(Some(1.3 * gap), None));
    let s = &slack[0].spacers[0];
    assert!(s.slack && s.force.abs() < 1.0, "{s:?}");
    // Its weight: the two clamps lower than without it.
    assert!(mid(&slack[1])[2] < mid_of(&free[1])[2] + 1e-6);
    // The incremental solve: the same.
    let line = spaced(Some(0.7 * gap), Some(10_000.0));
    let inc = crate::incremental::SolvedLine::solve(&line, &lib, &case);
    assert_eq!(inc.shapes(&line), solve(&line));
}
