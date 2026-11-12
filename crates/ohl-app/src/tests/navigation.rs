// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The CAD navigation scheme (owner, 2026-11-12): a middle drag pans,
//! Shift + middle turns, a double middle-click fits, a left drag leaves
//! the view, the wheel zooms and never changes the tool; the hotbar, E and
//! a click on a slot stay; the plan follows; the default scheme is
//! unchanged; the choice is kept.

use super::session_edits::{press, synthetic_project};
use super::*;
use crate::controls::Controls;
use crate::tools::Tool;

/// A drag of `button` from `from` by `by` in a few steps, `modifiers` held.
fn drag(
    h: &mut Harness<'_, App>,
    from: egui::Pos2,
    by: egui::Vec2,
    button: egui::PointerButton,
    modifiers: egui::Modifiers,
) {
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(modifiers));
    h.input_mut().events.push(egui::Event::PointerMoved(from));
    h.step();
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers,
    });
    h.step();
    for k in 1..=5 {
        let at = from + by * (k as f32 / 5.0);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.step();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from + by,
        button,
        pressed: false,
        modifiers,
    });
    h.step();
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
}

/// A double-click of the middle button at `at`.
fn double_middle(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    // In one frame: the harness's frames are farther apart than a double
    // click's delay.
    for _ in 0..2 {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Middle,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.step();
    h.run();
}

fn wheel(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    h.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run();
}

/// The camera's numbers to compare.
fn cam(h: &mut Harness<'_, App>) -> [f64; 6] {
    let c = h.state_mut().ready().unwrap().view3d.camera.unwrap();
    [
        c.target[0],
        c.target[1],
        c.target[2],
        c.distance,
        c.yaw,
        c.pitch,
    ]
}

fn same(a: [f64; 6], b: [f64; 6]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// The 3D view in the CAD scheme (with a GPU): left drag leaves it, middle
/// drag pans (the direction kept), Shift + middle turns, the wheel zooms
/// and the tool held stays, a double middle-click fits again; E opens the
/// inventory, a click on a hotbar slot holds its tool. The default scheme
/// (Orbit) still turns with a left drag.
#[test]
fn the_cad_scheme_in_the_3d_view() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic_project("nav-cad-3d");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.run();
    let centre = h.state_mut().ready().unwrap().view3d.rect.unwrap().center();
    // The default: a left drag turns.
    assert_eq!(h.state_mut().ready().unwrap().view3d.controls.name, "Orbit");
    let before = cam(&mut h);
    drag(
        &mut h,
        centre,
        egui::vec2(60.0, 0.0),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert!(cam(&mut h)[4] != before[4]);
    // CAD.
    h.state_mut().ready().unwrap().view3d.controls = Controls::by_name("cad").unwrap();
    h.run();
    let fitted = {
        let r = h.state_mut().ready().unwrap();
        r.view3d.camera = None;
        r.hotbar.hold(Tool::Select);
        r.selection = Selection::None;
        h.run();
        cam(&mut h)
    };
    let a = cam(&mut h);
    drag(
        &mut h,
        centre,
        egui::vec2(60.0, 30.0),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert!(same(cam(&mut h), a), "a left drag moved the view");
    drag(
        &mut h,
        centre,
        egui::vec2(60.0, 30.0),
        egui::PointerButton::Middle,
        egui::Modifiers::NONE,
    );
    let b = cam(&mut h);
    assert!(b[0] != a[0] || b[1] != a[1], "a middle drag pans");
    assert!((b[4] - a[4]).abs() < 1e-12 && (b[5] - a[5]).abs() < 1e-12);
    drag(
        &mut h,
        centre,
        egui::vec2(60.0, 0.0),
        egui::PointerButton::Middle,
        egui::Modifiers::SHIFT,
    );
    let c = cam(&mut h);
    assert!((c[4] - b[4]).abs() > 1e-3, "Shift + middle turns");
    let tool = h.state_mut().ready().unwrap().hotbar.tool();
    wheel(&mut h, centre + egui::vec2(100.0, 50.0));
    let d = cam(&mut h);
    assert!(d[3] < c[3], "the wheel zooms in");
    assert_eq!(h.state_mut().ready().unwrap().hotbar.tool(), tool);
    double_middle(&mut h, centre);
    assert!(same(cam(&mut h), fitted), "a double middle-click fits");
    // The hotbar as in the other schemes.
    press(&mut h, egui::Key::E, egui::Modifiers::NONE);
    h.get_by_label("Inventory — tools");
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    h.get_by_label("slot: Delete").click();
    h.run();
    assert_eq!(h.state_mut().ready().unwrap().hotbar.tool(), Tool::Delete);
}

/// The plan in the CAD scheme: a left drag leaves it, a middle drag pans,
/// a double middle-click fits again; in Orbit a left drag pans.
#[test]
fn the_cad_scheme_in_the_plan() {
    let model = synthetic_project("nav-cad-plan");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run();
    let at = h.state_mut().ready().unwrap().plan.drawn.unwrap().center();
    let centre = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().plan.centre().unwrap();
    let fitted = centre(&mut h);
    let v = egui::vec2(80.0, 40.0);
    drag(
        &mut h,
        at,
        v,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert_ne!(centre(&mut h), fitted, "Orbit: a left drag pans");
    h.state_mut().ready().unwrap().view3d.controls = Controls::by_name("cad").unwrap();
    h.run();
    let a = centre(&mut h);
    drag(
        &mut h,
        at,
        v,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert_eq!(centre(&mut h), a, "CAD: a left drag leaves the plan");
    drag(
        &mut h,
        at,
        v,
        egui::PointerButton::Middle,
        egui::Modifiers::NONE,
    );
    assert_ne!(centre(&mut h), a, "CAD: a middle drag pans");
    double_middle(&mut h, at);
    let f = centre(&mut h);
    assert!((f[0] - fitted[0]).abs() < 1e-6 && (f[1] - fitted[1]).abs() < 1e-6);
}

/// The scheme is kept between runs.
#[test]
fn the_cad_scheme_is_kept() {
    #[derive(Default)]
    struct Memory(std::collections::HashMap<String, String>);
    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }
    let c = Controls::by_name("cad").unwrap();
    let mut m = Memory::default();
    c.save(&mut m);
    let back = Controls::load(&m).unwrap();
    assert_eq!(back, c);
    assert_eq!(back.mouse, ohl_ui::navigation::Mouse::CAD);
    assert!(back.keys.is_empty());
    assert_eq!(Controls::default().name, "Orbit");
    assert!(
        c.describe()
            .iter()
            .any(|(k, v)| k == "Shift + middle drag" && v.contains("turn"))
    );
}
