// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A weather case and a calculation per electrical system in the field
//! windows (owner, 2026-11-01: "a weather case per system (e.g. earth
//! wire at 10 °C and conductors at 40 °C due to being in use) … best would
//! be per system"): each system's wires (and the earthed wires, without a
//! system) solved in their own case and by their own calculation, the
//! window's where none is given; each case and calculation solved once.
//! Shared by the Magnetic and the Electric field windows.

use std::collections::BTreeMap;

use eframe::egui;
use ohl_clearance::calculation::{Calc, line_for};
use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_weather::WeatherCase;

use crate::model::{Model, Solved, WindDisplay};

/// Per system id (`""`: the earthed wires, no system): its weather case
/// by name and its calculation (none: the window's).
pub type PerSystem = BTreeMap<String, (Option<String>, Calc)>;

/// The key of the earthed wires (sections without a system).
pub const EARTHED: &str = "";

/// The rows of a line: each system (id, its name) and the earthed wires —
/// with a route worked on, those with a section on it (2026-11-01).
fn rows(model: &Model) -> Vec<(String, String)> {
    let line = &model.line;
    let on = model.scope();
    let has = |system: Option<&str>| {
        line.sections.iter().any(|s| {
            s.system.as_deref() == system && on.as_ref().is_none_or(|o| o.sections.contains(&s.id))
        })
    };
    let mut out: Vec<(String, String)> = line
        .systems
        .iter()
        .filter(|s| has(Some(s.id.as_str())))
        .map(|s| {
            let name = if s.name.is_empty() {
                s.id.clone()
            } else {
                format!("{} ({})", s.name, s.id)
            };
            (s.id.clone(), name)
        })
        .collect();
    if has(None) {
        out.push((EARTHED.to_owned(), "earthed wires (no system)".to_owned()));
    }
    out
}

/// The table: per system its weather case and calculation (*as above*:
/// the window's). Folded; its header says how many differ.
pub fn ui(ui: &mut egui::Ui, id: &str, model: &Model, per: &mut PerSystem) {
    let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
    let own = per
        .values()
        .filter(|(c, k)| c.is_some() || k.is_some())
        .count();
    egui::CollapsingHeader::new(if own == 0 {
        "Per electrical system: as above".to_owned()
    } else {
        format!("Per electrical system: {own} with their own")
    })
    .id_salt((id, "per-system"))
    .show(ui, |ui| {
        ui.label(
            egui::RichText::new(
                "e.g. the conductors at their load's temperature, the earth wires at the air's",
            )
            .small()
            .weak(),
        );
        egui::Grid::new((id, "per-system-grid"))
            .num_columns(3)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                for (key, name) in rows(model) {
                    let entry = per.entry(key.clone()).or_default();
                    ui.label(name);
                    egui::ComboBox::from_id_salt((id, "case", &key))
                        .selected_text(entry.0.clone().unwrap_or_else(|| "as above".into()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut entry.0, None, "as above");
                            for n in &names {
                                ui.selectable_value(&mut entry.0, Some(n.clone()), n);
                            }
                        });
                    let words = |c: Calc| c.map_or("as above".to_owned(), |k| k.label());
                    egui::ComboBox::from_id_salt((id, "calc", &key))
                        .selected_text(words(entry.1))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut entry.1, None, "as above");
                            for k in ohl_line::condition::Calculation::ALL {
                                ui.selectable_value(&mut entry.1, Some(k), k.label());
                            }
                        });
                    ui.end_row();
                }
            });
        // Rows back at "as above" are not kept.
        per.retain(|_, v| v.0.is_some() || v.1.is_some());
    });
}

/// What a run solves besides the window's case: per system key, its
/// weather case and calculation, when one differs from the window's
/// (`case`, `calc`).
pub fn plan(
    model: &Model,
    per: &PerSystem,
    case: &WeatherCase,
    calc: Calc,
) -> Vec<(String, WeatherCase, Calc)> {
    per.iter()
        .filter_map(|(key, (name, k))| {
            let c = match name {
                Some(n) => model.cases.iter().find(|c| &c.case.name == n)?.case.clone(),
                None => case.clone(),
            };
            let k = k.or(calc);
            (c.name != case.name || k != calc).then(|| (key.clone(), c, k))
        })
        .collect()
}

/// The sections of each system in `plan` taken from their own case and
/// calculation (each solved once, in the background), the others from
/// `shapes` (the window's); notes for what did not solve.
pub fn compose(
    line: &Line,
    library: &ohl_cables::Library,
    wind: WindDisplay,
    shapes: Vec<SectionShape>,
    plan: &[(String, WeatherCase, Calc)],
) -> (Vec<SectionShape>, Vec<String>) {
    if plan.is_empty() {
        return (shapes, Vec::new());
    }
    let mut notes = Vec::new();
    let mut solved: Vec<((String, Calc), Vec<SectionShape>)> = Vec::new();
    for (_, case, calc) in plan {
        let key = (case.name.clone(), *calc);
        if solved.iter().any(|s| s.0 == key) {
            continue;
        }
        let l = line_for(line, *calc);
        let s = Solved::with_wind(&l, library, case.clone(), wind);
        for f in &s.failed {
            notes.push(format!(
                "\"{}\": not solved: {f}",
                ohl_clearance::calculation::label(&case.name, *calc)
            ));
        }
        solved.push((key, s.shapes));
    }
    let of = |section: &str| -> Option<&(String, WeatherCase, Calc)> {
        let system = line
            .sections
            .iter()
            .find(|s| s.id == section)?
            .system
            .clone()
            .unwrap_or_default();
        plan.iter().find(|p| p.0 == system)
    };
    let mut out: Vec<SectionShape> = Vec::new();
    for s in line.sections.iter() {
        match of(&s.id) {
            Some((_, case, calc)) => {
                let from = &solved
                    .iter()
                    .find(|x| x.0 == (case.name.clone(), *calc))
                    .expect("solved above")
                    .1;
                if let Some(x) = from.iter().find(|x| x.section == s.id) {
                    out.push(x.clone());
                }
            }
            None => {
                if let Some(x) = shapes.iter().find(|x| x.section == s.id) {
                    out.push(x.clone());
                }
            }
        }
    }
    let said: Vec<String> = plan
        .iter()
        .map(|(key, case, calc)| {
            format!(
                "{}: in {}",
                if key.is_empty() {
                    "the earthed wires".to_owned()
                } else {
                    format!("system {key}")
                },
                ohl_clearance::calculation::label(&case.name, *calc)
            )
        })
        .collect();
    notes.insert(0, format!("per system: {}", said.join("; ")));
    (out, notes)
}
