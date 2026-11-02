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
        2 * 201
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

/// A cut at an alignment station with its label (`--cut ALIGNMENT@STATION#LABEL`)
/// in the CSV and the results JSON (an `ohl-field-results-set` of one
/// field, 2026-10-31), whose values are the CSV's; the older span place
/// `FROM:TO@F` converted to its alignment station, said.
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
        "main@150#house 1",
        "--cut",
        "T2:T3@0.25#old",
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
    assert!(
        text.contains("cut house 1 main@150.0: span T1–T2 at 150.0 m"),
        "{text}"
    );
    assert!(
        text.contains("cut old main@375.0 (given as T2:T3@0.25: converted to main@375.00, cuts are placed on the alignment): span T2–T3 at 75.0 m"),
        "{text}"
    );
    let table = std::fs::read_to_string(&csv).unwrap();
    let rows: Vec<&str> = table
        .lines()
        .filter(|l| l.starts_with("cut,") && l.ends_with(",house 1"))
        .collect();
    assert!(!rows.is_empty() && rows.iter().all(|l| l.ends_with(",house 1")));
    let set: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(set["format"], "ohl-field-results-set");
    assert_eq!(set["fields"].as_array().unwrap().len(), 1);
    let v = &set["fields"][0];
    assert_eq!(v["format"], "ohl-field-results");
    assert_eq!(v["version"], 2);
    assert_eq!(v["quantity"], "B");
    let cut = &v["cuts"][0];
    assert_eq!(cut["label"], "house 1");
    assert_eq!(
        (cut["alignment"].as_str(), cut["station"].as_f64()),
        (Some("main"), Some(150.0))
    );
    assert_eq!(v["cuts"][1]["station"], 375.0);
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
        text.contains("cut house-A main@150.0: span T1–T2")
            && text.contains("cut house-B main@450.0: span T2–T3"),
        "{text}"
    );
    let summary = std::fs::read_to_string(out_dir.join("summary.txt")).unwrap();
    assert!(
        summary.starts_with("preset default\ncuts 2\npoints 2\n"),
        "{summary}"
    );
    assert!(
        summary.contains("ohl-field-results-set true\nfields 1\n"),
        "{summary}"
    );
    assert!(
        summary.contains("label house-A 1\nlabel house-B 1\n"),
        "{summary}"
    );
    // One cut at a time (2026-11-02): each cut's document with its grid,
    // the rest without the grids.
    assert!(
        summary.contains("grids false\ncut reports 2\nplan false\n"),
        "{summary}"
    );
    let cut = std::fs::read_to_string(out_dir.join("cut-house-A.txt")).unwrap();
    assert!(
        cut.contains("ohl-field-cut true\nfields 1\nvalues 1\n"),
        "{cut}"
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
    // Refused: a cut beyond the alignment (checked by the host), a preset
    // not given.
    let params = dir.join("bad.toml");
    std::fs::write(&params, "bad = \"station\"\n").unwrap();
    for (extra, want) in [
        (
            vec!["--params", params.to_str().unwrap()],
            "station 700 m beyond alignment main",
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

/// `ohl electrical fields` (owner, 2026-10-31: "Ok for 1 but keep it
/// clean"): both fields in one run on the same cut (at an alignment
/// station) and points, one results set holding both documents, one
/// report of a preset plugin with both; a cut without a station or on an
/// alignment not in the line refused.
#[test]
fn both_fields_in_one_run() {
    let dir = std::env::temp_dir().join(format!("ohl-fields-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let json = dir.join("fields.json");
    let ex = root().join("examples/electrical");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .args(["electrical", "fields"])
            .arg(ex.join("double-circuit.ohl.json"))
            .arg("--library")
            .arg(ex.join("wires.ohlw"))
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&[
        "--current",
        "*=1000",
        "--profile",
        "T1:T2@0.5",
        "--plane",
        "--cut",
        "main@150#mid",
        "--cut-step",
        "2",
        "--width",
        "20",
        "--level-ut",
        "10",
        "--level-kv-m",
        "1",
        "--json",
        json.to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("Magnetic flux density of") && text.contains("Electric field strength of"),
        "{text}"
    );
    assert_eq!(
        text.matches("cut mid main@150.0: span T1–T2").count(),
        2,
        "{text}"
    );
    let set: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(set["format"], "ohl-field-results-set");
    let q: Vec<&str> = set["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["quantity"].as_str().unwrap())
        .collect();
    assert_eq!(q, ["B", "E"]);
    // The same plane in both.
    let plane = |k: usize| set["fields"][k]["cuts"][0]["plane"].clone();
    assert_eq!(plane(0), plane(1));
    assert_eq!(set["fields"][0]["cuts"][0]["isolines"][0]["level"], 10.0);
    assert_eq!(set["fields"][1]["cuts"][0]["isolines"][0]["level"], 1.0);
    // The preset plugin's one report from both fields.
    if let Some(plugin) = built_plugin("tests/field-presets") {
        let out_dir = dir.join("report");
        let out = run(&[
            "--current",
            "*=1000",
            "--profile",
            "T1:T2@0.5",
            "--plane",
            "--preset-plugin",
            plugin.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let summary = std::fs::read_to_string(out_dir.join("summary.txt")).unwrap();
        assert!(
            summary.contains("ohl-field-results-set true\nfields 2\n"),
            "{summary}"
        );
        // Each cut reported once, with both fields' grids.
        assert!(
            summary.contains("label house-A 1\nlabel house-B 1\n"),
            "{summary}"
        );
        let cut = std::fs::read_to_string(out_dir.join("cut-house-B.txt")).unwrap();
        assert!(cut.contains("fields 2\nvalues 2\n"), "{cut}");
    }
    // A cut at an alignment station without a station, and on an
    // alignment that is not there: refused, said.
    for (cut, want) in [
        ("main", "ALIGNMENT@STATION"),
        ("other@10", "alignment other is not in the line"),
    ] {
        let out = run(&["--current", "*=1000", "--plane", "--cut", cut]);
        assert!(!out.status.success());
        let e = String::from_utf8_lossy(&out.stderr);
        assert!(e.contains(want), "{cut}: {e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A field plan template (2026-11-02): its weather cases (the wires in
/// one, system sysB and the earth wire each in their own, from
/// --weather), its levels added and its limits taken; handed to a preset
/// plugin's report. `--system-case` over the plan's; an unknown system
/// refused.
#[test]
fn a_field_plan_drives_the_run() {
    let dir = std::env::temp_dir().join(format!("ohl-field-plan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let weather = dir.join("cases.ohlwc");
    std::fs::write(
        &weather,
        "format = \"ohl-weather-cases\"\nversion = 1\nname = \"t\"\n\
         [[case]]\nname = \"warm\"\ntemperature = \"40 °C\"\n\
         [[case]]\nname = \"hot\"\ntemperature = \"80 °C\"\n\
         [[case]]\nname = \"cold\"\ntemperature = \"10 °C\"\n",
    )
    .unwrap();
    let plan = dir.join("plans.ohlfp");
    let text = ohl_electrical::field_plan::TEMPLATE
        .replace("case = \"\"", "case = \"warm\"")
        .replace("earth_wires = \"\"", "earth_wires = \"cold\"")
        .replace("systems = {}", "systems = { sysB = \"hot\" }");
    std::fs::write(&plan, text).unwrap();
    let ex = root().join("examples/electrical");
    let json = dir.join("fields.json");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .args(["electrical", "fields"])
            .arg(ex.join("double-circuit.ohl.json"))
            .arg("--library")
            .arg(ex.join("wires.ohlw"))
            .args(["--current", "*=1000", "--profile", "T1:T2@0.5", "--plane"])
            .arg("--weather")
            .arg(&weather)
            .arg("--plan")
            .arg(&plan)
            .args(args)
            .output()
            .unwrap()
    };
    let mut args = vec!["--cut", "main@150#mid", "--cut-step", "2", "--width", "20"];
    let j = json.to_str().unwrap().to_owned();
    args.extend(["--json", j.as_str()]);
    let out = run(&args);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains(
            "weather per system: the wires of no system in cold; system sysB in hot; the others in warm"
        ),
        "{text}"
    );
    let set: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(set["case"]["name"], "warm");
    // The plan's limits and levels (1 µT and 0.4 µT; 5 and 1 kV/m).
    assert_eq!(set["fields"][0]["limit"], 1.0);
    assert_eq!(set["fields"][1]["limit"], 5.0);
    let levels = |k: usize| -> Vec<f64> {
        set["fields"][k]["cuts"][0]["isolines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["level"].as_f64().unwrap())
            .collect()
    };
    assert_eq!(levels(0), vec![0.4, 1.0]);
    assert_eq!(levels(1), vec![1.0, 5.0]);
    // --system-case over the plan's; an unknown system refused.
    let out = run(&["--system-case", "sysB=cold"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("weather per system: the wires of no system in cold; system sysB in cold"),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run(&["--system-case", "nope=cold"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no electrical system nope"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Handed to a preset plugin's report.
    if let Some(plugin) = built_plugin("tests/field-presets") {
        let out_dir = dir.join("report");
        let o = out_dir.to_str().unwrap().to_owned();
        let p = plugin.to_str().unwrap().to_owned();
        let out = run(&["--preset-plugin", p.as_str(), "--out", o.as_str()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let summary = std::fs::read_to_string(out_dir.join("summary.txt")).unwrap();
        assert!(summary.contains("plan true\n"), "{summary}");
        let cut = std::fs::read_to_string(out_dir.join("cut-house-A.txt")).unwrap();
        assert!(cut.contains("plan true\n"), "{cut}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
