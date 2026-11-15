// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A placed structure handed from the line to the structure application:
//! its line-values file (`ohl_structure_link::placed_values`) through JSON
//! and back gives the same table as the line's values taken directly; the
//! section library gives the members' areas for surface loads.

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::{CaseLoads, LoadCases};
use ohl_structure_link::{
    SectionAreas, SectionLibrary, line_values, placed_values, structure_case_loads, structure_cases,
};
use ohl_structure_loads::{
    CaseSource, LineQuantity, PlacedValues, StructureLoadCases, evaluate, evaluate_with_areas,
    line_values_path, overrides_path,
};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

/// The typed example line (resolved), its cables, weather cases and load
/// cases.
fn example() -> (
    Line,
    ohl_cables::Library,
    Vec<ohl_weather::WeatherCase>,
    LoadCases,
) {
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
    let cases = LoadCases::load(&ex().join("load-cases.ohllc")).unwrap();
    (line, library, weather, cases)
}

/// The typed example line's structure loads in its three load cases.
fn solved() -> (Line, Vec<CaseLoads>) {
    let (line, library, weather, cases) = example();
    let loads = ohl_loads::structure_loads(
        &line,
        &library,
        &weather,
        &cases.cases,
        &ohl_wirefe::default_fe_budget(),
    )
    .unwrap();
    (line, loads)
}

/// T2's line-values file, written and read back, evaluates the recipes of
/// its type to the same table as its values taken directly; it names the
/// line, the type, the cases and the other place of the type (T4).
#[test]
fn the_line_values_file_gives_the_same_table() {
    let (line, loads) = solved();
    let placed = placed_values(&loads, &line, "T2", "synthetic");
    assert_eq!(placed.structure, "T2");
    assert_eq!(placed.line, "synthetic");
    assert_eq!(placed.structure_type, "type-02");
    assert_eq!(placed.cases, ["everyday", "wind", "ice"]);
    assert_eq!(placed.sources, [CaseSource::LoadCase; 3]);
    assert_eq!(placed.note, None);
    let others: Vec<&str> = placed.others.iter().map(|o| o.structure.as_str()).collect();
    assert_eq!(others, ["T4"]);
    assert_eq!(placed.others[0].values, line_values(&loads, &line, "T4"));

    let dir = std::env::temp_dir().join(format!("ohl-placed-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("type-02.ohls");
    let at = line_values_path(&file, "T2");
    assert_eq!(at, dir.join("type-02.T2.line-values.json"));
    assert_eq!(
        overrides_path(&file, Some("T2")),
        dir.join("type-02.T2.loads.csv")
    );
    assert_eq!(overrides_path(&file, None), dir.join("type-02.loads.csv"));
    placed.save(&at).unwrap();
    let back = PlacedValues::load(&at).unwrap();
    assert_eq!(back, placed);
    std::fs::remove_dir_all(&dir).ok();

    let path = ex().join("Structures/type-02.ohls");
    let structure = StructureType::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap();
    let recipes = StructureLoadCases::from_structure(&structure).unwrap();
    let direct = evaluate(&recipes, &structure, &line_values(&loads, &line, "T2"));
    let through = evaluate(&recipes, &structure, &back.values);
    assert_eq!(through, direct);
    // As the integration test has it: the wind case takes the line's
    // factored loads at A1.
    let v = back
        .values
        .get("wind", "A1", &[1], LineQuantity::TransverseFactored)
        .unwrap();
    let r = through
        .case("wind")
        .find(|r| r.load == Some(1) && r.label == "A1")
        .unwrap();
    assert_eq!(r.force, [v, 0.0, 0.0]);
    // The governing values per point (for the other places).
    let g = back.values.governing();
    assert_eq!(g.len(), 2);
    assert!(g.iter().all(|g| g.largest.iter().all(Option::is_some)));
}

/// A mast of one vertical member, a tube of 100 mm from the section
/// library, 10 m long: 1 kPa across it gives 1 kPa × 0.1 m × 10 m = 1 kN
/// along the pressure's direction, and the row shows the area (1 m²). A
/// member whose section is not in the library says so.
#[test]
fn a_pressure_on_a_library_section_takes_width_times_length() {
    let text = r#"
format = "ohl-structure"
version = 2
id = "mast"
name = "test mast"
kind = "other"

[[node]]
id = "B"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T"
point = ["0 m", "0 m", "10 m"]

[[node]]
id = "A"
point = ["2 m", "0 m", "10 m"]

[[member]]
id = "mast"
nodes = ["B", "T"]
section = "CHS-100"
kind = "frame"

[[member]]
id = "arm"
nodes = ["T", "A"]
section = "own"
kind = "frame"

[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"

[[section]]
id = "own"
material = "steel"
area = "500 mm2"
iy = "100 cm4"
iz = "100 cm4"

[[load_case]]
name = "body wind"

[[load_case.load]]
pressure = "1 kPa"
direction = "x"
members = "all"

[[load_case.load]]
pressure = "1 kPa"
direction = "-y"
members = ["mast"]
"#;
    let sections = SectionLibrary::parse(
        r#"
format = "ohl-sections"
version = 1
name = "test"

[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"
yield = "355 MPa"

[[section]]
id = "CHS-100"
shape = "circular_hollow"
material = "steel"
d = "100 mm"
t = "5 mm"
"#,
    )
    .unwrap();
    let structure = StructureType::parse(Path::new("mast.ohls"), text).unwrap();
    let recipes = StructureLoadCases::from_structure(&structure).unwrap();
    let areas = SectionAreas::new(&structure, text, None, &sections);
    let table = evaluate_with_areas(&recipes, &structure, &Default::default(), &areas);
    let row = |load: usize, label: &str| {
        table
            .case("body wind")
            .find(|r| r.load == Some(load) && r.label == label)
            .unwrap()
            .clone()
    };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * (1.0 + b.abs());
    let m = row(1, "mast");
    assert_eq!(m.missing, None);
    assert!(close(m.area.unwrap(), 1.0), "{:?}", m.area);
    assert!(close(m.force[0], 1000.0) && m.force[1] == 0.0 && m.force[2] == 0.0);
    // Ahead or back, a tube shows the same width.
    let m = row(2, "mast");
    assert!(close(m.force[1], -1000.0), "{:?}", m.force);
    // The arm's section is the file's own: no area, saying why.
    let a = row(1, "arm");
    assert_eq!(a.area, None);
    assert!(
        a.missing
            .as_deref()
            .unwrap()
            .contains("not in the cross-section library"),
        "{:?}",
        a.missing
    );
    // Without the library: no area anywhere.
    let none = SectionAreas::new(&structure, text, None, &SectionLibrary::default());
    let table = evaluate_with_areas(&recipes, &structure, &Default::default(), &none);
    assert!(table.case("body wind").all(|r| {
        r.missing
            .as_deref()
            .is_some_and(|m| m.starts_with("no area"))
    }));
}

/// A section that does not solve (its cable not in the library): the
/// line-values file of a structure it hangs on carries, per case, the
/// section missing (through JSON and back) — the structure application
/// marks those cases incomplete — and that of a structure it does not
/// touch carries nothing; the other places of the type carry theirs.
#[test]
fn the_line_values_file_lists_the_wires_not_solved() {
    let (mut line, library, weather, cases) = example();
    for s in line.sections.iter_mut().filter(|s| s.id == "S3") {
        s.cable = "no-such-cable".into();
    }
    let loads = ohl_loads::structure_loads(
        &line,
        &library,
        &weather,
        &cases.cases,
        &ohl_wirefe::default_fe_budget(),
    )
    .unwrap();
    // T3: S3 starts there.
    let t3 = placed_values(&loads, &line, "T3", "synthetic");
    assert_eq!(t3.missing.len(), 3, "{:?}", t3.missing);
    for (m, case) in t3.missing.iter().zip(["everyday", "wind", "ice"]) {
        assert_eq!(
            (m.case.as_str(), m.kind.as_str(), m.id.as_str()),
            (case, "section", "S3")
        );
        assert!(!m.reason.is_empty());
        assert_eq!(
            t3.incomplete(case).as_deref(),
            Some("incomplete: S3 not solved")
        );
    }
    let back = PlacedValues::parse(&t3.to_json()).unwrap();
    assert_eq!(back, t3);
    // T2: only S1 and S2, solved.
    let t2 = placed_values(&loads, &line, "T2", "synthetic");
    assert!(t2.missing.is_empty(), "{:?}", t2.missing);
    assert!(t2.others.iter().all(|o| o.missing.is_empty()));
    assert_eq!(t2.incomplete("ice"), None);
}

/// A load case of the library with a broken wire (roadmap O9; since
/// 2026-11-15 the only way it reaches a structure: as a library case of
/// its groups): T2 gets it with its unfactored values — the core's
/// broken-wire loads with unit factors — and its factor on the
/// longitudinal ones; the structure next to the broken span takes the
/// unbalanced pull along the line, nearly balanced in the case without
/// the break.
#[test]
fn a_broken_wire_case_reaches_the_structure_as_a_library_case() {
    let (line, library, weather, _) = example();
    let lc = LoadCases::parse(
        "[[case]]\nname = \"S1 broken\"\nweather = \"everyday\"\nlongitudinal = 1.3\n\
         [case.broken]\nsection = \"S1\"\nspan = 2\n\
         [[case]]\nname = \"plain\"\nweather = \"everyday\"\n",
    )
    .unwrap()
    .cases;
    let cases = structure_cases(&weather, &lc);
    let budget = ohl_wirefe::default_fe_budget();
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    let placed = ohl_structure_link::placed_values_with(&loads, &line, "T2", "synthetic");
    assert_eq!(placed.cases, ["S1 broken", "plain"]);
    assert_eq!(placed.sources, [CaseSource::LoadCase; 2]);
    // Unfactored, as the core gives it with unit factors.
    let mut unit = lc[0].clone();
    unit.longitudinal = 1.0;
    let direct = ohl_loads::structure_loads(&line, &library, &weather, &[unit], &budget).unwrap();
    let s1 = placed
        .values
        .points
        .iter()
        .find(|p| {
            line.sections[0]
                .points
                .iter()
                .any(|x| x.structure == "T2" && p.sets.contains(&x.set))
        })
        .unwrap();
    let get = |case: &str, q| placed.values.get(case, &s1.label, &s1.sets, q).unwrap();
    let core = line_values(&direct, &line, "T2")
        .get("S1 broken", &s1.label, &s1.sets, LineQuantity::Longitudinal)
        .unwrap();
    assert!((get("S1 broken", LineQuantity::Longitudinal) - core).abs() < 1e-9);
    let f = get("S1 broken", LineQuantity::LongitudinalFactored);
    assert!(
        (f - 1.3 * core).abs() < 1e-6 * f.abs().max(1.0),
        "{f} {core}"
    );
    // The broken span pulls T2 along the line where S1 hangs; whole, it is
    // nearly balanced.
    let (broken, whole) = (
        get("S1 broken", LineQuantity::Longitudinal),
        get("plain", LineQuantity::Longitudinal),
    );
    assert!(
        broken.abs() > 10.0 * whole.abs().max(1.0),
        "{broken} {whole}"
    );
}
