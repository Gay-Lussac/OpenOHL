// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl drawing --sheet-template`: the example template's sheets with the
//! fields, revisions, another version of the line in its role, the lines
//! below the lowest conductor; a required field left empty refuses the
//! export, naming it; `ohl sheet-templates` lists and writes them.

use std::path::{Path, PathBuf};
use std::process::Command;

fn examples(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn drawing(dir: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(examples("synthetic-line/synthetic-line.ohl.json"))
        .arg("--terrain")
        .arg(examples("synthetic-line/synthetic-survey.csv"))
        .arg("--weather")
        .arg(examples("synthetic-line/weather-cases.ohlwc"))
        .arg("--rules")
        .arg(examples("synthetic-line/clearance-rules.ohlr"))
        .arg("--route")
        .arg("main")
        .arg("--format")
        .arg("svg")
        .arg("--sheet-template")
        .arg(examples("sheet-templates/example-a1.ohlsheet"))
        .args(extra)
        .arg("--out")
        .arg(dir)
        .output()
        .unwrap()
}

#[test]
fn the_example_template_draws_the_sheets() {
    let dir = std::env::temp_dir().join(format!("ohl-sheet-template-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    // Without its required field: refused, named, nothing written.
    let out = drawing(&dir, &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("Project number (project_no)"), "{stderr}");
    let crossing = examples("synthetic-line/crossing-line.ohl.json");
    let version = format!("{}=other:{{name}} {{voltage}}", crossing.display());
    let out = drawing(
        &dir,
        &[
            "--field",
            "project_no=EX-9",
            "--field",
            "remarks@1=first sheet",
            "--revision",
            "A;2026-10-20;first issue;AB;CD",
            "--version",
            &version,
            "--role",
            "project",
            "--conductor-clearance",
            "--elevation-labels",
            "number,height",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("A1"), "{stdout}");
    let svg = std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap();
    for text in [
        "EX-9-main-001",
        "first sheet",
        "first issue",
        "Crossing line (invented) 110 kV",
        "below the lowest conductor",
        "corridor ±25 m",
    ] {
        assert!(svg.contains(text), "{text}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sheet_templates_are_listed_and_written() {
    let dir = std::env::temp_dir().join(format!("ohl-sheet-list-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("sheet-templates")
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("built-in") && stdout.contains("default"),
        "{stdout}"
    );
    let text = std::fs::read_to_string(dir.join("default.ohlsheet")).unwrap();
    assert!(text.contains("format = \"ohl-sheet-template\""));
    std::fs::remove_dir_all(&dir).ok();
}

/// `--profile-lines`, `--profile-distance`, `--profile-circuits`: the
/// profile's side lines at a distance from the axis and under the lowest
/// arm, named in the legend; an unknown circuit refused.
#[test]
fn the_profile_side_lines_are_chosen() {
    let dir = std::env::temp_dir().join(format!("ohl-profile-lines-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let svg = |extra: &[&str]| {
        let mut args = vec!["--field", "project_no=EX-9"];
        args.extend_from_slice(extra);
        let out = drawing(&dir, &args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("side lines"), "{stdout}");
        std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap()
    };
    // As the template says: the corridor's edges.
    let edges = svg(&[]);
    assert!(edges.contains("near side (right): edge and ground"));
    assert!(!edges.contains("of the axis") && !edges.contains("lowest arm"));
    let at = svg(&["--profile-distance", "10"]);
    assert!(at.contains("terrain 10 m right of the axis"));
    assert!(at.contains("terrain 10 m left of the axis"));
    assert!(at.contains("corridor ±25 m"), "the corridor's edges stay");
    let arm = svg(&[
        "--profile-lines",
        "lowest_arm",
        "--profile-circuits",
        "Example 1",
    ]);
    assert!(arm.contains("terrain under the outermost conductor of the lowest arm, right"));
    assert!(arm.contains("terrain under the outermost conductor of the lowest arm, left"));
    for (extra, message) in [
        (
            &[
                "--profile-lines",
                "lowest_arm",
                "--profile-circuits",
                "Nope",
            ][..],
            "no circuit \"Nope\" (circuits: Example 1, Branch)",
        ),
        (&["--profile-lines", "distance"][..], "give the distance"),
    ] {
        let mut args = vec!["--field", "project_no=EX-9"];
        args.extend_from_slice(extra);
        let out = drawing(&dir, &args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success());
        assert!(stderr.contains(message), "{stderr}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
