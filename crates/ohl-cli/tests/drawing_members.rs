// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
    std::fs::remove_dir_all(&dir).ok();
}
