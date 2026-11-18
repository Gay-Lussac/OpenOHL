// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Orient angles (2026-11-18): Design › Orient angles… opens the window,
//! its preview is worked out on a copy, Apply is one undo step, Escape
//! changes nothing.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::tests::{copy_of, harness};

/// The example tower with angles for its legs and bracing (the example
/// sections beside it), no rolls: as an import gives it.
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
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("section = \"leg\"", "section = \"EA-61x7\"")
        .replace("section = \"brace\"", "section = \"EA-61x7\"");
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, text).unwrap();
    let h = harness(&tower);
    (h, dir)
}

/// Runs frames until the window's preview is there (at most ~30 s).
fn wait_preview(h: &mut Harness<'static, StructureApp>) {
    for _ in 0..600 {
        h.step();
        if h.state()
            .orient_window
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
fn the_legs_are_oriented_in_one_undo_step() {
    let (mut h, dir) = open("orient-window");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_orient_window();
    h.step();
    assert!(h.state().orient_window.is_some());
    wait_preview(&mut h);
    let turned = {
        let w = h.state().orient_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        assert!(
            p.run
                .turned
                .iter()
                .any(|o| o.kind == ohl_structure_link::OrientKind::Chord),
            "{:?} left {:?}",
            p.run.turned,
            p.run.left
        );
        p.run.turned.len()
    };
    crate::tool_window_tests::window_shot(&mut h, "Orient angles", "orient-angles");
    // Nothing written before Apply.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().orient_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_ne!(f.text(), before);
    assert!(f.undo_name().unwrap().starts_with("Orient angles"));
    let rec = ohl_structure_link::LayoutRecord::of(f.structure());
    assert_eq!(rec.oriented.len(), turned);
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn escape_closes_the_orient_window_and_changes_nothing() {
    let (mut h, dir) = open("orient-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_orient_window();
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().orient_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}
