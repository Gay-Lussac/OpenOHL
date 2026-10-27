// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl project import-into`.
//!
//! Add mode: the example line split at its junction tower T3 — the
//! project the main route with T3 as a stub, the import (a bundle of the
//! example project plugin) the branch with T3 in full and its wire file.
//! The dry run prints the tower in both with its sets and writes nothing;
//! the import adds the branch's set to the project's T3 (as chosen), its
//! section and jumper, and its wire file (the project has none); the wire
//! library loads and `ohl line` solves every section.
//!
//! Replace mode (the default): a variant of a stretch of a straight line
//! replaces it; the dry run lists what it removes, a removal can be kept,
//! the merged line solves.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ohl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_branch_is_imported_into_its_main_line() {
    let plugin = root().join("plugins/examples/project-bundle");
    if !plugin
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "project-bundle plugin not built"
        );
        eprintln!("skipping: project-bundle plugin not built");
        return;
    }
    let dir = std::env::temp_dir().join(format!("ohl-import-into-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let full =
        ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap();
    // The project: the main route, T3 without the branch's set 2.
    let mut main = full.clone();
    main.routes.retain(|r| r.id == "main");
    main.alignments.retain(|a| a.route == "main");
    let points = main.alignments[0].points.clone();
    main.alignment_points.retain(|p| points.contains(&p.id));
    main.structures.retain(|s| s.id != "T6" && s.id != "T7");
    main.sections.retain(|s| s.id != "S3");
    main.jumpers.clear();
    let t3 = main.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    t3.wires.retain(|w| w.set != Some(2));
    t3.strings.retain(|s| s.wire != "W2");
    t3.points.retain(|p| p.label != "A2");
    let project = dir.join("main.ohl");
    let mut s = ohl_session::Session::open(&project).unwrap();
    s.import(&main, "tester").unwrap();
    drop(s);
    // The import: the branch, T3 in full, the wire file.
    let mut branch = full;
    branch.routes.retain(|r| r.id == "branch");
    branch.alignments.retain(|a| a.route == "branch");
    let points = branch.alignments[0].points.clone();
    branch.alignment_points.retain(|p| points.contains(&p.id));
    branch
        .structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    branch.sections.retain(|s| s.id == "S3");
    let wires =
        std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw")).unwrap();
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(&branch.to_json()).unwrap(),
        "libraries": [{"kind": "cables", "name": "cables.ohlw", "text": wires}],
    });
    std::fs::write(dir.join("branch.ohlb"), bundle.to_string()).unwrap();
    let plugin = plugin.display().to_string();
    let args = [
        "project",
        "import-into",
        "branch.ohlb",
        "--into",
        "main.ohl",
        "--user",
        "tester",
        "--plugin",
        &plugin,
        "--mode",
        "add",
    ];

    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let out = ohl(&dir, &dry);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("towers in both"), "{t}");
    assert!(
        t.contains("T3: 2 sets in the project's, 3 in the imported"),
        "{t}"
    );
    assert!(t.contains("[tower:T3:set:2]") && t.contains(": add"), "{t}");
    assert!(t.contains("dry run: nothing written"), "{t}");
    let unchanged = ohl_session::Session::open(&project).unwrap().export();
    assert_eq!(unchanged.sections.len(), 2);

    let mut go = args.to_vec();
    go.extend(["--choose", "tower:T3:set:2=add"]);
    let out = ohl(&dir, &go);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("library file written: cables/cables.ohlw"),
        "{t}"
    );
    assert!(t.contains("the wire library loads: 1 cables"), "{t}");
    let s = ohl_session::Session::open(&project).unwrap();
    let line = s.export();
    assert!(line.structure("T3").unwrap().wire_at_set(2).is_some());
    assert_eq!(line.sections.len(), 3);
    assert_eq!(line.jumpers.len(), 1);
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    drop(s);
    // Again: everything the same, no second wire file.
    let out = ohl(&dir, &go);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("library file written"), "{t}");
    assert!(t.contains("the wire library loads: 1 cables"), "{t}");
    let s = ohl_session::Session::open(&project).unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    assert_eq!(s.export().sections.len(), 3);
    drop(s);
    let out = ohl(&dir, &["line", "main.ohl"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("solved 3 of 3 sections"), "{t}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A tower of the straight line: one string per set (1 and 21 left, 11
/// and 31 right, 21 and 31 half a metre ahead), world positions.
fn tower(id: &str, x: f64, y: f64, z: f64, sets: &[u32]) -> serde_json::Value {
    let (mut points, mut strings, mut wires) = (vec![], vec![], vec![]);
    for &n in sets {
        let side = if n % 20 == 1 { -6.0 } else { 6.0 };
        let ahead = if n > 20 { 0.5 } else { 0.0 };
        points.push(serde_json::json!({
            "label": format!("A{n}"), "position": [x + ahead, y + side, z + 30.0]
        }));
        strings.push(serde_json::json!({
            "label": format!("S{n}"), "top": format!("A{n}"), "wire": format!("W{n}"),
            "length": 3.5, "weight": 900.0
        }));
        wires.push(serde_json::json!({
            "label": format!("W{n}"), "position": [x + ahead, y + side, z + 26.5], "set": n
        }));
    }
    serde_json::json!({
        "id": id, "position": [x, y, z], "orientation_deg": -90.0,
        "points": points, "strings": strings, "wires": wires
    })
}

/// A line of the example's header and system: towers, sections of
/// circuits A (sets 1/21) and B (11/31) over structures (the ahead set at
/// a strain tower first), jumpers at strain towers (`AT5`: circuit A at T5); one
/// route through a point tied to each tower.
fn straight(
    route: &str,
    towers: Vec<serde_json::Value>,
    sections: &[(&str, &[&str])],
    jumpers: &[&str],
) -> ohl_line::Line {
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap(),
    )
    .unwrap();
    let points: Vec<serde_json::Value> = towers
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": format!("{route}-{}", s["id"].as_str().unwrap()),
                "position": s["position"], "structure": s["id"]
            })
        })
        .collect();
    let mut secs = Vec::new();
    for (c, (b, a)) in [("A", (1, 21)), ("B", (11, 31))] {
        for (id, over) in sections {
            let pts: Vec<serde_json::Value> = over
                .iter()
                .enumerate()
                .map(|(k, s)| {
                    let ahead = k == 0 && ["T3", "T5", "T8"].contains(s);
                    serde_json::json!({"structure": s, "set": if ahead { a } else { b }})
                })
                .collect();
            secs.push(serde_json::json!({
                "id": format!("{c}{id}"), "cable": "acsr-drake-26-7", "circuit_name": c,
                "phase_label": if c == "A" { "L1" } else { "L2" }, "system": "ex220",
                "points": pts,
                "stringing": {"kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0}
            }));
        }
    }
    v["routes"] = serde_json::json!([{"id": route, "name": route}]);
    v["alignments"] = serde_json::json!([{
        "id": route, "name": route, "route": route,
        "points": points.iter().map(|p| p["id"].clone()).collect::<Vec<_>>()
    }]);
    v["alignment_points"] = serde_json::Value::Array(points);
    v["structures"] = serde_json::Value::Array(towers);
    v["sections"] = serde_json::Value::Array(secs);
    v["jumpers"] = serde_json::Value::Array(
        jumpers
            .iter()
            .map(|j| {
                let (c, at) = j.split_at(1);
                let (b, a) = if c == "A" { (1, 21) } else { (11, 31) };
                serde_json::json!({
                    "id": format!("J{j}"), "cable": "acsr-drake-26-7", "sag": 1.5,
                    "temperature": 15.0,
                    "from": {"structure": at, "set": b}, "to": {"structure": at, "set": a}
                })
            })
            .collect(),
    );
    let line: ohl_line::Line = serde_json::from_value(v).unwrap();
    line.validate().unwrap();
    line
}

/// The project: T1…T10 300 m apart and T6b between T6 and T7, strain
/// towers T3, T5, T8, two circuits; the variant covers T3–T8 with V6 at
/// T6's place and V7 13 m beside T7, without T6b and T7, and only the
/// jumper of circuit A at T5. The dry run lists the removals (T6b, T7,
/// the sections over them, the jumper of B at T5) and writes nothing;
/// Add mode removes nothing; the import with that jumper kept writes the
/// variant into the line, which solves.
#[test]
fn a_variant_replaces_its_stretch() {
    let plugin = root().join("plugins/examples/project-bundle");
    if !plugin
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "project-bundle plugin not built"
        );
        eprintln!("skipping: project-bundle plugin not built");
        return;
    }
    let dir = std::env::temp_dir().join(format!("ohl-import-variant-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let x = |k: f64| 300.0 * (k - 1.0);
    let all = [1, 11, 21, 31];
    let ours = straight(
        "main",
        vec![
            tower("T1", x(1.0), 0.0, 500.0, &[1, 11]),
            tower("T2", x(2.0), 0.0, 500.0, &[1, 11]),
            tower("T3", x(3.0), 0.0, 500.0, &all),
            tower("T4", x(4.0), 0.0, 500.0, &[1, 11]),
            tower("T5", x(5.0), 0.0, 500.0, &all),
            tower("T6", x(6.0), 0.0, 500.0, &[1, 11]),
            tower("T6b", x(6.5), 0.0, 500.0, &[1, 11]),
            tower("T7", x(7.0), 0.0, 500.0, &[1, 11]),
            tower("T8", x(8.0), 0.0, 500.0, &all),
            tower("T9", x(9.0), 0.0, 500.0, &[1, 11]),
            tower("T10", x(10.0), 0.0, 500.0, &[1, 11]),
        ],
        &[
            ("0", &["T1", "T2", "T3"]),
            ("1", &["T3", "T4", "T5"]),
            ("2", &["T5", "T6", "T6b", "T7", "T8"]),
            ("3", &["T8", "T9", "T10"]),
        ],
        &["AT3", "BT3", "AT5", "BT5", "AT8", "BT8"],
    );
    let theirs = straight(
        "variant",
        vec![
            tower("T3", x(3.0), 0.0, 500.0, &all),
            tower("T4", x(4.0), 0.0, 501.0, &[1, 11]),
            tower("T5", x(5.0), 0.0, 500.0, &all),
            tower("V6", x(6.0), 0.0, 500.0, &[1, 11]),
            tower("V7", x(7.0), 13.0, 500.0, &[1, 11]),
            tower("T8", x(8.0), 0.0, 500.0, &all),
        ],
        &[("1", &["T3", "T4", "T5"]), ("2", &["T5", "V6", "V7", "T8"])],
        &["AT5"],
    );
    let project = dir.join("line.ohl");
    let mut s = ohl_session::Session::open(&project).unwrap();
    s.import(&ours, "tester").unwrap();
    drop(s);
    let wires =
        std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw")).unwrap();
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(&theirs.to_json()).unwrap(),
        "libraries": [{"kind": "cables", "name": "cables.ohlw", "text": wires}],
    });
    std::fs::write(dir.join("variant.ohlb"), bundle.to_string()).unwrap();
    let plugin = plugin.display().to_string();
    let args = [
        "project",
        "import-into",
        "variant.ohlb",
        "--into",
        "line.ohl",
        "--user",
        "tester",
        "--plugin",
        &plugin,
    ];

    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let out = ohl(&dir, &dry);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("mode: Replace what the import covers"), "{t}");
    assert!(t.contains("route variant: the project's route main"), "{t}");
    assert!(
        t.contains("removed: structures T6b, T7; sections A2, B2; jumpers JBT5"),
        "{t}"
    );
    assert!(t.contains("[remove:jumper:JBT5]"), "{t}");
    assert!(t.contains("structure (the project's) T6 is now V6"), "{t}");
    assert!(t.contains("dry run: nothing written"), "{t}");
    let before = ohl_session::Session::open(&project).unwrap().export();
    assert_eq!(before.structures.len(), 11);

    let mut add = dry.clone();
    add.extend(["--mode", "add"]);
    let t = text(&ohl(&dir, &add));
    assert!(t.contains("mode: Add to the project"), "{t}");
    assert!(!t.contains("removed:"), "{t}");

    let mut go = args.to_vec();
    go.extend(["--choose", "remove:jumper:JBT5=keep"]);
    let out = ohl(&dir, &go);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let line = ohl_session::Session::open(&project).unwrap().export();
    let mut ids: Vec<&str> = line.structures.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["T1", "T10", "T2", "T3", "T4", "T5", "T8", "T9", "V6", "V7"]
    );
    assert!(line.jumpers.iter().any(|j| j.id == "JBT5"));
    assert_eq!(line.sections.len(), 8);
    let out = ohl(&dir, &["line", "line.ohl"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("solved 8 of 8 sections"), "{t}");
    std::fs::remove_dir_all(&dir).ok();
}
