// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Apply this joint's detail to similar joints (J4, 2026-11-19): the
//! window's preview names the joints like it and what changes there;
//! Apply is one undo step; an unticked joint is left; Escape changes
//! nothing. With `OHL_RENDER_PNG` set, the window's picture for the user
//! guide.

use std::collections::BTreeMap;
use std::fmt::Write;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::tests::harness;

/// An invented body of four legs (face diagonals turning round it) with
/// its angles oriented from its shape and joint J10 laid out, the example
/// sections beside it.
fn open(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dir = std::env::temp_dir().join(format!("ohl-structure-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let sections = root.join("libraries/sections/example-sections.ohlsec");
    std::fs::copy(&sections, dir.join("Sections/example-sections.ohlsec")).unwrap();
    let lib = ohl_structure_link::SectionLibrary::load(&sections).unwrap();
    let mut s =
        String::from("id = \"B\"\nname = \"symmetric body (invented)\"\nkind = \"lattice\"\n");
    let levels = [(0.0, 2.0), (3.0, 1.8), (6.0, 1.6)];
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
             kind = \"frame\"\ndesign = {{ class = \"{class}\" }}\n"
        );
    };
    for i in 0..2 {
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
    for i in 1..3 {
        for c in 0..4 {
            member(
                format!("H{i}{c}"),
                format!("J{i}{c}"),
                format!("J{i}{}", (c + 1) % 4),
                "bracing",
            );
        }
    }
    let oriented = ohl_structure_link::orient_angles(
        &s,
        &lib,
        &ohl_structure_link::OrientAsk {
            members: None,
            replace_hand: true,
        },
    )
    .unwrap()
    .text;
    let settings = ohl_structure_link::LayoutSettings {
        bolt: Some(ohl_structure_link::BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let bolts = Default::default();
    let laid = ohl_structure_link::lay_out(
        &oriented,
        &ohl_structure_link::RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &settings,
            preset_fields: BTreeMap::new(),
            joints: Some(["J10".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap()
    .text;
    let tower = dir.join("body.ohls");
    std::fs::write(&tower, laid).unwrap();
    (harness(&tower), dir)
}

/// Runs frames until the window's preview is there (at most ~30 s).
fn wait_preview(h: &mut Harness<'static, StructureApp>) {
    for _ in 0..600 {
        h.step();
        if h.state()
            .apply_window
            .as_ref()
            .is_some_and(|w| w.preview.is_some())
        {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no preview");
}

#[test]
fn a_joints_detail_goes_to_the_joints_like_it_in_one_undo_step() {
    let (mut h, dir) = open("apply-detail");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().apply_window = Some(crate::apply_window::ApplyWindow::new("J10"));
    h.step();
    wait_preview(&mut h);
    {
        let w = h.state().apply_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let to: Vec<&str> = p.all.applied.iter().map(|a| a.joint.as_str()).collect();
        assert_eq!(to, ["J11", "J12", "J13"], "{:?}", p.all.left);
    }
    assert!(h.query_by_label_contains("J12 — turned").is_some());
    crate::tool_window_tests::window_shot(
        &mut h,
        "Apply joint J10's detail to similar joints",
        "apply-detail",
    );
    // Nothing written before Apply; J13 unticked: left.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.state_mut()
        .apply_window
        .as_mut()
        .unwrap()
        .unticked
        .insert("J13".into());
    h.step();
    for _ in 0..600 {
        h.step();
        if h.query_by_label_contains("Apply (2 joints)").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    h.get_by_label_contains("Apply (2 joints)").click();
    h.run_steps(2);
    assert!(h.state().apply_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert!(f.undo_name().unwrap().starts_with("Joint J10's detail"));
    let t = f.structure();
    assert!(t.connection("D12", "J12").is_some_and(|c| c.cut.is_some()));
    assert!(t.connection("D13", "J13").is_none());
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn escape_closes_the_apply_window_and_changes_nothing() {
    let (mut h, dir) = open("apply-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().apply_window = Some(crate::apply_window::ApplyWindow::new("J10"));
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().apply_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}
