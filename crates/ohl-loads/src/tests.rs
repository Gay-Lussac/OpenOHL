// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn setup() -> (Line, ohl_cables::Library, Vec<WeatherCase>, LoadCases) {
    let mut line = Line::load(&ex().join("synthetic-line.ohl.json")).unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    let cases = LoadCases::load(&ex().join("load-cases.ohllc")).unwrap();
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
    let budget = ohl_wirefe::default_fe_budget();
    let r = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    assert_eq!(r.len(), 3);
    for c in &r {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
        assert!(!c.loads.is_empty());
        // Every point of every section is held at some point of its
        // structure.
        let points: std::collections::BTreeSet<(String, u32)> = line
            .sections
            .iter()
            .flat_map(|s| s.points.iter())
            .map(|p| (p.structure.clone(), p.set))
            .collect();
        let loaded: std::collections::BTreeSet<(String, u32)> = c
            .loads
            .iter()
            .flat_map(|l| l.sets.iter().map(|set| (l.structure.clone(), *set)))
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
    assert!(csv.starts_with("case,structure,point,label,sets"));
}

/// A weather case on its own as a load case: named after it, as strung,
/// every factor 1; its loads those of the example's load case of the same
/// weather case with factors 1 ("everyday"), factored = unfactored.
#[test]
fn a_weather_case_on_its_own_is_a_load_case_of_factors_one() {
    let (line, library, weather, cases) = setup();
    let c = LoadCase::from_weather("everyday");
    assert_eq!(c.name, "weather: everyday");
    assert_eq!(c.weather, "everyday");
    assert_eq!(c.condition, WireCondition::Initial);
    assert_eq!(
        [c.vertical, c.transverse, c.longitudinal, c.strength],
        [1.0; 4]
    );
    assert_eq!(c.tags, [WEATHER_CASE_TAG]);
    c.validate().unwrap();
    let budget = ohl_wirefe::default_fe_budget();
    let own = structure_loads(&line, &library, &weather, &[c], &budget).unwrap();
    let file = structure_loads(&line, &library, &weather, &cases.cases[..1], &budget).unwrap();
    assert_eq!(own[0].case.name, "weather: everyday");
    assert_eq!(own[0].loads.len(), file[0].loads.len());
    for (a, b) in own[0].loads.iter().zip(&file[0].loads) {
        assert_eq!((&a.structure, &a.point), (&b.structure, &b.point));
        assert_eq!(a.local, b.local);
        assert_eq!(a.factored, a.local);
    }
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
        &ohl_wirefe::default_fe_budget(),
    )
    .unwrap_err();
    assert!(e.contains("no such"), "{e}");
}

/// The typed example line, every type given `capacity`.
fn typed(capacity: ohl_structures::Capacity) -> (Line, ohl_structures::StructureLibrary) {
    let line = Line::load(&ex().join("synthetic-line-typed.ohl.json")).unwrap();
    let loaded = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut types = ohl_structures::StructureLibrary::default();
    for t in loaded.iter() {
        let mut t = t.clone();
        // The load tree of the type's own points.
        let mut c = capacity.clone();
        c.attachments = capacity
            .attachments
            .first()
            .map(|a| {
                t.points
                    .iter()
                    .map(|x| ohl_structures::AttachmentCapacity {
                        point: Some(x.label.clone()),
                        set: None,
                        phase: None,
                        ..a.clone()
                    })
                    .collect()
            })
            .unwrap_or_default();
        t.capacity = Some(c);
        types
            .insert(&PathBuf::from(format!("{}.ohls", t.id)), t)
            .unwrap();
    }
    let mut line = line
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    (line, types)
}

fn capacity(text: &str) -> ohl_structures::Capacity {
    #[derive(serde::Deserialize)]
    struct Doc {
        capacity: ohl_structures::Capacity,
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
    let budget = ohl_wirefe::default_fe_budget();
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
    let a = |id: &str| line.structure(id).unwrap().wires[0].position;
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
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    for (r, u) in loads.iter().zip(&usage) {
        let phi = r.case.strength;
        for s in &u.structures {
            // Expected: the largest ratio over this structure's attachments.
            let mut sums: BTreeMap<&str, [f64; 3]> = BTreeMap::new();
            for l in r
                .loads
                .iter()
                .filter(|l| l.structure == s.structure && !l.point.is_empty())
            {
                let e = sums.entry(l.point.as_str()).or_default();
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

/// Wind on the structures: one load per band of the type, across the
/// line, pressure × drag × area, factored by the transverse factor; none
/// in still air; not an attachment for the capacity checks.
#[test]
fn wind_on_the_structures() {
    let (line, mut types) = typed(ohl_structures::Capacity::default());
    let mut t2 = types.get("type-02").unwrap().clone();
    t2.wind = Some(
        toml::from_str::<ohl_structures::StructureType>(
            "id = \"x\"\nname = \"x\"\nkind = \"other\"\n[[wind.band]]\nfrom = \"0 m\"\nto = \"20 m\"\narea = \"10 m²\"\ndrag = 3.0\n",
        )
        .unwrap()
        .wind
        .unwrap(),
    );
    let mut with_wind = ohl_structures::StructureLibrary::default();
    for t in types.iter() {
        let t = if t.id == "type-02" {
            t2.clone()
        } else {
            t.clone()
        };
        with_wind
            .insert(&PathBuf::from(format!("{}.ohls", t.id)), t)
            .unwrap();
    }
    types = with_wind;
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let mut loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let before: Vec<usize> = loads.iter().map(|r| r.loads.len()).collect();
    add_structure_wind(&mut loads, &line, &types, &weather);
    let typed2: Vec<&str> = line
        .structures
        .iter()
        .filter(|s| s.structure_type.as_deref() == Some("type-02"))
        .map(|s| s.id.as_str())
        .collect();
    for (r, n) in loads.iter().zip(before) {
        let w = weather.iter().find(|w| w.name == r.case.weather).unwrap();
        assert!(r.loads[n..].iter().all(|l| l.point.is_empty()));
        // Those of type-02 (one band here; the example's other types may
        // have their own).
        let added: Vec<&AttachmentLoad> = r.loads[n..]
            .iter()
            .filter(|l| typed2.contains(&l.structure.as_str()))
            .collect();
        if w.wind_pressure.si() == 0.0 {
            assert_eq!(r.loads.len(), n);
            continue;
        }
        assert_eq!(added.len(), typed2.len());
        for l in added {
            let f = w.wind_pressure.si() * 3.0 * 10.0;
            assert!((l.local[0].abs() - f).abs() < 1e-9 && l.local[1] == 0.0 && l.local[2] == 0.0);
            assert!((l.factored[0] - l.local[0] * r.case.transverse).abs() < 1e-9);
            let s = line.structure(&l.structure).unwrap();
            assert!((l.at[2] - (s.position[2] + 10.0)).abs() < 1e-9);
        }
    }
    // The capacity checks see no attachment 0:0.
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    assert!(
        usage
            .iter()
            .flat_map(|c| &c.structures)
            .all(|s| !s.notes.iter().any(|n| n.contains("0:0")))
    );
}

/// A structure standing on a height extension: its wind bands rise by
/// the extension's `dz` with it (one band 0–20 m on a +3 m extension acts
/// at 3 + 10 = 13 m above the ground), the force unchanged (pressure ×
/// drag × area, the same at every height); the other places of the type,
/// as written, keep the band's mid-height (10 m).
#[test]
fn wind_bands_rise_with_the_extension() {
    let (mut line, _) = typed(ohl_structures::Capacity::default());
    let loaded = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut types = ohl_structures::StructureLibrary::default();
    for t in loaded.iter() {
        let mut t = t.clone();
        if t.id == "type-02" {
            t.wind = Some(
                toml::from_str::<ohl_structures::StructureType>(
                    "id = \"x\"\nname = \"x\"\nkind = \"other\"\n[[wind.band]]\nfrom = \"0 m\"\nto = \"20 m\"\narea = \"10 m²\"\ndrag = 3.0\n",
                )
                .unwrap()
                .wind
                .unwrap(),
            );
            t.extensions = vec![ohl_structures::Extension::plain("+3", 3.0)];
        }
        types
            .insert(&PathBuf::from(format!("{}.ohls", t.id)), t)
            .unwrap();
    }
    let k = line
        .structures
        .iter()
        .position(|s| s.structure_type.as_deref() == Some("type-02"))
        .unwrap();
    line.structures[k].extension = Some("+3".into());
    let raised = line.structures[k].id.clone();
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let mut loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    add_structure_wind(&mut loads, &line, &types, &weather);
    let mut seen = 0;
    for r in &loads {
        let w = weather.iter().find(|w| w.name == r.case.weather).unwrap();
        for l in r.loads.iter().filter(|l| l.point.is_empty()) {
            let s = line.structure(&l.structure).unwrap();
            if s.structure_type.as_deref() != Some("type-02") {
                continue;
            }
            let f = w.wind_pressure.si() * 3.0 * 10.0;
            assert!((l.local[0].abs() - f).abs() < 1e-9);
            let mid = if l.structure == raised { 13.0 } else { 10.0 };
            assert!((l.at[2] - (s.position[2] + mid)).abs() < 1e-9, "{l:?}");
            if l.structure == raised {
                assert_eq!(l.label, "wind on the structure (3–23 m)");
                seen += 1;
            }
        }
    }
    assert!(seen > 0);
}

/// The example line with its own structure folder: type-02 (T2, T4) has
/// an invented lattice model, type-05 (T6) an invented steel pole; both
/// are checked in every load case and the CSV carries their usage.
#[test]
fn structure_analysis_on_the_example_line() {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    for c in &usage {
        for s in &c.structures {
            let typed_as = s.structure_type.as_deref();
            if typed_as == Some("type-02") {
                let fe = s.fe.as_ref().unwrap_or_else(|| panic!("{s:?}"));
                assert_eq!(fe.method, capacity::Method::Fe);
                assert!(fe.usage > 0.0 && fe.usage < 1.0, "{}: {fe:?}", c.case);
                assert!(fe.governs.starts_with("member "), "{fe:?}");
            } else {
                assert!(s.fe.is_none());
            }
            if typed_as == Some("type-05") {
                let pole = s.pole.as_ref().unwrap_or_else(|| panic!("{s:?}"));
                assert!(
                    pole.usage >= 0.0 && pole.usage < 1.0,
                    "{}: {pole:?}",
                    c.case
                );
                // The wind on the wires and on the pole bends it.
                if c.case == "wind" {
                    assert!(pole.usage > 0.05, "{pole:?}");
                    assert!(pole.governs.contains("P-Delta"), "{pole:?}");
                }
            } else {
                assert!(s.pole.is_none());
            }
            assert!(s.notes.is_empty(), "{s:?}");
        }
    }
    // The wind case loads the tower more than still air.
    let fe = |case: usize| {
        usage[case]
            .structures
            .iter()
            .find(|s| s.structure == "T2")
            .and_then(|s| s.fe.as_ref())
            .unwrap()
            .usage
    };
    assert!(fe(1) > fe(0), "{} {}", fe(1), fe(0));
    let csv = capacity::usage_csv(&usage);
    assert!(csv.lines().next().unwrap().contains("fe_usage,pole_usage"));
}

/// With a section library holding the example tower's section ids (as
/// invented angles), its members take them and the wind of the load case
/// on their profiles: the FE usage is that of the model solved by hand
/// with the attachment loads and the member wind, and the type's wind
/// bands (in the loads as the body wind) change nothing — not counted
/// twice. The note says which wind was used.
#[test]
fn member_wind_replaces_the_bands_in_the_fe_check() {
    let sections = ohl_sections::SectionLibrary::parse(
        "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
         yield = \"355 MPa\"\n\
         [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"110 mm\"\nt = \"12 mm\"\n\
         [[section]]\nid = \"brace\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"70 mm\"\nt = \"7 mm\"\n\
         [[section]]\nid = \"arm\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"90 mm\"\nt = \"9 mm\"\n",
    )
    .unwrap();
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let mut with_bands = loads.clone();
    add_structure_wind(&mut with_bands, &line, &types, &weather);
    assert!(with_bands[1].loads.len() > loads[1].loads.len());
    let run = |loads: &[CaseLoads]| {
        capacity::usage_with_sections(&line, &types, &sections, &library, &weather, loads)
    };
    let (plain, banded) = (run(&loads), run(&with_bands));
    let t2 = |u: &[capacity::CaseUsage], case: usize| {
        u[case]
            .structures
            .iter()
            .find(|s| s.structure == "T2")
            .unwrap()
            .clone()
    };
    let (wind, banded_wind) = (t2(&plain, 1), t2(&banded, 1));
    assert_eq!(wind.fe, banded_wind.fe, "the bands are not in the model");
    assert!(
        wind.notes
            .iter()
            .any(|n| n.contains("member wind") && n.contains("wind bands not in the model")),
        "{:?}",
        wind.notes
    );
    // Still air: no member wind, no note.
    assert!(t2(&plain, 0).notes.is_empty(), "{:?}", t2(&plain, 0).notes);
    // By hand: the model with the library, the factored loads at its
    // points and the member wind (high wind × 1.4 transverse, to −x: the
    // wind comes from the right).
    let t = types.get("type-02").unwrap();
    let model =
        ohl_structures_fe::Model::load_with(types.path("type-02").unwrap(), &sections).unwrap();
    assert!(model.has_profiles());
    let case = &cases.cases[1];
    let w = weather.iter().find(|w| w.name == case.weather).unwrap();
    let mut at: std::collections::BTreeMap<&str, [f64; 3]> = Default::default();
    for l in loads[1]
        .loads
        .iter()
        .filter(|l| l.structure == "T2" && !l.point.is_empty())
    {
        let e = at.entry(l.point.as_str()).or_default();
        for (sum, f) in e.iter_mut().zip(l.factored) {
            *sum += f;
        }
    }
    let mut nodal = Vec::new();
    for (label, [tr, lo, ve]) in at {
        nodal.extend(model.point_loads(label, [tr, lo, -ve]).unwrap());
    }
    let q = w.wind_pressure.si() * case.transverse;
    let member_wind = model.member_wind(q, [-w.wind_from.to_left(), 0.0, 0.0]);
    let total: f64 = member_wind.iter().map(|l| l.force[0]).sum();
    assert!(total.abs() > 1000.0, "{total}");
    nodal.extend(member_wind);
    let fe_loads = ohl_structures_fe::Loads {
        nodal,
        self_weight: case.vertical,
    };
    let (_, usage) = ohl_structures_fe::analyse(
        &model,
        &fe_loads,
        case.strength,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let worst = ohl_structures_fe::check::worst(&usage).unwrap();
    let fe = wind.fe.clone().unwrap();
    assert!(
        (fe.usage - worst.usage.unwrap()).abs() < 1e-12,
        "{fe:?} {worst:?}"
    );
    assert!(t.wind.is_some());
    // Without the library: the type's own tables, no member wind.
    let own = capacity::usage(&line, &types, &library, &weather, &loads);
    assert!(t2(&own, 1).notes.is_empty());
    assert_ne!(t2(&own, 1).fe, wind.fe);
}

/// A level synthetic line T1–T4, 300 m apart: T1 and T4 end the wire, T2
/// is a suspension structure (one attachment carries both spans), T3 a
/// dead-end whose back and ahead strain strings are separate attachments
/// (1:1 and 2:1, clamps 0.5 m either side) joined by a jumper.
fn dead_end_line() -> Line {
    let structure = |id: &str, x: f64, points: &[(u32, f64)]| {
        let attachments: Vec<String> = points
            .iter()
            .map(|(set, dx)| {
                format!(
                    r#"{{"set": {set}, "phase": 1, "insulator_point": [{x}, 0, 30],
                        "wire_point": [{}, 0, 30]}}"#,
                    x + dx
                )
            })
            .collect();
        format!(
            r#"{{"id": "{id}", "position": [{x}, 0, 0], "orientation_deg": -90,
                "attachments": [{}]}}"#,
            attachments.join(",")
        )
    };
    let point = |s: &str, set: u32| format!(r#"{{"structure": "{s}", "set": {set}, "phase": 1}}"#);
    let section = |id: &str, points: &[String]| {
        format!(
            r#"{{"id": "{id}", "cable": "acsr-drake-26-7", "points": [{}],
                "stringing": {{"kind": "tension", "horizontal_tension": 25000.0,
                "temperature": 15.0}}}}"#,
            points.join(",")
        )
    };
    let json = format!(
        r#"{{"format": "ohl-line", "version": 1, "name": "dead-end test", "crs": "local",
            "structures": [{}, {}, {}, {}],
            "sections": [{}, {}],
            "jumpers": [{{"id": "J3", "cable": "acsr-drake-26-7", "from": {}, "to": {},
                "sag": 1.0, "temperature": 15.0}}]}}"#,
        structure("T1", 0.0, &[(1, 0.0)]),
        structure("T2", 300.0, &[(1, 0.0)]),
        structure("T3", 600.0, &[(1, -0.5), (2, 0.5)]),
        structure("T4", 900.0, &[(1, 0.0)]),
        section("back", &[point("T1", 1), point("T2", 1), point("T3", 1)]),
        section("ahead", &[point("T3", 2), point("T4", 1)]),
        point("T3", 1),
        point("T3", 2),
    );
    let mut line = Line::from_json(&json).unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    line
}

/// Method 1 per conductor position: a dead-end's back and ahead strings
/// (separate attachments joined by a jumper) add up to the structure's
/// wind and weight spans, as a suspension structure's one attachment
/// does; a line end keeps half its one span.
#[test]
fn dead_end_spans_take_both_sides() {
    let line = dead_end_line();
    // The dead-end's two strings are one conductor position.
    let p = capacity::positions(&line);
    let key = |s: &str, set: u32| (s.to_string(), set);
    assert_eq!(p.get(&key("T3", 2)), Some(&key("T3", 1)));
    assert_eq!(p.get(&key("T3", 1)), Some(&key("T3", 1)));
    assert!(!p.contains_key(&key("T2", 1)));
    // The example's tee-off jumper (a branch's end to a suspension point)
    // joins nothing: the branch is another position.
    let (example, library, weather, cases) = setup();
    assert!(example.jumpers.len() == 1 && capacity::positions(&example).is_empty());
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    assert!(loads[0].failed.is_empty(), "{:?}", loads[0].failed);
    let types = ohl_structures::StructureLibrary::default();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    let everyday = &usage[0];
    // Spans: T1–T2 300 m, T2–T3 299.5 m, T3–T4 299.5 m (to the clamps).
    for (id, span) in [("T1", 150.0), ("T2", 299.75), ("T3", 299.5), ("T4", 149.75)] {
        let s = everyday
            .structures
            .iter()
            .find(|s| s.structure == id)
            .unwrap();
        let [lo, hi] = s.weight_span.unwrap();
        eprintln!(
            "{id}: wind span {:.2} m, weight span {lo:.2}–{hi:.2} m",
            s.wind_span
        );
        assert!((s.wind_span - span).abs() < 0.01, "{id}: {}", s.wind_span);
        // Level: the low points mid-span (the weight span within a metre;
        // at T3 the jumper's weight on the dead-end clamps lowers them, the
        // low points come a little towards it: within 2.5 m).
        let within = if id == "T3" { 2.5 } else { 1.0 };
        assert!(
            (lo - span).abs() < within && (hi - span).abs() < within,
            "{id}: {lo} {hi}"
        );
        // One conductor position: one weight span.
        assert!((hi - lo).abs() < 1e-9, "{id}: {lo} {hi}");
    }
}

/// A jumper's support clamp loads the top of its string on the structure:
/// at least half the string's weight down, and the jumper's weight it
/// carries; the load is the jumper's.
#[test]
fn jumper_support_strings_load_their_tops() {
    let (mut line, library, weather, cases) = setup();
    let still = WeatherCase::still("still", ohl_core::units::Temperature::from_celsius(15.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &still)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let free = ohl_line::jumper::solve_jumpers(&line, &library, &still, &shapes)
        .remove(0)
        .unwrap();
    let middle = free.points[free.points.len() / 2];
    let j = &mut line.jumpers[0];
    j.sag = None;
    j.length = Some(free.unstressed_length);
    j.set_supports(vec![ohl_line::JumperClamp::new("JW", 0.5)]);
    let st = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    st.points.push(ohl_line::StructurePoint {
        description: Default::default(),
        label: "JS".into(),
        position: [middle[0], middle[1], middle[2] + 2.0],
    });
    st.wires.push(ohl_line::WirePoint {
        description: Default::default(),
        label: "JW".into(),
        position: [middle[0], middle[1], middle[2] + 1.0],
        set: None,
        weight: 0.0,
    });
    st.strings.push(ohl_line::InsulatorString {
        label: "JI".into(),
        top: "JS".into(),
        wire: "JW".into(),
        weight: 100.0,
        ..Default::default()
    });
    line.validate().unwrap();
    let budget = ohl_wirefe::default_fe_budget();
    let r = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    for c in &r {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
        let l = c
            .loads
            .iter()
            .find(|l| l.structure == "T3" && l.point == "JS")
            .expect("a load at the string's top");
        assert_eq!(l.sections, vec![line.jumpers[0].id.clone()]);
        assert!(l.sets.is_empty());
        // Down: half the string, and part of the jumper (~16 N/m on a
        // ~10.8 m jumper).
        assert!(l.local[2] > 50.0 + 20.0, "{}: {:?}", c.case.name, l.local);
        assert!(l.local[2] < 50.0 + 400.0, "{}: {:?}", c.case.name, l.local);
    }
}

/// A placed structure standing on an extension with a body is checked on
/// the model with that body: type-02 gets an extension `+2` (a piece of
/// four pinned joints 2 m under its feet, X-braced), T2 stands on it; its
/// finite-element check runs on the model with the piece (four joints and
/// twelve members more), T4 (no extension) on the model as written; a
/// plain extension (`+1`) has no model of its own.
#[test]
fn structure_analysis_with_an_extension_body() {
    let dir = std::env::temp_dir().join(format!("ohl-loads-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(ex().join("Structures")).unwrap() {
        let f = f.unwrap().path();
        std::fs::copy(&f, dir.join(f.file_name().unwrap())).unwrap();
    }
    let feet = [
        ("L0A", -2.5, -2.5),
        ("L0B", 2.5, -2.5),
        ("L0C", 2.5, 2.5),
        ("L0D", -2.5, 2.5),
    ];
    let mut piece = String::from(
        "\n[[extension]]\nname = \"+1\"\ndz = \"1 m\"\n\n[[extension]]\nname = \"+2\"\ndz = \"2 m\"\n",
    );
    for (k, (_, x, y)) in feet.iter().enumerate() {
        piece.push_str(&format!(
            "[[extension.node]]\nid = \"P{k}\"\npoint = [\"{x} m\", \"{y} m\", \"-2 m\"]\nsupport = \"pinned\"\n"
        ));
    }
    let mut n = 0;
    let mut member = |a: String, b: &str, s: &str| {
        n += 1;
        format!(
            "[[extension.member]]\nid = \"PM{n}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"{s}\"\n"
        )
    };
    for k in 0..4 {
        let j = (k + 1) % 4;
        piece.push_str(&member(format!("P{k}"), feet[k].0, "leg"));
        piece.push_str(&member(format!("P{k}"), feet[j].0, "brace"));
        piece.push_str(&member(format!("P{j}"), feet[k].0, "brace"));
    }
    let path = dir.join("type-02.ohls");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}{piece}")).unwrap();
    let types = ohl_structures::StructureLibrary::load_dir(&dir).unwrap();
    let analyses =
        crate::analysis::Analyses::load(&types, &ohl_sections::SectionLibrary::default());
    let size = |ext: Option<&str>| {
        analyses
            .model("type-02", ext)
            .map(|m| (m.nodes.len(), m.members.len()))
    };
    let (n0, m0) = size(None).unwrap();
    assert_eq!(size(Some("+2")), Some((n0 + 4, m0 + 12)));
    assert_eq!(size(Some("+1")), None);
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json")).unwrap();
    for s in line.structures.iter_mut().filter(|s| s.id == "T2") {
        s.extension = Some("+2".into());
    }
    let mut line = line
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    let mut seen = 0;
    for c in &usage {
        for s in c
            .structures
            .iter()
            .filter(|s| s.structure == "T2" || s.structure == "T4")
        {
            let fe = s.fe.as_ref().unwrap_or_else(|| panic!("{s:?}"));
            assert!(fe.usage > 0.0 && fe.usage < 1.0, "{}: {fe:?}", c.case);
            assert!(s.notes.is_empty(), "{s:?}");
            seen += 1;
        }
    }
    assert_eq!(seen, 2 * usage.len());
    std::fs::remove_dir_all(&dir).ok();
}

/// A section that does not solve (here: a cable not in the library) is
/// listed at every structure it ends at or hangs on: their loads and
/// usage in that case are incomplete — the checks keep the numbers of
/// the loads there are, marked, never a plain pass — the others' not;
/// the usage CSV says why in its `incomplete` column.
#[test]
fn a_section_not_solved_makes_its_structures_incomplete() {
    let c = capacity(
        "[[capacity.spans]]\nangle = \"0 deg\"\nwind_span = \"4000 m\"\nweight_span = \"6000 m\"\n\
         [[capacity.spans]]\nangle = \"90 deg\"\nwind_span = \"4000 m\"\nweight_span = \"6000 m\"\n",
    );
    let (mut line, types) = typed(c);
    for s in line.sections.iter_mut().filter(|s| s.id == "S3") {
        s.cable = "no-such-cable".into();
    }
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let ends = section_structures(&line, "S3");
    assert_eq!(ends, ["T3", "T6", "T7"]);
    for c in &loads {
        assert!(
            c.failed.iter().any(|f| f.starts_with("S3:")),
            "{:?}",
            c.failed
        );
        for id in &ends {
            let m: Vec<&MissingWire> = c.missing_at(id).collect();
            assert!(
                m.iter()
                    .any(|m| m.kind == WireKind::Section && m.id == "S3"),
                "{id}: {m:?}"
            );
            assert!(m[0].describe().starts_with("section S3 not solved ("));
        }
        for id in ["T1", "T2", "T4", "T5"] {
            assert!(!c.is_incomplete(id), "{id} in {}", c.case.name);
        }
    }
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    for c in &usage {
        for s in &c.structures {
            let incomplete = ends.contains(&s.structure);
            assert_eq!(s.is_incomplete(), incomplete, "{s:?}");
            if incomplete {
                assert_eq!(s.status(), capacity::UsageStatus::Incomplete);
                // The numbers as computed, marked.
                let g = s.spans.as_ref().expect("checked");
                assert!(g.incomplete && g.usage.is_finite());
                assert!(s.not_checked_text().contains("S3 not solved"));
            } else {
                assert_eq!(s.status(), capacity::UsageStatus::Pass, "{s:?}");
                assert!(!s.spans.as_ref().unwrap().incomplete);
            }
        }
        // T7 only carries S3: listed though it has no load left.
        assert!(c.structures.iter().any(|s| s.structure == "T7"));
    }
    assert_eq!(
        capacity::incomplete_cases(&usage, "T6").len(),
        usage.len(),
        "every case"
    );
    assert!(capacity::incomplete_cases(&usage, "T1").is_empty());
    let csv = capacity::usage_csv(&usage);
    let header = csv.lines().next().unwrap();
    assert!(header.ends_with(",notes,incomplete"), "{header}");
    let t6 = csv.lines().find(|l| l.starts_with("everyday,T6,")).unwrap();
    assert!(t6.contains("incomplete: section S3 not solved ("), "{t6}");
    let t1 = csv.lines().find(|l| l.starts_with("everyday,T1,")).unwrap();
    assert!(t1.ends_with(','), "{t1}");
}

/// A load case in which every section fails (after creep with no creep
/// case) makes every structure incomplete in that case only.
#[test]
fn a_case_with_sections_not_solved_is_incomplete_only_there() {
    let (mut line, library, weather, cases) = setup();
    line.creep_stretch.creep_case = None;
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_loads(&line, &library, &weather, &cases.cases, &budget).unwrap();
    let (everyday, ice) = (&loads[0], &loads[2]);
    assert_eq!(ice.case.name, "ice");
    assert!(everyday.missing.is_empty(), "{:?}", everyday.missing);
    for s in &line.structures {
        assert!(ice.is_incomplete(&s.id), "{}", s.id);
    }
}

/// A finite-element check whose tension-only members did not settle (kept
/// active, [`ohl_structures_fe::Solution::kept_active`]) is approximate:
/// the structure's status says so whatever its usage (incomplete loads
/// still come first), and the CSV's `incomplete` column names it.
#[test]
fn an_approximate_fe_check_is_never_a_plain_pass() {
    let check = |method, approximate| capacity::Check {
        method,
        usage: 0.4,
        governs: "member AM tension 1.0 kN of 10.0 kN — kept active (did not settle)".into(),
        incomplete: false,
        approximate,
    };
    let mut s = capacity::StructureUsage {
        structure: "T1".into(),
        structure_type: Some("t".into()),
        line_angle_deg: 0.0,
        wind_span: 300.0,
        weight_span: None,
        spans: Some(check(capacity::Method::Spans, false)),
        loads: None,
        fe: Some(check(capacity::Method::Fe, false)),
        pole: None,
        notes: Vec::new(),
        missing: Vec::new(),
    };
    assert_eq!(s.status(), capacity::UsageStatus::Pass);
    assert!(!s.is_approximate() && s.approximate_text().is_empty());
    s.fe.as_mut().unwrap().approximate = true;
    assert!(s.is_approximate());
    assert_eq!(s.status(), capacity::UsageStatus::Approximate);
    assert_eq!(s.status().as_str(), "approximate");
    assert_eq!(s.approximate_text(), capacity::APPROXIMATE_FE);
    let usage = vec![capacity::CaseUsage {
        case: "wind".into(),
        structures: vec![s.clone()],
    }];
    let csv = capacity::usage_csv(&usage);
    let row = csv.lines().nth(1).unwrap();
    assert!(
        row.ends_with(&format!(",\"{}\"", capacity::APPROXIMATE_FE)),
        "{row}"
    );
    // Over 100 %: still approximate (the number is not to be trusted
    // either way), as an incomplete structure is incomplete.
    s.fe.as_mut().unwrap().usage = 1.5;
    assert_eq!(s.status(), capacity::UsageStatus::Approximate);
}

/// A load case's own values for the structures (`[case.structure]`,
/// version 2): read with units, checked, and the file written at version
/// 2 only when a case has one; version 1 files read as before.
#[test]
fn structure_values_of_a_load_case_are_read_and_checked() {
    let old = "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"a\"\nweather = \"w\"\n";
    let c = LoadCases::parse(old).unwrap();
    assert_eq!(c.cases[0].structure, None);
    assert_eq!(c.written_version(), 1);
    let new = "format = \"ohl-load-cases\"\nversion = 2\n[[case]]\nname = \"a\"\nweather = \"w\"\n\
               transverse = 1.4\n[case.structure]\nwind_pressure = \"0.9 kPa\"\nwind_factor = 1.2\n";
    let c = LoadCases::parse(new).unwrap();
    let s = c.cases[0].structure.unwrap();
    assert_eq!(s.wind_pressure, Some(900.0));
    assert_eq!(s.wind_factor, Some(1.2));
    assert_eq!(c.written_version(), 2);
    // Only the pressure: the factor stays the transverse one.
    let only = "[[case]]\nname = \"a\"\nweather = \"w\"\ntransverse = 1.4\n\
                [case.structure]\nwind_pressure = 700\n";
    let c = LoadCases::parse(only).unwrap();
    let w: WeatherCase =
        toml::from_str("name = \"w\"\ntemperature = \"15 °C\"\nwind_pressure = \"600 Pa\"\n")
            .unwrap();
    let b = c.cases[0].structure_wind(&w);
    assert_eq!((b.pressure, b.factor), (700.0, 1.4));
    assert!(b.own_pressure && !b.own_factor);
    assert!((b.factored() - 980.0).abs() < 1e-9);
    // Without the table: the weather case's pressure, the transverse factor.
    let plain = LoadCases::parse(old).unwrap().cases[0].structure_wind(&w);
    assert_eq!(
        (plain.pressure, plain.factor, plain.towards),
        (600.0, 1.0, -1.0)
    );
    assert!(!plain.is_own() && plain.origin().is_empty());
    // An empty table writes version 1.
    let empty =
        LoadCases::parse("[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\n").unwrap();
    assert_eq!(empty.written_version(), 1);
    for bad in [
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nwind_pressure = -5\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nwind_factor = 101\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\ndrag = 2\n",
        "format = \"ohl-load-cases\"\nversion = 3\n[[case]]\nname = \"a\"\nweather = \"w\"\n",
    ] {
        assert!(LoadCases::parse(bad).is_err(), "{bad}");
    }
}

/// The example line in its wind case (high wind, 600 Pa, transverse 1.4)
/// with the structures' own values. Invented numbers, by hand:
///
/// - `[case.structure]` 420 Pa × 2.0 = 840 Pa = 600 Pa × 1.4: every
///   check of the structures (member wind of the FE model, wind on the
///   pole) is the same as without the table; the wires' loads are the
///   same (they keep the weather case's 600 Pa × 1.4); a 0–20 m band of
///   10 m² with drag 3 carries 420 × 3 × 10 = 12 600 N unfactored (600 ×
///   30 = 18 000 N without), 12 600 × 2.0 = 25 200 N factored (18 000 ×
///   1.4 = 25 200 N without).
/// - 900 Pa × 1.2 = 1 080 Pa: the band 900 × 30 = 27 000 N, factored
///   32 400 N; the tower's FE usage is that of its model solved by hand
///   with the member wind of 1 080 Pa; the pole's that of the pole check
///   by hand with a wind of 1 080 Pa to −x.
#[test]
fn the_structures_take_the_load_cases_own_wind() {
    let sections = ohl_sections::SectionLibrary::parse(
        "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
         yield = \"355 MPa\"\n\
         [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"110 mm\"\nt = \"12 mm\"\n\
         [[section]]\nid = \"brace\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"70 mm\"\nt = \"7 mm\"\n\
         [[section]]\nid = \"arm\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
         h = \"90 mm\"\nt = \"9 mm\"\n",
    )
    .unwrap();
    let loaded = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut types = ohl_structures::StructureLibrary::default();
    for t in loaded.iter() {
        let mut t = t.clone();
        if t.id == "type-02" {
            t.wind = Some(
                toml::from_str::<ohl_structures::StructureType>(
                    "id = \"x\"\nname = \"x\"\nkind = \"other\"\n[[wind.band]]\nfrom = \"0 m\"\nto = \"20 m\"\narea = \"10 m²\"\ndrag = 3.0\n",
                )
                .unwrap()
                .wind
                .unwrap(),
            );
        }
        // Kept where it was read from (its model and pole files beside).
        let path = loaded.path(&t.id).unwrap().to_path_buf();
        types.insert(&path, t).unwrap();
    }
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, library, weather, cases) = setup();
    let budget = ohl_wirefe::default_fe_budget();
    let wind_case = cases.cases[1].clone();
    assert_eq!(
        (wind_case.name.as_str(), wind_case.transverse),
        ("wind", 1.4)
    );
    let with = |pressure: f64, factor: f64| {
        let mut c = wind_case.clone();
        c.structure = Some(StructureWind {
            wind_pressure: Some(pressure),
            wind_factor: Some(factor),
        });
        c
    };
    let run = |case: LoadCase| {
        let wires = structure_loads(
            &line,
            &library,
            &weather,
            std::slice::from_ref(&case),
            &budget,
        )
        .unwrap();
        let mut loads = wires.clone();
        add_structure_wind(&mut loads, &line, &types, &weather);
        let usage =
            capacity::usage_with_sections(&line, &types, &sections, &library, &weather, &wires);
        (wires, loads, usage)
    };
    let at = |u: &[capacity::CaseUsage], id: &str| {
        u[0].structures
            .iter()
            .find(|s| s.structure == id)
            .unwrap()
            .clone()
    };
    let band = |l: &[CaseLoads]| {
        l[0].loads
            .iter()
            .find(|l| l.point.is_empty() && l.structure == "T2")
            .unwrap()
            .clone()
    };
    let (plain_wires, plain_loads, plain) = run(wind_case.clone());
    let (same_wires, same_loads, same) = run(with(420.0, 2.0));
    // The wires keep the weather case's wind.
    assert_eq!(plain_wires[0].loads, same_wires[0].loads);
    assert!((plain_wires[0].structure_wind.factored() - 840.0).abs() < 1e-9);
    assert!((same_wires[0].structure_wind.factored() - 840.0).abs() < 1e-9);
    // The same design pressure on the structures: the same checks.
    for id in ["T2", "T6"] {
        assert_eq!(at(&plain, id).fe, at(&same, id).fe, "{id}");
        assert_eq!(at(&plain, id).pole, at(&same, id).pole, "{id}");
    }
    assert!(at(&plain, "T2").fe.is_some() && at(&plain, "T6").pole.is_some());
    // The band: 18 000 N / 12 600 N unfactored, 25 200 N factored both.
    let (b0, b1) = (band(&plain_loads), band(&same_loads));
    assert!((b0.local[0] + 18_000.0).abs() < 1e-6, "{b0:?}");
    assert!((b1.local[0] + 12_600.0).abs() < 1e-6, "{b1:?}");
    assert!((b0.factored[0] + 25_200.0).abs() < 1e-6);
    assert!((b1.factored[0] + 25_200.0).abs() < 1e-6);

    // 900 Pa × 1.2.
    let (wires, loads, own) = run(with(900.0, 1.2));
    assert_eq!(plain_wires[0].loads, wires[0].loads);
    let b = band(&loads);
    assert!((b.local[0] + 27_000.0).abs() < 1e-6, "{b:?}");
    assert!((b.factored[0] + 32_400.0).abs() < 1e-6, "{b:?}");
    let factored_at = |id: &str| {
        let mut at: std::collections::BTreeMap<String, [f64; 3]> = Default::default();
        for l in wires[0]
            .loads
            .iter()
            .filter(|l| l.structure == id && !l.point.is_empty())
        {
            let e = at.entry(l.point.clone()).or_default();
            for (sum, f) in e.iter_mut().zip(l.factored) {
                *sum += f;
            }
        }
        at
    };
    // The tower by hand: its model, the wires' loads, the member wind.
    let model =
        ohl_structures_fe::Model::load_with(types.path("type-02").unwrap(), &sections).unwrap();
    let mut nodal = Vec::new();
    for (label, [tr, lo, ve]) in factored_at("T2") {
        nodal.extend(model.point_loads(&label, [tr, lo, -ve]).unwrap());
    }
    nodal.extend(model.member_wind(1080.0, [-1.0, 0.0, 0.0]));
    let (_, usage) = ohl_structures_fe::analyse(
        &model,
        &ohl_structures_fe::Loads {
            nodal,
            self_weight: wind_case.vertical,
        },
        wind_case.strength,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let worst = ohl_structures_fe::check::worst(&usage).unwrap();
    let fe = at(&own, "T2").fe.unwrap();
    assert!((fe.usage - worst.usage.unwrap()).abs() < 1e-12, "{fe:?}");
    assert!(fe.usage > at(&plain, "T2").fe.unwrap().usage);
    assert!(
        at(&own, "T2")
            .notes
            .iter()
            .any(|n| n.contains("member wind") && n.contains("900 Pa × 1.2")),
        "{:?}",
        at(&own, "T2").notes
    );
    // The pole by hand: the wires' loads at its points, 1 080 Pa to −x.
    let t5 = types.get("type-05").unwrap();
    let dir = types.path("type-05").unwrap().parent().unwrap();
    let pole =
        ohl_structures_fe::pole::structure_pole(t5, dir, &ohl_sections::SectionLibrary::default())
            .unwrap()
            .unwrap();
    let on_pole: Vec<ohl_structures_fe::PoleLoad> = factored_at("T6")
        .into_iter()
        .map(|(label, [tr, lo, ve])| ohl_structures_fe::PoleLoad {
            point: t5.point_local(&label, None).unwrap(),
            force: [tr, lo, -ve],
        })
        .collect();
    let by_hand = ohl_structures_fe::check_pole(
        &pole,
        &on_pole,
        [-1080.0, 0.0],
        wind_case.strength,
        true,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let p = at(&own, "T6").pole.unwrap();
    assert!((p.usage - by_hand.usage).abs() < 1e-12, "{p:?} {by_hand:?}");
    assert!(p.usage > at(&plain, "T6").pole.unwrap().usage);
}
