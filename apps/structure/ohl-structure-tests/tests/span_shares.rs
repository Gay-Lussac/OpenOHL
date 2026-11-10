// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The shares of the back and ahead spans' pull from the line to the
//! solve (owner 2026-11-10: the line only transmits the pulls, the load
//! case is made on the structure), and a structure of the line that takes
//! no load case, through the link. The synthetic example line: every
//! value is INVENTED.

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::LoadCases;
use ohl_structure_link::{
    analyse_cases, cases_to_run, placed_values_with, structure_case_loads, structure_cases,
    structure_cases_with,
};
use ohl_structure_loads::{
    LineForces, LineQuantity, LoadCaseRecipe, PlacedValues, SPAN_SHARE_ASSUMPTION, evaluate,
};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};

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

fn everyday() -> ohl_line::StructureLoads {
    ohl_line::StructureLoads {
        weather_cases: vec!["everyday".into()],
        ..Default::default()
    }
}

fn case(name: &str, l: LineForces) -> LoadCaseRecipe {
    let mut c = LoadCaseRecipe::new(name);
    c.line.push(l);
    c
}

/// T2, a suspension structure on the straight part of the line, in still
/// air ("everyday"): the line sends each point's force and the horizontal
/// pull of its back and ahead spans; at a suspension point in still air
/// the two pulls are the longitudinal force (sent_L = back_L + ahead_L).
/// A case with back 5 %, ahead 0 %:
///
/// L = sent_L − 0.95·back_L − 1.0·ahead_L = 0.05·back_L,
///
/// 5 % of the back span's horizontal pull; the vertical force is the one
/// sent, bit for bit. The rows and the solved case say it is a load
/// assumption, not on the safe side; through the line-values file (JSON)
/// the table is the same.
#[test]
fn back_5_percent_on_a_suspension_structure_in_still_air() {
    let (line, library, weather) = example();
    let budget = ohl_wirefe::default_fe_budget();
    let cases = structure_cases(&everyday(), &weather);
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    let placed = placed_values_with(&loads, &line, "T2", "synthetic");
    assert_eq!(placed.version, 6);
    let values = &placed.values;
    let dir = ex().join("Structures");
    let text = std::fs::read_to_string(dir.join("type-02.ohls")).unwrap();
    let t = StructureType::parse(&dir.join("type-02.ohls"), &text).unwrap();
    let recipes = [
        case("as solved", LineForces::new("everyday")),
        case(
            "unbalanced",
            LineForces {
                back: Some(5.0),
                ahead: Some(0.0),
                ..LineForces::new("everyday")
            },
        ),
    ];
    let table = evaluate(&recipes, &t, values);
    assert!(table.missing().next().is_none(), "{table:#?}");
    assert!(!values.points.is_empty());
    for p in &values.points {
        let get = |q| values.get("everyday", &p.label, &p.sets, q).unwrap();
        let (back, ahead) = (
            get(LineQuantity::PullBackLongitudinal),
            get(LineQuantity::PullAheadLongitudinal),
        );
        // The back span pulls back, the ahead span ahead, kilonewtons.
        assert!(
            back < -1000.0 && ahead > 1000.0,
            "{}: {back} {ahead}",
            p.label
        );
        // Still air, a straight line: nothing across it.
        assert!(get(LineQuantity::PullBackTransverse).abs() < 1e-3 * back.abs());
        let sent = get(LineQuantity::Longitudinal);
        assert!((sent - (back + ahead)).abs() < 0.5, "{}: {sent}", p.label);
        let row = |c: &str| {
            table
                .rows
                .iter()
                .find(|r| r.case == c && r.label == p.label)
                .unwrap_or_else(|| panic!("no row at {}", p.label))
        };
        let (solved, share) = (row("as solved"), row("unbalanced"));
        assert!(
            (share.force[1] - 0.05 * back).abs() < 0.5,
            "{}: {} of {back}",
            p.label,
            share.force[1]
        );
        assert_eq!(share.force[2].to_bits(), solved.force[2].to_bits());
        assert!(
            share.note.contains("NOT ON THE SAFE SIDE"),
            "{}",
            share.note
        );
        assert!(!solved.note.contains("NOT ON THE SAFE SIDE"));
    }
    // Through the file.
    let read = PlacedValues::parse(&placed.to_json()).unwrap();
    assert_eq!(evaluate(&recipes, &t, &read.values), table);
    // The solve says it with the case; the case as solved does not.
    let sections = ohl_sections::SectionLibrary::default();
    let run = cases_to_run(&table, &recipes, None);
    let runs = analyse_cases(
        &t,
        &text,
        Some(&dir),
        &sections,
        &table,
        &run,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let said = |name: &str| {
        let r = runs.iter().find(|r| r.0 == name).expect("run");
        let r = r.1.as_ref().unwrap_or_else(|e| panic!("{name}: {e}"));
        r.notes
            .iter()
            .any(|n| n == &format!("NOT ON THE SAFE SIDE: {SPAN_SHARE_ASSUMPTION}"))
    };
    assert!(said("unbalanced"));
    assert!(!said("as solved"));
}

/// A structure that takes no load case (`no_load_cases`): none of the
/// line's library is sent for it and the file's note says so; the line
/// results (the forces the line transmits) are sent all the same; the
/// other structures keep theirs.
#[test]
fn a_structure_taking_no_load_case_gets_none_of_the_library() {
    let (mut line, library, weather) = example();
    let lc = LoadCases::load(&ex().join("load-cases.ohllc"))
        .unwrap()
        .cases;
    assert!(!lc.is_empty());
    line.structures
        .iter_mut()
        .find(|s| s.id == "T2")
        .unwrap()
        .no_load_cases = true;
    let budget = ohl_wirefe::default_fe_budget();
    let cases = structure_cases_with(&everyday(), &weather, &lc);
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    assert!(loads.library_error.is_none(), "{:?}", loads.library_error);
    // The core: no loads at T2 in any library case, said in each.
    for c in &loads.library {
        assert!(!c.loads.iter().any(|l| l.structure == "T2"));
        assert!(
            c.notes
                .iter()
                .any(|n| n.starts_with(ohl_loads::TAKES_NO_LOAD_CASE) && n.contains("T2")),
            "{:?}",
            c.notes
        );
    }
    let t2 = placed_values_with(&loads, &line, "T2", "synthetic");
    assert!(t2.library_cases.is_empty());
    let note = t2.note.as_deref().unwrap();
    assert!(note.contains("T2 takes no load case"), "{note}");
    assert_eq!(t2.cases, ["everyday"]);
    assert!(!t2.values.values.is_empty());
    let t3 = placed_values_with(&loads, &line, "T3", "synthetic");
    assert_eq!(t3.library_cases.len(), lc.len());
    assert!(t3.note.is_none(), "{:?}", t3.note);
}
