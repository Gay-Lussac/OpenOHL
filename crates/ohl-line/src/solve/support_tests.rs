// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of a section solved with its structure points moved, and of its
//! stiffness there ([`super::support`]): nothing changes without a move,
//! a support moved along a span changes the tension as the elastic
//! catenary says, and the stiffness is the finite difference of the
//! moved solves (a bundle's).

use super::final_stringing_tests::one_span;
use super::local_tests::two_spans;
use super::*;
use ohl_core::units::{Stress, Temperature};
use std::path::Path;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

fn windy(t: f64, pressure: f64) -> WeatherCase {
    let mut c = WeatherCase::still("case", Temperature::from_celsius(t));
    c.wind_pressure = Stress::from_pascals(pressure);
    c
}

/// Every structure point a solved section reports loads on, moved by
/// `u`.
fn all_moved(section: &Section, shape: &SectionShape, u: Point) -> PointMoves {
    let mut moves = PointMoves::new();
    for (k, supports) in shape.attachment_loads.iter().enumerate() {
        for s in supports {
            moves.insert((section.points[k].structure.clone(), s.label.clone()), u);
        }
    }
    moves
}

/// No move, zero moves, or the stiffness asked for: every section of the
/// example line (strings, a V, jumpers, a TAK) and of a two-span section
/// solves bit for bit as before, in still air and in wind.
#[test]
fn without_a_move_nothing_changes_bit_for_bit() {
    let lib = library();
    let budget = default_fe_budget();
    let example = crate::tests::example();
    let (two, two_cable) = two_spans();
    let mut checked = 0;
    for case in [windy(15.0, 0.0), windy(-5.0, 450.0)] {
        let mut runs: Vec<(&Line, &Section, &Cable)> = example
            .sections
            .iter()
            .map(|s| (&example, s, lib.get(&s.cable).unwrap()))
            .collect();
        runs.push((&two, &two.sections[0], &two_cable));
        for (line, section, cable) in runs {
            let plain = solve_section(line, section, cable, &case, &budget).unwrap();
            let zero = Coupling {
                moved: all_moved(section, &plain, [0.0; 3]),
                ..Coupling::default()
            };
            let asked = Coupling {
                stiffness: true,
                ..zero.clone()
            };
            let a = solve_section_coupled(line, section, cable, &case, &budget, &zero).unwrap();
            let mut b =
                solve_section_coupled(line, section, cable, &case, &budget, &asked).unwrap();
            if section.calculation.method() == crate::condition::Method::Fe {
                let k = b.support_stiffness.take().expect("asked for");
                let points: usize = plain.attachment_loads.iter().map(Vec::len).sum();
                assert_eq!(k.points.len(), points, "section {}", section.id);
            }
            assert_eq!(
                format!("{plain:?}"),
                format!("{a:?}"),
                "section {}",
                section.id
            );
            assert_eq!(
                format!("{plain:?}"),
                format!("{b:?}"),
                "section {}",
                section.id
            );
            checked += 1;
        }
    }
    assert!(checked >= 4, "{checked}");
}

/// A level span between wire points fixed to two structures, one of them
/// moved 5 cm towards the other: the horizontal tension falls as the
/// elastic catenary with the same unstressed length and weight says,
/// \\( L = \frac{2H}{w}\operatorname{asinh}\frac{w L_0}{2H} + \frac{H L_0}{EA} \\)
/// with the chord L shortened by the move (w and L₀ from the rigid
/// solve's tension and the supports' vertical loads, H′ by bisection).
#[test]
fn a_support_moved_along_the_span_slackens_it_as_the_catenary_says() {
    let (line, cable) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}"#,
        "initial-fe",
    );
    let section = &line.sections[0];
    let case = windy(15.0, 0.0);
    let budget = default_fe_budget();
    let rigid = solve_section(&line, section, &cable, &case, &budget).unwrap();
    let h0 = rigid.spans[0].horizontal_tension;
    assert!((h0 - 20_000.0).abs() < 1e-6 * h0, "{h0}");
    let v: f64 = rigid
        .attachment_loads
        .iter()
        .flatten()
        .map(|s| -s.force[2])
        .sum();
    let ea = cable.axial_stiffness().si();
    let span = 300.0;
    // L₀ from L = (2H L₀ / V) asinh(V / 2H) + H L₀ / EA, then w = V / L₀.
    let l0 = span / (2.0 * h0 / v * (v / (2.0 * h0)).asinh() + h0 / ea);
    let w = v / l0;
    let chord = |h: f64| 2.0 * h / w * (w * l0 / (2.0 * h)).asinh() + h * l0 / ea;
    let delta = 0.05;
    let (mut lo, mut hi) = (1_000.0, h0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chord(mid) > span - delta {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let expected = 0.5 * (lo + hi);
    let moved = Coupling {
        moved: PointMoves::from([(("T2".into(), "W1".into()), [-delta, 0.0, 0.0])]),
        ..Coupling::default()
    };
    let shape = solve_section_coupled(&line, section, &cable, &case, &budget, &moved).unwrap();
    let h1 = shape.spans[0].horizontal_tension;
    assert!(
        (h1 - expected).abs() < 1e-7 * expected,
        "H {h1} against the catenary's {expected} (rigid {h0})"
    );
    // 5 cm off a 300 m span at 20 kN: about 0.6 kN less — the span's
    // stiffness along it, 1/(L/EA + w²L³/(12H³)) ≈ 12.4 kN/m, times the
    // move.
    assert!(h0 - h1 > 550.0 && h0 - h1 < 700.0, "{h0} → {h1}");
    // The support is where it was moved to.
    let t2 = &shape.attachment_loads[1][0];
    assert!((t2.at[0] - (span - delta)).abs() < 1e-12, "{:?}", t2.at);
}

/// The two-span section of the local tests (a suspension string at T2),
/// clipped in (its lengths fixed), as a quad bundle, its ice and wind
/// per stretched or per unstretched metre.
fn quad(loads_per: crate::LoadsPer) -> (Line, Cable) {
    let (mut line, cable) = two_spans();
    let (lengths, temperature) = clip_lengths(&line, &line.sections[0], &cable).unwrap();
    line.sections[0].stringing = Stringing::UnstressedLengths {
        lengths,
        temperature,
        sagged: None,
    };
    line.sections[0].bundle = 4;
    line.sections[0].bundle_spacing = Some(0.4);
    line.solver.loads_per = loads_per;
    line.validate().unwrap();
    (line, cable)
}

/// The support stiffness of `line`'s first section in `case` against
/// −d(force on the structures)/d(move) of whole solves with each point
/// moved ±3 mm along each axis: the largest difference, a share of the
/// largest stiffness; with the stiffness.
fn against_moved_solves(line: &Line, cable: &Cable, case: &WeatherCase) -> (f64, SupportStiffness) {
    let section = &line.sections[0];
    let budget = default_fe_budget();
    let asked = Coupling {
        stiffness: true,
        ..Coupling::default()
    };
    let base = solve_section_coupled(line, section, cable, case, &budget, &asked).unwrap();
    let k = base.support_stiffness.clone().expect("asked for");
    let forces = |shape: &SectionShape| -> Vec<Vector3<f64>> {
        k.points
            .iter()
            .map(|p| {
                let s = shape.attachment_loads[p.point]
                    .iter()
                    .find(|s| s.label == p.label)
                    .unwrap();
                v(s.force)
            })
            .collect()
    };
    let h = 3e-3;
    let mut worst: f64 = 0.0;
    for (b, p) in k.points.iter().enumerate() {
        for axis in 0..3 {
            let at = |d: f64| {
                let mut u = [0.0; 3];
                u[axis] = d;
                let c = Coupling {
                    moved: PointMoves::from([((p.structure.clone(), p.label.clone()), u)]),
                    ..Coupling::default()
                };
                forces(&solve_section_coupled(line, section, cable, case, &budget, &c).unwrap())
            };
            let (plus, minus) = (at(h), at(-h));
            for a in 0..k.points.len() {
                let fd = -(plus[a] - minus[a]) / (2.0 * h);
                for c in 0..3 {
                    let e = (k.matrix[(3 * a + c, 3 * b + axis)] - fd[c]).abs();
                    worst = worst.max(e / k.matrix.abs().max());
                }
            }
        }
    }
    (worst, k)
}

/// A quad bundle in wind: the support stiffness (the whole bundle's) is
/// the finite difference of the moved solves at every point and axis —
/// to 10⁻⁵ with the ice and wind per unstretched metre (the tangent is
/// then exact), to 10⁻³ per stretched metre (the default: the load's
/// dependence on the stretch is left out of the tangent, said); and four
/// times one wire's.
#[test]
fn the_support_stiffness_of_a_bundle_is_the_finite_difference() {
    let case = windy(-5.0, 300.0);
    let (line, cable) = quad(crate::LoadsPer::Unstretched);
    let (worst, k) = against_moved_solves(&line, &cable, &case);
    assert_eq!(k.points.len(), 3, "{:?}", k.points);
    assert!(worst < 1e-5, "per unstretched metre: {worst:e}");
    let (stretched, cable) = quad(crate::LoadsPer::Stretched);
    let (worst, _) = against_moved_solves(&stretched, &cable, &case);
    assert!(worst < 1e-3, "per stretched metre: {worst:e}");
    // The whole bundle's: four times one wire's (the same section with
    // one wire and a quarter of the strings' weight and wind).
    let mut single = line.clone();
    single.sections[0].bundle = 1;
    single.sections[0].bundle_spacing = None;
    for s in &mut single.structures {
        for st in &mut s.strings {
            st.weight /= 4.0;
            st.wind_area /= 4.0;
        }
    }
    single.validate().unwrap();
    let asked = Coupling {
        stiffness: true,
        ..Coupling::default()
    };
    let one = solve_section_coupled(
        &single,
        &single.sections[0],
        &cable,
        &case,
        &default_fe_budget(),
        &asked,
    )
    .unwrap()
    .support_stiffness
    .unwrap();
    let ratio = k.matrix.abs().max() / one.matrix.abs().max();
    assert!((ratio - 4.0).abs() < 1e-9, "{ratio}");
}
