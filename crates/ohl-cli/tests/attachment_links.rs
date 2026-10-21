// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints from the command line
//! (2026-10-21): `ohl structures points` lists the links and matches a
//! type swap; `ohl structures link` links at an offset, takes the joint
//! offered, unlinks — the line file written and read again.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn ohl(args: &[&str]) -> Output {
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

#[test]
fn link_offer_and_unlink() {
    let dir = std::env::temp_dir().join(format!("ohl-cli-links-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let line = example().join("synthetic-line-typed.ohl.json");
    let structures = example().join("Structures");
    let (line, structures) = (line.to_str().unwrap(), structures.to_str().unwrap());
    let out = dir.join("linked.ohl.json");
    let out = out.to_str().unwrap();
    let listed = ok(&ohl(&["structures", "points", line, "--structure", "T2"]));
    assert!(listed.contains("A11          A2"), "{listed}");
    // type-03's arm tips are 25 mm further along the line: offered.
    let swap = ok(&ohl(&[
        "structures",
        "points",
        line,
        "--structure",
        "T2",
        "--to",
        "type-03",
    ]));
    assert!(swap.contains("offered: joint A1"), "{swap}");
    ok(&ohl(&[
        "structures",
        "link",
        line,
        "--structure",
        "T2",
        "--point",
        "A1",
        "--joint",
        "A1",
        "--offset",
        "0,0.5,0",
        "--structures",
        structures,
        "--out",
        out,
    ]));
    let listed = ok(&ohl(&[
        "structures",
        "points",
        out,
        "--structure",
        "T2",
        "--structures",
        structures,
    ]));
    assert!(listed.contains("(0.000, 0.500, 0.000)"), "{listed}");
    ok(&ohl(&[
        "structures",
        "link",
        out,
        "--structure",
        "T2",
        "--point",
        "A1",
        "--unlink",
        "--structures",
        structures,
        "--out",
        out,
    ]));
    let listed = ok(&ohl(&[
        "structures",
        "points",
        out,
        "--structure",
        "T2",
        "--structures",
        structures,
    ]));
    assert!(listed.contains("unlinked"), "{listed}");
    ok(&ohl(&[
        "structures",
        "link",
        out,
        "--structure",
        "T2",
        "--point",
        "A1",
        "--nearest",
        "--structures",
        structures,
        "--out",
        out,
    ]));
    let listed = ok(&ohl(&[
        "structures",
        "points",
        out,
        "--structure",
        "T2",
        "--structures",
        structures,
    ]));
    assert!(!listed.contains("unlinked"), "{listed}");
    std::fs::remove_dir_all(&dir).ok();
}
