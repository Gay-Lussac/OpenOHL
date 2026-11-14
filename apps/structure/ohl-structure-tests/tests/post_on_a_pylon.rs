// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A fixed post insulator on a lattice pylon (2026-11-14, the post plan's
//! phase 2c): the typed example's T2 holding its set 1 on an upright post
//! 1.5 m tall standing on its attachment point A1. The line's values at A1
//! are its loads summed — the post's top and its own weight and wind at its
//! middle — once per quantity (before, the second was sent and never
//! read); the post's moment is sent split by the force components; a
//! structure load case takes it with its factors, exactly; the
//! finite-element load puts it at A1's joint where a frame meets it, else
//! leaves it out and says NOT ON THE SAFE SIDE.

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::{CaseLoads, LoadCases};
use ohl_structure_link::{fe_loads, line_values};
use ohl_structure_loads::{LineQuantity, StructureLoadCases, evaluate};
use ohl_structures::{LibraryItem, StructureLibrary, StructureType};
use ohl_structures_fe::Model;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

fn type_02() -> StructureType {
    let path = ex().join("Structures/type-02.ohls");
    StructureType::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The typed example with T2's set 1 on an upright fixed post on A1.
fn with_a_post() -> (Line, Vec<CaseLoads>) {
    let types = StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let w = t2.wire_at_set(1).unwrap().label.clone();
    let base = t2.point("A1").unwrap().position;
    t2.strings.retain(|x| x.wire != w || x.top == "A1");
    let x = t2.strings.iter_mut().find(|x| x.wire == w).unwrap();
    x.post = true;
    x.strain = false;
    x.angle_deg = 180.0;
    x.length = 1.5;
    x.weight = 450.0;
    x.wind_area = 0.3;
    let wire = t2.wires.iter_mut().find(|v| v.label == w).unwrap();
    wire.position = [base[0], base[1], base[2] + 1.5];
    line.validate().unwrap();
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
    (line, loads)
}

const CASE: &str = r#"
[[load_case]]
name = "post"
description = "Invented factors."

[[load_case.line]]
result = "wind"
vertical = 1.35
transverse = 1.5
longitudinal = 1.1
"#;

#[test]
fn a_posts_moment_reaches_the_pylon() {
    let (line, loads) = with_a_post();
    let values = line_values(&loads, &line, "T2");
    let wind = loads.iter().find(|c| c.case.name == "wind").unwrap();
    let at_a1: Vec<&ohl_loads::AttachmentLoad> = wind
        .loads
        .iter()
        .filter(|l| l.structure == "T2" && l.point == "A1")
        .collect();
    // The top's load and the post's own at its middle.
    assert!(at_a1.len() >= 2, "{at_a1:?}");
    // One value per quantity at A1, the loads summed.
    let rows: Vec<_> = values
        .values
        .iter()
        .filter(|v| {
            v.case == "wind"
                && v.point.as_deref() == Some("A1")
                && v.set.is_none()
                && v.quantity == LineQuantity::Transverse
        })
        .collect();
    assert_eq!(rows.len(), 1);
    let sum: f64 = at_a1.iter().map(|l| l.local[0]).sum();
    assert!(
        (rows[0].value - sum).abs() < 1e-9,
        "{} {sum}",
        rows[0].value
    );
    // The moment, split by component, as the loads give it.
    let m = values
        .moments
        .iter()
        .find(|m| m.case == "wind" && m.point == "A1")
        .expect("the post's moment sent");
    let wind_moment: f64 = m.from_transverse.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!(wind_moment > 100.0, "{m:?}");
    // A structure load case takes it with its factors: exactly the
    // loads' factored moment about A1.
    let f: [f64; 3] = [1.5, 1.1, 1.35];
    let expected = at_a1.iter().fold([0.0; 3], |mut a, l| {
        let by = l.moments_by_component();
        let largest = f[0].max(f[1]).max(f[2]);
        for k in 0..3 {
            a[k] += f[0] * by[0][k] + f[1] * by[1][k] + f[2] * by[2][k] + largest * l.couple[k];
        }
        a
    });
    let structure = type_02();
    let recipes = StructureLoadCases::parse(CASE).unwrap().cases;
    let table = evaluate(&recipes, &structure, &values);
    let row = table
        .case("post")
        .find(|r| r.label == "A1")
        .expect("A1's line forces");
    for k in 0..3 {
        assert!(
            (row.moment[k] - expected[k]).abs() < 1e-6 * (1.0 + expected[k].abs()),
            "{:?} {expected:?}",
            row.moment
        );
    }
    // The table through CSV and back keeps it.
    let back = ohl_structure_loads::LoadTable::from_csv(&table.to_csv()).unwrap();
    assert_eq!(back, table);
    // Into the finite-element model: at A1's joint where a frame meets
    // it, else left out and said.
    let model = Model::load(&ex().join("Structures/type-02.ohls")).unwrap();
    let node = model.attachment_node("A1").unwrap();
    let loads = fe_loads(&model, &table, "post", 1.0).unwrap();
    let at = model
        .point_load_with(
            "A1",
            [row.force[0], row.force[1], -row.force[2]],
            row.moment,
        )
        .unwrap();
    let applied: Vec<_> = loads.nodal.iter().filter(|l| l.node == node).collect();
    if model.frame_at(node) {
        assert!(at.moment_applied && at.note.is_none());
        assert!(applied.iter().any(|l| l.moment != [0.0; 3]));
    } else {
        assert!(!at.moment_applied);
        let note = at.note.expect("said");
        assert!(
            note.starts_with("NOT ON THE SAFE SIDE") && note.contains("post insulator"),
            "{note}"
        );
        assert!(applied.iter().all(|l| l.moment == [0.0; 3]));
    }
}
