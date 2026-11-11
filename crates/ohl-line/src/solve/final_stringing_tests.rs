// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of a stringing given in a final condition and by catenary
//! constant (roadmap O22 item 3).

use super::*;
use crate::StrungAt;
use crate::condition::{Calculation, Condition, Method};
use ohl_core::units::Temperature;
use std::path::Path;

/// The example line's first section alone as at stringing (no jumpers,
/// no point loads), on the synthetic cable with curves and creep (invented
/// data), with its creep and load cases.
fn strung_line() -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut lib = ohl_cables::Library::load(&dir.join("../../libraries/cables")).unwrap();
    let path = dir.join("../ohl-sagtension/tests/data/synthetic-cable.ohlw");
    lib.add_toml(&path, &std::fs::read_to_string(&path).unwrap())
        .unwrap();
    let mut line = crate::tests::example();
    line.jumpers.clear();
    line.sections[0].cable = "synthetic".into();
    line.sections[0].clear_point_loads();
    let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
    let mut ice = WeatherCase::still("heavy ice", Temperature::from_celsius(-5.0));
    ice.ice_thickness = ohl_core::units::Length::from_millimeters(15.0);
    line.creep_stretch.creep_case = Some("everyday".into());
    line.creep_stretch.load_cases = vec!["heavy ice".into()];
    line.set_weather_cases(&[every, ice]);
    (line, lib)
}

fn mean_h(shape: &SectionShape) -> f64 {
    shape
        .spans
        .iter()
        .map(|s| s.horizontal_tension)
        .sum::<f64>()
        / shape.spans.len() as f64
}

/// A value given after creep or after load holds in that condition:
/// solved in still air at the stringing temperature by the section's own
/// method, the section's mean horizontal tension is the tension asked —
/// given itself, by a catenary constant (H = C w) or by a sag — and the
/// initial tension worked back is the higher one.
#[test]
fn a_final_value_holds_in_its_condition() {
    let (mut line, lib) = strung_line();
    let cable = lib.get("synthetic").unwrap();
    let budget = default_fe_budget();
    let still = WeatherCase::still("15", Temperature::from_celsius(15.0));
    for method in [Method::Fe, Method::Rs] {
        for condition in [StrungAt::Creep, StrungAt::Load] {
            let kinds = [
                Stringing::Tension {
                    horizontal_tension: 22_000.0,
                    temperature: 15.0,
                    condition,
                },
                Stringing::CatenaryConstant {
                    constant: 1500.0,
                    temperature: 15.0,
                    condition,
                },
                Stringing::Sag {
                    sag: 9.2,
                    temperature: 15.0,
                    span: None,
                    condition,
                },
            ];
            for stringing in kinds {
                let s = &mut line.sections[0];
                s.stringing = stringing.clone();
                s.calculation = Calculation::of(condition.as_condition(), method);
                let section = &line.sections[0];
                let back = worked_back(&line, section, cable, &budget)
                    .unwrap()
                    .unwrap();
                let shape = solve_section(&line, section, cable, &still, &budget).unwrap();
                let h = mean_h(&shape);
                assert!(
                    (h - back.target).abs() < 1e-6 * back.target,
                    "{method:?} {stringing:?}: {h} for {}",
                    back.target
                );
                assert!(back.initial > back.target * 1.001, "{back:?}");
                assert!(back.solves > 2 && back.solves <= WORK_BACK_SOLVES);
                assert!(back.text().contains(condition.label()), "{}", back.text());
                match stringing {
                    Stringing::Tension { .. } => assert_eq!(back.target, 22_000.0),
                    Stringing::CatenaryConstant { .. } => {
                        let w = strung_cable(section, cable).unwrap().weight().si();
                        assert!((h / w - 1500.0).abs() < 1e-3, "H / w = {}", h / w);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The same number taken as initial is today's reference, bit for bit:
/// an initial tension, and a catenary constant against the tension it is.
#[test]
fn an_initial_value_is_the_reference_as_before() {
    let (mut line, lib) = strung_line();
    let cable = lib.get("synthetic").unwrap();
    let budget = default_fe_budget();
    let hot = WeatherCase::still("50", Temperature::from_celsius(50.0));
    line.sections[0].stringing = Stringing::tension(22_000.0, 15.0);
    let section = &line.sections[0];
    let back = worked_back(&line, section, cable, &budget)
        .unwrap()
        .unwrap();
    assert_eq!(
        (back.target, back.initial, back.solves),
        (22_000.0, 22_000.0, 0)
    );
    let as_tension = solve_section(&line, section, cable, &hot, &budget).unwrap();
    let w = strung_cable(section, cable).unwrap().weight().si();
    let mut by_constant = line.clone();
    by_constant.sections[0].stringing = Stringing::CatenaryConstant {
        constant: 22_000.0 / w,
        temperature: 15.0,
        condition: StrungAt::Initial,
    };
    let c = solve_section(&by_constant, &by_constant.sections[0], cable, &hot, &budget).unwrap();
    for (a, b) in as_tension.spans.iter().zip(&c.spans) {
        assert!((a.horizontal_tension - b.horizontal_tension).abs() < 1e-6);
        assert!((a.sag - b.sag).abs() < 1e-9);
    }
    // Clipped in from a final value: the initial cut lengths, those of the
    // initial tension found.
    line.sections[0].stringing = Stringing::Tension {
        horizontal_tension: 22_000.0,
        temperature: 15.0,
        condition: StrungAt::Creep,
    };
    line.sections[0].calculation = Calculation::CREEP_FE;
    let found = worked_back(&line, &line.sections[0], cable, &budget)
        .unwrap()
        .unwrap();
    let (cut, _) = clip_lengths(&line, &line.sections[0], cable).unwrap();
    let mut initial = line.clone();
    initial.sections[0].stringing = Stringing::tension(found.initial, 15.0);
    let (same, _) = clip_lengths(&initial, &initial.sections[0], cable).unwrap();
    assert_eq!(cut, same);
}

/// One level 300 m span between fixed wire points, a linear cable whose
/// creep is a temperature shift of 20 K (w 15.970 N/m, EA 34.656 MN,
/// α 18.8514e-6 /K), sagged to 25 kN after creep at 15 °C.
pub(super) fn one_span(stringing: &str, calculation: &str) -> (Line, Cable) {
    let cable: Cable = toml::from_str(
        "id = \"hand\"\nname = \"hand\"\nkind = \"aaac\"\nsource = \"invented test data\"\n\
         diameter = \"28.1 mm\"\nmass_per_length = \"1.6285 kg/m\"\n\
         rated_tensile_strength = \"140 kN\"\nmodel = \"linear_creep_temperature\"\n\
         creep_temperature_shift = 20.0\n\
         outer = { area = \"400 mm²\", modulus = \"86.64 GPa\", thermal_expansion = 18.8514e-6 }\n",
    )
    .unwrap();
    let mut line = Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 6, "name": "hand", "crs": "local",
 "creep_stretch": {{"creep_case": "everyday", "load_cases": ["everyday"]}},
 "structures": [
  {{"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
   "wires": [{{"label": "W1", "position": [0, 0, 30], "set": 1}}]}},
  {{"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
   "wires": [{{"label": "W1", "position": [300, 0, 30], "set": 1}}]}}
 ],
 "sections": [{{"id": "S", "cable": "hand", "bundle": 1, "calculation": "{calculation}",
  "points": [{{"structure": "T1", "set": 1}}, {{"structure": "T2", "set": 1}}],
  "stringing": {stringing}}}]
}}"#
    ))
    .unwrap();
    line.set_weather_cases(&[WeatherCase::still(
        "everyday",
        Temperature::from_celsius(15.0),
    )]);
    (line, cable)
}

/// By hand (parabola): with the creep a temperature shift of 20 K, the
/// change of state from the initial tension \\( H_0 \\) to the 25 kN after
/// creep is \\( H_0^2 (H_0 + K) = EA\, w^2 S^2 / 24 \\) with
/// \\( K = -H_f + EA\, w^2 S^2 / (24 H_f^2) - EA\, \alpha \cdot 20 \\) =
/// 14 966 N and the right side 3.31457e13: \\( H_0 \\) = 27 830 N. The
/// catenary and the weight at 15 °C move it by less than 0.1 %.
#[test]
fn the_initial_tension_of_a_linear_cable_by_hand() {
    let (ea, w, s, alpha) = (34.656e6_f64, 15.970_f64, 300.0_f64, 18.8514e-6_f64);
    let h_f = 25_000.0_f64;
    let right = ea * w * w * s * s / 24.0;
    let k = -h_f + right / (h_f * h_f) - ea * alpha * 20.0;
    assert!((k - 14_966.0).abs() < 2.0, "{k}");
    // Newton on the cubic.
    let mut h0 = h_f;
    for _ in 0..50 {
        h0 -= (h0 * h0 * (h0 + k) - right) / (3.0 * h0 * h0 + 2.0 * h0 * k);
    }
    assert!((h0 - 27_830.0).abs() < 1.0, "{h0}");
    let budget = default_fe_budget();
    for calculation in ["creep-fe", "creep-rs"] {
        let (line, cable) = one_span(
            r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15, "condition": "creep"}"#,
            calculation,
        );
        let back = worked_back(&line, &line.sections[0], &cable, &budget)
            .unwrap()
            .unwrap();
        assert!(
            (back.initial - h0).abs() < 1e-3 * h0,
            "{calculation}: {} against {h0} by hand",
            back.initial
        );
        assert_eq!(
            back.text(),
            format!(
                "sagged to 25.00 kN after creep at 15 °C: {:.2} kN initial",
                back.initial / 1000.0
            )
        );
    }
    // After load a linear cable has no permanent stretch: the same as
    // initial, and said.
    let (line, cable) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15, "condition": "load"}"#,
        "load-fe",
    );
    let back = worked_back(&line, &line.sections[0], &cable, &budget)
        .unwrap()
        .unwrap();
    assert_eq!(back.initial, 25_000.0);
    assert!(
        back.text().contains("no permanent stretch"),
        "{}",
        back.text()
    );
}

/// What cannot be: a tension after creep that needs more than the wire's
/// strength initially, a condition without its weather case, Max Sag as a
/// stringing condition.
#[test]
fn what_cannot_be_strung_is_said() {
    let budget = default_fe_budget();
    let (mut line, mut cable) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15, "condition": "creep"}"#,
        "creep-fe",
    );
    // An RTS just above the value asked: the 27.8 kN needed are beyond it.
    cable.rated_tensile_strength = ohl_core::units::Force::from_si(26_000.0);
    let e = worked_back(&line, &line.sections[0], &cable, &budget).unwrap_err();
    let text = e.to_string();
    assert!(
        text.contains("section S") && text.contains("cannot be reached below the wire's rated"),
        "{text}"
    );
    cable.rated_tensile_strength = ohl_core::units::Force::from_si(20_000.0);
    let e = worked_back(&line, &line.sections[0], &cable, &budget).unwrap_err();
    assert!(
        e.to_string().contains("is not below the wire's rated"),
        "{e}"
    );
    cable.rated_tensile_strength = ohl_core::units::Force::from_si(140_000.0);
    // No creep case.
    line.creep_stretch.creep_case = None;
    line.set_weather_cases(&[]);
    let e = worked_back(&line, &line.sections[0], &cable, &budget).unwrap_err();
    assert!(e.to_string().contains("Creep-Stretch"), "{e}");
    // Max Sag is no stringing condition; nor is a constant of 0.
    let json = line.to_json();
    assert!(Line::from_json(&json.replace("\"creep\"", "\"max_sag\"")).is_err());
    let bad = json.replace(
        "\"kind\": \"tension\",\n        \"horizontal_tension\": 25000.0",
        "\"kind\": \"catenary_constant\",\n        \"constant\": 0.0",
    );
    assert_ne!(bad, json);
    let e = Line::from_json(&bad).unwrap_err();
    assert!(e.to_string().contains("catenary constant"), "{e}");
}

/// The file: the condition and the catenary constant are written only
/// when used, and only then is the line of version 15; an initial
/// stringing reads and writes as before.
#[test]
fn the_condition_is_written_only_when_used() {
    let (line, _) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15}"#,
        "initial-fe",
    );
    let json = line.to_json();
    assert!(
        !json.contains("condition") && !json.contains("\"version\": 15"),
        "{json}"
    );
    for stringing in [
        r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15, "condition": "creep"}"#,
        r#"{"kind": "sag", "sag": 9.2, "temperature": 15, "condition": "load"}"#,
        r#"{"kind": "catenary_constant", "constant": 1500, "temperature": 15}"#,
        r#"{"kind": "unstressed_lengths", "lengths": [300.5], "temperature": 15,
            "sagged": {"kind": "catenary_constant", "constant": 1500, "temperature": 15, "condition": "creep"}}"#,
    ] {
        let (line, _) = one_span(stringing, "initial-fe");
        let json = line.to_json();
        assert!(json.contains("\"version\": 15"), "{stringing}: {json}");
        let again = Line::from_json(&json).unwrap();
        assert_eq!(again.sections[0].stringing, line.sections[0].stringing);
    }
    let (line, _) = one_span(
        r#"{"kind": "sag", "sag": 9.2, "temperature": 15, "condition": "load"}"#,
        "initial-fe",
    );
    assert_eq!(line.sections[0].stringing.condition(), StrungAt::Load);
    assert_eq!(StrungAt::parse("creep"), Some(StrungAt::Creep));
    assert_eq!(StrungAt::parse("max_sag"), None);
    assert_eq!(StrungAt::Load.as_condition(), Condition::Load);
}
