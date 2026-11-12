// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Structure's navigation schemes (owner, 2026-11-12): Orbit by
//! default (a left drag turns, as before); View › Navigation › CAD: a left
//! drag leaves the 3D view, a middle drag pans, Shift + middle turns, the
//! wheel zooms and leaves the tool, a double middle-click fits; E and the
//! hotbar's slots as before; the choice kept between runs; the "?" text
//! follows the scheme.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_ui::navigation::Scheme;

use crate::app::StructureApp;
use crate::tests::{copy_of, harness, menu_path};

type H = Harness<'static, StructureApp>;

fn drag(
    h: &mut H,
    from: egui::Pos2,
    by: egui::Vec2,
    button: egui::PointerButton,
    m: egui::Modifiers,
) {
    h.input_mut().events.push(egui::Event::ModifiersChanged(m));
    h.input_mut().events.push(egui::Event::PointerMoved(from));
    h.step();
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers: m,
    });
    h.step();
    for k in 1..=5 {
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(from + by * (k as f32 / 5.0)));
        h.step();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from + by,
        button,
        pressed: false,
        modifiers: m,
    });
    h.step();
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
}

fn cam(h: &H) -> [f64; 6] {
    let c = h.state().view_camera().unwrap();
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

#[test]
fn the_cad_scheme_moves_the_3d_view() {
    let path = copy_of("type-02.ohls", "nav-cad");
    let mut h = harness(&path);
    assert_eq!(h.state().navigation, Scheme::Orbit);
    // No GPU: no view to drag.
    let Some(rect) = h.state().view_rect() else {
        return;
    };
    // Away from the structure (a drag on a joint is the Move tool's).
    let at = rect.left_top() + egui::vec2(80.0, 150.0);
    let none = egui::Modifiers::NONE;
    let fitted = cam(&h);
    drag(
        &mut h,
        at,
        egui::vec2(60.0, 0.0),
        egui::PointerButton::Primary,
        none,
    );
    assert!(cam(&h)[4] != fitted[4], "Orbit: a left drag turns");
    // View › Navigation › CAD (the menu stays open: Escape closes it).
    menu_path(&mut h, &["View", "Navigation", "CAD"]);
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().navigation, Scheme::Cad);
    // The "?" text (its hover) is made for the scheme in force.
    assert!(crate::view::controls(Scheme::Cad.mouse()).contains("middle drag pans"));
    let a = cam(&h);
    drag(
        &mut h,
        at,
        egui::vec2(60.0, 30.0),
        egui::PointerButton::Primary,
        none,
    );
    assert!(same(cam(&h), a), "CAD: a left drag leaves the view");
    drag(
        &mut h,
        at,
        egui::vec2(60.0, 30.0),
        egui::PointerButton::Middle,
        none,
    );
    let b = cam(&h);
    assert!(b[0] != a[0] || b[2] != a[2], "CAD: a middle drag pans");
    assert!((b[4] - a[4]).abs() < 1e-12);
    drag(
        &mut h,
        at,
        egui::vec2(60.0, 0.0),
        egui::PointerButton::Middle,
        egui::Modifiers::SHIFT,
    );
    let c = cam(&h);
    assert!((c[4] - b[4]).abs() > 1e-3, "CAD: Shift + middle turns");
    let tool = h.state().held;
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    h.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: none,
        phase: egui::TouchPhase::Move,
    });
    h.run();
    assert!(cam(&h)[3] < c[3], "the wheel zooms in");
    assert_eq!(h.state().held, tool, "the wheel leaves the tool");
    // A double middle-click (in one frame: the harness's frames are
    // farther apart than a double click's delay): the whole structure (as it
    // opened, within the view.s size: the menus changed it slightly).
    for _ in 0..2 {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Middle,
                pressed,
                modifiers: none,
            });
        }
    }
    h.step();
    h.run();
    let f = cam(&h);
    assert!(
        (f[3] - fitted[3]).abs() < 0.01 * fitted[3]
            && (0..3).all(|k| (f[k] - fitted[k]).abs() < 0.01),
        "a double middle-click fits: {f:?} {fitted:?}"
    );
    // E: the inventory; a click on a hotbar slot holds its tool.
    h.key_press(egui::Key::E);
    h.run();
    assert!(h.state().inventory);
    h.key_press(egui::Key::E);
    h.run();
    let name = format!("slot: {}", crate::tools::Tool::Move.name());
    h.get_by_label(&name).click();
    h.run();
    assert_eq!(h.state().hotbar.tool(), crate::tools::Tool::Move);
}

#[test]
fn the_scheme_is_kept() {
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
    let mut m = Memory::default();
    let key = crate::style::NAVIGATION_KEY;
    assert_eq!(Scheme::load(Some(&m), key), Scheme::Orbit);
    Scheme::Cad.save(&mut m, key);
    assert_eq!(Scheme::load(Some(&m), key), Scheme::Cad);
    assert!(crate::view::controls(Scheme::Orbit.mouse()).starts_with("Navigation Orbit"));
}
