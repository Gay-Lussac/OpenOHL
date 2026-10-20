// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wind on the structures set apart from the wind on the wires in a
//! load case (`[case.structure]` of the `.ohllc`): one rule for the line's
//! structure checks and the structure application. The example line's
//! wind case (high wind 600 Pa, transverse 1.4) given 900 Pa × 1.2 for
//! the structures (invented values): the line values carry them, and the
//! pole of T6 checked in the structure application with the line's forces
//! and `structure_wind_factored` is the line's own pole check of T6 —
//! the same loads at its point and the same wind, 900 × 1.2 = 1 080 Pa to
//! −x (the wind comes from the right of the line).

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::{LoadCases, StructureWind};
use ohl_structure_link::{RunInput, analyse_structure, cases_to_run, line_values};
use ohl_structure_loads::{Direction, StructureLoadCases, evaluate, resolve_line_pressures};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

#[test]
fn the_structure_application_takes_the_line_cases_wind_on_the_body() {
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
    let mut cases = LoadCases::load(&ex().join("load-cases.ohllc"))
        .unwrap()
        .cases;
    assert_eq!(cases[1].name, "wind");
    cases[1].structure = Some(StructureWind {
        wind_pressure: Some(900.0),
        wind_factor: Some(1.2),
    });
    let loads = ohl_loads::structure_loads(
        &line,
        &library,
        &weather,
        &cases,
        &ohl_wirefe::default_fe_budget(),
    )
    .unwrap();
    // The line values carry the wind on the body per case.
    let values = line_values(&loads, &line, "T6");
    let w = values.structure_wind("wind").unwrap();
    assert_eq!(
        (w.pressure, w.factor, w.towards),
        (900.0, 1.2, Direction::Left)
    );
    assert!(w.origin.contains("own pressure and factor"), "{}", w.origin);
    let still = values.structure_wind("everyday").unwrap();
    assert_eq!((still.pressure, still.factor), (0.0, 1.0));
    assert!(still.origin.is_empty());

    // The line's own pole check of T6 in the wind case.
    let usage = ohl_loads::capacity::usage(&line, &types, &library, &weather, &loads);
    let line_pole = usage[1]
        .structures
        .iter()
        .find(|s| s.structure == "T6")
        .and_then(|s| s.pole.clone())
        .unwrap();

    // The structure application: the line's factored forces at every
    // point and its wind on the body.
    let dir = ex().join("Structures");
    let text = std::fs::read_to_string(dir.join("type-05.ohls"))
        .unwrap()
        .replace("version = 2", "version = 6")
        + "\n[[load_case]]\nname = \"wind\"\nline_case = \"wind\"\n\
           [[load_case.load]]\nwhat = \"transverse_factored\"\npoints = \"all\"\n\
           [[load_case.load]]\nwhat = \"longitudinal_factored\"\npoints = \"all\"\n\
           [[load_case.load]]\nwhat = \"vertical_factored\"\npoints = \"all\"\n\
           [[load_case.load]]\nwhat = \"structure_wind_factored\"\nmembers = \"all\"\n";
    let t = StructureType::parse(&dir.join("type-05.ohls"), &text).unwrap();
    let recipes = StructureLoadCases::from_structure(&t).unwrap();
    let table = evaluate(&recipes, &t, &values);
    let resolved = resolve_line_pressures(&recipes, &values);
    let run_cases = cases_to_run(&table, &resolved, None);
    let run = analyse_structure(
        &RunInput {
            structure: &t,
            text: &text,
            dir: Some(&dir),
            sections: &ohl_sections::SectionLibrary::default(),
            table: &table,
            recipes: &resolved,
            cases: &run_cases,
            extension: None,
        },
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let p = run.cases[0].pole.as_ref().unwrap().as_ref().unwrap();
    assert!(
        (p.wind[0] + 1080.0).abs() < 1e-9 && p.wind[1] == 0.0,
        "{:?}",
        p.wind
    );
    assert!(
        (p.usage - line_pole.usage).abs() < 1e-12,
        "{} {}",
        p.usage,
        line_pole.usage
    );
}
