// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The snap marker (owner, 2026-11-12): hovering near an object puts the
//! red dot on the point a click there picks; the click then picks that
//! object; a snap kind switched off in Edit › Snap takes the dot away
//! there; View › Show the snap point off hides it.

use super::session_edits::{click_at, press, synthetic_project};
use super::*;

fn hover(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
}

/// Ticks `label` in the submenu `submenu` of the menu `menu`, then closes
/// the menus (Escape).
fn tick(h: &mut Harness<'_, App>, menu: &str, submenu: Option<&str>, label: &str) {
    h.get_by_label(menu).click();
    h.run();
    let mut opener = h.get_by_label(menu).rect();
    if let Some(s) = submenu {
        let b = h.get_by_label_contains(s);
        opener = b.rect();
        b.click();
        h.run();
    }
    // (The Layers panel may hold a tick of the same name: a submenu's is
    // right of the button that opened it, a menu's below it.)
    let in_menu = |r: egui::Rect| {
        if submenu.is_some() {
            r.min.x >= opener.max.x - 1.0
        } else {
            r.min.y >= opener.max.y - 1.0 && r.max.x >= opener.min.x
        }
    };
    h.query_all_by_role_and_label(egui::accesskit::Role::CheckBox, label)
        .find(|n| in_menu(n.rect()))
        .expect("the tick")
        .click();
    h.run();
    press(h, egui::Key::Escape, egui::Modifiers::NONE);
    h.run();
}

/// In the plan: near structure T3 the dot sits on T3's drawing (a few
/// pixels from the pointer), the click picks T3; Structures off in the Snap menu, no
/// dot there and the click does not take T3; the setting off, no dot.
#[test]
fn the_snap_marker_shows_what_a_click_picks_in_the_plan() {
    let model = synthetic_project("snap-marker-plan");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run();
    let centre = {
        let r = h.state_mut().ready().unwrap();
        let p = r.model.line.structure("T3").unwrap().position;
        r.plan.centre_on(p);
        r.plan.drawn.unwrap().center()
    };
    h.run();
    let marker = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().plan.snap_hover.at;
    let near = centre + egui::vec2(5.0, 3.0);
    hover(&mut h, near);
    // On T3's drawing (its point nearest the pointer), not at the pointer.
    let m = marker(&mut h).expect("a dot near T3");
    assert!(
        m.distance(centre) < 4.0 && m.distance(near) > 1.0,
        "{m:?} {centre:?}"
    );
    click_at(&mut h, near, egui::PointerButton::Primary);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T3".into())
    );
    // Structures off in Edit › Snap: not there any more.
    tick(
        &mut h,
        "Edit",
        Some("Snap: what can be clicked"),
        "Structures",
    );
    assert!(
        !h.state_mut()
            .ready()
            .unwrap()
            .snaps
            .on(crate::selection::Snap::Structure)
    );
    hover(&mut h, near + egui::vec2(0.0, 1.0));
    hover(&mut h, near);
    assert!(marker(&mut h) != Some(m), "{:?}", marker(&mut h));
    click_at(&mut h, near, egui::PointerButton::Primary);
    assert_ne!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T3".into())
    );
    // Back on; the setting off in the View menu: no dot.
    tick(
        &mut h,
        "Edit",
        Some("Snap: what can be clicked"),
        "Structures",
    );
    hover(&mut h, near + egui::vec2(0.0, 1.0));
    hover(&mut h, near);
    assert!(marker(&mut h).is_some());
    tick(&mut h, "View", None, "Show the snap point");
    assert!(!ohl_ui::snap_marker::Settings::get(&h.ctx).show);
    hover(&mut h, near + egui::vec2(0.0, 1.0));
    assert_eq!(marker(&mut h), None);
}

/// In the 3D view (with a GPU): near an alignment point the dot sits on
/// the point, the click picks it; alignment points off in the Snap menu,
/// the dot leaves it.
#[test]
fn the_snap_marker_shows_what_a_click_picks_in_3d() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic_project("snap-marker-3d");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.run();
    let (target, p) = {
        let r = h.state_mut().ready().unwrap();
        // Alignment points clickable; the first as the click's search
        // sees it.
        r.snaps.0.insert(crate::selection::Snap::AlignmentPoint);
        let (target, p, _) = crate::selection::candidates(&r.model, r.case, &r.snaps)
            .into_iter()
            .find(|(s, _, _)| matches!(s, Selection::AlignmentPoint { .. }))
            .expect("an alignment point");
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = p;
        cam.distance = 40.0;
        (target, p)
    };
    h.run();
    let screen = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let rect = r.view3d.rect.unwrap();
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let (q, _) = r.view3d.camera.unwrap().project(p, size).unwrap();
        rect.min + egui::vec2(q[0] as f32, q[1] as f32)
    };
    let at = screen(&mut h);
    // Close to it: the structure's drawing through there would win
    // further off (structures over wires, points over both).
    let near = at + egui::vec2(2.0, -1.5);
    hover(&mut h, near);
    let marker = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().view3d.snap_hover.at;
    let m = marker(&mut h).expect("a dot near the point");
    assert!(m.distance(at) < 0.5, "{m:?} {at:?}");
    click_at(&mut h, near, egui::PointerButton::Primary);
    assert_eq!(h.state_mut().ready().unwrap().selection, target);
    tick(
        &mut h,
        "Edit",
        Some("Snap: what can be clicked"),
        "Alignment points",
    );
    hover(&mut h, near + egui::vec2(0.0, 1.0));
    hover(&mut h, near);
    assert!(marker(&mut h).is_none_or(|m| m.distance(at) > 0.5));
}
