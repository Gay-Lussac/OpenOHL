// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The UI review of 2026-10-26 (owner): units in the column headers, the
//! short "No line values" line closed, an error notice staying until
//! closed, the recent files and the example on the start page, the 3D
//! view's standard views, the Line loads menu; then the tools: a click
//! only selects, the Move tool drags (one undo step), the hotbar's keys
//! and the inventory, an Inspector edit (one undo step), the calmer
//! arrangement. With `OHL_RENDER_PNG` set, images of the window.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::layout::Panel;
use crate::tests::{copy_of, harness, menu};
use crate::tools::Tool;

/// Saves the window as `name` in `OHL_RENDER_PNG` (a GPU rendering).
fn shot(h: &mut Harness<'static, StructureApp>, name: &str) {
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = h.render().unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

fn text(h: &Harness<'static, StructureApp>) -> String {
    h.state().file.as_ref().unwrap().text()
}

/// The window as it opens: the header, the 3D view with its hotbar, the
/// Inspector, the tables below.
#[test]
fn the_window_opens_calmly() {
    let path = copy_of("type-02.ohls", "review-window");
    let mut h = harness(&path);
    h.state_mut().focus(Panel::Joints);
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.run();
    shot(&mut h, "ohl-structure-window.png");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The units are in the column headers ("x \[m\]", "Transverse \[kN\]"), the
/// cells show the bare numbers; a number typed is read as before.
#[test]
fn units_are_in_the_column_headers() {
    let path = copy_of("type-02.ohls", "review-units");
    let mut h = harness(&path);
    menu(&mut h, "Body", "Joints…");
    for header in ["Joint", "x [m]", "y [m]", "z [m]", "Kind", "Members"] {
        assert!(h.query_by_label(header).is_some(), "{header}");
    }
    // The supports' heights: bare numbers, the unit in the header.
    menu(&mut h, "Design", "Supports…");
    h.get_by_label("z [m]");
    assert!(h.query_all_by_label("0.000").count() >= 4);
    assert!(h.query_by_label("0.000 m").is_none());
    menu(&mut h, "Load cases", "Load table…");
    for header in ["Transverse [kN]", "Longitudinal [kN]", "Vertical [kN]"] {
        assert!(h.query_by_label(header).is_some(), "{header}");
    }
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Opened on its own, the load panels say "No line values" in one short
/// line (the whole sentence on hover) that its × closes; the Store button
/// says why on hover.
#[test]
fn the_line_values_notice_is_closed() {
    let path = copy_of("type-02.ohls", "review-notice");
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load table…");
    h.state_mut().message = None;
    h.state_mut().notices.items.clear();
    h.run();
    h.get_by_label("No line values (opened on its own)");
    h.get_by_label("\u{d7}").click();
    h.run();
    assert!(h.state().line_notice_closed);
    assert!(
        h.query_by_label("No line values (opened on its own)")
            .is_none()
    );
    // Still there: the Store button (greyed).
    h.get_by_label("Store in the structure file…");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A message is also a notice at the bottom right: an information goes
/// after a few seconds, an error stays until its × is clicked (a long one
/// scrolls in a box, its × in reach); the status line stays.
#[test]
fn an_error_notice_stays_until_closed() {
    let path = copy_of("type-01.ohls", "review-error");
    let mut h = harness(&path);
    h.state_mut().message = Some(("saved it".into(), false));
    h.run();
    assert_eq!(h.state().notices.items.len(), 1);
    h.run_steps(24);
    assert!(h.state().notices.items.is_empty(), "faded");
    let long = format!("did not read: {}", "a reason; ".repeat(80));
    h.state_mut().message = Some((long.clone(), true));
    h.run();
    h.run_steps(40);
    assert_eq!(h.state().notices.items.len(), 1, "an error stays");
    let close = h.get_by_label("\u{d7}");
    let r = close.rect();
    assert!(r.max.y <= 900.0 && r.max.x <= 1300.0, "{r:?}");
    close.click();
    h.run();
    assert!(h.state().notices.items.is_empty());
    // The status line keeps it.
    assert_eq!(h.state().message.as_ref().map(|m| m.1), Some(true));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A storage in memory.
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

/// The start page: the recent files (kept between runs, the last first),
/// one not there any more marked and removed with its ×; Open the example
/// opens a copy of the example structure (not added to the recent files).
#[test]
fn the_start_page_lists_recent_files_and_the_example() {
    let path = copy_of("type-01.ohls", "review-recent");
    let gone = path.parent().unwrap().join("gone.ohls");
    let mut app = StructureApp::new(None);
    app.recent.add(&gone);
    app.recent.add(&path);
    let mut storage = Memory::default();
    eframe::App::save(&mut app, &mut storage);
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1300.0, 900.0))
        .build_eframe(move |_| {
            let mut a = StructureApp::new(None);
            a.restore(Some(&storage));
            a
        });
    h.run();
    assert_eq!(h.state().recent.files.len(), 2);
    assert!(h.state().recent.files[0].ends_with("type-01.ohls"));
    h.get_by_label_contains("type-01.ohls");
    h.get_by_label_contains("gone.ohls (missing)");
    // The second row's ×: the missing file.
    let crosses: Vec<_> = h.get_all_by_label("\u{d7}").collect();
    assert_eq!(crosses.len(), 2);
    crosses[1].click();
    h.run();
    assert_eq!(h.state().recent.files.len(), 1);
    assert!(h.query_by_label_contains("gone.ohls").is_none());
    // The example: a copy, opened.
    h.get_by_label("Open the example").click();
    h.run();
    let f = h.state().file.as_ref().expect("the example opened");
    let opened = f.path().unwrap().to_path_buf();
    assert!(opened.ends_with("type-02.ohls"));
    assert!(!opened.starts_with(env!("CARGO_MANIFEST_DIR")), "a copy");
    assert_eq!(h.state().recent.files.len(), 1);
    std::fs::remove_dir_all(opened.parent().unwrap()).ok();
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The 3D view's buttons: Front looks along +y, Side along -x, Top from
/// above (parallel), Iso and Fit as the view opens.
#[test]
fn the_standard_views() {
    let path = copy_of("type-02.ohls", "review-views");
    let mut h = harness(&path);
    if h.state().view_camera().is_none() {
        return; // no GPU: no view
    }
    let turned = |h: &mut Harness<'static, StructureApp>, name: &str| {
        h.get_by_label(name).click();
        h.run();
        h.state().view_camera().unwrap()
    };
    let c = turned(&mut h, "Front");
    assert!((c.yaw.to_degrees() + 90.0).abs() < 1e-9 && c.pitch.abs() < 1e-9 && c.ortho);
    // Looking along +y: the eye behind the target.
    assert!(c.eye()[1] < c.target[1]);
    shot(&mut h, "ohl-structure-front-view.png");
    let c = turned(&mut h, "Side");
    assert!(c.yaw.abs() < 1e-9 && c.ortho && c.eye()[0] > c.target[0]);
    let c = turned(&mut h, "Top");
    assert!(c.pitch.to_degrees() > 89.0 && c.ortho);
    let c = turned(&mut h, "Iso");
    assert!(!c.ortho && (c.pitch.to_degrees() - 35.0).abs() < 1e-9);
    let c = turned(&mut h, "Fit");
    assert!((c.pitch.to_degrees() - 15.0).abs() < 1e-9);
    // The controls on hover of "?".
    h.get_by_label("?");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// "In the line" is now "Line loads": the menu and its panel.
#[test]
fn line_loads_is_the_menu_and_the_panel() {
    let path = copy_of("type-02.ohls", "review-rename");
    let mut h = harness(&path);
    assert!(h.query_by_label("In the line").is_none());
    menu(&mut h, "Line loads", "Line loads…");
    assert_eq!(h.state_mut().focused(), Some(Panel::InTheLine));
    assert_eq!(Panel::InTheLine.title(), "Line loads");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A drag from `from` to `to` in a few steps.
fn drag(h: &mut Harness<'static, StructureApp>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run();
    h.drag_at(from);
    h.run();
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.run();
    }
    h.drop_at(to);
    h.run();
}

/// A click on a joint selects it; a drag from it with Select turns the
/// view, the joint stays (owner, 2026-10-26: no drag straight away). With
/// the Move tool (key 2) the drag moves it, one undo step.
#[test]
fn a_joint_moves_only_with_the_move_tool() {
    let path = copy_of("type-02.ohls", "review-move");
    let mut h = harness(&path);
    let before = text(&h);
    let j = h
        .state()
        .file
        .as_ref()
        .unwrap()
        .structure()
        .joint("L10B")
        .unwrap()
        .position;
    let Some(at) = h.state().view_screen(j) else {
        return; // no GPU: no view
    };
    assert_eq!(h.state().tool(), Tool::Select);
    // A click selects.
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
    assert_eq!(h.state().selected, vec![Handle::Joint("L10B".into())]);
    // A drag with Select: the view turns, the file stays.
    drag(&mut h, at, at + egui::vec2(40.0, -30.0));
    assert_eq!(text(&h), before);
    assert_ne!(h.state().view_camera().unwrap().yaw, -135f64.to_radians());
    // The Move tool: key 2.
    h.state_mut().view_fit = true;
    h.run();
    h.run();
    h.key_press(egui::Key::Num2);
    h.run();
    assert_eq!(h.state().tool(), Tool::Move);
    let at = h.state().view_screen(j).unwrap();
    drag(&mut h, at, at + egui::vec2(0.0, -40.0));
    let moved = text(&h);
    assert_ne!(moved, before, "moved");
    // One undo step for the whole drag.
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The hotbar: keys 1-9 hold a slot's tool, E opens the inventory, a tool
/// dropped there from a slot empties it, Escape lets go of the tool; the
/// arrangement is kept between runs.
#[test]
fn the_hotbar_keys_and_the_inventory() {
    let path = copy_of("type-02.ohls", "review-hotbar");
    let mut h = harness(&path);
    h.key_press(egui::Key::Num3);
    h.run();
    assert_eq!(h.state().tool(), Tool::AddPoint);
    h.get_by_label("Add point");
    h.key_press(egui::Key::Num6);
    h.run();
    assert_eq!(h.state().tool(), Tool::Link);
    assert!(h.state().link.is_some(), "the Link tool taken up");
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().tool(), Tool::Select);
    assert!(h.state().link.is_none());
    h.key_press(egui::Key::E);
    h.run();
    assert!(h.state().inventory);
    h.get_by_label("Inventory — tools");
    h.get_by_label("Insert a joint on a member");
    shot(&mut h, "ohl-structure-inventory.png");
    // Closed again (E); a slot clicked in the hotbar holds its tool.
    h.key_press(egui::Key::E);
    h.run();
    assert!(!h.state().inventory);
    h.get_by_label("slot: Delete").click();
    h.run();
    assert_eq!(h.state().tool(), Tool::Delete);
    // A tool not in the hotbar held from a menu: put in a slot.
    h.state_mut().hold_tool(Tool::Symmetry);
    h.run();
    assert_eq!(h.state().tool(), Tool::Symmetry);
    h.get_by_label("Make symmetric");
    let mut storage = Memory::default();
    eframe::App::save(h.state_mut(), &mut storage);
    let mut next = StructureApp::new(None);
    next.restore(Some(&storage));
    assert_eq!(next.hotbar, h.state().hotbar);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// An acting tool: Add point on a joint clicked adds a point on it (one
/// undo step), Delete asks first.
#[test]
fn a_tool_acts_on_a_click() {
    let path = copy_of("type-02.ohls", "review-tool-click");
    let mut h = harness(&path);
    let before = text(&h);
    h.state_mut().hold_tool(Tool::AddPoint);
    h.state_mut()
        .tool_click(Some(Handle::Joint("L9A".into())), None);
    h.run();
    let after = text(&h);
    assert_ne!(after, before);
    assert!(matches!(h.state().selected.as_slice(), [Handle::Point(_)]));
    h.state_mut().undo();
    assert_eq!(text(&h), before);
    h.state_mut().hold_tool(Tool::Delete);
    h.state_mut()
        .tool_click(Some(Handle::Member("m155".into())), None);
    h.run();
    assert!(h.state().confirm.is_some(), "asked first");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The Inspector shows what is selected; a value typed there is one
/// edit (one undo step); nothing selected: a hint; several joints: the
/// coordinates they share.
#[test]
fn an_inspector_edit_is_one_undo_step() {
    let path = copy_of("type-02.ohls", "review-inspector");
    let mut h = harness(&path);
    let before = text(&h);
    h.get_by_label_contains("Nothing selected");
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.run();
    h.get_by_label("Joint L10B");
    // Its z: the third value field of the Inspector (the panels in front
    // have none).
    let fields: Vec<_> = h
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .collect();
    assert_eq!(fields.len(), 3, "x, y, z");
    fields[2].focus();
    h.run();
    h.run();
    h.get_all_by_role(egui::accesskit::Role::SpinButton)
        .nth(2)
        .unwrap()
        .type_text("31.5");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    let z = h
        .state()
        .file
        .as_ref()
        .unwrap()
        .structure()
        .joint("L10B")
        .unwrap()
        .position[2];
    assert!((z - 31.5).abs() < 1e-9, "{z}");
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), before);
    // Two joints at one height: z shared, x differs.
    h.state_mut().selected = vec![Handle::Joint("L0A".into()), Handle::Joint("L0B".into())];
    h.run();
    h.get_by_label("2 joints");
    h.get_by_label("differ");
    shot(&mut h, "ohl-structure-inspector.png");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
