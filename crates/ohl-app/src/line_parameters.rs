// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Line parameters…** (roadmap O18, owner
//! 2026-10-27; built, in active development, not approved yet): the
//! line's electrical settings (soil resistivity, earth return method, the
//! wires' height in a span; one undoable command each change) and, on
//! Run, its line parameters in a weather case ([`ohl_electrical::run`]):
//! a row per circuit — positive and zero sequence R, X, B per km and in
//! total — its stretches folded under it, the coupling between circuits,
//! the notes (what was assumed); Export CSV. Out of date like the other
//! checks when the line changes.

use std::collections::BTreeSet;

use eframe::egui;
use ohl_electrical::LineParameters;
use ohl_electrical::report::{self, Row};
use ohl_line::electrical::{
    DEFAULT_SOIL_RESISTIVITY_OHM_M, EarthReturn, ElectricalSettings, MeanHeight,
};

use crate::check_help as help;
use crate::job::{Job, save_csv};
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// New electrical settings for the line (one command).
    pub settings: Option<ElectricalSettings>,
}

/// The answer: the parameters and the weather case.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The parameters.
    pub parameters: LineParameters,
    /// The weather case's name.
    pub case: String,
}

/// The Line parameters window.
#[derive(Default)]
pub struct LineParametersWindow {
    /// Shown.
    pub open: bool,
    /// The weather case (index into the model's cases).
    pub case: usize,
    /// The circuits whose stretches are shown.
    expanded: BTreeSet<String>,
    /// The run.
    pub(crate) job: Job<Answer>,
}

impl LineParametersWindow {
    /// Opens the window on the weather case shown.
    pub fn show(&mut self, case: usize) {
        if !self.open {
            self.case = case;
        }
        self.open = true;
    }

    /// What the last run was computed from that changed since, in words
    /// (`None`: nothing, or no answer).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// Starts a run: the parameters from the case's solved sections.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(solved) = model.cases.get(self.case) else {
            self.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let shapes = solved.shapes.clone();
        let case = solved.case.clone();
        self.job.start_at(ctx, model.revision(), move || {
            let mut parameters =
                ohl_electrical::run(&line, &library, &shapes, case.temperature, None);
            for id in line
                .sections
                .iter()
                .map(|s| &s.id)
                .filter(|id| !shapes.iter().any(|s| &s.section == *id))
            {
                parameters
                    .notes
                    .push(format!("section {id}: not solved in this case, left out"));
            }
            Ok(Answer {
                parameters,
                case: case.name.clone(),
            })
        });
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Out {
        let mut out = Out::default();
        self.job.poll();
        if !self.open {
            return out;
        }
        let mut open = true;
        let mut run = false;
        let stale = self.out_of_date(model);
        egui::Window::new("Line parameters")
            .open(&mut open)
            .default_width(900.0)
            .default_height(600.0)
            .show(ctx, |ui| {
                help::body(ui, "line-parameters-window", |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Series impedance with earth return (the earth wires reduced out) \
                             and shunt capacitance of every circuit, from the wires where they \
                             hang in a weather case: positive and zero sequence per km and in \
                             total, the ideally transposed line's; how much the sequences \
                             couple as built. Built, in active development, not approved yet.",
                        )
                        .small()
                        .weak(),
                    );
                    if let Some(s) = settings_ui(ui, model) {
                        out.settings = Some(s);
                    }
                    ui.separator();
                    self.results_ui(ui, &stale, &mut run);
                });
                help::footer(ui, "line-parameters-window", |ui| {
                    let names: Vec<String> =
                        model.cases.iter().map(|c| c.case.name.clone()).collect();
                    ui.label("Weather case");
                    crate::job::case_combo(ui, "line-parameters-case", &names, &mut self.case);
                    let why = names.is_empty().then_some("no weather case in the model");
                    run |= help::run_button(
                        ui,
                        !self.job.running(),
                        "Compute the line parameters from the wires as solved in this weather \
                         case (the conductor temperature for the resistances is the case's)",
                        why,
                    );
                    self.job.status_ui(ui);
                });
            });
        self.open = open;
        if run {
            self.start(model, ctx);
        }
        out
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, stale: &Option<String>, run: &mut bool) {
        let answer = match &self.job.result {
            None => {
                ui.label("Run to compute the line parameters.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(a)) => a,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        let p = &answer.parameters;
        ui.label(format!(
            "In \"{}\"; earth return: {}",
            answer.case,
            p.method.describe()
        ));
        for n in &p.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        let rows = report::rows(p, &[]);
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} circuit(s), {} span(s)",
                p.circuits.len(),
                p.spans.len()
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("line-parameters.csv", &report::csv(p, &[]));
            }
        });
        egui::Grid::new("line-parameters-table")
            .striped(true)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                for h in [
                    "",
                    "circuit / stretch",
                    "km",
                    "R1 Ω/km",
                    "X1 Ω/km",
                    "B1 µS/km",
                    "R0 Ω/km",
                    "X0 Ω/km",
                    "B0 µS/km",
                    "Z1 total Ω",
                    "Z0 total Ω",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                let mut circuit_open = false;
                for r in &rows {
                    match r.kind {
                        "circuit" => {
                            circuit_open = self.expanded.contains(&r.circuit);
                            let label = if circuit_open { "-" } else { "+" };
                            if ui
                                .small_button(label)
                                .on_hover_text("Show or hide its stretches")
                                .clicked()
                            {
                                if circuit_open {
                                    self.expanded.remove(&r.circuit);
                                } else {
                                    self.expanded.insert(r.circuit.clone());
                                }
                            }
                            let tip = match r.unbalance {
                                Some(u) if u > 0.01 => format!(
                                    "As built the sequences couple by {:.1} % of Z1: the line is \
                                     not transposed; the values are those of the ideally \
                                     transposed line",
                                    100.0 * u
                                ),
                                _ => "Ideally transposed values".into(),
                            };
                            ui.strong(&r.circuit).on_hover_text(tip);
                            row_cells(ui, r);
                        }
                        "stretch" if circuit_open => {
                            ui.label("");
                            ui.label(format!("  {}–{}: {}", r.from, r.to, r.sections.join(", ")));
                            row_cells(ui, r);
                        }
                        "coupling" => {
                            ui.label("");
                            ui.label(format!("{} (zero sequence mutual)", r.circuit));
                            ui.label(format!("{:.3}", r.length_km));
                            for _ in 0..3 {
                                ui.label("");
                            }
                            let z = r.zero.unwrap_or_default();
                            ui.label(format!("{:.4}", z[0]));
                            ui.label(format!("{:.4}", z[1]));
                            ui.label("");
                            ui.label("");
                            ui.label(format!(
                                "{:.3} + j{:.3}",
                                z[0] * r.length_km,
                                z[1] * r.length_km
                            ));
                        }
                        _ => continue,
                    }
                    ui.end_row();
                }
            });
    }
}

/// The values of a circuit's or stretch's row (after its first two cells).
fn row_cells(ui: &mut egui::Ui, r: &Row) {
    ui.label(format!("{:.3}", r.length_km));
    let num = |x: Option<f64>, d: usize| x.map_or("—".into(), |v| format!("{v:.d$}"));
    ui.label(num(r.positive.map(|x| x[0]), 4));
    ui.label(num(r.positive.map(|x| x[1]), 4));
    ui.label(num(r.positive.map(|x| x[2]), 3));
    ui.label(num(r.zero.map(|x| x[0]), 4));
    ui.label(num(r.zero.map(|x| x[1]), 4));
    ui.label(num(r.zero.map(|x| x[2]), 3));
    match r.totals() {
        Some((z, _)) => {
            ui.label(format!("{:.3} + j{:.3}", z[0], z[1]));
            ui.label(format!("{:.3} + j{:.3}", z[2], z[3]));
        }
        None => {
            ui.label("—");
            ui.label("—");
        }
    }
}

/// The line's electrical settings; the new ones when changed.
fn settings_ui(ui: &mut egui::Ui, model: &Model) -> Option<ElectricalSettings> {
    let current = model.line.electrical;
    let mut s = current;
    let editable = model.can_edit().is_ok();
    ui.add_enabled_ui(editable, |ui| {
        egui::Grid::new("line-parameters-settings")
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Soil resistivity");
                ui.horizontal(|ui| {
                    let mut given = s.soil_resistivity_ohm_m.is_some();
                    if ui
                        .checkbox(&mut given, "given")
                        .on_hover_text(
                            "Not given: 100 Ω·m assumed — the value the IEEE distribution test \
                             feeders assume; soils range from a few Ω·m to thousands",
                        )
                        .changed()
                    {
                        s.soil_resistivity_ohm_m = given.then_some(DEFAULT_SOIL_RESISTIVITY_OHM_M);
                    }
                    match &mut s.soil_resistivity_ohm_m {
                        Some(r) => {
                            let mut v = *r;
                            let resp = crate::style::field(
                                ui,
                                egui::DragValue::new(&mut v)
                                    .range(0.1..=100_000.0)
                                    .speed(1.0)
                                    .suffix(" Ω·m"),
                            );
                            if resp.drag_stopped() || resp.lost_focus() {
                                *r = v;
                            }
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("100 Ω·m assumed (not given)")
                                    .color(ui.visuals().warn_fg_color),
                            );
                        }
                    }
                });
                ui.end_row();
                ui.label("Earth return");
                egui::ComboBox::from_id_salt("line-parameters-earth")
                    .selected_text(s.earth_return.describe())
                    .show_ui(ui, |ui| {
                        for m in EarthReturn::ALL {
                            ui.selectable_value(&mut s.earth_return, m, m.describe());
                        }
                    });
                ui.end_row();
                ui.label("Height in a span");
                egui::ComboBox::from_id_salt("line-parameters-height")
                    .selected_text(s.mean_height.describe())
                    .show_ui(ui, |ui| {
                        for m in MeanHeight::ALL {
                            ui.selectable_value(&mut s.mean_height, m, m.describe());
                        }
                    });
                ui.end_row();
            });
    });
    if !editable {
        ui.label(
            egui::RichText::new("read-only: the settings are the line's")
                .small()
                .weak(),
        );
    }
    (s != current).then_some(s)
}
