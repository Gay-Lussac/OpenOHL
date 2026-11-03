// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::model::{self, Sources};
use crate::selection::Selection;

mod alignments;
mod backgrounds_lines;
mod building_fields;
mod check_shots;
mod check_windows;
mod checks_spacers;
mod clearances;
mod display_codes;
mod field_setup;
mod lidar;
mod load_case_library;
mod local_winds;
mod multi_select;
mod objects;
mod plugins_hosting;
mod project;
mod session_edits;
mod string_builder;
mod strings_colours;
mod strings_picking;
mod structures_menus;
mod tutorial;
mod types_drawings;
mod ui_review;
mod window;
use clearances::*;
use objects::*;
use plugins_hosting::*;
pub(crate) use session_edits::*;
pub(crate) use types_drawings::*;
use window::*;

/// Opens `menu` of the menu bar, then its submenu `sub` (hovered): the
/// menus by task (owner: OHL-Design UI review, 2026-10-26), e.g. Line ›
/// Terrain, Edit › Tools, File › Plugins.
pub(crate) fn submenu(h: &mut Harness<'_, App>, menu: &str, sub: &str) {
    h.get_by_label(menu).click();
    h.run();
    h.get_by_label(&format!("{sub} ⏵")).hover();
    h.run();
}

/// Opens the check window's *Model* (folded by default, 2026-10-31:
/// [`crate::check_ui::model_section`]), where its modelling settings are.
pub(crate) fn open_model(h: &mut Harness<'_, App>) {
    h.get_by_label_contains("  Model").click_accesskit();
    h.run_steps(3);
}

/// Opens Line › `sub` (Terrain, Structures, Sections, Criteria).
pub(crate) fn line_menu(h: &mut Harness<'_, App>, sub: &str) {
    submenu(h, "Line", sub);
}

pub(crate) fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Every weather case solved (the application solves the others when
/// they are shown; the tests look at all of them).
fn solved_all(mut m: model::Model) -> model::Model {
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

/// Works on project line `name`, every weather case solved at once (the
/// application solves them in the background).
fn switch(m: &mut model::Model, name: &str) {
    m.switch_line(name).unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
}

/// The Run button of the check window titled `title` (in its footer, at
/// the bottom left): of the Run buttons in the window (its accessible
/// node is named by its title), the one nearest its bottom left corner
/// (several check windows may be open, overlapping).
pub(crate) fn run_in<'h>(h: &'h Harness<'_, App>, title: &str) -> egui_kittest::Node<'h> {
    let w = h.get_by_label(title).rect();
    h.query_all_by_label("Run")
        .filter(|n| w.contains_rect(n.rect()))
        .min_by(|a, b| {
            let d = |n: &egui_kittest::Node<'_>| {
                (n.rect().left() - w.left()).abs() + (w.bottom() - n.rect().bottom()).abs()
            };
            d(a).total_cmp(&d(b))
        })
        .unwrap_or_else(|| panic!("no Run in \"{title}\""))
}

/// The shielded example line (examples/lightning: the synthetic line's
/// five structures, three phases and two earth wires) on the synthetic
/// survey.
pub(crate) fn shielded() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    solved_all(
        model::load(
            Sources {
                file: root().join("examples/lightning/shielded-line.ohl.json"),
                library: Some(root().join("libraries/cables")),
                terrain: Some(ex.join("synthetic-survey.csv")),
                weather: Some(root().join("examples/lightning/weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    )
}

pub(crate) fn synthetic() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    solved_all(
        model::load(
            Sources {
                file: ex.join("synthetic-line.ohl.json"),
                library: Some(root().join("libraries/cables")),
                terrain: Some(ex.join("synthetic-survey.csv")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    )
}
