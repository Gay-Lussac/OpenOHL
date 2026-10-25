// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl internal` on the typed example line with an INVENTED rule (a
//! distance no real code is taken from): the structures of type-02 (level
//! 3, members without profiles: to their centre lines) checked against
//! their members, T6 against its hollow pole (the example's wire point
//! sits on the pole's axis at its top: in its hole, the wires through
//! its wall — failing), every structure without a body
//! listed as not checked; the smallest margin first; a rule file without
//! internal rules refused.

use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

#[test]
fn ohl_internal_checks_the_structures_with_a_body() {
    let dir = std::env::temp_dir().join(format!("ohl-internal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rules = dir.join("internal.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"invented\"\n\
         [[internal_rule]]\nname = \"invented 0.5 m\"\ncases = [\"everyday\"]\ndistance = \"0.5 m\"\n",
    )
    .unwrap();
    let csv = dir.join("i.csv");
    let run = |rules: &Path| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("internal")
            .arg(example("synthetic-line-typed.ohl.json"))
            .arg("--structures")
            .arg(example("Structures"))
            .arg("--weather")
            .arg(example("weather-cases.ohlwc"))
            .arg("--rules")
            .arg(rules)
            .arg("--csv")
            .arg(&csv)
            .output()
            .unwrap()
    };
    let out = run(&rules);
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{text}\n{err}");
    assert!(text.contains("not checked: T1"), "{text}");
    assert!(!text.contains("not checked: T2"), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<&str> = table.lines().skip(1).collect();
    assert!(!rows.is_empty(), "{text}");
    // Only the level-3 structures, the case the rule names.
    let at = |r: &str, s: &str| r.contains(&format!(",{s},"));
    assert!(
        rows.iter()
            .all(|r| at(r, "T2") || at(r, "T4") || at(r, "T6")),
        "{table}"
    );
    assert!(text.contains("to the members' centre lines"), "{text}");
    assert!(rows.iter().all(|r| r.contains(",everyday,")));
    // The smallest margin first; the lattice towers' wires clear of their
    // members, T6's wire point in its pole's hole (0.17 m from its inner
    // wall), its wires through the wall (below 0).
    let margin = |r: &str| r.split(',').nth(7).unwrap().parse::<f64>().unwrap();
    assert!(rows.windows(2).all(|w| margin(w[0]) <= margin(w[1])));
    let distance = |r: &str| r.split(',').nth(5).unwrap().parse::<f64>().unwrap();
    for r in &rows {
        if at(r, "T6") {
            assert!(r.contains(",pole,") && r.contains(",fail,"), "{r}");
            assert!(distance(r) < 0.5, "{r}");
            if r.contains(" span ") {
                assert!(distance(r) <= 0.0, "{r}");
            }
        } else {
            assert!(distance(r) > 0.0 && r.contains(",centre line,"), "{r}");
        }
    }
    // A rule file without internal rules: refused (status 2).
    let out = run(&example("clearance-rules.ohlr"));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no [[internal_rule]]"));
    let _ = std::fs::remove_dir_all(&dir);
}
