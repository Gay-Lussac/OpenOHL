// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A clearance rule's own calculation (owner, 2026-11-01: e.g. the ground
//! at max sag): `ohl line --rules` on the example line, the same ground
//! rule by each section's own calculation and by the ruling span, each
//! checked on the wires solved by its calculation.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn a_ground_rule_takes_its_own_calculation() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-clearance-calc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rules = dir.join("rules.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"test\"\n\n\
         [[rule]]\nname = \"own\"\ntarget = \"ground\"\nvertical = \"100 m\"\n\n\
         [[rule]]\nname = \"rs\"\ntarget = \"ground\"\nvertical = \"100 m\"\ncalculation = \"initial-rs\"\n",
    )
    .unwrap();
    let csv = dir.join("c.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("line")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .args(["--case", "max temperature", "--rules"])
        .arg(&rules)
        .arg("--terrain")
        .arg(ex.join("synthetic-survey.csv"))
        .arg("--clearance-csv")
        .arg(&csv)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    // (100 m: every span below it, a row for each rule; `ohl line`
    // reports, its status stays 0.)
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Each rule checked once, the second on the ruling span's wires.
    assert_eq!(text.matches("rule \"own\" (").count(), 1, "{text}");
    assert_eq!(
        text.matches("rule \"rs\", wires by Initial RS (").count(),
        1,
        "{text}"
    );
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows = |rule: &str| {
        table
            .lines()
            .filter(|l| l.split(',').nth(1) == Some(rule))
            .count()
    };
    assert!(rows("own") > 0 && rows("own") == rows("rs"), "{table}");
    // The ruling span's wires hang otherwise: another lowest clearance.
    let lowest = |rule: &str| -> f64 {
        table
            .lines()
            .filter(|l| l.split(',').nth(1) == Some(rule))
            .map(|l| l.split(',').nth(7).unwrap().parse::<f64>().unwrap())
            .fold(f64::INFINITY, f64::min)
    };
    assert!((lowest("own") - lowest("rs")).abs() > 0.01, "{table}");
    let _ = std::fs::remove_dir_all(&dir);
}
