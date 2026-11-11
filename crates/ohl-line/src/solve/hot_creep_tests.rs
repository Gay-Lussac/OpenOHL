// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the creep at elevated temperature on the line (roadmap O22
//! item 6): the mechanism, with INVENTED coefficients — no conductor's
//! law, from no publication.

use super::final_stringing_tests::one_span;
use super::*;
use crate::condition::{Condition, HotStage};
use ohl_cables::hot_creep::HotCreep;
use ohl_core::units::Temperature;

/// INVENTED coefficients, for the tests of the mechanism only: k = 1,
/// a = 1.3, b = 0.16, g = exp(0.02 (T − 20 °C)), from 90 °C.
fn invented_law() -> HotCreep {
    toml::from_str(
        "source = \"INVENTED for the tests — not a conductor's law\"\n\
         k = 1.0\nstress_exponent = 1.3\ntime_exponent = 0.16\n\
         temperature = \"exp\"\ntemperature_coefficient = 0.02\n\
         above = \"90 °C\"\n",
    )
    .unwrap()
}

const STAGES: [HotStage; 2] = [
    HotStage {
        temperature: 100.0,
        hours: 500.0,
    },
    HotStage {
        temperature: 150.0,
        hours: 100.0,
    },
];

/// The level 300 m span of a linear cable (creep as +20 K), strung at
/// 25 kN at 15 °C, solved after creep by `method`.
fn span(method: &str) -> (Line, Cable) {
    one_span(
        r#"{"kind": "tension", "horizontal_tension": 25000, "temperature": 15}"#,
        &format!("creep-{method}"),
    )
}

fn at(line: &Line, cable: &Cable, t: f64) -> SectionShape {
    let case = WeatherCase::still("t", Temperature::from_celsius(t));
    solve_section(line, &line.sections[0], cable, &case, &default_fe_budget()).unwrap()
}

/// The stages add a lengthening of the whole wire on top of the state
/// after creep: the section then hangs as one whose creep (here a
/// temperature shift) is larger by that strain — the change of state with
/// L₀ (1 + ε). By hand each stage's strain lies between the law at the
/// stage's horizontal and largest tensions (its mean tension is between
/// them), held over the stage.
#[test]
fn the_stages_lengthen_the_wire_after_creep() {
    let budget = default_fe_budget();
    for method in ["fe", "rs"] {
        let (mut line, mut cable) = span(method);
        let cold = at(&line, &cable, 40.0);
        let before =
            final_state(&line, &line.sections[0], &cable, Condition::Creep, &budget).unwrap();
        assert_eq!((before.hot_creep, before.notes.len()), (0.0, 0));
        // One stage first, by hand.
        cable.hot_creep = Some(invented_law());
        line.creep_stretch.hot = vec![STAGES[0]];
        let one = final_state(&line, &line.sections[0], &cable, Condition::Creep, &budget).unwrap();
        let law = invented_law();
        let hot = at(
            &{
                let mut l = line.clone();
                l.creep_stretch.hot.clear();
                l
            },
            &cable,
            100.0,
        );
        let stress = |tension: f64| law.stress_of(tension, cable.area().si(), 0.0);
        let hundred = Temperature::from_celsius(100.0);
        let (low, high) = (
            law.strain(stress(hot.spans[0].horizontal_tension), hundred, 500.0),
            law.strain(stress(hot.spans[0].max_tension), hundred, 500.0),
        );
        assert!(
            one.hot_creep > low * (1.0 - 2e-3) && one.hot_creep < high * (1.0 + 2e-3),
            "{method}: {} between {low} and {high}",
            one.hot_creep
        );
        assert_eq!(one.stretch, before.stretch);
        // Both stages: more, and less than the two taken from fresh.
        line.creep_stretch.hot = STAGES.to_vec();
        let both =
            final_state(&line, &line.sections[0], &cable, Condition::Creep, &budget).unwrap();
        assert!(both.hot_creep > one.hot_creep, "{both:?}");
        let fresh = law.strain(stress(hot.spans[0].max_tension), hundred, 500.0)
            + law.strain(
                stress(hot.spans[0].max_tension),
                Temperature::from_celsius(150.0),
                100.0,
            );
        assert!(both.hot_creep < fresh, "{} < {fresh}", both.hot_creep);
        // The order typed does not matter.
        let mut reversed = line.clone();
        reversed.creep_stretch.hot.reverse();
        let r = final_state(
            &reversed,
            &reversed.sections[0],
            &cable,
            Condition::Creep,
            &budget,
        )
        .unwrap();
        assert_eq!(r.hot_creep, both.hot_creep);
        // The wire hangs as one whose creep is a larger temperature shift:
        // the same lengthening, the same change of state.
        let sagged = at(&line, &cable, 40.0);
        assert!(sagged.spans[0].sag > cold.spans[0].sag + 0.05);
        assert!(sagged.spans[0].horizontal_tension < cold.spans[0].horizontal_tension);
        let (mut plain, mut shifted) = span(method);
        shifted.creep_temperature_shift = Some(20.0 + both.hot_creep / shifted.thermal_expansion());
        plain.creep_stretch.hot.clear();
        let same = at(&plain, &shifted, 40.0);
        assert!(
            (same.spans[0].horizontal_tension - sagged.spans[0].horizontal_tension).abs() < 1e-5,
            "{method}: {} against {}",
            sagged.spans[0].horizontal_tension,
            same.spans[0].horizontal_tension
        );
        assert!((same.spans[0].sag - sagged.spans[0].sag).abs() < 1e-8);
        // Every result in that state says whose coefficients they are.
        assert_eq!(sagged.flags.len(), 1, "{:?}", sagged.flags);
        let note = &sagged.flags[0];
        assert!(
            note.contains("the user's coefficients, source: INVENTED for the tests")
                && note.contains("500 h at 100 °C, 100 h at 150 °C")
                && note.contains("wire type hand"),
            "{note}"
        );
        // Initially and after load nothing changes: the history is the
        // state after creep's.
        for condition in [Condition::Initial, Condition::Load] {
            let f = final_state(&line, &line.sections[0], &cable, condition, &budget).unwrap();
            assert_eq!((f.hot_creep, f.notes.len()), (0.0, 0));
        }
    }
}

/// A wire type without a law: its hot creep is not counted, the results
/// are those without the history — and say "NOT ON THE SAFE SIDE". A
/// stage below the law's range is left out and said.
#[test]
fn what_is_not_counted_is_said() {
    let (mut line, mut cable) = span("fe");
    let plain = at(&line, &cable, 40.0);
    line.creep_stretch.hot = STAGES.to_vec();
    let no_law = at(&line, &cable, 40.0);
    assert_eq!(no_law.spans[0].sag, plain.spans[0].sag);
    assert_eq!(
        no_law.flags,
        [
            "NOT ON THE SAFE SIDE: wire type hand gives no law of creep at elevated temperature — \
          its creep during 500 h at 100 °C, 100 h at 150 °C is not counted"
        ]
    );
    // Without a history: nothing said, the law or not.
    assert!(plain.flags.is_empty());
    // A stage below the law's range.
    cable.hot_creep = Some(invented_law());
    line.creep_stretch.hot = vec![
        HotStage {
            temperature: 80.0,
            hours: 2000.0,
        },
        STAGES[0],
    ];
    let partly = at(&line, &cable, 40.0);
    assert_eq!(partly.flags.len(), 2, "{:?}", partly.flags);
    assert!(
        partly.flags[1].contains("2000 h at 80 °C not counted")
            && partly.flags[1].contains("applies from 90 °C"),
        "{:?}",
        partly.flags
    );
    line.creep_stretch.hot = vec![STAGES[0]];
    let only = at(&line, &cable, 40.0);
    assert_eq!(only.spans[0].sag, partly.spans[0].sag);
}

/// A cable with stress-strain curves: the stages lengthen it on top of
/// its creep curve's set, by finite elements and by the ruling span
/// alike (INVENTED law on the synthetic test cable).
#[test]
fn curves_take_the_lengthening_on_top_of_their_set() {
    use crate::condition::Calculation;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let text =
        std::fs::read_to_string(dir.join("../ohl-sagtension/tests/data/synthetic-cable.ohlw"))
            .unwrap();
    let mut lib = ohl_cables::Library::default();
    lib.add_toml(std::path::Path::new("synthetic"), &text)
        .unwrap();
    let mut cable = lib.get("synthetic").unwrap().clone();
    let (mut line, _) = span("fe");
    let budget = default_fe_budget();
    let hot = Temperature::from_celsius(60.0);
    let case = WeatherCase::still("60", hot);
    let mut sags = Vec::new();
    for with_law in [false, true] {
        if with_law {
            cable.hot_creep = Some(invented_law());
            line.creep_stretch.hot = STAGES.to_vec();
        }
        let mut by_method = Vec::new();
        for calc in [Calculation::CREEP_FE, Calculation::CREEP_RS] {
            line.sections[0].calculation = calc;
            let shape = solve_section(&line, &line.sections[0], &cable, &case, &budget).unwrap();
            by_method.push((shape.spans[0].horizontal_tension, shape.spans[0].sag));
        }
        // FE and RS agree as without the stages (tension within 0.1 %).
        let (fe, rs) = (by_method[0], by_method[1]);
        assert!((fe.0 - rs.0).abs() < 1e-3 * rs.0, "FE {fe:?} RS {rs:?}");
        sags.push(by_method);
    }
    let f = final_state(&line, &line.sections[0], &cable, Condition::Creep, &budget).unwrap();
    assert!(f.hot_creep > 1e-4 && f.stretch > 0.0, "{f:?}");
    assert_eq!(f.lengthening(), f.hot_creep);
    for (with, without) in sags[1].iter().zip(&sags[0]) {
        assert!(
            with.1 > without.1 + 0.05,
            "{} against {}",
            with.1,
            without.1
        );
    }
}

/// The file: the time at high temperature is written only when given,
/// and only then is the line of version 15; what cannot be a stage is
/// refused.
#[test]
fn the_history_is_written_only_when_given() {
    let (mut line, _) = span("fe");
    assert!(!line.to_json().contains("\"hot\""));
    assert!(!line.to_json().contains("\"version\": 15"));
    line.creep_stretch.hot = STAGES.to_vec();
    let json = line.to_json();
    assert!(
        json.contains("\"version\": 15") && json.contains("\"hot\""),
        "{json}"
    );
    let back = Line::from_json(&json).unwrap();
    assert_eq!(back.creep_stretch, line.creep_stretch);
    assert_eq!(
        back.creep_stretch.hot_text(),
        "500 h at 100 °C, 100 h at 150 °C"
    );
    for (bad, says) in [
        (
            HotStage {
                temperature: 100.0,
                hours: 0.0,
            },
            "hours above 0",
        ),
        (
            HotStage {
                temperature: 120.0,
                hours: -5.0,
            },
            "hours above 0",
        ),
    ] {
        let mut l = line.clone();
        l.creep_stretch.hot.push(bad);
        let e = l.validate().unwrap_err().to_string();
        assert!(e.contains(says), "{e}");
    }
    let mut l = line.clone();
    l.creep_stretch.hot = vec![STAGES[0]; crate::condition::HOT_STAGES + 1];
    assert!(
        l.validate()
            .unwrap_err()
            .to_string()
            .contains("at most 100")
    );
}
