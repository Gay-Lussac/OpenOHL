// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure load case of the two parts (2026-10-25) from the line to
//! the solve: the example line sends its weather cases (the high wind
//! twice, from the left and from the right) to T2; type-02 gets a case
//! of line forces (factors per axis, a point off) and loads on the
//! structure itself (self weight, the wind on the body at the pressure of
//! the line result, ice, a point load); evaluated with an invented
//! section library (profiles and wind factors for type-02's sections) the
//! table's wind is the line's own member wind, its ice ρ g (P t + π t²) L,
//! and the finite-element solve balances the table and the factored self
//! weight. Every value is invented.

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_structure_link::{
    SectionAreas, analyse_cases, cases_to_run, outline_perimeter, placed_values,
    structure_case_loads, structure_cases,
};
use ohl_structure_loads::{
    LineQuantity, LoadTable, Part, Source, StructureLoadCases, evaluate_with_areas,
    resolve_line_pressures,
};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

const LEFT: &str = "high wind — wind from the left";

/// Invented profiles for type-02's sections and invented wind factors.
const SECTIONS: &str = r#"
format = "ohl-sections"
version = 1
name = "invented"

[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"
yield = "355 MPa"

[[section]]
id = "leg"
shape = "equal_angle"
material = "steel"
h = "90 mm"
t = "9 mm"

[[section]]
id = "brace"
shape = "equal_angle"
material = "steel"
h = "60 mm"
t = "6 mm"

[[section]]
id = "arm"
shape = "equal_angle"
material = "steel"
h = "70 mm"
t = "7 mm"

[wind]
source = "invented"
drag = { equal_angle = 2.0 }
"#;

/// The case of both parts, added to type-02.
const CASE: &str = r#"
[[load_case]]
name = "ULS wind left"
description = """Invented: the high wind from the left.
Every factor invented."""

[[load_case.line]]
result = "high wind — wind from the left"
vertical = 1.35
transverse = 1.5
longitudinal = 1.0
off = ["A11"]

[load_case.structure]
self_weight = 1.35

[[load_case.structure.wind]]
from = "left"
pressure_of = "high wind — wind from the left"
factor = 1.5

[[load_case.structure.ice]]
thickness = "10 mm"
density = "900 kg/m³"

[[load_case.structure.point]]
value = "2 kN"
direction = "down"
points = ["A1"]
"#;

fn sum(t: &LoadTable, case: &str, source: Source) -> [f64; 3] {
    let mut s = [0.0; 3];
    for r in t
        .case(case)
        .filter(|r| r.source == source && r.missing.is_none())
    {
        for (a, f) in s.iter_mut().zip(r.force) {
            *a += f;
        }
    }
    s
}

#[test]
fn a_case_of_two_parts_goes_from_the_line_to_the_solve() {
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
    let choice = ohl_line::StructureLoads {
        load_cases: None,
        weather_cases: vec!["high wind".into()],
        condition: None,
    };
    let budget = ohl_wirefe::default_fe_budget();
    let loads =
        structure_case_loads(&line, &library, structure_cases(&choice, &weather), &budget).unwrap();
    let placed = placed_values(&[], &loads.weather, &[], &line, "T2", "synthetic");
    assert!(placed.cases.iter().any(|c| c == LEFT), "{:?}", placed.cases);
    let values = placed.values;

    // type-02 with the case, read with the invented library.
    let dir = ex().join("Structures");
    let text = std::fs::read_to_string(dir.join("type-02.ohls")).unwrap() + CASE;
    let t = StructureType::parse(&dir.join("type-02.ohls"), &text).unwrap();
    let recipes = StructureLoadCases::from_structure(&t).unwrap();
    let case = recipes.iter().find(|c| c.name == "ULS wind left").unwrap();
    let sections = ohl_sections::SectionLibrary::parse(SECTIONS).unwrap();
    let areas = SectionAreas::new(&t, &text, Some(&dir), &sections);
    let table = evaluate_with_areas(std::slice::from_ref(case), &t, &values, &areas);
    let name = "ULS wind left";

    // 1. Line forces: each axis × its factor, A11 off.
    let p = values.points.iter().find(|p| p.label == "A1").unwrap();
    let at = |q| values.get(LEFT, "A1", &p.sets, q).unwrap();
    let a1 = table
        .case(name)
        .find(|r| r.load == Some(Part::Line.number(0)) && r.label == "A1")
        .unwrap();
    let expected = [
        1.5 * at(LineQuantity::Transverse),
        at(LineQuantity::Longitudinal),
        1.35 * at(LineQuantity::Vertical),
    ];
    for (f, e) in a1.force.iter().zip(expected) {
        assert!((f - e).abs() < 1e-9, "{a1:?}");
    }
    let a11 = table
        .case(name)
        .find(|r| r.load == Some(Part::Line.number(0)) && r.label == "A11")
        .unwrap();
    assert_eq!(a11.force, [0.0; 3]);
    assert!(a11.note.starts_with("off"));

    // 2. The wind on the body: the line's member wind (the library's
    // factors) at the pressure of the line result × 1.5, every member
    // 20 mm wider for the ice.
    let model = ohl_structures_fe::Model::from_structure_type_with(&text, &sections).unwrap();
    let pressure = values.structure_wind(LEFT).unwrap().pressure;
    assert!(pressure > 0.0);
    let setup = ohl_structures_fe::wind::MemberWindSetup::new(&model, sections.wind.as_ref());
    let iced = setup.wind_iced(
        pressure * 1.5,
        [1.0, 0.0, 0.0],
        &vec![0.02; model.members.len()],
    );
    let wind = sum(&table, name, Source::Wind);
    let want = iced.total();
    for k in 0..3 {
        assert!(
            (wind[k] - [want[0], want[1], -want[2]][k]).abs() < 1e-6 * want[0].abs(),
            "{wind:?} {want:?}"
        );
    }
    // Without the ice: the line's own member wind of the structure checks.
    let mut bare = case.clone();
    bare.structure.as_mut().unwrap().ice.clear();
    let plain = evaluate_with_areas(std::slice::from_ref(&bare), &t, &values, &areas);
    let checks = model.member_wind_with(pressure * 1.5, [1.0, 0.0, 0.0], sections.wind.as_ref());
    let w = sum(&plain, name, Source::Wind);
    assert!(
        (w[0] - checks.total()[0]).abs() < 1e-6 * w[0],
        "{w:?} {:?}",
        checks.total()
    );
    assert!(w[0] < wind[0]);

    // Ice: ρ g (P t + π t²) × L per member, down.
    let ice = sum(&table, name, Source::Ice);
    let by_hand: f64 = model
        .members
        .iter()
        .map(|m| {
            let p = outline_perimeter(model.sections[m.section].profile.as_ref().unwrap());
            900.0
                * ohl_core::units::STANDARD_GRAVITY
                * (p * 0.01 + std::f64::consts::PI * 1e-4)
                * model.length(m)
        })
        .sum();
    assert!(
        (ice[2] - by_hand).abs() < 1e-9 * by_hand,
        "{ice:?} {by_hand}"
    );
    assert!(table.case(name).all(|r| r.missing.is_none()), "{table:#?}");

    // The solve: the table's loads and the self weight × 1.35 balance the
    // reactions.
    let run_recipes = resolve_line_pressures(std::slice::from_ref(case), &values);
    let cases = cases_to_run(&table, &run_recipes, Some(name));
    assert_eq!(cases[0].self_weight, 1.35);
    let runs = analyse_cases(
        &t,
        &text,
        Some(&dir),
        &sections,
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let r = runs[0].1.as_ref().unwrap();
    let mut rows = [0.0; 3];
    for row in table.case(name).filter(|r| r.missing.is_none()) {
        for (a, f) in rows.iter_mut().zip(row.force) {
            *a += f;
        }
    }
    assert!((r.load_sum[0] - rows[0]).abs() < 1e-6 * rows[0].abs());
    let weight = -model.weight() * 1.35;
    assert!((r.load_sum[2] - (weight - rows[2])).abs() < 1e-6 * r.load_sum[2].abs());
    assert!(r.imbalance() < 1e-6 * r.load_sum[2].abs(), "{r:?}");
}
