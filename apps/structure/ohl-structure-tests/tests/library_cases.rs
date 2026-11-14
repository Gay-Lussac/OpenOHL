// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's load case library as a placed structure's load cases in
//! OHL-Structure (owner 2026-11-03: "The library's cases"): its set of
//! the library, each case with its factors, its wind on the body from any
//! angle, its broken wires all broken, its ice and snow on the members.
//! The values are INVENTED (the synthetic example line).

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::LoadCases;
use ohl_structure_link::{
    library_recipes, placed_values_with, structure_case_loads, structure_cases_with,
};
use ohl_structure_loads::{LineQuantity, PlacedValues, WindFrom};
use ohl_structures::StructureLibrary;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

fn example() -> (Line, ohl_cables::Library, Vec<ohl_weather::WeatherCase>) {
    let types = StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let library = ohl_cables::Library::load(&ex().join("../../libraries/cables")).unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    (line, library, weather)
}

/// A case of the library with the wind on the structures from 30° (0 from
/// the right, 90 from ahead), two broken wires (S1's first two spans,
/// broken together) and its own factors; T2 takes it and "calm" (its own
/// set), T3 every case. In OHL-Structure T2 gets exactly its set, each
/// case with the library's factors; the angled wind arrives as an angle
/// (towards (−cos 30°, −sin 30°)); the broken wires are both broken in the
/// line's values (as the core computes the case), and through JSON the
/// file reads back the same.
#[test]
fn a_placed_structure_takes_its_set_with_an_angled_wind_and_two_broken_wires() {
    let (mut line, library, weather) = example();
    let lc = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 4\n\
         [[case]]\nname = \"calm\"\nweather = \"everyday\"\nvertical = 1.2\n\
         [[case]]\nname = \"storm, two broken\"\nweather = \"high wind\"\nvertical = 1.1\n\
         transverse = 1.4\nlongitudinal = 1.3\nstrength = 0.9\n\
         [case.structure]\ndirection = 30\n\
         [[case.broken]]\nsection = \"S1\"\nspan = 1\n\
         [[case.broken]]\nsection = \"S1\"\nspan = 2\n\
         [[case]]\nname = \"not for T2\"\nweather = \"everyday\"\n",
    )
    .unwrap()
    .cases;
    line.structures
        .iter_mut()
        .find(|s| s.id == "T2")
        .unwrap()
        .load_cases = vec!["calm".into(), "storm, two broken".into()];
    let choice = ohl_line::StructureLoads {
        weather_cases: vec!["everyday".into()],
        broken_cases: vec!["storm, two broken".into()],
        ..Default::default()
    };
    let cases = structure_cases_with(&choice, &weather, &lc);
    assert_eq!(cases.library.len(), 3);
    // The broken-wire line result breaks both wires.
    let sent: Vec<&str> = cases.weather.iter().map(|c| c.name.as_str()).collect();
    assert!(
        sent.contains(&"high wind — S1 span 1 broken, S1 span 2 broken — wind from the left"),
        "{sent:?}"
    );
    assert!(
        cases
            .weather
            .iter()
            .filter(|c| c.name.contains("broken"))
            .all(|c| c.broken_wires().count() == 2)
    );
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    assert!(
        loads.library_failed.is_empty(),
        "{:?}",
        loads.library_failed
    );
    let placed = placed_values_with(&loads, &line, "T2", "synthetic");
    let names: Vec<&str> = placed
        .library_cases
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["calm", "storm, two broken"]);
    assert!(placed.cases.iter().all(|c| c != "not for T2"));
    let storm = &placed.library_cases[1];
    assert_eq!(storm.strength, 0.9);
    let forces = &storm.line[0];
    assert_eq!(forces.result, "storm, two broken");
    assert_eq!(
        (forces.vertical, forces.transverse, forces.longitudinal),
        (1.1, 1.4, 1.3)
    );
    let own = storm.structure.as_ref().unwrap();
    assert_eq!(own.self_weight, Some(1.1));
    assert_eq!(own.wind.len(), 1);
    assert_eq!(own.wind[0].from, WindFrom::Angle(30.0));
    let t = own.wind[0].from.towards();
    let r = 30f64.to_radians();
    assert!((t[0] + r.cos()).abs() < 1e-12 && (t[1] + r.sin()).abs() < 1e-12);
    let w = weather.iter().find(|w| w.name == "high wind").unwrap();
    assert_eq!(own.wind[0].pressure, Some(w.wind_pressure.si()));
    // Its own structure wind (factor the transverse one) at an angle.
    let cw = placed.values.structure_wind("storm, two broken").unwrap();
    assert_eq!(cw.from_deg, Some(30.0));
    // Both wires broken: the values are the core's for that case.
    let direct = ohl_loads::structure_loads(&line, &library, &weather, &lc[1..2], &budget).unwrap();
    assert_eq!(direct[0].case.broken_wires().count(), 2);
    let core = ohl_structure_link::line_values(&direct, &line, "T2");
    let p = &placed.values.points[0];
    for q in [LineQuantity::Longitudinal, LineQuantity::Transverse] {
        assert_eq!(
            placed.values.get("storm, two broken", &p.label, &p.sets, q),
            core.get("storm, two broken", &p.label, &p.sets, q),
        );
    }
    // Through the file and back.
    let read = PlacedValues::parse(&placed.to_json()).unwrap();
    assert_eq!(read.library_cases, placed.library_cases);
    // T3 has no set: every case.
    let t3 = library_recipes(&loads.library, &weather, &line, "T3");
    assert_eq!(t3.len(), 3);
}

/// An angle read back: named directions stay names (old files read), an
/// angle is a number; mirrored left for right (θ → 180° − θ).
#[test]
fn wind_from_reads_names_and_angles() {
    assert_eq!(WindFrom::from_degrees(90.0), WindFrom::Ahead);
    assert_eq!(WindFrom::from_degrees(-45.0), WindFrom::BackRight);
    assert_eq!(WindFrom::from_degrees(30.0), WindFrom::Angle(30.0));
    assert_eq!(WindFrom::Angle(30.0).mirrored(), WindFrom::Angle(150.0));
    let a = ohl_structure_loads::StructureLoadCases::parse(
        "format = \"ohl-structure-load-cases\"\nversion = 3\n[[load_case]]\nname = \"w\"\n\
         [[load_case.structure.wind]]\nfrom = 30.0\npressure = \"500 Pa\"\n\
         [[load_case.structure.wind]]\nfrom = \"ahead_left\"\npressure = \"500 Pa\"\n",
    )
    .unwrap();
    let w = &a.cases[0].structure.as_ref().unwrap().wind;
    assert_eq!(w[0].from, WindFrom::Angle(30.0));
    assert_eq!(w[1].from, WindFrom::AheadLeft);
    let back = ohl_structure_loads::StructureLoadCases::parse(&a.to_toml()).unwrap();
    assert_eq!(back, a);
}

/// The structure's local ice in OHL-Structure (owner 2026-11-04: "yes
/// apply local ice too"): a library case taking the local conditions on
/// an INVENTED iced weather case (10 mm, 900 kg/m³) puts T2's local ice
/// (20 mm) on its members, said in the case's description; the same case
/// without the local conditions puts none.
#[test]
fn a_case_taking_the_local_conditions_puts_the_local_ice_on_the_members() {
    let (mut line, library, mut weather) = example();
    let mut iced = weather
        .iter()
        .find(|w| w.name == "everyday")
        .unwrap()
        .clone();
    iced.name = "INVENTED ice".into();
    iced.ice_thickness = ohl_core::units::Length::from_si(0.01);
    iced.ice_density = Some(ohl_core::units::Density::from_si(900.0));
    weather.push(iced);
    line.structures
        .iter_mut()
        .find(|s| s.id == "T2")
        .unwrap()
        .local
        .structure_ice = Some(0.02);
    let plain = ohl_loads::LoadCase::from_weather("INVENTED ice");
    let local = ohl_loads::LoadCase {
        name: "local".into(),
        use_local: true,
        ..plain.clone()
    };
    let budget = ohl_wirefe::default_fe_budget();
    let loads =
        ohl_loads::structure_loads(&line, &library, &weather, &[plain, local], &budget).unwrap();
    let recipes = library_recipes(&loads, &weather, &line, "T2");
    let ice = |k: usize| recipes[k].structure.as_ref().unwrap().ice.clone();
    assert!(ice(0).is_empty());
    let on = ice(1);
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].thickness, Some(0.02));
    assert_eq!(on[0].density, Some(900.0));
    assert!(
        recipes[1]
            .description
            .contains("the structure's local ice, 20 mm"),
        "{}",
        recipes[1].description
    );
}

/// Never silently absent (2026-11-14). A library case naming a weather
/// case that is not there fails only itself: the other cases still reach
/// OHL-Structure, and the note of every structure taking the faulty one
/// names it (NOT SENT). A structure whose own set names only cases no
/// longer in the library gets no library case and is told why (NOT
/// CHECKED); one naming one such case among others gets the others and
/// is told which is missing.
#[test]
fn one_faulty_case_fails_alone_and_a_set_of_missing_cases_is_said() {
    let (mut line, library, weather) = example();
    let lc = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 1\n\
         [[case]]\nname = \"calm\"\nweather = \"everyday\"\nvertical = 1.2\n\
         [[case]]\nname = \"faulty\"\nweather = \"no such weather\"\n\
         [[case]]\nname = \"windy\"\nweather = \"high wind\"\ntransverse = 1.4\n",
    )
    .unwrap()
    .cases;
    let set = |line: &mut Line, id: &str, cases: &[&str]| {
        line.structures
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .load_cases = cases.iter().map(|c| (*c).to_owned()).collect();
    };
    set(&mut line, "T2", &["calm (renamed)"]);
    set(&mut line, "T3", &["calm", "windy (renamed)"]);
    let cases = structure_cases_with(&ohl_line::StructureLoads::default(), &weather, &lc);
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    assert_eq!(loads.library_failed.len(), 1);
    assert_eq!(loads.library_failed[0].0, "faulty");
    let names: Vec<&str> = loads.library.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names, ["calm", "windy"]);
    // T1 takes every case: the two that compute, the faulty one named.
    let t1 = placed_values_with(&loads, &line, "T1", "synthetic");
    let got: Vec<&str> = t1.library_cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(got, ["calm", "windy"]);
    let note = t1.note.clone().unwrap_or_default();
    assert!(
        note.starts_with(ohl_structure_link::NOT_SENT)
            && note.contains("\"faulty\"")
            && note.contains("no such weather"),
        "{note}"
    );
    // T2: every case of its set gone from the library.
    let t2 = placed_values_with(&loads, &line, "T2", "synthetic");
    assert!(t2.library_cases.is_empty());
    let note = t2.note.clone().unwrap_or_default();
    assert!(
        note.contains("NOT CHECKED: load case(s) calm (renamed) of T2's set")
            && note.contains("no library case is sent for it"),
        "{note}"
    );
    // Not taking the faulty case: not told of it.
    assert!(!note.contains("faulty"), "{note}");
    // T3: "calm" arrives, "windy (renamed)" said.
    let t3 = placed_values_with(&loads, &line, "T3", "synthetic");
    let got: Vec<&str> = t3.library_cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(got, ["calm"]);
    let note = t3.note.clone().unwrap_or_default();
    assert!(note.contains("windy (renamed)"), "{note}");
    assert!(!note.contains("no library case is sent"), "{note}");
}

/// One name, one meaning (2026-11-14): a library with a load case
/// `everyday` on weather case `everyday`, S1 removed in it (a construction
/// state); with `everyday` also ticked in Loads for structures (the wires
/// after creep there), the line result is sent as `everyday (weather
/// case)`, said in the note — two cases of two names, each with its own
/// forces (before, both were `everyday` and OHL-Structure took the
/// library's for both: without S1's pull, not on the safe side).
#[test]
fn a_weather_case_sent_with_a_library_case_name_is_renamed() {
    let (line, library, weather) = example();
    let lc = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 4\n\
         [[case]]\nname = \"everyday\"\nweather = \"everyday\"\n\
         [[case.removed]]\nsection = \"S1\"\n",
    )
    .unwrap()
    .cases;
    let choice = ohl_line::StructureLoads {
        weather_cases: vec!["everyday".into()],
        condition: Some("creep".into()),
        ..Default::default()
    };
    let cases = structure_cases_with(&choice, &weather, &lc);
    let sent: Vec<&str> = cases.weather.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(sent, ["everyday (weather case)"]);
    assert_eq!(cases.weather_cases[0].name, "everyday (weather case)");
    assert_eq!(cases.renamed.len(), 1);
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    let placed = placed_values_with(&loads, &line, "T2", "synthetic");
    assert!(placed.cases.contains(&"everyday".to_owned()));
    assert!(placed.cases.contains(&"everyday (weather case)".to_owned()));
    let note = placed.note.clone().unwrap_or_default();
    assert!(
        note.contains(ohl_structure_link::RENAMED) && note.contains("\"everyday (weather case)\""),
        "{note}"
    );
    // Solved after creep although the creep case is sent renamed (the
    // line's own weather cases go along).
    assert!(!note.contains("not solved"), "{note}");
    // Each its own forces: S1 removed (the library's) or there, at T1 (a
    // line end of S1: its pull along the line).
    let t1 = placed_values_with(&loads, &line, "T1", "synthetic");
    let pull = |case: &str| -> f64 {
        t1.values
            .points
            .iter()
            .filter_map(|p| {
                t1.values
                    .get(case, &p.label, &p.sets, LineQuantity::Longitudinal)
            })
            .map(f64::abs)
            .sum()
    };
    let (removed, sent) = (pull("everyday"), pull("everyday (weather case)"));
    assert!(sent > 1000.0, "{sent}");
    assert!(
        (removed - sent).abs() > 1000.0,
        "the two cases have their own forces: {removed} N, {sent} N"
    );
}

/// Local conditions reach OHL-Structure (2026-11-14, owner's answer 3):
/// T2 with `wind_static = 1.3`; with the line check's **Use local
/// conditions** ticked, what T2 gets equals the line's check — the
/// library case's forces at its points are those of the core with every
/// case `use_local` (`ohl_loads::with_local`, as Checks › Structure loads
/// computes them), its wind on the body 1.3 × the case's pressure, once —
/// and the note says so. Unticked: the cases as written (1.0 ×).
#[test]
fn the_local_conditions_of_the_check_reach_the_placed_structure() {
    let (mut line, library, weather) = example();
    line.structures
        .iter_mut()
        .find(|s| s.id == "T2")
        .unwrap()
        .local
        .wind_static = Some(1.3);
    let lc = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 1\n\
         [[case]]\nname = \"storm\"\nweather = \"high wind\"\ntransverse = 1.4\n",
    )
    .unwrap()
    .cases;
    let w = weather.iter().find(|w| w.name == "high wind").unwrap();
    let budget = ohl_wirefe::default_fe_budget();
    let choice = ohl_line::StructureLoads::default();
    let sent = |local: bool| {
        let cases = structure_cases_with(&choice, &weather, &lc).with_local(local);
        let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
        placed_values_with(&loads, &line, "T2", "synthetic")
    };
    let (on, off) = (sent(true), sent(false));
    let wind = |p: &PlacedValues| {
        p.library_cases[0].structure.as_ref().unwrap().wind[0]
            .pressure
            .unwrap()
    };
    assert!((wind(&on) - 1.3 * w.wind_pressure.si()).abs() < 1e-9);
    assert!((wind(&off) - w.wind_pressure.si()).abs() < 1e-9);
    let note = on.note.clone().unwrap_or_default();
    assert!(note.starts_with(ohl_structure_link::WITH_LOCAL), "{note}");
    assert!(
        !off.note
            .clone()
            .unwrap_or_default()
            .contains(ohl_structure_link::WITH_LOCAL)
    );
    // The forces at the points: the line check's (every case use_local).
    let checked = ohl_loads::structure_loads(
        &line,
        &library,
        &weather,
        &ohl_loads::with_local(lc.clone(), true),
        &budget,
    )
    .unwrap();
    let core = ohl_structure_link::line_values(&checked, &line, "T2");
    let plain = ohl_structure_link::line_values(
        &ohl_loads::structure_loads(&line, &library, &weather, &lc, &budget).unwrap(),
        &line,
        "T2",
    );
    let mut differs = false;
    for p in &on.values.points {
        for q in [
            LineQuantity::Transverse,
            LineQuantity::Longitudinal,
            LineQuantity::Vertical,
        ] {
            let got = on.values.get("storm", &p.label, &p.sets, q);
            assert_eq!(
                got,
                core.get("storm", &p.label, &p.sets, q),
                "{} {q:?}",
                p.label
            );
            differs |= got != plain.get("storm", &p.label, &p.sets, q);
        }
    }
    assert!(differs, "the local conditions change T2's forces");
}
