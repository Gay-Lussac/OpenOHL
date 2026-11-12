// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The swing arc in the vertical and radial clearance checks against the
//! wind steps, on the example line (theory: Vertical and radial
//! clearances › Arc or steps: speed and quality).

use std::path::Path;

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
            altitude: None,
            name: "ground".into(),
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: Requirement::Fixed(required),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
            to_ground: ohl_clearance::rules::ToGround::ByTerrain,
            calculation: None,
        },
        ZoneRule {
            altitude: None,
            name: "points".into(),
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: Requirement::Fixed(required),
            classes: vec![5],
            ground: false,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
            to_ground: ohl_clearance::rules::ToGround::ByTerrain,
            calculation: None,
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
    run_rules(measure, wind, points, rules(required))
}

fn run_rules(
    measure: Measure,
    wind: &WindSteps,
    points: &[[f64; 3]],
    rules: Vec<ZoneRule>,
) -> ZoneCheckResult {
    let (line, library, terrain) = example();
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
        wind.arc.then_some(wind.margins()),
        Some(&read),
        Some(&terrain),
        &[],
        None,
        &Budget::new(),
        &ohl_clearance::zone_progress::ZoneProgress::new(),
        &Default::default(),
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

/// Down to the ground on the swing arc (2026-11-11): a point 8 m above
/// the wire at half the wind — the top of something tall the swinging
/// wire passes. Vertically (3 m to keep) the arc is 8 m below it: no
/// finding; with the rule "down to the ground" the arc is inside the
/// point's column: 0 m, found on the arc.
#[test]
fn down_to_the_ground_on_the_swing_arc() {
    let (line, library, _) = example();
    let mut half = windy();
    half.wind_pressure = Stress::from_si(300.0);
    half.wind_from = WindSide::Right;
    let p = ohl_line::solve::solve_one(&line, &line.sections[0], &library, &half)
        .unwrap()
        .spans[0]
        .mid_point;
    let top = [p[0], p[1], p[2] + 8.0];
    let arc = WindSteps {
        arc: true,
        ..WindSteps::default()
    };
    let found = |to_ground: bool| {
        let mut rules = rules(3.0);
        rules[1].to_ground = ohl_clearance::rules::ToGround::Given(to_ground);
        let r = run_rules(Measure::Vertical, &arc, &[top], rules);
        r.zone
            .findings
            .iter()
            .find(|f| f.rule == 1 && f.section == "S1" && f.span == 0)
            .map(|f| (f.distance, f.arc.is_some()))
    };
    assert_eq!(found(false), None);
    assert_eq!(found(true), Some((0.0, true)));
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
        ..Default::default()
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

/// The bands' margins are inputs, and the jumpers have bands of their own
/// with a margin of their own (2026-10-21): rules of 100 m to 200 invented
/// points (every span and the jumper break them), one on the sections, one
/// on the jumpers, the swing arc on; the spans' margin 0.15 m then 0.40 m
/// with the jumpers' 0 (its default) then 0.30 m. Each span's and the
/// jumper's lowest clearance is on its band (an angle given); with the
/// larger margins the spans' exactly 0.25 m lower and the jumper's exactly
/// 0.30 m lower: the band holds the solved positions, and every distance
/// to it is taken its own margin smaller. By hand: 0.40 − 0.15 = 0.25 m,
/// 0.30 − 0 = 0.30 m.
#[test]
fn the_margin_is_an_input_and_the_jumpers_have_bands() {
    let (line, library, _) = example();
    let points = cloud(&line, &library, 200);
    let read = |_: &[Vec<[f64; 2]>],
                _: f64,
                _: &[u8],
                _: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        for p in &points {
            visit(p[0], p[1], p[2], 5);
        }
        Ok(())
    };
    let mut rules = rules(100.0);
    rules.remove(0);
    let mut jumpers = rules[0].clone();
    jumpers.name = "jumpers".into();
    jumpers.wires = ohl_clearance::Wires::Jumpers;
    rules.push(jumpers);
    let check = |margin: f64, jumper: f64| {
        let wind = WindSteps {
            arc: true,
            band_margin: margin,
            jumper_band_margin: jumper,
            ..WindSteps::default()
        };
        let states = wind_states(&[windy()], |_| true, &wind);
        run_zone_check(
            &line,
            &library,
            Zone::new(Measure::Radial),
            &Default::default(),
            rules.clone(),
            states,
            Some(wind.margins()),
            Some(&read),
            None,
            &[],
            None,
            &Budget::new(),
            &ohl_clearance::zone_progress::ZoneProgress::new(),
            &Default::default(),
        )
        .unwrap()
    };
    let (a, b) = (check(0.15, 0.0), check(0.40, 0.30));
    assert_eq!(
        a.band_margin,
        Some(ohl_clearance::positions::BandMargins::default())
    );
    // Every jumper on a band (none at its solved positions only); at a
    // jumper margin of 0 without steps, said not on the safe side
    // (2026-11-12), with a margin not.
    assert!(
        a.zone
            .notes
            .iter()
            .filter(|n| n.contains("jumper"))
            .all(|n| n == ohl_clearance::zone::JUMPER_BAND_NOTE)
            && a.zone
                .notes
                .contains(&ohl_clearance::zone::JUMPER_BAND_NOTE.to_owned()),
        "{:?}",
        a.zone.notes
    );
    assert!(
        !b.zone.notes.iter().any(|n| n.contains("jumper")),
        "{:?}",
        b.zone.notes
    );
    let (la, lb) = (lowest(&a), lowest(&b));
    assert_eq!(la.len(), lb.len());
    assert!(
        la.iter()
            .any(|x| x.0.1.starts_with("jumper:") && x.0.0 == 1)
    );
    assert!(
        la.iter()
            .any(|x| !x.0.1.starts_with("jumper:") && x.0.0 == 0)
    );
    assert_eq!(a.arc_spans, la.len());
    for (x, y) in la.iter().zip(&lb) {
        assert_eq!(x.0, y.0);
        assert!(x.2.is_some() && y.2.is_some(), "{x:?} on the band");
        let more = if x.0.1.starts_with("jumper:") {
            0.30
        } else {
            0.25
        };
        assert!((x.1 - y.1 - more).abs() < 1e-9, "{x:?} {y:?}");
    }
    // A negative margin is refused.
    let e = run_zone_check(
        &line,
        &library,
        Zone::new(Measure::Radial),
        &Default::default(),
        rules.clone(),
        wind_states(&[windy()], |_| true, &WindSteps::default()),
        Some(ohl_clearance::positions::BandMargins {
            span: -0.1,
            jumper: 0.0,
        }),
        Some(&read),
        None,
        &[],
        None,
        &Budget::new(),
        &ohl_clearance::zone_progress::ZoneProgress::new(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(e.contains("band margin"), "{e}");
    let e = run_zone_check(
        &line,
        &library,
        Zone::new(Measure::Radial),
        &Default::default(),
        rules.clone(),
        wind_states(&[windy()], |_| true, &WindSteps::default()),
        Some(ohl_clearance::positions::BandMargins {
            span: 0.15,
            jumper: -0.1,
        }),
        Some(&read),
        None,
        &[],
        None,
        &Budget::new(),
        &ohl_clearance::zone_progress::ZoneProgress::new(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(e.contains("jumper band margin"), "{e}");
}

/// A ground rule naming a terrain surface of its own (a TIN layer) is
/// checked against it, the others against the checks' ground: the
/// example's survey raised 5 m under one rule's name brings every span's
/// lowest vertical clearance 5 m nearer; a name not given is noted, its
/// rule not checked.
#[test]
fn a_ground_rule_checks_the_surface_it_names() {
    let (line, library, terrain) = example();
    let raised = Terrain::from_mesh(
        terrain
            .points()
            .iter()
            .map(|p| [p[0], p[1], p[2] + 5.0])
            .collect(),
        terrain.triangles().to_vec(),
    )
    .unwrap();
    let wind = WindSteps::default();
    let states = wind_states(&[windy()], |_| true, &wind);
    let mut rules = rules(100.0);
    rules.truncate(1);
    let mut named = rules[0].clone();
    named.name = "raised".into();
    named.surface = Some("raised TIN".into());
    let mut missing = rules[0].clone();
    missing.name = "missing".into();
    missing.surface = Some("no such TIN".into());
    rules.push(named);
    rules.push(missing);
    let r = run_zone_check(
        &line,
        &library,
        Zone::new(Measure::Vertical),
        &Default::default(),
        rules,
        states,
        None,
        None,
        Some(&terrain),
        &[("raised TIN".to_owned(), &raised)],
        None,
        &Budget::new(),
        &ohl_clearance::zone_progress::ZoneProgress::new(),
        &Default::default(),
    )
    .unwrap();
    let all = lowest(&r);
    let of = |rule: usize| -> Vec<(String, usize, f64)> {
        all.iter()
            .filter(|x| x.0.0 == rule)
            .map(|x| (x.0.1.clone(), x.0.2, x.1))
            .collect()
    };
    let (own, ground) = (of(1), of(0));
    assert!(!ground.is_empty());
    assert_eq!(own.len(), ground.len());
    for (a, b) in own.iter().zip(&ground) {
        assert_eq!((&a.0, a.1), (&b.0, b.1));
        assert!((b.2 - a.2 - 5.0).abs() < 1e-6, "{a:?} {b:?}");
    }
    assert!(of(2).is_empty());
    assert!(
        r.zone
            .notes
            .iter()
            .any(|n| n.contains("\"missing\"") && n.contains("no such TIN")),
        "{:?}",
        r.zone.notes
    );
}

/// A run with the swing arc on the example line, its progress and its
/// budget given; `points` repeated `times`.
fn run_watched(
    points: &[[f64; 3]],
    times: usize,
    budget: &Budget,
    progress: &ohl_clearance::zone_progress::ZoneProgress,
) -> Result<ZoneCheckResult, String> {
    let (line, library, terrain) = example();
    let wind = WindSteps {
        stepped: false,
        arc: true,
        ..WindSteps::default()
    };
    let states = wind_states(&[windy()], |_| true, &wind);
    let read = |_: &[Vec<[f64; 2]>],
                _: f64,
                _: &[u8],
                _: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        for _ in 0..times {
            for p in points {
                visit(p[0], p[1], p[2], 5);
            }
        }
        Ok(())
    };
    run_zone_check(
        &line,
        &library,
        Zone::new(Measure::Radial),
        &Default::default(),
        rules(100.0),
        states,
        Some(wind.margins()),
        Some(&read),
        Some(&terrain),
        &[],
        None,
        budget,
        progress,
        &Default::default(),
    )
}

/// Every step is counted (owner, 2026-11-04: "make it count"): a run
/// with points, the ground and the swing arcs says its five steps, and
/// at the end the last one (the ground: its wires and every line of the
/// arcs' boundaries) is counted to the full.
#[test]
fn a_zone_check_counts_its_steps() {
    use ohl_clearance::zone_progress::{Step, ZoneProgress};
    let (line, library, _) = example();
    let points = cloud(&line, &library, 2000);
    let progress = ZoneProgress::new();
    run_watched(&points, 1, &Budget::new(), &progress).unwrap();
    let s = progress.snapshot().unwrap();
    assert_eq!((s.step, s.number, s.steps), (Step::Ground, 5, 5));
    assert!(s.total > 0);
    assert_eq!(s.done, s.total);
    assert_eq!(s.left(), None);
}

/// Stop while the points are checked (2026-11-04: Stop did nothing for
/// minutes): the run returns at once with "cancelled", well before the
/// points it was given are checked.
#[test]
fn a_zone_check_stopped_among_the_points_returns_at_once() {
    use ohl_clearance::zone_progress::{Step, ZoneProgress};
    use ohl_core::budget::CancelToken;
    use std::time::{Duration, Instant};
    let (line, library, _) = example();
    let points = cloud(&line, &library, 20_000);
    let progress = ZoneProgress::new();
    let token = CancelToken::new();
    let budget = Budget::new().cancel_token(token.clone());
    let stopped = std::thread::scope(|s| {
        let watcher = s.spawn(|| {
            // Cancelled once the points are being checked.
            let begun = Instant::now();
            while progress.snapshot().is_none_or(|p| p.step != Step::Points) {
                assert!(begun.elapsed() < Duration::from_secs(300), "no points step");
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(100));
            token.cancel();
            Instant::now()
        });
        // Far more points than are checked in the time allowed below.
        let r = run_watched(&points, 2_000, &budget, &progress);
        let at = watcher.join().unwrap();
        (r, at.elapsed())
    });
    let (r, after) = stopped;
    let e = r.unwrap_err();
    assert!(e.contains("cancelled"), "{e}");
    assert!(
        after < Duration::from_secs(5),
        "returned {after:?} after Stop"
    );
}

/// The radial check to LiDAR points says that it measures only to the
/// points scanned (owner, 2026-11-12); the vertical check, and the radial
/// check without points, do not.
#[test]
fn the_radial_check_says_it_measures_only_to_the_points_scanned() {
    let p = [0.0, 0.0, 0.0];
    let note = ohl_clearance::zone::RADIAL_POINTS_NOTE.to_owned();
    let said = |r: &ZoneCheckResult| r.zone.notes.contains(&note);
    assert!(said(&run(Measure::Radial, &WindSteps::default(), &[p])));
    assert!(!said(&run(Measure::Vertical, &WindSteps::default(), &[p])));
    let ground_only: Vec<ZoneRule> = rules(100.0)
        .into_iter()
        .filter(|r| r.classes.is_empty())
        .collect();
    assert!(!said(&run_rules(
        Measure::Radial,
        &WindSteps::default(),
        &[p],
        ground_only
    )));
}
