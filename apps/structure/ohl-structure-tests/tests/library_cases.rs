// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's load case library as a placed structure's load cases in
//! OHL-Structure (owner 2026-11-03: "The library's cases"; 2026-11-15:
//! those only): the cases of its groups, each case with its factors, its wind on the body from any
//! angle, its broken wires all broken, its ice and snow on the members.
//! The values are INVENTED (the synthetic example line).

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::LoadCases;
use ohl_structure_link::{
    library_recipes, placed_values_with, structure_case_loads, structure_cases,
};
use ohl_structure_loads::{LineQuantity, PlacedValues, WindFrom};
use ohl_structures::{LibraryItem, StructureLibrary};

/// Group `name` picking `ids`, taking `cases`.
fn pick(line: &mut Line, name: &str, ids: &[&str], cases: &[&str]) {
    let mut g = ohl_line::StructureGroup::new(name);
    g.structures = ids.iter().map(|s| (*s).to_owned()).collect();
    g.load_cases = cases.iter().map(|s| (*s).to_owned()).collect();
    line.structure_groups.push(g);
}

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
/// broken together) and its own factors; T2 takes it and "calm" (its
/// group), T3 every case. In OHL-Structure T2 gets exactly those, each
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
    pick(&mut line, "T2", &["T2"], &["calm", "storm, two broken"]);
    let cases = structure_cases(&weather, &lc);
    assert_eq!(cases.library.len(), 3);
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
    // T3 is in no group: every case.
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
/// names it (NOT SENT). A structure whose groups name only cases no
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
    pick(&mut line, "T2", &["T2"], &["calm (renamed)"]);
    pick(&mut line, "T3", &["T3"], &["calm", "windy (renamed)"]);
    let cases = structure_cases(&weather, &lc);
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
    // T2: every case of its groups gone from the library.
    let t2 = placed_values_with(&loads, &line, "T2", "synthetic");
    assert!(t2.library_cases.is_empty());
    let note = t2.note.clone().unwrap_or_default();
    assert!(
        note.contains("NOT CHECKED: load case(s) calm (renamed) of T2's groups")
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

/// Only the library cases reach OHL-Structure (owner, 2026-11-15): a
/// line still holding the weather cases once sent unfactored (and a
/// broken-wire case once sent) gets one note saying they are no longer
/// used, and T2 gets the cases of its groups only, every one a library
/// case — a broken-wire one too. An extra load case of type-02 taking the
/// line forces of the library case "storm" (factor 1 on each axis) gets
/// its **unfactored** forces, not the factored ones; one taking a weather
/// case's line result (an older recipe) gets no value, said.
#[test]
fn only_library_cases_are_sent_and_an_extra_case_takes_their_unfactored_forces() {
    let (mut line, library, weather) = example();
    line.structure_loads.weather_cases = vec!["everyday".into(), "high wind".into()];
    line.structure_loads.broken_cases = vec!["S1 broken".into()];
    assert!(
        line.unused_structure_loads_note()
            .is_some_and(|n| n.contains("no longer used"))
    );
    let lc = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 4\n\
         [[case]]\nname = \"storm\"\nweather = \"high wind\"\ntransverse = 1.4\nvertical = 1.2\n\
         [[case]]\nname = \"S1 broken\"\nweather = \"everyday\"\n\
         [[case.broken]]\nsection = \"S1\"\nspan = 2\n\
         [[case]]\nname = \"ice\"\nweather = \"glaze ice\"\n",
    )
    .unwrap()
    .cases;
    pick(&mut line, "T2", &["T2"], &["storm", "S1 broken"]);
    let budget = ohl_wirefe::default_fe_budget();
    let loads =
        structure_case_loads(&line, &library, structure_cases(&weather, &lc), &budget).unwrap();
    let placed = placed_values_with(&loads, &line, "T2", "synthetic");
    assert_eq!(placed.cases, ["storm", "S1 broken"]);
    assert!(
        placed
            .sources
            .iter()
            .all(|s| *s == ohl_structure_loads::CaseSource::LoadCase)
    );
    let names: Vec<&str> = placed
        .library_cases
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["storm", "S1 broken"]);
    let note = placed.note.clone().unwrap_or_default();
    assert!(note.contains("Groups: T2 is in T2"), "{note}");
    // An extra case on "storm", every factor 1: the unfactored forces.
    let dir = ex().join("Structures");
    let text = std::fs::read_to_string(dir.join("type-02.ohls")).unwrap()
        + "\n[[load_case]]\nname = \"mine\"\n[[load_case.line]]\nresult = \"storm\"\n\
           vertical = 1.0\ntransverse = 1.0\nlongitudinal = 1.0\n\
           [[load_case]]\nname = \"older\"\n[[load_case.line]]\n\
           result = \"high wind — wind from the left\"\n";
    let t = ohl_structures::StructureType::parse(&dir.join("type-02.ohls"), &text).unwrap();
    let recipes = ohl_structure_loads::StructureLoadCases::from_structure(&t).unwrap();
    let table = ohl_structure_loads::evaluate_with_areas(
        &recipes,
        &t,
        &placed.values,
        &ohl_structure_loads::NoAreas,
    );
    let p = &placed.values.points[0];
    let row = table
        .case("mine")
        .find(|r| r.label == p.label && r.missing.is_none())
        .expect("a row of line forces");
    let get = |q| placed.values.get("storm", &p.label, &p.sets, q).unwrap();
    assert!((row.force[0] - get(LineQuantity::Transverse)).abs() < 1e-9);
    assert!((row.force[2] - get(LineQuantity::Vertical)).abs() < 1e-9);
    let factored = get(LineQuantity::TransverseFactored);
    assert!(
        (factored - 1.4 * get(LineQuantity::Transverse)).abs() < 1e-6 * factored.abs().max(1.0)
    );
    assert!(
        (row.force[0] - factored).abs() > 1e-6,
        "unfactored, not factored"
    );
    // The older recipe on a weather case's line result: no value.
    assert!(table.case("older").all(|r| r.missing.is_some()));
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
    let sent = |local: bool| {
        let cases = structure_cases(&weather, &lc).with_local(local);
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
    assert!(note.contains(ohl_structure_link::GROUPS), "{note}");
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
