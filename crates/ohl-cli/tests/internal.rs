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

/// The volumes on the strings: T2 of the typed example given its own
/// string S1 labelled with the example's INVENTED single string with
/// volumes (examples/synthetic-line/Insulators), its corona ring
/// overridden as insulated (left out) and its tower-side horn as a longer
/// one (INVENTED); `ohl internal --insulators …` reports a row per
/// electrified shape, the line-side horn 1.15 m from the tower-side one.
#[test]
fn ohl_internal_reports_the_volumes_on_the_strings() {
    let dir = std::env::temp_dir().join(format!("ohl-internal-shapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut line: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(example("synthetic-line-typed.ohl.json")).unwrap(),
    )
    .unwrap();
    let t2 = line["structures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["id"] == "T2")
        .unwrap();
    // Its own wire point W1 (set 1: it replaces the type's, placed again
    // from the string) and string S1.
    let at = t2["position"].clone();
    t2["wires"] = serde_json::json!([{
        "label": "W1", "set": 1,
        "position": [at[0], at[1], at[2].as_f64().unwrap() + 26.5]
    }]);
    t2["strings"] = serde_json::json!([{
        "label": "S1", "top": "A1", "wire": "W1", "length": 3.5, "weight": 900.0,
        "insulator": "invented-i-volumes",
        "shapes": [{"name": "corona ring", "kind": "sphere", "state": "insulated",
                    "at_fraction": 1.0, "diameter": 0.3},
                   {"name": "tower-side horn", "kind": "cylinder", "state": "grounded",
                    "at": 1.0, "diameter": 0.02, "length": 2.0, "offset_across": 0.25}]
    }]);
    let file = dir.join("typed-with-volumes.ohl.json");
    std::fs::write(&file, serde_json::to_string_pretty(&line).unwrap()).unwrap();
    let rules = dir.join("internal.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"invented\"\n\
         [[internal_rule]]\nname = \"invented 0.5 m\"\ncases = [\"everyday\"]\ndistance = \"0.5 m\"\n",
    )
    .unwrap();
    let csv = dir.join("i.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("internal")
        .arg(&file)
        .arg("--structures")
        .arg(example("Structures"))
        .arg("--insulators")
        .arg(example("Insulators"))
        .arg("--weather")
        .arg(example("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(&rules)
        .arg("--csv")
        .arg(&csv)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.code().is_some_and(|c| c <= 1), "{text}\n{err}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let row = |part: &str| {
        table
            .lines()
            .find(|r| r.contains(&format!(",T2,{part},")))
            .unwrap_or_else(|| panic!("{part} not in\n{table}"))
    };
    // The line-side horn (3.15 m to 3.45 m below the top) nearest the
    // tower-side horn as overridden (down to 2 m, the same offset).
    let horn = row("string S1 line-side horn");
    assert!(horn.contains(",shape string S1 tower-side horn,"), "{horn}");
    assert!(
        horn.contains(",1.150,") && horn.contains(",surface,"),
        "{horn}"
    );
    row("string S1 clamp");
    assert!(!table.contains("corona ring"), "{table}");
    assert!(text.contains("string S1 line-side horn"), "{text}");
    assert!(text.contains("shape string S1 tower-side horn"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
