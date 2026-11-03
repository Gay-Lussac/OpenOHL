// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menus of the cleanup (owner 2026-11-03, "yes implement the
//! menus") driven as a user would (egui_kittest): Edit › Delete selected…
//! and the Delete key, the 3D view's right-click menu, View's tick menus
//! staying open on a tick, the panels of another structure kind left out,
//! Edit › Tools, File › Close, a panel moved from its tab's menu.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::layout::{Panel, Place, move_panel};
use crate::tests::{copy_of, harness, menu, menu_entry, menu_path};
use crate::tools::Tool;

/// A click of `button` at `at`.
fn click_at(h: &mut Harness<'static, StructureApp>, at: egui::Pos2, button: egui::PointerButton) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run();
    h.run();
}

/// Edit › Delete selected… asks first, as the Delete key (and Backspace)
/// does; nothing selected: greyed out, the key does nothing.
#[test]
fn edit_delete_and_the_delete_key_ask_first() {
    let path = copy_of("type-02.ohls", "menu-delete");
    let mut h = harness(&path);
    // Nothing selected: the key does nothing.
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(h.state().confirm.is_none());
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.run();
    menu(&mut h, "Edit", "Delete selected…");
    assert!(h.state().confirm.is_some(), "asked first");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().confirm.is_none());
    assert_eq!(h.state().selected.len(), 1, "kept");
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(h.state().confirm.is_some(), "the Delete key");
    h.get_by_label("Cancel").click();
    h.run();
    h.key_press(egui::Key::Backspace);
    h.run();
    assert!(h.state().confirm.is_some(), "Backspace");
    let before = h.state().file.as_ref().unwrap().text();
    // The window's Delete button (its title says Delete too).
    h.get_all_by_label("Delete")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .unwrap()
        .click();
    h.run();
    let after = h.state().file.as_ref().unwrap().text();
    assert_ne!(before, after, "deleted");
    assert!(h.state().selected.is_empty());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A right-click on a joint in the 3D view selects it and opens its menu
/// (its name, Inspector, Open in the Detail view, Point on the joint,
/// Delete…); Delete… asks first. A right-click on nothing with something
/// selected: the selection's menu.
#[test]
fn a_right_click_opens_the_objects_menu() {
    let path = copy_of("type-02.ohls", "menu-right-click");
    let mut h = harness(&path);
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
    click_at(&mut h, at, egui::PointerButton::Secondary);
    assert_eq!(h.state().selected, vec![Handle::Joint("L10B".into())]);
    h.get_by_label("joint L10B");
    h.get_by_label("Inspector");
    h.get_by_label("Open in the Detail view");
    h.get_by_label("Point on the joint");
    h.get_by_label("Delete…").click();
    h.run();
    assert!(h.state().confirm.is_some(), "asked first");
    h.get_by_label("Cancel").click();
    h.run();
    // Nothing under the pointer, the joint still selected: the
    // selection's menu.
    let rect = h.state().view_rect().unwrap();
    click_at(
        &mut h,
        rect.right_top() + egui::vec2(-20.0, 60.0),
        egui::PointerButton::Secondary,
    );
    h.get_by_label("Clear selection").click();
    h.run();
    assert!(h.state().selected.is_empty());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// View and its submenus are tick menus: a tick never closes them
/// (Display options: two ticks in a row, View's own entries still there).
#[test]
fn views_tick_menus_stay_open_on_a_tick() {
    let path = copy_of("type-02.ohls", "menu-view-ticks");
    let mut h = harness(&path);
    assert!(h.state().rigid_links && !h.state().joint_colors);
    h.get_by_label("View").click();
    h.run();
    menu_entry(&h, "Display options").hover();
    h.run();
    h.run();
    h.get_by_label("Rigid links").click();
    h.run();
    assert!(!h.state().rigid_links);
    // Still open: another tick.
    h.get_by_label("Joints by their details").click();
    h.run();
    assert!(h.state().joint_colors);
    h.get_by_label("Usage legend");
    h.get_by_label("Reset layout");
    // A main panel ticked: View stays open too.
    h.get_by_label("Libraries").click();
    h.run();
    assert!(h.state().is_open(Panel::Libraries));
    h.get_by_label("Reset layout");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A pole: Design lists Pole design, not Joint or Member details; View ›
/// Tables shows those greyed out (why on hover).
#[test]
fn panels_of_another_kind_are_left_out() {
    let path = copy_of("type-02.ohls", "menu-kind");
    let mut h = harness(&path);
    h.state_mut()
        .file
        .as_mut()
        .unwrap()
        .edit("kind", |doc, _| {
            ohl_structure_edit::set_structure_kind(doc, "pole")
        })
        .unwrap();
    h.run();
    h.get_by_label("Design").click();
    h.run();
    h.get_by_label("Pole design");
    assert!(h.query_by_label("Joint details").is_none());
    assert!(h.query_by_label("Member details").is_none());
    h.key_press(egui::Key::Escape);
    h.run();
    menu_path(&mut h, &["View", "Tables"]);
    assert!(
        h.get_by_label("Joint details")
            .accesskit_node()
            .is_disabled()
    );
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Edit › Tools holds a tool (its hotbar key shown), the Link tool among
/// them; File › Close closes the structure (the start page again).
#[test]
fn edit_tools_hold_a_tool_and_file_close_closes() {
    let path = copy_of("type-02.ohls", "menu-tools");
    let mut h = harness(&path);
    menu_path(&mut h, &["Edit", "Tools", "Add a joint"]);
    assert_eq!(h.state().tool(), Tool::AddJoint);
    menu_path(&mut h, &["Edit", "Tools", "Insert a joint"]);
    assert_eq!(h.state().tool(), Tool::InsertJoint);
    menu(&mut h, "File", "Close");
    assert!(h.state().file.is_none());
    h.get_by_label("Open a structure…");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A panel's tab menu: floated in a window of its own, docked again on
/// the right with the Inspector, or with the tables.
#[test]
fn a_panel_moves_from_its_tabs_menu() {
    let mut app = StructureApp::new(None);
    move_panel(&mut app.dock, Panel::Supports, Place::Window);
    let at = app.dock.find_tab(&Panel::Supports).unwrap();
    assert_ne!(at.surface, egui_dock::SurfaceIndex::main());
    move_panel(&mut app.dock, Panel::Supports, Place::Right);
    let at = app.dock.find_tab(&Panel::Supports).unwrap();
    let inspector = app.dock.find_tab(&Panel::Inspector).unwrap();
    assert_eq!(at.node_path(), inspector.node_path());
    move_panel(&mut app.dock, Panel::Supports, Place::Tables);
    let at = app.dock.find_tab(&Panel::Supports).unwrap();
    let joints = app.dock.find_tab(&Panel::Joints).unwrap();
    assert_eq!(at.node_path(), joints.node_path());
}
