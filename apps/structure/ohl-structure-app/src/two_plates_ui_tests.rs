// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Two plates on one member end (O44 L33, O49 stage 1b; built, in active
//! development, not approved yet) in the application: on the invented
//! angle tower of the joint tests, its diagonal D1 pulled by a load case of
//! the file's own and its bolts given a small shear capacity, Lay out
//! joints at N2 gives D1's end a second plate (the run's note says so),
//! Apply writes it in one undo step; the second plate, its packing and its
//! own bolts are drawn in the 3D view and the Detail tab, and Joint
//! details' sketch says the two shear planes. With `OHL_RENDER_PNG`, the
//! user guide's pictures.

use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::joint_ui_tests::ANGLES;
use crate::tests::{copy_of, harness, menu};

/// The angle tower with D1 and H1 on the leg's outer face (laid out onto a
/// plate; D1's bolts on a 50 mm gauge and its steel stopping 260 mm from
/// the work point: room for a second plate's own bolts clear of the
/// first plate's onto the leg),
/// held so that a pull at N2 along −x goes into D1 (about 16 kN of
/// tension); D1's bolts of 4 kN per shear plane (three at N2: 12 kN on one
/// plate, 24 kN between two; eight at its foot N4); the load case "pull".
fn tower() -> String {
    ANGLES
        .replace(
            "orientation = \"y-z+\"\ndesign = { class = \"bracing\" }",
            "orientation = \"y-z+\"\nroll = \"180 deg\"\ndesign = { class = \"bracing\", \
             bolts = { diameter = \"16 mm\", hole = \"17.5 mm\", count = 3, shear_capacity = \
             \"4 kN\" } }",
        )
        .replace(
            "orientation = \"z+y-\"\n",
            "orientation = \"z+y-\"\nroll = \"180 deg\"\n",
        )
        .replace(
            "id = \"N2\"\npoint = [\"0 m\", \"1 m\", \"5 m\"]\n",
            "id = \"N2\"\npoint = [\"0 m\", \"1 m\", \"5 m\"]\nsupport = [\"y\"]\n",
        )
        .replace(
            "id = \"N3\"\npoint = [\"0 m\", \"1 m\", \"10 m\"]\n",
            "id = \"N3\"\npoint = [\"0 m\", \"1 m\", \"10 m\"]\nsupport = \"fixed\"\n",
        )
        .replace(
            "id = \"N5\"\npoint = [\"4 m\", \"1 m\", \"5 m\"]\n",
            "id = \"N5\"\npoint = [\"4 m\", \"1 m\", \"5 m\"]\nsupport = [\"y\"]\n",
        )
        .replace("grade = \"8.8\" }", "}")
        .replace(
            "gauges = [\"30 mm\"]",
            "gauges = [\"50 mm\"]\ncut = \"260 mm\"",
        )
        + "\n[[connection]]\nmember = \"D1\"\njoint = \"N4\"\nleg = \"long\"\n\
           bolts = { count = 8, diameter = \"16 mm\" }\n\
           \n[[point]]\nlabel = \"P2\"\nnode = \"N2\"\n\
           \n[[load_case]]\nname = \"pull\"\nself_weight = 0\n\
           \n[[load_case.load]]\nvalue = \"10 kN\"\ndirection = \"-x\"\npoints = [\"P2\"]\n"
}

#[test]
fn a_second_plate_is_laid_out_drawn_and_said() {
    let path = copy_of("type-02.ohls", "two-plates-ui");
    let dir = path.parent().unwrap().to_path_buf();
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../libraries");
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        examples.join("sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        examples.join("bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower_path = dir.join("two-plates.ohls");
    std::fs::write(&tower_path, tower()).unwrap();
    let mut h = harness(&tower_path);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    let before = h.state().file.as_ref().unwrap().text();
    h.get_by_label("Lay out joint…").click();
    h.step();
    crate::layout_ui_tests::wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        assert!(
            p.run
                .notes
                .iter()
                .any(|n| n.contains("given a second plate") && n.contains("D1 at N2")),
            "{:#?}",
            p.run.notes
        );
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        // Turned to look from the leg's inner side: the second plate in
        // front of the first, D1 between them.
        if let Some(w) = h.state_mut().layout_window.as_mut() {
            for v in [&mut w.views.0, &mut w.views.1] {
                v.yaw -= 2.3;
                v.pitch += 0.5;
                v.zoom = 1.0;
            }
        }
        h.event(eframe::egui::Event::PointerMoved(eframe::egui::pos2(
            1290.0, 890.0,
        )));
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-two-plates",
        );
    }
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    crate::layout_ui_tests::wait_apply(&mut h);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert!(f.undo_name().unwrap().starts_with("Lay out joints"));
    let second = f
        .structure()
        .plates
        .iter()
        .find(|p| p.paired.as_ref().is_some_and(|q| q.member == "D1"))
        .map(|p| p.id.clone())
        .expect("a second plate written");
    // Joint details: D1's end in two shear planes, the second plate drawn.
    let d = h.state_mut().joint_details().unwrap();
    let j = d.as_ref().as_ref().unwrap().joint("N2").unwrap().clone();
    let g2 = j.plates.iter().find(|p| p.id == second).unwrap();
    assert!(
        g2.paired.as_ref().is_some_and(|q| q.bolts.len() >= 2),
        "{g2:?}"
    );
    let a = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let shown = crate::joint_sketch::Shown::of(a, "N2");
    assert_eq!(
        shown.planes.as_ref().map(|p| p.0),
        Some(2),
        "{:?}",
        a.planes
    );
    // The sketch with D1's values: a bolt of its end picked.
    h.state_mut().joints.picked = Some(crate::joint_panel::Picked::Bolt {
        member: "D1".into(),
        joint: "N2".into(),
        index: 0,
    });
    h.run_steps(2);
    // The Detail view it opens turned so too.
    h.state_mut().joints.yaw = -2.3;
    h.state_mut().joints.pitch = 0.4;
    h.state_mut().joints.zoom = 0.7;
    h.get_by_label("What the values mean").click();
    h.run_steps(2);
    h.get_by_label_contains("with the values of D1")
        .scroll_to_me();
    h.run_steps(2);
    h.event(eframe::egui::Event::PointerMoved(eframe::egui::pos2(
        1290.0, 890.0,
    )));
    crate::tool_window_tests::panel_shot(&mut h, "two-plates-sketch");
    h.state_mut().joints.picked = None;
    if h.state().view_camera().is_some() && ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        // The 3D view on D1's end at N2, from the side of the second plate.
        let ctx = h.ctx.clone();
        let _ = h.state_mut().wait_for_solids(&ctx);
        h.state_mut().focus(crate::layout::Panel::View3d);
        h.run_steps(2);
        h.state_mut().set_view_camera(ohl_render::Camera {
            target: a.bolt_points[0],
            distance: 0.6,
            yaw: 120f64.to_radians(),
            pitch: 30f64.to_radians(),
            fov_y: 45f64.to_radians(),
            exaggeration: 1.0,
            ortho: false,
        });
        h.run_steps(4);
        crate::tool_window_tests::panel_shot(&mut h, "two-plates-3d");
        // The Detail tab on N2.
        h.state_mut().open_in_detail(Handle::Joint("N2".into()));
        h.state_mut().focus(crate::layout::Panel::JointView);
        h.state_mut().joints.yaw = -2.3;
        h.state_mut().joints.pitch = 0.4;
        h.state_mut().joints.zoom = 0.8;
        h.run_steps(4);
        crate::tool_window_tests::panel_shot(&mut h, "two-plates-detail");
    }
    // One undo step takes it back.
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

/// O49 stage 1c (2026-11-27, the owner's answer to stage 1b's question 4):
/// the second plate's room. The same tower with D1's steel stopping near
/// the joint: its own bolts nearer the joint would fall on the first
/// plate's leg bolts, so those become its own (one bolt group), or it goes
/// beside D1's bolts — the run's note says which. The window's picture
/// (`OHL_RENDER_PNG`).
#[test]
fn a_second_plate_finds_room() {
    let path = copy_of("type-02.ohls", "two-plates-room-ui");
    let dir = path.parent().unwrap().to_path_buf();
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../libraries");
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        examples.join("sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        examples.join("bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower_path = dir.join("two-plates-room.ohls");
    std::fs::write(
        &tower_path,
        tower().replace("cut = \"260 mm\"", &format!("cut = \"{ROOM_CUT}\"")),
    )
    .unwrap();
    let mut h = harness(&tower_path);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    h.get_by_label("Lay out joint…").click();
    h.step();
    crate::layout_ui_tests::wait_preview(&mut h);
    let notes = {
        let w = h.state().layout_window.as_ref().unwrap();
        w.preview.as_ref().unwrap().run.notes.clone()
    };
    let note = notes
        .iter()
        .find(|n| n.starts_with("two plates on one end"))
        .unwrap_or_else(|| panic!("{notes:#?}"));
    assert!(
        note.contains("bolt shear given a second plate") && note.contains(ROOM_WORDS),
        "{notes:#?}"
    );
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        if let Some(w) = h.state_mut().layout_window.as_mut() {
            for v in [&mut w.views.0, &mut w.views.1] {
                v.yaw -= 2.3;
                v.pitch += 0.5;
                v.zoom = 1.0;
            }
        }
        h.event(eframe::egui::Event::PointerMoved(eframe::egui::pos2(
            1290.0, 890.0,
        )));
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-two-plates-shared",
        );
    }
}

/// Where D1's steel stops in [`a_second_plate_finds_room`], and the room
/// the run then finds for its second plate.
const ROOM_CUT: &str = "120 mm";
const ROOM_WORDS: &str = "the first plate's leg bolts as its own";
