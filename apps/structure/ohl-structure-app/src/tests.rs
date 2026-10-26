// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window driven as a user would (egui_kittest).

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::layout::{Area, Panel};

/// A copy of example structure `name` in a folder of its own.
pub(crate) fn copy_of(name: &str, tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures")
        .join(name);
    let dir = std::env::temp_dir().join(format!("ohl-structure-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join(name);
    std::fs::copy(&from, &to).unwrap();
    to
}

pub(crate) fn harness(path: &Path) -> Harness<'static, StructureApp> {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut builder = Harness::builder().with_size(egui::vec2(1300.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let path = path.to_path_buf();
    let mut h = builder.build_eframe(move |cc| {
        let mut app = StructureApp::new(cc.wgpu_render_state.clone());
        app.open(&path);
        app
    });
    h.run();
    h
}

/// Clicks menu `top`, then its entry `entry`.
/// Clicks the widget labelled `label`, scrolled into view first (the
/// Load case editor's parts may lie below a short panel's edge).
pub(crate) fn press(h: &mut Harness<'static, StructureApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run();
    h.get_by_label(label).click();
    h.run();
}

pub(crate) fn menu(h: &mut Harness<'static, StructureApp>, top: &str, entry: &str) {
    h.get_by_label(top).click();
    h.run();
    h.get_by_label(entry).click();
    h.run();
}

/// A full design opens on the Design panels with its level; a joint added
/// (Body) is one undo step, undone with Ctrl+Z, made again with Redo, and
/// Save writes it to the file.
#[test]
fn a_structure_is_edited_undone_and_saved() {
    let path = copy_of("type-02.ohls", "edit");
    let mut h = harness(&path);
    h.get_by_label("Level 3 — full design");
    assert_eq!(h.state().area, Area::Design);
    assert_eq!(h.state_mut().focused(), Some(Panel::Supports));
    // The X-braces cross without a joint: advice, not a lack (the
    // header's "N to do").
    h.get_by_label("40 to do");
    menu(&mut h, "Body", "Joints…");
    assert_eq!(h.state().area, Area::Body);
    h.state_mut().selected = vec![Handle::Joint("A1".into())];
    h.run();
    h.get_by_label("Add joint").click();
    h.run();
    let text = h.state().file.as_ref().unwrap().text();
    assert!(
        text.contains("point = [\"6.000 m\", \"0.000 m\", \"31.000 m\"]"),
        "{text}"
    );
    assert!(h.state().dirty());
    h.get_by_label("* unsaved");
    // Ctrl+Z: back to the file as it was.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert!(!h.state().dirty());
    // Redo, then Save.
    h.get_by_label("Redo").click();
    h.run();
    assert!(h.state().dirty());
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().dirty());
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("31.000 m"), "{saved}");
    // Comments stay.
    assert!(saved.contains("# The structure (invented, NOT a real design)"));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A stick figure: level 1, and for level 2 what it lacks (the header's
/// "N to do": the To do list); the item clicked brings the Members panel
/// to the front.
#[test]
fn the_header_names_what_the_next_level_lacks() {
    let path = copy_of("type-01.ohls", "level");
    let mut h = harness(&path);
    h.get_by_label("Level 1 — stick figure");
    h.get_by_label_contains(" to do").click();
    h.run();
    assert_eq!(h.state_mut().focused(), Some(Panel::Level));
    h.get_by_label_contains("For level 2");
    h.get_by_label("\u{2022} no member: build the body (Body › Generators › legs and panels), or add joints and members")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().area, Area::Body);
    assert_eq!(h.state_mut().focused(), Some(Panel::Members));
    // The placeholders say what comes.
    menu(&mut h, "Results", "FE results…");
    assert_eq!(h.state().area, Area::Results);
    h.get_by_label_contains("Results need level 3");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Closing with unsaved changes asks first; Cancel keeps the window and
/// the changes.
#[test]
fn closing_with_changes_asks_first() {
    let path = copy_of("type-01.ohls", "close");
    let mut h = harness(&path);
    h.get_by_label("+ Point").click();
    h.run();
    assert!(h.state().dirty());
    h.state_mut().closing = true;
    h.run();
    h.get_by_label("Save and close");
    h.get_by_label("Close without saving");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().closing && h.state().dirty());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
