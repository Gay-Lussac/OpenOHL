// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Linked and unlinked attachment points on one 3D structure (owner
//! 2026-11-15): an invented frame column C (4 m, fixed at its foot N1) with
//! a point A1 linked to its top N2 at an offset of 0.5 m along x, and a
//! point "N2" — the same name as the joint — standing alone (unlinked) at
//! the joint's very place, a string hanging from each. Solved as the
//! structure application solves it: the load at A1 reaches the model
//! (its offset's moment at N2, unchanged), the load at the unlinked point
//! does not — left out and said not on the safe side, naming it. A lookup
//! of the point by its label never finds the joint of its name.

use std::path::Path;

use ohl_structure_link::{CaseToRun, analyse_cases};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

const TEXT: &str = r#"format = "ohl-structure"
version = 17
id = "U"
name = "linked and unlinked"
kind = "lattice"
[analysis]
nonlinear = false
self_weight = false
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "50 cm2"
iy = "2000 cm4"
iz = "2000 cm4"
torsion = "1000 cm4"
tension = "1000 kN"
compression = "1000 kN"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 4]
kind = "attach"
[[member]]
id = "C"
nodes = ["N1", "N2"]
section = "s"
kind = "frame"
[[point]]
label = "A1"
node = "N2"
offset = ["0.5 m", "0 m", "0 m"]
[[point]]
label = "N2"
point = [0, 0, 4]
unlinked = true
[[wire]]
label = "W1"
[[wire]]
label = "W2"
[[string]]
label = "S1"
top = "A1"
wire = "W1"
length = "2 m"
[[string]]
label = "S2"
top = "N2"
wire = "W2"
length = "2 m"
"#;

fn row(label: &str, down: f64) -> LoadRow {
    LoadRow {
        case: "c".into(),
        load: None,
        at: At::Point,
        label: label.into(),
        force: [0.0, 0.0, down],
        source: Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
        moment: [0.0; 3],
    }
}

#[test]
fn only_linked_points_load_the_3d_structure_and_the_rest_is_said() {
    let structure = StructureType::parse(Path::new("u.ohls"), TEXT).unwrap();
    // The point "N2" stands alone though joint N2 is at its very place
    // and has its name.
    assert_eq!(structure.point_link("N2"), None);
    assert_eq!(structure.point_link("A1"), Some(("N2", [0.5, 0.0, 0.0])));
    assert_eq!(structure.unlinked_points(), ["N2"]);
    let w = structure.unlinked_warning().unwrap();
    assert!(
        w.contains("N2") && w.contains("NOT ON THE SAFE SIDE"),
        "{w}"
    );
    let model = ohl_structures_fe::Model::from_structure_type(TEXT).unwrap();
    assert_eq!(model.unlinked_points, ["N2"]);
    // By its label, the point is not the joint of its name.
    assert_eq!(model.attachment_node("N2"), None);
    assert_eq!(
        model.attachment_node("A1"),
        model.nodes.iter().position(|n| n.id == "N2")
    );
    // 10 kN down at each point.
    let table = LoadTable {
        rows: vec![row("A1", 10e3), row("N2", 10e3)],
    };
    let cases = [CaseToRun {
        name: "c".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    let results = analyse_cases(
        &structure,
        TEXT,
        None,
        &Default::default(),
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let r = results[0].1.clone().unwrap_or_else(|e| panic!("{e}"));
    // Only A1's 10 kN reaches the model.
    assert!((r.load_sum[2] + 10e3).abs() < 1e-6, "{:?}", r.load_sum);
    assert!(
        (r.reaction_sum[2] - 10e3).abs() < 1e-3,
        "{:?}",
        r.reaction_sum
    );
    // Its offset's moment at N2: 0.5 m × 10 kN = 5 kN m, applied.
    let m = r.points.iter().find(|p| p.label == "A1").unwrap();
    assert!(m.applied);
    let size = m.moment.iter().map(|v| v * v).sum::<f64>().sqrt();
    assert!((size - 5e3).abs() < 1e-6, "{m:?}");
    // The unlinked point's load left out, said.
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("attachment point N2 linked to no joint")
                && n.contains("NOT ON THE SAFE SIDE")),
        "{:?}",
        r.notes
    );
}
