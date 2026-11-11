// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl tensions` on the example line with the example's tension rules
//! (invented factors): a row per span, case and rule, the largest usage
//! first, the CSV written; exit status 1 when a span is above its allowed
//! tension; a rule file without tension rules refused.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ohl_tensions_writes_every_span_and_says_which_fail() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-tensions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("t.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("tensions")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(ex.join("tension-rules.ohlr"))
        .arg("--csv")
        .arg(&csv)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    // The everyday horizontal rule (invented 0.18 RTS) is exceeded on the
    // example's first section: exit status 1.
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("FAIL") && text.contains("pass"), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<&str> = table.lines().skip(1).collect();
    assert!(!rows.is_empty());
    // The largest usage first.
    let usage = |r: &str| r.split(',').nth(10).unwrap().parse::<f64>().unwrap();
    assert!(rows.windows(2).all(|w| usage(w[0]) >= usage(w[1])));
    // Only the cases the rules name.
    assert!(rows.iter().all(|r| !r.contains("high wind")));
    // A rule file without tension rules: refused (status 2).
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("tensions")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(ex.join("clearance-rules.ohlr"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no [[tension_rule]]"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A CSV row's fields (quoted ones whole).
fn fields(row: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    for c in row.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// A rule's calculation (owner, 2026-11-01): the same rule by each
/// section's own calculation and by the ruling span, side by side in one
/// file — each case solved once per calculation, the rows naming it, the
/// tensions not all the same.
#[test]
fn a_tension_rule_takes_its_own_calculation() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-tensions-calc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rules = dir.join("rules.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"test\"\n\n\
         [[tension_rule]]\nname = \"own\"\ncases = [\"everyday\"]\nfactor = 0.9\n\n\
         [[tension_rule]]\nname = \"rs\"\ncases = [\"everyday\"]\ncalculation = \"initial-rs\"\nfactor = 0.9\n",
    )
    .unwrap();
    let csv = dir.join("t.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("tensions")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(&rules)
        .arg("--csv")
        .arg(&csv)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let of = |rule: &str| -> Vec<(String, f64)> {
        table
            .lines()
            .skip(1)
            .map(fields)
            .filter(|f| f[0] == rule)
            .map(|f| (f[1].clone(), f[6].parse().unwrap()))
            .collect()
    };
    let (own, rs) = (of("own"), of("rs"));
    assert_eq!(own.len(), rs.len());
    assert!(own.iter().all(|r| r.0 == "everyday"), "{own:?}");
    assert!(rs.iter().all(|r| r.0 == "everyday, Initial RS"), "{rs:?}");
    assert!(
        own.iter().zip(&rs).any(|(a, b)| (a.1 - b.1).abs() > 1.0),
        "{table}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--sag-to-limits` on a copy of the example line (invented factors):
/// each section in the blocks gets the largest stringing tension the
/// rules allow, so the check that follows passes (exit status 0) with a
/// span at its limit; nothing is written without `--write`; with it the
/// line file holds the tensions found and checks the same again.
#[test]
fn ohl_tensions_sags_to_the_limits_and_writes_them_back() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-sag-to-limits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let line = dir.join("line.ohl.json");
    let before = std::fs::read_to_string(ex.join("synthetic-line.ohl.json")).unwrap();
    std::fs::write(&line, &before).unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("tensions")
            .arg(&line)
            .arg("--weather")
            .arg(ex.join("weather-cases.ohlwc"))
            .arg("--rules")
            .arg(ex.join("tension-rules.ohlr"))
            .args(extra)
            .output()
            .unwrap()
    };
    // Not written: the file as it was, the stringings printed.
    let out = run(&["--sag-to-limits", "--sections", "S1"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Sag to limits:") && text.contains("not written (give --write)"));
    assert!(text.contains("check the clearances"), "{text}");
    assert!(text.contains("governed by \"Everyday horizontal"), "{text}");
    assert!(!text.contains("\n  S3 "), "only the section asked: {text}");
    assert_eq!(std::fs::read_to_string(&line).unwrap(), before);
    // Written: every section in the blocks; the check passes at the limit.
    let out = run(&["--sag-to-limits", "--write"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("written: 2 stringings"), "{text}");
    assert!(text.contains("clipped in: only a section in the blocks is sagged"));
    assert!(
        text.contains("100.0 pass") && !text.contains("FAIL"),
        "{text}"
    );
    let after = std::fs::read_to_string(&line).unwrap();
    assert_ne!(after, before);
    // The file as written checks the same without sagging again.
    let again = run(&[]);
    assert_eq!(again.status.code(), Some(0));
    // --write needs --sag-to-limits.
    assert_eq!(run(&["--write"]).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}
