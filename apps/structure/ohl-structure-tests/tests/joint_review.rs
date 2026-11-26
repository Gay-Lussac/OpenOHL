// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints left for review (O44 L31), end to end as OHL-Structure and
//! its command line give them, on an invented square body of single angles
//! (legs, face diagonals, horizontals, plan diagonals at two levels),
//! turned by Orient angles and laid out by the automatic layout: the joints whole and left add up to the
//! joints counted, each joint left in one group with its reasons; a tick
//! written with the joint survives saving and reading, a file of version 28
//! has none; a run laying the tower out again with other settings clears
//! the ticks of the joints whose layout changed and keeps the others; the
//! command line's CSV and text are the same list.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use ohl_structure_link::joint_review::{review_list, set_reviewed};
use ohl_structure_link::{BoltArea, LayoutSettings, RunAsk, lay_out};
use ohl_structures::LibraryItem;

/// An invented body: four legs over `levels` (z, half width), face
/// diagonals and horizontals, plan diagonals at the levels `plan`.
fn body(levels: &[(f64, f64)], plan: &[usize]) -> String {
    let mut s = String::from(
        "format = \"ohl-structure\"\nversion = 28\nid = \"B\"\nname = \"invented body\"\n\
         kind = \"lattice\"\n",
    );
    let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    for (i, (z, h)) in levels.iter().enumerate() {
        for (c, (sx, sy)) in corners.iter().enumerate() {
            let _ = write!(
                s,
                "\n[[node]]\nid = \"J{i}{c}\"\npoint = [\"{} m\", \"{} m\", \"{z} m\"]\n{}",
                sx * h,
                sy * h,
                if i == 0 { "support = \"fixed\"\n" } else { "" }
            );
        }
    }
    let mut member = |id: String, a: String, b: String, class: &str| {
        let _ = write!(
            s,
            "\n[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"EA-61x7\"\n\
             design = {{ class = \"{class}\" }}\n"
        );
    };
    for i in 0..levels.len() - 1 {
        for c in 0..4 {
            member(
                format!("L{i}{c}"),
                format!("J{i}{c}"),
                format!("J{}{c}", i + 1),
                "leg",
            );
            member(
                format!("D{i}{c}"),
                format!("J{i}{c}"),
                format!("J{}{}", i + 1, (c + 1) % 4),
                "bracing",
            );
        }
    }
    for i in 1..levels.len() {
        for c in 0..4 {
            member(
                format!("H{i}{c}"),
                format!("J{i}{c}"),
                format!("J{i}{}", (c + 1) % 4),
                "bracing",
            );
        }
        if plan.contains(&i) {
            member(
                format!("P{i}0"),
                format!("J{i}0"),
                format!("J{i}2"),
                "bracing",
            );
            member(
                format!("P{i}1"),
                format!("J{i}1"),
                format!("J{i}3"),
                "bracing",
            );
        }
    }
    s
}

/// The tower of these tests: five levels, plan diagonals at two.
pub fn tower() -> String {
    let levels: Vec<(f64, f64)> = (0..5)
        .map(|i| (3.0 * i as f64, 2.0 - 0.2 * i as f64))
        .collect();
    body(&levels, &[2, 4])
}

fn library() -> ohl_sections::SectionLibrary {
    ohl_sections::SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// The layout over the whole tower (bolts of 16 mm; the longest
/// pull-back `pull` m).
fn run(text: &str, lib: &ohl_sections::SectionLibrary, pull: f64) -> String {
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        pull_back: pull,
        ..Default::default()
    };
    let ask = RunAsk {
        library: lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    lay_out(text, &ask).unwrap().text
}

#[test]
fn the_joints_left_are_listed_ticked_and_cleared_by_a_change_of_their_layout() {
    let lib = library();
    // Turned as Orient angles turns a tower without orientation data, then
    // laid out.
    let oriented = ohl_structure_link::orient_angles(
        &tower(),
        &lib,
        &ohl_structure_link::OrientAsk {
            members: None,
            replace_hand: true,
            ..Default::default()
        },
    )
    .unwrap()
    .text;
    let laid = run(&oriented, &lib, LayoutSettings::default().pull_back);
    let list = review_list(&laid).unwrap();
    assert!(list.run);
    // Every joint here has two or more members.
    assert_eq!(list.total, 20, "{}", list.text());
    assert_eq!(list.whole + list.left(), list.total);
    assert!(list.whole > 0 && list.left() > 0, "{}", list.text());
    // Each joint left: in exactly one group; its reasons the record's.
    let t = ohl_structures::StructureType::parse(Path::new("b.ohls"), &laid).unwrap();
    let rec = ohl_structure_link::LayoutRecord::of(&t);
    let counted: usize = list.groups().iter().map(|g| g.1).sum();
    assert_eq!(counted, list.left());
    for r in &list.rows {
        match rec.hand.get(&r.joint) {
            Some(w) => assert_eq!(r.reasons.join("; "), *w),
            None => assert!(r.reasons.is_empty() && !rec.joints.contains(&r.joint)),
        }
    }
    // Every joint left ticked; saved and read again: all reviewed.
    let mut doc: toml_edit::DocumentMut = laid.parse().unwrap();
    let ticks: Vec<(String, Option<String>)> = list
        .rows
        .iter()
        .map(|r| (r.joint.clone(), Some(r.stamp.clone())))
        .collect();
    set_reviewed(&mut doc, &ticks).unwrap();
    let ticked = doc.to_string();
    assert!(ticked.contains("version = 29"), "the file raised to 29");
    let dir = scratch("ticks");
    let file = dir.join("b.ohls");
    std::fs::write(&file, &ticked).unwrap();
    let read = std::fs::read_to_string(&file).unwrap();
    let list2 = review_list(&read).unwrap();
    assert_eq!(list2.reviewed(), list.left());
    // The file before the ticks (version 28, as every older file): none
    // reviewed.
    assert!(!laid.contains("version = 29"));
    assert_eq!(list.reviewed(), 0);
    // The ticks change nothing else: the joints in detail, and a run on
    // the file, are as without them (the ticks aside).
    let lib_bolts = Default::default();
    assert_eq!(
        ohl_structure_link::joint_details(&ticked, &lib, &lib_bolts).unwrap(),
        ohl_structure_link::joint_details(&laid, &lib, &lib_bolts).unwrap()
    );
    let strip = |t: &str| {
        t.lines()
            .filter(|l| !l.starts_with("reviewed =") && !l.starts_with("version ="))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let pull = LayoutSettings::default().pull_back;
    assert_eq!(
        strip(&run(&ticked, &lib, pull)),
        strip(&run(&laid, &lib, pull))
    );
    // Laid out again with a shorter longest pull-back: the joints whose
    // layout changed lose their tick, the others keep it.
    let short = run(&ticked, &lib, 0.05);
    let list3 = review_list(&short).unwrap();
    let mut cleared = 0;
    for r in &list3.rows {
        let before = list.row(&r.joint).map(|x| x.stamp.clone());
        if before.as_deref() == Some(r.stamp.as_str()) {
            assert!(r.reviewed, "{} kept its layout: its tick kept", r.joint);
        } else {
            assert!(
                !r.reviewed,
                "{} laid out otherwise: its tick cleared",
                r.joint
            );
            cleared += usize::from(r.cleared);
        }
    }
    assert!(cleared > 0, "{}", list3.text());
    // The command line: the same list.
    let tool = export_tool();
    for (format, expected) in [
        ("joints-left", list2.csv()),
        ("joints-left-text", list2.text()),
    ] {
        let out = Command::new(&tool)
            .arg(&file)
            .args(["--format", format, "-o", "-"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    }
    let csv = dir.join("left.csv");
    let out = Command::new(&tool)
        .arg(&file)
        .args(["--format", "joints-left", "-o"])
        .arg(&csv)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_to_string(&csv).unwrap(), list2.csv());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&list2.summary()));
    std::fs::remove_dir_all(dir).ok();
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-joint-review-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `ohl-structure-export`, built with the tests.
fn export_tool() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let mut build = Command::new(env!("CARGO"));
    build.args([
        "build",
        "-q",
        "-p",
        "ohl-structure-export",
        "--bin",
        "ohl-structure-export",
    ]);
    if dir.file_name().is_some_and(|n| n == "release") {
        build.arg("--release");
    }
    if let Some(target) = dir.parent() {
        build.env("CARGO_TARGET_DIR", target);
    }
    assert!(build.status().expect("cargo runs").success());
    let tool = dir.join(format!(
        "ohl-structure-export{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(tool.is_file(), "{}", tool.display());
    tool
}
