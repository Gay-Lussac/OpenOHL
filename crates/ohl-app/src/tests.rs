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
mod checks_spacers;
mod clearances;
mod display_codes;
mod lidar;
mod objects;
mod plugins_hosting;
mod project;
mod session_edits;
mod strings_colours;
mod strings_picking;
mod structures_menus;
mod types_drawings;
mod window;
use clearances::*;
use objects::*;
use plugins_hosting::*;
pub(crate) use session_edits::*;
pub(crate) use types_drawings::*;
use window::*;

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
