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

/// A cut's label from `--cut FROM:TO@F#LABEL` in the CSV and the
/// `ohl-field-results` JSON (2026-10-31), whose values are the CSV's.
#[test]
fn cut_labels_and_the_results_json() {
    let dir = std::env::temp_dir().join(format!("ohl-field-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (csv, json) = (dir.join("b.csv"), dir.join("b.json"));
    let out = magnetic(&[
        "--current",
        "*=1000",
        "--profile",
        "T1:T2@0.5",
        "--cut",
        "T1:T2@0.5#house 1",
        "--cut-step",
        "2",
        "--width",
        "20",
        "--level",
        "10",
        "--csv",
        csv.to_str().unwrap(),
        "--json",
        json.to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("cut house 1 T1–T2 at 150.0 m"), "{text}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<&str> = table.lines().filter(|l| l.starts_with("cut,")).collect();
    assert!(!rows.is_empty() && rows.iter().all(|l| l.ends_with(",house 1")));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(v["format"], "ohl-field-results");
    assert_eq!(v["quantity"], "B");
    let cut = &v["cuts"][0];
    assert_eq!(cut["label"], "house 1");
    let values = cut["values"].as_array().unwrap();
    assert_eq!(values.len(), rows.len());
    for (j, r) in values.iter().zip(&rows) {
        let b: f64 = r.split(',').nth(10).unwrap().parse().unwrap();
        assert!((j.as_f64().unwrap() - b).abs() <= 0.6e-5, "{j} {r}");
    }
    assert_eq!(cut["isolines"][0]["level"], 10.0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The test plugin's directory when built, `None` to skip.
fn built_plugin(relative: &str) -> Option<PathBuf> {
    let dir = root().join("plugins").join(relative);
    let m = ohl_plugins::Manifest::load(&dir).unwrap();
    if m.entry_path().is_file() {
        return Some(dir);
    }
    assert!(
        ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugin not built: {}",
        m.entry_path().display()
    );
    eprintln!("skipping: {relative} not built");
    None
}

/// `--preset-plugin` (2026-10-31): the survey points of the manifest's
/// codes gathered from --terrain and given to the test plugin, its
/// labelled cuts and points added and run, its report file written into
/// --out from the results; the electric field the same; a preset the
/// host refuses (a cut in no span) and a preset not given are errors.
#[test]
fn field_presets_from_a_plugin() {
    let Some(plugin) = built_plugin("tests/field-presets") else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("ohl-field-presets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // An invented survey: level ground at 400 m, two building points, one
    // of them beyond the reach.
    let survey = dir.join("survey.csv");
    std::fs::write(
        &survey,
        "x,y,z,code\n-50,-150,400,GND\n650,-150,400,GND\n650,150,400,GND\n-50,150,400,GND\n\
         150,-30,406,BLD-roof\n150,140,406,BLD-far\n",
    )
    .unwrap();
    let out_dir = dir.join("report");
    let out = magnetic(&[
        "--current",
        "*=1000",
        "--profile",
        "T1:T2@0.5",
        "--terrain",
        survey.to_str().unwrap(),
        "--preset-plugin",
        plugin.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("1 survey points of the codes BLD* (of 6 read)"),
        "{text}"
    );
    assert!(
        text.contains("preset default of plugin field-presets (of default, points-only): 2 cuts, 2 points, 1 outlines"),
        "{text}"
    );
    assert!(
        text.contains("plugin note: received 1 points (1 survey, classes none), 0 ground points"),
        "{text}"
    );
    assert!(
        text.contains("cut house-A T1–T2") && text.contains("cut house-B T2–T3"),
        "{text}"
    );
    let summary = std::fs::read_to_string(out_dir.join("summary.txt")).unwrap();
    assert!(
        summary.starts_with("preset default\ncuts 2\npoints 2\n"),
        "{summary}"
    );
    assert!(summary.contains("ohl-field-results true"), "{summary}");
    assert!(
        summary.contains("label house-A 1\nlabel house-B 1\n"),
        "{summary}"
    );
    // The electric field: the points-only preset, the plane ground (quick).
    let ex = root().join("examples/electrical");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["electrical", "electric"])
        .arg(ex.join("double-circuit.ohl.json"))
        .arg("--library")
        .arg(ex.join("wires.ohlw"))
        .args(["--profile", "T1:T2@0.5", "--plane", "--terrain"])
        .arg(&survey)
        .arg("--preset-plugin")
        .arg(&plugin)
        .args(["--preset", "points-only", "--out"])
        .arg(&out_dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("house-A") && text.contains("E kV/m"),
        "{text}"
    );
    let summary = std::fs::read_to_string(out_dir.join("summary.txt")).unwrap();
    assert!(
        summary.starts_with("preset points-only\ncuts 0\npoints 2\n"),
        "{summary}"
    );
    // Refused: a cut in no span (checked by the host), a preset not given.
    let params = dir.join("bad.toml");
    std::fs::write(&params, "bad = \"span\"\n").unwrap();
    for (extra, want) in [
        (
            vec!["--params", params.to_str().unwrap()],
            "no span between structures T1 and T3",
        ),
        (vec!["--preset", "nope"], "no preset \"nope\""),
    ] {
        let mut args = vec![
            "--current",
            "*=1000",
            "--profile",
            "T1:T2@0.5",
            "--preset-plugin",
            plugin.to_str().unwrap(),
        ];
        args.extend(extra);
        let out = magnetic(&args);
        assert!(!out.status.success());
        let e = String::from_utf8_lossy(&out.stderr);
        assert!(e.contains(want), "{e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
