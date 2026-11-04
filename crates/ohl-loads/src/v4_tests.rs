// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load case file version 4 (owner 2026-11-03): removed conductors,
//! several broken wires together, the wires' wind side, the structure
//! wind's direction, ice and snow on the structures, per-pylon sets.
//! Every line and value here is INVENTED.

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Three level 300 m spans with two wires: set 1 (S1) at 27 m and set 2
/// (S2) at 22 m, each on a 3 m suspension string at T2 and T3, dead-ended
/// at T1 and T4. INVENTED.
fn two_wires() -> Line {
    let structure = |id: &str, x: f64, suspension: bool| {
        let strings = if suspension {
            format!(
                r#""points": [{{"label": "A1", "position": [{x}, 0, 30]}},
                              {{"label": "A2", "position": [{x}, 0, 25]}}],
                   "strings": [{{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                                 "weight": 800.0, "stiffness": 5e7}},
                               {{"label": "S2", "top": "A2", "wire": "W2", "length": 3.0,
                                 "weight": 800.0, "stiffness": 5e7}}],"#
            )
        } else {
            String::new()
        };
        format!(
            r#"{{"id": "{id}", "position": [{x}, 0, 0], "orientation_deg": -90, {strings}
               "wires": [{{"label": "W1", "position": [{x}, 0, 27], "set": 1}},
                         {{"label": "W2", "position": [{x}, 0, 22], "set": 2}}]}}"#
        )
    };
    let section = |id: &str, set: u32| {
        format!(
            r#"{{"id": "{id}", "cable": "acsr-drake-26-7", "bundle": 1,
               "points": [{{"structure": "T1", "set": {set}}}, {{"structure": "T2", "set": {set}}},
                          {{"structure": "T3", "set": {set}}}, {{"structure": "T4", "set": {set}}}],
               "stringing": {{"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}}}"#
        )
    };
    Line::from_json(&format!(
        r#"{{"format": "ohl-line", "version": 6, "name": "two wires", "crs": "local",
             "structures": [{}, {}, {}, {}],
             "sections": [{}, {}]}}"#,
        structure("T1", 0.0, false),
        structure("T2", 300.0, true),
        structure("T3", 600.0, true),
        structure("T4", 900.0, false),
        section("S1", 1),
        section("S2", 2),
    ))
    .unwrap()
}

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"calm\"\ntemperature = \"10 °C\"\n\
         [[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n\
         [[case]]\nname = \"ice\"\ntemperature = \"-5 °C\"\nice_thickness = \"10 mm\"\n\
         ice_density = \"900 kg/m³\"\n",
    )
    .unwrap()
    .cases
}

fn at<'a>(c: &'a CaseLoads, structure: &str, point: &str) -> Option<&'a AttachmentLoad> {
    c.loads
        .iter()
        .find(|l| l.structure == structure && l.point == point)
}

/// Version 4 read, checked, written and read back the same; a file
/// without a version-4 value is still written at its old version; the
/// version-3 single `[case.broken]` reads as one broken wire.
#[test]
fn version_4_round_trip() {
    let text = r#"format = "ohl-load-cases"
version = 4
name = "construction"

[[case]]
name = "stringing T7"
description = "invented"
weather = "wind"
condition = "creep"
vertical = 1.35
transverse = 1.5
tags = ["construction"]
wind_from = "left"
use_local = true

[case.structure]
wind_pressure = "900 Pa"
wind_factor = 1.2
direction = 30
ice = "20 mm"
ice_density = "900 kg/m³"
snow = true
snow_shape = 0.8
snow_exposure = 1.0
snow_thermal = 1.0

[[case.removed]]
section = "S3"
set = 2

[[case.removed]]
section = "S4"

[[case.broken]]
section = "S1"
span = 2

[[case.broken]]
section = "S1"
span = 3
set = 1

[[case]]
name = "every"
weather = "calm"

[case.structure]
direction = "ahead"
ice = true

[[case.broken]]
section = "S2"
span = "every"

[[case.broken]]
section = "S1"
span = 1
"#;
    let c = LoadCases::parse(text).unwrap();
    assert_eq!(c.written_version(), 4);
    let a = &c.cases[0];
    assert_eq!(a.wind_from, Some(ohl_weather::WindSide::Left));
    assert!(a.use_local);
    assert_eq!(a.removed.len(), 2);
    assert_eq!(a.removed[0].describe(), "S3 set 2 removed");
    assert_eq!(a.broken_wires().count(), 2);
    assert_eq!(a.broken.as_ref().unwrap().span, BrokenSpan::One(2));
    assert_eq!(a.on_structure.direction, Some(WindDirection::Angle(30.0)));
    assert_eq!(a.on_structure.ice, Some(StructureIce::Thickness(0.02)));
    assert_eq!(a.on_structure.ice_density, Some(900.0));
    assert!(a.on_structure.snow);
    assert_eq!(a.structure.unwrap().wind_pressure, Some(900.0));
    assert_eq!(c.cases[1].on_structure.ice, Some(StructureIce::Local));
    assert_eq!(
        c.cases[1].on_structure.direction,
        Some(WindDirection::Ahead)
    );
    // Written and read back: the same cases.
    let written = c.to_toml();
    assert!(written.contains("version = 4\n"), "{written}");
    let back = LoadCases::parse(&written).unwrap();
    assert_eq!(back.cases, c.cases, "{written}");
    assert_eq!(back.name, c.name);
    // Older files: written at their version, read back the same.
    for (old, version) in [
        (
            "[[case]]\nname = \"a\"\nweather = \"w\"\nvertical = 1.2\n",
            1,
        ),
        (
            "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nwind_factor = 1.2\n",
            2,
        ),
        (
            "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"S\"\nspan = 2\n",
            3,
        ),
    ] {
        let c = LoadCases::parse(old).unwrap();
        assert_eq!(c.written_version(), version, "{old}");
        let w = c.to_toml();
        assert!(w.contains(&format!("version = {version}\n")), "{w}");
        if version == 3 {
            assert!(w.contains("\n[case.broken]\n"), "{w}");
        }
        assert_eq!(LoadCases::parse(&w).unwrap().cases, c.cases, "{w}");
    }
    // Refused: "every" with another broken span of its section, two
    // "every", a section removed and broken, removed twice, snow values
    // without snow, a density with ice = false, a bad direction, ice not
    // true, false, "none" or a thickness, a version above 4.
    for bad in [
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.broken]]\nsection = \"S\"\nspan = \"every\"\n[[case.broken]]\nsection = \"S\"\nspan = 1\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.broken]]\nsection = \"S\"\nspan = \"every\"\n[[case.broken]]\nsection = \"T\"\nspan = \"every\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.broken]]\nsection = \"S\"\nspan = 1\n[[case.broken]]\nsection = \"S\"\nspan = 1\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.removed]]\nsection = \"S\"\n[[case.broken]]\nsection = \"S\"\nspan = 1\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.removed]]\nsection = \"S\"\n[[case.removed]]\nsection = \"S\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[[case.removed]]\nsection = \"S\"\nspan = 1\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nsnow_shape = 0.8\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nice = false\nice_density = 900\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\ndirection = \"up\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nice = \"lots\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\nice = -0.01\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\nwind_from = \"ahead\"\n",
        "version = 5\n[[case]]\nname = \"a\"\nweather = \"w\"\n",
    ] {
        assert!(LoadCases::parse(bad).is_err(), "{bad}");
    }
    // "every" in one section beside a break in another: allowed.
    assert!(c.cases[1].validate().is_ok());
}

/// A removed conductor (S2 not strung yet): no load of S2 on any
/// structure (its strings' points A2 carry nothing), S1's loads as in
/// the case with both wires; said in the notes.
#[test]
fn a_removed_conductor_gives_no_loads() {
    let line = two_wires();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let whole = LoadCase::from_weather("calm");
    let removed = LoadCase {
        name: "S2 not strung".into(),
        removed: vec![RemovedWire {
            section: "S2".into(),
            set: Some(2),
        }],
        ..LoadCase::from_weather("calm")
    };
    let r = structure_loads(&line, &lib, &weather, &[whole, removed], &budget).unwrap();
    let (w, x) = (&r[0], &r[1]);
    assert!(w.failed.is_empty() && x.failed.is_empty(), "{:?}", x.failed);
    for s in ["T1", "T2", "T3", "T4"] {
        assert!(
            x.loads
                .iter()
                .all(|l| !l.sections.iter().any(|s| s == "S2"))
        );
        assert!(x.loads.iter().all(|l| !l.sets.contains(&2)), "{s}");
    }
    for s in ["T2", "T3"] {
        assert!(at(w, s, "A2").is_some());
        assert!(at(x, s, "A2").is_none(), "{s}: {:?}", at(x, s, "A2"));
        let (a, b) = (at(w, s, "A1").unwrap(), at(x, s, "A1").unwrap());
        for k in 0..3 {
            assert!((a.local[k] - b.local[k]).abs() < 1.0, "{s}: {a:?} {b:?}");
        }
    }
    assert!(
        x.notes.iter().any(|n| n.contains("S2 set 2 removed")),
        "{:?}",
        x.notes
    );
    // A section not in the line, or a set it does not have: refused.
    let mut bad = r[1].case.clone();
    bad.removed[0].set = Some(3);
    assert!(structure_loads(&line, &lib, &weather, &[bad], &budget).is_err());
}

/// Two spans of one section broken together, in one solve: S1's spans 1
/// and 2 gone, T2 between them holds no wire (its string hangs: its
/// weight down, nothing along the line), T3 swings ahead and is pulled
/// ahead by span 3 alone; S2 whole. Not the same as either break alone.
#[test]
fn two_broken_spans_solve_together() {
    let line = two_wires();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let broken = |spans: &[usize]| {
        let mut c = LoadCase {
            name: format!("broken {spans:?}"),
            ..LoadCase::from_weather("calm")
        };
        c.set_broken(
            spans
                .iter()
                .map(|&k| BrokenWire {
                    section: "S1".into(),
                    span: BrokenSpan::One(k),
                    set: None,
                })
                .collect(),
        );
        c
    };
    let r = structure_loads(
        &line,
        &lib,
        &weather,
        &[broken(&[1, 2]), broken(&[2])],
        &budget,
    )
    .unwrap();
    let (both, one) = (&r[0], &r[1]);
    assert!(both.failed.is_empty(), "{:?}", both.failed);
    let t2 = at(both, "T2", "A1").unwrap();
    assert!(t2.local[1].abs() < 1.0, "{:?}", t2.local);
    assert!((t2.local[2] - 800.0).abs() < 1.0, "{:?}", t2.local);
    let t3 = at(both, "T3", "A1").unwrap();
    assert!(t3.local[1] > 1000.0, "{:?}", t3.local);
    // T2 with span 2 alone broken still holds span 1 (pulled back).
    assert!(at(one, "T2", "A1").unwrap().local[1] < -1000.0);
    // S2 untouched: as whole at T2.
    let whole = structure_loads(
        &line,
        &lib,
        &weather,
        &[LoadCase::from_weather("calm")],
        &budget,
    )
    .unwrap();
    let (a, b) = (
        at(&whole[0], "T2", "A2").unwrap(),
        at(both, "T2", "A2").unwrap(),
    );
    assert!((a.local[1] - b.local[1]).abs() < 1.0 && (a.local[2] - b.local[2]).abs() < 1.0);
    assert_eq!(t3.broken_span, Some([1, 1, 1]));
    assert!(both.notes.iter().any(|n| n.contains("broken together")));
    // The solver itself: one shape, both spans gone.
    let shapes = ohl_line::solve::solve_line_broken_spans(
        &line,
        &lib,
        &weather[0],
        &[("S1", vec![0, 1])],
        &budget,
    )
    .unwrap();
    let s1 = shapes[0].as_ref().unwrap();
    assert_eq!(s1.broken_spans, vec![0, 1]);
    assert!(s1.spans[0].points.is_empty() && s1.spans[1].points.is_empty());
    assert!(s1.spans[2].horizontal_tension > 1000.0);
}

/// "every" in one section beside a fixed break in another: per span of
/// the "every" section a solve with the fixed break too, each kept.
#[test]
fn every_span_beside_a_fixed_break() {
    let line = two_wires();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let mut c = LoadCase {
        name: "mixed".into(),
        ..LoadCase::from_weather("calm")
    };
    c.set_broken(vec![
        BrokenWire {
            section: "S2".into(),
            span: BrokenSpan::One(1),
            set: None,
        },
        BrokenWire {
            section: "S1".into(),
            span: BrokenSpan::Every,
            set: None,
        },
    ]);
    let r = structure_loads(&line, &lib, &weather, &[c], &budget).unwrap();
    assert_eq!(r[0].broken_each.len(), 3);
    // In every break, S2's span 1 is gone: T2's A2 pulled ahead by span 2.
    for b in &r[0].broken_each {
        let a2 = b
            .loads
            .iter()
            .find(|l| l.structure == "T2" && l.point == "A2")
            .unwrap();
        assert!(a2.local[1] > 1000.0, "span {}: {:?}", b.span, a2.local);
    }
    let split = r[0].each_break();
    assert_eq!(split.len(), 3);
    assert_eq!(split[1].case.broken_wires().count(), 2);
    assert!(
        split[1]
            .case
            .broken_wires()
            .any(|b| b.section == "S1" && b.span == BrokenSpan::One(2))
    );
}

/// The wind on the wires from the case's side: the transverse loads turn
/// sign; a case whose weather has no wind says it has no effect.
#[test]
fn wind_from_turns_the_wires_wind() {
    let line = two_wires();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let right = LoadCase::from_weather("wind");
    let left = LoadCase {
        name: "from the left".into(),
        wind_from: Some(ohl_weather::WindSide::Left),
        ..LoadCase::from_weather("wind")
    };
    let calm = LoadCase {
        name: "calm left".into(),
        wind_from: Some(ohl_weather::WindSide::Left),
        ..LoadCase::from_weather("calm")
    };
    let r = structure_loads(&line, &lib, &weather, &[right, left, calm], &budget).unwrap();
    let (a, b) = (
        at(&r[0], "T2", "A1").unwrap().local[0],
        at(&r[1], "T2", "A1").unwrap().local[0],
    );
    assert!(a.abs() > 100.0 && (a + b).abs() < 1e-6 * a.abs(), "{a} {b}");
    assert!(r[2].notes.iter().any(|n| n.contains("has no effect")));
    // The body wind follows the case's side too.
    let w = &weather[1];
    assert_eq!(r[1].case.structure_wind(w).towards, 1.0);
}

/// The structure wind's direction: at 90° (from ahead) a band's force
/// is all along the line (−y, |F| = q·c·A), none across; at 30° split by
/// cos and sin; the default from the side as before.
#[test]
fn structure_wind_at_90_degrees_is_longitudinal() {
    let mut line = two_wires();
    line.structures[1].structure_type = Some("band".into());
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
    let case = |name: &str, d: Option<WindDirection>| {
        let mut c = LoadCase {
            name: name.into(),
            longitudinal: 1.1,
            transverse: 1.3,
            ..LoadCase::from_weather("wind")
        };
        c.on_structure.direction = d;
        c
    };
    let mut r = structure_loads(
        &line,
        &lib,
        &weather,
        &[
            case("across", None),
            case("ahead", Some(WindDirection::Angle(90.0))),
            case("30", Some(WindDirection::Angle(30.0))),
        ],
        &budget,
    )
    .unwrap();
    add_structure_wind(&mut r, &line, &types, &weather);
    let band = |c: &CaseLoads| {
        c.loads
            .iter()
            .find(|l| l.structure == "T2" && l.point.is_empty())
            .unwrap()
            .clone()
    };
    let f = 500.0 * 3.0 * 10.0;
    let across = band(&r[0]);
    assert!((across.local[0] + f).abs() < 1e-9 && across.local[1] == 0.0);
    let ahead = band(&r[1]);
    assert!(ahead.local[0].abs() < 1e-9 * f, "{:?}", ahead.local);
    assert!((ahead.local[1] + f).abs() < 1e-9 * f, "{:?}", ahead.local);
    // The structures' factor (here the transverse one) on both parts.
    assert!((ahead.factored[1] - ahead.local[1] * 1.3).abs() < 1e-6);
    // In line axes: the structure's ahead (orientation −90°: ahead is +x
    // east... whatever it is, the force is along it).
    let (_, a) = ohl_line::links::axes(line.structures[1].orientation_deg);
    assert!((ahead.force[0] - ahead.local[1] * a[0]).abs() < 1e-6);
    let thirty = band(&r[2]);
    let r30 = 30f64.to_radians();
    assert!((thirty.local[0] + f * r30.cos()).abs() < 1e-6);
    assert!((thirty.local[1] + f * r30.sin()).abs() < 1e-6);
    assert!(r[1].structure_wind.has_longitudinal() && !r[0].structure_wind.has_longitudinal());
    assert!(
        r[1].notes.iter().any(|n| n.contains("from 90°")),
        "{:?}",
        r[1].notes
    );
}

/// Snow by hand: q = μ·C_e·C_T·s_k (SIA 261 (9)), s_k from the reference
/// altitude by SIA 261 (10): h0 = 1000 m gives s_k = [1 + (1000/350)²] ×
/// 0.4 kN/m² = 3.6653 kN/m², q = 0.8 × 1 × 1 × s_k = 2.9322 kN/m²; on a
/// horizontal round bar of 50 mm, 2 m long: q × 0.05 × 2; a vertical bar
/// takes none. Ice of 10 mm, 900 kg/m³ all round: ρ g (P t + π t²) per
/// metre, P the bar's outline.
#[test]
fn snow_and_ice_on_the_members_by_hand() {
    let local = ohl_line::local::LocalConditions {
        snow_altitude: Some(1000.0),
        structure_ice: Some(0.01),
        ..Default::default()
    };
    let mut s = ohl_line::Structure::new("T".to_owned(), [0.0; 3]);
    s.local = local;
    let mut case = LoadCase::from_weather("ice");
    case.on_structure.snow = true;
    case.on_structure.ice = Some(StructureIce::Local);
    let snow = case.structure_snow_at(Some(&s)).unwrap();
    let s_k = (1.0 + (1000.0f64 / 350.0).powi(2)) * 400.0;
    assert!((snow.ground - s_k).abs() < 1e-9);
    assert!((s_k - 3665.3).abs() < 0.1);
    assert!((snow.load() - 0.8 * s_k).abs() < 1e-9);
    assert!((snow.load() - snow_load(s_k, 0.8, 1.0, 1.0)).abs() < 1e-9);
    // C_e and C_T given.
    case.on_structure.snow_exposure = Some(1.2);
    case.on_structure.snow_thermal = Some(0.9);
    let given = case.structure_snow_at(Some(&s)).unwrap();
    assert!((given.load() - 0.8 * 1.2 * 0.9 * s_k).abs() < 1e-9);
    case.on_structure.snow_exposure = None;
    case.on_structure.snow_thermal = None;
    // Ice: the local thickness, the weather case's density.
    let w = &weather()[2];
    let ice = case.structure_ice_at(w, Some(&s)).unwrap().unwrap();
    assert_eq!((ice.thickness, ice.density), (0.01, 900.0));
    // Without a density anywhere: refused.
    assert!(case.structure_ice_at(&weather()[0], Some(&s)).is_err());
    // No local ice: none.
    let bare = ohl_line::Structure::new("U".to_owned(), [0.0; 3]);
    assert_eq!(case.structure_ice_at(w, Some(&bare)), Ok(None));
    assert!(case.structure_snow_at(Some(&bare)).is_none());
    // On a model: a horizontal and a vertical bar of 50 mm.
    let lib = ohl_sections::SectionLibrary::parse(
        "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
         [[section]]\nid = \"B50\"\nshape = \"round_bar\"\nmaterial = \"steel\"\nd = \"50 mm\"\n",
    )
    .unwrap();
    let model = ohl_structures_fe::Model::parse_with(
        "[analysis]\nnonlinear = false\nself_weight = false\n\
         [[node]]\nid = \"a\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
         [[node]]\nid = \"b\"\npoint = [0, 0, 2]\n\
         [[node]]\nid = \"c\"\npoint = [2, 0, 2]\n\
         [[member]]\nid = \"v\"\nnodes = [\"a\", \"b\"]\nsection = \"B50\"\n\
         [[member]]\nid = \"h\"\nnodes = [\"b\", \"c\"]\nsection = \"B50\"\n",
        &lib,
    )
    .unwrap();
    let weights = analysis::body_weights(&model, Some(ice), Some(snow.load()));
    assert!(
        (weights.snow - snow.load() * 0.05 * 2.0).abs() < 1e-6,
        "{}",
        weights.snow
    );
    let p = analysis::outline_perimeter(model.sections[0].profile.as_ref().unwrap());
    // The round bar's outline: a 16-gon round the 50 mm circle.
    assert!((p - 16.0 * 0.05 * (std::f64::consts::PI / 16.0).sin()).abs() < 1e-9);
    let per_metre =
        900.0 * ohl_core::units::STANDARD_GRAVITY * (p * 0.01 + std::f64::consts::PI * 1e-4);
    assert!(
        (weights.ice - per_metre * 4.0).abs() < 1e-6,
        "{}",
        weights.ice
    );
    let down: f64 = weights.nodal.iter().map(|l| -l.force[2]).sum();
    assert!((down - weights.ice - weights.snow).abs() < 1e-6);
    assert_eq!(weights.widening, vec![0.02, 0.02]);
}

fn ex() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

/// The synthetic example line, its weather cases.
fn example() -> (Line, Vec<WeatherCase>) {
    let mut line = Line::load(&ex().join("synthetic-line.ohl.json")).unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    (line, weather)
}

/// Ice on the structures (version 4) in the checks by analysis: the
/// finite-element model's members iced (weight and wider for the wind)
/// and the pole's shaft iced, each said in the notes and using no less
/// of the structure than the same case bare (more for some); listed per
/// structure by `add_structure_loads`; refused without a density.
#[test]
fn ice_on_the_structures_in_the_checks() {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, weather) = example();
    let library = library();
    let budget = default_budget();
    let bare = LoadCase::from_weather("everyday");
    let mut iced = LoadCase {
        name: "iced".into(),
        ..bare.clone()
    };
    iced.on_structure.ice = Some(StructureIce::Thickness(0.03));
    iced.on_structure.ice_density = Some(900.0);
    let mut loads =
        structure_loads(&line, &library, &weather, &[bare, iced.clone()], &budget).unwrap();
    let usage = capacity::usage(&line, &types, &library, &weather, &loads);
    let mut more = 0;
    for (a, b) in usage[0].structures.iter().zip(&usage[1].structures) {
        for (x, y) in [(&a.fe, &b.fe), (&a.pole, &b.pole)] {
            if let (Some(x), Some(y)) = (x, y)
                && y.usage.is_finite()
            {
                assert!(y.usage >= x.usage - 1e-9, "{}: {x:?} {y:?}", a.structure);
                more += usize::from(y.usage > x.usage + 1e-6);
            }
        }
        if b.fe.is_some() || b.pole.is_some() {
            assert!(
                b.notes.iter().any(|n| n.contains("ice 30 mm")),
                "{}: {:?}",
                b.structure,
                b.notes
            );
        }
    }
    assert!(more >= 1, "the ice uses more of some structure");
    // Listed per structure, down, factored by the vertical factor.
    let sections = ohl_sections::SectionLibrary::default();
    add_structure_loads(&mut loads, &line, &types, &sections, &weather);
    let rows: Vec<&AttachmentLoad> = loads[1]
        .loads
        .iter()
        .filter(|l| l.label == "ice on the structure")
        .collect();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|l| l.local[2] > 0.0 && l.force[2] < 0.0));
    assert!(
        loads[0]
            .loads
            .iter()
            .all(|l| l.label != "ice on the structure")
    );
    // Without a density (none in the case, none in the weather case):
    // refused when computed.
    iced.on_structure.ice_density = None;
    let w = weather.iter().find(|w| w.name == "everyday").unwrap();
    if w.ice_density.is_none() {
        let err = structure_loads(&line, &library, &weather, &[iced], &budget).unwrap_err();
        assert!(err.contains("ice_density"), "{err}");
    }
}

/// `use_local`: the wires and the body wind take the structures'
/// wind_static (one structure × 1.5): its load across the line larger
/// than the same case without.
#[test]
fn use_local_takes_the_structures_wind() {
    let (mut line, weather) = example();
    let library = library();
    let budget = default_budget();
    let windy = weather
        .iter()
        .find(|w| w.wind_pressure.si() > 0.0)
        .unwrap()
        .name
        .clone();
    let plain = LoadCase::from_weather(&windy);
    let local = LoadCase {
        name: "local".into(),
        use_local: true,
        ..plain.clone()
    };
    let id = line.structures[1].id.clone();
    line.structures[1].local.wind_static = Some(1.5);
    let r = structure_loads(&line, &library, &weather, &[plain, local], &budget).unwrap();
    let across = |c: &CaseLoads| -> f64 {
        c.loads
            .iter()
            .filter(|l| l.structure == id)
            .map(|l| l.local[0].abs())
            .sum()
    };
    assert!(
        across(&r[1]) > 1.05 * across(&r[0]),
        "{} {}",
        across(&r[1]),
        across(&r[0])
    );
    assert!(r[1].notes.iter().any(|n| n.contains("local conditions")));
    let w = weather.iter().find(|w| w.name == windy).unwrap();
    let body = r[1].case.structure_wind_at(w, line.structure(&id));
    assert!((body.pressure - 1.5 * w.wind_pressure.si()).abs() < 1e-9);
}

/// Per-pylon sets: T2 takes only "calm" (its own), T3 its type's default
/// "wind"; T1 and T4 every case. A name not in the library is said.
#[test]
fn per_pylon_load_case_sets() {
    let mut line = two_wires();
    line.structures[1].load_cases = vec!["calm".into()];
    line.structures[2].structure_type = Some("tangent".into());
    line.structure_loads
        .type_load_cases
        .insert("tangent".into(), vec!["wind".into(), "gone".into()]);
    assert_eq!(line.load_cases_for(&line.structures[1]), ["calm"]);
    assert_eq!(line.load_cases_for(&line.structures[2]), ["wind", "gone"]);
    assert!(line.load_cases_for(&line.structures[0]).is_empty());
    assert!(line.takes_load_case("T1", "anything"));
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let cases = [
        LoadCase::from_weather("calm"),
        LoadCase::from_weather("wind"),
    ];
    let cases = [
        LoadCase {
            name: "calm".into(),
            ..cases[0].clone()
        },
        LoadCase {
            name: "wind".into(),
            ..cases[1].clone()
        },
    ];
    let r = structure_loads(&line, &lib, &weather, &cases, &budget).unwrap();
    let has = |c: &CaseLoads, s: &str| c.loads.iter().any(|l| l.structure == s);
    assert!(has(&r[0], "T2") && !has(&r[0], "T3"));
    assert!(!has(&r[1], "T2") && has(&r[1], "T3"));
    for c in &r {
        assert!(has(c, "T1") && has(c, "T4"));
        assert!(
            c.notes
                .iter()
                .any(|n| n.contains("tangent") && n.contains("gone")),
            "{:?}",
            c.notes
        );
    }
    assert!(
        r[0].notes
            .iter()
            .any(|n| n.contains("not in the load case set"))
    );
}

/// `ice = false` and `ice = "none"` (2026-11-04): no ice on the
/// structures, written as `false` and read back; a density alone (the
/// density of the local ice a case taking the local conditions puts on
/// them) is kept.
#[test]
fn no_ice_on_the_structures_is_written_and_read() {
    for text in ["ice = false", "ice = \"none\""] {
        let c = LoadCases::parse(&format!(
            "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.structure]\n{text}\n"
        ))
        .unwrap();
        assert_eq!(c.cases[0].on_structure.ice, Some(StructureIce::NoIce));
        let written = c.to_toml();
        assert!(written.contains("ice = false"), "{written}");
        assert_eq!(LoadCases::parse(&written).unwrap().cases, c.cases);
    }
    let c = LoadCases::parse(
        "[[case]]\nname = \"a\"\nweather = \"w\"\nuse_local = true\n\
         [case.structure]\nice_density = 900\n",
    )
    .unwrap();
    assert_eq!(c.cases[0].on_structure.ice_density, Some(900.0));
    assert_eq!(LoadCases::parse(&c.to_toml()).unwrap().cases, c.cases);
}

/// The structures' local ice in a case taking the local conditions (owner
/// 2026-11-04: "yes apply local ice too"), by hand: a structure with 15 mm
/// of local ice gets 15 mm in a case whose weather case has ice ("ice", 10
/// mm on the wires, 900 kg/m³) and takes the local conditions (the
/// check's switch, [`with_local`]); none in a case without ice in its
/// weather ("wind"), none without the local conditions, and the case's own
/// ice wins (a thickness, `ice = false`). Without a density anywhere:
/// refused, said.
#[test]
fn local_ice_in_iced_cases_with_the_local_conditions() {
    let w = weather();
    let (wind, ice) = (&w[1], &w[2]);
    let mut s = ohl_line::Structure::new("T".to_owned(), [0.0; 3]);
    s.local.structure_ice = Some(0.015);
    let plain = LoadCase::from_weather("ice");
    // Without the local conditions: none.
    assert_eq!(plain.structure_ice_at(ice, Some(&s)), Ok(None));
    assert!(!plain.ices_or_snows(ice));
    // With them (the switch): 15 mm of the weather case's 900 kg/m³.
    let local = with_local(vec![plain.clone()], true).remove(0);
    assert!(local.takes_local_ice(ice));
    let on = local.structure_ice_at(ice, Some(&s)).unwrap().unwrap();
    assert_eq!((on.thickness, on.density), (0.015, 900.0));
    // A weather case without ice: none.
    assert!(!local.takes_local_ice(wind));
    assert_eq!(local.structure_ice_at(wind, Some(&s)), Ok(None));
    // A structure without local ice: none.
    let bare = ohl_line::Structure::new("U".to_owned(), [0.0; 3]);
    assert_eq!(local.structure_ice_at(ice, Some(&bare)), Ok(None));
    // The case's own: a thickness, or none.
    let mut own = local.clone();
    own.on_structure.ice = Some(StructureIce::Thickness(0.03));
    assert!(!own.takes_local_ice(ice));
    assert_eq!(
        own.structure_ice_at(ice, Some(&s))
            .unwrap()
            .unwrap()
            .thickness,
        0.03
    );
    own.on_structure.ice = Some(StructureIce::NoIce);
    assert_eq!(own.structure_ice_at(ice, Some(&s)), Ok(None));
    assert!(!own.ices_or_snows(ice));
    // No density in the case or the weather case: refused, the message
    // says why and how.
    let mut no_density = ice.clone();
    no_density.ice_density = None;
    let err = local.structure_ice_at(&no_density, Some(&s)).unwrap_err();
    assert!(
        err.contains("local ice") && err.contains("ice_density") && err.contains("ice = false"),
        "{err}"
    );
    // The case's density is taken.
    let mut dense = local.clone();
    dense.on_structure.ice_density = Some(700.0);
    assert_eq!(
        dense
            .structure_ice_at(&no_density, Some(&s))
            .unwrap()
            .unwrap()
            .density,
        700.0
    );
}

/// The local ice on the line: in an iced case with the switch, the
/// structures giving local ice are named in the notes and their models
/// and poles take it (more usage than without, said per structure); not
/// without the switch.
#[test]
fn local_ice_in_the_structure_checks() {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let (_, mut weather) = example();
    // An INVENTED iced weather case with a density.
    let mut iced = weather
        .iter()
        .find(|w| w.name == "everyday")
        .unwrap()
        .clone();
    iced.name = "INVENTED ice".into();
    iced.ice_thickness = ohl_core::units::Length::from_si(0.01);
    iced.ice_density = Some(ohl_core::units::Density::from_si(900.0));
    weather.push(iced);
    for s in &mut line.structures {
        s.local.structure_ice = Some(0.03);
    }
    let library = library();
    let budget = default_budget();
    let case = LoadCase::from_weather("INVENTED ice");
    let without = structure_loads(
        &line,
        &library,
        &weather,
        std::slice::from_ref(&case),
        &budget,
    )
    .unwrap();
    let with = structure_loads(
        &line,
        &library,
        &weather,
        &with_local(vec![case], true),
        &budget,
    )
    .unwrap();
    assert!(!without[0].notes.iter().any(|n| n.contains("local ice")));
    assert!(
        with[0]
            .notes
            .iter()
            .any(|n| n.starts_with("local ice on the structures") && n.contains("30 mm")),
        "{:?}",
        with[0].notes
    );
    let a = capacity::usage(&line, &types, &library, &weather, &without);
    let b = capacity::usage(&line, &types, &library, &weather, &with);
    let mut more = 0;
    for (x, y) in a[0].structures.iter().zip(&b[0].structures) {
        if y.fe.is_some() || y.pole.is_some() {
            assert!(
                y.notes
                    .iter()
                    .any(|n| n.contains("the structure's local ice 30 mm")),
                "{}: {:?}",
                y.structure,
                y.notes
            );
            assert!(!x.notes.iter().any(|n| n.contains("local ice")));
        }
        for (p, q) in [(&x.fe, &y.fe), (&x.pole, &y.pole)] {
            if let (Some(p), Some(q)) = (p, q)
                && q.usage.is_finite()
                && q.usage > p.usage + 1e-6
            {
                more += 1;
            }
        }
    }
    assert!(more >= 1, "the local ice uses more of some structure");
}
