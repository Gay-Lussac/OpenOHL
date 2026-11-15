// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wind at any direction on the wires (owner 2026-11-15): the hand
//! cases of the plan — a level 300 m span of a 30 mm wire at 500 Pa
//! (4500 N across, 3182 N at 45° by the projected area, 2250 N by the
//! cross-flow principle), friction along the wire, the vertical part on an
//! inclined span, an angle structure — and a direction across the line
//! solved bit for bit as the case without one. Every line and value here
//! is INVENTED.

use super::*;
use crate::wind::{Friction, WireWindOptions};
use ohl_core::units::{Stress, Temperature};
use ohl_weather::{ObliqueLaw, WindDirection};

/// A synthetic 30 mm wire.
fn cable() -> Cable {
    toml::from_str(
        r#"
id = "thirty"
name = "Synthetic 30 mm test cable"
kind = "aaac"
source = "Synthetic test data, not a real conductor"
diameter = "30 mm"
mass_per_length = "1.6 kg/m"
rated_tensile_strength = "150 kN"
outer = { area = "500 mm²", modulus = "60 GPa", thermal_expansion = 19e-6 }
"#,
    )
    .unwrap()
}

/// Dead-end to dead-end: the wire points of `ends` (x, y, z), one span
/// per pair, the middle ones on 3 m suspension strings, strung at 30 kN at
/// 10 °C; with a route through the structures when `route`. INVENTED.
fn line_through(ends: &[[f64; 3]], route: bool) -> Line {
    let n = ends.len();
    let structures: Vec<String> = ends
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let strings = if k > 0 && k + 1 < n {
                format!(
                    r#""points": [{{"label": "A1", "position": [{}, {}, {}]}}],
                       "strings": [{{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                                     "weight": 500.0, "stiffness": 5e7, "wind_area": 0.0}}],"#,
                    p[0],
                    p[1],
                    p[2] + 3.0
                )
            } else {
                String::new()
            };
            format!(
                r#"{{"id": "T{k}", "position": [{}, {}, 0], "orientation_deg": 0, {strings}
                   "wires": [{{"label": "W1", "position": [{}, {}, {}], "set": 1}}]}}"#,
                p[0], p[1], p[0], p[1], p[2]
            )
        })
        .collect();
    let points: Vec<String> = (0..n)
        .map(|k| format!(r#"{{"structure": "T{k}", "set": 1}}"#))
        .collect();
    let routing = if route {
        let pis: Vec<String> = ends
            .iter()
            .enumerate()
            .map(|(k, p)| {
                format!(
                    r#"{{"id": "P{k}", "position": [{}, {}, 0], "structure": "T{k}"}}"#,
                    p[0], p[1]
                )
            })
            .collect();
        let ids: Vec<String> = (0..n).map(|k| format!("\"P{k}\"")).collect();
        format!(
            r#""routes": [{{"id": "main", "name": "main"}}],
               "alignments": [{{"id": "main", "route": "main", "points": [{}]}}],
               "alignment_points": [{}],"#,
            ids.join(", "),
            pis.join(", ")
        )
    } else {
        String::new()
    };
    Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 6, "name": "wind directions", "crs": "local",
             {routing}
             "structures": [{}],
             "sections": [{{"id": "S", "cable": "thirty", "bundle": 1, "points": [{}],
               "stringing": {{"kind": "tension", "horizontal_tension": 30000.0,
                              "temperature": 10.0}}}}]}}"#,
        structures.join(", "),
        points.join(", ")
    ))
    .unwrap()
}

fn wind(direction: Option<WindDirection>) -> WeatherCase {
    WeatherCase {
        wind_pressure: Stress::from_pascals(500.0),
        wind_direction: direction,
        ..WeatherCase::still("wind", Temperature::from_celsius(10.0))
    }
}

fn solve(line: &Line, case: &WeatherCase) -> SectionShape {
    solve_section(
        line,
        &line.sections[0],
        &cable(),
        case,
        &default_fe_budget(),
    )
    .unwrap()
}

/// The sum of the forces on every support, N (line axes): the loads on
/// the wire, all told.
fn total(s: &SectionShape) -> Vector3<f64> {
    s.attachment_loads
        .iter()
        .flatten()
        .map(|l| v(l.force))
        .sum()
}

/// The wire's length in span `k`, m (its polyline).
fn arc(s: &SectionShape, k: usize) -> f64 {
    s.spans[k]
        .points
        .windows(2)
        .map(|w| (v(w[1]) - v(w[0])).norm())
        .sum()
}

fn with(line: &Line, options: WireWindOptions) -> Line {
    let mut l = line.clone();
    l.solver.wind = options;
    l
}

/// A direction across the line is the across rule from that side: 90° the
/// case from the right, 270° the case from the left, bit for bit (read
/// from a file the direction is kept as the case's own, 2026-11-15 — its
/// side the same, no wind at an angle).
#[test]
fn across_directions_are_the_old_cases_bit_for_bit() {
    let line = line_through(
        &[[0.0, 0.0, 30.0], [300.0, 0.0, 30.0], [600.0, 0.0, 30.0]],
        true,
    );
    let read = |extra: &str| {
        ohl_weather::WeatherCases::parse(&format!(
            "[[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n{extra}"
        ))
        .unwrap()
        .cases
        .remove(0)
    };
    // The case as before but for its own direction kept.
    let own = |c: ohl_weather::WeatherCase| ohl_weather::WeatherCase {
        wind_direction: None,
        ..c
    };
    let right = read("");
    assert_eq!(own(read("wind_direction = 90")), right);
    assert_eq!(own(read("wind_direction = \"right\"")), right);
    assert_eq!(read("wind_direction = \"across\""), right);
    let left = read("wind_from = \"left\"");
    assert_eq!(own(read("wind_direction = 270")), left);
    assert_eq!(own(read("wind_direction = \"left\"")), left);
    assert_eq!(read("wind_direction = 270").oblique(), None);
    assert_eq!(
        solve(&line, &read("wind_direction = 90")),
        solve(&line, &right)
    );
    assert_eq!(
        solve(&line, &read("wind_direction = 270")),
        solve(&line, &left)
    );
    // A case with a direction built in code takes the across rule too.
    let built = wind(Some(WindDirection::Angle(270.0)));
    assert_eq!(built.oblique(), None);
    assert_eq!(
        solve(&line, &built.with_direction(WindDirection::Angle(270.0))),
        solve(&line, &left)
    );
}

/// The plan's hand case: a level 300 m span, 30 mm, 500 Pa — across
/// 4500 N (per metre of the wire as it hangs: a little more, its length);
/// at 45° 3182 N by the projected area (the default), 2250 N by the
/// cross-flow principle; from ahead none across.
#[test]
fn a_level_span_at_an_angle() {
    let line = line_through(&[[0.0, 0.0, 30.0], [300.0, 0.0, 30.0]], false);
    let qd = 500.0 * 0.030;
    let across = solve(&line, &wind(None));
    let t = total(&across);
    let l = arc(&across, 0);
    // Wind from the right (the default) blows to −y... the line runs +x,
    // its left is +y: the wires blown to the left, the supports pushed +y.
    assert!((t.y - qd * l).abs() < 1e-3 * qd * l, "{t:?} {l}");
    assert!((qd * 300.0 - 4500.0).abs() < 1e-9);
    let at45 = solve(&line, &wind(Some(WindDirection::Angle(45.0))));
    let t45 = total(&at45);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    // From 45° (ahead right) it blows back and to the left: the normal
    // part on a wire along x is the y part, q D sin 45° per metre.
    let l45 = arc(&at45, 0);
    assert!((t45.y - qd * s * l45).abs() < 1e-3 * qd * l45, "{t45:?}");
    assert!(t45.x.abs() < 1e-3 * qd * l45, "no force along: {t45:?}");
    assert!((qd * s * 300.0 - 3181.98).abs() < 0.01);
    let cross = solve(
        &with(
            &line,
            WireWindOptions {
                law: ObliqueLaw::CrossFlow,
                friction: None,
            },
        ),
        &wind(Some(WindDirection::Angle(45.0))),
    );
    let lc = arc(&cross, 0);
    assert!((total(&cross).y - qd * 0.5 * lc).abs() < 1e-3 * qd * lc);
    assert!((qd * 0.5 * 300.0 - 2250.0).abs() < 1e-9);
    // From ahead: along the wire, nothing across it.
    let ahead = solve(&line, &wind(Some(WindDirection::Ahead)));
    assert!(
        total(&ahead).xy().norm() < 1e-3 * qd * l,
        "{:?}",
        total(&ahead)
    );
}

/// Friction along the wire (owner 2026-11-15: an option, off by default):
/// from ahead the wire takes c · q · D per metre along it, blowing back
/// (−x); off, none. The coefficient here is an INVENTED test value.
#[test]
fn friction_along_the_wire() {
    let line = line_through(&[[0.0, 0.0, 30.0], [300.0, 0.0, 30.0]], false);
    let qd = 500.0 * 0.030;
    let c = 0.05;
    let on = with(
        &line,
        WireWindOptions {
            law: ObliqueLaw::Projected,
            friction: Some(Friction::new(c).unwrap()),
        },
    );
    let ahead = wind(Some(WindDirection::Ahead));
    let off = solve(&line, &ahead);
    let with_friction = solve(&on, &ahead);
    let l = arc(&with_friction, 0);
    assert!(total(&off).x.abs() < 1e-6, "{:?}", total(&off));
    let tx = total(&with_friction).x;
    assert!(
        (tx + c * qd * l).abs() < 1e-3 * c * qd * l,
        "{tx} vs {}",
        -c * qd * l
    );
    // At 45°: c · q · D · cos² 45° along the wire, and the normal part.
    let at45 = solve(&on, &wind(Some(WindDirection::Angle(45.0))));
    let l = arc(&at45, 0);
    assert!((total(&at45).x + c * qd * 0.5 * l).abs() < 1e-3 * qd * l);
    // Across, friction or not: the across rule, unchanged.
    assert_eq!(solve(&on, &wind(None)), solve(&line, &wind(None)));
}

/// A span rising 30°, the wind along it (cross-flow): 3.75 N/m normal to
/// the wire, 3.25 N/m of it vertical — down for a wind blowing uphill,
/// up for one blowing downhill.
#[test]
fn the_vertical_part_on_an_inclined_span() {
    let rise = 300.0 * 30f64.to_radians().tan();
    let line = with(
        &line_through(&[[0.0, 0.0, 30.0], [300.0, 0.0, 30.0 + rise]], false),
        WireWindOptions {
            law: ObliqueLaw::CrossFlow,
            friction: None,
        },
    );
    let still = solve(
        &line,
        &WeatherCase::still("still", Temperature::from_celsius(10.0)),
    );
    let uphill = solve(&line, &wind(Some(WindDirection::Back)));
    let downhill = solve(&line, &wind(Some(WindDirection::Ahead)));
    // Per metre of the chord's direction: the part normal to it.
    let n = 15.0 * 0.5 * (0.5 * 30f64.to_radians().cos());
    assert!((n - 3.2476).abs() < 1e-3, "{n}");
    let (lu, ld) = (arc(&uphill, 0), arc(&downhill, 0));
    let dz_up = total(&uphill).z - total(&still).z;
    let dz_down = total(&downhill).z - total(&still).z;
    // The supports carry the wire's loads: more down uphill.
    assert!(
        (dz_up + n * lu).abs() < 2e-3 * n * lu,
        "{dz_up} vs {}",
        -n * lu
    );
    assert!(
        (dz_down - n * ld).abs() < 2e-3 * n * ld,
        "{dz_down} vs {}",
        n * ld
    );
}

/// A 20° angle structure on a route: each span takes the wind relative to
/// its own route segment, the part normal to it; the supports carry it all
/// (the sum by hand from the solved lengths). From ahead (along the line)
/// the spans take none.
#[test]
fn an_angle_structure() {
    let d = 20f64.to_radians();
    let ends = [
        [0.0, 0.0, 30.0],
        [300.0, 0.0, 30.0],
        [300.0 + 300.0 * d.cos(), 300.0 * d.sin(), 30.0],
    ];
    let line = line_through(&ends, true);
    let qd = 15.0;
    let case = wind(Some(WindDirection::Angle(45.0)));
    let shape = solve(&line, &case);
    // The chords the loads act on: the clamps as strung (still air at the
    // stringing temperature; the suspension clamp swung into the angle).
    let strung = solve(
        &line,
        &WeatherCase::still("still", Temperature::from_celsius(10.0)),
    );
    let mut expected = Vector3::zeros();
    for k in 0..2 {
        let a = v(strung.clamps[k + 1]) - v(strung.clamps[k]);
        let route = Vector3::from(ends[k + 1]) - Vector3::from(ends[k]);
        let ahead = route.xy() / route.xy().norm();
        let [x, y] = case.towards_at([ahead.x, ahead.y]).unwrap();
        let w = Vector3::new(x, y, 0.0);
        let t = a / a.norm();
        let normal = w - t * w.dot(&t);
        expected += normal * (qd * arc(&shape, k));
    }
    let got = total(&shape);
    assert!(
        // (Within 0.5 %: the chords the loads act on are the reference
        // clamps', the suspension clamp a little off the still-air one.)
        (got.xy() - expected.xy()).norm() < 5e-3 * expected.norm(),
        "{got:?} vs {expected:?}"
    );
    // Along the line: almost no wind on either span (the strung chords
    // lie a little off the route's segments: the suspension clamp hangs
    // into the angle; under 1 % of the across load).
    let along = solve(&line, &wind(Some(WindDirection::Ahead)));
    assert!(
        total(&along).xy().norm() < 0.01 * qd * 600.0,
        "{:?}",
        total(&along)
    );
}

/// The options are written only when not the defaults (line file version
/// 20), and read back; a negative friction is refused.
#[test]
fn the_options_round_trip() {
    let line = line_through(&[[0.0, 0.0, 30.0], [300.0, 0.0, 30.0]], false);
    let plain = line.to_json();
    assert!(!plain.contains("\"wind\""), "{plain}");
    assert!(!plain.contains(&format!("\"version\": {}", crate::LINE_FILE_VERSION)));
    let set = with(
        &line,
        WireWindOptions {
            law: ObliqueLaw::CrossFlow,
            friction: Some(Friction::new(0.02).unwrap()),
        },
    );
    let text = set.to_json();
    assert!(text.contains("\"version\": 20"), "{text}");
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.solver.wind, set.solver.wind);
    assert!(Friction::new(-0.1).is_err() && Friction::new(f64::NAN).is_err());
    let bad = text.replace("0.02", "-0.02");
    assert!(Line::from_json(&bad).is_err());
}

/// A section entered against the line gives the same loads as the same
/// section entered along it (the wind is the line's, not the section's).
#[test]
fn a_section_against_the_line() {
    let ends = [[0.0, 0.0, 30.0], [300.0, 0.0, 30.0], [600.0, 0.0, 30.0]];
    let along = line_through(&ends, true);
    let mut against = along.clone();
    against.sections[0].points.reverse();
    let case = wind(Some(WindDirection::Angle(60.0)));
    let (a, b) = (total(&solve(&along, &case)), total(&solve(&against, &case)));
    assert!((a - b).norm() < 1e-3 * a.norm(), "{a:?} vs {b:?}");
}
