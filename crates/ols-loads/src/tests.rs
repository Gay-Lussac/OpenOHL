// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn setup() -> (Line, ols_cables::Library, Vec<WeatherCase>, LoadCases) {
    let mut line = Line::load(&ex().join("synthetic-line.ols.json")).unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let library = ols_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ols_weather::WeatherCases::load(&ex().join("weather-cases.olswc"))
        .unwrap()
        .cases;
    let cases = LoadCases::load(&ex().join("load-cases.olslc")).unwrap();
    (line, library, weather, cases)
}

#[test]
fn load_case_files_are_checked() {
    let ok = "[[case]]\nname = \"a\"\nweather = \"w\"\n";
    let c = LoadCases::parse(ok).unwrap();
    assert_eq!(c.cases[0].vertical, 1.0);
    assert_eq!(c.cases[0].condition, WireCondition::Initial);
    for bad in [
        format!("{ok}{ok}"),
        "[[case]]\nname = \"a\"\nweather = \"w\"\nvertical = -1\n".into(),
        "[[case]]\nname = \"a\"\nweather = \"w\"\nstrength = 0\n".into(),
        "[[case]]\nname = \"a\"\nweather = \"w\"\nwind = 1\n".into(),
        format!("format = \"other\"\n{ok}"),
    ] {
        assert!(LoadCases::parse(&bad).is_err(), "{bad}");
    }
}

/// The loads hold the wires up (still air: their weight, down), the wind
/// pushes the structures across the line, factors scale their direction.
#[test]
fn structures_carry_the_wires() {
    let (line, library, weather, cases) = setup();
    let budget = ols_wirefe::default_fe_budget();
    let r = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    assert_eq!(r.len(), 3);
    for c in &r {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
        assert!(!c.loads.is_empty());
        // Every point of every section has its support(s).
        let points: std::collections::BTreeSet<(String, u32, u32)> = line
            .sections
            .iter()
            .flat_map(|s| s.points.iter())
            .map(|p| (p.structure.clone(), p.set, p.phase))
            .collect();
        let loaded: std::collections::BTreeSet<(String, u32, u32)> = c
            .loads
            .iter()
            .map(|l| (l.structure.clone(), l.set, l.phase))
            .collect();
        assert_eq!(loaded, points);
    }
    let total = |c: &CaseLoads, k: usize| -> f64 { c.loads.iter().map(|l| l.local[k]).sum() };
    let factored = |c: &CaseLoads, k: usize| -> f64 { c.loads.iter().map(|l| l.factored[k]).sum() };
    let (still, wind, ice) = (&r[0], &r[1], &r[2]);
    // Down, and the same factored (factors 1).
    assert!(total(still, 2) > 0.0);
    assert!((factored(still, 2) - total(still, 2)).abs() < 1e-9);
    // Ice: heavier, and 1.3 × factored.
    assert!(total(ice, 2) > total(still, 2));
    assert!((factored(ice, 2) - 1.3 * total(ice, 2)).abs() < 1e-6 * total(ice, 2));
    // Wind: across the line, factored 1.4.
    let across = |c: &CaseLoads| c.loads.iter().map(|l| l.local[0].abs()).sum::<f64>();
    assert!(across(wind) > 2.0 * across(still) + 100.0);
    for l in &wind.loads {
        assert!((l.factored[0] - 1.4 * l.local[0]).abs() < 1e-9);
    }
    // CSV: a header and a row per load.
    let csv = to_csv(&r);
    assert_eq!(
        csv.lines().count(),
        1 + r.iter().map(|c| c.loads.len()).sum::<usize>()
    );
    assert!(csv.starts_with("case,structure,set,phase"));
}

#[test]
fn an_unknown_weather_case_is_said() {
    let (line, library, weather, mut cases) = setup();
    cases.cases[0].weather = "no such".into();
    let e = structure_loads(
        &line,
        &library,
        &weather,
        &cases.cases,
        &ols_wirefe::default_fe_budget(),
    )
    .unwrap_err();
    assert!(e.contains("no such"), "{e}");
}
