// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `--library FILE` on `ohl project import-from` and `import-into`: an
//! export cut off before its cable data (a bundle of the example project
//! plugin with the line only) gets the wire file added. The new project
//! (the example's main route) solves every section; the branch brought
//! into it with the same wire file added keeps one wire file (cables by
//! id) and every section solves. A file of no library kind is refused.

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

/// A bundle holding the line only (no library files).
fn bundle(line: &ohl_line::Line, file: &Path) {
    let b = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(&line.to_json()).unwrap(),
        "libraries": [],
    });
    std::fs::write(file, b.to_string()).unwrap();
}

#[test]
fn a_cut_off_export_gets_its_wire_file_added() {
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
    let dir = std::env::temp_dir().join(format!("ohl-import-libraries-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let full =
        ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap();
    // The main route, T3 without the branch's set 2.
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
    bundle(&main, &dir.join("main.ohlb"));
    // The branch, T3 in full.
    let mut branch = full;
    branch.routes.retain(|r| r.id == "branch");
    branch.alignments.retain(|a| a.route == "branch");
    let points = branch.alignments[0].points.clone();
    branch.alignment_points.retain(|p| points.contains(&p.id));
    branch
        .structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    branch.sections.retain(|s| s.id == "S3");
    bundle(&branch, &dir.join("branch.ohlb"));
    std::fs::copy(
        root().join("libraries/cables/acsr-examples.ohlw"),
        dir.join("wires.ohlw"),
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not a library").unwrap();
    let plugin = plugin.display().to_string();

    // A file of no library kind: refused, nothing made.
    let out = ohl(
        &dir,
        &[
            "project",
            "import-from",
            "main.ohlb",
            "--into",
            "bad.ohl",
            "--user",
            "tester",
            "--plugin",
            &plugin,
            "--library",
            "notes.txt",
        ],
    );
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("not a library file"), "{t}");
    assert!(!dir.join("bad.ohl").exists());

    let out = ohl(
        &dir,
        &[
            "project",
            "import-from",
            "main.ohlb",
            "--into",
            "main.ohl",
            "--user",
            "tester",
            "--plugin",
            &plugin,
            "--library",
            "wires.ohlw",
            "--clip-in",
        ],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("library file wires.ohlw (cables) added"), "{t}");
    assert!(t.contains("sections clipped in"), "{t}");
    assert!(!t.contains("not clipped in"), "{t}");
    let out = ohl(&dir, &["line", "main.ohl"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("solved 2 of 2 sections"), "{t}");

    let out = ohl(
        &dir,
        &[
            "project",
            "import-into",
            "branch.ohlb",
            "--into",
            "main.ohl",
            "--user",
            "tester",
            "--plugin",
            &plugin,
            "--library",
            "wires.ohlw",
            "--choose",
            "tower:T3:set:2=add",
        ],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let s = ohl_session::Session::open(&dir.join("main.ohl")).unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    assert_eq!(s.export().sections.len(), 3);
    drop(s);
    let out = ohl(&dir, &["line", "main.ohl"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("solved 3 of 3 sections"), "{t}");
    std::fs::remove_dir_all(&dir).ok();
}
