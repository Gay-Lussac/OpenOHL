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
    assert!(loads.library_error.is_none(), "{:?}", loads.library_error);
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
