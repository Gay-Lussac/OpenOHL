// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the constant horizontal forces on span points (roadmap O22
//! item 7): across and along the line.

use super::final_stringing_tests::one_span;
use super::*;
use crate::{LineObject, SpanPlace, SpanPoint};
use ohl_core::units::Temperature;

/// The level 300 m span between fixed wire points, strung at 20 kN at
/// 15 °C, with a clamp of no weight at mid-span carrying `across` and
/// `along` (N).
fn span_with_force(across: f64, along: f64, calculation: &str) -> (Line, Cable) {
    let (mut line, cable) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}"#,
        calculation,
    );
    line.sections[0].span_points.push(SpanPoint {
        label: "P1".into(),
        span: 0,
        at: SpanPlace::at_fraction(0.5),
        description: String::new(),
        object: Some(LineObject {
            force_across: across,
            force_along: along,
            ..LineObject::clamp(0.0)
        }),
    });
    line.validate().unwrap();
    (line, cable)
}

fn solved(line: &Line, cable: &Cable) -> SectionShape {
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    solve_section(line, &line.sections[0], cable, &case, &default_fe_budget()).unwrap()
}

/// By hand (a taut string): 500 N across at the middle of a level 300 m
/// span at H ≈ 20 kN moves the wire there by P S / (4 H) = 500 × 300 /
/// (4 × 20 000) = 1.875 m — to the right looking ahead (the span runs
/// along +x: towards −y). The supports carry every load: the wire's
/// weight down, the force across.
#[test]
fn a_force_across_deflects_the_wire() {
    let (line, cable) = span_with_force(500.0, 0.0, "initial-fe");
    let shape = solved(&line, &cable);
    let (label, at) = &shape.span_points[0];
    assert_eq!(label, "P1");
    assert!(
        (at[1] + 1.875).abs() < 0.02 * 1.875,
        "deflection {} m against 1.875 m by hand",
        -at[1]
    );
    assert!((at[0] - 150.0).abs() < 1e-6, "{}", at[0]);
    // No force: the wire in its vertical plane.
    let (plain, _) = span_with_force(0.0, 0.0, "initial-fe");
    let straight = solved(&plain, &cable);
    assert!(straight.span_points[0].1[1].abs() < 1e-9);
    // The wire's pulls on its two ends balance the loads on it: across,
    // the 500 N; down, its weight; along, nothing.
    let pulls = shape.spans[0].end_pulls;
    let sum: Vec<f64> = (0..3).map(|k| pulls[0][k] + pulls[1][k]).collect();
    let weight = straight.spans[0].end_pulls[0][2] + straight.spans[0].end_pulls[1][2];
    assert!(sum[0].abs() < 1e-4, "{sum:?}");
    assert!((sum[1].abs() - 500.0).abs() < 1e-4, "{sum:?}");
    assert!((sum[2] - weight).abs() < 1e-4, "{sum:?} against {weight}");
    // To the left with the other sign, by as much.
    let (left, _) = span_with_force(-500.0, 0.0, "initial-fe");
    assert!((solved(&left, &cable).span_points[0].1[1] + at[1]).abs() < 1e-9);
}

/// By hand: a force along the line at a point is the difference of the
/// horizontal tensions on its two sides, exactly (the horizontal balance
/// of the point): 300 N ahead, the wire behind 300 N tighter than the
/// wire ahead.
#[test]
fn a_force_along_is_the_difference_of_the_two_sides() {
    let (line, cable) = span_with_force(0.0, 300.0, "initial-fe");
    let shape = solved(&line, &cable);
    let pulls = shape.spans[0].end_pulls;
    // Level, in still air, in the plane y = 0: the x parts are the
    // horizontal tensions at the two ends.
    let (back, ahead) = (pulls[0][0].abs(), pulls[1][0].abs());
    assert!(
        (back - ahead - 300.0).abs() < 1e-6,
        "behind {back} N, ahead {ahead} N"
    );
    assert!(pulls[0][1].abs() < 1e-9 && pulls[1][1].abs() < 1e-9);
    // The point moves ahead.
    assert!(shape.span_points[0].1[0] > 150.0 + 1e-4);
    // A bundle shares it: twice the force on two subconductors is the
    // single wire's deflection.
    let (mut twin, _) = span_with_force(1000.0, 0.0, "initial-fe");
    twin.sections[0].bundle = 2;
    let (single, _) = span_with_force(500.0, 0.0, "initial-fe");
    let (a, b) = (
        solved(&twin, &cable).span_points[0].1,
        solved(&single, &cable).span_points[0].1,
    );
    assert!((a[1] - b[1]).abs() < 1e-9, "{a:?} {b:?}");
}

/// The ruling-span method leaves point loads out: it says so, naming
/// them — the constant forces and the weights alike; the finite elements
/// say nothing of the kind.
#[test]
fn the_ruling_span_says_the_point_loads_it_leaves_out() {
    let (mut line, cable) = span_with_force(400.0, 0.0, "initial-rs");
    let flags = solved(&line, &cable).flags;
    assert_eq!(
        flags,
        [
            "NOT ON THE SAFE SIDE: point loads (P1: across 400 N) left out by the ruling-span \
          method — use FE"
        ]
    );
    // A weight too (a marker ball), and a force along.
    let o = line.sections[0].span_points[0].object.as_mut().unwrap();
    (o.weight, o.force_along) = (55.0, -120.0);
    let flags = solved(&line, &cable).flags;
    assert!(
        flags[0].contains("(P1: 55 N down, across 400 N, along -120 N)"),
        "{flags:?}"
    );
    // A clamp of no weight and no force: nothing to say.
    let (bare, _) = span_with_force(0.0, 0.0, "initial-rs");
    assert!(solved(&bare, &cable).flags.is_empty());
    let (fe, _) = span_with_force(400.0, 0.0, "initial-fe");
    assert!(solved(&fe, &cable).flags.is_empty());
}

/// The file: `force_across` / `force_along` are written only when not
/// zero, and only then is the line of version 15; they read back; a
/// jumper's point takes none.
#[test]
fn the_forces_are_written_only_when_given() {
    let (plain, _) = span_with_force(0.0, 0.0, "initial-fe");
    let json = plain.to_json();
    assert!(
        !json.contains("force_") && !json.contains("\"version\": 15"),
        "{json}"
    );
    let (line, _) = span_with_force(400.0, -120.0, "initial-fe");
    let json = line.to_json();
    assert!(json.contains("\"version\": 15"), "{json}");
    assert!(json.contains("\"force_across\": 400.0") && json.contains("\"force_along\": -120.0"));
    let back = Line::from_json(&json).unwrap();
    assert_eq!(back.sections[0].span_points, line.sections[0].span_points);
    // With a unit, as other forces.
    let with_unit = json.replace("\"force_across\": 400.0", "\"force_across\": \"0.4 kN\"");
    let read = Line::from_json(&with_unit).unwrap();
    let o = read.sections[0].span_points[0].object.as_ref().unwrap();
    assert!((o.force_across - 400.0).abs() < 1e-9);
    assert_eq!(o.force_text(), "across 400 N, along -120 N");
    // On a jumper's point: refused.
    let mut line = crate::tests::example();
    let j = &mut line.jumpers[0];
    j.points.push(crate::JumperPoint {
        label: "F1".into(),
        at: crate::JumperPlace::from_parts(0.5, None, None),
        description: String::new(),
        object: Some(LineObject {
            force_across: 100.0,
            ..LineObject::clamp(0.0)
        }),
    });
    let e = line.validate().unwrap_err().to_string();
    assert!(e.contains("only a section's span point takes one"), "{e}");
}
