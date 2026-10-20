// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structure analyse|report` end to end: both programs built (`ohl`
//! of `ohl-cli`, `ohl-structure-report` of `ohl-structure-export`; the
//! first never depends on the second's crates), `ohl` finding the tool
//! next to it and asking it in JSON, on the example structure type-02
//! (its own cases: the model's `[[load]]` tables as case "model"). The
//! answers are those of the tool run on its own; without the tool, `ohl`
//! says where it looked and how to build it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The folder of the programs built with the tests (`target/<profile>`):
/// the test runs in `target/<profile>/deps`.
fn bin_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Builds both programs (nothing to do when they are up to date) and
/// gives their paths.
fn programs() -> (PathBuf, PathBuf) {
    let dir = bin_dir();
    let mut build = Command::new(env!("CARGO"));
    build.args([
        "build",
        "-q",
        "-p",
        "ohl-cli",
        "--bin",
        "ohl",
        "-p",
        "ohl-structure-export",
        "--bin",
        "ohl-structure-report",
    ]);
    if dir.file_name().is_some_and(|n| n == "release") {
        build.arg("--release");
    }
    if let Some(target) = dir.parent() {
        build.env("CARGO_TARGET_DIR", target);
    }
    let status = build.status().expect("cargo runs");
    assert!(status.success(), "building ohl and ohl-structure-report");
    let exe = |name: &str| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let (ohl, tool) = (exe("ohl"), exe("ohl-structure-report"));
    assert!(ohl.is_file() && tool.is_file(), "{}", dir.display());
    (ohl, tool)
}

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures/type-02.ohls")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-structure-cli-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ohl_asks_the_structure_side_for_a_structures_results() {
    let (ohl, tool) = programs();
    let file = example();
    // The JSON reply, through ohl.
    let out = Command::new(&ohl)
        .args(["structure", "analyse"])
        .arg(&file)
        .args(["--case", "model", "--json"])
        .env_remove("OHL_STRUCTURE_REPORT")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reply["format"], "ohl-structure-reply");
    assert_eq!(reply["version"], 1);
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["structure"], "type-02");
    assert_eq!(reply["cases"].as_array().unwrap().len(), 1);
    assert_eq!(reply["cases"][0]["name"], "model");
    assert_eq!(reply["checks"][0]["check"], "members");
    assert!(reply["files"].is_null());
    // The same request to the tool on its own: the same answer.
    let request = serde_json::json!({
        "format": "ohl-structure-request", "version": 1, "action": "analyse",
        "structure": file, "cases": ["model"],
    });
    let direct = {
        use std::io::Write as _;
        let mut child = Command::new(&tool)
            .args(["--request", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    assert!(direct.status.success());
    let direct: serde_json::Value = serde_json::from_slice(&direct.stdout).unwrap();
    assert_eq!(direct, reply);

    // In words, through ohl.
    let out = Command::new(&ohl)
        .args(["structure", "analyse"])
        .arg(&file)
        .output()
        .unwrap();
    let words = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{words}");
    assert!(words.starts_with("type-02 — as written"), "{words}");
    assert!(words.contains("members: "), "{words}");

    // A report: the files written where asked, named in the reply.
    let dir = scratch("report");
    let html = dir.join("type-02-report.html");
    let out = Command::new(&ohl)
        .args(["structure", "report"])
        .arg(&file)
        .args(["--case", "model", "-o"])
        .arg(&html)
        .output()
        .unwrap();
    let words = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{words}");
    let csv = dir.join("type-02-report.members.csv");
    assert!(words.contains(&format!("→ {}, {}", html.display(), csv.display())));
    let page = std::fs::read_to_string(&html).unwrap();
    assert!(page.contains("type-02"), "{page}");
    assert!(
        std::fs::read_to_string(&csv)
            .unwrap()
            .starts_with("case,member,")
    );

    // A case not in the table: the tool's error, said by ohl.
    let out = Command::new(&ohl)
        .args(["structure", "analyse"])
        .arg(&file)
        .args(["--case", "nothing"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no case \"nothing\""), "{err}");

    // OHL_STRUCTURE_REPORT naming no file: said.
    let out = Command::new(&ohl)
        .args(["structure", "analyse"])
        .arg(&file)
        .env("OHL_STRUCTURE_REPORT", dir.join("missing-tool"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("OHL_STRUCTURE_REPORT names") && err.contains("cargo build"),
        "{err}"
    );

    // ohl alone in a folder, nothing on the PATH: not found, where it
    // looked and how to build it.
    let alone = scratch("alone").join("bin");
    std::fs::create_dir_all(&alone).unwrap();
    let copy = alone.join(ohl.file_name().unwrap());
    std::fs::copy(&ohl, &copy).unwrap();
    let out = Command::new(&copy)
        .args(["structure", "analyse"])
        .arg(&file)
        .env_remove("OHL_STRUCTURE_REPORT")
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("was not found") && err.contains(&alone.display().to_string()),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(alone.parent().unwrap());
}
