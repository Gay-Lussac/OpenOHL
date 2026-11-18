// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A tower with provisional bolts is level 3 on the command line too
//! (owner 2026-11-18, third round): `ohl structures fe` on an invented
//! level-4 structure asking for its joint details, whose joint layout
//! record lists a provisional bolt, leaves the joint details out and says
//! why; the same file with the bolt confirmed takes them.

use std::path::PathBuf;
use std::process::Command;

/// A column C and a member T, every end with its connection, the joint
/// details asked for, loaded at N3; `record` the joint layout's
/// provisional list.
fn file(name: &str, record: &str) -> PathBuf {
    let text = format!(
        r#"id = "P"
name = "laid-out tower (invented)"
kind = "lattice"
joint_details = true
[analysis]
nonlinear = false
self_weight = false
capacity = "simple"
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
iy = "200 cm4"
iz = "200 cm4"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 4]
[[node]]
id = "N3"
point = [3, 0, 4]
support = ["y", "z"]
[[member]]
id = "C"
nodes = ["N1", "N2"]
section = "s"
kind = "frame"
[[member]]
id = "T"
nodes = ["N2", "N3"]
section = "s"
[[connection]]
member = "T"
joint = "N2"
onto = "C"
bolts = {{ count = 2, diameter = "16 mm" }}
[[connection]]
member = "T"
joint = "N3"
[[connection]]
member = "C"
joint = "N1"
[[connection]]
member = "C"
joint = "N2"
[[load]]
node = "N3"
force = ["10 kN", "0 kN", "0 kN"]
[[generator]]
kind = "joint_layout"
id = "joint-layout"
provisional = [{record}]
"#
    );
    let d = std::env::temp_dir().join(format!("ohl-cli-provisional-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("p.ohls");
    std::fs::write(&p, text).unwrap();
    p
}

fn fe(path: &std::path::Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["structures".as_ref(), "fe".as_ref(), path.as_os_str()])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

#[test]
fn provisional_bolts_hold_the_structure_at_level_3() {
    let held = file("held", "[\"T\", \"N2\"]");
    let text = fe(&held);
    assert!(
        text.contains("joint details asked for (joint_details = true) but not taken: level 3, not 4: 1 member end(s) with provisional bolts"),
        "{text}"
    );
    assert!(!text.contains("joint details taken"), "{text}");
    let confirmed = file("confirmed", "");
    let text = fe(&confirmed);
    assert!(text.contains("joint details taken"), "{text}");
    assert!(!text.contains("provisional bolts"), "{text}");
    for p in [held, confirmed] {
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }
}
