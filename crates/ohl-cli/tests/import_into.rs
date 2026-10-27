// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl project import-into`: the example line split at its junction
//! tower T3 — the project the main route with T3 as a stub, the import (a
//! bundle of the example project plugin) the branch with T3 in full and
//! its wire file. The dry run prints the tower in both with its sets and
//! writes nothing; the import adds the branch's set to the project's T3
//! (as chosen), its section and jumper, and its wire file (the project
//! has none); the wire library loads and `ohl line` solves every
//! section.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ohl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_branch_is_imported_into_its_main_line() {
    let plugin = root().join("plugins/examples/project-bundle");
    if !plugin
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "project-bundle plugin not built"
        );
        eprintln!("skipping: project-bundle plugin not built");
        return;
    }
    let dir = std::env::temp_dir().join(format!("ohl-import-into-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let full =
        ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap();
    // The project: the main route, T3 without the branch's set 2.
    let mut main = full.clone();
    main.routes.retain(|r| r.id == "main");
    main.alignments.retain(|a| a.route == "main");
    let points = main.alignments[0].points.clone();
    main.alignment_points.retain(|p| points.contains(&p.id));
    main.structures.retain(|s| s.id != "T6" && s.id != "T7");
    main.sections.retain(|s| s.id != "S3");
    main.jumpers.clear();
    let t3 = main.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    t3.wires.retain(|w| w.set != Some(2));
    t3.strings.retain(|s| s.wire != "W2");
    t3.points.retain(|p| p.label != "A2");
    let project = dir.join("main.ohl");
    let mut s = ohl_session::Session::open(&project).unwrap();
    s.import(&main, "tester").unwrap();
    drop(s);
    // The import: the branch, T3 in full, the wire file.
    let mut branch = full;
    branch.routes.retain(|r| r.id == "branch");
    branch.alignments.retain(|a| a.route == "branch");
    let points = branch.alignments[0].points.clone();
    branch.alignment_points.retain(|p| points.contains(&p.id));
    branch
        .structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    branch.sections.retain(|s| s.id == "S3");
    let wires =
        std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw")).unwrap();
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(&branch.to_json()).unwrap(),
        "libraries": [{"kind": "cables", "name": "cables.ohlw", "text": wires}],
    });
    std::fs::write(dir.join("branch.ohlb"), bundle.to_string()).unwrap();
    let plugin = plugin.display().to_string();
    let args = [
        "project",
        "import-into",
        "branch.ohlb",
        "--into",
        "main.ohl",
        "--user",
        "tester",
        "--plugin",
        &plugin,
    ];

    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let out = ohl(&dir, &dry);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("towers in both"), "{t}");
    assert!(
        t.contains("T3: 2 sets in the project's, 3 in the imported"),
        "{t}"
    );
    assert!(t.contains("[tower:T3:set:2]") && t.contains(": add"), "{t}");
    assert!(t.contains("dry run: nothing written"), "{t}");
    let unchanged = ohl_session::Session::open(&project).unwrap().export();
    assert_eq!(unchanged.sections.len(), 2);

    let mut go = args.to_vec();
    go.extend(["--choose", "tower:T3:set:2=add"]);
    let out = ohl(&dir, &go);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("library file written: cables/cables.ohlw"),
        "{t}"
    );
    assert!(t.contains("the wire library loads: 1 cables"), "{t}");
    let s = ohl_session::Session::open(&project).unwrap();
    let line = s.export();
    assert!(line.structure("T3").unwrap().wire_at_set(2).is_some());
    assert_eq!(line.sections.len(), 3);
    assert_eq!(line.jumpers.len(), 1);
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    drop(s);
    // Again: everything the same, no second wire file.
    let out = ohl(&dir, &go);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("library file written"), "{t}");
    assert!(t.contains("the wire library loads: 1 cables"), "{t}");
    let s = ohl_session::Session::open(&project).unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    assert_eq!(s.export().sections.len(), 3);
    drop(s);
    let out = ohl(&dir, &["line", "main.ohl"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("solved 3 of 3 sections"), "{t}");
    std::fs::remove_dir_all(&dir).ok();
}
