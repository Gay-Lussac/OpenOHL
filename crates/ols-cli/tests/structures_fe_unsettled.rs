// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl structures fe` on an invented model whose tension-only members do
//! not settle (a pair of diagonals in halves through a joint 1 mm off the
//! line, the joint held otherwise only by a weak spring): it solves with
//! the flipping halves kept active, says so in a note — approximate, to
//! be treated later — and marks those members in the member list and the
//! CSV.

use std::process::Command;

const MODEL: &str = r#"format = "ols-structure-model"
version = 2
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
[[load]]
node = "M"
force = ["10 kN", "0 kN", "0 kN"]
"#;

#[test]
fn an_unsettled_model_solves_flagged_approximate() {
    let dir = std::env::temp_dir().join(format!("ols-fe-unsettled-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kinked.olsfe");
    std::fs::write(&file, MODEL).unwrap();
    let csv = dir.join("members.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["structures", "fe"])
        .arg(&file)
        .arg("--weight-factor")
        .arg("0")
        .arg("--csv")
        .arg(&csv)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains(
            "tension-only members kept active because the set did not settle: AM, MC — \
             approximate; to be treated later"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("kept active (did not settle)"), "{stdout}");
    let text = std::fs::read_to_string(&csv).unwrap();
    for id in ["AM", "MC"] {
        let row = text
            .lines()
            .find(|l| l.contains(&format!(",{id},")))
            .unwrap_or_else(|| panic!("{id}: {text}"));
        assert!(row.contains("kept active (did not settle)"), "{row}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
