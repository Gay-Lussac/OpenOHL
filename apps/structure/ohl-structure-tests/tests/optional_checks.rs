// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The optional checks of 2026-11-15 (owner) through the link, as the
//! structure application runs them: an invented column braced at mid-height
//! by two horizontal redundant members, its file asking for a redundant
//! force of 2.5 % and a climbing load of 1100 N — the redundants' usage
//! from the nominal force (what governs says so), the climbing check a
//! list of its own beside the case's members, with its note; without the
//! keys neither.

use std::path::Path;

use ohl_structure_link::{CaseToRun, analyse_cases};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

const TYPE: &str = r#"id = "C"
name = "braced column"
kind = "lattice"
version = 16
[analysis]
nonlinear = false
capacity = "simple"
ANALYSIS
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
[[section]]
id = "leg"
material = "steel"
area = "40 cm2"
i_min = "400 cm4"
[[section]]
id = "small"
material = "steel"
area = "1 cm2"
iy = "1 cm4"
iz = "1 cm4"
wy = "1 cm3"
wz = "1 cm3"
[[node]]
id = "A"
point = [0, 0, 0]
support = "pinned"
[[node]]
id = "M"
point = [0, 0, 2]
[[node]]
id = "B"
point = [0, 0, 4]
support = ["x", "y"]
[[node]]
id = "S1"
point = [2, 0, 2]
support = "pinned"
[[node]]
id = "S2"
point = [0, 2, 2]
support = "pinned"
[[member]]
id = "AM"
nodes = ["A", "M"]
section = "leg"
design = { class = "leg" }
[[member]]
id = "MB"
nodes = ["M", "B"]
section = "leg"
design = { class = "leg" }
[[member]]
id = "R1"
nodes = ["M", "S1"]
section = "small"
design = { class = "redundant" }
[[member]]
id = "R2"
nodes = ["M", "S2"]
section = "small"
design = { class = "redundant" }
"#;

fn run(analysis: &str) -> ohl_structure_link::CaseResult {
    let text = TYPE.replace("ANALYSIS", analysis);
    let structure = StructureType::parse(Path::new("c.ohls"), &text).unwrap();
    let table = LoadTable {
        rows: vec![LoadRow {
            case: "down".into(),
            load: None,
            at: At::Joint,
            label: "B".into(),
            force: [0.0, 0.0, 100e3],
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
            moment: [0.0; 3],
        }],
    };
    let cases = [CaseToRun {
        name: "down".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    let results = analyse_cases(
        &structure,
        &text,
        None,
        &Default::default(),
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    results[0]
        .1
        .as_ref()
        .unwrap_or_else(|e| panic!("{e}"))
        .clone()
}

#[test]
fn the_optional_checks_through_the_link() {
    let off = run("");
    assert!(off.climbing.is_empty());
    assert!(!off.notes.iter().any(|n| n.contains("climbing")));
    let r1 = off.members.iter().find(|m| m.id == "R1").unwrap();
    assert!(!r1.governs.contains("[redundant force:"), "{}", r1.governs);
    let on = run("redundant_force = 2.5\nclimbing_load = \"1100 N\"");
    let r1 = on.members.iter().find(|m| m.id == "R1").unwrap();
    let euler = std::f64::consts::PI.powi(2) * 210e9 * 1e-8 / 4.0;
    assert!(
        (r1.usage.unwrap() - 2.5e3 / euler).abs() < 1e-6,
        "{:?} {}",
        r1.usage,
        r1.governs
    );
    assert!(
        r1.governs
            .contains("the compression of AM at M (the member it braces): 2.50 kN — governs"),
        "{}",
        r1.governs
    );
    // The climbing list: the two flat redundants, the legs not.
    let ids: Vec<&str> = on.climbing.iter().map(|c| c.member.as_str()).collect();
    assert_eq!(ids, ["R1", "R2"]);
    assert!((on.climbing[0].usage.unwrap() - 550.0 / 355.0).abs() < 1e-9);
    assert!(
        on.notes
            .iter()
            .any(|n| n.starts_with("climbing load (alone, not a load case")),
        "{:?}",
        on.notes
    );
    // The legs' usage does not change with either check.
    let leg =
        |r: &ohl_structure_link::CaseResult| r.members.iter().find(|m| m.id == "AM").unwrap().usage;
    assert_eq!(leg(&off), leg(&on));
}
