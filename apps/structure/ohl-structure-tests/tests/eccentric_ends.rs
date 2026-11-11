// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Eccentric member ends (a connection's shift, stage 4.4 of the joint
//! details, 2026-11-02) solved through the link as the structure
//! application does, on an invented structure: a column C (4 m) and a
//! member T from its top N2 to N3 (3 m along x, held in y and z), T
//! shifted 20 mm at N2 onto C and pulled by 10 kN at N3. With C a frame
//! the moment P·e = 0.20 kN m goes to C (said in T's results and in the
//! notes); with C a truss (N2 held along x) only bars meet N2: left out,
//! said. The file asks for its joint details in the calculation
//! (`joint_details = true`) and is level 4, every member end with a
//! connection (owner 2026-11-11); without the option the shift is not
//! taken, said.

use std::path::Path;

use ohl_structure_link::{CaseResult, CaseToRun, analyse_cases};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

fn text(column: &str, n2: &str) -> String {
    text_with(column, n2, "joint_details = true")
}

fn text_with(column: &str, n2: &str, option: &str) -> String {
    format!(
        r#"id = "E"
name = "eccentric end"
kind = "lattice"
{option}
[analysis]
nonlinear = false
self_weight = false
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
iy = "200 cm4"
iz = "200 cm4"
tension = "100 kN"
compression = "100 kN"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 4]
{n2}
[[node]]
id = "N3"
point = [3, 0, 4]
support = ["y", "z"]
[[member]]
id = "C"
nodes = ["N1", "N2"]
section = "s"
kind = "{column}"
[[member]]
id = "T"
nodes = ["N2", "N3"]
section = "s"
[[connection]]
member = "T"
joint = "N2"
onto = "C"
shift = "20 mm"
[[connection]]
member = "T"
joint = "N3"
[[connection]]
member = "C"
joint = "N1"
[[connection]]
member = "C"
joint = "N2"
"#
    )
}

fn solved(text: &str) -> CaseResult {
    let structure = StructureType::parse(Path::new("e.ohls"), text).unwrap();
    let table = LoadTable {
        rows: vec![LoadRow {
            case: "pull".into(),
            load: None,
            at: At::Joint,
            label: "N3".into(),
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
        text,
        None,
        &Default::default(),
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    results[0].1.clone().unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn onto_a_frame_leg_the_moment_goes_to_the_leg_and_is_said() {
    let r = solved(&text("frame", ""));
    let t = r.members.iter().find(|m| m.id == "T").unwrap();
    let words = t.eccentric.as_deref().unwrap();
    assert_eq!(words, "N2 e 20 mm, 0.20 kN m to the joint's frame members");
    assert!(
        t.governs
            .contains(&format!("{} {words}]", ohl_structures_fe::check::SHIFTED)),
        "{}",
        t.governs
    );
    assert!(
        r.members
            .iter()
            .find(|m| m.id == "C")
            .unwrap()
            .eccentric
            .is_none()
    );
    // C's foot: P·(4 − e).
    let foot = r.reactions.iter().find(|x| x.joint == "N1").unwrap();
    assert!((foot.moment[1] + 10e3 * 3.98).abs() < 1e-3, "{foot:?}");
    assert!(
        r.notes.iter().any(|n| n
            .starts_with("eccentric member ends (connection shifts): 1 end(s)")
            && !n.contains("LEFT OUT")),
        "{:?}",
        r.notes
    );
}

#[test]
fn onto_a_bar_only_joint_the_moment_is_left_out_and_said() {
    let r = solved(&text("truss", "support = [\"x\", \"y\"]"));
    let t = r.members.iter().find(|m| m.id == "T").unwrap();
    assert!(
        t.eccentric.as_deref().unwrap().contains("LEFT OUT"),
        "{t:?}"
    );
    // The force through the work point: T carries P.
    assert!((t.axial - 10e3).abs() < 1e-6, "{t:?}");
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("LEFT OUT at 1 end(s)") && n.contains("T at N2 0.20 kN m")),
        "{:?}",
        r.notes
    );
}

/// Without the option (the default) the same file is solved in the simple
/// form: no end offset, nothing eccentric said for T — as the same file
/// with its connections removed.
#[test]
fn without_the_option_the_shift_is_not_taken() {
    let off = text_with("frame", "", "");
    let t = StructureType::parse(Path::new("e.ohls"), &off).unwrap();
    assert_eq!(t.detail(), ohl_structures::Detail::Joints);
    assert!(!t.joint_details_taken());
    let r = solved(&off);
    let bare = off.split("[[connection]]").next().unwrap().to_owned();
    assert_eq!(
        StructureType::parse(Path::new("e.ohls"), &bare)
            .unwrap()
            .detail(),
        ohl_structures::Detail::Design
    );
    let plain = solved(&bare);
    for (a, b) in r.members.iter().zip(&plain.members) {
        assert_eq!(a, b, "{}", a.id);
    }
    assert!(r.members.iter().all(|m| m.eccentric.is_none()));
    // With it, T's end is eccentric.
    let on = solved(&text("frame", ""));
    assert!(on.members.iter().any(|m| m.eccentric.is_some()));
}
