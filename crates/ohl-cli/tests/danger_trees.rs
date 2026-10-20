// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl lidar vegetation` with the danger-tree options on the synthetic
//! example line and an invented point cloud: the ground of the example
//! survey on a 2 m grid along S1's first span, a tree under its middle and
//! a tall tree beside it.

use std::path::{Path, PathBuf};
use std::process::Command;

use ohl_core::budget::Budget;
use ohl_weather::WeatherCases;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> PathBuf {
    root().join("examples/synthetic-line").join(name)
}

#[test]
fn danger_trees_with_swing_arc_ice_and_isoclearance_lines() {
    let dir = std::env::temp_dir().join(format!("ohl-danger-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let line = ohl_line::Line::load(&example("synthetic-line.ohl.json")).unwrap();
    let library = ohl_cables::Library::load(&root().join("libraries/cables")).unwrap();
    let hot = WeatherCases::load(&example("weather-cases.ohlwc"))
        .unwrap()
        .cases
        .into_iter()
        .find(|c| c.name == "max temperature")
        .unwrap();
    let shapes = ohl_line::solve::solve_line(&line, &library, &hot);
    let s1 = shapes.iter().flatten().find(|s| s.section == "S1").unwrap();
    let span = &s1.spans[0];
    let (a, b) = (span.points[0], *span.points.last().unwrap());
    let mid = span.mid_point;
    let survey = ohl_terrain::Terrain::load(&example("synthetic-survey.csv")).unwrap();
    let point = |p: [f64; 3], class: u8| ohl_lidar::LidarPoint {
        x: p[0],
        y: p[1],
        z: p[2],
        class,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    };
    let mut points = Vec::new();
    // Ground on a 2 m grid around the span's chord, ±40 m.
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    let (ux, uy) = (dx / len, dy / len);
    let mut s = -10.0;
    while s <= len + 10.0 {
        let mut o = -40.0;
        while o <= 40.0 {
            let (x, y) = (a[0] + s * ux - o * uy, a[1] + s * uy + o * ux);
            if let Some(z) = survey.height(x, y) {
                points.push(point([x, y, z], 2));
            }
            o += 2.0;
        }
        s += 2.0;
    }
    // A tree top 3 m under the middle of the span; a tall tree 12 m
    // beside it, as tall as the wire is high plus 5 m.
    points.push(point([mid[0], mid[1], mid[2] - 3.0], 5));
    let (tx, ty) = (mid[0] - 12.0 * uy, mid[1] + 12.0 * ux);
    let g = survey.height(tx, ty).unwrap();
    points.push(point([tx, ty, mid[2].max(g + 10.0) + 5.0], 5));
    let file = dir.join("scan.ohl");
    ohl_pointcloud::Store::open(&file)
        .unwrap()
        .import_points("scan", "", None, 64.0, 0.001, points, &Budget::new())
        .unwrap();
    let cloud = format!("{}#scan", file.display());
    let (csv, geojson, dxf, lines_csv) = (
        dir.join("danger.csv"),
        dir.join("iso.geojson"),
        dir.join("iso.dxf"),
        dir.join("iso.csv"),
    );
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["lidar", "vegetation"])
        .arg(example("synthetic-line.ohl.json"))
        .args(["--pointcloud", &cloud, "--weather"])
        .arg(example("weather-cases.ohlwc"))
        .args([
            "--case",
            "max temperature",
            "--case",
            "high wind",
            "--case",
            "glaze ice",
            "--clearance",
            "4",
            "--fall-in",
            "--root-ball",
            "0.1",
            "--fall-allowance",
            "1",
            "--swing-arc",
            "--unbalanced-ice",
            "case",
            "--csv",
        ])
        .arg(&csv)
        .arg("--isoclearance")
        .arg(&geojson)
        .arg("--isoclearance")
        .arg(&dxf)
        .arg("--isoclearance")
        .arg(&lines_csv)
        .args(["--iso-interval", "5", "--iso-max", "30"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Against S1's first span: the tree under it grows in, both trees
    // can fall in (the one under it too); the arcs of the windy case and
    // the unbalanced ice of the iced case solved.
    assert!(stdout.contains("swing arcs"), "{stdout}");
    assert!(stdout.contains("unbalanced ice:"), "{stdout}");
    let row = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("S1 span 0"))
        .unwrap_or_else(|| panic!("{stdout}"));
    let cols: Vec<&str> = row.split_whitespace().collect();
    assert_eq!((cols[3], cols[4]), ("1", "2"), "{row}");
    println!("{stdout}");
    let table = std::fs::read_to_string(&csv).unwrap();
    assert!(table.starts_with(ohl_clearance::vegetation::DANGER_CSV_HEADER));
    assert!(
        table
            .lines()
            .any(|l| l.starts_with("S1,0,") && l.contains(",worst_fall,"))
    );
    let gj = std::fs::read_to_string(&geojson).unwrap();
    assert!(gj.contains("\"FeatureCollection\"") && gj.contains("OHL_ISOCLEARANCE"));
    assert!(
        std::fs::read_to_string(&dxf)
            .unwrap()
            .contains("OHL_ISOCLEARANCE")
    );
    assert!(
        std::fs::read_to_string(&lines_csv)
            .unwrap()
            .starts_with(ohl_clearance::isoclearance::CSV_HEADER)
    );
    std::fs::remove_dir_all(&dir).ok();
}
