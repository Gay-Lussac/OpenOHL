// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl drawing` on the typed example: the structures of type-02 (level
//! 3, 178 members) are drawn with their members on the layer
//! `OHL_MEMBERS` in the DXF and SVG; the others with their symbol.

use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

#[test]
fn level_3_structures_are_drawn_with_their_members() {
    let dir = std::env::temp_dir().join(format!("ohl-drawing-members-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(example("synthetic-line-typed.ohl.json"))
        .arg("--structures")
        .arg(example("Structures"))
        .arg("--route")
        .arg("main")
        .arg("--format")
        .arg("svg,dxf")
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let svg = std::fs::read_to_string(dir.join("main.svg")).unwrap();
    let dxf = std::fs::read_to_string(dir.join("main.dxf")).unwrap();
    // T2 and T4 (type-02) on the main route: 178 members each, in the
    // plan and the profile.
    let members = svg.matches(r#"class="OHL_MEMBERS""#).count();
    assert_eq!(members, 2 * 2 * 178, "{members}");
    let entities = dxf.matches("\n  8\nOHL_MEMBERS\n").count();
    assert!(entities >= members, "{entities}");
    // No elevations unless asked for.
    assert!(!svg.contains("OHL_ELEVATIONS"));
    std::fs::remove_dir_all(&dir).ok();
}

/// T6 (type-05) names the example's `.ohlp` pole (level 3): on the branch
/// it is drawn with its outline on OHL_MEMBERS (the silhouette in the
/// profile, the outlines at the ground line and the top in the plan);
/// `--elevations above` adds every structure's elevation on
/// OHL_ELEVATIONS.
#[test]
fn a_pole_is_drawn_with_its_outline_and_elevations_on_request() {
    let dir = std::env::temp_dir().join(format!("ohl-drawing-pole-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(example("synthetic-line-typed.ohl.json"))
        .arg("--structures")
        .arg(example("Structures"))
        .arg("--route")
        .arg("branch")
        .arg("--format")
        .arg("svg,dxf")
        .arg("--elevations")
        .arg("above")
        .arg("--elevation-offset")
        .arg("8")
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let svg = std::fs::read_to_string(dir.join("branch.svg")).unwrap();
    let dxf = std::fs::read_to_string(dir.join("branch.dxf")).unwrap();
    // The pole: its silhouette titled T6 and two outlines in the plan.
    let pole = svg
        .lines()
        .filter(|l| l.contains(r#"class="OHL_MEMBERS""#))
        .count();
    assert!(pole >= 3, "{pole}");
    assert!(svg.contains("<title>T6</title>"));
    // Elevations: one per structure of the route, titled.
    assert!(
        svg.contains("<title>T6 elevation</title>"),
        "no T6 elevation"
    );
    assert!(dxf.contains("\n  8\nOHL_ELEVATIONS\n"));
    // A bad place is refused.
    let bad = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(example("synthetic-line-typed.ohl.json"))
        .arg("--elevations")
        .arg("sideways")
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("off, above or below"));
    std::fs::remove_dir_all(&dir).ok();
}
