// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The synthetic example line solved by the core, one placed structure's
//! loads taken through `ohl-structure-link`, the recipes of its structure
//! file evaluated: the table against the line's loads by hand, and the
//! table's cases through the structure's finite-element model.

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::{CaseLoads, LoadCases};
use ohl_structure_link::{CaseToRun, analyse_cases, fe_loads, line_values};
use ohl_structure_loads::{
    At, LineQuantity, LoadRow, LoadTable, MODEL_CASE, Source, StructureLoadCases, evaluate,
    evaluate_with_areas,
};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};
use ohl_structures_fe::{Loads, Model};

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

fn type_02() -> StructureType {
    let path = ex().join("Structures/type-02.ohls");
    StructureType::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The typed example line's structure loads in its three load cases.
fn solved() -> (Line, Vec<CaseLoads>) {
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
    let loads = ohl_loads::structure_loads(
        &line,
        &library,
        &weather,
        &cases.cases,
        &ohl_wirefe::default_fe_budget(),
    )
    .unwrap();
    for c in &loads {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
    }
    (line, loads)
}

/// The line's load at a point of T2 in a case.
fn at_t2<'a>(loads: &'a [CaseLoads], case: &str, point: &str) -> &'a ohl_loads::AttachmentLoad {
    loads
        .iter()
        .find(|c| c.case.name == case)
        .and_then(|c| {
            c.loads
                .iter()
                .find(|l| l.structure == "T2" && l.point == point)
        })
        .unwrap_or_else(|| panic!("T2 {point} in {case}"))
}

fn row<'a>(t: &'a LoadTable, case: &str, load: usize, label: &str) -> &'a LoadRow {
    t.rows
        .iter()
        .find(|r| r.case == case && r.load == Some(load) && r.label == label)
        .unwrap_or_else(|| panic!("{case} {load} {label}"))
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= 1e-9 * (1.0 + b[k].abs()))
}

#[test]
fn a_placed_structure_gets_its_table_from_the_line() {
    let (line, loads) = solved();
    let values = line_values(&loads, &line, "T2");
    // The points of T2, with their sets and the sections' phases.
    let labels: Vec<&str> = values.points.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["A1", "A11"]);
    assert_eq!(values.points[0].sets, [1]);
    assert_eq!(values.points[1].sets, [11]);
    // Six values per point and case.
    assert_eq!(values.values.len(), 2 * 3 * 6);
    let v = values
        .get("ice", "A1", &[1], LineQuantity::Vertical)
        .unwrap();
    assert_eq!(v, at_t2(&loads, "ice", "A1").local[2]);
    assert!(v > 0.0, "the wires hang down on the structure: {v}");

    let structure = type_02();
    let recipes = StructureLoadCases::from_structure(&structure).unwrap();
    assert_eq!(recipes.len(), 4);
    let table = evaluate(&recipes, &structure, &values);

    // "wind": the line's factored loads, one component per load.
    for p in ["A1", "A11"] {
        let f = at_t2(&loads, "wind", p).factored;
        let sum: [f64; 3] = [1, 2, 3].iter().fold([0.0; 3], |mut s, &k| {
            let r = row(&table, "wind", k, p);
            assert_eq!(r.source, Source::Line);
            for (x, f) in s.iter_mut().zip(r.force) {
                *x += f;
            }
            s
        });
        assert!(close(sum, f), "{p}: {sum:?} {f:?}");
        assert!(close(
            table.totals()[&("wind".into(), At::Point, p.into())],
            f
        ));
    }

    // "ice, unbalanced": 1.3 × the unfactored vertical, and 2 kN ahead at
    // set 1 (A1) only.
    for p in ["A1", "A11"] {
        let v = at_t2(&loads, "ice", p).local[2];
        assert!(close(
            row(&table, "ice, unbalanced", 1, p).force,
            [0.0, 0.0, 1.3 * v]
        ));
    }
    let r = row(&table, "ice, unbalanced", 2, "A1");
    assert_eq!((r.force, r.source), ([0.0, 2000.0, 0.0], Source::Recipe));
    assert!(
        table
            .case("ice, unbalanced")
            .all(|r| r.load != Some(2) || r.label == "A1")
    );

    // "body wind": no areas yet, one row per member marked.
    let members = structure.members.len();
    let body: Vec<&LoadRow> = table.case("body wind").collect();
    assert_eq!(body.len(), members);
    assert!(body.iter().all(|r| r.missing.as_deref() == Some("no area")));
    // With an area per member (as the section library will give): 0.6 kPa
    // × 0.1 m² = 60 N each, along x.
    let areas = |_: &str| Some(0.1);
    let with = evaluate_with_areas(&recipes, &structure, &values, &areas);
    assert!(
        with.case("body wind")
            .all(|r| close(r.force, [60.0, 0.0, 0.0]))
    );

    // [[load]] as case "model": (4 kN, 0, −9 kN up) → 9 kN down.
    let m = row(&table, MODEL_CASE, 1, "A1");
    assert_eq!((m.at, m.force), (At::Joint, [4000.0, 0.0, 9000.0]));

    // The table through CSV and back.
    assert_eq!(LoadTable::from_csv(&table.to_csv()).unwrap(), table);
}

/// The table's "wind" case solved by the finite-element model gives what
/// the line run's check gives with the same factored loads.
#[test]
fn the_table_loads_the_model_as_the_line_run_does() {
    let (line, loads) = solved();
    let values = line_values(&loads, &line, "T2");
    let structure = type_02();
    let recipes = StructureLoadCases::from_structure(&structure).unwrap();
    let table = evaluate(&recipes, &structure, &values);
    let model = Model::load(&ex().join("Structures/type-02.ohls")).unwrap();
    let budget = ohl_structures_fe::default_budget();

    let wind = recipes.iter().find(|c| c.name == "wind").unwrap();
    let from_table = fe_loads(&model, &table, "wind", 1.0).unwrap();
    // As the line run builds them (ohl_loads' analysis).
    let mut direct = Loads {
        nodal: Vec::new(),
        self_weight: 1.0,
    };
    for p in ["A1", "A11"] {
        let f = at_t2(&loads, "wind", p).factored;
        direct.nodal.push(ohl_structures_fe::NodalLoad {
            node: model.attachment_node(p).unwrap(),
            force: [f[0], f[1], -f[2]],
            moment: [0.0; 3],
        });
    }
    let (_, a) = ohl_structures_fe::analyse(&model, &from_table, wind.strength, &budget).unwrap();
    let (_, b) = ohl_structures_fe::analyse(&model, &direct, wind.strength, &budget).unwrap();
    let worst = |u: &[ohl_structures_fe::MemberUsage]| {
        u.iter().filter_map(|m| m.usage).fold(0.0_f64, f64::max)
    };
    assert!(
        (worst(&a) - worst(&b)).abs() < 1e-9,
        "{} {}",
        worst(&a),
        worst(&b)
    );
    assert!(worst(&a) > 0.0 && worst(&a) < 1.0);

    // Case "model" gives the model's own [[load]]s.
    let model_case = fe_loads(&model, &table, MODEL_CASE, 1.0).unwrap();
    assert_eq!(model_case.nodal.len(), model.loads.len());
    for (x, y) in model_case.nodal.iter().zip(&model.loads) {
        assert_eq!((x.node, x.force), (y.node, y.force));
    }
    let own = Loads {
        nodal: model.loads.clone(),
        self_weight: 1.0,
    };
    let (sa, _) = ohl_structures_fe::analyse(&model, &model_case, 1.0, &budget).unwrap();
    let (sb, _) = ohl_structures_fe::analyse(&model, &own, 1.0, &budget).unwrap();
    assert_eq!(sa.displacements, sb.displacements);

    // A point no joint carries is said.
    let mut t = table.clone();
    t.rows[0].label = "nowhere".into();
    assert!(
        fe_loads(&model, &t, "wind", 1.0)
            .unwrap_err()
            .contains("nowhere")
    );
}

/// Every case of the table solved through the link: the loads in
/// equilibrium with the reactions, the "wind" case's usage as the line
/// run's, the rows without a value said.
#[test]
fn the_cases_of_the_table_are_solved_through_the_link() {
    let (line, loads) = solved();
    let values = line_values(&loads, &line, "T2");
    let path = ex().join("Structures/type-02.ohls");
    let text = std::fs::read_to_string(&path).unwrap();
    let structure = type_02();
    let recipes = StructureLoadCases::from_structure(&structure).unwrap();
    let table = evaluate(&recipes, &structure, &values);
    let mut cases: Vec<CaseToRun> = recipes
        .iter()
        .map(|c| CaseToRun {
            name: c.name.clone(),
            strength: c.strength,
            self_weight: c.self_weight_factor(),
        })
        .collect();
    cases.push(CaseToRun {
        name: MODEL_CASE.into(),
        strength: 1.0,
        self_weight: 1.0,
    });
    let budget = ohl_structures_fe::default_budget();
    let results = analyse_cases(
        &structure,
        &text,
        path.parent(),
        &Default::default(),
        &table,
        &cases,
        &budget,
    )
    .unwrap();
    assert_eq!(results.len(), 5);
    let model = Model::load(&path).unwrap();
    for (name, r) in &results {
        let r = r.as_ref().unwrap_or_else(|e| panic!("{name}: {e}"));
        // Σ loads + Σ reactions = 0 (the self-weight included).
        let scale = r.load_sum.iter().map(|x| x.abs()).fold(1.0, f64::max);
        assert!(r.imbalance() < 1e-6 * scale, "{name}: {r:?}");
        assert!(
            r.load_sum[2] < -model.weight() * 0.99,
            "{name}: {:?}",
            r.load_sum
        );
        assert_eq!(r.members.len(), model.members.len());
        assert_eq!(r.reactions.len(), 4);
        assert!(r.governing().is_some());
        assert!(r.largest_displacement().unwrap().1 > 0.0);
    }
    // "wind": the same worst usage as the line run's check.
    let wind = results[0].1.as_ref().unwrap();
    let mut direct = Loads {
        nodal: Vec::new(),
        self_weight: 1.0,
    };
    for p in ["A1", "A11"] {
        let f = at_t2(&loads, "wind", p).factored;
        direct
            .nodal
            .extend(model.point_loads(p, [f[0], f[1], -f[2]]).unwrap());
    }
    let (_, b) = ohl_structures_fe::analyse(&model, &direct, 1.0, &budget).unwrap();
    let worst = b.iter().filter_map(|m| m.usage).fold(0.0_f64, f64::max);
    assert!((wind.governing().unwrap().usage.unwrap() - worst).abs() < 1e-9);
    // "body wind" has no areas: its rows are said, only the weight acts.
    let body = results[2].1.as_ref().unwrap();
    assert!(
        body.notes[0].contains("without a value"),
        "{:?}",
        body.notes
    );
    assert!(body.load_sum[0].abs() < 1e-9);
}
