// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the cases solved per span against the ruling span and the
//! finite elements.

use super::*;
use crate::condition::Calculation;
use ohl_core::units::{Length, Stress, Temperature};
use std::path::Path;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

fn iced(t: f64, ice_mm: f64, wind: f64) -> WeatherCase {
    WeatherCase {
        ice_thickness: Length::from_millimeters(ice_mm),
        wind_pressure: Stress::from_pascals(wind),
        ..WeatherCase::still("iced", Temperature::from_celsius(t))
    }
}

/// Every span in the section's case: the section solved bit for bit
/// as with its one case — every section of the example (rolling,
/// clipped, marker balls, jumpers), still, hot, iced and windy; also
/// with a span given the section's case as its own.
#[test]
fn one_case_for_every_span_is_the_section_solved_as_before() {
    let line = crate::tests::example();
    let lib = library();
    let budget = default_fe_budget();
    let cases = [
        WeatherCase::still("15", Temperature::from_celsius(15.0)),
        WeatherCase::still("80", Temperature::from_celsius(80.0)),
        iced(-5.0, 20.0, 400.0),
    ];
    let mut solved = 0;
    for case in &cases {
        for section in &line.sections {
            let cable = lib.get(&section.cable).unwrap();
            let one = solve_section(&line, section, cable, case, &budget);
            let all = SpanCases::uniform(case.clone());
            let uniform = solve_section_span_cases(&line, section, cable, &all, &budget);
            let given = all.clone().with_span(0, case.clone());
            assert!(given.is_uniform());
            let own = solve_section_span_cases(&line, section, cable, &given, &budget);
            match (one, uniform, own) {
                (Ok(a), Ok(b), Ok(c)) => {
                    assert_eq!(a, b, "{} {}", section.id, case.name);
                    assert_eq!(a, c, "{} {}", section.id, case.name);
                    solved += 1;
                }
                (Err(a), Err(b), Err(c)) => {
                    assert_eq!(a.to_string(), b.to_string());
                    assert_eq!(a.to_string(), c.to_string());
                }
                (a, b, c) => panic!("{a:?}\n{b:?}\n{c:?}"),
            }
        }
    }
    assert!(solved >= 3 * line.sections.len() - 1, "{solved}");
}

/// One span iced (10 mm, −5 °C) in a section otherwise bare at −5 °C:
/// that span sags more and is the only one with the iced radius; its
/// tension rises, the strings at its ends swing into it (its clamps
/// move towards its middle), and the neighbours are pulled tighter
/// too. With wind, the iced span is blown further than the same span
/// bare in the same wind (the wind on its iced diameter).
#[test]
fn one_iced_span_sags_and_pulls_its_strings_in() {
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    let lib = library();
    let section = &line.sections[0];
    let cable = lib.get(&section.cable).unwrap();
    let budget = default_fe_budget();
    let bare = iced(-5.0, 0.0, 0.0);
    let k = 1;
    let base = solve_section(&line, section, cable, &bare, &budget).unwrap();
    let cases = SpanCases::uniform(bare.clone()).with_span(k, iced(-5.0, 10.0, 0.0));
    assert!(!cases.is_uniform());
    let one = solve_section_span_cases(&line, section, cable, &cases, &budget).unwrap();
    assert!(one.spans[k].sag > base.spans[k].sag + 0.1);
    let r_bare = cable.diameter.si() / 2.0;
    for (s, span) in one.spans.iter().enumerate() {
        let r = if s == k { r_bare + 0.01 } else { r_bare };
        assert!((span.surface_radius - r).abs() < 1e-12, "{s}");
        assert!(
            span.horizontal_tension > base.spans[s].horizontal_tension,
            "{s}"
        );
    }
    // The clamps at the iced span's ends move towards its middle.
    let at = |s: &SectionShape, i: usize| v(s.clamps[i]);
    let towards = |i: usize, j: usize| {
        let d = (at(&one, j) - at(&one, i)).xy().normalize();
        (at(&one, i) - at(&base, i)).xy().dot(&d)
    };
    assert!(towards(k, k + 1) > 0.01, "{}", towards(k, k + 1));
    assert!(towards(k + 1, k) > 0.01, "{}", towards(k + 1, k));
    // Wind on the iced diameter: blown further than bare in the same
    // wind.
    let windy = iced(-5.0, 0.0, 400.0);
    let blown_bare = solve_section(&line, section, cable, &windy, &budget).unwrap();
    let cases = SpanCases::uniform(windy.clone()).with_span(k, iced(-5.0, 10.0, 400.0));
    let blown = solve_section_span_cases(&line, section, cable, &cases, &budget).unwrap();
    let out = |s: &SectionShape| {
        let chord = (v(s.clamps[k]) + v(s.clamps[k + 1])) / 2.0;
        (v(s.spans[k].mid_point) - chord).xy().norm()
    };
    assert!(
        out(&blown) > out(&blown_bare),
        "{} {}",
        out(&blown),
        out(&blown_bare)
    );
}

/// The ruling span has one tension for all spans: spans in different
/// cases are refused; in one case they solve.
#[test]
fn the_ruling_span_takes_one_case_only() {
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    line.sections[0].calculation = Calculation::INITIAL_RS;
    let lib = library();
    let cable = lib.get(&line.sections[0].cable).unwrap();
    let bare = iced(-5.0, 0.0, 0.0);
    let budget = default_fe_budget();
    let all = SpanCases::uniform(bare.clone());
    let section = &line.sections[0];
    assert!(solve_section_span_cases(&line, section, cable, &all, &budget).is_ok());
    let one = all.with_span(1, iced(-5.0, 10.0, 0.0));
    let e = solve_section_span_cases(&line, section, cable, &one, &budget).unwrap_err();
    assert!(e.to_string().contains("finite-element"), "{e}");
}
