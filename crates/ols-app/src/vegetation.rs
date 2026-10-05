// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Vegetation: points of the vegetation classes (by role) near
//! the wires of one weather case — encroaching now or after the growth
//! until the next cut, and trees that could fall onto a wire (heights
//! above the surface of the checks) — per span
//! ([`ols_clearance::vegetation`]). The points are read again from the
//! point cloud in a corridor around the spans, on a background thread.

use eframe::egui;
use ols_clearance::vegetation::{VegetationCheck, VegetationParams, WireVegetation};
use ols_clearance::{Wire, wires_of};

use crate::job::{Job, case_combo, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What a check found.
#[derive(Debug, Clone)]
pub struct Report {
    /// Weather case.
    pub case: String,
    /// The wires and what was found near each.
    pub wires: Vec<(Wire, WireVegetation)>,
    /// The parameters used.
    pub params: VegetationParams,
    /// Vegetation points read.
    pub points: u64,
    /// Notes.
    pub notes: Vec<String>,
}

/// The Vegetation window.
pub struct VegetationWindow {
    /// Shown.
    pub open: bool,
    /// Weather case (index in the model's cases).
    pub case: usize,
    /// What to look for.
    pub params: VegetationParams,
    job: Job<Report>,
}

impl Default for VegetationWindow {
    fn default() -> Self {
        Self {
            open: false,
            case: 0,
            params: VegetationParams {
                fall_in: true,
                ..VegetationParams::default()
            },
            job: Job::default(),
        }
    }
}

impl VegetationWindow {
    /// Opens the window on the case shown.
    pub fn show(&mut self, case: usize) {
        self.open = true;
        self.case = case;
    }

    /// The last run's answer, and whether one runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(spec) = model.cloud.clone() else {
            self.job.result = Some(Err("no point cloud in this model".into()));
            return;
        };
        let Some(solved) = model.cases.get(self.case) else {
            return;
        };
        let classes = model.scheme.codes_with_role("vegetation");
        if classes.is_empty() {
            self.job.result = Some(Err(format!(
                "no class of \"{}\" has the role vegetation",
                model.scheme.name
            )));
            return;
        }
        let line = model.line.clone();
        let shapes = solved.shapes.clone();
        let case = solved.case.name.clone();
        let ground = model.ground_for_checks();
        let params = self.params.clone();
        self.job.start(ctx, move || {
            let wires = wires_of(&line, &shapes, &[]);
            let mut check = VegetationCheck::new(wires.clone(), &classes, params.clone());
            let (file, name) = ols_lidar::cloud_spec(&spec);
            let source =
                ols_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
            let reach = params.clearance
                + params.growth()
                + if params.fall_in {
                    params.max_tree_height
                } else {
                    ols_clearance::vegetation::SEARCH_MARGIN
                };
            let corridor = ols_lidar::Corridor::along_spans(&line, reach);
            // Exit clause: ~1 s per 10⁷ points decoded; 10 min is far beyond.
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
            ols_pointcloud::PointSource::visit(
                &source,
                Some(&corridor),
                Some(&classes),
                &budget,
                &mut |x, y, z, c| check.visit(ground.as_deref(), x, y, z, c),
            )?;
            let mut notes = Vec::new();
            if params.fall_in && ground.is_none() {
                notes.push("no ground surface: no fall-in check".into());
            } else if check.without_ground > 0 {
                notes.push(format!(
                    "{} points with no ground below them (no fall-in check)",
                    check.without_ground
                ));
            }
            let points = check.points;
            let results = check.finish();
            Ok(Report {
                case,
                wires: wires.into_iter().zip(results).collect(),
                params,
                points,
                notes,
            })
        });
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut open = self.open;
        egui::Window::new("Vegetation")
            .open(&mut open)
            .default_width(760.0)
            .show(ctx, |ui| {
                let p = &mut self.params;
                ui.horizontal(|ui| {
                    ui.label("clearance m");
                    ui.add(
                        egui::DragValue::new(&mut p.clearance)
                            .speed(0.1)
                            .range(0.0..=100.0),
                    );
                    ui.label("growth m/year");
                    ui.add(
                        egui::DragValue::new(&mut p.growth_per_year)
                            .speed(0.05)
                            .range(0.0..=5.0),
                    );
                    ui.label("years");
                    ui.add(
                        egui::DragValue::new(&mut p.years)
                            .speed(0.1)
                            .range(0.0..=30.0),
                    );
                    ui.checkbox(&mut p.fall_in, "trees falling in");
                });
                ui.horizontal(|ui| {
                    let names: Vec<String> =
                        model.cases.iter().map(|c| c.case.name.clone()).collect();
                    case_combo(ui, "vegetation-case", &names, &mut self.case);
                    if ui
                        .add_enabled(!self.job.running(), egui::Button::new("Check"))
                        .on_hover_text("Read the vegetation points again and check every span")
                        .clicked()
                    {
                        self.start(model, ui.ctx());
                    }
                    self.job.status_ui(ui);
                });
                let report = match &self.job.result {
                    None => {
                        ui.label(
                            "Vegetation points near the wires of the case (a hot case for sag, \
                             a windy one for blowout), now and after growth, and trees that \
                             could fall onto them.",
                        );
                        return;
                    }
                    Some(Err(e)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                        return;
                    }
                    Some(Ok(r)) => r,
                };
                for n in &report.notes {
                    ui.label(egui::RichText::new(n).small().weak());
                }
                ui.horizontal(|ui| {
                    let (enc, fall) = report
                        .wires
                        .iter()
                        .fold((0, 0), |(e, f), (_, r)| (e + r.encroaching, f + r.fall_in));
                    ui.label(format!(
                        "\"{}\": {} vegetation points; {enc} within {:.2} m, {fall} of trees \
                         that could fall in",
                        report.case,
                        report.points,
                        report.params.clearance + report.params.growth()
                    ));
                    if ui.button("Export CSV…").clicked() {
                        let (w, r): (Vec<Wire>, Vec<WireVegetation>) =
                            report.wires.iter().cloned().unzip();
                        save_csv(
                            "vegetation.csv",
                            &ols_clearance::vegetation::to_csv(
                                &report.case,
                                &w,
                                &r,
                                &report.params,
                            ),
                        );
                    }
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("vegetation-rows")
                            .striped(true)
                            .spacing([14.0, 3.0])
                            .show(ui, |ui| {
                                for h in [
                                    "span",
                                    "encroaching",
                                    "fall-in",
                                    "nearest m",
                                    "fall margin m",
                                ] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for (w, r) in report
                                    .wires
                                    .iter()
                                    .filter(|(_, r)| r.nearest.is_some() || r.fall_in > 0)
                                {
                                    if ui
                                        .link(format!("{} span {}", w.section, w.span))
                                        .on_hover_text("Go to this span")
                                        .clicked()
                                    {
                                        go = Some(Selection::Span {
                                            section: w.section.clone(),
                                            span: w.span,
                                        });
                                    }
                                    let red = |n: u64| {
                                        let t = egui::RichText::new(n.to_string()).monospace();
                                        if n > 0 {
                                            t.color(egui::Color32::LIGHT_RED)
                                        } else {
                                            t
                                        }
                                    };
                                    ui.label(red(r.encroaching));
                                    ui.label(red(r.fall_in));
                                    ui.label(or_dash(r.nearest.map(|v| v.distance), 2));
                                    ui.label(or_dash(r.worst_fall.and_then(|v| v.fall_margin), 2));
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.open = open;
        go
    }
}
