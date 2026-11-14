// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tool windows of OHL-Structure (owner, 2026-11-14, as OHL-Design's;
//! docs/development/tool-pages.md): every tool has its section on the
//! tools' page, Escape closes a tool's window and changes nothing, and
//! with `OHL_RENDER_PNG` set the windows are rendered for the user guide
//! (`tool-<name>.png`, the window alone).

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::detail_tools::DetailTool;
use crate::tests::{copy_of, harness};
use crate::tools::Tool;

/// Every tool's User guide button leads to a heading of the tools' page;
/// the inventory's and the Continuous members window's too.
#[test]
fn every_tool_has_its_section() {
    let read = |page: &str| {
        let p = crate::help::shipped(&format!("docs/{page}.md")).expect(page);
        std::fs::read_to_string(p).unwrap()
    };
    let tools = read(crate::help::TOOLS_PAGE);
    let anchors = Tool::ALL
        .iter()
        .map(|t| t.guide())
        .chain(DetailTool::ALL.iter().map(|t| t.guide()))
        .chain(["holding-and-using-a-tool".to_owned()]);
    for a in anchors {
        assert!(
            ohl_ui::tool_window::has_heading(&tools, &a),
            "{}: no heading for {a}",
            crate::help::TOOLS_PAGE
        );
    }
    let (page, a) = crate::joint_tools::CONTINUOUS_PAGE;
    assert!(
        ohl_ui::tool_window::has_heading(&read(page), a),
        "{page}: {a}"
    );
}

/// Escape on a tool's window (or the inventory) closes it, nothing done,
/// and keeps the tool held; a second Escape lets go of the tool.
#[test]
fn escape_closes_a_tool_window() {
    let path = copy_of("type-02.ohls", "tool-window-escape");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    let slot = h
        .state()
        .hotbar
        .slots
        .iter()
        .position(|s| *s == Some(Tool::Delete))
        .expect("Delete in the default hotbar");
    h.state_mut().hotbar.active = slot;
    h.state_mut().tool_changed();
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.state_mut().ask_delete();
    h.run();
    assert!(h.state().confirm.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().confirm.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    assert_eq!(h.state().tool(), Tool::Delete, "the tool stays held");
    h.state_mut().joints.ask_open = Some(Handle::Joint("L10B".into()));
    h.state_mut().inventory = true;
    h.run();
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().joints.ask_open.is_none() && !h.state().inventory);
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().tool(), Tool::Select);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The window titled `title` (its largest node), for the picture.
fn window_rect(h: &Harness<'static, StructureApp>, title: &str) -> Option<egui::Rect> {
    h.query_all_by_label_contains(title)
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}

/// Saves the window titled `title` alone as `tool-<name>.png` in
/// `OHL_RENDER_PNG` (made as `USER=tester`; a GPU rendering), and the
/// screen as `screen-tool-<name>.png`.
pub(crate) fn window_shot(h: &mut Harness<'static, StructureApp>, title: &str, name: &str) {
    let Some(dir) =
        ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| ohl_render::headless_device().is_ok())
    else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    h.run_steps(4);
    let image = h.render().unwrap();
    image
        .save(dir.join(format!("screen-tool-{name}.png")))
        .unwrap();
    let ppp = image.width() as f32 / 1300.0;
    let Some(rect) = window_rect(h, title) else {
        return;
    };
    let m = 6.0;
    let x0 = ((rect.left() - m) * ppp).max(0.0) as u32;
    let y0 = ((rect.top() - m) * ppp).max(0.0) as u32;
    let x1 = (((rect.right() + m) * ppp) as u32).min(image.width());
    let y1 = (((rect.bottom() + m) * ppp) as u32).min(image.height());
    image::imageops::crop_imm(&image, x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
        .to_image()
        .save(dir.join(format!("tool-{name}.png")))
        .unwrap();
}

/// The inventory, the Delete window and Edit's question, rendered (the
/// New gauge line? window: in `joint_ui_tests`, where a bolt has no
/// room).
#[test]
fn tool_windows_rendered_for_review() {
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_none() {
        return;
    }
    let path = copy_of("type-02.ohls", "tool-window-shots");
    let mut h = harness(&path);
    h.state_mut().inventory = true;
    h.run();
    window_shot(&mut h, "Inventory — tools", "inventory");
    h.state_mut().inventory = false;
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.state_mut().ask_delete();
    h.run();
    window_shot(&mut h, "Delete", "delete");
    h.state_mut().confirm = None;
    h.state_mut().joints.ask_open = Some(Handle::Joint("L10B".into()));
    h.run();
    window_shot(&mut h, "Open in the Detail view?", "detail-question");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
