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

/// Two sections on one attachment point: at T2 the point A1 holds set 1
/// (section S, strung at 20 kN) and set 2 (section R, 12 kN), each by its
/// own string. INVENTED.
const TWO_SETS_LINE: &str = r#"{
  "format": "ohl-line", "version": 6, "name": "two sets on a point", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1},
               {"label": "W2", "position": [0, 0, 26], "set": 2}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7},
                 {"label": "S2", "top": "A1", "wire": "W2", "length": 4.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1},
               {"label": "W2", "position": [300, 0, 26], "set": 2}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1},
               {"label": "W2", "position": [600, 0, 26], "set": 2}]}
  ],
  "sections": [
   {"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}},
   {"id": "R", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 2}, {"structure": "T2", "set": 2},
               {"structure": "T3", "set": 2}],
    "stringing": {"kind": "tension", "horizontal_tension": 12000.0, "temperature": 15.0}}]
}"#;

/// The structure type of T2 above, for the load table.
const TWO_SETS_TYPE: &str = r#"
id = "two-sets"
name = "two sets on a point"
kind = "lattice"

[[point]]
label = "A1"
point = ["0 m", "0 m", "30 m"]

[[wire]]
label = "W1"
set = 1

[[wire]]
label = "W2"
set = 2

[[string]]
label = "S1"
top = "A1"
wire = "W1"
length = "3 m"

[[string]]
label = "S2"
top = "A1"
wire = "W2"
length = "4 m"
"#;

/// A share of the pull per wire set at a shared attachment point (roadmap
/// O22 item 7), from the line to the load table: the line sends, with the
/// point's pulls, each set's part of them (line values of version 7 —
/// only for a structure with such a point), the parts adding up to the
/// point's. Line forces of set 1 with back 0 % take off set 1's back pull
/// only:
///
/// L = sent_L − back_L(set 1),
///
/// set 2's pull staying as solved — and with both sets named, the
/// point's whole back pull, as before.
#[test]
fn a_shared_point_sends_each_sets_pull() {
    let (_, library, weather) = example();
    let mut line = Line::from_json(TWO_SETS_LINE).unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let budget = ohl_wirefe::default_fe_budget();
    let cases = structure_cases(&everyday(), &weather);
    let loads = structure_case_loads(&line, &library, cases, &budget).unwrap();
    let placed = placed_values_with(&loads, &line, "T2", "two sets");
    assert_eq!(placed.version, 7);
    // A structure whose points each hold one set: still version 6.
    assert_eq!(
        placed_values_with(&loads, &line, "T1", "two sets").version,
        6
    );
    let values = &placed.values;
    let point = |q| values.get("everyday", "A1", &[1, 2], q).unwrap();
    let part = |set, q| values.get_set("everyday", "A1", set, q).unwrap();
    for q in [
        LineQuantity::PullBackTransverse,
        LineQuantity::PullBackLongitudinal,
        LineQuantity::PullAheadTransverse,
        LineQuantity::PullAheadLongitudinal,
    ] {
        assert!(
            (part(1, q) + part(2, q) - point(q)).abs() < 1e-6,
            "{q:?}: {} + {} against {}",
            part(1, q),
            part(2, q),
            point(q)
        );
    }
    let back = LineQuantity::PullBackLongitudinal;
    // Set 1 pulls with about its 20 kN, set 2 with about its 12 kN.
    assert!(part(1, back) < -15_000.0, "{}", part(1, back));
    assert!(part(2, back) < -9000.0 && part(2, back) > -15_000.0);
    let t = StructureType::parse(Path::new("two-sets.ohls"), TWO_SETS_TYPE).unwrap();
    let forces = |sets: Vec<u32>| LineForces {
        sets,
        back: Some(0.0),
        ..LineForces::new("everyday")
    };
    let recipes = [
        case("set 1", forces(vec![1])),
        case("both", forces(vec![1, 2])),
    ];
    let table = evaluate(&recipes, &t, values);
    assert!(table.missing().next().is_none(), "{table:#?}");
    let row = |c: &str| {
        table
            .rows
            .iter()
            .find(|r| r.case == c && r.label == "A1")
            .unwrap_or_else(|| panic!("no row of {c}"))
    };
    let sent = point(LineQuantity::Longitudinal);
    assert!((row("set 1").force[1] - (sent - part(1, back))).abs() < 1e-6);
    assert!((row("both").force[1] - (sent - point(back))).abs() < 1e-6);
    // The two differ by set 2's back pull, kilonewtons.
    assert!((row("both").force[1] - row("set 1").force[1] + part(2, back)).abs() < 1e-6);
    assert!(
        row("set 1")
            .note
            .contains("only the pulls of set(s) 1 scaled"),
        "{}",
        row("set 1").note
    );
    // Through the file.
    let read = PlacedValues::parse(&placed.to_json()).unwrap();
    assert_eq!(read, placed);
    assert_eq!(evaluate(&recipes, &t, &read.values), table);
}
