// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the local winds per span (roadmap O15) and of the broken
//! wire (roadmap O9).

use super::*;
use crate::condition::Calculation;
use crate::{SpanWind, parse_wind_factors};
use ohl_core::units::{Length, Stress, Temperature};
use std::path::Path;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

fn windy(t: f64, ice_mm: f64, wind: f64) -> WeatherCase {
    WeatherCase {
        ice_thickness: Length::from_millimeters(ice_mm),
        wind_pressure: Stress::from_pascals(wind),
        ..WeatherCase::still("windy", Temperature::from_celsius(t))
    }
}

/// How far span `k`'s middle is blown off its chord, m.
fn blown(s: &SectionShape, k: usize) -> f64 {
    let chord = (v(s.clamps[k]) + v(s.clamps[k + 1])) / 2.0;
    (v(s.spans[k].mid_point) - chord).xy().norm()
}

/// The two level 300 m spans of the validation cases (spancases,
/// broken-wire): dead-end wire points fixed at (0, 0, 27) and
/// (600, 0, 27) m, a suspension string in the middle from (300, 0, 30) m,
/// 3 m, 800 N, 5e7 N; strung at 28 kN at 15 °C in the blocks. INVENTED.
pub(super) fn two_spans() -> (Line, Cable) {
    let cable: Cable = toml::from_str(
        r#"
id = "span-cases"
name = "Synthetic homogeneous test cable"
kind = "aaac"
source = "Synthetic test data, not a real conductor"
diameter = "28 mm"
mass_per_length = "1.6285 kg/m"
rated_tensile_strength = "120 kN"
outer = { area = "400 mm²", modulus = "86.6 GPa", thermal_expansion = 18.9e-6 }
"#,
    )
    .unwrap();
    let line = Line::from_json(
        r#"{
  "format": "ohl-line", "version": 6, "name": "two spans", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "span-cases", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 28000.0, "temperature": 15.0}}]
}"#,
    )
    .unwrap();
    (line, cable)
}

/// A factor of 1 given (or none) solves every section of the example
/// bit for bit as before, in still air and in wind.
#[test]
fn a_factor_of_one_changes_nothing() {
    let line = crate::tests::example();
    let lib = library();
    let budget = default_fe_budget();
    for case in [windy(15.0, 0.0, 0.0), windy(-5.0, 10.0, 450.0)] {
        for section in &line.sections {
            let cable = lib.get(&section.cable).unwrap();
            let mut ones = section.clone();
            ones.wind_factors = vec![SpanWind {
                span: 0,
                factor: 1.0,
            }];
            assert!(!ones.has_local_wind());
            let a = solve_section(&line, section, cable, &case, &budget);
            let b = solve_section(&line, &ones, cable, &case, &budget);
            match (a, b) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "{}", section.id),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                (a, b) => panic!("{a:?}\n{b:?}"),
            }
        }
    }
}

/// The same factor on every span is the case's wind times it: the
/// section solves as in that wind (to rounding), strings included.
#[test]
fn one_factor_on_every_span_is_the_case_times_it() {
    let (mut line, cable) = two_spans();
    let budget = default_fe_budget();
    let case = windy(10.0, 0.0, 500.0);
    let section = line.sections[0].clone();
    let plain = solve_section(
        &line,
        &section,
        &cable,
        &case.with_wind_factor(1.2),
        &budget,
    )
    .unwrap();
    line.sections[0].set_wind_factor(0, 1, 1.2);
    let local = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
    for (a, b) in plain.spans.iter().zip(&local.spans) {
        assert!((a.horizontal_tension - b.horizontal_tension).abs() < 1e-6);
        for k in 0..3 {
            assert!((a.mid_point[k] - b.mid_point[k]).abs() < 1e-8);
        }
    }
    for (a, b) in plain
        .attachment_loads
        .iter()
        .flatten()
        .zip(local.attachment_loads.iter().flatten())
    {
        for k in 0..3 {
            assert!((a.force[k] - b.force[k]).abs() < 1e-6, "{a:?} {b:?}");
        }
    }
    // In still air a factor does nothing.
    let still = windy(10.0, 0.0, 0.0);
    let a = solve_section(&line, &section, &cable, &still, &budget).unwrap();
    let b = solve_section(&line, &line.sections[0], &cable, &still, &budget).unwrap();
    assert_eq!(a, b);
}

/// A factor on one span blows that span further (0: not at all) and
/// leaves the other nearly as in the case's wind; the string between
/// them takes the larger factor.
#[test]
fn a_factor_on_one_span_blows_that_span() {
    let (mut line, cable) = two_spans();
    let budget = default_fe_budget();
    let case = windy(10.0, 0.0, 500.0);
    let base = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
    line.sections[0].set_wind_factor(0, 0, 1.5);
    let more = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
    assert!(
        blown(&more, 0) > 1.2 * blown(&base, 0),
        "{} {}",
        blown(&more, 0),
        blown(&base, 0)
    );
    assert!((blown(&more, 1) - blown(&base, 1)).abs() < 0.2 * blown(&base, 1));
    assert!(more.spans[0].horizontal_tension > base.spans[0].horizontal_tension);
    line.sections[0].set_wind_factor(0, 0, 0.0);
    let none = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
    assert!(blown(&none, 0) < 0.05, "{}", blown(&none, 0));
    assert_eq!(line.sections[0].wind_factor_at_point(1), 1.0);
    assert_eq!(line.sections[0].wind_factor_at_point(0), 0.0);
    // The ruling span takes the largest.
    let mut rs = line.clone();
    rs.sections[0].calculation = Calculation::INITIAL_RS;
    rs.sections[0].set_wind_factor(0, 0, 1.5);
    let a = solve_section(&rs, &rs.sections[0], &cable, &case, &budget).unwrap();
    rs.sections[0].wind_factors.clear();
    let b = solve_section(
        &rs,
        &rs.sections[0],
        &cable,
        &case.with_wind_factor(1.5),
        &budget,
    )
    .unwrap();
    assert_eq!(a, b);
}

/// Local winds as words: runs joined, spans from 1, read back; bad text
/// refused saying why.
#[test]
fn local_winds_in_words() {
    let (mut line, _) = two_spans();
    let s = &mut line.sections[0];
    s.points.push(s.points[2].clone());
    s.points.push(s.points[2].clone());
    s.set_wind_factor(0, 1, 1.2);
    s.set_wind_factor(3, 3, 0.7);
    assert_eq!(s.wind_factors_text(), "1–2: 1.2; 4: 0.7");
    assert_eq!(
        parse_wind_factors(&s.wind_factors_text(), 4).unwrap(),
        s.wind_factors
    );
    assert_eq!(
        parse_wind_factors("1-2: 1.2; 4:0.7", 4).unwrap(),
        s.wind_factors
    );
    assert_eq!(parse_wind_factors("", 4).unwrap(), vec![]);
    assert_eq!(parse_wind_factors("3: 1", 4).unwrap(), vec![]);
    for bad in [
        "5: 1.2",
        "1: x",
        "1: 11",
        "2-1: 1.2",
        "1: 1.2; 1: 1.3",
        "1.2",
    ] {
        assert!(parse_wind_factors(bad, 4).is_err(), "{bad}");
    }
    s.set_wind_factor(0, 3, 1.0);
    assert!(s.wind_factors.is_empty());
}

/// The broken wire, hand-checkable: span 1 of the two spans broken, the
/// string at T2 swings back towards span 0 until the string's pull, its
/// half weight and span 0's end force balance at the clamp — the moment
/// about the string's top: \\( \tan\varphi = H / (V + W_s/2) \\), with V
/// the wire's vertical pull at the clamp; T2 takes span 0's pull along
/// the line; T3 takes nothing.
#[test]
fn a_broken_span_swings_its_string_to_the_other_side() {
    let (line, cable) = two_spans();
    let budget = default_fe_budget();
    let case = windy(10.0, 0.0, 0.0);
    let whole = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
    let s = &line.sections[0];
    let broken =
        solve_section_broken(&line, s, &cable, &case, 1, &budget, &Coupling::default()).unwrap();
    assert_eq!(broken.broken, Some(1));
    assert!(broken.spans[1].points.is_empty());
    assert_eq!(broken.spans[1].horizontal_tension, 0.0);
    // The clamp swung back (towards T1), and up.
    let clamp = broken.clamps[1];
    assert!(clamp[0] < 300.0 - 1.0, "{clamp:?}");
    assert!(clamp[2] > whole.clamps[1][2] + 0.1, "{clamp:?}");
    let string = &broken.strings[1][0];
    let swing = (string.top[0] - string.bottom[0]).atan2(string.top[2] - string.bottom[2]);
    // The equilibrium at the clamp: the string along the wire's pull plus
    // its half weight (the wire's pull on the clamp from `pulls`).
    let pull = broken.pulls[1];
    let h = -pull[0];
    let down = -pull[2] + 400.0;
    assert!(h > 1000.0, "{h}");
    assert!(
        (swing.tan() - h / down).abs() < 1e-6,
        "{} {}",
        swing.tan(),
        h / down
    );
    // T2 takes span 0's pull along the line, T3 nothing.
    let t2: Point = broken.attachment_loads[1].iter().fold([0.0; 3], |a, l| {
        [a[0] + l.force[0], a[1] + l.force[1], a[2] + l.force[2]]
    });
    assert!((t2[0] + h).abs() < 1e-3 * h, "{t2:?} {h}");
    let t3: f64 = broken.attachment_loads[2]
        .iter()
        .map(|l| l.force.iter().map(|f| f.abs()).sum::<f64>())
        .sum();
    assert!(t3 < 1e-9, "{t3}");
    // The remaining span pulls less than whole (its clamp came closer).
    assert!(broken.spans[0].horizontal_tension < whole.spans[0].horizontal_tension);
    // Breaking span 0 is the mirror image.
    let other =
        solve_section_broken(&line, s, &cable, &case, 0, &budget, &Coupling::default()).unwrap();
    assert!((other.clamps[1][0] - (600.0 - clamp[0])).abs() < 1e-6);
    assert!((other.spans[1].horizontal_tension - broken.spans[0].horizontal_tension).abs() < 1e-3);
}

/// What a broken wire refuses: a span the section does not have, the
/// ruling span. A one-span section broken holds nothing at either end.
#[test]
fn what_a_broken_wire_refuses() {
    let (mut line, cable) = two_spans();
    let budget = default_fe_budget();
    let case = windy(10.0, 0.0, 0.0);
    let s = line.sections[0].clone();
    let e = solve_section_broken(&line, &s, &cable, &case, 2, &budget, &Coupling::default())
        .unwrap_err();
    assert!(e.to_string().contains("no span 3"), "{e}");
    let mut rs = s.clone();
    rs.calculation = Calculation::INITIAL_RS;
    let e = solve_section_broken(&line, &rs, &cable, &case, 0, &budget, &Coupling::default())
        .unwrap_err();
    assert!(e.to_string().contains("finite-element"), "{e}");
    // One span: T1 to T2 only; broken, both ends hold no wire: T2's
    // string hangs under its weight.
    line.sections[0].points.truncate(2);
    let one = line.sections[0].clone();
    let shape =
        solve_section_broken(&line, &one, &cable, &case, 0, &budget, &Coupling::default()).unwrap();
    let t2: f64 = shape.attachment_loads[1].iter().map(|l| l.force[2]).sum();
    assert!((t2 + 800.0).abs() < 1e-9, "{t2}");
}

/// The line with a wire broken: the other sections as solved whole.
#[test]
fn the_line_with_a_wire_broken() {
    let line = crate::tests::example();
    let lib = library();
    let budget = default_fe_budget();
    let case = windy(10.0, 0.0, 0.0);
    let whole = solve_line_with(&line, &lib, &case, &budget);
    let s = line
        .sections
        .iter()
        .find(|s| s.points.len() > 2 && s.calculation.method() == crate::condition::Method::Fe)
        .unwrap();
    let broken = solve_line_broken(&line, &lib, &case, &s.id, 0, &budget).unwrap();
    for (sec, (a, b)) in line.sections.iter().zip(whole.iter().zip(&broken)) {
        if sec.id == s.id {
            let b = b.as_ref().unwrap();
            assert_eq!(b.broken, Some(0));
        } else {
            assert_eq!(a.as_ref().ok(), b.as_ref().ok(), "{}", sec.id);
        }
    }
    assert!(solve_line_broken(&line, &lib, &case, "nope", 0, &budget).is_err());
}

/// Jumpers take the full wind square to each piece, like the spans,
/// pushed to the case's side of the line (physics review 2026-10-26: at
/// 80° to the line, 500 Pa on 30 mm, cos² gave 0.45 N/m instead of
/// 15 N/m). A piece along the line takes it all across the line, one
/// across the line all along it (pushed ahead).
#[test]
fn jumpers_take_the_full_wind_square_to_each_piece() {
    let (_, cable) = two_spans();
    let case = windy(10.0, 0.0, 500.0);
    let q = case
        .wire_load(cable.diameter, cable.weight())
        .horizontal
        .si();
    let ahead = [0.0, 1.0];
    let left = nalgebra::Vector2::new(-1.0, 0.0);
    // Wind from the right (the default) blows to the left.
    let to = case.wind_from.to_left();
    for deg in [0.0_f64, 30.0, 80.0, 100.0, 180.0, 260.0] {
        let a = deg.to_radians();
        // `deg` from the line's direction (ahead).
        let d = Vector3::new(-a.sin(), a.cos(), -0.3);
        let w = wire_load_in_line_wind(&case, &cable, d, Some(ahead));
        let h = w.xy();
        assert!(
            (h.norm() - q).abs() < 1e-9 * q,
            "{deg}°: {} of {q}",
            h.norm()
        );
        // Square to the piece, on the side the wind blows to.
        assert!(h.dot(&d.xy()).abs() < 1e-9 * q, "{deg}°");
        assert!(h.dot(&left) * to > 0.0, "{deg}°: {h:?}");
        assert_eq!(
            w.z,
            -case.wire_load(cable.diameter, cable.weight()).vertical.si()
        );
    }
    // Across the line exactly: pushed ahead, the full load.
    let w = wire_load_in_line_wind(&case, &cable, Vector3::new(1.0, 0.0, 0.0), Some(ahead));
    assert!(
        (w.y - q * to).abs() < 1e-9 * q && w.x.abs() < 1e-12,
        "{w:?}"
    );
}
