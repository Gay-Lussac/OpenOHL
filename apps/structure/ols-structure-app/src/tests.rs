// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The window driven as a user would (egui_kittest).

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ols_structure_edit::Handle;

use crate::app::{Sheet, StructureApp, Tab};

/// A copy of example structure `name` in a folder of its own.
pub(crate) fn copy_of(name: &str, tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures")
        .join(name);
    let dir = std::env::temp_dir().join(format!("ols-structure-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join(name);
    std::fs::copy(&from, &to).unwrap();
    to
}

pub(crate) fn harness(path: &Path) -> Harness<'static, StructureApp> {
    let gpu = ols_render::headless_device().is_ok();
    assert!(
        gpu || std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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

/// A full design opens on the Design tab with its level; a joint added
/// (Body) is one undo step, undone with Ctrl+Z, made again with Redo, and
/// Save writes it to the file.
#[test]
fn a_structure_is_edited_undone_and_saved() {
    let path = copy_of("type-02.olss", "edit");
    let mut h = harness(&path);
    h.get_by_label("Level 3 — full design");
    assert_eq!(h.state().tab, Tab::Design);
    // The X-braces cross without a joint: advice, not a lack.
    h.get_by_label("advice (40)");
    h.get_by_label("Body").click();
    h.run();
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
    h.get_by_label("⟳ Redo").click();
    h.run();
    assert!(h.state().dirty());
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().dirty());
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("31.000 m"), "{saved}");
    // Comments stay.
    assert!(saved.contains("# The pylon (invented, NOT a real design)"));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A stick figure: level 1, and for level 2 what it lacks; the item
/// clicked opens the Body tab's members.
#[test]
fn the_header_names_what_the_next_level_lacks() {
    let path = copy_of("type-01.olss", "level");
    let mut h = harness(&path);
    h.get_by_label("Level 1 — stick figure");
    h.get_by_label("for level 2:");
    h.get_by_label("no member: add joints and members (Body), or a 3D [[body]]")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().tab, Tab::Body);
    assert_eq!(h.state().sheets[1], Sheet::Members);
    // The placeholders say what comes.
    h.get_by_label("Results").click();
    h.run();
    assert_eq!(h.state().tab, Tab::Results);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Closing with unsaved changes asks first; Cancel keeps the window and
/// the changes.
#[test]
fn closing_with_changes_asks_first() {
    let path = copy_of("type-01.olss", "close");
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
