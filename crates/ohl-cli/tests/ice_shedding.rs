// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl ice-shedding` on the example line with the example's ice shedding
//! rules (invented factors and distances): a row per pair, case and rule,
//! the smallest margin first, the CSV written, exit status 0 when every
//! clearance is kept; a distance no pair keeps gives status 1; a rule file
//! without ice shedding rules is refused.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(rules: &Path, csv: Option<&Path>) -> std::process::Output {
    let ex = root().join("examples/synthetic-line");
    let mut c = Command::new(env!("CARGO_BIN_EXE_ohl"));
    c.arg("ice-shedding")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(rules);
    if let Some(p) = csv {
        c.arg("--csv").arg(p);
    }
    c.output().unwrap()
}

#[test]
fn ohl_ice_shedding_writes_every_pair_and_says_which_fail() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-ice-shedding-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("i.csv");
    let out = run(&ex.join("ice-shedding-rules.ohlr"), Some(&csv));
    let text = String::from_utf8_lossy(&out.stdout);
    // S1 and S2 hang side by side 11 m apart: every clearance kept.
    assert_eq!(out.status.code(), Some(0), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<Vec<&str>> = table
        .lines()
        .skip(1)
        .map(|r| r.split(',').collect())
        .collect();
    // Four spans of S1 and S2, each against the other, by two rules.
    assert_eq!(rows.len(), 16, "{table}");
    // The smallest margin first.
    let margin = |r: &Vec<&str>| r[13].parse::<f64>().unwrap();
    assert!(rows.windows(2).all(|w| margin(&w[0]) <= margin(&w[1])));
    // A span shedding alone rises more than every span shedding (the
    // tension held: the conservative case), the jump 2 × the rise (the
    // default rebound factor: the example gives none).
    let rise = |rule: &str, section: &str, span: &str| -> f64 {
        rows.iter()
            .find(|r| r[0].starts_with(rule) && r[3] == section && r[4] == span)
            .unwrap()[8]
            .parse()
            .unwrap()
    };
    assert!(rise("One span", "S1", "1") > rise("Every span", "S1", "1"));
    for r in &rows {
        let (rise, jump): (f64, f64) = (r[8].parse().unwrap(), r[9].parse().unwrap());
        assert!((jump - 2.0 * rise).abs() < 2e-3, "{r:?}");
        assert_eq!(r[2], "glaze ice without ice");
    }
    // A distance no pair keeps: status 1.
    let tight = dir.join("tight.ohlr");
    std::fs::write(
        &tight,
        "format = \"ohl-clearance-rules\"\nversion = 1\n[[ice_shedding_rule]]\nname = \"t\"\n\
         cases = [\"glaze ice\"]\nfactor = 1.0\ndistance = \"50 m\"\n",
    )
    .unwrap();
    let out = run(&tight, None);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("FAIL"), "{text}");
    // A rule file without ice shedding rules: refused (status 2).
    let out = run(&ex.join("clearance-rules.ohlr"), None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no [[ice_shedding_rule]]"));
    let _ = std::fs::remove_dir_all(&dir);
}
