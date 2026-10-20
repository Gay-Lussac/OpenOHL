// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure whose tension-only members used to go round (an invented
//! pair of diagonals in halves through a joint 1 mm off the line, the
//! joint held otherwise only by a weak spring; a section of 0.1 cm⁴),
//! solved through the link as the structure application does: it
//! settles, the shortened halves bowed at their buckling load — marked
//! so in the member results, named in the notes and in what governs —
//! and nothing kept active.

use std::path::Path;

use ohl_structure_link::{CaseToRun, analyse_cases};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

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
i_min = "0.1 cm4"
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
fn the_formerly_unsettled_structure_settles_with_bowed_halves() {
    let structure = StructureType::parse(Path::new("k.ohls"), TYPE).unwrap();
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
        &ohl_structures_fe::default_budget(),
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
            == "tension-only members: 0 of 4 slack, 2 bowed (3 solve(s)); bowed (carry their \
                buckling load): MC 0.33 kN, BM 0.33 kN"),
        "{:?}",
        r.notes
    );
    assert!(!r.notes.iter().any(|n| n.contains("kept active")));
    for m in &r.members {
        let bowed = m.id == "MC" || m.id == "BM";
        assert_eq!(m.bowed, bowed, "{m:?}");
        assert!(!m.slack, "{m:?}");
        assert_eq!(
            m.governs.starts_with(ohl_structures_fe::check::BOWED),
            bowed,
            "{m:?}"
        );
        if bowed {
            assert!((m.axial + 331.7).abs() < 0.5, "{m:?}");
        } else {
            assert!(m.axial > 5e3, "{m:?}");
        }
    }
}
