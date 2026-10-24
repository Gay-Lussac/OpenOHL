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

/// The default arrangement: the level and the tables on the left, the 3D
/// view on the right (about 45 / 55).
#[test]
fn the_3d_view_is_on_the_right_by_default() {
    let path = copy_of("type-02.ohls", "layout-default");
    let mut h = harness(&path);
    // A picture of it, when asked for (OHL_STRUCTURE_SCREENSHOT=file.png).
    if let Some(png) = ohl_core::env::var_os("OHL_STRUCTURE_SCREENSHOT") {
        h.run_steps(4);
        h.render().unwrap().save(png).unwrap();
    }
    let app = h.state();
    let view = rect_of(app, Panel::View3d);
    for p in [
        Panel::Level,
        Panel::PointsInsulators,
        Panel::Supports,
        Panel::LoadRecipes,
        Panel::Results,
    ] {
        let r = rect_of(app, p);
        assert!(r.right() <= view.left() + 1.0, "{p:?} {r:?} {view:?}");
    }
    let level = rect_of(app, Panel::Level);
    let tables = rect_of(app, Panel::PointsInsulators);
    assert!(level.bottom() <= tables.top() + 1.0);
    let share = view.width() / (view.right() - level.left());
    assert!((0.5..0.6).contains(&share), "{share}");
    // Closed until asked for.
    assert!(!app.is_open(Panel::StructureFields));
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

/// A header link to a closed panel opens it, in front.
#[test]
fn a_header_link_opens_a_closed_panel() {
    let path = copy_of("type-01.ohls", "layout-link");
    let mut h = harness(&path);
    close_panel(&mut h.state_mut().dock, Panel::Members);
    h.run();
    assert!(!h.state().is_open(Panel::Members));
    h.get_by_label("no member: build the body (Body › Generators › legs and panels), or add joints and members")
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
/// nothing kept, the default.
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
    assert!(fresh.is_open(Panel::Groups));
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
