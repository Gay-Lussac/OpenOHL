// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Speed of the vertical and radial clearance checks (the windows'
//! [`ohl_clearance::zone`]) on an invented long line — `SPANS` spans of
//! 350 m (default 600: 60 tension sections of three phases, 180 sections
//! solved) over gentle hills — with an invented cloud of `POINTS` points
//! (default 10⁶) within ±40 m of the line, read from memory: the time of
//! the solves, of the ground check and of the point check, with the wind
//! stepped and on the swing arc. Ignored by default; run with
//! `SPANS=600 POINTS=1000000 STEP=10 [SCREEN=1] cargo test -p ohl-clearance --release
//! --test zone_speed -- --ignored --nocapture`.

use std::fmt::Write;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

use ohl_clearance::rules::{Requirement, WindSides, WindSteps};
use ohl_clearance::zone::{
    Measure, Zone, ZoneRule, ZoneTargets, arc_cases, check_zones, sections_needed, solve_states,
    wind_states, zone_arcs,
};
use ohl_core::budget::Budget;
use ohl_core::units::{Stress, Temperature};
use ohl_line::Line;
use ohl_terrain::Terrain;
use ohl_weather::WeatherCase;

const SPAN: f64 = 350.0;
const PER_SECTION: usize = 10;

fn env(name: &str, or: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(or)
}

fn ground_z(x: f64, y: f64) -> f64 {
    500.0 + 8.0 * (x / 700.0).sin() + 3.0 * (x / 130.0).cos() + 0.03 * y
}

/// The invented line: towers every 350 m along x, three phases 6 m apart,
/// 30 m up; suspension strings of 3.5 m, tension strings at the ends of
/// every section of ten spans.
pub fn long_line(spans: usize) -> Line {
    let mut structures = Vec::new();
    for i in 0..=spans {
        let x = i as f64 * SPAN;
        let z = ground_z(x, 0.0);
        let dead = i % PER_SECTION == 0 || i == spans;
        let mut points = Vec::new();
        let mut strings = Vec::new();
        let mut wires = Vec::new();
        for (k, y) in [(1, -6.0), (2, 0.0), (3, 6.0)] {
            points.push(format!(
                r#"{{"label": "A{k}", "position": [{x}, {y}, {}]}}"#,
                z + 30.0
            ));
            if dead {
                let mut sides = Vec::new();
                if i > 0 {
                    sides.push((k, 180.0, -5.0));
                }
                if i < spans {
                    sides.push((10 + k, 0.0, 5.0));
                }
                for (set, dir, dx) in sides {
                    strings.push(format!(
                        r#"{{"label": "S{set}", "top": "A{k}", "wire": "W{set}", "length": 5.024937810560445, "angle_deg": 84.28940686250036, "direction_deg": {dir}}}"#
                    ));
                    wires.push(format!(
                        r#"{{"label": "W{set}", "position": [{}, {y}, {}], "set": {set}}}"#,
                        x + dx,
                        z + 29.5
                    ));
                }
            } else {
                strings.push(format!(
                    r#"{{"label": "S{k}", "top": "A{k}", "wire": "W{k}", "length": 3.5, "weight": 900.0}}"#
                ));
                wires.push(format!(
                    r#"{{"label": "W{k}", "position": [{x}, {y}, {}], "set": {k}}}"#,
                    z + 26.5
                ));
            }
        }
        structures.push(format!(
            r#"{{"id": "T{i}", "position": [{x}, 0.0, {z}], "orientation_deg": -90.0, "points": [{}], "strings": [{}], "wires": [{}]}}"#,
            points.join(", "),
            strings.join(", "),
            wires.join(", ")
        ));
    }
    let mut sections = Vec::new();
    let mut start = 0;
    while start < spans {
        let end = (start + PER_SECTION).min(spans);
        for k in 1..=3 {
            let mut pts = vec![format!(r#"{{"structure": "T{start}", "set": {}}}"#, 10 + k)];
            for i in start + 1..=end {
                pts.push(format!(r#"{{"structure": "T{i}", "set": {k}}}"#));
            }
            sections.push(format!(
                r#"{{"id": "S{start}-L{k}", "cable": "acsr-drake-26-7", "circuit_name": "C", "phase_label": "L{k}", "system": "ex220", "bundle": 1, "points": [{}], "stringing": {{"kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0}}}}"#,
                pts.join(", ")
            ));
        }
        start = end;
    }
    let text = format!(
        r#"{{"format": "ohl-line", "version": 1, "name": "invented long line", "structures": [{}], "sections": [{}], "jumpers": [], "systems": [{{"id": "ex220", "name": "220 kV", "kind": "ac", "voltage_kv": 220.0, "frequency_hz": 50.0, "phases": [{{"label": "L1", "angle_deg": 0.0}}, {{"label": "L2", "angle_deg": -120.0}}, {{"label": "L3", "angle_deg": 120.0}}]}}]}}"#,
        structures.join(",\n"),
        sections.join(",\n")
    );
    Line::from_json(&text).unwrap()
}

/// The ground: a TIN of 10 m points over ±60 m.
pub fn ground(spans: usize) -> Terrain {
    let mut pts = Vec::new();
    let length = spans as f64 * SPAN;
    let mut x = -50.0;
    while x <= length + 50.0 {
        let mut y = -60.0;
        while y <= 60.0 {
            pts.push([x, y, ground_z(x, y)]);
            y += 10.0;
        }
        x += 10.0;
    }
    Terrain::from_xyz(&pts, None).unwrap()
}

/// `n` invented points over ±40 m of the line, 0–28 m above the ground,
/// of classes 3, 4 and 5 (a fixed sequence: the same cloud each run).
pub fn cloud(spans: usize, n: usize) -> Vec<([f64; 3], u8)> {
    let length = spans as f64 * SPAN;
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            let x = next() * length;
            let y = (next() - 0.5) * 80.0;
            let h = next() * 28.0;
            let c = 3 + (next() * 3.0) as u8;
            ([x, y, ground_z(x, y) + h], c)
        })
        .collect()
}

fn windy() -> WeatherCase {
    let mut c = WeatherCase::still("high wind", Temperature::from_celsius(15.0));
    c.wind_pressure = Stress::from_si(650.0);
    c
}

fn rules() -> Vec<ZoneRule> {
    vec![
        ZoneRule {
            name: "Ground".into(),
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: Requirement::Fixed(7.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
        },
        ZoneRule {
            name: "Vegetation".into(),
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: Requirement::Fixed(5.0),
            classes: vec![3, 4, 5],
            ground: false,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
        },
    ]
}

/// A screen (`SCREEN=1`, owner 2026-10-27): every point of class 6
/// within 5 m + 0.01 m/kV of a wire in plan, at any height (a design
/// code's horizontal distance).
fn screen() -> ZoneRule {
    ZoneRule {
        name: "Screen".into(),
        voltages: Vec::new(),
        cases: Vec::new(),
        distance: Requirement::Formula {
            base: 5.0,
            per_kv: 0.01,
            above_kv: 0.0,
        },
        classes: vec![6],
        ground: false,
        surface: None,
        wires: ohl_clearance::Wires::Sections,
        width: None,
        screen: true,
    }
}

#[test]
#[ignore]
fn speed() {
    let spans = env("SPANS", 600);
    let n = env("POINTS", 1_000_000);
    let step = env("STEP", 50) as f64;
    let line = long_line(spans);
    let library = ohl_cables::Library::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let terrain = ground(spans);
    let points = cloud(spans, n);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut text = format!(
        "{spans} spans, {} sections, {n} points, {threads} threads\n",
        line.sections.len()
    );
    let mut rules = rules();
    // With a screen (`SCREEN=1`): one point in seven of class 6.
    let mut points = points;
    if env("SCREEN", 0) == 1 {
        rules.push(screen());
        for (k, p) in points.iter_mut().enumerate() {
            if k % 7 == 0 {
                p.1 = 6;
            }
        }
    }
    for (what, wind) in [
        (
            format!("steps {step} Pa"),
            WindSteps {
                stepped: true,
                step,
                sides: WindSides::Both,
                arc: false,
                ..Default::default()
            },
        ),
        (
            "arc".into(),
            WindSteps {
                stepped: false,
                step,
                sides: WindSides::Both,
                arc: true,
                ..Default::default()
            },
        ),
    ] {
        let states = wind_states(&[windy()], |_| true, &wind);
        let budget = Budget::new();
        let t = Instant::now();
        let solved = solve_states(
            &line,
            &library,
            &states,
            &sections_needed(&line, &rules),
            false,
            &budget,
            &AtomicUsize::new(0),
        )
        .unwrap();
        let solving = t.elapsed();
        let t = Instant::now();
        let cases = if wind.arc {
            arc_cases(&states)
        } else {
            Vec::new()
        };
        let (arcs, _) = zone_arcs(
            &line,
            &solved.shapes,
            &solved.jumpers,
            &cases,
            wind.margins(),
        );
        let arcs_made = t.elapsed();
        for measure in [Measure::Vertical, Measure::Radial] {
            let zone = Zone::new(measure);
            // Each rule alone, then all of them in one check (a preset's
            // rules: one pass through the points).
            let mut runs: Vec<(String, Vec<ZoneRule>)> = rules
                .iter()
                .map(|r| (r.name.clone(), vec![r.clone()]))
                .collect();
            runs.push(("all rules".into(), rules.clone()));
            for (k, (name, these)) in runs.iter().enumerate() {
                if measure == Measure::Radial && these.iter().any(|r| r.screen) {
                    continue;
                }
                let t = Instant::now();
                let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
                    for (p, c) in &points {
                        visit(p[0], p[1], p[2], *c);
                    }
                    Ok(())
                };
                let report = check_zones(
                    &line,
                    &zone,
                    these,
                    &states,
                    &solved.shapes,
                    &solved.jumpers,
                    &arcs,
                    ZoneTargets {
                        ground: Some(&terrain),
                        surfaces: &[],
                        points: Some(&mut feed),
                        filter: &Default::default(),
                    },
                    &budget,
                )
                .unwrap();
                let took = t.elapsed();
                let low = report
                    .lowest
                    .iter()
                    .flatten()
                    .map(|f| f.distance)
                    .fold(f64::NAN, f64::min);
                let _ = writeln!(
                    text,
                    "{what}: {} solves in {solving:.2?} (arcs {arcs_made:.2?}); {} {name}: {took:.2?}{}; lowest {low:.3} m, {} spans below",
                    states.len(),
                    measure.as_str(),
                    if k == 1 {
                        format!(
                            " ({:.2} µs a point)",
                            took.as_secs_f64() * 1e6 / n.max(1) as f64
                        )
                    } else {
                        String::new()
                    },
                    report.findings.len()
                );
            }
        }
    }
    eprintln!("{text}");
}
