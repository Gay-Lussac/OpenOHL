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

/// The typed example line, every type given `capacity`.
fn typed(capacity: ols_structures::Capacity) -> (Line, ols_structures::StructureLibrary) {
    let line = Line::load(&ex().join("synthetic-line-typed.ols.json")).unwrap();
    let loaded = ols_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut types = ols_structures::StructureLibrary::default();
    for t in loaded.iter() {
        let mut t = t.clone();
        // The load tree of the type's own attachments.
        let mut c = capacity.clone();
        c.attachments = capacity
            .attachments
            .first()
            .map(|a| {
                t.attachments
                    .iter()
                    .map(|x| ols_structures::AttachmentCapacity {
                        set: x.set,
                        phase: x.phase,
                        ..a.clone()
                    })
                    .collect()
            })
            .unwrap_or_default();
        t.capacity = Some(c);
        types
            .insert(&PathBuf::from(format!("{}.olss", t.id)), t)
            .unwrap();
    }
    let mut line = line
        .resolve(&types, &ols_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    (line, types)
}

fn capacity(text: &str) -> ols_structures::Capacity {
    #[derive(serde::Deserialize)]
    struct Doc {
        capacity: ols_structures::Capacity,
    }
    toml::from_str::<Doc>(text).unwrap().capacity
}

/// Method 1: wind spans are half the adjacent spans, weight spans close to
/// them on this gently sloping line; usage is span over allowable; a line
/// angle beyond the type's fails.
#[test]
fn allowable_spans() {
    let c = capacity(
        "[[capacity.spans]]\nangle = \"0 deg\"\nwind_span = \"400 m\"\nweight_span = \"600 m\"\n\
         [[capacity.spans]]\nangle = \"40 deg\"\nwind_span = \"200 m\"\nweight_span = \"300 m\"\n",
    );
    let (line, types) = typed(c.clone());
    let (_, library, weather, cases) = setup();
    let budget = ols_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    assert_eq!(usage.len(), loads.len());
    let everyday = &usage[0];
    for s in &everyday.structures {
        let check = s.spans.as_ref().expect("checked");
        let allowed = c.spans_at(s.line_angle_deg).unwrap();
        let [lo, hi] = s.weight_span.unwrap();
        assert!(lo <= hi && hi > 0.0, "{}: {lo} {hi}", s.structure);
        let expected = (s.wind_span / allowed.wind_span).max(hi / allowed.weight_span);
        assert!((check.usage - expected).abs() < 1e-12, "{s:?}");
        assert!(s.wind_span > 0.0);
    }
    // T1 ends the main sections: its wind span is half its one span.
    let t1 = everyday
        .structures
        .iter()
        .find(|s| s.structure == "T1")
        .unwrap();
    let a = |id: &str| line.structure(id).unwrap().attachments[0].wire_point;
    let half = 0.5 * (a("T2")[0] - a("T1")[0]).hypot(a("T2")[1] - a("T1")[1]);
    assert!(
        (t1.wind_span - half).abs() < 0.5,
        "{} vs {half}",
        t1.wind_span
    );
    // Only straight structures allowed: those with an angle fail.
    let straight = capacity("[[capacity.spans]]\nwind_span = \"400 m\"\nweight_span = \"600 m\"\n");
    let (line, types) = typed(straight);
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    let mut turning = 0;
    for s in &usage[0].structures {
        let u = s.spans.as_ref().unwrap().usage;
        if s.line_angle_deg > 1e-6 {
            turning += 1;
            assert!(u.is_infinite(), "{s:?}");
        } else {
            assert!(u.is_finite());
        }
    }
    assert!(turning > 0);
}

/// Method 2: the factored loads over the allowable loads times the
/// strength factor; the worst attachment governs; linear interaction adds
/// the ratios.
#[test]
fn allowable_loads() {
    let tree = |extra: &str| {
        capacity(&format!(
            "[[capacity.attachment]]\nset = 1\nphase = 1\nvertical = \"20 kN\"\n\
             transverse = \"20 kN\"\nlongitudinal = \"40 kN\"\nuplift = \"5 kN\"\n{extra}"
        ))
    };
    let (line, types) = typed(tree(""));
    let (_, library, weather, cases) = setup();
    let budget = ols_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    for (r, u) in loads.iter().zip(&usage) {
        let phi = r.case.strength;
        for s in &u.structures {
            // Expected: the largest ratio over this structure's attachments.
            let mut sums: BTreeMap<(u32, u32), [f64; 3]> = BTreeMap::new();
            for l in r.loads.iter().filter(|l| l.structure == s.structure) {
                let e = sums.entry((l.set, l.phase)).or_default();
                for (sum, f) in e.iter_mut().zip(l.factored) {
                    *sum += f;
                }
            }
            let expected = sums
                .values()
                .map(|[t, l, v]| {
                    let vertical = if *v >= 0.0 { v / 20e3 } else { -v / 5e3 };
                    (t.abs() / 20e3).max(l.abs() / 40e3).max(vertical) / phi
                })
                .fold(0.0, f64::max);
            let got = s.loads.as_ref().unwrap().usage;
            assert!(
                (got - expected).abs() < 1e-9,
                "{}: {got} vs {expected}",
                s.structure
            );
        }
    }
    // Linear interaction: never less than each direction on its own.
    let (line, types) = typed(tree("interaction = \"linear\"\n"));
    let linear = capacity::usage(&line, &types, &library, &weather, &loads);
    for (a, b) in usage[0].structures.iter().zip(&linear[0].structures) {
        assert!(b.loads.as_ref().unwrap().usage >= a.loads.as_ref().unwrap().usage - 1e-12);
    }
    // The worst structure first; the CSV has a row per case and structure.
    let g = capacity::governing(&usage);
    assert_eq!(g.len(), usage[0].structures.len());
    for w in g.windows(2) {
        assert!(w[0].1.governing().unwrap().usage >= w[1].1.governing().unwrap().usage);
    }
    let csv = capacity::usage_csv(&usage);
    let rows: usize = usage.iter().map(|c| c.structures.len()).sum();
    assert_eq!(csv.lines().count(), rows + 1);
}
