// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menus and panels driven as a user would (egui_kittest): View
//! shows and hides a panel, a menu's action brings its panel to the front,
//! the default arrangement, a header link opening a closed panel, the
//! arrangement kept between runs.

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

/// The default arrangement (owner, 2026-10-26): the 3D view large in the
/// middle, the Inspector and the To do list on its right, the tables as
/// tabs below it; the Load case editor not open.
#[test]
fn the_3d_view_is_in_the_middle_by_default() {
    let path = copy_of("type-02.ohls", "layout-default");
    let mut h = harness(&path);
    // A picture of it, when asked for (OHL_STRUCTURE_SCREENSHOT=file.png).
    if let Some(png) = ohl_core::env::var_os("OHL_STRUCTURE_SCREENSHOT") {
        h.run_steps(4);
        h.render().unwrap().save(png).unwrap();
    }
    let app = h.state();
    let view = rect_of(app, Panel::View3d);
    for p in [Panel::Level, Panel::Inspector] {
        let r = rect_of(app, p);
        assert!(r.left() >= view.right() - 1.0, "{p:?} {r:?} {view:?}");
    }
    for p in [Panel::PointsInsulators, Panel::Supports, Panel::Results] {
        let r = rect_of(app, p);
        assert!(r.top() >= view.bottom() - 1.0, "{p:?} {r:?} {view:?}");
    }
    let inspector = rect_of(app, Panel::Inspector);
    let share = view.width() / (inspector.right() - view.left());
    assert!((0.75..0.85).contains(&share), "{share}");
    // The Inspector in front of the To do list.
    assert!(app.panels_drawn.contains(&Panel::Inspector));
    assert!(!app.panels_drawn.contains(&Panel::Level));
    // Closed until asked for; the Load case editor then in a window of
    // its own.
    assert!(!app.is_open(Panel::StructureFields));
    assert!(!app.is_open(Panel::LoadRecipes));
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
    // Reset layout: the default again.
    h.get_by_label("Reset layout").click();
    h.run();
    assert!(!h.state().is_open(Panel::StructureFields));
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
