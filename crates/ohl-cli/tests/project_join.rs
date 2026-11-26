// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl project join`: the example line split at its Y tower T3 into two
//! project files (the main route; the branch with its own T3). The dry
//! run prints the plan — the ids renamed, the twin with the option that
//! ticks it — and writes nothing; `--merge T3=T3` joins the branch with
//! one shared T3 as one logged command and one revision; `ohl project
//! undo` takes it all back; the other file is never changed.

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
fn a_branch_file_is_joined_with_one_shared_tower() {
    let dir = std::env::temp_dir().join(format!("ohl-project-join-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let full =
        ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap();
    let mut main = full.clone();
    main.routes.retain(|r| r.id == "main");
    main.alignments.retain(|a| a.id == "main");
    main.alignment_points
        .retain(|p| ["P1", "P2", "P3"].contains(&p.id.as_str()));
    main.structures.retain(|s| s.id != "T6" && s.id != "T7");
    main.sections.retain(|s| s.id != "S3");
    main.jumpers.clear();
    main.circuits.retain(|c| c.name != "Branch");
    let mut branch = full;
    branch.routes.retain(|r| r.id == "branch");
    branch.alignments.retain(|a| a.id == "branch");
    branch
        .alignment_points
        .retain(|p| ["P2", "P4", "P5"].contains(&p.id.as_str()));
    branch
        .structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    branch.sections.retain(|s| s.id == "S3");
    branch.jumpers.clear();
    branch.circuits.retain(|c| c.name == "Branch");
    for (name, line) in [("line.ohl", &main), ("branch.ohl", &branch)] {
        let mut s = ohl_session::Session::open(&dir.join(name)).unwrap();
        s.import(line, "tester").unwrap();
    }
    let bytes = std::fs::read(dir.join("branch.ohl")).unwrap();
    // The dry run: the plan, nothing written.
    let out = ohl(
        &dir,
        &[
            "project",
            "join",
            "branch.ohl",
            "--into",
            "line.ohl",
            "--user",
            "tester",
            "--dry-run",
        ],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("structure T3 → branch-T3"), "{t}");
    assert!(t.contains("two towers (not ticked)"), "{t}");
    assert!(t.contains("--merge T3=T3"), "{t}");
    assert!(t.contains("dry run: nothing written"), "{t}");
    let s = ohl_session::Session::open_read_only(&dir.join("line.ohl")).unwrap();
    assert_eq!(s.export().structures.len(), 5);
    drop(s);
    // Joined, T3 shared.
    let out = ohl(
        &dir,
        &[
            "project",
            "join",
            "branch.ohl",
            "--into",
            "line.ohl",
            "--user",
            "tester",
            "--merge",
            "T3=T3",
        ],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("one shared tower"), "{t}");
    assert!(t.contains("joined routes from branch.ohl"), "{t}");
    let s = ohl_session::Session::open_read_only(&dir.join("line.ohl")).unwrap();
    let l = s.export();
    assert_eq!(l.structures.len(), 7);
    let s3 = l.sections.iter().find(|x| x.id == "S3").unwrap();
    assert_eq!(
        (s3.points[0].structure.as_str(), s3.points[0].set),
        ("T3", 2)
    );
    drop(s);
    // One undo takes it all back.
    let out = ohl(&dir, &["project", "undo", "line.ohl", "--user", "tester"]);
    assert!(out.status.success(), "{}", text(&out));
    let s = ohl_session::Session::open_read_only(&dir.join("line.ohl")).unwrap();
    assert_eq!(s.export(), main);
    drop(s);
    assert_eq!(std::fs::read(dir.join("branch.ohl")).unwrap(), bytes);
}
