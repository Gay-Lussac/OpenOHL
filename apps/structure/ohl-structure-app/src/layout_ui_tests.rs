// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Lay out joints (2026-11-18): Joint details › Lay out joint… opens the
//! window, its preview is worked out on a copy, Apply is one undo step,
//! the joint shows what is automatic, and a later edit near it is
//! followed in the same undo step.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::joint_ui_tests::ANGLES;
use crate::tests::{copy_of, harness, menu};

/// The angles fixture (D1 and H1 running into each other at N2) open,
/// Joint details at N2.
fn open(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let path = copy_of("type-02.ohls", tag);
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    // D1 and H1 turned half a turn: on the outer face of the leg (as
    // written they lie on its inner face, where no plate is laid out).
    let text = ANGLES
        .replace(
            "orientation = \"y-z+\"\ndesign = { class = \"bracing\" }",
            "orientation = \"y-z+\"\nroll = \"180 deg\"\ndesign = { class = \"bracing\" }",
        )
        .replace(
            "orientation = \"z+y-\"\n",
            "orientation = \"z+y-\"\nroll = \"180 deg\"\n",
        );
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, text).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    (h, dir)
}

/// Runs frames until the window's preview is there (at most ~30 s).
fn wait_preview(h: &mut Harness<'static, StructureApp>) {
    for _ in 0..600 {
        h.step();
        let ready = h
            .state()
            .layout_window
            .as_ref()
            .is_some_and(|w| w.preview.is_some());
        if ready {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no preview");
}

#[test]
fn a_joint_is_laid_out_from_its_details_in_one_undo_step() {
    let (mut h, dir) = open("layout-window");
    let before = h.state().file.as_ref().unwrap().text();
    // D1 and H1 run into each other at N2: a clash, said.
    let d = h.state_mut().joint_details().unwrap();
    let clashes = d
        .as_ref()
        .as_ref()
        .unwrap()
        .joint("N2")
        .unwrap()
        .clashes
        .len();
    assert!(clashes > 0);
    h.get_by_label("Lay out joint…").click();
    h.step();
    assert!(h.state().layout_window.is_some());
    wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let n2 = p.run.joints.iter().find(|o| o.joint == "N2").unwrap();
        assert!(n2.written, "{:?}", n2.hand);
        assert!(p.run.clashes.1 < p.run.clashes.0, "{:?}", p.run.clashes);
    }
    // The window's picture for the user guide (OHL_RENDER_PNG).
    crate::tool_window_tests::window_shot(&mut h, "Lay out joints", "lay-out-joints");
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        // The joint before and after, scrolled into view.
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-before-after",
        );
    }
    // Nothing written before Apply.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().layout_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_ne!(f.text(), before);
    assert!(f.undo_name().unwrap().starts_with("Lay out joints"));
    let rec = ohl_structure_link::LayoutRecord::of(f.structure());
    assert!(rec.joints.contains("N2"));
    let d = h.state_mut().joint_details().unwrap();
    let after = d
        .as_ref()
        .as_ref()
        .unwrap()
        .joint("N2")
        .unwrap()
        .clashes
        .len();
    assert!(after < clashes, "{after} {clashes}");
    // Joint details says what is automatic.
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("laid out automatically")
            .next()
            .is_some()
    );
    // One undo step takes it all back.
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn escape_closes_the_window_and_changes_nothing() {
    let (mut h, dir) = open("layout-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Joint("N2".into()));
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().layout_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn an_edit_near_a_laid_out_joint_is_followed_in_the_same_undo_step() {
    let (mut h, dir) = open("layout-follow");
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Joint("N2".into()));
    wait_preview(&mut h);
    // A few frames more: Apply is enabled once the preview is the
    // window's (its key and the file's revision).
    h.run_steps(4);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let laid = h.state().file.as_ref().unwrap().text();
    let steps = h.state().file.as_ref().unwrap().undo_steps();
    // H1's section changed: N2 is laid out again with it, in that edit's
    // undo step; the message says so.
    h.state_mut()
        .file
        .as_mut()
        .unwrap()
        .edit("Section of H1", |doc, _| {
            ohl_structure_edit::edits::set_member_section(doc, "H1", Some("EA-61x7"))
        });
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.undo_steps(), steps + 1);
    let msg = h.state().message.clone().map(|m| m.0).unwrap_or_default();
    assert!(
        msg.contains("laid out again") || msg.contains("needs a hand detail"),
        "{msg}"
    );
    // Undo: the layout as it was.
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), laid);
    std::fs::remove_dir_all(dir).ok();
}

/// J2 (2026-11-19): a joint not laid out whole names the members left —
/// the rest laid out — and those whose place is kept free; a joint left
/// whole says so.
#[test]
fn the_hand_list_names_the_members_left_and_the_place_kept() {
    let plan = ohl_structure_link::JointPlan {
        joint: "N2".into(),
        hand_members: vec!["R1".into(), "R2".into()],
        kept_free: vec!["R1".into()],
        hand: vec!["R1: too short".into(), "R2: no room".into()],
        ..Default::default()
    };
    let mut o = ohl_structure_link::JointOutcome {
        joint: "N2".into(),
        hand: plan.hand.clone(),
        plan,
        written: true,
        turned: Vec::new(),
        plates: Vec::new(),
    };
    assert_eq!(
        crate::layout_window::hand_words(&o),
        "N2: by hand R1 (its place kept free), R2, the rest laid out — R1: too short; R2: no room"
    );
    o.written = false;
    assert_eq!(
        crate::layout_window::hand_words(&o),
        "N2: left whole — R1: too short; R2: no room"
    );
}
