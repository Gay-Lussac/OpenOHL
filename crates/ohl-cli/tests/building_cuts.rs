// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The example plugin `plugins/examples/building-cuts` (owner 2026-10-31)
//! end to end: `ohl electrical fields --preset-plugin` (both fields in one
//! run, one report) on the synthetic double circuit (`examples/electrical`:
//! T1 at x = 0, T2 at 300 m, T3 at 600 m along x on the alignment `main`;
//! the right side is y < 0) with an invented point cloud (level ground at
//! 400 m; three invented buildings — a 12 × 8 × 7 m box 25 m right of
//! T1–T2 at mid-span, an L-shape 40 m left of T2–T3, a building across
//! T2 — and noise points) and an invented survey (ground points and a
//! surveyed outline): one summary with both fields' columns, one points
//! file, per cut a CSV and an SVG per field (parsed as XML), the GeoJSON;
//! the single-field command's report holds that field alone.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The plugin's directory when built, `None` to skip.
fn built_plugin() -> Option<PathBuf> {
    let dir = root().join("plugins/examples/building-cuts");
    let m = ohl_plugins::Manifest::load(&dir).unwrap();
    if m.entry_path().is_file() {
        return Some(dir);
    }
    assert!(
        ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugin not built: {}",
        m.entry_path().display()
    );
    eprintln!("skipping: building-cuts not built");
    None
}

fn point(x: f64, y: f64, z: f64, class: u8) -> ohl_lidar::LidarPoint {
    ohl_lidar::LidarPoint {
        x,
        y,
        z,
        class,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    }
}

/// Building points (class 6) on a 0.5 m grid where `keep` holds.
fn roof(
    out: &mut Vec<ohl_lidar::LidarPoint>,
    (x0, x1): (f64, f64),
    (y0, y1): (f64, f64),
    z: f64,
    keep: impl Fn(f64, f64) -> bool,
) {
    let (nx, ny) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
    for i in 0..=nx {
        for j in 0..=ny {
            let (x, y) = (x0 + i as f64 * 0.5, y0 + j as f64 * 0.5);
            if keep(x, y) {
                out.push(point(x, y, z, 6));
            }
        }
    }
}

fn run(kind: &str, extra: &[&str]) -> String {
    let ex = root().join("examples/electrical");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["electrical", kind])
        .arg(ex.join("double-circuit.ohl.json"))
        .arg("--library")
        .arg(ex.join("wires.ohlw"))
        .args(extra)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

#[test]
fn building_cuts_end_to_end() {
    let Some(plugin) = built_plugin() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("ohl-building-cuts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The invented point cloud.
    let mut cloud = Vec::new();
    roof(&mut cloud, (144.0, 156.0), (-33.0, -25.0), 407.0, |_, _| {
        true
    });
    roof(&mut cloud, (430.0, 450.0), (40.0, 60.0), 409.0, |x, y| {
        !(x > 440.0 && y > 50.0)
    });
    roof(&mut cloud, (294.0, 306.0), (-40.0, -30.0), 406.0, |_, _| {
        true
    });
    for k in 0..15 {
        let y = if k % 2 == 0 { 70.0 } else { -70.0 };
        cloud.push(point(20.0 + 37.0 * k as f64, y, 401.0, 6));
    }
    for i in 0..=320 {
        for j in 0..=80 {
            cloud.push(point(
                i as f64 * 2.0 - 20.0,
                j as f64 * 2.0 - 80.0,
                400.0,
                2,
            ));
        }
    }
    let las = dir.join("cloud.las");
    ohl_lidar::classify::write_las(&las, &cloud, &ohl_core::budget::Budget::new()).unwrap();
    let project = dir.join("cloud.ohl");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["pointcloud", "import"])
        .arg(&las)
        .arg("--into")
        .arg(&project)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The invented survey: the ground (the fields' terrain) and a
    // surveyed outline at the ground, 30 m left of T1–T2.
    let mut survey = String::from("x,y,z,code,line\n");
    for i in 0..=40 {
        for j in 0..=16 {
            survey.push_str(&format!(
                "{},{},400,GND,\n",
                i as f64 * 20.0 - 100.0,
                j as f64 * 20.0 - 160.0
            ));
        }
    }
    for (x, y) in [(100.0, 30.0), (110.0, 30.0), (110.0, 40.0), (100.0, 40.0)] {
        survey.push_str(&format!("{x},{y},400,BLD,S1\n"));
    }
    let terrain = dir.join("survey.csv");
    std::fs::write(&terrain, survey).unwrap();
    let params = dir.join("params.toml");
    std::fs::write(
        &params,
        "step = 2.0\nlimit_ut = 1.0\nlimit_kv_m = 5.0\ndxf = true\n",
    )
    .unwrap();
    let out_dir = dir.join("report");
    let common = [
        "--terrain",
        terrain.to_str().unwrap(),
        "--preset-plugin",
        plugin.to_str().unwrap(),
        "--params",
        params.to_str().unwrap(),
        "--cloud",
        project.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
    ];
    // Both fields in one run; the electric field's ground a plane under
    // each point (quick).
    let mut args = vec!["--current", "*=1000", "--plane"];
    args.extend(common);
    let text = run("fields", &args);
    // Per building the governing cut and the nearest point's on the
    // alignment (2 m and more apart here): two each.
    assert!(
        text.contains("preset buildings of plugin building-cuts (of buildings, places): 8 cuts, 30 points, 4 outlines"),
        "{text}"
    );
    assert!(text.contains("15 clusters left out"), "{text}");
    assert!(
        text.contains("Magnetic flux density of") && text.contains("Electric field strength of"),
        "{text}"
    );
    let read = |name: &str| {
        std::fs::read_to_string(out_dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    // The summary: four buildings in station order (the surveyed outline
    // first, at x = 105), both fields' values filled.
    let summary = read("buildings.csv");
    let rows: Vec<Vec<&str>> = summary.lines().map(|l| l.split(',').collect()).collect();
    assert_eq!(rows.len(), 5, "{summary}");
    let col = |name: &str| {
        rows[0]
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("{name}: {:?}", rows[0]))
    };
    let ids: Vec<&str> = rows[1..].iter().map(|r| r[0]).collect();
    assert_eq!(ids, ["B001", "B002", "B003", "B004"]);
    let the_box = &rows[2];
    assert_eq!(the_box[col("area_m2")], "96.0", "{summary}");
    assert_eq!(the_box[col("floors")], "3");
    assert_eq!(the_box[col("span")], "T1–T2");
    assert_eq!(the_box[col("alignment")], "main");
    for name in [
        "max_section_B_uT",
        "max_places_B_uT",
        "max_section_E_kV_m",
        "max_places_E_kV_m",
    ] {
        let v: f64 = the_box[col(name)].parse().unwrap();
        assert!(v > 0.0, "{name}: {summary}");
    }
    col("margin_B_uT");
    col("margin_E_kV_m");
    // The surveyed outline: 6 m assumed, 2 floors.
    assert_eq!(rows[1][col("floors")], "2");
    let points = read("points.csv");
    assert!(
        points
            .lines()
            .next()
            .unwrap()
            .ends_with("B_uT,B_major_uT,margin_B_uT,E_kV_m,margin_E_kV_m"),
        "{points}"
    );
    assert!(
        points.lines().nth(1).unwrap().starts_with("B001-F1-"),
        "{points}"
    );
    // The cuts' files: per cut a CSV grid and an SVG that parses, per
    // field; no file of the old per-field names.
    let names: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("buildings-") || n.starts_with("notes-")),
        "{names:?}"
    );
    for q in ["B", "E"] {
        let svgs: Vec<&String> = names
            .iter()
            .filter(|n| n.starts_with("cut-") && n.ends_with(&format!("-{q}.svg")))
            .collect();
        assert_eq!(svgs.len(), 8, "{names:?}");
        for s in svgs {
            let text = read(s);
            let doc = roxmltree::Document::parse(&text).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(doc.root_element().tag_name().name(), "svg");
            assert!(text.contains("<polyline"), "{s}");
            assert!(text.contains("alignment main at"), "{s}");
            let csv = read(&s.replace(".svg", ".csv"));
            assert!(csv.lines().count() > 100, "{s}");
        }
    }
    let gov = read("cut-B002-gov-B.svg");
    assert!(gov.contains(">B002 max "), "{gov}");
    assert!(gov.contains(">South L1<") || gov.contains("earth"), "{gov}");
    let geo: serde_json::Value = serde_json::from_str(&read("buildings.geojson")).unwrap();
    let kinds = |k: &str| {
        geo["features"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["properties"]["kind"] == k)
            .count()
    };
    assert_eq!((kinds("building"), kinds("cut")), (4, 8));
    assert!(read("buildings.dxf").ends_with("0\nEOF\n"));
    let notes = read("notes.txt");
    assert!(
        notes.contains("plugin: building B002: points 425;"),
        "{notes}"
    );
    assert!(
        notes.contains("field B: ")
            || notes.contains("field E: ")
            || notes.contains("B (µT) and E (kV/m)"),
        "{notes}"
    );
    // A single field's report: that field's columns alone.
    let single = dir.join("single");
    let mut args = vec!["--plane"];
    args.extend(common);
    let out_pos = args.iter().position(|a| *a == "--out").unwrap();
    let single_s = single.to_str().unwrap().to_owned();
    args[out_pos + 1] = &single_s;
    let text_e = run("electric", &args);
    assert!(text_e.contains("written:"), "{text_e}");
    let summary = std::fs::read_to_string(single.join("buildings.csv")).unwrap();
    assert!(
        summary.contains("max_section_E_kV_m") && !summary.contains("_B_"),
        "{summary}"
    );
    if ohl_core::env::var_os("OHL_KEEP_BUILDING_CUTS").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
