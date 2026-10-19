// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl structures fe` on an invented model whose tension-only members
//! used to go round (a pair of diagonals in halves through a joint 1 mm
//! off the line, the joint held otherwise only by a weak spring): it
//! settles — without a second moment the shortened halves slack, with
//! one (0.1 cm⁴) bowed at their buckling load, said in the note, the
//! member list and the CSV — and nothing is kept active.

use std::process::Command;

const MODEL: &str = r#"format = "ohl-structure-model"
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

fn run(model: &str, name: &str) -> (String, String) {
    let dir = std::env::temp_dir().join(format!("ohl-fe-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kinked.ohlfe");
    std::fs::write(&file, model).unwrap();
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
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    let text = std::fs::read_to_string(&csv).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    (stdout, text)
}

fn row<'a>(csv: &'a str, id: &str) -> &'a str {
    csv.lines()
        .find(|l| l.contains(&format!(",{id},")))
        .unwrap_or_else(|| panic!("{id}: {csv}"))
}

#[test]
fn the_formerly_unsettled_model_settles_its_shortened_halves_slack() {
    let (stdout, csv) = run(MODEL, "slack");
    assert!(
        stdout.contains("tension-only members: 2 of 4 slack (6 solve(s)): MC, BM"),
        "{stdout}"
    );
    assert!(!stdout.contains("kept active"), "{stdout}");
    for id in ["MC", "BM"] {
        assert!(row(&csv, id).ends_with(",slack"), "{}", row(&csv, id));
    }
}

#[test]
fn with_a_second_moment_the_shortened_halves_are_bowed() {
    let model = MODEL.replace(
        "compression = \"100 kN\"\n",
        "compression = \"100 kN\"\ni_min = \"0.1 cm4\"\n",
    );
    let (stdout, csv) = run(&model, "bowed");
    assert!(
        stdout.contains(
            "tension-only members: 0 of 4 slack, 2 bowed (3 solve(s)); bowed (carry their \
             buckling load): MC 0.33 kN, BM 0.33 kN"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("kept active"), "{stdout}");
    for id in ["MC", "BM"] {
        let r = row(&csv, id);
        assert!(
            r.contains("bowed: carries its buckling load 0.33 kN — compression 0.3 kN of 100.0 kN"),
            "{r}"
        );
        assert!(r.contains(",-331."), "{r}");
    }
}
