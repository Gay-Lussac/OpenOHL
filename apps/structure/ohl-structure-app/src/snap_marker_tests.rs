// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The snap marker in OHL-Structure's 3D view (owner, 2026-11-12):
//! hovering near a joint puts the red dot on the joint, the click then
//! picks it; View › Show the snap point off hides the dot. (OHL-Structure
//! has no menu of snap kinds: everything drawn can be clicked.)

use eframe::egui;
use ohl_structure_edit::Handle;

use crate::tests::{copy_of, harness, menu_path};

#[test]
fn the_snap_marker_shows_what_a_click_picks() {
    let path = copy_of("type-02.ohls", "snap-marker");
    let mut h = harness(&path);
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let j = t.joint("L10B").unwrap().position;
    // No GPU: no view to hover.
    let Some(at) = h.state().view_screen(j) else {
        return;
    };
    let near = at + egui::vec2(3.0, 2.0);
    h.input_mut().events.push(egui::Event::PointerMoved(near));
    h.run();
    let m = h.state().view_snap_marker().expect("a dot near the joint");
    assert!(m.distance(at) < 0.5, "{m:?} {at:?}");
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: near,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run();
    assert_eq!(h.state().selected, vec![Handle::Joint("L10B".into())]);
    // Away from everything: no dot.
    let rect = h.state().view_rect().unwrap();
    h.input_mut().events.push(egui::Event::PointerMoved(
        rect.left_top() + egui::vec2(60.0, 200.0),
    ));
    h.run();
    // The setting off (the menu stays open: Escape closes it): no dot.
    menu_path(&mut h, &["View", "Show the snap point"]);
    h.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    assert!(!ohl_ui::snap_marker::Settings::get(&h.ctx).show);
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(near + egui::vec2(0.0, 1.0)));
    h.run();
    assert_eq!(h.state().view_snap_marker(), None);
}
