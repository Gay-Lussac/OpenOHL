// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The project's height datum from the command line: set with `ohl
//! project apply` as the coordinate system is (a `put` of the line
//! settings row `meta` `line`), kept in the project, written to the line
//! file (version 7), and taken by the sheets' `{height_datum}` before the
//! coordinate system's vertical part and the template's text for an
//! empty field.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> PathBuf {
    root().join("examples").join(name)
}

fn ohl(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// The sheet's text of `line` drawn with the example template.
fn sheet(line: &Path, dir: &Path) -> String {
    let os = |s: &str| std::ffi::OsString::from(s);
    let out = ohl(&[
        os("drawing").as_os_str(),
        line.as_os_str(),
        os("--terrain").as_os_str(),
        example("synthetic-line/synthetic-survey.csv").as_os_str(),
        os("--weather").as_os_str(),
        example("synthetic-line/weather-cases.ohlwc").as_os_str(),
        os("--route").as_os_str(),
        os("main").as_os_str(),
        os("--format").as_os_str(),
        os("svg").as_os_str(),
        os("--sheet-template").as_os_str(),
        example("sheet-templates/example-a1.ohlsheet").as_os_str(),
        os("--field").as_os_str(),
        os("project_no=EX-9").as_os_str(),
        os("--out").as_os_str(),
        dir.as_os_str(),
    ]);
    ok(&out);
    std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap()
}

#[test]
fn the_height_datum_is_set_kept_written_and_on_the_sheets() {
    let dir = std::env::temp_dir().join(format!("ohl-height-datum-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("datum.ohl");
    let line = example("synthetic-line/synthetic-line.ohl.json");
    let os = |s: &str| std::ffi::OsString::from(s);
    ok(&ohl(&[
        os("project").as_os_str(),
        os("import").as_os_str(),
        line.as_os_str(),
        os("--into").as_os_str(),
        project.as_os_str(),
        os("--user").as_os_str(),
        os("ann").as_os_str(),
    ]));
    // Without one: the template's text for an empty field.
    let before = dir.join("before.ohl.json");
    ok(&ohl(&[
        os("project").as_os_str(),
        os("export").as_os_str(),
        project.as_os_str(),
        os("--out").as_os_str(),
        before.as_os_str(),
    ]));
    let text = std::fs::read_to_string(&before).unwrap();
    assert!(!text.contains("height_datum"));
    assert!(
        text.contains("\"version\": 7"),
        "written at the current version"
    );
    let svg = sheet(&before, &dir.join("sheets-before"));
    assert!(
        svg.contains("Heights above the survey's height datum (not given)"),
        "the template's [if_empty]"
    );
    // Set as the coordinate system is: the line settings row.
    let command = r#"{"op": "put", "kind": "meta", "id": "line", "value": {"name": "Synthetic example line (invented coordinates)", "crs": "local (invented)", "height_datum": {"name": "LN02", "epsg": 5728}}}"#;
    ok(&ohl(&[
        os("project").as_os_str(),
        os("apply").as_os_str(),
        project.as_os_str(),
        os("--user").as_os_str(),
        os("ann").as_os_str(),
        os(command).as_os_str(),
    ]));
    let after = dir.join("after.ohl.json");
    ok(&ohl(&[
        os("project").as_os_str(),
        os("export").as_os_str(),
        project.as_os_str(),
        os("--out").as_os_str(),
        after.as_os_str(),
    ]));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&after).unwrap()).unwrap();
    assert_eq!(v["version"], 7);
    assert_eq!(v["crs"], "local (invented)");
    assert_eq!(
        v["height_datum"],
        serde_json::json!({"name": "LN02", "epsg": 5728})
    );
    let svg = sheet(&after, &dir.join("sheets-after"));
    assert!(
        svg.contains("Heights above LN02 (EPSG:5728)."),
        "the datum first"
    );
    // A code that is no number: refused.
    let bad = r#"{"op": "put", "kind": "meta", "id": "line", "value": {"name": "x", "height_datum": {"name": "LN02", "epsg": "LN"}}}"#;
    let out = ohl(&[
        os("project").as_os_str(),
        os("apply").as_os_str(),
        project.as_os_str(),
        os("--user").as_os_str(),
        os("ann").as_os_str(),
        os(bad).as_os_str(),
    ]);
    assert!(!out.status.success());
    std::fs::remove_dir_all(&dir).ok();
}
