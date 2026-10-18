// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A structure whose tension-only members do not settle (an invented
//! pair of diagonals in halves through a joint 1 mm off the line, the
//! joint held otherwise only by a weak spring), solved through the link
//! as the structure application does: it solves, its results carry the
//! note — kept active, approximate, to be treated later — and the
//! members kept active say so in what governs.

use std::path::Path;

use ols_structure_link::{CaseToRun, analyse_cases};
use ols_structure_loads::{At, LoadRow, LoadTable, Source};
use ols_structures::{LibraryItem, StructureType};

const TYPE: &str = r#"id = "K"
name = "kinked joint"
kind = "lattice"
[analysis]
nonlinear = false
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
tension = "100 kN"
compression = "100 kN"
[[node]]
id = "A"
point = [0, 0, 0]
support = "pinned"
[[node]]
id = "B"
point = [4, 0, 0]
support = "pinned"
[[node]]
id = "C"
point = [4, 0, 3]
support = "pinned"
[[node]]
id = "D"
point = [0, 0, 3]
support = "pinned"
[[node]]
id = "M"
point = [2, 0, 1.501]
spring = [1000, 1000, 1000, 0, 0, 0]
[[member]]
id = "AM"
nodes = ["A", "M"]
section = "s"
kind = "tension"
[[member]]
id = "MC"
nodes = ["C", "M"]
section = "s"
kind = "tension"
[[member]]
id = "BM"
nodes = ["B", "M"]
section = "s"
kind = "tension"
[[member]]
id = "MD"
nodes = ["D", "M"]
section = "s"
kind = "tension"
"#;

#[test]
fn an_unsettled_structure_solves_flagged_approximate() {
    let structure = StructureType::parse(Path::new("k.olss"), TYPE).unwrap();
    // 10 kN at M across the frame, typed in the table's case "pull".
    let table = LoadTable {
        rows: vec![LoadRow {
            case: "pull".into(),
            load: None,
            at: At::Joint,
            label: "M".into(),
            force: [10e3, 0.0, 0.0],
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
        }],
    };
    let cases = [CaseToRun {
        name: "pull".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    let results = analyse_cases(
        &structure,
        TYPE,
        None,
        &Default::default(),
        &table,
        &cases,
        &ols_structures_fe::default_budget(),
    )
    .unwrap();
    let r = results[0].1.as_ref().unwrap_or_else(|e| panic!("{e}"));
    // In balance: the spring's force at M (−k·u) is among the reactions.
    assert!(
        r.imbalance() < 1e-6 * 10e3,
        "{:?} {:?}",
        r.load_sum,
        r.reaction_sum
    );
    let at_m = r.reactions.iter().find(|x| x.joint == "M").unwrap();
    let u = r.joints.iter().find(|j| j.id == "M").unwrap().displacement;
    for k in 0..3 {
        assert!(
            (at_m.force[k] + 1000.0 * u[k]).abs() < 1e-6 * 10e3,
            "{at_m:?} {u:?}"
        );
    }
    assert!(
        r.notes.iter().any(|n| n
            == "tension-only members kept active because the set did not settle: AM, MC — \
                approximate; to be treated later"),
        "{:?}",
        r.notes
    );
    for m in &r.members {
        let kept = m.governs.contains(ols_structures_fe::check::KEPT_ACTIVE);
        assert_eq!(kept, m.id == "AM" || m.id == "MC", "{m:?}");
        if kept {
            assert!(!m.slack);
        }
    }
}
