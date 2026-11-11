// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of gap-type conductors on the line (roadmap O22 item 7): the
//! outer strands unloaded in each section's stringing state.

use super::final_stringing_tests::one_span;
use super::*;
use ohl_cables::BimetallicBehavior;
use ohl_core::units::Temperature;

/// The components of the bimetallic textbook cable (textbook input only)
/// as a gap-type conductor.
fn gap_cable() -> Cable {
    toml::from_str(
        "id = \"gap\"\nname = \"gap\"\nkind = \"other\"\nsource = \"textbook input\"\n\
         diameter = \"28.1 mm\"\nmass_per_length = \"1.6285 kg/m\"\n\
         rated_tensile_strength = \"140 kN\"\nbimetallic_behavior = \"gap\"\n\
         outer = { area = \"402.8 mm²\", modulus = \"55 GPa\", thermal_expansion = 23e-6 }\n\
         core = { area = \"65.8 mm²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6 }\n",
    )
    .unwrap()
}

/// The level 300 m span between fixed wire points, strung at 28 kN at
/// `t` °C, solved initially by `method`.
fn span(method: &str, t: f64) -> Line {
    one_span(
        &format!(r#"{{"kind": "tension", "horizontal_tension": 28000, "temperature": {t}}}"#),
        &format!("initial-{method}"),
    )
    .0
}

fn at(line: &Line, cable: &Cable, t: f64) -> SectionShape {
    let case = WeatherCase::still("t", Temperature::from_celsius(t));
    solve_section(line, &line.sections[0], cable, &case, &default_fe_budget()).unwrap()
}

/// The benchmark `tbgap` on the line: strung at 28 kN at 20 °C, the
/// stringing state gives the 28 kN back; at 60 °C the core alone
/// (25 450.60 N: a single-material cable of the core, by the independent
/// reference), at 0 °C both components (31 725.72 N) — by the ruling span
/// to 1e-6, by the finite elements within their thermal form's
/// difference. The same components as a bird cage sag 0.8 m more at
/// 60 °C.
#[test]
fn a_gap_type_has_its_knee_at_the_stringing_temperature() {
    let cable = gap_cable();
    for (method, tolerance) in [("rs", 1e-6), ("fe", 1e-3)] {
        let line = span(method, 20.0);
        for (t, h, sag) in [
            (20.0, 28_000.0, 6.420_484_711),
            (60.0, 25_450.603_53, 7.062_724_233),
            (0.0, 31_725.717_45, 5.667_255_655),
        ] {
            let shape = at(&line, &cable, t);
            let got = shape.spans[0].horizontal_tension;
            assert!(
                (got - h).abs() < tolerance * h,
                "{method} at {t} °C: {got} against {h}"
            );
            assert!(
                (shape.spans[0].sag - sag).abs() < 0.01,
                "{method} at {t} °C: sag {} against {sag}",
                shape.spans[0].sag
            );
            // Past its knee by its nature: said once per line, not per span.
            assert!(shape.flags.is_empty(), "{:?}", shape.flags);
        }
        // The same components as a bird cage (both unstressed together).
        let mut cage = gap_cable();
        cage.bimetallic_behavior = Some(BimetallicBehavior::BirdCage);
        let (gap, bird) = (at(&line, &cable, 60.0), at(&line, &cage, 60.0));
        assert!(
            bird.spans[0].sag > gap.spans[0].sag + 0.7,
            "{method}: bird cage {} gap {}",
            bird.spans[0].sag,
            gap.spans[0].sag
        );
    }
}

/// The knee point is each section's own: strung at 5 °C, the outer
/// strands are slack at 20 °C, where a section strung at 20 °C has them
/// just unloaded and one strung at 35 °C loaded — the offsets differ, and
/// in the stringing state the outer strands carry nothing.
#[test]
fn the_offset_is_the_sections_own() {
    use ohl_cables::mechanics::{MaterialState, OuterState};
    use ohl_core::units::Force;
    let cable = gap_cable();
    let budget = default_fe_budget();
    let mut offsets = Vec::new();
    for t_s in [5.0, 20.0, 35.0] {
        let line = span("rs", t_s);
        let section = &line.sections[0];
        let strung = strung_cable(section, &cable).unwrap();
        let (ns, _, state, h) = strung_section(&line, section, &strung, &budget).unwrap();
        let m = &ns.mechanics;
        // In the stringing state: the outer strands unloaded, the core
        // carrying the mean tension (a little above H).
        let solved = ns.solve(state, &MaterialState::Initial, &budget).unwrap();
        assert!((solved.horizontal_tension.si() - h).abs() < 1e-6);
        let mean = ohl_sagtension::nonlinear::NonlinearSection::section_mean_tension(&solved);
        let strain = m
            .strain(
                Force::from_si(mean),
                state.temperature,
                &MaterialState::Initial,
            )
            .unwrap();
        let loads = m.component_loads(strain, state.temperature, &MaterialState::Initial);
        assert!(loads[0].abs() < 1e-6, "outer {} N at stringing", loads[0]);
        assert!((loads[1] - mean).abs() < 1e-6);
        // At 20 °C: slack when strung colder, loaded when strung warmer.
        let at_20 = m
            .outer_state(
                Force::from_si(mean),
                Temperature::from_celsius(20.0),
                &MaterialState::Initial,
            )
            .unwrap()
            .unwrap();
        match t_s {
            t if t < 20.0 => assert_eq!(at_20, OuterState::Slack),
            t if t > 20.0 => assert_eq!(at_20, OuterState::Tension),
            _ => {}
        }
        offsets.push(m.outer_offset());
    }
    // 2242 µε at 20 °C (the core's strain at 28 034.2 N: no thermal part at
    // the reference temperature), and 23e-6 − 11.5e-6 per kelvin of
    // stringing temperature less.
    assert!((offsets[1] - 2242.38e-6).abs() < 0.05e-6, "{offsets:?}");
    let per_kelvin = (offsets[0] - offsets[2]) / 30.0;
    assert!((per_kelvin - 11.5e-6).abs() < 0.05e-6, "{per_kelvin}");
}

/// What the results say: the gap type is named with its sections, "NOT
/// ON THE SAFE SIDE" (valid only for a conductor installed on its core);
/// a line cannot choose it for its wires; a gap type needs a core.
#[test]
fn a_gap_type_is_said_and_checked() {
    let cable = gap_cable();
    assert!(cable.is_gap());
    assert_eq!(
        cable.bimetallic_model(),
        Some(ohl_cables::BimetallicUse::Gap)
    );
    let note = ohl_cables::BimetallicUse::Gap.note("gap", "section S");
    assert!(
        note.starts_with("NOT ON THE SAFE SIDE: gap-type wire gap (section S)")
            && note.contains("installed otherwise sags more when hot"),
        "{note}"
    );
    assert!(!ohl_cables::BimetallicUse::Gap.is_safe_for_sag());
    // The line's own model: never a gap type.
    let mut line = span("fe", 20.0);
    line.bimetallic.behavior = BimetallicBehavior::Gap;
    let e = line.validate().unwrap_err().to_string();
    assert!(e.contains("a wire type's property"), "{e}");
    // Without a core: refused.
    let mut solid = gap_cable();
    solid.core = None;
    let e = solid.validate().unwrap_err().to_string();
    assert!(e.contains("needs a core"), "{e}");
    // A stringing given after creep: the offset is set in the initial
    // state worked back (here the wire has no creep: the same state).
    let mut after = span("fe", 20.0);
    after.creep_stretch.creep_case = Some("everyday".into());
    after.set_weather_cases(&[WeatherCase::still(
        "everyday",
        Temperature::from_celsius(15.0),
    )]);
    after.sections[0].stringing = Stringing::Tension {
        horizontal_tension: 28_000.0,
        temperature: 20.0,
        condition: crate::StrungAt::Creep,
    };
    after.sections[0].calculation = crate::condition::Calculation::CREEP_FE;
    let shape = at(&after, &cable, 20.0);
    assert!((shape.spans[0].horizontal_tension - 28_000.0).abs() < 1e-3);
}
