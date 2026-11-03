// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! One rule for the local conditions (owner 2026-11-03): a check's "Use
//! local conditions" switch and a case's `use_local` mean the same, the
//! structures' `wind_static` taken once. Every line and value here is
//! INVENTED.

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Two level 300 m spans of one wire, T2 a suspension structure with a
/// wind band. INVENTED.
fn line() -> Line {
    let structure = |id: &str, x: f64, suspension: bool| {
        let strings = if suspension {
            format!(
                r#""points": [{{"label": "A1", "position": [{x}, 0, 30]}}],
                   "strings": [{{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                                 "weight": 800.0, "stiffness": 5e7}}],"#
            )
        } else {
            String::new()
        };
        format!(
            r#"{{"id": "{id}", "position": [{x}, 0, 0], "orientation_deg": -90, {strings}
               "wires": [{{"label": "W1", "position": [{x}, 0, 27], "set": 1}}]}}"#
        )
    };
    Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 6, "name": "one wire", "crs": "local",
             "structures": [{}, {}, {}],
             "sections": [{{"id": "S1", "cable": "acsr-drake-26-7", "bundle": 1,
               "points": [{{"structure": "T1", "set": 1}}, {{"structure": "T2", "set": 1}},
                          {{"structure": "T3", "set": 1}}],
               "stringing": {{"kind": "tension", "horizontal_tension": 20000.0,
                              "temperature": 15.0}}}}]}}"#,
        structure("T1", 0.0, false),
        structure("T2", 300.0, true),
        structure("T3", 600.0, false),
    ))
    .unwrap()
}

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n",
    )
    .unwrap()
    .cases
}

/// A structure of wind_static 1.5 gets exactly 1.5 × its body wind with
/// `use_local` ([`with_local`], what a check's switch does), not 2.25 ×
/// — whether the line given is the model's or already localized; the
/// structure application's pressure ([`CaseLoads::structure_wind_at`])
/// is the same.
#[test]
fn wind_static_is_taken_once() {
    let mut line = line();
    line.structures[1].structure_type = Some("band".into());
    line.structures[1].local.wind_static = Some(1.5);
    let t: ohl_structures::StructureType = toml::from_str(
        "id = \"band\"\nname = \"band\"\nkind = \"other\"\n[[wind.band]]\nfrom = \"0 m\"\n\
         to = \"20 m\"\narea = \"10 m²\"\ndrag = 3.0\n",
    )
    .unwrap();
    let mut types = ohl_structures::StructureLibrary::default();
    types
        .insert(&std::path::PathBuf::from("band.ohls"), t)
        .unwrap();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let band = |line: &Line, cases: Vec<LoadCase>| {
        let mut r = structure_loads(line, &lib, &weather, &cases, &budget).unwrap();
        add_structure_wind(&mut r, line, &types, &weather);
        let l = r[0]
            .loads
            .iter()
            .find(|l| l.structure == "T2" && l.point.is_empty())
            .unwrap()
            .clone();
        (l.local[0], r)
    };
    let case = LoadCase::from_weather("wind");
    let (plain, _) = band(&line, with_local(vec![case.clone()], false));
    let (local, r) = band(&line, with_local(vec![case.clone()], true));
    assert!((local / plain - 1.5).abs() < 1e-12, "{local} {plain}");
    // The line already localized (as the switch did before): still 1.5 ×.
    let localized = ohl_line::local::localized(&line, ohl_line::local::Purpose::Strength);
    let (again, _) = band(&localized, with_local(vec![case], true));
    assert!((again / plain - 1.5).abs() < 1e-12, "{again} {plain}");
    let w = &weather[0];
    let p = r[0].structure_wind_at(line.structure("T2")).pressure;
    assert!((p - 1.5 * w.wind_pressure.si()).abs() < 1e-9, "{p}");
    assert_eq!(
        r[0].structure_wind_at(line.structure("T1")).pressure,
        w.wind_pressure.si()
    );
}
