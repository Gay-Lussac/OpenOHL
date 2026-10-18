// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use ohl_structures::SpanLimit;

use super::*;

fn limits(wind: f64, weight: f64, min_weight: Option<f64>) -> Capacity {
    Capacity {
        spans: vec![SpanLimit {
            angle: None,
            wind_span: Length::from_si(wind),
            weight_span: Length::from_si(weight),
            min_weight_span: min_weight.map(Length::from_si),
        }],
        attachments: Vec::new(),
    }
}

/// Dead-end allowable spans up to a 60° line angle.
fn dead_end_limits() -> Capacity {
    let mut c = limits(450.0, 600.0, Some(-100.0));
    let mut turned = c.spans[0].clone();
    turned.angle = Some(ohl_core::units::Angle::from_degrees(60.0));
    c.spans.push(turned);
    c
}

fn candidate(
    id: &str,
    ext: Option<&str>,
    height: f64,
    cost: f64,
    cap: Option<Capacity>,
) -> Candidate {
    Candidate {
        structure_type: id.into(),
        extension: ext.map(Into::into),
        height,
        cost,
        dead_end: id.starts_with("DE"),
        capacity: cap,
    }
}

/// Flat ground 0–`length` m at 100 m; c = H/w = 1400 m, so a 400 m span
/// sags 14.3 m and a 450 m span 18.1 m.
fn flat(length: f64, candidates: Vec<Candidate>) -> Problem {
    Problem {
        ground: vec![[0.0, 100.0], [length, 100.0]],
        required: Vec::new(),
        forbidden: Vec::new(),
        candidates,
        step: 50.0,
        max_span: 600.0,
        min_span: 0.0,
        clearance: 5.0,
        horizontal_tension: 21_000.0,
        weight: 15.0,
        span_cost: 0.0,
        wire_cost_per_m: 0.0,
    }
}

fn stations(r: &Spotting) -> Vec<f64> {
    r.structures.iter().map(|s| s.station).collect()
}

/// On flat ground with the clearance allowing spans up to 400 m (on a 50 m
/// grid), the cheapest line is equal spans at that length.
#[test]
fn flat_ground_gives_equal_longest_spans() {
    let p = flat(
        2000.0,
        vec![
            candidate("DE", None, 20.0, 10.0, None),
            candidate("TAN", None, 20.0, 5.0, None),
        ],
    );
    let r = spot(&p, &default_budget()).unwrap();
    assert_eq!(
        stations(&r),
        vec![0.0, 400.0, 800.0, 1200.0, 1600.0, 2000.0]
    );
    assert_eq!(r.total, 2.0 * 10.0 + 4.0 * 5.0);
    assert!(r.spans.iter().all(|s| s.clearance >= 5.0));
    // The middle structures are tangents, the ends dead-ends.
    let types: Vec<usize> = r.structures.iter().map(|s| s.candidate).collect();
    assert_eq!(types, vec![0, 1, 1, 1, 1, 0]);
    // The result passes its own check.
    let placement: Vec<(f64, usize)> = r
        .structures
        .iter()
        .map(|s| (s.station, s.candidate))
        .collect();
    assert_eq!(p.evaluate(&placement).unwrap().total, r.total);
}

/// A 3 m hill in the middle of the first span takes 3 m of its clearance:
/// the cheapest answer is the same spans with a 6 m taller structure at
/// 400 m (the wire at the hill is then 3 m higher).
#[test]
fn a_hill_forces_a_taller_structure() {
    let mut p = flat(
        1200.0,
        vec![
            candidate("DE", None, 20.0, 10.0, None),
            candidate("TAN", None, 20.0, 5.0, None),
            candidate("TAN", Some("+6"), 26.0, 6.0, None),
        ],
    );
    let flat_answer = spot(&p, &default_budget()).unwrap();
    assert_eq!(stations(&flat_answer), vec![0.0, 400.0, 800.0, 1200.0]);
    assert_eq!(flat_answer.total, 30.0);
    p.ground = vec![
        [0.0, 100.0],
        [150.0, 100.0],
        [200.0, 103.0],
        [250.0, 100.0],
        [1200.0, 100.0],
    ];
    let r = spot(&p, &default_budget()).unwrap();
    assert_eq!(stations(&r), vec![0.0, 400.0, 800.0, 1200.0]);
    assert_eq!(r.structures[1].candidate, 2, "{:?}", r.structures);
    assert_eq!(r.total, 31.0);
    assert!(r.spans[0].clearance >= 5.0 && r.spans[0].clearance < 6.0);
    // Without the extension, an extra structure is cheaper than nothing.
    p.candidates.pop();
    let r = spot(&p, &default_budget()).unwrap();
    assert_eq!(r.structures.len(), 5);
    assert_eq!(r.total, 35.0);
}

/// Required (angle) points get a structure that may turn there; forbidden
/// zones get none.
#[test]
fn angle_points_and_forbidden_zones() {
    let mut p = flat(
        1200.0,
        vec![
            candidate("DE", None, 20.0, 10.0, None),
            candidate("TAN", None, 20.0, 5.0, None),
        ],
    );
    p.required = vec![RequiredPoint {
        station: 530.0,
        angle_deg: 20.0,
    }];
    p.forbidden = vec![[750.0, 900.0]];
    let r = spot(&p, &default_budget()).unwrap();
    let at = r.structures.iter().find(|s| s.station == 530.0).unwrap();
    // The tangent may not turn: a dead-end stands there.
    assert_eq!(at.candidate, 0);
    assert_eq!(at.angle_deg, 20.0);
    assert!(
        r.structures
            .iter()
            .all(|s| !(s.station > 750.0 && s.station < 900.0))
    );
    assert!(r.spans.iter().all(|s| s.length <= 400.0 + 1e-9));
    // A tangent with allowable spans up to 25° takes it.
    p.candidates[1].capacity = Some(Capacity {
        spans: vec![
            SpanLimit {
                angle: None,
                wind_span: Length::from_si(1000.0),
                weight_span: Length::from_si(1000.0),
                min_weight_span: None,
            },
            SpanLimit {
                angle: Some(ohl_core::units::Angle::from_degrees(25.0)),
                wind_span: Length::from_si(1000.0),
                weight_span: Length::from_si(1000.0),
                min_weight_span: None,
            },
        ],
        attachments: Vec::new(),
    });
    let r = spot(&p, &default_budget()).unwrap();
    let at = r.structures.iter().find(|s| s.station == 530.0).unwrap();
    assert_eq!(at.candidate, 1);
}

/// Every placement on a small hilly profile, tried one by one: the
/// dynamic programme finds the cheapest.
#[test]
fn matches_exhaustive_search_on_hilly_ground() {
    for seed in 0..6u64 {
        // A deterministic rough profile.
        let mut x = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut rnd = || {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (x >> 33) as f64 / (1u64 << 31) as f64
        };
        let ground: Vec<[f64; 2]> = (0..=24)
            .map(|k| {
                [
                    k as f64 * 25.0,
                    100.0 + 12.0 * rnd() + 0.01 * k as f64 * 25.0,
                ]
            })
            .collect();
        let p = Problem {
            ground,
            required: Vec::new(),
            forbidden: Vec::new(),
            candidates: vec![
                candidate(
                    "DE",
                    None,
                    20.0,
                    10.0,
                    Some(limits(400.0, 500.0, Some(-50.0))),
                ),
                candidate(
                    "TAN",
                    None,
                    18.0,
                    4.0,
                    Some(limits(260.0, 380.0, Some(0.0))),
                ),
                candidate(
                    "TAN",
                    Some("+5"),
                    23.0,
                    5.5,
                    Some(limits(250.0, 360.0, Some(0.0))),
                ),
            ],
            step: 75.0,
            max_span: 320.0,
            min_span: 0.0,
            clearance: 6.0,
            horizontal_tension: 18_000.0,
            weight: 15.0,
            span_cost: 1.0,
            wire_cost_per_m: 0.001,
        };
        // 0, 75, …, 600: seven inner stations, each empty or one of three.
        let inner: Vec<f64> = (1..8).map(|k| k as f64 * 75.0).collect();
        let mut best: Option<f64> = None;
        for code in 0..4usize.pow(7) {
            let mut placement = vec![(0.0, 0)];
            let mut c = code;
            for &s in &inner {
                if c % 4 > 0 {
                    placement.push((s, c % 4 - 1));
                }
                c /= 4;
            }
            placement.push((600.0, 0));
            if let Ok(r) = p.evaluate(&placement) {
                best = Some(best.map_or(r.total, |b: f64| b.min(r.total)));
            }
        }
        match (spot(&p, &default_budget()), best) {
            (Ok(r), Some(b)) => assert!(
                (r.total - b).abs() < 1e-9,
                "seed {seed}: {} vs {b}",
                r.total
            ),
            (Err(SpotError::Infeasible(_)), None) => {}
            (r, b) => panic!("seed {seed}: {r:?} vs {b:?}"),
        }
    }
}

/// A 5 km route at 20 m steps with four candidates: how long it takes.
#[test]
fn timing_of_a_long_route() {
    let ground: Vec<[f64; 2]> = (0..=500)
        .map(|k| {
            let s = k as f64 * 10.0;
            [
                s,
                400.0 + 25.0 * (s / 700.0).sin() + 8.0 * (s / 130.0).cos(),
            ]
        })
        .collect();
    let p = Problem {
        ground,
        required: vec![RequiredPoint {
            station: 2510.0,
            angle_deg: 12.0,
        }],
        forbidden: vec![[1000.0, 1100.0]],
        candidates: vec![
            candidate("DE", None, 22.0, 60.0, Some(dead_end_limits())),
            candidate(
                "TAN",
                None,
                22.0,
                25.0,
                Some(limits(420.0, 600.0, Some(0.0))),
            ),
            candidate(
                "TAN",
                Some("+3"),
                25.0,
                27.0,
                Some(limits(420.0, 600.0, Some(0.0))),
            ),
            candidate(
                "TAN",
                Some("+6"),
                28.0,
                29.5,
                Some(limits(400.0, 580.0, Some(0.0))),
            ),
        ],
        step: 20.0,
        max_span: 450.0,
        min_span: 50.0,
        clearance: 7.0,
        horizontal_tension: 25_000.0,
        weight: 16.0,
        span_cost: 3.0,
        wire_cost_per_m: 0.01,
    };
    let r = spot(&p, &default_budget()).unwrap();
    eprintln!(
        "5 km: {} stations, {} states, {} structures, {:.2?}",
        r.stations,
        r.states,
        r.structures.len(),
        r.elapsed
    );
    assert!(r.elapsed < Duration::from_secs(30));
    let placement: Vec<(f64, usize)> = r
        .structures
        .iter()
        .map(|s| (s.station, s.candidate))
        .collect();
    p.evaluate(&placement).unwrap();
}

#[test]
fn exit_clauses_stop_it() {
    let p = flat(
        2000.0,
        vec![
            candidate("DE", None, 20.0, 10.0, None),
            candidate("TAN", None, 20.0, 5.0, None),
        ],
    );
    let r = spot(&p, &Budget::new().max_iterations(10));
    assert!(
        matches!(r, Err(SpotError::Stopped(Stop::IterationLimit { .. }))),
        "{r:?}"
    );
    let token = ohl_core::budget::CancelToken::new();
    token.cancel();
    let r = spot(&p, &Budget::new().cancel_token(token));
    assert!(matches!(r, Err(SpotError::Stopped(Stop::Cancelled { .. }))));
}

#[test]
fn infeasible_and_invalid_problems_say_why() {
    let mut p = flat(1000.0, vec![candidate("DE", None, 4.0, 10.0, None)]);
    // Lower than the clearance: no span is possible.
    assert!(matches!(
        spot(&p, &default_budget()),
        Err(SpotError::Infeasible(_))
    ));
    p.candidates = vec![candidate("TAN", None, 20.0, 5.0, None)];
    assert!(matches!(
        spot(&p, &default_budget()),
        Err(SpotError::Invalid(_))
    ));
    p.candidates = vec![candidate("DE", None, 20.0, 10.0, None)];
    p.max_span = 2000.0;
    assert!(
        p.evaluate(&[(0.0, 0), (1000.0, 0)])
            .unwrap_err()
            .contains("clearance")
    );
}

/// The main route of the synthetic example spotted on its survey with its
/// structure types and example unit costs: a Y pylon (the only type that
/// turns 10°) at the angle point, dead-ends at the ends; the new line
/// resolves and solves, and its wires keep about the clearance over the
/// surface (the profile is the centreline's; the wires run 6 m aside).
#[test]
fn spots_the_synthetic_main_route() {
    use std::path::Path;
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
    let line = ohl_line::Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    let terrain = ohl_terrain::Terrain::load(&ex.join("synthetic-survey.csv")).unwrap();
    let types = ohl_structures::StructureLibrary::load_dir(&ex.join("Structures")).unwrap();
    let insulators = ohl_structures::InsulatorLibrary::default();
    let cables = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let cost = ohl_bom::CostData::load(&ex.join("unit-costs.ohlcost")).unwrap();
    let options = route::RouteOptions {
        route: "main".into(),
        cable: "acsr-drake-26-7".into(),
        horizontal_tension: 20_000.0,
        temperature: 80.0,
        clearance: 7.0,
        step: 20.0,
        max_span: Some(450.0),
        min_span: 50.0,
        forbidden: Vec::new(),
        conductors: None,
    };
    let rp = route::route_problem(
        &line,
        &terrain,
        &types,
        &insulators,
        &cables,
        &cost,
        &options,
    )
    .unwrap();
    assert_eq!(rp.problem.required.len(), 1);
    assert!((rp.problem.required[0].station - 640.0).abs() < 1e-6);
    let r = spot(&rp.problem, &default_budget()).unwrap();
    let text = route::report(&rp, &r, &cost.currency);
    let at_angle = r
        .structures
        .iter()
        .find(|s| (s.station - 640.0).abs() < 1e-6)
        .unwrap();
    assert_eq!(
        rp.problem.candidates[at_angle.candidate].structure_type, "type-03",
        "{text}"
    );
    for end in [r.structures.first().unwrap(), r.structures.last().unwrap()] {
        assert!(rp.problem.candidates[end.candidate].dead_end, "{text}");
    }
    assert!(r.spans.iter().all(|s| s.clearance >= 7.0 - 1e-9), "{text}");

    let spotted = route::spotted_line(&line, &rp, &r, &types, &options);
    assert_eq!(spotted.structures.len(), r.structures.len());
    assert!(!spotted.sections.is_empty());
    let resolved = spotted.resolve(&types, &insulators).unwrap();
    let hot =
        ohl_weather::WeatherCase::still("80 °C", ohl_core::units::Temperature::from_celsius(80.0));
    let mut lowest = f64::INFINITY;
    for shape in ohl_line::solve::solve_line(&resolved, &cables, &hot) {
        for span in shape.unwrap().spans {
            for p in span.points {
                if let Some(z) = terrain.height(p[0], p[1]) {
                    lowest = lowest.min(p[2] - z);
                }
            }
        }
    }
    assert!(lowest > 7.0 - 1.5, "{lowest}\n{text}");
}
