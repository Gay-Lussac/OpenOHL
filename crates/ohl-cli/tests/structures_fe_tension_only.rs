// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures fe` on an invented model whose tension-only members
//! used to go round (a pair of diagonals in halves through a joint 1 mm
//! off the line, the joint held otherwise only by a weak spring): it
//! settles — without a second moment the shortened halves slack, with
//! one (0.1 cm⁴) bowed at their buckling load, said in the note, the
//! member list and the CSV — and nothing is kept active. With
//! `--tension-compression none` (or the model saying it) the halves with
//! a second moment slack too, the law said; `buckling` overrides a model
//! saying none; another word refused.

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
    run_with(model, name, &[])
}

/// `ohl structures fe` on `model` with the options `extra`; its output
/// (`Err`: it failed, its error output) and its member CSV.
fn output(model: &str, name: &str, extra: &[&str]) -> Result<(String, String), String> {
    let dir = std::env::temp_dir().join(format!("ohl-fe-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kinked.ohlfe");
    std::fs::write(&file, model).unwrap();
    let csv = dir.join("members.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["structures", "fe"])
        .arg(&file)
        .args(["--weight-factor", "0", "--csv"])
        .arg(&csv)
        .args(extra)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let text = std::fs::read_to_string(&csv).unwrap_or_default();
    std::fs::remove_dir_all(&dir).ok();
    if out.status.success() {
        Ok((stdout, text))
    } else {
        Err(format!("{stdout}\n{stderr}"))
    }
}

fn run_with(model: &str, name: &str, extra: &[&str]) -> (String, String) {
    output(model, name, extra).unwrap_or_else(|e| panic!("{e}"))
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
        stdout.contains("tension-only members (compression up to their buckling load): 2 of 4 slack (6 solve(s)): MC, BM"),
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
            "tension-only members (compression up to their buckling load): 0 of 4 slack, 2 bowed (3 solve(s)); bowed (carry their \
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

/// The same model, its second moment given, with `--tension-compression
/// none` (owner 2026-11-15): the shortened halves slack — not bowed —,
/// said with the law in the note and in the CSV.
#[test]
fn without_compression_the_shortened_halves_are_slack_too() {
    let model = MODEL.replace(
        "compression = \"100 kN\"\n",
        "compression = \"100 kN\"\ni_min = \"0.1 cm4\"\n",
    );
    let (stdout, csv) = run_with(&model, "none", &["--tension-compression", "none"]);
    assert!(
        stdout.contains("tension-only members (no compression): 2 of 4 slack"),
        "{stdout}"
    );
    assert!(!stdout.contains("bowed"), "{stdout}");
    assert!(!stdout.contains("kept active"), "{stdout}");
    for id in ["MC", "BM"] {
        let r = row(&csv, id);
        assert!(r.ends_with(",slack (no compression)"), "{r}");
    }
    // The model file saying it (version 6): the same; the option
    // `buckling` overrides it, the halves bowed as by default.
    let file_none = model.replace(
        "version = 2\n[analysis]\n",
        "version = 6\n[analysis]\ntension_compression = \"none\"\n",
    );
    let (stdout, _) = run(&file_none, "file-none");
    assert!(
        stdout.contains("tension-only members (no compression): 2 of 4 slack"),
        "{stdout}"
    );
    let (stdout, _) = run_with(
        &file_none,
        "file-none-buckling",
        &["--tension-compression", "buckling"],
    );
    assert!(
        stdout.contains(
            "tension-only members (compression up to their buckling load): 0 of 4 slack, 2 bowed"
        ),
        "{stdout}"
    );
    // Another word: refused, naming the two.
    let e = output(&model, "zero", &["--tension-compression", "zero"]).unwrap_err();
    assert!(
        e.contains("--tension-compression: tension_compression \"zero\""),
        "{e}"
    );
}
