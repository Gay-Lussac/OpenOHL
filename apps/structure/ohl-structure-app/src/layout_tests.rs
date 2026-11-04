// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menus and panels driven as a user would (egui_kittest): View
//! shows and hides a panel, a menu's action brings its panel to the front,
//! the default arrangement (two panes: text and edit on the left, the 3D
//! view on the right, the Detail view a tab on top of it), a header link
//! opening a closed panel, the arrangement kept between runs (a custom one
//! kept, an old default replaced).

use std::collections::HashMap;

use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::layout::{Area, Panel, close_panel};
use crate::tests::{copy_of, harness, menu};

/// The leaf rectangle `panel` is in.
fn rect_of(app: &crate::app::StructureApp, panel: Panel) -> eframe::egui::Rect {
    app.dock
        .iter_leaves()
        .find(|(_, l)| l.tabs().contains(&panel))
        .map(|(_, l)| l.rect())
        .unwrap_or_else(|| panic!("{panel:?} is not open"))
}

/// The default arrangement (owner 2026-11-04: "the text and edit
/// parameters on the left and 3d view with toolbar on the right"): two
/// panes — the Inspector, the To do list and the tables as tabs on the
/// left, the 3D view alone on the right; the Load case editor not open.
#[test]
fn a_new_file_has_two_panes_text_left_3d_right() {
    let path = copy_of("type-02.ohls", "layout-default");
    let mut h = harness(&path);
    // A picture of it, when asked for (OHL_STRUCTURE_SCREENSHOT=file.png).
    if let Some(png) = ohl_core::env::var_os("OHL_STRUCTURE_SCREENSHOT") {
        h.run_steps(4);
        h.render().unwrap().save(png).unwrap();
    }
    let app = h.state();
    assert_eq!(app.dock.iter_leaves().count(), 2);
    assert_eq!(app.dock.iter_surfaces().count(), 1, "no window");
    let view = rect_of(app, Panel::View3d);
    let leaf = |p: Panel| app.dock.find_tab(&p).unwrap().node_path();
    assert_eq!(
        app.dock.leaf(leaf(Panel::View3d)).unwrap().tabs(),
        [Panel::View3d]
    );
    for p in [
        Panel::Inspector,
        Panel::Level,
        Panel::PointsInsulators,
        Panel::Joints,
        Panel::Supports,
    ] {
        assert_eq!(leaf(p), leaf(Panel::Inspector), "{p:?}");
        let r = rect_of(app, p);
        assert!(r.right() <= view.left() + 1.0, "{p:?} {r:?} {view:?}");
        assert!((r.top() - view.top()).abs() < 1.0, "{p:?} {r:?} {view:?}");
    }
    let left = rect_of(app, Panel::Inspector);
    let share = left.width() / (view.right() - left.left());
    assert!((0.40..0.48).contains(&share), "{share}");
    // Closed until asked for; the Load case editor then in a window of
    // its own.
    assert!(!app.is_open(Panel::StructureFields));
    assert!(!app.is_open(Panel::LoadRecipes));
    assert!(!app.is_open(Panel::JointView));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A tower's joint opened in the Detail view: the Detail view a tab on
/// top of the 3D view (in front), Joint details in front on the left — no
/// third pane; a member too. Back (‹ 3D view, Escape) or the Detail view
/// closed: the 3D view in front again; opened again: on top of it.
#[test]
fn the_detail_view_opens_on_top_of_the_3d_view() {
    use crate::layout::{is_shown, same_leaf};
    let path = copy_of("type-02.ohls", "layout-detail");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("kind = \"other\"", "kind = \"lattice\"");
    std::fs::write(&path, text).unwrap();
    let mut h = harness(&path);
    h.state_mut().open_in_detail(Handle::Joint("L9A".into()));
    h.run();
    h.run();
    let two_panes = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        let dock = &h.state().dock;
        assert_eq!(
            dock.iter_leaves().count(),
            2,
            "{}",
            crate::layout::shape(dock)
        );
        assert!(same_leaf(dock, Panel::JointView, Panel::View3d));
    };
    two_panes(&h);
    assert!(is_shown(&h.state().dock, Panel::JointView));
    assert!(!h.state().panels_drawn.contains(&Panel::View3d));
    assert!(same_leaf(
        &h.state().dock,
        Panel::JointDetails,
        Panel::Inspector
    ));
    assert!(is_shown(&h.state().dock, Panel::JointDetails));
    // The way back above the drawing.
    h.get_by_label("\u{2039} 3D view").click();
    h.run();
    h.run();
    assert!(is_shown(&h.state().dock, Panel::View3d));
    assert!(h.state().panels_drawn.contains(&Panel::View3d));
    assert!(h.state().is_open(Panel::JointView));
    two_panes(&h);
    // A member: on top again, Member details on the left.
    h.state_mut().open_in_detail(Handle::Member("m155".into()));
    h.run();
    h.run();
    two_panes(&h);
    assert!(is_shown(&h.state().dock, Panel::JointView));
    assert!(same_leaf(
        &h.state().dock,
        Panel::MemberDetails,
        Panel::Inspector
    ));
    assert!(is_shown(&h.state().dock, Panel::MemberDetails));
    // Escape: from the member back to its joint, then back to the 3D view.
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    assert_eq!(
        h.state().joints.subject,
        crate::member_panel::Subject::Joint
    );
    assert!(is_shown(&h.state().dock, Panel::JointView));
    // From its details (in front on the left), Escape stays.
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    assert!(is_shown(&h.state().dock, Panel::JointView));
    // Over the drawing: back to the 3D view.
    let over = h.state().joints.view_rect.unwrap().center();
    h.hover_at(over);
    h.run();
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    h.run();
    assert!(is_shown(&h.state().dock, Panel::View3d));
    two_panes(&h);
    // Closed: the 3D view in front, the two panes.
    h.state_mut().open_in_detail(Handle::Joint("L9A".into()));
    h.run();
    assert!(is_shown(&h.state().dock, Panel::JointView));
    close_panel(&mut h.state_mut().dock, Panel::JointView);
    h.run();
    assert!(is_shown(&h.state().dock, Panel::View3d));
    assert_eq!(h.state().dock.iter_leaves().count(), 2);
    // View › Detail view: on top of the 3D view again.
    h.state_mut().focus(Panel::JointView);
    h.run();
    two_panes(&h);
    assert!(is_shown(&h.state().dock, Panel::JointView));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// View: a panel ticked off closes, ticked on opens again in front; the
/// menu stays open while ticking.
#[test]
fn view_shows_and_hides_a_panel() {
    let path = copy_of("type-01.ohls", "layout-view");
    let mut h = harness(&path);
    h.get_by_label("View").click();
    h.run();
    // View › Tables: a tick menu too.
    crate::tests::menu_entry(&h, "Tables").hover();
    h.run();
    h.run();
    h.get_by_label("Structure fields").click();
    h.run();
    assert!(h.state().is_open(Panel::StructureFields));
    assert_eq!(h.state_mut().focused(), Some(Panel::StructureFields));
    // Still open: the 3D view ticked off.
    h.get_by_label("3D view").click();
    h.run();
    assert!(!h.state().is_open(Panel::View3d));
    h.get_by_label("3D view").click();
    h.run();
    assert!(h.state().is_open(Panel::View3d));
    // Opened again on the right.
    let view = rect_of(h.state(), Panel::View3d);
    let tables = rect_of(h.state(), Panel::PointsInsulators);
    assert!(tables.right() <= view.left() + 1.0);
    // Reset layout: the default again (the panes as a new file's, the
    // area's first panel in front).
    h.get_by_label("Reset layout").click();
    h.run();
    assert!(!h.state().is_open(Panel::StructureFields));
    let dock = &h.state().dock;
    assert_eq!(
        crate::layout::shape(dock),
        crate::layout::shape(&crate::layout::default_dock())
    );
    assert_eq!(dock.iter_leaves().count(), 2);
    assert!(crate::layout::same_leaf(
        dock,
        Panel::Inspector,
        Panel::PointsInsulators
    ));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A menu's action does its work and brings its panel to the front.
#[test]
fn a_menu_action_focuses_its_panel() {
    let path = copy_of("type-02.ohls", "layout-action");
    let mut h = harness(&path);
    assert_eq!(h.state_mut().focused(), Some(Panel::Supports));
    let joints = h.state().file.as_ref().unwrap().structure().nodes.len();
    h.state_mut().selected = vec![Handle::Joint("A1".into())];
    menu(&mut h, "Body", "Add a joint");
    let after = h.state().file.as_ref().unwrap().structure().nodes.len();
    assert!(after > joints);
    assert_eq!(h.state_mut().focused(), Some(Panel::Joints));
    assert_eq!(h.state().area, Area::Body);
    // Geometry › Add a point: the points.
    menu(&mut h, "Geometry", "Add a point");
    assert_eq!(h.state_mut().focused(), Some(Panel::PointsInsulators));
    assert!(matches!(h.state().selected.as_slice(), [Handle::Point(_)]));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// An item of the To do list (the header's "N to do") opens its closed
/// panel, in front.
#[test]
fn a_header_link_opens_a_closed_panel() {
    let path = copy_of("type-01.ohls", "layout-link");
    let mut h = harness(&path);
    close_panel(&mut h.state_mut().dock, Panel::Members);
    h.run();
    assert!(!h.state().is_open(Panel::Members));
    h.get_by_label_contains(" to do").click();
    h.run();
    h.get_by_label("\u{2022} no member: build the body (Body › Generators › legs and panels), or add joints and members")
        .click();
    h.run_steps(3);
    assert!(h.state().is_open(Panel::Members));
    assert_eq!(h.state_mut().focused(), Some(Panel::Members));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A storage in memory.
#[derive(Default)]
struct Memory(HashMap<String, String>);

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

/// The arrangement is kept: a panel closed stays closed in the next run;
/// nothing kept, or kept by an older version (before the arrangement of
/// 2026-10-26), the default.
#[test]
fn the_arrangement_is_kept_between_runs() {
    let mut app = crate::app::StructureApp::new(None);
    close_panel(&mut app.dock, Panel::Groups);
    crate::layout::open_panel(&mut app.dock, Panel::StructureFields);
    let mut storage = Memory::default();
    eframe::App::save(&mut app, &mut storage);
    let mut next = crate::app::StructureApp::new(None);
    next.restore(Some(&storage));
    assert!(!next.is_open(Panel::Groups));
    assert!(next.is_open(Panel::StructureFields));
    let mut fresh = crate::app::StructureApp::new(None);
    fresh.restore(Some(&Memory::default()));
    assert!(fresh.is_open(Panel::Supports));
    assert!(fresh.is_open(Panel::Inspector));
    // An older arrangement (no version kept): the default instead.
    eframe::Storage::remove_string(&mut storage, crate::layout::DOCK_VERSION_KEY);
    let mut old = crate::app::StructureApp::new(None);
    old.restore(Some(&storage));
    assert!(old.is_open(Panel::Inspector));
    assert!(!old.is_open(Panel::StructureFields));
    // One that does not read: the default.
    eframe::Storage::set_string(&mut storage, crate::layout::DOCK_VERSION_KEY, "2".into());
    eframe::Storage::set_string(&mut storage, crate::layout::DOCK_KEY, "not ron".into());
    let mut broken = crate::app::StructureApp::new(None);
    broken.restore(Some(&storage));
    assert!(broken.is_open(Panel::Inspector));
}

/// The default before 2026-11-04 (version 2): the 3D view in the middle,
/// the Inspector and the To do list on its right, the tables below it —
/// the Detail view beside the 3D view and the details beside the tables
/// when they had been opened.
fn old_default(detail: bool) -> egui_dock::DockState<Panel> {
    use egui_dock::NodeIndex;
    let mut dock = egui_dock::DockState::new(vec![Panel::View3d]);
    let tree = dock.main_surface_mut();
    let [view, _] = tree.split_right(NodeIndex::root(), 0.8, vec![Panel::Inspector, Panel::Level]);
    let [view, tables] = tree.split_below(view, 0.6, vec![Panel::Joints, Panel::Results]);
    if detail {
        tree.split_right(view, 0.5, vec![Panel::JointView]);
        tree.split_right(tables, 0.5, vec![Panel::JointDetails]);
    }
    dock
}

/// An arrangement the user changed is theirs: a pane split (the tables in
/// two) is kept across a restart, also one kept by the version before
/// (2026-11-04); that version's own default — as it opened, with or
/// without the Detail view — gives way to the new default (two panes).
#[test]
fn a_custom_split_survives_a_restart() {
    use egui_dock::NodeIndex;
    let mut app = crate::app::StructureApp::new(None);
    // The tables split: Supports in a pane of its own below the others.
    let at = app.dock.find_tab(&Panel::Supports).unwrap();
    app.dock.remove_tab(at);
    let left = app.dock.find_tab(&Panel::Inspector).unwrap().node;
    app.dock
        .main_surface_mut()
        .split_below(left, 0.5, vec![Panel::Supports]);
    let custom = crate::layout::shape(&app.dock);
    assert_eq!(app.dock.iter_leaves().count(), 3);
    let mut storage = Memory::default();
    eframe::App::save(&mut app, &mut storage);
    let mut next = crate::app::StructureApp::new(None);
    next.restore(Some(&storage));
    assert_eq!(crate::layout::shape(&next.dock), custom);
    assert!(!crate::layout::same_leaf(
        &next.dock,
        Panel::Supports,
        Panel::Joints
    ));
    // Kept by version 2: still theirs.
    eframe::Storage::set_string(&mut storage, crate::layout::DOCK_VERSION_KEY, "2".into());
    let mut v2 = crate::app::StructureApp::new(None);
    v2.restore(Some(&storage));
    assert_eq!(crate::layout::shape(&v2.dock), custom);
    // Version 2's own default (also with the Detail view and its details
    // opened as they did): the new default.
    let new = crate::layout::shape(&crate::layout::default_dock());
    for detail in [false, true] {
        let old = old_default(detail);
        assert!(
            crate::layout::is_old_default(&old),
            "{}",
            crate::layout::shape(&old)
        );
        eframe::set_value(&mut storage, crate::layout::DOCK_KEY, &old);
        let mut app = crate::app::StructureApp::new(None);
        app.restore(Some(&storage));
        assert_eq!(crate::layout::shape(&app.dock), new);
    }
    // Version 2's default with a pane moved (the Inspector below the
    // others): the user's, kept.
    let mut moved = old_default(false);
    let at = moved.find_tab(&Panel::Inspector).unwrap();
    moved.remove_tab(at);
    let at = moved.find_tab(&Panel::Level).unwrap();
    moved.remove_tab(at);
    moved
        .main_surface_mut()
        .split_below(NodeIndex::root(), 0.7, vec![Panel::Inspector]);
    assert!(!crate::layout::is_old_default(&moved));
    eframe::set_value(&mut storage, crate::layout::DOCK_KEY, &moved);
    let mut app = crate::app::StructureApp::new(None);
    app.restore(Some(&storage));
    assert_eq!(
        crate::layout::shape(&app.dock),
        crate::layout::shape(&moved)
    );
}

/// No tab group of the default arrangement holds more tabs than its width
/// shows: every tab title is in sight (no tab bar scrolled).
#[test]
fn every_default_tab_title_shows() {
    let path = copy_of("type-02.ohls", "layout-tabs");
    let mut h = harness(&path);
    h.run_steps(2);
    let app = h.state();
    for (_, leaf) in app.dock.iter_leaves() {
        // About 7 px a character and 30 px of tab around it.
        let needed: f32 = leaf
            .tabs()
            .iter()
            .map(|p| p.title().chars().count() as f32 * 7.0 + 30.0)
            .sum();
        assert!(
            needed <= leaf.rect().width(),
            "{:?}: {needed} px for {} px",
            leaf.tabs(),
            leaf.rect().width()
        );
    }
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
