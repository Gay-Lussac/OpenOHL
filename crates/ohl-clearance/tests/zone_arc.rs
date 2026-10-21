// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The swing arc in the vertical and radial clearance checks against the
//! wind steps, on the example line (theory: Vertical and radial
//! clearances › Arc or steps: speed and quality).

use std::path::Path;
use std::sync::atomic::AtomicUsize;

use ohl_clearance::rules::{Requirement, WindSides, WindSteps};
use ohl_clearance::zone::{Measure, Zone, ZoneCheckResult, ZoneRule, run_zone_check, wind_states};
use ohl_core::budget::Budget;
use ohl_core::units::{Stress, Temperature};
use ohl_line::Line;
use ohl_terrain::Terrain;
use ohl_weather::{WeatherCase, WindSide};

fn example() -> (Line, ohl_cables::Library, Terrain) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    let terrain =
        Terrain::load(&root.join("examples/synthetic-line/synthetic-survey.csv")).unwrap();
    (line, library, terrain)
}

fn windy() -> WeatherCase {
    let mut c = WeatherCase::still("high wind", Temperature::from_celsius(15.0));
    c.wind_pressure = Stress::from_si(600.0);
    c
}

/// Rules of `required` m to the ground and to the points (100 m: every
/// span breaks them, each span's lowest clearance is a finding).
fn rules(required: f64) -> Vec<ZoneRule> {
    vec![
        ZoneRule {
            name: "ground".into(),
            voltage: None,
            cases: Vec::new(),
            distance: Requirement::Fixed(required),
            classes: Vec::new(),
            ground: true,
            wires: ohl_clearance::Wires::Sections,
            width: None,
        },
        ZoneRule {
            name: "points".into(),
            voltage: None,
            cases: Vec::new(),
            distance: Requirement::Fixed(required),
            classes: vec![5],
            ground: false,
            wires: ohl_clearance::Wires::Sections,
            width: None,
        },
    ]
}

/// Invented points around the wires in still air and blown (a fixed
/// sequence): within 14 m across and from 22 m below to 2 m above.
fn cloud(line: &Line, library: &ohl_cables::Library, n: usize) -> Vec<[f64; 3]> {
    let mut seed: u64 = 0x5851_f42d_4c95_7f2d;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let shapes: Vec<_> = ohl_line::solve::solve_line(
        line,
        library,
        &WeatherCase::still("s", Temperature::from_celsius(15.0)),
    )
    .into_iter()
    .map(Result::unwrap)
    .collect();
    let wire: Vec<[f64; 3]> = shapes
        .iter()
        .flat_map(|s| &s.spans)
        .flat_map(|s| s.points.iter().copied())
        .collect();
    (0..n)
        .map(|_| {
            let p = wire[(next() * wire.len() as f64) as usize % wire.len()];
            [
                p[0] + (next() - 0.5) * 28.0,
                p[1] + (next() - 0.5) * 28.0,
                p[2] - next() * 24.0 + 2.0,
            ]
        })
        .collect()
}

fn run(measure: Measure, wind: &WindSteps, points: &[[f64; 3]]) -> ZoneCheckResult {
    run_with(measure, wind, points, 100.0)
}

fn run_with(
    measure: Measure,
    wind: &WindSteps,
    points: &[[f64; 3]],
    required: f64,
) -> ZoneCheckResult {
    let (line, library, terrain) = example();
    let rules = rules(required);
    let states = wind_states(&[windy()], |_| true, wind);
    let read = |_: &[Vec<[f64; 2]>],
                _: f64,
                _: &[u8],
                _: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        for p in points {
            visit(p[0], p[1], p[2], 5);
        }
        Ok(())
    };
    run_zone_check(
        &line,
        &library,
        Zone::new(measure),
        &Default::default(),
        rules,
        states,
        wind.arc,
        Some(&read),
        Some(&terrain),
        None,
        &Budget::new(),
        &AtomicUsize::new(0),
    )
    .unwrap()
}

/// Each span's lowest clearance per rule: (rule, section, span) → m.
/// (rule, section, span), the clearance, the angle on the arc.
type Lowest = ((usize, String, usize), f64, Option<f64>);

fn lowest(r: &ZoneCheckResult) -> Vec<Lowest> {
    let mut v: Vec<_> = r
        .zone
        .findings
        .iter()
        .map(|f| ((f.rule, f.section.clone(), f.span), f.distance, f.arc))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// A point on the wire at half the wind (300 Pa from the right): 1 m and
/// more from the full blowouts, inside the swing arc's band (three
/// solves; 0.47 m from the arc at the largest radius before 2026-10-21).
#[test]
fn the_arc_finds_the_wire_at_half_the_wind() {
    let (line, library, _) = example();
    let mut half = windy();
    half.wind_pressure = Stress::from_si(300.0);
    half.wind_from = WindSide::Right;
    let p = ohl_line::solve::solve_one(&line, &line.sections[0], &library, &half)
        .unwrap()
        .spans[0]
        .mid_point;
    let arc = WindSteps {
        arc: true,
        ..WindSteps::default()
    };
    let r = run(Measure::Radial, &arc, &[p]);
    assert_eq!(r.states.len(), 3);
    let f = r
        .zone
        .findings
        .iter()
        .find(|f| f.rule == 1 && f.section == "S1" && f.span == 0)
        .unwrap();
    // On the arc, blown to the left (the wind from the right).
    let angle = f.arc.expect("found on the arc");
    assert!(angle < -5.0, "{angle}");
    // The wire at 300 Pa sags less than at 600 Pa: it lies 0.47 m nearer
    // the axis than the largest radius, between the smallest and the
    // largest — in the band (within its margin): its clearance below 0
    // (theory: Arc or steps).
    assert!(f.distance <= 0.0, "{f:?}");
    assert!(f.position(&r.states).contains("swing arc at"));
    // Without the arc: the full blowouts only, far from it.
    let plain = run(Measure::Radial, &WindSteps::default(), &[p]);
    let g = plain
        .zone
        .findings
        .iter()
        .find(|f| f.rule == 1 && f.section == "S1" && f.span == 0)
        .unwrap();
    assert!(g.distance > 1.0, "{g:?}");
}

/// The arc against the steps (50 Pa, both sides) on the example line, to
/// the ground and to 4000 invented points, vertically and radially: per
/// span and rule, the arc's lowest clearance (the band with its margin,
/// 2026-10-21) is never above the steps' (the largest difference printed;
/// measured 2026-10-21: see the theory; before, at the largest radius
/// only, up to 2.1 cm above).
#[test]
fn the_arc_against_the_steps_on_the_example_line() {
    let (line, library, _) = example();
    let points = cloud(&line, &library, 4000);
    let steps = WindSteps {
        stepped: true,
        step: 50.0,
        sides: WindSides::Both,
        arc: false,
    };
    let arc = WindSteps {
        arc: true,
        ..WindSteps::default()
    };
    for measure in [Measure::Vertical, Measure::Radial] {
        let a = run(measure, &arc, &points);
        let s = run(measure, &steps, &points);
        let (la, ls) = (lowest(&a), lowest(&s));
        assert_eq!(la.len(), ls.len());
        let mut worst = (f64::NEG_INFINITY, String::new());
        let mut gain = f64::NEG_INFINITY;
        for (x, y) in la.iter().zip(&ls) {
            assert_eq!(x.0, y.0);
            let d = x.1 - y.1;
            if d > worst.0 {
                worst = (
                    d,
                    format!(
                        "{} {} span {} ({}: arc {:.3} m at {:?}°, steps {:.3} m)",
                        measure.as_str(),
                        x.0.1,
                        x.0.2 + 1,
                        if x.0.0 == 0 { "ground" } else { "points" },
                        x.1,
                        x.2.map(|a| a.round()),
                        y.1
                    ),
                );
            }
            gain = gain.max(-d);
        }
        eprintln!(
            "{}: {} solves (arc: solved in {:.1?}, checked in {:.1?}) vs {} (steps: {:.1?}, {:.1?}); arc − steps at most {:+.3} m ({}); the arc lower by up to {:.3} m",
            measure.as_str(),
            a.states.len(),
            a.solving,
            a.checking,
            s.states.len(),
            s.solving,
            s.checking,
            worst.0,
            worst.1,
            gain
        );
        assert!(worst.0 <= 0.0, "{}", worst.1);
        // The time with rules of 7 m (most spans keep them).
        let a = run_with(measure, &arc, &points, 7.0);
        let s = run_with(measure, &steps, &points, 7.0);
        eprintln!(
            "{}, rules of 7 m: arc solved in {:.1?}, checked in {:.1?}; steps solved in {:.1?}, checked in {:.1?}",
            measure.as_str(),
            a.solving,
            a.checking,
            s.solving,
            s.checking
        );
    }
}
