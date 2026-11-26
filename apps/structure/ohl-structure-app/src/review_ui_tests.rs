// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Review the joints left (O44 L31): Design › Review the joints left…
//! opens the window on an invented body laid out by the automatic layout;
//! a click on a joint selects it and its members and opens it in the
//! Detail view; N and P step through the list; Reviewed, next ticks the
//! joint (stored with it, one undo step) and goes on; Lay out this joint… opens
//! its window; Escape closes that first, then this one, nothing changed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::review_window::REVIEW_TITLE;
use crate::tests::{copy_of, harness, menu};

/// An invented body of single angles: four legs over five levels, face
/// diagonals and horizontals, plan diagonals at two levels.
fn tower() -> String {
    let mut s = String::from(
        "format = \"ohl-structure\"\nversion = 28\nid = \"B\"\nname = \"invented body\"\n\
         kind = \"lattice\"\n",
    );
    let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    let levels: Vec<(f64, f64)> = (0..5)
        .map(|i| (3.0 * i as f64, 2.0 - 0.2 * i as f64))
        .collect();
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
        if [2, 4].contains(&i) {
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

/// The invented body, its legs oriented and five bracing members turned by
/// hand, laid out (the whole tower, bolts of 16 mm) and open,
/// its libraries beside it.
fn open(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let path = copy_of("type-02.ohls", tag);
    let dir = path.parent().unwrap().to_path_buf();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        root.join("libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        root.join("libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let lib = ohl_sections::SectionLibrary::load(
        &root.join("libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    let bolts = Default::default();
    let s = ohl_structure_link::LayoutSettings {
        bolt: Some(ohl_structure_link::BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let ask = ohl_structure_link::RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    // The legs turned as Orient angles turns them, five bracing members
    // given a roll by hand that lays no leg flat: joints left for a hand
    // detail, for several reasons.
    let legs: std::collections::BTreeSet<String> = (0..4)
        .flat_map(|i| (0..4).map(move |c| format!("L{i}{c}")))
        .collect();
    let oriented = ohl_structure_link::orient_angles(
        &tower(),
        &lib,
        &ohl_structure_link::OrientAsk {
            members: Some(legs),
            replace_hand: true,
            ..Default::default()
        },
    )
    .unwrap()
    .text;
    let mut doc: toml_edit::DocumentMut = oriented.parse().unwrap();
    for m in doc["member"].as_array_of_tables_mut().unwrap().iter_mut() {
        let id = m["id"].as_str().unwrap_or("").to_owned();
        if ["D11", "D22", "P20", "H31", "D02"].contains(&id.as_str()) {
            m.insert("roll", toml_edit::value("37 deg"));
        }
    }
    let ask = ohl_structure_link::RunAsk {
        replace_hand: false,
        ..ask
    };
    let laid = ohl_structure_link::lay_out(&doc.to_string(), &ask)
        .unwrap()
        .text;
    let file = dir.join("body.ohls");
    std::fs::write(&file, laid).unwrap();
    let mut h = harness(&file);
    h.run_steps(2);
    (h, dir)
}

/// The window's current joint.
fn current(h: &Harness<'static, StructureApp>) -> Option<String> {
    h.state()
        .review_window
        .as_ref()
        .and_then(|w| w.current.clone())
}

#[test]
fn the_joints_left_are_reviewed_one_after_the_other() {
    let (mut h, dir) = open("joints-left-window");
    menu(&mut h, "Design", "Review the joints left…");
    h.run_steps(3);
    assert!(h.state().review_window.is_some());
    let list = h.state_mut().review_list().unwrap();
    let list = list.as_ref().as_ref().unwrap().clone();
    assert!(list.left() >= 3, "{}", list.text());
    // The counts, said.
    assert!(
        h.query_all_by_label_contains(&list.summary())
            .next()
            .is_some()
    );
    // The window as it opens (the user guide's "before").
    crate::tool_window_tests::window_shot(&mut h, REVIEW_TITLE, "review-the-joints-left");
    let order: Vec<String> = list.by_group().iter().map(|r| r.joint.clone()).collect();
    // A click on the first joint: selected with its members, in the
    // Detail view.
    let first = order[0].clone();
    h.get_all_by_label_contains(&format!("{first}  ·"))
        .next()
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(current(&h).as_deref(), Some(first.as_str()));
    let st = h.state();
    assert_eq!(st.selected.last(), Some(&Handle::Joint(first.clone())));
    let row = list.row(&first).unwrap();
    for m in &row.members {
        assert!(st.selected.contains(&Handle::Member(m.clone())), "{m}");
    }
    assert_eq!(st.joints.joint.as_deref(), Some(first.as_str()));
    assert!(st.is_open(crate::layout::Panel::JointView));
    // N: the next in the list's order; P: back.
    h.key_press(eframe::egui::Key::N);
    h.run_steps(2);
    assert_eq!(current(&h).as_deref(), Some(order[1].as_str()));
    assert_eq!(
        h.state().selected.last(),
        Some(&Handle::Joint(order[1].clone()))
    );
    h.key_press(eframe::egui::Key::P);
    h.run_steps(2);
    assert_eq!(current(&h).as_deref(), Some(first.as_str()));
    // Reviewed, next: the tick stored with the joint (one undo step), the
    // next joint shown.
    let before = h.state().file.as_ref().unwrap().text();
    h.get_by_label("Reviewed, next ›").click();
    h.run_steps(3);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(
        f.undo_name(),
        Some(format!("Reviewed: joint {first}").as_str())
    );
    let t = f.structure();
    assert!(
        t.nodes
            .iter()
            .find(|n| n.id == first)
            .unwrap()
            .reviewed
            .is_some()
    );
    assert!(f.text().contains("version = 29"));
    assert_eq!(current(&h).as_deref(), Some(order[1].as_str()));
    // Enter: the same for the next one.
    h.key_press(eframe::egui::Key::Enter);
    h.run_steps(3);
    let list = h.state_mut().review_list().unwrap();
    let list = list.as_ref().as_ref().unwrap().clone();
    assert_eq!(list.reviewed(), 2, "{}", list.text());
    assert_eq!(current(&h).as_deref(), Some(order[2].as_str()));
    // The reviewed ones hidden (the user guide's "after").
    h.state_mut().review_window.as_mut().unwrap().hide_reviewed = true;
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains(&format!("{first}  ·"))
            .next()
            .is_none()
    );
    crate::tool_window_tests::window_shot(&mut h, REVIEW_TITLE, "review-the-joints-left-after");
    // P from the third goes past the hidden ones: none before.
    h.key_press(eframe::egui::Key::P);
    h.run_steps(2);
    assert_eq!(current(&h).as_deref(), Some(order[2].as_str()));
    // Undo takes the last tick back.
    h.state_mut().file.as_mut().unwrap().undo();
    h.run_steps(2);
    let list = h.state_mut().review_list().unwrap();
    assert_eq!(list.as_ref().as_ref().unwrap().reviewed(), 1);
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn lay_out_joint_opens_from_the_list_and_escape_closes_that_first() {
    let (mut h, dir) = open("joints-left-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_review_window(None);
    h.run_steps(2);
    h.key_press(eframe::egui::Key::N);
    h.run_steps(2);
    let j = current(&h).expect("a joint shown");
    h.get_by_label("Lay out this joint…").click();
    h.run_steps(2);
    let scope = h.state().layout_window.as_ref().map(|w| w.scope.clone());
    assert_eq!(scope, Some(crate::layout_window::Scope::Joint(j)));
    // Escape: Lay out joints closed, the list still open.
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().layout_window.is_none());
    assert!(h.state().review_window.is_some());
    // Escape again: the list closed; nothing changed.
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().review_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn joint_details_ticks_a_joint_left_and_opens_the_list() {
    let (mut h, dir) = open("joints-left-details");
    let list = h.state_mut().review_list().unwrap();
    let j = list.as_ref().as_ref().unwrap().rows[0].joint.clone();
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint(j.clone())];
    h.run_steps(3);
    h.get_by_label("Reviewed").click();
    h.run_steps(2);
    let list = h.state_mut().review_list().unwrap();
    assert!(list.as_ref().as_ref().unwrap().row(&j).unwrap().reviewed);
    h.get_by_label("Review the joints left…").click();
    h.run_steps(2);
    assert_eq!(current(&h).as_deref(), Some(j.as_str()));
    std::fs::remove_dir_all(dir).ok();
}
