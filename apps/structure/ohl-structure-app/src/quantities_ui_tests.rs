// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Design › Weight & quantities (joint details stage 4.6, 2026-11-19) on
//! an invented joint whose every member end has its connection: counted
//! in the background, the totals, the bill and the notes shown, what a
//! run takes said; "Dead-load factor only" keeps the factor (one undo
//! step, a file with a version raised to 19). With `OHL_RENDER_PNG` set, an image of
//! the window for the user guide.

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::layout::Panel;
use crate::tests::{harness_steps, menu};

/// A leg spliced at N2 and a member bolted onto a gusset plate, every end
/// with its connection (the example libraries beside it).
const JOINT: &str = r#"id = "W"
name = "weighed joint (invented)"
kind = "lattice"
[analysis]
nonlinear = false
dead_load_factor = 1.3
capacity = "simple"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 3]
[[node]]
id = "N3"
point = [2, 0, 3]
support = "fixed"
[[member]]
id = "A"
nodes = ["N1", "N2"]
section = "EA-61x7"
[[member]]
id = "C"
nodes = ["N2", "N3"]
section = "EA-61x7"
[[connection]]
member = "A"
joint = "N1"
kind = "fixed"
[[connection]]
member = "C"
joint = "N3"
kind = "fixed"
[[connection]]
member = "A"
joint = "N2"
kind = "fixed"
[[connection]]
member = "C"
joint = "N2"
onto = "G1"
leg = "long"
bolts = { count = 2, bolt = "X17-example" }
bolt_points = [["60 mm", "30 mm"], ["120 mm", "30 mm"]]
[[plate]]
id = "G1"
joint = "N2"
thickness = "10 mm"
outline = [["0 m", "0 m", "2.9 m"], ["0.3 m", "0 m", "2.9 m"], ["0.3 m", "0 m", "3.1 m"], ["0 m", "0 m", "3.1 m"]]
"#;

fn joint_file() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dir = std::env::temp_dir().join(format!("ohl-structure-quantities-{}", std::process::id()));
    for (sub, from) in [
        ("Sections", "libraries/sections/example-sections.ohlsec"),
        ("Bolts", "libraries/bolts/example-bolts.ohlb"),
    ] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        let name = Path::new(from).file_name().unwrap();
        std::fs::copy(root.join(from), dir.join(sub).join(name)).unwrap();
    }
    let path = dir.join("weighed.ohls");
    std::fs::write(&path, JOINT).unwrap();
    path
}

/// Steps frames until no background thread of the panel or the view is
/// working (its answer taken, two frames in a row — a thread starts in the
/// frame after an edit), then runs to rest: the threads each ask for a
/// frame when done, which under load arrived in the middle of a `run` and
/// ran it past its frames (O46: the test's flakiness). At most 30 s.
fn settle(h: &mut egui_kittest::Harness<'static, StructureApp>) {
    let start = std::time::Instant::now();
    let mut idle = 0;
    while idle < 2 {
        h.step();
        let busy = h.state().quantities.counting() || h.state().solids.working();
        idle = if busy { 0 } else { idle + 1 };
        assert!(start.elapsed().as_secs() < 30, "still working after 30 s");
        if busy {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    h.run();
}

/// Settles until the count is in (at most 30 s).
fn counted(h: &mut egui_kittest::Harness<'static, StructureApp>) {
    let start = std::time::Instant::now();
    loop {
        settle(h);
        if h.state().quantities.counted.is_some() {
            return;
        }
        assert!(start.elapsed().as_secs() < 30, "not counted");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn shows(h: &egui_kittest::Harness<'_, StructureApp>, words: &str) -> bool {
    h.query_all_by_label_contains(words).next().is_some()
}

#[test]
fn the_panel_counts_says_and_keeps_the_factor() {
    let path = joint_file();
    // (The menu's own runs may see the count's thread ask for a frame: a
    // few more frames allowed than the default four.)
    let mut h = harness_steps(&path, 16);
    menu(&mut h, "Design", "Weight & quantities");
    assert!(h.state().is_open(Panel::Quantities));
    counted(&mut h);
    let b = match &h.state().quantities.counted {
        Some((_, Ok(b), _)) => b.clone(),
        other => panic!("{:?}", other.as_ref().map(|o| &o.1)),
    };
    assert!(b.every_joint_defined(), "{:?}", b.undefined);
    assert!(b.total(ohl_bom::structure::PartKind::Plate).is_some());
    assert!(shows(
        &h,
        "Every joint is defined: a run takes the larger of the counted mass"
    ));
    assert!(shows(&h, "Bill of materials ("));
    assert!(shows(&h, "plate 10 mm"));
    assert!(shows(&h, "NOT ON THE SAFE SIDE"));
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = h.render().unwrap();
        image
            .save(Path::new(&dir).join("structure-quantities.png"))
            .unwrap();
    }
    // Dead-load factor only: the factor kept, one undo step.
    let before = h.state().file.as_ref().unwrap().text();
    h.get_by_label("Dead-load factor only").click();
    settle(&mut h);
    let text = h.state().file.as_ref().unwrap().text();
    assert!(text.contains("self_weight_from = \"factor\""), "{text}");
    assert!(shows(
        &h,
        "Every joint is defined, but a run keeps the members"
    ));
    h.state_mut().undo();
    settle(&mut h);
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
}
