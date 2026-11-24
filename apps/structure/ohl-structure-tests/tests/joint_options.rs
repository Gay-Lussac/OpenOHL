// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joint options never lower a usage (stage 4.4 follow-up,
//! 2026-11-24), end to end through the link as OHL-Structure runs them, on
//! an invented truss: two bars in line, a–b bolted (two bolts of 16 mm in
//! holes of 17.5 mm at each end: δ = 2 × 1.5 = 3 mm of slip) and b–c
//! without bolts, 1 m each, EA = 210 GPa × 1000 mm² = 210 MN (k = EA/L =
//! 210 MN/m), P = 3kδ = 1890 kN along them at b.
//!
//! Without the slip each bar carries P/2 = 945 kN (a–b in tension, b–c in
//! compression), b moves P/2k = 4.5 mm. With it (u = (P + kδ)/2k = 2δ =
//! 6 mm): a–b carries k(u − δ) = kδ = 630 kN, b–c ku = 1260 kN. The
//! larger of the two: a–b from the solve without the slip (945 kN, said
//! "without the joint options"), b–c from the solve with it; b's
//! displacement 6 mm; the reactions per support the larger: 945 kN at a,
//! 1260 kN at c.

use std::path::Path;

use ohl_structure_link::{
    CaseResult, CaseToRun, JointOptions, RunInput, analyse_structure_with, joint_envelope,
};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

const TEXT: &str = r#"id = "J"
name = "joint options"
kind = "lattice"
[analysis]
nonlinear = false
self_weight = false
capacity = "simple"
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "200 MPa"
[[section]]
id = "s"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[section]]
id = "t"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[node]]
id = "a"
point = [0, 0, 0]
support = "pinned"
[[node]]
id = "b"
point = [1, 0, 0]
support = ["y", "z"]
[[node]]
id = "c"
point = [2, 0, 0]
support = "pinned"
[[member]]
id = "ab"
nodes = ["a", "b"]
section = "s"
kind = "truss"
design = { bolts = { count = 2, diameter = "16 mm", hole = "17.5 mm" } }
[[member]]
id = "bc"
nodes = ["b", "c"]
section = "t"
kind = "truss"
"#;

const P: f64 = 1890e3;

fn solve(options: JointOptions) -> CaseResult {
    let t = StructureType::parse(Path::new("j.ohls"), TEXT).unwrap();
    let table = LoadTable {
        rows: vec![LoadRow {
            case: "pull".into(),
            load: None,
            at: At::Joint,
            label: "b".into(),
            force: [P, 0.0, 0.0],
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
            moment: [0.0; 3],
        }],
    };
    let cases = [CaseToRun {
        name: "pull".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    let run = analyse_structure_with(
        &RunInput {
            structure: &t,
            text: TEXT,
            dir: None,
            sections: &ohl_sections::SectionLibrary::default(),
            table: &table,
            recipes: &[],
            cases: &cases,
            extension: None,
            bolts: None,
            feet: None,
        },
        &ohl_structures_fe::default_budget(),
        options,
    )
    .unwrap();
    run.cases[0].fe.clone().unwrap().unwrap()
}

/// Within the slip passes' tolerance (10⁻⁴ of the largest force).
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-3 * P, "{a} against {b}");
}

#[test]
fn the_slip_never_lowers_a_usage() {
    let off = solve(JointOptions::default());
    near(off.members[0].axial, P / 2.0);
    near(off.members[1].axial, -P / 2.0);
    assert!(!off.notes.iter().any(|n| n.starts_with("joint options:")));
    let on = solve(JointOptions {
        end_releases: false,
        bolt_slip: true,
    });
    let (ab, bc) = (&on.members[0], &on.members[1]);
    // a–b relieved by the slip: the solve without it governs, said.
    assert_eq!(ab.usage, off.members[0].usage, "{ab:?}");
    near(ab.axial, P / 2.0);
    assert!(
        ab.governs
            .ends_with(&format!("({})", joint_envelope::WITHOUT)),
        "{}",
        ab.governs
    );
    // b–c loaded more: the solve with it (2kδ = 1260 kN).
    near(bc.axial, -1260e3);
    assert!(bc.usage > off.members[1].usage);
    assert!(!bc.governs.contains(joint_envelope::WITHOUT));
    // b moves 2δ = 6 mm with the slip (4.5 mm without).
    let b = on.joints.iter().find(|j| j.id == "b").unwrap();
    assert!((b.displacement[0] - 6e-3).abs() < 1e-6, "{b:?}");
    // The reactions: the larger at each support.
    let at = |id: &str| on.reactions.iter().find(|r| r.joint == id).unwrap().force[0];
    near(at("a"), -P / 2.0);
    near(at("c"), -1260e3);
    let notes = on.notes.join("\n");
    assert!(
        notes.contains("1 member(s) governed by the solve with them, 1 by the solve without them"),
        "{notes}"
    );
    assert!(notes.contains("effect of the joint options"), "{notes}");
    // Every usage at least that without the option.
    for (w, o) in on.members.iter().zip(&off.members) {
        assert!(w.usage >= o.usage, "{w:?} against {o:?}");
    }
}
