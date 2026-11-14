// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl staking` on the typed example with its survey: both tables
//! printed, the CSVs written with a row per structure and alignment and
//! per foot, the ground filled in; without a terrain, said and left empty.

use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

#[test]
fn the_staking_table_from_the_command_line() {
    let d = std::env::temp_dir().join(format!("ohl-cli-staking-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    let (csv, feet) = (d.join("staking.csv"), d.join("feet.csv"));
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("staking")
        .arg(example("synthetic-line-typed.ohl.json"))
        .arg("--terrain")
        .arg(example("synthetic-survey.csv"))
        .arg("--csv")
        .arg(&csv)
        .arg("--feet-csv")
        .arg(&feet)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("10.00° L"), "{stdout}");
    let rows = std::fs::read_to_string(&csv).unwrap();
    assert!(rows.starts_with("alignment,structure,type,extension,station"));
    // Seven structures, the Y pylon on two alignments.
    assert_eq!(rows.lines().count(), 1 + 8, "{rows}");
    // Every row with a ground (the 13th field).
    for l in rows.lines().skip(1) {
        let ground = l.split(',').nth(12).unwrap();
        assert!(!ground.is_empty(), "{l}");
    }
    let feet = std::fs::read_to_string(&feet).unwrap();
    assert_eq!(feet.lines().count(), 1 + 8, "{feet}");
    // Without a terrain: said, the ground empty.
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("staking")
        .arg(example("synthetic-line-typed.ohl.json"))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ground: none given"));
    std::fs::remove_dir_all(&d).ok();
}
