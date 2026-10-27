// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl electrical parameters` on the synthetic double-circuit example
//! (`examples/electrical`): a row per circuit and stretch and the coupling,
//! the assumed soil resistivity said, the CSV written; the settings from
//! the command line; an unknown section refused.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(extra: &[&str]) -> std::process::Output {
    let ex = root().join("examples/electrical");
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["electrical", "parameters"])
        .arg(ex.join("double-circuit.ohl.json"))
        .arg("--library")
        .arg(ex.join("wires.ohlw"))
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn parameters_of_the_example() {
    let dir = std::env::temp_dir().join(format!("ohl-electrical-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("p.csv");
    let out = run(&["--csv", csv.to_str().unwrap(), "--cross-sections"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("100 Ω·m (assumed)"), "{text}");
    assert!(text.contains("cross-section T1–T2"), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<&str> = table
        .lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .collect();
    let kinds: Vec<&str> = rows.iter().map(|r| r.split(',').next().unwrap()).collect();
    assert_eq!(
        kinds,
        ["circuit", "stretch", "circuit", "stretch", "coupling"]
    );
    // The positive sequence reactance of a single Drake circuit, a few
    // tenths of an ohm per km.
    let x1: f64 = rows[0].split(',').nth(7).unwrap().parse().unwrap();
    assert!((0.3..0.5).contains(&x1), "{x1}");
    assert!(table.contains("# soil resistivity not set"));
    // Given on the command line: no longer assumed; a section filter.
    let out = run(&[
        "--soil",
        "250",
        "--height",
        "mean",
        "--earth",
        "complex_depth",
        "--section",
        "NL2",
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("soil (the line's) 250 Ω·m;"), "{text}");
    assert!(
        text.contains("complex depth") && text.contains("2/3 of the sag"),
        "{text}"
    );
    assert!(text.contains("North") && !text.contains("South"), "{text}");
    // An unknown section: refused.
    let out = run(&["--section", "nope"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no such section"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn magnetic(extra: &[&str]) -> std::process::Output {
    let ex = root().join("examples/electrical");
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["electrical", "magnetic"])
        .arg(ex.join("double-circuit.ohl.json"))
        .arg("--library")
        .arg(ex.join("wires.ohlw"))
        .args(extra)
        .output()
        .unwrap()
}

/// `ohl electrical magnetic`: the profiles, a limit's line, given points
/// from a CSV, the CSV written; the thermal rating as the currents; 2D;
/// no current refused.
#[test]
fn magnetic_field_of_the_example() {
    let dir = std::env::temp_dir().join(format!("ohl-magnetic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let points = dir.join("points.csv");
    std::fs::write(&points, "name,x,y,height\nhouse,150,-30,4\n").unwrap();
    let csv = dir.join("b.csv");
    let out = magnetic(&[
        "--current",
        "*=1000",
        "--current",
        "South=1000:reverse",
        "--earth-wires",
        "--limit",
        "1",
        "--points",
        points.to_str().unwrap(),
        "--csv",
        csv.to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("against the line"), "{text}");
    assert!(text.contains("T1–T2") && text.contains("T2–T3"), "{text}");
    assert!(text.contains("house"), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    assert_eq!(
        table.lines().filter(|l| l.starts_with("profile")).count(),
        2 * 101
    );
    assert!(table.lines().any(|l| l.starts_with("point,,,,house")));
    // The thermal rating of the example's wire as the currents, 2D.
    let ambient = root().join("examples/synthetic-line/thermal-ambient.ohlta");
    let out = magnetic(&[
        "--thermal",
        ambient.to_str().unwrap(),
        "--ambient-case",
        "summer still air",
        "--max-temp",
        "80",
        "--two-d",
        "--profile",
        "T1:T2@0.5",
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("thermal rating") && text.contains("2D"),
        "{text}"
    );
    // No current: refused.
    let out = magnetic(&[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--current"));
    let _ = std::fs::remove_dir_all(&dir);
}
